//! Game adapter for the infantry swarm (the owner, 9 November 2026). The policy is renderer-free
//! elsewhere: `infantry_swarm` (reading the Tiger, danger arcs, bounds, the decision ladder),
//! `thrown_weapons` (throws, flight, fuse), `infantry_boarding` (the deck, grip, the barrel's
//! sweep, hatches), `exterior_fire` (a bottle fire's heat path and petrol) and `swarm_sound`
//! (named events). This module does the world queries and applies the outcomes through the
//! existing rules: the shared infantry combat for everyone not on the hull, the existing blast
//! law for infantry, the crew's existing casualty route (`injure_field_actor`), the surface fire
//! and its foam, and the one-zone cabin heat model.
//!
//! Off (`KESSEL_INFANTRY_SWARM=0`, or any map but the two playtest courses) nothing here runs and
//! no soldier, slot or rule differs from the build before it.
use super::*;
use crate::infantry_combat as combat;
use crate::exterior_fire as xfire;
use crate::infantry_boarding as board;
use crate::infantry_swarm as swarm;
use crate::swarm_sound::{SoundHooks, SwarmSound};
use crate::thrown_weapons as thrown;

pub(super) const N_THROWN_SLOTS: usize = 24;
/// A fragment's speed falls off with the distance flown (m; estimate for gram-sized fragments).
const FRAGMENT_DECAY_M: f32 = 12.0;
/// Each soviet rifleman's thrown weapons (estimates for 1942-43 rifle companies: one or two
/// grenades a man, petrol bottles with some).
const GRENADES_PER_MAN: u8 = 2;
/// One man in this many also carries a petrol bottle.
const BOTTLE_EVERY: usize = 3;
/// A thrower stands, faces the Tiger and winds up for this long before the release (s).
const WINDUP_S: f32 = 1.1;
/// Seconds between one boarder's hammer blows on a shut hatch.
const HAMMER_BLOW_S: f32 = 0.7;
/// A man who could not reach a climb place in this long gives it up (s).
const RUN_TIMEOUT_S: f32 = 25.0;
/// How long a man covering the driver's visor stays there (s).
const VISOR_S: f32 = 12.0;
/// A man the swarm grounded stands again once the arc has been away this long (s).
const STAND_AFTER_S: f32 = 1.2;

/// The thrown-weapon bodies' meshes: even slots grenades (a dark 6 x 6 x 19 cm stick), odd slots
/// bottles (green glass, 7 x 7 x 25 cm). Plain unlit-free bodies, no emissive.
pub(super) fn push_thrown_meshes(scene: &mut Vec<CpuMesh>) -> Vec<usize> {
    (0..N_THROWN_SLOTS).map(|i| {
        let slot = scene.len();
        if i % 2 == 0 {
            scene.push(box_mesh(Vec3::new(-0.03, -0.03, -0.095), Vec3::new(0.03, 0.03, 0.095), [52, 54, 48]));
        } else {
            scene.push(box_mesh(Vec3::new(-0.035, -0.035, -0.125), Vec3::new(0.035, 0.035, 0.125), [70, 96, 72]));
        }
        slot
    }).collect()
}

#[derive(Clone, Copy, Debug)]
struct Windup { left_s: f32, kind: thrown::Kind, target: Vec3 }

#[derive(Clone, Copy, Debug, PartialEq)]
enum BoardState {
    /// Running to a climb place beside the hull.
    Run { climb: usize, t: f32 },
    /// Climbing from the ground at that place onto the deck.
    Climb { climb: usize, t: f32 },
    /// Walking over the deck to `Boarder::target`.
    Walk,
    /// At a place, doing what it is for.
    Work { place: usize },
    /// Down in an open aperture, fighting from it.
    InHatch(board::Hatch),
    /// Thrown off: falling.
    Fall { vel: Vec3 },
}

#[derive(Clone, Copy, Debug)]
struct Boarder {
    state: BoardState,
    frame: board::Frame,
    /// Feet in `frame`'s local coordinates.
    local: Vec3,
    grip_over: f32,
    timer: f32,
    fire_cd: f32,
    /// Hatches tried (bit per `Hatch::index`).
    tried: u8,
    target: usize,
}

#[derive(Clone, Debug, Default)]
struct Mind {
    sighting: Option<swarm::Sighting>,
    look_in_s: f32,
    plan_in_s: f32,
    intent: Option<swarm::Intent>,
    bound: Option<Vec3>,
    covered: bool,
    prone_since: Option<f32>,
    clear_since: f32,
    throw_cd: f32,
    grenades: u8,
    bottles: u8,
    windup: Option<Windup>,
    boarder: Option<Boarder>,
    armed: bool,
    /// The Tiger aim point he last sampled (hull frame, so it rides with the tank) and its age.
    aim_cache: Option<(Option<Vec3>, f32)>,
}

struct Flight { body: thrown::Thrown, inside: bool, sounded: bool, from: Vec3 }

#[derive(Clone, Copy)]
struct TankMotion { center: Vec3, vel: Vec3, accel: Vec3, heading: f32, yaw_rate: f32, yaw_accel: f32, barrel: (Vec3, Vec3) }

#[derive(Default, serde::Serialize, Clone)]
struct Stats {
    sightings: u64, grounded: u64, rushes: u64, throws_grenade: u64, throws_bottle: u64, bursts: u64,
    bursts_inside: u64, bottles_broken_on_tank: u64, bottles_broken_on_ground: u64, deck_patches: u64,
    engine_bay_fires: u64, fixed_extinguisher: u64, climbs: u64, thrown_off_grip: u64, thrown_off_barrel: u64,
    falls_killed: u64, falls_injured: u64, hatch_blows: u64, hatch_shots: u64, grenades_inside: u64,
    climbed_in: u64, visor_covered: u64, wingman_mg_rounds: u64, not_fielded: u64, crew_wounds: u64,
}

pub(super) struct SwarmRuntime {
    pub enabled: bool,
    clock_s: f32,
    tick: u64,
    minds: Vec<Mind>,
    lod_dt: Vec<f32>,
    flights: Vec<Flight>,
    next_id: u32,
    fires: Vec<xfire::BottleFire>,
    classes: std::collections::HashMap<u32, xfire::Place>,
    intakes: Vec<xfire::Intake>,
    bay_burning_s: f32,
    fixed_used: bool,
    pub sounds: SoundHooks,
    decoded: std::collections::HashMap<String, Option<std::sync::Arc<[f32]>>>,
    motion: Option<TankMotion>,
    thrown_slots: Vec<usize>,
    places: Vec<board::Place>,
    stats: Stats,
    wingman_mg: Vec<(f32, u32)>,
    last_shout_s: f32,
    /// Last update's states for the generated sounds' moments (`Game::sfx_monitor`).
    monitor: Option<(bool, bool, bool, f32, f32)>,
}

/// What the shared combat loop does with one soldier this step.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SwarmOrder {
    /// The swarm moves him itself (thrower, boarder).
    pub skip: bool,
    /// Stay where he is (prone or holding cover); he may still turn, aim and fire.
    pub hold: bool,
    /// Bound to this point at a run; no firing on the move.
    pub rush: Option<Vec3>,
}

impl SwarmRuntime {
    pub(super) fn new(enabled: bool, thrown_slots: Vec<usize>) -> Self {
        let set = crate::fea_damage::components::tiger_components();
        let intakes = set.envelopes.iter().filter(|e| {
            let id = crate::fea_damage::components::COMPONENT_SPECS[e.spec].id;
            id.starts_with("radiator_") || id.starts_with("cooling_fans_")
        }).map(|e| {
            let (min, max) = e.bounds();
            xfire::Intake { min: Vec2::new(min.x, min.z), max: Vec2::new(max.x, max.z), top_y: max.y }
        }).collect();
        Self {
            enabled, clock_s: 0.0, tick: 0, minds: Vec::new(), lod_dt: Vec::new(), flights: Vec::new(),
            next_id: 1, fires: Vec::new(), classes: Default::default(), intakes, bay_burning_s: 0.0,
            fixed_used: false, sounds: SoundHooks::default(), decoded: Default::default(), motion: None,
            thrown_slots, places: board::places(), stats: Stats::default(), wingman_mg: Vec::new(), last_shout_s: -100.0, monitor: None,
        }
    }
    fn mind(&mut self, i: usize) -> &mut Mind {
        if self.minds.len() <= i {
            let start = self.minds.len();
            self.minds.resize(i + 1, Mind::default());
            for (k, m) in self.minds.iter_mut().enumerate().skip(start) {
                m.grenades = GRENADES_PER_MAN;
                m.bottles = u8::from(k % BOTTLE_EVERY == 1);
                // Stagger the looks over the interval.
                m.look_in_s = (k % 16) as f32 / 16.0 * swarm::LOOK_INTERVAL_S;
                m.plan_in_s = (k % 8) as f32 * 0.1;
                m.armed = true;
            }
        }
        &mut self.minds[i]
    }
    pub(super) fn record_not_fielded(&mut self, n: usize) { self.stats.not_fielded += n as u64; }
}

fn flat(v: Vec3) -> Vec3 { Vec3::new(v.x, 0.0, v.z) }
fn yaw_toward(from: Vec3, to: Vec3) -> f32 { let d = to - from; d.x.atan2(-d.z) }

impl Game {
    fn swarm_frame_model(&self, frame: board::Frame) -> Mat4 {
        match frame { board::Frame::Hull => self.player.hull_model(), board::Frame::Turret => self.player.turret_model() }
    }

    fn swarm_hatch_open(&self, h: board::Hatch) -> bool {
        match h {
            board::Hatch::Cupola => self.hatch_t > 0.5,
            _ => self.roof_hatches.phase[h.index()] > 0.5,
        }
    }

    fn swarm_aperture_world(&self, h: board::Hatch) -> Vec3 {
        self.swarm_frame_model(h.frame()).transform_point3(h.aperture())
    }

    /// The actual source surface under a deck point (world), swept down from above it.
    fn swarm_feet(&self, world: Vec3, models: &[Mat4; surface_fire::FRAMES]) -> Vec3 {
        let up = self.player.hull_model().transform_vector3(Vec3::Y).normalize_or_zero();
        let from = world + up * 2.2;
        let to = world - up * 1.6;
        match self.cabin_fire.sweep(from, to, models) {
            Some(hit) => from.lerp(to, hit.fraction),
            None => world,
        }
    }

    fn swarm_place_world(&self, place: &board::Place, models: &[Mat4; surface_fire::FRAMES]) -> Vec3 {
        let probe = self.swarm_frame_model(place.frame).transform_point3(Vec3::new(place.xz.x, 2.4, place.xz.y));
        self.swarm_feet(probe, models)
    }

    fn swarm_emit(&mut self, sound: SwarmSound, at: Vec3) {
        let t = self.swarm.clock_s;
        let seed = self.swarm.tick as u32 ^ (sound as u32).wrapping_mul(0x9E37_79B9);
        let _ = self.swarm.sounds.emit(sound, at, t, seed);
        self.sfx_row(sound.id(), at, seed);
    }

    /// A generated take (decoded once, mono at the engine's rate), cached by file.
    pub(super) fn generated_mono(&mut self, path: &str) -> Option<std::sync::Arc<[f32]>> {
        self.swarm.decoded.entry(path.to_string()).or_insert_with(|| decode_take(path).map(std::sync::Arc::from)).clone()
    }

    /// Moments with no sound today, watched from the state (any mode, swarm or not): a track
    /// breaking (BR-01), the Tiger's fire going out (FR-08), the turret stopping after a traverse
    /// (IN-04). Generated takes, played by default when installed.
    pub(super) fn sfx_monitor(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 { return; }
        let tracks = (self.module_ok(ModuleKind::LeftTrack), self.module_ok(ModuleKind::RightTrack));
        let fire = self.fire_active;
        let yaw = self.player.turret_yaw;
        let seed = self.kill_tick;
        if let Some((left, right, was_fire, last_yaw, rate)) = self.swarm.monitor {
            if self.player.alive && ((left && !tracks.0) || (right && !tracks.1)) {
                let side = if left && !tracks.0 { 1.75 } else { -1.75 };
                let at = self.player.hull_model().transform_point3(Vec3::new(side, 0.5, 0.0));
                self.sfx_row("BR-01", at, seed);
            }
            if was_fire && !fire && self.player.alive { let at = self.player.center(); self.sfx_row("FR-08", at, seed); }
            let now_rate = ((yaw - last_yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs() / dt;
            // A powered traverse (over about 2 deg/s) coming to rest.
            if rate > 0.035 && now_rate < 0.002 && self.player.alive {
                let at = self.player.turret_model().transform_point3(Vec3::new(0.0, 1.2, 0.0));
                self.sfx_row("IN-04", at, seed);
            }
            self.swarm.monitor = Some((tracks.0, tracks.1, fire, yaw, now_rate));
        } else {
            self.swarm.monitor = Some((tracks.0, tracks.1, fire, yaw, 0.0));
        }
    }

    /// A switched row played in place of its existing voice (the caller checked the switch), at
    /// the row's level or, for a row without one, at the engine-death interior level (estimate).
    pub(super) fn sfx_row_forced(&mut self, id: &str, at: Vec3, seed: u32) -> bool {
        let files = crate::sfx_rows::takes(id, false);
        if files.is_empty() { return false; }
        let Some(mono) = self.generated_mono(&files[seed as usize % files.len()]) else { return false };
        let level = crate::sfx_rows::find(id).map_or(0.0, |r| r.lpk_1m_db);
        let base = crate::sound_propagation::Emission::from_level(&crate::sound_levels::BULLET_ON_STEEL_1M);
        let emission = base.shifted(if level > 0.0 { level } else { 110.0 } - base.lpk_db);
        let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect();
        self.schedule_emission(crate::audio::Sfx::ImpactGround, at, emission, None, Some(stereo));
        true
    }

    /// Play a generated row (`sfx_rows`) from `at` through the ordinary propagation, at the row's
    /// 1 m level (estimate), if the rule wants it and a take is installed. True if one played.
    pub(super) fn sfx_row(&mut self, id: &str, at: Vec3, seed: u32) -> bool {
        let Some(row) = crate::sfx_rows::find(id) else { return false };
        if !crate::sfx_rows::wanted(row) || row.lpk_1m_db <= 0.0 { return false; }
        let files = crate::sfx_rows::takes(id, false);
        if files.is_empty() { return false; }
        let Some(mono) = self.generated_mono(&files[seed as usize % files.len()]) else { return false };
        let base = crate::sound_propagation::Emission::from_level(&crate::sound_levels::BULLET_ON_STEEL_1M);
        let emission = base.shifted(row.lpk_1m_db - base.lpk_db);
        let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect();
        self.schedule_emission(crate::audio::Sfx::ImpactGround, at, emission, None, Some(stereo));
        true
    }

    /// Per-step cadence: `Some(dt to step with)` when this soldier is due.
    pub(super) fn swarm_step_dt(&mut self, i: usize, dt: f32) -> Option<f32> {
        if !self.swarm.enabled || i >= self.soviet_soldier_count { return Some(dt); }
        let engaged = self.mixed_combat.actors.get(i).is_some_and(|a| a.last_seen.is_some() || a.memory_s > 0.0)
            || self.soldiers[i].is_fleeing()
            || self.swarm.minds.get(i).is_some_and(|m| m.sighting.is_some() || m.intent.is_some());
        let to_tank = self.soldiers[i].pos.distance(self.player.pos);
        let to_foot = if self.mode == Mode::Foot { self.soldiers[i].pos.distance(self.foot.pos) } else { f32::INFINITY };
        let period = swarm::update_period(to_tank, to_foot, engaged);
        if self.swarm.lod_dt.len() <= i { self.swarm.lod_dt.resize(i + 1, 0.0); }
        self.swarm.lod_dt[i] += dt;
        if swarm::due(i, self.swarm.tick, period) {
            Some(std::mem::take(&mut self.swarm.lod_dt[i]).min(0.25))
        } else { None }
    }

    pub(super) fn swarm_order(&self, i: usize) -> SwarmOrder {
        if !self.swarm.enabled || i >= self.soviet_soldier_count { return SwarmOrder::default(); }
        let Some(mind) = self.swarm.minds.get(i) else { return SwarmOrder::default(); };
        if mind.boarder.is_some() || mind.windup.is_some() { return SwarmOrder { skip: true, ..Default::default() }; }
        match mind.intent {
            Some(swarm::Intent::Ground) | Some(swarm::Intent::Hold) | Some(swarm::Intent::Throw) | Some(swarm::Intent::Board) =>
                SwarmOrder { hold: true, ..Default::default() },
            Some(swarm::Intent::Rush(p)) => SwarmOrder { rush: Some(p), ..Default::default() },
            _ => SwarmOrder::default(),
        }
    }

    /// The soldier's perception point on the Tiger (`small_arm_player_aim_point`) with a clear
    /// sight line through the world to it, re-sampled every `AIM_RESAMPLE_S` (staggered) and kept
    /// in the hull's frame between samples so it moves with the tank: both visibility tests run
    /// at sensor cadence, not every frame (named).
    pub(super) fn swarm_cached_aim_point(&mut self, i: usize, eye: Vec3, dt: f32) -> Option<Vec3> {
        const AIM_RESAMPLE_S: f32 = 0.2;
        let hull = self.player.hull_model();
        let _ = self.swarm.mind(i);
        let stale = match self.swarm.minds[i].aim_cache {
            Some((_, age)) => age + dt >= AIM_RESAMPLE_S,
            None => true,
        };
        if stale {
            let point = self.small_arm_player_aim_point(eye).filter(|&p| self.optical_los_clear(eye, p));
            let local = point.map(|p| hull.inverse().transform_point3(p));
            // Stagger the next sample by the man's index so the samples spread over the frames.
            let phase = (i % 12) as f32 / 12.0 * AIM_RESAMPLE_S * 0.5;
            self.swarm.minds[i].aim_cache = Some((local, phase));
            return point;
        }
        let (local, age) = self.swarm.minds[i].aim_cache.expect("cached above");
        self.swarm.minds[i].aim_cache = Some((local, age + dt));
        local.map(|l| hull.transform_point3(l))
    }

    /// Do not rout from a comrade's fall while assaulting the Tiger (swarm only).
    pub(super) fn swarm_holds_nerve(&self, i: usize) -> bool {
        self.swarm.enabled && self.swarm.minds.get(i).is_some_and(|m| m.intent.is_some_and(|it| it != swarm::Intent::Doctrine)
            || m.boarder.is_some() || m.windup.is_some())
    }

    /// What the Tiger shows a man who can see its turret.
    fn swarm_tank_look(&self) -> swarm::TankLook {
        let (muzzle, gun) = self.player.muzzle();
        let gun_forward = flat(gun).normalize_or(Vec3::Z);
        swarm::TankLook { hull_pos: self.player.pos, hull_forward: flat(self.player.forward()).normalize_or(Vec3::Z),
            muzzle, gun_forward, bow: self.player.hull_mg_muzzle().0, speed_m_s: self.player.speed.abs() }
    }

    /// Before the shared combat loop: looks, plans and intents for every Soviet man not already
    /// owned by the swarm.
    pub(super) fn swarm_think(&mut self, dt: f32) {
        let _probe = crate::sim_phase::Probe::new("swarm_think");
        if !self.swarm.enabled || !dt.is_finite() || dt <= 0.0 { return; }
        self.swarm.clock_s += dt;
        self.swarm.tick = self.swarm.tick.wrapping_add(1);
        let now = self.swarm.clock_s;
        let n = self.soviet_soldier_count.min(self.soldiers.len()).min(self.mixed_combat.actors.len());
        if n > 0 { let _ = self.swarm.mind(n - 1); }
        let look = self.swarm_tank_look();
        let turret_top = self.player.turret_model().transform_point3(Vec3::new(0.0, 2.3, 0.0));
        let tank_alive = self.player.alive;
        let boarders = self.swarm.minds.iter().filter(|m| m.boarder.is_some()).count();
        let crowd = swarm::crowd((0..n).filter(|&i| self.soldiers[i].alive && !self.soldiers[i].is_fleeing())
            .map(|i| self.soldiers[i].pos), self.player.pos, swarm::SWARM_RADIUS_M);
        let boarding_open = tank_alive && boarders < swarm::MAX_BOARDERS;
        let mut cover: Option<world_index::WorldCover> = None;
        for i in 0..n {
            if !self.soldiers[i].alive {
                let m = &mut self.swarm.minds[i];
                m.boarder = None; m.windup = None; m.intent = None;
                continue;
            }
            {
                let m = &mut self.swarm.minds[i];
                m.throw_cd = (m.throw_cd - dt).max(0.0);
                if m.boarder.is_some() || m.windup.is_some() { continue; }
            }
            if self.soldiers[i].is_fleeing() {
                self.soldiers[i].gone_to_ground = false;
                self.swarm.minds[i].intent = None;
                continue;
            }
            let eye = self.soldiers[i].anatomical_eye();
            let pos = self.soldiers[i].pos;
            // ── Look ──
            let look_due = { let m = &mut self.swarm.minds[i]; m.look_in_s -= dt; m.look_in_s <= 0.0 };
            if look_due {
                self.swarm.minds[i].look_in_s += swarm::LOOK_INTERVAL_S;
                if tank_alive && eye.distance(turret_top) <= swarm::READ_RANGE_M && self.optical_los_clear(eye, turret_top) {
                    let previous = self.swarm.minds[i].sighting;
                    self.swarm.minds[i].sighting = Some(swarm::Sighting::observe(previous, look, now));
                    self.swarm.stats.sightings += 1;
                }
            }
            let sighting = self.swarm.minds[i].sighting.filter(|s| s.age(now) <= swarm::SIGHTING_MEMORY_S && tank_alive);
            self.swarm.minds[i].sighting = sighting;
            let Some(s) = sighting else {
                if self.soldiers[i].gone_to_ground { self.soldiers[i].gone_to_ground = false; }
                self.swarm.minds[i].intent = None;
                continue;
            };
            let danger = swarm::danger(pos, &s, now);
            let to_tank = flat(pos - s.look.hull_pos).length();
            // ── Plan the next bound (cadenced; each candidate is one line-of-sight query) ──
            let plan_due = { let m = &mut self.swarm.minds[i]; m.plan_in_s -= dt;
                m.plan_in_s <= 0.0 || m.bound.is_some_and(|b| flat(b - pos).length() < 1.0) };
            if plan_due && to_tank <= swarm::ASSAULT_RANGE_M {
                self.swarm.minds[i].plan_in_s = 0.8;
                let has_throwable = { let m = &self.swarm.minds[i]; m.grenades > 0 || m.bottles > 0 };
                let standoff = if has_throwable { swarm::THROW_RANGE_M * 0.7 } else { 9.0 };
                let goal = swarm::blind_side_point(&s, now, pos, standoff);
                let chest = Vec3::Y * 1.2;
                let covered_here = !self.los_clear(s.look.muzzle, pos + chest);
                if cover.is_none() { cover = Some(self.mixed_cover(None)); }
                let cover = cover.as_ref().expect("built above");
                let ground: Vec<swarm::Ground> = swarm::bound_candidates(pos, goal).into_iter().filter_map(|p| {
                    let y = self.hf.height_at(p.x, p.z);
                    let at = Vec3::new(p.x, y, p.z);
                    let walkable = (y - pos.y).abs() < 2.5 && !combat::CoverSet::occupies(cover, at, 0.35, y, y + 1.75);
                    walkable.then(|| swarm::Ground { at, covered: !self.los_clear(s.look.muzzle, at + chest) })
                }).collect();
                let bound = swarm::choose_bound(pos, covered_here, goal, &s, now, &ground);
                let m = &mut self.swarm.minds[i];
                m.bound = bound;
                m.covered = covered_here;
            }
            // ── Decide ──
            let (prone, prone_for, has_throwable, throw_ready, bound, covered) = {
                let m = &self.swarm.minds[i];
                (self.soldiers[i].gone_to_ground, m.prone_since.map_or(0.0, |t| now - t), m.grenades > 0 || m.bottles > 0,
                    m.throw_cd <= 0.0, m.bound, m.covered)
            };
            let sit = swarm::Situation { pos, covered_here: covered, danger, to_tank_m: to_tank,
                tank_speed_m_s: s.look.speed_m_s, has_throwable, throw_ready, crowd,
                boarding_open: boarding_open && self.swarm.minds.iter().filter(|m| m.boarder.is_some()).count() < swarm::MAX_BOARDERS,
                prone, prone_for_s: if swarm::must_go_to_ground(&danger) { 0.0 } else { prone_for } };
            let mut intent = swarm::decide(&sit, bound);
            // Keep a grounded man down until the arc has been away STAND_AFTER_S.
            if prone && intent != swarm::Intent::Ground {
                let m = &mut self.swarm.minds[i];
                if swarm::must_go_to_ground(&danger) { m.clear_since = now; }
                if now - m.clear_since < STAND_AFTER_S { intent = swarm::Intent::Ground; }
            }
            match intent {
                swarm::Intent::Ground => {
                    if !self.soldiers[i].gone_to_ground {
                        self.soldiers[i].gone_to_ground = true;
                        let m = &mut self.swarm.minds[i];
                        m.prone_since = Some(now);
                        m.clear_since = now;
                        self.swarm.stats.grounded += 1;
                        if i % 3 == 0 { self.swarm_emit(SwarmSound::GoToGround, pos); }
                    }
                }
                _ => {
                    if self.soldiers[i].gone_to_ground {
                        self.soldiers[i].gone_to_ground = false;
                        self.swarm.minds[i].prone_since = None;
                    }
                }
            }
            match intent {
                swarm::Intent::Throw => self.swarm_begin_throw(i, now),
                swarm::Intent::Board => self.swarm_begin_board(i, now),
                swarm::Intent::Rush(_) if !matches!(self.swarm.minds[i].intent, Some(swarm::Intent::Rush(_))) => {
                    self.swarm.stats.rushes += 1;
                    if i % 4 == 0 { self.swarm_emit(SwarmSound::Rush, pos); }
                }
                _ => {}
            }
            self.swarm.minds[i].intent = Some(intent);
        }
    }

    /// Choose what to throw and where, and start the wind-up.
    fn swarm_begin_throw(&mut self, i: usize, _now: f32) {
        let pos = self.soldiers[i].pos;
        let eye = self.soldiers[i].anatomical_eye();
        let (grenades, bottles) = { let m = &self.swarm.minds[i]; (m.grenades, m.bottles) };
        let to_tank = flat(pos - self.player.pos).length();
        let kind = if bottles > 0 && to_tank <= 20.0 { thrown::Kind::PetrolBomb }
            else if grenades > 0 { if i % 2 == 0 { thrown::Kind::Rgd33 } else { thrown::Kind::F1 } }
            else if bottles > 0 { thrown::Kind::PetrolBomb } else { return; };
        let models = self.fire_surface_models();
        let target = if kind == thrown::Kind::PetrolBomb {
            self.swarm_intake_target()
        } else {
            // An open hatch he can see, else the cupola with the commander out, else the engine deck.
            board::Hatch::ALL.into_iter().filter(|&h| self.swarm_hatch_open(h)).map(|h| self.swarm_aperture_world(h) + Vec3::Y * 0.05)
                .find(|&p| self.cabin_fire.sweep(eye, p + Vec3::Y * 0.3, &models).is_none() && self.los_clear(eye, p))
                .unwrap_or_else(|| self.swarm_intake_target())
        };
        self.swarm.minds[i].windup = Some(Windup { left_s: WINDUP_S, kind, target });
        self.soldiers[i].gone_to_ground = false;
        if kind != thrown::Kind::PetrolBomb { self.swarm_emit(SwarmSound::GrenadePrimed, eye); }
    }

    /// The engine deck over a cooling intake (world).
    fn swarm_intake_target(&self) -> Vec3 {
        let hull = self.player.hull_model();
        let local = self.swarm.intakes.get((self.swarm.tick % 2) as usize).map(|i| {
            let c = (i.min + i.max) * 0.5;
            Vec3::new(c.x, i.top_y + 0.6, c.y)
        }).unwrap_or(Vec3::new(0.0, 2.0, -2.6));
        hull.transform_point3(local)
    }

    fn swarm_begin_board(&mut self, i: usize, now: f32) {
        let pos = self.soldiers[i].pos;
        let hull = self.player.hull_model();
        let climb = self.swarm.places.iter().enumerate().filter(|(_, p)| p.kind == board::PlaceKind::Climb)
            .min_by(|a, b| {
                let da = hull.transform_point3(Vec3::new(a.1.xz.x, 0.0, a.1.xz.y)).distance(pos);
                let db = hull.transform_point3(Vec3::new(b.1.xz.x, 0.0, b.1.xz.y)).distance(pos);
                da.total_cmp(&db)
            }).map(|(k, _)| k);
        let Some(climb) = climb else { return };
        self.soldiers[i].gone_to_ground = false;
        self.swarm.minds[i].boarder = Some(Boarder { state: BoardState::Run { climb, t: 0.0 }, frame: board::Frame::Hull,
            local: Vec3::ZERO, grip_over: 0.0, timer: 0.0, fire_cd: 0.0, tried: 0, target: climb });
        if now - self.swarm.last_shout_s > 8.0 {
            self.swarm.last_shout_s = now;
            self.swarm_emit(SwarmSound::MassedShout, pos);
        }
    }

    fn swarm_update_motion(&mut self, dt: f32) {
        let center = self.player.center();
        let heading = self.player.heading;
        let hull = self.player.hull_model();
        let inv = hull.inverse();
        let (muzzle, _) = self.player.muzzle();
        let trunnion = self.player.gun_model().transform_point3(Vec3::ZERO);
        let barrel = (inv.transform_point3(trunnion), inv.transform_point3(muzzle));
        let motion = match self.swarm.motion {
            Some(m) if dt > 1e-5 => {
                let vel = (center - m.center) / dt;
                let accel = (vel - m.vel) / dt;
                // The hull model is rotation_y(heading): heading is the rotation about +Y.
                let yaw_rate = ((heading - m.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI) / dt;
                let yaw_accel = (yaw_rate - m.yaw_rate) / dt;
                // A teleport (a debug placement, a respawn) is not an acceleration.
                if vel.length() > 40.0 { TankMotion { center, vel: Vec3::ZERO, accel: Vec3::ZERO, heading, yaw_rate: 0.0, yaw_accel: 0.0, barrel } }
                else { TankMotion { center, vel, accel, heading, yaw_rate, yaw_accel, barrel } }
            }
            _ => TankMotion { center, vel: Vec3::ZERO, accel: Vec3::ZERO, heading, yaw_rate: 0.0, yaw_accel: 0.0, barrel },
        };
        self.swarm.motion = Some(motion);
    }

    /// After the shared combat loop: throwers, boarders, thrown bodies, bottle fires, the
    /// wingmen's machine guns and the bodies' draw.
    pub(super) fn swarm_act(&mut self, dt: f32, r: &mut Renderer) {
        let _probe = crate::sim_phase::Probe::new("swarm_act");
        if !self.swarm.enabled { return; }
        if !dt.is_finite() || dt <= 0.0 { self.swarm_drive_thrown(r); return; }
        let last_barrel = self.swarm.motion.map(|m| m.barrel);
        self.swarm_update_motion(dt);
        let models = self.fire_surface_models();
        let n = self.swarm.minds.len().min(self.soldiers.len());
        for i in 0..n {
            if !self.soldiers[i].alive { continue; }
            if self.swarm.minds[i].windup.is_some() { self.swarm_step_thrower(i, dt); }
            if self.swarm.minds[i].boarder.is_some() { self.swarm_step_boarder(i, dt, &models, last_barrel); }
        }
        self.swarm_step_flights(dt, &models);
        self.swarm_step_fires(dt);
        self.swarm_drive_thrown(r);
    }

    fn swarm_step_thrower(&mut self, i: usize, dt: f32) {
        let Some(mut w) = self.swarm.minds[i].windup else { return };
        let pos = self.soldiers[i].pos;
        let yaw = yaw_toward(pos, w.target);
        w.left_s -= dt;
        let eye = self.soldiers[i].anatomical_eye();
        if w.left_s > 0.0 {
            self.swarm.minds[i].windup = Some(w);
            self.soldiers[i].apply_combat_pose(pos, yaw, Vec3::ZERO, true, 0, false);
            if let Some(actor) = self.mixed_combat.actors.get_mut(i) { actor.pos = pos; actor.yaw = yaw; actor.velocity = Vec3::ZERO; }
            return;
        }
        self.swarm.minds[i].windup = None;
        let release = eye + Vec3::new(yaw.sin(), 0.1, -yaw.cos()) * 0.35;
        let seed = self.swarm.next_id.wrapping_mul(2654435761) ^ i as u32;
        let aimed = w.target + thrown::aim_error(release.distance(w.target), seed);
        let Some(vel) = thrown::throw_velocity(release, aimed, thrown::MAX_THROW_SPEED) else {
            self.swarm.minds[i].throw_cd = 2.0;
            return;
        };
        let id = self.swarm.next_id;
        self.swarm.next_id = self.swarm.next_id.wrapping_add(1).max(1);
        let m = &mut self.swarm.minds[i];
        m.throw_cd = swarm::THROW_COOLDOWN_S;
        if w.kind == thrown::Kind::PetrolBomb { m.bottles = m.bottles.saturating_sub(1); self.swarm.stats.throws_bottle += 1; }
        else { m.grenades = m.grenades.saturating_sub(1); self.swarm.stats.throws_grenade += 1; }
        self.swarm.flights.push(Flight { body: thrown::Thrown::new(id, w.kind, release, vel, i), inside: false, sounded: false, from: release });
        if w.kind == thrown::Kind::PetrolBomb { self.swarm_emit(SwarmSound::BottleThrown, release); }
        self.soldiers[i].apply_combat_pose(pos, yaw, Vec3::ZERO, false, 0, false);
    }

    // ── Boarders ──────────────────────────────────────────────────────────────────────────

    fn swarm_choose_target(&self, i: usize, b: &Boarder) -> usize {
        let m = &self.swarm.minds[i];
        let here = self.swarm_frame_model(b.frame).transform_point3(b.local);
        let places = &self.swarm.places;
        let find = |pred: &dyn Fn(&board::Place) -> bool| places.iter().enumerate().filter(|(_, p)| pred(p))
            .min_by(|a, c| {
                let pa = self.swarm_frame_model(a.1.frame).transform_point3(Vec3::new(a.1.xz.x, b.local.y, a.1.xz.y));
                let pc = self.swarm_frame_model(c.1.frame).transform_point3(Vec3::new(c.1.xz.x, b.local.y, c.1.xz.y));
                pa.distance(here).total_cmp(&pc.distance(here))
            }).map(|(k, _)| k);
        let untried = |h: board::Hatch| b.tried & (1 << h.index()) == 0;
        let taken = |h: board::Hatch| self.swarm.minds.iter().any(|o| o.boarder.is_some_and(|ob| ob.state == BoardState::InHatch(h)));
        find(&|p| matches!(p.kind, board::PlaceKind::Hatch(h) if self.swarm_hatch_open(h) && !taken(h)))
            .or_else(|| (m.bottles > 0).then(|| find(&|p| p.kind == board::PlaceKind::EngineDeck)).flatten())
            .or_else(|| find(&|p| matches!(p.kind, board::PlaceKind::Hatch(h) if untried(h))))
            .or_else(|| find(&|p| p.kind == board::PlaceKind::DriverVisor
                && !self.swarm.minds.iter().any(|o| o.boarder.is_some_and(|ob| matches!(ob.state, BoardState::Work { place } if places[place].kind == board::PlaceKind::DriverVisor)))))
            .unwrap_or_else(|| find(&|p| matches!(p.kind, board::PlaceKind::Hatch(_))).unwrap_or(0))
    }

    fn swarm_release_boarder(&mut self, i: usize) {
        self.swarm.minds[i].boarder = None;
        self.swarm.minds[i].intent = None;
        self.swarm.minds[i].plan_in_s = 0.0;
    }

    fn swarm_fall(&mut self, i: usize, b: &mut Boarder, push: Vec3, barrel: bool) {
        let world = self.swarm_frame_model(b.frame).transform_point3(b.local);
        let tank_vel = self.swarm.motion.map_or(Vec3::ZERO, |m| m.vel);
        let outward = flat(world - self.player.pos).normalize_or_zero();
        b.state = BoardState::Fall { vel: tank_vel + outward * 1.2 + push + Vec3::Y * 1.0 };
        b.local = world; // Fall keeps world coordinates
        if barrel { self.swarm.stats.thrown_off_barrel += 1; self.swarm_emit(SwarmSound::TurretSweep, world); }
        else { self.swarm.stats.thrown_off_grip += 1; }
        self.swarm_emit(SwarmSound::FallsOff, world);
        let _ = i;
    }

    fn swarm_step_boarder(&mut self, i: usize, dt: f32, models: &[Mat4; surface_fire::FRAMES], last_barrel: Option<(Vec3, Vec3)>) {
        let Some(mut b) = self.swarm.minds[i].boarder else { return };
        let hull = self.player.hull_model();
        let tank_speed = self.player.speed.abs();
        let mut world = self.soldiers[i].pos;
        let mut yaw = self.soldiers[i].yaw;
        let mut aiming = false;
        let mut fired = false;
        let before = world;
        // A dead or abandoned Tiger: everyone gets down.
        if !self.player.alive && !matches!(b.state, BoardState::Fall { .. }) {
            self.swarm_release_boarder(i);
            return;
        }
        match b.state {
            BoardState::Run { climb, mut t } => {
                t += dt;
                let place = self.swarm.places[climb];
                let beside = hull.transform_point3(Vec3::new(place.xz.x * 1.12, 0.3, place.xz.y * 1.04));
                let goal = Vec3::new(beside.x, self.hf.height_at(beside.x, beside.z), beside.z);
                let to = flat(goal - world);
                if to.length() < board::CLIMB_REACH_M && tank_speed < swarm::BOARD_MAX_SPEED_M_S * 1.4 {
                    b.state = BoardState::Climb { climb, t: 0.0 };
                    self.swarm.stats.climbs += 1;
                    self.swarm_emit(SwarmSound::ClimbHull, world);
                } else if t > RUN_TIMEOUT_S || tank_speed > swarm::BOARD_MAX_SPEED_M_S * 2.5 {
                    self.swarm_release_boarder(i);
                    return;
                } else {
                    let step = to.normalize_or_zero() * swarm::RUSH_SPEED_M_S * dt;
                    let next = self.hf.ground_walker(world, world + step);
                    world = Vec3::new(next.x, self.hf.height_at(next.x, next.z), next.z);
                    yaw = yaw_toward(before, goal);
                    b.state = BoardState::Run { climb, t };
                }
            }
            BoardState::Climb { climb, mut t } => {
                t += dt / board::CLIMB_S;
                let place = self.swarm.places[climb];
                let beside = hull.transform_point3(Vec3::new(place.xz.x * 1.12, 0.3, place.xz.y * 1.04));
                let ground = Vec3::new(beside.x, self.hf.height_at(beside.x, beside.z), beside.z);
                let deck = self.swarm_place_world(&place, models);
                world = ground.lerp(deck, t.clamp(0.0, 1.0));
                yaw = yaw_toward(ground, deck);
                if t >= 1.0 {
                    b.frame = place.frame;
                    b.local = self.swarm_frame_model(b.frame).inverse().transform_point3(deck);
                    b.state = BoardState::Walk;
                    b.target = self.swarm_choose_target(i, &b);
                } else {
                    b.state = BoardState::Climb { climb, t };
                }
            }
            BoardState::Walk => {
                let place = self.swarm.places[b.target];
                if place.frame != b.frame {
                    let w = self.swarm_frame_model(b.frame).transform_point3(b.local);
                    b.frame = place.frame;
                    b.local = self.swarm_frame_model(b.frame).inverse().transform_point3(w);
                }
                let target = Vec3::new(place.xz.x, b.local.y, place.xz.y);
                let to = flat(target - b.local);
                let scale = self.player.parts.frame_scale.max(1e-3);
                let step = board::DECK_SPEED_M_S / scale * dt;
                let model = self.swarm_frame_model(b.frame);
                if to.length() <= step.max(0.05) {
                    b.local.x = target.x; b.local.z = target.z;
                    b.state = BoardState::Work { place: b.target };
                    b.timer = 0.0;
                } else {
                    b.local += to.normalize() * step;
                }
                let probe = model.transform_point3(Vec3::new(b.local.x, b.local.y + 0.4, b.local.z));
                let feet = self.swarm_feet(probe, models);
                b.local = model.inverse().transform_point3(feet);
                world = feet;
                yaw = yaw_toward(before, world);
            }
            BoardState::Work { place } => {
                let p = self.swarm.places[place];
                world = self.swarm_frame_model(b.frame).transform_point3(b.local);
                b.timer += dt;
                match p.kind {
                    board::PlaceKind::Hatch(h) => {
                        let aperture = self.swarm_aperture_world(h);
                        yaw = yaw_toward(world, aperture);
                        let open = self.swarm_hatch_open(h);
                        let taken = self.swarm.minds.iter().enumerate().any(|(k, o)| k != i && o.boarder.is_some_and(|ob| ob.state == BoardState::InHatch(h)));
                        let has_grenade = self.swarm.minds[i].grenades > 0;
                        let crew_point = if open { self.swarm_crew_in_sight(aperture + Vec3::Y * 0.65) } else { None };
                        match board::hatch_action(open, has_grenade, crew_point.is_some(), taken) {
                            board::HatchAction::Hammer => {
                                if b.timer - dt <= 0.0 { self.swarm_emit(SwarmSound::HatchHandle, aperture); }
                                b.fire_cd -= dt;
                                if b.fire_cd <= 0.0 {
                                    b.fire_cd = HAMMER_BLOW_S;
                                    self.swarm.stats.hatch_blows += 1;
                                    self.swarm_emit(SwarmSound::HammerHatch, aperture);
                                }
                                if b.timer >= board::HAMMER_S {
                                    b.tried |= 1 << h.index();
                                    if b.tried == 0b1_1111 { b.tried = 0; }
                                    b.target = self.swarm_choose_target(i, &b);
                                    b.state = BoardState::Walk;
                                }
                            }
                            board::HatchAction::GrenadeIn => {
                                let id = self.swarm.next_id;
                                self.swarm.next_id = self.swarm.next_id.wrapping_add(1).max(1);
                                let drop = aperture + Vec3::Y * 0.25;
                                self.swarm.flights.push(Flight { body: thrown::Thrown::new(id, thrown::Kind::Rgd33, drop,
                                    Vec3::new(0.0, -1.2, 0.0), i), inside: true, sounded: true, from: drop });
                                self.swarm.minds[i].grenades = self.swarm.minds[i].grenades.saturating_sub(1);
                                self.swarm.stats.grenades_inside += 1;
                                self.swarm_emit(SwarmSound::GrenadeDroppedInside, aperture);
                                b.tried |= 1 << h.index();
                                b.target = self.swarm_choose_target(i, &b);
                                b.state = BoardState::Walk;
                            }
                            board::HatchAction::FireIn => {
                                aiming = true;
                                if let Some(point) = crew_point {
                                    b.fire_cd -= dt;
                                    if b.fire_cd <= 0.0 && self.swarm_fire_into(i, aperture + Vec3::Y * 0.65, point) {
                                        b.fire_cd = combat::RIFLE_INTERVAL;
                                        fired = true;
                                    }
                                }
                                if b.timer > 12.0 { b.target = self.swarm_choose_target(i, &b); b.state = BoardState::Walk; }
                            }
                            board::HatchAction::ClimbIn => {
                                b.state = BoardState::InHatch(h);
                                self.swarm.stats.climbed_in += 1;
                                self.swarm_emit(SwarmSound::OnRoof, aperture);
                            }
                        }
                    }
                    board::PlaceKind::EngineDeck if self.swarm.minds[i].bottles > 0 => {
                        // He breaks his bottle on the deck at his feet.
                        self.swarm.minds[i].bottles -= 1;
                        self.swarm.stats.throws_bottle += 1;
                        let normal = hull.transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
                        let owner_pos = world + Vec3::Y * 1.2;
                        self.swarm_bottle_breaks(world + normal * 0.02, normal, thrown::Surface::Steel, owner_pos);
                        b.target = self.swarm_choose_target(i, &b);
                        b.state = BoardState::Walk;
                    }
                    board::PlaceKind::DriverVisor => {
                        if b.timer - dt <= 0.0 { self.swarm.stats.visor_covered += 1; self.swarm_emit(SwarmSound::VisionBlocked, world); }
                        yaw = yaw_toward(world, hull.transform_point3(Vec3::new(0.75, 1.6, 3.6)));
                        if b.timer > VISOR_S { b.target = self.swarm_choose_target(i, &b); b.state = BoardState::Walk; }
                    }
                    _ => {
                        if b.timer > 0.8 { b.target = self.swarm_choose_target(i, &b); b.state = BoardState::Walk; }
                    }
                }
            }
            BoardState::InHatch(h) => {
                let aperture = self.swarm_aperture_world(h);
                let up = self.swarm_frame_model(h.frame()).transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
                world = aperture - up * board::IN_HATCH_DEPTH_M;
                // The lid cannot shut on him (`infantry_boarding::can_close`).
                match h {
                    board::Hatch::Cupola => { self.hatch_open = true; }
                    _ => { self.roof_hatches.open[h.index()] = true; }
                }
                aiming = true;
                b.fire_cd -= dt;
                if let Some(point) = self.swarm_crew_in_sight(aperture + up * 0.45) {
                    yaw = yaw_toward(aperture, point);
                    if b.fire_cd <= 0.0 && self.swarm_fire_into(i, aperture + up * 0.45, point) {
                        b.fire_cd = combat::RIFLE_INTERVAL;
                        fired = true;
                    }
                }
            }
            BoardState::Fall { mut vel } => {
                let mut p = b.local;
                vel.y -= thrown::GRAVITY * dt;
                p += vel * dt;
                let ground = self.hf.height_at(p.x, p.z);
                if p.y <= ground {
                    p.y = ground;
                    let speed = vel.length();
                    let outcome = board::landing(speed);
                    self.soldiers[i].pos = p;
                    if let Some(actor) = self.mixed_combat.actors.get_mut(i) { actor.pos = p; actor.velocity = Vec3::ZERO; }
                    self.swarm_release_boarder(i);
                    match outcome {
                        board::Landing::Killed => { self.swarm.stats.falls_killed += 1; self.kill_soldier(i, "fall from the Tiger"); }
                        board::Landing::Injured => { self.swarm.stats.falls_injured += 1; self.soldiers[i].set_wounded_motion(0.65); self.soldiers[i].startle(2.5, p); }
                        board::Landing::Stumble => { self.soldiers[i].startle(1.2, p); }
                    }
                    return;
                }
                b.local = p;
                b.state = BoardState::Fall { vel };
                world = p;
            }
        }
        // ── Staying on: grip and the barrel (on the deck, climbing or at a place) ──
        let on_deck = matches!(b.state, BoardState::Walk | BoardState::Work { .. } | BoardState::Climb { .. });
        if on_deck {
            if let Some(m) = self.swarm.motion {
                let a = board::point_acceleration(m.accel, world - m.center, m.yaw_rate, m.yaw_accel);
                let (over, lost) = board::grip(b.grip_over, a, dt);
                b.grip_over = over;
                if lost {
                    b.local = world;
                    self.swarm_fall(i, &mut b, Vec3::ZERO, false);
                }
                if let (false, Some((a0, b0))) = (lost, last_barrel) {
                    if b.frame == board::Frame::Hull && !matches!(b.state, BoardState::Climb { .. }) {
                        let scale = self.player.parts.frame_scale.max(1e-3);
                        if board::barrel_sweeps(a0 * scale, b0 * scale, m.barrel.0 * scale, m.barrel.1 * scale, b.local * scale, 1.75) {
                            let tip_push = (hull.transform_point3(m.barrel.1) - hull.transform_point3(b0)) / dt.max(1e-3);
                            b.local = world;
                            self.swarm_fall(i, &mut b, flat(tip_push).clamp_length_max(4.0) + flat(tip_push).normalize_or_zero() * 1.5, true);
                        }
                    }
                }
            }
        }
        let vel = if dt > 1e-5 { (world - before) / dt } else { Vec3::ZERO };
        self.soldiers[i].pos = world;
        self.soldiers[i].gone_to_ground = false;
        self.soldiers[i].apply_combat_pose(world, yaw, if matches!(b.state, BoardState::Fall { .. }) { Vec3::ZERO } else { vel }, aiming, 0, fired);
        if let Some(actor) = self.mixed_combat.actors.get_mut(i) { actor.pos = world; actor.yaw = yaw; actor.velocity = vel; actor.aiming = aiming; }
        self.swarm.minds[i].boarder = Some(b);
    }

    /// The nearest crewman's head or chest a man can see from `from` through the hatches (no
    /// source armour between): the existing small-arms visibility rule.
    fn swarm_crew_in_sight(&self, from: Vec3) -> Option<Vec3> {
        let _probe = crate::sim_phase::Probe::new("swarm_crew_in_sight");
        let hull = self.player.hull_model();
        let mut best: Option<(f32, Vec3)> = None;
        for body in self.small_arm_crew_bodies() {
            for part in [crew_physics::CrewBodyPart::Head, crew_physics::CrewBodyPart::Torso] {
                let v = body.hit_volumes[part.index()];
                if v.half_extents.max_element() <= 0.0 { continue; }
                let p = hull.transform_point3(v.center);
                let d = p.distance(from);
                if d > 4.0 || best.is_some_and(|(bd, _)| bd <= d) { continue; }
                if self.small_arm_player_armor(from, p).is_none() { best = Some((d, p)); }
            }
        }
        best.map(|(_, p)| p)
    }

    /// A rifle shot fired down into the hull through the ordinary projectile path. False if he
    /// is out of rounds (then he reloads).
    fn swarm_fire_into(&mut self, i: usize, origin: Vec3, target: Vec3) -> bool {
        let Some(actor) = self.mixed_combat.actors.get_mut(i) else { return false };
        if actor.reload > 0.0 { actor.reload = (actor.reload - combat::RIFLE_INTERVAL).max(0.0); return false; }
        if actor.magazine == 0 {
            if actor.reserve == 0 { return false; }
            let n = 5u32.min(actor.reserve);
            actor.magazine += n; actor.reserve -= n; actor.reload = combat::RIFLE_RELOAD;
            return false;
        }
        actor.magazine -= 1;
        let j = |n: u32| ((n.wrapping_mul(2654435761) >> 8) & 0xffff) as f32 / 32768.0 - 1.0;
        let seed = self.swarm.tick as u32 ^ (i as u32).wrapping_mul(7919);
        let dir = ((target - origin).normalize_or_zero() + Vec3::new(j(seed), j(seed ^ 77), j(seed ^ 991)) * 0.012).normalize_or_zero();
        let shot = combat::Shot { shooter: i as u32, team: combat::Team::Soviet, origin, direction: dir, range: 8.0, weapon: combat::Weapon::Rifle };
        self.launch_small_arm(shot, combat::small_arms_damage(combat::Weapon::Rifle, false), true, None);
        let k = self.next_inf_fx;
        self.next_inf_fx = (k + 1) % INF_FX;
        self.inf_flash_t[k] = 0.06;
        self.inf_flash_pos[k] = origin + dir * 0.4;
        self.schedule_sound_from(crate::audio::Sfx::RifleShot, origin);
        self.swarm.stats.hatch_shots += 1;
        self.swarm_emit(SwarmSound::ShotsIntoHatch, origin);
        true
    }

    // ── Thrown bodies ─────────────────────────────────────────────────────────────────────

    /// The nearest surface along a thrown body's path: the Tiger's source surfaces (steel) or the
    /// world (terrain, structures, trees).
    fn swarm_sweep(&self, from: Vec3, to: Vec3, models: &[Mat4; surface_fire::FRAMES]) -> Option<thrown::Contact> {
        let delta = to - from;
        let len = delta.length();
        if len < 1e-6 { return None; }
        let dir = delta / len;
        let steel = self.cabin_fire.sweep(from, to, models).map(|hit| {
            let normal = models[hit.frame].inverse().transpose().transform_vector3(hit.normal).normalize_or(Vec3::Y);
            let normal = if normal.dot(dir) > 0.0 { -normal } else { normal };
            thrown::Contact { point: from.lerp(to, hit.fraction), normal, fraction: hit.fraction, surface: thrown::Surface::Steel }
        });
        let world = self.nearest_cover_ray(from, dir, len).map(|(t, hit)| {
            let point = from + dir * t;
            let normal = match hit {
                CoverRayHit::Terrain => {
                    let e = 0.5;
                    let h = |x: f32, z: f32| self.hf.height_at(x, z);
                    Vec3::new(h(point.x - e, point.z) - h(point.x + e, point.z), 2.0 * e, h(point.x, point.z - e) - h(point.x, point.z + e)).normalize_or(Vec3::Y)
                }
                // A wall or a trunk: the incoming direction reversed (its face normal is not kept).
                _ => -dir,
            };
            thrown::Contact { point, normal, fraction: t / len, surface: thrown::Surface::Ground }
        });
        match (steel, world) {
            (Some(a), Some(b)) => Some(if a.fraction <= b.fraction { a } else { b }),
            (a, b) => a.or(b),
        }
    }

    fn swarm_step_flights(&mut self, dt: f32, models: &[Mat4; surface_fire::FRAMES]) {
        let _probe = crate::sim_phase::Probe::new("swarm_step_flights");
        let mut flights = std::mem::take(&mut self.swarm.flights);
        let mut events: Vec<(thrown::Kind, u32, usize, bool, Vec3, thrown::Event)> = Vec::new();
        for f in &mut flights {
            let evs = f.body.step(dt, |a, b| self.swarm_sweep(a, b, models));
            for e in evs {
                if let thrown::Event::Struck { point, surface, .. } = e {
                    if !f.sounded { f.sounded = true;
                        events.push((f.body.kind, f.body.id, f.body.owner, f.inside, f.from, thrown::Event::Struck { point, surface, speed_m_s: 0.0 })); }
                    continue;
                }
                events.push((f.body.kind, f.body.id, f.body.owner, f.inside, f.from, e));
            }
        }
        flights.retain(|f| !f.body.done && f.body.pos.is_finite() && f.body.pos.y > -500.0);
        self.swarm.flights = flights;
        for (kind, id, owner, inside, from, e) in events {
            match e {
                thrown::Event::Struck { point, surface, .. } => {
                    if kind != thrown::Kind::PetrolBomb {
                        self.swarm_emit(if surface == thrown::Surface::Steel { SwarmSound::GrenadeOnSteel } else { SwarmSound::GrenadeInDirt }, point);
                    }
                }
                thrown::Event::Burst { point } => self.swarm_grenade_burst(kind, id, owner, point, inside),
                thrown::Event::Shattered { point, normal, surface } => self.swarm_bottle_breaks(point, normal, surface, from),
            }
        }
    }

    /// A grenade's burst: the existing exterior blast law for the men round it, the crew's
    /// casualty route for the crew it reaches (fragments swept against the source armour and the
    /// lids, pressure where the path is open), the existing burst's report, front and flash.
    fn swarm_grenade_burst(&mut self, kind: thrown::Kind, id: u32, owner: usize, point: Vec3, inside: bool) {
        self.swarm.stats.bursts += 1;
        let filler_kg = kind.filler_kg();
        let filler_j = filler_kg * 4.184e6;
        let radius = 7.0 * (filler_kg / 0.9).cbrt();
        let models = self.fire_surface_models();
        // Is it on (or in) the Tiger? Within 0.5 m of its source surfaces.
        let near_steel = !self.cabin_fire.outside_source_bounds(point, &models, 0.5);
        if inside {
            self.swarm.stats.bursts_inside += 1;
            self.spawn_interior_blast(point, 0.35, 2.0, 0.08);
        } else {
            self.weapon_vfx.blast(point, Vec3::Y, 0.9);
            // The men round it (boarders included): the existing exterior law, terrain and walls
            // shielding; the Tiger's armour shields whoever it is between.
            self.infantry_pressure_blast(point, Vec3::NEG_Y, radius, filler_j, kind.name());
            self.field_people_he_blast(0x5A00_0000_0000 | u64::from(id), point, Vec3::NEG_Y, radius, filler_j);
            if self.mode == Mode::Foot && !self.god {
                let eye = self.foot.eye();
                let d = eye.distance(point);
                let blocked = !self.los_clear(point + Vec3::Y * 0.05, eye) || self.cabin_fire.sweep(point, eye, &models).is_some();
                let damage = combat::he_blast_damage(d, filler_kg, blocked).min(self.foot.hp);
                self.foot.hp = (self.foot.hp - damage).max(0.0);
            }
        }
        self.swarm_crew_burst(kind, id, point);
        let burst = crate::sound_propagation::Emission::he_burst(filler_kg, kind.mass_kg() - filler_kg);
        // HT-18 (generated) replaces the burst's report on the steel only behind the switch.
        let generated = (inside || near_steel) && crate::sfx_rows::generated_switch()
            && !crate::sfx_rows::takes("HT-18", false).is_empty();
        if generated {
            let stereo = self.generated_mono(&crate::sfx_rows::takes("HT-18", false)[(id % 2) as usize % crate::sfx_rows::takes("HT-18", false).len()])
                .map(|m| m.iter().flat_map(|v| [*v * std::f32::consts::FRAC_1_SQRT_2; 2]).collect::<Vec<f32>>());
            self.schedule_emission(crate::audio::Sfx::KillExplosion, point, burst, None, stereo);
        } else {
            self.schedule_emission(crate::audio::Sfx::KillExplosion, point, burst, None, None);
        }
        self.queue_blast_front(0x5A00_0000 | u64::from(id), point, filler_kg, true);
        if inside || near_steel { let t = self.swarm.clock_s; self.swarm.sounds.emit(SwarmSound::GrenadeBurstOnDeck, point, t, id); }
        let _ = owner;
    }

    fn swarm_crew_burst(&mut self, kind: thrown::Kind, id: u32, point: Vec3) {
        let _probe = crate::sim_phase::Probe::new("swarm_crew_burst");
        let bodies = self.small_arm_crew_bodies();
        if bodies.is_empty() { return; }
        let hull = self.player.hull_model();
        let models = self.fire_surface_models();
        let filler_j = kind.filler_kg() * 4.184e6;
        let (count, mass, v0) = kind.fragments();
        let mut energy = vec![[0.0f32; crew_physics::CREW_BODY_PART_COUNT]; bodies.len()];
        for dir in thrown::fragment_directions(count, id) {
            let start = point + dir * 0.03;
            let end = point + dir * 10.0;
            let armour = self.small_arm_player_armor(start, end).map_or(f32::INFINITY, |c| c.t_m);
            if let Some(c) = self.small_arm_exposed_crew_contact_with(&bodies, start, end) {
                if c.t_m < armour {
                    if let ProjectileContactKind::ExposedCrew(casualty, part) = c.kind {
                        if let Some(b) = bodies.iter().position(|b| b.casualty.peer_id == casualty.peer_id) {
                            let v = v0 * (-c.t_m / FRAGMENT_DECAY_M).exp();
                            energy[b][part.index()] += 0.5 * mass * v * v;
                        }
                    }
                }
            }
        }
        for (b, body) in bodies.iter().enumerate() {
            if self.singleplayer_actor_casualty(body.casualty.peer_id).is_some_and(|m| m.dead) { continue; }
            // One wound per crewman per burst: the part the burst does the most work on (named).
            let mut best: Option<(f32, crew_physics::CrewBodyPart, f32, f32, f32, Vec3)> = None;
            for part in crew_physics::CrewBodyPart::ALL {
                let v = body.hit_volumes[part.index()];
                if v.half_extents.max_element() <= 0.0 { continue; }
                let center = hull.transform_point3(v.center);
                let distance = center.distance(point).max(crew_damage::MIN_BLAST_DISTANCE_M);
                let open = self.cabin_fire.sweep(point, center, &models).is_none();
                let blast = if open && distance < 6.0 { filler_j } else { 0.0 };
                let frag = energy[b][part.index()];
                if blast <= 0.0 && frag <= 0.0 { continue; }
                let result = crew_damage::resolve_crew_damage(crew_damage::CrewDamageInput {
                    part, fragment_energy_j: frag, explosive_energy_j: blast, blast_distance_m: distance });
                if result.outcome == crew_damage::CrewTissueOutcome::None { continue; }
                if best.is_none_or(|x| result.total_work_j > x.0) { best = Some((result.total_work_j, part, frag, blast, distance, center)); }
            }
            if let Some((_, part, frag, blast, distance, center)) = best {
                self.swarm.stats.crew_wounds += 1;
                println!("[swarm] {} burst reaches {:?}'s {part:?} ({frag:.0} J fragments, {:.0} kJ blast at {distance:.2} m)",
                    kind.name(), body.casualty.role, blast / 1000.0);
                self.injure_field_actor(body.casualty, part, center, (center - point).normalize_or_zero() * v0, frag, blast, distance,
                    0x5A00_0000_0000 | u64::from(id));
            }
        }
    }

    /// A bottle breaks: on the Tiger's steel it lights `SPLASH_PATCHES` source-surface fire patches
    /// round the point (the existing surface fire); on the ground its petrol is not modelled.
    fn swarm_bottle_breaks(&mut self, point: Vec3, normal: Vec3, surface: thrown::Surface, from: Vec3) {
        self.swarm_emit(SwarmSound::PetrolBomb, point);
        if surface != thrown::Surface::Steel || !self.player.alive {
            self.swarm.stats.bottles_broken_on_ground += 1;
            return;
        }
        self.swarm.stats.bottles_broken_on_tank += 1;
        let models = self.fire_surface_models();
        let seed = self.swarm.next_id ^ self.swarm.tick as u32;
        let mut ids = Vec::new();
        for p in xfire::splash_points(point, normal, seed) {
            if let Some(id) = self.cabin_fire.seed_from_segment_identity(p + normal * 0.25, p - normal * 0.25, &models, 1.0) {
                ids.push(id);
            }
        }
        if ids.is_empty() { return; }
        self.swarm.stats.deck_patches += ids.len() as u64;
        if !self.fire_active { self.fire_t = 0.0; }
        self.fire_active = true;
        self.fire = self.fire.max(self.cabin_fire.strongest());
        self.fire_origin.get_or_insert((from, point, (point - from).normalize_or_zero()));
        println!("[swarm] petrol bomb on the Tiger at {:.1?}: {} patches lit", point.to_array(), ids.len());
        self.swarm.fires.push(xfire::BottleFire::new(ids, thrown::Kind::PetrolBomb.petrol_kg()));
        self.swarm_emit(SwarmSound::DeckFire, point);
    }

    /// Where each burning patch is (crew air, engine bay, an outside intake, outside plate),
    /// classified once per patch identity from the source geometry.
    fn swarm_patch_place(&mut self, identity: u32, surface: &surface_fire::FireSurface, models: &[Mat4; surface_fire::FRAMES]) -> xfire::Place {
        if let Some(&place) = self.swarm.classes.get(&identity) { return place; }
        let n = surface.normal.normalize_or(Vec3::Y);
        let outside = self.cabin_fire.sweep(surface.position + n * 0.03, surface.position + n * 3.0, models).is_none();
        let local = self.player.hull_model().inverse().transform_point3(surface.position);
        let place = xfire::classify(outside, local, game_fea::in_crew_air(ArmorPart::Hull, local), &self.swarm.intakes);
        self.swarm.classes.insert(identity, place);
        place
    }

    /// The burning surfaces' heat into the crew compartments' air (W) by the swarm's heat path
    /// (`exterior_fire`); `game_cookoff` uses it in place of its own share while the swarm is on.
    pub(super) fn swarm_fire_w(&mut self, patch_heat_w: f32, firewall_share: f32) -> f32 {
        let models = self.fire_surface_models();
        let mut total = 0.0;
        for k in 0..surface_fire::MAX_PATCHES {
            let Some(s) = self.cabin_fire.patch_surface(k, &models) else { continue };
            if s.intensity <= 0.02 { continue; }
            let place = self.swarm_patch_place(s.identity, &s, &models);
            total += xfire::crew_air_share(place, firewall_share) * s.intensity * patch_heat_w;
        }
        total
    }

    fn swarm_quench(&mut self, ids: &[u32]) {
        for &id in ids { self.cabin_fire.quench(id); }
        let strongest = self.cabin_fire.strongest();
        self.fire = strongest;
        if strongest <= 0.0 { self.fire_active = false; }
    }

    fn swarm_step_fires(&mut self, dt: f32) {
        let _probe = crate::sim_phase::Probe::new("swarm_step_fires");
        let models = self.fire_surface_models();
        // The burning patches now, by identity.
        let mut burning: Vec<(u32, surface_fire::FireSurface)> = Vec::new();
        for k in 0..surface_fire::MAX_PATCHES {
            if let Some(s) = self.cabin_fire.patch_surface(k, &models) { if s.intensity > 0.02 { burning.push((s.identity, s)); } }
        }
        let mut places = std::collections::HashMap::new();
        for (id, s) in &burning { let p = self.swarm_patch_place(*id, s, &models); places.insert(*id, p); }
        let mut fires = std::mem::take(&mut self.swarm.fires);
        let mut quench = Vec::new();
        let mut light_bay = Vec::new();
        for fire in &mut fires {
            let alive: Vec<u32> = fire.patches.iter().copied().filter(|id| places.contains_key(id)).collect();
            let over = alive.iter().filter(|id| places[*id] == xfire::Place::Intake).count();
            let (spent, light) = fire.advance(dt, alive.len(), over);
            if spent { quench.extend(alive.iter().copied().filter(|id| places[id] != xfire::Place::Intake)); }
            if light {
                if let Some((_, s)) = burning.iter().find(|(id, _)| alive.contains(id) && places[id] == xfire::Place::Intake) { light_bay.push(*s); }
            }
            fire.patches = alive;
        }
        fires.retain(|f| !f.patches.is_empty());
        self.swarm.fires = fires;
        if !quench.is_empty() { self.swarm_quench(&quench); }
        for s in light_bay {
            let n = s.normal.normalize_or(Vec3::Y);
            let from = s.position - n * 0.06;
            let to = from - Vec3::Y * 1.6;
            if let Some(id) = self.cabin_fire.seed_from_segment_identity(from, to, &models, 1.0) {
                self.swarm.classes.insert(id, xfire::Place::EngineBay);
                self.swarm.stats.engine_bay_fires += 1;
                self.fire = self.fire.max(self.cabin_fire.strongest());
                println!("[swarm] the engine bay has caught under the burning intake");
                self.swarm_emit(SwarmSound::EngineBayFire, s.position);
            }
        }
        // The fixed engine-bay extinguisher: once, after its delay.
        let bay: Vec<u32> = burning.iter().filter(|(id, _)| places.get(id) == Some(&xfire::Place::EngineBay)).map(|(id, _)| *id).collect();
        if bay.is_empty() { self.swarm.bay_burning_s = 0.0; }
        else if !self.swarm.fixed_used {
            self.swarm.bay_burning_s += dt;
            if self.swarm.bay_burning_s >= xfire::FIXED_EXTINGUISHER_DELAY_S {
                self.swarm.fixed_used = true;
                self.swarm.stats.fixed_extinguisher += 1;
                let at = burning.iter().find(|(id, _)| bay.contains(id)).map_or(self.player.center(), |(_, s)| s.position);
                println!("[swarm] the fixed engine-bay extinguisher discharges ({} patches; its one charge)", bay.len());
                self.swarm_emit(SwarmSound::FixedExtinguisher, at);
                self.swarm_quench(&bay);
            }
        }
    }

    fn swarm_drive_thrown(&mut self, r: &mut Renderer) {
        let mut used = [false; N_THROWN_SLOTS];
        for f in &self.swarm.flights {
            let parity = usize::from(f.body.kind == thrown::Kind::PetrolBomb);
            let Some(k) = (0..N_THROWN_SLOTS).find(|&k| k % 2 == parity && !used[k]) else { continue };
            used[k] = true;
            let slot = self.swarm.thrown_slots[k];
            let dir = f.body.vel.normalize_or(Vec3::Z);
            let spin = Quat::from_rotation_arc(Vec3::Z, if f.body.resting { flat(dir).normalize_or(Vec3::X) } else { dir });
            r.set_visible(slot, true);
            r.set_model(slot, Mat4::from_rotation_translation(spin, f.body.pos));
        }
        for (k, &slot) in self.swarm.thrown_slots.iter().enumerate() {
            if !used[k] { r.set_visible(slot, false); }
        }
    }

    // ── Wingmen ───────────────────────────────────────────────────────────────────────────

    /// The nearest man on the leader's hull a wingman can see from its coaxial MG (platoon mode).
    pub(super) fn swarm_boarder_target_for(&self, ally: usize) -> Option<Vec3> {
        if !self.swarm.enabled { return None; }
        let tank = &self.allies.get(ally)?.tank;
        if !tank.alive { return None; }
        let (port, _) = tank.coax_muzzle();
        self.swarm.minds.iter().enumerate()
            .filter(|(i, m)| m.boarder.is_some_and(|b| matches!(b.state, BoardState::Walk | BoardState::Work { .. }))
                && self.soldiers.get(*i).is_some_and(|s| s.alive))
            .map(|(i, _)| self.soldiers[i].pos + Vec3::Y * 1.1)
            .filter(|p| p.distance(port) < 200.0 && self.los_clear(port, *p))
            .min_by(|a, b| a.distance(port).total_cmp(&b.distance(port)))
    }

    /// A laid wingman sweeps the leader's deck with its coaxial MG (the ordinary projectile path;
    /// rounds that strike the leader's armour ring off it, any that find an exposed crewman wound
    /// him: friendly fire is physical).
    pub(super) fn swarm_wingman_mg(&mut self, ally: usize, dt: f32) {
        if self.swarm.wingman_mg.len() <= ally { self.swarm.wingman_mg.resize(ally + 1, (0.0, 1500)); }
        let Some((port, dir)) = self.allies.get(ally).map(|a| a.tank.coax_muzzle()) else { return };
        let (mut timer, mut ammo) = self.swarm.wingman_mg[ally];
        let rounds = combat::cycle_machine_gun(&mut timer, &mut ammo, true, dt);
        self.swarm.wingman_mg[ally] = (timer, ammo);
        for k in 0..rounds {
            let j = |n: u32| ((n.wrapping_mul(2654435761) >> 8) & 0xffff) as f32 / 32768.0 - 1.0;
            let seed = (self.swarm.tick as u32) ^ (k << 8) ^ (ally as u32) << 16;
            let d = (dir + Vec3::new(j(seed), j(seed ^ 3), j(seed ^ 9)) * 0.006).normalize_or_zero();
            let shot = combat::Shot { shooter: 0xA11E_0000 + ally as u32, team: combat::Team::German, origin: port, direction: d,
                range: combat::MG_RANGE.min(400.0), weapon: combat::Weapon::MachineGun };
            self.launch_small_arm(shot, combat::small_arms_damage(combat::Weapon::MachineGun, false), true, None);
            self.swarm.stats.wingman_mg_rounds += 1;
        }
        if rounds > 0 && self.swarm.stats.wingman_mg_rounds % 25 < rounds as u64 { self.swarm_emit(SwarmSound::SweptByMg, port + dir * 12.0); }
    }

    // ── Read-outs ─────────────────────────────────────────────────────────────────────────

    pub fn debug_swarm_summary(&self) -> serde_json::Value {
        if !self.swarm.enabled { return serde_json::json!({"enabled": false}); }
        let count = |pred: &dyn Fn(&Mind) -> bool| self.swarm.minds.iter().enumerate()
            .filter(|(i, m)| self.soldiers.get(*i).is_some_and(|s| s.alive) && pred(m)).count();
        serde_json::json!({
            "enabled": true,
            "seeing_tank": count(&|m| m.sighting.is_some()),
            "prone": self.soldiers.iter().take(self.soviet_soldier_count).filter(|s| s.alive && s.gone_to_ground).count(),
            "rushing": count(&|m| matches!(m.intent, Some(swarm::Intent::Rush(_)))),
            "throwing": count(&|m| m.windup.is_some()),
            "on_hull": count(&|m| m.boarder.is_some_and(|b| matches!(b.state, BoardState::Walk | BoardState::Work { .. } | BoardState::Climb { .. }))),
            "in_hatch": count(&|m| m.boarder.is_some_and(|b| matches!(b.state, BoardState::InHatch(_)))),
            "in_flight": self.swarm.flights.len(),
            "bottle_fires": self.swarm.fires.len(),
            "stats": self.swarm.stats.clone(),
            "sounds": self.swarm.sounds.summary(),
        })
    }

    /// Soviet riflemen alive, and those within `radius` of the Tiger.
    pub fn debug_soviet_infantry_counts(&self, radius: f32) -> (usize, usize) {
        let alive: Vec<&Soldier> = self.soldiers.iter().take(self.soviet_soldier_count).filter(|s| s.alive).collect();
        let near = alive.iter().filter(|s| s.pos.distance(self.player.pos) <= radius).count();
        (alive.len(), near)
    }

    /// The infantry phase's accumulated milliseconds (a bench reads the difference per frame).
    pub fn debug_infantry_phase_ms(&self) -> f64 {
        self.sim_phase.ms(crate::sim_phase::Phase::Infantry)
    }

    /// Disclosed fixture: stage `count` riflemen in a ring `range_m` from the Tiger, each aware of
    /// it (a sighting as if he had just looked), for a bench or a still of the swarm's behaviour.
    pub fn debug_stage_swarm_ring(&mut self, count: usize, range_m: f32) -> usize {
        if !self.swarm.enabled { return 0; }
        let center = self.player.pos;
        let n = self.soviet_soldier_count.min(self.soldiers.len());
        let mut placed = 0;
        let look = self.swarm_tank_look();
        let now = self.swarm.clock_s;
        for i in 0..n {
            if placed >= count { break; }
            if !self.soldiers[i].alive { continue; }
            let a = placed as f32 / count.max(1) as f32 * std::f32::consts::TAU;
            let r = range_m + (placed % 5) as f32 * 3.0;
            let p = center + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            let p = Vec3::new(p.x, self.hf.height_at(p.x, p.z), p.z);
            self.soldiers[i].activate_escort([p; 4], &self.hf);
            self.soldiers[i].yaw = yaw_toward(p, center);
            self.infantry_casualties.clear_actor(i);
            if let Some(actor) = self.mixed_combat.actors.get_mut(i) { *actor = combat::Combatant::new(actor.id, actor.team, p, self.soldiers[i].yaw); }
            let m = self.swarm.mind(i);
            m.sighting = Some(swarm::Sighting { look, at_s: now, traverse_rad_s: 0.0 });
            m.boarder = None; m.windup = None; m.intent = None;
            placed += 1;
        }
        placed
    }
}

/// Decode a take file to mono at the engine's rate (linear resampling).
fn decode_take(path: &str) -> Option<Vec<f32>> {
    use rodio::Source;
    let file = crate::resource::open(path).ok()?;
    let decoder = rodio::Decoder::new(std::io::BufReader::new(file)).ok()?;
    let channels = decoder.channels().max(1) as usize;
    let rate = decoder.sample_rate().max(1) as f32;
    let samples: Vec<i16> = decoder.collect();
    let mono: Vec<f32> = samples.chunks(channels).map(|c| c.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32).collect();
    if mono.is_empty() { return None; }
    let out_rate = crate::acoustics::SAMPLE_RATE as f32;
    let n = (mono.len() as f32 * out_rate / rate) as usize;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let x = k as f32 * rate / out_rate;
        let i = x as usize;
        let f = x - i as f32;
        let a = mono[i.min(mono.len() - 1)];
        let b = mono[(i + 1).min(mono.len() - 1)];
        out.push(a + (b - a) * f);
    }
    Some(out)
}
