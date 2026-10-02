//! Cosmetic snow retained on the source Tiger track steel, plus small ground-contact flecks.
//! The source belt remains rigid in this game: retained marks follow its actual hull transform.
//! This is a bounded snow transport estimate, not a deforming belt or granular snow solver.
use std::{collections::HashSet,sync::{Arc,OnceLock}};
use anyhow::{Context,ensure};
use glam::{Mat4,Quat,Vec2,Vec3};

pub const ATTACHED_CAPACITY:usize=4096;
pub const LOOSE_CAPACITY:usize=512;
const STEP:f64=1.0/120.0;
const PATH:&str="assets/tanks/tiger/baked/tiger_hull.glb";

#[derive(Clone,Copy)]
struct Anchor { p:Vec3,n:Vec3,side:usize,id:u32 }
struct Source { anchors:Vec<Anchor>,contact:[Vec<usize>;2],nodes:usize,triangles:usize }
#[derive(Clone,Copy)]
struct Triangle { p:[Vec3;3],n:Vec3,end_area:f64 }
fn hash(seed:u32)->f32 {
    let mut x=seed.wrapping_add(0x9e37_79b9);
    x=(x^(x>>16)).wrapping_mul(0x85eb_ca6b);x=(x^(x>>13)).wrapping_mul(0xc2b2_ae35);
    ((x^(x>>16))>>8) as f32/16_777_216.0
}

fn source()->anyhow::Result<Arc<Source>> {
    static SOURCE:OnceLock<Result<Arc<Source>,String>>=OnceLock::new();
    SOURCE.get_or_init(||load_source().map(Arc::new).map_err(|e|format!("{e:#}")))
        .clone().map_err(anyhow::Error::msg)
}
fn load_source()->anyhow::Result<Source> {
    // Geometry only: do not decode or duplicate the large shared Tiger material atlas.
    let bytes=crate::resource::read(PATH)?;
    let gltf=gltf::Gltf::from_slice(&bytes).context("track-snow source GLB")?;
    let blob=gltf.blob.as_deref().context("track source must embed its vertex buffer")?;
    ensure!(gltf.buffers().all(|b|b.index()==0&&matches!(b.source(),gltf::buffer::Source::Bin)),
        "track source must use one embedded geometry buffer");
    let selected=crate::tank::tiger_track_source_nodes();
    let mut found=HashSet::new();let mut triangles=Vec::new();let mut area=0.0;
    fn walk(node:gltf::Node<'_>,parent:Mat4,blob:&[u8],selected:&[&str],
        found:&mut HashSet<String>,triangles:&mut Vec<Triangle>,area:&mut f64)->anyhow::Result<()> {
        let transform=parent*Mat4::from_cols_array_2d(&node.transform().matrix());
        if node.name().is_some_and(|name|selected.contains(&name)) {
            let name=node.name().unwrap();
            ensure!(found.insert(name.to_owned()),"duplicate track source node {name}");
            if let Some(mesh)=node.mesh() {
                for primitive in mesh.primitives() {
                    ensure!(primitive.mode()==gltf::mesh::Mode::Triangles,"nontriangle track source {name}");
                    let reader=primitive.reader(|_|Some(blob));
                    let positions:Vec<_>=reader.read_positions().context("track positions")?
                        .map(|p|transform.transform_point3(Vec3::from_array(p))).collect();
                    let indices:Vec<_>=reader.read_indices().map(|x|x.into_u32().collect())
                        .unwrap_or_else(||(0..positions.len() as u32).collect());
                    for tri in indices.chunks_exact(3) {
                        let p=[positions[tri[0] as usize],positions[tri[1] as usize],positions[tri[2] as usize]];
                        let cross=(p[1]-p[0]).cross(p[2]-p[0]);let twice_area=cross.length();
                        if !twice_area.is_finite()||twice_area<1e-9 {continue;}
                        *area+=twice_area as f64*0.5;
                        triangles.push(Triangle {p,n:cross/twice_area,end_area:*area});
                    }
                }
            }
        }
        for child in node.children() {walk(child,transform,blob,selected,found,triangles,area)?;}
        Ok(())
    }
    let scene=gltf.default_scene().or_else(||gltf.scenes().next()).context("track source scene")?;
    for node in scene.nodes() {walk(node,Mat4::IDENTITY,blob,selected,&mut found,&mut triangles,&mut area)?;}
    ensure!(found.len()==selected.len(),"found {}/{} exact track nodes",found.len(),selected.len());
    ensure!(!triangles.is_empty()&&area>0.0,"empty track source geometry");
    let mut anchors=Vec::with_capacity(ATTACHED_CAPACITY);
    for i in 0..ATTACHED_CAPACITY {
        let at=area*(i as f64+0.5)/ATTACHED_CAPACITY as f64;
        let tri=&triangles[triangles.partition_point(|t|t.end_area<at).min(triangles.len()-1)];
        let u=hash(i as u32^0x51a9).sqrt();let v=hash(i as u32^0xa935);
        let p=tri.p[0]*(1.0-u)+tri.p[1]*(u*(1.0-v))+tri.p[2]*(u*v);
        anchors.push(Anchor {p,n:tri.n,side:usize::from(p.x<0.0),id:i as u32});
    }
    let contact=std::array::from_fn(|side| {
        let floor=anchors.iter().filter(|a|a.side==side).map(|a|a.p.y).fold(f32::INFINITY,f32::min);
        let low:Vec<_>=anchors.iter().enumerate().filter(|(_,a)|a.side==side&&a.p.y<floor+0.16)
            .map(|(i,_)|i).collect();
        // Uniformly spread bounded contact queries across actual sampled tread triangles.
        (0..low.len().min(64)).map(|i|low[i*low.len()/low.len().min(64)]).collect()
    });
    ensure!(contact.iter().all(|c:&Vec<usize>|!c.is_empty()),"source has no low track contact samples");
    Ok(Source {anchors,contact,nodes:found.len(),triangles:triangles.len()})
}

#[derive(Clone,Copy)]
struct Fleck {origin:Vec3,velocity:Vec3,wind:Vec3,born:f64,life:f32,id:u32}
impl Fleck {
    fn position(self,time:f64)->Vec3 {
        let t=(time-self.born).max(0.0) as f32;
        // Analytic drag/advection and gravity: age, rather than render-frame integration.
        let terminal=self.wind*0.28;
        self.origin+terminal*t+(self.velocity-terminal)*((1.0-(-3.0*t).exp())/3.0)
            -Vec3::Y*(0.5*9.81*t*t)
    }
}

pub struct System {
    source:Arc<Source>,model:Mat4,previous:Option<Mat4>,credit:f64,time:f64,
    snow:[f32;2],emit_credit:[f64;2],sequence:[u32;2],flecks:Vec<Fleck>,emitted:u64,
    weather_credit:f64,weather_sequence:u32,weather_marks:Vec<bool>,weather_contacts:u64,
}
impl System {
    pub fn load()->anyhow::Result<Self> {Ok(Self::from_source(source()?))}
    /// New match: retain the immutable source triangle sampling, clear every transient/mass.
    pub fn reset(&mut self) { *self=Self::from_source(self.source.clone()); }
    fn from_source(source:Arc<Source>)->Self {
        let weather_marks=vec![false;source.anchors.len()];
        Self {source,model:Mat4::IDENTITY,previous:None,credit:0.0,time:0.0,snow:[0.0;2],
            emit_credit:[0.0;2],sequence:[0;2],flecks:Vec::new(),emitted:0,
            weather_credit:0.0,weather_sequence:0,weather_marks,weather_contacts:0}
    }
    /// Belt order is [LeftTrack(+local X), RightTrack(-local X)]. Caller supplies actual motion
    /// and zeros a missing belt; a revving detached sprocket cannot pick up ground snow.
    pub fn update(&mut self,dt:f32,model:Mat4,belt_mps:[f32;2],winter:bool,
        precipitation:f32,wind:Vec2,ground:impl Fn(f32,f32)->f32,
        exposed:impl Fn(Vec3,Vec3)->bool) {
        if !dt.is_finite()||dt<=0.0||!model.is_finite(){return;}
        let old=self.previous.unwrap_or(model);self.model=model;self.previous=Some(model);
        let (s0,q0,p0)=old.to_scale_rotation_translation();let (s1,q1,p1)=model.to_scale_rotation_translation();
        let velocity=(p1-p0)/dt;
        let angular=(q1*q0.inverse()).to_scaled_axis()/dt;
        let belt=belt_mps.map(|v|if v.is_finite(){v.clamp(-15.0,15.0)}else{0.0});
        let precip=if precipitation.is_finite(){precipitation.clamp(0.0,5.0)}else{0.0};
        let wind=if wind.is_finite(){Vec3::new(wind.x,0.0,wind.y)}else{Vec3::ZERO};
        self.credit+=dt as f64;
        while self.credit+1e-9>=STEP {
            let t=((dt as f64-self.credit+STEP)/dt as f64).clamp(0.0,1.0) as f32;
            let pose=Mat4::from_scale_rotation_translation(s0.lerp(s1,t),q0.slerp(q1,t),p0.lerp(p1,t));
            self.step(pose,velocity,angular,belt,winter,precip,wind,&ground,&exposed);
            self.credit=(self.credit-STEP).max(0.0);
        }
    }
    fn step(&mut self,pose:Mat4,velocity:Vec3,angular:Vec3,belt:[f32;2],winter:bool,precip:f32,wind:Vec3,
        ground:&impl Fn(f32,f32)->f32,exposed:&impl Fn(Vec3,Vec3)->bool) {
        self.time+=STEP;
        self.flecks.retain(|f| {
            let p=f.position(self.time);
            self.time-f.born<f.life as f64 && p.y>ground(p.x,p.z)+0.003
        });
        for side in 0..2 {
            let contacts:Vec<_>=self.source.contact[side].iter().filter_map(|&index| {
                let anchor=self.source.anchors[index];let mut p=pose.transform_point3(anchor.p);
                let y=ground(p.x,p.z);let clearance=p.y-y;
                if !y.is_finite()||!(-0.085..0.18).contains(&clearance) {return None;}
                p.y=p.y.max(y+0.008);Some((anchor,p))
            }).collect();
            let fraction=contacts.len() as f32/self.source.contact[side].len() as f32;
            let speed=belt[side].abs();
            // Contact pickup is transported around the source belt as it runs. There is no
            // accumulation on the hull or wheels; snow remains limited to selected track faces.
            let pickup=if winter {0.20*speed*fraction}else{0.0};
            let loss=if winter {0.009*speed+0.0008*wind.length()}else{0.18};
            let rate=pickup+loss;
            if rate>0.0 {let target=pickup/rate;self.snow[side]=target+(self.snow[side]-target)*(-rate*STEP as f32).exp();}
            if !winter||contacts.is_empty()||speed<0.08 {continue;}
            self.emit_credit[side]+=f64::from(speed*fraction*(3.0+8.0*self.snow[side]))*STEP;
            while self.emit_credit[side]>=1.0 {
                self.emit_credit[side]-=1.0;
                self.sequence[side]=self.sequence[side].wrapping_add(1);
                let id=self.sequence[side].wrapping_mul(977)^((side as u32)<<30);
                let (_,p)=contacts[(hash(id)*contacts.len() as f32) as usize%contacts.len()];
                if self.flecks.len()>=LOOSE_CAPACITY {continue;}
                let outward=if side==0 {1.0}else{-1.0};
                let local=Vec3::new(outward*(0.10+0.16*hash(id^71)),
                    0.35+speed.min(8.0)*(0.16+0.06*hash(id^73)),-belt[side]);
                self.flecks.push(Fleck {origin:p,velocity:velocity+angular.cross(p-pose.w_axis.truncate())+pose.transform_vector3(local),wind,
                    born:self.time,life:0.40+hash(id^79)*0.40,id});
                self.emitted+=1;
            }
        }
        // A few source-face candidates per second, not per-frame rays. Covered track faces
        // receive no weather mark. The exact geometry callback decides fender/armor exposure.
        if winter {
            self.weather_credit+=f64::from(precip*4.0)*STEP;
            while self.weather_credit>=1.0 {
                self.weather_credit-=1.0;self.weather_sequence=self.weather_sequence.wrapping_add(1);
                let id=self.weather_sequence.wrapping_mul(104729)^0x534e_4f57;
                let index=(hash(id)*self.source.anchors.len() as f32) as usize%self.source.anchors.len();
                let a=self.source.anchors[index];
                let n=pose.transform_vector3(a.n).normalize_or_zero();
                let incoming=wind*0.28-Vec3::Y*1.6;
                if incoming.dot(n)>=-0.10 {continue;}
                let p=pose.transform_point3(a.p)+n*0.004;
                let from=p-incoming.normalize_or_zero()*4.0;
                if !exposed(from,p+n*0.010) {continue;}
                self.weather_marks[index]=true;self.weather_contacts+=1;
                if self.flecks.len()<LOOSE_CAPACITY {
                    let rebound=(incoming-n*(2.0*incoming.dot(n)))*0.18;
                    self.flecks.push(Fleck {origin:p,velocity:rebound+velocity
                        +angular.cross(p-pose.w_axis.truncate()),wind,born:self.time,
                        life:0.12+hash(id^41)*0.12,id});
                    self.emitted+=1;
                }
            }
        } else {
            self.weather_credit=0.0;
            // Warm-season transition has no new precipitation; retained weather marks leave
            // with the same gradual clearing state as the tracked material.
            if self.snow.iter().all(|v|*v<0.01) {self.weather_marks.fill(false);}
        }
    }
    pub fn draw(&self,camera:&crate::camera::Camera,native_height:u32)->(Vec<Mat4>,Vec<Mat4>) {
        let mut attached=Vec::new();let mut loose=Vec::new();
        let forward=camera.forward();
        let pixel=|p:Vec3| {
            let depth=(p-camera.pos).dot(forward);
            if depth<=camera.znear {return None;}
            Some((2.0*depth*(camera.fov_y*0.5).tan()/native_height.max(1) as f32).max(1e-6))
        };
        for (index,a) in self.source.anchors.iter().enumerate() {
            if hash(a.id^0x736e_6f77)>self.snow[a.side]*0.82 && !self.weather_marks[index] {continue;}
            let p=self.model.transform_point3(a.p+a.n*0.0025);
            let Some(size)=pixel(p)else{continue;};
            // Never enlarge a distant retained patch beyond a real centimetre-scale snow clump.
            // Near the observer its projected footprint is at most one native pixel.
            let size=(size*0.72).min(0.028);
            let n=self.model.transform_vector3(a.n).normalize_or_zero();
            attached.push(Mat4::from_scale_rotation_translation(Vec3::new(size,size.min(0.0018),size),
                Quat::from_rotation_arc(Vec3::Y,n),p));
        }
        for f in &self.flecks {
            let p=f.position(self.time+self.credit);let Some(size)=pixel(p)else{continue;};
            let size=(size*0.65).min(0.022)*(0.80+hash(f.id^89)*0.20);
            loose.push(Mat4::from_scale_rotation_translation(Vec3::splat(size),
                Quat::from_rotation_y(hash(f.id)*std::f32::consts::TAU),p));
        }
        (attached,loose)
    }
    pub fn receipt(&self)->serde_json::Value {
        serde_json::json!({"source":PATH,"selected_track_nodes":self.source.nodes,
            "source_track_triangles":self.source.triangles,"surface_anchors":self.source.anchors.len(),
            "snow_fraction_left_right":self.snow,"loose_flecks":self.flecks.len(),
            "emitted_flecks":self.emitted,"weather_contacts":self.weather_contacts,
            "retained_weather_grains":self.weather_marks.iter().filter(|v|**v).count(),"fixed_step_hz":120,"retained_on_hull_geometry":true,
            "animated_belt_circulation":false,"emissive":false})
    }
}

pub fn prototype()->crate::gltf_load::CpuMesh {
    let mut mesh=crate::prims::box_mesh(Vec3::splat(-0.5),Vec3::splat(0.5),[192,198,200]);
    mesh.metallic_factor=0.0;mesh.roughness_factor=1.0;mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture()->System {
        let anchors=vec![Anchor {p:Vec3::new(1.5,0.01,0.0),n:Vec3::Y,side:0,id:0},
            Anchor {p:Vec3::new(-1.5,0.01,0.0),n:Vec3::Y,side:1,id:1}];
        System::from_source(Arc::new(Source {anchors,contact:[vec![0],vec![1]],nodes:2,triangles:2}))
    }
    #[test]
    fn snow_uses_only_exact_track_source_triangles_without_loading_images() {
        let source=source().unwrap();
        assert_eq!(source.nodes,crate::tank::tiger_track_source_nodes().len());
        assert_eq!(source.anchors.len(),ATTACHED_CAPACITY);
        assert!(source.triangles>1000);
        assert!(source.anchors.iter().all(|a|a.p.is_finite()&&a.n.is_finite()&&(a.n.length()-1.0).abs()<1e-4));
        for side in 0..2 {assert!(!source.contact[side].is_empty());assert!(source.contact[side].len()<=64);}
    }
    #[test]
    fn pickup_shedding_and_wind_are_time_partition_stable_and_broken_side_does_not_drive() {
        let run=|fps:u32| {
            let mut s=fixture();
            for _ in 0..fps*3 {s.update(1.0/fps as f32,Mat4::IDENTITY,[3.0,0.0],true,0.0,Vec2::new(6.0,-2.0),|_,_|0.0,|_,_|true);}
            s
        };
        let reference=run(60);
        assert!(reference.snow[0]>0.7);assert_eq!(reference.snow[1],0.0);
        assert!(reference.emitted>20);assert_eq!(reference.sequence[1],0);
        for fps in [30,144] {
            let other=run(fps);assert_eq!(other.emitted,reference.emitted);assert_eq!(other.flecks.len(),reference.flecks.len());
            assert!((other.snow[0]-reference.snow[0]).abs()<1e-5);
            for (a,b) in other.flecks.iter().zip(&reference.flecks) {
                assert_eq!(a.id,b.id);assert!(a.position(other.time).distance(b.position(reference.time))<1e-5);
            }
        }
        let mut no_ground=fixture();no_ground.update(3.0,Mat4::from_translation(Vec3::Y*2.0),[3.0;2],true,0.0,Vec2::ZERO,|_,_|0.0,|_,_|true);
        assert_eq!(no_ground.snow,[0.0;2]);assert_eq!(no_ground.emitted,0);
        let mut dry=fixture();dry.update(3.0,Mat4::IDENTITY,[3.0;2],false,3.0,Vec2::ZERO,|_,_|0.0,|_,_|true);
        assert_eq!(dry.snow,[0.0;2]);assert_eq!(dry.emitted,0);
        let mut reset=reference;let anchors=reset.source.clone();reset.reset();
        assert!(Arc::ptr_eq(&anchors,&reset.source));assert_eq!(reset.snow,[0.0;2]);
        assert_eq!(reset.emitted,0);assert!(reset.flecks.is_empty());
    }
    #[test]
    fn precipitation_needs_an_exposed_source_face_before_it_accumulates_or_rebounds() {
        let mut blocked=fixture();let mut clear=fixture();
        for _ in 0..240 {
            blocked.update(1.0/120.0,Mat4::IDENTITY,[0.0;2],true,3.0,Vec2::ZERO,|_,_|0.0,|_,_|false);
            clear.update(1.0/120.0,Mat4::IDENTITY,[0.0;2],true,3.0,Vec2::ZERO,|_,_|0.0,|from,to| {
                assert!(from.y>to.y+3.0);assert!(to.y<0.1);true
            });
        }
        assert_eq!(blocked.weather_contacts,0);assert_eq!(blocked.emitted,0);
        assert!(blocked.weather_marks.iter().all(|x|!*x));
        assert!(clear.weather_contacts>=20);assert!(clear.weather_marks.iter().any(|x|*x));
        assert_eq!(clear.snow,[0.0;2],"stationary tracks must not invent ground pickup");
        clear.reset();assert_eq!(clear.weather_contacts,0);assert!(clear.weather_marks.iter().all(|x|!*x));
    }
}
