//! Near-field solid grass. Deterministic world roots, a shared damped wind field, and persistent
//! tread contact replace the old metre-wide crossed cards. The bounded raster pool is visible in
//! both render tiers; it is deliberately not included in secondary ray visibility or ray shadows.
use std::collections::HashMap;
use glam::{Mat4, Quat, Vec2, Vec3};
use crate::renderer::Renderer;
#[path = "grass_filament.rs"]
mod filament;
use filament::{Bend, TrackMemory};

pub const CAPACITY: usize = 32_768;
const CELL: f32 = 2.0;
const RADIUS: i32 = 13; // 26 m; 32k hard cap, allocated foreground first.
const CANDIDATES: u32 = 2048; // 512 stable candidate roots/m² in the immediate on-foot foreground.

/// Spend the existing cap near the feet first. Cell iteration uses the nearest cell distance to
/// bound candidate work; distant cells never loop over all 2048 foreground candidates.
fn candidate_density(distance: f32) -> f32 {
    let blend = |a: f32, b: f32, t: f32| {let t=t.clamp(0.0,1.0); a+(b-a)*t*t*(3.0-2.0*t)};
    if distance < 2.5 { 512.0 }
    else if distance < 5.0 { blend(512.0,64.0,(distance-2.5)/2.5) }
    else if distance < 10.0 { blend(64.0,4.0,(distance-5.0)/5.0) }
    else { blend(4.0,0.0,(distance-10.0)/16.0) }
}

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

struct Mask { w:usize, h:usize, rgba:Vec<u8>, half_extent:f32 }
impl Mask {
    fn sample(&self,x:f32,z:f32)->(f32,f32,f32) {
        let u=(x/(2.0*self.half_extent)+0.5).clamp(0.0,1.0);
        let v=(z/(2.0*self.half_extent)+0.5).clamp(0.0,1.0);
        let px=((u*(self.w-1) as f32) as usize).min(self.w-1);
        let py=((v*(self.h-1) as f32) as usize).min(self.h-1);
        let i=(py*self.w+px)*4;
        (self.rgba[i] as f32/255.0,self.rgba[i+1] as f32/255.0,self.rgba[i+2] as f32/255.0)
    }
}

struct Root {
    model:Mat4,
    position:Vec3,
    wind_cell:(i32,i32),
    shelter:f32,
    compression:f32,
    target_compression:f32,
    direction:Vec2,
}

pub struct Grass {
    batch:usize,
    mask:Option<Mask>,
    mask_requested:bool,
    center:Option<(i32,i32)>,
    seed:u32,
    roots:Vec<Root>,
    wind:HashMap<(i32,i32),Bend>,
    tracks:TrackMemory,
    contact_revision:u64,
}

impl Grass {
    pub fn new(r:&mut Renderer,device:&wgpu::Device,queue:&wgpu::Queue,
        mask_path:Option<&str>,half_extent:f32,seed:u32)->Self {
        let mask=mask_path.and_then(|p| {
            match crate::resource::resolve(p).and_then(|resolved|image::open(resolved).map_err(anyhow::Error::from)) {
                Ok(img)=>{let rgba=img.into_rgba8();let(w,h)=rgba.dimensions();Some(Mask{w:w as usize,h:h as usize,rgba:rgba.into_raw(),half_extent})},
                Err(error)=>{println!("[grass] no ground-class mask at {p}: {error}; sparse dry stalks");None}
            }
        });
        let batch=r.add_dynamic_instanced_pool(device,queue,&filament::stalk_mesh(mask.is_none()),CAPACITY)
            .expect("bounded grass instance pool");
        r.set_batch_flags(batch,4.0);
        Self{batch,mask,mask_requested:mask_path.is_some(),center:None,seed,roots:Vec::new(),wind:HashMap::new(),tracks:TrackMemory::default(),contact_revision:0}
    }

    pub fn stats(&self)->serde_json::Value {
        let focus=self.center.map(|(x,z)|Vec2::new((x as f32+0.5)*CELL,(z as f32+0.5)*CELL));
        let within=|radius:f32|focus.map_or(0,|focus|self.roots.iter().filter(|r|
            Vec2::new(r.position.x,r.position.z).distance(focus)<=radius).count());
        serde_json::json!({"visible_window_stalks":self.roots.len(),"persistent_contact_cells":self.tracks.cell_count(),
            "max_stalks":CAPACITY,"within_2_5_m":within(2.5),"within_5_m":within(5.0),
            "ground_class_loaded":self.mask.is_some(),"ground_class_requested":self.mask_requested,
            "winter_fallback":self.mask.is_none(),"triangles_per_stalk":16,"primary_raster_only":true,
            "secondary_visibility_and_self_shadow":false})
    }

    pub fn trample(&mut self,position:Vec3,heading:f32){self.tracks.stamp(position,heading);}

    fn rebuild(&mut self,center:(i32,i32),ground:&impl Fn(f32,f32)->f32){
        self.center=Some(center);
        let focus=Vec2::new((center.0 as f32+0.5)*CELL,(center.1 as f32+0.5)*CELL);
        // Visit near cells first so the hard cap never consumes the foreground in distant grass.
        let mut cells:Vec<_>=(-RADIUS..=RADIUS).flat_map(|z|(-RADIUS..=RADIUS).map(move|x|(x,z))).collect();
        cells.sort_by_key(|&(x,z)|(x*x+z*z,z,x));
        self.roots.clear();
        for (dx,dz) in cells {
            let (cx,cz)=(center.0+dx,center.1+dz);
            let seed=self.seed ^ (cx as u32).wrapping_mul(73_856_093) ^ (cz as u32).wrapping_mul(19_349_663);
            let box_center=Vec2::new((cx as f32+0.5)*CELL,(cz as f32+0.5)*CELL);
            let nearest=(box_center-focus).abs()-Vec2::splat(CELL*0.5);
            let nearest_distance=nearest.max(Vec2::ZERO).length();
            let count=if self.mask.is_some() {
                (candidate_density(nearest_distance)*CELL*CELL).ceil() as u32
            } else {256}; // Preserve the existing sparse winter distribution.
            for k in 0..count.min(CANDIDATES) {
                if self.roots.len()>=CAPACITY { break; }
                let key=seed.wrapping_add(k.wrapping_mul(31));
                let x=(cx as f32+hash01(key.wrapping_add(1)))*CELL;
                let z=(cz as f32+hash01(key.wrapping_add(2)))*CELL;
                let distance=Vec2::new(x,z).distance(focus);
                if distance>26.0 {continue;}
                let falloff=if self.mask.is_some(){
                    if k as f32+hash01(key^0x746C_918D)>=candidate_density(distance)*CELL*CELL {continue;}
                    1.0
                } else {(1.0-((distance-11.0)/15.0).clamp(0.0,1.0)).powi(2)};
                let (pasture,woods,crop)=self.mask.as_ref().map(|m|m.sample(x,z)).unwrap_or((0.0,0.0,0.0));
                // Continuous short pasture; the authored mask, not periodic holes, leaves roads
                // and building floors bare. Woodland roots are sparser under the canopy.
                let patch=(x*0.17+z*0.043).sin()*(z*0.13-x*0.037).cos()*0.5+0.5;
                let density=if self.mask.is_some(){pasture*(0.82+0.16*patch)+woods*0.24+crop*0.10}else{0.003};
                if hash01(key.wrapping_add(3))>=density.min(0.98)*falloff {continue;}
                let height=if self.mask.is_some(){0.28+hash01(key.wrapping_add(4))*0.38}else{0.35+hash01(key.wrapping_add(4))*0.50};
                let width=0.85+hash01(key.wrapping_add(5))*0.25;
                let pos=Vec3::new(x,ground(x,z)-0.005,z);
                let contact=self.tracks.sample(x,z);
                let model=Mat4::from_scale_rotation_translation(Vec3::new(width,height,width),
                    Quat::from_rotation_y(hash01(key.wrapping_add(6))*std::f32::consts::TAU),pos);
                self.roots.push(Root{model,position:pos,wind_cell:((x/8.0).floor() as i32,(z/8.0).floor() as i32),
                    shelter:(1.0-woods*0.82).clamp(0.15,1.0),compression:contact.compression,
                    target_compression:contact.compression,direction:contact.direction});
            }
        }
        let live_cells:std::collections::HashSet<_>=self.roots.iter().map(|root|root.wind_cell).collect();
        self.wind.retain(|key,_|live_cells.contains(key));
        for root in &self.roots {self.wind.entry(root.wind_cell).or_default();}
        self.contact_revision=self.tracks.revision;
        println!("[grass] {} solid stalks / {CAPACITY} budget; {} persistent tread cells; primary raster only",self.roots.len(),self.tracks.cell_count());
    }

    /// `wind_at` is the same actual world wind used by smoke/weather. Only a few 8 m wind cells
    /// integrate springs; stalks share their cell response with a smooth spatial gust phase.
    pub fn update(&mut self,r:&mut Renderer,cam:Vec3,dt:f32,
        ground:impl Fn(f32,f32)->f32,wind_at:impl Fn(Vec3)->Vec2){
        let center=((cam.x/CELL).floor() as i32,(cam.z/CELL).floor() as i32);
        if self.center!=Some(center) {self.rebuild(center,&ground);}
        if self.contact_revision!=self.tracks.revision {
            for root in &mut self.roots {
                let contact=self.tracks.sample(root.position.x,root.position.z);
                root.target_compression=contact.compression;
                root.direction=contact.direction;
            }
            self.contact_revision=self.tracks.revision;
        }
        for (&(x,z),bend) in &mut self.wind {
            let point=Vec3::new((x as f32+0.5)*8.0,0.0,(z as f32+0.5)*8.0);
            let wind=wind_at(point);
            let phase=r.time*1.4-point.x*0.075-point.z*0.052;
            let gust=0.78+0.18*phase.sin()+0.07*(phase*2.17+0.8).sin();
            // Drag response is quadratic in flow speed, bounded for the short grass length.
            let target=wind.normalize_or_zero()*(wind.length_squared()*0.005*gust).min(0.19);
            bend.update(target,dt);
        }
        let mut mats=Vec::with_capacity(self.roots.len());
        for root in &mut self.roots {
            root.compression+=(root.target_compression-root.compression)*(1.0-(-dt.clamp(0.0,0.1)*18.0).exp());
            let flat=root.compression;
            let target=Vec3::new(root.direction.x*0.98*flat,1.0-0.82*flat,root.direction.y*0.98*flat).normalize();
            let hinge=Quat::from_rotation_arc(Vec3::Y,target);
            let mut m=Mat4::from_translation(root.position)*Mat4::from_quat(hinge)
                *Mat4::from_translation(-root.position)*root.model;
            let displacement=self.wind[&root.wind_cell].displacement*root.shelter*(1.0-flat*0.93)
                *root.model.y_axis.truncate().length();
            // The instance's unused affine bottom row carries world-space tip displacement. Both
            // vertex shaders restore a proper affine matrix before projection and transform normals.
            m.x_axis.w=displacement.x;
            m.z_axis.w=displacement.y;
            mats.push(m);
        }
        let uploaded=r.set_dynamic_instanced_mats(self.batch,&mats);
        debug_assert!(uploaded);
    }
}

#[cfg(test)]
mod population_tests {
    use super::*;
    fn population(mask:Option<Mask>,seed:u32)->Grass {
        Grass {batch:usize::MAX,mask_requested:mask.is_some(),mask,center:None,seed,
            roots:Vec::new(),wind:HashMap::new(),tracks:TrackMemory::default(),contact_revision:0}
    }
    fn constant_mask(rgb:[u8;3])->Mask {Mask{w:1,h:1,rgba:vec![rgb[0],rgb[1],rgb[2],255],half_extent:384.0}}
    #[test]
    fn immediate_pasture_has_dense_grounded_coverage_under_the_existing_cap() {
        for seed in [0,42,20260905] {
            let mut grass=population(Some(constant_mask([255,0,0])),seed);
            grass.rebuild((0,0),&|x,z|0.03*x-0.02*z);
            let stats=grass.stats();
            assert!(grass.roots.len()<=CAPACITY);
            assert!(stats["within_2_5_m"].as_u64().unwrap()>8_000);
            assert!(stats["within_5_m"].as_u64().unwrap()>20_000);
            assert!(grass.roots.iter().all(|root|(root.position.y-(0.03*root.position.x-0.02*root.position.z-0.005)).abs()<1e-6));
            let mut occupied=std::collections::HashSet::new();
            for root in &grass.roots {
                if (root.position.x-1.0).abs()<1.5 && (root.position.z-1.0).abs()<1.5 {
                    occupied.insert(((root.position.x/0.25).floor() as i32,(root.position.z/0.25).floor() as i32));
                }
            }
            assert_eq!(occupied.len(),144,"every quarter-metre foreground patch needs rooted blades");
            let before:Vec<_>=grass.roots.iter().map(|r|r.position).collect();
            grass.rebuild((0,0),&|x,z|0.03*x-0.02*z);
            assert_eq!(grass.roots.iter().map(|r|r.position).collect::<Vec<_>>(),before);
        }
    }
    #[test]
    fn authored_bare_road_and_slab_mask_stays_bare_and_winter_stays_disclosed() {
        let mut bare=population(Some(constant_mask([0,0,0])),42);
        bare.rebuild((0,0),&|_,_|0.0);
        assert!(bare.roots.is_empty());
        let mut winter=population(None,42);
        winter.rebuild((0,0),&|_,_|0.0);
        assert!((80..350).contains(&winter.roots.len()));
        assert_eq!(winter.stats()["winter_fallback"],true);
        assert_eq!(winter.stats()["ground_class_loaded"],false);
    }
    #[test]
    fn active_playtest_class_mask_is_registered_to_its_768_metre_world() {
        let path="assets/worldgen/derived/aa4d7e350a597265/playtest_ground_class.png";
        let resolved=crate::resource::resolve(path).expect("versioned Playtest class mask");
        let rgba=image::open(resolved).unwrap().into_rgba8();let (w,h)=rgba.dimensions();
        let mask=Mask{w:w as usize,h:h as usize,rgba:rgba.into_raw(),half_extent:384.0};
        assert!(mask.sample(20.0,20.0).0>0.98,"unobstructed pasture should be green");
        assert_eq!(mask.sample(-67.0,94.0),(0.0,0.0,0.0),"garage slab must exclude roots");
        let mut grass=population(Some(mask),20260905);
        grass.rebuild((10,10),&|_,_|0.0);
        assert!(grass.stats()["within_2_5_m"].as_u64().unwrap()>8_000);
        assert_eq!(grass.stats()["ground_class_loaded"],true);
    }
}
