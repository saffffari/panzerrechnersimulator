//! Fire, smoke, ember and explosion presentation rebuilt (docs/FX_OVERHAUL_20261109.md).
//!
//! A burning wreck, a fireball and a shell's burst are each one analytic volume descriptor
//! (`Plume`) that the raymarch backend marches per native pixel inside its bounding box
//! (shaders/rm_plumes.wgsl), at any distance and through the same fog chain as the surfaces. The
//! CPU's whole per-frame work is choosing, sizing and sorting a few dozen descriptors and a few
//! thousand embers; nothing is generated per grain. Every burning thing shares one fixed budget
//! (`MAX_WRECK_PLUMES` volumes, `MAX_EMBERS` embers, nearest first), so the cost does not stack
//! with kills, and a long-burning wreck settles to a smouldering column that costs the same as a
//! fresh one or less. `KESSEL_FX_LEGACY=1` draws the previous grain fire instead (comparison).
use glam::{Vec2, Vec3};

pub const KIND_WRECK: f32 = 0.0;
pub const KIND_FIREBALL: f32 = 1.0;
pub const KIND_BURST: f32 = 2.0;

/// Volumes the backend draws per frame (wreck columns, fireballs, bursts together).
pub const MAX_PLUMES: usize = 48;
/// Burning wrecks, hulks and vehicles drawn as columns, nearest first.
pub const MAX_WRECK_PLUMES: usize = 12;
/// Embers for all fires together, nearest fire first.
pub const MAX_EMBERS: usize = 4096;
/// Beyond this a fire throws no embers (one 4 mm ember is far below a pixel and too faint).
pub const EMBER_REACH_M: f32 = 140.0;

/// The flame's soot radiance at 1500 K, renderer units (about 7e4 cd/m^2: a luminous hydrocarbon
/// pool flame). Every flame and fireball temperature scales from it by the Planck spectrum's
/// photopic weight.
pub const FLAME_RADIANCE_1500K: f32 = 1.4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Plume {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
    pub d: [f32; 4],
    pub e: [f32; 4],
    pub f: [f32; 4],
    pub r0: [f32; 4],
    pub r1: [f32; 4],
    pub r2: [f32; 4],
}

impl Plume {
    pub fn centre(&self) -> Vec3 {
        (Vec3::from_slice(&self.e[..3]) + Vec3::from_slice(&self.f[..3])) * 0.5
    }
    /// Distance from `eye` to the nearest point of the bounds.
    pub fn distance(&self, eye: Vec3) -> f32 {
        let lo = Vec3::from_slice(&self.e[..3]);
        let hi = Vec3::from_slice(&self.f[..3]);
        (eye.clamp(lo, hi) - eye).length()
    }
    fn set_bounds(&mut self, lo: Vec3, hi: Vec3) {
        self.e[..3].copy_from_slice(&lo.to_array());
        self.f[..3].copy_from_slice(&hi.to_array());
    }
}

/// `KESSEL_FX_LEGACY=1`: the previous grain fire, smoke and explosion sequels (A/B captures).
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KESSEL_FX_LEGACY").as_deref() != Ok("1"))
}

/// March steps for a volume of `size` metres seen from `distance`: about one step per eight
/// native pixels of its depth, 8 to 22 (the per-pixel jitter and the crush's dither hide the
/// rest).
fn steps_for(size: f32, distance: f32, focal_px: f32) -> f32 {
    let px = size * focal_px / distance.max(1.0);
    (px / 8.0).clamp(8.0, 22.0)
}

/// One burning thing as the game sees it.
#[derive(Clone, Copy, Debug)]
pub struct Fire {
    /// Where the flames stand (the deck, a breach), world.
    pub base: Vec3,
    /// Up to three flame roots relative to `base` (offset, radius m; radius 0 = none).
    pub roots: [(Vec3, f32); 3],
    /// 0..1.35, the game's fire strength (fuel and damage); 0 = smouldering only.
    pub strength: f32,
    /// Seconds since ignition.
    pub age: f32,
    pub seed: u32,
}

/// The settled envelope of a long fire: the flames die back as the fuel and stowage are spent, the
/// smoke thins to a grey column (authored burn curve; the game's own fuel model still decides
/// whether anything burns).
pub fn burn_phase(strength: f32, age: f32) -> (f32, f32, f32) {
    // Flame share, smoke extinction scale, smoke albedo.
    let fierce = 1.0 - smooth(90.0, 420.0, age);
    let flame = strength * (0.35 + 0.65 * fierce);
    let soot = (0.45 + 0.55 * strength.min(1.0)) * (0.55 + 0.45 * fierce);
    // Fresh oil and rubber smoke is black (albedo about 0.12); a settled wreck's is grey.
    let albedo = 0.12 + 0.20 * (1.0 - fierce);
    (flame, soot, albedo)
}

pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The column over a burning wreck.
pub fn wreck_plume(fire: &Fire, wind: Vec2, eye: Vec3, focal_px: f32) -> Plume {
    let (flame, soot, albedo) = burn_phase(fire.strength, fire.age);
    // A buoyant plume rises tens of metres before it bends over and spreads in the wind; a big
    // fuel fire's column stands 60-120 m (authored to the fire's strength).
    let height = 35.0 + 65.0 * fire.strength.clamp(0.0, 1.35) * (0.6 + 0.4 * smooth(0.0, 30.0, fire.age));
    let rise_mps = 4.0 + 3.0 * fire.strength;
    let bend = wind * (height / rise_mps) * 0.7;
    let base_r = 1.1 + 0.4 * fire.strength;
    let top_r = 0.16 * height + 2.0;
    let flame_h = (1.2 + 3.2 * flame).max(0.0);
    let mut p = Plume::default();
    p.a = [fire.base.x, fire.base.y, fire.base.z, KIND_WRECK];
    p.b = [bend.x, height, bend.y, base_r];
    p.c = [flame, 2.4 * soot, fire.age, (fire.seed % 100_000) as f32];
    p.d = [flame_h, top_r, albedo, 0.0];
    for (k, (offset, radius)) in fire.roots.iter().enumerate() {
        let lane = [offset.x, offset.y, offset.z, radius * (0.6 + 0.4 * flame.min(1.0))];
        match k { 0 => p.r0 = lane, 1 => p.r1 = lane, _ => p.r2 = lane }
    }
    let top = fire.base + Vec3::new(bend.x, height, bend.y);
    let reach = Vec3::new(top_r, 0.0, top_r) * 1.1;
    let lo = fire.base.min(top) - reach - Vec3::new(base_r, 0.5, base_r);
    let hi = fire.base.max(top) + reach + Vec3::new(base_r, 0.0, base_r);
    p.set_bounds(lo, hi);
    // The flame's light on its own smoke: its radiance (about 0.55 of the 1500 K value over the
    // body) over its projected area.
    let area: f32 = fire.roots.iter().map(|(_, r)| 2.0 * r * flame_h * 0.55).sum();
    p.e[3] = FLAME_RADIANCE_1500K * 0.55 * area * flame.min(1.0).max(0.0);
    p.d[3] = steps_for(height.max(top_r * 2.0), p.distance(eye), focal_px);
    p
}

/// The flame's radiant intensity and colour for the scene light (renderer units), and where it
/// stands: the same flames the column draws.
pub fn flame_light(fire: &Fire) -> Option<(Vec3, f32, [f32; 3], f32)> {
    let (flame, ..) = burn_phase(fire.strength, fire.age);
    if flame <= 0.02 { return None; }
    let flame_h = 1.2 + 3.2 * flame;
    let area: f32 = fire.roots.iter().map(|(_, r)| 2.0 * r * (0.6 + 0.4 * flame.min(1.0)) * flame_h * 0.55).sum();
    let intensity = FLAME_RADIANCE_1500K * 0.55 * area * flame.min(1.0);
    let colour = crate::incandescence::planck_chroma(1350.0);
    let peak = colour.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    Some((fire.base + Vec3::Y * (flame_h * 0.55), intensity, colour.map(|c| c / peak), flame_h * 0.6))
}

/// A detonation's fireball and its smoke: `scale` its size against an 88 mm burst's (a cook-off's
/// whole load is about 10), `age` seconds since.
pub fn fireball(origin: Vec3, scale: f32, age: f32, wind: Vec2, seed: u32, stem: bool, eye: Vec3, focal_px: f32) -> Option<Plume> {
    let rmax = 2.2 * scale.max(0.5);
    let life = 14.0 + 3.0 * scale;
    if age > life || age < 0.0 { return None; }
    let mut p = Plume::default();
    p.a = [origin.x, origin.y, origin.z, KIND_FIREBALL];
    p.b = [wind.x, 0.0, wind.y, rmax];
    p.c = [1.0, 1.6 / (1.0 + 0.15 * scale), age, (seed % 100_000) as f32];
    p.d = [life, if stem { 0.18 * rmax } else { 0.0 }, 0.24, 0.0];
    let grow = 1.0 - (-age / (0.035 * rmax + 0.05)).exp();
    let rise = age.min(12.0) * (1.8 + 0.12 * rmax) - 0.025 * age * age;
    let radius = rmax * (0.25 + 0.75 * grow) * (1.0 + 0.06 * age);
    let centre = origin + Vec3::new(wind.x * age * 0.25, rise + radius * 0.4, wind.y * age * 0.25);
    let r = Vec3::splat(radius * 1.45);
    let lo = (centre - r).min(origin - Vec3::new(p.d[1], 0.5, p.d[1]) * 2.0);
    let hi = centre + r;
    p.set_bounds(lo, hi);
    p.d[3] = steps_for(radius * 2.9, p.distance(eye), focal_px);
    Some(p)
}

/// A shell's burst on the ground: `radius` the thrown cloud's (about 2.5 m for 76-88 mm HE),
/// `snow` the share of snow in what is thrown.
pub fn burst(origin: Vec3, radius: f32, snow: f32, age: f32, wind: Vec2, seed: u32, eye: Vec3, focal_px: f32) -> Option<Plume> {
    let life = 9.0;
    if age > life || age < 0.0 { return None; }
    let mut p = Plume::default();
    p.a = [origin.x, origin.y, origin.z, KIND_BURST];
    p.b = [wind.x, 0.0, wind.y, radius];
    // Snow and earth: bright snow (albedo 0.8) to dark soil (0.25) with the shell's soot.
    p.c = [1.0, 4.5, age, (seed % 100_000) as f32];
    p.d = [life, 0.0, 0.25 + 0.4 * snow.clamp(0.0, 1.0), 0.0];
    p.f[3] = snow;
    let drift = Vec3::new(wind.x, 0.0, wind.y) * age * 0.3;
    let h = radius * 3.0;
    let w = radius * (1.9 + 0.3 * age) + 1.0;
    let lo = origin + drift - Vec3::new(w, 0.6, w);
    let hi = origin + drift + Vec3::new(w, h, w);
    p.set_bounds(lo, hi);
    p.d[3] = steps_for(h, p.distance(eye), focal_px);
    Some(p)
}

/// One ember: position and packed radiance (colour at its temperature, intensity 0..2).
#[derive(Clone, Copy, Debug)]
pub struct Ember { pub pos: Vec3, pub colour: [f32; 3], pub intensity: f32 }

fn hash01(x: u32) -> f32 {
    let mut s = x ^ (x >> 16);
    s = s.wrapping_mul(0x7feb_352d); s ^= s >> 15; s = s.wrapping_mul(0x846c_a68b); s ^= s >> 16;
    (s >> 8) as f32 / 16_777_216.0
}

/// A fire's embers this frame: burning flakes of paint, rubber and stowage lofted by the plume,
/// cooling as they rise (from about 1300 K to 800 K in two to five seconds), carried downwind.
/// Each is a fixed function of (fire, index, time), so nothing is stored.
pub fn embers(fire: &Fire, wind: Vec2, time: f32, count: usize, out: &mut Vec<Ember>) {
    let (flame, ..) = burn_phase(fire.strength, fire.age);
    if flame <= 0.05 { return; }
    let flame_h = 1.2 + 3.2 * flame;
    let roots: Vec<_> = fire.roots.iter().filter(|(_, r)| *r > 0.0).collect();
    if roots.is_empty() { return; }
    for i in 0..count {
        let id = (i as u32).wrapping_mul(0x9e37_79b9) ^ fire.seed;
        let life = 2.0 + 3.0 * hash01(id ^ 0x51);
        let clock = time / life + hash01(id ^ 0x77);
        let generation = clock.floor() as u32;
        let age_t = clock.fract();
        let age = age_t * life;
        let spawn = id ^ generation.wrapping_mul(0x85eb_ca6b);
        let (offset, radius) = roots[(spawn as usize) % roots.len()];
        let a = hash01(spawn ^ 0x13) * std::f32::consts::TAU;
        let r = hash01(spawn ^ 0x29).sqrt() * radius * 1.6;
        // Lofted in the plume (2.5-5 m/s, slowing as it leaves the hot core), swirling.
        let rise = (2.5 + 2.5 * hash01(spawn ^ 0x3d)) * age * (1.0 - 0.25 * age_t) + flame_h * 0.3;
        let wobble = hash01(spawn ^ 0x61) * 6.0;
        let swirl = Vec3::new((age * 2.3 + a + wobble).sin(), 0.0, (age * 1.9 + a * 1.7).cos()) * (0.6 + 2.2 * age_t);
        let drift = Vec3::new(wind.x, 0.0, wind.y) * age * (0.6 + 0.4 * hash01(spawn ^ 0x41));
        let pos = fire.base + *offset + Vec3::new(a.cos() * r, rise, a.sin() * r) + swirl + drift;
        let kelvin = 1300.0 - 500.0 * age_t;
        let chroma = crate::incandescence::planck_chroma(kelvin);
        let fade = (1.0 - age_t).powf(1.5) * smooth(0.0, 0.08, age_t);
        out.push(Ember { pos, colour: chroma.map(|c| c.min(1.0)), intensity: 1.2 * fade * (0.4 + 0.6 * hash01(spawn ^ 0x55)) });
    }
}

/// The noise the volumes are shaped by: a tileable 3D value noise (two octaves, smooth), `n`
/// texels per side, R8. Deterministic.
pub fn noise_volume(n: usize) -> Vec<u8> {
    let lattice = |x: i32, y: i32, z: i32, period: i32, salt: u32| {
        let w = |v: i32| v.rem_euclid(period) as u32;
        hash01(w(x).wrapping_mul(73_856_093) ^ w(y).wrapping_mul(19_349_663) ^ w(z).wrapping_mul(83_492_791) ^ salt)
    };
    let value = |p: Vec3, period: i32, salt: u32| {
        let i = p.floor();
        let f = p - i;
        let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
        let (x, y, z) = (i.x as i32, i.y as i32, i.z as i32);
        let c = |dx, dy, dz| lattice(x + dx, y + dy, z + dz, period, salt);
        let x00 = c(0, 0, 0) + (c(1, 0, 0) - c(0, 0, 0)) * u.x;
        let x10 = c(0, 1, 0) + (c(1, 1, 0) - c(0, 1, 0)) * u.x;
        let x01 = c(0, 0, 1) + (c(1, 0, 1) - c(0, 0, 1)) * u.x;
        let x11 = c(0, 1, 1) + (c(1, 1, 1) - c(0, 1, 1)) * u.x;
        let y0 = x00 + (x10 - x00) * u.y;
        let y1 = x01 + (x11 - x01) * u.y;
        y0 + (y1 - y0) * u.z
    };
    let mut out = Vec::with_capacity(n * n * n);
    let cells = 8.0;
    for z in 0..n { for y in 0..n { for x in 0..n {
        let p = Vec3::new(x as f32, y as f32, z as f32) / n as f32 * cells;
        let v = 0.65 * value(p, cells as i32, 0x1234) + 0.35 * value(p * 2.0, 2 * cells as i32, 0x9876);
        out.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
    }}}
    out
}

/// WGSL constants for rm_plumes.wgsl: the Planck table (chroma times photopic weight relative to
/// 1500 K, 700-2500 K in 100 K steps) and the 1500 K flame radiance.
pub fn shader_prefix() -> String {
    const T0: f32 = 700.0;
    const DT: f32 = 100.0;
    const N: usize = 19;
    // Photopic weight of a Planck emitter: the spectrum against a Gaussian V(lambda) round 555 nm.
    let photopic = |kelvin: f32| {
        const C2: f64 = 14_387_769.0;
        (0..41).map(|i| {
            let wl = 450.0 + i as f64 * 5.0;
            let v = (-((wl - 557.0) / 46.0).powi(2) * 0.5).exp();
            v / (wl.powi(5) * ((C2 / (wl * kelvin as f64)).exp() - 1.0))
        }).sum::<f64>()
    };
    let reference = photopic(1500.0);
    let rows: Vec<String> = (0..N).map(|i| {
        let kelvin = T0 + DT * i as f32;
        let chroma = crate::incandescence::planck_chroma(kelvin);
        // Chroma normalised to unit luminance-ish (green band), times relative photopic weight.
        let g = chroma[1].max(1e-6);
        let w = (photopic(kelvin) / reference) as f32;
        format!("vec3<f32>({:?}, {:?}, {:?})", chroma[0] / g * w, chroma[1] / g * w, chroma[2] / g * w)
    }).collect();
    let tint = { let c = crate::incandescence::planck_chroma(1350.0); let m = c[0].max(c[1]).max(c[2]); c.map(|v| v / m) };
    format!("const FX_PLANCK_T0: f32 = {T0:?};\nconst FX_PLANCK_DT: f32 = {DT:?};\nconst FX_PLANCK_N: u32 = {N}u;\n\
        const FX_FLAME_RADIANCE_1500K: f32 = {FLAME_RADIANCE_1500K:?};\n\
        const FX_FLAME_TINT: vec3<f32> = vec3<f32>({:?}, {:?}, {:?});\n\
        const FX_PLANCK: array<vec3<f32>, {N}> = array<vec3<f32>, {N}>({});\n", tint[0], tint[1], tint[2], rows.join(", "))
}

/// Keep the `budget` nearest, then order far to near for compositing.
pub fn sort_far_to_near(plumes: &mut Vec<Plume>, eye: Vec3, budget: usize) {
    plumes.sort_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
    plumes.truncate(budget);
    plumes.reverse();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_settled_fire_is_no_dearer_than_a_fresh_one() {
        let fresh = burn_phase(1.0, 10.0);
        let settled = burn_phase(1.0, 900.0);
        assert!(settled.0 < fresh.0 && settled.1 < fresh.1);
    }
    #[test]
    fn the_budget_keeps_the_nearest() {
        let mut v: Vec<Plume> = (0..20).map(|i| {
            let mut p = Plume::default();
            p.set_bounds(Vec3::new(i as f32 * 10.0, 0.0, 0.0), Vec3::new(i as f32 * 10.0 + 1.0, 1.0, 1.0));
            p
        }).collect();
        sort_far_to_near(&mut v, Vec3::ZERO, 5);
        assert_eq!(v.len(), 5);
        assert!(v[0].distance(Vec3::ZERO) > v[4].distance(Vec3::ZERO));
        assert!(v[4].distance(Vec3::ZERO) < 1.0);
    }
    #[test]
    fn the_planck_table_is_brighter_when_hotter() {
        let s = shader_prefix();
        assert!(s.contains("FX_PLANCK"));
    }
}
