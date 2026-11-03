//! What a breached component does that can be seen from outside the Tiger: the effects the
//! layered damage model drives (`crate::fea_damage`, docs/FEA_DAMAGE_MODEL_20261102.md). The
//! chain says which casing the round opened and where; this turns that into matter leaving the
//! vehicle, each by a named law:
//! * a holed fuel tank: petrol stands above the round's hole in the tank, so it runs out through
//!   the round's own hole in the plate at Torricelli's rate through the solved aperture (discharge
//!   coefficient 0.62, a sharp-edged hole), the head the tank's live level above the hole, falling
//!   as the tank drains; it falls, pools on the ground (a pool of a few millimetres) and, once the
//!   fire the breach started is burning, burns there as a pool fire (the game's fire rule lights the
//!   fire; the pool is where that fire can stand outside); the litres that ran out are gone from
//!   the live fuel;
//! * a holed radiator: the coolant boils off the hot engine as steam through the deck grilles over
//!   it, for the minutes the volume lasts;
//! * a breached engine: its oil smokes off the hot block through the grilles and, with the fire
//!   burning, its flames stand out of the grilles (a world fire source on the deck);
//! * a holed gearbox, steering unit or final drive: its oil drips out under the hull (Torricelli
//!   through the hole at a named viscous share) and pools there.
//! Named approximations: the drawn drops are beads standing for the stream's volume (a fixed count
//! per litre), the pool is a flat disc of a fixed depth on whatever ground it lands on, the two
//! modelled tanks share the fuel system's fill, the vents' durations are estimates of the volumes
//! involved, and nothing here is saved (a leak does not survive a reload; the fuel it took does).
use super::*;
use crate::fea_damage::chain;
use crate::fea_damage::components::{self, Frame, System, COMPONENT_SPECS};

/// Torricelli's discharge coefficient for a sharp-edged hole (named).
pub(super) const DISCHARGE_CD: f32 = 0.62;
/// Oil through the same hole at a named share of petrol's rate (viscosity; named).
pub(super) const OIL_FLOW_SHARE: f32 = 0.08;
/// A pool on the ground spreads to this depth (named).
pub(super) const POOL_DEPTH_M: f32 = 0.004;
/// Drawn beads per litre that runs out (presentation).
pub(super) const DROPS_PER_LITRE: f32 = 14.0;
pub(super) const MAX_DROPS: usize = 320;
pub(super) const STEAM_S: f32 = 120.0;
pub(super) const OIL_SMOKE_S: f32 = 240.0;
/// Litres of oil a holed housing can lose (estimates): gearbox, steering unit, final drive.
pub(super) const GEARBOX_OIL_L: f32 = 30.0;
pub(super) const STEERING_OIL_L: f32 = 20.0;
pub(super) const FINAL_DRIVE_OIL_L: f32 = 12.0;

/// Litres per second through `area_m2` under `head_m` of liquid.
pub(super) fn torricelli_lps(area_m2: f32, head_m: f32, cd: f32) -> f32 {
    if !(head_m > 0.0) || !(area_m2 > 0.0) { return 0.0; }
    cd * area_m2 * (2.0 * 9.81 * head_m).sqrt() * 1000.0
}
/// What stands above a hole at height `hole_y` in a tank whose liquid surface is at `level_y`
/// and floor at `floor_y`, holding `tank_l`: (litres above the hole, the head), or nothing when
/// the hole is above the liquid (vapour, no stream).
pub(super) fn above_hole(level_y: f32, hole_y: f32, floor_y: f32, tank_l: f32) -> Option<(f32, f32)> {
    let head = level_y - hole_y;
    if !(head > 0.0) || !(tank_l > 0.0) { return None; }
    let depth = (level_y - floor_y).max(0.01);
    Some((tank_l * (head / depth).clamp(0.0, 1.0), head))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Liquid { Petrol, Oil }
#[derive(Clone, Copy, Debug)]
pub(super) struct Drop { pub pos: Vec3, pub vel: Vec3, pub size: f32 }
#[derive(Clone, Debug)]
pub(super) struct Leak {
    pub liquid: Liquid,
    /// Where the liquid leaves the vehicle (hull frame) and which way it first goes.
    pub hole_hull: Vec3,
    pub out_hull: Vec3,
    pub area_m2: f32,
    pub remaining_l: f32,
    pub remaining0_l: f32,
    pub head0_m: f32,
    pub pooled_l: f32,
    pub pool_world: Option<Vec3>,
    pub drops: Vec<Drop>,
    pub drop_credit: f32,
    pub seed: u32,
}
impl Leak {
    pub(super) fn head_m(&self) -> f32 {
        if self.remaining0_l <= 0.0 { 0.0 } else { self.head0_m * (self.remaining_l / self.remaining0_l).clamp(0.0, 1.0) }
    }
    pub(super) fn rate_lps(&self) -> f32 {
        let share = match self.liquid { Liquid::Petrol => 1.0, Liquid::Oil => OIL_FLOW_SHARE };
        torricelli_lps(self.area_m2, self.head_m(), DISCHARGE_CD) * share
    }
    pub(super) fn pool_radius_m(&self) -> f32 {
        (self.pooled_l * 1e-3 / (std::f32::consts::PI * POOL_DEPTH_M)).max(0.0).sqrt()
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum VentKind { Steam, OilSmoke }
#[derive(Clone, Debug)]
pub(super) struct Vent { pub kind: VentKind, pub point_hull: Vec3, pub remaining_s: f32, pub total_s: f32, pub seed: u32 }

#[derive(Default, Clone, Debug)]
pub(super) struct FeaEffects {
    pub leaks: Vec<Leak>,
    pub vents: Vec<Vent>,
    /// The engine bay's fire out of the deck grilles (hull point), once the breach's fire burns.
    pub engine_fire_hull: Option<Vec3>,
    pub litres_leaked: f32,
}

fn hash01(x: u32) -> f32 {
    let mut s = x.wrapping_mul(2654435761).wrapping_add(0x9E37_79B9);
    s ^= s >> 15; s = s.wrapping_mul(0x2c1b3c6d); s ^= s >> 12; s = s.wrapping_mul(0x297a2d39); s ^= s >> 15;
    (s >> 8) as f32 / 16777216.0
}

impl Game {
    /// The deck surface above hull-frame (x, z): the first source surface met from above, as a
    /// hull-frame point; the engine deck's authored height where none is met.
    fn fea_deck_grille(&self, x: f32, z: f32) -> Vec3 {
        let hull = self.player.hull_model();
        let models = self.fire_surface_models();
        let from = hull.transform_point3(Vec3::new(x, 3.2, z));
        let to = hull.transform_point3(Vec3::new(x, 0.8, z));
        if let Some(c) = self.cabin_fire.sweep(from, to, &models) {
            let world = models[c.frame].transform_point3(c.point);
            return hull.inverse().transform_point3(world) + Vec3::Y * 0.02;
        }
        Vec3::new(x, 1.05, z)
    }

    /// The chain's breaches as effects: called once per solved penetration with the shot line
    /// (hull frame), the chain's result and the solved hole's area.
    pub(super) fn fea_breach_effects(&mut self, line: &chain::ShotLine, result: &chain::ChainResult, hole_area_m2: f32) {
        let set = components::tiger_components();
        // The live fuel (campaign or standalone); a scene with no fuel state is taken as 0.8 full (named).
        let fuel = self.tank_fuel_liters().unwrap_or(TIGER_FUEL_CAPACITY_L * 0.8);
        let fill = (fuel / TIGER_FUEL_CAPACITY_L).clamp(0.0, 1.0);
        let hole_area = hole_area_m2.max(1e-5);
        let seed_base = (self.armor_tick as u32).wrapping_mul(0x9E37_79B9);
        for (k, hit) in result.hits.iter().enumerate() {
            if !hit.breached { continue; }
            let spec = &COMPONENT_SPECS[hit.component];
            let env = set.envelopes.iter().find(|e| e.spec == hit.component);
            let Some(first) = result.crossings.iter().find(|c| c.layer.kind.component() == Some(hit.component)) else { continue };
            let inside = line.entry + line.dir * first.layer.s0;
            let seed = seed_base ^ (k as u32).wrapping_mul(7919);
            match spec.system {
                System::Fuel(_) => {
                    let Some(env) = env.filter(|e| e.frame == Frame::Hull) else { continue };
                    let level = chain::liquid_level(env, Vec3::Y, fill);
                    let (lo, _) = env.bounds();
                    // The two modelled tanks share the system's fill (named).
                    let Some((above, head)) = above_hole(level, inside.y, lo.y, TIGER_FUEL_CAPACITY_L * 0.5 * fill) else { continue };
                    self.fea_effects.leaks.push(Leak { liquid: Liquid::Petrol, hole_hull: line.entry, out_hull: -line.dir,
                        area_m2: hole_area, remaining_l: above, remaining0_l: above, head0_m: head, pooled_l: 0.0,
                        pool_world: None, drops: Vec::new(), drop_credit: 0.0, seed });
                    println!("[fea] petrol runs from the hole: {above:.0} l above it, {head:.2} m head, {:.2} l/s at first", torricelli_lps(hole_area, head, DISCHARGE_CD));
                }
                System::Cooling(_) => {
                    let Some(env) = env else { continue };
                    let (lo, hi) = env.bounds();
                    let grille = self.fea_deck_grille((lo.x + hi.x) * 0.5, (lo.z + hi.z) * 0.5);
                    if !self.fea_effects.vents.iter().any(|v| v.kind == VentKind::Steam && v.point_hull.distance(grille) < 0.3) {
                        self.fea_effects.vents.push(Vent { kind: VentKind::Steam, point_hull: grille, remaining_s: STEAM_S, total_s: STEAM_S, seed });
                    }
                }
                System::Engine => {
                    let grille = self.fea_deck_grille(0.0, -2.4);
                    if !self.fea_effects.vents.iter().any(|v| v.kind == VentKind::OilSmoke) {
                        self.fea_effects.vents.push(Vent { kind: VentKind::OilSmoke, point_hull: grille, remaining_s: OIL_SMOKE_S, total_s: OIL_SMOKE_S, seed });
                    }
                    self.fea_effects.engine_fire_hull = Some(grille);
                }
                System::Gearbox | System::Steering | System::FinalDrive(_) => {
                    let Some(env) = env else { continue };
                    let (lo, hi) = env.bounds();
                    let litres = match spec.system { System::Gearbox => GEARBOX_OIL_L, System::Steering => STEERING_OIL_L, _ => FINAL_DRIVE_OIL_L };
                    let hole = Vec3::new((lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5);
                    self.fea_effects.leaks.push(Leak { liquid: Liquid::Oil, hole_hull: hole, out_hull: -Vec3::Y, area_m2: hole_area.min(4e-4),
                        remaining_l: litres, remaining0_l: litres, head0_m: (hi.y - lo.y) * 0.5, pooled_l: 0.0,
                        pool_world: None, drops: Vec::new(), drop_credit: 0.0, seed });
                }
                _ => {}
            }
        }
    }

    /// Each update: the leaks run, fall and pool; the vents smoke; the drops and pools are drawn.
    pub(super) fn drive_fea_effects(&mut self, dt: f32, r: &mut Renderer) {
        if self.fea_effects.leaks.is_empty() && self.fea_effects.vents.is_empty() { return; }
        let dt = dt.clamp(0.0, 0.1);
        let hull = self.player.hull_model();
        let gravity = Vec3::new(0.0, -9.81, 0.0);
        let mut petrol_mats = Vec::new();
        let mut oil_mats = Vec::new();
        let mut leaked = 0.0;
        for leak in &mut self.fea_effects.leaks {
            let q = leak.rate_lps();
            let litres = (q * dt).min(leak.remaining_l);
            leak.remaining_l -= litres;
            leak.pooled_l += litres;
            if leak.liquid == Liquid::Petrol { leaked += litres; }
            leak.drop_credit += litres * DROPS_PER_LITRE;
            let hole = hull.transform_point3(leak.hole_hull);
            let out = hull.transform_vector3(leak.out_hull).normalize_or_zero();
            while leak.drop_credit >= 1.0 && leak.drops.len() < MAX_DROPS {
                leak.drop_credit -= 1.0;
                leak.seed = leak.seed.wrapping_add(1);
                let j = Vec3::new(hash01(leak.seed ^ 11) - 0.5, hash01(leak.seed ^ 23) - 0.5, hash01(leak.seed ^ 37) - 0.5);
                let size = match leak.liquid { Liquid::Petrol => 0.012, Liquid::Oil => 0.008 } * (0.8 + 0.4 * hash01(leak.seed ^ 41));
                leak.drops.push(Drop { pos: hole + out * 0.03, vel: out * (0.3 + 0.4 * hash01(leak.seed ^ 5)) + j * 0.15, size });
            }
            let mut landed = None;
            leak.drops.retain_mut(|d| {
                d.vel += gravity * dt;
                d.pos += d.vel * dt;
                let ground = self.hf.height_at(d.pos.x, d.pos.z);
                if d.pos.y <= ground + 0.004 { landed = Some(Vec3::new(d.pos.x, ground, d.pos.z)); false } else { true }
            });
            if leak.pool_world.is_none() { leak.pool_world = landed; }
            let mats = if leak.liquid == Liquid::Petrol { &mut petrol_mats } else { &mut oil_mats };
            for d in &leak.drops {
                mats.push(Mat4::from_scale_rotation_translation(Vec3::splat(d.size), Quat::IDENTITY, d.pos));
            }
            if let Some(p) = leak.pool_world {
                let radius = leak.pool_radius_m();
                if radius > 0.02 {
                    mats.push(Mat4::from_scale_rotation_translation(Vec3::new(radius * 2.0, POOL_DEPTH_M, radius * 2.0), Quat::IDENTITY, p + Vec3::Y * POOL_DEPTH_M * 0.5));
                }
            }
        }
        if leaked > 0.0 {
            if let Some(fuel) = self.campaign.as_mut().map(|r| &mut r.fuel_liters).or(self.standalone_fuel_liters.as_mut()) {
                *fuel = (*fuel - leaked).max(0.0);
            }
            self.fea_effects.litres_leaked += leaked;
        }
        self.fea_effects.leaks.retain(|l| l.remaining_l > 0.01 || !l.drops.is_empty() || l.pooled_l > 0.5);
        let wind = r.weather_wind_world;
        let time = r.time;
        for vent in &mut self.fea_effects.vents {
            vent.remaining_s -= dt;
            let strength = (vent.remaining_s / vent.total_s).clamp(0.0, 1.0);
            if strength <= 0.0 { continue; }
            let root = hull.transform_point3(vent.point_hull);
            let (albedo, rise, density) = match vent.kind { VentKind::Steam => (0.92, 1.4, 0.35), VentKind::OilSmoke => (0.08, 1.0, 0.45) };
            for lobe in 0..3 {
                let age = lobe as f32 * 1.1 + 0.4;
                let roll = crate::particle_motion::advancing_phase(vent.seed, time, 0.6);
                let sway = Vec3::new((roll + age).sin(), 0.0, (roll * 0.73 + age).cos()) * 0.15 * age;
                let center = root + Vec3::new(wind.x * age * 0.3, age * rise, wind.y * age * 0.3) + sway;
                r.add_smoke_sweep(root, center, 0.22 + age * 0.3, strength * density / (1.0 + age * 0.3), roll + age, albedo);
            }
        }
        self.fea_effects.vents.retain(|v| v.remaining_s > 0.0);
        if self.slots.fea_liquid[0] != usize::MAX {
            petrol_mats.truncate(FEA_LIQUID_CAPACITY);
            oil_mats.truncate(FEA_LIQUID_CAPACITY);
            r.set_dynamic_instanced_mats(self.slots.fea_liquid[0], &petrol_mats);
            r.set_dynamic_instanced_mats(self.slots.fea_liquid[1], &oil_mats);
        }
    }

    /// The fires the breaches stand outside the hull: the engine's out of the deck grilles and
    /// the petrol pools', once the breach's fire burns (world position, strength 0..1, seed).
    pub(super) fn fea_fire_sources(&self) -> Vec<(Vec3, f32, u32)> {
        let mut out = Vec::new();
        if !self.fire_active || self.fire <= 0.0 { return out; }
        let hull = self.player.hull_model();
        if let Some(p) = self.fea_effects.engine_fire_hull {
            out.push((hull.transform_point3(p), self.fire.clamp(0.0, 1.0), 0x7E00_0001));
        }
        for (i, leak) in self.fea_effects.leaks.iter().enumerate() {
            if leak.liquid != Liquid::Petrol || leak.pooled_l < 2.0 { continue; }
            if let Some(p) = leak.pool_world {
                out.push((p + Vec3::Y * 0.05, (leak.pooled_l / 40.0).clamp(0.25, 1.0), 0x7E00_0010 ^ i as u32));
            }
        }
        out
    }

    pub fn debug_fea_effects_receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "leaks": self.fea_effects.leaks.iter().map(|l| serde_json::json!({
                "liquid": format!("{:?}", l.liquid), "remaining_l": l.remaining_l, "pooled_l": l.pooled_l,
                "rate_lps": l.rate_lps(), "head_m": l.head_m(), "pool_radius_m": l.pool_radius_m(),
                "pool": l.pool_world.map(|p| p.to_array()), "drops": l.drops.len()})).collect::<Vec<_>>(),
            "vents": self.fea_effects.vents.iter().map(|v| serde_json::json!({"kind": format!("{:?}", v.kind), "remaining_s": v.remaining_s})).collect::<Vec<_>>(),
            "engine_fire": self.fea_effects.engine_fire_hull.map(|p| p.to_array()),
            "litres_leaked": self.fea_effects.litres_leaked, "fuel_liters": self.tank_fuel_liters(),
            "fire_sources": self.fea_fire_sources().len()})
    }
}

pub(super) const FEA_LIQUID_CAPACITY: usize = 1024;
/// The two liquids' bead and pool pools: petrol (a pale amber, glossy) and oil (near black, glossy).
pub(super) fn allocate_liquid_pools(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer) -> [usize; 2] {
    [[150u8, 110, 40], [22u8, 18, 14]].map(|rgb| {
        let mut mesh = crate::prims::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5), rgb);
        mesh.metallic_factor = 0.0;
        mesh.roughness_factor = 0.12;
        r.add_dynamic_instanced_pool(device, queue, &mesh, FEA_LIQUID_CAPACITY).expect("liquid pool")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn petrol_runs_at_torricellis_rate_and_only_from_below_the_surface() {
        // A 76 mm hole (aperture radius 38 mm) under 0.3 m of petrol: about 6.8 l/s (0.62 x
        // 4.5e-3 m2 x 2.43 m/s); the solved 14 mm aperture of the fixture passes 0.8 l/s.
        let area = std::f32::consts::PI * 0.038 * 0.038;
        let q = torricelli_lps(area, 0.3, DISCHARGE_CD);
        assert!(q > 6.0 && q < 7.5, "{q}");
        let solved = torricelli_lps(std::f32::consts::PI * 0.014 * 0.014, 0.22, DISCHARGE_CD);
        assert!(solved > 0.6 && solved < 1.0, "{solved}");
        assert_eq!(torricelli_lps(area, 0.0, DISCHARGE_CD), 0.0);
        assert!(above_hole(1.2, 1.3, 0.8, 100.0).is_none(), "a hole above the petrol gives vapour, no stream");
        let (above, head) = above_hole(1.2, 1.0, 0.8, 100.0).unwrap();
        assert!((above - 50.0).abs() < 1e-3 && (head - 0.2).abs() < 1e-6);
    }
    #[test]
    fn a_leak_slows_as_the_tank_drains_and_the_pool_grows_with_what_ran_out() {
        let mut leak = Leak { liquid: Liquid::Petrol, hole_hull: Vec3::ZERO, out_hull: Vec3::X, area_m2: 4.5e-3,
            remaining_l: 60.0, remaining0_l: 60.0, head0_m: 0.3, pooled_l: 0.0, pool_world: None, drops: Vec::new(), drop_credit: 0.0, seed: 1 };
        let first = leak.rate_lps();
        let mut t = 0.0;
        while leak.remaining_l > 0.01 && t < 600.0 { let q = leak.rate_lps(); let l = (q / 60.0).min(leak.remaining_l); leak.remaining_l -= l; leak.pooled_l += l; t += 1.0 / 60.0; }
        // 60 l at 6.8 l/s falling with the root of what is left: empty in about 18 s.
        assert!(first > 6.0 && t > 12.0 && t < 60.0, "{first} {t}");
        assert!((leak.pooled_l - 60.0).abs() < 0.05);
        assert!(leak.pool_radius_m() > 1.5 && leak.pool_radius_m() < 2.5, "{}", leak.pool_radius_m());
        let oil = Leak { liquid: Liquid::Oil, ..leak.clone() };
        assert!(oil.rate_lps() < leak.rate_lps() * 0.1 + 1e-6);
    }
}
