//! Read-only measurement of the enemy tank crews' fighting (docs/ENEMY_TANK_AI.md): how long each
//! tank sat idle or stuck while it knew of the Tiger, where it fired from relative to the Tiger's
//! armour, and what each side's rounds struck. Nothing here feeds back into a decision; the
//! headless Canyon and arena receipts read the summary.
use glam::{Vec2, Vec3};

use crate::ballistics::Facing;

/// Below this hull speed a tank counts as standing still.
const IDLE_SPEED_M_S: f32 = 0.5;
/// A tank ordered to drive at least this much throttle while slower than `STUCK_SPEED_M_S` is
/// pinned (a wall, a wreck, a slope) rather than halted by choice.
const STUCK_DRIVE: f32 = 0.15;
const STUCK_SPEED_M_S: f32 = 0.3;

/// The Tiger's hull aspect a shot or a hit came from. Bearings within the hull's corner angle of
/// dead ahead (or astern) are front (rear); the rest are side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Aspect { Front, Side, Rear }

impl Aspect {
    fn index(self) -> usize { match self { Self::Front => 0, Self::Side => 1, Self::Rear => 2 } }
    pub(super) fn of_facing(facing: Facing) -> Self {
        match facing {
            Facing::HullFront | Facing::TurretFront => Self::Front,
            Facing::HullSide | Facing::TurretSide => Self::Side,
            Facing::HullRear | Facing::TurretRear => Self::Rear,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct TankStats {
    pub active_s: f32,
    /// The crew knew of the Tiger: visual contact, a search of a fix or an investigated report.
    pub contact_s: f32,
    /// The crew had the Tiger in sight.
    pub track_s: f32,
    pub idle_contact_s: f32,
    pub stuck_s: f32,
    pub travelled_m: f32,
    /// Main-gun rounds fired, by the Tiger hull aspect presented to the firer.
    pub shots: [u32; 3],
    /// ... of which the crew's own armour solution gave a penetration of the presented plate.
    pub shots_with_solution: u32,
    pub hits_on_tiger: [u32; 3],
    pub pens_on_tiger: u32,
    /// Tiger rounds that struck this tank while it fought, by its own facing.
    pub hits_taken: [u32; 3],
    pub first_contact_s: Option<f32>,
    pub knocked_out_s: Option<f32>,
    last_pos: Option<Vec3>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct EnemyAiTelemetry {
    pub clock_s: f32,
    pub tanks: Vec<TankStats>,
    pub first_contact_s: Option<f32>,
    pub first_enemy_shot_s: Option<f32>,
    pub first_hit_on_tiger_s: Option<f32>,
}

impl EnemyAiTelemetry {
    fn tank(&mut self, i: usize) -> &mut TankStats {
        if self.tanks.len() <= i { self.tanks.resize(i + 1, TankStats::default()); }
        &mut self.tanks[i]
    }

    pub fn tick(&mut self, dt: f32) { self.clock_s += dt.max(0.0); }

    /// One simulated frame of a living, fielded tank after its motion was resolved.
    pub fn frame(&mut self, i: usize, dt: f32, contact: bool, track: bool, speed: f32, drive: f32, pos: Vec3) {
        let clock = self.clock_s;
        if contact && self.first_contact_s.is_none() && track { self.first_contact_s = Some(clock); }
        let t = self.tank(i);
        t.active_s += dt;
        if let Some(last) = t.last_pos {
            t.travelled_m += Vec2::new(pos.x - last.x, pos.z - last.z).length();
        }
        t.last_pos = Some(pos);
        if contact {
            t.contact_s += dt;
            if t.first_contact_s.is_none() { t.first_contact_s = Some(clock); }
            if speed.abs() < IDLE_SPEED_M_S { t.idle_contact_s += dt; }
        }
        if track { t.track_s += dt; }
        if drive.abs() >= STUCK_DRIVE && speed.abs() < STUCK_SPEED_M_S { t.stuck_s += dt; }
    }

    pub fn shot(&mut self, i: usize, aspect: Aspect, solution: bool) {
        let clock = self.clock_s;
        self.first_enemy_shot_s.get_or_insert(clock);
        let t = self.tank(i);
        t.shots[aspect.index()] += 1;
        if solution { t.shots_with_solution += 1; }
    }

    pub fn hit_on_tiger(&mut self, i: usize, aspect: Aspect, penetrated: bool) {
        let clock = self.clock_s;
        self.first_hit_on_tiger_s.get_or_insert(clock);
        let t = self.tank(i);
        t.hits_on_tiger[aspect.index()] += 1;
        if penetrated { t.pens_on_tiger += 1; }
    }

    pub fn hit_taken(&mut self, i: usize, aspect: Aspect) { self.tank(i).hits_taken[aspect.index()] += 1; }

    pub fn knocked_out(&mut self, i: usize) {
        let clock = self.clock_s;
        self.tank(i).knocked_out_s.get_or_insert(clock);
    }

    pub fn summary(&self) -> serde_json::Value {
        let sum3 = |f: &dyn Fn(&TankStats) -> [u32; 3]| self.tanks.iter().fold([0u32; 3], |a, t| {
            let b = f(t); [a[0] + b[0], a[1] + b[1], a[2] + b[2]] });
        let sum = |f: &dyn Fn(&TankStats) -> f32| self.tanks.iter().map(f).sum::<f32>();
        let shots = sum3(&|t| t.shots);
        let hits = sum3(&|t| t.hits_on_tiger);
        let taken = sum3(&|t| t.hits_taken);
        let contact = sum(&|t| t.contact_s);
        let idle = sum(&|t| t.idle_contact_s);
        let fired = shots.iter().sum::<u32>();
        let flank_fraction = if fired > 0 { (shots[1] + shots[2]) as f32 / fired as f32 } else { 0.0 };
        let fielded: Vec<_> = self.tanks.iter().enumerate().filter(|(_, t)| t.active_s > 0.0).collect();
        serde_json::json!({
            "clock_s": self.clock_s,
            "first_contact_s": self.first_contact_s,
            "first_enemy_shot_s": self.first_enemy_shot_s,
            "first_hit_on_tiger_s": self.first_hit_on_tiger_s,
            "tanks_fielded": fielded.len(),
            "tanks_in_contact": fielded.iter().filter(|(_, t)| t.contact_s > 0.0).count(),
            "tanks_knocked_out": fielded.iter().filter(|(_, t)| t.knocked_out_s.is_some()).count(),
            "contact_tank_s": contact,
            "idle_in_contact_tank_s": idle,
            "idle_in_contact_fraction": if contact > 0.0 { idle / contact } else { 0.0 },
            "stuck_tank_s": sum(&|t| t.stuck_s),
            "travelled_m": sum(&|t| t.travelled_m),
            "shots_front_side_rear": shots,
            "shots_flank_or_rear_fraction": flank_fraction,
            "shots_with_kill_solution": self.tanks.iter().map(|t| t.shots_with_solution).sum::<u32>(),
            "hits_on_tiger_front_side_rear": hits,
            "pens_on_tiger": self.tanks.iter().map(|t| t.pens_on_tiger).sum::<u32>(),
            "tiger_hits_taken_front_side_rear": taken,
            "tanks": fielded.iter().map(|(i, t)| serde_json::json!({
                "slot": i, "active_s": t.active_s, "contact_s": t.contact_s, "track_s": t.track_s,
                "idle_in_contact_s": t.idle_contact_s, "stuck_s": t.stuck_s, "travelled_m": t.travelled_m,
                "shots": t.shots, "with_solution": t.shots_with_solution, "hits": t.hits_on_tiger,
                "pens": t.pens_on_tiger, "hits_taken": t.hits_taken,
                "first_contact_s": t.first_contact_s, "knocked_out_s": t.knocked_out_s,
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_and_stuck_time_are_counted_only_in_their_conditions() {
        let mut m = EnemyAiTelemetry::default();
        for k in 0..100 {
            m.tick(0.1);
            // 0..50: patrolling at speed; 50..100: in contact, ordered forward but not moving.
            let contact = k >= 50;
            let (speed, drive) = if contact { (0.0, 0.6) } else { (3.0, 0.25) };
            m.frame(0, 0.1, contact, contact, speed, drive, Vec3::new(k as f32 * 0.3, 0.0, 0.0));
        }
        let t = &m.tanks[0];
        assert!((t.contact_s - 5.0).abs() < 1e-3 && (t.idle_contact_s - 5.0).abs() < 1e-3);
        assert!((t.stuck_s - 5.0).abs() < 1e-3);
        assert!((t.travelled_m - 29.7).abs() < 1e-2);
        assert!(m.first_contact_s.is_some_and(|s| (s - 5.1).abs() < 1e-3));
        m.shot(0, Aspect::Side, true);
        m.shot(0, Aspect::Front, false);
        let s = m.summary();
        assert_eq!(s["shots_front_side_rear"], serde_json::json!([1, 1, 0]));
        assert_eq!(s["shots_flank_or_rear_fraction"], serde_json::json!(0.5));
    }
}
