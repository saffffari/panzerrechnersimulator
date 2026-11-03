//! Range-only simulation-output frames. CPU data only: no GPU targets or camera history are kept.
//! Playback uploads recorded geometry/effects and never calls Game::update or a damage solver.
use super::*;

#[derive(Clone)]
struct MeshFrame {
    model: glam::Mat4, visible: bool, emissive: f32,
    local_min: glam::Vec3, local_max: glam::Vec3,
    finish: [f32; 4], gear: u32,
}
#[derive(Clone)]
struct BatchFrame {
    index: usize, mats: Arc<Vec<glam::Mat4>>, hidden: InstanceVisibility,
    emissive: f32, flags: f32,
}

pub(crate) struct SceneFrame {
    meshes: Vec<MeshFrame>, batches: Vec<BatchFrame>,
    vertices: HashMap<usize, Arc<Vec<crate::gltf_load::Vertex>>>,
    indices: HashMap<usize, Arc<Vec<u32>>>,
    scars: HashMap<usize, Vec<ScarDraw>>,
    lights: Vec<PointLight>, registered: Vec<PointLight>, emitters: Vec<Option<usize>>,
    smoke: [PuffGpu; MAX_PUFFS], gas: PuffGpu,
    fumes: [[f32; 4]; crate::cabin_fumes::LANES],
    bursts: Vec<renderer_weapon_particles::Record>, segments: Vec<renderer_weapon_particles::Record>,
    grain_ids: HashMap<usize, Vec<[u32; 4]>>,
    grain_materials: HashMap<usize, renderer_fine_vfx::Material>,
    medium_pose: Option<(glam::Mat4, glam::Mat4)>,
    atmosphere: (bool, f32, f32, f32, glam::Vec3),
    fronts: crate::pressure_front::Fronts, heat: Vec<u8>, pixels: Vec<u8>,
    pub(crate) time: f32,
    concussion: f32, flash: bool, wind: glam::Vec2,
    pub(crate) bytes: usize,
}

impl Renderer {
    pub(crate) fn enable_scene_cache_capture(&mut self, enabled: bool) {
        self.scene_cache_enabled = enabled;
        self.scene_cache_vertices.clear(); self.scene_cache_indices.clear();
        if enabled {
            for (slot,mesh) in self.meshes.iter().enumerate().filter(|(_,m)|m.temporal_dynamic) {
                if let Some(source)=&mesh.visibility_source {
                    self.scene_cache_vertices.insert(slot,Arc::new(source.vertices.clone()));
                    self.scene_cache_indices.insert(slot,Arc::new(source.indices.clone()));
                }
            }
        }
    }

    pub(crate) fn capture_scene_frame(&self, previous: Option<&SceneFrame>) -> anyhow::Result<SceneFrame> {
        let meshes: Vec<_> = self.meshes.iter().map(|m| MeshFrame {model: m.model, visible: m.visible,
            emissive: m.emissive, local_min: m.local_min, local_max: m.local_max,
            finish: m.vehicle_finish, gear: m.running_gear}).collect();
        // Static world instances are shared; only changed CPU matrices allocate a new payload.
        let batches: Vec<_> = self.inst_batches.iter().enumerate().map(|(index,b)| {
            let mats = previous.and_then(|p| p.batches.iter().find(|p| p.index == index))
                .filter(|p| p.mats.as_slice() == b.cpu_mats.as_slice())
                .map(|p| p.mats.clone()).unwrap_or_else(|| Arc::new(b.cpu_mats.clone()));
            BatchFrame { index, mats, hidden: b.hidden.clone(), emissive: b.emissive, flags: b.flags }
        }).collect();
        let fronts = self.pressure_fronts.clone();
        let heat = bincode::serialize(&self.heat_sources)?;
        let pixels = bincode::serialize(&*self.pixel_particle_field.borrow())?;
        let bytes = meshes.len() * std::mem::size_of::<MeshFrame>()
            + batches.iter().filter(|b| !previous.is_some_and(|p| p.batches.iter()
                .any(|old| old.index==b.index && Arc::ptr_eq(&old.mats,&b.mats))))
                .map(|b| b.mats.len() * 64).sum::<usize>()
            + self.scene_cache_vertices.iter().filter(|(i,v)| !previous.is_some_and(|p|
                p.vertices.get(i).is_some_and(|old|Arc::ptr_eq(old,v))))
                .map(|(_,v)| v.len()*std::mem::size_of::<crate::gltf_load::Vertex>()).sum::<usize>()
            + self.scene_cache_indices.iter().filter(|(i,v)| !previous.is_some_and(|p|
                p.indices.get(i).is_some_and(|old|Arc::ptr_eq(old,v))))
                .map(|(_,v)| v.len()*4).sum::<usize>()
            + self.fine_vfx.particle_ids.values().map(|v| v.len()*16).sum::<usize>()
            + heat.len() + pixels.len();
        Ok(SceneFrame { meshes, batches,
            vertices: self.scene_cache_vertices.clone(), indices: self.scene_cache_indices.clone(),
            scars: self.mesh_scars.clone(), lights: self.lights_full.clone(),
            registered: self.cabin_shadows.lights.borrow().clone(),
            emitters: self.cabin_shadows.emitting_meshes.borrow().clone(),
            smoke: self.smoke_puffs, gas: self.breech_gas, fumes: self.cabin_fumes,
            bursts: self.weapon_particles.bursts.clone(), segments: self.weapon_particles.segments.clone(),
            grain_ids: self.fine_vfx.particle_ids.clone(), grain_materials: self.fine_vfx.materials.clone(),
            medium_pose: self.cabin_medium_pose,
            atmosphere: (self.atmo_enabled, self.atmo_haze, self.atmo_soot, self.atmo_dust, self.atmo_origin),
            fronts, heat, pixels, time: self.time, concussion: self.concussion,
            flash: self.observer_flash_source, wind: self.weather_wind_world, bytes })
    }

    pub(crate) fn restore_scene_frame(&mut self, frame: &SceneFrame, lamp_ratio: f32) -> anyhow::Result<()> {
        anyhow::ensure!(frame.meshes.len() <= self.meshes.len(), "cached mesh slots no longer exist");
        for m in self.meshes.iter_mut().skip(frame.meshes.len()) { m.visible = false; }
        for (m, saved) in self.meshes.iter_mut().zip(&frame.meshes) {
            m.model = saved.model; m.visible = saved.visible; m.emissive = saved.emissive;
            m.local_min = saved.local_min; m.local_max = saved.local_max;
            (m.world_min, m.world_max) = transform_aabb(m.model, m.local_min, m.local_max);
            m.vehicle_finish = saved.finish; m.running_gear = saved.gear;
        }
        let capture = self.scene_cache_enabled;
        self.scene_cache_enabled = false;
        for (&slot, v) in &frame.vertices {
            if !self.scene_cache_vertices.get(&slot).is_some_and(|now| Arc::ptr_eq(now,v)) {
                anyhow::ensure!(self.upload_pose_vertices(slot,v), "cached vertex upload failed at {slot}");
            }
        }
        for (&slot, v) in &frame.indices {
            if !self.scene_cache_indices.get(&slot).is_some_and(|now| Arc::ptr_eq(now,v)) {
                anyhow::ensure!(self.upload_pose_indices(slot,v), "cached index upload failed at {slot}");
            }
        }
        self.scene_cache_enabled = capture;
        self.scene_cache_vertices = frame.vertices.clone(); self.scene_cache_indices = frame.indices.clone();
        // A pool first activated later in the run must disappear when seeking backward.
        let saved: std::collections::HashSet<_> = frame.batches.iter().map(|b| b.index).collect();
        let empty: Vec<_> = self.inst_batches.iter().enumerate().filter(|(i,b)|
            (b.raster_only_dynamic || b.moving_rt_pool) && !saved.contains(i)).map(|(i,_)|i).collect();
        for i in empty {
            if self.inst_batches[i].moving_rt_pool { self.set_moving_instanced_mats(i,&[]); }
            else { self.set_dynamic_instanced_mats(i,&[]); }
        }
        for b in &frame.batches {
            let moving = self.inst_batches[b.index].moving_rt_pool;
            if moving { anyhow::ensure!(self.set_moving_instanced_mats(b.index,&b.mats), "cached moving pool overflow"); }
            else if self.inst_batches[b.index].raster_only_dynamic {
                anyhow::ensure!(self.set_dynamic_instanced_mats(b.index,&b.mats), "cached FX pool overflow");
            }
            else if self.inst_batches[b.index].cpu_mats.as_slice()!=b.mats.as_slice()
                || self.inst_batches[b.index].hidden!=b.hidden {
                let device=self.pose_device.clone();
                self.set_instanced_mats(&device,b.index,&b.mats);
                let batch=&self.inst_batches[b.index];
                let visible:Vec<_>=b.mats.iter().enumerate().map(|(i,m)|
                    if b.hidden.is_hidden(i as u32) {glam::Mat4::ZERO} else {*m}).collect();
                if !visible.is_empty() {
                    self.instance_queue.write_buffer(&batch.inst_buf,batch.inst_byte_offset,bytemuck::cast_slice(&visible));
                }
                self.rt_batches_dirty=true;
            }
            self.inst_batches[b.index].emissive = b.emissive;
            self.inst_batches[b.index].flags = b.flags;
            self.inst_batches[b.index].hidden = b.hidden.clone();
        }
        self.mesh_scars = frame.scars.clone();
        let mut lights = frame.lights.clone();
        let mut registered = frame.registered.clone();
        // Only named cabin fixtures follow the review lamp-power control. Flashes/fire keep power.
        for light in registered.iter_mut().take(CABIN_LIGHT_REGISTRATIONS) {
            if let Some(original) = frame.registered.iter().take(CABIN_LIGHT_REGISTRATIONS)
                .find(|p| p.pos == light.pos && p.source_radius == light.source_radius) {
                for offered in &mut lights {
                    if offered.pos == original.pos && offered.source_radius == original.source_radius {
                        offered.radiant_intensity *= lamp_ratio;
                    }
                }
            }
            light.radiant_intensity *= lamp_ratio;
        }
        for slot in frame.emitters.iter().take(CABIN_LIGHT_REGISTRATIONS).flatten() {
            if let Some(m) = self.meshes.get_mut(*slot) { m.emissive *= lamp_ratio; }
        }
        *self.cabin_shadows.lights.borrow_mut() = registered;
        *self.cabin_shadows.emitting_meshes.borrow_mut() = frame.emitters.clone();
        self.set_lights(&lights);
        self.smoke_puffs = frame.smoke; self.breech_gas = frame.gas; self.cabin_fumes = frame.fumes;
        self.weapon_particles.bursts = frame.bursts.clone(); self.weapon_particles.segments = frame.segments.clone();
        self.fine_vfx.particle_ids = frame.grain_ids.clone(); self.fine_vfx.materials = frame.grain_materials.clone();
        if let Some((h,t)) = frame.medium_pose { self.set_cabin_medium_pose(h,t); }
        (self.atmo_enabled,self.atmo_haze,self.atmo_soot,self.atmo_dust,self.atmo_origin) = frame.atmosphere;
        self.pressure_fronts = frame.fronts.clone();
        self.heat_sources = bincode::deserialize(&frame.heat)?;
        *self.pixel_particle_field.borrow_mut() = bincode::deserialize(&frame.pixels)?;
        self.weather_wind_world = frame.wind;
        self.time = frame.time; self.concussion = frame.concussion; self.observer_flash_source = frame.flash;
        Ok(())
    }
}

impl SceneFrame {
    /// Receipt of cached geometry/particles independent of camera, lighting transport and history.
    pub(crate) fn content_hash(&self) -> u64 {
        use std::hash::{Hash,Hasher};
        let mut h=std::collections::hash_map::DefaultHasher::new();
        for (index,m) in self.meshes.iter().enumerate().filter(|(_,m)|m.visible) {
            index.hash(&mut h);
            for v in m.model.to_cols_array() {v.to_bits().hash(&mut h);}
            m.visible.hash(&mut h);m.emissive.to_bits().hash(&mut h);
        }
        for b in self.batches.iter().filter(|b| !b.mats.is_empty()) {
            b.index.hash(&mut h);bytemuck::cast_slice::<_,u8>(b.mats.as_slice()).hash(&mut h);
            b.hidden.hash(&mut h);b.emissive.to_bits().hash(&mut h);
        }
        let mut slots:Vec<_>=self.vertices.keys().collect();slots.sort();
        for slot in slots {slot.hash(&mut h);bytemuck::cast_slice::<_,u8>(self.vertices[slot].as_slice()).hash(&mut h);}
        let mut slots:Vec<_>=self.indices.keys().collect();slots.sort();
        for slot in slots {slot.hash(&mut h);self.indices[slot].hash(&mut h);}
        bytemuck::cast_slice::<_,u8>(&self.smoke).hash(&mut h);
        bytemuck::cast_slice::<_,u8>(&self.bursts).hash(&mut h);
        bytemuck::cast_slice::<_,u8>(&self.segments).hash(&mut h);
        self.time.to_bits().hash(&mut h);
        h.finish()
    }
}
