//! Existing Pizza hand props, source-derived articulation, and shared physical VFX anchors.
//! Asset scale/hand poses and the packet's hinged opening are authored gameplay dimensions;
//! the modern packet/lighter artwork is not a claim of period reconstruction.
use crate::{
    cigarette::View,
    gltf_load::{CpuImage, CpuMesh, Vertex},
    renderer::Renderer,
};
use anyhow::{Context, Result};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};
const ROOT: &str =
    "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";
pub const CIGARETTE_LENGTH_M: f32 = 0.084;
pub const FILTER_LENGTH_M: f32 = 0.022;
pub const CIGARETTE_RADIUS_M: f32 = 0.0135 * 0.5 * (0.084 / 0.153);
const PACK_COUNT: usize = 20;
/// Butts thrown down persist; past this many the oldest is given up.
pub const BUTT_CAPACITY: usize = 160;
pub const BUTT_LENGTH_M: f32 = FILTER_LENGTH_M + 0.004;
/// The open packet stays where it was brought out; the view may turn this far before it follows.
const PACK_LEASH_RAD: f32 = 0.38;
const FLAME_HEIGHT_M: f32 = 0.008;
const FLAME_RADIUS_M: f32 = 0.001;
const FLAME_LIGHT_HEIGHT_M: f32 = 0.0035;
#[derive(Clone, Copy, Debug, Default)]
pub struct Anchors {
    pub tip: Vec3,
    pub mouth: Vec3,
    pub lighter: Vec3,
    pub forward: Vec3,
    pub radius: f32,
    pub cigarette_visible: bool,
    pub lighter_visible: bool,
}
pub struct HandProps {
    body: usize,
    lid: usize,
    stock: [usize; PACK_COUNT],
    cigarette: usize,
    lighter: usize,
    ember: usize,
    tobacco_cap: usize,
    charred: usize,
    ash: usize,
    flame: usize,
    source: CpuMesh,
    pose: Vec<Vertex>,
    last_length: f32,
    pack_min: Vec3,
    pack_max: Vec3,
    butts: Vec<usize>,
    /// View direction the open packet is held out along, and its pose this frame.
    pack_anchor: Option<Vec3>,
    pack_model: Mat4,
}
fn load(relative: &str) -> Result<CpuMesh> {
    crate::gltf_load::load_glb(&format!("{ROOT}{relative}"))
}
fn push(scene: &mut Vec<CpuMesh>, mesh: CpuMesh) -> usize {
    let slot = scene.len();
    scene.push(mesh);
    slot
}
fn subset(mesh: &CpuMesh, keep: impl Fn(&[u32]) -> bool) -> Result<CpuMesh> {
    let mut out = mesh.clone();
    out.vertices.clear();
    out.indices.clear();
    out.min = Vec3::splat(f32::INFINITY);
    out.max = Vec3::splat(f32::NEG_INFINITY);
    for triangle in mesh.indices.chunks_exact(3).filter(|t| keep(t)) {
        for &i in triangle {
            let v = mesh.vertices[i as usize];
            let p = Vec3::from(v.pos);
            out.indices.push(out.vertices.len() as u32);
            out.vertices.push(v);
            out.min = out.min.min(p);
            out.max = out.max.max(p);
        }
    }
    anyhow::ensure!(
        !out.indices.is_empty(),
        "source hand-prop partition is empty"
    );
    Ok(out)
}
fn flat_mesh(vertices: Vec<Vertex>, indices: Vec<u32>, color: [u8; 3]) -> CpuMesh {
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for v in &vertices {
        lo = lo.min(Vec3::from(v.pos));
        hi = hi.max(Vec3::from(v.pos));
    }
    CpuMesh::basic(
        vertices,
        indices,
        Some(CpuImage::new(1, 1, vec![color[0], color[1], color[2], 255])),
        lo,
        hi,
    )
}
fn vertex(p: Vec3, n: Vec3) -> Vertex {
    Vertex {
        pos: p.to_array(),
        normal: n.to_array(),
        uv: [0.0; 2],
        tangent: [1.0, 0.0, 0.0, 1.0],
    }
}
/// Eight-sided cross-section matches the existing source cigarette; no screen-space glow card.
fn cap_mesh() -> CpuMesh {
    let mut vertices = vec![vertex(Vec3::ZERO, Vec3::Z)];
    let mut indices = Vec::new();
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        vertices.push(vertex(Vec3::new(a.cos(), a.sin(), 0.0), Vec3::Z));
    }
    for i in 0..8 {
        indices.extend([0, i + 1, (i + 1) % 8 + 1]);
    }
    flat_mesh(vertices, indices, [255, 128, 35])
}
fn flame_mesh() -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (ring, (y, radius)) in [
        (0.0, 0.0004),
        (0.0025, FLAME_RADIUS_M),
        (FLAME_HEIGHT_M, 0.00006),
    ]
    .into_iter()
    .enumerate()
    {
        for i in 0..8 {
            let a = i as f32 * std::f32::consts::TAU / 8.0;
            let mut v = vertex(
                Vec3::new(a.cos() * radius, y, a.sin() * radius),
                Vec3::new(a.cos(), 0.1, a.sin()).normalize(),
            );
            v.uv = [0.5, (ring as f32 + 0.5) / 3.0];
            vertices.push(v);
        }
    }
    for ring in 0..2 {
        for i in 0..8 {
            let a = ring * 8 + i;
            let b = ring * 8 + (i + 1) % 8;
            indices.extend([a, b, a + 8, b, b + 8, a + 8]);
        }
    }
    let mut mesh = flat_mesh(vertices, indices, [255, 207, 102]);
    mesh.texture = Some(CpuImage::new(
        1,
        3,
        vec![112, 154, 255, 255, 255, 168, 53, 255, 255, 228, 135, 255],
    ));
    mesh
}
/// Project-authored small flame output, shared with the real source light. The existing point
/// transport has inverse-square support and geometry shadows; no invented distance cutoff.
pub fn lighter_light(view: View, anchors: Anchors) -> Option<crate::renderer::PointLight> {
    (view.lighter_flame && anchors.lighter_visible).then_some(crate::renderer::PointLight {
        pos: anchors.lighter + Vec3::Y * FLAME_LIGHT_HEIGHT_M,
        source_radius: FLAME_RADIUS_M,
        color: Vec3::new(1.0, 0.67, 0.27),
        radiant_intensity: 0.00025,
    })
}
/// Radiant intensity of the burning tip at rest and at the peak of a toke [E]. The cabin dome
/// lamp is about 0.000004 and the lighter flame 0.00025 in the same units, so the tip is dimmer
/// than the flame and still enough to read a dial or a face at arm's length.
pub const TIP_INTENSITY_IDLE: f32 = 0.000018;
pub const TIP_INTENSITY_TOKE: f32 = 0.000070;
pub fn cigarette_tip_light(view: View, anchors: Anchors) -> Option<crate::renderer::PointLight> {
    (view.lit && view.cigarette_present && anchors.cigarette_visible
        && anchors.tip.is_finite()).then_some(crate::renderer::PointLight {
        pos: anchors.tip,
        source_radius: 0.004,
        color: Vec3::new(1.0, 0.40 + 0.12 * view.puff_phase.clamp(0.0, 1.0), 0.05),
        radiant_intensity: TIP_INTENSITY_IDLE
            + (TIP_INTENSITY_TOKE - TIP_INTENSITY_IDLE) * view.puff_phase.clamp(0.0, 1.0),
    })
}
fn sleeve_mesh(color: [u8; 3]) -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let b = (i + 1) as f32 * std::f32::consts::TAU / 8.0;
        let n = Vec3::new(((a + b) * 0.5).cos(), ((a + b) * 0.5).sin(), 0.0);
        let base = vertices.len() as u32;
        for (angle, z) in [(a, 0.0), (b, 0.0), (b, 1.0), (a, 1.0)] {
            vertices.push(vertex(Vec3::new(angle.cos(), angle.sin(), z), n));
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    flat_mesh(vertices, indices, color)
}
fn cigarette_source() -> Result<CpuMesh> {
    let source = load("Small Props/cig_1.glb")?;
    let size = source.max - source.min;
    anyhow::ensure!(
        (size.x - 0.153).abs() < 0.00001 && (size.y - 0.0135).abs() < 0.00001,
        "source cigarette dimensions changed; review anchors before use"
    );
    let scale = CIGARETTE_LENGTH_M / size.x;
    // Source +X is the pale filter cap; -X is the brown tobacco cap (atlas verified).
    let matrix = Mat4::from_cols(
        Vec3::new(0.0, 0.0, -scale).extend(0.0),
        Vec3::new(0.0, scale, 0.0).extend(0.0),
        Vec3::new(scale, 0.0, 0.0).extend(0.0),
        Vec3::new(
            -(source.min.z + source.max.z) * 0.5 * scale,
            -(source.min.y + source.max.y) * 0.5 * scale,
            source.max.x * scale,
        )
        .extend(1.0),
    );
    Ok(source.transformed(matrix))
}
pub fn cigarette_length(burn_remaining: f32) -> f32 {
    FILTER_LENGTH_M
        + (CIGARETTE_LENGTH_M - FILTER_LENGTH_M)
            * if burn_remaining.is_finite() {
                burn_remaining.clamp(0.0, 1.0)
            } else {
                0.0
            }
}
/// The source is an eight-sided tube with two end rings. Moving only the burning ring and
/// interpolating its side UVs clips the paper; the filter/mouth end and diameter stay unchanged.
fn clipped_cigarette(source: &CpuMesh, length: f32, out: &mut [Vertex]) {
    for (v, original) in out.iter_mut().zip(&source.vertices) {
        *v = *original;
        let p = Vec3::from(original.pos);
        if p.z < CIGARETTE_LENGTH_M - 0.00001 {
            continue;
        }
        v.pos[2] = length;
        if original.normal[2].abs() < 0.9 {
            if let Some(other) = source.vertices.iter().find(|other| {
                other.pos[2] < 0.00001
                    && Vec2::new(other.pos[0] - p.x, other.pos[1] - p.y).length_squared() < 1e-10
                    && Vec3::from(other.normal).dot(Vec3::from(original.normal)) > 0.99
            }) {
                v.uv = Vec2::from(other.uv)
                    .lerp(Vec2::from(original.uv), length / CIGARETTE_LENGTH_M)
                    .to_array();
            }
        }
    }
}

/// Hold source props within the ordinary camera frustum, with no overlay or depth override.
/// The cigarette lies almost along the line of sight, filter toward the eye: tipped up ten
/// degrees and a few to the left. At rest it is held low and a little right; while it is being
/// lit or drawn on it comes to the centre of the view on the lower third line, where the flame
/// and the brightening tip can be watched; the exhale leaves from that same place. `raise` is
/// the larger of the lighter's raise and the toke. `tap` is the 0..1 phase of knocking the ash
/// off (0 when idle).
fn cigarette_pose(eye: Vec3, forward: Vec3, right: Vec3, up: Vec3, raise: f32, tan_half_fov: f32, tap: f32) -> (Mat4, Vec3) {
    const RAISED_DISTANCE_M: f32 = 0.28;
    let tan_half_fov = if tan_half_fov.is_finite() { tan_half_fov.clamp(0.1, 1.5) } else { 0.65 };
    // One third of the half-height below the centre is the lower third line of the frame.
    let raised_at = eye + forward * RAISED_DISTANCE_M - up * (RAISED_DISTANCE_M * tan_half_fov / 3.0);
    let resting = eye + forward * 0.27 + right * 0.075 - up * 0.085;
    let raise = raise.clamp(0.0, 1.0);
    let raised = raise * raise * (3.0 - 2.0 * raise);
    let tap = if tap.is_finite() { tap.clamp(0.0, 1.0) } else { 0.0 };
    // Two quick knocks: the hand dips aside and the tip flicks down twice.
    let dip = (std::f32::consts::PI * tap).sin();
    let flick = (std::f32::consts::TAU * 2.0 * tap).sin().max(0.0) * 0.45 * dip;
    let grip = resting.lerp(raised_at, raised) + (right * 0.035 - up * 0.02) * dip;
    let axis = (forward + up * (10.0_f32.to_radians().tan() - flick) - right * 4.0_f32.to_radians().tan())
        .normalize();
    (
        Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Z, axis), grip),
        // The breath leaves from where the cigarette is held while drawn on.
        raised_at,
    )
}

/// Between the lips: the filter just under the eye, the cigarette pointing where the head points.
fn mouth_pose(eye: Vec3, forward: Vec3, up: Vec3) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Z, forward),
        eye + forward * 0.10 - up * 0.065)
}

/// The source packet's long axis is X. Open its filter-end face, not its broad side,
/// and face that physical opening toward the eye so all three stock rows can be inspected.
/// It is held out in front, just under the line of sight, close enough to pick one out.
fn packet_pose(eye: Vec3, forward: Vec3, right: Vec3, up: Vec3, lo: Vec3, hi: Vec3) -> Mat4 {
    let opening = (up * 0.65 - forward * 0.76).normalize();
    let front = right.cross(opening).normalize();
    Mat4::from_rotation_translation(
        Quat::from_mat3(&Mat3::from_cols(opening, front, right)),
        eye + forward * 0.26 - up * 0.085,
    ) * Mat4::from_translation(-(lo + hi) * 0.5)
}

fn packet_lid_pose(pack: Mat4, lo: Vec3, hi: Vec3) -> Mat4 {
    let hinge = Vec3::new(hi.x, lo.y, (lo.z + hi.z) * 0.5);
    pack * Mat4::from_translation(hinge)
        * Mat4::from_rotation_z(-2.05)
        * Mat4::from_translation(-hinge)
}

impl HandProps {
    pub fn allocate(scene: &mut Vec<CpuMesh>) -> Result<Self> {
        // Complete every source load/partition before mutating the shared scene.
        let full_cigarette = cigarette_source()?;
        let is_tip = |triangle: &[u32]| {
            triangle
                .iter()
                .all(|&i| full_cigarette.vertices[i as usize].normal[2] > 0.99)
        };
        let source = subset(&full_cigarette, |t| !is_tip(t))?;
        let tobacco_cap_mesh = subset(&full_cigarette, is_tip)?;
        let scale = CIGARETTE_LENGTH_M / 0.153;
        let pack = load("Items & Weapons/cigs_packet_1.glb")?
            .transformed(Mat4::from_scale(Vec3::splat(scale)));
        let pack_min = pack.min;
        let pack_max = pack.max;
        let is_lid = |triangle: &[u32]| {
            triangle
                .iter()
                .all(|&i| (pack.vertices[i as usize].pos[0] - pack.max.x).abs() < 0.00001)
        };
        let body_mesh = subset(&pack, |t| !is_lid(t))?;
        let lid_mesh = subset(&pack, is_lid)?;
        anyhow::ensure!(
            lid_mesh.indices.len() == 6,
            "packet filter-end is not the expected source quad"
        );
        let lighter_source = load("Items & Weapons/lighter_mp_1.glb")?;
        // Existing burner's source location, not the bounding-box tip of the adjacent flint wheel.
        let nozzle = Vec3::new(-0.010, 0.010, -0.069);
        let lighter_mesh = lighter_source.transformed(
            Mat4::from_scale(Vec3::splat(0.55))
                * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
                * Mat4::from_translation(-nozzle),
        );
        let body = push(scene, body_mesh);
        let lid = push(scene, lid_mesh);
        let stock = std::array::from_fn(|_| push(scene, full_cigarette.clone()));
        let cigarette = push(scene, source.clone());
        let lighter = push(scene, lighter_mesh);
        let ember = push(scene, cap_mesh());
        let tobacco_cap = push(scene, tobacco_cap_mesh);
        let charred = push(scene, sleeve_mesh([35, 31, 27]));
        let ash = push(scene, sleeve_mesh([143, 141, 132]));
        let flame = push(scene, flame_mesh());
        let pose = source.vertices.clone();
        // A butt is the same source paper clipped to just past the filter.
        let mut butt_mesh = source.clone();
        clipped_cigarette(&source, BUTT_LENGTH_M, &mut butt_mesh.vertices);
        butt_mesh.min = Vec3::splat(f32::INFINITY);
        butt_mesh.max = Vec3::splat(f32::NEG_INFINITY);
        for v in &butt_mesh.vertices {
            butt_mesh.min = butt_mesh.min.min(Vec3::from(v.pos));
            butt_mesh.max = butt_mesh.max.max(Vec3::from(v.pos));
        }
        let butts = (0..BUTT_CAPACITY).map(|_| push(scene, butt_mesh.clone())).collect();
        Ok(Self {
            body,
            lid,
            stock,
            cigarette,
            lighter,
            ember,
            tobacco_cap,
            charred,
            ash,
            flame,
            source,
            pose,
            last_length: CIGARETTE_LENGTH_M,
            pack_min,
            pack_max,
            butts,
            pack_anchor: None,
            pack_model: Mat4::IDENTITY,
        })
    }
    pub fn lighter_emitter(&self) -> usize {
        self.flame
    }
    pub fn butt_slots(&self) -> &[usize] {
        &self.butts
    }
    pub fn stock_slot(&self, slot: u8) -> Option<usize> {
        self.stock.get(usize::from(slot)).copied()
    }
    /// The cigarette in the open packet that the line of sight rests on: the nearest filter end
    /// within a little more than its own radius of the ray.
    pub fn pick(&self, eye: Vec3, forward: Vec3, view: View) -> Option<u8> {
        if !view.pack_open || view.cigarette_present || self.pack_anchor.is_none() { return None; }
        let forward = forward.normalize_or_zero();
        pack_positions(self.pack_min, self.pack_max).iter().enumerate()
            .filter(|(i, _)| view.pack_slots & (1 << i) != 0)
            .filter_map(|(i, &position)| {
                let end = self.pack_model.transform_point3(position);
                let along = (end - eye).dot(forward);
                let miss = (eye + forward * along - end).length();
                (along > 0.05 && along < 0.8 && miss <= CIGARETTE_RADIUS_M * 1.3).then_some((i as u8, miss))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
    }

    pub fn slots(&self) -> impl Iterator<Item = usize> + '_ {
        [
            self.body,
            self.lid,
            self.cigarette,
            self.lighter,
            self.ember,
            self.tobacco_cap,
            self.charred,
            self.ash,
            self.flame,
        ]
        .into_iter()
        .chain(self.stock)
        .chain(self.butts.iter().copied())
    }
    pub fn initialize(&self, r: &mut Renderer) {
        for slot in self.slots() {
            r.mark_raster_only(slot);
            r.mark_temporal_dynamic(slot);
            r.set_visible(slot, false);
            r.set_emissive(slot, 0.0);
        }
        r.mark_own_emissive(self.ember);
        r.mark_own_emissive(self.flame);
    }
    /// `hands` is whether the hands may be shown; `face` whether the view is the naked eye (a
    /// cigarette in the mouth needs no hands, only no optic over the eye).
    pub fn update(&mut self, r: &mut Renderer, view: View, hands: bool, face: bool) -> Anchors {
        let visible = hands && view.equipped;
        let forward = r.camera.forward().normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let basis = Quat::from_mat3(&Mat3::from_cols(right, up, -forward));
        let (hand_model, mouth) = cigarette_pose(r.camera.pos, forward, right, up,
            view.puff_phase.max(view.lighter_raise), (r.camera.fov_y * 0.5).tan(), view.ash_tap);
        let model = if view.in_mouth { mouth_pose(r.camera.pos, forward, up) } else { hand_model };
        let length = cigarette_length(view.burn_remaining);
        if (length - self.last_length).abs() > 0.000002 {
            clipped_cigarette(&self.source, length, &mut self.pose);
            if r.upload_pose_vertices(self.cigarette, &self.pose) {
                self.last_length = length;
            }
        }
        // Anchors use the last uploaded geometry length, including a backend rejecting a pose.
        let tip = model.transform_point3(Vec3::Z * self.last_length);
        let cigarette_visible = view.cigarette_present && if view.in_mouth { face } else { visible };
        r.set_model(self.cigarette, model);
        r.set_visible(self.cigarette, cigarette_visible);
        r.set_visible(self.tobacco_cap, cigarette_visible && !view.lit);
        r.set_model(
            self.tobacco_cap,
            model * Mat4::from_translation(Vec3::Z * (self.last_length - CIGARETTE_LENGTH_M)),
        );
        let surface = crate::cigarette_vfx::surface(view);
        // The coal swells a little past the paper as it is drawn on.
        let coal = CIGARETTE_RADIUS_M * (1.12 + 0.28 * view.puff_phase.clamp(0.0, 1.0));
        let cap = model
            * Mat4::from_translation(Vec3::Z * (self.last_length + 0.0002))
            * Mat4::from_scale(Vec3::new(coal, coal, 1.0));
        r.set_model(self.ember, cap);
        r.set_visible(self.ember, cigarette_visible && view.lit);
        r.set_emissive(
            self.ember,
            if cigarette_visible && view.lit {
                surface.ember_intensity
            } else {
                0.0
            },
        );
        for (slot, extent, offset) in [
            (self.charred, surface.char_length_m, 0.00001),
            (self.ash, surface.ash_length_m, 0.000025),
        ] {
            let extent = extent.min((self.last_length - FILTER_LENGTH_M).max(0.0));
            r.set_visible(slot, cigarette_visible && extent > 0.00005);
            r.set_model(
                slot,
                model
                    * Mat4::from_translation(Vec3::Z * (self.last_length - extent))
                    * Mat4::from_scale(Vec3::new(
                        CIGARETTE_RADIUS_M + offset,
                        CIGARETTE_RADIUS_M + offset,
                        extent.max(0.000001),
                    )),
            );
        }
        let pack_visible = visible && view.pack_open;
        // The open packet is held out where it was brought out, so the eye can move over it and
        // rest on one cigarette. Turn far enough and the hand follows.
        let held_along = if pack_visible {
            let mut along = self.pack_anchor.filter(|a| a.is_finite()).unwrap_or(forward);
            let angle = along.angle_between(forward);
            if angle > PACK_LEASH_RAD {
                along = along.lerp(forward, 1.0 - PACK_LEASH_RAD / angle).normalize_or_zero();
            }
            self.pack_anchor = Some(along);
            along
        } else {
            self.pack_anchor = None;
            forward
        };
        let held_right = held_along.cross(Vec3::Y).normalize_or_zero();
        let held_up = held_right.cross(held_along).normalize_or_zero();
        let pack_model = packet_pose(
            r.camera.pos,
            held_along,
            held_right,
            held_up,
            self.pack_min,
            self.pack_max,
        );
        self.pack_model = pack_model;
        r.set_model(self.body, pack_model);
        r.set_visible(self.body, pack_visible);
        r.set_model(
            self.lid,
            packet_lid_pose(pack_model, self.pack_min, self.pack_max),
        );
        r.set_visible(self.lid, pack_visible);
        let stock_positions = pack_positions(self.pack_min, self.pack_max);
        for (i, &slot) in self.stock.iter().enumerate() {
            r.set_visible(slot, pack_visible && view.pack_slots & (1 << i) != 0);
            r.set_model(slot, pack_model * stock_model(stock_positions[i], i));
        }
        let raised = view.lighter_raise.clamp(0.0, 1.0);
        let lowered = r.camera.pos + forward * 0.24 - right * 0.13 - up * 0.33;
        let nozzle = lowered.lerp(tip - Vec3::Y * FLAME_HEIGHT_M, raised);
        r.set_model(
            self.lighter,
            Mat4::from_rotation_translation(basis * Quat::from_rotation_y(-0.3), nozzle),
        );
        let lighter_visible = visible && !view.pack_open && raised > 0.01;
        r.set_visible(self.lighter, lighter_visible);
        let flame_visible = lighter_visible && view.lighter_flame;
        // Flame rises in world gravity; changing the player's look never turns a flame sideways.
        r.set_model(self.flame, Mat4::from_translation(nozzle));
        r.set_visible(self.flame, flame_visible);
        r.set_emissive(self.flame, if flame_visible { 2.4 } else { 0.0 });
        Anchors {
            tip,
            mouth,
            lighter: nozzle,
            forward,
            radius: CIGARETTE_RADIUS_M,
            cigarette_visible,
            lighter_visible,
        }
    }
}
/// A cigarette in its slot, filter toward the opening. The one in the inverted slot is the
/// other way up: tobacco toward the opening, as a smoker turns one over in a fresh packet.
fn stock_model(position: Vec3, slot: usize) -> Mat4 {
    if slot == usize::from(crate::cigarette::INVERTED_SLOT) {
        Mat4::from_translation(position - Vec3::X * CIGARETTE_LENGTH_M)
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
    } else {
        Mat4::from_translation(position) * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
    }
}
/// Seven/six/seven physical cigarettes fit the source-derived packet's three rows.
fn pack_positions(lo: Vec3, hi: Vec3) -> [Vec3; PACK_COUNT] {
    let diameter = 2.0 * CIGARETTE_RADIUS_M;
    std::array::from_fn(|i| {
        let (row, column, count) = if i < 7 {
            (0, i, 7)
        } else if i < 13 {
            (1, i - 7, 6)
        } else {
            (2, i - 13, 7)
        };
        Vec3::new(
            (lo.x + hi.x + CIGARETTE_LENGTH_M) * 0.5,
            lo.y + CIGARETTE_RADIUS_M + 0.0003 + row as f32 * (diameter + 0.00012),
            (column as f32 - (count - 1) as f32 * 0.5) * (diameter + 0.00015),
        )
    })
}
/// Reuse the exact source flashlight, partitioning the actual emissive atlas face triangles.
/// Returned body/lens point along local +Z; the15mm lens is at the existing torch's z=.035m.
pub fn load_hand_torch() -> Result<[CpuMesh; 2]> {
    let path = format!("{ROOT}Items & Weapons/flashlight_mp_1.glb");
    let source = crate::gltf_load::load_glb(&path)?;
    let bytes = crate::resource::read(&path)?;
    let gltf = gltf::Gltf::from_slice(&bytes)?;
    let info = gltf
        .materials()
        .find_map(|m| m.emissive_texture())
        .context("source torch emissive atlas")?;
    let gltf::image::Source::View { view, .. } = info.texture().source().source() else {
        anyhow::bail!("embedded torch emissive atlas required")
    };
    let blob = gltf.blob.as_deref().context("source torch GLB buffer")?;
    let image =
        image::load_from_memory(&blob[view.offset()..view.offset() + view.length()])?.to_rgba8();
    let luminous = |triangle: &[u32]| {
        let uv = triangle
            .iter()
            .map(|&i| Vec2::from(source.vertices[i as usize].uv))
            .sum::<Vec2>()
            / 3.0;
        let pixel = image.get_pixel(
            ((uv.x.rem_euclid(1.0) * image.width() as f32) as u32).min(image.width() - 1),
            ((uv.y.rem_euclid(1.0) * image.height() as f32) as u32).min(image.height() - 1),
        );
        pixel.0[..3].iter().copied().max().unwrap_or(0) > 50
    };
    let body = subset(&source, |t| !luminous(t))?;
    let lens = subset(&source, luminous)?;
    let mut area = 0.0;
    let mut center = Vec3::ZERO;
    let mut normal = Vec3::ZERO;
    for t in lens.indices.chunks_exact(3) {
        let p: Vec<_> = t
            .iter()
            .map(|&i| Vec3::from(lens.vertices[i as usize].pos))
            .collect();
        let cross = (p[1] - p[0]).cross(p[2] - p[0]);
        let a = cross.length();
        area += a;
        center += (p[0] + p[1] + p[2]) * (a / 3.0);
        normal += cross;
    }
    anyhow::ensure!(
        area > 0.00001 && normal.length() > 0.00001,
        "source torch lens has no finite outward area"
    );
    center /= area;
    normal = normal.normalize();
    let rotate = Quat::from_rotation_arc(normal, Vec3::Z);
    let radius = lens
        .vertices
        .iter()
        .map(|v| (rotate * (Vec3::from(v.pos) - center)).truncate().length())
        .fold(0.0f32, f32::max);
    anyhow::ensure!(
        (0.02..0.05).contains(&radius),
        "source torch lens radius changed"
    );
    let model = Mat4::from_translation(Vec3::Z * 0.035)
        * Mat4::from_scale(Vec3::splat(0.015 / radius))
        * Mat4::from_quat(rotate)
        * Mat4::from_translation(-center);
    Ok([body.transformed(model), lens.transformed(model)])
}
#[cfg(test)]
mod tests {
    use super::*;
    fn inside_native_field(point: Vec3) -> bool {
        let half_height = -point.z * 30f32.to_radians().tan();
        point.z < -0.025
            && point.y.abs() < half_height * 0.95
            && point.x.abs() < half_height * (16.0 / 9.0) * 0.95
    }
    fn blocks_eye_segment(point: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
        let edge = b - a;
        let other = c - a;
        let h = point.cross(other);
        let determinant = edge.dot(h);
        if determinant.abs() < 1e-12 {
            return false;
        }
        let from = -a;
        let u = from.dot(h) / determinant;
        let v = point.dot(from.cross(edge)) / determinant;
        let t = other.dot(from.cross(edge)) / determinant;
        u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.00001 && t < 0.99999
    }
    #[test]
    fn packet_filter_end_exposes_all_twenty_stock_centers_in_native_view() {
        let mut scene = Vec::new();
        let props = HandProps::allocate(&mut scene).unwrap();
        let model = packet_pose(
            Vec3::ZERO,
            -Vec3::Z,
            Vec3::X,
            Vec3::Y,
            props.pack_min,
            props.pack_max,
        );
        let lid = packet_lid_pose(model, props.pack_min, props.pack_max);
        for (slot, transform) in [(props.body, model), (props.lid, lid)] {
            assert!(scene[slot]
                .vertices
                .iter()
                .all(|v| inside_native_field(transform.transform_point3(Vec3::from(v.pos)))));
        }
        for position in pack_positions(props.pack_min, props.pack_max) {
            let filter = model.transform_point3(position);
            assert!(inside_native_field(filter));
            for (slot, transform) in [(props.body, model), (props.lid, lid)] {
                for triangle in scene[slot].indices.chunks_exact(3) {
                    let p = |i| {
                        transform.transform_point3(Vec3::from(scene[slot].vertices[i as usize].pos))
                    };
                    assert!(
                        !blocks_eye_segment(filter, p(triangle[0]), p(triangle[1]), p(triangle[2])),
                        "source packet face hides a remaining cigarette"
                    );
                }
            }
        }
    }
    #[test]
    fn a_raised_cigarette_sits_at_centre_on_the_lower_third_line() {
        for tan_half in [0.36_f32, 0.65, 0.9] {
            let (model, breath) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, 1.0, tan_half, 0.0);
            let grip = model.transform_point3(Vec3::ZERO);
            let depth = -grip.z;
            // Normalised view coordinates: 0 centre, -1 bottom edge.
            assert!((grip.x / depth).abs() < 1e-4, "not centred: {grip:?}");
            assert!((grip.y / (depth * tan_half) + 1.0 / 3.0).abs() < 1e-4, "not on the lower third: {grip:?}");
            assert!(breath.distance(grip) < 1e-5, "the exhale leaves from where it was held");
        }
        // At rest it is off to the lower right, clear of the centre.
        let (rest, _) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, 0.0, 0.65, 0.0);
        assert!(rest.transform_point3(Vec3::ZERO).x > 0.05);
        // It lies almost along the line of sight: ten degrees up, a few to the left.
        let axis = rest.transform_vector3(Vec3::Z);
        assert!((axis.y.atan2(-axis.z).to_degrees() - 10.0).abs() < 0.5, "{axis:?}");
        assert!(((-axis.x).atan2(-axis.z).to_degrees() - 4.0).abs() < 0.5, "{axis:?}");
        // The ash tap moves it and puts it back.
        let (tapped, _) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, 0.0, 0.65, 0.4);
        assert!(tapped.transform_point3(Vec3::ZERO).distance(rest.transform_point3(Vec3::ZERO)) > 0.01);
        let (done, _) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, 0.0, 0.65, 1.0);
        assert!(done.transform_point3(Vec3::Z * 0.08).distance(rest.transform_point3(Vec3::Z * 0.08)) < 1e-4);
        // In the mouth it sits under the eye, inside the view, pointing ahead.
        let mouth = mouth_pose(Vec3::ZERO, -Vec3::Z, Vec3::Y);
        let (filter, tip) = (mouth.transform_point3(Vec3::ZERO), mouth.transform_point3(Vec3::Z * CIGARETTE_LENGTH_M));
        assert!(filter.x.abs() < 1e-5 && filter.y < 0.0 && tip.z < filter.z);
        assert!(inside_native_field(tip));
    }
    #[test]
    fn cigarette_puff_and_raised_source_lighter_remain_inside_native_view() {
        let mut scene = Vec::new();
        let props = HandProps::allocate(&mut scene).unwrap();
        for puff in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (model, _) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, puff, 0.65, 0.0);
            assert!(scene[props.cigarette]
                .vertices
                .iter()
                .all(|v| inside_native_field(model.transform_point3(Vec3::from(v.pos)))));
        }
        let (model, _) = cigarette_pose(Vec3::ZERO, -Vec3::Z, Vec3::X, Vec3::Y, 0.0, 0.65, 0.0);
        let nozzle =
            model.transform_point3(Vec3::Z * CIGARETTE_LENGTH_M) - Vec3::Y * FLAME_HEIGHT_M;
        let lighter = Mat4::from_rotation_translation(Quat::from_rotation_y(-0.3), nozzle);
        assert!(scene[props.lighter]
            .vertices
            .iter()
            .all(|v| inside_native_field(lighter.transform_point3(Vec3::from(v.pos)))));
        let flame = &scene[props.flame];
        assert!((flame.max.y - flame.min.y - FLAME_HEIGHT_M).abs() < 1e-6);
        assert!(flame.max.x - flame.min.x <= 0.00201);
        assert!(FLAME_LIGHT_HEIGHT_M > flame.min.y && FLAME_LIGHT_HEIGHT_M < flame.max.y);
    }
    #[test]
    fn source_cigarette_clips_tip_without_moving_filter_or_changing_diameter() {
        let source = cigarette_source().unwrap();
        let mut pose = source.vertices.clone();
        for burn in [1.0, 0.7, 0.1, 0.0] {
            let length = cigarette_length(burn);
            clipped_cigarette(&source, length, &mut pose);
            for (a, b) in source.vertices.iter().zip(&pose) {
                assert_eq!(&a.pos[..2], &b.pos[..2]);
                if a.pos[2] < 0.00001 {
                    assert_eq!(a.pos, b.pos);
                    assert_eq!(a.uv, b.uv);
                }
                assert!(b.pos[2] <= length + 0.00001);
                assert!(Vec2::from(b.uv).is_finite());
            }
        }
    }
    #[test]
    fn source_pack_has_twenty_nonoverlapping_inside_cigarettes() {
        let source = load("Items & Weapons/cigs_packet_1.glb")
            .unwrap()
            .transformed(Mat4::from_scale(Vec3::splat(0.084 / 0.153)));
        let positions = pack_positions(source.min, source.max);
        let radius = CIGARETTE_RADIUS_M;
        for (i, p) in positions.iter().enumerate() {
            assert!(p.x <= source.max.x && p.x - CIGARETTE_LENGTH_M >= source.min.x);
            assert!(p.y - radius >= source.min.y && p.y + radius <= source.max.y);
            assert!(p.z - radius >= source.min.z && p.z + radius <= source.max.z);
            for q in &positions[..i] {
                assert!(Vec2::new(p.y - q.y, p.z - q.z).length() >= 2.0 * radius);
            }
        }
    }
    #[test]
    fn source_torch_lens_is_partitioned_and_registered_to_existing_emitter() {
        let [body, lens] = load_hand_torch().unwrap();
        assert!(!body.indices.is_empty());
        assert!(!lens.indices.is_empty());
        assert!(lens.min.z > 0.030 && lens.max.z < 0.040);
        assert!((lens.max.x - lens.min.x - 0.030).abs() < 0.003);
        let source = load("Items & Weapons/flashlight_mp_1.glb").unwrap();
        assert_eq!(
            source.indices.len(),
            body.indices.len() + lens.indices.len()
        );
    }
    #[test]
    fn lighter_source_exists_only_at_the_visible_ignited_nozzle() {
        let mut view = View::default();
        let anchors = Anchors {
            lighter: Vec3::new(2.0, 3.0, 4.0),
            lighter_visible: true,
            ..Default::default()
        };
        assert!(lighter_light(view, anchors).is_none());
        view.lighter_flame = true;
        let light = lighter_light(view, anchors).unwrap();
        assert!(light
            .pos
            .abs_diff_eq(anchors.lighter + Vec3::Y * FLAME_LIGHT_HEIGHT_M, 1e-6));
        assert_eq!(light.source_radius, FLAME_RADIUS_M);
        assert!(light.radiant_intensity > 0.0);
        assert!(lighter_light(
            view,
            Anchors {
                lighter_visible: false,
                ..anchors
            }
        )
        .is_none());
    }
    #[test]
    fn held_paper_and_tobacco_cap_are_separate_surfaces() {
        let mut scene = Vec::new();
        let props = HandProps::allocate(&mut scene).unwrap();
        assert!(scene[props.cigarette]
            .vertices
            .iter()
            .all(|v| v.normal[2] < 0.99));
        assert!(scene[props.tobacco_cap]
            .vertices
            .iter()
            .all(|v| v.normal[2] > 0.99));
        assert_eq!(
            scene[props.cigarette].indices.len() + scene[props.tobacco_cap].indices.len(),
            scene[props.stock[0]].indices.len()
        );
    }
    #[test]
    fn source_allocation_has_a_complete_bounded_slot_set() {
        let mut scene = Vec::new();
        let props = HandProps::allocate(&mut scene).unwrap();
        assert_eq!(scene.len(), 29 + BUTT_CAPACITY);
        assert_eq!(props.stock.len(), 20);
        assert!(props.slots().all(|s| s < scene.len()));
    }
}
