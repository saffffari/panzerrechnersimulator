//! Versioned, source-skinned German crouch frames. Runtime selects exact deformed geometry;
//! there is no height scale or translation which pretends that straight legs are bent.
use super::*;
use crate::infantry_combat::{CROUCH_STAGES, GERMAN_BODY_HEIGHTS};

pub(super) const POSE_COUNT: usize = CROUCH_STAGES * 2;
const ROOT: &str = "assets/flavor/Characters/german-ww2-soldier-cc0/baked/infantry_crouch_v1";

// Audited against every original FBX polygon and its directed UV boundary, including n-2
// triangles per polygon. See artifacts/astra-audit-20260905/german-pose-topology-audit-20260905.json.
// Blender splits vertices at deformed normal seams (5228..5231 vertices), and re-tessellates
// a few source n-gons differently per pose. Vertex-count equality is therefore not topology
// validation. Pin the audited oriented UV triangles instead: this ignores split indices and
// export order, but still rejects changed UVs, winding, missing/duplicated faces or new diagonals.
const SOURCE_TRIANGLE_UV_FINGERPRINTS: [u64; POSE_COUNT] = [
    0x81fb902c4ebf5445, 0x92489c9dbdd3e144, 0x90f016ca7407865e,
    0x9ea5a29e30711fee, 0xbd409850a69f1e0f, 0xb0be8ca810756364,
    0xdf465d9bca024f55, 0x3a2836012cbd53c7, 0xac83f16f8fd38bb7,
    0xf81ae4844ac3cc26,
];

/// Call only after finite UVs and valid triangle indices have been checked. This is a
/// deterministic asset-regression fingerprint, not a security/content-authentication hash.
pub(super) fn triangle_uv_fingerprint(mesh: &CpuMesh) -> u64 {
    let mut triangles: Vec<[[u32; 2]; 3]> = mesh.indices.chunks_exact(3).map(|indices| {
        let corners = std::array::from_fn(|i| mesh.vertices[indices[i] as usize].uv.map(f32::to_bits));
        // Cyclic rotation preserves winding; sorting the corners would conceal flipped faces.
        corners.min([corners[1], corners[2], corners[0]])
            .min([corners[2], corners[0], corners[1]])
    }).collect();
    triangles.sort_unstable();
    triangles.iter().flatten().flatten().flat_map(|bits| bits.to_le_bytes())
        .fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3))
}

pub(super) fn load_german_poses() -> anyhow::Result<[CpuMesh; POSE_COUNT]> {
    let mut meshes: Vec<CpuMesh> = Vec::with_capacity(POSE_COUNT);
    for aimed in [false, true] {
        for stage in 0..CROUCH_STAGES {
            let mode = if aimed { "aim" } else { "relaxed" };
            let path = format!("{ROOT}/german_infantry_{mode}_crouch_{stage:02}.glb");
            let mesh = crate::gltf_load::load_glb(&path)?;
            anyhow::ensure!(!mesh.indices.is_empty() && mesh.indices.len() % 3 == 0,
                "invalid German pose topology: {path}");
            anyhow::ensure!(mesh.indices.iter().all(|&i| (i as usize) < mesh.vertices.len())
                && mesh.vertices.iter().all(|v| v.pos.iter().chain(v.normal.iter()).chain(v.uv.iter()).all(|f| f.is_finite())),
                "nonfinite or invalid German pose vertices: {path}");
            anyhow::ensure!(mesh.indices.len() == 15_219
                && triangle_uv_fingerprint(&mesh) == SOURCE_TRIANGLE_UV_FINGERPRINTS[meshes.len()],
                "German pose changed audited source triangle coverage, winding or UVs: {path}");
            anyhow::ensure!(mesh.min.y.abs() < 0.012 && (mesh.max.y - GERMAN_BODY_HEIGHTS[stage]).abs() < 0.003,
                "German pose does not match the fixed-root physics stage: {path}");
            let texture = mesh.texture.as_ref().ok_or_else(|| anyhow::anyhow!("missing German source texture: {path}"))?;
            if let Some(first) = meshes.first() {
                let first_texture = first.texture.as_ref().unwrap();
                anyhow::ensure!(first_texture.width == texture.width && first_texture.height == texture.height
                    && first_texture.rgba.as_slice() == texture.rgba.as_slice(),
                    "German pose changed source texture: {path}");
            }
            meshes.push(mesh);
        }
    }
    meshes.try_into().map_err(|_: Vec<CpuMesh>| anyhow::anyhow!("incomplete German crouch set"))
}

pub(super) fn register_german_poses(device: &wgpu::Device, queue: &wgpu::Queue,
    renderer: &mut Renderer) -> Option<[usize; POSE_COUNT]> {
    let meshes = match load_german_poses() {
        Ok(meshes) => meshes,
        Err(error) => {
            eprintln!("[mixed-combat] German articulated set unavailable: {error:#}; standing-only fallback");
            return None;
        }
    };
    let mut batches = Vec::with_capacity(POSE_COUNT);
    for mesh in &meshes {
        let prototype = renderer.register_instanced_prototype(device, queue, mesh);
        let batch = renderer.add_moving_instanced_pool(device, prototype, mixed_combat_runtime::GERMAN_COUNT)?;
        batches.push(batch);
    }
    println!("[mixed-combat] German original-rig crouch: five fixed-root stages, shared relaxed/aim geometry");
    batches.try_into().ok()
}

impl Game {
    pub(super) fn drive_german_pose_batches(&self, renderer: &mut Renderer) {
        let Some(batches) = self.german_pose_batches else { return; };
        let mut transforms: [Vec<Mat4>; POSE_COUNT] = std::array::from_fn(|_| Vec::with_capacity(mixed_combat_runtime::GERMAN_COUNT));
        if self.mixed_combat.enabled {
            for soldier in self.soldiers.iter().skip(self.soviet_soldier_count).filter(|s| s.alive) {
                let pose = soldier.crouch_stage + if soldier.aiming { CROUCH_STAGES } else { 0 };
                transforms[pose].push(Mat4::from_rotation_translation(
                    Quat::from_rotation_arc(Vec3::Z, soldier.forward()), soldier.pos));
            }
        }
        for (batch, mats) in batches.into_iter().zip(transforms.iter()) {
            assert!(renderer.set_moving_instanced_mats(batch, mats), "German pose pool rejected bounded actor count");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infantry_combat::{GERMAN_EYE_FORWARDS, GERMAN_EYE_HEIGHTS, GERMAN_EYE_X};

    #[test]
    fn supplied_soviet_library_loads_all_textured_poses_at_shared_standing_height() {
        let meshes = super::super::load_exact_soviet_pose_meshes()
            .expect("complete derivative of the user-supplied Soviet character");
        assert_eq!(meshes.len(), crate::soviet_animation::SOVIET_POSE_COUNT);
        for mesh in &meshes {
            assert!(mesh.min.y.abs() < 0.003, "every static frame is grounded");
            assert_eq!(mesh.indices.len(), 6_252);
            assert!(mesh.texture.as_ref().unwrap().width > 1, "actual source atlas, not a solid fallback");
        }
        for pose in [crate::soviet_animation::SovietPose::Idle00, crate::soviet_animation::SovietPose::Aim00] {
            assert!((meshes[pose.index()].max.y - 1.75).abs() < 0.003,
                "neutral/aim source body follows the existing standing physics height");
        }
        assert!(meshes[crate::soviet_animation::SovietPose::Death03.index()].max.y < 1.1,
            "final authored death frame is a fallen pose");
    }

    #[test]
    fn source_surface_accepts_index_splits_and_export_order_changes() {
        let mut mesh = crate::gltf_load::load_glb(&format!("{ROOT}/german_infantry_relaxed_crouch_00.glb")).unwrap();
        let expected = triangle_uv_fingerprint(&mesh);
        let duplicate = mesh.vertices[mesh.indices[0] as usize];
        mesh.indices[0] = mesh.vertices.len() as u32;
        mesh.vertices.push(duplicate);
        for i in 0..3 { mesh.indices.swap(i, i + 3); }
        for triangle in mesh.indices.chunks_exact_mut(3) { triangle.rotate_left(1); }
        assert_eq!(triangle_uv_fingerprint(&mesh), expected);
    }

    #[test]
    fn source_surface_rejects_uv_winding_and_triangle_coverage_changes() {
        let mesh = crate::gltf_load::load_glb(&format!("{ROOT}/german_infantry_relaxed_crouch_00.glb")).unwrap();
        let expected = triangle_uv_fingerprint(&mesh);
        let mut altered = mesh.clone();
        altered.vertices[altered.indices[0] as usize].uv[0] += 0.001;
        assert_ne!(triangle_uv_fingerprint(&altered), expected, "UV corruption must be detected");
        let mut altered = mesh.clone();
        altered.indices.swap(0, 1);
        assert_ne!(triangle_uv_fingerprint(&altered), expected, "reversed winding must be detected");
        let mut altered = mesh.clone();
        altered.indices[..3].copy_from_slice(&mesh.indices[3..6]);
        assert_ne!(triangle_uv_fingerprint(&altered), expected, "same-count missing/duplicate faces must be detected");
    }

    #[test]
    fn actual_pose_geometry_and_head_landmarks_match_shared_physics() {
        let meshes = load_german_poses().expect("complete versioned German derivative");
        let source_texture = image::open("assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier_Texture.png")
            .expect("preserved original source texture").into_rgba8();
        for mesh in &meshes {
            let texture = mesh.texture.as_ref().unwrap();
            assert_eq!((texture.width, texture.height), source_texture.dimensions());
            assert_eq!(texture.rgba.as_slice(), source_texture.as_raw().as_slice(),
                "every derivative must retain exact original source texels");
        }
        let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(format!("{ROOT}/manifest.v1.json")).unwrap()).unwrap();
        for (index, record) in manifest["poses"].as_array().unwrap().iter().enumerate() {
            let stage = index % CROUCH_STAGES;
            assert_eq!(record["deformation_quality"]["passed"], true);
            let eye = record["eye_proxy_y_up"].as_array().unwrap();
            for (value, expected) in eye.iter().zip([GERMAN_EYE_X, GERMAN_EYE_HEIGHTS[stage], GERMAN_EYE_FORWARDS[stage]]) {
                assert!((value.as_f64().unwrap() as f32 - expected).abs() < 0.00001);
            }
            assert!(meshes[index].min.y < 0.003, "boots remain on original floor, not shifted into terrain");
            assert!(GERMAN_EYE_HEIGHTS[stage] < meshes[index].max.y);
            if stage == 4 {
                for side in ["Left", "Right"] {
                    assert!(record["joints_blender_z_up"][side]["knee_angle_deg"].as_f64().unwrap() < 90.0);
                }
            }
        }
        // Actual vertex deformation cannot be reproduced by a whole-body height scale or sink:
        // the crown drops 0.65m while the floor remains within 3mm for both arm poses.
        for base in [0, CROUCH_STAGES] {
            assert!((meshes[base].max.y - meshes[base + 4].max.y - 0.65).abs() < 0.001);
            assert!((meshes[base].min.y - meshes[base + 4].min.y).abs() < 0.003);
        }
    }
}
