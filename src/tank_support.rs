//! Tank ground contact on everything it can stand on (solid city, 2026-09-29).
//!
//! Before this module a tank was grounded on the terrain heightfield alone (`pos.y = height_at`)
//! and met only walls, pillars, partitions and door frames: it drove through building floors,
//! plinths, external steps, stairs and rubble. Here the hull rests on the highest supports under
//! its tracks, whatever they are (terrain, a building's floors, plinths and steps, rubble), and the
//! Tiger's documented obstacle limits decide whether it climbs, crushes or stops.
//!
//! The Tiger's limits, with their sources:
//! - **Step (vertical obstacle) 0.76 m**: "Step 2 feet 6 inches", US War Department TM-E 30-451,
//!   *Handbook on German Military Forces* (15 March 1945), ch. VII, p. VII-82, the specification
//!   of the Pz.Kpfw. "Tiger", Model E (Sd.Kfz. 181); transcription at
//!   <https://www.ibiblio.org/hyperwar/Germany/HB/HB-7.html>. (The often-quoted 0.79 m of Jentz and
//!   Doyle could not be checked here; 0.76 m is the value we could read.)
//! - **Trench 2.50 m**, **gradient 35°**, **ground clearance 0.47 m**: W. J. Spielberger, *Der
//!   Panzerkampfwagen Tiger und seine Abarten* (Motorbuch), pp. 190-191, as cited by the German
//!   Wikipedia article's data table
//!   (<https://de.wikipedia.org/wiki/Panzerkampfwagen_VI_Tiger>, read 2026-09-29). TM-E 30-451
//!   gives 10 ft (3.05 m), 30° and 1 ft 5 in (0.43 m); we use the German figures. The ground
//!   clearance agrees with the baked hull mesh, whose belly is 0.469 m above the track bottoms.
//!
//! The geometry the rules act on is measured from the rendered hull (`tiger_hull.glb`): the track
//! bottom profile along the hull ([`TIGER_TRACK_PROFILE`], flat from z = -2.0 to +2.2 m and rising
//! to the idler and sprocket), the tracks' lateral band (|x| 1.15-1.93 m) and the belly between
//! them ([`TIGER_BELLY_PROFILE`]). `tests::profiles_match_the_baked_hull` re-measures them.
//!
//! Named simplifications (docs/PHYSICAL_TRUTH_AUDIT.md, "Tank ground contact"): the hull is rigid
//! on rigid tracks (no torsion-bar travel), and each track is its centre line with the support
//! taken as the highest point across its width; the rest pose is the exact rest of that rigid body
//! on the sampled supports (sample spacing 0.1 m); the step rule measures the rise of the support
//! along a track that a 35° gradient does not explain; a hull that would come to rest on its belly
//! is stopped instead of being simulated high-centred.

#[cfg(test)]
use glam::Vec3;

/// Largest vertical step the Tiger climbs: TM-E 30-451 (1945), p. VII-82, "Step 2 feet 6 inches".
pub const TIGER_STEP_M: f32 = 0.762;
/// Widest trench the Tiger crosses: Spielberger pp. 190-191 (via de.wikipedia): 2.50 m.
pub const TIGER_TRENCH_M: f32 = 2.50;
/// Steepest gradient the Tiger climbs: Spielberger pp. 190-191 (via de.wikipedia): 35°.
pub const TIGER_GRADIENT_DEG: f32 = 35.0;
/// Ground clearance: Spielberger pp. 190-191, 47 cm; the baked belly measures 0.469 m (the belly
/// rule reads the measured profile; `tests::profiles_match_the_baked_hull` holds it to this).
#[cfg_attr(not(test), allow(dead_code))]
pub const TIGER_GROUND_CLEARANCE_M: f32 = 0.47;
/// Combat weight (docs/TIGER_I_SPEC.md: about 57,250 kg).
pub const TIGER_MASS_KG: f32 = 57_250.0;
/// Engine: Maybach HL 210 P45, 650 PS at 3000 rpm (docs/TIGER_I_SPEC.md) = 478 kW.
pub const TIGER_ENGINE_W: f32 = 478_000.0;
/// Share of engine power that reaches the ground on a climb (gearbox, steering unit, final drives
/// and track losses). An engineering estimate [I], not a measurement.
pub const DRIVE_EFFICIENCY: f32 = 0.6;
const G: f32 = 9.81;
/// Rolling resistance on the level, chosen so the power limit there equals the model's governed
/// top speed (`tank::TOP_SPEED`, 12 m/s): 0.043, within the 0.03-0.1 usually quoted for steel
/// tracks from roads to firm soil [I]. On a grade the weight's downhill pull adds to it.
pub const ROLLING_RESISTANCE: f32 =
    TIGER_ENGINE_W * DRIVE_EFFICIENCY / (TIGER_MASS_KG * G * crate::tank::TOP_SPEED);

/// Lateral centre and half width of each track band on the baked hull (|x| from 1.15 to 1.93 m).
pub const TIGER_TRACK_X: f32 = 1.54;
pub const TIGER_TRACK_HALF_WIDTH: f32 = 0.39;
/// Hull-local z of the front and rear ends of the track (the hull mesh's extent).
pub const TIGER_FRONT_Z: f32 = 3.56;
pub const TIGER_REAR_Z: f32 = -3.45;

/// Height of the track bottom above its flat run, along the hull (hull-local z, metres), measured
/// from `assets/tanks/tiger/baked/tiger_hull.glb`: the lowest vertex of the track band
/// (|x| 1.15-1.93 m) in 0.1 m slices. Sorted by z; beyond the ends the track does not reach.
pub const TIGER_TRACK_PROFILE: &[(f32, f32)] = &[
    (-3.45, 0.93),
    (-3.1, 0.409),
    (-3.0, 0.281),
    (-2.9, 0.196),
    (-2.8, 0.180),
    (-2.7, 0.130),
    (-2.6, 0.118),
    (-2.5, 0.107),
    (-2.4, 0.082),
    (-2.3, 0.063),
    (-2.2, 0.057),
    (-2.1, 0.032),
    (-2.0, 0.0),
    (2.2, 0.0),
    (2.3, 0.013),
    (2.4, 0.066),
    (2.5, 0.122),
    (2.6, 0.132),
    (2.7, 0.183),
    (2.8, 0.241),
    (2.9, 0.296),
    (3.0, 0.307),
    (3.1, 0.357),
    (3.2, 0.421),
    (3.3, 0.474),
    (3.4, 0.560),
    (3.5, 0.887),
    (3.56, 0.95),
];

/// Highest point of the track run (the front of the track over the sprocket, the rear over the
/// idler): a solid whose underside is higher hangs over the tracks, which cannot reach it; only
/// the upper hull or the turret meet it.
pub const TIGER_TRACK_TOP_M: f32 = 0.95;

/// Height of the belly between the tracks (|x| < 1.0 m) above the flat track run, measured from
/// the same mesh; the belly is 0.469-0.571 m over its length.
pub const TIGER_BELLY_PROFILE: &[(f32, f32)] = &[
    (-2.8, 0.560),
    (-2.6, 0.524),
    (-2.0, 0.503),
    (-1.2, 0.571),
    (1.4, 0.571),
    (1.5, 0.524),
    (2.8, 0.559),
    (2.9, 0.469),
    (3.1, 0.485),
    (3.2, 0.560),
];

/// Spacing of the support samples along each track.
pub const SAMPLE_STEP_M: f32 = 0.1;
/// Horizontal window of the step rule, about the run of the track's rising front (s 2.2-3.5 m): a
/// rise spread over a longer run is a slope, which the gradient limit governs.
pub const STEP_WINDOW_M: f32 = 1.0;

fn interpolate(table: &[(f32, f32)], s: f32) -> Option<f32> {
    let first = table.first()?;
    let last = table.last()?;
    if s < first.0 || s > last.0 {
        return None;
    }
    let upper = table.partition_point(|(z, _)| *z < s).min(table.len() - 1);
    if upper == 0 {
        return Some(first.1);
    }
    let (a, b) = (table[upper - 1], table[upper]);
    let t = if b.0 > a.0 { (s - a.0) / (b.0 - a.0) } else { 0.0 };
    Some(a.1 + (b.1 - a.1) * t.clamp(0.0, 1.0))
}

/// Height of the track bottom above the flat run at hull-local z = `s` (`None` beyond the ends).
pub fn track_bottom(s: f32) -> Option<f32> {
    interpolate(TIGER_TRACK_PROFILE, s)
}

/// Height of the belly above the flat track run at hull-local z = `s` (`None` beyond the belly).
pub fn belly_bottom(s: f32) -> Option<f32> {
    interpolate(TIGER_BELLY_PROFILE, s)
}

/// Stations along the hull where the support is sampled: every [`SAMPLE_STEP_M`] from the rear to
/// the front end of the track.
pub fn track_stations() -> Vec<f32> {
    let count = ((TIGER_FRONT_Z - TIGER_REAR_Z) / SAMPLE_STEP_M).round() as usize;
    (0..=count)
        .map(|i| (TIGER_REAR_Z + i as f32 * SAMPLE_STEP_M).min(TIGER_FRONT_Z))
        .collect()
}

/// Stations of the belly samples.
pub fn belly_stations() -> Vec<f32> {
    let (first, last) = (TIGER_BELLY_PROFILE[0].0, TIGER_BELLY_PROFILE[TIGER_BELLY_PROFILE.len() - 1].0);
    let count = ((last - first) / 0.2).round() as usize;
    (0..=count).map(|i| first + i as f32 * 0.2).collect()
}

/// Which part of the hull a support sample lies under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// Hull-local +x (the tank's left).
    LeftTrack,
    RightTrack,
    Belly,
}

impl Line {
    pub fn x(self) -> f32 {
        match self {
            Line::LeftTrack => TIGER_TRACK_X,
            Line::RightTrack => -TIGER_TRACK_X,
            Line::Belly => 0.0,
        }
    }
}

/// One support sample: the highest support under a station of a line, in world height.
#[derive(Clone, Copy, Debug)]
pub struct SupportSample {
    pub line: Line,
    /// Hull-local z of the station (where the track profile is read).
    pub s: f32,
    /// Horizontal distance of the station from the hull origin along the heading (`s` times the
    /// cosine of the hull's pitch): the coordinate the rest plane and the step rule use.
    pub along: f32,
    /// World height of the highest support there.
    pub height: f32,
    /// Opaque identity of the support (the caller's source index), for crush and load.
    pub source: u32,
    /// A solid there that rises more than a step above the track's contact within the tank's
    /// height (an obstacle, never a support).
    pub obstacle: Option<Obstacle>,
}

/// A solid a hull meets: how far its top stands above the track's contact, its source, and
/// whether it hangs over the tracks (its underside above [`TIGER_TRACK_TOP_M`]: an upper floor
/// meeting the upper hull or turret, which the tracks cannot reach) or reaches down within the
/// tracks' height (the tracks and bow push against it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub height: f32,
    pub source: u32,
    pub overhead: bool,
}

/// Top of the hull above the track bottoms along it (hull-local z, metres): the highest vertex of
/// `tiger_hull.glb` in 0.2 m slices (engine deck fittings and the exhaust stacks included).
pub const TIGER_HULL_TOP_PROFILE: &[(f32, f32)] = &[
    (-3.6, 2.181),
    (-3.4, 2.208),
    (-3.2, 2.540),
    (-3.0, 2.595),
    (-2.8, 2.175),
    (-2.4, 2.188),
    (-2.0, 2.211),
    (-1.8, 2.119),
    (-1.0, 2.242),
    (-0.8, 2.082),
    (1.2, 2.085),
    (1.4, 2.149),
    (2.0, 2.184),
    (2.2, 2.143),
    (2.6, 2.108),
    (2.8, 2.068),
    (3.0, 1.790),
    (3.2, 1.509),
    (3.4, 1.472),
    (3.6, 0.911),
];
/// The turret (`tiger_turret.glb` on its ring, hull frame): every vertex lies within 1.76 m of the
/// ring axis at hull-local z 0.019, and its top (the cupola) stands 3.21 m above the track bottoms.
pub const TIGER_TURRET_AXIS_Z: f32 = 0.019;
pub const TIGER_TURRET_RADIUS: f32 = 1.76;
pub const TIGER_TURRET_TOP: f32 = 3.211;

/// Height of the tank's top above its track bottoms at hull-local (x, s): the hull's profile, or
/// the turret's where its footprint (a disc: it turns) covers the point.
pub fn top_at(s: f32, x: f32) -> f32 {
    let hull = interpolate(TIGER_HULL_TOP_PROFILE, s).unwrap_or(0.0);
    let dz = s - TIGER_TURRET_AXIS_Z;
    if dz * dz + x * x <= TIGER_TURRET_RADIUS * TIGER_TURRET_RADIUS {
        hull.max(TIGER_TURRET_TOP)
    } else {
        hull
    }
}

/// The plane the rigid track bottoms lie on: world height at (horizontal distance along the
/// heading `s`, lateral hull offset `x`) is `y0 + slope_s * s + slope_x * x`; `slope_s` is the
/// tangent of the pitch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RestPlane {
    pub y0: f32,
    pub slope_s: f32,
    pub slope_x: f32,
}

impl RestPlane {
    pub fn height(&self, s: f32, x: f32) -> f32 {
        self.y0 + self.slope_s * s + self.slope_x * x
    }

    /// Nose-up pitch angle (radians).
    pub fn pitch(&self) -> f32 {
        self.slope_s.atan()
    }

    /// Left-side-up roll angle (radians).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn roll(&self) -> f32 {
        self.slope_x.atan()
    }
}

/// Upper convex hull of points sorted by s (Andrew's monotone chain, upper half).
fn upper_hull(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut hull: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for &p in points {
        while hull.len() >= 2 {
            let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
            // Pop b when it lies on or below the segment a → p.
            let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
            if cross >= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    hull
}

/// `max_i (h_i - a s_i)` over a hull's vertices (the lowest line of slope `a` above every point,
/// evaluated at s = 0).
fn support_at(hull: &[(f32, f32)], a: f32) -> f32 {
    hull.iter()
        .map(|&(s, h)| h - a * s)
        .fold(f32::NEG_INFINITY, f32::max)
}

/// The rest of the rigid hull on its two tracks: the plane of least centre-of-mass height (the
/// CoM at hull-local (s_com, 0)) that no track support rises above. `lifted` are per-track points
/// (s, support height minus the track bottom's height above its flat run at s): the plane carries
/// the flat run, and a support meeting the rising ends of the track lifts it by the profile.
///
/// For a fixed pitch slope `a` each track line's lowest height is `U(a) = max_i (h_i - a s_i)`,
/// and the plane through both lines has CoM height `(U_L + U_R)/2 + a s_com`, a convex piecewise
/// linear function of `a` whose minimum lies at a slope of an upper-hull edge of either track.
pub fn rest_plane(left: &[(f32, f32)], right: &[(f32, f32)], s_com: f32) -> Option<RestPlane> {
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let sorted = |points: &[(f32, f32)]| {
        let mut out: Vec<(f32, f32)> = points
            .iter()
            .copied()
            .filter(|(s, h)| s.is_finite() && h.is_finite())
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1)));
        // Keep the highest point of every station.
        out.dedup_by(|b, a| (a.0 - b.0).abs() < 1.0e-6);
        out
    };
    let (left, right) = (sorted(left), sorted(right));
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let (hl, hr) = (upper_hull(&left), upper_hull(&right));
    let mut candidates: Vec<f32> = Vec::with_capacity(hl.len() + hr.len());
    for hull in [&hl, &hr] {
        for pair in hull.windows(2) {
            let ds = pair[1].0 - pair[0].0;
            if ds > 1.0e-6 {
                candidates.push((pair[1].1 - pair[0].1) / ds);
            }
        }
    }
    if candidates.is_empty() {
        candidates.push(0.0);
    }
    let cost = |a: f32| 0.5 * (support_at(&hl, a) + support_at(&hr, a)) + a * s_com;
    let best = candidates
        .iter()
        .copied()
        .min_by(|a, b| cost(*a).total_cmp(&cost(*b)).then(a.abs().total_cmp(&b.abs())))?;
    let (yl, yr) = (support_at(&hl, best), support_at(&hr, best));
    Some(RestPlane {
        y0: 0.5 * (yl + yr),
        slope_s: best,
        slope_x: (yl - yr) / (2.0 * TIGER_TRACK_X),
    })
}

/// The largest step along one track in the direction of travel: the rise of the support from
/// station i to a station j ahead of it within [`STEP_WINDOW_M`], less what a climbable gradient
/// explains (`tan 35° · (s_j - s_i)`), plus the gradient allowance of one sample spacing so a
/// vertical face between neighbouring stations counts at its full height. `points` are (s, height)
/// sorted by s; `forward` picks the direction. Returns (step, index of the higher station).
pub fn largest_step(points: &[(f32, f32)], forward: bool) -> Option<(f32, usize)> {
    let tan_g = TIGER_GRADIENT_DEG.to_radians().tan();
    let mut best: Option<(f32, usize)> = None;
    let n = points.len();
    for j in 0..n {
        let sj = points[j].0;
        // Stations behind j in the direction of travel, within the window (points are sorted).
        let range = if forward {
            let lo = points.partition_point(|p| p.0 < sj - STEP_WINDOW_M - 1.0e-4);
            lo..j
        } else {
            let hi = points.partition_point(|p| p.0 <= sj + STEP_WINDOW_M + 1.0e-4);
            (j + 1)..hi
        };
        for i in range {
            let (si, hi) = points[i];
            let ahead = if forward { sj - si } else { si - sj };
            if ahead <= 1.0e-6 {
                continue;
            }
            let step = points[j].1 - hi - tan_g * (ahead - SAMPLE_STEP_M).max(0.0);
            if best.map_or(true, |(value, _)| step > value) {
                best = Some((step, j));
            }
        }
    }
    best
}

/// Gaps along one track: runs of stations whose support lies more than a step below both of the
/// supports that bound the run, as (first s, last s, width). A tank bridges a gap up to
/// [`TIGER_TRENCH_M`].
pub fn gaps(points: &[(f32, f32)]) -> Vec<(f32, f32, f32)> {
    let mut out = Vec::new();
    let n = points.len();
    let mut i = 0;
    while i < n {
        // A gap starts after station i when the next station drops by more than a step.
        if i + 1 < n && points[i + 1].1 < points[i].1 - TIGER_STEP_M {
            let rim = points[i].1;
            let mut j = i + 1;
            while j < n && points[j].1 < rim - TIGER_STEP_M {
                j += 1;
            }
            if j < n && points[j].1 >= points[j - 1].1 + TIGER_STEP_M {
                let (first, last) = (points[i + 1].0, points[j - 1].0);
                // The gap spans from the near rim to the far rim.
                out.push((first, last, (points[j].0 - points[i].0).abs() - SAMPLE_STEP_M));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Speed the engine can hold climbing a gradient of `pitch` (radians, positive uphill): the
/// power at the sprockets over the weight's downhill pull plus rolling resistance. At 35° this is
/// 0.8 m/s (2.9 km/h, the Tiger's first gear of 2.8 km/h in docs/TIGER_I_SPEC.md).
pub fn grade_speed_limit(pitch: f32) -> f32 {
    let resistance = TIGER_MASS_KG * G * (pitch.sin() + ROLLING_RESISTANCE * pitch.cos());
    if resistance <= 1.0 {
        return f32::INFINITY;
    }
    TIGER_ENGINE_W * DRIVE_EFFICIENCY / resistance
}

/// Why a proposed pose cannot be taken.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// A step higher than the Tiger's 0.76 m limit rises ahead of a track, or a solid more than a
    /// step above the tracks stands where the hull would be: (height, the source).
    Step(f32, u32),
    /// A solid hanging above the tracks meets the hull or turret (a floor over a raised ground
    /// floor, a landing): (height of its top above the tracks, the source).
    Overhead(f32, u32),
    /// The rest pose would pitch the hull beyond the 35° gradient limit, uphill in the direction
    /// of travel.
    Gradient(f32),
    /// A support would reach the belly (0.47 m clearance): (penetration, source).
    Belly(f32, u32),
}

/// The pose a hull takes on its supports, or why it cannot.
#[derive(Clone, Debug)]
pub struct Settled {
    pub plane: RestPlane,
    /// Supports that touch the track bottoms (within 2 cm): their sources carry the tank.
    pub carriers: Vec<u32>,
    /// The stations where they touch: (line, hull-local s, source).
    pub carrier_stations: Vec<(Line, f32, u32)>,
    /// The worst step ahead of either track and its source.
    pub step: Option<(f32, u32)>,
    /// The worst belly penetration and its source.
    pub belly: Option<(f32, u32)>,
    /// Gaps wider than the trench limit under either track (hull-local s of their near rim).
    #[cfg_attr(not(test), allow(dead_code))]
    pub wide_gap: Option<f32>,
    /// Stations where a solid stands more than a step above the tracks, and the highest of them.
    pub obstacles: u32,
    pub worst_obstacle: Option<Obstacle>,
}

/// Settle the rigid hull on the sampled supports. `samples` are every station of both tracks and
/// the belly; `forward` is the direction of travel; `cos_tilt` the cosine of the previous pitch
/// times that of the previous roll (the track profile is vertical in the hull frame).
pub fn settle(samples: &[SupportSample], forward: bool, cos_tilt: f32, s_com: f32) -> Option<Settled> {
    // (along, height, source, hull-local s) per line, sorted along the heading.
    type Point = (f32, f32, u32, f32);
    let mut left: Vec<Point> = Vec::new();
    let mut right: Vec<Point> = Vec::new();
    let mut belly: Vec<Point> = Vec::new();
    for sample in samples {
        let point = (sample.along, sample.height, sample.source, sample.s);
        match sample.line {
            Line::LeftTrack => left.push(point),
            Line::RightTrack => right.push(point),
            Line::Belly => belly.push(point),
        }
    }
    let by_along = |v: &mut Vec<Point>| v.sort_by(|a, b| a.0.total_cmp(&b.0));
    by_along(&mut left);
    by_along(&mut right);
    by_along(&mut belly);

    // The trench rule: supports beyond a gap wider than the trench limit (in the direction of
    // travel, ahead of the centre of mass) do not carry the hull, so it tips into the gap once its
    // centre of mass has passed the near rim.
    let mut wide_gap: Option<f32> = None;
    let mut cut_ahead = f32::INFINITY;
    for track in [&left, &right] {
        let profile: Vec<(f32, f32)> = track.iter().map(|p| (p.0, p.1)).collect();
        for (first, last, width) in gaps(&profile) {
            if width <= TIGER_TRENCH_M {
                continue;
            }
            let near = if forward { first } else { last };
            // Only a gap the centre of mass has entered matters.
            let entered = if forward { near <= s_com } else { near >= s_com };
            if entered {
                let far = if forward { last } else { first };
                cut_ahead = cut_ahead.min(if forward { far } else { -far });
                wide_gap = Some(near);
            }
        }
    }
    let carries = |along: f32| {
        if cut_ahead.is_infinite() {
            true
        } else if forward {
            along <= cut_ahead
        } else {
            -along <= cut_ahead
        }
    };
    let lift = |points: &[Point]| -> Vec<(f32, f32)> {
        points
            .iter()
            .filter(|p| carries(p.0))
            .filter_map(|&(along, h, _, s)| track_bottom(s).map(|bottom| (along, h - bottom * cos_tilt)))
            .collect()
    };
    let plane = rest_plane(&lift(&left), &lift(&right), s_com)?;

    let mut carriers = Vec::new();
    let mut carrier_stations = Vec::new();
    for (line, points) in [(Line::LeftTrack, &left), (Line::RightTrack, &right)] {
        for &(along, h, source, s) in points.iter() {
            let Some(bottom) = track_bottom(s) else { continue };
            if !carries(along) {
                continue;
            }
            if plane.height(along, line.x()) + bottom * cos_tilt - h < 0.02 {
                carrier_stations.push((line, s, source));
                if !carriers.contains(&source) {
                    carriers.push(source);
                }
            }
        }
    }
    let mut step: Option<(f32, u32)> = None;
    for points in [&left, &right] {
        let profile: Vec<(f32, f32)> = points.iter().map(|p| (p.0, p.1)).collect();
        if let Some((value, index)) = largest_step(&profile, forward) {
            if step.map_or(true, |(best, _)| value > best) {
                step = Some((value, points[index].2));
            }
        }
    }
    let mut belly_worst: Option<(f32, u32)> = None;
    for &(along, h, source, s) in &belly {
        let Some(bottom) = belly_bottom(s) else { continue };
        let penetration = h - (plane.height(along, 0.0) + bottom * cos_tilt);
        if penetration > 0.0 && belly_worst.map_or(true, |(best, _)| penetration > best) {
            belly_worst = Some((penetration, source));
        }
    }
    let mut obstacles = 0;
    let mut worst_obstacle: Option<Obstacle> = None;
    for sample in samples {
        if let Some(obstacle) = sample.obstacle {
            obstacles += 1;
            // A solid the tracks push against outranks one hanging over the hull.
            let rank = |o: &Obstacle| (!o.overhead, o.height);
            if worst_obstacle.map_or(true, |best| rank(&obstacle) > rank(&best)) {
                worst_obstacle = Some(obstacle);
            }
        }
    }
    Some(Settled {
        plane,
        carriers,
        carrier_stations,
        step,
        belly: belly_worst,
        wide_gap,
        obstacles,
        worst_obstacle,
    })
}

/// Would a hull moving from a settled pose `before` into `after` violate a limit? Only a move that
/// makes things worse is refused, so a hull already in an inconsistent state (placed by a debug
/// teleport, or left by a failure beneath it) can always move out of it.
pub fn refusal(before: Option<&Settled>, after: &Settled, forward: bool) -> Option<Refusal> {
    // Rounding only: a violation may not grow by creeping a little every frame (with 1 mm per
    // frame a hull high-centred on a rubble crest ground over it at 6 cm/s, 2026-09-29).
    const WORSE: f32 = 1.0e-5;
    // A solid higher than a step where the hull moves in: the hull meets it.
    if let Some(obstacle) = after.worst_obstacle {
        if after.obstacles > before.map_or(0, |b| b.obstacles) {
            return Some(if obstacle.overhead {
                Refusal::Overhead(obstacle.height, obstacle.source)
            } else {
                Refusal::Step(obstacle.height, obstacle.source)
            });
        }
    }
    if let Some((step, source)) = after.step {
        let old = before.and_then(|b| b.step).map_or(f32::NEG_INFINITY, |s| s.0);
        if step > TIGER_STEP_M && step > old + WORSE {
            return Some(Refusal::Step(step, source));
        }
    }
    let uphill = if forward { after.plane.slope_s } else { -after.plane.slope_s };
    let limit = TIGER_GRADIENT_DEG.to_radians().tan();
    if uphill > limit {
        let old = before.map_or(f32::NEG_INFINITY, |b| if forward { b.plane.slope_s } else { -b.plane.slope_s });
        if uphill > old + WORSE {
            return Some(Refusal::Gradient(uphill.atan()));
        }
    }
    if let Some((penetration, source)) = after.belly {
        let old = before.and_then(|b| b.belly).map_or(0.0, |b| b.0);
        if penetration > 0.02 && penetration > old + WORSE {
            return Some(Refusal::Belly(penetration, source));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samples of both tracks and the belly over a support function of world (s, x) for a hull at
    /// the origin heading +Z.
    fn samples(support: impl Fn(f32, f32) -> f32) -> Vec<SupportSample> {
        let mut out = Vec::new();
        for s in track_stations() {
            for line in [Line::LeftTrack, Line::RightTrack] {
                let height = [-TIGER_TRACK_HALF_WIDTH, 0.0, TIGER_TRACK_HALF_WIDTH]
                    .into_iter()
                    .map(|dx| support(s, line.x() + dx))
                    .fold(f32::NEG_INFINITY, f32::max);
                out.push(SupportSample { line, s, along: s, height, source: 0, obstacle: None });
            }
        }
        for s in belly_stations() {
            out.push(SupportSample {
                line: Line::Belly,
                s,
                along: s,
                height: support(s, 0.0),
                source: 1,
                obstacle: None,
            });
        }
        out
    }

    fn settle_on(support: impl Fn(f32, f32) -> f32) -> Settled {
        settle(&samples(support), true, 1.0, 0.0).expect("settles")
    }

    #[test]
    fn a_flat_ground_carries_the_level_hull_on_its_flat_run() {
        let settled = settle_on(|_, _| 3.0);
        assert!((settled.plane.y0 - 3.0).abs() < 1e-5);
        assert!(settled.plane.slope_s.abs() < 1e-6 && settled.plane.slope_x.abs() < 1e-6);
        assert!(settled.step.unwrap().0 <= 1e-4);
        assert!(refusal(None, &settled, true).is_none());
    }

    #[test]
    fn steps_up_to_the_documented_limit_are_climbed_and_higher_ones_refused() {
        // The front of the track meets a step at s = 3.0.
        for (height, climbable) in [(0.3, true), (0.6, true), (0.75, true), (0.85, false), (1.0, false)] {
            let settled = settle_on(|s, _| if s >= 3.0 { height } else { 0.0 });
            let refused = refusal(None, &settled, true);
            assert_eq!(refused.is_none(), climbable, "step {height}: {refused:?}");
            if climbable {
                // The rising front run rides up the edge: the hull pitches nose-up.
                assert!(settled.plane.slope_s >= 0.0);
            }
        }
    }

    #[test]
    fn a_climbable_step_lifts_the_front_as_the_hull_rolls_onto_it() {
        let mut last_pitch = -1.0;
        for edge in [3.3, 2.6, 1.5, 0.5, -0.5] {
            let settled = settle_on(|s, _| if s >= edge { 0.7 } else { 0.0 });
            assert!(refusal(None, &settled, true).is_none());
            let pitch = settled.plane.pitch();
            if edge > 0.0 {
                assert!(pitch >= last_pitch - 1e-4, "pitch grows while the front climbs");
            }
            last_pitch = pitch;
        }
        // With the centre of mass past the edge the hull lies on the step, level.
        let settled = settle_on(|s, _| if s >= -2.5 { 0.7 } else { 0.0 });
        assert!((settled.plane.y0 - 0.7).abs() < 0.02 && settled.plane.pitch().abs() < 0.01);
    }

    #[test]
    fn the_gradient_limit_stops_a_climb_beyond_35_degrees() {
        for (deg, climbable) in [(20.0f32, true), (34.0, true), (40.0, false), (45.0, false)] {
            let t = deg.to_radians().tan();
            let settled = settle_on(|s, _| s * t);
            assert!((settled.plane.pitch() - deg.to_radians()).abs() < 0.01);
            assert_eq!(refusal(None, &settled, true).is_none(), climbable, "{deg}°");
            // Downhill (reversing up it is refused the same way; driving down is allowed).
            let down = settle(&samples(|s, _| -s * t), true, 1.0, 0.0).unwrap();
            assert!(refusal(None, &down, true).is_none(), "driving down {deg}°");
        }
    }

    #[test]
    fn a_trench_up_to_the_documented_width_is_bridged_and_a_wider_one_is_not() {
        let trench = |width: f32, near: f32| {
            move |s: f32, _x: f32| if s > near && s < near + width { -2.5 } else { 0.0 }
        };
        // The centre of mass over the middle of a 2.4 m trench: bridged level.
        let settled = settle_on(trench(2.4, -1.2));
        assert!(settled.wide_gap.is_none());
        assert!(settled.plane.y0.abs() < 0.02 && settled.plane.pitch().abs() < 0.01);
        // A 3.0 m trench the centre of mass has entered: the far rim does not carry it, the hull
        // tips into the trench.
        let settled = settle_on(trench(3.0, -0.4));
        assert!(settled.wide_gap.is_some());
        assert!(settled.plane.pitch() < -0.2, "nose down: {}", settled.plane.pitch());
        // Before the centre of mass reaches the near rim nothing tips.
        let settled = settle_on(trench(3.0, 0.4));
        assert!(settled.wide_gap.is_none());
        assert!(settled.plane.pitch().abs() < 0.01);
    }

    #[test]
    fn a_crest_between_the_tracks_that_reaches_the_belly_is_refused() {
        let crest = |height: f32| move |s: f32, x: f32| if x.abs() < 0.6 && s.abs() < 1.0 { height } else { 0.0 };
        assert!(refusal(None, &settle_on(crest(0.4)), true).is_none());
        let settled = settle_on(crest(0.7));
        assert!(matches!(refusal(None, &settled, true), Some(Refusal::Belly(_, 1))));
    }

    #[test]
    fn a_step_under_one_track_rolls_the_hull() {
        let settled = settle_on(|_, x| if x > 0.0 { 0.5 } else { 0.0 });
        assert!((settled.plane.slope_x - 0.5 / (2.0 * TIGER_TRACK_X)).abs() < 1e-4);
        assert!((settled.plane.y0 - 0.25).abs() < 1e-4);
    }

    #[test]
    fn only_a_move_that_makes_a_violation_worse_is_refused() {
        let blocked = settle_on(|s, _| if s >= 3.0 { 1.2 } else { 0.0 });
        assert!(refusal(None, &blocked, true).is_some());
        // Backing away from it keeps the step behind: allowed.
        let backing = settle(&samples(|s, _| if s >= 3.0 { 1.2 } else { 0.0 }), false, 1.0, 0.0).unwrap();
        assert!(refusal(Some(&blocked), &backing, false).is_none());
        // Staying on the same violation (a pose already inside it) is not a new refusal.
        assert!(refusal(Some(&blocked), &blocked, true).is_none());
    }

    #[test]
    fn a_solid_higher_than_a_step_is_an_obstacle_never_a_support() {
        // A slab 1.2 m above the tracks under the front stations: it does not lift the hull, and
        // moving further into it is refused; standing still or backing out is not.
        let with_slab = |edge: f32| {
            let mut out = samples(|_, _| 0.0);
            for sample in &mut out {
                if sample.s >= edge {
                    sample.obstacle = Some(Obstacle { height: 1.2, source: 7, overhead: false });
                }
            }
            settle(&out, true, 1.0, 0.0).unwrap()
        };
        let touching = with_slab(3.5);
        assert!(touching.plane.y0.abs() < 1e-6 && touching.plane.pitch().abs() < 1e-6);
        let deeper = with_slab(3.3);
        assert!(matches!(refusal(Some(&touching), &deeper, true), Some(Refusal::Step(h, 7)) if (h - 1.2).abs() < 1e-6));
        assert!(refusal(Some(&deeper), &touching, false).is_none());
        assert!(refusal(Some(&touching), &touching, true).is_none());
        // One hanging over the hull is an overhead refusal (it stops the tank; tracks cannot crush it).
        let mut out = samples(|_, _| 0.0);
        out[0].obstacle = Some(Obstacle { height: 2.5, source: 9, overhead: true });
        let under = settle(&out, true, 1.0, 0.0).unwrap();
        assert!(matches!(refusal(None, &under, true), Some(Refusal::Overhead(_, 9))));
    }

    #[test]
    fn the_tank_top_is_the_hull_and_the_turret_disc() {
        assert!((top_at(3.6, 0.0) - 0.911).abs() < 1e-3, "the nose is low");
        assert!((top_at(0.0, 0.0) - TIGER_TURRET_TOP).abs() < 1e-6, "the cupola over the ring");
        // On a track line the turret disc covers only |s - 0.019| <= sqrt(1.76² - 1.54²) = 0.85.
        assert!((top_at(0.8, TIGER_TRACK_X) - TIGER_TURRET_TOP).abs() < 1e-6);
        assert!(top_at(1.0, TIGER_TRACK_X) < 2.3);
    }

    #[test]
    fn the_track_top_is_the_highest_point_of_the_track_profile() {
        let top = TIGER_TRACK_PROFILE.iter().map(|p| p.1).fold(0.0_f32, f32::max);
        assert_eq!(top, TIGER_TRACK_TOP_M);
        assert!(TIGER_TRACK_TOP_M > TIGER_STEP_M, "a step the tracks climb is below the track top");
    }

    #[test]
    fn grade_speed_limit_matches_first_gear_at_the_gradient_limit() {
        let v = grade_speed_limit(TIGER_GRADIENT_DEG.to_radians());
        assert!((v * 3.6 - 2.9).abs() < 0.3, "{} km/h", v * 3.6);
        assert!((grade_speed_limit(0.0) - crate::tank::TOP_SPEED).abs() < 1e-3);
    }

    #[test]
    fn largest_step_counts_a_vertical_face_at_its_full_height() {
        let points: Vec<(f32, f32)> = track_stations()
            .into_iter()
            .map(|s| (s, if s >= 1.05 { 0.8 } else { 0.0 }))
            .collect();
        let (step, _) = largest_step(&points, true).unwrap();
        assert!((step - 0.8).abs() < 1e-4, "{step}");
        // A 35° slope is no step at all; a 60° face 1.5 m high is one.
        let tan35 = 35f32.to_radians().tan();
        let slope: Vec<(f32, f32)> = track_stations().into_iter().map(|s| (s, s.max(0.0) * tan35)).collect();
        assert!(largest_step(&slope, true).unwrap().0 < 0.1);
        let tan60 = 60f32.to_radians().tan();
        let face: Vec<(f32, f32)> = track_stations()
            .into_iter()
            .map(|s| (s, (s.max(0.0) * tan60).min(1.5)))
            .collect();
        assert!(largest_step(&face, true).unwrap().0 > TIGER_STEP_M);
    }

    /// The profiles are the baked hull's: re-measure the lowest vertex of the track band and of the
    /// belly in 0.1 m slices and compare with the tables.
    #[test]
    fn profiles_match_the_baked_hull() {
        let path = "assets/tanks/tiger/baked/tiger_hull.glb";
        if !std::path::Path::new(path).is_file() {
            eprintln!("skipping: {path} is not installed");
            return;
        }
        let mesh = crate::gltf_load::load_glb(path).expect("hull loads");
        let lowest = |x_min: f32, x_max: f32, z: f32| {
            mesh.vertices
                .iter()
                .map(|v| Vec3::from(v.pos))
                .filter(|p| p.x.abs() >= x_min && p.x.abs() <= x_max && (p.z - z).abs() <= 0.05)
                .map(|p| p.y)
                .fold(f32::INFINITY, f32::min)
        };
        let floor = mesh.vertices.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
        assert!(floor.abs() < 0.01, "the hull's origin is its ground contact ({floor})");
        // The slices where the profile is steep (the idler and sprocket ends) depend on the bin
        // edges; compare the run from the idler's lower arc to the sprocket's.
        for &(z, table) in TIGER_TRACK_PROFILE.iter().filter(|(z, _)| (-3.0..=3.3).contains(z)) {
            let measured = lowest(1.15, 1.93, z);
            assert!((measured - table).abs() < 0.03, "track at z {z}: mesh {measured}, table {table}");
        }
        for s in [-2.0, 0.0, 1.0, 2.9] {
            let measured = lowest(0.0, 1.0, s);
            let table = belly_bottom(s).unwrap();
            assert!((measured - table).abs() < 0.03, "belly at z {s}: mesh {measured}, table {table}");
        }
        let belly_min = (-28..=31)
            .map(|i| lowest(0.0, 1.0, i as f32 * 0.1))
            .fold(f32::INFINITY, f32::min);
        assert!((belly_min - TIGER_GROUND_CLEARANCE_M).abs() < 0.01, "clearance {belly_min}");
    }
}
