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
pub const EMBER_REACH_M: f32 = 80.0;

/// The flame's soot radiance at 1500 K, renderer units (about 2.4e4 cd/m^2: a luminous hydrocarbon
/// pool flame). Every flame and fireball temperature scales from it by the Planck spectrum's
/// photopic weight.
pub const FLAME_RADIANCE_1500K: f32 = 0.45;

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
/// One place on a burning surface the flames stand on: a burning surface patch's point and
/// outward normal (world), its footprint radius and how strongly it burns (0..1).
#[derive(Clone, Copy, Debug)]
pub struct Root { pub pos: Vec3, pub normal: Vec3, pub radius: f32, pub strength: f32 }

/// Roots drawn per fire, and over all fires (the shader loops over a fire's roots per sample).
pub const MAX_ROOTS_PER_FIRE: usize = 40;
pub const MAX_ROOTS: usize = 384;

#[derive(Clone, Debug)]
pub struct Fire {
    /// Where the flames stand (the deck, a breach), world.
    pub base: Vec3,
    /// The burning surface the flames stand on (world), at most `MAX_ROOTS_PER_FIRE`.
    pub roots: Vec<Root>,
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
/// `KESSEL_FX_COLUMN=H,W,D`: the wreck columns' height, width and smoke density times these (a
/// review of the column's size; default 1,1,1).
fn column_scale() -> [f32; 3] {
    static SCALE: std::sync::OnceLock<[f32; 3]> = std::sync::OnceLock::new();
    *SCALE.get_or_init(|| {
        let v: Vec<f32> = std::env::var("KESSEL_FX_COLUMN").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default();
        if v.len() == 3 && v.iter().all(|x| x.is_finite() && *x > 0.0) { [v[0], v[1], v[2]] } else { [1.0; 3] }
    })
}

pub fn wreck_plume(fire: &Fire, wind: Vec2, eye: Vec3, focal_px: f32, roots_out: &mut Vec<[f32; 4]>) -> Plume {
    let (flame, soot, albedo) = burn_phase(fire.strength, fire.age);
    let [col_h, col_w, col_d] = column_scale();
    let soot = soot * col_d;
    // A buoyant plume rises tens of metres before it bends over and spreads in the wind; a big
    // fuel fire's column stands 60-120 m (authored to the fire's strength).
    let height = (35.0 + 65.0 * fire.strength.clamp(0.0, 1.35) * (0.6 + 0.4 * smooth(0.0, 30.0, fire.age))) * col_h;
    let rise_mps = 4.0 + 3.0 * fire.strength;
    let bend = wind * (height / rise_mps) * 0.7;
    let base_r = (1.1 + 0.4 * fire.strength) * col_w;
    let top_r = (0.16 * height + 2.0) * col_w;
    let flame_h = (1.2 + 3.2 * flame).max(0.0);
    let mut p = Plume::default();
    p.a = [fire.base.x, fire.base.y, fire.base.z, KIND_WRECK];
    p.b = [bend.x, height, bend.y, base_r];
    p.c = [flame, 2.4 * soot, fire.age, (fire.seed % 100_000) as f32];
    p.d = [flame_h, top_r, albedo, 0.0];
    // The roots go to the shared root list; the plume names its slice of it.
    let start = roots_out.len() / 2;
    let mut root_lo = fire.base;
    let mut root_hi = fire.base;
    for root in fire.roots.iter().take(MAX_ROOTS_PER_FIRE) {
        if roots_out.len() / 2 >= MAX_ROOTS { break; }
        roots_out.push([root.pos.x, root.pos.y, root.pos.z, root.radius]);
        roots_out.push([root.normal.x, root.normal.y, root.normal.z, root.strength]);
        root_lo = root_lo.min(root.pos);
        root_hi = root_hi.max(root.pos);
    }
    p.r0 = [start as f32, (roots_out.len() / 2 - start) as f32, root_lo.y - 0.3, root_hi.y + flame_h * 1.4 + 0.5];
    // The flames' own box (the shader marches it finely): the roots, their footprints, licks and lean.
    let lean = Vec3::new(bend.x, 0.0, bend.y) / height.max(1.0) * flame_h * 0.4;
    let flame_lo = root_lo - Vec3::new(1.2, 0.4, 1.2) + lean.min(Vec3::ZERO);
    let flame_hi = root_hi + Vec3::new(1.2, flame_h * 1.4 + 0.5, 1.2) + lean.max(Vec3::ZERO);
    p.r1 = [flame_lo.x, flame_lo.y, flame_lo.z, 0.0];
    p.r2 = [flame_hi.x, flame_hi.y, flame_hi.z, 0.0];
    let top = fire.base + Vec3::new(bend.x, height, bend.y);
    let reach = Vec3::new(top_r, 0.0, top_r) * 1.1;
    let lo = fire.base.min(top).min(root_lo - Vec3::splat(1.5)) - reach - Vec3::new(base_r, 0.5, base_r);
    let hi = fire.base.max(top).max(root_hi + Vec3::new(1.5, flame_h * 1.4, 1.5)) + reach + Vec3::new(base_r, 0.0, base_r);
    p.set_bounds(lo, hi);
    // The flame's light on its own smoke: its radiance (about 0.55 of the 1500 K value over the
    // body) over its projected area.
    let area = root_area(fire, flame_h).min(8.0);
    p.e[3] = FLAME_RADIANCE_1500K * 0.55 * area * flame.min(1.0).max(0.0);
    p.d[3] = steps_for(height.max(top_r * 2.0), p.distance(eye), focal_px);
    p
}

/// The flames' projected area: each root's footprint width by its flame's height, about half filled.
fn root_area(fire: &Fire, flame_h: f32) -> f32 {
    fire.roots.iter().map(|r| 2.0 * r.radius * flame_h * (0.35 + 0.65 * r.strength) * 0.5).sum()
}

/// The flame's radiant intensity and colour for the scene light (renderer units), and where it
/// stands: the same flames the column draws.
pub fn flame_light(fire: &Fire) -> Option<(Vec3, f32, [f32; 3], f32)> {
    let (flame, ..) = burn_phase(fire.strength, fire.age);
    if flame <= 0.02 { return None; }
    let flame_h = 1.2 + 3.2 * flame;
    let area = root_area(fire, flame_h).min(8.0);
    let intensity = FLAME_RADIANCE_1500K * 0.55 * area * flame.min(1.0);
    let colour = flame_colour(1350.0);
    let peak = colour.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    // The light stands over the burning surface's strength-weighted middle.
    let weight: f32 = fire.roots.iter().map(|r| r.strength).sum();
    let centre = if weight > 1e-3 { fire.roots.iter().map(|r| r.pos * r.strength).sum::<Vec3>() / weight } else { fire.base };
    Some((centre + Vec3::Y * (flame_h * 0.55), intensity, colour.map(|c| c / peak), flame_h * 0.6))
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

/// A tank round striking armour: the white-orange flash of the steel and the round's work, a
/// metre or so across for a few tens of milliseconds, then the puff it throws off the plate (paint,
/// scale and dust, grey; a perforation's darker and longer, the compartment venting through the
/// hole). `strength` about 1 for a 76-88 mm round. Readable through a sight at combat ranges.
pub fn strike_flash(origin: Vec3, normal: Vec3, strength: f32, perforated: bool, age: f32, wind: Vec2, seed: u32, eye: Vec3, focal_px: f32) -> Option<Plume> {
    let life = if perforated { 7.0 } else { 3.0 };
    if age > life || age < 0.0 { return None; }
    let rmax = (0.75 * strength).clamp(0.4, 1.4);
    let o = origin + normal * (rmax * 0.5);
    let mut p = Plume::default();
    p.a = [o.x, o.y, o.z, KIND_FIREBALL];
    p.b = [wind.x, 0.0, wind.y, rmax];
    p.c = [1.4, if perforated { 3.0 } else { 1.6 }, age, (seed % 100_000) as f32];
    p.d = [life, 0.0, if perforated { 0.14 } else { 0.42 }, 0.0];
    let r = Vec3::splat(rmax * (2.0 + 0.4 * age) + 1.0);
    let rise = Vec3::new(wind.x * age * 0.25, age.min(12.0) * (1.8 + 0.12 * rmax) + rmax, wind.y * age * 0.25);
    p.set_bounds(o.min(o + rise) - r, o.max(o + rise) + r);
    p.d[3] = steps_for(2.0 * r.x, p.distance(eye), focal_px);
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

pub const KIND_SHOCK: f32 = 3.0;

/// A blast's pressure front at `radius` (moving at the speed of sound): the snow it lifts off the
/// ground in a thin band just behind it, and while the overpressure is high (tens of kPa, within
/// some metres of a large charge) a brief condensation shell in the cold humid air. The refraction
/// of the front itself is not drawn on this backend (named in the ledger). `ground_y` is the ground
/// under the origin.
pub fn shock(origin: Vec3, ground_y: f32, radius: f32, max_radius: f32, tnt_kg: f32, seed: u32, eye: Vec3, focal_px: f32) -> Option<Plume> {
    if !(radius > 0.2 && radius < max_radius) || tnt_kg <= 0.0 { return None; }
    let kpa = crate::pressure_front::overpressure_kpa(tnt_kg, radius);
    if kpa < 1.0 { return None; }
    let mut p = Plume::default();
    p.a = [origin.x, ground_y, origin.z, KIND_SHOCK];
    p.b = [0.0, origin.y - ground_y, 0.0, radius];
    p.c = [kpa, 0.0, 0.0, (seed % 100_000) as f32];
    p.d = [max_radius, 0.0, 0.85, 0.0];
    let r = radius + 1.5;
    p.set_bounds(Vec3::new(origin.x - r, ground_y - 0.5, origin.z - r), Vec3::new(origin.x + r, origin.y + r, origin.z + r));
    p.d[3] = steps_for(2.0 * r, p.distance(eye), focal_px).max(16.0);
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
    let roots: Vec<_> = fire.roots.iter().filter(|r| r.strength > 0.05).collect();
    if roots.is_empty() { return; }
    for i in 0..count {
        let id = (i as u32).wrapping_mul(0x9e37_79b9) ^ fire.seed;
        // Most embers burn out low and soon; a few ride the plume high (life skewed short).
        let life = 0.7 + 3.8 * hash01(id ^ 0x51).powf(2.5);
        let clock = time / life + hash01(id ^ 0x77);
        let generation = clock.floor() as u32;
        let age_t = clock.fract();
        let age = age_t * life;
        let spawn = id ^ generation.wrapping_mul(0x85eb_ca6b);
        let root = roots[(spawn as usize) % roots.len()];
        let (offset, radius) = (root.pos - fire.base + root.normal * 0.1, root.radius * 1.5);
        let a = hash01(spawn ^ 0x13) * std::f32::consts::TAU;
        let r = hash01(spawn ^ 0x29).sqrt() * radius;
        // Lofted at its own speed (1.5-5 m/s), slowing as it leaves the hot core.
        let rise = (1.5 + 3.5 * hash01(spawn ^ 0x3d)) * age * (1.0 - 0.3 * age_t) + flame_h * 0.2;
        // Its own turbulent wander: two incommensurate sways per axis at random rates and phases,
        // growing as the square root of its age (a random walk), so no two share a path and the
        // cloud has no envelope.
        let sway = |salt: u32| {
            let f1 = 0.7 + 2.5 * hash01(spawn ^ salt);
            let f2 = 1.9 + 3.1 * hash01(spawn ^ salt.wrapping_mul(3));
            let p1 = hash01(spawn ^ salt.wrapping_mul(5)) * 6.283;
            let p2 = hash01(spawn ^ salt.wrapping_mul(7)) * 6.283;
            (age * f1 + p1).sin() * 0.65 + (age * f2 + p2).sin() * 0.35
        };
        let amp = 0.5 * age.sqrt() * (0.4 + 1.2 * hash01(spawn ^ 0x61));
        let wander = Vec3::new(sway(0x101) * amp, 0.0, sway(0x202) * amp);
        let drift = Vec3::new(wind.x, 0.0, wind.y) * age * (0.4 + 0.6 * hash01(spawn ^ 0x41));
        let pos = fire.base + offset + Vec3::new(a.cos() * r, rise, a.sin() * r) + wander + drift;
        let kelvin = 1250.0 - 450.0 * age_t;
        let chroma = flame_colour(kelvin);
        let fade = (1.0 - age_t).powf(2.0) * smooth(0.0, 0.06, age_t);
        out.push(Ember { pos, colour: chroma.map(|c| c.min(1.0)), intensity: 0.7 * fade * (0.2 + 0.8 * hash01(spawn ^ 0x55).powi(2)) });
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

/// The flame's displayed colour (linear sRGB, red 1) at `kelvin`: dull red at 900 K, red-orange,
/// orange at 1,300 K, deep orange-yellow at 1,500 K, yellow only at 1,750 K and above. Authored to how a sooty flame
/// reads by eye; the three-band Planck chroma (`incandescence::planck_chroma`) put so little in
/// green that the display's tone curve turned the flame pinkish.
pub fn flame_colour(kelvin: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 5] = [(900.0, [1.0, 0.07, 0.01]), (1100.0, [1.0, 0.17, 0.02]),
        (1300.0, [1.0, 0.30, 0.035]), (1500.0, [1.0, 0.45, 0.07]), (1750.0, [1.0, 0.66, 0.20])];
    let k = kelvin.clamp(STOPS[0].0, STOPS[4].0);
    let i = STOPS.iter().rposition(|(t, _)| *t <= k).unwrap_or(0).min(3);
    let (t0, c0) = STOPS[i];
    let (t1, c1) = STOPS[i + 1];
    let f = ((k - t0) / (t1 - t0)).clamp(0.0, 1.0);
    std::array::from_fn(|c| c0[c] + (c1[c] - c0[c]) * f)
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
        let chroma = flame_colour(kelvin);
        // Unit luminance, times the Planck photopic weight relative to 1500 K compressed to its
        // square root (the full weight spans 200x from the dull edges to the core and the core
        // washed white; authored).
        let y = 0.2126 * chroma[0] + 0.7152 * chroma[1] + 0.0722 * chroma[2];
        let w = ((photopic(kelvin) / reference) as f32).sqrt() / y;
        format!("vec3<f32>({:?}, {:?}, {:?})", chroma[0] * w, chroma[1] * w, chroma[2] * w)
    }).collect();
    let tint = flame_colour(1350.0);
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
