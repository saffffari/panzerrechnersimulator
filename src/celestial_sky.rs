//! The sky of an act: a place, a date and a time of Universal Time that the ephemeris lights.
//!
//! The Sun and the Moon come from `ephemeris` for the act's site and the running clock; the
//! physical atmosphere (`physical_environment`) scatters their light and shows their discs, and
//! the night-sky layer adds airglow and faint starlight. Nothing here is a palette, a floor or a
//! keyed brightness: at night with no Moon and no fire the sky is as dark as the night sky.
//!
//! One directional source carries the direct beam and the sun-shadow cascades (the *key*): the
//! Sun while it stands above -1 deg, otherwise the Moon if it is up. The other body is scattered
//! by the air and the cloud as the *secondary* source. Named approximation (the only one): the
//! Moon is dropped while the Sun stands above -1 deg. Its whole contribution is then at most
//! `MOON_DROPPED_BOUND` of the Sun's horizontal illuminance (tested over the campaign window).
//! A body below `ATMOSPHERE_SHADOW_ALTITUDE_DEG` lights no part of the modelled 100 km
//! atmosphere seen from the ground, so skipping it there is exact.

use glam::DVec3;

use crate::ephemeris::{CelestialState, Site, UtcInstant, HALBE, ODERBRUCH, STALINGRAD};
use crate::renderer::physical_environment::AtmosphereParameters;

/// Altitude above which a body's beam can reach the camera past the horizon dip.
pub const DIRECT_BEAM_ALTITUDE_DEG: f64 = -1.0;
/// Below this altitude a body lights no air along any ray from an observer within 1 km of the
/// datum: the ray that skims the horizon leaves the 100 km atmosphere 1,128 km away, 10.1 deg
/// round the planet, where grazing light from a body more than 10.1 deg lower still misses.
pub const ATMOSPHERE_SHADOW_ALTITUDE_DEG: f64 = -21.5;
/// Largest ratio of the dropped Moon's horizontal illuminance to the Sun's while the Sun is the
/// key (Sun above -1 deg; USNO 171, same sky for both bodies).
pub const MOON_DROPPED_BOUND: f64 = 1.5e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    Sun,
    Moon,
}

/// A directional source as the atmosphere takes it: toward the body, its top-of-atmosphere
/// spectral irradiance in the three bands (W m^-2 nm^-1) and its disc radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionalSource {
    pub body: Body,
    pub direction: DVec3,
    pub irradiance: DVec3,
    pub angular_radius_rad: f64,
}

/// The two bodies of one instant, assigned to the key and secondary slots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialLighting {
    pub state: CelestialState,
    pub key: DirectionalSource,
    pub secondary: Option<DirectionalSource>,
}

impl CelestialLighting {
    /// `solar_toa_at_1au` is the extraterrestrial solar spectral irradiance at one astronomical
    /// unit (`AtmosphereParameters::default().solar_irradiance`).
    pub fn at(instant: UtcInstant, site: Site, solar_toa_at_1au: DVec3) -> Self {
        let state = CelestialState::at(instant, site);
        let au_ratio = crate::ephemeris::AU_KM / state.sun_distance_km;
        let sun = DirectionalSource {
            body: Body::Sun,
            direction: state.sun.engine_direction(),
            irradiance: solar_toa_at_1au * (au_ratio * au_ratio),
            angular_radius_rad: state.sun_angular_radius_rad(),
        };
        let moon = DirectionalSource {
            body: Body::Moon,
            direction: state.moon.engine_direction(),
            irradiance: state.moon_toa_spectral_irradiance(solar_toa_at_1au),
            angular_radius_rad: state.moon_angular_radius_rad(),
        };
        let sun_beam = state.sun.altitude_deg > DIRECT_BEAM_ALTITUDE_DEG;
        let moon_beam = state.moon.altitude_deg > DIRECT_BEAM_ALTITUDE_DEG;
        let lights_air = |altitude: f64| altitude > ATMOSPHERE_SHADOW_ALTITUDE_DEG;
        let (key, secondary) = if !sun_beam && moon_beam {
            (moon, lights_air(state.sun.altitude_deg).then_some(sun))
        } else {
            // The Sun keeps the key in daylight, and in twilight or darkness with no Moon up; a
            // Moon below the horizon lights only the far upper air, far under the airglow.
            (sun, None)
        };
        Self { state, key, secondary }
    }

    /// Install both bodies into an atmosphere; weather and medium stay as the caller set them.
    pub fn apply_to(&self, atmosphere: &mut AtmosphereParameters) {
        atmosphere.sun_direction = self.key.direction;
        atmosphere.solar_irradiance = self.key.irradiance;
        atmosphere.sun_angular_radius_rad = self.key.angular_radius_rad;
        match self.secondary {
            Some(source) => {
                atmosphere.secondary_direction = source.direction;
                atmosphere.secondary_irradiance = source.irradiance;
                atmosphere.secondary_angular_radius_rad = source.angular_radius_rad;
            }
            None => atmosphere.secondary_irradiance = DVec3::ZERO,
        }
    }

    /// 0 while the Sun is up, 1 once civil twilight has ended (Sun at -6 deg). It gates the
    /// legacy emissive-window material only; it never scales a light source.
    pub fn darkness(&self) -> f32 {
        ((-0.8333 - self.state.sun.altitude_deg) / (6.0 - 0.8333)).clamp(0.0, 1.0) as f32
    }
}

/// Named times of day. Each maps to an instant on the act's date; the light comes from the
/// ephemeris at that instant, never from the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

pub fn act_site(act_id: u8) -> Site {
    match act_id {
        2 => ODERBRUCH,
        3 => HALBE,
        _ => STALINGRAD,
    }
}

/// The act's representative date (Universal Time midnight) and its night instant.
fn act_calendar(act_id: u8) -> (UtcInstant, UtcInstant) {
    match act_id {
        // 16 April 1945: the Seelow offensive opened at 03:00 Moscow time (01:00 UT).
        2 => (UtcInstant::from_calendar(1945, 4, 16, 0, 0, 0.0),
            UtcInstant::from_calendar(1945, 4, 16, 1, 0, 0.0)),
        // 28 April 1945, the Halbe breakout nights.
        3 => (UtcInstant::from_calendar(1945, 4, 28, 0, 0, 0.0),
            UtcInstant::from_calendar(1945, 4, 28, 22, 0, 0.0)),
        // The Kessel at the solstice during Wintergewitter; the night is Christmas Eve 1942 at
        // 23:00 UT (CET midnight), the pocket-wide flare salute of docs/ENVSTORY.md, under a 96 %
        // Moon about 59 deg up in the south.
        _ => (UtcInstant::from_calendar(1942, 12, 21, 0, 0, 0.0),
            UtcInstant::from_calendar(1942, 12, 24, 23, 0, 0.0)),
    }
}

/// Sun altitude at which the Dawn and Dusk phases stand.
pub const TWILIGHT_PHASE_SUN_ALTITUDE_DEG: f64 = 3.0;

pub fn phase_instant(act_id: u8, phase: ClockPhase) -> UtcInstant {
    let site = act_site(act_id);
    let (midnight, night) = act_calendar(act_id);
    let altitude = |minutes: f64| {
        let instant = midnight.plus_seconds(minutes * 60.0);
        let sun = crate::ephemeris::sun_position(instant.jde());
        crate::ephemeris::horizontal_position(&sun, instant, site).altitude_deg
    };
    // Local apparent noon: the day's highest Sun, searched at one-minute steps.
    let noon = (0..24 * 60).map(|m| m as f64)
        .max_by(|a, b| altitude(*a).total_cmp(&altitude(*b)))
        .unwrap_or(720.0);
    let crossing = |from: f64, to: f64| {
        // Bisection on a monotonic half-day for the Dawn/Dusk altitude.
        let (mut lo, mut hi) = (from, to);
        let rising = altitude(hi) > altitude(lo);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            let above = altitude(mid) > TWILIGHT_PHASE_SUN_ALTITUDE_DEG;
            if above == rising { hi = mid } else { lo = mid }
        }
        0.5 * (lo + hi)
    };
    let minutes = match phase {
        ClockPhase::Day => noon,
        ClockPhase::Dawn => crossing(noon - 12.0 * 60.0, noon),
        ClockPhase::Dusk => crossing(noon + 12.0 * 60.0, noon),
        ClockPhase::Night => return night,
    };
    midnight.plus_seconds(minutes * 60.0)
}

/// A running act clock: advances with simulated time from an authored or named instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyClock {
    pub start: UtcInstant,
    pub site: Site,
    pub elapsed_s: f64,
}

impl SkyClock {
    pub fn new(start: UtcInstant, site: Site) -> Self {
        Self { start, site, elapsed_s: 0.0 }
    }

    pub fn for_act(act_id: u8, phase: ClockPhase) -> Self {
        Self::new(phase_instant(act_id, phase), act_site(act_id))
    }

    pub fn now(&self) -> UtcInstant {
        self.start.plus_seconds(self.elapsed_s)
    }

    pub fn advance(&mut self, dt_s: f64) {
        if dt_s.is_finite() && dt_s > 0.0 {
            self.elapsed_s += dt_s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ephemeris::{
        photopic_illuminance_lux, usno_moon_illuminance_lux, usno_sun_illuminance_lux,
        horizontal_position, lunar_phase, moon_position, sun_position, SKY_CLEAR,
    };
    use crate::renderer::physical_environment::{
        integrate_environment, solar_transmittance_at, IntegrationQuality,
    };

    /// Horizontal illuminance at the local ground from the sky hemisphere (Gauss-Legendre in the
    /// zenith cosine, uniform in azimuth) plus the key body's direct beam, lux.
    fn horizontal_illuminance_lux(atmosphere: &AtmosphereParameters) -> f64 {
        const MU: [f64; 8] = [0.019_855_071_751_231_9, 0.101_666_761_293_186_6,
            0.237_233_795_041_835_5, 0.408_282_678_752_175_1, 0.591_717_321_247_824_9,
            0.762_766_204_958_164_5, 0.898_333_238_706_813_4, 0.980_144_928_248_768_1];
        const W: [f64; 8] = [0.050_614_268_145_188_1, 0.111_190_517_226_687_2,
            0.156_853_322_938_943_6, 0.181_341_891_689_181_0, 0.181_341_891_689_181_0,
            0.156_853_322_938_943_6, 0.111_190_517_226_687_2, 0.050_614_268_145_188_1];
        let origin = DVec3::new(0.0, 2.0, 0.0);
        let quality = IntegrationQuality::REFERENCE;
        let azimuths = 24;
        let mut sky = DVec3::ZERO;
        for (mu, w) in MU.iter().zip(W) {
            for k in 0..azimuths {
                let phi = (k as f64 + 0.5) / azimuths as f64 * std::f64::consts::TAU;
                let s = (1.0 - mu * mu).sqrt();
                let direction = DVec3::new(s * phi.cos(), *mu, s * phi.sin());
                let transport = integrate_environment(atmosphere, origin, direction,
                    f64::INFINITY, quality);
                sky += transport.inscattered_radiance * (mu * w
                    * std::f64::consts::TAU / azimuths as f64);
            }
        }
        let key = atmosphere.sun_direction.normalize();
        let beam = atmosphere.solar_irradiance
            * solar_transmittance_at(atmosphere, origin, key, quality.source_steps)
            * key.y.max(0.0);
        photopic_illuminance_lux(sky + beam)
    }

    fn sky_at(when: &str, overcast: bool) -> (AtmosphereParameters, CelestialLighting) {
        let mut atmosphere = AtmosphereParameters::default();
        let lighting = CelestialLighting::at(UtcInstant::parse_iso(when).unwrap(), STALINGRAD,
            atmosphere.solar_irradiance);
        lighting.apply_to(&mut atmosphere);
        atmosphere.cloud.coverage = if overcast { 0.99 } else { 0.0 };
        (atmosphere, lighting)
    }

    #[test]
    fn the_key_is_the_sun_by_day_and_the_moon_at_a_moonlit_night() {
        let (_, day) = sky_at("1942-12-21T12:00:00Z", false);
        assert_eq!(day.key.body, Body::Sun);
        assert!(day.secondary.is_none());
        let (_, night) = sky_at("1942-12-24T23:00:00Z", false);
        assert_eq!(night.key.body, Body::Moon);
        assert!(night.secondary.is_none(), "the Sun is far below the lit air at 23 UT");
        assert!((night.state.moon.altitude_deg - 59.0).abs() < 2.0, "{:?}", night.state.moon);
        // Moonrise over a twilight: the Moon keys the beam while the Sun still lights the air.
        let (_, dusk) = sky_at("1942-12-22T14:30:00Z", false);
        assert_eq!(dusk.key.body, Body::Moon);
        assert_eq!(dusk.secondary.map(|s| s.body), Some(Body::Sun));
        assert_eq!(dusk.darkness(), 1.0);
        assert_eq!(day.darkness(), 0.0);
    }

    #[test]
    fn named_phases_are_instants_of_the_ephemeris() {
        let day = CelestialState::at(phase_instant(1, ClockPhase::Day), STALINGRAD);
        assert!((day.sun.altitude_deg - 17.85).abs() < 0.1, "{:?}", day.sun);
        assert!((day.sun.azimuth_deg - 180.0).abs() < 1.0, "{:?}", day.sun);
        for phase in [ClockPhase::Dawn, ClockPhase::Dusk] {
            let state = CelestialState::at(phase_instant(1, phase), STALINGRAD);
            assert!((state.sun.altitude_deg - TWILIGHT_PHASE_SUN_ALTITUDE_DEG).abs() < 0.01);
            let east = state.sun.azimuth_deg < 180.0;
            assert_eq!(east, phase == ClockPhase::Dawn, "{phase:?} {:?}", state.sun);
        }
        let night = CelestialState::at(phase_instant(1, ClockPhase::Night), STALINGRAD);
        assert!(night.sun.altitude_deg < -50.0, "{:?}", night.sun);
        // Oderbruch on 16 April 1945 (declination +10 deg at 52.55 N) and Halbe on 28 April
        // (+14 deg at 52.11 N).
        for (act, expected) in [(2, 47.5), (3, 51.9)] {
            let noon = CelestialState::at(phase_instant(act, ClockPhase::Day), act_site(act));
            assert!((noon.sun.altitude_deg - expected).abs() < 0.6, "act {act}: {:?}", noon.sun);
        }
        let mut clock = SkyClock::for_act(1, ClockPhase::Day);
        clock.advance(3600.0);
        clock.advance(f64::NAN);
        assert!((clock.now().jd - phase_instant(1, ClockPhase::Day).jd - 1.0 / 24.0).abs() < 1e-9);
    }

    #[test]
    fn dropping_the_moon_while_the_sun_keys_is_within_its_bound() {
        // Every 10 minutes from 19 Nov 1942 to 5 Feb 1943.
        let start = UtcInstant::parse_iso("1942-11-19T00:00:00Z").unwrap();
        let mut worst: f64 = 0.0;
        for step in 0..(78 * 24 * 6) {
            let instant = start.plus_seconds(step as f64 * 600.0);
            let jde = instant.jde();
            let (sun, moon) = (sun_position(jde), moon_position(jde));
            let sun_h = horizontal_position(&sun, instant, STALINGRAD);
            if sun_h.altitude_deg <= DIRECT_BEAM_ALTITUDE_DEG { continue; }
            let moon_h = horizontal_position(&moon, instant, STALINGRAD);
            let moon_lux = usno_moon_illuminance_lux(moon_h.altitude_deg,
                lunar_phase(&sun, &moon).elongation_rad, SKY_CLEAR);
            worst = worst.max(moon_lux / usno_sun_illuminance_lux(sun_h.altitude_deg, SKY_CLEAR));
        }
        assert!(worst <= MOON_DROPPED_BOUND, "dropped Moon {worst:.2e} of the Sun");
        assert!(worst > 1e-5, "the bound is tight enough to mean something: {worst:.2e}");
    }

    #[test]
    fn courtyard_afternoon_illuminance_matches_the_measured_curve() {
        // 15:00 local apparent time on 21 Dec 1942, 7.41 deg (USNO 171 clear: about 7,100 lx).
        let (clear, _) = sky_at("1942-12-21T12:00:00Z", false);
        let lux = horizontal_illuminance_lux(&clear);
        let usno = usno_sun_illuminance_lux(7.41, SKY_CLEAR);
        // Brown's "clear" class admits up to 7/10 cloud and typical haze; this sky is cloudless
        // with the light default aerosol and the December (perihelion) Sun, so it reads higher.
        eprintln!("clear 7.41 deg: {lux:.0} lx against USNO 171 clear {usno:.0} lx");
        assert!(lux > 0.9 * usno && lux < 1.6 * usno, "clear 7.41 deg: {lux:.0} lx vs USNO {usno:.0}");
        // The Act I storm deck (tau about 49): Brown's dark-stratus class is USNO / 10.
        let (overcast, _) = sky_at("1942-12-21T12:00:00Z", true);
        let deck = horizontal_illuminance_lux(&overcast);
        eprintln!("overcast 7.41 deg: {deck:.0} lx (USNO dark stratus {:.0})", usno / 10.0);
        assert!(deck > usno / 30.0 && deck < usno / 3.0, "overcast 7.41 deg: {deck:.0} lx");
    }

    #[test]
    fn moonlight_and_the_moonless_night_are_as_dark_as_measured() {
        // Christmas Eve 1942, 23:00 UT: 96 % Moon about 59 deg up (USNO clear 0.11 lx).
        let (moonlit, lighting) = sky_at("1942-12-24T23:00:00Z", false);
        let lux = horizontal_illuminance_lux(&moonlit);
        let jde = lighting.state.instant.jde();
        let geocentric = horizontal_position(&moon_position(jde), lighting.state.instant, STALINGRAD);
        let usno = usno_moon_illuminance_lux(geocentric.altitude_deg,
            lighting.state.moon_phase.elongation_rad, SKY_CLEAR);
        eprintln!("moonlit clear: {lux:.4} lx against USNO {usno:.4}");
        assert!((lux / usno - 1.0).abs() < 0.35, "moonlit {lux:.4} lx vs USNO {usno:.4}");
        assert!((0.05..0.3).contains(&lux));
        // New Moon, 6 Jan 1943, 20:00 UT: airglow, zodiacal light and faint stars only.
        let (moonless, dark) = sky_at("1943-01-06T20:00:00Z", false);
        assert!(dark.state.moon.altitude_deg < -10.0);
        let clear = horizontal_illuminance_lux(&moonless);
        eprintln!("moonless clear: {clear:.2e} lx");
        assert!((4.0e-4..1.0e-3).contains(&clear), "moonless clear {clear:.2e} lx");
        let (overcast, _) = sky_at("1943-01-06T20:00:00Z", true);
        let deck = horizontal_illuminance_lux(&overcast);
        eprintln!("moonless overcast: {deck:.2e} lx");
        assert!((3.0e-5..2.0e-4).contains(&deck), "moonless overcast {deck:.2e} lx");
    }

    #[test]
    fn twilight_single_scattering_is_named_not_hidden() {
        // Civil dusk (-6 deg) under a clear sky measures about 3 lx (USNO 171). This atmosphere
        // scatters sunlight once; multiple scattering, which dominates below about -6 deg, is the
        // ledger's open item. The model must still be dark, finite and far above the night.
        let base = UtcInstant::parse_iso("1942-12-21T13:00:00Z").unwrap();
        let minutes = (0..240).find(|m| CelestialState::at(base.plus_seconds(*m as f64 * 60.0),
            STALINGRAD).sun.altitude_deg < -6.0).unwrap();
        let instant = base.plus_seconds(minutes as f64 * 60.0);
        let mut atmosphere = AtmosphereParameters::default();
        CelestialLighting::at(instant, STALINGRAD, atmosphere.solar_irradiance)
            .apply_to(&mut atmosphere);
        atmosphere.cloud.coverage = 0.0;
        let lux = horizontal_illuminance_lux(&atmosphere);
        eprintln!("civil dusk, clear, single scattering: {lux:.3} lx (USNO 2.99)");
        assert!(lux.is_finite() && lux > 0.5 && lux < 8.0, "{lux}");
    }
}
