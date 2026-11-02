//! The written order as a physical print. The support detail's teleprinter head sits in the
//! turret at the commander's left, under the roof above the traverse gearbox, and the detail's
//! own machine (the reviewed Enigma, with a head beside it) stands on the camp's orders table.
//! An order accepted on the map prints on the strip line by line, at whichever machine the
//! reader is nearer; the reader tears the strip off (E or the action button while looking at
//! it), reads it in hand, folds it into a pocket with X and holds it up again with O. The flat
//! order slip that used to hang over the world is gone: the paper is the only sheet.
//!
//! Geometry (turret-local metres): the head's box was chosen on a 5 cm occupancy map of the
//! baked turret mesh (2 November 2026): the traverse gearbox and flywheel (source parts
//! Turret_0137/0142) top out at y 1.99 under it, the roof underside is at y 2.30, the wall is
//! at x 1.0 at this height, and the commander stands at (0.564, floor, -0.423) with his left
//! shoulder near x 0.78. The strip hangs 0.22 m from the slot and ends above the fixture at
//! y 1.83 (Turret_0199). `tests::the_head_and_strip_stand_clear_of_the_turret` checks the
//! boxes against the source triangles when the asset root is present.
use super::*;
use serde::{Deserialize, Serialize};

pub const HEAD_MIN: Vec3 = Vec3::new(0.84, 2.06, -0.60);
pub const HEAD_MAX: Vec3 = Vec3::new(0.99, 2.22, -0.38);
/// The paper slot on the head's inboard face, where the strip leaves it.
pub const SLOT: Vec3 = Vec3::new(0.838, 2.09, -0.49);
pub const STRIP_WIDTH: f32 = 0.076;
pub const STRIP_LENGTH: f32 = 0.228;
const STRIP_THICKNESS: f32 = 0.0004;
/// Ink texture of the strip: the same 1:3 as the paper so the glyphs keep their shape.
pub const INK_WIDTH: u32 = crate::renderer::RECEIPT_INK_WIDTH;
pub const INK_HEIGHT: u32 = crate::renderer::RECEIPT_INK_HEIGHT;
/// Print cadence: an authored line time (a teleprinter printed about ten characters a second;
/// a 28-character line in 2.8 s would be truthful but slow to read, so the print runs at four
/// lines a second and says so here). Not a measured machine.
const LINE_SECONDS: f32 = 0.25;
pub const CHARS_PER_LINE: usize = crate::renderer::RECEIPT_CHARS_PER_LINE;
pub const MAX_LINES: usize = 40;
const REACH_M: f32 = 2.2;
const HOLD_DISTANCE: f32 = 0.165;
/// The camp's machine prints when the reader is on foot within this distance of the table.
const CAMP_PRINT_RADIUS_M: f32 = 40.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Site {
    #[default]
    Turret,
    Camp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Nothing printed; the strip is in the machine.
    #[default]
    Idle,
    /// Lines coming out at the site.
    Printing,
    /// All lines out; the strip hangs from the slot until torn off.
    Hanging,
    /// Torn off and in the reader's hands.
    Held,
    /// Folded into the pocket; O holds it up again.
    Pocket,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Event {
    #[default]
    None,
    LinePrinted,
    Finished,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Printer {
    pub title: String,
    pub heading: String,
    pub lines: Vec<String>,
    pub printed: usize,
    pub clock: f32,
    pub phase: Phase,
    pub at: Site,
    /// The camp's machine and head, cloned from the turret's when the camp is installed.
    #[serde(skip)]
    pub camp_slots: Option<[usize; 2]>,
}

/// The print lines of an order: the title, the small print, then the order's lines wrapped to
/// the strip, in the teleprinter's capitals, with a rule between paragraphs.
pub fn print_lines(order: &[String], first_beat: &str) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in order {
        for line in crate::ui::wrap_text(paragraph, CHARS_PER_LINE) {
            out.push(line.to_uppercase());
        }
        out.push(String::new());
    }
    if !first_beat.is_empty() {
        for line in crate::ui::wrap_text(first_beat, CHARS_PER_LINE) {
            out.push(line.to_uppercase());
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.truncate(MAX_LINES);
    out
}

impl Printer {
    pub fn begin(&mut self, title: &str, heading: &str, order: &[String], first_beat: &str, at: Site) {
        self.title = title.to_uppercase();
        self.heading = heading.to_uppercase();
        self.lines = print_lines(order, first_beat);
        self.printed = 0;
        self.clock = 0.0;
        self.phase = Phase::Printing;
        self.at = at;
    }
    pub fn tick(&mut self, dt: f32) -> Event {
        if self.phase != Phase::Printing {
            return Event::None;
        }
        self.clock += dt.max(0.0);
        if self.clock < LINE_SECONDS {
            return Event::None;
        }
        self.clock -= LINE_SECONDS;
        self.printed += 1;
        if self.printed >= self.lines.len() + 2 {
            self.printed = self.lines.len() + 2;
            self.phase = Phase::Hanging;
            return Event::Finished;
        }
        Event::LinePrinted
    }
    /// Lines on the paper so far, the title and the small print counted as the first two.
    pub fn lines_out(&self) -> usize {
        self.printed.min(self.lines.len() + 2)
    }
    /// How much of the strip has left the slot, 0 to 1.
    pub fn emerged(&self) -> f32 {
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Printing => (self.lines_out() as f32 + 1.0) / (self.lines.len() as f32 + 3.0),
            _ => 1.0,
        }
    }
    pub fn tear(&mut self) -> bool {
        if self.phase != Phase::Hanging {
            return false;
        }
        self.phase = Phase::Held;
        true
    }
    pub fn stow(&mut self) -> bool {
        if self.phase != Phase::Held {
            return false;
        }
        self.phase = Phase::Pocket;
        true
    }
    pub fn hold_up(&mut self) -> bool {
        if self.phase != Phase::Pocket {
            return false;
        }
        self.phase = Phase::Held;
        true
    }
    pub fn held(&self) -> bool {
        self.phase == Phase::Held
    }
    pub fn on_machine(&self) -> bool {
        matches!(self.phase, Phase::Printing | Phase::Hanging)
    }
    /// The ink the strip carries now.
    pub fn ink(&self) -> crate::renderer::ReceiptInk<'_> {
        crate::renderer::ReceiptInk { title: &self.title, heading: &self.heading, lines: &self.lines, printed: self.lines_out() }
    }
}

/// The head: a closed steel box with a slot plate and a key panel on its inboard face.
pub fn head_mesh() -> CpuMesh {
    let mut out = crate::prims::box_mesh(HEAD_MIN, HEAD_MAX, [52, 54, 50]);
    let add = |out: &mut CpuMesh, m: CpuMesh| {
        let base = out.vertices.len() as u32;
        out.vertices.extend(m.vertices);
        out.indices.extend(m.indices.into_iter().map(|i| i + base));
        out.min = out.min.min(m.min);
        out.max = out.max.max(m.max);
    };
    // The slot plate stands 2 mm proud of the inboard face around the paper slit.
    add(&mut out, crate::prims::box_mesh(
        Vec3::new(HEAD_MIN.x - 0.004, SLOT.y - 0.012, SLOT.z - STRIP_WIDTH * 0.5 - 0.008),
        Vec3::new(HEAD_MIN.x, SLOT.y + 0.012, SLOT.z + STRIP_WIDTH * 0.5 + 0.008), [38, 38, 36]));
    // Four keys below the slot: the commander's line to the detail (O / typed line, later).
    for n in 0..4 {
        let z = SLOT.z - 0.045 + n as f32 * 0.03;
        add(&mut out, crate::prims::box_mesh(
            Vec3::new(HEAD_MIN.x - 0.006, HEAD_MIN.y + 0.012, z - 0.010),
            Vec3::new(HEAD_MIN.x, HEAD_MIN.y + 0.030, z + 0.010), [70, 66, 58]));
    }
    out.roughness_factor = 0.62;
    out.metallic_factor = 0.55;
    out
}

/// The strip: a thin paper, its top edge at the local origin, hanging down local -Y, its
/// printed face local +Z. UV v runs 0 at the top (the first line) to 1 at the bottom.
pub fn strip_mesh() -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut quad = |points: [Vec3; 4], uvs: [[f32; 2]; 4]| {
        let normal = (points[1] - points[0]).cross(points[2] - points[0]).normalize();
        let tangent = (points[1] - points[0]).normalize();
        let first = vertices.len() as u32;
        for (p, uv) in points.into_iter().zip(uvs) {
            vertices.push(crate::gltf_load::Vertex { pos: p.to_array(), normal: normal.to_array(), uv, tangent: [tangent.x, tangent.y, tangent.z, 1.0] });
        }
        indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    };
    let (w, l, t) = (STRIP_WIDTH * 0.5, STRIP_LENGTH, STRIP_THICKNESS);
    let front = [Vec3::new(-w, -l, 0.0), Vec3::new(w, -l, 0.0), Vec3::new(w, 0.0, 0.0), Vec3::new(-w, 0.0, 0.0)];
    let uv = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    quad(front, uv);
    quad([front[3], front[2], front[1], front[0]].map(|p| p - Vec3::Z * t), [uv[3], uv[2], uv[1], uv[0]]);
    let mut mesh = CpuMesh::basic(vertices, indices, Some(crate::gltf_load::CpuImage::new(1, 1, vec![228, 224, 208, 255])),
        Vec3::new(-w, -l, -t), Vec3::new(w, 0.0, 0.0));
    mesh.roughness_factor = 0.96;
    mesh
}

/// The strip hanging from the slot in the turret: `emerged` of it out, the rest inside the head.
pub fn strip_model(turret: Mat4, emerged: f32) -> Mat4 {
    turret
        * Mat4::from_translation(SLOT + Vec3::Y * STRIP_LENGTH * (1.0 - emerged.clamp(0.0, 1.0)))
        * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
}

/// The strip read in hand: held close, a little below the gaze, tilted back like the map.
pub fn held_model(camera: &crate::camera::Camera) -> Mat4 {
    let forward = camera.forward();
    let right = camera.right();
    let up = right.cross(forward).normalize_or_zero();
    let frame = Mat4::from_cols(right.extend(0.0), up.extend(0.0), (-forward).extend(0.0),
        (camera.pos + forward * HOLD_DISTANCE - right * 0.004 + up * (STRIP_LENGTH * 0.5 - 0.012)).extend(1.0));
    frame * Mat4::from_rotation_x(-12_f32.to_radians())
}

fn ray_box(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = f32::INFINITY;
    for axis in 0..3 {
        let (o, d, lo, hi) = (origin[axis], dir[axis], min[axis], max[axis]);
        if d.abs() < 1e-6 {
            if o < lo || o > hi { return None; }
            continue;
        }
        let (mut a, mut b) = ((lo - o) / d, (hi - o) / d);
        if a > b { std::mem::swap(&mut a, &mut b); }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 { return None; }
    }
    Some(t0)
}

impl Game {
    /// The machine the reader is at: aboard, the turret's; on foot near the camp's table, the
    /// camp's; otherwise the turret's (the print waits in the Tiger).
    fn order_print_site(&self) -> Site {
        if self.mode == Mode::Foot {
            if let Some(anchors) = self.installed_camp_anchors() {
                if self.foot.pos.distance(anchors.orders) < CAMP_PRINT_RADIUS_M {
                    return Site::Camp;
                }
            }
        }
        Site::Turret
    }
    /// A mission's written order goes to the strip at once; the flat sheet never shows.
    pub(super) fn print_mission_order(&mut self) {
        let Some(m) = self.mission.as_mut() else { return };
        let title = m.mission.title.clone();
        let heading = m.mission.heading.clone().unwrap_or_else(|| "Kampfgruppe · Nachhut · schriftlicher Befehl".into());
        let order = m.mission.order.clone();
        let first = m.mission.beats.first().map(|b| format!("1  {}", b.title)).unwrap_or_default();
        m.order_printed();
        let at = self.order_print_site();
        self.order_printer.begin(&title, &heading, &order, &first, at);
        println!("[order] printing {} lines at the {:?} machine", self.order_printer.lines.len() + 2, at);
    }
    /// The strip's box in world space, for the hands.
    fn order_strip_box(&self) -> Option<(Mat4, Vec3, Vec3)> {
        if !self.order_printer.on_machine() {
            return None;
        }
        let emerged = self.order_printer.emerged();
        let (model, min, max) = match self.order_printer.at {
            Site::Turret => (self.player.turret_model(), Vec3::new(SLOT.x - 0.012, SLOT.y - STRIP_LENGTH * emerged, SLOT.z - STRIP_WIDTH * 0.5),
                Vec3::new(SLOT.x + 0.004, SLOT.y + 0.005, SLOT.z + STRIP_WIDTH * 0.5)),
            Site::Camp => {
                let root = self.camp_machine_root()?;
                (root, Vec3::new(SLOT.x - 0.012, SLOT.y - STRIP_LENGTH * emerged, SLOT.z - STRIP_WIDTH * 0.5),
                    Vec3::new(SLOT.x + 0.004, SLOT.y + 0.005, SLOT.z + STRIP_WIDTH * 0.5))
            }
        };
        Some((model, min, max))
    }
    /// The camp machine's frame: the head's turret-local box re-based onto the orders table so
    /// the slot stands at a reader's chest height, facing the approach from the camp's spawn.
    fn camp_machine_root(&self) -> Option<Mat4> {
        let anchors = self.installed_camp_anchors()?;
        let table_top = self.camp_table_top().unwrap_or(anchors.orders.y + 0.80);
        let toward = (anchors.spawn - anchors.orders).with_y(0.0).normalize_or_zero();
        let yaw = toward.x.atan2(toward.z);
        let origin = anchors.orders - toward * 0.55;
        // The head's box, authored in the turret frame, re-based so that its inboard face
        // (-X, the slot and keys) looks at the reader, who comes from `toward`, and its
        // underside sits on the table top; the strip hangs over the table's front edge.
        let local = Mat4::from_rotation_y(yaw + std::f32::consts::FRAC_PI_2)
            * Mat4::from_translation(Vec3::new(-HEAD_MIN.x - 0.30, table_top - origin.y - HEAD_MIN.y, -(HEAD_MIN.z + HEAD_MAX.z) * 0.5));
        Some(Mat4::from_translation(origin) * local)
    }
    fn camp_table_top(&self) -> Option<f32> {
        let camp = self.support_camp.as_ref()?;
        let part = camp.placement.parts.iter().find(|p| p.name == "orders.table")?;
        let top = camp.placement.triangles.iter().filter(|t| t.scene_mesh == part.scene_mesh)
            .flat_map(|t| t.vertices).map(|v| v.y).fold(f32::MIN, f32::max);
        top.is_finite().then_some(top)
    }
    pub fn order_printer_hover(&self, eye: Vec3, dir: Vec3) -> Option<String> {
        let (model, min, max) = self.order_strip_box()?;
        let inv = model.inverse();
        let o = inv.transform_point3(eye);
        let d = inv.transform_vector3(dir).normalize_or_zero();
        let t = ray_box(o, d, min, max)?;
        if t > REACH_M || !self.equipment_path_clear(eye, eye + dir * (t - 0.02), 0.02) {
            return None;
        }
        Some(match self.order_printer.phase {
            Phase::Printing => "ORDER PRINTING".into(),
            _ => "TEAR OFF THE ORDER".into(),
        })
    }
    pub fn order_printer_interact(&mut self, eye: Vec3, dir: Vec3) -> bool {
        if self.order_printer_hover(eye, dir).is_none() || self.order_printer.phase != Phase::Hanging {
            return false;
        }
        if self.carry_mesh.is_some() || self.held_item.is_some() || self.carried_tools() || self.map_held || self.repair_manual.held {
            return false;
        }
        if self.order_printer.tear() {
            let at = self.hands_pos();
            self.recorded(crate::audio::RecordedFx::PaperPage, -1.0, at);
            println!("[order] torn off and in hand");
        }
        true
    }
    /// X: the held strip into the pocket.
    pub fn stow_order_receipt(&mut self) -> bool {
        if self.order_printer.stow() {
            let at = self.hands_pos();
            self.recorded(crate::audio::RecordedFx::PaperPage, -3.0, at);
            return true;
        }
        false
    }
    /// O: the strip out of the pocket and up to read (the free hands rule of the map).
    pub fn hold_up_order_receipt(&mut self) -> bool {
        if !self.controlled_soldier_alive() || self.climb.active || self.carry_mesh.is_some() || self.held_item.is_some() || self.carried_tools() {
            return false;
        }
        if self.order_printer.hold_up() {
            self.set_map_held(false);
            self.repair_manual.set_held(false);
            let at = self.hands_pos();
            self.recorded(crate::audio::RecordedFx::PaperPage, -2.0, at);
            return true;
        }
        false
    }
    pub fn order_receipt_held(&self) -> bool {
        self.order_printer.held()
    }
    /// The strip is printing or hangs torn-off-able at a machine.
    pub fn order_strip_active(&self) -> bool {
        self.order_printer.on_machine()
    }
    pub fn order_print_phase(&self) -> Phase {
        self.order_printer.phase
    }
    /// Harness fixture (disclosed where used): the print finished at once and the strip torn
    /// off into the hands, as the reader would after waiting for it.
    pub fn debug_tear_order_strip(&mut self) -> bool {
        if self.order_printer.phase == Phase::Printing {
            self.order_printer.printed = self.order_printer.lines.len() + 2;
            self.order_printer.phase = Phase::Hanging;
        }
        self.order_printer.tear()
    }
    pub(super) fn tick_order_printer(&mut self, dt: f32) {
        if !self.controlled_soldier_alive() && self.order_printer.held() {
            self.order_printer.stow();
        }
        match self.order_printer.tick(dt) {
            Event::LinePrinted => {
                let at = self.order_machine_slot_world();
                self.recorded(crate::audio::RecordedFx::SwitchButton, -14.0, at);
            }
            Event::Finished => {
                let at = self.order_machine_slot_world();
                self.recorded(crate::audio::RecordedFx::SwitchButton, -8.0, at);
                println!("[order] print complete; the strip hangs at the {:?} machine", self.order_printer.at);
            }
            Event::None => {}
        }
    }
    fn order_machine_slot_world(&self) -> Vec3 {
        match self.order_printer.at {
            Site::Turret => self.player.turret_model().transform_point3(SLOT),
            Site::Camp => self.camp_machine_root().map_or(self.player.turret_model().transform_point3(SLOT), |m| m.transform_point3(SLOT)),
        }
    }
    /// Poses: the head and strip in the turret, the camp's machine on its table, the strip in
    /// hand. Called with the interior parts each frame.
    pub(super) fn drive_order_printer(&mut self, r: &mut Renderer) {
        let tm = self.player.turret_model();
        r.set_model(self.slots.order_printer_head, tm);
        r.set_visible(self.slots.order_printer_head, true);
        r.set_emissive(self.slots.order_printer_head, 0.0);
        if self.order_printer.camp_slots.is_none() && self.installed_camp_anchors().is_some() {
            let machine = r.clone_actor_mesh(self.slots.enigma);
            let head = r.clone_actor_mesh(self.slots.order_printer_head);
            self.order_printer.camp_slots = Some([machine, head]);
        }
        if let (Some([machine, head]), Some(root)) = (self.order_printer.camp_slots, self.camp_machine_root()) {
            r.set_model(head, root);
            r.set_visible(head, true);
            r.set_emissive(head, 0.0);
            // The Enigma beside the head on the table (0.33 m along it, clear of the head's
            // 0.22 m), its keyboard (the asset's +Z) turned toward the reader (-X here).
            let enigma = root * Mat4::from_translation(Vec3::new(HEAD_MIN.x + 0.08, HEAD_MIN.y, HEAD_MAX.z + 0.33))
                * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2);
            r.set_model(machine, enigma);
            r.set_visible(machine, true);
            r.set_emissive(machine, 0.0);
        }
        let on_machine = self.order_printer.on_machine();
        r.set_visible(self.slots.order_strip, on_machine);
        if on_machine {
            let frame = match self.order_printer.at {
                Site::Turret => tm,
                Site::Camp => self.camp_machine_root().unwrap_or(tm),
            };
            r.set_model(self.slots.order_strip, strip_model(frame, self.order_printer.emerged()));
            r.set_emissive(self.slots.order_strip, 0.0);
        }
        let held = self.order_printer.held() && self.controlled_soldier_alive();
        r.set_visible(self.slots.order_receipt, held);
        if held {
            r.camera.fov_y = crate::repair_manual::reading_fov(&r.camera);
            r.set_model(self.slots.order_receipt, held_model(&r.camera));
            r.set_emissive(self.slots.order_receipt, 0.0);
            r.optic_mask = 0.0;
            r.reticle = false;
            self.hide_p08(r);
            r.set_visible(self.slots.panzerfaust, false);
            r.set_visible(self.slots.panzerfaust_sight, false);
            r.set_visible(self.slots.binocular_prop, false);
        }
    }
    /// The strip's ink, repainted only when a line is added: before the scene, like the map.
    pub fn prepare_order_receipt(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.order_printer.phase == Phase::Idle {
            return;
        }
        let ink = self.order_printer.ink();
        renderer.render_order_receipt_ink(device, queue, [self.slots.order_strip, self.slots.order_receipt], &ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_order_prints_line_by_line_then_hangs_until_torn_and_pockets_on_x() {
        let mut p = Printer::default();
        p.begin("Order 10: Hagen ammunition yard", "Unterstützungstrupp", &["Elster: clear the yard, then load what you can carry.".into(), "Report at the turn.".into()], "1  The approach", Site::Turret);
        assert_eq!(p.phase, Phase::Printing);
        assert!(p.lines.iter().all(|l| l.chars().count() <= CHARS_PER_LINE && l == &l.to_uppercase()));
        assert_eq!(p.emerged(), 1.0 / (p.lines.len() as f32 + 3.0));
        assert!(!p.tear(), "nothing to tear while printing");
        let mut events = 0;
        let mut finished = false;
        for _ in 0..400 {
            match p.tick(0.05) {
                Event::LinePrinted => events += 1,
                Event::Finished => { finished = true; break; }
                Event::None => {}
            }
        }
        assert!(finished && events == p.lines.len() + 1, "{events} line events for {} lines", p.lines.len());
        assert_eq!(p.phase, Phase::Hanging);
        assert_eq!(p.emerged(), 1.0);
        assert!(p.tear() && p.held() && !p.on_machine());
        assert!(!p.hold_up(), "already in hand");
        assert!(p.stow() && p.phase == Phase::Pocket);
        assert!(p.hold_up() && p.held());
    }

    #[test]
    fn long_orders_are_cut_to_the_strip() {
        let order: Vec<String> = (0..30).map(|n| format!("Paragraph {n} of a long order that runs on for a while.")).collect();
        let lines = print_lines(&order, "");
        assert_eq!(lines.len(), MAX_LINES);
    }

    #[test]
    fn the_strip_emerges_from_the_slot_and_hangs_below_it() {
        let top = strip_model(Mat4::IDENTITY, 1.0).transform_point3(Vec3::ZERO);
        assert!((top - SLOT).length() < 1e-5);
        let bottom = strip_model(Mat4::IDENTITY, 1.0).transform_point3(Vec3::new(0.0, -STRIP_LENGTH, 0.0));
        assert!((bottom.y - (SLOT.y - STRIP_LENGTH)).abs() < 1e-5 && bottom.y > 1.83, "the strip ends above the fixture at y 1.83");
        let face = strip_model(Mat4::IDENTITY, 1.0).transform_vector3(Vec3::Z);
        assert!(face.x < -0.99, "the printed face looks inboard, toward the commander: {face}");
        let half = strip_model(Mat4::IDENTITY, 0.5).transform_point3(Vec3::new(0.0, -STRIP_LENGTH, 0.0));
        assert!((half.y - (SLOT.y - STRIP_LENGTH * 0.5)).abs() < 1e-5);
    }

    #[test]
    fn ray_box_hits_the_near_face_and_misses_beside_it() {
        let min = Vec3::new(0.0, 0.0, 0.0);
        let max = Vec3::new(1.0, 1.0, 1.0);
        assert!((ray_box(Vec3::new(-1.0, 0.5, 0.5), Vec3::X, min, max).unwrap() - 1.0).abs() < 1e-6);
        assert!(ray_box(Vec3::new(-1.0, 1.5, 0.5), Vec3::X, min, max).is_none());
        assert!(ray_box(Vec3::new(2.0, 0.5, 0.5), Vec3::X, min, max).is_none(), "the box is behind the ray");
    }

    /// The head and the hanging strip against the baked turret's source triangles.
    #[test]
    fn the_head_and_strip_stand_clear_of_the_turret() {
        let Ok(turret) = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_turret.glb") else {
            eprintln!("turret bake not present (KESSEL_ASSET_ROOT); clearance not checked here");
            return;
        };
        let boxes = [("head", HEAD_MIN - Vec3::splat(0.004), HEAD_MAX + Vec3::splat(0.004)),
            ("strip", Vec3::new(SLOT.x - 0.006, SLOT.y - STRIP_LENGTH, SLOT.z - STRIP_WIDTH * 0.5), Vec3::new(SLOT.x + 0.002, SLOT.y, SLOT.z + STRIP_WIDTH * 0.5))];
        for (name, min, max) in boxes {
            let hits = turret.indices.chunks(3).filter(|tri| {
                let p = tri.iter().map(|i| Vec3::from_array(turret.vertices[*i as usize].pos)).collect::<Vec<_>>();
                crate::building_kit::triangle_box_overlap((min + max) * 0.5, (max - min) * 0.5, &[p[0], p[1], p[2]])
            }).count();
            assert_eq!(hits, 0, "{name} box {min}..{max} meets {hits} turret triangles");
        }
    }
}
