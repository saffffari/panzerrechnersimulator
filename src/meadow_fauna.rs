//! The meadow's insects (docs/PHYSICAL_TRUTH_AUDIT.md "Spring meadow, butterflies and
//! fireflies"): butterflies by day over the flower clumps, fireflies at night in swarms. Every
//! decision is a counter-seeded integer hash of a world cell, an individual and a time bin
//! (`meadow::hash`), so the same instant of the same map shows the same insects.
//!
//! Butterflies: drawn as two real wing meshes each (raster only, no shadow), flapping at 9-11 Hz
//! (small pierids and nymphalids beat at about 8-12 Hz; Dudley 2000, The Biomechanics of Insect
//! Flight), flying 1.4-2.6 m/s in short erratic bouts of 2-10 s (a heading that turns by a new
//! hashed amount every quarter second) and landing on the nearest flower head of the meadow's own
//! placement (`meadow::flower_at`) to feed for 3-15 s. A behavioural model, not aerodynamics.
//!
//! Fireflies: tiny modeled emitters. Spectrum: firefly emission maxima span 552-582 nm (Seliger &
//! McElroy 1964, PNAS 52:75; Photinus pyralis in vivo near 560-562 nm), here a Gaussian peaking at
//! 560 nm with 22 / 24 nm half-widths (about 54 nm full width at half maximum, an estimate of the
//! published curves' shape). Power: 1e11 photons per flash, the top of the 1e8-1e11 that Silver
//! (2026, "How bright is a firefly? Resolving a century of overestimation", Am. J. Phys.,
//! doi:10.1119/5.0325834) finds after correcting Coblentz's 1912 1/50-1/400 candle (his stated
//! ceiling: every luciferase turning over once with unit yield), emitted isotropically over a
//! 0.12 s sin^2 pulse: about 3e-5 cd at the peak. Flash pattern: Photinus carolinus, the measured
//! clustered flasher: in a group of more than 15 males, synchronous flashes every 0.55 s in bursts,
//! the bursts repeating about every 12 s, the flashes of the swarm within a few tens of ms
//! (Sarfati, Hayes, Peleg 2020 J. R. Soc. Interface 17:20200179; 2021 Sci. Adv. 7:eabg9259);
//! smaller groups and lone males burst at irregular intervals of a few seconds to minutes. Swarms
//! drift slowly over the grass and rise from the grass top toward 2 m as civil dusk deepens; they
//! fly only with the Sun more than 3 degrees below the horizon. Not a species of the lower Volga:
//! the measured clustered behaviour stands in for the local Luciola and Lampyris (named).
use glam::{Mat4, Quat, Vec2, Vec3};

use crate::meadow::{self, hash, hash2, unit, value_noise};

// ── Firefly light ────────────────────────────────────────────────────────────────────────────

pub const FIREFLY_PEAK_NM: f64 = 560.0;
pub const FIREFLY_SIGMA_SHORT_NM: f64 = 22.0;
pub const FIREFLY_SIGMA_LONG_NM: f64 = 24.0;
pub const PHOTONS_PER_FLASH: f64 = 1.0e11;
pub const FLASH_S: f32 = 0.12;
pub const SYNC_FLASH_PERIOD_S: f32 = 0.55;
pub const BURST_PERIOD_S: f32 = 12.0;
pub const SYNC_JITTER_S: f32 = 0.03;
pub const SYNC_GROUP_MIN: u32 = 16;
const PLANCK: f64 = 6.626_070_15e-34;
const LIGHT: f64 = 2.997_924_58e8;

fn lobe(l: f64, mu: f64, s1: f64, s2: f64) -> f64 {
    let s = if l < mu { s1 } else { s2 };
    (-0.5 * ((l - mu) / s).powi(2)).exp()
}
/// CIE 1931 2-degree colour matching functions, the multi-lobe fit of Wyman, Sloan & Shirley
/// 2013 (JCGT 2(2)), accurate to about 1 % of their peaks.
pub fn cie_xyz(l: f64) -> [f64; 3] {
    [1.056 * lobe(l, 599.8, 37.9, 31.0) + 0.362 * lobe(l, 442.0, 16.0, 26.7) - 0.065 * lobe(l, 501.1, 20.4, 26.2),
     0.821 * lobe(l, 568.8, 46.9, 40.5) + 0.286 * lobe(l, 530.9, 16.3, 31.1),
     1.217 * lobe(l, 437.0, 11.8, 36.0) + 0.681 * lobe(l, 459.0, 26.0, 13.8)]
}
/// The lantern's relative spectral power (peak 1).
pub fn firefly_spectrum(l: f64) -> f64 { lobe(l, FIREFLY_PEAK_NM, FIREFLY_SIGMA_SHORT_NM, FIREFLY_SIGMA_LONG_NM) }

/// The lantern's light in the renderer's three lanes: (colour with its largest lane 1, lane
/// intensity per W/sr of radiant intensity, photons per joule). The narrow 560 nm emission
/// enters as its display-equivalent triplet (CIE XYZ to linear sRGB primaries, the small negative
/// blue clipped) scaled so its photopic luminance (observer::PHOTOPIC_WEIGHTS) equals the
/// spectrum's own: point samples at 680/550/440 nm would put it on the 550 lane alone and show a
/// pure green, not the yellow-green the eye sees. Named in the ledger.
pub fn firefly_lanes() -> ([f32; 3], f64, f64) {
    let (mut xyz, mut power, mut photons) = ([0.0f64; 3], 0.0, 0.0);
    let mut l = 380.0;
    while l <= 780.0 {
        let s = firefly_spectrum(l);
        let c = cie_xyz(l);
        for i in 0..3 { xyz[i] += s * c[i]; }
        power += s;
        photons += s * l * 1e-9 / (PLANCK * LIGHT);
        l += 1.0;
    }
    let [x, y, z] = xyz;
    let rgb = [3.2406 * x - 1.5372 * y - 0.4986 * z, -0.9689 * x + 1.8758 * y + 0.0415 * z, 0.0557 * x - 0.2040 * y + 1.0570 * z]
        .map(|v: f64| v.max(0.0));
    let top = rgb.iter().cloned().fold(0.0, f64::max);
    let colour = rgb.map(|v| v / top);
    // cd per W/sr of this spectrum: 683 lm/W x the V-weighted share.
    let candela_per_watt_sr = 683.0 * y / power;
    let lane_luminance: f64 = (0..3).map(|i| colour[i] * crate::observer::PHOTOPIC_WEIGHTS[i]).sum();
    (colour.map(|v| v as f32), candela_per_watt_sr / lane_luminance, photons / power)
}

/// Peak radiant intensity of one flash (W/sr): the flash's photons over the sin^2 pulse's
/// effective duration (half its length), isotropic.
pub fn flash_peak_radiant_intensity() -> f64 {
    let (_, _, photons_per_joule) = firefly_lanes();
    let energy = PHOTONS_PER_FLASH / photons_per_joule;
    energy / (FLASH_S as f64 * 0.5) / (4.0 * std::f64::consts::PI)
}
pub fn pulse(t: f32) -> f32 {
    if !(0.0..FLASH_S).contains(&t) { return 0.0; }
    let s = (std::f32::consts::PI * t / FLASH_S).sin();
    s * s
}

pub const SWARM_CELL_M: f32 = 32.0;
const SALT_SWARM: u32 = 0x5f1e_f1e5;
pub const SWARM_REACH_M: f32 = 160.0;

/// A swarm of a world cell (if the cell has one).
#[derive(Clone, Copy, Debug)]
pub struct Swarm {
    pub home: Vec2,
    pub seed: u32,
    pub members: u32,
    pub radius: f32,
}
impl Swarm {
    pub fn of_cell(cx: i32, cz: i32, grass: &dyn Fn(Vec2) -> f32) -> Option<Self> {
        let seed = hash2(cx, cz, SALT_SWARM);
        let home = (Vec2::new(cx as f32, cz as f32) + Vec2::new(0.2 + 0.6 * unit(hash(seed ^ 1)), 0.2 + 0.6 * unit(hash(seed ^ 2)))) * SWARM_CELL_M;
        let cover = grass(home);
        if cover < 0.3 || unit(seed) >= 0.8 * cover { return None; }
        Some(Swarm { home, seed, members: 8 + hash(seed ^ 3) % 72, radius: 3.5 + 4.0 * unit(hash(seed ^ 4)) })
    }
    pub fn synchronous(&self) -> bool { self.members >= SYNC_GROUP_MIN }
    /// The swarm's centre: a slow wander about its home (under 0.4 m/s).
    pub fn centre(&self, t: f32) -> Vec2 {
        let s = self.seed;
        let w = |salt: u32| (value_noise(t / 45.0, 0.5, s ^ salt) - 0.5) * 2.0;
        self.home + Vec2::new(w(11), w(12)) * 9.0
    }
    /// Member `i`'s position at `t` given the swarm's altitude above the ground and the ground.
    pub fn member(&self, i: u32, t: f32, altitude: f32, ground: &dyn Fn(f32, f32) -> f32) -> Vec3 {
        let h = hash(self.seed ^ (i + 1).wrapping_mul(0x9e37_79b1));
        let r = |k: u32| unit(hash(h ^ k));
        let w = [0.08 + 0.12 * r(1), 0.08 + 0.12 * r(2), 0.15 + 0.3 * r(3), 0.05 + 0.1 * r(4)];
        let p = [r(5), r(6), r(7), r(8)].map(|v| v * std::f32::consts::TAU);
        let c = self.centre(t);
        let off = Vec2::new((w[0] * t + p[0]).sin() * 0.75 + (w[3] * t + p[3]).sin() * 0.25,
                            (w[1] * t + p[1]).cos() * 0.75 + (w[3] * t + p[2]).cos() * 0.25) * self.radius;
        let off = if off.length() > self.radius { off * (self.radius / off.length()) } else { off };
        let x = c + off;
        let y = ground(x.x, x.y) + altitude + (w[2] * t + p[2]).sin() * 0.45 * altitude.min(1.0) + (r(9) - 0.5) * 0.4;
        Vec3::new(x.x, y.max(ground(x.x, x.y) + 0.15), x.y)
    }
    /// Member `i`'s flash level (0..1 of its peak) at `t`.
    pub fn flash(&self, i: u32, t: f32) -> f32 {
        let h = hash(self.seed ^ (i + 1).wrapping_mul(0x85eb_ca6b));
        if self.synchronous() {
            let phase = unit(hash(self.seed ^ 77)) * BURST_PERIOD_S;
            let tt = t + phase;
            let burst = (tt / BURST_PERIOD_S).floor();
            let into = tt - burst * BURST_PERIOD_S;
            let hb = hash(h ^ (burst as i32 as u32).wrapping_mul(0x27d4_eb2d));
            let first = hb % 3;
            let len = 3 + (hb >> 8) % 6;
            let slot = (into / SYNC_FLASH_PERIOD_S).floor() as u32;
            let mut level = 0.0f32;
            for s in [slot.saturating_sub(1), slot] {
                if s < first || s >= first + len { continue; }
                let jitter = (unit(hash(hb ^ s.wrapping_mul(0x165_667b1))) - 0.5) * 2.0 * SYNC_JITTER_S;
                level = level.max(pulse(into - (s as f32 * SYNC_FLASH_PERIOD_S + SYNC_JITTER_S + jitter)));
            }
            level
        } else {
            // Irregular: in each 6 s bin, with even odds, a short burst of 1-4 flashes.
            let bin = (t / 6.0).floor();
            let mut level = 0.0f32;
            for b in [bin - 1.0, bin] {
                let hb = hash(h ^ (b as i32 as u32).wrapping_mul(0x27d4_eb2d));
                if unit(hb) >= 0.5 { continue; }
                let start = b * 6.0 + unit(hash(hb ^ 5)) * 4.0;
                let flashes = 1 + hash(hb ^ 6) % 4;
                for k in 0..flashes { level = level.max(pulse(t - start - k as f32 * SYNC_FLASH_PERIOD_S)); }
            }
            level
        }
    }
}

/// How far into the night the fireflies are: 0 with the Sun above -3 deg, 1 below -9 deg.
pub fn emergence(sun_altitude_deg: f32) -> f32 { meadow::smoothstep(-3.0, -9.0, sun_altitude_deg) }

/// Every lit lantern near the eye at `t`: (position, lane intensity).
pub fn fireflies(eye: Vec3, t: f32, sun_altitude_deg: f32, grass: &dyn Fn(Vec2) -> f32, ground: &dyn Fn(f32, f32) -> f32) -> Vec<[f32; 4]> {
    let e = emergence(sun_altitude_deg);
    let mut out = Vec::new();
    if e <= 0.0 { return out; }
    let (colour, per_watt, _) = firefly_lanes();
    let _ = colour;
    let peak = (flash_peak_radiant_intensity() * per_watt) as f32;
    let altitude = 0.45 + 1.3 * e;
    let reach = (SWARM_REACH_M / SWARM_CELL_M).ceil() as i32 + 1;
    let (cx0, cz0) = ((eye.x / SWARM_CELL_M).floor() as i32, (eye.z / SWARM_CELL_M).floor() as i32);
    for cz in cz0 - reach..=cz0 + reach {
        for cx in cx0 - reach..=cx0 + reach {
            let Some(swarm) = Swarm::of_cell(cx, cz, grass) else { continue };
            if swarm.centre(t).distance(Vec2::new(eye.x, eye.z)) > SWARM_REACH_M + swarm.radius { continue; }
            for i in 0..swarm.members {
                // Members emerge one by one through dusk.
                if unit(hash(swarm.seed ^ (i + 1).wrapping_mul(0xc2b2_ae35))) >= e { continue; }
                let level = swarm.flash(i, t);
                if level <= 0.0 { continue; }
                let p = swarm.member(i, t, altitude, ground);
                out.push([p.x, p.y, p.z, peak * level]);
            }
        }
    }
    out
}

// ── Butterflies ──────────────────────────────────────────────────────────────────────────────

pub struct ButterflySpecies { pub name: &'static str, pub base: [u8; 3], pub tip: [u8; 3], pub span_m: f32 }
/// Display-equivalent wing colours (sRGB bytes of the albedo).
pub const BUTTERFLIES: [ButterflySpecies; 4] = [
    ButterflySpecies { name: "small white (Pieris rapae)", base: [228, 228, 218], tip: [52, 52, 52], span_m: 0.050 },
    ButterflySpecies { name: "brimstone (Gonepteryx rhamni)", base: [226, 220, 96], tip: [214, 206, 84], span_m: 0.058 },
    ButterflySpecies { name: "small tortoiseshell (Aglais urticae)", base: [196, 112, 36], tip: [40, 34, 30], span_m: 0.048 },
    ButterflySpecies { name: "common blue (Polyommatus icarus)", base: [110, 140, 220], tip: [200, 200, 205], span_m: 0.034 },
];
pub const BUTTERFLY_CELL_M: f32 = 12.0;
pub const BUTTERFLY_REACH_M: f32 = 40.0;
const SALT_BUTTERFLY: u32 = 0xb077_e4f1;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode { Fly, Approach(Vec3), Perch(Vec3) }

#[derive(Clone, Debug)]
pub struct Butterfly {
    pub cell: (i32, i32),
    pub index: u32,
    pub species: usize,
    pub pos: Vec3,
    pub heading: f32,
    mode: Mode,
    timer: f32,
    bout: u32,
    speed: f32,
    flap_hz: f32,
    flap_phase: f32,
    turn: f32,
    turn_timer: f32,
}

/// Nearest flower head within `reach` of p (the meadow's own placement).
pub fn nearest_flower(p: Vec2, reach: f32, grass: &dyn Fn(Vec2) -> f32, ground: &dyn Fn(f32, f32) -> f32) -> Option<Vec3> {
    let (lo, hi) = (meadow::tile_of(p - Vec2::splat(reach)), meadow::tile_of(p + Vec2::splat(reach)));
    let mut best: Option<(f32, Vec3)> = None;
    for tz in lo.1..=hi.1 {
        for tx in lo.0..=hi.0 {
            for k in 0..meadow::FLOWER_SLOTS {
                let Some(f) = meadow::flower_at(tx, tz, k, grass) else { continue };
                let d = f.root.distance(p);
                if d <= reach && best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, Vec3::new(f.root.x, ground(f.root.x, f.root.y) - 0.01 + f.stem, f.root.y)));
                }
            }
        }
    }
    best.map(|b| b.1)
}

impl Butterfly {
    fn spawn(cell: (i32, i32), index: u32, grass: &dyn Fn(Vec2) -> f32, ground: &dyn Fn(f32, f32) -> f32) -> Option<Self> {
        let h = hash(hash2(cell.0, cell.1, SALT_BUTTERFLY) ^ (index + 1).wrapping_mul(0x9e37_79b1));
        let spot = (Vec2::new(cell.0 as f32, cell.1 as f32) + Vec2::new(unit(hash(h ^ 1)), unit(hash(h ^ 2)))) * BUTTERFLY_CELL_M;
        let flower = nearest_flower(spot, 10.0, grass, ground)?;
        Some(Butterfly {
            cell, index, species: (hash(h ^ 3) % BUTTERFLIES.len() as u32) as usize, pos: flower,
            heading: unit(hash(h ^ 4)) * std::f32::consts::TAU, mode: Mode::Perch(flower), timer: 1.0 + 6.0 * unit(hash(h ^ 5)),
            bout: 0, speed: 1.4 + 1.2 * unit(hash(h ^ 6)), flap_hz: 9.0 + 2.0 * unit(hash(h ^ 7)), flap_phase: unit(hash(h ^ 8)),
            turn: 0.0, turn_timer: 0.0,
        })
    }
    fn seed(&self) -> u32 { hash(hash2(self.cell.0, self.cell.1, SALT_BUTTERFLY) ^ (self.index + 1).wrapping_mul(0x9e37_79b1)) }
    fn home(&self) -> Vec2 { (Vec2::new(self.cell.0 as f32, self.cell.1 as f32) + Vec2::splat(0.5)) * BUTTERFLY_CELL_M }

    pub fn step(&mut self, dt: f32, wind: Vec2, grass: &dyn Fn(Vec2) -> f32, ground: &dyn Fn(f32, f32) -> f32) {
        let s = self.seed() ^ self.bout.wrapping_mul(0x68e3_1da4);
        self.timer -= dt;
        self.flap_phase = (self.flap_phase + dt * self.flap_hz).fract();
        match self.mode {
            Mode::Perch(at) => {
                self.pos = at;
                // A wind over 7 m/s keeps it down.
                if self.timer <= 0.0 && wind.length() < 7.0 {
                    self.bout += 1;
                    self.mode = Mode::Fly;
                    self.timer = 2.0 + 8.0 * unit(hash(s ^ 21));
                }
            }
            Mode::Fly | Mode::Approach(_) => {
                self.turn_timer -= dt;
                if self.turn_timer <= 0.0 {
                    // A new erratic turn rate every quarter second; steer home past 15 m.
                    let k = (self.timer * 4.0) as i32 as u32;
                    self.turn = (unit(hash(s ^ k.wrapping_mul(0x2545_f491))) - 0.5) * 7.0;
                    self.turn_timer = 0.25;
                }
                let flat = Vec2::new(self.pos.x, self.pos.z);
                let mut target_heading = None;
                let mut climb_to = ground(flat.x, flat.y) + 0.7 + 0.8 * value_noise(self.timer * 0.7, self.index as f32, s);
                if let Mode::Approach(f) = self.mode {
                    let to = Vec2::new(f.x, f.z) - flat;
                    target_heading = Some(to.x.atan2(to.y));
                    climb_to = f.y + 0.05 + (to.length() * 0.15).min(1.0);
                    if to.length() < 0.08 && (self.pos.y - f.y).abs() < 0.12 {
                        self.mode = Mode::Perch(f);
                        self.pos = f;
                        self.timer = 3.0 + 12.0 * unit(hash(s ^ 23));
                        return;
                    }
                } else if flat.distance(self.home()) > 15.0 {
                    let to = self.home() - flat;
                    target_heading = Some(to.x.atan2(to.y));
                }
                if let Some(want) = target_heading {
                    let diff = (want - self.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                    self.heading += diff.clamp(-4.0 * dt, 4.0 * dt);
                    if matches!(self.mode, Mode::Approach(_)) { self.heading += self.turn * dt * 0.25; } else { self.heading += self.turn * dt * 0.5; }
                } else {
                    self.heading += self.turn * dt;
                }
                let forward = Vec2::new(self.heading.sin(), self.heading.cos());
                let speed = if matches!(self.mode, Mode::Approach(_)) { self.speed * 0.6 } else { self.speed };
                let v = forward * speed + wind * 0.3;
                // Each downstroke lifts the body a few centimetres.
                let bob = (self.flap_phase * std::f32::consts::TAU).sin() * 0.25;
                let vy = (climb_to - self.pos.y) * 2.5 + bob;
                self.pos += Vec3::new(v.x, vy, v.y) * dt;
                let floor = ground(self.pos.x, self.pos.z) + 0.2;
                if self.pos.y < floor { self.pos.y = floor; }
                if self.timer <= 0.0 && matches!(self.mode, Mode::Fly) {
                    match nearest_flower(Vec2::new(self.pos.x, self.pos.z), 10.0, grass, ground) {
                        Some(f) => self.mode = Mode::Approach(f),
                        None => self.timer = 2.0,
                    }
                }
                if matches!(self.mode, Mode::Approach(_)) && self.timer < -12.0 { self.mode = Mode::Fly; self.timer = 3.0; }
            }
        }
    }

    pub fn perched(&self) -> bool { matches!(self.mode, Mode::Perch(_)) }

    /// The two wings' transforms (left, right) of the wing mesh (hinge on the body axis +Z).
    pub fn wing_models(&self) -> [Mat4; 2] {
        let angle = if self.perched() {
            // Wings closed over the back, opening a little now and then.
            1.35 - 0.35 * ((self.flap_phase * 0.15 * std::f32::consts::TAU).sin().max(0.0))
        } else {
            0.20 + 0.95 * (self.flap_phase * std::f32::consts::TAU).sin()
        };
        let span = BUTTERFLIES[self.species].span_m / 0.056;
        let body = Mat4::from_rotation_translation(Quat::from_rotation_y(self.heading), self.pos);
        [-1.0f32, 1.0].map(|side| body * Mat4::from_rotation_z(-side * angle) * Mat4::from_scale(Vec3::new(side * span, span, span)))
    }
}

/// The butterflies alive around the eye: spawned per world cell when it comes within reach,
/// dropped when it leaves.
#[derive(Default)]
pub struct Butterflies {
    pub alive: Vec<Butterfly>,
    cells: std::collections::HashSet<(i32, i32)>,
}
impl Butterflies {
    pub fn update(&mut self, eye: Vec3, dt: f32, day: bool, wind: Vec2, grass: &dyn Fn(Vec2) -> f32, ground: &dyn Fn(f32, f32) -> f32) {
        if !day { self.alive.clear(); self.cells.clear(); return; }
        let eye2 = Vec2::new(eye.x, eye.z);
        let reach = (BUTTERFLY_REACH_M / BUTTERFLY_CELL_M).ceil() as i32;
        let (cx0, cz0) = ((eye.x / BUTTERFLY_CELL_M).floor() as i32, (eye.z / BUTTERFLY_CELL_M).floor() as i32);
        let mut want = std::collections::HashSet::new();
        for cz in cz0 - reach..=cz0 + reach {
            for cx in cx0 - reach..=cx0 + reach {
                let centre = (Vec2::new(cx as f32, cz as f32) + Vec2::splat(0.5)) * BUTTERFLY_CELL_M;
                if centre.distance(eye2) <= BUTTERFLY_REACH_M { want.insert((cx, cz)); }
            }
        }
        self.alive.retain(|b| want.contains(&b.cell));
        self.cells.retain(|c| want.contains(c));
        for &cell in &want {
            if self.cells.contains(&cell) { continue; }
            self.cells.insert(cell);
            let centre = (Vec2::new(cell.0 as f32, cell.1 as f32) + Vec2::splat(0.5)) * BUTTERFLY_CELL_M;
            let flowers: f32 = (0..meadow::SPECIES.len()).map(|s| meadow::SPECIES[s].peak * meadow::bed(s, centre.x, centre.y)).sum::<f32>() * grass(centre);
            let count = (unit(hash2(cell.0, cell.1, SALT_BUTTERFLY ^ 0xc0)) * (1.2 + flowers / 1.5)).floor().min(6.0) as u32;
            for i in 0..count {
                if let Some(b) = Butterfly::spawn(cell, i, grass, ground) { self.alive.push(b); }
            }
        }
        for b in &mut self.alive { b.step(dt, wind, grass, ground); }
    }
}

/// A wing: forewing and hindwing as two fans in the XZ plane (hinge on +Z), 5.6 cm span (both
/// wings), uv.x from the base (0) to the tip (1) for the two-colour pattern.
pub fn wing_mesh(species: usize) -> crate::gltf_load::CpuMesh {
    use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
    let fore = [Vec3::new(0.0, 0.0, 0.006), Vec3::new(0.012, 0.0, 0.014), Vec3::new(0.027, 0.0, 0.010), Vec3::new(0.026, 0.0, -0.002), Vec3::new(0.011, 0.0, -0.006)];
    let hind = [Vec3::new(0.0, 0.0, -0.002), Vec3::new(0.012, 0.0, -0.006), Vec3::new(0.017, 0.0, -0.015), Vec3::new(0.007, 0.0, -0.021), Vec3::new(0.0, 0.0, -0.012)];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for fan in [fore, hind] {
        let base = vertices.len() as u32;
        for p in fan {
            vertices.push(Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0], uv: [(p.x / 0.027).clamp(0.0, 1.0), 0.5], tangent: [1.0, 0.0, 0.0, 1.0] });
        }
        // Both faces: a wing is seen from above and below, and the mirrored wing flips winding.
        for k in 1..fan.len() as u32 - 1 { indices.extend([base, base + k, base + k + 1, base, base + k + 1, base + k]); }
    }
    let sp = &BUTTERFLIES[species];
    let texels = [sp.base, sp.base, sp.base, sp.tip].iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect();
    let mut mesh = CpuMesh::basic(vertices, indices, Some(CpuImage::new(4, 1, texels)), Vec3::new(-0.03, -0.03, -0.03), Vec3::new(0.03, 0.03, 0.03));
    mesh.roughness_factor = 0.7;
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lantern_is_yellow_green_and_its_power_is_silvers_count() {
        let (colour, per_watt, photons_per_joule) = firefly_lanes();
        assert_eq!(colour[1], 1.0, "{colour:?}");
        assert!(colour[0] > 0.4 && colour[0] < 0.7, "green-yellow wants a red share under green: {colour:?}");
        assert!(colour[2] < 0.05, "{colour:?}");
        // 560 nm photons carry 3.55e-19 J.
        assert!((1.0 / photons_per_joule - 3.55e-19).abs() < 0.05e-19, "{}", 1.0 / photons_per_joule);
        // The peak luminous intensity is tens of micro-candela: 1e11 photons in ~60 ms.
        let (_, _, _) = firefly_lanes();
        let lane = flash_peak_radiant_intensity() * per_watt;
        let candela: f64 = (0..3).map(|i| colour[i] as f64 * crate::observer::PHOTOPIC_WEIGHTS[i]).sum::<f64>() * lane;
        assert!(candela > 1e-5 && candela < 1e-4, "{candela} cd");
        // Integrating the pulse returns the flash's photon count.
        let steps = 10_000;
        let dt = FLASH_S as f64 / steps as f64;
        let peak_w = flash_peak_radiant_intensity() * 4.0 * std::f64::consts::PI;
        let photons: f64 = (0..steps).map(|k| peak_w * pulse((k as f64 + 0.5) as f32 * dt as f32) as f64 * dt).sum::<f64>() * photons_per_joule;
        assert!((photons / PHOTONS_PER_FLASH - 1.0).abs() < 0.01, "{photons}");
    }

    fn swarm(members: u32) -> Swarm { Swarm { home: Vec2::new(10.0, 10.0), seed: 0x1234, members, radius: 5.0 } }

    #[test]
    fn a_large_swarm_flashes_in_synchronous_bursts() {
        let s = swarm(30);
        assert!(s.synchronous());
        // Onsets: each member's flash starts within the jitter of a 0.55 s slot; bursts every 12 s.
        let dt = 0.002;
        let mut onsets: Vec<f32> = Vec::new();
        for i in 0..s.members {
            let mut was = 0.0;
            let mut t = 0.0;
            while t < 48.0 {
                let level = s.flash(i, t);
                if level > 0.0 && was == 0.0 { onsets.push(t); }
                was = level;
                t += dt;
            }
        }
        assert!(onsets.len() > 30 * 4 * 2, "{} flashes", onsets.len());
        let phase = unit(hash(s.seed ^ 77)) * BURST_PERIOD_S;
        for &o in &onsets {
            let into = (o + phase).rem_euclid(BURST_PERIOD_S);
            let slot_err = (into / SYNC_FLASH_PERIOD_S - (into / SYNC_FLASH_PERIOD_S).round()) * SYNC_FLASH_PERIOD_S;
            assert!(slot_err.abs() <= 2.0 * SYNC_JITTER_S + 2.0 * dt, "onset {o} off its slot by {slot_err}");
            assert!(into < 9.0 * SYNC_FLASH_PERIOD_S + 0.1, "a flash in the dark gap at {into} s into the burst");
        }
        // Dark between bursts: nobody flashes in the last 6 s of a burst period.
        for i in 0..s.members {
            for k in 0..40 {
                let t = BURST_PERIOD_S * 2.0 - phase + 6.0 + k as f32 * 0.14;
                assert_eq!(s.flash(i, t), 0.0);
            }
        }
    }

    #[test]
    fn a_small_group_flashes_irregularly_and_each_flash_is_a_short_pulse() {
        let s = swarm(8);
        assert!(!s.synchronous());
        let mut lit = 0;
        let mut t = 0.0;
        while t < 120.0 { if s.flash(0, t) > 0.0 { lit += 1; } t += 0.01; }
        assert!(lit > 10 && lit < 120 * 100 / 8, "{lit}");
        assert_eq!(pulse(-0.01), 0.0);
        assert_eq!(pulse(FLASH_S), 0.0);
        assert!((pulse(FLASH_S * 0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_swarm_moves_together() {
        let s = swarm(30);
        let ground = |_: f32, _: f32| 0.0;
        let mut last = s.centre(0.0);
        for k in 1..600 {
            let t = k as f32 * 0.5;
            let c = s.centre(t);
            assert!(c.distance(last) / 0.5 < 0.8, "the swarm darts at {t}");
            assert!(c.distance(s.home) < 9.0 * 1.5, "the swarm wanders off");
            last = c;
            for i in 0..s.members {
                let p = s.member(i, t, 1.2, &ground);
                assert!(Vec2::new(p.x, p.z).distance(c) <= s.radius * 1.01, "member {i} left its swarm");
                assert!(p.y >= 0.15 && p.y < 3.2, "{p:?}");
            }
        }
        // Fireflies only fly in the dark; they rise as dusk deepens.
        assert_eq!(emergence(5.0), 0.0);
        assert_eq!(emergence(-2.0), 0.0);
        assert_eq!(emergence(-12.0), 1.0);
    }

    #[test]
    fn no_fireflies_by_day_and_a_dark_meadow_has_some() {
        let grass = |_: Vec2| 1.0;
        let ground = |_: f32, _: f32| 0.0;
        assert!(fireflies(Vec3::ZERO, 10.0, 20.0, &grass, &ground).is_empty());
        let total: usize = (0..200).map(|k| fireflies(Vec3::ZERO, k as f32 * 0.1, -12.0, &grass, &ground).len()).sum();
        assert!(total > 0);
        let bare = |_: Vec2| 0.0;
        assert!(fireflies(Vec3::ZERO, 10.0, -12.0, &bare, &ground).is_empty());
    }

    #[test]
    fn butterflies_fly_and_land_on_flowers() {
        let grass = |_: Vec2| 1.0;
        let ground = |_: f32, _: f32| 0.0;
        let mut flock = Butterflies::default();
        flock.update(Vec3::new(0.0, 1.7, 0.0), 0.0, true, Vec2::ZERO, &grass, &ground);
        assert!(!flock.alive.is_empty(), "no butterflies over a flowering meadow");
        let mut flew = 0;
        let mut landed = 0;
        let mut was: Vec<bool> = flock.alive.iter().map(|b| b.perched()).collect();
        for _ in 0..(60 * 60) {
            flock.update(Vec3::new(0.0, 1.7, 0.0), 1.0 / 60.0, true, Vec2::new(2.0, 0.0), &grass, &ground);
            for (b, w) in flock.alive.iter().zip(was.iter_mut()) {
                if *w && !b.perched() { flew += 1; }
                if !*w && b.perched() {
                    landed += 1;
                    // Perched on a real flower head of the meadow.
                    let f = nearest_flower(Vec2::new(b.pos.x, b.pos.z), 0.01, &grass, &ground).expect("perched on a flower");
                    assert!((f - b.pos).length() < 1e-3);
                }
                *w = b.perched();
                assert!(b.pos.y > 0.15 && b.pos.y < 3.0, "{:?}", b.pos);
            }
        }
        assert!(flew > 0 && landed > 0, "flew {flew} landed {landed}");
        // Night: none.
        flock.update(Vec3::ZERO, 0.1, false, Vec2::ZERO, &grass, &ground);
        assert!(flock.alive.is_empty());
    }
}
