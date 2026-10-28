//! Chunked distance LOD of the detail terrain for the raymarch backend (docs/TERRAIN_DETAIL.md).
//!
//! CDLOD (Strugar 2009): a quadtree over the 5 km map whose leaves are 16x16-cell quadrants at
//! lattice 0.305 m * 2^level. A node at level L is refined while it intersects the sphere of
//! radius `range(L-1)` around the camera (ranges double per level from `KESSEL_TERRAIN_LOD_RANGE`,
//! default 40 m); a quadrant outside it is drawn at the parent's level. Every vertex is the surface
//! function of `terrain_relief` evaluated on the GPU (`shaders/rm_terrain.wgsl`) at its own
//! position. In the outer quarter of each range the odd vertices slide onto the even ones and the
//! level's finest relief octave fades out, so at a level boundary both sides are the coarser
//! level's vertices exactly: no cracks, no skirts. The finest level, unmorphed, is the physics
//! surface (`Detail::height_at`).
//!
//! Resident on the GPU: the 4097² native heights (R32Float) and ground attributes (Rgba8Uint),
//! about 134 MB together; streamed: 32 m rut pages into a 16x16-page atlas (an LRU over distance;
//! the CPU keeps every page), the crater list and grid, and each frame's node lists per view.
use crate::terrain_relief::{self, Shared};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;

pub const QUAD_CELLS: u32 = 16;
pub const SLOTS: usize = 16;
pub const GBUFFER_SLOT: usize = 15;
pub const MORPH_START: f32 = 0.75;
pub const DEFAULT_RANGE0_M: f32 = 40.0;
pub const RUT_TABLE: u32 = 160;
pub const RUT_ATLAS_PAGES: u32 = 16;
/// Rut pages within this distance of the camera are kept resident; beyond it they may be evicted
/// (a 12.5 cm rut is under a pixel there at 800x450).
pub const RUT_RESIDENT_M: f32 = 600.0;
/// Node boxes: the native heights' min/max plus relief and rut/drift headroom.
const BOX_BELOW_M: f32 = 2.5;
const BOX_ABOVE_M: f32 = 0.8;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    pub origin: [f32; 2],
    pub size: f32,
    pub level: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    eye: [f32; 4],
    ranges: [[f32; 4]; 3],
    params: [f32; 4],
    rut: [f32; 4],
    crater: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LodStats {
    pub quads_selected: usize,
    pub quads_per_level: [u32; 12],
    pub gbuffer_quads: u32,
    pub shadow_quads: u32,
    pub gbuffer_triangles: u32,
    pub shadow_triangles: u32,
    pub rut_pages_cpu: usize,
    pub rut_pages_resident: usize,
    pub rut_uploads: usize,
    pub craters: usize,
    pub gpu_bytes: u64,
    pub select_ms: f32,
}

pub struct TerrainLod {
    pub detail: Shared,
    levels: usize,
    ranges: [f32; 12],
    /// Per area level a (squares of 16 * lattice * 2^a), min/max of the native heights.
    minmax: Vec<(usize, Vec<(f32, f32)>)>,
    _heights: wgpu::Texture,
    _surface: wgpu::Texture,
    rut_table: wgpu::Texture,
    rut_atlas: wgpu::Texture,
    craters_buf: wgpu::Buffer,
    cells_buf: wgpu::Buffer,
    items_buf: wgpu::Buffer,
    openings_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    instance_capacity: usize,
    patch_vbuf: wgpu::Buffer,
    patch_ibuf: wgpu::Buffer,
    patch_indices: u32,
    pub bgl: wgpu::BindGroupLayout,
    pub bind: wgpu::BindGroup,
    slot_ranges: [(u32, u32); SLOTS],
    resident: HashMap<(i32, i32), u32>,
    slot_owner: Vec<Option<((i32, i32), u64)>>,
    table: Vec<u32>,
    crater_revision: Option<u64>,
    crater_count: u32,
    opening_count: u32,
    frame: u64,
    pub stats: LodStats,
}

fn f32_to_f16(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mant = bits & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 { return sign; }
        let m = (mant | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exp >= 31 { return sign | 0x7c00; }
    let rounded = (exp as u32) << 10 | (mant + 0x1000) >> 13;
    sign | rounded.min(0x7bff) as u16
}

fn sphere_box(c: Vec3, r: f32, mn: Vec3, mx: Vec3) -> bool {
    (c.clamp(mn, mx) - c).length_squared() <= r * r
}

fn planes(vp: Mat4) -> [Vec4; 6] {
    let r = [vp.row(0), vp.row(1), vp.row(2), vp.row(3)];
    [r[3] + r[0], r[3] - r[0], r[3] + r[1], r[3] - r[1], r[2], r[3] - r[2]]
}

fn box_visible(planes: &[Vec4; 6], mn: Vec3, mx: Vec3) -> bool {
    planes.iter().all(|p| {
        let v = Vec3::new(if p.x >= 0.0 { mx.x } else { mn.x }, if p.y >= 0.0 { mx.y } else { mn.y }, if p.z >= 0.0 { mx.z } else { mn.z });
        p.truncate().dot(v) + p.w >= 0.0
    })
}

fn storage(device: &wgpu::Device, label: &str, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label), size: bytes.max(16), mapped_at_creation: false,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

impl TerrainLod {
    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        use wgpu::ShaderStages as S;
        let vis = S::VERTEX | S::FRAGMENT | S::COMPUTE;
        let tex = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding, visibility: vis, count: None,
            ty: wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        };
        let buf = |binding| wgpu::BindGroupLayoutEntry {
            binding, visibility: vis, count: None,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
        };
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-lod-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: vis, count: None,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None } },
                tex(1, wgpu::TextureSampleType::Float { filterable: false }),
                tex(2, wgpu::TextureSampleType::Uint),
                tex(3, wgpu::TextureSampleType::Uint),
                tex(4, wgpu::TextureSampleType::Float { filterable: false }),
                buf(5), buf(6), buf(7), buf(8),
            ],
        })
    }

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, detail: Shared) -> Self {
        let res = detail.res as u32;
        let texture = |label: &str, width: u32, format: wgpu::TextureFormat| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d { width, height: width, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let write = |tex: &wgpu::Texture, bytes: &[u8], width: u32, texel: u32| queue.write_texture(
            tex.as_image_copy(), bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * texel), rows_per_image: Some(width) },
            wgpu::Extent3d { width, height: width, depth_or_array_layers: 1 });
        let heights = texture("terrain-heights", res, wgpu::TextureFormat::R32Float);
        write(&heights, bytemuck::cast_slice(detail.heights()), res, 4);
        let surface = texture("terrain-surface", res, wgpu::TextureFormat::Rgba8Uint);
        write(&surface, bytemuck::cast_slice(detail.surface()), res, 4);
        let rut_table = texture("terrain-rut-table", RUT_TABLE, wgpu::TextureFormat::R32Uint);
        let atlas_res = RUT_ATLAS_PAGES * terrain_relief::RUT_PAGE as u32;
        let rut_atlas = texture("terrain-rut-atlas", atlas_res, wgpu::TextureFormat::R16Float);
        let grid = (detail.half * 2.0 / terrain_relief::CRATER_CELL_M).ceil() as u64 + 1;
        let craters_buf = storage(device, "terrain-craters", 16);
        let cells_buf = storage(device, "terrain-crater-cells", grid * grid * 8);
        let items_buf = storage(device, "terrain-crater-items", 16);
        let openings_buf = storage(device, "terrain-openings", 16);
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-lod-uniform"), size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let instance_capacity = 4096;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-lod-instances"), size: (instance_capacity * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let n = QUAD_CELLS + 1;
        let verts: Vec<[f32; 2]> = (0..n).flat_map(|j| (0..n).map(move |i| [i as f32, j as f32])).collect();
        let mut indices = Vec::with_capacity((QUAD_CELLS * QUAD_CELLS * 6) as usize);
        for j in 0..QUAD_CELLS {
            for i in 0..QUAD_CELLS {
                let a = j * n + i;
                let (b, c) = (a + 1, a + n);
                // Split along the (i+1, j)-(i, j+1) diagonal, as terrain.rs and Detail::height_at.
                indices.extend_from_slice(&[a, c, b, b, c, c + 1]);
            }
        }
        use wgpu::util::DeviceExt;
        let patch_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-patch"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX });
        let patch_ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-patch-indices"), contents: bytemuck::cast_slice(&indices), usage: wgpu::BufferUsages::INDEX });
        let bgl = Self::bind_group_layout(device);
        let range0 = std::env::var("KESSEL_TERRAIN_LOD_RANGE").ok().and_then(|v| v.parse::<f32>().ok())
            .filter(|r| *r >= 30.0).unwrap_or(DEFAULT_RANGE0_M);
        // Area level a squares: 16 lattice cells * 2^a; the root area spans the map.
        let mut levels = 1;
        while (QUAD_CELLS as f32 * detail.lattice * (1u32 << levels) as f32) < detail.half * 2.0 - 0.01 { levels += 1; }
        let mut ranges = [f32::INFINITY; 12];
        for (l, r) in ranges.iter_mut().enumerate().take(levels) { *r = range0 * (1u32 << l) as f32; }
        let minmax = Self::minmax_pyramid(&detail, levels);
        let gpu_bytes = res as u64 * res as u64 * 8 + (atlas_res as u64).pow(2) * 2 + (RUT_TABLE as u64).pow(2) * 4 + grid * grid * 8;
        let mut lod = Self {
            bind: Self::make_bind(device, &bgl, &uniform_buf, &heights, &surface, &rut_table, &rut_atlas, &craters_buf, &cells_buf, &items_buf, &openings_buf),
            detail, levels, ranges, minmax, _heights: heights, _surface: surface, rut_table, rut_atlas,
            craters_buf, cells_buf, items_buf, openings_buf, uniform_buf, instance_buf, instance_capacity,
            patch_vbuf, patch_ibuf, patch_indices: indices.len() as u32, bgl,
            slot_ranges: [(0, 0); SLOTS], resident: HashMap::new(),
            slot_owner: vec![None; (RUT_ATLAS_PAGES * RUT_ATLAS_PAGES) as usize],
            table: vec![0; (RUT_TABLE * RUT_TABLE) as usize], crater_revision: None, crater_count: 0,
            opening_count: 0, frame: 0, stats: LodStats::default(),
        };
        lod.stats.gpu_bytes = gpu_bytes;
        println!("[panzerrechnersystem] terrain LOD: {} levels, lattice {:.3} m, range0 {range0:.0} m, {:.0} MB resident on the GPU",
            lod.levels, lod.detail.lattice, gpu_bytes as f64 / 1e6);
        lod
    }

    #[allow(clippy::too_many_arguments)]
    fn make_bind(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, uniform: &wgpu::Buffer, heights: &wgpu::Texture,
        surface: &wgpu::Texture, table: &wgpu::Texture, atlas: &wgpu::Texture, craters: &wgpu::Buffer,
        cells: &wgpu::Buffer, items: &wgpu::Buffer, openings: &wgpu::Buffer) -> wgpu::BindGroup {
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let (hv, sv, tv, av) = (view(heights), view(surface), view(table), view(atlas));
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-lod"), layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&hv) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&sv) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&tv) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&av) },
                wgpu::BindGroupEntry { binding: 5, resource: craters.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: cells.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: items.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: openings.as_entire_binding() },
            ],
        })
    }

    fn rebind(&mut self, device: &wgpu::Device) {
        self.bind = Self::make_bind(device, &self.bgl, &self.uniform_buf, &self._heights, &self._surface, &self.rut_table,
            &self.rut_atlas, &self.craters_buf, &self.cells_buf, &self.items_buf, &self.openings_buf);
    }

    fn minmax_pyramid(detail: &terrain_relief::Detail, levels: usize) -> Vec<(usize, Vec<(f32, f32)>)> {
        // Area level 0 squares are 4 grid cells (16 lattice cells).
        let res = detail.res;
        let cells = (res - 1) / 4;
        let h = detail.heights();
        let mut level0 = vec![(f32::INFINITY, f32::NEG_INFINITY); cells * cells];
        for qz in 0..cells {
            for qx in 0..cells {
                let mut mm = (f32::INFINITY, f32::NEG_INFINITY);
                for j in qz * 4..=qz * 4 + 4 {
                    for i in qx * 4..=qx * 4 + 4 {
                        let v = h[j * res + i];
                        mm = (mm.0.min(v), mm.1.max(v));
                    }
                }
                level0[qz * cells + qx] = mm;
            }
        }
        let mut out = vec![(cells, level0)];
        for _ in 0..=levels {
            let (side, prev) = out.last().unwrap();
            let next_side = side.div_ceil(2);
            let mut next = vec![(f32::INFINITY, f32::NEG_INFINITY); next_side * next_side];
            for z in 0..*side {
                for x in 0..*side {
                    let (a, b) = prev[z * side + x];
                    let m = &mut next[(z / 2) * next_side + x / 2];
                    *m = (m.0.min(a), m.1.max(b));
                }
            }
            out.push((next_side, next));
        }
        out
    }

    fn area_box(&self, area: usize, x: i32, z: i32) -> (Vec3, Vec3) {
        let size = QUAD_CELLS as f32 * self.detail.lattice * (1u32 << area) as f32;
        let (side, grid) = &self.minmax[area.min(self.minmax.len() - 1)];
        let (lo, hi) = grid[(z.max(0) as usize).min(side - 1) * side + (x.max(0) as usize).min(side - 1)];
        let o = Vec2::new(x as f32, z as f32) * size - self.detail.half;
        (Vec3::new(o.x, lo - BOX_BELOW_M, o.y), Vec3::new(o.x + size, hi + BOX_ABOVE_M, o.y + size))
    }

    fn push_quad(&self, out: &mut Vec<(Instance, Vec3, Vec3)>, level: usize, x: i32, z: i32) {
        let size = QUAD_CELLS as f32 * self.detail.lattice * (1u32 << level) as f32;
        if x as f32 * size >= self.detail.half * 2.0 - 0.01 || z as f32 * size >= self.detail.half * 2.0 - 0.01 { return; }
        let (mn, mx) = self.area_box(level, x, z);
        out.push((Instance { origin: [mn.x, mn.z], size, level: level as f32 }, mn, mx));
    }

    /// CDLOD selection of node (level, x, z) (a node of level L covers area level L+1).
    fn select(&self, level: usize, x: i32, z: i32, eye: Vec3, out: &mut Vec<(Instance, Vec3, Vec3)>) -> bool {
        let (mn, mx) = self.area_box(level + 1, x, z);
        let node = QUAD_CELLS as f32 * self.detail.lattice * (1u32 << (level + 1)) as f32;
        if x as f32 * node >= self.detail.half * 2.0 - 0.01 || z as f32 * node >= self.detail.half * 2.0 - 0.01 { return true; }
        if level + 1 < self.levels && !sphere_box(eye, self.ranges[level], mn, mx) { return false; }
        if level == 0 || !sphere_box(eye, self.ranges[level - 1], mn, mx) {
            for (a, b) in [(0, 0), (1, 0), (0, 1), (1, 1)] { self.push_quad(out, level, 2 * x + a, 2 * z + b); }
            return true;
        }
        for (a, b) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            if !self.select(level - 1, 2 * x + a, 2 * z + b, eye, out) {
                self.push_quad(out, level, 2 * x + a, 2 * z + b);
            }
        }
        true
    }

    /// Select for `eye`, cull per view, stream events. `views[s]` is a world view-projection;
    /// `active[s]` false skips a view.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, eye: Vec3, views: &[Mat4; SLOTS], active: &[bool; SLOTS]) {
        let started = std::time::Instant::now();
        self.frame += 1;
        let mut quads = Vec::with_capacity(4096);
        let root = self.levels - 1;
        self.select(root, 0, 0, eye, &mut quads);
        let mut instances: Vec<Instance> = Vec::with_capacity(quads.len() * 3);
        let mut stats = LodStats { quads_selected: quads.len(), ..self.stats };
        stats.quads_per_level = [0; 12];
        for (q, _, _) in &quads { stats.quads_per_level[q.level as usize] += 1; }
        stats.gbuffer_quads = 0;
        stats.shadow_quads = 0;
        for slot in 0..SLOTS {
            let start = instances.len() as u32;
            if active[slot] {
                let p = planes(views[slot]);
                instances.extend(quads.iter().filter(|(_, mn, mx)| box_visible(&p, *mn, *mx)).map(|(q, _, _)| *q));
            }
            let count = instances.len() as u32 - start;
            if slot == GBUFFER_SLOT { stats.gbuffer_quads = count } else { stats.shadow_quads += count }
            self.slot_ranges[slot] = (start, count);
        }
        let tris = QUAD_CELLS * QUAD_CELLS * 2;
        stats.gbuffer_triangles = stats.gbuffer_quads * tris;
        stats.shadow_triangles = stats.shadow_quads * tris;
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("terrain-lod-instances"), size: (self.instance_capacity * std::mem::size_of::<Instance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
            });
        }
        if !instances.is_empty() { queue.write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&instances)); }
        stats.rut_uploads = self.stream_ruts(queue, eye);
        self.stream_craters(device, queue);
        self.stream_openings(device, queue);
        stats.rut_pages_resident = self.resident.len();
        stats.rut_pages_cpu = self.detail.rut_page_count();
        stats.craters = self.crater_count as usize;
        let mut ranges = [[0.0f32; 4]; 3];
        for l in 0..12 { ranges[l / 4][l % 4] = if self.ranges[l].is_finite() { self.ranges[l] } else { 1.0e9 }; }
        let grid = (self.detail.half * 2.0 / terrain_relief::CRATER_CELL_M).ceil() + 1.0;
        let uniform = Uniform {
            eye: [eye.x, eye.y, eye.z, self.detail.lattice],
            ranges,
            params: [MORPH_START, self.levels as f32, QUAD_CELLS as f32, self.opening_count as f32],
            rut: [RUT_TABLE as f32, RUT_ATLAS_PAGES as f32, terrain_relief::RUT_PAGE as f32, if self.resident.is_empty() { 0.0 } else { 1.0 }],
            crater: [grid, self.crater_count as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniform));
        stats.select_ms = started.elapsed().as_secs_f32() * 1000.0;
        self.stats = stats;
    }

    fn page_center(&self, key: (i32, i32)) -> Vec2 {
        (Vec2::new(key.0 as f32, key.1 as f32) + 0.5) * terrain_relief::RUT_PAGE_M - self.detail.half
    }

    fn upload_page(&mut self, queue: &wgpu::Queue, key: (i32, i32), data: &[f32], eye: Vec3) -> bool {
        if key.0 < 0 || key.1 < 0 || key.0 >= RUT_TABLE as i32 || key.1 >= RUT_TABLE as i32 { return false; }
        let slot = match self.resident.get(&key) {
            Some(&slot) => slot,
            None => {
                let Some(free) = self.slot_owner.iter().position(Option::is_none).or_else(|| {
                    // Evict the resident page farthest from the camera, if it is beyond the
                    // resident radius; otherwise this page waits (it is farther still).
                    let e = Vec2::new(eye.x, eye.z);
                    let (slot, far) = self.slot_owner.iter().enumerate()
                        .filter_map(|(s, o)| o.map(|(k, _)| (s, self.page_center(k).distance(e))))
                        .max_by(|a, b| a.1.total_cmp(&b.1))?;
                    (far > RUT_RESIDENT_M && self.page_center(key).distance(e) < far).then_some(slot)
                }) else { return false };
                if let Some((old, _)) = self.slot_owner[free] {
                    self.resident.remove(&old);
                    self.table[(old.1 as u32 * RUT_TABLE + old.0 as u32) as usize] = 0;
                }
                self.resident.insert(key, free as u32);
                self.table[(key.1 as u32 * RUT_TABLE + key.0 as u32) as usize] = free as u32 + 1;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.rut_table, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    bytemuck::cast_slice(&self.table),
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(RUT_TABLE * 4), rows_per_image: Some(RUT_TABLE) },
                    wgpu::Extent3d { width: RUT_TABLE, height: RUT_TABLE, depth_or_array_layers: 1 });
                free as u32
            }
        };
        self.slot_owner[slot as usize] = Some((key, self.frame));
        let page = terrain_relief::RUT_PAGE as u32;
        let half: Vec<u16> = data.iter().map(|&v| f32_to_f16(v)).collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.rut_atlas, mip_level: 0, aspect: wgpu::TextureAspect::All,
                origin: wgpu::Origin3d { x: (slot % RUT_ATLAS_PAGES) * page, y: (slot / RUT_ATLAS_PAGES) * page, z: 0 } },
            bytemuck::cast_slice(&half),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(page * 2), rows_per_image: Some(page) },
            wgpu::Extent3d { width: page, height: page, depth_or_array_layers: 1 });
        true
    }

    fn stream_ruts(&mut self, queue: &wgpu::Queue, eye: Vec3) -> usize {
        let mut uploads = 0;
        for (key, data) in self.detail.take_dirty_rut_pages() {
            if self.upload_page(queue, key, &data, eye) { uploads += 1; }
        }
        // Every half second, bring back evicted pages the camera has returned to.
        if self.frame % 30 == 0 {
            let e = Vec2::new(eye.x, eye.z);
            let keys = self.detail.rut_page_keys();
            for key in keys {
                if self.resident.contains_key(&key) || self.page_center(key).distance(e) > RUT_RESIDENT_M { continue; }
                if let Some(data) = self.detail.rut_page(key) {
                    if self.upload_page(queue, key, &data, eye) { uploads += 1; }
                }
            }
        }
        uploads
    }

    fn stream_craters(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let revision = self.detail.crater_revision();
        if self.crater_revision == Some(revision) { return; }
        self.crater_revision = Some(revision);
        let craters = self.detail.craters();
        let side = (self.detail.half * 2.0 / terrain_relief::CRATER_CELL_M).ceil() as i32 + 1;
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); (side * side) as usize];
        for (k, c) in craters.iter().enumerate() {
            let reach = c.radius * terrain_relief::CRATER_REACH_SHARE;
            let lo = ((c.center - reach + self.detail.half) / terrain_relief::CRATER_CELL_M).floor();
            let hi = ((c.center + reach + self.detail.half) / terrain_relief::CRATER_CELL_M).floor();
            for z in (lo.y as i32).max(0)..=(hi.y as i32).min(side - 1) {
                for x in (lo.x as i32).max(0)..=(hi.x as i32).min(side - 1) {
                    lists[(z * side + x) as usize].push(k as u32);
                }
            }
        }
        let mut cells = Vec::with_capacity(lists.len());
        let mut items = Vec::new();
        for l in &lists {
            cells.push([items.len() as u32, l.len() as u32]);
            items.extend_from_slice(l);
        }
        let packed: Vec<[f32; 4]> = craters.iter().map(|c| [c.center.x, c.center.y, c.radius, c.depth]).collect();
        let mut rebind = false;
        if (packed.len() * 16) as u64 > self.craters_buf.size() {
            self.craters_buf = storage(device, "terrain-craters", (packed.len().next_power_of_two() * 16) as u64);
            rebind = true;
        }
        if (items.len() * 4) as u64 > self.items_buf.size() {
            self.items_buf = storage(device, "terrain-crater-items", (items.len().next_power_of_two() * 4) as u64);
            rebind = true;
        }
        if rebind { self.rebind(device); }
        if !packed.is_empty() { queue.write_buffer(&self.craters_buf, 0, bytemuck::cast_slice(&packed)); }
        if !items.is_empty() { queue.write_buffer(&self.items_buf, 0, bytemuck::cast_slice(&items)); }
        queue.write_buffer(&self.cells_buf, 0, bytemuck::cast_slice(&cells));
        self.crater_count = packed.len() as u32;
    }

    fn stream_openings(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(openings) = self.detail.take_openings() else { return };
        let packed: Vec<[f32; 4]> = openings.iter().map(|o| [o[0], o[1], o[2], o[3]]).collect();
        if (packed.len() * 16) as u64 > self.openings_buf.size() {
            self.openings_buf = storage(device, "terrain-openings", (packed.len().next_power_of_two() * 16) as u64);
            self.rebind(device);
        }
        if !packed.is_empty() { queue.write_buffer(&self.openings_buf, 0, bytemuck::cast_slice(&packed)); }
        self.opening_count = packed.len() as u32;
    }

    /// Draw this frame's nodes for `slot` (bind groups 0 and 1 and the pipeline already set).
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, slot: usize) -> u32 {
        let (start, count) = self.slot_ranges[slot];
        if count == 0 { return 0; }
        pass.set_bind_group(2, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.patch_vbuf.slice(..));
        pass.set_vertex_buffer(1, self.instance_buf.slice(..));
        pass.set_index_buffer(self.patch_ibuf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.patch_indices, 0, start..start + count);
        count
    }

    pub fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
        const GRID: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        const NODE: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![1 => Float32x4];
        [
            wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &GRID },
            wgpu::VertexBufferLayout { array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &NODE },
        ]
    }

    /// The WGSL source shared by every terrain program: generated constants + rm_terrain.wgsl.
    pub fn shader_prelude(&self) -> String {
        format!("{}\nconst TR_QUAD_CELLS: f32 = {:?};\n{}", terrain_relief::wgsl_constants(&self.detail),
            QUAD_CELLS as f32, include_str!("../shaders/rm_terrain.wgsl"))
    }

    /// Evaluate the GPU surface (finest level, unmorphed) at `points` and return it: the check
    /// that the drawn terrain is the physics surface (`Detail::height_at` at lattice nodes).
    pub fn probe(&self, device: &wgpu::Device, queue: &wgpu::Queue, points: &[Vec2]) -> Vec<f32> {
        use wgpu::util::DeviceExt;
        let source = format!("{}\n{}", self.shader_prelude(), PROBE_WGSL);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("terrain-probe"), source: wgpu::ShaderSource::Wgsl(source.into()) });
        let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("terrain-probe-empty"), entries: &[] });
        let io_entry = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::COMPUTE, count: None,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
        };
        let io = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("terrain-probe-io"), entries: &[io_entry(0, true), io_entry(1, false)] });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("terrain-probe"), bind_group_layouts: &[&empty, &io, &self.bgl], push_constant_ranges: &[] });
        let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("terrain-probe"), layout: Some(&layout), module: &module, entry_point: Some("cs_probe"), compilation_options: Default::default(), cache: None });
        let pts: Vec<[f32; 2]> = points.iter().map(|p| p.to_array()).collect();
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("probe-in"), contents: bytemuck::cast_slice(&pts), usage: wgpu::BufferUsages::STORAGE });
        let bytes = (points.len() * 4) as u64;
        let output = device.create_buffer(&wgpu::BufferDescriptor { label: Some("probe-out"), size: bytes, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: Some("probe-read"), size: bytes, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let empty_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty, entries: &[] });
        let io_bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &io, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: input.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() }] });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipe);
            pass.set_bind_group(0, &empty_bind, &[]);
            pass.set_bind_group(1, &io_bind, &[]);
            pass.set_bind_group(2, &self.bind, &[]);
            pass.dispatch_workgroups((points.len() as u32).div_ceil(64), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        queue.submit([encoder.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = readback.slice(..).get_mapped_range();
        let out = bytemuck::cast_slice::<u8, f32>(&data).to_vec();
        drop(data);
        readback.unmap();
        out
    }
}

const PROBE_WGSL: &str = r#"
@group(1) @binding(0) var<storage, read> probe_points: array<vec2<f32>>;
@group(1) @binding(1) var<storage, read_write> probe_out: array<f32>;
@compute @workgroup_size(64)
fn cs_probe(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&probe_points)) { return; }
    probe_out[id.x] = tr_height(probe_points[id.x], 0, 0.0);
}
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn half_floats_round_trip_rut_depths() {
        for v in [0.0f32, 0.3, -0.075, 0.45, 0.001] {
            let h = super::f32_to_f16(v);
            let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
            let exp = ((h >> 10) & 0x1f) as i32;
            let mant = (h & 0x3ff) as f32;
            let back = if exp == 0 { sign * mant * 2f32.powi(-24) } else { sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15) };
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-6, "{v} -> {back}");
        }
    }
}
