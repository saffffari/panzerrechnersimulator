//! Isolated tank-garage documents, strict source loading, and articulated attachments.
//! No Act/world generation or renderer is constructed here. Save destinations are explicit local
//! authoring paths: KESSEL_ASSET_ROOT is for reading purchased assets, never deciding where to save.
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context};
use glam::{Vec2, Vec3};
use serde_json::{json, Value};

use crate::environment_authoring::{self, AuthoredLight, LightingTuning};
use crate::gltf_load::CpuMesh;
use crate::tank::{Parts, Tank};
use crate::tank_dressing::{Frame, TigerDressing};

pub const SCHEMA: &str = "kessel.tank-editor-scene";
pub const VERSION: u64 = 1;
pub const MAX_LIGHTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum TankAssetId {
    TigerI,
    TigerII,
    Is2,
    T34_76,
    PanzerIV,
}

impl TankAssetId {
    pub const ALL: [Self; 5] = [
        Self::TigerI,
        Self::TigerII,
        Self::Is2,
        Self::T34_76,
        Self::PanzerIV,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TigerI => "tiger_i",
            Self::TigerII => "tiger_ii",
            Self::Is2 => "is2",
            Self::T34_76 => "t34_76",
            Self::PanzerIV => "panzer_iv",
        }
    }
    pub fn parse_arg(value: &str) -> anyhow::Result<Self> {
        Self::parse(&value.to_ascii_lowercase().replace('-', "_"))
    }
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        Self::ALL
            .into_iter()
            .find(|id| id.as_str() == value)
            .ok_or_else(|| anyhow::anyhow!("unknown tank editor asset {value:?}"))
    }
    pub fn descriptor(self) -> TankAsset {
        match self {
            Self::TigerI => TankAsset {
                id: self,
                label: "Tiger I",
                articulated: true,
                full_game_playable: true,
                source_note: "Authored interior and existing full Tiger crew gameplay",
                paths: [
                    Some("assets/tanks/tiger/baked/tiger_hull.glb"),
                    Some("assets/tanks/tiger/baked/tiger_turret.glb"),
                    Some("assets/tanks/tiger/baked/tiger_gun.glb"),
                ],
            },
            Self::T34_76 => TankAsset {
                id: self,
                label: "T-34/76",
                articulated: true,
                full_game_playable: false,
                source_note: "Baked exterior rig; no verified crew interior gameplay",
                paths: [
                    Some("assets/tanks/T34-76/baked/t34_hull.glb"),
                    Some("assets/tanks/T34-76/baked/t34_turret.glb"),
                    Some("assets/tanks/T34-76/baked/t34_gun.glb"),
                ],
            },
            Self::TigerII => static_asset(
                self,
                "Tiger II",
                "assets/tanks/editor/tiger_ii/source_full.glb",
            ),
            Self::Is2 => static_asset(self, "IS-2", "assets/tanks/editor/is2/source_full.glb"),
            Self::PanzerIV => static_asset(
                self,
                "Panzer IV",
                "assets/tanks/editor/panzer_iv/source_full.glb",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TankAsset {
    pub id: TankAssetId,
    pub label: &'static str,
    pub articulated: bool,
    pub full_game_playable: bool,
    pub source_note: &'static str,
    pub paths: [Option<&'static str>; 3],
}

fn static_asset(id: TankAssetId, label: &'static str, path: &'static str) -> TankAsset {
    TankAsset { id, label, articulated: false, full_game_playable: false,
        source_note: "Full interior source geometry; neutral preview material; source has no authored hinge rig", paths: [Some(path), None, None] }
}

impl TankAsset {
    pub fn available(self) -> bool {
        self.paths
            .into_iter()
            .flatten()
            .all(|path| resolve_asset(path).is_ok())
    }
    pub fn full_game_playable(self) -> bool {
        self.full_game_playable
    }
    pub fn supports_frame(self, frame: Frame) -> bool {
        self.articulated || frame == Frame::Hull
    }
}

/// Only models whose real source files are available are offered in the garage, never box fallbacks.
pub fn catalog() -> Vec<TankAsset> {
    TankAssetId::ALL
        .into_iter()
        .map(TankAssetId::descriptor)
        .filter(|asset| asset.available())
        .collect()
}

/// Prefer the explicit current checkout's new authoring exports over a separate read-only asset
/// warehouse. Return an absolute path so the GLB loader cannot remap it back through asset roots.
pub(crate) fn resolve_asset(logical: &str) -> anyhow::Result<PathBuf> {
    let local = Path::new(logical);
    let path = if local.is_file() {
        local.to_owned()
    } else {
        crate::resource::resolve(logical)?
    };
    ensure!(
        path.is_file(),
        "tank editor source is missing: {}",
        path.display()
    );
    path.canonicalize()
        .with_context(|| format!("resolve tank editor source {}", path.display()))
}

pub struct PreviewParts {
    pub meshes: [CpuMesh; 3],
    pub parts: Parts,
    pub visible_parts: [bool; 3],
}

pub fn load_preview(id: TankAssetId) -> anyhow::Result<PreviewParts> {
    let asset = id.descriptor();
    let hidden = || crate::prims::box_mesh(Vec3::splat(-0.001), Vec3::splat(0.001), [0, 0, 0]);
    let mut meshes = [hidden(), hidden(), hidden()];
    let mut visible_parts = [false; 3];
    for (index, logical) in asset.paths.into_iter().enumerate() {
        let Some(logical) = logical else { continue };
        let path = resolve_asset(logical)?;
        let path = path.to_str().context("tank source path is not UTF-8")?;
        meshes[index] = if id == TankAssetId::TigerI {
            crate::gltf_load::load_glb_with_material_override(
                path,
                crate::tank::tiger_paint_finish,
            )?
        } else {
            crate::gltf_load::load_glb(path)?
        };
        ensure!(
            !meshes[index].vertices.is_empty() && !meshes[index].indices.is_empty(),
            "tank source mesh is empty: {logical}"
        );
        visible_parts[index] = true;
    }
    let parts = match id {
        TankAssetId::TigerI => crate::tank::TIGER_PARTS,
        TankAssetId::T34_76 => crate::tank::T34_PARTS,
        _ => {
            // Whole-model exports are already normalized by their provenance-bearing import. Do
            // not invent a turret pivot or break source objects into guessed articulation groups.
            let mesh = &meshes[0];
            let center = (mesh.min + mesh.max) * 0.5;
            Parts {
                turret_pivot: Vec3::ZERO,
                gun_trunnion: Vec3::ZERO,
                gun_len: 0.0,
                commander_eye: Vec3::new(center.x, mesh.max.y + 0.25, center.z),
                center_h: center.y,
                hull_half_extents: Vec2::new(
                    mesh.min.x.abs().max(mesh.max.x.abs()),
                    mesh.min.z.abs().max(mesh.max.z.abs()),
                ),
            }
        }
    };
    Ok(PreviewParts {
        meshes,
        parts,
        visible_parts,
    })
}

/// The existing renderer-neutral light record uses tank-local position/direction here. Conversion
/// back to world coordinates follows exactly the same Hull/Turret/Gun transforms as dressing.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachedLight {
    pub frame: Frame,
    pub light: AuthoredLight,
}

impl AttachedLight {
    pub fn world_light(&self, tank: &Tank) -> AuthoredLight {
        let parent = self.frame.visual_parent_model(tank);
        let mut light = self.light.clone();
        light.position = parent.transform_point3(light.position);
        light.direction = parent
            .transform_vector3(light.direction)
            .normalize_or_zero();
        light
    }
    pub fn from_world(frame: Frame, tank: &Tank, mut light: AuthoredLight) -> Self {
        let inverse = frame.visual_parent_model(tank).inverse();
        light.position = inverse.transform_point3(light.position);
        light.direction = inverse
            .transform_vector3(light.direction)
            .normalize_or_zero();
        Self { frame, light }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub tank: TankAssetId,
    pub dressing: TigerDressing,
    pub lights: Vec<AttachedLight>,
    pub lighting: LightingTuning,
}

/// Name used by the editor-world integration.
pub type TankEditorScene = Scene;

impl Scene {
    pub fn new(tank: TankAssetId) -> Self {
        Self {
            tank,
            dressing: TigerDressing::default(),
            lights: Vec::new(),
            // The studio stands under the act clock's sky like the world; only its weather is
            // authored here.
            lighting: LightingTuning {
                enabled: true,
                utc: None,
                fog_density: 0.0,
                overcast: 0.25,
                storm: 0.0,
            },
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        self.dressing.validate()?;
        self.lighting.validate()?;
        ensure!(
            self.lights.len() <= MAX_LIGHTS,
            "tank scene exceeds {MAX_LIGHTS} lights"
        );
        let asset = self.tank.descriptor();
        for (index, prop) in self.dressing.props.iter().enumerate() {
            ensure!(
                asset.supports_frame(prop.frame),
                "{} props[{index}] has no authored {} frame",
                asset.label,
                prop.frame.as_str()
            );
        }
        let mut ids = BTreeSet::new();
        for (index, source) in self.lights.iter().enumerate() {
            source.light.validate()?;
            ensure!(
                source.light.position.abs().max_element() <= 100.0,
                "lights[{index}] must remain within 100m of its tank frame"
            );
            ensure!(
                asset.supports_frame(source.frame),
                "{} lights[{index}] has no authored {} frame",
                asset.label,
                source.frame.as_str()
            );
            ensure!(
                ids.insert(&source.light.id),
                "duplicate attached light id {}",
                source.light.id
            );
        }
        Ok(())
    }
    pub fn to_value(&self) -> anyhow::Result<Value> {
        self.validate()?;
        let lights = self.lights.iter().filter(|source| !source.light.deleted).map(|source| {
            json!({"frame":source.frame.as_str(), "light":environment_authoring::lights_to_json(std::slice::from_ref(&source.light))[0]})
        }).collect::<Vec<_>>();
        Ok(
            json!({"schema":SCHEMA,"version":VERSION,"tank":self.tank.as_str(),
            "dressing":self.dressing.to_value()?,"lights":lights,"lighting":self.lighting.to_json()}),
        )
    }
    pub fn from_value(value: &Value) -> anyhow::Result<Self> {
        ensure!(
            value["schema"].as_str() == Some(SCHEMA),
            "tank scene schema must be {SCHEMA}"
        );
        ensure!(
            value["version"].as_u64() == Some(VERSION),
            "unsupported tank scene version"
        );
        let tank = TankAssetId::parse(
            value["tank"]
                .as_str()
                .context("tank scene has no tank id")?,
        )?;
        let dressing = TigerDressing::from_value(&value["dressing"])?;
        let values = value["lights"]
            .as_array()
            .context("tank scene lights must be an array")?;
        ensure!(
            values.len() <= MAX_LIGHTS,
            "tank scene exceeds {MAX_LIGHTS} lights"
        );
        let mut lights = Vec::with_capacity(values.len());
        for source in values {
            let frame = match source["frame"].as_str() {
                Some("hull") => Frame::Hull,
                Some("turret") => Frame::Turret,
                Some("gun") => Frame::Gun,
                _ => anyhow::bail!("attached light frame must be hull, turret, or gun"),
            };
            let parsed = environment_authoring::lights_from_edits(Some(&json!({
                (environment_authoring::LIGHTS_KEY): [source["light"].clone()]
            })))?;
            lights.push(AttachedLight {
                frame,
                light: parsed
                    .into_iter()
                    .next()
                    .context("attached light missing")?,
            });
        }
        ensure!(
            value["lighting"].is_object(),
            "tank scene lighting must be an object"
        );
        let lighting = LightingTuning::from_edits(Some(
            &json!({ (environment_authoring::LIGHTING_KEY): value["lighting"].clone() }),
        ))?;
        let scene = Self {
            tank,
            dressing,
            lights,
            lighting,
        };
        scene.validate()?;
        Ok(scene)
    }
    pub fn world_lights(&self, tank: &Tank) -> Vec<AuthoredLight> {
        self.lights
            .iter()
            .filter(|source| !source.light.deleted)
            .map(|source| source.world_light(tank))
            .collect()
    }
}

/// A checkout-relative authoring document, independent from any canonical purchase/source root.
pub fn default_scene_path(id: TankAssetId) -> PathBuf {
    PathBuf::from("assets/tanks/editor")
        .join(id.as_str())
        .join("scene.v1.json")
}

pub fn load_or_new(path: impl AsRef<Path>, expected_tank: TankAssetId) -> anyhow::Result<Scene> {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Scene::new(expected_tank))
        }
        Err(error) => {
            return Err(error).with_context(|| format!("read tank scene {}", path.display()))
        }
    };
    let value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse tank scene {}", path.display()))?;
    let scene = Scene::from_value(&value)?;
    ensure!(
        scene.tank == expected_tank,
        "tank scene belongs to {}, not {}",
        scene.tank.as_str(),
        expected_tank.as_str()
    );
    Ok(scene)
}

/// Validate and encode before touching the old file, then atomically replace it in the same folder.
pub fn save(path: impl AsRef<Path>, scene: &Scene) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(&scene.to_value()?)?;
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let leaf = path
        .file_name()
        .context("tank scene destination must have a filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{leaf}.{}.{nonce}.tmp", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.with_context(|| format!("save tank scene {}", path.display()))
}

/// Only Tiger I uses the existing full Game dressing consumer. The caller names its checkout's
/// destination explicitly; another vehicle can never silently replace the Tiger's props.
pub fn sync_tiger_dressing(path: impl AsRef<Path>, scene: &Scene) -> anyhow::Result<()> {
    ensure!(
        scene.tank == TankAssetId::TigerI,
        "only Tiger I supports full-game dressing sync"
    );
    scene.validate()?;
    crate::tank_dressing::save(path, &scene.dressing)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "loads the three large purchased full-interior source exports"]
    fn imported_editor_sources_load_exact_geometry() {
        for (id, triangles, approximate_height) in [
            (TankAssetId::TigerII, 1_136_835, 3.94),
            (TankAssetId::Is2, 804_467, 3.382),
            (TankAssetId::PanzerIV, 616_690, 2.820),
        ] {
            let preview = load_preview(id).unwrap();
            assert_eq!(preview.visible_parts, [true, false, false]);
            assert_eq!(preview.meshes[0].indices.len(), triangles * 3, "{}", id.as_str());
            assert!(preview.meshes[0].min.y.abs() < 0.002);
            assert!((preview.meshes[0].max.y - approximate_height).abs() < 0.02);
            assert!(!id.descriptor().articulated && !id.descriptor().full_game_playable);
        }
    }

    fn tank() -> Tank {
        let h = crate::terrain::Heightfield {
            heights: vec![0.0; 9],
            res: 3,
            spacing: 100.0,
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(),
        };
        Tank::new(2.0, 3.0, 0.6, &h, crate::tank::TIGER_PARTS)
    }
    fn lamp(frame: Frame) -> AttachedLight {
        AttachedLight {
            frame,
            light: AuthoredLight::point_preset("cabin".into(), "work", Vec3::new(0.2, 0.4, -0.3)),
        }
    }
    #[test]
    fn tank_local_lamp_roundtrip_and_turret_motion() {
        let mut tank = tank();
        let local = lamp(Frame::Turret);
        let initial = local.world_light(&tank);
        let back = AttachedLight::from_world(Frame::Turret, &tank, initial.clone());
        assert!(back.light.position.distance(local.light.position) < 0.0001);
        tank.turret_yaw += 0.7;
        let moved = local.world_light(&tank);
        assert!(moved.position.distance(initial.position) > 0.01);
        assert!(
            moved
                .position
                .distance(tank.turret_model().transform_point3(local.light.position))
                < 0.0001
        );
    }
    #[test]
    fn scene_roundtrip_preserves_dressing_light_frame_and_disabled_light() {
        let mut scene = Scene::new(TankAssetId::TigerI);
        let mut source = lamp(Frame::Gun);
        source.light.enabled = false;
        scene.lights.push(source);
        scene.dressing.props.push(crate::tank_dressing::Prop {
            asset: "assets/tanks/tiger/baked/tiger_gun.glb".into(),
            frame: Frame::Gun,
            local_translation_m: Vec3::new(0.1, 0.2, 0.3),
            local_rotation_deg_xyz: Vec3::new(10.0, 20.0, 30.0),
            scale: Vec3::ONE,
        });
        assert_eq!(
            Scene::from_value(&scene.to_value().unwrap()).unwrap(),
            scene
        );
    }
    #[test]
    fn invalid_or_cross_vehicle_document_never_replaces_saved_scene() {
        let folder = std::env::temp_dir().join(format!(
            "tank-editor-scene-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = folder.join("scene.json");
        let scene = Scene::new(TankAssetId::TigerI);
        save(&path, &scene).unwrap();
        assert!(load_or_new(&path, TankAssetId::TigerII).is_err());
        let original = std::fs::read(&path).unwrap();
        let mut bad = scene.clone();
        bad.lighting.fog_density = f32::NAN;
        assert!(save(&path, &bad).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(load_or_new(&path, TankAssetId::TigerI).unwrap(), scene);
        assert!(sync_tiger_dressing(
            folder.join("must-not-exist.json"),
            &Scene::new(TankAssetId::Is2)
        )
        .is_err());
        assert!(!folder.join("must-not-exist.json").exists());
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn unsupported_hinge_frame_duplicate_lights_and_unknown_versions_rejected() {
        let mut scene = Scene::new(TankAssetId::TigerII);
        scene.lights.push(lamp(Frame::Turret));
        assert!(scene.validate().is_err());
        scene.tank = TankAssetId::TigerI;
        scene.lights.push(lamp(Frame::Hull));
        assert!(scene.validate().is_err());
        let mut value = Scene::new(TankAssetId::TigerI).to_value().unwrap();
        value["version"] = json!(2);
        assert!(Scene::from_value(&value).is_err());
    }
}
