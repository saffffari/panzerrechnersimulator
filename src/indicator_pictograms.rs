//! Owner-requested graphical labels for the gameplay indicators. These are authored symbols,
//! not recovered historical markings. Actual thin metal tags join the EXISTING plinth edges;
//! the neck supports a tag where the cupola ring ends above it. No light or HUD surface is added.
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::lamp_mounts::{self as mounts, Mount, MountFrame};
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Reload,
    ChamberedRound,
    Fire,
    Repair,
    Station,
    Ignition,
    Commander,
    Gunner,
    Loader,
    Driver,
    Radio,
}
pub type Stroke = ([f32; 2], [f32; 2]);

/// Reusable painted linework in normalized [-1,1], X right / Y up. Roles are binoculars,
/// aiming reticle, round with loading arrow, steering wheel and radio headset respectively.
pub fn strokes(glyph: Glyph) -> Vec<Stroke> {
    fn path(out: &mut Vec<Stroke>, p: &[[f32; 2]]) {
        out.extend(p.windows(2).map(|w| (w[0], w[1])));
    }
    fn ring(out: &mut Vec<Stroke>, center: [f32; 2], radius: f32, n: usize) {
        let p: Vec<_> = (0..=n)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / n as f32;
                [center[0] + a.cos() * radius, center[1] + a.sin() * radius]
            })
            .collect();
        path(out, &p);
    }
    let mut s = Vec::new();
    match glyph {
        Glyph::ChamberedRound => {
            path(
                &mut s,
                &[
                    [-0.30, -0.76],
                    [-0.30, 0.28],
                    [0., 0.76],
                    [0.30, 0.28],
                    [0.30, -0.76],
                    [-0.30, -0.76],
                ],
            );
            path(&mut s, &[[-0.30, -0.47], [0.30, -0.47]]);
        }
        Glyph::Reload | Glyph::Loader => {
            path(
                &mut s,
                &[
                    [-0.75, -0.22],
                    [-0.10, -0.22],
                    [0.16, 0.],
                    [-0.10, 0.22],
                    [-0.75, 0.22],
                    [-0.75, -0.22],
                ],
            );
            path(
                &mut s,
                &[[0.18, -0.58], [0.68, -0.58], [0.68, 0.58], [0.18, 0.58]],
            );
            path(&mut s, &[[-0.58, -0.66], [-0.04, -0.66], [-0.20, -0.48]]);
            path(&mut s, &[[-0.04, -0.66], [-0.20, -0.82]]);
        }
        Glyph::Fire => {
            path(
                &mut s,
                &[
                    [-0.10, 0.80],
                    [-0.24, 0.24],
                    [-0.54, 0.48],
                    [-0.60, -0.30],
                    [-0.32, -0.70],
                    [0.30, -0.70],
                    [0.60, -0.22],
                    [0.44, 0.30],
                    [0.12, 0.02],
                    [-0.10, 0.80],
                ],
            );
            path(&mut s, &[[-0.18, -0.47], [0., -0.10], [0.18, -0.47]]);
        }
        Glyph::Repair => {
            path(
                &mut s,
                &[
                    [-0.64, -0.67],
                    [0.20, 0.17],
                    [0.06, 0.42],
                    [0.14, 0.73],
                    [0.43, 0.77],
                    [0.30, 0.44],
                    [0.48, 0.27],
                    [0.78, 0.43],
                    [0.75, 0.10],
                    [0.43, 0.04],
                    [-0.40, -0.76],
                    [-0.64, -0.67],
                ],
            );
        }
        Glyph::Station => {
            ring(&mut s, [0., 0.45], 0.25, 8);
            path(
                &mut s,
                &[
                    [-0.48, -0.70],
                    [-0.48, -0.18],
                    [-0.22, 0.02],
                    [0.22, 0.02],
                    [0.48, -0.18],
                    [0.48, -0.70],
                ],
            );
            path(&mut s, &[[-0.48, -0.70], [0.48, -0.70]]);
        }
        Glyph::Ignition => {
            path(
                &mut s,
                &[
                    [0.14, 0.80],
                    [-0.50, -0.08],
                    [0.02, -0.08],
                    [-0.12, -0.80],
                    [0.52, 0.16],
                    [0.02, 0.16],
                    [0.14, 0.80],
                ],
            );
        }
        Glyph::Commander => {
            path(
                &mut s,
                &[
                    [-0.72, -0.52],
                    [-0.72, 0.40],
                    [-0.26, 0.40],
                    [-0.26, -0.52],
                    [-0.72, -0.52],
                ],
            );
            path(
                &mut s,
                &[
                    [0.26, -0.52],
                    [0.26, 0.40],
                    [0.72, 0.40],
                    [0.72, -0.52],
                    [0.26, -0.52],
                ],
            );
            path(&mut s, &[[-0.26, 0.04], [0.26, 0.04]]);
            path(
                &mut s,
                &[[-0.63, 0.40], [-0.63, 0.68], [-0.35, 0.68], [-0.35, 0.40]],
            );
            path(
                &mut s,
                &[[0.35, 0.40], [0.35, 0.68], [0.63, 0.68], [0.63, 0.40]],
            );
        }
        Glyph::Gunner => {
            ring(&mut s, [0., 0.], 0.50, 12);
            path(&mut s, &[[-0.80, 0.], [0.80, 0.]]);
            path(&mut s, &[[0., -0.80], [0., 0.80]]);
        }
        Glyph::Driver => {
            ring(&mut s, [0., 0.], 0.72, 12);
            path(&mut s, &[[-0.62, 0.36], [0., 0.], [0.62, 0.36]]);
            path(&mut s, &[[0., 0.], [0., -0.72]]);
        }
        Glyph::Radio => {
            path(
                &mut s,
                &[
                    [-0.64, -0.35],
                    [-0.64, 0.25],
                    [-0.43, 0.62],
                    [0., 0.76],
                    [0.43, 0.62],
                    [0.64, 0.25],
                    [0.64, -0.35],
                ],
            );
            path(
                &mut s,
                &[[-0.64, 0.10], [-0.38, 0.10], [-0.38, -0.40], [-0.64, -0.40]],
            );
            path(
                &mut s,
                &[[0.64, 0.10], [0.38, 0.10], [0.38, -0.40], [0.64, -0.40]],
            );
            path(&mut s, &[[0.64, -0.40], [0.40, -0.68], [0.04, -0.68]]);
        }
    }
    s
}

/// Finite-width painted strokes on local XY, normal +Z, centred at the origin. `size_m` is
/// the full allocated square. The caller places it on their physical panel and keeps emission 0.
pub fn glyph_mesh(glyph: Glyph, size_m: f32, ink: [u8; 3]) -> CpuMesh {
    assert!(size_m.is_finite() && size_m > 0.);
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (a, b) in strokes(glyph) {
        let a = Vec2::from_array(a) * size_m * 0.5;
        let b = Vec2::from_array(b) * size_m * 0.5;
        let d = (b - a).normalize();
        let side = Vec2::new(-d.y, d.x) * size_m * 0.045;
        let corners = [a - side, b - side, b + side, a + side];
        let base = vertices.len() as u32;
        for p in corners {
            let p = p.extend(0.);
            min = min.min(p);
            max = max.max(p);
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: [0., 0., 1.],
                uv: [0.5, 0.5],
                tangent: [1., 0., 0., 1.],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    CpuMesh::basic(
        vertices,
        indices,
        Some(crate::prims::solid_color(ink)),
        min,
        max,
    )
}

pub struct MountedPictogram {
    pub name: &'static str,
    pub frame: MountFrame,
    /// Frame-local; compose with hull_model / turret_model / visual_gun_model each frame.
    pub model: Mat4,
    pub mesh: CpuMesh,
}

#[derive(Clone, Copy)]
struct Label {
    name: &'static str,
    glyph: Glyph,
    mount: Mount,
    along: f32,
    lamp_half: f32,
    side: bool,
}
fn labels() -> Vec<Label> {
    use crate::interior_rig as rig;
    let mut out = Vec::new();
    for (station, pair) in mounts::SIGHT.into_iter().enumerate() {
        for (i, (mount, along)) in pair.into_iter().enumerate() {
            out.push(Label {
                name: [
                    ["gunner.loading", "gunner.round"],
                    ["commander.loading", "commander.round"],
                ][station][i],
                glyph: if i == 0 {
                    Glyph::Reload
                } else {
                    Glyph::ChamberedRound
                },
                mount,
                along,
                lamp_half: rig::SIGHT_LOAD_FIXTURES[0].half_size_m,
                side: false,
            });
        }
    }
    for (i, (mount, along)) in mounts::ANNUNCIATORS.into_iter().enumerate() {
        // The bottom station lamp borders an outward source bracket. Its tag needs a
        // separately surveyed bracket; do not cover that source geometry with this plate.
        if i == 3 {
            continue;
        }
        out.push(Label {
            name: [
                "annunciator.fire",
                "annunciator.breech",
                "annunciator.repair",
                "annunciator.station",
            ][i],
            glyph: [Glyph::Fire, Glyph::Reload, Glyph::Repair, Glyph::Station][i],
            mount,
            along,
            lamp_half: rig::ANNUNCIATOR_FIXTURES[i].half_size_m,
            side: true,
        });
    }
    for (i, mount) in mounts::BREECH.into_iter().enumerate() {
        // The left guard ends just below its lamp, beside another source part.
        if i == 1 {
            continue;
        }
        out.push(Label {
            name: ["breech.right", "breech.left"][i],
            glyph: Glyph::Reload,
            mount,
            along: 0.,
            lamp_half: rig::BREECH_FIXTURES[0].half_size_m,
            side: false,
        });
    }
    // The driver's sloped instrument bezel protrudes through the proposed lower tag.
    // Keep its lamp untouched; the reusable ignition glyph is ready for a surveyed bracket.
    out
}

const TAG_HALF: f32 = 0.009;
const NECK_HALF: f32 = 0.0025;
const TAG_GAP: f32 = 0.004;
const BACK: f32 = 0.0015;
const FRONT: f32 = 0.0021;
const INK_Z: f32 = 0.0022;
const TAG_COLOR: [u8; 3] = [40, 43, 39];
const INK_COLOR: [u8; 3] = [220, 211, 183];

fn tag_mesh(label: Label) -> CpuMesh {
    let edge = label.lamp_half * mounts::PLINTH_FOOTPRINT;
    let offset = edge + TAG_GAP + TAG_HALF;
    let center = if label.side {
        Vec3::new(offset, 0., 0.)
    } else {
        Vec3::new(0., -offset, 0.)
    };
    let mut pieces = vec![crate::prims::box_mesh(
        center + Vec3::new(-TAG_HALF, -TAG_HALF, BACK),
        center + Vec3::new(TAG_HALF, TAG_HALF, FRONT),
        TAG_COLOR,
    )];
    // The neck's top/left edge touches the existing plinth's lower/right side, with its complete
    // depth inside that side. It is a real continuous supporting tag, including over open air.
    let (lo, hi) = if label.side {
        (
            Vec3::new(edge, -NECK_HALF, BACK),
            Vec3::new(edge + TAG_GAP, NECK_HALF, FRONT),
        )
    } else {
        (
            Vec3::new(-NECK_HALF, -edge - TAG_GAP, BACK),
            Vec3::new(NECK_HALF, -edge, FRONT),
        )
    };
    pieces.push(crate::prims::box_mesh(lo, hi, TAG_COLOR));
    let glyph = glyph_mesh(label.glyph, TAG_HALF * 1.8, INK_COLOR)
        .transformed(Mat4::from_translation(center + Vec3::Z * INK_Z));
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (is_ink, mesh) in pieces
        .into_iter()
        .map(|m| (false, m))
        .chain(std::iter::once((true, glyph)))
    {
        let base = vertices.len() as u32;
        min = min.min(mesh.min);
        max = max.max(mesh.max);
        vertices.extend(mesh.vertices.into_iter().map(|mut v| {
            v.uv = [if is_ink { 0.75 } else { 0.25 }, 0.5];
            v
        }));
        indices.extend(mesh.indices.into_iter().map(|i| i + base));
    }
    CpuMesh::basic(
        vertices,
        indices,
        Some(CpuImage::new(
            2,
            1,
            vec![
                TAG_COLOR[0],
                TAG_COLOR[1],
                TAG_COLOR[2],
                255,
                INK_COLOR[0],
                INK_COLOR[1],
                INK_COLOR[2],
                255,
            ],
        )),
        min,
        max,
    )
}

/// Eight non-emissive source-mounted labels. No source mesh is reloaded at runtime; the
/// private-asset regression below verifies the existing attachment footprints and clearance.
pub fn lamp_plates() -> Vec<MountedPictogram> {
    labels()
        .into_iter()
        .map(|label| MountedPictogram {
            name: label.name,
            frame: label.mount.frame,
            model: label.mount.label_frame(label.along),
            mesh: tag_mesh(label),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_symbol_has_bounded_finite_front_facing_ink_and_distinct_roles() {
        let glyphs = [
            Glyph::Reload,
            Glyph::ChamberedRound,
            Glyph::Fire,
            Glyph::Repair,
            Glyph::Station,
            Glyph::Ignition,
            Glyph::Commander,
            Glyph::Gunner,
            Glyph::Loader,
            Glyph::Driver,
            Glyph::Radio,
        ];
        for glyph in glyphs {
            let mesh = glyph_mesh(glyph, 0.03, INK_COLOR);
            assert!(!mesh.indices.is_empty());
            for v in &mesh.vertices {
                let p = Vec3::from_array(v.pos);
                assert!(p.is_finite());
                assert!(p.x.abs() <= 0.015 && p.y.abs() <= 0.015 && p.z == 0.);
            }
            for tri in mesh.indices.chunks_exact(3) {
                let [a, b, c] =
                    [0, 1, 2].map(|i| Vec3::from_array(mesh.vertices[tri[i] as usize].pos));
                assert!(
                    (b - a).cross(c - a).z > 0.,
                    "{glyph:?}: ink must face the viewer"
                );
            }
        }
        let roles = [
            Glyph::Commander,
            Glyph::Gunner,
            Glyph::Loader,
            Glyph::Driver,
            Glyph::Radio,
        ];
        for i in 0..roles.len() {
            for j in i + 1..roles.len() {
                assert_ne!(strokes(roles[i]), strokes(roles[j]));
            }
        }
    }
    #[test]
    fn tags_join_supported_source_plinths_and_leave_lamps_uncovered() {
        let path = |frame| match frame {
            MountFrame::Hull => "assets/tanks/tiger/baked/tiger_hull.glb",
            MountFrame::Turret => "assets/tanks/tiger/baked/tiger_turret.glb",
            MountFrame::Gun => "assets/tanks/tiger/baked/tiger_gun.glb",
        };
        if !crate::resource::exists(path(MountFrame::Turret)) {
            return;
        }
        let sources = [MountFrame::Hull, MountFrame::Turret, MountFrame::Gun]
            .map(|frame| (frame, crate::gltf_load::load_glb(path(frame)).unwrap()));
        for label in labels() {
            let mesh = &sources
                .iter()
                .find(|(f, _)| *f == label.mount.frame)
                .unwrap()
                .1;
            let model = label.mount.label_frame(label.along);
            let n = model.z_axis.truncate();
            assert!(
                mounts::flat_patch(
                    mesh,
                    model.w_axis.truncate(),
                    n,
                    label.lamp_half * mounts::PLINTH_FOOTPRINT
                ),
                "{}: actual plinth attachment must be supported",
                label.name
            );
            assert!(
                BACK > 0. && FRONT < label.lamp_half * mounts::PLINTH_HEIGHT,
                "{}: neck must join the solid plinth edge",
                label.name
            );
            let tag = tag_mesh(label);
            let edge = label.lamp_half * mounts::PLINTH_FOOTPRINT;
            assert!(
                tag.vertices.iter().all(|v| if label.side {
                    v.pos[0] >= edge - 1e-6
                } else {
                    v.pos[1] <= -edge + 1e-6
                }),
                "{}: tag must not cover its lamp or plinth",
                label.name
            );
            assert!(model.determinant() > 0.999 && model.determinant() < 1.001);
        }
        // Adjacent source-mounted sight pairs need distinct labels at their real separation.
        for pair in mounts::SIGHT {
            let centers = pair.map(|(m, a)| {
                m.label_frame(a)
                    .transform_point3(Vec3::new(0., -0.019, INK_Z))
            });
            assert!(centers[0].distance(centers[1]) > 2. * TAG_HALF + 0.001);
        }
    }
}
