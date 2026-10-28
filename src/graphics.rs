//! Session-local graphics quality selection.
//!
//! This deliberately never enters LAN snapshots or campaign state: each process renders the same
//! simulation at the quality its own machine can afford.  The active value is thread-local so the
//! existing headless verification commands and parallel unit tests do not share mutable process
//! state.  Live play reapplies it immediately before every world/renderer build.

use std::cell::Cell;

/// The Studio Display playtest uses one authored profile. The desktop surface still comes from
/// the monitor; these dimensions describe only the game's native pixel grid.
pub const FIXED_PLAYTEST_RESOLUTION: (u32, u32) = (800, 450);

pub fn fixed_playtest_graphics() -> bool {
    std::env::var("KESSEL_FIXED_PLAYTEST_GRAPHICS").is_ok_and(|value| value == "1")
}

/// Separate rendering architectures, independent of the scene/detail preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererBackend { Physical, Shader, Raymarch }

impl RendererBackend {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "physical" | "legacy" => Some(Self::Physical),
            "shader" | "raster" => Some(Self::Shader),
            "raymarch" | "rm" => Some(Self::Raymarch),
            _ => None,
        }
    }
}

pub fn renderer_backend() -> RendererBackend {
    if fixed_playtest_graphics() { return RendererBackend::Shader; }
    match std::env::var("KESSEL_RENDERER") {
        Ok(value) => RendererBackend::parse(&value)
            .unwrap_or_else(|| panic!("Unknown KESSEL_RENDERER={value:?}; use shader, raymarch or physical")),
        Err(_) => RendererBackend::Physical,
    }
}

/// True for the raster-fed backends (shader and raymarch): the game then supplies the
/// simulation-owned grains, analytic weapon records and raster optics instead of RT payloads.
pub fn shader_backend_enabled() -> bool {
    matches!(renderer_backend(), RendererBackend::Shader | RendererBackend::Raymarch)
}
/// The raymarch frame pipeline (src/renderer_raymarch.rs) replaces the shader backend's passes.
pub fn raymarch_backend_enabled() -> bool { renderer_backend() == RendererBackend::Raymarch }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsPreset {
    /// Gaming-laptop budget: fewer froxel slices, smaller shadow and sky maps, one shadowed lamp,
    /// gameplay transport sampling and no hardware ray queries. Never changes the scene or the
    /// simulation, only how finely the same frame is sampled.
    Low,
    /// The desktop reference (RTX 4080/4090 class). The previous "4090" preset, unchanged.
    High,
    /// Headroom on a 4090/5090: denser froxels, 4K sun shadows, finer sky cache and clouds, more
    /// samples per pixel and an 800x450 grid.
    Ultra,
}

/// The raymarch backend's per-preset budgets (src/renderer_raymarch.rs reads them once when the
/// backend is built; a preset change rebuilds the renderer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaymarchQuality {
    /// Froxel grid: internal pixels per froxel column, the smallest grid, and slices in depth.
    pub vol_divisor: u32,
    pub vol_min: [u32; 2],
    pub vol_slices: u32,
    /// Sun shadow cascades (far, near) and the cloud-shadow map, texels per side.
    pub shadow_far_res: u32,
    pub shadow_near_res: u32,
    pub cloud_shadow_res: u32,
    /// Point lights given a shadow cube (at most 2) and each cube face's texels per side.
    pub point_shadows: usize,
    pub point_face_res: u32,
    /// Sky cache width (its height is 3/8 of it) and the cloud march steps baked into it.
    pub sky_cache_width: u32,
    pub cloud_view_steps: u32,
    pub cloud_light_steps: u32,
    /// Per-pixel sample budget: `budget_pixels / (iw*ih)`, clamped to 1..=`max_samples`.
    pub budget_pixels: f32,
    pub max_samples: f32,
}

impl RaymarchQuality {
    pub fn sample_budget(&self, iw: u32, ih: u32) -> f32 {
        (self.budget_pixels / (iw.max(1) * ih.max(1)) as f32).clamp(1.0, self.max_samples)
    }
}

impl GraphicsPreset {
    pub const ALL: [Self; 3] = [Self::Low, Self::High, Self::Ultra];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// The menu's graphics entry cycles Low -> High -> Ultra -> Low.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Low => Self::High,
            Self::High => Self::Ultra,
            Self::Ultra => Self::Low,
        }
    }

    pub const fn internal_resolution(self) -> (u32, u32) {
        match self {
            Self::Low => (320, 180),
            Self::High => (640, 360),
            Self::Ultra => (800, 450),
        }
    }

    /// The authored terrain's render grid: its collision grid on every preset (the terrain is an
    /// occluder; `terrain::authored_render_res`). Before 2026-09-29 the laptop drew 256² and the
    /// 4090 512² of a 1024² grid.
    pub const fn authored_terrain_grid(self) -> &'static str {
        match self {
            Self::Low | Self::High | Self::Ultra => "collision grid",
        }
    }

    pub const fn shadow_resolution(self) -> u32 {
        match self {
            // At the 320x180 Low grid, a 512px map remains comfortably denser than the final
            // image. The renderer's 3x3 PCF masks the small texel increase.
            Self::Low => 512,
            Self::High => 2048,
            Self::Ultra => 4096,
        }
    }

    pub const fn allows_hardware_rt(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }

    /// Periscope and vision-block relay views re-rendered per frame (raster_optics.rs). Low keeps
    /// every visible pane drawn but refreshes the largest each frame and rotates one more
    /// through the rest, so a small side pane may lag a frame or three.
    pub const fn optic_relays_per_frame(self) -> usize {
        match self {
            Self::Low => 2,
            Self::High | Self::Ultra => usize::MAX,
        }
    }

    /// High and Ultra share the desktop budgets wherever a system has only two tiers.
    pub const fn is_desktop(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }

    pub const fn raymarch(self) -> RaymarchQuality {
        match self {
            Self::Low => RaymarchQuality {
                vol_divisor: 6, vol_min: [80, 45], vol_slices: 48,
                shadow_far_res: 1024, shadow_near_res: 512, cloud_shadow_res: 128,
                point_shadows: 1, point_face_res: 128,
                sky_cache_width: 384, cloud_view_steps: 12, cloud_light_steps: 3,
                budget_pixels: 115_200.0, max_samples: 2.0,
            },
            Self::High => RaymarchQuality {
                vol_divisor: 4, vol_min: [160, 90], vol_slices: 96,
                shadow_far_res: 2048, shadow_near_res: 1024, cloud_shadow_res: 256,
                point_shadows: 2, point_face_res: 256,
                sky_cache_width: 768, cloud_view_steps: 20, cloud_light_steps: 4,
                budget_pixels: 360_000.0, max_samples: 3.0,
            },
            Self::Ultra => RaymarchQuality {
                vol_divisor: 3, vol_min: [160, 90], vol_slices: 128,
                shadow_far_res: 4096, shadow_near_res: 2048, cloud_shadow_res: 512,
                point_shadows: 2, point_face_res: 512,
                sky_cache_width: 1024, cloud_view_steps: 32, cloud_light_steps: 5,
                budget_pixels: 720_000.0, max_samples: 3.0,
            },
        }
    }

    /// Integrated adapters default to Low.  Only an adapter explicitly reported as discrete
    /// gets High automatically; an Other/virtual/software device is treated conservatively.
    /// Ultra is always an explicit choice.  The menu choice can override this for the session.
    pub const fn for_adapter(device_type: wgpu::DeviceType) -> Self {
        match device_type {
            wgpu::DeviceType::DiscreteGpu => Self::High,
            _ => Self::Low,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" | "laptop" => Some(Self::Low),
            "high" | "4090" | "hero" => Some(Self::High),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }
}

thread_local! {
    // Headless graphics/RT validation historically meant the full renderer.  Live WindowState and
    // cpuprof explicitly replace this after adapter discovery, before constructing any world.
    static ACTIVE_PRESET: Cell<GraphicsPreset> = const { Cell::new(GraphicsPreset::High) };
}

pub fn set_active_preset(preset: GraphicsPreset) {
    ACTIVE_PRESET.with(|active| active.set(if fixed_playtest_graphics() {
        GraphicsPreset::High
    } else {
        preset
    }));
}

pub fn active_preset() -> GraphicsPreset {
    ACTIVE_PRESET.with(Cell::get)
}

/// The raymarch backend's ray queries (world-space occlusion and reflection fallbacks against the
/// scene TLAS): on every preset, used when the adapter supports them, vetoed by `KESSEL_NO_RT=1`
/// or `KESSEL_RM_RT=0`. Low keeps them: without ray queries this backend lights the hull interior
/// like open sky (commander gfxbench mean brightness 103 vs 30, 2026-10-28), and RTX laptop GPUs
/// support them. The physical backend's deferred ray integrator stays off on this backend.
pub fn raymarch_ray_queries_wanted() -> bool {
    raymarch_backend_enabled()
        && std::env::var("KESSEL_NO_RT").map(|value| value != "1").unwrap_or(true)
        && std::env::var("KESSEL_RM_RT").map(|value| value != "0").unwrap_or(true)
}

/// Existing `KESSEL_NO_RT=1` remains the authoritative kill switch even on the hero preset.
pub fn hardware_rt_enabled() -> bool {
    !shader_backend_enabled() && active_preset().allows_hardware_rt()
        && std::env::var("KESSEL_NO_RT")
            .map(|value| value != "1")
            .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_backend_is_explicit_and_distinct_from_detail_preset() {
        assert_eq!(RendererBackend::parse("shader"), Some(RendererBackend::Shader));
        assert_eq!(RendererBackend::parse("physical"), Some(RendererBackend::Physical));
        assert_eq!(RendererBackend::parse("high"), None);
        assert_eq!(RendererBackend::parse("unknown"), None);
    }

    #[test]
    fn presets_match_the_three_shipping_budgets() {
        assert_eq!(GraphicsPreset::Low.internal_resolution(), (320, 180));
        assert_eq!(GraphicsPreset::Low.authored_terrain_grid(), GraphicsPreset::High.authored_terrain_grid());
        assert_eq!(GraphicsPreset::Low.shadow_resolution(), 512);
        assert!(!GraphicsPreset::Low.allows_hardware_rt());

        assert_eq!(GraphicsPreset::High.internal_resolution(), (640, 360));
        assert_eq!(GraphicsPreset::High.authored_terrain_grid(), "collision grid");
        assert_eq!(GraphicsPreset::High.shadow_resolution(), 2048);
        assert!(GraphicsPreset::High.allows_hardware_rt());

        assert_eq!(GraphicsPreset::Ultra.internal_resolution(), (800, 450));
        assert!(GraphicsPreset::Ultra.allows_hardware_rt());
        assert!(GraphicsPreset::Ultra.shadow_resolution() > GraphicsPreset::High.shadow_resolution());
    }

    #[test]
    fn presets_parse_their_keys_and_the_old_names_and_cycle() {
        for preset in GraphicsPreset::ALL {
            assert_eq!(GraphicsPreset::parse(preset.key()), Some(preset));
            assert_eq!(GraphicsPreset::parse(preset.label()), Some(preset));
        }
        assert_eq!(GraphicsPreset::parse("laptop"), Some(GraphicsPreset::Low));
        assert_eq!(GraphicsPreset::parse("4090"), Some(GraphicsPreset::High));
        let mut p = GraphicsPreset::Low;
        for _ in 0..3 { p = p.toggled(); }
        assert_eq!(p, GraphicsPreset::Low);
    }

    #[test]
    fn raymarch_budgets_rise_monotonically_and_high_keeps_the_reference() {
        let [low, high, ultra] = GraphicsPreset::ALL.map(GraphicsPreset::raymarch);
        assert_eq!((high.vol_divisor, high.vol_slices, high.shadow_far_res, high.point_shadows), (4, 96, 2048, 2));
        assert_eq!(high.sample_budget(600, 600), 1.0);
        for (a, b) in [(low, high), (high, ultra)] {
            assert!(a.vol_slices < b.vol_slices && a.shadow_far_res < b.shadow_far_res);
            assert!(a.sky_cache_width < b.sky_cache_width && a.cloud_view_steps < b.cloud_view_steps);
            assert!(a.point_shadows <= b.point_shadows && b.point_shadows <= 2);
        }
    }

    #[test]
    fn only_discrete_adapters_default_high() {
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::DiscreteGpu),
            GraphicsPreset::High
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::IntegratedGpu),
            GraphicsPreset::Low
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::Cpu),
            GraphicsPreset::Low
        );
    }
}
