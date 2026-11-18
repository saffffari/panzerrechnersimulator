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

/// The player's quality choice (the owner, 18 November 2026: "low medium and high", Low for a
/// Raspberry Pi 5 at 30 to 60 fps, Medium for an Intel Core Ultra 5 laptop at 30 fps, High a flat
/// 60 fps). Every preset renders the same scene and runs the same simulation; they differ only in
/// the render grid and how finely the frame is sampled (CLAUDE.md, second principle, rule 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsPreset {
    /// Single-board and weakest GPUs (Raspberry Pi 5, VideoCore VII): the 320x180 grid, one
    /// sample per pixel, the smallest shadow maps and sky cache, one shadowed lamp at 64 texels a
    /// face, one relayed port refreshed per frame.
    Low,
    /// Integrated laptop graphics (Intel Core Ultra 5 with Arc graphics class): the 640x360 grid,
    /// fewer froxel slices, smaller shadow and sky maps, one shadowed lamp, gameplay transport
    /// sampling, two relayed ports refreshed per frame. Before 18 November 2026 this was "Low".
    Medium,
    /// The desktop reference (RTX 4080/4090 class) at the 1024x576 grid. Unchanged budgets.
    High,
    /// Explicit only (`KESSEL_GRAPHICS_PRESET=ultra`, review captures); not a menu choice.
    /// Denser froxels, 4K sun shadows, finer sky cache and clouds.
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
    /// One-pixel effect samples (weapon sparks and ejecta, fire and soot grains) per native pixel
    /// of a relayed port's view, the most each of its two effect passes draws (`relay_effect_budget`).
    pub relay_effect_samples_per_pixel: f32,
    /// Fire and soot grain samples per native pixel of a relayed port's view
    /// (`relay_grain_budget`): a port's native extent is already twice its displayed span, and a
    /// flame parcel covers thousands of a port's pixels, so the spark rate drew 3.7 million grains
    /// into the gunner's sight pane for one burning wreck (44 ms, 2026-11-01).
    pub relay_grain_samples_per_pixel: f32,
}

impl RaymarchQuality {
    /// `KESSEL_RM_SAMPLES=N` (1 to 16) sets the budget outright: N times the occlusion rays,
    /// horizon-gather directions (to 32) and rough-reflection rays (to 8) per pixel per frame.
    pub fn sample_budget(&self, iw: u32, ih: u32) -> f32 {
        static FORCED: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
        if let Some(forced) = *FORCED.get_or_init(|| std::env::var("KESSEL_RM_SAMPLES").ok()
            .and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).map(|v| v.clamp(1.0, 16.0))) {
            return forced;
        }
        (self.budget_pixels / (iw.max(1) * ih.max(1)) as f32).clamp(1.0, self.max_samples)
    }

    /// The one-pixel effect samples a relayed port of `extent` native pixels may draw per pass.
    /// `KESSEL_RELAY_EFFECT_SPP` overrides the preset's rate for A/B (a large value restores the
    /// passes' own caps).
    pub fn relay_effect_budget(&self, extent: [u32; 2]) -> u64 {
        static OVERRIDE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
        let rate = OVERRIDE.get_or_init(|| std::env::var("KESSEL_RELAY_EFFECT_SPP").ok().and_then(|v| v.parse().ok()))
            .unwrap_or(self.relay_effect_samples_per_pixel);
        (rate.max(1.0) as f64 * extent[0] as f64 * extent[1] as f64) as u64
    }

    /// The fire and soot grain samples a relayed port of `extent` native pixels may draw.
    /// `KESSEL_RELAY_GRAIN_SPP` overrides the preset's rate for A/B.
    pub fn relay_grain_budget(&self, extent: [u32; 2]) -> u64 {
        static OVERRIDE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
        let rate = OVERRIDE.get_or_init(|| std::env::var("KESSEL_RELAY_GRAIN_SPP").ok().and_then(|v| v.parse().ok()))
            .unwrap_or(self.relay_grain_samples_per_pixel);
        (rate.max(1.0) as f64 * extent[0] as f64 * extent[1] as f64) as u64
    }
}

impl GraphicsPreset {
    /// The menu's three choices, cheapest first. Ultra stays an explicit review setting.
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    /// The hardware each menu choice is sized for, shown beside it.
    pub const fn target(self) -> &'static str {
        match self {
            Self::Low => "Raspberry Pi 5 class",
            Self::Medium => "laptop graphics, 30 fps",
            Self::High => "desktop GPU, 60 fps",
            Self::Ultra => "review captures",
        }
    }

    /// The menu cycles Low -> Medium -> High -> Low (Ultra, set only explicitly, steps to Low).
    pub const fn toggled(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High | Self::Ultra => Self::Low,
        }
    }

    /// One step through the menu's choices in either direction.
    pub const fn stepped(self, up: bool) -> Self {
        if up { return self.toggled(); }
        match self {
            Self::Low => Self::High,
            Self::Medium => Self::Low,
            Self::High | Self::Ultra => Self::Medium,
        }
    }

    /// The grid a renderer is first built at when nothing else names one (headless tools, the
    /// title world before saved preferences apply). Live play uses `play_resolution`.
    pub const fn internal_resolution(self) -> (u32, u32) {
        match self {
            Self::Low => (320, 180),
            Self::Medium | Self::High => (640, 360),
            Self::Ultra => (800, 450),
        }
    }

    /// The render grid the menu's choice sets for live play. Each is a whole multiple of the
    /// common panels: 320x180 is 6x on 1920x1080 and 16x on 5120x2880, 640x360 is 3x and 8x,
    /// 1024x576 is 5x on 5120x2880 (the owner's grid since 2 November 2026).
    pub const fn play_resolution(self) -> (u32, u32) {
        match self {
            Self::Low => (320, 180),
            Self::Medium => (640, 360),
            Self::High => (1024, 576),
            Self::Ultra => (1280, 720),
        }
    }

    /// Whether live play samples the full transport (`TransportQuality::Full`) or the gameplay
    /// budget (`Gameplay`): fewer environment, smoke and glass-fog steps, the same law.
    pub const fn full_transport(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }

    /// The authored terrain's render grid: its collision grid on every preset (the terrain is an
    /// occluder; `terrain::authored_render_res`). Before 2026-09-29 the laptop drew 256² and the
    /// 4090 512² of a 1024² grid.
    pub const fn authored_terrain_grid(self) -> &'static str {
        match self {
            Self::Low | Self::Medium | Self::High | Self::Ultra => "collision grid",
        }
    }

    pub const fn shadow_resolution(self) -> u32 {
        match self {
            // At the 320x180 Low grid, a 512px map remains comfortably denser than the final
            // image. The renderer's 3x3 PCF masks the small texel increase.
            Self::Low => 512,
            Self::Medium => 1024,
            Self::High => 2048,
            Self::Ultra => 4096,
        }
    }

    pub const fn allows_hardware_rt(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }

    /// Periscope and vision-block relay views re-rendered per frame (raster_optics.rs). Low and
    /// Medium keep every visible pane drawn but refresh the largest each frame and rotate one more
    /// through the rest, so a small side pane may lag a frame or three (a budget of 1 would never
    /// refresh the others at all while the viewer holds still).
    /// `KESSEL_OPTIC_RELAYS_PER_FRAME=N` sets it outright (the largest port in view always
    /// refreshes; the others take turns at N - 1 a frame and show their last image between).
    pub fn optic_relays_per_frame(self) -> usize {
        static FORCED: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
        if let Some(forced) = *FORCED.get_or_init(|| std::env::var("KESSEL_OPTIC_RELAYS_PER_FRAME").ok()
            .and_then(|v| v.parse::<usize>().ok()).filter(|&n| n >= 1)) {
            return forced;
        }
        match self {
            Self::Low | Self::Medium => 2,
            Self::High | Self::Ultra => usize::MAX,
        }
    }

    /// High and Ultra share the desktop budgets wherever a system has only two tiers.
    pub const fn is_desktop(self) -> bool {
        matches!(self, Self::High | Self::Ultra)
    }

    pub const fn raymarch(self) -> RaymarchQuality {
        match self {
            // The leanest sampling of the same frame: one sample per pixel at 320x180, 32 froxel
            // slices, half-size shadow maps, a 64-texel lamp cube, a 256-wide sky cache.
            Self::Low => RaymarchQuality {
                vol_divisor: 6, vol_min: [64, 36], vol_slices: 32,
                shadow_far_res: 512, shadow_near_res: 512, cloud_shadow_res: 64,
                point_shadows: 1, point_face_res: 64,
                sky_cache_width: 256, cloud_view_steps: 8, cloud_light_steps: 2,
                budget_pixels: 57_600.0, max_samples: 1.0,
                relay_effect_samples_per_pixel: 4.0,
                relay_grain_samples_per_pixel: 0.5,
            },
            // The laptop budgets (the "Low" preset until 18 November 2026).
            Self::Medium => RaymarchQuality {
                vol_divisor: 6, vol_min: [80, 45], vol_slices: 48,
                shadow_far_res: 1024, shadow_near_res: 512, cloud_shadow_res: 128,
                point_shadows: 1, point_face_res: 128,
                sky_cache_width: 384, cloud_view_steps: 12, cloud_light_steps: 3,
                budget_pixels: 115_200.0, max_samples: 2.0,
                relay_effect_samples_per_pixel: 8.0,
                relay_grain_samples_per_pixel: 1.0,
            },
            Self::High => RaymarchQuality {
                vol_divisor: 4, vol_min: [160, 90], vol_slices: 96,
                shadow_far_res: 2048, shadow_near_res: 1024, cloud_shadow_res: 256,
                point_shadows: 2, point_face_res: 256,
                sky_cache_width: 768, cloud_view_steps: 20, cloud_light_steps: 4,
                budget_pixels: 360_000.0, max_samples: 3.0,
                relay_effect_samples_per_pixel: 16.0,
                relay_grain_samples_per_pixel: 2.0,
            },
            Self::Ultra => RaymarchQuality {
                vol_divisor: 3, vol_min: [160, 90], vol_slices: 128,
                shadow_far_res: 4096, shadow_near_res: 2048, cloud_shadow_res: 512,
                point_shadows: 2, point_face_res: 512,
                sky_cache_width: 1024, cloud_view_steps: 32, cloud_light_steps: 5,
                budget_pixels: 720_000.0, max_samples: 3.0,
                relay_effect_samples_per_pixel: 32.0,
                relay_grain_samples_per_pixel: 4.0,
            },
        }
    }

    /// Discrete adapters default to High and integrated ones to Medium; a software, virtual or
    /// unknown device gets Low. Ultra is always an explicit choice. The menu choice and saved
    /// preferences override this.
    pub const fn for_adapter(device_type: wgpu::DeviceType) -> Self {
        match device_type {
            wgpu::DeviceType::DiscreteGpu => Self::High,
            wgpu::DeviceType::IntegratedGpu => Self::Medium,
            _ => Self::Low,
        }
    }

    /// `for_adapter`, with the single-board GPUs that report themselves integrated (the
    /// Raspberry Pi 5's VideoCore VII as "V3D 7.1", Mali, Adreno, PowerVR) sent to Low.
    pub fn for_adapter_info(info: &wgpu::AdapterInfo) -> Self {
        let name = info.name.to_ascii_lowercase();
        let single_board = ["v3d", "videocore", "mali", "adreno", "powervr", "llvmpipe", "lavapipe"]
            .iter().any(|tag| name.contains(tag));
        if single_board { Self::Low } else { Self::for_adapter(info.device_type) }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" | "pi" | "pi5" => Some(Self::Low),
            "medium" | "laptop" => Some(Self::Medium),
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
        assert_eq!(GraphicsPreset::ALL, [GraphicsPreset::Low, GraphicsPreset::Medium, GraphicsPreset::High]);
        assert_eq!(GraphicsPreset::Low.internal_resolution(), (320, 180));
        assert_eq!(GraphicsPreset::Low.authored_terrain_grid(), GraphicsPreset::High.authored_terrain_grid());
        assert_eq!(GraphicsPreset::Low.shadow_resolution(), 512);
        assert!(!GraphicsPreset::Low.allows_hardware_rt() && !GraphicsPreset::Medium.allows_hardware_rt());
        assert_eq!(GraphicsPreset::Medium.shadow_resolution(), 1024);

        assert_eq!(GraphicsPreset::High.internal_resolution(), (640, 360));
        assert_eq!(GraphicsPreset::High.authored_terrain_grid(), "collision grid");
        assert_eq!(GraphicsPreset::High.shadow_resolution(), 2048);
        assert!(GraphicsPreset::High.allows_hardware_rt());

        assert_eq!(GraphicsPreset::Ultra.internal_resolution(), (800, 450));
        assert!(GraphicsPreset::Ultra.allows_hardware_rt());
        assert!(GraphicsPreset::Ultra.shadow_resolution() > GraphicsPreset::High.shadow_resolution());
    }

    #[test]
    fn play_grids_are_supported_and_whole_multiples_of_the_common_panels() {
        assert_eq!(GraphicsPreset::Low.play_resolution(), (320, 180));
        assert_eq!(GraphicsPreset::Medium.play_resolution(), (640, 360));
        assert_eq!(GraphicsPreset::High.play_resolution(), (1024, 576));
        for preset in GraphicsPreset::ALL {
            let (w, h) = preset.play_resolution();
            assert!(crate::renderer::INTERNAL_RES_PRESETS.contains(&(w, h)), "{preset:?}");
            assert!(crate::graphics_menu::RESOLUTIONS.contains(&(w, h)), "{preset:?} is not a menu size");
            assert_eq!((5120 % w, 2880 % h, 5120 / w), (0, 0, 2880 / h), "{preset:?} on 5120x2880");
            assert!([512, 768, 1024, 1536, 2048].contains(&preset.shadow_resolution()), "{preset:?}");
        }
        assert_eq!((1920 % 320, 1080 % 180, 1920 % 640, 1080 % 360), (0, 0, 0, 0));
        assert!(!GraphicsPreset::Low.full_transport() && !GraphicsPreset::Medium.full_transport());
        assert!(GraphicsPreset::High.full_transport());
        // A relay budget of 1 never refreshes the smaller panes while the viewer holds still.
        if std::env::var_os("KESSEL_OPTIC_RELAYS_PER_FRAME").is_none() {
            for preset in GraphicsPreset::ALL { assert!(preset.optic_relays_per_frame() >= 2, "{preset:?}"); }
        }
    }

    #[test]
    fn presets_parse_their_keys_and_the_old_names_and_cycle() {
        for preset in GraphicsPreset::ALL.into_iter().chain([GraphicsPreset::Ultra]) {
            assert_eq!(GraphicsPreset::parse(preset.key()), Some(preset));
            assert_eq!(GraphicsPreset::parse(preset.label()), Some(preset));
        }
        // "laptop" named the laptop budgets, which are Medium now.
        assert_eq!(GraphicsPreset::parse("laptop"), Some(GraphicsPreset::Medium));
        assert_eq!(GraphicsPreset::parse("pi5"), Some(GraphicsPreset::Low));
        assert_eq!(GraphicsPreset::parse("4090"), Some(GraphicsPreset::High));
        let mut p = GraphicsPreset::Low;
        for _ in 0..3 { p = p.toggled(); }
        assert_eq!(p, GraphicsPreset::Low);
        for preset in GraphicsPreset::ALL {
            assert_eq!(preset.stepped(true).stepped(false), preset);
            assert!(GraphicsPreset::ALL.contains(&preset.stepped(true)));
        }
        assert_eq!(GraphicsPreset::Ultra.toggled(), GraphicsPreset::Low);
    }

    #[test]
    fn raymarch_budgets_rise_monotonically_and_high_keeps_the_reference() {
        let [low, medium, high] = GraphicsPreset::ALL.map(GraphicsPreset::raymarch);
        let ultra = GraphicsPreset::Ultra.raymarch();
        assert_eq!((high.vol_divisor, high.vol_slices, high.shadow_far_res, high.point_shadows), (4, 96, 2048, 2));
        assert_eq!(high.sample_budget(600, 600), 1.0);
        // Low draws one sample a pixel at its own grid; Medium is the old laptop table.
        assert_eq!(low.sample_budget(320, 180), 1.0);
        assert_eq!((medium.vol_slices, medium.shadow_far_res, medium.budget_pixels), (48, 1024, 115_200.0));
        // Every preset keeps a shadowed lamp: a budget never removes an occluder (CLAUDE.md).
        assert!(low.point_shadows >= 1);
        for (a, b) in [(low, medium), (medium, high), (high, ultra)] {
            assert!(a.vol_slices < b.vol_slices && a.shadow_far_res < b.shadow_far_res);
            assert!(a.sky_cache_width < b.sky_cache_width && a.cloud_view_steps < b.cloud_view_steps);
            assert!(a.point_shadows <= b.point_shadows && b.point_shadows <= 2);
            assert!(a.relay_effect_samples_per_pixel < b.relay_effect_samples_per_pixel);
            assert!(a.relay_grain_samples_per_pixel < b.relay_grain_samples_per_pixel);
            assert!(b.relay_grain_samples_per_pixel <= b.relay_effect_samples_per_pixel / 4.0);
        }
        // The gunner's sight pane (640x360) on High draws at most 460,800 grains a frame: at the
        // spark rate it drew 3.7 million for one burning wreck (44 ms, 2026-11-01).
        assert_eq!(high.relay_grain_budget([640, 360]), 460_800);
        // A 240x136 relayed port on Medium draws at most 261,120 effect samples a pass (it drew
        // 1.5 million sparks and 3.5 million grains in one HE strike before, 2026-10-28).
        assert_eq!(medium.relay_effect_budget([240, 136]), 261_120);
    }

    #[test]
    fn adapters_default_high_medium_or_low() {
        let info = |name: &str, device_type| wgpu::AdapterInfo { name: name.into(), vendor: 0, device: 0,
            device_type, driver: String::new(), driver_info: String::new(), backend: wgpu::Backend::Vulkan };
        assert_eq!(GraphicsPreset::for_adapter_info(&info("V3D 7.1.7", wgpu::DeviceType::IntegratedGpu)), GraphicsPreset::Low);
        assert_eq!(GraphicsPreset::for_adapter_info(&info("Intel(R) Arc(TM) Graphics (MTL)", wgpu::DeviceType::IntegratedGpu)),
            GraphicsPreset::Medium);
        assert_eq!(GraphicsPreset::for_adapter_info(&info("NVIDIA GeForce RTX 4090", wgpu::DeviceType::DiscreteGpu)),
            GraphicsPreset::High);
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::DiscreteGpu),
            GraphicsPreset::High
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::IntegratedGpu),
            GraphicsPreset::Medium
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::Cpu),
            GraphicsPreset::Low
        );
    }
}
