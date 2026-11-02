//! Material table for the layered damage model. Generic engineering values: each row names its
//! provenance class. None of these is a measured property of a particular Tiger's plate or casing.
//!
//! The constitutive form used by the lattice is the common Johnson–Cook-shaped flow stress
//! `σ = (σ_y + H ε_p) (1 + C ln(ε̇/ε̇₀)) (1 − T*)`, `T* = (T − T₀)/(T_m − T₀)`, with linear hardening
//! instead of a power law. The chain uses only `density` and `ke_resistance`.

use serde::{Deserialize, Serialize};

/// Room temperature of a cold vehicle, K. The physical environment's air temperature replaces it
/// where the caller has one.
pub const AMBIENT_K: f32 = 293.0;
/// Fraction of plastic work converted to heat (Taylor–Quinney); 0.9 is the textbook value for
/// steels at large strain.
pub const TAYLOR_QUINNEY: f32 = 0.9;
/// Stefan–Boltzmann constant, W m⁻² K⁻⁴.
pub const STEFAN_BOLTZMANN: f32 = 5.670_374e-8;
/// Rolled armour density, kg/m³: the reference for RHA-equivalent thickness.
pub const RHA_DENSITY: f32 = 7850.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialId {
    /// The Tiger's rolled homogeneous armour (Brinell ~260–300).
    RolledArmour,
    /// Cast armour steel (Soviet cast turrets; the Tiger's cast mantlet parts).
    CastArmour,
    /// Grey cast iron.
    CastIron,
    /// Cast steel for housings (gear and final-drive cases).
    CastSteel,
    /// Cast aluminium alloy (the HL 210's crankcase).
    CastAluminium,
    /// Mild sheet steel (tanks, bins, cabinets, shrouds).
    MildSteel,
    /// Copper and brass (radiator cores, cartridge cases).
    Copper,
    /// Optical glass.
    Glass,
    Petrol,
    EngineOil,
    Coolant,
    Air,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Material {
    pub name: &'static str,
    /// kg/m³
    pub density: f32,
    /// Young's modulus, Pa.
    pub youngs: f32,
    /// Initial yield stress, Pa.
    pub yield_stress: f32,
    /// Linear hardening modulus, Pa per unit plastic strain.
    pub hardening: f32,
    /// Logarithmic strain-rate coefficient (Johnson–Cook form).
    pub rate_c: f32,
    /// Reference strain rate, 1/s.
    pub ref_rate: f32,
    /// Melting (softening end) temperature, K.
    pub melt_k: f32,
    /// Plastic strain at failure in tension, the lattice's bond failure stretch.
    pub failure_strain: f32,
    /// J/(kg K)
    pub specific_heat: f32,
    /// W/(m K)
    pub conductivity: f32,
    /// Total hemispherical emissivity of the surface as found in service (oxidised, scaled).
    pub emissivity: f32,
    /// Resistance to a kinetic penetrator per unit areal density, relative to rolled armour. The
    /// chain's RHA-equivalent thickness is `thickness × density / RHA_DENSITY × ke_resistance`.
    pub ke_resistance: f32,
    pub fluid: bool,
    pub provenance: &'static str,
}

impl MaterialId {
    pub fn get(self) -> &'static Material {
        match self {
            MaterialId::RolledArmour => &ROLLED_ARMOUR,
            MaterialId::CastArmour => &CAST_ARMOUR,
            MaterialId::CastIron => &CAST_IRON,
            MaterialId::CastSteel => &CAST_STEEL,
            MaterialId::CastAluminium => &CAST_ALUMINIUM,
            MaterialId::MildSteel => &MILD_STEEL,
            MaterialId::Copper => &COPPER,
            MaterialId::Glass => &GLASS,
            MaterialId::Petrol => &PETROL,
            MaterialId::EngineOil => &ENGINE_OIL,
            MaterialId::Coolant => &COOLANT,
            MaterialId::Air => &AIR,
        }
    }
}

const STEEL_C: f32 = 470.0;
const STEEL_K: f32 = 40.0;

pub static ROLLED_ARMOUR: Material = Material {
    name: "rolled armour steel",
    density: RHA_DENSITY,
    youngs: 207.0e9,
    yield_stress: 800.0e6,
    hardening: 1.5e9,
    rate_c: 0.014,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.20,
    specific_heat: STEEL_C,
    conductivity: STEEL_K,
    emissivity: 0.80,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "generic quenched-and-tempered armour steel, Brinell ~270: yield from the hardness-strength relation, hardening and rate coefficient generic Johnson-Cook-form steel values, elongation ~20 %; emissivity of oxidised steel",
};

pub static CAST_ARMOUR: Material = Material {
    name: "cast armour steel",
    density: 7800.0,
    youngs: 200.0e9,
    yield_stress: 700.0e6,
    hardening: 1.2e9,
    rate_c: 0.014,
    ref_rate: 1.0,
    melt_k: 1773.0,
    failure_strain: 0.12,
    specific_heat: STEEL_C,
    conductivity: STEEL_K,
    emissivity: 0.80,
    ke_resistance: 0.94,
    fluid: false,
    provenance: "generic cast armour: lower yield and ductility than rolled plate; resistance 0.94 is the project's existing Cast quality factor (ballistics.rs)",
};

pub static CAST_IRON: Material = Material {
    name: "grey cast iron",
    density: 7200.0,
    youngs: 110.0e9,
    yield_stress: 250.0e6,
    hardening: 0.0,
    rate_c: 0.0,
    ref_rate: 1.0,
    melt_k: 1450.0,
    failure_strain: 0.01,
    specific_heat: 460.0,
    conductivity: 50.0,
    emissivity: 0.80,
    ke_resistance: 0.45,
    fluid: false,
    provenance: "generic grey iron (tensile ~250 MPa, brittle); resistance per areal density is a named estimate, no test data",
};

pub static CAST_STEEL: Material = Material {
    name: "cast housing steel",
    density: 7850.0,
    youngs: 200.0e9,
    yield_stress: 350.0e6,
    hardening: 0.9e9,
    rate_c: 0.014,
    ref_rate: 1.0,
    melt_k: 1773.0,
    failure_strain: 0.18,
    specific_heat: STEEL_C,
    conductivity: STEEL_K,
    emissivity: 0.80,
    ke_resistance: 0.60,
    fluid: false,
    provenance: "generic unalloyed cast steel for gear housings; resistance per areal density is a named estimate scaled by hardness against armour",
};

pub static CAST_ALUMINIUM: Material = Material {
    name: "cast aluminium alloy",
    density: 2700.0,
    youngs: 70.0e9,
    yield_stress: 140.0e6,
    hardening: 0.3e9,
    rate_c: 0.002,
    ref_rate: 1.0,
    melt_k: 870.0,
    failure_strain: 0.04,
    specific_heat: 900.0,
    conductivity: 150.0,
    emissivity: 0.25,
    ke_resistance: 0.60,
    fluid: false,
    provenance: "generic Al-Si casting alloy; resistance per areal density is a named estimate, no test data",
};

pub static MILD_STEEL: Material = Material {
    name: "mild sheet steel",
    density: 7850.0,
    youngs: 207.0e9,
    yield_stress: 250.0e6,
    hardening: 0.8e9,
    rate_c: 0.03,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.30,
    specific_heat: STEEL_C,
    conductivity: 50.0,
    emissivity: 0.80,
    ke_resistance: 0.55,
    fluid: false,
    provenance: "generic low-carbon sheet; resistance per areal density is a named estimate scaled by hardness against armour",
};

pub static COPPER: Material = Material {
    name: "copper and brass",
    density: 8700.0,
    youngs: 115.0e9,
    yield_stress: 100.0e6,
    hardening: 0.5e9,
    rate_c: 0.025,
    ref_rate: 1.0,
    melt_k: 1250.0,
    failure_strain: 0.40,
    specific_heat: 385.0,
    conductivity: 300.0,
    emissivity: 0.60,
    ke_resistance: 0.30,
    fluid: false,
    provenance: "generic copper/brass; resistance per areal density is a named estimate",
};

pub static GLASS: Material = Material {
    name: "optical glass",
    density: 2500.0,
    youngs: 70.0e9,
    yield_stress: 50.0e6,
    hardening: 0.0,
    rate_c: 0.0,
    ref_rate: 1.0,
    melt_k: 1000.0,
    failure_strain: 0.001,
    specific_heat: 840.0,
    conductivity: 1.0,
    emissivity: 0.90,
    ke_resistance: 0.30,
    fluid: false,
    provenance: "generic soda-lime/crown glass; brittle",
};

const fn fluid(name: &'static str, density: f32, specific_heat: f32, provenance: &'static str) -> Material {
    Material {
        name,
        density,
        youngs: 0.0,
        yield_stress: 0.0,
        hardening: 0.0,
        rate_c: 0.0,
        ref_rate: 1.0,
        melt_k: 0.0,
        failure_strain: 0.0,
        specific_heat,
        conductivity: 0.15,
        emissivity: 0.9,
        ke_resistance: 0.0,
        fluid: true,
        provenance,
    }
}

pub static PETROL: Material = fluid("petrol", 740.0, 2220.0, "typical motor gasoline density");
pub static ENGINE_OIL: Material = fluid("engine and gear oil", 880.0, 1900.0, "typical mineral oil density");
pub static COOLANT: Material = fluid("coolant", 1050.0, 3600.0, "water-glycol mixture, typical");
pub static AIR: Material = fluid("air", 1.2, 1005.0, "sea-level air at 20 C; the caller supplies the physical environment's density where known");

/// Flow stress of `m` at plastic strain `ep`, strain rate `rate` (1/s) and temperature `t_k`.
pub fn flow_stress(m: &Material, ep: f32, rate: f32, t_k: f32) -> f32 {
    let base = m.yield_stress + m.hardening * ep.max(0.0);
    let rate_term = 1.0 + m.rate_c * (rate.abs() / m.ref_rate).max(1.0).ln();
    let soft = if m.melt_k > AMBIENT_K {
        (1.0 - ((t_k - AMBIENT_K) / (m.melt_k - AMBIENT_K)).clamp(0.0, 1.0)).max(0.0)
    } else {
        1.0
    };
    base * rate_term * soft
}

/// RHA-equivalent thickness (mm) of `thickness_m` of `m` at the given mean density (a homogenised
/// interior passes its own; a solid layer passes the material's).
pub fn rha_equivalent_mm(m: &Material, thickness_m: f32, density: f32) -> f32 {
    thickness_m.max(0.0) * 1000.0 * (density / RHA_DENSITY) * m.ke_resistance
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flow_stress_hardens_with_strain_and_rate_and_softens_with_heat() {
        let m = MaterialId::RolledArmour.get();
        let cold = flow_stress(m, 0.0, 1.0, AMBIENT_K);
        assert!((cold - m.yield_stress).abs() < 1.0);
        assert!(flow_stress(m, 0.1, 1.0, AMBIENT_K) > cold);
        assert!(flow_stress(m, 0.0, 1.0e4, AMBIENT_K) > cold * 1.1);
        assert!(flow_stress(m, 0.0, 1.0, 1200.0) < cold * 0.5);
        assert_eq!(flow_stress(m, 0.0, 1.0, m.melt_k + 10.0), 0.0);
    }
    #[test]
    fn rha_equivalent_is_mass_scaled_and_armour_is_identity() {
        let armour = MaterialId::RolledArmour.get();
        assert!((rha_equivalent_mm(armour, 0.08, armour.density) - 80.0).abs() < 1e-3);
        let al = MaterialId::CastAluminium.get();
        let eq = rha_equivalent_mm(al, 0.01, al.density);
        assert!(eq > 1.0 && eq < 3.0, "10 mm cast aluminium = {eq} mm RHA");
        assert_eq!(rha_equivalent_mm(MaterialId::Petrol.get(), 1.0, 740.0), 0.0);
    }
}
