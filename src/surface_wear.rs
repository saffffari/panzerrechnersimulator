//! Wear and grime on the player's Tiger (raymarch backend, 2026-10-28).
//!
//! Four loads, each 0 (factory clean) to 1: mechanical wear (chipped edges, scratches: the
//! vehicle's history, growing with every round handled), interior grime (soot, oil, propellant
//! residue, the crew's living), exterior grime (slush and mud thrown on the lower hull by the
//! tracks), and glass grime (the film on the vision blocks and sight windows, cleared by a wipe).
//! The renderer turns them into masks per pixel (`rm_gbuffer.wgsl` `apply_wear`, `rm_shade.wgsl`
//! worn edges and cavity grime, `rm_glass.wgsl`).
//!
//! Named approximations (docs/PHYSICAL_TRUTH_AUDIT.md): scalar loads for the whole vehicle, not
//! per-face accumulation; the rates are presentation estimates, not measurements; the starting
//! state is a Tiger some weeks into the cauldron.

/// The starting state: mechanical wear, interior grime, exterior grime, glass grime.
pub const CAMPAIGN_START: [f32; 4] = [0.55, 0.35, 0.30, 0.25];
/// Seconds of crewed running that take interior grime from clean toward full (1 - 1/e).
const INTERIOR_GRIME_S: f32 = 2400.0;
/// Interior grime and mechanical wear added by one main-gun round (fraction of what remains).
const SHOT_INTERIOR_GRIME: f32 = 0.03;
const SHOT_MECHANICAL: f32 = 0.004;
/// Glass film added by one round's muzzle blast and fumes (fraction of what remains).
const SHOT_GLASS: f32 = 0.02;
/// Metres driven that take exterior grime toward full, and glass grime.
const EXTERIOR_GRIME_M: f32 = 3000.0;
const GLASS_GRIME_M: f32 = 8000.0;
/// What a wipe leaves on the glass.
pub const WIPED_GLASS: f32 = 0.05;

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceWear {
    pub mechanical: f32,
    pub interior_grime: f32,
    pub exterior_grime: f32,
    pub glass_grime: f32,
    prev_rounds: Option<u32>,
}

impl Default for SurfaceWear {
    fn default() -> Self { Self::new(CAMPAIGN_START) }
}

impl SurfaceWear {
    pub fn new(loads: [f32; 4]) -> Self {
        let [mechanical, interior_grime, exterior_grime, glass_grime] = loads.map(|v| v.clamp(0.0, 1.0));
        Self { mechanical, interior_grime, exterior_grime, glass_grime, prev_rounds: None }
    }

    /// The loads for the renderer: interior grime, exterior grime, glass grime, mechanical wear.
    pub fn uniform(&self) -> [f32; 4] {
        [self.interior_grime, self.exterior_grime, self.glass_grime, self.mechanical]
    }

    pub fn wipe_glass(&mut self) { self.glass_grime = self.glass_grime.min(WIPED_GLASS); }

    /// Advance from the frame's state; `rounds_spent` is compared with the previous call.
    pub fn update(&mut self, dt: f32, vehicle_speed_m_s: f32, rounds_spent: u32) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        let shots = self.prev_rounds.map_or(0, |prev| rounds_spent.wrapping_sub(prev).min(8));
        self.prev_rounds = Some(rounds_spent);
        let toward = |load: f32, share: f32| load + (1.0 - load) * share.clamp(0.0, 1.0);
        let metres = vehicle_speed_m_s.abs().min(20.0) * dt;
        self.interior_grime = toward(self.interior_grime, dt / INTERIOR_GRIME_S);
        self.exterior_grime = toward(self.exterior_grime, metres / EXTERIOR_GRIME_M);
        self.glass_grime = toward(self.glass_grime, metres / GLASS_GRIME_M);
        for _ in 0..shots {
            self.interior_grime = toward(self.interior_grime, SHOT_INTERIOR_GRIME);
            self.mechanical = toward(self.mechanical, SHOT_MECHANICAL);
            self.glass_grime = toward(self.glass_grime, SHOT_GLASS);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_shots_and_driving_raise_their_loads_and_a_wipe_clears_the_glass() {
        let mut w = SurfaceWear::new([0.0; 4]);
        w.update(1.0 / 60.0, 0.0, 0);
        for _ in 0..(600 * 60) { w.update(1.0 / 60.0, 0.0, 0); }
        assert!((0.2..0.25).contains(&w.interior_grime), "ten minutes crewed: {}", w.interior_grime);
        assert_eq!(w.exterior_grime, 0.0, "standing still throws no slush");
        let before = w.interior_grime;
        w.update(1.0 / 60.0, 0.0, 5);
        assert!(w.interior_grime > before + 0.1, "five rounds of propellant residue");
        assert!(w.mechanical > 0.0 && w.glass_grime > 0.09);
        for _ in 0..(300 * 60) { w.update(1.0 / 60.0, 10.0, 5); }
        assert!((0.6..0.68).contains(&w.exterior_grime), "3 km driven: {}", w.exterior_grime);
        w.wipe_glass();
        assert_eq!(w.glass_grime, WIPED_GLASS);
    }

    #[test]
    fn loads_stay_in_range() {
        let mut w = SurfaceWear::new([2.0, -1.0, 0.5, 0.3]);
        assert_eq!(w.uniform(), [0.0, 0.5, 0.3, 1.0], "clamped, in uniform order");
        for _ in 0..1000 { w.update(0.25, 50.0, 0); w.update(0.25, 50.0, 100); }
        assert!(w.uniform().iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
