//! Optional retained-source diagnostic. This pass never changes gameplay visibility, collision,
//! transport, injury records, or inventory. Source sectioning is a presentation choice, not armor
//! transparency. Bone pages are approximate source anatomy, not an organ simulation.
use super::{
    BloodFrame, COUCH_CREW_ROLES, Game, ModuleKind,
    crew_physics::CrewBodyPart,
    damage_readout::{DamageReadout, RegionState},
};
use crate::{
    renderer::{DiagnosticMeshLease, Renderer},
    ui::Align,
};
use glam::{Mat4, Vec3};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const MAX_DRAWS: usize = 512;
// Logical 640×360 pixels. The source image and its labels use the same integer grid before 3× export.
const MODEL_VIEWPORT: [f32; 4] = [154.0, 44.0, 480.0, 268.0];
// VT323's authored vertical cell is 80/1000 em: 12.5 px em makes each stroke exactly one
// logical row. The former 8.28 px em reduced strokes to 0.6624 px before binary thresholding.
const LABEL_EM_PX: f32 = 12.5;
const NEAR_SHELL_CUT_Y: f32 = 1.05;
const ROOF_CUT_Y: f32 = 2.80;
// Bounds measured from unchanged baked source GLBs. Turret is translated by its real 0.524 m
// mount; the gun's own source bounds are transformed by the CURRENT articulated gun matrix.
const BODY_BOUNDS: [Vec3; 2] = [
    Vec3::new(-2.061, 0.0, -3.564),
    Vec3::new(2.061, 3.212, 3.564),
];
const GUN_BOUNDS: [Vec3; 2] = [
    Vec3::new(-0.827, -0.284, -1.066),
    Vec3::new(0.935, 0.522, 4.799),
];

fn corners(bounds: [Vec3; 2]) -> [Vec3; 8] {
    std::array::from_fn(|i| {
        Vec3::new(
            bounds[(i & 1) as usize].x,
            bounds[((i >> 1) & 1) as usize].y,
            bounds[((i >> 2) & 1) as usize].z,
        )
    })
}
fn diagnostic_camera(time: f32, gun_in_hull: Mat4) -> Camera {
    let azimuth = 0.65 + time * 0.09;
    // Higher elevation reveals the source cabin floor and breech across the near-side section.
    let target = Vec3::new(0.0, 1.35, 0.0);
    let view = Mat4::look_at_rh(
        target + Vec3::new(azimuth.cos() * 8.0, 6.4, azimuth.sin() * 8.0),
        target,
        Vec3::Y,
    );
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for p in corners(BODY_BOUNDS)
        .into_iter()
        .chain(corners(GUN_BOUNDS).map(|p| gun_in_hull.transform_point3(p)))
    {
        let p = view.transform_point3(p);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    let aspect = MODEL_VIEWPORT[2] / MODEL_VIEWPORT[3];
    let half_y = ((max.y - min.y) * 0.5).max((max.x - min.x) * 0.5 / aspect) * 1.10;
    let half_x = half_y * aspect;
    let projection = Mat4::orthographic_rh(
        center.x - half_x,
        center.x + half_x,
        center.y - half_y,
        center.y + half_y,
        0.1,
        50.0,
    );
    Camera {
        vp: (projection * view).to_cols_array_2d(),
        section: [azimuth.cos(), azimuth.sin(), NEAR_SHELL_CUT_Y, ROOF_CUT_Y],
    }
}
fn label(r: &mut Renderer, text: &str, x: u32, y: u32, large: bool, color: [f32; 4]) {
    r.draw_text(
        text,
        x as f32 / WIDTH as f32,
        y as f32 / HEIGHT as f32,
        LABEL_EM_PX * if large { 2.0 } else { 1.0 } / HEIGHT as f32,
        color,
        Align::Left,
    );
}
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
    tint: [f32; 4],
    // x: source section enabled; y: source-albedo contribution (0 = status color).
    flags: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Camera {
    vp: [[f32; 4]; 4],
    section: [f32; 4],
}
struct Draw {
    lease: DiagnosticMeshLease,
    instance: Instance,
    overlay: bool,
}

fn instance(model: Mat4, tint: [f32; 4], section: bool, albedo: bool) -> Instance {
    Instance {
        model: model.to_cols_array_2d(),
        normal: model.inverse().transpose().to_cols_array_2d(),
        tint,
        flags: [section as u8 as f32, albedo as u8 as f32, 0., 0.],
    }
}
fn bone_color(state: RegionState) -> [f32; 4] {
    match state {
        RegionState::NoRegionRecord => [0.58, 0.69, 0.70, 1.],
        RegionState::Retained => [0.83, 0.78, 0.59, 1.],
        RegionState::PrimaryContact => [1., 0.42, 0.12, 1.],
        RegionState::Detached => [0.85, 0.15, 0.08, 1.],
        RegionState::Removed => [0.60, 0.07, 0.10, 1.],
    }
}
fn geometry_status(kind: Option<ModuleKind>, g: &Game) -> [f32; 4] {
    if kind.is_some_and(|kind| !g.is_module_functional(kind)) {
        [0.90, 0.12, 0.06, 1.]
    } else {
        [0.55, 0.64, 0.68, 1.]
    }
}
fn round_mesh_slot(g: &Game, stable_id: u32) -> Option<usize> {
    if stable_id == 0x1_0000 {
        Some(if g.he_loaded {
            g.slots.chamber_round_he
        } else {
            g.slots.chamber_round
        })
    } else if (0x2_0000..0x2_0000 + super::N_LOOSE as u32).contains(&stable_id) {
        let i = (stable_id - 0x2_0000) as usize;
        Some(if g.loose[i].nature_he {
            g.slots.loose_shell_he[i]
        } else {
            g.slots.loose_shell[i]
        })
    } else if (stable_id as usize) < crate::interior_rig::SHELL_SLOTS.len() {
        Some(g.slots.shell0 + stable_id as usize)
    } else {
        None
    }
}

/// Live model instances in the hull's metre coordinate frame. Hidden source anatomy pages may be
/// leased for this explicit diagnostic; their gameplay visibility is never written. Ammo instances
/// come solely from the same inventory used by internal ballistics, including carried round poses.
fn collect(g: &Game, r: &Renderer, state: &DamageReadout) -> anyhow::Result<Vec<Draw>> {
    let hull_inverse = g.player.hull_model().inverse();
    let mut out = Vec::new();
    let mut add = |slot: usize,
                   model: Option<Mat4>,
                   tint: [f32; 4],
                   section: bool,
                   albedo: bool,
                   overlay: bool| {
        if let Some(lease) = r.diagnostic_mesh_lease(slot) {
            let model = model.unwrap_or(hull_inverse * lease.model);
            if model.is_finite() && model.determinant().abs() > 1e-10 && lease.index_count > 0 {
                out.push(Draw {
                    lease,
                    instance: instance(model, tint, section, albedo),
                    overlay,
                });
            }
        }
    };
    // These are retained authored partitions, not spheres fitted to module positions.
    for (index, &slot) in g.slots.tiger.iter().enumerate() {
        add(
            slot,
            None,
            geometry_status((index == 2).then_some(ModuleKind::MainGun), g),
            index != 2, // Cut armor shell only; the actual gun/breech must remain intact.
            true,
            false,
        );
    }
    for &slot in g
        .slots
        .roof_hatches
        .iter()
        .chain(g.slots.driver_visor_moving.iter())
        .chain(g.slots.tiger_periscopes.iter())
        .chain(g.slots.tiger_dressing.iter())
    {
        add(slot, None, [0.55, 0.64, 0.68, 1.], true, true, false);
    }
    for &slot in &g.slots.gunner_sight {
        add(
            slot,
            None,
            geometry_status(Some(ModuleKind::GunnerSight), g),
            false,
            true,
            false,
        );
    }
    for slot in [
        g.slots.driver_visor_holder,
        g.slots.driver_visor_guides,
        g.slots.driver_roof_eyepiece,
        g.slots.radio_roof_eyepiece,
        g.slots.hatchlever,
        g.slots.gunlever,
        g.slots.breechcover,
        g.slots.turret_disc,
        g.slots.enigma,
        g.slots.hatch,
        g.slots.tpz_needle,
        g.slots.wheel1,
        g.slots.wheel2,
    ] {
        add(
            slot,
            None,
            [0.55, 0.64, 0.68, 1.],
            slot == g.slots.hatch,
            true,
            false,
        );
    }
    let roster = super::couch_crew_bodies(
        g.couch_crew_frame,
        &g.couch_kia_peers,
        &g.couch_crew_identities,
    );
    for (i, crew) in state.crew.iter().enumerate() {
        let identity = COUCH_CREW_ROLES[i];
        let recorded = g
            .couch_ragdolls
            .iter()
            .any(|b| b.casualty.identity == identity);
        // No unoccupied, dismounted, or killed-without-retained-pose body is invented for the view.
        let living = roster.iter().any(|body| {
            body.casualty.identity == identity
                && COUCH_CREW_ROLES.iter().any(|&channel| {
                    g.couch_crew_frame.occupied(channel)
                        && g.couch_crew_frame.roster[channel as usize] == body.casualty.peer_id
                        && !g
                            .couch_crew_frame
                            .input(channel)
                            .flag(crate::lan::F_DISMOUNTED)
                })
        });
        if !recorded && !living {
            continue;
        }
        for part in CrewBodyPart::ALL {
            let region = &crew.regions[part.index()];
            if !region.bone_retained {
                continue;
            }
            let scale = region.half_extents / part.half_extents();
            let model = hull_inverse
                * region.world_from_bone
                * Mat4::from_scale(scale)
                * Mat4::from_translation(-part.rest_center());
            // All four pools share the same eleven source pages. Identity chooses its pool;
            // casualty allocation order does not determine the retained anatomical pose.
            add(
                g.slots.couch_bones[i][part.index()],
                Some(model),
                bone_color(region.state),
                false,
                false,
                true,
            );
        }
    }
    for round in g.live_ammunition_in_frame(BloodFrame::Hull) {
        if let Some(slot) = round_mesh_slot(g, round.stable_id) {
            add(
                slot,
                Some(Mat4::from_rotation_translation(
                    round.rotation,
                    round.center,
                )),
                [0.91, 0.65, 0.20, 1.],
                false,
                true,
                true,
            );
        }
    }
    anyhow::ensure!(
        out.len() <= MAX_DRAWS,
        "retained model diagnostic exceeds {MAX_DRAWS} source draws"
    );
    Ok(out)
}

pub struct Gpu {
    _color: wgpu::Texture,
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    camera: wgpu::Buffer,
    instances: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipe: wgpu::RenderPipeline,
    image: u32,
    capture: wgpu::Texture,
    capture_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    pub last_draw_count: usize,
}
impl Gpu {
    pub fn new(device: &wgpu::Device, r: &mut Renderer) -> Self {
        let texture = |label: &str, w, h, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "retained-model-color",
            WIDTH,
            HEIGHT,
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let color_view = color.create_view(&Default::default());
        let depth = texture(
            "retained-model-depth",
            WIDTH,
            HEIGHT,
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        let image = r.import_diagnostic_image(device, &color_view);
        let buffer = |label: &str, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let camera = buffer(
            "retained-model-camera",
            std::mem::size_of::<Camera>() as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let instances = buffer(
            "retained-model-instances",
            (std::mem::size_of::<Instance>() * MAX_DRAWS) as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("retained-model-layout"),
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
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retained-model-bind"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: instances.as_entire_binding(),
                },
            ],
        });
        let material = r.diagnostic_material_layout();
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("retained-model-pipeline-layout"),
            bind_group_layouts: &[&layout, &material],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("retained-model-shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/damage_model_view.wgsl").into(),
            ),
        });
        let pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {label:Some("retained-model-pipeline"),layout:Some(&pl),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),
                buffers:&[wgpu::VertexBufferLayout {array_stride:48,step_mode:wgpu::VertexStepMode::Vertex,
                    attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4]}]},
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState{format:COLOR,blend:None,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,
                stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None});
        let capture = texture(
            "retained-model-video",
            1920,
            1080,
            COLOR,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let capture_view = capture.create_view(&Default::default());
        let readback = buffer(
            "retained-model-video-readback",
            1920 * 1080 * 4,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        Self {
            _color: color,
            color: color_view,
            depth,
            camera,
            instances,
            bind,
            pipe,
            image,
            capture,
            capture_view,
            readback,
            last_draw_count: 0,
        }
    }
    pub fn draw(
        &mut self,
        g: &Game,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        time: f32,
    ) -> anyhow::Result<()> {
        let state = g.damage_readout();
        let draws = collect(g, r, &state)?;
        self.last_draw_count = draws.len();
        let camera = diagnostic_camera(
            time,
            g.player.hull_model().inverse() * g.player.visual_gun_model(),
        );
        queue.write_buffer(&self.camera, 0, bytemuck::bytes_of(&camera));
        let instances: Vec<_> = draws.iter().map(|d| d.instance).collect();
        if !instances.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&instances));
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained-model-frame"),
        });
        for overlay in [false, true] {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained-source-section"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if overlay {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.008,
                                g: 0.018,
                                b: 0.027,
                                a: 1.,
                            })
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                // Explicit X-ray overlay: reset only this diagnostic depth so source skeletons and
                // live rounds are visible through the section. Mutual body/ammo occlusion remains.
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipe);
            pass.set_viewport(
                MODEL_VIEWPORT[0],
                MODEL_VIEWPORT[1],
                MODEL_VIEWPORT[2],
                MODEL_VIEWPORT[3],
                0.0,
                1.0,
            );
            pass.set_scissor_rect(
                MODEL_VIEWPORT[0] as u32,
                MODEL_VIEWPORT[1] as u32,
                MODEL_VIEWPORT[2] as u32,
                MODEL_VIEWPORT[3] as u32,
            );
            pass.set_bind_group(0, &self.bind, &[]);
            for (i, draw) in draws
                .iter()
                .enumerate()
                .filter(|(_, d)| d.overlay == overlay)
            {
                pass.set_bind_group(1, draw.lease.material.as_ref(), &[]);
                pass.set_vertex_buffer(0, draw.lease.vertices.slice(..));
                pass.set_index_buffer(draw.lease.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.lease.index_count, 0, i as u32..i as u32 + 1);
            }
        }
        queue.submit([encoder.finish()]);
        r.ui_reset();
        r.draw_image(self.image, 0., 0., 1., 1., [1.; 4]);
        r.fill_rect(
            0.,
            0.,
            150.0 / WIDTH as f32,
            1.,
            [0.006, 0.015, 0.022, 0.97],
        );
        let ink = [0.77, 0.85, 0.86, 1.];
        label(r, "TIGER I / SOURCE CUTAWAY", 160, 8, true, ink);
        label(
            r,
            "DIAGNOSTIC / LIVE MODEL + STATE",
            160,
            29,
            false,
            [0.49, 0.64, 0.67, 1.],
        );
        label(r, "COMPONENT STATUS", 8, 10, false, ink);
        for (i, c) in state.components.iter().enumerate() {
            let color = if c.functional {
                [0.44, 0.67, 0.58, 1.]
            } else {
                [0.96, 0.30, 0.14, 1.]
            };
            label(
                r,
                &format!("{} {}", if c.functional { "+" } else { "!" }, c.label),
                8,
                32 + i as u32 * 14,
                false,
                color,
            );
        }
        label(r, "CREW / RETAINED RECORD", 8, 212, false, ink);
        for (i, c) in state.crew.iter().enumerate() {
            label(
                r,
                &format!(
                    "{} / {}",
                    c.identity,
                    if c.has_anatomical_record {
                        "INJURED"
                    } else {
                        "NO RECORD"
                    }
                ),
                8,
                228 + i as u32 * 16,
                false,
                if c.has_anatomical_record {
                    [0.94, 0.44, 0.22, 1.]
                } else {
                    ink
                },
            );
        }
        label(
            r,
            &format!("LIVE ROUNDS {}", state.ammunition.len()),
            8,
            322,
            false,
            [0.89, 0.70, 0.35, 1.],
        );
        label(r, "BONES + ROUNDS: XRAY OVERLAY", 160, 321, false, ink);
        label(
            r,
            "SOURCE SECTIONS / APPROXIMATE SKELETON FIT",
            160,
            337,
            false,
            [0.47, 0.61, 0.64, 1.],
        );
        label(
            r,
            "NO ORGAN HEALTH MODEL",
            160,
            348,
            false,
            [0.47, 0.61, 0.64, 1.],
        );
        Ok(())
    }
    /// Final video export only. The live model texture stays on GPU; no model-image readback,
    /// texture reupload, second Renderer, ray traversal, or history convergence loop is involved.
    pub fn capture_rgba(
        &self,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Vec<u8>> {
        r.render_ui_to_surface(device, queue, &self.capture_view, COLOR, 1920, 1080);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("diagnostic-video-export"),
        });
        encoder.copy_texture_to_buffer(
            self.capture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1920 * 4),
                    rows_per_image: Some(1080),
                },
            },
            wgpu::Extent3d {
                width: 1920,
                height: 1080,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let pixels = data.to_vec();
        drop(data);
        self.readback.unmap();
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn section_camera_frames_actual_body_and_articulated_gun_across_an_orbit() {
        for step in 0..72 {
            let time = step as f32 * std::f32::consts::TAU / (72.0 * 0.09);
            for yaw in [-2.0, 0.0, 1.2] {
                for elevation in [-0.15, 0.0, 0.3] {
                    let gun = Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot)
                        * Mat4::from_rotation_y(yaw)
                        * Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion)
                        * Mat4::from_rotation_x(elevation);
                    let camera = diagnostic_camera(time, gun);
                    let vp = Mat4::from_cols_array_2d(&camera.vp);
                    for p in corners(BODY_BOUNDS)
                        .into_iter()
                        .chain(corners(GUN_BOUNDS).map(|p| gun.transform_point3(p)))
                    {
                        let clip = vp * p.extend(1.0);
                        let ndc = clip.truncate() / clip.w;
                        assert!(
                            ndc.x.abs() < 0.92 && ndc.y.abs() < 0.92,
                            "source geometry leaves its dedicated viewport: {ndc:?}"
                        );
                        assert!((0.0..1.0).contains(&ndc.z));
                    }
                }
            }
        }
    }

    #[test]
    fn native_diagnostic_text_preserves_all_capital_stroke_rows_after_atlas_sampling() {
        // Reproduce the actual Ui glyph coverage, linear sampling and 0.42 binary threshold on
        // CPU. This catches the old subpixel-row loss without asserting a GPU visual result.
        use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
        let font =
            FontRef::try_from_slice(include_bytes!("../assets/fonts/VT323-Regular.ttf")).unwrap();
        let atlas_em = 110.0;
        let sf = font.as_scaled(PxScale::from(atlas_em));
        for em in [LABEL_EM_PX, LABEL_EM_PX * 2.0] {
            let factor = em / atlas_em;
            for ch in ['H', 'E', 'B', '8'] {
                let outline = font
                    .outline_glyph(font.glyph_id(ch).with_scale(atlas_em))
                    .unwrap();
                let bounds = outline.px_bounds();
                let w = bounds.width().ceil() as usize;
                let h = bounds.height().ceil() as usize;
                let mut coverage = vec![0.0f32; w * h];
                outline.draw(|x, y, value| {
                    coverage[y as usize * w + x as usize] = (value * 255.0 + 0.5).floor() / 255.0
                });
                let origin = glam::Vec2::new(8.0, 10.0);
                let baseline = origin.y + sf.ascent() * factor;
                let first =
                    origin + glam::Vec2::new(bounds.min.x, sf.ascent() + bounds.min.y) * factor;
                let sample = |x: isize, y: isize| -> f32 {
                    if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
                        0.0
                    } else {
                        coverage[y as usize * w + x as usize]
                    }
                };
                let cap_rows = (em * 0.56).round() as i32;
                for row in baseline.round() as i32 - cap_rows..baseline.round() as i32 {
                    let mut pixels = 0;
                    for column in 0..40 {
                        let q = (glam::Vec2::new(column as f32 + 0.5, row as f32 + 0.5) - first)
                            / factor
                            - glam::Vec2::splat(0.5);
                        let x = q.x.floor() as isize;
                        let y = q.y.floor() as isize;
                        let fx = q.x - x as f32;
                        let fy = q.y - y as f32;
                        let a = sample(x, y) * (1.0 - fx) + sample(x + 1, y) * fx;
                        let b = sample(x, y + 1) * (1.0 - fx) + sample(x + 1, y + 1) * fx;
                        pixels += usize::from(a * (1.0 - fy) + b * fy >= 0.42);
                    }
                    assert!(
                        pixels > 0,
                        "{ch} loses native stroke row {row} at {em} px em"
                    );
                }
            }
        }
    }

    #[test]
    fn retained_model_shader_validates_and_matches_gpu_records() {
        let module =
            wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/damage_model_view.wgsl"))
                .unwrap();
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        for (name, expected) in [
            ("Camera", std::mem::size_of::<Camera>()),
            ("Instance", std::mem::size_of::<Instance>()),
        ] {
            let ty = module
                .types
                .iter()
                .find(|(_, t)| t.name.as_deref() == Some(name))
                .unwrap()
                .1;
            let wgpu::naga::TypeInner::Struct { span, .. } = &ty.inner else {
                panic!("record must be a struct")
            };
            assert_eq!(*span as usize, expected);
        }
    }
    #[test]
    fn retained_bone_attachment_preserves_articulated_origin_and_source_scale() {
        let part = CrewBodyPart::RightForearm;
        let pose = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_z(0.7),
            Vec3::new(0.4, 1.6, -0.2),
        );
        let scale = Vec3::splat(1.17);
        let model = pose * Mat4::from_scale(scale) * Mat4::from_translation(-part.rest_center());
        assert!(
            model
                .transform_point3(part.rest_center())
                .distance(pose.w_axis.truncate())
                < 1e-6
        );
        let p = part.rest_center() + Vec3::X * 0.02;
        assert!((model.transform_point3(p).distance(pose.w_axis.truncate()) - 0.0234).abs() < 1e-6);
    }
    #[test]
    fn hull_relative_diagnostic_survives_large_world_translation() {
        let hull = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(0.8),
            Vec3::new(400., 20., -280.),
        );
        let local = Mat4::from_translation(Vec3::new(0.2, 1.5, -0.3));
        let relative = hull.inverse() * (hull * local);
        assert!((relative.w_axis - local.w_axis).abs().max_element() < 0.0001);
        assert_ne!(
            bone_color(RegionState::NoRegionRecord),
            bone_color(RegionState::Retained)
        );
        assert_ne!(
            bone_color(RegionState::Removed),
            bone_color(RegionState::Detached)
        );
    }
}
