//! Retained visible source geometry for the opt-in transport driving experiment. AABB tests
//! reject distant work only; every accepted obstacle contact is a triangle/segment contact.
use super::*;
use crate::renderer::{InstanceRef, SceneRef};

pub(super) type Triangle = [Vec3; 3];
const TRIANGLE_QUERY_LIMIT: usize = 32_768;

struct Prototype {
    triangles: Vec<Triangle>,
    min: Vec3,
    max: Vec3,
}
struct Instance { prototype: usize, handle: SceneRef }

#[derive(Default)]
pub(crate) struct WorldSources {
    prototypes: Vec<Prototype>,
    instances: Vec<Instance>,
}

pub(super) fn triangles(mesh: &CpuMesh) -> Vec<Triangle> {
    mesh.indices.chunks_exact(3).map(|i| [i[0],i[1],i[2]].map(|j| Vec3::from_array(mesh.vertices[j as usize].pos))).collect()
}

pub(super) fn bounds(t: Triangle) -> (Vec3, Vec3) {
    (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]))
}
fn overlap(a: (Vec3,Vec3), b: (Vec3,Vec3)) -> bool {
    a.0.cmple(b.1 + Vec3::splat(0.0001)).all() && b.0.cmple(a.1 + Vec3::splat(0.0001)).all()
}

/// Double-sided, finite segment against the actual source triangle. Coplanar segments are
/// ignored here; translated source edges and vertical foot probes cover their crossing cases.
pub(super) fn segment(from:Vec3,to:Vec3,t:Triangle)->Option<(f32,Vec3)> {
    let d=to-from; let e1=t[1]-t[0]; let e2=t[2]-t[0];
    let h=d.cross(e2); let det=e1.dot(h);
    if det.abs()<1e-9 { return None; }
    let inv=1.0/det; let s=from-t[0]; let u=inv*s.dot(h);
    if !(-0.00001..=1.00001).contains(&u) {return None;}
    let q=s.cross(e1); let v=inv*d.dot(q);
    if v < -0.00001 || u+v > 1.00001 {return None;}
    let at=inv*e2.dot(q);
    if !(0.00001..=1.0).contains(&at) {return None;}
    let mut normal=e1.cross(e2).normalize_or_zero();
    if normal.dot(d)>0.0 {normal=-normal;}
    Some((at,normal))
}

impl WorldSources {
    /// A fluid surface is rendered geometry but is not a rigid obstacle.
    pub(crate) fn exclude_loose(&mut self,slot:usize){
        self.instances.retain(|i|!matches!(i.handle,SceneRef::Mesh(index) if index==slot));
    }
    pub(crate) fn add_prototype(&mut self,mesh:&CpuMesh)->usize {
        let source=triangles(mesh);
        if let Some(index)=self.prototypes.iter().position(|p|p.min==mesh.min&&p.max==mesh.max&&p.triangles==source) {return index;}
        let index=self.prototypes.len();
        self.prototypes.push(Prototype{triangles:source,min:mesh.min,max:mesh.max});
        index
    }
    pub(crate) fn add_instance(&mut self,prototype:usize,handle:SceneRef) {
        self.instances.push(Instance{prototype,handle});
    }
    pub(crate) fn add_loose(&mut self,scene:&[CpuMesh],range:std::ops::Range<usize>) {
        for index in range {let proto=self.add_prototype(&scene[index]);self.add_instance(proto,SceneRef::Mesh(index));}
    }
    pub(crate) fn add_pages(&mut self,mesh:&CpuMesh,pages:&[usize],counts:impl Iterator<Item=usize>) {
        let proto=self.add_prototype(mesh);
        for (&batch,count) in pages.iter().zip(counts) {
            for instance in 0..count {
                self.add_instance(proto,SceneRef::Instance(InstanceRef{batch,instance:instance as u32}));
            }
        }
    }
    /// Hidden intact modules cease to collide and moved rubble uses its current visible pose.
    /// Query saturation refuses movement; missing geometry never silently becomes free space.
    pub(crate) fn nearby(&self,r:&Renderer,lo:Vec3,hi:Vec3,out:&mut Vec<Triangle>)->bool {
        self.nearby_excluding(r,lo,hi,out,&[])
    }
    /// A moving source assembly must not collide with its own loose mesh registrations.
    /// Exclusions are exact retained source slots; other world/instance geometry is unchanged.
    pub(crate) fn nearby_excluding(&self,r:&Renderer,lo:Vec3,hi:Vec3,out:&mut Vec<Triangle>,excluded:&[usize])->bool {
        out.clear();
        for instance in &self.instances {
            if matches!(instance.handle,SceneRef::Mesh(slot) if excluded.contains(&slot)){continue;}
            let Some((visible,model))=r.source_ref_state(instance.handle) else {return false;};
            if !visible {continue;}
            let proto=&self.prototypes[instance.prototype];
            if !overlap((lo,hi),crate::building_render::transformed_bounds(model,proto.min,proto.max)) {continue;}
            for tri in &proto.triangles {
                let world=tri.map(|p|model.transform_point3(p));
                if overlap((lo,hi),bounds(world)) {
                    if out.len()==TRIANGLE_QUERY_LIMIT {out.clear();return false;}
                    out.push(world);
                }
            }
        }
        true
    }
    pub(super) fn counts(&self)->(usize,usize,usize) {
        (self.prototypes.len(),self.instances.len(),self.prototypes.iter().map(|p|p.triangles.len()).sum())
    }
}

/// Bounded kinematic sweep: source vertices trace their actual motion; source edges at the
/// destination also catch a small obstacle whose vertex lies between vehicle vertices.
pub(super) fn motion_blocked(source:&[Triangle],before:Mat4,after:Mat4,world:&[Triangle])->bool {
    for local in source {
        let a=local.map(|p|before.transform_point3(p));
        let b=local.map(|p|after.transform_point3(p));
        let (alo,ahi)=bounds(a);let (blo,bhi)=bounds(b);
        for &obstacle in world {
            if !overlap((alo.min(blo),ahi.max(bhi)),bounds(obstacle)) {continue;}
            // Ground contacts are supported separately. Avoid coplanar contact chatter on a
            // source floor already underneath all corners, while still rejecting low walls.
            let n=(obstacle[1]-obstacle[0]).cross(obstacle[2]-obstacle[0]).normalize_or_zero();
            if n.y.abs()>0.85 && b.iter().all(|p|p.y>=obstacle.iter().map(|q|q.y).fold(f32::NEG_INFINITY,f32::max)-0.025) {continue;}
            for i in 0..3 {
                if segment(a[i],b[i],obstacle).is_some() {return true;}
                // Do not turn an existing resting/touching overlap into a new blockade.
                if segment(obstacle[i],obstacle[(i+1)%3],b).is_some()
                    && segment(obstacle[i],obstacle[(i+1)%3],a).is_none() {return true;}
                if segment(b[i],b[(i+1)%3],obstacle).is_some()
                    && segment(a[i],a[(i+1)%3],obstacle).is_none() {return true;}
            }
        }
    }
    false
}

pub(super) fn first_contact(from:Vec3,to:Vec3,triangles:&[Triangle])->Option<(Vec3,Vec3)> {
    triangles.iter().filter_map(|&t|segment(from,to,t))
        .min_by(|a,b|a.0.total_cmp(&b.0)).map(|(at,n)|(from.lerp(to,at),n))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn swept_source_contact_stops_at_wall_but_not_its_empty_bounds() {
        let wall=[Vec3::new(0.,0.,0.),Vec3::new(0.,2.,0.),Vec3::new(0.,0.,2.)];
        assert!(segment(Vec3::new(-1.,0.5,0.5),Vec3::new(1.,0.5,0.5),wall).is_some());
        assert!(segment(Vec3::new(-1.,1.8,1.8),Vec3::new(1.,1.8,1.8),wall).is_none());
        let vehicle=[[Vec3::new(-0.2,0.2,0.2),Vec3::new(-0.2,0.4,0.2),Vec3::new(-0.2,0.2,0.4)]];
        assert!(motion_blocked(&vehicle,Mat4::IDENTITY,Mat4::from_translation(Vec3::X*0.4),&[wall]));
        assert!(!motion_blocked(&vehicle,Mat4::IDENTITY,Mat4::from_translation(Vec3::NEG_X),&[wall]));
    }
}
