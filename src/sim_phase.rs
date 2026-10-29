//! Wall time spent in the heavier simulation phases of `Game::update`, accumulated between two
//! session-log lines (`KESSEL_SESSION_LOG`) and reported there as `game.sim_phases_ms`. Timing
//! only: no phase reads it. The phases are the calls a contact-heavy second is suspected to
//! spend its time in; `cpu_sim_ms` minus their sum is everything else in the update.
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    /// Inclusive time of probed functions (`Probe`): (label, total ms, worst call ms, calls).
    static CALLS: RefCell<Vec<(&'static str, f64, f64, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Times the function it is created in, until it drops: `let _probe = Probe::new("name");`.
/// Inclusive (a probed callee's time is also its caller's); reported as `calls` beside the
/// segments, for paths that branch too much for checkpoints (the shell-impact chain).
pub struct Probe(&'static str, Instant);

impl Probe {
    pub fn new(label: &'static str) -> Self {
        Self(label, Instant::now())
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let ms = self.1.elapsed().as_secs_f64() * 1000.0;
        CALLS.with(|calls| {
            let mut calls = calls.borrow_mut();
            match calls.iter_mut().find(|(l, ..)| *l == self.0) {
                Some(e) => { e.1 += ms; e.2 = e.2.max(ms); e.3 += 1; }
                None => calls.push((self.0, ms, ms, 1)),
            }
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Infantry,
    EnemyAi,
    VehicleContacts,
    Projectiles,
    Fx,
    WrecksFireHaze,
    CanyonMission,
    /// Inside `enemy_ai`: the stealth/detection pass, the winter encounter's orders (street
    /// routing), the anti-tank guns. The rest of `enemy_ai` is the per-vehicle loop.
    EnemyStealth,
    EnemyOrders,
    EnemyAtGuns,
}

const NAMES: [&str; 10] = [
    "infantry", "enemy_ai", "vehicle_contacts", "projectiles", "fx", "wrecks_fire_haze", "canyon_mission",
    "enemy_ai.stealth", "enemy_ai.orders", "enemy_ai.at_guns",
];

#[derive(Default)]
pub struct SimPhases {
    ms: [f64; 10],
    updates: u32,
    /// Checkpoint segments over the whole update (`seg_begin` / `seg`): each mark charges the
    /// time since the previous mark to its label, so the segments add up to the update and no
    /// untimed remainder hides between the phases. (label, total ms, worst single update ms)
    seg: Vec<(&'static str, f64, f64)>,
    seg_t: Option<Instant>,
}

impl SimPhases {
    pub fn add(&mut self, phase: Phase, since: Instant) {
        self.ms[phase as usize] += since.elapsed().as_secs_f64() * 1000.0;
    }

    pub fn count_update(&mut self) {
        self.updates += 1;
    }

    /// Start the checkpoint clock for this update.
    pub fn seg_begin(&mut self) {
        self.seg_t = Some(Instant::now());
    }

    /// Charge the time since the previous mark to `label`.
    pub fn seg(&mut self, label: &'static str) {
        let now = Instant::now();
        if let Some(t) = self.seg_t {
            let ms = (now - t).as_secs_f64() * 1000.0;
            match self.seg.iter_mut().find(|(l, ..)| *l == label) {
                Some(e) => { e.1 += ms; e.2 = e.2.max(ms); }
                None => self.seg.push((label, ms, ms)),
            }
        }
        self.seg_t = Some(now);
    }

    /// Mean milliseconds per `Game::update` call for each phase since the last take, then reset.
    pub fn take(&mut self) -> serde_json::Value {
        let n = self.updates.max(1) as f64;
        let mut out = serde_json::Map::new();
        for (name, ms) in NAMES.iter().zip(self.ms) {
            out.insert((*name).into(), serde_json::json!(ms / n));
        }
        out.insert("updates".into(), serde_json::json!(self.updates));
        let seg: serde_json::Map<_, _> = self.seg.iter().map(|(l, ms, _)| ((*l).to_string(), serde_json::json!(ms / n))).collect();
        let seg_max: serde_json::Map<_, _> = self.seg.iter().map(|(l, _, mx)| ((*l).to_string(), serde_json::json!(mx))).collect();
        out.insert("seg".into(), serde_json::Value::Object(seg));
        out.insert("seg_max".into(), serde_json::Value::Object(seg_max));
        let calls = CALLS.with(|calls| std::mem::take(&mut *calls.borrow_mut()));
        let per: serde_json::Map<_, _> = calls.iter().map(|(l, ms, ..)| ((*l).to_string(), serde_json::json!(ms / n))).collect();
        let worst: serde_json::Map<_, _> = calls.iter().map(|(l, _, mx, _)| ((*l).to_string(), serde_json::json!(mx))).collect();
        let count: serde_json::Map<_, _> = calls.iter().map(|(l, .., c)| ((*l).to_string(), serde_json::json!(c))).collect();
        out.insert("calls".into(), serde_json::Value::Object(per));
        out.insert("calls_max".into(), serde_json::Value::Object(worst));
        out.insert("calls_n".into(), serde_json::Value::Object(count));
        *self = Self::default();
        serde_json::Value::Object(out)
    }
}
