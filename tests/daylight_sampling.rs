//! Independent double-precision directional-PDF oracle, separate from live WGSL execution.
#[path = "../src/daylight_sampling.rs"]
mod daylight_sampling;
use daylight_sampling::Proposals;
use glam::{DVec3, Vec3};
use std::f64::consts::PI;
fn quad(z: f32, half: f32) -> Vec<[Vec3; 3]> {
    let p = [
        Vec3::new(-half, -half, z),
        Vec3::new(half, -half, z),
        Vec3::new(half, half, z),
        Vec3::new(-half, half, z),
    ];
    vec![[p[0], p[1], p[2]], [p[0], p[2], p[3]]]
}
fn triangles(p: &Proposals) -> Vec<[DVec3; 3]> {
    p.triangles[..p.meta[0] as usize]
        .iter()
        .map(|t| {
            [t.a_area, t.b_cumulative, t.c]
                .map(|v| DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64))
        })
        .collect()
}
fn area_pdf(table: &Proposals, origin: DVec3, d: DVec3) -> f64 {
    triangles(table)
        .iter()
        .filter_map(|p| {
            let e1 = p[1] - p[0];
            let e2 = p[2] - p[0];
            let h = d.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-12 {
                return None;
            }
            let delta = origin - p[0];
            let u = delta.dot(h) / det;
            let q = delta.cross(e1);
            let v = d.dot(q) / det;
            let t = e2.dot(q) / det;
            if u < 0.0 || v < 0.0 || u + v > 1.0 || t <= 0.0 {
                return None;
            }
            Some(t * t / (table.meta[1] as f64 * e1.cross(e2).normalize().dot(d).abs()))
        })
        .sum()
}
fn random(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((z ^ (z >> 31)) >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
}
fn sample(table: &Proposals, origin: DVec3, cosine: bool, state: &mut u64) -> (DVec3, f64) {
    let d = if random(state) < 0.5 {
        let target = random(state) * table.meta[1] as f64;
        let index = table.triangles[..table.meta[0] as usize]
            .iter()
            .position(|t| target < t.b_cumulative[3] as f64)
            .unwrap();
        let p = triangles(table)[index];
        let r = random(state).sqrt();
        let v = random(state);
        (p[0] * (1.0 - r) + p[1] * (r * (1.0 - v)) + p[2] * (r * v) - origin).normalize()
    } else {
        let u = random(state);
        let phi = 2.0 * PI * random(state);
        let z = if cosine { u.sqrt() } else { 1.0 - 2.0 * u };
        let r = (1.0 - z * z).sqrt();
        DVec3::new(r * phi.cos(), r * phi.sin(), z)
    };
    let base = if cosine {
        d.z.max(0.0) / PI
    } else {
        1.0 / (4.0 * PI)
    };
    (d, 0.5 * base + 0.5 * area_pdf(table, origin, d))
}
#[test]
fn pdf_sums_both_pane_faces_and_is_winding_independent() {
    let front = quad(1.0, 1.0);
    let back = quad(2.0, 1.0);
    let p = Proposals::from_triangles(front.into_iter().chain(back));
    let origin = DVec3::new(0.13, 0.21, 0.0);
    assert!((area_pdf(&p, origin, DVec3::Z) - (1.0 + 4.0) / 8.0).abs() < 1e-12);
    let reversed = Proposals::from_triangles(
        triangles(&p)
            .into_iter()
            .map(|t| [t[2].as_vec3(), t[1].as_vec3(), t[0].as_vec3()]),
    );
    assert_eq!(
        area_pdf(&p, origin, DVec3::Z),
        area_pdf(&reversed, origin, DVec3::Z)
    );
    assert_eq!(area_pdf(&p, origin, -DVec3::Z), 0.0);
}
#[test]
fn sphere_and_lambert_mixtures_preserve_constant_environment_energy() {
    let p = Proposals::from_triangles(quad(1.0, 1.0).into_iter().chain(quad(1.01, 1.0)));
    for cosine in [false, true] {
        let mut state = 193721;
        let mut sum = 0.0;
        for _ in 0..80_000 {
            let (d, pdf) = sample(&p, DVec3::new(0.13, 0.21, 0.0), cosine, &mut state);
            let integral = if cosine {
                d.z.max(0.0) / PI
            } else {
                1.0 / (4.0 * PI)
            };
            sum += integral / pdf;
        }
        let estimate = sum / 80_000.0;
        assert!(
            (estimate - 1.0).abs() < 0.012,
            "cosine={cosine}: energy={estimate}"
        );
    }
}
#[test]
fn occluded_half_opening_has_geometric_solid_angle_and_zero_blocked_energy() {
    let p = Proposals::from_triangles(quad(1.0, 0.1));
    // Rectangle solid angle, halved by an opaque blocker at x=0. This visibility factor must
    // be applied after the proposal, never folded into an invented emitter distribution.
    let expected = 2.0 * (0.01f64 / 1.02f64.sqrt()).atan();
    let mut state = 893791;
    let mut sum = 0.0;
    for _ in 0..80_000 {
        let (d, pdf) = sample(&p, DVec3::ZERO, false, &mut state);
        if d.z > 0.0 && d.x > 0.0 && d.x / d.z < 0.1 && (d.y / d.z).abs() < 0.1 {
            sum += 1.0 / pdf;
        }
    }
    let estimate = sum / 80_000.0;
    assert!(
        (estimate / expected - 1.0).abs() < 0.025,
        "{estimate} vs {expected}"
    );
}
#[test]
fn proposal_table_is_bounded_exact_and_rejects_invalid_geometry() {
    let invalid = [[Vec3::ZERO; 3], [Vec3::NAN; 3]];
    assert_eq!(Proposals::from_triangles(invalid).meta[0], 0.0);
    let source: Vec<_> = (0..100)
        .flat_map(|i| quad(1.0 + i as f32, 0.01 * (i + 1) as f32))
        .collect();
    let table = Proposals::from_triangles(source.clone());
    assert_eq!(table.meta[0], 64.0);
    let selected = triangles(&table);
    for t in selected {
        assert!(source.iter().any(|p| p.map(|v| v.as_dvec3()) == t));
    }
    assert_eq!(table.triangles[63].b_cumulative[3], table.meta[1]);
    let shader = include_str!("../shaders/daylight_sampling.wgsl");
    assert!(shader.contains("array<DaylightTriangle, 64>"));
    assert!(shader.contains("pdf += t * t /"));
    assert!(shader.contains("(1.0 - mixture) * directional_base_pdf"));
    assert!(!shader.contains("irradiance"));
    assert!(!shader.contains("solar_irradiance_angle"));
}
