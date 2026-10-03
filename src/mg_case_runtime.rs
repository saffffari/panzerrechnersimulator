//! Real mounted-MG cartridge ejection. Fixed pool, source-triangle collision and contact audio.
//! Receiver coordinates/throw are explicit authored approximations pending source port tagging.
use super::*;
pub(super) const CAPACITY:usize=48;
#[derive(Clone)]
pub(super) struct Case { pub active:bool, pub body:shell_collision::Body,
    pub rest:shell_collision::DropRest, pub cooldown:f32, pub seed:u32 }
impl Default for Case {fn default()->Self {Self {active:false,
    body:shell_collision::Body {position:Vec3::ZERO,rotation:Quat::IDENTITY,
        velocity:Vec3::ZERO,angular_velocity:Vec3::ZERO,half:Vec3::new(0.006,0.006,0.029),mass:0.012},
    rest:Default::default(),cooldown:0.0,seed:0}}}
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
        *c=Case::default();c.active=true;c.seed=n;
        c.body.position=frame.transform_point3(origin);
        let (_,rotation,_)=frame.to_scale_rotation_translation();c.body.rotation=rotation;
        c.body.velocity=frame.transform_vector3(Vec3::new(-0.55-0.30*scatter,-0.35,0.20));
        c.body.angular_velocity=frame.transform_vector3(Vec3::new(5.0,8.0+scatter*3.0,3.0));
    }
    pub(super) fn drive_mg_cases(&mut self,dt:f32,r:&mut Renderer) {
        let models=self.fire_surface_models();let source=self.cabin_fire.source_contacts(&models);
        let signature=shell_collision::scene_signature(&models);let mut budget=shell_collision::DropBudget::default();
        let mut sounds=Vec::new();
        for (i,c) in self.mg_cases.iter_mut().enumerate() {
            let slot=self.slots.mg_case[i];r.set_visible(slot,c.active);
            if !c.active {continue;}
            c.cooldown=(c.cooldown-dt.max(0.0)).max(0.0);
            let hit=c.body.advance_dropped(dt,&source,signature,&mut c.rest,&mut budget,0.34);
            if cartridge_contact_audible(&hit)&&c.cooldown<=0.0 {
                c.cooldown=0.10;
                sounds.push((hit.impact_speed,hit.tangential_speed,
                    bearing_deg_from(r.camera.forward(),hit.contact_point-r.camera.pos),c.seed^self.drop_physics_frame as u32));
            }
            r.set_emissive(slot,0.0);r.set_model(slot,c.body.model());
        }
        if let Some(audio)=&self.audio {for &(normal,slide,bearing,seed) in sounds.iter().take(4) {
            audio.play_mg_case_contact(normal,slide,bearing,seed);
        }}
    }
}
#[cfg(test)]mod tests {use super::*;
    #[test]fn case_mesh_matches_dedicated_small_body_and_pool_is_bounded(){let m=mesh();
        assert!((m.max.z-m.min.z-0.058).abs()<1e-6);
        assert!((m.max.x-m.min.x-0.012).abs()<1e-6);
        assert!(m.vertices.iter().all(|v|Vec3::from_array(v.pos).abs().cmple(m.max+Vec3::splat(1e-5)).all()));
        assert_eq!(Case::default().body.mass,0.012);assert_eq!(CAPACITY,48);
    }
}
