//! Solo play with a full living crew.
//!
//! The player is the commander of a five-man Tiger. The other four men work their stations:
//! the driver follows the commander's orders, the gunner lays on contacts and fires under the
//! standing order, the loader serves the breech at a human rate. Hits wound them
//! (`crate::first_aid`), and the player keeps them alive with the vehicle's first-aid box by
//! going to the man and working on him. A dead man's station goes dark: nothing is automated
//! away, the body stays in the seat until the player drags it aside, and the player has to go
//! there and do the job by hand.
//!
//! This module owns the crew's state and decisions; `game.rs` keeps the minimal hooks that read
//! them (drive/lay/trigger/breech authority, hover, station entry, wound recording). Nothing here
//! invents a hit: wounds come from the anatomical fragment/blast events of the live penetration
//! solver, burns from the live cabin fire, and bleeding to death from the first-aid time course.
use super::*;
use crate::first_aid::{self, AidBox, Casualty, Event as AidEvent, Hands, Procedure, ProcedureRun, Progress, Region, Wound, WoundKind};
use std::sync::Mutex;
use crate::lan::LanRole;
use serde_json::{json, Value};

/// Seconds the player spends dragging a dead man off his seat.
pub const CLEAR_BODY_S: f32 = 4.0;
/// Reach from the eye to a man or the aid box for hands-on work (metres).
const AID_REACH_M: f32 = 1.75;
/// Reach down into the hull front from the basket floor's edge: the driver and radio operator
/// sit low and forward, and a body there has slumped into the footwell.
const AID_REACH_HULL_M: f32 = 2.8;
const AID_CONE_COS: f32 = 0.84;
/// Seconds an intercom line stays legible.
const INTERCOM_S: f32 = 3.2;
/// The loader's stroke cadence at full performance: three strokes of `RAM_PER_PRESS_OK` seat a
/// round, which with the case fumble comes to about six seconds from the shot to the next round
/// seated, the rate of a practised man handling a 10 kg cartridge in a cramped turret. While he
/// works the round in he is holding it, so the off-station ram decay does not apply to him; a
/// hurt or hampered loader is slower, never stalled.
const LOADER_STROKE_S: f32 = 1.5;
/// The gunner's settle on a target before he fires (seconds at full performance).
const GUNNER_SETTLE_S: f32 = 0.55;
/// The driver's reaction to an order (seconds at full performance).
const DRIVER_REACT_S: f32 = 0.30;
/// Engagement range the gunner will open fire at on his own (metres).
const GUNNER_ENGAGE_M: f32 = 1800.0;
/// Turret-local point of the Kfz-Verbandkasten on the rear right wall, by the extinguisher
/// bracket. No mesh represents it yet (named in `docs/PHYSICAL_TRUTH_AUDIT.md`).
const AID_BOX_TURRET_LOCAL: Vec3 = Vec3::new(-0.70, 1.55, -0.58);
/// Burn severity per second in a fully developed cabin fire, hull seats.
const FIRE_BURN_PER_S: f32 = 0.02;
/// Turret-local spot where a man the player has displaced stands by: the rear of the basket
/// floor, not the cupola the player just left (the commander's post is the player's).
const STANDBY_TURRET_LOCAL: Vec3 = Vec3::new(0.05, crate::interior_rig::FLOOR_STAND_Y, -0.68);
/// The commander's post as `crew_live_model` places the standing figure.
const COMMANDER_POST_TURRET_LOCAL: Vec3 = Vec3::new(0.564, crate::interior_rig::FLOOR_STAND_Y, -0.423);
/// The loader is displaced only when the player is at the breech, within this of his post
/// (turret-local planar distance) or with ammunition in hand; walking the compartment leaves
/// him at work.
const LOADER_POST_TURRET_XZ: Vec2 = Vec2::new(-0.72, 0.12);
const LOADER_DISPLACE_M: f32 = 0.55;
/// Seconds between blood emissions from a bleeding man.
const BLEED_EMIT_S: f32 = 0.5;
/// The empties rack: this many upright slots along the curved inner rear wall of the turret.
pub const EMPTIES_RACK_SLOTS: usize = 12;
/// Seconds the loader spends turning, stowing the case upright and coming back, at full
/// performance; added to his reload cycle.
const STOW_S: f32 = 2.2;
/// A loader hurt below this stops stowing and lets the empties fall.
const STOW_PERF_MIN: f32 = 0.7;
/// The recess behind the rear-wall bands: probe height and the shelf's accepted height range
/// (turret-local metres), read off the probed wall profile.
const RACK_ROWS: [(f32, (f32, f32)); 2] = [(2.02, (1.55, 1.95)), (1.2, (0.88, 1.12))];
/// The commander's target designation lasts this long unless replaced or fulfilled.
const DESIGNATION_TTL_S: f32 = 90.0;
/// A second designation within this bearing of the first refines its range (radians, ~3 deg).
const DESIGNATION_REFINE_RAD: f32 = 0.05;
/// Farthest the commander's eye ray is taken to estimate a range (metres).
const DESIGNATION_RANGE_MAX_M: f32 = 2500.0;
/// The turret counts as on the designated bearing inside this (radians, ~1.1 deg).
const DESIGNATION_ON_RAD: f32 = 0.02;
/// Index of the turret's side escape hatch among the roof/side lids.
const ESCAPE_HATCH: usize = 3;
/// Seconds per case for the loader to take an empty and heave it out of the open escape
/// hatch, at full performance.
const DUMP_EACH_S: f32 = 1.8;
/// Seconds between repeats of an unanswered request.
const ASK_AGAIN_S: f32 = 30.0;
/// Loose cases within this of the loader's post get under his feet (turret-local planar m).
const CLUTTER_LOADER_M: f32 = 0.7;
/// Loose cases within this of the player's feet slow and are kicked (planar m).
const CLUTTER_FEET_M: f32 = 0.32;
/// Reach from the hands to a slot centre to count as placing the case in it.
const STOW_REACH_M: f32 = 0.45;
/// Hull-local point of the driver's seat used as the interior entry anchor.
const DRIVER_SEAT_ANCHOR_HULL: Vec3 = Vec3::new(0.535, 1.40, 1.55);
const DRIVER_SEAT_REACH_M: f32 = 1.95;

pub(super) fn role_index(role: CrewRole) -> usize {
    CrewRole::ALL
        .iter()
        .position(|r| *r == role)
        .expect("every crew role is listed")
}

pub(super) fn role_of_post(post: LanRole) -> CrewRole {
    match post {
        LanRole::Commander => CrewRole::Commander,
        LanRole::Gunner => CrewRole::Gunner,
        LanRole::Loader => CrewRole::Loader,
        LanRole::Driver => CrewRole::Driver,
        LanRole::Radio => CrewRole::Radioman,
    }
}

fn station_for_role(role: CrewRole) -> Station {
    match role {
        CrewRole::Commander => Station::Commander,
        CrewRole::Gunner => Station::Gunner,
        CrewRole::Loader => Station::Loader,
        CrewRole::Driver => Station::Driver,
        CrewRole::Radioman => Station::Radioman,
    }
}

fn region_of_part(part: crew_physics::CrewBodyPart) -> Region {
    use crew_physics::CrewBodyPart as P;
    match part {
        P::Head => Region::Head,
        P::Torso | P::Pelvis => Region::Torso,
        P::LeftUpperArm | P::LeftForearm | P::RightUpperArm | P::RightForearm => Region::Arm,
        P::LeftThigh | P::LeftShin | P::RightThigh | P::RightShin => Region::Leg,
    }
}

fn role_short(role: CrewRole) -> &'static str {
    match role {
        CrewRole::Commander => "Commander",
        CrewRole::Gunner => "Gunner",
        CrewRole::Loader => "Loader",
        CrewRole::Driver => "Driver",
        CrewRole::Radioman => "Radio",
    }
}

/// What the crosshair is resting on, among the crew's own bodies and kit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HoverAction {
    /// Work this procedure on this man.
    Treat(CrewRole, Procedure),
    /// Drag this dead man off his station.
    ClearBody(CrewRole),
    /// Work this procedure on the player himself, at the box.
    SelfAid(Procedure),
}

impl HoverAction {
    fn duration_s(self) -> f32 {
        match self {
            HoverAction::Treat(_, p) | HoverAction::SelfAid(p) => p.nominal_seconds(),
            HoverAction::ClearBody(_) => CLEAR_BODY_S,
        }
    }
}

/// Interned crosshair labels: the hover text path is `&'static str`, and the set of labels
/// here is bounded (roles x procedures x a few kit states), each leaked once.
fn intern(text: String) -> &'static str {
    static POOL: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut pool = POOL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(found) = pool.iter().find(|known| ***known == *text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.into_boxed_str());
    pool.push(leaked);
    leaked
}

fn need_word(procedure: Procedure) -> &'static str {
    match procedure {
        Procedure::Tourniquet => "ARTERIAL BLEED",
        Procedure::PressureDressing => "BLEEDING",
        Procedure::Splint => "FRACTURE",
        Procedure::BurnCare => "BURNED",
        Procedure::ChestSeal => "CHEST WOUND",
        Procedure::Airway => "OUT COLD",
        Procedure::Morphine => "IN PAIN",
    }
}

/// The offer under the crosshair: who, what is wrong, and what the hands would do; whether the
/// kit allows it (`Some(false)` issued, `Some(true)` improvised, `None` nothing left).
fn offer_label(role: CrewRole, procedure: Procedure, can: Option<bool>) -> &'static str {
    let need = need_word(procedure);
    let what = procedure.label();
    let text = if role == CrewRole::Commander {
        match can {
            Some(false) => format!("FIRST-AID BOX - YOU ARE {need} - {what} ON YOURSELF [E]"),
            Some(true) => format!("FIRST-AID BOX - YOU ARE {need} - IMPROVISED {what} ON YOURSELF [E]"),
            None => format!("FIRST-AID BOX - YOU ARE {need} - NOTHING LEFT FOR A {what}"),
        }
    } else {
        let who = if role == CrewRole::Radioman { "RADIO OP".to_owned() } else { role_short(role).to_uppercase() };
        match can {
            Some(false) => format!("{who} - {need} - {what} [E]"),
            Some(true) => format!("{who} - {need} - IMPROVISED {what} [E]"),
            None => format!("{who} - {need} - NOTHING LEFT FOR A {what}"),
        }
    };
    intern(text)
}

fn clear_label(role: CrewRole) -> &'static str {
    match role {
        CrewRole::Gunner => "GUNNER'S BODY - DRAG HIM OFF THE SEAT [HOLD E]",
        CrewRole::Loader => "LOADER'S BODY - DRAG HIM ASIDE [HOLD E]",
        CrewRole::Driver => "DRIVER'S BODY - DRAG HIM OFF THE SEAT [HOLD E]",
        CrewRole::Radioman => "RADIO OP'S BODY - DRAG HIM ASIDE [HOLD E]",
        CrewRole::Commander => "BODY - DRAG HIM ASIDE [HOLD E]",
    }
}

fn blocked_label(role: CrewRole) -> &'static str {
    match role {
        CrewRole::Gunner => "GUNNER DEAD IN THE SEAT - DRAG HIM OFF FIRST",
        CrewRole::Driver => "DRIVER DEAD IN THE SEAT - DRAG HIM OFF FIRST",
        CrewRole::Loader => "LOADER DEAD AT THE BREECH - DRAG HIM ASIDE FIRST",
        CrewRole::Radioman => "RADIO OP DEAD IN THE SEAT - DRAG HIM OFF FIRST",
        CrewRole::Commander => "BODY IN THE WAY - DRAG HIM ASIDE FIRST",
    }
}

/// The empties rack along the curved rear wall: upright spent cases, filled in order. The slot
/// poses are probed once from the turret's own interior triangles (the rear wall and the floor
/// or shelf under it), so a slot exists only where a case can really stand.
#[derive(Clone, Debug, Default)]
pub(super) struct EmptiesRack {
    /// Turret-local centre and rotation per slot, once probed; `None` where no stand was found.
    poses: Option<Vec<Option<(Vec3, Quat)>>>,
    /// Which casing (index into `Game::casings`) stands in each slot.
    pub slots: [Option<usize>; EMPTIES_RACK_SLOTS],
    /// Stowed, dumped and dropped counts for the receipts and the salvage paths.
    pub stowed_total: u32,
    pub dropped_total: u32,
    /// What the probe found per slot, for the receipt.
    pub probe_trace: Vec<String>,
}

impl EmptiesRack {
    pub fn pose(&self, slot: usize) -> Option<(Vec3, Quat)> {
        self.poses.as_ref()?.get(slot).copied().flatten()
    }

    pub fn count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// The next empty slot that has a real pose.
    pub fn free_slot(&self) -> Option<usize> {
        (0..EMPTIES_RACK_SLOTS).find(|&i| self.slots[i].is_none() && self.pose(i).is_some())
    }

    pub fn is_full(&self) -> bool {
        self.free_slot().is_none()
    }

    pub fn probed(&self) -> bool {
        self.poses.is_some()
    }
}

/// The commander's designation: where he looked when he pressed the middle mouse button.
#[derive(Clone, Copy, Debug)]
pub(super) struct Designation {
    /// The point under the view centre (world), from the eye ray's first hit.
    pub point: Vec3,
    /// The turret yaw (hull-relative) that lays the gun on it.
    pub bearing: f32,
    /// Estimated range: the ray's hit distance (a game shortcut; no rangefinder).
    pub range_m: f32,
    /// A living contact the ray struck, if any.
    pub enemy: Option<usize>,
    pub set_t: f32,
    pub refinements: u8,
    pub on_target_after_s: Option<f32>,
    pub turret_yaw_at_set: f32,
}

/// The commander's answer to the loader's request to dump the empties out of the side hatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DumpPermission {
    NotAsked,
    Asked,
    Approved,
    Refused,
}

#[derive(Clone, Debug)]
pub(super) struct CrewSolo {
    /// Full-crew solo is on (set by `enable_singleplayer_crew`).
    pub enabled: bool,
    /// One physiological record per `CrewRole::ALL` entry. The commander's is the player's own.
    pub men: Vec<Casualty>,
    pub aid_box: AidBox,
    /// Standing order: the gunner engages contacts on his own ("Feuer frei").
    pub fire_at_will: bool,
    /// A one-shot "Feuer!" from the commander, consumed by the next laid shot.
    pub fire_command: bool,
    pub gunner_target: Option<usize>,
    gunner_settle_s: f32,
    /// Set by the gunner's decision, consumed by the trigger path in `game.rs`.
    pub gunner_fire_pulse: bool,
    /// The commander's standing drive/steer order and what the driver is actually giving.
    driver_order: Vec2,
    driver_cmd: Vec2,
    driver_react_s: f32,
    loader_clock: f32,
    prev_breech_loaded: bool,
    /// Crosshair target among the crew this frame.
    pub hover: Option<HoverAction>,
    /// Action in progress: what, seconds held, where the player stood when he started.
    action: Option<(HoverAction, f32, Vec3)>,
    /// The procedure in the player's hands: the man, the run, where the player stood.
    procedure: Option<(CrewRole, ProcedureRun, Vec3)>,
    prev_hands_button: bool,
    /// The men last called in a triage line, so the call is made once per change.
    triage_called: Vec<CrewRole>,
    /// Every finished or abandoned procedure, for the receipt.
    pub procedures_done: Vec<Value>,
    /// Blood emission clocks, one per man.
    bleed_emit_t: [f32; 5],
    /// The empties rack and the loader's time away from the breech stowing a case.
    pub empties_rack: EmptiesRack,
    loader_busy_s: f32,
    /// The loader has a round in his hands at the breech this step (the ram does not slip back).
    pub loader_holding: bool,
    pub dump_permission: DumpPermission,
    ask_clock: f32,
    dump_clock: f32,
    /// The loader opened the side hatch to dump (and so closes it again).
    dump_hatch_open: bool,
    pub dumped_total: u32,
    prev_yes: bool,
    prev_no: bool,
    pub designation: Option<Designation>,
    prev_designate: bool,
    /// The gunner is slewing to the commander's designation this step (no contact of his own).
    slewing_to_designation: bool,
    /// A dead man's body has been dragged off his station.
    pub body_cleared: [bool; 5],
    intercom: Vec<(String, f32)>,
    prev_fire_order: bool,
    prev_command_lmb: bool,
    /// Receipt counters for the headless proof.
    pub shots_by_gunner: u32,
    pub loads_by_loader: u32,
    pub treatments: u32,
    pub log: Vec<Value>,
    time_s: f32,
}

impl Default for CrewSolo {
    fn default() -> Self {
        Self {
            enabled: false,
            men: CrewRole::ALL.iter().map(|_| Casualty::default()).collect(),
            aid_box: AidBox::FULL,
            fire_at_will: true,
            fire_command: false,
            gunner_target: None,
            gunner_settle_s: 0.0,
            gunner_fire_pulse: false,
            driver_order: Vec2::ZERO,
            driver_cmd: Vec2::ZERO,
            driver_react_s: 0.0,
            loader_clock: 0.0,
            prev_breech_loaded: true,
            hover: None,
            action: None,
            procedure: None,
            prev_hands_button: false,
            triage_called: Vec::new(),
            procedures_done: Vec::new(),
            bleed_emit_t: [0.0; 5],
            empties_rack: EmptiesRack::default(),
            loader_busy_s: 0.0,
            loader_holding: false,
            dump_permission: DumpPermission::NotAsked,
            ask_clock: 0.0,
            dump_clock: 0.0,
            dump_hatch_open: false,
            dumped_total: 0,
            prev_yes: false,
            prev_no: false,
            designation: None,
            prev_designate: false,
            slewing_to_designation: false,
            body_cleared: [false; 5],
            intercom: Vec::new(),
            prev_fire_order: false,
            prev_command_lmb: false,
            shots_by_gunner: 0,
            loads_by_loader: 0,
            treatments: 0,
            log: Vec::new(),
            time_s: 0.0,
        }
    }
}

impl CrewSolo {
    pub(super) fn man(&self, role: CrewRole) -> &Casualty {
        &self.men[role_index(role)]
    }

    fn man_mut(&mut self, role: CrewRole) -> &mut Casualty {
        &mut self.men[role_index(role)]
    }

    fn say(&mut self, line: impl Into<String>) {
        let line = line.into();
        println!("[crew] {line}");
        self.log.push(json!({"t": self.time_s, "intercom": line}));
        self.intercom.push((line, INTERCOM_S));
        if self.intercom.len() > 4 {
            self.intercom.remove(0);
        }
    }

    /// The most recent intercom line still on the air.
    pub fn intercom_line(&self) -> Option<&str> {
        self.intercom
            .iter()
            .rev()
            .find(|(_, left)| *left > 0.0)
            .map(|(line, _)| line.as_str())
    }
}

/// Posture offset for a hurt or unconscious man, applied about his seat so the figure slumps
/// forward and sideways instead of sitting at attention. A static pose with timing: the crew
/// GLBs carry no animation clips.
pub(super) fn slump_pose(man: &Casualty) -> Mat4 {
    if man.dead {
        return Mat4::IDENTITY;
    }
    let perf = man.performance();
    let (forward, side) = if !man.is_conscious() {
        (0.62, 0.34)
    } else if perf < 0.999 {
        (0.42 * (1.0 - perf), 0.18 * (1.0 - perf))
    } else {
        (0.0, 0.0)
    };
    if forward <= 0.0 && side <= 0.0 {
        return Mat4::IDENTITY;
    }
    let hip = Vec3::new(0.0, 0.55, 0.0);
    Mat4::from_translation(hip)
        * Mat4::from_rotation_x(forward)
        * Mat4::from_rotation_z(side)
        * Mat4::from_translation(-hip)
}

impl Game {
    fn solo_identity(&self) -> Option<CrewRole> {
        self.singleplayer_crew.map(|state| state.identity)
    }

    /// Full-crew solo is live: the singleplayer crew is active and the module is enabled.
    pub(super) fn crew_solo_active(&self) -> bool {
        self.singleplayer_crew_active && self.crew_solo.enabled
    }

    /// The station the player's own body works.
    fn solo_player_post(&self) -> LanRole {
        singleplayer_crew::body_post(singleplayer_crew::role_for_station(self.station))
    }

    /// Where a living AI crewman physically is. His own post, unless the player is working it:
    /// then he has moved to the commander's place under the cupola and stands by.
    pub(super) fn crew_solo_post(&self, role: CrewRole) -> LanRole {
        let own = singleplayer_crew::body_post(role);
        if self.mode == Mode::Tank && self.solo_player_post() == own {
            if role == CrewRole::Loader && !self.crew_solo_player_at_breech() {
                // Walking the compartment does not take the loader's post; the breech does.
                return own;
            }
            LanRole::Commander
        } else {
            own
        }
    }

    /// The player, on the interior walk, is working the breech: close to the loader's post or
    /// with a round or case in his hands.
    fn crew_solo_player_at_breech(&self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.grab
            || self.carry_mesh.is_some()
            || self.carried_casing.is_some()
            || self.lever_pulling
            || Vec2::new(self.walk_pos.x, self.walk_pos.z).distance(LOADER_POST_TURRET_XZ) < LOADER_DISPLACE_M
    }

    /// Where a displaced man stands relative to the commander's post (turret-local): a man the
    /// player pushed out of his seat does not stand in the player's own place under the cupola.
    pub(super) fn crew_solo_standby_offset(&self, casualty: CouchCrewCasualty) -> Option<Vec3> {
        if !self.crew_solo_active() || casualty.role != LanRole::Commander {
            return None;
        }
        let me = self.solo_identity().map(singleplayer_crew::body_post);
        if Some(casualty.identity) == me {
            return None;
        }
        Some(STANDBY_TURRET_LOCAL - COMMANDER_POST_TURRET_LOCAL)
    }

    /// World point of a living man's torso, where he actually is (his post, or the standby spot).
    pub(super) fn crew_solo_man_point(&self, role: CrewRole) -> Vec3 {
        let post = if Some(role) == self.solo_identity() { self.solo_player_post() } else { self.crew_solo_post(role) };
        let mut casualty = singleplayer_crew::casualty_identity(role);
        casualty.role = post;
        let torso = crew_live_model(&self.player, post).transform_point3(Vec3::new(0.0, crew_live_torso_y(post), 0.0));
        match self.crew_solo_standby_offset(casualty) {
            Some(offset) => torso + self.player.turret_model().transform_vector3(offset),
            None => torso,
        }
    }

    /// Alive and at his own station (not displaced by the player).
    fn crew_solo_at_own_post(&self, role: CrewRole) -> bool {
        self.crew_solo_active()
            && Some(role) != self.solo_identity()
            && singleplayer_crew::role_alive(&self.crew, role)
            && !self.crew_solo.man(role).dead
            && self.crew_solo_post(role) == singleplayer_crew::body_post(role)
    }

    /// Alive, conscious, fit enough to work, at his own station.
    fn crew_solo_working(&self, role: CrewRole) -> bool {
        self.crew_solo_at_own_post(role) && self.crew_solo.man(role).able()
    }

    /// The crew-side answer to `crew_ok`: the job at `role`'s station has somebody fit doing it,
    /// the player included when he is there.
    pub(super) fn crew_solo_role_ok(&self, role: CrewRole) -> bool {
        if !self.crew_solo_active() {
            return true;
        }
        let player_here = self.mode == Mode::Tank
            && singleplayer_crew::role_for_station(self.station) == role;
        if player_here {
            return self.crew_solo_player_able();
        }
        self.crew_solo_working(role)
    }

    fn crew_solo_player_able(&self) -> bool {
        self.solo_identity()
            .is_none_or(|me| singleplayer_crew::role_alive(&self.crew, me) && self.crew_solo.man(me).able())
    }

    /// The player's own performance, 0..1, scaling the work of his hands.
    pub(super) fn crew_solo_player_perf(&self) -> f32 {
        if !self.crew_solo_active() {
            return 1.0;
        }
        self.solo_identity()
            .map_or(1.0, |me| self.crew_solo.man(me).performance())
    }

    pub(super) fn crew_solo_ai_gunner_active(&self) -> bool {
        self.crew_solo_working(CrewRole::Gunner) && self.player.alive
    }

    /// The gunner has a target and owns the lay this frame.
    pub(super) fn crew_solo_ai_gunner_laying(&self) -> bool {
        self.crew_solo_ai_gunner_active()
            && (self.crew_solo.gunner_target.is_some() || self.crew_solo.slewing_to_designation)
    }

    pub(super) fn crew_solo_take_gunner_fire(&mut self) -> bool {
        std::mem::take(&mut self.crew_solo.gunner_fire_pulse)
    }

    /// A dead man still in the seat the player wants.
    pub(super) fn crew_solo_station_blocked(&self, station: Station) -> Option<CrewRole> {
        if !self.crew_solo_active() {
            return None;
        }
        // The interior walk is the compartment itself, never a seat a body can fill: the player
        // can always drop into it, step past the dead and drag them.
        if station == Station::Loader {
            return None;
        }
        let role = singleplayer_crew::role_for_station(station);
        if Some(role) == self.solo_identity() {
            return None;
        }
        let dead = !singleplayer_crew::role_alive(&self.crew, role) || self.crew_solo.man(role).dead;
        let body_present = self
            .couch_ragdolls
            .iter()
            .any(|body| body.casualty.identity == singleplayer_crew::body_post(role));
        (dead && body_present && !self.crew_solo.body_cleared[role_index(role)]).then_some(role)
    }

    // ── Decisions, once per simulation step ─────────────────────────────────────────────────

    /// Advance the crew: wounds, fire, the AI stations' decisions, the player's orders and his
    /// hands-on work. Runs before the vehicle authority reads drive/lay/trigger.
    pub(super) fn crew_solo_update(&mut self, input: &TankInput, dt: f32, eye: Vec3, look: Vec3) {
        if !self.crew_solo_active() {
            return;
        }
        self.crew_solo.time_s += dt;
        for line in &mut self.crew_solo.intercom {
            line.1 -= dt;
        }
        self.crew_solo.intercom.retain(|(_, left)| *left > -1.0);

        self.crew_solo_fire_exposure(dt);
        self.crew_solo_time_course(dt);
        self.crew_solo_orders(input);
        self.crew_solo_designate_input(input, eye, look);
        self.crew_solo_gunner_think(dt);
        self.crew_solo_driver_think(input, dt);
        self.crew_solo_hands(input, dt);
        self.crew_solo_empties_dump(input, dt);
    }

    fn crew_solo_fire_exposure(&mut self, dt: f32) {
        if !self.fire_active || self.fire <= 0.05 {
            return;
        }
        let me = self.solo_identity();
        for role in CrewRole::ALL {
            if !singleplayer_crew::role_alive(&self.crew, role) {
                continue;
            }
            let aboard = Some(role) != me || self.mode == Mode::Tank;
            if !aboard {
                continue;
            }
            let post = if Some(role) == me {
                self.solo_player_post()
            } else {
                self.crew_solo_post(role)
            };
            let compartment = match post {
                LanRole::Driver | LanRole::Radio => 1.0,
                LanRole::Commander if Some(role) == me && self.commander_outside => 0.15,
                _ => 0.6,
            };
            let rate = self.fire * FIRE_BURN_PER_S * compartment;
            self.crew_solo.man_mut(role).expose_to_fire(rate, dt);
        }
    }

    fn crew_solo_time_course(&mut self, dt: f32) {
        let me = self.solo_identity();
        for role in CrewRole::ALL {
            if !singleplayer_crew::role_alive(&self.crew, role) {
                continue;
            }
            self.crew_solo_bleed_onto_the_station(role, dt);
            let events = self.crew_solo.man_mut(role).advance(dt);
            for event in events {
                match event {
                    AidEvent::LostConsciousness => {
                        self.crew_solo.say(format!("{}: down, not answering.", role_short(role)));
                        if Some(role) == me {
                            // The player is out: nothing he can do until the loss is treated;
                            // untreated he dies of it below.
                            self.crew_ko_t = CREW_KO_DUR;
                        }
                    }
                    AidEvent::Woke => {
                        self.crew_solo.say(format!("{}: coming round.", role_short(role)));
                    }
                    AidEvent::Died => {
                        if Some(role) == me {
                            self.crew_solo_player_bled_out();
                        } else {
                            self.crew_solo_bled_out(role);
                        }
                    }
                }
            }
        }
        self.crew_solo_triage_call();
    }

    /// Two or more men needing care: the one nearest death is named first, once per change.
    fn crew_solo_triage_call(&mut self) {
        let me = self.solo_identity();
        let needing: Vec<CrewRole> = CrewRole::ALL
            .into_iter()
            .filter(|&role| Some(role) != me && singleplayer_crew::role_alive(&self.crew, role))
            .filter(|&role| self.crew_solo.man(role).procedure_needed().is_some())
            .collect();
        if needing.len() < 2 {
            self.crew_solo.triage_called.clear();
            return;
        }
        if needing == self.crew_solo.triage_called {
            return;
        }
        let men: Vec<&Casualty> = needing.iter().map(|&role| self.crew_solo.man(role)).collect();
        let order = first_aid::triage(&men);
        let line = order.first().map(|&first| {
            let count = match needing.len() { 2 => "Two", 3 => "Three", _ => "Four" };
            format!("{count} down. {} first.", role_short(needing[first]))
        });
        self.crew_solo.triage_called = needing;
        if let Some(line) = line {
            self.crew_solo.say(line);
        }
    }

    /// A bleeding man bleeds where he is: grains from the wound run down him onto the seat and
    /// the floor, more the faster he bleeds, through the same surface-blood transport as a hit.
    fn crew_solo_bleed_onto_the_station(&mut self, role: CrewRole, dt: f32) {
        let rate = self.crew_solo.man(role).total_bleed_rate_l_per_s();
        if rate <= 1.0e-5 {
            self.crew_solo.bleed_emit_t[role_index(role)] = 0.0;
            return;
        }
        let clock = &mut self.crew_solo.bleed_emit_t[role_index(role)];
        *clock += dt;
        if *clock < BLEED_EMIT_S {
            return;
        }
        *clock = 0.0;
        let aboard = Some(role) != self.solo_identity() || self.mode == Mode::Tank;
        if !aboard {
            return;
        }
        let post = if Some(role) == self.solo_identity() { self.solo_player_post() } else { self.crew_solo_post(role) };
        let mut casualty = singleplayer_crew::casualty_identity(role);
        casualty.role = post;
        // The wound's region decides where the grains leave him: a leg bleeds into the lap and
        // the floor, an arm beside him, the trunk and head down the front.
        let worst = self.crew_solo.man(role).wounds.iter()
            .filter(|w| w.bleed_rate_l_per_s() > 0.0)
            .max_by(|a, b| a.bleed_rate_l_per_s().total_cmp(&b.bleed_rate_l_per_s()))
            .map(|w| w.region);
        let model = crew_live_model(&self.player, post);
        let local = match worst {
            Some(Region::Leg) => Vec3::new(0.0, crew_live_torso_y(post) - 0.45, 0.12),
            Some(Region::Arm) => Vec3::new(0.18, crew_live_torso_y(post) - 0.15, 0.0),
            Some(Region::Head) => Vec3::new(0.0, crew_live_torso_y(post) + 0.30, 0.04),
            _ => Vec3::new(0.0, crew_live_torso_y(post), 0.08),
        };
        let mut point = model.transform_point3(local);
        if let Some(offset) = self.crew_solo_standby_offset(casualty) {
            point += self.player.turret_model().transform_vector3(offset);
        }
        // Grains per half second: a trickle at a dressed wound, a steady run at a femoral bleed.
        let count = ((rate * 60.0 * 14.0).round() as usize).clamp(4, 48);
        let seed = ((self.crew_solo.time_s * 53.0) as u32) ^ 0xB1EE_D000 ^ (role_index(role) as u32) << 8;
        self.spawn_cubic_blood(casualty, Some(point), Vec3::NEG_Y, Vec3::ZERO, 0.25, 0.55, seed, role_index(role), count);
    }

    fn crew_solo_bled_out(&mut self, role: CrewRole) {
        let post = self.crew_solo_post(role);
        let mut casualty = singleplayer_crew::casualty_identity(role);
        casualty.role = post;
        self.crew_ko_t = CREW_KO_DUR;
        self.crew_ko_world_dir = Vec3::NEG_Y;
        // The last of it: what was still in him runs out as he folds.
        let point = self.crew_solo_man_point(role) - Vec3::Y * 0.2;
        let seed = ((self.crew_solo.time_s * 53.0) as u32) ^ 0xDEAD_B100 ^ role_index(role) as u32;
        self.spawn_cubic_blood(casualty, Some(point), Vec3::NEG_Y, Vec3::ZERO, 0.6, 0.8, seed, role_index(role), 96);
        let wiped = self.record_physical_crew_casualties(&[casualty]);
        // A gentle collapse, not a fragment strike: the man folds where he sat.
        let seed = (self.crew_solo.time_s * 97.0) as u32 ^ 0x5EED_B10D ^ role_index(role) as u32;
        self.spawn_couch_ragdoll(casualty, Vec3::NEG_Y, 0.06, seed);
        self.crew_solo.say(format!("{}: gone.", role_short(role)));
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "died": role.name(), "cause": "bled out"}));
        println!("[panzerrechnersystem] CREWMAN DEAD — {} bled out at {}", role.name(), post.label());
        debug_assert!(!wiped, "an AI crewman's death never wipes the crew while the player lives");
    }

    fn crew_solo_player_bled_out(&mut self) {
        let Some(me) = self.solo_identity() else { return; };
        if let Some(person) = self.crew.iter_mut().find(|p| p.role == me) {
            person.ok = false;
        }
        self.player.alive = false;
        self.recompute_hp();
        self.cue_engine_death(0.7);
        self.death_cause = Some(DeathCause::CrewLost);
        self.death_replay = None;
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "died": "commander", "cause": "bled out"}));
        println!("[panzerrechnersystem] KILLED — you bled out at the {:?} station", self.station);
    }

    /// The commander's orders: K toggles the standing order; LMB buttoned up at the cupola with
    /// nothing under the crosshair is the fire command.
    fn crew_solo_orders(&mut self, input: &TankInput) {
        let toggle = input.fire_order && !self.crew_solo.prev_fire_order;
        self.crew_solo.prev_fire_order = input.fire_order;
        if toggle && self.player.alive {
            self.crew_solo.fire_at_will = !self.crew_solo.fire_at_will;
            let line = if self.crew_solo.fire_at_will {
                "Commander: fire at will."
            } else {
                "Commander: hold fire."
            };
            self.crew_solo.say(line);
        }
        let command_press = input.mg_coax && !self.crew_solo.prev_command_lmb;
        self.crew_solo.prev_command_lmb = input.mg_coax;
        if command_press
            && self.player.alive
            && self.mode == Mode::Tank
            && self.station == Station::Commander
            && !self.commander_outside
            && self.switch_t <= 0.0
            && self.hover.is_none()
            && self.hover_station.is_none()
            && !self.hover_hatch
            && !self.map_held
            && !self.compass_equipped
            && self.crew_solo_ai_gunner_active()
        {
            self.crew_solo.fire_command = true;
            self.crew_solo.say("Commander: fire!");
        }
    }

    fn crew_solo_gunner_think(&mut self, dt: f32) {
        if !self.crew_solo_ai_gunner_active() {
            self.crew_solo.gunner_target = None;
            self.crew_solo.gunner_settle_s = 0.0;
            self.crew_solo.gunner_fire_pulse = false;
            return;
        }
        let perf = self.crew_solo.man(CrewRole::Gunner).performance().max(0.15);
        // The sight line is tested from the muzzle: the TZF's objective sits in the mantlet,
        // and a ray from the eyepiece itself would stop in the Tiger's own armour.
        let eye = self.player.muzzle().0;
        let visible = |game: &Game, i: usize| {
            game.enemy_active.get(i).copied().unwrap_or(false)
                && game.enemies[i].alive
                && game.enemies[i].pos.distance(game.player.pos) <= GUNNER_ENGAGE_M
                && game.optical_los_clear(eye, game.enemies[i].center())
        };
        // The commander's designation first: a designated contact the gunner can see becomes
        // his target (the designation is then fulfilled); otherwise he slews to the designated
        // bearing and holds there, firing at nothing, until it expires or is replaced.
        self.crew_solo.slewing_to_designation = false;
        if let Some(d) = self.crew_solo.designation {
            if self.crew_solo.time_s - d.set_t > DESIGNATION_TTL_S {
                self.crew_solo.designation = None;
                self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "designation": "expired"}));
            } else if let Some(i) = d.enemy.filter(|&i| visible(self, i)) {
                if self.crew_solo.gunner_target != Some(i) {
                    self.crew_solo.gunner_target = Some(i);
                    self.crew_solo.gunner_settle_s = 0.0;
                }
                self.crew_solo.designation = None;
                self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "designation": "fulfilled", "enemy": i}));
            } else {
                self.crew_solo.gunner_target = None;
                self.crew_solo.gunner_fire_pulse = false;
                self.crew_solo.slewing_to_designation = true;
                let laid = self.campaign_ai_order_lay(d.point, DESIGNATION_ON_RAD);
                if laid && d.on_target_after_s.is_none() {
                    let after = self.crew_solo.time_s - d.set_t;
                    if let Some(live) = self.crew_solo.designation.as_mut() {
                        live.on_target_after_s = Some(after);
                    }
                    self.crew_solo.say("Gunner: on.");
                    self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "designation": "on_target",
                        "after_s": after, "bearing_rad": d.bearing, "range_m": d.range_m}));
                    println!("[crew] designation on target after {after:.1} s (bearing {:.1} deg, range {:.0} m)",
                        d.bearing.to_degrees(), d.range_m);
                }
                return;
            }
        }
        // Keep the man on his target while it lives and stays in view; otherwise the nearest.
        let keep = self.crew_solo.gunner_target.filter(|&i| visible(self, i));
        let target = keep.or_else(|| {
            (0..self.enemies.len())
                .filter(|&i| visible(self, i))
                .min_by(|&a, &b| {
                    self.enemies[a]
                        .pos
                        .distance(self.player.pos)
                        .total_cmp(&self.enemies[b].pos.distance(self.player.pos))
                })
        });
        if target != self.crew_solo.gunner_target {
            self.crew_solo.gunner_settle_s = 0.0;
            if let Some(i) = target {
                let range = self.enemies[i].pos.distance(self.player.pos);
                let to = self.enemies[i].pos - self.player.pos;
                let bearing = (to.x.atan2(to.z) - self.player.heading).rem_euclid(std::f32::consts::TAU);
                let oclock = ((bearing / std::f32::consts::TAU * 12.0).round() as i32).rem_euclid(12);
                let oclock = if oclock == 0 { 12 } else { oclock };
                self.crew_solo.say(format!(
                    "Gunner: tank, {oclock} o'clock, {}.",
                    (range / 50.0).round() as i32 * 50
                ));
            }
        }
        self.crew_solo.gunner_target = target;
        let Some(i) = target else {
            self.crew_solo.gunner_fire_pulse = false;
            return;
        };
        let center = self.enemies[i].center();
        let tolerance = 0.010 / perf;
        let laid = self.campaign_ai_order_lay(center, tolerance);
        // A hurt man's hands shake on the wheels: the lay wanders by a little, more the worse he is.
        let shake = 1.0 - perf;
        if shake > 0.0 {
            let t = self.crew_solo.time_s;
            self.lay_pitch = (self.lay_pitch + 0.006 * shake * (t * 3.1).sin()).clamp(-0.139, 0.261);
            self.lay_yaw += 0.004 * shake * (t * 2.3).cos();
        }
        if laid {
            self.crew_solo.gunner_settle_s += dt;
        } else {
            self.crew_solo.gunner_settle_s = 0.0;
        }
        let may_fire = self.crew_solo.fire_at_will || self.crew_solo.fire_command;
        let ready = laid
            && self.crew_solo.gunner_settle_s >= GUNNER_SETTLE_S / perf
            && matches!(self.breech, Breech::Loaded)
            && self.player_can_fire();
        if ready && may_fire {
            self.crew_solo.gunner_fire_pulse = true;
            self.crew_solo.gunner_settle_s = 0.0;
            self.crew_solo.fire_command = false;
            self.crew_solo.shots_by_gunner += 1;
            self.crew_solo.say("Gunner: firing.");
        } else {
            self.crew_solo.gunner_fire_pulse = false;
        }
    }

    fn crew_solo_driver_think(&mut self, input: &TankInput, dt: f32) {
        let takes_orders = self.mode == Mode::Tank
            && !matches!(self.station, Station::Loader | Station::Driver)
            && self.switch_t <= 0.0
            && self.activity.is_none()
            && self.player.alive;
        let order = if takes_orders {
            Vec2::new(input.drive, input.steer)
        } else {
            Vec2::ZERO
        };
        if order != self.crew_solo.driver_order {
            self.crew_solo.driver_order = order;
            self.crew_solo.driver_react_s = 0.0;
            if self.crew_solo_working(CrewRole::Driver) && order != Vec2::ZERO {
                let word = if order.x > 0.0 {
                    "Driver: forward."
                } else if order.x < 0.0 {
                    "Driver: back."
                } else if order.y > 0.0 {
                    "Driver: left."
                } else {
                    "Driver: right."
                };
                self.crew_solo.say(word);
            }
        }
        if !self.crew_solo_working(CrewRole::Driver) {
            // Nobody fit at the levers: the tank coasts to a stop whatever the commander orders.
            self.crew_solo.driver_cmd = Vec2::ZERO;
            return;
        }
        let perf = self.crew_solo.man(CrewRole::Driver).performance().max(0.15);
        self.crew_solo.driver_react_s += dt;
        if self.crew_solo.driver_react_s < DRIVER_REACT_S / perf {
            return;
        }
        let rate = 2.5 * perf * dt;
        let target = self.crew_solo.driver_order;
        let cmd = self.crew_solo.driver_cmd;
        self.crew_solo.driver_cmd = Vec2::new(
            cmd.x + (target.x - cmd.x).clamp(-rate, rate),
            cmd.y + (target.y - cmd.y).clamp(-rate, rate),
        );
    }

    /// Drive/steer authority in solo: `None` leaves the raw input to the player's own hands (he
    /// is at the levers, or the crew module is off); `Some` is the driver's output, zero when
    /// nobody fit sits there.
    pub(super) fn crew_solo_drive_command(&self) -> Option<(f32, f32)> {
        if !self.crew_solo_active() {
            return None;
        }
        if self.mode == Mode::Tank && self.station == Station::Driver {
            return None;
        }
        Some((self.crew_solo.driver_cmd.x, self.crew_solo.driver_cmd.y))
    }

    /// The loader serves the breech at his own rate when he is at it and fit.
    pub(super) fn crew_solo_service_breech(&mut self, dt: f32) {
        if !self.crew_solo_active() {
            return;
        }
        let loaded_now = matches!(self.breech, Breech::Loaded);
        if loaded_now && !self.crew_solo.prev_breech_loaded && self.crew_solo_at_own_post(CrewRole::Loader) {
            self.crew_solo.loads_by_loader += 1;
            self.crew_solo.say("Loader: up.");
        }
        self.crew_solo.prev_breech_loaded = loaded_now;
        self.crew_solo.loader_holding = false;
        if !self.crew_solo_working(CrewRole::Loader) || !self.player.alive || loaded_now {
            self.crew_solo.loader_clock = 0.0;
            return;
        }
        if self.crew_solo.loader_busy_s > 0.0 {
            // Turned away to the rack with the empty: no strokes at the breech meanwhile.
            self.crew_solo.loader_busy_s -= dt;
            return;
        }
        let perf = self.crew_solo.man(CrewRole::Loader).performance().max(0.15);
        // Empties rolling round his feet slow him: a quarter more per case, at most double.
        let underfoot = self.crew_solo_cases_near_turret_xz(LOADER_POST_TURRET_XZ, CLUTTER_LOADER_M);
        let clutter = (1.0 + 0.25 * underfoot as f32).min(2.0);
        self.crew_solo.loader_clock += dt;
        let stroke = self.crew_solo.loader_clock >= LOADER_STROKE_S * clutter / perf;
        if stroke {
            self.crew_solo.loader_clock = 0.0;
        }
        match self.breech {
            // Freeing the hot case is a timed fumble: it needs the step every frame.
            Breech::Spent => self.service_breech(false, dt),
            // Ramming is strokes against the ordinary ram decay the tank arm already applies
            // off-station; a second decay here would never let the round home.
            Breech::Empty => {
                self.crew_solo.loader_holding = true;
                if stroke {
                    self.service_breech(true, dt);
                }
            }
            Breech::Loaded => {}
        }
    }

    // ── The empties rack ────────────────────────────────────────────────────────────────────

    /// Probe the slot poses once from the turret's interior triangles. The rear wall carries
    /// curved bands with a recess behind them (back wall at about z -1.16, a shelf at about
    /// y 1.0); the cases stand upright in that offset space, behind the upper band. Rays fan
    /// out along the rear arc at the recess's height to find the back wall; each case stands a
    /// hand's width in from it on the shelf a downward ray finds, if the shelf is at recess
    /// height and an upward ray finds headroom for the case's full length. Slots with no real
    /// stand stay empty.
    fn crew_solo_probe_empties_rack(&mut self) {
        if self.crew_solo.empties_rack.probed() {
            return;
        }
        let models = self.fire_surface_models();
        let contacts = self.cabin_fire.source_contacts(&models);
        let turret = self.player.turret_model();
        let turret_inverse = turret.inverse();
        // A contact's point is in the struck frame's own coordinates; bring it to the turret's.
        let to_turret = |c: &surface_fire::Contact| turret_inverse.transform_point3(models[c.frame].transform_point3(c.point));
        let mut trace = Vec::with_capacity(EMPTIES_RACK_SLOTS * 2 + 14);
        // The rear wall's profile by height (turret-local), straight back along the centreline:
        // where the basket ends and the bustle overhang begins.
        for step in 0..14 {
            let y = 0.75 + step as f32 * 0.1;
            let from = turret.transform_point3(Vec3::new(0.0, y, -0.15));
            let to = turret.transform_point3(Vec3::new(0.0, y, -2.4));
            let hit = contacts.sweep_batch(&[[from, to]])[0].map(|c| to_turret(&c));
            trace.push(match hit {
                Some(p) => format!("profile y {y:.2}: rear surface at z {:.2}", p.z),
                None => format!("profile y {y:.2}: nothing within 2.4 m"),
            });
        }
        let span = 100.0_f32.to_radians();
        let mut best: Option<(usize, Vec<Option<(Vec3, Quat)>>, f32)> = None;
        for &(probe_y, shelf) in &RACK_ROWS {
        let origin_local = Vec3::new(0.0, probe_y, -0.55);
        let mut poses = Vec::with_capacity(EMPTIES_RACK_SLOTS);
        trace.push(format!("row at probe height {probe_y:.2}, shelf {:.2}..{:.2}", shelf.0, shelf.1));
        for i in 0..EMPTIES_RACK_SLOTS {
            let theta = -span * 0.5 + span * (i as f32 + 0.5) / EMPTIES_RACK_SLOTS as f32;
            let dir_local = Vec3::new(theta.sin(), 0.0, -theta.cos());
            let from = turret.transform_point3(origin_local);
            let to = turret.transform_point3(origin_local + dir_local * 2.0);
            let Some(wall) = contacts.sweep_batch(&[[from, to]])[0] else {
                trace.push(format!("{i}: no wall within 2 m"));
                poses.push(None);
                continue;
            };
            let wall_local = to_turret(&wall);
            let stand_xz = wall_local - dir_local * (SPENT_CASE_RADIUS + 0.03);
            // The floor or shelf under that spot, from just under the wall hit's height down.
            let top = turret.transform_point3(Vec3::new(stand_xz.x, wall_local.y, stand_xz.z));
            let bottom = turret.transform_point3(Vec3::new(stand_xz.x, shelf.0 - 0.2, stand_xz.z));
            let Some(floor) = contacts.sweep_batch(&[[top, bottom]])[0] else {
                trace.push(format!("{i}: wall at {:.2},{:.2},{:.2}; no surface below", wall_local.x, wall_local.y, wall_local.z));
                poses.push(None);
                continue;
            };
            let floor_local = to_turret(&floor);
            if floor_local.y < shelf.0 || floor_local.y > shelf.1 {
                trace.push(format!("{i}: surface at y {:.2} is not the recess shelf", floor_local.y));
                poses.push(None);
                continue;
            }
            let center = Vec3::new(stand_xz.x, floor_local.y + 0.01 + SPENT_CASE_HALF_LENGTH, stand_xz.z);
            let head_from = turret.transform_point3(center);
            let head_to = turret.transform_point3(center + Vec3::Y * (SPENT_CASE_HALF_LENGTH + 0.04));
            if let Some(head) = contacts.sweep_batch(&[[head_from, head_to]])[0] {
                let head_local = to_turret(&head);
                trace.push(format!("{i}: stand y {:.2}, headroom blocked at y {:.2}", floor_local.y, head_local.y));
                poses.push(None);
                continue;
            }
            trace.push(format!("{i}: stand at {:.2},{:.2},{:.2}", center.x, floor_local.y, center.z));
            // Upright: the case's long axis (local Z) stands on Y, its base turned to the wall.
            let rotation = Quat::from_rotation_y(theta) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            poses.push(Some((center, rotation)));
        }
        let count = poses.iter().filter(|p| p.is_some()).count();
        if best.as_ref().is_none_or(|(c, _, _)| count > *c) {
            best = Some((count, poses, probe_y));
        }
        }
        let (found, poses, row_y) = best.unwrap_or((0, vec![None; EMPTIES_RACK_SLOTS], 0.0));
        println!("[crew] empties rack probed: {found} of {EMPTIES_RACK_SLOTS} upright stands along the rear wall (row probed at y {row_y:.2})");
        for line in &trace {
            println!("[crew]   rack {line}");
        }
        self.crew_solo.empties_rack.poses = Some(poses);
        self.crew_solo.empties_rack.probe_trace = trace;
    }

    /// The loader, at his post and fit, has just freed the hot case: he stows it upright in the
    /// rack and is away from the breech for the stow time. Rushed (a contact under fire at
    /// will), hurt, or with the rack full, he lets it fall as before.
    pub(super) fn crew_solo_offer_stow(&mut self, casing: usize, position_world: Vec3, rotation_world: Quat) {
        if !self.crew_solo_active() || !self.crew_solo_working(CrewRole::Loader) {
            return;
        }
        self.crew_solo_probe_empties_rack();
        let perf = self.crew_solo.man(CrewRole::Loader).performance();
        let rushed = self.crew_solo.gunner_target.is_some() && self.crew_solo.fire_at_will;
        let Some(slot) = self.crew_solo.empties_rack.free_slot() else {
            self.crew_solo.empties_rack.dropped_total += 1;
            self.crew_solo_rack_full_notice();
            return;
        };
        if rushed || perf < STOW_PERF_MIN {
            self.crew_solo.empties_rack.dropped_total += 1;
            return;
        }
        let seconds = STOW_S / perf.max(0.15);
        self.crew_solo_stow_case(casing, slot, position_world, rotation_world, seconds);
        self.crew_solo.loader_busy_s = seconds;
    }

    /// Letting go of a carried case within reach of an empty slot puts it in upright.
    pub(super) fn crew_solo_stow_from_hands(&mut self, casing: usize) -> bool {
        if !self.crew_solo_active() {
            return false;
        }
        self.crew_solo_probe_empties_rack();
        let Some(slot) = self.crew_solo_slot_in_reach(self.carry_pos) else { return false; };
        self.crew_solo_stow_case(casing, slot, self.carry_pos, self.carry_rotation, 0.35);
        self.crew_solo.say("Empty in the rack.");
        true
    }

    fn crew_solo_slot_in_reach(&self, hands_world: Vec3) -> Option<usize> {
        let turret = self.player.turret_model();
        let rack = &self.crew_solo.empties_rack;
        (0..EMPTIES_RACK_SLOTS)
            .filter(|&i| rack.slots[i].is_none())
            .filter_map(|i| rack.pose(i).map(|(center, _)| (i, turret.transform_point3(center))))
            .filter(|(_, center)| center.distance(hands_world) <= STOW_REACH_M)
            .min_by(|a, b| a.1.distance(hands_world).total_cmp(&b.1.distance(hands_world)))
            .map(|(i, _)| i)
    }

    fn crew_solo_stow_case(&mut self, casing: usize, slot: usize, position_world: Vec3, rotation_world: Quat, seconds: f32) {
        let turret = self.player.turret_model();
        let inverse = turret.inverse();
        let turret_rot = turret.to_scale_rotation_translation().1;
        let case = &mut self.casings[casing];
        case.carried = false;
        case.stowed = Some(slot as u8);
        case.stow_s = seconds;
        case.stow_t = seconds;
        case.stow_from = (inverse.transform_point3(position_world), (turret_rot.inverse() * rotation_world).normalize());
        case.vel = Vec3::ZERO;
        case.angular_vel = Vec3::ZERO;
        case.aboard = true;
        case.outside_rest = false;
        self.crew_solo.empties_rack.slots[slot] = Some(casing);
        self.crew_solo.empties_rack.stowed_total += 1;
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "stowed_empty": slot,
            "in_rack": self.crew_solo.empties_rack.count()}));
    }

    fn crew_solo_rack_full_notice(&mut self) {
        if !self.crew_solo.triage_called.is_empty() {
            return;
        }
        let line = "Loader: rack's full.";
        if self.crew_solo.intercom_line() != Some(line) {
            self.crew_solo.say(line);
        }
    }

    /// A carried case near an empty slot: say so (the hover text; letting go stows it).
    pub(super) fn crew_solo_rack_hover(&mut self) {
        if !self.crew_solo_active() || self.carried_casing.is_none() || self.hover_label.is_some() {
            return;
        }
        self.crew_solo_probe_empties_rack();
        if self.crew_solo_slot_in_reach(self.carry_pos).is_some() {
            self.hover_label = Some("EMPTIES RACK - LET GO TO STAND THE CASE IN IT");
        }
    }

    /// Walk to the rear of the basket and face the empties rack (headless review).
    pub fn debug_crew_solo_face_empties_rack(&mut self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.crew_solo_probe_empties_rack();
        self.debug_walk_to(Vec3::new(-0.10, 0.0, -0.05));
        // The middle of the filled row (or of the whole row before any case is in it).
        let rack = &self.crew_solo.empties_rack;
        let filled: Vec<Vec3> = (0..EMPTIES_RACK_SLOTS).filter(|&i| rack.slots[i].is_some())
            .filter_map(|i| rack.pose(i).map(|(c, _)| c)).collect();
        let all: Vec<Vec3> = (0..EMPTIES_RACK_SLOTS).filter_map(|i| rack.pose(i).map(|(c, _)| c)).collect();
        let pick = if filled.is_empty() { &all } else { &filled };
        let aim = if pick.is_empty() { Vec3::new(0.0, 1.95, -1.05) }
            else { pick.iter().copied().sum::<Vec3>() / pick.len() as f32 };
        let point = self.player.turret_model().transform_point3(aim);
        self.loader_face_world(point);
        true
    }

    /// Spent cases standing in the rack; the salvage and resupply paths read this.
    pub fn stowed_empties(&self) -> usize {
        self.crew_solo.empties_rack.count()
    }

    /// Spent cases loose aboard: not stowed, not in the hands, inside the hull envelope.
    pub fn loose_empties_aboard(&self) -> usize {
        self.casings.iter().filter(|c| c.active && !c.carried && c.stowed.is_none() && c.aboard).count()
    }

    pub fn empties_rack_receipt(&self) -> Value {
        let rack = &self.crew_solo.empties_rack;
        json!({"slots": EMPTIES_RACK_SLOTS, "stands_found": rack.poses.as_ref().map(|p| p.iter().filter(|x| x.is_some()).count()),
            "in_rack": rack.count(), "stowed_total": rack.stowed_total, "dropped_total": rack.dropped_total,
            "loose_aboard": self.loose_empties_aboard(), "probe_trace": rack.probe_trace,
            "poses": rack.poses.as_ref().map(|p| p.iter().map(|x| x.map(|(c, _)| c.to_array())).collect::<Vec<_>>())})
    }

    // ── The commander's designation ─────────────────────────────────────────────────────────

    /// MMB (pad RB) at the cupola, buttoned or head out: designate what the view centre is on.
    fn crew_solo_designate_input(&mut self, input: &TankInput, eye: Vec3, look: Vec3) {
        let press = input.coax && !self.crew_solo.prev_designate;
        self.crew_solo.prev_designate = input.coax;
        if !press
            || self.mode != Mode::Tank
            || self.station != Station::Commander
            || self.switch_t > 0.0
            || !self.player.alive
            || self.map_held
            || self.compass_equipped
        {
            return;
        }
        self.crew_solo_designate(eye, look);
    }

    /// Cast from the commander's eye along his look: the first living contact (a sphere round
    /// its centre) or the terrain; the hit distance is the range estimate. A second designation
    /// on about the same bearing refines the range instead of starting over.
    pub(super) fn crew_solo_designate(&mut self, eye: Vec3, look: Vec3) -> Option<Designation> {
        let look = look.normalize_or_zero();
        if look.length_squared() < 0.5 || !eye.is_finite() {
            return None;
        }
        let mut best: Option<(f32, Option<usize>)> = None;
        for (i, e) in self.enemies.iter().enumerate() {
            if !e.alive || !self.enemy_active.get(i).copied().unwrap_or(false) {
                continue;
            }
            let to = e.center() - eye;
            let along = to.dot(look);
            if along <= 0.0 || along > DESIGNATION_RANGE_MAX_M {
                continue;
            }
            let miss = (to - look * along).length();
            if miss <= 2.4 && best.is_none_or(|(t, _)| along < t) {
                best = Some((along, Some(i)));
            }
        }
        if let Some(t) = ray_terrain(eye, look, &self.hf, DESIGNATION_RANGE_MAX_M) {
            if best.is_none_or(|(b, _)| t < b) {
                best = Some((t, None));
            }
        }
        let (range, enemy) = best.unwrap_or((DESIGNATION_RANGE_MAX_M, None));
        let point = eye + look * range;
        let shell = if self.he_loaded { &crate::ballistics::SPRGR_88 } else { &crate::ballistics::PZGR_39 };
        let bearing = scripted_gun_lay(&self.player, point, shell).0;
        let refine = self.crew_solo.designation.filter(|d| {
            let mut diff = (d.bearing - bearing) % std::f32::consts::TAU;
            if diff > std::f32::consts::PI { diff -= std::f32::consts::TAU; }
            if diff < -std::f32::consts::PI { diff += std::f32::consts::TAU; }
            diff.abs() < DESIGNATION_REFINE_RAD
        });
        let designation = match refine {
            Some(old) => Designation { point, bearing, range_m: range, enemy: enemy.or(old.enemy),
                refinements: old.refinements.saturating_add(1), ..old },
            None => Designation { point, bearing, range_m: range, enemy, set_t: self.crew_solo.time_s,
                refinements: 0, on_target_after_s: None, turret_yaw_at_set: self.player.turret_yaw },
        };
        self.crew_solo.designation = Some(designation);
        let to = point - self.player.pos;
        let rel = (to.x.atan2(to.z) - self.player.heading).rem_euclid(std::f32::consts::TAU);
        let oclock = ((rel / std::f32::consts::TAU * 12.0).round() as i32).rem_euclid(12);
        let oclock = if oclock == 0 { 12 } else { oclock };
        let range_word = (range / 50.0).round() as i32 * 50;
        let gunner = self.crew_solo_ai_gunner_active();
        if gunner {
            let line = match (refine.is_some(), enemy.is_some()) {
                (true, _) => format!("Gunner: range {range_word}."),
                (false, true) => format!("Gunner: tank, {oclock} o'clock, {range_word}. Traversing."),
                (false, false) => format!("Gunner: {oclock} o'clock, {range_word}. Traversing."),
            };
            self.crew_solo.say(line);
        }
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "designation": if refine.is_some() { "refined" } else { "set" },
            "bearing_rad": bearing, "range_m": range, "enemy": enemy, "oclock": oclock, "gunner_alive": gunner}));
        println!("[crew] commander designates {oclock} o'clock, {range:.0} m (turret bearing {:.1} deg{}){}",
            bearing.to_degrees(), if enemy.is_some() { ", a contact" } else { "" },
            if refine.is_some() { ", refined" } else { "" });
        Some(designation)
    }

    /// The second pointer on the gunner's turret position indicator: the commander's
    /// designated bearing, while a designation stands. The turret's own pointer turns with the
    /// turret; the two lying together is the "on" cue.
    pub(super) fn crew_solo_drive_tpz_commander_needle(&self, turret: Mat4, axis: Vec3, r: &mut Renderer) {
        let slot = self.slots.tpz_commander_needle;
        let Some(d) = self.crew_solo.designation.filter(|_| self.crew_solo_active()) else {
            r.set_visible(slot, false);
            return;
        };
        // Drawn a few millimetres in front of the turret pointer and shorter, so both read.
        let model = turret
            * Mat4::from_translation(crate::interior_rig::TPZ_NEEDLE_POS + axis.normalize_or_zero() * 0.004)
            * Mat4::from_axis_angle(axis, -d.bearing)
            * Mat4::from_scale(Vec3::splat(0.72));
        r.set_model(slot, model);
        r.set_visible(slot, self.interior_light > 0.02);
    }

    pub fn crew_solo_session_json(&self) -> Value {
        if !self.crew_solo_active() {
            return Value::Null;
        }
        json!({
            "designation": self.crew_solo.designation.map(|d| json!({"bearing_rad": d.bearing,
                "range_m": d.range_m, "contact": d.enemy, "age_s": self.crew_solo.time_s - d.set_t,
                "refinements": d.refinements, "on_target_after_s": d.on_target_after_s})),
            "turret_yaw_rad": self.player.turret_yaw,
            "gunner_target": self.crew_solo.gunner_target,
            "fire_at_will": self.crew_solo.fire_at_will,
            "men": CrewRole::ALL.iter().map(|&role| json!({"role": role.name(),
                "state": self.crew_solo.man(role).state_label(),
                "performance": self.crew_solo.man(role).performance()})).collect::<Vec<_>>(),
        })
    }

    /// Headless: designate a world point from the commander's eye, through the same cast.
    pub fn debug_crew_solo_designate_at(&mut self, point: Vec3) -> Option<Value> {
        let eye = self.station_eye(Station::Commander);
        let d = self.crew_solo_designate(eye, point - eye)?;
        Some(json!({"bearing_rad": d.bearing, "range_m": d.range_m, "contact": d.enemy, "refinements": d.refinements}))
    }

    pub fn debug_crew_solo_designation(&self) -> Value {
        self.crew_solo.designation.map_or(Value::Null, |d| json!({"bearing_rad": d.bearing,
            "range_m": d.range_m, "contact": d.enemy, "age_s": self.crew_solo.time_s - d.set_t,
            "refinements": d.refinements, "on_target_after_s": d.on_target_after_s,
            "turret_yaw_at_set": d.turret_yaw_at_set}))
    }

    pub fn debug_ground_height(&self, x: f32, z: f32) -> f32 {
        self.hf.height_at(x, z)
    }

    pub fn debug_crew_solo_tpz_needles_visible(&self) -> bool {
        self.crew_solo.designation.is_some()
    }

    pub fn debug_crew_solo_face_tpz(&mut self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.debug_walk_to(Vec3::new(0.45, 0.0, -0.10));
        let point = self.player.turret_model().transform_point3(crate::interior_rig::TPZ_NEEDLE_POS);
        self.loader_face_world(point);
        true
    }

    // ── Dumping the empties ─────────────────────────────────────────────────────────────────

    /// With the rack full the loader asks to dump the empties out of the side escape hatch.
    /// Approved (Y), he opens the hatch between reloads when no contact is in the gunner's
    /// sight, heaves the cases out one by one, rack first, then the loose ones, and shuts it;
    /// a contact makes him shut it at once. Refused (N) or unanswered, the cases fall to the
    /// floor and stay there, underfoot. N later withdraws an approval; Y later grants it.
    fn crew_solo_empties_dump(&mut self, input: &TankInput, dt: f32) {
        let yes = input.crew_yes && !self.crew_solo.prev_yes;
        let no = input.crew_no && !self.crew_solo.prev_no;
        self.crew_solo.prev_yes = input.crew_yes;
        self.crew_solo.prev_no = input.crew_no;
        let loader_ok = self.crew_solo_working(CrewRole::Loader) && self.player.alive;
        let rack = &self.crew_solo.empties_rack;
        let full = rack.probed() && rack.is_full();

        match self.crew_solo.dump_permission {
            DumpPermission::NotAsked => {
                if full && loader_ok {
                    self.crew_solo.dump_permission = DumpPermission::Asked;
                    self.crew_solo.ask_clock = 0.0;
                    self.crew_solo.say("Loader: rack's full. Out the side hatch? (Y/N)");
                }
            }
            DumpPermission::Asked => {
                self.crew_solo.ask_clock += dt;
                if self.crew_solo.ask_clock >= ASK_AGAIN_S && loader_ok {
                    self.crew_solo.ask_clock = 0.0;
                    self.crew_solo.say("Loader: empties, Herr Kommandant? (Y/N)");
                }
            }
            DumpPermission::Approved | DumpPermission::Refused => {}
        }
        if yes && self.crew_solo.dump_permission != DumpPermission::Approved
            && self.crew_solo.dump_permission != DumpPermission::NotAsked
        {
            self.crew_solo.dump_permission = DumpPermission::Approved;
            self.crew_solo.say("Commander: out with them.");
            self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "dump": "approved"}));
        }
        if no && self.crew_solo.dump_permission != DumpPermission::Refused
            && self.crew_solo.dump_permission != DumpPermission::NotAsked
        {
            self.crew_solo.dump_permission = DumpPermission::Refused;
            self.crew_solo.say("Commander: keep the hatch shut.");
            self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "dump": "refused"}));
        }

        let contact = self.crew_solo.gunner_target.is_some();
        let has_empties = self.crew_solo.empties_rack.count() > 0 || self.loose_empties_aboard() > 0;
        let may_dump = self.crew_solo.dump_permission == DumpPermission::Approved
            && loader_ok
            && !contact
            && has_empties
            && matches!(self.breech, Breech::Loaded)
            && self.crew_solo.loader_busy_s <= 0.0;
        if !may_dump {
            if self.crew_solo.dump_hatch_open {
                self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "dump_gate_closed": {
                    "permission": format!("{:?}", self.crew_solo.dump_permission), "loader_ok": loader_ok,
                    "contact": contact, "has_empties": has_empties, "breech": format!("{:?}", self.breech),
                    "loader_busy_s": self.crew_solo.loader_busy_s}}));
                self.roof_hatches.open[ESCAPE_HATCH] = false;
                self.crew_solo.dump_hatch_open = false;
                self.crew_solo.say(if contact { "Loader: contact, hatch shut." } else { "Loader: hatch shut." });
            }
            self.crew_solo.dump_clock = 0.0;
            return;
        }
        if !self.roof_hatches.open[ESCAPE_HATCH] {
            self.roof_hatches.open[ESCAPE_HATCH] = true;
            self.crew_solo.dump_hatch_open = true;
            self.crew_solo.say("Loader: side hatch open.");
        }
        if !self.access_hatch_open(ESCAPE_HATCH) {
            return; // the lid is still swinging
        }
        let perf = self.crew_solo.man(CrewRole::Loader).performance().max(0.15);
        self.crew_solo.dump_clock += dt;
        if self.crew_solo.dump_clock < DUMP_EACH_S / perf {
            return;
        }
        self.crew_solo.dump_clock = 0.0;
        // Rack first (last in, first out), then a loose case from the floor.
        let from_rack = (0..EMPTIES_RACK_SLOTS).rev().find_map(|i| self.crew_solo.empties_rack.slots[i].map(|k| (i, k)));
        let casing = match from_rack {
            Some((slot, k)) => {
                self.crew_solo.empties_rack.slots[slot] = None;
                Some(k)
            }
            None => self.casings.iter().position(|c| c.active && !c.carried && c.stowed.is_none() && c.aboard),
        };
        if let Some(k) = casing {
            self.crew_solo_heave_out(k);
        }
    }

    /// The case leaves through the open escape hatch: placed just outside the aperture with the
    /// heave's velocity outward and up, then the ordinary case physics carries it to wherever it
    /// lands (the deck, the track guard or the ground), where it stays.
    fn crew_solo_heave_out(&mut self, k: usize) {
        let turret = self.player.turret_model();
        let center = turret.transform_point3(tiger_hatches::APERTURE_CENTERS[ESCAPE_HATCH]);
        let normal = turret.transform_vector3(tiger_hatches::APERTURE_NORMALS[ESCAPE_HATCH]).normalize_or_zero();
        let seed = self.crew_solo.dumped_total.wrapping_mul(2654435761) ^ 0xD0_4D5E;
        let jitter = |n: u32| hash01(seed.wrapping_add(n)) - 0.5;
        let case = &mut self.casings[k];
        case.stowed = None;
        case.stow_t = 0.0;
        case.carried = false;
        case.pos = center + normal * 0.22 + Vec3::Y * 0.05;
        case.vel = normal * (CASING_THROW_SPEED + jitter(1)) + Vec3::Y * (1.2 + 0.4 * jitter(2));
        case.rotation = (Quat::from_rotation_y(jitter(3) * 2.0) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.8)).normalize();
        case.angular_vel = Vec3::new(jitter(4), jitter(5), jitter(6)).normalize_or_zero() * 6.0;
        case.drop_rest = shell_collision::DropRest::default();
        case.impact_cooldown = 0.0;
        case.aboard = false;
        case.outside_rest = false;
        self.crew_solo.dumped_total += 1;
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "dumped_empty": k,
            "in_rack": self.crew_solo.empties_rack.count()}));
    }

    /// Loose aboard cases within `radius` of a turret-local floor point (planar).
    fn crew_solo_cases_near_turret_xz(&self, point: Vec2, radius: f32) -> usize {
        if !self.crew_solo_active() {
            return 0;
        }
        let inverse = self.player.turret_model().inverse();
        self.casings.iter().filter(|c| c.active && !c.carried && c.stowed.is_none() && c.aboard)
            .filter(|c| {
                let local = inverse.transform_point3(c.pos);
                local.y < crate::interior_rig::FLOOR_STAND_Y + 0.25 && Vec2::new(local.x, local.z).distance(point) < radius
            })
            .count()
    }

    /// The player's step over loose empties: slower with each one underfoot (never below half).
    pub(super) fn crew_solo_floor_clutter_factor(&self, walk_pos: Vec3) -> f32 {
        let n = self.crew_solo_cases_near_turret_xz(Vec2::new(walk_pos.x, walk_pos.z), CLUTTER_FEET_M);
        (1.0 - 0.18 * n as f32).max(0.5)
    }

    /// Walking into loose empties kicks them along the step: the resting case is woken with the
    /// foot's speed and rolls on through the ordinary case physics.
    pub(super) fn crew_solo_kick_brass(&mut self, walk_delta_turret: Vec3, dt: f32) {
        if !self.crew_solo_active() || dt <= 0.0 {
            return;
        }
        let step = Vec2::new(walk_delta_turret.x, walk_delta_turret.z);
        if step.length() < 1.0e-4 {
            return;
        }
        let turret = self.player.turret_model();
        let inverse = turret.inverse();
        let feet = Vec2::new(self.walk_pos.x, self.walk_pos.z);
        let foot_velocity = turret.transform_vector3(Vec3::new(step.x, 0.0, step.y) / dt);
        for c in self.casings.iter_mut().filter(|c| c.active && !c.carried && c.stowed.is_none() && c.aboard) {
            let local = inverse.transform_point3(c.pos);
            if local.y > crate::interior_rig::FLOOR_STAND_Y + 0.25 {
                continue;
            }
            if Vec2::new(local.x, local.z).distance(feet) < CLUTTER_FEET_M * 0.8 {
                c.vel = foot_velocity * 1.4 + Vec3::Y * 0.3;
                c.angular_vel += Vec3::new(foot_velocity.z, 0.0, -foot_velocity.x) * 4.0;
                c.drop_rest = shell_collision::DropRest::default();
            }
        }
    }

    pub fn crew_solo_dump_receipt(&self) -> Value {
        json!({"permission": format!("{:?}", self.crew_solo.dump_permission),
            "dumped_total": self.crew_solo.dumped_total, "hatch_open": self.roof_hatches.open[ESCAPE_HATCH],
            "hatch_opened_by_loader": self.crew_solo.dump_hatch_open,
            "outside_resting": self.casings.iter().filter(|c| c.active && c.outside_rest).count(),
            "loose_aboard": self.loose_empties_aboard(), "in_rack": self.stowed_empties()})
    }

    /// Headless: stand a case upright in every free rack slot (a run's worth of empties), so the
    /// full-rack request comes up without firing a dozen rounds. The receipt names it.
    pub fn debug_crew_solo_fill_empties_rack(&mut self) -> usize {
        self.crew_solo_probe_empties_rack();
        let mut filled = 0;
        while let Some(slot) = self.crew_solo.empties_rack.free_slot() {
            let Some((center, rotation)) = self.crew_solo.empties_rack.pose(slot) else { break };
            let turret = self.player.turret_model();
            let world = turret.transform_point3(center);
            let Some(k) = self.spawn_casing_from(world, Vec3::ZERO) else { break };
            let world_rot = (turret.to_scale_rotation_translation().1 * rotation).normalize();
            self.crew_solo_stow_case(k, slot, world, world_rot, 0.0);
            filled += 1;
        }
        filled
    }

    /// Walk to the rear of the basket and face the open side escape hatch (headless review).
    pub fn debug_crew_solo_face_escape_hatch(&mut self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.debug_walk_to(Vec3::new(-0.35, 0.0, -0.30));
        let point = self.player.turret_model().transform_point3(tiger_hatches::APERTURE_CENTERS[ESCAPE_HATCH]);
        self.loader_face_world(point);
        true
    }

    /// Headless: drop a spent case at a turret-local floor spot (clutter for the refusal check).
    pub fn debug_crew_solo_drop_case(&mut self, turret_local: Vec3) -> bool {
        let world = self.player.turret_model().transform_point3(turret_local);
        self.spawn_casing_from(world, Vec3::ZERO).is_some()
    }

    pub fn debug_crew_solo_walk_pos(&self) -> Vec3 {
        self.walk_pos
    }

    pub fn debug_crew_solo_floor_clutter(&self) -> (usize, f32) {
        (self.crew_solo_cases_near_turret_xz(LOADER_POST_TURRET_XZ, CLUTTER_LOADER_M),
            self.crew_solo_floor_clutter_factor(self.walk_pos))
    }

    // ── The player's hands ──────────────────────────────────────────────────────────────────

    /// The procedure in the hands, or the hold that drags a body. The mouse is the hands while
    /// a procedure runs (the look is locked in `game.rs`); E or LMB is the button.
    fn crew_solo_hands(&mut self, input: &TankInput, dt: f32) {
        let button = input.mg_coax && self.mode == Mode::Tank && self.player.alive;
        let press = button && !self.crew_solo.prev_hands_button;
        self.crew_solo.prev_hands_button = button;
        let here = self.walk_pos;

        if let Some((role, mut run, start)) = self.crew_solo.procedure.take() {
            self.crew_solo.action = None;
            if self.mode != Mode::Tank || self.station != Station::Loader || !self.player.alive
                || start.distance(here) > 0.10
            {
                self.crew_solo.say(format!("{}: left half done.", role_short(role)));
                self.crew_solo.procedures_done.push(json!({"t": self.crew_solo.time_s, "man": role.name(),
                    "procedure": run.procedure.label(), "abandoned_at_step": run.step, "quality": run.quality}));
                return;
            }
            // A step whose item is gone: the canteen is skipped dry, anything else ends the work.
            if let Some(step) = run.current() {
                if let Some(item) = step.consumes {
                    if !self.crew_solo.aid_box.has(item) {
                        if item == first_aid::Item::Canteen {
                            run.skip_step();
                            self.crew_solo.say("No water left. Dress it dry.");
                        } else {
                            self.crew_solo.say("Nothing left in the box.");
                            self.crew_solo.procedures_done.push(json!({"t": self.crew_solo.time_s, "man": role.name(),
                                "procedure": run.procedure.label(), "out_of_kit_at_step": run.step}));
                            return;
                        }
                    }
                }
            }
            let hands = Hands { hold: button, press, stroke: input.look_dx, dt };
            let step_before = run.step;
            let outcome = run.advance(hands);
            if run.step != step_before {
                if let Some(step) = run.steps().get(step_before) {
                    self.crew_solo.aid_box.take_for(step);
                }
                // A step of the procedure done: a bandage torn and wound, a strap pulled.
                let at = self.hands_pos() - Vec3::Y * 0.3;
                self.foley(crate::acoustics::foley::Cue::Body(crate::acoustics::foley::BodyCue::BandageTear), 1.0, 0.0, at);
            }
            match outcome {
                Progress::Working | Progress::StepDone => {
                    self.crew_solo.procedure = Some((role, run, start));
                }
                Progress::Finished => {
                    let result = self.crew_solo.man_mut(role).finish_procedure(&run);
                    let quality = if run.improvised { run.quality.min(0.6) } else { run.quality };
                    match result {
                        Ok(()) => {
                            self.crew_solo.treatments += 1;
                            let how = if quality >= 0.85 { "good and tight" } else if quality >= 0.5 { "loose" } else { "barely holding" };
                            let word = match run.procedure {
                                Procedure::Tourniquet => "tourniquet on",
                                Procedure::PressureDressing => "dressed",
                                Procedure::Splint => "splinted",
                                Procedure::BurnCare => "burn dressed",
                                Procedure::ChestSeal => "chest sealed",
                                Procedure::Airway => "on his side, breathing",
                                Procedure::Morphine => "morphine in, marked",
                            };
                            self.crew_solo.say(format!("{}: {word}, {how}.", role_short(role)));
                            self.crew_solo.procedures_done.push(json!({"t": self.crew_solo.time_s, "man": role.name(),
                                "procedure": run.procedure.label(), "improvised": run.improvised,
                                "quality": quality, "seconds": run.elapsed_s, "interruptions": run.interruptions,
                                "skipped_cooling": run.skipped_cooling, "box": aid_box_json(&self.crew_solo.aid_box)}));
                        }
                        Err(refusal) => {
                            self.crew_solo.procedures_done.push(json!({"t": self.crew_solo.time_s, "man": role.name(),
                                "procedure": run.procedure.label(), "refused": format!("{refusal:?}")}));
                        }
                    }
                }
            }
            return;
        }

        let hover = self.crew_solo.hover;
        match hover {
            Some(HoverAction::Treat(..)) | Some(HoverAction::SelfAid(_)) => {
                self.crew_solo.action = None;
                if !press {
                    return;
                }
                let (role, procedure) = match hover {
                    Some(HoverAction::Treat(role, p)) => (role, p),
                    Some(HoverAction::SelfAid(p)) => (self.solo_identity().unwrap_or(CrewRole::Commander), p),
                    _ => unreachable!(),
                };
                let man = self.crew_solo.man(role);
                let wound = man.procedure_needed().filter(|(p, _)| *p == procedure).and_then(|(_, w)| w);
                if let Some(run) = man.begin_procedure(procedure, wound, &self.crew_solo.aid_box) {
                    self.crew_solo.procedure = Some((role, run, here));
                    self.crew_solo.say(format!("{}: hold still.", role_short(role)));
                }
            }
            Some(action @ HoverAction::ClearBody(role)) => {
                if !button {
                    self.crew_solo.action = None;
                    return;
                }
                let progress = match self.crew_solo.action {
                    Some((current, t, start)) if current == action && start.distance(here) < 0.08 => t + dt,
                    _ => 0.0,
                };
                if progress < CLEAR_BODY_S {
                    self.crew_solo.action = Some((action, progress, here));
                    // The remaining distance over the remaining hold: the body arrives as the hold ends.
                    let remaining = (CLEAR_BODY_S - progress).max(dt);
                    if progress <= dt {
                        let at = self.hands_pos() - Vec3::Y * 0.4;
                        self.foley(crate::acoustics::foley::Cue::Body(crate::acoustics::foley::BodyCue::DragBody), 1.0, 0.0, at);
                    }
                    self.crew_solo_drag_body(role, dt / remaining);
                    return;
                }
                self.crew_solo.action = None;
                self.crew_solo.body_cleared[role_index(role)] = true;
                self.crew_solo_drag_body(role, 1.0);
                self.crew_solo.say(format!("{}'s body moved aside.", role_short(role)));
                self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "body_cleared": role.name()}));
            }
            None => self.crew_solo.action = None,
        }
    }

    /// The player's hands are on a man: the look is his hands now.
    pub(super) fn crew_solo_hands_busy(&self) -> bool {
        self.crew_solo.procedure.is_some()
    }

    /// A hit on the tank mid-procedure: the hands jerk, the step is lost.
    fn crew_solo_interrupt_hands(&mut self) {
        if let Some((role, run, _)) = self.crew_solo.procedure.as_mut() {
            run.interrupt();
            let line = format!("Hit. Hands off {} a moment.", role_short(*role));
            self.crew_solo.say(line);
        }
    }

    /// Drag a settled casualty off his seat toward the rear of the basket floor by `fraction` of
    /// the remaining distance. The articulated body is translated as one (its joints keep their
    /// solved pose); not a contact solve.
    fn crew_solo_drag_body(&mut self, role: CrewRole, fraction: f32) {
        let identity = singleplayer_crew::body_post(role);
        let Some(body) = self
            .couch_ragdolls
            .iter_mut()
            .find(|body| body.casualty.identity == identity)
        else {
            return;
        };
        let frame_model = blood_parent_model(&self.player, body.frame);
        let rest = match body.frame {
            BloodFrame::Turret => Vec3::new(-0.35, crate::interior_rig::FLOOR_STAND_Y + 0.12, -0.62),
            BloodFrame::Hull => Vec3::new(0.0, 0.80, 0.25),
        };
        let center = crew_physics::center_of_mass(&body.segments);
        let step = (rest - center) * fraction.clamp(0.0, 1.0);
        if !step.is_finite() {
            return;
        }
        let _ = frame_model;
        for segment in &mut body.segments {
            segment.pos += step;
            segment.vel = Vec3::ZERO;
        }
        body.last_physical_center = crew_physics::center_of_mass(&body.segments);
    }

    /// Crosshair on a crewman, a dead man's body, or the first-aid box. Runs from the interior
    /// walk with empty hands, after the ordinary hotspot/station hover has had its say.
    pub(super) fn crew_solo_hover(&mut self, eye: Vec3, fwd: Vec3) {
        // A procedure in the hands keeps its man; the label is the step the hands are on.
        if let Some((role, run, _)) = &self.crew_solo.procedure {
            let action = if Some(*role) == self.solo_identity() { HoverAction::SelfAid(run.procedure) } else { HoverAction::Treat(*role, run.procedure) };
            self.crew_solo.hover = Some(action);
            self.hover_label = run.current().map(|step| step.label);
            return;
        }
        // Hands already on a body: the drag keeps its target until E is released or the player
        // steps away (the body moves as it is dragged; a neighbour must not steal it).
        if let Some((action, _, start)) = self.crew_solo.action {
            if self.mode == Mode::Tank && self.station == Station::Loader && start.distance(self.walk_pos) < 0.08 {
                self.crew_solo.hover = Some(action);
                self.hover_label = Some(match action {
                    HoverAction::Treat(role, p) => offer_label(role, p, Some(false)),
                    HoverAction::SelfAid(p) => offer_label(CrewRole::Commander, p, Some(false)),
                    HoverAction::ClearBody(role) => clear_label(role),
                });
                return;
            }
        }
        self.crew_solo.hover = None;
        if !self.crew_solo_active()
            || self.mode != Mode::Tank
            || self.station != Station::Loader
            || self.switch_t > 0.0
            || self.climb.active
            || !self.player.alive
            || self.activity.is_some()
            || self.hover.is_some()
            || self.hover_station.is_some()
            || self.carry_mesh.is_some()
            || self.carried_casing.is_some()
            || self.ext_equipped
            || self.carrying_track_kit
            || self.panzerfaust_claimed
            || self.map_held
        {
            return;
        }
        let me = self.solo_identity();
        let mut best = AID_CONE_COS;
        let mut pick: Option<(HoverAction, &'static str)> = None;
        let mut consider = |point: Vec3, action: HoverAction, label: &'static str, reach: f32| {
            let to = point - eye;
            let distance = to.length();
            if distance > reach || distance < 1.0e-3 {
                return;
            }
            let d = (to / distance).dot(fwd);
            if d > best {
                best = d;
                pick = Some((action, label));
            }
        };
        for role in CrewRole::ALL {
            if Some(role) == me {
                continue;
            }
            let alive = singleplayer_crew::role_alive(&self.crew, role) && !self.crew_solo.man(role).dead;
            if alive {
                let Some((procedure, _)) = self.crew_solo.man(role).procedure_needed() else { continue; };
                let post = self.crew_solo_post(role);
                let torso = self.crew_solo_man_point(role);
                let can = self.crew_solo.aid_box.can_start(procedure);
                let reach = if matches!(post, LanRole::Driver | LanRole::Radio) { AID_REACH_HULL_M } else { AID_REACH_M };
                consider(torso, HoverAction::Treat(role, procedure), offer_label(role, procedure, can), reach);
            } else if !self.crew_solo.body_cleared[role_index(role)] {
                let identity = singleplayer_crew::body_post(role);
                if let Some(body) = self.couch_ragdolls.iter().find(|b| b.casualty.identity == identity) {
                    let center = blood_parent_model(&self.player, body.frame)
                        .transform_point3(crew_physics::center_of_mass(&body.segments));
                    let reach = if body.frame == BloodFrame::Hull { AID_REACH_HULL_M } else { AID_REACH_M };
                    consider(center, HoverAction::ClearBody(role), clear_label(role), reach);
                }
            }
        }
        if let Some(me) = me {
            if let Some((procedure, _)) = self.crew_solo.man(me).procedure_needed() {
                let point = self.player.turret_model().transform_point3(AID_BOX_TURRET_LOCAL);
                let can = self.crew_solo.aid_box.can_start(procedure);
                consider(point, HoverAction::SelfAid(procedure), offer_label(CrewRole::Commander, procedure, can), AID_REACH_M);
            }
        }
        if let Some((action, label)) = pick {
            let supplied = match action {
                HoverAction::Treat(_, p) | HoverAction::SelfAid(p) => self.crew_solo.aid_box.can_start(p).is_some(),
                HoverAction::ClearBody(_) => true,
            };
            self.hover_label = Some(label);
            if supplied {
                self.crew_solo.hover = Some(action);
            }
        }
    }

    /// The driver's seat as an interior entry anchor, and the dead-man gate on every anchor.
    /// Runs right after the ordinary station anchors are resolved.
    pub(super) fn crew_solo_station_anchors(&mut self, eye: Vec3, fwd: Vec3) {
        if !self.crew_solo_active() || self.mode != Mode::Tank || self.station != Station::Loader
            || self.hover.is_some() || self.hover_lever
        {
            return;
        }
        if self.hover_station.is_none() {
            let anchor = self.player.hull_model().transform_point3(DRIVER_SEAT_ANCHOR_HULL);
            let to = anchor - eye;
            if to.length() <= DRIVER_SEAT_REACH_M && to.normalize_or_zero().dot(fwd) > 0.93 {
                self.hover_station = Some(Station::Driver);
                self.hover_label = Some("DRIVER'S SEAT [click]");
            }
        }
        if let Some(station) = self.hover_station {
            if let Some(role) = self.crew_solo_station_blocked(station) {
                self.hover_station = None;
                self.hover_label = Some(blocked_label(role));
            }
        }
    }

    /// Entering a station in full-crew solo: refused while a dead man fills it; otherwise the
    /// move costs the real clamber between seats.
    pub(super) fn crew_solo_begin_station_move(&mut self, to: Station) -> bool {
        if !self.crew_solo_active() {
            return true;
        }
        if let Some(role) = self.crew_solo_station_blocked(to) {
            println!("[crew] {} is dead in that seat — drag him off first", role.name());
            return false;
        }
        if to == self.station {
            return true;
        }
        let origin = self.station_eye(self.station);
        self.switch_origin_hull = Some(self.player.hull_model().inverse().transform_point3(origin));
        self.switch_from = self.station;
        self.switch_dur = switch_cost(self.station, to);
        self.switch_t = self.switch_dur;
        true
    }

    // ── Wounds from the live solvers ─────────────────────────────────────────────────────────

    /// Record what the anatomical fragment/blast events did to a man who survived them. Called
    /// from the penetration path beside the retained-wound record.
    pub(super) fn crew_solo_record_wounds(&mut self, casualty: CouchCrewCasualty, events: &[CrewPartDamageEvent]) {
        if !self.crew_solo_active() {
            return;
        }
        let role = role_of_post(casualty.identity);
        if !singleplayer_crew::role_alive(&self.crew, role) {
            return;
        }
        let mut worst: Option<Region> = None;
        let mut any = false;
        for event in events {
            use crew_damage::CrewTissueOutcome as O;
            let region = region_of_part(event.result.part);
            let (severity, fracture) = match event.result.outcome {
                O::None => continue,
                O::Injury => ((event.result.total_work_j / 350.0).clamp(0.2, 0.85), event.result.fragment_work_j > 220.0 && region.is_limb()),
                O::Detachment => (1.0, false),
                O::Pulverized => (1.0, false),
            };
            any = true;
            worst = Some(region);
            let man = self.crew_solo.man_mut(role);
            man.add_wound(Wound::new(WoundKind::Bleeding, region, severity));
            if fracture {
                man.add_wound(Wound::new(WoundKind::Fracture, region, 0.6));
            }
            if event.result.blast_work_j > 150.0 || event.result.outcome == O::Pulverized {
                man.knock_out(6.0 + (event.result.blast_work_j / 60.0).min(40.0));
            }
        }
        if !any {
            return;
        }
        self.crew_solo_interrupt_hands();
        let man = self.crew_solo.man(role);
        let state = man.state_label();
        let (performance, conscious) = (man.performance(), man.is_conscious());
        let where_ = worst.map_or("", Region::label);
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "wounded": role.name(),
            "region": where_, "state": state, "performance": performance}));
        let line = if Some(role) == self.solo_identity() {
            format!("You: hit, {where_}.")
        } else if !conscious {
            format!("{}: hit — no answer.", role_short(role))
        } else {
            format!("{}: hit, {where_} — {}.", role_short(role), state)
        };
        self.crew_solo.say(line);
        // The short red flash and the ears: a man hit beside you, not only your own death.
        self.crew_ko_t = self.crew_ko_t.max(CREW_KO_DUR * 0.45);
    }

    /// A man killed outright by the solver: close his record so the time course stops.
    pub(super) fn crew_solo_record_kia(&mut self, casualty: CouchCrewCasualty) {
        if !self.crew_solo_active() {
            return;
        }
        let role = role_of_post(casualty.identity);
        self.crew_solo_interrupt_hands();
        if self.crew_solo.procedure.as_ref().is_some_and(|(patient, _, _)| *patient == role) {
            self.crew_solo.procedure = None;
        }
        let man = self.crew_solo.man_mut(role);
        man.dead = true;
        man.cause = Some("killed");
        if Some(role) != self.solo_identity() {
            self.crew_solo.say(format!("{}: killed.", role_short(role)));
        }
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "died": role.name(), "cause": "killed"}));
    }

    // ── Public readouts ─────────────────────────────────────────────────────────────────────

    /// Enter: the intercom line on the frame leaves it now. True when one was showing.
    pub fn crew_intercom_clear(&mut self) -> bool {
        let showing = self.crew_solo.intercom.iter().any(|(_, left)| *left > 0.0);
        for line in &mut self.crew_solo.intercom { line.1 = line.1.min(0.0); }
        showing
    }

    /// The latest intercom line on the air, for the front end. `KESSEL_INTERCOM=0` silences it.
    pub fn intercom_line(&self) -> Option<&str> {
        static SHOW: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let show = *SHOW.get_or_init(|| std::env::var("KESSEL_INTERCOM").map_or(true, |v| v != "0"));
        if !show || !self.crew_solo_active() {
            return None;
        }
        self.crew_solo.intercom_line()
    }

    pub fn crew_solo_receipt(&self) -> Value {
        let me = self.solo_identity();
        let men: Vec<Value> = CrewRole::ALL
            .iter()
            .map(|&role| {
                let man = self.crew_solo.man(role);
                let alive = singleplayer_crew::role_alive(&self.crew, role) && !man.dead;
                let post = if Some(role) == me {
                    if self.mode == Mode::Tank { Some(self.solo_player_post()) } else { None }
                } else if alive {
                    Some(self.crew_solo_post(role))
                } else {
                    None
                };
                json!({
                    "role": role.name(),
                    "player": Some(role) == me,
                    "alive": alive,
                    "state": man.state_label(),
                    "performance": man.performance(),
                    "blood_loss": man.blood_loss_fraction(),
                    "bleed_l_per_min": man.total_bleed_rate_l_per_s() * 60.0,
                    "conscious": man.is_conscious(),
                    "wounds": man.wounds.iter().map(|w| json!({
                        "kind": format!("{:?}", w.kind), "region": w.region.label(),
                        "severity": w.severity, "dressed": w.dressed,
                        "tourniquet": w.tourniquet, "splinted": w.splinted})).collect::<Vec<_>>(),
                    "post": post.map(LanRole::label),
                    "working": self.crew_solo_working(role),
                    "body_cleared": self.crew_solo.body_cleared[role_index(role)],
                })
            })
            .collect();
        json!({
            "enabled": self.crew_solo_active(),
            "men": men,
            "aid_box": aid_box_json(&self.crew_solo.aid_box),
            "fire_at_will": self.crew_solo.fire_at_will,
            "gunner_target": self.crew_solo.gunner_target,
            "driver_cmd": [self.crew_solo.driver_cmd.x, self.crew_solo.driver_cmd.y],
            "shots_by_gunner": self.crew_solo.shots_by_gunner,
            "loads_by_loader": self.crew_solo.loads_by_loader,
            "treatments": self.crew_solo.treatments,
            "procedures": self.crew_solo.procedures_done,
            "empties_rack": self.empties_rack_receipt(),
            "empties_dump": self.crew_solo_dump_receipt(),
            "player_station": format!("{:?}", self.station),
            "log": self.crew_solo.log,
        })
    }

    // ── Headless hooks ──────────────────────────────────────────────────────────────────────

    pub fn debug_crew_solo_set_fire_at_will(&mut self, on: bool) {
        self.crew_solo.fire_at_will = on;
    }

    pub fn debug_crew_solo_fire_command(&mut self) {
        self.crew_solo.fire_command = true;
    }

    /// Inject a wound directly (bypasses the ballistic solver; the receipt says so).
    pub fn debug_crew_solo_wound(&mut self, role: CrewRole, region: Region, severity: f32) {
        self.crew_solo.man_mut(role).add_wound(Wound::new(WoundKind::Bleeding, region, severity));
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "injected_wound": role.name(),
            "region": region.label(), "severity": severity}));
        self.crew_solo.say(format!("{}: hit, {}.", role_short(role), region.label()));
    }

    /// Fixture: a Tiger late in its run, with only `count` rounds left in the rearmost stowage
    /// (farthest from the hull front and the breech). The receipt names it. The live solver
    /// still owns every fragment: with a full load, every wounding lane tried through a plate
    /// into a seated man found a round and the Tiger went up.
    pub fn debug_crew_solo_ammunition_left(&mut self, count: usize) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.slot_taken.len()).collect();
        order.sort_by(|&a, &b| {
            let pa = crate::interior_rig::SHELL_SLOTS[a].1;
            let pb = crate::interior_rig::SHELL_SLOTS[b].1;
            pa.z.total_cmp(&pb.z).then(a.cmp(&b))
        });
        let keep: Vec<usize> = order.into_iter().take(count).collect();
        for (i, taken) in self.slot_taken.iter_mut().enumerate() {
            *taken = !keep.contains(&i);
        }
        keep
    }

    /// Why the crew hover did or did not pick something: the eye, the look, every candidate's
    /// distance and cosine, and the ordinary hover state that can pre-empt it.
    pub fn debug_crew_solo_hover_diag(&self, eye: Vec3, fwd: Vec3) -> Value {
        let me = self.solo_identity();
        let mut candidates = Vec::new();
        for role in CrewRole::ALL {
            if Some(role) == me {
                continue;
            }
            let alive = singleplayer_crew::role_alive(&self.crew, role) && !self.crew_solo.man(role).dead;
            let point = if alive {
                let post = self.crew_solo_post(role);
                crew_live_model(&self.player, post).transform_point3(Vec3::new(0.0, crew_live_torso_y(post), 0.0))
            } else {
                let identity = singleplayer_crew::body_post(role);
                match self.couch_ragdolls.iter().find(|b| b.casualty.identity == identity) {
                    Some(body) => blood_parent_model(&self.player, body.frame)
                        .transform_point3(crew_physics::center_of_mass(&body.segments)),
                    None => continue,
                }
            };
            let to = point - eye;
            candidates.push(json!({"role": role.name(), "alive": alive, "point": point.to_array(),
                "distance": to.length(), "cos": to.normalize_or_zero().dot(fwd),
                "cleared": self.crew_solo.body_cleared[role_index(role)]}));
        }
        let anchor = self.player.hull_model().transform_point3(DRIVER_SEAT_ANCHOR_HULL);
        let to_anchor = anchor - eye;
        json!({"eye": eye.to_array(), "fwd": fwd.to_array(), "station": format!("{:?}", self.station),
            "hover": self.hover.map(|h| format!("{h:?}")), "hover_station": self.hover_station.map(|s| format!("{s:?}")),
            "hover_label": self.hover_label, "hover_lever": self.hover_lever, "switch_t": self.switch_t,
            "activity": self.activity.is_some(), "walk_pos": self.walk_pos.to_array(),
            "driver_seat_anchor": {"distance": to_anchor.length(), "cos": to_anchor.normalize_or_zero().dot(fwd),
                "blocked_by": self.crew_solo_station_blocked(Station::Driver).map(|r| r.name())},
            "candidates": candidates})
    }

    /// Headless fixture: switch an enemy's activity (the proof keeps a queue of range targets
    /// out of the fight until each is needed).
    pub fn debug_crew_solo_set_enemy_active(&mut self, index: usize, active: bool) {
        if let Some(slot) = self.enemy_active.get_mut(index) {
            *slot = active;
        }
    }

    /// Modules out of action and the engine state (headless receipts).
    pub fn debug_crew_solo_modules(&self) -> Value {
        json!({"engine_on": self.engine_on, "down": self.modules.iter().filter(|m| !m.functional)
            .map(|m| m.kind.name()).collect::<Vec<_>>()})
    }

    pub fn debug_rounds_spent(&self) -> u32 {
        self.rounds_spent
    }

    /// Every gate between the player's trigger at the sights and a discharge (headless receipts).
    pub fn debug_crew_solo_fire_gates(&self) -> Value {
        json!({"station": format!("{:?}", self.station), "switch_t": self.switch_t,
            "activity": self.activity.is_some(), "hover_station": self.hover_station.map(|s| format!("{s:?}")),
            "hover_hatch": self.hover_hatch, "primary_pickup_press": self.primary_pickup_press,
            "prev_lmb": self.prev_lmb, "lever_open": self.lever_open, "breech": format!("{:?}", self.breech),
            "tank_can_fire": self.player.can_fire(), "gun_ok": self.module_ok(ModuleKind::MainGun),
            "crew_ok_gunner": self.crew_ok(CrewRole::Gunner), "player_can_fire": self.player_can_fire(),
            "carrying_track_kit": self.carrying_track_kit, "panzerfaust": self.panzerfaust_claimed,
            "compass": self.compass_equipped, "map": self.map_held, "sidearm": self.sidearm_is_drawn(),
            "primer_trigger_pulls": self.cartridge_primers.trigger_pulls})
    }

    /// Whether the gunner could see enemy `index` from the muzzle: (through the medium, clear
    /// of terrain, cover and trees).
    pub fn debug_crew_solo_target_visible(&self, index: usize) -> (bool, bool) {
        let Some(enemy) = self.enemies.get(index) else { return (false, false); };
        let from = self.player.muzzle().0;
        let to = enemy.center();
        (self.weather_view.recognizable(from, to), self.los_clear(from, to))
    }

    pub fn debug_crew_solo_man(&self, role: CrewRole) -> &Casualty {
        self.crew_solo.man(role)
    }

    pub fn debug_crew_solo_aid_box(&self) -> AidBox {
        self.crew_solo.aid_box
    }

    pub fn debug_crew_solo_hover(&self) -> Option<String> {
        self.crew_solo.hover.map(|h| format!("{h:?}"))
    }

    pub fn debug_crew_solo_action_progress(&self) -> Option<f32> {
        self.crew_solo.action.map(|(_, t, _)| t)
    }

    /// The procedure in the hands, as the harness drives it: the man, the step's kind and
    /// label, how far along, and the quality so far.
    pub fn debug_crew_solo_procedure(&self) -> Option<Value> {
        let (role, run, _) = self.crew_solo.procedure.as_ref()?;
        let step = run.current()?;
        let (kind, needed) = match step.kind {
            first_aid::StepKind::Hold(s) => ("Hold", s),
            first_aid::StepKind::Press => ("Press", 1.0),
            first_aid::StepKind::Wrap(n) => ("Wrap", f32::from(n)),
            first_aid::StepKind::Turn(n) => ("Turn", f32::from(n)),
            first_aid::StepKind::Watch(s) => ("Watch", s),
        };
        Some(json!({"man": role.name(), "procedure": run.procedure.label(), "improvised": run.improvised,
            "step": run.step, "steps": run.steps().len(), "kind": kind, "label": step.label,
            "needed": needed, "count": run.count, "progress": run.progress, "quality": run.quality}))
    }

    /// Every man's body as the renderer sees it: alive at which post (or standing by), or a
    /// casualty ragdoll (settled, where), and the surface blood that has reached the cabin.
    pub fn debug_crew_solo_bodies(&self) -> Value {
        let me = self.solo_identity();
        let men: Vec<Value> = CrewRole::ALL.iter().map(|&role| {
            let alive = singleplayer_crew::role_alive(&self.crew, role) && !self.crew_solo.man(role).dead;
            let identity = singleplayer_crew::body_post(role);
            let ragdoll = self.couch_ragdolls.iter().find(|b| b.casualty.identity == identity).map(|body| {
                let center = blood_parent_model(&self.player, body.frame)
                    .transform_point3(crew_physics::center_of_mass(&body.segments));
                json!({"settled": body.settled, "age": body.age, "center": center.to_array(),
                    "frame": format!("{:?}", body.frame), "post_at_death": body.casualty.role.label()})
            });
            let post = if Some(role) == me { self.solo_player_post() } else { self.crew_solo_post(role) };
            let mut casualty = singleplayer_crew::casualty_identity(role);
            casualty.role = post;
            json!({"role": role.name(), "player": Some(role) == me, "alive": alive,
                "post": post.label(), "standing_by": alive && self.crew_solo_standby_offset(casualty).is_some(),
                "point": alive.then(|| self.crew_solo_man_point(role).to_array()),
                "ragdoll": ragdoll, "body_cleared": self.crew_solo.body_cleared[role_index(role)],
                "bleed_l_per_min": self.crew_solo.man(role).total_bleed_rate_l_per_s() * 60.0})
        }).collect();
        json!({"men": men, "casualty_records": self.couch_casualties.len(), "ragdolls": self.couch_ragdolls.len(),
            "surface_blood": self.debug_surface_blood(), "intercom": self.crew_solo.intercom_line(),
            "crew_ko_t": self.crew_ko_t})
    }

    pub fn debug_crew_solo_procedures_done(&self) -> Vec<Value> {
        self.crew_solo.procedures_done.clone()
    }

    /// Stage a wound of a given kind directly (bypasses the ballistic solver; the receipt says so).
    pub fn debug_crew_solo_stage_wound(&mut self, role: CrewRole, kind: WoundKind, region: Region, severity: f32) {
        self.crew_solo.man_mut(role).add_wound(Wound::new(kind, region, severity));
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "staged_wound": role.name(),
            "kind": format!("{kind:?}"), "region": region.label(), "severity": severity}));
    }

    pub fn debug_crew_solo_body_cleared(&self, role: CrewRole) -> bool {
        self.crew_solo.body_cleared[role_index(role)]
    }

    pub fn debug_crew_solo_working(&self, role: CrewRole) -> bool {
        self.crew_solo_working(role)
    }

    /// What the renderer will draw of the crew this frame: every living-figure slot (seated and
    /// standing pools, one per man) that is visible, with the man it belongs to and where its
    /// origin stands in the turret's frame. Read from the renderer, not from the crew model.
    pub fn debug_crew_solo_visible_figures(&self, r: &Renderer) -> Value {
        let names = ["commander", "gunner", "driver", "loader", "radio-operator"];
        let inverse = self.player.turret_model().inverse();
        let mut figures = Vec::new();
        for (pool, slots) in [("seated", &self.slots.lan_crew), ("standing", &self.slots.commander_crew)] {
            for (identity, &slot) in slots.iter().enumerate() {
                if let Some((true, model)) = r.debug_mesh_state(slot) {
                    let origin = inverse.transform_point3(model.transform_point3(Vec3::ZERO));
                    let cupola = Vec2::new(origin.x, origin.z).distance(Vec2::new(COMMANDER_POST_TURRET_LOCAL.x, COMMANDER_POST_TURRET_LOCAL.z));
                    figures.push(json!({"man": names[identity], "pool": pool,
                        "turret_local": origin.to_array(), "from_cupola_post_m": cupola}));
                }
            }
        }
        json!(figures)
    }

    /// Walk the interior body to a floor spot in small steps through the ordinary floor/breech
    /// clamp, routing round the breech by the side corridor when the straight line is blocked.
    /// The result is a reachable standing spot, never a teleport through the gun.
    fn debug_walk_to(&mut self, target: Vec3) {
        let step_toward = |from: Vec3, to: Vec3| -> Vec3 {
            let mut at = from;
            for _ in 0..80 {
                let delta = Vec3::new(to.x - at.x, 0.0, to.z - at.z);
                if delta.length() < 0.02 {
                    break;
                }
                let next = at + delta.clamp_length_max(0.08);
                let moved = clamp_interior_walk(at, Vec3::new(next.x, at.y, next.z));
                if moved.distance(at) < 1.0e-4 {
                    break;
                }
                at = moved;
            }
            at
        };
        let goal = Vec3::new(target.x, self.walk_pos.y, target.z);
        let direct = step_toward(self.walk_pos, goal);
        if direct.distance(goal) < 0.06 {
            self.walk_pos = direct;
            return;
        }
        // Round the breech: out to the corridor on the goal's side, along it, then in.
        let side = if goal.x.abs() > 0.3 { goal.x.signum() } else if self.walk_pos.x.abs() > 0.05 { self.walk_pos.x.signum() } else { 1.0 };
        let corridor_x = side * 0.60;
        let mut at = self.walk_pos;
        for waypoint in [
            Vec3::new(corridor_x, at.y, at.z),
            Vec3::new(corridor_x, at.y, goal.z),
            goal,
        ] {
            at = step_toward(at, waypoint);
        }
        self.walk_pos = at;
    }

    /// Walk the interior body next to a man (or his body) and face him. The walk is clamped by
    /// the ordinary floor/breech limits, so the result is a reachable standing spot, not a
    /// teleport into him.
    pub fn debug_crew_solo_go_to(&mut self, role: CrewRole) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        let turret = self.player.turret_model();
        let alive = singleplayer_crew::role_alive(&self.crew, role) && !self.crew_solo.man(role).dead;
        let (target_world, post) = if alive {
            (self.crew_solo_man_point(role), self.crew_solo_post(role))
        } else {
            let identity = singleplayer_crew::body_post(role);
            let Some(body) = self.couch_ragdolls.iter().find(|b| b.casualty.identity == identity) else {
                return false;
            };
            (blood_parent_model(&self.player, body.frame)
                .transform_point3(crew_physics::center_of_mass(&body.segments)), body.casualty.role)
        };
        let local = turret.inverse().transform_point3(target_world);
        // Stand a little short of him, on the floor, from the breech-free side of his post.
        let stand = match post {
            LanRole::Driver | LanRole::Radio => Vec3::new(local.x.clamp(-0.6, 0.6), 0.0, 0.62),
            LanRole::Gunner => Vec3::new(0.62, 0.0, -0.25),
            LanRole::Loader => Vec3::new(-0.45, 0.0, -0.40),
            LanRole::Commander => Vec3::new(0.55, 0.0, -0.40),
        };
        self.debug_walk_to(stand);
        self.loader_face_world(target_world);
        true
    }

    /// Walk to the front of the basket floor and face the driver's seat anchor.
    pub fn debug_crew_solo_face_driver_seat(&mut self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.debug_walk_to(Vec3::new(0.50, 0.0, 0.62));
        let anchor = self.player.hull_model().transform_point3(DRIVER_SEAT_ANCHOR_HULL);
        self.loader_face_world(anchor);
        true
    }

    /// Face the first-aid box from the interior walk.
    pub fn debug_crew_solo_go_to_aid_box(&mut self) -> bool {
        if self.mode != Mode::Tank || self.station != Station::Loader {
            return false;
        }
        self.debug_walk_to(Vec3::new(-0.40, 0.0, -0.45));
        let point = self.player.turret_model().transform_point3(AID_BOX_TURRET_LOCAL);
        self.loader_face_world(point);
        true
    }

    /// Headless staging: kill a man outright through the same casualty ledger, ragdoll and blood
    /// path a death in the compartment takes, without a penetration (the koop proof stages its
    /// KIAs the same way). Used where a live cone would also wreck the gun or the drivetrain the
    /// proof goes on to use; receipts name it.
    pub fn debug_crew_solo_kia(&mut self, role: CrewRole) -> bool {
        if Some(role) == self.solo_identity()
            || !singleplayer_crew::role_alive(&self.crew, role)
            || self.crew_solo.man(role).dead
        {
            return false;
        }
        let post = self.crew_solo_post(role);
        let mut casualty = singleplayer_crew::casualty_identity(role);
        casualty.role = post;
        let side = self.player.hull_model().transform_vector3(Vec3::new(-0.8, 0.15, -0.5)).normalize_or_zero();
        self.crew_ko_t = CREW_KO_DUR;
        self.crew_ko_world_dir = side;
        let point = self.crew_solo_man_point(role);
        let seed = ((self.crew_solo.time_s * 61.0) as u32) ^ 0x57A6_ED00 ^ role_index(role) as u32;
        self.crew_solo_record_kia(casualty);
        self.record_physical_crew_casualties(&[casualty]);
        self.spawn_couch_ragdoll(casualty, side, 0.5, seed);
        self.spawn_cubic_blood(casualty, Some(point), side, Vec3::ZERO, 0.8, 1.2, seed ^ 0xB10D, role_index(role), 192);
        self.crew_solo.log.push(json!({"t": self.crew_solo.time_s, "staged_kia": role.name(), "post": post.label()}));
        println!("[panzerrechnersystem] CREWMAN KIA (staged) — {} at {}", role.name(), post.label());
        true
    }

    /// Strike a crewman with a real penetration: the live armor solve, fragment cloud, anatomy
    /// and retained-wound path, aimed at his seat through the plate `arc` names (the side
    /// nearest him, or the rear), `aim_down_m` below his torso centre (legs for a wounding
    /// shot). Returns a receipt with every attempt and what the solver produced.
    pub fn debug_crew_solo_strike(&mut self, role: CrewRole, lethal: bool, arc: Arc, aim_down_m: f32) -> Value {
        self.debug_crew_solo_strike_raked(role, lethal, arc, aim_down_m, 0.0)
    }

    /// As `debug_crew_solo_strike`, with the shot raked `rake_deg` toward the rear in the
    /// horizontal plane (a side shot that has passed the man goes on behind the breech or the
    /// transmission instead of into them). The rake is the plate obliquity the armour solve sees.
    pub fn debug_crew_solo_strike_raked(&mut self, role: CrewRole, lethal: bool, arc: Arc, aim_down_m: f32, rake_deg: f32) -> Value {
        let post = self.crew_solo_post(role);
        let frame = blood_frame_for_role(post);
        let parent = blood_parent_model(&self.player, frame);
        let torso_world = self.crew_solo_man_point(role);
        let torso_local = parent.inverse().transform_point3(torso_world) - Vec3::Y * aim_down_m;
        let (part, bounds) = match frame {
            BloodFrame::Turret => (ArmorPart::Turret, rig_armor_bounds(crate::ballistics::ArmorScheme::TigerI, ArmorPart::Turret)),
            BloodFrame::Hull => (ArmorPart::Hull, rig_armor_bounds(crate::ballistics::ArmorScheme::TigerI, ArmorPart::Hull)),
        };
        let Some((min, max)) = bounds else {
            return json!({"error": "no armor bounds"});
        };
        let arc = if arc == Arc::Front { Arc::Side } else { arc };
        // A wounding round comes in low and rises through the man, so what goes on past him
        // climbs away from the sponson racks instead of into them (a side hit at rack height
        // is the Tiger's own catastrophic case, which this proof is not about).
        let rise = if lethal { 0.0 } else { 0.30 };
        let rake = rake_deg.to_radians();
        let entry_local = match arc {
            // Through the side plate nearest the man, raked toward the rear by `rake`.
            Arc::Side => {
                let from_right = torso_local.x < 0.0;
                let plate_x = if from_right { min.x } else { max.x };
                let run = (plate_x - torso_local.x).abs() / rake.cos().max(0.2);
                Vec3::new(plate_x, torso_local.y - rise, torso_local.z + run * rake.sin())
            }
            // Through the rear plate behind him.
            _ => Vec3::new(torso_local.x, torso_local.y - rise, min.z),
        };
        let axis_local = (torso_local - entry_local).normalize_or_zero();
        let entry_world = parent.transform_point3(entry_local);
        let dir_world = parent.transform_vector3(axis_local).normalize_or_zero();
        let shooter_at = |range: f32| entry_world - dir_world * range;
        let facing = facing_from(arc, part == ArmorPart::Turret);
        let plate = crate::ballistics::ArmorScheme::TigerI.plate(facing);
        // A killing shot is a solid APCR slug first, from close: its energy is the man's death,
        // and without a filler it does not fill the compartment with a burst that finds the
        // stowage. A wounding shot is solid only (a filled round's burst inside the hull or
        // turret is lethal to every man near it), from farther out, coming closer until the
        // plate gives.
        let shots: &[(&'static crate::ballistics::ShellSpec, f32)] = if lethal {
            &[(&crate::ballistics::BR_354P, 60.0), (&crate::ballistics::BR_412, 60.0), (&crate::ballistics::BR_471, 60.0)]
        } else {
            &[(&crate::ballistics::BR_354P, 350.0), (&crate::ballistics::BR_354P, 200.0),
              (&crate::ballistics::BR_354P, 100.0), (&crate::ballistics::BR_354P, 40.0)]
        };
        let mut attempts = Vec::new();
        let breech_before = format!("{:?}", self.breech);
        let live_before = self.live_ammunition_in_frame(BloodFrame::Hull).len()
            + self.live_ammunition_in_frame(BloodFrame::Turret).len();
        self.god = false;
        for attempt in 0..16u32 {
            let seed = 0xC5E0_0000u32.wrapping_add(attempt).wrapping_add(role_index(role) as u32 * 0x100);
            let (shell, range) = shots[(attempt as usize).min(shots.len() * 4 - 1) / 4 % shots.len()];
            let shooter = shooter_at(range);
            let event = crate::ballistics::resolve(shell, range, plate, rake_deg.abs(), seed);
            if !matches!(event.terminal, crate::ballistics::Terminal::Penetration) {
                attempts.push(json!({"attempt": attempt, "shell": shell.name, "range_m": range, "terminal": format!("{:?}", event.terminal)}));
                continue;
            }
            self.armor_scars.record(facing, entry_local, &event);
            self.stat_hits_on_player = self.stat_hits_on_player.saturating_add(1);
            self.stat_pens_on_player = self.stat_pens_on_player.saturating_add(1);
            self.resolve_player_penetration(
                arc,
                shooter,
                entry_world,
                dir_world,
                part,
                entry_local,
                0xC5E0_0000 + u64::from(attempt),
                seed,
                event,
            );
            let man = self.crew_solo.man(role);
            let dead = !singleplayer_crew::role_alive(&self.crew, role) || man.dead;
            attempts.push(json!({"attempt": attempt, "shell": shell.name, "range_m": range, "terminal": "Penetration",
                "dead": dead, "wounded": man.is_wounded(), "state": man.state_label()}));
            let done = if lethal { dead } else { man.is_wounded() || dead };
            if done || !self.player.alive {
                break;
            }
        }
        let man = self.crew_solo.man(role);
        json!({
            "role": role.name(), "post": post.label(), "lethal_intent": lethal,
            "breech_before": breech_before, "live_rounds_before": live_before,
            "arc": format!("{arc:?}"), "aim_down_m": aim_down_m, "rake_deg": rake_deg,
            "entry_local": entry_local.to_array(), "attempts": attempts,
            "result": man.state_label(), "dead": !singleplayer_crew::role_alive(&self.crew, role) || man.dead,
            "player_alive": self.player.alive,
            "method": "live armor solve, fragment cloud, anatomy; seeds iterated until the intended class of outcome",
        })
    }
}

fn aid_box_json(aid: &AidBox) -> Value {
    json!({"dressings": aid.dressings, "tourniquets": aid.tourniquets,
        "morphine": aid.morphine, "splints": aid.splints, "burn_dressings": aid.burn_dressings,
        "triangular": aid.triangular, "canteen": aid.canteen, "scissors": aid.scissors})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slump_grows_with_hurt_and_is_identity_for_a_fit_man() {
        let fit = Casualty::default();
        assert_eq!(slump_pose(&fit), Mat4::IDENTITY);
        let mut hurt = Casualty::default();
        hurt.add_wound(Wound::new(WoundKind::Fracture, Region::Arm, 0.8));
        let head = Vec3::new(0.0, 1.3, 0.0);
        let slumped = slump_pose(&hurt).transform_point3(head);
        assert!(slumped.y < head.y, "a hurt man's head drops");
        assert!(slumped.z.abs() > 0.05, "and comes forward");
        let mut out = Casualty::default();
        out.knock_out(30.0);
        let out_head = slump_pose(&out).transform_point3(head);
        assert!(out_head.y < slumped.y, "an unconscious man slumps further");
        let mut dead = Casualty::default();
        dead.dead = true;
        assert_eq!(slump_pose(&dead), Mat4::IDENTITY, "a dead man is a ragdoll, not a slump");
    }

    #[test]
    fn every_role_has_an_offer_label_for_every_procedure_and_kit_state() {
        for role in CrewRole::ALL {
            for p in Procedure::ALL {
                assert!(offer_label(role, p, Some(false)).contains("[E]"));
                assert!(offer_label(role, p, Some(true)).contains("IMPROVISED"));
                assert!(!offer_label(role, p, None).contains("[E]"), "no prompt without kit");
                assert!(std::ptr::eq(offer_label(role, p, Some(false)), offer_label(role, p, Some(false))), "interned once");
            }
            assert!(clear_label(role).contains("[HOLD E]"));
            assert!(!blocked_label(role).is_empty());
        }
    }

    #[test]
    fn empties_rack_fills_only_real_stands_in_order_and_knows_when_it_is_full() {
        let mut rack = EmptiesRack::default();
        assert!(!rack.probed());
        assert_eq!(rack.free_slot(), None, "nothing can be stowed before the probe");
        let mut poses = vec![None; EMPTIES_RACK_SLOTS];
        for i in [1usize, 2, 5] {
            poses[i] = Some((Vec3::new(i as f32 * 0.1, 0.9, -1.3), Quat::IDENTITY));
        }
        rack.poses = Some(poses);
        assert_eq!(rack.free_slot(), Some(1), "slot 0 has no stand on the probed wall");
        rack.slots[1] = Some(40);
        assert_eq!(rack.free_slot(), Some(2));
        rack.slots[2] = Some(41);
        rack.slots[5] = Some(42);
        assert!(rack.is_full());
        assert_eq!(rack.count(), 3);
        assert!(rack.pose(0).is_none() && rack.pose(5).is_some());
    }

    #[test]
    fn regions_cover_every_body_part() {
        for part in crew_physics::CrewBodyPart::ALL {
            let _ = region_of_part(part);
        }
        assert_eq!(region_of_part(crew_physics::CrewBodyPart::LeftShin), Region::Leg);
        assert_eq!(region_of_part(crew_physics::CrewBodyPart::Head), Region::Head);
        for post in LanRole::ALL {
            assert_eq!(singleplayer_crew::body_post(role_of_post(post)), post);
        }
        for role in CrewRole::ALL {
            assert_eq!(singleplayer_crew::role_for_station(station_for_role(role)), role);
        }
    }
}
