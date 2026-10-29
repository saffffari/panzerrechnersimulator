//! Geometric room acoustics: sound energy traced along rays through the actual scene geometry.
//!
//! This is stochastic energy ray tracing in the geometric-acoustics (high-frequency) limit, the
//! method of Krokstad, Strøm & Sørsdal (1968) and Vorländer (1989), per octave band:
//!
//! - **Emission**: `rays` directions from the source, a Fibonacci lattice under a seeded rotation
//!   (deterministic). Each ray carries 1/N of the source energy in every band.
//! - **Transport**: straight segments at the speed of sound; ISO 9613-1 air absorption per band
//!   over each segment's length (the act's air, from `sound_propagation`).
//! - **Surfaces**: every hit asks the scene for the surface's material; the ray keeps (1 − α) of
//!   its energy per band (random-incidence absorption coefficients, table below [E]), then leaves
//!   specularly, or, with the material's scattering probability, in a Lambert (cosine) direction.
//! - **Reception**: a sphere of `receiver_radius_m` around the listener's head. A segment through
//!   it deposits energy × chord length / sphere volume (the energy-density estimator), stored per
//!   1 ms bin and band, relative to the direct sound's density at the source distance, with the
//!   arrival's lateral angle relative to the head. At every reflection the scattered share s of
//!   the ray's energy is also sent straight to the receiver when a shadow ray finds it visible
//!   ("diffuse rain", Schröder & Vorländer): Lambert's cos θ / π times the sphere's solid angle;
//!   a ray that leaves a surface diffusely then counts no sphere crossing of its own, so each
//!   share is counted once. This keeps distant receivers (a source 60 m off) from depending on a
//!   handful of lucky rays.
//! - **Termination**: escape to the sky or out of the scene, `max_order` reflections,
//!   `max_time_s`, or −60 dB in every band.
//!
//! What it is not: no diffraction (edges and the open hatch rim pass no energy around corners
//! beyond what Maekawa barrier loss in `sound_propagation` already takes off the direct path), no
//! transmission through plates (the hull's insertion loss is SL §8's, applied to the dry path), no
//! modal behaviour (a 10 m³ steel box has modes below ~100 Hz the ray model smooths over), no
//! phase. Early reflections are rendered as band noise of the traced energy and direction per bin,
//! not as discrete image-source taps.
//!
//! From the echogram, `StereoIr::synthesize` builds a binaural impulse response of the reflected
//! field (band-filtered deterministic noise shaped by each band's traced energy envelope, split
//! between the ears by the energy-weighted lateral angle of the arrivals) and `StereoIr::convolve`
//! applies it to a voice by FFT. The direct path is the caller's (`acoustics::spatial`).

use std::cell::RefCell;
use std::f32::consts::{PI, TAU};

use glam::Vec3;

use crate::sound_propagation::{Bands, BANDS, OCTAVES_HZ};

use super::SAMPLE_RATE;

/// Echogram bin width, s.
pub const BIN_S: f32 = 0.001;

// ── Materials ────────────────────────────────────────────────────────────────────────────────────

/// Surface classes the scene can report. Absorption is random-incidence, per octave band
/// 31.5 Hz–8 kHz; values are typical handbook figures for the class (Kuttruff, Vorländer
/// tables; snow after Albert & Orcutt / CRREL measurements) and are estimates for the game's
/// surfaces [E].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    /// Rolled armour and heavy painted steel: the hull and turret walls.
    Armor,
    /// The cabin's machinery, racks, breech and fittings: steel with dense small features.
    Machinery,
    /// Fresh, uncompacted snow.
    Snow,
    /// Packed or wind-crusted snow and ice.
    PackedSnow,
    /// Frozen soil, tracked mud, ploughed ground.
    Ground,
    /// Brick and plastered masonry.
    Masonry,
    /// Timber walls, floors and fences.
    Timber,
    /// Rubble heaps: brick fragments with voids.
    Rubble,
    /// A wrecked vehicle's outside: steel with burnt paint.
    Wreck,
}

impl Material {
    pub const ALL: [Material; 9] = [Material::Armor, Material::Machinery, Material::Snow,
        Material::PackedSnow, Material::Ground, Material::Masonry, Material::Timber,
        Material::Rubble, Material::Wreck];

    pub fn absorption(self) -> Bands {
        match self {
            Self::Armor => [0.01, 0.01, 0.01, 0.01, 0.02, 0.02, 0.02, 0.03, 0.03],
            Self::Machinery => [0.02, 0.03, 0.04, 0.05, 0.06, 0.07, 0.08, 0.09, 0.10],
            Self::Snow => [0.10, 0.20, 0.35, 0.55, 0.75, 0.85, 0.90, 0.90, 0.90],
            Self::PackedSnow => [0.02, 0.03, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.30],
            Self::Ground => [0.05, 0.10, 0.15, 0.25, 0.40, 0.55, 0.60, 0.60, 0.60],
            Self::Masonry => [0.02, 0.02, 0.03, 0.03, 0.03, 0.04, 0.05, 0.07, 0.07],
            Self::Timber => [0.10, 0.15, 0.15, 0.11, 0.10, 0.07, 0.06, 0.07, 0.07],
            Self::Rubble => [0.05, 0.10, 0.20, 0.30, 0.40, 0.45, 0.50, 0.50, 0.50],
            Self::Wreck => [0.01, 0.02, 0.02, 0.03, 0.04, 0.05, 0.06, 0.07, 0.07],
        }
    }

    /// Probability that a reflection leaves diffusely (the scattering coefficient at mid band).
    pub fn scattering(self) -> f32 {
        match self {
            Self::Armor => 0.10,
            Self::Machinery => 0.60,
            Self::Snow | Self::PackedSnow => 0.30,
            Self::Ground => 0.50,
            Self::Masonry => 0.20,
            Self::Timber => 0.15,
            Self::Rubble => 0.80,
            Self::Wreck => 0.40,
        }
    }
}

// ── Scene ────────────────────────────────────────────────────────────────────────────────────────

/// One ray–surface intersection.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    /// Along the ray, m.
    pub distance: f32,
    /// Surface normal facing the air the ray arrives through (a scene with two-sided triangles
    /// orients it against the ray; a voxel or terrain normal already faces the air).
    pub normal: Vec3,
    pub material: Material,
}

/// What the tracer needs from the world: the nearest surface along a ray. `dir` is unit length.
pub trait AcousticScene {
    fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit>;
}

/// Axis-aligned boxes, either solid (rays reflect off their outside) or hollow (rays inside
/// reflect off their inner walls, through any `openings`): the tests' shoebox rooms and a scene
/// helper for box-shaped world cover.
#[derive(Clone, Debug, Default)]
pub struct BoxScene {
    pub solids: Vec<(Vec3, Vec3, Material)>,
    pub rooms: Vec<Room>,
}

/// A hollow box. An opening is a rectangle on one wall through which rays leave.
#[derive(Clone, Debug)]
pub struct Room {
    pub min: Vec3,
    pub max: Vec3,
    pub material: Material,
    /// (axis 0..3, side 0 = min / 1 = max, rectangle min/max in the two other axes).
    pub openings: Vec<(usize, usize, [f32; 2], [f32; 2])>,
}

pub fn ray_box_entry(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    let mut t_near = f32::NEG_INFINITY;
    let mut t_far = f32::INFINITY;
    let mut normal = Vec3::ZERO;
    for axis in 0..3 {
        let (o, d, lo, hi) = (o[axis], d[axis], min[axis], max[axis]);
        if d.abs() < 1.0e-12 {
            if o < lo || o > hi { return None; }
            continue;
        }
        let (mut t0, mut t1) = ((lo - o) / d, (hi - o) / d);
        let mut n = Vec3::ZERO;
        n[axis] = -d.signum();
        if t0 > t1 { std::mem::swap(&mut t0, &mut t1); }
        if t0 > t_near { t_near = t0; normal = n; }
        t_far = t_far.min(t1);
        if t_near > t_far { return None; }
    }
    (t_near > 1.0e-5).then_some((t_near, normal))
}

impl Room {
    /// Exit of a ray from inside: distance, inward normal, and whether it leaves through an
    /// opening.
    fn exit(&self, o: Vec3, d: Vec3) -> Option<(f32, Vec3, bool)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for axis in 0..3 {
            if d[axis].abs() < 1.0e-12 { continue; }
            let (side, plane) = if d[axis] > 0.0 { (1, self.max[axis]) } else { (0, self.min[axis]) };
            let t = (plane - o[axis]) / d[axis];
            if t > 1.0e-5 && best.is_none_or(|(b, _, _)| t < b) { best = Some((t, axis, side)); }
        }
        let (t, axis, side) = best?;
        let p = o + d * t;
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        let through = self.openings.iter().any(|&(ax, sd, ra, rb)| ax == axis && sd == side
            && p[a] >= ra[0] && p[a] <= ra[1] && p[b] >= rb[0] && p[b] <= rb[1]);
        let mut n = Vec3::ZERO;
        n[axis] = if side == 1 { -1.0 } else { 1.0 };
        Some((t, n, through))
    }

    fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
}

impl AcousticScene for BoxScene {
    fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        let mut consider = |hit: Hit| {
            if hit.distance <= max_distance && best.is_none_or(|b| hit.distance < b.distance) {
                best = Some(hit);
            }
        };
        for &(min, max, material) in &self.solids {
            if let Some((t, normal)) = ray_box_entry(origin, dir, min, max) {
                consider(Hit { distance: t, normal, material });
            }
        }
        for room in &self.rooms {
            if room.contains(origin) {
                if let Some((t, normal, through)) = room.exit(origin, dir) {
                    if !through { consider(Hit { distance: t, normal, material: room.material }); }
                }
            } else if let Some((t, normal)) = ray_box_entry(origin, dir, room.min, room.max) {
                consider(Hit { distance: t, normal, material: room.material });
            }
        }
        best
    }
}

/// A flat ground plane at `y` under an open sky, plus any other scene.
pub struct OverGround<'a, S: AcousticScene + ?Sized> {
    pub y: f32,
    pub material: Material,
    pub rest: &'a S,
}

impl<S: AcousticScene + ?Sized> AcousticScene for OverGround<'_, S> {
    fn first_hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<Hit> {
        let ground = (dir.y < -1.0e-6).then(|| (self.y - origin.y) / dir.y)
            .filter(|t| *t > 1.0e-5 && *t <= max_distance)
            .map(|t| Hit { distance: t, normal: Vec3::Y, material: self.material });
        let other = self.rest.first_hit(origin, dir, max_distance);
        match (ground, other) {
            (Some(g), Some(o)) => Some(if g.distance < o.distance { g } else { o }),
            (g, o) => g.or(o),
        }
    }
}

// ── Voxel solids ─────────────────────────────────────────────────────────────────────────────────

/// Geometry for acoustic rays at a resolution sound resolves: the source triangles rasterized into
/// an occupancy grid (`cell` m, 5 cm = λ at 6.9 kHz), dilated by one cell so a 26-connected
/// surface cannot leak a ray between diagonal cells. A hit's normal is the smoothed occupancy
/// gradient over the 3×3×3 neighbourhood, not the voxel face, so slanted plates reflect along
/// their slope. Named limits: features under one cell are merged or lost (they scatter, which
/// the material's scattering coefficient carries); every wall stands one cell thicker into the air.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelSolid {
    pub min: Vec3,
    pub cell: f32,
    pub dims: [usize; 3],
    bits: Vec<u64>,
}

impl VoxelSolid {
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    pub fn filled(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 { return false; }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] { return false; }
        let i = self.index(x, y, z);
        self.bits[i >> 6] >> (i & 63) & 1 == 1
    }

    fn set(&mut self, x: i32, y: i32, z: i32) {
        if x < 0 || y < 0 || z < 0 { return; }
        let (x, y, z) = (x as usize, y as usize, z as usize);
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] { return; }
        let i = self.index(x, y, z);
        self.bits[i >> 6] |= 1 << (i & 63);
    }

    pub fn occupied_cells(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Rasterize triangles (sampled at a third of a cell, so every crossed cell is marked), then
    /// dilate by one cell.
    pub fn from_triangles(triangles: &[[Vec3; 3]], cell: f32) -> Option<Self> {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for t in triangles { for v in t { if v.is_finite() { lo = lo.min(*v); hi = hi.max(*v); } } }
        if !lo.is_finite() || !hi.is_finite() { return None; }
        let min = lo - Vec3::splat(cell * 2.0);
        let size = hi - lo + Vec3::splat(cell * 4.0);
        let dims = [(size.x / cell).ceil() as usize + 1, (size.y / cell).ceil() as usize + 1, (size.z / cell).ceil() as usize + 1];
        let total = dims[0] * dims[1] * dims[2];
        if total > 64_000_000 { return None; }
        let mut raw = Self { min, cell, dims, bits: vec![0; total.div_ceil(64)] };
        let cell_of = |p: Vec3| -> [i32; 3] {
            let q = (p - min) / cell;
            [q.x.floor() as i32, q.y.floor() as i32, q.z.floor() as i32]
        };
        let step = cell / 3.0;
        for t in triangles {
            let [a, b, c] = *t;
            if !(a.is_finite() && b.is_finite() && c.is_finite()) { continue; }
            let longest = a.distance(b).max(b.distance(c)).max(c.distance(a));
            let n = ((longest / step).ceil() as i32).max(1);
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                    let p = a + (b - a) * u + (c - a) * v;
                    let [x, y, z] = cell_of(p);
                    raw.set(x, y, z);
                }
            }
        }
        // Dilate by one cell (6-neighbourhood).
        let mut out = raw.clone();
        for z in 0..dims[2] as i32 { for y in 0..dims[1] as i32 { for x in 0..dims[0] as i32 {
            if raw.filled(x, y, z) {
                for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                    out.set(x + dx, y + dy, z + dz);
                }
            }
        }}}
        Some(out)
    }

    fn normal_at(&self, c: [i32; 3], fallback: Vec3) -> Vec3 {
        let mut g = Vec3::ZERO;
        for dz in -1..=1 { for dy in -1..=1 { for dx in -1..=1 {
            if (dx, dy, dz) != (0, 0, 0) && self.filled(c[0] + dx, c[1] + dy, c[2] + dz) {
                g += Vec3::new(dx as f32, dy as f32, dz as f32);
            }
        }}}
        let n = (-g).normalize_or_zero();
        if n == Vec3::ZERO || n.dot(fallback) < -0.2 { fallback } else { n }
    }

    /// First occupied cell along the ray (local frame), within `max_distance`: distance and normal.
    pub fn hit(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<(f32, Vec3)> {
        let size = Vec3::new(self.dims[0] as f32, self.dims[1] as f32, self.dims[2] as f32) * self.cell;
        let (t_enter, t_exit) = {
            let mut t0 = 0.0f32;
            let mut t1 = max_distance;
            for axis in 0..3 {
                let (o, d, lo, hi) = (origin[axis], dir[axis], self.min[axis], self.min[axis] + size[axis]);
                if d.abs() < 1.0e-12 {
                    if o < lo || o > hi { return None; }
                } else {
                    let (mut a, mut b) = ((lo - o) / d, (hi - o) / d);
                    if a > b { std::mem::swap(&mut a, &mut b); }
                    t0 = t0.max(a);
                    t1 = t1.min(b);
                    if t0 > t1 { return None; }
                }
            }
            (t0, t1)
        };
        let start = origin + dir * (t_enter + 1.0e-5);
        let q = (start - self.min) / self.cell;
        let mut c = [q.x.floor() as i32, q.y.floor() as i32, q.z.floor() as i32];
        let step = [dir.x.signum() as i32, dir.y.signum() as i32, dir.z.signum() as i32];
        let mut t_max = [0.0f32; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for axis in 0..3 {
            if dir[axis].abs() > 1.0e-12 {
                let boundary = self.min[axis] + (c[axis] + i32::from(step[axis] > 0)) as f32 * self.cell;
                t_max[axis] = (boundary - origin[axis]) / dir[axis];
                t_delta[axis] = self.cell / dir[axis].abs();
            } else {
                t_max[axis] = f32::INFINITY;
            }
        }
        let mut t = t_enter;
        let mut face = Vec3::ZERO;
        // The first cell may be the one the origin sits in (a reflection leaving a wall): skip
        // cells the ray starts inside, so it can leave the surface it was reflected from.
        let mut leaving = self.filled(c[0], c[1], c[2]) && t_enter <= 1.0e-4;
        loop {
            let occupied = self.filled(c[0], c[1], c[2]);
            if occupied && !leaving && t > 1.0e-4 {
                return Some((t, self.normal_at(c, face)));
            }
            if !occupied { leaving = false; }
            let axis = if t_max[0] < t_max[1] { if t_max[0] < t_max[2] { 0 } else { 2 } }
                else if t_max[1] < t_max[2] { 1 } else { 2 };
            t = t_max[axis];
            if t > t_exit { return None; }
            c[axis] += step[axis];
            t_max[axis] += t_delta[axis];
            face = Vec3::ZERO;
            face[axis] = -step[axis] as f32;
        }
    }
}

// ── Tracing ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct TraceSettings {
    pub rays: u32,
    pub max_order: u32,
    pub max_time_s: f32,
    pub receiver_radius_m: f32,
    pub speed_m_s: f32,
    /// ISO 9613-1 attenuation per band, dB/m.
    pub air_db_per_m: Bands,
    pub seed: u32,
}

impl TraceSettings {
    /// Inside the vehicle: a 0.25 m head sphere, 0.6 s, up to 120 reflections.
    pub fn cabin(rays: u32, speed_m_s: f32, air_db_per_m: Bands, seed: u32) -> Self {
        Self { rays, max_order: 120, max_time_s: 0.6, receiver_radius_m: 0.25, speed_m_s,
            air_db_per_m, seed }
    }

    /// Outdoors: a receiver sphere growing with the source distance (bias for variance: a
    /// fixed 0.25 m sphere would catch almost no rays from a source 300 m off), 1.5 s.
    pub fn outdoor(rays: u32, source_distance_m: f32, speed_m_s: f32, air_db_per_m: Bands,
        seed: u32) -> Self
    {
        Self { rays, max_order: 12, max_time_s: 1.5,
            receiver_radius_m: (0.04 * source_distance_m).clamp(1.0, 8.0), speed_m_s,
            air_db_per_m, seed }
    }
}

/// The traced energy response at the listener.
#[derive(Clone, Debug)]
pub struct Echogram {
    pub bin_s: f32,
    /// Reflected energy per bin and band, relative to the direct sound's energy.
    pub energy: Vec<Bands>,
    /// Energy-weighted sum of the arrivals' signed lateral sine (right positive), per bin.
    pub lateral: Vec<f32>,
    /// Total (all-band) energy per bin, the weight for `lateral`.
    pub weight: Vec<f32>,
    pub direct_distance_m: f32,
    pub rays: u32,
    /// Share of rays that left the scene (to the sky, out of an open hatch).
    pub escaped: f32,
    pub reflections: u64,
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

fn unit_hash(a: u32, b: u32) -> f32 {
    (hash_u32(a ^ hash_u32(b.wrapping_add(0x9E37_79B9))) >> 8) as f32 / (1u32 << 24) as f32
}

/// Orthonormal basis around `n`.
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let a = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let t = n.cross(a).normalize();
    (t, n.cross(t))
}

/// Ray `i` of `n` on a Fibonacci sphere under a seeded rotation.
fn emission_direction(i: u32, n: u32, seed: u32) -> Vec3 {
    let golden = PI * (3.0 - 5f32.sqrt());
    let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
    let r = (1.0 - y * y).max(0.0).sqrt();
    let phi = golden * i as f32 + TAU * unit_hash(seed, 0xF1B0);
    let v = Vec3::new(r * phi.cos(), y, r * phi.sin());
    let tilt = PI * unit_hash(seed, 0x711);
    let (s, c) = tilt.sin_cos();
    Vec3::new(v.x, v.y * c - v.z * s, v.y * s + v.z * c)
}

/// A trace in progress: rays can be spent a batch at a time (the game spends a fixed number per
/// simulation update, so results are deterministic and no frame stalls on a whole trace).
pub struct TraceJob {
    settings: TraceSettings,
    source: Vec3,
    listener: Vec3,
    right: Vec3,
    next: u32,
    escaped: u32,
    gram: Echogram,
}

impl TraceJob {
    /// From `source` to a listener at `listener` whose head faces `forward` (horizontal).
    pub fn new(source: Vec3, listener: Vec3, forward: Vec3, settings: &TraceSettings) -> Self {
        let bins = ((settings.max_time_s / BIN_S).ceil() as usize).max(1);
        let flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let right = if flat == Vec3::ZERO { Vec3::X } else { flat.cross(Vec3::Y) };
        Self {
            settings: *settings,
            source,
            listener,
            right,
            next: 0,
            escaped: 0,
            gram: Echogram { bin_s: BIN_S, energy: vec![[0.0; BANDS]; bins], lateral: vec![0.0; bins],
                weight: vec![0.0; bins], direct_distance_m: source.distance(listener),
                rays: settings.rays.max(1), escaped: 0.0, reflections: 0 },
        }
    }

    pub fn done(&self) -> bool {
        self.next >= self.settings.rays.max(1)
    }

    /// Rays traced so far.
    pub fn progress(&self) -> u32 {
        self.next
    }

    /// Trace up to `budget` more rays; true once every ray is done.
    pub fn advance(&mut self, scene: &(impl AcousticScene + ?Sized), budget: u32) -> bool {
        let settings = self.settings;
        let rays = settings.rays.max(1);
        let bins = self.gram.energy.len();
        let r = settings.receiver_radius_m.max(0.01);
        let volume = 4.0 / 3.0 * PI * r * r * r;
        let direct = self.gram.direct_distance_m.max(r);
        let geometry = 4.0 * PI * direct * direct / volume;
        let c = settings.speed_m_s.max(1.0);
        let max_path = settings.max_time_s * c;
        let end = (self.next.saturating_add(budget)).min(rays);
        for i in self.next..end {
            let mut origin = self.source;
            let mut dir = emission_direction(i, rays, settings.seed);
            let mut energy = [1.0 / rays as f32; BANDS];
            let mut travelled = 0.0f32;
            let mut order = 0u32;
            let mut grazes = 0u32;
            let mut diffuse_leg = false;
            loop {
                let remaining = max_path - travelled;
                if remaining <= 0.0 { break; }
                // Ask for the nearest surface at any range: a surface beyond the time left ends
                // the ray as timed out; no surface at all is an escape (sky, an open hatch).
                let found = scene.first_hit(origin, dir, f32::INFINITY);
                let hit = found.filter(|h| h.distance <= remaining);
                let length = hit.map_or(remaining, |h| h.distance);
                // Receiver: reflected paths only (the direct path is the caller's); a diffuse leg
                // was already counted by its diffuse rain.
                if order > 0 && !diffuse_leg {
                    let to = self.listener - origin;
                    let along = to.dot(dir).clamp(0.0, length);
                    let miss = (origin + dir * along).distance(self.listener);
                    if miss < r {
                        let chord = 2.0 * (r * r - miss * miss).sqrt();
                        let path = travelled + along;
                        let bin = (path / c / BIN_S) as usize;
                        if bin < bins {
                            let mut total = 0.0;
                            for b in 0..BANDS {
                                let air = 10f32.powf(-settings.air_db_per_m[b] * (path - direct).max(0.0) / 10.0);
                                let e = energy[b] * air * chord * geometry;
                                self.gram.energy[bin][b] += e;
                                total += e;
                            }
                            self.gram.lateral[bin] += total * (-dir).dot(self.right);
                            self.gram.weight[bin] += total;
                        }
                    }
                }
                let Some(hit) = hit else {
                    self.escaped += u32::from(found.is_none());
                    break;
                };
                travelled += hit.distance;
                origin += dir * hit.distance;
                let n = hit.normal.normalize_or_zero();
                if n == Vec3::ZERO { break; }
                if dir.dot(n) >= 0.0 {
                    // A graze: the ray is already leaving the surface (normals face the air the
                    // ray travels in). Step on without reflecting.
                    origin += n * 1.0e-3 + dir * 1.0e-3;
                    grazes += 1;
                    if grazes > 64 { break; }
                    continue;
                }
                order += 1;
                self.gram.reflections += 1;
                if order > settings.max_order { break; }
                let alpha = hit.material.absorption();
                let mut strongest = 0.0f32;
                for b in 0..BANDS {
                    energy[b] *= 1.0 - alpha[b];
                    strongest = strongest.max(energy[b] * rays as f32);
                }
                if strongest < 1.0e-6 { break; }
                // Diffuse rain: the scattered share straight to the receiver, if it is in view.
                let s_share = hit.material.scattering();
                let to = self.listener - origin;
                let dist = to.length();
                if s_share > 0.0 && dist > r {
                    let toward = to / dist;
                    let cos_out = toward.dot(n);
                    if cos_out > 0.0 {
                        let start = origin + n * 1.0e-3;
                        let blocked = scene.first_hit(start, toward, dist - r).is_some_and(|h| h.distance < dist - r);
                        if !blocked {
                            let omega = (PI * r * r / (dist * dist)).min(2.0 * PI);
                            let fraction = s_share * cos_out / PI * omega;
                            let path = travelled + dist;
                            let bin = (path / c / BIN_S) as usize;
                            if bin < bins {
                                let mut total = 0.0;
                                for b in 0..BANDS {
                                    let air = 10f32.powf(-settings.air_db_per_m[b] * (path - direct).max(0.0) / 10.0);
                                    let e = energy[b] * fraction * air * (4.0 / 3.0 * r) * geometry;
                                    self.gram.energy[bin][b] += e;
                                    total += e;
                                }
                                self.gram.lateral[bin] += total * (-toward).dot(self.right);
                                self.gram.weight[bin] += total;
                            }
                        }
                    }
                }
                let key = i.wrapping_mul(0x2545_F491) ^ order.wrapping_mul(0x9E37_79B9) ^ settings.seed;
                let incoming = dir;
                diffuse_leg = unit_hash(key, 1) < hit.material.scattering();
                dir = if diffuse_leg {
                    // Lambert: cosine-weighted about the normal.
                    let (u1, u2) = (unit_hash(key, 2), unit_hash(key, 3));
                    let (t, b) = basis(n);
                    let radius = u1.sqrt();
                    let phi = TAU * u2;
                    (t * radius * phi.cos() + b * radius * phi.sin() + n * (1.0 - u1).max(0.0).sqrt()).normalize()
                } else {
                    (dir - 2.0 * dir.dot(n) * n).normalize()
                };
                // Restart in air: back along the incoming path (always the side it came from)
                // and off the surface.
                origin += n * 1.0e-3 - incoming * 1.0e-3;
            }
        }
        self.next = end;
        self.gram.escaped = self.escaped as f32 / self.next.max(1) as f32;
        self.done()
    }

    /// The echogram of the rays traced so far (complete once `done`).
    pub fn echogram(&self) -> &Echogram {
        &self.gram
    }

    pub fn into_echogram(self) -> Echogram {
        self.gram
    }
}

/// Trace from `source` to a listener at `listener` whose head faces `forward` (horizontal).
pub fn trace(scene: &(impl AcousticScene + ?Sized), source: Vec3, listener: Vec3, forward: Vec3,
    settings: &TraceSettings) -> Echogram
{
    let mut job = TraceJob::new(source, listener, forward, settings);
    job.advance(scene, u32::MAX);
    job.into_echogram()
}

impl Echogram {
    /// Reflected energy relative to the direct sound, per band, dB.
    pub fn reflected_db(&self) -> Bands {
        std::array::from_fn(|b| {
            let e: f32 = self.energy.iter().map(|bin| bin[b]).sum();
            10.0 * e.max(1.0e-12).log10()
        })
    }

    /// Reverberation time per band from the Schroeder backward integral, T20 (−5 to −25 dB)
    /// extrapolated to 60 dB; `None` where the band never decays 25 dB inside the trace.
    pub fn decay_times_s(&self) -> [Option<f32>; BANDS] {
        std::array::from_fn(|b| {
            let mut edc: Vec<f32> = self.energy.iter().map(|bin| bin[b]).collect();
            for i in (0..edc.len().saturating_sub(1)).rev() { edc[i] += edc[i + 1]; }
            let start = edc.iter().position(|e| *e > 0.0)?;
            let total = edc[start];
            if total <= 0.0 { return None; }
            let level = |i: usize| 10.0 * (edc[i] / total).max(1.0e-12).log10();
            let t5 = (start..edc.len()).find(|&i| level(i) <= -5.0)?;
            let t25 = (t5..edc.len()).find(|&i| level(i) <= -25.0)?;
            Some(3.0 * (t25 - t5) as f32 * self.bin_s)
        })
    }

    /// Energy-weighted lateral sine of everything reflected (right positive).
    pub fn mean_lateral(&self) -> f32 {
        let w: f32 = self.weight.iter().sum();
        if w <= 0.0 { 0.0 } else { self.lateral.iter().sum::<f32>() / w }
    }

    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "rays": self.rays, "escaped": self.escaped, "reflections": self.reflections,
            "direct_distance_m": self.direct_distance_m,
            "reflected_db": self.reflected_db(),
            "t20_s": self.decay_times_s(),
            "mean_lateral": self.mean_lateral(),
        })
    }
}

// ── Impulse response ─────────────────────────────────────────────────────────────────────────────

/// RBJ constant-0-dB-peak band-pass at `f0`, one octave wide, applied in place.
fn octave_bandpass(x: &mut [f32], f0: f32, rate: f32) {
    let w0 = TAU * f0.min(rate * 0.45) / rate;
    let (s, c) = w0.sin_cos();
    let bw = 1.0f32;
    let alpha = s * ((2f32.ln() / 2.0) * bw * w0 / s).sinh();
    let a0 = 1.0 + alpha;
    let (b0, b2) = (alpha / a0, -alpha / a0);
    let (a1, a2) = (-2.0 * c / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for v in x.iter_mut() {
        let y = b0 * *v + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1; x1 = *v; y2 = y1; y1 = y;
        *v = y;
    }
}

/// The reflected field as a binaural impulse response at `SAMPLE_RATE`.
pub struct StereoIr {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    spectrum: RefCell<Option<(usize, Vec<f32>, Vec<f32>)>>,
}

impl StereoIr {
    pub fn synthesize(gram: &Echogram, seed: u32) -> Self {
        let rate = SAMPLE_RATE as f32;
        let per_bin = (gram.bin_s * rate).max(1.0);
        // Trim to the last bin holding energy above −70 dB of the strongest.
        let peak = gram.weight.iter().fold(0.0f32, |m, w| m.max(*w));
        let last = gram.weight.iter().rposition(|w| *w > peak * 1.0e-7).map_or(0, |i| i + 1);
        let len = (last as f32 * per_bin) as usize;
        let mut left = vec![0.0f32; len];
        let mut right = vec![0.0f32; len];
        if len == 0 {
            return Self { left, right, spectrum: RefCell::new(None) };
        }
        let lateral: Vec<f32> = (0..last).map(|i| if gram.weight[i] > 0.0 {
            (gram.lateral[i] / gram.weight[i]).clamp(-1.0, 1.0) } else { 0.0 }).collect();
        // Unit-power white noise through a 0 dB-peak octave band-pass has unit spectral density
        // inside the band; scaled by sqrt(e / samples-per-bin) its gain in that band is the traced
        // energy ratio e of each bin, which is what convolving the direct sound needs.
        let unit = 3f32.sqrt();
        for b in 0..BANDS {
            for (ear, out) in [&mut left, &mut right].into_iter().enumerate() {
                let mut noise: Vec<f32> = (0..len).map(|n| unit * (2.0 * unit_hash(seed ^ (b as u32 * 7919
                    + ear as u32 * 104_729), n as u32) - 1.0)).collect();
                octave_bandpass(&mut noise, OCTAVES_HZ[b], rate);
                for (n, sample) in noise.iter().enumerate() {
                    let pos = n as f32 / per_bin;
                    let i = (pos as usize).min(last - 1);
                    let next = (i + 1).min(last - 1);
                    let f = pos - i as f32;
                    let e = gram.energy[i][b] * (1.0 - f) + gram.energy[next][b] * f;
                    let lat = lateral[i] * (1.0 - f) + lateral[next] * f;
                    let share = if ear == 1 { 0.5 * (1.0 + lat) } else { 0.5 * (1.0 - lat) };
                    out[n] += sample * (e * share / per_bin).max(0.0).sqrt();
                }
            }
        }
        Self { left, right, spectrum: RefCell::new(None) }
    }

    pub fn len(&self) -> usize {
        self.left.len()
    }

    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    pub fn energy(&self) -> f32 {
        self.left.iter().chain(&self.right).map(|v| v * v).sum()
    }

    /// The voice `mono` through the reflected field, interleaved stereo of length
    /// `mono.len() + len() − 1`. One complex FFT carries both ears: H = FFT(hL + i·hR); since
    /// the voice is real, the inverse transform of X·H holds x∗hL in its real part and x∗hR in
    /// its imaginary part.
    pub fn convolve(&self, mono: &[f32]) -> Vec<f32> {
        if mono.is_empty() || self.is_empty() {
            return Vec::new();
        }
        let out_len = mono.len() + self.len() - 1;
        let size = out_len.next_power_of_two();
        {
            let mut cache = self.spectrum.borrow_mut();
            if cache.as_ref().is_none_or(|(n, _, _)| *n != size) {
                let mut re = vec![0.0f32; size];
                let mut im = vec![0.0f32; size];
                re[..self.len()].copy_from_slice(&self.left);
                im[..self.len()].copy_from_slice(&self.right);
                fft(&mut re, &mut im, false);
                *cache = Some((size, re, im));
            }
        }
        let cache = self.spectrum.borrow();
        let (_, hr, hi) = cache.as_ref().expect("spectrum");
        let mut xr = vec![0.0f32; size];
        let mut xi = vec![0.0f32; size];
        xr[..mono.len()].copy_from_slice(mono);
        fft(&mut xr, &mut xi, false);
        for k in 0..size {
            let (a, b) = (xr[k], xi[k]);
            xr[k] = a * hr[k] - b * hi[k];
            xi[k] = a * hi[k] + b * hr[k];
        }
        fft(&mut xr, &mut xi, true);
        let mut out = Vec::with_capacity(out_len * 2);
        for n in 0..out_len {
            out.push(xr[n]);
            out.push(xi[n]);
        }
        out
    }
}

/// In-place iterative radix-2 FFT (`re.len()` a power of two); the inverse is scaled by 1/N.
pub fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j |= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    let mut len = 2;
    while len <= n {
        let angle = if inverse { TAU } else { -TAU } / len as f32;
        let (ws, wc) = (angle as f64).sin_cos();
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] as f64 * cr - im[b] as f64 * ci, re[b] as f64 * ci + im[b] as f64 * cr);
                re[b] = (re[a] as f64 - tr) as f32;
                im[b] = (im[a] as f64 - ti) as f32;
                re[a] = (re[a] as f64 + tr) as f32;
                im[a] = (im[a] as f64 + ti) as f32;
                let next = cr * wc - ci * ws;
                ci = cr * ws + ci * wc;
                cr = next;
            }
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / n as f32;
        for v in re.iter_mut().chain(im.iter_mut()) { *v *= scale; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_AIR: Bands = [0.0; BANDS];

    fn shoebox(size: Vec3, material: Material) -> BoxScene {
        BoxScene { solids: Vec::new(), rooms: vec![Room { min: Vec3::ZERO, max: size, material,
            openings: Vec::new() }] }
    }

    /// Eyring's reverberation time for a room of uniform absorption (the diffuse-field limit a
    /// scattering ray tracer converges to).
    fn eyring(size: Vec3, alpha: f32) -> f32 {
        let v = size.x * size.y * size.z;
        let s = 2.0 * (size.x * size.y + size.y * size.z + size.z * size.x);
        0.161 * v / (-s * (1.0 - alpha).ln())
    }

    #[test]
    fn a_shoebox_room_decays_at_eyrings_rate() {
        let size = Vec3::new(5.0, 3.0, 4.0);
        // Masonry at 1 kHz (0.04) is too slow to trace cheaply; rubble (0.45) is a fast room.
        let scene = shoebox(size, Material::Rubble);
        let settings = TraceSettings { rays: 3000, max_order: 400, max_time_s: 1.0,
            receiver_radius_m: 0.5, speed_m_s: 343.0, air_db_per_m: NO_AIR, seed: 7 };
        let gram = trace(&scene, Vec3::new(1.2, 1.4, 1.1), Vec3::new(3.6, 1.6, 2.7), Vec3::Z, &settings);
        assert_eq!(gram.escaped, 0.0, "a closed room loses no rays");
        let t = gram.decay_times_s();
        for b in [4usize, 5, 6] {
            let want = eyring(size, Material::Rubble.absorption()[b]);
            let got = t[b].expect("decays");
            assert!((got / want - 1.0).abs() < 0.2, "{} Hz: traced {got} s vs Eyring {want} s", OCTAVES_HZ[b]);
        }
        // Lower absorption decays more slowly.
        assert!(t[2].unwrap() > t[6].unwrap());
    }

    #[test]
    fn a_job_spent_in_batches_equals_one_whole_trace() {
        let scene = shoebox(Vec3::new(2.0, 1.6, 2.4), Material::Machinery);
        let settings = TraceSettings::cabin(300, 343.0, NO_AIR, 8);
        let (s, l) = (Vec3::new(0.5, 0.8, 0.5), Vec3::new(1.4, 1.0, 1.8));
        let whole = trace(&scene, s, l, Vec3::Z, &settings);
        let mut job = TraceJob::new(s, l, Vec3::Z, &settings);
        let mut steps = 0;
        while !job.advance(&scene, 37) { steps += 1; }
        assert!(steps >= 7);
        assert_eq!(job.echogram().energy, whole.energy);
        assert_eq!(job.echogram().escaped, whole.escaped);
    }

    #[test]
    fn tracing_is_deterministic() {
        let scene = shoebox(Vec3::new(2.0, 1.6, 2.4), Material::Machinery);
        let settings = TraceSettings::cabin(400, 343.0, NO_AIR, 3);
        let a = trace(&scene, Vec3::new(0.5, 0.8, 0.5), Vec3::new(1.4, 1.0, 1.8), Vec3::Z, &settings);
        let b = trace(&scene, Vec3::new(0.5, 0.8, 0.5), Vec3::new(1.4, 1.0, 1.8), Vec3::Z, &settings);
        assert_eq!(a.energy, b.energy);
        assert_eq!(a.lateral, b.lateral);
    }

    #[test]
    fn an_open_hatch_lets_energy_out_and_shortens_the_decay() {
        let size = Vec3::new(2.0, 1.8, 2.6);
        let closed = shoebox(size, Material::Machinery);
        let mut open = closed.clone();
        open.rooms[0].openings.push((1, 1, [0.3, 1.7], [0.5, 2.1]));
        let settings = TraceSettings::cabin(1500, 343.0, NO_AIR, 11);
        let (s, l) = (Vec3::new(0.4, 0.9, 0.5), Vec3::new(1.3, 1.2, 1.9));
        let gc = trace(&closed, s, l, Vec3::Z, &settings);
        let go = trace(&open, s, l, Vec3::Z, &settings);
        assert_eq!(gc.escaped, 0.0);
        assert!(go.escaped > 0.05, "rays leave through the hatch ({})", go.escaped);
        let (ec, eo) = (gc.reflected_db()[6], go.reflected_db()[6]);
        assert!(eo < ec - 2.0, "less reverberant energy with the hatch open: {eo} vs {ec} dB");
        let (tc, to) = (gc.decay_times_s()[6].unwrap(), go.decay_times_s()[6].unwrap());
        assert!(to < tc, "decay {to} s open vs {tc} s closed");
    }

    #[test]
    fn a_steel_cabin_rings_longer_than_the_same_cabin_lined_with_machinery() {
        let size = Vec3::new(2.0, 1.8, 2.6);
        let settings = TraceSettings::cabin(1200, 343.0, NO_AIR, 5);
        let (s, l) = (Vec3::new(0.4, 0.9, 0.5), Vec3::new(1.3, 1.2, 1.9));
        let bare = trace(&shoebox(size, Material::Armor), s, l, Vec3::Z, &settings);
        let lined = trace(&shoebox(size, Material::Machinery), s, l, Vec3::Z, &settings);
        assert!(bare.reflected_db()[6] > lined.reflected_db()[6]);
    }

    #[test]
    fn open_ground_returns_one_early_reflection_and_lets_the_rest_go() {
        let empty = BoxScene::default();
        let ground = OverGround { y: 0.0, material: Material::PackedSnow, rest: &empty };
        let settings = TraceSettings::outdoor(4000, 40.0, 319.0, NO_AIR, 9);
        let gram = trace(&ground, Vec3::new(0.0, 1.5, 40.0), Vec3::new(0.0, 1.7, 0.0), Vec3::Z, &settings);
        assert!(gram.escaped > 0.45, "most energy leaves to the sky ({})", gram.escaped);
        let first = gram.weight.iter().position(|w| *w > 0.0).expect("ground reflection");
        let direct_s = 40.0 / 319.0;
        assert!((first as f32 * BIN_S - direct_s).abs() < 0.02, "arrives just after the direct ({first} ms)");
        let total: f32 = gram.weight.iter().sum();
        let late: f32 = gram.weight.iter().skip(first + 60).sum();
        assert!(late < total * 0.01, "almost nothing late over bare ground ({late} of {total})");
        let snow = OverGround { y: 0.0, material: Material::Snow, rest: &empty };
        let soft = trace(&snow, Vec3::new(0.0, 1.5, 40.0), Vec3::new(0.0, 1.7, 0.0), Vec3::Z, &settings);
        assert!(soft.reflected_db()[7] < gram.reflected_db()[7] - 5.0, "fresh snow swallows the 4 kHz reflection");
    }

    /// The game's convention (`bearing_deg_from`): right = forward × up, so a head facing +Z has
    /// its right ear toward −X.
    #[test]
    fn a_wall_on_the_right_puts_the_reflections_on_the_right() {
        assert_eq!(Vec3::Z.cross(Vec3::Y), Vec3::NEG_X);
        let wall = BoxScene { solids: vec![(Vec3::new(-5.0, 0.0, -20.0), Vec3::new(-4.0, 6.0, 20.0), Material::Masonry)],
            rooms: Vec::new() };
        let ground = OverGround { y: 0.0, material: Material::Snow, rest: &wall };
        let settings = TraceSettings::outdoor(4000, 30.0, 319.0, NO_AIR, 4);
        let gram = trace(&ground, Vec3::new(0.0, 1.5, 30.0), Vec3::new(0.0, 1.7, 0.0), Vec3::Z, &settings);
        // The wall's image source lies 15 degrees right of the source (sin = 0.26); the ground
        // reflection arrives from the median plane and dilutes the mean.
        assert!(gram.mean_lateral() > 0.1, "wall on the right: lateral {}", gram.mean_lateral());
    }

    /// A voxelized shoebox (its six walls as triangles) behaves like the analytic room: sealed,
    /// and decaying at Eyring's rate within the same tolerance.
    #[test]
    fn a_voxelized_room_is_sealed_and_decays_like_the_analytic_one() {
        let size = Vec3::new(3.0, 2.0, 2.5);
        let quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| [[a, b, c], [a, c, d]];
        let (x, y, z) = (size.x, size.y, size.z);
        let p = |i: f32, j: f32, k: f32| Vec3::new(i, j, k);
        let mut tris = Vec::new();
        tris.extend(quad(p(0.,0.,0.), p(x,0.,0.), p(x,0.,z), p(0.,0.,z)));
        tris.extend(quad(p(0.,y,0.), p(x,y,0.), p(x,y,z), p(0.,y,z)));
        tris.extend(quad(p(0.,0.,0.), p(0.,y,0.), p(0.,y,z), p(0.,0.,z)));
        tris.extend(quad(p(x,0.,0.), p(x,y,0.), p(x,y,z), p(x,0.,z)));
        tris.extend(quad(p(0.,0.,0.), p(x,0.,0.), p(x,y,0.), p(0.,y,0.)));
        tris.extend(quad(p(0.,0.,z), p(x,0.,z), p(x,y,z), p(0.,y,z)));
        let grid = VoxelSolid::from_triangles(&tris, 0.05).expect("grid");
        struct Grid(VoxelSolid);
        impl AcousticScene for Grid {
            fn first_hit(&self, o: Vec3, d: Vec3, max: f32) -> Option<Hit> {
                self.0.hit(o, d, max.min(50.0)).map(|(t, n)| Hit { distance: t, normal: n, material: Material::Rubble })
            }
        }
        let scene = Grid(grid);
        let settings = TraceSettings { rays: 2000, max_order: 300, max_time_s: 0.6, receiver_radius_m: 0.4,
            speed_m_s: 343.0, air_db_per_m: NO_AIR, seed: 5 };
        let gram = trace(&scene, Vec3::new(0.8, 0.9, 0.7), Vec3::new(2.1, 1.1, 1.8), Vec3::Z, &settings);
        assert_eq!(gram.escaped, 0.0, "no ray leaks through the voxel walls");
        // The air is one dilated cell smaller on every side.
        let inner = size - Vec3::splat(0.2);
        let want = eyring(inner, Material::Rubble.absorption()[5]);
        let got = gram.decay_times_s()[5].expect("decays");
        assert!((got / want - 1.0).abs() < 0.25, "voxel room {got} s vs Eyring {want} s");
        // A slanted wall's normal follows its slope, not the voxel face.
        let slope = [[p(0.,0.,0.), p(4.,4.,0.), p(4.,4.,4.)], [p(0.,0.,0.), p(4.,4.,4.), p(0.,0.,4.)]];
        let g = VoxelSolid::from_triangles(&slope, 0.05).unwrap();
        let (_, n) = g.hit(p(3.0, 0.5, 2.0), Vec3::NEG_X, 10.0).expect("hits the slope");
        assert!(n.dot(Vec3::new(1.0, -1.0, 0.0).normalize()) > 0.85, "normal {n:?}");
    }

    #[test]
    fn fft_round_trips_and_convolution_matches_the_direct_sum() {
        let mut re: Vec<f32> = (0..64).map(|i| ((i * 37) % 11) as f32 - 5.0).collect();
        let mut im = vec![0.0f32; 64];
        let original = re.clone();
        fft(&mut re, &mut im, false);
        fft(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&original) { assert!((a - b).abs() < 1.0e-4); }
        let ir = StereoIr { left: vec![0.5, 0.0, -0.25], right: vec![0.0, 1.0, 0.0], spectrum: RefCell::new(None) };
        let x = [1.0f32, 2.0, -1.0, 0.5];
        let y = ir.convolve(&x);
        let direct = |h: &[f32], n: usize| (0..h.len()).filter(|&k| n >= k && n - k < x.len()).map(|k| h[k] * x[n - k]).sum::<f32>();
        for n in 0..x.len() + 2 {
            assert!((y[n * 2] - direct(&ir.left, n)).abs() < 1.0e-5);
            assert!((y[n * 2 + 1] - direct(&ir.right, n)).abs() < 1.0e-5);
        }
    }

    /// A steady 1 kHz tone through the synthesized response gains the traced 1 kHz energy, and
    /// a wall on the listener's right (−X for a head facing +Z) feeds the right ear.
    #[test]
    fn the_impulse_response_carries_the_traced_energy_and_side() {
        let wall = BoxScene { solids: vec![(Vec3::new(-4.0, 0.0, -10.0), Vec3::new(-3.0, 5.0, 10.0), Material::Masonry)],
            rooms: Vec::new() };
        let ground = OverGround { y: 0.0, material: Material::Ground, rest: &wall };
        let settings = TraceSettings::outdoor(3000, 20.0, 319.0, NO_AIR, 2);
        let gram = trace(&ground, Vec3::new(2.0, 1.5, 20.0), Vec3::new(0.0, 1.7, 0.0), Vec3::Z, &settings);
        let ir = StereoIr::synthesize(&gram, 1);
        assert!(ir.left.iter().chain(&ir.right).all(|v| v.is_finite()));
        let traced: f32 = gram.energy.iter().map(|bin| bin[5]).sum();
        let rate = SAMPLE_RATE as f32;
        let tone: Vec<f32> = (0..(rate as usize)).map(|n| (TAU * 1000.0 * n as f32 / rate).sin() * 2f32.sqrt()).collect();
        let out = ir.convolve(&tone);
        let (a, b) = (ir.len() + 100, tone.len() - 100);
        let power: f32 = (a..b).map(|n| out[n * 2].powi(2) + out[n * 2 + 1].powi(2)).sum::<f32>() / (b - a) as f32;
        assert!(power > traced * 0.6 && power < traced * 2.2, "1 kHz gain {power} vs traced {traced}");
        let (l, r): (f32, f32) = (ir.left.iter().map(|v| v * v).sum(), ir.right.iter().map(|v| v * v).sum());
        assert!(r > l * 1.2, "the wall's side (L={l} R={r})");
    }
}


#[cfg(test)]
mod cost {
    use super::*;
    /// Prints what one event's convolution costs (release build), which sets the game's
    /// per-update convolution budget.
    #[test]
    #[ignore = "timing print; run in release"]
    fn convolution_cost_of_a_four_second_report() {
        let gram = Echogram { bin_s: BIN_S, energy: vec![[0.01; BANDS]; 1500], lateral: vec![0.0; 1500],
            weight: vec![0.09; 1500], direct_distance_m: 50.0, rays: 1, escaped: 0.0, reflections: 0 };
        let ir = StereoIr::synthesize(&gram, 1);
        let voice: Vec<f32> = (0..SAMPLE_RATE as usize * 4).map(|n| ((n * 7919) % 101) as f32 / 101.0 - 0.5).collect();
        let _ = ir.convolve(&voice);
        let start = std::time::Instant::now();
        for _ in 0..5 { let _ = ir.convolve(&voice); }
        println!("4 s voice through a 1.5 s IR: {:.1} ms per event", start.elapsed().as_secs_f64() * 1e3 / 5.0);
    }
}
