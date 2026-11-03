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

/// The far meadow as a turbid canopy slab (SAIL-type, Verhoef 1984/1998): leaves of uniformly
/// distributed orientation (spherical; G = 0.5) and bi-Lambertian optics. Its area scattering
/// phase function (Ross 1981) is linear in rho and tau, Gamma = rho Gamma_r(beta) + tau Gamma_t(beta),
/// beta the angle between the beam's travel and the scattered direction; with the source term
/// J = u (Gamma / pi) E for leaf area density u. Tabulated at `GAMMA_STEPS` angles over [0, pi].
pub const GAMMA_STEPS: usize = 17;
/// SAIL's hotspot size parameter: leaf width over canopy height (7 mm / 0.45 m, rounded up).
pub const HOTSPOT: f32 = 0.02;
/// Dry soil under the far canopy (an estimate; it shows only where the canopy is thin).
pub const SOIL_ALBEDO: f32 = 0.12;
pub fn gamma_tables() -> ([f32; GAMMA_STEPS], [f32; GAMMA_STEPS]) {
    let mut r = [0.0f32; GAMMA_STEPS];
    let mut t = [0.0f32; GAMMA_STEPS];
    // Quadrature over leaf normals on the sphere (theta, phi grid, solid-angle weighted).
    let (nt, np) = (180usize, 360usize);
    for k in 0..GAMMA_STEPS {
        let beta = std::f64::consts::PI * k as f64 / (GAMMA_STEPS - 1) as f64;
        let a = [0.0, 0.0, 1.0f64]; // the beam's direction of travel
        let b = [beta.sin(), 0.0, beta.cos()]; // the scattered direction
        let (mut sr, mut st) = (0.0f64, 0.0f64);
        for i in 0..nt {
            let th = std::f64::consts::PI * (i as f64 + 0.5) / nt as f64;
            let w = th.sin() * (std::f64::consts::PI / nt as f64) * (std::f64::consts::TAU / np as f64);
            for j in 0..np {
                let ph = std::f64::consts::TAU * (j as f64 + 0.5) / np as f64;
                let n = [th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos()];
                let ca = a[0] * n[0] + a[1] * n[1] + a[2] * n[2];
                let cb = b[0] * n[0] + b[1] * n[1] + b[2] * n[2];
                // Reflection when the scattered direction leaves the face the beam came in on.
                let v = ca.abs() * cb.abs() * w;
                if ca * cb < 0.0 { sr += v } else { st += v }
            }
        }
        // Gamma = (1/4) integral |a.n| |b.n| f dn with f = rho/pi or tau/pi.
        r[k] = (sr / (4.0 * std::f64::consts::PI)) as f32;
        t[k] = (st / (4.0 * std::f64::consts::PI)) as f32;
    }
    (r, t)
}

/// Kubelka-Munk diffuse reflectance of a deep canopy of single-scattering albedo omega = rho + tau,
/// backscattering a share 1/2 + (rho - tau) / (6 omega) of what it scatters (spherical leaves; the
/// two-stream canopy of Sellers 1985).
pub fn km_infinite_reflectance(rho: f64, tau: f64) -> f64 {
    let omega = (rho + tau).max(1e-6);
    let back = 0.5 + (rho - tau) / (6.0 * omega);
    let k = 1.0 - omega;
    let s = back * omega;
    let q = k / s.max(1e-9);
    1.0 + q - (q * q + 2.0 * q).sqrt()
}

/// The slab's bidirectional reflectance factor (pi L / (E mu_s)) for one lane, as rm_shade.wgsl
/// computes it: single scattering with Kuusk's hotspot, the soil through both gaps, and the
/// multiple-scattering estimate. `alpha` is the hotspot's angular separation (0 at the hotspot),
/// `beta` the angle between the beam's travel and the view direction.
pub fn slab_reflectance(rho: f64, tau: f64, lai: f64, mu_s: f64, mu_v: f64, alpha: f64, beta: f64) -> f64 {
    let (gr, gt) = gamma_tables();
    let x = (beta / std::f64::consts::PI).clamp(0.0, 1.0) * (GAMMA_STEPS - 1) as f64;
    let i = (x as usize).min(GAMMA_STEPS - 2);
    let t = x - i as f64;
    let lerp = |g: [f32; GAMMA_STEPS]| g[i] as f64 * (1.0 - t) + g[i + 1] as f64 * t;
    let gamma = rho * lerp(gr) + tau * lerp(gt);
    let (ks, kv) = (0.5 / mu_s, 0.5 / mu_v);
    let h = HOTSPOT as f64;
    let mut gap = 0.0;
    let dl = lai / 8.0;
    for k in 0..8 {
        let l = (k as f64 + 0.5) * dl;
        let shared = if alpha > 1e-4 { (ks * kv).sqrt() * h / alpha * (1.0 - (-alpha * l / h).exp()) } else { (ks * kv).sqrt() * l };
        gap += (-(ks + kv) * l + shared).exp() * dl;
    }
    let single = gamma * gap / (mu_v * mu_s);
    let soil = SOIL_ALBEDO as f64 * (-(ks + kv) * lai).exp();
    let multiple = (km_infinite_reflectance(rho, tau) - (rho + tau) * 0.25).max(0.0);
    single + soil + multiple
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
    fn the_canopy_phase_function_conserves_energy_and_backscatters_the_reflection() {
        let (gr, gt) = gamma_tables();
        // Integral over all scattered directions of Gamma/pi = omega G, G = 1/2 (spherical).
        let steps = GAMMA_STEPS - 1;
        let (mut ir, mut it) = (0.0f64, 0.0f64);
        for k in 0..steps {
            let (b0, b1) = (std::f64::consts::PI * k as f64 / steps as f64, std::f64::consts::PI * (k + 1) as f64 / steps as f64);
            let w = std::f64::consts::TAU * (b0.cos() - b1.cos()) / std::f64::consts::PI;
            ir += 0.5 * (gr[k] + gr[k + 1]) as f64 * w;
            it += 0.5 * (gt[k] + gt[k + 1]) as f64 * w;
        }
        assert!((ir - 0.5).abs() < 0.02 && (it - 0.5).abs() < 0.02, "{ir} {it}");
        // Reflection goes back toward the source, transmission forward.
        assert!(gr[GAMMA_STEPS - 1] > gr[0] && gt[0] > gt[GAMMA_STEPS - 1]);
        let (rho, tau) = lanes(SPRING_GRASS);
        let r550 = km_infinite_reflectance(rho[1] as f64, tau[1] as f64);
        assert!(r550 > 0.05 && r550 < 0.2, "{r550}");
        assert!(km_infinite_reflectance(rho[0] as f64, tau[0] as f64) < r550 * 0.5);
    }

    #[test]
    fn the_far_canopy_reads_as_a_green_meadow_with_a_hotspot() {
        let (rho, tau) = lanes(SPRING_GRASS);
        let lane = |i: usize, mu_s: f64, mu_v: f64, alpha: f64, beta: f64| slab_reflectance(rho[i] as f64, tau[i] as f64, 2.5, mu_s, mu_v, alpha, beta);
        // Sun 50 deg up, view 60 deg off the vertical, opposite azimuths (forward scattering side).
        let (ts, tv) = (40f64.to_radians(), 60f64.to_radians());
        let alpha_off = (ts.tan().powi(2) + tv.tan().powi(2) + 2.0 * ts.tan() * tv.tan()).sqrt();
        let beta_off = std::f64::consts::PI - (ts + tv);
        let r550 = lane(1, ts.cos(), tv.cos(), alpha_off, beta_off);
        let r680 = lane(0, ts.cos(), tv.cos(), alpha_off, beta_off);
        let r440 = lane(2, ts.cos(), tv.cos(), alpha_off, beta_off);
        // Meadow canopies: green ~0.05-0.15, red and blue a few per cent (field spectra).
        assert!(r550 > 0.04 && r550 < 0.2, "{r550}");
        assert!(r680 < 0.5 * r550 && r440 < 0.5 * r550, "{r680} {r550} {r440}");
        // The hotspot: looking along the beam (shadows hidden) is brighter than the same zenith
        // angles across it.
        let hot = lane(1, ts.cos(), ts.cos(), 0.0, std::f64::consts::PI);
        let across = lane(1, ts.cos(), ts.cos(), 2.0 * ts.tan() * std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::PI - 2.0 * ts * std::f64::consts::FRAC_1_SQRT_2);
        assert!(hot > across * 1.2, "hotspot {hot} vs {across}");
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
