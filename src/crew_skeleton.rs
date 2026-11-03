//! The casualties' skeleton (2026-11-02, the owner's choice): the rigged
//! `retro-lowpoly-psx-skeleton` (19 joints, 760 vertices; licence and author unconfirmed, see its
//! `PROVENANCE.txt`), cut into the crew's eleven anatomical pages by each triangle's dominant joint
//! and fitted inside the relaxed crew figure part by part. Each page is scaled uniformly into its
//! flesh page's box at `FIT` of that box and centred on it, then shrunk until every one of its
//! vertices lies inside the whole figure's surface (generalized winding number above one half), so
//! no bone pokes through the skin. An approximate visual fit, not anatomical registration: the
//! injury model does not read it, and like the skeleton before it, it is drawn only where a body
//! part's flesh is gone.
use super::*;
use crate::gltf_load::{CpuMesh, Vertex};
use anyhow::Context;

pub(super) const PSX_SKELETON_GLB: &str =
    "assets/flavor/Characters/retro-lowpoly-psx-skeleton-licence-unconfirmed/source/skeleton.glb";
/// A bone page is first fitted to this share of its flesh page's box.
const FIT: f32 = 0.78;
/// Each further shrink while any of its vertices is outside the figure's surface.
const SHRINK: f32 = 0.92;
const MAX_SHRINKS: usize = 16;
/// The relaxed crew figure's height and its pelvis/torso line (`classify_relaxed_mesh_point`).
const CREW_FIGURE_HEIGHT_M: f32 = 1.53;
const CREW_PELVIS_TOP_M: f32 = 0.835;
/// The skeleton's authored material: a dielectric (metallic 0) with a mid roughness.
const SKELETON_ROUGHNESS: f32 = 0.72;

/// A skeleton joint's anatomical page, side to be resolved by where its triangles lie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JointKind {
    Pelvis,
    Torso,
    Head,
    UpperArm,
    Forearm,
    Thigh,
    Shin,
}

fn joint_kind(name: &str) -> Option<JointKind> {
    let base = name.trim_end_matches(".L").trim_end_matches(".R");
    Some(match base {
        "Hip" => JointKind::Pelvis,
        "Torso" | "Neck" | "Shoulder" => JointKind::Torso,
        "Head" => JointKind::Head,
        "UpperArm" => JointKind::UpperArm,
        "LowerArm" | "Hand" => JointKind::Forearm,
        "UpperLeg" => JointKind::Thigh,
        "LowerLeg" | "Foot" => JointKind::Shin,
        _ => return None,
    })
}

/// The crew part of a joint whose triangles' mean x is `mean_x`: the crew frame puts the figure's
/// left at -x (`crew_physics::classify_relaxed_mesh_point`), whatever the skeleton's own names say.
fn crew_part(kind: JointKind, mean_x: f32) -> crew_physics::CrewBodyPart {
    use crew_physics::CrewBodyPart as P;
    let left = mean_x < 0.0;
    match kind {
        JointKind::Pelvis => P::Pelvis,
        JointKind::Torso => P::Torso,
        JointKind::Head => P::Head,
        JointKind::UpperArm => if left { P::LeftUpperArm } else { P::RightUpperArm },
        JointKind::Forearm => if left { P::LeftForearm } else { P::RightForearm },
        JointKind::Thigh => if left { P::LeftThigh } else { P::RightThigh },
        JointKind::Shin => if left { P::LeftShin } else { P::RightShin },
    }
}

/// The skeleton's bind-pose mesh and each triangle's crew part.
pub(super) fn load_psx_skeleton() -> anyhow::Result<(CpuMesh, Vec<crew_physics::CrewBodyPart>)> {
    let bytes = crate::resource::read(PSX_SKELETON_GLB)?;
    let (doc, buffers, images) = gltf::import_slice(&bytes).context("import the PSX skeleton")?;
    let skin = doc.skins().next().context("the skeleton has no skin")?;
    let kinds: Vec<Option<JointKind>> = skin.joints().map(|j| joint_kind(j.name().unwrap_or(""))).collect();
    let mesh = doc.meshes().next().context("the skeleton has no mesh")?;
    let primitive = mesh.primitives().next().context("the skeleton mesh has no primitive")?;
    let reader = primitive.reader(|b| Some(&buffers[b.index()]));
    let positions: Vec<[f32; 3]> = reader.read_positions().context("positions")?.collect();
    let normals: Vec<[f32; 3]> = reader.read_normals().context("normals")?.collect();
    let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).context("uvs")?.into_f32().collect();
    let joints: Vec<[u16; 4]> = reader.read_joints(0).context("joints")?.into_u16().collect();
    let weights: Vec<[f32; 4]> = reader.read_weights(0).context("weights")?.into_f32().collect();
    let indices: Vec<u32> = reader.read_indices().context("indices")?.into_u32().collect();
    anyhow::ensure!(positions.len() == normals.len() && positions.len() == uvs.len()
        && positions.len() == joints.len() && positions.len() == weights.len(), "skeleton attributes differ in length");
    let vertices: Vec<Vertex> = (0..positions.len()).map(|i| Vertex { pos: positions[i], normal: normals[i],
        uv: uvs[i], tangent: [1.0, 0.0, 0.0, 1.0] }).collect();
    // Each triangle's dominant joint by summed skin weight; each joint's side by its triangles.
    let triangle_joint: Vec<usize> = indices.chunks_exact(3).map(|t| {
        let mut sum = vec![0.0f32; kinds.len()];
        for &v in t {
            for k in 0..4 {
                let j = joints[v as usize][k] as usize;
                if j < sum.len() { sum[j] += weights[v as usize][k]; }
            }
        }
        (0..sum.len()).max_by(|a, b| sum[*a].total_cmp(&sum[*b])).unwrap_or(0)
    }).collect();
    let mut mean_x = vec![(0.0f32, 0u32); kinds.len()];
    for (t, &j) in indices.chunks_exact(3).zip(&triangle_joint) {
        let c = t.iter().map(|&v| positions[v as usize][0]).sum::<f32>() / 3.0;
        mean_x[j].0 += c;
        mean_x[j].1 += 1;
    }
    // The rig weights its pelvis to the torso: torso triangles below the crew's own pelvis line,
    // at the crew's height, are the pelvis (`crew_physics::classify_relaxed_mesh_point`).
    let height = positions.iter().map(|p| p[1]).fold(0.0f32, f32::max).max(1.0e-3);
    let parts = indices.chunks_exact(3).zip(&triangle_joint).map(|(t, &j)| {
        let kind = kinds[j].unwrap_or(JointKind::Torso);
        let y = t.iter().map(|&v| positions[v as usize][1]).sum::<f32>() / 3.0 * CREW_FIGURE_HEIGHT_M / height;
        if kind == JointKind::Torso && y < CREW_PELVIS_TOP_M {
            crew_physics::CrewBodyPart::Pelvis
        } else {
            crew_part(kind, mean_x[j].0 / mean_x[j].1.max(1) as f32)
        }
    }).collect();
    let texture = images.first().map(crate::gltf_load::to_rgba8);
    let (min, max) = bounds(vertices.iter().map(|v| Vec3::from(v.pos)));
    Ok((CpuMesh { vertices, indices, texture, metallic_roughness_texture: None, normal_texture: None,
        metallic_factor: 0.0, roughness_factor: SKELETON_ROUGHNESS, min, max }, parts))
}

fn bounds(points: impl Iterator<Item = Vec3>) -> (Vec3, Vec3) {
    points.fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

/// The solid angle of a triangle seen from the origin (Van Oosterom and Strackee 1983).
fn solid_angle(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let (la, lb, lc) = (a.length(), b.length(), c.length());
    let numerator = a.dot(b.cross(c));
    let denominator = la * lb * lc + a.dot(b) * lc + b.dot(c) * la + c.dot(a) * lb;
    2.0 * numerator.atan2(denominator)
}

/// Generalized winding number of `p` about a triangle soup: 1 inside a closed surface, 0
/// outside, and a smooth estimate for a figure built of several open pieces.
pub(super) fn winding_number(p: Vec3, mesh: &CpuMesh) -> f32 {
    mesh.indices.chunks_exact(3).map(|t| {
        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[t[i] as usize].pos) - p);
        solid_angle(a, b, c)
    }).sum::<f32>() / (4.0 * std::f32::consts::PI)
}

/// Inside a figure whatever way its triangles wind: |winding number| above one half.
pub(super) fn inside(p: Vec3, mesh: &CpuMesh) -> bool {
    winding_number(p, mesh).abs() > 0.5
}

/// What the fit did to one bone page, for receipts and tests.
#[derive(Clone, Copy, Debug)]
pub(super) struct BoneFit {
    pub scale: f32,
    pub shrinks: usize,
    /// Vertices set just inside the skin after the shrinks, and the largest such move.
    pub pulled: usize,
    pub worst_pull_m: f32,
    pub vertices_outside: usize,
}

/// The point of `mesh` nearest `p`, and that triangle's outward-ish normal (oriented away from
/// the inside as the winding number reads it).
fn nearest_surface(p: Vec3, mesh: &CpuMesh) -> (Vec3, Vec3) {
    let mut best = (f32::INFINITY, p, Vec3::Y);
    for t in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[t[i] as usize].pos));
        let q = closest_point_on_triangle(p, a, b, c);
        let d = q.distance_squared(p);
        if d < best.0 {
            best = (d, q, (b - a).cross(c - a).normalize_or_zero());
        }
    }
    let (_, q, mut n) = best;
    // Orient the normal outward: a step along it from the surface must leave the figure.
    if inside(q + n * 0.004, mesh) {
        n = -n;
    }
    (q, n)
}

/// Closest point on triangle abc to p (Ericson, Real-Time Collision Detection, 5.1.5).
pub(super) fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { return a + ab * (d1 / (d1 - d3)); }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { return a + ac * (d2 / (d2 - d6)); }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// A vertex still outside after the shrinks is set this far inside the skin, at the nearest point
/// (deeper steps where the first is not enough: a thin limb's far side is near).
const SKIN_INSETS_M: [f32; 4] = [0.004, 0.008, 0.015, 0.025];

/// The principal axis of a point set (power iteration on its covariance).
fn principal_axis(points: &[Vec3]) -> Vec3 {
    let mean = points.iter().fold(Vec3::ZERO, |a, b| a + *b) / points.len().max(1) as f32;
    let mut cov = glam::Mat3::ZERO;
    for p in points {
        let d = *p - mean;
        cov += glam::Mat3::from_cols(d * d.x, d * d.y, d * d.z);
    }
    let mut axis = Vec3::new(0.31, 0.93, 0.19);
    for _ in 0..32 {
        axis = (cov * axis).normalize_or_zero();
        if axis == Vec3::ZERO { return Vec3::Y; }
    }
    axis
}
/// How far outside the skin a bone vertex may lie before it counts as poking through.
const POKE_TOLERANCE_M: f32 = 0.003;
/// The smallest share of its fitted size a page may shrink to before stray vertices are pulled in.
const MIN_SHRINK: f32 = 0.7;

/// The skeleton cut into the crew's eleven pages and fitted inside `figure` (the relaxed crew
/// mesh, whose own partition gives each part's flesh). Each page is moved so its centroid sits at
/// its flesh page's vertex mean, scaled by the ratio of the two pages' longest extents, shrunk
/// about its centroid while any of its vertices is outside the figure (to `MIN_SHRINK` at most),
/// and a vertex still outside is then set `SKIN_INSET_M` inside the skin at the nearest point (a
/// measured visual correction, reported). A page with no skeleton triangles stays None.
pub(super) fn fitted_bone_pages(figure: &CpuMesh)
    -> anyhow::Result<([Option<CpuMesh>; crew_physics::CREW_BODY_PART_COUNT], [Option<BoneFit>; crew_physics::CREW_BODY_PART_COUNT])> {
    let (skeleton, parts) = load_psx_skeleton()?;
    let flesh = partition_relaxed_crew_mesh(figure);
    let mut fits = [None; crew_physics::CREW_BODY_PART_COUNT];
    let pages = std::array::from_fn(|part_index| {
        let part = crew_physics::CrewBodyPart::ALL[part_index];
        let mut remap = std::collections::HashMap::new();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for (t, &p) in skeleton.indices.chunks_exact(3).zip(&parts) {
            if p != part { continue; }
            for &v in t {
                let next = vertices.len() as u32;
                let i = *remap.entry(v).or_insert_with(|| { vertices.push(skeleton.vertices[v as usize]); next });
                indices.push(i);
            }
        }
        let flesh_page = flesh[part_index].as_ref()?;
        if indices.is_empty() { return None; }
        let flesh_points: Vec<Vec3> = flesh_page.indices.iter().map(|&i| Vec3::from(flesh_page.vertices[i as usize].pos)).collect();
        let target = flesh_points.iter().fold(Vec3::ZERO, |a, b| a + *b) / flesh_points.len() as f32;
        let bone_points: Vec<Vec3> = vertices.iter().map(|v: &Vertex| Vec3::from(v.pos)).collect();
        let centroid = bone_points.iter().fold(Vec3::ZERO, |a, b| a + *b) / bone_points.len() as f32;
        let (blo, bhi) = bounds(bone_points.iter().copied());
        let (flo, fhi) = bounds(flesh_points.iter().copied());
        // Length along each page's own long axis, after the turn below.
        let length = |pts: &[Vec3], axis: Vec3| { let (lo, hi) = pts.iter().fold((f32::INFINITY, f32::NEG_INFINITY),
            |(lo, hi), p| (lo.min(p.dot(axis)), hi.max(p.dot(axis)))); (hi - lo).max(1.0e-4) };
        let _ = (blo, bhi, flo, fhi);
        let fitted = length(&flesh_points, principal_axis(&flesh_points)) / length(&bone_points, principal_axis(&bone_points)) * FIT;
        // Turn the bone's long axis onto the flesh's (the crew's limbs hang at their own angles).
        let (bone_axis, flesh_axis) = (principal_axis(&bone_points), principal_axis(&flesh_points));
        let flesh_axis = if flesh_axis.dot(bone_axis) < 0.0 { -flesh_axis } else { flesh_axis };
        let turn = glam::Quat::from_rotation_arc(bone_axis, flesh_axis);
        let place = |scale: f32, p: Vec3| target + turn * (p - centroid) * scale;
        let mut scale = fitted;
        let mut shrinks = 0;
        let outside_at = |scale: f32| bone_points.iter().filter(|p| !inside(place(scale, **p), figure)).count();
        let mut outside = outside_at(scale);
        while outside > 0 && scale > fitted * MIN_SHRINK && shrinks < MAX_SHRINKS {
            scale = (scale * SHRINK).max(fitted * MIN_SHRINK);
            shrinks += 1;
            outside = outside_at(scale);
        }
        let mut pulled = 0;
        let mut worst_pull = 0.0f32;
        for (v, p) in vertices.iter_mut().zip(&bone_points) {
            let mut q = place(scale, *p);
            if !inside(q, figure) {
                let (surface, normal) = nearest_surface(q, figure);
                let moved = SKIN_INSETS_M.iter().map(|inset| surface - normal * *inset)
                    .find(|m| inside(*m, figure)).unwrap_or(surface - normal * SKIN_INSETS_M[0]);
                worst_pull = worst_pull.max(moved.distance(q));
                q = moved;
                pulled += 1;
            }
            v.pos = q.to_array();
        }
        // Poking through: outside the figure by more than the tolerance (a thin, open part of the
        // figure such as a hand has no inside to hold a vertex; one on its skin does not show).
        let still_outside = vertices.iter().filter(|v| { let p = Vec3::from(v.pos);
            !inside(p, figure) && nearest_surface(p, figure).0.distance(p) > POKE_TOLERANCE_M }).count();
        fits[part_index] = Some(BoneFit { scale, shrinks, pulled, worst_pull_m: worst_pull, vertices_outside: still_outside });
        let (min, max) = bounds(vertices.iter().map(|v| Vec3::from(v.pos)));
        Some(CpuMesh { vertices, indices, texture: skeleton.texture.clone(), metallic_roughness_texture: None,
            normal_texture: None, metallic_factor: 0.0, roughness_factor: SKELETON_ROUGHNESS, min, max })
    });
    Ok((pages, fits))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rig's nineteen joints all have an anatomical page; both sides of each limb are found.
    #[test]
    fn every_joint_has_a_page_and_both_sides_are_found() {
        for name in ["Torso", "Shoulder.L", "UpperArm.L", "LowerArm.L", "Hand.L", "Neck", "Head", "Shoulder.R",
            "UpperArm.R", "LowerArm.R", "Hand.R", "Hip.L", "UpperLeg.L", "LowerLeg.L", "Foot.L", "Hip.R",
            "UpperLeg.R", "LowerLeg.R", "Foot.R"] {
            assert!(joint_kind(name).is_some(), "{name}");
        }
        assert_eq!(crew_part(JointKind::UpperArm, -0.2), crew_physics::CrewBodyPart::LeftUpperArm);
        assert_eq!(crew_part(JointKind::Shin, 0.1), crew_physics::CrewBodyPart::RightShin);
    }

    #[test]
    fn the_winding_number_tells_inside_from_outside_a_closed_box() {
        let cube = crate::prims::box_mesh(Vec3::splat(-1.0), Vec3::splat(1.0), [9, 9, 9]);
        assert!((winding_number(Vec3::ZERO, &cube).abs() - 1.0).abs() < 1.0e-3);
        assert!(winding_number(Vec3::new(3.0, 0.2, 0.0), &cube).abs() < 1.0e-3);
        assert!(inside(Vec3::new(0.9, -0.9, 0.5), &cube) && !inside(Vec3::new(1.1, 0.0, 0.0), &cube));
    }

    /// With the assets present: every one of the eleven parts gets bones, and none pokes through
    /// the relaxed figure (`KESSEL_ASSET_ROOT`; skipped where the assets are absent).
    #[test]
    fn the_skeleton_fits_inside_the_crew_figure() {
        let Some(figure) = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "skeleton fit test") else { return };
        let (pages, fits) = match fitted_bone_pages(&figure) { Ok(p) => p, Err(_) => return };
        for part in crew_physics::CrewBodyPart::ALL {
            let fit = fits[part.index()].unwrap_or_else(|| panic!("{part:?} has no bones"));
            println!("{part:?}: {fit:?}");
            assert!(pages[part.index()].is_some());
            assert_eq!(fit.vertices_outside, 0, "{part:?} pokes through the skin: {fit:?}");
            assert!(fit.scale > 0.2, "{part:?} shrank to nothing: {fit:?}");
            assert!(fit.worst_pull_m < 0.06, "{part:?} needed a large correction: {fit:?}");
        }
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    #[test]
    #[ignore]
    fn print_winding_inside_each_part() {
        let Some(figure) = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "probe") else { return };
        let flesh = partition_relaxed_crew_mesh(&figure);
        for part in crew_physics::CrewBodyPart::ALL {
            let Some(page) = flesh[part.index()].as_ref() else { continue };
            let c = (page.min + page.max) * 0.5;
            let samples: Vec<String> = [-0.3f32, 0.0, 0.3].iter().flat_map(|dy| [-0.3f32, 0.0, 0.3].map(|dx| {
                let p = c + (page.max - page.min) * Vec3::new(dx, *dy, 0.0);
                format!("{:.2}", winding_number(p, &figure))
            })).collect();
            println!("{part:?} box {:?}..{:?} centre w {:.2} grid {:?}", page.min.to_array().map(|v| (v * 100.0).round() / 100.0),
                page.max.to_array().map(|v| (v * 100.0).round() / 100.0), winding_number(c, &figure), samples);
        }
    }
}

#[cfg(test)]
mod probe2 {
    use super::*;
    #[test]
    #[ignore]
    fn print_global_fit() {
        let Some(figure) = load_full_crew_mesh(CREW_RELAXED_GLBS[0], "probe") else { return };
        let (skeleton, parts) = load_psx_skeleton().unwrap();
        let s = CREW_FIGURE_HEIGHT_M / skeleton.max.y;
        let (flo, fhi) = (figure.min, figure.max);
        let fc = (flo + fhi) * 0.5;
        let sc = (skeleton.min + skeleton.max) * 0.5;
        println!("figure {:?}..{:?}, skeleton {:?}..{:?}, scale {s:.3}", flo.to_array(), fhi.to_array(), skeleton.min.to_array(), skeleton.max.to_array());
        for part in crew_physics::CrewBodyPart::ALL {
            let pts: Vec<Vec3> = skeleton.indices.chunks_exact(3).zip(&parts).filter(|(_, p)| **p == part)
                .flat_map(|(t, _)| t.iter().map(|&v| Vec3::from(skeleton.vertices[v as usize].pos)).collect::<Vec<_>>())
                .map(|p| Vec3::new(fc.x + (p.x - sc.x) * s, p.y * s, fc.z + (p.z - sc.z) * s)).collect();
            if pts.is_empty() { println!("{part:?}: none"); continue; }
            let c = pts.iter().fold(Vec3::ZERO, |a, b| a + *b) / pts.len() as f32;
            let out = pts.iter().filter(|p| !inside(**p, &figure)).count();
            println!("{part:?}: {} verts, centroid {:?} w {:.2}, outside {}", pts.len(), (c * 100.0).round().to_array(), winding_number(c, &figure), out);
        }
    }
}
