//! Adaptive encounter pacing, separate from enemy perception and physical combat.
//! The director can request reinforcements; only the built map can admit them. No target health,
//! damage, accuracy, ammunition or sight rules change. There is no total-enemy quota or exit timer.
use glam::Vec2;

pub const MAX_LIVE: usize = 12;
pub const TARGET_SECONDS: f32 = 90.0 * 60.0;
pub const ENTRY_MIN_M: f32 = 650.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Situation {
    pub progress: f32,
    pub live: usize,
    pub engaged: usize,
    pub kills: u32,
    pub shots: u32,
    pub damage: u32,
    pub critical: bool,
    pub alive: bool,
    pub finished: bool,
}

#[derive(Clone, Debug)]
pub struct Director {
    pub seed: u64,
    pub elapsed: f32,
    pub progress: f32,
    pub total_spawned: u32,
    pub arrivals: u32,
    pub next_check: f32,
    pub rest_until: f32,
    pub credit: f32,
    pub target_live: usize,
    pub state: &'static str,
    pub rejected_entries: u32,
    last_progress_s: f32,
    last_shot_s: f32,
    last: Situation,
}
impl Director {
    pub fn new(seed: u64, initial: usize) -> Self {
        Self {
            seed,
            elapsed: 0.0,
            progress: 0.0,
            total_spawned: initial as u32,
            arrivals: 0,
            next_check: 90.0,
            rest_until: 90.0,
            credit: 0.0,
            target_live: 3,
            state: "opening",
            rejected_entries: 0,
            last_progress_s: 0.0,
            last_shot_s: -1000.0,
            last: Situation {
                alive: true,
                ..Default::default()
            },
        }
    }
    /// Called in simulation time only. Pause/menu time never builds a reinforcement backlog.
    pub fn step(&mut self, dt: f32, s: Situation) -> Option<usize> {
        if !s.alive || s.finished {
            self.state = "ended";
            return None;
        }
        self.elapsed += dt.max(0.0);
        if s.progress > self.progress + 0.006 {
            self.progress = s.progress.clamp(0.0, 1.0);
            self.last_progress_s = self.elapsed;
        }
        let fresh_damage = s.damage > self.last.damage || (s.critical && !self.last.critical);
        if fresh_damage {
            self.rest_until = self.rest_until.max(self.elapsed + 120.0);
        }
        if s.kills > self.last.kills {
            self.rest_until = self.rest_until.max(self.elapsed + 65.0);
        }
        // A crew substantially ahead of the 90-minute reference meets a denser defence. This is
        // a pacing reference, not an enforced duration: a successful breakout can finish sooner.
        let expected = self.elapsed / TARGET_SECONDS;
        let ahead = (self.progress - expected).max(0.0);
        self.target_live =
            (3 + (self.progress * 6.0) as usize + (ahead * 5.0) as usize).min(MAX_LIVE);
        let stalled = self.elapsed - self.last_progress_s > 420.0;
        if stalled {
            self.target_live = self.target_live.min(4);
        }
        if s.critical {
            self.target_live = self.target_live.min(4);
        }
        let interval = (155.0 - self.progress * 70.0 - ahead * 45.0).clamp(65.0, 155.0);
        self.credit = (self.credit + dt.max(0.0) / interval).min(4.0);
        if s.shots > self.last.shots {
            self.last_shot_s = self.elapsed;
        }
        self.last = s;
        if self.elapsed < self.rest_until {
            self.state = "recovery";
            return None;
        }
        if self.elapsed < self.next_check {
            return None;
        }
        self.next_check = self.elapsed + 12.0;
        if s.engaged >= (2 + (self.progress * 3.0) as usize) {
            self.state = "combat_pressure";
            return None;
        }
        if s.live >= self.target_live || s.live >= MAX_LIVE {
            self.state = "existing_force";
            return None;
        }
        if self.elapsed - self.last_shot_s < 35.0 || self.credit < 1.0 {
            self.state = "gathering";
            return None;
        }
        let desired = (self.target_live - s.live)
            .min(3)
            .min(self.credit.floor() as usize);
        if desired == 0 {
            self.state = "gathering";
            return None;
        }
        self.state = "seeking_hidden_entry";
        Some(desired)
    }
    pub fn admitted(&mut self, count: usize) {
        self.total_spawned += count as u32;
        self.arrivals += 1;
        self.credit = (self.credit - count as f32).max(0.0);
        let jitter = (crate::canyon::mix(self.seed ^ self.arrivals as u64) % 71) as f32;
        self.next_check = self.elapsed + 100.0 + jitter;
        self.state = "reinforcements_en_route";
    }
    pub fn deferred(&mut self) {
        self.rejected_entries += 1;
        self.state = "no_safe_entry";
    }
}

/// Arc-distance projection onto the real winding route, not straight-line distance to the exit.
pub fn route_progress(route: &[Vec2], point: Vec2) -> f32 {
    let mut along = 0.0;
    let mut best = (f32::INFINITY, 0.0);
    for edge in route.windows(2) {
        let v = edge[1] - edge[0];
        let len = v.length();
        let t = ((point - edge[0]).dot(v) / v.length_squared().max(0.001)).clamp(0.0, 1.0);
        let d = point.distance_squared(edge[0] + v * t);
        if d < best.0 {
            best = (d, along + t * len);
        }
        along += len;
    }
    if along > 0.0 {
        (best.1 / along).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn simulate(seed: u64, fast: bool) -> (u32, Vec<(u32, usize)>) {
        let mut d = Director::new(seed, 5);
        let mut live = 5;
        let mut kills = 0;
        let mut log = Vec::new();
        for sec in 0..7200 {
            if sec % 110 == 100 && live > 0 {
                live -= 1;
                kills += 1;
            }
            let progress = (sec as f32 / if fast { 5000.0 } else { 10000.0 }).min(0.98);
            let s = Situation {
                progress,
                live,
                kills,
                shots: kills,
                alive: true,
                ..Default::default()
            };
            if let Some(n) = d.step(1.0, s) {
                live += n;
                d.admitted(n);
                log.push((sec, n));
            }
            assert!(live <= MAX_LIVE);
        }
        (d.total_spawned, log)
    }
    #[test]
    fn long_run_is_adaptive_reproducible_and_not_a_five_tank_quota() {
        let slow = simulate(29, false);
        let fast = simulate(29, true);
        assert!(slow.0 > 25 && fast.0 > slow.0, "{slow:?} {fast:?}");
        assert_eq!(fast, simulate(29, true));
        assert_ne!(fast.1, simulate(73, true).1);
        println!("two-hour director model: slow={} fast={}", slow.0, fast.0);
    }
    #[test]
    fn pressure_damage_death_finish_and_failed_siting_do_not_force_spawns() {
        let mut d = Director::new(29, 5);
        let mut s = Situation {
            progress: 0.65,
            alive: true,
            ..Default::default()
        };
        d.step(1000.0, s);
        d.deferred();
        let total = d.total_spawned;
        let credit = d.credit;
        assert_eq!(total, 5);
        assert_eq!(d.credit, credit);
        s.damage = 1;
        assert!(d.step(1.0, s).is_none());
        assert_eq!(d.state, "recovery");
        s.engaged = 8;
        assert!(d.step(130.0, s).is_none());
        assert_eq!(d.state, "combat_pressure");
        s.alive = false;
        let elapsed = d.elapsed;
        assert!(d.step(500.0, s).is_none());
        assert_eq!(d.elapsed, elapsed);
        s.alive = true;
        s.finished = true;
        assert!(d.step(500.0, s).is_none());
        assert_eq!(d.elapsed, elapsed);
    }
    #[test]
    fn winding_route_measures_travel_not_proximity_to_finish() {
        let r = [
            Vec2::ZERO,
            Vec2::new(100., 0.),
            Vec2::new(100., 100.),
            Vec2::new(0., 100.),
        ];
        assert!((route_progress(&r, Vec2::new(100., 50.)) - 0.5).abs() < 1e-5);
        assert!(route_progress(&r, Vec2::ZERO) < 0.01);
    }
}
