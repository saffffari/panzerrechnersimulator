//! The Tiger's exhaust (2026-10-28): what leaves its two stacks on the rear plate, as puffs the
//! raymarch medium draws (`renderer_raymarch.rs` exhaust parcels, `rm_volume.wgsl` kind 7).
//!
//! The Maybach's exhaust flow follows its speed and load; a warm-up model gives the engine a
//! temperature (none existed). In winter the exhaust's water vapour condenses in the cold air into
//! a dense white plume, densest from a cold engine at idle (unburnt fuel joins the vapour) and
//! thinner once warm, never gone below freezing; elsewhere it is a bluish-grey haze that only a
//! cold engine whitens. A hard opening of the throttle sends black puffs (rich mixture). Each puff
//! leaves its stack with the exhaust's exit velocity on top of the tank's own, rises while hot,
//! slows toward the wind and grows by entrainment, so the plume trails behind a moving tank, curls
//! downwind and pools round a standing one.
//!
//! Named approximations (docs/PHYSICAL_TRUTH_AUDIT.md): puffs, not a fluid solver; flow, exit
//! speed, densities, lifetimes and the warm-up time are presentation estimates sized from the
//! engine's displacement and the stacks' bore, not measurements; no ground interaction (puffs pass
//! through the snow surface); the plume shadows nothing in the sun (the medium's beam march sees
//! only the continuous medium).

use glam::{Mat4, Vec3};

/// Hull-local outlets of the two stacks (`Hull_1207`, `Hull_1208` in the baked hull: centres at
/// x +-0.408, the pipes' tops 2.09 m up, z -3.18) and the exhaust's direction out of them: up and
/// a little to the rear.
const STACKS: [Vec3; 2] = [Vec3::new(0.408, 2.09, -3.18), Vec3::new(-0.408, 2.09, -3.18)];
const EXIT_DIRECTION: Vec3 = Vec3::new(0.0, 1.0, -0.35);
/// Puffs per second from each stack; a puff's density carries the flow.
const PUFFS_PER_STACK_S: f32 = 10.0;
pub const MAX_PUFFS: usize = 320;
/// Seconds of idling that take the engine from cold to running temperature (1 - 1/e); load
/// shortens it, and a stopped engine cools over half an hour.
const WARM_UP_S: f32 = 420.0;
const COOL_DOWN_S: f32 = 1800.0;
/// A puff's first radius: the pipes' 6 cm bore radius widened by the jet's first expansion.
const START_RADIUS_M: f32 = 0.15;

#[derive(Clone, Copy, Debug)]
struct Puff {
    pos: Vec3,
    vel: Vec3,
    radius: f32,
    /// Extinction at its centre on leaving the stack, per metre.
    density0: f32,
    age: f32,
    life: f32,
    /// Excess temperature, 1 at the outlet, falling as it mixes.
    heat: f32,
    /// 0 condensate white, 1 soot black; `haze` the bluish-grey of a warm engine's dry exhaust.
    soot: f32,
    haze: f32,
    seed: f32,
}

/// One puff as the renderer draws it.
#[derive(Clone, Copy, Debug)]
pub struct RenderPuff {
    pub pos: Vec3,
    pub radius: f32,
    /// Scattering colour (albedo times 1/0.75, the medium's smoke convention).
    pub colour: [f32; 3],
    /// Extinction at its centre, per metre.
    pub density: f32,
    pub heat: f32,
    pub seed: f32,
}

#[derive(Clone, Debug)]
pub struct ExhaustPlume {
    puffs: Vec<Puff>,
    emit: f32,
    /// 0 cold, 1 at running temperature.
    pub engine_temperature: f32,
    throttle_prev: f32,
    soot_s: f32,
    prev_hull: Option<Vec3>,
    counter: u32,
}

impl Default for ExhaustPlume {
    fn default() -> Self {
        Self { puffs: Vec::with_capacity(MAX_PUFFS), emit: 0.0, engine_temperature: 0.0, throttle_prev: 0.0,
            soot_s: 0.0, prev_hull: None, counter: 0 }
    }
}

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15; x = x.wrapping_mul(0x2C1B_3C6D); x ^= x >> 12;
    (x & 0x00FF_FFFF) as f32 / 16_777_216.0
}

impl ExhaustPlume {
    /// Advance by `dt`. `running`: the engine turns and burns; `rpm`, `load` and `throttle` from the
    /// powertrain; `hull` the hull's pose; `wind` the world wind (m/s); `winter` the act's season.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f32, running: bool, rpm: f32, load: f32, throttle: f32, hull: Mat4, wind: Vec3, winter: bool) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        if dt <= 0.0 { return; }
        let load = if load.is_finite() { load.clamp(0.0, 1.0) } else { 0.0 };
        let throttle = if throttle.is_finite() { throttle.clamp(0.0, 1.0) } else { 0.0 };
        // Warm-up: faster under load; a stopped engine cools.
        if running {
            let tau = WARM_UP_S / (1.0 + 3.0 * load);
            self.engine_temperature += (1.0 - self.engine_temperature) * (dt / tau).min(1.0);
        } else {
            self.engine_temperature -= self.engine_temperature * (dt / COOL_DOWN_S).min(1.0);
        }
        // A hard opening of the throttle runs rich for a moment: black puffs.
        if running && (throttle - self.throttle_prev) / dt > 2.5 && throttle > 0.6 { self.soot_s = 0.7; }
        self.throttle_prev = throttle;
        self.soot_s = (self.soot_s - dt).max(0.0);

        let origin = hull.transform_point3(Vec3::ZERO);
        let hull_velocity = self.prev_hull.map_or(Vec3::ZERO, |p| (origin - p) / dt);
        self.prev_hull = Some(origin);
        let hull_velocity = if hull_velocity.length() > 40.0 { Vec3::ZERO } else { hull_velocity };

        // Move, cool, grow and age the puffs.
        let drag = 1.0 - (-dt / 0.35).exp();
        for p in &mut self.puffs {
            p.age += dt;
            p.heat *= (-dt / 1.5).exp();
            let relative = p.vel - wind;
            // Entrainment while the jet still runs against the air, turbulent spread after.
            p.radius += (0.12 * relative.length() + 0.16 / (p.age + 0.2).sqrt()) * dt;
            p.vel += (wind - p.vel) * drag;
            // Buoyancy while hot: a rise velocity toward which the vertical motion relaxes.
            p.vel.y += (1.4 * p.heat - (p.vel.y - wind.y)) * drag;
            p.pos += p.vel * dt;
        }
        self.puffs.retain(|p| p.age < p.life && p.pos.is_finite());

        if !running { return; }
        // Emission. Relative flow: the engine's volume flow through its speed and load (a
        // four-stroke's swept volume per second at the crank speed, hotter and larger under load).
        let flow = (rpm / crate::powertrain::RATED_RPM).clamp(0.2, 1.2) * (0.4 + 0.6 * load);
        let cold = 1.0 - self.engine_temperature;
        // Visible condensate: in winter always, densest cold at idle; elsewhere only a cold engine.
        let white = if winter { 0.55 + 0.45 * cold } else { 0.3 * cold } * (0.7 + 0.3 * (1.0 - load));
        let haze = (1.0 - white).clamp(0.0, 1.0);
        let soot = if self.soot_s > 0.0 { 1.0 } else { 0.15 * load * load };
        // Exit speed: about 18 m/s at idle through the two bores, 95 m/s at rated speed under load.
        let exit_speed = 18.0 + 77.0 * ((flow - 0.2) / 1.0).clamp(0.0, 1.0);
        // Extinction at the outlet: dense white makes the jet opaque within a hand's breadth.
        let density = (6.0 + 18.0 * white) * (0.5 + 0.5 * flow) + 10.0 * soot;
        let life = if winter { 6.0 + 8.0 * white } else { 3.0 + 4.0 * white };
        self.emit += dt * PUFFS_PER_STACK_S;
        let direction = hull.transform_vector3(EXIT_DIRECTION).normalize_or_zero();
        while self.emit >= 1.0 {
            self.emit -= 1.0;
            for stack in STACKS {
                if self.puffs.len() >= MAX_PUFFS { self.puffs.remove(0); }
                self.counter = self.counter.wrapping_add(1);
                let jitter = Vec3::new(hash01(self.counter * 3) - 0.5, hash01(self.counter * 3 + 1) - 0.5, hash01(self.counter * 3 + 2) - 0.5);
                self.puffs.push(Puff {
                    pos: hull.transform_point3(stack),
                    vel: hull_velocity + (direction + jitter * 0.25) * exit_speed * (0.2 + 0.1 * hash01(self.counter * 7)),
                    radius: START_RADIUS_M,
                    density0: density,
                    age: 0.0,
                    life: life * (0.8 + 0.4 * hash01(self.counter * 11)),
                    heat: 1.0,
                    soot,
                    haze,
                    seed: hash01(self.counter * 13) * 100.0,
                });
            }
        }
    }

    /// The puffs to draw: extinction falls as the puff grows, but only inversely with its radius:
    /// the cold air it entrains keeps condensing its vapour into fog (a dry plume would fall with
    /// the square); it fades over its last third as the fog evaporates.
    pub fn render_puffs(&self) -> impl Iterator<Item = RenderPuff> + '_ {
        self.puffs.iter().map(|p| {
            let spread = START_RADIUS_M / p.radius;
            let fade = 1.0 - ((p.age / p.life - 0.66) / 0.34).clamp(0.0, 1.0);
            let white = [1.12, 1.12, 1.14];
            let haze = [0.78, 0.84, 0.98];
            let black = [0.10, 0.10, 0.10];
            let colour: [f32; 3] = std::array::from_fn(|i| {
                let base = white[i] + (haze[i] - white[i]) * p.haze;
                base + (black[i] - base) * p.soot
            });
            RenderPuff { pos: p.pos, radius: p.radius, colour, density: (p.density0 * spread).max(p.density0 * 0.004) * fade,
                heat: p.heat, seed: p.seed }
        })
    }

    /// How strongly the plume marks the tank to an observer at range, 0..1: a dense, white,
    /// growing column by day (the caller scales for night).
    pub fn signature(&self) -> f32 {
        if self.puffs.is_empty() { return 0.0; }
        // The summed optical depth across the puffs: a full cold winter idle plume sums to about
        // 1100 (each puff keeps density x radius as it grows).
        let depth: f32 = self.render_puffs().map(|p| p.density * p.radius).sum();
        (depth / 2000.0).clamp(0.0, 1.0)
    }

    pub fn puff_count(&self) -> usize { self.puffs.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(plume: &mut ExhaustPlume, seconds: f32, rpm: f32, load: f32, throttle: f32, hull: Mat4, wind: Vec3, winter: bool) {
        for _ in 0..(seconds * 60.0) as usize { plume.update(1.0 / 60.0, true, rpm, load, throttle, hull, wind, winter); }
    }

    #[test]
    fn a_cold_winter_idle_fills_the_air_and_warming_up_thins_it() {
        let mut cold = ExhaustPlume::default();
        run(&mut cold, 12.0, 800.0, 0.0, 0.0, Mat4::IDENTITY, Vec3::ZERO, true);
        let mut warm = ExhaustPlume { engine_temperature: 1.0, ..ExhaustPlume::default() };
        run(&mut warm, 12.0, 800.0, 0.0, 0.0, Mat4::IDENTITY, Vec3::ZERO, true);
        assert!(cold.puff_count() > 150, "{}", cold.puff_count());
        assert!(cold.signature() > warm.signature() * 1.3, "{} {}", cold.signature(), warm.signature());
        let mut summer = ExhaustPlume { engine_temperature: 1.0, ..ExhaustPlume::default() };
        run(&mut summer, 12.0, 800.0, 0.0, 0.0, Mat4::IDENTITY, Vec3::ZERO, false);
        assert!(summer.signature() < warm.signature(), "warm exhaust in winter stays visible");
    }

    #[test]
    fn the_plume_trails_behind_a_moving_tank_and_drifts_downwind() {
        let mut plume = ExhaustPlume::default();
        // Driving +z at 5 m/s: the hull's origin advances each step.
        for i in 0..600 {
            let hull = Mat4::from_translation(Vec3::new(0.0, 0.0, i as f32 * 5.0 / 60.0));
            plume.update(1.0 / 60.0, true, 1800.0, 0.6, 0.6, hull, Vec3::new(3.0, 0.0, 0.0), true);
        }
        let tank_z = 599.0 * 5.0 / 60.0;
        let mean = plume.render_puffs().map(|p| p.pos).sum::<Vec3>() / plume.puff_count() as f32;
        assert!(mean.z < tank_z - 8.0, "the plume lies behind the tank: {mean:?}");
        assert!(mean.x > 2.0, "and downwind: {mean:?}");
        assert!(plume.render_puffs().all(|p| p.radius > 0.0 && p.density >= 0.0));
    }

    #[test]
    fn a_hard_throttle_sends_black_puffs_and_a_stopped_engine_none() {
        let mut plume = ExhaustPlume { engine_temperature: 1.0, ..ExhaustPlume::default() };
        run(&mut plume, 1.0, 800.0, 0.0, 0.0, Mat4::IDENTITY, Vec3::ZERO, true);
        run(&mut plume, 0.3, 2600.0, 1.0, 1.0, Mat4::IDENTITY, Vec3::ZERO, true);
        assert!(plume.render_puffs().any(|p| p.colour[0] < 0.3), "black puffs on the throttle");
        let mut off = ExhaustPlume::default();
        for _ in 0..120 { off.update(1.0 / 60.0, false, 0.0, 0.0, 0.0, Mat4::IDENTITY, Vec3::ZERO, true); }
        assert_eq!(off.puff_count(), 0);
    }
}
