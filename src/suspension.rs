//! Sprung hull on its road-wheel stations (2026-10-28), for every tank in the game.
//!
//! Before this module a hull was rigid on rigid tracks: the player Tiger took the exact rest pose
//! of `tank_support::settle` every frame and AI vehicles a six-contact terrain fit
//! (`vehicle_ground`), so every pebble, rubble piece or scatter support snapped the hull up at
//! once. Here the hull is a body with three integrated degrees of freedom (heave, pitch, roll)
//! carried by a spring, a damper, a travel limit and a bump stop at each road-wheel station
//! (`vehicle_spec`), against the support under each station:
//!
//! - **Wheel envelope.** A station's support is the lowest height its wheel's bottom can take
//!   while clearing every support sample within one wheel radius (`h + sqrt(r^2 - d^2) - r`): a
//!   wheel rolls over what is narrower than itself instead of jumping onto its peak. The first and
//!   last station also take the rising track run beyond them (a step met by the track's nose lifts
//!   the front station through the track, as in `tank_support::settle`).
//! - **Station force.** `k (d0 + gap) + c d(gap)/dt`, `gap` the support's height over the station's
//!   static position on the hull, `d0` the static deflection (each station carries an equal share
//!   of the sprung weight at rest [E]). A wheel whose support falls more than its droop travel
//!   below the hull leaves the ground (force 0); past its jounce travel a bump stop adds 8 times
//!   the station rate [E] with 0.3 of its critical damping [E]. The ground never pulls.
//! - **Belly.** Where a crest reaches the belly, a stiff contact (`BELLY_HZ` against the sprung
//!   mass) carries the hull: high-centring as in `tank_support`.
//! - **Load transfer.** The tracks' horizontal force on the ground acts the centre of gravity's
//!   height below it: accelerating squats the tail, braking dives the nose, a turn rolls the hull
//!   outward (`m a h` about each axis). The accelerations are the frame's own change of speed and
//!   heading, bounded at `ACCEL_BOUND` (a contact projection's speed change is not a measured
//!   impulse).
//! - **Integration.** Semi-implicit Euler in substeps of at most `STEP_SECONDS`, the supports
//!   interpolated through the frame; the same frame sequence always gives the same result.
//!
//! Named simplifications (docs/PHYSICAL_TRUTH_AUDIT.md, "Vehicle dynamics"): the wheels and track
//! are massless (the unsprung share's weight goes straight to the ground and its own bounce is not
//! modelled); each station moves along the hull's vertical; the track between stations bridges
//! whatever lies beyond a wheel radius of every wheel (no track wrap and tension); the inertias
//! are uniform-box estimates; the hull's refusals (step, trench, gradient, tipping, belly
//! hang-up) stay `tank_support`'s rigid-rest rules, which decide what the tank may cross.

use glam::{Vec2, Vec3};

use crate::vehicle_spec::{VehicleSpec, G};

/// Longest integration substep (s).
pub const STEP_SECONDS: f32 = 1.0 / 240.0;
/// Bump-stop rate as a multiple of the station's spring rate, and its damping ratio [E].
pub const BUMP_RATE_FACTOR: f32 = 8.0;
pub const BUMP_DAMPING: f32 = 0.3;
/// Natural frequency of the hull on its belly contacts alone (Hz) [E: steel on frozen ground
/// stiff enough that the hull sinks a few centimetres into a crest] and its damping ratio.
pub const BELLY_HZ: f32 = 5.0;
pub const BELLY_DAMPING: f32 = 0.5;
/// Bound on the frame-differenced acceleration used for load transfer (m/s^2): 3 g.
pub const ACCEL_BOUND: f32 = 3.0 * G;
/// Approach angle of the track run beyond the end stations of an AI vehicle [E: 30 degrees, the
/// order of the drawn tracks' idler and sprocket run-up].
pub const AI_APPROACH_TAN: f32 = 0.577;

/// Hull-local s of the first and last road-wheel station of the left (+x) and right side of the
/// baked Tiger hull (`tiger_hull.glb`): the centres of its 48 road-wheel nodes (three interleaved
/// wheels per swing arm, 0.88 m across as drawn), 0.571 and 0.573 m apart
/// (`tests::tiger_stations_match_the_baked_wheels` re-measures them).
pub const TIGER_STATION_RUNS: [(f32, f32); 2] = [(-1.86, 2.14), (-1.91, 2.10)];

/// One road-wheel station: hull-local position (x left, s forward), rate, damping coefficient,
/// static deflection and travel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Station {
    pub x: f32,
    pub s: f32,
    pub k: f32,
    pub c: f32,
    pub d0: f32,
    pub jounce: f32,
    pub droop: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum End {
    Rear,
    Middle,
    Front,
}

/// The suspension of one vehicle as laid out on its drawn hull.
#[derive(Clone, Debug)]
pub struct Layout {
    pub spec: &'static VehicleSpec,
    /// Left side first (stations rear to front), then the right side.
    pub stations: Vec<Station>,
    /// Hull-local s of the first and last station (the ends of the flat track run).
    pub run: (f32, f32),
    /// Hull-local s of the hull's rear and front ends (where the track run-up ends).
    pub ends: (f32, f32),
    pub half_gauge: f32,
    /// Belly contacts of an AI hull: (s, height of the belly over the track bottoms).
    pub belly: Vec<(f32, f32)>,
    pub mass: f32,
    pub pitch_inertia: f32,
    pub roll_inertia: f32,
}

impl Layout {
    /// Stations evenly from `run.0` to `run.1` on the two track lines at `±half_gauge`.
    pub fn new(spec: &'static VehicleSpec, run: (f32, f32), ends: (f32, f32), half_gauge: f32, belly: Vec<(f32, f32)>) -> Self {
        Self::with_runs(spec, [run, run], ends, half_gauge, belly)
    }

    /// As [`Layout::new`] with each side's own run (left, right): interleaved running gear puts
    /// the two sides' stations a few centimetres apart.
    pub fn with_runs(spec: &'static VehicleSpec, runs: [(f32, f32); 2], ends: (f32, f32), half_gauge: f32, belly: Vec<(f32, f32)>) -> Self {
        let n = spec.stations.max(2);
        let mean_load = spec.station_load_n();
        let mass = spec.sprung_mass();
        let station_mass = mass / (2 * n) as f32;
        let mut stations = Vec::with_capacity(2 * n);
        for (side, x) in [half_gauge, -half_gauge].into_iter().enumerate() {
            let run = runs[side];
            // Station loads at rest: equal shares when the run is centred under the centre of
            // gravity (hull-local s = 0, as `tank_support::settle`'s `s_com`), shifted linearly
            // along an off-centre run so the static pose is level (`F_i = W/N + b (s_i - mean)`,
            // the moments about s = 0 balanced).
            let at = |i: usize| run.0 + (run.1 - run.0) * i as f32 / (n - 1) as f32;
            let mean = (0..n).map(at).sum::<f32>() / n as f32;
            let spread: f32 = (0..n).map(|i| (at(i) - mean).powi(2)).sum();
            let slope = -(n as f32) * mean_load * mean / spread.max(1e-6);
            for i in 0..n {
                let s = at(i);
                let load = mean_load + slope * (s - mean);
                let k = spec.station_rate(i);
                let zeta = spec.friction_damping
                    + if spec.damper_stations.contains(&i) { spec.shock_damping } else { 0.0 };
                stations.push(Station {
                    x,
                    s,
                    k,
                    c: zeta * 2.0 * (k * station_mass).sqrt(),
                    d0: load / k,
                    jounce: spec.jounce_m,
                    droop: spec.droop_m,
                });
            }
        }
        let run = (runs[0].0.min(runs[1].0), runs[0].1.max(runs[1].1));
        Self {
            spec,
            stations,
            run,
            ends,
            half_gauge,
            belly,
            mass,
            pitch_inertia: spec.pitch_inertia(),
            roll_inertia: spec.roll_inertia(),
        }
    }

    /// The player Tiger on its measured hull: the eight swing-arm stations of each side where the
    /// baked hull's road-wheel nodes are (`TIGER_STATION_RUNS`), on the measured track centre
    /// lines. The belly is the measured profile, sampled by the caller.
    pub fn tiger_measured() -> Self {
        use crate::tank_support as ts;
        Self::with_runs(&crate::vehicle_spec::TIGER_I, TIGER_STATION_RUNS, (ts::TIGER_REAR_Z, ts::TIGER_FRONT_Z), ts::TIGER_TRACK_X, Vec::new())
    }

    /// An AI vehicle on its collision footprint: the contact span and track lines of
    /// `vehicle_ground`'s declared contacts (0.65 of the half-length, 0.75 of the half-width),
    /// the belly at the spec's ground clearance at the middle and both ends of the run.
    pub fn for_footprint(spec: &'static VehicleSpec, half_extents: Vec2) -> Self {
        let span = half_extents.y * 0.65;
        let belly = vec![(-0.5 * span, spec.ground_clearance_m), (0.0, spec.ground_clearance_m), (0.5 * span, spec.ground_clearance_m)];
        Self::new(spec, (-span, span), (-half_extents.y, half_extents.y), half_extents.x * 0.75, belly)
    }

    pub fn per_side(&self) -> usize {
        self.stations.len() / 2
    }

    pub fn end_of(&self, index: usize) -> End {
        let i = index % self.per_side();
        if i == 0 {
            End::Rear
        } else if i + 1 == self.per_side() {
            End::Front
        } else {
            End::Middle
        }
    }
}

/// The wheel-bottom support of a station at hull-local `s` with wheel radius `r`, from support
/// samples `(s', height)` along its track line. `End::Front` / `End::Rear` also take the samples
/// beyond the station, lowered by the track's rise `rise(s')` there (`None` where the track does
/// not reach). `NEG_INFINITY` when nothing is under it.
pub fn station_support(points: &[(f32, f32)], s: f32, r: f32, end: End, rise: &dyn Fn(f32) -> Option<f32>) -> f32 {
    let mut best = f32::NEG_INFINITY;
    for &(sp, h) in points {
        if !h.is_finite() {
            continue;
        }
        let d = sp - s;
        if d.abs() <= r {
            best = best.max(h + (r * r - d * d).sqrt() - r);
        }
        let beyond = match end {
            End::Front => d > 0.0,
            End::Rear => d < 0.0,
            End::Middle => false,
        };
        if beyond {
            if let Some(lift) = rise(sp) {
                best = best.max(h - lift);
            }
        }
    }
    best
}

/// The ground under one hull this frame: each station's support (world height, `Layout` order)
/// and the belly contacts as (hull-local s, belly height over the track bottoms, support height).
#[derive(Clone, Debug, Default)]
pub struct Ground {
    pub wheel: Vec<f32>,
    pub belly: Vec<(f32, f32, f32)>,
}

/// Integrated hull state and its per-station read-outs.
#[derive(Clone, Debug, Default)]
pub struct Ride {
    pub active: bool,
    /// Height of the hull origin (the track bottoms' plane under the hull centre at static load).
    pub y: f32,
    pub vy: f32,
    pub pitch: f32,
    pub pitch_rate: f32,
    pub roll: f32,
    pub roll_rate: f32,
    /// Where the last step was taken (a larger jump is a placement: the ride restarts).
    pub xz: Vec2,
    last_speed: f32,
    last_heading: f32,
    prev_wheel: Vec<f32>,
    /// Per station (`Layout` order): wheel travel relative to its static position on the hull
    /// (+ up into the hull), and the force it carries (N).
    pub travel: Vec<f32>,
    pub load: Vec<f32>,
    /// Frame-mean hull accelerations: heave (m/s^2), pitch and roll (rad/s^2), and the
    /// longitudinal and lateral accelerations the load transfer used (m/s^2).
    pub heave_accel: f32,
    pub pitch_accel: f32,
    pub roll_accel: f32,
    pub along_accel: f32,
    pub lateral_accel: f32,
    /// Some station's support rose faster than the rise limit allows (`rise_limit`).
    pub limited: bool,
}

/// A placement further than this from the last step restarts the ride at the rigid rest.
pub const RIDE_CONTINUITY_M: f32 = 3.0;

impl Ride {
    /// Start at a pose with the hull at rest (no velocities).
    pub fn reset(&mut self, y: f32, pitch: f32, roll: f32, xz: Vec2, speed: f32, heading: f32) {
        *self = Ride {
            active: true,
            y,
            pitch,
            roll,
            xz,
            last_speed: speed,
            last_heading: heading,
            ..Ride::default()
        };
    }

    /// Whether the ride continues at `xz` or the hull was placed elsewhere.
    pub fn continues(&self, xz: Vec2) -> bool {
        self.active && self.xz.distance(xz) <= RIDE_CONTINUITY_M
    }

    /// Bound each station's support rise to `max_rise_m` since the last frame: rubble that lands
    /// round a standing hull does not throw it up (the tracks climb it no faster than driving up
    /// a steep face lets them, as `solid_city::settle_player_tank` bounded the rigid rest).
    pub fn rise_limit(&mut self, ground: &mut Ground, max_rise_m: f32) {
        self.limited = false;
        if self.prev_wheel.len() != ground.wheel.len() {
            return;
        }
        for (now, &before) in ground.wheel.iter_mut().zip(&self.prev_wheel) {
            if before.is_finite() && *now > before + max_rise_m {
                *now = before + max_rise_m;
                self.limited = true;
            }
        }
    }

    /// Advance the hull over `dt` on `ground`, its speed and heading at the end of the frame
    /// driving the load transfer.
    pub fn step(&mut self, layout: &Layout, ground: &Ground, speed: f32, heading: f32, dt: f32) {
        let n = layout.stations.len();
        if !self.active || !dt.is_finite() || dt <= 0.0 || ground.wheel.len() != n {
            return;
        }
        let mut wrap = heading - self.last_heading;
        while wrap > std::f32::consts::PI {
            wrap -= std::f32::consts::TAU;
        }
        while wrap < -std::f32::consts::PI {
            wrap += std::f32::consts::TAU;
        }
        let along = ((speed - self.last_speed) / dt).clamp(-ACCEL_BOUND, ACCEL_BOUND);
        let lateral = (speed * wrap / dt).clamp(-ACCEL_BOUND, ACCEL_BOUND);
        self.last_speed = speed;
        self.last_heading = heading;
        self.along_accel = if along.is_finite() { along } else { 0.0 };
        self.lateral_accel = if lateral.is_finite() { lateral } else { 0.0 };

        if self.prev_wheel.len() != n {
            self.prev_wheel = ground.wheel.clone();
        }
        let steps = (dt / STEP_SECONDS).ceil().clamp(1.0, 2400.0) as usize;
        let h = dt / steps as f32;
        let m = layout.mass;
        let cg = layout.spec.cg_height_m;
        let belly_k = if ground.belly.is_empty() {
            0.0
        } else {
            m * (std::f32::consts::TAU * BELLY_HZ).powi(2) / ground.belly.len() as f32
        };
        let belly_c = BELLY_DAMPING * 2.0 * (belly_k * m / ground.belly.len().max(1) as f32).sqrt();
        let rates: Vec<f32> = (0..n)
            .map(|i| {
                let (a, b) = (self.prev_wheel[i], ground.wheel[i]);
                if a.is_finite() && b.is_finite() { (b - a) / dt } else { 0.0 }
            })
            .collect();
        if self.travel.len() != n {
            self.travel = vec![0.0; n];
            self.load = vec![0.0; n];
        }
        let (vy0, wp0, wr0) = (self.vy, self.pitch_rate, self.roll_rate);
        for step in 0..steps {
            let t = (step + 1) as f32 / steps as f32;
            let (sp, cp) = self.pitch.sin_cos();
            let (sr, cr) = self.roll.sin_cos();
            let mut force = 0.0;
            let mut pitch_moment = m * self.along_accel * cg;
            let mut roll_moment = m * self.lateral_accel * cg;
            for (i, st) in layout.stations.iter().enumerate() {
                let (a, b) = (self.prev_wheel[i], ground.wheel[i]);
                let support = if a.is_finite() && b.is_finite() { a + (b - a) * t } else { b };
                let plane = self.y + st.s * sp + st.x * sr * cp;
                let gap = support - plane;
                let plane_rate = self.vy + st.s * cp * self.pitch_rate + st.x * (cr * cp * self.roll_rate - sr * sp * self.pitch_rate);
                let gap_rate = rates[i] - plane_rate;
                let mut f = 0.0;
                if gap.is_finite() && gap >= -st.droop {
                    f = st.k * (st.d0 + gap) + st.c * gap_rate;
                    if gap > st.jounce {
                        let kb = BUMP_RATE_FACTOR * st.k;
                        let cb = BUMP_DAMPING * 2.0 * (kb * m / n as f32).sqrt();
                        f += kb * (gap - st.jounce) + cb * gap_rate;
                    }
                    f = f.max(0.0);
                }
                if step + 1 == steps {
                    self.travel[i] = if gap.is_finite() { gap.clamp(-st.droop, st.jounce) } else { -st.droop };
                    self.load[i] = f;
                }
                force += f;
                pitch_moment += f * (st.s * cp - st.x * sr * sp + cg * cr * sp);
                roll_moment += f * (st.x * cr + cg * sr);
            }
            for &(s, clearance, support) in &ground.belly {
                let plane = self.y + s * sp + clearance * cp * cr;
                let pen = support - plane;
                if pen > 0.0 && pen.is_finite() {
                    let rate = -(self.vy + s * cp * self.pitch_rate);
                    let f = (belly_k * pen + belly_c * rate).max(0.0);
                    force += f;
                    pitch_moment += f * (s * cp + cg * cr * sp);
                    roll_moment += f * cg * sr;
                }
            }
            let ay = force / m - G;
            let ap = pitch_moment / layout.pitch_inertia;
            let ar = roll_moment / layout.roll_inertia;
            self.vy += ay * h;
            self.pitch_rate += ap * h;
            self.roll_rate += ar * h;
            self.y += self.vy * h;
            self.pitch += self.pitch_rate * h;
            self.roll += self.roll_rate * h;
        }
        self.heave_accel = (self.vy - vy0) / dt;
        self.pitch_accel = (self.pitch_rate - wp0) / dt;
        self.roll_accel = (self.roll_rate - wr0) / dt;
        self.prev_wheel.clone_from(&ground.wheel);
        if ![self.y, self.vy, self.pitch, self.pitch_rate, self.roll, self.roll_rate].iter().all(|v| v.is_finite()) {
            self.active = false;
        }
    }

    /// Kinetic energy of the sprung hull plus its potential energy above `reference` and the
    /// energy stored in the station springs at `ground` (J): what a passive suspension can only
    /// lose over a bump.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn energy(&self, layout: &Layout, ground: &Ground, reference: f32) -> f32 {
        let m = layout.mass;
        let kinetic = 0.5 * m * self.vy * self.vy
            + 0.5 * layout.pitch_inertia * self.pitch_rate * self.pitch_rate
            + 0.5 * layout.roll_inertia * self.roll_rate * self.roll_rate;
        let (sp, cp) = self.pitch.sin_cos();
        let sr = self.roll.sin();
        let mut spring = 0.0;
        for (i, st) in layout.stations.iter().enumerate() {
            let gap = ground.wheel[i] - (self.y + st.s * sp + st.x * sr * cp);
            if gap >= -st.droop {
                let c = (st.d0 + gap).max(0.0);
                spring += 0.5 * st.k * c * c;
                if gap > st.jounce {
                    spring += 0.5 * BUMP_RATE_FACTOR * st.k * (gap - st.jounce).powi(2);
                }
            }
        }
        kinetic + m * G * (self.y - reference) + spring
    }

    /// Hull pose (height, pitch, roll) for drawing and for everything the hull carries.
    pub fn pose(&self) -> Vec3 {
        Vec3::new(self.y, self.pitch, self.roll)
    }
}

/// A seated crewman's head on his neck, riding the hull (2026-10-28): the eye lags the seat's
/// acceleration in the hull frame as a damped spring, so the view takes the hull's real motion
/// (the slow squat on starting off, the lurch when it stops, the lean in a turn) rather than a
/// painted shake. Natural frequency and damping [E: the seated head-neck's dominant fore-aft mode
/// is reported at about 1.5-3 Hz with heavy damping; 1.8 Hz and 0.5 are taken], travel bounded at
/// `HEAD_TRAVEL_M` (the neck's comfortable range).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeadSway {
    pub offset: Vec3,
    velocity: Vec3,
}

pub const HEAD_HZ: f32 = 1.8;
pub const HEAD_DAMPING: f32 = 0.5;
pub const HEAD_TRAVEL_M: f32 = 0.08;

impl HeadSway {
    /// Advance by `dt` under the seat's acceleration `seat` (hull frame: x left, y up, z forward;
    /// m/s^2), returning the head's offset from its seated position in the same frame.
    pub fn step(&mut self, seat: Vec3, dt: f32) -> Vec3 {
        if !dt.is_finite() || dt <= 0.0 || !seat.is_finite() {
            return self.offset;
        }
        let w = std::f32::consts::TAU * HEAD_HZ;
        let steps = (dt / STEP_SECONDS).ceil().clamp(1.0, 240.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let accel = -seat - 2.0 * HEAD_DAMPING * w * self.velocity - w * w * self.offset;
            self.velocity += accel * h;
            self.offset += self.velocity * h;
            let clamped = self.offset.clamp(Vec3::splat(-HEAD_TRAVEL_M), Vec3::splat(HEAD_TRAVEL_M));
            if clamped != self.offset {
                self.velocity = Vec3::ZERO;
                self.offset = clamped;
            }
        }
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_spec::{ALL, T34_76, TIGER_I};

    fn flat(layout: &Layout, h: f32) -> Ground {
        Ground { wheel: vec![h; layout.stations.len()], belly: Vec::new() }
    }

    fn run(ride: &mut Ride, layout: &Layout, ground: &Ground, seconds: f32, dt: f32) {
        for _ in 0..(seconds / dt).round() as usize {
            ride.step(layout, ground, 0.0, 0.0, dt);
        }
    }

    #[test]
    fn a_hull_at_its_static_pose_on_flat_ground_stays_there() {
        for spec in ALL {
            let layout = Layout::for_footprint(spec, Vec2::new(1.8, 3.5));
            let mut ride = Ride::default();
            ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, 0.0, 0.0);
            run(&mut ride, &layout, &flat(&layout, 0.0), 5.0, 1.0 / 60.0);
            assert!(ride.y.abs() < 2e-3 && ride.pitch.abs() < 1e-4 && ride.roll.abs() < 1e-4, "{} {:?}", spec.name, ride.pose());
            let total: f32 = ride.load.iter().sum();
            assert!((total - layout.mass * G).abs() < layout.mass * G * 0.01, "{}", spec.name);
        }
    }

    /// Drop the hull 5 cm onto its springs and measure the bounce period and decay: the heave
    /// frequency is the spec's and the response decays with the friction damping.
    #[test]
    fn heave_rings_at_the_spec_frequency_and_decays() {
        for spec in [&TIGER_I, &T34_76] {
            let layout = Layout::for_footprint(spec, Vec2::new(1.8, 3.5));
            let mut ride = Ride::default();
            ride.reset(0.05, 0.0, 0.0, Vec2::ZERO, 0.0, 0.0);
            let ground = flat(&layout, 0.0);
            let dt = 1.0 / 240.0;
            let (mut crossings, mut last, mut peaks) = (Vec::new(), ride.y, Vec::new());
            let mut max_since = 0.0_f32;
            for i in 0..(8.0 / dt) as usize {
                ride.step(&layout, &ground, 0.0, 0.0, dt);
                max_since = max_since.max(ride.y.abs());
                if last > 0.0 && ride.y <= 0.0 {
                    crossings.push(i as f32 * dt);
                    peaks.push(max_since);
                    max_since = 0.0;
                }
                last = ride.y;
            }
            let period = (crossings[3] - crossings[1]) / 2.0;
            let hz = 1.0 / period;
            assert!((hz - spec.heave_hz()).abs() / spec.heave_hz() < 0.08, "{} {hz} vs {}", spec.name, spec.heave_hz());
            assert!(peaks[2] < peaks[1] && peaks[1] < 0.05, "{} decays: {peaks:?}", spec.name);
            assert!(ride.y.abs() < 0.01, "{} settles", spec.name);
        }
    }

    /// The Tiger's end-station shock absorbers damp pitch far more than the T-34's friction alone.
    #[test]
    fn the_tiger_pitch_settles_faster_than_the_undamped_christie_t34() {
        let settle_time = |spec: &'static VehicleSpec| {
            let layout = Layout::for_footprint(spec, Vec2::new(1.8, 3.5));
            let mut ride = Ride::default();
            ride.reset(0.0, 0.03, 0.0, Vec2::ZERO, 0.0, 0.0);
            let ground = flat(&layout, 0.0);
            let dt = 1.0 / 240.0;
            let mut last_big = 0.0;
            for i in 0..(15.0 / dt) as usize {
                ride.step(&layout, &ground, 0.0, 0.0, dt);
                if ride.pitch.abs() > 0.003 {
                    last_big = i as f32 * dt;
                }
            }
            last_big
        };
        let (tiger, t34) = (settle_time(&TIGER_I), settle_time(&T34_76));
        assert!(tiger < t34 * 0.6, "tiger {tiger} s, t34 {t34} s");
    }

    /// Accelerating squats the tail, braking dives the nose, a left turn rolls the hull to the
    /// right (left side up): in steady state the pitch is `m a h / K_pitch`.
    #[test]
    fn load_transfer_squats_dives_and_rolls_outward() {
        let layout = Layout::tiger_measured();
        let ground = flat(&layout, 0.0);
        let dt = 1.0 / 60.0;
        let steady = |accel: f32, yaw_rate: f32, speed0: f32| {
            let mut ride = Ride::default();
            ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, speed0, 0.0);
            let (mut speed, mut heading) = (speed0, 0.0);
            for _ in 0..(6.0 / dt) as usize {
                speed += accel * dt;
                heading += yaw_rate * dt;
                ride.step(&layout, &ground, speed, heading, dt);
            }
            ride
        };
        let squat = steady(1.0, 0.0, 0.0);
        let k_pitch: f32 = layout.stations.iter().map(|s| s.k * s.s * s.s).sum();
        let expected = layout.mass * 1.0 * TIGER_I.cg_height_m / k_pitch;
        assert!(squat.pitch > 0.0 && (squat.pitch - expected).abs() < expected * 0.25, "{} vs {expected}", squat.pitch);
        let dive = steady(-2.0, 0.0, 12.0);
        assert!(dive.pitch < -1.5 * expected, "{}", dive.pitch);
        let turn = steady(0.0, 0.5, 5.0);
        assert!(turn.roll > 0.005, "a left turn rolls the left side up: {}", turn.roll);
        let right = steady(0.0, -0.5, 5.0);
        assert!((right.roll + turn.roll).abs() < 1e-3);
    }

    /// Driving over a 15 cm bump: the hull rises less than the bump at its peak rate, never
    /// gains energy, and wheels roll over a stone narrower than themselves.
    #[test]
    fn a_bump_is_ridden_not_snapped_and_energy_only_decays() {
        let layout = Layout::tiger_measured();
        let r = TIGER_I.wheel_radius();
        let bump = |x: f32| if (0.0..0.3).contains(&x) { 0.15 } else { 0.0 };
        let speed = 4.0;
        let dt = 1.0 / 60.0;
        let mut ride = Ride::default();
        ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, speed, 0.0);
        let mut position = -4.0_f32;
        let mut peak_vy = 0.0_f32;
        let mut peak_y = 0.0_f32;
        let rise = |_: f32| None;
        let mut energies = Vec::new();
        for _ in 0..(5.0 / dt) as usize {
            position += speed * dt;
            let points: Vec<(f32, f32)> = (-40..=40).map(|i| {
                let s = i as f32 * 0.1;
                (s, bump(position + s))
            }).collect();
            let wheel = layout.stations.iter().enumerate()
                .map(|(i, st)| station_support(&points, st.s, r, layout.end_of(i), &rise))
                .collect();
            let ground = Ground { wheel, belly: Vec::new() };
            ride.step(&layout, &ground, speed, 0.0, dt);
            peak_vy = peak_vy.max(ride.vy.abs());
            peak_y = peak_y.max(ride.y);
            energies.push(ride.energy(&layout, &ground, 0.0));
        }
        assert!(peak_y < 0.1, "hull rose {peak_y}");
        assert!(peak_vy < 0.5, "vertical speed {peak_vy}");
        let rest = energies[0];
        let after = *energies.last().unwrap();
        assert!(after <= rest + 50.0, "energy {rest} -> {after}");
        // The envelope: a 10 cm stone 0.1 m long under a 0.8 m wheel lifts it 10 cm only when
        // centred, and less than 4 cm 0.25 m off centre.
        let stone = [(0.0, 0.1), (0.1, 0.1)];
        assert!((station_support(&stone, 0.05, r, End::Middle, &rise) - (0.1 + (r * r - 0.05f32 * 0.05).sqrt() - r)).abs() < 1e-5);
        assert!(station_support(&stone, 0.35, r, End::Middle, &rise) < 0.04);
    }

    /// A 40 cm step met by the track nose lifts the front station through the rising track run
    /// before the wheel itself reaches it.
    #[test]
    fn the_track_nose_lifts_the_front_station_onto_a_step() {
        let points: Vec<(f32, f32)> = (-35..=35).map(|i| {
            let s = i as f32 * 0.1;
            (s, if s >= 2.9 { 0.4 } else { 0.0 })
        }).collect();
        let rise = |s: f32| crate::tank_support::track_bottom(s);
        let front = station_support(&points, 2.2, 0.4, End::Front, &rise);
        let middle = station_support(&points, 1.6, 0.4, End::Middle, &rise);
        assert!(front > 0.05 && front < 0.4, "{front}");
        assert_eq!(middle, 0.0);
    }

    #[test]
    fn a_wheel_over_a_hole_hangs_at_its_droop_and_carries_nothing() {
        let layout = Layout::tiger_measured();
        let mut ground = flat(&layout, 0.0);
        ground.wheel[3] = -0.5;
        let mut ride = Ride::default();
        ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, 0.0, 0.0);
        run(&mut ride, &layout, &ground, 4.0, 1.0 / 60.0);
        assert_eq!(ride.load[3], 0.0);
        assert!((ride.travel[3] + TIGER_I.droop_m).abs() < 1e-6);
        assert!(ride.y.abs() < 0.02, "the other fifteen carry it: {}", ride.y);
    }

    #[test]
    fn with_no_ground_the_hull_falls_freely_and_a_belly_crest_carries_it() {
        let layout = Layout::tiger_measured();
        let mut ride = Ride::default();
        ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, 0.0, 0.0);
        let none = flat(&layout, f32::NEG_INFINITY);
        ride.step(&layout, &none, 0.0, 0.0, 0.5);
        assert!((ride.y + 0.5 * G * 0.25).abs() < 0.05, "{}", ride.y);
        let mut ride = Ride::default();
        ride.reset(0.0, 0.0, 0.0, Vec2::ZERO, 0.0, 0.0);
        let crest = Ground { wheel: vec![-1.0; layout.stations.len()], belly: vec![(-0.5, 0.5, 0.7), (0.5, 0.5, 0.7)] };
        run(&mut ride, &layout, &crest, 4.0, 1.0 / 60.0);
        assert!((ride.y - 0.2).abs() < 0.05, "high-centred on the crest: {}", ride.y);
    }

    /// The player layout's stations are the drawn road wheels: load the baked hull with every
    /// wheel-sized node under the track band split out, and find their centres per side.
    #[test]
    fn tiger_stations_match_the_baked_wheels() {
        let path = "assets/tanks/tiger/baked/tiger_hull.glb";
        if crate::resource::resolve(path).map_or(true, |p| !p.is_file()) {
            eprintln!("skipping: {path} is not installed");
            return;
        }
        let wheels = crate::gltf_load::glb_root_bounds(path).expect("hull loads");
        let mut sides: [Vec<f32>; 2] = [Vec::new(), Vec::new()];
        for (_, min, max) in wheels {
            let (c, size) = ((min + max) * 0.5, max - min);
            if (0.25..0.6).contains(&c.y) && size.y > 0.6 && c.x.abs() > 0.9 {
                sides[if c.x > 0.0 { 0 } else { 1 }].push(c.z);
            }
        }
        let layout = Layout::tiger_measured();
        for (side, zs) in sides.iter_mut().enumerate() {
            assert_eq!(zs.len(), 24, "three wheels on each of eight arms");
            zs.sort_by(f32::total_cmp);
            for (i, arm) in zs.chunks(3).enumerate() {
                let z = arm.iter().sum::<f32>() / 3.0;
                let st = layout.stations[side * 8 + i];
                assert!((z - st.s).abs() < 0.02, "side {side} arm {i}: wheels at {z}, station at {}", st.s);
            }
        }
    }

    #[test]
    fn the_head_lags_the_seat_and_returns() {
        let mut head = HeadSway::default();
        // Braking at 0.4 g: the head goes forward, by a/w^2 in the steady state.
        for _ in 0..120 {
            head.step(Vec3::new(0.0, 0.0, -0.4 * G), 1.0 / 60.0);
        }
        let w = std::f32::consts::TAU * HEAD_HZ;
        assert!((head.offset.z - 0.4 * G / (w * w)).abs() < 1e-3, "{:?}", head.offset);
        for _ in 0..240 {
            head.step(Vec3::ZERO, 1.0 / 60.0);
        }
        assert!(head.offset.length() < 1e-3);
    }

    #[test]
    fn the_result_does_not_depend_on_frame_partition_within_a_substep_bound() {
        let layout = Layout::tiger_measured();
        let mut ground = flat(&layout, 0.0);
        ground.wheel[0] = 0.08;
        let mut a = Ride::default();
        a.reset(0.02, 0.01, 0.0, Vec2::ZERO, 0.0, 0.0);
        let mut b = a.clone();
        run(&mut a, &layout, &ground, 2.0, 1.0 / 60.0);
        run(&mut b, &layout, &ground, 2.0, 1.0 / 120.0);
        assert!((a.y - b.y).abs() < 2e-3 && (a.pitch - b.pitch).abs() < 5e-4, "{:?} {:?}", a.pose(), b.pose());
        let mut c = Ride::default();
        c.reset(0.02, 0.01, 0.0, Vec2::ZERO, 0.0, 0.0);
        run(&mut c, &layout, &ground, 2.0, 1.0 / 60.0);
        assert_eq!(a.pose(), c.pose(), "deterministic");
    }
}
