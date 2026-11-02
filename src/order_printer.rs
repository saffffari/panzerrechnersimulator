//! The written order as a physical print, and the commander's line to the support detail. The
//! detail's page teleprinter head sits in the turret at the commander's left, under the roof
//! above the traverse gearbox, and the detail's own machine (the reviewed Enigma, with a head
//! beside it) stands on the camp's orders table. An order accepted on the map prints on a
//! 210 mm sheet line by line (a page teleprinter's paper width, 60 characters a line), at
//! whichever machine the reader is nearer; the sheet comes out of the slot leaning toward the
//! reader; he tears it off (E or the action button while looking at it), reads it in hand like
//! the map, folds it into a pocket with X and holds it up again with O. The head's keys (E
//! while looking at them) open a typed line to the detail ("Stab"): Enter sends it, the line
//! and the detail's answer print on the sheet and hang there until torn off. The flat order
//! slip that used to hang over the world is gone: the paper is the only sheet.
//!
//! Geometry (turret-local metres): the head's box was chosen on a 5 cm occupancy map of the
//! baked turret mesh (2 November 2026): the traverse gearbox and flywheel (source parts
//! Turret_0137/0142) top out at y 1.99 under it, the roof underside is at y 2.30, the wall is
//! at x 1.0 at this height, and the commander stands at (0.564, floor, -0.423) with his left
//! shoulder near x 0.78. The sheet leaves the slot at the head's top inboard edge and hangs
//! 0.15 m, leaning 25 degrees inboard so its face meets the commander's gaze from the cupola;
//! it ends at y 1.95, above the fixture at y 1.83 (Turret_0199).
//! `tests::the_head_and_strip_stand_clear_of_the_turret` checks the boxes against the source
//! triangles when the asset root is present.
use super::*;
use serde::{Deserialize, Serialize};

pub const HEAD_MIN: Vec3 = Vec3::new(0.84, 2.06, -0.60);
pub const HEAD_MAX: Vec3 = Vec3::new(0.99, 2.22, -0.38);
/// The paper slot along the head's top inboard edge, where the sheet leaves it.
pub const SLOT: Vec3 = Vec3::new(0.838, 2.205, -0.49);
/// The sheet: a page teleprinter's 210 mm paper, torn off at 150 mm.
pub const STRIP_WIDTH: f32 = 0.210;
pub const STRIP_LENGTH: f32 = 0.150;
const STRIP_THICKNESS: f32 = 0.0004;
/// How far the hanging sheet leans inboard from the vertical, toward the reader (radians).
pub const SHEET_LEAN: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// Ink texture of the sheet: the same 1.4:1 as the paper so the glyphs keep their shape.
pub const INK_WIDTH: u32 = crate::renderer::RECEIPT_INK_WIDTH;
pub const INK_HEIGHT: u32 = crate::renderer::RECEIPT_INK_HEIGHT;
/// Print cadence: an authored line time (a teleprinter printed about ten characters a second;
/// a 28-character line in 2.8 s would be truthful but slow to read, so the print runs at four
/// lines a second and says so here). Not a measured machine.
const LINE_SECONDS: f32 = 0.25;
pub const CHARS_PER_LINE: usize = crate::renderer::RECEIPT_CHARS_PER_LINE;
pub const MAX_LINES: usize = 20;
const REACH_M: f32 = 2.2;
/// Read in hand at the map's distance: the 210 mm sheet spans two thirds of the frame.
const HOLD_DISTANCE: f32 = 0.19;
/// The torn-off sheet rises into the reading overlay over this time, and sinks when pocketed.
const RISE_SECONDS: f32 = 0.25;
/// The camp's machine prints when the reader is on foot within this distance of the table.
const CAMP_PRINT_RADIUS_M: f32 = 40.0;
const ENTRY_MAX_CHARS: usize = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Site {
    #[default]
    Turret,
    Camp,
}

/// The strip on the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Nothing printed; the paper is in the machine.
    #[default]
    Idle,
    /// Lines coming out.
    Printing,
    /// All lines out; the strip hangs from the slot until torn off.
    Hanging,
}

/// The strip in the reader's keeping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    #[default]
    Held,
    Pocket,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Event {
    #[default]
    None,
    LinePrinted,
    Finished,
}

/// One printed paper: what it says and how much of it is out.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Strip {
    pub title: String,
    pub heading: String,
    pub lines: Vec<String>,
    pub printed: usize,
}

impl Strip {
    /// Lines on the paper so far, the title and the small print counted as the first two.
    pub fn lines_out(&self) -> usize {
        self.printed.min(self.lines.len() + 2)
    }
    pub fn complete(&self) -> bool {
        self.printed >= self.lines.len() + 2
    }
    pub fn ink(&self) -> crate::renderer::ReceiptInk<'_> {
        crate::renderer::ReceiptInk { title: &self.title, heading: &self.heading, lines: &self.lines, printed: self.lines_out() }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Printer {
    pub machine: Strip,
    pub phase: Phase,
    pub clock: f32,
    pub at: Site,
    pub hand: Option<Strip>,
    pub hand_phase: Hand,
    /// The typed line to the detail: open at the head's keys, sent with Enter.
    pub typing: bool,
    pub entry: String,
    /// The held sheet's rise into the reading overlay, 0 (pocketed or none) to 1 (up).
    #[serde(default)]
    pub rise: f32,
    /// The camp's machine and head, cloned from the turret's when the camp is installed.
    #[serde(skip)]
    pub camp_slots: Option<[usize; 2]>,
}

/// The print lines of an order: the order's lines wrapped to the strip in the teleprinter's
/// capitals, a blank between paragraphs, the first beat last.
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
    /// A new paper on the machine (whatever hung there is lost, as a strip left in a machine is).
    pub fn begin(&mut self, title: &str, heading: &str, order: &[String], first_beat: &str, at: Site) {
        self.machine = Strip { title: title.to_uppercase(), heading: heading.to_uppercase(), lines: print_lines(order, first_beat), printed: 0 };
        self.clock = 0.0;
        self.phase = Phase::Printing;
        self.at = at;
    }
    /// More lines onto the paper in the machine: the print resumes where it stopped.
    pub fn append(&mut self, paragraphs: &[String]) {
        if self.phase == Phase::Idle {
            return;
        }
        let mut more = print_lines(paragraphs, "");
        if !self.machine.lines.is_empty() && !more.is_empty() {
            self.machine.lines.push(String::new());
        }
        self.machine.lines.append(&mut more);
        self.machine.lines.truncate(MAX_LINES);
        if !self.machine.complete() {
            self.phase = Phase::Printing;
        }
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
        self.machine.printed += 1;
        if self.machine.complete() {
            self.machine.printed = self.machine.lines.len() + 2;
            self.phase = Phase::Hanging;
            return Event::Finished;
        }
        Event::LinePrinted
    }
    /// How much of the strip has left the slot, 0 to 1.
    pub fn emerged(&self) -> f32 {
        match self.phase {
            Phase::Idle => 0.0,
            Phase::Printing => (self.machine.lines_out() as f32 + 1.0) / (self.machine.lines.len() as f32 + 3.0),
            Phase::Hanging => 1.0,
        }
    }
    /// The hanging strip into the hands; a strip already in the pocket is thrown away for it.
    pub fn tear(&mut self) -> bool {
        if self.phase != Phase::Hanging {
            return false;
        }
        self.hand = Some(std::mem::take(&mut self.machine));
        self.hand_phase = Hand::Held;
        self.phase = Phase::Idle;
        true
    }
    pub fn stow(&mut self) -> bool {
        if !self.held() {
            return false;
        }
        self.hand_phase = Hand::Pocket;
        true
    }
    pub fn hold_up(&mut self) -> bool {
        if self.hand.is_none() || self.hand_phase != Hand::Pocket {
            return false;
        }
        self.hand_phase = Hand::Held;
        true
    }
    pub fn held(&self) -> bool {
        self.hand.is_some() && self.hand_phase == Hand::Held
    }
    pub fn on_machine(&self) -> bool {
        matches!(self.phase, Phase::Printing | Phase::Hanging)
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
    // The slot plate: a lip 4 mm proud of the top inboard edge, the paper slit along it.
    add(&mut out, crate::prims::box_mesh(
        Vec3::new(HEAD_MIN.x - 0.006, HEAD_MAX.y - 0.020, HEAD_MIN.z),
        Vec3::new(HEAD_MIN.x + 0.010, HEAD_MAX.y + 0.004, HEAD_MAX.z), [38, 38, 36]));
    // Four keys below the slot: the commander's line to the detail.
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

/// The sheet hanging from the slot in the turret: `emerged` of it out, the rest inside the
/// head; the hanging part leans `SHEET_LEAN` inboard, its printed face toward the reader.
pub fn strip_model(turret: Mat4, emerged: f32) -> Mat4 {
    let out = STRIP_LENGTH * emerged.clamp(0.0, 1.0);
    // The sheet's own frame: top edge at the origin, hanging -Y, face +Z (toward -X in the
    // turret after the quarter turn). The unprinted part sits in the slot; the part that is
    // out leans away from the vertical about the slot's axis.
    turret
        * Mat4::from_translation(SLOT)
        * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2)
        * Mat4::from_rotation_x(-SHEET_LEAN)
        * Mat4::from_translation(Vec3::Y * (STRIP_LENGTH - out))
}

/// The axis-aligned box of the hanging sheet in the machine's frame (for the hands and tests).
pub fn hanging_sheet_box(emerged: f32) -> (Vec3, Vec3) {
    let out = STRIP_LENGTH * emerged.clamp(0.0, 1.0);
    let (s, c) = SHEET_LEAN.sin_cos();
    (Vec3::new(SLOT.x - out * s - 0.004, SLOT.y - out * c, SLOT.z - STRIP_WIDTH * 0.5),
        Vec3::new(SLOT.x + 0.004, SLOT.y + 0.004, SLOT.z + STRIP_WIDTH * 0.5))
}

/// The sheet read in hand: held like the map, a little below the gaze, tilted back.
pub fn held_model(camera: &crate::camera::Camera) -> Mat4 {
    let forward = camera.forward();
    let right = camera.right();
    let up = right.cross(forward).normalize_or_zero();
    let frame = Mat4::from_cols(right.extend(0.0), up.extend(0.0), (-forward).extend(0.0),
        (camera.pos + forward * HOLD_DISTANCE - right * 0.006 + up * (STRIP_LENGTH * 0.5 - 0.004)).extend(1.0));
    frame * Mat4::from_rotation_x(-15_f32.to_radians())
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

/// What the hands find at a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Strip,
    Keys,
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
        println!("[order] printing {} lines at the {:?} machine", self.order_printer.machine.lines.len() + 2, at);
    }
    /// The frame of the machine the print is at.
    fn order_machine_frame(&self, at: Site) -> Mat4 {
        match at {
            Site::Turret => self.player.turret_model(),
            Site::Camp => self.camp_machine_root().unwrap_or_else(|| self.player.turret_model()),
        }
    }
    /// The machine in reach of the hands: the turret's when aboard, the camp's when on foot by it.
    fn machine_in_reach(&self) -> Option<Site> {
        match self.mode {
            Mode::Tank => Some(Site::Turret),
            Mode::Foot => {
                let anchors = self.installed_camp_anchors()?;
                (self.foot.pos.distance(anchors.orders) < 6.0).then_some(Site::Camp)
            }
        }
    }
    /// What the crosshair rests on at the machine in reach: the hanging strip, or the keys.
    fn order_machine_pick(&self, eye: Vec3, dir: Vec3) -> Option<(Site, Pick)> {
        let site = self.machine_in_reach()?;
        let inv = self.order_machine_frame(site).inverse();
        let o = inv.transform_point3(eye);
        let d = inv.transform_vector3(dir).normalize_or_zero();
        let mut best: Option<(f32, Pick)> = None;
        if self.order_printer.on_machine() && self.order_printer.at == site {
            let (min, max) = hanging_sheet_box(self.order_printer.emerged());
            if let Some(t) = ray_box(o, d, min, max) { best = Some((t, Pick::Strip)); }
        }
        let keys_min = Vec3::new(HEAD_MIN.x - 0.012, HEAD_MIN.y, HEAD_MIN.z);
        let keys_max = Vec3::new(HEAD_MAX.x, HEAD_MAX.y, HEAD_MAX.z);
        if let Some(t) = ray_box(o, d, keys_min, keys_max) {
            if best.is_none_or(|b| t < b.0) { best = Some((t, Pick::Keys)); }
        }
        let (t, pick) = best?;
        if t > REACH_M || !self.equipment_path_clear(eye, eye + dir * (t - 0.02), 0.02) {
            return None;
        }
        Some((site, pick))
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
        if self.order_printer.typing {
            return Some("LINE TO STAB  ·  ENTER SENDS, ESC CANCELS".into());
        }
        let (_, pick) = self.order_machine_pick(eye, dir)?;
        Some(match (pick, self.order_printer.phase) {
            (Pick::Strip, Phase::Printing) => "ORDER PRINTING".into(),
            (Pick::Strip, _) => "TEAR OFF THE STRIP".into(),
            (Pick::Keys, _) => "KEY A LINE TO STAB".into(),
        })
    }
    pub fn order_printer_interact(&mut self, eye: Vec3, dir: Vec3) -> bool {
        if self.order_printer.typing {
            return true;
        }
        let Some((_, pick)) = self.order_machine_pick(eye, dir) else { return false };
        match pick {
            Pick::Strip => {
                if self.order_printer.phase != Phase::Hanging {
                    return true;
                }
                if self.carry_mesh.is_some() || self.held_item.is_some() || self.carried_tools() || self.map_held || self.repair_manual.held {
                    return true;
                }
                if self.order_printer.tear() {
                    let at = self.hands_pos();
                    self.recorded(crate::audio::RecordedFx::PaperPage, -1.0, at);
                    println!("[order] torn off and in hand");
                }
            }
            Pick::Keys => {
                if !self.controlled_soldier_alive() {
                    return true;
                }
                self.order_printer.typing = true;
                self.order_printer.entry.clear();
                let at = self.hands_pos();
                self.recorded(crate::audio::RecordedFx::SwitchButton, -4.0, at);
                println!("[order] line to Stab open");
            }
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
    /// The held sheet's rise into the overlay, 0 to 1.
    pub fn order_sheet_rise(&self) -> f32 {
        self.order_printer.rise
    }
    /// A torn-off sheet is in the reader's keeping, in hand or in the pocket.
    pub fn order_sheet_in_hand(&self) -> bool {
        self.order_printer.hand.is_some()
    }
    /// The strip is printing or hangs torn-off-able at a machine.
    pub fn order_strip_active(&self) -> bool {
        self.order_printer.on_machine()
    }
    pub fn order_print_phase(&self) -> Phase {
        self.order_printer.phase
    }
    /// Harness fixture (disclosed where used): the commander's head turned to the machine at
    /// his left so a still shows the head and the strip.
    pub fn debug_look_at_order_machine(&mut self) {
        // The commander's gaze is an offset from the turret's forward (as the loader's helper
        // solves it): yaw and pitch of the slot from the eye, less the turret's own.
        let eye = self.station_eye(Station::Commander);
        let target = self.player.turret_model().transform_point3(SLOT);
        let to = (target - eye).normalize_or_zero();
        let tf = self.player.turret_forward();
        let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.head_yaw = wrap(to.x.atan2(-to.z) - tf.x.atan2(-tf.z));
        self.head_pitch = (to.y.clamp(-0.999, 0.999).asin() - tf.y.clamp(-0.999, 0.999).asin()).clamp(-1.2, 1.0);
        println!("[order] STAGED look at the machine: yaw {:.2} pitch {:.2}", self.head_yaw, self.head_pitch);
    }
    /// Harness fixture (disclosed where used): the print finished at once and the strip torn
    /// off into the hands, as the reader would after waiting for it.
    pub fn debug_tear_order_strip(&mut self) -> bool {
        if self.order_printer.phase == Phase::Printing {
            self.order_printer.machine.printed = self.order_printer.machine.lines.len() + 2;
            self.order_printer.phase = Phase::Hanging;
        }
        self.order_printer.tear()
    }
    /// Harness fixture (disclosed where used): a sample order printed in full at the turret's
    /// machine and torn off into the hands, so a review can show the sheet without a mission.
    pub fn debug_hold_sample_order(&mut self) -> bool {
        let order = [
            "Kampfgruppe Elster holds the southern crossing until relieved. The bridge stays passable for the column.".to_string(),
            "Enemy armour reported moving west of the river at first light. Engage only from cover; do not pursue.".to_string(),
            "Report by teleprinter on reaching the crossing and on every contact.".to_string(),
        ];
        self.repair_manual.set_held(false);
        self.order_printer.begin("Befehl Nr. 7", "Kampfgruppe · Nachhut · schriftlicher Befehl", &order, "1  To the crossing", Site::Turret);
        while self.order_printer.phase == Phase::Printing {
            self.order_printer.tick(LINE_SECONDS);
        }
        self.order_printer.tear()
    }
    /// Harness fixture: the sheet's rise set directly, for a still mid-rise.
    pub fn debug_set_order_sheet_rise(&mut self, rise: f32) {
        self.order_printer.rise = rise.clamp(0.0, 1.0);
    }

    // --- the typed line to the detail ------------------------------------------------------

    /// The line to Stab is open: keys go to text, not to the driver.
    pub fn stab_typing(&self) -> bool {
        self.order_printer.typing
    }
    /// The entry under the cursor while the line is open.
    pub fn stab_entry(&self) -> Option<&str> {
        self.order_printer.typing.then_some(self.order_printer.entry.as_str())
    }
    /// Keyboard routing while the line is open. Enter sends; Escape closes the line.
    pub fn stab_radio_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode;
        if !self.order_printer.typing {
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.order_printer.typing = false;
                self.order_printer.entry.clear();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = std::mem::take(&mut self.order_printer.entry);
                self.order_printer.typing = false;
                self.stab_transmit(&line);
            }
            KeyCode::Backspace => {
                self.order_printer.entry.pop();
            }
            _ => {
                if let Some(t) = text {
                    for ch in t.chars() {
                        if !ch.is_control() && self.order_printer.entry.chars().count() < ENTRY_MAX_CHARS {
                            self.order_printer.entry.push(ch);
                        }
                    }
                }
            }
        }
        true
    }
    /// What the detail's radio knows when the commander keys it: truth from the simulation.
    pub fn stab_radio_context(&self) -> crate::radio_llm::StabContext {
        let callsign = self.mission.as_ref().map(|m| m.mission.callsign.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "Elster".into());
        let (order_title, order_lines, beat, beats_done, beats_total) = match self.mission.as_ref() {
            Some(m) => {
                let report = m.report();
                let beats = report["beats"].as_array().cloned().unwrap_or_default();
                let done = beats.iter().filter(|b| b["status"] == "Done").count();
                let current = self.mission_current_beat().map(|(n, title)| format!("{n}  {title}"));
                (Some(m.mission.title.clone()), m.mission.order.clone(), current, done, beats.len())
            }
            None => (None, Vec::new(), None, 0, 0),
        };
        let stock_lines = self.expedition_stock().map(|s| s.lines().to_vec()).unwrap_or_default();
        let site_notice = self.theatre.as_ref().map(|t| t.notice.clone()).unwrap_or_default();
        let camp = self.expedition_state().map(|e| {
            let p = e.camp.position;
            format!("site {} at ({:.0}, {:.0})", e.camp.site.0, p.x, p.z)
        }).unwrap_or_else(|| "none".into());
        let p = if self.mode == Mode::Foot { self.foot.pos } else { self.player.pos };
        crate::radio_llm::StabContext {
            callsign,
            order_title,
            order_lines,
            beat,
            beats_done,
            beats_total,
            stock_lines,
            site_notice,
            camp,
            position: format!("({:.0}, {:.0}), {}", p.x, p.z, if self.mode == Mode::Foot { "on foot" } else { "aboard" }),
            clock: if self.night { "night".into() } else { "day".into() },
            weather: if self.darkness > 0.5 { "dark".into() } else { "winter overcast".into() },
        }
    }
    /// Key the detail: the line prints on the strip at once; the answer prints when it comes.
    pub fn stab_transmit(&mut self, text: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let ctx = self.stab_radio_context();
        let callsign = ctx.callsign.clone();
        let net = self.stab_net.get_or_insert_with(crate::radio_llm::RadioLlm::new);
        let id = net.submit_stab(text, ctx);
        let at = self.order_print_site();
        let line = format!("{callsign}: {text}");
        if self.order_printer.on_machine() && self.order_printer.at == at && self.order_printer.machine.title == "FUNKSPRUCH" {
            self.order_printer.append(&[line]);
        } else {
            self.order_printer.begin("Funkspruch", &format!("{callsign} an Stab · Fernschreiber"), &[line], "", at);
        }
        println!("[order] #{id} line to Stab \"{text}\"");
        Some(id)
    }
    pub(super) fn tick_order_printer(&mut self, dt: f32) {
        if !self.controlled_soldier_alive() {
            if self.order_printer.held() { self.order_printer.stow(); }
            self.order_printer.typing = false;
        }
        // The detail's answers, printed as they come.
        let replies: Vec<(u64, String, String)> = self.stab_net.as_mut().map(|net| net.poll().into_iter()
            .flat_map(|r| r.replies.into_iter().map(move |reply| (r.request_id, reply.callsign, reply.text))).collect()).unwrap_or_default();
        for (id, callsign, text) in replies {
            println!("[order] #{id} {callsign} ▸ {text}");
            let line = format!("{callsign}: {text}");
            if self.order_printer.on_machine() {
                self.order_printer.append(&[line]);
            } else {
                let at = self.order_print_site();
                self.order_printer.begin("Funkspruch", "Stab an Elster · Fernschreiber", &[line], "", at);
            }
        }
        match self.order_printer.tick(dt) {
            Event::LinePrinted => {
                let at = self.order_machine_frame(self.order_printer.at).transform_point3(SLOT);
                self.recorded(crate::audio::RecordedFx::SwitchButton, -14.0, at);
            }
            Event::Finished => {
                let at = self.order_machine_frame(self.order_printer.at).transform_point3(SLOT);
                self.recorded(crate::audio::RecordedFx::SwitchButton, -8.0, at);
                println!("[order] print complete; the strip hangs at the {:?} machine", self.order_printer.at);
            }
            Event::None => {}
        }
        // The held sheet's rise into the overlay, and its sinking when pocketed or let go.
        let target = if self.order_printer.held() && self.controlled_soldier_alive() { 1.0 } else { 0.0 };
        let step = (dt.max(0.0) / RISE_SECONDS).min(1.0);
        self.order_printer.rise = if target > self.order_printer.rise {
            (self.order_printer.rise + step).min(target)
        } else {
            (self.order_printer.rise - step).max(target)
        };
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
            r.set_model(self.slots.order_strip, strip_model(self.order_machine_frame(self.order_printer.at), self.order_printer.emerged()));
            r.set_emissive(self.slots.order_strip, 0.0);
        }
        let held = self.order_printer.held() && self.controlled_soldier_alive();
        // Read top-down as an overlay (the manual's presentation), or in hand as a sheet in the
        // scene when the physical book is asked for.
        let overlay = crate::repair_manual::overlay_mode();
        r.set_visible(self.slots.order_receipt, held && !overlay);
        if held {
            if !overlay {
                r.camera.fov_y = crate::repair_manual::reading_fov(&r.camera);
                r.set_model(self.slots.order_receipt, held_model(&r.camera));
                r.set_emissive(self.slots.order_receipt, 0.0);
            }
            r.optic_mask = 0.0;
            r.reticle = false;
            self.hide_p08(r);
            r.set_visible(self.slots.panzerfaust, false);
            r.set_visible(self.slots.panzerfaust_sight, false);
            r.set_visible(self.slots.binocular_prop, false);
        }
    }
    /// The strips' ink, repainted only when a line is added: before the scene, like the map.
    pub fn prepare_order_receipt(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let machine = self.order_printer.on_machine().then(|| self.order_printer.machine.ink());
        let hand = self.order_printer.hand.as_ref().map(Strip::ink);
        if machine.is_none() && hand.is_none() {
            return;
        }
        renderer.render_order_strip_ink(device, queue, [self.slots.order_strip, self.slots.order_receipt], [machine, hand]);
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
        assert!(p.machine.lines.iter().all(|l| l.chars().count() <= CHARS_PER_LINE && l == &l.to_uppercase()));
        assert_eq!(p.emerged(), 1.0 / (p.machine.lines.len() as f32 + 3.0));
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
        assert!(finished && events == p.machine.lines.len() + 1, "{events} line events for {} lines", p.machine.lines.len());
        assert_eq!(p.phase, Phase::Hanging);
        assert_eq!(p.emerged(), 1.0);
        assert!(p.tear() && p.held() && !p.on_machine());
        assert!(p.machine.lines.is_empty(), "the machine is empty after the tear");
        assert!(!p.hold_up(), "already in hand");
        assert!(p.stow() && p.hand_phase == Hand::Pocket && !p.held());
        assert!(p.hold_up() && p.held());
    }

    #[test]
    fn a_reply_appends_to_the_hanging_line_and_the_print_resumes() {
        let mut p = Printer::default();
        p.begin("Funkspruch", "Elster an Stab", &["ELSTER: WHERE IS THE CAMP".into()], "", Site::Camp);
        while !matches!(p.tick(0.05), Event::Finished) {}
        assert_eq!(p.phase, Phase::Hanging);
        let before = p.machine.lines.len();
        p.append(&["Stab: Elster, Stab. The camp is at site 0. Out.".into()]);
        assert_eq!(p.phase, Phase::Printing);
        assert!(p.machine.lines.len() > before + 1, "a blank and the reply's lines");
        assert!(p.emerged() < 1.0);
        while !matches!(p.tick(0.05), Event::Finished) {}
        assert_eq!(p.phase, Phase::Hanging);
        assert!(p.tear());
        p.append(&["nothing".into()]);
        assert_eq!(p.phase, Phase::Idle, "an empty machine takes no appended lines");
    }

    #[test]
    fn long_orders_are_cut_to_the_strip() {
        let order: Vec<String> = (0..30).map(|n| format!("Paragraph {n} of a long order that runs on for a while.")).collect();
        let lines = print_lines(&order, "");
        assert_eq!(lines.len(), MAX_LINES);
    }

    #[test]
    fn the_sheet_leaves_the_slot_leaning_toward_the_reader_and_ends_above_the_fixture() {
        let top = strip_model(Mat4::IDENTITY, 1.0).transform_point3(Vec3::ZERO);
        assert!((top - SLOT).length() < 1e-5, "{top}");
        let bottom = strip_model(Mat4::IDENTITY, 1.0).transform_point3(Vec3::new(0.0, -STRIP_LENGTH, 0.0));
        assert!(bottom.y > 1.83 && bottom.y < SLOT.y - STRIP_LENGTH * 0.8, "hangs below the slot, above the fixture: {bottom}");
        assert!(bottom.x < SLOT.x - 0.04, "the bottom leans inboard toward the commander: {bottom}");
        let face = strip_model(Mat4::IDENTITY, 1.0).transform_vector3(Vec3::Z);
        assert!(face.x < -0.85 && face.y > 0.3, "the printed face looks inboard and up, to the cupola: {face}");
        let (min, max) = hanging_sheet_box(1.0);
        assert!(min.x <= bottom.x + 1e-4 && max.y >= SLOT.y && min.y <= bottom.y + 1e-4, "the box holds the sheet: {min} {max}");
        let half = strip_model(Mat4::IDENTITY, 0.5).transform_point3(Vec3::new(0.0, -STRIP_LENGTH, 0.0));
        assert!((half.y - (SLOT.y - 0.5 * STRIP_LENGTH * SHEET_LEAN.cos())).abs() < 1e-4, "{half}");
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
        let (sheet_min, sheet_max) = hanging_sheet_box(1.0);
        let boxes = [("head", HEAD_MIN - Vec3::splat(0.004), HEAD_MAX + Vec3::new(0.004, 0.008, 0.004)), ("sheet", sheet_min, sheet_max)];
        for (name, min, max) in boxes {
            let hits = turret.indices.chunks(3).filter(|tri| {
                let p = tri.iter().map(|i| Vec3::from_array(turret.vertices[*i as usize].pos)).collect::<Vec<_>>();
                crate::building_kit::triangle_box_overlap((min + max) * 0.5, (max - min) * 0.5, &[p[0], p[1], p[2]])
            }).count();
            assert_eq!(hits, 0, "{name} box {min}..{max} meets {hits} turret triangles");
        }
    }
}
