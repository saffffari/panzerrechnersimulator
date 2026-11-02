//! Released whole-tree terrain dynamics. Every source vertex remains, including buried roots.
//! This reuses loose-prop surface contacts and uniform thin-shell mass moments. Foliage remains
//! rigid/load-bearing; there is no wood fracture, deforming root plate, or soil excavation.
//! Initial source burial is measured and removed by the same source-contact projection used
//! thereafter, rather than granting the buried root a hidden penetration exemption.
use crate::game::scatter::scatter_support::{Motion, Source as RigidSource, Step};
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Quat, Vec3};

pub(super) struct Source { rigid: Option<RigidSource>, pub(super) sample_gap_m:f32 }

/// Separate versioned save packet: the existing tree identity/instance record remains intact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Released {
    pub(super) rest:Mat4,
    pub(super) anchor:Vec3,
    pub(super) motion:Motion,
    pub(super) initial_clearance_m:Option<f32>,
    pub(super) maximum_contact_correction_m:f32,
    next_impact_s:f32,
}
impl Released {
    pub(super) fn new(mut rest:Mat4,_dir:Vec3,elapsed:f32)->Self {
        let anchor=rest.w_axis.truncate();
        // Integrate around a fixed local origin. Repeatedly adding submillimetre motion to
        // kilometre-sized world positions otherwise changes the contact trajectory.
        rest.w_axis=Vec3::ZERO.extend(1.0);
        Self {rest,anchor,motion:Motion {pos:Vec3::ZERO,velocity:Vec3::ZERO,
            rotation:Quat::IDENTITY,angular_velocity:Vec3::ZERO,age:elapsed,bounces:0},
            initial_clearance_m:None,maximum_contact_correction_m:0.0,next_impact_s:elapsed}
    }
    pub(super) fn valid(&self)->bool {
        self.rest.is_finite()&&self.anchor.is_finite()&&self.motion.pos.is_finite()
            &&self.motion.velocity.is_finite()&&self.motion.angular_velocity.is_finite()
            &&self.motion.rotation.is_finite()&&(self.motion.rotation.length_squared()-1.0).abs()<0.002
            &&self.motion.age.is_finite()&&self.motion.age>=0.0
            &&self.initial_clearance_m.is_none_or(f32::is_finite)
            &&self.maximum_contact_correction_m.is_finite()&&self.maximum_contact_correction_m>=0.0
            &&self.next_impact_s.is_finite()&&self.next_impact_s>=0.0
    }
}
impl Source {
    pub(super) fn new(mesh:&CpuMesh)->Self {
        let rigid=RigidSource::from_mesh(mesh);
        let sample_gap_m=rigid.as_ref().map_or(0.0,|s|s.gap_bound_m);
        Self {rigid,sample_gap_m}
    }
    pub(super) fn advance(&self,state:&mut Released,dt:f32,dir:Vec3,work_j_per_kg:f32,
        height:impl Fn(f32,f32)->f32)->Option<Step> {
        let anchor=state.anchor;
        let local_height=|x,z|height(anchor.x+x,anchor.z+z)-anchor.y;
        self.advance_local(state,dt,dir,work_j_per_kg,local_height)
    }
    fn advance_local(&self,state:&mut Released,dt:f32,dir:Vec3,work_j_per_kg:f32,
        local_height:impl Fn(f32,f32)->f32)->Option<Step> {
        let source=self.rigid.as_ref()?;
        let anchor=state.anchor;
        if state.initial_clearance_m.is_none() {
            state.initial_clearance_m=source.minimum_clearance(state.rest,&local_height);
            if dt>0.0 {
                source.release_about_support(state.rest,Vec3::ZERO,&mut state.motion,dir,work_j_per_kg,&local_height);
            }
        }
        let mut step=source.advance_all_impacts(state.rest,Vec3::ZERO,&mut state.motion,dt,0.60,0.18,local_height);
        state.maximum_contact_correction_m=state.maximum_contact_correction_m.max(step.max_correction_m);
        step.model.w_axis=(step.model.w_axis.truncate()+anchor).extend(1.0);
        step.hit=step.hit.map(|(point,impact)|(point+anchor,impact));
        if step.hit.is_some() {
            if state.motion.age>=state.next_impact_s {state.next_impact_s=state.motion.age+0.25;}
            else {step.hit=None;}
        }
        Some(step)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    fn source()->Source {Source::new(&crate::prims::box_mesh(
        Vec3::new(-0.2,0.0,-0.2),Vec3::new(0.2,5.0,0.2),[100;3]))}
    fn run(source:&Source,base:Mat4,grade:impl Fn(f32,f32)->f32)->(Released,Mat4,bool) {
        run_with_work(source,base,crate::trees::release_work_per_mass(0.4,5.0,0.0),grade)
    }
    fn run_with_work(source:&Source,base:Mat4,work:f32,grade:impl Fn(f32,f32)->f32)->(Released,Mat4,bool) {
        let mut state=Released::new(base,Vec3::Z,0.0);let mut model=base;let mut sleeping=false;
        for _ in 0..3600 {
            let step=source.advance(&mut state,1.0/120.0,Vec3::Z,work,&grade).unwrap();model=step.model;
            let clearance=source.rigid.as_ref().unwrap().minimum_clearance(model,&grade).unwrap();
            assert!(clearance>=-0.0015,"source samples cannot enter soil: {clearance}");
            if step.sleeping {sleeping=true;break;}
        }
        (state,model,sleeping)
    }
    #[test] fn released_tree_falls_and_rests_on_source_on_each_grade() {
        let source=source();let mut angles=Vec::new();
        for slope in [-0.20,0.0,0.20] {
            let (state,model,sleeping)=run(&source,Mat4::from_translation(Vec3::Y*-0.025),|_,z|slope*z);
            let angle=model.to_scale_rotation_translation().1.to_scaled_axis().length();angles.push(angle);
            assert!(angle>1.0,"released root cannot jam near upright: {angle}");
            assert!(sleeping,"released source must actually stabilize; v={:?} w={:?}",state.motion.velocity,state.motion.angular_velocity);
            assert!(state.initial_clearance_m.unwrap()<0.0&&state.maximum_contact_correction_m>=0.025,
                "existing burial is measured and projected out, never exempted");
        }
        assert!(angles[0]>angles[2]+0.15,"actual grades must affect final pose: {angles:?}");
    }
    #[test] fn released_motion_roundtrip_continues_exactly_and_does_not_sleep_in_air() {
        let source=source();let mut state=Released::new(Mat4::from_translation(Vec3::Y*8.0),Vec3::Z,0.0);
        let step=source.advance(&mut state,0.05,Vec3::Z,1.0,|_,_|0.0).unwrap();assert!(!step.sleeping);
        assert!(state.motion.pos.y<0.0&&state.valid());
        let bytes=bincode::serialize(&state).unwrap();let mut restored:Released=bincode::deserialize(&bytes).unwrap();
        for _ in 0..30 {
            let a=source.advance(&mut state,1.0/120.0,Vec3::Z,1.0,|_,_|0.0).unwrap();
            let b=source.advance(&mut restored,1.0/120.0,Vec3::Z,1.0,|_,_|0.0).unwrap();
            assert_eq!(a.model,b.model);assert_eq!(state,restored);
        }
    }
    #[test] fn released_sleep_wakes_when_its_real_terrain_support_is_removed() {
        let source=source();
        let (mut state,_,sleeping)=run(&source,Mat4::from_translation(Vec3::Y*-0.025),|_,_|0.0);
        assert!(sleeping);
        let before=state.motion.pos;
        let probe=source.advance(&mut state,0.0,Vec3::ZERO,0.0,|_,_|-1.0).unwrap();
        assert!(!probe.sleeping,"removed terrain cannot keep the tree certified asleep");
        source.advance(&mut state,1.0/60.0,Vec3::ZERO,0.0,|_,_|-1.0);
        assert!(state.motion.pos.y<before.y&&state.motion.velocity.y<0.0);
    }
    #[test] fn identical_local_inputs_have_identical_motion_at_far_world_origin() {
        let source=source();let origin=Vec3::new(5250.0,60.0,-5250.0);
        // Exactly representable initial burial makes the initial local inputs identical.
        let mut near=Released::new(Mat4::from_translation(Vec3::Y*-0.03125),Vec3::Z,0.0);
        let mut far=Released::new(Mat4::from_translation(origin-Vec3::Y*0.03125),Vec3::Z,0.0);
        let work=crate::trees::release_work_per_mass(0.4,5.0,0.0);
        for _ in 0..3600 {
            let a=source.advance_local(&mut near,1.0/120.0,Vec3::Z,work,|_,z|0.2*z+0.03125).unwrap();
            let b=source.advance_local(&mut far,1.0/120.0,Vec3::Z,work,|_,z|0.2*z+0.03125).unwrap();
            assert_eq!(near.motion,far.motion,"world translation cannot enter local integration");
            for &point in source.rigid.as_ref().unwrap().samples() {
                assert!(a.model.transform_point3(point).distance(b.model.transform_point3(point)-origin)<0.002,
                    "only final f32 world presentation can differ");
            }
            if a.sleeping {assert!(b.sleeping);return;}
        }
        panic!("controlled equal-input source falls must stabilize");
    }
    #[test] fn translated_quantized_ground_still_has_clear_stable_source_support() {
        let source=source();let origin=Vec3::new(5250.0,60.0,-5250.0);
        let (_,near,a)=run(&source,Mat4::from_translation(Vec3::Y*-0.025),|_,z|0.2*z);
        let (_,far,b)=run(&source,Mat4::from_translation(origin-Vec3::Y*0.025),|_,z|origin.y+0.2*(z-origin.z));
        assert!(a&&b);
        let local=|z:f32|0.2*z;
        let translated=|z:f32|origin.y+0.2*((origin.z+z)-origin.z)-origin.y;
        let normal=|f:&dyn Fn(f32)->f32,z|Vec3::new(0.0,0.05,f(z-0.025)-f(z+0.025)).normalize();
        let (mut height_error,mut normal_error)=(0.0_f32,0.0_f32);
        for i in 0..=16000 {
            let z=-10.0+i as f32*(20.0/16000.0);
            height_error=height_error.max((local(z)-translated(z)).abs());
            normal_error=normal_error.max(normal(&local,z).distance(normal(&translated,z)));
        }
        let ulp=|x:f32|f32::from_bits(x.abs().to_bits()+1)-x.abs();
        let height_bound=0.5*(0.2*ulp(origin.z)+ulp(origin.y))+4.0*f32::EPSILON*2.0;
        assert!(height_error<=height_bound&&normal_error<=height_bound/0.025);
        // Those callbacks are genuinely different discrete surfaces/normals. Contact/friction
        // can amplify them, so an invented8cm trajectory equivalence is not a physics invariant.
        // run() above still requires every source sample clear every tick and supported rest.
        eprintln!("translated_tree input_height_error_m={height_error} input_normal_error={normal_error} height_error_bound_m={height_bound} terminal_origin_difference_m={}",
            near.w_axis.truncate().distance(far.w_axis.truncate()-origin));
    }
    #[test] fn private_authored_tree_sources_fall_clear_and_stabilize() {
        let path="assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/pine_tree_n_1.glb";
        if !crate::resource::is_file(path) {eprintln!("private tree source unavailable");return;}
        let mesh=crate::worldgen::load_kit(crate::worldgen::Kit::Pine).expect("available private tree loads");
        let source=Source::new(&mesh);
        let diameter=crate::trees::trunk_section_width(&mesh,mesh.min.y+crate::tank_push::BOW_CONTACT_H_M,1.0).unwrap();
        let work=crate::trees::release_work_per_mass(diameter,mesh.max.y-mesh.min.y,0.0);
        for slope in [-0.2,0.0,0.2] {
            let (state,pose,sleeping)=run_with_work(&source,Mat4::from_translation(Vec3::Y*-0.025),work,|_,z|slope*z);
            let angle=pose.to_scale_rotation_translation().1.to_scaled_axis().length();
            assert!(angle>0.8,"actual source must fall materially: slope={slope} angle={angle}");
            assert!(sleeping,"actual source must settle on a stable contact polygon");
            eprintln!("released_tree slope={slope} angle_rad={angle} samples={} sample_gap_m={} initial_clearance_m={:?} max_correction_m={}",
                source.rigid.as_ref().unwrap().sample_count(),source.sample_gap_m,state.initial_clearance_m,state.maximum_contact_correction_m);
        }
    }
}
