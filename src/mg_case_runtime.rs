//! Real mounted-MG cartridge ejection. Fixed pool, source-triangle collision and contact audio.
//! Receiver coordinates/throw are explicit authored approximations pending source port tagging.
use super::*;
pub(super) const CAPACITY:usize=48;
#[derive(Clone)]
pub(super) struct Case { pub active:bool, pub body:shell_collision::Body,
    pub rest:shell_collision::DropRest, pub cooldown:f32, pub seed:u32,
    /// Which frame the case lies in and rides with: the turret (coaxial gun) or the hull (bow gun).
    pub turret:bool,
    /// Where it was, in its frame, when it last made a sound.
    pub sounded_at:Vec3 }
impl Default for Case {fn default()->Self {Self {active:false,
    body:shell_collision::Body {position:Vec3::ZERO,rotation:Quat::IDENTITY,
        velocity:Vec3::ZERO,angular_velocity:Vec3::ZERO,half:Vec3::new(0.006,0.006,0.029),mass:0.012},
    rest:Default::default(),cooldown:0.0,seed:0,turret:false,sounded_at:Vec3::splat(f32::MAX)}}}
pub(super) fn mesh()->CpuMesh {
    let mut mesh=spent_88_case_mesh();
    let extent=mesh.max-mesh.min;
    let scale=Vec3::new(0.012/extent.x.max(0.001),0.012/extent.y.max(0.001),0.058/extent.z.max(0.001));
    let center=(mesh.max+mesh.min)*0.5;
    for v in &mut mesh.vertices {v.pos=((Vec3::from_array(v.pos)-center)*scale).to_array();
        v.normal=(Vec3::from_array(v.normal)/scale).normalize_or_zero().to_array();}
    mesh.min=Vec3::new(-0.006,-0.006,-0.029);mesh.max=-mesh.min;
    mesh
}
impl Game {
    pub(super) fn eject_mg_case(&mut self,coax:bool) {
        let frame=if coax {self.player.visual_gun_model()}else{self.player.hull_model()};
        let origin=if coax {Vec3::new(-0.36,-0.20,-0.20)}else{Vec3::new(-0.72,1.10,2.30)};
        let n=self.mg_round;let scatter=hash01(n^0xCA5E_7920);
        let c=&mut self.mg_cases[self.next_mg_case];self.next_mg_case=(self.next_mg_case+1)%CAPACITY;
        *c=Case::default();c.active=true;c.seed=n;c.turret=coax;
        c.body.position=frame.transform_point3(origin);
        let (_,rotation,_)=frame.to_scale_rotation_translation();c.body.rotation=rotation;
        c.body.velocity=frame.transform_vector3(Vec3::new(-0.55-0.30*scatter,-0.35,0.20));
        c.body.angular_velocity=frame.transform_vector3(Vec3::new(5.0,8.0+scatter*3.0,3.0));
    }
    pub(super) fn drive_mg_cases(&mut self,dt:f32,r:&mut Renderer) {
        let _probe = crate::sim_phase::Probe::new("drive_mg_cases");
        let models=self.fire_surface_models();let source=self.cabin_fire.source_contacts(&models);
        let signature=shell_collision::scene_signature(&models);let mut budget=shell_collision::DropBudget::default();
        let mut sounds=Vec::new();
        // `drive_casings` ran first this update and measured both frames' motion.
        let steps=self.brass_steps;let hull_inverse=self.player.hull_model().inverse();
        let ground_floor=self.player.pos.y-50.0;
        let frame_inverse=[self.player.turret_model().inverse(),hull_inverse];
        for (i,c) in self.mg_cases.iter_mut().enumerate() {
            let slot=self.slots.mg_case[i];r.set_visible(slot,c.active);
            if !c.active {continue;}
            c.cooldown=(c.cooldown-dt.max(0.0)).max(0.0);
            // The ejection velocity is already relative to the gun; the case rides with its frame.
            if brass_aboard(hull_inverse,c.body.position) {
                carry_with_frame(steps[if c.turret {0} else {1}],&mut c.body.position,&mut c.body.rotation,
                    &mut c.body.velocity,&mut c.body.angular_velocity);
            } else if c.body.position.y<ground_floor {c.active=false;r.set_visible(slot,false);continue;}
            let hit=c.body.advance_dropped(dt,&source,signature,&mut c.rest,&mut budget,0.34);
            let place=frame_inverse[if c.turret {0} else {1}].transform_point3(c.body.position);
            if cartridge_contact_audible(&hit)&&c.cooldown<=0.0&&contact_has_travelled(&hit,place,c.sounded_at) {
                c.cooldown=0.15;c.sounded_at=place;
                sounds.push((hit.impact_speed,hit.tangential_speed,
                    bearing_deg_from(r.camera.forward(),hit.contact_point-r.camera.pos),c.seed^self.drop_physics_frame as u32));
            }
            r.set_emissive(slot,0.0);r.set_model(slot,c.body.model());
        }
        strongest_first(&mut sounds);
        // Owner, 2026-10-04: the machine-gun cases' clinking "needs to stop". They still fall and
        // lie where they land; they are silent.
        const MG_CASES_AUDIBLE:bool=false;
        let allowed=if MG_CASES_AUDIBLE {(self.brass_sound_credit.floor().max(0.0) as usize).min(sounds.len()).min(4)} else {0};
        self.brass_sound_credit-=allowed as f32;
        if let Some(audio)=&self.audio {for &(normal,slide,bearing,seed) in sounds.iter().take(allowed) {
            audio.play_mg_case_contact(normal,slide,bearing,seed);
        }}
    }
}
/// Order contacts by the same collision energy `play_mg_case_contact` calibrates from, so the
/// per-update voice limit keeps the strongest contacts and not the lowest pool indices.
fn strongest_first(sounds:&mut [(f32,f32,f32,u32)]) {
    let energy=|s:&(f32,f32,f32,u32)|s.0*s.0+0.018*s.1*s.1;
    sounds.sort_by(|a,b|energy(b).total_cmp(&energy(a)));
}
#[cfg(test)]mod tests {use super::*;
    #[test]fn voice_limit_keeps_the_strongest_case_contacts_not_the_first_pool_slots(){
        let mut sounds:Vec<(f32,f32,f32,u32)>=(0..6).map(|i|(0.05,0.2,0.0,i)).collect();
        sounds.push((1.8,0.1,0.0,6));sounds.push((0.04,4.0,0.0,7));
        strongest_first(&mut sounds);
        let kept:Vec<u32>=sounds.iter().take(4).map(|s|s.3).collect();
        assert_eq!(&kept[..2],&[6,7],"hard bounce, then fast slide, lead: {kept:?}");
        assert_eq!(&kept[2..],&[0,1],"equal contacts keep pool order");
    }
    #[test]fn case_mesh_matches_dedicated_small_body_and_pool_is_bounded(){let m=mesh();
        assert!((m.max.z-m.min.z-0.058).abs()<1e-6);
        assert!((m.max.x-m.min.x-0.012).abs()<1e-6);
        assert!(m.vertices.iter().all(|v|Vec3::from_array(v.pos).abs().cmple(m.max+Vec3::splat(1e-5)).all()));
        assert_eq!(Case::default().body.mass,0.012);assert_eq!(CAPACITY,48);
    }
}
