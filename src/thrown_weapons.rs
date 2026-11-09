//! Thrown weapons of the Soviet infantry (the owner, 9 November 2026): hand grenades and petrol
//! bombs. Renderer-free: the throw solution, the flight under gravity with bounces off the
//! surfaces the caller sweeps, the fuse, and the moment a bottle breaks. What a burst or a broken
//! bottle does is the caller's (`game::swarm_runtime`: the existing blast rules, the crew's
//! casualty rules, the surface fire).
//!
//! Approximations (each in docs/PHYSICAL_TRUTH_AUDIT.md, "Infantry swarm"):
//! - no air drag (for a 0.6 kg grenade at 15 m/s the drag is about 2 % of its weight);
//! - a bounce is a restitution/friction pair per surface class, not a contact solve;
//! - the fuse is one hashed time inside the published range;
//! - a bottle breaks on hard contact above `BOTTLE_BREAK_M_S` (estimate), not by a glass model.
use glam::Vec3;

pub const GRAVITY: f32 = 9.81;
/// A thrower's release speed (m/s): a grenade thrown overarm at 15-20 m/s reaches the 35-40 m
/// published for the RGD-33 at 45 degrees (estimate inside that range).
pub const MAX_THROW_SPEED: f32 = 18.0;
/// Below this impact speed a bottle does not break on steel or earth (m/s; estimate).
pub const BOTTLE_BREAK_M_S: f32 = 4.0;
/// A body slower than this after a bounce comes to rest (m/s).
const REST_M_S: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Kind {
    /// RGD-33 stick grenade with its fragmentation sleeve: about 0.75 kg, 85 g TNT, a 3.2-3.8 s
    /// fuse (published figures).
    Rgd33,
    /// F-1 defensive grenade: about 0.6 kg, 60 g TNT, a 3.2-4.2 s fuse (published figures).
    F1,
    /// A petrol bottle with a burning rag (the "Molotov"): about 0.5 l of petrol, 0.37 kg, in a
    /// 0.4 kg bottle; no fuse, it burns when it breaks.
    PetrolBomb,
}

impl Kind {
    pub fn mass_kg(self) -> f32 {
        match self { Kind::Rgd33 => 0.75, Kind::F1 => 0.6, Kind::PetrolBomb => 0.77 }
    }
    /// Explosive filler (kg TNT); none for the bottle.
    pub fn filler_kg(self) -> f32 {
        match self { Kind::Rgd33 => 0.085, Kind::F1 => 0.060, Kind::PetrolBomb => 0.0 }
    }
    /// Petrol carried (kg).
    pub fn petrol_kg(self) -> f32 {
        if self == Kind::PetrolBomb { 0.37 } else { 0.0 }
    }
    /// The fuse's published range (s).
    pub fn fuse_range_s(self) -> Option<(f32, f32)> {
        match self { Kind::Rgd33 => Some((3.2, 3.8)), Kind::F1 => Some((3.2, 4.2)), Kind::PetrolBomb => None }
    }
    /// Fragments for the casualty rays (count, mass kg each, speed m/s): the RGD-33 sleeve's
    /// notched band and the F-1's cast body; about 1,000-1,500 m/s for cased grenades (published
    /// orders of magnitude), sampled sparsely (a game count, named).
    pub fn fragments(self) -> (u32, f32, f32) {
        match self { Kind::Rgd33 => (48, 0.0012, 1_100.0), Kind::F1 => (48, 0.0025, 900.0), Kind::PetrolBomb => (0, 0.0, 0.0) }
    }
    pub fn name(self) -> &'static str {
        match self { Kind::Rgd33 => "RGD-33", Kind::F1 => "F-1", Kind::PetrolBomb => "petrol bomb" }
    }
}

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 16; x = x.wrapping_mul(0x7FEB_352D); x ^= x >> 15; x = x.wrapping_mul(0x846C_A68B); x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

/// The fuse for throw `seed`, inside the kind's published range.
pub fn fuse_s(kind: Kind, seed: u32) -> Option<f32> {
    kind.fuse_range_s().map(|(a, b)| a + (b - a) * hash01(seed))
}

/// The release velocity that lands at `to` from `from` on the lower of the two arcs at the least
/// speed that reaches it, or `None` beyond `max_speed`. Exact for a drag-free point mass.
pub fn throw_velocity(from: Vec3, to: Vec3, max_speed: f32) -> Option<Vec3> {
    let d = to - from;
    let horizontal = glam::Vec2::new(d.x, d.z);
    let x = horizontal.length();
    let y = d.y;
    if x < 0.05 { return (y.abs() < 2.0).then_some(Vec3::new(0.0, (2.0 * GRAVITY * y.max(0.0)).sqrt(), 0.0)); }
    // Minimum speed to reach (x, y): v² = g (y + sqrt(x² + y²)).
    let v_min2 = GRAVITY * (y + (x * x + y * y).sqrt());
    let v2 = v_min2.max(0.0) * 1.05 + 0.5; // a little over the minimum: the lower, flatter arc
    if v2 > max_speed * max_speed { return None; }
    let v = v2.sqrt();
    let disc = v2 * v2 - GRAVITY * (GRAVITY * x * x + 2.0 * y * v2);
    if disc < 0.0 { return None; }
    let tan = (v2 - disc.sqrt()) / (GRAVITY * x);
    let cos = 1.0 / (1.0 + tan * tan).sqrt();
    let dir = horizontal / x;
    Some(Vec3::new(dir.x * v * cos, v * cos * tan, dir.y * v * cos))
}

/// A thrower's aim error: a deterministic offset growing with range (σ about 6 % of the range;
/// estimate for a throw at a vehicle).
pub fn aim_error(range_m: f32, seed: u32) -> Vec3 {
    let a = hash01(seed) * std::f32::consts::TAU;
    let r = 0.06 * range_m * (hash01(seed ^ 0x5bd1_e995) * 2.0).min(1.6);
    Vec3::new(a.cos() * r, 0.0, a.sin() * r)
}

/// What a sweep of the caller's world found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub point: Vec3,
    pub normal: Vec3,
    /// Fraction along the swept segment.
    pub fraction: f32,
    pub surface: Surface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Surface {
    /// The Tiger's steel (outside or inside).
    Steel,
    /// Earth, rubble, the world.
    Ground,
}

impl Surface {
    /// (restitution, tangential retention) per bounce (estimates).
    fn bounce(self) -> (f32, f32) {
        match self { Surface::Steel => (0.35, 0.7), Surface::Ground => (0.12, 0.45) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Struck a surface and bounced (or came to rest on it).
    Struck { point: Vec3, surface: Surface, speed_m_s: f32 },
    /// The fuse burnt out here.
    Burst { point: Vec3 },
    /// A bottle broke here on this surface.
    Shattered { point: Vec3, normal: Vec3, surface: Surface },
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Thrown {
    pub id: u32,
    pub kind: Kind,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Seconds of fuse left (grenades).
    pub fuse_s: Option<f32>,
    /// The soldier who threw it.
    pub owner: usize,
    pub resting: bool,
    /// The last surface it touched was the Tiger's steel.
    pub on_steel: bool,
    pub done: bool,
}

impl Thrown {
    pub fn new(id: u32, kind: Kind, pos: Vec3, vel: Vec3, owner: usize) -> Self {
        Thrown { id, kind, pos, vel, fuse_s: fuse_s(kind, id), owner, resting: false, on_steel: false, done: false }
    }

    /// Advance `dt` against the caller's sweep (from, to) -> nearest contact. At most a few
    /// bounces per step; returns the events in order. A finished body returns nothing.
    pub fn step(&mut self, dt: f32, mut sweep: impl FnMut(Vec3, Vec3) -> Option<Contact>) -> Vec<Event> {
        let mut events = Vec::new();
        if self.done || !dt.is_finite() || dt <= 0.0 { return events; }
        if let Some(f) = self.fuse_s.as_mut() {
            *f -= dt;
            if *f <= 0.0 {
                self.done = true;
                events.push(Event::Burst { point: self.pos });
                return events;
            }
        }
        if self.resting { return events; }
        let mut remaining = dt;
        for _ in 0..3 {
            if remaining <= 1e-6 { break; }
            let next_vel = self.vel - Vec3::Y * GRAVITY * remaining;
            let next = self.pos + (self.vel + next_vel) * 0.5 * remaining;
            match sweep(self.pos, next) {
                Some(c) => {
                    let n = c.normal.normalize_or_zero();
                    let speed = self.vel.length();
                    self.pos = c.point + n * 0.03;
                    self.on_steel = c.surface == Surface::Steel;
                    if self.kind == Kind::PetrolBomb && speed >= BOTTLE_BREAK_M_S {
                        self.done = true;
                        events.push(Event::Shattered { point: c.point, normal: n, surface: c.surface });
                        return events;
                    }
                    events.push(Event::Struck { point: c.point, surface: c.surface, speed_m_s: speed });
                    let (e, keep) = c.surface.bounce();
                    let vn = self.vel.dot(n);
                    let tangential = self.vel - n * vn;
                    self.vel = tangential * keep - n * vn.min(0.0) * e;
                    remaining *= 1.0 - c.fraction.clamp(0.0, 1.0);
                    if self.vel.length() < REST_M_S && n.y > 0.5 {
                        self.vel = Vec3::ZERO;
                        self.resting = true;
                        // A bottle that rolls to rest unbroken lies there (its rag may still be
                        // burning; it is not ignited by this model, named).
                        break;
                    }
                }
                None => {
                    self.pos = next;
                    self.vel = next_vel;
                    remaining = 0.0;
                }
            }
        }
        // A body that ever leaves the world (fell through) stops.
        if !self.pos.is_finite() { self.done = true; }
        events
    }
}

/// Unit directions for a burst's fragment rays: a deterministic Fibonacci sphere, rotated by
/// the seed so two bursts do not share rays.
pub fn fragment_directions(count: u32, seed: u32) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let spin = hash01(seed) * std::f32::consts::TAU;
    (0..count).map(|i| {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / count as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = golden * i as f32 + spin;
        Vec3::new(a.cos() * r, y, a.sin() * r)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_ground(from: Vec3, to: Vec3) -> Option<Contact> {
        if from.y >= 0.0 && to.y < 0.0 {
            let f = from.y / (from.y - to.y);
            Some(Contact { point: from.lerp(to, f), normal: Vec3::Y, fraction: f, surface: Surface::Ground })
        } else { None }
    }

    #[test]
    fn throw_solution_lands_on_target_without_drag_and_refuses_out_of_reach() {
        let from = Vec3::new(0.0, 1.8, 0.0);
        for to in [Vec3::new(12.0, 2.0, 5.0), Vec3::new(-20.0, 0.0, -8.0), Vec3::new(3.0, 2.4, 0.0)] {
            let v = throw_velocity(from, to, MAX_THROW_SPEED).expect("in reach");
            assert!(v.length() <= MAX_THROW_SPEED + 1e-3);
            // Integrate the exact parabola to the target's horizontal distance.
            let hx = glam::Vec2::new(to.x - from.x, to.z - from.z).length();
            let vx = glam::Vec2::new(v.x, v.z).length();
            let t = hx / vx;
            let y = from.y + v.y * t - 0.5 * GRAVITY * t * t;
            assert!((y - to.y).abs() < 0.02, "{to:?}: lands at {y}");
            assert!(v.y > 0.0);
        }
        assert!(throw_velocity(from, Vec3::new(60.0, 0.0, 0.0), MAX_THROW_SPEED).is_none());
    }

    #[test]
    fn fuse_times_stay_inside_the_published_ranges_and_a_bottle_has_none() {
        for seed in 0..200 {
            let r = fuse_s(Kind::Rgd33, seed).unwrap();
            assert!((3.2..=3.8).contains(&r));
            let f = fuse_s(Kind::F1, seed).unwrap();
            assert!((3.2..=4.2).contains(&f));
        }
        assert!(fuse_s(Kind::PetrolBomb, 3).is_none());
    }

    #[test]
    fn grenade_bounces_comes_to_rest_and_bursts_when_its_fuse_runs_out() {
        let v = throw_velocity(Vec3::new(0.0, 1.8, 0.0), Vec3::new(15.0, 0.0, 0.0), MAX_THROW_SPEED).unwrap();
        let mut g = Thrown::new(7, Kind::Rgd33, Vec3::new(0.0, 1.8, 0.0), v, 0);
        let fuse = g.fuse_s.unwrap();
        let mut t = 0.0;
        let mut struck = 0;
        let mut burst = None;
        while t < 6.0 && burst.is_none() {
            for e in g.step(1.0 / 60.0, flat_ground) {
                match e { Event::Struck { .. } => struck += 1, Event::Burst { point } => burst = Some((t, point)), _ => {} }
            }
            t += 1.0 / 60.0;
        }
        let (at, point) = burst.expect("burst");
        assert!((at - fuse).abs() < 0.05, "burst at {at}, fuse {fuse}");
        assert!(struck >= 1 && g.resting);
        assert!(point.y >= 0.0 && point.y < 0.1 && point.x > 13.0, "{point:?}");
    }

    #[test]
    fn bottle_breaks_on_a_hard_landing_and_survives_a_gentle_one() {
        let mut fast = Thrown::new(1, Kind::PetrolBomb, Vec3::new(0.0, 1.8, 0.0), Vec3::new(8.0, 4.0, 0.0), 0);
        let mut events = Vec::new();
        for _ in 0..240 { events.extend(fast.step(1.0 / 60.0, flat_ground)); if fast.done { break; } }
        assert!(matches!(events.last(), Some(Event::Shattered { surface: Surface::Ground, .. })));
        let mut slow = Thrown::new(2, Kind::PetrolBomb, Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.5, 0.0, 0.0), 0);
        for _ in 0..240 { slow.step(1.0 / 60.0, flat_ground); }
        assert!(!slow.done && slow.resting);
    }

    #[test]
    fn fragment_directions_cover_the_sphere_evenly() {
        let dirs = fragment_directions(48, 9);
        assert_eq!(dirs.len(), 48);
        assert!(dirs.iter().all(|d| (d.length() - 1.0).abs() < 1e-4));
        let mean: Vec3 = dirs.iter().copied().sum::<Vec3>() / 48.0;
        assert!(mean.length() < 0.08, "{mean:?}");
        assert_ne!(fragment_directions(48, 9)[0], fragment_directions(48, 10)[0]);
    }
}
