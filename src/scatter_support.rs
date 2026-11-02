//! Source-sampled terrain contact for loose props (not hinged props or a scene physics engine).
//! Open furniture uses uniform thin-shell triangle-area mass moments, an explicit estimate.
//! No collision box/ball supports the body. Source barycentric samples have a measured maximum
//! triangle-lattice edge; terrain peaks between samples can still be missed. No CCD claim.
use std::collections::HashSet;
use glam::{Mat3, Mat4, Quat, Vec3};
use crate::gltf_load::CpuMesh;

const TARGET_SAMPLE_M:f32=0.16;
const SAMPLE_BUDGET:usize=8192;
const CONTACT_M:f32=0.002;
const STEP_S:f32=1.0/120.0;
const POINT_STEP_M:f32=0.04;
const G:f32=9.81;

pub(crate) struct Source {
    triangles:Vec<[Vec3;3]>,
    points:Vec<Vec3>,
    pub gap_bound_m:f32,
}
struct Mass {
    center_offset:Vec3,
    inverse_inertia:Mat3, // per unit mass, in the rest-pose frame
    radius:f32,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Motion {
    pub pos:Vec3,
    pub velocity:Vec3,
    pub rotation:Quat,
    pub angular_velocity:Vec3,
    pub age:f32,
    pub bounces:u8,
}
pub(crate) struct Step {
    pub model:Mat4,
    pub sleeping:bool,
    pub hit:Option<(Vec3,f32)>,
    /// Largest actual source-ground penetration projected out during this step.
    pub max_correction_m:f32,
}
#[derive(Clone,Copy,Debug)]
pub(crate) struct ReleaseEvidence {
    pub pivot:Vec3,
    pub center:Vec3,
    pub barrier_j_per_kg:f32,
    pub work_j_per_kg:f32,
}
struct Contact {
    correction:f32,
    points:Vec<Vec3>, // actual source offsets from motion.pos
}
fn outer(v:Vec3)->Mat3 {Mat3::from_cols(v*v.x,v*v.y,v*v.z)}
fn edge(t:[Vec3;3])->f32 {t[0].distance(t[1]).max(t[1].distance(t[2])).max(t[2].distance(t[0]))}
fn divisions(t:[Vec3;3],spacing:f32)->usize {(edge(t)/spacing).ceil().clamp(1.0,64.0) as usize}

impl Source {
    pub fn from_mesh(mesh:&CpuMesh)->Option<Self> {
        let mut triangles=Vec::new();
        for ids in mesh.indices.chunks_exact(3) {
            let p=[Vec3::from(mesh.vertices.get(ids[0] as usize)?.pos),
                Vec3::from(mesh.vertices.get(ids[1] as usize)?.pos),
                Vec3::from(mesh.vertices.get(ids[2] as usize)?.pos)];
            if p.iter().any(|p|!p.is_finite()) {return None;}
            if (p[1]-p[0]).cross(p[2]-p[0]).length_squared()>1e-16 {triangles.push(p);}
        }
        if triangles.is_empty() {return None;}
        let mut spacing=TARGET_SAMPLE_M;
        // Retain every source triangle vertex. The budget limits refinement, not source
        // membership; very detailed originals may themselves exceed it. Report actual gap.
        while triangles.iter().map(|&t|{let n=divisions(t,spacing);(n+1)*(n+2)/2}).sum::<usize>()>SAMPLE_BUDGET
            && triangles.iter().any(|&t|divisions(t,spacing)>1) {spacing*=2.0;}
        let mut points=Vec::new();let mut seen=HashSet::new();let mut gap=0.0_f32;
        for &t in &triangles {
            let n=divisions(t,spacing);gap=gap.max(edge(t)/n as f32);
            for i in 0..=n {for j in 0..=n-i {
                let p=t[0]+(t[1]-t[0])*(i as f32/n as f32)+(t[2]-t[0])*(j as f32/n as f32);
                if seen.insert(p.to_array().map(f32::to_bits)) {points.push(p);}
            }}
        }
        Some(Self {triangles,points,gap_bound_m:gap})
    }

    pub fn sample_count(&self)->usize {self.points.len()}

    /// Immutable barycentric source samples; not contact proxies or a source triangle query.
    pub fn samples(&self)->&[Vec3] {&self.points}

    /// Minimum signed clearance among every retained source sample. Negative means actual
    /// sampled penetration, including any buried roots; nothing is exempted or projected here.
    pub fn minimum_clearance(&self,model:Mat4,height:impl Fn(f32,f32)->f32)->Option<f32> {
        if !model.is_finite() {return None;}
        self.contact(model,model.w_axis.truncate(),&height).map(|c|-c.correction)
    }

    /// Initialize a released rooted body about its ACTUAL forward support edge. Motion.velocity
    /// is COM velocity: assigning angular velocity alone makes the root sweep through the soil
    /// and lets correct contact friction absorb the kick. The matching COM velocity leaves the
    /// chosen support edge instantaneously still. Input work is estimated by the tree owner;
    /// the source's COM-over-edge barrier supplies an explicit minimum (2x for dissipative loss).
    pub fn release_about_support(&self,rest:Mat4,anchor:Vec3,motion:&mut Motion,dir:Vec3,
        estimated_work_j_per_kg:f32,height:impl Fn(f32,f32)->f32)->Option<ReleaseEvidence> {
        let direction=Vec3::new(dir.x,0.0,dir.z).normalize_or_zero();
        if direction.length_squared()<0.5{return None;}
        let mass=self.mass(rest,anchor)?;
        let contact=self.contact(rest,anchor,&height)?;
        if contact.correction< -CONTACT_M{return None;}
        let polygon=crate::vehicle_ground::support_polygon(&contact.points);
        let center=mass.center_offset;
        let cross=|a:glam::Vec2,b:glam::Vec2|a.x*b.y-a.y*b.x;
        let origin=glam::Vec2::new(center.x,center.z);
        let d=glam::Vec2::new(direction.x,direction.z);
        let mut exit:Option<(f32,Vec3)>=None;
        for (&a,&b) in polygon.iter().zip(polygon.iter().cycle().skip(1)).take(polygon.len()) {
            let p=glam::Vec2::new(a.x,a.z);let edge=glam::Vec2::new(b.x-a.x,b.z-a.z);
            let det=cross(d,edge);if det.abs()<1e-8 {continue;}
            let t=cross(p-origin,edge)/det;let u=cross(p-origin,d)/det;
            if t>=0.0&&(0.0..=1.0).contains(&u)&&exit.is_none_or(|(old,_)|t<old) {
                exit=Some((t,a+(b-a)*u));
            }
        }
        let pivot=exit.map(|(_,p)|p).or_else(||polygon.iter().copied()
            .max_by(|a,b|a.dot(direction).total_cmp(&b.dot(direction))))?;
        let axis=Vec3::Y.cross(direction);let lever=center-pivot;
        let along=lever.dot(direction);
        let barrier=if along<0.0 {G*(lever.y.hypot(along)-lever.y).max(0.0)}else{0.0};
        let inertia=mass.inverse_inertia.inverse();
        let about_edge=axis.dot(inertia*axis)+lever.length_squared()-lever.dot(axis).powi(2);
        let work=estimated_work_j_per_kg.max(2.0*barrier);
        if !work.is_finite()||work<0.0||!about_edge.is_finite()||about_edge<=0.0{return None;}
        motion.angular_velocity=axis*(2.0*work/about_edge).sqrt();
        motion.velocity=motion.angular_velocity.cross(lever);
        Some(ReleaseEvidence{pivot,center,barrier_j_per_kg:barrier,work_j_per_kg:work})
    }

    fn mass(&self,rest:Mat4,anchor:Vec3)->Option<Mass> {
        // Work relative to the rest translation: squaring kilometre-scale world coordinates
        // and then subtracting COM^2 destroys the small body's inertia in f32.
        let mut area=0.0;let mut first=Vec3::ZERO;let mut second=Mat3::ZERO;
        for t in &self.triangles {
            let [a,b,c]=t.map(|p|rest.transform_vector3(p));
            let m=(b-a).cross(c-a).length()*0.5;let sum=a+b+c;
            area+=m;first+=sum*(m/3.0);
            second+=(outer(a)+outer(b)+outer(c)+outer(sum))*(m/12.0);
        }
        if !area.is_finite()||area<=1e-12 {return None;}
        let center=first/area;
        let covariance=second/area-outer(center);
        let inertia=Mat3::IDENTITY*(covariance.x_axis.x+covariance.y_axis.y+covariance.z_axis.z)-covariance;
        if !inertia.is_finite()||inertia.determinant()<=1e-14 {return None;}
        let radius=self.points.iter().map(|&p|(rest.transform_vector3(p)-center).length()).fold(0.0,f32::max);
        Some(Mass {center_offset:(rest.w_axis.truncate()-anchor)+center,inverse_inertia:inertia.inverse(),radius})
    }

    fn contact(&self,model:Mat4,anchor:Vec3,height:&impl Fn(f32,f32)->f32)->Option<Contact> {
        let offset=model.w_axis.truncate()-anchor;
        let mut greatest=f32::NEG_INFINITY;let mut contacts:Vec<(f32,Vec3)>=Vec::new();
        for &local in &self.points {
            let p=model.transform_vector3(local)+offset;
            let d=(height(anchor.x+p.x,anchor.z+p.z)-anchor.y)-p.y;
            if !d.is_finite() {continue;}
            if d>greatest {greatest=d;contacts.retain(|&(old,_)|old>=greatest-CONTACT_M);}
            if d>=greatest-CONTACT_M {contacts.push((d,p));}
        }
        greatest.is_finite().then(||Contact {correction:greatest,points:contacts.into_iter().map(|(_,p)|p).collect()})
    }

    pub fn advance(&self,rest:Mat4,anchor:Vec3,motion:&mut Motion,dt:f32,
        friction:f32,linear_drag:f32,height:impl Fn(f32,f32)->f32)->Step {
        self.advance_contacts(rest,anchor,motion,dt,friction,linear_drag,height,false)
    }

    /// Released trees can contact soil at their roots before a much later canopy impact.
    /// Report actual impacts throughout the fall; the tree owner applies its sound cadence.
    pub fn advance_all_impacts(&self,rest:Mat4,anchor:Vec3,motion:&mut Motion,dt:f32,
        friction:f32,linear_drag:f32,height:impl Fn(f32,f32)->f32)->Step {
        self.advance_contacts(rest,anchor,motion,dt,friction,linear_drag,height,true)
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_contacts(&self,rest:Mat4,anchor:Vec3,motion:&mut Motion,dt:f32,
        friction:f32,linear_drag:f32,height:impl Fn(f32,f32)->f32,all_impacts:bool)->Step {
        let dt=if dt.is_finite(){dt.clamp(0.0,0.1)}else{0.0};
        let Some(mass)=self.mass(rest,anchor) else {
            return Step {model:pose(rest,anchor,motion),sleeping:false,hit:None,max_correction_m:0.0};
        };
        motion.age+=dt;let mut remaining=dt;let mut hit:Option<(Vec3,f32)>=None;let mut max_correction_m=0.0_f32;
        while remaining>0.0 {
            // This bounds sampled source-point travel, including angular motion. It is not
            // swept triangle collision and cannot certify terrain between the samples.
            let speed=motion.velocity.length()+motion.angular_velocity.length()*mass.radius+G*STEP_S;
            let h=remaining.min(STEP_S).min(POINT_STEP_M/speed.max(0.001));
            if h<=0.0||remaining-h==remaining {break;}
            motion.velocity.y-=G*h;
            motion.pos+=motion.velocity*h;
            let old_center=motion.rotation*mass.center_offset;
            motion.rotation=(Quat::from_scaled_axis(motion.angular_velocity*h)*motion.rotation).normalize();
            motion.pos+=old_center-motion.rotation*mass.center_offset;
            let model=pose(rest,anchor,motion);
            if let Some(contact)=self.contact(model,motion.pos,&height) {
                if contact.correction>=-CONTACT_M {
                    max_correction_m=max_correction_m.max(contact.correction);
                    motion.pos.y+=contact.correction.max(0.0);
                    let polygon=crate::vehicle_ground::support_polygon(&contact.points);
                    let center=motion.rotation*mass.center_offset;
                    let rotation=Mat3::from_quat(motion.rotation);
                    let inverse=rotation*mass.inverse_inertia*rotation.transpose();
                    let impact=polygon.iter().map(|&p| {
                        let normal=ground_normal(motion.pos+p,&height);
                        -(motion.velocity+motion.angular_velocity.cross(p-center)).dot(normal)
                    }).fold(0.0_f32,f32::max);
                    // Source contacts carry gravity through their actual lever arms. An
                    // eccentric point therefore generates angular acceleration, never sleep.
                    for pass in 0..4 {for &p in &polygon {
                        let normal=ground_normal(motion.pos+p,&height);let arm=p-center;
                        let v=motion.velocity+motion.angular_velocity.cross(arm);
                        let closing=v.dot(normal);
                        if closing>=0.0 {continue;}
                        let lever=arm.cross(normal);
                        let restitution=if pass==0 {if motion.bounces==0 {0.24}else{0.10}}else{0.0};
                        let impulse=-(1.0+restitution)*closing/(1.0+lever.dot(inverse*lever));
                        motion.velocity+=normal*impulse;motion.angular_velocity+=inverse*lever*impulse;
                        let v=motion.velocity+motion.angular_velocity.cross(arm);
                        let tangent=v-normal*v.dot(normal);let speed=tangent.length();
                        if speed>1e-6 {
                            let direction=tangent/speed;let lever=arm.cross(direction);
                            let loss=(speed/(1.0+lever.dot(inverse*lever))).min(friction.max(0.0)*impulse);
                            motion.velocity-=direction*loss;motion.angular_velocity-=inverse*lever*loss;
                        }
                    }}
                    if impact>0.2 {
                        if (all_impacts||motion.bounces<2) && hit.is_none_or(|(_,old)|impact>old) {
                            if let Some(&p)=polygon.first() {hit=Some((motion.pos+p,impact));}
                        }
                        motion.bounces=motion.bounces.saturating_add(1);
                    }
                }
            }
            // Retain the existing scatter drag coefficients as explicit dissipative estimates.
            let drag=(-linear_drag.max(0.0)*h).exp();motion.velocity.x*=drag;motion.velocity.z*=drag;
            motion.angular_velocity*=(-0.72*h).exp();
            remaining=(remaining-h).max(0.0);
        }
        let model=pose(rest,anchor,motion);
        let stable=self.contact(model,motion.pos,&height).is_some_and(|contact| {
            if contact.correction.abs()>CONTACT_M {return false;}
            let polygon=crate::vehicle_ground::support_polygon(&contact.points);
            let center=motion.rotation*mass.center_offset;
            crate::vehicle_ground::stable_support(&polygon,center) && polygon.iter().all(|&p| {
                let n=ground_normal(motion.pos+p,&height);
                Vec3::new(n.x,0.0,n.z).length()<=friction.max(0.0)*n.y
            })
        });
        let sleeping=motion.age>0.45 && stable && motion.velocity.length()<0.28
            && motion.angular_velocity.length()<0.18;
        if sleeping {motion.velocity=Vec3::ZERO;motion.angular_velocity=Vec3::ZERO;}
        Step {model,sleeping,hit,max_correction_m}
    }
}

fn ground_normal(p:Vec3,height:&impl Fn(f32,f32)->f32)->Vec3 {
    const D:f32=0.025;
    Vec3::new(height(p.x-D,p.z)-height(p.x+D,p.z),2.0*D,
        height(p.x,p.z-D)-height(p.x,p.z+D)).normalize_or_zero()
}
fn pose(rest:Mat4,anchor:Vec3,motion:&Motion)->Mat4 {
    let mut model=Mat4::from_quat(motion.rotation)*rest;
    model.w_axis=(motion.pos+motion.rotation*(rest.w_axis.truncate()-anchor)).extend(1.0);
    model
}

#[cfg(test)] mod tests {
    use super::*;
    fn motion(pos:Vec3,rotation:Quat)->Motion {
        Motion {pos,velocity:Vec3::ZERO,rotation,angular_velocity:Vec3::ZERO,age:1.0,bounces:0}
    }
    fn source(min:Vec3,max:Vec3)->Source {
        Source::from_mesh(&crate::prims::box_mesh(min,max,[80;3])).unwrap()
    }
    #[test] fn release_work_rotates_about_actual_root_without_initial_root_slip() {
        let source=source(Vec3::new(-0.2,0.0,-0.2),Vec3::new(0.2,5.0,0.2));
        let rest=Mat4::from_translation(Vec3::Y*-0.025);
        let mut motion=motion(rest.w_axis.truncate(),Quat::IDENTITY);
        let evidence=source.release_about_support(rest,rest.w_axis.truncate(),&mut motion,
            Vec3::Z,1.0,|_,_|0.0).unwrap();
        assert!((evidence.pivot.z-0.2).abs()<1e-6&&evidence.pivot.x.abs()<1e-6);
        let root_velocity=motion.velocity+motion.angular_velocity.cross(evidence.pivot-evidence.center);
        assert!(root_velocity.length()<1e-6,"a releasing pivot is not driven through soil");
        let mass=source.mass(rest,rest.w_axis.truncate()).unwrap();
        let energy=0.5*(motion.velocity.length_squared()
            +motion.angular_velocity.dot(mass.inverse_inertia.inverse()*motion.angular_velocity));
        assert!((energy-evidence.work_j_per_kg).abs()<1e-5);
        assert!(evidence.work_j_per_kg>=2.0*evidence.barrier_j_per_kg);
    }
    #[test] fn buried_source_is_reported_and_released_motion_has_serializable_state() {
        let source=source(Vec3::new(-0.1,-0.025,-0.1),Vec3::new(0.1,3.0,0.1));
        assert!((source.minimum_clearance(Mat4::IDENTITY,|_,_|0.0).unwrap()+0.025).abs()<1e-6);
        let mut motion=motion(Vec3::ZERO,Quat::IDENTITY);
        let step=source.advance(Mat4::IDENTITY,Vec3::ZERO,&mut motion,1.0/120.0,0.55,2.15,|_,_|0.0);
        assert!(step.max_correction_m>=0.025 && step.max_correction_m<0.027,
            "initial root burial plus the ordinary gravity step must be reported");
        assert!(source.minimum_clearance(step.model,|_,_|0.0).unwrap()>=-1e-5);
        assert!(source.samples().iter().all(|p|p.is_finite()));
        let bytes=bincode::serialize(&motion).unwrap();
        assert_eq!(bincode::deserialize::<Motion>(&bytes).unwrap(),motion);
    }
    #[test] fn final_rotated_long_source_is_supported_without_fixed_floor_clearance() {
        let source=source(Vec3::new(-0.08,-1.5,-0.1),Vec3::new(0.08,1.5,0.1));
        let mut m=motion(Vec3::Y*0.05,Quat::from_rotation_z(0.63));
        let step=source.advance(Mat4::IDENTITY,Vec3::ZERO,&mut m,1.0/60.0,0.55,2.15,|_,_|0.0);
        assert!(source.points.iter().all(|&p|step.model.transform_point3(p).y>=-1e-5));
        assert!(m.pos.y>1.0,"actual tall rotated source cannot rest at smallest half-extent");
    }
    #[test] fn eccentric_corner_contact_produces_rotation_and_lowers_center_of_mass() {
        let source=source(Vec3::new(-1.0,-0.4,-0.5),Vec3::new(1.0,0.4,0.5));
        let mut m=motion(Vec3::ZERO,Quat::from_rotation_z(0.4)*Quat::from_rotation_x(0.2));
        let contact=source.contact(pose(Mat4::IDENTITY,Vec3::ZERO,&m),m.pos,&|_,_|0.0).unwrap();
        m.pos.y=contact.correction;
        let initial=m.pos.y;let rotation=m.rotation;
        for _ in 0..60 {source.advance(Mat4::IDENTITY,Vec3::ZERO,&mut m,1.0/60.0,0.55,2.15,|_,_|0.0);}
        assert!(m.rotation.dot(rotation).abs()<0.999,"unsupported COM must actually rotate");
        assert!(m.pos.y<initial-0.01,"gravity must lower the source COM");
    }
    #[test] fn old_lifetime_limit_cannot_freeze_airborne_source_and_supported_face_can_sleep() {
        let source=source(Vec3::splat(-0.5),Vec3::splat(0.5));
        let mut air=motion(Vec3::Y*20.0,Quat::IDENTITY);air.age=10.0;
        assert!(!source.advance(Mat4::IDENTITY,Vec3::ZERO,&mut air,1.0/60.0,0.55,2.15,|_,_|0.0).sleeping);
        assert!(air.pos.y<20.0 && air.velocity.y<0.0);
        let mut face=motion(Vec3::Y*0.5,Quat::IDENTITY);
        assert!(source.advance(Mat4::IDENTITY,Vec3::ZERO,&mut face,1.0/60.0,0.55,2.15,|_,_|0.0).sleeping);
    }
    #[test] fn uniform_shell_mass_uses_transformed_triangles_and_reports_sample_gap() {
        let source=source(Vec3::new(-1.0,0.0,-0.5),Vec3::new(1.0,2.0,0.5));
        let rest=Mat4::from_scale_rotation_translation(Vec3::new(1.0,2.0,0.5),Quat::from_rotation_y(0.6),Vec3::new(6000.0,4.0,-5000.0));
        let mass=source.mass(rest,rest.w_axis.truncate()).unwrap();
        assert!(mass.center_offset.distance(Vec3::Y*2.0)<1e-4);
        assert!(mass.inverse_inertia.is_finite()&&mass.inverse_inertia.determinant()>0.0);
        assert!(source.gap_bound_m>0.0&&source.gap_bound_m<=TARGET_SAMPLE_M);
        assert!(source.sample_count()>8);
    }
}
