//! GPU side of the spring meadow (`meadow.rs`, `shaders/rm_meadow.wgsl`) on the raymarch backend.
//! Per frame the CPU lists the 2 m world tiles each view needs (the camera within the draw
//! radius and its frustum; the near sun cascade and the sky-cover map within the shadow radius),
//! assigns each tile the coarsest level whose blade capacity holds the density the footprint LOD
//! asks at the tile's nearest point, and draws one instanced call per level: the vertex shader
//! builds every blade from the tile's seed. The camera also draws the flowers. The textures are
//! the drawn terrain grid's heights, the ground cover, the wind grid's bend and the tread contact
//! pages.
//!
//! Named limits: blades and flowers are primary raster geometry plus the near sun cascade and the
//! sky-cover map (not the far cascade, not the point-light faces, not the ray-query TLAS, so the
//! reflection and ray-traced visibility passes do not see them); flowers cast no shadow.
use glam::{Mat4, Vec2, Vec3};

use super::{frustum_planes, Renderer, DEPTH};
use crate::meadow::{self, LEVELS};

const VIEWS: usize = 4;
const CAMERA: usize = 3;
/// Tile instances per view (camera blades and flowers share the camera's region).
const MAX_TILES: usize = 16_384;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuView {
    vp: [[f32; 4]; 4],
    origin: [f32; 4],
    eye: [f32; 4],
    lod: [f32; 4],
    wind: [f32; 4],
    map: [f32; 4],
    air: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MeadowStats {
    /// Tiles drawn per view (far cascade, near cascade, sky cover, camera) and camera tiles per level.
    pub tiles: [usize; VIEWS],
    pub camera_levels: [usize; LEVELS],
    pub flower_tiles: usize,
    /// Upper bound of blades the camera's draws can emit this frame (capacity of its tiles).
    pub camera_blade_capacity: u64,
    pub track_pages: usize,
    pub track_pages_dropped: usize,
}

pub struct MeadowGpu {
    view_bufs: Vec<wgpu::Buffer>,
    binds: Vec<wgpu::BindGroup>,
    wind_tex: wgpu::Texture,
    track_table: wgpu::Texture,
    track_atlas: wgpu::Texture,
    tiles: wgpu::Buffer,
    /// Per view: per level (first instance, count); the camera's flower range.
    ranges: [[(u32, u32); LEVELS]; VIEWS],
    flowers: (u32, u32),
    blade_gbuffer: wgpu::RenderPipeline,
    flower_gbuffer: wgpu::RenderPipeline,
    blade_depth: wgpu::RenderPipeline,
    heights: Vec<f32>,
    height_res: usize,
    height_spacing: f32,
    cover_half: f32,
    pending: Option<meadow::Frame>,
    wind_origin: Vec2,
    mean_wind: Vec2,
    time: f32,
    track_pages: usize,
    pub stats: MeadowStats,
    skip: bool,
}

fn tex(device: &wgpu::Device, label: &str, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1,
        sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
    })
}
fn write(queue: &wgpu::Queue, t: &wgpu::Texture, origin: [u32; 2], size: [u32; 2], bytes_per_texel: u32, data: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d { x: origin[0], y: origin[1], z: 0 }, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size[0] * bytes_per_texel), rows_per_image: Some(size[1]) },
        wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
    );
}

/// The program's source: the generated constants, then the shader.
pub(super) fn shader_source() -> String {
    format!("{}\n{}", meadow::wgsl_constants(), include_str!("../shaders/rm_meadow.wgsl"))
}

impl MeadowGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, build: meadow::Build) -> Self {
        use wgpu::ShaderStages as S;
        let res = build.height_res as u32;
        let heights_tex = tex(device, "meadow-heights", res, res, wgpu::TextureFormat::R32Float);
        write(queue, &heights_tex, [0, 0], [res, res], 4, bytemuck::cast_slice(&build.heights));
        let cover_res = build.cover.res as u32;
        let cover_tex = tex(device, "meadow-cover", cover_res, cover_res, wgpu::TextureFormat::Rg8Unorm);
        write(queue, &cover_tex, [0, 0], [cover_res, cover_res], 2, &build.cover.rg);
        let n = meadow::WIND_CELLS as u32;
        let wind_tex = tex(device, "meadow-wind", n, n, wgpu::TextureFormat::Rg32Float);
        write(queue, &wind_tex, [0, 0], [n, n], 8, &vec![0u8; (n * n * 8) as usize]);
        let side = meadow::TRACK_TABLE_SIDE as u32;
        let track_table = tex(device, "meadow-track-table", side, side, wgpu::TextureFormat::R32Uint);
        write(queue, &track_table, [0, 0], [side, side], 4, &vec![0u8; (side * side * 4) as usize]);
        let atlas_side = meadow::TRACK_ATLAS_PAGES * meadow::TRACK_PAGE as u32;
        let track_atlas = tex(device, "meadow-track-atlas", atlas_side, atlas_side, wgpu::TextureFormat::R8Uint);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("meadow-cover"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge, ..Default::default()
        });
        let t = |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
            binding, visibility: S::VERTEX, count: None,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meadow"), entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: S::VERTEX, count: None,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None } },
                t(1, wgpu::TextureSampleType::Float { filterable: false }),
                t(2, wgpu::TextureSampleType::Float { filterable: true }),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: S::VERTEX, count: None, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering) },
                t(4, wgpu::TextureSampleType::Float { filterable: false }),
                t(5, wgpu::TextureSampleType::Uint),
                t(6, wgpu::TextureSampleType::Uint),
            ],
        });
        let view_bufs: Vec<wgpu::Buffer> = (0..VIEWS).map(|i| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("meadow-view-{i}")), size: std::mem::size_of::<GpuView>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        })).collect();
        let views = [heights_tex.create_view(&Default::default()), cover_tex.create_view(&Default::default()),
            wind_tex.create_view(&Default::default()), track_table.create_view(&Default::default()), track_atlas.create_view(&Default::default())];
        let binds = view_bufs.iter().map(|buf| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("meadow"), layout: &bgl, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[0]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[1]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&views[2]) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&views[3]) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&views[4]) },
            ],
        })).collect();
        let tiles = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("meadow-tiles"), size: (VIEWS * MAX_TILES * 16) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rm-meadow"), source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("meadow-pl"), bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
        const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Sint32x4];
        let instance = wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &ATTRS };
        let targets: Vec<Option<wgpu::ColorTargetState>> = (0..3).map(|_| Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL })).collect();
        let gbuffer = |label: &str, entry: &str| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some(entry), buffers: std::slice::from_ref(&instance), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs_gbuffer"), targets: &targets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let blade_gbuffer = gbuffer("meadow-blades", "vs_blade");
        let flower_gbuffer = gbuffer("meadow-flowers", "vs_flower");
        let blade_depth = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("meadow-blades-depth"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_blade"), buffers: std::slice::from_ref(&instance), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 } }),
            multisample: Default::default(), fragment: None, multiview: None, cache: None,
        });
        println!("[meadow] installed: heights {}x{} at {:.2} m, cover {}x{} over {:.0} m, {} species of flowers",
            res, res, build.height_spacing, cover_res, cover_res, 2.0 * build.cover.half_extent, meadow::SPECIES.len());
        Self {
            view_bufs, binds, wind_tex, track_table, track_atlas, tiles,
            ranges: [[(0, 0); LEVELS]; VIEWS], flowers: (0, 0), blade_gbuffer, flower_gbuffer, blade_depth,
            heights: build.heights, height_res: build.height_res, height_spacing: build.height_spacing, cover_half: build.cover.half_extent,
            pending: None, wind_origin: Vec2::ZERO, mean_wind: Vec2::ZERO, time: 0.0, track_pages: 0,
            stats: MeadowStats::default(),
            skip: std::env::var("KESSEL_MEADOW_SKIP").as_deref() == Ok("1"),
        }
    }

    pub fn set_frame(&mut self, frame: meadow::Frame) {
        // Keep pages of a frame that was never presented.
        if let Some(old) = self.pending.take() {
            let mut frame = frame;
            let mut pages = old.track_pages;
            pages.retain(|p| !frame.track_pages.iter().any(|q| q.0 == p.0));
            pages.append(&mut frame.track_pages);
            frame.track_pages = pages;
            self.pending = Some(frame);
        } else {
            self.pending = Some(frame);
        }
    }

    /// The drawn grid's height (as `md_ground`).
    fn ground(&self, x: f32, z: f32) -> f32 {
        let res = self.height_res;
        let half = (res as f32 - 1.0) * 0.5;
        let (gx, gz) = (x / self.height_spacing + half, z / self.height_spacing + half);
        let last = res.saturating_sub(2) as f32;
        let (x0, z0) = (gx.floor().clamp(0.0, last), gz.floor().clamp(0.0, last));
        let (tx, tz) = ((gx - x0).clamp(0.0, 1.0), (gz - z0).clamp(0.0, 1.0));
        let (i, j) = (x0 as usize, z0 as usize);
        let h = |a: usize, b: usize| self.heights[b * res + a];
        if tx + tz <= 1.0 { h(i, j) + (h(i + 1, j) - h(i, j)) * tx + (h(i, j + 1) - h(i, j)) * tz }
        else { h(i + 1, j + 1) + (h(i, j + 1) - h(i + 1, j + 1)) * (1.0 - tx) + (h(i + 1, j) - h(i + 1, j + 1)) * (1.0 - tz) }
    }

    /// Per frame from `upload_uniforms`: the simulation's frame, the tiles of every view, the views.
    pub(super) fn prepare(&mut self, r: &Renderer, shadow_views: &[Mat4], shadow_texels: [f32; 3], queue: &wgpu::Queue) {
        if let Some(frame) = self.pending.take() {
            let n = meadow::WIND_CELLS as u32;
            write(queue, &self.wind_tex, [0, 0], [n, n], 8, bytemuck::cast_slice(&frame.wind));
            self.wind_origin = frame.wind_origin;
            self.mean_wind = frame.mean_wind;
            self.time = frame.time;
            let half = meadow::TRACK_TABLE_SIDE / 2;
            let page = meadow::TRACK_PAGE as u32;
            for (key, slot, cells) in &frame.track_pages {
                let entry = (slot + 1).to_le_bytes();
                write(queue, &self.track_table, [(key.0 + half) as u32, (key.1 + half) as u32], [1, 1], 4, &entry);
                let (sx, sz) = (slot % meadow::TRACK_ATLAS_PAGES, slot / meadow::TRACK_ATLAS_PAGES);
                write(queue, &self.track_atlas, [sx * page, sz * page], [page, page], 1, cells);
                self.track_pages = self.track_pages.max(*slot as usize + 1);
            }
        }
        let cam = &r.camera;
        let eye = cam.pos;
        let pixel_angle = 2.0 * (cam.fov_y * 0.5).tan() / r.ih as f32;
        let mut tiles: Vec<[i32; 4]> = Vec::with_capacity(VIEWS * MAX_TILES);
        let mut stats = MeadowStats { track_pages: self.track_pages, ..MeadowStats::default() };
        let eye2 = Vec2::new(eye.x, eye.z);
        let (lo_tile, hi_tile) = (meadow::tile_of(eye2 - Vec2::splat(meadow::DRAW_RADIUS_M)), meadow::tile_of(eye2 + Vec2::splat(meadow::DRAW_RADIUS_M)));
        let bound = self.cover_half;
        // Each tile once: its box (ground +- a margin, up to the tallest flower) and nearest distance.
        let mut candidates: Vec<(i32, i32, Vec3, Vec3, f32)> = Vec::new();
        for tz in lo_tile.1..=hi_tile.1 {
            for tx in lo_tile.0..=hi_tile.0 {
                let x0 = tx as f32 * meadow::TILE_M;
                let z0 = tz as f32 * meadow::TILE_M;
                if x0 + meadow::TILE_M <= -bound || z0 + meadow::TILE_M <= -bound || x0 >= bound || z0 >= bound { continue; }
                let near = Vec2::new(eye.x.clamp(x0, x0 + meadow::TILE_M), eye.z.clamp(z0, z0 + meadow::TILE_M));
                let horizontal = near.distance(eye2);
                if horizontal > meadow::DRAW_RADIUS_M { continue; }
                let corners = [self.ground(x0, z0), self.ground(x0 + meadow::TILE_M, z0), self.ground(x0, z0 + meadow::TILE_M),
                    self.ground(x0 + meadow::TILE_M, z0 + meadow::TILE_M)];
                let (gmin, gmax) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), &h| (a.min(h), b.max(h)));
                let lo = Vec3::new(x0, gmin - 0.4, z0);
                let hi = Vec3::new(x0 + meadow::TILE_M, gmax + 0.9, z0 + meadow::TILE_M);
                let dy = if eye.y > gmax { eye.y - gmax } else if eye.y < gmin { gmin - eye.y } else { 0.0 };
                let distance = (horizontal * horizontal + dy * dy).sqrt();
                candidates.push((tx, tz, lo, hi, distance));
            }
        }
        let mut views = [GpuView { vp: Mat4::IDENTITY.to_cols_array_2d(), origin: [0.0; 4], eye: [0.0; 4], lod: [0.0; 4], wind: [0.0; 4], map: [0.0; 4], air: [0.0; 4] }; VIEWS];
        self.ranges = [[(0, 0); LEVELS]; VIEWS];
        self.flowers = (0, 0);
        for view in [1usize, 2, CAMERA] {
            let camera = view == CAMERA;
            let (vp_planes, radius, texel) = if camera {
                (cam.view_proj(), meadow::DRAW_RADIUS_M, 0.0)
            } else {
                (shadow_views.get(view).copied().unwrap_or(Mat4::IDENTITY), meadow::SHADOW_RADIUS_M, shadow_texels[view])
            };
            let planes = frustum_planes(vp_planes);
            let base = view * MAX_TILES;
            let mut by_level: [Vec<[i32; 4]>; LEVELS] = std::array::from_fn(|_| Vec::new());
            let mut flower_tiles: Vec<[i32; 4]> = Vec::new();
            for &(tx, tz, lo, hi, distance) in &candidates {
                if distance > radius + meadow::TILE_M * 1.5 || !super::aabb_in_frustum(&planes, lo, hi) { continue; }
                let level = meadow::tile_level(meadow::blade_density(distance, pixel_angle, texel));
                by_level[level as usize].push([tx, tz, level as i32, 0]);
                if camera && distance <= meadow::FLOWER_RADIUS_M { flower_tiles.push([tx, tz, 0, 0]); }
            }
            let start = tiles.len();
            for (level, list) in by_level.iter().enumerate() {
                let reserve = if camera { flower_tiles.len().min(MAX_TILES / 2) } else { 0 };
                let take = list.len().min(MAX_TILES.saturating_sub(tiles.len() - start + reserve));
                self.ranges[view][level] = ((base + tiles.len() - start) as u32, take as u32);
                tiles.extend_from_slice(&list[..take]);
                if camera {
                    stats.camera_levels[level] = take;
                    stats.camera_blade_capacity += take as u64 * meadow::level_capacity(level as u32) as u64;
                }
            }
            if camera {
                let take = flower_tiles.len().min(MAX_TILES.saturating_sub(tiles.len() - start));
                self.flowers = ((base + tiles.len() - start) as u32, take as u32);
                tiles.extend_from_slice(&flower_tiles[..take]);
                stats.flower_tiles = take;
            }
            stats.tiles[view] = tiles.len() - start;
            // Write this view's region.
            queue.write_buffer(&self.tiles, (base * 16) as u64, bytemuck::cast_slice(&tiles[start..]));
            let (vp, origin) = if camera { (cam.relative_view_proj(), eye) } else { (vp_planes, Vec3::ZERO) };
            views[view] = GpuView {
                vp: vp.to_cols_array_2d(),
                origin: [origin.x, origin.y, origin.z, self.time],
                eye: [eye.x, eye.y, eye.z, pixel_angle],
                lod: [texel, radius, if camera { 1.0 } else { 0.0 }, 0.0],
                wind: [self.wind_origin.x, self.wind_origin.y, meadow::WIND_CELL_M, meadow::WIND_CELLS as f32],
                map: [self.height_spacing, self.height_res as f32, self.cover_half, meadow::TRACK_TABLE_SIDE as f32],
                air: [self.mean_wind.x, self.mean_wind.y, meadow::TRACK_ATLAS_PAGES as f32, 0.0],
            };
        }
        for (view, buf) in views.iter().zip(&self.view_bufs) { queue.write_buffer(buf, 0, bytemuck::bytes_of(view)); }
        self.stats = stats;
    }

    /// Draw one view's meadow: blades for the near cascade (1), the sky cover (2) and the camera
    /// (3, with the flowers), depth-only for 1 and 2.
    pub(super) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, view: usize, work: &mut super::RenderWorkStats) {
        if self.skip || view == 0 { return; }
        let camera = view == CAMERA;
        pass.set_pipeline(if camera { &self.blade_gbuffer } else { &self.blade_depth });
        pass.set_bind_group(0, &self.binds[view], &[]);
        pass.set_vertex_buffer(0, self.tiles.slice(..));
        for level in 0..LEVELS {
            let (first, count) = self.ranges[view][level];
            if count == 0 { continue; }
            let l = level as u32;
            let vertices = meadow::level_capacity(l) * meadow::level_vertices(l);
            pass.draw(0..vertices, first..first + count);
            if camera { work.scene_instanced_draws += 1; } else { work.shadow_instanced_draws += 1; }
        }
        if camera && self.flowers.1 > 0 {
            pass.set_pipeline(&self.flower_gbuffer);
            pass.set_bind_group(0, &self.binds[view], &[]);
            pass.draw(0..meadow::FLOWER_SLOTS * meadow::FLOWER_VERTICES, self.flowers.0..self.flowers.0 + self.flowers.1);
            work.scene_instanced_draws += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_meadow_program_validates() {
        let source = super::shader_source();
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("rm_meadow: {}", error.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(), wgpu::naga::valid::Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|error| panic!("rm_meadow: {}", error.emit_to_string(&source)));
    }

    /// The shader's blade ribbon emits the vertex count the draw asks for at every level.
    #[test]
    fn level_vertex_counts_match_the_ribbons() {
        for level in 0..crate::meadow::LEVELS as u32 {
            let segs = if level < 2 { 3 } else if level < 4 { 2 } else { 1 };
            assert_eq!(crate::meadow::level_vertices(level), (2 * segs - 1) * 3);
        }
    }
}
