//! Binaural rendering of positioned sources: the listener's head and the steel around it.
//!
//! Every part of this model is named; none of it is a wave solution and none of it is a measured
//! head-related transfer function (no permissively licensed measured set is bundled; the
//! parametric model below is the documented stand-in, see `docs/ACOUSTICS.md` §9 and the
//! "Binaural rendering" row of `docs/PHYSICAL_TRUTH_AUDIT.md`).
//!
//! - **Direction** is head-relative: azimuth 0 ahead, 90 right, 270 left (the convention of every
//!   bearing in this crate) and elevation above the horizontal. A sphere has no pinna, so the only
//!   angle the head model uses is the lateral angle θ = asin(sin az · cos el): front and back, and
//!   elevation along the cone of confusion, are not told apart (named limit).
//! - **Interaural time difference**: Woodworth & Schlosberg's rigid spherical head of radius
//!   87.5 mm, ITD = (a/c)(θ + sin θ): 0.66 ms at 90°, applied as a fractional delay to the ear
//!   away from the source.
//! - **Head shadow and interaural level difference**: Brown & Duda's (1998) first-order
//!   head-shadow filter per ear, H(s) = (α s + β)/(s + β) with β = 2c/a: unity at low frequency,
//!   α at high frequency, where α runs from 2 at the ear the source faces (pressure doubling on the
//!   sphere) to 0.1 at the ear in its shadow, with the minimum at 150° from the ear's axis. Its far-
//!   ear loss at 90° agrees with the per-band head shadow the hearing model uses
//!   (`sound_propagation::HEAD_SHADOW_BANDS_DB`) within 3 dB from 1 to 8 kHz; the near-ear rise is
//!   not in that table (tested, recorded in the audit).
//! - **Presentation pan**: the constant-power pan the mixer has used since 2026-09, an
//!   owner-directed departure ("every world sound is panned harder so its direction can be judged
//!   by ear", `docs/PHYSICAL_TRUTH_AUDIT.md`), stays on top of the head model at full strength so
//!   no voice's lateralisation or measured level moves. `PHYSICAL` renders the head model alone.
//! - **Transmission**: a source outside the vehicle reaches a crewman inside through the plates
//!   (every hatch closed) and through each open hatch. How much energy each way carries is the
//!   SL §8 band insertion loss of `sound_propagation` (the level is set there, not here); this
//!   module decides only which direction cues survive. The plate-borne share arrives from the
//!   whole re-radiating cabin and keeps `HULL_LATERAL_RETENTION` of its lateral angle [E]; the
//!   aperture share arrives through the opening above the head, lifted to
//!   `APERTURE_ELEVATION_DEG` [E], which shortens its ITD and ILD by the cosine of that angle.
//!   Head-out and on foot the path is direct.
//! - **Distance, air and the hull's spectrum** are not here: `sound_propagation::propagate`
//!   carries the spherical/blast distance law, ISO 9613-1 air absorption, the snow's ground
//!   effect, barrier loss and the plate and hatch insertion loss per band into `EarLevel`, whose
//!   transfer the caller applies before this stage (`audio_calibrated::render_for_ear`).
//!
//! Everything here is deterministic (no clocks, no randomness) and runs on plain slices, so the
//! unit tests below measure it exactly.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use crate::sound_propagation::{Bands, Enclosure, Transmission, BANDS};

use super::SAMPLE_RATE;

/// Rigid-sphere head radius (Woodworth; the ITU/Algazi mean), m.
pub const HEAD_RADIUS_M: f32 = 0.0875;
/// Speed of sound in the air at the head (the cabin, or the air outside at the ear), m/s. The
/// act's −20 °C air would give 319 m/s and 7 % more ITD; the head model keeps one value.
pub const HEAD_SOUND_SPEED_M_S: f32 = 343.0;
/// Brown–Duda head-shadow floor at the far ear's high frequencies (−20 dB).
pub const HEAD_SHADOW_ALPHA_MIN: f32 = 0.1;
/// Incidence (from the ear's axis) at which the head shadow is deepest; past it the far side
/// brightens again (the "bright spot" of the sphere).
pub const HEAD_SHADOW_THETA_MIN_DEG: f32 = 150.0;
/// Share of a source's lateral angle that survives arrival through the closed plates: the plate
/// nearest the source radiates most, the rest of the cabin re-radiates the remainder [E].
pub const HULL_LATERAL_RETENTION: f32 = 0.30;
/// Elevation from which sound entering through an open hatch reaches the head below the rim [E].
pub const APERTURE_ELEVATION_DEG: f32 = 45.0;
/// Longest delay line this module needs (the 90° ITD is 0.66 ms = 29 frames at 44.1 kHz).
const DELAY_FRAMES: usize = 64;

/// Largest interaural time difference of the head model, s.
pub fn max_itd_s() -> f32 {
    HEAD_RADIUS_M / HEAD_SOUND_SPEED_M_S * (FRAC_PI_2 + 1.0)
}

// ── Direction ────────────────────────────────────────────────────────────────────────────────────

/// A source direction relative to the listener's head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Direction {
    /// 0 ahead, 90 right, 270 left (degrees, any range).
    pub azimuth_deg: f32,
    /// Above the horizontal, degrees, −90..=90.
    pub elevation_deg: f32,
}

impl Direction {
    pub const AHEAD: Self = Self { azimuth_deg: 0.0, elevation_deg: 0.0 };

    /// A source in the horizontal plane (every game bearing today).
    pub fn horizontal(azimuth_deg: f32) -> Self {
        Self { azimuth_deg: if azimuth_deg.is_finite() { azimuth_deg } else { 0.0 }, elevation_deg: 0.0 }
    }

    /// Unit vector in head space: x right, y up, z ahead.
    pub fn unit(self) -> [f32; 3] {
        let az = self.azimuth_deg.to_radians();
        let el = self.elevation_deg.clamp(-90.0, 90.0).to_radians();
        let (sa, ca) = az.sin_cos();
        let (se, ce) = el.sin_cos();
        [sa * ce, se, ca * ce]
    }

    pub fn from_unit(v: [f32; 3]) -> Self {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if !len.is_finite() || len < 1.0e-6 {
            return Self::AHEAD;
        }
        let [x, y, z] = [v[0] / len, v[1] / len, v[2] / len];
        Self {
            azimuth_deg: x.atan2(z).to_degrees().rem_euclid(360.0),
            elevation_deg: y.clamp(-1.0, 1.0).asin().to_degrees(),
        }
    }

    /// Signed lateral angle, radians, positive toward the right ear: the angle between the source
    /// and the median plane, the one angle a spherical head resolves.
    pub fn lateral_rad(self) -> f32 {
        self.unit()[0].clamp(-1.0, 1.0).asin()
    }

    /// Source behind the interaural plane.
    pub fn is_behind(self) -> bool {
        self.unit()[2] < 0.0
    }

    /// Interaural-polar coordinates: the lateral angle (radians, positive right) and the polar
    /// angle around the interaural axis (0 ahead, π/2 above, π behind).
    pub fn interaural_polar(self) -> (f32, f32) {
        let [x, y, z] = self.unit();
        let lateral = x.clamp(-1.0, 1.0).asin();
        let polar = if y.abs() < 1.0e-6 && z.abs() < 1.0e-6 { 0.0 } else { y.atan2(z) };
        (lateral, polar)
    }

    pub fn from_interaural_polar(lateral_rad: f32, polar_rad: f32) -> Self {
        let (sl, cl) = lateral_rad.sin_cos();
        let (sp, cp) = polar_rad.sin_cos();
        Self::from_unit([sl, cl * sp, cl * cp])
    }

    /// The same direction with its lateral angle scaled by `k` (0 = in the median plane,
    /// 1 = unchanged), keeping its place on the cone of confusion (front/back, up/down).
    pub fn with_lateral_scaled(self, k: f32) -> Self {
        let (lateral, polar) = self.interaural_polar();
        Self::from_interaural_polar(lateral * k.clamp(0.0, 1.0), polar)
    }

    /// The same azimuth lifted to `elevation_deg`.
    pub fn lifted_to(self, elevation_deg: f32) -> Self {
        Self { azimuth_deg: self.azimuth_deg, elevation_deg: elevation_deg.clamp(-90.0, 90.0) }
    }

    /// Incidence of the source on `ear` (0 left, 1 right): the angle from that ear's axis, degrees,
    /// 0 when the source faces the ear, 180 when it is behind the head from that ear.
    pub fn ear_incidence_deg(self, ear: usize) -> f32 {
        let x = self.unit()[0].clamp(-1.0, 1.0);
        let along = if ear == 0 { -x } else { x };
        along.acos().to_degrees()
    }
}

// ── Interaural time difference ───────────────────────────────────────────────────────────────────

/// Woodworth's spherical-head interaural time difference for a lateral angle (radians, positive
/// right), seconds, positive when the right ear leads.
pub fn woodworth_itd_s(lateral_rad: f32) -> f32 {
    let theta = if lateral_rad.is_finite() { lateral_rad.clamp(-FRAC_PI_2, FRAC_PI_2) } else { 0.0 };
    HEAD_RADIUS_M / HEAD_SOUND_SPEED_M_S * (theta + theta.sin())
}

/// Arrival delay of each ear (left, right), seconds: the ear away from the source waits the ITD.
pub fn ear_delays_s(direction: Direction) -> [f32; 2] {
    let itd = woodworth_itd_s(direction.lateral_rad());
    if itd >= 0.0 { [itd, 0.0] } else { [0.0, -itd] }
}

// ── Head shadow ──────────────────────────────────────────────────────────────────────────────────

/// Brown–Duda high-frequency gain α of the head-shadow filter for an ear at `incidence_deg` from
/// the source: 2 facing it, `HEAD_SHADOW_ALPHA_MIN` at `HEAD_SHADOW_THETA_MIN_DEG`.
pub fn head_shadow_alpha(incidence_deg: f32) -> f32 {
    let theta = if incidence_deg.is_finite() { incidence_deg.clamp(0.0, 180.0) } else { 90.0 };
    (1.0 + HEAD_SHADOW_ALPHA_MIN / 2.0)
        + (1.0 - HEAD_SHADOW_ALPHA_MIN / 2.0) * (theta / HEAD_SHADOW_THETA_MIN_DEG * PI).cos()
}

/// Magnitude of the head-shadow filter H(s) = (α s + β)/(s + β), β = 2c/a, at `f_hz`, dB.
pub fn head_shadow_gain_db(alpha: f32, f_hz: f32) -> f32 {
    let beta = 2.0 * HEAD_SOUND_SPEED_M_S / HEAD_RADIUS_M;
    let w = std::f32::consts::TAU * f_hz / beta;
    10.0 * ((1.0 + (alpha * w).powi(2)) / (1.0 + w * w)).log10()
}

/// One ear's head-shadow filter: the bilinear transform of H(s) above, unity at DC.
#[derive(Clone, Copy, Debug)]
pub struct HeadShadowFilter {
    b0: f32,
    b1: f32,
    a1: f32,
    x1: f32,
    y1: f32,
}

impl HeadShadowFilter {
    pub fn new(alpha: f32, rate: f32) -> Self {
        let mut filter = Self { b0: 1.0, b1: 0.0, a1: 0.0, x1: 0.0, y1: 0.0 };
        filter.set_alpha(alpha, rate);
        filter
    }

    /// Retune without clearing the state (smooth head turns).
    pub fn set_alpha(&mut self, alpha: f32, rate: f32) {
        let alpha = if alpha.is_finite() { alpha.clamp(HEAD_SHADOW_ALPHA_MIN, 2.0) } else { 1.0 };
        let beta = 2.0 * HEAD_SOUND_SPEED_M_S / HEAD_RADIUS_M;
        let k = 2.0 * rate; // 2/T
        let norm = k + beta;
        self.b0 = (alpha * k + beta) / norm;
        self.b1 = (beta - alpha * k) / norm;
        self.a1 = (beta - k) / norm;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 - self.a1 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// A short fractional delay (linear interpolation) for the interaural delay.
#[derive(Clone, Debug)]
pub struct FractionalDelay {
    buffer: [f32; DELAY_FRAMES],
    write: usize,
}

impl Default for FractionalDelay {
    fn default() -> Self {
        Self { buffer: [0.0; DELAY_FRAMES], write: 0 }
    }
}

impl FractionalDelay {
    /// Push one sample and read the one `delay_frames` (≥ 0, < 63) before it.
    #[inline]
    pub fn process(&mut self, x: f32, delay_frames: f32) -> f32 {
        self.buffer[self.write] = x;
        let d = if delay_frames.is_finite() { delay_frames.clamp(0.0, (DELAY_FRAMES - 2) as f32) } else { 0.0 };
        let whole = d.floor();
        let frac = d - whole;
        let i0 = (self.write + DELAY_FRAMES - whole as usize) % DELAY_FRAMES;
        let i1 = (i0 + DELAY_FRAMES - 1) % DELAY_FRAMES;
        self.write = (self.write + 1) % DELAY_FRAMES;
        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }
}

// ── The head model's per-ear parameters ──────────────────────────────────────────────────────────

/// How hard the presentation pan is: 1 is the owner's full constant-power pan, 0 the head model
/// alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadModel {
    pub presentation_pan: f32,
}

impl HeadModel {
    /// What the game plays (owner-directed, `docs/PHYSICAL_TRUTH_AUDIT.md`).
    pub const PRESENTED: Self = Self { presentation_pan: 1.0 };
    /// The spherical head alone: ITD and head shadow, no pan law.
    pub const PHYSICAL: Self = Self { presentation_pan: 0.0 };
}

/// The per-ear rendering of one direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarParams {
    /// Arrival delay per ear, frames.
    pub delay_frames: [f32; 2],
    /// Head-shadow α per ear.
    pub alpha: [f32; 2],
    /// Presentation pan gain per ear (constant power at full strength, equal-power centre at 0).
    pub pan: [f32; 2],
}

impl EarParams {
    pub fn of(direction: Direction, model: HeadModel, rate: f32) -> Self {
        let delays = ear_delays_s(direction);
        let lateral = direction.lateral_rad().sin() * model.presentation_pan.clamp(0.0, 1.0);
        let pan = ((lateral + 1.0) * FRAC_PI_4).clamp(0.0, FRAC_PI_2);
        Self {
            delay_frames: [delays[0] * rate, delays[1] * rate],
            alpha: [
                head_shadow_alpha(direction.ear_incidence_deg(0)),
                head_shadow_alpha(direction.ear_incidence_deg(1)),
            ],
            pan: [pan.cos(), pan.sin()],
        }
    }
}

// ── Transmission into the vehicle ────────────────────────────────────────────────────────────────

/// The share of a source's energy at the ear that came through the plates rather than through the
/// open hatches, 0..=1, for a source whose band spectrum at the ear is `spectrum` (dB relative).
/// Direct paths have no plate share. The two insertion-loss tables are SL §8's
/// (`sound_propagation::Enclosure::insertion_loss_bands`).
pub fn plate_share(transmission: Transmission, spectrum: &Bands) -> f32 {
    match transmission {
        Transmission::Direct => 0.0,
        Transmission::Plates => 1.0,
        Transmission::Hatches { open } => {
            let plates = Enclosure::Buttoned.insertion_loss_bands();
            let total = crate::sound_propagation::hatch_open_insertion_loss_bands(open);
            let (mut through_plates, mut through_all) = (0.0f64, 0.0f64);
            for b in 0..BANDS {
                let weight = 10f64.powf(f64::from(spectrum[b]) / 10.0);
                if !weight.is_finite() {
                    continue;
                }
                through_plates += weight * 10f64.powf(-f64::from(plates[b]) / 10.0);
                through_all += weight * 10f64.powf(-f64::from(total[b]) / 10.0);
            }
            if through_all <= 0.0 {
                return 1.0;
            }
            (through_plates / through_all).clamp(0.0, 1.0) as f32
        }
    }
}

/// The directions the two arrival paths present to the head: the plate-borne path with its
/// lateral angle reduced to `HULL_LATERAL_RETENTION`, the aperture path lifted to
/// `APERTURE_ELEVATION_DEG`, and the energy share of the first. A direct path is the source
/// direction alone.
pub fn arrival_paths(direction: Direction, transmission: Transmission, spectrum: &Bands)
    -> (Direction, Direction, f32)
{
    match transmission {
        Transmission::Direct => (direction, direction, 0.0),
        _ => (
            direction.with_lateral_scaled(HULL_LATERAL_RETENTION),
            direction.lifted_to(APERTURE_ELEVATION_DEG),
            plate_share(transmission, spectrum),
        ),
    }
}

/// One direction standing for both arrival paths (the streaming voices use one delay line per
/// ear): the energy-weighted mean of the two path directions.
pub fn effective_direction(direction: Direction, transmission: Transmission, spectrum: &Bands)
    -> Direction
{
    let (plates, aperture, share) = arrival_paths(direction, transmission, spectrum);
    if matches!(transmission, Transmission::Direct) {
        return direction;
    }
    let (p, a) = (plates.unit(), aperture.unit());
    Direction::from_unit(std::array::from_fn(|i| share * p[i] + (1.0 - share) * a[i]))
}

// ── Rendering ────────────────────────────────────────────────────────────────────────────────────

/// Render a mono buffer at `direction` through the head model: per ear a fractional arrival delay,
/// the head-shadow filter and the presentation pan. Output is interleaved stereo of the same
/// length; the delayed ear's tail past the buffer end is dropped (< 0.7 ms).
pub fn render_binaural(mono: &[f32], direction: Direction, model: HeadModel) -> Vec<f32> {
    let rate = SAMPLE_RATE as f32;
    let params = EarParams::of(direction, model, rate);
    let mut delays = [FractionalDelay::default(), FractionalDelay::default()];
    let mut filters = [
        HeadShadowFilter::new(params.alpha[0], rate),
        HeadShadowFilter::new(params.alpha[1], rate),
    ];
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &x in mono {
        for ear in 0..2 {
            let delayed = delays[ear].process(x, params.delay_frames[ear]);
            out.push(filters[ear].process(delayed) * params.pan[ear]);
        }
    }
    out
}

/// Render a mono buffer that reached the listener by `transmission`: the plate-borne and aperture
/// paths are rendered separately and summed by their energy shares (amplitudes √share), so the
/// buffer's energy is that of a single rendering and the caller's level placement holds.
pub fn render_through(mono: &[f32], direction: Direction, transmission: Transmission,
    spectrum: &Bands, model: HeadModel) -> Vec<f32>
{
    let (plates, aperture, share) = arrival_paths(direction, transmission, spectrum);
    if matches!(transmission, Transmission::Direct) {
        return render_binaural(mono, direction, model);
    }
    if share >= 0.999 {
        return render_binaural(mono, plates, model);
    }
    if share <= 0.001 {
        return render_binaural(mono, aperture, model);
    }
    let mut out = render_binaural(mono, plates, model);
    let through_hatch = render_binaural(mono, aperture, model);
    let (gp, ga) = (share.sqrt(), (1.0 - share).sqrt());
    for (o, h) in out.iter_mut().zip(&through_hatch) {
        *o = *o * gp + *h * ga;
    }
    out
}

/// Apply the head model to an already panned stereo buffer in place: the ear away from
/// `direction` waits the ITD and both ears take their head-shadow filters. The buffer's pan is
/// kept (this is the legacy stage behind `spatialize_mono`).
pub fn shadow_stereo_in_place(buf: &mut [f32], direction: Direction) {
    if buf.len() < 4 {
        return;
    }
    let rate = SAMPLE_RATE as f32;
    let params = EarParams::of(direction, HeadModel::PHYSICAL, rate);
    for ear in 0..2 {
        let mut delay = FractionalDelay::default();
        let mut filter = HeadShadowFilter::new(params.alpha[ear], rate);
        for frame in buf.chunks_exact_mut(2) {
            let delayed = delay.process(frame[ear], params.delay_frames[ear]);
            frame[ear] = filter.process(delayed);
        }
    }
}

// ── Streaming ────────────────────────────────────────────────────────────────────────────────────

/// The head model for a continuous voice whose direction moves: targets are approached per
/// sample with a 20 ms constant so a head turn neither clicks nor jumps.
#[derive(Clone, Debug)]
pub struct BinauralPanner {
    delays: [FractionalDelay; 2],
    filters: [HeadShadowFilter; 2],
    current: EarParams,
    target: EarParams,
    smooth: f32,
    rate: f32,
}

impl BinauralPanner {
    pub fn new(direction: Direction, model: HeadModel) -> Self {
        let rate = SAMPLE_RATE as f32;
        let params = EarParams::of(direction, model, rate);
        Self {
            delays: [FractionalDelay::default(), FractionalDelay::default()],
            filters: [
                HeadShadowFilter::new(params.alpha[0], rate),
                HeadShadowFilter::new(params.alpha[1], rate),
            ],
            current: params,
            target: params,
            smooth: 1.0 - (-1.0 / (0.02 * rate)).exp(),
            rate,
        }
    }

    pub fn set_direction(&mut self, direction: Direction, model: HeadModel) {
        self.target = EarParams::of(direction, model, self.rate);
    }

    /// One mono input sample to (left, right).
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 2] {
        let mut out = [0.0f32; 2];
        for ear in 0..2 {
            let c = &mut self.current;
            c.delay_frames[ear] += (self.target.delay_frames[ear] - c.delay_frames[ear]) * self.smooth;
            c.pan[ear] += (self.target.pan[ear] - c.pan[ear]) * self.smooth;
            let alpha = c.alpha[ear] + (self.target.alpha[ear] - c.alpha[ear]) * self.smooth;
            if (alpha - c.alpha[ear]).abs() > 1.0e-4 {
                c.alpha[ear] = alpha;
                self.filters[ear].set_alpha(alpha, self.rate);
            }
            let delayed = self.delays[ear].process(x, c.delay_frames[ear]);
            out[ear] = self.filters[ear].process(delayed) * c.pan[ear];
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound_propagation::{class_spectrum, HEAD_SHADOW_BANDS_DB, OCTAVES_HZ};
    use crate::sound_levels::SpectrumClass;

    fn energy(stereo: &[f32]) -> (f32, f32) {
        super::super::channel_energy(stereo)
    }

    fn tone(f_hz: f32, seconds: f32) -> Vec<f32> {
        let rate = SAMPLE_RATE as f32;
        (0..(seconds * rate) as usize)
            .map(|n| (std::f32::consts::TAU * f_hz * n as f32 / rate).sin() * 0.4)
            .collect()
    }

    fn click() -> Vec<f32> {
        let mut m = vec![0.0f32; 2_048];
        m[100] = 1.0;
        m
    }

    fn first_arrival(stereo: &[f32], ear: usize) -> usize {
        stereo.chunks_exact(2).position(|f| f[ear].abs() > 1.0e-4).unwrap()
    }

    #[test]
    fn woodworth_itd_is_zero_ahead_antisymmetric_and_two_thirds_of_a_millisecond_abeam() {
        assert_eq!(woodworth_itd_s(0.0), 0.0);
        let right = woodworth_itd_s(FRAC_PI_2);
        assert!((right - 0.000656).abs() < 0.00002, "{right}");
        assert!((woodworth_itd_s(-FRAC_PI_2) + right).abs() < 1.0e-7);
        assert!((max_itd_s() - right).abs() < 1.0e-7);
        // Monotonic in the lateral angle.
        let mut last = -1.0;
        for step in 0..=90 {
            let itd = woodworth_itd_s((step as f32).to_radians());
            assert!(itd >= last);
            last = itd;
        }
        assert_eq!(woodworth_itd_s(f32::NAN), 0.0);
    }

    #[test]
    fn ear_delays_wait_on_the_far_ear_only() {
        assert_eq!(ear_delays_s(Direction::AHEAD), [0.0, 0.0]);
        let [l, r] = ear_delays_s(Direction::horizontal(90.0));
        assert!(l > 0.0006 && r == 0.0, "source right: left ear waits ({l}, {r})");
        let [l2, r2] = ear_delays_s(Direction::horizontal(270.0));
        assert!(r2 > 0.0006 && l2 == 0.0, "source left: right ear waits ({l2}, {r2})");
        // Behind the head mirrors the front on the cone of confusion.
        assert_eq!(ear_delays_s(Direction::horizontal(135.0)), ear_delays_s(Direction::horizontal(45.0)));
        // Elevation shortens the lateral angle.
        let [lifted, _] = ear_delays_s(Direction { azimuth_deg: 90.0, elevation_deg: 60.0 });
        assert!(lifted < l * 0.6 && lifted > 0.0);
    }

    #[test]
    fn direction_vectors_round_trip_and_lateral_scaling_keeps_front_and_back() {
        for (az, el) in [(0.0, 0.0), (90.0, 0.0), (200.0, -30.0), (330.0, 45.0)] {
            let d = Direction { azimuth_deg: az, elevation_deg: el };
            let back = Direction::from_unit(d.unit());
            assert!((back.azimuth_deg - az).abs() < 1.0e-3 && (back.elevation_deg - el).abs() < 1.0e-3, "{d:?} -> {back:?}");
        }
        let behind_right = Direction::horizontal(120.0);
        let narrowed = behind_right.with_lateral_scaled(HULL_LATERAL_RETENTION);
        assert!(narrowed.is_behind() && narrowed.lateral_rad() > 0.0);
        assert!((narrowed.lateral_rad() - behind_right.lateral_rad() * HULL_LATERAL_RETENTION).abs() < 1.0e-4);
        assert!(narrowed.elevation_deg.abs() < 1.0e-3);
        let abeam = Direction::horizontal(90.0).with_lateral_scaled(HULL_LATERAL_RETENTION);
        assert!((abeam.lateral_rad().to_degrees() - 90.0 * HULL_LATERAL_RETENTION).abs() < 1.0e-3);
        assert_eq!(Direction::horizontal(90.0).with_lateral_scaled(0.0).lateral_rad(), 0.0);
        let high = Direction { azimuth_deg: 45.0, elevation_deg: 40.0 }.with_lateral_scaled(0.5);
        let (_, polar_before) = Direction { azimuth_deg: 45.0, elevation_deg: 40.0 }.interaural_polar();
        let (lat_after, polar_after) = high.interaural_polar();
        assert!((polar_after - polar_before).abs() < 1.0e-4 && lat_after > 0.0);
        assert!((Direction::horizontal(90.0).ear_incidence_deg(1)).abs() < 1.0e-3);
        assert!((Direction::horizontal(90.0).ear_incidence_deg(0) - 180.0).abs() < 1.0e-3);
        assert!((Direction::AHEAD.ear_incidence_deg(0) - 90.0).abs() < 1.0e-3);
    }

    #[test]
    fn head_shadow_alpha_runs_from_facing_to_shadowed() {
        assert!((head_shadow_alpha(0.0) - 2.0).abs() < 1.0e-5);
        assert!((head_shadow_alpha(HEAD_SHADOW_THETA_MIN_DEG) - HEAD_SHADOW_ALPHA_MIN).abs() < 1.0e-5);
        assert!(head_shadow_alpha(180.0) > HEAD_SHADOW_ALPHA_MIN, "bright spot behind the sphere");
        assert!(head_shadow_alpha(90.0) < 1.0 && head_shadow_alpha(90.0) > 0.5);
        assert_eq!(head_shadow_alpha(f32::NAN), head_shadow_alpha(90.0));
    }

    #[test]
    fn head_shadow_filter_matches_its_analytic_response_and_is_unity_at_dc() {
        let rate = SAMPLE_RATE as f32;
        for alpha in [HEAD_SHADOW_ALPHA_MIN, 0.5, 1.0, 2.0] {
            for f in [125.0, 1_000.0, 4_000.0, 8_000.0] {
                let mut filter = HeadShadowFilter::new(alpha, rate);
                let input = tone(f, 0.25);
                let output: Vec<f32> = input.iter().map(|&x| filter.process(x)).collect();
                let tail = output.len() / 2;
                let gain = 20.0 * (super::super::rms(&output[tail..]) / super::super::rms(&input[tail..])).log10();
                let want = head_shadow_gain_db(alpha, f);
                // Bilinear warping bends the top octave a little.
                assert!((gain - want).abs() < if f > 6_000.0 { 1.0 } else { 0.3 }, "alpha {alpha} f {f}: {gain} vs {want}");
            }
            assert!(head_shadow_gain_db(alpha, 1.0).abs() < 0.01);
        }
    }

    /// The hearing model's per-band far-ear table (SL §4 [E]) against the deepest Brown–Duda
    /// shadow: within 3 dB at 4 and 8 kHz; from 500 Hz to 2 kHz the table is 2–5 dB deeper than
    /// the bare sphere (a real head's torso and pinna add mid-band shadow the sphere lacks;
    /// recorded in `docs/PHYSICAL_TRUTH_AUDIT.md`).
    #[test]
    fn far_ear_shadow_agrees_with_the_hearing_models_band_table_in_the_top_octaves() {
        let deepest = HEAD_SHADOW_ALPHA_MIN;
        for b in 7..BANDS {
            let loss = -head_shadow_gain_db(deepest, OCTAVES_HZ[b]);
            assert!((loss - HEAD_SHADOW_BANDS_DB[b]).abs() <= 3.0, "{} Hz: {loss} vs {}", OCTAVES_HZ[b], HEAD_SHADOW_BANDS_DB[b]);
        }
        for b in 4..7 {
            let loss = -head_shadow_gain_db(deepest, OCTAVES_HZ[b]);
            assert!(HEAD_SHADOW_BANDS_DB[b] - loss > 1.5 && HEAD_SHADOW_BANDS_DB[b] - loss < 5.0,
                "{} Hz: the sphere shadows {loss} dB, the table {}", OCTAVES_HZ[b], HEAD_SHADOW_BANDS_DB[b]);
        }
    }

    #[test]
    fn fractional_delay_is_exact_at_whole_frames_and_interpolates_between() {
        let mut d = FractionalDelay::default();
        let mut out = Vec::new();
        for n in 0..40 {
            out.push(d.process(if n == 0 { 1.0 } else { 0.0 }, 10.0));
        }
        assert_eq!(out[10], 1.0);
        assert!(out.iter().enumerate().all(|(i, v)| i == 10 || *v == 0.0));
        let mut d = FractionalDelay::default();
        let mut out = Vec::new();
        for n in 0..40 {
            out.push(d.process(if n == 0 { 1.0 } else { 0.0 }, 10.5));
        }
        assert!((out[10] - 0.5).abs() < 1.0e-6 && (out[11] - 0.5).abs() < 1.0e-6);
        assert_eq!(d.process(1.0, f32::NAN), 1.0);
    }

    #[test]
    fn binaural_render_pans_delays_and_shadows_the_far_ear() {
        let right = render_binaural(&tone(3_000.0, 0.1), Direction::horizontal(90.0), HeadModel::PHYSICAL);
        let (l, r) = energy(&right);
        assert!(r > l * 4.0 && l > 0.0, "3 kHz from the right: far ear shadowed, not silent (L={l} R={r})");
        let left = render_binaural(&tone(3_000.0, 0.1), Direction::horizontal(270.0), HeadModel::PHYSICAL);
        let (l2, r2) = energy(&left);
        assert!((l2 - r).abs() < 1.0e-3 * r && (r2 - l).abs() < 1.0e-3 * l.max(1.0e-9), "mirror symmetry");
        let click_right = render_binaural(&click(), Direction::horizontal(90.0), HeadModel::PHYSICAL);
        let itd_frames = first_arrival(&click_right, 0) as i64 - first_arrival(&click_right, 1) as i64;
        assert!((28..=30).contains(&itd_frames), "0.66 ms ITD at 44.1 kHz is 29 frames, got {itd_frames}");
        let ahead = render_binaural(&tone(1_000.0, 0.1), Direction::AHEAD, HeadModel::PHYSICAL);
        let (l3, r3) = energy(&ahead);
        assert!((l3 - r3).abs() < 1.0e-4 * l3, "ahead is symmetric");
        // Low frequencies pass around the head: ILD small, ITD intact.
        let low = render_binaural(&tone(125.0, 0.2), Direction::horizontal(90.0), HeadModel::PHYSICAL);
        let (l4, r4) = energy(&low);
        assert!(10.0 * (r4 / l4).log10() < 1.5, "125 Hz ILD must be small, got {} dB", 10.0 * (r4 / l4).log10());
    }

    #[test]
    fn presented_model_keeps_the_owners_constant_power_pan() {
        let abeam = render_binaural(&tone(500.0, 0.1), Direction::horizontal(90.0), HeadModel::PRESENTED);
        let (l, r) = energy(&abeam);
        assert!(l < r * 1.0e-6, "full pan puts a 90° source in one ear (L={l} R={r})");
        // At 125 Hz the head shadows nothing, so the ear ratio is the pan law's alone.
        let half = render_binaural(&tone(125.0, 0.3), Direction::horizontal(30.0), HeadModel::PRESENTED);
        let (l, r) = energy(&half);
        let pan = ((0.5f32 + 1.0) * FRAC_PI_4).sin().powi(2) / ((0.5f32 + 1.0) * FRAC_PI_4).cos().powi(2);
        assert!(((r / l) / pan - 1.0).abs() < 0.08, "30°: pan law ratio {pan}, got {}", r / l);
    }

    #[test]
    fn plate_share_is_total_when_buttoned_and_small_through_an_open_hatch() {
        let spectrum = class_spectrum(SpectrumClass::LowFrequencyBlast);
        assert_eq!(plate_share(Transmission::Direct, &spectrum), 0.0);
        assert_eq!(plate_share(Transmission::Plates, &spectrum), 1.0);
        let one = plate_share(Transmission::Hatches { open: 1 }, &spectrum);
        assert!(one > 0.0 && one < 0.12, "one open hatch carries most of a blast ({one})");
        let five = plate_share(Transmission::Hatches { open: 5 }, &spectrum);
        assert!(five < one, "more openings, smaller plate share ({five} vs {one})");
        let crack = plate_share(Transmission::Hatches { open: 1 }, &class_spectrum(SpectrumClass::ShellCrack));
        assert!(crack < one, "the plates pass even less of a bright crack ({crack} vs {one})");
    }

    #[test]
    fn buttoned_arrival_narrows_direction_and_an_open_hatch_restores_it() {
        let spectrum = class_spectrum(SpectrumClass::LowFrequencyBlast);
        let source = tone(2_000.0, 0.1);
        let direction = Direction::horizontal(90.0);
        let direct = render_through(&source, direction, Transmission::Direct, &spectrum, HeadModel::PHYSICAL);
        let plates = render_through(&source, direction, Transmission::Plates, &spectrum, HeadModel::PHYSICAL);
        let hatch = render_through(&source, direction, Transmission::Hatches { open: 1 }, &spectrum, HeadModel::PHYSICAL);
        let ild = |s: &[f32]| { let (l, r) = energy(s); 10.0 * (r / l).log10() };
        // The bare sphere's far-ear bright spot makes its 2 kHz ILD largest a little short of
        // abeam, so only the plates (lateral 27°) are ordered against the direct path here; the
        // presented model's pan orders all three.
        assert!(ild(&plates) < ild(&direct) - 2.0, "ILD plates {} < direct {}", ild(&plates), ild(&direct));
        let presented = |t: Transmission| ild(&render_through(&source, direction, t, &spectrum, HeadModel::PRESENTED));
        let (pp, ph, pd) = (presented(Transmission::Plates), presented(Transmission::Hatches { open: 1 }), presented(Transmission::Direct));
        assert!(pp + 3.0 < ph && ph + 3.0 < pd, "presented ILD plates {pp} < hatch {ph} < direct {pd}");
        // The dominant interaural lag (cross-correlation peak): the two summed paths of an open
        // hatch make the plate-borne click arrive first but faintly, so first arrival is not it.
        let itd = |s: &[f32]| -> i64 {
            let frames = s.len() / 2;
            (-40i64..=40).max_by(|&a, &b| {
                let corr = |lag: i64| (0..frames).filter_map(|i| {
                    let j = i as i64 + lag;
                    (j >= 0 && (j as usize) < frames).then(|| s[i * 2 + 1] * s[j as usize * 2])
                }).sum::<f32>();
                corr(a).total_cmp(&corr(b))
            }).unwrap()
        };
        let (cp, ch, cd) = (
            render_through(&click(), direction, Transmission::Plates, &spectrum, HeadModel::PHYSICAL),
            render_through(&click(), direction, Transmission::Hatches { open: 1 }, &spectrum, HeadModel::PHYSICAL),
            render_through(&click(), direction, Transmission::Direct, &spectrum, HeadModel::PHYSICAL),
        );
        assert!(itd(&cp) < itd(&ch) && itd(&ch) < itd(&cd), "ITD plates {} < hatch {} < direct {}", itd(&cp), itd(&ch), itd(&cd));
        assert!((itd(&cd) - 29).abs() <= 1 && (itd(&cp) - 10).abs() <= 1 && (itd(&ch) - 17).abs() <= 1,
            "lags {} {} {}", itd(&cp), itd(&ch), itd(&cd));
        // Energy is that of one rendering, whichever way the sound came in.
        let total = |s: &[f32]| { let (l, r) = energy(s); l + r };
        assert!((total(&hatch) / total(&direct) - 1.0).abs() < 0.25, "{} vs {}", total(&hatch), total(&direct));
        assert_eq!(render_through(&source, direction, Transmission::Direct, &spectrum, HeadModel::PHYSICAL), direct);
    }

    #[test]
    fn effective_direction_sits_between_the_two_paths() {
        let spectrum = class_spectrum(SpectrumClass::Engine);
        let d = Direction::horizontal(90.0);
        let plates = effective_direction(d, Transmission::Plates, &spectrum);
        let hatch = effective_direction(d, Transmission::Hatches { open: 1 }, &spectrum);
        assert_eq!(effective_direction(d, Transmission::Direct, &spectrum), d);
        assert!(plates.lateral_rad() < hatch.lateral_rad() && hatch.lateral_rad() < d.lateral_rad());
        assert!(hatch.elevation_deg > 30.0, "through the roof hatch the sound comes from above ({})", hatch.elevation_deg);
    }

    #[test]
    fn streaming_panner_converges_to_the_one_shot_render_and_turns_without_clicks() {
        let source = tone(1_000.0, 0.5);
        let mut panner = BinauralPanner::new(Direction::horizontal(60.0), HeadModel::PRESENTED);
        let streamed: Vec<f32> = source.iter().flat_map(|&x| panner.process(x)).collect();
        let fixed = render_binaural(&source, Direction::horizontal(60.0), HeadModel::PRESENTED);
        let tail = streamed.len() / 2;
        let (sl, sr) = energy(&streamed[tail..]);
        let (fl, fr) = energy(&fixed[tail..]);
        assert!((sl / fl - 1.0).abs() < 0.02 && (sr / fr - 1.0).abs() < 0.02, "{sl}/{fl} {sr}/{fr}");
        // Turn the head from 60° right to 60° left over the second half: no sample jumps.
        let mut panner = BinauralPanner::new(Direction::horizontal(60.0), HeadModel::PRESENTED);
        let mut last = [0.0f32; 2];
        let mut largest_step = 0.0f32;
        for (n, &x) in source.iter().enumerate() {
            if n == source.len() / 2 {
                panner.set_direction(Direction::horizontal(300.0), HeadModel::PRESENTED);
            }
            let out = panner.process(x);
            for ear in 0..2 {
                largest_step = largest_step.max((out[ear] - last[ear]).abs());
            }
            last = out;
        }
        let tone_step = 0.4 * (std::f32::consts::TAU * 1_000.0 / SAMPLE_RATE as f32) * 2.0;
        assert!(largest_step < tone_step * 1.5, "head turn clicked: step {largest_step} vs tone {tone_step}");
        let (l, r) = energy(&source.iter().flat_map(|&x| panner.process(x)).collect::<Vec<_>>());
        assert!(l > r * 4.0, "after the turn the source is on the left (L={l} R={r})");
    }

    #[test]
    fn shadow_stage_keeps_the_pan_and_adds_the_itd() {
        let mut stereo: Vec<f32> = click().iter().flat_map(|&x| [x * 0.3, x * 0.9]).collect();
        shadow_stereo_in_place(&mut stereo, Direction::horizontal(90.0));
        let itd = first_arrival(&stereo, 0) as i64 - first_arrival(&stereo, 1) as i64;
        assert!((28..=29).contains(&itd), "28.9 frames, linearly interpolated: {itd}");
        assert!(stereo.iter().all(|v| v.is_finite()));
    }
}
