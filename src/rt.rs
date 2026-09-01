//! M-D hardware ray tracing (wgpu 27 `EXPERIMENTAL_RAY_QUERY`).
//!
//! S0 lives here: `cargo run -- rtprobe out.png` — the hello-world spike that validates the
//! whole experimental stack (feature token → naga WGSL ray-query compile → driver AS build →
//! traversal → readback) before any renderer surgery. S1 grows this module into the scene
//! BLAS/TLAS owner behind the one-trace-interface rule (plan: ok-this-is-the-ticklish-sphinx).

use anyhow::bail;
use wgpu::util::DeviceExt;

/// The identity 3×4 row-major instance transform (rows 0..2 of a 4×4).
pub const IDENTITY_3X4: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

/// glam Mat4 (column-major) → the 3×4 row-major affine a `TlasInstance` wants (rows 0..2;
/// translation lands in each row's 4th element).
pub fn rows_3x4(m: glam::Mat4) -> [f32; 12] {
    let (r0, r1, r2) = (m.row(0), m.row(1), m.row(2));
    [
        r0.x, r0.y, r0.z, r0.w, r1.x, r1.y, r1.z, r1.w, r2.x, r2.y, r2.z, r2.w,
    ]
}

/// TLAS cull-mask bits (per-view ray masks pick what a ray can see — no TLAS membership churn).
pub const MASK_WORLD: u8 = 0x01; // terrain + static dressing + destructibles
/// Closed direct-view protective glass. Keeping it on a distinct bit lets the High transport
/// path find the second interface of a solid vision block without accepting an intervening opaque
/// triangle as the exit surface. Ordinary camera/lighting rays still use `0xff` and therefore see
/// the glass unless they are deliberately tracing the post-interface segment.
pub const MASK_GLASS: u8 = 0x02;
pub const MASK_PROTO: u8 = 0x10; // instanced prototypes (trees, worldgen modules)

/// One mesh's BLAS geometry description, kept so the build entry can be re-issued (BLASes are
/// built once at load — rigid world — but destruction-spawned geometry arrives later).
pub struct BlasSet {
    pub blas: wgpu::Blas,
    pub size: wgpu::BlasTriangleGeometrySizeDescriptor,
}

/// Create (but do not build) a BLAS for an indexed triangle mesh whose buffers carry
/// `BufferUsages::BLAS_INPUT`. Positions are read straight out of the interleaved 48-byte
/// vertex layout (Float32x3 at offset 0 — verified legal: stride ≥ 12, multiple of 4).
pub fn make_blas(device: &wgpu::Device, vertex_count: u32, index_count: u32) -> BlasSet {
    let size = wgpu::BlasTriangleGeometrySizeDescriptor {
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count,
        index_format: Some(wgpu::IndexFormat::Uint32),
        index_count: Some(index_count),
        // OPAQUE everywhere: naga ray queries have no candidate handling; non-opaque = no hits.
        flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
    };
    let blas = device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("scene-blas"),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: vec![size.clone()],
        },
    );
    BlasSet { blas, size }
}

/// The build entry for a [`BlasSet`] over its source buffers (borrow lives for the encode call).
pub fn blas_entry<'a>(
    set: &'a BlasSet,
    vbuf: &'a wgpu::Buffer,
    ibuf: &'a wgpu::Buffer,
) -> wgpu::BlasBuildEntry<'a> {
    wgpu::BlasBuildEntry {
        blas: &set.blas,
        geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
            size: &set.size,
            vertex_buffer: vbuf,
            first_vertex: 0,
            vertex_stride: std::mem::size_of::<crate::gltf_load::Vertex>() as u64,
            index_buffer: Some(ibuf),
            first_index: Some(0),
            transform_buffer: None,
            transform_buffer_offset: None,
        }]),
    }
}

const PROBE_DIM: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RtViewUniform {
    inv_vp: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    cam_fwd: [f32; 4], // xyz forward, w = zfar
}

/// S1 debug trace: primary rays against the (already built) scene TLAS from the given camera.
/// Writes `{prefix}_rt.png` (instance-id false color) and returns the traced view-space depth
/// per pixel (1e9 = miss) for the correlation assert.
pub fn bvhview(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tlas: &wgpu::Tlas,
    cam: &crate::camera::Camera,
    w: u32,
    h: u32,
    prefix: &str,
) -> anyhow::Result<Vec<f32>> {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rtview"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/rtview.wgsl").into()),
    });
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rtview-color"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let zbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtview-z"),
        size: (w * h * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rtview-u"),
        contents: bytemuck::bytes_of(&RtViewUniform {
            inv_vp: cam.view_proj().inverse().to_cols_array_2d(),
            cam_pos: [cam.pos.x, cam.pos.y, cam.pos.z, 0.0],
            cam_fwd: {
                let f = cam.forward();
                [f.x, f.y, f.z, cam.zfar]
            },
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rtview-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::AccelerationStructure {
                    vertex_return: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let view = color.create_view(&Default::default());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rtview-bind"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: tlas.as_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: ubuf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: zbuf.as_entire_binding(),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rtview-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("rtview-pipe"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs"),
        compilation_options: Default::default(),
        cache: None,
    });

    let row = w * 4; // rgba8: 640*4 = 2560, already 256-aligned
    let img_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtview-img-staging"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let z_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtview-z-staging"),
        size: (w * h * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rtview"),
    });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rtview"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &img_staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    enc.copy_buffer_to_buffer(&zbuf, 0, &z_staging, 0, (w * h * 4) as u64);
    queue.submit([enc.finish()]);

    img_staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    z_staging.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let img = img_staging.slice(..).get_mapped_range().to_vec();
    let z: Vec<f32> =
        bytemuck::cast_slice::<u8, f32>(&z_staging.slice(..).get_mapped_range()).to_vec();
    img_staging.unmap();
    z_staging.unmap();

    image::RgbaImage::from_raw(w, h, img)
        .expect("rtview image")
        .save(format!("{prefix}_rt.png"))?;
    Ok(z)
}

/// S0 spike. Builds a 1-triangle BLAS + 1-instance TLAS, traces one ray per pixel in a
/// compute pass, reads the image back, asserts center-hit/corner-miss, writes the PNG.
/// Separately spikes `getCommittedHitVertexPositions` (vertex return) behind error scopes —
/// reported, but not part of the PASS gate (the SSBO fetch path is the documented fallback).
pub fn probe(device: &wgpu::Device, queue: &wgpu::Queue, out_path: &str) -> anyhow::Result<()> {
    if !device
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
    {
        bail!("[rtprobe] FAIL: no EXPERIMENTAL_RAY_QUERY on this device — the hero lit path needs an RT-class GPU (ENGINE.md §7)");
    }
    let has_vr = device
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_HIT_VERTEX_RETURN);

    // One triangle in the z=0 plane: covers the film centre, misses the corners.
    let tri: [f32; 9] = [-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0];
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rtprobe-tri"),
        contents: bytemuck::cast_slice(&tri),
        usage: wgpu::BufferUsages::BLAS_INPUT,
    });

    let size = wgpu::BlasTriangleGeometrySizeDescriptor {
        vertex_format: wgpu::VertexFormat::Float32x3,
        vertex_count: 3,
        index_format: None,
        index_count: None,
        // OPAQUE is mandatory for us everywhere: naga's ray-query path has no candidate
        // handling, non-opaque geometry simply never reports hits.
        flags: wgpu::AccelerationStructureGeometryFlags::OPAQUE,
    };
    let mut as_flags = wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE;
    if has_vr {
        as_flags |= wgpu::AccelerationStructureFlags::ALLOW_RAY_HIT_VERTEX_RETURN;
    }
    let blas = device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("rtprobe-blas"),
            flags: as_flags,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: vec![size.clone()],
        },
    );
    let mut tlas = device.create_tlas(&wgpu::CreateTlasDescriptor {
        label: Some("rtprobe-tlas"),
        max_instances: 1,
        flags: as_flags,
        update_mode: wgpu::AccelerationStructureUpdateMode::Build,
    });
    tlas[0] = Some(wgpu::TlasInstance::new(&blas, IDENTITY_3X4, 0, 0xff));

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rtprobe-build"),
    });
    enc.build_acceleration_structures(
        std::iter::once(&wgpu::BlasBuildEntry {
            blas: &blas,
            geometry: wgpu::BlasGeometries::TriangleGeometries(vec![wgpu::BlasTriangleGeometry {
                size: &size,
                vertex_buffer: &vbuf,
                first_vertex: 0,
                vertex_stride: 12,
                index_buffer: None,
                first_index: None,
                transform_buffer: None,
                transform_buffer_offset: None,
            }]),
        }),
        std::iter::once(&tlas),
    );
    queue.submit([enc.finish()]);

    // ── trace pass ────────────────────────────────────────────────────────────────────────
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rtprobe"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/rtprobe.wgsl").into()),
    });
    let out_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rtprobe-out"),
        size: wgpu::Extent3d {
            width: PROBE_DIM,
            height: PROBE_DIM,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rtprobe-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::AccelerationStructure {
                    vertex_return: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    });
    let view = out_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rtprobe-bind"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: tlas.as_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rtprobe-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("rtprobe-pipe"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rtprobe-trace"),
    });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rtprobe"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(PROBE_DIM / 8, PROBE_DIM / 8, 1);
    }
    // readback (64×4 = 256 B/row — already COPY_BYTES_PER_ROW_ALIGNMENT aligned)
    let row = PROBE_DIM * 4;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtprobe-staging"),
        size: (row * PROBE_DIM) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &out_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(PROBE_DIM),
            },
        },
        wgpu::Extent3d {
            width: PROBE_DIM,
            height: PROBE_DIM,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([enc.finish()]);

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();

    let px = |x: u32, y: u32| -> [u8; 4] {
        let o = ((y * PROBE_DIM + x) * 4) as usize;
        [data[o], data[o + 1], data[o + 2], data[o + 3]]
    };
    let center = px(PROBE_DIM / 2, PROBE_DIM / 2);
    let corner = px(2, 2);
    println!(
        "[rtprobe] center px = {:?} (want R=255 hit), corner px = {:?} (want R=0 miss)",
        center, corner
    );

    image::RgbaImage::from_raw(PROBE_DIM, PROBE_DIM, data)
        .expect("probe image")
        .save(out_path)?;

    let core_pass = center[0] == 255 && corner[0] == 0;
    let vr_pass = if has_vr {
        probe_vertex_return(device, queue, &tlas, &tri)?
    } else {
        None
    };
    match vr_pass {
        Some(true) => println!("[rtprobe] vertex_return: triangle corners match — PASS"),
        Some(false) => {
            println!("[rtprobe] vertex_return: FAIL (SSBO-fetch fallback stands, see plan)")
        }
        None => println!("[rtprobe] vertex_return: feature absent — SKIP"),
    }
    if core_pass {
        println!("[rtprobe] PASS → {out_path}");
        Ok(())
    } else {
        bail!("[rtprobe] FAIL: ray query returned wrong hit pattern → {out_path}")
    }
}

/// The vertex-return sub-spike: one hardcoded center ray, `getCommittedHitVertexPositions`
/// written to a storage buffer, compared against the authored triangle. Created behind
/// validation error scopes so a naga/driver gap degrades to a report, not a crash.
fn probe_vertex_return(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tlas: &wgpu::Tlas,
    tri: &[f32; 9],
) -> anyhow::Result<Option<bool>> {
    const SRC: &str = r#"
@group(0) @binding(0) var tlas: acceleration_structure<vertex_return>;
@group(0) @binding(1) var<storage, read_write> out_pos: array<vec4<f32>, 3>;

@compute @workgroup_size(1)
fn cs() {
    var rq: ray_query<vertex_return>;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_FORCE_OPAQUE, 0xFFu, 0.01, 100.0, vec3<f32>(0.0, 0.0, -2.0), vec3<f32>(0.0, 0.0, 1.0)));
    while (rayQueryProceed(&rq)) { }
    let hit = rayQueryGetCommittedIntersection(&rq);
    if (hit.kind == RAY_QUERY_INTERSECTION_TRIANGLE) {
        let vs = getCommittedHitVertexPositions(&rq);
        out_pos[0] = vec4<f32>(vs[0], 1.0);
        out_pos[1] = vec4<f32>(vs[1], 1.0);
        out_pos[2] = vec4<f32>(vs[2], 1.0);
    }
}
"#;
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("rtprobe-vr"),
        source: wgpu::ShaderSource::Wgsl(SRC.into()),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rtprobe-vr-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::AccelerationStructure {
                    vertex_return: true,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtprobe-vr-out"),
        size: 48,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("rtprobe-vr-bind"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: tlas.as_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rtprobe-vr-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("rtprobe-vr-pipe"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("cs"),
        compilation_options: Default::default(),
        cache: None,
    });
    if let Some(e) = pollster::block_on(device.pop_error_scope()) {
        println!("[rtprobe] vertex_return validation error: {e}");
        return Ok(Some(false));
    }

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rtprobe-vr-staging"),
        size: 48,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("rtprobe-vr"),
    });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rtprobe-vr"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipe);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    enc.copy_buffer_to_buffer(&out_buf, 0, &staging, 0, 48);
    queue.submit([enc.finish()]);
    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let got: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()).to_vec();
    staging.unmap();

    // vec4-strided [x y z 1] triplet vs the authored flat triangle, any winding-preserving order.
    let mut ok = true;
    for v in 0..3 {
        for c in 0..3 {
            if (got[v * 4 + c] - tri[v * 3 + c]).abs() > 1e-5 {
                ok = false;
            }
        }
    }
    if !ok {
        println!("[rtprobe] vertex_return got {:?}", &got[..12]);
    }
    Ok(Some(ok))
}
