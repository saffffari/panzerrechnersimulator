//! Renderer-neutral environment authoring data and deterministic dressing generation.
//!
//! The editor owns these records; the runtime/world-generation lane can consume the same JSON
//! without inheriting editor UI or GPU code.  The important contract is procedural-first and
//! non-destructive: a seeded recipe materializes ordinary placements, then artists edit those
//! placements exactly like hand-authored props while the recipe/provenance remains in the overlay.

use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path};

use glam::{Vec2, Vec3};

pub const LIGHTS_KEY: &str = "authored_lights";
pub const LIGHTING_KEY: &str = "environment_lighting";
pub const LAYERS_KEY: &str = "procedural_dressing_layers";
pub const SURFACE_ZONES_KEY: &str = "surface_material_zones";
pub const SCENE_RECIPES_KEY: &str = "scene_recipe_instances";
pub const INFRASTRUCTURE_RUNS_KEY: &str = "infrastructure_runs";
pub const SCHEMA: &str = "kessel.environment-authoring";
pub const VERSION: u64 = 1;

pub const MAX_SURFACE_ZONES: usize = 4_096;
pub const MAX_SCENE_INSTANCES: usize = 1_024;
pub const MAX_INFRASTRUCTURE_RUNS: usize = 512;
const UTILITY_ATTACHMENT_HEIGHT_M: f32 = 5.72;

fn validate_repo_relative_path(path: &str, label: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!path.trim().is_empty(), "{label} path is empty");
    let path = Path::new(path);
    anyhow::ensure!(
        !path.is_absolute(),
        "{label} path must be repository-relative"
    );
    anyhow::ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "{label} path contains an unsafe component"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    Point,
    Spot,
    Area,
}

impl LightKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Spot => "spot",
            Self::Area => "area",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "point" => Some(Self::Point),
            "spot" => Some(Self::Spot),
            "area" => Some(Self::Area),
            _ => None,
        }
    }
}

/// One authored practical light.  `range_m` is an authoring/culling radius, not a fake physical
/// cutoff: the current renderer previews point transport with inverse-square falloff.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthoredLight {
    pub id: String,
    pub kind: LightKind,
    pub position: Vec3,
    pub direction: Vec3,
    pub temperature_k: f32,
    pub tint_rgb: Vec3,
    pub radiant_intensity: f32,
    pub source_radius_m: f32,
    pub range_m: f32,
    pub cone_angle_deg: f32,
    pub flicker: f32,
    pub enabled: bool,
    pub deleted: bool,
}

impl AuthoredLight {
    pub fn point_preset(id: String, preset: &str, position: Vec3) -> Self {
        let (temperature_k, tint_rgb, radiant_intensity, source_radius_m, range_m, flicker) =
            match preset {
                "fire" => (1_850.0, Vec3::new(1.0, 0.72, 0.46), 95.0, 0.28, 24.0, 0.30),
                "work" => (3_200.0, Vec3::new(1.0, 0.92, 0.78), 135.0, 0.10, 30.0, 0.03),
                "cold" => (7_200.0, Vec3::new(0.70, 0.84, 1.0), 110.0, 0.16, 34.0, 0.02),
                "red" => (2_000.0, Vec3::new(1.0, 0.12, 0.06), 80.0, 0.08, 20.0, 0.08),
                _ => (4_500.0, Vec3::ONE, 100.0, 0.12, 28.0, 0.0),
            };
        Self {
            id,
            kind: LightKind::Point,
            position,
            direction: Vec3::NEG_Y,
            temperature_k,
            tint_rgb,
            radiant_intensity,
            source_radius_m,
            range_m,
            cone_angle_deg: 45.0,
            flicker,
            enabled: true,
            deleted: false,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty(), "light id is empty");
        anyhow::ensure!(
            self.position.is_finite() && self.direction.is_finite(),
            "light {} has a non-finite transform",
            self.id
        );
        if self.kind != LightKind::Point {
            anyhow::ensure!(
                self.direction.length_squared() > 1.0e-6,
                "light {} direction is zero",
                self.id
            );
        }
        anyhow::ensure!(
            (800.0..=40_000.0).contains(&self.temperature_k),
            "light {} temperature must be 800..40000 K",
            self.id
        );
        anyhow::ensure!(
            self.tint_rgb.is_finite() && self.tint_rgb.min_element() >= 0.0,
            "light {} tint must be finite and non-negative",
            self.id
        );
        anyhow::ensure!(
            self.radiant_intensity.is_finite() && self.radiant_intensity >= 0.0,
            "light {} intensity must be finite and non-negative",
            self.id
        );
        anyhow::ensure!(
            self.source_radius_m.is_finite() && (0.01..=100.0).contains(&self.source_radius_m),
            "light {} source radius must be 0.01..100 m",
            self.id
        );
        anyhow::ensure!(
            self.range_m.is_finite() && (0.25..=10_000.0).contains(&self.range_m),
            "light {} range must be 0.25..10000 m",
            self.id
        );
        anyhow::ensure!(
            self.cone_angle_deg.is_finite() && (1.0..=179.0).contains(&self.cone_angle_deg),
            "light {} cone angle must be 1..179 degrees",
            self.id
        );
        anyhow::ensure!(
            self.flicker.is_finite() && (0.0..=1.0).contains(&self.flicker),
            "light {} flicker must be 0..1",
            self.id
        );
        Ok(())
    }

    /// Approximate black-body color multiplied by the authored gel/tint, converted to linear RGB.
    pub fn linear_rgb(&self) -> Vec3 {
        let kelvin = kelvin_srgb(self.temperature_k);
        Vec3::new(
            srgb_to_linear(kelvin.x),
            srgb_to_linear(kelvin.y),
            srgb_to_linear(kelvin.z),
        ) * self.tint_rgb
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "kind": self.kind.as_str(),
            "position": [self.position.x, self.position.y, self.position.z],
            "direction": [self.direction.x, self.direction.y, self.direction.z],
            "temperature_k": self.temperature_k,
            "tint_rgb": [self.tint_rgb.x, self.tint_rgb.y, self.tint_rgb.z],
            "radiant_intensity": self.radiant_intensity,
            "source_radius_m": self.source_radius_m,
            "range_m": self.range_m,
            "cone_angle_deg": self.cone_angle_deg,
            "flicker": self.flicker,
            "enabled": self.enabled,
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let id = value["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("authored light is missing id"))?
            .to_owned();
        let kind = LightKind::parse(value["kind"].as_str().unwrap_or("point"))
            .ok_or_else(|| anyhow::anyhow!("light {id} has an unknown kind"))?;
        let position = json_vec3(&value["position"], "position")?;
        let direction = json_vec3_or(&value["direction"], Vec3::NEG_Y);
        let light = Self {
            id,
            kind,
            position,
            direction,
            temperature_k: json_f32_or(&value["temperature_k"], 4_500.0),
            tint_rgb: json_vec3_or(&value["tint_rgb"], Vec3::ONE),
            radiant_intensity: json_f32_or(&value["radiant_intensity"], 100.0),
            source_radius_m: json_f32_or(&value["source_radius_m"], 0.12),
            range_m: json_f32_or(&value["range_m"], 28.0),
            cone_angle_deg: json_f32_or(&value["cone_angle_deg"], 45.0),
            flicker: json_f32_or(&value["flicker"], 0.0),
            enabled: value["enabled"].as_bool().unwrap_or(true),
            deleted: false,
        };
        light.validate()?;
        Ok(light)
    }
}

pub fn lights_from_edits(edits: Option<&serde_json::Value>) -> anyhow::Result<Vec<AuthoredLight>> {
    let Some(value) = edits.and_then(|edits| edits.get(LIGHTS_KEY)) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{LIGHTS_KEY} must be an array"))?;
    let mut ids = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let light = AuthoredLight::from_json(value)?;
            anyhow::ensure!(
                ids.insert(light.id.clone()),
                "duplicate light id {}",
                light.id
            );
            Ok(light)
        })
        .collect()
}

pub fn lights_to_json(lights: &[AuthoredLight]) -> Vec<serde_json::Value> {
    lights
        .iter()
        .filter(|light| !light.deleted)
        .map(AuthoredLight::to_json)
        .collect()
}

/// Camera-relative importance for the editor's bounded practical-light preview. `range_m` is an
/// authoring/culling radius: lights outside it do not occupy one of the renderer's scarce slots,
/// while sources near the camera are ranked by their approximate incident radiance.
pub fn preview_light_score(light: &AuthoredLight, camera_position: Vec3) -> Option<f32> {
    if light.deleted || !light.enabled || light.radiant_intensity <= 0.0 {
        return None;
    }
    let distance_squared = light.position.distance_squared(camera_position);
    let range_squared = light.range_m * light.range_m;
    if distance_squared >= range_squared {
        return None;
    }
    let range_fraction = (distance_squared / range_squared.max(0.001)).clamp(0.0, 1.0);
    let range_fade = (1.0 - range_fraction * range_fraction).powi(2);
    let regularized_distance_squared =
        distance_squared.max(light.source_radius_m * light.source_radius_m);
    Some(
        light.radiant_intensity * light.linear_rgb().max_element() * range_fade
            / regularized_distance_squared.max(0.000_1),
    )
}

/// Global preview/tuning controls.  They are optional so an untouched map keeps the shipping
/// campaign atmosphere exactly; enabling this record is an explicit authoring decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightingTuning {
    pub enabled: bool,
    pub sun_azimuth_deg: f32,
    pub sun_elevation_deg: f32,
    pub camera_exposure: f32,
    pub fog_density: f32,
    pub overcast: f32,
    pub storm: f32,
    pub twilight: f32,
}

impl Default for LightingTuning {
    fn default() -> Self {
        Self {
            enabled: false,
            sun_azimuth_deg: 225.0,
            sun_elevation_deg: 28.0,
            camera_exposure: 0.55,
            fog_density: 0.02,
            overcast: 0.90,
            storm: 0.72,
            twilight: 0.0,
        }
    }
}

impl LightingTuning {
    pub fn from_live(
        sun_direction: Vec3,
        camera_exposure: f32,
        fog_density: f32,
        overcast: f32,
        storm: f32,
        twilight: f32,
    ) -> Self {
        let direction = sun_direction.normalize_or_zero();
        Self {
            enabled: true,
            sun_azimuth_deg: direction
                .z
                .atan2(direction.x)
                .to_degrees()
                .rem_euclid(360.0),
            sun_elevation_deg: direction.y.asin().to_degrees().clamp(-8.0, 89.0),
            camera_exposure: camera_exposure.clamp(0.02, 8.0),
            fog_density: fog_density.clamp(0.0, 0.25),
            overcast: overcast.clamp(0.0, 1.0),
            storm: storm.clamp(0.0, 1.0),
            twilight: twilight.clamp(-1.0, 1.0),
        }
    }

    pub fn sun_direction(self) -> Vec3 {
        let azimuth = self.sun_azimuth_deg.to_radians();
        let elevation = self.sun_elevation_deg.to_radians();
        Vec3::new(
            elevation.cos() * azimuth.cos(),
            elevation.sin(),
            elevation.cos() * azimuth.sin(),
        )
        .normalize()
    }

    pub fn validate(self) -> anyhow::Result<()> {
        for (name, value) in [
            ("sun_azimuth_deg", self.sun_azimuth_deg),
            ("sun_elevation_deg", self.sun_elevation_deg),
            ("camera_exposure", self.camera_exposure),
            ("fog_density", self.fog_density),
            ("overcast", self.overcast),
            ("storm", self.storm),
            ("twilight", self.twilight),
        ] {
            anyhow::ensure!(value.is_finite(), "lighting {name} is not finite");
        }
        anyhow::ensure!((-8.0..=89.0).contains(&self.sun_elevation_deg));
        anyhow::ensure!((0.02..=8.0).contains(&self.camera_exposure));
        anyhow::ensure!((0.0..=0.25).contains(&self.fog_density));
        anyhow::ensure!((0.0..=1.0).contains(&self.overcast));
        anyhow::ensure!((0.0..=1.0).contains(&self.storm));
        anyhow::ensure!((-1.0..=1.0).contains(&self.twilight));
        Ok(())
    }

    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "enabled": self.enabled,
            "sun_azimuth_deg": self.sun_azimuth_deg,
            "sun_elevation_deg": self.sun_elevation_deg,
            "camera_exposure": self.camera_exposure,
            "fog_density": self.fog_density,
            "overcast": self.overcast,
            "storm": self.storm,
            "twilight": self.twilight,
        })
    }

    pub fn from_edits(edits: Option<&serde_json::Value>) -> anyhow::Result<Self> {
        let Some(value) = edits.and_then(|edits| edits.get(LIGHTING_KEY)) else {
            return Ok(Self::default());
        };
        let tuning = Self {
            enabled: value["enabled"].as_bool().unwrap_or(false),
            sun_azimuth_deg: json_f32_or(&value["sun_azimuth_deg"], 225.0),
            sun_elevation_deg: json_f32_or(&value["sun_elevation_deg"], 28.0),
            camera_exposure: json_f32_or(&value["camera_exposure"], 0.55),
            fog_density: json_f32_or(&value["fog_density"], 0.02),
            overcast: json_f32_or(&value["overcast"], 0.90),
            storm: json_f32_or(&value["storm"], 0.72),
            twilight: json_f32_or(&value["twilight"], 0.0),
        };
        tuning.validate()?;
        Ok(tuning)
    }
}

/// Authoring intent for one terrain-projected material treatment. The graphics lane may realize
/// these as decals, virtual-texture layers, or terrain splat weights; the editor deliberately saves
/// a renderer-neutral record so painting does not lock the world format to today's shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRole {
    MaterialMix,
    Mud,
    Scorch,
    FrozenBlood,
}

impl SurfaceRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaterialMix => "material_mix",
            Self::Mud => "mud",
            Self::Scorch => "scorch",
            Self::FrozenBlood => "frozen_blood",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "material_mix" => Some(Self::MaterialMix),
            "mud" => Some(Self::Mud),
            "scorch" => Some(Self::Scorch),
            "frozen_blood" => Some(Self::FrozenBlood),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceLayer {
    /// Empty only for semantic layers such as scorch or frozen blood that the runtime synthesizes.
    pub texture: Option<String>,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceMaterialZone {
    pub id: String,
    pub role: SurfaceRole,
    pub center: Vec3,
    pub radius_m: f32,
    pub falloff: f32,
    pub rotation_deg: f32,
    pub tint_rgb: Vec3,
    pub opacity: f32,
    pub wetness: f32,
    pub snow_cover: f32,
    pub layers: Vec<SurfaceLayer>,
    pub deleted: bool,
}

impl SurfaceMaterialZone {
    pub fn preset(id: String, preset: &str, center: Vec3, texture: Option<String>) -> Self {
        let (role, radius_m, falloff, tint_rgb, opacity, wetness, snow_cover) = match preset {
            "mud_snow" => (
                SurfaceRole::Mud,
                8.0,
                0.42,
                Vec3::new(0.30, 0.24, 0.18),
                0.72,
                0.78,
                0.28,
            ),
            "scorch" => (
                SurfaceRole::Scorch,
                5.0,
                0.58,
                Vec3::new(0.075, 0.060, 0.048),
                0.82,
                0.08,
                0.10,
            ),
            "frozen_blood" => (
                SurfaceRole::FrozenBlood,
                1.25,
                0.62,
                Vec3::new(0.34, 0.012, 0.009),
                0.76,
                0.18,
                0.46,
            ),
            _ => (
                SurfaceRole::MaterialMix,
                10.0,
                0.40,
                Vec3::ONE,
                0.70,
                0.20,
                0.0,
            ),
        };
        let layers = texture
            .map(|texture| {
                vec![SurfaceLayer {
                    texture: Some(texture),
                    weight: 1.0,
                }]
            })
            .unwrap_or_else(|| {
                vec![SurfaceLayer {
                    texture: None,
                    weight: 1.0,
                }]
            });
        Self {
            id,
            role,
            center,
            radius_m,
            falloff,
            rotation_deg: 0.0,
            tint_rgb,
            opacity,
            wetness,
            snow_cover,
            layers,
            deleted: false,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty(), "surface zone id is empty");
        anyhow::ensure!(
            self.center.is_finite(),
            "surface zone {} has a non-finite center",
            self.id
        );
        anyhow::ensure!(
            self.radius_m.is_finite() && (0.10..=2_000.0).contains(&self.radius_m),
            "surface zone {} radius must be 0.1..2000m",
            self.id
        );
        anyhow::ensure!(
            self.falloff.is_finite() && (0.0..=1.0).contains(&self.falloff),
            "surface zone {} falloff must be 0..1",
            self.id
        );
        anyhow::ensure!(
            self.rotation_deg.is_finite(),
            "surface zone {} rotation is not finite",
            self.id
        );
        anyhow::ensure!(
            self.tint_rgb.is_finite() && self.tint_rgb.min_element() >= 0.0,
            "surface zone {} tint must be finite and non-negative",
            self.id
        );
        for (name, value) in [
            ("opacity", self.opacity),
            ("wetness", self.wetness),
            ("snow_cover", self.snow_cover),
        ] {
            anyhow::ensure!(
                value.is_finite() && (0.0..=1.0).contains(&value),
                "surface zone {} {name} must be 0..1",
                self.id
            );
        }
        anyhow::ensure!(
            !self.layers.is_empty(),
            "surface zone {} has no layers",
            self.id
        );
        anyhow::ensure!(
            self.layers.len() <= 4,
            "surface zone {} has more than 4 layers",
            self.id
        );
        let mut paths = BTreeSet::new();
        let mut total = 0.0;
        for layer in &self.layers {
            anyhow::ensure!(layer.weight.is_finite() && layer.weight > 0.0);
            total += layer.weight;
            if let Some(texture) = &layer.texture {
                validate_repo_relative_path(texture, "surface texture")?;
                let extension = Path::new(texture)
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase);
                anyhow::ensure!(
                    matches!(extension.as_deref(), Some("png" | "jpg" | "jpeg")),
                    "surface texture must be a PNG or JPEG"
                );
                anyhow::ensure!(
                    paths.insert(texture.clone()),
                    "surface zone {} repeats {texture}",
                    self.id
                );
            }
        }
        anyhow::ensure!(total.is_finite() && total > 0.0);
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "id": self.id,
            "role": self.role.as_str(),
            "center": [self.center.x, self.center.y, self.center.z],
            "radius_m": self.radius_m,
            "falloff": self.falloff,
            "rotation_deg": self.rotation_deg,
            "tint_rgb": [self.tint_rgb.x, self.tint_rgb.y, self.tint_rgb.z],
            "opacity": self.opacity,
            "wetness": self.wetness,
            "snow_cover": self.snow_cover,
            "projection": "terrain",
            "layers": self.layers.iter().map(|layer| serde_json::json!({
                "texture": layer.texture,
                "weight": layer.weight,
            })).collect::<Vec<_>>(),
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let id = value["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("surface material zone is missing id"))?
            .to_owned();
        let role = SurfaceRole::parse(value["role"].as_str().unwrap_or("material_mix"))
            .ok_or_else(|| anyhow::anyhow!("surface zone {id} has an unknown role"))?;
        let layers = value["layers"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("surface zone {id} layers must be an array"))?
            .iter()
            .map(|layer| SurfaceLayer {
                texture: layer["texture"].as_str().map(str::to_owned),
                weight: json_f32_or(&layer["weight"], 1.0),
            })
            .collect();
        let zone = Self {
            id,
            role,
            center: json_vec3(&value["center"], "surface center")?,
            radius_m: json_f32_or(&value["radius_m"], 10.0),
            falloff: json_f32_or(&value["falloff"], 0.4),
            rotation_deg: json_f32_or(&value["rotation_deg"], 0.0),
            tint_rgb: json_vec3_or(&value["tint_rgb"], Vec3::ONE),
            opacity: json_f32_or(&value["opacity"], 0.7),
            wetness: json_f32_or(&value["wetness"], 0.2),
            snow_cover: json_f32_or(&value["snow_cover"], 0.0),
            layers,
            deleted: false,
        };
        zone.validate()?;
        Ok(zone)
    }
}

pub fn surface_zones_from_edits(
    edits: Option<&serde_json::Value>,
) -> anyhow::Result<Vec<SurfaceMaterialZone>> {
    let Some(value) = edits.and_then(|edits| edits.get(SURFACE_ZONES_KEY)) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{SURFACE_ZONES_KEY} must be an array"))?;
    let zones = values
        .iter()
        .map(SurfaceMaterialZone::from_json)
        .collect::<anyhow::Result<Vec<_>>>()?;
    validate_surface_zones(&zones)?;
    Ok(zones)
}

pub fn validate_surface_zones(zones: &[SurfaceMaterialZone]) -> anyhow::Result<()> {
    let active = zones.iter().filter(|zone| !zone.deleted);
    anyhow::ensure!(
        active.clone().count() <= MAX_SURFACE_ZONES,
        "{SURFACE_ZONES_KEY} exceeds the {MAX_SURFACE_ZONES} zone cap"
    );
    let mut ids = BTreeSet::new();
    for zone in active {
        zone.validate()?;
        anyhow::ensure!(
            ids.insert(zone.id.clone()),
            "duplicate surface zone id {}",
            zone.id
        );
    }
    Ok(())
}

pub fn surface_zones_to_json(zones: &[SurfaceMaterialZone]) -> Vec<serde_json::Value> {
    zones
        .iter()
        .filter(|zone| !zone.deleted)
        .map(SurfaceMaterialZone::to_json)
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneRecipeInstance {
    pub id: String,
    pub recipe_id: String,
    pub recipe_revision: u32,
    pub seed: u32,
    pub anchor: Vec3,
    pub yaw_deg: f32,
    pub resolved_assets: Vec<String>,
    pub generated_count: usize,
}

impl SceneRecipeInstance {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.id.trim().is_empty(),
            "scene recipe instance id is empty"
        );
        anyhow::ensure!(
            !self.recipe_id.trim().is_empty(),
            "scene recipe {} is missing recipe id",
            self.id
        );
        anyhow::ensure!(self.recipe_revision > 0);
        anyhow::ensure!(self.anchor.is_finite() && self.yaw_deg.is_finite());
        anyhow::ensure!(
            self.generated_count <= 8_192,
            "scene recipe {} exceeds the 8192 output cap",
            self.id
        );
        anyhow::ensure!(self.resolved_assets.len() <= 128);
        let mut paths = BTreeSet::new();
        for path in &self.resolved_assets {
            validate_repo_relative_path(path, "resolved scene asset")?;
            anyhow::ensure!(
                paths.insert(path.clone()),
                "scene recipe {} repeats {path}",
                self.id
            );
        }
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "id": self.id,
            "recipe_id": self.recipe_id,
            "recipe_revision": self.recipe_revision,
            "seed": self.seed,
            "anchor": [self.anchor.x, self.anchor.y, self.anchor.z],
            "yaw_deg": self.yaw_deg,
            "resolved_assets": self.resolved_assets,
            "generated_count": self.generated_count,
            "materialization": "ordinary-placement-ops",
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let instance = Self {
            id: value["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("scene recipe instance is missing id"))?
                .to_owned(),
            recipe_id: value["recipe_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("scene recipe instance is missing recipe_id"))?
                .to_owned(),
            recipe_revision: value["recipe_revision"].as_u64().unwrap_or(1) as u32,
            seed: value["seed"].as_u64().unwrap_or(0) as u32,
            anchor: json_vec3(&value["anchor"], "scene recipe anchor")?,
            yaw_deg: json_f32_or(&value["yaw_deg"], 0.0),
            resolved_assets: value["resolved_assets"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("scene recipe resolved_assets must be an array"))?
                .iter()
                .map(|path| {
                    path.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| anyhow::anyhow!("scene recipe asset path must be a string"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
            generated_count: value["generated_count"].as_u64().unwrap_or(0) as usize,
        };
        instance.validate()?;
        Ok(instance)
    }
}

pub fn scene_recipes_from_edits(
    edits: Option<&serde_json::Value>,
) -> anyhow::Result<Vec<SceneRecipeInstance>> {
    let Some(value) = edits.and_then(|edits| edits.get(SCENE_RECIPES_KEY)) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{SCENE_RECIPES_KEY} must be an array"))?;
    let instances = values
        .iter()
        .map(SceneRecipeInstance::from_json)
        .collect::<anyhow::Result<Vec<_>>>()?;
    validate_scene_recipes(&instances)?;
    Ok(instances)
}

pub fn validate_scene_recipes(instances: &[SceneRecipeInstance]) -> anyhow::Result<()> {
    anyhow::ensure!(
        instances.len() <= MAX_SCENE_INSTANCES,
        "{SCENE_RECIPES_KEY} exceeds the {MAX_SCENE_INSTANCES} instance cap"
    );
    let mut ids = BTreeSet::new();
    for instance in instances {
        instance.validate()?;
        anyhow::ensure!(
            ids.insert(instance.id.clone()),
            "duplicate scene recipe id {}",
            instance.id
        );
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureNode {
    pub id: String,
    pub position: Vec3,
    pub height_m: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CableSpan {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    pub asset: String,
    pub sag_m: f32,
    pub broken: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfrastructureRun {
    pub id: String,
    pub kind: String,
    pub nodes: Vec<InfrastructureNode>,
    pub spans: Vec<CableSpan>,
}

impl InfrastructureRun {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty() && !self.kind.trim().is_empty());
        anyhow::ensure!((2..=512).contains(&self.nodes.len()));
        anyhow::ensure!(self.spans.len() <= 1_536);
        let mut node_ids = BTreeSet::new();
        for node in &self.nodes {
            anyhow::ensure!(!node.id.trim().is_empty() && node_ids.insert(node.id.clone()));
            anyhow::ensure!(node.position.is_finite());
            anyhow::ensure!(node.height_m.is_finite() && (1.0..=80.0).contains(&node.height_m));
        }
        let mut span_ids = BTreeSet::new();
        let mut edges = BTreeSet::new();
        for span in &self.spans {
            anyhow::ensure!(!span.id.trim().is_empty() && span_ids.insert(span.id.clone()));
            anyhow::ensure!(node_ids.contains(&span.from_node) && node_ids.contains(&span.to_node));
            anyhow::ensure!(span.from_node != span.to_node);
            validate_repo_relative_path(&span.asset, "infrastructure span asset")?;
            anyhow::ensure!(span.sag_m.is_finite() && (0.0..=8.0).contains(&span.sag_m));
            let edge = if span.from_node < span.to_node {
                (span.from_node.clone(), span.to_node.clone())
            } else {
                (span.to_node.clone(), span.from_node.clone())
            };
            anyhow::ensure!(
                edges.insert(edge),
                "infrastructure run {} repeats a cable edge",
                self.id
            );
        }
        Ok(())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "id": self.id,
            "kind": self.kind,
            "nodes": self.nodes.iter().map(|node| serde_json::json!({
                "id": node.id,
                "position": [node.position.x, node.position.y, node.position.z],
                "height_m": node.height_m,
            })).collect::<Vec<_>>(),
            "spans": self.spans.iter().map(|span| serde_json::json!({
                "id": span.id,
                "from_node": span.from_node,
                "to_node": span.to_node,
                "asset": span.asset,
                "sag_m": span.sag_m,
                "broken": span.broken,
            })).collect::<Vec<_>>(),
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let nodes = value["nodes"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("infrastructure nodes must be an array"))?
            .iter()
            .map(|node| {
                Ok(InfrastructureNode {
                    id: node["id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("infrastructure node is missing id"))?
                        .to_owned(),
                    position: json_vec3(&node["position"], "infrastructure node position")?,
                    height_m: json_f32_or(&node["height_m"], 6.4),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let spans = value["spans"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("infrastructure spans must be an array"))?
            .iter()
            .map(|span| {
                Ok(CableSpan {
                    id: span["id"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cable span is missing id"))?
                        .to_owned(),
                    from_node: span["from_node"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cable span is missing from_node"))?
                        .to_owned(),
                    to_node: span["to_node"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cable span is missing to_node"))?
                        .to_owned(),
                    asset: span["asset"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("cable span is missing asset"))?
                        .to_owned(),
                    sag_m: json_f32_or(&span["sag_m"], 0.8),
                    broken: span["broken"].as_bool().unwrap_or(false),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let run = Self {
            id: value["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("infrastructure run is missing id"))?
                .to_owned(),
            kind: value["kind"].as_str().unwrap_or("utility").to_owned(),
            nodes,
            spans,
        };
        run.validate()?;
        Ok(run)
    }
}

pub fn infrastructure_runs_from_edits(
    edits: Option<&serde_json::Value>,
) -> anyhow::Result<Vec<InfrastructureRun>> {
    let Some(value) = edits.and_then(|edits| edits.get(INFRASTRUCTURE_RUNS_KEY)) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{INFRASTRUCTURE_RUNS_KEY} must be an array"))?;
    let runs = values
        .iter()
        .map(InfrastructureRun::from_json)
        .collect::<anyhow::Result<Vec<_>>>()?;
    validate_infrastructure_runs(&runs)?;
    Ok(runs)
}

pub fn validate_infrastructure_runs(runs: &[InfrastructureRun]) -> anyhow::Result<()> {
    anyhow::ensure!(
        runs.len() <= MAX_INFRASTRUCTURE_RUNS,
        "{INFRASTRUCTURE_RUNS_KEY} exceeds the {MAX_INFRASTRUCTURE_RUNS} run cap"
    );
    let mut ids = BTreeSet::new();
    for run in runs {
        run.validate()?;
        anyhow::ensure!(
            ids.insert(run.id.clone()),
            "duplicate infrastructure run id {}",
            run.id
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldRange {
    pub min: f32,
    pub max: f32,
    /// Fraction of the interval used for smooth entry/exit.  Zero makes a hard gate.
    pub feather: f32,
}

impl FieldRange {
    pub const fn new(min: f32, max: f32, feather: f32) -> Self {
        Self { min, max, feather }
    }

    pub fn score(self, value: f32) -> f32 {
        if !value.is_finite() || value < self.min || value > self.max || self.min > self.max {
            return 0.0;
        }
        let span = (self.max - self.min).max(1.0e-5);
        let edge = (span * self.feather.clamp(0.0, 0.49)).max(1.0e-5);
        let lo = ((value - self.min) / edge).clamp(0.0, 1.0);
        let hi = ((self.max - value) / edge).clamp(0.0, 1.0);
        smoothstep01(lo) * smoothstep01(hi)
    }

    fn validate(self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.min.is_finite() && self.max.is_finite() && self.feather.is_finite(),
            "brush field {name} contains a non-finite value"
        );
        anyhow::ensure!(self.min <= self.max, "brush field {name} range is reversed");
        anyhow::ensure!(
            (0.0..=0.49).contains(&self.feather),
            "brush field {name} feather must be 0..0.49"
        );
        Ok(())
    }

    fn to_json(self) -> serde_json::Value {
        serde_json::json!([self.min, self.max, self.feather])
    }

    fn from_json(value: &serde_json::Value, fallback: Self) -> Self {
        let Some(values) = value.as_array().filter(|values| values.len() >= 2) else {
            return fallback;
        };
        Self {
            min: json_f32_or(&values[0], fallback.min),
            max: json_f32_or(&values[1], fallback.max),
            feather: values
                .get(2)
                .map(|value| json_f32_or(value, fallback.feather))
                .unwrap_or(fallback.feather),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentSample {
    pub height_m: f32,
    pub slope_deg: f32,
    pub solar_exposure: f32,
    pub soil_moisture: f32,
    pub fertility: f32,
    pub wind_exposure: f32,
    pub destruction: f32,
    pub concavity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentalConstraints {
    pub height_m: FieldRange,
    pub slope_deg: FieldRange,
    pub solar_exposure: FieldRange,
    pub soil_moisture: FieldRange,
    pub fertility: FieldRange,
    pub wind_exposure: FieldRange,
    pub destruction: FieldRange,
}

impl Default for EnvironmentalConstraints {
    fn default() -> Self {
        Self {
            height_m: FieldRange::new(-1_000.0, 10_000.0, 0.0),
            slope_deg: FieldRange::new(0.0, 89.9, 0.05),
            solar_exposure: FieldRange::new(0.0, 1.0, 0.05),
            soil_moisture: FieldRange::new(0.0, 1.0, 0.05),
            fertility: FieldRange::new(0.0, 1.0, 0.05),
            wind_exposure: FieldRange::new(0.0, 1.0, 0.05),
            destruction: FieldRange::new(0.0, 1.0, 0.05),
        }
    }
}

impl EnvironmentalConstraints {
    pub fn score(self, sample: EnvironmentSample) -> f32 {
        // Geometric mean keeps every field meaningful without collapsing useful soft masks into
        // microscopic values after seven multiplications.
        let scores = [
            self.height_m.score(sample.height_m),
            self.slope_deg.score(sample.slope_deg),
            self.solar_exposure.score(sample.solar_exposure),
            self.soil_moisture.score(sample.soil_moisture),
            self.fertility.score(sample.fertility),
            self.wind_exposure.score(sample.wind_exposure),
            self.destruction.score(sample.destruction),
        ];
        if scores.contains(&0.0) {
            0.0
        } else {
            scores
                .iter()
                .product::<f32>()
                .powf(1.0 / scores.len() as f32)
        }
    }

    fn validate(self) -> anyhow::Result<()> {
        for (name, range) in [
            ("height_m", self.height_m),
            ("slope_deg", self.slope_deg),
            ("solar_exposure", self.solar_exposure),
            ("soil_moisture", self.soil_moisture),
            ("fertility", self.fertility),
            ("wind_exposure", self.wind_exposure),
            ("destruction", self.destruction),
        ] {
            range.validate(name)?;
        }
        Ok(())
    }

    fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "height_m": self.height_m.to_json(),
            "slope_deg": self.slope_deg.to_json(),
            "solar_exposure": self.solar_exposure.to_json(),
            "soil_moisture": self.soil_moisture.to_json(),
            "fertility": self.fertility.to_json(),
            "wind_exposure": self.wind_exposure.to_json(),
            "destruction": self.destruction.to_json(),
        })
    }

    fn from_json(value: &serde_json::Value) -> Self {
        let fallback = Self::default();
        Self {
            height_m: FieldRange::from_json(&value["height_m"], fallback.height_m),
            slope_deg: FieldRange::from_json(&value["slope_deg"], fallback.slope_deg),
            solar_exposure: FieldRange::from_json(
                &value["solar_exposure"],
                fallback.solar_exposure,
            ),
            soil_moisture: FieldRange::from_json(&value["soil_moisture"], fallback.soil_moisture),
            fertility: FieldRange::from_json(&value["fertility"], fallback.fertility),
            wind_exposure: FieldRange::from_json(&value["wind_exposure"], fallback.wind_exposure),
            destruction: FieldRange::from_json(&value["destruction"], fallback.destruction),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrushAsset {
    pub path: String,
    pub weight: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub sink_m: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrushRecipe {
    pub id: String,
    pub name: String,
    pub role: String,
    pub assets: Vec<BrushAsset>,
    pub seed: u32,
    pub default_radius_m: f32,
    pub density_per_100m2: f32,
    pub min_spacing_m: f32,
    pub cluster_scale_m: f32,
    pub constraints: EnvironmentalConstraints,
}

impl BrushRecipe {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.trim().is_empty(), "brush id is empty");
        anyhow::ensure!(!self.name.trim().is_empty(), "brush name is empty");
        anyhow::ensure!(!self.assets.is_empty(), "brush {} has no assets", self.id);
        anyhow::ensure!(
            self.default_radius_m.is_finite() && (1.0..=2_000.0).contains(&self.default_radius_m)
        );
        anyhow::ensure!(
            self.density_per_100m2.is_finite()
                && (0.001..=1_000.0).contains(&self.density_per_100m2)
        );
        anyhow::ensure!(
            self.min_spacing_m.is_finite() && (0.1..=500.0).contains(&self.min_spacing_m)
        );
        anyhow::ensure!(
            self.cluster_scale_m.is_finite() && (1.0..=10_000.0).contains(&self.cluster_scale_m)
        );
        self.constraints.validate()?;
        let mut paths = BTreeSet::new();
        for asset in &self.assets {
            anyhow::ensure!(
                !asset.path.trim().is_empty(),
                "brush {} has an empty asset",
                self.id
            );
            anyhow::ensure!(
                paths.insert(asset.path.clone()),
                "brush {} repeats {}",
                self.id,
                asset.path
            );
            anyhow::ensure!(asset.weight.is_finite() && asset.weight > 0.0);
            anyhow::ensure!(
                asset.scale_min.is_finite()
                    && asset.scale_max.is_finite()
                    && asset.scale_min > 0.0
                    && asset.scale_min <= asset.scale_max
                    && asset.scale_max <= 100.0
            );
            anyhow::ensure!(asset.sink_m.is_finite() && asset.sink_m.abs() <= 20.0);
        }
        Ok(())
    }

    pub fn custom(paths: impl IntoIterator<Item = String>, seed: u32) -> anyhow::Result<Self> {
        let unique = paths.into_iter().collect::<BTreeSet<_>>();
        let assets = unique
            .into_iter()
            .map(|path| BrushAsset {
                path,
                weight: 1.0,
                scale_min: 0.72,
                scale_max: 1.45,
                sink_m: -0.10,
            })
            .collect::<Vec<_>>();
        let fingerprint = assets.iter().fold(0x811c_9dc5u32, |mut hash, asset| {
            for byte in asset.path.bytes().chain(std::iter::once(0xff)) {
                hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
            }
            hash
        });
        let recipe = Self {
            id: format!("custom_mix_{fingerprint:08x}"),
            name: "Custom mix".to_owned(),
            role: "custom".to_owned(),
            assets,
            seed,
            default_radius_m: 30.0,
            density_per_100m2: 1.8,
            min_spacing_m: 3.0,
            cluster_scale_m: 85.0,
            constraints: EnvironmentalConstraints::default(),
        };
        recipe.validate()?;
        Ok(recipe)
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "name": self.name,
            "role": self.role,
            "seed": self.seed,
            "default_radius_m": self.default_radius_m,
            "density_per_100m2": self.density_per_100m2,
            "min_spacing_m": self.min_spacing_m,
            "cluster_scale_m": self.cluster_scale_m,
            "constraints": self.constraints.to_json(),
            "assets": self.assets.iter().map(|asset| serde_json::json!({
                "path": asset.path,
                "weight": asset.weight,
                "scale": [asset.scale_min, asset.scale_max],
                "sink_m": asset.sink_m,
            })).collect::<Vec<_>>(),
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let id = value["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("procedural brush is missing id"))?
            .to_owned();
        let assets = value["assets"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("brush {id} assets must be an array"))?
            .iter()
            .map(|asset| {
                let scale = asset["scale"].as_array();
                Ok(BrushAsset {
                    path: asset["path"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("brush {id} asset is missing path"))?
                        .to_owned(),
                    weight: json_f32_or(&asset["weight"], 1.0),
                    scale_min: scale
                        .and_then(|scale| scale.first())
                        .map(|value| json_f32_or(value, 1.0))
                        .unwrap_or(1.0),
                    scale_max: scale
                        .and_then(|scale| scale.get(1))
                        .map(|value| json_f32_or(value, 1.0))
                        .unwrap_or(1.0),
                    sink_m: json_f32_or(&asset["sink_m"], -0.1),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let recipe = Self {
            id,
            name: value["name"].as_str().unwrap_or("Unnamed brush").to_owned(),
            role: value["role"].as_str().unwrap_or("custom").to_owned(),
            assets,
            seed: value["seed"].as_u64().unwrap_or(0) as u32,
            default_radius_m: json_f32_or(&value["default_radius_m"], 30.0),
            density_per_100m2: json_f32_or(&value["density_per_100m2"], 1.8),
            min_spacing_m: json_f32_or(&value["min_spacing_m"], 3.0),
            cluster_scale_m: json_f32_or(&value["cluster_scale_m"], 85.0),
            constraints: EnvironmentalConstraints::from_json(&value["constraints"]),
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

/// Resolve one editor brush card against the installed library.  Rules use semantic name/path
/// tokens rather than fixed vendor filenames, so adding another Pizza pack automatically enriches
/// the mix while the stable sort and per-role caps keep the recipe deterministic and readable.
pub fn recipe_from_browser_preset(
    preset_path: &str,
    library: &[crate::assetlib::AssetEntry],
    seed: u32,
) -> anyhow::Result<BrushRecipe> {
    let preset = preset_path
        .strip_prefix("editor:brush/")
        .ok_or_else(|| anyhow::anyhow!("unknown brush preset {preset_path}"))?;
    let mut assets = Vec::<BrushAsset>::new();
    let mut used = BTreeSet::<String>::new();

    let mut add_matches =
        |needles: &[&str], limit: usize, weight: f32, scale: [f32; 2], sink_m: f32| {
            let mut candidates = library
                .iter()
                .filter(|entry| entry.kind.is_model())
                .filter(|entry| {
                    let key = entry.search_key.as_str();
                    needles.iter().any(|needle| key.contains(needle))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| {
                left.path
                    .len()
                    .cmp(&right.path.len())
                    .then_with(|| left.path.cmp(&right.path))
            });
            let mut added = 0usize;
            for entry in candidates {
                if assets.len() >= 32 || !used.insert(entry.path.clone()) {
                    continue;
                }
                assets.push(BrushAsset {
                    path: entry.path.clone(),
                    weight,
                    scale_min: scale[0],
                    scale_max: scale[1],
                    sink_m,
                });
                added += 1;
                if added >= limit {
                    break;
                }
            }
        };

    let (name, role, radius, density, spacing, cluster, constraints) = match preset {
        "forest_mixed" => {
            add_matches(
                &["pine", "spruce", "birch", "tree_n_"],
                6,
                9.0,
                [0.72, 1.38],
                -0.16,
            );
            add_matches(
                &["tree_stump", "tree_log", "branch", "stick"],
                4,
                2.0,
                [0.65, 1.30],
                -0.10,
            );
            add_matches(&["rock", "stone"], 4, 2.4, [0.45, 1.35], -0.12);
            add_matches(&["grass", "fern", "mushroom"], 4, 3.5, [0.55, 1.20], -0.05);
            let mut constraints = EnvironmentalConstraints::default();
            constraints.slope_deg = FieldRange::new(0.0, 34.0, 0.18);
            constraints.solar_exposure = FieldRange::new(0.10, 0.92, 0.12);
            constraints.soil_moisture = FieldRange::new(0.28, 1.0, 0.16);
            constraints.fertility = FieldRange::new(0.30, 1.0, 0.16);
            constraints.destruction = FieldRange::new(0.0, 0.72, 0.16);
            (
                "Mixed forest",
                "ecology",
                42.0,
                2.8,
                3.8,
                118.0,
                constraints,
            )
        }
        "forest_floor" => {
            add_matches(
                &["tree_stump", "tree_log", "branch", "stick", "root"],
                7,
                3.8,
                [0.55, 1.35],
                -0.10,
            );
            add_matches(&["rock", "stone"], 6, 3.0, [0.32, 1.05], -0.11);
            add_matches(
                &["grass", "fern", "mushroom", "flower"],
                7,
                5.5,
                [0.45, 1.15],
                -0.04,
            );
            let mut constraints = EnvironmentalConstraints::default();
            constraints.slope_deg = FieldRange::new(0.0, 42.0, 0.12);
            constraints.soil_moisture = FieldRange::new(0.24, 1.0, 0.12);
            constraints.fertility = FieldRange::new(0.22, 1.0, 0.12);
            ("Forest floor", "ecology", 28.0, 5.5, 2.2, 74.0, constraints)
        }
        "battlefield_debris" => {
            add_matches(&["debris", "rubble", "brick"], 10, 7.0, [0.45, 1.35], -0.12);
            add_matches(&["board", "plank", "timber"], 5, 3.2, [0.65, 1.45], -0.08);
            add_matches(
                &["tire", "barrel", "crate", "bottle", "can_"],
                7,
                2.2,
                [0.62, 1.18],
                -0.04,
            );
            add_matches(
                &["shell", "ammo", "weapon", "rifle", "helmet"],
                5,
                1.2,
                [0.78, 1.08],
                -0.02,
            );
            let mut constraints = EnvironmentalConstraints::default();
            constraints.slope_deg = FieldRange::new(0.0, 38.0, 0.12);
            constraints.destruction = FieldRange::new(0.20, 1.0, 0.18);
            (
                "Battlefield debris",
                "destruction",
                34.0,
                3.6,
                2.6,
                62.0,
                constraints,
            )
        }
        "aftermath" => {
            add_matches(
                &["fallen soldier", "deadman", "bodies / remains"],
                1,
                1.1,
                [0.88, 1.08],
                -0.03,
            );
            add_matches(&["burned", "vehicle wrecks"], 6, 0.16, [0.88, 1.12], -0.20);
            add_matches(&["debris", "rubble", "brick"], 8, 7.5, [0.50, 1.35], -0.12);
            add_matches(
                &["board", "plank", "tire", "crate", "barrel"],
                7,
                2.8,
                [0.65, 1.25],
                -0.06,
            );
            add_matches(
                &["shell", "ammo", "weapon", "rifle", "helmet"],
                5,
                1.1,
                [0.78, 1.08],
                -0.02,
            );
            let mut constraints = EnvironmentalConstraints::default();
            constraints.slope_deg = FieldRange::new(0.0, 30.0, 0.12);
            constraints.destruction = FieldRange::new(0.34, 1.0, 0.20);
            (
                "Aftermath: bodies + wreckage",
                "destruction",
                46.0,
                2.5,
                3.2,
                78.0,
                constraints,
            )
        }
        _ => anyhow::bail!("unknown brush preset {preset_path}"),
    };
    let recipe = BrushRecipe {
        id: preset.to_owned(),
        name: name.to_owned(),
        role: role.to_owned(),
        assets,
        seed,
        default_radius_m: radius,
        density_per_100m2: density,
        min_spacing_m: spacing,
        cluster_scale_m: cluster,
        constraints,
    };
    recipe.validate().map_err(|error| {
        anyhow::anyhow!("{name} could not resolve against the installed asset library: {error}")
    })?;
    Ok(recipe)
}

/// One deterministic child emitted by a reviewed scene recipe. `offset` is anchor-local and is
/// re-draped onto the destination terrain by the editor. `fit_length_m` asks the editor to stretch
/// the model's longest horizontal axis to a real connection length (used by cable spans).
#[derive(Clone, Debug, PartialEq)]
pub struct ScenePlacementSpec {
    pub child_id: String,
    pub asset_path: String,
    pub offset: Vec3,
    pub yaw_deg: f32,
    pub scale: Vec3,
    pub fit_length_m: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLightSpec {
    pub child_id: String,
    pub preset: String,
    pub offset: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneSurfaceSpec {
    pub child_id: String,
    pub preset: String,
    pub offset: Vec3,
    pub radius_m: f32,
    pub rotation_deg: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneRecipePlan {
    pub recipe_id: String,
    pub name: String,
    pub placements: Vec<ScenePlacementSpec>,
    pub lights: Vec<SceneLightSpec>,
    pub surfaces: Vec<SceneSurfaceSpec>,
    /// Positions are anchor-local until the editor materializes the plan.
    pub infrastructure: Option<InfrastructureRun>,
}

impl SceneRecipePlan {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.recipe_id.trim().is_empty() && !self.name.trim().is_empty());
        anyhow::ensure!(self.placements.len() <= 4_096);
        anyhow::ensure!(self.lights.len() <= 256);
        anyhow::ensure!(self.surfaces.len() <= 768);
        let mut ids = BTreeSet::new();
        for placement in &self.placements {
            anyhow::ensure!(
                ids.insert(placement.child_id.clone()),
                "scene plan repeats child {}",
                placement.child_id
            );
            validate_repo_relative_path(&placement.asset_path, "scene placement asset")?;
            anyhow::ensure!(placement.offset.is_finite() && placement.yaw_deg.is_finite());
            anyhow::ensure!(
                placement.scale.is_finite()
                    && placement.scale.min_element() > 0.0
                    && placement.scale.max_element() <= 100.0
            );
            if let Some(length) = placement.fit_length_m {
                anyhow::ensure!(length.is_finite() && (0.5..=100.0).contains(&length));
            }
        }
        for light in &self.lights {
            anyhow::ensure!(
                ids.insert(light.child_id.clone()),
                "scene plan repeats child {}",
                light.child_id
            );
            anyhow::ensure!(light.offset.is_finite());
        }
        for surface in &self.surfaces {
            anyhow::ensure!(
                ids.insert(surface.child_id.clone()),
                "scene plan repeats child {}",
                surface.child_id
            );
            anyhow::ensure!(surface.offset.is_finite() && surface.rotation_deg.is_finite());
            anyhow::ensure!(
                surface.radius_m.is_finite() && (0.1..=2_000.0).contains(&surface.radius_m)
            );
        }
        if let Some(run) = &self.infrastructure {
            run.validate()?;
        }
        Ok(())
    }

    pub fn resolved_assets(&self) -> Vec<String> {
        let mut assets = self
            .placements
            .iter()
            .map(|placement| placement.asset_path.clone())
            .collect::<BTreeSet<_>>();
        if let Some(run) = &self.infrastructure {
            assets.extend(run.spans.iter().map(|span| span.asset.clone()));
        }
        assets.into_iter().collect()
    }
}

/// Initial reviewed registry for period-facing recipe roles. The whole catalog remains browsable,
/// but automatic historical scenes never pick cell phones, modern screens, or arbitrary furniture
/// merely because a filename contains a broad word such as "table".
fn reviewed_asset(
    library: &[crate::assetlib::AssetEntry],
    accepted_stems: &[&str],
) -> Option<String> {
    let accepted = accepted_stems
        .iter()
        .map(|stem| stem.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut candidates = library
        .iter()
        .filter(|entry| entry.kind.is_model())
        .filter(|entry| {
            std::path::Path::new(&entry.path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| accepted.contains(&stem.to_ascii_lowercase()))
        })
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
    candidates.into_iter().next()
}

fn required_reviewed_asset(
    library: &[crate::assetlib::AssetEntry],
    role: &str,
    accepted_stems: &[&str],
) -> anyhow::Result<String> {
    reviewed_asset(library, accepted_stems).ok_or_else(|| {
        anyhow::anyhow!(
            "scene recipe requires reviewed {role}; none of {:?} is installed",
            accepted_stems
        )
    })
}

fn push_scene_prop(
    plan: &mut SceneRecipePlan,
    child_id: impl Into<String>,
    asset_path: &str,
    offset: Vec3,
    yaw_deg: f32,
    scale: Vec3,
) {
    plan.placements.push(ScenePlacementSpec {
        child_id: child_id.into(),
        asset_path: asset_path.to_owned(),
        offset,
        yaw_deg,
        scale,
        fit_length_m: None,
    });
}

fn utility_scene_plan(
    library: &[crate::assetlib::AssetEntry],
    seed: u32,
) -> anyhow::Result<SceneRecipePlan> {
    let pole = required_reviewed_asset(library, "telegraph pole", &["pillar_hr_2"])?;
    let wire = required_reviewed_asset(library, "overhead wire", &["wires_hr_1", "wires_hr_2"])?;
    let lamp = required_reviewed_asset(
        library,
        "period-plausible lamp housing",
        &["lamp_mx_1_a_on", "lamp_mx_1_b_on", "lamp_1_on"],
    )?;
    let holder = reviewed_asset(
        library,
        &["wires_holder_hr_small_1", "wires_holder_hr_large_1"],
    );
    let mut plan = SceneRecipePlan {
        recipe_id: "utility.lit_compound_road".to_owned(),
        name: "Lit utility road".to_owned(),
        placements: Vec::new(),
        lights: Vec::new(),
        surfaces: Vec::new(),
        infrastructure: None,
    };
    let pitch = 24.0;
    let mut nodes = Vec::new();
    for slot in 0..5 {
        let x = (slot as f32 - 2.0) * pitch;
        let jitter = (hash01(seed ^ (slot as u32).wrapping_mul(0x9E37_79B9)) - 0.5) * 1.8;
        let offset = Vec3::new(x + jitter, 0.0, 0.0);
        push_scene_prop(
            &mut plan,
            format!("utility.pole.{slot:02}"),
            &pole,
            offset,
            0.0,
            Vec3::new(0.22, 2.55, 0.22),
        );
        if let Some(holder) = &holder {
            push_scene_prop(
                &mut plan,
                format!("utility.holder.{slot:02}"),
                holder,
                offset + Vec3::Y * UTILITY_ATTACHMENT_HEIGHT_M,
                0.0,
                Vec3::splat(0.75),
            );
        }
        push_scene_prop(
            &mut plan,
            format!("utility.lamp.{slot:02}"),
            &lamp,
            offset + Vec3::new(0.0, 5.2, -0.45),
            0.0,
            Vec3::splat(0.72),
        );
        plan.lights.push(SceneLightSpec {
            child_id: format!("utility.light.{slot:02}"),
            preset: "work".to_owned(),
            offset: offset + Vec3::new(0.0, 5.15, -0.75),
        });
        nodes.push(InfrastructureNode {
            id: format!("node_{slot:02}"),
            position: offset,
            height_m: UTILITY_ATTACHMENT_HEIGHT_M,
        });
    }
    let mut spans = Vec::new();
    for slot in 0..4 {
        let a = nodes[slot].position;
        let b = nodes[slot + 1].position;
        let length = (b.x - a.x).hypot(b.z - a.z);
        plan.placements.push(ScenePlacementSpec {
            child_id: format!("utility.wire.{slot:02}"),
            asset_path: wire.clone(),
            offset: (a + b) * 0.5 + Vec3::Y * UTILITY_ATTACHMENT_HEIGHT_M,
            yaw_deg: (b.z - a.z).atan2(b.x - a.x).to_degrees(),
            scale: Vec3::ONE,
            fit_length_m: Some(length),
        });
        spans.push(CableSpan {
            id: format!("span_{slot:02}"),
            from_node: nodes[slot].id.clone(),
            to_node: nodes[slot + 1].id.clone(),
            asset: wire.clone(),
            sag_m: 0.8,
            broken: false,
        });
    }
    plan.infrastructure = Some(InfrastructureRun {
        id: "local_utility_run".to_owned(),
        kind: "lit_utility_road".to_owned(),
        nodes,
        spans,
    });
    plan.validate()?;
    Ok(plan)
}

fn forward_base_scene_plan(
    library: &[crate::assetlib::AssetEntry],
) -> anyhow::Result<SceneRecipePlan> {
    let wall = required_reviewed_asset(library, "modular wall", &["wall_hr_1"])?;
    let doorway = required_reviewed_asset(library, "doorway", &["doorway_hr_1"])?;
    let fence = required_reviewed_asset(library, "perimeter fence", &["fence_b_1"])?;
    let garage = required_reviewed_asset(
        library,
        "motor-pool garage",
        &["garage_hl_1", "garage_hm_1"],
    )?;
    let bed = required_reviewed_asset(library, "barracks bed", &["bed_1", "bed_2"])?;
    let table =
        required_reviewed_asset(library, "work table", &["table_large_2", "table_large_3"])?;
    let chair = required_reviewed_asset(library, "chair", &["chair_mp_1", "chair_mp_4"])?;
    let cabinet = required_reviewed_asset(
        library,
        "storage cabinet",
        &["wardrobe_mp_1", "display_cabinet_mp_1"],
    )?;
    let crate_asset = required_reviewed_asset(
        library,
        "supply crate",
        &["supply_crate_1", "wooden_crate_1"],
    )?;
    let lamp = required_reviewed_asset(library, "lamp", &["ceiling_lamp_1_on", "lamp_mx_1_a_on"])?;
    let radio = reviewed_asset(library, &["handheld_fm_radio_etx_1"]);
    let first_aid = reviewed_asset(library, &["first_aid_kit_hr_1"]);
    let jerrycan = reviewed_asset(library, &["jerrycan_mx_1"]);
    let mut plan = SceneRecipePlan {
        recipe_id: "compound.forward_base".to_owned(),
        name: "Furnished forward base".to_owned(),
        placements: Vec::new(),
        lights: Vec::new(),
        surfaces: Vec::new(),
        infrastructure: None,
    };

    // A 48 x 36 m perimeter with an eight-metre vehicle gate in the south wall.
    for side in [-1.0, 1.0] {
        for slot in -6..=6 {
            let x = slot as f32 * 4.0;
            if side < 0.0 && x.abs() < 5.0 {
                continue;
            }
            push_scene_prop(
                &mut plan,
                format!("base.fence.z{side:+.0}.{slot:+03}"),
                &fence,
                Vec3::new(x, 0.0, side * 18.0),
                0.0,
                Vec3::ONE,
            );
        }
        for slot in -4..=4 {
            push_scene_prop(
                &mut plan,
                format!("base.fence.x{side:+.0}.{slot:+03}"),
                &fence,
                Vec3::new(side * 24.0, 0.0, slot as f32 * 4.0),
                90.0,
                Vec3::ONE,
            );
        }
    }
    for (slot, x) in [-12.0, 12.0].into_iter().enumerate() {
        push_scene_prop(
            &mut plan,
            format!("base.motor_pool.{slot:02}"),
            &garage,
            Vec3::new(x, 0.0, 10.0),
            180.0,
            Vec3::ONE,
        );
    }

    // Ground-level modular barracks. Walls use the runtime's reviewed wall/door collision path;
    // furniture keeps a clear central aisle and doorway landing.
    for (slot, x) in [-18.0, -14.0, -10.0, -6.0].into_iter().enumerate() {
        push_scene_prop(
            &mut plan,
            format!("barracks.wall.n.{slot}"),
            &wall,
            Vec3::new(x, 0.0, 2.0),
            0.0,
            Vec3::ONE,
        );
        if slot != 1 {
            push_scene_prop(
                &mut plan,
                format!("barracks.wall.s.{slot}"),
                &wall,
                Vec3::new(x, 0.0, -10.0),
                0.0,
                Vec3::ONE,
            );
        }
    }
    push_scene_prop(
        &mut plan,
        "barracks.door",
        &doorway,
        Vec3::new(-14.0, 0.0, -10.0),
        0.0,
        Vec3::ONE,
    );
    for (side, x) in [("w", -20.0), ("e", -4.0)] {
        for (slot, z) in [-8.0, -4.0, 0.0].into_iter().enumerate() {
            push_scene_prop(
                &mut plan,
                format!("barracks.wall.{side}.{slot}"),
                &wall,
                Vec3::new(x, 0.0, z),
                90.0,
                Vec3::ONE,
            );
        }
    }
    for row in 0..2 {
        for slot in 0..3 {
            let x = -18.0 + slot as f32 * 5.0;
            let z = if row == 0 { -7.2 } else { -0.8 };
            push_scene_prop(
                &mut plan,
                format!("barracks.bed.{row}.{slot}"),
                &bed,
                Vec3::new(x, 0.0, z),
                if row == 0 { 0.0 } else { 180.0 },
                Vec3::ONE,
            );
        }
    }
    push_scene_prop(
        &mut plan,
        "barracks.table",
        &table,
        Vec3::new(-12.0, 0.0, -4.0),
        90.0,
        Vec3::splat(0.9),
    );
    for (slot, (x, z)) in [(-14.2, -4.0), (-9.8, -4.0), (-12.0, -6.0), (-12.0, -2.0)]
        .into_iter()
        .enumerate()
    {
        push_scene_prop(
            &mut plan,
            format!("barracks.chair.{slot}"),
            &chair,
            Vec3::new(x, 0.0, z),
            slot as f32 * 90.0,
            Vec3::ONE,
        );
    }
    push_scene_prop(
        &mut plan,
        "barracks.cabinet",
        &cabinet,
        Vec3::new(-5.0, 0.0, 1.0),
        180.0,
        Vec3::ONE,
    );
    push_scene_prop(
        &mut plan,
        "barracks.lamp",
        &lamp,
        Vec3::new(-12.0, 2.7, -4.0),
        0.0,
        Vec3::ONE,
    );
    plan.lights.push(SceneLightSpec {
        child_id: "barracks.light".to_owned(),
        preset: "work".to_owned(),
        offset: Vec3::new(-12.0, 2.45, -4.0),
    });

    // Signals/HQ room and stocked motor pool.
    for (slot, x) in [6.0, 10.0, 14.0].into_iter().enumerate() {
        push_scene_prop(
            &mut plan,
            format!("hq.wall.n.{slot}"),
            &wall,
            Vec3::new(x, 0.0, 2.0),
            0.0,
            Vec3::ONE,
        );
        if slot != 1 {
            push_scene_prop(
                &mut plan,
                format!("hq.wall.s.{slot}"),
                &wall,
                Vec3::new(x, 0.0, -6.0),
                0.0,
                Vec3::ONE,
            );
        }
    }
    push_scene_prop(
        &mut plan,
        "hq.door",
        &doorway,
        Vec3::new(10.0, 0.0, -6.0),
        0.0,
        Vec3::ONE,
    );
    for (side, x) in [("w", 4.0), ("e", 16.0)] {
        for (slot, z) in [-4.0, 0.0].into_iter().enumerate() {
            push_scene_prop(
                &mut plan,
                format!("hq.wall.{side}.{slot}"),
                &wall,
                Vec3::new(x, 0.0, z),
                90.0,
                Vec3::ONE,
            );
        }
    }
    push_scene_prop(
        &mut plan,
        "hq.table",
        &table,
        Vec3::new(10.0, 0.0, -1.5),
        0.0,
        Vec3::ONE,
    );
    for (slot, (x, z)) in [(7.6, -1.5), (12.4, -1.5), (10.0, 0.4)]
        .into_iter()
        .enumerate()
    {
        push_scene_prop(
            &mut plan,
            format!("hq.chair.{slot}"),
            &chair,
            Vec3::new(x, 0.0, z),
            slot as f32 * 120.0,
            Vec3::ONE,
        );
    }
    if let Some(radio) = &radio {
        push_scene_prop(
            &mut plan,
            "hq.radio",
            radio,
            Vec3::new(10.0, 1.05, -1.5),
            0.0,
            Vec3::splat(0.9),
        );
    }
    if let Some(first_aid) = &first_aid {
        push_scene_prop(
            &mut plan,
            "hq.first_aid",
            first_aid,
            Vec3::new(15.3, 0.8, 1.5),
            90.0,
            Vec3::ONE,
        );
    }
    push_scene_prop(
        &mut plan,
        "hq.lamp",
        &lamp,
        Vec3::new(10.0, 2.7, -1.5),
        0.0,
        Vec3::ONE,
    );
    plan.lights.push(SceneLightSpec {
        child_id: "hq.light".to_owned(),
        preset: "work".to_owned(),
        offset: Vec3::new(10.0, 2.45, -1.5),
    });
    for slot in 0..10 {
        let x = -19.0 + (slot % 5) as f32 * 2.3;
        let z = 5.0 + (slot / 5) as f32 * 2.3;
        push_scene_prop(
            &mut plan,
            format!("base.supply.{slot:02}"),
            &crate_asset,
            Vec3::new(x, 0.0, z),
            (slot * 17) as f32,
            Vec3::splat(0.92),
        );
    }
    if let Some(jerrycan) = &jerrycan {
        for slot in 0..5 {
            push_scene_prop(
                &mut plan,
                format!("base.jerrycan.{slot:02}"),
                jerrycan,
                Vec3::new(17.5 + (slot % 2) as f32, 0.0, 6.0 + slot as f32 * 1.25),
                (slot * 31) as f32,
                Vec3::ONE,
            );
        }
    }
    plan.validate()?;
    Ok(plan)
}

fn aftermath_scene_plan(
    library: &[crate::assetlib::AssetEntry],
    seed: u32,
    artillery: bool,
) -> anyhow::Result<SceneRecipePlan> {
    let body = required_reviewed_asset(library, "frozen casualty silhouette", &["deadman_prone"])?;
    let wreck_a = required_reviewed_asset(
        library,
        "burned vehicle silhouette",
        &["t34_85", "panther", "is2"],
    )?;
    let wreck_b = required_reviewed_asset(
        library,
        "second burned vehicle silhouette",
        &["pz3", "tiger2", "kv1"],
    )?;
    let debris = required_reviewed_asset(
        library,
        "battle debris",
        &["debris_bricks_mx_1", "gravel_pile_hr_1"],
    )?;
    let board = reviewed_asset(library, &["wooden_board_2", "wooden_plank_1"]);
    let crate_asset = reviewed_asset(library, &["supply_crate_1", "wooden_crate_1"]);
    let tire = reviewed_asset(library, &["tire_1"]);
    let (recipe_id, name, wreck_count, body_count) = if artillery {
        (
            "aftermath.artillery_position",
            "Overrun artillery position",
            4,
            32,
        )
    } else {
        (
            "aftermath.destroyed_convoy",
            "Destroyed forest convoy",
            8,
            24,
        )
    };
    let mut plan = SceneRecipePlan {
        recipe_id: recipe_id.to_owned(),
        name: name.to_owned(),
        placements: Vec::new(),
        lights: Vec::new(),
        surfaces: Vec::new(),
        infrastructure: None,
    };
    for slot in 0..wreck_count {
        let key = seed ^ (slot as u32).wrapping_mul(0x9E37_79B9);
        let (x, z) = if artillery {
            let angle = slot as f32 * std::f32::consts::TAU / wreck_count as f32;
            (angle.cos() * 12.0, angle.sin() * 12.0)
        } else {
            (-31.5 + slot as f32 * 9.0, (hash01(key) - 0.5) * 4.2)
        };
        let asset = if slot % 2 == 0 { &wreck_a } else { &wreck_b };
        push_scene_prop(
            &mut plan,
            format!("aftermath.wreck.{slot:02}"),
            asset,
            Vec3::new(x, -0.18, z),
            if artillery {
                slot as f32 * 91.0
            } else {
                84.0 + (hash01(key ^ 0xA11C_E001) - 0.5) * 28.0
            },
            Vec3::splat(0.92 + hash01(key ^ 0x55C4_1E11) * 0.18),
        );
        plan.surfaces.push(SceneSurfaceSpec {
            child_id: format!("aftermath.scorch.{slot:02}"),
            preset: "scorch".to_owned(),
            offset: Vec3::new(x, 0.02, z),
            radius_m: 3.4 + hash01(key ^ 0x5C0C_0001) * 2.2,
            rotation_deg: hash01(key ^ 0x5C0C_0002) * 360.0,
        });
    }
    for slot in 0..body_count {
        let key = seed ^ (slot as u32).wrapping_mul(0x85EB_CA6B) ^ 0xB0D1_E500;
        let parent = slot % wreck_count;
        let parent_x = if artillery {
            let angle = parent as f32 * std::f32::consts::TAU / wreck_count as f32;
            angle.cos() * 12.0
        } else {
            -31.5 + parent as f32 * 9.0
        };
        let parent_z = if artillery {
            let angle = parent as f32 * std::f32::consts::TAU / wreck_count as f32;
            angle.sin() * 12.0
        } else {
            0.0
        };
        // Crews cluster beside hatches, ditches and attempted cover instead of forming a uniform
        // field. Alternating road sides preserves a narrow path through the authored tableau.
        let side = if (slot / wreck_count) % 2 == 0 {
            -1.0
        } else {
            1.0
        };
        let x = parent_x + (hash01(key) - 0.5) * 7.0;
        let z = parent_z + side * (3.0 + hash01(key ^ 0x11A7_0001) * 5.5);
        push_scene_prop(
            &mut plan,
            format!("aftermath.body.{slot:03}"),
            &body,
            Vec3::new(x, -0.03, z),
            hash01(key ^ 0x11A7_0002) * 360.0,
            Vec3::splat(0.90 + hash01(key ^ 0x11A7_0003) * 0.16),
        );
        plan.surfaces.push(SceneSurfaceSpec {
            child_id: format!("aftermath.stain.{slot:03}"),
            preset: "frozen_blood".to_owned(),
            offset: Vec3::new(x + (hash01(key ^ 4) - 0.5) * 0.5, 0.02, z),
            radius_m: 0.48 + hash01(key ^ 5) * 0.72,
            rotation_deg: hash01(key ^ 6) * 360.0,
        });
    }
    let optional_debris = [Some(debris.clone()), board, crate_asset, tire]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for slot in 0..36 {
        let key = seed ^ (slot as u32).wrapping_mul(0x27D4_EB2D) ^ 0xDEB2_1500;
        let angle = hash01(key) * std::f32::consts::TAU;
        let radius = 5.0 + hash01(key ^ 1) * if artillery { 24.0 } else { 38.0 };
        let asset = &optional_debris[slot % optional_debris.len()];
        push_scene_prop(
            &mut plan,
            format!("aftermath.debris.{slot:03}"),
            asset,
            Vec3::new(angle.cos() * radius, -0.08, angle.sin() * radius * 0.45),
            hash01(key ^ 2) * 360.0,
            Vec3::splat(0.65 + hash01(key ^ 3) * 0.75),
        );
    }
    plan.validate()?;
    Ok(plan)
}

pub fn scene_recipe_from_browser_preset(
    preset_path: &str,
    library: &[crate::assetlib::AssetEntry],
    seed: u32,
) -> anyhow::Result<SceneRecipePlan> {
    let preset = preset_path
        .strip_prefix("editor:scene/")
        .ok_or_else(|| anyhow::anyhow!("unknown scene recipe {preset_path}"))?;
    match preset {
        "lit_utility_road" => utility_scene_plan(library, seed),
        "forward_base" => forward_base_scene_plan(library),
        "destroyed_convoy" => aftermath_scene_plan(library, seed, false),
        "artillery_position" => aftermath_scene_plan(library, seed, true),
        _ => anyhow::bail!("unknown scene recipe {preset_path}"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProceduralLayerRecord {
    pub id: String,
    pub recipe: BrushRecipe,
    pub bounds: [f32; 4], // min x, min z, max x, max z
    pub generated_count: usize,
}

impl ProceduralLayerRecord {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "id": self.id,
            "recipe": self.recipe.to_json(),
            "bounds": self.bounds,
            "generated_count": self.generated_count,
            "materialization": "ordinary-placement-ops",
        })
    }

    fn from_json(value: &serde_json::Value) -> anyhow::Result<Self> {
        let bounds = value["bounds"]
            .as_array()
            .filter(|values| values.len() == 4)
            .ok_or_else(|| anyhow::anyhow!("procedural layer bounds must have four numbers"))?;
        let record = Self {
            id: value["id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("procedural layer is missing id"))?
                .to_owned(),
            recipe: BrushRecipe::from_json(&value["recipe"])?,
            bounds: [
                json_f32_or(&bounds[0], 0.0),
                json_f32_or(&bounds[1], 0.0),
                json_f32_or(&bounds[2], 0.0),
                json_f32_or(&bounds[3], 0.0),
            ],
            generated_count: value["generated_count"].as_u64().unwrap_or(0) as usize,
        };
        anyhow::ensure!(!record.id.trim().is_empty(), "procedural layer id is empty");
        anyhow::ensure!(record.bounds.iter().all(|value| value.is_finite()));
        anyhow::ensure!(
            record.bounds[0] <= record.bounds[2] && record.bounds[1] <= record.bounds[3]
        );
        Ok(record)
    }
}

pub fn layers_from_edits(
    edits: Option<&serde_json::Value>,
) -> anyhow::Result<Vec<ProceduralLayerRecord>> {
    let Some(value) = edits.and_then(|edits| edits.get(LAYERS_KEY)) else {
        return Ok(Vec::new());
    };
    let mut ids = BTreeSet::new();
    value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{LAYERS_KEY} must be an array"))?
        .iter()
        .map(|value| {
            let layer = ProceduralLayerRecord::from_json(value)?;
            anyhow::ensure!(
                ids.insert(layer.id.clone()),
                "duplicate procedural layer id {}",
                layer.id
            );
            Ok(layer)
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentContext {
    pub seed: u32,
    pub min_height_m: f32,
    pub max_height_m: f32,
    pub sun_direction: Vec3,
    pub wind_direction: Vec2,
    pub base_moisture: f32,
    pub base_fertility: f32,
    pub base_destruction: f32,
}

/// Terrain/environment sample shared by painted dabs and whole-map initialization.  Concavity is
/// a cheap soil-water proxy; coherent macro noise prevents every equal-height contour becoming a
/// visible band.  Story/battle hot spots can be layered by the caller through `hotspot_destruction`.
#[allow(clippy::too_many_arguments)]
pub fn sample_heightfield(
    heights: &[f32],
    res: usize,
    spacing: f32,
    x: f32,
    z: f32,
    context: EnvironmentContext,
    hotspot_destruction: f32,
) -> EnvironmentSample {
    let height = sample_grid(heights, res, spacing, x, z);
    let step = spacing.max(0.1);
    let left = sample_grid(heights, res, spacing, x - step, z);
    let right = sample_grid(heights, res, spacing, x + step, z);
    let down = sample_grid(heights, res, spacing, x, z - step);
    let up = sample_grid(heights, res, spacing, x, z + step);
    let dx = (right - left) / (2.0 * step);
    let dz = (up - down) / (2.0 * step);
    let normal = Vec3::new(-dx, 1.0, -dz).normalize_or_zero();
    let slope_deg = normal.y.clamp(-1.0, 1.0).acos().to_degrees();
    let concavity = ((left + right + down + up) * 0.25 - height) / step;
    let height_span = (context.max_height_m - context.min_height_m).max(1.0);
    let normalized_height = ((height - context.min_height_m) / height_span).clamp(0.0, 1.0);
    let macro_noise = value_noise(x, z, context.seed ^ 0x51A7_70A1, 180.0);
    let soil_noise = value_noise(x, z, context.seed ^ 0x5011_5EED, 92.0);
    let moisture = (context.base_moisture + (soil_noise - 0.5) * 0.34 + concavity * 0.20
        - normalized_height * 0.20
        - slope_deg / 90.0 * 0.16)
        .clamp(0.0, 1.0);
    let fertility = (context.base_fertility + (moisture - 0.5) * 0.42
        - slope_deg / 90.0 * 0.24
        - hotspot_destruction.clamp(0.0, 1.0) * 0.38)
        .clamp(0.0, 1.0);
    let sun = context.sun_direction.normalize_or_zero();
    let solar_exposure = (normal.dot(sun).max(0.0) * 0.88 + 0.12).clamp(0.0, 1.0);
    let wind3 =
        Vec3::new(context.wind_direction.x, 0.0, context.wind_direction.y).normalize_or_zero();
    let windward = (-normal.dot(wind3)).clamp(-1.0, 1.0) * 0.25;
    let wind_exposure =
        (0.42 + windward + slope_deg / 90.0 * 0.28 + (macro_noise - 0.5) * 0.18).clamp(0.0, 1.0);
    let destruction = (context.base_destruction * (0.55 + macro_noise * 0.55)
        + hotspot_destruction * 0.78)
        .clamp(0.0, 1.0);
    EnvironmentSample {
        height_m: height,
        slope_deg,
        solar_exposure,
        soil_moisture: moisture,
        fertility,
        wind_exposure,
        destruction,
        concavity,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedPlacement {
    pub asset_path: String,
    pub position: Vec3,
    pub yaw_deg: f32,
    pub scale: f32,
    pub suitability: f32,
}

/// Adapt a recipe for one bounded, whole-map materialization.  The instance budget alone is not a
/// CPU budget: scanning a multi-kilometre map at a two-metre forest-floor spacing would still score
/// millions of rejected candidates.  At most sixteen stable candidates are considered for each
/// possible output instance, while the density is reduced to keep the unclipped expected count at
/// or below the requested cap. Local brush dabs retain the recipe's full authored spacing/density.
pub fn budget_whole_map_recipe(
    recipe: &BrushRecipe,
    bounds: [f32; 4],
    max_instances: usize,
) -> anyhow::Result<BrushRecipe> {
    recipe.validate()?;
    anyhow::ensure!(bounds.iter().all(|value| value.is_finite()));
    anyhow::ensure!(bounds[0] <= bounds[2] && bounds[1] <= bounds[3]);
    anyhow::ensure!(max_instances > 0, "whole-map instance budget is zero");
    let area_m2 = (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]);
    anyhow::ensure!(
        area_m2.is_finite() && area_m2 > 0.0,
        "whole-map bounds are empty"
    );

    let mut budgeted = recipe.clone();
    let expected = budgeted.density_per_100m2 * area_m2 / 100.0;
    if expected > max_instances as f32 {
        budgeted.density_per_100m2 *= max_instances as f32 / expected;
    }
    const CANDIDATE_OVERSAMPLE: f32 = 16.0;
    let budget_spacing = (area_m2 / (max_instances as f32 * CANDIDATE_OVERSAMPLE)).sqrt();
    budgeted.min_spacing_m = budgeted.min_spacing_m.max(budget_spacing);
    // `scatter_rect` visits inclusive floor/ceil cell bounds, so the area quotient alone can
    // undercount the real grid at the perimeter. Grow spacing until the actual integer grid also
    // satisfies the CPU cap.
    let candidate_budget = max_instances as f64 * CANDIDATE_OVERSAMPLE as f64;
    for _ in 0..8 {
        let spacing = budgeted.min_spacing_m as f64;
        let cells_x = ((bounds[2] as f64 / spacing).ceil() - (bounds[0] as f64 / spacing).floor()
            + 1.0)
            .max(0.0);
        let cells_z = ((bounds[3] as f64 / spacing).ceil() - (bounds[1] as f64 / spacing).floor()
            + 1.0)
            .max(0.0);
        let candidates = cells_x * cells_z;
        if candidates <= candidate_budget {
            break;
        }
        budgeted.min_spacing_m *=
            ((candidates / candidate_budget).sqrt() as f32 * 1.001).max(1.001);
    }
    budgeted.validate()?;
    Ok(budgeted)
}

/// Stable cell-based scatter.  Candidate identity comes from integer world cells, so changing one
/// rule or hand-editing one placement cannot reshuffle the rest of the map like a sequential RNG.
#[allow(clippy::too_many_arguments)]
pub fn scatter_rect(
    recipe: &BrushRecipe,
    bounds: [f32; 4],
    max_instances: usize,
    existing: &[(f32, f32)],
    sample: impl FnMut(f32, f32) -> EnvironmentSample,
    ground: impl FnMut(f32, f32) -> f32,
) -> anyhow::Result<Vec<GeneratedPlacement>> {
    scatter_region(
        recipe,
        bounds,
        max_instances,
        existing,
        |_, _| true,
        sample,
        ground,
    )
}

#[allow(clippy::too_many_arguments)]
fn scatter_region(
    recipe: &BrushRecipe,
    bounds: [f32; 4],
    max_instances: usize,
    existing: &[(f32, f32)],
    mut contains: impl FnMut(f32, f32) -> bool,
    mut sample: impl FnMut(f32, f32) -> EnvironmentSample,
    mut ground: impl FnMut(f32, f32) -> f32,
) -> anyhow::Result<Vec<GeneratedPlacement>> {
    recipe.validate()?;
    anyhow::ensure!(bounds.iter().all(|value| value.is_finite()));
    anyhow::ensure!(bounds[0] <= bounds[2] && bounds[1] <= bounds[3]);
    if max_instances == 0 {
        return Ok(Vec::new());
    }
    let cell = recipe.min_spacing_m.max(0.1);
    let min_cx = (bounds[0] / cell).floor() as i32;
    let max_cx = (bounds[2] / cell).ceil() as i32;
    let min_cz = (bounds[1] / cell).floor() as i32;
    let max_cz = (bounds[3] / cell).ceil() as i32;
    let density_probability = (recipe.density_per_100m2 * cell * cell / 100.0).clamp(0.0, 1.0);
    let mut output = Vec::<(u32, GeneratedPlacement)>::new();
    let mut occupied = HashMap::<(i32, i32), Vec<(f32, f32)>>::new();
    for &(x, z) in existing {
        occupied
            .entry(((x / cell).floor() as i32, (z / cell).floor() as i32))
            .or_default()
            .push((x, z));
    }
    for cz in min_cz..=max_cz {
        for cx in min_cx..=max_cx {
            let key = cell_hash(cx, cz, recipe.seed);
            let density_roll = hash01(key ^ 0xD311_5171);
            // Reject against the field-independent density first. On a five-kilometre act this
            // avoids terrain/noise/hotspot work for roughly fifteen of every sixteen budget-grid
            // cells while preserving the exact final acceptance probability below.
            if density_roll >= density_probability {
                continue;
            }
            let jitter_x = hash01(key ^ 0xA119_5EED);
            let jitter_z = hash01(key ^ 0x7E22_A11D);
            let x = (cx as f32 + jitter_x) * cell;
            let z = (cz as f32 + jitter_z) * cell;
            if x < bounds[0] || x > bounds[2] || z < bounds[1] || z > bounds[3] {
                continue;
            }
            if !contains(x, z) {
                continue;
            }
            let environment = sample(x, z);
            let suitability = recipe.constraints.score(environment);
            if suitability <= 0.0 {
                continue;
            }
            // Coherent habitat clusters sit above the local constraint score.  Different recipes
            // reuse the same fields but get independent cluster boundaries through their seed.
            let cluster = value_noise(x, z, recipe.seed ^ 0xC1A5_7E12, recipe.cluster_scale_m);
            let cluster_gate = smoothstep01(((cluster - 0.24) / 0.62).clamp(0.0, 1.0));
            let probability = (density_probability * suitability * cluster_gate).clamp(0.0, 1.0);
            if density_roll >= probability {
                continue;
            }
            let separation = recipe.min_spacing_m * 0.88;
            let occupied_nearby = (-1..=1).any(|dz| {
                (-1..=1).any(|dx| {
                    occupied.get(&(cx + dx, cz + dz)).is_some_and(|points| {
                        points
                            .iter()
                            .any(|&(px, pz)| (x - px).hypot(z - pz) < separation)
                    })
                })
            });
            if occupied_nearby {
                continue;
            }
            let asset = weighted_asset(&recipe.assets, hash01(key ^ 0xA55E_7001));
            let scale =
                asset.scale_min + (asset.scale_max - asset.scale_min) * hash01(key ^ 0x5CA1_E001);
            let yaw_deg = hash01(key ^ 0x7A90_11E5) * 360.0;
            output.push((
                cell_hash(cx, cz, recipe.seed ^ 0xB0D6_E771),
                GeneratedPlacement {
                    asset_path: asset.path.clone(),
                    position: Vec3::new(x, ground(x, z) + asset.sink_m, z),
                    yaw_deg,
                    scale,
                    suitability,
                },
            ));
            occupied.entry((cx, cz)).or_default().push((x, z));
        }
    }
    // A row-major early exit visibly filled the south edge of dense brush dabs first. Stable hash
    // priority gives a spatially unbiased subset while keeping identical seeds exactly repeatable.
    if output.len() > max_instances {
        output.sort_by_key(|(priority, _)| *priority);
        output.truncate(max_instances);
    }
    Ok(output.into_iter().map(|(_, placement)| placement).collect())
}

pub fn scatter_circle(
    recipe: &BrushRecipe,
    center: Vec2,
    radius_m: f32,
    stroke: u32,
    max_instances: usize,
    existing: &[(f32, f32)],
    mut sample: impl FnMut(f32, f32) -> EnvironmentSample,
    mut ground: impl FnMut(f32, f32) -> f32,
) -> anyhow::Result<Vec<GeneratedPlacement>> {
    let mut local = recipe.clone();
    local.seed ^= stroke.wrapping_mul(0x9E37_79B9);
    let bounds = [
        center.x - radius_m,
        center.y - radius_m,
        center.x + radius_m,
        center.y + radius_m,
    ];
    scatter_region(
        &local,
        bounds,
        max_instances,
        existing,
        |x, z| (x - center.x).hypot(z - center.y) <= radius_m,
        |x, z| sample(x, z),
        |x, z| ground(x, z),
    )
}

fn weighted_asset(assets: &[BrushAsset], sample: f32) -> &BrushAsset {
    let total = assets
        .iter()
        .map(|asset| asset.weight.max(0.0))
        .sum::<f32>();
    let mut target = sample.clamp(0.0, 0.999_999) * total;
    for asset in assets {
        target -= asset.weight.max(0.0);
        if target <= 0.0 {
            return asset;
        }
    }
    &assets[assets.len() - 1]
}

fn sample_grid(heights: &[f32], res: usize, spacing: f32, x: f32, z: f32) -> f32 {
    if res < 2 || heights.len() != res * res || !spacing.is_finite() || spacing <= 0.0 {
        return 0.0;
    }
    let half = (res as f32 - 1.0) * 0.5;
    let fx = (x / spacing + half).clamp(0.0, res as f32 - 1.0);
    let fz = (z / spacing + half).clamp(0.0, res as f32 - 1.0);
    let x0 = fx.floor() as usize;
    let z0 = fz.floor() as usize;
    let x1 = (x0 + 1).min(res - 1);
    let z1 = (z0 + 1).min(res - 1);
    let tx = fx - x0 as f32;
    let tz = fz - z0 as f32;
    let h00 = heights[z0 * res + x0];
    let h10 = heights[z0 * res + x1];
    let h01 = heights[z1 * res + x0];
    let h11 = heights[z1 * res + x1];
    (h00 + (h10 - h00) * tx) + ((h01 + (h11 - h01) * tx) - (h00 + (h10 - h00) * tx)) * tz
}

fn cell_hash(x: i32, z: i32, seed: u32) -> u32 {
    let mut value =
        seed ^ (x as u32).wrapping_mul(0x9E37_79B9) ^ (z as u32).wrapping_mul(0x85EB_CA6B);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^ (value >> 16)
}

fn hash01(value: u32) -> f32 {
    (cell_hash(value as i32, (value >> 16) as i32, 0xA511_E9B3) & 0x00ff_ffff) as f32 / 16_777_216.0
}

fn value_noise(x: f32, z: f32, seed: u32, scale: f32) -> f32 {
    let scale = scale.max(1.0e-3);
    let fx = x / scale;
    let fz = z / scale;
    let x0 = fx.floor() as i32;
    let z0 = fz.floor() as i32;
    let tx = smoothstep01(fx - x0 as f32);
    let tz = smoothstep01(fz - z0 as f32);
    let sample = |ix, iz| hash01(cell_hash(ix, iz, seed));
    let a = sample(x0, z0) + (sample(x0 + 1, z0) - sample(x0, z0)) * tx;
    let b = sample(x0, z0 + 1) + (sample(x0 + 1, z0 + 1) - sample(x0, z0 + 1)) * tx;
    a + (b - a) * tz
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn kelvin_srgb(kelvin: f32) -> Vec3 {
    // Tanner Helland-style approximation; authoring UI quality is sufficient and deterministic.
    let temperature = (kelvin.clamp(800.0, 40_000.0) / 100.0).max(1.0);
    let red = if temperature <= 66.0 {
        255.0
    } else {
        (329.698_73 * (temperature - 60.0).powf(-0.133_204_76)).clamp(0.0, 255.0)
    };
    let green = if temperature <= 66.0 {
        (99.470_8 * temperature.ln() - 161.119_57).clamp(0.0, 255.0)
    } else {
        (288.122_16 * (temperature - 60.0).powf(-0.075_514_846)).clamp(0.0, 255.0)
    };
    let blue = if temperature >= 66.0 {
        255.0
    } else if temperature <= 19.0 {
        0.0
    } else {
        (138.517_73 * (temperature - 10.0).ln() - 305.044_8).clamp(0.0, 255.0)
    };
    Vec3::new(red, green, blue) / 255.0
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn json_f32_or(value: &serde_json::Value, fallback: f32) -> f32 {
    value
        .as_f64()
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn json_vec3(value: &serde_json::Value, name: &str) -> anyhow::Result<Vec3> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 3)
        .ok_or_else(|| anyhow::anyhow!("{name} must have three numbers"))?;
    let vector = Vec3::new(
        json_f32_or(&values[0], f32::NAN),
        json_f32_or(&values[1], f32::NAN),
        json_f32_or(&values[2], f32::NAN),
    );
    anyhow::ensure!(vector.is_finite(), "{name} contains a non-finite number");
    Ok(vector)
}

fn json_vec3_or(value: &serde_json::Value, fallback: Vec3) -> Vec3 {
    json_vec3(value, "vector").unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EnvironmentSample {
        EnvironmentSample {
            height_m: 12.0,
            slope_deg: 8.0,
            solar_exposure: 0.55,
            soil_moisture: 0.62,
            fertility: 0.70,
            wind_exposure: 0.35,
            destruction: 0.18,
            concavity: 0.0,
        }
    }

    fn recipe() -> BrushRecipe {
        BrushRecipe {
            id: "test_forest".into(),
            name: "Test forest".into(),
            role: "ecology".into(),
            assets: vec![
                BrushAsset {
                    path: "tree.glb".into(),
                    weight: 5.0,
                    scale_min: 0.8,
                    scale_max: 1.2,
                    sink_m: -0.1,
                },
                BrushAsset {
                    path: "rock.glb".into(),
                    weight: 1.0,
                    scale_min: 0.5,
                    scale_max: 1.0,
                    sink_m: -0.05,
                },
            ],
            seed: 77,
            default_radius_m: 30.0,
            density_per_100m2: 8.0,
            min_spacing_m: 3.0,
            cluster_scale_m: 30.0,
            constraints: EnvironmentalConstraints::default(),
        }
    }

    #[test]
    fn light_and_environment_roundtrip() {
        let light = AuthoredLight::point_preset("fire_01".into(), "fire", Vec3::new(1.0, 2.0, 3.0));
        let mut edits = serde_json::json!({});
        edits
            .as_object_mut()
            .unwrap()
            .insert(LIGHTS_KEY.into(), serde_json::json!([light.to_json()]));
        let loaded = lights_from_edits(Some(&edits)).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "fire_01");
        assert!(loaded[0].linear_rgb().is_finite());
        assert!(preview_light_score(&loaded[0], loaded[0].position).unwrap() > 0.0);
        assert!(
            preview_light_score(&loaded[0], loaded[0].position + Vec3::X * loaded[0].range_m)
                .is_none()
        );

        let tuning = LightingTuning {
            enabled: true,
            ..LightingTuning::default()
        };
        let mut edits = serde_json::json!({});
        edits
            .as_object_mut()
            .unwrap()
            .insert(LIGHTING_KEY.into(), tuning.to_json());
        assert_eq!(LightingTuning::from_edits(Some(&edits)).unwrap(), tuning);
        assert!((tuning.sun_direction().length() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn environmental_constraints_gate_each_field() {
        let mut constraints = EnvironmentalConstraints::default();
        constraints.slope_deg = FieldRange::new(0.0, 20.0, 0.1);
        constraints.soil_moisture = FieldRange::new(0.4, 0.9, 0.1);
        assert!(constraints.score(sample()) > 0.4);
        let mut steep = sample();
        steep.slope_deg = 45.0;
        assert_eq!(constraints.score(steep), 0.0);
        let mut dry = sample();
        dry.soil_moisture = 0.1;
        assert_eq!(constraints.score(dry), 0.0);
    }

    #[test]
    fn terrain_fields_respond_to_slope_sun_soil_and_destruction() {
        let res = 9;
        let heights = (0..res)
            .flat_map(|z| (0..res).map(move |x| x as f32 * 0.5 + (z as f32 - 4.0).powi(2) * 0.04))
            .collect::<Vec<_>>();
        let context = EnvironmentContext {
            seed: 9,
            min_height_m: 0.0,
            max_height_m: 8.0,
            sun_direction: Vec3::new(-0.4, 0.8, 0.2).normalize(),
            wind_direction: Vec2::X,
            base_moisture: 0.6,
            base_fertility: 0.7,
            base_destruction: 0.2,
        };
        let calm = sample_heightfield(&heights, res, 2.0, 0.0, 0.0, context, 0.0);
        let blasted = sample_heightfield(&heights, res, 2.0, 0.0, 0.0, context, 1.0);
        assert!(calm.slope_deg > 0.0);
        assert!((0.0..=1.0).contains(&calm.solar_exposure));
        assert!((0.0..=1.0).contains(&calm.soil_moisture));
        assert!(blasted.destruction > calm.destruction);
        assert!(blasted.fertility < calm.fertility);
    }

    #[test]
    fn scatter_is_deterministic_bounded_and_respects_existing_points() {
        let recipe = recipe();
        let run = |existing: &[(f32, f32)]| {
            scatter_rect(
                &recipe,
                [-30.0, -20.0, 30.0, 20.0],
                250,
                existing,
                |_, _| sample(),
                |x, z| x * 0.01 + z * 0.02,
            )
            .unwrap()
        };
        let a = run(&[]);
        let b = run(&[]);
        assert_eq!(a, b);
        assert!(!a.is_empty());
        assert!(a.iter().all(|placement| {
            (-30.0..=30.0).contains(&placement.position.x)
                && (-20.0..=20.0).contains(&placement.position.z)
                && placement.position.is_finite()
        }));
        let blocked = [(a[0].position.x, a[0].position.z)];
        let c = run(&blocked);
        assert!(!c.iter().any(|placement| {
            (placement.position.x - blocked[0].0).hypot(placement.position.z - blocked[0].1)
                < recipe.min_spacing_m * 0.88
        }));
    }

    #[test]
    fn dense_circle_cap_is_spatially_balanced() {
        let mut dense = recipe();
        dense.density_per_100m2 = 1_000.0;
        dense.min_spacing_m = 2.0;
        let placements = scatter_circle(
            &dense,
            Vec2::ZERO,
            30.0,
            17,
            80,
            &[],
            |_, _| sample(),
            |_, _| 0.0,
        )
        .unwrap();
        assert_eq!(placements.len(), 80);
        let mut quadrants = [false; 4];
        for placement in placements {
            assert!(placement.position.x.hypot(placement.position.z) <= 30.0);
            let quadrant = usize::from(placement.position.x >= 0.0)
                + usize::from(placement.position.z >= 0.0) * 2;
            quadrants[quadrant] = true;
        }
        assert!(quadrants.into_iter().all(|occupied| occupied));
    }

    #[test]
    fn whole_map_budget_limits_expected_outputs_and_candidate_grid() {
        let source = recipe();
        let bounds = [-2_600.0, -2_600.0, 2_600.0, 2_600.0];
        let budgeted = budget_whole_map_recipe(&source, bounds, 12_000).unwrap();
        let area = (bounds[2] - bounds[0]) * (bounds[3] - bounds[1]);
        let expected = budgeted.density_per_100m2 * area / 100.0;
        let candidate_cells_x = (bounds[2] / budgeted.min_spacing_m).ceil()
            - (bounds[0] / budgeted.min_spacing_m).floor()
            + 1.0;
        let candidate_cells_z = (bounds[3] / budgeted.min_spacing_m).ceil()
            - (bounds[1] / budgeted.min_spacing_m).floor()
            + 1.0;
        let candidate_cells = candidate_cells_x * candidate_cells_z;
        assert!(expected <= 12_000.1);
        assert!(candidate_cells <= 12_000.0 * 16.0);
        assert!(budgeted.min_spacing_m > source.min_spacing_m);
    }

    #[test]
    fn layer_recipe_json_preserves_environment_rules() {
        let layer = ProceduralLayerRecord {
            id: "forest_initial".into(),
            recipe: recipe(),
            bounds: [-100.0, -80.0, 100.0, 80.0],
            generated_count: 73,
        };
        let mut edits = serde_json::json!({});
        edits
            .as_object_mut()
            .unwrap()
            .insert(LAYERS_KEY.into(), serde_json::json!([layer.to_json()]));
        assert_eq!(layers_from_edits(Some(&edits)).unwrap(), vec![layer]);
    }

    #[test]
    fn custom_mix_identity_is_order_independent_and_duplicate_layers_are_rejected() {
        let left = BrushRecipe::custom(["tree.glb".into(), "rock.glb".into()], 9).unwrap();
        let right = BrushRecipe::custom(["rock.glb".into(), "tree.glb".into()], 9).unwrap();
        assert_eq!(left, right);

        let layer = ProceduralLayerRecord {
            id: "duplicate".into(),
            recipe: left,
            bounds: [-1.0, -1.0, 1.0, 1.0],
            generated_count: 1,
        };
        let mut edits = serde_json::json!({});
        edits.as_object_mut().unwrap().insert(
            LAYERS_KEY.into(),
            serde_json::json!([layer.to_json(), layer.to_json()]),
        );
        assert!(layers_from_edits(Some(&edits)).is_err());
    }
}
