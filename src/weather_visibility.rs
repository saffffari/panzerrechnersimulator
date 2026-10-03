//! Gameplay optical weather, matching `weather_volume.wgsl` in world space.
//! This is sampled Beer–Lambert contrast loss, not 3D multiple scattering or vision physiology.
//! Roofs/walls remain geometry occluders. Only the same source-derived sealed Tiger air domains
//! as rendering are excluded; open ruins do not acquire an AI-only clear-air bubble.
use glam::{IVec3, Mat4, Vec3};
use crate::renderer::cabin_medium::{Medium, Uniform, CABIN_RAY_EPSILON};
use std::sync::Arc;

pub(crate) const WORLD_AEROSOL_BOTTOM_M:f32=-128.0;
pub(crate) const WORLD_AEROSOL_TOP_M:f32=256.0;
pub(crate) const BLOWING_SNOW_TOP_M:f32=128.0;
/// A declared gameplay recognition criterion, not a measured human threshold. Optical instruments
/// may magnify angular size, but cannot restore contrast removed on the common outdoor path.
pub(crate) const RECOGNITION_CONTRAST:f32=0.10;
#[derive(Clone,Copy,Debug,PartialEq)]
pub(crate) struct WorldMediumProfile {pub snow_fraction:f32,pub layer:[f32;4]}
pub(crate) fn world_medium_profile(precipitation:f32,storm:f32)->WorldMediumProfile {
    let snow_fraction=precipitation.clamp(0.0,1.0)*storm.clamp(0.0,1.0);
    let w=smoothstep(0.05,0.75,snow_fraction);
    WorldMediumProfile{snow_fraction,layer:[WORLD_AEROSOL_BOTTOM_M,
        WORLD_AEROSOL_TOP_M+(BLOWING_SNOW_TOP_M-WORLD_AEROSOL_TOP_M)*w,0.0,0.0]}
}
#[derive(Clone)]
pub(crate) struct WeatherView {
    pub sigma:f32,
    pub snow_fraction:f32,
    pub layer:[f32;2],
    pub offset:Vec3,
    pub cabin:Option<Arc<Medium>>,
    pub cabin_uniform:Uniform,
}
impl Default for WeatherView {
    fn default()->Self {Self{sigma:0.0,snow_fraction:0.0,layer:[-128.0,256.0],
        offset:Vec3::ZERO,cabin:None,cabin_uniform:Uniform::default()}}
}
fn smoothstep(lo:f32,hi:f32,x:f32)->f32 {let t=((x-lo)/(hi-lo)).clamp(0.0,1.0);t*t*(3.0-2.0*t)}
fn mix(a:f32,b:f32,t:f32)->f32 {a+(b-a)*t}
fn weather_hash(p:IVec3)->f32 {
    let mut h=(p.x as u32).wrapping_mul(1597334677)^(p.y as u32).wrapping_mul(3812015801)
        ^(p.z as u32).wrapping_mul(2798796415);
    h=(h^(h>>16)).wrapping_mul(2246822519);h=(h^(h>>13)).wrapping_mul(3266489917);
    ((h^(h>>16))>>8) as f32/16777216.0
}
fn weather_noise(p:Vec3)->f32 {
    let cell=p.floor().as_ivec3();let f=p-p.floor();let u=f*f*(Vec3::splat(3.0)-2.0*f);
    let a=mix(weather_hash(cell),weather_hash(cell+IVec3::X),u.x);
    let b=mix(weather_hash(cell+IVec3::Y),weather_hash(cell+IVec3::new(1,1,0)),u.x);
    let c=mix(weather_hash(cell+IVec3::Z),weather_hash(cell+IVec3::new(1,0,1)),u.x);
    let d=mix(weather_hash(cell+IVec3::new(0,1,1)),weather_hash(cell+IVec3::ONE),u.x);
    mix(mix(a,b,u.y),mix(c,d,u.y),u.z)
}
pub(crate) fn relative_density(p:Vec3,offset:Vec3,snow:f32)->f32 {
    let q=p-offset;
    let broad=weather_noise(q/Vec3::new(110.0,42.0,110.0));
    let eddies=weather_noise(q/Vec3::new(27.0,13.0,27.0)+Vec3::new(19.7,3.1,-6.3));
    mix(1.0,0.22+2.15*smoothstep(0.18,0.82,broad*0.72+eddies*0.28),snow.clamp(0.0,1.0))
}
impl WeatherView {
    pub fn set_cabin_pose(&mut self,hull:Mat4,turret:Mat4) {
        if let Some(cabin)=&self.cabin {self.cabin_uniform=cabin.uniform(hull,turret);}
    }
    // Same layer clipping and 48 midpoint quadrature samples as the highest-quality shared shader
    // path. No scene query/allocation per sample; source cabin traversal happens only near its box.
    fn unpartitioned_depth(&self,origin:Vec3,ray:Vec3,limit:f32)->f32 {
        let mut begin=0.0f32;let mut end=limit.max(0.0);
        if ray.y.abs()<1e-6 {if origin.y<self.layer[0]||origin.y>self.layer[1] {return 0.0;}}
        else {let a=(self.layer[0]-origin.y)/ray.y;let b=(self.layer[1]-origin.y)/ray.y;
            begin=a.min(b).max(0.0);end=end.min(a.max(b));}
        if end<=begin || self.sigma<=0.0 {return 0.0;}
        if self.snow_fraction<=0.0 {return self.sigma*(end-begin);}
        let mut tau=0.0;
        for i in 0..48 {let a=i as f32/48.0;let b=(i+1) as f32/48.0;
            let lo=begin+(end-begin)*a*a;let hi=begin+(end-begin)*b*b;
            tau+=self.sigma*relative_density(origin+ray*((lo+hi)*0.5),self.offset,self.snow_fraction)*(hi-lo);}
        tau
    }
    fn directed_depth(&self,from:Vec3,to:Vec3)->f32 {
        let length=from.distance(to);if length<1e-5 || self.sigma<=0.0{return 0.0;}
        let ray=(to-from)/length;let mut cursor=0.0;let mut tau=0.0;
        loop {if cursor>=length {break;}
            let span=self.cabin.as_ref().map_or([length,length],|c|c.next_span(&self.cabin_uniform,from,ray,cursor,length));
            let exterior_end=if span[1]>span[0] {span[0]}else{length};
            if exterior_end>cursor {tau+=self.unpartitioned_depth(from+ray*cursor,ray,exterior_end-cursor);}
            if span[1]<=span[0] {break;}
            cursor=span[1]+CABIN_RAY_EPSILON;
        }
        tau
    }
    pub fn transmittance(&self,from:Vec3,to:Vec3)->f32 {
        if !from.is_finite()||!to.is_finite() {return 0.0;}
        // A symmetric quadrature removes observer/team-dependent integration bias. The exact
        // density, vertical support and cabin intervals remain those used by both rendered views.
        (-0.5*(self.directed_depth(from,to)+self.directed_depth(to,from))).exp().clamp(0.0,1.0)
    }
    pub fn recognizable(&self,from:Vec3,to:Vec3)->bool {self.transmittance(from,to)>=RECOGNITION_CONTRAST}
    /// Local outdoor optical severity. Do not remove cabin spans here: the sound director applies
    /// enclosure/hatch/building transmission separately, while this reports the storm outside.
    pub fn local_whiteout(&self,point:Vec3)->f32 {
        if self.snow_fraction<=0.0 || !point.is_finite(){return 0.0;}
        let mut tau=0.0;
        for ray in [Vec3::X,-Vec3::X,Vec3::Z,-Vec3::Z] {
            tau+=self.unpartitioned_depth(point,ray,35.0);
        }
        let sigma=tau/(4.0*35.0);
        let range=if sigma>1e-8 {-RECOGNITION_CONTRAST.ln()/sigma}else{f32::INFINITY};
        smoothstep(0.0,1.0,(35.0-range)/23.0)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn uniform_fog_is_beer_lambert_and_clips_vertical_layer() {
        let v=WeatherView{sigma:0.02,..Default::default()};
        assert!((v.transmittance(Vec3::ZERO,Vec3::X*50.0)-(-1.0f32).exp()).abs()<1e-6);
        assert_eq!(v.transmittance(Vec3::Y*300.0,Vec3::new(100.0,300.0,0.0)),1.0);
        assert_eq!(v.transmittance(Vec3::ZERO,Vec3::ZERO),1.0);
    }
    #[test] fn moving_weather_is_local_reciprocal_and_team_independent() {
        let mut v=WeatherView{sigma:0.09,snow_fraction:1.0,offset:Vec3::new(42.0,0.0,-19.0),..Default::default()};
        let mut minimum=1.0f32;let mut maximum=0.0f32;
        for i in -10..10 {let a=Vec3::new(i as f32*17.0,2.0,18.0);let b=a+Vec3::new(25.0,1.0,8.0);
            let t=v.transmittance(a,b);assert_eq!(t,v.transmittance(b,a));
            minimum=minimum.min(t);maximum=maximum.max(t);
        }
        assert!(maximum-minimum>0.2,"local banks must matter, not a global storm radius");
        let a=Vec3::new(30.0,2.0,80.0);let b=a+Vec3::Z*35.0;let before=v.transmittance(a,b);
        let wind=Vec3::new(11.0,0.0,-5.0);v.offset+=wind;
        assert!((v.transmittance(a+wind,b+wind)-before).abs()<1e-6,"world density follows actual wind offset");
    }
    #[test] fn nearby_observers_share_whiteout_and_optical_reach() {
        let v=WeatherView{sigma:0.25,snow_fraction:1.0,..Default::default()};
        assert!(v.recognizable(Vec3::Y*2.0,Vec3::Y*2.0+Vec3::X));
        assert!(!v.recognizable(Vec3::Y*2.0,Vec3::Y*2.0+Vec3::X*150.0));
        let a=v.local_whiteout(Vec3::new(120.0,2.0,100.0));
        let b=v.local_whiteout(Vec3::new(121.0,2.0,100.0));
        assert!((a-b).abs()<0.04,"one metre should not switch between unrelated weather");
        assert_eq!(WeatherView::default().local_whiteout(Vec3::ZERO),0.0);
        assert_eq!(WeatherView{snow_fraction:0.0,..v}.local_whiteout(Vec3::ZERO),0.0);
    }
    #[test] fn shader_density_constants_remain_shared_contract() {
        let shader=include_str!("../shaders/weather_volume.wgsl");
        for token in ["1597334677u","3812015801u","2798796415u","2246822519u","3266489917u",
            "vec3(110.0,42.0,110.0)","vec3(27.0,13.0,27.0)","vec3(19.7,3.1,-6.3)",
            "0.22+2.15*smoothstep(0.18,0.82,broad*0.72+eddies*0.28)"] {
            assert!(shader.contains(token),"update CPU/GPU weather parity for {token}");
        }
    }
}
