//! The Tiger's cook-off in play (2026-11-02, the owner; `crate::cabin_heat`). The compartment's
//! heat is fed by the burning surfaces and the live rounds; the first round to ignite is the first
//! bang (the crew inside die in its flame); under the owner's whole-load rule the rest follow
//! within a tenth of a second (`cabin_heat::MASS_SPREAD_S`), each its own bang and flash and a
//! flame from every opening; the pressure blows the latched hatches and throws the turret as a
//! rigid body (`wreck_turret::Body`), carrying everything authored in its frame with it
//! (`Tank::turret_thrown`); the dead in the turret are left to fall into the hull. A direct hit on
//! the stowage sets the rounds it crossed off at once. The turreted enemies' whole loads are
//! solved once per kind (`enemy_cookoff_throw`).
use super::*;
use crate::cabin_heat::{self, CabinHeat, Event as HeatEvent};

/// The Tiger I's turret with its gun (kg) and its ring's diameter (m), real size.
const TURRET_MASS_KG: f32 = 11_000.0;
const TURRET_RING_M: f32 = 1.85;
/// Heat release of one burning surface patch at full intensity (W): a patch of about 0.12 m of
/// burning oil, paint and stowage at about 1 MW/m² (authored).
const PATCH_HEAT_W: f32 = 45_000.0;
/// The share of an engine-bay fire's heat that reaches the crew compartments' air through the
/// firewall (authored).
const FIREWALL_SHARE: f32 = 0.25;
/// A flame's flux on a case it touches at full intensity (W/m²), falling as (r/(r + d))² with
/// its distance d (authored: flames impinging on a surface, about 50-100 kW/m²); flames farther
/// than `FLAME_SIGHT_M` or hidden from the case by the tank's own surfaces give none.
const FLAME_FLUX_W_M2: f32 = 80_000.0;
const FLAME_REACH_M: f32 = 0.2;
const FLAME_SIGHT_M: f32 = 1.5;
/// How often each round's flame flux is re-measured (s).
const FLUX_INTERVAL_S: f32 = 0.2;
/// The turret leans off the vertical toward the side the stowage burnt first, at this share of
/// its speed, and turns at this rate per metre of that offset (authored).
const THROW_LEAN: f32 = 0.18;
const THROW_SPIN_PER_M: f32 = 0.9;
/// The outside shot's vertical field of view (degrees).
pub(super) const COOKOFF_OUTSIDE_FOV: f32 = 42.0;
/// Seconds of the thrown turret's path kept for the death replay.
const PATH_KEEP_S: f32 = 15.0;
/// The whole load's rounds go within a tenth of a second: the compartment's flash and each
/// opening's jet are one through them, issued at most this often (s; every round keeps its own
/// report).
const POP_INTERVAL_S: f32 = 0.12;

/// A cook-off death's shots (seconds; the owner, 2026-11-02): black from the bang while it rings,
/// the tank from outside as the stowage goes and the turret is thrown, the dead from above.
pub const COOKOFF_BLACK_S: f32 = 3.5;
pub const COOKOFF_THROW_S: f32 = 7.0;
pub const COOKOFF_CREW_S: f32 = 8.0;

/// The 8.8 cm cartridge's propellant (kg; `sound_levels::propellant_charge_kg`, 2.5 kg).
fn charge_kg() -> f32 {
    crate::sound_levels::propellant_charge_kg(88.0, 10.2, 773.0)
}

#[derive(Default, Clone)]
pub(super) struct Cookoff {
    heat: Option<CabinHeat>,
    flux: Vec<(u32, f32)>,
    flux_clock_s: f32,
    /// Seconds since this tank's cook-off state began (the clock every time below is on).
    pub clock_s: f32,
    /// When the first round went, where (world), and when the man inside heard it.
    pub first_s: Option<f32>,
    pub first_at: Option<Vec3>,
    pub black_from_s: Option<f32>,
    pub launched_s: Option<f32>,
    pub turret: Option<wreck_turret::Body>,
    /// The thrown turret's frame (world, with the frame's scale) by seconds since its launch.
    pub path: Vec<(f32, Mat4)>,
    /// Shot holes through the crew compartments' walls (m², real size).
    pub holes_m2: f32,
    pub ignitions: u32,
    pub detonations: u32,
    /// What has gone so far: burning mass for the fireball (kg; filler at its energy in
    /// propellant) and the TNT equivalent for the pressure front (kg).
    pub fuel_kg: f32,
    pub tnt_kg: f32,
    /// The burning mass of the whole load aboard (what has gone and what is still live): what
    /// goes under the owner's rule once one round does, so the outside shot stands clear of its
    /// fireball from its first frame.
    pub load_fuel_kg: f32,
    /// The height above the ring the outside shot frames (eased toward the thrown turret's).
    framed_apex_m: std::cell::Cell<(f32, f32)>,
    /// The death's last shot's camera, chosen once (eye, target, field of view).
    crew_camera: std::cell::Cell<Option<(Vec3, Vec3, f32)>>,
    /// The player's Tiger at real size for the thrown body (built once, on a worker at start).
    pub source: Option<PendingSource>,
    /// When the compartment last flashed and each opening last vented (the five hatches, then
    /// the open ring), on the cook-off clock (`POP_INTERVAL_S`).
    popped_s: [Option<f32>; 7],
}

/// The thrown turret's source, filled by a worker thread at start (it takes about two seconds).
pub(super) type PendingSource = std::sync::Arc<std::sync::OnceLock<std::sync::Arc<wreck_turret::Source>>>;

/// Support samples are kept one per cube of this size on the player's detailed turret (m).
const SUPPORT_CELL_M: f32 = 0.12;

impl Cookoff {
    pub(super) fn with_source(source: PendingSource) -> Self {
        Self { source: Some(source), ..Self::default() }
    }

    /// How near the hottest live case is to its propellant's ignition, 0 (ambient) to 1.
    pub(super) fn ammo_urgency(&self) -> f32 {
        let ambient = crate::fea_damage::materials::AMBIENT_K;
        self.heat.as_ref().and_then(CabinHeat::hottest_wall_k)
            .map_or(0.0, |k| ((k - ambient) / (cabin_heat::IGNITION_K - ambient)).clamp(0.0, 1.0))
    }

    /// A fresh tank (a new match, a remounted spare): cold, its turret on its ring.
    pub(super) fn reset(&mut self) {
        let source = self.source.take();
        *self = Self { source, ..Self::default() };
    }

    /// Whether the flash (slot 0) or an opening's jet (1-5 the hatches, 6 the ring) is due on a
    /// pop now; marks it issued if so.
    fn pop_due(&mut self, slot: usize) -> bool {
        let now = self.clock_s;
        let due = self.popped_s[slot].is_none_or(|last| now - last >= POP_INTERVAL_S);
        if due { self.popped_s[slot] = Some(now); }
        due
    }

    /// A cartridge moved between inventory locations keeps its heat.
    pub(super) fn transfer(&mut self, old: u32, new: u32) {
        if let Some(heat) = self.heat.as_mut() {
            heat.transfer(old, new);
        }
    }

    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "heat": self.heat.as_ref().map(CabinHeat::receipt),
            "first_s": self.first_s, "launched_s": self.launched_s, "ignitions": self.ignitions, "detonations": self.detonations,
            "turret": self.turret.as_ref().map(wreck_turret::Body::report),
            "apex_m": self.path.iter().map(|(_, m)| m.w_axis.y).fold(None, |a: Option<f32>, y| Some(a.map_or(y, |a| a.max(y))))
                .zip(self.path.first().map(|(_, m)| m.w_axis.y)).map(|(top, start)| top - start),
        })
    }
}

/// A turreted enemy's stowage and turret for its whole-load cook-off (authored estimates, named:
/// not measured vehicles): the turret with its gun (kg), its ring (m), the compartments' free air
/// (m³), the rounds aboard (a full load; its own firing is not counted, named), each round's
/// propellant (kg), the share that are HE and their filler (kg), the other rounds' filler (kg).
#[derive(Clone, Copy, Debug)]
struct EnemyLoad {
    turret_kg: f32,
    ring_m: f32,
    air_m3: f32,
    rounds: u32,
    propellant_kg: f32,
    he_share: f32,
    he_filler_kg: f32,
    other_filler_kg: f32,
}

fn enemy_load(kind: EnemyKind) -> Option<EnemyLoad> {
    match kind {
        // T-34/76: 77 rounds of 76.2 mm, 1.08 kg charges, two thirds OF-350 (0.71 kg).
        EnemyKind::T34 => Some(EnemyLoad { turret_kg: 4_000.0, ring_m: 1.42, air_m3: 5.0, rounds: 77, propellant_kg: 1.08,
            he_share: 0.67, he_filler_kg: 0.71, other_filler_kg: 0.15 }),
        // KV-2: 36 rounds of 152 mm with separate charges of about 2.5 kg, OF-530 (5.9 kg).
        EnemyKind::Kv2 => Some(EnemyLoad { turret_kg: 12_000.0, ring_m: 1.535, air_m3: 9.0, rounds: 36, propellant_kg: 2.5,
            he_share: 0.8, he_filler_kg: 5.9, other_filler_kg: 0.2 }),
        // IS-2 and IS-3: 28 rounds of 122 mm with separate charges of about 3.6 kg, OF-471 (3.6 kg).
        EnemyKind::Is2 | EnemyKind::Is3 => Some(EnemyLoad { turret_kg: 9_000.0, ring_m: 1.8, air_m3: 8.0, rounds: 28,
            propellant_kg: 3.6, he_share: 0.6, he_filler_kg: 3.6, other_filler_kg: 0.16 }),
        // The casemates have no turret to throw.
        EnemyKind::Su100 | EnemyKind::Isu152 => None,
    }
}

/// The fireball's diameter (m) and lifetime (s) for `fuel_kg` of burning propellant (and filler at
/// its energy in propellant): the published fireball correlations D = 5.8·M^(1/3), t = 0.45·M^(1/3)
/// (Roberts 1982; CCPS), taken for a propellant fireball (named: they are fuel-fireball fits).
pub(super) fn fireball(fuel_kg: f32) -> (f32, f32) {
    let m = fuel_kg.max(0.0).cbrt();
    (5.8 * m, 0.45 * m)
}

/// Its diameter against the 88 mm burst's authored fireball (about 1.5 m): the raymarch's scale.
pub(super) fn fireball_scale(fuel_kg: f32) -> f32 {
    (fireball(fuel_kg).0 / 1.5).max(1.0)
}

/// The whole load set off at once in `kind`'s own compartment (`cabin_heat`, every round within
/// `MASS_SPREAD_S`): the speed its turret leaves at, the fireball's burning mass (kg) and the
/// load's TNT equivalent for the blast front (kg; fillers and 0.36 a kilogram of propellant).
/// Solved once per kind and kept.
pub(super) fn enemy_cookoff_throw(kind: EnemyKind) -> Option<(f32, f32, f32)> {
    static SOLVED: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<u8, Option<(f32, f32, f32)>>>> = std::sync::OnceLock::new();
    let key = kind as u8;
    let cache = SOLVED.get_or_init(Default::default);
    if let Some(known) = cache.lock().ok().and_then(|c| c.get(&key).copied()) {
        return known;
    }
    let solved = enemy_load(kind).map(|load| {
        let ambient = crate::fea_damage::materials::AMBIENT_K;
        let mut heat = CabinHeat::with_volume(ambient, load.air_m3);
        let rounds: Vec<cabin_heat::LiveRound> = (0..load.rounds).map(|i| {
            let he = ((i as f32 + 0.5) / load.rounds as f32) < load.he_share;
            cabin_heat::LiveRound { id: i, center: Vec3::new((i % 11) as f32 * 0.11, 0.3, (i / 11) as f32 * 0.1),
                propellant_kg: load.propellant_kg, filler_kg: if he { load.he_filler_kg } else { load.other_filler_kg } }
        }).collect();
        let openings: Vec<crate::fea_damage::compartment::Opening> = ["driver's hatch", "turret hatch", "second hatch"].iter()
            .map(|&name| crate::fea_damage::compartment::Opening { name, area_m2: 0.15, open: false, hatch: true }).collect();
        let flux = vec![0.0; rounds.len()];
        let env = cabin_heat::Env { ambient_k: ambient, fire_w: 0.0, soot: 0.0, openings: &openings, rounds: &rounds,
            flame_flux_w_m2: &flux, turret: Some(cabin_heat::Turret { mass_kg: load.turret_kg, ring_diameter_m: load.ring_m }) };
        heat.advance(1.0e-4, &env);
        heat.strike(&[0]);
        let mut speed = 0.0;
        for _ in 0..480 {
            if let Some(v) = heat.advance(1.0 / 240.0, &env).iter().find_map(|e| match e {
                HeatEvent::TurretLaunched { speed_mps, .. } => Some(*speed_mps), _ => None }) {
                speed = v;
                break;
            }
        }
        let he = (load.rounds as f32 * load.he_share).round();
        let fillers = he * load.he_filler_kg + (load.rounds as f32 - he) * load.other_filler_kg;
        let propellant = load.rounds as f32 * load.propellant_kg;
        let fuel = propellant + fillers * crate::fea_damage::compartment::TNT_J_PER_KG / cabin_heat::PROPELLANT_J_PER_KG;
        let tnt = fillers + crate::sound_levels::TNT_PER_KG_PROPELLANT * propellant;
        println!("[cookoff] {}: the whole load ({} rounds) throws its {:.0} t turret at {speed:.1} m/s (about {:.0} m up); fireball {:.0} m",
            kind.name(), load.rounds, load.turret_kg / 1000.0, speed * speed / 19.62, fireball(fuel).0);
        (speed, fuel, tnt)
    });
    if let Ok(mut c) = cache.lock() {
        c.insert(key, solved);
    }
    solved
}

/// The player's Tiger meshes (hull, turret, gun) at real size for the thrown turret, prepared on a
/// worker thread.
pub(super) fn player_turret_source(meshes: &[CpuMesh; 3], frame_scale: f32) -> PendingSource {
    let pending: PendingSource = Default::default();
    let fill = pending.clone();
    let meshes = meshes.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let scaled = meshes.map(|mut mesh| {
            for v in &mut mesh.vertices {
                v.pos = (Vec3::from(v.pos) * frame_scale).to_array();
            }
            mesh.min *= frame_scale;
            mesh.max *= frame_scale;
            mesh
        });
        let source = wreck_turret::Source::new(&scaled, crate::ballistics::ArmorScheme::TigerI).decimated(SUPPORT_CELL_M);
        println!("[cookoff] the Tiger's turret as a rigid body prepared in {:.0} ms ({} support samples)",
            started.elapsed().as_secs_f32() * 1000.0, source.support_samples());
        let _ = fill.set(std::sync::Arc::new(source));
    });
    pending
}

impl Game {
    /// Advance the compartment's heat by `dt` with the live rounds (hull frame) and act on what it
    /// reports. Called from `update_fire` every update, the man inside alive or not.
    pub(super) fn update_cookoff(&mut self, dt: f32, rounds: &[LiveAmmoRound]) {
        self.cookoff.clock_s += dt.max(0.0);
        // An invincible Tiger: the fire burns, the ammunition never goes (as before the heat
        // model). A chain already going goes on; a dead tank's stowage still cooks off.
        if self.god && self.cookoff.first_s.is_none() {
            self.advance_thrown_turret(dt);
            return;
        }
        let models = self.fire_surface_models();
        let hull_inverse = models[0].inverse();
        let patches: Vec<(Vec3, f32)> = (0..surface_fire::MAX_PATCHES)
            .filter_map(|i| self.cabin_fire.patch_world(i, &models)).filter(|(_, power)| *power > 0.02).collect();
        // Flames in the crew compartments' air heat it fully; a fire behind the firewall (the engine
        // bay's petrol and oil) at `FIREWALL_SHARE` of its release, the steel bulkhead conducting
        // and radiating it (authored; the bulkhead's temperature is not solved).
        let fire_w: f32 = patches.iter().map(|(p, power)| {
            let share = if game_fea::in_crew_air(ArmorPart::Hull, hull_inverse.transform_point3(*p)) { 1.0 } else { FIREWALL_SHARE };
            share * power * PATCH_HEAT_W
        }).sum();
        // Each round's flux from the flames in sight of it, re-measured every FLUX_INTERVAL_S.
        self.cookoff.flux_clock_s -= dt;
        let ids_changed = self.cookoff.flux.len() != rounds.len()
            || self.cookoff.flux.iter().zip(rounds).any(|((id, _), r)| *id != r.stable_id);
        if self.cookoff.flux_clock_s <= 0.0 || ids_changed {
            self.cookoff.flux_clock_s = FLUX_INTERVAL_S;
            self.cookoff.flux = rounds.iter().map(|round| {
                let flux: f32 = patches.iter().filter_map(|&(at, power)| {
                    let local = hull_inverse.transform_point3(at);
                    let surface = closest_round_surface(round, local);
                    let d = surface.distance(local);
                    if d > FLAME_SIGHT_M {
                        return None;
                    }
                    let to = models[0].transform_point3(surface);
                    let clear = self.cabin_fire.sweep(at, to, &models)
                        .is_none_or(|hit| models[hit.frame].transform_point3(hit.point).distance(to) < 0.03);
                    clear.then(|| FLAME_FLUX_W_M2 * power.min(1.0) * (FLAME_REACH_M / (FLAME_REACH_M + d)).powi(2))
                }).sum();
                (round.stable_id, flux)
            }).collect();
        }
        let live: Vec<cabin_heat::LiveRound> = rounds.iter()
            .map(|r| cabin_heat::LiveRound { id: r.stable_id, center: r.center, propellant_kg: charge_kg(),
                filler_kg: r.explosive_energy_j / crate::fea_damage::compartment::TNT_J_PER_KG }).collect();
        self.cookoff.load_fuel_kg = self.cookoff.fuel_kg + live.iter().map(|r| r.propellant_kg
            + r.filler_kg * crate::fea_damage::compartment::TNT_J_PER_KG / cabin_heat::PROPELLANT_J_PER_KG).sum::<f32>();
        let flux: Vec<f32> = self.cookoff.flux.iter().map(|(_, f)| *f).collect();
        let openings = self.fea_crew_openings(self.cookoff.holes_m2);
        let ambient = crate::fea_damage::materials::AMBIENT_K;
        let turret = (!self.player.kind.casemate())
            .then_some(cabin_heat::Turret { mass_kg: TURRET_MASS_KG, ring_diameter_m: TURRET_RING_M });
        let env = cabin_heat::Env { ambient_k: ambient, fire_w, soot: self.interior_soot, openings: &openings,
            rounds: &live, flame_flux_w_m2: &flux, turret };
        // A thrown turret still over its ring takes the push of the gas leaving through it.
        let over_ring = self.cookoff.turret.as_ref().and_then(|body| {
            let hull = self.player.hull_model();
            let ring = hull.transform_point3(self.player.parts.turret_pivot);
            let up = hull.transform_vector3(Vec3::Y).normalize();
            let off = body.model().transform_point3(Vec3::ZERO) - ring;
            let height = off.dot(up);
            ((off - up * height).length() < 0.5 * TURRET_RING_M && height > -0.5).then_some(height.max(0.0))
        });
        let heat = self.cookoff.heat.get_or_insert_with(|| CabinHeat::new(ambient));
        heat.set_thrown_height(over_ring);
        let events = heat.advance(dt, &env);
        let push_ns = heat.take_jet_impulse();
        self.cookoff_events(events);
        // A push too small to lift it (under 2 cm/s this update) leaves a turret at rest asleep.
        let kick = push_ns / TURRET_MASS_KG;
        if let Some(body) = self.cookoff.turret.as_mut().filter(|b| kick > 0.0 && (!b.settled || kick > 0.02)) {
            let up = self.player.hull_model().transform_vector3(Vec3::Y).normalize();
            body.velocity += up * kick;
            body.settled = false;
        }
        self.advance_thrown_turret(dt);
    }

    /// A strike crossed these rounds: they go at once.
    pub(super) fn cookoff_strike(&mut self, ids: &[u32]) {
        let ambient = crate::fea_damage::materials::AMBIENT_K;
        let rounds = self.live_ammunition_in_frame(BloodFrame::Hull);
        // The heat model needs the rounds it is told to strike.
        if self.cookoff.heat.is_none() || ids.iter().any(|id| self.cookoff.heat.as_ref().is_some_and(|h| h.wall_k(*id).is_none())) {
            let live: Vec<cabin_heat::LiveRound> = rounds.iter()
                .map(|r| cabin_heat::LiveRound { id: r.stable_id, center: r.center, propellant_kg: charge_kg(),
                filler_kg: r.explosive_energy_j / crate::fea_damage::compartment::TNT_J_PER_KG }).collect();
            let flux = vec![0.0; live.len()];
            let openings = self.fea_crew_openings(self.cookoff.holes_m2);
            let env = cabin_heat::Env { ambient_k: ambient, fire_w: 0.0, soot: self.interior_soot, openings: &openings,
                rounds: &live, flame_flux_w_m2: &flux, turret: None };
            self.cookoff.heat.get_or_insert_with(|| CabinHeat::new(ambient)).advance(1.0e-4, &env);
        }
        let events = self.cookoff.heat.as_mut().map_or_else(Vec::new, |heat| heat.strike(ids));
        self.cookoff_events(events);
    }

    /// Rounds that go in the same update are one volley (the owner, 2026-11-03: a full load need
    /// not be presented shell by shell): each round's heat, fuel and stowage are its own, but the
    /// volley has one bang of their summed TNT, one flash and one log line, at their TNT-weighted
    /// centre.
    fn cookoff_events(&mut self, events: Vec<HeatEvent>) {
        let hull = self.player.hull_model();
        // (id, place, TNT kg, detonated, cause) of every round going now.
        let mut volley: Vec<(u32, Vec3, f32, bool, cabin_heat::Cause)> = Vec::new();
        for event in events {
            match event {
                HeatEvent::Ignited { id, center, cause } => {
                    self.cookoff.ignitions += 1;
                    self.cookoff.fuel_kg += charge_kg();
                    let tnt_kg = crate::sound_levels::TNT_PER_KG_PROPELLANT * charge_kg();
                    self.cookoff.tnt_kg += tnt_kg;
                    self.consume_live_round(id);
                    volley.push((id, hull.transform_point3(center), tnt_kg, false, cause));
                }
                HeatEvent::Detonated { id, center, filler_kg, cause } => {
                    self.cookoff.ignitions += 1;
                    self.cookoff.detonations += 1;
                    self.cookoff.fuel_kg += charge_kg() + filler_kg * crate::fea_damage::compartment::TNT_J_PER_KG / cabin_heat::PROPELLANT_J_PER_KG;
                    let tnt_kg = filler_kg + crate::sound_levels::TNT_PER_KG_PROPELLANT * charge_kg();
                    self.cookoff.tnt_kg += tnt_kg;
                    self.consume_live_round(id);
                    volley.push((id, hull.transform_point3(center), tnt_kg, true, cause));
                }
                HeatEvent::HatchBlown { index } => {
                    match index {
                        0..=3 => self.roof_hatches.open[index] = true,
                        4 => self.hatch_open = true,
                        _ => {}
                    }
                    println!("[cookoff] the pressure threw hatch {index} open at {:.0} kPa",
                        self.cookoff.heat.as_ref().map_or(0.0, |h| h.gauge_pa() / 1000.0));
                }
                HeatEvent::TurretLaunched { speed_mps, gauge_pa } => self.throw_player_turret(speed_mps, gauge_pa),
            }
        }
        let Some(&(first_id, _, _, _, first_cause)) = volley.first() else { return };
        let tnt_kg: f32 = volley.iter().map(|v| v.2).sum();
        let at = volley.iter().map(|v| v.1 * v.2).sum::<Vec3>() / tnt_kg.max(1e-9);
        let detonated = volley.iter().filter(|v| v.3).count();
        let gas_k = self.cookoff.heat.as_ref().map_or(0.0, CabinHeat::gas_k);
        let gauge_kpa = self.cookoff.heat.as_ref().map_or(0.0, |h| h.gauge_pa() / 1000.0);
        if volley.len() == 1 {
            println!("[cookoff] round {first_id:#x} ({first_cause:?}) {} at {:.2} s: {tnt_kg:.2} kg TNT, gas {gas_k:.0} K, {gauge_kpa:.0} kPa",
                if detonated > 0 { "detonates" } else { "ignites" }, self.cookoff.clock_s);
        } else {
            println!("[cookoff] {} rounds go together at {:.2} s ({detonated} filled shells detonate): {tnt_kg:.2} kg TNT, gas {gas_k:.0} K, {gauge_kpa:.0} kPa",
                volley.len(), self.cookoff.clock_s);
        }
        if self.cookoff.first_s.is_none() {
            self.cookoff.first_s = Some(self.cookoff.clock_s);
            self.cookoff.first_at = Some(at);
            self.cookoff_first_bang(first_id, at, first_cause, tnt_kg);
        } else {
            self.cookoff_pop(at, tnt_kg, detonated > 0);
        }
    }

    /// The first round's propellant goes: the bang, its flash and the compartment's flame. The
    /// crew inside die in it (the compartment's gas at the propellant's flame: named, every man
    /// inside, not solved per body); the blast that throws them is the charge's TNT equivalent.
    fn cookoff_first_bang(&mut self, id: u32, at: Vec3, cause: cabin_heat::Cause, tnt_kg: f32) {
        self.schedule_cookoff_bang(crate::audio::Sfx::KillExplosion, at, tnt_kg);
        self.spawn_interior_blast(at, 0.65, 3.0, 0.11);
        self.fire_active = true;
        self.fire = FIRE_MAX;
        self.ammo_fire.latch(id);
        if !self.player.alive || self.god || !self.tiger_accepts_damage() {
            return;
        }
        let inside = self.mode == Mode::Tank;
        let blast = InternalAmmoBlast { origin_world: at, explosive_energy_j: tnt_kg * 4_184_000.0,
            seed: self.kill_tick ^ id.rotate_left(7) ^ 0xC00C_0FF5 };
        self.player.alive = false;
        self.kill_all_couch_crew_from_blast(blast, BloodFrame::Hull, &[]);
        cold_recommission::mark_legacy_catastrophic_crew(self.singleplayer_crew_active, self.couch_crew_active, &mut self.crew);
        self.recompute_hp();
        self.cue_engine_death(0.7);
        let death = if cause == cabin_heat::Cause::Struck { DeathCause::AmmoDetonation } else { DeathCause::FuelFire };
        self.death_cause = Some(death);
        self.death_replay = None;
        self.last_fatal = Some(FatalHit {
            shooter_pos: self.station_eye(self.station),
            shell_dir: Vec3::ZERO,
            entry_point: at,
            spall_dir: Vec3::ZERO,
            hit_body: true,
            hit_modules: vec![ModuleKind::AmmoRack],
            cause: death,
            fragment_frame: BloodFrame::Hull,
            fragments: Vec::new(),
            fragment_contacts: Vec::new(),
            struck_at_s: None,
        });
        // The man inside: black the instant the bang reaches him (the presentation reads this).
        if inside {
            self.cookoff.black_from_s = Some(self.cookoff.clock_s + at.distance(self.threat_center()) / speed_of_sound());
        }
        println!("[cookoff] the ammunition is burning: the crew inside are dead ({:?})", death);
    }

    /// A later round: its bang and flash (a detonating filler's the larger), and the flame from
    /// every opening.
    fn cookoff_pop(&mut self, at: Vec3, tnt_kg: f32, detonation: bool) {
        self.schedule_cookoff_bang(if detonation { crate::audio::Sfx::KillExplosion } else { crate::audio::Sfx::CookOff }, at, tnt_kg);
        if self.cookoff.pop_due(0) {
            if detonation { self.spawn_interior_blast(at, 0.65, 3.0, 0.11); } else { self.spawn_interior_blast(at, 0.45, 2.0, 0.08); }
        }
        let strength = self.cookoff.heat.as_ref().map_or(0.5, |h| (h.gauge_pa() / 100_000.0).clamp(0.3, 1.0));
        for index in 0..5 {
            if !self.access_hatch_open(index) && !(index < 4 && self.roof_hatches.open[index]) && !(index == 4 && self.hatch_open) {
                continue;
            }
            if !self.cookoff.pop_due(1 + index) {
                continue;
            }
            let frame = self.access_hatch_frame(index);
            let normal = if index < 4 { tiger_hatches::APERTURE_NORMALS[index] } else { Vec3::Y };
            let normal = frame.transform_vector3(normal).normalize_or_zero();
            self.weapon_vfx.pressure_vent(self.access_hatch_center(index), normal, strength);
        }
        if let Some(launched) = self.cookoff.launched_s.filter(|_| self.cookoff.turret.is_some()) {
            // Through the open ring once the turret has gone.
            let ring = self.player.hull_model().transform_point3(self.player.parts.turret_pivot);
            if self.cookoff.clock_s - launched < 30.0 && self.cookoff.pop_due(6) {
                self.weapon_vfx.pressure_vent(ring, self.player.hull_model().transform_vector3(Vec3::Y).normalize(), strength);
            }
        }
    }

    fn schedule_cookoff_bang(&mut self, sfx: crate::audio::Sfx, at: Vec3, tnt_kg: f32) {
        self.scheduled.push(ScheduledSound {
            delay: at.distance(self.threat_center()) / speed_of_sound(),
            sfx, emission: crate::sound_propagation::Emission::blast(tnt_kg), muzzle: None, source: at,
            source_in_player: true, buf: None,
            wave: Some(AcousticWave { emitted_at_s: self.enemy_report_audit.clock_s, speed_m_s: speed_of_sound() }),
            enemy_report: None,
        });
    }

    /// The pressure has thrown the turret: a rigid body from its ring, mostly up, leaning away
    /// from where the stowage burnt first; the dead in its frame are left to fall into the hull.
    fn throw_player_turret(&mut self, speed_mps: f32, gauge_pa: f32) {
        // The worker had seconds of play to prepare it; wait for it if it has not finished.
        let waited = std::time::Instant::now();
        let source = loop {
            match self.cookoff.source.as_ref().map(|pending| pending.get().cloned()) {
                Some(Some(source)) => break Some(source),
                Some(None) if waited.elapsed().as_secs_f32() < 10.0 => std::thread::sleep(std::time::Duration::from_millis(5)),
                _ => break None,
            }
        };
        let Some(source) = source else {
            println!("[cookoff] the turret's rigid body was not prepared: it stays on its ring (named)");
            return;
        };
        let scale = self.player.parts.frame_scale;
        let turret = self.player.turret_model();
        let ring = turret * Mat4::from_scale(Vec3::splat(1.0 / scale));
        let gun_local = Mat4::from_scale(Vec3::splat(scale)) * turret.inverse() * self.player.visual_gun_model()
            * Mat4::from_scale(Vec3::splat(1.0 / scale));
        let up = self.player.hull_model().transform_vector3(Vec3::Y).normalize();
        let ring_at = ring.transform_point3(Vec3::ZERO);
        let away = self.cookoff.first_at.map_or(Vec3::ZERO, |first| {
            let off = ring_at - first;
            (off - up * off.dot(up)).normalize_or_zero()
        });
        let velocity = (up + away * THROW_LEAN).normalize() * speed_mps;
        let offset = self.cookoff.first_at.map_or(0.0, |first| {
            let off = ring_at - first;
            (off - up * off.dot(up)).length()
        });
        let spin = up.cross(away) * (THROW_SPIN_PER_M * offset.min(2.0));
        // The dead in the turret's frame stay in the hull's world; detach them first.
        for body in self.couch_ragdolls.iter_mut().filter(|b| !b.world_space && b.frame == BloodFrame::Turret) {
            body.detach_into_world(&self.player);
        }
        let Some(body) = wreck_turret::Body::thrown(source, gun_local, ring, velocity, spin) else {
            println!("[cookoff] the turret's rigid body could not be built: it stays on its ring (named)");
            return;
        };
        println!("[cookoff] the turret is thrown at {speed_mps:.1} m/s ({:.0} kPa under it): estimated source mass {:.0} kg",
            gauge_pa / 1000.0, body.mass_kg);
        self.player.turret_thrown = Some(body.model() * Mat4::from_scale(Vec3::splat(scale)));
        self.cookoff.turret = Some(body);
        self.cookoff.launched_s = Some(self.cookoff.clock_s);
        self.cookoff.path.clear();
        self.cookoff.path.push((0.0, self.player.turret_model()));
        let seed = self.kill_tick.wrapping_mul(0x9E37_79B9) ^ 0x7055;
        // The fireball of everything that has gone, out of the open ring (`fireball`), its light
        // the fireball's size, and the pressure front of its TNT equivalent.
        let fuel = self.cookoff.fuel_kg.max(charge_kg());
        let (diameter, life) = fireball(fuel);
        self.weapon_vfx.cookoff_fireball(ring_at + up * 0.5, up, fireball_scale(fuel));
        self.spawn_blast_light(ring_at + up * (0.25 * diameter), 0.5 * diameter / 0.28, 26.0, life);
        self.weapon_vfx.full_cookoff_plume(ring_at, up);
        self.spawn_debris(ring_at, 40, 25.0, seed);
        self.schedule_sound_from(crate::audio::Sfx::TurretPop, ring_at);
        self.schedule_sound_from(crate::audio::Sfx::TurretRip, ring_at);
        let tnt = self.cookoff.tnt_kg.max(crate::sound_levels::AMMUNITION_DETONATION_TNT_KG);
        self.queue_blast_front(0x5401_0000 | u64::from(seed), ring_at, tnt, true);
        println!("[cookoff] fireball {diameter:.0} m for {life:.1} s ({fuel:.0} kg gone, {tnt:.0} kg TNT equivalent)");
    }

    fn advance_thrown_turret(&mut self, dt: f32) {
        let Some(body) = self.cookoff.turret.as_mut() else { return };
        let scale = self.player.parts.frame_scale;
        let hull = self.player.hull_model() * Mat4::from_scale(Vec3::splat(1.0 / scale));
        let was_settled = body.settled;
        body.advance(dt, &self.hf, hull);
        let model = body.model() * Mat4::from_scale(Vec3::splat(scale));
        self.player.turret_thrown = Some(model);
        if body.settled && !was_settled {
            println!("[cookoff] the turret came to rest {:.1} s after the throw", self.cookoff.clock_s - self.cookoff.launched_s.unwrap_or(0.0));
        }
        if let Some(launched) = self.cookoff.launched_s {
            let t = self.cookoff.clock_s - launched;
            if t <= PATH_KEEP_S && self.cookoff.path.last().is_none_or(|(last, _)| t > *last) {
                self.cookoff.path.push((t, model));
            }
        }
    }

    /// One cartridge out of the live inventory by its identity (its case burst in the cook-off;
    /// named: its burst case and shell are not drawn after).
    fn consume_live_round(&mut self, id: u32) {
        const CHAMBER_ID: u32 = 0x1_0000;
        const LOOSE_ID_BASE: u32 = 0x2_0000;
        if id == CHAMBER_ID {
            self.breech = Breech::Empty;
            return;
        }
        if id >= LOOSE_ID_BASE {
            let index = (id - LOOSE_ID_BASE) as usize;
            if matches!(self.carry_mesh, Some(CarriedMesh::LooseShell(i)) if i == index) {
                self.carry_mesh = None;
                self.grab = false;
            }
            if let Some(shell) = self.loose.get_mut(index) {
                shell.active = false;
            }
            return;
        }
        if let Some(index) = (0..crate::interior_rig::SHELL_SLOTS.len()).find(|&i| crate::interior_rig::shell_source_id(i) == id) {
            if matches!(self.carry_mesh, Some(CarriedMesh::RackShell(i)) if i == index) {
                self.carry_mesh = None;
                self.grab = false;
            }
            if self.crew_loader_round().is_some_and(|(slot, _)| slot == index) {
                self.clear_crew_loader_inventory();
            }
            self.slot_taken[index] = true;
        }
    }

    /// The man inside hears the stowage go now (a direct hit): black from the moment it reaches him.
    pub(super) fn cookoff_black_now(&mut self, at: Vec3) {
        if self.cookoff.black_from_s.is_none() {
            self.cookoff.black_from_s = Some(self.cookoff.clock_s + at.distance(self.threat_center()) / speed_of_sound());
        }
    }

    /// Whether the man died in the cook-off: his death's presentation is the cook-off's.
    pub fn cookoff_death(&self) -> bool {
        self.cookoff.black_from_s.is_some() && !self.player.alive
    }

    /// The outside shot's camera: wide, from the side and a little behind, 2.5 m up, framing the
    /// tank and the thrown turret's whole height (6 m of headroom): the framed height follows the highest the
    /// turret has been or is bound for (its height plus v²/2g while rising), eased so the camera
    /// draws back smoothly; never less than 9 m above the ring. The first of four quarters whose
    /// line to the tank the ground does not cut (buildings are not tested; named).
    pub(super) fn cookoff_outside_camera(&self) -> (Vec3, Vec3) {
        let hull = self.player.hull_model();
        let ring = hull.transform_point3(self.player.parts.turret_pivot);
        let bound = self.cookoff.turret.as_ref().map_or(0.0, |body| {
            let height = body.model().transform_point3(Vec3::ZERO).y - ring.y;
            height + body.velocity.y.max(0.0).powi(2) / (2.0 * 9.81)
        });
        // Eased on the cook-off's clock (4 /s), so the pull-back is the same at any frame rate.
        let (framed, at) = self.cookoff.framed_apex_m.get();
        let dt = (self.cookoff.clock_s - at).clamp(0.0, 1.0);
        let framed = if bound > framed { framed + (bound - framed) * (1.0 - (-4.0 * dt).exp()) } else { framed }.max(6.0);
        self.cookoff.framed_apex_m.set((framed, self.cookoff.clock_s));
        // The frame spans from 3 m below the ring to 6 m above the apex (the turret and its gun
        // reach about 3 m round its ring); the eye stands clear of the whole load's fireball as
        // drawn (about twice the correlation's diameter across: `fireball`).
        let span = framed + 9.0;
        let half = (0.5 * COOKOFF_OUTSIDE_FOV).to_radians().tan();
        let clear_of_fire = 24.0 + 1.5 * fireball(self.cookoff.load_fuel_kg.max(self.cookoff.fuel_kg)).0;
        let distance = (0.5 * span / half).max(clear_of_fire);
        let target = ring + Vec3::Y * (0.5 * span - 3.0);
        let x = hull.transform_vector3(Vec3::X).normalize();
        let z = hull.transform_vector3(Vec3::Z).normalize();
        let lift = |p: Vec3| Vec3::new(p.x, p.y.max(self.hf.height_at(p.x, p.z) + 1.7), p.z);
        let clear = |eye: Vec3| (1..20).all(|k| {
            let p = eye.lerp(ring, k as f32 / 20.0);
            p.y > self.hf.height_at(p.x, p.z) + 0.3
        });
        let toward = |o: Vec3| o.normalize() * distance;
        let eyes = [x * 24.0 - z * 9.0, -x * 24.0 - z * 9.0, x * 24.0 + z * 9.0, -x * 24.0 + z * 9.0]
            .map(|offset| lift(ring + toward(offset) + Vec3::Y * 2.5));
        (eyes.into_iter().find(|&e| clear(e)).unwrap_or(eyes[0]), target)
    }

    /// The cook-off death's outside shot: the world as it runs on, from `cookoff_outside_camera`.
    pub fn render_cookoff_outside(&self, _phase: f32, r: &mut Renderer) {
        let (eye, target) = self.cookoff_outside_camera();
        r.camera.fov_y = COOKOFF_OUTSIDE_FOV.to_radians();
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.znear = 0.2;
        r.camera.zfar = 6000.0;
        r.reticle = false;
        r.crew_ko = 0.0;
        r.concussion = 0.0;
        r.optic_blast = 0.0;
        r.optic_mask = 0.0;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.rt_mix = 1.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere_enabled(false);
    }

    /// The cook-off death's last shot, the dead: down through the open ring from 4.5 m to 2.2 m
    /// above it; if the turret stayed on its ring, from the turret's own witness eye.
    pub fn render_cookoff_crew(&self, phase: f32, r: &mut Renderer) {
        let (eye, target, fov) = self.cookoff_crew_camera(phase);
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.fov_y = f32::to_radians(fov);
        r.camera.znear = 0.05;
        r.camera.zfar = 6000.0;
        r.reticle = false;
        r.crew_ko = 0.0;
        r.concussion = 0.0;
        r.optic_blast = 0.0;
        r.optic_mask = 0.0;
        r.interior_light = if self.cookoff.turret.is_some() { 0.0 } else { 1.0 };
        r.rt_mix = 1.0;
        r.weather_shelter = if self.cookoff.turret.is_some() { 0.0 } else { 1.0 };
        r.set_interior_atmosphere_enabled(true);
    }

    /// The camera of `render_cookoff_crew` at `phase` (eye, target, vertical field of view).
    /// The dead are framed from the first eye with a clear line (through the tank's own surfaces,
    /// the thrown turret's among them) to the middle of their bodies: above the open ring, above
    /// each roof opening, then inside the hull ahead of and behind the fighting compartment; the
    /// choice is kept for the whole shot, and the eye closes a third of the way in over it.
    pub(super) fn cookoff_crew_camera(&self, phase: f32) -> (Vec3, Vec3, f32) {
        let ease = phase.clamp(0.0, 1.0);
        let ease = ease * ease * (3.0 - 2.0 * ease);
        let (eye, target, fov) = self.cookoff.crew_camera.get().unwrap_or_else(|| {
            let chosen = self.choose_cookoff_crew_camera();
            self.cookoff.crew_camera.set(Some(chosen));
            chosen
        });
        (eye.lerp(target, 0.33 * ease), target, fov)
    }

    fn choose_cookoff_crew_camera(&self) -> (Vec3, Vec3, f32) {
        let hull = self.player.hull_model();
        let ring = hull.transform_point3(self.player.parts.turret_pivot);
        let up = hull.transform_vector3(Vec3::Y).normalize();
        let torso = crew_physics::CrewBodyPart::Torso.index();
        let bodies: Vec<Vec3> = self.couch_ragdolls.iter()
            .map(|b| b.parent_model(&self.player).transform_point3(b.segments[torso].pos)).collect();
        let target = if bodies.is_empty() { ring - up * 0.5 } else { bodies.iter().sum::<Vec3>() / bodies.len() as f32 };
        let models = self.fire_surface_models();
        let clear = |eye: Vec3| self.cabin_fire.sweep(eye, target, &models).is_none_or(|c| c.fraction > 0.95);
        let x = hull.transform_vector3(Vec3::X).normalize();
        let z = hull.transform_vector3(Vec3::Z).normalize();
        let mut candidates: Vec<(Vec3, f32)> = [Vec3::ZERO, x, -x, z, -z].iter()
            .map(|&o| (ring + up * 3.2 + o * 0.6, 62.0)).collect();
        candidates.extend((0..5).map(|i| (self.access_hatch_center(i) + up * 1.1, 70.0)));
        candidates.extend([Vec3::new(0.0, 1.75, 1.45), Vec3::new(0.0, 1.85, -1.0), Vec3::new(0.6, 1.8, 1.3), Vec3::new(-0.6, 1.8, 1.3)]
            .map(|p| (hull.transform_point3(p), 84.0)));
        let (eye, fov) = candidates.iter().copied().find(|&(eye, _)| clear(eye)).unwrap_or(candidates[0]);
        (eye, target, fov)
    }

    pub fn cookoff_receipt(&self) -> serde_json::Value {
        self.cookoff.receipt()
    }
}
