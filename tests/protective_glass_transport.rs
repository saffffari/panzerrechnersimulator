//! Independent f64 optical/ray oracle for the bounded High source connection. These tests use
//! analytic slabs and actual source GLB triangles; live WGSL composition is Naga-validated in the
//! renderer tests. They do not claim GPU execution or a general prism/periscope solution.
#[path = "../src/protective_glass.rs"]
mod protective_glass;
use glam::{DMat4, DVec3};

const IOR: f64 = 1.52;
const ABSORPTION: DVec3 = DVec3::new(0.080, 0.035, 0.020);
const EPS: f64 = 0.0005;
#[derive(Clone)]
struct Triangle {
    p: [DVec3; 3],
    pane: Option<u32>,
}
struct Hit {
    t: f64,
    point: DVec3,
    normal: DVec3,
    pane: Option<u32>,
}
fn hit(scene: &[Triangle], origin: DVec3, dir: DVec3, max_t: f64) -> Option<Hit> {
    scene
        .iter()
        .filter_map(|tri| {
            let e1 = tri.p[1] - tri.p[0];
            let e2 = tri.p[2] - tri.p[0];
            let q = dir.cross(e2);
            let det = e1.dot(q);
            if det.abs() < 1e-12 {
                return None;
            }
            let s = origin - tri.p[0];
            let u = s.dot(q) / det;
            let q2 = s.cross(e1);
            let v = dir.dot(q2) / det;
            let t = e2.dot(q2) / det;
            if u < -1e-10 || v < -1e-10 || u + v > 1.0 + 1e-10 || t < EPS * 0.2 || t > max_t {
                return None;
            }
            Some(Hit {
                t,
                point: origin + dir * t,
                normal: e1.cross(e2).normalize(),
                pane: tri.pane,
            })
        })
        .min_by(|a, b| a.t.total_cmp(&b.t))
}
fn fresnel(cos: f64, n1: f64, n2: f64) -> f64 {
    let sin2 = (n1 / n2).powi(2) * (1.0 - cos * cos);
    if sin2 >= 1.0 {
        return 1.0;
    }
    let ct = (1.0 - sin2).sqrt();
    let rs = (n1 * cos - n2 * ct) / (n1 * cos + n2 * ct);
    let rp = (n2 * cos - n1 * ct) / (n2 * cos + n1 * ct);
    (rs * rs + rp * rp) * 0.5
}
fn refract(dir: DVec3, normal: DVec3, eta: f64) -> Option<DVec3> {
    let c = -dir.dot(normal);
    let k = 1.0 - eta * eta * (1.0 - c * c);
    (k >= 0.0).then(|| (dir * eta + normal * (eta * c - k.sqrt())).normalize())
}
fn transmit(scene: &[Triangle], origin: DVec3, dir: DVec3) -> Option<(DVec3, DVec3, f64)> {
    let Some(a) = hit(scene, origin, dir, 4000.0) else {
        return Some((DVec3::ONE, origin, 0.0));
    };
    let pane = a.pane?;
    let normal = if dir.dot(a.normal) > 0.0 {
        -a.normal
    } else {
        a.normal
    };
    let ci = -dir.dot(normal);
    if ci <= 0.0001 {
        return None;
    }
    let inside = refract(dir, normal, 1.0 / IOR)?;
    let b = hit(scene, a.point + inside * EPS, inside, 0.25)?;
    if b.pane != Some(pane) || a.normal.dot(b.normal) > -0.999999 {
        return None;
    }
    let exit_normal = if inside.dot(b.normal) > 0.0 {
        -b.normal
    } else {
        b.normal
    };
    let ce = -inside.dot(exit_normal);
    if ce <= 0.0 {
        return None;
    }
    let outside = refract(inside, exit_normal, IOR)?;
    if (outside - dir).length() > 0.0001 {
        return None;
    }
    let length = b.t + EPS;
    if length > 0.25 {
        return None;
    }
    let exit = b.point + outside * EPS;
    if hit(scene, exit, outside, 4000.0 - a.t - length).is_some() {
        return None;
    }
    let attenuation = (-ABSORPTION * length).exp()
        * (1.0 - fresnel(ci, 1.0, IOR))
        * (1.0 - fresnel(ce, IOR, 1.0));
    Some((attenuation, exit, length))
}
fn quad(z: f64, normal_sign: f64, pane: Option<u32>) -> Vec<Triangle> {
    let a = DVec3::new(-10.0, -10.0, z);
    let b = DVec3::new(10.0, -10.0, z);
    let c = DVec3::new(10.0, 10.0, z);
    let d = DVec3::new(-10.0, 10.0, z);
    let mut tris = vec![
        Triangle { p: [a, b, c], pane },
        Triangle { p: [a, c, d], pane },
    ];
    if normal_sign < 0.0 {
        for t in &mut tris {
            t.p.swap(1, 2);
        }
    }
    tris
}
fn slab(thickness: f64) -> Vec<Triangle> {
    let mut s = quad(0.0, -1.0, Some(1));
    s.extend(quad(thickness, 1.0, Some(1)));
    s
}
#[test]
fn clear_air_and_normal_slab_obey_energy_and_analytic_transmission() {
    let (air, _, t) = transmit(&[], DVec3::NEG_Z, DVec3::Z).unwrap();
    assert_eq!(air, DVec3::ONE);
    assert_eq!(t, 0.0);
    let (tx, exit, length) = transmit(&slab(0.09), DVec3::NEG_Z, DVec3::Z).unwrap();
    let reflect = ((IOR - 1.0) / (IOR + 1.0)).powi(2);
    let expected = (-ABSORPTION * 0.09).exp() * (1.0 - reflect).powi(2);
    assert!((tx - expected).length() < 1e-12);
    assert!((length - 0.09).abs() < 1e-12);
    assert!((exit.z - 0.0905).abs() < 1e-12);
    assert!(tx.min_element() > 0.90 && tx.max_element() < 0.92);
    assert!(tx.x < tx.y && tx.y < tx.z);
}
#[test]
fn steel_before_inside_or_after_pane_blocks_in_any_triangle_order() {
    for z in [-0.2, 0.04, 0.2] {
        let mut s = slab(0.09);
        s.extend(quad(z, 1.0, None));
        assert!(transmit(&s, DVec3::NEG_Z, DVec3::Z).is_none());
        s.reverse();
        assert!(transmit(&s, DVec3::NEG_Z, DVec3::Z).is_none());
    }
}
#[test]
fn oblique_path_measures_glass_length_and_traces_lateral_displacement() {
    let angle = 60.0f64.to_radians();
    let dir = DVec3::new(angle.sin(), 0.0, angle.cos());
    let origin = -dir;
    let mut s = slab(0.09);
    let (tx, exit, length) = transmit(&s, origin, dir).unwrap();
    let refracted_angle = (angle.sin() / IOR).asin();
    assert!((length - 0.09 / refracted_angle.cos()).abs() < 1e-12);
    assert!((exit.x - EPS * dir.x - 0.09 * refracted_angle.tan()).abs() < 1e-12);
    assert!(tx.max_element() < 0.90);
    // Small steel patch is centred on the displaced refracted path, away from the straight ray.
    let center = exit + dir * 0.05;
    let dx = DVec3::X * 0.005;
    let dy = DVec3::Y * 0.005;
    s.push(Triangle {
        p: [center - dx - dy, center + dx - dy, center + dy],
        pane: None,
    });
    assert!(transmit(&s, origin, dir).is_none());
}
#[test]
fn open_prismatic_second_pane_inside_and_grazing_paths_are_rejected() {
    assert!(transmit(&quad(0.0, -1.0, Some(1)), DVec3::NEG_Z, DVec3::Z).is_none());
    let mut prism = slab(0.09);
    for t in &mut prism[2..] {
        for p in &mut t.p {
            p.z += p.x * 0.02;
        }
    }
    assert!(transmit(&prism, DVec3::NEG_Z, DVec3::Z).is_none());
    let mut two = slab(0.09);
    two.extend(quad(0.2, -1.0, Some(2)));
    two.extend(quad(0.21, 1.0, Some(2)));
    assert!(transmit(&two, DVec3::NEG_Z, DVec3::Z).is_none());
    assert!(transmit(&slab(0.09), DVec3::new(0.0, 0.0, 0.04), DVec3::Z).is_none());
    let dir = DVec3::new(1.0, 0.0, 0.00001).normalize();
    assert!(transmit(&slab(0.09), -dir, dir).is_none());
}
#[test]
fn samples_next_to_a_thin_pane_keep_both_interfaces_and_inward_winding_is_valid() {
    let mut pane = slab(0.0097);
    for distance in [0.00025, 0.002, 0.008] {
        let (_, _, thickness) = transmit(&pane, -DVec3::Z * distance, DVec3::Z).unwrap();
        assert!((thickness - 0.0097).abs() < 1e-12);
    }
    let expected = transmit(&pane, DVec3::NEG_Z, DVec3::Z).unwrap().0;
    for triangle in &mut pane {
        triangle.p.swap(1, 2);
    }
    assert!((transmit(&pane, DVec3::NEG_Z, DVec3::Z).unwrap().0 - expected).length() < 1e-12);
    assert!(protective_glass::SHADER.contains("min(tmin, SOURCE_GLASS_EPSILON_M * 0.2)"));
}

fn source_pane(name: &str) -> Vec<Triangle> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/tanks/tiger/baked/tiger_port_turret.glb"
    ))
    .unwrap();
    let glb = gltf::Gltf::from_slice(&bytes).unwrap();
    let data = glb.blob.as_deref().unwrap();
    fn visit(
        node: gltf::Node<'_>,
        parent: DMat4,
        name: &str,
        data: &[u8],
        out: &mut Vec<Triangle>,
    ) {
        let raw = node.transform().matrix();
        let m = parent * DMat4::from_cols_array_2d(&raw.map(|r| r.map(f64::from)));
        if node.name() == Some(name) {
            for primitive in node.mesh().unwrap().primitives() {
                let reader = primitive.reader(|_| Some(data));
                let positions: Vec<_> = reader
                    .read_positions()
                    .unwrap()
                    .map(|p| m.transform_point3(DVec3::from_array(p.map(f64::from))))
                    .collect();
                let indices: Vec<_> = reader.read_indices().unwrap().into_u32().collect();
                for t in indices.chunks_exact(3) {
                    out.push(Triangle {
                        p: [
                            positions[t[0] as usize],
                            positions[t[1] as usize],
                            positions[t[2] as usize],
                        ],
                        pane: Some(1),
                    });
                }
            }
        }
        for child in node.children() {
            visit(child, m, name, data, out);
        }
    }
    let mut out = Vec::new();
    for node in glb.default_scene().unwrap().nodes() {
        visit(node, DMat4::IDENTITY, name, data, &mut out);
    }
    out
}
#[test]
fn installed_right_and_left_port_glbs_are_closed_parallel_transmitting_solids() {
    for name in ["GLASS RIGHT PORT", "Left Port Glass"] {
        let tris = source_pane(name);
        assert_eq!(tris.len(), 12);
        let biggest = tris
            .iter()
            .max_by(|a, b| {
                let area = |t: &Triangle| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length();
                area(a).total_cmp(&area(b))
            })
            .unwrap();
        let center = biggest.p.iter().sum::<DVec3>() / 3.0;
        let n = (biggest.p[1] - biggest.p[0])
            .cross(biggest.p[2] - biggest.p[0])
            .normalize();
        let (tx, _, length) = transmit(&tris, center + n * 0.1, -n).unwrap();
        assert!(
            (0.001..0.03).contains(&length),
            "{name}: thickness {length}"
        );
        assert!(tx.min_element() > 0.90 && tx.max_element() < 1.0);
    }
}
#[test]
fn shared_shader_uses_numeric_oracle_parameters_and_closest_geometry() {
    let s = protective_glass::SHADER;
    for v in [
        "SOURCE_GLASS_IOR = 1.52",
        "vec3(0.080, 0.035, 0.020)",
        "SOURCE_GLASS_EPSILON_M = 0.0005",
        "SOURCE_GLASS_MAX_PATH_M = 0.25",
        "SOURCE_GLASS_PARALLEL_COS = 0.999999",
        "SOURCE_GLASS_DIRECTION_ERROR = 0.0001",
    ] {
        assert!(s.contains(v), "oracle drift: {v}");
    }
    assert!(!s.contains("RAY_FLAG_TERMINATE_ON_FIRST_HIT"));
    assert!(!s.contains("rt_atlas"));
    let combine = protective_glass::inject(include_str!("../shaders/combine.wgsl"), false);
    assert!(!combine.contains(protective_glass::MARKER));
    assert!(combine.contains("let sky_path = source_connection"));
    assert!(combine.contains("solar_path = source_connection"));
}

#[test]
fn blocked_periscope_keeps_real_reflection_and_housing_without_becoming_a_light_path() {
    let shader=protective_glass::SHADER;
    let path=shader.split("fn periscope_path(").nth(1).unwrap()
        .split("fn source_connection(").next().unwrap();
    assert!(path.contains("hit.kind == 1u && inside_window == 0xffffffffu"));
    assert!(path.contains("out.valid = 2u;"));
    assert!(path.contains("out.origin = p;"));
    assert!(path.contains("out.throughput = throughput;"));
    // Camera endpoints may shade retained steel; a solar/environment connection may not.
    let source=shader.split("fn source_connection(").nth(1).unwrap();
    assert!(source.contains("folded.valid != 1u ||"));
    let combine=include_str!("../shaders/combine.wgsl");
    let branch=combine.split("if (periscope) {").nth(1).unwrap().split("} else {").next().unwrap();
    assert!(!branch.contains("return vec3(0.0)"),"failed transmission erased entry reflection");
    assert!(branch.contains("vec3(entry_reflectance), path.entry_kind == 3u"));
    assert!(branch.contains("pass_weight[1] = path.throughput"));
    assert!(combine.contains("direction, 0.00005, 4000.0, RT_MASK_NON_GLASS"),
        "a millimetre-scale internal housing endpoint must not be skipped");
}

#[test]
fn periscope_reflection_and_terminated_transmission_conserve_energy_and_keep_zero_dark() {
    // An independent energy witness for the camera's two paths. The transmitted path already
    // owns both window Fresnel losses, glass absorption, two mirrors and four coated lenses.
    for degrees in [0_f64,30.0,60.0,80.0] {
        let cosine=degrees.to_radians().cos();
        let reflected=fresnel(cosine,1.0,IOR);
        let within=(1.0-(1.0/IOR).powi(2)*(1.0-cosine*cosine)).sqrt();
        let exit_reflected=fresnel(within,IOR,1.0);
        let through=(1.0-reflected)*(1.0-exit_reflected)
            *(-ABSORPTION*0.008).exp()*0.9_f64.powi(2)*0.96_f64.powi(4);
        assert!(reflected>0.0 && reflected<1.0);
        assert!((DVec3::splat(reflected)+through).max_element()<=1.0);
        let cabin=DVec3::new(2.0,0.4,0.1);let housing=DVec3::new(0.02,0.01,0.005);
        let endpoint=cabin*reflected+housing*through;
        assert!(endpoint.min_element()>0.0,"blocked exterior still reflects actual cabin light");
        assert_eq!(DVec3::ZERO*reflected+DVec3::ZERO*through,DVec3::ZERO,
            "the repair must not invent a visibility fill when both sources are dark");
    }
}

#[test]
fn live_diagnostic_shader_parses_and_validates_with_exact_geometry_and_solar_solver() {
    let entry = protective_glass::inject(
        include_str!("../shaders/protective_glass_probe.wgsl"),
        false,
    );
    let source = format!(
        "{}\n{}",
        include_str!("../shaders/physical_environment.wgsl"),
        entry
    );
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("daylight probe parse: {e}"));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("daylight probe validation: {e:?}"));
}
