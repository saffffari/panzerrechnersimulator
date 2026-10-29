//! Each vehicle's own finish on the raymarch backend (2026-10-28, "the T-34 is too bright, doesn't
//! read as a real T-34 colour, and looks brand new"): the enemy vehicles, their wrecks and the
//! friendly Tigers take a seed of their own, and from it their paint batch, wear, mud and grime and,
//! in winter, the remains of a field whitewash. The renderer draws them in the G-buffer
//! (`rm_gbuffer.wgsl` `vehicle_finish`: paint variance between the 4BO chips, whitewash, chips,
//! streaks, mud and slush, exhaust soot, a burnt-out wreck's scorch) and in the shade pass (worn
//! edges and corner grime, as on the player's Tiger: `Renderer::set_worn_vehicles`).
//!
//! Named approximations (docs/PHYSICAL_TRUTH_AUDIT.md): the loads are per vehicle and fixed for its
//! life, not accumulated per face from its history; their ranges and the whitewash share are
//! estimates for a winter-1942 front, not counts from records.
use super::*;

/// Share of winter vehicles that carry the remains of a field whitewash. Winter 1942 practice was
/// mixed: many T-34s fought in plain 4BO, others in chalk or lime washes worn patchy (an estimate).
const WINTER_WHITEWASH_SHARE: f32 = 0.4;
/// Reach of a vehicle's worn edges round its hull centre, beyond its half length (m).
const WORN_REACH_MARGIN_M: f32 = 1.0;
/// Vehicles beyond this distance from the eye are not worth a worn-edge slot (a few pixels tall).
const WORN_RANGE_M: f32 = 250.0;

/// One vehicle's finish: its seed and loads, each 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Finish {
    pub seed: u32,
    pub mechanical: f32,
    pub grime: f32,
    pub whitewash: f32,
}

/// A uniform 0..1 from a seed and a salt (an integer hash: lowbias32).
fn unit(seed: u32, salt: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ salt;
    x ^= x >> 16; x = x.wrapping_mul(0x7feb_352d); x ^= x >> 15; x = x.wrapping_mul(0x846c_a68b); x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

/// The finish of the vehicle with `seed`: weeks to months in the field (chipped and scraped, 0.35
/// to 0.8), mud and slush 0.45 to 0.95, and in winter a scrubbed whitewash on some.
pub(crate) fn finish(seed: u32, winter: bool) -> Finish {
    let seed = (seed & 0xF_FFFF).max(1);
    let washed = winter && unit(seed, 0x51) < WINTER_WHITEWASH_SHARE;
    Finish {
        seed,
        mechanical: 0.35 + 0.45 * unit(seed, 0x13),
        grime: 0.45 + 0.50 * unit(seed, 0x29),
        whitewash: if washed { 0.4 + 0.5 * unit(seed, 0x37) } else { 0.0 },
    }
}

/// The record `rm_gbuffer.wgsl` reads: [seed * 8 + burnt * 4 + part, ground y, loads, rear z].
/// Every value is an integer below 2^24 or a plain float, so it survives f32 exactly.
pub(crate) fn pack(f: Finish, part: u32, burnt: bool, ground_y: f32, rear_z: f32) -> [f32; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    let code = (f.seed & 0xF_FFFF).max(1) * 8 + if burnt { 4 } else { 0 } + part.min(2);
    let loads = byte(f.grime) | byte(f.whitewash) << 8 | byte(f.mechanical) << 16;
    [code as f32, ground_y, loads as f32, rear_z]
}

/// Per enemy slot, the seed of the vehicle now in it: a new one each time the slot activates.
#[derive(Default)]
pub(super) struct Seeds {
    seeds: Vec<u32>,
    active: Vec<bool>,
    serial: u32,
}
impl Seeds {
    pub(super) fn seed(&self, slot: usize) -> u32 { self.seeds.get(slot).copied().unwrap_or(1) }
    fn observe(&mut self, slot: usize, active: bool, act_seed: u32) -> u32 {
        if self.seeds.len() <= slot {
            self.seeds.resize(slot + 1, 0);
            self.active.resize(slot + 1, false);
        }
        if active && (!self.active[slot] || self.seeds[slot] == 0) {
            self.serial = self.serial.wrapping_add(1);
            self.seeds[slot] = ((unit(act_seed, self.serial.wrapping_mul(0x85EB_CA6B)) * 1_048_575.0) as u32).max(1);
        }
        self.active[slot] = active;
        self.seeds[slot]
    }
}

/// `KESSEL_VEHICLE_FINISH=0`: the source models' own materials and no per-vehicle finish (review).
pub(crate) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("KESSEL_VEHICLE_FINISH").as_deref() != Ok("0"))
}

impl Game {
    /// Headless fixtures: the first `count` enemy slots made T-34s (alive, parts and kind).
    pub fn debug_t34_slots(&mut self, count: usize) -> Vec<usize> {
        let n = count.min(self.enemies.len());
        for e in self.enemies.iter_mut().take(n) {
            e.kind = EnemyKind::T34;
            e.parts = tank::parts_for(EnemyKind::T34);
        }
        (0..n).collect()
    }

    /// Push every other vehicle's finish and the worn-edge list (raymarch backend only).
    pub(super) fn drive_vehicle_finishes(&mut self, r: &mut Renderer) {
        if !crate::graphics::raymarch_backend_enabled() || !enabled() { return; }
        let winter = self.act.season == crate::world::Season::Winter;
        let eye = r.camera.pos;
        let mut worn: Vec<(f32, [[f32; 4]; 2])> = Vec::new();
        let mut wear_vehicle = |r: &mut Renderer, slots: [usize; 3], f: Finish, burnt: bool, ground_y: f32,
            centre: Vec3, half_length: f32, worn: &mut Vec<(f32, [[f32; 4]; 2])>| {
            let rear = r.mesh_local_bounds(slots[0]).map_or(-half_length, |(lo, _)| lo.z);
            for (part, &slot) in slots.iter().enumerate() {
                r.set_vehicle_finish(slot, pack(f, part as u32, burnt, ground_y, rear));
            }
            let distance = centre.distance(eye);
            if distance < WORN_RANGE_M {
                worn.push((distance, [[centre.x, centre.y, centre.z, half_length + WORN_REACH_MARGIN_M],
                    [f.mechanical, f.grime, 0.0, 0.0]]));
            }
        };
        let act_seed = self.act.seed;
        for ei in 0..self.enemies.len() {
            let active = self.enemy_active.get(ei).copied().unwrap_or(false);
            let seed = self.vehicle_finish_seeds.observe(ei, active, act_seed);
            let rigs = [self.slots.t34[ei], self.slots.su100[ei], self.slots.kv2[ei],
                self.slots.isu152[ei], self.slots.is2[ei], self.slots.is3[ei]];
            if !active { continue; }
            let e = &self.enemies[ei];
            let rig = rigs[match e.kind {
                EnemyKind::T34 => 0, EnemyKind::Su100 => 1, EnemyKind::Kv2 => 2,
                EnemyKind::Isu152 => 3, EnemyKind::Is2 => 4, EnemyKind::Is3 => 5,
            }];
            let burnt = !e.alive && self.wrecks.get(ei).is_some_and(|w| w.burning || w.fuel_remaining < 0.999);
            let half_length = e.parts.hull_half_extents.y;
            wear_vehicle(r, rig, finish(seed, winter), burnt, e.pos.y, e.center(), half_length, &mut worn);
        }
        for (i, hulk) in self.campaign_hulks.iter().enumerate() {
            let rigs = [self.slots.hulk_t34[i], self.slots.hulk_su100[i], self.slots.hulk_kv2[i],
                self.slots.hulk_isu152[i], self.slots.hulk_is2[i], self.slots.hulk_is3[i]];
            let rig = rigs[match hulk.kind {
                EnemyKind::T34 => 0, EnemyKind::Su100 => 1, EnemyKind::Kv2 => 2,
                EnemyKind::Isu152 => 3, EnemyKind::Is2 => 4, EnemyKind::Is3 => 5,
            }];
            let ground = hulk.models[0].w_axis.truncate();
            let half_length = tank::parts_for(hulk.kind).hull_half_extents.y;
            wear_vehicle(r, rig, finish(hulk.finish_seed, winter), hulk.burning || hulk.ember > 0.0,
                ground.y, ground + Vec3::Y * 1.0, half_length, &mut worn);
        }
        // The friendly Tigers: the same wear, mud and grime on their Panzergrau (the 4BO batch and
        // the whitewash are the Soviet paint's). The abandoned Tigers are left as they are: the
        // player may take one over as their own vehicle.
        for (i, ally) in self.allies.iter().enumerate() {
            if let Some(&slots) = self.slots.allies.get(i) {
                wear_vehicle(r, slots, finish(0x7100 + i as u32, false), !ally.tank.alive, ally.tank.pos.y,
                    ally.tank.center(), ally.tank.parts.hull_half_extents.y, &mut worn);
            }
        }
        worn.sort_by(|a, b| a.0.total_cmp(&b.0));
        let list: Vec<[[f32; 4]; 2]> = worn.into_iter().map(|(_, v)| v).take(crate::renderer::MAX_WORN_VEHICLES).collect();
        r.set_worn_vehicles(&list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_record_survives_f32_and_unpacks_as_the_shader_reads_it() {
        for seed in [1u32, 2, 777, 0xF_FFFF, 0xFFFF_FFFF] {
            let f = finish(seed, true);
            for part in 0..3 {
                for burnt in [false, true] {
                    let [x, y, z, w] = pack(f, part, burnt, -19.75, -3.49);
                    let code = (x + 0.5) as u32;
                    let loads = (z + 0.5) as u32;
                    assert_eq!(code & 3, part);
                    assert_eq!(code & 4 != 0, burnt);
                    assert_eq!(code >> 3, f.seed);
                    assert!(x < 16_777_216.0 && z < 16_777_216.0);
                    assert!(((loads & 255) as f32 / 255.0 - f.grime).abs() < 0.003);
                    assert!((((loads >> 8) & 255) as f32 / 255.0 - f.whitewash).abs() < 0.003);
                    assert!((((loads >> 16) & 255) as f32 / 255.0 - f.mechanical).abs() < 0.003);
                    assert_eq!((y, w), (-19.75, -3.49));
                }
            }
        }
    }

    #[test]
    fn the_t34_takes_4bo_at_a_flat_finish_and_the_shader_recognises_it() {
        // FS 595C 34102 (CIELAB D65 37 / -5 / 11), the 4BO card's match: sRGB 86/89/69.
        let body = tank::t34_paint_finish("t34_hull", Some("T-34 Body")).unwrap();
        let bolts = tank::t34_paint_finish("t34_hull", Some("T-34 nails")).unwrap();
        let running = tank::t34_paint_finish("t34_hull", Some("T-34 Truck")).unwrap();
        assert_eq!(body.base, bolts.base);
        assert!(body.roughness >= 0.8 && body.metallic == 0.0, "4BO was a flat paint, not the source's mirror");
        let luminance = 0.2126 * body.base[0] + 0.7152 * body.base[1] + 0.0722 * body.base[2];
        assert!((luminance - 0.095).abs() < 0.003, "{luminance}");
        let shader = include_str!("../shaders/rm_gbuffer.wgsl");
        let constant = |name: &str| -> [f32; 3] {
            let line = shader.lines().find(|l| l.starts_with(&format!("const {name}:"))).unwrap();
            let inner = line.split("vec3<f32>(").nth(1).unwrap().trim_end_matches(");");
            let v: Vec<f32> = inner.split(',').map(|x| x.trim().parse().unwrap()).collect();
            [v[0], v[1], v[2]]
        };
        for (shader_value, finish) in [(constant("RM_4BO_PAINT"), body.base), (constant("RM_TRACK_STEEL"), running.base)] {
            for c in 0..3 { assert!((shader_value[c] - finish[c]).abs() < 2e-4, "{shader_value:?} {finish:?}"); }
        }
        assert!(tank::t34_paint_finish("t34_hull", Some("Material.001")).is_none());
    }

    #[test]
    fn tanks_differ_and_some_winter_tanks_keep_a_whitewash() {
        let finishes: Vec<Finish> = (1..=400).map(|s| finish(s * 7919, true)).collect();
        let washed = finishes.iter().filter(|f| f.whitewash > 0.0).count() as f32 / finishes.len() as f32;
        assert!((washed - WINTER_WHITEWASH_SHARE).abs() < 0.08, "{washed}");
        assert!(finishes.iter().all(|f| (0.35..=0.8).contains(&f.mechanical) && (0.45..=0.95).contains(&f.grime)));
        assert!(finishes.windows(2).all(|w| w[0] != w[1]));
        assert!((1..=50).all(|s| finish(s, false).whitewash == 0.0), "no whitewash outside winter");
        // A slot draws a new seed only when a vehicle arrives in it.
        let mut seeds = Seeds::default();
        let first = seeds.observe(3, true, 99);
        assert_eq!(seeds.observe(3, true, 99), first);
        seeds.observe(3, false, 99);
        assert_ne!(seeds.observe(3, true, 99), first);
    }
}
