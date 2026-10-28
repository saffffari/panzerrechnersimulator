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
}

impl SimPhases {
    pub fn add(&mut self, phase: Phase, since: Instant) {
        self.ms[phase as usize] += since.elapsed().as_secs_f64() * 1000.0;
    }

    pub fn count_update(&mut self) {
        self.updates += 1;
    }

    /// Mean milliseconds per `Game::update` call for each phase since the last take, then reset.
    pub fn take(&mut self) -> serde_json::Value {
        let n = self.updates.max(1) as f64;
        let mut out = serde_json::Map::new();
        for (name, ms) in NAMES.iter().zip(self.ms) {
            out.insert((*name).into(), serde_json::json!(ms / n));
        }
        out.insert("updates".into(), serde_json::json!(self.updates));
        *self = Self::default();
        serde_json::Value::Object(out)
    }
}
