//! texelsplat.rs — an ISOLATED texel-splatting prototype (`cargo run -- texel <glb> [out_prefix]`).
//!
//! This is a project-within-the-project: it reuses only the stable substrate (the GLB loader, the
//! device setup, PNG readback) and builds the entire render path fresh. It touches NOTHING in
//! renderer.rs / game.rs — the shipped crush pipeline is not called here. The goal is a beauty-shot
//! A/B against the crush render to decide whether texel splatting becomes Kessel's renderer.
//!
//! Pipeline (see docs/TEXEL_SPLATTING.md, faithful-but-simplified port of dylanebert/texel-splatting):
//!   1. CAPTURE  — rasterize the scene into a probe cubemap (radial / normal / albedo / id) from a
//!                 GRID-SNAPPED world origin near the camera. Grid-snapping is what makes the pixels
//!                 world-stable within a cell and re-resolves finer detail as the camera approaches.
//!   2. SHADE    — one compute invocation per cube texel: sun diffuse + cold ambient, OKLab
//!                 posterize, id/normal outlines. Camera-independent → perspective-stable.
//!   3. SPLAT    — draw every non-empty texel as a world quad; depth resolves overlap.
//!
//! Deliberately NOT ported yet (milestone 2+): ray-traced shadows/GI, the 3-probe eye/grid/prev
//! crossfade for fast motion, indirect/culled splat lists. Single grid probe, brute-force splat.

use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use crate::gltf_load::{self, CpuMesh, Vertex};

const PROBE_SIZE: u32 = 256; // must match the shaders' PROBE_SIZE
const FACE_TEXELS: u32 = PROBE_SIZE * PROBE_SIZE;
const NEAR: f32 = 0.1;
const FAR: f32 = 200.0;
const GRID: f32 = 2.0; // probe origin snaps to this world grid (metres)

const WORLD_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float; // xyz world pos, w valid
const NORMAL_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ALBEDO_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const LIT_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const OUT_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

// Cube face view targets — must pair with the shaders' texel_dir(). Verbatim from the reference.
const FACE_DIR: [[f32; 3]; 6] = [
    [1., 0., 0.],
    [-1., 0., 0.],
    [0., 1., 0.],
    [0., -1., 0.],
    [0., 0., 1.],
    [0., 0., -1.],
];
const FACE_UP: [[f32; 3]; 6] = [
    [0., -1., 0.],
    [0., -1., 0.],
    [0., 0., 1.],
    [0., 0., -1.],
    [0., -1., 0.],
    [0., -1., 0.],
];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceU {
    view_proj: [[f32; 4]; 4],
    origin: [f32; 3],
    near: f32,
    far: f32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadeU {
    sun_dir: [f32; 3],
    _p0: f32,
    sun_col: [f32; 3],
    _p1: f32,
    ambient: [f32; 3],
    _p2: f32,
    sky_col: [f32; 3],
    _p3: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SplatU {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    near: f32,
    origin: [f32; 3],
    far: f32,
}

struct MeshGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_idx: u32,
    id: u32,
}

/// A flat ground quad centred at origin so the object casts onto something and doesn't float.
fn ground_mesh(half: f32, y: f32, color: [u8; 3]) -> CpuMesh {
    let n = [0.0, 1.0, 0.0];
    let v = |x: f32, z: f32| Vertex {
        pos: [x, y, z],
        normal: n,
        uv: [(x / half) * 0.5 + 0.5, (z / half) * 0.5 + 0.5],
        tangent: [1.0, 0.0, 0.0, 1.0],
    };
    let verts = vec![
        v(-half, -half),
        v(half, -half),
        v(half, half),
        v(-half, half),
    ];
    let idx = vec![0u32, 2, 1, 0, 3, 2];
    CpuMesh::basic(
        verts,
        idx,
        Some(crate::prims::solid_color(color)),
        Vec3::new(-half, y, -half),
        Vec3::new(half, y, half),
    )
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}

/// Face view-projection for cube face `f` from `origin`. glam RH perspective (NDC z∈[0,1], matches wgpu).
fn face_view_proj(origin: Vec3, f: usize) -> Mat4 {
    let dir = Vec3::from(FACE_DIR[f]);
    let up = Vec3::from(FACE_UP[f]);
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, NEAR, FAR);
    let view = Mat4::look_to_rh(origin, dir, up);
    proj * view
}

pub fn run_texel(glb: &str, out_prefix: &str) -> anyhow::Result<()> {
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    println!("[texel] adapter: {}", adapter.get_info().name);
    let (device, queue) = crate::request_device(&adapter);

    // ── scene: the dropped mesh (id 1) + a ground quad (id 2) ──
    let hero = gltf_load::load_glb(glb).map_err(|e| anyhow::anyhow!("load {glb}: {e}"))?;
    println!(
        "[texel] loaded {glb}: {} verts, tex={}",
        hero.vertices.len(),
        hero.texture.is_some()
    );
    let ground = ground_mesh(60.0, hero.min.y, [150, 158, 150]);

    let samp = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("texel-samp"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let mk = |m: &CpuMesh, id: u32| -> MeshGpu {
        MeshGpu {
            vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("v"),
                contents: bytemuck::cast_slice(&m.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            ibuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("i"),
                contents: bytemuck::cast_slice(&m.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            n_idx: m.indices.len() as u32,
            id,
        }
    };
    let lift: f32 = std::env::var("KESSEL_TEXEL_LIFT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4.5);
    let meshes = [mk(&hero, 1), mk(&ground, 2)];

    // ── capture G-buffer array textures (6 layers) + depth ──
    let arr = |fmt: wgpu::TextureFormat, usage: wgpu::TextureUsages, label: &str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: PROBE_SIZE,
                height: PROBE_SIZE,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage,
            view_formats: &[],
        })
    };
    let rt_use = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC;
    let world = arr(WORLD_FMT, rt_use, "world");
    let normal = arr(NORMAL_FMT, rt_use, "normal");
    let albedo = arr(ALBEDO_FMT, rt_use, "albedo");
    let lit = arr(
        LIT_FMT,
        wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        "lit",
    );
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cap-depth"),
        size: wgpu::Extent3d {
            width: PROBE_SIZE,
            height: PROBE_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let layer_view = |t: &wgpu::Texture, f: u32| {
        t.create_view(&wgpu::TextureViewDescriptor {
            base_array_layer: f,
            array_layer_count: Some(1),
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        })
    };
    let full_arr_view = |t: &wgpu::Texture| {
        t.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    };

    // ── capture pipeline ──
    let face_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("face-u"),
        size: std::mem::size_of::<FaceU>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cap_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("cap-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let cap_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("texel-capture"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/texel_capture.wgsl").into()),
    });
    let cap_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("cap-pl"),
        bind_group_layouts: &[&cap_bgl],
        push_constant_ranges: &[wgpu::PushConstantRange {
            stages: wgpu::ShaderStages::FRAGMENT,
            range: 0..4,
        }],
    });
    let color_target = |fmt| {
        Some(wgpu::ColorTargetState {
            format: fmt,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })
    };
    let cap_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("cap-pipe"),
        layout: Some(&cap_layout),
        vertex: wgpu::VertexState {
            module: &cap_shader,
            entry_point: Some("vs"),
            buffers: &[vertex_layout()],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &cap_shader,
            entry_point: Some("fs"),
            targets: &[
                color_target(WORLD_FMT),
                color_target(NORMAL_FMT),
                color_target(ALBEDO_FMT),
            ],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });
    // Per-mesh albedo texture views for the capture bind group.
    let hero_alb = AlbedoTex::new(&device, &queue, &hero.texture, lift);
    let ground_alb = AlbedoTex::new(&device, &queue, &ground.texture, 1.0);
    let mesh_alb = [&hero_alb, &ground_alb];

    // ── shade compute ──
    let shade_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shade-u"),
        size: std::mem::size_of::<ShadeU>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sun = Vec3::new(-0.4, 0.7, 0.3).normalize();
    queue.write_buffer(
        &shade_buf,
        0,
        bytemuck::bytes_of(&ShadeU {
            sun_dir: sun.to_array(),
            _p0: 0.0,
            sun_col: [1.35, 1.22, 1.02],
            _p1: 0.0,
            ambient: [0.42, 0.47, 0.56],
            _p2: 0.0,
            sky_col: [0.55, 0.62, 0.75],
            _p3: 0.0,
        }),
    );
    let shade_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shade-bgl"),
        entries: &[
            tex_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
            tex_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
            tex_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: LIT_FMT,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let shade_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("texel-shade"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/texel_shade.wgsl").into()),
    });
    let shade_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("shade-pipe"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shade-pl"),
                bind_group_layouts: &[&shade_bgl],
                push_constant_ranges: &[],
            }),
        ),
        module: &shade_shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let shade_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shade-bind"),
        layout: &shade_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&world)),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&normal)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&albedo)),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&lit)),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: shade_buf.as_entire_binding(),
            },
        ],
    });

    // ── splat pipeline ──
    let splat_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("splat-u"),
        size: std::mem::size_of::<SplatU>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let splat_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("splat-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            tex_entry_vf(1, wgpu::TextureSampleType::Float { filterable: false }),
            tex_entry_vf(2, wgpu::TextureSampleType::Float { filterable: false }),
        ],
    });
    let splat_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("texel-splat"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/texel_splat.wgsl").into()),
    });
    let (ow, oh) = (1280u32, 720u32);
    let splat_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("splat-pipe"),
        layout: Some(
            &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("splat-pl"),
                bind_group_layouts: &[&splat_bgl],
                push_constant_ranges: &[],
            }),
        ),
        vertex: wgpu::VertexState {
            module: &splat_shader,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &splat_shader,
            entry_point: Some("fs"),
            targets: &[color_target(OUT_FMT)],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });
    let splat_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("splat-bind"),
        layout: &splat_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: splat_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&lit)),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&full_arr_view(&world)),
            },
        ],
    });

    // output target + depth
    let out_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("out"),
        size: wgpu::Extent3d {
            width: ow,
            height: oh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OUT_FMT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let out_view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let out_depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("out-depth"),
        size: wgpu::Extent3d {
            width: ow,
            height: oh,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let out_depth_view = out_depth.create_view(&wgpu::TextureViewDescriptor::default());

    // ── camera dolly: from FAR to NEAR on a 3/4 view, re-capturing each frame (grid-snapped
    // probe → progressive detail on approach) ──
    let center = (hero.min + hero.max) * 0.5;
    let dists = [22.0f32, 14.0, 8.0, 4.5];
    for (fi, &d) in dists.iter().enumerate() {
        let cam_pos = center + Vec3::new(0.8, 0.55, 1.0).normalize() * d;
        let probe_origin = Vec3::new(
            (cam_pos.x / GRID).round() * GRID,
            (cam_pos.y / GRID).round() * GRID,
            (cam_pos.z / GRID).round() * GRID,
        );

        // CAPTURE 6 faces — one submit per face so the per-face uniform write is observed (a single
        // encoder would see only the last write_buffer before submit).
        for f in 0..6u32 {
            let fu = FaceU {
                view_proj: face_view_proj(probe_origin, f as usize).to_cols_array_2d(),
                origin: probe_origin.to_array(),
                near: NEAR,
                far: FAR,
                _pad: [0.0; 3],
            };
            queue.write_buffer(&face_buf, 0, bytemuck::bytes_of(&fu));
            let wv = layer_view(&world, f);
            let nv = layer_view(&normal, f);
            let av = layer_view(&albedo, f);
            let op = |r, g, b, a| wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                store: wgpu::StoreOp::Store,
            };
            let color_atts = [
                Some(wgpu::RenderPassColorAttachment {
                    view: &wv,
                    resolve_target: None,
                    depth_slice: None,
                    ops: op(0.0, 0.0, 0.0, 0.0),
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &nv,
                    resolve_target: None,
                    depth_slice: None,
                    ops: op(0.5, 0.5, 1.0, 0.0),
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: &av,
                    resolve_target: None,
                    depth_slice: None,
                    ops: op(0.0, 0.0, 0.0, 1.0),
                }),
            ];
            let mut fenc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("face"),
            });
            {
                let mut pass = fenc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cap-face"),
                    color_attachments: &color_atts,
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&cap_pipe);
                for (mi, m) in meshes.iter().enumerate() {
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("cap-bind"),
                        layout: &cap_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: face_buf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&mesh_alb[mi].view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&samp),
                            },
                        ],
                    });
                    pass.set_bind_group(0, &bind, &[]);
                    pass.set_push_constants(
                        wgpu::ShaderStages::FRAGMENT,
                        0,
                        bytemuck::bytes_of(&m.id),
                    );
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.n_idx, 0, 0..1);
                }
            }
            queue.submit(Some(fenc.finish()));
        }

        // DEBUG: dump the captured cube faces (albedo + radial) as horizontal strips so we can see
        // whether the mesh landed in the cubemap at all.
        if std::env::var("KESSEL_TEXEL_DEBUG").is_ok() && fi == 1 {
            dump_array_strip(
                &device,
                &queue,
                &albedo,
                ALBEDO_FMT,
                &format!("{out_prefix}_dbg_albedo.png"),
            )?;
            dump_array_strip(
                &device,
                &queue,
                &normal,
                NORMAL_FMT,
                &format!("{out_prefix}_dbg_normal.png"),
            )?;
        }

        // SHADE
        let mut senc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("shade"),
        });
        {
            let mut pass = senc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("shade"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&shade_pipe);
            pass.set_bind_group(0, &shade_bind, &[]);
            let wg = PROBE_SIZE.div_ceil(8);
            pass.dispatch_workgroups(wg, wg, 6);
        }
        queue.submit(Some(senc.finish()));

        // SPLAT
        let cam_center = center + Vec3::Y * (hero.max.y - hero.min.y) * 0.35;
        let view = Mat4::look_at_rh(cam_pos, cam_center, Vec3::Y);
        let proj = Mat4::perspective_rh(50f32.to_radians(), ow as f32 / oh as f32, 0.05, 500.0);
        queue.write_buffer(
            &splat_buf,
            0,
            bytemuck::bytes_of(&SplatU {
                view_proj: (proj * view).to_cols_array_2d(),
                cam_pos: cam_pos.to_array(),
                near: NEAR,
                origin: probe_origin.to_array(),
                far: FAR,
            }),
        );
        let mut penc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("splat"),
        });
        {
            let mut pass = penc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("splat"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.55,
                            g: 0.62,
                            b: 0.75,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &out_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&splat_pipe);
            pass.set_bind_group(0, &splat_bind, &[]);
            pass.draw(0..6, 0..6 * FACE_TEXELS);
        }
        queue.submit(Some(penc.finish()));

        // READBACK → PNG
        let path = format!("{out_prefix}_{fi}.png");
        save_png(&device, &queue, &out_tex, ow, oh, &path)?;
        println!(
            "[texel] frame {fi}: dist {d:.1} m, probe grid {:?} → {path}",
            probe_origin.to_array()
        );
    }
    Ok(())
}

// ── helpers ──
struct AlbedoTex {
    view: wgpu::TextureView,
}
impl AlbedoTex {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &Option<gltf_load::CpuImage>,
        lift: f32,
    ) -> Self {
        let (w, h, mut rgba) = match img {
            Some(im) => (im.width, im.height, im.rgba.clone()),
            None => (1, 1, vec![180, 180, 180, 255]),
        };
        if lift != 1.0 {
            for px in rgba.chunks_exact_mut(4) {
                for c in &mut px[..3] {
                    *c = ((*c as f32) * lift).min(255.0) as u8;
                }
            }
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("alb"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w.max(1) * 4),
                rows_per_image: Some(h.max(1)),
            },
            wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
        );
        Self {
            view: tex.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}
fn tex_entry(binding: u32, st: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: st,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}
fn tex_entry_vf(binding: u32, st: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: st,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    }
}

/// DEBUG: read a 6-layer array texture back and save its faces as a horizontal strip PNG. Handles
/// Rgba8Unorm (bytes) and Rgba16Float (half → the .r channel greyscaled for radial).
fn dump_array_strip(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    fmt: wgpu::TextureFormat,
    path: &str,
) -> anyhow::Result<()> {
    let sz = PROBE_SIZE;
    let bpp: u32 = match fmt {
        wgpu::TextureFormat::Rgba16Float => 8,
        _ => 4,
    };
    let unpadded = sz * bpp;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dbg"),
        size: (padded * sz * 6) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dbgcopy"),
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(sz),
            },
        },
        wgpu::Extent3d {
            width: sz,
            height: sz,
            depth_or_array_layers: 6,
        },
    );
    queue.submit(Some(enc.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let data = slice.get_mapped_range();
    let mut img = image::RgbaImage::new(sz * 6, sz);
    let half = |b: &[u8]| -> f32 {
        let h = u16::from_le_bytes([b[0], b[1]]);
        let (s, e, m) = ((h >> 15) & 1, ((h >> 10) & 0x1f) as i32, (h & 0x3ff) as u32);
        let sign = if s == 1 { -1.0 } else { 1.0 };
        if e == 0 {
            sign * (m as f32) * 2f32.powi(-24)
        } else if e == 31 {
            sign
        } else {
            sign * (1.0 + m as f32 / 1024.0) * 2f32.powi(e - 15)
        }
    };
    for face in 0..6u32 {
        for y in 0..sz {
            let row = ((face * sz + y) * padded) as usize;
            for x in 0..sz {
                let o = row + (x * bpp) as usize;
                let px = if bpp == 8 {
                    let v = (half(&data[o..o + 2]) * 255.0).clamp(0.0, 255.0) as u8;
                    [v, v, v, 255]
                } else {
                    [data[o], data[o + 1], data[o + 2], 255]
                };
                img.put_pixel(face * sz + x, y, image::Rgba(px));
            }
        }
    }
    drop(data);
    buf.unmap();
    img.save(path)?;
    println!("[texel] dbg → {path}");
    Ok(())
}

fn save_png(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
    path: &str,
) -> anyhow::Result<()> {
    let unpadded = w * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = unpadded.div_ceil(align) * align;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("copy"),
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(enc.finish()));
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let data = slice.get_mapped_range();
    let mut px = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let s = (row * padded) as usize;
        px.extend_from_slice(&data[s..s + unpadded as usize]);
    }
    drop(data);
    buf.unmap();
    image::RgbaImage::from_raw(w, h, px)
        .ok_or_else(|| anyhow::anyhow!("bad size"))?
        .save(path)?;
    Ok(())
}
