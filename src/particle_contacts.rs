//! Transient first-surface queries against the geometry actually drawn for crew.
//! Living figures contribute posed triangles; damaged figures and loose pieces contribute
//! surviving voxel cubes. Bounds only accelerate the query, never stand in for the surface.
use glam::{Mat4, Quat, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Hit { pub key: u64, pub fraction: f32, pub point: Vec3, pub normal: Vec3, pub local_stamp: Mat4 }
#[derive(Clone, Copy)]
enum Shape { Triangle, Cube }
struct Primitive { key:u64, model:Mat4, inverse:Mat4, shape:Shape, lo:Vec3, hi:Vec3 }
struct Node { lo:Vec3, hi:Vec3, range:std::ops::Range<usize>, children:Option<(usize,usize)> }
#[derive(Default)]
pub struct Scene { primitives:Vec<Primitive>, nodes:Vec<Node>, lookup:HashMap<u64,usize> }
pub fn key(actor:u32, kind:u8, element:u32)->u64 { ((actor as u64)<<32)|((kind as u64)<<28)|element as u64 }
fn box_interval(a:Vec3,b:Vec3,lo:Vec3,hi:Vec3)->Option<(f32,Vec3)> {
    let d=b-a;let mut near=0.0f32;let mut far=1.0f32;let mut normal=Vec3::ZERO;
    for axis in 0..3 {
        if d[axis].abs()<1e-12 {if a[axis]<lo[axis]||a[axis]>hi[axis]{return None;}continue;}
        let mut t0=(lo[axis]-a[axis])/d[axis];let mut t1=(hi[axis]-a[axis])/d[axis];
        let mut n=Vec3::ZERO;n[axis]=-d[axis].signum();if t0>t1 {std::mem::swap(&mut t0,&mut t1);}
        if t0>near {near=t0;normal=n;}far=far.min(t1);if near>far{return None;}
    }Some((near,normal))
}
impl Scene {
    pub fn triangle(&mut self,key:u64,t:[Vec3;3]) {
        let x=t[1]-t[0];let y=t[2]-t[0];let n=x.cross(y).normalize_or_zero();
        if n.length_squared()<0.5{return;}
        let model=Mat4::from_cols(x.extend(0.),y.extend(0.),n.extend(0.),t[0].extend(1.));
        self.primitives.push(Primitive{key,model,inverse:model.inverse(),shape:Shape::Triangle,
            lo:t[0].min(t[1]).min(t[2])-Vec3::splat(1e-6),hi:t[0].max(t[1]).max(t[2])+Vec3::splat(1e-6)});
    }
    pub fn cube(&mut self,key:u64,model:Mat4) {
        let extent=(model.x_axis.truncate().abs()+model.y_axis.truncate().abs()+model.z_axis.truncate().abs())*0.5;
        let p=model.w_axis.truncate();
        self.primitives.push(Primitive{key,model,inverse:model.inverse(),shape:Shape::Cube,lo:p-extent,hi:p+extent});
    }
    pub fn finish(&mut self) {
        self.nodes.clear();self.lookup.clear();if !self.primitives.is_empty(){self.node(0..self.primitives.len());}
        for (i,p) in self.primitives.iter().enumerate(){self.lookup.insert(p.key,i);}
    }
    fn node(&mut self,range:std::ops::Range<usize>)->usize {
        let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
        for p in &self.primitives[range.clone()]{lo=lo.min(p.lo);hi=hi.max(p.hi);}
        let i=self.nodes.len();self.nodes.push(Node{lo,hi,range:range.clone(),children:None});
        if range.len()>8 {
            let span=hi-lo;let axis=if span.x>span.y&&span.x>span.z{0}else if span.y>span.z{1}else{2};
            let mid=range.start+range.len()/2;
            self.primitives[range.clone()].select_nth_unstable_by(range.len()/2,|a,b|(a.lo[axis]+a.hi[axis]).total_cmp(&(b.lo[axis]+b.hi[axis])));
            let a=self.node(range.start..mid);let b=self.node(mid..range.end);self.nodes[i].children=Some((a,b));
        }i
    }
    pub fn model(&self,key:u64)->Option<Mat4>{self.lookup.get(&key).map(|&i|self.primitives[i].model)}
    pub fn sweep_batch(&self,paths:&[[Vec3;2]])->Vec<Option<Hit>> {
        let mut hits=vec![None;paths.len()];
        let trace=|out:&mut [Option<Hit>],paths:&[[Vec3;2]]| {
            for (out,p) in out.iter_mut().zip(paths){*out=self.sweep(p[0],p[1]);}
        };
        if paths.len()<1024 {trace(&mut hits,paths);}else{
            let chunk=paths.len().div_ceil(4);
            std::thread::scope(|scope|{for (out,paths) in hits.chunks_mut(chunk).zip(paths.chunks(chunk)) {
                let trace=&trace;scope.spawn(move ||trace(out,paths));
            }});
        }
        hits
    }
    pub fn sweep(&self,from:Vec3,to:Vec3)->Option<Hit> {
        if self.nodes.is_empty(){return None;}
        // Median-split BVH depth is bounded by usize::BITS; avoid a heap allocation per grain.
        let mut stack=[0usize;usize::BITS as usize+1];let mut depth=1;let mut best:Option<Hit>=None;
        while depth>0 {depth-=1;let i=stack[depth];let node=&self.nodes[i];
            let Some((near,_))=box_interval(from,to,node.lo,node.hi)else{continue;};
            if best.is_some_and(|h|near>h.fraction){continue;}
            if let Some((a,b))=node.children{stack[depth]=a;stack[depth+1]=b;depth+=2;continue;}
            for p in &self.primitives[node.range.clone()] {
                let a=p.inverse.transform_point3(from);let b=p.inverse.transform_point3(to);
                let contact=match p.shape {
                    Shape::Triangle=>{let dz=b.z-a.z;if dz.abs()<1e-12{None}else{let t=-a.z/dz;let q=a.lerp(b,t);
                        ((0.0..=1.0).contains(&t)&&q.x>=-1e-6&&q.y>=-1e-6&&q.x+q.y<=1.0+1e-6).then_some((t,Vec3::Z*(-dz.signum()))) }},
                    // A droplet born inside a wound can leave its source cell. It still collides
                    // with every subsequent occupied cell; empty wound voxels contribute nothing.
                    Shape::Cube=>if a.abs().cmplt(Vec3::splat(0.5-1e-6)).all(){None}else{box_interval(a,b,Vec3::splat(-0.5),Vec3::splat(0.5))},
                };
                let Some((fraction,n))=contact else{continue;};if n.length_squared()<0.5||best.is_some_and(|h|h.fraction<=fraction){continue;}
                let normal=p.inverse.transpose().transform_vector3(n).normalize_or_zero();let point=from.lerp(to,fraction);
                let stamp=Mat4::from_scale_rotation_translation(Vec3::new(0.009,0.00035,0.009),Quat::from_rotation_arc(Vec3::Y,normal),point+normal*0.00025);
                best=Some(Hit{key:p.key,fraction,point,normal,local_stamp:p.inverse*stamp});
            }
        }best
    }
}

pub struct Query<'a> { pub tank:super::surface_fire::SourceContacts<'a>, pub crew:&'a Scene }
impl super::surface_blood::SourceQuery for Query<'_> {
    fn sweep_batch(&self,paths:&[[Vec3;2]],_:&[Mat4;super::surface_fire::FRAMES])->Vec<Option<super::surface_fire::Contact>>{self.tank.sweep_batch(paths)}
    fn supports_batch(&self,frame:usize,paths:&[[Vec3;2]])->Vec<bool>{self.tank.supports_batch(frame,paths)}
    fn supports_cell(&self,frame:usize,corners:&[Vec3;4])->bool{self.tank.supports_cell(frame,corners)}
    fn dynamic_sweep(&self,a:Vec3,b:Vec3)->Option<Hit>{self.crew.sweep(a,b)}
    fn dynamic_sweep_batch(&self,paths:&[[Vec3;2]])->Vec<Option<Hit>>{self.crew.sweep_batch(paths)}
    fn dynamic_model(&self,key:u64)->Option<Mat4>{self.crew.model(key)}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test]fn batched_contacts_preserve_nearest_hits_and_order(){
        let mut s=Scene::default();
        for i in 0..48 {s.cube(i,Mat4::from_translation(Vec3::new(i as f32*0.7,0.,0.)));}s.finish();
        let paths:Vec<_>=(0..4096).map(|i|{let x=(i%600) as f32*0.1;[Vec3::new(x,0.,-2.),Vec3::new(x,0.,2.)]}).collect();
        let batch=s.sweep_batch(&paths);
        for (p,hit) in paths.iter().zip(batch){let direct=s.sweep(p[0],p[1]);assert_eq!(hit.map(|h|(h.key,h.fraction)),direct.map(|h|(h.key,h.fraction)));}
    }
    #[test]fn nearest_mesh_wins_and_empty_bounds_do_not_block(){let mut s=Scene::default();
        s.triangle(2,[Vec3::new(0.,0.,2.),Vec3::new(1.,0.,2.),Vec3::new(0.,1.,2.)]);
        s.triangle(1,[Vec3::ZERO,Vec3::X,Vec3::Y]);s.finish();
        assert_eq!(s.sweep(Vec3::new(0.2,0.2,-5.),Vec3::new(0.2,0.2,5.)).unwrap().key,1);
        assert!(s.sweep(Vec3::new(0.9,0.9,-5.),Vec3::new(0.9,0.9,5.)).is_none());}
    #[test]fn rotated_voxel_and_removed_wound_are_exact(){let mut s=Scene::default();
        s.cube(3,Mat4::from_scale_rotation_translation(Vec3::splat(0.01),Quat::from_rotation_y(0.7),Vec3::ZERO));s.finish();
        let h=s.sweep(Vec3::NEG_Z,Vec3::Z).unwrap();assert!(h.point.z<0.&&h.point.z> -0.01);
        assert!(s.sweep(Vec3::new(0.02,0.,-1.),Vec3::new(0.02,0.,1.)).is_none());
        let empty=Scene::default();assert!(empty.sweep(Vec3::NEG_Z,Vec3::Z).is_none());}
    #[test]fn attached_stain_follows_its_surface(){let mut s=Scene::default();s.cube(1,Mat4::IDENTITY);s.finish();
        let h=s.sweep(Vec3::NEG_Z,Vec3::ZERO).unwrap();let before=s.model(1).unwrap()*h.local_stamp;
        let mut moved=Scene::default();moved.cube(1,Mat4::from_translation(Vec3::X));moved.finish();
        assert!(((moved.model(1).unwrap()*h.local_stamp).w_axis-before.w_axis-Vec3::X.extend(0.)).length()<1e-6);}
}
