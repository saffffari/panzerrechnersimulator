//! Direct driver visor derived from the preserved source assembly, not a screen/secondary camera.
//!
//! Hull_0164 already contains the outer slit. The bake put only the inner glass face in the RTT
//! mesh and left a closed opaque backing in Hull_0189_housing. Partition only that holder, retain
//! its material/UVs, remove the volume occupied by its existing glass aperture, and register a
//! closed glass block so the ordinary dielectric solver can measure both interfaces.
use crate::gltf_load::{load_glb_partitioned_roots, CpuMesh, Vertex};
use glam::{Vec2, Vec3};

pub const DRIVER_EYE: Vec3 = Vec3::new(0.535, 1.733, 1.82);
pub const DRIVER_LOOK: Vec3 = Vec3::new(0.535, 1.733, 2.3859);
pub const DRIVER_LEAN_EYE: Vec3 = Vec3::new(0.535, 1.733, 2.32);
/// Accessibility relay objective, outside the front visor shield at the same aperture height.
/// This is a virtual optical pupil, never a crew/body position or an interaction origin.
pub const WIDE_OBJECTIVE: Vec3 = Vec3::new(0.535, 1.733, 2.74);
pub const WIDE_HORIZONTAL_FOV: f32 = std::f32::consts::FRAC_PI_2;

pub(super) fn wide_view_ready(lean: f32, openness: f32) -> bool {
    lean >= 0.92 && openness >= 0.98
}

pub(super) fn wide_camera(hull: glam::Mat4, aspect: f32) -> crate::camera::Camera {
    let mut camera = crate::camera::Camera::new(aspect);
    camera.pos = hull.transform_point3(WIDE_OBJECTIVE);
    camera.set_forward(hull.transform_vector3(Vec3::Z));
    camera.fov_y = 2.0 * ((WIDE_HORIZONTAL_FOV * 0.5).tan() / aspect.max(0.01)).atan();
    camera
}

/// Local seated movement, never a roof-exposed posture. The source-triangle audit retains
/// >=49 mm eye clearance over this 500 mm path at the default turret heading.
pub(super) fn advance_lean(phase: f32, requested: bool, dt: f32) -> f32 {
    let target = if requested { 1.0 } else { 0.0 };
    let phase = phase.clamp(0.0, 1.0);
    phase + (target - phase).clamp(-2.5 * dt.max(0.0), 2.5 * dt.max(0.0))
}

pub(super) fn seated_eye(phase: f32) -> Vec3 {
    let t = phase.clamp(0.0, 1.0);
    // Only depth changes. General vector lerp can round even identical X/Y endpoints, which
    // needlessly perturbs the registered narrow slit lane during this straight movement.
    Vec3::new(
        DRIVER_EYE.x,
        DRIVER_EYE.y,
        DRIVER_EYE.z + (DRIVER_LEAN_EYE.z - DRIVER_EYE.z) * t * t * (3.0 - 2.0 * t),
    )
}

/// Physical seated surround keeps its normal field while leaning. Once focused with shields
/// open, wide_camera supplies the deliberately wider virtual field through the slit mask.
pub(super) fn inspection_fov(_phase: f32, _openness: f32) -> f32 {
    48_f32.to_radians()
}

#[derive(Clone, Copy)]
struct Aperture {
    origin: Vec3,
    u: Vec3,
    v: Vec3,
    outward: Vec3,
    half_width: f32,
    half_height: f32,
    thickness: f32,
}

impl Aperture {
    fn from_pane(pane: &CpuMesh) -> anyhow::Result<Self> {
        anyhow::ensure!(
            pane.indices.len() == 6,
            "driver source pane changed; review its aperture"
        );
        let p = |index: usize| Vec3::from(pane.vertices[pane.indices[index] as usize].pos);
        let mut outward = (p(1) - p(0)).cross(p(2) - p(0)).normalize();
        if outward.z < 0.0 {
            outward = -outward;
        }
        let u = Vec3::X;
        let v = outward.cross(u).normalize();
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &pane.vertices {
            let p = Vec3::from(vertex.pos);
            let local = Vec3::new(p.dot(u), p.dot(v), p.dot(outward));
            lo = lo.min(local);
            hi = hi.max(local);
        }
        let center = (lo + hi) * 0.5;
        let aperture = Self {
            origin: u * center.x + v * center.y + outward * center.z,
            u,
            v,
            outward,
            half_width: (hi.x - lo.x) * 0.5,
            half_height: (hi.y - lo.y) * 0.5,
            // Measured normal distance to the source holder's rear broad face. This is geometry
            // registration, not a newly asserted historical laminate specification.
            thickness: 0.09730,
        };
        anyhow::ensure!(
            (0.12..0.14).contains(&aperture.half_width)
                && (0.023..0.028).contains(&aperture.half_height)
                && (aperture.origin - Vec3::new(0.546824, 1.743752, 2.383757)).length() < 0.002
                && outward.dot(Vec3::Z) > 0.98,
            "driver source glass moved or changed shape; refusing an unregistered holder cut"
        );
        Ok(aperture)
    }

    fn point(self, u: f32, v: f32, w: f32) -> Vec3 {
        // The source holder's inner and rear lower edges share y=1.71877. Its block extends
        // horizontally through the sloped plate; extruding along the face normal would raise
        // that rear edge by 15 mm and invent a smaller, sky-only exit aperture.
        self.origin + self.u * u + self.v * v + Vec3::Z * (w / self.outward.z)
    }

    fn planes(self) -> [(Vec3, f32); 6] {
        let center = self.point(0.0, 0.0, self.thickness * 0.5);
        let vertical_side = Vec3::Y / self.v.y;
        // Tiny numerical seam clearance belongs only to this holder aperture, never hull armor.
        [
            (self.u, self.half_width + 0.00005),
            (-self.u, self.half_width + 0.00005),
            (vertical_side, self.half_height + 0.00005),
            (-vertical_side, self.half_height + 0.00005),
            (self.outward, self.thickness * 0.5 + 0.0005),
            (-self.outward, self.thickness * 0.5 + 0.0005),
        ]
        .map(|(n, d)| (n, n.dot(center) + d))
    }
}

fn interpolate(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let pos = Vec3::from(a.pos).lerp(Vec3::from(b.pos), t);
    let normal = Vec3::from(a.normal)
        .lerp(Vec3::from(b.normal), t)
        .normalize_or_zero();
    let uv = Vec2::from(a.uv).lerp(Vec2::from(b.uv), t);
    let tangent = std::array::from_fn(|i| a.tangent[i] + (b.tangent[i] - a.tangent[i]) * t);
    Vertex {
        pos: pos.to_array(),
        normal: normal.to_array(),
        uv: uv.to_array(),
        tangent,
    }
}

pub(super) fn half_polygon(poly: &[Vertex], normal: Vec3, distance: f32, inside: bool) -> Vec<Vertex> {
    if poly.is_empty() {
        return Vec::new();
    }
    let signed = |v: Vertex| normal.dot(Vec3::from(v.pos)) - distance;
    let keep = |d: f32| if inside { d <= 0.0 } else { d >= 0.0 };
    let mut result = Vec::new();
    let mut a = *poly.last().unwrap();
    let mut da = signed(a);
    for &b in poly {
        let db = signed(b);
        if keep(da) != keep(db) {
            result.push(interpolate(a, b, (da / (da - db)).clamp(0.0, 1.0)));
        }
        if keep(db) {
            result.push(b);
        }
        a = b;
        da = db;
    }
    result
}

pub(super) fn append_polygon(vertices: &mut Vec<Vertex>, indices: &mut Vec<u32>, poly: &[Vertex]) {
    for i in 1..poly.len().saturating_sub(1) {
        let triangle = [poly[0], poly[i], poly[i + 1]];
        let a = Vec3::from(triangle[0].pos);
        let b = Vec3::from(triangle[1].pos);
        let c = Vec3::from(triangle[2].pos);
        if (b - a).cross(c - a).length_squared() < 1e-16 {
            continue;
        }
        let start = vertices.len() as u32;
        vertices.extend(triangle);
        indices.extend([start, start + 1, start + 2]);
    }
}

fn remove_glass_volume(holder: &mut CpuMesh, aperture: Aperture) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let planes = aperture.planes();
    for triangle in holder.indices.chunks_exact(3) {
        let mut remaining = triangle
            .iter()
            .map(|&i| holder.vertices[i as usize])
            .collect::<Vec<_>>();
        // Disjoint outside polygons conserve all source triangles outside the registered prism.
        // Only the final intersection with all six planes is removed.
        for &(normal, distance) in &planes {
            append_polygon(
                &mut vertices,
                &mut indices,
                &half_polygon(&remaining, normal, distance, false),
            );
            remaining = half_polygon(&remaining, normal, distance, true);
            if remaining.is_empty() {
                break;
            }
        }
    }
    holder.vertices = vertices;
    holder.indices = indices;
    // All decoded source material pages and interpolated UVs remain on this separate holder mesh.
    // It must not be merged into the hull's independently packed material atlas.
    refresh_bounds(holder);
}

fn refresh_bounds(mesh: &mut CpuMesh) {
    mesh.min = Vec3::splat(f32::INFINITY);
    mesh.max = Vec3::splat(f32::NEG_INFINITY);
    for vertex in &mesh.vertices {
        mesh.min = mesh.min.min(Vec3::from(vertex.pos));
        mesh.max = mesh.max.max(Vec3::from(vertex.pos));
    }
}

fn closed_glass(mut pane: CpuMesh, aperture: Aperture) -> CpuMesh {
    let x = aperture.half_width;
    let y = aperture.half_height;
    let z = aperture.thickness;
    let p = |u, v, w| aperture.point(u, v, w);
    let faces = [
        (
            [p(-x, -y, 0.0), p(x, -y, 0.0), p(x, y, 0.0), p(-x, y, 0.0)],
            -aperture.outward,
        ),
        (
            [p(-x, -y, z), p(x, -y, z), p(x, y, z), p(-x, y, z)],
            aperture.outward,
        ),
        (
            [p(-x, -y, 0.0), p(-x, y, 0.0), p(-x, y, z), p(-x, -y, z)],
            -aperture.u,
        ),
        (
            [p(x, -y, 0.0), p(x, y, 0.0), p(x, y, z), p(x, -y, z)],
            aperture.u,
        ),
        (
            [p(-x, -y, 0.0), p(x, -y, 0.0), p(x, -y, z), p(-x, -y, z)],
            -Vec3::Y,
        ),
        (
            [p(-x, y, 0.0), p(x, y, 0.0), p(x, y, z), p(-x, y, z)],
            Vec3::Y,
        ),
    ];
    let uv = pane.vertices[0].uv;
    pane.vertices.clear();
    pane.indices.clear();
    for (mut points, normal) in faces {
        if (points[1] - points[0])
            .cross(points[2] - points[0])
            .dot(normal)
            < 0.0
        {
            points.swap(1, 3);
        }
        let tangent = (points[1] - points[0]).normalize();
        let poly = points.map(|pos| Vertex {
            pos: pos.to_array(),
            normal: normal.to_array(),
            uv,
            tangent: [tangent.x, tangent.y, tangent.z, 1.0],
        });
        append_polygon(&mut pane.vertices, &mut pane.indices, &poly);
    }
    refresh_bounds(&mut pane);
    pane
}

/// Separate the direct visor and the roof-lid eyepiece from the old static screen page. Each
/// keeps an independent source material atlas; Hull_0300 must move with its hatch housing.
pub(super) fn load_optics() -> anyhow::Result<(CpuMesh, CpuMesh, CpuMesh)> {
    let mut pages = load_glb_partitioned_roots(
        "assets/tanks/tiger/baked/tiger_rtt_hull.glb",
        &["Hull_0189", "Hull_0300"],
    )?;
    let roof_pane = pages.pop().expect("driver roof eyepiece");
    let pane = pages.pop().expect("registered driver pane");
    let aperture = Aperture::from_pane(&pane)?;
    Ok((
        closed_glass(pane, aperture),
        pages.pop().expect("remaining hull eyepieces"),
        roof_pane,
    ))
}

/// Called only on the named source holder root, already split from the hull and its roof hatch.
pub(super) fn repair_holder(mut holder: CpuMesh) -> anyhow::Result<CpuMesh> {
    anyhow::ensure!(
        (holder.min - Vec3::new(0.40282, 1.66835, 2.33687)).length() < 0.002
            && (holder.max - Vec3::new(0.69083, 1.82384, 2.48752)).length() < 0.002,
        "driver holder source bounds changed; refusing an unregistered aperture repair"
    );
    let mut pages = load_glb_partitioned_roots(
        "assets/tanks/tiger/baked/tiger_rtt_hull.glb",
        &["Hull_0189"],
    )?;
    let aperture = Aperture::from_pane(&pages.pop().unwrap())?;
    let before = holder.indices.len() / 3;
    remove_glass_volume(&mut holder, aperture);
    anyhow::ensure!(
        !holder.indices.is_empty(),
        "driver holder repair removed the whole assembly"
    );
    println!("[driver-visor] preserved source holder; {before} triangles → {}; closed Schutzglas {:.1} mm; no hull armor cut",holder.indices.len()/3,aperture.thickness*1000.0);
    Ok(holder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_driver_view_is_ninety_degrees_horizontal_and_requires_open_shields() {
        let hull = glam::Mat4::from_rotation_translation(glam::Quat::from_rotation_y(0.7),
            Vec3::new(10.0, 3.0, -20.0));
        for aspect in [1.0, 4.0/3.0, 16.0/9.0, 21.0/9.0] {
            let camera = wide_camera(hull, aspect);
            let horizontal = 2.0 * ((camera.fov_y * 0.5).tan() * aspect).atan();
            assert!((horizontal - WIDE_HORIZONTAL_FOV).abs() < 0.000001);
            assert!(camera.pos.distance(hull.transform_point3(WIDE_OBJECTIVE)) < 0.000001);
            assert!(camera.forward().distance(hull.transform_vector3(Vec3::Z)) < 0.000001);
        }
        assert!(wide_view_ready(1.0, 1.0));
        assert!(!wide_view_ready(0.9, 1.0));
        assert!(!wide_view_ready(1.0, 0.5));
        assert!(!wide_view_ready(1.0, 0.0), "closed armor never grants a virtual exterior view");
        assert!(seated_eye(1.0).z < DRIVER_LOOK.z, "physical crew eye stays behind the glass");
        assert!(WIDE_OBJECTIVE.z > DRIVER_LOOK.z);
    }

    #[test]
    fn wide_objective_has_clear_horizon_across_ninety_degrees_in_source_hull() {
        let hull = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hull.glb")
            .expect("actual source hull for visor objective clearance");
        for degrees in [-45_f32, -30.0, -15.0, 0.0, 15.0, 30.0, 45.0] {
            let yaw = degrees.to_radians();
            let direction = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            assert!(ray(&hull, WIDE_OBJECTIVE, direction).is_empty(),
                "virtual objective remains behind source armor at {degrees} degrees");
        }
    }
    fn ray(mesh: &CpuMesh, o: Vec3, d: Vec3) -> Vec<f32> {
        let mut hits = Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            let a = Vec3::from(mesh.vertices[tri[0] as usize].pos);
            let b = Vec3::from(mesh.vertices[tri[1] as usize].pos);
            let c = Vec3::from(mesh.vertices[tri[2] as usize].pos);
            let e1 = b - a;
            let e2 = c - a;
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-8 {
                continue;
            }
            let t = o - a;
            let u = t.dot(p) / det;
            let q = t.cross(e1);
            let v = d.dot(q) / det;
            let at = e2.dot(q) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && at > 1e-5 {
                hits.push(at);
            }
        }
        hits.sort_by(f32::total_cmp);
        hits.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
        hits
    }

    #[test]
    fn holder_cut_preserves_surround_and_glass_is_a_closed_two_interface_solid() {
        let aperture = Aperture {
            origin: Vec3::ZERO,
            u: Vec3::X,
            v: Vec3::Y,
            outward: Vec3::Z,
            half_width: 0.13,
            half_height: 0.025,
            thickness: 0.0973,
        };
        let mut holder = crate::prims::box_mesh(
            Vec3::new(-0.15, -0.04, 0.09),
            Vec3::new(0.15, 0.04, 0.0973),
            [40, 40, 40],
        );
        let old_bounds = (holder.min, holder.max);
        let old_tex = holder.texture.clone();
        remove_glass_volume(&mut holder, aperture);
        assert!(ray(&holder, Vec3::new(0.01, 0.005, -1.0), Vec3::Z).is_empty());
        assert!(!ray(&holder, Vec3::new(0.145, 0.005, -1.0), Vec3::Z).is_empty());
        assert!(!ray(&holder, Vec3::new(0.01, 0.035, -1.0), Vec3::Z).is_empty());
        assert!(
            holder.min.abs_diff_eq(old_bounds.0, 1e-6)
                && holder.max.abs_diff_eq(old_bounds.1, 1e-6)
        );
        assert_eq!(holder.texture.as_ref().unwrap().rgba, old_tex.unwrap().rgba);
        let glass = closed_glass(
            crate::prims::box_mesh(Vec3::ZERO, Vec3::ONE, [190, 240, 230]),
            aperture,
        );
        let hits = ray(&glass, Vec3::new(0.01, 0.005, -1.0), Vec3::Z);
        assert_eq!(hits.len(), 2);
        assert!((hits[1] - hits[0] - 0.0973).abs() < 1e-5);
        assert_eq!(glass.indices.len(), 36);
        // Inward and outward views reach the same physical thickness; no single emissive face.
        let reverse = ray(&glass, Vec3::new(0.01, 0.005, 1.0), -Vec3::Z);
        assert_eq!(reverse.len(), 2);
        assert!((reverse[1] - reverse[0] - 0.0973).abs() < 1e-5);
    }

    #[test]
    fn supplied_pane_registration_builds_real_front_and_rear_surfaces() {
        let (glass, remaining, _) = load_optics().expect("preserved driver pane source");
        let hits = ray(&glass, DRIVER_EYE, Vec3::Z);
        assert_eq!(
            hits.len(),
            2,
            "source registration must put both interfaces in the driver's lane"
        );
        assert!((0.09..0.11).contains(&(hits[1] - hits[0])));
        assert!(
            ray(&remaining, DRIVER_EYE, Vec3::Z).is_empty(),
            "old opaque driver pane duplicated"
        );
        for phase in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let hits = ray(&glass, seated_eye(phase), Vec3::Z);
            assert_eq!(
                hits.len(),
                2,
                "lean must retain both physical glass interfaces"
            );
            assert!((0.09..0.11).contains(&(hits[1] - hits[0])));
        }
    }

    #[test]
    fn roof_eyepiece_keeps_its_material_and_leaves_the_static_hull() {
        fn assert_same_page(actual: &CpuMesh, source: &CpuMesh) {
            assert_eq!(actual.indices, source.indices);
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&actual.vertices),
                bytemuck::cast_slice::<Vertex, u8>(&source.vertices)
            );
            assert_eq!(actual.min, source.min);
            assert_eq!(actual.max, source.max);
            assert_eq!(actual.metallic_factor, source.metallic_factor);
            assert_eq!(actual.roughness_factor, source.roughness_factor);
            for (actual, source) in [
                (&actual.texture, &source.texture),
                (
                    &actual.metallic_roughness_texture,
                    &source.metallic_roughness_texture,
                ),
                (&actual.normal_texture, &source.normal_texture),
            ] {
                assert_eq!(actual.is_some(), source.is_some());
                if let (Some(actual), Some(source)) = (actual, source) {
                    assert_eq!((actual.width, actual.height), (source.width, source.height));
                    assert_eq!(actual.rgba, source.rgba);
                }
            }
        }
        let (_, remaining, roof_pane) = load_optics().expect("source eyepieces");
        let mut original = load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_rtt_hull.glb",
            &["Hull_0300"],
        )
        .unwrap();
        let original = original.pop().unwrap();
        assert_same_page(&roof_pane, &original);
        // Hull_0241 is the source's 24-triangle assembly, not one rectangle. Compare exact source
        // contents, not a guessed primitive count, to catch any static duplicate of Hull_0300.
        let original_static = load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_rtt_hull.glb",
            &["Hull_0241"],
        )
        .unwrap()
        .pop()
        .unwrap();
        assert_same_page(&remaining, &original_static);
        let mut hatches = super::super::tiger_hatches::RoofHatches::default();
        let center = (roof_pane.min + roof_pane.max) * 0.5;
        let closed = hatches.models(glam::Mat4::IDENTITY, glam::Mat4::IDENTITY)[0];
        assert!(closed.transform_point3(center).abs_diff_eq(center, 1e-6));
        hatches.set(true, false, true);
        let open = hatches.models(glam::Mat4::IDENTITY, glam::Mat4::IDENTITY)[0];
        assert!(
            open.transform_point3(center).y > center.y + 0.4,
            "the low eyepiece must rise with its actual housing"
        );
    }

    #[test]
    fn seated_lean_is_smooth_bounded_reversible_and_keeps_the_visor_lane() {
        let mut phase = 0.0;
        for _ in 0..24 {
            phase = advance_lean(phase, true, 1.0 / 120.0);
        }
        assert!((phase - 0.5).abs() < 1e-6);
        let midpoint = seated_eye(phase);
        assert!(midpoint.abs_diff_eq((DRIVER_EYE + DRIVER_LEAN_EYE) * 0.5, 1e-6));
        for _ in 0..24 {
            phase = advance_lean(phase, true, 1.0 / 120.0);
        }
        assert!(seated_eye(phase).abs_diff_eq(DRIVER_LEAN_EYE, 1e-6));
        assert_eq!(advance_lean(phase, true, 100.0), 1.0);
        let reverse = advance_lean(phase, false, 0.1);
        assert!((reverse - 0.75).abs() < 1e-6);
        assert_eq!(advance_lean(reverse, false, 100.0), 0.0);
        assert_eq!(advance_lean(reverse, true, -1.0), reverse);
        for i in 0..=100 {
            let eye = seated_eye(i as f32 / 100.0);
            assert!(eye.is_finite());
            assert_eq!(eye.x, DRIVER_EYE.x);
            assert_eq!(eye.y, DRIVER_EYE.y);
            assert!((DRIVER_EYE.z..=DRIVER_LEAN_EYE.z).contains(&eye.z));
            let fov = inspection_fov(i as f32 / 100.0,0.0);
            assert!(fov.is_finite() && (48_f32.to_radians()..=90_f32.to_radians()).contains(&fov));
            if i > 0 {
                assert!(fov >= inspection_fov((i - 1) as f32 / 100.0,0.0));
            }
        }
        assert_eq!(inspection_fov(-1.0,0.0), 48_f32.to_radians());
        assert_eq!(inspection_fov(2.0,0.0), 48_f32.to_radians());
        let travel = DRIVER_LEAN_EYE.z - DRIVER_EYE.z;
        // At 1% phase smoothstep covers 0.0298% of the chosen travel, versus 1% for a
        // linear movement. Keep this contract relative to travel so a longer lean is legal.
        let initial_fraction = (seated_eye(0.01).z - DRIVER_EYE.z) / travel;
        assert!((initial_fraction - 0.000298).abs() < 1e-6);
        assert!(initial_fraction < 0.01 * 0.04,
            "motion starts gently rather than at full velocity");
    }
}
