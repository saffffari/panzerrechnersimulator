//! Leaf optics for the meadow's blades: PROSPECT-D (Feret et al. 2017, after Jacquemoud & Baret
//! 1990's plate model) evaluated at the renderer's three wavelengths (680 / 550 / 440 nm), giving
//! a blade's diffuse reflectance rho and transmittance tau. The specific absorption coefficients
//! and refractive index are PROSPECT-D's published table (as distributed with the prosail package,
//! `prospect_d_spectra.txt`) at those wavelengths; the leaf parameters are a fresh grass blade's,
//! inside the ranges of Jacquemoud & Ustin (2019, Leaf Optical Properties, Cambridge). The
//! shading model these feed is a bi-Lambertian sheet with a cuticle specular
//! (`shaders/rm_shade.wgsl`, kind 5), after Baranoski & Rokne's survey of leaf models (1998).

/// PROSPECT-D table rows at 680, 550, 440 nm: refractive index, k_Cab (cm^2/ug), k_Car (cm^2/ug),
/// k_Anth (cm^2/ug), k_brown (arbitrary), k_w (1/cm), k_m (cm^2/g).
pub const TABLE: [[f64; 7]; 3] = [
    [1.4458, 6.89206e-02, 0.0, 2.01097e-04, 1.50400e-01, 4.31800e-03, 2.30000e+00],
    [1.4739, 9.30939e-03, 4.18898e-03, 6.81398e-02, 3.51100e-01, 5.88000e-04, 2.30000e+00],
    [1.4980, 7.37942e-02, 1.69569e-01, 4.47865e-02, 4.85200e-01, 1.04000e-04, 1.09600e+01],
];

/// A fresh spring grass blade: structure N, chlorophyll a+b, carotenoids, anthocyanins (ug/cm^2),
/// brown pigments, equivalent water thickness (cm), dry matter (g/cm^2). Estimates.
#[derive(Clone, Copy, Debug)]
pub struct Leaf { pub n: f64, pub cab: f64, pub car: f64, pub anth: f64, pub brown: f64, pub cw: f64, pub cm: f64 }
pub const SPRING_GRASS: Leaf = Leaf { n: 1.5, cab: 40.0, car: 9.0, anth: 0.0, brown: 0.0, cw: 0.010, cm: 0.004 };
/// An older or nitrogen-poor blade: less chlorophyll.
pub const PALE_GRASS: Leaf = Leaf { n: 1.6, cab: 20.0, car: 6.0, anth: 0.0, brown: 0.0, cw: 0.009, cm: 0.004 };
/// A senescent tip or dry stand: chlorophyll gone, brown pigments, little water.
pub const DRY_GRASS: Leaf = Leaf { n: 1.8, cab: 2.0, car: 3.0, anth: 0.0, brown: 0.8, cw: 0.002, cm: 0.006 };
/// GGX roughness of a grass blade's waxy cuticle (an estimate: grass shows a broad sheen).
pub const CUTICLE_ROUGHNESS: f32 = 0.4;

/// Exponential integral E1(x), x > 0 (series below 1, continued fraction above).
pub fn e1(x: f64) -> f64 {
    if x <= 1.0 {
        let mut sum = 0.0;
        let mut term = 1.0;
        for k in 1..60 {
            term *= -x / k as f64;
            sum += -term / k as f64;
        }
        -0.577_215_664_901_532_9 - x.ln() + sum
    } else {
        // Lentz continued fraction.
        let mut b = x + 1.0;
        let mut c = 1.0 / 1e-300;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..200 {
            let an = -(i as f64) * (i as f64);
            b += 2.0;
            d = 1.0 / (an * d + b);
            c = b + an / c;
            let del = c * d;
            h *= del;
            if (del - 1.0).abs() < 1e-15 { break; }
        }
        h * (-x).exp()
    }
}

/// Average transmissivity of a dielectric plane surface for isotropic incidence within a cone of
/// half-angle `alpha` degrees (Stern 1964; Allen et al. 1969), as PROSPECT computes it.
pub fn tav(alpha_deg: f64, nr: f64) -> f64 {
    let rd = std::f64::consts::PI / 180.0;
    let n2 = nr * nr;
    let np = n2 + 1.0;
    let nm = n2 - 1.0;
    let a = (nr + 1.0) * (nr + 1.0) / 2.0;
    let k = -(n2 - 1.0) * (n2 - 1.0) / 4.0;
    let sa = (alpha_deg * rd).sin();
    let b1 = if alpha_deg != 90.0 { ((sa * sa - np / 2.0).powi(2) + k).sqrt() } else { 0.0 };
    let b2 = sa * sa - np / 2.0;
    let b = b1 - b2;
    let b3 = b.powi(3);
    let a3 = a.powi(3);
    let ts = (k * k / (6.0 * b3) + k / b - b / 2.0) - (k * k / (6.0 * a3) + k / a - a / 2.0);
    let tp1 = -2.0 * n2 * (b - a) / (np * np);
    let tp2 = -2.0 * n2 * np * (b / a).ln() / (nm * nm);
    let tp3 = n2 * (1.0 / b - 1.0 / a) / 2.0;
    let tp4 = 16.0 * n2 * n2 * (n2 * n2 + 1.0) * ((2.0 * np * b - nm * nm) / (2.0 * np * a - nm * nm)).ln() / (np.powi(3) * nm * nm);
    let tp5 = 16.0 * n2.powi(3) * (1.0 / (2.0 * np * b - nm * nm) - 1.0 / (2.0 * np * a - nm * nm)) / np.powi(3);
    (ts + tp1 + tp2 + tp3 + tp4 + tp5) / (2.0 * sa * sa)
}

/// PROSPECT's plate model at one table row: (reflectance, transmittance).
pub fn prospect(leaf: Leaf, row: [f64; 7]) -> (f64, f64) {
    let [nr, kab, kcar, kant, kbrown, kw, km] = row;
    let k = (leaf.cab * kab + leaf.car * kcar + leaf.anth * kant + leaf.brown * kbrown + leaf.cw * kw + leaf.cm * km) / leaf.n;
    let tau = if k > 0.0 { (1.0 - k) * (-k).exp() + k * k * e1(k) } else { 1.0 };
    // One elementary layer: top surface for incidence within 40 deg, inner faces isotropic.
    let talf = tav(40.0, nr);
    let ralf = 1.0 - talf;
    let t12 = tav(90.0, nr);
    let r12 = 1.0 - t12;
    let t21 = t12 / (nr * nr);
    let r21 = 1.0 - t21;
    let denom = 1.0 - r21 * r21 * tau * tau;
    let ta = talf * tau * t21 / denom;
    let ra = ralf + r21 * tau * ta;
    let t = t12 * tau * t21 / denom;
    let r = r12 + r21 * tau * t;
    // N - 1 further layers (Stokes).
    let d = ((1.0 + r + t) * (1.0 + r - t) * (1.0 - r + t) * (1.0 - r - t)).sqrt();
    let (rq, tq) = (r * r, t * t);
    let a = (1.0 + rq - tq + d) / (2.0 * r);
    let b = (1.0 - rq + tq + d) / (2.0 * t);
    let bnm1 = b.powf(leaf.n - 1.0);
    let bn2 = bnm1 * bnm1;
    let a2 = a * a;
    let den = a2 * bn2 - 1.0;
    let (rsub, tsub) = if r + t >= 1.0 {
        let ts = t / (t + (1.0 - t) * (leaf.n - 1.0));
        (1.0 - ts, ts)
    } else {
        (a * (bn2 - 1.0) / den, bnm1 * (a2 - 1.0) / den)
    };
    let den = 1.0 - rsub * r;
    (ra + ta * rsub * t / den, ta * tsub / den)
}

/// The blade's (rho, tau) in the renderer's lanes (680, 550, 440 nm).
pub fn lanes(leaf: Leaf) -> ([f32; 3], [f32; 3]) {
    let mut rho = [0.0f32; 3];
    let mut tau = [0.0f32; 3];
    for i in 0..3 {
        let (r, t) = prospect(leaf, TABLE[i]);
        rho[i] = r as f32;
        tau[i] = t as f32;
    }
    (rho, tau)
}

/// Fresnel reflectance at normal incidence of the cuticle (wax, n ~ 1.47 at 550 nm).
pub fn cuticle_f0(n: f64) -> f64 { ((n - 1.0) / (n + 1.0)).powi(2) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exponential_integral_and_surface_transmissivity_are_right() {
        assert!((e1(0.5) - 0.559_773_594_8).abs() < 1e-9);
        assert!((e1(2.0) - 0.048_900_510_7).abs() < 1e-9);
        // Normal-ish incidence through n = 1.5 transmits about 96 %; isotropic about 90 %.
        assert!((tav(40.0, 1.5) - 0.955).abs() < 0.01, "{}", tav(40.0, 1.5));
        assert!((tav(90.0, 1.5) - 0.908).abs() < 0.01, "{}", tav(90.0, 1.5));
    }

    #[test]
    fn a_fresh_blade_is_green_dark_in_red_and_blue_and_lets_green_through() {
        let (rho, tau) = lanes(SPRING_GRASS);
        // Typical fresh leaves: R(550) ~ 0.1-0.2, R(680) and R(440) ~ 0.04-0.07, T(550) ~ 0.05-0.2.
        assert!(rho[1] > 0.08 && rho[1] < 0.22, "{rho:?}");
        assert!(rho[0] < rho[1] * 0.7 && rho[2] < rho[1] * 0.7, "{rho:?}");
        assert!(rho[0] > 0.03 && rho[2] > 0.03, "the surface reflection floor: {rho:?}");
        assert!(tau[1] > 0.04 && tau[1] < 0.25, "{tau:?}");
        assert!(tau[0] < tau[1] && tau[2] < tau[1], "{tau:?}");
        for i in 0..3 { assert!(rho[i] + tau[i] < 1.0); }
        assert!((cuticle_f0(1.47) - 0.036).abs() < 0.002);
    }
}
