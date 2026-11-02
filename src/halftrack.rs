//! Optional transport prototype adapter, not a vehicle simulation.
//!
//! The reviewed publisher mesh has +Y up and +Z forward, matching `Tank::hull_model`.
//! Geometry is partitioned by existing glTF subtrees; articulation uses their actual accumulated
//! pivot frames. No invented suspension, steering limit, mass, towing rating, seats or armor.
//! The source pairs both rear sprockets in one object and both idlers in another: they cannot
//! have differential left/right animation without further asset work. Track belts stay static.

use crate::gltf_load::CpuMesh;
use anyhow::{ensure, Context};
use glam::{Mat4, Vec3};

pub const ASSET_PATH: &str =
    "assets/vehicles/polyfork_halftrack_20261030/baked/halftrack_transport_prototype.glb";
pub const REVIEWED_SHA256: &str =
    "9a8ba1ed19f1e025c2203c2b31436fd15145e66ce1e024a7c08958948435557f";
pub const PART_COUNT: usize = 6;
pub const PART_NAMES: [&str; PART_COUNT] = [
    "hull",
    "steer-fl",
    "steer-fr",
    "sprockets",
    "idlers",
    "tools",
];
const PIVOT_NAMES: [&str; 6] = [
    "steer-fl",
    "steer-fr",
    "wheel-fl",
    "wheel-fr",
    "sprockets",
    "idlers",
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Articulation {
    pub steer_left_rad: f32,
    pub steer_right_rad: f32,
    pub front_left_spin_rad: f32,
    pub front_right_spin_rad: f32,
    pub sprocket_spin_rad: f32,
    pub idler_spin_rad: f32,
}

#[derive(Clone, Debug)]
pub struct HalftrackRig {
    /// Model-space transforms of the six source nodes, including every ancestor.
    pub pivot_frames: [Mat4; 6],
}

fn at_frame(frame: Mat4, rotation: Mat4) -> Mat4 {
    frame * rotation * frame.inverse()
}

impl HalftrackRig {
    /// Apply world placement after model-space pivot motions. Existing loader pages are already
    /// baked into model space, so multiplying a node frame directly would translate them twice.
    pub fn models(&self, root: Mat4, pose: Articulation) -> anyhow::Result<[Mat4; PART_COUNT]> {
        let angles = [
            pose.steer_left_rad,
            pose.steer_right_rad,
            pose.front_left_spin_rad,
            pose.front_right_spin_rad,
            pose.sprocket_spin_rad,
            pose.idler_spin_rad,
        ];
        ensure!(
            root.is_finite()
                && root.determinant().abs() > 1e-8
                && angles.iter().all(|v| v.is_finite()),
            "halftrack root and articulation must be finite and root must be invertible"
        );
        let [sl, sr, wl, wr, sprockets, idlers] = self.pivot_frames;
        Ok([
            root,
            root * at_frame(sl, Mat4::from_rotation_y(pose.steer_left_rad))
                * at_frame(wl, Mat4::from_rotation_x(pose.front_left_spin_rad)),
            root * at_frame(sr, Mat4::from_rotation_y(pose.steer_right_rad))
                * at_frame(wr, Mat4::from_rotation_x(pose.front_right_spin_rad)),
            root * at_frame(sprockets, Mat4::from_rotation_x(pose.sprocket_spin_rad)),
            root * at_frame(idlers, Mat4::from_rotation_x(pose.idler_spin_rad)),
            root,
        ])
    }
}

pub struct HalftrackAsset {
    pub meshes: Vec<CpuMesh>,
    pub rig: HalftrackRig,
    pub min: Vec3,
    pub max: Vec3,
    pub triangles: usize,
}

impl HalftrackAsset {
    /// Explicit opt-in: ordinary Game construction does not load this private prototype.
    pub fn load() -> anyhow::Result<Self> {
        Self::load_path(ASSET_PATH).with_context(|| format!(
            "optional halftrack prototype unavailable or invalid at {ASSET_PATH}; restore its private reviewed assets before running halftrack-review"))
    }

    pub fn load_path(path: &str) -> anyhow::Result<Self> {
        let bytes = crate::resource::read(path)?;
        let document =
            gltf::Gltf::from_slice(&bytes).context("parse halftrack source hierarchy")?;
        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .context("halftrack has no scene")?;
        let mut frames = [None; 6];
        fn visit(
            node: gltf::Node<'_>,
            parent: Mat4,
            frames: &mut [Option<Mat4>; 6],
        ) -> anyhow::Result<()> {
            let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
            ensure!(
                world.is_finite() && world.determinant().abs() > 1e-8,
                "invalid halftrack node transform"
            );
            if let Some(i) = PIVOT_NAMES
                .iter()
                .position(|name| Some(*name) == node.name())
            {
                ensure!(
                    frames[i].is_none(),
                    "duplicate halftrack pivot {}",
                    PIVOT_NAMES[i]
                );
                frames[i] = Some(world);
            }
            for child in node.children() {
                visit(child, world, frames)?;
            }
            Ok(())
        }
        for node in scene.nodes() {
            visit(node, Mat4::IDENTITY, &mut frames)?;
        }
        let mut pivot_frames = [Mat4::IDENTITY; 6];
        for (i, frame) in frames.into_iter().enumerate() {
            pivot_frames[i] =
                frame.with_context(|| format!("missing halftrack pivot {}", PIVOT_NAMES[i]))?;
        }
        // Select steering parents, not nested wheel children: the loader forbids overlapping
        // selections. The nested wheel frames remain available separately for composite motion.
        let meshes = crate::gltf_load::load_glb_partitioned_subtrees(path, &PART_NAMES[1..])?;
        ensure!(
            meshes.len() == PART_COUNT,
            "halftrack mesh partition changed"
        );
        ensure!(
            meshes
                .iter()
                .all(|mesh| !mesh.indices.is_empty() && mesh.texture.is_some()),
            "halftrack part is empty or lost its source palette texture"
        );
        let min = meshes
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |v, m| v.min(m.min));
        let max = meshes
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |v, m| v.max(m.max));
        let dimensions = max - min;
        ensure!(
            min.is_finite()
                && max.is_finite()
                && (dimensions - Vec3::new(2.3, 2.55, 6.04))
                    .abs()
                    .max_element()
                    < 0.01,
            "halftrack source dimensions changed: {dimensions:?}"
        );
        let triangles = meshes.iter().map(|mesh| mesh.indices.len() / 3).sum();
        ensure!(
            triangles == 2652,
            "halftrack source topology changed: {triangles} triangles"
        );
        Ok(Self {
            meshes,
            rig: HalftrackRig { pivot_frames },
            min,
            max,
            triangles,
        })
    }

    pub fn append_to_scene(&self, scene: &mut Vec<CpuMesh>) -> HalftrackInstance {
        let first = scene.len();
        scene.extend(self.meshes.iter().cloned());
        HalftrackInstance {
            first_mesh: first,
            rig: self.rig.clone(),
        }
    }

    pub fn inspection(&self) -> serde_json::Value {
        serde_json::json!({
            "asset":ASSET_PATH, "reviewed_file_sha256":REVIEWED_SHA256,
            "hash_note":"Provenance receipt hash; runtime loader verifies hierarchy, dimensions and topology, not SHA-256",
            "bounds_m":{"min":self.min.to_array(),"max":self.max.to_array()},
            "triangles":self.triangles, "parts":PART_NAMES,
            "pivots":PIVOT_NAMES.iter().zip(self.rig.pivot_frames).map(|(name,frame)|
                serde_json::json!({"name":name,"model_frame":frame.to_cols_array(),
                    "model_origin_m":frame.transform_point3(Vec3::ZERO).to_array()})).collect::<Vec<_>>(),
            "axes":"Y up; +Z forward as Tank::hull_model; source X coordinates preserved",
            "limitations":["generic transport prototype, not an identified historical recovery vehicle",
                "no driving, suspension, collision, boarding, cargo, damage or towing simulation",
                "rear pairs share source objects; track belts do not animate"]
        })
    }
}

#[derive(Clone)]
pub struct HalftrackInstance {
    pub first_mesh: usize,
    pub rig: HalftrackRig,
}

impl HalftrackInstance {
    pub fn apply(
        &self,
        renderer: &mut crate::renderer::Renderer,
        root: Mat4,
        pose: Articulation,
    ) -> anyhow::Result<()> {
        ensure!(
            self.first_mesh
                .checked_add(PART_COUNT)
                .is_some_and(|end| end <= renderer.mesh_count()),
            "halftrack instance slots are outside renderer scene"
        );
        for (i, model) in self.rig.models(root, pose)?.into_iter().enumerate() {
            renderer.set_model(self.first_mesh + i, model);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: Vec3, b: Vec3) {
        assert!((a - b).length() < 0.0001, "{a:?} != {b:?}");
    }
    #[test]
    fn composite_steering_and_spin_keep_the_source_hub_fixed() {
        let hub = Vec3::new(-0.86, 0.435, 2.12);
        let frame = Mat4::from_translation(hub);
        let rig = HalftrackRig {
            pivot_frames: [frame; 6],
        };
        let root = Mat4::from_translation(Vec3::new(18., 4., -9.)) * Mat4::from_rotation_y(0.7);
        let models = rig
            .models(
                root,
                Articulation {
                    steer_left_rad: 0.4,
                    front_left_spin_rad: 1.1,
                    ..Default::default()
                },
            )
            .unwrap();
        near(models[1].transform_point3(hub), root.transform_point3(hub));
        let local = Vec3::new(0., 0.4, 0.);
        let expected = root.transform_point3(
            hub + Mat4::from_rotation_y(0.4)
                .transform_vector3(Mat4::from_rotation_x(1.1).transform_vector3(local)),
        );
        near(models[1].transform_point3(hub + local), expected);
        assert_eq!(models[0], root);
        assert_eq!(models[5], root);
    }
    #[test]
    fn articulated_frames_preserve_parent_axes_and_nested_offsets() {
        let parent = Mat4::from_translation(Vec3::new(3., 1., 2.)) * Mat4::from_rotation_z(0.3);
        let wheel = parent * Mat4::from_translation(Vec3::X * 0.2);
        let rig = HalftrackRig {
            pivot_frames: [parent, parent, wheel, wheel, parent, parent],
        };
        let model = rig
            .models(
                Mat4::IDENTITY,
                Articulation {
                    steer_left_rad: 0.5,
                    front_left_spin_rad: 0.8,
                    ..Default::default()
                },
            )
            .unwrap()[1];
        let point = wheel.transform_point3(Vec3::Y);
        let expected = (parent
            * Mat4::from_rotation_y(0.5)
            * Mat4::from_translation(Vec3::X * 0.2)
            * Mat4::from_rotation_x(0.8))
        .transform_point3(Vec3::Y);
        near(model.transform_point3(point), expected);
    }
    #[test]
    fn invalid_pose_fails_before_render_transforms_are_changed() {
        let rig = HalftrackRig {
            pivot_frames: [Mat4::IDENTITY; 6],
        };
        assert!(rig.models(Mat4::ZERO, Articulation::default()).is_err());
        assert!(rig
            .models(
                Mat4::IDENTITY,
                Articulation {
                    steer_left_rad: f32::NAN,
                    ..Default::default()
                }
            )
            .is_err());
    }
    #[test]
    #[ignore = "requires the private reviewed halftrack asset; CPU only"]
    fn reviewed_private_asset_loads_with_source_pivots_and_textured_pages() {
        let asset = HalftrackAsset::load().unwrap();
        near(
            asset.rig.pivot_frames[0].transform_point3(Vec3::ZERO),
            Vec3::new(-0.86, 0.435, 2.12),
        );
        near(
            asset.rig.pivot_frames[1].transform_point3(Vec3::ZERO),
            Vec3::new(0.86, 0.435, 2.12),
        );
        near(
            asset.rig.pivot_frames[4].transform_point3(Vec3::ZERO),
            Vec3::new(0., 0.52, 0.78),
        );
        near(
            asset.rig.pivot_frames[5].transform_point3(Vec3::ZERO),
            Vec3::new(0., 0.52, -1.93),
        );
        let models = asset
            .rig
            .models(Mat4::IDENTITY, Articulation::default())
            .unwrap();
        for model in models {
            assert!(model.abs_diff_eq(Mat4::IDENTITY, 1e-5));
        }
    }
}
