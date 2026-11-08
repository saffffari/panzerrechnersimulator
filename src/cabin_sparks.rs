//! Cosmetic incandescent dust. Physical steel fragments retain their own damage solver.
//! Every flight is swept against the first tank/crew surface, followed by exactly one
//! rendered orange contact frame. No invisible analytic flight continues behind that surface.
use super::*;
pub const CAPACITY:usize=512;
#[derive(Clone,serde::Serialize,serde::Deserialize)]
struct Spark { pos:Vec3, vel:Vec3, size:f32, age:f32, snuff:Option<u32> }
#[derive(Default,serde::Serialize,serde::Deserialize)]
pub(super) struct Pool { sparks:Vec<Spark>, sequence:u32, pub contacts:u64, pub crew_contacts:u64 }
impl Pool {
    pub fn active(&self)->bool{!self.sparks.is_empty()}
    pub fn spawn(&mut self,point:Vec3,axis:Vec3,energy:f32){
        if !point.is_finite()||!axis.is_finite()||energy<1.0{return;}
        self.sequence=self.sequence.wrapping_add(1);
        let count=(energy.sqrt()*2.0).clamp(32.0,256.0)as usize;
        let n=axis.normalize_or_zero();
        for i in 0..count.min(CAPACITY.saturating_sub(self.sparks.len())) {
            let seed=self.sequence.wrapping_mul(7919).wrapping_add(i as u32*31);
            let q=Vec3::new(hash01(seed)*2.0-1.0,hash01(seed^0xA31)*2.0-1.0,hash01(seed^0x79)*2.0-1.0);
            self.sparks.push(Spark{pos:point+n*0.003,vel:(n+q*0.65).normalize_or_zero()*(25.0+90.0*hash01(seed^0x17)),
                size:0.001+0.0018*hash01(seed^0x71),age:0.,snuff:None});
        }
    }
    pub fn advance(&mut self,dt:f32,frame:u32,tank:&surface_fire::SourceContacts<'_>,crew:&particle_contacts::Scene){
        self.advance_with(dt,frame,|a,b|{
            let wall=tank.sweep(a,b).map(|h|(h.fraction,tank.model(h.frame).transform_point3(h.point),false));
            let person=crew.sweep(a,b).map(|h|(h.fraction,h.point,true));
            match (wall,person){(Some(a),Some(b))=>Some(if a.0<b.0{a}else{b}),(a,b)=>a.or(b)}
        });
    }
    fn advance_with(&mut self,dt:f32,frame:u32,mut contact:impl FnMut(Vec3,Vec3)->Option<(f32,Vec3,bool)>){
        let dt=dt.clamp(0.,0.05);
        self.sparks.retain(|s|s.age<0.45&&s.snuff.is_none_or(|f|f==frame));
        for s in &mut self.sparks {
            if s.snuff.is_some(){continue;}
            let end=s.pos+s.vel*dt+Vec3::NEG_Y*4.9*dt*dt;
            if let Some((_,at,crew))=contact(s.pos,end){s.pos=at;s.vel=Vec3::ZERO;s.snuff=Some(frame);
                self.contacts+=1;self.crew_contacts+=u64::from(crew);
            }else{s.pos=end;s.vel=(s.vel+Vec3::NEG_Y*9.8*dt)*(-2.0*dt).exp();}
            s.age+=dt;
        }
    }
    pub fn models(&self,r:&Renderer)->[Vec<Mat4>;3]{
        let mut out:[Vec<Mat4>;3]=std::array::from_fn(|_|Vec::new());
        for s in &self.sparks {
            let pixel_m=2.0*s.pos.distance(r.camera.pos)*(r.camera.fov_y*0.5).tan()/r.ih as f32;
            let snuff=s.snuff.is_some();let size=s.size.max(pixel_m)*(if snuff{2.0}else{1.0});
            out[usize::from(snuff)].push(Mat4::from_scale_rotation_translation(Vec3::splat(size),Quat::IDENTITY,s.pos));
        }out
    }
}
#[cfg(test)]mod tests{use super::*;
    #[test]fn actual_crew_geometry_stops_a_spark_before_a_wall(){
        let mesh=box_mesh(Vec3::new(-1.,-1.,0.004),Vec3::new(1.,1.,0.006),[80;3]);
        let tank=surface_fire::CabinFire::new(&[(0,&mesh)]);
        let models=[Mat4::IDENTITY;surface_fire::FRAMES];let source=tank.source_contacts(&models);
        for voxel in [false,true] {
            let mut crew=particle_contacts::Scene::default();
            if voxel {crew.cube(17,Mat4::from_scale_rotation_translation(Vec3::splat(0.001),Quat::IDENTITY,Vec3::Z*0.0025));}
            else {crew.triangle(17,[Vec3::new(-1.,-1.,0.002),Vec3::new(1.,-1.,0.002),Vec3::new(0.,1.,0.002)]);}
            crew.finish();let mut pool=Pool::default();
            pool.sparks.push(Spark{pos:Vec3::ZERO,vel:Vec3::Z*100.,size:0.001,age:0.,snuff:None});
            pool.advance(1./15360.,20,&source,&crew);
            assert_eq!(pool.crew_contacts,1);assert!((pool.sparks[0].pos.z-0.002).abs()<1e-6);
            pool.advance(1./15360.,21,&source,&crew);assert!(!pool.active());
        }
    }
    #[test]fn snuff_lasts_one_render_frame_even_with_many_substeps(){
        let mut pool=Pool::default();pool.sparks.push(Spark{pos:Vec3::ZERO,vel:Vec3::Z*100.,size:0.002,age:0.,snuff:None});
        pool.advance_with(1./15360.,20,|_,_|Some((0.5,Vec3::Z*0.003,true)));
        assert_eq!(pool.crew_contacts,1);assert_eq!(pool.sparks[0].snuff,Some(20));assert_eq!(pool.sparks[0].vel,Vec3::ZERO);
        for _ in 0..16{pool.advance_with(1./15360.,20,|_,_|panic!("snuffed spark must not travel"));}
        assert_eq!(pool.sparks.len(),1);pool.advance_with(1./15360.,21,|_,_|None);assert!(!pool.active());
    }
}
