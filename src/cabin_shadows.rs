//! Eight practical lamps plus eight admitted wreck fires, with source-geometry cubemap visibility. Static lamp
//! and caster poses reuse depth; light selection never guesses from brightness or camera distance.
use super::*;
use std::cell::RefCell;

pub const CABIN_LIGHTS: usize = 8;
pub const WORLD_FIRE_LIGHTS: usize = 8;
pub const MAX_LIGHTS: usize = CABIN_LIGHTS + WORLD_FIRE_LIGHTS;
const SIDE: u32 = 512;
// The smaller dome sits 14 mm below the source fan. A 35 mm near plane erased that blocker.
// 5 mm retains it; only the fixture's explicitly registered emitting mesh is excluded.
// Depth32Float error is bounded below 0.4 mm through 4 m (1.6 mm at the 8 m limit).
const NEAR: f32 = 0.005;
const FAR: f32 = 8.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub lamps: [[f32; 4]; MAX_LIGHTS], // world position; enabled, matched to packed source position
    pub range: [f32; 4],               // near, far, texel angular scale, active lights
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct CacheWork {
    rebuild_static: bool,
    compose: bool,
}

#[derive(Clone, Copy, Default)]
struct LampCache {
    static_signature: Option<u64>,
    dynamic_signature: Option<u64>,
    pending: CacheWork,
}
impl LampCache {
    fn observe(&mut self, static_signature: u64, dynamic_signature: u64) -> CacheWork {
        let changed = CacheWork {
            rebuild_static: self.static_signature != Some(static_signature),
            compose: self.static_signature != Some(static_signature)
                || self.dynamic_signature != Some(dynamic_signature),
        };
        self.static_signature = Some(static_signature);
        self.dynamic_signature = Some(dynamic_signature);
        self.pending.rebuild_static |= changed.rebuild_static;
        self.pending.compose |= changed.compose;
        changed
    }
    fn take_work(&mut self) -> CacheWork { std::mem::take(&mut self.pending) }
}

pub struct CabinShadows {
    pub view: wgpu::TextureView,
    composed_depth: wgpu::Texture,
    static_depth: wgpu::Texture,
    static_faces: Vec<wgpu::TextureView>,
    pub uniform: wgpu::Buffer,
    pub sampling_bgl: wgpu::BindGroupLayout,
    pub sampling_bind: wgpu::BindGroup,
    faces: Vec<wgpu::TextureView>,
    cameras: Vec<wgpu::Buffer>,
    binds: Vec<wgpu::BindGroup>,
    lights: RefCell<Vec<PointLight>>,
    emitting_meshes: RefCell<Vec<Option<usize>>>,
    caches: RefCell<[LampCache; MAX_LIGHTS]>,
    dynamic_meshes: Vec<usize>,
    dynamic_flags: Vec<bool>,
}

fn face_view_proj(position: glam::Vec3, face: usize) -> glam::Mat4 {
    let (direction, up) = [
        (glam::Vec3::X, -glam::Vec3::Y),
        (-glam::Vec3::X, -glam::Vec3::Y),
        (glam::Vec3::Y, glam::Vec3::Z),
        (-glam::Vec3::Y, -glam::Vec3::Z),
        (glam::Vec3::Z, -glam::Vec3::Y),
        (-glam::Vec3::Z, -glam::Vec3::Y),
    ][face];
    // WebGPU framebuffer Y grows downward. Flip the projection to match the cube sampler's
    // face orientation, rather than vertically mirroring every off-axis occluder.
    let mut projection = glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, NEAR, FAR);
    projection.y_axis.y = -projection.y_axis.y;
    projection * glam::Mat4::look_to_rh(position, direction, up)
}

fn near_box(position: glam::Vec3, lo: glam::Vec3, hi: glam::Vec3) -> bool {
    lo.is_finite()
        && hi.is_finite()
        && !lo.cmpgt(hi).any()
        && position.distance_squared(position.clamp(lo, hi)) <= FAR * FAR
}

fn active_fixture(lamp: &PointLight) -> bool {
    lamp.pos.is_finite() && lamp.radiant_intensity.is_finite() && lamp.radiant_intensity > 0.0
}

fn casts_for_fixture(mesh: usize, emitting_mesh: Option<usize>) -> bool {
    Some(mesh) != emitting_mesh
}

impl CabinShadows {
    pub fn new(
        device: &wgpu::Device,
        enabled: bool,
        camera_bgl: &wgpu::BindGroupLayout,
        terrain: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        shadow_sampler: &wgpu::Sampler,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cabin-lamp-shadow-cubes"),
            size: wgpu::Extent3d {
                width: if enabled { SIDE } else { 1 },
                height: if enabled { SIDE } else { 1 },
                depth_or_array_layers: (MAX_LIGHTS * 6) as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Machinery is expensive and normally stationary. Keep its exact depth separately,
        // then restore it before drawing moving ammunition so an old shell shadow cannot linger.
        let static_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cabin-static-shadow-cache"),
            size: texture.size(), mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("cabin-lamp-shadow-cube-array"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });
        let mut faces = Vec::new();
        let mut static_faces = Vec::new();
        let mut cameras = Vec::new();
        let mut binds = Vec::new();
        // Two 512² Depth32Float banks × 96 faces = 192 MiB when enabled.
        for face in 0..(MAX_LIGHTS * 6) as u32 {
            static_faces.push(static_depth.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face, array_layer_count: Some(1), ..Default::default()
            }));
            faces.push(texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                ..Default::default()
            }));
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cabin-shadow-camera"),
                size: std::mem::size_of::<CameraUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            binds.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("cabin-shadow-camera-bind"),
                layout: camera_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(terrain),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            }));
            cameras.push(buffer);
        }
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cabin-shadow-light-list"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampling_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cabin-shadow-sampling-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::CubeArray, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let sampling_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cabin-shadow-sampling-bind"), layout: &sampling_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(shadow_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        Self {
            view,
            sampling_bgl,
            sampling_bind,
            composed_depth: texture,
            static_depth,
            static_faces,
            uniform,
            faces,
            cameras,
            binds,
            lights: RefCell::new(Vec::new()),
            emitting_meshes: RefCell::new(Vec::new()),
            caches: RefCell::new([LampCache::default(); MAX_LIGHTS]),
            dynamic_meshes: Vec::new(),
            dynamic_flags: Vec::new(),
        }
    }

    pub fn set_lights(&self, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
        // Preserve off fixtures in their named slots; switching a status lamp cannot reassign
        // another lamp's cached faces. Invalid/off slots stay disabled in the GPU uniform.
        let mut selected = self.lights.borrow_mut();
        selected.clear();
        selected.extend(lights.iter().copied().take(CABIN_LIGHTS));
        let mut emitters = self.emitting_meshes.borrow_mut();
        emitters.clear();
        emitters.extend((0..selected.len()).map(|i| emitting_meshes.get(i).copied().flatten()));
    }

    fn set_world_fire_lights(&self, lights: &[PointLight]) {
        let mut selected = self.lights.borrow_mut();
        let mut emitters = self.emitting_meshes.borrow_mut();
        selected.resize(CABIN_LIGHTS, PointLight { pos:glam::Vec3::ZERO,
            source_radius:0.0,color:glam::Vec3::ZERO,radiant_intensity:0.0 });
        emitters.resize(CABIN_LIGHTS, None);
        selected.extend(lights.iter().copied().take(WORLD_FIRE_LIGHTS));
        emitters.resize(selected.len(), None);
    }

    fn set_dynamic_meshes(&mut self, meshes: &[usize]) {
        self.dynamic_meshes.clear();
        self.dynamic_meshes.extend_from_slice(meshes);
        self.dynamic_meshes.sort_unstable();
        self.dynamic_meshes.dedup();
        self.dynamic_flags = vec![false; self.dynamic_meshes.last().map_or(0, |i| i + 1)];
        for &index in &self.dynamic_meshes { self.dynamic_flags[index] = true; }
        *self.caches.borrow_mut() = [LampCache::default(); MAX_LIGHTS];
    }
    fn is_dynamic(&self, index: usize) -> bool {
        self.dynamic_flags.get(index).copied().unwrap_or(false)
    }

    fn signature(&self, renderer: &Renderer, lamp: PointLight, emitting_mesh: Option<usize>, dynamic: bool) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for f in lamp.pos.to_array() {
            f.to_bits().hash(&mut hash);
        }
        emitting_mesh.hash(&mut hash);
        // Only participating casters define this map. Appending distant world geometry must
        // not invalidate all six faces of an otherwise unchanged cabin fixture.
        for (i, m) in renderer.meshes.iter().enumerate() {
            if !casts_for_fixture(i, emitting_mesh) || self.is_dynamic(i) != dynamic || !m.visible || m.glass || m.raster_only
                || !near_box(lamp.pos, m.world_min, m.world_max) {
                continue;
            }
            // Geometry buffers are immutable. Their identity acts as the scene revision, so a
            // replacement with the same vertex count cannot silently reuse old depth.
            i.hash(&mut hash);
            m.index_count.hash(&mut hash);
            m.vbuf.hash(&mut hash);
            m.ibuf.hash(&mut hash);
            for f in m.model.to_cols_array() {
                f.to_bits().hash(&mut hash);
            }
        }
        if dynamic { return hash.finish(); }
        for (i, b) in renderer.inst_batches.iter().enumerate() {
            if b.inst_count == 0
                || b.raster_only_dynamic
                || b.flags > 3.5
                || !near_box(lamp.pos, b.world_min, b.world_max)
            {
                continue;
            }
            i.hash(&mut hash);
            b.inst_count.hash(&mut hash);
            b.inst_buffer_id.hash(&mut hash);
            b.prototype.hash(&mut hash);
            renderer.inst_prototypes[b.prototype].vbuf.hash(&mut hash);
            renderer.inst_prototypes[b.prototype].ibuf.hash(&mut hash);
            for model in b.cpu_mats.iter().take(b.inst_count as usize) {
                for f in model.to_cols_array() {
                    f.to_bits().hash(&mut hash);
                }
            }
            // Per-instance visibility (failed building modules, revealed rubble) is static depth.
            b.hidden.hash(&mut hash);
        }
        hash.finish()
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue, camera: &CameraUniform) {
        let lights = self.lights.borrow();
        let emitting_meshes = self.emitting_meshes.borrow();
        let mut uniform = Uniform {
            lamps: [[0.0; 4]; MAX_LIGHTS],
            range: [NEAR, FAR, 2.0 / SIDE as f32, 0.0],
        };
        let mut caches = self.caches.borrow_mut();
        for (i, &lamp) in lights.iter().enumerate() {
            // The packed source list can be reordered independently of these stable fixture IDs.
            // A source omitted from that list does not spend shadow rendering work.
            if !active_fixture(&lamp) || !renderer
                .lights
                .iter()
                .any(|l| l.pos.abs_diff_eq(lamp.pos, 0.0001))
            {
                continue;
            }
            uniform.lamps[i] = [lamp.pos.x, lamp.pos.y, lamp.pos.z, 1.0];
            uniform.range[3] += 1.0;
            let changed = caches[i].observe(self.signature(renderer, lamp, emitting_meshes[i], false),
                self.signature(renderer, lamp, emitting_meshes[i], true));
            if !changed.compose { continue; }
            for face in 0..6 {
                let cam = CameraUniform {
                    light_view_proj: face_view_proj(lamp.pos, face).to_cols_array_2d(),
                    ..*camera
                };
                queue.write_buffer(&self.cameras[i * 6 + face], 0, bytemuck::bytes_of(&cam));
            }
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        work: &mut RenderWorkStats,
    ) {
        let lights = self.lights.borrow();
        let emitting_meshes = self.emitting_meshes.borrow();
        for (light, &lamp) in lights.iter().enumerate() {
            if !active_fixture(&lamp) { continue; }
            let changed = self.caches.borrow_mut()[light].take_work();
            if !changed.compose { continue; }
            let emitting_mesh = emitting_meshes[light];
            for face in 0..6 {
                let layer = light * 6 + face;
                let matrix = face_view_proj(lamp.pos, face);
                let planes = frustum_planes(matrix);
                if changed.rebuild_static {
                    let pages = renderer.inst_spatial_index.query(matrix);
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("cabin-shadow-static"), color_attachments: &[],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.static_faces[layer],
                            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store }), stencil_ops: None,
                        }),
                        timestamp_writes: renderer.gpu_profile.as_ref().and_then(|p| p.render("cabin-shadow-static")),
                        occlusion_query_set: None,
                    });
                    work.shadow_passes += 1;
                    pass.set_pipeline(&renderer.shadow_pipe);
                    pass.set_bind_group(0, &self.binds[layer], &[]);
                    for (index, m) in renderer.meshes.iter().enumerate() {
                        if !casts_for_fixture(index, emitting_mesh) || self.is_dynamic(index) || !m.visible || m.glass || m.raster_only
                            || !near_box(lamp.pos, m.world_min, m.world_max)
                            || !aabb_in_frustum(&planes, m.world_min, m.world_max) { continue; }
                        let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
                        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                        pass.set_vertex_buffer(0, m.vbuf.slice(..));
                        pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.index_count, 0, 0..1);
                        work.shadow_mesh_draws += 1;
                    }
                    pass.set_pipeline(&renderer.shadow_inst_pipe);
                    for &index in &pages.batches {
                        let b = &renderer.inst_batches[index];
                        if b.inst_count == 0 || b.raster_only_dynamic || b.flags > 3.5
                            || !near_box(lamp.pos, b.world_min, b.world_max) { continue; }
                        let p = &renderer.inst_prototypes[b.prototype];
                        let push = PushData { model: glam::Mat4::IDENTITY.to_cols_array_2d(), params: [0.0; 4] };
                        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                        pass.set_vertex_buffer(0, p.vbuf.slice(..));
                        pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                        pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                        work.shadow_instanced_draws += 1;
                    }
                }
                // Always restore static depth on a dynamic change, including when the last
                // loose case becomes hidden. Loading last frame's composed depth leaves trails.
                copy_cached_depth_face(encoder, &self.static_depth, &self.composed_depth, layer as u32);
                let visible_dynamic: Vec<_> = self.dynamic_meshes.iter().filter_map(|&index| {
                    let m = renderer.meshes.get(index)?;
                    (casts_for_fixture(index, emitting_mesh) && m.visible && !m.glass && !m.raster_only
                        && near_box(lamp.pos, m.world_min, m.world_max)
                        && aabb_in_frustum(&planes, m.world_min, m.world_max)).then_some(m)
                }).collect();
                if visible_dynamic.is_empty() { continue; }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cabin-shadow-ammunition"), color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.faces[layer],
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store }), stencil_ops: None,
                    }),
                    timestamp_writes: renderer.gpu_profile.as_ref().and_then(|p| p.render("cabin-shadow-ammunition")),
                    occlusion_query_set: None,
                });
                work.shadow_passes += 1;
                pass.set_pipeline(&renderer.shadow_pipe);
                pass.set_bind_group(0, &self.binds[layer], &[]);
                for m in visible_dynamic {
                    let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                    work.shadow_mesh_draws += 1;
                }
            }
        }
    }

}

fn copy_cached_depth_face(encoder: &mut wgpu::CommandEncoder, source: &wgpu::Texture,
    target: &wgpu::Texture, layer: u32) {
    let copy = |texture| wgpu::TexelCopyTextureInfo { texture, mip_level: 0,
        origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::DepthOnly };
    encoder.copy_texture_to_texture(copy(source), copy(target), wgpu::Extent3d {
        width: source.width(), height: source.height(), depth_or_array_layers: 1,
    });
}

impl Renderer {
    /// Stable named fixture selection supplied by Game, independent of light sorting or exposure.
    /// Ammunition remains a real shadow caster, but cannot invalidate static machinery depth.
    /// Register the actual finite pool once; visibility/model changes drive composed depth later.
    pub fn set_cabin_shadow_dynamic_meshes(&mut self, meshes: &[usize]) {
        self.cabin_shadows.set_dynamic_meshes(meshes);
    }
    pub fn set_shadowed_cabin_lights(&mut self, lights: &[PointLight], emitting_meshes: &[Option<usize>]) {
        self.cabin_shadows.set_lights(lights, emitting_meshes);
    }
    /// The same eight exterior sources admitted to the flame pool. Their own rendered hull,
    /// nearby terrain and walls cast into the shared 8 m, 512-square visibility bank.
    pub fn set_shadowed_world_fire_lights(&mut self, lights: &[PointLight]) {
        self.cabin_shadows.set_world_fire_lights(lights);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cabin_and_wreck_shadow_slots_share_the_expanded_shader_abi() {
        assert_eq!(CABIN_LIGHTS, 8);
        assert_eq!(WORLD_FIRE_LIGHTS, 8);
        assert_eq!(std::mem::size_of::<Uniform>(), 272);
        assert_eq!(std::mem::offset_of!(Uniform, range), 256);
    }


    #[test]
    fn moving_ammunition_preserves_static_depth_and_settled_frames_do_no_work() {
        let mut cache = LampCache::default();
        let mut static_passes = 0;
        let mut depth_copies = 0;
        // The same cabin sees a carried shell move through 120 distinct poses.
        // Cubemap work is six faces per lamp; static geometry must render only once.
        for pose in 0..120 {
            cache.observe(42, pose);
            let work = cache.take_work();
            static_passes += usize::from(work.rebuild_static) * 6;
            depth_copies += usize::from(work.compose) * 6;
        }
        assert_eq!(static_passes, 6);
        assert_eq!(depth_copies, 720);
        for _ in 0..120 {
            cache.observe(42, 119);
            assert_eq!(cache.take_work(), CacheWork::default());
        }
    }

    #[test]
    fn disappearing_ammunition_restores_depth_without_redrawing_the_cabin() {
        let mut cache = LampCache::default();
        cache.observe(42, 100); // one visible shell
        cache.take_work();
        // Dropping/moving the shell, hiding it, and recycling its visible pool slot each
        // change the participating caster set. A hidden pool removal changes nothing.
        for (casters, changed) in [(101, true), (0, true), (0, false), (102, true)] {
            cache.observe(42, casters);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: false,
                compose: changed,
            });
        }
    }

    #[test]
    fn machinery_or_lamp_changes_rebuild_both_depth_layers() {
        let mut cache = LampCache::default();
        cache.observe(10, 1);
        cache.take_work();
        // Opening/closing a hatch and moving the lamp all change its exact static key.
        for static_key in [11, 10, 12] {
            cache.observe(static_key, 1);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: true,
                compose: true,
            });
        }
    }

    #[test]
    fn light_reordering_cannot_reuse_the_other_fixture_depth() {
        let mut caches = [LampCache::default(); 2];
        for (cache, fixture) in caches.iter_mut().zip([100, 200]) {
            cache.observe(fixture, fixture + 1);
            cache.take_work();
        }
        // Signatures include each lamp's exact position, so swapping packed light slots
        // must rebuild each destination cubemap rather than merely swapping uniforms.
        for (cache, fixture) in caches.iter_mut().zip([200, 100]) {
            cache.observe(fixture, fixture + 1);
            assert_eq!(cache.take_work(), CacheWork {
                rebuild_static: true,
                compose: true,
            });
            cache.observe(fixture, fixture + 1);
            assert_eq!(cache.take_work(), CacheWork::default());
        }
    }

    #[test]
    fn repeated_preparation_cannot_cancel_unencoded_cache_work() {
        let mut cache = LampCache::default();
        cache.observe(42, 1);
        // A second prepare, including a late ammunition update, must not discard the
        // initial static rebuild before the command encoder has consumed it.
        cache.observe(42, 2);
        cache.observe(42, 2);
        assert_eq!(cache.take_work(), CacheWork {
            rebuild_static: true,
            compose: true,
        });
        assert_eq!(cache.take_work(), CacheWork::default());
        cache.observe(42, 3);
        cache.observe(42, 3);
        assert_eq!(cache.take_work(), CacheWork {
            rebuild_static: false,
            compose: true,
        });
    }

    #[test]
    fn sun_shadow_lattice_stays_fixed_for_subtexel_camera_motion() {
        let sun = glam::Vec3::new(-0.5, 0.55, 0.25).normalize();
        let light = glam::Mat4::look_to_rh(glam::Vec3::ZERO, -sun, glam::Vec3::Y);
        let texel = 12.0 / 2048.0;
        let a = stable_shadow_view_proj(sun, glam::Vec3::ZERO, 6.0, 60.0, 2048);
        let shift = light
            .inverse()
            .transform_vector3(glam::Vec3::new(0.2, -0.2, 0.2) * texel);
        let b = stable_shadow_view_proj(sun, shift, 6.0, 60.0, 2048);
        assert!(a.abs_diff_eq(b, 1e-7));
        let c = stable_shadow_view_proj(
            sun,
            light.inverse().transform_vector3(glam::Vec3::X * texel),
            6.0,
            60.0,
            2048,
        );
        let projected = a.project_point3(glam::Vec3::ZERO) - c.project_point3(glam::Vec3::ZERO);
        assert!((projected.x.abs() - 2.0 / 2048.0).abs() < 1e-7);
    }

    #[test]
    fn cabin_sun_cascade_resolves_machinery_and_near_vertical_sun() {
        for sun in [glam::Vec3::Y, glam::Vec3::new(-0.5, 0.55, 0.25).normalize()] {
            let matrix = stable_shadow_view_proj(sun, glam::Vec3::ZERO, 6.0, 60.0, 2048);
            assert!(matrix.is_finite());
            let view_axis = if sun == glam::Vec3::Y {
                glam::Vec3::X
            } else {
                sun.cross(glam::Vec3::Y).normalize()
            };
            let separation =
                matrix.project_point3(view_axis * 0.008) - matrix.project_point3(glam::Vec3::ZERO);
            assert!(
                separation.truncate().length() * 1024.0 > 1.0,
                "8mm machinery must span a near-map texel"
            );
            let toward_sun = matrix.project_point3(sun * 0.008).z;
            let receiver = matrix.project_point3(glam::Vec3::ZERO).z;
            assert!(
                receiver - toward_sun > 0.0075 / 119.9,
                "8mm contact must exceed millimetre receiver bias"
            );
        }
        assert_eq!(std::mem::size_of::<NearShadowUniform>(), 80);
        assert_eq!(std::mem::size_of::<Uniform>(), 272);
    }

    #[test]
    fn close_dome_fan_remains_a_blocker_but_only_own_emitter_is_excluded() {
        let position = crate::interior_rig::TURRET_DOME_LAMP_POS;
        let underside = crate::interior_rig::TURRET_FAN_UNDERSIDE_Y;
        let separation = underside - position.y;
        assert!((separation - crate::interior_rig::TURRET_DOME_LAMP_HALF_SIZE).abs() < 1e-6);
        assert!(NEAR < separation && separation < 0.035);
        let projection = face_view_proj(position, 2);
        let blocker = projection.project_point3(glam::Vec3::new(position.x, underside, position.z));
        assert!((0.0..1.0).contains(&blocker.z), "actual fan underside must survive near clipping");
        let behind = projection.project_point3(position + glam::Vec3::Y * 0.05);
        assert!(blocker.z < behind.z, "fan must occlude receivers above the dome");
        // Exclusion is an exact per-fixture mesh identity, never an emissive-material class.
        assert!(!casts_for_fixture(17, Some(17)));
        assert!(casts_for_fixture(18, Some(17))); // fan / another lamp remain casters
        assert!(casts_for_fixture(17, Some(18)));
        assert!(casts_for_fixture(17, None));
        // Largest floating-point depth step at FAR stays below the existing receiver bias.
        let depth_step_m = f32::EPSILON * FAR * (FAR - NEAR) / NEAR;
        // At the full 8 m range the worst bound is 1.53 mm; at cabin distances <=4 m, <0.4 mm.
        assert!(depth_step_m < 0.0016);
        assert!(depth_step_m * 0.25 < 0.0007);
    }

    #[test]
    fn cube_face_depth_matches_comparison_reference_on_all_axes() {
        let position = glam::Vec3::new(17.0, 2.0, -4.0);
        for (face, axis) in [
            glam::Vec3::X,
            -glam::Vec3::X,
            glam::Vec3::Y,
            -glam::Vec3::Y,
            glam::Vec3::Z,
            -glam::Vec3::Z,
        ]
        .into_iter()
        .enumerate()
        {
            for distance in [0.006, 0.027, 0.05, 0.2, 1.0, 6.0] {
                let point = position + axis * distance;
                // WGSL also subtracts the represented world point from the represented lamp.
                // At x=17 m, a requested 6 mm becomes 6.000519 mm in f32; comparing against
                // the unrepresentable request adds 7.2e-5 depth error before projection rounding.
                let represented_major = (point - position).dot(axis);
                let p = face_view_proj(position, face).project_point3(point);
                let reference = FAR / (FAR - NEAR)
                    - FAR * NEAR / ((FAR - NEAR) * represented_major);
                assert!(
                    (p.z - reference).abs() < 0.0001,
                    "face {face}: {p:?} expected {reference}, major {represented_major}"
                );
                assert!(p.x.abs() < 0.0001 && p.y.abs() < 0.0001);
            }
        }
    }

    #[test]
    fn cube_face_off_axis_projection_matches_sampler_uv_orientation() {
        for (face, dir) in [
            glam::Vec3::new(1.0, 0.2, -0.3),
            glam::Vec3::new(-1.0, 0.2, 0.3),
            glam::Vec3::new(0.2, 1.0, -0.3),
            glam::Vec3::new(0.2, -1.0, 0.3),
            glam::Vec3::new(0.2, -0.3, 1.0),
            glam::Vec3::new(-0.2, 0.3, -1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let p = face_view_proj(glam::Vec3::ZERO, face).project_point3(dir);
            let uv = glam::Vec2::new(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
            let st = match face {
                0 => glam::Vec2::new(-dir.z, -dir.y) / dir.x,
                1 => glam::Vec2::new(dir.z, -dir.y) / (-dir.x),
                2 => glam::Vec2::new(dir.x, dir.z) / dir.y,
                3 => glam::Vec2::new(dir.x, -dir.z) / (-dir.y),
                4 => glam::Vec2::new(dir.x, -dir.y) / dir.z,
                _ => glam::Vec2::new(-dir.x, -dir.y) / (-dir.z),
            };
            assert!(
                uv.abs_diff_eq(st * 0.5 + glam::Vec2::splat(0.5), 0.0001),
                "face {face}: {uv:?}"
            );
        }
    }
}
