//! Snow lying on exposed surfaces (raymarch backend, 2026-10-28).
//!
//! Two loads, each 0 (bare) to 1 (every face that can hold snow is covered): the player's Tiger,
//! which starts clean, fills under snowfall and sheds snow when it drives, traverses or fires;
//! and the world's (wrecks, buildings, rubble, props), which starts at the winter's lying snow
//! and only grows. The renderer turns a load into coverage per pixel from the surface's slope,
//! whether it is open to the falling snow (the sky-cover map) and a breakup noise
//! (`rm_shade.wgsl` `snow_coverage`).
//!
//! Named approximations (docs/PHYSICAL_TRUTH_AUDIT.md): one scalar load per vehicle, not a
//! per-face accumulation map; rates are presentation estimates, not snow-physics measurements:
//! full cover after two minutes of steady snowfall, loose snow blown off the faces above about
//! 2 m/s of air stream (quadratic in speed), shaken off by traverse, half of it thrown off by the
//! muzzle blast and recoil of a shot; no melt on hot parts (engine deck, exhausts, the barrel).

/// Seconds of steady snowfall (renderer precipitation 1) that bring a bare surface to full cover.
pub const FULL_COVER_SNOWFALL_S: f32 = 120.0;
/// The world's load at the start of a winter act: lying snow from the days before.
pub const WINTER_WORLD_LOAD: f32 = 0.7;
/// Air-stream shedding: load lost per second per unit load at 12 m/s (about 43 km/h).
const SHED_AT_12_M_S: f32 = 0.5;
/// Air speed below which loose snow stays on the faces.
const SHED_THRESHOLD_M_S: f32 = 2.0;
/// Load lost per second per unit load per rad/s of turret traverse.
const TRAVERSE_SHAKE: f32 = 0.4;
/// Share of the load a shot leaves on the vehicle (muzzle blast over the glacis, recoil).
const SHOT_KEEPS: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnowInputs {
    pub dt: f32,
    /// Renderer precipitation (0 none, 1 steady snowfall, storms above).
    pub snowfall: f32,
    pub vehicle_speed_m_s: f32,
    pub traverse_rad_s: f32,
    pub fired: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnowCover {
    pub player: f32,
    pub world: f32,
    prev_turret_yaw: Option<f32>,
    prev_rounds: Option<u32>,
}

impl SnowCover {
    pub fn new(winter: bool) -> Self {
        Self { player: 0.0, world: if winter { WINTER_WORLD_LOAD } else { 0.0 }, prev_turret_yaw: None, prev_rounds: None }
    }

    /// One load's update: accumulation toward full cover, shedding in proportion to the load.
    pub fn step_load(load: f32, i: &SnowInputs) -> f32 {
        let dt = if i.dt.is_finite() { i.dt.clamp(0.0, 0.25) } else { 0.0 };
        let snowfall = if i.snowfall.is_finite() { i.snowfall.max(0.0) } else { 0.0 };
        let gain = snowfall / FULL_COVER_SNOWFALL_S * dt * (1.0 - load);
        let air = ((i.vehicle_speed_m_s.abs() - SHED_THRESHOLD_M_S).max(0.0) / (12.0 - SHED_THRESHOLD_M_S)).powi(2) * SHED_AT_12_M_S;
        let shake = i.traverse_rad_s.abs().min(2.0) * TRAVERSE_SHAKE;
        let loss = (air + shake) * dt * load;
        let mut next = (load + gain - loss).clamp(0.0, 1.0);
        if i.fired { next *= SHOT_KEEPS; }
        next
    }

    /// Advance both loads from the frame's state. `turret_yaw` and `rounds_spent` are compared
    /// with the previous call to find traverse rate and shots.
    pub fn update(&mut self, dt: f32, snowfall: f32, vehicle_speed_m_s: f32, turret_yaw: f32, rounds_spent: u32) {
        let traverse = match self.prev_turret_yaw {
            Some(prev) if dt > 1.0e-4 => {
                let mut d = turret_yaw - prev;
                while d > std::f32::consts::PI { d -= std::f32::consts::TAU; }
                while d < -std::f32::consts::PI { d += std::f32::consts::TAU; }
                d / dt
            }
            _ => 0.0,
        };
        let fired = self.prev_rounds.is_some_and(|prev| rounds_spent != prev);
        self.prev_turret_yaw = Some(turret_yaw);
        self.prev_rounds = Some(rounds_spent);
        self.player = Self::step_load(self.player, &SnowInputs { dt, snowfall, vehicle_speed_m_s, traverse_rad_s: traverse, fired });
        self.world = Self::step_load(self.world, &SnowInputs { dt, snowfall, vehicle_speed_m_s: 0.0, traverse_rad_s: 0.0, fired: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(load: f32, seconds: f32, i: SnowInputs) -> f32 {
        let mut l = load;
        for _ in 0..(seconds * 60.0) as usize { l = SnowCover::step_load(l, &SnowInputs { dt: 1.0 / 60.0, ..i }); }
        l
    }

    #[test]
    fn steady_snowfall_covers_a_standing_tank_in_about_two_minutes() {
        let i = SnowInputs { dt: 0.0, snowfall: 1.0, vehicle_speed_m_s: 0.0, traverse_rad_s: 0.0, fired: false };
        let after = run(0.0, 120.0, i);
        assert!((0.55..0.75).contains(&after), "1 - 1/e of full cover after the nominal time: {after}");
        assert!(run(0.0, 600.0, i) > 0.98);
        assert_eq!(run(0.4, 60.0, SnowInputs { snowfall: 0.0, ..i }), 0.4, "no snowfall, no change");
    }

    #[test]
    fn driving_traverse_and_firing_shed_snow() {
        let still = SnowInputs { dt: 0.0, snowfall: 0.0, vehicle_speed_m_s: 0.0, traverse_rad_s: 0.0, fired: false };
        assert!(run(1.0, 10.0, SnowInputs { vehicle_speed_m_s: 12.0, ..still }) < 0.05, "a fast drive clears loose snow");
        assert!(run(1.0, 10.0, SnowInputs { vehicle_speed_m_s: 1.5, ..still }) > 0.99, "a crawl keeps it");
        assert!(run(1.0, 4.0, SnowInputs { traverse_rad_s: 0.35, ..still }) < 0.6);
        let shot = SnowCover::step_load(1.0, &SnowInputs { dt: 1.0 / 60.0, fired: true, ..still });
        assert!((shot - SHOT_KEEPS).abs() < 1.0e-3);
    }

    #[test]
    fn update_detects_traverse_and_shots_and_keeps_the_world_load() {
        let mut s = SnowCover::new(true);
        s.player = 1.0;
        s.update(1.0 / 60.0, 0.0, 0.0, 0.0, 3);
        assert_eq!(s.player, 1.0, "the first call only records the state");
        s.update(1.0 / 60.0, 0.0, 0.0, 0.0, 4);
        assert!((s.player - SHOT_KEEPS).abs() < 1.0e-3);
        s.update(1.0 / 60.0, 0.0, 0.0, 6.2, 4);
        assert!(s.player < SHOT_KEEPS, "a turn across the wrap is a small traverse, still a shake");
        assert_eq!(s.world, WINTER_WORLD_LOAD);
        assert_eq!(SnowCover::new(false).world, 0.0);
    }
}
