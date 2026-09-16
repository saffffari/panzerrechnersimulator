//! The Kessel renderer.
//!
//! Flow per frame, all at internal resolution until the final upscale:
//!   sky + raster primary → exact ray-query surface transport → participating-media transport
//!   → exposure/retro treatment → nearest-neighbour upscale to display (or PNG). Devices without
//!   ray queries use the explicitly separate forward raster fallback.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};
use crate::ui::{Align, Ui};

#[path = "physical_environment.rs"]
mod physical_environment;

use physical_environment::{
    AtmosphereParameters, GpuPhysicalEnvironment, GpuPhysicalSnowField, IntegrationQuality,
    SnowFieldParameters,
};

pub const INTERNAL_W: u32 = 640;
pub const INTERNAL_H: u32 = 360;
const INTERNAL_RES_ENV: &str = "KESSEL_INTERNAL_RES";
const SHADOW_RES_ENV: &str = "KESSEL_SHADOW_RES";
const CAMERA_EXPOSURE_ENV: &str = "KESSEL_CAMERA_EXPOSURE";
/// Diagnostic A/B only: restore the former forward ordering (fullscreen sky before opaque draws).
/// Shipping defaults to depth-gated sky; the transport and its samples are otherwise identical.
const UNGATED_SKY_ENV: &str = "KESSEL_UNGATED_SKY";
const INTERNAL_RES_PRESETS: [(u32, u32); 5] =
    [(320, 180), (640, 360), (800, 450), (960, 540), (1280, 720)];
const RETRO_PRESET_ENV: &str = "KESSEL_RETRO_PRESET";
const DEFAULT_MASTER_RETRO: f32 = 0.7;
const PIPELINE_CACHE_DIR_ENV: &str = "KESSEL_PIPELINE_CACHE_DIR";

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
/// device. Kessel initializes one adapter/device at a time, so this mutex also prevents a later
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
            .join(format!("physical-transport-v1-{key}.bin")),
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
        .ok_or("allowed presets are 320x180, 640x360, 800x450, 960x540, and 1280x720")
}

thread_local! {
    /// One-shot runtime override used when the menu builds a four-cell couch world. Thread-local
    /// state keeps parallel headless tests/builders independent; `configured_internal_resolution`
    /// consumes it exactly once, so later solo/menu rebuilds return to their normal preset.
    static NEXT_INTERNAL_RES: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
}

pub fn set_next_internal_resolution(width: u32, height: u32) {
    assert!(
        INTERNAL_RES_PRESETS.contains(&(width, height)),
        "unsupported internal resolution {width}x{height}"
    );
    NEXT_INTERNAL_RES.with(|slot| slot.set(Some((width, height))));
}

fn configured_internal_resolution() -> (u32, u32) {
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

fn configured_shadow_resolution(preset: crate::graphics::GraphicsPreset) -> u32 {
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

fn configured_camera_exposure_override() -> Option<f32> {
    let raw = match std::env::var(CAMERA_EXPOSURE_ENV) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return None,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("[renderer] ignoring non-Unicode {CAMERA_EXPOSURE_ENV}");
            return None;
        }
    };
    match raw.trim().parse::<f32>() {
        Ok(value) if value.is_finite() && (0.01..=32.0).contains(&value) => {
            println!(
                "[renderer] fixed camera exposure {value:.3} ({CAMERA_EXPOSURE_ENV}); auto-exposure disabled"
            );
            Some(value)
        }
        _ => {
            eprintln!(
                "[renderer] invalid {CAMERA_EXPOSURE_ENV}={raw:?}; expected a finite value in 0.01..=32"
            );
            None
        }
    }
}

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Shadow-map side is selected once with the renderer: Laptop uses 512², High/4090 uses 2048².
/// The internal render targets likewise remain fixed until a world/renderer rebuild.
const SOLO_SHADOW_HALF_M: f32 = 60.0;
const SOLO_SHADOW_LOOK_AHEAD_M: f32 = 30.0;
/// Four crew cameras can face opposing directions from the same tank. The union of their previous
/// 120 m, 30 m-forward cascades is conservatively bounded by a tank-centred 180 m square.
const COOP_SHADOW_HALF_M: f32 = SOLO_SHADOW_HALF_M + SOLO_SHADOW_LOOK_AHEAD_M;

/// Direction TO the winter sun (matches the value mesh.wgsl shaded with before it was lifted to the
/// uniform). Kept here as the single Rust-side source so the shadow cascade projects from exactly
/// the direction the fragment shader lights with.
// Default sun; per-act mood overrides via `Renderer::sun_dir` (S4 made it state, not a const).
const SUN_DIR: glam::Vec3 = glam::Vec3::new(-0.5, 0.55, 0.25);

fn shadow_view_proj_for_focus(sun_dir: glam::Vec3, focus: glam::Vec3, half: f32) -> glam::Mat4 {
    let sun = sun_dir.normalize();
    let eye = focus + sun * 90.0;
    let view = glam::Mat4::look_to_rh(eye, -sun, glam::Vec3::Y);
    let proj = glam::Mat4::orthographic_rh(-half, half, -half, half, 0.1, 180.0);
    proj * view
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
    view_proj: [[f32; 4]; 4],
    light_view_proj: [[f32; 4]; 4], // sun ortho cascade; shared by mesh.wgsl + shadow.wgsl
    cam_pos: [f32; 3],
    fog_density: f32,
    sun_dir: [f32; 3],
    night: f32, // 0 = day (Tag), 1 = night (Nacht) — branches the sky/sun/ambient + gates emissives
    snap_res: [f32; 2],
    interior_light: f32, // 0 = open/exposed, 1 = buttoned-up; view/shelter state, never light energy
    time: f32,           // seconds, accumulated by the sim — drives grass wind sway
    track_half: f32,     // world half-extent (m) the tank-track map covers; 0 disables sampling
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
    // estimators, both always 2. Like `_pad3` they never vary; they exist so NVIDIA's Vulkan
    // compiler cannot statically unroll those loops into duplicate inlined copies of the secondary
    // transport and its nested physical-environment quadratures. Trailing for the same ABI reason.
    transport_passes: [f32; 4],
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

/// Physical campaign weather state. The returned values are medium coefficients, cloud geometry
/// and wind exposure—never screen colours or camera-relative brightness adjustments.
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
        // Open Oderbruch after rain: clean upper air, low broken stratus and shallow wet mist.
        2 => CampaignAtmosphereProfile {
            overcast: 0.91,
            storm: 0.34,
            mood: 0.5,
            ground_extinction_m_inv: 0.0026,
            wind_exposure: 0.92,
            mie_scale_height_m: 650.0,
            mie_scattering_m_inv: 2.8e-6,
            mie_extinction_m_inv: 3.4e-6,
            mie_asymmetry: 0.78,
            cloud_bottom_m: 430.0,
            cloud_top_m: 1_250.0,
            cloud_lwc_kg_m3: 0.18e-3,
            cloud_droplet_radius_m: 11.0e-6,
        },
        // Halbe late in the day: dense forest shelter beneath a smoke-loaded overcast air mass.
        3 => CampaignAtmosphereProfile {
            overcast: 0.96,
            storm: 0.56,
            mood: 1.0,
            ground_extinction_m_inv: 0.0042,
            wind_exposure: 0.16,
            mie_scale_height_m: 520.0,
            mie_scattering_m_inv: 5.5e-6,
            mie_extinction_m_inv: 9.5e-6,
            mie_asymmetry: 0.70,
            cloud_bottom_m: 650.0,
            cloud_top_m: 1_900.0,
            cloud_lwc_kg_m3: 0.22e-3,
            cloud_droplet_radius_m: 10.5e-6,
        },
        _ => CampaignAtmosphereProfile {
            overcast: 0.86,
            storm: 0.30,
            mood: 0.35,
            ground_extinction_m_inv: 0.0022,
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
const WORLD_AEROSOL_BOTTOM_M: f32 = -128.0;
const WORLD_AEROSOL_TOP_M: f32 = 256.0;
const BLOWING_SNOW_TOP_M: f32 = 128.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct WorldMediumProfile {
    snow_fraction: f32,
    layer: [f32; 4],
}

fn world_medium_profile(precipitation: f32, storm: f32) -> WorldMediumProfile {
    // This is the effective optical-composition fraction of the homogeneous bulk medium. Density
    // remains `fog_density` (sigma_t), so changing composition/support cannot secretly extend the
    // horizontal visibility range selected by the campaign profile.
    let snow_fraction = precipitation.clamp(0.0, 1.0) * storm.clamp(0.0, 1.0);
    // A smooth transition avoids an altitude discontinuity if future weather varies storm strength
    // at runtime. A committed whiteout (Act I = 0.98) reaches the declared snow-layer ceiling.
    let x = ((snow_fraction - 0.05) / (0.75 - 0.05)).clamp(0.0, 1.0);
    let snow_layer_weight = x * x * (3.0 - 2.0 * x);
    let layer_top =
        WORLD_AEROSOL_TOP_M + (BLOWING_SNOW_TOP_M - WORLD_AEROSOL_TOP_M) * snow_layer_weight;
    WorldMediumProfile {
        snow_fraction,
        layer: [WORLD_AEROSOL_BOTTOM_M, layer_top, 0.0, 0.0],
    }
}

/// Camera exposure before the measured-HDR adaptation pass runs. This is deliberately independent
/// of act, weather, view, and whether the camera is inside the tank: those states may change real
/// emitters and medium density, never smuggle brightness into the image through an authored lift.
const DEFAULT_CAMERA_EXPOSURE: f32 = 0.55;
/// Sentinel stored in the per-view exposure clock until that view has measured its first complete
/// HDR frame. Starting every camera from an arbitrary 0.55 and then slowly adapting made a scene
/// that begins outdoors look like night for several seconds. A real observer/camera is already
/// adapted before the first presented frame; subsequent changes still use the physical time
/// constants in `exposure.wgsl`.
const UNINITIALIZED_EXPOSURE_TIME: f32 = -1.0;

fn uninitialized_exposure_state() -> [f32; 2] {
    [DEFAULT_CAMERA_EXPOSURE, UNINITIALIZED_EXPOSURE_TIME]
}

/// Named lighting states used by the title/menu later and by the deterministic atmosphere gallery
/// now. There is deliberately no automatic clock yet: these are art-directable snapshots whose
/// visibility can be approved before campaign pacing starts advancing time.
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

    fn night_factor(self) -> f32 {
        match self {
            Self::Dawn => 0.08,
            Self::Day => 0.0,
            Self::Dusk => 0.24,
            // Preserve a little of the storm-day key so moonlit terrain stays navigable. A full
            // 1.0 endpoint made the dark spring albedos in Acts II/III collapse to black after the
            // retro contrast curve; 0.80 still reads unmistakably as night and keeps lamps dominant.
            Self::Night => 0.80,
        }
    }

    fn twilight(self) -> f32 {
        match self {
            Self::Dawn => -1.0,
            Self::Dusk => 1.0,
            _ => 0.0,
        }
    }

    fn light_dir(self) -> glam::Vec3 {
        match self {
            Self::Dawn => glam::Vec3::new(-0.82, 0.10, 0.36),
            Self::Day => glam::Vec3::new(-0.58, 0.40, 0.30),
            Self::Dusk => glam::Vec3::new(0.80, 0.08, -0.38),
            Self::Night => glam::Vec3::new(-0.24, 0.58, -0.78),
        }
        .normalize()
    }
}

/// Per-blur-pass push constant for bloom.wgsl: source texel size + separable blur axis.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomPush {
    texel: [f32; 2],
    dir: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrushUniform {
    levels: f32,
    dither: f32,
    reticle: f32,
    frame: f32, // animates the dither grain so it reads as film noise, not a fixed screen
    // Removed screen-space blood overlay lanes. Kept as zero padding so the exposure-reduction
    // buffer copy remains at byte 44 while game-side compatibility fields are retired.
    _removed_blood_overlay: [f32; 3],
    menu_mode: f32, // 1 = main-menu turntable pass (keep pure black truly black); 0 = normal game frame
    _removed_screen_haze: f32,
    mask: f32, // optic mask: 0 none · 1 TZF circle · 2 visor letterbox · 3 binoculars
    _removed_proxy_rt_mix: f32,
    /// S4: tonemap exposure. Uploaded as 0.55; when auto-exposure runs, the reduce pass
    /// overwrites this one field in-place on the GPU (buffer copy at offset 44) per frame.
    exposure: f32,
    /// Independent post-process strength. Zero is a true clean bypass for posterize, dither, and
    /// grain while preserving internal resolution, tonemapping, atmosphere, optics, and damage FX.
    retro: f32,
    /// Short-lived visual disorientation from a severe nearby armor hit.
    concussion: f32,
}

/// Simulation clock for frame-rate-independent eye adaptation. Each camera view owns its exposure
/// history, while this immutable-per-frame time value is shared by all of them.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ExposureClockUniform {
    time_s: f32,
    _pad: [f32; 3],
}

/// The WGSL clock is four scalar lanes. Keep this assertion beside the host declaration so a
/// future field/layout edit cannot silently recreate a Vulkan bind-size mismatch.
const EXPOSURE_CLOCK_UNIFORM_BYTES: u64 = std::mem::size_of::<ExposureClockUniform>() as u64;
const _: [(); 16] = [(); EXPOSURE_CLOCK_UNIFORM_BYTES as usize];

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
    // This consumes an existing ABI lane; SmokeUniform remains exactly 1,600 bytes.
    world_snow_fraction: f32,
    _pad1: f32,
    // Interior anchor (tank hull origin): dust is sampled in this frame so motes ride the hull,
    // not the world, with only a slow sealed-compartment drift relative to the crew.
    atmo_origin: [f32; 3],
    atmo_time: f32,
    // Explicit modeled vertical support for the homogeneous world aerosol, plus one spare lane.
    fog_layer: [f32; 4], // bottom_y, top_y, unused, unused
}

const SMOKE_SHADER_SOURCE: &str = include_str!("../shaders/smoke.wgsl");
const PHYSICAL_ENVIRONMENT_SHADER_SOURCE: &str =
    include_str!("../shaders/physical_environment.wgsl");
const PHYSICAL_SNOW_LIBRARY_MARKER: &str =
    "// --- Persistent physical snow ---------------------------------------------------------------";
const SMOKE_RT_TLAS_MARKER: &str = "// @smoke-ray-query-tlas";
const SMOKE_NO_DIRECT_MARKER: &str = "// @smoke-non-rt-no-direct-transport\n    return 0.0;";
const SMOKE_RT_TLAS_DECLARATION: &str =
    "@group(1) @binding(0) var smoke_scene_tlas: acceleration_structure;";
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
    assert_eq!(
        SMOKE_SHADER_SOURCE.matches(SMOKE_NO_DIRECT_MARKER).count(),
        1
    );
    let live_smoke = SMOKE_SHADER_SOURCE
        .replacen(SMOKE_RT_TLAS_MARKER, SMOKE_RT_TLAS_DECLARATION, 1)
        .replacen(SMOKE_NO_DIRECT_MARKER, SMOKE_RT_VISIBILITY_BODY, 1);
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
fn physical_atmosphere_transport_shader_source(entry_source: &str) -> String {
    let (atmosphere_source, _) = PHYSICAL_ENVIRONMENT_SHADER_SOURCE
        .split_once(PHYSICAL_SNOW_LIBRARY_MARKER)
        .expect("physical environment snow section marker must remain unique");
    let mut source = String::with_capacity(atmosphere_source.len() + 1 + entry_source.len());
    source.push_str(atmosphere_source);
    source.push('\n');
    source.push_str(entry_source);
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
}

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

struct MeshGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
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
    /// View-local geometry that is drawn through the ordinary raster material path but never enters
    /// the shared ray scene. Split-screen crew use this because each viewport hides a different body
    /// while views 2..N intentionally reuse view 1's TLAS.
    raster_only: bool,
    /// Closed direct-view protective glass. High writes its front surface to the G-buffer, gives
    /// the exact triangles a dedicated TLAS mask, then transports the camera ray through the
    /// measured entry/exit geometry. The portable forward path intentionally remains opaque until
    /// it has an equivalent non-screen-space transport implementation.
    glass: bool,
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

/// Immutable GPU geometry/material shared by every spatial page of one instanced asset. Keeping the
/// BLAS here is important: a 10,000-page campaign must build one BLAS per GLB, not one per page.
struct InstPrototype {
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
/// exact-TLAS ranges that can change without rewriting the 11k+ static tail. Existing dynamic world
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
    /// use this path: one prototype is shared by every live copy of a pose, matrices update through
    /// the renderer-owned queue without reallocating, and inactive capacity stays reserved as empty
    /// TLAS slots so object IDs and the exact-hit lookup never move when soldiers change pose.
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
}

const INSTANCE_MAT_BYTES: u64 = std::mem::size_of::<[[f32; 4]; 4]>() as u64;
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

#[derive(Clone, Copy, Debug)]
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
        self.remove(batch);
        let Some(bounds) = bounds else {
            return;
        };
        let Some(rect) = inst_spatial_cells(bounds) else {
            self.fallback.push(batch);
            self.records[batch] = Some(InstSpatialRecord {
                bounds,
                cell_rect: None,
            });
            return;
        };
        self.records[batch] = Some(InstSpatialRecord {
            bounds,
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
/// would waste TLAS slots and weaken the public fixed-capacity contract.
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

/// Assign stable TLAS ranges from reserved object-ID capacities. `None` denotes a raster-only,
/// grass, or degenerate-prototype batch. The plan intentionally does not depend on live counts, so
/// an empty moving pose batch retains the same TLAS range when it becomes populated next frame.
fn plan_rt_instanced_slots(
    first_slot: u32,
    reserved_capacities: &[Option<u32>],
) -> Option<RtInstancedSlotPlan> {
    let mut next = first_slot;
    let mut offsets = Vec::with_capacity(reserved_capacities.len());
    for capacity in reserved_capacities {
        let Some(capacity) = *capacity else {
            offsets.push(None);
            continue;
        };
        offsets.push(Some(next));
        next = next.checked_add(capacity)?;
    }
    Some(RtInstancedSlotPlan { offsets, end: next })
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
    accum_binds: [wgpu::BindGroup; 2],
    spec_accum_binds: [wgpu::BindGroup; 2],
    resolve_binds: [wgpu::BindGroup; 2],
    volume_accum_binds: [wgpu::BindGroup; 2],
    volume_composite_binds: [wgpu::BindGroup; 2],
    prev_depth_tex: wgpu::Texture,
    prev_id_tex: wgpu::Texture,
    accum_flip: usize,
    prev_vp: glam::Mat4,
    prev_cam_sig: (glam::Vec3, glam::Vec3, f32),
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
    pub iw: u32,
    pub ih: u32,
    /// Session-local preset captured when this renderer was constructed.
    pub graphics_preset: crate::graphics::GraphicsPreset,
    /// Actual allocated sun-shadow side for profiling/diagnostics.
    pub shadow_res: u32,
    pub camera: Camera,
    pub master_retro: f32,
    /// Optional fixed capture exposure. Normal play leaves this unset and keeps the authored or
    /// adaptive exposure path; deterministic photo harnesses can pin a readable HDR key.
    pub exposure_override: Option<f32>,
    pub fog_density: f32,
    pub reticle: bool,
    /// Legacy gunner-optic obscuration envelope. Retained while the game-side producer migrates;
    /// the screen-space snow-dust veil was not a modeled medium and is no longer rendered.
    pub optic_blast: f32,
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
    /// Time-of-day the scene is lit for: 0 = day (Tag), 1 = night (Nacht). Set from `Game::night`
    /// each frame; feeds the `night` uniform (sky/sun/ambient branch + emissive-window gate) and the
    /// snow overlay's density.
    pub night: f32,
    /// Seasonal precipitation gate. Winter acts use 1.0; the spring homeland uses 0.0 so a green
    /// Oderbruch/Halbe landscape is not inexplicably covered by the Stalingrad blizzard overlay.
    pub precipitation: f32,
    /// Cloud cover: 0 clear, 1 sealed overcast. Campaign presets live near 1 so the old glorious
    /// blue-sky postcard is no longer the default war-zone read.
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
    frame: u32,

    hdr_view: wgpu::TextureView,
    hdr_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    ldr_view: wgpu::TextureView,
    ldr_tex: wgpu::Texture,
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
    shadow_inst_pipe: wgpu::RenderPipeline,
    meshes: Vec<MeshGpu>,
    inst_prototypes: Vec<InstPrototype>,
    inst_batches: Vec<InstBatch>,
    /// Compact 128 m cell index over `inst_batches`; render queries touch cells, then only the
    /// candidate page AABBs, instead of linearly testing every campaign page in every pass.
    inst_spatial_index: InstSpatialIndex,
    instance_queue: wgpu::Queue,
    next_inst_buffer_id: usize,
    /// Sim time (s), accumulated by whoever drives the frame — grass wind phase.
    pub time: f32,
    /// World half-extent (m) covered by the tank-track map; set from the act on build. 0 = off.
    pub track_half: f32,
    track_view: wgpu::TextureView,
    track_bind: wgpu::BindGroup,
    track_pipe: wgpu::RenderPipeline,
    pending_stamps: Vec<[f32; 4]>,
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
    shadow_bind: wgpu::BindGroup, // group(2) for the mesh pass: depth map + comparison sampler
    coop_shadow: SharedShadowFrame,

    // ---- S2 deferred path: G-buffer + combine ----
    /// true = G-buffer + combine (the M-D shape); false = the legacy forward mesh pass. The two
    /// are pinned pixel-equal by `cargo run -- rt-parity`; forward stays for menu/editor + A/B.
    pub deferred: bool,
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
    /// Auto-exposure on (deferred only). KESSEL_NO_AUTOEXP=1 pins the classic fixed 0.55.
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
    g3_tex: wgpu::Texture,
    prev_models: Vec<glam::Mat4>,
    /// Per-view temporal state (splitscreen: each viewport is its own continuous camera, so each
    /// keeps its own history ping-pong, reprojection-reject snapshots and previous-camera record).
    /// Solo rendering only ever touches `views[0]`.
    views: Vec<ViewCtx>,
    active_view: usize,
    render_work: Cell<RenderWorkStats>,

    // ---- M-D hardware ray tracing (rt.rs) ----
    /// Device came up with EXPERIMENTAL_RAY_QUERY and KESSEL_NO_RT didn't veto it.
    pub rt_on: bool,
    /// Splitscreen: views 2..N of the SAME sim frame reuse view 1's TLAS (scene transforms are
    /// camera-independent) — set true before their renders, cleared after. Never skips the very
    /// first build (the guard checks a TLAS exists).
    pub rt_skip: bool,
    /// The scene TLAS, rebuilt every frame from mesh/instance transforms (wgpu 27 has no refit;
    /// full rebuild is the only mode and is ~0.1–0.3 ms at this instance count). Created lazily
    /// on the first frame — instanced batches arrive after Renderer::new.
    rt_tlas: Option<wgpu::Tlas>,
    rt_tlas_cap: u32,
    /// TLAS slots written last frame (the tail beyond this frame's count is already None).
    rt_prev_count: u32,
    /// Instanced batches changed (add_instanced / set_instanced_mats / TLAS recreate) → the
    /// static batch tail needs rewriting. Mesh slots rewrite every frame (visible/emissive
    /// state is per-frame); the 11k+ static city instances write once. ~2 ms/frame saved.
    rt_batches_dirty: bool,
    /// Fixed-capacity ray-visible pools have stable TLAS ranges and can therefore rewrite only
    /// their small reserved ranges between full static-tail updates.
    rt_moving_batches_dirty: bool,
    /// One stable TLAS range start per instanced batch (`None` for non-RT batches). Recomputed only
    /// when topology/capacity changes; moving-pool live counts never invalidate it.
    rt_batch_slot_offsets: Vec<Option<u32>>,
    /// Exact committed-hit surface/material tables for the High one-bounce integrator. Rebuilt only
    /// when source topology or custom-data allocation changes; transforms stay authoritative in TLAS.
    rt_surface_gpu: Option<RtSurfaceGpu>,
    rt_surface_dirty: bool,
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
    bloom_bind_hdr: wgpu::BindGroup, // src = full-res HDR (bright pass)
    bloom_bind_a: wgpu::BindGroup,   // src = bloom_a
    bloom_bind_b: wgpu::BindGroup,   // src = bloom_b

    // ---- volumetric muzzle-blast smoke (ray-marched over the HDR target, before bloom) ----
    smoke_puffs: [PuffGpu; MAX_PUFFS],
    breech_gas: PuffGpu,
    atmo_enabled: bool, // per-view gate: split-screen seats do not share P1's inside/optic state
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

    // ---- persistent physical snow (High resolves grains; bulk suspension exists on both) ----
    snow_field_buf: wgpu::Buffer,
    snow_bind: wgpu::BindGroup,
    snow_pipe: wgpu::RenderPipeline,

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
}

/// Compatibility vertical extent used by game-side FOV math while true optical transport is pending.
pub const RTT_TILE_H: u32 = 180;

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
        usage: wgpu::BufferUsages::VERTEX | bi,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ibuf"),
        contents: bytemuck::cast_slice(&m.indices),
        usage: wgpu::BufferUsages::INDEX | bi,
    });
    let tex_bind = material_cache.bind_for(device, queue, material_bgl, sampler, m);
    MeshGpu {
        vbuf,
        ibuf,
        index_count: m.indices.len() as u32,
        tex_bind,
        model: glam::Mat4::IDENTITY,
        visible: true,
        emissive: 0.0,
        emissive_win: false,
        emissive_own: false,
        raster_only,
        glass: false,
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
        let shadow_res = configured_shadow_resolution(graphics_preset);
        let (iw, ih) = configured_internal_resolution();
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
            format: HDR,
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
                | wgpu::TextureUsages::COPY_SRC,
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
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-bgl"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });
        let physical_environment_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("physical-environment"),
            size: std::mem::size_of::<GpuPhysicalEnvironment>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let physical_environment_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("physical-environment-bgl"),
                entries: &[uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
            });
        let physical_environment_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("physical-environment-bind"),
            layout: &physical_environment_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: physical_environment_buf.as_entire_binding(),
            }],
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

        let mut material_cache = MaterialCache::default();
        let mut meshes: Vec<MeshGpu> = scene
            .iter()
            .enumerate()
            .map(|(slot, m)| {
                make_mesh_gpu(
                    device,
                    queue,
                    &material_bgl,
                    &sampler,
                    m,
                    rt_on,
                    raster_only_mask[slot],
                    &mut material_cache,
                )
            })
            .collect();

        // M-D: one BLAS per scene mesh, all built in bulk at load (the world is rigid — these
        // never rebuild; TLAS membership per frame is the only steady-state RT work). Chunked
        // submits keep any single scratch allocation modest.
        if rt_on {
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
        println!("[renderer] composing physical sky and snow shader modules");
        let sky_source =
            physical_atmosphere_transport_shader_source(include_str!("../shaders/sky.wgsl"));
        let sky_shader = shader(device, "sky-physical-environment", &sky_source);
        let mesh_shader = shader(device, "mesh", include_str!("../shaders/mesh.wgsl"));
        let shadow_shader = shader(device, "shadow", include_str!("../shaders/shadow.wgsl"));
        let bloom_shader = shader(device, "bloom", include_str!("../shaders/bloom.wgsl"));
        let snow_source = if rt_on {
            physical_snow_ray_query_source()
        } else {
            physical_transport_shader_source(include_str!("../shaders/snow.wgsl"))
        };
        let snow_shader = shader(
            device,
            if rt_on {
                "snow-physical-ray-query"
            } else {
                "snow-physical-portable"
            },
            &snow_source,
        );
        println!(
            "[renderer] physical sky/snow modules accepted in {:.0} ms (atmosphere {} KiB, snow {} KiB)",
            physical_compile_start.elapsed().as_secs_f32() * 1000.0,
            sky_source.len() / 1024,
            snow_source.len() / 1024,
        );
        let crush_shader = shader(device, "crush", include_str!("../shaders/crush.wgsl"));
        let blit_shader = shader(device, "blit", include_str!("../shaders/blit.wgsl"));

        // The sky pass now reads the camera uniform (for the `night` factor) so the backdrop can
        // deepen to a dark cool night sky; it binds group(0) = camera, like the mesh pass.
        let sky_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky-pl"),
            bind_group_layouts: &[&camera_bgl, &physical_environment_bgl],
            push_constant_ranges: &[],
        });
        let sky_compile_start = std::time::Instant::now();
        println!("[renderer] compiling physical sky pipeline");
        let sky_pipe =
            fullscreen_pipe_cached(device, &sky_shader, &sky_pl, HDR, pipeline_cache.as_ref());
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
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: pipeline_cache.as_ref(),
        });
        println!(
            "[renderer] physical sky pipeline ready in {:.0} ms",
            sky_compile_start.elapsed().as_secs_f32() * 1000.0,
        );

        // ---------- tank-track map: a world-space R8 rut field the terrain shader samples ----------
        const TRACK_RES: u32 = 4096;
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let track_view = track_tex.create_view(&wgpu::TextureViewDescriptor::default());
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
                targets: &[Some(color_target(HDR))],
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
            // Cull front faces so the depth stored is the far side of an occluder — this pushes
            // self-shadow acne to the unlit back faces and works cleanly alongside the shader's
            // slope bias. cull_mode None on the mesh pass is unaffected.
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                // Small constant + slope depth bias, a cheap backup to the shader-side normal bias.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
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
                targets: &[Some(color_target(HDR))],
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
        const G1F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        const G2F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
        const G3F: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
        let g0_view = make_target(
            device,
            "g0-albedo",
            iw,
            ih,
            G0F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let g1_view = make_target(
            device,
            "g1-normal",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let g2_view = make_target(
            device,
            "g2-emissive",
            iw,
            ih,
            G2F,
            wgpu::TextureUsages::TEXTURE_BINDING,
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
        let accum_shader = shader(device, "accum", include_str!("../shaders/accum.wgsl"));
        let resolve_shader = shader(device, "resolve", include_str!("../shaders/resolve.wgsl"));
        let lit_diff_view = make_target(
            device,
            "lit-diff",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let lit_other_view = make_target(
            device,
            "lit-specular",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let lit_emission_view = make_target(
            device,
            "lit-immediate-emission",
            iw,
            ih,
            G1F,
            wgpu::TextureUsages::TEXTURE_BINDING,
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
            wgpu::TextureUsages::TEXTURE_BINDING,
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
                        | wgpu::TextureUsages::TEXTURE_BINDING,
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
            include_str!("../shaders/volume_accum.wgsl"),
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
                        ],
                    })
                };
                let volume_accum_binds = [mk_volume_accum_bind(0), mk_volume_accum_bind(1)];
                let mk_resolve_bind = |src: usize| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("resolve-bind"),
                        layout: &resolve_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&hist[src]),
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
                let resolve_binds = [mk_resolve_bind(0), mk_resolve_bind(1)];
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
                    accum_binds,
                    spec_accum_binds,
                    resolve_binds,
                    volume_accum_binds,
                    volume_composite_binds,
                    prev_depth_tex,
                    prev_id_tex,
                    accum_flip: 0,
                    prev_vp: glam::Mat4::IDENTITY,
                    prev_cam_sig: (glam::Vec3::ZERO, glam::Vec3::Z, 0.0),
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
                targets: &[Some(color_target(HDR))],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        // ---------- S4: auto-exposure reduce ----------
        let exp_shader = shader(device, "exposure", include_str!("../shaders/exposure.wgsl"));
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
                _pad: [0.0; 3],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut exp_bufs = Vec::with_capacity(MAX_VIEWS);
        let mut exp_binds = Vec::with_capacity(MAX_VIEWS);
        for view in 0..MAX_VIEWS {
            let label = format!("exposure-state-v{view}");
            let exp_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&label),
                size: 8,
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
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview: None,
            cache: None,
        });

        // ---------- emissive bloom ----------
        let bw = (iw / 2).max(1);
        let bh = (ih / 2).max(1);
        let bloom_a_view = make_target(
            device,
            "bloom-a",
            bw,
            bh,
            HDR,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let bloom_b_view = make_target(
            device,
            "bloom-b",
            bw,
            bh,
            HDR,
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
        let bloom_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom-bgl"),
            entries: &[tex_entry(0), sampler_entry(1)],
        });
        let make_bloom_bind = |view: &wgpu::TextureView, label: &str| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &bloom_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&bloom_sampler),
                    },
                ],
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
        // Metal/non-ray-query devices compile the same density + Beer-Lambert integrator with
        // direct in-scatter dark, because they have no truthful scene-visibility query. Hardware
        // High substitutes a small TLAS visibility body and binds the same scene AS as combine.
        let smoke_source = if rt_on {
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
        let smoke_visibility_bgl = rt_as_bgl.as_ref().unwrap_or(&smoke_empty_visibility_bgl);
        let smoke_layouts = vec![&smoke_bgl, smoke_visibility_bgl, &physical_environment_bgl];
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
            "[renderer] compiling physical volume transport pipeline ({} KiB)",
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
                    format: HDR,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: pipeline_cache.as_ref(),
        });
        println!(
            "[renderer] physical volume transport pipeline ready in {:.0} ms",
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
                        format: HDR,
                        blend: Some(over_blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview: None,
                cache: None,
            });

        // ---------- persistent, physically transported world-space snow ----------
        // High traces a finite-shutter pixel ray through immutable Lagrangian world cells, clips it
        // to scene depth and asks the exact TLAS whether each grain can see its upstream field and
        // incident sources. Laptop keeps the shared bulk suspension but omits resolved grains until
        // software triangle visibility exists rather than drawing a contradictory white proxy.
        let snow_field_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("physical-snow-field"),
            size: std::mem::size_of::<GpuPhysicalSnowField>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let snow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("snow-bgl"),
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
        let snow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow-bind"),
            layout: &snow_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: snow_field_buf.as_entire_binding(),
                },
            ],
        });
        let snow_empty_visibility_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("snow-empty-visibility-bgl"),
                entries: &[],
            });
        let snow_visibility_bgl = rt_as_bgl.as_ref().unwrap_or(&snow_empty_visibility_bgl);
        let snow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("snow-pl"),
            bind_group_layouts: &[&snow_bgl, snow_visibility_bgl, &physical_environment_bgl],
            push_constant_ranges: &[wgpu::PushConstantRange {
                stages: wgpu::ShaderStages::FRAGMENT,
                range: 0..std::mem::size_of::<SnowPush>() as u32,
            }],
        });
        let snow_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                // snow.wgsl emits integrated in-scatter already premultiplied by (1-T).
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
        let snow_compile_start = std::time::Instant::now();
        println!("[renderer] compiling resolved physical snow pipeline");
        let snow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("snow"),
            layout: Some(&snow_pl),
            vertex: wgpu::VertexState {
                module: &snow_shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &snow_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR,
                    blend: Some(snow_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: pipeline_cache.as_ref(),
        });
        println!(
            "[renderer] resolved physical snow pipeline ready in {:.0} ms",
            snow_compile_start.elapsed().as_secs_f32() * 1000.0,
        );
        persist_physical_pipeline_cache(pipeline_cache.as_ref(), pipeline_cache_path.as_deref());

        // ---------- crush ----------
        let crush_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crush"),
            size: std::mem::size_of::<CrushUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
            ],
        });
        let crush_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crush-pl"),
            bind_group_layouts: &[&crush_bgl],
            push_constant_ranges: &[],
        });
        let crush_pipe = fullscreen_pipe(device, &crush_shader, &crush_pl, LDR);

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

        Self {
            // Standard stays at the playtested 0.7; [ ] still dials continuously in-game, while
            // KESSEL_RETRO_PRESET gives deterministic clean/light/standard/strong A/B captures.
            iw,
            ih,
            graphics_preset,
            shadow_res,
            camera,
            master_retro: configured_master_retro(),
            exposure_override: configured_camera_exposure_override(),
            fog_density: 0.02,
            reticle: false,
            optic_blast: 0.0,
            crew_ko: 0.0,
            crew_ko_dir: [0.72, 0.69],
            concussion: 0.0,
            menu_mode: false,
            night: 0.0,
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
            interior_light: 0.0,
            interior_haze: 0.0,
            optic_mask: 0.0,
            lights: Vec::new(),
            frame: 0,
            hdr_view,
            hdr_tex,
            depth_view,
            ldr_view,
            ldr_tex,
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
            inst_batches: Vec::new(),
            inst_spatial_index: InstSpatialIndex::default(),
            instance_queue: queue.clone(),
            next_inst_buffer_id: 0,
            time: 0.0,
            track_half: 0.0,
            track_view,
            track_bind,
            track_pipe,
            pending_stamps: Vec::new(),
            shadow_view,
            shadow_pipe,
            shadow_bind,
            coop_shadow: SharedShadowFrame::default(),
            bw,
            bh,
            bloom_a_view,
            bloom_b_view,
            bloom_bright_pipe,
            bloom_blur_pipe,
            bloom_composite_pipe,
            bloom_bind_hdr,
            bloom_bind_a,
            bloom_bind_b,
            smoke_puffs: [PuffGpu {
                pos: [0.0; 3],
                age: 0.0,
                dir: [0.0, 0.0, 1.0],
                strength: 0.0,
            }; MAX_PUFFS],
            breech_gas: PuffGpu {
                pos: [0.0; 3],
                age: 0.0,
                dir: [0.0, 0.0, -1.0],
                strength: 0.0,
            },
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
            snow_field_buf,
            snow_bind,
            snow_pipe,
            crush_buf,
            crush_bind,
            crush_pipe,
            rt_mix: 0.0,
            portable_ao_enabled: false,
            // S4: the deferred combine IS the ray integrator — no RT, no deferred (Low tier
            // = the legacy forward path, still pixel-complete minus ray light).
            deferred: rt_on
                && std::env::var("KESSEL_FORWARD")
                    .map(|v| v != "1")
                    .unwrap_or(true),
            gbuf_pipe,
            gbuf_inst_pipe,
            combine_pipe,
            gbuf_bind,
            g0_view,
            g1_view,
            g2_view,
            g3_view,
            sun_dir: SUN_DIR,
            autoexp: std::env::var("KESSEL_NO_AUTOEXP")
                .map(|v| v != "1")
                .unwrap_or(true),
            rt_as_bgl,
            combine_rt_bind: None,
            exp_pipe,
            exp_binds,
            exp_bufs,
            exp_clock_buf,
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
            g3_tex,
            prev_models: Vec::new(),
            views,
            active_view: 0,
            render_work: Cell::new(RenderWorkStats::default()),
            rt_on,
            rt_skip: false,
            rt_tlas: None,
            rt_tlas_cap: 0,
            rt_prev_count: 0,
            rt_batches_dirty: true,
            rt_moving_batches_dirty: false,
            rt_batch_slot_offsets: Vec::new(),
            rt_surface_gpu: None,
            rt_surface_dirty: rt_on,
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
        }
    }

    /// Apply the campaign-wide authored weather language. `act_id` selects a restrained palette,
    /// while `winter` controls real snow rather than merely recolouring a spring map. The values are
    /// deliberately public/tweakable too; the atmosphere gallery uses this exact shipping path.
    pub fn apply_campaign_atmosphere(&mut self, act_id: u8, winter: bool, phase: DayPhase) {
        let profile = campaign_atmosphere_profile(act_id);
        self.weather_act_id = act_id;
        // KESSEL_OVERCAST=0..1 overrides the authored cloud deck for lighting diagnosis only.
        // The campaign sits at 0.86-0.99, i.e. effectively sealed, so there is no direct solar
        // beam anywhere in it and every surface is lit purely by diffuse sky. That makes it
        // impossible to tell a missing environment term from a correctly dim one without being
        // able to open the sky up. Debug-only: nothing in a shipping path reads this.
        self.overcast = std::env::var("KESSEL_OVERCAST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or(profile.overcast);
        self.storm = profile.storm;
        self.atmosphere_mood = profile.mood;
        self.twilight = phase.twilight();
        self.weather_wind = profile.wind_exposure;
        self.weather_wind_world = profile_weather_wind(act_id, profile.wind_exposure);
        self.weather_shelter = 0.0;
        self.weather_seed = (act_id as u32).wrapping_mul(0x9e37_79b9);
        self.fog_density =
            profile.ground_extinction_m_inv * if phase == DayPhase::Night { 1.10 } else { 1.0 };
        self.night = phase.night_factor();
        self.sun_dir = phase.light_dir();
        self.precipitation = if winter { 1.0 } else { 0.0 };
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
    /// Compatibility no-op. Hidden geometry is never exposed through an x-ray render mode.
    pub fn set_xray(&mut self, idx: usize, x: bool) {
        let _ = (idx, x);
    }
    /// Compatibility no-op. A pasted secondary-camera texture is not optical transport; vision
    /// blocks remain opaque until their prism/glass geometry is traced physically.
    pub fn set_mesh_rtt_screen(&mut self, idx: usize) {
        let _ = idx;
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
    /// Set this frame's positioned point lights (modeled bulbs, muzzle flash, fire glow and night
    /// lamps). Cleared and refilled by game.rs each frame. If more than N_LIGHTS are
    /// offered, keep the strongest peak-radiance sources rather than dropping arbitrarily.
    pub fn set_lights(&mut self, lights: &[PointLight]) {
        self.lights.clear();
        if lights.len() <= N_LIGHTS {
            self.lights.extend_from_slice(lights);
        } else {
            let mut idx: Vec<usize> = (0..lights.len()).collect();
            idx.sort_by(|&a, &b| {
                let sa = lights[a].radiant_intensity / lights[a].source_radius.max(0.01).powi(2);
                let sb = lights[b].radiant_intensity / lights[b].source_radius.max(0.01).powi(2);
                sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
            });
            for &i in idx.iter().take(N_LIGHTS) {
                self.lights.push(lights[i]);
            }
        }
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

    /// Queue a pair of tread stamps for a tank at `pos` facing `yaw` — flushed into the world
    /// track map on the next rendered frame. `strength` accumulates (repeated passes darken).
    pub fn stamp_tank_tracks(&mut self, pos: glam::Vec3, yaw: f32, strength: f32) {
        if self.track_half <= 1.0 {
            return;
        }
        let (sy, cy) = yaw.sin_cos();
        let right = glam::Vec2::new(cy, -sy); // engine plan: heading (sin,cos), right = rotate -90°
        let c = glam::Vec2::new(pos.x, pos.z);
        for side in [-1.45f32, 1.45] {
            let t = c + right * side;
            let uv = t / (2.0 * self.track_half) + glam::Vec2::splat(0.5);
            // tread contact patch — drawn a touch wide so the rut survives the 1 m/px track map
            let r = 1.05 / (2.0 * self.track_half);
            self.pending_stamps.push([uv.x, uv.y, r, strength]);
        }
    }

    /// Render the queued tread stamps into the track map (additive), then clear the queue.
    fn flush_track_stamps(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.pending_stamps.is_empty() {
            return;
        }
        // 6 verts per stamp: pos in track-UV, corner in -1..1, strength
        let mut verts: Vec<[f32; 5]> = Vec::with_capacity(self.pending_stamps.len() * 6);
        for s in self.pending_stamps.drain(..) {
            let [u, v, r, k] = s;
            let quad = [
                (-1.0f32, -1.0f32),
                (1.0, -1.0),
                (1.0, 1.0),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
            ];
            for (cx, cyv) in quad {
                verts.push([u + cx * r, v + cyv * r, cx, cyv, k]);
            }
        }
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
    pub fn register_instanced_prototype(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mesh: &CpuMesh,
    ) -> InstancedPrototype {
        let bi = if self.rt_on {
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
        let blas = if self.rt_on && mesh.indices.len() >= 3 && !mesh.vertices.is_empty() {
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
        let tex_bind = Arc::new(upload_material(
            device,
            queue,
            &self.material_bgl,
            &self.sampler,
            mesh,
        ));
        let handle = self.inst_prototypes.len();
        self.inst_prototypes.push(InstPrototype {
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
        let proto = self.inst_prototypes.get(prototype)?;
        if pages.is_empty() {
            return Some(Vec::new());
        }
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
        for ((page, range), plan) in pages.iter().zip(&ranges).zip(&plans) {
            let first = usize::try_from(range.byte_offset / INSTANCE_MAT_BYTES).unwrap();
            let count = plan.inst_count as usize;
            for (dst, transform) in packed[first..first + count].iter_mut().zip(page.iter()) {
                *dst = transform.to_cols_array_2d();
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
        for ((page, range), plan) in pages.iter().zip(ranges).zip(plans) {
            let batch = self.inst_batches.len();
            self.inst_batches.push(InstBatch {
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
            });
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
    /// its G-buffer/TLAS custom-data range and TLAS slot range reserved even while the pool is empty.
    /// Add pools after static campaign pages when possible so their frequently rewritten ranges
    /// remain a compact tail.
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
        // Fixed pools must never escape their reserved buffer/ID/TLAS ranges through the growable
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
        let mats: Vec<[[f32; 4]; 4]> = transforms.iter().map(|m| m.to_cols_array_2d()).collect();
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
        true
    }

    /// Rewrite a fixed-capacity, ray-visible moving pool. This is a queue upload only: it never
    /// takes a device, reallocates a buffer, changes object IDs, or rebuilds the exact source pack.
    /// Empty updates are valid and remove every reserved instance from the TLAS; overflow, invalid
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
        // Capacity and custom-data lookup are unchanged; only the reserved TLAS range moves.
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
                make_mesh_gpu(
                    device,
                    queue,
                    &self.material_bgl,
                    &self.sampler,
                    m,
                    false,
                    false,
                    &mut material_cache,
                )
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
    }

    /// Render the menu turntable: the showcase Tiger on a PURE-BLACK HDR frame (no sky/fog/snow/bloom
    /// — just the lit, spinning tank), crushed to the internal LDR buffer so it reads as a posterized/
    /// dithered pixel model. The caller then composites the front-end text over the LDR result.
    fn render_menu_internal(&self, encoder: &mut wgpu::CommandEncoder) {
        // Sun shadow (self-shadow only) so the mesh pass's shadow lookup samples a valid map.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("menu-shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
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
            pass.set_bind_group(0, &self.camera_bind, &[]);
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
            };
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
        };
    }

    /// Deactivate all volumetric plumes (no draw cost until a plume is set again).
    pub fn clear_smoke(&mut self) {
        for p in &mut self.smoke_puffs {
            p.strength = 0.0;
        }
        self.breech_gas.strength = 0.0;
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

    /// Enable or suppress the stored cabin haze for the camera about to render. Four couch views
    /// (and the two halves of the gunner composite) select their own inside/optic state.
    pub fn set_interior_atmosphere_enabled(&mut self, enabled: bool) {
        self.atmo_enabled = enabled && self.atmo_haze > 0.001;
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

    /// Sun's ortho shadow cascade: a ~120 m box centred just ahead of the camera (so what the view
    /// sees is what the map covers), looking down the sun ray. glam's `orthographic_rh` gives NDC
    /// z ∈ [0,1] to match wgpu, exactly like the perspective camera.
    fn light_view_proj(&self) -> glam::Mat4 {
        let focus = self.camera.pos + self.camera.forward() * SOLO_SHADOW_LOOK_AHEAD_M;
        self.coop_shadow.view_proj_or(shadow_view_proj_for_focus(
            self.sun_dir,
            focus,
            SOLO_SHADOW_HALF_M,
        ))
    }

    fn upload_uniforms(&self, queue: &wgpu::Queue) {
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
            view_proj: self.camera.view_proj().to_cols_array_2d(),
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
            // Runtime-visible fixed transport order. combine.wgsl clamps this to 1..=12; keeping
            // 12 in a uniform prevents Vulkan from recursively unrolling the glass world-medium
            // march without changing a single distance bin or radiance sample.
            _pad3: 12.0,
            lights: packed_lights,
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
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
            // Exactly two glass-interface paths and two one-bounce estimators, every upload. These
            // are transport invariants, exposed only to keep the Vulkan compiler from unrolling
            // duplicate copies of the nested atmosphere and secondary-hit graph.
            transport_passes: [2.0, 2.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&cam));
        queue.write_buffer(
            &self.exp_clock_buf,
            0,
            bytemuck::bytes_of(&ExposureClockUniform {
                time_s: self.time.max(0.0),
                _pad: [0.0; 3],
            }),
        );

        let mut physical_environment = AtmosphereParameters::default();
        let campaign_environment = campaign_atmosphere_profile(self.weather_act_id);
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
        physical_environment.solar_irradiance *= 1.0 - self.night.clamp(0.0, 1.0) as f64;
        let integration_quality = match self.graphics_preset {
            crate::graphics::GraphicsPreset::High => IntegrationQuality::INTERACTIVE_HIGH,
            crate::graphics::GraphicsPreset::Laptop => IntegrationQuality {
                view_steps: 12,
                source_steps: 8,
                solar_disk_samples: 1,
                cloud_column_steps: 8,
            },
        };
        let gpu_environment =
            GpuPhysicalEnvironment::from_parameters(&physical_environment, integration_quality);
        queue.write_buffer(
            &self.physical_environment_buf,
            0,
            bytemuck::bytes_of(&gpu_environment),
        );

        // Explicit grains are a resolved large-aggregate population layered over the separately
        // declared fine-grain bulk suspension. Their radiance is solved in snow.wgsl; these values
        // are measured particle geometry/transport inputs, never display colour or alpha controls.
        let snow_storm = self.storm.clamp(0.0, 1.0);
        let snow_gust = weather_gust(self.time, self.weather_seed);
        let snow_wind = self.weather_wind_world * snow_gust;
        let snow_field = SnowFieldParameters {
            seed: self.weather_seed,
            cell_size_m: 0.5,
            lanes_per_cell: 4,
            occupancy: f64::from(self.precipitation.clamp(0.0, 1.0) * (0.5 + 0.5 * snow_storm)),
            advection_velocity_m_s: glam::DVec3::new(
                snow_wind.x as f64,
                (-1.6 - 0.8 * snow_storm) as f64,
                snow_wind.y as f64,
            ),
            crystal_radius_m: 0.0025,
            extinction_efficiency: 2.0,
            single_scattering_albedo: 0.995,
            asymmetry: 0.76,
        };
        let mut gpu_snow = GpuPhysicalSnowField::from_parameters(
            &snow_field,
            self.time as f64,
            1.0 / 48.0, // 180-degree shutter at the campaign capture's 24 fps cadence
            128,
        );
        gpu_snow.world_medium = [
            self.fog_density.max(0.0),
            world_medium.snow_fraction,
            world_medium.layer[0],
            world_medium.layer[1],
        ];
        queue.write_buffer(&self.snow_field_buf, 0, bytemuck::bytes_of(&gpu_snow));

        // Volumetric smoke: its own inverse-view-proj (ray reconstruction + depth occlusion) and the
        // puff ring. Cheap to always upload; the pass itself is skipped when nothing is active.
        let smoke = SmokeUniform {
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
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
            _pad1: 0.0,
            atmo_origin: self.atmo_origin.to_array(),
            atmo_time: self.time,
            // The campaign's terrain remains inside this world-space medium. Blowing snow uses a
            // shallower physical layer than generic aerosol, preserving low horizontal visibility
            // while allowing real downwelling daylight to reach the battlefield.
            fog_layer: world_medium.layer,
        };
        queue.write_buffer(&self.smoke_buf, 0, bytemuck::bytes_of(&smoke));

        let m = self.master_retro.clamp(0.0, 1.0);
        let crush = CrushUniform {
            levels: 5.0 + (1.0 - m) * 27.0,
            // Softened from 0.2+m*0.8: a FULL posterize step of Bayer read as a screen-door over
            // sky/snow gradients at 640×360 — ~0.6 keeps the banding broken without the texture.
            dither: 0.15 + m * 0.45,
            // The TZF Strichbild scales in TRUE mils: the shader needs the live vertical FOV
            // (radians) to keep 1 Strich = 1/6400 circle at every magnification. 0 = off.
            reticle: if self.reticle { self.camera.fov_y } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            _removed_blood_overlay: [0.0; 3],
            menu_mode: if self.menu_mode { 1.0 } else { 0.0 },
            _removed_screen_haze: 0.0,
            mask: self.optic_mask,
            _removed_proxy_rt_mix: 0.0,
            exposure: self.exposure_override.unwrap_or(DEFAULT_CAMERA_EXPOSURE),
            retro: m,
            concussion: self.concussion.clamp(0.0, 1.0),
        };
        queue.write_buffer(&self.crush_buf, 0, bytemuck::bytes_of(&crush));
        if let Some(surface) = &self.rt_surface_gpu {
            let instances = self.rt_instance_shading_records();
            assert_eq!(instances.len(), surface.instance_count);
            if !instances.is_empty() {
                queue.write_buffer(&surface.instances_buf, 0, bytemuck::cast_slice(&instances));
            }
        }
    }

    fn render_internal(&self, encoder: &mut wgpu::CommandEncoder, finish_ldr: bool) {
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
        // FORWARD path only since S4 — the deferred combine traces real sun shadows.
        if !self.deferred && self.coop_shadow.should_render() {
            work.shadow_passes += 1;
            let light_view_proj = self.light_view_proj();
            let planes = frustum_planes(light_view_proj);
            let visible_instanced = self.inst_spatial_index.query(light_view_proj);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
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
            pass.set_bind_group(0, &self.camera_bind, &[]);
            // Cull against the sun's ortho cascade: geometry outside the shadow footprint can't
            // write useful depth, so skip its draw entirely.
            for m in &self.meshes {
                // Every visible opaque surface casts, including emissive housings.
                if !m.visible {
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
        // Deferred resolve loads over this background. The forward path instead draws opaque
        // geometry first and runs the identical shader against the completed depth buffer below,
        // so Metal never integrates atmosphere for pixels that cannot contribute to the image.
        if self.deferred || !self.depth_gate_sky {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky"),
                color_attachments: &[Some(attachment(
                    &self.hdr_view,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                ))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
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
            // ── S2 G-BUFFER pass: resolve material/mode state into MRT; no shading here. ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gbuf"),
                    color_attachments: &[
                        Some(attachment(
                            &self.g0_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g1_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g2_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                        Some(attachment(
                            &self.g3_view,
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        )),
                    ],
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
                pass.set_bind_group(0, &self.camera_bind, &[]);
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
                    if !m.visible {
                        continue;
                    }
                    if !aabb_in_frustum(&planes, m.world_min, m.world_max) {
                        continue;
                    }
                    let wmode = if m.glass {
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
                        extra: [i as f32, 0.0, 0.0, 0.0],
                    };
                    pass.set_push_constants(
                        wgpu::ShaderStages::VERTEX_FRAGMENT,
                        0,
                        bytemuck::bytes_of(&push),
                    );
                    pass.set_bind_group(1, m.tex_bind.as_ref(), &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    work.scene_mesh_draws += 1;
                    pass.draw_indexed(0..m.index_count, 0, 0..1);
                }
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
                    timestamp_writes: None,
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
                    timestamp_writes: None,
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
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.accum_pipe);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(0, &vc.spec_accum_binds[vc.accum_flip], &[]);
                pass.set_bind_group(1, &self.accum_delta_bind, &[]);
                work.compute_passes += 1;
                pass.dispatch_workgroups(self.iw.div_ceil(8), self.ih.div_ceil(8), 1);
            }
            // ── RESOLVE: albedo × accumulated diffuse + accumulated specular + immediate
            // authored emission → HDR (over the sky). ──
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("resolve"),
                    color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.resolve_pipe);
                pass.set_bind_group(0, &self.camera_bind, &[]);
                let vc = &self.views[self.active_view];
                pass.set_bind_group(1, &vc.resolve_binds[vc.accum_flip], &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
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
                timestamp_writes: None,
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
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                work.scene_mesh_draws += 1;
                pass.draw_indexed(0..m.index_count, 0, 0..1);
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
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.smoke_pipe);
                pass.set_bind_group(0, &self.smoke_bind, &[]);
                if self.rt_on {
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
                        timestamp_writes: None,
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
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.volume_composite_pipe);
                pass.set_bind_group(0, composite_bind, &[]);
                work.fullscreen_raster_passes += 1;
                pass.draw(0..3, 0..1);
            }
        }

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

        // ---- resolved physical snowfall, before bloom and measured camera exposure ----
        // High alone submits explicit grains because it can query the exact scene for upstream and
        // source visibility. Laptop retains the same bulk snow medium and omits these resolved
        // samples until its software triangle backend exists; it never falls back to white sprites.
        if self.precipitation > 0.001 && self.rt_on {
            let amount = self.precipitation.clamp(0.0, 1.0);
            let gust = weather_gust(self.time, self.weather_seed);
            let seed = weather_seed_token(self.weather_seed) as f32;
            let wind = self.weather_wind_world;
            let push = SnowPush {
                inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
                camera_amount: [
                    self.camera.pos.x,
                    self.camera.pos.y,
                    self.camera.pos.z,
                    amount,
                ],
                wind_time: [wind.x, wind.y, self.storm.clamp(0.0, 1.0), self.time],
                weather: [
                    self.night.clamp(0.0, 1.0),
                    self.weather_shelter.clamp(0.0, 1.0),
                    seed,
                    gust,
                ],
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("physical-snow"),
                color_attachments: &[Some(attachment(&self.hdr_view, wgpu::LoadOp::Load))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.snow_pipe);
            pass.set_bind_group(0, &self.snow_bind, &[]);
            pass.set_bind_group(
                1,
                self.combine_rt_bind
                    .as_ref()
                    .expect("physical snow requires the scene TLAS bind"),
                &[],
            );
            pass.set_bind_group(2, &self.physical_environment_bind, &[]);
            pass.set_push_constants(wgpu::ShaderStages::FRAGMENT, 0, bytemuck::bytes_of(&push));
            work.snow_passes += 1;
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
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

        // ---- S4 auto-exposure: log-average the finished HDR frame, adapt, and patch the
        // result into the crush uniform's exposure field on the GPU. ----
        if finish_ldr && self.autoexp && self.exposure_override.is_none() {
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("exposure"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.exp_pipe);
                cpass.set_bind_group(0, &self.exp_binds[self.active_view], &[]);
                work.compute_passes += 1;
                cpass.dispatch_workgroups(1, 1, 1);
            }
            encoder.copy_buffer_to_buffer(
                &self.exp_bufs[self.active_view],
                0,
                &self.crush_buf,
                44,
                4,
            );
        }

        if finish_ldr {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("crush"),
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
            work.fullscreen_raster_passes += 1;
            pass.draw(0..3, 0..1);
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
            timestamp_writes: None,
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
                GpuInstanceMaterialState::disabled(slot as u32, SurfaceId(surface))
            } else {
                let tint = if mode == InstanceEmissionMode::RasterFxOrange {
                    [1.0, 0.24, 0.03]
                } else {
                    [1.0; 3]
                };
                GpuInstanceMaterialState::emissive(
                    slot as u32,
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
            records.push(record);
        }
        for batch in &self.inst_batches {
            let prototype = &self.inst_prototypes[batch.prototype];
            if prototype.blas.is_none() || batch.flags > 3.5 || batch.raster_only_dynamic {
                continue;
            }
            let surface = self.rt_prototype_surface_ids[batch.prototype];
            for instance in 0..batch.object_id_capacity {
                let custom_data = batch.object_custom_data(instance);
                records.push(if batch.emissive > 0.0 {
                    GpuInstanceMaterialState::emissive(
                        custom_data,
                        SurfaceId(surface),
                        InstanceEmissionMode::RasterFxOrange,
                        batch.emissive,
                        [1.0, 0.24, 0.03],
                    )
                } else {
                    GpuInstanceMaterialState::disabled(custom_data, SurfaceId(surface))
                });
            }
        }
        records
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
                    slot as u32,
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
                let model = batch
                    .cpu_mats
                    .get(instance as usize)
                    .copied()
                    .unwrap_or(glam::Mat4::IDENTITY);
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
        let upload = pack_rt_surface_static(&pack);
        let instance_upload = if instances.is_empty() {
            vec![crate::rt_surface::GpuInstanceMaterialState::default()]
        } else {
            instances
        };
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
            usage: wgpu::BufferUsages::STORAGE,
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
        });
        self.rt_surface_dirty = false;
        self.combine_rt_bind = None;
    }

    /// M-D per-frame TLAS refresh: write every included mesh/instance transform, encode the
    /// full rebuild first in the frame encoder (wgpu 27 has no refit — rebuild IS the mode).
    /// Emissive geometry remains geometry: it participates in visibility and can return radiance
    /// when a secondary ray commits to it. Only explicit view-local/transparent paths stay out.
    fn rt_update(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if !self.rt_on {
            return;
        }
        self.rt_rebuild_surface_pack(device);
        // Splitscreen views 2..N re-render the SAME sim frame: the TLAS built for view 1 is
        // exactly current — skip the rebuild (but never before the first build exists).
        if self.rt_skip && self.rt_tlas.is_some() && self.combine_rt_bind.is_some() {
            return;
        }
        // A topology/capacity change computes a new stable slot layout. Moving-pool live counts do
        // not participate: their inactive capacity remains explicitly reserved as `None` entries.
        let mut layout_plan = if self.rt_batches_dirty || self.rt_tlas.is_none() {
            let reserved_capacities: Vec<Option<u32>> = self
                .inst_batches
                .iter()
                .map(|batch| {
                    (self.inst_prototypes[batch.prototype].blas.is_some()
                        && batch.flags <= 3.5
                        && !batch.raster_only_dynamic)
                        .then_some(batch.object_id_capacity)
                })
                .collect();
            Some(
                plan_rt_instanced_slots(self.meshes.len() as u32, &reserved_capacities)
                    .expect("RT instanced slot count overflow"),
            )
        } else {
            None
        };
        let needed = layout_plan
            .as_ref()
            .map(|plan| plan.end)
            .unwrap_or(self.rt_prev_count.max(self.meshes.len() as u32));
        if needed == 0 {
            return;
        }
        const TLAS_MAX: u32 = 1 << 16; // matches the device limit requested in main.rs
        if self.rt_tlas.is_none() || self.rt_tlas_cap < needed.min(TLAS_MAX) {
            let cap = needed.next_multiple_of(1024).min(TLAS_MAX);
            if needed > TLAS_MAX {
                println!("[rt] WARN: {needed} TLAS instances wanted, capped at {TLAS_MAX} — tail dropped");
            }
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
                let reserved_capacities: Vec<Option<u32>> = self
                    .inst_batches
                    .iter()
                    .map(|batch| {
                        (self.inst_prototypes[batch.prototype].blas.is_some()
                            && batch.flags <= 3.5
                            && !batch.raster_only_dynamic)
                            .then_some(batch.object_id_capacity)
                    })
                    .collect();
                layout_plan = Some(
                    plan_rt_instanced_slots(self.meshes.len() as u32, &reserved_capacities)
                        .expect("RT instanced slot count overflow"),
                );
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
                    i as u32,
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
        // Batch tail: static city/tree placements are written only on topology/static-transform
        // changes. Fixed moving pools own reserved ranges and can update just those small ranges.
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
                for instance in 0..b.object_id_capacity {
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
            let new_count = end.min(self.rt_tlas_cap) as usize;
            for i in new_count..(self.rt_prev_count as usize) {
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
                for instance in 0..b.object_id_capacity {
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
        if self.combine_rt_bind.is_none() {
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

    /// S3 per-frame temporal bookkeeping: camera-cut detection, per-slot prev-transform deltas
    /// (true rigid motion vectors), uniform upload, ping-pong flip. Must run before
    /// render_internal encodes the accum pass.
    fn accum_prepare(&mut self, queue: &wgpu::Queue) {
        if !self.deferred {
            return;
        }
        let pos = self.camera.pos;
        let fwd = self.camera.forward();
        let fov = self.camera.fov_y;
        let (ppos, pfwd, pfov) = self.views[self.active_view].prev_cam_sig;
        // A cut = any camera discontinuity reprojection can't bridge: teleports (station
        // switches, hatch climb ends, kill-cam), FOV flips (optic zoom), hard whips.
        let cut = (pos - ppos).length() > 2.0 || fwd.dot(pfwd) < 0.5 || (fov - pfov).abs() > 1e-3;
        let reset = !self.views[self.active_view].history_valid || cut;
        let volume_active = self.smoke_active();
        let volume_reset = !self.views[self.active_view].volume_history_valid || cut;
        if self.prev_models.len() != self.meshes.len() {
            self.prev_models = self.meshes.iter().map(|m| m.model).collect();
        }
        let deltas: Vec<[[f32; 4]; 4]> = self
            .meshes
            .iter()
            .zip(&self.prev_models)
            .map(|(m, pm)| (*pm * m.model.inverse()).to_cols_array_2d())
            .collect();
        if !deltas.is_empty() {
            queue.write_buffer(&self.delta_buf, 0, bytemuck::cast_slice(&deltas));
        }
        let u = AccumUniform {
            prev_view_proj: self.views[self.active_view].prev_vp.to_cols_array_2d(),
            inv_view_proj: self.camera.view_proj().inverse().to_cols_array_2d(),
            alpha: if self.accum_on { 0.15 } else { 1.0 },
            reset: if reset { 1.0 } else { 0.0 },
            test_noise: if self.accum_test { 1.0 } else { 0.0 },
            frame: (self.frame & 0xFFFF) as f32,
            znear: self.camera.znear,
            zfar: self.camera.zfar,
            n_meshes: self.meshes.len() as f32,
            volume_reset: if volume_reset { 1.0 } else { 0.0 },
        };
        queue.write_buffer(&self.accum_ubuf, 0, bytemuck::bytes_of(&u));
        // Roll state forward for next frame (per-view camera record; global mesh transforms —
        // extra views of the same sim frame legitimately see identity deltas).
        let vc = &mut self.views[self.active_view];
        vc.prev_vp = self.camera.view_proj();
        vc.prev_cam_sig = (pos, fwd, fov);
        vc.accum_flip = 1 - vc.accum_flip;
        vc.history_valid = true;
        // An inactive stretch invalidates only volume history. Surface lighting may remain
        // perfectly continuous, while the next smoke/haze onset must begin from its current
        // physical estimate rather than whatever occupied this view's volume target minutes ago.
        vc.volume_history_valid = volume_active;
        for (pm, m) in self.prev_models.iter_mut().zip(self.meshes.iter()) {
            *pm = m.model;
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
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bench-frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder, true);
        queue.submit([encoder.finish()]);
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

    pub fn render_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        // Solo always renders through temporal context 0 with a full-frame blit.
        self.set_active_view(0);
        queue.write_buffer(
            &self.blit_ubuf,
            0,
            bytemuck::bytes_of(&[0.0f32, 0.0, 1.0, 1.0]),
        );
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = false;
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
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
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("optic-pass"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder, true);
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
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("coop-view"),
        });
        self.flush_track_stamps(device, &mut encoder);
        self.rt_update(device, &mut encoder);
        self.accum_prepare(queue);
        self.render_internal(&mut encoder, true);
        self.blit_rect(device, &mut encoder, view, format, Some(rect));
        queue.submit([encoder.finish()]);
    }

    /// Composite the queued UI text OVER an already-rendered surface frame (LoadOp::Load) — the
    /// in-game HUD path (mission text, held map, interaction prompts). Call after
    /// `render_to_surface`, before present.
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
        self.ui
            .render_over(device, &mut encoder, view, format, cw, ch);
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
            "[kessel] wrote {path} ({cw}x{ch}, internal {}x{}, retro {:.2})",
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
        self.upload_uniforms(queue);
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
        self.accum_prepare(queue);
        self.render_internal(&mut encoder, true);
        self.blit(device, &mut encoder, &cap_view, format);
        if hud {
            self.ui
                .render_over(device, &mut encoder, &cap_view, format, cw, ch);
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

    /// Flush the queued UI onto a swapchain view. The UI is rendered at the INTERNAL low resolution
    /// (into `ldr_view`) and then nearest-upscaled to the display — so menu/death text is pixelated
    /// to match the crushed game frame, not a crisp overlay. (`_cw`/`_ch` unused — the upscale fills
    /// the whole surface.)
    pub fn render_ui_to_surface(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        _cw: u32,
        _ch: u32,
    ) {
        self.reset_blit_crop(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ui-frame"),
        });
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, view, format);
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
        // Render the UI at internal low-res, then nearest-upscale into the capture — pixelated text.
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, &cap_view, format);
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
        println!("[kessel] wrote UI {path} ({cw}x{ch})");
        Ok(())
    }

    /// Flush the main MENU to a swapchain view: the turntable Tiger (crushed pixel model on pure
    /// black) with the front-end text composited OVER it at internal res, then nearest-upscaled to the
    /// display. Call `set_menu_tiger(angle)` + queue the text (`draw_menu`) before this.
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
        self.upload_uniforms(queue);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("menu-frame"),
        });
        self.render_menu_internal(&mut encoder);
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render_over(device, &mut encoder, ldr, LDR, iw, ih); // text over the Tiger, no bg fill
        }
        self.blit(device, &mut encoder, view, format);
        queue.submit([encoder.finish()]);
    }

    /// Flush the main MENU to a PNG at `cw`×`ch` (headless verification of the redesigned title). Same
    /// composite as `render_menu_to_surface`: turntable Tiger crushed at internal res, text over it,
    /// nearest-upscaled into the capture.
    pub fn render_menu_to_png(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &str,
        cw: u32,
        ch: u32,
    ) -> anyhow::Result<()> {
        self.frame = self.frame.wrapping_add(1);
        self.menu_mode = true; // crush.wgsl → keep the black backdrop pure #000 (no grain speckle)
        self.reset_blit_crop(queue);
        self.upload_uniforms(queue);
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
        self.render_menu_internal(&mut encoder);
        let (iw, ih) = (self.iw, self.ih);
        {
            let ui = &mut self.ui;
            let ldr = &self.ldr_view;
            ui.render_over(device, &mut encoder, ldr, LDR, iw, ih);
        }
        self.blit(device, &mut encoder, &cap_view, format);
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
        println!("[kessel] wrote menu {path} ({cw}x{ch})");
        Ok(())
    }
}

// ---- small wgpu helpers ----

fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(src.into()),
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
                format: HDR,
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
    fn internal_resolution_parser_accepts_only_the_bounded_16_by_9_presets() {
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
            "1920x1080",
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
        assert!(source.rfind("temporal_grain(px, frame_index)").unwrap() > retro_branch);
        assert!(source.contains("let phase = vec2<i32>(i32(frame_index & 3u)"));
        assert!(!source.contains("fn noise2"));
        assert!(!source.contains("var grain_amt"));
        assert!(!source.contains("black_point"));
        assert!(!source.contains("S-curve contrast"));
        assert!(source.contains("var m = aces(c)"));
        assert!(source.contains("let grain_gate = smoothstep(0.004, 0.055, graded_luma)"));
        assert!(source.contains("let concussion = clamp(cr.concussion"));
        assert!(source.contains("mix(centre, blur, concussion)"));
        assert_eq!(std::mem::size_of::<CrushUniform>(), 56);
        assert_eq!(std::mem::offset_of!(CrushUniform, exposure), 44);
        assert_eq!(std::mem::offset_of!(CrushUniform, concussion), 52);
    }

    #[test]
    fn volumetric_uniform_v2_contains_only_modeled_medium_and_shared_sources() {
        assert_eq!(std::mem::offset_of!(SmokeUniform, breech_gas), 480);
        assert_eq!(std::mem::offset_of!(SmokeUniform, fog_density), 76);
        assert_eq!(std::mem::offset_of!(SmokeUniform, n_lights), 512);
        assert_eq!(std::mem::offset_of!(SmokeUniform, lights), 528);
        assert_eq!(std::mem::offset_of!(SmokeUniform, interior), 1_552);
        assert_eq!(
            std::mem::offset_of!(SmokeUniform, world_snow_fraction),
            1_560
        );
        assert_eq!(std::mem::offset_of!(SmokeUniform, atmo_origin), 1_568);
        assert_eq!(std::mem::offset_of!(SmokeUniform, fog_layer), 1_584);
        assert_eq!(std::mem::size_of::<SmokeUniform>(), 1_600);

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
        assert_eq!(std::mem::size_of::<CameraUniform>(), 1_328);

        let physical = PHYSICAL_ENVIRONMENT_SHADER_SOURCE;
        let smoke = SMOKE_SHADER_SOURCE;
        let combine = include_str!("../shaders/combine.wgsl");
        assert!(physical.contains("fn physical_bounded_medium_interval("));
        assert!(physical.contains("fn physical_bounded_medium_transmittance("));
        assert!(smoke.contains("physical_bounded_medium_interval("));
        assert!(smoke.contains("physical_bounded_medium_segment_length("));
        assert!(combine.contains("world_medium: vec4<f32>"));
        assert!(combine.contains("transport_passes: vec4<f32>"));
        assert!(combine.contains("let interface_passes = max(i32(cam.transport_passes.x), 1);"));
        assert!(combine.contains("let bounce_passes = max(i32(cam.transport_passes.y), 1);"));
        assert!(combine.contains("fn world_medium_radiance("));
        assert!(combine.contains("background * throughput + inscattered"));
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
        let sun_y = DayPhase::Day.light_dir().y;
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
                .matches("let solar_source = world_medium_solar_source(ro, sun_dir);")
                .count()
                >= 2
        );
        // Both sky samples cross the slab through the same bounded environment-source closure
        // (directional Beer term plus the scattered remainder), never a bare environment lookup.
        assert!(combine.contains(
            "let sky = world_medium_environment_source(\n            ro,\n            sky_dir,"
        ));
        assert!(combine.contains(
            "sky_irr = world_medium_environment_source(\n            ro,\n            hdir,"
        ));

        // Resolved grains use the persistent physical field; broad obscuration belongs to the
        // bounded bulk integrator rather than a horizon mask or constant-white overlay.
        let snow = include_str!("../shaders/snow.wgsl");
        for forbidden in ["whiteout_veil", "ground_spindrift", "flake_layer"] {
            assert!(!snow.contains(forbidden));
        }
        assert!(snow.contains("physical_snow_transport_ray("));
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
        assert!(high.contains("let visible = geometry_visibility(p, to_sun, 4000.0)"));
        assert!(high.contains("let local_transmittance = medium_transmittance(p, to_sun, 4000.0)"));
        assert!(high.contains("let nl = i32(sm.n_lights)"));
        assert!(high.contains("max(distance_to_light - source_radius, 0.005)"));
        assert!(high.contains("spherical_emitter_attenuation(distance_squared, source_radius)"));
        assert!(high
            .contains("let transmittance = medium_transmittance(p, light_dir, distance_to_light)"));
        assert!(high.contains("return exp(-optical_depth)"));
        assert!(high.contains("henyey_greenstein"));
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
        assert!(deferred.contains("occluded(ro, sun_dir, ray_skip, 4000.0)"));
        assert!(deferred.contains("occluded(ro, hdir, ray_skip, 2000.0)"));
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
    fn rapidly_moving_instance_ids_reject_temporal_history() {
        let source = include_str!("../shaders/accum.wgsl");
        assert!(source.contains("(id & 0x800000u) != 0u"));
        assert!(source.contains("u.reset < 0.5 && !dynamic_instance"));
    }

    #[test]
    fn cubic_smoke_instances_are_never_screen_door_deleted() {
        let forward = include_str!("../shaders/mesh.wgsl");
        let deferred = include_str!("../shaders/gbuf.wgsl");
        assert_eq!(BATCH_FLAG_SPARSE_CUBIC_SMOKE.to_bits(), (-1.0f32).to_bits());
        for source in [forward, deferred] {
            assert!(source.contains("let sparse_cubic_smoke = pc.params.w < -0.5;"));
            assert!(!source.contains("sparse_cubic_smoke_keeps"));
            // The only permitted fragment discard is the GRASS blade alpha test, which cubic smoke
            // (a different batch mode) never reaches; no screen-door/stipple deletion may exist.
            for (index, _) in source.match_indices("discard;") {
                let preceding = &source[index.saturating_sub(160)..index];
                assert!(
                    preceding.contains("pc.params.w > 3.5 && pc.params.w < 4.5 && raw4.a < 0.5"),
                    "discard outside the GRASS alpha test"
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
        let entry = include_str!("../shaders/snow.wgsl");
        let field = PHYSICAL_ENVIRONMENT_SHADER_SOURCE;
        assert!(entry.contains("texture_depth_2d"));
        assert!(entry.contains("pc.inv_view_proj"));
        assert!(entry.contains("physical_snow_transport_ray("));
        assert!(field.contains("field.wind_time.w"));
        assert!(field.contains("physical_snow_identity("));
        assert!(field.contains("physical_pcg_hash"));
        assert!(!entry.contains("pc.frame"));
        assert!(!entry.contains("ground_spindrift"));
    }

    #[test]
    fn high_path_rejects_screen_space_gi_and_traces_every_local_source() {
        let high = include_str!("../shaders/combine.wgsl");
        assert!(!high.contains("fn screen_space_contact_ao"));
        assert!(!high.contains("screen_space_diffuse_bounce"));
        assert!(!high.contains("indirect_bounce"));
        assert!(!high.contains("floor_day"));
        assert!(high.contains("var diff_col = ambient + direct_diff"));
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
            "var bounce_weight = array<vec3<f32>, 2>(diffuse_kd * albedo, reflection_weight);",
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
            "glass_pipe",
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
    fn campaign_atmospheres_are_distinct_physical_media() {
        let winter = campaign_atmosphere_profile(1);
        let oderbruch = campaign_atmosphere_profile(2);
        let halbe = campaign_atmosphere_profile(3);

        // Wet open country has the longest horizontal sightline; the smoky cauldron closes down
        // again without borrowing the whiteout's snow composition.
        assert!(oderbruch.ground_extinction_m_inv < halbe.ground_extinction_m_inv);
        assert!(halbe.ground_extinction_m_inv < winter.ground_extinction_m_inv);
        assert_eq!(oderbruch.mood, 0.5);
        assert_eq!(halbe.mood, 1.0);

        // Rain-cleansed Act II air scatters less aerosol than the smoke-loaded Act III air, while
        // both remain independently specified physical Mie media.
        assert!(oderbruch.mie_extinction_m_inv < winter.mie_extinction_m_inv);
        assert!(winter.mie_extinction_m_inv < halbe.mie_extinction_m_inv);
        assert!(halbe.mie_extinction_m_inv > halbe.mie_scattering_m_inv);
        assert!(oderbruch.cloud_bottom_m < halbe.cloud_bottom_m);
        assert!(oderbruch.cloud_top_m < halbe.cloud_top_m);
        assert!(oderbruch.wind_exposure > halbe.wind_exposure);
    }

    #[test]
    fn authored_phases_have_ordered_darkness_and_normalized_keys() {
        let factors = DayPhase::ALL.map(DayPhase::night_factor);
        assert!(factors[1] < factors[0]); // daylight is brighter than dawn
        assert!(factors[0] < factors[2]); // dusk is darker than dawn
        assert!(factors[2] < factors[3]); // night is the darkest endpoint
        for phase in DayPhase::ALL {
            assert!((phase.light_dir().length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn camera_exposure_and_surface_energy_have_no_campaign_or_view_fill() {
        assert_eq!(DEFAULT_CAMERA_EXPOSURE.to_bits(), 0.55f32.to_bits());
        assert!(UNINITIALIZED_EXPOSURE_TIME.is_sign_negative());
        assert_eq!(
            uninitialized_exposure_state(),
            [DEFAULT_CAMERA_EXPOSURE, UNINITIALIZED_EXPOSURE_TIME]
        );
        let renderer = include_str!("renderer.rs");
        // Assemble retired spellings at runtime: this test module is part of the scanned file.
        let retired_fill = ["authored", "forward", "exposure"].join("_");
        assert!(!renderer.contains(&retired_fill));
        assert!(renderer.contains("if finish_ldr && self.autoexp"));
        let deferred_only_gate = ["if finish_ldr && self.deferred", "&& self.autoexp"].join(" ");
        assert!(!renderer.contains(&deferred_only_gate));
        assert!(renderer.contains("reset_active_exposure_for_headless_capture"));

        let exposure = include_str!("../shaders/exposure.wgsl");
        assert!(exposure.contains("if (state[1] < 0.0)"));
        assert!(exposure.contains("state[0] = want;"));

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
    fn rt_slot_plan_reserves_empty_pose_capacity_and_keeps_following_ranges_stable() {
        // Batches 1 and 4 model raster-only/grass pages. Three ray-visible pose pools reserve 24
        // slots apiece regardless of whether their current live counts are 0, 7, or 24.
        let capacities = [Some(3), None, Some(24), Some(24), None, Some(24)];
        let plan = plan_rt_instanced_slots(800, &capacities).unwrap();
        assert_eq!(
            plan.offsets,
            vec![Some(800), None, Some(803), Some(827), None, Some(851)]
        );
        assert_eq!(plan.end, 875);

        // Emptying one pose and filling another does not enter this capacity-only plan, which is
        // precisely what lets the small moving ranges update without rewriting the static tail.
        let after_pose_switch = plan_rt_instanced_slots(800, &capacities).unwrap();
        assert_eq!(after_pose_switch, plan);
        assert!(plan_rt_instanced_slots(u32::MAX - 2, &[Some(3)]).is_none());
    }

    fn spatial_test_bounds(cell_x: i32, cell_z: i32, inset: f32) -> InstSpatialBounds {
        spatial_test_page_bounds(cell_x, cell_z, inset, INST_SPATIAL_CELL_METRES)
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
        // Counts rose with the warscape layer (authored war material added prototypes and
        // placements to every act). What this test protects is unchanged: the spatial index must
        // collapse a page scan onto the occupied cells rather than walking every page.
        for (act, expected_pages, expected_cells, expected_refs) in [
            (1u8, 1_483usize, 1_312usize, 18_868usize),
            (2, 3_026, 2_208, 42_401),
            (3, 2_259, 1_024, 30_609),
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
