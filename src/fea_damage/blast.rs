//! Pressure waves (2026-11-03, the owner: one pressure-wave model for damage and for the visible
//! refraction of the air, outside the tank and inside it, each modelled as suits it).
//!
//! Outside: an analytic event field. A charge's blast is the free-air spherical wave of Kinney &
//! Graham (1985) at the scaled distance Z = r / W^1/3 (W the TNT-equivalent mass): peak
//! overpressure and positive-phase duration from their fits, arrival from the Rankine-Hugoniot
//! shock speed integrated out from the charge's own radius, and a Friedlander time history behind
//! the front with a decay coefficient of 1 (named: the fits give duration, not the shape). A burst
//! on the ground is the free-air wave of twice its charge (the ground a rigid mirror; real soil
//! gives about 1.7-1.8: named). Mach stems, terrain and building shadowing and the negative phase
//! are not modelled (named). `sound_levels::kingery_bulmash_kpa`, the surface-burst law the sound
//! uses beyond Z = 2.9, agrees with it to within 20 % there (measured: Kinney-Graham 20 % higher at
//! Z = 3, closer beyond; test); the Kinney-Graham form also
//! holds closer in, where interior bursts are.
//!
//! Inside: the compartment's own walls. The shock and its reflections reach a point as the free-
//! field waves of mirror images of the source across the walls' planes (the image-source method),
//! to second order, each wall a rigid reflector found by tracing the compartment's own triangles
//! (named: walls are the planes the probe rays met, the reflection is perfect and acoustic -
//! pressures superpose linearly, the real shock's nonlinear reflection doubling and more is not
//! modelled). The quasi-static pressure of the gas then builds on that and vents through the
//! openings (`compartment`). The two hand over at the openings: a wave arriving from outside at an
//! open hatch enters as a source there scaled by the opening's share of the wave's area (named).
//!
//! The image: air's refractive index follows its density (Gladstone-Dale, n - 1 = 2.26e-4 m³/kg
//! x rho), and the density behind a front follows its pressure (Rankine-Hugoniot at the front,
//! isentropic behind: named). `ray_deflection` measures how far the transported light is turned.
//!
//! The crew: the published thresholds for an unreinforced, unreflected wave (US Army FM 8-9, 1996,
//! para 403): eardrum rupture from about 22 kPa, half at 90-130 kPa; lung damage from about
//! 69 kPa; death possible from 193 kPa, survival recorded to 262 kPa. They are peak values with no
//! duration dependence (named: the Bowen curves' duration effect is not used).

use glam::Vec3;

/// Ambient air (Pa, kg/m³, m/s) and its ratio of specific heats.
pub const P0_PA: f32 = 101_325.0;
pub const RHO0: f32 = 1.204;
pub const C0: f32 = 343.0;
pub const GAMMA: f32 = 1.4;
/// Gladstone-Dale constant of air, m³/kg.
pub const GLADSTONE_DALE: f32 = 2.26e-4;
/// Friedlander decay coefficient (named).
pub const FRIEDLANDER_B: f32 = 1.0;
/// A TNT charge's own radius per kg^1/3 (a sphere at 1600 kg/m³), where the integration of the
/// arrival time starts.
pub const CHARGE_RADIUS_PER_CBRT_KG: f32 = 0.0530;
/// A surface burst counts as a free-air burst of this many times its charge (named).
pub const SURFACE_REFLECTION: f32 = 2.0;

/// Kinney-Graham peak overpressure (Pa) at scaled distance `z` (m/kg^1/3), free air.
pub fn peak_overpressure_pa(z: f32) -> f32 {
    let z = z.max(0.02);
    let num = 808.0 * (1.0 + (z / 4.5).powi(2));
    let den = ((1.0 + (z / 0.048).powi(2)) * (1.0 + (z / 0.32).powi(2)) * (1.0 + (z / 1.35).powi(2))).sqrt();
    P0_PA * num / den
}

/// Kinney-Graham positive-phase duration per kg^1/3 (s/kg^1/3) at scaled distance `z`.
pub fn duration_per_cbrt_kg_s(z: f32) -> f32 {
    let z = z.max(0.02);
    let num = 980.0 * (1.0 + (z / 0.54).powi(10));
    let den = (1.0 + (z / 0.02).powi(3)) * (1.0 + (z / 0.74).powi(6)) * (1.0 + (z / 6.9).powi(2)).sqrt();
    num / den * 1.0e-3
}

/// The shock front's speed (m/s) at overpressure `dp` (Rankine-Hugoniot).
pub fn shock_speed(dp: f32) -> f32 {
    C0 * (1.0 + (GAMMA + 1.0) / (2.0 * GAMMA) * dp.max(0.0) / P0_PA).sqrt()
}

/// Density behind a shock of overpressure `dp` over ambient (Rankine-Hugoniot).
pub fn shock_density_ratio(dp: f32) -> f32 {
    let p2 = P0_PA + dp.max(0.0);
    ((GAMMA + 1.0) * p2 + (GAMMA - 1.0) * P0_PA) / ((GAMMA - 1.0) * p2 + (GAMMA + 1.0) * P0_PA)
}

/// Density behind the front at gauge pressure `p` (isentropic from ambient; named).
pub fn isentropic_density_ratio(p: f32) -> f32 {
    ((P0_PA + p).max(1.0) / P0_PA).powf(1.0 / GAMMA)
}

/// Friedlander overpressure `tau` s after the front of peak `dp` and duration `td` (the positive
/// phase only; zero before the front and after the phase).
pub fn friedlander(dp: f32, td: f32, tau: f32) -> f32 {
    if tau < 0.0 || tau > td || td <= 0.0 {
        return 0.0;
    }
    let x = tau / td;
    dp * (1.0 - x) * (-FRIEDLANDER_B * x).exp()
}

/// The positive-phase impulse (Pa s) of a Friedlander wave of peak `dp` and duration `td`.
pub fn friedlander_impulse(dp: f32, td: f32) -> f32 {
    let b = FRIEDLANDER_B;
    dp * td * (1.0 / b - (1.0 - (-b).exp()) / (b * b))
}

/// A charge going off: where, its TNT equivalent, when, and whether it sits on the ground.
#[derive(Clone, Copy, Debug)]
pub struct Charge {
    pub at: Vec3,
    pub tnt_kg: f32,
    pub t0_s: f32,
    pub surface: bool,
}

/// The blast at one distance: when its front arrives, its peak, its duration and impulse.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wave {
    pub arrival_s: f32,
    pub peak_pa: f32,
    pub duration_s: f32,
    pub impulse_pa_s: f32,
}

impl Charge {
    /// The free-air mass the wave scales with.
    pub fn effective_kg(&self) -> f32 {
        (self.tnt_kg * if self.surface { SURFACE_REFLECTION } else { 1.0 }).max(1.0e-9)
    }

    /// The wave at distance `r` m from the charge.
    pub fn wave(&self, r: f32) -> Wave {
        let w3 = self.effective_kg().cbrt();
        let z = r.max(1.0e-4) / w3;
        let peak = peak_overpressure_pa(z);
        let duration = duration_per_cbrt_kg_s(z) * w3;
        Wave { arrival_s: self.t0_s + arrival_after_s(self.effective_kg(), r), peak_pa: peak, duration_s: duration,
            impulse_pa_s: friedlander_impulse(peak, duration) }
    }

    /// Overpressure (Pa) at `p` at time `t` (direct wave only).
    pub fn pressure_at(&self, p: Vec3, t: f32) -> f32 {
        let wave = self.wave(p.distance(self.at));
        friedlander(wave.peak_pa, wave.duration_s, t - wave.arrival_s)
    }
}

/// Seconds for the front of a free-air burst of `w` kg to travel `r` m (the shock speed integrated
/// out from the charge's radius; the first stretch inside the charge's radius is taken at the
/// speed there: named).
pub fn arrival_after_s(w: f32, r: f32) -> f32 {
    let w3 = w.max(1.0e-9).cbrt();
    let r0 = CHARGE_RADIUS_PER_CBRT_KG * w3;
    if r <= r0 {
        return r / shock_speed(peak_overpressure_pa(r0 / w3));
    }
    // Log-spaced trapezoid steps: the speed falls fast near the charge.
    const STEPS: usize = 96;
    let (l0, l1) = (r0.ln(), r.ln());
    let mut t = r0 / shock_speed(peak_overpressure_pa(r0 / w3));
    let mut prev_r = r0;
    let mut prev_inv = 1.0 / shock_speed(peak_overpressure_pa(r0 / w3));
    for i in 1..=STEPS {
        let ri = (l0 + (l1 - l0) * i as f32 / STEPS as f32).exp();
        let inv = 1.0 / shock_speed(peak_overpressure_pa(ri / w3));
        t += 0.5 * (inv + prev_inv) * (ri - prev_r);
        prev_r = ri;
        prev_inv = inv;
    }
    t
}

/// Air's refractive index minus one at density ratio `rho_ratio` to ambient.
pub fn refractivity(rho_ratio: f32) -> f32 {
    GLADSTONE_DALE * RHO0 * rho_ratio
}

/// Density ratio to ambient at `p` at time `t` in the field of `charges` (the front's own jump by
/// Rankine-Hugoniot, behind it isentropic).
pub fn density_ratio_at(charges: &[Charge], p: Vec3, t: f32) -> f32 {
    let mut gauge = 0.0;
    let mut front = false;
    for c in charges {
        let wave = c.wave(p.distance(c.at));
        let tau = t - wave.arrival_s;
        if tau >= 0.0 && tau < 1.0e-5 {
            front = true;
        }
        gauge += friedlander(wave.peak_pa, wave.duration_s, tau);
    }
    if front { shock_density_ratio(gauge) } else { isentropic_density_ratio(gauge) }
}

/// How far (rad) a ray from `origin` along `dir` is turned by the density field of `charges` at
/// time `t`, over `length` m (paraxial: the integral of the transverse gradient of n along the
/// straight ray, by central differences every `step` m).
pub fn ray_deflection(charges: &[Charge], origin: Vec3, dir: Vec3, length: f32, t: f32, step: f32) -> f32 {
    let dir = dir.normalize_or_zero();
    let n = |p: Vec3| refractivity(density_ratio_at(charges, p, t));
    let h = (0.25 * step).max(1.0e-4);
    let mut bend = Vec3::ZERO;
    let mut s = 0.0;
    while s < length {
        let p = origin + dir * s;
        let grad = Vec3::new(n(p + Vec3::X * h) - n(p - Vec3::X * h), n(p + Vec3::Y * h) - n(p - Vec3::Y * h),
            n(p + Vec3::Z * h) - n(p - Vec3::Z * h)) / (2.0 * h);
        bend += (grad - dir * grad.dot(dir)) * step;
        s += step;
    }
    bend.length()
}

/// What a man takes from a peak overpressure (FM 8-9 para 403, unreflected wave): the share of
/// eardrums ruptured, of lungs damaged, and of deaths, each rising linearly in log pressure from
/// its threshold to its half point and on to all. The manual gives every threshold and the
/// eardrum's half point (90-130 kPa); the lung's half point (193 kPa, where it puts the start of
/// death) and death's (400 kPa, above its 262 kPa survival record) are authored, as are the curves
/// between and beyond (named).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlastInjury {
    pub eardrum: f32,
    pub lung: f32,
    pub lethal: f32,
}

/// (threshold Pa, half point Pa) for each injury.
pub const EARDRUM_PA: (f32, f32) = (22.0e3, 110.0e3);
pub const LUNG_PA: (f32, f32) = (69.0e3, 193.0e3);
pub const LETHAL_PA: (f32, f32) = (193.0e3, 400.0e3);

fn log_share(p: f32, (threshold, half): (f32, f32)) -> f32 {
    if p <= threshold {
        return 0.0;
    }
    let x = (p / threshold).ln() / (half / threshold).ln();
    (0.5 * x).clamp(0.0, 1.0)
}

pub fn injury(peak_pa: f32) -> BlastInjury {
    BlastInjury { eardrum: log_share(peak_pa, EARDRUM_PA), lung: log_share(peak_pa, LUNG_PA),
        lethal: log_share(peak_pa, LETHAL_PA) }
}

/// The impulse a body of frontal area `area_m2` and mass `mass_kg` takes, as the speed it is thrown
/// at (the wave's positive-phase impulse on its area, drag and reflection not counted: named).
pub fn thrown_speed(impulse_pa_s: f32, area_m2: f32, mass_kg: f32) -> f32 {
    impulse_pa_s * area_m2 / mass_kg.max(1.0)
}

// ── Inside: the image-source method ──────────────────────────────────────────────────────────────

/// A reflecting wall: a point on it and its normal into the room.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub point: Vec3,
    pub normal: Vec3,
}

impl Wall {
    fn mirror(&self, p: Vec3) -> Vec3 {
        p - self.normal * 2.0 * (p - self.point).dot(self.normal)
    }
}

/// Probe directions: a Fibonacci sphere of `count` points.
pub fn probe_directions(count: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..count).map(|i| {
        let y = 1.0 - 2.0 * (i as f32 + 0.5) / count as f32;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = golden * i as f32;
        Vec3::new(r * a.cos(), y, r * a.sin())
    }).collect()
}

/// The walls around `source`: the planes the probe rays meet (`cast` returns the first surface
/// along a ray, its point and normal), merged when their normals agree within 10 degrees and their
/// planes within 5 cm.
pub fn find_walls(source: Vec3, probes: usize, reach: f32, cast: &dyn Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>) -> Vec<Wall> {
    let mut walls: Vec<Wall> = Vec::new();
    for dir in probe_directions(probes) {
        let Some((point, mut normal)) = cast(source, source + dir * reach) else { continue };
        if normal.dot(dir) > 0.0 {
            normal = -normal;
        }
        let normal = normal.normalize_or_zero();
        if normal == Vec3::ZERO {
            continue;
        }
        let duplicate = walls.iter().any(|w| w.normal.dot(normal) > 0.985 && (point - w.point).dot(w.normal).abs() < 0.05);
        if !duplicate {
            walls.push(Wall { point, normal });
        }
    }
    walls
}

/// An image source: where, and the walls it reflected off (for the path check).
#[derive(Clone, Debug)]
pub struct Image {
    pub at: Vec3,
    pub walls: Vec<usize>,
}

/// The source and its images to second order across `walls`, in every order (mirroring across two
/// perpendicular walls gives one image either way, but only one order is a real path from a given
/// receiver: `valid_images` keeps each real image once).
pub fn images(source: Vec3, walls: &[Wall]) -> Vec<Image> {
    let mut out = vec![Image { at: source, walls: Vec::new() }];
    for (i, w) in walls.iter().enumerate() {
        let first = w.mirror(source);
        out.push(Image { at: first, walls: vec![i] });
        for (j, v) in walls.iter().enumerate() {
            if j != i {
                out.push(Image { at: v.mirror(first), walls: vec![i, j] });
            }
        }
    }
    out
}

/// The images whose reflection paths reach `receiver`, each position once.
pub fn valid_images(source: Vec3, receiver: Vec3, walls: &[Wall], cast: &dyn Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>) -> Vec<Image> {
    let mut out: Vec<Image> = Vec::new();
    for image in images(source, walls) {
        if out.iter().any(|o| o.at.distance(image.at) < 1.0e-4) {
            continue;
        }
        if path_valid(&image, receiver, source, walls, cast) {
            out.push(image);
        }
    }
    out
}

/// Whether the reflection path from `image` to `receiver` is real: each leg reaches its wall at the
/// predicted point without meeting another surface first (`cast` as in `find_walls`).
pub fn path_valid(image: &Image, receiver: Vec3, source: Vec3, walls: &[Wall],
    cast: &dyn Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>) -> bool {
    // Walk back from the receiver: the path to the image crosses the last wall first.
    let mut to = receiver;
    let mut target = image.at;
    let mut chain: Vec<Vec3> = Vec::new();
    // The images of the image at each depth (back toward the source).
    let mut layers = vec![source];
    for &w in &image.walls {
        let last = *layers.last().unwrap();
        layers.push(walls[w].mirror(last));
    }
    for depth in (0..image.walls.len()).rev() {
        let wall = walls[image.walls[depth]];
        let d = target - to;
        let denom = d.dot(wall.normal);
        if denom.abs() < 1.0e-6 {
            return false;
        }
        let t = (wall.point - to).dot(wall.normal) / denom;
        if !(0.0..=1.0).contains(&t) {
            return false;
        }
        let hit = to + d * t;
        chain.push(hit);
        // The leg from `to` to the hit must meet nothing before it.
        if let Some((p, _)) = cast(to, hit) {
            if p.distance(hit) > 0.03 {
                return false;
            }
        }
        to = hit;
        target = layers[depth];
    }
    // The last leg, to the source itself.
    match cast(to, target) {
        Some((p, _)) => p.distance(target) < 0.03,
        None => true,
    }
}

/// A receiver's pressure history inside (Pa, every `dt` s for `samples`): the sum of the valid
/// images' free-field waves.
pub fn interior_history(charge: &Charge, receiver: Vec3, walls: &[Wall], cast: &dyn Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>,
    dt: f32, samples: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; samples];
    for image in valid_images(charge.at, receiver, walls, cast) {
        let wave = Charge { at: image.at, ..*charge }.wave(receiver.distance(image.at));
        for (i, p) in out.iter_mut().enumerate() {
            *p += friedlander(wave.peak_pa, wave.duration_s, i as f32 * dt + charge.t0_s - wave.arrival_s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinney_graham_agrees_with_the_sounds_kingery_bulmash_where_both_hold() {
        // The sound's law is the surface burst; ours is free air at twice the charge.
        for z in [3.0f32, 5.0, 10.0, 20.0] {
            let ours = peak_overpressure_pa(z / SURFACE_REFLECTION.cbrt());
            let theirs = crate::sound_levels::kingery_bulmash_kpa(z) * 1000.0;
            assert!((ours / theirs - 1.0).abs() < 0.22, "Z {z}: {ours:.0} vs {theirs:.0} Pa");
        }
    }

    #[test]
    fn the_front_slows_to_sound_and_its_density_jump_to_nothing() {
        let c = Charge { at: Vec3::ZERO, tnt_kg: 1.0, t0_s: 0.0, surface: false };
        let near = c.wave(1.0);
        let far = c.wave(100.0);
        assert!(near.peak_pa > 500.0e3 && far.peak_pa < 2.0e3, "{} {}", near.peak_pa, far.peak_pa);
        assert!(shock_speed(near.peak_pa) > 2.0 * C0);
        // Far out the front moves at about the speed of sound.
        let far_speed = 10.0 / (c.wave(110.0).arrival_s - c.wave(100.0).arrival_s);
        assert!((far_speed / C0 - 1.0).abs() < 0.02, "{far_speed}");
        assert!((shock_density_ratio(1.0) - 1.0).abs() < 1.0e-4);
        assert!(shock_density_ratio(near.peak_pa) > 3.0);
        assert!(friedlander_impulse(near.peak_pa, near.duration_s) > 0.0);
    }

    #[test]
    fn injury_follows_the_manual_thresholds() {
        assert_eq!(injury(15.0e3), BlastInjury::default());
        let ear = injury(110.0e3);
        assert!((ear.eardrum - 0.5).abs() < 1e-3 && ear.lung > 0.0 && ear.lethal == 0.0);
        let severe = injury(300.0e3);
        assert!(severe.lung > 0.5 && severe.lethal > 0.0 && severe.lethal < 0.5);
        assert_eq!(injury(5.0e6).lethal, 1.0);
    }

    /// A 2 x 1.6 x 2 m box room: the image method in a box is exact, so every first- and
    /// second-order image is valid from any point inside.
    fn box_room() -> (Vec<Wall>, impl Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>) {
        let lo = Vec3::new(-1.0, 0.0, -1.0);
        let hi = Vec3::new(1.0, 1.6, 1.0);
        let walls = vec![
            Wall { point: lo, normal: Vec3::X }, Wall { point: hi, normal: -Vec3::X },
            Wall { point: lo, normal: Vec3::Y }, Wall { point: hi, normal: -Vec3::Y },
            Wall { point: lo, normal: Vec3::Z }, Wall { point: hi, normal: -Vec3::Z },
        ];
        let cast = move |from: Vec3, to: Vec3| -> Option<(Vec3, Vec3)> {
            let d = to - from;
            let mut best: Option<(f32, Vec3)> = None;
            for (axis, n) in [(0, Vec3::X), (1, Vec3::Y), (2, Vec3::Z)] {
                for (plane, normal) in [(lo[axis], n), (hi[axis], -n)] {
                    if d[axis].abs() < 1e-9 { continue; }
                    let t = (plane - from[axis]) / d[axis];
                    if t > 1e-5 && t <= 1.0 && best.is_none_or(|b| t < b.0) {
                        best = Some((t, normal));
                    }
                }
            }
            best.map(|(t, n)| (from + d * t, n))
        };
        (walls, cast)
    }

    #[test]
    fn the_walls_are_found_and_every_box_image_is_real() {
        let (_, cast) = box_room();
        let source = Vec3::new(0.2, 0.8, -0.3);
        let walls = find_walls(source, 200, 5.0, &cast);
        assert_eq!(walls.len(), 6, "{walls:?}");
        let receiver = Vec3::new(-0.5, 1.1, 0.6);
        // Direct, six first-order, twelve corners (one real order each) and six opposite-wall
        // pairs: in a box all 25 distinct images are real paths.
        assert_eq!(images(source, &walls).len(), 1 + 6 + 6 * 5);
        assert_eq!(valid_images(source, receiver, &walls, &cast).len(), 25);
    }

    #[test]
    fn a_burst_inside_hits_harder_than_the_same_burst_in_the_open() {
        let (walls, cast) = box_room();
        let charge = Charge { at: Vec3::new(0.2, 0.8, -0.3), tnt_kg: 0.164, t0_s: 0.0, surface: false };
        let receiver = Vec3::new(-0.5, 1.1, 0.6);
        let dt = 2.0e-5;
        let history = interior_history(&charge, receiver, &walls, &cast, dt, 1000);
        // The walls send the wave back again and again: the man takes several times the open
        // air's impulse, while the first peak is the direct wave's.
        let inside: f32 = history.iter().map(|p| p.max(0.0) * dt).sum();
        let wave = charge.wave(receiver.distance(charge.at));
        assert!(inside > 3.0 * wave.impulse_pa_s, "inside {inside:.1} Pa s, in the open {:.1} Pa s", wave.impulse_pa_s);
        let peak = history.iter().copied().fold(0.0f32, f32::max);
        assert!(peak >= 0.99 * wave.peak_pa);
    }

    #[test]
    #[ignore]
    fn print_pressure_and_refraction() {
        // The 8.8's muzzle blast: its 2.5 kg charge's TNT equivalent (sound_levels).
        let muzzle = Charge { at: Vec3::ZERO, tnt_kg: 2.5 * crate::sound_levels::TNT_PER_KG_PROPELLANT, t0_s: 0.0, surface: false };
        let he85 = Charge { at: Vec3::ZERO, tnt_kg: 0.164, t0_s: 0.0, surface: true };
        let he122 = Charge { at: Vec3::ZERO, tnt_kg: 3.6, t0_s: 0.0, surface: true };
        for (name, c) in [("8.8 muzzle blast", muzzle), ("85 mm filler on the ground", he85), ("122 mm HE filler on the ground", he122)] {
            for r in [1.0f32, 2.0, 5.0, 10.0, 20.0] {
                let w = c.wave(r);
                let inj = injury(w.peak_pa);
                // A ray passing the charge at `r` when the front is at r: how far is it turned?
                let t = w.arrival_s + 0.2 * w.duration_s;
                let bend = ray_deflection(&[c], Vec3::new(-3.0 * r, r * 0.999, 0.0), Vec3::X, 6.0 * r, t, (r * 0.002).max(0.002));
                println!("{name} at {r} m: {:.1} kPa, {:.2} ms, {:.0} Pa s, front at {:.2} ms; eardrum {:.2} lung {:.2} lethal {:.2}; a grazing ray turned {:.2e} rad ({:.2} px at 1024 px over 60 deg)",
                    w.peak_pa / 1000.0, w.duration_s * 1000.0, w.impulse_pa_s, w.arrival_s * 1000.0, inj.eardrum, inj.lung, inj.lethal,
                    bend, bend / (60f32.to_radians() / 1024.0));
            }
        }
    }
}
