//! Close the straight light path below the source traverse bearing with an overlapping annulus.
//! Source Turret_0113 spans r=1.222 m, hull y=1.935..2.122 m; Hull_0708 spans
//! y=1.908..2.102 m. This small authoring repair adds a continuous bearing lip at their joint,
//! not a disk across the occupied compartment. Original source faces and assets are preserved.
use crate::gltf_load::{CpuMesh,Vertex};
use glam::Vec3;
const INNER:f32=1.17;
const OUTER:f32=1.285;
const LOW:f32=1.91;
const HIGH:f32=2.055;
const CENTRE_Z:f32=0.017;
const SEGMENTS:usize=128;
fn annulus(sample:Vertex)->(Vec<Vertex>,Vec<u32>) {
    let mut vertices=Vec::new();let mut indices=Vec::new();
    for k in 0..SEGMENTS {
        let a=k as f32*std::f32::consts::TAU/SEGMENTS as f32;
        let b=(k+1) as f32*std::f32::consts::TAU/SEGMENTS as f32;
        let radial=Vec3::new(((a+b)*0.5).cos(),0.0,((a+b)*0.5).sin());
        let p=|r:f32,y:f32,t:f32|Vec3::new(t.cos()*r,y,CENTRE_Z+t.sin()*r);
        for (corners,normal) in [
            ([p(INNER,LOW,a),p(INNER,LOW,b),p(INNER,HIGH,b),p(INNER,HIGH,a)],-radial),
            ([p(OUTER,LOW,a),p(OUTER,LOW,b),p(OUTER,HIGH,b),p(OUTER,HIGH,a)],radial),
            ([p(INNER,HIGH,a),p(INNER,HIGH,b),p(OUTER,HIGH,b),p(OUTER,HIGH,a)],Vec3::Y),
            ([p(INNER,LOW,a),p(INNER,LOW,b),p(OUTER,LOW,b),p(OUTER,LOW,a)],-Vec3::Y),
        ] {
            let base=vertices.len() as u32;
            for point in corners {let mut v=sample;v.pos=point.to_array();v.normal=normal.to_array();vertices.push(v);}
            let winding=(corners[1]-corners[0]).cross(corners[2]-corners[0]).dot(normal);
            for tri in if winding>0.0 {[[0,1,2],[0,2,3]]}else{[[0,2,1],[0,3,2]]} {
                indices.extend(tri.map(|i|base+i));
            }
        }
    }
    (vertices,indices)
}
pub(super) fn close_bearing_joint(mut hull:CpuMesh)->CpuMesh {
    let at=Vec3::new(OUTER,2.0,CENTRE_Z);
    let sample=hull.vertices.iter().min_by(|a,b|Vec3::from(a.pos).distance_squared(at).total_cmp(&Vec3::from(b.pos).distance_squared(at))).copied();
    if let Some(sample)=sample {
        let (vertices,indices)=annulus(sample);let base=hull.vertices.len() as u32;
        hull.vertices.extend(vertices);hull.indices.extend(indices.into_iter().map(|i|base+i));
    }
    hull
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn ring_seal_blocks_radial_light_and_keeps_compartment_open(){
        let (v,i)=annulus(Vertex{pos:[0.;3],normal:[0.;3],uv:[0.;2],tangent:[1.,0.,0.,1.]});
        let hit=|origin:Vec3,dir:Vec3|i.chunks_exact(3).any(|tri|{
            let [a,b,c]=[0,1,2].map(|k|Vec3::from(v[tri[k] as usize].pos));
            let e=b-a;let f=c-a;let p=dir.cross(f);let det=e.dot(p);if det.abs()<1e-8{return false;}
            let s=origin-a;let u=s.dot(p)/det;let q=s.cross(e);let w=dir.dot(q)/det;let t=f.dot(q)/det;
            u>=-1e-5&&w>=-1e-5&&u+w<=1.00001&&t>0.0
        });
        for k in 0..360 {let a=(k as f32+0.37).to_radians();assert!(hit(Vec3::new(0.,2.,CENTRE_Z),Vec3::new(a.cos(),0.,a.sin())));}
        for x in [-0.9,0.0,0.9] {assert!(!hit(Vec3::new(x,1.0,CENTRE_Z),Vec3::Y));}
        assert!(hit(Vec3::new(1.23,1.0,CENTRE_Z),Vec3::Y));
    }
}
