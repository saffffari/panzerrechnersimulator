//! The weapon smoke's cost and amount (the owner, 2026-11-03: "smoke is killing me", "please turn
//! down the smoke", "next one needs to run in real time"). Tests only: the marches toward the lights
//! take each support's envelope times its breakup's measured mean (rm_medium.wgsl
//! `rm_puff_extinction_mean`), a support too thin to see costs nothing, and every weapon smoke is
//! thinner and gone by 2.5 s (`weapon_particle_math::weapon_smoke_scale`).

use crate::renderer::renderer_weapon_particles::math::{weapon_smoke_scale, WEAPON_SMOKE_DENSITY, WEAPON_SMOKE_LIFE_S};

/// The value noise of rm_common.wgsl `rm_noise`: trilinear with a smoothstep weight over lattice
/// values uniform in 0..1 (any well-mixed hash; the profile's mean does not depend on which).
fn noise(p: [f32; 3], seed: u32) -> f32 {
    let lattice = |i: i32, j: i32, k: i32| {
        let mut h = (i as u32).wrapping_mul(73_856_093) ^ (j as u32).wrapping_mul(19_349_663) ^ (k as u32).wrapping_mul(83_492_791) ^ seed;
        h ^= h >> 16; h = h.wrapping_mul(0x7feb_352d); h ^= h >> 15; h = h.wrapping_mul(0x846c_a68b); h ^= h >> 16;
        h as f32 / u32::MAX as f32
    };
    let (i, f): (Vec<i32>, Vec<f32>) = p.iter().map(|v| (v.floor() as i32, v - v.floor())).unzip();
    let w: Vec<f32> = f.iter().map(|t| t * t * (3.0 - 2.0 * t)).collect();
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x = |j: i32, k: i32| mix(lattice(i[0], i[1] + j, i[2] + k), lattice(i[0] + 1, i[1] + j, i[2] + k), w[0]);
    mix(mix(x(0, 0), x(1, 0), w[1]), mix(x(0, 1), x(1, 1), w[1]), w[2])
}

#[test]
fn the_marches_take_the_billows_measured_mean() {
    let smooth = |a: f32, b: f32, x: f32| { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) };
    let n = 200_000u32;
    let mut sum = 0.0f64;
    for i in 0..n {
        let r = |k: u32| (((i.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(40_503)) >> 8) as f32 / 16_777_216.0) * 40.0;
        let q = [r(1), r(2), r(3)];
        let v = noise(q.map(|c| c * 3.2), 3) * 0.75 + noise([q[0] * 7.1 + 7.3, q[1] * 7.1, q[2] * 7.1 + 4.1], 5) * 0.25;
        sum += f64::from(smooth(0.15, 0.75, v));
    }
    let mean = (sum / f64::from(n)) as f32;
    let shader = include_str!("../shaders/rm_medium.wgsl");
    assert!(shader.contains("const RM_BILLOW_MEAN: f32 = 0.6;"));
    assert!((mean - 0.6).abs() < 0.03, "the breakup's mean is {mean}");
    // The marches (toward the key body and the lamps, and the shade pass's far segments) use it.
    assert_eq!(shader.matches("rm_puff_extinction_mean(p)").count(), 2);
    assert_eq!(shader.matches("if (shape.y * 5.2 < RM_PUFF_MIN_EXTINCTION) { continue; }").count(), 2);
}

#[test]
fn weapon_smoke_is_thinner_and_gone_in_two_and_a_half_seconds() {
    assert_eq!(weapon_smoke_scale(0.0), WEAPON_SMOKE_DENSITY);
    assert_eq!(weapon_smoke_scale(WEAPON_SMOKE_LIFE_S), 0.0);
    assert_eq!(weapon_smoke_scale(10.0), 0.0);
    let mut last = f32::MAX;
    for i in 0..=300 {
        let s = weapon_smoke_scale(i as f32 * 0.01);
        assert!(s <= last + 1e-6 && s <= WEAPON_SMOKE_DENSITY);
        last = s;
    }
    // Both paths take it: the game's supports and the renderer's event clouds.
    assert!(include_str!("weapon_vfx.rs").contains("gpu_weapon::weapon_smoke_scale(b.age)"));
    assert!(include_str!("renderer_raymarch.rs").contains("wm::weapon_smoke_scale(t)"));
}
