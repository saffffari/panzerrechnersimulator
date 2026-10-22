//! Raymarch backend (`KESSEL_RENDERER=raymarch`): a frame pipeline rebuilt around marched
//! light transport through participating media, selectable beside the shader and physical
//! backends. See docs/RAYMARCHING_ENGINE.md.
//!
//! The backend borrows only the scene containers the game already fills (mesh/instance buffers,
//! material bind groups, lights, plumes, particle fields, ballistic records), the physical
//! atmosphere solver for primary sky misses, and the shared display stage (exposure meter, bloom,
//! crush). Every pass between the scene and the display stage is its own program:
//!
//!   depth maps  -> G-buffer -> froxel media lighting (compute) -> integration (compute)
//!   -> deferred shade + fog composite -> HDR one-pixel snow/dust -> HDR sparks/tracers
//!   -> exposure -> bloom -> crush (PS1 quantize/dither) -> ldr
use super::*;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

const VOL_DIVISOR: u32 = 5;
const VOL_SLICES: u32 = 96;
const VOL_NEAR_M: f32 = 0.25;
const VOL_FAR_M: f32 = 320.0;
const CELLS: [u32; 3] = [32, 18, 24];
const CELL_COUNT: usize = (CELLS[0] * CELLS[1] * CELLS[2]) as usize;
const MAX_PARCELS: usize = 16384;
const MAX_CELL_ITEMS: usize = 1 << 18;
const SHADOW_FAR_RES: u32 = 2048;
const SHADOW_NEAR_RES: u32 = 1024;
const SKY_COVER_RES: u32 = 512;
const SKY_COVER_HALF_M: f32 = 48.0;
const POINT_FACE_RES: u32 = 256;
const POINT_SHADOWS: usize = 2;
const SHADOW_VIEWS: usize = 16;
const PUFF_LANES: usize = 13 * 3;
const WEAPON_RECORD_BYTES: u64 = std::mem::size_of::<renderer_weapon_particles::Record>() as u64;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    prev_cam_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_radiance: [f32; 4],
    sky_zenith: [f32; 4],
    sky_horizon: [f32; 4],
    sky_ground: [f32; 4],
    res: [f32; 4],
    vol: [f32; 4],
    vol_range: [f32; 4],
    medium: [f32; 4],
    cabin: [f32; 4],
    wind: [f32; 4],
    shadow_params: [f32; 4],
    light_count: [f32; 4],
    pshadow: [[f32; 4]; 2],
    lights: [[f32; 4]; N_LIGHTS * 2],
    puffs: [[f32; 4]; PUFF_LANES],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowViewsUniform { vp: [[[f32; 4]; 4]; SHADOW_VIEWS] }

/// One binned medium parcel. Layout shared with rm_volume.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Parcel { a: [f32; 4], b: [f32; 4], c: [f32; 4], d: [f32; 4] }

const PARCEL_FIRE: f32 = 0.0;
const PARCEL_SOOT: f32 = 1.0;
const PARCEL_SMOKE: f32 = 2.0;
const PARCEL_GLOW: f32 = 3.0;
const PARCEL_SEGMENT: f32 = 4.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push { model: [[f32; 4]; 4], params: [f32; 4] }

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub parcels: usize,
    pub cell_items: usize,
    pub merged_grains: usize,
    pub shadowed_lights: usize,
    pub spark_records: usize,
    pub tracer_records: usize,
    pub history_valid: bool,
}

struct Target { _tex: wgpu::Texture, view: wgpu::TextureView }

struct Binds {
    depth: wgpu::BindGroup,
    visibility: wgpu::BindGroup,
    volume: [wgpu::BindGroup; 2],
    integrate: [wgpu::BindGroup; 2],
    shade: [wgpu::BindGroup; 2],
    points: wgpu::BindGroup,
}

pub(super) struct Backend {
    pub iw: u32,
    pub ih: u32,
    pub vol: [u32; 3],
    frame_buf: wgpu::Buffer,
    shadow_views_buf: wgpu::Buffer,
    parcels_buf: wgpu::Buffer,
    cell_start_buf: wgpu::Buffer,
    cell_items_buf: wgpu::Buffer,
    weapon_buf: wgpu::Buffer,
    g0: Target,
    g1: Target,
    g2: Target,
    sky: Target,
    shadow_far: Target,
    shadow_near: Target,
    sky_cover: Target,
    point_faces_tex: wgpu::Texture,
    point_faces_array: wgpu::TextureView,
    point_face_views: Vec<wgpu::TextureView>,
    scatter: [Target; 2],
    integrated: Target,
    linear_sampler: wgpu::Sampler,
    cmp_sampler: wgpu::Sampler,
    depth_bgl: wgpu::BindGroupLayout,
    gbuffer_bind: wgpu::BindGroup,
    visibility_bgl: wgpu::BindGroupLayout,
    volume_bgl: wgpu::BindGroupLayout,
    integrate_bgl: wgpu::BindGroupLayout,
    shade_bgl: wgpu::BindGroupLayout,
    points_bgl: wgpu::BindGroupLayout,
    depth_pipe: wgpu::RenderPipeline,
    depth_inst_pipe: wgpu::RenderPipeline,
    gbuffer_pipe: wgpu::RenderPipeline,
    gbuffer_inst_pipe: wgpu::RenderPipeline,
    volume_pipe: wgpu::ComputePipeline,
    integrate_pipe: wgpu::ComputePipeline,
    shade_pipe: wgpu::RenderPipeline,
    points_pipe: wgpu::RenderPipeline,
    sparks_pipe: wgpu::RenderPipeline,
    tracers_pipe: wgpu::RenderPipeline,
    binds: Option<Binds>,
    flip: usize,
    prev_camera: Option<(Vec3, Vec3, f32, f32, Mat4)>,
    history_valid: bool,
    shadow_views: [Mat4; SHADOW_VIEWS],
    shadowed: [Option<usize>; POINT_SHADOWS],
    parcels: Vec<Parcel>,
    pairs: Vec<(u32, u32)>,
    cell_start: Vec<u32>,
    cell_items: Vec<u32>,
    weapon_draws: Vec<(u32, u32)>,
    tracer_range: (u32, u32),
    pub stats: Stats,
}

fn target(device: &wgpu::Device, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format, usage, view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Target { _tex: tex, view }
}

fn volume_target(device: &wgpu::Device, label: &str, dims: [u32; 3]) -> Target {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: dims[0], height: dims[1], depth_or_array_layers: dims[2] },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Target { _tex: tex, view }
}

fn depth_target(device: &wgpu::Device, label: &str, size: u32) -> Target {
    target(device, label, size, size, DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
}

fn tex2d_entry(binding: u32, vis: wgpu::ShaderStages, sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: vis,
        ty: wgpu::BindingType::Texture { sample_type, view_dimension: dim, multisampled: false },
        count: None,
    }
}
fn float_tex(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2)
}
fn float_tex_unfiltered(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Float { filterable: false }, wgpu::TextureViewDimension::D2)
}
fn depth_tex(binding: u32, vis: wgpu::ShaderStages, dim: wgpu::TextureViewDimension) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Depth, dim)
}
fn tex3d(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    tex2d_entry(binding, vis, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D3)
}
fn storage3d(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D3,
        },
        count: None,
    }
}
fn readonly_storage(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding, visibility: vis,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}
fn sampler_filtering(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}
fn sampler_comparison(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None }
}
fn entry<'a>(binding: u32, resource: wgpu::BindingResource<'a>) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry { binding, resource }
}

/// Shader sources are compiled in; `KESSEL_RM_LIVE_SHADERS=1` reads `shaders/rm_*.wgsl` from the
/// asset root at pipeline creation instead, so a look change needs no rebuild.
fn rm_source(name: &str) -> String {
    let compiled = match name {
        "rm_common.wgsl" => COMMON,
        "rm_visibility.wgsl" => VISIBILITY,
        "rm_medium.wgsl" => MEDIUM,
        "rm_depth.wgsl" => include_str!("../shaders/rm_depth.wgsl"),
        "rm_gbuffer.wgsl" => include_str!("../shaders/rm_gbuffer.wgsl"),
        "rm_volume.wgsl" => include_str!("../shaders/rm_volume.wgsl"),
        "rm_integrate.wgsl" => include_str!("../shaders/rm_integrate.wgsl"),
        "rm_shade.wgsl" => include_str!("../shaders/rm_shade.wgsl"),
        "rm_points.wgsl" => include_str!("../shaders/rm_points.wgsl"),
        "rm_sparks.wgsl" => include_str!("../shaders/rm_sparks.wgsl"),
        "terrain_material.wgsl" => include_str!("../shaders/terrain_material.wgsl"),
        _ => panic!("unknown raymarch shader {name}"),
    };
    if std::env::var("KESSEL_RM_LIVE_SHADERS").as_deref() == Ok("1") {
        let root = std::env::var("KESSEL_ASSET_ROOT").unwrap_or_else(|_| ".".into());
        let path = std::path::Path::new(&root).join("shaders").join(name);
        match std::fs::read_to_string(&path) {
            Ok(text) => { println!("[raymarch] live shader {}", path.display()); return text; }
            Err(error) => eprintln!("[raymarch] live shader {} unreadable ({error}); using the compiled copy", path.display()),
        }
    }
    compiled.to_owned()
}

fn rm_shader(device: &wgpu::Device, label: &str, parts: &[&str]) -> wgpu::ShaderModule {
    let source = parts.iter().map(|name| rm_source(name)).collect::<Vec<_>>().join("
");
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

const COMMON: &str = include_str!("../shaders/rm_common.wgsl");
const VISIBILITY: &str = include_str!("../shaders/rm_visibility.wgsl");
const MEDIUM: &str = include_str!("../shaders/rm_medium.wgsl");

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const VERTEX_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    const INST_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
    [
        wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &VERTEX_ATTRS },
        wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Instance, attributes: &INST_ATTRS },
    ]
}

impl Backend {
    pub(super) fn new(device: &wgpu::Device, material_bgl: &wgpu::BindGroupLayout,
        terrain_detail_view: &wgpu::TextureView, terrain_detail_sampler: &wgpu::Sampler, iw: u32, ih: u32) -> Self {
        let vol = [iw.div_ceil(VOL_DIVISOR).max(8), ih.div_ceil(VOL_DIVISOR).max(8), VOL_SLICES];
        println!("[raymarch] froxel grid {}x{}x{} over {:.2}..{:.0} m; cascades {}/{} px, sky cover {} px, {} point-light cubes {} px",
            vol[0], vol[1], vol[2], VOL_NEAR_M, VOL_FAR_M, SHADOW_FAR_RES, SHADOW_NEAR_RES, SKY_COVER_RES, POINT_SHADOWS, POINT_FACE_RES);
        let uniform = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let storage = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let frame_buf = uniform("rm-frame", std::mem::size_of::<FrameUniform>() as u64);
        let shadow_views_buf = uniform("rm-shadow-views", std::mem::size_of::<ShadowViewsUniform>() as u64);
        let parcels_buf = storage("rm-parcels", (MAX_PARCELS * std::mem::size_of::<Parcel>()) as u64);
        let cell_start_buf = storage("rm-cell-start", ((CELL_COUNT + 1) * 4) as u64);
        let cell_items_buf = storage("rm-cell-items", (MAX_CELL_ITEMS * 4) as u64);
        let weapon_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rm-weapon-records"),
            size: (renderer_weapon_particles::math::MAX_BURSTS + renderer_weapon_particles::math::MAX_TRACERS) as u64 * WEAPON_RECORD_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let gusage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let g0 = target(device, "rm-g0-albedo", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let g1 = target(device, "rm-g1-normal", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let g2 = target(device, "rm-g2-emission", iw, ih, wgpu::TextureFormat::Rgba16Float, gusage);
        let sky = target(device, "rm-sky", iw, ih, hdr_format(), gusage);
        let shadow_far = depth_target(device, "rm-shadow-far", SHADOW_FAR_RES);
        let shadow_near = depth_target(device, "rm-shadow-near", SHADOW_NEAR_RES);
        let sky_cover = depth_target(device, "rm-sky-cover", SKY_COVER_RES);
        let point_faces_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rm-point-faces"),
            size: wgpu::Extent3d { width: POINT_FACE_RES, height: POINT_FACE_RES, depth_or_array_layers: (POINT_SHADOWS * 6) as u32 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        });
        let point_faces_array = point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default()
        });
        let point_face_views = (0..(POINT_SHADOWS * 6) as u32).map(|layer| point_faces_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: layer, array_layer_count: Some(1), ..Default::default()
        })).collect();
        let scatter = [volume_target(device, "rm-scatter-a", vol), volume_target(device, "rm-scatter-b", vol)];
        let integrated = volume_target(device, "rm-integrated", vol);
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rm-linear"), address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest, ..Default::default()
        });
        let cmp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rm-compare"), address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, compare: Some(wgpu::CompareFunction::LessEqual), ..Default::default()
        });

        use wgpu::ShaderStages as S;
        let depth_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-depth-bgl"), entries: &[uniform_entry(0, S::VERTEX)],
        });
        let gbuffer_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-gbuffer-bgl"), entries: &[uniform_entry(0, S::VERTEX_FRAGMENT),
                tex2d_entry(1, S::FRAGMENT, wgpu::TextureSampleType::Float { filterable: true }, wgpu::TextureViewDimension::D2Array),
                sampler_filtering(2, S::FRAGMENT)],
        });
        let gbuffer_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-gbuffer"), layout: &gbuffer_bgl, entries: &[
                entry(0, frame_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(terrain_detail_view)),
                entry(2, wgpu::BindingResource::Sampler(terrain_detail_sampler)),
            ],
        });
        let vis_stages = S::FRAGMENT | S::COMPUTE;
        let visibility_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-visibility-bgl"), entries: &[
                uniform_entry(0, vis_stages),
                depth_tex(1, vis_stages, wgpu::TextureViewDimension::D2),
                depth_tex(2, vis_stages, wgpu::TextureViewDimension::D2),
                sampler_comparison(3, vis_stages),
                depth_tex(4, vis_stages, wgpu::TextureViewDimension::D2),
                depth_tex(5, vis_stages, wgpu::TextureViewDimension::D2Array),
            ],
        });
        let volume_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-volume-bgl"), entries: &[
                uniform_entry(0, S::COMPUTE), readonly_storage(1, S::COMPUTE), readonly_storage(2, S::COMPUTE), readonly_storage(3, S::COMPUTE),
                tex3d(4, S::COMPUTE), sampler_filtering(5, S::COMPUTE), storage3d(6), depth_tex(7, S::COMPUTE, wgpu::TextureViewDimension::D2),
            ],
        });
        let integrate_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-integrate-bgl"), entries: &[uniform_entry(0, S::COMPUTE), tex3d(1, S::COMPUTE), storage3d(2)],
        });
        let shade_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-shade-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), float_tex_unfiltered(1, S::FRAGMENT), float_tex_unfiltered(2, S::FRAGMENT), float_tex_unfiltered(3, S::FRAGMENT),
                depth_tex(4, S::FRAGMENT, wgpu::TextureViewDimension::D2), float_tex_unfiltered(5, S::FRAGMENT),
                tex3d(6, S::FRAGMENT), tex3d(7, S::FRAGMENT), sampler_filtering(8, S::FRAGMENT),
            ],
        });
        let points_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rm-points-bgl"), entries: &[
                uniform_entry(0, S::VERTEX_FRAGMENT), depth_tex(1, S::FRAGMENT, wgpu::TextureViewDimension::D2),
                tex3d(2, S::FRAGMENT), sampler_filtering(3, S::FRAGMENT),
            ],
        });

        let push_range = |stages: wgpu::ShaderStages| wgpu::PushConstantRange { stages, range: 0..std::mem::size_of::<Push>() as u32 };
        let [vertex_layout, inst_layout] = vertex_layouts();
        let depth_state = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH, depth_write_enabled: write, depth_compare: compare, stencil: Default::default(), bias,
        };

        // Depth-only program for every shadow/cover map.
        let depth_shader = rm_shader(device, "rm-depth", &["rm_depth.wgsl"]);
        let depth_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-depth-pl"), bind_group_layouts: &[&depth_bgl], push_constant_ranges: &[push_range(S::VERTEX)],
        });
        let depth_pipeline = |label: &str, entry: &str, buffers: &[wgpu::VertexBufferLayout]| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&depth_pl),
            vertex: wgpu::VertexState { module: &depth_shader, entry_point: Some(entry), buffers, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 1, slope_scale: 1.5, clamp: 0.0 })),
            multisample: Default::default(), fragment: None, multiview: None, cache: None,
        });
        let depth_pipe = depth_pipeline("rm-depth", "vs", std::slice::from_ref(&vertex_layout));
        let depth_inst_pipe = depth_pipeline("rm-depth-inst", "vs_inst", &[vertex_layout.clone(), inst_layout.clone()]);

        // G-buffer.
        let gbuffer_shader = rm_shader(device, "rm-gbuffer", &["rm_common.wgsl", "terrain_material.wgsl", "rm_gbuffer.wgsl"]);
        let gbuffer_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-gbuffer-pl"), bind_group_layouts: &[&gbuffer_bgl, material_bgl], push_constant_ranges: &[push_range(S::VERTEX_FRAGMENT)],
        });
        let gtargets: Vec<Option<wgpu::ColorTargetState>> = (0..3).map(|_| Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba16Float, blend: None, write_mask: wgpu::ColorWrites::ALL,
        })).collect();
        let gbuffer_pipeline = |label: &str, entry: &str, buffers: &[wgpu::VertexBufferLayout]| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&gbuffer_pl),
            vertex: wgpu::VertexState { module: &gbuffer_shader, entry_point: Some(entry), buffers, compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &gbuffer_shader, entry_point: Some("fs"), targets: &gtargets, compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let gbuffer_pipe = gbuffer_pipeline("rm-gbuffer", "vs", std::slice::from_ref(&vertex_layout));
        let gbuffer_inst_pipe = gbuffer_pipeline("rm-gbuffer-inst", "vs_inst", &[vertex_layout.clone(), inst_layout.clone()]);

        // Froxel lighting and integration.
        let volume_shader = rm_shader(device, "rm-volume", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", "rm_volume.wgsl"]);
        let volume_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-volume-pl"), bind_group_layouts: &[&volume_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        let volume_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-volume"), layout: Some(&volume_pl), module: &volume_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
        });
        let integrate_shader = rm_shader(device, "rm-integrate", &["rm_common.wgsl", "rm_integrate.wgsl"]);
        let integrate_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-integrate-pl"), bind_group_layouts: &[&integrate_bgl], push_constant_ranges: &[],
        });
        let integrate_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rm-integrate"), layout: Some(&integrate_pl), module: &integrate_shader, entry_point: Some("cs"), compilation_options: Default::default(), cache: None,
        });

        // Deferred shade + fog composite.
        let shade_shader = rm_shader(device, "rm-shade", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_medium.wgsl", "rm_shade.wgsl"]);
        let shade_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-shade-pl"), bind_group_layouts: &[&shade_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        let shade_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-shade"), layout: Some(&shade_pl),
            vertex: wgpu::VertexState { module: &shade_shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &shade_shader, entry_point: Some("fs"), targets: &[Some(color_target(hdr_format()))], compilation_options: Default::default() }),
            multiview: None, cache: None,
        });

        // HDR one-pixel particles and ballistic emitters.
        let premultiplied = Some(wgpu::ColorTargetState { format: hdr_format(), blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL });
        let points_shader = rm_shader(device, "rm-points", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_points.wgsl"]);
        let points_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rm-points-pl"), bind_group_layouts: &[&points_bgl, &visibility_bgl], push_constant_ranges: &[],
        });
        const POINT_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32];
        let points_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rm-points"), layout: Some(&points_pl),
            vertex: wgpu::VertexState { module: &points_shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16, step_mode: wgpu::VertexStepMode::Instance, attributes: &POINT_ATTRS }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &points_shader, entry_point: Some("fs"), targets: std::slice::from_ref(&premultiplied), compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let sparks_shader = rm_shader(device, "rm-sparks", &["rm_common.wgsl", "rm_visibility.wgsl", "rm_sparks.wgsl"]);
        const WEAPON_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Uint32x4];
        let weapon_pipeline = |label: &str, entry: &str| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label), layout: Some(&points_pl),
            vertex: wgpu::VertexState { module: &sparks_shader, entry_point: Some(entry), compilation_options: Default::default(), buffers: &[wgpu::VertexBufferLayout {
                array_stride: WEAPON_RECORD_BYTES, step_mode: wgpu::VertexStepMode::Instance, attributes: &WEAPON_ATTRS }] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() }, depth_stencil: None, multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &sparks_shader, entry_point: Some("fs_weapon"), targets: std::slice::from_ref(&premultiplied), compilation_options: Default::default() }),
            multiview: None, cache: None,
        });
        let sparks_pipe = weapon_pipeline("rm-sparks", "vs_spark");
        let tracers_pipe = weapon_pipeline("rm-tracers", "vs_tracer");

        Self {
            iw, ih, vol, frame_buf, shadow_views_buf, parcels_buf, cell_start_buf, cell_items_buf, weapon_buf,
            g0, g1, g2, sky, shadow_far, shadow_near, sky_cover, point_faces_tex, point_faces_array, point_face_views,
            scatter, integrated, linear_sampler, cmp_sampler,
            depth_bgl, gbuffer_bind, visibility_bgl, volume_bgl, integrate_bgl, shade_bgl, points_bgl,
            depth_pipe, depth_inst_pipe, gbuffer_pipe, gbuffer_inst_pipe, volume_pipe, integrate_pipe, shade_pipe, points_pipe, sparks_pipe, tracers_pipe,
            binds: None, flip: 0, prev_camera: None, history_valid: false,
            shadow_views: [Mat4::IDENTITY; SHADOW_VIEWS], shadowed: [None; POINT_SHADOWS],
            parcels: Vec::with_capacity(MAX_PARCELS), pairs: Vec::new(), cell_start: vec![0; CELL_COUNT + 1], cell_items: Vec::new(),
            weapon_draws: Vec::new(), tracer_range: (0, 0), stats: Stats::default(),
        }
    }

    fn make_binds(&self, device: &wgpu::Device, r: &Renderer) -> Binds {
        let frame = self.frame_buf.as_entire_binding();
        let depth = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-depth"), layout: &self.depth_bgl, entries: &[entry(0, self.shadow_views_buf.as_entire_binding())],
        });
        let visibility = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-visibility"), layout: &self.visibility_bgl, entries: &[
                entry(0, self.shadow_views_buf.as_entire_binding()),
                entry(1, wgpu::BindingResource::TextureView(&self.shadow_far.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.shadow_near.view)),
                entry(3, wgpu::BindingResource::Sampler(&self.cmp_sampler)),
                entry(4, wgpu::BindingResource::TextureView(&self.sky_cover.view)),
                entry(5, wgpu::BindingResource::TextureView(&self.point_faces_array)),
            ],
        });
        let volume = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-volume"), layout: &self.volume_bgl, entries: &[
                entry(0, frame.clone()), entry(1, self.parcels_buf.as_entire_binding()),
                entry(2, self.cell_start_buf.as_entire_binding()), entry(3, self.cell_items_buf.as_entire_binding()),
                entry(4, wgpu::BindingResource::TextureView(&self.scatter[1 - i].view)),
                entry(5, wgpu::BindingResource::Sampler(&self.linear_sampler)),
                entry(6, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(7, wgpu::BindingResource::TextureView(&r.depth_view)),
            ],
        }));
        let integrate = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-integrate"), layout: &self.integrate_bgl, entries: &[
                entry(0, frame.clone()), entry(1, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(2, wgpu::BindingResource::TextureView(&self.integrated.view)),
            ],
        }));
        let shade = std::array::from_fn(|i| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-shade"), layout: &self.shade_bgl, entries: &[
                entry(0, frame.clone()),
                entry(1, wgpu::BindingResource::TextureView(&self.g0.view)),
                entry(2, wgpu::BindingResource::TextureView(&self.g1.view)),
                entry(3, wgpu::BindingResource::TextureView(&self.g2.view)),
                entry(4, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(5, wgpu::BindingResource::TextureView(&self.sky.view)),
                entry(6, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(7, wgpu::BindingResource::TextureView(&self.scatter[i].view)),
                entry(8, wgpu::BindingResource::Sampler(&self.linear_sampler)),
            ],
        }));
        let points = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rm-points"), layout: &self.points_bgl, entries: &[
                entry(0, frame.clone()), entry(1, wgpu::BindingResource::TextureView(&r.depth_view)),
                entry(2, wgpu::BindingResource::TextureView(&self.integrated.view)),
                entry(3, wgpu::BindingResource::Sampler(&self.linear_sampler)),
            ],
        });
        Binds { depth, visibility, volume, integrate, shade, points }
    }

    /// Per-frame CPU side: camera/history state, source radiance, shadow views, medium parcels,
    /// ballistic records. Runs from `Renderer::upload_uniforms` after the shared uniforms.
    pub(super) fn prepare(&mut self, r: &Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.binds.is_none() { self.binds = Some(self.make_binds(device, r)); }
        self.flip = 1 - self.flip;
        let cam = &r.camera;
        let eye = cam.pos;
        let rel_vp = cam.relative_view_proj();
        let forward = cam.forward();
        // History survives continuous motion only; a cut (seat change, optic zoom, kill-cam)
        // restarts the froxel accumulation.
        let (prev_vp, prev_eye, valid) = match self.prev_camera {
            Some((p_eye, p_fwd, p_fov, p_aspect, p_vp)) => {
                let cut = temporal_camera_cut((eye - p_eye).length(), forward.dot(p_fwd), cam.fov_y - p_fov,
                    cam.aspect - p_aspect, false, r.continuous_camera_translation);
                (p_vp, p_eye, !cut)
            }
            None => (rel_vp, eye, false),
        };
        self.history_valid = valid;
        self.prev_camera = Some((eye, forward, cam.fov_y, cam.aspect, rel_vp));

        // Source radiance from the shared physical atmosphere: key-beam irradiance at the eye and
        // mean sky radiance bands. The same solver renders the visible sky.
        let atmosphere = r.raymarch_atmosphere();
        let quality = r.transport_quality.environment(r.graphics_preset);
        let sun_dir = r.sun_dir.normalize_or_zero();
        let origin = eye.as_dvec3();
        let beam_on = sun_dir.y > -0.02 && atmosphere.solar_irradiance.max_element() > 0.0;
        let sun_radiance = if beam_on {
            atmosphere.solar_irradiance * physical_environment::solar_transmittance_at(&atmosphere, origin, sun_dir.as_dvec3(), quality.source_steps.max(4))
        } else { glam::DVec3::ZERO };
        let sample_sky = |dir: glam::DVec3| physical_environment::integrate_environment(&atmosphere, origin, dir, f64::INFINITY, quality).inscattered_radiance;
        let mut zenith = sample_sky(glam::DVec3::Y);
        for i in 0..4 {
            let a = i as f64 * std::f64::consts::FRAC_PI_2 + 0.4;
            zenith += sample_sky(glam::DVec3::new(a.cos() * 0.5736, 0.8192, a.sin() * 0.5736));
        }
        zenith /= 5.0;
        let mut horizon = glam::DVec3::ZERO;
        for i in 0..6 {
            let a = i as f64 * std::f64::consts::FRAC_PI_3 + 0.2;
            horizon += sample_sky(glam::DVec3::new(a.cos() * 0.9781, 0.2079, a.sin() * 0.9781));
        }
        horizon /= 6.0;
        let ground_albedo = 0.30 + 0.45 * r.precipitation.clamp(0.0, 1.0) as f64;
        let ground = (horizon * 0.6 + zenith * 0.4) * ground_albedo + sun_radiance * (sun_dir.y.max(0.0) as f64) * ground_albedo / std::f64::consts::PI;

        // Lamps: the game's light list, with the two strongest nearby ones given depth faces.
        let mut lights = [[0.0f32; 4]; N_LIGHTS * 2];
        let n_lights = r.lights.len().min(N_LIGHTS);
        for (i, l) in r.lights.iter().take(N_LIGHTS).enumerate() {
            lights[i * 2] = [l.pos.x, l.pos.y, l.pos.z, l.source_radius.max(0.005)];
            lights[i * 2 + 1] = [l.color.x, l.color.y, l.color.z, l.radiant_intensity.max(0.0)];
        }
        let mut ranked: Vec<(f32, usize)> = r.lights.iter().take(N_LIGHTS).enumerate().filter_map(|(i, l)| {
            let d2 = (l.pos - eye).length_squared();
            let power = l.radiant_intensity * l.color.max_element();
            (d2 < 45.0 * 45.0 && power > 0.0).then(|| (power / (d2 + 1.0), i))
        }).collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        self.shadowed = [None; POINT_SHADOWS];
        let mut pshadow = [[0.0f32; 4]; POINT_SHADOWS];
        for (slot, &(_, index)) in ranked.iter().take(POINT_SHADOWS).enumerate() {
            let l = &r.lights[index];
            let range = (l.radiant_intensity * l.color.max_element() / 0.004).sqrt().clamp(2.0, 40.0);
            self.shadowed[slot] = Some(index);
            pshadow[slot] = [l.pos.x, l.pos.y, l.pos.z, range];
        }

        // Visibility views.
        let near_half = if r.interior_light > 0.02 { 6.0 } else { 20.0 };
        let focus = eye + forward * SOLO_SHADOW_LOOK_AHEAD_M;
        self.shadow_views = [Mat4::IDENTITY; SHADOW_VIEWS];
        self.shadow_views[0] = stable_shadow_view_proj(sun_dir, focus, SOLO_SHADOW_HALF_M, FAR_SHADOW_DEPTH_HALF_M, SHADOW_FAR_RES);
        self.shadow_views[1] = stable_shadow_view_proj(sun_dir, eye, near_half, 60.0, SHADOW_NEAR_RES);
        {
            let texel = 2.0 * SKY_COVER_HALF_M / SKY_COVER_RES as f32;
            let snapped = Vec3::new((eye.x / texel).round() * texel, eye.y, (eye.z / texel).round() * texel);
            let top = snapped + Vec3::Y * 150.0;
            self.shadow_views[2] = Mat4::orthographic_rh(-SKY_COVER_HALF_M, SKY_COVER_HALF_M, -SKY_COVER_HALF_M, SKY_COVER_HALF_M, 0.1, 400.0)
                * Mat4::look_to_rh(top, -Vec3::Y, Vec3::Z);
        }
        for slot in 0..POINT_SHADOWS {
            let Some(index) = self.shadowed[slot] else { continue };
            let pos = r.lights[index].pos;
            let range = pshadow[slot][3];
            let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.03, range);
            let faces = [(Vec3::X, -Vec3::Y), (-Vec3::X, -Vec3::Y), (Vec3::Y, Vec3::Z), (-Vec3::Y, -Vec3::Z), (Vec3::Z, -Vec3::Y), (-Vec3::Z, -Vec3::Y)];
            for (face, (dir, up)) in faces.iter().enumerate() {
                self.shadow_views[3 + slot * 6 + face] = proj * Mat4::look_to_rh(pos, *dir, *up);
            }
        }
        queue.write_buffer(&self.shadow_views_buf, 0, bytemuck::bytes_of(&ShadowViewsUniform {
            vp: self.shadow_views.map(|m| m.to_cols_array_2d()),
        }));

        // Media.
        let world_medium = world_medium_profile(r.precipitation, r.storm);
        let gust = weather_gust(r.time, r.weather_seed);
        let wind = r.weather_wind_world * gust;
        let mut puffs = [[0.0f32; 4]; PUFF_LANES];
        for (i, p) in r.smoke_puffs.iter().chain(std::iter::once(&r.breech_gas)).enumerate() {
            puffs[i * 3] = [p.pos[0], p.pos[1], p.pos[2], p.age];
            puffs[i * 3 + 1] = [p.dir[0], p.dir[1], p.dir[2], p.strength];
            puffs[i * 3 + 2] = p.shape;
        }
        let cabin_sigma = if r.atmo_enabled { r.atmo_haze * 0.010 * 5.2 } else { 0.0 };

        self.collect_parcels(r, rel_vp, eye);
        self.collect_weapon_records(r, queue, rel_vp, eye);

        let frame = FrameUniform {
            view_proj: rel_vp.to_cols_array_2d(),
            inv_view_proj: rel_vp.inverse().to_cols_array_2d(),
            prev_view_proj: prev_vp.to_cols_array_2d(),
            cam_pos: [eye.x, eye.y, eye.z, r.time],
            prev_cam_pos: [prev_eye.x, prev_eye.y, prev_eye.z, if valid { 1.0 } else { 0.0 }],
            sun_dir: [sun_dir.x, sun_dir.y, sun_dir.z, r.night.clamp(0.0, 1.0)],
            sun_radiance: [sun_radiance.x as f32, sun_radiance.y as f32, sun_radiance.z as f32, if beam_on { 1.0 } else { 0.0 }],
            sky_zenith: [zenith.x as f32, zenith.y as f32, zenith.z as f32, r.overcast.clamp(0.0, 1.0)],
            sky_horizon: [horizon.x as f32, horizon.y as f32, horizon.z as f32, r.storm.clamp(0.0, 1.0)],
            sky_ground: [ground.x as f32, ground.y as f32, ground.z as f32, r.precipitation.clamp(0.0, 1.0)],
            res: [self.iw as f32, self.ih as f32, 1.0 / self.iw as f32, 1.0 / self.ih as f32],
            vol: [self.vol[0] as f32, self.vol[1] as f32, self.vol[2] as f32, (r.frame & 0xFFFF) as f32],
            vol_range: [VOL_NEAR_M, VOL_FAR_M, (VOL_FAR_M / VOL_NEAR_M).ln(), r.interior_light.clamp(0.0, 1.0)],
            medium: [r.fog_density.max(0.0), world_medium.snow_fraction, world_medium.layer[0], world_medium.layer[1]],
            cabin: [r.atmo_origin.x, r.atmo_origin.y, r.atmo_origin.z, cabin_sigma],
            wind: [wind.x, r.atmosphere_mood.clamp(0.0, 1.0), wind.y, gust],
            shadow_params: [SOLO_SHADOW_HALF_M, near_half, SHADOW_FAR_RES as f32, SHADOW_NEAR_RES as f32],
            light_count: [n_lights as f32, self.shadowed[0].map_or(-1.0, |i| i as f32), self.shadowed[1].map_or(-1.0, |i| i as f32), self.parcels.len() as f32],
            pshadow,
            lights,
            puffs,
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&frame));
        self.stats.shadowed_lights = self.shadowed.iter().flatten().count();
        self.stats.history_valid = valid;
    }

    /// Fire, soot and smoke grains from the simulation's instance pools, merged into cells, plus
    /// the glow of spark bursts and tracer strips, binned into the froxel grid's coarse cells.
    fn collect_parcels(&mut self, r: &Renderer, rel_vp: Mat4, eye: Vec3) {
        self.parcels.clear();
        self.pairs.clear();
        self.stats.merged_grains = 0;
        #[derive(Default)]
        struct Cell { sum: Vec3, count: u32, size: f32 }
        let mut merged: HashMap<(i32, i32, i32, u32), Cell> = HashMap::new();
        let mut cell_kinds: Vec<(u32, f32, [f32; 3], f32)> = Vec::new(); // batch -> (kind, colour, emissive)
        for (batch_index, b) in r.inst_batches.iter().enumerate() {
            if b.inst_count == 0 { continue; }
            let kind = if (b.flags - 5.0).abs() < 0.5 { PARCEL_FIRE }
                else if (b.flags - 6.0).abs() < 0.5 { PARCEL_SOOT }
                else if b.flags < -0.5 { PARCEL_SMOKE }
                else { continue };
            let color = r.fine_vfx.material_color(batch_index).unwrap_or([0.5, 0.5, 0.5]);
            cell_kinds.push((batch_index as u32, kind, color, b.emissive));
            for (i, mat) in b.cpu_mats.iter().take(b.inst_count as usize).enumerate() {
                if b.is_hidden(i as u32) { continue; }
                let c = mat.w_axis.truncate();
                if !c.is_finite() { continue; }
                let size = (mat.x_axis.length() + mat.y_axis.length() + mat.z_axis.length()) / 3.0;
                let d = (c - eye).length();
                if d > VOL_FAR_M + size { continue; }
                // Merge cell: a few grains deep, scaled with distance so far fires stay compact.
                let cs = (size * 3.0).clamp(0.12, 0.6) * (1.0 + d / 40.0).min(4.0);
                let key = ((c.x / cs).floor() as i32, (c.y / cs).floor() as i32, (c.z / cs).floor() as i32, batch_index as u32);
                let cell = merged.entry(key).or_default();
                cell.sum += c;
                cell.count += 1;
                cell.size = cell.size.max(size);
                self.stats.merged_grains += 1;
            }
        }
        let mut cells: Vec<((i32, i32, i32, u32), Cell)> = merged.into_iter().collect();
        cells.sort_by(|a, b| {
            let da = (a.1.sum / a.1.count as f32 - eye).length_squared();
            let db = (b.1.sum / b.1.count as f32 - eye).length_squared();
            da.total_cmp(&db)
        });
        for (key, cell) in cells {
            if self.parcels.len() >= MAX_PARCELS - 256 { break; }
            let Some(&(_, kind, color, emissive)) = cell_kinds.iter().find(|k| k.0 == key.3) else { continue };
            let center = cell.sum / cell.count as f32;
            let d = (center - eye).length();
            let cs = (cell.size * 3.0).clamp(0.12, 0.6) * (1.0 + d / 40.0).min(4.0);
            let radius = (cs * 0.8).max(cell.size * 1.5);
            // Fraction of the cell's cross-section the grains actually cover.
            let coverage = ((cell.count as f32 * cell.size * cell.size) / (radius * radius)).clamp(0.08, 1.0);
            let (emission, extinction) = match kind {
                k if k == PARCEL_FIRE => (emissive * coverage * 3.0 / radius, 0.6 * coverage / radius),
                k if k == PARCEL_SOOT => (0.0, 5.0 * coverage / radius),
                _ => (0.0, 3.0 * coverage / radius),
            };
            self.parcels.push(Parcel {
                a: [center.x, center.y, center.z, radius],
                b: [color[0], color[1], color[2], kind],
                c: [emission, extinction, (key.0 ^ key.1 ^ key.2) as f32, 0.0],
                d: [0.0; 4],
            });
        }
        // Spark showers glow in the medium around their emitter; tracers along their strip.
        use renderer_weapon_particles::math as wm;
        for burst in r.weapon_particles.bursts.iter().filter(|b| b.active()) {
            if self.parcels.len() >= MAX_PARCELS { break; }
            let age: f32 = burst.origin_age[3].max(0.0);
            let count = burst.meta[1] as f32;
            let kind = burst.meta[3];
            if kind == wm::GROUND || count <= 0.0 { continue; }
            let origin = burst.origin();
            let n = Vec3::from_array([burst.normal_offset[0], burst.normal_offset[1], burst.normal_offset[2]]);
            let (center, radius, fade, color) = match kind {
                k if k == wm::HOT_RIM => (origin, (burst.parameters[0] + burst.parameters[1]) * 2.0 + 0.05, (-age / 2.5).exp(), [1.0, 0.45, 0.12]),
                k if k == wm::FULL_COOKOFF => (origin + Vec3::Y * (20.0 * age).min(120.0), (6.0 + 12.0 * age).min(60.0), (1.0 - age / 17.0).max(0.0), [1.0, 0.59, 0.05]),
                k if k == wm::PRESSURE_VENT => (origin + n * (4.0 * age).min(3.0), (0.5 + 3.0 * age).min(3.0), (1.0 - age / 0.65).max(0.0), [1.0, 0.59, 0.05]),
                _ => (origin + n * 0.25 + Vec3::Y * (-2.0 * age * age).max(-1.5) + n * 3.0 * age, (0.35 + 2.5 * age).min(3.0), (1.0 - age / 1.6).max(0.0), [1.0, 0.42, 0.06]),
            };
            if fade <= 0.0 { continue; }
            let strength = (count / 4000.0).min(6.0).sqrt() * fade;
            self.parcels.push(Parcel {
                a: [center.x, center.y, center.z, radius],
                b: [color[0], color[1], color[2], PARCEL_GLOW],
                c: [strength * 0.9 / radius, 0.0, 0.0, 0.0],
                d: [0.0; 4],
            });
        }
        for segment in &r.weapon_particles.segments {
            if self.parcels.len() >= MAX_PARCELS { break; }
            let a = Vec3::from_array([segment.origin_age[0], segment.origin_age[1], segment.origin_age[2]]);
            let b = Vec3::from_array([segment.direction_life[0], segment.direction_life[1], segment.direction_life[2]]);
            self.parcels.push(Parcel {
                a: [a.x, a.y, a.z, 0.35],
                b: [1.0, 0.4621, 0.0296, PARCEL_SEGMENT],
                c: [1.2, 0.0, 0.0, 0.0],
                d: [b.x, b.y, b.z, 0.0],
            });
        }
        // Bin into coarse froxel cells.
        let vz = self.vol[2] as f32;
        let ln_range = (VOL_FAR_M / VOL_NEAR_M).ln();
        for (index, parcel) in self.parcels.iter().enumerate() {
            let center = Vec3::from_array([parcel.a[0], parcel.a[1], parcel.a[2]]);
            let r = parcel.a[3];
            let (lo_c, hi_c) = if parcel.b[3] == PARCEL_SEGMENT {
                let end = Vec3::from_array([parcel.d[0], parcel.d[1], parcel.d[2]]);
                (center.min(end) - Vec3::splat(r), center.max(end) + Vec3::splat(r))
            } else { (center - Vec3::splat(r), center + Vec3::splat(r)) };
            let mut lo = [i32::MAX; 2];
            let mut hi = [i32::MIN; 2];
            let mut dmin = f32::MAX;
            let mut dmax = 0.0f32;
            let mut behind = false;
            let mut front = false;
            for corner in 0..8 {
                let p = Vec3::new(if corner & 1 == 0 { lo_c.x } else { hi_c.x }, if corner & 2 == 0 { lo_c.y } else { hi_c.y }, if corner & 4 == 0 { lo_c.z } else { hi_c.z }) - eye;
                let d = p.length();
                dmin = dmin.min(d);
                dmax = dmax.max(d);
                let clip = rel_vp * p.extend(1.0);
                if clip.w <= 0.02 { behind = true; continue; }
                front = true;
                let ndc = clip.truncate() / clip.w;
                let cx = ((ndc.x * 0.5 + 0.5) * CELLS[0] as f32).floor() as i32;
                let cy = ((0.5 - ndc.y * 0.5) * CELLS[1] as f32).floor() as i32;
                lo[0] = lo[0].min(cx); hi[0] = hi[0].max(cx);
                lo[1] = lo[1].min(cy); hi[1] = hi[1].max(cy);
            }
            if !front { continue; }
            if behind { lo = [0, 0]; hi = [CELLS[0] as i32 - 1, CELLS[1] as i32 - 1]; }
            if (center - eye).length() < r { dmin = 0.0; }
            let slice = |d: f32| ((d.max(VOL_NEAR_M) / VOL_NEAR_M).ln() / ln_range * vz);
            let z0 = ((slice(dmin) / vz) * CELLS[2] as f32).floor() as i32;
            let z1 = ((slice(dmax) / vz) * CELLS[2] as f32).floor() as i32;
            let x0 = lo[0].clamp(0, CELLS[0] as i32 - 1); let x1 = hi[0].clamp(0, CELLS[0] as i32 - 1);
            let y0 = lo[1].clamp(0, CELLS[1] as i32 - 1); let y1 = hi[1].clamp(0, CELLS[1] as i32 - 1);
            let z0 = z0.clamp(0, CELLS[2] as i32 - 1); let z1 = z1.clamp(0, CELLS[2] as i32 - 1);
            if lo[0] > hi[0] || lo[1] > hi[1] || dmin > VOL_FAR_M { continue; }
            for z in z0..=z1 { for y in y0..=y1 { for x in x0..=x1 {
                if self.pairs.len() >= MAX_CELL_ITEMS { break; }
                let cell = (z as u32 * CELLS[1] + y as u32) * CELLS[0] + x as u32;
                self.pairs.push((cell, index as u32));
            } } }
        }
        // Counting sort into CSR lists.
        self.cell_start.iter_mut().for_each(|c| *c = 0);
        for &(cell, _) in &self.pairs { self.cell_start[cell as usize + 1] += 1; }
        for i in 0..CELL_COUNT { self.cell_start[i + 1] += self.cell_start[i]; }
        self.cell_items.clear();
        self.cell_items.resize(self.pairs.len(), 0);
        let mut cursor: Vec<u32> = self.cell_start[..CELL_COUNT].to_vec();
        for &(cell, index) in &self.pairs {
            let slot = &mut cursor[cell as usize];
            self.cell_items[*slot as usize] = index;
            *slot += 1;
        }
        self.stats.parcels = self.parcels.len();
        self.stats.cell_items = self.cell_items.len();
    }

    fn upload_parcels(&self, queue: &wgpu::Queue) {
        if !self.parcels.is_empty() { queue.write_buffer(&self.parcels_buf, 0, bytemuck::cast_slice(&self.parcels)); }
        queue.write_buffer(&self.cell_start_buf, 0, bytemuck::cast_slice(&self.cell_start));
        if !self.cell_items.is_empty() { queue.write_buffer(&self.cell_items_buf, 0, bytemuck::cast_slice(&self.cell_items)); }
    }

    fn collect_weapon_records(&mut self, r: &Renderer, queue: &wgpu::Queue, rel_vp: Mat4, eye: Vec3) {
        use renderer_weapon_particles::math as wm;
        self.upload_parcels(queue);
        self.weapon_draws.clear();
        let planes = frustum_planes(rel_vp);
        let mut bursts: Vec<wm::Record> = r.weapon_particles.bursts.iter().copied().filter(|b| b.active()).collect();
        bursts.sort_by(|a, b| a.origin().distance_squared(eye).total_cmp(&b.origin().distance_squared(eye)));
        let mut records: Vec<wm::Record> = Vec::with_capacity(bursts.len() + r.weapon_particles.segments.len());
        let mut available = wm::MAX_SPARKS_PER_VIEW;
        for burst in bursts {
            let center = burst.origin() - eye;
            let radius = Vec3::splat(burst.support_radius());
            if !aabb_in_frustum(&planes, center - radius, center + radius) { continue; }
            let count = burst.meta[1];
            if count > available { continue; }
            available -= count;
            self.weapon_draws.push((records.len() as u32, count));
            records.push(burst);
        }
        self.weapon_draws.sort_by(|a, b| records[b.0 as usize].origin().distance_squared(eye)
            .total_cmp(&records[a.0 as usize].origin().distance_squared(eye)));
        self.tracer_range = (records.len() as u32, r.weapon_particles.segments.len() as u32);
        records.extend_from_slice(&r.weapon_particles.segments);
        if !records.is_empty() { queue.write_buffer(&self.weapon_buf, 0, bytemuck::cast_slice(&records)); }
        self.stats.spark_records = self.weapon_draws.len();
        self.stats.tracer_records = self.tracer_range.1 as usize;
    }

    /// Draw every opaque source mesh and instanced page visible from `vp` with the given programs.
    #[allow(clippy::too_many_arguments)]
    fn draw_scene<'a>(&'a self, r: &'a Renderer, pass: &mut wgpu::RenderPass<'a>, vp: Mat4, view_index: f32,
        shadow: bool, mesh_pipe: &'a wgpu::RenderPipeline, inst_pipe: &'a wgpu::RenderPipeline, work: &mut RenderWorkStats) {
        let planes = frustum_planes(vp);
        let visible = r.inst_spatial_index.query(vp);
        let stages = if shadow { wgpu::ShaderStages::VERTEX } else { wgpu::ShaderStages::VERTEX_FRAGMENT };
        pass.set_pipeline(inst_pipe);
        for &batch in &visible.batches {
            let b = &r.inst_batches[batch];
            if b.inst_count == 0 { continue; }
            let flags = b.flags;
            // Smoke cubes, flame and soot grains are media here; glass sheen panes are open.
            if flags < -0.5 || (flags > 4.5 && flags < 6.5) || flags > 7.5 { continue; }
            if shadow && (flags > 3.5 || b.raster_only_dynamic) { continue; }
            let p = &r.inst_prototypes[b.prototype];
            let push = Push {
                model: Mat4::IDENTITY.to_cols_array_2d(),
                params: if shadow { [view_index, 0.0, 0.0, 0.0] } else { [b.emissive, p.metallic_factor, p.roughness_factor, if flags > 3.5 { 4.0 } else { 0.0 }] },
            };
            pass.set_push_constants(stages, 0, bytemuck::bytes_of(&push));
            if !shadow { pass.set_bind_group(1, p.tex_bind.as_ref(), &[]); }
            pass.set_vertex_buffer(0, p.vbuf.slice(..));
            pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
            pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            if shadow { work.shadow_instanced_draws += 1; } else { work.scene_instanced_draws += 1; }
            pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
        }
        pass.set_pipeline(mesh_pipe);
        for m in &r.meshes {
            if !m.visible || m.glass { continue; }
            if !shadow && r.optic_hide_pass && m.optic_hidden { continue; }
            if !aabb_in_frustum(&planes, m.world_min, m.world_max) { continue; }
            let kind = if m.track_recv { 3.0 } else if m.emissive_own { 2.0 } else if m.emissive_win { 1.0 } else { 0.0 };
            let push = Push {
                model: m.model.to_cols_array_2d(),
                params: if shadow { [view_index, 0.0, 0.0, 0.0] } else { [m.emissive, m.metallic_factor, m.roughness_factor, kind] },
            };
            pass.set_push_constants(stages, 0, bytemuck::bytes_of(&push));
            if !shadow { pass.set_bind_group(1, m.tex_bind.as_ref(), &[]); }
            pass.set_vertex_buffer(0, m.vbuf.slice(..));
            pass.set_index_buffer(m.rigid_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
            for range in m.primary_index_ranges(vp) {
                if range.is_empty() { continue; }
                if shadow { work.shadow_mesh_draws += 1; } else { work.scene_mesh_draws += 1; }
                pass.draw_indexed(range, 0, 0..1);
            }
        }
    }

    fn depth_pass(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder, label: &str, view: &wgpu::TextureView, index: usize, work: &mut RenderWorkStats) {
        let binds = self.binds.as_ref().expect("raymarch binds prepared");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label), color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None,
            }),
            timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render(label)), occlusion_query_set: None,
        });
        pass.set_bind_group(0, &binds.depth, &[]);
        work.shadow_passes += 1;
        self.draw_scene(r, &mut pass, self.shadow_views[index], index as f32, true, &self.depth_pipe, &self.depth_inst_pipe, work);
    }

    /// The whole frame into `r.ldr_view`, replacing `Renderer::render_internal`.
    pub(super) fn encode(&self, r: &Renderer, encoder: &mut wgpu::CommandEncoder, finish_ldr: bool) {
        let binds = self.binds.as_ref().expect("raymarch binds prepared");
        let mut work = RenderWorkStats { scene_renders: 1, ..RenderWorkStats::default() };
        // 1. Visibility maps from the scene geometry.
        self.depth_pass(r, encoder, "rm-shadow-far", &self.shadow_far.view, 0, &mut work);
        self.depth_pass(r, encoder, "rm-shadow-near", &self.shadow_near.view, 1, &mut work);
        self.depth_pass(r, encoder, "rm-sky-cover", &self.sky_cover.view, 2, &mut work);
        for slot in 0..POINT_SHADOWS {
            if self.shadowed[slot].is_none() { continue; }
            for face in 0..6 {
                let layer = slot * 6 + face;
                self.depth_pass(r, encoder, "rm-point-face", &self.point_face_views[layer], 3 + layer, &mut work);
            }
        }
        // 2. Primary visibility.
        {
            let clear = |view| Some(attachment(view, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-gbuffer"),
                color_attachments: &[clear(&self.g0.view), clear(&self.g1.view), clear(&self.g2.view)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None,
                }),
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-gbuffer")), occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.gbuffer_bind, &[]);
            self.draw_scene(r, &mut pass, r.camera.view_proj(), 0.0, false, &self.gbuffer_pipe, &self.gbuffer_inst_pipe, &mut work);
        }
        // 3. Physical sky at primary misses (the shared atmosphere solver, depth-gated).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-sky"),
                color_attachments: &[Some(attachment(&self.sky.view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &r.depth_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None,
                }),
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-sky")), occlusion_query_set: None,
            });
            pass.set_pipeline(&r.sky_depth_pipe);
            pass.set_bind_group(0, &r.camera_bind, &[]);
            pass.set_bind_group(1, &r.physical_environment_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // 4. Froxel media: light every cell, then integrate each column front to back.
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-volume"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-volume")),
            });
            cpass.set_pipeline(&self.volume_pipe);
            cpass.set_bind_group(0, &binds.volume[self.flip], &[]);
            cpass.set_bind_group(1, &binds.visibility, &[]);
            cpass.dispatch_workgroups(self.vol[0].div_ceil(4), self.vol[1].div_ceil(4), self.vol[2].div_ceil(4));
            work.compute_passes += 1;
        }
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rm-integrate"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("rm-integrate")),
            });
            cpass.set_pipeline(&self.integrate_pipe);
            cpass.set_bind_group(0, &binds.integrate[self.flip], &[]);
            cpass.dispatch_workgroups(self.vol[0].div_ceil(8), self.vol[1].div_ceil(8), 1);
            work.compute_passes += 1;
        }
        // 5. Deferred lighting and fog composite into the HDR frame.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-shade"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-shade")), occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shade_pipe);
            pass.set_bind_group(0, &binds.shade[self.flip], &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // 6. One-pixel snow and dust, then ballistic emitters, in HDR before bloom and crush.
        let particles = r.pixel_particle_count.get();
        if particles > 0 || !self.weapon_draws.is_empty() || self.tracer_range.1 > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rm-points"),
                color_attachments: &[Some(attachment(&r.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("rm-points")), occlusion_query_set: None,
            });
            pass.set_bind_group(0, &binds.points, &[]);
            pass.set_bind_group(1, &binds.visibility, &[]);
            if particles > 0 {
                pass.set_pipeline(&self.points_pipe);
                pass.set_vertex_buffer(0, r.pixel_particle_vertices.slice(..));
                pass.draw(0..6, 0..particles);
                work.snow_passes += 1;
            }
            if !self.weapon_draws.is_empty() {
                pass.set_pipeline(&self.sparks_pipe);
                pass.set_vertex_buffer(0, self.weapon_buf.slice(..));
                for &(first, count) in &self.weapon_draws {
                    pass.draw(0..count * 6, first..first + 1);
                    work.scene_instanced_draws += 1;
                }
            }
            if self.tracer_range.1 > 0 {
                pass.set_pipeline(&self.tracers_pipe);
                pass.set_vertex_buffer(0, self.weapon_buf.slice(..));
                pass.draw(0..6, self.tracer_range.0..self.tracer_range.0 + self.tracer_range.1);
                work.scene_instanced_draws += 1;
            }
        }
        // 7. The shared display stage: the crewman's eye, bloom, crush.
        if finish_ldr && r.autoexp && r.exposure_override.is_none() {
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("exposure"), timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.compute("exposure")),
                });
                cpass.set_pipeline(&r.exp_pipe);
                cpass.set_bind_group(0, &r.exp_binds[r.active_view], &[]);
                work.compute_passes += 1;
                cpass.dispatch_workgroups(1, 1, 1);
            }
            let eye = &r.exp_bufs[r.active_view];
            encoder.copy_buffer_to_buffer(eye, 0, &r.crush_buf, 44, 4);
            encoder.copy_buffer_to_buffer(eye, 8, &r.crush_buf, 16, 8);
        }
        if finish_ldr {
            work.bloom_passes += 4;
            work.fullscreen_raster_passes += 4;
            let btexel = [1.0 / r.bw as f32, 1.0 / r.bh as f32];
            r.bloom_pass(encoder, "bloom-bright", &r.bloom_bright_pipe, &r.bloom_bind_hdr, &r.bloom_a_view, btexel, [0.0, 0.0], false);
            r.bloom_pass(encoder, "bloom-blur-h", &r.bloom_blur_pipe, &r.bloom_bind_a, &r.bloom_b_view, btexel, [1.0, 0.0], false);
            r.bloom_pass(encoder, "bloom-blur-v", &r.bloom_blur_pipe, &r.bloom_bind_b, &r.bloom_a_view, btexel, [0.0, 1.0], false);
            r.bloom_pass(encoder, "bloom-composite", &r.bloom_composite_pipe, &r.bloom_bind_a, &r.hdr_view, btexel, [0.0, 0.0], true);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(attachment(&r.ldr_view, wgpu::LoadOp::Clear(wgpu::Color::BLACK)))],
                depth_stencil_attachment: None,
                timestamp_writes: r.gpu_profile.as_ref().and_then(|p| p.render("crush")), occlusion_query_set: None,
            });
            pass.set_pipeline(&r.crush_pipe);
            pass.set_bind_group(0, &r.crush_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        let mut total = r.render_work.get();
        total.accumulate(work);
        r.render_work.set(total);
    }
}

impl Renderer {
    /// The same atmosphere the shared uniforms upload (`upload_uniforms`), for CPU source sampling.
    fn raymarch_atmosphere(&self) -> AtmosphereParameters {
        let mut atmosphere = AtmosphereParameters::default();
        let campaign = self.current_campaign_environment();
        atmosphere.mie_scale_height_m = campaign.mie_scale_height_m;
        atmosphere.mie_scattering_m_inv = campaign.mie_scattering_m_inv;
        atmosphere.mie_extinction_m_inv = campaign.mie_extinction_m_inv;
        atmosphere.mie_asymmetry = campaign.mie_asymmetry;
        atmosphere.cloud.bottom_altitude_m = campaign.cloud_bottom_m;
        atmosphere.cloud.top_altitude_m = campaign.cloud_top_m;
        atmosphere.cloud.liquid_water_content_kg_m3 = campaign.cloud_lwc_kg_m3;
        atmosphere.cloud.droplet_effective_radius_m = campaign.cloud_droplet_radius_m;
        atmosphere.cloud.coverage = self.overcast.clamp(0.0, 1.0) as f64;
        atmosphere.cloud_offset_m = self.weather_offset.as_dvec3();
        atmosphere.cloud_evolution_s = self.time as f64;
        self.celestial.apply_to(&mut atmosphere);
        atmosphere.sun_direction = self.sun_dir.as_dvec3().normalize_or_zero();
        atmosphere
    }

    pub fn raymarch_stats(&self) -> Option<Stats> { self.raymarch.as_ref().map(|rm| rm.stats) }
}
