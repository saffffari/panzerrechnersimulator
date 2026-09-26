//! Near-field sky visibility from the rendered scene geometry.
//!
//! The stylized backend's diffuse sky and ground fill (`hemisphere()` in stylized_mesh.wgsl) is the
//! environment emitter. Before this module every surface received all of it, so closed armour,
//! a buttoned turret and a roofed room were lit as if they stood in an open field. Here a fixed set
//! of world directions (24 over the sky dome, 8 over the ground hemisphere, equal solid angle
//! within each) is rasterized as orthographic depth tiles of one atlas around the camera. The mesh
//! pass compares each receiver against every tile, so hull steel, hatch lids and roofs block the
//! environment while real openings admit it.
//!
//! Stated bounds (also in docs/PHYSICAL_TRUTH_AUDIT.md): 32 discrete directions, one bounce-free
//! visibility test per direction; terrain is the ground emitter and never an occluder; occluders
//! farther than `REACH_M` beyond the footprint, and receivers outside the footprint, see open sky.
use super::*;
use std::cell::Cell;

pub const UP_DIRECTIONS: usize = 24;
pub const DOWN_DIRECTIONS: usize = 8;
pub const DIRECTIONS: usize = UP_DIRECTIONS + DOWN_DIRECTIONS;
const COLS: u32 = 8;
const ROWS: u32 = 4;
const TILE: u32 = 384;
/// Footprint half extent around the focus. A seated crewman needs the whole hull inside the
/// footprint at fine texels; on foot a room-scale footprint covers walls and roofs.
const INSIDE_HALF_M: f32 = 4.5;
const OUTSIDE_HALF_M: f32 = 12.0;
/// Occluders are captured up to this far beyond the footprint along each direction.
const REACH_M: f32 = 10.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub view_proj: [[[f32; 4]; 4]; DIRECTIONS],
    /// Unit world direction toward the environment; w = solid angle (sr).
    pub directions: [[f32; 4]; DIRECTIONS],
    /// Atlas columns, rows, active directions (0 = open sky everywhere), world texel size (m).
    pub grid: [f32; 4],
}

/// Equal-area Fibonacci directions: 24 over the upper hemisphere, 8 over the lower. Heights are
/// uniformly spaced in y, which is uniform in solid angle on a hemisphere (Archimedes).
pub fn directions() -> [glam::Vec4; DIRECTIONS] {
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    let mut out = [glam::Vec4::ZERO; DIRECTIONS];
    let hemisphere = |count: usize, sign: f32, phase: f32, out: &mut [glam::Vec4]| {
        for (i, slot) in out.iter_mut().enumerate().take(count) {
            let y = 1.0 - (i as f32 + 0.5) / count as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let phi = i as f32 * golden + phase;
            *slot = glam::Vec3::new(r * phi.cos(), sign * y, r * phi.sin())
                .normalize()
                .extend(std::f32::consts::TAU / count as f32);
        }
    };
    hemisphere(UP_DIRECTIONS, 1.0, 0.0, &mut out[..UP_DIRECTIONS]);
    hemisphere(DOWN_DIRECTIONS, -1.0, 0.5, &mut out[UP_DIRECTIONS..]);
    out
}

fn footprint_half(interior: bool) -> f32 { if interior { INSIDE_HALF_M } else { OUTSIDE_HALF_M } }

/// Snap the focus to a coarse lattice so small head motion inside a parked tank reuses the atlas.
fn snapped_focus(focus: glam::Vec3, half: f32) -> glam::Vec3 {
    let step = half / 8.0;
    (focus / step).round() * step
}

fn direction_view_proj(direction: glam::Vec3, focus: glam::Vec3, half: f32) -> glam::Mat4 {
    stable_shadow_view_proj(direction, focus, half, half + REACH_M, TILE)
}

/// Axis-aligned box that holds every direction's footprint and reach, for coarse culling.
fn region_contains(focus: glam::Vec3, half: f32, lo: glam::Vec3, hi: glam::Vec3) -> bool {
    lo.is_finite()
        && hi.is_finite()
        && !lo.cmpgt(hi).any()
        && focus.distance(focus.clamp(lo, hi)) <= half * 3.0_f32.sqrt() + REACH_M
}

#[derive(Clone, Copy)]
struct Prepared {
    focus: glam::Vec3,
    half: f32,
    matrices: [glam::Mat4; DIRECTIONS],
}

pub struct SkyVisibility {
    enabled: bool,
    pub view: wgpu::TextureView,
    composed: wgpu::Texture,
    target: wgpu::TextureView,
    /// Instanced static scenery only. It changes when the footprint moves, not when a vehicle
    /// drives, so moving meshes are drawn over a copy of it rather than redrawing every module.
    static_depth: wgpu::Texture,
    static_target: wgpu::TextureView,
    pub uniform: wgpu::Buffer,
    /// Fragment-stage atlas, comparison sampler and matrices for participating media (group 1 of
    /// the stylized smoke pipelines).
    pub sampling_bgl: wgpu::BindGroupLayout,
    pub sampling_bind: wgpu::BindGroup,
    bind: wgpu::BindGroup,
    pipe: wgpu::RenderPipeline,
    inst_pipe: wgpu::RenderPipeline,
    static_signature: Cell<Option<u64>>,
    dynamic_signature: Cell<Option<u64>>,
    pending_static: Cell<bool>,
    pending_compose: Cell<bool>,
    prepared: Cell<Option<Prepared>>,
    coop_focus: Cell<Option<glam::Vec3>>,
    directions: [glam::Vec4; DIRECTIONS],
    rebuilds: Cell<u64>,
}

impl SkyVisibility {
    pub fn new(device: &wgpu::Device, enabled: bool) -> Self {
        let size = if enabled {
            wgpu::Extent3d { width: COLS * TILE, height: ROWS * TILE, depth_or_array_layers: 1 }
        } else {
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky-visibility-atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let static_depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky-visibility-static-cache"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let static_target = static_depth.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky-visibility"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let compare = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky-visibility-compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let sampling_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-visibility-sampling-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let sampling_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-visibility-sampling-bind"),
            layout: &sampling_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&compare) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-visibility-bgl"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-visibility-bind"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-visibility-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let module = shader(device, "sky-visibility", include_str!("../shaders/sky_visibility.wgsl"));
        let position = [wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }];
        let instance = [
            wgpu::VertexAttribute { offset: 0, shader_location: 4, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 16, shader_location: 5, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 32, shader_location: 6, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { offset: 48, shader_location: 7, format: wgpu::VertexFormat::Float32x4 },
        ];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &position,
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: INSTANCE_MAT_BYTES,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance,
        };
        let make = |label: &str, entry: &str, buffers: &[wgpu::VertexBufferLayout]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(entry),
                    buffers,
                    compilation_options: Default::default(),
                },
                // Thin steel and one-sided authored surfaces block light from either side.
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                // Depth only; the fragment stage just keeps each direction inside its own tile.
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    targets: &[],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            })
        };
        let pipe = make("sky-visibility", "vs", &[vertex_layout.clone()]);
        let inst_pipe = make("sky-visibility-inst", "vs_inst", &[vertex_layout, instance_layout]);
        Self {
            enabled,
            view,
            composed: texture,
            target,
            static_depth,
            static_target,
            uniform,
            sampling_bgl,
            sampling_bind,
            bind,
            pipe,
            inst_pipe,
            static_signature: Cell::new(None),
            dynamic_signature: Cell::new(None),
            pending_static: Cell::new(false),
            pending_compose: Cell::new(false),
            prepared: Cell::new(None),
            coop_focus: Cell::new(None),
            directions: directions(),
            rebuilds: Cell::new(0),
        }
    }

    pub fn set_coop_focus(&self, focus: Option<glam::Vec3>) { self.coop_focus.set(focus); }

    fn participates(m: &MeshGpu) -> bool {
        // Terrain is the ground emitter; view-local split-screen bodies differ per viewport.
        m.visible && !m.glass && !m.raster_only && !m.track_recv
    }

    fn batch_participates(b: &InstBatch) -> bool {
        // Grass and tiny transient FX are excluded exactly as they are from sun shadows.
        b.inst_count > 0 && !b.raster_only_dynamic && b.flags <= 3.5
    }

    fn region_query(focus: glam::Vec3, half: f32) -> glam::Mat4 {
        let e = half * 3.0_f32.sqrt() + REACH_M;
        glam::Mat4::orthographic_rh(-e, e, -e, e, 0.0, 2.0 * e)
            * glam::Mat4::look_to_rh(focus + glam::Vec3::Y * e, -glam::Vec3::Y, glam::Vec3::Z)
    }

    fn footprint_hash(prepared: &Prepared) -> std::collections::hash_map::DefaultHasher {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for f in prepared.focus.to_array() { f.to_bits().hash(&mut hash); }
        prepared.half.to_bits().hash(&mut hash);
        hash
    }

    /// Moving meshes: vehicles, hatches, carried ammunition, crew.
    fn dynamic_signature(&self, renderer: &Renderer, prepared: &Prepared) -> u64 {
        let mut hash = Self::footprint_hash(prepared);
        for (i, m) in renderer.meshes.iter().enumerate() {
            if !Self::participates(m)
                || !region_contains(prepared.focus, prepared.half, m.world_min, m.world_max) {
                continue;
            }
            // Geometry buffers are immutable; their identity is the scene revision.
            i.hash(&mut hash);
            m.index_count.hash(&mut hash);
            m.vbuf.hash(&mut hash);
            m.ibuf.hash(&mut hash);
            for f in m.model.to_cols_array() { f.to_bits().hash(&mut hash); }
        }
        hash.finish()
    }

    /// Instanced static scenery inside the footprint.
    fn static_signature(&self, renderer: &Renderer, prepared: &Prepared) -> u64 {
        let mut hash = Self::footprint_hash(prepared);
        let pages = renderer.inst_spatial_index.query(Self::region_query(prepared.focus, prepared.half));
        for &index in &pages.batches {
            let b = &renderer.inst_batches[index];
            if !Self::batch_participates(b)
                || !region_contains(prepared.focus, prepared.half, b.world_min, b.world_max) {
                continue;
            }
            index.hash(&mut hash);
            b.inst_count.hash(&mut hash);
            b.inst_buffer_id.hash(&mut hash);
            b.prototype.hash(&mut hash);
            renderer.inst_prototypes[b.prototype].vbuf.hash(&mut hash);
            for model in b.cpu_mats.iter().take(b.inst_count as usize) {
                for f in model.to_cols_array() { f.to_bits().hash(&mut hash); }
            }
        }
        hash.finish()
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue) {
        if !self.enabled { return; }
        let half = footprint_half(renderer.interior_light > 0.02 || self.coop_focus.get().is_some());
        let focus = snapped_focus(self.coop_focus.get().unwrap_or(renderer.camera.pos), half);
        if !focus.is_finite() {
            self.prepared.set(None);
            queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&<Uniform as bytemuck::Zeroable>::zeroed()));
            return;
        }
        let mut prepared = Prepared { focus, half, matrices: [glam::Mat4::IDENTITY; DIRECTIONS] };
        let mut uniform = Uniform {
            view_proj: [[[0.0; 4]; 4]; DIRECTIONS],
            directions: [[0.0; 4]; DIRECTIONS],
            grid: [COLS as f32, ROWS as f32, DIRECTIONS as f32, 2.0 * half / TILE as f32],
        };
        for (k, d) in self.directions.iter().enumerate() {
            prepared.matrices[k] = direction_view_proj(d.truncate(), focus, half);
            uniform.view_proj[k] = prepared.matrices[k].to_cols_array_2d();
            uniform.directions[k] = d.to_array();
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.prepared.set(Some(prepared));
        let static_signature = self.static_signature(renderer, &prepared);
        let dynamic_signature = self.dynamic_signature(renderer, &prepared);
        if self.static_signature.replace(Some(static_signature)) != Some(static_signature) {
            self.pending_static.set(true);
            self.pending_compose.set(true);
        }
        if self.dynamic_signature.replace(Some(dynamic_signature)) != Some(dynamic_signature) {
            self.pending_compose.set(true);
        }
    }

    pub fn encode(&self, renderer: &Renderer, encoder: &mut wgpu::CommandEncoder,
        work: &mut RenderWorkStats) {
        // Take both flags together so a skipped frame can never drop a pending static rebuild.
        let rebuild_static = self.pending_static.replace(false);
        let compose = self.pending_compose.replace(false);
        if !self.enabled || !compose { return; }
        let Some(prepared) = self.prepared.get() else { return };
        let mut drawn = (0u32, 0u32);
        if rebuild_static {
            let planes: Vec<_> = prepared.matrices.iter().map(|m| frustum_planes(*m)).collect();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky-visibility-static"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.static_target,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: renderer.gpu_profile.as_ref()
                    .and_then(|p| p.render("sky-visibility-static")),
                occlusion_query_set: None,
            });
            work.shadow_passes += 1;
            pass.set_pipeline(&self.inst_pipe);
            pass.set_bind_group(0, &self.bind, &[]);
            let pages = renderer.inst_spatial_index.query(Self::region_query(prepared.focus, prepared.half));
            for &index in &pages.batches {
                let b = &renderer.inst_batches[index];
                if !Self::batch_participates(b)
                    || !region_contains(prepared.focus, prepared.half, b.world_min, b.world_max) {
                    continue;
                }
                let p = &renderer.inst_prototypes[b.prototype];
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                // Instance-rate matrices already occupy the instance index, so each direction
                // whose view actually contains the batch is its own draw.
                for (k, planes) in planes.iter().enumerate() {
                    if !aabb_in_frustum(planes, b.world_min, b.world_max) { continue; }
                    let push = PushData {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        params: [k as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                    pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                    work.shadow_instanced_draws += 1;
                }
                drawn.1 += 1;
            }
        }
        let whole = |texture| wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        };
        encoder.copy_texture_to_texture(whole(&self.static_depth), whole(&self.composed),
            self.static_depth.size());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky-visibility-dynamic"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.target,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: renderer.gpu_profile.as_ref()
                .and_then(|p| p.render("sky-visibility-dynamic")),
            occlusion_query_set: None,
        });
        work.shadow_passes += 1;
        pass.set_pipeline(&self.pipe);
        pass.set_bind_group(0, &self.bind, &[]);
        for m in &renderer.meshes {
            if !Self::participates(m)
                || !region_contains(prepared.focus, prepared.half, m.world_min, m.world_max) {
                continue;
            }
            let push = PushData { model: m.model.to_cols_array_2d(), params: [0.0; 4] };
            pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
            pass.set_vertex_buffer(0, m.vbuf.slice(..));
            pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            // One instance per direction; the vertex stage places each in its atlas tile.
            pass.draw_indexed(0..m.index_count, 0, 0..DIRECTIONS as u32);
            work.shadow_mesh_draws += 1;
            drawn.0 += 1;
        }
        let rebuilds = self.rebuilds.get() + 1;
        self.rebuilds.set(rebuilds);
        if rebuilds == 1 {
            println!("[sky-visibility] {DIRECTIONS} directions x {TILE}px, footprint +/-{:.1} m: \
                {} meshes, {} instanced batches", prepared.half, drawn.0, drawn.1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_are_unit_equal_area_and_split_cleanly_by_hemisphere() {
        let dirs = directions();
        let mut up = 0.0;
        let mut down = 0.0;
        for (k, d) in dirs.iter().enumerate() {
            assert!((d.truncate().length() - 1.0).abs() < 1e-5);
            assert!(d.y.abs() > 0.01, "no direction may sit exactly on the horizon");
            if k < UP_DIRECTIONS { assert!(d.y > 0.0); up += d.w; } else { assert!(d.y < 0.0); down += d.w; }
        }
        assert!((up - std::f32::consts::TAU).abs() < 1e-4);
        assert!((down - std::f32::consts::TAU).abs() < 1e-4);
        // A cosine lobe about +Y integrates to pi over the upper hemisphere (Lambert).
        let lobe: f32 = dirs[..UP_DIRECTIONS].iter().map(|d| d.y * d.w).sum();
        assert!((lobe - std::f32::consts::PI).abs() < 0.02, "{lobe}");
    }

    #[test]
    fn every_direction_keeps_the_footprint_inside_its_tile_and_depth_range() {
        let focus = glam::Vec3::new(1850.0, 7.0, 250.0);
        for d in directions() {
            let m = direction_view_proj(d.truncate(), focus, INSIDE_HALF_M);
            for corner in [glam::Vec3::splat(-1.0), glam::Vec3::splat(1.0),
                glam::Vec3::new(1.0, -1.0, 1.0), glam::Vec3::new(-1.0, 1.0, -1.0)] {
                // The footprint's inscribed sphere (radius half) must project inside the tile.
                let p = m.project_point3(focus + corner.normalize() * INSIDE_HALF_M * 0.99);
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p:?}");
                assert!(p.z > 0.0 && p.z < 1.0, "{p:?}");
            }
            // An occluder REACH metres toward the environment still lands in the depth range.
            let sky = m.project_point3(focus + d.truncate() * (INSIDE_HALF_M + REACH_M * 0.95));
            assert!(sky.z > 0.0 && sky.z < m.project_point3(focus).z);
        }
    }

    #[test]
    fn atlas_layout_and_head_motion_inside_one_snap_cell_reuse_the_footprint() {
        assert_eq!(std::mem::size_of::<Uniform>(), DIRECTIONS * 64 + DIRECTIONS * 16 + 16);
        assert_eq!(COLS * ROWS, DIRECTIONS as u32);
        // The inside lattice step is 0.5625 m; a seated crewman's head motion stays in one cell.
        let step = INSIDE_HALF_M / 8.0;
        let centre = glam::Vec3::new(18.0, 4.0, 5.0) * step;
        let a = snapped_focus(centre, INSIDE_HALF_M);
        let b = snapped_focus(centre + glam::Vec3::new(0.2, -0.15, 0.25), INSIDE_HALF_M);
        assert_eq!(a, centre);
        assert_eq!(a, b);
        // Crossing the cell boundary moves the footprint by exactly one step.
        let c = snapped_focus(centre + glam::Vec3::X * (step * 0.6), INSIDE_HALF_M);
        assert!((c.x - a.x - step).abs() < 1e-5);
    }
}
