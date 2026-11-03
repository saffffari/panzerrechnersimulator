//! The fighting compartment's heat, its ammunition and its turret (2026-11-02, the owner: model
//! the internal temperature; a live round, racked, chambered, loose or carried, ignites when its
//! propellant is hot enough, from the compartment's hot gas or from flames reaching it; one burning
//! round sets off the rest within a second or two and blows the turret off; a direct hit on the
//! racks goes at once).
//!
//! One well-mixed gas zone, its mass and internal energy, in the burst model's free air
//! (`compartment::TIGER_CREW_AIR_M3`, ideal gas, γ 1.4):
//! - heated by the burning surfaces (their heat release, from the caller) and by burning
//!   propellant (its heat of explosion and the mass of its gas); losing heat to the hull's steel,
//!   held at ambient by its mass (one conductance); trading air with the outside through what is
//!   open (Kawagoe's ventilation factor, 0.5·A·√H kg/s); pushed out through the openings by its
//!   pressure (`compartment::orifice_mass_flow`, choked above the critical ratio) and drawing air
//!   in below ambient; never hotter than the propellant's flame. A closed hatch whose latch the
//!   gauge pressure beats is blown open (`compartment::HATCH_LATCH_N`).
//! - Each round's case wall, with the propellant heated behind it, is one lumped areal heat
//!   capacity: convection and radiation from the gas (its emissivity rising with soot), the flux of
//!   the compartment's flames in sight of it (from the caller, which owns the surfaces and their
//!   visibility) and the jet of any round already burning near it. At `IGNITION_K` its propellant
//!   ignites and burns its heat of explosion, faster under pressure (Vieille's law, r ∝ pⁿ).
//! - An HE shell's filler detonates when a strike crosses it, when another HE shell within
//!   `SYMPATHETIC_M` detonates, or when its steel body (heated as the case is, with its own heavier
//!   areal heat capacity, and by its own burning charge) reaches `FILLER_COOKOFF_K`: its energy
//!   goes into the gas at once and its own charge is lit. An armour-piercing round's small
//!   bursting charge (under `HE_MIN_FILLER_KG`) is not counted (named).
//! - The turret sits on its ring under its weight (no hold-down modelled): the gauge pressure on
//!   the ring's area lifts it; once its skirt has cleared the ring's lip (`RING_OVERLAP_M`) the gap
//!   round the ring vents the gas as it rises, and once that gap is as large as the ring itself (a
//!   quarter of its diameter further up) it flies with the speed it has; the ring then stays open,
//!   and while the turret is still over it the gas leaving through it pushes it on: the gauge
//!   pressure on the ring's area, falling as (D/(D + 2h))² with its height h above the ring (the
//!   jet widening; authored).
//!
//! Every constant here is an authored game-level value, named where it stands; this is not a
//! measured tank fire or a cook-off test.
use crate::fea_damage::compartment::{self, Opening, GAMMA, P_ATM, R_AIR};
use glam::Vec3;
use std::collections::BTreeMap;

const CV: f32 = R_AIR / (GAMMA - 1.0);
const CP: f32 = CV * GAMMA;
const VOLUME_M3: f32 = compartment::TIGER_CREW_AIR_M3;
/// Heat lost to the hull's steel per kelvin of gas above ambient (W/K): about 15 W/m²·K of mixed
/// convection over about 40 m² of interior steel held at ambient by its mass (authored).
const WALL_CONDUCTANCE_W_K: f32 = 600.0;
/// The height in Kawagoe's ventilation factor for the openings (m; authored for roof hatches).
const VENT_HEIGHT_M: f32 = 0.5;
/// The gaps of a buttoned-up tank: mantlet, ports, ring (m²; authored).
const LEAK_AREA_M2: f32 = 0.01;
/// The propellant's flame temperature (K; nitrocellulose propellant, about 2,500-3,000 K).
pub const FLAME_K: f32 = 2_600.0;
/// Heat of explosion (J/kg; a single-base propellant's, about 3.5-4 MJ/kg).
pub const PROPELLANT_J_PER_KG: f32 = 3.7e6;
/// A ruptured charge's burn at atmospheric pressure (s), shortened by Vieille's law with exponent
/// `VIEILLE_N` as the pressure rises (authored within nitrocellulose's 0.7-0.9).
pub const BURN_S: f32 = 0.3;
const VIEILLE_N: f32 = 0.8;
/// The case wall's temperature at which the propellant behind it ignites (K; nitrocellulose,
/// about 170-180 °C).
pub const IGNITION_K: f32 = 453.0;
/// Areal heat capacity of a case wall and the propellant heated behind it (J/m²·K): 1.5 mm of
/// brass (4.8 kJ/m²·K) and about a millimetre of propellant (authored).
const WALL_J_M2K: f32 = 9_000.0;
/// Convection from the compartment's gas to a case (W/m²·K; mixed convection, authored).
const CASE_CONVECTION_W_M2K: f32 = 25.0;
/// The gas's emissivity, hot combustion gas with its glowing grains to a sooty fire's (authored).
const EMISSIVITY_CLEAR: f32 = 0.3;
const EMISSIVITY_SOOT: f32 = 0.7;
const STEFAN_BOLTZMANN: f32 = 5.670_374e-8;
/// A burning charge's flame on another case (W/m²) at contact, for as long as it burns (a property
/// of the flame: a charge burning faster under pressure passes less heat on), falling as
/// (R/(R + gap))² with the gap between the cases (authored: the flame and the burning grains a
/// ruptured charge throws; a case it touches ignites in about 0.2 s).
const JET_FLUX_W_M2: f32 = 1.2e7;
const JET_REACH_M: f32 = 0.05;
/// A case's radius, for the gap between two (m; the 8.8 cm cartridge's).
const CASE_RADIUS_M: f32 = 0.045;
/// An HE filler's detonation sets off another HE shell within this distance, centre to centre (m;
/// about two shell diameters, authored).
const SYMPATHETIC_M: f32 = 0.2;
/// Fillers under this are an armour-piercing round's bursting charge and are not counted (kg).
const HE_MIN_FILLER_KG: f32 = 0.2;
/// TNT's cook-off in a heated shell, about 300 °C (K; authored), and the shell body's areal heat
/// capacity: about 10 mm of steel (J/m²·K, authored).
const FILLER_COOKOFF_K: f32 = 570.0;
const SHELL_J_M2K: f32 = 36_000.0;
/// How far the turret rises before gas escapes round its ring (m): its skirt over the ring's lip
/// (authored).
const RING_OVERLAP_M: f32 = 0.06;
const GRAVITY: f32 = 9.81;

/// A live round as the caller has it now: its identity, centre (one frame for all), propellant,
/// and its shell's high-explosive filler (kg; TNT equivalent).
#[derive(Clone, Copy, Debug)]
pub struct LiveRound {
    pub id: u32,
    pub center: Vec3,
    pub propellant_kg: f32,
    pub filler_kg: f32,
}

/// The turret on its ring: its mass and the ring's diameter (real size).
#[derive(Clone, Copy, Debug)]
pub struct Turret {
    pub mass_kg: f32,
    pub ring_diameter_m: f32,
}

/// What the compartment is given each update.
pub struct Env<'a> {
    pub ambient_k: f32,
    /// The burning surfaces' heat release (W).
    pub fire_w: f32,
    /// Soot load 0-1 (the gas's emissivity).
    pub soot: f32,
    pub openings: &'a [Opening],
    pub rounds: &'a [LiveRound],
    /// Per round (same order), the flux of flames in sight of it (W/m²).
    pub flame_flux_w_m2: &'a [f32],
    pub turret: Option<Turret>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The gas and the flames in sight heated it.
    Heat,
    /// A burning neighbour's jet did.
    Jet,
    /// A strike set it off at once.
    Struck,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Ignited { id: u32, center: Vec3, cause: Cause },
    /// An HE shell's filler detonated (its energy is in the gas; its charge is lit).
    Detonated { id: u32, center: Vec3, filler_kg: f32, cause: Cause },
    /// The `index`th opening's latch gave.
    HatchBlown { index: usize },
    TurretLaunched { speed_mps: f32, gauge_pa: f32 },
}

#[derive(Clone, Copy, Debug)]
struct Round {
    center: Vec3,
    propellant_kg: f32,
    filler_kg: f32,
    wall_k: f32,
    /// The HE shell body's temperature (K).
    shell_k: f32,
    flame_flux: f32,
    /// Share of its propellant burnt, once it has ignited.
    burnt: Option<f32>,
    detonated: bool,
}

impl Round {
    fn high_explosive(&self) -> bool {
        self.filler_kg >= HE_MIN_FILLER_KG
    }
}

impl Round {
    fn burning(&self) -> bool {
        self.burnt.is_some_and(|b| b < 1.0)
    }
}

/// The turret's lift off its ring.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lift {
    pub z_m: f32,
    pub v_mps: f32,
    pub launched: bool,
}

#[derive(Clone, Debug)]
pub struct CabinHeat {
    mass_kg: f32,
    energy_j: f32,
    rounds: BTreeMap<u32, Round>,
    lift: Lift,
    peak_gauge_pa: f32,
    /// Openings (by index) whose latches the pressure has beaten.
    blown: std::collections::BTreeSet<usize>,
    /// The thrown turret's height above its ring while it is still over it (the caller's).
    thrown_height_m: Option<f32>,
    /// Momentum the leaving gas has given the thrown turret since it was last taken (N·s).
    jet_impulse_ns: f32,
}

impl CabinHeat {
    pub fn new(ambient_k: f32) -> Self {
        let mass_kg = P_ATM * VOLUME_M3 / (R_AIR * ambient_k);
        Self { mass_kg, energy_j: mass_kg * CV * ambient_k, rounds: BTreeMap::new(), lift: Lift::default(), peak_gauge_pa: 0.0,
            blown: Default::default(), thrown_height_m: None, jet_impulse_ns: 0.0 }
    }

    pub fn gas_k(&self) -> f32 {
        self.energy_j / (self.mass_kg * CV)
    }

    pub fn gauge_pa(&self) -> f32 {
        self.mass_kg * R_AIR * self.gas_k() / VOLUME_M3 - P_ATM
    }

    #[cfg(test)]
    pub fn peak_gauge_pa(&self) -> f32 {
        self.peak_gauge_pa
    }

    #[cfg(test)]
    pub fn lift(&self) -> Lift {
        self.lift
    }

    /// The thrown turret's height above its ring while it is over it (`None` once it is clear).
    pub fn set_thrown_height(&mut self, height_m: Option<f32>) {
        self.thrown_height_m = height_m;
    }

    /// The gas's push on the thrown turret since the last call (N·s, up along the ring's axis).
    pub fn take_jet_impulse(&mut self) -> f32 {
        std::mem::take(&mut self.jet_impulse_ns)
    }

    pub fn wall_k(&self, id: u32) -> Option<f32> {
        self.rounds.get(&id).map(|r| r.wall_k)
    }

    pub fn hottest_wall_k(&self) -> Option<f32> {
        self.rounds.values().filter(|r| r.burnt.is_none()).map(|r| r.wall_k).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b))))
    }

    /// Rounds burning now, and rounds that have ignited at all.
    pub fn burning(&self) -> usize {
        self.rounds.values().filter(|r| r.burning()).count()
    }

    pub fn ignited(&self) -> usize {
        self.rounds.values().filter(|r| r.burnt.is_some()).count()
    }

    #[cfg(test)]
    /// Whether the round has ignited (burning or burnt out).
    pub fn has_ignited(&self, id: u32) -> bool {
        self.rounds.get(&id).is_some_and(|r| r.burnt.is_some())
    }

    /// Move a cartridge between inventory locations: it keeps its heat.
    pub fn transfer(&mut self, old: u32, new: u32) {
        if let Some(round) = self.rounds.remove(&old) {
            self.rounds.insert(new, round);
        }
    }

    /// Set struck rounds off at once (a direct hit on the stowage): an HE shell's filler
    /// detonates, a charge is lit.
    pub fn strike(&mut self, ids: &[u32]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut detonate = Vec::new();
        for id in ids {
            let Some(round) = self.rounds.get_mut(id) else { continue };
            if round.high_explosive() && !round.detonated {
                detonate.push(*id);
            } else if round.burnt.is_none() {
                round.burnt = Some(0.0);
                events.push(Event::Ignited { id: *id, center: round.center, cause: Cause::Struck });
            }
        }
        self.detonate(detonate, Cause::Struck, &mut events);
        events
    }

    /// Detonate these HE shells and every HE shell within `SYMPATHETIC_M` of one that goes: each
    /// filler's energy and products into the gas at once (the gas held at the flame's temperature
    /// at most; named), its charge lit.
    fn detonate(&mut self, mut queue: Vec<u32>, cause: Cause, events: &mut Vec<Event>) {
        let mut first = true;
        while let Some(id) = queue.pop() {
            let Some(round) = self.rounds.get_mut(&id).filter(|r| r.high_explosive() && !r.detonated) else { continue };
            round.detonated = true;
            let (center, filler) = (round.center, round.filler_kg);
            if round.burnt.is_none() {
                round.burnt = Some(0.0);
            }
            self.mass_kg += filler;
            self.energy_j = (self.energy_j + filler * compartment::TNT_J_PER_KG).min(self.mass_kg * CV * FLAME_K);
            events.push(Event::Detonated { id, center, filler_kg: filler, cause: if first { cause } else { Cause::Jet } });
            first = false;
            queue.extend(self.rounds.iter().filter(|(_, r)| r.high_explosive() && !r.detonated && r.center.distance(center) <= SYMPATHETIC_M)
                .map(|(id, _)| *id));
        }
    }

    /// Take the inventory as it is: new rounds start at the case's ambient (or the gas's, if
    /// cooler), each kept round moves with its identity, and a round no longer present is
    /// dropped unless it is burning (its burst case stays where it was).
    fn sync(&mut self, env: &Env) {
        let present: std::collections::BTreeSet<u32> = env.rounds.iter().map(|r| r.id).collect();
        self.rounds.retain(|id, r| present.contains(id) || r.burnt.is_some());
        let start = env.ambient_k.min(self.gas_k());
        for (i, live) in env.rounds.iter().enumerate() {
            let flux = env.flame_flux_w_m2.get(i).copied().unwrap_or(0.0).max(0.0);
            let round = self.rounds.entry(live.id).or_insert(Round { center: live.center, propellant_kg: live.propellant_kg,
                filler_kg: live.filler_kg, wall_k: start, shell_k: start, flame_flux: 0.0, burnt: None, detonated: false });
            round.center = live.center;
            round.propellant_kg = live.propellant_kg;
            round.filler_kg = live.filler_kg;
            round.flame_flux = flux;
        }
    }

    /// Advance by `dt` (s): fine steps while propellant burns or the pressure is up.
    pub fn advance(&mut self, dt: f32, env: &Env) -> Vec<Event> {
        let mut events = Vec::new();
        if !(dt > 0.0) {
            return events;
        }
        self.sync(env);
        let mut open: Vec<bool> = env.openings.iter().enumerate().map(|(i, o)| o.open || self.blown.contains(&i)).collect();
        let mut left = dt.min(0.25);
        while left > 1.0e-7 {
            let violent = self.rounds.values().any(Round::burning) || self.lift.z_m > 0.0 || self.gauge_pa() > 2_000.0;
            let h = left.min(if violent { 2.5e-4 } else { 0.02 });
            left -= h;
            self.step(h, env, &mut open, &mut events);
        }
        events
    }

    fn step(&mut self, h: f32, env: &Env, open: &mut [bool], events: &mut Vec<Event>) {
        let ambient = env.ambient_k;
        let t = self.gas_k();
        let p = self.mass_kg * R_AIR * t / VOLUME_M3;
        let gauge = p - P_ATM;
        self.peak_gauge_pa = self.peak_gauge_pa.max(gauge);
        for (i, o) in env.openings.iter().enumerate() {
            if o.hatch && !open[i] && gauge * o.area_m2 > compartment::HATCH_LATCH_N {
                open[i] = true;
                self.blown.insert(i);
                events.push(Event::HatchBlown { index: i });
            }
        }
        let opened: f32 = env.openings.iter().zip(open.iter()).filter(|(_, &o)| o).map(|(o, _)| o.area_m2).sum();
        let ring = env.turret.map_or(0.0, |turret| {
            let d = turret.ring_diameter_m;
            if self.lift.launched { std::f32::consts::FRAC_PI_4 * d * d }
            else { std::f32::consts::PI * d * (self.lift.z_m - RING_OVERLAP_M).max(0.0) }
        });
        let area = opened + ring + LEAK_AREA_M2;
        // Burning propellant: each charge's burnt share advances at 1/BURN_S times (p/p_atm)^n.
        let speedup = (p / P_ATM).max(0.2).powf(VIEILLE_N);
        let rate = speedup / BURN_S;
        let mut gas_kg_s = 0.0;
        for round in self.rounds.values_mut() {
            if let Some(burnt) = round.burnt.as_mut().filter(|b| **b < 1.0) {
                let share = (rate * h).min(1.0 - *burnt);
                *burnt += share;
                gas_kg_s += round.propellant_kg * share / h;
            }
        }
        let heat_w = gas_kg_s * PROPELLANT_J_PER_KG + env.fire_w.max(0.0);
        // Air traded through what is open, and the flow the pressure drives.
        let traded = 0.5 * (opened + ring) * VENT_HEIGHT_M.sqrt();
        let out = if gauge > 0.0 { compartment::orifice_mass_flow(p, t, P_ATM, compartment::DISCHARGE * area) } else { 0.0 };
        let inflow = if gauge < 0.0 { compartment::orifice_mass_flow(P_ATM, ambient, p, compartment::DISCHARGE * area) } else { 0.0 };
        let out_kg = (out * h).min(0.5 * self.mass_kg);
        let du = (heat_w + gas_kg_s * CV * ambient - WALL_CONDUCTANCE_W_K * (t - ambient) - traded * CP * (t - ambient)
            + inflow * CP * ambient) * h - out_kg * CP * t;
        self.mass_kg = (self.mass_kg + gas_kg_s * h + inflow * h - out_kg).max(0.05);
        self.energy_j = (self.energy_j + du).clamp(self.mass_kg * CV * 200.0, self.mass_kg * CV * FLAME_K);
        // The thrown turret, while it is over the open ring.
        if let (Some(turret), Some(height)) = (env.turret.filter(|_| self.lift.launched), self.thrown_height_m) {
            let d = turret.ring_diameter_m;
            let ring_area = std::f32::consts::FRAC_PI_4 * d * d;
            self.jet_impulse_ns += self.gauge_pa().max(0.0) * ring_area * (d / (d + 2.0 * height.max(0.0))).powi(2) * h;
        }
        // The turret on its ring.
        if let Some(turret) = env.turret.filter(|_| !self.lift.launched) {
            let ring_area = std::f32::consts::FRAC_PI_4 * turret.ring_diameter_m * turret.ring_diameter_m;
            let force = self.gauge_pa().max(0.0) * ring_area - turret.mass_kg * GRAVITY;
            if self.lift.z_m > 0.0 || force > 0.0 {
                self.lift.v_mps += force / turret.mass_kg * h;
                self.lift.z_m += self.lift.v_mps * h;
                if self.lift.z_m <= 0.0 {
                    self.lift = Lift::default();
                } else if self.lift.z_m >= RING_OVERLAP_M + 0.25 * turret.ring_diameter_m {
                    self.lift.launched = true;
                    events.push(Event::TurretLaunched { speed_mps: self.lift.v_mps, gauge_pa: self.gauge_pa() });
                }
            }
        }
        // The cases.
        let t = self.gas_k();
        let emissivity = EMISSIVITY_CLEAR + (EMISSIVITY_SOOT - EMISSIVITY_CLEAR) * env.soot.clamp(0.0, 1.0);
        let jets: Vec<Vec3> = self.rounds.values().filter(|r| r.burning()).map(|r| r.center).collect();
        let mut cooked = Vec::new();
        for (id, round) in self.rounds.iter_mut() {
            if round.detonated {
                continue;
            }
            let jet_on = |at: Vec3, own: bool| -> f32 { jets.iter().map(|c| {
                if !own && c.distance(at) < 1.0e-4 { return 0.0; }
                let gap = (c.distance(at) - 2.0 * CASE_RADIUS_M).max(0.0);
                JET_FLUX_W_M2 * (JET_REACH_M / (JET_REACH_M + gap)).powi(2)
            }).sum() };
            if round.burnt.is_none() {
                let tw = round.wall_k;
                let gas = CASE_CONVECTION_W_M2K * (t - tw) + emissivity * STEFAN_BOLTZMANN * (t.powi(4) - tw.powi(4));
                let jet = jet_on(round.center, false);
                round.wall_k = tw + (gas + round.flame_flux + jet) / WALL_J_M2K * h;
                if round.wall_k >= IGNITION_K {
                    round.burnt = Some(0.0);
                    let cause = if jet > gas + round.flame_flux { Cause::Jet } else { Cause::Heat };
                    events.push(Event::Ignited { id: *id, center: round.center, cause });
                }
            }
            if round.high_explosive() {
                // Its body under the same gas and flames, and its own burning charge.
                let ts = round.shell_k;
                let gas = CASE_CONVECTION_W_M2K * (t - ts) + emissivity * STEFAN_BOLTZMANN * (t.powi(4) - ts.powi(4));
                let jet = jet_on(round.center, true);
                round.shell_k = ts + (gas + round.flame_flux + jet) / SHELL_J_M2K * h;
                if round.shell_k >= FILLER_COOKOFF_K {
                    cooked.push(*id);
                }
            }
        }
        if !cooked.is_empty() {
            self.detonate(cooked, Cause::Heat, events);
        }
    }

    pub fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "gas_k": self.gas_k(), "gauge_pa": self.gauge_pa(), "peak_gauge_pa": self.peak_gauge_pa,
            "rounds": self.rounds.len(), "ignited": self.ignited(), "burning": self.burning(),
            "hottest_wall_k": self.hottest_wall_k(), "lift": {"z_m": self.lift.z_m, "v_mps": self.lift.v_mps, "launched": self.lift.launched},
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMBIENT: f32 = 293.0;
    const TIGER: Turret = Turret { mass_kg: 11_000.0, ring_diameter_m: 1.85 };

    /// The Tiger's openings at real size (`game_fea::crew_openings`): four lids and the cupola.
    fn hatches(open: bool) -> Vec<Opening> {
        let names = ["driver's hatch", "loader's hatch", "radio operator's hatch", "turret escape hatch", "commander's cupola"];
        [0.178, 0.140, 0.178, 0.178, 0.159].iter().zip(names)
            .map(|(&area_m2, name)| Opening { name, area_m2, open, hatch: true }).collect()
    }

    /// The game's stowage at real size: two sponson groups of 32 (4 high, 8 long, 0.112 m apart)
    /// 2.5 m apart (`interior_rig::SHELL_SLOTS`, measured: 31 and 33, 0.112 m nearest neighbours).
    fn racks() -> Vec<LiveRound> {
        (0..64u32).map(|i| {
            let side = if i < 32 { -1.25 } else { 1.25 };
            let k = i % 32;
            LiveRound { id: i, center: Vec3::new(side, 1.3 + (k / 8) as f32 * 0.112, -0.4 + (k % 8) as f32 * 0.112), propellant_kg: 2.5, filler_kg: 0.0 }
        }).collect()
    }

    fn run(heat: &mut CabinHeat, seconds: f32, env: &Env) -> Vec<(f32, Event)> {
        let mut log = Vec::new();
        let mut t = 0.0;
        while t < seconds {
            for e in heat.advance(1.0 / 60.0, env) {
                log.push((t, e));
            }
            t += 1.0 / 60.0;
        }
        log
    }

    #[test]
    #[ignore]
    fn print_cookoff_timelines() {
        for (struck, open_hatches) in [(true, false), (true, true), (false, false), (false, true)] {
            let mut heat = CabinHeat::new(AMBIENT);
            let rounds = racks();
            let mut flux = vec![0.0; rounds.len()];
            if !struck { flux[0] = 60_000.0; }
            let openings = hatches(open_hatches);
            let env = Env { ambient_k: AMBIENT, fire_w: if struck { 0.0 } else { 300_000.0 }, soot: if struck { 0.0 } else { 0.5 },
                openings: &openings, rounds: &rounds, flame_flux_w_m2: &flux, turret: Some(TIGER) };
            heat.advance(1.0 / 60.0, &env);
            if struck { heat.strike(&[0]); }
            let log = run(&mut heat, if struck { 4.0 } else { 120.0 }, &env);
            let times: Vec<f32> = log.iter().filter_map(|(t, e)| matches!(e, Event::Ignited { .. }).then_some(*t)).collect();
            let first = if struck { 0.0 } else { times.first().copied().unwrap_or(f32::NAN) };
            let rel: Vec<String> = [1usize, 2, 4, 8, 16, 32, 48, 63].iter().filter_map(|&n| times.get(n.min(times.len().saturating_sub(1)))
                .map(|t| format!("{n}:{:.2}", t - first))).collect();
            let blown: Vec<String> = log.iter().filter_map(|(t, e)| match e { Event::HatchBlown { index } => Some(format!("{index}@{:.2}", t - first)), _ => None }).collect();
            let launch = log.iter().find_map(|(t, e)| match e { Event::TurretLaunched { speed_mps, gauge_pa } =>
                Some(format!("{:.2} s at {speed_mps:.1} m/s ({:.0} kPa)", t - first, gauge_pa / 1000.0)), _ => None });
            println!("struck {struck} open {open_hatches}: first {first:.1} s; ignitions (n: s after first) {rel:?}; total {}; hatches {blown:?}; launch {launch:?}; peak {:.0} kPa",
                times.len() + struck as usize, heat.peak_gauge_pa() / 1000.0);
        }
    }

    #[test]
    fn without_fire_nothing_changes() {
        let mut heat = CabinHeat::new(AMBIENT);
        let rounds = racks();
        let flux = vec![0.0; rounds.len()];
        let openings = hatches(false);
        let env = Env { ambient_k: AMBIENT, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds, flame_flux_w_m2: &flux, turret: Some(TIGER) };
        assert!(run(&mut heat, 60.0, &env).is_empty());
        assert!((heat.gas_k() - AMBIENT).abs() < 0.5 && heat.gauge_pa().abs() < 10.0);
        assert!((heat.hottest_wall_k().unwrap() - AMBIENT).abs() < 0.5);
    }

    #[test]
    fn a_fire_heats_the_air_over_tens_of_seconds_a_round_in_its_flames_cooks_off_and_its_group_follows() {
        let mut heat = CabinHeat::new(AMBIENT);
        let rounds = racks();
        // The first round of the first group sits in a flame (60 kW/m²), the rest see none.
        let mut flux = vec![0.0; rounds.len()];
        flux[0] = 60_000.0;
        let openings = hatches(false);
        let env = Env { ambient_k: AMBIENT, fire_w: 300_000.0, soot: 0.5, openings: &openings, rounds: &rounds, flame_flux_w_m2: &flux, turret: Some(TIGER) };
        run(&mut heat, 5.0, &env);
        let early = heat.gas_k();
        assert!(early > AMBIENT + 20.0 && heat.ignited() == 0, "the air warms, nothing goes yet: {early} K");
        let log = run(&mut heat, 120.0, &env);
        let times: Vec<f32> = log.iter().filter_map(|(t, e)| matches!(e, Event::Ignited { .. }).then_some(*t)).collect();
        let first = times[0];
        assert!((5.0..60.0).contains(&(first + 5.0)), "the round in the flames goes after {} s", first + 5.0);
        assert!(matches!(log.iter().find(|(_, e)| matches!(e, Event::Ignited { .. })).unwrap().1,
            Event::Ignited { id: 0, cause: Cause::Heat, .. }));
        // Its group pops one after another, not all at once.
        let within = times.iter().filter(|&&t| t - first < 2.0).count();
        assert!(within >= 16, "{within} rounds within two seconds");
        assert!(times[8] - first >= 0.2, "the popping is spread: the ninth after {} s", times[8] - first);
        let launch = log.iter().find_map(|(t, e)| match e { Event::TurretLaunched { speed_mps, .. } => Some((*t, *speed_mps)), _ => None })
            .expect("the turret goes");
        assert!(launch.0 - first < 2.5 && launch.1 > 2.0, "the turret goes {} s after the first round at {} m/s", launch.0 - first, launch.1);
    }

    #[test]
    fn a_struck_rack_goes_at_once_and_throws_the_turret() {
        for open_hatches in [false, true] {
            let mut heat = CabinHeat::new(AMBIENT);
            let rounds = racks();
            let flux = vec![0.0; rounds.len()];
            let openings = hatches(open_hatches);
            let env = Env { ambient_k: AMBIENT, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds, flame_flux_w_m2: &flux, turret: Some(TIGER) };
            heat.advance(1.0 / 60.0, &env);
            // The rounds the strike's fragments and flame crossed.
            let struck = heat.strike(&[0, 1, 8, 9]);
            assert_eq!(struck.len(), 4);
            let log = run(&mut heat, 3.0, &env);
            let launch = log.iter().find_map(|(t, e)| match e { Event::TurretLaunched { speed_mps, .. } => Some((*t, *speed_mps)), _ => None })
                .expect("the turret is thrown");
            assert!(launch.0 < 0.8, "at once: {} s", launch.0);
            assert!((3.0..40.0).contains(&launch.1), "thrown at {} m/s", launch.1);
            assert_eq!(log.iter().any(|(_, e)| matches!(e, Event::HatchBlown { .. })), !open_hatches, "closed hatches go too");
            assert!(heat.peak_gauge_pa() > 40_000.0);
        }
    }

    #[test]
    fn a_struck_he_shell_detonates_its_neighbours_and_throws_the_turret_hard() {
        let mut heat = CabinHeat::new(AMBIENT);
        // The first group's rounds alternate PzGr (59 g filler: not counted) and SprGr (0.9 kg).
        let rounds: Vec<LiveRound> = racks().into_iter().map(|r| LiveRound { filler_kg: if r.id % 2 == 0 { 0.9 } else { 0.059 }, ..r }).collect();
        let flux = vec![0.0; rounds.len()];
        let openings = hatches(false);
        let env = Env { ambient_k: AMBIENT, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds, flame_flux_w_m2: &flux, turret: Some(TIGER) };
        heat.advance(1.0 / 60.0, &env);
        let struck = heat.strike(&[0]);
        let detonated: Vec<u32> = struck.iter().filter_map(|e| match e { Event::Detonated { id, .. } => Some(*id), _ => None }).collect();
        assert!(detonated.len() >= 2 && detonated.iter().all(|id| id % 2 == 0), "sympathetic HE only: {detonated:?}");
        let log = run(&mut heat, 2.0, &env);
        let launch = log.iter().find_map(|(t, e)| match e { Event::TurretLaunched { speed_mps, .. } => Some((*t, *speed_mps)), _ => None })
            .expect("the turret is thrown");
        assert!(launch.0 < 0.2 && launch.1 > 6.0, "at once and hard: {launch:?}");
        // An AP round struck alone detonates nothing.
        let mut heat = CabinHeat::new(AMBIENT);
        heat.advance(1.0 / 60.0, &env);
        assert!(heat.strike(&[1]).iter().all(|e| matches!(e, Event::Ignited { .. })));
    }

    #[test]
    fn one_round_burning_under_open_hatches_does_not_throw_the_turret() {
        let mut heat = CabinHeat::new(AMBIENT);
        let rounds = [LiveRound { id: 7, center: Vec3::ZERO, propellant_kg: 2.5, filler_kg: 0.0 }];
        let openings = hatches(true);
        let env = Env { ambient_k: AMBIENT, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds, flame_flux_w_m2: &[0.0], turret: Some(TIGER) };
        heat.advance(1.0 / 60.0, &env);
        heat.strike(&[7]);
        let log = run(&mut heat, 3.0, &env);
        assert!(!log.iter().any(|(_, e)| matches!(e, Event::TurretLaunched { .. })), "peak {} Pa", heat.peak_gauge_pa());
        assert!(heat.gas_k() < 1_500.0, "the gas cools again through the hatches: {} K", heat.gas_k());
    }

    #[test]
    fn a_moved_round_keeps_its_heat_and_an_empty_tank_never_ignites() {
        let mut heat = CabinHeat::new(AMBIENT);
        let rounds = [LiveRound { id: 7, center: Vec3::ZERO, propellant_kg: 2.5, filler_kg: 0.0 }];
        let openings = hatches(false);
        let env = Env { ambient_k: AMBIENT, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds, flame_flux_w_m2: &[50_000.0], turret: None };
        run(&mut heat, 5.0, &env);
        let warm = heat.wall_k(7).unwrap();
        assert!(warm > AMBIENT + 15.0);
        heat.transfer(7, 0x2_0000);
        assert_eq!(heat.wall_k(0x2_0000), Some(warm));
        let empty = Env { rounds: &[], flame_flux_w_m2: &[], fire_w: 2.0e6, ..env };
        assert!(run(&mut heat, 30.0, &empty).iter().all(|(_, e)| !matches!(e, Event::Ignited { .. })));
        assert!(heat.gas_k() > 500.0, "the air still heats: {} K", heat.gas_k());
    }
}
