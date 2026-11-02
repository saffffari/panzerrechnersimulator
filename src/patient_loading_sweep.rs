//! Conservative continuous rigid-source sweep for assisted patient loading.
//! Uses the convex hull of each original triangle's two endpoint poses. That hull contains
//! every linear vertex interpolation, so a thin edge cannot tunnel between vertex rays.
//! A caller-supplied analytic rotation-arc deviation enlarges only the numerical sweep bound.
use glam::{Mat4,Vec3};
use std::collections::{BTreeMap,BTreeSet};
type Triangle=[Vec3;3];
const CELL:f32=0.5;
#[derive(Clone,Copy,Debug)]
pub(in crate::game) struct Contact {pub triangle:usize,pub obstacle:usize}
pub(in crate::game) struct SourceIndex {triangles:Vec<Triangle>,cells:BTreeMap<(i32,i32,i32),Vec<usize>>,domain:Option<(Vec3,Vec3)>}
fn bounds(points:impl IntoIterator<Item=Vec3>)->(Vec3,Vec3){points.into_iter().fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),|(lo,hi),p|(lo.min(p),hi.max(p)))}
fn overlaps(a:(Vec3,Vec3),b:(Vec3,Vec3),margin:f32)->bool {a.0.cmple(b.1+Vec3::splat(margin)).all()&&b.0.cmple(a.1+Vec3::splat(margin)).all()}
fn separated(axis:Vec3,hull:&[Vec3;6],obstacle:&Triangle,margin:f32)->bool {
    let length=axis.length();if length<1e-9{return false;}let axis=axis/length;
    let (a,b)=hull.iter().map(|p|p.dot(axis)).fold((f32::INFINITY,f32::NEG_INFINITY),|(lo,hi),v|(lo.min(v),hi.max(v)));
    let (c,d)=obstacle.iter().map(|p|p.dot(axis)).fold((f32::INFINITY,f32::NEG_INFINITY),|(lo,hi),v|(lo.min(v),hi.max(v)));
    b+margin<c||d+margin<a
}
fn swept_triangle_intersects(a:Triangle,b:Triangle,obstacle:Triangle,margin:f32)->bool {
    if !overlaps(bounds(a.into_iter().chain(b)),bounds(obstacle),margin){return false;}
    // Source-local subtraction improves projection arithmetic without moving either surface.
    let origin=a[0];let hull=[a[0],a[1],a[2],b[0],b[1],b[2]].map(|p|p-origin);let obstacle=obstacle.map(|p|p-origin);
    if separated((obstacle[1]-obstacle[0]).cross(obstacle[2]-obstacle[0]),&hull,&obstacle,margin){return false;}
    for i in 0..6{for j in i+1..6{let edge=hull[j]-hull[i];
        for k in j+1..6{if separated(edge.cross(hull[k]-hull[i]),&hull,&obstacle,margin){return false;}}
        for k in 0..3{if separated(edge.cross(obstacle[(k+1)%3]-obstacle[k]),&hull,&obstacle,margin){return false;}}
    }}true
}
impl SourceIndex {
    pub(in crate::game) fn new(triangles:Vec<Triangle>)->Self {
        let mut cells=BTreeMap::<_,Vec<usize>>::new();for(i,tri)in triangles.iter().enumerate(){let(lo,hi)=bounds(*tri);let lo=(lo/CELL).floor().as_ivec3();let hi=(hi/CELL).floor().as_ivec3();
            for x in lo.x..=hi.x{for y in lo.y..=hi.y{for z in lo.z..=hi.z{cells.entry((x,y,z)).or_default().push(i);}}}
        }Self{triangles,cells,domain:None}
    }
    /// Retain original triangles, but index only the caller's finite admitted query domain.
    /// Insertion budget bounds work/memory even for a kilometre-long wall or bridge face.
    pub(in crate::game) fn bounded(triangles:Vec<Triangle>,lo:Vec3,hi:Vec3,budget:usize)->Result<Self,&'static str> {
        if !lo.is_finite()||!hi.is_finite()||lo.cmpgt(hi).any()||triangles.len()>32768{return Err("invalid bounded source domain");}
        let mut cells=BTreeMap::<_,Vec<usize>>::new();let mut inserted=0usize;
        for(i,tri)in triangles.iter().enumerate(){
            if tri.iter().any(|p|!p.is_finite()){return Err("non-finite source triangle");}
            let(a,b)=bounds(*tri);if !overlaps((a,b),(lo,hi),0.){continue;}
            let first=(a.max(lo)/CELL).floor();let last=(b.min(hi)/CELL).floor();
            if first.min_element()<i32::MIN as f32+1024.||last.max_element()>i32::MAX as f32-1024.{return Err("source cell index outside bounded range");}
            let first=first.as_ivec3();let last=last.as_ivec3();
            let extent=[0,1,2].map(|axis|i64::from(last[axis])-i64::from(first[axis])+1);
            let count=extent.into_iter().try_fold(1usize,|n,d|n.checked_mul(usize::try_from(d).ok()?)).ok_or("source cell insertion capacity exceeded")?;
            inserted=inserted.checked_add(count).filter(|n|*n<=budget).ok_or("source cell insertion capacity exceeded")?;
            for x in first.x..=last.x{for y in first.y..=last.y{for z in first.z..=last.z{cells.entry((x,y,z)).or_default().push(i);}}}
        }
        Ok(Self{triangles,cells,domain:Some((lo,hi))})
    }
    pub(in crate::game) fn contact(&self,source:&[Triangle],before:Mat4,after:Mat4,arc_margin:f32)->Option<Contact>{
        if !before.is_finite()||!after.is_finite()||!arc_margin.is_finite()||arc_margin<0.{return Some(Contact{triangle:0,obstacle:0});}
        for(i,tri)in source.iter().enumerate(){let a=tri.map(|p|before.transform_point3(p));let b=tri.map(|p|after.transform_point3(p));
            let(lo,hi)=bounds(a.into_iter().chain(b));
            if self.domain.is_some_and(|(minimum,maximum)|(lo-Vec3::splat(arc_margin)).cmplt(minimum).any()||(hi+Vec3::splat(arc_margin)).cmpgt(maximum).any()) {return Some(Contact{triangle:i,obstacle:usize::MAX});}
            let lo=((lo-Vec3::splat(arc_margin))/CELL).floor().as_ivec3();let hi=((hi+Vec3::splat(arc_margin))/CELL).floor().as_ivec3();let mut visited=BTreeSet::new();
            for x in lo.x..=hi.x{for y in lo.y..=hi.y{for z in lo.z..=hi.z{if let Some(candidates)=self.cells.get(&(x,y,z)){for &j in candidates{if visited.insert(j)&&swept_triangle_intersects(a,b,self.triangles[j],arc_margin){return Some(Contact{triangle:i,obstacle:j});}}}}}}
        }None
    }
}
/// Maximum circular arc deviation from its endpoint chord. This is derived from the actual
/// retained source radius and admitted rotation, not an arbitrary body inflation.
pub(in crate::game) fn arc_deviation(radius:f32,radians:f32)->f32{radius*(1.0-(radians.abs()*0.5).cos())}
#[cfg(test)]mod tests {use super::*;
    #[test]fn catches_thin_obstacle_between_all_vertex_rays(){
        let source=[Vec3::new(-1.,-1.,-1.),Vec3::new(1.,-1.,-1.),Vec3::new(0.,1.,-1.)];
        let small=[Vec3::new(-0.05,-0.05,0.),Vec3::new(0.05,-0.05,0.),Vec3::new(0.,0.05,0.)];
        let index=SourceIndex::new(vec![small]);assert!(index.contact(&[source],Mat4::IDENTITY,Mat4::from_translation(Vec3::Z*2.),0.).is_some());
    }
    #[test]fn clear_endpoint_does_not_admit_a_floor_crossing(){let source=[Vec3::new(-0.2,-1.,0.),Vec3::new(0.2,-1.,0.),Vec3::new(0.,-1.,0.3)];let floor=[Vec3::new(-2.,0.,-2.),Vec3::new(2.,0.,-2.),Vec3::new(0.,0.,2.)];
        let index=SourceIndex::new(vec![floor]);assert!(index.contact(&[source],Mat4::IDENTITY,Mat4::from_translation(Vec3::Y*2.),0.).is_some());}
    #[test]fn separated_lift_is_admitted_without_rescaling(){let source=[Vec3::new(-0.2,0.1,0.),Vec3::new(0.2,0.1,0.),Vec3::new(0.,0.1,0.3)];let floor=[Vec3::new(-2.,0.,-2.),Vec3::new(2.,0.,-2.),Vec3::new(0.,0.,2.)];
        let index=SourceIndex::new(vec![floor]);assert!(index.contact(&[source],Mat4::IDENTITY,Mat4::from_translation(Vec3::Y),0.).is_none());}
    #[test]fn rotational_bound_uses_original_radius(){assert_eq!(arc_deviation(1.,0.),0.);assert!((arc_deviation(2.,std::f32::consts::PI)-2.).abs()<1e-6);assert!(arc_deviation(1.,1f32.to_radians())<0.00004);}
    #[test]fn bounded_source_index_clamps_long_wall_without_clipping_geometry(){
        let wall=[Vec3::new(-100000.,0.,-100000.),Vec3::new(100000.,0.,-100000.),Vec3::new(0.,0.,100000.)];
        let index=SourceIndex::bounded(vec![wall],Vec3::splat(-1.),Vec3::splat(1.),32).unwrap();
        assert_eq!(index.triangles[0],wall);assert!(index.cells.values().map(Vec::len).sum::<usize>()<=25);
        let face=[[Vec3::new(-0.1,-0.2,-0.1),Vec3::new(0.1,-0.2,-0.1),Vec3::new(0.,-0.2,0.1)]];
        assert!(index.contact(&face,Mat4::IDENTITY,Mat4::from_translation(Vec3::Y*0.4),0.).is_some());
        assert!(SourceIndex::bounded(vec![wall],Vec3::splat(-1.),Vec3::splat(1.),24).is_err());
        assert!(index.contact(&face,Mat4::IDENTITY,Mat4::from_translation(Vec3::X*3.),0.).is_some(),"queries outside indexed domain must fail closed");
    }

}


#[derive(Clone,Copy,serde::Serialize,serde::Deserialize)]
pub(super) struct Pose {pub center:Vec3,pub rotation:glam::Quat}
impl Pose {
    pub(super) fn matrix(self,original_center:Vec3)->Mat4 {Mat4::from_translation(self.center)*Mat4::from_quat(self.rotation)*Mat4::from_translation(-original_center)}
    pub(super) fn blend(self,to:Self,t:f32)->Self {Self{center:self.center.lerp(to.center,t),rotation:self.rotation.slerp(to.rotation,t)}}
}
/// Derived waypoints, not a guessed lift height. The complete source radius clears rotation
/// behind the physical gate; its measured lower face determines clearance over the gate.
pub(super) fn staged_route(source:&[Triangle],from:Vec3,to:Vec3,rotation:glam::Quat,gate_rear:f32,gate_top:f32,back_ground:f32)->Vec<Pose> {
    let radius=source.iter().flatten().map(|p|p.distance(from)).fold(0.,f32::max);
    let minimum=source.iter().flatten().map(|p|(to+rotation*(*p-from)).y).fold(f32::INFINITY,f32::min);
    let back=from.z.min(gate_rear-radius-0.002);
    let lift=from.y.max(to.y+(gate_top+0.002-minimum).max(0.)).max(back_ground+radius+0.002);
    let mut route=vec![Pose{center:from,rotation:glam::Quat::IDENTITY}];
    for pose in [Pose{center:Vec3::new(from.x,from.y,back),rotation:glam::Quat::IDENTITY},
        Pose{center:Vec3::new(from.x,lift,back),rotation:glam::Quat::IDENTITY},
        Pose{center:Vec3::new(to.x,lift,back),rotation},Pose{center:Vec3::new(to.x,lift,to.z),rotation},Pose{center:to,rotation}] {
        let last=route.last().unwrap();if last.center.distance_squared(pose.center)>1e-10||last.rotation.angle_between(pose.rotation).abs()>1e-5{route.push(pose);}
    }route
}
pub(super) fn pose_at(route:&[Pose],elapsed:f32)->Pose {let segments=route.len().saturating_sub(1).max(1);let f=(elapsed/2.).clamp(0.,1.)*segments as f32;let i=(f.floor()as usize).min(segments-1);let t=(f-i as f32).clamp(0.,1.);route[i].blend(route[(i+1).min(route.len()-1)],t*t*(3.-2.*t))}
/// No outcome is forced: return the first blocked interval with its original source contact.
/// Max2cm translation and1degree rotation keep conservative hull expansion small. Every
/// interval still covers the continuous linear vertex motion, unlike endpoint-only sampling.
pub(super) fn admit_route(source:&[Triangle],index:&SourceIndex,origin:Vec3,route:&[Pose],helper_eye:Vec3,reach:f32)->Result<(),String> {
    if source.is_empty()||route.len()<2||route.len()>6{return Err("patient loading path is unavailable".into());}
    let radius=source.iter().flatten().map(|p|p.distance(origin)).fold(0.,f32::max);
    for(stage,pair)in route.windows(2).enumerate(){
        let distance=pair[0].center.distance(pair[1].center);let angle=pair[0].rotation.angle_between(pair[1].rotation).abs();
        let count=((distance/0.02).max(angle/1f32.to_radians()).ceil()as usize).max(1);
        if count>1024{return Err("patient loading path exceeds bounded source query capacity".into());}
        let mut before=pair[0];for step in 1..=count {let after=pair[0].blend(pair[1],step as f32/count as f32);
            let matrix=after.matrix(origin);let nearest=source.iter().flatten().map(|p|matrix.transform_point3(*p).distance_squared(helper_eye)).fold(f32::INFINITY,f32::min);
            if nearest>reach*reach{return Err(format!("move beside patient: assisted loading stage{} exceeds actual source reach",stage+1));}
            let margin=arc_deviation(radius,before.rotation.angle_between(after.rotation));
            if let Some(contact)=index.contact(source,before.matrix(origin),matrix,margin){return Err(format!("patient loading stage{} blocked by source triangle{} at flesh triangle{}",stage+1,contact.obstacle,contact.triangle));}
            before=after;
        }
    }Ok(())
}
#[cfg(test)]mod route_tests{use super::*;
    #[test]fn staged_loading_clears_rotation_before_entering_gate(){let source=[[Vec3::new(-0.2,0.2,-4.3),Vec3::new(0.2,0.2,-4.3),Vec3::new(0.,0.6,-5.3)]];let from=Vec3::new(0.,0.4,-4.8);let to=Vec3::new(0.,1.5,-1.38);
        let route=staged_route(&source,from,to,glam::Quat::IDENTITY,-3.63,1.062,0.);assert!(route.len()>=3);assert_eq!(route.last().unwrap().center,to);
        let max_back_z=source.iter().flatten().map(|p|route[1].matrix(from).transform_point3(*p).z).fold(f32::NEG_INFINITY,f32::max);assert!(max_back_z< -3.63);
        let gate=[[Vec3::new(-1.2,0.,-3.0),Vec3::new(1.2,0.,-3.0),Vec3::new(0.,1.062,-3.0)]];let index=SourceIndex::new(gate.to_vec());
        assert!(admit_route(&source,&index,from,&route,Vec3::new(0.,1.6,-4.8),4.).is_ok());
        assert!(admit_route(&source,&index,from,&[route[0],*route.last().unwrap()],Vec3::new(0.,1.6,-4.8),4.).is_err());
    }
}

/// Preserve intermediate corners if a variable frame crosses a loading-stage boundary.
pub(super) fn interval_route(route:&[Pose],from:f32,to:f32)->Vec<Pose>{
    let mut out=vec![pose_at(route,from)];let segments=route.len()-1;
    for(i,&pose)in route.iter().enumerate().skip(1).take(segments-1){let time=2.*i as f32/segments as f32;if time>from&&time<to{out.push(pose);}}
    out.push(pose_at(route,to));out
}
