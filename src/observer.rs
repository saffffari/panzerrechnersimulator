//! The crewman's eye: the reproduction transform from transported radiance to the display.
//!
//! This is not physics. Like the audio chain's tone map ("Reproduction transform (not physics)",
//! `docs/PHYSICAL_TRUTH_AUDIT.md`), it models how an observer adapted to the scene would see it,
//! runs after transport, and never writes a transport buffer. Its constants are identical in both
//! presets; `shaders/exposure.wgsl` receives them from `wgsl_constants()` so the GPU cannot drift
//! from this reference. Sources and tags follow `LIGHT_NIGHT_ADAPTATION_20260929.md` section 6:
//!
//! - meter: photopic and scotopic luminance from the three spectral samples (section 1.2), a
//!   linear spatial average blending a 2 deg fovea, a 20 deg Gaussian and the whole field
//!   (0.50/0.30/0.20 [E]). This bounded reproduction approximation retains light from peripheral
//!   apertures in a dark cabin; it is not a spatially resolved retinal adaptation model;
//! - pupil: Watson & Yellott 2012's unified formula [R] (Stanley-Davies with the age term, age
//!   25), constricting with 0.3 s, dilating with 2 s plus a 15 s tail (30 %) [M/E];
//! - pigments: rate-limited regeneration (Mahroo & Lamb 2004 [M] for cones: K_m 0.2,
//!   0.5 /min, 2.1e6 td s; rods [R/D]: 0.045 /min, 1e7 scotopic td s); bleaching raises the
//!   threshold by 3 (cones) and 20 (rods) log units per unit fraction bleached (Rushton [M]);
//! - neural: log-domain display adaptation, cones 1.2 s up / 2.5 s down, rods 1.8 s / 4 s [E];
//!   these are deliberate gameplay reproduction timings, not claimed physiological measurements;
//! - threshold-versus-intensity: Ferwerda, Pattanaik, Shirley & Greenberg 1996 [R];
//! - mesopic weighting: CIE 191:2010 [R] decides how much the rods add;
//! - display: Ferwerda's visibility-matching scale TVI(L_da)/TVI(A) with the display adapted to
//!   100 cd/m^2 shown at the ACES key 0.18; cones keep colour, rods add an achromatic signal that
//!   saturates at 1,700 scotopic trolands (Mahroo & Lamb 2004);
//! - flash blindness: the recovery fit of the 1968 flash-blindness review (section 6.6 [D]),
//!   log t = log 20 + 0.53 log(E/9e5) - 0.4 log(L/0.19), inverted into a decaying afterimage
//!   background; global (coverage-weighted), not a retinal map.
//!
//! The composition keeps the cone channel at full weight and bounds the (1 - m)-weighted rod
//! channel by the existing TVI response at the local saturating stimulus. This is a bounded
//! reproduction approximation, not instantaneous local physiological adaptation: globally dark
//! surroundings must not give a bright coloured lamp an arbitrarily large achromatic pedestal.

/// Photopic luminance per unit of each band, cd m^-2 per W m^-2 sr^-1 nm^-1 (section 1.2).
pub const PHOTOPIC_WEIGHTS: [f64; 3] = [12_458.6, 52_558.9, 7_965.8];
/// Scotopic luminance per unit of each band, scotopic cd m^-2 per W m^-2 sr^-1 nm^-1.
pub const SCOTOPIC_WEIGHTS: [f64; 3] = [2_322.5, 89_437.1, 73_261.6];
/// A spectrally flat radiance of one renderer unit has this luminance (the photopic sum).
pub const LUMINANCE_PER_UNIT_RADIANCE: f64 = 72_983.3;

pub const METER_MIN_CD_M2: f64 = 1.0e-7;
pub const METER_MAX_CD_M2: f64 = 1.0e7;
pub const METER_FOVEA_RADIUS_DEG: f64 = 1.0;
pub const METER_GAUSSIAN_SIGMA_DEG: f64 = 10.0;
// OWNER-DIRECTED (2026-10-04): the eye followed the spot under the crosshair so closely that a
// glance at a port blew the port out and blacked the cabin, and a glance back reversed it.
// The whole field now carries most of the weight, and the adaptation rates below are slower.
pub const METER_FOVEA_WEIGHT: f64 = 0.15;
pub const METER_GAUSSIAN_WEIGHT: f64 = 0.35;
pub const METER_FIELD_WEIGHT: f64 = 0.50;
/// Sample grid of the GPU meter, identical in both presets.
pub const METER_GRID: (u32, u32) = (256, 144);

pub const PUPIL_AGE_YEARS: f64 = 25.0;
pub const BINOCULAR_FIELD_DEG2: f64 = 3_000.0;
/// One eye at a 25 deg sight field (section 6.3).
pub const SIGHT_FIELD_DEG2: f64 = 491.0;
pub const PUPIL_CONSTRICT_TAU_S: f64 = 1.2;
pub const PUPIL_DILATE_FAST_TAU_S: f64 = 2.0;
pub const PUPIL_DILATE_SLOW_TAU_S: f64 = 15.0;
pub const PUPIL_SLOW_SHARE: f64 = 0.3;

pub const PIGMENT_KM: f64 = 0.2;
pub const CONE_REGENERATION_PER_S: f64 = 0.5 / 60.0;
pub const CONE_PHOTOSENSITIVITY_TD_S: f64 = 2.1e6;
/// 0.045 /min: the rate at which the research's own statement holds, a full bleach falling to
/// 10 % residual in about 30 min under the rate-limited law [D]. Its derivation from Rushton's
/// 400 s time constant (0.03 /min) takes 45 min to do that and 57 min to bring the rods within
/// half a log unit of their absolute threshold, outside the classical 30-40 min.
pub const ROD_REGENERATION_PER_S: f64 = 0.045 / 60.0;
pub const ROD_PHOTOSENSITIVITY_SCOT_TD_S: f64 = 1.0e7;
pub const CONE_BLEACH_LOG_UNITS: f64 = 3.0;
pub const ROD_BLEACH_LOG_UNITS: f64 = 20.0;

pub const CONE_TAU_UP_S: f64 = 4.5;
pub const CONE_TAU_DOWN_S: f64 = 6.0;
pub const ROD_TAU_UP_S: f64 = 5.0;
pub const ROD_TAU_DOWN_S: f64 = 8.0;

pub const DISPLAY_ADAPTATION_CD_M2: f64 = 100.0;
pub const DISPLAY_KEY: f64 = 0.18;
pub const ROD_SATURATION_SCOT_TD: f64 = 1_700.0;
/// How much brighter than the threshold-matched level the screen shows the rod signal. At 1 a
/// moonlit field is reproduced as dim as it is to a dark-adapted eye, which on a monitor in a
/// lit room is too dark to play by (owner, 2026-10-04). This is a reproduction choice named in
/// docs/PHYSICAL_TRUTH_AUDIT.md: it changes no light, no adaptation state and no daylight
/// frame (the rod share is zero in photopic light). `Mapping::aces_input` stays the eye model
/// without it, so the research gates still test the model; only `crush.wgsl` applies it.
pub const ROD_DISPLAY_GAIN: f64 = 6.0;

/// A sample brighter than this multiple of the cone adaptation counts as flash light [E].
pub const FLASH_ABOVE_ADAPTATION: f64 = 30.0;
/// Flash energy fades over minutes: the 1968 review found no cumulative effect of successive
/// flashes 3-4 min apart [E from M].
pub const FLASH_MEMORY_TAU_S: f64 = 180.0;
pub const FLASH_MIN_RECOVERY_S: f64 = 0.2;
/// Coverage below which a frame holds no flash.
pub const FLASH_MIN_COVERAGE: f64 = 1.0e-3;
/// Flash detection requires a temporal field-energy rise, not merely contrast with a dark cabin.
/// This 120ms onset reference is a bounded reproduction detector, not another adaptation state.
pub const FLASH_ONSET_TAU_S: f64 = 0.12;
pub const FLASH_ONSET_RATIO: f64 = 1.5;

/// Largest step the integrator takes; longer frames are substepped.
pub const MAX_STEP_S: f64 = 0.25;

/// What the meter reports for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meter {
    /// Field-weighted arithmetic-average photopic luminance, cd/m^2.
    pub photopic_cd_m2: f64,
    /// Field-weighted arithmetic-average scotopic luminance, scotopic cd/m^2.
    pub scotopic_cd_m2: f64,
    /// Fraction of the field brighter than `FLASH_ABOVE_ADAPTATION` times the cone adaptation.
    pub bright_coverage: f64,
    /// Mean photopic luminance of that bright part, cd/m^2.
    pub bright_cd_m2: f64,
    /// Unweighted arithmetic field luminance. Unlike thresholded bright coverage, it does not
    /// disappear/reappear when the eye's flash threshold crosses a steady bright aperture.
    pub field_cd_m2: f64,
    /// At least one genuine transient emitting source exists in this scene observation.
    /// This only admits flash detection; measured visible radiance still supplies its energy.
    pub transient_source: bool,
}

impl Meter {
    /// A uniform field of the given luminances.
    pub fn uniform(photopic_cd_m2: f64, scotopic_cd_m2: f64) -> Self {
        Self { photopic_cd_m2, scotopic_cd_m2, bright_coverage: 0.0, bright_cd_m2: 0.0,
            field_cd_m2:photopic_cd_m2,transient_source:false }
    }

    fn clamped(self) -> Self {
        Self {
            photopic_cd_m2: clamp_luminance(self.photopic_cd_m2),
            scotopic_cd_m2: clamp_luminance(self.scotopic_cd_m2),
            bright_coverage: if self.bright_coverage.is_finite() { self.bright_coverage.clamp(0.0, 1.0) } else { 0.0 },
            bright_cd_m2: if self.bright_cd_m2.is_finite() { self.bright_cd_m2.clamp(0.0, METER_MAX_CD_M2) } else { 0.0 },
            field_cd_m2:clamp_luminance(self.field_cd_m2),
            transient_source:self.transient_source,
        }
    }
}

fn clamp_luminance(value: f64) -> f64 {
    if value.is_finite() { value.clamp(METER_MIN_CD_M2, METER_MAX_CD_M2) } else { METER_MIN_CD_M2 }
}

/// How the eye views the scene: both eyes over the natural field, or one eye at a sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Viewing {
    Binocular,
    // The GPU eye takes the sight from the clock's `monocular` lane.
    #[cfg_attr(not(test), allow(dead_code))]
    MonocularSight,
}

impl Viewing {
    fn field(self) -> (f64, f64) {
        match self {
            Self::Binocular => (BINOCULAR_FIELD_DEG2, 1.0),
            Self::MonocularSight => (SIGHT_FIELD_DEG2, 0.1),
        }
    }
}

/// Steady-state pupil diameter, mm (Watson & Yellott 2012, the Stanley-Davies term with age).
pub fn pupil_diameter_mm(luminance_cd_m2: f64, viewing: Viewing) -> f64 {
    let (area, eyes) = viewing.field();
    let x = (clamp_luminance(luminance_cd_m2) * area * eyes / 846.0).powf(0.41);
    let stanley_davies = 7.75 - 5.75 * (x / (x + 2.0));
    (stanley_davies + (PUPIL_AGE_YEARS - 28.58) * (0.021_323 - 0.009_562_3 * stanley_davies))
        .clamp(1.5, 9.0)
}

pub fn pupil_area_mm2(diameter_mm: f64) -> f64 {
    std::f64::consts::PI * 0.25 * diameter_mm * diameter_mm
}

/// Cone increment threshold, log10 cd/m^2, at adaptation log10 cd/m^2 (Ferwerda 1996).
pub fn cone_tvi_log(adaptation_log: f64) -> f64 {
    if adaptation_log <= -2.6 {
        -0.72
    } else if adaptation_log < 1.9 {
        (0.249 * adaptation_log + 0.65).powf(2.7) - 0.72
    } else {
        adaptation_log - 1.255
    }
}

/// Rod increment threshold, log10 scotopic cd/m^2 (Ferwerda 1996).
pub fn rod_tvi_log(adaptation_log: f64) -> f64 {
    if adaptation_log <= -3.94 {
        -2.86
    } else if adaptation_log < -1.44 {
        (0.405 * adaptation_log + 1.6).powf(2.18) - 2.86
    } else {
        adaptation_log - 0.395
    }
}

/// The background whose cone threshold equals `threshold_log` (log10 cd/m^2): the "equivalent
/// background" of a bleached eye. At or below the absolute threshold it is the TVI knee.
pub fn cone_equivalent_background_log(threshold_log: f64) -> f64 {
    let knee = (0.249 * 1.9 + 0.65f64).powf(2.7) - 0.72;
    if threshold_log <= -0.72 {
        -2.6
    } else if threshold_log < knee {
        ((threshold_log + 0.72).powf(1.0 / 2.7) - 0.65) / 0.249
    } else {
        threshold_log + 1.255
    }
}

pub fn rod_equivalent_background_log(threshold_log: f64) -> f64 {
    let knee = (0.405 * -1.44 + 1.6f64).powf(2.18) - 2.86;
    if threshold_log <= -2.86 {
        -3.94
    } else if threshold_log < knee {
        ((threshold_log + 2.86).powf(1.0 / 2.18) - 1.6) / 0.405
    } else {
        threshold_log + 0.395
    }
}

/// CIE 191:2010 mesopic adaptation coefficient m from photopic and scotopic adaptation
/// luminances (iterated from m = 0.5; 0 scotopic, 1 photopic).
pub fn mesopic_coefficient(photopic_cd_m2: f64, scotopic_cd_m2: f64) -> f64 {
    const V_PRIME_555: f64 = 683.0 / 1699.0;
    let (lp, ls) = (clamp_luminance(photopic_cd_m2), clamp_luminance(scotopic_cd_m2));
    let mut m = 0.5;
    for _ in 0..12 {
        let mesopic = (m * lp + (1.0 - m) * ls * V_PRIME_555) / (m + (1.0 - m) * V_PRIME_555);
        m = (0.767 + 0.3334 * mesopic.max(METER_MIN_CD_M2).log10()).clamp(0.0, 1.0);
    }
    m
}

/// Steady-state bleached fraction for a retinal illuminance (td or scotopic td) under the
/// rate-limited regeneration law: v B / (B + K) = (1 - B) I / Q.
pub fn steady_bleach(retinal: f64, regeneration_per_s: f64, photosensitivity: f64) -> f64 {
    let k = retinal.max(0.0) / photosensitivity;
    if k <= 0.0 { return 0.0; }
    let b = regeneration_per_s - k + k * PIGMENT_KM;
    ((-b + (b * b + 4.0 * k * k * PIGMENT_KM).sqrt()) / (2.0 * k)).clamp(0.0, 1.0)
}

/// Afterimage background after a flash of retinal energy `energy_td_s`, `since_s` seconds after
/// it ended: the luminance a target needs to be recognised again, from the recovery fit.
pub fn flash_afterimage_cd_m2(energy_td_s: f64, since_s: f64) -> f64 {
    if energy_td_s <= 0.0 { return 0.0; }
    let t = since_s.max(FLASH_MIN_RECOVERY_S);
    (0.19 * (20.0 / t).powf(2.5) * (energy_td_s / 9.0e5).powf(1.325)).min(METER_MAX_CD_M2)
}

/// The eye of one view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub pupil_fast_mm: f64,
    pub pupil_slow_mm: f64,
    pub cone_bleach: f64,
    pub rod_bleach: f64,
    /// Neural adaptation, log10 cd/m^2 (cones) and log10 scotopic cd/m^2 (rods).
    pub cone_neural_log: f64,
    pub rod_neural_log: f64,
    pub flash_energy_td_s: f64,
    pub since_flash_s: f64,
    pub flash_field_reference_cd_m2:f64,
}

/// What the display transform needs from the eye, and what the eye is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// ACES input per renderer unit of radiance, applied to RGB (the cone channel keeps colour).
    pub cone_exposure: f64,
    /// ACES input per scotopic cd/m^2 of the saturating rod signal, added to all three channels.
    pub rod_exposure: f64,
    /// Rod saturation: signal = Ys / (1 + Ys * rod_inverse_saturation), Ys in scotopic cd/m^2.
    pub rod_inverse_saturation: f64,
    pub cone_adaptation_cd_m2: f64,
    pub rod_adaptation_scot_cd_m2: f64,
    pub mesopic_m: f64,
    pub pupil_mm: f64,
    pub afterimage_cd_m2: f64,
}

impl Eye {
    /// The eye fully adapted to a steady field: the state a view starts in before its first
    /// presented frame.
    pub fn adapted(meter: Meter, viewing: Viewing) -> Self {
        let meter = meter.clamped();
        let pupil = pupil_diameter_mm(meter.photopic_cd_m2, viewing);
        let area = pupil_area_mm2(pupil);
        Self {
            pupil_fast_mm: pupil,
            pupil_slow_mm: pupil,
            cone_bleach: steady_bleach(meter.photopic_cd_m2 * area, CONE_REGENERATION_PER_S,
                CONE_PHOTOSENSITIVITY_TD_S),
            rod_bleach: steady_bleach(meter.scotopic_cd_m2 * area, ROD_REGENERATION_PER_S,
                ROD_PHOTOSENSITIVITY_SCOT_TD_S),
            cone_neural_log: meter.photopic_cd_m2.log10(),
            rod_neural_log: meter.scotopic_cd_m2.log10(),
            flash_energy_td_s: 0.0,
            since_flash_s: 1.0e6,
            flash_field_reference_cd_m2:meter.field_cd_m2,
        }
    }

    pub fn pupil_mm(&self) -> f64 {
        self.pupil_fast_mm * (1.0 - PUPIL_SLOW_SHARE) + self.pupil_slow_mm * PUPIL_SLOW_SHARE
    }

    /// Advance by `dt_s` seconds of simulated time with this frame's meter. The CPU statement of
    /// what `exposure.wgsl` integrates on every frame (tests hold the two together).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn step(&mut self, meter: Meter, viewing: Viewing, dt_s: f64) {
        if !(dt_s > 0.0) { return; }
        let meter = meter.clamped();
        let substeps = (dt_s / MAX_STEP_S).ceil().clamp(1.0, 4096.0);
        let dt = dt_s / substeps;
        for _ in 0..substeps as u32 {
            self.substep(meter, viewing, dt);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn substep(&mut self, meter: Meter, viewing: Viewing, dt: f64) {
        let relax = |value: f64, target: f64, tau: f64| value + (target - value) * (1.0 - (-dt / tau).exp());
        // Pupil.
        let target = pupil_diameter_mm(meter.photopic_cd_m2, viewing);
        let (fast_tau, slow_tau) = if target < self.pupil_fast_mm {
            (PUPIL_CONSTRICT_TAU_S, PUPIL_CONSTRICT_TAU_S)
        } else {
            (PUPIL_DILATE_FAST_TAU_S, PUPIL_DILATE_SLOW_TAU_S)
        };
        self.pupil_fast_mm = relax(self.pupil_fast_mm, target, fast_tau);
        self.pupil_slow_mm = relax(self.pupil_slow_mm, target,
            if target < self.pupil_slow_mm { PUPIL_CONSTRICT_TAU_S } else { slow_tau });
        let area = pupil_area_mm2(self.pupil_mm());
        // Pigments: exact bleaching over the step (unbleached pigment decays at I/Q), then
        // rate-limited regeneration.
        let bleach = |b: f64, retinal: f64, v: f64, q: f64| {
            let bleached = 1.0 - (1.0 - b) * (-(retinal / q) * dt).exp();
            (bleached - v * bleached / (bleached + PIGMENT_KM) * dt).clamp(0.0, 1.0)
        };
        self.cone_bleach = bleach(self.cone_bleach, meter.photopic_cd_m2 * area,
            CONE_REGENERATION_PER_S, CONE_PHOTOSENSITIVITY_TD_S);
        self.rod_bleach = bleach(self.rod_bleach, meter.scotopic_cd_m2 * area,
            ROD_REGENERATION_PER_S, ROD_PHOTOSENSITIVITY_SCOT_TD_S);
        // Neural adaptation to the light the retina actually receives: a pupil still constricted
        // after the light falls passes less of it than the steady pupil for that field would.
        let steady_area = pupil_area_mm2(target);
        let retinal_ratio_log = (area / steady_area).log10();
        let cone_target = meter.photopic_cd_m2.log10() + retinal_ratio_log;
        let rod_target = meter.scotopic_cd_m2.log10() + retinal_ratio_log;
        self.cone_neural_log = relax(self.cone_neural_log, cone_target,
            if cone_target > self.cone_neural_log { CONE_TAU_UP_S } else { CONE_TAU_DOWN_S });
        self.rod_neural_log = relax(self.rod_neural_log, rod_target,
            if rod_target > self.rod_neural_log { ROD_TAU_UP_S } else { ROD_TAU_DOWN_S });
        // Flash energy on the part of the retina that saw it.
        let cone_adaptation = self.cone_adaptation_cd_m2();
        // A steady high-contrast port is not a new flash on every frame. The detector uses
        // all transported field energy, independent of the adaptation-dependent bright mask.
        let rising=meter.field_cd_m2 > FLASH_ONSET_RATIO*self.flash_field_reference_cd_m2;
        self.flash_field_reference_cd_m2=relax(self.flash_field_reference_cd_m2,
            meter.field_cd_m2,FLASH_ONSET_TAU_S);
        let flashing = meter.transient_source && rising && meter.bright_coverage > FLASH_MIN_COVERAGE
            && meter.bright_cd_m2 > FLASH_ABOVE_ADAPTATION * cone_adaptation;
        self.flash_energy_td_s *= (-dt / FLASH_MEMORY_TAU_S).exp();
        if flashing {
            self.flash_energy_td_s += meter.bright_coverage * meter.bright_cd_m2 * area * dt;
            self.since_flash_s = 0.0;
        } else {
            self.since_flash_s = (self.since_flash_s + dt).min(1.0e6);
        }
    }

    fn afterimage_cd_m2(&self) -> f64 {
        flash_afterimage_cd_m2(self.flash_energy_td_s, self.since_flash_s)
    }

    /// Effective cone adaptation: the larger of the neural state and the bleached eye's
    /// equivalent background, plus the flash afterimage.
    pub fn cone_adaptation_cd_m2(&self) -> f64 {
        let bleached = cone_equivalent_background_log(-0.72 + CONE_BLEACH_LOG_UNITS * self.cone_bleach);
        10f64.powf(self.cone_neural_log.max(bleached)) + self.afterimage_cd_m2()
    }

    pub fn rod_adaptation_scot_cd_m2(&self) -> f64 {
        let bleached = rod_equivalent_background_log(-2.86 + ROD_BLEACH_LOG_UNITS * self.rod_bleach);
        10f64.powf(self.rod_neural_log.max(bleached)) + self.afterimage_cd_m2()
    }

    pub fn mapping(&self, viewing: Viewing) -> Mapping {
        let cone = self.cone_adaptation_cd_m2();
        let rod = self.rod_adaptation_scot_cd_m2();
        let display_threshold = cone_tvi_log(DISPLAY_ADAPTATION_CD_M2.log10());
        let cone_scale = 10f64.powf(display_threshold - cone_tvi_log(cone.log10()));
        let rod_scale = 10f64.powf(display_threshold - rod_tvi_log(rod.log10()));
        let m = mesopic_coefficient(cone, rod);
        let key = DISPLAY_KEY / DISPLAY_ADAPTATION_CD_M2;
        let pupil = self.pupil_mm();
        // The adaptation states are expressed as the luminance a steady pupil would admit. The
        // scene reaches the retina through the pupil as it is now: a pupil still constricted
        // after the light falls shows the scene darker, one still dilated after a flash brighter.
        let pupil_gain = pupil_area_mm2(pupil) / pupil_area_mm2(pupil_diameter_mm(cone, viewing));
        Mapping {
            cone_exposure: key * cone_scale * LUMINANCE_PER_UNIT_RADIANCE * pupil_gain,
            rod_exposure: key * (1.0 - m) * rod_scale * pupil_gain,
            rod_inverse_saturation: pupil_area_mm2(pupil) / ROD_SATURATION_SCOT_TD,
            cone_adaptation_cd_m2: cone,
            rod_adaptation_scot_cd_m2: rod,
            mesopic_m: m,
            pupil_mm: pupil,
            afterimage_cd_m2: self.afterimage_cd_m2(),
        }
    }
}

impl Mapping {
    /// ACES input of a spectral-radiance triplet (what `crush.wgsl` computes per pixel).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aces_input(&self, radiance: [f64; 3]) -> [f64; 3] {
        let scotopic: f64 = (0..3).map(|i| radiance[i] * SCOTOPIC_WEIGHTS[i]).sum();
        let rods = rod_response(scotopic, self.rod_exposure, self.rod_inverse_saturation);
        radiance.map(|c| c * self.cone_exposure + rods)
    }
}

/// Display-only rod response. Preserve the view's adaptation when it is less sensitive, but do
/// not apply its dark-field threshold gain beyond the local stimulus's existing rod TVI curve.
/// Both candidate responses are nondecreasing with stimulus, so their minimum is nondecreasing.
/// No light, eye state, cone gain, or display encoding is changed here. This does not make the
/// subsequent peak-channel tone curve luminance-preserving for every saturated RGB spectrum.
pub fn rod_response(scotopic: f64, gain: f64, inverse_saturation: f64) -> f64 {
    let stimulus = scotopic.max(0.0);
    let signal = stimulus / (1.0 + stimulus * inverse_saturation);
    let local_key = DISPLAY_KEY / DISPLAY_ADAPTATION_CD_M2
        * 10f64.powf(cone_tvi_log(DISPLAY_ADAPTATION_CD_M2.log10()));
    let raw_local_response = local_key * signal
        / 10f64.powf(rod_tvi_log(signal.max(METER_MIN_CD_M2).log10()));
    // The published rounded TVI coefficients leave a 16 ppm downward response step at the
    // absolute-threshold knee. Its monotone envelope closes that rounding seam only; the
    // eye's adaptation TVI and the response on either side remain unchanged.
    let absolute_response = local_key * signal.min(10f64.powf(-3.94)) / 10f64.powf(-2.86);
    let local_response = raw_local_response.max(absolute_response);
    (gain * signal).min(local_response)
}

/// The ACES fit `crush.wgsl` applies (Narkowicz), the one tone curve of the display.
#[cfg_attr(not(test), allow(dead_code))]
pub fn aces(x: f64) -> f64 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// Radiance triplet of a spectrally flat surface of the given luminance.
#[cfg_attr(not(test), allow(dead_code))]
pub fn flat_radiance(luminance_cd_m2: f64) -> [f64; 3] {
    [luminance_cd_m2 / LUMINANCE_PER_UNIT_RADIANCE; 3]
}

/// Constants for `shaders/exposure.wgsl`, generated here so both presets and the GPU share one
/// definition.
pub fn wgsl_constants() -> String {
    let mut out = String::from("// Generated from src/observer.rs (observer::wgsl_constants).\n");
    let mut push = |name: &str, value: f64| {
        out.push_str(&format!("const {name}: f32 = {value:e};\n"));
    };
    push("OBS_PHOTOPIC_R", PHOTOPIC_WEIGHTS[0]);
    push("OBS_PHOTOPIC_G", PHOTOPIC_WEIGHTS[1]);
    push("OBS_PHOTOPIC_B", PHOTOPIC_WEIGHTS[2]);
    push("OBS_SCOTOPIC_R", SCOTOPIC_WEIGHTS[0]);
    push("OBS_SCOTOPIC_G", SCOTOPIC_WEIGHTS[1]);
    push("OBS_SCOTOPIC_B", SCOTOPIC_WEIGHTS[2]);
    push("OBS_LUMINANCE_PER_UNIT_RADIANCE", LUMINANCE_PER_UNIT_RADIANCE);
    push("OBS_METER_MIN", METER_MIN_CD_M2);
    push("OBS_METER_MAX", METER_MAX_CD_M2);
    push("OBS_FOVEA_RADIUS_DEG", METER_FOVEA_RADIUS_DEG);
    push("OBS_GAUSSIAN_SIGMA_DEG", METER_GAUSSIAN_SIGMA_DEG);
    push("OBS_FOVEA_WEIGHT", METER_FOVEA_WEIGHT);
    push("OBS_GAUSSIAN_WEIGHT", METER_GAUSSIAN_WEIGHT);
    push("OBS_FIELD_WEIGHT", METER_FIELD_WEIGHT);
    push("OBS_GRID_X", METER_GRID.0 as f64);
    push("OBS_GRID_Y", METER_GRID.1 as f64);
    push("OBS_AGE", PUPIL_AGE_YEARS);
    push("OBS_BINOCULAR_FIELD", BINOCULAR_FIELD_DEG2);
    push("OBS_SIGHT_FIELD", SIGHT_FIELD_DEG2);
    push("OBS_PUPIL_CONSTRICT_TAU", PUPIL_CONSTRICT_TAU_S);
    push("OBS_PUPIL_DILATE_FAST_TAU", PUPIL_DILATE_FAST_TAU_S);
    push("OBS_PUPIL_DILATE_SLOW_TAU", PUPIL_DILATE_SLOW_TAU_S);
    push("OBS_PUPIL_SLOW_SHARE", PUPIL_SLOW_SHARE);
    push("OBS_PIGMENT_KM", PIGMENT_KM);
    push("OBS_CONE_REGEN", CONE_REGENERATION_PER_S);
    push("OBS_CONE_Q", CONE_PHOTOSENSITIVITY_TD_S);
    push("OBS_ROD_REGEN", ROD_REGENERATION_PER_S);
    push("OBS_ROD_Q", ROD_PHOTOSENSITIVITY_SCOT_TD_S);
    push("OBS_CONE_BLEACH_LOG", CONE_BLEACH_LOG_UNITS);
    push("OBS_ROD_BLEACH_LOG", ROD_BLEACH_LOG_UNITS);
    push("OBS_CONE_TAU_UP", CONE_TAU_UP_S);
    push("OBS_CONE_TAU_DOWN", CONE_TAU_DOWN_S);
    push("OBS_ROD_TAU_UP", ROD_TAU_UP_S);
    push("OBS_ROD_TAU_DOWN", ROD_TAU_DOWN_S);
    push("OBS_DISPLAY_ADAPTATION", DISPLAY_ADAPTATION_CD_M2);
    push("OBS_DISPLAY_KEY", DISPLAY_KEY);
    push("OBS_ROD_SATURATION_TD", ROD_SATURATION_SCOT_TD);
    push("OBS_ROD_DISPLAY_GAIN", std::env::var("KESSEL_ROD_DISPLAY_GAIN").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(ROD_DISPLAY_GAIN));
    push("OBS_FLASH_ABOVE_ADAPTATION", FLASH_ABOVE_ADAPTATION);
    push("OBS_FLASH_MEMORY_TAU", FLASH_MEMORY_TAU_S);
    push("OBS_FLASH_MIN_RECOVERY", FLASH_MIN_RECOVERY_S);
    push("OBS_FLASH_MIN_COVERAGE", FLASH_MIN_COVERAGE);
    push("OBS_FLASH_ONSET_TAU", FLASH_ONSET_TAU_S);
    push("OBS_FLASH_ONSET_RATIO", FLASH_ONSET_RATIO);
    push("OBS_MAX_STEP", MAX_STEP_S);
    push("OBS_ROD_LOCAL_TVI_KEY", DISPLAY_KEY / DISPLAY_ADAPTATION_CD_M2
        * 10f64.powf(cone_tvi_log(DISPLAY_ADAPTATION_CD_M2.log10())));
    push("OBS_ROD_ABSOLUTE_KNEE", 10f64.powf(-3.94));
    push("OBS_ROD_ABSOLUTE_THRESHOLD", 10f64.powf(-2.86));
    push("OBS_ROD_WEBER_KNEE", 10f64.powf(-1.44));
    push("OBS_ROD_WEBER_RESPONSE", DISPLAY_KEY / DISPLAY_ADAPTATION_CD_M2
        * 10f64.powf(cone_tvi_log(DISPLAY_ADAPTATION_CD_M2.log10()) + 0.395));
    // Inject one implementation into every observer-aware shader. It operates on linear
    // display input, before tone mapping/encoding, and does not modify the exposure meter.
    out.push_str(r#"
fn obs_local_rod_tvi_log(l: f32) -> f32 {
    if (l <= -3.94) { return -2.86; }
    if (l < -1.44) { return pow(0.405 * l + 1.6, 2.18) - 2.86; }
    return l - 0.395;
}
fn obs_rod_response(scotopic: f32, gain: f32, inverse_saturation: f32) -> f32 {
    let stimulus = max(scotopic, 0.0);
    let signal = stimulus / (1.0 + stimulus * inverse_saturation);
    // Exact algebraic branches avoid logarithms/powers for black, deep night and bright pixels.
    if (signal <= OBS_ROD_ABSOLUTE_KNEE) {
        return min(gain * signal, OBS_ROD_LOCAL_TVI_KEY * signal / OBS_ROD_ABSOLUTE_THRESHOLD);
    }
    if (signal >= OBS_ROD_WEBER_KNEE) {
        return min(gain * signal, OBS_ROD_WEBER_RESPONSE);
    }
    let local_log = log2(max(signal, OBS_METER_MIN)) * 0.30102999566;
    let raw_local_response = OBS_ROD_LOCAL_TVI_KEY * signal
        / pow(10.0, obs_local_rod_tvi_log(local_log));
    let absolute_response = OBS_ROD_LOCAL_TVI_KEY * min(signal, OBS_ROD_ABSOLUTE_KNEE)
        / OBS_ROD_ABSOLUTE_THRESHOLD;
    let local_response = max(raw_local_response, absolute_response);
    return min(gain * signal, local_response);
}
"#);
    out
}

#[cfg(test)]
#[path = "observer_meter_tests.rs"]
mod spatial_meter_tests;

#[cfg(test)]
mod tests {
    use super::*;

    const DAWN_SNOW: (f64, f64) = (877.0, 877.0 * 2.35);

    fn run(eye: &mut Eye, meter: Meter, seconds: f64, dt: f64) {
        let mut t = 0.0;
        while t < seconds {
            eye.step(meter, Viewing::Binocular, dt);
            t += dt;
        }
    }

    /// The visual threshold for a daylight-white test light, log10 cd/m^2: the better of the
    /// cones and the rods (rod threshold divided by the white's S/P ratio of 2.35).
    fn white_threshold_log(eye: &Eye) -> (f64, f64) {
        let cone = cone_tvi_log(eye.cone_adaptation_cd_m2().log10());
        let rod = rod_tvi_log(eye.rod_adaptation_scot_cd_m2().log10()) - 2.35f64.log10();
        (cone, rod)
    }

    #[test]
    fn pupil_follows_the_unified_formula_and_its_published_dynamics() {
        for (luminance, expected) in [(1.0e-4, 7.8), (1.0e-2, 7.3), (1.0, 5.2), (100.0, 2.9), (1.0e4, 2.15)] {
            let d = pupil_diameter_mm(luminance, Viewing::Binocular);
            assert!((d - expected).abs() < 0.15, "{luminance} cd/m^2: {d} mm vs {expected}");
        }
        let sight = pupil_diameter_mm(1.0, Viewing::MonocularSight);
        assert!((sight - 7.1).abs() < 0.2, "monocular sight {sight}");
        // Constriction completes in about a second; dilation takes seconds and has a slow tail.
        let mut eye = Eye::adapted(Meter::uniform(0.05, 0.12), Viewing::Binocular);
        run(&mut eye, Meter::uniform(500.0, 1175.0), 4.0, 1.0 / 60.0);
        let bright = pupil_diameter_mm(500.0, Viewing::Binocular);
        // 1.2 s constriction: within 5 % of the swing after four seconds.
        let swing = pupil_diameter_mm(0.05, Viewing::Binocular) - bright;
        assert!(eye.pupil_mm() - bright < 0.05 * swing, "constricted {} vs {bright}", eye.pupil_mm());
        run(&mut eye, Meter::uniform(0.05, 0.12), 2.0, 1.0 / 60.0);
        let dark = pupil_diameter_mm(0.05, Viewing::Binocular);
        assert!(dark - eye.pupil_mm() > 1.0, "dilation takes seconds: {}", eye.pupil_mm());
        run(&mut eye, Meter::uniform(0.05, 0.12), 60.0, 1.0 / 60.0);
        assert!((dark - eye.pupil_mm()).abs() < 0.02);
    }

    #[test]
    fn a_steady_photopic_scene_is_shown_at_the_key_and_the_eye_is_stable() {
        for luminance in [100.0, 877.0, 5_000.0] {
            let meter = Meter::uniform(luminance, luminance * 2.35);
            let mut eye = Eye::adapted(meter, Viewing::Binocular);
            let before = eye.mapping(Viewing::Binocular);
            let x = before.aces_input(flat_radiance(luminance));
            assert!((x[1] / DISPLAY_KEY - 1.0).abs() < 0.08, "{luminance}: {x:?}");
            run(&mut eye, meter, 30.0, 1.0 / 60.0);
            let after = eye.mapping(Viewing::Binocular);
            assert!((after.cone_exposure / before.cone_exposure - 1.0).abs() < 1e-3,
                "an adapted eye stays adapted: {before:?} {after:?}");
        }
    }

    #[test]
    fn the_night_display_gain_is_a_named_bounded_departure_applied_only_by_the_display() {
        assert!((1.0..=8.0).contains(&ROD_DISPLAY_GAIN));
        let crush = include_str!("../shaders/crush.wgsl");
        assert_eq!(crush.matches("OBS_ROD_DISPLAY_GAIN").count(), 1);
        assert!(crush.contains("vec3(rods * OBS_ROD_DISPLAY_GAIN)"));
        assert!(wgsl_constants().contains("const OBS_ROD_DISPLAY_GAIN: f32 = "));
        assert!(include_str!("../docs/PHYSICAL_TRUTH_AUDIT.md").contains("ROD_DISPLAY_GAIN"));
    }

    #[test]
    fn the_display_mapping_is_monotonic_and_a_moonless_night_is_dark() {
        // Brighter surfaces are never shown darker, at any adaptation.
        for adaptation in [1.0e-4, 1.0e-2, 0.05, 1.0, 100.0, 1.0e4] {
            let eye = Eye::adapted(Meter::uniform(adaptation, adaptation * 2.1), Viewing::Binocular);
            let map = eye.mapping(Viewing::Binocular);
            let mut previous = -1.0;
            for step in 0..200 {
                let luminance = adaptation * 10f64.powf(-4.0 + step as f64 * 0.05);
                let shown = aces(map.aces_input(flat_radiance(luminance))[1]);
                assert!(shown >= previous, "{adaptation}: not monotonic at {luminance}");
                previous = shown;
            }
        }
        // Each adapted scene's own surface is shown darker as the scene gets darker, decade by
        // decade. Within the mesopic range the rods' growing weight lifts it by at most a third
        // (the model's composition: full cones plus (1 - m) rods), never across a decade.
        let shown_at = |adaptation: f64| {
            let eye = Eye::adapted(Meter::uniform(adaptation, adaptation * 2.2), Viewing::Binocular);
            eye.mapping(Viewing::Binocular).aces_input(flat_radiance(adaptation))[1]
        };
        let decades = [1.0e4, 1.0e3, 100.0, 10.0, 1.0, 0.1, 1.0e-2, 1.0e-3, 1.0e-4, 1.0e-5];
        for pair in decades.windows(2) {
            let (brighter, darker) = (shown_at(pair[0]), shown_at(pair[1]));
            assert!(darker <= brighter * 1.35, "{pair:?}: {brighter} -> {darker}");
        }
        assert!(shown_at(1.0e-4) < 0.2 * shown_at(1.0), "night is darker than dusk");
        assert!(shown_at(1.0) < shown_at(1.0e3), "dusk is darker than day");
        // Research gate: a moonless scene maps under 2 % of display white (section 7.9 (d)).
        for (snow, scotopic) in [(1.8e-4, 3.8e-4), (6.0e-5, 1.3e-4)] {
            let eye = Eye::adapted(Meter::uniform(snow, scotopic), Viewing::Binocular);
            let mut radiance = flat_radiance(snow);
            // Give the snow its scotopic luminance: the night sky's S/P is about 2.1.
            radiance[2] *= 1.4;
            let shown = aces(eye.mapping(Viewing::Binocular).aces_input(radiance)[1]);
            assert!(shown < 0.02, "moonless snow {snow}: {shown}");
            assert!(shown > 0.0, "the night is dark, not erased");
        }
        // Moonlit snow is dark but legible, and mostly rods.
        let moon = Eye::adapted(Meter::uniform(0.049, 0.115), Viewing::Binocular)
            .mapping(Viewing::Binocular);
        let shown = aces(moon.aces_input(flat_radiance(0.049))[1]);
        assert!((0.003..0.08).contains(&shown), "full-Moon snow {shown}");
        assert!(moon.mesopic_m < 0.6);
    }

    #[test]
    fn dark_adaptation_has_a_cone_plateau_and_reaches_rod_threshold_at_thirty_to_forty_minutes() {
        // Research gate (section 7.9 (c)): the cone plateau within 10 minutes, the rods within
        // half a log unit of their absolute threshold at 30-40 minutes, after the classical
        // reference condition: a total bleach, then total darkness.
        let mut eye = Eye::adapted(Meter::uniform(DAWN_SNOW.0, DAWN_SNOW.1), Viewing::Binocular);
        eye.cone_bleach = 1.0;
        eye.rod_bleach = 1.0;
        let dark = Meter::uniform(1.0e-7, 1.0e-7);
        let mut cone_curve = Vec::new();
        let mut rod_curve = Vec::new();
        for minute in 0..=45 {
            let (cone, rod) = white_threshold_log(&eye);
            cone_curve.push(cone);
            rod_curve.push(rod);
            if minute < 45 { run(&mut eye, dark, 60.0, 0.25); }
        }
        assert!(cone_curve[10] - cone_curve[45] < 0.05, "cone plateau {:?}", &cone_curve[..12]);
        assert!(cone_curve[0] - cone_curve[10] > 0.3, "cones recover first: {cone_curve:?}");
        let absolute = -2.86 - 2.35f64.log10();
        let within = (0..=45).find(|m| rod_curve[*m] - absolute < 0.5).unwrap_or(99);
        assert!((30..=40).contains(&within), "rods within 0.5 log units at {within} min: {rod_curve:?}");
        // Named, not hidden: with the research's rod constants the rod branch crosses the cone
        // plateau late (the classical rod-cone break after a strong bleach is 7-12 min).
        let crossing = (0..=45).find(|m| rod_curve[*m] < cone_curve[*m]).unwrap_or(99);
        eprintln!("after a total bleach: rod-cone break {crossing} min; rods at threshold {within} min");
        assert!(crossing < within);
    }

    #[test]
    fn closing_the_hatch_plunges_into_black_and_reveals_the_cabin_over_time() {
        // From the courtyard's dusk snow (research table 2.7: 210 cd/m^2) into a dim cabin.
        let outside = Meter::uniform(210.0, 210.0 * 2.35);
        let cabin = Meter::uniform(0.5, 0.5 * 1.4);
        let mut eye = Eye::adapted(outside, Viewing::Binocular);
        run(&mut eye, outside, 120.0, 1.0 / 60.0);
        let steady = aces(Eye::adapted(cabin, Viewing::Binocular).mapping(Viewing::Binocular)
            .aces_input(flat_radiance(0.5))[1]);
        let mut shown = Vec::new();
        for (seconds, dt) in [(0.05, 1.0 / 60.0), (1.0, 1.0 / 60.0), (5.0, 1.0 / 60.0),
            (30.0, 0.05), (120.0, 0.05), (600.0, 0.25)] {
            let elapsed: f64 = shown.iter().map(|(t, _): &(f64, f64)| *t).last().unwrap_or(0.0);
            run(&mut eye, cabin, seconds - elapsed, dt);
            shown.push((seconds,
                aces(eye.mapping(Viewing::Binocular).aces_input(flat_radiance(0.5))[1])));
        }
        eprintln!("cabin after closing the hatch (s, shown): {shown:?}; steady {steady}");
        assert!(shown[0].1 < 0.05 * steady, "the cabin starts black: {shown:?} vs {steady}");
        for pair in shown.windows(2) {
            assert!(pair[1].1 >= pair[0].1 * 0.999, "revealed monotonically: {shown:?}");
        }
        assert!(shown[1].1 < 0.5 * steady, "not revealed within a second: {shown:?}");
        assert!(shown.last().unwrap().1 > 0.9 * steady, "revealed after ten minutes: {shown:?}");
    }

    #[test]
    fn flash_recovery_follows_the_published_fit() {
        // The gunner's own 88 (research section 6.6): 6e3-3e4 td s; a moonlit target (0.05
        // cd/m^2) is seen again after 2-5 s, a dark one (1e-4) after about half a minute to a
        // minute.
        let recovered = |energy: f64, target: f64| {
            (1..600_000).map(|ms| ms as f64 / 1000.0)
                .find(|t| flash_afterimage_cd_m2(energy, *t) <= target).unwrap()
        };
        for energy in [6.0e3, 3.0e4] {
            let moonlit = recovered(energy, 0.05);
            let dark = recovered(energy, 1.0e-4);
            assert!((0.5..6.0).contains(&moonlit), "{energy}: moonlit {moonlit} s");
            assert!((20.0..70.0).contains(&dark), "{energy}: dark {dark} s");
        }
        // Inside the fit's measured range the model reproduces it: 9e5 td s, 20/60 target at
        // 0.06 mL (0.19 cd/m^2) recovers in 20 s.
        assert!((recovered(9.0e5, 0.19) - 20.0).abs() < 0.05);
        // A full-field muzzle flash seen dark-adapted: the eye is blinded, then recovers.
        // A muzzle flash filling a fifth of a moonlit view: the snow vanishes, then returns.
        let night = Meter::uniform(0.05, 0.12);
        let snow = |eye: &Eye| {
            aces(eye.mapping(Viewing::Binocular).aces_input(flat_radiance(0.05))[1])
        };
        let mut eye = Eye::adapted(night, Viewing::Binocular);
        let before = snow(&eye);
        let flash = Meter { photopic_cd_m2: 60.0, scotopic_cd_m2: 30.0, bright_coverage: 0.2,
            bright_cd_m2: 3_000.0, field_cd_m2:600.0,transient_source:true };
        run(&mut eye, flash, 0.065, 1.0 / 240.0);
        assert!(eye.flash_energy_td_s > 1_000.0, "{eye:?}");
        run(&mut eye, night, 0.5, 1.0 / 60.0);
        let blinded = snow(&eye);
        run(&mut eye, night, 29.5, 1.0 / 60.0);
        let recovered = snow(&eye);
        eprintln!("moonlit snow shown {before:.4} before, {blinded:.4} 0.5 s after, {recovered:.4} 30 s after");
        assert!(blinded < 0.3 * before, "{blinded} vs {before}");
        assert!(recovered > 0.8 * before, "{recovered} vs {before}");
    }

    #[test]
    fn local_rod_response_preserves_black_photopic_bypass_and_unsaturated_dark_samples() {
        for gain in [0.0, 0.1, 1.0, 7.860959529876709] {
            assert_eq!(rod_response(0.0, gain, 0.028473004698753357), 0.0);
        }
        for stimulus in [0.0, 1e-8, 1e-4, 1.0, 1000.0] {
            assert_eq!(rod_response(stimulus, 0.0, 0.028), 0.0);
        }
        let stimulus = 1e-8;
        let gain = 1.0;
        assert_eq!(rod_response(stimulus, gain, 0.028),
            gain * stimulus / (1.0 + stimulus * 0.028));
    }

    #[test]
    fn local_rod_response_bounds_the_recorded_cabin_without_changing_cone_color() {
        // GPU eye state from hotfix_cabin_eye_diagnostic_20261002_turret_photometry.json.
        let (gain, inverse_saturation) = (7.860959529876709, 0.028473004698753357);
        let plateau = DISPLAY_KEY / DISPLAY_ADAPTATION_CD_M2
            * 10f64.powf(cone_tvi_log(DISPLAY_ADAPTATION_CD_M2.log10()) + 0.395);
        assert!((plateau - 0.024846916762851935).abs() < 1e-14);
        assert!(gain / inverse_saturation > 276.0);
        assert!((rod_response(1000.0, gain, inverse_saturation) - plateau).abs() < 1e-12);

        let mut mapping = Eye::adapted(Meter::uniform(0.0000553732425, 0.000050396975),
            Viewing::Binocular).mapping(Viewing::Binocular);
        mapping.cone_exposure = 4156.2275390625;
        mapping.rod_exposure = gain;
        mapping.rod_inverse_saturation = inverse_saturation;
        // Fixed warm spectrum at the captured bright-region average. The cone channel must
        // remain dominant here while a dim neutral sample still receives its rod signal.
        let spectrum = [1.0, 0.30, 0.03];
        let luminance: f64 = (0..3).map(|i| spectrum[i] * PHOTOPIC_WEIGHTS[i]).sum();
        let radiance = spectrum.map(|v| v * 43.00748062133789 / luminance);
        let mapped = mapping.aces_input(radiance);
        assert!(mapped[0] / mapped[2] > 10.0, "bright warm patch lost chroma: {mapped:?}");
        assert!(mapped[0] / mapped[1] > 2.5, "bright warm patch lost chroma: {mapped:?}");
        assert!((mapped[0] - mapped[1] - mapping.cone_exposure
            * (radiance[0] - radiance[1])).abs() < 1e-12);
        let dark = flat_radiance(1e-6);
        assert!(mapping.aces_input(dark)[0] > dark[0] * mapping.cone_exposure);
    }

    #[test]
    fn local_rod_response_is_nondecreasing_including_rounded_tvi_knees() {
        for gain in [0.0, 0.01, 0.1, 1.0, 7.860959529876709, 100.0] {
            for inverse_saturation in [0.0, 0.01, 0.028473004698753357, 0.04] {
                let mut stimuli: Vec<f64> = (0..=20_000).map(|i|
                    10f64.powf(-10.0 + f64::from(i) * 17.0 / 20_000.0)).collect();
                for knee in [10f64.powf(-3.94), 10f64.powf(-1.44)] {
                    for i in -1000..=1000 {
                        let signal = knee * (1.0 + f64::from(i) * 1e-7);
                        stimuli.push(signal / (1.0 - signal * inverse_saturation));
                    }
                    for signal in [f64::from_bits(knee.to_bits() - 1), knee,
                        f64::from_bits(knee.to_bits() + 1)] {
                        stimuli.push(signal / (1.0 - signal * inverse_saturation));
                    }
                }
                stimuli.sort_by(f64::total_cmp);
                let mut previous = 0.0;
                for stimulus in stimuli {
                    let response = rod_response(stimulus, gain, inverse_saturation);
                    assert!(response + 1e-14 >= previous,
                        "gain={gain} saturation={inverse_saturation} stimulus={stimulus}: {previous} -> {response}");
                    previous = response;
                }
            }
        }
    }

    #[test]
    fn local_rod_shader_helper_is_shared_and_stays_before_display_encoding() {
        let generated = wgsl_constants();
        let module = wgpu::naga::front::wgsl::parse_str(&generated).unwrap();
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
        for shader in [include_str!("../shaders/crush.wgsl"),
            include_str!("../shaders/stylized_particles.wgsl"),
            include_str!("../shaders/pixel_particles.wgsl")] {
            assert!(shader.contains("let rods = obs_rod_response(scotopic,"));
            assert!(!shader.contains("let rods = cr.rod_exposure *"));
            assert!(!shader.contains("let rods = eye.rod_exposure *"));
        }
        let helper = generated.split("fn obs_rod_response").nth(1).unwrap();
        assert!(!helper.contains("display_encode"));
        assert!(!helper.contains("display_decode"));
    }

    #[test]
    fn steady_bright_aperture_never_retriggers_flash_afterimage() {
        // 95% dark compartment and 5% bright exterior. A peripheral aperture can remain
        // above the flash threshold, even after the arithmetic field drives adaptation.
        let port=Meter {photopic_cd_m2:24.04,scotopic_cd_m2:50.5,
            bright_coverage:0.05,bright_cd_m2:1000.0,field_cd_m2:50.095,transient_source:false};
        let mut eye=Eye::adapted(port,Viewing::Binocular);
        run(&mut eye,port,30.0,1.0/60.0);
        assert_eq!(eye.flash_energy_td_s,0.0,"a steady source has no flash onset");
        assert_eq!(eye.mapping(Viewing::Binocular).afterimage_cd_m2,0.0);

        let mut arriving=Eye::adapted(Meter::uniform(0.1,0.21),Viewing::Binocular);
        run(&mut arriving,port,1.0,1.0/240.0);
        assert_eq!(arriving.flash_energy_td_s,0.0,
            "looking toward static daylight must not create a weapons-flash afterimage");
        run(&mut arriving,port,10.0,1.0/60.0);
        assert_eq!(arriving.flash_energy_td_s,0.0,"static port must not keep charging flash energy");
        assert!(arriving.since_flash_s>10.0,"static port must not reset recovery");
    }

    #[test]
    fn brief_glances_do_not_complete_display_adaptation_and_are_frame_rate_independent() {
        let cabin = Meter::uniform(0.5, 0.8);
        let daylight = Meter::uniform(500.0, 800.0);
        let mut reference: Option<[f64; 2]> = None;
        for fps in [30.0, 60.0, 120.0] {
            // Integer frame counts avoid the older duration helper's possible extra frame
            // when an accumulated floating-point time lands just below its endpoint.
            let advance = |eye: &mut Eye, meter: Meter, seconds: f64| {
                for _ in 0..(seconds * fps).round() as usize {
                    eye.step(meter, Viewing::Binocular, 1.0 / fps);
                }
            };
            let mut eye = Eye::adapted(cabin, Viewing::Binocular);
            let start = eye.cone_neural_log;
            let span = daylight.photopic_cd_m2.log10() - start;
            advance(&mut eye, daylight, 0.1);
            let glance_fraction = (eye.cone_neural_log - start) / span;
            assert!((0.02..0.12).contains(&glance_fraction),
                "a 100 ms glance must start, not complete adaptation: {glance_fraction}");
            advance(&mut eye, daylight, 0.9);
            let held_fraction = (eye.cone_neural_log - start) / span;
            assert!((0.15..0.35).contains(&held_fraction),
                "holding a bright view must still adapt: {held_fraction}");
            let before_return = eye.cone_neural_log;
            advance(&mut eye, cabin, 0.1);
            assert!(eye.cone_neural_log < before_return);
            assert!(eye.cone_neural_log > start + 0.9 * (before_return - start),
                "returning to the cabin must not snap its gain in one glance");
            assert_eq!(eye.flash_energy_td_s, 0.0);
            let final_logs = [eye.cone_neural_log, eye.rod_neural_log];
            if let Some(previous) = reference {
                for (a, b) in final_logs.into_iter().zip(previous) {
                    // Pupil area changes during each integration step, so the coupled
                    // neural target is not constant. Bound 30–120 Hz convergence to
                    // 0.005 log10 units (under 1.2% luminance), not bit equality.
                    assert!((a - b).abs() < 0.005, "frame-dependent adaptation: {a} vs {b}");
                }
            }
            reference = Some(final_logs);
        }
    }

    #[test]
    fn a_transient_source_without_a_visible_radiance_onset_cannot_blind_the_observer() {
        let static_port = Meter { photopic_cd_m2: 24.04, scotopic_cd_m2: 50.5,
            bright_coverage: 0.05, bright_cd_m2: 1000.0, field_cd_m2: 50.095,
            transient_source: false };
        let mut eye = Eye::adapted(static_port, Viewing::Binocular);
        // A gun can fire behind armor while a bright exterior opening remains in view.
        // Merely admitting that source must not borrow the steady port's radiance as a flash.
        run(&mut eye, Meter { transient_source: true, ..static_port }, 0.2, 1.0 / 120.0);
        assert_eq!(eye.flash_energy_td_s, 0.0);
        assert_eq!(eye.mapping(Viewing::Binocular).afterimage_cd_m2, 0.0);
    }

    #[test]
    fn the_model_has_one_set_of_constants_for_every_preset() {
        let generated = wgsl_constants();
        assert!(generated.contains("const OBS_CONE_TAU_DOWN: f32 = 6e0;"), "{generated}");
        let source = include_str!("observer.rs");
        let body = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in ["GraphicsPreset", "Laptop", "graphics::", "shader_backend"] {
            assert!(!body.contains(forbidden), "the eye may not depend on {forbidden}");
        }
        let shader = include_str!("../shaders/exposure.wgsl");
        assert!(shader.contains("// @observer-constants"));
        for name in ["OBS_CONE_TAU_DOWN", "OBS_ROD_BLEACH_LOG", "OBS_DISPLAY_KEY", "OBS_PUPIL_SLOW_SHARE"] {
            assert!(shader.contains(name), "{name} unused by the GPU eye");
            assert!(!shader.contains(&format!("const {name}")), "{name} must come from Rust");
        }
    }
}
