//! The hot bodies the fire, spark and fume side of the game owns, read once a frame by the heat
//! shimmer's refraction (fea-damage; the owner, 2026-11-03: air refracting around glowing metal,
//! hot fragments, spent cases and fires) through `Game::hot_surfaces`. Each is what is hot, where,
//! how big and how hot, from the model that already carries it:
//!
//! * a penetration's rim: `weapon_particle_math::rim_heat` and `rim_temperature` (its worked steel
//!   cooling by conduction into the plate);
//! * a hot fragment: its solved temperature (`fea_damage::fragments::cool`);
//! * a fire: the cabin fire's burning surface patches and the burning wrecks, at a flame
//!   temperature, their area scaled by the fire model's own intensity;
//! * a spent case: its wall cooling from the breech (`case_wall_k`).

use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum HotKind { PenetrationRim, Fragment, FirePatch, WreckFire, SpentCase }

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct HotSurface {
    pub kind: HotKind,
    /// Its centre, world metres.
    pub world: Vec3,
    /// The hot face's outward normal where it has one (a rim's plate, a fire's surface); zero for a
    /// body in the air or lying loose (a chip, a case), whose plume simply rises.
    pub normal: Vec3,
    /// Its characteristic length, m: a rim's outer radius, a chip's edge, a fire patch's radius, a
    /// case's length.
    pub size_m: f32,
    /// Its hot area in contact with the air, m².
    pub area_m2: f32,
    /// Its surface temperature, or a fire's flame temperature, K.
    pub surface_k: f32,
}

/// The surroundings' temperature the rims and cases cool toward, K (as `rim_temperature`).
pub const AMBIENT_K: f32 = 293.0;
/// A buoyant fire's flame temperature, K: McCaffrey's continuous-flame region of a buoyant
/// diffusion flame, about 1,100-1,200 K (named estimate for the cabin's fuel and oil and a wreck).
pub const FLAME_K: f32 = 1150.0;
/// A cabin fire patch's radius, m (`surface_fire`'s patch).
pub const FIRE_PATCH_RADIUS_M: f32 = 0.12;
/// A burning wreck's drawn fire, m: the crown radius its flame and soot are drawn with (named).
pub const WRECK_FIRE_RADIUS_M: f32 = 0.72;

/// A spent case wall's temperature as it leaves the breech, K (named estimate: a fired large
/// calibre case is far too hot to hold, of the order of 150 °C; no measurement for the 8.8 cm case
/// was found).
pub const CASE_WALL_INITIAL_K: f32 = 423.0;
/// The case wall's cooling time constant, s: its heat capacity per area over the air's film
/// coefficient, (rho c t) / (h_conv + h_rad), with steel (7,850 kg/m³, 490 J/kg K), a 1.5 mm wall,
/// natural convection 8 W/m²K and a lacquered wall's radiation (emissivity 0.8 near 360 K) 8 W/m²K:
/// 360 s (named estimate; a brass case is of the same order).
pub const CASE_WALL_TAU_S: f32 = 7850.0 * 490.0 * 0.0015 / 16.0;
/// A case's length and its outer area in contact with the air, m and m² (the 0.58 m case of
/// `procedural_ordnance`, a mean diameter of about 0.12 m).
pub const CASE_LENGTH_M: f32 = crate::procedural_ordnance::CASE_LENGTH_M;
pub const CASE_AREA_M2: f32 = std::f32::consts::PI * 0.12 * CASE_LENGTH_M;
/// Reported until within this of the surroundings, K.
pub const REPORT_ABOVE_K: f32 = 5.0;

/// A spent case's wall temperature `age_s` seconds after it was drawn from the breech, K.
pub fn case_wall_k(age_s: f32) -> f32 {
    AMBIENT_K + (CASE_WALL_INITIAL_K - AMBIENT_K) * (-age_s.max(0.0) / CASE_WALL_TAU_S).exp()
}

/// How long a case is reported (and its clock kept), s: until it is within `REPORT_ABOVE_K`.
pub fn case_hot_s() -> f32 {
    CASE_WALL_TAU_S * ((CASE_WALL_INITIAL_K - AMBIENT_K) / REPORT_ABOVE_K).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_case_cools_over_minutes_from_the_breech() {
        assert_eq!(case_wall_k(0.0), CASE_WALL_INITIAL_K);
        assert!((CASE_WALL_TAU_S - 360.6).abs() < 1.0);
        assert!(case_wall_k(60.0) > 400.0, "still hot a minute on");
        assert!(case_wall_k(CASE_WALL_TAU_S) < AMBIENT_K + 0.37 * (CASE_WALL_INITIAL_K - AMBIENT_K) + 0.5);
        assert!((case_wall_k(case_hot_s()) - (AMBIENT_K + REPORT_ABOVE_K)).abs() < 0.01);
        assert!((15.0 * 60.0..25.0 * 60.0).contains(&case_hot_s()));
    }

    #[test]
    fn every_owned_heat_source_is_reported_and_every_drawn_case_starts_its_clock() {
        let gather = include_str!("game_hot_surfaces.rs");
        for kind in ["HotKind::PenetrationRim", "HotKind::Fragment", "HotKind::FirePatch", "HotKind::WreckFire", "HotKind::SpentCase"] {
            assert!(gather.contains(kind), "Game::hot_surfaces never reports {kind}");
        }
        // The cases drawn from the breech (the crew's ejection, the loader's pull) are the ones that vent.
        let mut clocks = 0;
        for source in [include_str!("game.rs"), include_str!("loader_hand_service.rs")] {
            let lines: Vec<&str> = source.lines().collect();
            for (i, line) in lines.iter().enumerate().filter(|(_, l)| l.contains("cabin_fumes.case_freed(")) {
                assert!(lines[i.saturating_sub(2)..i].iter().any(|l| l.contains("fired_s = Some(0.0)")), "line {}: {line}", i + 1);
                clocks += 1;
            }
        }
        assert_eq!(clocks, 2);
    }
}
