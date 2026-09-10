//! Two explicitly selected practical lamps, with source-geometry cubemap visibility. Static lamp
//! and caster poses reuse depth; light selection never guesses from brightness or camera distance.
use super::*;
use std::cell::{Cell, RefCell};

pub const MAX_LIGHTS: usize = 2;
const SIDE: u32 = 512;
const NEAR: f32 = 0.035;
const FAR: f32 = 8.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniform {
    pub lamps: [[f32; 4]; MAX_LIGHTS], // world position; enabled, matched to packed source position
    pub range: [f32; 4],               // near, far, texel angular scale, active lights
}

pub struct CabinShadows {
    pub view: wgpu::TextureView,
    pub uniform: wgpu::Buffer,
    faces: Vec<wgpu::TextureView>,
    cameras: Vec<wgpu::Buffer>,
    binds: Vec<wgpu::BindGroup>,
    lights: RefCell<Vec<PointLight>>,
    signatures: RefCell<[Option<u64>; MAX_LIGHTS]>,
    dirty: Cell<u32>,
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

impl CabinShadows {
    pub fn new(
        device: &wgpu::Device,
        enabled: bool,
        camera_bgl: &wgpu::BindGroupLayout,
        terrain: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cabin-lamp-shadow-cubes"),
            size: wgpu::Extent3d {
                width: if enabled { SIDE } else { 1 },
                height: if enabled { SIDE } else { 1 },
                depth_or_array_layers: 12,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("cabin-lamp-shadow-cube-array"),
            dimension: Some(wgpu::TextureViewDimension::CubeArray),
            ..Default::default()
        });
        let mut faces = Vec::new();
        let mut cameras = Vec::new();
        let mut binds = Vec::new();
        for face in 0..12 {
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
        Self {
            view,
            uniform,
            faces,
            cameras,
            binds,
            lights: RefCell::new(Vec::new()),
            signatures: RefCell::new([None; MAX_LIGHTS]),
            dirty: Cell::new(0),
        }
    }

    pub fn set_lights(&self, lights: &[PointLight]) {
        let mut selected = self.lights.borrow_mut();
        selected.clear();
        selected.extend(
            lights
                .iter()
                .copied()
                .filter(|l| l.pos.is_finite() && l.radiant_intensity > 0.0)
                .take(MAX_LIGHTS),
        );
    }

    fn signature(renderer: &Renderer, lamp: PointLight) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for f in lamp.pos.to_array() {
            f.to_bits().hash(&mut hash);
        }
        // Only participating casters define this map. Appending distant world geometry must
        // not invalidate all six faces of an otherwise unchanged cabin fixture.
        for (i, m) in renderer.meshes.iter().enumerate() {
            if !m.visible || m.glass || m.raster_only || !near_box(lamp.pos, m.world_min, m.world_max) {
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
        }
        hash.finish()
    }

    pub fn prepare(&self, renderer: &Renderer, queue: &wgpu::Queue, camera: &CameraUniform) {
        let lights = self.lights.borrow();
        let mut uniform = Uniform {
            lamps: [[0.0; 4]; MAX_LIGHTS],
            range: [NEAR, FAR, 2.0 / SIDE as f32, 0.0],
        };
        let mut dirty = 0;
        let mut signatures = self.signatures.borrow_mut();
        for (i, &lamp) in lights.iter().enumerate() {
            // set_lights may trim/reorder packed sources. Match identity by the explicit fixture
            // position so an unrelated muzzle/annunciator never inherits its visibility map.
            if !renderer
                .lights
                .iter()
                .any(|l| l.pos.abs_diff_eq(lamp.pos, 0.0001))
            {
                continue;
            }
            uniform.lamps[i] = [lamp.pos.x, lamp.pos.y, lamp.pos.z, 1.0];
            uniform.range[3] += 1.0;
            let signature = Self::signature(renderer, lamp);
            if signatures[i] == Some(signature) {
                continue;
            }
            signatures[i] = Some(signature);
            dirty |= 1 << i;
            for face in 0..6 {
                let cam = CameraUniform {
                    light_view_proj: face_view_proj(lamp.pos, face).to_cols_array_2d(),
                    ..*camera
                };
                queue.write_buffer(&self.cameras[i * 6 + face], 0, bytemuck::bytes_of(&cam));
            }
        }
        self.dirty.set(self.dirty.get() | dirty);
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn encode(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        work: &mut RenderWorkStats,
    ) {
        let dirty = self.dirty.replace(0);
        if dirty == 0 {
            return;
        }
        let lights = self.lights.borrow();
        for (light, &lamp) in lights.iter().enumerate() {
            if dirty & (1 << light) == 0 {
                continue;
            }
            for face in 0..6 {
                let matrix = face_view_proj(lamp.pos, face);
                let planes = frustum_planes(matrix);
                let pages = renderer.inst_spatial_index.query(matrix);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cabin-lamp-shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.faces[light * 6 + face],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                work.shadow_passes += 1;
                pass.set_pipeline(&renderer.shadow_pipe);
                pass.set_bind_group(0, &self.binds[light * 6 + face], &[]);
                for m in &renderer.meshes {
                    if !m.visible
                        || m.glass
                        || m.raster_only
                        || !near_box(lamp.pos, m.world_min, m.world_max)
                        || !aabb_in_frustum(&planes, m.world_min, m.world_max)
                    {
                        continue;
                    }
                    let push = PushData {
                        model: m.model.to_cols_array_2d(),
                        params: [0.0; 4],
                    };
                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                    work.shadow_mesh_draws += 1;
                }
                pass.set_pipeline(&renderer.shadow_inst_pipe);
                for &index in &pages.batches {
                    let b = &renderer.inst_batches[index];
                    if b.inst_count == 0
                        || b.raster_only_dynamic
                        || b.flags > 3.5
                        || !near_box(lamp.pos, b.world_min, b.world_max)
                    {
                        continue;
                    }
                    let p = &renderer.inst_prototypes[b.prototype];
                    let push = PushData {
                        model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                        params: [0.0; 4],
                    };
                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    pass.set_vertex_buffer(0, p.vbuf.slice(..));
                    pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                    pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
                    work.shadow_instanced_draws += 1;
                }
            }
        }
    }
}

impl Renderer {
    /// Stable named fixture selection supplied by Game, independent of light sorting or exposure.
    pub fn set_shadowed_cabin_lights(&mut self, lights: &[PointLight]) {
        self.cabin_shadows.set_lights(lights);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(std::mem::size_of::<Uniform>(), 48);
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
            for distance in [0.05, 0.2, 1.0, 6.0] {
                let p = face_view_proj(position, face).project_point3(position + axis * distance);
                let reference = FAR / (FAR - NEAR) - FAR * NEAR / ((FAR - NEAR) * distance);
                assert!(
                    (p.z - reference).abs() < 0.0001,
                    "face {face}: {p:?} expected {reference}"
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
