//! Persistent props authored relative to the player's articulated Tiger rig.
//!
//! World-map placements are intentionally ground-relative. Tank dressing is different: a map case
//! on the hull must follow the vehicle, a cupola prop must follow turret traverse, and anything
//! mounted on the breech/gun group must follow gun elevation. This small, versioned file is the
//! shared serialization and transform seam for the editor and game.

use std::ffi::OsStr;
use std::path::{Component, Path};

use anyhow::Context;
use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::tank::Tank;

pub const TIGER_DRESSING_SCHEMA: &str = "kessel.tiger-dressing";
pub const TIGER_DRESSING_VERSION: u64 = 1;
pub const TIGER_DRESSING_PATH: &str = "assets/tanks/tiger/dressing.v1.json";

const MAX_PROPS: usize = 4096;
const MAX_ASSET_PATH_BYTES: usize = 1024;
const MAX_LOCAL_TRANSLATION_METRES: f32 = 100.0;
const MAX_ABS_ROTATION_DEGREES: f32 = 1_000_000.0;
const MIN_SCALE: f32 = 0.001;
const MAX_SCALE: f32 = 100.0;

/// The articulated Tiger part that owns a prop's local transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Hull,
    Turret,
    Gun,
}

impl Frame {
    pub const ALL: [Self; 3] = [Self::Hull, Self::Turret, Self::Gun];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hull => "hull",
            Self::Turret => "turret",
            Self::Gun => "gun",
        }
    }

    fn parse(value: &serde_json::Value, context: &str) -> anyhow::Result<Self> {
        match value.as_str() {
            Some("hull") => Ok(Self::Hull),
            Some("turret") => Ok(Self::Turret),
            Some("gun") => Ok(Self::Gun),
            Some(other) => {
                anyhow::bail!("{context}.frame is {other:?}; expected hull, turret, or gun")
            }
            None => anyhow::bail!("{context}.frame must be a string"),
        }
    }

    /// Authoritative parent transform. Gun dressing follows lay, but not presentation-only recoil.
    pub fn parent_model(self, tank: &Tank) -> Mat4 {
        match self {
            Self::Hull => tank.hull_model(),
            Self::Turret => tank.turret_model(),
            Self::Gun => tank.gun_model(),
        }
    }

    /// Render parent transform. A prop physically fixed to the gun group follows visible recoil.
    pub fn visual_parent_model(self, tank: &Tank) -> Mat4 {
        match self {
            Self::Gun => tank.visual_gun_model(),
            _ => self.parent_model(tank),
        }
    }

    /// Convert a world transform into this articulated frame for editor placement.
    pub fn local_model_from_world(self, tank: &Tank, world_model: Mat4) -> Mat4 {
        self.parent_model(tank).inverse() * world_model
    }
}

/// One user-authored GLB attached to an articulated Tiger frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub asset: String,
    pub frame: Frame,
    pub local_translation_m: Vec3,
    /// Intrinsic XYZ Euler rotation in degrees, converted with [`EulerRot::XYZ`].
    pub local_rotation_deg_xyz: Vec3,
    pub scale: Vec3,
}

impl Prop {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_asset_path(&self.asset)?;
        validate_finite_vec3(self.local_translation_m, "local_translation_m")?;
        anyhow::ensure!(
            self.local_translation_m.abs().max_element() <= MAX_LOCAL_TRANSLATION_METRES,
            "local_translation_m must stay within +/-{MAX_LOCAL_TRANSLATION_METRES} m of its Tiger frame"
        );
        validate_finite_vec3(self.local_rotation_deg_xyz, "local_rotation_deg_xyz")?;
        anyhow::ensure!(
            self.local_rotation_deg_xyz.abs().max_element() <= MAX_ABS_ROTATION_DEGREES,
            "local_rotation_deg_xyz is too large"
        );
        validate_finite_vec3(self.scale, "scale")?;
        anyhow::ensure!(
            self.scale.cmpge(Vec3::splat(MIN_SCALE)).all()
                && self.scale.cmple(Vec3::splat(MAX_SCALE)).all(),
            "scale components must be in {MIN_SCALE}..={MAX_SCALE}"
        );
        Ok(())
    }

    pub fn local_rotation(&self) -> Quat {
        let radians = Vec3::new(
            self.local_rotation_deg_xyz.x.to_radians(),
            self.local_rotation_deg_xyz.y.to_radians(),
            self.local_rotation_deg_xyz.z.to_radians(),
        );
        Quat::from_euler(EulerRot::XYZ, radians.x, radians.y, radians.z)
    }

    pub fn local_model(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.scale,
            self.local_rotation(),
            self.local_translation_m,
        )
    }

    /// Authoritative world transform, suitable for collision and editor coordinate conversion.
    pub fn world_model(&self, tank: &Tank) -> Mat4 {
        self.frame.parent_model(tank) * self.local_model()
    }

    /// Presentation transform, suitable for rendering. Only gun-frame props differ during recoil.
    pub fn visual_world_model(&self, tank: &Tank) -> Mat4 {
        self.frame.visual_parent_model(tank) * self.local_model()
    }

    fn from_value(value: &serde_json::Value, index: usize) -> anyhow::Result<Self> {
        let context = format!("props[{index}]");
        anyhow::ensure!(value.is_object(), "{context} must be an object");
        let asset = value["asset"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{context}.asset must be a string"))?
            .to_owned();
        let prop = Self {
            asset,
            frame: Frame::parse(&value["frame"], &context)?,
            local_translation_m: parse_vec3(
                &value["local_translation_m"],
                &format!("{context}.local_translation_m"),
            )?,
            local_rotation_deg_xyz: parse_vec3(
                &value["local_rotation_deg_xyz"],
                &format!("{context}.local_rotation_deg_xyz"),
            )?,
            scale: parse_vec3(&value["scale"], &format!("{context}.scale"))?,
        };
        prop.validate()
            .with_context(|| format!("invalid {context}"))?;
        Ok(prop)
    }

    fn to_value(&self) -> serde_json::Value {
        serde_json::json!({
            "asset": self.asset,
            "frame": self.frame.as_str(),
            "local_translation_m": vec3_value(self.local_translation_m),
            "local_rotation_deg_xyz": vec3_value(self.local_rotation_deg_xyz),
            "scale": vec3_value(self.scale),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TigerDressing {
    pub props: Vec<Prop>,
}

impl TigerDressing {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.props.len() <= MAX_PROPS,
            "Tiger dressing contains {} props; maximum is {MAX_PROPS}",
            self.props.len()
        );
        for (index, prop) in self.props.iter().enumerate() {
            prop.validate()
                .with_context(|| format!("invalid props[{index}]"))?;
        }
        Ok(())
    }

    pub fn from_value(value: &serde_json::Value) -> anyhow::Result<Self> {
        anyhow::ensure!(value.is_object(), "Tiger dressing root must be an object");
        anyhow::ensure!(
            value["schema"].as_str() == Some(TIGER_DRESSING_SCHEMA),
            "Tiger dressing schema must be {TIGER_DRESSING_SCHEMA:?}"
        );
        anyhow::ensure!(
            value["version"].as_u64() == Some(TIGER_DRESSING_VERSION),
            "Tiger dressing version must be {TIGER_DRESSING_VERSION}"
        );
        let values = value["props"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Tiger dressing props must be an array"))?;
        anyhow::ensure!(
            values.len() <= MAX_PROPS,
            "Tiger dressing contains {} props; maximum is {MAX_PROPS}",
            values.len()
        );
        let props = values
            .iter()
            .enumerate()
            .map(|(index, prop)| Prop::from_value(prop, index))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let dressing = Self { props };
        dressing.validate()?;
        Ok(dressing)
    }

    pub fn to_value(&self) -> anyhow::Result<serde_json::Value> {
        self.validate()?;
        Ok(serde_json::json!({
            "schema": TIGER_DRESSING_SCHEMA,
            "version": TIGER_DRESSING_VERSION,
            "props": self.props.iter().map(Prop::to_value).collect::<Vec<_>>(),
        }))
    }
}

/// Load a dressing file. A missing file is a valid, empty authored state; malformed files are not.
pub fn load_optional(path: impl AsRef<Path>) -> anyhow::Result<TigerDressing> {
    let path = path.as_ref();
    let encoded = match std::fs::read_to_string(path) {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TigerDressing::default())
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let value: serde_json::Value =
        serde_json::from_str(&encoded).with_context(|| format!("parse {}", path.display()))?;
    TigerDressing::from_value(&value).with_context(|| format!("validate {}", path.display()))
}

pub fn load_default() -> anyhow::Result<TigerDressing> {
    load_optional(crate::resource::resolve(TIGER_DRESSING_PATH)?)
}

pub fn save(path: impl AsRef<Path>, dressing: &TigerDressing) -> anyhow::Result<()> {
    let path = path.as_ref();
    let value = dressing.to_value()?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut encoded = serde_json::to_string_pretty(&value)?;
    encoded.push('\n');
    std::fs::write(path, encoded).with_context(|| format!("write {}", path.display()))
}

pub fn save_default(dressing: &TigerDressing) -> anyhow::Result<()> {
    save(crate::resource::resolve(TIGER_DRESSING_PATH)?, dressing)
}

fn parse_vec3(value: &serde_json::Value, field: &str) -> anyhow::Result<Vec3> {
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{field} must be a three-number array"))?;
    anyhow::ensure!(
        values.len() == 3,
        "{field} must contain exactly three numbers"
    );
    let component = |index: usize| {
        values[index]
            .as_f64()
            .map(|component| component as f32)
            .ok_or_else(|| anyhow::anyhow!("{field}[{index}] must be a number"))
    };
    let vector = Vec3::new(component(0)?, component(1)?, component(2)?);
    validate_finite_vec3(vector, field)?;
    Ok(vector)
}

fn vec3_value(value: Vec3) -> serde_json::Value {
    serde_json::json!([value.x, value.y, value.z])
}

fn validate_finite_vec3(value: Vec3, field: &str) -> anyhow::Result<()> {
    anyhow::ensure!(value.is_finite(), "{field} components must be finite");
    Ok(())
}

fn validate_asset_path(asset: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!asset.is_empty(), "asset path cannot be empty");
    anyhow::ensure!(
        asset.trim() == asset,
        "asset path cannot have surrounding whitespace"
    );
    anyhow::ensure!(
        asset.len() <= MAX_ASSET_PATH_BYTES,
        "asset path is longer than {MAX_ASSET_PATH_BYTES} bytes"
    );
    anyhow::ensure!(
        !asset.contains('\\') && !asset.chars().any(char::is_control),
        "asset path must use clean forward-slash repository syntax"
    );
    let path = Path::new(asset);
    anyhow::ensure!(
        !path.is_absolute(),
        "asset path must be repository-relative"
    );
    anyhow::ensure!(
        path.components()
            .next()
            .is_some_and(|component| component == Component::Normal(OsStr::new("assets"))),
        "asset path must begin with assets/"
    );
    anyhow::ensure!(
        !path.components().any(|component| matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )),
        "asset path cannot escape the repository asset root"
    );
    anyhow::ensure!(
        path.extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("glb")),
        "asset path must reference a .glb file"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tank::TIGER_PARTS;
    use crate::terrain::Heightfield;

    fn flat() -> Heightfield {
        Heightfield {
            res: 2,
            spacing: 1000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
        }
    }

    fn sample_prop(frame: Frame) -> Prop {
        Prop {
            asset: "assets/pizza/_extracted/test/field_radio.glb".into(),
            frame,
            local_translation_m: Vec3::new(0.4, 1.2, -0.7),
            local_rotation_deg_xyz: Vec3::new(12.0, -34.0, 5.0),
            scale: Vec3::new(0.8, 0.9, 1.1),
        }
    }

    fn approx_vec3(actual: Vec3, expected: Vec3) {
        assert!(
            actual.distance(expected) < 1.0e-5,
            "actual={actual:?}, expected={expected:?}"
        );
    }

    #[test]
    fn json_and_file_roundtrip_preserve_authored_props() {
        let dressing = TigerDressing {
            props: Frame::ALL.into_iter().map(sample_prop).collect(),
        };
        let value = dressing.to_value().expect("encode value");
        assert_eq!(value["schema"], TIGER_DRESSING_SCHEMA);
        assert_eq!(value["version"], TIGER_DRESSING_VERSION);
        assert_eq!(
            TigerDressing::from_value(&value).expect("decode value"),
            dressing
        );

        let dir =
            std::env::temp_dir().join(format!("kessel-tiger-dressing-test-{}", std::process::id()));
        let path = dir.join("roundtrip.json");
        let _ = std::fs::remove_file(&path);
        save(&path, &dressing).expect("save dressing");
        assert_eq!(load_optional(&path).expect("load dressing"), dressing);
        std::fs::remove_file(path).expect("remove test file");
        std::fs::remove_dir(dir).expect("remove test directory");
    }

    #[test]
    fn absent_optional_file_is_an_empty_dressing() {
        let path = std::env::temp_dir().join(format!(
            "kessel-tiger-dressing-definitely-absent-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            load_optional(path).expect("missing is valid"),
            TigerDressing::default()
        );
    }

    #[test]
    fn malformed_or_unsafe_props_are_rejected() {
        let mut prop = sample_prop(Frame::Hull);
        prop.asset = "assets/../outside.glb".into();
        assert!(prop.validate().is_err());
        prop = sample_prop(Frame::Hull);
        prop.scale.x = 0.0;
        assert!(prop.validate().is_err());
        prop = sample_prop(Frame::Hull);
        prop.local_translation_m.y = f32::NAN;
        assert!(prop.validate().is_err());

        let wrong_version = serde_json::json!({
            "schema": TIGER_DRESSING_SCHEMA,
            "version": TIGER_DRESSING_VERSION + 1,
            "props": [],
        });
        assert!(TigerDressing::from_value(&wrong_version).is_err());
    }

    #[test]
    fn each_frame_follows_only_its_articulated_parent() {
        let mut tank = Tank::new(8.0, -13.0, 0.35, &flat(), TIGER_PARTS);
        let local = Vec3::new(0.3, 0.2, 1.4);
        let prop = |frame| Prop {
            local_translation_m: local,
            local_rotation_deg_xyz: Vec3::ZERO,
            scale: Vec3::ONE,
            ..sample_prop(frame)
        };

        for frame in Frame::ALL {
            let attached = prop(frame);
            approx_vec3(
                attached.world_model(&tank).transform_point3(Vec3::ZERO),
                frame.parent_model(&tank).transform_point3(local),
            );
        }

        let hull_before = prop(Frame::Hull)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        let turret_before = prop(Frame::Turret)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        tank.turret_yaw += 0.8;
        let hull_after = prop(Frame::Hull)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        let turret_after = prop(Frame::Turret)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        approx_vec3(hull_after, hull_before);
        assert!(turret_after.distance(turret_before) > 0.25);

        let gun_before = prop(Frame::Gun)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        tank.gun_pitch = 0.2;
        let gun_after = prop(Frame::Gun)
            .world_model(&tank)
            .transform_point3(Vec3::ZERO);
        assert!(gun_after.distance(gun_before) > 0.1);

        tank.pos += Vec3::new(17.0, 2.0, -9.0);
        tank.heading -= 0.55;
        for frame in Frame::ALL {
            let attached = prop(frame);
            approx_vec3(
                attached.world_model(&tank).transform_point3(Vec3::ZERO),
                frame.parent_model(&tank).transform_point3(local),
            );
        }
    }

    #[test]
    fn world_to_local_helper_inverts_the_selected_parent() {
        let mut tank = Tank::new(-4.0, 9.0, -0.7, &flat(), TIGER_PARTS);
        tank.turret_yaw = 0.41;
        tank.gun_pitch = -0.08;
        let local = Mat4::from_scale_rotation_translation(
            Vec3::splat(0.7),
            Quat::from_rotation_y(0.22),
            Vec3::new(-0.5, 1.1, 0.8),
        );
        for frame in Frame::ALL {
            let world = frame.parent_model(&tank) * local;
            let recovered = frame.local_model_from_world(&tank, world);
            for point in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                approx_vec3(
                    recovered.transform_point3(point),
                    local.transform_point3(point),
                );
            }
        }
    }
}
