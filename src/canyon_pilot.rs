//! Explicit offscreen AI test driver, never installed in human play. Route knowledge comes from
//! the paper-map course. Contacts use geometry-based sight within 420 m; no hidden-contact hunt.
//! It supplies ordinary drive/mouse/trigger input and native station actions. Reloading is the
//! physical interior walk, lever, casing, rack and ram sequence from the player-input showcase,
//! driven stage by stage: every press waits for the crosshair prompt a player would read first.
use super::*;

/// One physical stage of the loader's reload. A stage ends on the game's own state (lever
/// position, breech contents, hands, seat), never on a clock; the clock only bounds a retry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ReloadStage {
    /// Around the back of the breech to the loader's side of the gun.
    Walk,
    /// One click on the lever prompt opens the block.
    OpenLever,
    /// Hold the hot case until it comes out in the hands.
    PullCase,
    /// Turn away from the breech and let the case go behind the loader, clear of every sight
    /// line the rest of the cycle needs.
    CarryCaseAway,
    /// Press on a ready-rack round and keep hold of it.
    GrabRound,
    /// Carry the round to the tray mouth and flick it home.
    Ram,
    /// One click on the lever prompt closes the block on the live round.
    CloseLever,
    /// Back around the breech to the gunner's side.
    WalkBack,
    /// Click the gunner's seat.
    Seat,
}

impl ReloadStage {
    fn label(self) -> &'static str {
        match self {
            ReloadStage::Walk => "reload_walk",
            ReloadStage::OpenLever => "reload_open_lever",
            ReloadStage::PullCase => "reload_pull_case",
            ReloadStage::CarryCaseAway => "reload_carry_case",
            ReloadStage::GrabRound => "reload_grab_round",
            ReloadStage::Ram => "reload_ram",
            ReloadStage::CloseLever => "reload_close_lever",
            ReloadStage::WalkBack => "reload_walk_back",
            ReloadStage::Seat => "reload_seat",
        }
    }
    /// How long a stage may wait for its prompt before the cycle is retried from the walk.
    fn patience_s(self) -> f32 {
        match self {
            ReloadStage::Walk | ReloadStage::WalkBack => 14.0,
            ReloadStage::Ram => 16.0,
            _ => 10.0,
        }
    }
}

/// Small crosshair offsets (turret-local metres) tried in turn when the prompt under the
/// crosshair is not the one the stage needs: a case lying across the sight line, a rack round
/// behind the lever. The lever and seat cones are wide; the case grip is tight, so it uses the
/// smaller set.
const NUDGES_WIDE: [Vec3; 7] = [
    Vec3::ZERO,
    Vec3::new(0.0, 0.10, 0.0),
    Vec3::new(0.0, -0.10, 0.0),
    Vec3::new(0.0, 0.0, 0.10),
    Vec3::new(0.0, 0.0, -0.10),
    Vec3::new(0.0, 0.10, 0.10),
    Vec3::new(0.0, -0.10, -0.10),
];
const NUDGES_TIGHT: [Vec3; 5] = [
    Vec3::ZERO,
    Vec3::new(0.0, 0.035, 0.0),
    Vec3::new(0.0, -0.035, 0.0),
    Vec3::new(0.0, 0.0, 0.035),
    Vec3::new(0.0, 0.0, -0.035),
];
/// Where the loader looks while letting the spent case go: the rear-left corner of the basket
/// floor, behind him and away from the breech, the lever and the ready rack.
const CASE_DROP_LOOK: Vec3 = Vec3::new(-0.75, 1.15, -0.80);
/// Frames of the ram flick (8 px per frame at 60 Hz is a 480 px/s stroke, over the gate).
const RAM_FLICK_FRAMES: u32 = 6;
/// Whole-cycle retries before the pilot gives up on a reload.
const RELOAD_RETRIES: u32 = 2;
/// After an abandoned reload the crew drives on for this long before the loader tries again
/// (the turret has turned and other rack slots may be in reach by then).
const RELOAD_REST_S: f32 = 45.0;
/// How long the gunner stays on a contact that has slipped out of sight before the crew goes
/// back to driving (the station changes each way cost seconds and a walk position).
const ENGAGE_HOLD_S: f32 = 3.0;
/// Beyond this angle off the bore the hull pivots toward the contact instead of waiting for the
/// 6 deg/s traverse alone.
const ENGAGE_PIVOT_RAD: f32 = 0.35;
/// Knocked-out hull bypass (`hull_bypass`): how far ahead along the route a dead hull is looked
/// for, how close to the travel line it must lie to block, the clearance kept from every hull, and
/// how far behind the Tiger a passed hull is let go.
const HULL_BYPASS_AHEAD_M: f32 = 26.0;
const HULL_BYPASS_LANE_M: f32 = 6.0;
const HULL_BYPASS_CLEAR_M: f32 = 5.0;
const HULL_BYPASS_BEHIND_M: f32 = 4.0;
/// The Tiger's half width plus a little: the street floor grid is measured for a T-34.
const TIGER_HALF_WIDTH_M: f32 = 2.0;

pub struct CanyonPilot {
    route: Vec<Vec2>,
    cursor: usize,
    mouse_velocity: Vec2,
    reload_at: Option<f32>,
    reload_waypoint: usize,
    stage: ReloadStage,
    stage_at: f32,
    stage_tries: u32,
    nudge: usize,
    nudge_at: f32,
    flick_frames: u32,
    flick_at: f32,
    primary_held: bool,
    last_check: f32,
    check_pos: Vec2,
    reverse_until: f32,
    /// Route watchdog: the farthest waypoint reached and when, plus the escalating
    /// alternating-side detour state used to leave a bump-and-retreat loop.
    best_cursor: usize,
    best_cursor_at: f32,
    stuck_count: u32,
    avoid_until: f32,
    avoid_side: f32,
    was_advancing: bool,
    /// The dead hull being passed and the aim point beside and beyond it.
    bypass: Option<(Vec2, Vec2)>,
    /// Rack slot the loader is reaching for, and the slots whose prompt never came this reload.
    round_slot: Option<usize>,
    skip_slots: Vec<usize>,
    /// When the last reload was given up and the crew drove on with the gun unready.
    reload_abandoned_at: Option<f32>,
    /// The last contact actually seen and when: the gunner keeps his seat and his lay on it for
    /// a few seconds after it slips out of sight instead of climbing back to the cupola at once.
    last_target: Option<(Vec3, Option<usize>)>,
    last_target_at: f32,
    debug: bool,
    pub reloads: u32,
    pub phase: &'static str,
}

impl CanyonPilot {
    pub fn new(g: &Game) -> Self {
        let route = g
            .canyon_spec
            .as_ref()
            .expect("Canyon AI needs the real course")
            .route
            .clone();
        Self::from_route(route, Vec2::new(g.player.pos.x, g.player.pos.z))
    }

    fn from_route(route: Vec<Vec2>, pos: Vec2) -> Self {
        assert!(!route.is_empty(), "Canyon pilot needs a route");
        let cursor = (0..route.len())
            .min_by(|&a, &b| {
                route[a]
                    .distance_squared(pos)
                    .total_cmp(&route[b].distance_squared(pos))
            })
            .unwrap();
        Self {
            route,
            cursor,
            mouse_velocity: Vec2::ZERO,
            reload_at: None,
            reload_waypoint: 0,
            stage: ReloadStage::Walk,
            stage_at: 0.0,
            stage_tries: 0,
            nudge: 0,
            nudge_at: 0.0,
            flick_frames: 0,
            flick_at: -10.0,
            primary_held: false,
            last_check: 0.0,
            check_pos: pos,
            reverse_until: 0.0,
            best_cursor: cursor,
            best_cursor_at: 0.0,
            stuck_count: 0,
            avoid_until: 0.0,
            avoid_side: -1.0,
            was_advancing: false,
            bypass: None,
            round_slot: None,
            skip_slots: Vec::new(),
            reload_abandoned_at: None,
            last_target: None,
            last_target_at: -100.0,
            debug: std::env::var("KESSEL_PILOT_DBG").is_ok(),
            reloads: 0,
            phase: "approach",
        }
    }

    /// Escalate out of a blocked spot: back off in an arc, then approach past the obstruction
    /// through a perpendicular offset carrot. Sides alternate and both the reverse and the
    /// detour grow with each consecutive failure; any real route progress resets the ladder.
    fn trigger_avoid(&mut self, t: f32) {
        self.stuck_count = (self.stuck_count + 1).min(6);
        self.avoid_side = -self.avoid_side;
        let grade = self.stuck_count.min(4) as f32;
        self.reverse_until = t + 2.5 + 0.7 * grade;
        self.avoid_until = self.reverse_until + 8.0 + 2.5 * grade;
        self.best_cursor_at = t;
        // A bypass that ended in a stall is re-chosen (with the side preference flipped).
        self.bypass = None;
    }

    /// Knocked-out hulls are the obstruction the authored route cannot know about: the defence
    /// dies in the hamlet's 20 m street (Canyon v2 seed 29, 2026-10-27: dead T-34s 7, 60 and 100 m
    /// along the frontage, and the perpendicular detour ladder ran the hull into the houses). A
    /// dead hull within `HULL_BYPASS_LANE_M` of the travel line in the next `HULL_BYPASS_AHEAD_M`
    /// is passed on whichever side the street's measured floor grid shows clear for the Tiger's
    /// width, `HULL_BYPASS_CLEAR_M` from every hull; the choice holds until the hull is behind.
    /// Route knowledge only: the hulls are where the crew watched them die.
    fn hull_bypass(&mut self, g: &Game, here: Vec2) -> Option<Vec2> {
        let n = self.route.len();
        let a = self.route[self.cursor.min(n - 1)];
        let b = self.route[(self.cursor + 3).min(n - 1)];
        let along = (b - a).normalize_or_zero();
        if along == Vec2::ZERO {
            return None;
        }
        let perp = Vec2::new(-along.y, along.x);
        let hulls: Vec<Vec2> = g.enemies.iter().enumerate()
            .filter(|(i, e)| !e.alive && g.enemy_active.get(*i).copied().unwrap_or(true))
            .map(|(_, e)| Vec2::new(e.pos.x, e.pos.z))
            .collect();
        if let Some((hull, carrot)) = self.bypass {
            if (hull - here).dot(along) > -HULL_BYPASS_BEHIND_M {
                return Some(carrot);
            }
            self.bypass = None;
        }
        let blocking = hulls.iter().copied()
            .filter(|h| {
                let rel = *h - here;
                (0.0..HULL_BYPASS_AHEAD_M).contains(&rel.dot(along)) && rel.dot(perp).abs() < HULL_BYPASS_LANE_M
            })
            .min_by(|x, y| (*x - here).dot(along).total_cmp(&(*y - here).dot(along)))?;
        let nav = g.winter_encounter.as_ref().and_then(|w| w.streets()).map(|s| s.nav());
        let lat = (blocking - here).dot(perp);
        let ahead = (blocking - here).dot(along);
        // Pass on the far side of the hull from the travel line; after a stall, try the other first.
        let mut first = if lat >= 0.0 { -1.0 } else { 1.0 };
        if self.stuck_count % 2 == 1 {
            first = -first;
        }
        let seg_dist = |p: Vec2, s0: Vec2, s1: Vec2| {
            let d = s1 - s0;
            let k = ((p - s0).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
            p.distance(s0 + d * k)
        };
        for side in [first, -first] {
            for off in [5.0, 6.5, 8.0, 9.5] {
                let p = here + along * (ahead + 9.0) + perp * (lat + side * off);
                let start = here.lerp(p, 0.3);
                let clear_of_hulls = hulls.iter().all(|h| h.distance(p) >= HULL_BYPASS_CLEAR_M
                    && seg_dist(*h, start, p) >= HULL_BYPASS_CLEAR_M - 1.0);
                let clear_floor = nav.is_none_or(|nav| [-TIGER_HALF_WIDTH_M, 0.0, TIGER_HALF_WIDTH_M]
                    .iter().all(|w| nav.segment_clear(start + perp * *w, p + perp * *w)));
                if clear_of_hulls && clear_floor {
                    self.bypass = Some((blocking, p));
                    return Some(p);
                }
            }
        }
        None
    }

    /// Boxed in among knocked-out hulls (Canyon v2 seed 29, 2026-10-27: three dead T-34s 6-14 m
    /// round the Tiger at (346, 349), vehicle avoidance holding the throttle at zero with a hull
    /// ahead and refusing reverse with one behind): of 16 headings, the one with the longest
    /// corridor the Tiger's width clear of every hull and of the street floor's obstructions, out
    /// to 14 m, favouring the route's direction; the aim point is 10 m along it. None when no
    /// dead hull is within 10 m.
    fn hull_escape(&self, g: &Game, here: Vec2) -> Option<Vec2> {
        let hulls: Vec<Vec2> = g.enemies.iter().enumerate()
            .filter(|(i, e)| !e.alive && g.enemy_active.get(*i).copied().unwrap_or(true))
            .map(|(_, e)| Vec2::new(e.pos.x, e.pos.z))
            .collect();
        if !hulls.iter().any(|h| h.distance(here) < 10.0) {
            return None;
        }
        let n = self.route.len();
        let route_dir = (self.route[(self.cursor + 3).min(n - 1)] - self.route[self.cursor.min(n - 1)]).normalize_or_zero();
        let nav = g.winter_encounter.as_ref().and_then(|w| w.streets()).map(|s| s.nav());
        let mut best: Option<(f32, Vec2)> = None;
        for k in 0..16 {
            let a = k as f32 * std::f32::consts::TAU / 16.0;
            let dir = Vec2::new(a.sin(), a.cos());
            let side = Vec2::new(-dir.y, dir.x);
            // How far the corridor runs before a hull (within the Tiger's half width plus a
            // T-34's) or an obstructed floor cell.
            let mut reach = 14.0f32;
            for h in &hulls {
                let rel = *h - here;
                let along = rel.dot(dir);
                if along > 0.0 && rel.dot(side).abs() < TIGER_HALF_WIDTH_M + 2.2 {
                    reach = reach.min(along - 3.5);
                }
            }
            if let Some(nav) = nav {
                let mut d = 1.0;
                while d <= reach {
                    let p = here + dir * d;
                    if ![-TIGER_HALF_WIDTH_M, 0.0, TIGER_HALF_WIDTH_M].iter().all(|w| nav.is_clear(p + side * *w)) {
                        reach = d - 1.0;
                        break;
                    }
                    d += 1.0;
                }
            }
            if reach < 4.0 {
                continue;
            }
            let score = reach + 4.0 * dir.dot(route_dir);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, here + dir * reach.min(10.0)));
            }
        }
        best.map(|(_, p)| p)
    }

    fn look(&mut self, g: &Game, r: &Renderer, target: Vec3, dt: f32, input: &mut TankInput) {
        let (error, sensitivity) = g.capture_gaze_error(target, r);
        let gain = if g.station == Station::Gunner {
            12.0
        } else {
            5.0
        };
        let desired = (error * gain).clamp(Vec2::splat(-1.9), Vec2::splat(1.9));
        self.mouse_velocity +=
            (desired - self.mouse_velocity).clamp(Vec2::splat(-5.0 * dt), Vec2::splat(5.0 * dt));
        let delta = self.mouse_velocity * dt;
        input.look_dx += delta.x / sensitivity.x;
        input.look_dy += delta.y / sensitivity.y;
    }

    /// The armour-piercing round nearest the loader's eye that has not failed to prompt this
    /// reload (the `rack_ap` gaze target with the failed slots left out).
    fn rack_round(&self, g: &Game) -> Option<(usize, Vec3)> {
        let eye = g.station_eye(Station::Loader);
        let tm = g.player.turret_model();
        crate::interior_rig::SHELL_SLOTS.iter().enumerate()
            .filter(|(i, _)| !g.slot_taken[*i] && !g.slot_nature_he[*i] && !self.skip_slots.contains(i))
            .map(|(i, (frame, pos, _))| {
                let at = if *frame == crate::interior_rig::Frame::Hull {
                    g.player.hull_model().transform_point3(*pos)
                } else {
                    tm.transform_point3(*pos)
                };
                (i, at)
            })
            .min_by(|a, b| a.1.distance_squared(eye).total_cmp(&b.1.distance_squared(eye)))
    }

    fn gaze_point(g: &Game, name: &str) -> Option<Vec3> {
        g.capture_gaze_target(&serde_json::json!({ "gaze": name }))
    }

    /// `KESSEL_PILOT_DBG=1`: the prompt under the crosshair and the breech state at every stage
    /// change and every press, so a lost click can be read from the log.
    fn pilot_log(&self, g: &Game, t: f32, what: &str) {
        if !self.debug {
            return;
        }
        println!(
            "[pilot] t={t:.2} {} {what} hover={:?} seat_hover={:?} lever={:.2} breech={:?} carry={} case={} pull={:.2} ram={:.2} station={:?} walk=({:.2},{:.2})",
            self.stage.label(),
            g.hover_label,
            g.hover_station,
            g.lever_open,
            g.breech,
            g.grab,
            g.carried_casing.is_some(),
            g.casing_pull,
            g.ram,
            g.station,
            g.walk_pos.x,
            g.walk_pos.z,
        );
    }

    fn enter_stage(&mut self, g: &Game, t: f32, stage: ReloadStage) {
        self.pilot_log(g, t, &format!("-> {}", stage.label()));
        self.stage = stage;
        self.stage_at = t;
        self.nudge = 0;
        self.nudge_at = t;
        self.flick_frames = 0;
        self.mouse_velocity = Vec2::ZERO;
        if stage == ReloadStage::WalkBack {
            self.reload_waypoint = 0;
        }
    }

    /// The stage's prompt has not appeared where the crosshair rests: try the next offset.
    fn consider_nudge(&mut self, t: f32, prompt_ok: bool, count: usize) {
        if prompt_ok {
            return;
        }
        if t - self.stage_at > 0.9 && t - self.nudge_at > 0.45 {
            self.nudge = (self.nudge + 1) % count;
            self.nudge_at = t;
        }
    }

    fn nudged(&self, g: &Game, base: Vec3, set: &[Vec3]) -> Vec3 {
        base + g.player.turret_model().transform_vector3(set[self.nudge % set.len()])
    }

    /// A stage waited past its patience: walk the cycle again from the start, a bounded number
    /// of times, then report the stall with the game state in the trace.
    fn stage_timed_out(&mut self, g: &Game, t: f32) -> bool {
        if t - self.stage_at <= self.stage.patience_s() {
            return false;
        }
        self.pilot_log(g, t, "TIMEOUT");
        if self.stage == ReloadStage::GrabRound {
            // That slot never showed its prompt from the loader's post (a hull bin behind the
            // turret basket, typically, once the ready racks are empty): reach for another.
            if let Some(slot) = self.round_slot.take() {
                self.skip_slots.push(slot);
            }
        }
        if self.stage_tries < RELOAD_RETRIES {
            self.stage_tries += 1;
            self.reload_waypoint = if g.walk_pos.x < 0.0 { 2 } else { 0 };
            self.enter_stage(g, t, ReloadStage::Walk);
        } else {
            self.phase = "reload_stalled";
        }
        true
    }

    fn walk_towards(&mut self, g: &Game, target: Vec2, input: &mut TankInput) -> bool {
        let (d, s) = g.capture_walk_input(target);
        input.drive = d;
        input.steer = s;
        // The basket floor clamp can stop the body 5 cm short of a corner waypoint.
        Vec2::new(g.walk_pos.x, g.walk_pos.z).distance(target) < 0.07
    }

    fn reload(&mut self, g: &mut Game, r: &Renderer, t: f32, dt: f32) -> TankInput {
        let mut input = TankInput::default();
        if self.phase == "reload_stalled" {
            // Give the run back to the road rather than freezing at the breech (the missions
            // branch's rule, kept through the stage-machine rewrite): drive on with the gun
            // unready and try again after `RELOAD_REST_S`.
            println!("[canyon-pilot] reload abandoned at {t:.1}s (breech {:?}, lever {:.2}, {} slot(s) out of reach); driving on",
                g.breech, g.lever_open, self.skip_slots.len());
            self.reload_at = None;
            self.reload_abandoned_at = Some(t);
            self.mouse_velocity = Vec2::ZERO;
            self.phase = "advance";
            return input;
        }
        self.phase = self.stage.label();
        let a = t - self.stage_at;
        let hover_lever = g.hover == Some(Hotspot::GunLever);
        match self.stage {
            ReloadStage::Walk => {
                // Walk around the back of the breech before reaching the loader's side. A straight
                // leftward command from the eyepiece is physically blocked by the gun assembly.
                let waypoints = [
                    Vec2::new(0.6, -0.6),
                    Vec2::new(-0.6, -0.6),
                    Vec2::new(-0.6, 0.1),
                ];
                if let Some(at) = Self::gaze_point(g, "cupola") {
                    self.look(g, r, at, dt, &mut input);
                }
                if self.reload_waypoint >= waypoints.len() {
                    self.enter_stage(g, t, ReloadStage::OpenLever);
                } else if self.walk_towards(g, waypoints[self.reload_waypoint], &mut input) {
                    self.reload_waypoint += 1;
                } else {
                    self.stage_timed_out(g, t);
                }
            }
            ReloadStage::OpenLever => {
                if g.lever_open >= LEVER_OPEN {
                    self.enter_stage(g, t, ReloadStage::PullCase);
                } else if let Some(lever) = Self::gaze_point(g, "lever") {
                    let at = self.nudged(g, lever, &NUDGES_WIDE);
                    self.look(g, r, at, dt, &mut input);
                    if hover_lever && !self.primary_held && a > 0.3 {
                        input.mg_coax = true;
                        self.pilot_log(g, t, "CLICK lever (open)");
                    }
                    self.consider_nudge(t, hover_lever, NUDGES_WIDE.len());
                    self.stage_timed_out(g, t);
                }
            }
            ReloadStage::PullCase => {
                if g.carried_casing.is_some() {
                    input.mg_coax = true;
                    self.enter_stage(g, t, ReloadStage::CarryCaseAway);
                } else if matches!(g.breech, Breech::Empty) {
                    self.enter_stage(g, t, ReloadStage::GrabRound);
                } else if matches!(g.breech, Breech::Loaded) {
                    self.enter_stage(g, t, ReloadStage::CloseLever);
                } else if let Some(grip) = Self::gaze_point(g, "breech") {
                    let at = self.nudged(g, grip, &NUDGES_TIGHT);
                    self.look(g, r, at, dt, &mut input);
                    let gripping = g.hover == Some(Hotspot::SpentCasing) || g.casing_pull > 0.0;
                    input.mg_coax = gripping;
                    if gripping && !self.primary_held {
                        self.pilot_log(g, t, "GRIP case");
                    }
                    self.consider_nudge(t, gripping, NUDGES_TIGHT.len());
                    self.stage_timed_out(g, t);
                }
            }
            ReloadStage::CarryCaseAway => {
                if g.carried_casing.is_none() {
                    self.enter_stage(g, t, ReloadStage::GrabRound);
                } else {
                    let at = g.player.turret_model().transform_point3(CASE_DROP_LOOK);
                    self.look(g, r, at, dt, &mut input);
                    input.mg_coax = a < 0.8;
                    if !input.mg_coax {
                        self.pilot_log(g, t, "DROP case");
                    }
                }
            }
            ReloadStage::GrabRound => {
                if g.grab {
                    input.mg_coax = true;
                    self.enter_stage(g, t, ReloadStage::Ram);
                } else if let Some((slot, rack)) = self.rack_round(g) {
                    self.round_slot = Some(slot);
                    let at = self.nudged(g, rack, &NUDGES_WIDE);
                    self.look(g, r, at, dt, &mut input);
                    let on_round = matches!(g.hover, Some(Hotspot::RackSlot(_)));
                    if on_round && !self.primary_held && a > 0.3 {
                        input.mg_coax = true;
                        self.pilot_log(g, t, "GRAB round");
                    }
                    self.consider_nudge(t, on_round, NUDGES_WIDE.len());
                    self.stage_timed_out(g, t);
                } else {
                    // No armour-piercing round left in reach (or in any rack).
                    self.phase = "reload_stalled";
                }
            }
            ReloadStage::Ram => {
                if matches!(g.breech, Breech::Loaded) {
                    self.enter_stage(g, t, ReloadStage::CloseLever);
                } else if !g.grab {
                    self.enter_stage(g, t, ReloadStage::GrabRound);
                } else {
                    input.mg_coax = true;
                    if self.flick_frames > 0 {
                        // The flick itself: a short fast stroke, no gaze correction underneath it.
                        input.look_dy = -8.0;
                        self.flick_frames -= 1;
                    } else {
                        if let Some(mouth) = Self::gaze_point(g, "tray") {
                            self.look(g, r, mouth, dt, &mut input);
                        }
                        if g.ram_ready() && t - self.flick_at > 0.6 {
                            self.flick_frames = RAM_FLICK_FRAMES;
                            self.flick_at = t;
                            self.pilot_log(g, t, "FLICK ram");
                        }
                    }
                    if self.stage_timed_out(g, t) {
                        // Let go of the round before walking off; it stays where it lies.
                        input.mg_coax = false;
                    }
                }
            }
            ReloadStage::CloseLever => {
                if g.lever_open < LEVER_OPEN {
                    self.enter_stage(g, t, ReloadStage::WalkBack);
                } else if let Some(lever) = Self::gaze_point(g, "lever") {
                    let at = self.nudged(g, lever, &NUDGES_WIDE);
                    self.look(g, r, at, dt, &mut input);
                    // Only the lever prompt: the loaded-round prompt on the same sight line would
                    // pull the live round back out.
                    if hover_lever && !self.primary_held && a > 0.3 {
                        input.mg_coax = true;
                        self.pilot_log(g, t, "CLICK lever (close)");
                    }
                    self.consider_nudge(t, hover_lever, NUDGES_WIDE.len());
                    self.stage_timed_out(g, t);
                }
            }
            ReloadStage::WalkBack => {
                let waypoints = [
                    Vec2::new(-0.6, -0.6),
                    Vec2::new(0.6, -0.6),
                    Vec2::new(0.6, 0.2),
                ];
                let gaze = if self.reload_waypoint == 0 { "cupola" } else { "gunner" };
                if let Some(at) = Self::gaze_point(g, gaze) {
                    self.look(g, r, at, dt, &mut input);
                }
                if self.reload_waypoint >= waypoints.len() {
                    self.enter_stage(g, t, ReloadStage::Seat);
                } else if self.walk_towards(g, waypoints[self.reload_waypoint], &mut input) {
                    self.reload_waypoint += 1;
                } else {
                    self.stage_timed_out(g, t);
                }
            }
            ReloadStage::Seat => {
                if g.station == Station::Gunner {
                    if g.breech_loaded() && g.lever_open < LEVER_OPEN {
                        self.reloads += 1;
                        self.reload_at = None;
                        self.stage_tries = 0;
                        self.mouse_velocity = Vec2::ZERO;
                        self.pilot_log(g, t, "RELOAD COMPLETE");
                    } else {
                        // Seated on an unready gun: go round again.
                        self.reload_waypoint = 0;
                        self.enter_stage(g, t, ReloadStage::Walk);
                    }
                } else if let Some(seat) = Self::gaze_point(g, "gunner") {
                    let at = self.nudged(g, seat, &NUDGES_WIDE);
                    self.look(g, r, at, dt, &mut input);
                    let on_seat = g.hover_station == Some(Station::Gunner)
                        && !g.primary_loader_owns_click();
                    if on_seat && !self.primary_held && a > 0.3 {
                        input.mg_coax = true;
                        self.pilot_log(g, t, "CLICK seat");
                    }
                    self.consider_nudge(t, on_seat, NUDGES_WIDE.len());
                    self.stage_timed_out(g, t);
                }
            }
        }
        input
    }

    pub fn step(&mut self, g: &mut Game, r: &Renderer, t: f32, dt: f32) -> TankInput {
        if g.game_over().is_some() || g.canyon_finished().is_some() {
            self.phase = "ended";
            return TankInput::default();
        }
        let seen = g.canyon_pilot_visible_target();
        if seen.is_some() {
            self.last_target = seen;
            self.last_target_at = t;
        }
        let target = if t - self.last_target_at < ENGAGE_HOLD_S { self.last_target } else { None };
        let mut input = if self.reload_at.is_some() {
            self.reload(g, r, t, dt)
        } else if !g.breech_loaded() && self.reload_abandoned_at.is_none_or(|at| t - at > RELOAD_REST_S) {
            self.reload_abandoned_at = None;
            self.skip_slots.clear();
            // Same Q action as the native gunner: leave the sight for the free-walk interior.
            g.sights_mag = 0;
            g.commander_outside = false;
            g.jump_station(Station::Loader);
            self.reload_waypoint = if g.walk_pos.x < 0.0 { 2 } else { 0 };
            self.reload_at = Some(t);
            self.stage_tries = 0;
            self.enter_stage(g, t, ReloadStage::Walk);
            self.reload(g, r, t, dt)
        } else if let Some((at, gun)) = target.filter(|_| g.lever_open < LEVER_OPEN) {
            // Only a gun that can fire is worth stopping for; with the lever still open after an
            // abandoned reload the pilot drives on rather than standing in the open.
            self.phase = "engage";
            g.commander_outside = false;
            g.request_station(Station::Gunner);
            let mut input = TankInput::default();
            if g.station_ready(Station::Gunner) {
                self.look(g, r, at, dt, &mut input);
                // Fire only at a contact that is in sight now, not at where one was.
                let ready = seen.is_some() && g.capture_aim_ready(at);
                input.mg_coax = ready && g.auto_can_fire();
                input.coax = gun.is_some() && ready;
            }
            // The hull turns seven times faster than the turret traverses. A contact far off the
            // bore is met by pivoting the hull toward it (neutral steer, no advance) while the
            // turret chases; the fine lay is the gunner's as before. Driving from the sights is
            // the view model's own rule.
            let here = Vec2::new(g.player.pos.x, g.player.pos.z);
            let bearing = navigation_input(Vec2::new(at.x, at.z) - here, g.player.heading);
            let off_bore = (Vec2::new(at.x, at.z) - here).x.atan2((Vec2::new(at.x, at.z) - here).y)
                - (g.player.heading + g.player.turret_yaw);
            let off_bore = (off_bore + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            if off_bore.abs() > ENGAGE_PIVOT_RAD {
                input.drive = 0.0;
                input.steer = bearing.steer;
                if !g.engine_on {
                    input.ignition = true;
                }
            }
            input
        } else {
            self.phase = "advance";
            if g.station_ready(Station::Gunner) {
                g.request_station(Station::Loader);
            } else if g.station_ready(Station::Loader) {
                g.request_station(Station::Commander);
            }
            // Buttoned up: posted riflemen along the route kill a head-out commander within
            // seconds (seed 7, 2026-10-27), and this driver's contact model is geometric, so the
            // open cupola bought it nothing.
            if g.hatch_is_open() {
                g.toggle_hatch();
            }
            g.commander_outside = false;
            let here = Vec2::new(g.player.pos.x, g.player.pos.z);
            let close = (self.cursor.saturating_sub(3)..(self.cursor + 12).min(self.route.len()))
                .min_by(|&a, &b| {
                    self.route[a]
                        .distance_squared(here)
                        .total_cmp(&self.route[b].distance_squared(here))
                })
                .unwrap();
            self.cursor = self.cursor.max(close);
            // Route watchdog. Combat and reloads pause the advance; only advancing time counts.
            if !self.was_advancing {
                self.best_cursor_at = t;
                self.last_check = t;
                self.check_pos = here;
            }
            if self.cursor > self.best_cursor {
                self.best_cursor = self.cursor;
                self.best_cursor_at = t;
                if t >= self.avoid_until {
                    self.stuck_count = 0;
                }
            } else if t - self.best_cursor_at > 30.0 && t >= self.avoid_until {
                self.trigger_avoid(t);
            }
            let mut carrot = self.route[(self.cursor + 1).min(self.route.len() - 1)];
            let mut run_up = false;
            if t >= self.reverse_until && t < self.avoid_until {
                if self.stuck_count <= 1 {
                    // First recovery: back off, then run straight at the obstruction at full
                    // throttle. Trunks and brick piles yield to momentum, not to a sideways creep.
                    run_up = true;
                    self.phase = "run_up";
                } else {
                    // Approach past the obstruction: offset the carrot perpendicular to the local
                    // route direction, farther out on each consecutive failure.
                    let a = self.route[self.cursor.min(self.route.len() - 1)];
                    let b = self.route[(self.cursor + 2).min(self.route.len() - 1)];
                    let along = (b - a).normalize_or_zero();
                    let perp = Vec2::new(-along.y, along.x);
                    // Offsets stay inside the authored lane (12 m half-width): the village
                    // streets have frontage houses 13-19 m off the centreline, and the old
                    // 16-48 m ladder drove straight into them (seed 29, 2026-10-27).
                    carrot += perp * self.avoid_side * (5.0 + 3.0 * (self.stuck_count - 1).min(4) as f32);
                    self.phase = "detouring";
                }
            }
            // A dead hull on the line is passed on clear floor; a run-up never moves one. Boxed
            // in among several (the bypass and the ladder both failed twice), drive out along
            // the clearest corridor first.
            if t >= self.reverse_until {
                if let Some(out) = (self.stuck_count >= 2).then(|| self.hull_escape(g, here)).flatten() {
                    carrot = out;
                    run_up = false;
                    self.phase = "escaping";
                } else if let Some(past) = self.hull_bypass(g, here) {
                    carrot = past;
                    run_up = false;
                    self.phase = "bypassing";
                }
            }
            let mut input = navigation_input(carrot - here, g.player.heading);
            if run_up && input.drive > 0.0 {
                input.drive = 1.0;
            }
            if t - self.last_check > 4.0 {
                if input.drive > 0.1 && here.distance(self.check_pos) < 2.0 && t >= self.avoid_until {
                    self.trigger_avoid(t);
                }
                self.check_pos = here;
                self.last_check = t;
            }
            if t < self.reverse_until {
                self.phase = "backing_off";
                input.drive = -0.55;
                // Arc the reverse so the hull rotates away from the blocked line instead of
                // backing straight down it and driving straight back in.
                input.steer = -self.avoid_side * 0.45;
            }
            let look_at = self.route[(self.cursor + 12).min(self.route.len() - 1)];
            self.look(
                g,
                r,
                Vec3::new(
                    look_at.x,
                    g.hf.height_at(look_at.x, look_at.y) + 2.7,
                    look_at.y,
                ),
                dt,
                &mut input,
            );
            input
        };
        // Vehicle avoidance shapes drive commands only. At the loader's post the same two axes
        // walk the body, and an avoidance "turn" there pins the man against the basket wall.
        if g.mode == Mode::Tank && g.station != Station::Loader {
            (input.drive, input.steer) = g.safe_ai_tiger_drive(input.drive, input.steer);
        }
        self.was_advancing = matches!(self.phase, "advance" | "backing_off" | "detouring" | "run_up" | "bypassing" | "escaping");
        // Mirror native primary-click dispatch. Held ammunition is owned by Game::update.
        if input.mg_coax && !self.primary_held {
            let eye = r.camera.pos;
            let forward = r.camera.forward();
            if g.click_articulated_hatch() {
            } else if g.primary_loader_owns_click() {
            } else if g.interact_world_door(forward, eye) || g.interact_portable_item(forward, eye)
            {
                g.consume_primary_press();
            } else if g.interact_hatch_lever(forward, eye) || g.click_station() {
            } else if g.hover_hatch() {
                g.click_hatch_or_exit();
            }
        }
        self.primary_held = input.mg_coax;
        if !g.engine_on && self.was_advancing {
            input.ignition = true;
        }
        input
    }
}

/// Pivot before accelerating into a new heading. Forward creep during a large turn cuts across
/// the obstruction the recovery is trying to clear. Tank::update supplies normal braking inertia.
fn navigation_input(to: Vec2, heading: f32) -> TankInput {
    let error = (to.x.atan2(to.y) - heading + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    TankInput {
        drive: if error.abs() > 0.6 { 0.0 } else { 0.60 },
        steer: (error * 2.2).clamp(-1.0, 1.0),
        ..Default::default()
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn turn_to_detour_brakes_before_accelerating() {
        let turning = navigation_input(Vec2::X * 24.0, 0.0);
        assert_eq!(turning.drive, 0.0);
        assert_eq!(turning.steer, 1.0);
        let aligned = navigation_input(Vec2::X * 24.0, std::f32::consts::FRAC_PI_2);
        assert_eq!(aligned.drive, 0.6);
        assert!(aligned.steer.abs() < 1e-5);
    }

    #[test]
    fn steering_takes_short_arc_across_heading_wrap() {
        let angle = -std::f32::consts::PI + 0.1;
        let input = navigation_input(Vec2::new(angle.sin(), angle.cos()), std::f32::consts::PI - 0.1);
        assert!(input.steer > 0.0 && input.steer < 0.5);
        assert!(input.drive > 0.0);
    }

    #[test]
    fn repeated_stalls_alternate_and_bound_recovery() {
        let mut pilot = CanyonPilot::from_route(vec![Vec2::ZERO, Vec2::Y * 100.0], Vec2::ZERO);
        let mut last_side = pilot.avoid_side;
        let mut last_duration = 0.0;
        for i in 0..20 {
            let t = i as f32 * 100.0;
            pilot.trigger_avoid(t);
            assert_eq!(pilot.avoid_side, -last_side);
            let duration = pilot.avoid_until - t;
            assert!(duration >= last_duration - 0.001 && duration <= 23.301);
            assert!(pilot.reverse_until > t && pilot.reverse_until < pilot.avoid_until);
            last_side = pilot.avoid_side;
            last_duration = duration;
        }
    }
}

impl Game {
    fn canyon_pilot_visible_target(&self) -> Option<(Vec3, Option<usize>)> {
        let eye = self.player.center() + Vec3::Y * 1.1;
        self.enemies
            .iter()
            .enumerate()
            .filter(|(i, e)| self.enemy_active[*i] && e.alive)
            .filter_map(|(i, e)| {
                self.capture_gaze_target(&serde_json::json!({"gaze":"enemy","enemy":i}))
                    .map(|p| (p, None, e.center()))
            })
            .chain(self.at_guns.iter().enumerate().filter_map(|(i, g)| {
                g.filter(|g| g.working())
                    .map(|g| (g.pos + Vec3::Y * 0.6, Some(i), g.pos + Vec3::Y * 0.6))
            }))
            .filter(|(at, _, centre)| {
                eye.distance(*centre) < 420.0
                    && self.optical_los_clear(eye, *centre)
                    && self
                        .street_line_of_fire_clear(eye + (*at - eye).normalize_or_zero() * 5.0, *at)
                    && self.rubble_segment_contact(eye, *at).is_none()
            })
            .min_by(|a, b| {
                a.0.distance_squared(eye)
                    .total_cmp(&b.0.distance_squared(eye))
            })
            .map(|(p, g, _)| (p, g))
    }

    pub fn canyon_ai_state(&self, pilot: &CanyonPilot, t: f32) -> serde_json::Value {
        let run = self.canyon_run.as_ref().unwrap();
        serde_json::json!({"time_s":t,"phase":pilot.phase,"position":self.player.pos.to_array(),
            "heading":self.player.heading,"speed_m_s":self.player.speed,"station":format!("{:?}",self.station),
            "progress":crate::canyon_director::route_progress(&run.spec.route,Vec2::new(self.player.pos.x,self.player.pos.z)),
            "alive":self.player.alive,"death_cause":self.game_over().map(|c|format!("{c:?}")),
            "finished_s":run.finished_s,"shots":self.gun_shots,"kills":self.stat_kills_by_player,
            "hits":self.stat_hits_by_player,"incoming_hits":self.stat_hits_on_player,"incoming_penetrations":self.stat_pens_on_player,
            "ammo":remaining_main_gun_rounds(&self.slot_taken,self.breech),"reloads":pilot.reloads,
            "breech":format!("{:?}",self.breech),"lever":self.lever_open,"carrying":self.grab,
            "carried_casing":self.carried_casing.is_some(),"casing_pull":self.casing_pull,"ram":self.ram,
            "hover":self.hover_label,"stage":pilot.stage.label(),"stage_tries":pilot.stage_tries,
            "walk":self.walk_pos.to_array(),"broken_modules":self.modules.iter().filter(|m|!m.functional).map(|m|m.kind.name()).collect::<Vec<_>>(),
            "live_t34s":self.campaign_ai_enemies_alive(),"total_t34s":run.director.as_ref().unwrap().total_spawned,
            "combat":self.capture_combat_receipt(),"live_at_guns":self.at_guns.iter().flatten().filter(|g|g.working()).count(),
            "navigation":{"cursor":pilot.cursor,"recoveries":pilot.stuck_count,"side":pilot.avoid_side,"reverse_until":pilot.reverse_until,"avoid_until":pilot.avoid_until},
            "ground":format!("{:?}",self.tank_ground_state()),
            "god":self.god,"return_fire":self.playtest_return_fire(),"target_in_sight":self.canyon_pilot_visible_target().map(|(p,g)|serde_json::json!({"position":p.to_array(),"gun":g}))})
    }
}
