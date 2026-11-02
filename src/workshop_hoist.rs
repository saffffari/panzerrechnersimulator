//! Authored 2.5 t service tool, NOT a certified structure or historical reconstruction.
//! Kinematic manual chain hoist: no rope elasticity, motor, hidden support, or automatic repair.
//! Register these same visible triangles for collision. The caller must verify the source engine's
//! access/extraction corridor, grounded feet, attachment eye and every candidate movement.
use anyhow::{ensure, Result};
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

pub const RATED_LOAD_KG: f32 = 2_500.0; // Authored tool specification, not a stress calculation.
pub const HOOK_MIN_Y: f32 = 1.70;
pub const HOOK_MAX_Y: f32 = 3.80;
const BLOCK_Y: f32 = 3.91;
const TROLLEY_LIMIT: f32 = 1.80;
const STEEL: [u8;3] = [83,88,77];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoistLoad {
    /// Reference to the existing finite component; does not create another inventory object.
    pub source_item: u64,
    pub mass_kg: f32,
    pub source_to_hook: [f32;16],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hoist {
    /// Rigid camp-local to world frame; local y=0 is the supporting ground plane.
    pub world_from_local: [f32;16],
    pub trolley_x: f32,
    /// Top of the visible lower hook block, in metres above that ground plane.
    pub hook_y: f32,
    pub load: Option<HoistLoad>,
    pub chain_travel_m: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoistPart { Support, Trolley, Chain, Hook }
pub struct HoistMesh {
    pub part: HoistPart,
    pub mesh: CpuMesh,
    pub world_from_mesh: Mat4,
}

fn rigid(m: Mat4)->bool {
    let (s,q,p)=m.to_scale_rotation_translation();
    m.is_finite()&&p.is_finite()&&q.is_finite()&&(s-Vec3::ONE).abs().max_element()<1e-4
        &&(m.determinant()-1.0).abs()<1e-4
        &&m.x_axis.dot(m.y_axis).abs()<1e-4&&m.x_axis.dot(m.z_axis).abs()<1e-4
        &&m.y_axis.dot(m.z_axis).abs()<1e-4
        &&m.x_axis.w.abs()+m.y_axis.w.abs()+m.z_axis.w.abs()+(m.w_axis.w-1.0).abs()<1e-5
}
impl Hoist {
    pub fn new(world_from_local: Mat4)->Result<Self> {
        let h=Self {world_from_local:world_from_local.to_cols_array(),trolley_x:0.0,
            hook_y:2.10,load:None,chain_travel_m:0.0};h.validate()?;Ok(h)
    }
    pub fn validate(&self)->Result<()> {
        ensure!(rigid(Mat4::from_cols_array(&self.world_from_local)),"invalid hoist camp frame");
        ensure!(self.trolley_x.is_finite()&&self.trolley_x.abs()<=TROLLEY_LIMIT
            &&self.hook_y.is_finite()&&(HOOK_MIN_Y..=HOOK_MAX_Y).contains(&self.hook_y)
            &&self.chain_travel_m.is_finite()&&self.chain_travel_m>=0.0,"invalid hoist travel");
        if let Some(load)=&self.load {
            ensure!(load.source_item!=0&&load.mass_kg.is_finite()&&load.mass_kg>0.0
                &&load.mass_kg<=RATED_LOAD_KG&&rigid(Mat4::from_cols_array(&load.source_to_hook)),
                "invalid or overloaded hoist");
        }Ok(())
    }
    pub fn hook_world(&self)->Mat4 { Mat4::from_cols_array(&self.world_from_local)
        *Mat4::from_translation(Vec3::new(self.trolley_x,self.hook_y,0.0)) }
    pub fn hook_seat_world(&self)->Vec3 {self.hook_world().transform_point3(Vec3::new(0.0,-0.18,0.0))}
    pub fn chain_length_m(&self)->f32 {BLOCK_Y-self.hook_y}
    pub fn load_world(&self)->Option<Mat4> {self.load.as_ref().map(|l|
        self.hook_world()*Mat4::from_cols_array(&l.source_to_hook))}
    /// Caller supplies the actual registered source lifting eye, never an invented centroid.
    pub fn attach(&mut self,item:u64,kg:f32,source_world:Mat4,lift_eye:Vec3)->Result<()> {
        self.validate()?;ensure!(self.load.is_none()&&rigid(source_world),"hoist occupied or invalid source pose");
        ensure!(lift_eye.is_finite()&&source_world.transform_point3(lift_eye)
            .distance(self.hook_seat_world())<=0.04,"lifting eye is not at hook seat");
        let mut candidate=self.clone();candidate.load=Some(HoistLoad {source_item:item,mass_kg:kg,
            source_to_hook:(self.hook_world().inverse()*source_world).to_cols_array()});
        candidate.validate()?;*self=candidate;Ok(())
    }
    /// No drop/teleport: caller proves that the existing assembly has acquired physical support.
    pub fn detach(&mut self,physically_supported:bool)->Result<Option<HoistLoad>> {
        self.validate()?;ensure!(physically_supported,"load must be supported before unhooking");Ok(self.load.take())
    }
    /// At most 10 mm lift / 8 mm traverse per call. Candidate must pass actual swept source contact
    /// tests in the adapter; a blocked motion consumes no progress. No mass-dependent magic speed.
    pub fn step(&mut self,lift:f32,traverse:f32,dt:f32,clear:impl FnOnce(&Self,&Self)->bool)->Result<bool> {
        self.validate()?;ensure!(lift.is_finite()&&traverse.is_finite()&&dt.is_finite()
            &&(0.0..=0.1).contains(&dt),"invalid manual hoist input/step");
        let mut next=self.clone();next.hook_y=(next.hook_y+lift.clamp(-1.0,1.0)*0.10*dt).clamp(HOOK_MIN_Y,HOOK_MAX_Y);
        next.trolley_x=(next.trolley_x+traverse.clamp(-1.0,1.0)*0.08*dt).clamp(-TROLLEY_LIMIT,TROLLEY_LIMIT);
        let distance=(next.hook_y-self.hook_y).abs();
        if distance==0.0&&next.trolley_x==self.trolley_x {return Ok(false)}
        if !clear(self,&next) {return Ok(false)}
        next.chain_travel_m+=distance;next.validate()?;*self=next;Ok(true)
    }
    /// Two A-frames flank the drive-through lane. Lower braces stay in the side planes.
    pub fn support_mesh()->CpuMesh {
        let mut pieces=Vec::new();
        for x in [-2.50,2.50] {
            pieces.push(block(Vec3::new(x-0.20,0.0,-1.65),Vec3::new(x+0.20,0.13,1.65)));
            for z in [-1.45,1.45] {
                pieces.push(bar(Vec3::new(x,0.13,z),Vec3::new(x,4.28,0.0),0.18));
                pieces.push(bar(Vec3::new(x,1.15,z*0.76),Vec3::new(x,2.65,-z*0.39),0.075));
            }
        }
        // Visible I-beam: lower flange carries the trolley wheels, web and upper flange carry load.
        pieces.push(block(Vec3::new(-2.70,4.22,-0.15),Vec3::new(2.70,4.27,0.15)));
        pieces.push(block(Vec3::new(-2.70,4.27,-0.025),Vec3::new(2.70,4.48,0.025)));
        pieces.push(block(Vec3::new(-2.70,4.48,-0.15),Vec3::new(2.70,4.53,0.15)));
        merge(pieces)
    }
    pub fn trolley_mesh()->CpuMesh {
        let mut p=vec![block(Vec3::new(-0.14,3.91,-0.12),Vec3::new(0.14,4.14,0.12)),
            block(Vec3::new(-0.17,4.13,-0.21),Vec3::new(0.17,4.18,0.21))];
        for z in [-0.19,0.19] {
            p.push(block(Vec3::new(-0.19,4.15,z-0.025),Vec3::new(0.19,4.37,z+0.025)));
            for x in [-0.115,0.115] {p.push(rod(Vec3::new(x,4.33,z.signum()*0.055),
                Vec3::new(x,4.33,z),0.06));}
        }merge(p)
    }
    pub fn hook_mesh()->CpuMesh {
        let mut p=vec![block(Vec3::new(-0.11,-0.045,-0.055),Vec3::new(0.11,0.015,0.055))];
        let path=[(0.0,-0.035),(0.0,-0.10),(-0.055,-0.15),(-0.055,-0.215),
            (0.0,-0.245),(0.065,-0.205),(0.075,-0.135)];
        for pair in path.windows(2) {p.push(rod(Vec3::new(pair[0].0,pair[0].1,0.0),
            Vec3::new(pair[1].0,pair[1].1,0.0),0.025));}merge(p)
    }
    pub fn parts(&self)->Result<Vec<HoistMesh>> {
        self.validate()?;let world=Mat4::from_cols_array(&self.world_from_local);
        let trolley=world*Mat4::from_translation(Vec3::X*self.trolley_x);
        let chain=merge([-0.07,0.07].into_iter().map(|x|rod(Vec3::new(x,self.hook_y,0.0),
            Vec3::new(x,BLOCK_Y,0.0),0.012)).collect());
        Ok(vec![HoistMesh{part:HoistPart::Support,mesh:Self::support_mesh(),world_from_mesh:world},
            HoistMesh{part:HoistPart::Trolley,mesh:Self::trolley_mesh(),world_from_mesh:trolley},
            HoistMesh{part:HoistPart::Chain,mesh:chain,world_from_mesh:trolley},
            HoistMesh{part:HoistPart::Hook,mesh:Self::hook_mesh(),world_from_mesh:self.hook_world()}])
    }
}
impl HoistMesh {
    /// Collision triangles are derived from the exact visible mesh and its same rigid pose.
    pub fn triangles(&self)->Vec<[Vec3;3]> {self.mesh.indices.chunks_exact(3).map(|i|
        [i[0],i[1],i[2]].map(|j|self.world_from_mesh.transform_point3(Vec3::from_array(self.mesh.vertices[j as usize].pos)))).collect()}
}
fn block(min:Vec3,max:Vec3)->CpuMesh {crate::prims::box_mesh(min,max,STEEL)}
fn bar(a:Vec3,b:Vec3,width:f32)->CpuMesh {
    let d=b-a;block(Vec3::new(-width*0.5,0.0,-width*0.5),Vec3::new(width*0.5,d.length(),width*0.5))
        .transformed(Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y,d.normalize()),a))
}
/// Closed octagonal steel cylinder. Its eight sides are the actual geometry, not a collision proxy.
fn rod(a:Vec3,b:Vec3,r:f32)->CpuMesh {
    let mut p=Vec::new();let axis=(b-a).normalize();let u=axis.any_orthonormal_vector();let v=axis.cross(u);
    for i in 0..8 {let angle=i as f32*std::f32::consts::TAU/8.0;let next=angle+std::f32::consts::TAU/8.0;
        let d=(u*angle.cos()+v*angle.sin())*r;let e=(u*next.cos()+v*next.sin())*r;
        p.extend([[a+d,a+e,b+e],[a+d,b+e,b+d],[a,a+e,a+d],[b,b+d,b+e]]);}
    let mut vertices=Vec::new();for t in p {let n=(t[1]-t[0]).cross(t[2]-t[0]).normalize();let tangent=n.any_orthonormal_vector();
        for pos in t {vertices.push(crate::gltf_load::Vertex{pos:pos.to_array(),normal:n.to_array(),uv:[0.0,0.0],
            tangent:[tangent.x,tangent.y,tangent.z,1.0]});}}
    let min=vertices.iter().map(|v|Vec3::from_array(v.pos)).fold(Vec3::splat(f32::INFINITY),Vec3::min);
    let max=vertices.iter().map(|v|Vec3::from_array(v.pos)).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max);
    let indices=(0..vertices.len() as u32).collect();CpuMesh::basic(vertices,indices,Some(crate::prims::solid_color(STEEL)),min,max)
}
fn merge(parts:Vec<CpuMesh>)->CpuMesh {
    let mut mesh=CpuMesh::basic(Vec::new(),Vec::new(),Some(crate::prims::solid_color(STEEL)),
        Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY));
    for p in parts {let offset=mesh.vertices.len() as u32;mesh.min=mesh.min.min(p.min);mesh.max=mesh.max.max(p.max);
        mesh.indices.extend(p.indices.into_iter().map(|i|i+offset));mesh.vertices.extend(p.vertices);}
    mesh.metallic_factor=0.8;mesh.roughness_factor=0.72;mesh
}

#[cfg(test)] mod tests {
    use super::*;
    fn intersects(t:[Vec3;3],min:Vec3,max:Vec3)->bool {
        let mut poly=t.to_vec();for axis in 0..3 {for (edge,sign) in [(min[axis],1.0),(max[axis],-1.0)] {
            let mut out=Vec::new();for i in 0..poly.len() {let a=poly[i];let b=poly[(i+1)%poly.len()];
                let da=(a[axis]-edge)*sign;let db=(b[axis]-edge)*sign;
                if da>=0.0 {out.push(a)}if (da>=0.0)!=(db>=0.0) {out.push(a+(b-a)*(da/(da-db)));}}
            poly=out;if poly.is_empty(){return false}
        }}true
    }
    #[test] fn source_engine_fits_portal_and_clears_deck_at_full_lift() {
        // Measured Hull_0971 source bounds, centred longitudinally under the gantry.
        let min=Vec3::new(-0.48650,0.58235,-0.65110);let max=Vec3::new(0.56136,1.91730,0.65110);
        let mut h=Hoist::new(Mat4::IDENTITY).unwrap();h.hook_y=HOOK_MAX_Y;
        let lift=h.hook_seat_world().y-max.y;
        assert!(min.y+lift>2.05352+0.10,"engine must lift above measured Hull_0975 cover");
        for x in [-1.8,0.0,1.8] {for i in 0..=32 {let delta=Vec3::new(x,lift*i as f32/32.0,0.0);
            for part in h.parts().unwrap().into_iter().filter(|p|p.part==HoistPart::Support) {
                assert!(!part.triangles().into_iter().any(|t|intersects(t,min+delta,max+delta)),"engine hits portal");
            }
        }}
    }
    #[test] fn rated_attachment_motion_rejection_and_save_preserve_real_load() {
        let mut h=Hoist::new(Mat4::from_rotation_y(0.7)*Mat4::from_translation(Vec3::new(4.0,0.0,6.0))).unwrap();
        let source=Mat4::from_translation(h.hook_seat_world());let before=h.clone();
        assert!(h.attach(7,2500.1,source,Vec3::ZERO).is_err());assert_eq!(h,before);
        h.attach(7,1200.0,source,Vec3::ZERO).unwrap();let before=h.clone();
        assert!(!h.step(1.0,1.0,0.1,|_,_|false).unwrap());assert_eq!(h,before);
        assert!(h.detach(false).is_err());assert!(h.step(1.0,0.0,0.1,|_,_|true).unwrap());
        assert!((h.load_world().unwrap().w_axis.y-source.w_axis.y-0.01).abs()<1e-5);
        let bytes=bincode::serialize(&h).unwrap();let restored:Hoist=bincode::deserialize(&bytes).unwrap();
        restored.validate().unwrap();assert_eq!(h,restored);assert_eq!(h.load.as_ref().unwrap().source_item,7);
    }
    #[test] fn footprint_and_chain_are_visible_contact_geometry() {
        let mut h=Hoist::new(Mat4::IDENTITY).unwrap();let mesh=Hoist::support_mesh();
        let ground:Vec<_>=mesh.vertices.iter().map(|v|Vec3::from_array(v.pos)).filter(|p|p.y==0.0).collect();
        assert!(ground.iter().any(|p|p.x< -2.6&&p.z< -1.6));assert!(ground.iter().any(|p|p.x>2.6&&p.z>1.6));
        for height in [HOOK_MIN_Y,HOOK_MAX_Y] {h.hook_y=height;let chain=h.parts().unwrap().into_iter().find(|p|p.part==HoistPart::Chain).unwrap();
            assert!((chain.mesh.min.y-height).abs()<1e-6&&(chain.mesh.max.y-BLOCK_Y).abs()<1e-6);
            assert!((chain.mesh.max.y-chain.mesh.min.y-h.chain_length_m()).abs()<1e-6);
            let triangles=chain.triangles();
            let area:f64=triangles.iter().map(|t| {
                let p=t.map(|p|p.as_dvec3());
                let area=(p[1]-p[0]).cross(p[2]-p[0]).length()*0.5;
                assert!(area>0.0,"chain must have no degenerate contact triangles");area
            }).sum();
            // Independent regular-octagon formula for two closed, 12 mm-radius rods:
            // lateral area = perimeter*length, each end = n*r²*sin(2π/n)/2.
            // In particular the short 110 mm chain at maximum lift has only ~0.01779 m².
            let radius=0.012_f64;
            let perimeter=16.0*radius*(std::f64::consts::PI/8.0).sin();
            let end_area=4.0*radius*radius*(std::f64::consts::PI/4.0).sin();
            let length=f64::from(BLOCK_Y)-f64::from(height);
            let expected_area=2.0*(perimeter*length+2.0*end_area);
            assert!((area-expected_area).abs()<expected_area*2e-6,
                "visible chain area {area} must match closed-octagon area {expected_area}");
            assert_eq!(chain.mesh.indices.len(),192);
            // Rendering duplicates vertices for flat normals. Weld only for this topology
            // audit, at 1 μm (well below the rod radius), and require two closed surfaces.
            let mut edges=std::collections::BTreeMap::new();
            for triangle in triangles {
                let points=triangle.map(|p|p.to_array().map(|v|(f64::from(v)*1e6).round() as i64));
                for i in 0..3 {let a=points[i];let b=points[(i+1)%3];
                    assert_ne!(a,b);let edge=if a<b {(a,b)} else {(b,a)};
                    *edges.entry(edge).or_insert(0usize)+=1;
                }
            }
            assert_eq!(edges.len(),96,"two closed eight-sided rods have 96 triangulated edges");
            assert!(edges.values().all(|&uses|uses==2),"every chain edge must have two incident faces");
        }
    }
}
