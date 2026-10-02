//! Stable procedural particle phases. Identity stays integer until it has been reduced to a
//! bounded phase; casting a full u32 emitter ID to f32 first erases small motion increments.
//!
//! These are authoring coordinates, not a fluid simulation or a source of lighting energy.

/// A deterministic phase in [0, TAU), with 24 significant random bits.
pub(crate) fn seed_phase(mut seed: u32) -> f32 {
    seed = (seed ^ (seed >> 16)).wrapping_mul(0x7feb_352d);
    seed = (seed ^ (seed >> 15)).wrapping_mul(0x846c_a68b);
    let unit = ((seed ^ (seed >> 16)) >> 8) as f32 / 16_777_216.0;
    unit * std::f32::consts::TAU
}

/// Keep the seed independent of elapsed time. Do not wrap the result: consumers also use
/// noninteger phase frequencies and noise translations, for which a TAU wrap would pop.
pub(crate) fn advancing_phase(seed: u32, time_s: f32, rate: f32) -> f32 {
    let time = if time_s.is_finite() { time_s.max(0.0) } else { 0.0 };
    let rate = if rate.is_finite() { rate } else { 0.0 };
    seed_phase(seed) + time * rate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_emitter_ids_keep_independent_bounded_phases() {
        // These actor domains are used by the game. Neighboring full-width integers must not
        // become the same float before the hash, as happened to the smoke's lobe coordinates.
        for base in [0x7000_0000, 0x7300_0000, 0xffff_ff00] {
            let phases: Vec<_> = (0..128).map(|i| seed_phase(base + i)).collect();
            assert!(phases.iter().all(|&v| v >= 0.0 && v < std::f32::consts::TAU));
            for pair in phases.windows(2) { assert_ne!(pair[0], pair[1]); }
            assert_eq!(phases, (0..128).map(|i| seed_phase(base + i)).collect::<Vec<_>>());
        }
    }

    #[test]
    fn smoke_phase_advances_each_frame_even_for_large_source_ids() {
        for seed in [0, 0x7300_0000, u32::MAX] {
            for time in [0.0, 30.0, 600.0, 3600.0] {
                for rate in [0.55, 0.8] {
                    let a = advancing_phase(seed, time, rate);
                    let b = advancing_phase(seed, time + 1.0 / 60.0, rate);
                    assert!((b - a - rate / 60.0).abs() < 0.0004,
                        "source {seed:x}, t={time}, phase step={}", b-a);
                }
            }
        }
    }

    #[test]
    fn lobe_spacing_survives_without_phase_wrap_discontinuities() {
        let phase = seed_phase(0x7300_0000);
        let lobes = [phase, phase + 2.1, phase + 4.2];
        assert!(((lobes[1] - lobes[0]) - 2.1).abs() < 1e-6);
        assert!(((lobes[2] - lobes[1]) - 2.1).abs() < 1e-6);
        let crossing = (std::f32::consts::TAU - phase) / 0.55;
        let before = advancing_phase(0x7300_0000, crossing - 0.001, 0.55);
        let after = advancing_phase(0x7300_0000, crossing + 0.001, 0.55);
        assert!((after - before - 0.0011).abs() < 1e-6);
        assert!(advancing_phase(7, f32::NAN, 0.5).is_finite());
    }
}
