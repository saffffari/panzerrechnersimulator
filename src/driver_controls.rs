//! Source driver controls and bounded reach timing. Cosmetic hand motion never bypasses the
//! engine's fuel/damage gates; the real button dispatches ignition at hand contact.
use super::*;

pub(super) const WHEEL_ROOT: &str = "Hull_0238";
pub(super) const BUTTON_ROOT: &str = "Ignition Button";
pub(super) const WHEEL_CENTER: Vec3 = Vec3::new(0.596145, 1.379921, 2.184193);
pub(super) const BUTTON: Vec3 = Vec3::new(1.140952, 1.676846, 2.0933);
const CONTACT: f32 = 0.28;
const DURATION: f32 = 0.85;

pub(super) struct Source { pub wheel: CpuMesh, pub button: CpuMesh }
#[derive(Default)]
pub(super) struct Controls { pub steering: f32, press_age: Option<f32> }
impl Controls {
    pub fn reach(&self) -> f32 {
        let Some(age) = self.press_age else { return 0.0; };
        let t = if age < CONTACT { age / CONTACT }
            else if age < CONTACT + 0.10 { 1.0 }
            else { (DURATION-age)/(DURATION-CONTACT-0.10) }.clamp(0.0,1.0);
        t*t*(3.0-2.0*t)
    }
    pub fn request(&mut self) { if self.press_age.is_none() { self.press_age=Some(0.0); } }
    pub fn advance_steering(&mut self, dt:f32, steer:f32) {
        let dt=dt.clamp(0.0,0.05);
        self.steering += (steer.clamp(-1.0,1.0)*0.55-self.steering)*(1.0-(-dt*10.0).exp());
    }
    pub fn advance(&mut self, dt:f32) -> bool {
        let dt=dt.clamp(0.0,0.05);
        let Some(age)=self.press_age else {return false;};
        let next=age+dt;
        self.press_age=if next>=DURATION {None} else {Some(next)};
        age<CONTACT && next>=CONTACT
    }
    pub fn wheel_model(&self,hull:Mat4)->Mat4 {
        hull*Mat4::from_translation(WHEEL_CENTER)*Mat4::from_rotation_z(self.steering)
            *Mat4::from_translation(-WHEEL_CENTER)
    }
    pub fn hand_targets(&self,hull:Mat4)->[Option<Vec3>;2] {
        let model=self.wheel_model(hull);
        // Source +X arm is the seated driver's right hand. Grips follow the source yoke ends.
        let right=model.transform_point3(WHEEL_CENTER+Vec3::new(0.19,0.11,-0.025));
        let left=model.transform_point3(WHEEL_CENTER+Vec3::new(-0.19,0.11,-0.025));
        [Some(right.lerp(hull.transform_point3(BUTTON),self.reach())),Some(left)]
    }
}

impl Game {
    pub(super) fn advance_driver_controls(&mut self,dt:f32,input:&TankInput,primary_was_held:bool)->bool {
        if self.mode==Mode::Tank && self.station==Station::Driver && self.switch_t<=0.0
            && self.player.alive && input.mg_coax && !primary_was_held
            && self.hover==Some(Hotspot::Ignition) {
            self.driver_controls.request();
        }
        self.driver_controls.advance(dt)
    }
    pub(super) fn update_ignition_hover(&mut self,r:&Renderer) {
        if self.mode!=Mode::Tank || self.station!=Station::Driver || self.switch_t>0.0
            || self.driver_visor_lean>0.01 {return;}
        let inv=self.player.hull_model().inverse();
        let eye=inv.transform_point3(r.camera.pos);
        let direction=inv.transform_vector3(r.camera.forward());
        // Small button target on its real panel, not the panel or entire dashboard.
        if ray_aabb_near(eye,direction,BUTTON-Vec3::new(0.027,0.032,0.006),
            BUTTON+Vec3::new(0.027,0.032,0.054),1.0,0.0).is_some() {
            self.hover=Some(Hotspot::Ignition);
            self.hover_label=Some(if self.engine_on {"STOP ENGINE [LMB]"} else {"START ENGINE [LMB]"});
        }
    }
    pub(super) fn drive_driver_controls(&self,r:&mut Renderer) {
        let hull=self.player.hull_model();
        r.set_model(self.slots.driver_controls[0],self.driver_controls.wheel_model(hull));
        r.set_model(self.slots.driver_controls[1],hull*Mat4::from_translation(Vec3::Z*0.006*self.driver_controls.reach()));
        for slot in self.slots.driver_controls {r.set_visible(slot,true);}
    }
    pub fn driver_controls_receipt(&self)->serde_json::Value {
        serde_json::json!({"steering_rad":self.driver_controls.steering,
            "ignition_reach":self.driver_controls.reach(),"engine_on":self.engine_on,
            "engine_crank_s":self.engine_crank_t,"fuel_liters":self.tank_fuel_liters(),
            "hover":format!("{:?}",self.hover)})
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn button_fires_once_at_contact_and_returns_hand() {
        let mut c=Controls::default();c.request();let mut fires=0;
        for i in 0..120 {c.advance_steering(1.0/120.0,1.0);let fired=c.advance(1.0/120.0);fires+=fired as usize;
            if fired {assert!(i>=32 && i<=35);assert!(c.reach()>0.99);}
        }
        assert_eq!(fires,1);assert_eq!(c.reach(),0.0);assert!(c.steering>0.54);
    }
    #[test] fn steering_keeps_both_hands_attached_to_yoke() {
        for angle in [-0.55,0.0,0.55] {
            let c=Controls{steering:angle,press_age:None};
            let hands=c.hand_targets(Mat4::IDENTITY);
            assert!((hands[0].unwrap().distance(hands[1].unwrap())-0.38).abs()<1e-5);
        }
    }
}
