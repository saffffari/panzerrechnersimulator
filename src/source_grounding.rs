//! Initial support for independent, closed loose rocks. Source faces supply possible resting
//! orientations; uniform solid density supplies an estimated centre of mass. This is a bounded
//! static admission test, not a rigid-body settling solver or an instruction to bury a mesh.
//! Terrain/source projected triangle intersections bound vertical penetration over whole faces.
//! Stability uses a 5 mm near-contact band, explicitly allowing that much unresolved roughness.
use std::collections::HashMap;
use glam::{Mat4, Quat, Vec2, Vec3};
use crate::{gltf_load::CpuMesh, terrain::Heightfield};

pub const SUPPORT_GAP_M:f32=0.005;
/// Uniform-density COM must lie strictly inside the contact polygon, not balance on an edge.
pub const SUPPORT_MARGIN_M:f32=0.0005;
const GEOMETRY_EPS:f32=0.00005;
const MAX_SOURCE_TRIANGLES:usize=512;
const MAX_TERRAIN_CELLS:usize=4096;
const MAX_TRIANGLE_PAIRS:usize=32768;
const MAX_ORIENTATIONS:usize=4;

pub fn is_loose_rock_asset(path:&str)->bool {
    let stem=std::path::Path::new(path).file_stem().and_then(|s|s.to_str()).unwrap_or("").to_ascii_lowercase();
    let tokens:Vec<_>=stem.split(['_','-',' ']).collect();
    tokens.first().is_some_and(|t|matches!(*t,"stone"|"rock"|"boulder"))
        && !tokens.iter().any(|t|matches!(*t,"wall"|"fence"|"tile"|"floor"|"column"|"arch"|"pile"|"rubble"))
}

/// These exact Canyon generator sources are embedded rockfall formations, with an authored
/// ground-0.55m origin and upward snow material. They are not independent loose stones.
/// The caller must also require an unedited base placement; this never authorizes arbitrary
/// meshes named `rock`, later editor additions, or another map's use of the same source.
pub fn is_authored_canyon_formation(path:&str,authoring:&serde_json::Value)->bool {
    authoring["tool"].as_str()==Some("tools/worldgen/gaea_canyon_v2.py")
        &&authoring["tool_version"].as_u64()==Some(6)
        &&authoring["variant"].as_str()==Some("canyon-v2")
        &&authoring["mode"].as_str()==Some("course")
        &&matches!(path,"assets/authoring/canyon-v2/rock_0.glb"
            |"assets/authoring/canyon-v2/rock_1.glb"|"assets/authoring/canyon-v2/rock_2.glb")
}

pub struct SourceSupport {
    triangles:Vec<[Vec3;3]>,
    vertices:Vec<Vec3>,
    center_of_mass:Vec3,
    rests:Vec<Quat>,
    /// Maximum vertex distance from the authored origin. Safe for every candidate rotation.
    pub radius_m:f32,
}
pub struct GroundedPose {
    pub model:Mat4,
    pub support_gap_m:f32,
    pub tested_triangle_pairs:usize,
}

fn cross(a:Vec2,b:Vec2)->f32 {a.x*b.y-a.y*b.x}
fn xz(p:Vec3)->Vec2 {Vec2::new(p.x,p.z)}
fn hull(mut p:Vec<Vec2>)->Vec<Vec2> {
    p.sort_by(|a,b|a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a,b|a.distance_squared(*b)<1e-12);
    if p.len()<3 {return p;}
    let mut out=Vec::new();
    for &v in &p {
        while out.len()>=2&&cross(out[out.len()-1]-out[out.len()-2],v-out[out.len()-1])<=0.0 {out.pop();}
        out.push(v);
    }
    let lower=out.len();
    for &v in p.iter().rev().skip(1) {
        while out.len()>lower&&cross(out[out.len()-1]-out[out.len()-2],v-out[out.len()-1])<=0.0 {out.pop();}
        out.push(v);
    }
    out.pop();out
}
fn supports(p:&[Vec2],center:Vec2)->bool {
    p.len()>=3&&p.iter().zip(p.iter().cycle().skip(1)).all(|(&a,&b)|cross(b-a,center-a)>SUPPORT_MARGIN_M*(b-a).length())
        && p.iter().zip(p.iter().cycle().skip(1)).map(|(&a,&b)|cross(a,b)).sum::<f32>().abs()>1e-8
}

impl SourceSupport {
    pub fn from_mesh(mesh:&CpuMesh)->Option<Self> {
        if mesh.indices.len()%3!=0||mesh.indices.len()/3>MAX_SOURCE_TRIANGLES {return None;}
        let vertices:Vec<Vec3>=mesh.vertices.iter().map(|v|Vec3::from(v.pos)).collect();
        if vertices.is_empty()||vertices.iter().any(|v|!v.is_finite()) {return None;}
        let mut ids=HashMap::new();let mut edges=HashMap::<(usize,usize),(u32,i32)>::new();
        let mut triangles=Vec::new();let mut volume=0.0;let mut moment=Vec3::ZERO;
        for indices in mesh.indices.chunks_exact(3) {
            let [a,b,c]=[vertices.get(indices[0] as usize)?,vertices.get(indices[1] as usize)?,vertices.get(indices[2] as usize)?].map(|p|*p);
            if (b-a).cross(c-a).length_squared()<1e-14 {continue;}
            let mut ix=[0;3];
            for (i,p) in [a,b,c].into_iter().enumerate() {
                let key=((p.x*100000.0).round() as i64,(p.y*100000.0).round() as i64,(p.z*100000.0).round() as i64);
                let next=ids.len();ix[i]=*ids.entry(key).or_insert(next);
            }
            for (i,j) in [(ix[0],ix[1]),(ix[1],ix[2]),(ix[2],ix[0])] {
                if i==j {return None;}
                let e=edges.entry((i.min(j),i.max(j))).or_default();e.0+=1;e.1+=if i<j {1}else{-1};
            }
            let v=a.dot(b.cross(c))/6.0;volume+=v;moment+=(a+b+c)*(v/4.0);triangles.push([a,b,c]);
        }
        if volume.abs()<1e-8||edges.values().any(|&(count,sign)|count!=2||sign!=0) {return None;}
        let center_of_mass=moment/volume;
        if volume<0.0 {for t in &mut triangles {t.swap(1,2);}}
        let mut candidates:Vec<(f32,Quat)>=Vec::new();
        for &[a,b,c] in &triangles {
            let normal=(b-a).cross(c-a).normalize_or_zero();
            // A source face must also be a supporting hull plane, not a concavity.
            if vertices.iter().any(|p|normal.dot(*p-a)>GEOMETRY_EPS) {continue;}
            let q=Quat::from_rotation_arc(normal,Vec3::NEG_Y);
            if candidates.iter().any(|(_,old)|old.dot(q).abs()>0.999999) {continue;}
            let low=vertices.iter().map(|p|(q**p).y).fold(f32::INFINITY,f32::min);
            let contact=hull(vertices.iter().map(|p|q**p).filter(|p|p.y-low<GEOMETRY_EPS).map(xz).collect());
            let cm=q*center_of_mass;
            if supports(&contact,xz(cm)) {candidates.push((cm.y-low,q));}
        }
        candidates.sort_by(|a,b|a.0.total_cmp(&b.0));candidates.truncate(MAX_ORIENTATIONS);
        if candidates.is_empty() {return None;}
        let radius_m=vertices.iter().map(|p|p.length()).fold(0.0,f32::max);
        Some(Self{triangles,vertices,center_of_mass,rests:candidates.into_iter().map(|(_,q)|q).collect(),radius_m})
    }
}

/// Try the cached stable source faces, retaining caller yaw/terrain alignment. No mesh rewrite.
pub fn grounded_source_pose(source:&SourceSupport,rotation:Quat,scale:f32,at:Vec2,hf:&Heightfield)->Option<GroundedPose> {
    source.rests.iter().find_map(|rest|grounded_fixed_source_pose(source,(rotation**rest).normalize(),scale,at,hf))
}

/// Re-admit an already selected rotation, e.g. after the scatter GPU format quantizes it.
/// Terrain cells are exact heightfield/detail lattice triangles, not an AABB contact proxy.
pub fn grounded_fixed_source_pose(source:&SourceSupport,rotation:Quat,scale:f32,at:Vec2,hf:&Heightfield)->Option<GroundedPose> {
    if !rotation.is_finite()||!scale.is_finite()||scale<=0.0||!at.is_finite() {return None;}
    let points:Vec<_>=source.vertices.iter().map(|p|rotation*(*p*scale)).collect();
    let (lo,hi)=points.iter().fold((Vec2::splat(f32::INFINITY),Vec2::splat(f32::NEG_INFINITY)),|(lo,hi),&p|(lo.min(xz(p)+at),hi.max(xz(p)+at)));
    if !hf.openings.meeting(lo,hi).is_empty() {return None;}
    let (step,half,res)=hf.detail.as_ref().map_or((hf.spacing,(hf.res as f32-1.0)*hf.spacing*0.5,hf.res),|d|(d.lattice,d.half,d.lattice_res));
    if step<=0.0||res<2||lo.min_element()<=-half||hi.max_element()>=half {return None;}
    let cell=|p:Vec2|((p+Vec2::splat(half))/step).floor().as_ivec2();
    let (first,last)=(cell(lo),cell(hi));let size=last-first+glam::IVec2::ONE;
    if size.x as usize*size.y as usize>MAX_TERRAIN_CELLS {return None;}
    let mut samples=Vec::<(Vec2,f32)>::new();let mut highest=f32::NEG_INFINITY;let mut tested=0;
    let mut max_grade=0.0_f32;
    // Heights cached once per candidate. Computation remains bounded independently of world size.
    let mut ground=HashMap::<(i32,i32),Vec3>::new();
    for z in first.y..=last.y+1 {for x in first.x..=last.x+1 {
        let wx=x as f32*step-half;let wz=z as f32*step-half;
        ground.insert((x,z),Vec3::new(wx-at.x,hf.grade_at(wx,wz),wz-at.y));
    }}
    for tri in &source.triangles {
        let a=tri.map(|p|rotation*(p*scale));
        if (a[1]-a[0]).cross(a[2]-a[0]).y>=-1e-9 {continue;}
        let amin=a.iter().map(|p|xz(*p)+at).fold(Vec2::splat(f32::INFINITY),Vec2::min);
        let amax=a.iter().map(|p|xz(*p)+at).fold(Vec2::splat(f32::NEG_INFINITY),Vec2::max);
        let (begin,end)=(cell(amin).max(first),cell(amax).min(last));
        for z in begin.y..=end.y {for x in begin.x..=end.x {
            let [p00,p10,p01,p11]=[ground[&(x,z)],ground[&(x+1,z)],ground[&(x,z+1)],ground[&(x+1,z+1)]];
            for b in [[p00,p10,p01],[p11,p01,p10]] {
                tested+=1;if tested>MAX_TRIANGLE_PAIRS {return None;}
                let normal=(b[1]-b[0]).cross(b[2]-b[0]);
                max_grade=max_grade.max(Vec2::new(normal.x,normal.z).length()/normal.y.abs().max(1e-12));
                for point in overlap(a.map(xz),b.map(xz)) {
                    let delta=height_on(b,point)?-height_on(a,point)?;
                    if !delta.is_finite() {return None;}
                    highest=highest.max(delta);samples.push((point,delta));
                }
            }
        }}
    }
    if !highest.is_finite() {return None;}
    // Every triangle-plane difference is affine on its clipped polygon, so its maximum occurs
    // at one of these vertices. The tiny positive allowance covers f32 placement arithmetic.
    // World-space matrix arithmetic loses precision with distance from origin. Reserve a named
    // eight-ULP budget (including terrain slope) rather than claiming the fixed50um allowance
    // covers a placement kilometres away. If this consumes the support band, reject it.
    // X/Z rounding changes support height only through terrain grade. A distant flat plane
    // therefore has the same admissible contact band as one at the origin.
    let allowance=GEOMETRY_EPS+8.0*f32::EPSILON*((highest.abs()+source.radius_m*scale)*(1.0+max_grade)+at.abs().max_element()*max_grade);
    if allowance>=SUPPORT_GAP_M {return None;}
    let y=highest+allowance;
    let contacts=hull(samples.iter().filter(|(_,d)|highest-*d<=SUPPORT_GAP_M-allowance).map(|(p,_)|*p).collect());
    let cm=rotation*(source.center_of_mass*scale);
    if !supports(&contacts,xz(cm)) {return None;}
    let support_gap_m=samples.iter().filter(|(_,d)|highest-*d<=SUPPORT_GAP_M-allowance).map(|(_,d)|y-*d).fold(0.0,f32::max);
    Some(GroundedPose{model:Mat4::from_scale_rotation_translation(Vec3::splat(scale),rotation,Vec3::new(at.x,y,at.y)),support_gap_m,tested_triangle_pairs:tested})
}

fn height_on(t:[Vec3;3],p:Vec2)->Option<f32> {
    let a=xz(t[0]);let b=xz(t[1])-a;let c=xz(t[2])-a;let det=cross(b,c);
    if det.abs()<1e-12 {return None;}
    let q=p-a;let u=cross(q,c)/det;let v=cross(b,q)/det;
    Some(t[0].y+u*(t[1].y-t[0].y)+v*(t[2].y-t[0].y))
}
fn overlap(a:[Vec2;3],mut b:[Vec2;3])->Vec<Vec2> {
    if cross(b[1]-b[0],b[2]-b[0])<0.0 {b.swap(1,2);}
    let mut polygon=a.to_vec();
    for i in 0..3 {
        let (origin,end)=(b[i],b[(i+1)%3]);let mut next=Vec::new();
        for j in 0..polygon.len() {
            let (p,q)=(polygon[j],polygon[(j+1)%polygon.len()]);
            let (dp,dq)=(cross(end-origin,p-origin),cross(end-origin,q-origin));
            if dp>=0.0 {next.push(p);}
            if (dp>=0.0)!=(dq>=0.0) {next.push(p+(q-p)*(dp/(dp-dq)));}
        }
        polygon=next;if polygon.is_empty(){break;}
    }
    polygon
}

#[cfg(test)] mod tests {
    use super::*;
    fn diamond()->CpuMesh {
        let p=[Vec3::new(0.,0.,0.),Vec3::new(-1.,1.,-1.),Vec3::new(1.,1.,-1.),Vec3::new(1.,1.,1.),Vec3::new(-1.,1.,1.),Vec3::new(0.,2.,0.)];
        let vertices=p.iter().map(|p|crate::gltf_load::Vertex{pos:p.to_array(),normal:Vec3::Y.to_array(),uv:[0.;2],tangent:[1.,0.,0.,1.]}).collect();
        CpuMesh::basic(vertices,vec![0,2,1,0,3,2,0,4,3,0,1,4,5,1,2,5,2,3,5,3,4,5,4,1],None,Vec3::new(-1.,0.,-1.),Vec3::new(1.,2.,1.))
    }
    fn flat()->Heightfield {Heightfield{res:33,spacing:0.25,heights:vec![0.;33*33],min_h:0.,max_h:0.,openings:Default::default(),detail:None}}
    #[test] fn point_bottom_rock_gets_source_face_support_without_penetration() {
        let mesh=diamond();let source=SourceSupport::from_mesh(&mesh).unwrap();let hf=flat();
        assert!(grounded_fixed_source_pose(&source,Quat::IDENTITY,1.,Vec2::ZERO,&hf).is_none());
        let pose=grounded_source_pose(&source,Quat::IDENTITY,1.,Vec2::ZERO,&hf).unwrap();
        let p:Vec<_>=mesh.vertices.iter().map(|v|pose.model.transform_point3(Vec3::from(v.pos))).collect();
        assert!(p.iter().all(|p|p.y>=-GEOMETRY_EPS));
        assert!(p.iter().filter(|p|p.y<SUPPORT_GAP_M).count()>=3);
        assert!(pose.support_gap_m<=SUPPORT_GAP_M&&pose.tested_triangle_pairs>0);
    }
    #[test] fn interior_terrain_peak_is_checked_even_when_all_source_vertices_miss_it() {
        let a=[Vec3::new(-1.,0.,-1.),Vec3::new(1.,0.,-1.),Vec3::new(0.,0.,1.)];
        let b=[Vec3::new(0.,0.4,0.),Vec3::new(0.25,0.,0.),Vec3::new(0.,0.,0.25)];
        let max=overlap(a.map(xz),b.map(xz)).iter().map(|p|height_on(b,*p).unwrap()-height_on(a,*p).unwrap()).fold(f32::NEG_INFINITY,f32::max);
        assert!((max-0.4).abs()<1e-6);
    }
    #[test] fn unsupported_terrain_peak_rejects_a_previously_stable_rest() {
        let p=[Vec3::new(-1.,0.,-1.),Vec3::new(1.,0.,-1.),Vec3::new(0.,0.,1.),Vec3::new(0.,1.,0.)];
        let vertices=p.iter().map(|p|crate::gltf_load::Vertex{pos:p.to_array(),normal:Vec3::Y.to_array(),uv:[0.;2],tangent:[1.,0.,0.,1.]}).collect();
        let mesh=CpuMesh::basic(vertices,vec![0,1,2,0,3,1,1,3,2,2,3,0],None,Vec3::new(-1.,0.,-1.),Vec3::new(1.,1.,1.));
        let source=SourceSupport::from_mesh(&mesh).unwrap();let mut hf=flat();
        assert!(grounded_fixed_source_pose(&source,Quat::IDENTITY,1.,Vec2::ZERO,&hf).is_some());
        hf.heights[16*33+16]=0.4;
        assert!(grounded_fixed_source_pose(&source,Quat::IDENTITY,1.,Vec2::ZERO,&hf).is_none());
    }
    #[test] fn open_source_and_decorative_stone_names_are_not_silently_grounded() {
        let mut mesh=diamond();mesh.indices.truncate(mesh.indices.len()-3);assert!(SourceSupport::from_mesh(&mesh).is_none());
        assert!(is_loose_rock_asset("Trees, Rocks & Nature/stone_2.glb"));
        assert!(!is_loose_rock_asset("Trees, Rocks & Nature/tree_2.glb"));
        assert!(!is_loose_rock_asset("Props/stone_wall_1.glb"));
    }
    #[test] fn embedded_canyon_formation_requires_exact_source_and_generator_ownership() {
        let authoring=serde_json::json!({"tool":"tools/worldgen/gaea_canyon_v2.py","tool_version":6,"variant":"canyon-v2","mode":"course"});
        assert!(is_authored_canyon_formation("assets/authoring/canyon-v2/rock_1.glb",&authoring));
        assert!(!is_authored_canyon_formation("assets/pizza/rock_1.glb",&authoring));
        assert!(!is_authored_canyon_formation("assets/authoring/canyon-v2/rock_1.glb",&serde_json::Value::Null));
        let mut foreign=authoring;foreign["tool"]=serde_json::json!("hand_placed.py");
        assert!(!is_authored_canyon_formation("assets/authoring/canyon-v2/rock_1.glb",&foreign));
        foreign["tool"]=serde_json::json!("tools/worldgen/gaea_canyon_v2.py");foreign["tool_version"]=serde_json::json!(7);
        assert!(!is_authored_canyon_formation("assets/authoring/canyon-v2/rock_1.glb",&foreign));
    }
    #[test] fn distant_flat_and_sloped_support_do_not_fail_from_horizontal_translation() {
        let source=SourceSupport::from_mesh(&diamond()).unwrap();
        for grade in [0.0_f32,0.01] {
            let heights=(0..25*25).map(|i|((i%25) as f32-12.0)*1000.0*grade).collect();
            let hf=Heightfield{res:25,spacing:1000.0,heights,min_h:-120.,max_h:120.,openings:Default::default(),detail:None};
            let rotation=Quat::from_rotation_arc(Vec3::Y,Vec3::new(-grade,1.,0.).normalize());
            let near=grounded_source_pose(&source,rotation,1.,Vec2::ZERO,&hf).unwrap();
            let selected=near.model.to_scale_rotation_translation().1;
            let far=grounded_fixed_source_pose(&source,selected,1.,Vec2::new(5250.,0.),&hf).unwrap();
            assert!((far.model.w_axis.y-near.model.w_axis.y-5250.0*grade).abs()<0.001);
            assert!(far.support_gap_m<=SUPPORT_GAP_M);
        }
    }
    #[test] fn center_of_mass_on_a_support_edge_is_not_stable() {
        let triangle=[Vec2::ZERO,Vec2::X,Vec2::Y];
        assert!(!supports(&triangle,Vec2::new(0.5,0.5)));
        assert!(supports(&triangle,Vec2::splat(0.25)));
    }
}
