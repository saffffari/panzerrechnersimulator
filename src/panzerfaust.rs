//! Original low-poly Panzerfaust 60 geometry for the on-foot minigame.
//!
//! The silhouette is assembled from eight-sided frusta and hard-edged primitives: launch tube,
//! bell-shaped HEAT warhead, trigger housing, rear cap, and the folding sight leaf. It matches the
//! rest of Panzerrechnersystem's PS1 props and is cheap enough to duplicate in the Tiger rack.

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::prims::box_mesh;

const SIDES: usize = 8;

// Full-size Panzerfaust 60 reference dimensions.
const OVERALL_LENGTH_M: f32 = 1.045;
const TUBE_LENGTH_M: f32 = 0.800;
const TUBE_DIAMETER_M: f32 = 0.050;
pub const WARHEAD_DIAMETER_M: f32 = 0.150;
const PROJECTILE_LENGTH_M: f32 = 0.495;

const REAR_Z: f32 = -0.545;
const MUZZLE_Z: f32 = REAR_Z + TUBE_LENGTH_M;
const NOSE_Z: f32 = REAR_Z + OVERALL_LENGTH_M;

const SIGHT_LEAF_Z0: f32 = -0.130;
const SIGHT_LEAF_Z1: f32 = -0.100;
const SIGHT_BASELINE_M: f32 = 0.435;
const BEAD_CENTER_Y: f32 = 0.090;
const APERTURE_CENTERS_Y: [f32; 3] = [0.122, 0.155, 0.1775];
const BEAD_CENTER_Z: f32 = (SIGHT_LEAF_Z0 + SIGHT_LEAF_Z1) * 0.5 + SIGHT_BASELINE_M;
const PROJECTILE_REAR_Z: f32 = -0.250;
pub const PROJECTILE_NOSE_Z: f32 = PROJECTILE_REAR_Z + PROJECTILE_LENGTH_M;

/// One physical underarm hold shared by rendering, launch and flash placement. Moving the authored
/// metre-sized prop toward the shoulder restores its first-person size without inflating it.
#[derive(Clone, Copy, Debug)]
pub struct HeldPose {
    pub origin: Vec3,
    pub rotation: Quat,
    pub direction: Vec3,
}
impl HeldPose {
    pub fn model(self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation, self.origin)
    }
    pub fn nose(self) -> Vec3 {
        self.origin + self.direction * NOSE_Z
    }
    pub fn rear(self) -> Vec3 {
        self.origin + self.direction * REAR_Z
    }
}
pub fn held_pose(eye: Vec3, forward: Vec3, right: Vec3, up: Vec3, aiming: bool) -> HeldPose {
    let zero = crate::foot::PANZERFAUST_ZERO_ANGLE_DEG.to_radians();
    let direction = if aiming {
        (forward * zero.cos() + up * zero.sin()).normalize_or_zero()
    } else {
        forward.normalize_or_zero()
    };
    // Keep the authored +Y sight above the tube through camera yaw/pitch, including the
    // antiparallel -Z heading where a shortest-arc rotation leaves roll unconstrained.
    let rotation = crate::foot::held_weapon_rotation(direction, up);
    let origin = if aiming {
        // Centre the actual 60 m rear aperture on the camera sightline.
        eye + forward * 0.30 - rotation * Vec3::new(0.0, APERTURE_CENTERS_Y[1], -0.115)
    } else {
        eye + forward * 0.10 + right * 0.20 - up * 0.22
    };
    HeldPose {
        origin,
        rotation,
        direction,
    }
}

fn flat_texture(color: [u8; 3]) -> CpuImage {
    CpuImage {
        width: 1,
        height: 1,
        rgba: vec![color[0], color[1], color[2], 255].into(),
    }
}

/// A capped, hard-faceted frustum whose axis is local +Z.
fn frustum(z0: f32, z1: f32, r0: f32, r1: f32, color: [u8; 3]) -> CpuMesh {
    let mut vertices = Vec::with_capacity(SIDES * 4 + 2);
    let mut indices = Vec::with_capacity(SIDES * 12);
    let slope = (r0 - r1) / (z1 - z0).abs().max(1e-4);

    for i in 0..SIDES {
        let a0 = i as f32 * std::f32::consts::TAU / SIDES as f32;
        let a1 = (i + 1) as f32 * std::f32::consts::TAU / SIDES as f32;
        let mid = (a0 + a1) * 0.5;
        let normal = Vec3::new(mid.cos(), mid.sin(), slope).normalize_or_zero();
        let base = vertices.len() as u32;
        for (a, z, r, u) in [
            (a0, z0, r0, 0.0),
            (a1, z0, r0, 1.0),
            (a1, z1, r1, 1.0),
            (a0, z1, r1, 0.0),
        ] {
            vertices.push(Vertex {
                pos: [a.cos() * r, a.sin() * r, z],
                normal: normal.to_array(),
                uv: [u, if z == z0 { 0.0 } else { 1.0 }],
                tangent: [-mid.sin(), mid.cos(), 0.0, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    for (z, r, nz) in [(z0, r0, -1.0f32), (z1, r1, 1.0f32)] {
        let center = vertices.len() as u32;
        vertices.push(Vertex {
            pos: [0.0, 0.0, z],
            normal: [0.0, 0.0, nz],
            uv: [0.5, 0.5],
            tangent: [1.0, 0.0, 0.0, 1.0],
        });
        for i in 0..SIDES {
            let a0 = i as f32 * std::f32::consts::TAU / SIDES as f32;
            let a1 = (i + 1) as f32 * std::f32::consts::TAU / SIDES as f32;
            let v0 = vertices.len() as u32;
            for a in [a0, a1] {
                vertices.push(Vertex {
                    pos: [a.cos() * r, a.sin() * r, z],
                    normal: [0.0, 0.0, nz],
                    uv: [a.cos() * 0.5 + 0.5, a.sin() * 0.5 + 0.5],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                });
            }
            if nz < 0.0 {
                indices.extend_from_slice(&[center, v0 + 1, v0]);
            } else {
                indices.extend_from_slice(&[center, v0, v0 + 1]);
            }
        }
    }

    CpuMesh::basic(
        vertices,
        indices,
        Some(flat_texture(color)),
        Vec3::new(-r0.max(r1), -r0.max(r1), z0),
        Vec3::new(r0.max(r1), r0.max(r1), z1),
    )
}

fn merge(meshes: impl IntoIterator<Item = CpuMesh>) -> CpuMesh {
    let mut meshes = meshes.into_iter();
    let mut out = meshes.next().expect("Panzerfaust mesh has primitives");
    for mesh in meshes {
        let base = out.vertices.len() as u32;
        out.vertices.extend(mesh.vertices);
        out.indices
            .extend(mesh.indices.into_iter().map(|index| base + index));
        out.min = out.min.min(mesh.min);
        out.max = out.max.max(mesh.max);
    }
    out
}

/// Tube, warhead and trigger housing. Local +Z points downrange.
pub fn body_mesh() -> CpuMesh {
    let ochre = [154, 132, 67];
    merge([
        frustum(
            REAR_Z,
            MUZZLE_Z,
            TUBE_DIAMETER_M * 0.5,
            TUBE_DIAMETER_M * 0.5,
            ochre,
        ),
        frustum(REAR_Z, REAR_Z + 0.022, 0.030, 0.030, ochre),
        frustum(MUZZLE_Z - 0.040, 0.275, 0.028, 0.028, ochre),
        frustum(0.255, 0.320, 0.030, WARHEAD_DIAMETER_M * 0.5, ochre),
        frustum(0.320, 0.430, WARHEAD_DIAMETER_M * 0.5, 0.062, ochre),
        frustum(0.430, NOSE_Z, 0.062, 0.008, ochre),
        box_mesh(
            Vec3::new(-0.038, -0.080, -0.090),
            Vec3::new(0.038, -0.025, 0.045),
            ochre,
        ),
    ])
}

/// Folding sight leaf with open apertures at the physical 30/60/80 m ladder angles.
pub fn sight_mesh() -> CpuMesh {
    let dark = [45, 44, 37];
    merge([
        box_mesh(
            Vec3::new(-0.044, 0.110, SIGHT_LEAF_Z0),
            Vec3::new(-0.033, 0.190, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(0.033, 0.110, SIGHT_LEAF_Z0),
            Vec3::new(0.044, 0.190, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(-0.044, 0.110, SIGHT_LEAF_Z0),
            Vec3::new(0.044, APERTURE_CENTERS_Y[0] - 0.006, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(-0.044, APERTURE_CENTERS_Y[0] + 0.006, SIGHT_LEAF_Z0),
            Vec3::new(0.044, APERTURE_CENTERS_Y[1] - 0.007, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(-0.044, APERTURE_CENTERS_Y[1] + 0.007, SIGHT_LEAF_Z0),
            Vec3::new(0.044, APERTURE_CENTERS_Y[2] - 0.0065, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(-0.044, APERTURE_CENTERS_Y[2] + 0.0065, SIGHT_LEAF_Z0),
            Vec3::new(0.044, 0.190, SIGHT_LEAF_Z1),
            dark,
        ),
        box_mesh(
            Vec3::new(-0.006, BEAD_CENTER_Y - 0.023, BEAD_CENTER_Z - 0.010),
            Vec3::new(0.006, BEAD_CENTER_Y + 0.023, BEAD_CENTER_Z + 0.010),
            dark,
        ),
    ])
}

/// The fin-stabilized bomb that visibly leaves the disposable launch tube.
pub fn projectile_mesh() -> CpuMesh {
    let green = [105, 106, 67];
    merge([
        frustum(PROJECTILE_REAR_Z, -0.080, 0.022, 0.022, green),
        frustum(-0.080, 0.000, 0.028, WARHEAD_DIAMETER_M * 0.5, green),
        frustum(0.000, 0.130, WARHEAD_DIAMETER_M * 0.5, 0.062, green),
        frustum(0.130, PROJECTILE_NOSE_Z, 0.062, 0.008, green),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_weapon_has_full_size_upright_sights_and_measured_screen_scale() {
        let pose = held_pose(Vec3::ZERO, Vec3::Z, Vec3::X, Vec3::Y, false);
        assert!((pose.nose().distance(pose.rear()) - OVERALL_LENGTH_M).abs() < 1e-5);
        assert!((pose.model().transform_vector3(Vec3::Y) - Vec3::Y).length() < 1e-5);
        let warhead_z = 0.320;
        let previous_projected_width = WARHEAD_DIAMETER_M * 0.62 / (0.66 + warhead_z * 0.62);
        let restored_projected_width = WARHEAD_DIAMETER_M / (pose.origin.z + warhead_z);
        let ratio = restored_projected_width / previous_projected_width;
        assert!(
            (3.0..3.5).contains(&ratio),
            "warhead apparent width ratio {ratio}"
        );
        let aim = held_pose(Vec3::ZERO, Vec3::Z, Vec3::X, Vec3::Y, true);
        let aperture = aim
            .model()
            .transform_point3(Vec3::new(0.0, APERTURE_CENTERS_Y[1], -0.115));
        assert!(aperture.x.abs() < 1e-5 && aperture.y.abs() < 1e-5);
        assert!((aperture.z - 0.30).abs() < 1e-5);
    }

    #[test]
    fn held_panzerfaust_stays_upright_and_sight_aligned_through_full_camera_yaw() {
        let eye = Vec3::new(3.0, 1.7, -4.0);
        for pitch_degrees in [-70.0_f32, -20.0, 0.0, 20.0, 70.0] {
            let pitch = pitch_degrees.to_radians();
            for aiming in [false, true] {
                let mut previous: Option<Quat> = None;
                for degrees in 0..=360 {
                    let yaw = (degrees as f32).to_radians();
                    let forward = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
                    let right = forward.cross(Vec3::Y).normalize();
                    let up = right.cross(forward);
                    let pose = held_pose(eye, forward, right, up, aiming);
                    assert!(pose.rotation.is_finite());
                    assert!((pose.rotation * Vec3::Z).distance(pose.direction) < 1e-5);
                    assert!((pose.rotation * Vec3::Y).dot(up) > 0.98,
                        "sight rolled at yaw {degrees}, pitch {pitch_degrees}, aiming {aiming}");
                    assert!((pose.nose().distance(pose.rear()) - OVERALL_LENGTH_M).abs() < 1e-5);
                    assert!(pose.model().transform_point3(Vec3::Z * NOSE_Z).distance(pose.nose()) < 1e-5);
                    if aiming {
                        let aperture = pose.model().transform_point3(Vec3::new(0.0, APERTURE_CENTERS_Y[1], -0.115));
                        assert!(aperture.distance(eye + forward * 0.30) < 1e-5);
                    } else {
                        assert!(pose.origin.distance(eye + forward * 0.10 + right * 0.20 - up * 0.22) < 1e-5);
                    }
                    if let Some(previous) = previous {
                        assert!(pose.rotation.dot(previous).abs() > 0.999,
                            "held weapon snapped at yaw {degrees}, pitch {pitch_degrees}");
                    }
                    previous = Some(pose.rotation);
                }
            }
        }
    }

    #[test]
    fn silhouette_is_low_poly_and_points_downrange() {
        let mesh = body_mesh();
        assert!(mesh.vertices.len() < 800);
        assert!(mesh.indices.len() < 1_500);
        assert!((mesh.max.z - mesh.min.z - OVERALL_LENGTH_M).abs() < 1e-5);
        assert!((mesh.max.x - mesh.min.x - WARHEAD_DIAMETER_M).abs() < 1e-5);
        let tube_radius = mesh
            .vertices
            .iter()
            .filter(|v| {
                ((v.pos[2] - REAR_Z).abs() < 1e-5 || (v.pos[2] - MUZZLE_Z).abs() < 1e-5)
                    && Vec3::new(v.pos[0], v.pos[1], 0.0).length() <= TUBE_DIAMETER_M * 0.5 + 1e-5
            })
            .map(|v| Vec3::new(v.pos[0], v.pos[1], 0.0).length())
            .fold(0.0f32, f32::max);
        assert!((tube_radius * 2.0 - TUBE_DIAMETER_M).abs() < 1e-5);
    }

    #[test]
    fn projectile_matches_reference_envelope() {
        let mesh = projectile_mesh();
        assert!((mesh.max.z - mesh.min.z - PROJECTILE_LENGTH_M).abs() < 1e-5);
        assert!((mesh.max.x - mesh.min.x - WARHEAD_DIAMETER_M).abs() < 1e-5);
    }

    #[test]
    fn open_sight_slots_resolve_to_30_60_80_metres() {
        let leaf_z = (SIGHT_LEAF_Z0 + SIGHT_LEAF_Z1) * 0.5;
        assert!(((BEAD_CENTER_Z - leaf_z) - SIGHT_BASELINE_M).abs() < 1e-6);
        for ((centre_y, expected_deg), range_m) in APERTURE_CENTERS_Y
            .into_iter()
            .zip([4.2f32, 8.5, 11.4])
            .zip([30.0f32, 60.0, 80.0])
        {
            let angle = ((centre_y - BEAD_CENTER_Y) / SIGHT_BASELINE_M)
                .atan()
                .to_degrees();
            assert!(
                (angle - expected_deg).abs() < 0.15,
                "{range_m:.0} m aperture resolves to {angle:.2}°, expected {expected_deg:.1}°"
            );
        }
    }
}
