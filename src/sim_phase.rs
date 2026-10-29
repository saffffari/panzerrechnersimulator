//! Wall time spent in the heavier simulation phases of `Game::update`, accumulated between two
//! session-log lines (`KESSEL_SESSION_LOG`) and reported there as `game.sim_phases_ms`. Timing
//! only: no phase reads it. The phases are the calls a contact-heavy second is suspected to
//! spend its time in; `cpu_sim_ms` minus their sum is everything else in the update.
use std::time::Instant;

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
        *self = Self::default();
        serde_json::Value::Object(out)
    }
}
