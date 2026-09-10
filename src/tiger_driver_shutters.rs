//! Reconstructed adjustable Driver visor, retaining the source shields and fixed side guides.
//! The first-hand Tiger250122 survey establishes opposed shield motion; this export fuses the
//! components. The 25mm travel and quarter-turn wheel gearing are explicit gameplay reconstruction,
//! not recovered historical dimensions. Original GLBs are never modified.
use super::tiger_driver_visor::{append_polygon, half_polygon};
use crate::gltf_load::{CpuMesh, Vertex};
use glam::{Mat4, Vec2, Vec3};

pub const SHIELD_ROOT: &str = "Hull_0164";
pub const WHEEL_ROOT: &str = "Hull_0213";
pub const CONTROL: Vec3 = Vec3::new(0.32512876, 1.5836368, 2.2917209);
pub const FRAMES: [usize; 3] = [9, 10, 11];
const TRAVEL: f32 = 0.025;
const GUIDE: Vec3 = Vec3::new(0.0, 1.0, -0.16);

pub struct Source {
    pub guides: CpuMesh,
    pub moving: [CpuMesh; 3],
}
#[derive(Default, Clone, Copy)]
pub struct Shutters {
    pub open: bool,
    pub phase: f32,
    pressed: bool,
}
impl Shutters {
    pub fn set(&mut self, open: bool, instant: bool) {
        self.open = open;
        if instant {
            self.phase = if open { 1.0 } else { 0.0 };
        }
    }
    pub fn input(&mut self, pressed: bool, can_use: bool) {
        if pressed && !self.pressed && can_use {
            self.open = !self.open;
        }
        self.pressed = pressed;
    }
    pub fn advance(&mut self, dt: f32) {
        self.phase = super::tiger_driver_visor::advance_lean(self.phase, self.open, dt);
    }
    pub fn openness(&self) -> f32 {
        let t = self.phase.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
    pub fn models(&self, hull: Mat4) -> [Mat4; 3] {
        let open = self.openness();
        let shift = GUIDE.normalize() * TRAVEL * open;
        [
            hull * Mat4::from_translation(shift),
            hull * Mat4::from_translation(-shift),
            hull * Mat4::from_translation(CONTROL)
                * Mat4::from_rotation_z(open * std::f32::consts::FRAC_PI_2)
                * Mat4::from_translation(-CONTROL),
        ]
    }
}

fn empty_like(mesh: &CpuMesh) -> CpuMesh {
    let mut m = mesh.clone();
    m.vertices.clear();
    m.indices.clear();
    m
}
fn finish(mesh: &mut CpuMesh) {
    mesh.min = Vec3::splat(f32::INFINITY);
    mesh.max = Vec3::splat(f32::NEG_INFINITY);
    for v in &mesh.vertices {
        let p = Vec3::from(v.pos);
        mesh.min = mesh.min.min(p);
        mesh.max = mesh.max.max(p);
    }
}
fn polygon_area_normal(poly: &[Vec3]) -> Vec3 {
    let o = poly[0];
    (1..poly.len() - 1)
        .map(|i| (poly[i] - o).cross(poly[i + 1] - o))
        .sum()
}
fn cap_triangles(mut poly: Vec<Vec3>, normal: Vec3) -> anyhow::Result<Vec<[Vec3; 3]>> {
    if polygon_area_normal(&poly).dot(normal) < 0.0 {
        poly.reverse();
    }
    let mut indices: Vec<usize> = (0..poly.len()).collect();
    let mut triangles = Vec::new();
    let side = |a: Vec3, b: Vec3, c: Vec3| (b - a).cross(c - a).dot(normal);
    while indices.len() > 3 {
        let mut found = false;
        for j in 0..indices.len() {
            let a = indices[(j + indices.len() - 1) % indices.len()];
            let b = indices[j];
            let c = indices[(j + 1) % indices.len()];
            let turn = side(poly[a], poly[b], poly[c]);
            if turn <= 1e-12 {
                if turn.abs() < 1e-12 {
                    indices.remove(j);
                    found = true;
                    break;
                }
                continue;
            }
            let interior = indices
                .iter()
                .copied()
                .filter(|&k| k != a && k != b && k != c)
                .any(|k| {
                    side(poly[a], poly[b], poly[k]) > 1e-12
                        && side(poly[b], poly[c], poly[k]) > 1e-12
                        && side(poly[c], poly[a], poly[k]) > 1e-12
                });
            if interior {
                continue;
            }
            triangles.push([poly[a], poly[b], poly[c]]);
            indices.remove(j);
            found = true;
            break;
        }
        anyhow::ensure!(
            found,
            "Driver shield section has a non-simple cap; review source instead of opening a seam"
        );
    }
    if indices.len() == 3 {
        triangles.push([poly[indices[0]], poly[indices[1]], poly[indices[2]]]);
    }
    Ok(triangles)
}

/// Clip source polygons and add oppositely oriented section caps. Every source surface is kept
/// exactly once; caps only close the new internal interface and share the original material page.
fn split_closed(
    mesh: &CpuMesh,
    normal: Vec3,
    distance: f32,
) -> anyhow::Result<(CpuMesh, CpuMesh, f32)> {
    let mut inside = empty_like(mesh);
    let mut outside = empty_like(mesh);
    let mut points: Vec<Vec3> = Vec::new();
    let mut edges = Vec::new();
    for indices in mesh.indices.chunks_exact(3) {
        let tri = indices
            .iter()
            .map(|&i| mesh.vertices[i as usize])
            .collect::<Vec<_>>();
        append_polygon(
            &mut inside.vertices,
            &mut inside.indices,
            &half_polygon(&tri, normal, distance, true),
        );
        append_polygon(
            &mut outside.vertices,
            &mut outside.indices,
            &half_polygon(&tri, normal, distance, false),
        );
        let p = tri.iter().map(|v| Vec3::from(v.pos)).collect::<Vec<_>>();
        let signed = p
            .iter()
            .map(|p| normal.dot(*p) - distance)
            .collect::<Vec<_>>();
        if !signed.iter().any(|&v| v < -1e-8) || !signed.iter().any(|&v| v > 1e-8) {
            continue;
        }
        let mut crossing = Vec::new();
        for i in 0..3 {
            let j = (i + 1) % 3;
            if (signed[i] <= 0.0) != (signed[j] <= 0.0) {
                crossing.push(p[i] + (p[j] - p[i]) * (signed[i] / (signed[i] - signed[j])));
            }
        }
        anyhow::ensure!(
            crossing.len() == 2,
            "Driver shield section intersection changed"
        );
        let ids = crossing
            .into_iter()
            .map(|p| {
                if let Some(i) = points.iter().position(|q| p.distance(*q) < 2e-6) {
                    i
                } else {
                    points.push(p);
                    points.len() - 1
                }
            })
            .collect::<Vec<_>>();
        if ids[0] != ids[1] {
            edges.push([ids[0], ids[1]]);
        }
    }
    let mut neighbors = vec![Vec::new(); points.len()];
    for [a, b] in edges {
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    anyhow::ensure!(
        neighbors.iter().all(|n| n.len() == 2),
        "Driver shield section is not a closed contour"
    );
    let mut visited = vec![false; points.len()];
    let unit = normal.normalize();
    let mut cap_area = 0.0;
    for start in 0..points.len() {
        if visited[start] {
            continue;
        }
        let mut polygon = Vec::new();
        let mut current = start;
        let mut previous = usize::MAX;
        while !visited[current] {
            visited[current] = true;
            polygon.push(points[current]);
            let next = *neighbors[current].iter().find(|&&p| p != previous).unwrap();
            previous = current;
            current = next;
        }
        anyhow::ensure!(
            current == start && polygon.len() >= 3,
            "Driver shield cap did not close"
        );
        for triangle in cap_triangles(polygon, unit)? {
            cap_area += (triangle[1] - triangle[0])
                .cross(triangle[2] - triangle[0])
                .length()
                * 0.5;
            for (part, n, flip) in [(&mut inside, unit, false), (&mut outside, -unit, true)] {
                let tangent = if n.x.abs() < 0.8 {
                    n.cross(Vec3::X).normalize()
                } else {
                    n.cross(Vec3::Y).normalize()
                };
                let mut triangle = triangle;
                if flip {
                    triangle.swap(1, 2);
                }
                let poly = triangle.map(|p| {
                    let mut v: Vertex = mesh.vertices[0];
                    v.pos = p.to_array();
                    v.normal = n.to_array();
                    // These newly exposed internal steel faces use a source-page sample, never
                    // a new luminous material or exterior texture. Original face UVs are untouched.
                    v.tangent = [tangent.x, tangent.y, tangent.z, 1.0];
                    v
                });
                append_polygon(&mut part.vertices, &mut part.indices, &poly);
            }
        }
    }
    finish(&mut inside);
    finish(&mut outside);
    Ok((inside, outside, cap_area))
}

pub fn reconstruct(shield: CpuMesh, wheel: CpuMesh) -> anyhow::Result<Source> {
    anyhow::ensure!(
        shield.indices.len() / 3 == 960
            && shield
                .min
                .abs_diff_eq(Vec3::new(0.159571, 1.566727, 2.495054), 0.0001)
            && shield
                .max
                .abs_diff_eq(Vec3::new(0.934076, 1.925981, 2.651722), 0.0001),
        "Driver outer visor source changed"
    );
    anyhow::ensure!(
        wheel.indices.len() / 3 == 16284
            && wheel
                .min
                .abs_diff_eq(Vec3::new(0.248968, 1.512523, 2.268559), 0.0001)
            && wheel
                .max
                .abs_diff_eq(Vec3::new(0.400426, 1.660466, 2.324418), 0.0001),
        "Driver visor wheel source changed"
    );
    // Guarded source front seams are X=0.338602/0.755044. The section is displaced only0.1mm
    // toward the guides to avoid cutting coplanar vertices; closed pose conserves the source.
    let (left, rest, _) = split_closed(&shield, Vec3::X, 0.3385)?;
    let (central, right, _) = split_closed(&rest, Vec3::X, 0.75515)?;
    let (lower, upper, _) = split_closed(
        &central,
        Vec3::new(0.0, 1.0, -0.16),
        1.731443 - 0.16 * 2.534714,
    )?;
    let mut guides = left;
    let base = guides.vertices.len() as u32;
    guides.vertices.extend(right.vertices);
    guides
        .indices
        .extend(right.indices.into_iter().map(|i| i + base));
    finish(&mut guides);
    Ok(Source {
        guides,
        moving: [upper, lower, wheel],
    })
}

/// Independent seated mouselook for the Driver. It never changes the registered eye; holding
/// lean gradually aims back through the aperture, then release restores the user's glance.
pub fn advance_look(look: Vec2, dx: f32, dy: f32) -> Vec2 {
    Vec2::new(
        (look.x + dx * 0.0022).clamp(-0.85, 0.85),
        (look.y - dy * 0.0022).clamp(-0.60, 0.60),
    )
}
pub fn view_forward(base: Vec3, look: Vec2, lean: f32) -> Vec3 {
    let t = lean.clamp(0.0, 1.0);
    let weight = 1.0 - t * t * (3.0 - 2.0 * t);
    let yaw = look.x * weight;
    let pitch = look.y * weight;
    let right = base.cross(Vec3::Y).normalize();
    (glam::Quat::from_axis_angle(Vec3::Y, -yaw) * glam::Quat::from_axis_angle(right, pitch) * base)
        .normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area(mesh: &CpuMesh) -> f32 {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let a = Vec3::from(mesh.vertices[t[0] as usize].pos);
                let b = Vec3::from(mesh.vertices[t[1] as usize].pos);
                let c = Vec3::from(mesh.vertices[t[2] as usize].pos);
                (b - a).cross(c - a).length() * 0.5
            })
            .sum()
    }
    fn first(mesh: &CpuMesh, o: Vec3, d: Vec3) -> Option<(f32, Vec3)> {
        mesh.indices
            .chunks_exact(3)
            .filter_map(|t| {
                let a = Vec3::from(mesh.vertices[t[0] as usize].pos);
                let e1 = Vec3::from(mesh.vertices[t[1] as usize].pos) - a;
                let e2 = Vec3::from(mesh.vertices[t[2] as usize].pos) - a;
                let p = d.cross(e2);
                let det = e1.dot(p);
                if det.abs() < 1e-10 {
                    return None;
                }
                let s = o - a;
                let u = s.dot(p) / det;
                let q = s.cross(e1);
                let v = d.dot(q) / det;
                let at = e2.dot(q) / det;
                (u >= -1e-6 && v >= -1e-6 && u + v <= 1.0 + 1e-6 && at > 0.00001)
                    .then_some((at, e1.cross(e2).normalize()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
    fn source() -> (CpuMesh, CpuMesh) {
        let mut parts = crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_hull.glb",
            &[SHIELD_ROOT, WHEEL_ROOT],
        )
        .unwrap();
        let wheel = parts.pop().unwrap();
        let shield = parts.pop().unwrap();
        (shield, wheel)
    }
    #[test]
    fn capped_source_partition_conserves_surfaces_and_closed_first_hits() {
        let (original, wheel) = source();
        let (left, rest, a) = split_closed(&original, Vec3::X, 0.3385).unwrap();
        let (central, right, b) = split_closed(&rest, Vec3::X, 0.75515).unwrap();
        let (lower, upper, c) = split_closed(
            &central,
            Vec3::new(0.0, 1.0, -0.16),
            1.731443 - 0.16 * 2.534714,
        )
        .unwrap();
        let reconstructed =
            area(&left) + area(&right) + area(&lower) + area(&upper) - 2.0 * (a + b + c);
        assert!(
            (reconstructed - area(&original)).abs() < 0.00003,
            "original steel faces must survive clipping"
        );
        assert!(
            a > 0.001 && b > 0.001 && c > 0.00001,
            "source section caps must exist"
        );
        let s = reconstruct(original.clone(), wheel.clone()).unwrap();
        assert_eq!(
            bytemuck::cast_slice::<Vertex, u8>(&s.moving[2].vertices),
            bytemuck::cast_slice::<Vertex, u8>(&wheel.vertices)
        );
        for part in [&s.guides, &s.moving[0], &s.moving[1]] {
            assert_eq!(
                part.texture.as_ref().map(|t| &t.rgba),
                original.texture.as_ref().map(|t| &t.rgba)
            );
            assert!(part.vertices.iter().all(|v| Vec3::from(v.pos).is_finite()));
        }
        for x in [0.24, 0.40, 0.535, 0.69, 0.84] {
            for y in [1.68, 1.73, 1.78, 1.84] {
                for (z, d) in [(2.30, Vec3::Z), (2.90, -Vec3::Z)] {
                    let o = Vec3::new(x, y, z);
                    let old = first(&original, o, d);
                    let new = [&s.guides, &s.moving[0], &s.moving[1]]
                        .into_iter()
                        .filter_map(|m| first(m, o, d))
                        .min_by(|a, b| a.0.total_cmp(&b.0));
                    assert_eq!(
                        old.is_some(),
                        new.is_some(),
                        "closed visor changed exterior silhouette"
                    );
                    if let (Some(a), Some(b)) = (old, new) {
                        assert!((a.0 - b.0).abs() < 0.00005);
                    }
                }
            }
        }
    }
    #[test]
    fn open_shutters_pass_refracted_field_and_source_contacts_keep_closed_steel() {
        let (original, wheel) = source();
        let s = reconstruct(original, wheel).unwrap();
        let (glass, _, _) = super::super::tiger_driver_visor::load_optics().unwrap();
        let mut fixed = crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_hull.glb",
            &[SHIELD_ROOT, WHEEL_ROOT, "Hull_0189_housing"],
        )
        .unwrap();
        fixed.truncate(1); // retained complete hull, excluding only the separately tested assemblies
        let sources = [
            (0, &fixed[0]),
            (0, &s.guides),
            (9, &s.moving[0]),
            (10, &s.moving[1]),
            (11, &s.moving[2]),
        ];
        let contacts = super::super::surface_fire::CabinFire::new(&sources);
        let mut state = Shutters::default();
        state.set(true, true);
        let mut open = [Mat4::IDENTITY; super::super::surface_fire::FRAMES];
        for (frame, m) in FRAMES.into_iter().zip(state.models(Mat4::IDENTITY)) {
            open[frame] = m;
        }
        let closed = [Mat4::IDENTITY; super::super::surface_fire::FRAMES];
        let forward = Vec3::new(0.0, 2_f32.to_radians().sin(), 2_f32.to_radians().cos());
        let right = Vec3::X;
        let up = forward.cross(right);
        let mut blocked_closed = 0;
        for y in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                let d = (forward + (right * x * (16.0 / 9.0) + up * y) * 7_f32.to_radians().tan())
                    .normalize();
                let mut p = super::super::tiger_driver_visor::DRIVER_LEAN_EYE;
                let mut ray = d;
                for interface in 0..2 {
                    let (t, mut n) =
                        first(&glass, p, ray).expect("both real glass interfaces remain");
                    assert!(
                        n.z.abs() > 0.98,
                        "the usable field must cross broad glass faces"
                    );
                    p += ray * t;
                    if n.dot(ray) > 0.0 {
                        n = -n;
                    }
                    let eta = if interface == 0 { 1.0 / 1.52 } else { 1.52 };
                    let cos = -n.dot(ray);
                    let k = 1.0 - eta * eta * (1.0 - cos * cos);
                    assert!(k >= 0.0);
                    ray = (eta * ray + n * (eta * cos - k.sqrt())).normalize();
                    p += ray * 0.00005;
                }
                assert!(
                    ray.distance(d) < 0.00001,
                    "plane-parallel glass preserves outgoing direction"
                );
                let end = p + ray * 0.6;
                assert!(
                    contacts.sweep(p, end, &open).is_none(),
                    "opened14-degree field must clear retained source steel at{x},{y}"
                );
                if let Some(hit) = contacts.sweep(p, end, &closed) {
                    assert!(
                        hit.frame == 9 || hit.frame == 10,
                        "closed-field blocker must remain actual source shield"
                    );
                    blocked_closed += 1;
                }
            }
        }
        assert!(
            blocked_closed >= 15,
            "opening must add real source visibility rather than only changingFOV"
        );
        let wheel_world = state.models(Mat4::IDENTITY)[2].transform_point3(CONTROL);
        assert!(wheel_world.abs_diff_eq(CONTROL, 1e-6));
        let center = (s.moving[0].min + s.moving[0].max) * 0.5;
        assert!(((open[9].transform_point3(center) - center).length() - TRAVEL).abs() < 1e-6);
    }
    #[test]
    fn handwheel_is_edge_triggered_independent_and_lean_release_restores_the_glance() {
        let mut s = Shutters::default();
        s.input(true, false);
        assert!(!s.open);
        s.input(true, true);
        assert!(!s.open, "walking onto a control while held must not toggle");
        s.input(false, true);
        s.input(true, true);
        assert!(s.open);
        s.advance(0.2);
        assert!((s.phase - 0.5).abs() < 1e-6);
        s.input(true, true);
        assert!(s.open, "held click must not chatter");
        s.input(false, true);
        s.input(true, true);
        assert!(!s.open);
        s.advance(1.0);
        assert_eq!(s.phase, 0.0);
        let base = -Vec3::Z;
        let glance = advance_look(Vec2::ZERO, 100.0, 75.0);
        let normal = view_forward(base, glance, 0.0);
        assert!(normal.x > 0.0 && normal.y < 0.0);
        assert!(view_forward(base, glance, 1.0).abs_diff_eq(base, 1e-6));
        assert!(view_forward(base, glance, 0.0).abs_diff_eq(normal, 1e-6));
        let look = advance_look(glance, 100000.0, -100000.0);
        assert_eq!(look, Vec2::new(0.85, 0.60));
        for open in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(
                super::super::tiger_driver_visor::inspection_fov(0.0, open),
                48_f32.to_radians()
            );
            assert!(
                (super::super::tiger_driver_visor::inspection_fov(1.0, open)
                    - (4.0 + 10.0 * open).to_radians())
                .abs()
                    < 1e-6
            );
        }
    }
}
