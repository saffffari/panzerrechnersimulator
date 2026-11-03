//! One incandescence law for everything that glows hot: the fire pages, the embers, the vent
//! jet, the detonation's fireball and (in `rm_sparks.wgsl`, the same bands) the sparks and the
//! hot rims. A grey body's three Planck-band ratios at its temperature, red = 1, so a page or a
//! parcel at 1500 K is the deep red of cooling soot and one at 1900 K the orange of a burning
//! tongue; nothing is tinted by hand (the owner, 2026-11-02: the flame, the fire, the blast,
//! the sparks and the glowing metal in one visual language).

/// Planck-band chroma (650, 550, 450 nm) relative to the red band, as `spark_glow` in the
/// spark shaders computes it. Not calibrated steel or soot emissivity.
pub fn planck_chroma(kelvin: f32) -> [f32; 3] {
    const C2: f32 = 14_387_769.0; // nm K
    let k = kelvin.clamp(500.0, 20_000.0);
    let reference = (C2 / (650.0 * k)).exp() - 1.0;
    [650.0f32, 550.0, 450.0].map(|wavelength| {
        (650.0 / wavelength).powi(5) * reference / ((C2 / (wavelength * k)).exp() - 1.0)
    })
}

/// A flash or a burning effect mesh driven at `drive` (its emissive intensity) glows at the Planck
/// colour of 1,500 K plus 150 K per unit of drive, to 2,800 K: about 1,800 K, the deep orange of a
/// burning tongue, at a drive of 2, white-hot at the muzzle flash's 8. The shaders' `incandescent_glow`
/// and the ray-traced emission tint use the same law, replacing a hand-tinted orange and its added
/// white-hot core (2 November 2026).
pub fn fx_kelvin(drive: f32) -> f32 { (1500.0 + 150.0 * drive.max(0.0)).clamp(1500.0, 2800.0) }
/// The drawn glow of an effect mesh at `drive`: the chroma at `fx_kelvin` times the drive times 0.30
/// (the scale the orange used, so the red band is as bright as before).
pub fn fx_glow(drive: f32) -> [f32; 3] { planck_chroma(fx_kelvin(drive)).map(|c| c * drive.max(0.0) * 0.30) }

/// The chroma as an 8-bit sRGB page colour (the red band full), for the grain prototypes.
pub fn planck_srgb8(kelvin: f32) -> [u8; 3] {
    planck_chroma(kelvin).map(|v| (linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
}
fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_effect_glows_on_the_planck_law_and_matches_the_old_orange_at_a_low_drive() {
        // The retired hand tint (1, 0.24, 0.03) was the Planck colour of about 1,800 K.
        let c = planck_chroma(fx_kelvin(2.0));
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.24).abs() < 0.03 && (c[2] - 0.03).abs() < 0.01, "{c:?}");
        // Hotter with drive: green and blue rise toward white.
        let hot = planck_chroma(fx_kelvin(8.0));
        assert!(hot[1] > c[1] && hot[2] > c[2]);
        for shader in [include_str!("../shaders/rm_gbuffer.wgsl"), include_str!("../shaders/mesh.wgsl"),
            include_str!("../shaders/gbuf.wgsl"), include_str!("../shaders/stylized_mesh.wgsl")] {
            assert!(shader.contains("clamp(1500.0 + 150.0 * x, 1500.0, 2800.0)"));
            assert!(!shader.contains("vec3(1.0, 0.24, 0.03)"));
        }
    }
    #[test]
    fn hotter_is_whiter_and_red_leads() {
        for kelvin in [1200.0, 1500.0, 1900.0, 2800.0, 3400.0] {
            let [r, g, b] = planck_chroma(kelvin);
            assert!((r - 1.0).abs() < 1e-5);
            assert!(g < r && b < g, "{kelvin} K: {r} {g} {b}");
        }
        let cool = planck_chroma(1500.0);
        let hot = planck_chroma(1900.0);
        assert!(hot[1] > cool[1] * 1.5 && hot[2] > cool[2] * 3.0, "{hot:?} {cool:?}");
        // The tracer composition's bands (`TRACER_INTENSITY`'s note): 3400 K is (1, 0.705, 0.348).
        let tracer = planck_chroma(3400.0);
        assert!((tracer[1] - 0.705).abs() < 0.01 && (tracer[2] - 0.348).abs() < 0.01, "{tracer:?}");
    }
    #[test]
    fn pages_are_deep_red_through_orange() {
        let [r, g, b] = planck_srgb8(1500.0);
        assert_eq!(r, 255);
        assert!(g < 120 && b < 30, "1500 K page {g} {b}");
        let [_, g, b] = planck_srgb8(1900.0);
        assert!(g > 120 && g < 170 && b < 70, "1900 K page {g} {b}");
    }
}
