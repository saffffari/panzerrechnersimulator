//! The Sun and the Moon for a date, a time and a place, and the natural illuminance they give.
//!
//! Positions follow J. Meeus, *Astronomical Algorithms* (2nd ed., 1998):
//! - ch. 7: Julian day (Gregorian calendar);
//! - ch. 12: mean sidereal time (eq. 12.4), made apparent with the nutation in right ascension;
//! - ch. 13 and ch. 40: horizontal coordinates, the Moon's topocentric parallax;
//! - ch. 22: nutation, its four largest terms (about 0.5" in longitude, 0.1" in obliquity);
//! - ch. 25: the Sun, low accuracy (0.01 deg), apparent longitude;
//! - ch. 47: the Moon, the full 60 + 60 periodic terms of Tables 47.A and 47.B. The coefficients
//!   were transcribed from `soniakeys/meeus` v3 `moonposition.go` (MIT licence, (c) Sonia Keys);
//! - ch. 48: phase angle and illuminated fraction.
//!
//! Delta T is the Espenak-Meeus polynomial (26 s in 1943). Validation lives in the tests: Meeus's
//! worked examples 25.a and 47.a, the NASA/Espenak phase catalogue for November 1942 - February
//! 1943 (within 2 min), and 15:00 local apparent time on 21 December 1942 at Stalingrad (7.41 deg,
//! compass 220.9 deg), all from `LIGHT_NIGHT_ADAPTATION_20260929.md` sections 3.1-3.5.
//!
//! Illuminance: USNO Circular 171 (Janiczek & DeYoung 1987, DTIC ADA182110), the fit to Brown's
//! 1952 curve of more than 12,000 measurements. It validates the renderer's own atmosphere and
//! supplies the Moon's top-of-atmosphere photometry (its phase law). It is never displayed as a
//! sky: the renderer's sky is the physical atmosphere lit by these two bodies.
//!
//! Engine axes: +X east, +Y up, +Z south (`src/terrain.rs`).

use glam::DVec3;

const DEG: f64 = std::f64::consts::PI / 180.0;
pub const J2000: f64 = 2_451_545.0;
/// IAU astronomical unit, km.
pub const AU_KM: f64 = 149_597_870.7;
/// Mean Earth-Moon distance used by the lunar photometry, km.
pub const MOON_MEAN_DISTANCE_KM: f64 = 384_400.0;
pub const MOON_RADIUS_KM: f64 = 1_737.4;
pub const EARTH_EQUATORIAL_RADIUS_KM: f64 = 6_378.14;

/// A place on the Earth. Longitude is positive east.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub latitude_deg: f64,
    pub longitude_deg_east: f64,
    pub height_m: f64,
}

/// The Stalingrad DEM's map centre (`docs/STALINGRAD_CAMPAIGN.md`); the city stands 100-150 m up.
pub const STALINGRAD: Site = Site { latitude_deg: 48.708, longitude_deg_east: 44.513, height_m: 120.0 };
/// The Oderbruch below the Seelow Heights (Act II).
pub const ODERBRUCH: Site = Site { latitude_deg: 52.55, longitude_deg_east: 14.45, height_m: 10.0 };
/// Halbe, south-east of Berlin (Act III).
pub const HALBE: Site = Site { latitude_deg: 52.11, longitude_deg_east: 13.70, height_m: 50.0 };

/// An instant of Universal Time, as a Julian Day number.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct UtcInstant {
    pub jd: f64,
}

/// Julian Day of a Gregorian calendar date (Meeus 7.1); `day` may carry a fraction.
pub fn julian_day(year: i32, month: u32, day: f64) -> f64 {
    let (mut y, mut m) = (year as f64, month as f64);
    if month <= 2 {
        y -= 1.0;
        m += 12.0;
    }
    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + day + b - 1524.5
}

impl UtcInstant {
    pub fn from_calendar(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: f64) -> Self {
        let fraction = (hour as f64 + minute as f64 / 60.0 + second / 3600.0) / 24.0;
        Self { jd: julian_day(year, month, day as f64 + fraction) }
    }

    /// Parse `YYYY-MM-DDTHH:MM[:SS[.sss]]Z` (UTC; the trailing `Z` is required so a local clock
    /// can never be mistaken for Universal Time).
    pub fn parse_iso(text: &str) -> anyhow::Result<Self> {
        let text = text.trim();
        let body = text
            .strip_suffix('Z')
            .ok_or_else(|| anyhow::anyhow!("{text:?}: Universal Time must end in Z"))?;
        let (date, time) = body
            .split_once('T')
            .ok_or_else(|| anyhow::anyhow!("{text:?}: expected YYYY-MM-DDTHH:MM[:SS]Z"))?;
        let mut d = date.split('-');
        let year: i32 = d.next().unwrap_or("").parse()?;
        let month: u32 = d.next().unwrap_or("").parse()?;
        let day: u32 = d.next().unwrap_or("").parse()?;
        anyhow::ensure!(d.next().is_none(), "{text:?}: bad date");
        let mut t = time.split(':');
        let hour: u32 = t.next().unwrap_or("").parse()?;
        let minute: u32 = t.next().unwrap_or("").parse()?;
        let second: f64 = t.next().map(str::parse).transpose()?.unwrap_or(0.0);
        anyhow::ensure!(t.next().is_none(), "{text:?}: bad time");
        anyhow::ensure!((1..=12).contains(&month) && (1..=31).contains(&day), "{text:?}: bad date");
        anyhow::ensure!(hour < 24 && minute < 60 && (0.0..61.0).contains(&second), "{text:?}: bad time");
        Ok(Self::from_calendar(year, month, day, hour, minute, second))
    }

    /// Calendar date and time (Meeus ch. 7, inverse), rounded to the nearest second.
    pub fn calendar(self) -> (i32, u32, u32, u32, u32, u32) {
        let jd_seconds = ((self.jd + 0.5) * 86_400.0).round();
        let whole_days = (jd_seconds / 86_400.0).floor();
        let seconds_of_day = (jd_seconds - whole_days * 86_400.0) as u32;
        let z = whole_days;
        let alpha = ((z - 1_867_216.25) / 36_524.25).floor();
        let a = z + 1.0 + alpha - (alpha / 4.0).floor();
        let b = a + 1524.0;
        let c = ((b - 122.1) / 365.25).floor();
        let d = (365.25 * c).floor();
        let e = ((b - d) / 30.6001).floor();
        let day = (b - d - (30.6001 * e).floor()) as u32;
        let month = if e < 14.0 { e - 1.0 } else { e - 13.0 } as u32;
        let year = if month > 2 { c - 4716.0 } else { c - 4715.0 } as i32;
        (year, month, day, seconds_of_day / 3600, seconds_of_day / 60 % 60, seconds_of_day % 60)
    }

    pub fn to_iso(self) -> String {
        let (y, mo, d, h, mi, s) = self.calendar();
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }

    pub fn plus_seconds(self, seconds: f64) -> Self {
        Self { jd: self.jd + seconds / 86_400.0 }
    }

    /// Terrestrial (dynamical) time for the ephemeris: JDE = JD + Delta T.
    pub fn jde(self) -> f64 {
        self.jd + delta_t_seconds(decimal_year(self.jd)) / 86_400.0
    }
}

fn decimal_year(jd: f64) -> f64 {
    2000.0 + (jd - J2000) / 365.25
}

/// Delta T = TT - UT in seconds, Espenak & Meeus polynomials (NASA eclipse web site, 2006).
pub fn delta_t_seconds(year: f64) -> f64 {
    if year < 1941.0 {
        let u = year - 1920.0;
        21.20 + 0.84493 * u - 0.076100 * u * u + 0.0020936 * u * u * u
    } else if year < 1961.0 {
        let u = year - 1950.0;
        29.07 + 0.407 * u - u * u / 233.0 + u * u * u / 2547.0
    } else if year < 1986.0 {
        let u = year - 1975.0;
        45.45 + 1.067 * u - u * u / 260.0 - u * u * u / 718.0
    } else if year < 2005.0 {
        let u = year - 2000.0;
        63.86 + 0.3345 * u - 0.060374 * u * u + 0.0017275 * u.powi(3) + 0.000651814 * u.powi(4)
            + 0.00002373599 * u.powi(5)
    } else {
        let u = year - 2000.0;
        62.92 + 0.32217 * u + 0.005589 * u * u
    }
}

fn centuries(jde: f64) -> f64 {
    (jde - J2000) / 36_525.0
}

fn horner(x: f64, coefficients: &[f64]) -> f64 {
    coefficients.iter().rev().fold(0.0, |acc, c| acc * x + c)
}

fn wrap_degrees(angle: f64) -> f64 {
    angle.rem_euclid(360.0)
}

/// Nutation in longitude and obliquity and the mean obliquity, degrees (Meeus 22.2 and the
/// four-term series quoted there, accurate to 0.5" and 0.1").
pub fn nutation(jde: f64) -> (f64, f64, f64) {
    let t = centuries(jde);
    let omega = (125.04452 - 1934.136261 * t + 0.0020708 * t * t + t * t * t / 450_000.0) * DEG;
    let l_sun = (280.4665 + 36_000.7698 * t) * DEG;
    let l_moon = (218.3165 + 481_267.8813 * t) * DEG;
    let delta_psi_arcsec = -17.20 * omega.sin() - 1.32 * (2.0 * l_sun).sin()
        - 0.23 * (2.0 * l_moon).sin() + 0.21 * (2.0 * omega).sin();
    let delta_eps_arcsec = 9.20 * omega.cos() + 0.57 * (2.0 * l_sun).cos()
        + 0.10 * (2.0 * l_moon).cos() - 0.09 * (2.0 * omega).cos();
    let eps0_arcsec = 84_381.448 - 46.8150 * t - 0.00059 * t * t + 0.001_813 * t * t * t;
    (delta_psi_arcsec / 3600.0, delta_eps_arcsec / 3600.0, eps0_arcsec / 3600.0)
}

/// Geocentric apparent position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EquatorialPosition {
    /// Apparent ecliptic longitude and latitude, degrees.
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    /// Apparent right ascension and declination, degrees.
    pub right_ascension_deg: f64,
    pub declination_deg: f64,
    pub distance_km: f64,
}

fn ecliptic_to_equatorial(longitude_deg: f64, latitude_deg: f64, obliquity_deg: f64) -> (f64, f64) {
    let (l, b, e) = (longitude_deg * DEG, latitude_deg * DEG, obliquity_deg * DEG);
    let ra = (l.sin() * e.cos() - b.tan() * e.sin()).atan2(l.cos());
    let dec = (b.sin() * e.cos() + b.cos() * e.sin() * l.sin()).asin();
    (wrap_degrees(ra / DEG), dec / DEG)
}

/// The Sun, Meeus ch. 25 (low accuracy) with the apparent-longitude correction (25.8).
pub fn sun_position(jde: f64) -> EquatorialPosition {
    let t = centuries(jde);
    let l0 = horner(t, &[280.46646, 36_000.76983, 0.000_303_2]);
    let m = horner(t, &[357.52911, 35_999.05029, -0.000_153_7]) * DEG;
    let e = horner(t, &[0.016_708_634, -0.000_042_037, -0.000_000_126_7]);
    let c = horner(t, &[1.914_602, -0.004_817, -0.000_014]) * m.sin()
        + (0.019_993 - 0.000_101 * t) * (2.0 * m).sin()
        + 0.000_289 * (3.0 * m).sin();
    let true_longitude = l0 + c;
    let true_anomaly = m + c * DEG;
    let radius_au = 1.000_001_018 * (1.0 - e * e) / (1.0 + e * true_anomaly.cos());
    let omega = (125.04 - 1934.136 * t) * DEG;
    let apparent_longitude = wrap_degrees(true_longitude - 0.005_69 - 0.004_78 * omega.sin());
    let (_, _, eps0) = nutation(jde);
    let obliquity = eps0 + 0.002_56 * omega.cos();
    let (ra, dec) = ecliptic_to_equatorial(apparent_longitude, 0.0, obliquity);
    EquatorialPosition {
        longitude_deg: apparent_longitude,
        latitude_deg: 0.0,
        right_ascension_deg: ra,
        declination_deg: dec,
        distance_km: radius_au * AU_KM,
    }
}

// Meeus Table 47.A: D, M, M', F multipliers; longitude (1e-6 deg) and distance (1e-3 km) terms.
const MOON_LR: [(i8, i8, i8, i8, f64, f64); 60] = [
    (0, 0, 1, 0, 6_288_774.0, -20_905_355.0),
    (2, 0, -1, 0, 1_274_027.0, -3_699_111.0),
    (2, 0, 0, 0, 658_314.0, -2_955_968.0),
    (0, 0, 2, 0, 213_618.0, -569_925.0),
    (0, 1, 0, 0, -185_116.0, 48_888.0),
    (0, 0, 0, 2, -114_332.0, -3_149.0),
    (2, 0, -2, 0, 58_793.0, 246_158.0),
    (2, -1, -1, 0, 57_066.0, -152_138.0),
    (2, 0, 1, 0, 53_322.0, -170_733.0),
    (2, -1, 0, 0, 45_758.0, -204_586.0),
    (0, 1, -1, 0, -40_923.0, -129_620.0),
    (1, 0, 0, 0, -34_720.0, 108_743.0),
    (0, 1, 1, 0, -30_383.0, 104_755.0),
    (2, 0, 0, -2, 15_327.0, 10_321.0),
    (0, 0, 1, 2, -12_528.0, 0.0),
    (0, 0, 1, -2, 10_980.0, 79_661.0),
    (4, 0, -1, 0, 10_675.0, -34_782.0),
    (0, 0, 3, 0, 10_034.0, -23_210.0),
    (4, 0, -2, 0, 8_548.0, -21_636.0),
    (2, 1, -1, 0, -7_888.0, 24_208.0),
    (2, 1, 0, 0, -6_766.0, 30_824.0),
    (1, 0, -1, 0, -5_163.0, -8_379.0),
    (1, 1, 0, 0, 4_987.0, -16_675.0),
    (2, -1, 1, 0, 4_036.0, -12_831.0),
    (2, 0, 2, 0, 3_994.0, -10_445.0),
    (4, 0, 0, 0, 3_861.0, -11_650.0),
    (2, 0, -3, 0, 3_665.0, 14_403.0),
    (0, 1, -2, 0, -2_689.0, -7_003.0),
    (2, 0, -1, 2, -2_602.0, 0.0),
    (2, -1, -2, 0, 2_390.0, 10_056.0),
    (1, 0, 1, 0, -2_348.0, 6_322.0),
    (2, -2, 0, 0, 2_236.0, -9_884.0),
    (0, 1, 2, 0, -2_120.0, 5_751.0),
    (0, 2, 0, 0, -2_069.0, 0.0),
    (2, -2, -1, 0, 2_048.0, -4_950.0),
    (2, 0, 1, -2, -1_773.0, 4_130.0),
    (2, 0, 0, 2, -1_595.0, 0.0),
    (4, -1, -1, 0, 1_215.0, -3_958.0),
    (0, 0, 2, 2, -1_110.0, 0.0),
    (3, 0, -1, 0, -892.0, 3_258.0),
    (2, 1, 1, 0, -810.0, 2_616.0),
    (4, -1, -2, 0, 759.0, -1_897.0),
    (0, 2, -1, 0, -713.0, -2_117.0),
    (2, 2, -1, 0, -700.0, 2_354.0),
    (2, 1, -2, 0, 691.0, 0.0),
    (2, -1, 0, -2, 596.0, 0.0),
    (4, 0, 1, 0, 549.0, -1_423.0),
    (0, 0, 4, 0, 537.0, -1_117.0),
    (4, -1, 0, 0, 520.0, -1_571.0),
    (1, 0, -2, 0, -487.0, -1_739.0),
    (2, 1, 0, -2, -399.0, 0.0),
    (0, 0, 2, -2, -381.0, -4_421.0),
    (1, 1, 1, 0, 351.0, 0.0),
    (3, 0, -2, 0, -340.0, 0.0),
    (4, 0, -3, 0, 330.0, 0.0),
    (2, -1, 2, 0, 327.0, 0.0),
    (0, 2, 1, 0, -323.0, 1_165.0),
    (1, 1, -1, 0, 299.0, 0.0),
    (2, 0, 3, 0, 294.0, 0.0),
    (2, 0, -1, -2, 0.0, 8_752.0),
];

// Meeus Table 47.B: D, M, M', F multipliers; latitude terms (1e-6 deg).
const MOON_B: [(i8, i8, i8, i8, f64); 60] = [
    (0, 0, 0, 1, 5_128_122.0),
    (0, 0, 1, 1, 280_602.0),
    (0, 0, 1, -1, 277_693.0),
    (2, 0, 0, -1, 173_237.0),
    (2, 0, -1, 1, 55_413.0),
    (2, 0, -1, -1, 46_271.0),
    (2, 0, 0, 1, 32_573.0),
    (0, 0, 2, 1, 17_198.0),
    (2, 0, 1, -1, 9_266.0),
    (0, 0, 2, -1, 8_822.0),
    (2, -1, 0, -1, 8_216.0),
    (2, 0, -2, -1, 4_324.0),
    (2, 0, 1, 1, 4_200.0),
    (2, 1, 0, -1, -3_359.0),
    (2, -1, -1, 1, 2_463.0),
    (2, -1, 0, 1, 2_211.0),
    (2, -1, -1, -1, 2_065.0),
    (0, 1, -1, -1, -1_870.0),
    (4, 0, -1, -1, 1_828.0),
    (0, 1, 0, 1, -1_794.0),
    (0, 0, 0, 3, -1_749.0),
    (0, 1, -1, 1, -1_565.0),
    (1, 0, 0, 1, -1_491.0),
    (0, 1, 1, 1, -1_475.0),
    (0, 1, 1, -1, -1_410.0),
    (0, 1, 0, -1, -1_344.0),
    (1, 0, 0, -1, -1_335.0),
    (0, 0, 3, 1, 1_107.0),
    (4, 0, 0, -1, 1_021.0),
    (4, 0, -1, 1, 833.0),
    (0, 0, 1, -3, 777.0),
    (4, 0, -2, 1, 671.0),
    (2, 0, 0, -3, 607.0),
    (2, 0, 2, -1, 596.0),
    (2, -1, 1, -1, 491.0),
    (2, 0, -2, 1, -451.0),
    (0, 0, 3, -1, 439.0),
    (2, 0, 2, 1, 422.0),
    (2, 0, -3, -1, 421.0),
    (2, 1, -1, 1, -366.0),
    (2, 1, 0, 1, -351.0),
    (4, 0, 0, 1, 331.0),
    (2, -1, 1, 1, 315.0),
    (2, -2, 0, -1, 302.0),
    (0, 0, 1, 3, -283.0),
    (2, 1, 1, -1, -229.0),
    (1, 1, 0, -1, 223.0),
    (1, 1, 0, 1, 223.0),
    (0, 1, -2, -1, -220.0),
    (2, 1, -1, -1, -220.0),
    (1, 0, 1, 1, -185.0),
    (2, -1, -2, -1, 181.0),
    (0, 1, 2, 1, -177.0),
    (4, 0, -2, -1, 176.0),
    (4, -1, -1, -1, 166.0),
    (1, 0, 1, -1, -164.0),
    (4, 0, 1, -1, 132.0),
    (1, 0, -1, -1, -119.0),
    (4, -1, 0, -1, 115.0),
    (2, -2, 0, 1, 107.0),
];

/// Geocentric mean-equinox ecliptic position of the Moon (Meeus 47): longitude and latitude in
/// degrees (without nutation), distance in km.
pub fn moon_geometric_position(jde: f64) -> (f64, f64, f64) {
    let t = centuries(jde);
    let l_prime = horner(t, &[218.316_447_7, 481_267.881_234_21, -0.001_578_6, 1.0 / 538_841.0,
        -1.0 / 65_194_000.0]) * DEG;
    let d = horner(t, &[297.850_192_1, 445_267.111_403_4, -0.001_881_9, 1.0 / 545_868.0,
        -1.0 / 113_065_000.0]) * DEG;
    let m = horner(t, &[357.529_109_2, 35_999.050_290_9, -0.000_153_5, 1.0 / 24_490_000.0]) * DEG;
    let m_prime = horner(t, &[134.963_396_4, 477_198.867_505_5, 0.008_741_4, 1.0 / 69_699.0,
        -1.0 / 14_712_000.0]) * DEG;
    let f = horner(t, &[93.272_095, 483_202.017_523_3, -0.003_653_9, -1.0 / 3_526_000.0,
        1.0 / 863_310_000.0]) * DEG;
    let a1 = (119.75 + 131.849 * t) * DEG;
    let a2 = (53.09 + 479_264.29 * t) * DEG;
    let a3 = (313.45 + 481_266.484 * t) * DEG;
    let e = horner(t, &[1.0, -0.002_516, -0.000_007_4]);
    let eccentricity = |multiplier: i8| match multiplier.abs() {
        0 => 1.0,
        1 => e,
        _ => e * e,
    };
    let mut sum_l = 3958.0 * a1.sin() + 1962.0 * (l_prime - f).sin() + 318.0 * a2.sin();
    let mut sum_r = 0.0;
    let mut sum_b = -2235.0 * l_prime.sin() + 382.0 * a3.sin() + 175.0 * (a1 - f).sin()
        + 175.0 * (a1 + f).sin() + 127.0 * (l_prime - m_prime).sin()
        - 115.0 * (l_prime + m_prime).sin();
    for (cd, cm, cmp, cf, l, r) in MOON_LR {
        let argument = d * cd as f64 + m * cm as f64 + m_prime * cmp as f64 + f * cf as f64;
        let factor = eccentricity(cm);
        sum_l += l * argument.sin() * factor;
        sum_r += r * argument.cos() * factor;
    }
    for (cd, cm, cmp, cf, b) in MOON_B {
        let argument = d * cd as f64 + m * cm as f64 + m_prime * cmp as f64 + f * cf as f64;
        sum_b += b * argument.sin() * eccentricity(cm);
    }
    (
        wrap_degrees(l_prime / DEG + sum_l * 1e-6),
        sum_b * 1e-6,
        385_000.56 + sum_r * 1e-3,
    )
}

/// The Moon's apparent geocentric position (Meeus 47 with nutation).
pub fn moon_position(jde: f64) -> EquatorialPosition {
    let (longitude, latitude, distance) = moon_geometric_position(jde);
    let (delta_psi, delta_eps, eps0) = nutation(jde);
    let apparent_longitude = wrap_degrees(longitude + delta_psi);
    let (ra, dec) = ecliptic_to_equatorial(apparent_longitude, latitude, eps0 + delta_eps);
    EquatorialPosition {
        longitude_deg: apparent_longitude,
        latitude_deg: latitude,
        right_ascension_deg: ra,
        declination_deg: dec,
        distance_km: distance,
    }
}

/// Apparent Greenwich sidereal time, degrees (Meeus 12.4 plus the equation of the equinoxes).
pub fn apparent_sidereal_time_deg(instant: UtcInstant) -> f64 {
    let jd = instant.jd;
    let t = (jd - J2000) / 36_525.0;
    let mean = 280.460_618_37 + 360.985_647_366_29 * (jd - J2000) + 0.000_387_933 * t * t
        - t * t * t / 38_710_000.0;
    let (delta_psi, delta_eps, eps0) = nutation(instant.jde());
    wrap_degrees(mean + delta_psi * ((eps0 + delta_eps) * DEG).cos())
}

/// Where a body stands in the sky of a site.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalPosition {
    /// Geometric altitude above the astronomical horizon, degrees (no refraction).
    pub altitude_deg: f64,
    /// Compass azimuth: 0 north, 90 east, degrees.
    pub azimuth_deg: f64,
}

impl HorizontalPosition {
    /// Unit vector toward the body in engine axes (+X east, +Y up, +Z south).
    pub fn engine_direction(self) -> DVec3 {
        let (h, a) = (self.altitude_deg * DEG, self.azimuth_deg * DEG);
        DVec3::new(a.sin() * h.cos(), h.sin(), -a.cos() * h.cos())
    }

    /// Engine azimuth (compass azimuth minus 90 degrees), for comparison with the retired
    /// version-1 lighting records.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn engine_azimuth_deg(self) -> f64 {
        wrap_degrees(self.azimuth_deg - 90.0)
    }
}

fn horizontal_from_hour_angle(hour_angle_deg: f64, declination_deg: f64, latitude_deg: f64)
    -> HorizontalPosition
{
    let (h, d, phi) = (hour_angle_deg * DEG, declination_deg * DEG, latitude_deg * DEG);
    let altitude = (phi.sin() * d.sin() + phi.cos() * d.cos() * h.cos()).asin();
    // Meeus 13.5 measures azimuth from the south, westward; the compass turns it by 180 deg.
    let south_azimuth = h.sin().atan2(h.cos() * phi.sin() - d.tan() * phi.cos());
    HorizontalPosition {
        altitude_deg: altitude / DEG,
        azimuth_deg: wrap_degrees(south_azimuth / DEG + 180.0),
    }
}

/// Geocentric horizontal position (no parallax), used for the Sun.
pub fn horizontal_position(position: &EquatorialPosition, instant: UtcInstant, site: Site)
    -> HorizontalPosition
{
    let hour_angle = apparent_sidereal_time_deg(instant) + site.longitude_deg_east
        - position.right_ascension_deg;
    horizontal_from_hour_angle(hour_angle, position.declination_deg, site.latitude_deg)
}

/// Topocentric horizontal position including the diurnal parallax (Meeus 11 and 40). The Moon's
/// horizontal parallax is about 0.95 deg, nearly twice its disc.
pub fn topocentric_horizontal_position(position: &EquatorialPosition, instant: UtcInstant, site: Site)
    -> HorizontalPosition
{
    let phi = site.latitude_deg * DEG;
    let u = (0.996_647_19 * phi.tan()).atan();
    let height_ratio = site.height_m / (EARTH_EQUATORIAL_RADIUS_KM * 1000.0);
    let rho_sin = 0.996_647_19 * u.sin() + height_ratio * phi.sin();
    let rho_cos = u.cos() + height_ratio * phi.cos();
    let parallax = (EARTH_EQUATORIAL_RADIUS_KM / position.distance_km).asin();
    let hour_angle = (apparent_sidereal_time_deg(instant) + site.longitude_deg_east
        - position.right_ascension_deg) * DEG;
    let delta = position.declination_deg * DEG;
    let denominator = delta.cos() - rho_cos * parallax.sin() * hour_angle.cos();
    let delta_alpha = (-rho_cos * parallax.sin() * hour_angle.sin()).atan2(denominator);
    let topocentric_delta = ((delta.sin() - rho_sin * parallax.sin()) * delta_alpha.cos())
        .atan2(denominator);
    horizontal_from_hour_angle((hour_angle - delta_alpha) / DEG, topocentric_delta / DEG,
        site.latitude_deg)
}

/// The Moon's illumination geometry (Meeus 48.2-48.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LunarPhase {
    /// Geocentric elongation from the Sun, 0 at new Moon and pi at full, radians.
    pub elongation_rad: f64,
    /// Selenocentric phase angle Sun-Moon-Earth, radians (0 at full).
    pub phase_angle_rad: f64,
    pub illuminated_fraction: f64,
    /// Waxing: the Moon east of the Sun (apparent longitude difference below 180 deg).
    pub waxing: bool,
}

pub fn lunar_phase(sun: &EquatorialPosition, moon: &EquatorialPosition) -> LunarPhase {
    let longitude_difference = (moon.longitude_deg - sun.longitude_deg) * DEG;
    let cos_psi = (moon.latitude_deg * DEG).cos() * longitude_difference.cos();
    let psi = cos_psi.clamp(-1.0, 1.0).acos();
    let phase_angle = (sun.distance_km * psi.sin()).atan2(moon.distance_km - sun.distance_km * cos_psi);
    LunarPhase {
        elongation_rad: psi,
        phase_angle_rad: phase_angle,
        illuminated_fraction: 0.5 * (1.0 + phase_angle.cos()),
        waxing: wrap_degrees(moon.longitude_deg - sun.longitude_deg) < 180.0,
    }
}

/// Everything the sky of one instant needs from the ephemeris.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialState {
    pub instant: UtcInstant,
    pub site: Site,
    pub sun: HorizontalPosition,
    pub sun_distance_km: f64,
    pub moon: HorizontalPosition,
    pub moon_distance_km: f64,
    pub moon_phase: LunarPhase,
}

impl CelestialState {
    pub fn at(instant: UtcInstant, site: Site) -> Self {
        let jde = instant.jde();
        let sun = sun_position(jde);
        let moon = moon_position(jde);
        Self {
            instant,
            site,
            sun: horizontal_position(&sun, instant, site),
            sun_distance_km: sun.distance_km,
            moon: topocentric_horizontal_position(&moon, instant, site),
            moon_distance_km: moon.distance_km,
            moon_phase: lunar_phase(&sun, &moon),
        }
    }

    /// Mean apparent angular radius of the Moon's disc, radians.
    pub fn moon_angular_radius_rad(&self) -> f64 {
        (MOON_RADIUS_KM / self.moon_distance_km).asin()
    }

    /// Apparent angular radius of the Sun's disc, radians (959.63" at 1 au).
    pub fn sun_angular_radius_rad(&self) -> f64 {
        (959.63 / 3600.0 * DEG) * AU_KM / self.sun_distance_km
    }

    /// The Moon's illuminance at the top of the atmosphere on a surface facing it, lux: the USNO
    /// Circular 171 phase law (research section 2.3) at the mean distance, scaled by the inverse
    /// square of the actual distance (apogee to perigee is about 20 %).
    pub fn moon_toa_illuminance_lux(&self) -> f64 {
        usno_moon_phase_factor(self.moon_phase.elongation_rad, self.moon.altitude_deg)
            * (MOON_MEAN_DISTANCE_KM / self.moon_distance_km).powi(2)
    }

    /// The Moon's spectral irradiance at the top of the atmosphere in the renderer's three bands
    /// (W m^-2 nm^-1 at 680/550/440 nm): its photometric illuminance distributed over the solar
    /// spectrum reddened by the lunar reflectance, relative 1.22/1.00/0.80 (research section 2.3,
    /// the reflectance slope an estimate).
    pub fn moon_toa_spectral_irradiance(&self, solar_toa_irradiance: DVec3) -> DVec3 {
        let shape = solar_toa_irradiance * LUNAR_RELATIVE_REFLECTANCE;
        let lux_per_unit = photopic_illuminance_lux(shape);
        if lux_per_unit <= 0.0 { return DVec3::ZERO; }
        shape * (self.moon_toa_illuminance_lux() / lux_per_unit)
    }
}

/// Lunar reflectance at 680/550/440 nm relative to 550 nm [E] (research section 2.3).
pub const LUNAR_RELATIVE_REFLECTANCE: DVec3 = DVec3::new(1.22, 1.00, 0.80);

/// Photopic luminance of a spectral-radiance triplet at 680/550/440 nm, cd m^-2 per
/// W m^-2 sr^-1 nm^-1: CIE 1924 V(lambda) integrated over a piecewise-linear spectrum through the
/// three samples (research section 1.2, CVRL tables; sum 72,983). Applied to a spectral irradiance
/// triplet the same weights give lux.
pub const PHOTOPIC_WEIGHTS: DVec3 = DVec3::new(12_458.6, 52_558.9, 7_965.8);

pub fn photopic_illuminance_lux(spectral_irradiance: DVec3) -> f64 {
    spectral_irradiance.dot(PHOTOPIC_WEIGHTS)
}

// ---- USNO Circular 171 -------------------------------------------------------------------
// The Circular's empirical illuminance model is the reference the tests hold the physical
// atmosphere against; the renderer itself uses only the Moon's phase factor.

/// Brown's sky-condition divisors quoted by USNO Circular 171.
#[cfg_attr(not(test), allow(dead_code))]
pub const SKY_CLEAR: f64 = 1.0;
#[cfg_attr(not(test), allow(dead_code))]
pub const SKY_THIN_CLOUD: f64 = 2.0;
#[cfg_attr(not(test), allow(dead_code))]
pub const SKY_AVERAGE_CLOUD: f64 = 3.0;
#[cfg_attr(not(test), allow(dead_code))]
pub const SKY_DARK_STRATUS: f64 = 10.0;

/// The Circular's attenuation function M(h) of the geometric altitude h (degrees): refraction,
/// the air mass of a homogeneous spherical atmosphere and the diffuse term of Brown's curve.
#[cfg_attr(not(test), allow(dead_code))]
pub fn usno_attenuation(altitude_deg: f64) -> f64 {
    let h = altitude_deg;
    let refracted = if h < -5.0 / 6.0 {
        h
    } else {
        h + 1.0 / ((h + 8.6 / (h + 4.42)) * DEG).tan() / 60.0
    };
    let u = (refracted * DEG).sin();
    const X: f64 = 753.661_56;
    let s = (X * (refracted * DEG).cos() / (X + 1.0)).asin();
    let air_mass = X * (s.cos() - u) + s.cos();
    (-0.21 * air_mass).exp() * u
        + 0.0289 * (-0.042 * air_mass).exp() * (1.0 + (refracted + 90.0) * u / 57.295_78)
}

/// Horizontal illuminance from the Sun (without the night-sky term), lux.
#[cfg_attr(not(test), allow(dead_code))]
pub fn usno_sun_illuminance_lux(altitude_deg: f64, sky_divisor: f64) -> f64 {
    (133_775.0 * usno_attenuation(altitude_deg) / sky_divisor).max(0.0)
}

/// The Circular's night-sky term, lux (the text quotes 0.0003, the programs add 0.0005).
#[cfg_attr(not(test), allow(dead_code))]
pub const USNO_NIGHT_SKY_LUX: f64 = 0.0005;

/// The Circular's lunar phase factor P(E): the illuminance of the Moon at the mean distance
/// before atmospheric attenuation, lux. `elongation_rad` is 0 at new and pi at full; the altitude
/// enters the fit's small correction term.
pub fn usno_moon_phase_factor(elongation_rad: f64, altitude_deg: f64) -> f64 {
    let e = elongation_rad.clamp(1.0e-6, std::f64::consts::PI);
    let half_tan = (e * 0.5).tan();
    let opposition = if half_tan.is_finite() && half_tan > 0.0 {
        0.892 * (-3.343 / half_tan.powf(0.632)).exp()
    } else {
        0.892
    };
    let p = opposition + 0.0344 * (e.sin() - e * e.cos());
    (0.418 * p / (1.0 - 0.005 * e.cos() - 0.03 * (altitude_deg * DEG).sin())).max(0.0)
}

/// Horizontal illuminance from the Moon, lux, as the Circular computes it: geocentric altitude
/// corrected by the mean parallax, the phase factor and M(H).
#[cfg_attr(not(test), allow(dead_code))]
pub fn usno_moon_illuminance_lux(geocentric_altitude_deg: f64, elongation_rad: f64, sky_divisor: f64)
    -> f64
{
    let h = geocentric_altitude_deg;
    let topocentric = h - 0.95 * (h * DEG).cos();
    (usno_moon_phase_factor(elongation_rad, h) * usno_attenuation(topocentric) / sky_divisor).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deg_close(a: f64, b: f64, tolerance: f64) -> bool {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d) <= tolerance
    }

    #[test]
    fn julian_day_and_calendar_round_trip() {
        assert_eq!(julian_day(2000, 1, 1.5), J2000);
        assert!((julian_day(1957, 10, 4.81) - 2_436_116.31).abs() < 1e-6); // Meeus 7.a
        let instant = UtcInstant::parse_iso("1942-12-21T12:00:00Z").unwrap();
        assert_eq!(instant.to_iso(), "1942-12-21T12:00:00Z");
        assert_eq!(UtcInstant::parse_iso("1943-01-15T12:27Z").unwrap().to_iso(), "1943-01-15T12:27:00Z");
        assert!(UtcInstant::parse_iso("1942-12-21T12:00:00").is_err(), "local time is refused");
        assert!(UtcInstant::parse_iso("1942-13-21T12:00:00Z").is_err());
        assert!((delta_t_seconds(1943.0) - 26.0).abs() < 0.5, "Delta T 1943 {}", delta_t_seconds(1943.0));
    }

    #[test]
    fn sun_reproduces_meeus_example_25a() {
        // 1992 October 13.0 TD.
        let sun = sun_position(2_448_908.5);
        assert!((sun.longitude_deg - 199.908_95).abs() < 2e-5, "{sun:?}");
        assert!((sun.right_ascension_deg - 198.380_83).abs() < 2e-4, "{sun:?}");
        assert!((sun.declination_deg + 7.785_07).abs() < 2e-4, "{sun:?}");
        assert!((sun.distance_km / AU_KM - 0.997_66).abs() < 1e-5, "{sun:?}");
    }

    #[test]
    fn moon_reproduces_meeus_example_47a() {
        // 1992 April 12.0 TD.
        let (longitude, latitude, distance) = moon_geometric_position(2_448_724.5);
        assert!((longitude - 133.162_655).abs() < 1e-6, "lambda {longitude}");
        assert!((latitude + 3.229_126).abs() < 1e-6, "beta {latitude}");
        assert!((distance - 368_409.7).abs() < 0.1, "Delta {distance}");
        let apparent = moon_position(2_448_724.5);
        // Book: alpha 134.688470, delta 13.768368 with the full nutation series; the four-term
        // series leaves about 0.1".
        assert!((apparent.right_ascension_deg - 134.688_470).abs() < 1e-4, "{apparent:?}");
        assert!((apparent.declination_deg - 13.768_368).abs() < 1e-4, "{apparent:?}");
    }

    fn phase_elongation(jd_ut: f64) -> f64 {
        let instant = UtcInstant { jd: jd_ut };
        let jde = instant.jde();
        (moon_position(jde).longitude_deg - sun_position(jde).longitude_deg).rem_euclid(360.0)
    }

    /// Time (JD UT) near `guess` when the Moon-Sun longitude difference equals `target`.
    fn phase_time(guess: f64, target: f64) -> f64 {
        let offset = |jd: f64| {
            let d = phase_elongation(jd) - target;
            (d + 180.0).rem_euclid(360.0) - 180.0
        };
        let (mut lo, mut hi) = (guess - 3.0, guess + 3.0);
        assert!(offset(lo) < 0.0 && offset(hi) > 0.0, "phase not bracketed near {guess}");
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if offset(mid) < 0.0 { lo = mid } else { hi = mid }
        }
        0.5 * (lo + hi)
    }

    #[test]
    fn moon_phases_match_the_espenak_catalogue_for_the_campaign() {
        // NASA/Espenak Six Millennium Catalog, as quoted in the research (section 3.4).
        let catalogue: [(&str, f64); 16] = [
            ("1942-11-08T15:19Z", 0.0), ("1942-11-15T06:57Z", 90.0), ("1942-11-22T20:24Z", 180.0),
            ("1942-12-01T01:37Z", 270.0), ("1942-12-08T01:59Z", 0.0), ("1942-12-14T17:47Z", 90.0),
            ("1942-12-22T15:03Z", 180.0), ("1942-12-30T18:37Z", 270.0), ("1943-01-06T12:38Z", 0.0),
            ("1943-01-13T07:49Z", 90.0), ("1943-01-21T10:48Z", 180.0), ("1943-01-29T08:13Z", 270.0),
            ("1943-02-04T23:29Z", 0.0), ("1943-02-12T00:40Z", 90.0), ("1943-02-20T05:45Z", 180.0),
            ("1943-02-27T18:23Z", 270.0),
        ];
        for (when, target) in catalogue {
            let listed = UtcInstant::parse_iso(when).unwrap().jd;
            let computed = phase_time(listed, target);
            let minutes = (computed - listed) * 1440.0;
            assert!(minutes.abs() <= 2.0, "{when} phase {target}: {minutes:+.2} min");
        }
    }

    #[test]
    fn stalingrad_1500_local_apparent_time_on_the_solstice() {
        let instant = UtcInstant::parse_iso("1942-12-21T12:00:00Z").unwrap();
        let sky = CelestialState::at(instant, STALINGRAD);
        assert!((sky.sun.altitude_deg - 7.41).abs() <= 0.05, "{:?}", sky.sun);
        assert!(deg_close(sky.sun.azimuth_deg, 220.9, 0.2), "{:?}", sky.sun);
        let d = sky.sun.engine_direction();
        assert!((d - DVec3::new(-0.649, 0.129, 0.750)).length() < 3e-3, "{d:?}");
        assert!(deg_close(sky.sun.engine_azimuth_deg(), 130.9, 0.2));
        // The authored courtyard sun (engine azimuth 38, compass 128) is a morning direction.
        assert!(!deg_close(sky.sun.engine_azimuth_deg(), 38.0, 80.0));
    }

    #[test]
    fn winter_noon_never_reaches_the_old_day_key() {
        // The Sun culminates at 17.85 deg on the solstice and 25 deg in early February; the
        // retired DayPhase::Day key stood at 31.5 deg.
        for date in ["1942-11-19", "1942-12-21", "1943-01-15", "1943-02-02"] {
            let mut best: f64 = -90.0;
            for minute in (0..24 * 60).step_by(5) {
                let instant = UtcInstant::parse_iso(&format!("{date}T00:00Z")).unwrap()
                    .plus_seconds(minute as f64 * 60.0);
                best = best.max(CelestialState::at(instant, STALINGRAD).sun.altitude_deg);
            }
            assert!(best < 25.5 && best > 17.5, "{date}: noon {best}");
        }
        let solstice_noon = CelestialState::at(UtcInstant::parse_iso("1942-12-21T09:00Z").unwrap(),
            STALINGRAD).sun.altitude_deg;
        assert!((solstice_noon - 17.85).abs() < 0.1, "{solstice_noon}");
    }

    #[test]
    fn winter_full_moon_culminates_near_sixty_degrees_in_the_south() {
        // 22 Dec 1942 around 21 UT (research table 3.5 / Appendix B: 60.2 deg).
        let mut best = (f64::MIN, 0.0);
        for minute in 0..240 {
            let instant = UtcInstant::parse_iso("1942-12-22T19:00Z").unwrap()
                .plus_seconds(minute as f64 * 60.0);
            let sky = CelestialState::at(instant, STALINGRAD);
            if sky.moon.altitude_deg > best.0 { best = (sky.moon.altitude_deg, sky.moon.azimuth_deg); }
        }
        assert!((best.0 - 60.2).abs() < 0.6, "culmination {best:?}");
        assert!(deg_close(best.1, 180.0, 5.0), "due south {best:?}");
        let full = CelestialState::at(UtcInstant::parse_iso("1942-12-22T15:03Z").unwrap(), STALINGRAD);
        assert!(full.moon_phase.illuminated_fraction > 0.995, "{:?}", full.moon_phase);
        assert!((full.moon_angular_radius_rad() / DEG - 0.26).abs() < 0.02);
    }

    #[test]
    fn usno_circular_171_reproduces_its_table() {
        // Research section 2.1, clear-sky column.
        for (altitude, lux, tolerance) in [
            (60.0, 102_960.0, 0.01), (35.4, 62_156.0, 0.01), (17.85, 26_182.0, 0.01),
            (7.5, 7_329.0, 0.01), (0.0, 984.0, 0.02), (-3.0, 66.4, 0.03), (-6.0, 2.99, 0.03),
            (-9.0, 0.116, 0.05), (-12.0, 0.0044, 0.05),
        ] {
            let computed = usno_sun_illuminance_lux(altitude, SKY_CLEAR);
            assert!((computed / lux - 1.0).abs() < tolerance, "{altitude}: {computed} vs {lux}");
        }
        // Dark stratus divides by ten: 733 lx at the courtyard's 7.5 deg.
        assert!((usno_sun_illuminance_lux(7.5, SKY_DARK_STRATUS) - 733.0).abs() < 8.0);
    }

    #[test]
    fn moonlight_on_the_campaign_nights_matches_the_research_table() {
        // Appendix B maxima in astronomical darkness, clear sky (USNO 171), and Kyba et al. 2017's
        // measured ceiling of about 0.3 lx.
        for (start, expected) in [("1942-12-22T15:10Z", 0.222), ("1942-12-24T19:00Z", 0.111),
            ("1943-01-21T15:30Z", 0.213)] {
            let mut best: f64 = 0.0;
            for minute in (0..16 * 60).step_by(4) {
                let instant = UtcInstant::parse_iso(start).unwrap().plus_seconds(minute as f64 * 60.0);
                let jde = instant.jde();
                let (sun, moon) = (sun_position(jde), moon_position(jde));
                let geocentric = horizontal_position(&moon, instant, STALINGRAD);
                let sun_up = horizontal_position(&sun, instant, STALINGRAD);
                if sun_up.altitude_deg > -18.0 { continue; }
                let phase = lunar_phase(&sun, &moon);
                best = best.max(usno_moon_illuminance_lux(geocentric.altitude_deg,
                    phase.elongation_rad, SKY_CLEAR));
            }
            assert!((best / expected - 1.0).abs() < 0.08, "{start}: {best} vs {expected}");
            assert!(best < 0.3);
        }
    }

    #[test]
    fn lunar_top_of_atmosphere_photometry_is_the_full_moon_disc_of_the_research() {
        let full = CelestialState::at(UtcInstant::parse_iso("1942-12-22T15:03Z").unwrap(), STALINGRAD);
        let solar = DVec3::new(1.474, 1.8504, 1.91198);
        let irradiance = full.moon_toa_spectral_irradiance(solar);
        let lux = photopic_illuminance_lux(irradiance);
        assert!((lux - full.moon_toa_illuminance_lux()).abs() < 1e-9);
        // 22 Dec 1942 15:03 UT: the phase angle is a few degrees, below the opposition peak.
        assert!((0.24..0.48).contains(&lux), "full Moon at the top of the atmosphere: {lux} lx, {:?}",
            full.moon_phase);
        // Disc radiance near (0.067, 0.069, 0.057) RU for 4,900 cd/m^2 (section 2.3).
        let omega = std::f64::consts::PI * full.moon_angular_radius_rad().powi(2);
        let radiance = irradiance / omega;
        let luminance = radiance.dot(PHOTOPIC_WEIGHTS);
        assert!((3_500.0..7_500.0).contains(&luminance), "disc {luminance} cd/m^2");
        assert!(radiance.x > radiance.z, "lunar reflectance reddens the disc");
        let quarter = CelestialState::at(UtcInstant::parse_iso("1942-12-14T17:47Z").unwrap(), STALINGRAD);
        let ratio = quarter.moon_toa_illuminance_lux() / full.moon_toa_illuminance_lux();
        assert!((0.05..0.12).contains(&ratio), "quarter/full {ratio}");
    }

    #[test]
    fn solar_photometric_weights_give_the_solar_constant() {
        let lux = photopic_illuminance_lux(DVec3::new(1.474, 1.8504, 1.91198));
        // Research section 1.1: 132,538 lx from the full table; the three-sample read is 0.987.
        assert!((lux / 132_538.0 - 0.987).abs() < 0.005, "{lux}");
        assert!((PHOTOPIC_WEIGHTS.x + PHOTOPIC_WEIGHTS.y + PHOTOPIC_WEIGHTS.z - 72_983.3).abs() < 1.0);
    }
}
