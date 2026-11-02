//! The fighting compartment's air: the quasi-static pressure a filled round's burst raises in it,
//! its venting through whatever is open, and the load on the hatches.
//! docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 4".
//!
//! Named approximations:
//! * Quasi-static only: the burst's energy is shared at once through the compartment's free air as
//!   an ideal gas (γ = 1.4): Δp = (γ − 1) Q / V. No shock or reflected-pressure history is modelled.
//! * The energy is the filler's chemical energy (the game's existing TNT equivalence) minus the
//!   kinetic energy the round's own fragments carry away (conservation, not a fitted share).
//! * Venting through the open area by isentropic orifice flow (choked above the critical ratio)
//!   with a discharge coefficient of 0.6 (named, sharp-edged opening).
//! * The free air volume is an estimate of the real Tiger I's crew compartments with the turret,
//!   less machinery, stowage and crew (named).

use serde::Serialize;

pub const GAMMA: f32 = 1.4;
pub const R_AIR: f32 = 287.05;
pub const P_ATM: f32 = 101_325.0;
pub const DISCHARGE: f32 = 0.6;
/// Free air in the Tiger's driving and fighting compartments with the turret, m³ (named estimate,
/// real scale: about 13 m³ of enclosed volume less about 3 m³ of machinery, stowage and crew).
pub const TIGER_CREW_AIR_M3: f32 = 10.0;
/// Force a closed, latched hatch holds before it is blown open, N (named estimate).
pub const HATCH_LATCH_N: f32 = 20_000.0;
/// TNT's conventional energy equivalent, J/kg (the game's filler equivalence).
pub const TNT_J_PER_KG: f32 = 4.184e6;
/// Sampling of the game's compartment history: 0.1 ms steps over 0.2 s (named).
pub const GAME_DT_S: f32 = 1.0e-4;
pub const GAME_DURATION_S: f32 = 0.2;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Opening {
    pub name: &'static str,
    pub area_m2: f32,
    /// Open now (an open hatch, a port, a hole); a closed hatch only vents once blown open.
    pub open: bool,
    /// A hatch that the pressure can force (its area loads its latch).
    pub hatch: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PressureHistory {
    /// Seconds after the burst and the compartment's gauge pressure (Pa).
    pub samples: Vec<(f32, f32)>,
    pub peak_gauge_pa: f32,
    /// Hatches forced open, with the time (s) they went.
    pub hatches_blown: Vec<(&'static str, f32)>,
    pub energy_into_air_j: f32,
}

/// The burst of `filler_kg` of explosive (TNT-equivalent energy `j_per_kg`) in `volume_m3` of air
/// at `air_k`, with `fragment_kinetic_j` carried away by the round's fragments, venting through
/// `openings`; sampled every `dt` for `duration` seconds.
pub fn burst(filler_kg: f32, j_per_kg: f32, fragment_kinetic_j: f32, volume_m3: f32, air_k: f32,
    openings: &[Opening], dt: f32, duration: f32) -> PressureHistory {
    let q = (filler_kg.max(0.0) * j_per_kg - fragment_kinetic_j.max(0.0)).max(0.0);
    let v = volume_m3.max(0.1);
    // Initial state: ambient air, then the energy added at constant volume.
    let mut mass = P_ATM * v / (R_AIR * air_k);
    let cv = R_AIR / (GAMMA - 1.0);
    let mut energy = mass * cv * air_k + q;
    let mut open: Vec<bool> = openings.iter().map(|o| o.open).collect();
    let mut out = PressureHistory { samples: Vec::new(), peak_gauge_pa: 0.0, hatches_blown: Vec::new(), energy_into_air_j: q };
    let mut t = 0.0f32;
    let steps = (duration / dt).ceil().max(1.0) as usize;
    for _ in 0..=steps {
        let temp = energy / (mass * cv);
        let p = mass * R_AIR * temp / v;
        let gauge = p - P_ATM;
        out.peak_gauge_pa = out.peak_gauge_pa.max(gauge);
        out.samples.push((t, gauge));
        for (i, o) in openings.iter().enumerate() {
            if o.hatch && !open[i] && gauge * o.area_m2 > HATCH_LATCH_N {
                open[i] = true;
                out.hatches_blown.push((o.name, t));
            }
        }
        let area: f32 = openings.iter().zip(&open).filter(|(_, &is_open)| is_open).map(|(o, _)| o.area_m2).sum();
        if area > 0.0 && p > P_ATM {
            let mdot = orifice_mass_flow(p, temp, P_ATM, DISCHARGE * area);
            let dm = (mdot * dt).min(mass * 0.5);
            // The gas leaving carries its stagnation enthalpy (cp T per kg).
            let h = cv * GAMMA * temp;
            energy -= dm * h;
            mass -= dm;
        }
        t += dt;
    }
    out
}

/// Isentropic mass flow (kg/s) through an orifice of effective area `area` from stagnation
/// pressure `p0` and temperature `t0` into `p_back`; choked above the critical ratio.
pub fn orifice_mass_flow(p0: f32, t0: f32, p_back: f32, area: f32) -> f32 {
    if p0 <= p_back || area <= 0.0 {
        return 0.0;
    }
    let ratio = (p_back / p0).max(1e-6);
    let critical = (2.0 / (GAMMA + 1.0)).powf(GAMMA / (GAMMA - 1.0));
    let r = ratio.max(critical);
    let term = (2.0 * GAMMA / ((GAMMA - 1.0) * R_AIR * t0)) * (r.powf(2.0 / GAMMA) - r.powf((GAMMA + 1.0) / GAMMA));
    area * p0 * term.max(0.0).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_compartment_holds_the_ideal_gas_pressure() {
        let h = burst(0.065, 4.184e6, 0.0, 10.0, 293.0, &[], 1e-4, 0.01);
        let expected = (GAMMA - 1.0) * 0.065 * 4.184e6 / 10.0;
        assert!((h.peak_gauge_pa - expected).abs() / expected < 1e-3, "{} vs {expected}", h.peak_gauge_pa);
        let last = h.samples.last().unwrap().1;
        assert!((last - h.peak_gauge_pa).abs() < 1.0, "no vent, no loss");
    }

    #[test]
    fn fragments_carry_energy_away_and_venting_relieves_the_pressure() {
        let with_fragments = burst(0.065, 4.184e6, 100_000.0, 10.0, 293.0, &[], 1e-4, 0.001);
        let without = burst(0.065, 4.184e6, 0.0, 10.0, 293.0, &[], 1e-4, 0.001);
        assert!(with_fragments.peak_gauge_pa < without.peak_gauge_pa);
        let hatch = [Opening { name: "commander's cupola", area_m2: 0.2, open: true, hatch: true }];
        let vented = burst(0.065, 4.184e6, 0.0, 10.0, 293.0, &hatch, 1e-4, 0.5);
        let end = vented.samples.last().unwrap().1;
        assert!(end < 0.05 * vented.peak_gauge_pa, "vented to {end} Pa from {}", vented.peak_gauge_pa);
        // Monotone decay while venting.
        assert!(vented.samples.windows(2).all(|w| w[1].1 <= w[0].1 + 1e-3));
    }

    #[test]
    fn a_closed_hatch_blows_only_past_its_latch() {
        let hatches = [Opening { name: "loader's hatch", area_m2: 0.3, open: false, hatch: true }];
        let small = burst(0.065, 4.184e6, 0.0, 10.0, 293.0, &hatches, 1e-4, 0.05);
        assert!(small.hatches_blown.is_empty(), "{} Pa on 0.3 m2", small.peak_gauge_pa);
        let large = burst(2.0, 4.184e6, 0.0, 10.0, 293.0, &hatches, 1e-4, 0.05);
        assert_eq!(large.hatches_blown.len(), 1);
    }

    #[test]
    fn choked_flow_matches_the_closed_form() {
        // Choked: mdot = A p0 sqrt(γ/(R T0)) (2/(γ+1))^((γ+1)/(2(γ−1))).
        let (p0, t0, a) = (500_000.0f32, 400.0f32, 0.01f32);
        let expected = a * p0 * (GAMMA / (R_AIR * t0)).sqrt() * (2.0 / (GAMMA + 1.0)).powf((GAMMA + 1.0) / (2.0 * (GAMMA - 1.0)));
        let got = orifice_mass_flow(p0, t0, P_ATM, a);
        assert!((got - expected).abs() / expected < 1e-3, "{got} vs {expected}");
        assert_eq!(orifice_mass_flow(P_ATM, 300.0, P_ATM, a), 0.0);
    }
}
