//! Bounded airborne pressure-front presentation. Arrival travels at the shared acoustic speed;
//! the visual is a representative subframe exposure of a curved refractive cap, not a gas solver.
//! It adds no radiance. A film frame may contain the whole cabin transit, so representative
//! shutter sampling must never be reported as slow-motion physical propagation.
use glam::{Mat4, Vec3};
use std::collections::VecDeque;

/// Speed of sound in the current act's air (319 m/s at the winter acts' −20 °C; `sound_propagation`).
pub fn speed_of_sound_m_s() -> f32 {
    crate::sound_propagation::speed_of_sound()
}
pub const MAX_FRONTS: usize = 4;
pub const MAX_OPACITY: f32 = 0.08;
const RECENT_IDS: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct Front {
    pub id: u64,
    pub origin: Vec3,
    pub direction: Vec3,
    pub emitted_at: f32,
    pub strength: f32,
    pub max_radius: f32,
    /// -1 = sphere; 0 = inward hemisphere from an actual radiating surface.
    pub cap_cosine: f32,
}
impl Front {
    pub fn arrival_at(&self, listener: Vec3) -> Option<Arrival> {
        let delta = listener - self.origin;
        let distance = delta.length();
        if !distance.is_finite()
            || distance > self.max_radius
            || (distance > 0.001 && delta.dot(self.direction) / distance < self.cap_cosine)
        {
            return None;
        }
        Some(Arrival {
            id: self.id,
            at: self.emitted_at + distance / speed_of_sound_m_s(),
            source: self.origin,
            direction: delta.normalize_or_zero(),
            strength: self.strength,
        })
    }
    /// Every point lies on the same sphere; tangent distance bends the cap back toward its source.
    pub fn cap_point(
        &self,
        radius: f32,
        tangent: Vec3,
        bitangent: Vec3,
        u: f32,
        v: f32,
    ) -> Option<Vec3> {
        let axial_sq = radius * radius - u * u - v * v;
        if axial_sq < 0.0 {
            return None;
        }
        let axial = axial_sq.sqrt();
        if radius > 0.0 && axial / radius < self.cap_cosine {
            return None;
        }
        Some(self.origin + tangent * u + bitangent * v + self.direction * axial)
    }
    fn valid(&self) -> bool {
        self.origin.is_finite()
            && self.direction.is_finite()
            && self.emitted_at.is_finite()
            && self.strength.is_finite()
            && self.strength > 0.0
            && self.max_radius.is_finite()
            && self.max_radius > 0.0
            && self.cap_cosine.is_finite()
            && self.direction.length_squared() > 0.5
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Arrival {
    pub id: u64,
    pub at: f32,
    pub source: Vec3,
    pub direction: Vec3,
    pub strength: f32,
}

#[derive(Default)]
pub struct Fronts {
    events: VecDeque<Front>,
    recent: VecDeque<u64>,
}
impl Fronts {
    /// A duplicate impact cannot schedule a second pressure sound or camera response. Evicting an
    /// optical cap does not undo its receipt; the caller keeps already scheduled arrival payloads.
    pub fn emit(&mut self, mut front: Front, listener: Vec3) -> Option<Arrival> {
        if !front.valid() || self.recent.contains(&front.id) {
            return None;
        }
        front.direction = front.direction.normalize();
        front.cap_cosine = front.cap_cosine.clamp(-1.0, 0.95);
        front.strength = front.strength.clamp(0.0, 1.0);
        let arrival = front.arrival_at(listener);
        self.recent.push_back(front.id);
        while self.recent.len() > RECENT_IDS {
            self.recent.pop_front();
        }
        self.events.push_back(front);
        while self.events.len() > MAX_FRONTS {
            self.events.pop_front();
        }
        arrival
    }
    pub fn clear(&mut self) {
        self.events.clear();
        self.recent.clear();
    }
    fn samples(&self, now: f32, shutter_s: f32, listener: Vec3) -> [FrontGpu; MAX_FRONTS] {
        let mut samples = [FrontGpu::default(); MAX_FRONTS];
        for (output, front) in samples.iter_mut().zip(self.events.iter().rev()) {
            let age1 = now - front.emitted_at;
            if age1 <= 0.0 {
                continue;
            }
            let r0 = ((age1 - shutter_s.max(0.000_1)).max(0.0) * speed_of_sound_m_s()).max(0.015);
            let r1 = (age1 * speed_of_sound_m_s()).min(front.max_radius);
            if r0 >= r1 {
                continue;
            }
            // One representative radius INSIDE this actual exposure interval. Bias toward the
            // front approaching the observer when its millisecond crossing falls between frames.
            // This preserves a readable bow; it is presentation sampling, not a slower wave.
            let radius = (front.origin.distance(listener) * 0.82).clamp(r0, r1);
            let opacity = MAX_OPACITY * front.strength / (1.0 + radius * 0.04);
            *output = FrontGpu {
                origin_radius: front.origin.extend(radius).to_array(),
                direction_cap: front.direction.extend(front.cap_cosine).to_array(),
                optical: [opacity, 0.10, 1.5, 0.0],
            };
        }
        samples
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct FrontGpu {
    origin_radius: [f32; 4],
    direction_cap: [f32; 4],
    optical: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    inverse_view_projection: [[f32; 4]; 4],
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    viewport: [f32; 4],
    fronts: [FrontGpu; MAX_FRONTS],
}

/// Dedicated transient pass, deliberately outside surface/volume temporal histories and bloom.
/// High uses the live TLAS to stop each source-to-front path at geometry. Portable omits this
/// optical effect rather than pretend screen depth alone proves transmission through armor.
pub struct Gpu {
    snapshot: wgpu::Texture,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: Option<wgpu::RenderPipeline>,
    views: std::cell::RefCell<Vec<(f32, f32)>>,
    width: u32,
    height: u32,
}
impl Gpu {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth: &wgpu::TextureView,
        scene_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Self {
        let snapshot = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pressure-hdr-snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let snapshot_view = snapshot.create_view(&Default::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pressure-uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pressure-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pressure-bgl"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    3,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pressure-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&snapshot_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
            ],
        });
        let pipeline = scene_layout.map(|scene| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("pressure-front"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/pressure_front.wgsl").into(),
                ),
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pressure-pl"),
                bind_group_layouts: &[&layout, scene],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pressure-front"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            })
        });
        Self {
            snapshot,
            uniform,
            bind,
            pipeline,
            views: Default::default(),
            width,
            height,
        }
    }
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        hdr: &wgpu::Texture,
        target: &wgpu::TextureView,
        scene: Option<&wgpu::BindGroup>,
        fronts: &Fronts,
        now: f32,
        view: usize,
        camera: Vec3,
        relative_view_projection: Mat4,
    ) -> bool {
        let (Some(pipeline), Some(scene)) = (&self.pipeline, scene) else {
            return false;
        };
        let shutter = {
            let mut views = self.views.borrow_mut();
            while views.len() <= view {
                views.push((now - 1.0 / 60.0, 1.0 / 60.0));
            }
            let (previous, last_dt) = views[view];
            let dt = if now > previous {
                (now - previous).min(0.1)
            } else {
                last_dt
            };
            views[view] = (now, dt);
            dt
        };
        let samples = fronts.samples(now, shutter, camera);
        if samples.iter().all(|sample| sample.optical[0] <= 0.0) {
            return false;
        }
        let u = Uniform {
            inverse_view_projection: relative_view_projection.inverse().to_cols_array_2d(),
            view_projection: relative_view_projection.to_cols_array_2d(),
            camera: camera.extend(0.0).to_array(),
            viewport: [self.width as f32, self.height as f32, 0.0, 0.0],
            fronts: samples,
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));
        encoder.copy_texture_to_texture(
            hdr.as_image_copy(),
            self.snapshot.as_image_copy(),
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pressure-front"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_bind_group(1, scene, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn front(id: u64) -> Front {
        Front {
            id,
            origin: Vec3::ZERO,
            direction: Vec3::Z,
            emitted_at: 2.0,
            strength: 1.0,
            max_radius: 80.0,
            cap_cosine: 0.0,
        }
    }
    #[test]
    fn arrival_uses_real_distance_and_rejects_backside_or_out_of_range() {
        let f = front(1);
        let arrival = f.arrival_at(Vec3::Z * 2.0).unwrap();
        assert!((arrival.at - (2.0 + 2.0 / speed_of_sound_m_s())).abs() < 1e-6);
        assert!(f.arrival_at(-Vec3::Z).is_none());
        assert!(f.arrival_at(Vec3::Z * 81.0).is_none());
        assert_eq!(arrival.id, 1);
    }
    #[test]
    fn cap_has_constant_radius_and_bows_toward_source_at_edges() {
        let f = front(1);
        let middle = f.cap_point(2.0, Vec3::X, Vec3::Y, 0.0, 0.0).unwrap();
        for u in [-1.6, -0.8, 0.8, 1.6] {
            let p = f.cap_point(2.0, Vec3::X, Vec3::Y, u, 0.0).unwrap();
            assert!((p.length() - 2.0).abs() < 1e-6);
            assert!(p.z < middle.z);
        }
        assert!(f.cap_point(2.0, Vec3::X, Vec3::Y, 2.1, 0.0).is_none());
    }
    #[test]
    fn subframe_sampling_is_bounded_to_exposure_and_does_not_slow_arrival() {
        let mut fs = Fronts::default();
        let arrival = fs.emit(front(1), Vec3::Z * 2.0).unwrap();
        for hz in [24.0, 60.0, 120.0, 240.0] {
            let now = 2.0 + 1.0 / hz;
            let samples = fs.samples(now, 1.0 / hz, Vec3::Z * 2.0);
            let s = samples[0];
            assert!(s.optical[0] > 0.0 && s.optical[0] <= MAX_OPACITY);
            assert!(s.origin_radius[3] <= speed_of_sound_m_s() / hz + 0.0001);
            assert!((arrival.at - (2.0 + 2.0 / speed_of_sound_m_s())).abs() < 1e-6);
        }
        assert!(fs
            .samples(2.4, 1.0 / 24.0, Vec3::Z * 2.0)
            .iter()
            .all(|s| s.optical[0] == 0.0));
    }
    #[test]
    fn one_event_yields_one_arrival_even_after_optical_eviction() {
        let mut fs = Fronts::default();
        let mut arrivals = Vec::new();
        for id in 0..16 {
            if let Some(a) = fs.emit(front(id), Vec3::Z) {
                arrivals.push(a);
            }
        }
        assert_eq!(fs.events.len(), MAX_FRONTS);
        assert_eq!(arrivals.len(), 16);
        for id in 0..16 {
            assert!(fs.emit(front(id), Vec3::Z).is_none());
        }
        let invalid = Front {
            origin: Vec3::splat(f32::NAN),
            ..front(99)
        };
        assert!(fs.emit(invalid, Vec3::Z).is_none());
    }
    #[test]
    fn shader_validates_and_uses_geometry_visibility_without_added_white_energy() {
        let source = include_str!("../shaders/pressure_front.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source).expect("pressure WGSL");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("valid pressure WGSL");
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains("source_blocked(front.origin_radius.xyz,radial,radius)"));
        assert!(compact.contains("min(front.optical.x,0.08)"));
        assert!(compact.contains("mix(original.rgb,refracted,weight)"));
    }
}
