//! Exact triangle paging for large terrain shadow casters. Reordering an independent index
//! buffer changes submission only: original vertices, winding, full-detail triangles, primary
//! raster indices and ray-tracing primitive identities remain untouched.
use super::{aabb_in_frustum, frustum_planes, shadow_has_sample, CpuMesh};
use glam::{Mat4, Vec3, Vec4};
use std::ops::Range;

const AXIS: usize = 32;
const CELLS: usize = AXIS * AXIS;
const MIN_TRIANGLES: usize = 32_768;

#[derive(Clone, Debug)]
struct Bounds { min: Vec3, max: Vec3 }
impl Bounds {
    fn empty() -> Self { Self { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) } }
    fn include(&mut self, point: Vec3) { self.min = self.min.min(point); self.max = self.max.max(point); }
    fn union(&mut self, other: &Self) { self.min = self.min.min(other.min); self.max = self.max.max(other.max); }
}

#[derive(Clone, Debug)]
struct Node {
    bounds: Bounds,
    indices: Range<u32>,
    children: [Option<usize>; 4],
}

#[derive(Clone, Debug)]
pub(super) struct Pages { nodes: Vec<Node> }

pub(super) struct Prepared {
    pub indices: Vec<u32>,
    pub pages: Pages,
}

fn morton(x: usize, z: usize) -> usize {
    let mut cell = 0;
    for bit in 0..5 { cell |= ((x >> bit) & 1) << (2 * bit); cell |= ((z >> bit) & 1) << (2 * bit + 1); }
    cell
}

/// Only large, spatially broad immutable geometry merits an additional index upload. Runtime
/// selection additionally requires the terrain/track-receiver identity; ordinary models never
/// switch their index stream merely because they happen to be large.
pub(super) fn prepare(mesh: &CpuMesh) -> Option<Prepared> {
    let span = mesh.max - mesh.min;
    if mesh.indices.len() / 3 < MIN_TRIANGLES || mesh.vertices.len() <= 4096
        || span.x < 64.0 || span.z < 64.0 { return None; }
    build(mesh)
}

fn build(mesh: &CpuMesh) -> Option<Prepared> {
    let span = mesh.max - mesh.min;
    if mesh.indices.is_empty() || mesh.indices.len() % 3 != 0 || mesh.indices.len() > u32::MAX as usize
        || !mesh.min.is_finite() || !mesh.max.is_finite() || !span.is_finite()
        || span.x <= 0.0 || span.z <= 0.0 { return None; }
    let mut bounds: [Bounds; CELLS] = std::array::from_fn(|_| Bounds::empty());
    let mut counts = [0_u32; CELLS];
    let mut assignments = Vec::with_capacity(mesh.indices.len() / 3);
    for triangle in mesh.indices.chunks_exact(3) {
        let mut points = [Vec3::ZERO; 3];
        for (point, &index) in points.iter_mut().zip(triangle) {
            *point = Vec3::from_array(mesh.vertices.get(index as usize)?.pos);
            if !point.is_finite() { return None; }
        }
        let center = (points[0] + points[1] + points[2]) / 3.0;
        if !center.is_finite() { return None; }
        let relative = (center - mesh.min) / span;
        let x = (relative.x * AXIS as f32).clamp(0.0, (AXIS - 1) as f32) as usize;
        let z = (relative.z * AXIS as f32).clamp(0.0, (AXIS - 1) as f32) as usize;
        let cell = morton(x, z);
        assignments.push(cell as u16); counts[cell] += 3;
        // A triangle is assigned once by centroid, but its *whole* source bounds belong to
        // that page. Large triangles and clipped cellar-opening edges may cross many cells.
        for point in points { bounds[cell].include(point); }
    }
    let mut offsets = [0_u32; CELLS + 1];
    for cell in 0..CELLS { offsets[cell + 1] = offsets[cell] + counts[cell]; }
    let mut cursors = offsets;
    let mut indices = vec![0; mesh.indices.len()];
    for (triangle, &cell) in mesh.indices.chunks_exact(3).zip(&assignments) {
        let cursor = &mut cursors[cell as usize];
        indices[*cursor as usize..*cursor as usize + 3].copy_from_slice(triangle); *cursor += 3;
    }
    let mut nodes = Vec::with_capacity(CELLS * 2);
    fn append(nodes: &mut Vec<Node>, bounds: &[Bounds; CELLS], offsets: &[u32; CELLS + 1],
        first: usize, count: usize) -> Option<usize> {
        if offsets[first] == offsets[first + count] { return None; }
        let index = nodes.len();
        nodes.push(Node { bounds: Bounds::empty(), indices: offsets[first]..offsets[first + count], children: [None; 4] });
        if count == 1 { nodes[index].bounds = bounds[first].clone(); }
        else {
            let quarter = count / 4;
            for child in 0..4 {
                let next = append(nodes, bounds, offsets, first + child * quarter, quarter);
                nodes[index].children[child] = next;
                if let Some(next) = next { let child_bounds = nodes[next].bounds.clone(); nodes[index].bounds.union(&child_bounds); }
            }
        }
        Some(index)
    }
    append(&mut nodes, &bounds, &offsets, 0, CELLS)?;
    Some(Prepared { indices, pages: Pages { nodes } })
}

fn fully_inside(planes: &[Vec4; 6], bounds: &Bounds) -> bool {
    planes.iter().all(|plane| {
        let least = Vec3::new(if plane.x >= 0.0 { bounds.min.x } else { bounds.max.x },
            if plane.y >= 0.0 { bounds.min.y } else { bounds.max.y },
            if plane.z >= 0.0 { bounds.min.z } else { bounds.max.z });
        plane.dot(least.extend(1.0)) >= 0.0
    })
}

impl Pages {
    pub fn ranges(&self, matrix: Mat4, size: u32) -> Vec<Range<u32>> {
        if !matrix.is_finite() || size == 0 { return vec![self.nodes[0].indices.clone()]; }
        let planes = frustum_planes(matrix);
        let mut selected: Vec<Range<u32>> = Vec::new();
        fn visit(pages: &Pages, index: usize, matrix: Mat4, planes: &[Vec4; 6], size: u32,
            selected: &mut Vec<Range<u32>>) {
            let node = &pages.nodes[index];
            if !aabb_in_frustum(planes, node.bounds.min, node.bounds.max)
                || !shadow_has_sample(matrix, node.bounds.min, node.bounds.max, size) { return; }
            if node.children.iter().all(Option::is_none) || fully_inside(planes, &node.bounds) {
                if let Some(previous) = selected.last_mut().filter(|r| r.end == node.indices.start) {
                    previous.end = node.indices.end;
                } else { selected.push(node.indices.clone()); }
            } else {
                for child in node.children.into_iter().flatten() { visit(pages, child, matrix, planes, size, selected); }
            }
        }
        visit(self, 0, matrix, &planes, size, &mut selected);
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_load::Vertex;

    fn terrain(side: usize) -> CpuMesh {
        let mut vertices = Vec::new(); let mut indices = Vec::new();
        for z in 0..side { for x in 0..side {
            let x = x as f32 * 4.0 - (side - 1) as f32 * 2.0;
            let z = z as f32 * 4.0 - (side - 1) as f32 * 2.0;
            vertices.push(Vertex { pos: [x, (x * 0.025).sin() * (z * 0.019).cos() * 9.0, z],
                normal: [0.0, 1.0, 0.0], uv: [0.0; 2], tangent: [1.0, 0.0, 0.0, 1.0] });
        }}
        for z in 0..side - 1 { for x in 0..side - 1 {
            let a = (z * side + x) as u32; let b = a + side as u32;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }}
        let min = vertices.iter().map(|v| Vec3::from_array(v.pos)).fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let max = vertices.iter().map(|v| Vec3::from_array(v.pos)).fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        CpuMesh::basic(vertices, indices, None, min, max)
    }

    #[test]
    fn exact_pages_preserve_triangle_multiplicity_winding_and_cross_cell_vertices() {
        let mut mesh = terrain(33);
        // Deliberately spans the entire terrain, with a duplicate to catch set-based omission.
        let across = [0, 32 * 33 + 32, 32];
        mesh.indices.extend_from_slice(&across); mesh.indices.extend_from_slice(&across);
        let prepared = build(&mesh).unwrap();
        let triplets = |indices: &[u32]| {
            let mut out: Vec<[u32; 3]> = indices.chunks_exact(3).map(|t| t.try_into().unwrap()).collect();
            out.sort_unstable(); out
        };
        assert_eq!(triplets(&prepared.indices), triplets(&mesh.indices));
        for node in &prepared.pages.nodes {
            for &index in &prepared.indices[node.indices.start as usize..node.indices.end as usize] {
                let point = Vec3::from_array(mesh.vertices[index as usize].pos);
                assert!(point.cmpge(node.bounds.min).all() && point.cmple(node.bounds.max).all());
            }
        }
        assert!(prepare(&mesh).is_none(), "small geometry avoids an extra upload");
    }

    #[test]
    fn exact_shadow_pages_cull_only_outside_samples_and_keep_the_full_cascade_fast_path() {
        let mesh = terrain(129); let prepared = prepare(&mesh).unwrap();
        let broad = Mat4::orthographic_rh(-500.0, 500.0, -500.0, 500.0, 0.1, 1000.0)
            * Mat4::look_at_rh(Vec3::new(0.0, 500.0, 0.0), Vec3::ZERO, Vec3::Z);
        assert_eq!(prepared.pages.ranges(broad, 384), vec![0..mesh.indices.len() as u32]);
        for yaw in [0.0_f32, 0.3, 1.8, 3.7] {
            let eye = Vec3::new(yaw.cos() * 70.0, 40.0, yaw.sin() * 70.0);
            let matrix = Mat4::orthographic_rh(-8.0, 8.0, -8.0, 8.0, 0.1, 240.0)
                * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            let selected = prepared.pages.ranges(matrix, 384);
            let count: u32 = selected.iter().map(|r| r.end - r.start).sum();
            assert!(count > 0 && count < mesh.indices.len() as u32 / 8, "narrow actual shadow footprint: {count}");
            let planes = frustum_planes(matrix);
            for (i, triangle) in prepared.indices.chunks_exact(3).enumerate() {
                let mut bounds = Bounds::empty();
                for &index in triangle { bounds.include(Vec3::from_array(mesh.vertices[index as usize].pos)); }
                if aabb_in_frustum(&planes, bounds.min, bounds.max)
                    && shadow_has_sample(matrix, bounds.min, bounds.max, 384) {
                    assert!(selected.iter().any(|r| r.contains(&(i as u32 * 3))), "candidate source triangle omitted");
                }
            }
        }
        assert_eq!(prepared.pages.ranges(Mat4::from_cols_array(&[f32::NAN; 16]), 384),
            vec![0..mesh.indices.len() as u32], "invalid projection falls back to full source geometry");
    }

    #[test]
    #[ignore = "actual production shadow depth comparison; coordinator schedules GPU work"]
    fn exact_paged_point_shadow_depth_matches_full_source_draw_on_gpu() {
        use wgpu::util::DeviceExt;
        const SIDE: u32 = 128;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default()
        })).unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::PUSH_CONSTANTS,
            required_limits: wgpu::Limits { max_push_constant_size: 80, ..Default::default() },
            ..Default::default()
        })).unwrap();
        let mesh = terrain(129); let prepared = prepare(&mesh).unwrap();
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exact-terrain-source-vertices"), contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = |label, indices: &[u32]| device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label), contents: bytemuck::cast_slice(indices), usage: wgpu::BufferUsages::INDEX,
        });
        let original = index_buffer("unmodified-source-triangles", &mesh.indices);
        let paged = index_buffer("exact-page-ordering", &prepared.indices);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("production-point-shadow"), source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/point_shadow.wgsl").into()),
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None, entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(144) }, count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[wgpu::PushConstantRange { stages: wgpu::ShaderStages::VERTEX, range: 0..80 }],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("production-shadow-depth-equivalence"), layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex, attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 }] }],
                compilation_options: Default::default() },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[] }),
            multiview: None, cache: None,
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: None, size: 144, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera.as_entire_binding() }] });
        let targets: Vec<_> = (0..2).map(|_| device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain-equivalence-depth"), size: wgpu::Extent3d { width: SIDE, height: SIDE,
                depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        })).collect();
        let views: Vec<_> = targets.iter().map(|target| target.create_view(&Default::default())).collect();
        let target_bytes = u64::from(SIDE * SIDE * 4);
        let readback = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: target_bytes * 2,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        for origin in [Vec3::ZERO, Vec3::new(985.0, 5.49, 1205.0)] {
            let model = Mat4::from_rotation_translation(glam::Quat::from_rotation_y(0.37), origin);
            let source = origin + Vec3::new(17.0, 30.0, -25.0);
            for (direction, up) in [(Vec3::NEG_Y, Vec3::Z), (Vec3::X, Vec3::Y), (Vec3::Z, Vec3::Y)] {
                let projection = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.005, 1000.0);
                let relative = projection * Mat4::look_to_rh(Vec3::ZERO, direction, up);
                let absolute = projection * Mat4::look_to_rh(source, direction, up);
                let ranges = prepared.pages.ranges(absolute * model, SIDE);
                let selected: u32 = ranges.iter().map(|range| range.end - range.start).sum();
                assert!(selected < mesh.indices.len() as u32, "fixture must exercise actual triangle rejection");
                let mut camera_values = Mat4::IDENTITY.to_cols_array().to_vec();
                camera_values.extend(relative.to_cols_array()); camera_values.extend(source.to_array());
                camera_values.push(1000.0); queue.write_buffer(&camera, 0, bytemuck::cast_slice(&camera_values));
                let mut push = model.to_cols_array().to_vec(); push.extend([0.0; 4]);
                let mut encoder = device.create_command_encoder(&Default::default());
                for (index, buffer) in [&original, &paged].into_iter().enumerate() {
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("full-versus-exact-pages"), color_attachments: &[],
                            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                                view: &views[index], depth_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                            timestamp_writes: None, occlusion_query_set: None,
                        });
                        pass.set_pipeline(&pipeline); pass.set_bind_group(0, &bind, &[]);
                        pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::cast_slice(&push));
                        pass.set_vertex_buffer(0, vbuf.slice(..)); pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
                        if index == 0 { pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1); }
                        else { for range in &ranges { pass.draw_indexed(range.clone(), 0, 0..1); } }
                    }
                    encoder.copy_texture_to_buffer(targets[index].as_image_copy(), wgpu::TexelCopyBufferInfo {
                        buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: index as u64 * target_bytes,
                            bytes_per_row: Some(SIDE * 4), rows_per_image: Some(SIDE) } }, targets[index].size());
                }
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                readback.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap(); rx.recv().unwrap().unwrap();
                let bytes = readback.slice(..).get_mapped_range();
                let (reference, candidate) = bytes.split_at(target_bytes as usize);
                assert!(reference.chunks_exact(4).any(|pixel| f32::from_le_bytes(pixel.try_into().unwrap()) < 1.0),
                    "fixture must contain rendered terrain");
                assert_eq!(reference, candidate, "missing/different shadow depth at origin {origin:?}, direction {direction:?}");
                drop(bytes); readback.unmap();
                println!("Exact terrain shadow depth: origin={origin:?}, direction={direction:?}, submitted={selected}/{} indices, {} ranges",
                    mesh.indices.len(), ranges.len());
            }
        }
        println!("Terrain shadow GPU adapter: {:?}", adapter.get_info());
    }
}
