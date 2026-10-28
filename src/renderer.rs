//! The Panzerrechnersystem renderer.
//!
//! Flow per frame, all at internal resolution until the final upscale:
//!   sky + raster primary → exact ray-query surface transport → participating-media transport
//!   → exposure/retro treatment → nearest-neighbour upscale to display (or PNG). Devices without
//!   ray queries use the explicitly separate forward raster fallback.

#[path="smoke_support.rs"]
mod smoke_support;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::ui::{Align, Ui};

#[path = "renderer_fine_vfx.rs"]
mod renderer_fine_vfx;
#[path = "renderer_weapon_particles.rs"]
pub(crate) mod renderer_weapon_particles;
#[path = "renderer_raymarch.rs"]
pub(crate) mod raymarch;

#[path = "track_ruts.rs"]
mod track_ruts;

#[path = "held_map_render.rs"]
mod held_map_render;

#[path = "light_history.rs"]
mod light_history;

#[path = "renderer_scene_visibility.rs"]
mod renderer_scene_visibility;

#[path = "terrain_shadow_pages.rs"]
mod terrain_shadow_pages;
#[path = "rigid_triangle_pages.rs"]
mod rigid_triangle_pages;

#[path = "surface_reconstruction.rs"]
mod surface_reconstruction;

#[path = "live_texel_probe.rs"]
mod live_texel_probe;

#[path = "physical_environment.rs"]
pub(crate) mod physical_environment;

#[path = "display_tuning.rs"]
pub mod display_tuning;
#[path = "renderer_look_review.rs"]
mod look_review;

use physical_environment::{
    AtmosphereParameters, GpuPhysicalEnvironment, IntegrationQuality,
};

pub const INTERNAL_W: u32 = 640;
pub const INTERNAL_H: u32 = 360;
const INTERNAL_RES_ENV: &str = "KESSEL_INTERNAL_RES";
const SHADOW_RES_ENV: &str = "KESSEL_SHADOW_RES";
const CAMERA_EXPOSURE_ENV: &str = "KESSEL_CAMERA_EXPOSURE";
/// Diagnostic A/B only: restore the former ordering (fullscreen sky before opaque draws).
/// Shipping defaults to depth-gated sky; the transport and its samples are otherwise identical.
const UNGATED_SKY_ENV: &str = "KESSEL_UNGATED_SKY";
pub const INTERNAL_RES_PRESETS: [(u32, u32); 13] =
    [(320, 180), (320, 240), (480, 270), (480, 272), (576, 324), (640, 360), (800, 450), (800, 480), (960, 540), (1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
const RETRO_PRESET_ENV: &str = "KESSEL_RETRO_PRESET";
const DEFAULT_MASTER_RETRO: f32 = 0.7;
const PIPELINE_CACHE_DIR_ENV: &str = "KESSEL_PIPELINE_CACHE_DIR";
const TRANSPORT_QUALITY_ENV: &str = "KESSEL_TRANSPORT_QUALITY";

/// An opt-in numerical budget, independent of raster resolution and hardware ray visibility.
/// Both budgets preserve diffuse and metallic environment reflection plus both glass interfaces.
/// Numerical media quadrature may differ; a budget never removes a material's entire light path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportQuality {
    Full,
    Gameplay,
}

impl TransportQuality {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "gameplay" => Some(Self::Gameplay),
            _ => None,
        }
    }

    fn configured() -> Self {
        match std::env::var(TRANSPORT_QUALITY_ENV) {
            Ok(raw) => Self::parse(&raw).unwrap_or_else(|| {
                eprintln!("[renderer] invalid {TRANSPORT_QUALITY_ENV}={raw:?}; using full");
                Self::Full
            }),
            Err(_) => Self::Full,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Gameplay => "gameplay",
        }
    }

    fn environment(self, preset: crate::graphics::GraphicsPreset) -> IntegrationQuality {
        if let Some([view, source, disk, cloud]) = experiment_values::<4>("KESSEL_RT_EXPERIMENT_ENVIRONMENT") {
            return IntegrationQuality { view_steps: view as u32, source_steps: source as u32,
                solar_disk_samples: disk as u32, cloud_column_steps: cloud as u32 };
        }
        if self == Self::Gameplay || preset == crate::graphics::GraphicsPreset::Laptop {
            IntegrationQuality {
                view_steps: 12,
                source_steps: 8,
                solar_disk_samples: 1,
                cloud_column_steps: 8,
            }
        } else {
            IntegrationQuality::INTERACTIVE_HIGH
        }
    }

    fn smoke_steps(self) -> [f32; 4] {
        if let Some(steps) = experiment_values::<4>("KESSEL_RT_EXPERIMENT_SMOKE") { return steps; }
        match self {
            Self::Full => [18.0, 22.0, 26.0, 6.0],
            Self::Gameplay => [8.0, 8.0, 12.0, 4.0],
        }
    }

    fn glass_world_steps(self) -> f32 {
        if let Some([steps]) = experiment_values::<1>("KESSEL_RT_EXPERIMENT_GLASS") { return steps; }
        match self {
            Self::Full => 12.0,
            Self::Gameplay => 6.0,
        }
    }

    fn surface_passes(self) -> [f32; 4] {
        if let Some(passes) = experiment_values::<4>("KESSEL_RT_EXPERIMENT_PASSES") { return passes; }
        match self {
            Self::Full => [2.0, 2.0, 1.0, 0.0],
            Self::Gameplay => [2.0, 2.0, 4.0, 0.0],
        }
    }
}

/// Render-push experiments only: a comma-separated override of one sample-count table, read
/// once. Unset in every launcher; the shipping tables above are what play uses.
fn experiment_values<const N: usize>(name: &'static str) -> Option<[f32; N]> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<&'static str, Option<Vec<f32>>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    let values = cache.entry(name).or_insert_with(|| std::env::var(name).ok().and_then(|raw|
        raw.split(',').map(|v| v.trim().parse::<f32>().ok()).collect::<Option<Vec<_>>>()));
    values.as_ref().filter(|v| v.len() == N).map(|v| std::array::from_fn(|i| v[i]))
}

fn ensure_output_parent(path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// `request_device` records the exact adapter-class key immediately before constructing its
/// device. Panzerrechnersystem initializes one adapter/device at a time, so this mutex also prevents a later
/// headless command in the same process from reusing another adapter's unsafe cache bytes.
static PIPELINE_CACHE_KEY: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn configure_pipeline_cache(adapter_info: &wgpu::AdapterInfo) {
    let key = wgpu::util::pipeline_cache_key(adapter_info);
    let slot = PIPELINE_CACHE_KEY.get_or_init(|| Mutex::new(None));
    *slot.lock().expect("pipeline cache key mutex poisoned") = key;
}

fn pipeline_cache_file() -> Option<PathBuf> {
    let key = PIPELINE_CACHE_KEY.get()?.lock().ok()?.as_ref().cloned()?;
    let root = std::env::var_os(PIPELINE_CACHE_DIR_ENV)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .map(|home| {
                    let home = PathBuf::from(home);
                    if cfg!(target_os = "macos") {
                        home.join("Library").join("Caches")
                    } else {
                        home.join(".cache")
                    }
                })
        })
        .unwrap_or_else(std::env::temp_dir);
    Some(
        root.join("kessel")
            .join("wgpu")
            .join(format!("{}-{key}.bin", if crate::graphics::shader_backend_enabled() { "shader-graphics-v1" } else { "physical-transport-v1" })),
    )
}

fn create_physical_pipeline_cache(
    device: &wgpu::Device,
) -> (Option<wgpu::PipelineCache>, Option<PathBuf>) {
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return (None, None);
    }
    let path = pipeline_cache_file();
    let data = path
        .as_deref()
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| !bytes.is_empty());
    if let (Some(path), Some(bytes)) = (path.as_deref(), data.as_deref()) {
        println!(
            "[renderer] loading {} KiB Vulkan pipeline cache from {}",
            bytes.len() / 1024,
            path.display(),
        );
    }
    // SAFETY: cache files are written only from PipelineCache::get_data below and are namespaced
    // by wgpu's Vulkan vendor/device key. wgpu additionally validates driver compatibility;
    // `fallback: true` creates an empty cache if an old driver cannot consume the stored blob.
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("kessel-physical-transport-pipeline-cache"),
            data: data.as_deref(),
            fallback: true,
        })
    };
    (Some(cache), path)
}

fn persist_physical_pipeline_cache(cache: Option<&wgpu::PipelineCache>, path: Option<&Path>) {
    let (Some(cache), Some(path)) = (cache, path) else {
        return;
    };
    let Some(data) = cache.get_data() else { return };
    let Some(parent) = path.parent() else { return };
    if let Err(error) = std::fs::create_dir_all(parent) {
        eprintln!(
            "[renderer] could not create Vulkan pipeline cache directory {}: {error}",
            parent.display(),
        );
        return;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    if let Err(error) =
        std::fs::write(&temporary, &data).and_then(|()| std::fs::rename(&temporary, path))
    {
        eprintln!(
            "[renderer] could not persist Vulkan pipeline cache {}: {error}",
            path.display(),
        );
        return;
    }
    println!(
        "[renderer] persisted {} KiB Vulkan pipeline cache to {}",
        data.len() / 1024,
        path.display(),
    );
}

fn parse_internal_resolution(raw: &str) -> Result<(u32, u32), &'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    let (width, height) = normalized.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    let width = width
        .trim()
        .parse::<u32>()
        .map_err(|_| "width is not a positive integer")?;
    let height = height
        .trim()
        .parse::<u32>()
        .map_err(|_| "height is not a positive integer")?;
    INTERNAL_RES_PRESETS
        .contains(&(width, height))
        .then_some((width, height))
        .ok_or("unsupported internal resolution preset")
}

thread_local! {
    /// One-shot runtime override used when the menu builds a four-cell couch world. Thread-local
    /// state keeps parallel headless tests/builders independent; `configured_internal_resolution`
    /// consumes it exactly once, so later solo/menu rebuilds return to their normal preset.
    static DISPLAY_INTERNAL_RES: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    static NEXT_INTERNAL_RES: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
}

pub fn set_next_internal_resolution(width: u32, height: u32) {
    assert!(
        INTERNAL_RES_PRESETS.contains(&(width, height)),
        "unsupported internal resolution {width}x{height}"
    );
    NEXT_INTERNAL_RES.with(|slot| slot.set(Some((width, height))));
}

pub fn set_display_internal_resolution(size: Option<(u32, u32)>) {
    assert!(size.is_none_or(|s| INTERNAL_RES_PRESETS.contains(&s)));
    DISPLAY_INTERNAL_RES.with(|slot| slot.set(size));
}

pub(crate) fn display_dither_mode(name: &str) -> Option<u8> {
    match name.trim() {
        "" | "bayer4" => Some(0), "blue_noise64" => Some(1),
        "sphere_bayer8" => Some(2), "screen_bayer8" => Some(3), _ => None,
    }
}
fn configured_display_dither() -> f32 {
    let requested=std::env::var("KESSEL_DITHER_PATTERN").unwrap_or_default();
    let mode=display_dither_mode(&requested).unwrap_or_else(|| {
        eprintln!("[display] unknown dither pattern {requested:?}; using Bayer4");0
    });
    let label=["screen Bayer4","fixed blue noise64","Obra Dinn-inspired sphere Bayer8 (four threshold samples)",
        "native screen Bayer8 (one threshold per pixel)"][mode as usize];
    println!("[display] dither: {label}");
    mode as f32
}

fn configured_internal_resolution() -> (u32, u32) {
    if let Some(size) = DISPLAY_INTERNAL_RES.with(Cell::get) {
        // Consume any couch one-shot: it must not leak after leaving the display preview.
        NEXT_INTERNAL_RES.with(Cell::take);
        return size;
    }
    debug_assert_eq!(
        crate::graphics::GraphicsPreset::High.internal_resolution(),
        (INTERNAL_W, INTERNAL_H)
    );
    let preset = crate::graphics::active_preset();
    let fallback = preset.internal_resolution();
    // Environment remains the outermost capture/diagnostic override. In ordinary play the
    // one-shot couch cell size then wins over the preset's full-frame resolution.
    match std::env::var(INTERNAL_RES_ENV) {
        Ok(raw) => match parse_internal_resolution(&raw) {
            Ok(size) => {
                println!(
                    "[renderer] internal resolution {}x{} ({INTERNAL_RES_ENV})",
                    size.0, size.1
                );
                size
            }
            Err(reason) => {
                eprintln!(
                    "[renderer] invalid {INTERNAL_RES_ENV}={raw:?}: {reason}; using {}x{}",
                    fallback.0, fallback.1
                );
                fallback
            }
        },
        Err(std::env::VarError::NotPresent) => {
            if let Some(size) = NEXT_INTERNAL_RES.with(Cell::take) {
                println!(
                    "[renderer] internal resolution {}x{} (one-shot runtime profile)",
                    size.0, size.1
                );
                size
            } else {
                println!(
                    "[renderer] internal resolution {}x{} ({} preset)",
                    fallback.0,
                    fallback.1,
                    preset.label(),
                );
                fallback
            }
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[renderer] invalid non-Unicode {INTERNAL_RES_ENV}; using {}x{}",
                fallback.0, fallback.1
            );
            fallback
        }
    }
}

fn parse_shadow_resolution(raw: &str) -> Result<u32, &'static str> {
    let resolution = raw
        .trim()
        .parse::<u32>()
        .map_err(|_| "expected an integer texture side")?;
    [512, 768, 1024, 1536, 2048]
        .contains(&resolution)
        .then_some(resolution)
        .ok_or("allowed shadow sides are 512, 768, 1024, 1536, and 2048")
}

thread_local! { static MENU_SHADOW_RES: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) }; }

fn configured_shadow_resolution(preset: crate::graphics::GraphicsPreset) -> u32 {
    if let Some(size) = MENU_SHADOW_RES.get() { return size; }
    let fallback = preset.shadow_resolution();
    match std::env::var(SHADOW_RES_ENV) {
        Ok(raw) => match parse_shadow_resolution(&raw) {
            Ok(resolution) => {
                println!("[renderer] shadow resolution {resolution} ({SHADOW_RES_ENV})");
                resolution
            }
            Err(reason) => {
                eprintln!(
                    "[renderer] invalid {SHADOW_RES_ENV}={raw:?}: {reason}; using {fallback}"
                );
                fallback
            }
        },
        Err(_) => fallback,
    }
}

fn parse_retro_preset(raw: &str) -> Result<f32, &'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "clean" | "off" | "none" => Ok(0.0),
        "light" | "minimal" => Ok(0.25),
        "standard" | "default" => Ok(DEFAULT_MASTER_RETRO),
        "strong" | "max" => Ok(1.0),
        _ => Err("allowed presets are clean, light, standard, and strong"),
    }
}

fn configured_master_retro() -> f32 {
    match std::env::var(RETRO_PRESET_ENV) {
        Ok(raw) => match parse_retro_preset(&raw) {
            Ok(value) => {
                println!("[renderer] retro preset {raw:?} = {value:.2} ({RETRO_PRESET_ENV})");
                value
            }
            Err(reason) => {
                eprintln!(
                    "[renderer] invalid {RETRO_PRESET_ENV}={raw:?}: {reason}; using standard ({DEFAULT_MASTER_RETRO:.2})"
                );
                DEFAULT_MASTER_RETRO
            }
        },
        Err(std::env::VarError::NotPresent) => DEFAULT_MASTER_RETRO,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!(
                "[renderer] invalid non-Unicode {RETRO_PRESET_ENV}; using standard ({DEFAULT_MASTER_RETRO:.2})"
            );
            DEFAULT_MASTER_RETRO
        }
    }
}

/// A fixed camera exposure in place of the eye exists only behind the debug gate
/// (`debug_gates`, `KESSEL_CAMERA_EXPOSURE` in a debug build). Shipping frames are always shown by
/// the crewman's eye.
/// With the day and night cycle on: five minutes of full day, two and a half each of sunrise
/// and sunset, five of night. Fifteen minutes in all, ten of them with daylight.
pub const FULL_DAY_S: f64 = 300.0;
pub const TWILIGHT_S: f64 = 150.0;
pub const NIGHT_S: f64 = 300.0;
/// Sunrise and sunset run from the end of civil twilight to the end of the golden hour.
const TWILIGHT_FROM_DEG: f64 = -6.0;
const TWILIGHT_TO_DEG: f64 = 8.0;
/// 0 night, 1 sunrise or sunset, 2 full day, by the Sun's altitude.
fn day_phase(sun_altitude_deg: f64) -> usize {
    if sun_altitude_deg < TWILIGHT_FROM_DEG { 0 } else if sun_altitude_deg < TWILIGHT_TO_DEG { 1 } else { 2 }
}
/// On in play. Off for unit tests and wherever `KESSEL_DAY_NIGHT=0` asks for the real-time clock
/// (stills and films that must hold their authored light).
fn day_night_cycle_enabled() -> bool {
    match std::env::var("KESSEL_DAY_NIGHT").ok().as_deref() {
        Some("0") => false,
        Some("1") => true,
        _ => !cfg!(test),
    }
}
/// Exposure and saturation of the display grade at full boost.
pub const VISION_BOOST_EXPOSURE: f32 = 1.9;
pub const VISION_BOOST_SATURATION: f32 = 1.18;
fn boosted_display_grade(grade: [f32; 4], boost: f32) -> [f32; 4] {
    let boost = if boost.is_finite() { boost.clamp(0.0, 1.0) } else { 0.0 };
    [grade[0] * (1.0 + (VISION_BOOST_EXPOSURE - 1.0) * boost),
     grade[1] * (1.0 + (VISION_BOOST_SATURATION - 1.0) * boost), grade[2], grade[3]]
}
fn configured_camera_exposure_override() -> Option<f32> {
    let value = crate::debug_gates::DebugGates::active().fixed_camera_exposure?;
    println!("[renderer] fixed camera exposure {value:.3} ({CAMERA_EXPOSURE_ENV}, debug gate); the eye is bypassed");
    Some(value)
}

/// The scene-referred frame's storage format. Half floats hold radiance down to 6e-8 units
/// (0.004 cd/m2) and only in steps of that size, so a moonlit field stored as exactly zero and
/// the night sky in a handful of steps. Where the adapter can blend, filter and multisample
/// 32-bit float targets the frame is stored in those; the light and every shader are the same.
static HDR_FORMAT: std::sync::OnceLock<wgpu::TextureFormat> = std::sync::OnceLock::new();
pub(crate) fn hdr_format() -> wgpu::TextureFormat {
    *HDR_FORMAT.get().unwrap_or(&wgpu::TextureFormat::Rgba16Float)
}
/// A compute shader that stores into a frame-format target names that format in its source.
pub(crate) fn hdr_storage_source(source: &str) -> String {
    if hdr_format() == wgpu::TextureFormat::Rgba32Float { source.replace("rgba16float", "rgba32float") }
    else { source.to_owned() }
}
/// The device features a 32-bit float frame needs. `KESSEL_HDR_HALF=1` keeps half floats.
pub(crate) const HDR_FULL_FLOAT_FEATURES: wgpu::Features = wgpu::Features::FLOAT32_FILTERABLE
    .union(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
/// Call once, before any renderer exists, with the features the device was granted.
pub(crate) fn choose_hdr_format(adapter: &wgpu::Adapter, granted: wgpu::Features) -> wgpu::TextureFormat {
    let full = wgpu::TextureFormat::Rgba32Float;
    let flags = adapter.get_texture_format_features(full).flags;
    let able = granted.contains(HDR_FULL_FLOAT_FEATURES)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        && flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
        && flags.contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        && std::env::var_os("KESSEL_HDR_HALF").is_none();
    *HDR_FORMAT.get_or_init(|| if able { full } else { wgpu::TextureFormat::Rgba16Float })
}
// Keep the tone-mapped frame in display codes. Linear RGBA8 erased the first
// 31-level palette entry in dark interiors and its compensating dither made
// alternating black holes. Attachment writes encode; sampling decodes, so the
// blit, particle blend, UI and optic snapshot continue to work in linear light.
const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Shadow-map side is selected once with the renderer: Laptop uses 512², High/4090 uses 2048².
/// The internal render targets likewise remain fixed until a world/renderer rebuild.
const SOLO_SHADOW_HALF_M: f32 = 240.0;
const FAR_SHADOW_DEPTH_HALF_M: f32 = 700.0;
const SOLO_SHADOW_LOOK_AHEAD_M: f32 = 30.0;
/// Four crew cameras can face opposing directions from the same tank. The union of their previous
/// far cascades is conservatively bounded by their union around the tank.
const COOP_SHADOW_HALF_M: f32 = SOLO_SHADOW_HALF_M + SOLO_SHADOW_LOOK_AHEAD_M;

/// Direction TO the winter sun (matches the value mesh.wgsl shaded with before it was lifted to the
/// uniform). Kept here as the single Rust-side source so the shadow cascade projects from exactly
/// the direction the fragment shader lights with.
// Default sun; per-act mood overrides via `Renderer::sun_dir` (S4 made it state, not a const).
const SUN_DIR: glam::Vec3 = glam::Vec3::new(-0.5, 0.55, 0.25);

fn shadow_view_proj_for_focus(sun_dir: glam::Vec3, focus: glam::Vec3, half: f32) -> glam::Mat4 {
    let sun = sun_dir.normalize();
    let eye = focus + sun * FAR_SHADOW_DEPTH_HALF_M;
    let view = glam::Mat4::look_to_rh(eye, -sun, glam::Vec3::Y);
    let proj = glam::Mat4::orthographic_rh(-half, half, -half, half, 0.1, FAR_SHADOW_DEPTH_HALF_M*2.0);
    proj * view
}

/// Stabilize the light-space lattice in world coordinates. Camera rotation never rotates this
/// grid, and sub-texel camera movement cannot crawl a thin machinery shadow over its receiver.
fn stable_shadow_view_proj(sun_dir: glam::Vec3, focus: glam::Vec3, half: f32,
    depth_half: f32, resolution: u32) -> glam::Mat4 {
    let sun = sun_dir.normalize();
    let up = if sun.y.abs() > 0.98 { glam::Vec3::Z } else { glam::Vec3::Y };
    let orientation = glam::Mat4::look_to_rh(glam::Vec3::ZERO,-sun,up);
    let texel = 2.0 * half / resolution.max(1) as f32;
    let focus_light = (orientation.transform_point3(focus) / texel).round() * texel;
    let eye_light = focus_light + glam::Vec3::Z * depth_half;
    glam::Mat4::orthographic_rh(-half,half,-half,half,0.1,2.0*depth_half)
        * glam::Mat4::from_translation(-eye_light) * orientation
}

/// Conservative raster sample coverage. A source AABB with no shadow texel center in its
/// projection cannot write the single-sample depth map. This does not drop small casters by
/// size: a subpixel caster crossing a sample center is retained. Ambiguous near-plane boxes
/// are retained, and a 1/8-pixel guard covers subpixel raster snapping and f32 projection disagreement.
fn shadow_has_sample(matrix:glam::Mat4,lo:glam::Vec3,hi:glam::Vec3,size:u32)->bool {
    if !lo.is_finite() || !hi.is_finite() || size==0 {return true;}
    let mut mn=glam::Vec3::splat(f32::INFINITY);let mut mx=glam::Vec3::splat(f32::NEG_INFINITY);
    for x in [lo.x,hi.x] {for y in [lo.y,hi.y] {for z in [lo.z,hi.z] {
        let h=matrix*glam::Vec4::new(x,y,z,1.0);
        if !h.is_finite() || h.w<=0.00001 {return true;}
        let p=h.truncate()/h.w;mn=mn.min(p);mx=mx.max(p);
    }}}
    if mx.z<0.0 || mn.z>1.0 {return false;}
    let a=(mn.truncate()*0.5+glam::Vec2::splat(0.5))*size as f32;
    let b=(mx.truncate()*0.5+glam::Vec2::splat(0.5))*size as f32;
    let first=(a-glam::Vec2::splat(0.625)).ceil().max(glam::Vec2::ZERO);
    let last=(b-glam::Vec2::splat(0.375)).floor().min(glam::Vec2::splat(size as f32-1.0));
    first.cmple(last).all()
}
/// Return the exact source-instance draw ranges that can reach a shadow-map sample. Static
/// world pages keep their existing page/frustum submission; this targets small moving pools.
fn shadow_instance_ranges(batch:&InstBatch,prototype:&InstPrototype,matrix:glam::Mat4,size:u32)->Vec<std::ops::Range<u32>> {
    let mut ranges:Vec<std::ops::Range<u32>>=Vec::new();
    for (i,model) in batch.cpu_mats.iter().take(batch.inst_count as usize).enumerate() {
        if batch.is_hidden(i as u32){continue;}
        let (lo,hi)=transform_aabb(*model,prototype.local_min,prototype.local_max);
        if !shadow_has_sample(matrix,lo,hi,size){continue;}
        let i=i as u32;
        if let Some(last)=ranges.last_mut().filter(|r|r.end==i){last.end+=1;}else{ranges.push(i..i+1);}
    }
    ranges
}

#[cfg(test)]
mod shadow_coverage_tests {
    use super::*;
    #[test]
    fn subpixel_casters_are_kept_when_they_can_cover_a_sample() {
        let matrix=glam::Mat4::IDENTITY;
        let world=|pixel:f32|pixel/8.0*2.0-1.0;
        let box_at=|x:f32,y:f32| {
            let center=glam::Vec3::new(world(x),world(y),0.5);
            (center-glam::Vec3::splat(0.001),center+glam::Vec3::splat(0.001))
        };
        let (lo,hi)=box_at(3.5,4.5);assert!(shadow_has_sample(matrix,lo,hi,8));
        let (lo,hi)=box_at(3.1,4.1);assert!(!shadow_has_sample(matrix,lo,hi,8));
        let (lo,hi)=box_at(3.38,4.38);assert!(shadow_has_sample(matrix,lo,hi,8),"rounding guard retains near-edge samples");
    }
    #[test]
    fn clipped_or_ambiguous_source_bounds_never_invent_visibility() {
        let p=glam::Mat4::perspective_rh(1.2,1.0,0.01,1000.0);
        assert!(shadow_has_sample(p,glam::Vec3::splat(-1.0),glam::Vec3::splat(1.0),512),"near-plane crossing retained");
        assert!(!shadow_has_sample(glam::Mat4::IDENTITY,glam::Vec3::new(2.0,2.0,0.2),glam::Vec3::new(3.0,3.0,0.8),384));
        assert!(!shadow_has_sample(glam::Mat4::IDENTITY,glam::Vec3::new(-1.0,-1.0,1.1),glam::Vec3::new(1.0,1.0,2.0),384));
        assert!(shadow_has_sample(glam::Mat4::IDENTITY,glam::Vec3::splat(f32::NAN),glam::Vec3::ONE,384));
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NearShadowUniform {
    view_proj: [[f32;4];4],
    // world texel size, depth range, far-map world texel size, far depth range
    scale: [f32;4],
}

/// Ephemeral co-op-only shadow state. `view_proj` is created immediately before the first living
/// crew viewport and discarded immediately after the last, so RTT, solo and the next simulation
/// frame cannot inherit a stale light matrix. Later co-located views sample the exact matrix whose
/// depth map the first view rendered.
#[derive(Default)]
struct SharedShadowFrame {
    view_proj: Option<glam::Mat4>,
    reuse: bool,
}

impl SharedShadowFrame {
    fn begin(&mut self, sun_dir: glam::Vec3, tank_center: glam::Vec3) {
        self.view_proj = Some(shadow_view_proj_for_focus(
            sun_dir,
            tank_center,
            COOP_SHADOW_HALF_M,
        ));
        self.reuse = false;
    }

    fn set_reuse(&mut self, reuse: bool) -> bool {
        let previous = self.reuse;
        self.reuse = reuse && self.view_proj.is_some();
        previous
    }

    fn end(&mut self) {
        self.view_proj = None;
        self.reuse = false;
    }

    fn view_proj_or(&self, fallback: glam::Mat4) -> glam::Mat4 {
        self.view_proj.unwrap_or(fallback)
    }

    fn should_render(&self) -> bool {
        !self.reuse || self.view_proj.is_none()
    }
}

// ── Main-menu turntable framing (internal-res menu camera) ───────────────────────────────────────
// The showcase Tiger is parked in the RIGHT third of the frame, vertically centred, spun about its
// vertical axis on a pure-black backdrop. The camera looks slightly down from front-of-centre; the
// tank is then shifted along the camera's right vector so it sits in the right third (leaving the
// left ~2/3 clear for the front-end text). Tuned against the headless `ui menu` preview.
const MENU_CAM_POS: glam::Vec3 = glam::Vec3::new(0.0, 3.2, 12.5);
const MENU_CAM_TARGET: glam::Vec3 = glam::Vec3::new(0.0, 1.35, 0.0);
const MENU_FOV_DEG: f32 = 36.0;
const MENU_RIGHT_SHIFT: f32 = 3.7; // world metres along the camera's right → pushes the tank right

/// Max positioned point lights the mesh pass evaluates per fragment. Modeled fixtures, muzzle
/// flashes and fire glow are the working set; cheap at internal resolution.
pub const N_LIGHTS: usize = 32;

/// A modeled compact spherical emitter. `source_radius` is the physical luminous radius in metres,
/// used only to regularize the near field of the inverse-square law; it is never an illumination
/// cutoff. `radiant_intensity` is the source power in the renderer's linear radiometric scale.
#[derive(Clone, Copy)]
pub struct PointLight {
    pub pos: glam::Vec3,
    pub source_radius: f32,
    pub color: glam::Vec3,
    pub radiant_intensity: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4], // camera-relative world-vector projection, translation removed
    light_view_proj: [[f32; 4]; 4], // sun ortho cascade; shared by mesh.wgsl + shadow.wgsl
    cam_pos: [f32; 3],
    fog_density: f32,
    sun_dir: [f32; 3],
    night: f32, // 0 = day (Tag), 1 = night (Nacht) — branches the sky/sun/ambient + gates emissives
    snap_res: [f32; 2],
    interior_light: f32, // 0 = open/exposed, 1 = buttoned-up; view/shelter state, never light energy
    time: f32,           // seconds, accumulated by the sim — drives grass wind sway
    track_half: f32,     // act half-extent / enable flag; sparse track cells remain 0.125 m
    n_lights: f32,       // active positioned point lights (0..N_LIGHTS)
    _pad2: f32,
    _pad3: f32,
    // Point lights: 2 × vec4 each — [px,py,pz,radius], [r,g,b,intensity]. 8 lights = 16 vec4.
    lights: [[f32; 4]; N_LIGHTS * 2],
    // Appended (S2): world reconstruction from depth in the deferred combine pass. Shaders that
    // declare the shorter prefix struct (mesh/sky/smoke) are unaffected — trailing bytes ignored.
    inv_view_proj: [[f32; 4]; 4],
    // Campaign atmosphere, kept as one aligned vec4 so older/shorter shader camera prefixes stay
    // valid: overcast, storm strength, act mood (0 winter / .5 Oder / 1 Halbe), dawn↔dusk tint.
    atmosphere: [f32; 4],
    // Exact shared bounded-world-medium profile used by the screen volume integrator and by rays
    // that continue beyond a refractive viewport: sigma_t, snow optical fraction, bottom_y, top_y.
    // This is trailing so every shorter raster Camera declaration remains ABI-compatible.
    world_medium: [f32; 4],
    // Runtime-visible trip counts for combine.wgsl: x = glass interface passes, y = one-bounce
    // estimators (full = 2, gameplay = diffuse only), z = optical terminal sky samples (1 / 4).
    // These lanes also prevent NVIDIA's Vulkan
    // compiler from statically unrolling those loops into duplicate inlined copies of the secondary
    // transport and its nested physical-environment quadratures. Trailing for the same ABI reason.
    transport_passes: [f32; 4],
    cabin_medium: cabin_medium::Uniform,
    medium_transport:[f32;4], // cabin extinction, exterior weather displacement xyz
}

/// Per-pass push constant for snow.wgsl. Four aligned lanes keep the layout identical on Metal and
/// Vulkan: inverse VP, camera + precipitation amount, world wind + simulation time, then the
/// deterministic weather state. 112 bytes stays below the project's 128-byte push-constant floor.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SnowPush {
    inv_view_proj: [[f32; 4]; 4],
    camera_amount: [f32; 4], // xyz camera world position, w precipitation amount
    wind_time: [f32; 4],     // xz world wind, storm strength, simulation time
    weather: [f32; 4],       // night, shelter, exact-24-bit seed, deterministic gust
}

const WEATHER_SEED_MASK: u32 = 0x00ff_ffff;

/// Collapse an arbitrary act/world seed to an exactly representable f32 integer while preserving
/// good bit diffusion. The shader converts it back to u32 before its integer PCG placement hash.
fn weather_seed_token(mut seed: u32) -> u32 {
    seed ^= seed >> 16;
    seed = seed.wrapping_mul(0x7feb_352d);
    seed ^= seed >> 15;
    seed = seed.wrapping_mul(0x846c_a68b);
    seed ^= seed >> 16;
    seed & WEATHER_SEED_MASK
}

/// Low-frequency pressure waves shared by every view. This is simulation-time + act-seed driven,
/// never render-frame driven, so split-screen order and RTT refresh cadence cannot change a gust.
fn weather_gust(time: f32, seed: u32) -> f32 {
    let phase =
        weather_seed_token(seed) as f32 / (WEATHER_SEED_MASK as f32 + 1.0) * std::f32::consts::TAU;
    let slow = (time * 0.17 + phase).sin();
    let fast = (time * 0.43 + phase * 1.71 + 0.83).sin();
    (0.86 + slow * 0.16 + fast * 0.10).clamp(0.60, 1.12)
}

fn profile_weather_wind(act_id: u8, exposure: f32) -> glam::Vec2 {
    let profile = crate::world::weather_wind_profile(act_id);
    let speed = profile.sheltered_speed_m_s
        + (profile.exposed_speed_m_s - profile.sheltered_speed_m_s) * exposure.clamp(0.0, 1.0);
    profile.direction * speed
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CampaignAtmosphereProfile {
    overcast: f32,
    storm: f32,
    mood: f32,
    ground_extinction_m_inv: f32,
    wind_exposure: f32,
    mie_scale_height_m: f64,
    mie_scattering_m_inv: f64,
    mie_extinction_m_inv: f64,
    mie_asymmetry: f64,
    cloud_bottom_m: f64,
    cloud_top_m: f64,
    cloud_lwc_kg_m3: f64,
    cloud_droplet_radius_m: f64,
}

impl CampaignAtmosphereProfile {
    fn toward(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        macro_rules! f { ($name:ident) => { self.$name + (other.$name - self.$name) * t }; }
        macro_rules! d { ($name:ident) => { self.$name + (other.$name - self.$name) * t as f64 }; }
        Self {
            overcast: f!(overcast), storm: f!(storm), mood: f!(mood),
            ground_extinction_m_inv: f!(ground_extinction_m_inv), wind_exposure: f!(wind_exposure),
            mie_scale_height_m: d!(mie_scale_height_m), mie_scattering_m_inv: d!(mie_scattering_m_inv),
            mie_extinction_m_inv: d!(mie_extinction_m_inv), mie_asymmetry: d!(mie_asymmetry),
            cloud_bottom_m: d!(cloud_bottom_m), cloud_top_m: d!(cloud_top_m),
            cloud_lwc_kg_m3: d!(cloud_lwc_kg_m3), cloud_droplet_radius_m: d!(cloud_droplet_radius_m),
        }
    }
}

/// Physical campaign weather state. The returned values are medium coefficients, cloud geometry
/// and wind exposure—never screen colours or camera-relative brightness adjustments.
/// Terrain season is map data, not an atmosphere/palette interpolation. In particular the
/// winter training arena uses a neutral weather profile with mood0.35; feeding that directly
/// to terrain_material made its snow78.4% spring pasture. Weather fronts do not melt a map.
fn campaign_ground_mood(winter: bool, weather_mood: f32) -> f32 {
    if winter { 0.0 } else { weather_mood.clamp(0.5, 1.0) }
}

fn campaign_atmosphere_profile(act_id: u8) -> CampaignAtmosphereProfile {
    match act_id {
        // Preserve the calibrated Act-I atmosphere exactly; its distinct near-ground whiteout is
        // supplied by the separately bounded blowing-snow medium below.
        1 => CampaignAtmosphereProfile {
            overcast: 0.99,
            storm: 0.98,
            mood: 0.0,
            ground_extinction_m_inv: 0.0068,
            wind_exposure: 1.0,
            mie_scale_height_m: 1_200.0,
            mie_scattering_m_inv: 3.996e-6,
            mie_extinction_m_inv: 4.440e-6,
            mie_asymmetry: 0.80,
            cloud_bottom_m: 900.0,
            cloud_top_m: 2_200.0,
            cloud_lwc_kg_m3: 0.25e-3,
            cloud_droplet_radius_m: 10.0e-6,
        },
        // Spring clearing after rain: broken cloud, blue openings and a light aerial haze.
        2 => CampaignAtmosphereProfile {
            overcast: 0.45,
            storm: 0.12,
            mood: 0.5,
            ground_extinction_m_inv: 0.00075,
            wind_exposure: 0.92,
            mie_scale_height_m: 650.0,
            mie_scattering_m_inv: 2.8e-6,
            mie_extinction_m_inv: 3.4e-6,
            mie_asymmetry: 0.78,
            cloud_bottom_m: 1_300.0,
            cloud_top_m: 2_200.0,
            cloud_lwc_kg_m3: 0.10e-3,
            cloud_droplet_radius_m: 11.0e-6,
        },
        // Clear spring daylight over the forest; local battle smoke comes from actual fires.
        3 => CampaignAtmosphereProfile {
            overcast: 0.10,
            storm: 0.0,
            mood: 1.0,
            ground_extinction_m_inv: 0.00025,
            wind_exposure: 0.16,
            mie_scale_height_m: 1_000.0,
            mie_scattering_m_inv: 0.8e-6,
            mie_extinction_m_inv: 0.95e-6,
            mie_asymmetry: 0.76,
            cloud_bottom_m: 1_900.0,
            cloud_top_m: 2_800.0,
            cloud_lwc_kg_m3: 0.08e-3,
            cloud_droplet_radius_m: 10.5e-6,
        },
        _ => CampaignAtmosphereProfile {
            overcast: 0.20,
            storm: 0.05,
            mood: 0.35,
            ground_extinction_m_inv: 0.00045,
            wind_exposure: 0.72,
            mie_scale_height_m: 1_200.0,
            mie_scattering_m_inv: 3.996e-6,
            mie_extinction_m_inv: 4.440e-6,
            mie_asymmetry: 0.80,
            cloud_bottom_m: 900.0,
            cloud_top_m: 2_200.0,
            cloud_lwc_kg_m3: 0.25e-3,
            cloud_droplet_radius_m: 10.0e-6,
        },
    }
}

// The campaign terrain currently spans about -44..84 m. Ordinary suspended aerosol keeps the
// broader legacy support, while a precipitation-driven whiteout is a near-ground blowing-snow
// layer: it remains deep enough to cover every Act-I ridge, but sunlight from the real environment
// no longer crosses hundreds of metres of dense snow before reaching the battlefield.
use crate::weather_visibility::{world_medium_profile,WorldMediumProfile};
#[cfg(test)]
use crate::weather_visibility::{WORLD_AEROSOL_BOTTOM_M,WORLD_AEROSOL_TOP_M,BLOWING_SNOW_TOP_M};

/// Cone exposure a view holds before its eye has observed a frame: the observer's steady mapping
/// for a 41 cd/m^2 scene (the former fixed calibration's mid-grey, EV100 8.4). The eye replaces
/// it on the first metered frame; frames that are never metered (the menu turntable) keep it.
const DEFAULT_CAMERA_EXPOSURE: f32 = 320.0;
/// Sentinel stored in the per-view eye clock until that view has measured its first complete
/// HDR frame. A real observer is already adapted before the first presented frame; every later
/// change follows the eye's own time course in `observer.rs` / `exposure.wgsl`.
const UNINITIALIZED_EXPOSURE_TIME: f32 = -1.0;
/// Floats in one view's eye state (`exposure.wgsl` documents the lanes).
const EYE_STATE_LANES: usize = 32;

fn uninitialized_exposure_state() -> [f32; EYE_STATE_LANES] {
    let mut state = [0.0; EYE_STATE_LANES];
    state[0] = DEFAULT_CAMERA_EXPOSURE;
    state[1] = UNINITIALIZED_EXPOSURE_TIME;
    state
}

/// Capture-only observer integration at a frozen scene instant. Negative last-observation
/// times are the shader's initialization sentinel, so a scene at t=0 needs a temporary eye
/// clock of one step. The final lane is restored after rendering; scene time never changes.
fn headless_exposure_step_clocks(actual_time: f32) -> [f32; 3] {
    let actual = if actual_time.is_finite() { actual_time.max(0.0) } else { 0.0 };
    let current = actual.max(1.0 / 60.0);
    [current, current - 1.0 / 60.0, actual]
}

fn title_exposure_clock(actual_time: f32, elapsed_s: f32) -> f32 {
    let actual = if actual_time.is_finite() { actual_time.max(0.0) } else { 0.0 };
    let elapsed = if elapsed_s.is_finite() { elapsed_s.clamp(0.0, 1.0) } else { 0.0 };
    actual + elapsed
}

fn title_backdrop_ready(rendered_frames: u32, eye_stable: bool) -> bool {
    rendered_frames >= 64 && eye_stable
}

/// The measured static driver aperture needs 976 steps (16.27 s) to meet the unchanged
/// observer gate. Allow 30 s for this headless title only; live rendering remains one step
/// per callback and the general combat capture budget remains independent.
const TITLE_EXPOSURE_MAX_FRAMES: u32 = 1800;

fn title_capture_frame_budget(diagnostic: Option<&str>) -> u32 {
    diagnostic.and_then(|v| v.parse::<u32>().ok())
        .map(|v| v.clamp(TITLE_EXPOSURE_MAX_FRAMES, 3600))
        .unwrap_or(TITLE_EXPOSURE_MAX_FRAMES)
}

/// Named times of day on an act's date: the menu's Tag/Nacht and the atmosphere gallery. Each is
/// an instant of the ephemeris (`celestial_sky::phase_instant`): noon, the Sun at 3 deg rising or
/// setting, and the act's historical night. The name carries no light of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl DayPhase {
    pub const ALL: [Self; 4] = [Self::Dawn, Self::Day, Self::Dusk, Self::Night];

    pub fn label(self) -> &'static str {
        match self {
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
            Self::Night => "night",
        }
    }

    pub(crate) fn clock_phase(self) -> crate::celestial_sky::ClockPhase {
        use crate::celestial_sky::ClockPhase;
        match self {
            Self::Dawn => ClockPhase::Dawn,
            Self::Day => ClockPhase::Day,
            Self::Dusk => ClockPhase::Dusk,
            Self::Night => ClockPhase::Night,
        }
    }
}

/// Per-blur-pass push constant for bloom.wgsl: source texel size + separable blur axis.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomPush {
    texel: [f32; 2],
    dir: [f32; 2],
}

/// View-local raster aperture halo. Depth classifies visible openings; no meter/history or
/// surface-lighting energy is changed. Keep this separate from the physical bloom ABI.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomPortalUniform {
    depth_and_view: [f32; 4], // near, far, inside physical cabin, reserved
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrushUniform {
    levels: f32,
    dither: f32,
    reticle: f32,
    frame: f32, // animation clock for concussion; display dithering is frame-stable
    /// The eye's rod channel (bytes 16 and 20, copied from the view's eye state every metered
    /// frame): ACES input per scotopic cd/m^2 of the saturating rod signal, added achromatically.
    rod_exposure: f32,
    /// Rod saturation, per scotopic cd/m^2 (pupil area / 1,700 scotopic td).
    rod_inverse_saturation: f32,
    ambient_white_balance: f32,
    menu_mode: f32, // 1 = main-menu turntable pass (keep pure black truly black); 0 = normal game frame
    ldr_dither: f32, // shader-only fixed coverage for the linear 8-bit intermediate; ABI lane 32
    mask: f32, // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    _removed_proxy_rt_mix: f32,
    /// The eye's cone channel: ACES input per unit of radiance (byte 44). The eye pass copies it
    /// from the active view's state every metered frame; the upload only seeds unmetered frames.
    exposure: f32,
    /// Legacy recording treatment. Zero bypasses posterization and its dither/grain.
    /// Shader output may separately dither native LDR rounding via `ldr_dither`.
    retro: f32,
    /// Short-lived visual disorientation from a severe nearby armor hit.
    concussion: f32,
    _grade_pad: [f32; 2],
    display_grade: [f32; 4],
    // Append-only display mapping; physical eye lanes above retain their existing offsets.
    dither_right: [f32; 4], // xyz world right, w horizontal tangent of half-FOV
    dither_up: [f32; 4], // xyz world up, w vertical tangent of half-FOV
    dither_forward: [f32; 4],
}

/// Simulation clock and viewing geometry for the eye. Each camera view owns its eye state; the
/// clock is shared. `fov_y`/`aspect` place the meter's fovea and 20-degree field; `monocular` is
/// 1 while one eye looks through the gunner's sight.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ExposureClockUniform {
    time_s: f32,
    fov_y: f32,
    aspect: f32,
    monocular: f32,
    transient_source: f32,
    _pad: [f32;3],
}

/// The WGSL clock is eight scalar lanes. Keep this assertion beside the host declaration so a
/// future field/layout edit cannot silently recreate a Vulkan bind-size mismatch.
const EXPOSURE_CLOCK_UNIFORM_BYTES: u64 = std::mem::size_of::<ExposureClockUniform>() as u64;
const _: [(); 32] = [(); EXPOSURE_CLOCK_UNIFORM_BYTES as usize];

/// A replay dolly is authored continuous motion even when it travels several metres per frame.
/// Other discontinuities still cut, and the accumulator still checks each reprojected depth/ID.
fn temporal_camera_cut(translation_m: f32, direction_dot: f32, fov_delta: f32,
    aspect_delta: f32, depth_range_changed: bool, continuous_translation: bool) -> bool {
    (!continuous_translation && translation_m > 2.0)
        || direction_dot < 0.5 || fov_delta.abs() > 1e-3
        || aspect_delta.abs() > 1e-4 || depth_range_changed
}

fn replay_frame_is_continuous(previous_elapsed: Option<f32>, elapsed: f32) -> bool {
    previous_elapsed.is_some_and(|previous| elapsed >= previous)
}

fn replay_exposure_time(scene_time: f32, elapsed: f32) -> f32 {
    // Only the observer's adaptation clock advances. The scene clock continues to freeze wind,
    // precipitation, particles, pressure fronts and live FX at their recorded death state.
    scene_time.max(0.0) + elapsed.max(0.0)
}

/// Headless A/B controls. Shipping replay always selects Current. ContinuityOnly holds the
/// exposure clock at its legacy value so a capture can isolate camera history from eye response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplayCapturePolicy { Legacy, ContinuityOnly, Current }

impl ReplayCapturePolicy {
    pub(crate) fn key(self) -> &'static str {
        match self { Self::Legacy => "legacy", Self::ContinuityOnly => "continuity", Self::Current => "current" }
    }
}

fn replay_policy_parameters(previous: Option<f32>, elapsed_s: f32, scene_time: f32,
    policy: ReplayCapturePolicy) -> (f32, bool, bool, f32) {
    let elapsed = if elapsed_s.is_finite() { elapsed_s.max(0.0) } else { 0.0 };
    let continuous = replay_frame_is_continuous(previous, elapsed);
    (elapsed, !continuous, continuous && policy != ReplayCapturePolicy::Legacy,
        if policy == ReplayCapturePolicy::Current { replay_exposure_time(scene_time, elapsed) }
        else { scene_time.max(0.0) })
}

/// Max simultaneous smoke plumes the volumetric pass tracks. One is the norm (the Tiger's long
/// reload means the previous plume has dissipated before the next shot), but a small ring lets rapid
/// scripted fire stack a couple. Slot 0 is the "primary" plume that `set_smoke` drives.
pub const MAX_PUFFS: usize = 12;
/// Compatibility bound for callers still assembling the deleted analytic OBB list.
pub const MAX_RT_BOXES: usize = 12;

/// One analytic smoke plume for smoke.wgsl. Mirrors the WGSL `Puff` layout exactly (pos+age,
/// dir+strength → two 16-byte lanes, no padding). `age` is seconds since the plume was born;
/// `strength` 0 marks the slot inactive.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PuffGpu {
    pos: [f32; 3],
    age: f32,
    dir: [f32; 3],
    strength: f32,
    shape: [f32; 4], // radius (0 legacy), density, noise phase, reserved
}

/// Uniform for the volumetric smoke pass. Kept SEPARATE from the shared `CameraUniform` on purpose:
/// mesh.wgsl / shadow.wgsl bind that one and their struct layouts must not move, so smoke carries its
/// own inverse-view-proj (for ray reconstruction + depth-based occlusion) and the puff ring here.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SmokeUniform {
    inv_view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 3],
    fog_density: f32,
    sun_dir: [f32; 3],
    frame: f32,
    puffs: [PuffGpu; MAX_PUFFS],
    // Dedicated small interior bore-gas volume. It shares the ray marcher, but not the exterior
    // muzzle plume's metre-scale shape or the cubic fire/soot instance pages.
    breech_gas: PuffGpu,
    // Shared point lights (same set mesh.wgsl gets) so the plume in-scatters muzzle/fire light.
    n_lights: f32,
    night: f32,
    overcast: f32,
    world_max_distance: f32,
    lights: [[f32; 4]; N_LIGHTS * 2],
    interior: f32, // 0..1 master gate: base haze density scale
    dust: f32,     // 0..1 dust-mote sparkle amount inside the volume
    // Effective optical mixture for the bounded world medium: 0 generic aerosol, 1 blowing snow.
    // This consumes an existing ABI lane, preserving the source and medium prefix.
    world_snow_fraction: f32,
    storm: f32,
    // Interior anchor (tank hull origin): dust is sampled in this frame so motes ride the hull,
    // not the world, with only a slow sealed-compartment drift relative to the crew.
    atmo_origin: [f32; 3],
    atmo_time: f32,
    // Explicit modeled vertical support for the homogeneous world aerosol, plus one spare lane.
    fog_layer: [f32; 4], // bottom_y, top_y, unused, unused
    // Runtime numerical quadrature: world, cabin, plume, secondary local-medium steps.
    transport_steps: [f32; 4],
    weather_motion: [f32;4],
    cabin_medium: cabin_medium::Uniform,
}

const SMOKE_SHADER_SOURCE: &str = include_str!("../shaders/smoke.wgsl");
const PHYSICAL_ENVIRONMENT_SHADER_SOURCE: &str =
    include_str!("../shaders/physical_environment.wgsl");
const PHYSICAL_SNOW_LIBRARY_MARKER: &str =
    "// --- Persistent physical snow ---------------------------------------------------------------";
#[path = "cabin_medium.rs"]
pub(crate) mod cabin_medium;
#[path = "cabin_shadows.rs"]
mod cabin_shadows;
#[path = "sky_visibility.rs"]
mod sky_visibility;
#[path = "environment_probe.rs"]
mod environment_probe;
#[path = "hover_outline.rs"] mod hover_outline;
#[path = "raster_glass.rs"] mod raster_glass;
#[path = "raster_optics.rs"] mod raster_optics;
#[path = "raster_ssao.rs"] mod raster_ssao;
#[path = "renderer_aa.rs"] mod renderer_aa;
pub use raster_optics::RasterOpticSpec;
#[path = "daylight_sampling.rs"]
mod daylight_sampling;
#[path = "protective_glass.rs"]
mod protective_glass;

const SMOKE_RT_TLAS_MARKER: &str = "// @smoke-ray-query-tlas";
const SMOKE_NO_DIRECT_MARKER: &str = "// @smoke-non-rt-no-direct-transport\n    return 0.0;";
const SMOKE_RT_TLAS_DECLARATION: &str = r#"@group(1) @binding(0) var smoke_scene_tlas: acceleration_structure;
@group(1) @binding(1) var<storage, read> rt_vertices_a: array<u32>;
@group(1) @binding(2) var<storage, read> rt_vertices_b: array<u32>;
@group(1) @binding(3) var<storage, read> rt_records: array<u32>;
struct RtInstanceShading { ids: vec4<u32>, emission: vec4<f32>, };
@group(1) @binding(5) var<storage, read> rt_instances: array<RtInstanceShading>;
// @protective-glass-source-transport"#;
const SMOKE_NO_DAYLIGHT_MARKER: &str = "// @smoke-non-rt-no-daylight-transport";
const SMOKE_NO_DAYLIGHT_CACHE_MARKER: &str =
    "// @smoke-non-rt-no-daylight-cache\n    return false;";
const SMOKE_RT_DAYLIGHT_CACHE_BODY: &str = r#"let radius = daylight_proposals.counts.z - 0.01;
    return daylight_proposals.counts.x >= 0.5 && daylight_proposals.counts.y > 0.0
        && radius > 0.0
        && distance(start, daylight_proposals.center.xyz) < radius
        && distance(end, daylight_proposals.center.xyz) < radius;"#;
const SMOKE_RT_DAYLIGHT_BODY: &str = r#"let path = source_connection(origin, direction, 0.01, max_distance);
    out.throughput = path.throughput;
    out.origin = path.origin;
    out.air_before_glass = path.air_before_glass;
    out.glass_path_m = path.glass_path_m;"#;
const SMOKE_RT_VISIBILITY_BODY: &str = r#"// Hardware High: trace the actual triangle scene. Stop
    // short of a finite fixture so its own emitter housing cannot become a false blocker.
    let ray_min = 0.01;
    if (max_distance <= ray_min * 2.0) { return 1.0; }
    let ray_max = max(max_distance - ray_min, ray_min);
    var rq: ray_query;
    rayQueryInitialize(&rq, smoke_scene_tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xFFu, ray_min, ray_max, origin, normalize(direction)));
    while (rayQueryProceed(&rq)) { }
    return select(1.0, 0.0,
        rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE);"#;

/// Build the ray-query shader from the portable medium integrator without maintaining two copies
/// of several hundred lines of density/scattering code. The portable source intentionally returns
/// zero direct visibility; capable hardware receives this one narrowly substituted TLAS function.
fn smoke_ray_query_source() -> String {
    assert_eq!(SMOKE_SHADER_SOURCE.matches(SMOKE_RT_TLAS_MARKER).count(), 1);
    assert_eq!(SMOKE_SHADER_SOURCE.matches(SMOKE_NO_DAYLIGHT_CACHE_MARKER).count(), 1);
    assert_eq!(
        SMOKE_SHADER_SOURCE.matches(SMOKE_NO_DIRECT_MARKER).count(),
        1
    );
    let live_smoke = SMOKE_SHADER_SOURCE
        .replacen(SMOKE_RT_TLAS_MARKER, SMOKE_RT_TLAS_DECLARATION, 1)
        .replacen(SMOKE_NO_DIRECT_MARKER, SMOKE_RT_VISIBILITY_BODY, 1)
        .replacen(SMOKE_NO_DAYLIGHT_MARKER, SMOKE_RT_DAYLIGHT_BODY, 1)
        .replacen(SMOKE_NO_DAYLIGHT_CACHE_MARKER, SMOKE_RT_DAYLIGHT_CACHE_BODY, 1);
    physical_atmosphere_transport_shader_source(&live_smoke)
}

const SNOW_RT_TLAS_MARKER: &str = "// @physical-snow-ray-query-tlas";
const SNOW_NO_GEOMETRY_VISIBILITY_MARKER: &str =
    "// @physical-snow-no-geometry-visibility\n    return 1.0;";
const SNOW_RT_VISIBILITY_BODY: &str = r#"// High/Vulkan: a crystal is illuminated or admitted only
    // when its ray reaches the source/upstream field through the exact triangle scene.
    let ray_min = 0.002;
    if (max_distance <= ray_min * 2.0) { return 1.0; }
    var rq: ray_query;
    rayQueryInitialize(&rq, physical_snow_scene_tlas, RayDesc(
        RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE,
        0xFFu, ray_min, max(max_distance - ray_min, ray_min), origin, normalize(direction)));
    while (rayQueryProceed(&rq)) { }
    return select(1.0, 0.0,
        rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE);"#;

/// Compose the persistent snow transport with High's exact scene visibility. The portable shader
/// still compiles from the same laws, but its resolved-grain pass is not submitted until the
/// software triangle visibility backend exists; the shared bulk suspension remains live there.
fn physical_snow_ray_query_source() -> String {
    let source = physical_transport_shader_source(include_str!("../shaders/snow.wgsl"));
    assert_eq!(source.matches(SNOW_RT_TLAS_MARKER).count(), 1);
    assert_eq!(
        source.matches(SNOW_NO_GEOMETRY_VISIBILITY_MARKER).count(),
        1
    );
    source
        .replacen(
            SNOW_RT_TLAS_MARKER,
            "@group(1) @binding(0) var physical_snow_scene_tlas: acceleration_structure;",
            1,
        )
        .replacen(
            SNOW_NO_GEOMETRY_VISIBILITY_MARKER,
            SNOW_RT_VISIBILITY_BODY,
            1,
        )
}

/// WGSL has no include mechanism. Compose the single binding-free physical transport library at
/// renderer construction instead of copying atmosphere functions into sky, surface, and volume
/// shaders where their laws could drift apart.
fn physical_transport_shader_source(entry_source: &str) -> String {
    let mut source =
        String::with_capacity(PHYSICAL_ENVIRONMENT_SHADER_SOURCE.len() + 1 + entry_source.len());
    source.push_str(PHYSICAL_ENVIRONMENT_SHADER_SOURCE);
    source.push('\n');
    source.push_str(entry_source);
    source
}

/// Compose an atmosphere/solar/cloud entry without appending the independent persistent resolved-
/// snow implementation. The full library remains the single source of truth; this is a literal
/// prefix split at its section boundary, not a reduced transport approximation. It keeps Vulkan
/// from parsing and optimizing the 384-line crystal traversal for every sky/surface/volume entry.
/// Install the observer's generated constants (`observer::wgsl_constants`) at the shader's
/// `// @observer-constants` marker: the eye pass, the display crush and the one-pixel particles
/// all read the same values the CPU reference is tested with.
pub(crate) fn observer_shader_source(source: &str) -> String {
    assert!(source.contains("// @observer-constants"), "observer constants marker missing");
    source.replacen("// @observer-constants", &crate::observer::wgsl_constants(), 1)
}

fn physical_atmosphere_transport_shader_source(entry_source: &str) -> String {
    let (atmosphere_source, _) = PHYSICAL_ENVIRONMENT_SHADER_SOURCE
        .split_once(PHYSICAL_SNOW_LIBRARY_MARKER)
        .expect("physical environment snow section marker must remain unique");
    let entry_source =
        protective_glass::inject(entry_source, entry_source.contains("struct Smoke {"));
    let entry_source = terrain_material_shader_source(&entry_source);
    let mut source = String::with_capacity(atmosphere_source.len() + 1 + entry_source.len());
    source.push_str(atmosphere_source);
    source.push('\n');
    source.push_str(&entry_source);
    source
}

/// Per-draw push constant: model matrix + params (x = emissive intensity, 0 = lit PBR surface;
/// y = metallic factor; z = roughness factor; w = render mode, including -1 for sparse cubic
/// smoke). 80 bytes total, well inside the 128-byte PUSH_CONSTANTS limit (see main.rs) — the
/// metallic/roughness scalars reuse params' previously-unused y/z lanes, so the layout didn't need
/// to grow for this.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PushData {
    model: [[f32; 4]; 4],
    params: [f32; 4],
}

/// S3 temporal-accumulation uniform (accum.wgsl).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AccumUniform {
    prev_view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    alpha: f32,
    reset: f32,
    test_noise: f32,
    frame: f32,
    znear: f32,
    zfar: f32,
    n_meshes: f32,
    /// Separate validity bit for the volume history. The surface history can remain continuous
    /// while a view has no participating medium; when haze/fog later returns it must not inherit a
    /// stale volume frame.
    volume_reset: f32,
    sky_reprojection: [[f32; 4]; 4],
    surface_history: [f32; 4], // x: identical camera; y: significant source step for this view
    camera_shift: [f32; 4], // current eye origin minus previous eye origin
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PixelParticleUniform {
    view_proj: [[f32;4];4], camera: [f32;4], viewport: [f32;4],
    snow_velocity: [f32;4], sun_direction: [f32;4], solar_irradiance: [f32;4],
    sky_directions: [[f32;4];6], sky_radiances: [[f32;4];6],
    lights: [[f32;4];64], world_medium: [f32;4], cabin_origin: [f32;4],
    cabin_medium: cabin_medium::Uniform,
    medium_transport:[f32;4],
}

#[path = "sky_reprojection.rs"]
mod sky_reprojection;

/// Per-draw push constants for the S2 G-buffer pipelines: PushData + the object id (G3 / TLAS
/// custom_data agreement — temporal reject and the instance table key off it).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GbufPush {
    model: [[f32; 4]; 4],
    params: [f32; 4],
    extra: [f32; 4], // x = object id base (f32-exact to 2^24)
}

const RT_SURFACE_META_MAGIC: u32 = 0x4b52_5453; // "KRTS"
const RT_SURFACE_META_VERSION: u32 = 2;
const RT_SURFACE_HEADER_U32: usize = 32;

struct RtSurfaceGpu {
    vertices_a_buf: wgpu::Buffer,
    vertices_b_buf: wgpu::Buffer,
    records_buf: wgpu::Buffer,
    atlas_buf: wgpu::Buffer,
    instances_buf: wgpu::Buffer,
    instance_count: usize,
    instance_capacity: usize,
    lookup_offset_words: u32,
    daylight_proposals_buf: wgpu::Buffer,
}

struct RtSurfaceStaticUpload {
    vertices_a: Vec<u32>,
    vertices_b: Vec<u32>,
    records: Vec<u32>,
    atlas: Vec<u32>,
}

fn append_rt_records<T: bytemuck::Pod>(dst: &mut Vec<u32>, records: &[T]) -> u32 {
    let offset = u32::try_from(dst.len()).expect("RT surface metadata exceeds u32 addressing");
    let words: &[u32] = bytemuck::cast_slice(records);
    dst.extend_from_slice(words);
    offset
}

fn rt_vertex_partition_counts(vertex_count: usize) -> (usize, usize) {
    let first = vertex_count.div_ceil(2);
    (first, vertex_count - first)
}

fn pack_rt_surface_static(pack: &crate::rt_surface::SurfacePack) -> RtSurfaceStaticUpload {
    // Vertex records are the only semantic array that can individually exceed Vulkan's signed
    // 31-bit storage-binding range in a source-exact campaign pack. Split at a whole 64-byte
    // record near the midpoint; global triangle indices stay unchanged and the shader selects the
    // owning half using records[3]. Nothing is decimated, averaged, or reordered.
    let (vertex_split, vertex_tail) = rt_vertex_partition_counts(pack.vertices().len());
    debug_assert_eq!(vertex_split + vertex_tail, pack.vertices().len());
    let mut vertices_a: Vec<u32> = bytemuck::cast_slice(&pack.vertices()[..vertex_split]).to_vec();
    let mut vertices_b: Vec<u32> = bytemuck::cast_slice(&pack.vertices()[vertex_split..]).to_vec();
    if vertices_a.is_empty() {
        vertices_a.push(0);
    }
    if vertices_b.is_empty() {
        vertices_b.push(0);
    }

    let mut records = vec![0u32; RT_SURFACE_HEADER_U32];
    records[0] = RT_SURFACE_META_MAGIC;
    records[1] = RT_SURFACE_META_VERSION;
    records[2] =
        u32::try_from(pack.vertices().len()).expect("RT vertex count exceeds u32 addressing");
    records[3] = u32::try_from(vertex_split).expect("RT vertex split exceeds u32 addressing");

    macro_rules! append {
        ($offset:expr, $count:expr, $records:expr) => {{
            let source_records = $records;
            records[$offset] = append_rt_records(&mut records, source_records);
            records[$count] =
                u32::try_from(source_records.len()).expect("RT surface record count exceeds u32");
        }};
    }
    append!(4, 5, pack.triangles());
    append!(6, 7, pack.surfaces());
    append!(8, 9, pack.materials());
    append!(10, 11, pack.textures());
    append!(12, 13, pack.mip_rects());
    let pages: Vec<_> = pack.atlas_pages().iter().map(|page| page.gpu).collect();
    append!(14, 15, pages.as_slice());
    append!(16, 17, pack.instance_lookup());
    records[18] = u32::try_from(pack.instances().len()).expect("RT instance count exceeds u32");
    // Runtime-visible invariant shared by surface, volume and particle optical connections.
    // This formerly reserved header word prevents static unrolling without changing path depth.
    records[19] = 12;

    let mut atlas = pack.flattened_atlas_texels();
    if atlas.is_empty() {
        atlas.push(0);
    }
    RtSurfaceStaticUpload {
        vertices_a,
        vertices_b,
        records,
        atlas,
    }
}

fn validate_rt_storage_binding_sizes(
    limit: u64,
    bindings: &[(&'static str, usize)],
) -> Result<(), String> {
    for &(label, bytes) in bindings {
        let bytes =
            u64::try_from(bytes).map_err(|_| format!("{label} byte count does not fit u64"))?;
        if bytes > limit {
            return Err(format!(
                "{label} requires {bytes} bytes, exceeding this adapter's {limit}-byte storage-buffer binding limit; exact transport cannot drop or simplify this data"
            ));
        }
    }
    Ok(())
}

/// Explicit diagnostic lease of existing source buffers. Cloning these handles shares the
/// allocation; it does not copy vertices, upload a second tank, or change gameplay visibility.
pub struct DiagnosticMeshLease {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
    pub material: Arc<wgpu::BindGroup>,
    pub model: glam::Mat4,
    pub visible: bool,
}

#[derive(Clone)]
struct MeshGpu {
    visibility_geometry: Option<crate::scene_visibility::GeometryId>,
    visibility_source: Option<CpuMesh>,
    visibility_deformable: bool,
    pose_buffer_private: bool,
    /// Clustered occluder for the environment-visibility atlas's dynamic layer (vertex buffer,
    /// index buffer, index count); see `sky_visibility::clustered_occluder` for its error bound.
    visibility_occluder: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    /// Left out of the gunner's exterior optic raster only. It stays a shadow and environment
    /// caster in every pass, so hiding it does not invalidate any shadow or visibility cache.
    optic_hidden: bool,
    pose_revision: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    /// Separate exact index ordering for spatially culled terrain shadows; never primary/RT.
    shadow_pages: Option<(wgpu::Buffer, Arc<terrain_shadow_pages::Pages>)>,
    rigid_pages: Option<(wgpu::Buffer, Arc<rigid_triangle_pages::Pages>)>,
    tex_bind: Arc<wgpu::BindGroup>,
    model: glam::Mat4,
    visible: bool,
    emissive: f32,
    /// When true this mesh is a NIGHT window / streetlamp: the mesh pass takes the emissive path but
    /// keeps the mesh's OWN authored colour (warm-yellow / cold-blue / teal) instead of the FX orange,
    /// scaled by the `night` uniform — so it glows at Nacht and is dark at Tag. Signalled to the
    /// shader via params.w (0 = FX gunfire orange, 1 = window/lamp own-colour glow).
    emissive_win: bool,
    /// When true this mesh takes the OWN-COLOUR emissive path but is NOT night-gated (unlike a
    /// window): a warm interior bulb / a kicked-up snow puff glows with its authored colour scaled by
    /// `emissive`, day or night. Signalled to the shader via params.w = 2.0.
    emissive_own: bool,
    /// The player's vehicle: takes `player_snow_load` instead of the world's (raymarch backend).
    snow_vehicle: bool,
    /// A clear cover over an instrument dial (`mark_instrument_cover`).
    instrument_cover: bool,
    /// The source model's default metal on this mesh is painted steel (`mark_painted_steel`).
    painted_steel: bool,
    /// Left out of the sky-visibility atlas (`exclude_from_relay_visibility`).
    sky_visibility_excluded: bool,
    /// A small-arms muzzle flash (`mark_small_arms_flash`).
    small_arms_flash: bool,
    /// A legacy small-arms effect mesh the raymarch backend leaves out (`mark_legacy_small_arms_fx`).
    legacy_small_arms_fx: bool,
    /// View-local geometry that is drawn through the ordinary raster material path but never enters
    /// the shared ray scene. Split-screen crew use this because each viewport hides a different body
    /// while views 2..N intentionally reuse view 1's TLAS.
    raster_only: bool,
    /// Animated actor geometry: retain exact physical occlusion, but reject source/eye history.
    temporal_dynamic: bool,
    /// Closed direct-view protective glass. High writes its front surface to the G-buffer, gives
    /// the exact triangles a dedicated TLAS mask, then transports the camera ray through the
    /// measured entry/exit geometry. The portable forward path intentionally remains opaque until
    /// it has an equivalent non-screen-space transport implementation.
    glass: bool,
    periscope_kind: u32, // 0 ordinary, 1 closed window, 2 mirror, 3 finite ideal thin lens
    periscope_focal_m: f32,
    /// A mirror's plane normal in the mesh's own frame (raymarch ray queries fold at it).
    mirror_normal: glam::Vec3,
    /// When true this mesh is TERRAIN: the fragment stage samples the tank-track map (rut darkening
    /// + a normal-perturbed dip). Signalled via params.w = 3.0.
    track_recv: bool,
    metallic_factor: f32,
    roughness_factor: f32,
    /// Local-space bounds (from the CpuMesh) + the world-space AABB used for frustum culling.
    /// Static scene meshes are baked in world space with identity models, so world == local for
    /// them; dynamic meshes (tank parts, FX) refresh `world_*` on every `set_model`.
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    /// M-D: this mesh's bottom-level acceleration structure (None on non-RT devices, for the
    /// separate menu list, and for degenerate meshes). Inclusion in the TLAS is decided per
    /// frame from visible/emissive state — the BLAS itself is immutable (rigid world).
    blas: Option<crate::rt::BlasSet>,
    /// Exact source triangle/material payload retained only on hardware-RT devices. The one-bounce
    /// integrator packs this without simplification so committed BLAS primitive indices address
    /// the same vertices and textures the raster draw uses.
    rt_source: Option<CpuMesh>,
}

impl MeshGpu {
    fn rigid_index_buffer(&self) -> &wgpu::Buffer {
        if !self.temporal_dynamic && !self.visibility_deformable {
            if let Some((buffer, _)) = &self.rigid_pages { return buffer; }
        }
        &self.ibuf
    }
    fn primary_index_ranges(&self, matrix: glam::Mat4) -> Vec<std::ops::Range<u32>> {
        if !self.temporal_dynamic && !self.visibility_deformable {
            if let Some((_, pages)) = &self.rigid_pages { return pages.primary_ranges(matrix * self.model); }
        }
        vec![0..self.index_count]
    }
    fn shadow_index_buffer(&self) -> &wgpu::Buffer {
        if !self.temporal_dynamic && !self.visibility_deformable {
            if let Some((buffer, _)) = &self.rigid_pages { return buffer; }
        }
        if self.track_recv {
            if let Some((buffer, _)) = &self.shadow_pages { return buffer; }
        }
        &self.ibuf
    }

    fn shadow_index_ranges(&self, matrix: glam::Mat4, size: u32) -> Vec<std::ops::Range<u32>> {
        if !self.temporal_dynamic && !self.visibility_deformable {
            if let Some((_, pages)) = &self.rigid_pages { return pages.ranges(matrix * self.model, size); }
        }
        if self.track_recv {
            if let Some((_, pages)) = &self.shadow_pages { return pages.ranges(matrix * self.model, size); }
        }
        vec![0..self.index_count]
    }
}

/// Immutable GPU geometry/material shared by every spatial page of one instanced asset. Keeping the
/// BLAS here is important: a 10,000-page campaign must build one BLAS per GLB, not one per page.
struct InstPrototype {
    visibility_geometry: Option<crate::scene_visibility::GeometryId>,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    tex_bind: Arc<wgpu::BindGroup>,
    metallic_factor: f32,
    roughness_factor: f32,
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    blas: Option<crate::rt::BlasSet>,
    rt_source: Option<CpuMesh>,
}

/// Opaque-by-convention handle returned by `register_instanced_prototype` and accepted when adding
/// pages. Kept as an index so existing renderer batch handles remain simple `usize` values.
pub type InstancedPrototype = usize;

/// Compatibility mode tag for physical cubic smoke grains. Every submitted cube is shaded as real
/// geometry; the former screen-door fragment deletion is gone.
pub const BATCH_FLAG_SPARSE_CUBIC_SMOKE: f32 = -1.0;

/// One spatially bounded INSTANCED page: a prototype drawn at N nearby placements with one call.
/// Only the instance buffer, transforms and page AABB vary. Most pages are static world geometry;
/// bounded raster-only FX pools stay out of RT, while fixed moving geometry pools retain reserved
/// object IDs but occupy physical TLAS slots only for live instances. Existing dynamic world
/// callers retain stable batch indices when transforms are rewritten; campaign pages simply never
/// mutate after load.
struct InstBatch {
    prototype: usize,
    /// Shared packed instance storage. Initial campaign/editor loads put every page of a prototype
    /// in one buffer; a dynamic page only peels off into a replacement buffer if it outgrows its
    /// reserved range.
    inst_buf: Arc<wgpu::Buffer>,
    inst_buffer_id: usize,
    inst_byte_offset: u64,
    inst_capacity: u32,
    inst_count: u32,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    /// Shader flag (`params.w`) for the whole batch: -1 = cubic smoke material, 0 = plain module,
    /// 4 = grass (wind + alpha-test).
    flags: f32,
    /// Per-batch emissive intensity (params.x). Ordinary instanced geometry remains non-emissive.
    emissive: f32,
    /// Tiny short-lived FX (blood grains) still use the lit instanced raster path, but neither cast
    /// sun/RT shadows nor enter the scene TLAS. This keeps a high-frequency transform stream from
    /// forcing every static campaign instance to be rewritten into the TLAS each frame.
    raster_only_dynamic: bool,
    /// Fixed-capacity moving geometry that remains in the exact ray scene. Infantry pose batches
    /// use this path: one prototype is shared by every live copy of a pose and matrices update
    /// through the renderer-owned queue without reallocating. Inactive capacity reserves IDs and
    /// exact-hit records only; physical TLAS slots compact whenever live counts change.
    moving_rt_pool: bool,
    /// CPU copy of the instance transforms (GPU instance buffer isn't readable) for TLAS writes.
    cpu_mats: Vec<glam::Mat4>,
    /// Unique 22-bit range start shared by G-buffer IDs and TLAS custom_data. This replaces the old
    /// 10-bit batch / 12-bit instance split, which collided as soon as spatial paging exceeded
    /// 1,024 pages. Bases are reassigned deterministically whenever a dynamic batch changes size.
    object_id_base: u32,
    /// Reserved IDs for this page. Static pages track their live count; bounded raster-only and
    /// ray-visible moving pools retain full capacity so independently updated batches never overlap.
    object_id_capacity: u32,
    /// Per-instance visibility of a static page whose instances appear and vanish one by one
    /// (compiled building modules fail, their rubble appears). A hidden instance keeps its exact
    /// transform in `cpu_mats` (source record, object ID) but is written as a zero matrix into the
    /// raster instance stream, so every raster pass collapses it to a degenerate point that
    /// produces no fragment, and it occupies no TLAS slot.
    hidden: InstanceVisibility,
}

/// Hidden bits of one page's instances: one bit per instance ordinal, set = hidden, trailing zero
/// words trimmed (an empty set means every instance is visible and hashes like none ever hid).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct InstanceVisibility {
    words: Vec<u64>,
}

impl InstanceVisibility {
    fn is_hidden(&self, instance: u32) -> bool {
        self.words
            .get(instance as usize / 64)
            .is_some_and(|word| word & (1u64 << (instance % 64)) != 0)
    }

    fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Hidden instances among ordinals `0..count`.
    fn hidden_below(&self, count: u32) -> u32 {
        let full = count as usize / 64;
        let mut hidden: u32 = self.words.iter().take(full).map(|word| word.count_ones()).sum();
        if let Some(word) = self.words.get(full) {
            let used = count % 64;
            if used != 0 {
                hidden += (word & ((1u64 << used) - 1)).count_ones();
            }
        }
        hidden
    }

    /// Set or clear one ordinal's bit; true when it changed.
    fn set(&mut self, instance: u32, hidden: bool) -> bool {
        if self.is_hidden(instance) == hidden {
            return false;
        }
        let word = instance as usize / 64;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
        self.words[word] ^= 1u64 << (instance % 64);
        while self.words.last() == Some(&0) {
            self.words.pop();
        }
        true
    }

    /// Drop every bit at or beyond `count` (a page shrank).
    fn trim(&mut self, count: u32) {
        self.words.truncate((count as usize).div_ceil(64));
        if let Some(last) = self.words.last_mut() {
            let used = count % 64;
            if used != 0 {
                *last &= (1u64 << used) - 1;
            }
        }
        while self.words.last() == Some(&0) {
            self.words.pop();
        }
    }
}

const INSTANCE_MAT_BYTES: u64 = std::mem::size_of::<[[f32; 4]; 4]>() as u64;

/// One instance of an instanced page: the handle compiled buildings keep per module and rubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceRef {
    pub batch: usize,
    pub instance: u32,
}

/// A renderer object whose visibility the game switches: a loose scene mesh slot or one instance
/// of a static instanced page, or nothing (a manifest wall that leaves no heap of its own: its
/// debris is thrown when it falls).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneRef {
    Mesh(usize),
    Instance(InstanceRef),
    None,
}

/// The renderer evaluates at most `budget` point lights per frame. When more are offered it keeps
/// those with the largest irradiance scale at the eye, `I / max(d², r²)` for radiant intensity `I`,
/// distance `d` and source radius `r` (the source's own peak once the eye is inside it), in offer
/// order on a tie. Before 2026-09-28 it kept the largest peak radiance `I / r²` wherever the source
/// was, so a district of identical far fixtures could displace every light near the camera.
pub fn select_point_lights(lights: &[PointLight], eye: glam::Vec3, budget: usize) -> Vec<PointLight> {
    if lights.len() <= budget {
        return lights.to_vec();
    }
    let score = |light: &PointLight| {
        let reach = light.source_radius.max(0.01).powi(2);
        light.radiant_intensity / light.pos.distance_squared(eye).max(reach)
    };
    let mut order: Vec<usize> = (0..lights.len()).collect();
    order.sort_by(|&a, &b| {
        score(&lights[b])
            .partial_cmp(&score(&lights[a]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    order.into_iter().take(budget).map(|index| lights[index]).collect()
}

/// Hardware TLAS capacity the device is requested with (`request_device` in main.rs). 131,072
/// since the urban district's density pass (2026-09-29: 62,615 of the former 65,536 at build);
/// Vulkan ray-tracing devices report `maxInstanceCount` of 2^24, and `request_device` refuses an
/// adapter that offers less than this. The TLAS itself is allocated to the demand, not to this cap.
pub const TLAS_MAX_INSTANCES: u32 = 1 << 17;

/// What the ray scene needs from the TLAS right now: one slot per scene mesh (visible or not, the
/// layout keeps mesh indices) plus one per live, visible instance of every ray-visible page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlasDemand {
    pub mesh_slots: u32,
    pub instanced_live: u32,
    /// Instance IDs reserved by ray-visible pages and pools (hidden and empty capacity included).
    /// This is the source-record count, not TLAS occupancy.
    pub instanced_reserved: u32,
    pub capacity: u32,
}

impl TlasDemand {
    pub fn needed(&self) -> u32 {
        self.mesh_slots.saturating_add(self.instanced_live)
    }

    /// Instances that would not enter the ray scene: the TLAS overflow, never dropped silently.
    pub fn overflow(&self) -> u32 {
        self.needed().saturating_sub(self.capacity)
    }

    pub fn describe(&self) -> String {
        format!(
            "{} of {} TLAS instances ({} scene-mesh slots + {} live instances; {} reserved instance IDs)",
            self.needed(),
            self.capacity,
            self.mesh_slots,
            self.instanced_live,
            self.instanced_reserved
        )
    }
}

/// The explicit TLAS overflow report: an overflow is an error the build refuses and a frame logs,
/// never a silent drop of geometry from the ray scene.
pub fn tlas_overflow_message(needed: u32, capacity: u32) -> Option<String> {
    (needed > capacity).then(|| {
        format!(
            "the ray scene needs {needed} TLAS instances but the device TLAS holds {capacity}: \
             {} instances at the tail of the layout are NOT in the ray scene (no shadow, \
             reflection, GI or occlusion from them)",
            needed - capacity
        )
    })
}
/// The manifest/compiler's deterministic horizontal page size. The renderer indexes exact page
/// AABBs onto the same grid, so camera and shadow queries inspect cells before touching pages.
const INST_SPATIAL_CELL_METRES: f32 = 128.0;
/// A normal compiled page overlaps at most a handful of neighbouring cells after mesh bounds are
/// applied. Legacy batches can span a whole district; duplicating those into thousands of cells
/// would defeat the compact index, so unusually broad batches stay in a tiny fallback list.
const MAX_INST_INDEX_CELLS_PER_BATCH: u64 = 64;

impl InstBatch {
    fn active_instance_bytes(&self) -> std::ops::Range<u64> {
        self.inst_byte_offset
            ..self.inst_byte_offset + u64::from(self.inst_count) * INSTANCE_MAT_BYTES
    }

    fn temporal_dynamic(&self) -> bool {
        self.raster_only_dynamic || self.moving_rt_pool
    }

    fn object_custom_data(&self, instance: u32) -> u32 {
        debug_assert!(instance < self.object_id_capacity);
        instanced_object_custom_data(self.object_id_base, instance, self.temporal_dynamic())
    }

    fn is_hidden(&self, instance: u32) -> bool {
        self.hidden.is_hidden(instance)
    }

    /// Instances that enter the TLAS: every live instance that is not hidden.
    fn rt_live_count(&self) -> u32 {
        self.inst_count - self.hidden.hidden_below(self.inst_count)
    }

    /// Set or clear one instance's hidden bit; returns true when it changed.
    fn set_hidden(&mut self, instance: u32, hidden: bool) -> bool {
        self.hidden.set(instance, hidden)
    }

    /// The raster stream entry of an instance: its transform, or the zero matrix while hidden.
    fn raster_matrix(&self, instance: u32, model: glam::Mat4) -> [[f32; 4]; 4] {
        raster_instance_matrix(self.is_hidden(instance), model)
    }

    /// Drop hidden bits at or beyond `count` (the page shrank).
    fn trim_hidden(&mut self, count: u32) {
        self.hidden.trim(count);
    }
}

/// A page's raster stream entry: the instance transform, or the zero matrix while hidden. Every
/// vertex of a zero-matrix instance lands on one point, so its triangles have no area and no pass
/// rasterizes a fragment of it.
fn raster_instance_matrix(hidden: bool, model: glam::Mat4) -> [[f32; 4]; 4] {
    if hidden {
        [[0.0; 4]; 4]
    } else {
        model.to_cols_array_2d()
    }
}

// Bit 22 distinguishes instanced identities; bit 23 excludes moving geometry from histories.
// Regular actor slots retain their exact ordinal and must use this same key at every draw/RT site.
fn regular_object_custom_data(slot: usize, temporal_dynamic: bool) -> u32 {
    assert!(slot < 0x40_0000, "regular source identity exceeds its 22-bit range");
    slot as u32 | if temporal_dynamic { 0x80_0000 } else { 0 }
}

fn instanced_object_custom_data(base: u32, instance: u32, temporal_dynamic: bool) -> u32 {
    debug_assert!(base + instance < 0x40_0000);
    let kind = if temporal_dynamic {
        0xC0_0000
    } else {
        0x40_0000
    };
    kind | (base + instance)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct InstSpatialCell {
    x: i32,
    z: i32,
}

#[derive(Clone, Copy, Debug)]
struct InstSpatialBounds {
    min: glam::Vec3,
    max: glam::Vec3,
}

impl InstSpatialBounds {
    #[cfg(test)]
    fn new(min: glam::Vec3, max: glam::Vec3) -> Option<Self> {
        let bounds = Self { min, max };
        bounds.is_valid().then_some(bounds)
    }

    fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x <= self.max.x
            && self.min.y <= self.max.y
            && self.min.z <= self.max.z
    }
}

#[derive(Debug)]
struct InstSpatialRecord {
    bounds: InstSpatialBounds,
    /// Vertical bounds at the last membership/refit operation. XZ motion stays inside the cell
    /// footprint; a two-metre vertical change refits cells so their conservative history stays
    /// bounded even for a particle stream that moves upward indefinitely.
    indexed_y: glam::Vec2,
    /// Indexed batches occupy one compact inclusive cell rectangle; `None` is a broad/invalid
    /// legacy fallback. Avoids one heap allocation per campaign page.
    cell_rect: Option<InstSpatialCellRect>,
}

#[derive(Debug)]
struct InstSpatialCellEntry {
    batches: Vec<usize>,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
}

#[derive(Debug, Default)]
struct InstSpatialIndex {
    cells: HashMap<InstSpatialCell, InstSpatialCellEntry>,
    records: Vec<Option<InstSpatialRecord>>,
    fallback: Vec<usize>,
}

#[derive(Debug, Default)]
struct InstSpatialQuery {
    batches: Vec<usize>,
    /// Hash lookups (dense rectangle mode) or occupied entries visited (sparse mode).
    index_visits: usize,
    /// Occupied 128 m cells that reached an exact cell/frustum test.
    cell_tests: usize,
    /// Unique pages tested after visible-cell collection, including broad legacy fallbacks.
    page_tests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InstSpatialCellRect {
    min: InstSpatialCell,
    max: InstSpatialCell,
}

impl InstSpatialCellRect {
    fn area(self) -> Option<u64> {
        let width = i64::from(self.max.x)
            .checked_sub(i64::from(self.min.x))?
            .checked_add(1)?;
        let depth = i64::from(self.max.z)
            .checked_sub(i64::from(self.min.z))?
            .checked_add(1)?;
        (width > 0 && depth > 0)
            .then(|| {
                u64::try_from(width)
                    .ok()?
                    .checked_mul(u64::try_from(depth).ok()?)
            })
            .flatten()
    }
}

fn inst_spatial_coord(value: f32) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let cell = (value / INST_SPATIAL_CELL_METRES).floor();
    (cell >= i32::MIN as f32 && cell <= i32::MAX as f32).then_some(cell as i32)
}

fn inst_spatial_cells(bounds: InstSpatialBounds) -> Option<InstSpatialCellRect> {
    if !bounds.is_valid() {
        return None;
    }
    let min = InstSpatialCell {
        x: inst_spatial_coord(bounds.min.x)?,
        z: inst_spatial_coord(bounds.min.z)?,
    };
    let max = InstSpatialCell {
        x: inst_spatial_coord(bounds.max.x)?,
        z: inst_spatial_coord(bounds.max.z)?,
    };
    let rect = InstSpatialCellRect { min, max };
    if rect.area()? > MAX_INST_INDEX_CELLS_PER_BATCH {
        return None;
    }
    Some(rect)
}

/// Conservative world-XZ bounds of a wgpu-depth frustum. Used only to choose which grid cells to
/// inspect; exact cell and page AABB tests still decide visibility.
fn inst_frustum_cell_rect(view_proj: glam::Mat4) -> Option<InstSpatialCellRect> {
    let inv = view_proj.inverse();
    if !inv.to_cols_array().iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for z in [0.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                let h = inv * glam::Vec4::new(x, y, z, 1.0);
                if !h.is_finite() || h.w.abs() < 1.0e-6 {
                    return None;
                }
                let p = h.truncate() / h.w;
                min = min.min(p);
                max = max.max(p);
            }
        }
    }
    Some(InstSpatialCellRect {
        min: InstSpatialCell {
            x: inst_spatial_coord(min.x)?.saturating_sub(1),
            z: inst_spatial_coord(min.z)?.saturating_sub(1),
        },
        max: InstSpatialCell {
            x: inst_spatial_coord(max.x)?.saturating_add(1),
            z: inst_spatial_coord(max.z)?.saturating_add(1),
        },
    })
}

fn mark_inst_spatial_batches(marks: &mut [u64], batches: &[usize], unique: &mut usize) {
    for &batch in batches {
        let Some(word) = marks.get_mut(batch / 64) else {
            debug_assert!(false, "spatial index contains out-of-range batch {batch}");
            continue;
        };
        let bit = 1u64 << (batch % 64);
        if *word & bit == 0 {
            *word |= bit;
            *unique += 1;
        }
    }
}

impl InstSpatialIndex {
    fn remove(&mut self, batch: usize) {
        let Some(record) = self.records.get_mut(batch).and_then(Option::take) else {
            return;
        };
        let Some(rect) = record.cell_rect else {
            self.fallback.retain(|&candidate| candidate != batch);
            return;
        };
        for z in rect.min.z..=rect.max.z {
            for x in rect.min.x..=rect.max.x {
                let cell = InstSpatialCell { x, z };
                let mut remove_cell = false;
                if let Some(entry) = self.cells.get_mut(&cell) {
                    entry.batches.retain(|&candidate| candidate != batch);
                    if entry.batches.is_empty() {
                        remove_cell = true;
                    } else {
                        entry.world_min = glam::Vec3::splat(f32::INFINITY);
                        entry.world_max = glam::Vec3::splat(f32::NEG_INFINITY);
                        for &candidate in &entry.batches {
                            if let Some(candidate) = self
                                .records
                                .get(candidate)
                                .and_then(|candidate| candidate.as_ref())
                            {
                                entry.world_min = entry.world_min.min(candidate.bounds.min);
                                entry.world_max = entry.world_max.max(candidate.bounds.max);
                            }
                        }
                    }
                }
                if remove_cell {
                    self.cells.remove(&cell);
                }
            }
        }
    }

    fn update(&mut self, batch: usize, bounds: Option<InstSpatialBounds>) {
        if self.records.len() <= batch {
            self.records.resize_with(batch + 1, || None);
        }
        if let (Some(record), Some(bounds)) = (&mut self.records[batch], bounds) {
            let cell_rect = inst_spatial_cells(bounds);
            let vertical_change = (glam::Vec2::new(bounds.min.y, bounds.max.y) - record.indexed_y)
                .abs()
                .max_element();
            if record.cell_rect == cell_rect && (cell_rect.is_none() || vertical_change <= 2.0) {
                // Wind and other small motion normally stay in the same cells. Keep memberships
                // intact instead of removing the page and rescanning every static neighbour.
                // Per-page tests use the exact new bounds; cell bounds are conservative unions
                // until a membership change or bounded vertical refit rebuilds them in `remove`.
                record.bounds = bounds;
                if let Some(rect) = cell_rect {
                    for z in rect.min.z..=rect.max.z {
                        for x in rect.min.x..=rect.max.x {
                            let entry = self
                                .cells
                                .get_mut(&InstSpatialCell { x, z })
                                .expect("indexed page must retain its cells");
                            entry.world_min = entry.world_min.min(bounds.min);
                            entry.world_max = entry.world_max.max(bounds.max);
                        }
                    }
                }
                return;
            }
        }
        self.remove(batch);
        let Some(bounds) = bounds else {
            return;
        };
        let Some(rect) = inst_spatial_cells(bounds) else {
            self.fallback.push(batch);
            self.records[batch] = Some(InstSpatialRecord {
                bounds,
                indexed_y: glam::Vec2::new(bounds.min.y, bounds.max.y),
                cell_rect: None,
            });
            return;
        };
        self.records[batch] = Some(InstSpatialRecord {
            bounds,
            indexed_y: glam::Vec2::new(bounds.min.y, bounds.max.y),
            cell_rect: Some(rect),
        });
        for z in rect.min.z..=rect.max.z {
            for x in rect.min.x..=rect.max.x {
                let entry = self
                    .cells
                    .entry(InstSpatialCell { x, z })
                    .or_insert_with(|| InstSpatialCellEntry {
                        batches: Vec::new(),
                        world_min: bounds.min,
                        world_max: bounds.max,
                    });
                entry.batches.push(batch);
                entry.world_min = entry.world_min.min(bounds.min);
                entry.world_max = entry.world_max.max(bounds.max);
            }
        }
    }

    fn cell_visible(entry: &InstSpatialCellEntry, planes: &[glam::Vec4; 6]) -> bool {
        aabb_in_frustum(planes, entry.world_min, entry.world_max)
    }

    fn query(&self, view_proj: glam::Mat4) -> InstSpatialQuery {
        let planes = frustum_planes(view_proj);
        let rect = inst_frustum_cell_rect(view_proj);
        let dense_visits = rect.and_then(InstSpatialCellRect::area);
        let use_dense = dense_visits
            .map(|visits| visits <= self.cells.len() as u64)
            .unwrap_or(false);
        let mut query = InstSpatialQuery::default();
        // A tiny page-ID bitset deduplicates pages that overlap neighbouring cells and emits them
        // in stable ascending batch order. For Act 2 this scans 139 u64 words instead of sorting a
        // many-thousand-entry candidate vector on every camera/periscope pass.
        let mut marks = vec![0u64; self.records.len().div_ceil(64)];
        if use_dense {
            let rect = rect.expect("dense query requires a finite frustum cell rectangle");
            for z in rect.min.z..=rect.max.z {
                for x in rect.min.x..=rect.max.x {
                    query.index_visits += 1;
                    let cell = InstSpatialCell { x, z };
                    let Some(entry) = self.cells.get(&cell) else {
                        continue;
                    };
                    query.cell_tests += 1;
                    if Self::cell_visible(entry, &planes) {
                        mark_inst_spatial_batches(
                            &mut marks,
                            &entry.batches,
                            &mut query.page_tests,
                        );
                    }
                }
            }
        } else {
            for entry in self.cells.values() {
                query.index_visits += 1;
                query.cell_tests += 1;
                if Self::cell_visible(entry, &planes) {
                    mark_inst_spatial_batches(&mut marks, &entry.batches, &mut query.page_tests);
                }
            }
        }
        mark_inst_spatial_batches(&mut marks, &self.fallback, &mut query.page_tests);
        for (word_index, mut word) in marks.into_iter().enumerate() {
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let batch = word_index * 64 + bit;
                let visible = self
                    .records
                    .get(batch)
                    .and_then(|record| record.as_ref())
                    .map(|record| {
                        !record.bounds.is_valid()
                            || aabb_in_frustum(&planes, record.bounds.min, record.bounds.max)
                    })
                    .unwrap_or(false);
                if visible {
                    query.batches.push(batch);
                }
            }
        }
        query
    }

    fn stats(&self) -> (usize, usize, usize) {
        (
            self.cells.len(),
            self.cells.values().map(|entry| entry.batches.len()).sum(),
            self.fallback.len(),
        )
    }
}

/// Gribb–Hartmann frustum planes from a view-projection matrix (wgpu 0..1 depth: near = row2).
/// Each plane is (a,b,c,d) with p·(x,y,z,1) ≥ 0 meaning "inside".
fn frustum_planes(vp: glam::Mat4) -> [glam::Vec4; 6] {
    let r0 = vp.row(0);
    let r1 = vp.row(1);
    let r2 = vp.row(2);
    let r3 = vp.row(3);
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
}

/// Conservative AABB-vs-frustum: test the "positive vertex" (corner farthest along each plane
/// normal); a box strictly behind any plane is out.
fn aabb_in_frustum(planes: &[glam::Vec4; 6], mn: glam::Vec3, mx: glam::Vec3) -> bool {
    for p in planes {
        let v = glam::Vec3::new(
            if p.x >= 0.0 { mx.x } else { mn.x },
            if p.y >= 0.0 { mx.y } else { mn.y },
            if p.z >= 0.0 { mx.z } else { mn.z },
        );
        if p.dot(v.extend(1.0)) < 0.0 {
            return false;
        }
    }
    true
}

/// Transform a local AABB by a model matrix → world AABB (transform the 8 corners, re-box).
fn transform_aabb(m: glam::Mat4, mn: glam::Vec3, mx: glam::Vec3) -> (glam::Vec3, glam::Vec3) {
    let mut wmin = glam::Vec3::splat(f32::INFINITY);
    let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let c = glam::Vec3::new(
            if i & 1 == 0 { mn.x } else { mx.x },
            if i & 2 == 0 { mn.y } else { mx.y },
            if i & 4 == 0 { mn.z } else { mx.z },
        );
        let w = m.transform_point3(c);
        wmin = wmin.min(w);
        wmax = wmax.max(w);
    }
    (wmin, wmax)
}

/// Bounds for a moving raster pool. Center/extents need one affine transform per instance,
/// rather than eight corner transforms. Grass encodes a world-space tip displacement in unused
/// matrix-row values; include its complete quadratic root-to-tip bend in the culling envelope.
fn dynamic_instanced_bounds(
    transforms: &[glam::Mat4],
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    grass: bool,
) -> (glam::Vec3, glam::Vec3) {
    let center = (local_min + local_max) * 0.5;
    let half = (local_max - local_min) * 0.5;
    let mut world_min = glam::Vec3::splat(f32::INFINITY);
    let mut world_max = glam::Vec3::splat(f32::NEG_INFINITY);
    for transform in transforms {
        let x = transform.x_axis.truncate();
        let y = transform.y_axis.truncate();
        let z = transform.z_axis.truncate();
        let world_center = x * center.x + y * center.y + z * center.z + transform.w_axis.truncate();
        let extent = x.abs() * half.x + y.abs() * half.y + z.abs() * half.z;
        // Account for finite-precision reordering relative to the vertex shader at large world
        // coordinates. This small outward margin cannot cause edge-of-frustum blades to vanish.
        let margin =
            (world_center.abs() + extent) * (4.0 * f32::EPSILON) + glam::Vec3::splat(0.00001);
        let bend = if grass {
            glam::Vec3::new(transform.x_axis.w, 0.0, transform.z_axis.w)
        } else {
            glam::Vec3::ZERO
        };
        world_min = world_min.min(world_center - extent - margin + bend.min(glam::Vec3::ZERO));
        world_max = world_max.max(world_center + extent + margin + bend.max(glam::Vec3::ZERO));
    }
    (world_min, world_max)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct InstPagePlan {
    prototype: InstancedPrototype,
    inst_count: u32,
    world_min: glam::Vec3,
    world_max: glam::Vec3,
    object_id_base: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedPageRange {
    byte_offset: u64,
    capacity: u32,
}

fn instance_page_capacity(count: u32) -> Option<u32> {
    (count > 0)
        .then(|| count.checked_next_power_of_two())
        .flatten()
}

/// Validate a fixed pool allocation without touching wgpu. Unlike ordinary editor pages, moving
/// pools reserve exactly the requested number of matrices and IDs: rounding to a growth capacity
/// would waste storage and weaken the public fixed-capacity contract.
fn fixed_instanced_pool_capacity(capacity: usize) -> Option<u32> {
    let capacity = u32::try_from(capacity).ok()?;
    (capacity > 0 && capacity <= 0x40_0000).then_some(capacity)
}

/// Return the accepted live count for a fixed pool update. Zero is deliberately valid: pose
/// switching commonly empties one batch before populating another in the same simulation tick.
fn fixed_instanced_pool_update(capacity: u32, live_count: usize) -> Option<u32> {
    let live_count = u32::try_from(live_count).ok()?;
    (live_count <= capacity).then_some(live_count)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RtInstancedSlotPlan {
    offsets: Vec<Option<u32>>,
    end: u32,
}

/// Assign dense physical TLAS ranges from actual live counts. `None` denotes a raster-only,
/// grass, or degenerate-prototype batch; empty physical batches likewise occupy no slots. These
/// offsets are never object identities. Fixed pools keep their separately reserved custom-data
/// ranges and exact source records unchanged while this hardware-only layout moves.
fn plan_rt_instanced_slots(
    first_slot: u32,
    live_counts: &[Option<u32>],
) -> Option<RtInstancedSlotPlan> {
    let mut next = first_slot;
    let mut offsets = Vec::with_capacity(live_counts.len());
    for count in live_counts {
        let Some(count) = (*count).filter(|count| *count > 0) else {
            offsets.push(None);
            continue;
        };
        offsets.push(Some(next));
        next = next.checked_add(count)?;
    }
    Some(RtInstancedSlotPlan { offsets, end: next })
}

/// Every slot beyond the new dense layout must be cleared, including transitions to no live
/// moving instances. Both counts address the same allocated TLAS; allocation bounds win.
fn rt_vacated_slots(previous_count: u32, next_count: u32, capacity: u32) -> std::ops::Range<usize> {
    next_count.min(capacity) as usize..previous_count.min(capacity) as usize
}

fn packed_page_ranges(page_lengths: &[usize]) -> Option<(Vec<PackedPageRange>, u64)> {
    let mut byte_offset = 0u64;
    let mut ranges = Vec::with_capacity(page_lengths.len());
    for &length in page_lengths {
        let count = u32::try_from(length).ok()?;
        // Power-of-two slack lets editor paint/merge operations grow a page in place without
        // reallocating on every added instance. Static campaign pages pay less than 2x of a tiny
        // 64-byte matrix stream while geometry/material/BLAS remain shared independently.
        let capacity = instance_page_capacity(count)?;
        ranges.push(PackedPageRange {
            byte_offset,
            capacity,
        });
        byte_offset = byte_offset.checked_add(u64::from(capacity) * INSTANCE_MAT_BYTES)?;
    }
    Some((ranges, byte_offset))
}

impl InstPagePlan {
    fn object_id_end(self) -> u32 {
        self.object_id_base + self.inst_count
    }
}

/// CPU half of `add_instanced_page`: establishes the prototype reference, exact local AABB and
/// non-overlapping object-ID range before any wgpu object is created. This makes the fundamental
/// sharing/page-boundary contract testable without selecting a GPU adapter.
fn plan_instanced_page(
    prototype: InstancedPrototype,
    transforms: &[glam::Mat4],
    local_min: glam::Vec3,
    local_max: glam::Vec3,
    object_id_base: u32,
) -> Option<InstPagePlan> {
    if transforms.is_empty() {
        return None;
    }
    let inst_count = u32::try_from(transforms.len()).expect("instance page count exceeds u32");
    let mut world_min = glam::Vec3::splat(f32::INFINITY);
    let mut world_max = glam::Vec3::splat(f32::NEG_INFINITY);
    for transform in transforms {
        let (min, max) = transform_aabb(*transform, local_min, local_max);
        world_min = world_min.min(min);
        world_max = world_max.max(max);
    }
    Some(InstPagePlan {
        prototype,
        inst_count,
        world_min,
        world_max,
        object_id_base,
    })
}

/// Splitscreen ceiling: four couch-crew viewports (keyboard + three pads), with the fifth slot for
/// the gunner EYEPIECE COMPOSITE's interior pass. Solo still uses view 0 (and may use an otherwise
/// idle context for its composite). Every view keeps its own surface and volume temporal contexts
/// (three history ping-pongs, shared reject snapshots, previous camera); unused slots cost ~12.9 MB
/// VRAM each at 640×360 and are inert until something renders them.
pub const MAX_VIEWS: usize = 5;

/// One viewport's temporal state. Reprojection assumes a CONTINUOUS camera; in splitscreen each
/// viewport re-renders every frame with its own camera, so history/reject state must never be
/// shared across views (view A's pixels would ghost into view B's reprojection).
struct ViewCtx {
    // Retain views of the already allocated history targets for opt-in diagnostic readback.
    debug_diff_history: [wgpu::TextureView; 2],
    debug_spec_history: [wgpu::TextureView; 2],
    debug_volume_history: [wgpu::TextureView; 2],
    accum_binds: [wgpu::BindGroup; 2],
    spec_accum_binds: [wgpu::BindGroup; 2],
    resolve_binds: [wgpu::BindGroup; 2],
    reconstruction_binds: Option<surface_reconstruction::Bindings>,
    reconstruction_resolve_binds: Option<[wgpu::BindGroup; 2]>,
    probe_binds: Option<live_texel_probe::Bindings>,
    probe_resolve_binds: Option<[wgpu::BindGroup; 2]>,
    volume_accum_binds: [wgpu::BindGroup; 2],
    volume_composite_binds: [wgpu::BindGroup; 2],
    prev_depth_tex: wgpu::Texture,
    prev_id_tex: wgpu::Texture,
    accum_flip: usize,
    prev_vp: glam::Mat4,
    prev_models: Vec<glam::Mat4>,
    prev_cam_sig: (glam::Vec3, glam::Vec3, f32, f32),
    prev_depth_range: [f32; 2],
    prev_light_sources: Option<light_history::Sources>,
    history_valid: bool,
    volume_history_valid: bool,
}

/// A sub-viewport blit request: source crop (uv offset + size) out of the internal frame and the
/// destination rectangle in target pixels. `clear` = this is the first view of the frame (the
/// pass clears the whole target); later views load and paint their own rectangle only.
#[derive(Clone, Copy)]
pub struct BlitRect {
    pub src_uv: [f32; 4],
    pub dst_px: [f32; 4],
    pub clear: bool,
}

/// Actual work encoded by `render_internal`, accumulated until explicitly reset. This is a cheap
/// CPU-side counter—not a timing estimate—and therefore works in headless tests and on Metal where
/// timestamp-query support varies. Couch benchmarks reset it once per simulation frame so the log
/// exposes the real five crew-camera submissions plus any periscope RTT instead of inferring work
/// from scene size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderWorkStats {
    pub scene_renders: u64,
    pub shadow_passes: u64,
    pub shadow_mesh_draws: u64,
    pub shadow_instanced_draws: u64,
    pub scene_mesh_draws: u64,
    pub scene_instanced_draws: u64,
    pub fullscreen_raster_passes: u64,
    pub compute_passes: u64,
    pub smoke_passes: u64,
    pub bloom_passes: u64,
    pub snow_passes: u64,
}

impl RenderWorkStats {
    fn accumulate(&mut self, rhs: Self) {
        self.scene_renders += rhs.scene_renders;
        self.shadow_passes += rhs.shadow_passes;
        self.shadow_mesh_draws += rhs.shadow_mesh_draws;
        self.shadow_instanced_draws += rhs.shadow_instanced_draws;
        self.scene_mesh_draws += rhs.scene_mesh_draws;
        self.scene_instanced_draws += rhs.scene_instanced_draws;
        self.fullscreen_raster_passes += rhs.fullscreen_raster_passes;
        self.compute_passes += rhs.compute_passes;
        self.smoke_passes += rhs.smoke_passes;
        self.bloom_passes += rhs.bloom_passes;
        self.snow_passes += rhs.snow_passes;
    }

    pub fn indexed_draws(self) -> u64 {
        self.shadow_mesh_draws
            + self.shadow_instanced_draws
            + self.scene_mesh_draws
            + self.scene_instanced_draws
    }

    pub fn delta_since(self, earlier: Self) -> Self {
        Self {
            scene_renders: self.scene_renders.saturating_sub(earlier.scene_renders),
            shadow_passes: self.shadow_passes.saturating_sub(earlier.shadow_passes),
            shadow_mesh_draws: self
                .shadow_mesh_draws
                .saturating_sub(earlier.shadow_mesh_draws),
            shadow_instanced_draws: self
                .shadow_instanced_draws
                .saturating_sub(earlier.shadow_instanced_draws),
            scene_mesh_draws: self
                .scene_mesh_draws
                .saturating_sub(earlier.scene_mesh_draws),
            scene_instanced_draws: self
                .scene_instanced_draws
                .saturating_sub(earlier.scene_instanced_draws),
            fullscreen_raster_passes: self
                .fullscreen_raster_passes
                .saturating_sub(earlier.fullscreen_raster_passes),
            compute_passes: self.compute_passes.saturating_sub(earlier.compute_passes),
            smoke_passes: self.smoke_passes.saturating_sub(earlier.smoke_passes),
            bloom_passes: self.bloom_passes.saturating_sub(earlier.bloom_passes),
            snow_passes: self.snow_passes.saturating_sub(earlier.snow_passes),
        }
    }
}

impl Default for BlitRect {
    fn default() -> Self {
        Self {
            src_uv: [0.0, 0.0, 1.0, 1.0],
            dst_px: [0.0; 4],
            clear: true,
        }
    }
}

pub struct Renderer {
    pub shader_backend: bool,
    /// The raymarch frame pipeline (`KESSEL_RENDERER=raymarch`); None on the other backends.
    raymarch: Option<raymarch::Backend>,
    raster_optics: Option<raster_optics::RasterOptics>,
    raster_ssao: Option<raster_ssao::RasterSsao>,
    geometry_aa: Option<renderer_aa::GeometryAa>,
    hover_outline: Option<hover_outline::HoverOutline>,
    hovered_meshes: Vec<usize>,
    raster_glass_pipe: Option<wgpu::RenderPipeline>,
    raster_glow_glass_pipe: Option<wgpu::RenderPipeline>,
    pub iw: u32,
    pub ih: u32,
    /// Session-local preset captured when this renderer was constructed.
    pub graphics_preset: crate::graphics::GraphicsPreset,
    transport_quality: TransportQuality,
    /// Actual allocated sun-shadow side for profiling/diagnostics.
    pub shadow_res: u32,
    pub camera: Camera,
    pub master_retro: f32,
    pub display_tuning: Option<display_tuning::DisplayTuning>,
    /// User reproduction controls: exposure multiplier, saturation, warmth, gamma.
    pub display_grade: [f32; 4],
    /// Optional live display comparison; transport and lighting are unchanged.
    pub display_dither_pattern: f32,
    pub sky_time_scale: f32,
    /// The accelerated day and night cycle, and the share of the current day that is daylight.
    pub day_night_cycle: bool,
    /// Real seconds the Sun spends at night, in twilight and in full day over the current date.
    day_phase_seconds: Option<[f64; 3]>,
    weather_offset: glam::Vec3,
    pub weather_override: Option<f32>,
    weather_original: Option<(f32,f32,f32,f32,glam::Vec2)>,
    user_sky_phase: Option<(u8,crate::celestial_sky::SkyClock)>,
    /// Optional fixed capture exposure. Normal play uses DEFAULT_CAMERA_EXPOSURE without any
    /// adaptation. Deterministic photo harnesses can explicitly select another fixed HDR key.
    pub exposure_override: Option<f32>,
    /// 0..1 sharper, brighter eye (the cigarette). OWNER-DIRECTED PRESENTATION on the display
    /// grade only: no emitter, medium or material changes. See docs/PHYSICAL_TRUTH_AUDIT.md.
    pub vision_boost: f32,
    pub fog_density: f32,
    pub reticle: bool,
    /// Legacy gunner-optic obscuration envelope. Retained while the game-side producer migrates;
    /// the screen-space snow-dust veil was not a modeled medium and is no longer rendered.
    pub optic_blast: f32,
    /// Where the gunner's eye sits off the sight's axis, in fractions of the screen height (x
    /// right, y up): the sight's true line against the steadied view (`Game::sight_view_model`).
    /// The eyepiece pass shifts what lies on the glass and the exit pupil's shading by it.
    pub optic_eye: [f32; 2],
    /// The player's gun muzzle (position, bore direction) this frame, so a backend can tell the
    /// gun's own report from a detonation downrange. Set by the game each update.
    pub muzzle_hint: Option<(glam::Vec3, glam::Vec3)>,
    /// Snow lying on the player's vehicle and on the world, 0..1 (`snow_cover`), set by the game
    /// each update; the raymarch shade pass turns them into coverage per pixel.
    pub player_snow_load: f32,
    pub world_snow_load: f32,
    /// Wear on the player's vehicle (`surface_wear`): interior grime, exterior grime, glass grime,
    /// mechanical wear, each 0..1; set by the game each update (raymarch backend).
    pub wear: [f32; 4],
    /// The player's Tiger's winter whitewash, 0 (none) to 1 (a full field coat): the raymarch
    /// G-buffer lays it over the exterior paint (`rm_gbuffer.wgsl` `apply_whitewash`).
    pub whitewash: f32,
    /// The player's Tiger's exhaust puffs this frame (`exhaust_plume`), drawn by the raymarch
    /// medium (parcel kind 7).
    pub exhaust_puffs: Vec<crate::exhaust_plume::RenderPuff>,
    /// Fog banks in the world medium (raymarch backend): extinction inside a bank /m, coverage
    /// 0..1, depth m above the floor, the floor's altitude y. All zero: none.
    pub fog_bank: [f32; 4],
    /// Source-qualified admission for the observer's flash detector, never added light energy.
    pub observer_flash_source: bool,
    /// Compatibility input retained for game-side callers. It is intentionally not consumed by the
    /// renderer: crew injury is represented by physical grains, stains and bodies in scene space.
    pub crew_ko: f32,
    /// Compatibility direction paired with [`Self::crew_ko`]; also inert.
    pub crew_ko_dir: [f32; 2],
    /// Severe-hit visual disorientation, 0..1. UI is drawn after this effect and remains readable.
    pub concussion: f32,
    /// Main-menu turntable pass flag. Set true only by the `render_menu_*` entry points (before
    /// `upload_uniforms`) so crush.wgsl suppresses grain on the pure-black backdrop; false for the game.
    menu_mode: bool,
    /// Darkness derived from the ephemeris Sun: 0 while it is up, 1 once civil twilight ends.
    /// It gates only the legacy emissive-window material and stylized palettes still reading it;
    /// it never scales the Sun, the Moon or any other light source.
    pub night: f32,
    /// The act's running clock (Universal Time) and place. The Sun and the Moon come from it.
    pub sky_clock: crate::celestial_sky::SkyClock,
    /// Sun and Moon of `sky_clock`'s current instant, as key and secondary directional sources.
    celestial: crate::celestial_sky::CelestialLighting,
    /// The authored lighting record's instant last applied to the clock.
    authored_clock_start: Option<crate::ephemeris::UtcInstant>,
    /// Seasonal precipitation gate. Winter acts use 1.0; the spring homeland uses 0.0 so a green
    /// Oderbruch/Halbe landscape is not inexplicably covered by the Stalingrad blizzard overlay.
    pub precipitation: f32,
    /// Cloud cover: 0 clear, 1 sealed overcast. The campaign opens from blizzard into blue spring sky.
    pub overcast: f32,
    /// Moving weather severity. Drives cloud contrast, gust haze, snowfall density and spindrift.
    pub storm: f32,
    /// Act palette selector: 0 = Stalingrad winter, 0.5 = Oderbruch, 1 = Halbe/finale.
    pub atmosphere_mood: f32,
    /// -1 dawn, 0 neutral day/night, +1 dusk; lets twilight retain a bruised horizon under cloud.
    pub twilight: f32,
    /// Authored 0..1 wind strength for the act profile. Live play replaces `weather_wind_world`
    /// with the existing zoned world wind at the camera/tank; standalone renderer users retain this
    /// prevailing fallback.
    pub weather_wind: f32,
    /// Horizontal world-space wind (x,z), shared with smoke/fire in live play. Snow advection uses
    /// this vector directly, so turning the camera changes projection rather than wind direction.
    pub weather_wind_world: glam::Vec2,
    /// 0 outdoors, 1 inside armor. Shelter removes the camera-near layers and ground powder but
    /// leaves distant layers available through real depth openings such as the hatch and optics.
    pub weather_shelter: f32,
    /// Full authored act/world seed. A stable 24-bit token is sent to WGSL for integer placement.
    weather_seed: u32,
    /// Selects the physical cloud/aerosol and prevailing-wind profile uploaded to every view.
    weather_act_id: u8,
    weather_progress: f32,
    /// Interior/shelter state, 0..1. The game sets this while the player is buttoned up so view-local
    /// geometry, weather and interaction systems can select the correct state. It must not inject,
    /// clamp or recolor lighting; modeled emitters and normal light transport own radiance.
    pub interior_light: f32,
    /// Interior smoke veil (0..1) — set by game.rs from the buttoned-up haze level.
    pub interior_haze: f32,
    /// Optic mask over the crushed frame: 0 none, 1 TZF circle, 2 visor letterbox, 3 binoculars.
    pub optic_mask: f32,
    /// Positioned point lights this frame (game.rs sets them; capped at N_LIGHTS in the shader).
    lights: Vec<PointLight>,
    /// Point lights offered for the latest frame (all of them, before the `N_LIGHTS` budget).
    lights_offered: usize,
    /// Every offered light (bounded by `raymarch::MAX_SCENE_LIGHTS`); the raymarch backend
    /// clusters all of them instead of taking the `N_LIGHTS` budget.
    lights_full: Vec<PointLight>,
    frame: u32,

    hdr_view: wgpu::TextureView,
    hdr_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    ldr_view: wgpu::TextureView,
    ldr_tex: wgpu::Texture,
    /// A settled real driver-view frame. Menus reuse it so idle UI does not keep tracing rays.
    title_backdrop: Option<wgpu::Texture>,
    title_backdrop_frames: u32,
    title_backdrop_settled: bool,
    title_last_frame: Option<std::time::Instant>,
    title_eye_convergence: crate::HeadlessExposureConvergence,
    optic_tex: wgpu::Texture,
    #[allow(dead_code)] // retained for bind-group recreation on resize
    sampler: wgpu::Sampler,

    sky_pipe: wgpu::RenderPipeline,
    /// Same physical sky shader, but admitted only where the opaque forward depth buffer stayed at
    /// the far clear value. This preserves every visible sky sample while avoiding atmosphere work
    /// behind steel, terrain and buildings on the Metal/Laptop path.
    sky_depth_pipe: wgpu::RenderPipeline,
    depth_gate_sky: bool,

    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    /// One shared atmosphere/sun parameter authority for visible misses, surface paths, and media.
    physical_environment_buf: wgpu::Buffer,
    physical_environment_bind: wgpu::BindGroup,
    mesh_pipe: wgpu::RenderPipeline,
    mesh_inst_pipe: wgpu::RenderPipeline,
    fine_vfx: renderer_fine_vfx::FineVfx,
    weapon_particles: renderer_weapon_particles::WeaponParticles,
    shadow_inst_pipe: wgpu::RenderPipeline,
    meshes: Vec<MeshGpu>,
    inst_prototypes: Vec<InstPrototype>,
    /// Material uploads of instanced prototypes by their texture images (see
    /// `Renderer::instanced_material`).
    inst_material_cache: HashMap<[usize; 3], (Arc<wgpu::BindGroup>, Vec<Arc<Vec<u8>>>)>,
    inst_batches: Vec<InstBatch>,
    /// Compact 128 m cell index over `inst_batches`; render queries touch cells, then only the
    /// candidate page AABBs, instead of linearly testing every campaign page in every pass.
    inst_spatial_index: InstSpatialIndex,
    instance_queue: wgpu::Queue,
    pose_device: wgpu::Device,
    next_inst_buffer_id: usize,
    /// Sim time (s), accumulated by whoever drives the frame — grass wind phase.
    pub time: f32,
    /// Act half-extent enables the track material; 0 = off. Sparse cells keep fixed world spacing.
    pub track_half: f32,
    track_view: wgpu::TextureView,
    track_bind: wgpu::BindGroup,
    track_pipe: wgpu::RenderPipeline,
    pending_stamps: Vec<[f32; 5]>,
    track_lookup: wgpu::Texture,
    track_atlas: track_ruts::Atlas,
    track_footprints: HashMap<[u32; 2], track_ruts::Footprint>,
    /// Kept from `new` so the front-end can upload extra meshes after construction (the main-menu
    /// turntable Tiger, `menu_meshes`) reusing the same material bind-group layout.
    material_bgl: wgpu::BindGroupLayout,
    /// Showcase meshes for the main-menu turntable (the Tiger hull/turret/gun), SEPARATE from the
    /// scene `meshes` so the menu path is fully decoupled from the game world. Empty until
    /// `load_menu_meshes` is called; rendered only by the `render_menu_*` entry points.
    menu_meshes: Vec<MeshGpu>,

    // ---- sun shadow map ----
    shadow_view: wgpu::TextureView,
    shadow_pipe: wgpu::RenderPipeline,
    shadow_bgl: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    scene_visibility: crate::scene_visibility::SceneVisibility,
    scene_visibility_gpu: crate::scene_visibility_gpu::SceneVisibilityGpu,
    shadow_bind: wgpu::BindGroup, // group(2) for the mesh pass: depth map + comparison sampler
    near_shadow_view: wgpu::TextureView,
    near_shadow_camera_buf: wgpu::Buffer,
    near_shadow_camera_bind: wgpu::BindGroup,
    near_shadow_buf: wgpu::Buffer,
    near_shadow_res: u32,
    cabin_shadows: cabin_shadows::CabinShadows,
    sky_visibility: sky_visibility::SkyVisibility,
    coop_shadow: SharedShadowFrame,

    // ---- S2 deferred path: G-buffer + combine ----
    /// true = G-buffer + combine (the M-D shape); false = the forward raster mesh pass.
    pub deferred: bool,
    /// Raster surface lighting is live (the shader backend, or the physical backend without ray
    /// queries): the environment probe and the visibility atlas are prepared every frame.
    raster_lighting: bool,
    portable_visibility: bool,
    /// The physical environment for raster lighting (environment_probe.rs).
    environment_probe: environment_probe::EnvironmentProbe,
    gbuf_pipe: wgpu::RenderPipeline,
    gbuf_inst_pipe: wgpu::RenderPipeline,
    /// Present only when the device exposes EXPERIMENTAL_RAY_QUERY. `combine.wgsl` contains
    /// acceleration-structure bindings and must never be compiled on the Metal raster fallback.
    combine_pipe: Option<wgpu::RenderPipeline>,
    gbuf_bind: wgpu::BindGroup,
    g0_view: wgpu::TextureView,
    g1_view: wgpu::TextureView,
    g2_view: wgpu::TextureView,
    g3_view: wgpu::TextureView,

    // ---- S4 ray integrator + auto-exposure ----
    /// Per-act sun direction (TO the sun). Was a const; per-act mood is a field now.
    pub sun_dir: glam::Vec3,
    /// Legacy diagnostic meter only. Gameplay always constructs this false: bright windows,
    /// view changes, and elapsed time must not trigger eye/dark adaptation.
    pub autoexp: bool,
    /// Ray-query-only layout. Metal in wgpu 27 does not expose this feature, so its renderer
    /// initialization must not create even the layout while using the forward fallback.
    rt_as_bgl: Option<wgpu::BindGroupLayout>,
    /// TLAS bind shared by the surface combine and volumetric integrators; recreated whenever
    /// rt_update rebuilds the TLAS.
    combine_rt_bind: Option<wgpu::BindGroup>,
    exp_pipe: wgpu::ComputePipeline,
    /// Eye adaptation is view-local. Sharing one state made four couch cameras pump one another's
    /// exposure and made the result depend on viewport submission order.
    exp_binds: Vec<wgpu::BindGroup>,
    exp_bufs: Vec<wgpu::Buffer>,
    exp_clock_buf: wgpu::Buffer,
    replay_previous_elapsed: Option<f32>,
    replay_exposure_time: Option<f32>,
    continuous_camera_translation: bool,

    // ---- S3 temporal accumulation ----
    /// EMA accumulation of the demodulated diffuse (α=0.15). "Off" = α forced to 1.0 (the
    /// pass still runs so the resolve plumbing never branches). KESSEL_NO_ACCUM=1 disables.
    pub accum_on: bool,
    /// Deterministic hash-noise injection (the rt-accum convergence gate; stands in for S4 rays).
    pub accum_test: bool,
    lit_diff_view: wgpu::TextureView,
    lit_other_view: wgpu::TextureView,
    lit_emission_view: wgpu::TextureView,
    accum_pipe: wgpu::ComputePipeline,
    accum_delta_bind: wgpu::BindGroup,
    accum_ubuf: wgpu::Buffer,
    delta_buf: wgpu::Buffer,
    resolve_pipe: wgpu::RenderPipeline,
    /// Explicit A/B opt-in; separate transient diffuse output never feeds the original histories.
    surface_reconstruction: Option<surface_reconstruction::Gpu>,
    live_texel_probe: Option<live_texel_probe::Gpu>,
    live_texel_probe_view_inside: bool,
    g3_tex: wgpu::Texture,
    /// Per-view temporal state (splitscreen: each viewport is its own continuous camera, so each
    /// keeps its own history ping-pong, reprojection-reject snapshots and previous-camera record).
    /// Solo rendering only ever touches `views[0]`.
    views: Vec<ViewCtx>,
    active_view: usize,
    render_work: Cell<RenderWorkStats>,
    gpu_profile: Option<crate::gpu_profile::GpuProfile>,

    // ---- M-D hardware ray tracing (rt.rs) ----
    /// Instanced batches already reported for a degenerate instance transform (see
    /// `rt_safe_instance_transform`): one line per batch, not per frame.
    rt_degenerate_reported: std::collections::HashSet<usize>,
    /// Device came up with EXPERIMENTAL_RAY_QUERY and KESSEL_NO_RT didn't veto it.
    pub rt_on: bool,
    /// The scene BLASes and per-frame TLAS exist: `rt_on`, or the raymarch backend's ray queries
    /// (`graphics::raymarch_ray_queries_wanted`). The exact surface pack stays `rt_on`-only.
    pub rt_scene: bool,
    /// Splitscreen: views 2..N of the SAME sim frame reuse view 1's TLAS (scene transforms are
    /// camera-independent) — set true before their renders, cleared after. Never skips the very
    /// first build (the guard checks a TLAS exists).
    pub rt_skip: bool,
    /// True only while the gunner's exterior optic snapshot is being rasterised.
    optic_hide_pass: bool,
    /// The scene TLAS, rebuilt every frame from mesh/instance transforms (wgpu 27 has no refit;
    /// full rebuild is the only mode and is ~0.1–0.3 ms at this instance count). Created lazily
    /// on the first frame — instanced batches arrive after Renderer::new.
    rt_tlas: Option<wgpu::Tlas>,
    rt_tlas_cap: u32,
    /// Frames whose ray scene overflowed the TLAS, the instances missing in the latest frame, and
    /// the overflow last reported (logged again whenever it changes).
    rt_overflow_frames: u64,
    rt_overflow_now: u32,
    rt_overflow_reported: Option<u32>,
    /// TLAS slots written last frame (the tail beyond this frame's count is already None).
    rt_prev_count: u32,
    /// Instanced batches changed (add_instanced / set_instanced_mats / TLAS recreate) → the
    /// static batch tail needs rewriting. Mesh slots rewrite every frame (visible/emissive
    /// state is per-frame); the 11k+ static city instances write once. ~2 ms/frame saved.
    rt_batches_dirty: bool,
    /// Ray-visible pools with unchanged live counts have stable TLAS ranges and can rewrite only
    /// their small reserved ranges between full static-tail updates.
    rt_moving_batches_dirty: bool,
    /// One stable TLAS range start per instanced batch (`None` for non-RT batches). Recomputed only
    /// when topology/capacity changes; moving-pool live counts never invalidate it.
    rt_batch_slot_offsets: Vec<Option<u32>>,
    /// Exact committed-hit surface/material tables for the High one-bounce integrator. Rebuilt only
    /// when source topology or custom-data allocation changes; transforms stay authoritative in TLAS.
    rt_surface_gpu: Option<RtSurfaceGpu>,
    rt_surface_dirty: bool,
    rt_instance_topology_dirty: bool,
    rt_scene_surface_ids: Vec<u32>,
    rt_prototype_surface_ids: Vec<u32>,
    /// The depth attachment's texture, kept for the bvhview depth-correlation readback.
    depth_tex: wgpu::Texture,

    // ---- emissive bloom (half internal res) ----
    bw: u32,
    bh: u32,
    bloom_a_view: wgpu::TextureView,
    bloom_b_view: wgpu::TextureView,
    bloom_bright_pipe: wgpu::RenderPipeline,
    bloom_blur_pipe: wgpu::RenderPipeline,
    bloom_composite_pipe: wgpu::RenderPipeline,
    /// Veiling glare of the eye over the Gaussian bloom (raymarch chain only): a box-filtered
    /// pyramid of the blurred bright pass, one level per glare ring.
    /// `None` on the physical backend, whose `bloom.wgsl` has no glare entry points.
    bloom_glare_pipe: Option<wgpu::RenderPipeline>,
    bloom_composite_glare_pipe: Option<wgpu::RenderPipeline>,
    bloom_copy_pipe: Option<wgpu::RenderPipeline>,
    bloom_levels: Vec<wgpu::TextureView>,
    bloom_level_binds: Vec<wgpu::BindGroup>,
    bloom_glare_bind: Option<wgpu::BindGroup>,
    bloom_bind_hdr: wgpu::BindGroup, // src = full-res HDR (bright pass)
    bloom_bind_a: wgpu::BindGroup,   // src = bloom_a
    bloom_bind_b: wgpu::BindGroup,   // src = bloom_b
    bloom_portal_buf: Option<wgpu::Buffer>, // shader-only visible outdoor-aperture halo

    // ---- volumetric muzzle-blast smoke (ray-marched over the HDR target, before bloom) ----
    smoke_puffs: [PuffGpu; MAX_PUFFS],
    breech_gas: PuffGpu,
    cabin_medium: Option<std::sync::Arc<cabin_medium::Medium>>,
    cabin_medium_texture: wgpu::Texture,
    cabin_medium_view: wgpu::TextureView,
    cabin_medium_uniform: cabin_medium::Uniform,
    cabin_medium_pose: Option<(glam::Mat4,glam::Mat4)>,
    atmo_enabled: bool, // spatial cabin medium exists independently of the current camera
    atmo_haze: f32,     // 0..1 interior hazy-atmosphere density (0 = pass skips it)
    atmo_dust: f32,     // 0..1 dust-mote sparkle inside the cabin haze
    atmo_origin: glam::Vec3, // tank hull origin: dust rides this frame with a slow cabin-local drift
    smoke_buf: wgpu::Buffer,
    smoke_bind: wgpu::BindGroup,
    /// Portable volume transport keeps the physical-environment ABI at group(2), so group(1)
    /// remains an explicitly bound empty visibility set when ray queries are unavailable.
    smoke_empty_visibility_bind: wgpu::BindGroup,
    smoke_pipe: wgpu::RenderPipeline,
    /// Raw unbiased premultiplied volume estimate for this submission. High/Vulkan converges it in
    /// a per-view history; portable forward composites it directly (its visibility path emits no
    /// stochastic direct radiance).
    volume_current_view: wgpu::TextureView,
    volume_accum_pipe: wgpu::ComputePipeline,
    volume_composite_pipe: wgpu::RenderPipeline,
    volume_current_composite_bind: wgpu::BindGroup,

    // World-space discrete particles have an explicit one-native-pixel presentation. Continuous
    // aerosol transport stays in smoke.wgsl and is not replaced by these points.
    pixel_particle_field: RefCell<crate::pixel_particles::PixelParticleField>,
    pixel_particle_count: Cell<u32>,
    pixel_particle_vertices: wgpu::Buffer,
    pixel_particle_uniform: wgpu::Buffer,
    pixel_particle_bind: wgpu::BindGroup,
    pixel_particle_pipe: Option<wgpu::RenderPipeline>,

    pub pressure_fronts: crate::pressure_front::Fronts,
    pressure_gpu: crate::pressure_front::Gpu,
    pub heat_sources: crate::heat_haze::Sources,
    heat_gpu: Option<crate::heat_haze::Gpu>,

    crush_buf: wgpu::Buffer,
    /// Compatibility dial retained for callers; it no longer controls a proxy modulation pass.
    pub rt_mix: f32,
    /// Compatibility flag retained for callers; the analytic heightfield/OBB pass was deleted.
    pub portable_ao_enabled: bool,
    crush_bind: wgpu::BindGroup,
    crush_pipe: wgpu::RenderPipeline,

    blit_shader: wgpu::ShaderModule,
    blit_layout: wgpu::PipelineLayout,
    blit_bind: wgpu::BindGroup,
    blit_ubuf: wgpu::Buffer,
    blit_pipes: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,

    // ---- front-end text/UI (title, menu, death screen), composited over the final blit at DISPLAY
    // resolution — deliberately OUTSIDE the PS1 crush so the German UI stays crisp. See src/ui.rs. ----
    ui: Ui,
    held_map_surface: Option<held_map_render::MapSurface>,
}

/// Compatibility vertical extent used by game-side FOV math while true optical transport is pending.
pub const RTT_TILE_H: u32 = 180;

/// Scene-referred and observer state of one rendered frame (see `debug_frame_photometry`).
#[derive(Clone, Debug)]
pub struct FramePhotometry {
    pub mean_cd_m2: f64,
    pub log_mean_cd_m2: f64,
    pub median_cd_m2: f64,
    pub p10_cd_m2: f64,
    pub p90_cd_m2: f64,
    /// Mean luminance of an 8 x 4 grid of image regions, row-major from the top left.
    pub grid_cd_m2: [f64; 32],
    pub eye: [f32; EYE_STATE_LANES],
    pub probe_beam: [f32; 4],
    pub probe_horizontal: [f32; 4],
    pub key_body: String,
    pub sun_altitude_deg: f64,
    pub moon_altitude_deg: f64,
    pub instant: String,
}

impl FramePhotometry {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "instant": self.instant, "key_body": self.key_body,
            "sun_altitude_deg": self.sun_altitude_deg, "moon_altitude_deg": self.moon_altitude_deg,
            "hdr_mean_cd_m2": self.mean_cd_m2, "hdr_log_mean_cd_m2": self.log_mean_cd_m2,
            "hdr_median_cd_m2": self.median_cd_m2, "hdr_p10_cd_m2": self.p10_cd_m2,
            "hdr_p90_cd_m2": self.p90_cd_m2, "hdr_grid_cd_m2": self.grid_cd_m2.to_vec(),
            "eye_cone_exposure": self.eye[0], "eye_rod_exposure": self.eye[2],
            "eye_cone_adaptation_cd_m2": self.eye[12], "eye_rod_adaptation_scot_cd_m2": self.eye[13],
            "eye_mesopic_m": self.eye[14], "eye_metered_photopic_cd_m2": self.eye[15],
            "eye_metered_scotopic": self.eye[16], "eye_pupil_mm": self.eye[17],
            "probe_beam_ru": self.probe_beam, "probe_horizontal_ru": self.probe_horizontal,
        })
    }
}

fn debug_half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 1023);
    if exponent == 0 {
        let magnitude = fraction as f32 * (1.0 / 16_777_216.0);
        if sign == 0 { magnitude } else { -magnitude }
    } else {
        let exponent = if exponent == 31 { 255 } else { exponent + 112 };
        f32::from_bits(sign | (exponent << 23) | (fraction << 13))
    }
}

fn debug_linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn debug_light_aces(v: f32) -> f32 {
    let x = if v.is_finite() { v.max(0.0) } else { 0.0 };
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

fn debug_read_rgba16f(
    device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture,
) -> anyhow::Result<Vec<u8>> {
    // A 32-bit float frame is narrowed to the half floats these diagnostics were written for.
    if texture.format() == wgpu::TextureFormat::Rgba32Float {
        return Ok(debug_read_rgba_f32(device, queue, texture)?.into_iter().flatten()
            .flat_map(|v| debug_f32_to_half(v).to_le_bytes()).collect());
    }
    anyhow::ensure!(texture.format() == wgpu::TextureFormat::Rgba16Float,
        "light-layer readback requires RGBA16F");
    debug_read_light_target(device, queue, texture)
}

/// The frame's texels at the precision they are stored in, whichever float format holds them.
fn debug_read_rgba_f32(
    device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture,
) -> anyhow::Result<Vec<[f32; 4]>> {
    let raw = debug_read_light_target(device, queue, texture)?;
    Ok(match texture.format() {
        wgpu::TextureFormat::Rgba32Float => raw.chunks_exact(16).map(|p| std::array::from_fn(|c|
            f32::from_le_bytes([p[c * 4], p[c * 4 + 1], p[c * 4 + 2], p[c * 4 + 3]]))).collect(),
        _ => raw.chunks_exact(8).map(|p| std::array::from_fn(|c|
            debug_half_to_f32(u16::from_le_bytes([p[c * 2], p[c * 2 + 1]])))).collect(),
    })
}

/// Round a float to the nearest half float (diagnostic narrowing only).
fn debug_f32_to_half(value: f32) -> u16 {
    let sign = if value.is_sign_negative() { 0x8000u16 } else { 0 };
    let v = value.abs();
    if v.is_nan() { return sign | 0x7e00; }
    if v >= 65520.0 { return sign | 0x7c00; }
    if v < 6.103_515_6e-5 { return sign | (v / 5.960_464_5e-8).round() as u16; }
    let bits = v.to_bits();
    let exponent = ((bits >> 23) as i32 - 127 + 15) as u32;
    let mantissa = bits & 0x007f_ffff;
    let mut half = (exponent << 10) | (mantissa >> 13);
    if mantissa & 0x1000 != 0 { half += 1; }
    sign | half as u16
}

/// Copy only tightly packed original words. In particular, object IDs must never pass through
/// a half-float conversion: the dynamic/instanced tag bits are part of the geometry identity.
fn debug_read_light_target(
    device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture,
) -> anyhow::Result<Vec<u8>> {
    let bytes_per_pixel = match texture.format() {
        wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Float => 16,
        wgpu::TextureFormat::R32Uint => 4,
        format => anyhow::bail!("unsupported light diagnostic format: {format:?}"),
    };
    anyhow::ensure!(texture.usage().contains(wgpu::TextureUsages::COPY_SRC),
        "light-layer texture was not created with COPY_SRC");
    let row = texture.width() * bytes_per_pixel;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("light-layer-readback"), size: u64::from(padded) * u64::from(texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("light-layer-diagnostic-copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded),
                rows_per_image: Some(texture.height()) } },
        wgpu::Extent3d { width: texture.width(), height: texture.height(), depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = slice.get_mapped_range();
    let mut tight = Vec::with_capacity((row * texture.height()) as usize);
    for y in 0..texture.height() {
        let start = (y * padded) as usize;
        tight.extend_from_slice(&mapped[start..start + row as usize]);
    }
    drop(mapped); readback.unmap();
    Ok(tight)
}

fn make_target(
    device: &wgpu::Device,
    label: &str,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    extra: wgpu::TextureUsages,
) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | extra,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Upload one texture, or a 1×1 fallback texel when the mesh doesn't have that map. This is what
/// lets the mesh pipeline use ONE uniform bind-group layout for both PBR (Tiger) and non-PBR
/// (terrain, T-34, props, FX boxes) draws — no branching in the shader, just sampling a fallback
/// that's mathematically a no-op (white for base-color/metallic-roughness so factors pass through
/// unscaled, flat (128,128,255) for normal so the TBN perturbation resolves to the input normal).
#[path = "terrain_detail.rs"]
mod terrain_detail;

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    fallback: [u8; 4],
    img: &Option<CpuImage>,
) -> wgpu::TextureView {
    let (tw, th, texels): (u32, u32, Vec<u8>) = match img {
        Some(i) => (i.width, i.height, i.rgba.as_ref().clone()),
        None => (1, 1, fallback.to_vec()),
    };
    // Full mip chain, box-filtered on the CPU. Without mips, anything hi-res aliases hard under
    // minification through the 640×360 internal buffer — the 4k+ ground textures shimmered per
    // frame. Levels are picked by the material sampler with nearest min/mag, so the PSX texel
    // crunch survives; only the far-field noise goes.
    let mips = if tw >= 8 && th >= 8 {
        32 - tw.min(th).leading_zeros()
    } else {
        1
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: tw,
            height: th,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (mut lw, mut lh, mut level) = (tw, th, texels);
    for m in 0..mips {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: m,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(lw * 4),
                rows_per_image: Some(lh),
            },
            wgpu::Extent3d {
                width: lw,
                height: lh,
                depth_or_array_layers: 1,
            },
        );
        if m + 1 == mips {
            break;
        }
        // 2×2 box down to the next level (clamped on odd edges).
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            let (y0, y1) = ((y * 2).min(lh - 1), (y * 2 + 1).min(lh - 1));
            for x in 0..nw {
                let (x0, x1) = ((x * 2).min(lw - 1), (x * 2 + 1).min(lw - 1));
                for c in 0..4usize {
                    let s = level[((y0 * lw + x0) * 4) as usize + c] as u32
                        + level[((y0 * lw + x1) * 4) as usize + c] as u32
                        + level[((y1 * lw + x0) * 4) as usize + c] as u32
                        + level[((y1 * lw + x1) * 4) as usize + c] as u32;
                    next[((y * nw + x) * 4) as usize + c] = (s / 4) as u8;
                }
            }
        }
        (lw, lh, level) = (nw, nh, next);
    }
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ImageFingerprint {
    width: u32,
    height: u32,
    byte_len: usize,
    digest: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialFingerprint {
    albedo: Option<ImageFingerprint>,
    metallic_roughness: Option<ImageFingerprint>,
    normal: Option<ImageFingerprint>,
}

fn hash_cpu_image(image: &CpuImage) -> ImageFingerprint {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    image.rgba.as_slice().hash(&mut hasher);
    ImageFingerprint {
        width: image.width,
        height: image.height,
        byte_len: image.rgba.len(),
        digest: hasher.finish(),
    }
}

fn same_cpu_image(a: &Option<CpuImage>, b: &Option<CpuImage>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a.width == b.width
                && a.height == b.height
                && (Arc::ptr_eq(&a.rgba, &b.rgba) || a.rgba.as_slice() == b.rgba.as_slice())
        }
        (None, None) => true,
        _ => false,
    }
}

fn same_material_images(mesh: &CpuMesh, cached: &CachedMaterial) -> bool {
    same_cpu_image(&mesh.texture, &cached.albedo)
        && same_cpu_image(&mesh.metallic_roughness_texture, &cached.metallic_roughness)
        && same_cpu_image(&mesh.normal_texture, &cached.normal)
}

#[derive(Clone)]
struct CachedMaterial {
    albedo: Option<CpuImage>,
    metallic_roughness: Option<CpuImage>,
    normal: Option<CpuImage>,
    bind: Arc<wgpu::BindGroup>,
}

#[derive(Default)]
struct MaterialCache {
    entries: HashMap<MaterialFingerprint, Vec<CachedMaterial>>,
    /// Anatomical pages share their `Arc<Vec<u8>>`, so cache the content digest by allocation and
    /// hash a 1024² atlas once rather than once per limb. Independently loaded pose GLBs still meet
    /// in `entries` by exact bytes below.
    image_fingerprints: HashMap<(usize, u32, u32, usize), ImageFingerprint>,
}

impl MaterialCache {
    fn image_fingerprint(&mut self, image: Option<&CpuImage>) -> Option<ImageFingerprint> {
        let image = image?;
        let key = (
            Arc::as_ptr(&image.rgba) as usize,
            image.width,
            image.height,
            image.rgba.len(),
        );
        Some(
            *self
                .image_fingerprints
                .entry(key)
                .or_insert_with(|| hash_cpu_image(image)),
        )
    }

    fn material_fingerprint(&mut self, mesh: &CpuMesh) -> MaterialFingerprint {
        MaterialFingerprint {
            albedo: self.image_fingerprint(mesh.texture.as_ref()),
            metallic_roughness: self.image_fingerprint(mesh.metallic_roughness_texture.as_ref()),
            normal: self.image_fingerprint(mesh.normal_texture.as_ref()),
        }
    }

    fn bind_for(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        mesh: &CpuMesh,
    ) -> Arc<wgpu::BindGroup> {
        let fingerprint = self.material_fingerprint(mesh);
        if let Some(bind) = self
            .entries
            .get(&fingerprint)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| same_material_images(mesh, entry))
            })
            .map(|entry| Arc::clone(&entry.bind))
        {
            return bind;
        }

        let bind = Arc::new(upload_material(device, queue, bgl, sampler, mesh));
        self.entries
            .entry(fingerprint)
            .or_default()
            .push(CachedMaterial {
                albedo: mesh.texture.clone(),
                metallic_roughness: mesh.metallic_roughness_texture.clone(),
                normal: mesh.normal_texture.clone(),
                bind: Arc::clone(&bind),
            });
        bind
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MaterialSharingStats {
    pub mesh_materials: usize,
    pub unique_materials: usize,
    pub logical_gpu_bytes: u64,
    pub shared_gpu_bytes: u64,
}

fn uploaded_rgba_bytes(image: Option<&CpuImage>) -> u64 {
    let (mut width, mut height) = image.map_or((1, 1), |image| (image.width, image.height));
    let levels = if width >= 8 && height >= 8 {
        32 - width.min(height).leading_zeros()
    } else {
        1
    };
    let mut total = 0u64;
    for _ in 0..levels {
        total += u64::from(width) * u64::from(height) * 4;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    total
}

fn material_gpu_bytes(mesh: &CpuMesh) -> u64 {
    uploaded_rgba_bytes(mesh.texture.as_ref())
        + uploaded_rgba_bytes(mesh.metallic_roughness_texture.as_ref())
        + uploaded_rgba_bytes(mesh.normal_texture.as_ref())
}

/// Structural accounting for renderer material uploads. This uses the same content fingerprint and
/// collision-safe byte comparison as `MaterialCache`, but performs no GPU work, so asset tests can
/// pin the crew-page savings on every platform.
pub(crate) fn material_sharing_stats(scene: &[CpuMesh], slots: &[usize]) -> MaterialSharingStats {
    let mut stats = MaterialSharingStats::default();
    let mut unique: HashMap<MaterialFingerprint, Vec<usize>> = HashMap::new();
    for &slot in slots {
        let Some(mesh) = scene.get(slot) else {
            continue;
        };
        stats.mesh_materials += 1;
        stats.logical_gpu_bytes += material_gpu_bytes(mesh);
        let fingerprint = MaterialFingerprint {
            albedo: mesh.texture.as_ref().map(hash_cpu_image),
            metallic_roughness: mesh.metallic_roughness_texture.as_ref().map(hash_cpu_image),
            normal: mesh.normal_texture.as_ref().map(hash_cpu_image),
        };
        let already_present = unique.get(&fingerprint).is_some_and(|representatives| {
            representatives.iter().any(|&other_slot| {
                let other = &scene[other_slot];
                same_cpu_image(&mesh.texture, &other.texture)
                    && same_cpu_image(
                        &mesh.metallic_roughness_texture,
                        &other.metallic_roughness_texture,
                    )
                    && same_cpu_image(&mesh.normal_texture, &other.normal_texture)
            })
        });
        if !already_present {
            unique.entry(fingerprint).or_default().push(slot);
            stats.unique_materials += 1;
            stats.shared_gpu_bytes += material_gpu_bytes(mesh);
        }
    }
    stats
}

/// Base color is sRGB (glTF convention); metallic-roughness and normal maps are linear data, not
/// color, so they must NOT go through an sRGB-decoding texture format.
fn upload_material(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    mesh: &CpuMesh,
) -> wgpu::BindGroup {
    let albedo = upload_texture(
        device,
        queue,
        "albedo",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        [255, 255, 255, 255],
        &mesh.texture,
    );
    let mr = upload_texture(
        device,
        queue,
        "metallic-roughness",
        wgpu::TextureFormat::Rgba8Unorm,
        [255, 255, 255, 255],
        &mesh.metallic_roughness_texture,
    );
    let normal = upload_texture(
        device,
        queue,
        "normal",
        wgpu::TextureFormat::Rgba8Unorm,
        [128, 128, 255, 255],
        &mesh.normal_texture,
    );
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material-bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&albedo),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&mr),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&normal),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Upload one CPU mesh into a GPU-resident `MeshGpu` (vertex/index buffers + material bind group).
/// Shared by the scene-mesh upload in `Renderer::new` and the post-construction `load_menu_meshes`.
fn make_mesh_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material_bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    m: &CpuMesh,
    rt_on: bool,
    raster_only: bool,
    material_cache: &mut MaterialCache,
) -> MeshGpu {
    // M-D: BLAS builds read positions straight from the interleaved vertex buffer (Float32x3 at
    // offset 0, 48-byte stride) — BLAS_INPUT is only legal with the ray-query feature enabled.
    let participates_in_rt = rt_on && !raster_only;
    let bi = if participates_in_rt {
        wgpu::BufferUsages::BLAS_INPUT
    } else {
        wgpu::BufferUsages::empty()
    };
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vbuf"),
        contents: bytemuck::cast_slice(&m.vertices),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | bi,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ibuf"),
        contents: bytemuck::cast_slice(&m.indices),
        usage: wgpu::BufferUsages::INDEX | bi,
    });
    let tex_bind = material_cache.bind_for(device, queue, material_bgl, sampler, m);
    let shadow_pages = terrain_shadow_pages::prepare(m).map(|prepared| {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exact-terrain-shadow-pages"), contents: bytemuck::cast_slice(&prepared.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        (buffer, Arc::new(prepared.pages))
    });
    let rigid_pages = rigid_triangle_pages::prepare(m).map(|prepared| {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exact-rigid-triangle-pages"), contents: bytemuck::cast_slice(&prepared.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        (buffer, Arc::new(prepared.pages))
    });
    let visibility_occluder = sky_visibility::clustered_occluder(&m.vertices, &m.indices).map(|(vertices, indices)| {
        println!("[sky-visibility] clustered occluder: {} -> {} triangles ({:.1}%), max vertex error {:.1} mm",
            m.indices.len() / 3, indices.len() / 3, 100.0 * indices.len() as f32 / m.indices.len() as f32,
            sky_visibility::OCCLUDER_MAX_ERROR_M * 1000.0);
        (device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("visibility-occluder-vertices"), contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }), device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("visibility-occluder-indices"), contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        }), indices.len() as u32)
    });
    MeshGpu {
        visibility_occluder,
        visibility_geometry: None,
        visibility_source: (m.vertices.len() <= 4096).then(|| m.clone()),
        visibility_deformable: false,
        pose_buffer_private: false,
        optic_hidden: false,
        pose_revision: 0,
        vbuf,
        ibuf,
        index_count: m.indices.len() as u32,
        shadow_pages,
        rigid_pages,
        tex_bind,
        model: glam::Mat4::IDENTITY,
        visible: true,
        emissive: 0.0,
        emissive_win: false,
        emissive_own: false,
        snow_vehicle: false,
        instrument_cover: false,
        painted_steel: false,
        sky_visibility_excluded: false,
        small_arms_flash: false,
        legacy_small_arms_fx: false,
        raster_only,
        temporal_dynamic: false,
        glass: false,
        periscope_kind: 0,
        periscope_focal_m: 0.0,
        mirror_normal: glam::Vec3::ZERO,
        track_recv: false,
        metallic_factor: m.metallic_factor,
        roughness_factor: m.roughness_factor,
        local_min: m.min,
        local_max: m.max,
        world_min: m.min,
        world_max: m.max, // identity model → world == local until set_model
        blas: None,       // built in bulk by Renderer::new / add_instanced (never for menu meshes)
        rt_source: participates_in_rt.then(|| m.clone()),
    }
}

impl Renderer {
    /// Map casters are the same active scene selected by the forward mesh pass. A rotating
    /// menu model must not inherit the previous gameplay world's depth or source bounds.
    fn shadow_source_meshes(&self)->&[MeshGpu] {
        if self.menu_mode {&self.menu_meshes}else{&self.meshes}
    }
    pub(crate) fn lighting_visibility_receipt(&self)->serde_json::Value {
        if self.portable_visibility {
            serde_json::json!({"backend":"experimental_portable_source_triangles","experimental":true,
                "unsupported_optical_connections":"explicit magenta diagnostic; this is not a playable acceptance result"})
        } else {
            serde_json::json!({"backend":"source_geometry_shadow_maps","experimental":false,
                "point_sources":self.lights.len().min(N_LIGHTS),"point_shadow_capacity":N_LIGHTS,
                "point_near_m":0.005,"point_far":"per-source sphere enclosing loaded receivers, casters and camera far/medium support",
                "point_resolution_per_face":512,"environment":"32 local +32 source-bounds tiles, plus local/coarse actual-sun tiles",
                "environment_tile_resolution":384,"environment_cascade_bias":"0.35 local texel; 0.65 coarse texel",
                "caster_limits":"grass alpha cards and transient particles omitted; opaque terrain, armor, view-local bodies and mirror surfaces cast",
                "transmission_limits":"transparent glass/lenses pass straight light without Fresnel, absorption or refraction; mirrors block direct connections; no indirect reflected-light transport",
                "missing_coverage":"explicit diagnostic, never an unshadowed source fallback"})
        }
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, scene: &[CpuMesh]) -> Self {
        Self::new_with_raster_only(device, queue, scene, &[])
    }

    /// Construct the renderer while excluding known view-local meshes from RT at allocation time.
    /// Unlike `mark_raster_only`, this prevents BLAS-input buffers, exact CPU RT sources, BLASes and
    /// surface-atlas entries from ever being created for those slots.
    pub fn new_with_raster_only(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &[CpuMesh],
        raster_only_slots: &[usize],
    ) -> Self {
        Self::new_at_resolution(device, queue, scene, raster_only_slots, configured_internal_resolution())
    }

    fn new_at_resolution(
        device: &wgpu::Device, queue: &wgpu::Queue, scene: &[CpuMesh],
        raster_only_slots: &[usize], (iw, ih): (u32, u32),
    ) -> Self {
        let mut raster_only_mask = vec![false; scene.len()];
        for &slot in raster_only_slots {
            let value = raster_only_mask
                .get_mut(slot)
                .unwrap_or_else(|| panic!("raster-only scene slot {slot} is out of range"));
            *value = true;
        }
        if !raster_only_slots.is_empty() {
            let stats = material_sharing_stats(scene, raster_only_slots);
            println!(
                "[renderer] {} raster-only mesh materials share {} uploads: {:.1} MiB -> {:.1} MiB texture payload",
                stats.mesh_materials,
                stats.unique_materials,
                stats.logical_gpu_bytes as f64 / (1024.0 * 1024.0),
                stats.shared_gpu_bytes as f64 / (1024.0 * 1024.0),
            );
        }
        let graphics_preset = crate::graphics::active_preset();
        let shader_backend = crate::graphics::shader_backend_enabled();
        println!("[renderer] backend={} (ray queries {})", if crate::graphics::raymarch_backend_enabled() { "raymarch" } else if shader_backend { "shader" } else { "physical" }, if shader_backend && !crate::graphics::raymarch_ray_queries_wanted() { "disabled" } else { "adapter-dependent" });
        // Until a caller sets an act clock the sky is Stalingrad's solstice noon.
        let default_sky_clock = crate::celestial_sky::SkyClock::for_act(
            1, crate::celestial_sky::ClockPhase::Day);
        let default_celestial = crate::celestial_sky::CelestialLighting::at(
            default_sky_clock.now(), default_sky_clock.site,
            AtmosphereParameters::default().solar_irradiance);
        let transport_quality = TransportQuality::configured();
        println!("[renderer] transport quality {} ({TRANSPORT_QUALITY_ENV}); smoke steps {:?}, glass fog steps {}, surface passes {:?}",
            transport_quality.key(), transport_quality.smoke_steps(),
            transport_quality.glass_world_steps(), transport_quality.surface_passes());
        let shadow_res = configured_shadow_resolution(graphics_preset);
        let aspect = iw as f32 / ih as f32;
        // One Vulkan cache shares compilation products between the four physical-transport
        // pipelines and is atomically persisted after the last one succeeds. A cold shader edit
        // still compiles honestly; subsequent screenshots reuse the driver's exact machine code.
        let (pipeline_cache, pipeline_cache_path) = create_physical_pipeline_cache(device);

        let hdr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("hdr"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: hdr_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC, // RTT periscope tiles crop out of this
            view_formats: &[],
        });
        let hdr_view = hdr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // TEXTURE_BINDING so the volumetric smoke pass can sample scene depth back for occlusion.
        // Kept as a texture (not just its view): the S1 bvhview PASS metric reads scene depth
        // back and correlates it against traced primary-ray depth.
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // LDR keeps its raw texture handle (not just the view): the gunner EYEPIECE COMPOSITE
        // snapshots the crushed optic frame out of it between the optic and interior passes.
        let ldr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ldr"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let ldr_view = ldr_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // The snapshot target: last optic frame, composited into the interior view's eyepiece
        // circle by crush.wgsl mask mode 4.
        let optic_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("optic-snapshot"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let optic_view = optic_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Nearest mag/min = the PSX texel crunch stays; the LINEAR mipmap filter only blends
        // between the mip levels upload_texture now generates, so hi-res ground textures stop
        // shimmering at distance through the 640×360 crush (minification aliasing, not look).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });

        let camera = Camera::new(aspect);
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bgl = camera_bind_group_layout(device);
        let terrain_detail_view = terrain_detail::upload(device, queue);
        let terrain_detail_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-area-filter"),
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind"),
            layout: &camera_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: camera_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&terrain_detail_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&terrain_detail_sampler) },
            ],
        });
        let near_shadow_camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("near-shadow-camera"), size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let near_shadow_camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("near-shadow-camera-bind"), layout: &camera_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: near_shadow_camera_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&terrain_detail_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&terrain_detail_sampler) },
            ],
        });
        let near_shadow_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("near-shadow-sampling"), size: std::mem::size_of::<NearShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let physical_environment_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("physical-environment"),
            size: std::mem::size_of::<GpuPhysicalEnvironment>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cabin_medium=std::sync::Arc::new(cabin_medium::Medium::load().expect("source Tiger atmosphere domains"));
        let medium_size=cabin_medium.texture_size();
        let cabin_medium_texture=device.create_texture(&wgpu::TextureDescriptor {
            label:Some("source-cabin-air-columns"),size:wgpu::Extent3d {width:medium_size[0],height:medium_size[1],depth_or_array_layers:medium_size[2]},
            mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Rg32Float,
            usage:wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST,view_formats:&[],
        });
        queue.write_texture(wgpu::TexelCopyTextureInfo {texture:&cabin_medium_texture,mip_level:0,
            origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},bytemuck::cast_slice(cabin_medium.texture_data()),
            wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(medium_size[0]*8),rows_per_image:Some(medium_size[1])},
            cabin_medium_texture.size());
        let cabin_medium_view=cabin_medium_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension:Some(wgpu::TextureViewDimension::D2Array),..Default::default()});
        let physical_environment_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("physical-environment-bgl"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT),cabin_medium_texture_entry(1)],
            });
        let physical_environment_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("physical-environment-bind"),
            layout: &physical_environment_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: physical_environment_buf.as_entire_binding(),
            },wgpu::BindGroupEntry {binding:1,resource:wgpu::BindingResource::TextureView(&cabin_medium_view)}],
        });

        // binding 0 = base color (sRGB), 1 = metallic-roughness (linear, G=rough/B=metal),
        // 2 = tangent-space normal map (linear), 3 = shared sampler. Every mesh binds all three —
        // non-PBR meshes get 1×1 fallback textures (see `upload_texture`) so this layout, and the
        // mesh pipeline built from it, is uniform across every draw call in the scene.
        let material_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material-bgl"),
            entries: &[tex_entry(0), tex_entry(1), tex_entry(2), sampler_entry(3)],
        });

        // group(2) for the mesh pass: the sun shadow depth map + a comparison sampler (hardware PCF).
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-bgl"),
            entries: &[
                // The physical environment probe (environment_probe.rs) for raster lighting.
                uniform_entry(8, wgpu::ShaderStages::FRAGMENT),
                cabin_medium_texture_entry(11),
                crate::scene_visibility_gpu::SceneVisibilityGpu::layout_entries(9,10,wgpu::ShaderStages::FRAGMENT)[0],
                crate::scene_visibility_gpu::SceneVisibilityGpu::layout_entries(9,10,wgpu::ShaderStages::FRAGMENT)[1],
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
                },
                uniform_entry(3, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 4, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::CubeArray, multisampled: false }, count: None,
                },
                uniform_entry(5, wgpu::ShaderStages::FRAGMENT),
                // Near-field sky/ground visibility atlas and its per-direction matrices.
                wgpu::BindGroupLayoutEntry {
                    binding: 6, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
                },
                uniform_entry(7, wgpu::ShaderStages::FRAGMENT),
            ],
        });

        // upload every scene mesh
        // M-D: hardware ray tracing rides along whenever the device came up with the feature
        // (request_device auto-enables it on RT-class adapters). KESSEL_NO_RT=1 is the A/B and
        // benchmark kill-switch.
        let rt_on = device
            .features()
            .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            && crate::graphics::hardware_rt_enabled();
        // The scene's acceleration structures: built for the physical integrator (`rt_on`) and
        // for the raymarch backend's ray queries, which leave the physical deferred path off.
        let rt_scene = device.features().contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
            && (rt_on || crate::graphics::raymarch_ray_queries_wanted());
        // S4: the deferred combine IS the ray integrator — no RT, no deferred (Low tier
        // = the forward path).
        let deferred = rt_on
            && std::env::var("KESSEL_FORWARD")
                .map(|v| v != "1")
                .unwrap_or(true);
        // Raster surface lighting (the shader backend, and the physical backend without ray
        // queries) takes the physical environment through the visibility atlas and the probe.
        let raster_lighting = shader_backend || !deferred;
        let portable_visibility = portable_visibility_enabled();
        println!("[visibility] {}", if portable_visibility { "experimental portable source triangles" } else { "source geometry shadow maps" });

        let mut scene_visibility = crate::scene_visibility::SceneVisibility::new();
        let mut scene_visibility_gpu = crate::scene_visibility_gpu::SceneVisibilityGpu::new(device);
        let mut material_cache = MaterialCache::default();
        let mut meshes: Vec<MeshGpu> = scene
            .iter()
            .enumerate()
            .map(|(slot, m)| {
                let mut gpu = make_mesh_gpu(
                    device,
                    queue,
                    &material_bgl,
                    &sampler,
                    m,
                    rt_scene,
                    raster_only_mask[slot],
                    &mut material_cache,
                );
                if raster_lighting && portable_visibility && !m.indices.is_empty() {
                    gpu.visibility_geometry = Some(scene_visibility.register_mesh(m).expect("invalid source visibility mesh"));
                }
                gpu
            })
            .collect();
        scene_visibility_gpu.prepare(device,queue,&scene_visibility).expect("source visibility upload");

        // M-D: one BLAS per scene mesh, all built in bulk at load (the world is rigid — these
        // never rebuild; TLAS membership per frame is the only steady-state RT work). Chunked
        // submits keep any single scratch allocation modest.
        if rt_scene {
            let t0 = std::time::Instant::now();
            for (m, src) in meshes.iter_mut().zip(scene.iter()) {
                if !m.raster_only && m.index_count >= 3 && !src.vertices.is_empty() {
                    m.blas = Some(crate::rt::make_blas(
                        device,
                        src.vertices.len() as u32,
                        m.index_count,
                    ));
                }
            }
            let mut built = 0usize;
            for chunk in meshes.chunks(64) {
                let entries: Vec<wgpu::BlasBuildEntry> = chunk
                    .iter()
                    .filter_map(|m| {
                        m.blas
                            .as_ref()
                            .map(|b| crate::rt::blas_entry(b, &m.vbuf, &m.ibuf))
                    })
                    .collect();
                if entries.is_empty() {
                    continue;
                }
                built += entries.len();
                let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("blas-build"),
                });
                enc.build_acceleration_structures(
                    entries.iter(),
                    std::iter::empty::<&wgpu::Tlas>(),
                );
                queue.submit([enc.finish()]);
            }
            println!(
                "[rt] {} scene BLASes built in {:.0} ms",
                built,
                t0.elapsed().as_secs_f32() * 1000.0
            );
        }

        // ---------- pipelines ----------
        let physical_compile_start = std::time::Instant::now();
        println!("[renderer] composing sky shader module");
        // Both backends show the same physical sky: the atmosphere lit by the ephemeris' Sun and
        // Moon, with the night-sky layer; the visible sky and every surface share its units.
        let sky_source = physical_atmosphere_transport_shader_source(include_str!("../shaders/sky.wgsl"));
        let sky_shader = shader(device, "sky-physical-environment", &sky_source);
        // One raster surface shader for both backends: the forward physical path (no ray
        // queries, and the physical menu/editor) and the shader backend share its physical
        // environment, beam and emitters; the deferred ray-traced path uses gbuf + combine.
        let mesh_shader = shader(device, "mesh", include_str!("../shaders/stylized_mesh.wgsl"));
        let shadow_shader = shader(device, "shadow", include_str!("../shaders/shadow.wgsl"));
        let bloom_shader = shader(device, "bloom", if shader_backend { include_str!("../shaders/stylized_bloom.wgsl") } else { include_str!("../shaders/bloom.wgsl") });
        let crush_shader = shader(device, "crush", &observer_shader_source(include_str!("../shaders/crush.wgsl")));
        let blit_shader = shader(device, "blit", include_str!("../shaders/blit.wgsl"));
        println!("[renderer] sky module accepted in {:.0} ms ({} KiB)",
            physical_compile_start.elapsed().as_secs_f32()*1000.0, sky_source.len()/1024);
        // The sky pass now reads the camera uniform (for the `night` factor) so the backdrop can
        // deepen to a dark cool night sky; it binds group(0) = camera, like the mesh pass.
        let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-pl"),
            bind_group_layouts: &[&camera_bgl, &physical_environment_bgl],
            push_constant_ranges: &[],
        });
        let sky_compile_start = std::time::Instant::now();
        println!("[renderer] compiling sky pipeline");
        let sky_pipe =
            fullscreen_pipe_cached(device, &sky_shader, &sky_pl, hdr_format(), pipeline_cache.as_ref());
        let sky_depth_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky-depth-gated"),
            layout: Some(&sky_pl),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Equal,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format()))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: pipeline_cache.as_ref(),
        });
        println!(
            "[renderer] sky pipeline ready in {:.0} ms",
            sky_compile_start.elapsed().as_secs_f32() * 1000.0,
        );

        // ---------- tank-track map: fixed-resolution sparse R8 material history ----------
        const TRACK_RES: u32 = track_ruts::ATLAS_RES;
        let track_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("track-map"),
            size: wgpu::Extent3d {
                width: TRACK_RES,
                height: TRACK_RES,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let track_view = track_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let track_lookup = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("track-page-table"),
            size: wgpu::Extent3d { width: track_ruts::LOOKUP_RES, height: track_ruts::LOOKUP_RES,
                depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let track_lookup_view = track_lookup.create_view(&Default::default());
        let track_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("track-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false,
                    }, count: None,
                },
            ],
        });
        let track_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("track-samp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let track_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("track-bind"),
            layout: &track_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&track_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&track_samp),
                },
                wgpu::BindGroupEntry { binding: 2,
                    resource: wgpu::BindingResource::TextureView(&track_lookup_view) },
            ],
        });
        let track_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("track.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/track.wgsl").into()),
        });
        let track_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("track-pl"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let track_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("track-stamp-pipe"),
            layout: Some(&track_pl),
            vertex: wgpu::VertexState {
                module: &track_shader, entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &track_shader, entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::RED,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let mesh_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh-pl"),
            bind_group_layouts: &[&camera_bgl, &material_bgl, &shadow_bgl, &track_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let vertex_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 24,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let mesh_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format()))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- sun shadow map ----------
        // Depth-only cascade rendered from the sun's POV. RENDER_ATTACHMENT to draw into,
        // TEXTURE_BINDING so the mesh pass can sample it back.
        let shadow_view = make_target(
            device,
            "shadow-map",
            shadow_res,
            shadow_res,
            DEPTH,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let near_shadow_res = if raster_lighting { shadow_res.max(1024) } else { 1 };
        let near_shadow_view = make_target(device,"near-shadow-map",near_shadow_res,near_shadow_res,
            DEPTH,wgpu::TextureUsages::TEXTURE_BINDING);
        // Comparison sampler: LessEqual → textureSampleCompare returns the LIT fraction (ref depth
        // ≤ stored nearest depth). Clamp-to-edge so off-map lookups don't wrap. Filtering=Linear
        // gives the sampler its own 2×2 PCF, which our 3×3 kernel widens.
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-cmp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let cabin_shadows = cabin_shadows::CabinShadows::new(device,!portable_visibility,&camera_bgl,
            &terrain_detail_view,&terrain_detail_sampler,&shadow_sampler,&scene_visibility_gpu);
        let environment_probe = environment_probe::EnvironmentProbe::new(device, &physical_environment_buf);
        let sky_visibility = sky_visibility::SkyVisibility::new(device, !portable_visibility,
            &environment_probe.uniform);
        let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-bind"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&near_shadow_view) },
                wgpu::BindGroupEntry { binding: 3, resource: near_shadow_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&cabin_shadows.view) },
                wgpu::BindGroupEntry { binding: 5, resource: cabin_shadows.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&sky_visibility.view) },
                wgpu::BindGroupEntry { binding: 7, resource: sky_visibility.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 8, resource: environment_probe.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding:11,resource:wgpu::BindingResource::TextureView(&cabin_medium_view) },
                wgpu::BindGroupEntry { binding:9,resource:scene_visibility_gpu.geometry_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding:10,resource:scene_visibility_gpu.instance_buffer().as_entire_binding() },
            ],
        });
        // Shadow pipeline: reuses the camera uniform (for light_view_proj) + the same 80-byte
        // PushData range (only `model` is read). Vertex-only, so no fragment/color state.
        let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow-pl"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX,
                range: 0..std::mem::size_of::<PushData>() as u32,
            }],
        });
        let shadow_pos_attr = [wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }];
        let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &shadow_pos_attr,
                }],
                compilation_options: Default::default(),
            },
            // Thin cabin steel, handles and one-sided authored surfaces still cast shadows.
            // The stylized path stores the nearest surface rather than the caster's far face.
            primitive: wgpu::PrimitiveState {
                cull_mode: if shader_backend { None } else { Some(wgpu::Face::Front) },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                // Small constant + slope depth bias, a cheap backup to the shader-side normal bias.
                bias: wgpu::DepthBiasState {
                    constant: if shader_backend { 0 } else { 2 },
                    slope_scale: if shader_backend { 0.0 } else { 2.0 },
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview: None,
            cache: None,
        });

        // ---------- instanced-static pipelines ----------
        // Same layouts as the mesh/shadow pipelines, plus an instance-rate buffer carrying the
        // per-placement model matrix as four vec4 columns (locations 4-7 → mesh/shadow vs_inst).
        // One draw call renders every placement of a repeated module — the act-scale enabler.
        let inst_attrs = [
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 5,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 32,
                shader_location: 6,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: 48,
                shader_location: 7,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let inst_layout = wgpu::VertexBufferLayout {
            array_stride: 64, // one mat4 per instance
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &inst_attrs,
        };
        let mesh_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh-inst"),
            layout: Some(&mesh_pl),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    },
                    inst_layout.clone(),
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format()))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        // ---------- S2: deferred G-buffer + combine (the M-D pipeline shape) ----------
        // The forward mesh pipes above stay intact for the menu/editor paths and as the parity
        // fallback (`deferred = false`); the game path renders G-buffer MRT then shades it in
        // one fullscreen combine that reproduces mesh.wgsl exactly (rt-parity gate).
        let gbuf_shader = shader(device, "gbuf", include_str!("../shaders/gbuf.wgsl"));
        // G0 must hold the POST-LIFT albedo (pow(raw,0.65)*1.5 — up to 1.5, HDR-ish) losslessly;
        // Rgba8Unorm clamped it at 1.0 and the parity gate caught the darkening immediately.
        // Drops back to 8-bit once the S-calib slice moves the lift to texture-load time.
        const G0F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        let G1F: wgpu::TextureFormat = hdr_format();
        let G2F: wgpu::TextureFormat = hdr_format();
        const G3F: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
        let g0_view = make_target(
            device,
            "g0-albedo",
            iw,
            ih,
            G0F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let g1_view = make_target(
            device,
            "g1-normal",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let g2_view = make_target(
            device,
            "g2-emissive",
            iw,
            ih,
            G2F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        // G3 keeps its texture handle: the id buffer is copied into prev_id each frame for the
        // temporal reprojection reject.
        let g3_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("g3-id"),
            size: wgpu::Extent3d {
                width: iw,
                height: ih,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: G3F,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let g3_view = g3_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let gbuf_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gbuf-pl"),
            bind_group_layouts: &[&camera_bgl, &material_bgl, &shadow_bgl, &track_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::VERTEX_FRAGMENT,
                range: 0..std::mem::size_of::<GbufPush>() as u32,
            }],
        });
        let gbuf_targets = [
            Some(color_target(G0F)),
            Some(color_target(G1F)),
            Some(color_target(G2F)),
            Some(color_target(G3F)),
        ];
        let gbuf_depth = wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let gbuf_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuf"),
            layout: Some(&gbuf_pl),
            vertex: wgpu::VertexState {
                module: &gbuf_shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(gbuf_depth.clone()),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gbuf_shader,
                entry_point: Some("fs"),
                targets: &gbuf_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let gbuf_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gbuf-inst"),
            layout: Some(&gbuf_pl),
            vertex: wgpu::VertexState {
                module: &gbuf_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    },
                    inst_layout.clone(),
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(gbuf_depth),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &gbuf_shader,
                entry_point: Some("fs"),
                targets: &gbuf_targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let gbuf_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gbuf-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let gbuf_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gbuf-bind"),
            layout: &gbuf_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&g0_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&g1_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&g2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&g3_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
            ],
        });
        // S4: combine group(2) is the scene TLAS (fragment-stage ray queries). wgpu 27's Metal
        // backend does not expose EXPERIMENTAL_RAY_QUERY. Creating either this layout or the WGSL
        // module without the feature is itself a validation error, even though the forward path
        // never dispatches them, so keep the whole ray-only graph behind the capability gate.
        let (rt_as_bgl, combine_pipe) = if rt_on {
            let combine_source = physical_atmosphere_transport_shader_source(include_str!(
                "../shaders/combine.wgsl",
            ));
            let combine_shader = shader(device, "combine-physical-environment", &combine_source);
            let rt_as_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("rt-as-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::AccelerationStructure {
                            vertex_return: false,
                        },
                        count: None,
                    },
                    // Exact vertex halves, semantic triangle/surface/material/texture records,
                    // flattened atlas texels, and per-instance emission state respectively. The
                    // split keeps every truthful array below Vulkan's per-binding byte ceiling.
                    storage_entry(1, wgpu::ShaderStages::FRAGMENT, true),
                    storage_entry(2, wgpu::ShaderStages::FRAGMENT, true),
                    storage_entry(3, wgpu::ShaderStages::FRAGMENT, true),
                    storage_entry(4, wgpu::ShaderStages::FRAGMENT, true),
                    storage_entry(5, wgpu::ShaderStages::FRAGMENT, true),
                    uniform_entry(6, wgpu::ShaderStages::FRAGMENT),
                ],
            });
            let combine_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("combine-pl"),
                bind_group_layouts: &[
                    &camera_bgl,
                    &gbuf_bgl,
                    &rt_as_bgl,
                    &physical_environment_bgl,
                ],
                push_constant_ranges: &[],
            });
            let combine_compile_start = std::time::Instant::now();
            println!(
                "[renderer] compiling exact surface transport pipeline ({} KiB)",
                combine_source.len() / 1024,
            );
            let combine_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("combine"),
                layout: Some(&combine_pl),
                vertex: wgpu::VertexState {
                    module: &combine_shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &combine_shader,
                    entry_point: Some("fs"),
                    // S3: diffuse and specular receive independent temporal carriers; authored
                    // emission stays current-frame so a penetration flash never lags.
                    targets: &[
                        Some(color_target(G1F)),
                        Some(color_target(G1F)),
                        Some(color_target(G1F)),
                    ],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: pipeline_cache.as_ref(),
            });
            println!(
                "[renderer] exact surface transport pipeline ready in {:.0} ms",
                combine_compile_start.elapsed().as_secs_f32() * 1000.0,
            );
            println!("[renderer] ray-query pipeline enabled; deferred lighting available");
            (Some(rt_as_bgl), Some(combine_pipe))
        } else {
            println!(
                "[renderer] ray query unavailable or disabled; using forward lighting without creating ray-query resources"
            );
            (None, None)
        };
        debug_assert_eq!(rt_as_bgl.is_some(), rt_on);
        debug_assert_eq!(combine_pipe.is_some(), rt_on);

        // ---------- S3: temporal accumulation (accum.wgsl compute) + resolve ----------
        let accum_shader = shader(device, "accum", &hdr_storage_source(include_str!("../shaders/accum.wgsl")));
        let resolve_shader = shader(device, "resolve", include_str!("../shaders/resolve.wgsl"));
        let lit_diff_view = make_target(
            device,
            "lit-diff",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let lit_other_view = make_target(
            device,
            "lit-specular",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let lit_emission_view = make_target(
            device,
            "lit-immediate-emission",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        // The volume integrator must not composite its stochastic one-sample estimate straight
        // into HDR: doing so bypassed S3 and produced frame-wide salt-and-pepper with retro=0.
        // It first writes this raw premultiplied transport target, then High converges source
        // radiance in each view's history while preserving the current Beer-Lambert opacity.
        let volume_current_view = make_target(
            device,
            "volume-current",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        );
        let mk_accum = |label: &str| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: G1F,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let accum_ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("accum-uniform"),
            size: std::mem::size_of::<AccumUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let delta_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("accum-deltas"),
            size: (meshes.len().max(1) * 64) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ce = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let float_tex = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let depth_ty = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let uint_tex = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let accum_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("accum-bgl"),
            entries: &[
                ce(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                ce(1, float_tex), // lit_diff
                ce(2, depth_ty),  // depth
                ce(3, uint_tex),  // id
                ce(4, float_tex), // history
                ce(5, depth_ty),  // prev depth
                ce(6, uint_tex),  // prev id
                ce(8, float_tex), // current geometric normal / roughness
                ce(
                    7,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: G1F,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
            ],
        });
        let delta_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("accum-delta-bgl"),
            entries: &[ce(
                0,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            )],
        });
        let accum_delta_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("accum-delta-bind"),
            layout: &delta_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: delta_buf.as_entire_binding(),
            }],
        });
        let accum_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("accum-pl"),
            bind_group_layouts: &[&accum_bgl, &delta_bgl],
            push_constant_ranges: &[],
        });
        let accum_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("accum"),
            layout: Some(&accum_pl),
            module: &accum_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let volume_accum_shader = shader(
            device,
            "volume-accum",
            &hdr_storage_source(include_str!("../shaders/volume_accum.wgsl")),
        );
        let volume_accum_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("volume-accum"),
            layout: Some(&accum_pl),
            module: &volume_accum_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        let fe = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let resolve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("resolve-bgl"),
            entries: &[
                fe(0, float_tex), // accumulated diffuse
                fe(1, float_tex), // accumulated specular
                fe(2, float_tex), // current-frame authored emission
                fe(3, float_tex), // g0 albedo+flag
                fe(4, depth_ty),
            ],
        });
        let volume_composite_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("volume-composite-bgl"),
                entries: &[fe(0, float_tex)],
            });
        let volume_current_composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("volume-current-composite-bind"),
            layout: &volume_composite_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&volume_current_view),
            }],
        });
        let surface_reconstruction = (rt_on && surface_reconstruction::requested())
            .then(|| surface_reconstruction::Gpu::new(device, iw, ih));
        if surface_reconstruction.is_some() {
            println!("[renderer] optional diffuse spatial reconstruction enabled (two passes; original history retained)");
        }
        let live_texel_probe = (rt_on && live_texel_probe::requested()).then(|| {
            live_texel_probe::Gpu::new(device, iw, ih, &camera_bgl, &gbuf_bgl,
                &terrain_detail_view, &terrain_detail_sampler, live_texel_probe::splats_requested(),
                live_texel_probe::temporal_requested())
        });
        if live_texel_probe.is_some() {
            let presentation = if live_texel_probe::splats_requested() {"source-face diffuse texel cells"}
                else {"validated diffuse gather"};
            println!("[renderer] experimental live cabin probe enabled: one 64x64 face/frame; {presentation}; original eye visibility and other carriers retained");
        }
        // Per-view S3 contexts (splitscreen): each viewport owns its history ping-pong, its
        // reject snapshots (prev depth/id) and its previous-camera record. Solo uses views[0].
        let views: Vec<ViewCtx> = (0..MAX_VIEWS)
            .map(|vi| {
                let hist = [
                    mk_accum(&format!("accum-a-v{vi}")),
                    mk_accum(&format!("accum-b-v{vi}")),
                ];
                let spec_hist = [
                    mk_accum(&format!("spec-accum-a-v{vi}")),
                    mk_accum(&format!("spec-accum-b-v{vi}")),
                ];
                let volume_hist = [
                    mk_accum(&format!("volume-accum-a-v{vi}")),
                    mk_accum(&format!("volume-accum-b-v{vi}")),
                ];
                let prev_depth_tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("prev-depth"),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let prev_depth_view =
                    prev_depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
                let prev_id_tex = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("prev-id"),
                    size: wgpu::Extent3d {
                        width: iw,
                        height: ih,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: G3F,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let prev_id_view = prev_id_tex.create_view(&wgpu::TextureViewDescriptor::default());
                let mk_accum_bind = |out: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("accum-bind"),
                        layout: &accum_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accum_ubuf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&lit_diff_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&g3_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&hist[1 - out]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(&prev_depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(&prev_id_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 7,
                                resource: wgpu::BindingResource::TextureView(&hist[out]),
                            },
                            wgpu::BindGroupEntry { binding: 8,
                                resource: wgpu::BindingResource::TextureView(&g1_view) },
                        ],
                    })
                };
                let accum_binds = [mk_accum_bind(0), mk_accum_bind(1)];
                let mk_spec_accum_bind = |out: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("spec-accum-bind"),
                        layout: &accum_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accum_ubuf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&lit_other_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&g3_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&spec_hist[1 - out]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(&prev_depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(&prev_id_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 7,
                                resource: wgpu::BindingResource::TextureView(&spec_hist[out]),
                            },
                            wgpu::BindGroupEntry { binding: 8,
                                resource: wgpu::BindingResource::TextureView(&g1_view) },
                        ],
                    })
                };
                let spec_accum_binds = [mk_spec_accum_bind(0), mk_spec_accum_bind(1)];
                let mk_volume_accum_bind = |out: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("volume-accum-bind"),
                        layout: &accum_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: accum_ubuf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&volume_current_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&g3_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&volume_hist[1 - out]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(&prev_depth_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(&prev_id_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 7,
                                resource: wgpu::BindingResource::TextureView(&volume_hist[out]),
                            },
                            wgpu::BindGroupEntry { binding: 8,
                                resource: wgpu::BindingResource::TextureView(&g1_view) },
                        ],
                    })
                };
                let volume_accum_binds = [mk_volume_accum_bind(0), mk_volume_accum_bind(1)];
                let mk_resolve_bind = |src: usize, diffuse: &wgpu::TextureView| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("resolve-bind"),
                        layout: &resolve_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(diffuse),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&spec_hist[src]),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&lit_emission_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(&g0_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(&depth_view),
                            },
                        ],
                    })
                };
                let resolve_binds = [mk_resolve_bind(0, &hist[0]), mk_resolve_bind(1, &hist[1])];
                let reconstruction_binds = surface_reconstruction.as_ref().map(|gpu| {
                    gpu.bindings(device, surface_reconstruction::Inputs {
                        temporal: &hist, current: &lit_diff_view, normal: &g1_view,
                        depth: &depth_view, id: &g3_view,
                    })
                });
                let reconstruction_resolve_binds = surface_reconstruction.as_ref().map(|gpu| {
                    [mk_resolve_bind(0, gpu.output()), mk_resolve_bind(1, gpu.output())]
                });
                let probe_binds = live_texel_probe.as_ref().map(|gpu| {
                    gpu.bindings(device,
                    live_texel_probe::Inputs {
                        temporal: &hist, reconstructed: surface_reconstruction.as_ref().map(|r| r.output()),
                        g0: &g0_view, g1: &g1_view, g2: &g2_view, depth: &depth_view, id: &g3_view,
                    })
                });
                let probe_resolve_binds = live_texel_probe.as_ref().map(|gpu| {
                    [mk_resolve_bind(0, gpu.output()), mk_resolve_bind(1, gpu.output())]
                });
                let mk_volume_composite_bind = |src: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("volume-composite-bind"),
                        layout: &volume_composite_bgl,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&volume_hist[src]),
                        }],
                    })
                };
                let volume_composite_binds =
                    [mk_volume_composite_bind(0), mk_volume_composite_bind(1)];
                ViewCtx {
                    debug_diff_history: hist,
                    debug_spec_history: spec_hist,
                    debug_volume_history: volume_hist,
                    accum_binds,
                    spec_accum_binds,
                    resolve_binds,
                    reconstruction_binds,
                    reconstruction_resolve_binds,
                    probe_binds, probe_resolve_binds,
                    volume_accum_binds,
                    volume_composite_binds,
                    prev_depth_tex,
                    prev_id_tex,
                    accum_flip: 0,
                    prev_vp: glam::Mat4::IDENTITY,
                    prev_models: Vec::new(),
                    prev_cam_sig: (glam::Vec3::ZERO, glam::Vec3::Z, 0.0, 1.0),
                    prev_depth_range: [0.0, 0.0],
                    prev_light_sources: None,
                    history_valid: false,
                    volume_history_valid: false,
                }
            })
            .collect();
        let resolve_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("resolve-pl"),
            bind_group_layouts: &[&camera_bgl, &resolve_bgl],
            push_constant_ranges: &[],
        });
        let resolve_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("resolve"),
            layout: Some(&resolve_pl),
            vertex: wgpu::VertexState {
                module: &resolve_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &resolve_shader,
                entry_point: Some("fs"),
                targets: &[Some(color_target(hdr_format()))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- the eye: meter, observer state and display mapping ----------
        let exp_shader = shader(device, "exposure", &observer_shader_source(include_str!("../shaders/exposure.wgsl")));
        let exp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exposure-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(EXPOSURE_CLOCK_UNIFORM_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let exp_clock_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("exposure-clock"),
            contents: bytemuck::bytes_of(&ExposureClockUniform {
                time_s: 0.0,
                fov_y: 1.0,
                aspect: 16.0 / 9.0,
                monocular: 0.0,
                transient_source: 0.0,
                _pad: [0.0;3],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut exp_bufs = Vec::with_capacity(MAX_VIEWS);
        let mut exp_binds = Vec::with_capacity(MAX_VIEWS);
        for view in 0..MAX_VIEWS {
            let label = format!("exposure-state-v{view}");
            let exp_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&label),
                size: (EYE_STATE_LANES * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(
                &exp_buf,
                0,
                bytemuck::cast_slice(&uninitialized_exposure_state()),
            );
            let bind_label = format!("exposure-bind-v{view}");
            let exp_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&bind_label),
                layout: &exp_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&hdr_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: exp_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: exp_clock_buf.as_entire_binding(),
                    },
                ],
            });
            exp_bufs.push(exp_buf);
            exp_binds.push(exp_bind);
        }
        let exp_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exposure-pl"),
            bind_group_layouts: &[&exp_bgl],
            push_constant_ranges: &[],
        });
        let exp_pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("exposure"),
            layout: Some(&exp_pl),
            module: &exp_shader,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });

        let shadow_inst_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-inst"),
            layout: Some(&shadow_pl),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_inst"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &shadow_pos_attr,
                    },
                    inst_layout,
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: if shader_backend { None } else { Some(wgpu::Face::Front) },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: if shader_backend { 0 } else { 2 },
                    slope_scale: if shader_backend { 0.0 } else { 2.0 },
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview: None,
            cache: None,
        });

        // Shared GPU camera response for bloom and final tonemapping.
        let crush_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crush"),
            size: std::mem::size_of::<CrushUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let raster_ssao = shader_backend.then(|| {
            raster_ssao::RasterSsao::new(device, iw, ih, &depth_view, &mesh_pl, &mesh_shader)
        });

        // ---------- emissive bloom ----------
        let bw = (iw / 2).max(1);
        let bh = (ih / 2).max(1);
        let bloom_a_view = make_target(
            device,
            "bloom-a",
            bw,
            bh,
            hdr_format(),
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let bloom_b_view = make_target(
            device,
            "bloom-b",
            bw,
            bh,
            hdr_format(),
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        // Bilinear-clamp sampler for the bright pass / blur taps / half-res upsample.
        let bloom_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bloom-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut bloom_entries = vec![tex_entry(0), sampler_entry(1), uniform_entry(2, wgpu::ShaderStages::FRAGMENT)];
        let bloom_portal_buf = shader_backend.then(|| device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bloom-portal-view"),
            size: std::mem::size_of::<BloomPortalUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        if shader_backend {
            bloom_entries.push(tex_entry(3));
            bloom_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 4, visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            });
            bloom_entries.push(uniform_entry(5, wgpu::ShaderStages::FRAGMENT));
        }
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
            entries: &bloom_entries,
        });
        let make_bloom_bind = |view: &wgpu::TextureView, label: &str| {
            let mut entries = vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&bloom_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: crush_buf.as_entire_binding(),
                    },
                ];
            if let Some(ao)=&raster_ssao { entries.push(wgpu::BindGroupEntry {binding:3,resource:wgpu::BindingResource::TextureView(&ao.ambient)}); }
            if let Some(portal) = &bloom_portal_buf {
                entries.push(wgpu::BindGroupEntry {binding:4,resource:wgpu::BindingResource::TextureView(&depth_view)});
                entries.push(wgpu::BindGroupEntry {binding:5,resource:portal.as_entire_binding()});
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bloom_bgl,
                entries: &entries,
            })
        };
        let bloom_bind_hdr = make_bloom_bind(&hdr_view, "bloom-bind-hdr");
        let bloom_bind_a = make_bloom_bind(&bloom_a_view, "bloom-bind-a");
        let bloom_bind_b = make_bloom_bind(&bloom_b_view, "bloom-bind-b");
        let bloom_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-pl"),
            bind_group_layouts: &[&bloom_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..std::mem::size_of::<BloomPush>() as u32,
            }],
        });
        let bloom_bright_pipe = bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_bright", None);
        let bloom_blur_pipe = bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_blur", None);
        // Composite additively blends the halo back into the HDR target (src·1 + dst·1).
        let add_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let bloom_composite_pipe = bloom_pipe(
            device,
            &bloom_shader,
            &bloom_pl,
            "fs_composite",
            Some(add_blend),
        );
        // The veiling glare belongs to the stylized bloom (raster-fed backends); the physical
        // backend's bloom.wgsl has no such entry points.
        let bloom_composite_glare_pipe = shader_backend.then(|| bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_composite_glare", Some(add_blend)));
        // Glare pyramid: five box-filtered halvings of the blurred bright pass (a bilinear copy
        // into a half-size target is a 2x2 box); rings 1.5/3/6/12 deg read levels 1..4.
        let bloom_copy_pipe = shader_backend.then(|| bloom_pipe(device, &bloom_shader, &bloom_pl, "fs_composite_glare", None));
        let mut bloom_levels = Vec::new();
        let mut bloom_level_binds = Vec::new();
        for level in 0..5u32 {
            let (w, h) = ((bw >> (level + 1)).max(1), (bh >> (level + 1)).max(1));
            let view = make_target(device, "bloom-glare-level", w, h, hdr_format(), wgpu::TextureUsages::TEXTURE_BINDING);
            if level < 4 { bloom_level_binds.push(make_bloom_bind(&view, "bloom-glare-level-bind")); }
            bloom_levels.push(view);
        }
        let glare_tex = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding, visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let glare_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-glare-bgl"),
            entries: &[glare_tex(0), glare_tex(1), glare_tex(2), glare_tex(3), sampler_entry(4)],
        });
        let bloom_glare_bind = shader_backend.then(|| device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bloom-glare-bind"), layout: &glare_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&bloom_levels[1]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&bloom_levels[2]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&bloom_levels[3]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&bloom_levels[4]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&bloom_sampler) },
            ],
        }));
        let bloom_glare_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom-glare-pl"), bind_group_layouts: &[&bloom_bgl, &glare_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange { stages: wgpu::ShaderStages::FRAGMENT, range: 0..std::mem::size_of::<BloomPush>() as u32 }],
        });
        let bloom_glare_pipe = shader_backend.then(|| bloom_pipe(device, &bloom_shader, &bloom_glare_pl, "fs_glare", None));

        // ---------- volumetric smoke ----------
        // binding 0 = scene depth (sampled with textureLoad for occlusion), 1 = SmokeUniform.
        let smoke_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("smoke"),
            size: std::mem::size_of::<SmokeUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let smoke_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("smoke-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let smoke_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smoke-bind"),
            layout: &smoke_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: smoke_buf.as_entire_binding(),
                },
            ],
        });
        // Without ray queries both backends light media from the environment probe through the
        // scene-geometry visibility atlas (stylized_smoke.wgsl). Hardware High substitutes a small
        // TLAS visibility body into smoke.wgsl and binds the same scene AS as combine.
        let raster_media = raster_lighting && !rt_on;
        let smoke_source = if raster_media {
            include_str!("../shaders/stylized_smoke.wgsl").to_owned()
        } else if rt_on {
            smoke_ray_query_source()
        } else {
            physical_atmosphere_transport_shader_source(SMOKE_SHADER_SOURCE)
        };
        let smoke_shader = shader(
            device,
            if rt_on {
                "smoke-ray-query"
            } else {
                "smoke-portable"
            },
            &smoke_source,
        );
        // Keep the physical environment at group(2) in both variants. Portable devices receive an
        // empty group(1), while High binds the exact scene TLAS there for source visibility.
        let smoke_empty_visibility_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("smoke-empty-visibility-bgl"),
                entries: &[],
            });
        let smoke_empty_visibility_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("smoke-empty-visibility-bind"),
            layout: &smoke_empty_visibility_bgl,
            entries: &[],
        });
        // The stylized smoke lights its media through the same environment-visibility atlas as
        // surfaces, so gun haze inside closed armour sees only what the openings admit.
        let smoke_visibility_bgl = if raster_media { &sky_visibility.sampling_bgl }
            else { rt_as_bgl.as_ref().unwrap_or(&smoke_empty_visibility_bgl) };
        let mut smoke_layouts = vec![&smoke_bgl, smoke_visibility_bgl, &physical_environment_bgl];
        if raster_media { smoke_layouts.push(&cabin_shadows.sampling_bgl); }
        let smoke_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("smoke-pl"),
            bind_group_layouts: &smoke_layouts,
            push_constant_ranges: &[],
        });
        // Premultiplied "over" blend: dst*(1-srcA) + srcRGB. The volume estimate itself is first
        // written without blending, denoised, then this blend composites the converged result.
        let over_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let smoke_compile_start = std::time::Instant::now();
        println!(
            "[renderer] compiling volume pipeline ({} KiB)",
            smoke_source.len() / 1024,
        );
        let smoke_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("smoke"),
            layout: Some(&smoke_pl),
            vertex: wgpu::VertexState {
                module: &smoke_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &smoke_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr_format(),
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: pipeline_cache.as_ref(),
        });
        println!(
            "[renderer] volume pipeline ready in {:.0} ms",
            smoke_compile_start.elapsed().as_secs_f32() * 1000.0,
        );
        let volume_composite_shader = shader(
            device,
            "volume-composite",
            include_str!("../shaders/volume_composite.wgsl"),
        );
        let volume_composite_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("volume-composite-pl"),
                bind_group_layouts: &[&volume_composite_bgl],
                push_constant_ranges: &[],
            });
        let volume_composite_pipe =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("volume-composite"),
                layout: Some(&volume_composite_layout),
                vertex: wgpu::VertexState {
                    module: &volume_composite_shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &volume_composite_shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: hdr_format(),
                        blend: Some(over_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        let heat_gpu = crate::debug_gates::DebugGates::active().heat_haze
            .then(|| crate::heat_haze::Gpu::new(device, iw, ih, &depth_view));
        let pressure_gpu = crate::pressure_front::Gpu::new(device, iw, ih, &depth_view, rt_as_bgl.as_ref());

        // ---------- crush ----------
        // Fixed generated void-and-cluster rank tile. No random frame phase or temporal shuffle.
        let blue_noise_texture=device.create_texture(&wgpu::TextureDescriptor {
            label:Some("display-blue-noise64"),size:wgpu::Extent3d{width:64,height:64,depth_or_array_layers:1},
            mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,
            format:wgpu::TextureFormat::R16Uint,usage:wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST,
            view_formats:&[],
        });
        queue.write_texture(blue_noise_texture.as_image_copy(),include_bytes!("../assets/render/blue_noise64.bin"),
            wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(128),rows_per_image:Some(64)},
            wgpu::Extent3d{width:64,height:64,depth_or_array_layers:1});
        let blue_noise_view=blue_noise_texture.create_view(&Default::default());

        let crush_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crush-bgl"),
            entries: &[
                tex_entry(0),
                sampler_entry(1),
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                // Scene depth — so the interior smoke veil confines to NEAR geometry (the compartment)
                // instead of washing the far world seen through the optic.
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                tex_entry(5), // optic snapshot — the eyepiece-composite inset (mask mode 4)
                uniform_entry(6, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {binding:7,visibility:wgpu::ShaderStages::FRAGMENT,
                    ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Uint,
                        view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
            ],
        });
        let crush_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("crush-bind"),
            layout: &crush_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: crush_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&optic_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: environment_probe.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {binding:7,resource:wgpu::BindingResource::TextureView(&blue_noise_view)},
            ],
        });
        let crush_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crush-pl"),
            bind_group_layouts: &[&crush_bgl],
            push_constant_ranges: &[],
        });
        let crush_pipe = fullscreen_pipe(device, &crush_shader, &crush_pl, LDR);

        // Discrete snow/dust presentation is isolated from HDR weather and from display grading.
        let pixel_particle_vertices = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pixel-particle-world-positions"),
            size: (crate::pixel_particles::MAX_PIXEL_PARTICLES
                * std::mem::size_of::<crate::pixel_particles::PixelParticle>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pixel_particle_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pixel-particle-uniform"), size: std::mem::size_of::<PixelParticleUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pixel_particle_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pixel-particle-bgl"), entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                cabin_medium_texture_entry(3),
            ],
        });
        let pixel_particle_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pixel-particle-bind"), layout: &pixel_particle_bgl, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: pixel_particle_uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&depth_view) },
                wgpu::BindGroupEntry { binding: 2, resource: crush_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding:3,resource:wgpu::BindingResource::TextureView(&cabin_medium_view) },
            ],
        });
        let pixel_particle_pipe = if rt_on || shader_backend {
            let source = if shader_backend { include_str!("../shaders/stylized_particles.wgsl").to_owned() } else { protective_glass::inject(include_str!("../shaders/pixel_particles.wgsl"), false) };
            let module = shader(device, "pixel-particles", &observer_shader_source(&source));
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pixel-particles"),
                bind_group_layouts: &if shader_backend {
                    vec![&pixel_particle_bgl,&sky_visibility.sampling_bgl,&cabin_shadows.sampling_bgl]
                }else{vec![&pixel_particle_bgl,rt_as_bgl.as_ref().unwrap_or(&smoke_empty_visibility_bgl)]},
                push_constant_ranges: &[],
            });
            Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("pixel-particles"), layout: Some(&layout),
                vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"),
                    buffers: &[wgpu::VertexBufferLayout { array_stride: 16,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Uint32] }],
                    compilation_options: Default::default() },
                primitive: wgpu::PrimitiveState::default(), depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState { format: LDR,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default() }),
                multiview: None, cache: pipeline_cache.as_ref(),
            }))
        } else { None };

        // ---------- blit ----------
        // The crop uniform defaults to the full frame; splitscreen sub-viewport blits rewrite it
        // per view (each render is its own submit, so sequential writes never race).
        let blit_ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit-crop"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        blit_ubuf
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]));
        blit_ubuf.unmap();
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
            entries: &[
                tex_entry(0),
                sampler_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit-bind"),
            layout: &blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&ldr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: blit_ubuf.as_entire_binding(),
                },
            ],
        });
        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-pl"),
            bind_group_layouts: &[&blit_bgl],
            push_constant_ranges: &[],
        });

        // ---------- front-end UI (bakes the font atlas + text pipelines) ----------
        let ui = Ui::new(device, queue);

        // Save the exact driver-produced blob after every cached pipeline has been created.
        // Without this call each new game/capture recompiles the large ray-query programs.
        persist_physical_pipeline_cache(pipeline_cache.as_ref(), pipeline_cache_path.as_deref());

        let raymarch = crate::graphics::raymarch_backend_enabled()
            .then(|| raymarch::Backend::new(device, &material_bgl, &track_bgl, &terrain_detail_view, &terrain_detail_sampler, &sky_depth_pipe, iw, ih, rt_scene));
        Self {
            // Standard stays at the playtested 0.7; [ ] still dials continuously in-game, while
            // KESSEL_RETRO_PRESET gives deterministic clean/light/standard/strong A/B captures.
            iw,
            ih,
            graphics_preset,
            shader_backend,
            raymarch,
            raster_optics: shader_backend.then(|| {
                raster_optics::RasterOptics::new(device, iw, ih, &terrain_detail_view, &terrain_detail_sampler,
                    &sky_visibility.sampling_bgl, &cabin_shadows.sampling_bgl, &cabin_medium_view)
            }),
            fine_vfx: renderer_fine_vfx::FineVfx::new(device, &sky_visibility.sampling_bgl, &physical_environment_bgl, &cabin_shadows.sampling_bgl),
            weapon_particles: renderer_weapon_particles::WeaponParticles::new(device, &sky_visibility.sampling_bgl, &physical_environment_bgl, &cabin_shadows.sampling_bgl),
            geometry_aa: (shader_backend && renderer_aa::enabled()).then(||renderer_aa::GeometryAa::new(
                device,iw,ih,&mesh_pl,&mesh_shader,&sky_pl,&sky_shader)),
            raster_ssao,
            hover_outline: shader_backend.then(|| hover_outline::HoverOutline::new(device, iw, ih, &mesh_pl, &mesh_shader)),
            hovered_meshes: Vec::new(),
            raster_glass_pipe: shader_backend.then(|| raster_glass::pipeline(device, &mesh_pl, &mesh_shader, false)),
            raster_glow_glass_pipe: shader_backend.then(|| raster_glass::pipeline(device, &mesh_pl, &mesh_shader, true)),
            transport_quality,
            shadow_res,
            camera,
            master_retro: configured_master_retro(),
            display_tuning: None,
            display_grade: [1.0, 1.0, 0.0, 1.0],
            display_dither_pattern: configured_display_dither(),
            sky_time_scale: 1.0,
            day_night_cycle: day_night_cycle_enabled(),
            day_phase_seconds: None,
            weather_offset: glam::Vec3::ZERO,
            weather_override: None,
            weather_original: None,
            user_sky_phase: None,
            exposure_override: configured_camera_exposure_override(),
            vision_boost: 0.0,
            fog_density: 0.02,
            reticle: false,
            optic_blast: 0.0,
            optic_eye: [0.0; 2],
            observer_flash_source: false,
            crew_ko: 0.0,
            crew_ko_dir: [0.72, 0.69],
            concussion: 0.0,
            menu_mode: false,
            night: default_celestial.darkness(),
            sky_clock: default_sky_clock,
            celestial: default_celestial,
            authored_clock_start: None,
            // Bare/editor renderers start dry. Game::build_act applies the season explicitly, so a
            // spring editor world can never inherit an accidental winter overlay.
            precipitation: 0.0,
            overcast: 0.90,
            storm: 0.65,
            atmosphere_mood: 0.0,
            twilight: 0.0,
            weather_wind: 0.8,
            weather_wind_world: profile_weather_wind(0, 0.8),
            weather_shelter: 0.0,
            weather_seed: 0,
            weather_act_id: 0,
            weather_progress: 0.0,
            interior_light: 0.0,
            interior_haze: 0.0,
            optic_mask: 0.0,
            lights: Vec::new(),
            lights_offered: 0,
            lights_full: Vec::new(),
            muzzle_hint: None,
            player_snow_load: 0.0,
            world_snow_load: 0.0,
            wear: [0.0; 4],
            whitewash: 0.0,
            exhaust_puffs: Vec::new(),
            fog_bank: [0.0; 4],
            frame: 0,
            hdr_view,
            hdr_tex,
            depth_view,
            ldr_view,
            ldr_tex,
            title_backdrop: None,
            title_backdrop_frames: 0,
            title_backdrop_settled: false,
            title_last_frame: None,
            title_eye_convergence: crate::HeadlessExposureConvergence::default(),
            optic_tex,
            sampler,
            sky_pipe,
            sky_depth_pipe,
            depth_gate_sky: std::env::var(UNGATED_SKY_ENV)
                .map(|value| value != "1")
                .unwrap_or(true),
            camera_buf,
            camera_bind,
            physical_environment_buf,
            physical_environment_bind,
            mesh_pipe,
            mesh_inst_pipe,
            shadow_inst_pipe,
            meshes,
            inst_prototypes: Vec::new(),
            inst_material_cache: HashMap::new(),
            inst_batches: Vec::new(),
            inst_spatial_index: InstSpatialIndex::default(),
            instance_queue: queue.clone(),
            pose_device: device.clone(),
            next_inst_buffer_id: 0,
            time: 0.0,
            track_half: 0.0,
            track_view,
            track_bind,
            track_pipe,
            pending_stamps: Vec::new(),
            track_lookup,
            track_atlas: track_ruts::Atlas::default(),
            track_footprints: HashMap::new(),
            shadow_view,
            shadow_pipe,
            shadow_bind,
            shadow_bgl,shadow_sampler,scene_visibility,scene_visibility_gpu,
            near_shadow_view,
            near_shadow_camera_buf,
            near_shadow_camera_bind,
            near_shadow_buf,
            near_shadow_res,
            cabin_shadows,
            sky_visibility,
            coop_shadow: SharedShadowFrame::default(),
            bw,
            bh,
            bloom_a_view,
            bloom_b_view,
            bloom_bright_pipe,
            bloom_blur_pipe,
            bloom_composite_pipe,
            bloom_glare_pipe,
            bloom_composite_glare_pipe,
            bloom_copy_pipe,
            bloom_levels,
            bloom_level_binds,
            bloom_glare_bind,
            bloom_bind_hdr,
            bloom_bind_a,
            bloom_bind_b,
            bloom_portal_buf,
            smoke_puffs: [PuffGpu {
                pos: [0.0; 3],
                age: 0.0,
                dir: [0.0, 0.0, 1.0],
                strength: 0.0,
                shape: [0.0;4],
            }; MAX_PUFFS],
            breech_gas: PuffGpu {
                pos: [0.0; 3],
                age: 0.0,
                dir: [0.0, 0.0, -1.0],
                strength: 0.0,
                shape: [0.0;4],
            },
            cabin_medium: Some(cabin_medium),
            cabin_medium_texture,cabin_medium_view,
            cabin_medium_uniform: bytemuck::Zeroable::zeroed(),
            cabin_medium_pose: None,
            atmo_enabled: false,
            atmo_haze: 0.0,
            atmo_dust: 0.0,
            atmo_origin: glam::Vec3::ZERO,
            smoke_buf,
            smoke_bind,
            smoke_empty_visibility_bind,
            smoke_pipe,
            volume_current_view,
            volume_accum_pipe,
            volume_composite_pipe,
            volume_current_composite_bind,
            pixel_particle_field: RefCell::new(crate::pixel_particles::PixelParticleField::default()),
            pixel_particle_count: Cell::new(0),
            pixel_particle_vertices, pixel_particle_uniform, pixel_particle_bind, pixel_particle_pipe,
            heat_sources: crate::heat_haze::Sources::default(),
            heat_gpu,
            pressure_fronts: crate::pressure_front::Fronts::default(),
            pressure_gpu,
            crush_buf,
            crush_bind,
            crush_pipe,
            rt_mix: 0.0,
            portable_ao_enabled: false,
            deferred,
            raster_lighting,
            portable_visibility,
            environment_probe,
            gbuf_pipe,
            gbuf_inst_pipe,
            combine_pipe,
            gbuf_bind,
            g0_view,
            g1_view,
            g2_view,
            g3_view,
            sun_dir: default_celestial.key.direction.as_vec3(),
            // The crewman's eye runs on every presented frame (observer.rs). Only the headless
            // photo harness pauses it while stochastic transport converges.
            autoexp: true,
            rt_as_bgl,
            combine_rt_bind: None,
            exp_pipe,
            exp_binds,
            exp_bufs,
            exp_clock_buf,
            replay_previous_elapsed: None,
            replay_exposure_time: None,
            continuous_camera_translation: false,
            accum_on: std::env::var("KESSEL_NO_ACCUM")
                .map(|v| v != "1")
                .unwrap_or(true),
            accum_test: false,
            lit_diff_view,
            lit_other_view,
            lit_emission_view,
            accum_pipe,
            accum_delta_bind,
            accum_ubuf,
            delta_buf,
            resolve_pipe,
            surface_reconstruction,
            live_texel_probe,
            live_texel_probe_view_inside: false,
            g3_tex,
            views,
            active_view: 0,
            render_work: Cell::new(RenderWorkStats::default()),
            gpu_profile: crate::gpu_profile::GpuProfile::new(device),
            rt_degenerate_reported: std::collections::HashSet::new(),
            rt_on,
            rt_scene,
            rt_skip: false,
            optic_hide_pass: false,
            rt_tlas: None,
            rt_tlas_cap: 0,
            rt_overflow_frames: 0,
            rt_overflow_now: 0,
            rt_overflow_reported: None,
            rt_prev_count: 0,
            rt_batches_dirty: true,
            rt_moving_batches_dirty: false,
            rt_batch_slot_offsets: Vec::new(),
            rt_surface_gpu: None,
            rt_surface_dirty: rt_on,
            rt_instance_topology_dirty: false,
            rt_scene_surface_ids: Vec::new(),
            rt_prototype_surface_ids: Vec::new(),
            depth_tex,
            blit_shader,
            blit_layout,
            blit_bind,
            blit_ubuf,
            blit_pipes: HashMap::new(),
            material_bgl,
            menu_meshes: Vec::new(),
            ui,
            held_map_surface: None,
        }
    }

    /// Reallocate a complete, internally consistent target graph, retaining the live GPU scene.
    /// No Game rebuild, material reupload, mesh-slot renumbering or simulation reset occurs.
    /// Explicit pipeline layouts allow existing material/instance bindings to remain compatible.
    pub fn resize_shader_resolution(
        &mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32),
    ) -> anyhow::Result<()> {
        self.resize_shader_targets(device, queue, size, self.shadow_res)
    }

    pub fn full_transport_quality(&self) -> bool { self.transport_quality == TransportQuality::Full }
    pub fn set_full_transport_quality(&mut self, full: bool) {
        let quality = if full { TransportQuality::Full } else { TransportQuality::Gameplay };
        if quality != self.transport_quality { self.transport_quality = quality; self.reset_history(); }
    }

    pub fn resize_shader_targets(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        size: (u32,u32), shadows: u32) -> anyhow::Result<()> {
        anyhow::ensure!(self.shader_backend && !self.rt_on, "Resolution changes require the shader renderer");
        anyhow::ensure!([512,768,1024,1536,2048].contains(&shadows), "Unsupported shadow resolution");
        anyhow::ensure!(INTERNAL_RES_PRESETS.contains(&size), "Unsupported display resolution");
        if (self.iw, self.ih) == size && self.shadow_res == shadows { return Ok(()); }
        device.poll(wgpu::PollType::wait_indefinitely())?;
        MENU_SHADOW_RES.set(Some(shadows));
        let mut next = Self::new_at_resolution(device, queue, &[], &[], size);
        MENU_SHADOW_RES.set(None);
        // Scene and simulation-owned presentation state survive. Target-bound uniforms, shadow
        // caches, temporal histories and post-process bindings are rebuilt together by new().
        macro_rules! retain { ($($field:ident),* $(,)?) => { $(
            std::mem::swap(&mut next.$field, &mut self.$field);
        )* }; }
        retain!(camera, master_retro, display_tuning, display_dither_pattern, display_grade, transport_quality, exposure_override, fog_density, reticle, optic_blast, observer_flash_source,
            crew_ko, crew_ko_dir, concussion, menu_mode, night, precipitation, overcast, storm,
            atmosphere_mood, twilight, weather_wind, weather_wind_world, weather_shelter,
            sky_clock, celestial, authored_clock_start, sky_time_scale, user_sky_phase, weather_offset, weather_override, weather_original,
            weather_seed, weather_act_id, weather_progress, interior_light, interior_haze,
            optic_mask, lights, frame, time, sun_dir, autoexp, accum_on, accum_test,
            rt_mix, portable_ao_enabled, meshes, inst_prototypes, inst_material_cache, inst_batches,
            inst_spatial_index, next_inst_buffer_id, menu_meshes, hovered_meshes,
            track_half, track_view, track_bind, pending_stamps, track_lookup, track_atlas, track_footprints, smoke_puffs, breech_gas,
            atmo_enabled, atmo_haze, atmo_dust, atmo_origin, pixel_particle_field,
            pressure_fronts, heat_sources, ui, held_map_surface);
        if let (Some(old), Some(new)) = (&self.raster_optics, &mut next.raster_optics) {
            new.copy_registrations_from(old);
        }
        next.cabin_shadows.copy_registrations_from(&self.cabin_shadows);
        next.cabin_medium=self.cabin_medium.clone();
        next.cabin_medium_uniform=self.cabin_medium_uniform;
        next.cabin_medium_pose=self.cabin_medium_pose;
        next.fine_vfx.copy_registrations_from(&self.fine_vfx);
        // View buffers/bindings belong to the new render extents; producer records are resent.
        // Never retain a depth binding from the renderer being resized.
        next.camera.aspect = size.0 as f32 / size.1 as f32;
        println!("[display] render targets {}x{}; retained {} meshes, {} instance batches",
            size.0, size.1, next.meshes.len(), next.inst_batches.len());
        *self = next;
        Ok(())
    }

    /// Apply a validated saved world profile after campaign defaults, in both editor and play.
    /// The record's weather is the medium; its Universal Time, when it carries one, starts the
    /// act clock. It never sets a sun direction or a camera exposure: the ephemeris lights the
    /// sky and the observer adapts to it.
    pub fn apply_authored_lighting(&mut self, tuning: crate::environment_authoring::LightingTuning, winter: bool) {
        if !tuning.enabled { return; }
        // The record's instant starts the clock once (and again only when the record changes, as
        // in the editor); the running clock then advances with simulated time.
        if tuning.utc.is_some() && tuning.utc != self.authored_clock_start {
            self.authored_clock_start = tuning.utc;
            self.set_sky_clock(crate::celestial_sky::SkyClock::new(
                tuning.utc.unwrap(), crate::celestial_sky::act_site(self.weather_act_id)));
        }
        self.fog_density = tuning.fog_density;
        self.overcast = tuning.overcast;
        self.storm = tuning.storm;
        self.twilight = 0.0;
        self.precipitation = if winter { (tuning.storm / 0.98).clamp(0.0,1.0) } else { 0.0 };
    }

    /// Apply the campaign-wide authored weather language. `act_id` selects a restrained palette,
    /// while `winter` controls real snow rather than merely recolouring a spring map. The values are
    /// deliberately public/tweakable too; the atmosphere gallery uses this exact shipping path.
    /// `phase` names an instant on the act's date; the ephemeris lights it.
    pub fn apply_campaign_atmosphere(&mut self, act_id: u8, winter: bool, phase: DayPhase) {
        let profile = campaign_atmosphere_profile(act_id);
        self.weather_act_id = act_id;
        self.weather_progress = 0.0;
        self.overcast = profile.overcast;
        self.storm = profile.storm;
        self.atmosphere_mood = campaign_ground_mood(winter, profile.mood);
        self.twilight = 0.0;
        self.weather_wind = profile.wind_exposure;
        self.weather_wind_world = profile_weather_wind(act_id, profile.wind_exposure);
        self.weather_shelter = 0.0;
        self.weather_seed = (act_id as u32).wrapping_mul(0x9e37_79b9);
        self.fog_density = profile.ground_extinction_m_inv;
        self.set_sky_clock(crate::celestial_sky::SkyClock::for_act(act_id, phase.clock_phase()));
        self.precipitation = if winter { (profile.storm / 0.98).clamp(0.0, 1.0) } else { 0.0 };
    }

    /// Start the sky at an instant and place. The key body's direction becomes `sun_dir` (the
    /// direct beam and the shadow cascades), and `night` follows the Sun's altitude.
    pub fn set_sky_clock(&mut self, clock: crate::celestial_sky::SkyClock) {
        self.sky_clock = clock;
        self.refresh_celestial();
    }

    /// Advance the act clock with simulated time; the Sun and the Moon move with it.
    pub fn user_sky_phase(&self) -> u8 { self.user_sky_phase.map_or(0,|p|p.0) }

    pub fn set_user_sky_phase(&mut self, phase:u8) {
        if phase == self.user_sky_phase() { return; }
        if phase == 0 {
            if let Some((_,clock)) = self.user_sky_phase.take() { self.set_sky_clock(clock); }
        } else if phase <= 4 {
            let original = self.user_sky_phase.map_or(self.sky_clock,|p|p.1);
            let named = [DayPhase::Dawn,DayPhase::Day,DayPhase::Dusk,DayPhase::Night][phase as usize-1];
            self.set_sky_clock(crate::celestial_sky::SkyClock::for_act(self.weather_act_id,named.clock_phase()));
            self.user_sky_phase = Some((phase,original));
        }
        self.reset_history();
    }

    /// Read-only gameplay snapshot of the exact optical medium uploaded to main view and ports.
    pub(crate) fn weather_visibility(&self)->crate::weather_visibility::WeatherView {
        let profile=world_medium_profile(self.precipitation,self.storm);
        crate::weather_visibility::WeatherView {
            sigma:self.fog_density.max(0.0),snow_fraction:profile.snow_fraction,
            layer:[profile.layer[0],profile.layer[1]],offset:self.weather_offset,
            cabin:self.cabin_medium.clone(),cabin_uniform:self.cabin_medium_uniform,
        }
    }

    /// How fast the sky's clock runs now. With the day and night cycle on, the Sun and Moon
    /// still stand exactly where the ephemeris puts them for the date and place; only the rate
    /// of the clock changes, so that full day, each twilight and the night last their set times
    /// (owner, 2026-10-04).
    fn sky_clock_rate(&mut self) -> f64 {
        if !self.day_night_cycle { return self.sky_time_scale as f64; }
        let phases = *self.day_phase_seconds.get_or_insert_with(|| {
            // One day sampled every two minutes from now: how long the Sun spends in each phase.
            let mut probe = self.sky_clock;
            let mut seconds = [0.0_f64; 3];
            for _ in 0..720 {
                probe.advance(120.0);
                let altitude = crate::celestial_sky::CelestialLighting::at(probe.now(), probe.site,
                    AtmosphereParameters::default().solar_irradiance).sun_altitude_deg();
                seconds[day_phase(altitude)] += 120.0;
            }
            seconds
        });
        // Real seconds of each phase over its play seconds. A phase the date lacks runs at the
        // day's mean rate.
        let mean = 86_400.0 / (FULL_DAY_S + 2.0 * TWILIGHT_S + NIGHT_S);
        let rate = |phase: usize, play_s: f64| if phases[phase] > 60.0 { phases[phase] / play_s } else { mean };
        let rates = [rate(0, NIGHT_S), rate(1, 2.0 * TWILIGHT_S), rate(2, FULL_DAY_S)];
        // Blend across a degree at each boundary so the clock never jumps.
        let altitude = self.celestial.sun_altitude_deg();
        let ramp = |edge: f64| ((altitude - edge) / 1.0 + 0.5).clamp(0.0, 1.0);
        let (above_night, above_twilight) = (ramp(TWILIGHT_FROM_DEG), ramp(TWILIGHT_TO_DEG));
        (rates[0] * (1.0 - above_night) + rates[1] * (above_night - above_twilight).max(0.0)
            + rates[2] * above_twilight) * self.sky_time_scale as f64
    }

    pub fn advance_sky_clock(&mut self, dt_s: f32) {
        let rate = self.sky_clock_rate();
        self.sky_clock.advance(dt_s as f64 * rate);
        self.apply_weather_override();
        self.weather_offset += glam::Vec3::new(self.weather_wind_world.x,0.0,self.weather_wind_world.y)
            *weather_gust(self.time,self.weather_seed)*dt_s.max(0.0);
        self.refresh_celestial();
    }

    fn refresh_celestial(&mut self) {
        self.celestial = crate::celestial_sky::CelestialLighting::at(self.sky_clock.now(),
            self.sky_clock.site, AtmosphereParameters::default().solar_irradiance);
        self.sun_dir = self.celestial.key.direction.as_vec3();
        self.night = self.celestial.darkness();
    }

    /// The Sun and the Moon of the current instant.
    pub fn celestial(&self) -> &crate::celestial_sky::CelestialLighting {
        &self.celestial
    }

    /// Progress changes the physical medium gradually; backtracking cannot restore a blizzard.
    pub fn advance_campaign_weather(&mut self, goal: f32, winter: bool, dt: f32) {
        if !(1..=3).contains(&self.weather_act_id) { return; }
        let goal = goal.clamp(0.0, 1.0).max(self.weather_progress);
        self.weather_progress += (goal - self.weather_progress)
            * (1.0 - (-dt.clamp(0.0, 0.25) / 45.0).exp());
        let profile = self.current_campaign_environment();
        self.overcast = profile.overcast;
        self.storm = profile.storm;
        self.atmosphere_mood = campaign_ground_mood(winter, profile.mood);
        self.fog_density = profile.ground_extinction_m_inv;
        self.weather_wind = profile.wind_exposure;
        self.precipitation = if winter { (profile.storm / 0.98).clamp(0.0, 1.0) } else { 0.0 };
    }

    fn current_campaign_environment(&self) -> CampaignAtmosphereProfile {
        let from = campaign_atmosphere_profile(self.weather_act_id);
        let next = if (1..3).contains(&self.weather_act_id) { self.weather_act_id + 1 } else { self.weather_act_id };
        from.toward(campaign_atmosphere_profile(next), self.weather_progress)
    }

    /// Use the authored act/world seed for stable flake cells and gust phase. Placement is hashed in
    /// WGSL, but the full seed stays here so no f32 precision is lost before it is folded to 24 bits.
    pub fn set_weather_seed(&mut self, seed: u32) {
        self.weather_seed = seed;
    }

    /// Feed the same zoned world wind used by smoke and fire. Values are finite-clamped defensively:
    /// a broken gameplay vector should degrade to still air rather than poison every snow fragment.
    pub fn set_weather_wind(&mut self, wind: glam::Vec2) {
        self.weather_wind_world = if wind.is_finite() {
            wind.clamp_length_max(20.0)
        } else {
            glam::Vec2::ZERO
        };
        self.apply_weather_override();
    }

    pub fn set_model(&mut self, idx: usize, m: glam::Mat4) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.model = m;
            // Keep the culling AABB honest for dynamic meshes (tank parts, FX).
            let (mn, mx) = transform_aabb(m, g.local_min, g.local_max);
            g.world_min = mn;
            g.world_max = mx;
        }
    }
    /// Update a bounded articulated actor without changing topology, materials, draw count or
    /// pipelines. Hardware-ray meshes are excluded: their immutable BLAS may not follow vertices.
    /// Rare anatomical topology mask; retains index capacity and never mutates a shared actor buffer.
    pub fn upload_pose_indices(&mut self,slot:usize,indices:&[u32])->bool {
        let Some(mesh)=self.meshes.get_mut(slot) else{return false;};
        if indices.is_empty() || indices.len()!=mesh.index_count as usize || (self.rt_scene&&!mesh.raster_only) {return false;}
        let vertex_count=mesh.vbuf.size()/std::mem::size_of::<crate::gltf_load::Vertex>() as u64;
        if indices.iter().any(|i|*i as u64>=vertex_count){return false;}
        if self.raster_lighting&&self.portable_visibility {
            let Some(source)=mesh.visibility_source.as_mut() else{return false;};
            let topology_changed=source.indices.as_slice()!=indices;
            source.indices.clone_from_slice(indices);
            if mesh.visibility_deformable && !topology_changed {
                self.scene_visibility.refit_mesh(mesh.visibility_geometry.unwrap(),source).expect("wounded visibility refit");
            }else {
                mesh.visibility_geometry=Some(self.scene_visibility.register_deformable_mesh(source).expect("wounded visibility registration"));
                mesh.visibility_deformable=true;
            }
        }
        mesh.rigid_pages=None;
        mesh.ibuf=self.pose_device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:Some("private wounded actor indices"),contents:bytemuck::cast_slice(indices),
            usage:wgpu::BufferUsages::INDEX|wgpu::BufferUsages::COPY_DST,
        });true
    }
    pub fn upload_pose_vertices(&mut self, slot: usize, vertices: &[crate::gltf_load::Vertex]) -> bool {
        let Some(mesh) = self.meshes.get_mut(slot) else { return false; };
        if vertices.len() > 4096 || vertices.is_empty()
            || std::mem::size_of_val(vertices) as u64 != mesh.vbuf.size()
            || (self.rt_scene && !mesh.raster_only) { return false; }
        let mut lo=glam::Vec3::splat(f32::INFINITY);
        let mut hi=glam::Vec3::splat(f32::NEG_INFINITY);
        for vertex in vertices {
            let point=glam::Vec3::from_array(vertex.pos);
            if !point.is_finite() { return false; }
            lo=lo.min(point);hi=hi.max(point);
        }
        if self.raster_lighting && self.portable_visibility {
            let Some(source)=mesh.visibility_source.as_mut() else {return false;};
            source.vertices.clone_from_slice(vertices);source.min=lo;source.max=hi;
            if mesh.visibility_deformable {
                self.scene_visibility.refit_mesh(mesh.visibility_geometry.unwrap(),source).expect("deformed visibility refit");
            } else {
                mesh.visibility_geometry=Some(self.scene_visibility.register_deformable_mesh(source).expect("deformed visibility registration"));
                mesh.visibility_deformable=true;
            }
        }
        if !mesh.pose_buffer_private {
            // Actor clones share rigid buffers until their first deformation. Updating one must
            // not move another actor's rendered source while its private visibility BLAS stays put.
            mesh.vbuf=self.pose_device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label:Some("private articulated actor vertices"),contents:bytemuck::cast_slice(vertices),
                usage:wgpu::BufferUsages::VERTEX|wgpu::BufferUsages::COPY_DST,
            });
            mesh.pose_buffer_private=true;
        } else {self.instance_queue.write_buffer(&mesh.vbuf,0,bytemuck::cast_slice(vertices));}
        mesh.rigid_pages=None;
        mesh.pose_revision=mesh.pose_revision.wrapping_add(1);
        mesh.local_min=lo;mesh.local_max=hi;
        (mesh.world_min,mesh.world_max)=transform_aabb(mesh.model,lo,hi);
        true
    }
    /// Register fallback actor bodies and attached weapons, including actors rendered while
    /// the player is seated. This changes no visibility, geometry, material or ray mask.
    pub fn mark_temporal_dynamic(&mut self, idx: usize) {
        if let Some(mesh) = self.meshes.get_mut(idx) {
            if !mesh.temporal_dynamic {
                mesh.temporal_dynamic = true;
                // The tagged key is shared by G3, TLAS and the exact source/material lookup.
                self.rt_surface_dirty = self.rt_on;
            }
        }
    }
    /// New persistent actor identity sharing the original triangles, material and built BLAS.
    /// No dead actor is reused; its physics, wreck animation and render identity remain intact.
    pub fn clone_actor_mesh(&mut self, source: usize) -> usize {
        self.meshes[source].visibility_deformable = false;
        self.meshes[source].pose_buffer_private = false;
        let mut mesh = self.meshes[source].clone();
        mesh.visible = false;
        mesh.visibility_deformable = false;
        mesh.pose_buffer_private = false;
        mesh.temporal_dynamic = true;
        let slot = self.meshes.len();
        self.meshes.push(mesh);
        self.rt_batches_dirty = true;
        if self.rt_on && !self.rt_surface_dirty && self.rt_surface_gpu.is_some()
            && self.rt_scene_surface_ids.len() == slot {
            // Identical source triangles keep their existing surface ID. Only the sparse
            // instance lookup changes; do not repack a gigabyte of unchanged world geometry.
            self.rt_scene_surface_ids.push(self.rt_scene_surface_ids[source]);
            self.rt_instance_topology_dirty = true;
        } else {
            self.rt_surface_dirty = self.rt_on;
        }
        slot
    }
    /// Exclude a mesh from the gunner's exterior optic raster without changing its visibility,
    /// so it remains a caster and the shadow and sky-visibility caches are not disturbed.
    pub fn set_optic_hidden(&mut self, idx: usize, hidden: bool) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.optic_hidden = hidden;
        }
    }
    pub fn set_visible(&mut self, idx: usize, v: bool) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.visible = v;
        }
    }
    pub fn set_emissive(&mut self, idx: usize, e: f32) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive = e;
        }
    }
    /// Flag mesh `idx` as a NIGHT window / streetlamp: the mesh pass keeps its authored colour and
    /// scales the glow by the `night` uniform (glowing at Nacht, dark at Tag) instead of taking the
    /// FX gunfire-orange emissive path. Pair with `set_emissive(idx, intensity)` for the glow level.
    pub fn mark_window(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive_win = true;
        }
    }
    /// Mesh `idx` is part of the player's vehicle: its lying snow follows `player_snow_load`.
    pub fn mark_snow_vehicle(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.snow_vehicle = true;
        }
    }
    /// Flag mesh `idx`'s default-material metal as painted steel (the player's hull and turret):
    /// the raymarch G-buffer draws it as satin paint, bare steel only where wear exposes it
    /// (`rm_gbuffer.wgsl` `paint_source_metal`). The gun's machined parts are not flagged.
    pub fn mark_painted_steel(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.painted_steel = true;
        }
    }
    /// On the raymarch backend the sky-visibility atlas serves only the periscope relays' views of
    /// the outside: leave the player's own tank out of it, so its turret and hull moving do not
    /// force the atlas's whole static layer (66 tiles of the scene, 88 ms) to be redrawn every
    /// frame (2026-10-28). The relayed views lose the tank's own shadow. No effect elsewhere.
    pub fn exclude_from_relay_visibility(&mut self, idx: usize) {
        if crate::graphics::raymarch_backend_enabled() {
            if let Some(g) = self.meshes.get_mut(idx) {
                g.sky_visibility_excluded = true;
            }
        }
    }
    /// Flag self-luminous mesh `idx` as a small-arms muzzle flash: the raymarch medium draws it at
    /// most 10 cm in radius (an MG 34's flash), not the size of its scaled mesh.
    pub fn mark_small_arms_flash(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.small_arms_flash = true;
        }
    }
    /// Flag mesh `idx` as one of the MG's legacy tracer streaks or impact spurts: the raymarch
    /// backend draws small-arms tracers and impacts from the analytic weapon records (one-pixel
    /// streaks, armour sparks, ground ejecta) and leaves these out. Drawn there, a 2.4 m tracer box
    /// became a glowing ball 2.4 m in radius and a spurt a flat glowing block (2026-10-28).
    pub fn mark_legacy_small_arms_fx(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.legacy_small_arms_fx = true;
        }
    }
    /// Flag glass mesh `idx` as a clear cover over an instrument dial: the raymarch glass pass
    /// gives it a broad sheen of the cabin's lamps and a faint fixed dust (`rm_glass.wgsl`).
    pub fn mark_instrument_cover(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.instrument_cover = true;
        }
    }
    /// Flag mesh `idx` as an OWN-COLOUR emissive that is NOT night-gated: a warm interior bulb or a
    /// kicked-up snow puff glows with its authored colour scaled by `set_emissive`, day or night.
    /// Signalled to the shader via params.w = 2.0 (own-colour, ungated) vs 1.0 (window, night-gated).
    pub fn mark_own_emissive(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.emissive_own = true;
        }
    }
    /// Keep mesh `idx` on the ordinary lit raster/PBR path while excluding it from the shared TLAS.
    /// This is intended for view-local geometry whose visibility may differ between split-screen
    /// viewports after the frame's one ray scene has already been built.
    pub fn mark_raster_only(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.raster_only = true;
        }
    }
    /// Only the optional source-model diagnostic requests hidden anatomy pages by explicit slot.
    /// Normal scene submission still follows the original `visible` and ray visibility state.
    pub fn diagnostic_mesh_lease(&self, slot: usize) -> Option<DiagnosticMeshLease> {
        self.meshes.get(slot).map(|mesh| DiagnosticMeshLease {
            vertices: mesh.vbuf.clone(), indices: mesh.ibuf.clone(), index_count: mesh.index_count,
            material: mesh.tex_bind.clone(), model: mesh.model, visible: mesh.visible,
        })
    }
    pub fn diagnostic_material_layout(&self) -> wgpu::BindGroupLayout {
        self.material_bgl.clone()
    }
    /// Register a live, fixed-resolution diagnostic texture without a GPU-to-CPU copy.
    pub fn import_diagnostic_image(&mut self, device: &wgpu::Device, view: &wgpu::TextureView) -> u32 {
        self.ui.import_nearest_image_view(device, view)
    }

    /// Compatibility no-op. Hidden geometry is never exposed through an x-ray render mode.
    pub fn set_xray(&mut self, idx: usize, x: bool) {
        let _ = (idx, x);
    }
    /// Compatibility no-op. A pasted secondary-camera texture is not optical transport; vision
    /// blocks remain opaque until their prism/glass geometry is traced physically.
    pub fn set_mesh_rtt_screen(&mut self, idx: usize) {
        if self.shader_backend { self.set_glass(idx, true); }
    }

    /// Compatibility no-op for the deleted pasted-camera periscope path.
    pub fn render_rtt_tile(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tile: u32) {
        let _ = (device, queue, tile);
    }

    /// Mark a closed direct-view protective-glass mesh for the High/Vulkan dielectric path.
    /// The forward fallback remains opaque: it cannot produce a truthful refracted scene without
    /// the ray-query surface tables, and a pasted screen texture is not a substitute.
    pub fn set_glass(&mut self, idx: usize, on: bool) {
        if let Some(mesh) = self.meshes.get_mut(idx) {
            mesh.glass = on;
        }
    }
    /// A periscope mirror's plane normal in its mesh's frame, for the raymarch ray queries.
    pub fn set_periscope_mirror_normal(&mut self, idx: usize, normal: glam::Vec3) {
        if let Some(mesh) = self.meshes.get_mut(idx) {
            mesh.mirror_normal = normal.normalize_or_zero();
        }
    }
    pub fn set_periscope_optical_kind(&mut self, idx: usize, kind: u32) {
        if let Some(mesh) = self.meshes.get_mut(idx) {
            mesh.periscope_kind = kind.min(2);
            mesh.periscope_focal_m = 0.0;
        }
    }
    /// Finite ideal relay lens; its center comes from the actual mesh and follows its model.
    /// The focal length is a declared reconstruction parameter, not recovered historical data.
    pub fn set_periscope_lens(&mut self, idx: usize, focal_m: f32) {
        if let Some(mesh) = self.meshes.get_mut(idx) {
            if focal_m.is_finite() && focal_m > 0.001 {
                mesh.periscope_kind = 3;
                mesh.periscope_focal_m = focal_m;
            }
        }
    }
    /// Set this frame's positioned point lights (modeled bulbs, muzzle flash, fire glow and night
    /// lamps). Cleared and refilled by game.rs each frame. If more than N_LIGHTS are
    /// offered, keep those that contribute most at the camera (see `select_point_lights`).
    pub fn extend_lights(&mut self, lights: &[PointLight]) {
        let combined: Vec<_> = self.lights_full.iter().chain(lights.iter()).copied().collect();
        self.set_lights(&combined);
    }

    pub fn set_lights(&mut self, lights: &[PointLight]) {
        self.lights_offered = lights.len();
        self.lights_full.clear();
        self.lights_full.extend_from_slice(&lights[..lights.len().min(raymarch::MAX_SCENE_LIGHTS)]);
        self.lights = select_point_lights(lights, self.camera.pos, N_LIGHTS);
    }

    /// (point lights offered for the latest frame, admitted within the `N_LIGHTS` budget).
    pub fn light_budget_stats(&self) -> (usize, usize) {
        (self.lights_offered, self.lights.len())
    }

    #[cfg(test)]
    pub(crate) fn admitted_lights_for_review(&self) -> &[PointLight] {
        &self.lights
    }
    /// Compatibility no-op. The deleted portable AO pass used an analytic heightfield proxy;
    /// hardware High now intersects the actual terrain triangles stored in the scene TLAS.
    pub fn upload_heightfield(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        res: usize,
        spacing: f32,
        half_extent: f32,
        heights: &[f32],
    ) {
        let _ = (device, queue, res, spacing, half_extent, heights);
    }

    /// Compatibility no-op. Dynamic tanks are represented by their exact BLAS/TLAS geometry.
    pub fn set_rt_boxes(&mut self, boxes: &[(glam::Vec3, glam::Vec3, f32)]) {
        let _ = boxes;
    }

    /// Flag mesh `idx` as TERRAIN — it samples the tank-track map (ruts darken + dip-shade it).
    pub fn mark_track_receiver(&mut self, idx: usize) {
        if let Some(g) = self.meshes.get_mut(idx) {
            g.track_recv = true;
        }
    }
    /// Set an instanced batch's shader flag (`params.w`). -1.0 = physical cubic smoke particles;
    /// 4.0 = grass wind/alpha material; 0.0 = plain lit module (default).
    pub fn set_batch_flags(&mut self, batch: usize, flags: f32) {
        if let Some(b) = self.inst_batches.get_mut(batch) {
            let rt_membership_changed = (b.flags <= 3.5) != (flags <= 3.5);
            b.flags = flags;
            if rt_membership_changed {
                self.rt_batches_dirty = true;
                if self.rt_on {
                    self.rt_surface_dirty = true;
                    self.combine_rt_bind = None;
                }
            }
        }
    }

    /// Register the exact rendered hull used by this rig. Its lowest 5 cm define the two
    /// tread-band envelopes; this is a measured rectangle approximation, not a wheel/soil solver.
    pub fn register_track_footprint(&mut self, parts: crate::tank::Parts, hull: &CpuMesh) {
        let key = parts.hull_half_extents.to_array().map(f32::to_bits);
        if let Some(footprint) = track_ruts::Footprint::from_hull(hull) {
            self.track_footprints.insert(key, footprint);
        }
    }

    /// Stamp the source rig's oriented tread contact bands at fixed world sample spacing.
    pub fn stamp_tank_tracks(&mut self, hull_model: glam::Mat4, parts: crate::tank::Parts, strength: f32) {
        if self.track_half <= 1.0 { return; }
        let key = parts.hull_half_extents.to_array().map(f32::to_bits);
        let Some(footprint) = self.track_footprints.get(&key).copied() else { return; };
        for quad in footprint.quads_for_model(hull_model) {
            self.track_atlas.stamp(quad, strength, &mut self.pending_stamps);
        }
    }

    /// Render the queued tread stamps into the track map (additive), then clear the queue.
    fn flush_track_stamps(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.pending_stamps.is_empty() {
            return;
        }
        for ((x, y), value) in self.track_atlas.new_pages.drain(..) {
            self.instance_queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.track_lookup, mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                &value.to_le_bytes(),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 });
        }
        let verts = std::mem::take(&mut self.pending_stamps);
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("track-stamps"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("track-stamp"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.track_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.track_pipe);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..(verts.len() as u32), 0..1);
    }

    /// Number of scene meshes (used by the headless fire demo to locate the legacy FX slots).
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Upload one immutable instanced mesh/material/BLAS prototype. Spatial pages created with
    /// `add_instanced_page` share these resources; the returned handle remains valid for the life of
    /// the renderer. Registering is intentionally separate from adding pages so an editor asset can
    /// exist with zero placements.
    /// The material bind group of an instanced prototype. Prototypes whose textures are the same
    /// decoded images (one `Arc` of texels: a building kit asset's rubble variants and damage/soot/
    /// snow variants share their source's or their atlas's image) share one upload; the cache
    /// holds the image `Arc`s, so an address is never reused for other texels while it lives.
    fn instanced_material(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
    ) -> Arc<wgpu::BindGroup> {
        let texels = |image: &Option<CpuImage>| image.as_ref().map(|i| i.rgba.clone());
        let held = [
            texels(&mesh.texture),
            texels(&mesh.metallic_roughness_texture),
            texels(&mesh.normal_texture),
        ];
        let key = held.clone().map(|t| t.map_or(0, |t| Arc::as_ptr(&t) as usize));
        if key[0] != 0 {
            if let Some((bind, _)) = self.inst_material_cache.get(&key) {
                return bind.clone();
            }
        }
        let bind = Arc::new(upload_material(
            device,
            queue,
            &self.material_bgl,
            &self.sampler,
            mesh,
        ));
        if key[0] != 0 {
            self.inst_material_cache
                .insert(key, (bind.clone(), held.into_iter().flatten().collect()));
        }
        bind
    }

    pub fn register_instanced_prototype(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
    ) -> InstancedPrototype {
        let bi = if self.rt_scene {
            wgpu::BufferUsages::BLAS_INPUT
        } else {
            wgpu::BufferUsages::empty()
        };
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst-vbuf"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX | bi,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("inst-ibuf"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX | bi,
        });
        // M-D: the prototype's BLAS, built immediately (add_instanced is a load/editor-time path).
        let blas = if self.rt_scene && mesh.indices.len() >= 3 && !mesh.vertices.is_empty() {
            let set = crate::rt::make_blas(
                device,
                mesh.vertices.len() as u32,
                mesh.indices.len() as u32,
            );
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("inst-blas"),
            });
            enc.build_acceleration_structures(
                std::iter::once(&crate::rt::blas_entry(&set, &vbuf, &ibuf)),
                std::iter::empty::<&wgpu::Tlas>(),
            );
            queue.submit([enc.finish()]);
            Some(set)
        } else {
            None
        };
        let tex_bind = self.instanced_material(device, queue, mesh);
        let handle = self.inst_prototypes.len();
        let visibility_geometry = (self.raster_lighting && self.portable_visibility && !mesh.indices.is_empty()).then(||
            self.scene_visibility.register_mesh(mesh).expect("invalid instanced visibility source"));
        self.inst_prototypes.push(InstPrototype {
            visibility_geometry,
            vbuf,
            ibuf,
            index_count: mesh.indices.len() as u32,
            tex_bind,
            metallic_factor: mesh.metallic_factor,
            roughness_factor: mesh.roughness_factor,
            local_min: mesh.min,
            local_max: mesh.max,
            blas,
            rt_source: self.rt_on.then(|| mesh.clone()),
        });
        if self.rt_on {
            self.rt_surface_dirty = true;
            self.combine_rt_bind = None;
        }
        handle
    }

    fn allocate_inst_buffer_id(&mut self) -> usize {
        let id = self.next_inst_buffer_id;
        self.next_inst_buffer_id = self
            .next_inst_buffer_id
            .checked_add(1)
            .expect("instance-buffer id overflow");
        id
    }

    /// Add a load group of spatial pages against one prototype. All pages share one packed instance
    /// buffer while retaining independent byte ranges, capacities, culling bounds and renderer batch
    /// handles. Returns one batch handle per input page, or `None` for an invalid prototype/empty
    /// page. Callers that know all initial pages should prefer this over repeated single-page calls.
    pub fn add_instanced_pages(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        pages: &[&[glam::Mat4]],
    ) -> Option<Vec<usize>> {
        self.add_instanced_pages_with_hidden(device, prototype, pages, &[])
    }

    /// As [`Self::add_instanced_pages`], with per-instance initial visibility for static pages whose
    /// instances appear and vanish one by one (building modules and their rubble). `hidden` is
    /// empty (all visible) or holds one slice per page, each empty or as long as its page.
    pub fn add_instanced_pages_with_hidden(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        pages: &[&[glam::Mat4]],
        hidden: &[&[bool]],
    ) -> Option<Vec<usize>> {
        let proto = self.inst_prototypes.get(prototype)?;
        if pages.is_empty() {
            return Some(Vec::new());
        }
        if !hidden.is_empty()
            && (hidden.len() != pages.len()
                || hidden
                    .iter()
                    .zip(pages)
                    .any(|(flags, page)| !flags.is_empty() && flags.len() != page.len()))
        {
            return None;
        }
        let hidden_of = |page: usize, instance: usize| {
            hidden
                .get(page)
                .and_then(|flags| flags.get(instance))
                .copied()
                .unwrap_or(false)
        };
        let (local_min, local_max) = (proto.local_min, proto.local_max);
        let lengths: Vec<_> = pages.iter().map(|page| page.len()).collect();
        let (ranges, total_bytes) = packed_page_ranges(&lengths)?;

        let mut object_id_base = self
            .inst_batches
            .last()
            .map(|previous| previous.object_id_base + previous.object_id_capacity)
            .unwrap_or(0);
        let mut plans = Vec::with_capacity(pages.len());
        for page in pages {
            let plan = plan_instanced_page(prototype, page, local_min, local_max, object_id_base)?;
            object_id_base = plan.object_id_end();
            plans.push(plan);
        }
        assert!(
            object_id_base <= 0x40_0000,
            "instanced objects exceed the unique 22-bit G-buffer/TLAS ID range"
        );

        let packed_count = usize::try_from(total_bytes / INSTANCE_MAT_BYTES)
            .expect("packed instance buffer exceeds addressable memory");
        let mut packed = vec![[[0.0f32; 4]; 4]; packed_count];
        for (page_index, ((page, range), plan)) in pages.iter().zip(&ranges).zip(&plans).enumerate() {
            let first = usize::try_from(range.byte_offset / INSTANCE_MAT_BYTES).unwrap();
            let count = plan.inst_count as usize;
            for (instance, (dst, transform)) in
                packed[first..first + count].iter_mut().zip(page.iter()).enumerate()
            {
                // A hidden instance enters the raster stream as the zero matrix (see `hidden`).
                *dst = if hidden_of(page_index, instance) {
                    [[0.0; 4]; 4]
                } else {
                    transform.to_cols_array_2d()
                };
            }
        }
        let inst_buf = Arc::new(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("inst-packed-pages"),
                contents: bytemuck::cast_slice(&packed),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
        );
        let inst_buffer_id = self.allocate_inst_buffer_id();
        let mut batches = Vec::with_capacity(pages.len());
        for (page_index, ((page, range), plan)) in pages.iter().zip(ranges).zip(plans).enumerate() {
            let batch = self.inst_batches.len();
            let mut entry = InstBatch {
                prototype: plan.prototype,
                inst_buf: Arc::clone(&inst_buf),
                inst_buffer_id,
                inst_byte_offset: range.byte_offset,
                inst_capacity: range.capacity,
                inst_count: plan.inst_count,
                world_min: plan.world_min,
                world_max: plan.world_max,
                flags: 0.0,
                emissive: 0.0,
                raster_only_dynamic: false,
                moving_rt_pool: false,
                cpu_mats: page.to_vec(),
                object_id_base: plan.object_id_base,
                object_id_capacity: plan.inst_count,
                hidden: InstanceVisibility::default(),
            };
            for instance in 0..plan.inst_count {
                if hidden_of(page_index, instance as usize) {
                    entry.set_hidden(instance, true);
                }
            }
            self.inst_batches.push(entry);
            self.inst_spatial_index.update(
                batch,
                Some(InstSpatialBounds {
                    min: plan.world_min,
                    max: plan.world_max,
                }),
            );
            batches.push(batch);
        }
        self.rt_batches_dirty = true;
        if self.rt_on {
            self.rt_surface_dirty = true;
            self.combine_rt_bind = None;
        }
        Some(batches)
    }

    /// Add one spatial page that references an existing prototype. This compatibility wrapper makes
    /// a one-page load group; manifest/editor/tree initial loads use `add_instanced_pages` so their
    /// thousands of pages share O(prototypes) buffers.
    pub fn add_instanced_page(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        transforms: &[glam::Mat4],
    ) -> Option<usize> {
        self.add_instanced_pages(device, prototype, &[transforms])?
            .into_iter()
            .next()
    }

    /// Compatibility path for existing non-manifest callers: one private prototype + one batch.
    /// Its batch-index behavior is unchanged. Campaign/editor loading uses the split API above so
    /// many spatial pages can share one upload.
    pub fn add_instanced(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
        transforms: &[glam::Mat4],
    ) {
        if transforms.is_empty() {
            return;
        }
        let prototype = self.register_instanced_prototype(device, queue, mesh);
        let _ = self.add_instanced_page(device, prototype, transforms);
    }

    /// Allocate a fixed-capacity, raster-only instance pool for rapidly moving transient FX.
    /// Geometry/material are shared, the matrix buffer is reserved once, and the page starts empty.
    /// Updates use `set_dynamic_instanced_mats`, which cannot grow the allocation and deliberately
    /// does not dirty the static scene TLAS. Add these pools after static world batches so their
    /// reserved object-ID ranges sit at the tail.
    pub fn add_dynamic_instanced_pool(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
        capacity: usize,
    ) -> Option<usize> {
        if capacity == 0 {
            return None;
        }
        let prototype = self.register_instanced_prototype(device, queue, mesh);
        let reserve = vec![glam::Mat4::IDENTITY; capacity];
        let batch = self.add_instanced_page(device, prototype, &reserve)?;
        let b = &mut self.inst_batches[batch];
        b.raster_only_dynamic = true;
        b.object_id_capacity = u32::try_from(capacity).ok()?;
        b.inst_count = 0;
        b.cpu_mats.clear();
        b.world_min = glam::Vec3::splat(f32::INFINITY);
        b.world_max = glam::Vec3::splat(f32::NEG_INFINITY);
        self.inst_spatial_index.update(batch, None);
        Some(batch)
    }

    /// Allocate an initially empty, fixed-capacity moving pool that remains ordinary physical
    /// geometry in both render tiers. The caller registers one prototype per authored pose and can
    /// then place up to `capacity` identical soldiers in that pose without another mesh upload.
    ///
    /// Unlike `add_dynamic_instanced_pool`, this pool is not raster-only: active instances cast
    /// forward shadows and occupy exact BLAS instances in the shared TLAS. The full capacity keeps
    /// its G-buffer/TLAS custom-data range reserved even while the pool is empty; only actual live
    /// instances occupy physical TLAS slots. Add pools after static campaign pages when possible.
    pub fn add_moving_instanced_pool(
        &mut self,
        device: &wgpu::Device,
        prototype: InstancedPrototype,
        capacity: usize,
    ) -> Option<usize> {
        let capacity = fixed_instanced_pool_capacity(capacity)?;
        self.inst_prototypes.get(prototype)?;
        let reserve = vec![glam::Mat4::IDENTITY; capacity as usize];
        let batch = self.add_instanced_page(device, prototype, &reserve)?;
        let b = &mut self.inst_batches[batch];
        b.moving_rt_pool = true;
        b.object_id_capacity = capacity;
        b.inst_count = 0;
        b.cpu_mats.clear();
        b.world_min = glam::Vec3::splat(f32::INFINITY);
        b.world_max = glam::Vec3::splat(f32::NEG_INFINITY);
        self.inst_spatial_index.update(batch, None);
        // add_instanced_page already dirtied the complete TLAS/surface topology. Keep the explicit
        // moving flag clean until a post-build transform update arrives.
        self.rt_moving_batches_dirty = false;
        Some(batch)
    }

    fn reassign_instanced_object_ids(&mut self) {
        const INSTANCED_ID_BIT: u32 = 0x40_0000;
        let mut next = 0u32;
        for batch in &mut self.inst_batches {
            batch.object_id_base = next;
            next = next
                .checked_add(batch.object_id_capacity)
                .expect("instanced object-id count overflow");
        }
        assert!(
            next <= INSTANCED_ID_BIT,
            "{next} instanced objects exceed the unique 22-bit G-buffer/TLAS ID range"
        );
        if self.rt_on {
            self.rt_surface_dirty = true;
            self.combine_rt_bind = None;
        }
    }

    /// Rewrite one page's transforms. Updates its packed range in place while it fits; if it grows
    /// beyond capacity, the page safely migrates to its own exponentially-sized replacement buffer.
    /// `transforms` may be empty — the draw loops skip zero-count pages and its range stays reusable.
    pub fn set_instanced_mats(
        &mut self,
        device: &wgpu::Device,
        batch: usize,
        transforms: &[glam::Mat4],
    ) {
        // Fixed pools must never escape their reserved buffer/ID ranges through the growable
        // compatibility setter. Their typed setters below report overflow instead of reallocating.
        if self
            .inst_batches
            .get(batch)
            .is_some_and(|current| current.raster_only_dynamic || current.moving_rt_pool)
        {
            return;
        }
        if self
            .inst_batches
            .get(batch)
            .map(|current| current.cpu_mats.as_slice() == transforms)
            .unwrap_or(false)
        {
            return;
        }
        let Some(prototype) = self.inst_batches.get(batch).map(|b| b.prototype) else {
            return;
        };
        let Some(proto) = self.inst_prototypes.get(prototype) else {
            return;
        };
        let (local_min, local_max) = (proto.local_min, proto.local_max);
        let new_count = u32::try_from(transforms.len()).expect("instance batch count exceeds u32");
        let (old_count, old_capacity, old_offset, old_buffer) = {
            let current = &self.inst_batches[batch];
            (
                current.inst_count,
                current.inst_capacity,
                current.inst_byte_offset,
                Arc::clone(&current.inst_buf),
            )
        };
        let count_changed = old_count != new_count;
        if count_changed {
            // Hidden bits address instance ordinals; bits past the new count no longer exist.
            self.inst_batches[batch].trim_hidden(new_count);
        }
        let mats: Vec<[[f32; 4]; 4]> = {
            let current = &self.inst_batches[batch];
            transforms
                .iter()
                .enumerate()
                .map(|(instance, m)| current.raster_matrix(instance as u32, *m))
                .collect()
        };
        if new_count > old_capacity {
            let capacity = instance_page_capacity(new_count)
                .expect("non-empty growing instance page capacity overflow");
            let mut replacement = vec![[[0.0f32; 4]; 4]; capacity as usize];
            replacement[..mats.len()].copy_from_slice(&mats);
            let inst_buf = Arc::new(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("inst-grown-page"),
                    contents: bytemuck::cast_slice(&replacement),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                }),
            );
            let inst_buffer_id = self.allocate_inst_buffer_id();
            let b = &mut self.inst_batches[batch];
            b.inst_buf = inst_buf;
            b.inst_buffer_id = inst_buffer_id;
            b.inst_byte_offset = 0;
            b.inst_capacity = capacity;
        } else if !mats.is_empty() {
            self.instance_queue
                .write_buffer(&old_buffer, old_offset, bytemuck::cast_slice(&mats));
        }
        let b = &mut self.inst_batches[batch];
        b.inst_count = new_count;
        b.cpu_mats = transforms.to_vec(); // keeps TLAS instance writes honest (tree falls etc.)
        if count_changed {
            b.object_id_capacity = new_count;
            self.reassign_instanced_object_ids();
        }
        self.rt_batches_dirty = true;
        let mut wmin = glam::Vec3::splat(f32::INFINITY);
        let mut wmax = glam::Vec3::splat(f32::NEG_INFINITY);
        for t in transforms {
            let (mn, mx) = transform_aabb(*t, local_min, local_max);
            wmin = wmin.min(mn);
            wmax = wmax.max(mx);
        }
        let b = &mut self.inst_batches[batch];
        b.world_min = wmin;
        b.world_max = wmax;
        self.inst_spatial_index.update(
            batch,
            (!transforms.is_empty()).then_some(InstSpatialBounds {
                min: wmin,
                max: wmax,
            }),
        );
    }

    /// Rewrite a preallocated raster-only FX pool without reallocating, renumbering static object
    /// IDs, or marking the static instanced TLAS tail dirty. Returns false if the handle is not a
    /// dynamic pool or the caller exceeded its fixed capacity.
    pub fn set_dynamic_instanced_mats(&mut self, batch: usize, transforms: &[glam::Mat4]) -> bool {
        let Some(current) = self.inst_batches.get(batch) else {
            return false;
        };
        if !current.raster_only_dynamic || transforms.len() > current.object_id_capacity as usize {
            return false;
        }
        if current.cpu_mats.as_slice() == transforms {
            return true;
        }
        if !transforms.is_empty() {
            // glam's bytemuck feature guarantees the same packed four-column layout consumed
            // by the instance vertex buffer. Avoid a fresh staging Vec for every wind tick.
            self.instance_queue.write_buffer(
                &current.inst_buf,
                current.inst_byte_offset,
                bytemuck::cast_slice(transforms),
            );
        }
        let proto = &self.inst_prototypes[current.prototype];
        let (world_min, world_max) = dynamic_instanced_bounds(
            transforms,
            proto.local_min,
            proto.local_max,
            current.flags > 3.5 && current.flags < 4.5,
        );
        let b = &mut self.inst_batches[batch];
        b.inst_count = transforms.len() as u32;
        // The pool reserved this capacity at creation; preserve it across empty/partial updates.
        b.cpu_mats.clear();
        b.cpu_mats.extend_from_slice(transforms);
        b.world_min = world_min;
        b.world_max = world_max;
        self.inst_spatial_index.update(
            batch,
            (!transforms.is_empty()).then_some(InstSpatialBounds {
                min: world_min,
                max: world_max,
            }),
        );
        true
    }

    /// Rewrite a fixed-capacity, ray-visible moving pool. This is a queue upload only: it never
    /// takes a device, reallocates a buffer, changes object IDs, or rebuilds the exact source pack.
    /// Empty updates are valid and remove every active instance from the TLAS; overflow, invalid
    /// handles, and handles of another batch kind are rejected without mutating the pool.
    pub fn set_moving_instanced_mats(&mut self, batch: usize, transforms: &[glam::Mat4]) -> bool {
        let Some(current) = self.inst_batches.get(batch) else {
            return false;
        };
        if !current.moving_rt_pool
            || fixed_instanced_pool_update(current.object_id_capacity, transforms.len()).is_none()
        {
            return false;
        }
        if current.cpu_mats.as_slice() == transforms {
            return true;
        }
        let count_changed = current.inst_count != transforms.len() as u32;
        if !transforms.is_empty() {
            let mats: Vec<[[f32; 4]; 4]> =
                transforms.iter().map(|m| m.to_cols_array_2d()).collect();
            self.instance_queue.write_buffer(
                &current.inst_buf,
                current.inst_byte_offset,
                bytemuck::cast_slice(&mats),
            );
        }
        let mut world_min = glam::Vec3::splat(f32::INFINITY);
        let mut world_max = glam::Vec3::splat(f32::NEG_INFINITY);
        let proto = &self.inst_prototypes[current.prototype];
        for transform in transforms {
            let (min, max) = transform_aabb(*transform, proto.local_min, proto.local_max);
            world_min = world_min.min(min);
            world_max = world_max.max(max);
        }
        let b = &mut self.inst_batches[batch];
        b.inst_count = transforms.len() as u32;
        b.cpu_mats = transforms.to_vec();
        b.world_min = world_min;
        b.world_max = world_max;
        self.inst_spatial_index.update(
            batch,
            (!transforms.is_empty()).then_some(InstSpatialBounds {
                min: world_min,
                max: world_max,
            }),
        );
        // Custom-data IDs/source records stay fixed. A count change compacts the physical layout;
        // transform-only motion can still update just the existing moving ranges.
        self.rt_batches_dirty |= count_changed;
        self.rt_moving_batches_dirty = true;
        true
    }

    /// Set the emissive intensity of a raster-only dynamic pool. Invalid/static batch handles and
    /// non-finite values are rejected; negative intensities are safely clamped to zero.
    pub fn set_dynamic_instanced_emissive(&mut self, batch: usize, intensity: f32) -> bool {
        let Some(current) = self.inst_batches.get_mut(batch) else {
            return false;
        };
        if !current.raster_only_dynamic || !intensity.is_finite() {
            return false;
        }
        current.emissive = intensity.max(0.0);
        true
    }

    /// Show or hide one instance of a static page (a building module or its rubble). The instance
    /// keeps its object ID and exact source record; while hidden it is the zero matrix in the raster
    /// stream and takes no TLAS slot. Returns false for an invalid handle or a moving/dynamic pool.
    pub fn set_instance_visible(&mut self, handle: InstanceRef, visible: bool) -> bool {
        let rt_member = {
            let Some(b) = self.inst_batches.get_mut(handle.batch) else {
                return false;
            };
            if b.raster_only_dynamic || b.moving_rt_pool || handle.instance >= b.inst_count {
                return false;
            }
            if !b.set_hidden(handle.instance, !visible) {
                return true;
            }
            let model = b.cpu_mats[handle.instance as usize];
            let entry = b.raster_matrix(handle.instance, model);
            let offset = b.inst_byte_offset + u64::from(handle.instance) * INSTANCE_MAT_BYTES;
            self.instance_queue
                .write_buffer(&b.inst_buf, offset, bytemuck::bytes_of(&entry));
            b.flags <= 3.5
        };
        if rt_member {
            // Visible counts define the compact TLAS layout; the source records do not change.
            self.rt_batches_dirty = true;
        }
        true
    }

    /// Move one instance of a static page: a failed building module's rubble mound, laid where it
    /// fell (`BuildingSet::settle_rubble`). The instance keeps its object ID and exact source
    /// record; its raster entry is rewritten in place (still the zero matrix while hidden), the
    /// page's bounds and spatial cells grow to hold it, and the ray scene's static tail is rewritten
    /// from the CPU transforms. Returns false for an invalid handle, a dynamic pool or a non-finite
    /// transform.
    pub fn set_instance_matrix(&mut self, handle: InstanceRef, model: glam::Mat4) -> bool {
        if !model.is_finite() {
            return false;
        }
        let (prototype, rt_member) = {
            let Some(b) = self.inst_batches.get_mut(handle.batch) else {
                return false;
            };
            if b.raster_only_dynamic || b.moving_rt_pool || handle.instance >= b.inst_count {
                return false;
            }
            if b.cpu_mats[handle.instance as usize] == model {
                return true;
            }
            b.cpu_mats[handle.instance as usize] = model;
            let entry = b.raster_matrix(handle.instance, model);
            let offset = b.inst_byte_offset + u64::from(handle.instance) * INSTANCE_MAT_BYTES;
            self.instance_queue
                .write_buffer(&b.inst_buf, offset, bytemuck::bytes_of(&entry));
            (b.prototype, b.flags <= 3.5)
        };
        let Some(proto) = self.inst_prototypes.get(prototype) else {
            return false;
        };
        let (min, max) = transform_aabb(model, proto.local_min, proto.local_max);
        let b = &mut self.inst_batches[handle.batch];
        b.world_min = b.world_min.min(min);
        b.world_max = b.world_max.max(max);
        let bounds = InstSpatialBounds { min: b.world_min, max: b.world_max };
        self.inst_spatial_index.update(handle.batch, Some(bounds));
        if rt_member {
            self.rt_batches_dirty = true;
        }
        true
    }

    /// Visibility of one instance of a static page; `None` for an invalid handle.
    pub fn instance_visible(&self, handle: InstanceRef) -> Option<bool> {
        let b = self.inst_batches.get(handle.batch)?;
        (handle.instance < b.inst_count).then(|| !b.is_hidden(handle.instance))
    }

    /// Show or hide a loose scene mesh or one instance of a static page.
    pub fn set_ref_visible(&mut self, handle: SceneRef, visible: bool) {
        match handle {
            SceneRef::Mesh(slot) => self.set_visible(slot, visible),
            SceneRef::Instance(instance) => {
                let applied = self.set_instance_visible(instance, visible);
                debug_assert!(applied, "invalid instanced scene handle {instance:?}");
            }
            SceneRef::None => {}
        }
    }

    /// Give a static page the own-colour emissive material a loose mesh gets from
    /// `mark_own_emissive` + `set_emissive`: its authored colour glows, day or night, in raster
    /// (params.w = 2) and as a base-colour ungated emitter in the ray scene.
    pub fn set_batch_own_emissive(&mut self, batch: usize, intensity: f32) -> bool {
        let Some(b) = self.inst_batches.get_mut(batch) else {
            return false;
        };
        if b.raster_only_dynamic || b.moving_rt_pool || !intensity.is_finite() {
            return false;
        }
        b.flags = 2.0;
        b.emissive = intensity.max(0.0);
        true
    }

    /// True when a page's instances enter the ray scene: a triangle prototype (with its BLAS on a
    /// ray-tracing device), not grass and not a raster-only FX pool. Without hardware ray tracing
    /// the same rule tells what the TLAS would hold, so both backends report one demand.
    fn batch_in_ray_scene(&self, batch: &InstBatch) -> bool {
        let prototype = &self.inst_prototypes[batch.prototype];
        let geometry = if self.rt_scene {
            prototype.blas.is_some()
        } else {
            prototype.index_count >= 3
        };
        geometry && batch.flags <= 3.5 && !batch.raster_only_dynamic
    }

    /// The TLAS demand of the current scene state (see [`TlasDemand`]).
    pub fn tlas_demand(&self) -> TlasDemand {
        let mut demand = TlasDemand {
            mesh_slots: u32::try_from(self.meshes.len()).unwrap_or(u32::MAX),
            capacity: TLAS_MAX_INSTANCES,
            ..TlasDemand::default()
        };
        for batch in &self.inst_batches {
            if !self.batch_in_ray_scene(batch) {
                continue;
            }
            demand.instanced_live = demand.instanced_live.saturating_add(batch.rt_live_count());
            demand.instanced_reserved =
                demand.instanced_reserved.saturating_add(batch.object_id_capacity);
        }
        demand
    }

    /// TLAS overflow bookkeeping of the frames rendered so far: (frames with an overflow, the
    /// instances missing from the ray scene in the latest frame). Both zero in a healthy scene.
    pub fn tlas_overflow_stats(&self) -> (u64, u32) {
        (self.rt_overflow_frames, self.rt_overflow_now)
    }

    /// Number of instanced batches + total instances (telemetry for the worldgen build log).
    pub fn instanced_stats(&self) -> (usize, u32) {
        (
            self.inst_batches.len(),
            self.inst_batches.iter().map(|b| b.inst_count).sum(),
        )
    }

    /// Unique prototype uploads, live packed instance buffers, spatial/dynamic pages, instances.
    pub fn instanced_paging_stats(&self) -> (usize, usize, usize, u32) {
        let (pages, instances) = self.instanced_stats();
        let buffers = self
            .inst_batches
            .iter()
            .map(|batch| batch.inst_buffer_id)
            .collect::<HashSet<_>>()
            .len();
        (self.inst_prototypes.len(), buffers, pages, instances)
    }

    /// Occupied 128 m cells, cell-to-page references (a page may touch neighbours), and broad
    /// legacy fallback pages. This is load-time telemetry for verifying that render culling scales
    /// with world cells rather than the number of prototype pages.
    pub fn instanced_spatial_stats(&self) -> (usize, usize, usize) {
        self.inst_spatial_index.stats()
    }

    // ── Main-menu turntable Tiger ────────────────────────────────────────────────────────────────

    /// Upload the showcase meshes for the main-menu turntable — the Tiger hull/turret/gun triple from
    /// `tank::build_tiger()`. Kept in a SEPARATE list from the scene `meshes`, so the menu render path
    /// owns this Tiger and never has to reach into `Game`. Replaces any previously-loaded menu meshes.
    pub fn load_menu_meshes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        meshes: &[CpuMesh],
    ) {
        let mut material_cache = MaterialCache::default();
        self.menu_meshes = meshes
            .iter()
            .map(|m| {
                let mut gpu=make_mesh_gpu(
                    device,
                    queue,
                    &self.material_bgl,
                    &self.sampler,
                    m,
                    false,
                    false,
                    &mut material_cache,
                );
                if self.raster_lighting && self.portable_visibility && !m.indices.is_empty() {
                    gpu.visibility_geometry=Some(self.scene_visibility.register_mesh(m).expect("invalid menu visibility source"));
                }
                gpu
            })
            .collect();
    }

    /// Aim the internal menu camera at the turntable Tiger and pose it (hull/turret/gun as one rigid
    /// body) spun by `angle` radians about its vertical axis, parked in the RIGHT third of the frame.
    /// Drive `angle` off a frame counter for a slow, continuous showroom spin.
    pub fn set_menu_tiger(&mut self, angle: f32) {
        self.camera.pos = MENU_CAM_POS;
        self.camera.set_forward(MENU_CAM_TARGET - MENU_CAM_POS);
        self.camera.fov_y = MENU_FOV_DEG.to_radians();
        self.camera.aspect = self.iw as f32 / self.ih as f32;
        self.fog_density = 0.0;
        self.night = 0.0;
        self.reticle = false;
        // Push the whole tank toward screen-right along the camera's right vector, then spin in place
        // about its own vertical centreline (the baked hull is centred on X/Z, origin at ground).
        let shift = self.camera.right() * MENU_RIGHT_SHIFT;
        let base = glam::Mat4::from_translation(shift) * glam::Mat4::from_rotation_y(angle);
        let p = crate::tank::TIGER_PARTS;
        let hull = base;
        let turret = hull * glam::Mat4::from_translation(p.turret_pivot);
        let gun = turret * glam::Mat4::from_translation(p.gun_trunnion);
        if let Some(m) = self.menu_meshes.get_mut(0) {
            m.model = hull;
        }
        if let Some(m) = self.menu_meshes.get_mut(1) {
            m.model = turret;
        }
        if let Some(m) = self.menu_meshes.get_mut(2) {
            m.model = gun;
        }
        for m in &mut self.menu_meshes {
            (m.world_min,m.world_max)=transform_aabb(m.model,m.local_min,m.local_max);
        }
    }

    /// Render the menu turntable: the showcase Tiger on a PURE-BLACK HDR frame (no sky/fog/snow/bloom
    /// — just the lit, spinning tank), crushed to the internal LDR buffer so it reads as a posterized/
    /// dithered pixel model. The caller then composites the front-end text over the LDR result.
    fn render_menu_internal(&self, encoder: &mut wgpu::CommandEncoder) {
        // The turntable is lit by the sky of the clock like any other surface: the probe
        // (prepared by upload_uniforms at the menu camera) carries the key beam and the sky.
        let mut work = RenderWorkStats::default();
        self.cabin_shadows.encode(self,encoder,&mut work);
        self.sky_visibility.encode(self,encoder,&mut work);
        self.environment_probe.encode(encoder, &mut work);
        // Legacy forward consumers still require a depth map. Portable source visibility reads
        // the actual menu triangles, so producing an unused map would duplicate geometry work.
        for near in [false, true] {
            if self.raster_lighting || near { continue; }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if near { &self.near_shadow_view } else { &self.shadow_view },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, if near { &self.near_shadow_camera_bind } else { &self.camera_bind }, &[]);
            for m in &self.menu_meshes {
                if !m.visible {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [0.0, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }
        // Mesh onto a pure-black HDR frame (no sky pass → the backdrop stays black through the crush).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-mesh"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.mesh_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]);
            pass.set_bind_group(3, &self.track_bind, &[]);
            for m in &self.menu_meshes {
                if !m.visible {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, m.tex_bind.as_ref(), &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
        }
        // Crush HDR → LDR (posterize + dither + nearest, exactly like the game frame).
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-crush"),
                color_attachments: &[Some(attachment(
                    &self.ldr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.crush_pipe);
            pass.set_bind_group(0, &self.crush_bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Drive the PRIMARY volumetric smoke plume (slot 0). Call this every frame while the muzzle
    /// plume lives: `pos`/`dir` are the muzzle world position + gun forward (`Tank::muzzle()`),
    /// `age` is seconds since the shot fired, `strength` is 0..1 (1 while active). The plume grows,
    /// lofts up out of the sightline, and dissipates by ~0.9 s on its own — pass `strength = 0`
    /// (or call `clear_smoke`) once the FX timer expires so the slot goes inactive. Replaces the old
    /// scaling-box smoke: the game drives THIS instead of `set_visible`/`set_model` on the FX box.
    pub fn set_smoke(&mut self, pos: glam::Vec3, dir: glam::Vec3, age: f32, strength: f32) {
        self.set_smoke_slot(0, pos, dir, age, strength);
    }

    /// Drive an arbitrary plume slot (0..MAX_PUFFS). Lets rapid/overlapping fire stack a couple of
    /// plumes; most callers only need `set_smoke` (slot 0).
    pub fn set_smoke_slot(
        &mut self,
        slot: usize,
        pos: glam::Vec3,
        dir: glam::Vec3,
        age: f32,
        strength: f32,
    ) {
        if let Some(p) = self.smoke_puffs.get_mut(slot) {
            *p = PuffGpu {
                pos: pos.to_array(),
                age,
                dir: dir.normalize_or_zero().to_array(),
                strength,
                shape: [0.0;4],
            };
        }
    }

    /// Add a soft participating-medium support; bounded shared pool, no opaque particles.
    pub fn add_smoke_volume(&mut self, center: glam::Vec3, radius: f32, density: f32, phase: f32, albedo: f32) {
        if let Some(p) = self.smoke_puffs.iter_mut().find(|p|p.strength<=0.0) {
            *p=PuffGpu {pos:center.to_array(),age:0.0,dir:[0.0;3],strength:1.0,shape:[radius.max(0.02),density.max(0.0),phase,albedo.clamp(0.0,1.0)]};
        }
    }

    /// Source-connected support for a sustained plume. A density/Jacobian pair preserves
    /// the existing sphere's integrated optical mass; transient muzzle spheres stay unchanged.
    pub fn add_smoke_sweep(&mut self,start:glam::Vec3,end:glam::Vec3,radius:f32,density:f32,phase:f32,albedo:f32) {
        let support=smoke_support::from_sphere(start,end,radius,density);
        if let Some(p)=self.smoke_puffs.iter_mut().find(|p|p.strength<=0.0) {
            *p=PuffGpu {pos:support.center.to_array(),age:0.0,dir:support.half_axis.to_array(),strength:1.0,
                shape:[support.radius,support.density,phase,albedo.clamp(0.0,1.0)]};
        }
    }

    /// Drive the small, translucent propellant-gas volume that leaves the rear of an opened
    /// breech. Unlike `set_smoke_slot`, this uses a sub-metre elongated shape in smoke.wgsl and a
    /// much lower density envelope, so it cannot turn into an exterior blast cloud indoors.
    pub fn set_breech_gas(&mut self, pos: glam::Vec3, dir: glam::Vec3, age: f32, strength: f32) {
        self.breech_gas = PuffGpu {
            pos: pos.to_array(),
            age,
            dir: dir.normalize_or_zero().to_array(),
            strength: strength.clamp(0.0, 1.0),
            shape: [0.0;4],
        };
    }

    /// Deactivate all volumetric plumes (no draw cost until a plume is set again).
    pub fn clear_smoke(&mut self) {
        for p in &mut self.smoke_puffs {
            p.strength = 0.0;
        }
        self.breech_gas.strength = 0.0;
    }

    /// Register the real articulated vehicle frame, independently of the observing camera.
    /// Geometry is loaded once from the named source armor shells; poses only transform planes.
    pub fn set_cabin_medium_pose(&mut self, hull:glam::Mat4, turret:glam::Mat4) {
        if self.cabin_medium_pose==Some((hull,turret)) {return;}
        let medium=self.cabin_medium.get_or_insert_with(||std::sync::Arc::new(
            cabin_medium::Medium::load().expect("source Tiger atmosphere domains")));
        self.cabin_medium_uniform=medium.uniform(hull,turret);
        self.cabin_medium_pose=Some((hull,turret));
    }

    /// Interior hazy atmosphere: `strength` scales the base haze density (0 = off — the pass
    /// skips), `dust` the drifting-mote sparkle. Exterior lighting is deliberately absent here:
    /// modeled openings, sun shadows/rays and authored fixtures are the only lighting paths.

    pub fn set_interior_atmosphere(&mut self, strength: f32, dust: f32, origin: glam::Vec3) {
        self.atmo_haze = strength.clamp(0.0, 1.0);
        self.atmo_dust = dust.clamp(0.0, 1.0);
        self.atmo_enabled = self.atmo_haze > 0.001;
        self.atmo_origin = origin;
    }

    /// The eye's inside state still selects its texture probe. Aerosol occupies world-space
    /// domains and remains visible through an aperture from any camera/station.
    pub fn set_interior_atmosphere_enabled(&mut self, enabled: bool) {
        self.live_texel_probe_view_inside = enabled;
        self.atmo_enabled = self.atmo_haze > 0.001;
    }

    pub fn interior_atmosphere_enabled(&self) -> bool {
        self.atmo_enabled
    }

    /// Start one split-screen frame's shared sun cascade. The caller supplies the physical tank
    /// centre rather than a camera heading, so commander, gunner, driver and loader all lie inside
    /// one conservative footprint even when their views face opposite directions.
    pub fn begin_coop_shadow_frame(&mut self, tank_center: glam::Vec3) {
        debug_assert!(tank_center.is_finite());
        self.coop_shadow.begin(self.sun_dir, tank_center);
        // Capture complete camera/light support before an interior view narrows its draw
        // frustum. The gunner optic and surrounding cabin share one source projection.
        self.sky_visibility.begin_frame(tank_center, &self.camera);
    }

    /// Later views in the active co-op frame reuse the first view's depth map and exact light VP.
    /// Returns the previous reuse state so a nested gunner eyepiece pass can restore its caller.
    pub fn set_coop_shadow_reuse(&mut self, reuse: bool) -> bool {
        self.coop_shadow.set_reuse(reuse)
    }

    /// End the split-screen frame and invalidate its cached matrix. This is deliberately explicit:
    /// subsequent RTT, solo, menu or next-frame work falls straight back to its own camera cascade.
    pub fn end_coop_shadow_frame(&mut self) {
        self.coop_shadow.end();
        self.sky_visibility.end_frame();
    }

    /// True when there is a modeled volume to integrate. The retired screen-space optic veil does
    /// not keep this pass alive.
    fn smoke_active(&self) -> bool {
        self.fog_density > 1.0e-6
            || (self.atmo_enabled && self.atmo_haze > 0.001)
            || (self.breech_gas.strength > 0.0 && self.breech_gas.age < 2.2)
            || self
                .smoke_puffs
                .iter()
                .any(|p| p.strength > 0.0 && p.age < 1.0)
    }

    /// Sun's far shadow cascade: a 480 m box centred just ahead of the camera (so what the view
    /// sees is what the map covers), looking down the sun ray. glam's `orthographic_rh` gives NDC
    /// z ∈ [0,1] to match wgpu, exactly like the perspective camera.
    fn light_view_proj(&self) -> glam::Mat4 {
        let focus = self.camera.pos + self.camera.forward() * SOLO_SHADOW_LOOK_AHEAD_M;
        let fallback = if self.raster_lighting {
            stable_shadow_view_proj(self.sun_dir,focus,SOLO_SHADOW_HALF_M,FAR_SHADOW_DEPTH_HALF_M,self.shadow_res)
        } else { shadow_view_proj_for_focus(
            self.sun_dir,
            focus,
            SOLO_SHADOW_HALF_M,
        ) };
        self.coop_shadow.view_proj_or(fallback)
    }

    fn near_shadow_uniform(&self) -> NearShadowUniform {
        let half = if self.interior_light > 0.02 { 6.0 } else { 20.0 };
        NearShadowUniform {
            view_proj: stable_shadow_view_proj(self.sun_dir,self.camera.pos,half,60.0,
                self.near_shadow_res).to_cols_array_2d(),
            scale: [half*2.0/self.near_shadow_res as f32,119.9,
                if self.coop_shadow.view_proj.is_some() { COOP_SHADOW_HALF_M*2.0 } else { SOLO_SHADOW_HALF_M*2.0 }
                    /self.shadow_res as f32,FAR_SHADOW_DEPTH_HALF_M*2.0-0.1],
        }
    }

    fn upload_pixel_particles(&self, queue: &wgpu::Queue, atmosphere: &AtmosphereParameters,
        lights: &[[f32;4];N_LIGHTS*2], medium: WorldMediumProfile,
        quality: IntegrationQuality) {
        if self.pixel_particle_pipe.is_none() { self.pixel_particle_count.set(0); return; }
        let gust = weather_gust(self.time, self.weather_seed);
        let wind = glam::Vec3::new(self.weather_wind_world.x,0.0,self.weather_wind_world.y)*gust;
        let dust = if self.atmo_enabled { self.atmo_dust } else { 0.0 };
        let mut field = self.pixel_particle_field.borrow_mut();
        // The cabin's dust rides the hull: its sphere turns, pitches and rolls with the vehicle
        // (it was translated only, so turning the tank swirled it through the crew; 2026-10-28).
        let dust_model = self.cabin_medium_pose.map_or(glam::Mat4::from_translation(self.atmo_origin), |(hull, _)| {
            let (_, rotation, _) = hull.to_scale_rotation_translation();
            glam::Mat4::from_rotation_translation(rotation, self.atmo_origin)
        });
        let points = field.update(self.camera.pos,wind,self.time,self.precipitation,
            self.atmo_origin,dust,dust_model);
        self.pixel_particle_count.set(points.len() as u32);
        if points.is_empty() { return; }
        queue.write_buffer(&self.pixel_particle_vertices,0,bytemuck::cast_slice(points));
        let mut sky_directions = [[0.0;4];6];
        let mut sky_radiances = [[0.0;4];6];
        let mut solar = glam::DVec3::ZERO;
        if dust > 0.0 && !self.shader_backend {
            // Only distant atmospheric boundary radiance is cached at the view origin; the short
            // source connection and all roof/armor/glass visibility are evaluated at each mote.
            let origin = self.camera.pos.as_dvec3();
            solar = atmosphere.solar_irradiance * physical_environment::solar_transmittance_at(
                atmosphere,origin,atmosphere.sun_direction,quality.source_steps);
            for i in 0..6 {
                let y = (i as f64+0.5)/6.0;
                let phi = i as f64 * 2.399963229728653;
                let radial = (1.0-y*y).sqrt();
                let direction = glam::DVec3::new(radial*phi.cos(),y,radial*phi.sin());
                let incoming = physical_environment::integrate_environment(atmosphere,origin,
                    direction,f64::INFINITY,quality).inscattered_radiance;
                sky_directions[i] = [direction.x as f32,direction.y as f32,direction.z as f32,0.0];
                sky_radiances[i] = [incoming.x as f32,incoming.y as f32,incoming.z as f32,0.0];
            }
        }
        // Shader-backend flakes are lit by the environment probe's mean sky and ground radiance
        // and the key beam, copied into this uniform on the GPU after the probe runs (render_internal).
        let velocity = crate::pixel_particles::snow_velocity(wind);
        let uniform = PixelParticleUniform {
            view_proj:self.camera.relative_view_proj().to_cols_array_2d(),
            camera:[self.camera.pos.x,self.camera.pos.y,self.camera.pos.z,
                if self.shader_backend { self.weather_shelter.clamp(0.0,1.0) } else { 0.0 }],
            viewport:[self.iw as f32,self.ih as f32,0.16,self.lights.len().min(N_LIGHTS) as f32],
            snow_velocity:[velocity.x,velocity.y,velocity.z,0.0],
            sun_direction:[self.sun_dir.x,self.sun_dir.y,self.sun_dir.z,0.0],
            solar_irradiance:[solar.x as f32,solar.y as f32,solar.z as f32,0.0],
            sky_directions,sky_radiances,lights:*lights,
            world_medium:[self.fog_density.max(0.0),medium.layer[0],medium.layer[1],
                if self.atmo_enabled {self.atmo_haze*0.010*5.2}else{0.0}],
            cabin_origin:[self.atmo_origin.x,self.atmo_origin.y,self.atmo_origin.z,
                0.0],
            cabin_medium:self.cabin_medium_uniform,
            medium_transport:[medium.snow_fraction,self.weather_offset.x,self.weather_offset.y,self.weather_offset.z],
        };
        queue.write_buffer(&self.pixel_particle_uniform,0,bytemuck::bytes_of(&uniform));
    }

    pub fn set_weather_override(&mut self, value:Option<f32>) {
        if value==self.weather_override {return;}
        if self.weather_override.is_none() && value.is_some() {
            self.weather_original=Some((self.precipitation,self.fog_density,self.overcast,self.storm,self.weather_wind_world));
        }
        if value.is_none() {
            if let Some((snow,fog,cloud,storm,wind))=self.weather_original.take() {
                self.precipitation=snow;self.fog_density=fog;self.overcast=cloud;self.storm=storm;self.weather_wind_world=wind;
            }
        }
        self.weather_override=value;
    }
    fn apply_weather_override(&mut self) {
        if let Some(intensity)=self.weather_override {
            let weather=crate::storm_weather::sample(intensity,self.time);
            self.precipitation=weather.snow;self.fog_density=weather.extinction;
            self.overcast=weather.cloud;self.storm=weather.storm;
            let direction=self.weather_wind_world.try_normalize().unwrap_or(glam::Vec2::new(0.8,0.6));
            self.weather_wind_world=direction*weather.wind;
        }
    }
    fn upload_uniforms(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.apply_weather_override();
        self.prepare_scene_visibility(device,queue);
        // Actor topology can change between frames. Refresh source/instance tables before writing
        // per-frame material state, not later in rt_update after that write already used old sizes.
        self.rt_refresh_actor_lookup(queue);
        self.rt_rebuild_surface_pack(device);
        if let Some(buffer) = &self.bloom_portal_buf {
            // The actual interior-view gate stays true while a hatch is open, and is cleared
            // for outside cameras, driver's wide objective, and the gunner's sight snapshot.
            // This is independent of haze strength and of the fixed camera exposure.
            let inside = self.live_texel_probe_view_inside && !self.menu_mode;
            let portal = BloomPortalUniform { depth_and_view: [
                self.camera.znear, self.camera.zfar, if inside {1.0} else {0.0}, 0.0,
            ] };
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&portal));
        }
        // Interleaved [px,py,pz,radius] / [r,g,b,intensity] per light, shared by the mesh and smoke
        // passes so both see the same positioned lights.
        let packed_lights = {
            let mut a = [[0.0f32; 4]; N_LIGHTS * 2];
            for (i, l) in self.lights.iter().take(N_LIGHTS).enumerate() {
                a[i * 2] = [l.pos.x, l.pos.y, l.pos.z, l.source_radius.max(0.005)];
                a[i * 2 + 1] = [
                    l.color.x,
                    l.color.y,
                    l.color.z,
                    l.radiant_intensity.max(0.0),
                ];
            }
            a
        };
        let n_lights = self.lights.len().min(N_LIGHTS) as f32;
        let world_medium = world_medium_profile(self.precipitation, self.storm);
        let cam = CameraUniform {
            view_proj: self.camera.relative_view_proj().to_cols_array_2d(),
            light_view_proj: self.light_view_proj().to_cols_array_2d(),
            cam_pos: self.camera.pos.to_array(),
            fog_density: self.fog_density,
            sun_dir: self.sun_dir.normalize().to_array(),
            night: self.night.clamp(0.0, 1.0),
            snap_res: [self.iw as f32, self.ih as f32],
            interior_light: self.interior_light.clamp(0.0, 1.0),
            time: self.time,
            track_half: self.track_half,
            n_lights,
            _pad2: (self.frame & 0xFFFF) as f32, // integrator jitter seed (combine.wgsl)
            // Runtime visibility prevents recursive unrolling of the glass world-medium march.
            _pad3: self.transport_quality.glass_world_steps(),
            lights: packed_lights,
            inv_view_proj: self.camera.relative_view_proj().inverse().to_cols_array_2d(),
            atmosphere: [
                self.overcast.clamp(0.0, 1.0),
                self.storm.clamp(0.0, 1.0),
                self.atmosphere_mood.clamp(0.0, 1.0),
                self.twilight.clamp(-1.0, 1.0),
            ],
            world_medium: [
                self.fog_density.max(0.0),
                world_medium.snow_fraction,
                world_medium.layer[0],
                world_medium.layer[1],
            ],
            // Both diffuse and GGX environment paths survive in every budget. Removing the
            // second lane blackens fully metallic Tiger armor while diffuse T-34s remain lit.
            transport_passes: self.transport_quality.surface_passes(),
            cabin_medium:self.cabin_medium_uniform,
            medium_transport:[if self.atmo_enabled {self.atmo_haze*0.010*5.2}else{0.0},self.weather_offset.x,self.weather_offset.y,self.weather_offset.z],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam));
        // The raymarch backend renders none of the raster-lighting passes these feed
        // (cabin cubemaps, the visibility atlas, raster optics, grain/spark parcels).
        let raster_passes = self.raymarch.is_none();
        // The raymarch backend has no optical relay of its own yet: its vision blocks and sights
        // show this backend's exterior raster captures (an interim, docs/RAYMARCHING_ENGINE.md),
        // which need the visibility atlas and lamp cubes on the frames a port is in view.
        let relay_optics = !raster_passes && self.raymarch.as_ref().is_some_and(|rm| rm.relays_optics());
        if relay_optics { self.prepare_raster_optics(queue, &cam); }
        let relayed = relay_optics && self.raster_optics_prepared();
        if (self.raster_lighting || self.menu_mode) && (raster_passes || relayed) {
            let near = self.near_shadow_uniform();
            let near_camera = CameraUniform { light_view_proj: near.view_proj, ..cam };
            queue.write_buffer(&self.near_shadow_camera_buf,0,bytemuck::bytes_of(&near_camera));
            queue.write_buffer(&self.near_shadow_buf,0,bytemuck::bytes_of(&near));
            if raster_passes { self.cabin_shadows.prepare(self,queue,&cam); }
            self.sky_visibility.prepare(self,queue);
        }
        // The probe also lights the forward menu/editor pipe on the deferred backend.
        self.environment_probe.prepare(queue, self.camera.pos, cam.world_medium,
            self.atmosphere_mood);
        if raster_passes { self.prepare_raster_optics(queue, &cam); }
        if let Some(probe) = &self.live_texel_probe {
            probe.prepare(queue, &cam, &self.camera, self.frame,
                self.live_probe_geometry_key(probe.hull()), self.live_texel_probe_active());
        }

        queue.write_buffer(
            &self.exp_clock_buf,
            0,
            bytemuck::bytes_of(&ExposureClockUniform {
                time_s: self.replay_exposure_time.unwrap_or(self.time.max(0.0)),
                fov_y: self.camera.fov_y,
                aspect: self.camera.aspect,
                // The gunner's TZF is one eye at a sight; every other view is both eyes.
                monocular: if (self.optic_mask - 1.0).abs() < 0.5 { 1.0 } else { 0.0 },
                transient_source: if self.observer_flash_source {1.0}else{0.0},
                _pad: [0.0;3],
            }),
        );

        let mut physical_environment = AtmosphereParameters::default();
        let campaign_environment = self.current_campaign_environment();
        physical_environment.sun_direction = glam::DVec3::new(
            self.sun_dir.x as f64,
            self.sun_dir.y as f64,
            self.sun_dir.z as f64,
        )
        .normalize_or_zero();
        physical_environment.mie_scale_height_m = campaign_environment.mie_scale_height_m;
        physical_environment.mie_scattering_m_inv = campaign_environment.mie_scattering_m_inv;
        physical_environment.mie_extinction_m_inv = campaign_environment.mie_extinction_m_inv;
        physical_environment.mie_asymmetry = campaign_environment.mie_asymmetry;
        physical_environment.cloud.bottom_altitude_m = campaign_environment.cloud_bottom_m;
        physical_environment.cloud.top_altitude_m = campaign_environment.cloud_top_m;
        physical_environment.cloud.liquid_water_content_kg_m3 =
            campaign_environment.cloud_lwc_kg_m3;
        physical_environment.cloud.droplet_effective_radius_m =
            campaign_environment.cloud_droplet_radius_m;
        // The shared environment solver measures the structured liquid-water column and applies a
        // delta-Eddington two-stream multiple-scattering flux closure at its physical lower
        // boundary. Coverage therefore drives actual cloud water/extinction here; it is not a sky
        // colour mix. Laptop changes only the numerical column quadrature below, never this scene
        // state or transport law.
        physical_environment.cloud.coverage = self.overcast.clamp(0.0, 1.0) as f64;
        physical_environment.cloud_offset_m = self.weather_offset.as_dvec3();
        physical_environment.cloud_evolution_s = self.time as f64;
        let latitude = self.sky_clock.site.latitude_deg.to_radians();
        let sidereal = (crate::ephemeris::apparent_sidereal_time_deg(self.sky_clock.now())
            + self.sky_clock.site.longitude_deg_east).to_radians();
        physical_environment.star_frame = [latitude.sin(),latitude.cos(),sidereal,1.0];
        // The ephemeris supplies both bodies: the key carries the direct beam along `sun_dir`
        // (diagnostic fixtures may stage that direction), the secondary lights only air and
        // cloud. Night is whatever these give: no floor, no fraction of the Sun.
        self.celestial.apply_to(&mut physical_environment);
        physical_environment.sun_direction = glam::DVec3::new(
            self.sun_dir.x as f64,
            self.sun_dir.y as f64,
            self.sun_dir.z as f64,
        )
        .normalize_or_zero();
        let integration_quality = self.transport_quality.environment(self.graphics_preset);
        let gpu_environment =
            GpuPhysicalEnvironment::from_parameters(&physical_environment, integration_quality);
        queue.write_buffer(
            &self.physical_environment_buf,
            0,
            bytemuck::bytes_of(&gpu_environment),
        );

        // Full keeps the existing high-quality CPU boundary cache on every raster preset.
        self.upload_pixel_particles(queue, &physical_environment, &packed_lights, world_medium,
            self.transport_quality.environment(crate::graphics::GraphicsPreset::High));

        // Volumetric smoke: its own inverse-view-proj (ray reconstruction + depth occlusion) and the
        // puff ring. Cheap to always upload; the pass itself is skipped when nothing is active.
        let smoke = SmokeUniform {
            inv_view_proj: self.camera.relative_view_proj().inverse().to_cols_array_2d(),
            cam_pos: self.camera.pos.to_array(),
            fog_density: self.fog_density.max(0.0),
            sun_dir: self.sun_dir.normalize_or_zero().to_array(),
            frame: (self.frame & 0xFFFF) as f32,
            puffs: self.smoke_puffs,
            breech_gas: self.breech_gas,
            n_lights,
            night: self.night.clamp(0.0, 1.0),
            overcast: self.overcast.clamp(0.0, 1.0),
            world_max_distance: self.camera.zfar.max(1.0),
            lights: packed_lights,
            interior: if self.atmo_enabled {
                self.atmo_haze
            } else {
                0.0
            },
            dust: if self.atmo_enabled {
                self.atmo_dust
            } else {
                0.0
            },
            world_snow_fraction: world_medium.snow_fraction,
            storm: self.storm.clamp(0.0, 1.0),
            atmo_origin: self.atmo_origin.to_array(),
            atmo_time: self.time,
            // The campaign's terrain remains inside this world-space medium. Blowing snow uses a
            // shallower physical layer than generic aerosol, preserving low horizontal visibility
            // while allowing real downwelling daylight to reach the battlefield.
            fog_layer: world_medium.layer,
            transport_steps: self.transport_quality.smoke_steps(),
            weather_motion:[self.weather_offset.x,self.weather_offset.y,self.weather_offset.z,self.time],
            cabin_medium:self.cabin_medium_uniform,
        };
        queue.write_buffer(&self.smoke_buf, 0, bytemuck::bytes_of(&smoke));
        if raster_passes || relayed {
            self.prepare_fine_vfx(device, queue);
            self.prepare_weapon_particles(device, queue);
        }
        if let Some(mut rm) = self.raymarch.take() {
            rm.prepare(self, device, queue);
            self.raymarch = Some(rm);
        }

        let m = self.master_retro.clamp(0.0, 1.0);
        // Metered frames receive the eye's mapping from the GPU (copied after this upload). A
        // frame that is never metered (the menu turntable) is shown by an eye adapted to the
        // former fixed calibration's mid-grey, 41 cd/m^2; a debug-gated fixed exposure has no rods.
        let unmetered = crate::observer::Eye::adapted(
            crate::observer::Meter::uniform(41.0, 41.0 * 2.35), crate::observer::Viewing::Binocular)
            .mapping(crate::observer::Viewing::Binocular);
        let crush = CrushUniform {
            levels: self.display_tuning.map(|p| p.palette_levels).unwrap_or(if self.shader_backend { 31.0 } else { 5.0 + (1.0 - m) * 27.0 }),
            // Full-step ordered coverage in a 5-bit display palette breaks false contours.
            // Crush attenuates palette treatment in the distance to preserve sight readability.
            dither: self.display_tuning.map(|p| p.ordered_dither).unwrap_or(if self.shader_backend { 1.0 } else { 0.15 + m * 0.45 }),
            // The TZF Strichbild scales in TRUE mils: the shader needs the live vertical FOV
            // (radians) to keep 1 Strich = 1/6400 circle at every magnification. 0 = off.
            reticle: if self.reticle { self.camera.fov_y } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            rod_exposure: if self.exposure_override.is_some() { 0.0 } else { unmetered.rod_exposure as f32 },
            rod_inverse_saturation: unmetered.rod_inverse_saturation as f32,
            ambient_white_balance: self.display_tuning
                .filter(|p| p.ambient_white_balance)
                .map_or(0.0, |p| p.ambient_white_balance_strength.clamp(0.0, 1.0)),
            menu_mode: if self.menu_mode { 1.0 } else { 0.0 },
            ldr_dither: self.display_tuning.map(|p| p.ldr_dither).unwrap_or(if self.shader_backend { 1.0 } else { 0.0 }),
            mask: self.optic_mask,
            _removed_proxy_rt_mix: 0.0,
            exposure: self.exposure_override.unwrap_or(unmetered.cone_exposure as f32),
            retro: self.display_tuning.map(|p| p.retro_strength).unwrap_or(if self.shader_backend { 1.0 } else { m }),
            concussion: self.concussion.clamp(0.0, 1.0),
            // The eye's draw-in of regions far from its adapted level (crush.wgsl `eye_range`)
            // applies where a dark cabin and bright ports share the field: a buttoned-up view that
            // is not the gun sight's or the binoculars' full-field optic picture.
            _grade_pad: [self.display_dither_pattern,
                if self.interior_light > 0.5 && (self.optic_mask - 1.0).abs() > 0.5 && (self.optic_mask - 3.0).abs() > 0.5 { 1.0 } else { 0.0 }],
            display_grade: boosted_display_grade(self.display_grade, self.vision_boost),
            dither_right: self.camera.right().extend((self.camera.fov_y * 0.5).tan() * self.camera.aspect).to_array(),
            dither_up: self.camera.right().cross(self.camera.forward()).extend((self.camera.fov_y * 0.5).tan()).to_array(),
            dither_forward: self.camera.forward().extend(0.0).to_array(),
        };
        queue.write_buffer(&self.crush_buf, 0, bytemuck::bytes_of(&crush));
        if let Some(surface) = &self.rt_surface_gpu {
            let triangles = self.meshes.iter().filter(|m| m.glass && m.visible)
                .flat_map(|m| {
                    m.rt_source.iter().flat_map(move |source| {
                        source.indices.chunks_exact(3).map(move |ids| {
                            [0, 1, 2].map(|i| {
                                m.model.transform_point3(glam::Vec3::from_array(source.vertices[ids[i] as usize].pos))
                            })
                        })
                    })
                });
            let proposals = daylight_sampling::Proposals::from_triangles(triangles);
            queue.write_buffer(&surface.daylight_proposals_buf, 0, bytemuck::bytes_of(&proposals));
            let instances = self.rt_instance_shading_records();
            assert_eq!(instances.len(), surface.instance_count);
            if !instances.is_empty() {
                queue.write_buffer(&surface.instances_buf, 0, bytemuck::cast_slice(&instances));
            }
        }
    }

    /// Eye and live probe submit the same source meshes, visibility, instance ranges and full IDs.
    /// Target attachments, camera and frustum differ; probes omit view-local raster props.
    /// Lighting is a separate shared combine.
    fn encode_primary_scene(
        &self, encoder: &mut wgpu::CommandEncoder, camera_view_proj: glam::Mat4,
        camera_bind: &wgpu::BindGroup, targets: [&wgpu::TextureView; 4],
        depth: &wgpu::TextureView, physical_probe: bool, label: &str, work: &mut RenderWorkStats,
    ) {
        let planes = frustum_planes(camera_view_proj);
        let visible_instanced = self.inst_spatial_index.query(camera_view_proj);
        let colors = targets.map(|view| {
            Some(attachment(view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)))
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label), color_attachments: &colors,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth, depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store,
                }), stencil_ops: None,
            }),
            timestamp_writes: self.gpu_profile.as_ref().and_then(|p|p.render(label)),
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, camera_bind, &[]);
        pass.set_bind_group(2, &self.shadow_bind, &[]); // layout slot kept; gbuf.wgsl ignores it
        pass.set_bind_group(3, &self.track_bind, &[]);
        pass.set_pipeline(&self.gbuf_inst_pipe);
        for &batch in &visible_instanced.batches {
            let b = &self.inst_batches[batch];
            if b.inst_count == 0 {
                continue;
            }
            let p = &self.inst_prototypes[b.prototype];
            let push = GbufPush {
                model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                params: [b.emissive, p.metallic_factor, p.roughness_factor, b.flags],
                // Same globally unique id range as TLAS custom_data. The vertex shader adds
                // instance_index to this page's deterministic base.
                // Bit 23 marks every moving pool (including exact RT infantry) so temporal
                // accumulation rejects stale history instead of leaving pose/motion trails.
                extra: [b.object_custom_data(0) as f32, 0.0, 0.0, 0.0],
            };
            pass.set_push_constants(
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                0,
                bytemuck::bytes_of(&push),
            );
            pass.set_bind_group(1, p.tex_bind.as_ref(), &[]);
            pass.set_vertex_buffer(0, p.vbuf.slice(..));
            pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
            pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            work.scene_instanced_draws += 1;
            pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
        }
        pass.set_pipeline(&self.gbuf_pipe);
        for (i, m) in self.meshes.iter().enumerate() {
            if !m.visible || (physical_probe && m.raster_only) {
                continue;
            }
            if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                continue;
            }
            let wmode = if m.periscope_kind != 0 {
                5.0 + m.periscope_kind as f32
            } else if m.glass {
                // High-only closed dielectric. gbuf.wgsl records the entry surface and
                // combine.wgsl traces the actual second interface and exterior segment.
                5.0
            } else if m.track_recv {
                3.0
            } else if m.emissive_own {
                2.0
            } else if m.emissive_win {
                1.0
            } else {
                0.0
            };
            let push = GbufPush {
                model: m.model.to_cols_array_2d(),
                params: [m.emissive, m.metallic_factor, m.roughness_factor, wmode],
                extra: [regular_object_custom_data(i, m.temporal_dynamic) as f32, 0.0, 0.0, 0.0],
            };
            pass.set_push_constants(
                wgpu::ShaderStages::VERTEX_FRAGMENT,
                0,
                bytemuck::bytes_of(&push),
            );
            pass.set_bind_group(1, m.tex_bind.as_ref(), &[]);
            pass.set_vertex_buffer(0, m.vbuf.slice(..));
            pass.set_index_buffer(m.rigid_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
            for range in m.primary_index_ranges(camera_view_proj) {
                work.scene_mesh_draws += 1;
                pass.draw_indexed(range, 0, 0..1);
            }
        }
    }

    fn render_internal(&self, encoder: &mut wgpu::CommandEncoder, finish_ldr: bool) {
        if let Some(rm) = &self.raymarch {
            rm.encode(self, encoder, finish_ldr);
            return;
        }
        let mut work = RenderWorkStats {
            scene_renders: 1,
            ..RenderWorkStats::default()
        };
        debug_assert!(
            !self.deferred
                || (self.rt_on && self.combine_pipe.is_some() && self.rt_as_bgl.is_some()),
            "deferred rendering cannot run without the ray-query pipeline"
        );
        // ---- sun shadow pass: depth of all opaque casters from the light's POV ----
        // Only legacy forward consumers read these maps. Deferred hardware queries and the
        // portable source-BVH path both resolve sun visibility from actual scene triangles.
        if !self.deferred && !self.raster_lighting {
            for near in [false, true] {
            if (near && !self.shader_backend) || (!near && !self.coop_shadow.should_render()) { continue; }
            work.shadow_passes += 1;
            let light_view_proj = if near { glam::Mat4::from_cols_array_2d(&self.near_shadow_uniform().view_proj) }
                else { self.light_view_proj() };
            let planes = frustum_planes(light_view_proj);
            let visible_instanced = self.inst_spatial_index.query(light_view_proj);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if near { "near-shadow" } else { "shadow" }),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if near { &self.near_shadow_view } else { &self.shadow_view },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self
                    .gpu_profile
                    .as_ref()
                    .and_then(|profile| {
                        if near { None } else { profile.render("shadow") }
                    }),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow_pipe);
            pass.set_bind_group(0, if near { &self.near_shadow_camera_bind } else { &self.camera_bind }, &[]);
            // Cull against the sun's ortho cascade: geometry outside the shadow footprint can't
            // write useful depth, so skip its draw entirely.
            for m in &self.meshes {
                // Every visible opaque surface casts, including emissive housings.
                if !m.visible || (self.shader_backend && m.glass) {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                work.shadow_mesh_draws += 1;
                pass.draw_indexed(0..m.index_count, 0, 0..1);
            }
            // Instanced static batches: one depth draw per unique module covers every placement.
            pass.set_pipeline(&self.shadow_inst_pipe);
            for &batch in &visible_instanced.batches {
                let b = &self.inst_batches[batch];
                if b.inst_count == 0 {
                    continue;
                }
                if b.flags > 3.5 || b.raster_only_dynamic {
                    continue;
                } // grass / tiny transient FX cast no shadow (acne + cost, zero read)
                let p = &self.inst_prototypes[b.prototype];
                let push = PushData {
                    model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    params: [0.0, p.metallic_factor, p.roughness_factor, 0.0],
                };
                pass.set_push_constants(wgpu::ShaderStages::VERTEX, 0, bytemuck::bytes_of(&push));
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                work.shadow_instanced_draws += 1;
                pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
            }
            }
        }
        if self.raster_lighting {
            self.cabin_shadows.encode(self,encoder,&mut work);
            self.sky_visibility.encode(self,encoder,&mut work);
        }
        // Every frame: raster surfaces, raster media and particles read the physical sky from it.
        self.environment_probe.encode(encoder, &mut work);
        if self.shader_backend {
            // The raster flakes' light: mean sky radiance, mean ground radiance and the key beam.
            let probe = &self.environment_probe.output;
            use environment_probe::{PROBE_BEAM, PROBE_MEAN_GROUND, PROBE_MEAN_SKY};
            encoder.copy_buffer_to_buffer(probe, (PROBE_BEAM * 16) as u64,
                &self.pixel_particle_uniform, 128, 16);
            encoder.copy_buffer_to_buffer(probe, (PROBE_MEAN_SKY * 16) as u64,
                &self.pixel_particle_uniform, 240, 16);
            encoder.copy_buffer_to_buffer(probe, (PROBE_MEAN_GROUND * 16) as u64,
                &self.pixel_particle_uniform, 256, 16);
        }
        self.encode_raster_optics(encoder, &mut work);
        // Diagnostic baseline computes sky before opaque geometry. Normal forward and deferred
        // paths both evaluate the identical atmosphere only at primary misses after depth exists.
        if !self.depth_gate_sky {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: self
                    .gpu_profile
                    .as_ref()
                    .and_then(|profile| profile.render("sky")),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.sky_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(1, &self.physical_environment_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        let camera_view_proj = self.camera.view_proj();
        let planes = frustum_planes(camera_view_proj);
        // One page list serves either opaque path. Splitscreen calls `render_internal` once per
        // camera, so each seat gets its own list without mutating global page visibility.
        let visible_instanced = self.inst_spatial_index.query(camera_view_proj);
        if self.deferred {
            if self.live_texel_probe_active() {
                if let Some(capture) = self.live_texel_probe.as_ref().unwrap().take_capture() {
                    self.encode_primary_scene(encoder, capture.view_proj, capture.camera,
                        capture.targets, capture.depth, true, &format!("live-probe-gbuffer-{}", capture.face), &mut work);
                    let colors = capture.transport.map(|view| {
                        Some(attachment(view,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)))
                    });
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("live-probe-transport"), color_attachments: &colors,
                        depth_stencil_attachment: None,
                        timestamp_writes: self.gpu_profile.as_ref().and_then(|p|p.render("live-probe-transport")),
                        occlusion_query_set: None,
                    });
                    // EXACT existing integrator, TLAS/material atlas, source lights and environment.
                    // No duplicate transport shader and no zero-order ambient or quantized source.
                    pass.set_pipeline(self.combine_pipe.as_ref().unwrap());
                    pass.set_bind_group(0, capture.camera, &[]);
                    pass.set_bind_group(1, capture.gbuffer, &[]);
                    pass.set_bind_group(2, self.combine_rt_bind.as_ref().expect("live probe requires current TLAS"), &[]);
                    pass.set_bind_group(3, &self.physical_environment_bind, &[]);
                    pass.draw(0..3, 0..1);
                    work.fullscreen_raster_passes += 1;
                    drop(pass);
                    work.compute_passes += u64::from(self.live_texel_probe.as_ref().unwrap()
                        .encode_captured_reconstruction(encoder, capture.face, self.gpu_profile.as_ref()));
                }
            }
            self.encode_primary_scene(encoder, camera_view_proj, &self.camera_bind,
                [&self.g0_view, &self.g1_view, &self.g2_view, &self.g3_view],
                &self.depth_view, false, "gbuf", &mut work);
            if self.depth_gate_sky {
                // G-buffer depth already identifies every primary miss. Clear HDR, then shade
                // only those misses; resolve later overwrites every opaque pixel. Transport,
                // resolution and sample sequence remain identical to the ungated sky baseline.
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sky-depth-gated"),
                    color_attachments: &[Some(attachment(
                        &self.hdr_view,
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    ))],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.render("sky-depth-gated")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.sky_depth_pipe);
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(1, &self.physical_environment_bind, &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
            // ── COMBINE: fullscreen shade of the G-buffer into diffuse, specular and immediate
            // emission targets. S4 swaps the sun/ambient terms for the ray
            // integrator inside combine.wgsl; this pass structure is final. ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("combine"),
                    color_attachments: &[
                        Some(attachment(
                            &self.lit_diff_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.lit_other_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.lit_emission_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.render("combine")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(
                    self.combine_pipe
                        .as_ref()
                        .expect("deferred rendering requires the ray-query combine pipeline"),
                );
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(1, &self.gbuf_bind, &[]);
                pass.set_bind_group(
                    2,
                    self.combine_rt_bind
                        .as_ref()
                        .expect("TLAS bind (rt_update ran)"),
                    &[],
                );
                pass.set_bind_group(3, &self.physical_environment_bind, &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
            // ── S3 ACCUMULATE: diffuse and specular own independent radiance histories. The
            // shared depth/id/camera rejection keeps both attached to the same physical surface. ──
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("diffuse-accum"),
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.compute("diffuse-accum")),
                });
                pass.set_pipeline(&self.accum_pipe);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(0, &vc.accum_binds[vc.accum_flip], &[]);
                pass.set_bind_group(1, &self.accum_delta_bind, &[]);
                work.compute_passes += 1;
                pass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
            }
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("specular-accum"),
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.compute("specular-accum")),
                });
                pass.set_pipeline(&self.accum_pipe);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(0, &vc.spec_accum_binds[vc.accum_flip], &[]);
                pass.set_bind_group(1, &self.accum_delta_bind, &[]);
                work.compute_passes += 1;
                pass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
            }
            // Optional live reconstruction reads completed history, writes view-transient targets,
            // and is consumed only by resolve. It cannot recursively blur temporal history.
            if self.surface_reconstruction_active() {
                let vc = &self.views[self.active_view];
                work.compute_passes += self.surface_reconstruction.as_ref().unwrap().encode(
                    encoder, vc.reconstruction_binds.as_ref().unwrap(), vc.accum_flip,
                    self.gpu_profile.as_ref(),
                );
            }
            if self.live_texel_probe_active() {
                let vc = &self.views[self.active_view];
                let rasterized_texels = self.live_texel_probe.as_ref().unwrap().encode_reuse(encoder,
                    vc.probe_binds.as_ref().unwrap(), vc.accum_flip,
                    self.surface_reconstruction_active(), self.gpu_profile.as_ref());
                work.compute_passes += 1;
                work.scene_instanced_draws += u64::from(rasterized_texels);
            }
            // ── RESOLVE: albedo × accumulated diffuse + accumulated specular + immediate
            // authored emission → HDR (over the sky). ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("resolve"),
                    color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                    depth_stencil_attachment: None,
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.render("resolve")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.resolve_pipe);
                pass.set_bind_group(0, &self.camera_bind, &[]);
                let vc = &self.views[self.active_view];
                let resolve_binds = if self.live_texel_probe_active() {
                    vc.probe_resolve_binds.as_ref().unwrap()
                } else if self.surface_reconstruction_active() {
                    vc.reconstruction_resolve_binds.as_ref().unwrap()
                } else { &vc.resolve_binds };
                pass.set_bind_group(1, &resolve_binds[vc.accum_flip], &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
        } else if self.shader_backend {
            self.encode_raster_scene(encoder, &self.camera_bind, camera_view_proj,
                &self.hdr_view, &self.depth_view, false, None, &mut work);
            self.encode_raster_optic_panels(encoder, &mut work);
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self
                    .gpu_profile
                    .as_ref()
                    .and_then(|profile| profile.render("mesh")),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.mesh_pipe);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.shadow_bind, &[]); // sun shadow map (shared across draws)
            pass.set_bind_group(3, &self.track_bind, &[]); // world tank-track rut map
                                                           // Instanced static batches FIRST (opaque, depth-tested — order vs the loose meshes is
                                                           // irrelevant): one lit draw per unique module covers every placement, whole-batch culled
                                                           // (batches arrive spatially bucketed from the worldgen).
            pass.set_pipeline(&self.mesh_inst_pipe);
            for &batch in &visible_instanced.batches {
                let b = &self.inst_batches[batch];
                if b.inst_count == 0 {
                    continue;
                }
                let p = &self.inst_prototypes[b.prototype];
                let push = PushData {
                    model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    params: [b.emissive, p.metallic_factor, p.roughness_factor, b.flags],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, p.tex_bind.as_ref(), &[]);
                pass.set_vertex_buffer(0, p.vbuf.slice(..));
                pass.set_vertex_buffer(1, b.inst_buf.slice(b.active_instance_bytes()));
                pass.set_index_buffer(p.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                work.scene_instanced_draws += 1;
                pass.draw_indexed(0..p.index_count, 0, 0..b.inst_count);
            }
            pass.set_pipeline(&self.mesh_pipe);
            for m in &self.meshes {
                if !m.visible {
                    continue;
                }
                if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                    continue;
                }
                // params.w: 2 = own-colour emissive, ungated (interior bulb / snow puff); 1 = window/
                // lamp own-colour glow (night-gated); 0 = FX orange / lit PBR.
                let wmode = if m.track_recv {
                    3.0
                } else if m.emissive_own {
                    2.0
                } else if m.emissive_win {
                    1.0
                } else {
                    0.0
                };
                let push = PushData {
                    model: m.model.to_cols_array_2d(),
                    params: [m.emissive, m.metallic_factor, m.roughness_factor, wmode],
                };
                pass.set_push_constants(
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    0,
                    bytemuck::bytes_of(&push),
                );
                pass.set_bind_group(1, m.tex_bind.as_ref(), &[]);
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_index_buffer(m.rigid_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                for range in m.primary_index_ranges(camera_view_proj) {
                    work.scene_mesh_draws += 1;
                    pass.draw_indexed(range, 0, 0..1);
                }
            }
            if self.depth_gate_sky {
                // The fullscreen triangle sits exactly on the far plane and Equal-tests against
                // the 1.0 depth clear. Early depth therefore suppresses the expensive physical
                // atmosphere fragment on every opaque pixel without replacing, approximating or
                // resampling any visible sky transport. Keep it in this opaque render pass: on a
                // tile-based Metal GPU that avoids storing and immediately reloading HDR + depth.
                pass.set_pipeline(&self.sky_depth_pipe);
                pass.set_bind_group(0, &self.camera_bind, &[]);
                pass.set_bind_group(1, &self.physical_environment_bind, &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
        }

        // ---- physical participating media: estimate → converge → source-over composite ----
        // smoke.wgsl samples the real environment/emitter table and scene TLAS, but that unbiased
        // one-frame estimate is stochastic. It used to bypass S3 and go directly into HDR, making
        // retro=0 interiors look covered in salt-and-pepper. High now denoises only effective
        // source radiance in an independent per-view history; current Beer-Lambert opacity remains
        // untouched, preserving tiny world-space motes, plume silhouettes and transients.
        if self.smoke_active() {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("smoke-estimate"),
                    color_attachments: &[Some(attachment(
                        &self.volume_current_view,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    ))],
                    depth_stencil_attachment: None,
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.render("smoke-estimate")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.smoke_pipe);
                pass.set_bind_group(0, &self.smoke_bind, &[]);
                if self.raster_lighting && !self.rt_on {
                    pass.set_bind_group(1, &self.sky_visibility.sampling_bind, &[]);
                    pass.set_bind_group(3, &self.cabin_shadows.sampling_bind, &[]);
                } else if self.rt_on {
                    pass.set_bind_group(
                        1,
                        self.combine_rt_bind
                            .as_ref()
                            .expect("ray-query smoke requires the scene TLAS bind"),
                        &[],
                    );
                } else {
                    pass.set_bind_group(1, &self.smoke_empty_visibility_bind, &[]);
                }
                pass.set_bind_group(2, &self.physical_environment_bind, &[]);
                work.smoke_passes += 1;
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }

            let composite_bind = if self.deferred {
                let vc = &self.views[self.active_view];
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("volume-accum"),
                        timestamp_writes: self
                            .gpu_profile
                            .as_ref()
                            .and_then(|profile| profile.compute("volume-accum")),
                    });
                    pass.set_pipeline(&self.volume_accum_pipe);
                    pass.set_bind_group(0, &vc.volume_accum_binds[vc.accum_flip], &[]);
                    pass.set_bind_group(1, &self.accum_delta_bind, &[]);
                    work.compute_passes += 1;
                    pass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
                }
                &vc.volume_composite_binds[vc.accum_flip]
            } else {
                // Portable has no scene-visibility query and deliberately emits no stochastic
                // direct source radiance, so its deterministic extinction result needs no history.
                &self.volume_current_composite_bind
            };
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("volume-composite"),
                    color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                    depth_stencil_attachment: None,
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.render("volume-composite")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.volume_composite_pipe);
                pass.set_bind_group(0, composite_bind, &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
        }

        self.encode_fine_vfx(encoder, usize::MAX, &self.hdr_view, &mut work);
        self.encode_weapon_particles(encoder, usize::MAX, &self.hdr_view, false, false, &mut work);

        // Snapshot only after both surface and volume histories have consumed the previous frame's
        // reject buffers. The former placement before smoke made a volume reprojection pass read
        // the current depth under the name `prev_depth`.
        if self.deferred {
            let extent = wgpu::Extent3d {
                width: self.iw,
                height: self.ih,
                depth_or_array_layers: 1,
            };
            encoder.copy_texture_to_texture(
                self.depth_tex.as_image_copy(),
                self.views[self.active_view].prev_depth_tex.as_image_copy(),
                extent,
            );
            encoder.copy_texture_to_texture(
                self.g3_tex.as_image_copy(),
                self.views[self.active_view].prev_id_tex.as_image_copy(),
                extent,
            );
        }

        // ---- S4 auto-exposure: meter transported HDR before bloom, adapt, and patch the
        // result into the shared camera-response uniform before bloom and tonemapping. ----
        // This is the crewman's eye (observer.rs): it runs on every presented frame of both
        // backends and presets. It reads the HDR and writes only its own view state; the display
        // mapping lanes are then copied into the crush uniform (cone at byte 44, rods at 16).
        if finish_ldr && self.autoexp && self.exposure_override.is_none() {
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("exposure"),
                    timestamp_writes: self
                        .gpu_profile
                        .as_ref()
                        .and_then(|profile| profile.compute("exposure")),
                });
                cpass.set_pipeline(&self.exp_pipe);
                cpass.set_bind_group(0, &self.exp_binds[self.active_view], &[]);
                work.compute_passes += 1;
                cpass.dispatch_workgroups(1, 1, 1);
            }
            let eye = &self.exp_bufs[self.active_view];
            encoder.copy_buffer_to_buffer(eye, 0, &self.crush_buf, 44, 4);
            encoder.copy_buffer_to_buffer(eye, 8, &self.crush_buf, 16, 8);
            if let Some(optics) = &self.raster_optics {
                // Port images take the same eye (one frame later, as they render first).
                optics.copy_eye_mapping(encoder, eye);
            }
        }


        // ---- emissive bloom (half-res), BEFORE crush so the halo takes the PS1 treatment ----
        // Bloom only the visible frame. The former pasted-camera periscope atlas is removed.
        if finish_ldr {
            work.bloom_passes += 4;
            work.fullscreen_raster_passes += 4;
            let btexel = [1.0 / self.bw as f32, 1.0 / self.bh as f32];
            // 1) bright pass: HDR → bloom_a (extract energy above threshold).
            self.bloom_pass(
                encoder,
                "bloom-bright",
                &self.bloom_bright_pipe,
                &self.bloom_bind_hdr,
                &self.bloom_a_view,
                btexel,
                [0.0, 0.0],
                false,
            );
            // 2) horizontal blur: bloom_a → bloom_b.
            self.bloom_pass(
                encoder,
                "bloom-blur-h",
                &self.bloom_blur_pipe,
                &self.bloom_bind_a,
                &self.bloom_b_view,
                btexel,
                [1.0, 0.0],
                false,
            );
            // 3) vertical blur: bloom_b → bloom_a.
            self.bloom_pass(
                encoder,
                "bloom-blur-v",
                &self.bloom_blur_pipe,
                &self.bloom_bind_b,
                &self.bloom_a_view,
                btexel,
                [0.0, 1.0],
                false,
            );
            // 4) composite: bloom_a additively back into the full-res HDR.
            self.bloom_pass(
                encoder,
                "bloom-composite",
                &self.bloom_composite_pipe,
                &self.bloom_bind_a,
                &self.hdr_view,
                btexel,
                [0.0, 0.0],
                true,
            );
        }


        // Debug-only heat study: refraction outside temporal histories, never a light source.
        if let Some(heat) = &self.heat_gpu {
            if heat.encode(&self.instance_queue, encoder, &self.hdr_tex, &self.hdr_view,
                &self.heat_sources, self.heat_sources.time, self.camera.pos, self.camera.relative_view_proj()) {
                work.fullscreen_raster_passes += 1;
            }
        }
        // A transient optical pressure cap never feeds surface/volume histories or bloom.
        // Its source visibility comes from the same live TLAS; it only refracts existing HDR.
        if self.pressure_gpu.encode(&self.instance_queue, encoder, &self.hdr_tex,
            &self.hdr_view, self.combine_rt_bind.as_ref(), &self.pressure_fronts,
            self.time, self.active_view, self.camera.pos, self.camera.relative_view_proj()) {
            work.fullscreen_raster_passes += 1;
        }

        // Actual bullet strips are added after bloom and optical distortion, so neither can
        // widen a one-native-pixel trail. They still use HDR exposure and shared attenuation.
        if self.shader_backend {
            self.encode_weapon_particles(encoder, usize::MAX, &self.hdr_view, true, false, &mut work);
            self.encode_raster_optic_tracers(encoder, &mut work);
        }

        if finish_ldr {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
                color_attachments: &[Some(attachment(
                    &self.ldr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: self
                    .gpu_profile
                    .as_ref()
                    .and_then(|profile| profile.render("crush")),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.crush_pipe);
            pass.set_bind_group(0, &self.crush_bind, &[]);
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
        }
        // Native-pixel interaction UI remains outside exposure, bloom and scene dithering.
        if finish_ldr && self.shader_backend { self.encode_hover_outline(encoder, &mut work); }
        if finish_ldr && self.pixel_particle_count.get() > 0 {
            if let Some(pipeline) = &self.pixel_particle_pipe {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("one-pixel-world-particles"),
                    color_attachments: &[Some(attachment(&self.ldr_view,wgpu::LoadOp::Load))],
                    depth_stencil_attachment: None,
                    timestamp_writes: self.gpu_profile.as_ref().and_then(|p|p.render("one-pixel-world-particles")),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0,&self.pixel_particle_bind,&[]);
                if self.shader_backend {
                    pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);
                    pass.set_bind_group(2,&self.cabin_shadows.sampling_bind,&[]);
                }else{pass.set_bind_group(1,self.combine_rt_bind.as_ref().unwrap_or(&self.smoke_empty_visibility_bind),&[]);}
                pass.set_vertex_buffer(0,self.pixel_particle_vertices.slice(..));
                pass.draw(0..6,0..self.pixel_particle_count.get());
                work.snow_passes += 1;
            }
        }
        let mut total = self.render_work.get();
        total.accumulate(work);
        self.render_work.set(total);
    }

    /// One fullscreen bloom pass. `additive` loads (blends onto) the target instead of clearing —
    /// used only by the composite back into HDR.
    #[allow(clippy::too_many_arguments)]
    fn bloom_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        pipe: &wgpu::RenderPipeline,
        src_bind: &wgpu::BindGroup,
        target: &wgpu::TextureView,
        texel: [f32; 2],
        dir: [f32; 2],
        additive: bool,
    ) {
        let load = if additive {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(attachment(target, load))],
            depth_stencil_attachment: None,
            timestamp_writes: self
                .gpu_profile
                .as_ref()
                .and_then(|profile| profile.render(label)),
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, src_bind, &[]);
        let push = BloomPush { texel, dir };
        pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, bytemuck::bytes_of(&push));
        pass.draw(0..3, 0..1);
    }

    fn blit(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        self.blit_rect(device, encoder, target, format, None);
    }

    /// Full-frame crop for every non-splitscreen blit — a coop view render may have left a
    /// sub-crop in the uniform, and the menu/capture paths must never inherit it.
    fn reset_blit_crop(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.blit_ubuf,
            0,
            bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]),
        );
    }

    /// The upscale blit, optionally into a sub-viewport with a source crop (splitscreen). The
    /// crop uniform must already hold the right window — `render_to_surface_view` writes it.
    fn blit_rect(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        rect: Option<&BlitRect>,
    ) {
        if !self.blit_pipes.contains_key(&format) {
            let pipe = fullscreen_pipe(device, &self.blit_shader, &self.blit_layout, format);
            self.blit_pipes.insert(format, pipe);
        }
        let pipe = &self.blit_pipes[&format];
        let clear = rect.map_or(true, |r| r.clear);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit"),
            color_attachments: &[Some(attachment(
                target,
                if clear {
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                } else {
                    wgpu::LoadOp::Load
                },
            ))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if let Some(r) = rect {
            let [x, y, w, h] = r.dst_px;
            if w >= 1.0 && h >= 1.0 {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                // The fullscreen triangle overshoots NDC [-1,1]; the viewport transform alone
                // does not discard those fragments — scissor pins the paint to the view's cell.
                pass.set_scissor_rect(x as u32, y as u32, w as u32, h as u32);
            }
        }
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, &self.blit_bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn rt_instance_shading_records(&self) -> Vec<crate::rt_surface::GpuInstanceMaterialState> {
        use crate::rt_surface::{GpuInstanceMaterialState, InstanceEmissionMode, SurfaceId};

        let mut records = Vec::new();
        for (slot, mesh) in self.meshes.iter().enumerate() {
            if mesh.blas.is_none() {
                continue;
            }
            let surface = self.rt_scene_surface_ids[slot];
            let mode = if mesh.emissive <= 0.0 {
                InstanceEmissionMode::Disabled
            } else if mesh.emissive_win {
                InstanceEmissionMode::BaseColorNightGated
            } else if mesh.emissive_own {
                InstanceEmissionMode::BaseColorUngated
            } else {
                InstanceEmissionMode::RasterFxOrange
            };
            let mut record = if mode == InstanceEmissionMode::Disabled {
                GpuInstanceMaterialState::disabled(regular_object_custom_data(slot, mesh.temporal_dynamic), SurfaceId(surface))
            } else {
                let tint = if mode == InstanceEmissionMode::RasterFxOrange {
                    [1.0, 0.24, 0.03]
                } else {
                    [1.0; 3]
                };
                GpuInstanceMaterialState::emissive(
                    regular_object_custom_data(slot, mesh.temporal_dynamic),
                    SurfaceId(surface),
                    mode,
                    mesh.emissive.max(0.0),
                    tint,
                )
            };
            // Bit 0 is the exact closed-dielectric tag consumed by combine.wgsl. It is frame-
            // mutable alongside emission, so toggling set_glass never requires repacking source
            // triangles or changing the custom-data/surface lookup ABI.
            record.ids[3] = if mesh.glass {
                crate::rt_surface::INSTANCE_FLAG_CLOSED_DIELECTRIC
            } else {
                0
            };
            if mesh.track_recv {
                record.ids[3] |= crate::rt_surface::INSTANCE_FLAG_TERRAIN;
            }
            record.ids[3] |= match mesh.periscope_kind {
                1 => crate::rt_surface::INSTANCE_FLAG_PERISCOPE_WINDOW,
                2 => crate::rt_surface::INSTANCE_FLAG_PERISCOPE_MIRROR,
                3 => crate::rt_surface::INSTANCE_FLAG_PERISCOPE_LENS,
                _ => 0,
            };
            if mesh.periscope_kind == 3 {
                // Lens power is metadata, never emitted light. The source assemblies are rigid;
                // preserve metres under their ordinary hull/turret transforms.
                record.ids[2] = InstanceEmissionMode::Disabled as u32;
                let center = mesh.model.transform_point3((mesh.local_min + mesh.local_max) * 0.5);
                record.emission = [center.x, center.y, center.z, mesh.periscope_focal_m];
            }
            records.push(record);
        }
        for batch in &self.inst_batches {
            let prototype = &self.inst_prototypes[batch.prototype];
            if prototype.blas.is_none() || batch.flags > 3.5 || batch.raster_only_dynamic {
                continue;
            }
            let surface = self.rt_prototype_surface_ids[batch.prototype];
            // params.w = 2 (`set_batch_own_emissive`): the authored colour, ungated, exactly as a
            // loose mesh marked own-colour emissive.
            let own_colour = (batch.flags - 2.0).abs() < 0.5;
            for instance in 0..batch.object_id_capacity {
                let custom_data = batch.object_custom_data(instance);
                records.push(if batch.emissive > 0.0 {
                    let (mode, tint) = if own_colour {
                        (InstanceEmissionMode::BaseColorUngated, [1.0; 3])
                    } else {
                        (InstanceEmissionMode::RasterFxOrange, [1.0, 0.24, 0.03])
                    };
                    GpuInstanceMaterialState::emissive(
                        custom_data,
                        SurfaceId(surface),
                        mode,
                        batch.emissive,
                        tint,
                    )
                } else {
                    GpuInstanceMaterialState::disabled(custom_data, SurfaceId(surface))
                });
            }
        }
        records
    }

    /// A cloned actor changes identities, not triangles or materials. Keep the static buffers and
    /// bindings; rewrite the sorted sparse lookup so existing instanced props still address their
    /// correct material-state row after the new loose meshes are inserted before them.
    fn rt_refresh_actor_lookup(&mut self, queue: &wgpu::Queue) {
        if !self.rt_instance_topology_dirty || self.rt_surface_dirty { return; }
        let started = std::time::Instant::now();
        let instances = self.rt_instance_shading_records();
        let surface = self.rt_surface_gpu.as_mut().expect("cloned actor has a surface pack");
        if instances.len() > surface.instance_capacity {
            // Rare capacity growth uses the full, source-exact pack path; never omit actors.
            self.rt_surface_dirty = true;
            return;
        }
        let mut lookup: Vec<_> = instances.iter().enumerate().map(|(i, record)|
            crate::rt_surface::GpuInstanceLookup { key_value: [record.ids[0], i as u32, 0, 0] }).collect();
        lookup.sort_unstable_by_key(|record| record.key_value[0]);
        assert!(lookup.windows(2).all(|p| p[0].key_value[0] < p[1].key_value[0]),
            "cloned actor duplicated a ray-scene identity");
        queue.write_buffer(&surface.records_buf, u64::from(surface.lookup_offset_words) * 4,
            bytemuck::cast_slice(&lookup));
        queue.write_buffer(&surface.records_buf, 17 * 4,
            bytemuck::cast_slice(&[lookup.len() as u32, instances.len() as u32]));
        surface.instance_count = instances.len();
        self.rt_instance_topology_dirty = false;
        println!("[rt] actor lookup: {} instances; unchanged geometry/atlas ({:.2} ms)",
            instances.len(), started.elapsed().as_secs_f64() * 1000.0);
    }

    /// Repack exact source triangles/material maps only when topology or the sparse custom-data
    /// allocation changes. BLAS primitive ordering and SurfacePack triangle ordering are identical.
    fn rt_rebuild_surface_pack(&mut self, device: &wgpu::Device) {
        if !self.rt_on || !self.rt_surface_dirty {
            return;
        }
        let started = std::time::Instant::now();
        let mut builder = crate::rt_surface::SurfacePackBuilder::default();
        // Preserve scene-slot indexing for TLAS custom data while leaving an explicit sentinel for
        // upfront raster-only meshes. Those meshes own neither a BLAS nor an exact CPU source.
        let mut scene_surfaces = vec![u32::MAX; self.meshes.len()];
        for (slot, mesh) in self.meshes.iter().enumerate() {
            let Some(source) = mesh.rt_source.as_ref() else {
                assert!(
                    mesh.blas.is_none(),
                    "RT scene mesh {slot} has a BLAS without an exact CpuMesh source"
                );
                continue;
            };
            let surface = builder
                .add_mesh(source)
                .unwrap_or_else(|error| panic!("RT scene surface {slot} pack failed: {error}"));
            scene_surfaces[slot] = surface.0;
        }
        let mut prototype_surfaces = Vec::with_capacity(self.inst_prototypes.len());
        for (prototype, mesh) in self.inst_prototypes.iter().enumerate() {
            let source = mesh.rt_source.as_ref().unwrap_or_else(|| {
                panic!("RT instanced prototype {prototype} lost its exact CpuMesh source")
            });
            let surface = builder.add_mesh(source).unwrap_or_else(|error| {
                panic!("RT instanced surface {prototype} pack failed: {error}")
            });
            prototype_surfaces.push(surface.0);
        }

        for (slot, mesh) in self.meshes.iter().enumerate() {
            if mesh.blas.is_none() {
                continue;
            }
            builder
                .add_instance(
                    regular_object_custom_data(slot, mesh.temporal_dynamic),
                    crate::rt_surface::SurfaceId(scene_surfaces[slot]),
                    mesh.model,
                )
                .unwrap_or_else(|error| panic!("RT scene instance {slot} pack failed: {error}"));
        }
        for (batch_index, batch) in self.inst_batches.iter().enumerate() {
            let prototype = &self.inst_prototypes[batch.prototype];
            if prototype.blas.is_none() || batch.flags > 3.5 || batch.raster_only_dynamic {
                continue;
            }
            let surface = crate::rt_surface::SurfaceId(prototype_surfaces[batch.prototype]);
            for instance in 0..batch.object_id_capacity {
                let stored = batch
                    .cpu_mats
                    .get(instance as usize)
                    .copied()
                    .unwrap_or(glam::Mat4::IDENTITY);
                // A degenerate instance (zero scale, a collapsed axis, a non-finite entry) has no
                // volume to trace; it enters the exact pack as a point-sized stand-in so the
                // custom-data range stays dense, and is reported once per batch so the writer
                // can be fixed (the raster path draws it as nothing either way).
                let (model, degenerate) = rt_safe_instance_transform(stored);
                if degenerate && !self.rt_degenerate_reported.contains(&batch_index) {
                    self.rt_degenerate_reported.insert(batch_index);
                    eprintln!("[rt] batch {batch_index} instance {instance} (flags {:.1}, {} instances, prototype {}) has a degenerate transform {:?}; traced as a point",
                        batch.flags, batch.inst_count, batch.prototype, stored.to_cols_array_2d());
                }
                let custom = batch.object_custom_data(instance);
                builder
                    .add_instance(custom, surface, model)
                    .unwrap_or_else(|error| {
                        panic!("RT batch {batch_index} instance {instance} pack failed: {error}")
                    });
            }
        }
        let pack = builder
            .finish()
            .unwrap_or_else(|error| panic!("RT exact surface atlas pack failed: {error}"));
        self.rt_scene_surface_ids = scene_surfaces;
        self.rt_prototype_surface_ids = prototype_surfaces;
        let instances = self.rt_instance_shading_records();
        assert_eq!(instances.len(), pack.instances().len());
        pack.validate_instance_material_states(&instances)
            .unwrap_or_else(|error| panic!("RT dynamic material-state validation failed: {error}"));
        let mut upload = pack_rt_surface_static(&pack);
        let mut instance_upload = if instances.is_empty() {
            vec![crate::rt_surface::GpuInstanceMaterialState::default()]
        } else {
            instances
        };
        // Reserve only compact identity/state records. Geometry and texture payloads are exact,
        // unchanged source data; no actor or physical geometry is pre-spawned by this capacity.
        let instance_capacity = instance_upload.len().next_power_of_two();
        let lookup_offset_words = upload.records[16];
        upload.records.resize(lookup_offset_words as usize + instance_capacity * 4, 0);
        instance_upload.resize(instance_capacity, crate::rt_surface::GpuInstanceMaterialState::default());
        let limits = device.limits();
        let binding_limit =
            u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
        let binding_sizes = [
            (
                "RT exact vertices A",
                upload.vertices_a.len() * std::mem::size_of::<u32>(),
            ),
            (
                "RT exact vertices B",
                upload.vertices_b.len() * std::mem::size_of::<u32>(),
            ),
            (
                "RT exact semantic records",
                upload.records.len() * std::mem::size_of::<u32>(),
            ),
            (
                "RT exact atlas",
                upload.atlas.len() * std::mem::size_of::<u32>(),
            ),
            (
                "RT exact instance state",
                instance_upload.len()
                    * std::mem::size_of::<crate::rt_surface::GpuInstanceMaterialState>(),
            ),
        ];
        validate_rt_storage_binding_sizes(binding_limit, &binding_sizes)
            .unwrap_or_else(|error| panic!("RT exact surface upload rejected: {error}"));
        let vertices_a_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-surface-vertices-a"),
            contents: bytemuck::cast_slice(&upload.vertices_a),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let vertices_b_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-surface-vertices-b"),
            contents: bytemuck::cast_slice(&upload.vertices_b),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let records_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-surface-records"),
            contents: bytemuck::cast_slice(&upload.records),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let atlas_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-surface-atlas"),
            contents: bytemuck::cast_slice(&upload.atlas),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let instances_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rt-surface-instances"),
            contents: bytemuck::cast_slice(&instance_upload),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let daylight_proposals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("daylight-geometry-proposals"),
            size: std::mem::size_of::<daylight_sampling::Proposals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cost = pack.memory_cost();
        println!(
            "[rt] exact hit surfaces: {} vertices, {} triangles, {} materials, {} instances, {:.1} MiB payload; largest binding {:.1} MiB / {:.1} MiB ({:.0} ms)",
            pack.vertices().len(),
            pack.triangles().len(),
            pack.materials().len(),
            pack.instances().len(),
            cost.total_gpu_payload_bytes as f64 / (1024.0 * 1024.0),
            binding_sizes.iter().map(|(_, bytes)| *bytes).max().unwrap_or(0) as f64
                / (1024.0 * 1024.0),
            binding_limit as f64 / (1024.0 * 1024.0),
            started.elapsed().as_secs_f64() * 1000.0,
        );
        self.rt_surface_gpu = Some(RtSurfaceGpu {
            vertices_a_buf,
            vertices_b_buf,
            records_buf,
            atlas_buf,
            instances_buf,
            instance_count: pack.instances().len(),
            instance_capacity,
            lookup_offset_words,
            daylight_proposals_buf,
        });
        self.rt_surface_dirty = false;
        self.rt_instance_topology_dirty = false;
        self.combine_rt_bind = None;
    }

    /// Diagnostic-only numerical witness for a live daylight connection. Call after scene posing;
    /// this refreshes the actual TLAS and uniform state but does not draw or change lighting.
    pub fn probe_daylight_connection(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        origin: glam::Vec3,
        direction: glam::Vec3,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(self.rt_on, "daylight probe requires High ray queries");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("daylight-connection-probe"),
        });
        self.rt_update(device, &mut encoder);
        self.upload_uniforms(device, queue);
        let stages = wgpu::ShaderStages::COMPUTE;
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("daylight-probe-scene"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: stages,
                    ty: wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                    count: None,
                },
                storage_entry(1, stages, true),
                storage_entry(2, stages, true),
                storage_entry(3, stages, true),
                storage_entry(5, stages, true),
            ],
        });
        let surfaces = self.rt_surface_gpu.as_ref().unwrap();
        let scene_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("daylight-probe-scene"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.rt_tlas.as_ref().unwrap().as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: surfaces.vertices_a_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: surfaces.vertices_b_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: surfaces.records_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: surfaces.instances_buf.as_entire_binding(),
                },
            ],
        });
        let ray = [
            origin.extend(0.0).to_array(),
            direction.normalize().extend(0.0).to_array(),
        ];
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("daylight-probe-ray"),
            contents: bytemuck::cast_slice(&ray),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        const BYTES: u64 = 13 * 16;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("daylight-probe-output"),
            size: BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("daylight-probe-ray-output"),
            entries: &[uniform_entry(0, stages), storage_entry(1, stages, false)],
        });
        let output_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("daylight-probe-ray-output"),
            layout: &output_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let environment_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("daylight-probe-environment"),
                entries: &[uniform_entry(0, stages)],
            });
        let environment_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("daylight-probe-environment"),
            layout: &environment_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.physical_environment_buf.as_entire_binding(),
            }],
        });
        let source = physical_atmosphere_transport_shader_source(include_str!(
            "../shaders/protective_glass_probe.wgsl"
        ));
        let module = shader(device, "daylight-probe", &source);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("daylight-probe"),
            bind_group_layouts: &[&scene_layout, &output_layout, &environment_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("daylight-probe"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs"),
            compilation_options: Default::default(),
            cache: None,
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("daylight-probe"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &scene_bind, &[]);
            pass.set_bind_group(1, &output_bind, &[]);
            pass.set_bind_group(2, &environment_bind, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("daylight-probe-readback"),
            size: BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, BYTES);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = slice.get_mapped_range();
        let rows: Vec<[f32; 4]> = bytemuck::cast_slice::<u8, [f32; 4]>(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        let mesh_info = |row: usize| {
            let id = rows[row][1] as usize;
            self.meshes.get(id).map(|m| {
                serde_json::json!({
                    "slot": id, "glass": m.glass, "visible": m.visible,
                    "triangles": m.index_count / 3, "model": m.model.to_cols_array(),
                })
            })
        };
        Ok(serde_json::json!({
            "origin": origin.to_array(), "direction": direction.to_array(),
            "throughput_glass_path": rows[0], "exit_origin_air_before": rows[1],
            "first_raw_kind_id_distance_flags": rows[2], "first_position_kind": rows[3],
            "first_normal_cosine": rows[4], "exit_raw_kind_id_distance_flags": rows[5],
            "exit_position_kind": rows[6], "exit_normal_parallelism": rows[7],
            "blocker_raw_kind_id_distance_flags": rows[8], "exit_direction_error": rows[9],
            "solar_transmittance": rows[10], "received_solar_irradiance": rows[11],
            "top_solar_irradiance": rows[12], "first_mesh": mesh_info(2), "blocker_mesh": mesh_info(8),
        }))
    }

    /// The compact TLAS layout of the current scene: mesh slots, then each ray-visible page's
    /// visible instances.
    fn rt_slot_plan(&self) -> RtInstancedSlotPlan {
        let live_counts: Vec<Option<u32>> = self
            .inst_batches
            .iter()
            .map(|batch| {
                self.batch_in_ray_scene(batch)
                    .then_some(batch.rt_live_count())
            })
            .collect();
        plan_rt_instanced_slots(self.meshes.len() as u32, &live_counts)
            .expect("RT instanced slot count overflow")
    }

    /// M-D per-frame TLAS refresh: write every included mesh/instance transform, encode the
    /// full rebuild first in the frame encoder (wgpu 27 has no refit — rebuild IS the mode).
    /// Emissive geometry remains geometry: it participates in visibility and can return radiance
    /// when a secondary ray commits to it. Only explicit view-local/transparent paths stay out.
    fn rt_update(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if !self.rt_scene {
            return;
        }
        if self.rt_on {
            self.rt_rebuild_surface_pack(device);
        }
        // Splitscreen views 2..N re-render the SAME sim frame: the TLAS built for view 1 is
        // exactly current — skip the rebuild (but never before the first build exists).
        if self.rt_skip && self.rt_tlas.is_some() && self.combine_rt_bind.is_some() {
            return;
        }
        // A topology or live-count change computes a dense hardware layout. Reserved custom-data
        // identities remain unchanged; empty pools must not consume the adapter's TLAS capacity.
        let mut layout_plan = if self.rt_batches_dirty || self.rt_tlas.is_none() {
            Some(self.rt_slot_plan())
        } else {
            None
        };
        if let Some(plan) = &layout_plan {
            // Every re-layout re-measures the demand. An overflow is reported (again whenever
            // its size changes) and counted per frame below: never a silent drop.
            let needed = plan.end;
            match tlas_overflow_message(needed, TLAS_MAX_INSTANCES) {
                Some(message) => {
                    if self.rt_overflow_reported != Some(needed) {
                        eprintln!("[rt] ERROR: {message}");
                        self.rt_overflow_reported = Some(needed);
                    }
                    self.rt_overflow_now = needed - TLAS_MAX_INSTANCES;
                }
                None => {
                    if self.rt_overflow_reported.take().is_some() {
                        println!("[rt] TLAS demand back within capacity: {needed} of {TLAS_MAX_INSTANCES}");
                    }
                    self.rt_overflow_now = 0;
                }
            }
        }
        if self.rt_overflow_now > 0 {
            self.rt_overflow_frames += 1;
        }
        let needed = layout_plan
            .as_ref()
            .map(|plan| plan.end)
            .unwrap_or(self.rt_prev_count.max(self.meshes.len() as u32));
        if needed == 0 && self.rt_tlas.is_none() {
            return;
        }
        const TLAS_MAX: u32 = TLAS_MAX_INSTANCES; // matches the device limit requested in main.rs
        if self.rt_tlas.is_none() || self.rt_tlas_cap < needed.min(TLAS_MAX) {
            let cap = needed.next_multiple_of(1024).min(TLAS_MAX);
            self.rt_tlas = Some(device.create_tlas(&wgpu::CreateTlasDescriptor {
                label: Some("scene-tlas"),
                max_instances: cap,
                flags: wgpu::AccelerationStructureFlags::PREFER_FAST_BUILD,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            }));
            self.rt_tlas_cap = cap;
            self.rt_prev_count = 0;
            self.rt_batches_dirty = true; // fresh TLAS = every slot needs its first write
            if layout_plan.is_none() {
                layout_plan = Some(self.rt_slot_plan());
            }
            self.combine_rt_bind = None; // the integrator's TLAS bind must follow
        }
        // take/put so the TLAS writes can borrow mesh state immutably alongside.
        let mut tlas = self.rt_tlas.take().expect("tlas just ensured");
        let cap = self.rt_tlas_cap as usize;
        // Mesh slots [0..n_meshes): rewritten EVERY frame (models move, visible/emissive state
        // gates inclusion — mirrors the shadow-caster rule).
        let mut w = 0usize;
        for (i, m) in self.meshes.iter().enumerate() {
            if w >= cap {
                break;
            }
            tlas[w] = match &m.blas {
                Some(set) if m.visible && !m.raster_only => Some(wgpu::TlasInstance::new(
                    &set.blas,
                    crate::rt::rows_3x4(m.model),
                    regular_object_custom_data(i, m.temporal_dynamic),
                    if m.glass {
                        crate::rt::MASK_GLASS
                    } else {
                        crate::rt::MASK_WORLD
                    },
                )),
                _ => None,
            };
            w += 1;
        }
        // Batch tail: a changed count rewrites the compact layout, then clears its former tail.
        // Transform-only moving updates preserve the layout and rewrite only the moving ranges.
        if self.rt_batches_dirty {
            let plan = layout_plan
                .take()
                .expect("dirty RT batch topology requires a slot plan");
            let RtInstancedSlotPlan { offsets, end } = plan;
            self.rt_batch_slot_offsets = offsets;
            for (batch_index, b) in self.inst_batches.iter().enumerate() {
                let Some(start) = self
                    .rt_batch_slot_offsets
                    .get(batch_index)
                    .copied()
                    .flatten()
                else {
                    continue;
                };
                let Some(set) = &self.inst_prototypes[b.prototype].blas else {
                    continue;
                };
                // Hidden instances (failed building modules, rubble not yet revealed) take no
                // slot: the live ones fill the page's compact range in ordinal order.
                let mut slot = start as usize;
                for instance in 0..b.inst_count {
                    if b.is_hidden(instance) {
                        continue;
                    }
                    if slot >= cap {
                        break;
                    }
                    tlas[slot] = b.cpu_mats.get(instance as usize).map(|mat| {
                        wgpu::TlasInstance::new(
                            &set.blas,
                            crate::rt::rows_3x4(*mat),
                            b.object_custom_data(instance),
                            crate::rt::MASK_PROTO,
                        )
                    });
                    slot += 1;
                }
            }
            let new_count = end.min(self.rt_tlas_cap) as usize;
            for i in rt_vacated_slots(self.rt_prev_count, new_count as u32, self.rt_tlas_cap) {
                tlas[i] = None;
            }
            self.rt_prev_count = new_count as u32;
            self.rt_batches_dirty = false;
            self.rt_moving_batches_dirty = false;
        } else if self.rt_moving_batches_dirty {
            for (batch_index, b) in self.inst_batches.iter().enumerate() {
                if !b.moving_rt_pool {
                    continue;
                }
                let Some(start) = self
                    .rt_batch_slot_offsets
                    .get(batch_index)
                    .copied()
                    .flatten()
                else {
                    continue;
                };
                let Some(set) = &self.inst_prototypes[b.prototype].blas else {
                    continue;
                };
                for instance in 0..b.inst_count {
                    let slot = start as usize + instance as usize;
                    if slot >= cap {
                        break;
                    }
                    tlas[slot] = b.cpu_mats.get(instance as usize).map(|mat| {
                        wgpu::TlasInstance::new(
                            &set.blas,
                            crate::rt::rows_3x4(*mat),
                            b.object_custom_data(instance),
                            crate::rt::MASK_PROTO,
                        )
                    });
                }
            }
            self.rt_moving_batches_dirty = false;
        }
        self.rt_tlas = Some(tlas);
        encoder.build_acceleration_structures(
            std::iter::empty::<&wgpu::BlasBuildEntry>(),
            std::iter::once(self.rt_tlas.as_ref().unwrap()),
        );
        if self.rt_on && self.combine_rt_bind.is_none() {
            let rt_as_bgl = self
                .rt_as_bgl
                .as_ref()
                .expect("rt_on requires the ray-query bind-group layout");
            let surfaces = self
                .rt_surface_gpu
                .as_ref()
                .expect("ray-query transport requires exact surface buffers");
            self.combine_rt_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("combine-rt-bind"),
                layout: rt_as_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.rt_tlas.as_ref().unwrap().as_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: surfaces.vertices_a_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: surfaces.vertices_b_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: surfaces.records_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: surfaces.atlas_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: surfaces.instances_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: surfaces.daylight_proposals_buf.as_entire_binding(),
                    },
                ],
            }));
        }
    }

    /// The scene TLAS (post rt_update), for the debug/lit compute passes.
    pub fn scene_tlas(&self) -> Option<&wgpu::Tlas> {
        self.rt_tlas.as_ref()
    }

    /// Invalidate temporal history (explicit camera cut — view-mode switch, kill-cam, respawn).
    /// Auto-detection in accum_prepare catches most cuts; this is the explicit hook.
    pub fn reset_history(&mut self) {
        for v in &mut self.views {
            v.history_valid = false;
            v.volume_history_valid = false;
        }
    }

    /// The black interval starts a new replay; its first visible camera must always be a cut.
    pub fn reset_replay_history(&mut self) {
        self.replay_previous_elapsed = None;
        self.reset_history();
    }

    /// Select which per-view temporal context the next render uses (splitscreen viewports).
    /// Solo callers never touch this; everything defaults to view 0.
    pub fn set_active_view(&mut self, i: usize) {
        self.active_view = i.min(self.views.len() - 1);
    }

    /// Reset/read the actual pass and indexed-draw work encoded since the reset. Couch profiling
    /// brackets one simulation frame with these calls; ordinary rendering can ignore them.
    pub fn reset_render_work_stats(&self) {
        self.render_work.set(RenderWorkStats::default());
    }

    pub fn render_work_stats(&self) -> RenderWorkStats {
        self.render_work.get()
    }

    /// Capture diagnostics can name the actual path; accumulation-off retains its raw endpoint.
    pub fn surface_reconstruction_active(&self) -> bool {
        self.deferred && self.accum_on && self.surface_reconstruction.is_some()
    }

    /// One hull-local live probe. The Game supplies the actual physical hull frame; relay/RTT
    /// views subsequently clear the inside-view gate through set_interior_atmosphere_enabled.
    pub fn set_live_texel_probe_hull(&mut self, hull: glam::Mat4, enabled: bool) {
        self.live_texel_probe_view_inside = enabled;
        if let Some(probe) = &self.live_texel_probe { probe.set_hull(hull, enabled); }
    }

    pub fn live_texel_probe_active(&self) -> bool {
        self.deferred && self.accum_on && !self.menu_mode && self.live_texel_probe_view_inside
            && self.live_texel_probe.as_ref().is_some_and(|p|p.active())
    }

    fn live_probe_geometry_key(&self, hull: glam::Mat4) -> live_texel_probe::GeometrySignature {
        let inverse = hull.inverse();
        let center = hull.transform_point3(glam::Vec3::new(0.0, 1.5, 0.0));
        let nearby = |min: glam::Vec3, max: glam::Vec3| {
            min.is_finite() && max.is_finite() && !min.cmpgt(max).any()
                && center.distance_squared(center.clamp(min, max)) <= 100.0
        };
        let hash_model = |model: glam::Mat4, hash: &mut std::collections::hash_map::DefaultHasher| {
            for value in (inverse * model).to_cols_array() {
                ((value * 500.0).round() as i64).hash(hash);
            }
        };
        let mut entries = Vec::new();
        let mut dynamic_entries = Vec::new();
        for (id, mesh) in self.meshes.iter().enumerate() {
            if mesh.raster_only || !mesh.visible || mesh.blas.is_none()
                || !nearby(mesh.world_min, mesh.world_max) { continue; }
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            id.hash(&mut hash);
            mesh.index_count.hash(&mut hash);
            mesh.periscope_kind.hash(&mut hash);
            hash_model(mesh.model, &mut hash);
            ((mesh.emissive * 100.0).round() as i64).hash(&mut hash);
            // Explicit actor classification, not a slot allow-list or inferred amount of motion.
            // Hatches, breech, turret and other structural meshes retain full invalidation.
            if mesh.temporal_dynamic {
                dynamic_entries.push((regular_object_custom_data(id, true) as usize, hash.finish()));
            } else {
                entries.push((id, hash.finish()));
            }
        }
        // Static pages with per-instance visibility (compiled building modules and their rubble)
        // change structure without a matrix moving: a failed module near the hull is a
        // structural change exactly as a hidden loose mesh was.
        for batch in &self.inst_batches {
            if batch.hidden.is_empty() || batch.moving_rt_pool || batch.raster_only_dynamic
                || !self.batch_in_ray_scene(batch) || !nearby(batch.world_min, batch.world_max) {
                continue;
            }
            let prototype = &self.inst_prototypes[batch.prototype];
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            let mut hidden_nearby = 0usize;
            for (ordinal, model) in batch.cpu_mats.iter().take(batch.inst_count as usize).enumerate() {
                if !batch.is_hidden(ordinal as u32) { continue; }
                let (min, max) = transform_aabb(*model, prototype.local_min, prototype.local_max);
                if !nearby(min, max) { continue; }
                ordinal.hash(&mut hash);
                hidden_nearby += 1;
            }
            if hidden_nearby > 0 {
                entries.push((batch.object_custom_data(0) as usize, hash.finish()));
            }
        }
        // Authored actor pose pools use the same motion epoch as regular fallback bodies/rifles.
        // Hash live matrices only; the reserved dead tail does not affect source state or cost.
        for batch in &self.inst_batches {
            let prototype = &self.inst_prototypes[batch.prototype];
            if !batch.moving_rt_pool || batch.raster_only_dynamic || prototype.blas.is_none()
                || batch.inst_count == 0 || !nearby(batch.world_min, batch.world_max) { continue; }
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            batch.prototype.hash(&mut hash);
            batch.emissive.to_bits().hash(&mut hash);
            let mut live = 0usize;
            for (ordinal, model) in batch.cpu_mats.iter().take(batch.inst_count as usize).enumerate() {
                let (min, max) = transform_aabb(*model, prototype.local_min, prototype.local_max);
                if !nearby(min, max) { continue; }
                ordinal.hash(&mut hash);
                hash_model(*model, &mut hash);
                live += 1;
            }
            if live > 0 {
                dynamic_entries.push((batch.object_custom_data(0) as usize, hash.finish()));
            }
        }
        // Regular dynamic keys precede instanced dynamic keys, but sort explicitly so change
        // detection remains correct if source allocation order changes later.
        dynamic_entries.sort_unstable_by_key(|(id, _)| *id);
        let fingerprint = |items: &Vec<(usize, u64)>| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            items.hash(&mut hash); hash.finish()
        };
        live_texel_probe::GeometrySignature {
            key: fingerprint(&entries), entries,
            dynamic_key: fingerprint(&dynamic_entries), dynamic_entries,
        }
    }

    /// S3 per-frame temporal bookkeeping: camera-cut detection, per-slot prev-transform deltas
    /// (true rigid motion vectors), uniform upload, ping-pong flip. Must run before
    /// render_internal encodes the accum pass.
    fn accum_prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.deferred {
            return;
        }
        if let Some(gpu) = &self.surface_reconstruction {
            gpu.update_camera(queue, &self.camera);
        }
        let pos = self.camera.pos;
        let fwd = self.camera.forward();
        let fov = self.camera.fov_y;
        let (ppos, pfwd, pfov, paspect) = self.views[self.active_view].prev_cam_sig;
        // A cut = any camera discontinuity reprojection can't bridge: teleports (station
        // switches, hatch climb ends, kill-cam), FOV flips (optic zoom), hard whips.
        let cut = temporal_camera_cut((pos - ppos).length(), fwd.dot(pfwd), fov - pfov,
            self.camera.aspect - paspect,
            self.views[self.active_view].prev_depth_range != [self.camera.znear, self.camera.zfar],
            self.continuous_camera_translation);
        let reset = !self.views[self.active_view].history_valid || cut;
        let volume_active = self.smoke_active();
        let volume_reset = !self.views[self.active_view].volume_history_valid || cut;
        // Actor identities may grow when the canyon receives reinforcements. Resize the history
        // buffer and its shared binding before upload; existing source identities do not change.
        let required = (self.meshes.len().max(1) * 64) as u64;
        if required > self.delta_buf.size() {
            self.delta_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("growing-actor-history"), size: required.next_power_of_two(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.accum_delta_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("growing-actor-history-bind"),
                layout: &self.accum_pipe.get_bind_group_layout(1),
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.delta_buf.as_entire_binding() }],
            });
        }
        let previous_models = &mut self.views[self.active_view].prev_models;
        if previous_models.len() != self.meshes.len() {
            *previous_models = self.meshes.iter().map(|m| m.model).collect();
        }
        let deltas: Vec<[[f32; 4]; 4]> = self
            .meshes
            .iter()
            .zip(previous_models.iter())
            .map(|(m, pm)| {
                crate::camera::relative_history_delta(
                m.model, *pm, pos, ppos).to_cols_array_2d()
            })
            .collect();
        if !deltas.is_empty() {
            queue.write_buffer(&self.delta_buf, 0, bytemuck::cast_slice(&deltas));
        }
        let light_sources = light_history::Sources {
            sun_direction: self.sun_dir.normalize_or_zero().to_array(),
            solar_fraction: (1.0-self.night).max(0.0),
            weather: [self.overcast,self.fog_density,self.storm,self.precipitation],
            act: self.weather_act_id,
            lights: self.lights.iter().map(|light| light_history::Light {
                position: light.pos.to_array(), radius: light.source_radius,
                power: (light.color*light.radiant_intensity).to_array(),
            }).collect(),
        };
        let source_changed = light_sources.material_step_from(
            self.views[self.active_view].prev_light_sources.as_ref());
        let u = AccumUniform {
            prev_view_proj: self.views[self.active_view].prev_vp.to_cols_array_2d(),
            inv_view_proj: self.camera.relative_view_proj().inverse().to_cols_array_2d(),
            alpha: if self.accum_on { 0.15 } else { 1.0 },
            reset: if reset { 1.0 } else { 0.0 },
            test_noise: if self.accum_test { 1.0 } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            znear: self.camera.znear,
            zfar: self.camera.zfar,
            n_meshes: self.meshes.len() as f32,
            volume_reset: if volume_reset { 1.0 } else { 0.0 },
            sky_reprojection: sky_reprojection::matrix(fwd, fov, self.camera.aspect,
                pfwd, pfov, paspect).to_cols_array_2d(),
            surface_history: [if pos == ppos && fwd == pfwd && fov == pfov
                && self.camera.aspect == paspect { 1.0 } else { 0.0 },
                if source_changed {1.0}else{0.0}, 0.0, 0.0],
            camera_shift: (pos - ppos).extend(0.0).to_array(),
        };
        queue.write_buffer(&self.accum_ubuf, 0, bytemuck::bytes_of(&u));
        // Camera and object transforms must describe the same previous frame for THIS view.
        // The optic pass must not consume the cabin pass's object motion before it reprojects.
        let vc = &mut self.views[self.active_view];
        vc.prev_vp = self.camera.relative_view_proj();
        vc.prev_cam_sig = (pos, fwd, fov, self.camera.aspect);
        vc.prev_depth_range = [self.camera.znear, self.camera.zfar];
        vc.prev_light_sources = Some(light_sources);
        vc.accum_flip = 1 - vc.accum_flip;
        vc.history_valid = true;
        // An inactive stretch invalidates only volume history. Surface lighting may remain
        // perfectly continuous, while the next smoke/haze onset must begin from its current
        // physical estimate rather than whatever occupied this view's volume target minutes ago.
        vc.volume_history_valid = volume_active;
        for (pm, m) in vc.prev_models.iter_mut().zip(self.meshes.iter()) {
            *pm = m.model;
        }
    }

    pub fn begin_gpu_profile_frame(&self) {
        if let Some(profile) = &self.gpu_profile {
            profile.begin_frame();
        }
    }

    pub fn read_gpu_profile_frame(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        match &self.gpu_profile {
            Some(profile) => profile.read_frame(device, queue),
            None => Ok(Vec::new()),
        }
    }

    /// (BLAS count, TLAS instances written last frame) — the F3 stats overlay + gfxbench log.
    pub fn rt_stats(&self) -> (usize, u32) {
        let blases = self.meshes.iter().filter(|m| m.blas.is_some()).count()
            + self
                .inst_prototypes
                .iter()
                .filter(|p| p.blas.is_some())
                .count();
        (blases, self.rt_prev_count)
    }

    /// Render one full frame into the internal targets with no window and no readback — the
    /// gfxbench path. Identical GPU work to a live frame minus the trivial upscale blit.
    pub fn render_offscreen(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(device, queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench-frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(device, queue);
        self.render_internal(&mut encoder, true);
        queue.submit([encoder.finish()]);
    }

    /// Dump the just-completed active view's actual lighting targets without rendering, changing
    /// history, or changing transport. Raw IEEE half-float words are authoritative. PNGs are CPU
    /// previews at one shared exposure with ACES+sRGB and no dither; they are not the final crush.
    pub fn debug_capture_light_layers(
        &self, device: &wgpu::Device, queue: &wgpu::Queue, prefix: &str,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(self.deferred && self.rt_on, "light-layer diagnostic requires deferred RT");
        let manifest_path = format!("{prefix}_light_layers.json");
        ensure_output_parent(&manifest_path)?;
        let view = &self.views[self.active_view];
        // accum_prepare flips before the dispatch. This is the target actually used by resolve
        // and the volume composite in the completed frame, not the previous ping-pong target.
        let history = view.accum_flip;
        let eye = self.debug_read_active_exposure(device, queue)?;
        // Previews use the eye's cone channel only (no rods): a diagnostic, not the crush.
        let exposure = self.exposure_override.unwrap_or(eye[0]);
        let exposure = if exposure.is_finite() && exposure > 0.0 { exposure } else { DEFAULT_CAMERA_EXPOSURE };
        let g0_raw = debug_read_rgba16f(device, queue, self.g0_view.texture())?;
        let decode = |raw: &[u8]| -> Vec<[f32; 4]> {
            raw.chunks_exact(8).map(|pixel| {
                    std::array::from_fn(|c| {
                        debug_half_to_f32(u16::from_le_bytes([pixel[c*2],pixel[c*2+1]]))
                    })
                }).collect()
        };
        let g0 = decode(&g0_raw);
        let eye_ids_raw = debug_read_light_target(device, queue, self.g3_view.texture())?;
        let eye_ids_path = format!("{prefix}_g3_ids.r32u");
        std::fs::write(&eye_ids_path, &eye_ids_raw)?;
        let eye_ids: Vec<u32> = eye_ids_raw.chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap())).collect();
        anyhow::ensure!(eye_ids.len() == g0.len(), "eye ID dimensions differ from G0");
        // Preview mode: 0 radiance, 1 remodulated diffuse, 2 linear albedo, 3 normal false color.
        let mut layers = vec![
            ("diffuse_current", self.lit_diff_view.texture(), 1, "RGB demodulated diffuse; alpha negative marks diffuse; zero also carries glass"),
            ("diffuse_accumulated", view.debug_diff_history[history].texture(), 1, "RGB accumulated demodulated diffuse; alpha is history age, not opacity"),
            ("specular_current", self.lit_other_view.texture(), 0, "RGB specular radiance; alpha roughness"),
            ("specular_accumulated", view.debug_spec_history[history].texture(), 0, "RGB accumulated specular radiance; alpha is history age, not opacity"),
            ("volume_current", self.volume_current_view.texture(), 0, "RGB premultiplied volume radiance; alpha physical opacity; inactive may be stale"),
            ("volume_accumulated", view.debug_volume_history[history].texture(), 0, "RGB premultiplied accumulated volume radiance; alpha physical opacity; inactive may be stale"),
            ("immediate_emission", self.lit_emission_view.texture(), 0, "RGB immediate authored emission/transient"),
            ("g0_albedo", self.g0_view.texture(), 2, "RGB linear albedo; alpha surface mode"),
            ("g1_normal", self.g1_view.texture(), 3, "RGB signed world normal; alpha roughness"),
            ("g2_passthrough", self.g2_view.texture(), 0, "RGB actual primary-surface passthrough HDR before combine; alpha metallic"),
            ("hdr_final", &self.hdr_tex, 0, "Linear final HDR after composition/bloom and before display crush"),
        ];
        if self.surface_reconstruction_active() {
            layers.insert(2, ("diffuse_reconstructed",
                self.surface_reconstruction.as_ref().unwrap().output().texture(), 1,
                "RGB spatially reconstructed demodulated diffuse; original history age in alpha; never fed back to history"));
        }
        if self.live_texel_probe_active() {
            let probe = self.live_texel_probe.as_ref().unwrap();
            layers.insert(2, ("diffuse_probe_reused", probe.output().texture(), 1,
                "RGB validated live-probe diffuse or exact eye fallback; original eye history age; no temporal feedback"));
            layers.push(("probe_reuse_mask", probe.mask().texture(), 2,
                "R=accepted reuse; G=(face+1)/6; B=normalized gather weight; zero=exact eye fallback"));
        }
        let mut receipts = Vec::new();
        let mut emitting_pixel_ids = std::collections::BTreeMap::<u32, (u64, [f64; 3], f32)>::new();
        for (name, texture, preview_mode, semantics) in layers {
            let raw = if name == "g0_albedo" { g0_raw.clone() }
                else { debug_read_rgba16f(device, queue, texture)? };
            let raw_path = format!("{prefix}_{name}.rgba16f");
            std::fs::write(&raw_path, &raw)?;
            let pixels = decode(&raw);
            anyhow::ensure!(pixels.len() == g0.len(), "light-layer dimensions differ from G0");
            let mut png = Vec::with_capacity(pixels.len()*4);
            let mut nonfinite = 0usize;
            let mut rgb_squared_sum = [0.0f64; 3];
            for (index, pixel) in pixels.iter().enumerate() {
                if name == "immediate_emission" && pixel[..3].iter().any(|v| v.is_finite() && *v > 0.0) {
                    let entry = emitting_pixel_ids.entry(eye_ids[index]).or_insert((0, [0.0; 3], 0.0));
                    entry.0 += 1;
                    for c in 0..3 {
                        if pixel[c].is_finite() {
                            entry.1[c] += f64::from(pixel[c]);
                            entry.2 = entry.2.max(pixel[c]);
                        }
                    }
                }
                nonfinite += pixel.iter().filter(|v| !v.is_finite()).count();
                for c in 0..3 {
                    let raw_v = pixel[c];
                    if raw_v.is_finite() { rgb_squared_sum[c] += f64::from(raw_v).powi(2); }
                    let value = match preview_mode {
                        1 => {
                            // Match resolve's surface emission-mode exclusion and albedo remodulation.
                            let v = if g0[index][3] > 0.5 && g0[index][3] < 1.5 { 0.0 }
                                else { raw_v * g0[index][c] };
                            debug_linear_to_srgb(debug_light_aces(v * exposure))
                        }
                        2 => debug_linear_to_srgb(raw_v),
                        3 => (raw_v * 0.5 + 0.5).clamp(0.0, 1.0),
                        _ => debug_linear_to_srgb(debug_light_aces(raw_v * exposure)),
                    };
                    png.push((value * 255.0).round() as u8);
                }
                png.push(255);
            }
            let png_path = format!("{prefix}_{name}.png");
            image::RgbaImage::from_raw(texture.width(), texture.height(), png)
                .ok_or_else(|| anyhow::anyhow!("light-layer preview size mismatch"))?.save(&png_path)?;
            receipts.push(serde_json::json!({
                "layer":name, "raw":raw_path, "preview":png_path,
                "width":texture.width(), "height":texture.height(), "row_bytes":texture.width()*8,
                "format":"RGBA16Float", "byte_order":"little-endian", "row_order":"top-to-bottom",
                "semantics":semantics, "preview_mode":preview_mode,
                "nonfinite_channels":nonfinite, "rgb_squared_sum":rgb_squared_sum,
            }));
        }
        // Source indices are the same full IDs the primary draw supplies. There are no authored
        // names retained in MeshGpu; do not fabricate one or decode IDs from a lossy preview.
        const SOURCE_DIAGNOSTIC_LIMIT: usize = 256;
        let dynamic_regular_meshes: Vec<_> = self.meshes.iter().enumerate()
            .filter(|(_, mesh)| mesh.temporal_dynamic && mesh.visible)
            .map(|(index, mesh)| serde_json::json!({
                "source_mesh_index":index,
                "gbuffer_id":regular_object_custom_data(index, true),
                "model_column_major":mesh.model.to_cols_array(),
                "world_aabb_min":mesh.world_min.to_array(), "world_aabb_max":mesh.world_max.to_array(),
                "nearest_eye_distance_m":self.camera.pos.distance(self.camera.pos.clamp(mesh.world_min, mesh.world_max)),
                "has_blas":mesh.blas.is_some(), "raster_only":mesh.raster_only,
            })).take(SOURCE_DIAGNOSTIC_LIMIT).collect();
        let emissive_meshes: Vec<_> = self.meshes.iter().enumerate()
            .filter(|(_, mesh)| mesh.visible && mesh.emissive > 0.0)
            .map(|(index, mesh)| serde_json::json!({
                "source_mesh_index":index, "gbuffer_id":regular_object_custom_data(index, mesh.temporal_dynamic),
                "temporal_dynamic":mesh.temporal_dynamic,
                "emissive":mesh.emissive, "own_colour":mesh.emissive_own,
                "night_gated":mesh.emissive_win, "raster_only":mesh.raster_only,
                "glass":mesh.glass, "periscope_kind":mesh.periscope_kind,
                "triangles":mesh.index_count/3, "has_blas":mesh.blas.is_some(),
                "world_aabb_min":mesh.world_min.to_array(), "world_aabb_max":mesh.world_max.to_array(),
                "model_column_major":mesh.model.to_cols_array(),
                "finite":mesh.emissive.is_finite() && mesh.model.is_finite()
                    && mesh.world_min.is_finite() && mesh.world_max.is_finite(),
            })).collect();
        let emissive_batches: Vec<_> = self.inst_batches.iter().enumerate()
            .filter(|(_, batch)| batch.inst_count > 0 && batch.emissive > 0.0)
            .map(|(index, batch)| serde_json::json!({
                "source_batch_index":index, "prototype_index":batch.prototype,
                "emissive":batch.emissive, "material_flag":batch.flags,
                "active_count":batch.inst_count, "cpu_transform_count":batch.cpu_mats.len(),
                "moving_rt_pool":batch.moving_rt_pool, "raster_only_dynamic":batch.raster_only_dynamic,
                "has_blas":self.inst_prototypes[batch.prototype].blas.is_some(),
                "object_id_base_untagged":batch.object_id_base, "reserved_id_capacity":batch.object_id_capacity,
                "first_active_gbuffer_id":(batch.object_id_capacity > 0).then(|| batch.object_custom_data(0)),
                "world_aabb_min":batch.world_min.to_array(), "world_aabb_max":batch.world_max.to_array(),
                "finite":batch.emissive.is_finite() && batch.flags.is_finite()
                    && batch.world_min.is_finite() && batch.world_max.is_finite()
                    && batch.cpu_mats.iter().all(|model| model.is_finite()),
                "first_four_models_column_major":batch.cpu_mats.iter().take(4)
                    .map(|model| model.to_cols_array()).collect::<Vec<_>>(),
            })).collect();
        let mut emitting_pixel_ids: Vec<_> = emitting_pixel_ids.into_iter().collect();
        emitting_pixel_ids.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
        let emission_identity_summary: Vec<_> = emitting_pixel_ids.iter().take(SOURCE_DIAGNOSTIC_LIMIT)
            .map(|(id, (pixels, sum, peak))| {
                serde_json::json!({
                "full_gbuffer_id":id, "pixels":pixels, "immediate_rgb_sum":sum,
                "immediate_peak_channel":peak,
            })
            }).collect();
        std::fs::write(&manifest_path, serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"kessel.actual-light-layers.v1", "layers":receipts,
            "dynamic_regular_source_diagnostic":dynamic_regular_meshes,
            "eye_full_ids":{"raw":eye_ids_path,"format":"R32Uint","byte_order":"little-endian",
                "row_order":"top-to-bottom","width":self.g3_view.texture().width(),
                "height":self.g3_view.texture().height(),"row_bytes":self.g3_view.texture().width()*4,
                "semantics":"Unmodified full primary draw ID; bits 0..21 are the source ordinal, bit 22 marks an instance, bit 23 marks dynamic geometry (including regular actors). Consult G0 mode and depth for empty/background pixels.",
                "instanced_tag_mask":0x40_0000u32,"dynamic_tag_mask":0x80_0000u32,
                "instance_ordinal_mask":0x3f_ffffu32},
            "emissive_source_diagnostic":{
                "limit_per_list":SOURCE_DIAGNOSTIC_LIMIT,
                "visible_emissive_mesh_count":emissive_meshes.len(),
                "visible_emissive_meshes":emissive_meshes.iter().take(SOURCE_DIAGNOSTIC_LIMIT).collect::<Vec<_>>(),
                "active_emissive_batch_count":emissive_batches.len(),
                "active_emissive_batches":emissive_batches.iter().take(SOURCE_DIAGNOSTIC_LIMIT).collect::<Vec<_>>(),
                "actual_immediate_emitting_id_count":emitting_pixel_ids.len(),
                "actual_immediate_emitting_ids_by_pixel_count":emission_identity_summary,
                "tlas_capacity":self.rt_tlas_cap,"tlas_slot_count":self.rt_prev_count,
                "physical_instanced_live_count":self.inst_batches.iter().filter(|batch|
                    self.inst_prototypes[batch.prototype].blas.is_some() && batch.flags <= 3.5 && !batch.raster_only_dynamic)
                    .map(|batch|u64::from(batch.inst_count)).sum::<u64>(),
                "physical_instanced_reserved_id_count":self.inst_batches.iter().filter(|batch|
                    self.inst_prototypes[batch.prototype].blas.is_some() && batch.flags <= 3.5 && !batch.raster_only_dynamic)
                    .map(|batch|u64::from(batch.object_id_capacity)).sum::<u64>(),
                "scope":"Current CPU visibility/source metadata, not a claim that each candidate was on-screen. Actual emitting pixel identities are paired from the completed primary ID and immediate-emission targets. Counts expose list truncation; non-finite CPU values are flagged and serialized as null. No source textures or private paths are included."},
            "frame":self.frame, "active_view":self.active_view, "history_index":history,
            "transport_quality":self.transport_quality.key(),
            "surface_history_valid":view.history_valid, "volume_history_valid":view.volume_history_valid,
            "diffuse_reconstruction_active":self.surface_reconstruction_active(),
            "live_texel_probe_active":self.live_texel_probe_active(),
            "live_texel_probe":self.live_texel_probe.as_ref()
                .map(|p|p.debug_capture(device, queue, prefix)).transpose()?,
            "volume_active":self.smoke_active(), "camera_world_m":self.camera.pos.to_array(),
            "camera_forward":self.camera.forward().to_array(), "preview_exposure":exposure,
            "preview":"Shared exposure, CPU ACES+sRGB, no dither. Diffuse previews multiply actual G0 albedo. Raw RGB/alpha unchanged. G0 is sRGB albedo; G1 is normal false color.",
            "readback":"Existing completed targets only; no additional render, shader change or history reset",
        }))?)?;
        Ok(manifest_path)
    }

    /// Read the active view's measured exposure state for an opt-in headless renderer diagnostic.
    /// This is a pure GPU-buffer readback: it does not alter adaptation, scene radiance, or the
    /// presented image. Shipping frames never call it.
    pub fn debug_read_active_exposure(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<[f32; 2]> {
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure-diagnostic-readback"),
            size: 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("exposure-diagnostic"),
        });
        encoder.copy_buffer_to_buffer(&self.exp_bufs[self.active_view], 0, &readback, 0, 8);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = slice.get_mapped_range();
        let values = bytemuck::cast_slice::<u8, f32>(&mapped);
        let state = [values[0], values[1]];
        drop(mapped);
        readback.unmap();
        Ok(state)
    }

    /// Re-arm the active view's first-measurement sentinel after a headless transport pre-roll.
    ///
    /// The shipping render loop never calls this. A deterministic screenshot can temporarily pause
    /// auto-exposure while its stochastic surface/volume histories converge, invoke this hook, and
    /// then let the first *converged* HDR frame establish the eye state. This resets camera state;
    /// it does not alter scene radiance, weather, temporal transport, or the exposure algorithm.
    pub(crate) fn reset_active_exposure_for_headless_capture(&self, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.exp_bufs[self.active_view],
            0,
            bytemuck::cast_slice(&uninitialized_exposure_state()),
        );
    }

    /// Advance an already initialized headless eye by one 60 Hz step without advancing the
    /// scene, sky, weather or physics. Only the last-observation lane is written, never the
    /// exposure/adaptation lanes. Pair with finish_headless_exposure_step even on readback error.
    pub(crate) fn begin_headless_exposure_step(&mut self, queue: &wgpu::Queue) -> Option<f32> {
        let saved = self.replay_exposure_time;
        let [current, previous, _] = headless_exposure_step_clocks(saved.unwrap_or(self.time));
        self.replay_exposure_time = Some(current);
        queue.write_buffer(&self.exp_bufs[self.active_view], std::mem::size_of::<f32>() as u64,
            bytemuck::bytes_of(&previous));
        saved
    }

    pub(crate) fn finish_headless_exposure_step(&mut self, queue: &wgpu::Queue,
        previous_override: Option<f32>) {
        self.replay_exposure_time = previous_override;
        let [_, _, actual] = headless_exposure_step_clocks(previous_override.unwrap_or(self.time));
        queue.write_buffer(&self.exp_bufs[self.active_view], std::mem::size_of::<f32>() as u64,
            bytemuck::bytes_of(&actual));
    }

    /// Read back the raster depth attachment, LINEARIZED to view-space metres (misses → 1e9) —
    /// the S1 bvhview correlation reference. Blocking; headless verification only.
    pub fn read_depth(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<f32> {
        let (w, h) = (self.iw, self.ih);
        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth-readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("depth-read"),
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.depth_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let raw = buf.slice(..).get_mapped_range().to_vec();
        buf.unmap();
        let (n, f) = (self.camera.znear, self.camera.zfar);
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let base = (y * row) as usize;
            let px: &[f32] = bytemuck::cast_slice(&raw[base..base + (w * 4) as usize]);
            out.extend(px.iter().map(|&d| {
                if d >= 1.0 {
                    1e9
                } else {
                    n * f / (f - d * (f - n))
                }
            }));
        }
        out
    }

    /// Render a continuous death-replay camera without advancing the frozen world clock. The
    /// exposure clock follows actual replay elapsed seconds, independent of rendered frame rate.
    pub fn render_replay_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        elapsed_s: f32,
    ) {
        let elapsed = self.prepare_replay_frame(elapsed_s, ReplayCapturePolicy::Current);
        self.render_to_surface(device, queue, view, format);
        self.finish_replay_frame(elapsed);
    }

    fn prepare_replay_frame(&mut self, elapsed_s: f32, policy: ReplayCapturePolicy) -> f32 {
        let (elapsed, cut, continuous, exposure_time) = replay_policy_parameters(
            self.replay_previous_elapsed, elapsed_s, self.time, policy);
        if cut { self.reset_history(); }
        self.continuous_camera_translation = continuous;
        self.replay_exposure_time = Some(exposure_time);
        elapsed
    }

    fn finish_replay_frame(&mut self, elapsed: f32) {
        self.replay_previous_elapsed = Some(elapsed);
        self.replay_exposure_time = None;
        self.continuous_camera_translation = false;
    }

    /// Diagnostic-only restart: all A/B passes begin at identical random-frame and eye states.
    /// The caller retains one frozen Game and reposes its exact replay FX/camera for each pass.
    pub(crate) fn reset_replay_motion_capture(&mut self, queue: &wgpu::Queue) {
        self.set_active_view(0);
        self.frame = 0;
        self.reset_replay_history();
        self.replay_exposure_time = None;
        self.continuous_camera_translation = false;
        self.reset_active_exposure_for_headless_capture(queue);
    }

    pub(crate) fn render_replay_offscreen(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        elapsed_s: f32, policy: ReplayCapturePolicy) {
        let elapsed = self.prepare_replay_frame(elapsed_s, policy);
        self.render_offscreen(device, queue);
        self.finish_replay_frame(elapsed);
    }

    /// The same prepare/finish policy and scene passes as live replay, with one native-size
    /// capture replacing the window blit. This performs exactly one render per returned frame.
    pub(crate) fn render_replay_rgba(&mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        elapsed_s: f32, policy: ReplayCapturePolicy) -> anyhow::Result<Vec<u8>> {
        let elapsed = self.prepare_replay_frame(elapsed_s, policy);
        let result = self.capture_rgba(device, queue, self.iw, self.ih, false);
        self.finish_replay_frame(elapsed);
        result
    }

    pub fn render_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        if self.replay_exposure_time.is_none() { self.replay_previous_elapsed = None; }
        // Solo always renders through temporal context 0 with a full-frame blit.
        self.set_active_view(0);
        queue.write_buffer(
            &self.blit_ubuf,
            0,
            bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]),
        );
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(device, queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(device, queue);
        self.render_internal(&mut encoder, true);
        self.blit(device, &mut encoder, view, format);
        queue.submit([encoder.finish()]);
    }

    /// GUNNER EYEPIECE COMPOSITE, pass A: render the full optic frame (the caller has already
    /// posed the optic camera, reticle and circular mask) and snapshot the CRUSHED result into
    /// `optic_tex`. Nothing reaches the window — pass B (an interior render with optic_mask =
    /// 4.0) drops this snapshot into the eyepiece circle during its own crush.
    pub fn render_optic_snapshot(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(device, queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("optic-pass"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(device, queue);
        self.optic_hide_pass = true;
        self.render_internal(&mut encoder, true);
        self.optic_hide_pass = false;
        encoder.copy_texture_to_texture(
            self.ldr_tex.as_image_copy(),
            self.optic_tex.as_image_copy(),
            wgpu::Extent3d {
                width: self.iw,
                height: self.ih,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
    }

    /// One SPLITSCREEN viewport: a complete internal-resolution frame (the caller has posed
    /// `self.camera`, reticle/optic mask and `set_active_view` for this seat), blitted into the
    /// view's window cell with a source crop. Call once per view per frame; the first view
    /// clears the surface, later ones paint their cells over it. Set `rt_skip` for views 2..N
    /// so the frame's TLAS is built once.
    pub fn render_to_surface_view(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        rect: &BlitRect,
    ) {
        queue.write_buffer(&self.blit_ubuf, 0, bytemuck::bytes_of(&rect.src_uv));
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(device, queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("coop-view"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(device, queue);
        self.render_internal(&mut encoder, true);
        self.blit_rect(device, &mut encoder, view, format, Some(rect));
        queue.submit([encoder.finish()]);
    }

    /// Composite the queued UI text OVER an already-rendered surface frame (LoadOp::Load) — the
    /// in-game HUD path (mission text, held map, interaction prompts). Call after
    /// `render_to_surface`, before present.
    /// Hardware previews put text on the panel's pixel grid too. Ordinary desktop UI keeps
    /// its existing sharp output-resolution path. Split-screen overlays retain their layout.
    fn encode_preview_ui(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView, format: wgpu::TextureFormat, over: bool) -> bool {
        if DISPLAY_INTERNAL_RES.with(Cell::get).is_none() || self.active_view != 0 { return false; }
        if over {
            self.ui.render_over(device, encoder, &self.ldr_view, LDR, self.iw, self.ih);
        } else {
            self.ui.render(device, encoder, &self.ldr_view, LDR, self.iw, self.ih);
        }
        self.blit(device, encoder, view, format);
        true
    }

    pub fn render_hud_over_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("hud") });
        if !self.encode_preview_ui(device, &mut encoder, view, format, true) {
            self.ui.render_over(device, &mut encoder, view, format, cw, ch);
        }
        queue.submit([encoder.finish()]);
    }

    pub fn render_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.capture_png(device, queue, path, cw, ch, false)
    }

    /// Scene-referred photometry of the last rendered HDR frame (before bloom is not separable
    /// here: the HDR target holds transported radiance plus the camera's bloom halo) and the active
    /// view's eye state. Diagnostics and the backend-agreement regression only.
    pub fn debug_frame_photometry(&self, device: &wgpu::Device, queue: &wgpu::Queue)
        -> anyhow::Result<FramePhotometry>
    {
        let raw = debug_read_rgba_f32(device, queue, &self.hdr_tex)?;
        let weights = crate::observer::PHOTOPIC_WEIGHTS;
        let mut luminance: Vec<f64> = raw.iter().map(|pixel| {
            (0..3).map(|c| pixel[c] as f64 * weights[c]).sum::<f64>().max(0.0)
        }).map(|v| if v.is_finite() { v } else { 0.0 }).collect();
        anyhow::ensure!(!luminance.is_empty(), "empty HDR frame");
        // Mean luminance of an 8 x 4 grid of image regions (row-major from the top left).
        let (w, h) = (self.iw as usize, self.ih as usize);
        let mut grid = [0.0f64; 32];
        let mut counts = [0usize; 32];
        for (i, value) in luminance.iter().enumerate() {
            let (x, y) = (i % w, i / w);
            let cell = (y * 4 / h.max(1)).min(3) * 8 + (x * 8 / w.max(1)).min(7);
            grid[cell] += value;
            counts[cell] += 1;
        }
        for (g, c) in grid.iter_mut().zip(counts) { *g /= c.max(1) as f64; }
        let n = luminance.len() as f64;
        let mean = luminance.iter().sum::<f64>() / n;
        let log_mean = (luminance.iter().map(|v| v.max(1.0e-9).ln()).sum::<f64>() / n).exp();
        luminance.sort_by(f64::total_cmp);
        let quantile = |q: f64| luminance[((luminance.len() - 1) as f64 * q) as usize];
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("eye-diagnostic-readback"),
            size: (EYE_STATE_LANES * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("eye-diagnostic"),
        });
        encoder.copy_buffer_to_buffer(&self.exp_bufs[self.active_view], 0, &readback, 0,
            (EYE_STATE_LANES * 4) as u64);
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = tx.send(result); });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mut eye = [0.0; EYE_STATE_LANES];
        eye.copy_from_slice(bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()));
        readback.unmap();
        let probe = self.environment_probe.debug_read(device, queue)?;
        Ok(FramePhotometry {
            mean_cd_m2: mean,
            log_mean_cd_m2: log_mean,
            median_cd_m2: quantile(0.5),
            p10_cd_m2: quantile(0.1),
            p90_cd_m2: quantile(0.9),
            grid_cd_m2: grid,
            eye,
            probe_beam: probe[environment_probe::PROBE_BEAM],
            probe_horizontal: probe[environment_probe::PROBE_HORIZONTAL],
            key_body: format!("{:?}", self.celestial.key.body),
            sun_altitude_deg: self.celestial.state.sun.altitude_deg,
            moon_altitude_deg: self.celestial.state.moon.altitude_deg,
            instant: self.sky_clock.now().to_iso(),
        })
    }

    /// Like `render_to_png` but composites the queued HUD (images + text) over the frame — the
    /// editor's headless verify path, so the asset browser panel shows up in the captured PNG.
    pub fn render_to_png_hud(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.capture_png(device, queue, path, cw, ch, true)
    }

    /// Render one frame and return tightly packed RGBA8 pixels. The cinematic recorder streams
    /// these directly into ffmpeg, avoiding thousands of temporary PNGs and their encode/decode
    /// pass. Capture at the internal resolution and let ffmpeg nearest-upscale for an identical
    /// crisp retro image at 1080p/4K.
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<Vec<u8>> {
        self.capture_rgba(device, queue, cw, ch, false)
    }

    /// Render one coherent frame with its queued pixel HUD, for annotated audio/video captures.
    pub fn render_to_rgba_with_hud(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<Vec<u8>> {
        self.capture_rgba(device, queue, cw, ch, true)
    }

    fn capture_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
        hud: bool,
    ) -> anyhow::Result<()> {
        let pixels = self.capture_rgba(device, queue, cw, ch, hud)?;
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("capture buffer wrong size"))?;
        ensure_output_parent(path)?;
        img.save(path)?;
        println!(
            "[panzerrechnersystem] wrote {path} ({cw}x{ch}, internal {}x{}, retro {:.2})",
            self.iw, self.ih, self.master_retro
        );
        Ok(())
    }

    fn capture_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cw: u32,
        ch: u32,
        hud: bool,
    ) -> anyhow::Result<Vec<u8>> {
        // A capture is a real rendered frame, just like a window/split-screen/offscreen frame.
        // Advance the stochastic clock before uploading uniforms so cinematic recording does not
        // reuse one pixel-seeded surface/volume sample for the entire take. Repeating a frozen
        // sample defeats both temporal accumulators and reads as dirt glued to the camera.
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.reset_blit_crop(queue);
        self.upload_uniforms(device, queue);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("capture"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(device, queue);
        self.render_internal(&mut encoder, true);
        self.blit(device, &mut encoder, &cap_view, format);
        if hud && !self.encode_preview_ui(device, &mut encoder, &cap_view, format, true) {
            self.ui.render_over(device, &mut encoder, &cap_view, format, cw, ch);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        Ok(pixels)
    }

    /// Compatibility no-op for the removed pasted-camera atlas diagnostic.
    pub fn upload_rtt_test_pattern(&self, queue: &wgpu::Queue) {
        let _ = queue;
    }

    /// Compatibility error for the removed pasted-camera atlas diagnostic.
    pub fn dump_rtt_atlas(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
    ) -> anyhow::Result<()> {
        let _ = (device, queue, path);
        anyhow::bail!(
            "pasted-camera RTT atlas removed; physical optical transport is not implemented"
        )
    }

    // ── Front-end text/UI ────────────────────────────────────────────────────────────────────────
    // The menu/death front-end queues strings with `draw_text` / `draw_text_bold`, then flushes with
    // one of the `render_ui_*` entry points, which clears to the gradient/black background and draws
    // the glyph quads at DISPLAY resolution (crisp — never through the crush). See src/ui.rs.

    /// Set the background gradient (linear RGBA top → bottom) drawn behind the queued UI text.
    pub fn set_ui_bg(&mut self, top: [f32; 4], bottom: [f32; 4]) {
        self.ui.set_bg(top, bottom);
    }

    /// Drop any queued (un-flushed) UI strings.
    pub fn ui_reset(&mut self) {
        self.ui.clear();
    }

    /// Queue a body-weight string. `x`/`y` are normalized [0,1] (origin top-left), `size` is the em
    /// height as a fraction of the target height, `color` is linear RGBA, `align` is horizontal.
    pub fn draw_text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        align: Align,
    ) {
        self.ui.push(text, x, y, size, color, align, false);
    }

    /// Queue a bold-weight string (title / headline / menu items). Same coordinate convention.
    pub fn draw_text_bold(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
        align: Align,
    ) {
        self.ui.push(text, x, y, size, color, align, true);
    }

    /// Register an RGBA8 image (asset-browser thumbnail, preview inset) → a stable handle for
    /// `draw_image`. See `ui::Ui::upload_image`.
    pub fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> u32 {
        self.ui.upload_image(device, queue, rgba, w, h)
    }

    /// Queue an image quad at normalized `x,y,w,h` (top-left origin), tinted by `color`. Drawn in
    /// the HUD over-pass, before text, at display resolution.
    pub fn draw_image(&mut self, handle: u32, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.ui.draw_image(handle, x, y, w, h, color);
    }

    /// Queue a filled tinted rectangle (panels, selection highlights, the drag ghost).
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.ui.fill_rect(x, y, w, h, color);
    }

    /// Flush the queued UI at the output resolution. The pixel fonts supply the visual style;
    /// menu labels and controls must stay readable when the 3D scene uses a smaller pixel grid.
    pub fn render_ui_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        cw: u32,
        ch: u32,
    ) {
        self.reset_blit_crop(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-frame"),
        });
        if !self.encode_preview_ui(device, &mut encoder, view, format, false) {
            self.ui.render(device, &mut encoder, view, format, cw, ch);
        }
        queue.submit([encoder.finish()]);
    }

    /// Flush the queued UI to a PNG at `cw`×`ch` (headless verification of the title/death screens).
    pub fn render_ui_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.reset_blit_crop(queue);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-capture"),
        });
        // Match the live UI: glyphs are rasterized at the output size, independent of scene detail.
        if !self.encode_preview_ui(device, &mut encoder, &cap_view, format, false) {
            self.ui.render(device, &mut encoder, &cap_view, format, cw, ch);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("ui capture buffer wrong size"))?;
        ensure_output_parent(path)?;
        img.save(path)?;
        println!("[panzerrechnersystem] wrote UI {path} ({cw}x{ch})");
        Ok(())
    }

    /// Actual driver-view backdrop, converged once per menu entry and then copied at UI cost.
    /// The caller supplies the same driver camera used by gameplay. No separate decorative slit.
    pub fn invalidate_title_backdrop(&mut self) {
        self.title_backdrop_frames = 0;
        self.title_backdrop_settled = false;
        self.title_last_frame = None;
        self.title_eye_convergence = crate::HeadlessExposureConvergence::default();
    }

    pub fn title_backdrop_uninitialized(&self) -> bool {
        self.title_backdrop_frames == 0
    }

    pub fn render_driver_menu_to_surface(
        &mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        view: &wgpu::TextureView, format: wgpu::TextureFormat,
    ) {
        let now = std::time::Instant::now();
        let elapsed = self.title_last_frame.replace(now)
            .map_or(0.0, |previous| now.duration_since(previous).as_secs_f32());
        if let Err(error) = self.render_driver_menu_frame(device, queue, view, format, true, elapsed) {
            eprintln!("[menu] eye convergence readback failed; backdrop remains live: {error:#}");
        }
    }

    fn render_driver_menu_frame(
        &mut self, device: &wgpu::Device, queue: &wgpu::Queue,
        view: &wgpu::TextureView, format: wgpu::TextureFormat, draw_ui: bool, eye_dt_s: f32,
    ) -> anyhow::Result<()> {
        let size = wgpu::Extent3d { width: self.iw, height: self.ih, depth_or_array_layers: 1 };
        if self.title_backdrop.is_none() {
            self.title_backdrop = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("settled-driver-title"), size, mip_level_count: 1, sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: LDR,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }));
        }
        self.set_active_view(0);
        self.reset_blit_crop(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("driver-title-frame"),
        });
        let observing = !self.title_backdrop_settled;
        let saved_clock = self.replay_exposure_time;
        let actual_clock = saved_clock.unwrap_or(self.time.max(0.0));
        let eye_clock = title_exposure_clock(actual_clock, eye_dt_s);
        if observing {
            // Only the observer follows menu elapsed time. The scene remains frozen, and its
            // original timestamp is restored below so resumed gameplay has no future eye clock.
            self.replay_exposure_time = Some(eye_clock);
            self.frame = self.frame.wrapping_add(1);
            self.menu_mode = false;
            self.upload_uniforms(device, queue);
            self.rt_update(device, &mut encoder);
            self.accum_prepare(device, queue);
            self.render_internal(&mut encoder, true);
            encoder.copy_texture_to_texture(self.ldr_tex.as_image_copy(),
                self.title_backdrop.as_ref().unwrap().as_image_copy(), size);
            self.title_backdrop_frames += 1;
        } else {
            encoder.copy_texture_to_texture(self.title_backdrop.as_ref().unwrap().as_image_copy(),
                self.ldr_tex.as_image_copy(), size);
        }
        self.blit(device, &mut encoder, view, format);
        if draw_ui && !self.encode_preview_ui(device, &mut encoder, view, format, true) {
            self.ui.render_over(device, &mut encoder, view, format,
                view.texture().width(), view.texture().height());
        }
        queue.submit([encoder.finish()]);
        self.replay_exposure_time = saved_clock;
        if observing {
            let metered = self.autoexp && self.exposure_override.is_none();
            let stable = if metered {
                queue.write_buffer(&self.exp_bufs[self.active_view], std::mem::size_of::<f32>() as u64,
                    bytemuck::bytes_of(&actual_clock));
                // A small synchronous readback is limited to the unsteady title. After
                // convergence the menu returns to cached scene/UI-only rendering.
                let [exposure, _] = self.debug_read_active_exposure(device, queue)?;
                eye_clock > actual_clock && self.title_eye_convergence.observe(exposure)
            } else { true };
            self.title_backdrop_settled = title_backdrop_ready(self.title_backdrop_frames, stable);
        }
        Ok(())
    }

    /// Flush the legacy crew setup menu to a swapchain view: the turntable Tiger on pure
    /// black) nearest-upscaled to the display, with readable front-end text composited at output
    /// resolution. Call `set_menu_tiger(angle)` + queue the text (`draw_menu`) before this.
    pub fn render_menu_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = true; // crush.wgsl → keep the black backdrop pure #000 (no grain speckle)
        self.reset_blit_crop(queue);
        self.upload_uniforms(device, queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu-frame"),
        });
        self.render_menu_internal(&mut encoder);
        self.blit(device, &mut encoder, view, format);
        if !self.encode_preview_ui(device, &mut encoder, view, format, true) {
            self.ui.render_over(device, &mut encoder, view, format,
                view.texture().width(), view.texture().height());
        }
        queue.submit([encoder.finish()]);
    }

    /// Exercise the same driver backdrop cache, UI composition and upscale as the live title.
    /// The last two frames use the settled cache. Draw queued UI once on the final capture frame,
    /// after the scene history settles, so warmup does not drain the caller's labels.
    pub fn render_driver_menu_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let cap = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("menu-capture"),
            size: wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let cap_view = cap.create_view(&wgpu::TextureViewDescriptor::default());
        self.invalidate_title_backdrop();
        let mut cached_ms = Vec::new();
        // Optional diagnostic logging/continuation does not change the observer gate or live
        // menu behavior. The normal title budget includes measured dark-adaptation headroom.
        let diagnostic_frames = std::env::var("KESSEL_TITLE_EYE_DIAGNOSTIC_FRAMES").ok();
        let max_frames = title_capture_frame_budget(diagnostic_frames.as_deref());
        for frame in 0..max_frames {
            self.render_driver_menu_frame(device, queue, &cap_view, format, false, 1.0 / 60.0)?;
            if diagnostic_frames.is_some() && (frame == 0 || (frame + 1) % 60 == 0 || self.title_backdrop_settled) {
                let p = self.debug_frame_photometry(device, queue)?;
                println!("[title-eye] {}", serde_json::json!({"frame":frame+1,
                    "observer_elapsed_s":(frame+1) as f32/60.0,"scene_time_s":self.time,
                    "camera":self.camera.pos.to_array(),"camera_forward":self.camera.forward().to_array(),
                    "interior_light":self.interior_light,"shared_shadow_scope":self.coop_shadow.view_proj.is_some(),
                    "eye":p.eye,"hdr_mean_cd_m2":p.mean_cd_m2,"hdr_log_mean_cd_m2":p.log_mean_cd_m2,
                    "stable_observations":self.title_eye_convergence.stable_observations,
                    "settled":self.title_backdrop_settled}));
            }
            if self.title_backdrop_settled { break; }
        }
        anyhow::ensure!(self.title_backdrop_settled,
            "driver title eye did not converge within {} frozen-scene observations",
            max_frames);
        for frame in 0..2 {
            let began = std::time::Instant::now();
            self.render_driver_menu_frame(device, queue, &cap_view, format, frame == 1, 0.0)?;
            device.poll(wgpu::PollType::wait_indefinitely())?;
            cached_ms.push(began.elapsed().as_secs_f64() * 1000.0);
        }
        println!("[menu] {} scene frames, eye converged; two fenced cache frames (UI on final frame) {cached_ms:?} ms",
            self.title_backdrop_frames);
        let unpadded = cw * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("menu-readback"),
            size: (padded * ch) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu-capture"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &cap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(ch),
                },
            },
            wgpu::Extent3d {
                width: cw,
                height: ch,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded * ch) as usize);
        for row in 0..ch {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&data[start..start + unpadded as usize]);
        }
        drop(data);
        buf.unmap();
        let img = image::RgbaImage::from_raw(cw, ch, pixels)
            .ok_or_else(|| anyhow::anyhow!("menu capture buffer wrong size"))?;
        ensure_output_parent(path)?;
        img.save(path)?;
        println!("[panzerrechnersystem] wrote menu {path} ({cw}x{ch})");
        Ok(())
    }
}

// ---- small wgpu helpers ----

/// An instance transform the exact ray scene can invert, and whether it had to be replaced: a
/// transform whose linear part is singular or non-finite becomes a point-sized (1 mm) box at
/// the same translation, which no ray meets.
fn rt_safe_instance_transform(model: glam::Mat4) -> (glam::Mat4, bool) {
    let finite = model.to_cols_array().iter().all(|v| v.is_finite());
    let determinant = glam::Mat3::from_mat4(model).determinant();
    if finite && determinant.is_finite() && determinant.abs() > 1.0e-9 {
        return (model, false);
    }
    let translation = if finite { model.w_axis.truncate() } else { glam::Vec3::ZERO };
    (glam::Mat4::from_scale_rotation_translation(glam::Vec3::splat(1.0e-3), glam::Quat::IDENTITY, translation), true)
}

fn camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-bgl"),
            entries: &[
                uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false,
                    }, count: None,
                },
                sampler_entry(2),
            ],
        })
}

fn cabin_medium_texture_entry(binding:u32)->wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {binding,visibility:wgpu::ShaderStages::FRAGMENT,
        ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Float {filterable:false},
            view_dimension:wgpu::TextureViewDimension::D2Array,multisampled:false},count:None}
}

fn portable_visibility_enabled() -> bool {
    matches!(std::env::var("KESSEL_SOURCE_VISIBILITY").as_deref(), Ok("portable" | "experimental" | "bvh"))
}
fn terrain_material_shader_source(src: &str) -> String {
    terrain_material_shader_source_with_visibility(src, portable_visibility_enabled())
}
fn terrain_material_shader_source_with_visibility(src: &str, portable: bool) -> String {
    let visibility = |group,geometry,instances| if portable { crate::scene_visibility::shader_source(group,geometry,instances) } else { String::new() };
    let lighting = if portable {include_str!("../shaders/scene_light_visibility.wgsl")} else {include_str!("../shaders/shadow_map_visibility.wgsl")};
    let compare = if src.contains("// @scene-visibility-mesh") {"shadow_samp"} else {"sky_visibility_compare"};
    src.replace("// @raster-particle-lighting",include_str!("../shaders/raster_particle_lighting.wgsl"))
    .replace(
        "// @campaign-terrain-material",
        include_str!("../shaders/terrain_material.wgsl"),
    ).replace("// @camera-relative-projection", include_str!("../shaders/camera_relative.wgsl"))
    .replace("// @sky-visibility-sampling", include_str!("../shaders/sky_visibility_sampling.wgsl"))
    .replace("// @cabin-visibility-sampling", include_str!("../shaders/cabin_visibility.wgsl"))
    .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
    .replace("// @cabin-medium",include_str!("../shaders/cabin_medium.wgsl"))
    .replace("// @scene-visibility-mesh", &visibility(2,9,10))
    .replace("// @scene-visibility-smoke", &visibility(3,3,4))
    .replace("// @scene-visibility-particles", &visibility(2,3,4))
    .replace("// @scene-light-visibility",lighting)
    .replace("SHADOW_SKY_COMPARE",compare)
}

fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(terrain_material_shader_source(src).into()),
    })
}

fn color_target(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn attachment(
    view: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
            load,
            store: wgpu::StoreOp::Store,
        },
    }
}

fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_entry(
    binding: u32,
    vis: wgpu::ShaderStages,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

/// Fullscreen bloom pipeline with a chosen fragment entry point and optional (additive) blend.
/// Targets the HDR format; a FRAGMENT push-constant range (BloomPush) is supplied by `layout`.
fn bloom_pipe(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bloom"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: hdr_format(),
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn fullscreen_pipe(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    fullscreen_pipe_cached(device, module, layout, format, None)
}

fn fullscreen_pipe_cached(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("fullscreen"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            targets: &[Some(color_target(format))],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache,
    })
}

#[cfg(test)]
mod atmosphere_tests {
    use super::*;

    #[test]
    fn replay_capture_policies_isolate_history_from_exposure() {
        let parameters = |policy| replay_policy_parameters(Some(1.4), 1.5, 123.0, policy);
        assert_eq!(parameters(ReplayCapturePolicy::Legacy), (1.5, false, false, 123.0));
        assert_eq!(parameters(ReplayCapturePolicy::ContinuityOnly), (1.5, false, true, 123.0));
        assert_eq!(parameters(ReplayCapturePolicy::Current), (1.5, false, true, 124.5));
        for policy in [ReplayCapturePolicy::Legacy, ReplayCapturePolicy::ContinuityOnly,
            ReplayCapturePolicy::Current] {
            let (_, cut, continuous, _) = replay_policy_parameters(None, 1.5, 123.0, policy);
            assert!(cut && !continuous, "every pass starts at a real camera cut");
            let (_, cut, continuous, _) = replay_policy_parameters(Some(2.0), 1.5, 123.0, policy);
            assert!(cut && !continuous, "rewinding cannot inherit the previous path's history");
        }
    }

    #[test]
    fn replay_history_policy_bypasses_only_continuous_translation() {
        assert!(temporal_camera_cut(6.0, 1.0, 0.0, 0.0, false, false),
            "ordinary station/respawn teleports still cut");
        assert!(!temporal_camera_cut(6.0, 1.0, 0.0, 0.0, false, true),
            "a fast authored replay dolly can reproject the same ground");
        assert!(!temporal_camera_cut(0.4, 0.99, 0.0, 0.0, false, false));
        for (direction, fov, aspect, depth_changed) in [
            (0.49, 0.0, 0.0, false),
            (1.0, 0.01, 0.0, false),
            (1.0, -0.01, 0.0, false),
            (1.0, 0.0, 0.001, false),
            (1.0, 0.0, -0.001, false),
            (1.0, 0.0, 0.0, true),
        ] {
            assert!(temporal_camera_cut(6.0, direction, fov, aspect, depth_changed, true),
                "replay continuity must not suppress a non-translation camera cut");
        }
    }

    #[test]
    fn replay_history_policy_starts_and_rewinds_with_a_cut() {
        assert!(!replay_frame_is_continuous(None, 0.03));
        assert!(replay_frame_is_continuous(Some(0.03), 0.08));
        assert!(replay_frame_is_continuous(Some(0.08), 0.08));
        assert!(!replay_frame_is_continuous(Some(4.4), 0.03));
        assert!(!replay_frame_is_continuous(None, 0.0),
            "the black interval clears the previous replay even at an identical elapsed time");
    }

    #[test]
    fn replay_exposure_clock_adapts_by_elapsed_seconds_with_a_frozen_scene() {
        let scene_time = 123.0;
        let duration = 1.5;
        let target = 4.0;
        let expected = target + (350.0 - target) * (-duration / 0.22_f32).exp();
        for fps in [24, 30, 60, 144] {
            let mut exposure = 350.0;
            let mut previous = replay_exposure_time(scene_time, 0.0);
            for frame in 1..=(duration * fps as f32) as u32 {
                let clock = replay_exposure_time(scene_time, frame as f32 / fps as f32);
                let dt = (clock - previous).clamp(1.0 / 240.0, 0.25);
                // The existing exposure shader's bright-adaptation response, driven solely by
                // the observer clock. The recorded world remains at scene_time for every frame.
                exposure += (target - exposure) * (1.0 - (-dt / 0.22).exp());
                previous = clock;
            }
            assert!((exposure - expected).abs() < 0.001, "{fps} fps: {exposure} vs {expected}");
            assert_eq!(previous, scene_time + duration);
        }
        assert_eq!(replay_exposure_time(scene_time, -1.0), scene_time);
    }

    #[test]
    fn frontend_text_is_composited_once_at_output_resolution() {
        let source = include_str!("renderer.rs");
        let method = |name: &str| {
            let start = source.find(&format!("fn {name}(")).unwrap();
            let body = &source[start..];
            let end = body[3..].find("fn ").map_or(body.len(), |end| end + 3);
            body[..end].split_whitespace().collect::<String>()
        };
        for name in ["render_ui_to_surface", "render_ui_to_png"] {
            let body = method(name);
            assert!(!body.contains("self.ldr_view"), "{name} must not shrink UI into the scene grid");
            assert!(!body.contains("self.blit("), "{name} must render directly to its output");
            assert_eq!(body.matches("self.ui.render(").count(), 1);
            assert!(body.contains("format,cw,ch)"));
        }
        for name in ["render_driver_menu_frame", "render_menu_to_surface"] {
            let body = method(name);
            let blit = body.find("self.blit(").unwrap();
            let ui = body.find("self.ui.render_over(").unwrap();
            assert!(blit < ui, "{name} must upscale the scene before adding UI");
            assert_eq!(body.matches("self.ui.render_over(").count(), 1);
            assert!(body.contains("view,format,view.texture().width(),view.texture().height()"));
        }
        assert!(method("render_driver_menu_to_surface")
            .contains("self.render_driver_menu_frame(device,queue,view,format,true,elapsed)"));
        assert!(method("render_driver_menu_to_png")
            .contains("self.render_driver_menu_frame(device,queue,&cap_view,format,frame==1,0.0)"));
    }

    #[test]
    fn title_cache_requires_transport_and_progressing_valid_eye_observations() {
        let mut convergence = crate::HeadlessExposureConvergence::default();
        let mut stable = false;
        for frames in 1..=100 {
            let actual = 0.0;
            let current = title_exposure_clock(actual, 0.0);
            if current > actual { stable = convergence.observe(200.0); }
            assert!(!title_backdrop_ready(frames, stable), "frozen clocks cannot certify a backdrop");
        }
        for frames in 1..=64 {
            let actual = 0.0;
            assert!(title_exposure_clock(actual, 1.0 / 60.0) > actual);
            stable = convergence.observe(200.0);
            assert_eq!(title_backdrop_ready(frames, stable), frames == 64);
        }
        assert!(!title_backdrop_ready(480, convergence.observe(f32::NAN)));
        // Invalidation starts a fresh gate, rather than inheriting the prior stable streak.
        convergence = crate::HeadlessExposureConvergence::default();
        for _ in 0..12 { assert!(!convergence.observe(200.0)); }
        assert!(convergence.observe(200.0));
        let source = include_str!("renderer.rs");
        let invalidate = source.split("pub fn invalidate_title_backdrop").nth(1).unwrap()
            .split("pub fn title_backdrop_uninitialized").next().unwrap();
        assert!(invalidate.contains("self.title_backdrop_settled = false"));
        assert!(invalidate.contains("self.title_last_frame = None"));
        assert!(invalidate.contains("self.title_eye_convergence = crate::HeadlessExposureConvergence::default()"));
    }

    #[test]
    fn title_capture_budget_allows_slow_eye_progress_without_relaxing_the_gate() {
        assert_eq!(title_capture_frame_budget(None), 1800);
        for invalid in ["", "invalid", "-1"] {
            assert_eq!(title_capture_frame_budget(Some(invalid)), 1800);
        }
        assert_eq!(title_capture_frame_budget(Some("480")), 1800);
        assert_eq!(title_capture_frame_budget(Some("2400")), 2400);
        assert_eq!(title_capture_frame_budget(Some("99999")), 3600);
        assert_eq!(crate::HEADLESS_EXPOSURE_MAX_FRAMES, 480,
            "the longer title budget must not extend every headless capture");
        let mut drifting = crate::HeadlessExposureConvergence::default();
        let mut exposure = 3600.0;
        for _ in 0..TITLE_EXPOSURE_MAX_FRAMES {
            exposure *= 0.998;
            assert!(!drifting.observe(exposure), "a budget never certifies a changing eye");
        }
        let mut late = crate::HeadlessExposureConvergence::default();
        for _ in 0..976 {
            exposure *= 0.998;
            assert!(!late.observe(exposure));
        }
        for _ in 0..11 { assert!(!late.observe(exposure)); }
        assert!(late.observe(exposure), "the same twelve stable observations remain required");
        assert!(976 + 12 < TITLE_EXPOSURE_MAX_FRAMES);
    }

    #[test]
    fn title_eye_clock_tracks_elapsed_time_without_advancing_scene_time() {
        for fps in [30, 60, 144] {
            let scene = 2.0;
            let mut integrated = 0.0;
            for _ in 0..fps {
                integrated += title_exposure_clock(scene, 1.0 / fps as f32) - scene;
            }
            assert!((integrated - 1.0).abs() < 0.00002);
            assert_eq!(scene, 2.0);
        }
        assert_eq!(title_exposure_clock(0.0, 1.0 / 60.0), 1.0 / 60.0);
        assert_eq!(title_exposure_clock(2.0, f32::NAN), 2.0);
        assert_eq!(title_exposure_clock(2.0, -1.0), 2.0);
        assert_eq!(title_exposure_clock(2.0, 8.0), 3.0, "matches the shader's bounded dt");
    }

    #[test]
    fn gameplay_transport_keeps_diffuse_metallic_reflection_and_both_glass_paths() {
        use crate::graphics::GraphicsPreset;
        assert_eq!(TransportQuality::parse(" GAMEPLAY "), Some(TransportQuality::Gameplay));
        assert_eq!(TransportQuality::parse("full"), Some(TransportQuality::Full));
        for invalid in ["", "off", "no_rt", "0", "fast"] {
            assert_eq!(TransportQuality::parse(invalid), None);
        }
        let full = TransportQuality::Full;
        assert_eq!(full.smoke_steps(), [18.0, 22.0, 26.0, 6.0]);
        assert_eq!(full.glass_world_steps(), 12.0);
        assert_eq!(full.surface_passes(), [2.0, 2.0, 1.0, 0.0]);
        let gameplay = TransportQuality::Gameplay;
        assert_eq!(gameplay.surface_passes(), [2.0, 2.0, 4.0, 0.0]);
        for (lower, reference) in gameplay.smoke_steps().into_iter().zip(full.smoke_steps()) {
            assert!(lower >= 1.0 && lower < reference);
        }
        // Host/shader agreement matters: a metallic surface has no diffuse lobe, so keeping
        // only lane zero makes it black under sky light. Both estimators must execute.
        let surface = include_str!("../shaders/combine.wgsl");
        assert!(surface.contains("array<vec3<f32>, 2>(diffuse_dir, reflection_dir)"));
        assert!(surface.contains("let interface_passes = max(i32(cam.transport_passes.x), 1);"));
        let reference = GpuPhysicalEnvironment::from_parameters(
            &AtmosphereParameters::default(), IntegrationQuality::INTERACTIVE_HIGH);
        let unchanged = GpuPhysicalEnvironment::from_parameters(
            &AtmosphereParameters::default(), full.environment(GraphicsPreset::High));
        assert_eq!(bytemuck::bytes_of(&reference), bytemuck::bytes_of(&unchanged));
        let lower = gameplay.environment(GraphicsPreset::High);
        assert_eq!((lower.view_steps, lower.source_steps, lower.solar_disk_samples,
            lower.cloud_column_steps), (12, 8, 1, 8));
    }

    #[test]
    fn coop_shadow_frame_reuses_one_conservative_vp_then_invalidates() {
        let sun = SUN_DIR;
        let tank = glam::Vec3::new(14.0, 1.2, -37.0);
        let front_fallback = shadow_view_proj_for_focus(
            sun,
            tank + glam::Vec3::Z * SOLO_SHADOW_LOOK_AHEAD_M,
            SOLO_SHADOW_HALF_M,
        );
        let rear_fallback = shadow_view_proj_for_focus(
            sun,
            tank - glam::Vec3::Z * SOLO_SHADOW_LOOK_AHEAD_M,
            SOLO_SHADOW_HALF_M,
        );
        assert_ne!(
            front_fallback.to_cols_array(),
            rear_fallback.to_cols_array(),
            "opposing solo cameras normally need different cascades"
        );

        let mut shared = SharedShadowFrame::default();
        assert!(shared.should_render());
        shared.begin(sun, tank);
        let expected =
            shadow_view_proj_for_focus(sun, tank, SOLO_SHADOW_HALF_M + SOLO_SHADOW_LOOK_AHEAD_M);
        assert_eq!(
            shared.view_proj_or(front_fallback).to_cols_array(),
            expected.to_cols_array()
        );
        assert_eq!(
            shared.view_proj_or(rear_fallback).to_cols_array(),
            expected.to_cols_array(),
            "all crew headings must sample the exact depth-map VP"
        );
        assert!(
            shared.should_render(),
            "the first living view owns the pass"
        );
        assert!(!shared.set_reuse(true));
        assert!(!shared.should_render(), "later crew views skip the pass");

        shared.end();
        assert!(shared.should_render());
        assert_eq!(
            shared.view_proj_or(front_fallback).to_cols_array(),
            front_fallback.to_cols_array(),
            "solo/RTT work after co-op must not inherit the cached matrix"
        );
    }

    #[test]
    #[ignore = "requires GPU, game assets and KESSEL_RENDERER=shader"]
    fn display_preview_gpu_switch_preserves_live_scene() -> anyhow::Result<()> {
        assert!(crate::graphics::shader_backend_enabled());
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut r, mut game) = crate::game::Game::build_act(
            &device, &queue, false, &crate::world::garage())?;
        game.garage_setup();
        game.jump_station(crate::game::Station::Driver);
        game.update(&crate::game::TankInput::default(), 1.0 / 120.0, &mut r);
        let mesh_count = r.meshes.len();
        let batches = r.inst_batches.len();
        let original = (r.iw, r.ih);
        let camera = r.camera;
        r.master_retro = 0.43;
        r.time = 42.0;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        for (id, size) in [(913, (320, 240)), (1591, (480, 272)), (1678, (800, 480)), (0, original)] {
            r.resize_shader_resolution(&device, &queue, size)?;
            set_display_internal_resolution((id != 0).then_some(size));
            assert_eq!((r.iw, r.ih), size);
            assert_eq!(r.meshes.len(), mesh_count);
            assert_eq!(r.inst_batches.len(), batches);
            assert_eq!(r.camera.pos, camera.pos);
            assert_eq!(r.camera.aspect, size.0 as f32 / size.1 as f32);
            assert_eq!(r.master_retro, 0.43);
            assert_eq!(r.time, 42.0);
            game.refresh_rtt_periscopes(&mut r);
            r.ui_reset();
            r.draw_text("NATIVE PIXELS", 0.5, 0.9, 0.04, [1.0; 4], crate::ui::Align::Center);
            r.render_to_png_hud(&device, &queue,
                &format!("captures/astra/display_preview_{id}_20260914.png"), size.0, size.1)?;
        }
        device.poll(wgpu::PollType::wait_indefinitely())?;
        assert!(pollster::block_on(device.pop_error_scope()).is_none());
        Ok(())
    }

    #[test]
    fn internal_resolution_parser_accepts_bounded_game_and_display_presets() {
        assert_eq!((INTERNAL_W, INTERNAL_H), (640, 360));
        for preset in INTERNAL_RES_PRESETS {
            assert_eq!(
                parse_internal_resolution(&format!("{}x{}", preset.0, preset.1)),
                Ok(preset)
            );
        }
        assert_eq!(parse_internal_resolution(" 960 X 540 "), Ok((960, 540)));

        for invalid in [
            "",
            "960",
            "960x",
            "0x0",
            "640x480",
            "1024x576",
            "3840x2160",
            "potato",
        ] {
            assert!(
                parse_internal_resolution(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn shadow_resolution_parser_accepts_bounded_profile_and_capture_sizes() {
        for resolution in [512, 768, 1024, 1536, 2048] {
            assert_eq!(
                parse_shadow_resolution(&resolution.to_string()),
                Ok(resolution)
            );
        }
        for invalid in ["", "0", "256", "1000", "4096", "1024x1024"] {
            assert!(parse_shadow_resolution(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn retro_presets_are_bounded_and_include_a_true_clean_endpoint() {
        assert_eq!(parse_retro_preset("clean"), Ok(0.0));
        assert_eq!(parse_retro_preset(" OFF "), Ok(0.0));
        assert_eq!(parse_retro_preset("minimal"), Ok(0.25));
        assert_eq!(parse_retro_preset("standard"), Ok(DEFAULT_MASTER_RETRO));
        assert_eq!(parse_retro_preset("MAX"), Ok(1.0));
        assert!(parse_retro_preset("cinematic").is_err());
        assert!(parse_retro_preset("1.5").is_err());
    }

    #[test]
    fn optic_masks_derive_aspect_from_the_internal_target() {
        let source = include_str!("../shaders/crush.wgsl");
        assert!(source.contains("let dims = vec2<f32>(textureDimensions(hdr));"));
        assert!(source.contains("dims.x / max(dims.y, 1.0)"));
        assert!(!source.contains("640.0 / 360.0"));
        assert!(source.contains("if (retro > 0.001)"));
        let retro_branch = source.find("if (retro > 0.001)").unwrap();
        assert!(source.find("textureLoad(depth_tex").unwrap() > retro_branch);
        assert!(!source.contains("temporal_grain"));
        assert!(!source[retro_branch..].contains("hash13("));
        assert!(!source.contains("fn noise2"));
        assert!(!source.contains("var grain_amt"));
        assert!(!source.contains("black_point"));
        assert!(!source.contains("S-curve contrast"));
        assert!(source.contains("var m = aces(c)"));
        let ordered_branch = &source[retro_branch..source.find("// TZF 9b/9c").unwrap()];
        assert!(!ordered_branch.contains("cr.frame"));
        assert!(!ordered_branch.contains("grain_gate"));
        // The native threshold is shared by palette and LDR rounding. Verify its complete
        // gated block, not a retired local variable name from the former inline Bayer code.
        let code=source.lines().map(|line|line.split("//").next().unwrap()).collect::<String>()
            .chars().filter(|c|!c.is_whitespace()).collect::<String>();
        let block=|marker:&str| {
            let start=code.find(marker).unwrap_or_else(||panic!("missing {marker}"));
            let open=start+code[start..].find('{').unwrap();let mut depth=0;
            for (offset,byte) in code.as_bytes()[open..].iter().enumerate() {
                if *byte==b'{' {depth+=1;}
                if *byte==b'}' {depth-=1;if depth==0 {return &code[open+1..open+offset];}}
            }
            panic!("unclosed {marker}");
        };
        let enabled=block("if(cr.retro>0.001||cr.ldr_dither>0.0)");
        let threshold=block("fndisplay_threshold(");
        let palette=block("if(retro>0.001)");let ldr=block("if(cr.ldr_dither>0.0)");
        assert_eq!(code.matches("display_threshold(").count(),2,"one definition and one gated call");
        assert!(code.contains("varnative_threshold=0.0;"));
        assert!(enabled.contains("native_threshold=display_threshold(vec2<i32>(in.pos.xy));"));
        assert!(palette.contains("letordered=native_threshold;"));
        assert!(ldr.contains("letthreshold=native_threshold;"));
        assert!(threshold.contains("(px.y&3)*4+(px.x&3)"));
        assert!(threshold.contains("return(BAYER["));
        assert!(threshold.contains("textureLoad(blue_noise,vec2(px.x&63,px.y&63),0)"));
        for branch in [enabled,threshold,palette,ldr,block("fnsphere_threshold(")] {assert!(!branch.contains("cr.frame"));}
        // Read the production Bayer table: each of its16 fixed pixel phases must occur once,
        // with centred thresholds and neither an omitted rank nor a persistent brightness bias.
        let table=code.split("var<private>BAYER:").nth(1).unwrap().split("(").nth(1).unwrap()
            .split(");").next().unwrap();
        let mut ranks=table.split(',').filter(|s|!s.is_empty()).map(|s|s.parse::<f32>().unwrap()).collect::<Vec<_>>();
        assert_eq!(ranks.len(),16);ranks.sort_by(f32::total_cmp);
        assert_eq!(ranks,(0..16).map(|i|i as f32).collect::<Vec<_>>());
        assert_eq!(ranks.iter().map(|r|(r+0.5)/16.0-0.5).sum::<f32>(),0.0);
        assert!(source.contains("let concussion = clamp(cr.concussion"));
        assert!(source.contains("mix(centre, blur, concussion)"));
        assert_eq!(std::mem::size_of::<CrushUniform>(), 128);
        assert_eq!(std::mem::offset_of!(CrushUniform, exposure), 44);
        assert_eq!(std::mem::offset_of!(CrushUniform, concussion), 52);
        assert_eq!(std::mem::offset_of!(CrushUniform, display_grade), 64);
        assert_eq!(std::mem::offset_of!(CrushUniform, dither_right), 80);
        assert_eq!(std::mem::offset_of!(CrushUniform, dither_up), 96);
        assert_eq!(std::mem::offset_of!(CrushUniform, dither_forward), 112);
    }

    #[test]
    fn volumetric_uniform_v2_contains_only_modeled_medium_and_shared_sources() {
        assert_eq!(std::mem::offset_of!(SmokeUniform, breech_gas), 672);
        assert_eq!(std::mem::offset_of!(SmokeUniform, fog_density), 76);
        assert_eq!(std::mem::offset_of!(SmokeUniform, n_lights), 720);
        assert_eq!(std::mem::offset_of!(SmokeUniform, lights), 736);
        assert_eq!(std::mem::offset_of!(SmokeUniform, interior), 1_760);
        assert_eq!(
            std::mem::offset_of!(SmokeUniform, world_snow_fraction),
            1_768
        );
        assert_eq!(std::mem::offset_of!(SmokeUniform, atmo_origin), 1_776);
        assert_eq!(std::mem::offset_of!(SmokeUniform, fog_layer), 1_792);
        assert_eq!(std::mem::offset_of!(SmokeUniform, transport_steps), 1_808);
        assert_eq!(std::mem::size_of::<SmokeUniform>(), 1_840 + std::mem::size_of::<cabin_medium::Uniform>());

        // Read the actual composed WGSL layout rather than trusting matching Rust constants.
        let module = wgpu::naga::front::wgsl::parse_str(&smoke_ray_query_source()).unwrap();
        let (_, smoke_type) = module.types.iter()
            .find(|(_, ty)| ty.name.as_deref() == Some("Smoke")).unwrap();
        let wgpu::naga::TypeInner::Struct { members, span } = &smoke_type.inner else {
            panic!("Smoke must remain a uniform struct");
        };
        assert_eq!(*span as usize, std::mem::size_of::<SmokeUniform>());
        let step_member = members.iter()
            .find(|member| member.name.as_deref() == Some("transport_steps")).unwrap();
        assert_eq!(step_member.offset as usize, std::mem::offset_of!(SmokeUniform, transport_steps));

        let source = SMOKE_SHADER_SOURCE;
        assert!(source.contains("breech_gas_density"));
        assert!(source.contains("fog_density: f32"));
        assert!(source.contains("world_snow_fraction: f32"));
        assert!(source.contains("return world_medium_density(p) + local_medium_density(p)"));
        for retired in [
            "n_glows: f32",
            "glows: array",
            "n_aps: f32",
            "aps: array",
            "obscuration: f32",
        ] {
            assert!(
                !source.contains(retired),
                "retired volumetric lane {retired:?}"
            );
        }
        assert!(!source.contains("veil_a"));
        assert!(!source.contains("glow_lit"));
        assert!(!source.contains("unlit_air"));
        assert!(!source.contains("var warm"));
    }

    #[test]
    fn refracted_rays_and_fullscreen_volume_share_the_world_medium_abi() {
        assert_eq!(std::mem::offset_of!(CameraUniform, atmosphere), 1_280);
        assert_eq!(std::mem::offset_of!(CameraUniform, world_medium), 1_296);
        assert_eq!(std::mem::offset_of!(CameraUniform, transport_passes), 1_312);
        assert_eq!(std::mem::size_of::<CameraUniform>(), 1_328 + std::mem::size_of::<cabin_medium::Uniform>()+16);

        let physical = PHYSICAL_ENVIRONMENT_SHADER_SOURCE;
        let smoke = SMOKE_SHADER_SOURCE;
        let combine = include_str!("../shaders/combine.wgsl");
        assert!(physical.contains("fn physical_bounded_medium_interval("));
        assert!(physical.contains("fn physical_bounded_medium_transmittance("));
        assert!(smoke.contains("physical_bounded_medium_interval("));
        assert!(smoke.contains("cabin_path_length("));
        assert!(combine.contains("world_medium: vec4<f32>"));
        assert!(combine.contains("transport_passes: vec4<f32>"));
        assert!(combine.contains("let interface_passes = max(i32(cam.transport_passes.x), 1);"));
        assert!(combine.contains("let bounce_passes = max(i32(cam.transport_passes.y), 1);"));
        assert!(combine.contains("fn world_medium_radiance("));
        assert!(combine.contains("background * throughput + inscattered"));
        let empty_pack = crate::rt_surface::SurfacePackBuilder::new(4).unwrap().finish().unwrap();
        let upload = pack_rt_surface_static(&empty_pack);
        assert_eq!(upload.records[19], 12);
        assert!(protective_glass::SHADER.contains("let optical_steps = max(source_records[19], 1u);"));
        // One post-interface call site must serve direct glass and folded periscope rays. Counting
        // definition plus calls catches the accidental duplicate of the entire atmosphere graph.
        assert_eq!(combine.matches("non_glass_incident(").count(), 2);
        assert_eq!(combine.matches("glass_incident(").count()
            - combine.matches("non_glass_incident(").count(), 3);
    }

    #[test]
    fn act_i_whiteout_is_bounded_snow_transport_not_a_visibility_or_exposure_lift() {
        let act_i = world_medium_profile(1.0, 0.98);
        assert!((act_i.snow_fraction - 0.98).abs() < 1e-6);
        assert_eq!(act_i.layer[0], WORLD_AEROSOL_BOTTOM_M);
        assert_eq!(act_i.layer[1], BLOWING_SNOW_TOP_M);
        assert!(
            act_i.layer[1] > 84.0,
            "the declared layer must contain the highest current Act-I terrain"
        );

        let ordinary = world_medium_profile(0.0, 0.98);
        assert_eq!(ordinary.snow_fraction, 0.0);
        assert_eq!(ordinary.layer[1], WORLD_AEROSOL_TOP_M);

        // Horizontal combat visibility stays exactly T=exp(-sigma_t*d). Only the vertical support
        // changes, so a 205 m encounter remains at the authored ~25% transmission.
        let sigma_t = 0.0068_f32;
        let range_m = 205.0_f32;
        let horizontal_before = (-sigma_t * range_m).exp();
        let horizontal_after = (-sigma_t * range_m).exp();
        assert!((horizontal_before - horizontal_after).abs() < f32::EPSILON);
        assert!((horizontal_after - 0.248).abs() < 0.002);

        // At representative terrain height, the real daytime solar ray crosses much less dense
        // snow. This raises source transmittance in the volume equation; it does not add radiance.
        let terrain_y = 21.0_f32;
        // Act I's noon Sun from the ephemeris (17.85 deg at the solstice).
        let noon = crate::celestial_sky::CelestialLighting::at(
            crate::celestial_sky::phase_instant(1, crate::celestial_sky::ClockPhase::Day),
            crate::ephemeris::STALINGRAD, AtmosphereParameters::default().solar_irradiance);
        let sun_y = noon.key.direction.y as f32;
        let old_tau = sigma_t * (WORLD_AEROSOL_TOP_M - terrain_y) / sun_y;
        let new_tau = sigma_t * (act_i.layer[1] - terrain_y) / sun_y;
        let old_downwelling = (-old_tau).exp();
        let new_downwelling = (-new_tau).exp();
        assert!(new_downwelling > old_downwelling * 4.0);
        assert!((0.0..=1.0).contains(&new_downwelling));

        let source = SMOKE_SHADER_SOURCE;
        for required in [
            "const BLOWING_SNOW_PHASE_G: f32 = 0.76",
            "const BLOWING_SNOW_SINGLE_SCATTER_ALBEDO: f32 = 0.995",
            "let world_phase_g = mix(AEROSOL_PHASE_G, BLOWING_SNOW_PHASE_G",
            "physical_environment_transport(",
            "world_single_scatter_albedo * incident",
        ] {
            assert!(
                source.contains(required),
                "missing snow transport contract {required:?}"
            );
        }
        for forbidden in [
            "snow_tint",
            "whiteout_color",
            "ambient_floor",
            "flat_white_sky",
        ] {
            assert!(
                !source.contains(forbidden),
                "forbidden whiteout shortcut {forbidden:?}"
            );
        }

        // High's surface paths receive the same finite slab on their source side. Before this
        // contract landed, only camera/volume rays crossed the suspension: white snow was exposed
        // as if under a clear Sun while the physically blue sky was extinguished toward navy.
        let combine = include_str!("../shaders/combine.wgsl");
        // The Sun's source-side slab transport arrives through the two-stream closure of
        // world_medium_solar_source at both surface shading sites (primary fs + shade_secondary);
        // positioned emitters keep the directional world_medium_source_transmittance term.
        assert!(
            combine
                .matches("let solar_source = world_medium_solar_source(solar_path.origin, sun_dir);")
                .count()
                >= 2
        );
        // Both sky samples cross the slab through the same bounded environment-source closure
        // (directional Beer term plus the scattered remainder), never a bare environment lookup.
        let compact_combine: String = combine.chars().filter(|ch| !ch.is_whitespace()).collect();
        assert!(compact_combine.contains(
            "letsky=world_medium_environment_source(sky_path.origin,sky_dir,"
        ));
        assert!(combine.contains("var diff_col = direct_diff;"));
        assert!(combine.contains("let sky = world_medium_environment_source("));

        // Explicit one-pixel flakes keep world positions and exact upstream roof exclusion;
        // broad weather obscuration remains the bounded bulk transport above.
        let snow = include_str!("../shaders/pixel_particles.wgsl");
        for forbidden in ["whiteout_veil", "ground_spindrift", "flake_layer"] {
            assert!(!snow.contains(forbidden));
        }
        assert!(snow.contains("blocked(in.world,upstream,distance_m)"));
        assert!(snow.contains("u.view_proj * vec4(world - u.camera.xyz, 1.0)"));
    }

    #[test]
    fn high_volumetric_transport_traces_real_sources_and_medium_extinction() {
        let portable = SMOKE_SHADER_SOURCE;
        assert!(!portable.contains("var smoke_scene_tlas: acceleration_structure"));
        assert!(portable.contains(SMOKE_NO_DIRECT_MARKER));

        let high = smoke_ray_query_source();
        assert!(high.contains("@group(1) @binding(0) var smoke_scene_tlas: acceleration_structure"));
        assert!(high.contains("rayQueryInitialize(&rq, smoke_scene_tlas"));
        assert!(high.contains("RAY_FLAG_TERMINATE_ON_FIRST_HIT | RAY_FLAG_FORCE_OPAQUE"));
        assert!(!high.contains(SMOKE_NO_DIRECT_MARKER));

        // The sun and unbiased samples from the shared PointLight table require both triangle
        // visibility and Beer-Lambert transmittance before contributing radiance.
        assert!(high.contains("let path = daylight_connection(p, to_sun, 4000.0)"));
        assert!(high.contains("out.throughput = path.throughput"));
        assert!(high.contains(
            "let local_transmittance = daylight_medium_transmittance(p, to_sun, 4000.0, path)"
        ));
        assert!(high.contains("let nl = i32(sm.n_lights)"));
        assert!(high.contains("max(distance_to_light - source_radius, 0.005)"));
        assert!(high.contains("spherical_emitter_attenuation(distance_squared, source_radius)"));
        assert!(high
            .contains("let transmittance = medium_transmittance(p, light_dir, distance_to_light)"));
        assert!(high.contains("return exp(-optical_depth)"));
        assert!(high.contains("henyey_greenstein"));
    }

    #[test]
    fn relative_history_uniform_matches_both_gpu_consumers() {
        assert_eq!(std::mem::size_of::<AccumUniform>(), 256);
        for source in [include_str!("../shaders/accum.wgsl"),
                       include_str!("../shaders/volume_accum.wgsl")] {
            let module = wgpu::naga::front::wgsl::parse_str(source).unwrap();
            let mut found = false;
            for (_, ty) in module.types.iter() {
                if ty.name.as_deref() == Some("AccumUniform") {
                    if let wgpu::naga::TypeInner::Struct { ref members, span } = ty.inner {
                        assert_eq!(span as usize, std::mem::size_of::<AccumUniform>());
                        let shift = members.iter().find(|m| m.name.as_deref() == Some("camera_shift")).unwrap();
                        assert_eq!(shift.offset as usize, std::mem::offset_of!(AccumUniform, camera_shift));
                        found = true;
                    }
                }
            }
            assert!(found);
        }
    }

    #[test]
    fn campaign_terrain_material_compiles_in_forward_and_deferred_entries() {
        for (label, entry) in [
            ("forward terrain", include_str!("../shaders/mesh.wgsl")),
            ("G-buffer terrain", include_str!("../shaders/gbuf.wgsl")),
        ] {
            let source = terrain_material_shader_source(entry);
            assert_eq!(source.matches("fn terrain_material(").count(), 1);
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{label} WGSL parse failed: {error}"));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{label} WGSL validation failed: {error}"));
        }
        // High is fully parsed/validated by live_physical_environment_entries_parse_and_validate_together.
        // Verify that its actual composer also inserts this exact library once, not a copied palette.
        let high = physical_atmosphere_transport_shader_source(include_str!("../shaders/combine.wgsl"));
        assert_eq!(high.matches("fn terrain_material(").count(), 1);
        assert!(high.contains(include_str!("../shaders/terrain_material.wgsl")));
    }

    #[test]
    fn live_physical_environment_entries_parse_and_validate_together() {
        let entries = [
            (
                "visible sky",
                physical_atmosphere_transport_shader_source(include_str!("../shaders/sky.wgsl")),
            ),
            (
                "High surface transport",
                physical_atmosphere_transport_shader_source(include_str!(
                    "../shaders/combine.wgsl",
                )),
            ),
            ("High volume transport", smoke_ray_query_source()),
            (
                "portable volume transport",
                physical_atmosphere_transport_shader_source(SMOKE_SHADER_SOURCE),
            ),
            (
                "High resolved-snow transport",
                physical_snow_ray_query_source(),
            ),
            (
                "portable resolved-snow module",
                physical_transport_shader_source(include_str!("../shaders/snow.wgsl")),
            ),
        ];
        for (label, source) in entries {
            assert_eq!(
                source.matches("fn physical_environment_transport(").count(),
                1
            );
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{label} WGSL parse failed: {error}"));
            let mut validator = wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            );
            validator
                .validate(&module)
                .unwrap_or_else(|error| panic!("{label} WGSL validation failed: {error}"));
        }
    }

    #[test]
    fn exact_rt_arrays_are_semantically_split_below_hal_binding_limit() {
        const HAL_VERTICES: usize = 33_718_039;
        const VERTEX_BYTES: usize = std::mem::size_of::<crate::rt_surface::GpuSurfaceVertex>();
        const HAL_BINDING_LIMIT: u64 = 2_147_483_647;

        let unsplit = HAL_VERTICES * VERTEX_BYTES;
        assert!(unsplit as u64 > HAL_BINDING_LIMIT);
        let (a, b) = rt_vertex_partition_counts(HAL_VERTICES);
        assert_eq!(a, 16_859_020);
        assert_eq!(b, 16_859_019);
        assert_eq!(a * VERTEX_BYTES, 1_078_977_280);
        assert_eq!(b * VERTEX_BYTES, 1_078_977_216);
        validate_rt_storage_binding_sizes(
            HAL_BINDING_LIMIT,
            &[
                ("vertices-a", a * VERTEX_BYTES),
                ("vertices-b", b * VERTEX_BYTES),
                ("records", 495_500_000),
                ("atlas", 460_000_000),
                ("instances", 15_702 * 32),
            ],
        )
        .unwrap();
        assert!(validate_rt_storage_binding_sizes(
            HAL_BINDING_LIMIT,
            &[("forbidden monolith", 2_653_726_000)],
        )
        .unwrap_err()
        .contains("exceeding"));

        let high = include_str!("../shaders/combine.wgsl");
        for required in [
            "@group(2) @binding(1) var<storage, read> rt_vertices_a",
            "@group(2) @binding(2) var<storage, read> rt_vertices_b",
            "@group(2) @binding(3) var<storage, read> rt_records",
            "@group(2) @binding(4) var<storage, read> rt_atlas",
            "@group(2) @binding(5) var<storage, read> rt_instances",
            "let split = rt_records[3]",
            "rt_vertices_b[(index - split) * 16u + lane]",
            "RT_RECORDS_VERSION = 2u",
        ] {
            assert!(high.contains(required), "missing split ABI: {required}");
        }
        assert!(!high.contains("rt_meta"));
    }

    #[test]
    fn interior_has_no_analytic_aperture_or_view_state_energy_path() {
        let forward = include_str!("../shaders/mesh.wgsl");
        let deferred = include_str!("../shaders/combine.wgsl");
        let smoke = SMOKE_SHADER_SOURCE;
        for (name, source) in [("forward", forward), ("deferred", deferred)] {
            for forbidden in [
                "crew_floor",
                "cabin_bounce",
                "interior_kill",
                "warm * albedo",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{name} shader retained forbidden interior energy path {forbidden:?}"
                );
            }
        }

        // The removal must not bypass the existing physical visibility paths.
        assert!(forward.contains("let shadow = sun_shadow(in.world, n, NdotL)"));
        assert!(deferred.contains("@group(2) @binding(0) var tlas: acceleration_structure"));
        assert!(deferred.contains("rayQueryInitialize(&rq, tlas"));
        assert!(deferred.contains("solar_path = source_connection(ro, sun_dir, ray_skip, 4000.0)"));
        let glass = protective_glass::SHADER;
        assert!(glass.contains("if (entry.kind != 2u) { return out; }"));
        assert!(glass.contains("if (blocker.kind != 0u) { return out; }"));
        assert!(glass.contains("exp(-SOURCE_GLASS_ABSORPTION_PER_M * glass_path)"));
        assert!(deferred.contains("let shadow = solar_path.throughput;"));
        assert!(deferred.contains("let sky_path = source_connection(ro, sky_dir, 0.01, 2000.0)"));
        assert!(deferred.contains("let li = secondary_incident(bounce_origin, bounce_direction[slot], bounce_seed[slot]);"));
        assert!(!deferred.contains("var sky_irr"));
        for retired in [
            "n_glows: f32",
            "glows: array",
            "n_aps: f32",
            "aps: array",
            "obscuration: f32",
        ] {
            assert!(!smoke.contains(retired));
        }
    }

    #[test]
    fn weather_push_stays_inside_cross_platform_budget() {
        assert_eq!(std::mem::size_of::<SnowPush>(), 112);
        assert_eq!(std::mem::align_of::<SnowPush>(), 4);
        assert!(std::mem::size_of::<SnowPush>() <= 128);
    }

    #[test]
    fn regular_actor_ids_keep_exact_rt_lookup_and_do_not_alias_instance_or_static_ids() {
        let dynamic = regular_object_custom_data(722, true);
        assert_eq!(dynamic, 0x80_02D2);
        assert_eq!(dynamic as f32 as u32, dynamic);
        assert_ne!(dynamic, regular_object_custom_data(722, false));
        assert_ne!(dynamic, instanced_object_custom_data(722, 0, true));
        assert_eq!(dynamic & 0x3f_ffff, 722);
        assert_eq!(dynamic & 0x40_0000, 0, "regular actor must not be decoded as an instance");
        let mesh = crate::prims::box_mesh(glam::Vec3::ZERO, glam::Vec3::ONE, [72, 64, 48]);
        let mut builder = crate::rt_surface::SurfacePackBuilder::default();
        let surface = builder.add_mesh(&mesh).unwrap();
        for key in [dynamic, regular_object_custom_data(7, false),
            instanced_object_custom_data(722, 0, true)] {
            builder.add_instance(key, surface, glam::Mat4::IDENTITY).unwrap();
        }
        let pack = builder.finish().unwrap();
        let states = pack.default_instance_material_states();
        pack.validate_instance_material_states(&states).unwrap();
        assert_eq!(states[0].ids[0], dynamic);
        assert!(pack.evaluate_hit(dynamic, 0, [0.25, 0.25], Default::default()).is_ok());
        assert!(pack.evaluate_hit(722, 0, [0.25, 0.25], Default::default()).is_err(),
            "a stale untagged source key must not silently address the actor material");
    }

    #[test]
    fn rapidly_moving_instance_ids_reject_temporal_history() {
        let source = include_str!("../shaders/accum.wgsl");
        assert!(source.contains("(id & 0x800000u) != 0u"));
        let bypass = source.find("if (d >= 1.0 || u.alpha >= 0.999 || dynamic_instance)")
            .expect("dynamic geometry bypasses source history");
        let end = source[bypass..].find('}').unwrap() + bypass;
        let branch = &source[bypass..end];
        assert!(branch.contains("textureStore(out_tex, px, vec4(cur, 1.0))"));
        assert!(branch.contains("return;"));
        assert!(end < source.find("textureLoad(hist_tex").unwrap());
    }

    #[test]
    fn cubic_smoke_instances_are_never_screen_door_deleted() {
        let forward = include_str!("../shaders/mesh.wgsl");
        let deferred = include_str!("../shaders/gbuf.wgsl");
        assert_eq!(BATCH_FLAG_SPARSE_CUBIC_SMOKE.to_bits(), (-1.0f32).to_bits());
        for source in [forward, deferred] {
            assert!(source.contains("let sparse_cubic_smoke = pc.params.w < -0.5;"));
            assert!(!source.contains("sparse_cubic_smoke_keeps"));
            // Grass alpha and physical near-eye clipping are permitted. Neither is a random or
            // screen-pattern coverage deletion; soot outside the clipping margin stays solid.
            for (index, _) in source.match_indices("discard;") {
                let preceding = &source[index.saturating_sub(160)..index];
                assert!(
                    preceding.contains("pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5")
                        || preceding.contains("sparse_cubic_smoke && distance(in.world, cam.cam_pos) < 0.16"),
                    "discard outside grass alpha or physical near-eye clipping"
                );
            }
            // Even an accidentally non-zero batch emissive must not divert cubic smoke away from
            // the ordinary lit-material path after its deterministic discard has run.
            assert_eq!(
                source
                    .matches("if (!sparse_cubic_smoke && pc.params.x > 0.0")
                    .count(),
                3
            );
        }
    }

    #[test]
    fn seeded_gust_is_deterministic_bounded_and_act_specific() {
        for seed in [0, 1, 0x5751_1917, u32::MAX] {
            for time in [0.0, 1.0 / 60.0, 37.0, 3600.0] {
                let a = weather_gust(time, seed);
                assert_eq!(a.to_bits(), weather_gust(time, seed).to_bits());
                assert!((0.60..=1.12).contains(&a));
            }
        }
        assert_ne!(
            weather_seed_token(0x5751_1917),
            weather_seed_token(0x0a17_6b22)
        );
        assert_ne!(
            weather_gust(37.0, 0x5751_1917).to_bits(),
            weather_gust(37.0, 0x0a17_6b22).to_bits()
        );
    }

    #[test]
    fn weather_shader_uses_depth_world_time_and_integer_cells() {
        let entry = include_str!("../shaders/pixel_particles.wgsl");
        let field = include_str!("pixel_particles.rs");
        assert!(entry.contains("texture_depth_2d"));
        assert!(entry.contains("u.view_proj * vec4(world - u.camera.xyz, 1.0)"));
        assert!(entry.contains("blocked(in.world,upstream,distance_m)"));
        assert!(field.contains("fn snow_identity("));
        assert!(field.contains("fn advance(&mut self, time_s: f32, wind: Vec3)"));
        assert!(!entry.contains("ground_spindrift"));
        assert!(!entry.contains("self.snow_pipe"));
    }

    #[test]
    fn high_path_rejects_screen_space_gi_and_traces_every_local_source() {
        let high = include_str!("../shaders/combine.wgsl");
        assert!(!high.contains("fn screen_space_contact_ao"));
        assert!(!high.contains("screen_space_diffuse_bounce"));
        assert!(!high.contains("indirect_bounce"));
        assert!(!high.contains("floor_day"));
        assert!(high.contains("var diff_col = direct_diff"));
        assert!(high.contains("occluded(ro, ldir, ray_skip"));
        assert!(high.contains("spherical_emitter_attenuation(ld2, source_radius)"));
        assert!(!high.contains("distance_to_light >= radius"));
        assert!(!high.contains("let falloff"));

        let smoke = include_str!("../shaders/smoke.wgsl");
        assert!(smoke.contains("spherical_emitter_attenuation(distance_squared, source_radius)"));
        assert!(!smoke.contains("distance_to_light >= radius"));
        assert!(!smoke.contains("let falloff"));

        // Compatibility knobs remain inert while callers migrate; no environment can re-enable the
        // deleted heightfield/OBB compute pass.
        let renderer = include_str!("renderer.rs");
        assert!(renderer.contains("portable_ao_enabled: false"));
        // Assemble the literal at runtime: this test module is inside the scanned source, so spelling
        // the forbidden token here would make the assertion satisfy itself.
        let retired_ao_env = ["NO_PORTABLE", "AO_ENV"].join("_");
        assert!(!renderer.contains(&retired_ao_env));
    }

    #[test]
    fn exact_secondary_transport_reconstructs_committed_materials_without_proxies() {
        let high = include_str!("../shaders/combine.wgsl");
        for required in [
            "hit.instance_custom_data",
            "hit.primitive_index",
            "hit.barycentrics",
            "hit.object_to_world",
            "hit.world_to_object",
            "rt_sample_texture(base_tex, uv, 0.0)",
            "rt_sample_texture(mr_tex, uv, 0.0)",
            "rt_sample_texture(normal_tex, uv, 0.0)",
            "determinant(object_linear) >= 0.0",
            // Cosine diffuse plus the current visible-normal GGX path feed one runtime-bounded
            // secondary call site so the Vulkan compiler cannot duplicate its nested transport.
            "fn sample_ggx_vndf(",
            "var bounce_weight = array<vec3<f32>, 2>(diffuse_kd * albedo * diffuse_pdf_weight, reflection_weight);",
            "let li = secondary_incident(bounce_origin, bounce_direction[slot], bounce_seed[slot]);",
            "let bounce_passes = max(i32(cam.transport_passes.y), 1);",
            "return authored * strength",
        ] {
            assert!(
                high.contains(required),
                "missing exact-hit contract {required:?}"
            );
        }
        for forbidden in [
            "screen_space_diffuse_bounce",
            "screen_space_contact_ao",
            "heightfield",
            "RtBox",
            "average_material",
        ] {
            assert!(
                !high.contains(forbidden),
                "proxy transport returned: {forbidden:?}"
            );
        }
    }

    #[test]
    fn retired_proxy_pipelines_and_screen_overlays_cannot_ship() {
        let renderer = include_str!("renderer.rs");
        let shipping = renderer
            .split("#[cfg(test)]\nmod atmosphere_tests")
            .next()
            .unwrap();
        for forbidden in [
            "include_str!(\"../shaders/pathtrace.wgsl\")",
            "outline_pipe",
            "self.glass_pipe",
            "pt_out_view",
            "PTUniform",
            "set_interior_glows",
        ] {
            assert!(
                !shipping.contains(forbidden),
                "retired renderer path {forbidden:?}"
            );
        }
        assert!(!shipping.contains("rtt_tex"));
        assert!(!shipping.contains("rtt_bind"));
        assert!(!shipping.contains("label: Some(\"rtt-atlas\")"));

        let crush = include_str!("../shaders/crush.wgsl");
        for forbidden in ["cr.crew_ko", "cr.ko_dir", "rt_tex", "cr.haze", "cr.rt_mix"] {
            assert!(
                !crush.contains(forbidden),
                "screen overlay survived: {forbidden:?}"
            );
        }
        let resolve = include_str!("../shaders/resolve.wgsl");
        for forbidden in [
            "atmosphere_air",
            "weather_desat",
            "mix(col, air",
            "let fog =",
        ] {
            assert!(
                !resolve.contains(forbidden),
                "fake resolve atmosphere {forbidden:?}"
            );
        }
    }

    #[test]
    fn profile_wind_is_world_fixed_and_ordered() {
        let calm = profile_weather_wind(1, 0.0);
        let storm = profile_weather_wind(1, 1.0);
        assert!(calm.is_finite() && storm.is_finite());
        assert!(storm.length() > calm.length());
        assert!((calm.normalize().dot(storm.normalize()) - 1.0).abs() < 1e-6);

        let oderbruch = profile_weather_wind(2, 1.0);
        let halbe = profile_weather_wind(3, 0.0);
        assert!(oderbruch.length() > halbe.length() * 5.0);
        assert!(oderbruch.normalize().dot(halbe.normalize()) < 0.95);
    }

    #[test]
    fn winter_ground_stays_snow_across_weather_profiles_and_fronts() {
        // Exercise every authored profile, including the actual faulty arena value0.35, and
        // every interpolation into a different act. Only a new map changes material season.
        for act in 0..=3 {
            for next in 0..=3 {
                for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let p = campaign_atmosphere_profile(act).toward(campaign_atmosphere_profile(next), t);
                    assert_eq!(campaign_ground_mood(true, p.mood), 0.0);
                    assert!(campaign_ground_mood(false, p.mood) >= 0.5);
                }
            }
        }
        assert_eq!(campaign_ground_mood(false, 0.5), 0.5);
        assert_eq!(campaign_ground_mood(false, 1.0), 1.0);
    }

    #[test]
    fn campaign_atmospheres_are_distinct_physical_media() {
        let winter = campaign_atmosphere_profile(1);
        let clearing = campaign_atmosphere_profile(2);
        let sunny = campaign_atmosphere_profile(3);
        assert!(winter.overcast > 0.95 && winter.storm > 0.9);
        assert!(winter.overcast > clearing.overcast && clearing.overcast > sunny.overcast);
        assert!(sunny.overcast <= 0.15 && sunny.storm == 0.0);
        assert!(winter.ground_extinction_m_inv > clearing.ground_extinction_m_inv);
        assert!(clearing.ground_extinction_m_inv > sunny.ground_extinction_m_inv);
        assert!(sunny.ground_extinction_m_inv > 0.0, "clear air retains aerial perspective");
        assert!(sunny.mie_extinction_m_inv > sunny.mie_scattering_m_inv);
        assert!(clearing.cloud_bottom_m < sunny.cloud_bottom_m);
        assert!(clearing.cloud_top_m < sunny.cloud_top_m);
        assert!(clearing.wind_exposure > sunny.wind_exposure);
    }

    #[test]
    fn named_phases_carry_no_light_and_the_night_has_no_floor() {
        // Every phase is an ephemeris instant; the key body's direction and the whole irradiance
        // follow from it, and the physical upload scales no source by a night factor.
        for act in [1u8, 2, 3] {
            for phase in DayPhase::ALL {
                let lighting = crate::celestial_sky::CelestialLighting::at(
                    crate::celestial_sky::phase_instant(act, phase.clock_phase()),
                    crate::celestial_sky::act_site(act), AtmosphereParameters::default().solar_irradiance);
                assert!((lighting.key.direction.length() - 1.0).abs() < 1e-9);
                assert!(lighting.key.irradiance.is_finite());
                if phase == DayPhase::Day { assert_eq!(lighting.darkness(), 0.0); }
            }
        }
        let renderer = include_str!("renderer.rs");
        let retired_floor = ["solar_irradiance *= 1.0", " - self.night"].concat();
        assert!(!renderer.contains(&retired_floor), "the 20 % night floor must not return");
        let retired_fog = ["if phase == DayPhase::Night", " { 1.10 }"].concat();
        assert!(!renderer.contains(&retired_fog), "night is not a fog multiplier");
        assert!(renderer.contains("self.celestial.apply_to(&mut physical_environment);"));
    }

    #[test]
    fn headless_exposure_clock_advances_at_zero_without_overwriting_eye_state() {
        for actual in [0.0_f32, 1.0 / 60.0, 0.1, 123.5] {
            let [current, previous, restored] = headless_exposure_step_clocks(actual);
            assert!(previous >= 0.0, "must not repeat the initialization sentinel");
            assert!(((current - previous) - 1.0 / 60.0).abs() < 4.0e-6);
            assert_eq!(restored, actual);
            let original: [f32; EYE_STATE_LANES] = std::array::from_fn(|i| i as f32 + 0.25);
            let mut bytes = bytemuck::bytes_of(&original).to_vec();
            // The actual API writes only four bytes at state[1]. Check both operations against
            // a populated eye, so exposure, pigment, neural and flash history cannot be reset.
            for clock in [previous, restored] {
                bytes[4..8].copy_from_slice(bytemuck::bytes_of(&clock));
                assert_eq!(&bytes[..4], &bytemuck::bytes_of(&original)[..4]);
                assert_eq!(&bytes[8..], &bytemuck::bytes_of(&original)[8..]);
            }
        }
        let [current, previous, restored] = headless_exposure_step_clocks(f32::NAN);
        assert_eq!([current, previous, restored], [1.0 / 60.0, 0.0, 0.0]);
    }

    #[test]
    fn frozen_scene_headless_clock_advances_real_observer_adaptation() {
        use crate::observer::{Eye, Meter, Viewing};
        let viewing = Viewing::Binocular;
        let initial = Eye::adapted(Meter::uniform(0.05, 0.12), viewing);
        let meter = Meter::uniform(500.0, 1175.0);
        let mut stepped = initial;
        let mut frozen = initial;
        for _ in 0..360 {
            let [current, previous, restored] = headless_exposure_step_clocks(0.0);
            stepped.step(meter, viewing, (current - previous) as f64);
            frozen.step(meter, viewing, 0.0);
            assert_eq!(restored, 0.0, "the scene instant stays frozen");
        }
        assert_eq!(frozen, initial);
        assert!(stepped.mapping(viewing).cone_exposure < initial.mapping(viewing).cone_exposure * 0.1);
    }

    #[test]
    fn the_eye_is_the_display_and_never_touches_transport() {
        // The crewman's eye replaces the fixed exposure in gameplay, on both backends and presets.
        let constructor = include_str!("renderer.rs").split("let transport_quality = TransportQuality::configured();")
            .nth(1).unwrap().split("pub fn apply_").next().unwrap();
        assert!(constructor.contains("autoexp: true,"), "gameplay must run the eye");
        assert!(UNINITIALIZED_EXPOSURE_TIME.is_sign_negative());
        let state = uninitialized_exposure_state();
        assert_eq!((state[0], state[1]), (DEFAULT_CAMERA_EXPOSURE, UNINITIALIZED_EXPOSURE_TIME));
        assert_eq!(state.len() * 4, EYE_STATE_LANES * 4);
        let renderer = include_str!("renderer.rs");
        // Assemble retired spellings at runtime: this test module is part of the scanned file.
        let retired_fill = ["authored", "forward", "exposure"].join("_");
        assert!(!renderer.contains(&retired_fill));
        assert!(renderer.contains("if finish_ldr && self.autoexp && self.exposure_override.is_none()"));
        let deferred_only_gate = ["if finish_ldr && self.deferred", "&& self.autoexp"].join(" ");
        assert!(!renderer.contains(&deferred_only_gate));
        assert!(renderer.contains("reset_active_exposure_for_headless_capture"));
        // (a) The eye reads the HDR and writes only its own state; the renderer copies its
        // display lanes into camera-response buffers and nowhere else.
        let eye = include_str!("../shaders/exposure.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(&observer_shader_source(eye)).unwrap();
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
        let bindings: Vec<_> = module.global_variables.iter()
            .filter_map(|(_, v)| v.binding.as_ref().map(|b| (b.binding, v.name.clone().unwrap())))
            .collect();
        assert_eq!(bindings.len(), 3, "{bindings:?}");
        for (name, space) in module.global_variables.iter().map(|(_, v)| (v.name.clone().unwrap(), v.space)) {
            if name == "state" {
                assert!(matches!(space, wgpu::naga::AddressSpace::Storage { .. }));
            } else if name == "hdr" {
                assert_eq!(space, wgpu::naga::AddressSpace::Handle, "the HDR is a sampled texture");
            }
        }
        assert!(!eye.contains("texture_storage"), "the eye writes no image");
        // Earlier focused test modules may appear beside their pure helpers. Stop at this
        // source-contract module, after the actual renderer implementation and GPU copies.
        let shipping = renderer.split("\n#[cfg(test)]\nmod atmosphere_tests").next().unwrap();
        let copies: Vec<_> = shipping.match_indices("copy_buffer_to_buffer(eye,").collect();
        assert_eq!(copies.len(), 2, "cone and rod lanes into crush");
        assert!(renderer.contains("copy_buffer_to_buffer(eye, 0, &self.crush_buf, 44, 4)"));
        assert!(renderer.contains("copy_buffer_to_buffer(eye, 8, &self.crush_buf, 16, 8)"));
        assert!(eye.contains("if (state[1] < 0.0)"), "a view starts adapted to its first frame");
        // An authored lighting record cannot expose the camera; only the debug gate can fix it.
        let authored = renderer.split("pub fn apply_authored_lighting").nth(1).unwrap()
            .split("pub fn apply_campaign_atmosphere").next().unwrap();
        assert!(!authored.contains("exposure"), "authored lighting touched the exposure");
        let fixed = renderer.split("fn configured_camera_exposure_override").nth(1).unwrap()
            .split("\n}\n").next().unwrap();
        assert!(fixed.contains("DebugGates::active().fixed_camera_exposure"));
        assert_eq!(crate::debug_gates::DebugGates::SHIPPING.fixed_camera_exposure, None);
        // (b) One set of constants, generated from the CPU reference, reaches every consumer.
        for shader in [include_str!("../shaders/crush.wgsl"), include_str!("../shaders/exposure.wgsl"),
            include_str!("../shaders/pixel_particles.wgsl"), include_str!("../shaders/stylized_particles.wgsl")] {
            let composed = observer_shader_source(shader);
            assert!(composed.contains("Generated from src/observer.rs"));
        }
        // The crush applies the eye's two channels and nothing else before ACES.
        let crush = include_str!("../shaders/crush.wgsl");
        assert!(crush.contains("var c = eye_response(mix(centre, blur, concussion));"));
        assert!(crush.contains("return radiance * camera_white_gain() * cr.exposure + vec3(rods);"));

        let mesh = include_str!("../shaders/mesh.wgsl");
        assert!(!mesh.contains("let spring_fill"));
        assert!(!mesh.contains("let oder_fill"));
        assert!(!mesh.contains("interior_kill"));
    }

    #[test]
    fn instanced_pages_share_one_prototype_and_keep_independent_bounds_and_ids() {
        let prototype = 17;
        let local_min = glam::Vec3::splat(-1.0);
        let local_max = glam::Vec3::splat(1.0);
        let first_mats = [
            glam::Mat4::from_translation(glam::Vec3::new(4.0, 2.0, 8.0)),
            glam::Mat4::from_translation(glam::Vec3::new(10.0, 2.0, 12.0)),
        ];
        let first = plan_instanced_page(prototype, &first_mats, local_min, local_max, 0).unwrap();
        let second_mats = [glam::Mat4::from_translation(glam::Vec3::new(
            260.0, 2.0, 8.0,
        ))];
        let second = plan_instanced_page(
            prototype,
            &second_mats,
            local_min,
            local_max,
            first.object_id_end(),
        )
        .unwrap();

        assert_eq!(first.prototype, second.prototype, "one upload/BLAS handle");
        assert_eq!((first.inst_count, second.inst_count), (2, 1));
        assert_eq!(first.object_id_end(), second.object_id_base);
        assert_eq!(first.world_min, glam::Vec3::new(3.0, 1.0, 7.0));
        assert_eq!(first.world_max, glam::Vec3::new(11.0, 3.0, 13.0));
        assert_eq!(second.world_min, glam::Vec3::new(259.0, 1.0, 7.0));
        assert_eq!(second.world_max, glam::Vec3::new(261.0, 3.0, 9.0));
        assert!(first.world_max.x < second.world_min.x);
    }

    #[test]
    fn packed_instance_page_plan_has_aligned_disjoint_ranges_and_growth_slack() {
        let (ranges, total_bytes) = packed_page_ranges(&[1, 3, 4, 5]).unwrap();
        assert_eq!(
            ranges,
            vec![
                PackedPageRange {
                    byte_offset: 0,
                    capacity: 1,
                },
                PackedPageRange {
                    byte_offset: 64,
                    capacity: 4,
                },
                PackedPageRange {
                    byte_offset: 320,
                    capacity: 4,
                },
                PackedPageRange {
                    byte_offset: 576,
                    capacity: 8,
                },
            ]
        );
        assert_eq!(total_bytes, 1_088);
        for pair in ranges.windows(2) {
            let left_end = pair[0].byte_offset + u64::from(pair[0].capacity) * INSTANCE_MAT_BYTES;
            assert_eq!(left_end, pair[1].byte_offset);
            assert_eq!(pair[1].byte_offset % INSTANCE_MAT_BYTES, 0);
        }
        assert!(packed_page_ranges(&[1, 0, 2]).is_none());
        assert_eq!(instance_page_capacity(8), Some(8));
        assert_eq!(instance_page_capacity(9), Some(16));
        assert_eq!(instance_page_capacity(0), None);
    }

    #[test]
    fn fixed_moving_pool_accepts_empty_updates_and_rejects_overflow() {
        let capacity = fixed_instanced_pool_capacity(24).unwrap();
        assert_eq!(capacity, 24);
        assert_eq!(fixed_instanced_pool_update(capacity, 0), Some(0));
        assert_eq!(fixed_instanced_pool_update(capacity, 1), Some(1));
        assert_eq!(fixed_instanced_pool_update(capacity, 24), Some(24));
        assert_eq!(fixed_instanced_pool_update(capacity, 25), None);
        assert_eq!(fixed_instanced_pool_capacity(0), None);

        let dynamic_first = instanced_object_custom_data(1_200, 0, true);
        let dynamic_last = instanced_object_custom_data(1_200, capacity - 1, true);
        assert_eq!(dynamic_first, 0xC0_04B0);
        assert_eq!(dynamic_last, 0xC0_04C7);
        assert_ne!(
            dynamic_first,
            instanced_object_custom_data(1_200, 0, false),
            "moving exact geometry needs the temporal-dynamic bit while preserving its ID range"
        );
    }

    #[test]
    fn rt_slot_plan_compacts_empty_pools_without_renumbering_source_identities() {
        // Inactive blood capacity alone used to consume the whole 65,536-slot adapter budget.
        let empty_blood = plan_rt_instanced_slots(800, &[Some(0); 4]).unwrap();
        assert_eq!(empty_blood.end, 800);
        assert_eq!(empty_blood.offsets, vec![None; 4]);

        // Geometry identities reserve [24,24,16384] regardless of the following physical layout.
        let pose_a_id = instanced_object_custom_data(1_200, 0, true);
        let pose_b_id = instanced_object_custom_data(1_224, 0, true);
        let blood_id = instanced_object_custom_data(1_248, 0, true);
        let plan = plan_rt_instanced_slots(800, &[Some(3), None, Some(7), Some(0), None, Some(2)]).unwrap();
        assert_eq!(
            plan.offsets,
            vec![Some(800), None, Some(803), None, None, Some(810)]
        );
        assert_eq!(plan.end, 812);

        let after_pose_switch = plan_rt_instanced_slots(800, &[Some(3), None, Some(0), Some(4), None, Some(2)]).unwrap();
        assert_eq!(after_pose_switch.offsets, vec![Some(800), None, None, Some(803), None, Some(807)]);
        assert_eq!(after_pose_switch.end, 809);
        assert_ne!(plan.offsets[5], after_pose_switch.offsets[5], "physical blood range compacts");
        assert_eq!(blood_id, 0xC0_04E0, "blood custom ID does not depend on its physical slot");
        assert_ne!(pose_a_id, pose_b_id, "different source pose records must remain distinct");
        assert_eq!(instanced_object_custom_data(1_248, 0, true), blood_id);
        assert!(plan_rt_instanced_slots(u32::MAX - 2, &[Some(3)]).is_none());
    }

    /// Per-instance visibility of static pages (building modules and rubble): bits track ordinals,
    /// count only live instances, trim with the page, hash to the untouched state once every
    /// instance shows again, and a hidden instance is the zero matrix in the raster stream.
    #[test]
    fn instance_visibility_tracks_hidden_ordinals_and_zeroes_their_raster_entries() {
        let mut visibility = InstanceVisibility::default();
        assert!(visibility.is_empty() && visibility.hidden_below(1_000) == 0);
        assert!(visibility.set(3, true));
        assert!(!visibility.set(3, true), "setting the same state is no change");
        assert!(visibility.set(64, true) && visibility.set(130, true));
        assert!(visibility.is_hidden(3) && visibility.is_hidden(64) && !visibility.is_hidden(4));
        assert_eq!(visibility.hidden_below(200), 3);
        assert_eq!(visibility.hidden_below(65), 2, "ordinals past the live count do not count");
        assert_eq!(visibility.hidden_below(64), 1);
        visibility.trim(100);
        assert!(!visibility.is_hidden(130) && visibility.hidden_below(1_000) == 2);
        assert!(visibility.set(64, false) && visibility.set(3, false));
        assert_eq!(visibility, InstanceVisibility::default(), "all visible again equals untouched");
        let model = glam::Mat4::from_translation(glam::Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(raster_instance_matrix(false, model), model.to_cols_array_2d());
        let hidden = glam::Mat4::from_cols_array_2d(&raster_instance_matrix(true, model));
        // The instanced vertex stages rebuild the model with w = (0, 0, 0, 1): every vertex of a
        // hidden instance lands on one point, so no triangle has area.
        let rebuilt = glam::Mat4::from_cols(
            hidden.x_axis.truncate().extend(0.0),
            hidden.y_axis.truncate().extend(0.0),
            hidden.z_axis.truncate().extend(0.0),
            hidden.w_axis.truncate().extend(1.0),
        );
        let points: Vec<_> = [glam::Vec3::ZERO, glam::Vec3::X, glam::Vec3::new(3.0, -2.0, 7.0)]
            .into_iter()
            .map(|p| rebuilt.transform_point3(p))
            .collect();
        assert!(points.windows(2).all(|pair| pair[0] == pair[1]));
    }

    /// Hidden instances take no TLAS slot: a page whose instances are all hidden is laid out like
    /// an empty pool, and revealing one takes exactly one slot.
    #[test]
    fn hidden_instances_leave_the_compact_tlas_layout() {
        let live = |count: u32, hidden: &[u32]| {
            let mut visibility = InstanceVisibility::default();
            for &instance in hidden {
                visibility.set(instance, true);
            }
            count - visibility.hidden_below(count)
        };
        let intact_page = live(36, &[]);
        let rubble_page = live(26, &(0..26).collect::<Vec<_>>());
        let plan = plan_rt_instanced_slots(100, &[Some(intact_page), Some(rubble_page)]).unwrap();
        assert_eq!(plan.offsets, vec![Some(100), None]);
        assert_eq!(plan.end, 136);
        // One module fails: its intact instance hides, its rubble instance shows.
        let plan = plan_rt_instanced_slots(
            100,
            &[Some(live(36, &[7])), Some(live(26, &(0..26).filter(|i| *i != 5).collect::<Vec<_>>()))],
        )
        .unwrap();
        assert_eq!(plan.offsets, vec![Some(100), Some(135)]);
        assert_eq!(plan.end, 136, "one module is one instance in the ray scene either way");
    }

    /// Over budget, the lights that contribute most at the camera are kept: a near authored lamp
    /// survives a district of identical far fixtures that out-shine it at their own surfaces, a
    /// strong far fire still beats a weak near bulb, and ties keep the offer order.
    #[test]
    fn point_light_budget_keeps_the_lights_that_contribute_most_at_the_eye() {
        let light = |x: f32, intensity: f32, radius: f32| PointLight {
            pos: glam::Vec3::new(x, 2.0, 0.0),
            source_radius: radius,
            color: glam::Vec3::ONE,
            radiant_intensity: intensity,
        };
        let eye = glam::Vec3::new(0.0, 1.7, 0.0);
        let mut offered: Vec<PointLight> = (0..40).map(|i| light(400.0 + i as f32 * 8.0, 6.0, 0.06)).collect();
        offered.push(light(3.0, 8.0, 0.1)); // an authored courtyard lamp near the camera
        offered.push(light(200.0, 20_000.0, 1.5)); // a large fire far away
        let kept = select_point_lights(&offered, eye, 32);
        assert_eq!(kept.len(), 32);
        assert!(kept.iter().any(|l| l.pos.x == 3.0), "the near lamp was dropped");
        assert!(kept.iter().any(|l| l.pos.x == 200.0), "the far fire was dropped");
        // Among the identical far fixtures the nearest survive, in offer order.
        let fixtures: Vec<f32> = kept.iter().filter(|l| l.radiant_intensity == 6.0).map(|l| l.pos.x).collect();
        assert_eq!(fixtures, (0..30).map(|i| 400.0 + i as f32 * 8.0).collect::<Vec<_>>());
        // Within budget every offered light is kept as offered.
        assert_eq!(select_point_lights(&offered[..10], eye, 32).len(), 10);
        // At the source itself the score is the old peak radiance.
        let inside = light(0.0, 6.0, 0.06);
        let at = glam::Vec3::new(0.0, 2.0, 0.0);
        assert_eq!(select_point_lights(&[inside, light(1.0, 1.0, 0.5)], at, 1)[0].pos.x, 0.0);
    }

    /// TLAS capacity is explicit: the demand adds scene-mesh slots and live instances, an overflow
    /// names how many instances would be missing from the ray scene, and a scene within capacity
    /// reports none.
    #[test]
    fn tlas_overflow_is_reported_with_the_missing_instance_count() {
        let within = TlasDemand {
            mesh_slots: 2_786,
            instanced_live: 16_243,
            instanced_reserved: 82_444,
            capacity: TLAS_MAX_INSTANCES,
        };
        assert_eq!(within.needed(), 19_029);
        assert_eq!(within.overflow(), 0);
        assert!(tlas_overflow_message(within.needed(), within.capacity).is_none());
        assert!(within.describe().contains("19029 of 131072"));
        let over = TlasDemand { instanced_live: 140_000, ..within };
        assert_eq!(over.overflow(), 2_786 + 140_000 - 131_072);
        let message = tlas_overflow_message(over.needed(), over.capacity).unwrap();
        assert!(message.contains("142786") && message.contains("131072") && message.contains("11714"));
        assert!(message.contains("NOT in the ray scene"));
        assert!(tlas_overflow_message(TLAS_MAX_INSTANCES, TLAS_MAX_INSTANCES).is_none());
        assert!(tlas_overflow_message(TLAS_MAX_INSTANCES + 1, TLAS_MAX_INSTANCES).is_some());
    }

    #[test]
    fn rt_compacted_layout_clears_every_vacated_slot_including_last_live_pool() {
        // Exercise production planning and clearing against a CPU slot image. Identity ranges
        // stay fixed while two physical pools alternately empty, grow and swap their live counts.
        let mut slots = vec![None; 64];
        let mut previous_count = 0;
        for counts in [[3, 12, 0], [3, 0, 9], [3, 1, 1], [3, 0, 0], [0, 0, 0], [0, 0, 4]] {
            let plan = plan_rt_instanced_slots(0, &counts.map(Some)).unwrap();
            for (batch, count) in counts.into_iter().enumerate() {
                if let Some(start) = plan.offsets[batch] {
                    for instance in 0..count {
                        slots[(start + instance) as usize] = Some(instanced_object_custom_data(
                            [0, 100, 200][batch], instance, batch > 0));
                    }
                }
            }
            for slot in rt_vacated_slots(previous_count, plan.end, 64) { slots[slot] = None; }
            let live: Vec<_> = slots.iter().flatten().copied().collect();
            assert_eq!(live.len(), counts.iter().sum::<u32>() as usize);
            assert!(slots[plan.end as usize..].iter().all(Option::is_none), "stale geometry survived a shrinking layout");
            let unique: std::collections::BTreeSet<_> = live.iter().copied().collect();
            assert_eq!(unique.len(), live.len(), "compaction aliased independent full source IDs");
            if counts[0] > 0 { assert_eq!(&live[..3], &[0x40_0000, 0x40_0001, 0x40_0002]); }
            previous_count = plan.end;
        }
        assert_eq!(rt_vacated_slots(70, 60, 64), 60..64);
        assert!(rt_vacated_slots(2, 4, 64).is_empty());
    }

    fn spatial_test_bounds(cell_x: i32, cell_z: i32, inset: f32) -> InstSpatialBounds {
        spatial_test_page_bounds(cell_x, cell_z, inset, INST_SPATIAL_CELL_METRES)
    }

    #[test]
    fn dynamic_instance_upload_preserves_column_major_shader_payload() {
        let transforms = [
            glam::Mat4::from_cols_array(&[
                1.0, 2.0, 3.0, 0.12, 5.0, 6.0, 7.0, 0.0, 9.0, 10.0, 11.0, -0.18, 13.0, 14.0, 15.0,
                1.0,
            ]),
            glam::Mat4::from_translation(glam::Vec3::new(-100.0, 4.0, 27.0)),
        ];
        let packed: Vec<_> = transforms.iter()
        .map(|m| m.to_cols_array_2d()).collect();
        let direct: &[u8] = bytemuck::cast_slice(&transforms);
        assert_eq!(direct.len(), transforms.len() * INSTANCE_MAT_BYTES as usize);
        assert_eq!(direct, bytemuck::cast_slice::<_, u8>(&packed));
    }

    #[test]
    fn dynamic_instance_bounds_enclose_scaled_rotated_and_wind_bent_geometry() {
        let local_min = glam::Vec3::new(-0.2, 0.0, -0.3);
        let local_max = glam::Vec3::new(0.15, 0.48, 0.2);
        for ordinal in 0..64 {
            let phase = ordinal as f32 * 0.37;
            let mut model =
            glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::new(-0.7, 0.25 + phase, 1.3), glam::Quat::from_euler(glam::EulerRot::XYZ, phase, phase * 0.3, phase * 0.1),
                glam::Vec3::new(phase * 320.0, phase.sin(), -phase * 290.0),
            );
            model.x_axis.w = phase.sin() * 0.4;
            model.z_axis.w = phase.cos() * 0.4;
            for grass in [false, true] {
                let (min, max) = dynamic_instanced_bounds(&[model], local_min, local_max, grass);
                // Sample the whole curved blade, not just its extremities: the bend can cross an
                // affine face between corners when a blade is compressed, tilted or mirrored.
                for y in 0..=16 {
                    for corner in 0..4 {
                        let p = glam::Vec3::new(
                            if corner & 1 == 0 {
                                local_min.x
                            } else {
                                local_max.x
                            },
                            local_max.y * y as f32 / 16.0,
                            if corner & 2 == 0 {
                                local_min.z
                            } else {
                                local_max.z
                            },
                        );
                        let mut world = model.transform_point3(p);
                        if grass {
                            let u = (p.y / 0.48).clamp(0.0, 1.0);
                            world += glam::Vec3::new(model.x_axis.w, 0.0, model.z_axis.w) * u * u;
                        }
                        assert!(
                            world.cmpge(min).all() && world.cmple(max).all(),
                            "moving vertex {world:?} escaped {min:?}..{max:?}"
                        );
                    }
                }
            }
        }
        let (min, max) = dynamic_instanced_bounds(&[], local_min, local_max, true);
        assert_eq!(min, glam::Vec3::splat(f32::INFINITY));
        assert_eq!(max, glam::Vec3::splat(f32::NEG_INFINITY));
    }

    fn spatial_test_page_bounds(
        cell_x: i32,
        cell_z: i32,
        inset: f32,
        page_metres: f32,
    ) -> InstSpatialBounds {
        let x = cell_x as f32 * page_metres;
        let z = cell_z as f32 * page_metres;
        InstSpatialBounds::new(
            glam::Vec3::new(x + inset, -2.0, z + inset),
            glam::Vec3::new(x + page_metres - inset, 12.0, z + page_metres - inset),
        )
        .unwrap()
    }

    fn spatial_test_view(center_x: f32, center_z: f32, half: f32) -> glam::Mat4 {
        let eye = glam::Vec3::new(center_x, 100.0, center_z);
        let view =
            glam::Mat4::look_at_rh(eye, glam::Vec3::new(center_x, 0.0, center_z), glam::Vec3::Z);
        glam::Mat4::orthographic_rh(-half, half, -half, half, 0.1, 200.0) * view
    }

    fn brute_spatial_query(index: &InstSpatialIndex, view_proj: glam::Mat4) -> Vec<usize> {
        let planes = frustum_planes(view_proj);
        index
            .records
            .iter()
            .enumerate()
            .filter_map(|(batch, record)| {
                let bounds = record.as_ref()?.bounds;
                aabb_in_frustum(&planes, bounds.min, bounds.max).then_some(batch)
            })
            .collect()
    }

    #[test]
    fn spatial_index_keeps_exact_visibility_during_motion_within_shared_cells() {
        let mut index = InstSpatialIndex::default();
        for batch in 0..64 {
            let x = 2.0 + batch as f32 * 1.5;
            index.update(
                batch,
                InstSpatialBounds::new(
                    glam::Vec3::new(x, 0.0, 30.0),
                    glam::Vec3::new(x + 1.0, 2.0, 31.0),
                ),
            );
        }
        let cell = InstSpatialCell { x: 0, z: 0 };
        let members = index.cells[&cell].batches.clone();
        for step in 0..100 {
            let x = 5.0 + step as f32;
            let height = 2.0 + (step as f32 * 0.4).sin() * 0.5;
            index.update(
                0,
                InstSpatialBounds::new(
                    glam::Vec3::new(x, 0.0, 30.0),
                    glam::Vec3::new(x + 1.0, height, 31.0),
                ),
            );
            assert_eq!(
                index.cells[&cell].batches, members,
                "wind must not churn static memberships"
            );
            for center_x in [10.0, 50.0, 100.0] {
                let view = spatial_test_view(center_x, 30.5, 8.0);
                assert_eq!(index.query(view).batches, brute_spatial_query(&index, view));
            }
        }
        // Crossing a cell and emptying the pool must still remove the original membership.
        index.update(
            0,
            InstSpatialBounds::new(
                glam::Vec3::new(130.0, 0.0, 30.0),
                glam::Vec3::new(131.0, 2.0, 31.0),
            ),
        );
        assert!(!index.cells[&cell].batches.contains(&0));
        let view = spatial_test_view(130.5, 30.5, 1.0);
        assert_eq!(index.query(view).batches, vec![0]);
        index.update(0, None);
        assert!(index.query(view).batches.is_empty());
        assert_eq!(index.stats(), (1, 63, 0));

        // A rising pool in a cell of its own cannot leave an ever-taller visibility box behind.
        for step in 0..1_000 {
            let y = step as f32 * 0.1;
            index.update(64, InstSpatialBounds::new(
                glam::Vec3::new(260.0, y, 30.0), glam::Vec3::new(261.0, y + 1.0, 31.0)));
            let entry = &index.cells[&InstSpatialCell { x: 2, z: 0 }];
            assert!(entry.world_min.y <= y && entry.world_min.y >= y - 2.01);
            assert!(entry.world_max.y >= y + 1.0 && entry.world_max.y <= y + 3.01);
        }
    }

    #[test]
    #[ignore = "bounded CPU instance-update comparison; excludes GPU upload and frame time"]
    fn dynamic_instance_update_cpu_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;
        const COUNT: usize = 65_536;
        const UPDATES: usize = 24;
        let local_min = glam::Vec3::new(-0.2, 0.0, -0.2);
        let local_max = glam::Vec3::new(0.2, 0.48, 0.2);
        let transforms: Vec<_> = (0..COUNT).map(|i| {
            let mut model = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::new(1.0, 0.7 + (i % 7) as f32 * 0.1, 1.0),
                glam::Quat::from_rotation_y(i as f32 * 0.37),
                glam::Vec3::new((i % 256) as f32 * 0.375, 0.0, (i / 256) as f32 * 0.375),
            );
            model.x_axis.w = 0.12;
            model.z_axis.w = -0.08;
            model
        }).collect();
        let make_index = || {
            let mut index = InstSpatialIndex::default();
            for batch in 0..64 {
                index.update(batch, Some(spatial_test_bounds(0, 0, 2.0 + (batch % 8) as f32)));
            }
            index
        };
        let mut old_index = make_index();
        let mut new_index = make_index();
        let mut old_cpu = transforms.clone();
        let mut new_cpu = Vec::with_capacity(COUNT);
        let old_start = Instant::now();
        for _ in 0..UPDATES {
            // Previous CPU path: temporary upload conversion, fresh CPU copy, eight corners per
            // matrix, then removal/reinsertion that rescans static pages in all shared cells.
            let upload: Vec<_> = transforms.iter().map(|m| m.to_cols_array_2d()).collect();
            black_box(bytemuck::cast_slice::<_, u8>(&upload));
            old_cpu = transforms.to_vec();
            let mut min = glam::Vec3::splat(f32::INFINITY);
            let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
            for model in &transforms {
                let (lo, hi) = transform_aabb(*model, local_min, local_max);
                min = min.min(lo);
                max = max.max(hi);
            }
            old_index.remove(64);
            old_index.update(64, Some(InstSpatialBounds { min, max }));
            black_box((&old_cpu, &old_index));
        }
        let old_ms = old_start.elapsed().as_secs_f64() * 1_000.0 / UPDATES as f64;
        let new_start = Instant::now();
        for _ in 0..UPDATES {
            black_box(bytemuck::cast_slice::<_, u8>(&transforms));
            new_cpu.clear();
            new_cpu.extend_from_slice(&transforms);
            let (min, max) = dynamic_instanced_bounds(&transforms, local_min, local_max, true);
            new_index.update(64, Some(InstSpatialBounds { min, max }));
            black_box((&new_cpu, &new_index));
        }
        let new_ms = new_start.elapsed().as_secs_f64() * 1_000.0 / UPDATES as f64;
        assert_eq!(old_cpu, new_cpu);
        eprintln!(
            "[instance-update-cpu] {COUNT} matrices, {UPDATES} updates: previous {old_ms:.3} ms/update; reused {new_ms:.3} ms/update; {:.2}x. CPU preparation only; no GPU upload/render measurement.",
            old_ms / new_ms,
        );
    }

    #[test]
    fn spatial_index_matches_brute_force_and_reduces_page_tests() {
        let mut index = InstSpatialIndex::default();
        let mut batch = 0usize;
        // Model a dense district: several prototype pages share each deterministic world cell.
        for z in -20..=20 {
            for x in -20..=20 {
                for prototype in 0..8 {
                    index.update(
                        batch,
                        Some(spatial_test_bounds(x, z, 5.0 + prototype as f32)),
                    );
                    batch += 1;
                }
            }
        }
        assert_eq!(index.stats(), (1_681, 13_448, 0));

        let camera = glam::Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.1, 1_400.0)
            * glam::Mat4::look_at_rh(
                glam::Vec3::new(0.0, 24.0, 0.0),
                glam::Vec3::new(0.0, 4.0, -900.0),
                glam::Vec3::Y,
            );
        let camera_query = index.query(camera);
        assert_eq!(camera_query.batches, brute_spatial_query(&index, camera));
        assert!(camera_query.index_visits <= 1_681);
        assert!(camera_query.page_tests < batch / 2);

        // The camera-focused sun cascade is the especially cheap case: it touches a tiny grid
        // rectangle rather than all 13,448 pages.
        let shadow = spatial_test_view(0.0, 0.0, 60.0);
        let shadow_query = index.query(shadow);
        assert_eq!(shadow_query.batches, brute_spatial_query(&index, shadow));
        assert!(shadow_query.index_visits <= 16);
        assert!(shadow_query.page_tests <= 32);
    }

    #[test]
    fn spatial_index_handles_dynamic_reuse_fallbacks_and_split_views() {
        let mut index = InstSpatialIndex::default();
        index.update(0, Some(spatial_test_bounds(-5, 0, 8.0)));
        index.update(1, Some(spatial_test_bounds(5, 0, 8.0)));

        let left = spatial_test_view(-4.5 * INST_SPATIAL_CELL_METRES, 64.0, 80.0);
        let right = spatial_test_view(5.5 * INST_SPATIAL_CELL_METRES, 64.0, 80.0);
        assert_eq!(index.query(left).batches, vec![0]);
        assert_eq!(index.query(right).batches, vec![1]);
        let mut union = index.query(left).batches;
        union.extend(index.query(right).batches);
        union.sort_unstable();
        union.dedup();
        assert_eq!(union, vec![0, 1], "split cameras retain their union");

        // An editor slot keeps ID 0 while moving between cells, then becomes empty. Old cells must
        // not retain it, and empty-slot history must not grow the index.
        index.update(0, Some(spatial_test_bounds(7, 3, 8.0)));
        assert!(index.query(left).batches.is_empty());
        assert_eq!(index.stats().0, 2);
        index.update(0, None);
        assert_eq!(index.stats(), (1, 1, 0));

        // A legacy world-wide batch is represented exactly once in the fallback list, not copied
        // into thousands of cells, and remains visible to every intersecting view.
        let broad = InstSpatialBounds::new(
            glam::Vec3::new(-4_000.0, -5.0, -4_000.0),
            glam::Vec3::new(4_000.0, 20.0, 4_000.0),
        )
        .unwrap();
        index.update(2, Some(broad));
        assert_eq!(index.stats(), (1, 1, 1));
        assert_eq!(index.query(left).batches, vec![2]);
        assert_eq!(index.query(right).batches, vec![1, 2]);

        // Malformed legacy transforms retain the old conservative fail-open behaviour without
        // poisoning cell-coordinate math or panicking the query.
        index.update(
            3,
            Some(InstSpatialBounds {
                min: glam::Vec3::new(f32::NAN, 0.0, 0.0),
                max: glam::Vec3::ONE,
            }),
        );
        assert_eq!(index.stats(), (1, 1, 2));
        assert_eq!(index.query(left).batches, vec![2, 3]);
        assert_eq!(index.query(right).batches, vec![1, 2, 3]);
    }

    #[test]
    fn tracked_manifests_collapse_page_scans_to_occupied_cells() {
        for (act, expected_pages, expected_cells, expected_refs) in [
            (1u8, 580usize, 1_072usize, 7_315usize),
            (2, 1_938, 2_208, 28_098),
            (3, 1_336, 1_024, 19_066),
        ] {
            let manifest =
                crate::act_manifest::ActManifest::load(&crate::act_manifest::path_for_act(act))
                    .unwrap();
            let markers = manifest.effective_markers().unwrap();
            let spawn = markers
                .iter()
                .find(|marker| !marker.deleted && marker.kind == "Spawn")
                .unwrap()
                .pos;
            let goal = markers
                .iter()
                .find(|marker| !marker.deleted && marker.kind == "Goal")
                .unwrap()
                .pos;
            let paging = crate::editor::manifest_pages_from(&manifest).unwrap();
            assert_eq!(paging.stats().1, expected_pages);
            let mut index = InstSpatialIndex::default();
            let mut batch = 0usize;
            for prototype in &paging.prototypes {
                let page_metres = crate::editor::manifest_page_metres(&prototype.path);
                for page in &prototype.pages {
                    index.update(
                        batch,
                        Some(spatial_test_page_bounds(
                            page.cell.x,
                            page.cell.z,
                            1.0,
                            page_metres,
                        )),
                    );
                    batch += 1;
                }
            }
            let (occupied_cells, page_refs, fallbacks) = index.stats();
            assert_eq!(occupied_cells, expected_cells);
            assert_eq!(page_refs, expected_refs);
            assert_eq!(fallbacks, 0);
            assert_eq!(batch, expected_pages);
            println!(
                "[spatial-bench] act {act}: synthetic index has {occupied_cells} occupied cells / {page_refs} page refs"
            );

            // Benchmarkable spawn-to-goal camera proxy using the shipping 60-degree/6000 m
            // projection. This measures index work on the checked-in topology without a GPU.
            let eye = spawn + glam::Vec3::Y * 2.0;
            let forward = (goal - spawn).with_y(0.0).normalize();
            let camera = glam::Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.025, 6_000.0)
                * glam::Mat4::look_to_rh(eye, forward, glam::Vec3::Y);
            let query = index.query(camera);
            assert_eq!(query.batches, brute_spatial_query(&index, camera));
            assert!(query.index_visits <= occupied_cells);
            assert!(query.page_tests < expected_pages);
            println!(
                "[spatial-bench] act {act}: {expected_pages} pages / {occupied_cells} occupied cells -> {} cell visits / {} cell tests / {} candidate page tests / {} visible pages",
                query.index_visits,
                query.cell_tests,
                query.page_tests,
                query.batches.len()
            );
        }
    }
}
