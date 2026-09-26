//! The exposed Commander can raise the same physical P08 carried on foot.
//! This owns only the mounted input latch/pose; ammunition and swept bullets use existing paths.
use super::*;

#[derive(Default)]
pub(super) struct CupolaPistol {
    raised: bool,
    previous_primary: bool,
}
impl CupolaPistol {
    fn sample(&mut self, eligible: bool, aim: bool, primary: bool) -> bool {
        let was_raised=self.raised;
        self.raised=eligible && aim;
        let fire=self.raised && was_raised && primary && !self.previous_primary;
        self.previous_primary=primary;
        fire
    }
}

impl Game {
    pub fn exposed_commander_sidearm_post(&self) -> bool {
        self.mode==Mode::Tank && self.station==Station::Commander && self.commander_outside
    }

    pub(super) fn update_cupola_pistol(&mut self,input:&TankInput,dt:f32,r:&mut Renderer) {
        if self.mode!=Mode::Tank {
            self.cupola_pistol.sample(false,false,input.fire2||input.mg_coax);
            return;
        }
        let dt=if dt.is_finite(){dt.clamp(0.0,0.05)}else{0.0};
        self.foot.cooldown=(self.foot.cooldown-dt).max(0.0);
        self.foot.flash_t=(self.foot.flash_t-dt).max(0.0);
        self.foot.recoil=(self.foot.recoil-dt*5.5).max(0.0);
        let eligible=self.exposed_commander_sidearm_post() && self.player.alive
            && !self.climb.active && self.switch_t<=0.0 && !input.binoculars
            && self.binocular_raise<0.05 && !self.map_held && !self.compass_equipped
            && self.held_item.is_none() && self.carry_mesh.is_none() && self.carried_casing.is_none()
            && !self.ext_equipped && !self.carrying_track_kit && !self.panzerfaust_claimed
            && self.activity.is_none();
        let trigger=self.cupola_pistol.sample(eligible,input.aim,input.fire2||input.mg_coax);
        self.update_p08_reload(dt,self.cupola_pistol.raised && input.reload);
        self.foot.aiming=self.cupola_pistol.raised;
        if !self.cupola_pistol.raised {return;}
        self.hover=None;
        self.hover_station=None;
        self.hover_hatch=false;
        self.hover_label=Some("P08 — LMB: FIRE · R: RELOAD · RELEASE RMB: HOLSTER");
        r.set_hovered_meshes(&[]);
        r.camera.fov_y=45_f32.to_radians();
        r.optic_mask=0.0;
        r.reticle=false;
        r.camera.pitch+=0.055*self.foot.recoil;
        let forward=r.camera.forward();
        let right=r.camera.right();
        let up=right.cross(forward).normalize_or_zero();
        let mut rotation=crate::foot::held_weapon_rotation(forward,up);
        let mut position=p08::aimed_origin(r.camera.pos,forward,rotation);
        position-=forward*0.045*self.foot.recoil;
        rotation=Quat::from_axis_angle(right,-0.22*self.foot.recoil)*rotation;
        if trigger && !self.primary_pickup_press && self.foot.cooldown<=0.0 {
            let muzzle=position+rotation*Vec3::new(0.0,0.052_598_275,0.110);
            self.fire_luger(r.camera.pos,forward,right,up,muzzle);
        }
        self.draw_p08(r,position,rotation);
        self.drive_foot_weapon_fx(dt,r,position,rotation,forward,right,up);
    }
}

#[cfg(test)]
#[path = "cupola_pistol_scene_tests.rs"]
mod scene_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drawing_does_not_fire_and_held_primary_needs_a_fresh_press() {
        let mut pistol=CupolaPistol::default();
        assert!(!pistol.sample(true,false,true));
        assert!(!pistol.sample(true,true,true));
        assert!(!pistol.sample(true,true,true));
        assert!(!pistol.sample(true,true,false));
        assert!(pistol.sample(true,true,true));
        assert!(!pistol.sample(true,true,true));
        assert!(!pistol.sample(false,true,false));
        assert!(!pistol.raised);
        assert!(!pistol.sample(false,true,true));
    }
    #[test]
    fn simultaneous_draw_and_trigger_only_raises_the_pistol() {
        let mut pistol=CupolaPistol::default();
        assert!(!pistol.sample(true,true,true));
        assert!(pistol.raised);
        assert!(!pistol.sample(true,true,false));
        assert!(pistol.sample(true,true,true));
        assert!(!pistol.sample(true,false,true));
        assert!(!pistol.raised);
    }
}
