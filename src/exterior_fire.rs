//! A petrol bomb's fire on the Tiger's outside (the owner, 9 November 2026: "molotov cocktails at
//! the tank which could cause the interior to heat up if fires are not extinguished").
//!
//! The fire itself is the existing source-surface fire (`surface_fire::CabinFire` patches on the
//! hull and turret meshes, drawn by the existing fire presentation, put out by the existing
//! extinguisher's foam coverage). This module only says where each burning patch's heat goes,
//! so `game_cookoff` can hand the existing one-zone cabin heat model (`cabin_heat`) the right
//! share, and how long a bottle's own petrol lasts.
//!
//! Heat path (named in docs/PHYSICAL_TRUTH_AUDIT.md, all shares estimates):
//! - a patch inside the crew compartments: all of it (`cabin_heat`'s existing rule);
//! - inside the engine bay: `game_cookoff::FIREWALL_SHARE` of it (existing rule);
//! - on the outside over a cooling intake: the radiator fans draw the flame and the burning
//!   petrol runs through the louvres, so `INTAKE_SHARE` of it enters the engine bay and the
//!   firewall share of that reaches the crew; the fire there is fed by the bay's own fuel and oil
//!   wetted through the louvres and does not burn out by itself;
//! - on the outside anywhere else: `PLATE_SHARE` conducted through the armour plate under it;
//!   that patch burns only the bottle's petrol and goes out when it is spent.
//! - an intake fire burning `ENGINE_BAY_IGNITION_S` sets the bay's own contents alight (a patch
//!   on the first source surface below the louvres).
use glam::{Vec2, Vec3};

/// Share of an intake fire's heat drawn into the engine bay (estimate: the two radiator fan pairs
/// pull the air over the louvres).
pub const INTAKE_SHARE: f32 = 0.8;
/// Share of an outside fire's heat conducted through the armour plate under it into the space
/// below (estimate for 25-80 mm steel under a flame for minutes).
pub const PLATE_SHARE: f32 = 0.03;
/// Seconds an intake fire burns before the engine bay's own contents catch (estimate).
pub const ENGINE_BAY_IGNITION_S: f32 = 20.0;
/// Petrol's mass burning rate in a pool (kg/m²·s; Babrauskas' large-pool value, about 0.055).
pub const PETROL_BURN_KG_M2_S: f32 = 0.055;
/// One fire patch's burning area (m²): `surface_fire`'s 0.12 m patch radius.
pub const PATCH_AREA_M2: f32 = std::f32::consts::PI * 0.12 * 0.12;
/// Patches a broken bottle lights round the point it struck (its splash; estimate).
pub const SPLASH_PATCHES: usize = 3;
/// The splash's reach from the impact point (m; estimate for 0.5 l thrown against steel).
pub const SPLASH_M: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// In the fighting or driving compartment's air.
    CrewAir,
    /// In the engine bay, behind the firewall.
    EngineBay,
    /// On the outside, over a cooling intake.
    Intake,
    /// On the outside armour elsewhere.
    Plate,
}

/// The share of a patch's heat release that reaches the crew compartments' air.
pub fn crew_air_share(place: Place, firewall_share: f32) -> f32 {
    match place {
        Place::CrewAir => 1.0,
        Place::EngineBay => firewall_share,
        Place::Intake => INTAKE_SHARE * firewall_share,
        Place::Plate => PLATE_SHARE,
    }
}

/// A cooling intake's footprint on the engine deck (hull frame, metres in the frame's units):
/// the radiator and fan envelopes of `fea_damage::components` seen from above.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intake {
    pub min: Vec2,
    pub max: Vec2,
    /// Their top (the deck is above it).
    pub top_y: f32,
}

/// Is a hull-frame point over an intake (inside a footprint, with a 0.1 m margin, above its top)?
pub fn over_intake(local: Vec3, intakes: &[Intake]) -> bool {
    intakes.iter().any(|i| local.y > i.top_y && local.x >= i.min.x - 0.1 && local.x <= i.max.x + 0.1
        && local.z >= i.min.y - 0.1 && local.z <= i.max.y + 0.1)
}

/// Classify an outside patch. `outside`: its surface normal, followed 3 m, leaves the tank's
/// source geometry (the runtime's test); otherwise the existing crew-air/engine-bay rule applies.
pub fn classify(outside: bool, local: Vec3, in_crew_air: bool, intakes: &[Intake]) -> Place {
    if outside {
        if over_intake(local, intakes) { Place::Intake } else { Place::Plate }
    } else if in_crew_air { Place::CrewAir } else { Place::EngineBay }
}

/// How long a bottle's petrol keeps `patches` burning (s).
pub fn petrol_burn_s(petrol_kg: f32, patches: usize) -> f32 {
    if patches == 0 { return 0.0; }
    petrol_kg / (patches as f32 * PATCH_AREA_M2 * PETROL_BURN_KG_M2_S)
}

/// Splash points round an impact on a surface with this normal: `SPLASH_PATCHES` points within
/// `SPLASH_M`, the first at the impact, the rest round it in the surface's plane, slightly
/// downhill (petrol runs).
pub fn splash_points(point: Vec3, normal: Vec3, seed: u32) -> Vec<Vec3> {
    let n = normal.normalize_or_zero();
    let t = (if n.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }).cross(n).normalize_or_zero();
    let b = n.cross(t);
    let spin = (seed % 360) as f32 * std::f32::consts::PI / 180.0;
    let mut out = vec![point];
    for k in 1..SPLASH_PATCHES {
        let a = spin + k as f32 * std::f32::consts::TAU / (SPLASH_PATCHES - 1) as f32;
        let mut p = point + (t * a.cos() + b * a.sin()) * SPLASH_M;
        p.y -= 0.05 * (1.0 - n.y.abs());
        out.push(p);
    }
    out
}

/// One broken bottle's fire, by the identities of the patches it lit.
#[derive(Clone, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BottleFire {
    pub patches: Vec<u32>,
    /// Petrol left (kg).
    pub petrol_kg: f32,
    pub age_s: f32,
    /// Seconds any of its patches has burned over an intake.
    pub intake_s: f32,
    pub engine_bay_lit: bool,
}

impl BottleFire {
    pub fn new(patches: Vec<u32>, petrol_kg: f32) -> Self {
        BottleFire { patches, petrol_kg, age_s: 0.0, intake_s: 0.0, engine_bay_lit: false }
    }
    /// Burn `dt` with `burning` of its patches alight (`over_intake` of them over an intake).
    /// Returns (petrol spent: put the plate patches out; light the engine bay now).
    pub fn advance(&mut self, dt: f32, burning: usize, over_intake: usize) -> (bool, bool) {
        self.age_s += dt;
        // Intake patches feed on the bay below; only plate patches use the bottle's petrol.
        let plate = burning.saturating_sub(over_intake);
        self.petrol_kg = (self.petrol_kg - dt * plate as f32 * PATCH_AREA_M2 * PETROL_BURN_KG_M2_S).max(0.0);
        if over_intake > 0 { self.intake_s += dt; } else { self.intake_s = 0.0; }
        let light = !self.engine_bay_lit && self.intake_s >= ENGINE_BAY_IGNITION_S;
        if light { self.engine_bay_lit = true; }
        (self.petrol_kg <= 0.0, light)
    }
}

/// The Tiger I's engine-bay fire extinguisher: a fixed system the temperature switches set off
/// once (one charge; the Tiger's manual describes an automatic engine-compartment extinguisher;
/// the trigger delay here is an estimate). It quenches the bay's fires present when it fires.
pub const FIXED_EXTINGUISHER_DELAY_S: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cabin_heat::{CabinHeat, Env, Event, LiveRound};
    use crate::fea_damage::compartment::Opening;

    fn intakes() -> Vec<Intake> {
        vec![Intake { min: Vec2::new(0.5, -3.2), max: Vec2::new(1.5, -1.6), top_y: 1.6 },
             Intake { min: Vec2::new(-1.5, -3.2), max: Vec2::new(-0.5, -1.6), top_y: 1.6 }]
    }

    #[test]
    fn heat_path_shares_rank_crew_air_engine_bay_intake_and_plate() {
        let fw = 0.25;
        assert_eq!(crew_air_share(Place::CrewAir, fw), 1.0);
        assert_eq!(crew_air_share(Place::EngineBay, fw), 0.25);
        assert!((crew_air_share(Place::Intake, fw) - 0.2).abs() < 1e-6);
        assert!(crew_air_share(Place::Plate, fw) < crew_air_share(Place::Intake, fw));
        let i = intakes();
        assert_eq!(classify(true, Vec3::new(1.0, 2.0, -2.4), false, &i), Place::Intake);
        assert_eq!(classify(true, Vec3::new(0.0, 2.0, -2.4), false, &i), Place::Plate, "the centre engine hatch");
        assert_eq!(classify(true, Vec3::new(0.0, 2.9, 0.0), true, &i), Place::Plate, "turret roof outside");
        assert_eq!(classify(false, Vec3::new(0.0, 1.2, 0.0), true, &i), Place::CrewAir);
        assert_eq!(classify(false, Vec3::new(1.0, 1.2, -2.0), false, &i), Place::EngineBay);
    }

    #[test]
    fn a_bottle_on_plate_burns_out_but_over_an_intake_it_lights_the_bay() {
        // 0.37 kg over three patches: about 45 s.
        let t = petrol_burn_s(0.37, 3);
        assert!((35.0..60.0).contains(&t), "{t}");
        let mut plate = BottleFire::new(vec![1, 2, 3], 0.37);
        let mut out = None;
        for k in 0..6000 { let (spent, _) = plate.advance(0.05, 3, 0); if spent { out = Some(k as f32 * 0.05); break; } }
        assert!((out.unwrap() - t).abs() < 0.2);
        let mut intake = BottleFire::new(vec![4, 5, 6], 0.37);
        let mut lit = None;
        for k in 0..2000 { let (spent, light) = intake.advance(0.05, 3, 3); assert!(!spent); if light { lit = Some(k as f32 * 0.05); break; } }
        assert!((lit.unwrap() - ENGINE_BAY_IGNITION_S).abs() < 0.1);
    }

    #[test]
    fn splash_stays_on_the_struck_surface_within_its_reach() {
        let pts = splash_points(Vec3::new(1.0, 2.0, -2.0), Vec3::Y, 77);
        assert_eq!(pts.len(), SPLASH_PATCHES);
        assert!(pts.iter().all(|p| (p.y - 2.0).abs() < 1e-4 && p.distance(Vec3::new(1.0, 2.0, -2.0)) <= SPLASH_M + 1e-4));
    }

    /// The heat path into the existing one-zone model: an unattended intake fire of a dozen
    /// patches (a spread deck fire with the bay alight) takes the case walls to the existing
    /// ammunition ignition within minutes; a single plate patch does not.
    #[test]
    fn unattended_intake_fire_heats_the_cabin_to_the_existing_ammunition_ignition() {
        const PATCH_W: f32 = 45_000.0 * 1.4; // game_cookoff::PATCH_HEAT_W at full intensity
        let openings = [Opening { name: "hatch", area_m2: 0.2, open: false, hatch: true }];
        let rounds = [LiveRound { id: 1, center: Vec3::ZERO, propellant_kg: 2.5, filler_kg: 0.0 }];
        let flux = [0.0];
        let run = |fire_w: f32, seconds: f32| {
            let mut heat = CabinHeat::new(283.0);
            heat.set_whole_load(false);
            let env = Env { ambient_k: 283.0, fire_w, soot: 0.5, openings: &openings, rounds: &rounds,
                flame_flux_w_m2: &flux, turret: None };
            let mut t = 0.0;
            while t < seconds {
                if heat.advance(0.1, &env).iter().any(|e| matches!(e, Event::Ignited { .. })) { return Some(t); }
                t += 0.1;
            }
            None
        };
        let spread = 12.0 * PATCH_W * crew_air_share(Place::Intake, 0.25) + 4.0 * PATCH_W * crew_air_share(Place::EngineBay, 0.25);
        let ignited = run(spread, 1200.0).expect("an unattended deck and bay fire cooks the rounds off");
        assert!(ignited > 60.0, "not at once: {ignited} s");
        assert!(run(PATCH_W * crew_air_share(Place::Plate, 0.25), 1200.0).is_none());
    }
}
