//! GPU side of the strewn field (`scatter_field.rs`, `docs/SCATTER.md`): every placed object of
//! a map resident in one 32-byte-per-instance buffer, prototype geometry in one mega vertex/index
//! buffer with three LODs (the source mesh; a grid-clustered reduction; one native pixel), albedo
//! in texture-array pages, and per frame a GPU cull (cell frustum, instance sphere frustum,
//! distance, projected size, previous-frame Hi-Z occlusion for the camera) that compacts the
//! survivors into per-(prototype, LOD) buckets and writes indirect draw arguments, so each view's
//! field is drawn with one multi-draw per texture page. The camera view takes every tier; the
//! sun cascades and the sky-cover map take the large tier at the reduced LOD only.
//!
//! Named limits: Hi-Z is the previous frame's depth under the previous camera (one frame of
//! latency on disocclusion); the reduced LOD is a vertex-clustering of the source mesh, the far
//! LOD a one-pixel stand-in carrying the mean albedo; metallic-roughness and normal maps of a
//! prototype are not carried (the kit has none); point-light faces take no scatter shadows.
use std::cell::{Cell, RefCell};

use glam::{Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use super::{frustum_planes, Renderer, Vertex, DEPTH};
use crate::gltf_load::CpuMesh;
use crate::scatter_field::{Built, Packed};

const VIEWS: usize = 4;
const LODS: usize = 3;
const PAGE_SIZES: [u32; 3] = [128, 256, 512];
const WORKGROUP: u32 = 256;
/// Camera LOD thresholds in projected pixels of the object's bounding sphere diameter.
const NEAR_PX: f32 = 28.0;
const MID_PX: f32 = 5.0;
const FAR_PX: f32 = 0.8;
const MAX_DISTANCE_M: f32 = 1500.0;
const SHADOW_DISTANCE_M: f32 = 700.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuProto {
    a: [u32; 4],
    b: [u32; 4],
    c: [u32; 4],
    d: [f32; 4],
    e: [f32; 4],
    f: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuView {
    vp: [[f32; 4]; 4],
    planes: [[f32; 4]; 6],
    origin: [f32; 4],
    eye: [f32; 4],
    params: [f32; 4],
    params2: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CullUniform {
    views: [GpuView; VIEWS],
    counts: [u32; 4],
    hiz: [f32; 4],
    prev_vp: [[f32; 4]; 4],
    prev_origin: [f32; 4],
    proj_scale: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniform {
    view: GpuView,
    base: u32,
    bucket_base: u32,
    multi: u32,
    pad: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScatterStats {
    pub instances: usize,
    pub cells: usize,
    pub prototypes: usize,
    pub vertices: usize,
    pub triangles: [usize; 2],
    pub page_layers: [u32; 3],
    pub bytes_instances: u64,
    pub bytes_geometry: u64,
    pub bytes_textures: u64,
    pub bytes_cull: u64,
    /// Visible instances per view (shadow far, shadow near, sky cover, camera) and per camera LOD,
    /// from the last stats readback.
    pub visible: [u32; VIEWS],
    pub camera_lod: [u32; LODS],
    pub multi_draw: bool,
}

struct Page {
    size: u32,
    layers: u32,
    bind: wgpu::BindGroup,
    /// Bucket range [first, last) of this page's prototypes within one view.
    bucket_range: (u32, u32),
}

pub struct ScatterGpu {
    n_instances: u32,
    n_cells: u32,
    n_buckets: u32,
    placements: wgpu::Buffer,
    cells: wgpu::Buffer,
    protos: wgpu::Buffer,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    cull_uniform: wgpu::Buffer,
    draw_uniforms: [wgpu::Buffer; VIEWS],
    cell_vis: wgpu::Buffer,
    slot: wgpu::Buffer,
    bucket_count: wgpu::Buffer,
    bucket_offset: wgpu::Buffer,
    bucket_cursor: wgpu::Buffer,
    visible: wgpu::Buffer,
    indirect: wgpu::Buffer,
    stats_readback: wgpu::Buffer,
    cull_bind: wgpu::BindGroup,
    draw_binds: [wgpu::BindGroup; VIEWS],
    pages: Vec<Page>,
    cells_pipe: wgpu::ComputePipeline,
    count_pipe: wgpu::ComputePipeline,
    scan_pipe: wgpu::ComputePipeline,
    write_pipe: wgpu::ComputePipeline,
    gbuffer_pipe: wgpu::RenderPipeline,
    depth_pipe: wgpu::RenderPipeline,
    hiz_tex: wgpu::Texture,
    hiz_mips: u32,
    hiz_copy_pipe: wgpu::ComputePipeline,
    hiz_reduce_pipe: wgpu::ComputePipeline,
    hiz_binds: Vec<wgpu::BindGroup>,
    hiz_full_view: wgpu::TextureView,
    /// (camera-relative view_proj, eye, projection scale) of the frame whose depth the Hi-Z holds.
    hiz_frame: Cell<Option<(Mat4, Vec3, [f32; 2])>>,
    hiz_valid: Cell<bool>,
    multi_draw: bool,
    skip_draw: bool,
    skip_hiz: bool,
    stats_every: u32,
    frame: Cell<u32>,
    stats: RefCell<ScatterStats>,
    cull_dims: [u32; 2],
}

/// Grid-clustered reduction of a mesh: vertices merged by a cell of the mesh's extent over
/// `cells_across`, degenerate triangles dropped. Returns `None` when the source is already small.
fn reduce_mesh(mesh: &CpuMesh, cells_across: f32) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if mesh.indices.len() < 150 {
        return None;
    }
    let extent = (mesh.max - mesh.min).max_element().max(1.0e-3);
    let cell = extent / cells_across;
    let mut map: std::collections::HashMap<(i32, i32, i32), u32> = std::collections::HashMap::new();
    let mut remap = vec![0u32; mesh.vertices.len()];
    let mut vertices: Vec<Vertex> = Vec::new();
    for (i, v) in mesh.vertices.iter().enumerate() {
        let key = (
            ((v.pos[0] - mesh.min.x) / cell).floor() as i32,
            ((v.pos[1] - mesh.min.y) / cell).floor() as i32,
            ((v.pos[2] - mesh.min.z) / cell).floor() as i32,
        );
        let id = *map.entry(key).or_insert_with(|| {
            vertices.push(*v);
            vertices.len() as u32 - 1
        });
        remap[i] = id;
    }
    let mut indices = Vec::with_capacity(mesh.indices.len());
    for tri in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (remap[tri[0] as usize], remap[tri[1] as usize], remap[tri[2] as usize]);
        if a != b && b != c && a != c {
            indices.extend_from_slice(&[a, b, c]);
        }
    }
    if indices.len() < 12 || indices.len() * 10 > mesh.indices.len() * 9 {
        return None;
    }
    Some((vertices, indices))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Nearest-resample an RGBA8 image to `size` x `size`, then box-filter a full mip chain.
fn page_layer(rgba: &[u8], w: u32, h: u32, size: u32) -> Vec<Vec<u8>> {
    let mut level = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        let sy = (y * h / size).min(h - 1);
        for x in 0..size {
            let sx = (x * w / size).min(w - 1);
            let s = ((sy * w + sx) * 4) as usize;
            let d = ((y * size + x) * 4) as usize;
            level[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    let mut mips = vec![level];
    let mut s = size;
    while s > 1 {
        let prev = mips.last().unwrap();
        let n = s / 2;
        let mut next = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                        sum += prev[(((y * 2 + dy) * s + (x * 2 + dx)) * 4 + c) as usize] as u32;
                    }
                    next[((y * n + x) * 4 + c) as usize] = (sum / 4) as u8;
                }
            }
        }
        mips.push(next);
        s = n;
    }
    mips
}

impl ScatterGpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, built: Built, iw: u32, ih: u32, depth_view: &wgpu::TextureView) -> Self {
        // wgpu 27 offers multi_draw_indexed_indirect on every backend (looped where the API has
        // no native form); a non-zero first_instance in indirect arguments needs the feature.
        let multi_draw = device.features().contains(wgpu::Features::INDIRECT_FIRST_INSTANCE);
        let skip_draw = std::env::var("KESSEL_SCATTER_SKIP").as_deref() == Ok("1");
        let skip_hiz = std::env::var("KESSEL_SCATTER_NO_HIZ").as_deref() == Ok("1");
        let stats_every = std::env::var("KESSEL_SCATTER_STATS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        // ── prototypes: assign texture pages, order by page, build the mega buffers ──
        let page_of = |mesh: &CpuMesh| -> usize {
            let dim = mesh.texture.as_ref().map(|t| t.width.max(t.height)).unwrap_or(4);
            if dim <= 128 { 0 } else if dim <= 256 { 1 } else { 2 }
        };
        let mut order: Vec<usize> = (0..built.prototypes.len()).collect();
        order.sort_by_key(|&i| (page_of(&built.prototypes[i].mesh), i));
        let mut remap = vec![0u32; built.prototypes.len()];
        for (gpu, &src) in order.iter().enumerate() {
            remap[src] = gpu as u32;
        }
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut protos: Vec<GpuProto> = Vec::new();
        let mut page_layers = [0u32; 3];
        let mut layer_images: [Vec<(Vec<Vec<u8>>, u32)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        let mut tri_counts = [0usize; 2];
        // the pixel quad: corners in xy, index range shared by every prototype's far LOD
        let pixel_first = indices.len() as u32;
        let pixel_base = vertices.len() as u32;
        for (x, y) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex { pos: [x, y, 0.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0], tangent: [1.0, 0.0, 0.0, 1.0] });
        }
        indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        for &src in &order {
            let p = &built.prototypes[src];
            let page = page_of(&p.mesh);
            let size = PAGE_SIZES[page];
            let (mips, mean) = match &p.mesh.texture {
                Some(t) => {
                    let mips = page_layer(&t.rgba, t.width, t.height, size);
                    let mut sum = [0.0f32; 3];
                    let mut n = 0.0f32;
                    for px in t.rgba.chunks_exact(4) {
                        if px[3] >= 128 {
                            for c in 0..3 {
                                sum[c] += srgb_to_linear(px[c] as f32 / 255.0);
                            }
                            n += 1.0;
                        }
                    }
                    (mips, if n > 0.0 { [sum[0] / n, sum[1] / n, sum[2] / n] } else { [0.5; 3] })
                }
                None => (page_layer(&[255, 255, 255, 255], 1, 1, size), [0.6, 0.6, 0.6]),
            };
            let layer = page_layers[page];
            page_layers[page] += 1;
            layer_images[page].push((mips, layer));
            // lod 0: the source mesh
            let base0 = vertices.len() as u32;
            let first0 = indices.len() as u32;
            vertices.extend_from_slice(&p.mesh.vertices);
            indices.extend_from_slice(&p.mesh.indices);
            tri_counts[0] += p.mesh.indices.len() / 3;
            // lod 1: the reduction (or the source when already small)
            let (base1, first1, count1) = match reduce_mesh(&p.mesh, 6.0) {
                Some((v, i)) => {
                    let b = vertices.len() as u32;
                    let f = indices.len() as u32;
                    vertices.extend_from_slice(&v);
                    indices.extend_from_slice(&i);
                    tri_counts[1] += i.len() / 3;
                    (b, f, i.len() as u32)
                }
                None => {
                    tri_counts[1] += p.mesh.indices.len() / 3;
                    (base0, first0, p.mesh.indices.len() as u32)
                }
            };
            let flags = u32::from(p.flat) | (u32::from(p.casualty) << 1);
            protos.push(GpuProto {
                a: [first0, first1, pixel_first, ((page as u32) << 16) | layer],
                b: [p.mesh.indices.len() as u32, count1, 6, base0],
                c: [base1, pixel_base, flags, 0],
                d: [p.half.length().max(0.01), p.height, p.mesh.metallic_factor, p.mesh.roughness_factor],
                e: [mean[0], mean[1], mean[2], p.tier as u32 as f32],
                f: [p.center.x, p.center.y, p.center.z, 0.0],
            });
        }
        let n_protos = protos.len() as u32;
        let n_buckets = n_protos * LODS as u32;
        // instances with remapped prototype ids
        let mut placements: Vec<Packed> = built.placements;
        for p in &mut placements {
            let proto = remap[p.proto()];
            p.scale_proto = (p.scale_proto & 0xffff_0000) | proto;
        }
        let n_instances = placements.len() as u32;
        let n_cells = built.cells.len() as u32;
        let make = |label: &str, contents: &[u8], usage: wgpu::BufferUsages| device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label), contents, usage,
        });
        let empty = |label: &str, size: u64, usage: wgpu::BufferUsages| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size: size.max(16), usage, mapped_at_creation: false,
        });
        let st = wgpu::BufferUsages::STORAGE;
        let placements_buf = make("scatter-placements", bytemuck::cast_slice(&placements), st);
        let cells_buf = make("scatter-cells", bytemuck::cast_slice(&built.cells), st);
        let protos_buf = make("scatter-protos", bytemuck::cast_slice(&protos), st);
        let vbuf = make("scatter-vbuf", bytemuck::cast_slice(&vertices), wgpu::BufferUsages::VERTEX);
        let ibuf = make("scatter-ibuf", bytemuck::cast_slice(&indices), wgpu::BufferUsages::INDEX);
        let cull_uniform = empty("scatter-cull-uniform", std::mem::size_of::<CullUniform>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        let draw_uniforms: [wgpu::Buffer; VIEWS] = std::array::from_fn(|_| empty("scatter-draw-uniform", std::mem::size_of::<DrawUniform>() as u64, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST));
        let per_view_instances = u64::from(n_instances) * VIEWS as u64 * 4;
        let cell_vis = empty("scatter-cell-vis", u64::from(n_cells) * VIEWS as u64 * 4, st);
        let slot = empty("scatter-slot", per_view_instances, st);
        let bucket_bytes = u64::from(n_buckets) * VIEWS as u64 * 4;
        let bucket_count = empty("scatter-bucket-count", bucket_bytes, st | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC);
        let bucket_offset = empty("scatter-bucket-offset", bucket_bytes, st);
        let bucket_cursor = empty("scatter-bucket-cursor", bucket_bytes, st | wgpu::BufferUsages::COPY_DST);
        let visible = empty("scatter-visible", per_view_instances, st);
        let indirect = empty("scatter-indirect", u64::from(n_buckets) * VIEWS as u64 * 20, st | wgpu::BufferUsages::INDIRECT);
        let stats_readback = empty("scatter-stats-readback", bucket_bytes, wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        // ── texture pages ──
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scatter-nearest"),
            mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, ..Default::default()
        });
        let page_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scatter-page-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let mut pages = Vec::new();
        let mut bytes_textures = 0u64;
        let mut bucket_cursor_first = 0u32;
        for (page, size) in PAGE_SIZES.iter().enumerate() {
            let layers = page_layers[page].max(1);
            let mips = (32 - size.leading_zeros()) as u32;
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scatter-page"),
                size: wgpu::Extent3d { width: *size, height: *size, depth_or_array_layers: layers },
                mip_level_count: mips, sample_count: 1, dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
            });
            for (mip_images, layer) in &layer_images[page] {
                for (m, data) in mip_images.iter().enumerate() {
                    let s = size >> m;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: m as u32, origin: wgpu::Origin3d { x: 0, y: 0, z: *layer }, aspect: wgpu::TextureAspect::All },
                        data,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * 4), rows_per_image: Some(s) },
                        wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 1 },
                    );
                    bytes_textures += u64::from(s * s * 4);
                }
            }
            let view = tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scatter-page"), layout: &page_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            });
            let count = page_layers[page] * LODS as u32;
            pages.push(Page { size: *size, layers: page_layers[page], bind, bucket_range: (bucket_cursor_first, bucket_cursor_first + count) });
            bucket_cursor_first += count;
        }
        // ── layouts, shaders, pipelines ──
        let common = include_str!("../shaders/scatter_common.wgsl");
        let shader = |label: &str, body: &str| device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label), source: wgpu::ShaderSource::Wgsl(format!("{common}\n{body}").into()),
        });
        let cull_shader = shader("scatter-cull", include_str!("../shaders/scatter_cull.wgsl"));
        let draw_shader = shader("scatter-draw", include_str!("../shaders/scatter_draw.wgsl"));
        let hiz_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scatter-hiz"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scatter_hiz.wgsl").into()),
        });
        let storage = |binding: u32, read_only: bool, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None }, count: None,
        };
        let uniform = |binding: u32, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding, visibility: vis, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
        };
        let c = wgpu::ShaderStages::COMPUTE;
        let cull_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scatter-cull-bgl"),
            entries: &[
                uniform(0, c), storage(1, true, c), storage(2, true, c), storage(3, true, c),
                storage(4, false, c), storage(5, false, c), storage(6, false, c), storage(7, false, c),
                storage(8, false, c), storage(9, false, c), storage(10, false, c),
                wgpu::BindGroupLayoutEntry { binding: 11, visibility: c, ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let v = wgpu::ShaderStages::VERTEX;
        let draw_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scatter-draw-bgl"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT), storage(1, true, v), storage(2, true, wgpu::ShaderStages::VERTEX_FRAGMENT), storage(3, true, v), storage(4, true, v)],
        });
        // Hi-Z pyramid over the internal resolution
        let hiz_mips = (32 - iw.max(ih).leading_zeros()) as u32;
        let hiz_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scatter-hiz"),
            size: wgpu::Extent3d { width: iw, height: ih, depth_or_array_layers: 1 },
            mip_level_count: hiz_mips, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING, view_formats: &[],
        });
        let hiz_full_view = hiz_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let hiz_copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scatter-hiz-copy-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: c, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: c, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let hiz_reduce_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scatter-hiz-reduce-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: c, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: c, ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Float, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
            ],
        });
        let compute = |label: &str, module: &wgpu::ShaderModule, bgl: &wgpu::BindGroupLayout, entry: &str| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[bgl], push_constant_ranges: &[] });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label), layout: Some(&layout), module, entry_point: Some(entry), compilation_options: Default::default(), cache: None,
            })
        };
        let cells_pipe = compute("scatter-cells", &cull_shader, &cull_bgl, "cs_cells");
        let count_pipe = compute("scatter-count", &cull_shader, &cull_bgl, "cs_count");
        let scan_pipe = compute("scatter-scan", &cull_shader, &cull_bgl, "cs_scan");
        let write_pipe = compute("scatter-write", &cull_shader, &cull_bgl, "cs_write");
        let hiz_copy_pipe = compute("scatter-hiz-copy", &hiz_shader, &hiz_copy_bgl, "cs_copy");
        let hiz_reduce_pipe = compute("scatter-hiz-reduce", &hiz_shader, &hiz_reduce_bgl, "cs_reduce");
        let draw_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scatter-draw-pl"), bind_group_layouts: &[&draw_bgl, &page_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange { stages: wgpu::ShaderStages::VERTEX, range: 0..4 }],
        });
        const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
        let vertex_layout = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS };
        let gtargets: Vec<Option<wgpu::ColorTargetState>> = (0..3).map(|_| Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL,
        })).collect();
        let gbuffer_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scatter-gbuffer"), layout: Some(&draw_layout),
            vertex: wgpu::VertexState { module: &draw_shader, entry_point: Some("vs"), buffers: std::slice::from_ref(&vertex_layout), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &draw_shader, entry_point: Some("fs_gbuffer"), targets: &gtargets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let depth_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scatter-depth"), layout: Some(&draw_layout),
            vertex: wgpu::VertexState { module: &draw_shader, entry_point: Some("vs"), buffers: std::slice::from_ref(&vertex_layout), compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 } }),
            multisample: Default::default(), fragment: None, multiview: None, cache: None,
        });
        // ── bind groups ──
        let cull_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scatter-cull"), layout: &cull_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: cull_uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: placements_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: cells_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: protos_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: cell_vis.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: slot.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: bucket_count.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 7, resource: bucket_offset.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: bucket_cursor.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 9, resource: visible.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 10, resource: indirect.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 11, resource: wgpu::BindingResource::TextureView(&hiz_full_view) },
            ],
        });
        let draw_binds: [wgpu::BindGroup; VIEWS] = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scatter-draw"), layout: &draw_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: draw_uniforms[i].as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: placements_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: protos_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: visible.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: bucket_offset.as_entire_binding() },
            ],
        }));
        let mut hiz_binds = Vec::new();
        let mip_view = |m: u32| hiz_tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() });
        for m in 0..hiz_mips {
            let dst = mip_view(m);
            if m == 0 {
                hiz_binds.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scatter-hiz-copy"), layout: &hiz_copy_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(depth_view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&dst) },
                    ],
                }));
            } else {
                let src = mip_view(m - 1);
                hiz_binds.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("scatter-hiz-reduce"), layout: &hiz_reduce_bgl,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&dst) },
                    ],
                }));
            }
        }
        let bytes_instances = u64::from(n_instances) * 32 + u64::from(n_cells) * 32;
        let bytes_geometry = (vertices.len() * std::mem::size_of::<Vertex>() + indices.len() * 4) as u64;
        let bytes_cull = per_view_instances * 2 + u64::from(n_cells) * VIEWS as u64 * 4 + bucket_bytes * 3 + u64::from(n_buckets) * VIEWS as u64 * 20
            + u64::from(iw * ih) * 4 * 4 / 3;
        let stats = ScatterStats {
            instances: n_instances as usize, cells: n_cells as usize, prototypes: n_protos as usize, vertices: vertices.len(),
            triangles: tri_counts, page_layers, bytes_instances, bytes_geometry, bytes_textures, bytes_cull, visible: [0; VIEWS],
            camera_lod: [0; LODS], multi_draw,
        };
        println!(
            "[scatter] gpu: {} instances in {} cells, {} prototypes ({} tris near / {} tris mid, {} vertices) over pages {}x128 {}x256 {}x512; {:.1} MB instances, {:.1} MB geometry, {:.1} MB textures, {:.1} MB cull scratch; multi-draw {}; Hi-Z {}x{} {} mips",
            n_instances, n_cells, n_protos, tri_counts[0], tri_counts[1], vertices.len(), page_layers[0], page_layers[1], page_layers[2],
            bytes_instances as f64 / 1.0e6, bytes_geometry as f64 / 1.0e6, bytes_textures as f64 / 1.0e6, bytes_cull as f64 / 1.0e6,
            if multi_draw { "on" } else { "off (per-bucket indirect draws)" }, iw, ih, hiz_mips
        );
        Self {
            n_instances, n_cells, n_buckets, placements: placements_buf, cells: cells_buf, protos: protos_buf, vbuf, ibuf, cull_uniform, draw_uniforms,
            cell_vis, slot, bucket_count, bucket_offset, bucket_cursor, visible, indirect, stats_readback, cull_bind, draw_binds, pages,
            cells_pipe, count_pipe, scan_pipe, write_pipe, gbuffer_pipe, depth_pipe, hiz_tex, hiz_mips, hiz_copy_pipe, hiz_reduce_pipe, hiz_binds,
            hiz_full_view, hiz_frame: Cell::new(None), hiz_valid: Cell::new(false), multi_draw, skip_draw, skip_hiz, stats_every, frame: Cell::new(0),
            stats: RefCell::new(stats), cull_dims: [iw, ih],
        }
    }

    pub fn stats(&self) -> ScatterStats {
        *self.stats.borrow()
    }

    fn view_uniform(vp: Mat4, planes_vp: Mat4, origin: Vec3, eye: Vec3, pixel_angle: f32, camera: bool, iw: u32, ih: u32) -> GpuView {
        let planes = frustum_planes(planes_vp);
        GpuView {
            vp: vp.to_cols_array_2d(),
            planes: planes.map(|p: Vec4| p.to_array()),
            origin: [origin.x, origin.y, origin.z, 0.0],
            eye: [eye.x, eye.y, eye.z, pixel_angle],
            params: [if camera { 1.0 } else { 0.0 }, NEAR_PX, MID_PX, FAR_PX],
            params2: [if camera { MAX_DISTANCE_M } else { SHADOW_DISTANCE_M }, iw as f32, ih as f32, 0.0],
        }
    }

    /// Per-frame CPU side: the four views' matrices and the Hi-Z frame, from `upload_uniforms`.
    pub(super) fn prepare(&mut self, r: &Renderer, shadow_views: &[Mat4], device: &wgpu::Device, queue: &wgpu::Queue) {
        let cam = &r.camera;
        let eye = cam.pos;
        let vp_abs = cam.view_proj();
        let vp_rel = cam.relative_view_proj();
        let pixel_angle = 2.0 * (cam.fov_y * 0.5).tan() / r.ih as f32;
        let mut views = [GpuView { vp: [[0.0; 4]; 4], planes: [[0.0; 4]; 6], origin: [0.0; 4], eye: [0.0; 4], params: [0.0; 4], params2: [0.0; 4] }; VIEWS];
        for (i, view) in views.iter_mut().enumerate().take(3) {
            let m = shadow_views.get(i).copied().unwrap_or(Mat4::IDENTITY);
            *view = Self::view_uniform(m, m, Vec3::ZERO, eye, pixel_angle, false, r.iw, r.ih);
        }
        views[3] = Self::view_uniform(vp_rel, vp_abs, eye, eye, pixel_angle, true, r.iw, r.ih);
        let (prev_vp, prev_origin, proj_scale, valid) = match self.hiz_frame.get() {
            Some((vp, origin, scale)) if !self.skip_hiz => (vp, origin, scale, 1.0),
            _ => (Mat4::IDENTITY, Vec3::ZERO, [1.0, 1.0], 0.0),
        };
        let cull = CullUniform {
            views,
            counts: [self.n_instances, self.n_cells, self.n_buckets / LODS as u32, self.n_buckets],
            hiz: [self.cull_dims[0] as f32, self.cull_dims[1] as f32, self.hiz_mips as f32, valid],
            prev_vp: prev_vp.to_cols_array_2d(),
            prev_origin: [prev_origin.x, prev_origin.y, prev_origin.z, 0.0],
            proj_scale: [proj_scale[0], proj_scale[1], if self.multi_draw { 1.0 } else { 0.0 }, 0.0],
        };
        queue.write_buffer(&self.cull_uniform, 0, bytemuck::bytes_of(&cull));
        for (i, buf) in self.draw_uniforms.iter().enumerate() {
            let d = DrawUniform { view: views[i], base: i as u32 * self.n_instances, bucket_base: i as u32 * self.n_buckets, multi: u32::from(self.multi_draw), pad: 0 };
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&d));
        }
        // optional stats readback of the previous frame's bucket counts
        let frame = self.frame.get();
        if self.stats_every > 0 && frame > 1 && frame % self.stats_every == 0 {
            let slice = self.stats_readback.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |res| { let _ = tx.send(res); });
            if device.poll(wgpu::PollType::wait_indefinitely()).is_ok() && rx.recv().map(|r| r.is_ok()).unwrap_or(false) {
                let mapped = slice.get_mapped_range();
                let counts: &[u32] = bytemuck::cast_slice(&mapped);
                let nb = self.n_buckets as usize;
                let mut visible = [0u32; VIEWS];
                let mut lods = [0u32; LODS];
                for v in 0..VIEWS {
                    for b in 0..nb {
                        let n = counts[v * nb + b];
                        visible[v] += n;
                        if v == 3 { lods[b % LODS] += n; }
                    }
                }
                drop(mapped);
                self.stats_readback.unmap();
                let mut s = self.stats.borrow_mut();
                s.visible = visible;
                s.camera_lod = lods;
                println!("[scatter] frame {frame}: visible shadow-far {} / shadow-near {} / sky-cover {} / camera {} (near {} mid {} pixel {})",
                    visible[0], visible[1], visible[2], visible[3], lods[0], lods[1], lods[2]);
            }
        }
    }

    /// The cull passes, before any geometry pass of the frame.
    pub(super) fn encode_cull(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.bucket_count, 0, None);
        encoder.clear_buffer(&self.bucket_cursor, 0, None);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("scatter-cull"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("scatter-cull")),
        });
        pass.set_bind_group(0, &self.cull_bind, &[]);
        pass.set_pipeline(&self.cells_pipe);
        pass.dispatch_workgroups((self.n_cells * VIEWS as u32).div_ceil(WORKGROUP), 1, 1);
        pass.set_pipeline(&self.count_pipe);
        pass.dispatch_workgroups((self.n_instances * VIEWS as u32).div_ceil(WORKGROUP), 1, 1);
        pass.set_pipeline(&self.scan_pipe);
        pass.dispatch_workgroups(VIEWS as u32, 1, 1);
        pass.set_pipeline(&self.write_pipe);
        pass.dispatch_workgroups((self.n_instances * VIEWS as u32).div_ceil(WORKGROUP), 1, 1);
        drop(pass);
        let frame = self.frame.get();
        if self.stats_every > 0 && (frame + 1) % self.stats_every == 0 {
            encoder.copy_buffer_to_buffer(&self.bucket_count, 0, &self.stats_readback, 0, u64::from(self.n_buckets) * VIEWS as u64 * 4);
        }
        self.frame.set(frame + 1);
    }

    /// Draw one view's visible field into the current pass (depth-only for views 0..3, the
    /// G-buffer program for the camera view 3).
    pub(super) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, view: usize, work: &mut super::RenderWorkStats) {
        if self.skip_draw || self.n_instances == 0 {
            return;
        }
        pass.set_pipeline(if view == 3 { &self.gbuffer_pipe } else { &self.depth_pipe });
        pass.set_bind_group(0, &self.draw_binds[view], &[]);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        let view_base = view as u32 * self.n_buckets;
        for page in &self.pages {
            if page.layers == 0 {
                continue;
            }
            pass.set_bind_group(1, &page.bind, &[]);
            let (first, last) = page.bucket_range;
            if self.multi_draw {
                pass.multi_draw_indexed_indirect(&self.indirect, u64::from(view_base + first) * 20, last - first);
                if view == 3 { work.scene_instanced_draws += 1; } else { work.shadow_instanced_draws += 1; }
            } else {
                for b in first..last {
                    pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&b));
                    pass.draw_indexed_indirect(&self.indirect, u64::from(view_base + b) * 20);
                }
                if view == 3 { work.scene_instanced_draws += (last - first) as u64; } else { work.shadow_instanced_draws += (last - first) as u64; }
            }
        }
    }

    /// After the G-buffer: reduce this frame's depth into the Hi-Z pyramid for the next cull.
    pub(super) fn encode_hiz(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder) {
        if self.skip_hiz {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("scatter-hiz"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("scatter-hiz")),
        });
        let (mut w, mut h) = (self.cull_dims[0], self.cull_dims[1]);
        for m in 0..self.hiz_mips {
            pass.set_pipeline(if m == 0 { &self.hiz_copy_pipe } else { &self.hiz_reduce_pipe });
            pass.set_bind_group(0, &self.hiz_binds[m as usize], &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        drop(pass);
        let cam = &r.camera;
        let proj = Mat4::perspective_rh(cam.fov_y, cam.aspect.max(0.01), cam.znear, cam.zfar);
        self.hiz_frame.set(Some((cam.relative_view_proj(), cam.pos, [proj.x_axis.x, proj.y_axis.y])));
        self.hiz_valid.set(true);
    }
}
