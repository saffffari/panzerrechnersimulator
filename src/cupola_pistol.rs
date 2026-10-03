//! A crewman can draw the same physical P08 carried on foot, including inside the turret.
//! This owns only the mounted input latch/pose; ammunition and swept bullets use existing paths.
use super::*;

#[derive(Default)]
pub(super) struct CupolaPistol {
    equipped: bool,
    drawn: bool,
    raised: bool,
    previous_primary: bool,
    wait_for_release: bool,
    implicit_aim_holstered: bool,
}
impl CupolaPistol {
    fn sample(&mut self, eligible: bool, aim: bool, primary: bool) -> bool {
        if !aim {self.implicit_aim_holstered=false;}
        let was_drawn=self.drawn;
        self.drawn=eligible && (self.equipped || aim && !self.implicit_aim_holstered);
        self.raised=self.drawn && aim;
        let fire=self.drawn && was_drawn && primary && !self.previous_primary;
        self.previous_primary=primary;
        fire
    }
    fn holster(&mut self) {
        self.equipped=false;
        self.drawn=false;
        self.raised=false;
        self.wait_for_release|=self.previous_primary;
        self.implicit_aim_holstered=true;
    }
}

fn sidearm_post(mode:Mode,station:Station)->bool {
    mode==Mode::Tank && matches!(station,Station::Commander|Station::Loader)
}

fn mask_sidearm_controls(input:&mut TankInput,local_role:Option<crate::lan::LanRole>) {
    input.fire=false;input.fire2=false;input.mg_coax=false;input.coax=false;
    input.aim=false;input.reload=false;input.extinguish=false;input.repair=false;
    // Only the local person's hands are occupied. Other crew members keep their controls.
    if let (Some(frame),Some(role))=(&mut input.coop,local_role) {
        frame.inputs[role as usize].flags &= !(crate::lan::F_FIRE_MAIN|crate::lan::F_FIRE_COAX
            |crate::lan::F_FIRE_HULL|crate::lan::F_SERVICE_BREECH|crate::lan::F_EXTINGUISH
            |crate::lan::F_REPAIR);
    }
}

impl Game {
    pub fn exposed_commander_sidearm_post(&self) -> bool {
        self.mode==Mode::Tank && self.station==Station::Commander && self.commander_outside
    }

    fn mounted_sidearm_hands_available(&self)->bool {
        sidearm_post(self.mode,self.station) && self.player.alive
            && !self.climb.active && self.switch_t<=0.0
            && !self.map_held && !self.compass_equipped
            && self.held_item.is_none() && self.carry_mesh.is_none() && self.carried_casing.is_none()
            && !self.ext_equipped && !self.carrying_track_kit && !self.panzerfaust_claimed
            && self.activity.is_none()
    }

    pub fn mounted_sidearm_owns_input(&self,aim:bool)->bool {
        self.mounted_sidearm_hands_available()
            && (self.cupola_pistol.equipped || self.exposed_commander_sidearm_post() && aim
                && !self.cupola_pistol.implicit_aim_holstered)
    }

    /// Explicit draw leaves unarmed RMB available for physical optics. Full hands never drop
    /// their current object implicitly, and a draw/holster press cannot discharge a weapon.
    pub fn toggle_mounted_sidearm(&mut self)->bool {
        if self.cupola_pistol.equipped || self.cupola_pistol.drawn {
            self.cupola_pistol.holster();return true;
        }
        if !self.mounted_sidearm_hands_available() {return false;}
        self.cupola_pistol.equipped=true;
        self.primary_pickup_press=true;
        true
    }

    pub fn holster_mounted_sidearm(&mut self)->bool {
        if !self.cupola_pistol.equipped && !self.cupola_pistol.drawn {return false;}
        self.cupola_pistol.holster();true
    }

    /// Keep the pistol's original buttons for the post-camera update, and remove them from
    /// station/optic controls. A held trigger stays consumed through holstering until released.
    pub(super) fn route_mounted_sidearm_input(&mut self,input:&mut TankInput)->TankInput {
        let pistol=*input;
        if !input.mg_coax && !input.fire2 {self.cupola_pistol.wait_for_release=false;}
        if self.mounted_sidearm_owns_input(input.aim) || self.cupola_pistol.wait_for_release {
            let local_role=self.lan_local_role.or(self.couch_crew_active.then_some(crate::lan::LanRole::Commander));
            mask_sidearm_controls(input,local_role);
        }
        pistol
    }

    /// The camera-side segment prevents a clipped weapon from launching beyond closed steel.
    /// A real opening passes; subsequent bullet flight still queries every source triangle.
    pub(super) fn p08_muzzle_path_clear(&self,eye:Vec3,muzzle:Vec3)->bool {
        self.cabin_fire.sweep(eye,muzzle,&self.fire_surface_models()).is_none()
    }

    pub(super) fn update_cupola_pistol(&mut self,input:&TankInput,dt:f32,r:&mut Renderer) {
        if self.mode!=Mode::Tank {
            self.cupola_pistol.holster();
            self.cupola_pistol.sample(false,false,input.fire2||input.mg_coax);
            return;
        }
        let dt=if dt.is_finite(){dt.clamp(0.0,0.05)}else{0.0};
        self.foot.cooldown=(self.foot.cooldown-dt).max(0.0);
        self.foot.flash_t=(self.foot.flash_t-dt).max(0.0);
        self.foot.recoil=(self.foot.recoil-dt*5.5).max(0.0);
        if !sidearm_post(self.mode,self.station) {self.cupola_pistol.holster();}
        let eligible=self.mounted_sidearm_hands_available()
            && (self.cupola_pistol.equipped || self.exposed_commander_sidearm_post())
            && !input.binoculars && self.binocular_raise<0.05;
        let trigger=self.cupola_pistol.sample(eligible,input.aim,input.fire2||input.mg_coax);
        self.update_p08_reload(dt,self.cupola_pistol.drawn && input.reload);
        self.foot.aiming=self.cupola_pistol.raised;
        if !self.cupola_pistol.drawn {return;}
        self.hover=None;
        self.hover_station=None;
        self.hover_hatch=false;
        self.hover_label=Some(if self.cupola_pistol.equipped {
            "P08 — LMB: FIRE · RMB: AIM · R: RELOAD · V/Q: HOLSTER"
        }else{"P08 — LMB: FIRE · R: RELOAD · RELEASE RMB: HOLSTER"});
        r.set_hovered_meshes(&[]);
        if self.cupola_pistol.raised {r.camera.fov_y=45_f32.to_radians();}
        r.optic_mask=0.0;
        r.reticle=false;
        r.camera.pitch+=0.055*self.foot.recoil;
        let forward=r.camera.forward();
        let right=r.camera.right();
        let up=right.cross(forward).normalize_or_zero();
        let mut rotation=crate::foot::held_weapon_rotation(forward,up);
        let mut position=if self.cupola_pistol.raised {p08::aimed_origin(r.camera.pos,forward,rotation)}
            else {r.camera.pos+forward*0.35+right*0.16-up*0.20};
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
    fn p08_top_up_accepts_every_partial_load_and_conserves_rounds_at_all_update_rates() {
        use crate::foot::{FootSoldier,MAG_CAP,RELOAD_S};
        for initial in 0..=MAG_CAP+1 {
            for reserve in [0,1,3,8,17] {
                for dt in [1.0/144.0,1.0/60.0,1.0/30.0,0.25,RELOAD_S*2.0] {
                    let mut foot=FootSoldier::default();
                    foot.mag=initial;foot.ammo=reserve;foot.p08_chambered=initial>0;
                    let capacity=MAG_CAP+u32::from(initial>0);
                    let total=foot.total_p08_rounds();
                    let accepted=reserve>0 && initial<capacity;
                    assert_eq!(foot.begin_p08_reload(),accepted,
                        "mag={initial}, reserve={reserve}, dt={dt}");
                    let mut events=[0u32;4];let mut ticks=0;
                    while foot.reload_t>0.0 {
                        assert!(!foot.begin_p08_reload(),"held R cannot restart a change");
                        assert!(!foot.discharge_p08(),"cannot discharge while the magazine is moving");
                        let event=foot.advance_p08_reload(dt);
                        for (i,count) in events.iter_mut().enumerate(){*count+=u32::from(event&(1<<i)!=0);}
                        assert_eq!(foot.total_p08_rounds(),total,"every stage conserves rounds");
                        if initial>0 {assert!(foot.p08_chambered,"tactical reload retains chamber");}
                        ticks+=1;assert!(ticks<300);
                    }
                    assert_eq!(foot.mag,initial+capacity.saturating_sub(initial).min(reserve));
                    assert_eq!(foot.ammo,total-foot.mag);
                    let expected=if !accepted {[0;4]}else if initial==0 {[1;4]}else{[1,1,0,0]};
                    assert_eq!(events,expected,"each actual mechanism stage sounds once");
                    let loaded=foot.mag;
                    assert_eq!(foot.advance_p08_reload(RELOAD_S*2.0),0,
                        "advancing a completed change cannot repeat a stage");
                    assert_eq!(foot.mag,loaded,"completed change cannot transfer twice");
                    assert_eq!(foot.total_p08_rounds(),total);
                    // An empty reload seats eight rounds, then chambers one of those eight.
                    // With reserve left, a separate tactical reload can now fill the magazine's
                    // vacant position: eight in the magazine plus the retained chambered round.
                    let second_reload=initial==0 && reserve>MAG_CAP;
                    assert_eq!(foot.begin_p08_reload(),second_reload,
                        "a new request may fill the one position vacated by chambering");
                    if second_reload {
                        let mut second_events=[0u32;4];let mut second_ticks=0;
                        while foot.reload_t>0.0 {
                            assert!(!foot.begin_p08_reload());
                            let event=foot.advance_p08_reload(dt);
                            for (i,count) in second_events.iter_mut().enumerate(){*count+=u32::from(event&(1<<i)!=0);}
                            assert!(foot.p08_chambered);
                            assert_eq!(foot.total_p08_rounds(),total);
                            second_ticks+=1;assert!(second_ticks<300);
                        }
                        assert_eq!(second_events,[1,1,0,0],"top-up retains the chambered round");
                        assert_eq!(foot.mag,MAG_CAP+1);
                        assert_eq!(foot.ammo,total-foot.mag);
                        assert!(!foot.begin_p08_reload(),"full magazine and chamber reject another reload");
                    }
                }
            }
        }
    }

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

    #[test]
    fn explicit_draw_has_low_ready_and_requires_fresh_trigger_after_holster() {
        let mut pistol=CupolaPistol {equipped:true,..Default::default()};
        assert!(!pistol.sample(true,false,true),"drawing cannot discharge");
        assert!(pistol.drawn && !pistol.raised,"V draws without taking RMB aim");
        assert!(!pistol.sample(true,false,false));
        assert!(pistol.sample(true,false,true));
        pistol.holster();
        assert!(pistol.wait_for_release,"held LMB must not transfer to a tank weapon");
        assert!(!pistol.drawn && !pistol.equipped);
        assert!(!pistol.sample(false,false,true));
        pistol.equipped=true;
        assert!(!pistol.sample(true,true,true),"redrawing with a held trigger is safe");
        assert!(!pistol.sample(true,true,false));
        assert!(pistol.sample(true,true,true));
    }

    #[test]
    fn interior_pistol_owns_only_local_hands_and_leaves_other_crew_live() {
        use crate::lan::{CrewFrame,LanRole,F_FIRE_MAIN,F_FIRE_HULL,F_SERVICE_BREECH};
        assert!(sidearm_post(Mode::Tank,Station::Commander));
        assert!(sidearm_post(Mode::Tank,Station::Loader));
        assert!(!sidearm_post(Mode::Tank,Station::Gunner));
        assert!(!sidearm_post(Mode::Foot,Station::Loader));
        let mut frame=CrewFrame::default();
        frame.inputs[LanRole::Loader as usize].flags=F_SERVICE_BREECH|F_FIRE_HULL;
        frame.inputs[LanRole::Gunner as usize].flags=F_FIRE_MAIN;
        let mut input=TankInput {drive:1.0,look_dx:13.0,jump:true,hatch_toggle:true,
            fire:true,fire2:true,mg_coax:true,coax:true,aim:true,reload:true,
            extinguish:true,repair:true,coop:Some(frame),..Default::default()};
        mask_sidearm_controls(&mut input,Some(LanRole::Loader));
        assert!(!input.fire && !input.fire2 && !input.mg_coax && !input.coax
            && !input.aim && !input.reload && !input.extinguish && !input.repair);
        assert_eq!(input.drive,1.0);assert_eq!(input.look_dx,13.0);
        assert!(input.jump && input.hatch_toggle,"movement and actual hatch controls remain");
        assert_eq!(input.coop.unwrap().inputs[LanRole::Loader as usize].flags,0);
        assert_eq!(input.coop.unwrap().inputs[LanRole::Gunner as usize].flags,F_FIRE_MAIN);
    }

    #[test]
    fn explicit_holster_waits_for_rmb_release_before_implicit_cupola_redraw() {
        let mut pistol=CupolaPistol::default();
        assert!(!pistol.sample(true,true,false));
        assert!(pistol.drawn);
        pistol.holster();
        assert!(!pistol.sample(true,true,false));
        assert!(!pistol.drawn,"Q must remain holstered while the old RMB press is held");
        assert!(!pistol.sample(true,false,false));
        assert!(!pistol.sample(true,true,false));
        assert!(pistol.drawn,"a new RMB press can still use the existing head-out shortcut");
    }

    #[test]
    fn p08_escape_aperture_blocks_when_shut_and_permits_real_open_path() -> anyhow::Result<()> {
        let (parts,lids,_,_,_,_,_,_)=tiger_hatches::load_player_tiger()?;
        let source=surface_fire::CabinFire::new(&[(0,&parts[0]),(1,&parts[1]),(15,&lids[3])]);
        for yaw in [0.0,1.17,2.8] {
            let hull=Mat4::from_rotation_translation(Quat::from_rotation_y(yaw)
                *Quat::from_rotation_x(0.08),Vec3::new(985.0,5.5,1205.0));
            let turret=hull*Mat4::from_translation(tank::TIGER_PARTS.turret_pivot)
                *Mat4::from_rotation_y(0.61);
            let center=turret.transform_point3(tiger_hatches::APERTURE_CENTERS[3]);
            let normal=turret.transform_vector3(tiger_hatches::APERTURE_NORMALS[3]).normalize();
            let eye=center-normal*0.50;
            let inside_muzzle=center-normal*0.12;
            let poked_muzzle=center+normal*0.12;
            for open in [false,true] {
                let mut hatches=tiger_hatches::RoofHatches::default();
                hatches.phase[3]=if open {1.0}else{0.0};
                let mut models=[hull;surface_fire::FRAMES];
                models[1]=turret;models[15]=hatches.models(hull,turret)[3];
                assert!(source.sweep(eye,inside_muzzle,&models).is_none());
                assert_eq!(source.sweep(inside_muzzle,center+normal*1.5,&models).is_none(),open,
                    "the actual shut lid must stop flight; only its real open aperture passes");
                assert_eq!(source.sweep(eye,poked_muzzle,&models).is_none(),open,
                    "a muzzle clipped past a shut lid cannot launch outside armor");
            }
        }
        Ok(())
    }
}
