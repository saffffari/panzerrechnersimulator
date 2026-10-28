//! Platoon mode: two AI-driven Tigers under the player's command, ordered and answered over the
//! radio. Child module of `game.rs` (like `canyon_pilot.rs`) so it can drive the existing
//! `AllyTiger` rigs, the authoritative projectile path and the private avoidance helpers without
//! widening `game.rs`'s public surface.
//!
//! Division of labour:
//! * `radio_llm.rs` turns the commander's words into ONE validated [`RadioOrder`] and the wingmen's
//!   reply lines. It never touches a tank.
//! * This module owns the deterministic part: wingman state (callsign, crew, ammunition, damage),
//!   order execution (drive/steer/lay/fire every frame), the context the model is handed, the
//!   wingmen's damage model, and the typed radio line the player uses in the window.
//!
//! Mortality: a wingman uses the same `ArmorScheme::TigerI` swept armour contact and `ballistics`
//! solver as the player's hull (`nearest_projectile_contact_with` / `resolve_authoritative_terminal`).
//! What happens BEHIND a penetrated plate is a simplified, deterministic crew-and-module casualty
//! model (`on_penetration`): it is not the player's fragment shower. It is named as such here and in
//! docs/PHYSICAL_TRUTH_AUDIT.md.

use super::{
    apply_vehicle_avoidance, ballistic_aim_target, vehicle_inverse_mass, vehicle_motion, vehicle_pair_resolution, Game,
    ProjectileShooter,
};
use crate::radio_llm::{
    Addressee, ContactContext, OrderKind, OrderTarget, RadioContext, RadioLlm, RadioOrder, RadioResult,
    WingmanContext,
};
use crate::tank::{self, Tank};
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

pub const ENV_PLATOON: &str = "KESSEL_PLATOON";
/// Wingmen fielded by `KESSEL_PLATOON=2` (the only size the roster below supports).
pub const WINGMEN: usize = 2;
/// Reply lines fade from the HUD after this long.
pub const RADIO_LINE_LIFE_S: f32 = 14.0;
const RADIO_LOG_LINES: usize = 5;
const FORMATION_BACK_M: [f32; 2] = [22.0, 30.0];
const FORMATION_SIDE_M: f32 = 16.0;
const FLANK_SIDE_M: f32 = 90.0;
const FLANK_AHEAD_M: f32 = 120.0;
const ENGAGE_RANGE_M: f32 = 1200.0;
const COMBAT_STOP_RANGE_M: f32 = 900.0;
const CLOSE_CONTACT_M: f32 = 500.0;
const FALL_BACK_M: f32 = 120.0;
const RELOAD_S: f32 = 9.0;
/// Under `follow`, the trained policy fights only while the wingman stays this close to the
/// leader; beyond it the order controller drives back to formation.
const POLICY_LEASH_M: f32 = 250.0;
/// Longest short halt the driver makes for a policy shot before letting the policy move again
/// (a target that will not come into lay must not pin the tank in the open).
const SHORT_HALT_MAX_S: f32 = 6.0;
/// The neuroevolution Tiger policy from the duel training (14 obs -> drive, steer, fire).
pub const DEFAULT_POLICY: &str = "assets/ai/weights_tiger.bin";
const ARRIVE_M: f32 = 6.0;

/// Does the environment ask for the platoon? Only `KESSEL_PLATOON=2` is a valid size today.
pub fn requested() -> bool {
    std::env::var("KESSEL_PLATOON").map(|v| v.trim() == "2").unwrap_or(false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrewRole {
    Commander,
    Gunner,
    Loader,
    Driver,
    RadioOperator,
}

impl CrewRole {
    pub const ALL: [CrewRole; 5] =
        [CrewRole::Commander, CrewRole::Gunner, CrewRole::Loader, CrewRole::Driver, CrewRole::RadioOperator];
    pub fn name(self) -> &'static str {
        match self {
            CrewRole::Commander => "commander",
            CrewRole::Gunner => "gunner",
            CrewRole::Loader => "loader",
            CrewRole::Driver => "driver",
            CrewRole::RadioOperator => "radio operator",
        }
    }
    /// Where the man sits, in hull-local metres (x right, y up, z forward), for the casualty model.
    fn seat(self) -> Vec3 {
        match self {
            CrewRole::Commander => Vec3::new(-0.55, 1.75, -0.25),
            CrewRole::Gunner => Vec3::new(-0.55, 1.45, 0.35),
            CrewRole::Loader => Vec3::new(0.60, 1.45, 0.25),
            CrewRole::Driver => Vec3::new(-0.65, 0.95, 1.95),
            CrewRole::RadioOperator => Vec3::new(0.65, 0.95, 1.95),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    Gun,
    Turret,
    Engine,
    Tracks,
    Radio,
    Optics,
}

impl Module {
    pub fn name(self) -> &'static str {
        match self {
            Module::Gun => "gun",
            Module::Turret => "turret traverse",
            Module::Engine => "engine",
            Module::Tracks => "track",
            Module::Radio => "radio",
            Module::Optics => "optics",
        }
    }
}

/// The standing order a wingman is executing. `Follow` is the default and what `cease_fire`
/// / `report_status` / `spot` leave untouched (they are not movement orders).
#[derive(Clone, Debug, PartialEq)]
pub enum Standing {
    Follow,
    Hold,
    Advance(Vec2),
    Flank { side: f32, goal: Vec2 },
    Cover,
    FallBack(Vec2),
}

impl Standing {
    fn label(&self) -> String {
        match self {
            Standing::Follow => "follow".into(),
            Standing::Hold => "hold".into(),
            Standing::Advance(_) => "advance".into(),
            Standing::Flank { side, .. } => if *side < 0.0 { "flank_left".into() } else { "flank_right".into() },
            Standing::Cover => "cover_me".into(),
            Standing::FallBack(_) => "fall_back".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Wingman {
    pub callsign: String,
    pub commander: String,
    pub personality: String,
    pub standing: Standing,
    /// `engage` picked a contact or sector: what the gun prefers. `None` = nearest threat.
    pub engage: Option<OrderTarget>,
    pub weapons_hold: bool,
    pub ap_rounds: u32,
    pub he_rounds: u32,
    pub crew_alive: [bool; 5],
    pub modules_ok: [bool; 6],
    pub hull_hits: u32,
    pub penetrations: u32,
    pub fire_cd: f32,
    pub shots: u32,
    pub kills: u32,
    pub burning: bool,
    /// Seconds since the tank last answered; the HUD and the model both read it.
    pub last_reply: Option<String>,
    /// The trained policy is driving this frame (in contact under an order that allows it).
    pub policy_engaged: bool,
    /// Seconds spent under the trained policy, for the proof receipt.
    pub policy_s: f32,
    /// Seconds of the current short halt for a policy shot.
    pub halt_s: f32,
    stuck_s: f32,
    reverse_until: f32,
    time: f32,
}

impl Wingman {
    fn new(callsign: &str, commander: &str, personality: &str) -> Self {
        Wingman {
            callsign: callsign.into(),
            commander: commander.into(),
            personality: personality.into(),
            standing: Standing::Follow,
            engage: None,
            weapons_hold: false,
            ap_rounds: 52,
            he_rounds: 40,
            crew_alive: [true; 5],
            modules_ok: [true; 6],
            hull_hits: 0,
            penetrations: 0,
            fire_cd: 6.0,
            shots: 0,
            kills: 0,
            burning: false,
            last_reply: None,
            policy_engaged: false,
            policy_s: 0.0,
            halt_s: 0.0,
            stuck_s: 0.0,
            reverse_until: 0.0,
            time: 0.0,
        }
    }

    fn crew(&self, role: CrewRole) -> bool {
        self.crew_alive[CrewRole::ALL.iter().position(|r| *r == role).unwrap()]
    }
    fn module(&self, m: Module) -> bool {
        self.modules_ok[m as usize]
    }
    /// Can the tank move? Needs a driver and an engine and at least one track.
    pub fn can_drive(&self) -> bool {
        self.crew(CrewRole::Driver) && self.module(Module::Engine) && self.module(Module::Tracks)
    }
    /// Can the gun be laid and fired? Gunner + loader + gun; traverse loss means hull-aim only.
    pub fn can_shoot(&self) -> bool {
        self.crew(CrewRole::Gunner) && self.crew(CrewRole::Loader) && self.module(Module::Gun)
    }
    pub fn can_answer(&self) -> bool {
        self.crew(CrewRole::Commander) && self.module(Module::Radio)
    }
    pub fn damage_list(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, ok) in self.modules_ok.iter().enumerate() {
            if !ok {
                let m = [Module::Gun, Module::Turret, Module::Engine, Module::Tracks, Module::Radio, Module::Optics][i];
                out.push(format!("{} out", m.name()));
            }
        }
        if self.burning {
            out.push("fire aboard".into());
        }
        out
    }
    pub fn crew_dead(&self) -> Vec<String> {
        CrewRole::ALL
            .iter()
            .zip(self.crew_alive.iter())
            .filter(|(_, alive)| !**alive)
            .map(|(r, _)| r.name().to_string())
            .collect()
    }
}

/// One line on the HUD radio log.
#[derive(Clone, Debug)]
pub struct RadioLine {
    pub callsign: String,
    pub text: String,
    pub age: f32,
    pub path: &'static str,
}

pub struct Platoon {
    pub wingmen: Vec<Wingman>,
    pub radio: RadioLlm,
    /// The typed push-to-talk line.
    pub typing: bool,
    pub entry: String,
    pub log: Vec<RadioLine>,
    pub placed: bool,
    /// Transmissions in flight: `(request id, text)`.
    inflight: Vec<(u64, String)>,
    time: f32,
    pub enemy_kill_credit: Vec<Option<usize>>,
    squelch_tick: u32,
    /// The engagement policy the wingmen fight with (None: order controller only).
    pub policy: Option<crate::ai::Mlp>,
    /// Where the policy came from, for the logs and the proof receipt.
    pub policy_source: String,
}

impl Platoon {
    pub fn new() -> Self {
        Platoon {
            wingmen: vec![
                Wingman::new("Anton", "Feldwebel Jupp Hartmann", "laconic Eastern Front veteran; counts rounds, never raises his voice"),
                Wingman::new("Berta", "Unteroffizier Karl Weiss", "young, eager, reports more than asked, keen to be first on target"),
            ],
            radio: RadioLlm::new(),
            typing: false,
            entry: String::new(),
            log: Vec::new(),
            placed: false,
            inflight: Vec::new(),
            time: 0.0,
            enemy_kill_credit: Vec::new(),
            squelch_tick: 0,
            policy: None,
            policy_source: String::new(),
        }
        .with_policy_from_env()
    }

    /// `KESSEL_PLATOON_POLICY`: unset = the trained Tiger weights (`DEFAULT_POLICY`), `off` = the
    /// order controller alone, `doctrine` = the hand-built seed network, anything else = a path
    /// to a KSL1 weight artifact. A missing or mismatched artifact falls back to the doctrine
    /// seed, and the log says so.
    fn with_policy_from_env(mut self) -> Self {
        let want = std::env::var("KESSEL_PLATOON_POLICY").unwrap_or_default();
        let want = want.trim();
        let (policy, source) = match want {
            "off" | "none" => (None, "off".to_string()),
            "doctrine" => (Some(crate::ai::Mlp::tiger_doctrine_seed()), "doctrine seed".to_string()),
            path => {
                let path = if path.is_empty() { DEFAULT_POLICY } else { path };
                match crate::ai::Mlp::load(path) {
                    Ok(m) => (Some(m), path.to_string()),
                    Err(e) => {
                        println!("[platoon] policy {path} unavailable ({e:#}); using the doctrine seed");
                        (Some(crate::ai::Mlp::tiger_doctrine_seed()), "doctrine seed (fallback)".to_string())
                    }
                }
            }
        };
        println!("[platoon] engagement policy: {source}");
        self.policy = policy;
        self.policy_source = source;
        self
    }

    pub fn alive_count(&self, game: &Game) -> usize {
        game.allies.iter().filter(|a| a.tank.alive).count()
    }

    pub fn transcript_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.radio.transcript.iter().map(|r| r.to_json()).collect())
    }
}

impl Default for Platoon {
    fn default() -> Self {
        Self::new()
    }
}

fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}
fn xz(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}
fn compass_deg(dir: Vec2) -> f32 {
    // Engine forward = (sin h, cos h): heading h = atan2(x, z); 0 = +Z. Report it as a compass.
    dir.x.atan2(dir.y).to_degrees().rem_euclid(360.0)
}
fn heading_from_compass(deg: f32) -> f32 {
    deg.to_radians()
}
fn navigate(pos: Vec2, heading: f32, goal: Vec2, speed: f32, stop: f32) -> (f32, f32) {
    let to = goal - pos;
    if to.length() <= stop {
        return (0.0, 0.0);
    }
    let error = wrap(to.x.atan2(to.y) - heading);
    let drive = if error.abs() > 0.7 { 0.0 } else { speed * (to.length() / 25.0).clamp(0.35, 1.0) };
    (drive, (error * 2.0).clamp(-0.7, 0.7))
}
fn hash01(n: u32) -> f32 {
    (((n.wrapping_mul(2_654_435_761)) >> 8) & 0xffff) as f32 / 65_536.0
}

/// The trained Tiger policy's observation, built from a wingman's own tank. Same layout and
/// scaling as `Game::tiger_executor_obs` (the training input): target offset and range over
/// 300 m, relative bearing sin/cos, own speed, reload, breech, hp, living-enemy fraction, turret
/// error, the target's facing toward us and its speed, bias. The target is the one the order
/// controller chose (so an `engage contact #n` order points the policy at #n), not merely the
/// nearest. The wingmen have no breech model: `breech_ready` is their reload cycle.
pub fn wingman_policy_obs(own: &Tank, breech_ready: bool, living_frac: f32, target: Option<&Tank>) -> [f32; crate::ai::OBS] {
    let mut obs = [0.0f32; crate::ai::OBS];
    let pc = own.center();
    obs[5] = (own.speed / tank::TOP_SPEED).clamp(-1.0, 1.0);
    obs[6] = own.reload.clamp(0.0, 1.0);
    obs[7] = if breech_ready { 1.0 } else { -1.0 };
    obs[8] = (own.hp / tank::MAX_HP).clamp(0.0, 1.0);
    obs[9] = living_frac.clamp(0.0, 1.0);
    obs[13] = 1.0;
    if let Some(e) = target {
        let to = e.center() - pc;
        let range = Vec2::new(to.x, to.z).length();
        let bearing = to.x.atan2(to.z) - own.heading;
        obs[0] = (to.x / 300.0).clamp(-1.0, 1.0);
        obs[1] = (to.z / 300.0).clamp(-1.0, 1.0);
        obs[2] = (range / 300.0).min(1.0);
        obs[3] = bearing.sin();
        obs[4] = bearing.cos();
        let to_dir = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
        let turret = own.turret_forward();
        let turret_flat = Vec3::new(turret.x, 0.0, turret.z).normalize_or_zero();
        obs[10] = turret_flat.cross(to_dir).y.clamp(-1.0, 1.0);
        let toward_us = Vec3::new(-to.x, 0.0, -to.z).normalize_or_zero();
        obs[11] = e.forward().dot(toward_us).clamp(-1.0, 1.0);
        obs[12] = (e.speed / tank::TOP_SPEED).clamp(-1.0, 1.0);
    }
    obs
}

// ------------------------------------------------------------------------------------------------
// Game hooks
// ------------------------------------------------------------------------------------------------

impl Game {
    /// Is the platoon active (wingmen fielded)?
    pub fn platoon_active(&self) -> bool {
        self.platoon.is_some() && !self.allies.is_empty()
    }

    /// Enable the platoon on a built world that already carries ally rigs. Called by the title
    /// menu / env hook after `enable_canyon_course*`. The wingmen are placed in formation behind
    /// the player on the first update, once the player's course start is final.
    pub fn enable_platoon(&mut self) {
        if self.allies.is_empty() {
            println!("[platoon] no ally rigs were built for this world (platoon needs {} ally Tiger slots)", WINGMEN);
            return;
        }
        let mut p = Platoon::new();
        p.enemy_kill_credit = vec![None; self.enemies.len()];
        for ally in &mut self.allies {
            ally.tank.alive = true;
            ally.tank.hp = tank::MAX_HP;
        }
        println!(
            "[platoon] fielded {} wingmen: {}",
            self.allies.len(),
            p.wingmen.iter().map(|w| format!("{} ({})", w.callsign, w.commander)).collect::<Vec<_>>().join(", ")
        );
        self.platoon = Some(p);
    }

    /// Where the enemy AI believes the nearest Tiger is: the player (or the man on foot) in every
    /// other mode; in platoon mode also the wingmen, so T-34s fight the whole platoon.
    pub(crate) fn enemy_threat_center(&self, i: usize) -> Vec3 {
        let base = self.threat_center();
        if self.platoon.is_none() || self.allies.is_empty() {
            return base;
        }
        let from = self.enemies.get(i).map(|e| e.center()).unwrap_or(base);
        let mut best = (from.distance_squared(base) * if self.player.alive { 1.0 } else { 4.0 }, base);
        for ally in &self.allies {
            if !ally.tank.alive {
                continue;
            }
            let c = ally.tank.center();
            let d = from.distance_squared(c);
            if d < best.0 {
                best = (d, c);
            }
        }
        best.1
    }

    fn platoon_formation_point(&self, index: usize) -> Vec2 {
        let side = if index == 0 { -1.0 } else { 1.0 };
        let fwd = xz(self.player.forward()).normalize_or_zero();
        let right = Vec2::new(fwd.y, -fwd.x);
        xz(self.player.pos) - fwd * FORMATION_BACK_M[index.min(1)] + right * side * FORMATION_SIDE_M
    }

    /// Teleport the wingmen into formation behind the player (first frame, or a restart).
    pub fn platoon_place_in_formation(&mut self) {
        for i in 0..self.allies.len().min(WINGMEN) {
            let p = self.platoon_formation_point(i);
            let heading = self.player.heading;
            let mut tank = Tank::new(p.x, p.y, heading, &self.hf, tank::TIGER_PARTS);
            tank.drops_into_pits = false;
            self.allies[i].tank = tank;
            self.allies[i].formation_side = if i == 0 { -1.0 } else { 1.0 };
        }
        if let Some(p) = self.platoon.as_mut() {
            p.placed = true;
        }
    }

    /// Contacts a wingman (or the leader) can actually see: active, alive-or-burning T-34s with a
    /// clear optical line, labelled by stable enemy index so the model's `contact T-34 #n` maps back.
    fn platoon_contacts_from(&self, eye: Vec3, limit: usize) -> Vec<ContactContext> {
        let mut out: Vec<(f32, ContactContext)> = Vec::new();
        for (i, e) in self.enemies.iter().enumerate() {
            if !self.enemy_active.get(i).copied().unwrap_or(false) {
                continue;
            }
            let c = e.center();
            let d = eye.distance(c);
            if d > 1500.0 || !self.optical_los_clear(eye, c) {
                continue;
            }
            let state = if !e.alive {
                "dead"
            } else if e.speed.abs() > 0.5 {
                "moving"
            } else {
                "stationary"
            };
            let flat = xz(c) - xz(eye);
            out.push((
                d,
                ContactContext {
                    label: format!("{} #{}", e.kind.name(), i + 1),
                    bearing_deg: compass_deg(flat),
                    range_m: d,
                    state: state.into(),
                },
            ));
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().take(limit).map(|(_, c)| c).collect()
    }

    /// Route knots ahead of the player, named so an `advance to marker` order can point at them.
    fn platoon_markers(&self) -> Vec<(String, Vec2)> {
        let Some(spec) = self.canyon_spec.as_ref() else { return Vec::new() };
        let here = xz(self.player.pos);
        // The route edge nearest the leader; the markers are the knots after it, 30 m or more away.
        let mut best = (f32::INFINITY, 0usize);
        for (i, e) in spec.route.windows(2).enumerate() {
            let ab = e[1] - e[0];
            let t = if ab.length_squared() > 1e-6 { ((here - e[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            let d = (e[0] + ab * t).distance_squared(here);
            if d < best.0 {
                best = (d, i);
            }
        }
        let mut out = Vec::new();
        for (k, knot) in spec.route.iter().enumerate().skip(best.1 + 1) {
            if knot.distance(here) >= 30.0 {
                out.push((format!("Wegpunkt {k}"), *knot));
                if out.len() >= 3 {
                    break;
                }
            }
        }
        out
    }

    /// The truth the model (and the offline parser) is handed. Everything here is read from the
    /// simulation; nothing is invented for the reply.
    pub fn platoon_radio_context(&self) -> RadioContext {
        let Some(p) = self.platoon.as_ref() else { return RadioContext::default() };
        let leader_eye = self.player.center() + Vec3::Y * 1.2;
        let mut ctx = RadioContext {
            leader_heading_deg: compass_deg(xz(self.player.forward())),
            leader_speed_kph: self.player.speed.abs() * 3.6,
            clock: if self.night { "night".into() } else { "day".into() },
            weather: if self.darkness > 0.5 { "dark".into() } else { "winter overcast".into() },
            leader_contacts: self.platoon_contacts_from(leader_eye, 4),
            markers: self.platoon_markers().into_iter().map(|(n, _)| n).collect(),
            wingmen: Vec::new(),
        };
        for (i, w) in p.wingmen.iter().enumerate() {
            let Some(ally) = self.allies.get(i) else { continue };
            let eye = ally.tank.center() + Vec3::Y * 1.2;
            let rel = xz(ally.tank.pos) - xz(self.player.pos);
            ctx.wingmen.push(WingmanContext {
                callsign: w.callsign.clone(),
                commander: w.commander.clone(),
                personality: w.personality.clone(),
                alive: ally.tank.alive && w.can_answer(),
                bearing_from_leader_deg: compass_deg(rel),
                range_from_leader_m: rel.length(),
                heading_deg: compass_deg(xz(ally.tank.forward())),
                speed_kph: ally.tank.speed.abs() * 3.6,
                current_order: if w.weapons_hold { format!("{} (weapons hold)", w.standing.label()) } else { w.standing.label() },
                ap_rounds: w.ap_rounds,
                he_rounds: w.he_rounds,
                hull_hits: w.hull_hits,
                damage: w.damage_list(),
                crew_dead: w.crew_dead(),
                contacts: if ally.tank.alive { self.platoon_contacts_from(eye, 4) } else { Vec::new() },
            });
        }
        ctx
    }

    /// Key the set: queue a transmission. The reply arrives through `platoon_update` on a later
    /// frame; the order is applied there, at a frame boundary.
    pub fn platoon_transmit(&mut self, text: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let ctx = self.platoon_radio_context();
        let p = self.platoon.as_mut()?;
        let id = p.radio.submit(text, ctx);
        p.inflight.push((id, text.to_string()));
        p.log.push(RadioLine { callsign: crate::radio_llm::PLAYER_CALLSIGN.into(), text: text.into(), age: 0.0, path: "tx" });
        p.log.truncate_front(RADIO_LOG_LINES);
        println!("[radio] #{id} tx \"{text}\"");
        self.platoon_squelch(0.6);
        Some(id)
    }

    /// A short synthesized squelch burst: the set keying up / a reply breaking squelch. Rendered
    /// as a plain stereo buffer on the headset bus; no asset exists for it in the library.
    fn platoon_squelch(&mut self, volume: f32) {
        let Some(p) = self.platoon.as_mut() else { return };
        p.squelch_tick = p.squelch_tick.wrapping_add(1);
        let seed = p.squelch_tick;
        let Some(audio) = self.audio.as_ref() else { return };
        let rate = 44_100usize;
        let len = rate / 9; // ~110 ms
        let mut samples = Vec::with_capacity(len * 2);
        for n in 0..len {
            let t = n as f32 / rate as f32;
            let env = if t < 0.012 { t / 0.012 } else { (1.0 - (t - 0.012) / 0.1).clamp(0.0, 1.0) };
            let noise = hash01(seed.wrapping_mul(7919).wrapping_add(n as u32)) * 2.0 - 1.0;
            let tone = (t * 2_600.0 * TAU).sin() * 0.25 * (1.0 - t * 8.0).clamp(0.0, 1.0);
            let s = (noise * 0.55 + tone) * env * 0.35;
            samples.push(s);
            samples.push(s);
        }
        audio.play_stereo(samples, volume);
    }

    /// Apply one validated radio result: the order goes to the addressed wingmen, replies go on the
    /// HUD log. Runs on the frame the worker finished; the sim stays deterministic given the
    /// sequence of (frame, order) pairs, which the proof logs.
    fn platoon_apply(&mut self, result: &RadioResult) {
        let markers = self.platoon_markers();
        let leader_pos = xz(self.player.pos);
        let leader_fwd = xz(self.player.forward()).normalize_or_zero();
        let leader_heading = self.player.heading;
        let enemies: Vec<(usize, Vec2, bool)> =
            self.enemies.iter().enumerate().map(|(i, e)| (i, xz(e.center()), e.alive)).collect();
        let Some(p) = self.platoon.as_mut() else { return };
        p.inflight.retain(|(id, _)| *id != result.request_id);
        if let Some(order) = &result.order {
            for (i, w) in p.wingmen.iter_mut().enumerate() {
                if !order.addressee.includes(&w.callsign) {
                    continue;
                }
                apply_order(w, i, order, &markers, leader_pos, leader_fwd, leader_heading, &enemies);
            }
        }
        for reply in &result.replies {
            if let Some(w) = p.wingmen.iter_mut().find(|w| w.callsign == reply.callsign) {
                w.last_reply = Some(reply.text.clone());
            }
            p.log.push(RadioLine { callsign: reply.callsign.clone(), text: reply.text.clone(), age: 0.0, path: result.path.label() });
            println!("[radio] #{} {} ▸ {}", result.request_id, reply.callsign, reply.text);
        }
        p.log.truncate_front(RADIO_LOG_LINES);
        if !result.replies.is_empty() {
            self.platoon_squelch(0.45);
        }
        // Snap movement goals onto the course's measured clear floor, so an order into a rock
        // face becomes an order to the nearest drivable cell.
        let goals: Vec<(usize, Vec2)> = {
            let p = self.platoon.as_ref().unwrap();
            p.wingmen
                .iter()
                .enumerate()
                .filter_map(|(i, w)| match &w.standing {
                    Standing::Advance(g) | Standing::FallBack(g) | Standing::Flank { goal: g, .. } => Some((i, *g)),
                    _ => None,
                })
                .collect()
        };
        let snapped: Vec<(usize, Vec2)> = goals.into_iter().map(|(i, g)| (i, self.platoon_snap_goal(g))).collect();
        let p = self.platoon.as_mut().unwrap();
        for (i, g) in snapped {
            match &mut p.wingmen[i].standing {
                Standing::Advance(x) | Standing::FallBack(x) => *x = g,
                Standing::Flank { goal, .. } => *goal = g,
                _ => {}
            }
        }
    }

    /// The nearest cell the course measured clear for a tank, within 80 m; the goal itself when
    /// the map has no street navigation grid.
    fn platoon_snap_goal(&self, goal: Vec2) -> Vec2 {
        self.winter_encounter
            .as_ref()
            .and_then(|e| e.streets())
            .and_then(|s| s.nav().nearest_clear(goal, 80.0))
            .unwrap_or(goal)
    }

    /// Per-frame platoon step, called from `Game::update` in place of `update_allied_tigers`.
    pub(crate) fn platoon_update(&mut self, dt: f32) {
        self.weather_view.set_cabin_pose(self.player.hull_model(), self.player.turret_model());
        if self.platoon.is_none() || self.allies.is_empty() {
            return;
        }
        if !self.platoon.as_ref().unwrap().placed {
            self.platoon_place_in_formation();
        }
        {
            let n = self.enemies.len();
            let p = self.platoon.as_mut().unwrap();
            if p.enemy_kill_credit.len() < n {
                p.enemy_kill_credit.resize(n, None);
            }
        }
        // Radio traffic finished on the worker: apply at this frame boundary.
        let results = self.platoon.as_mut().unwrap().radio.poll();
        for r in &results {
            self.platoon_apply(r);
        }
        {
            let p = self.platoon.as_mut().unwrap();
            p.time += dt;
            for line in &mut p.log {
                line.age += dt;
            }
            p.log.retain(|l| l.age < RADIO_LINE_LIFE_S * 2.0);
        }

        let player_pos = self.player.pos;
        let leader_fwd = xz(self.player.forward()).normalize_or_zero();
        let leader_heading = self.player.heading;
        let mut fired: Vec<(usize, Option<usize>, Vec3, Vec3, f32)> = Vec::new();
        // Training input slot 9: living fraction of the whole enemy roster.
        let living_frac = self.enemies.iter().filter(|e| e.alive).count() as f32 / self.enemies.len().max(1) as f32;

        // Snapshot of every hull for avoidance; the wingman's own id is 1_000_000 + index.
        let mut bodies = Vec::with_capacity(self.enemies.len() + 3);
        bodies.push(vehicle_motion(usize::MAX, &self.player));
        bodies.extend(
            self.enemies
                .iter()
                .enumerate()
                .filter(|(i, _)| self.enemy_active.get(*i).copied().unwrap_or(false))
                .map(|(i, e)| vehicle_motion(i, e)),
        );
        bodies.extend(self.allies.iter().enumerate().map(|(i, a)| vehicle_motion(1_000_000 + i, &a.tank)));

        for index in 0..self.allies.len().min(WINGMEN) {
            if !self.allies[index].tank.alive {
                continue;
            }
            let own_id = 1_000_000 + index;
            let ally_pos = self.allies[index].tank.pos;
            let ally_center = self.allies[index].tank.center();
            let eye = ally_center + Vec3::Y * 1.2;
            let w = self.platoon.as_ref().unwrap().wingmen[index].clone();

            // --- target choice -------------------------------------------------------------
            let visible: Vec<(usize, Vec3, f32)> = self
                .enemies
                .iter()
                .enumerate()
                .filter(|(i, e)| self.enemy_active.get(*i).copied().unwrap_or(false) && e.alive)
                .map(|(i, e)| (i, e.center(), ally_center.distance(e.center())))
                .filter(|(_, c, d)| *d <= ENGAGE_RANGE_M && self.optical_los_clear(eye, *c))
                .collect();
            let preferred: Option<(usize, Vec3, f32)> = match &w.engage {
                Some(OrderTarget::Contact(label)) => {
                    let idx = label.rsplit('#').next().and_then(|n| n.trim().parse::<usize>().ok()).map(|n| n.saturating_sub(1));
                    idx.and_then(|i| visible.iter().copied().find(|(j, _, _)| *j == i))
                }
                Some(OrderTarget::Bearing(b)) => {
                    let want = heading_from_compass(*b);
                    visible
                        .iter()
                        .copied()
                        .filter(|(_, c, _)| wrap((xz(*c) - xz(ally_pos)).x.atan2((xz(*c) - xz(ally_pos)).y) - want).abs() < 0.35)
                        .min_by(|a, b| a.2.total_cmp(&b.2))
                }
                Some(OrderTarget::Sector(s)) => {
                    let offset = match s.as_str() {
                        "left" => -PI / 2.0,
                        "right" => PI / 2.0,
                        "rear" => PI,
                        _ => 0.0,
                    };
                    let want = leader_heading + offset;
                    visible
                        .iter()
                        .copied()
                        .filter(|(_, c, _)| wrap((xz(*c) - xz(player_pos)).x.atan2((xz(*c) - xz(player_pos)).y) - want).abs() < 0.8)
                        .min_by(|a, b| a.2.total_cmp(&b.2))
                }
                _ => None,
            };
            let target = preferred.or_else(|| visible.iter().copied().min_by(|a, b| a.2.total_cmp(&b.2)));

            // --- movement by standing order ---------------------------------------------------
            let here = xz(ally_pos);
            let heading = self.allies[index].tank.heading;
            let in_contact = target.is_some_and(|(_, _, d)| d < COMBAT_STOP_RANGE_M);
            // Movement orders halt only for a close contact worth a halt-and-shoot.
            let close_contact = target.is_some_and(|(_, _, d)| d < CLOSE_CONTACT_M);
            let (mut drive, mut steer) = match &w.standing {
                Standing::Follow => {
                    let goal = self.platoon_formation_point(index);
                    let dist = here.distance(goal);
                    if in_contact && dist < 60.0 {
                        (0.0, 0.0)
                    } else {
                        let speed = if dist > 60.0 { 1.0 } else { 0.7 };
                        navigate(here, heading, goal, speed, ARRIVE_M)
                    }
                }
                Standing::Hold | Standing::Cover => (0.0, 0.0),
                // Fight first, then move: the Tiger cannot fire on the move.
                Standing::Advance(goal) => if close_contact { (0.0, 0.0) } else { navigate(here, heading, *goal, 0.8, ARRIVE_M) },
                Standing::Flank { goal, .. } => {
                    if close_contact || here.distance(*goal) < ARRIVE_M { (0.0, 0.0) } else { navigate(here, heading, *goal, 0.9, ARRIVE_M) }
                }
                Standing::FallBack(goal) => {
                    // Reverse out of contact while the gun still faces the enemy; turn once clear.
                    let to = *goal - here;
                    if to.length() < ARRIVE_M {
                        (0.0, 0.0)
                    } else if in_contact {
                        let error = wrap(to.x.atan2(to.y) - (heading + PI));
                        (-0.7, (error * 1.5).clamp(-0.6, 0.6))
                    } else {
                        navigate(here, heading, *goal, 0.8, ARRIVE_M)
                    }
                }
            };
            // In contact, the trained Tiger policy fights the tank: it replaces the order
            // controller's drive/steer and adds its fire intent to the trigger gates below. Hold,
            // cover and fall back stay with the order controller; under follow the policy is on a
            // leash to the leader. Everything downstream (avoidance, wall veto, watchdog, hull
            // contact, gun lay, travel lock, weapons hold, friendly line) still applies.
            let leader_dist = here.distance(xz(player_pos));
            let policy_wanted = target.is_some()
                && match &w.standing {
                    Standing::Follow => in_contact && leader_dist < POLICY_LEASH_M,
                    Standing::Advance(_) | Standing::Flank { .. } => close_contact,
                    Standing::Hold | Standing::Cover | Standing::FallBack(_) => false,
                };
            let policy_action: Option<[f32; crate::ai::ACT]> = if policy_wanted {
                let p = self.platoon.as_ref().unwrap();
                p.policy.as_ref().map(|m| {
                    let obs = wingman_policy_obs(
                        &self.allies[index].tank,
                        w.fire_cd <= 0.0,
                        living_frac,
                        target.map(|(i, _, _)| &self.enemies[i]),
                    );
                    m.forward(&obs)
                })
            } else {
                None
            };
            // Short halt: the Tiger has no stabiliser and the trigger is travel-locked, while the
            // evolved policy keeps moving (it orbits its target). When it wants to fire and the gun
            // is loaded, the driver stops for the shot, as a crew would on the commander's "Halt!";
            // after the round (or SHORT_HALT_MAX_S) the policy moves the tank again.
            let ready_to_shoot = w.fire_cd <= 0.0 && w.ap_rounds > 0 && w.can_shoot() && !w.weapons_hold;
            let mut halting = false;
            if let Some(a) = policy_action {
                drive = a[0];
                steer = a[1];
                if a[2] > 0.0 && ready_to_shoot && w.halt_s < SHORT_HALT_MAX_S {
                    drive = 0.0;
                    steer = 0.0;
                    halting = true;
                }
            }
            {
                let p = self.platoon.as_mut().unwrap();
                let wm = &mut p.wingmen[index];
                let engaged = policy_action.is_some();
                if engaged && !wm.policy_engaged {
                    println!(
                        "[platoon] {} fighting under the trained policy (contact #{}, {:.0} m)",
                        wm.callsign,
                        target.map(|(i, _, _)| i + 1).unwrap_or(0),
                        target.map(|(_, _, d)| d).unwrap_or(0.0)
                    );
                } else if !engaged && wm.policy_engaged {
                    println!("[platoon] {} back on the order controller ({})", wm.callsign, wm.standing.label());
                }
                wm.policy_engaged = engaged;
                if engaged {
                    wm.policy_s += dt;
                }
                if halting {
                    wm.halt_s += dt;
                } else if !ready_to_shoot || !engaged {
                    // Reloading (just fired) or off the policy: the next halt starts fresh.
                    wm.halt_s = 0.0;
                }
            }
            if !w.can_drive() {
                drive = 0.0;
                steer = 0.0;
            }
            // Stuck watchdog: a tank that wants to move but does not, backs out and swings.
            {
                let p = self.platoon.as_mut().unwrap();
                let wm = &mut p.wingmen[index];
                wm.time += dt;
                let moving = self.allies[index].tank.speed.abs() > 0.4;
                if drive.abs() > 0.2 && !moving && wm.time > wm.reverse_until {
                    wm.stuck_s += dt;
                } else {
                    wm.stuck_s = (wm.stuck_s - dt * 2.0).max(0.0);
                }
                if wm.stuck_s > 2.5 {
                    wm.reverse_until = wm.time + 1.6;
                    wm.stuck_s = 0.0;
                }
                if wm.time < wm.reverse_until {
                    drive = -0.6;
                    steer = if index == 0 { 0.5 } else { -0.5 };
                }
            }
            let own = bodies.iter().copied().find(|b| b.id == own_id).unwrap_or_else(|| vehicle_motion(own_id, &self.allies[index].tank));
            let (drive, steer) = apply_vehicle_avoidance(own, &bodies, drive, steer);
            let previous = ally_pos;
            self.allies[index].tank.update(drive, steer, 0.0, 0.0, dt, &self.hf);
            // Keep the wingman out of the walls the player's course measured: fall back to the
            // previous position when the new one is not clear for a tank.
            let moved = xz(self.allies[index].tank.pos);
            if moved.distance_squared(xz(previous)) > 1e-6
                && !self.winter_patrol_point_clear(moved)
                && self.winter_patrol_point_clear(xz(previous))
            {
                self.allies[index].tank.pos.x = previous.x;
                self.allies[index].tank.pos.z = previous.z;
                self.allies[index].tank.speed = 0.0;
            }
            // Hull-to-hull contact with the leader and the enemy (same inverse-mass pair solver as
            // the enemies use against the player).
            for pass in 0..2 {
                let me = vehicle_motion(own_id, &self.allies[index].tank);
                let inv_me = vehicle_inverse_mass(&self.allies[index].tank, 57.0);
                if pass == 0 {
                    let other = vehicle_motion(usize::MAX, &self.player);
                    if let Some(res) = vehicle_pair_resolution(me, inv_me, other, vehicle_inverse_mass(&self.player, 57.0)) {
                        self.allies[index].tank.pos.x += res.a_delta.x;
                        self.allies[index].tank.pos.z += res.a_delta.y;
                        self.allies[index].tank.speed += res.a_speed_delta;
                        self.player.pos.x += res.b_delta.x;
                        self.player.pos.z += res.b_delta.y;
                        self.player.speed += res.b_speed_delta;
                    }
                } else {
                    for i in 0..self.enemies.len() {
                        if !self.enemy_active.get(i).copied().unwrap_or(false) {
                            continue;
                        }
                        let other = vehicle_motion(i, &self.enemies[i]);
                        let inv_other = vehicle_inverse_mass(&self.enemies[i], super::enemy_mass_tonnes(self.enemies[i].kind));
                        if let Some(res) = vehicle_pair_resolution(me, inv_me, other, inv_other) {
                            self.allies[index].tank.pos.x += res.a_delta.x;
                            self.allies[index].tank.pos.z += res.a_delta.y;
                            self.allies[index].tank.speed += res.a_speed_delta;
                            self.enemies[i].pos.x += res.b_delta.x;
                            self.enemies[i].pos.z += res.b_delta.y;
                            self.enemies[i].speed += res.b_speed_delta;
                        }
                    }
                }
            }

            // --- gun lay and fire --------------------------------------------------------------
            {
                let p = self.platoon.as_mut().unwrap();
                p.wingmen[index].fire_cd = (p.wingmen[index].fire_cd - dt).max(0.0);
            }
            let lay_point: Option<Vec3> = match (&w.standing, target) {
                (_, Some((_, c, _))) => Some(c),
                (Standing::Cover, None) => Some(player_pos + Vec3::new(leader_fwd.x, 0.0, leader_fwd.y) * 400.0 + Vec3::Y * 1.5),
                _ => None,
            };
            if let Some(point) = lay_point {
                let has_ammo = w.ap_rounds > 0;
                let traverse_ok = w.module(Module::Turret);
                let ally = &mut self.allies[index].tank;
                let aim_point = ballistic_aim_target(ally.muzzle().0, point, &crate::ballistics::PZGR_39);
                let laid = if traverse_ok { ally.ai_aim(aim_point, dt) } else { ally.casemate_aim(aim_point, dt) };
                if let Some((tidx, tpos, dist)) = target {
                    let policy_trigger = policy_action.map_or(true, |a| a[2] > 0.0);
                    if laid && has_ammo && policy_trigger && w.can_shoot() && !w.weapons_hold && ally.can_fire() && w.fire_cd <= 0.0 {
                        let (muzzle, direction) = ally.muzzle();
                        // Never shoot through a friend: the leader or the other wingman near the line.
                        let friendly_in_line = {
                            let line_clear = |c: Vec3| {
                                let to = c - muzzle;
                                let along = to.dot(direction);
                                along > 0.0 && along < dist && (to - direction * along).length() < 6.0
                            };
                            line_clear(self.player.center())
                                || self.allies.iter().enumerate().any(|(j, a)| j != index && a.tank.alive && line_clear(a.tank.center()))
                        };
                        if !friendly_in_line && self.optical_los_clear(muzzle, tpos) {
                            fired.push((index, Some(tidx), muzzle, direction, dist));
                            self.allies[index].tank.fired();
                        }
                    }
                }
            } else if w.standing == Standing::Follow || matches!(w.standing, Standing::Flank { .. }) {
                // Nothing to see: carry the gun forward along the hull.
                let ally = &mut self.allies[index].tank;
                let ahead = ally.center() + ally.forward() * 300.0;
                ally.ai_aim(ahead, dt);
            }
        }

        for (index, tidx, muzzle, direction, distance) in fired {
            self.ally_shot_tick = self.ally_shot_tick.wrapping_add(1);
            let seed = self.ally_shot_tick ^ 0xA117_0001 ^ (index as u32) << 20;
            let jitter = |n: u32| hash01(n) * 2.0 - 1.0;
            let speed_frac = (self.allies[index].tank.speed.abs() / tank::TOP_SPEED).clamp(0.0, 1.0);
            let dispersion = 0.0032 + 0.012 * speed_frac;
            let direction = super::gun_relative_dispersion(direction, jitter(seed) * dispersion, jitter(seed.wrapping_add(97)) * dispersion);
            let id = self.spawn_authoritative_projectile(ProjectileShooter::Ally(index), &crate::ballistics::PZGR_39, muzzle, direction, seed);
            self.present_external_projectile(id, muzzle, direction);
            self.spawn_blast(muzzle, 2.2, 18.0, 0.16);
            let reports = crate::audio::Sfx::PLAYER_GUNS_EXT;
            let report = reports[self.ally_shot_tick as usize % reports.len()];
            self.schedule_sound_from(report, muzzle);
            let p = self.platoon.as_mut().unwrap();
            let w = &mut p.wingmen[index];
            w.ap_rounds = w.ap_rounds.saturating_sub(1);
            w.shots += 1;
            w.fire_cd = RELOAD_S + hash01(seed.wrapping_add(31)) * 2.0;
            if let Some(t) = tidx {
                if t < p.enemy_kill_credit.len() {
                    p.enemy_kill_credit[t] = Some(index);
                }
            }
            println!("[platoon] {} firing on contact #{} at {:.0} m ({} AP left)", w.callsign, tidx.map(|t| t + 1).unwrap_or(0), distance, w.ap_rounds);
        }

        // Kill credit: an enemy the wingman was last to shoot at that is now dead.
        let dead: Vec<usize> = self.enemies.iter().enumerate().filter(|(_, e)| !e.alive).map(|(i, _)| i).collect();
        if let Some(p) = self.platoon.as_mut() {
            for i in dead {
                if let Some(Some(w)) = p.enemy_kill_credit.get(i).copied() {
                    p.enemy_kill_credit[i] = None;
                    if let Some(wm) = p.wingmen.get_mut(w) {
                        wm.kills += 1;
                    }
                }
            }
        }
    }

    /// A round penetrated wingman `index`'s armour. The armour decision itself came from the shared
    /// solver; this is the simplified behind-armour outcome (documented limits: no fragment
    /// geometry, seats are fixed points, outcome is a deterministic hash of the penetration).
    /// Returns true when the tank is destroyed.
    pub(crate) fn platoon_on_penetration(
        &mut self,
        index: usize,
        part: super::ArmorPart,
        facing: crate::ballistics::Facing,
        behind_armor: f32,
        seed: u32,
    ) -> bool {
        let Some(p) = self.platoon.as_mut() else { return true };
        let Some(w) = p.wingmen.get_mut(index) else { return true };
        w.penetrations += 1;
        let n = w.penetrations.wrapping_mul(0x9E37).wrapping_add(seed);
        let lethality = behind_armor.clamp(0.0, 2.0);
        // Where the round came through, in hull-local terms: the plate decides height band, the
        // facing decides the side. Seats nearer that entry are at higher risk.
        use crate::ballistics::Facing;
        let turret = matches!(part, super::ArmorPart::Turret);
        let front = matches!(facing, Facing::HullFront | Facing::TurretFront);
        let rear = matches!(facing, Facing::HullRear | Facing::TurretRear);
        // A side hit enters from whichever flank the seed picks; the plate solver does not tell
        // us left from right.
        let side_sign = if hash01(n.wrapping_add(17)) < 0.5 { -1.5 } else { 1.5 };
        let entry = match facing {
            Facing::HullFront | Facing::TurretFront => Vec3::new(0.0, if turret { 1.6 } else { 1.0 }, if turret { 1.0 } else { 2.5 }),
            Facing::HullRear | Facing::TurretRear => Vec3::new(0.0, if turret { 1.6 } else { 1.0 }, if turret { -1.2 } else { -2.5 }),
            Facing::HullSide | Facing::TurretSide => Vec3::new(side_sign, if turret { 1.6 } else { 1.0 }, if turret { 0.0 } else { 0.5 }),
        };
        let mut killed = Vec::new();
        for (k, role) in CrewRole::ALL.iter().enumerate() {
            if !w.crew_alive[k] {
                continue;
            }
            let d = role.seat().distance(entry);
            let p_kill = (lethality * 0.55 / (1.0 + d * d)).clamp(0.0, 0.95);
            if hash01(n.wrapping_add(k as u32 * 131)) < p_kill {
                w.crew_alive[k] = false;
                killed.push(role.name());
            }
        }
        let mut broke = Vec::new();
        let module_risk: [(Module, f32); 6] = [
            (Module::Gun, if turret { 0.35 } else { 0.05 }),
            (Module::Turret, if turret { 0.45 } else { 0.08 }),
            (Module::Engine, if !turret && rear { 0.75 } else if !turret { 0.15 } else { 0.02 }),
            (Module::Tracks, if !turret && !front { 0.25 } else { 0.05 }),
            (Module::Radio, if !turret && front { 0.30 } else { 0.08 }),
            (Module::Optics, if turret { 0.30 } else { 0.05 }),
        ];
        for (k, (m, risk)) in module_risk.iter().enumerate() {
            if w.modules_ok[k] && hash01(n.wrapping_add(977 + k as u32 * 53)) < risk * lethality.max(0.4) {
                w.modules_ok[k] = false;
                broke.push(m.name());
            }
        }
        // Ammunition or fuel fire: rear/side hull hits with a hot round. A burning Tiger is lost.
        let fire_risk = if turret { 0.10 } else if front { 0.12 } else { 0.30 };
        let ammo_fire = hash01(n.wrapping_add(4001)) < fire_risk * lethality;
        if ammo_fire {
            w.burning = true;
        }
        let crew_left = w.crew_alive.iter().filter(|a| **a).count();
        let destroyed = w.burning || crew_left == 0;
        println!(
            "[platoon] {} penetrated ({:?} {:?}, behind-armour {:.2}): crew killed {:?}, modules {:?}, fire {}{}",
            w.callsign, part, facing, behind_armor, killed, broke, ammo_fire, if destroyed { " — DESTROYED" } else { "" }
        );
        destroyed
    }

    /// The wingmen as the enemy AI's avoidance and the audio/visual layers should see them.
    pub(crate) fn platoon_bodies(&self) -> Vec<super::VehicleMotion> {
        if self.platoon.is_none() {
            return Vec::new();
        }
        self.allies.iter().enumerate().map(|(i, a)| vehicle_motion(1_000_000 + i, &a.tank)).collect()
    }

    // --- typed radio line ---------------------------------------------------------------------

    /// Keyboard routing for the radio line. Returns true when the key was consumed (the game must
    /// then ignore it for driving). `T` opens the line; Enter sends; Escape cancels.
    pub fn platoon_radio_key(&mut self, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
        use winit::keyboard::KeyCode;
        let Some(p) = self.platoon.as_mut() else { return false };
        if !p.typing {
            if matches!(code, KeyCode::KeyT) {
                p.typing = true;
                p.entry.clear();
                return true;
            }
            return false;
        }
        match code {
            KeyCode::Escape => {
                p.typing = false;
                p.entry.clear();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = std::mem::take(&mut p.entry);
                p.typing = false;
                self.platoon_transmit(&line);
            }
            KeyCode::Backspace => {
                p.entry.pop();
            }
            _ => {
                if let Some(t) = text {
                    for ch in t.chars() {
                        if !ch.is_control() && p.entry.chars().count() < 120 {
                            p.entry.push(ch);
                        }
                    }
                }
            }
        }
        true
    }

    /// Is the radio line open (keys go to text, not to the driver)?
    pub fn platoon_typing(&self) -> bool {
        self.platoon.as_ref().is_some_and(|p| p.typing)
    }

    /// Lines for the HUD: `(callsign, text, alpha, is_entry)`. Drawn by main.rs with the
    /// display-resolution `ab_glyph` text, outside the PS1 crush, like the menus.
    pub fn platoon_hud_lines(&self) -> Vec<(String, String, f32, bool)> {
        let Some(p) = self.platoon.as_ref() else { return Vec::new() };
        let mut out = Vec::new();
        for line in &p.log {
            let alpha = (1.0 - (line.age - RADIO_LINE_LIFE_S * 0.7) / (RADIO_LINE_LIFE_S * 0.3)).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            out.push((line.callsign.clone(), line.text.clone(), alpha, false));
        }
        if p.typing {
            let caret = if (p.time * 2.0).fract() < 0.5 { "_" } else { " " };
            out.push((crate::radio_llm::PLAYER_CALLSIGN.into(), format!("{}{}", p.entry, caret), 1.0, true));
        } else if !p.inflight.is_empty() {
            out.push(("".into(), "…".into(), 0.6, true));
        }
        out
    }

    /// Status for the proof's JSON trace.
    pub fn platoon_state_json(&self, t: f32) -> serde_json::Value {
        let Some(p) = self.platoon.as_ref() else { return serde_json::Value::Null };
        serde_json::json!({
            "time_s": t,
            "radio_path": p.radio.config().mode_label(),
            "policy_source": p.policy_source,
            "inflight": p.inflight.len(),
            "wingmen": p.wingmen.iter().enumerate().map(|(i, w)| {
                let tank = self.allies.get(i).map(|a| &a.tank);
                serde_json::json!({
                    "callsign": w.callsign,
                    "alive": tank.is_some_and(|t| t.alive),
                    "pos": tank.map(|t| t.pos.to_array()),
                    "speed": tank.map(|t| t.speed),
                    "range_to_leader_m": tank.map(|t| xz(t.pos).distance(xz(self.player.pos))),
                    "standing": w.standing.label(),
                    "goal": match &w.standing {
                        Standing::Advance(g) | Standing::FallBack(g) => Some(g.to_array()),
                        Standing::Flank { goal, .. } => Some(goal.to_array()),
                        _ => None,
                    },
                    "heading_deg": tank.map(|t| compass_deg(xz(t.forward()))),
                    "engage": w.engage.as_ref().map(|e| e.describe()),
                    "weapons_hold": w.weapons_hold,
                    "ap": w.ap_rounds, "he": w.he_rounds,
                    "shots": w.shots, "kills": w.kills,
                    "hull_hits": w.hull_hits, "penetrations": w.penetrations,
                    "crew_dead": w.crew_dead(), "damage": w.damage_list(),
                    "last_reply": w.last_reply,
                    "policy": w.policy_engaged,
                    "policy_s": w.policy_s,
                })
            }).collect::<Vec<_>>(),
        })
    }

    /// Wingman index for a dead ally the finale/wreck code may ask about.
    pub fn platoon_wingman_name(&self, index: usize) -> Option<&str> {
        self.platoon.as_ref()?.wingmen.get(index).map(|w| w.callsign.as_str())
    }

    /// The leader's kill count (private stat), for the proof receipt.
    pub fn platoon_leader_kills(&self) -> u32 {
        self.stat_kills_by_player
    }

    /// A wingman's hull centre, for the proof's exterior camera.
    pub fn platoon_wingman_center(&self, index: usize) -> Option<Vec3> {
        self.platoon.as_ref()?;
        self.allies.get(index).map(|a| a.tank.center())
    }
}

/// Translate a validated order into the wingman's standing order / engagement preference.
#[allow(clippy::too_many_arguments)]
fn apply_order(
    w: &mut Wingman,
    index: usize,
    order: &RadioOrder,
    markers: &[(String, Vec2)],
    leader_pos: Vec2,
    leader_fwd: Vec2,
    _leader_heading: f32,
    enemies: &[(usize, Vec2, bool)],
) {
    let right = Vec2::new(leader_fwd.y, -leader_fwd.x);
    match order.kind {
        OrderKind::Follow => {
            w.standing = Standing::Follow;
            w.engage = None;
        }
        OrderKind::Hold => w.standing = Standing::Hold,
        OrderKind::Advance => {
            let goal = match &order.target {
                OrderTarget::Marker(m) => markers.iter().find(|(n, _)| n == m).map(|(_, p)| *p),
                OrderTarget::Bearing(b) => {
                    let h = heading_from_compass(*b);
                    Some(leader_pos + Vec2::new(h.sin(), h.cos()) * 150.0)
                }
                OrderTarget::Sector(s) => {
                    let dir = match s.as_str() {
                        "left" => -right,
                        "right" => right,
                        "rear" => -leader_fwd,
                        _ => leader_fwd,
                    };
                    Some(leader_pos + dir * 150.0)
                }
                OrderTarget::Contact(label) => {
                    let idx = label.rsplit('#').next().and_then(|n| n.trim().parse::<usize>().ok()).map(|n| n.saturating_sub(1));
                    idx.and_then(|i| enemies.iter().find(|(j, _, _)| *j == i)).map(|(_, p, _)| *p)
                }
                OrderTarget::None => markers.first().map(|(_, p)| *p).or(Some(leader_pos + leader_fwd * 150.0)),
            };
            if let Some(goal) = goal {
                w.standing = Standing::Advance(goal);
            }
        }
        OrderKind::FlankLeft | OrderKind::FlankRight => {
            let side = if order.kind == OrderKind::FlankLeft { -1.0 } else { 1.0 };
            w.standing = Standing::Flank { side, goal: leader_pos + right * side * FLANK_SIDE_M + leader_fwd * FLANK_AHEAD_M };
        }
        OrderKind::Engage => {
            w.weapons_hold = false;
            w.engage = match &order.target {
                OrderTarget::None => None,
                t => Some(t.clone()),
            };
            // Engaging a named contact while following would stop at formation; let the gun work.
            if matches!(w.standing, Standing::Hold | Standing::Cover) {
                // stay
            }
        }
        OrderKind::CoverMe => {
            w.standing = Standing::Cover;
            w.weapons_hold = false;
        }
        OrderKind::CeaseFire => w.weapons_hold = true,
        OrderKind::FallBack => {
            let goal = leader_pos - leader_fwd * FALL_BACK_M + right * (if index == 0 { -1.0 } else { 1.0 }) * FORMATION_SIDE_M;
            w.standing = Standing::FallBack(goal);
        }
        OrderKind::ReportStatus | OrderKind::Spot => {}
    }
}

trait TruncateFront {
    fn truncate_front(&mut self, keep: usize);
}
impl<T> TruncateFront for Vec<T> {
    fn truncate_front(&mut self, keep: usize) {
        while self.len() > keep {
            self.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_change_standing_and_engagement() {
        let mut w = Wingman::new("Anton", "Hartmann", "");
        let markers = vec![("Wegpunkt 4".to_string(), Vec2::new(100.0, 200.0))];
        let ord = |kind, target| RadioOrder { addressee: Addressee::All, kind, target };
        apply_order(&mut w, 0, &ord(OrderKind::Hold, OrderTarget::None), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert_eq!(w.standing, Standing::Hold);
        apply_order(&mut w, 0, &ord(OrderKind::Advance, OrderTarget::Marker("Wegpunkt 4".into())), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert_eq!(w.standing, Standing::Advance(Vec2::new(100.0, 200.0)));
        apply_order(&mut w, 0, &ord(OrderKind::CeaseFire, OrderTarget::None), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert!(w.weapons_hold);
        apply_order(&mut w, 0, &ord(OrderKind::Engage, OrderTarget::Contact("T-34 #3".into())), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert!(!w.weapons_hold);
        assert_eq!(w.engage, Some(OrderTarget::Contact("T-34 #3".into())));
        apply_order(&mut w, 1, &ord(OrderKind::FallBack, OrderTarget::None), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert!(matches!(w.standing, Standing::FallBack(g) if g.y < 0.0 && g.x > 0.0));
        apply_order(&mut w, 1, &ord(OrderKind::FlankRight, OrderTarget::None), &markers, Vec2::ZERO, Vec2::Y, 0.0, &[]);
        assert!(matches!(w.standing, Standing::Flank { side, .. } if side > 0.0));
    }

    fn flat() -> crate::terrain::Heightfield {
        crate::terrain::Heightfield { res: 2, spacing: 8000.0, heights: vec![0.0; 4], min_h: 0.0, max_h: 0.0, openings: Default::default() }
    }

    #[test]
    fn policy_obs_matches_the_training_layout() {
        let hf = flat();
        let own = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
        let mut enemy = Tank::new(0.0, 150.0, std::f32::consts::PI, &hf, tank::T34_PARTS);
        enemy.speed = tank::TOP_SPEED * 0.5;
        let obs = wingman_policy_obs(&own, true, 0.5, Some(&enemy));
        assert!((obs[2] - 0.5).abs() < 0.02, "range over 300 m: {}", obs[2]);
        assert!(obs[3].abs() < 0.02 && obs[4] > 0.99, "dead ahead: sin {} cos {}", obs[3], obs[4]);
        assert_eq!(obs[7], 1.0);
        assert_eq!(obs[9], 0.5);
        assert!(obs[11] > 0.99, "the T-34 faces us");
        assert!((obs[12] - 0.5).abs() < 1e-3);
        assert_eq!(obs[13], 1.0);
        let none = wingman_policy_obs(&own, false, 1.0, None);
        assert_eq!(&none[0..5], &[0.0; 5]);
        assert_eq!(none[7], -1.0);
    }

    #[test]
    fn doctrine_policy_closes_turns_toward_and_wants_to_fire() {
        // The checked-in weights are an asset (gitignored); the doctrine seed has the same I/O
        // contract and documented behaviour, so it proves the wiring without the file.
        let hf = flat();
        let net = crate::ai::Mlp::tiger_doctrine_seed();
        let own = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
        let far_ahead = Tank::new(0.0, 280.0, PI, &hf, tank::T34_PARTS);
        let a = net.forward(&wingman_policy_obs(&own, true, 1.0, Some(&far_ahead)));
        assert!(a[0] > 0.2, "closes a distant target: drive {}", a[0]);
        assert!(a[2] > 0.0, "trigger intent");
        let right = Tank::new(150.0, 20.0, 0.0, &hf, tank::T34_PARTS);
        let a = net.forward(&wingman_policy_obs(&own, true, 1.0, Some(&right)));
        assert!(a[1] > 0.2, "turns toward +X (steer > 0 increases heading toward +X): {}", a[1]);
        let near = Tank::new(0.0, 60.0, PI, &hf, tank::T34_PARTS);
        let a = net.forward(&wingman_policy_obs(&own, true, 1.0, Some(&near)));
        assert!(a[0] < 0.0, "backs off inside its preferred range: drive {}", a[0]);
    }

    /// Probes the real trained weights when the asset tree is reachable (`KESSEL_ASSET_ROOT` or a
    /// checkout with `assets/`); skipped in a bare clone, where the file is not tracked.
    #[test]
    fn trained_weights_load_and_answer_the_probes_when_present() {
        let Ok(net) = crate::ai::Mlp::load(DEFAULT_POLICY) else {
            println!("skip: {DEFAULT_POLICY} not reachable");
            return;
        };
        let hf = flat();
        let own = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
        let probes = [
            ("far ahead 280 m", Tank::new(0.0, 280.0, PI, &hf, tank::T34_PARTS)),
            ("ahead 150 m", Tank::new(0.0, 150.0, PI, &hf, tank::T34_PARTS)),
            ("near 60 m", Tank::new(0.0, 60.0, PI, &hf, tank::T34_PARTS)),
            ("right 150 m", Tank::new(150.0, 20.0, -PI / 2.0, &hf, tank::T34_PARTS)),
            ("left 150 m", Tank::new(-150.0, 20.0, PI / 2.0, &hf, tank::T34_PARTS)),
            ("behind 200 m", Tank::new(0.0, -200.0, 0.0, &hf, tank::T34_PARTS)),
        ];
        for (name, enemy) in &probes {
            let a = net.forward(&wingman_policy_obs(&own, true, 1.0, Some(enemy)));
            assert!(a.iter().all(|v| v.is_finite()));
            println!("[policy-probe] {name:16} drive {:+.2} steer {:+.2} fire {:+.2}", a[0], a[1], a[2]);
        }
        let none = net.forward(&wingman_policy_obs(&own, true, 1.0, None));
        println!("[policy-probe] no target        drive {:+.2} steer {:+.2} fire {:+.2}", none[0], none[1], none[2]);
    }

    #[test]
    fn compass_and_navigation() {
        assert!((compass_deg(Vec2::Y) - 0.0).abs() < 1e-3);
        assert!((compass_deg(Vec2::X) - 90.0).abs() < 1e-3);
        let (drive, steer) = navigate(Vec2::ZERO, 0.0, Vec2::new(0.0, 100.0), 1.0, 5.0);
        assert!(drive > 0.9 && steer.abs() < 1e-3);
        let (drive, steer) = navigate(Vec2::ZERO, 0.0, Vec2::new(100.0, 0.0), 1.0, 5.0);
        assert!(drive == 0.0 && steer > 0.5, "pivot toward +X");
    }
}
