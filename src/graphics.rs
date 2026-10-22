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
    /// Integrated/laptop budget: preserve the authored atmosphere and bloom, shed resolution,
    /// terrain raster density, shadow-map bandwidth and the hardware ray/deferred path.
    Laptop,
    /// Discrete hero target.  The menu calls this "4090" so the intended rig is unambiguous.
    High,
}

impl GraphicsPreset {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Laptop => "Laptop",
            Self::High => "4090",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Laptop => "laptop",
            Self::High => "high",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Laptop => Self::High,
            Self::High => Self::Laptop,
        }
    }

    pub const fn internal_resolution(self) -> (u32, u32) {
        match self {
            Self::Laptop => (320, 180),
            Self::High => (640, 360),
        }
    }

    /// The authored terrain's render grid: its collision grid on both presets (the terrain is an
    /// occluder; `terrain::authored_render_res`). Before 2026-09-29 the laptop drew 256² and the
    /// 4090 512² of a 1024² grid.
    pub const fn authored_terrain_grid(self) -> &'static str {
        match self {
            Self::Laptop | Self::High => "collision grid",
        }
    }

    pub const fn shadow_resolution(self) -> u32 {
        match self {
            // At the 320x180 laptop target, a 512px map remains comfortably denser than the
            // final image. The renderer's 3x3 PCF masks the small texel increase while halving
            // each shadow dimension relative to the prior Laptop budget.
            Self::Laptop => 512,
            Self::High => 2048,
        }
    }

    pub const fn allows_hardware_rt(self) -> bool {
        matches!(self, Self::High)
    }

    /// Integrated adapters default to the measured laptop budget.  Only an adapter explicitly
    /// reported as discrete gets the hero preset automatically; an Other/virtual/software device
    /// is treated conservatively.  The menu choice can always override this for the session.
    pub const fn for_adapter(device_type: wgpu::DeviceType) -> Self {
        match device_type {
            wgpu::DeviceType::DiscreteGpu => Self::High,
            _ => Self::Laptop,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "laptop" | "low" => Some(Self::Laptop),
            "4090" | "high" | "hero" => Some(Self::High),
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
    fn presets_match_the_two_shipping_budgets() {
        assert_eq!(GraphicsPreset::Laptop.internal_resolution(), (320, 180));
        assert_eq!(GraphicsPreset::Laptop.authored_terrain_grid(), GraphicsPreset::High.authored_terrain_grid());
        assert_eq!(GraphicsPreset::Laptop.shadow_resolution(), 512);
        assert!(!GraphicsPreset::Laptop.allows_hardware_rt());

        assert_eq!(GraphicsPreset::High.internal_resolution(), (640, 360));
        assert_eq!(GraphicsPreset::High.authored_terrain_grid(), "collision grid");
        assert_eq!(GraphicsPreset::High.shadow_resolution(), 2048);
        assert!(GraphicsPreset::High.allows_hardware_rt());
    }

    #[test]
    fn only_discrete_adapters_default_high() {
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::DiscreteGpu),
            GraphicsPreset::High
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::IntegratedGpu),
            GraphicsPreset::Laptop
        );
        assert_eq!(
            GraphicsPreset::for_adapter(wgpu::DeviceType::Cpu),
            GraphicsPreset::Laptop
        );
    }
}
