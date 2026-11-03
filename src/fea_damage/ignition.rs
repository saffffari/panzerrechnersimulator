//! A fire lit by the strike itself (2026-11-02, the owner: a spark or a fragment meeting what
//! burns may start a fire inside the tank, each material at its own chance, some never, so that
//! fires start about as often as they did in the war).
//!
//! The round and every fragment the strike throws are traced through the vehicle's measured
//! components (`components`, the CAD envelopes the chain uses); each first entry into a component
//! that holds something that burns is one trial, a hashed draw against the chance below. The
//! "sparks" are these bodies: the incandescent spark showers drawn at an impact are presentation
//! and light nothing (named).
//!
//! - Petrol (the fuel tanks): the casing holed, the petrol sprays and its vapour (flash point about
//!   -45 °C, so a flammable mixture stands over it) lights from the impact's sparks: 0.5 a holing.
//!   A 1944 Soviet trial (NKV Laboratory 101-1) lit petrol tanks with 2 of 4 shots and full diesel
//!   tanks with 4 of 18; four shots is a small sample (named).
//! - The engine (its carburettors, pumps and fuel lines, and its oil): 0.3 a holing (authored,
//!   between petrol and oil; British 1944 casualty surveys found every engine-bay penetration that
//!   wounded the crew of a petrol-engined tank had ended in a petrol fire).
//! - Mineral oils (gearbox, steering unit, final drives, the oil-bath air filters, the hydraulic
//!   traverse and the gun's recoil oil): a spray lights only on a body hotter than its hot-surface
//!   ignition temperature (US Bureau of Mines and NIOSH (Yuan 2006) measurements: about 350 °C for
//!   lubricating and transmission oils, 440 °C for hydraulic oil): a fragment that hot meeting it,
//!   at 0.1 (authored: the sources give the temperatures, not a chance).
//! - Upholstery (the seats' leather and horsehair) and electrical insulation (the radio): a
//!   fragment above 700 K stopping in them smoulders into flame at 0.02 and 0.01 (authored; no
//!   wartime figures were found).
//! - Coolant (water with glycol), steel, glass and the guns' metal: never.
//!
//! The ammunition is not here: a case the round or a fragment crosses goes at once
//! (`crate::cabin_heat`), as dry-stowed charges "invariably" did in the US 1943 trials. A body
//! whose temperature the lattice did not solve (the round itself, the authored population) counts
//! as cold: it lights petrol and the engine by its sparks and nothing else (named). The wartime
//! comparison (fires per penetration and their causes) is measured by `fea-fire-odds`.

use serde::Serialize;

use super::components::{ComponentSpec, Interior, System};
use super::materials::MaterialId;

/// What a component holds that a strike's own heat can light.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Flammable {
    /// Petrol in a tank.
    Petrol,
    /// The engine with its fuel system and oil.
    Engine,
    /// A mineral oil, lit only by a body hotter than its hot-surface ignition (K).
    Oil { hot_surface_k: f32 },
    /// Leather and horsehair over a seat frame.
    Upholstery,
    /// Insulation and Bakelite.
    Electrical,
}

impl Flammable {
    pub fn name(self) -> &'static str {
        match self {
            Flammable::Petrol => "petrol",
            Flammable::Engine => "engine fuel system and oil",
            Flammable::Oil { hot_surface_k } if hot_surface_k >= HYDRAULIC_OIL_K => "hydraulic oil",
            Flammable::Oil { .. } => "lubricating oil",
            Flammable::Upholstery => "upholstery",
            Flammable::Electrical => "insulation",
        }
    }
}

/// A petrol tank holed by the round or a fragment lights at this chance.
pub const PETROL: f32 = 0.5;
/// The engine holed (authored).
pub const ENGINE: f32 = 0.3;
/// An oil spray meeting a body above its hot-surface ignition (authored).
pub const OIL: f32 = 0.1;
/// Hot-surface ignition: lubricating and transmission oils about 350 °C, hydraulic oil about
/// 440 °C (K).
pub const LUBRICATING_OIL_K: f32 = 623.0;
pub const HYDRAULIC_OIL_K: f32 = 713.0;
/// Upholstery and insulation smoulder into flame from a fragment above this (K; authored).
pub const SMOULDER_K: f32 = 700.0;
pub const UPHOLSTERY: f32 = 0.02;
pub const ELECTRICAL: f32 = 0.01;

/// A man's clothing (the player's own body, `player_burn`): the black wool of the Panzer uniform
/// chars rather than flames below about 570-600 °C (cotton underwear lights at about 400 °C): a
/// fragment above 850 K striking him sets it alight at 0.05 a strike, at the hottest fragment that
/// struck him (authored; no wartime figures found). The cabin fire's flames catch it as before.
pub const CLOTHING_K: f32 = 850.0;
pub const CLOTHING: f32 = 0.05;

/// The chance a man's clothing catches from a strike whose hottest fragment on him was `body_k`.
pub fn clothing_chance(body_k: f32) -> f32 {
    if body_k >= CLOTHING_K { CLOTHING } else { 0.0 }
}

/// What `spec` holds that burns, if anything.
pub fn flammable(spec: &ComponentSpec) -> Option<Flammable> {
    match spec.system {
        System::Fuel(_) => matches!(spec.interior, Interior::Tank { liquid: MaterialId::Petrol }).then_some(Flammable::Petrol),
        System::Engine => Some(Flammable::Engine),
        System::AirFilters | System::Gearbox | System::Steering | System::FinalDrive(_) =>
            Some(Flammable::Oil { hot_surface_k: LUBRICATING_OIL_K }),
        System::TurretTraverse => Some(Flammable::Oil { hot_surface_k: HYDRAULIC_OIL_K }),
        // The cradle carries the recoil brake's oil; the breech is steel.
        System::MainGun => (spec.id == "gun_cradle").then_some(Flammable::Oil { hot_surface_k: HYDRAULIC_OIL_K }),
        System::Seat => Some(Flammable::Upholstery),
        System::Radio => Some(Flammable::Electrical),
        System::Cooling(_) | System::Ammunition | System::HullMg | System::CoaxMg | System::GunnerSight => None,
    }
}

/// The chance that `what`, met by a body at `body_k` (0 when its temperature is not solved),
/// lights.
pub fn chance(what: Flammable, body_k: f32) -> f32 {
    match what {
        Flammable::Petrol => PETROL,
        Flammable::Engine => ENGINE,
        Flammable::Oil { hot_surface_k } => if body_k >= hot_surface_k { OIL } else { 0.0 },
        Flammable::Upholstery => if body_k >= SMOULDER_K { UPHOLSTERY } else { 0.0 },
        Flammable::Electrical => if body_k >= SMOULDER_K { ELECTRICAL } else { 0.0 },
    }
}

/// One deterministic draw against `chance` (a counter-seeded hash, no stateful generator).
pub fn lights(chance: f32, seed: u32) -> bool {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ 0x7F4A_7C15;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    ((h >> 8) as f32 / (1u32 << 24) as f32) < chance
}

/// One trial: the component met (`components::COMPONENT_SPECS` index), what it holds, the body's
/// temperature, the chance and whether it lit.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Trial {
    pub component: usize,
    pub what: Flammable,
    /// The round itself (true) or a fragment.
    pub by_round: bool,
    pub body_k: f32,
    pub chance: f32,
    pub lit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fea_damage::components::COMPONENT_SPECS;

    fn spec(id: &str) -> &'static ComponentSpec {
        COMPONENT_SPECS.iter().find(|s| s.id == id).expect(id)
    }

    #[test]
    fn what_burns_in_each_component() {
        assert_eq!(flammable(spec("fuel_tank_left")), Some(Flammable::Petrol));
        assert_eq!(flammable(spec("engine_block")), Some(Flammable::Engine));
        assert_eq!(flammable(spec("gearbox")), Some(Flammable::Oil { hot_surface_k: LUBRICATING_OIL_K }));
        assert_eq!(flammable(spec("turret_traverse_motor")), Some(Flammable::Oil { hot_surface_k: HYDRAULIC_OIL_K }));
        assert_eq!(flammable(spec("gun_cradle")), Some(Flammable::Oil { hot_surface_k: HYDRAULIC_OIL_K }));
        assert_eq!(flammable(spec("driver_seat")), Some(Flammable::Upholstery));
        assert_eq!(flammable(spec("radio")), Some(Flammable::Electrical));
        for never in ["gun_breech", "radiator_left", "cooling_fans_right", "ammo_floor_left", "hull_mg", "gunner_sight"] {
            assert_eq!(flammable(spec(never)), None, "{never}");
        }
    }

    #[test]
    fn oils_need_a_hot_body_and_petrol_does_not() {
        assert_eq!(chance(Flammable::Petrol, 0.0), PETROL);
        assert_eq!(chance(Flammable::Engine, 0.0), ENGINE);
        let lube = Flammable::Oil { hot_surface_k: LUBRICATING_OIL_K };
        let hydraulic = Flammable::Oil { hot_surface_k: HYDRAULIC_OIL_K };
        assert_eq!(chance(lube, 600.0), 0.0);
        assert_eq!(chance(lube, 650.0), OIL);
        assert_eq!(chance(hydraulic, 650.0), 0.0);
        assert_eq!(chance(hydraulic, 800.0), OIL);
        assert_eq!(chance(Flammable::Upholstery, 650.0), 0.0);
        assert_eq!(chance(Flammable::Upholstery, 900.0), UPHOLSTERY);
        assert_eq!(chance(Flammable::Electrical, 900.0), ELECTRICAL);
    }

    #[test]
    fn clothing_catches_only_from_a_very_hot_fragment() {
        assert_eq!(clothing_chance(0.0), 0.0);
        assert_eq!(clothing_chance(800.0), 0.0);
        assert_eq!(clothing_chance(900.0), CLOTHING);
    }

    #[test]
    fn the_draws_light_at_their_chance() {
        for p in [0.0, 0.01, 0.1, 0.3, 0.5, 1.0] {
            let n = 20_000u32;
            let lit = (0..n).filter(|&i| lights(p, i.wrapping_mul(7919) ^ 0xF1E5)).count() as f32 / n as f32;
            assert!((lit - p).abs() < 0.01 + 0.02 * p, "{p}: {lit}");
        }
    }
}
