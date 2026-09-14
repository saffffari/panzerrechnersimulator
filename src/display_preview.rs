//! Physical panel previews: native render pixels and millimetres are separate quantities.
//! Specs: adafruit.com/product/{913,1591,1678}; #1591 Datasheet.pdf, page 4.
use crate::renderer::{self, Renderer};
use winit::{dpi::PhysicalSize, window::{Fullscreen, Window}};

#[derive(Clone, Copy)]
pub struct Panel {
    pub id: u32,
    pub label: &'static str,
    pub pixels: (u32, u32),
    pub mm: (f64, f64),
}
pub const PANELS: [Panel; 3] = [
    Panel { id: 913, label: "3.5 inch / 320 x 240", pixels: (320, 240), mm: (70.1, 52.6) },
    Panel { id: 1591, label: "4.3 inch / 480 x 272", pixels: (480, 272), mm: (95.04, 53.856) },
    Panel { id: 1678, label: "5 inch / 800 x 480", pixels: (800, 480), mm: (108.0, 65.0) },
];

/// Physical framebuffer pixels per millimetre, never desktop logical pixels.
#[derive(Clone, Copy, Debug)]
struct Density { x: f64, y: f64 }
impl Density {
    fn panel_size(self, panel: Panel, percent: u32) -> PhysicalSize<u32> {
        let scale = percent as f64 / 100.0;
        PhysicalSize::new((panel.mm.0 * self.x * scale).round().max(1.0) as u32,
            (panel.mm.1 * self.y * scale).round().max(1.0) as u32)
    }
}

fn kscreen_density(value: &serde_json::Value, name: Option<&str>) -> Option<Density> {
    let outputs: Vec<_> = value.get("outputs")?.as_array()?.iter()
        .filter(|o| o["connected"] == true && o["enabled"] == true).collect();
    // Never silently use a different monitor's dimensions.
    let output = if let Some(name) = name {
        outputs.iter().find(|o| o["name"].as_str() == Some(name)).copied()?
    } else if outputs.len() == 1 { outputs[0] } else { return None };
    let mut mmx = output["sizeMM"]["width"].as_f64()?;
    let mut mmy = output["sizeMM"]["height"].as_f64()?;
    if matches!(output["rotation"].as_u64(), Some(2 | 8)) { std::mem::swap(&mut mmx, &mut mmy); }
    let x = output["size"]["width"].as_f64()? / mmx;
    let y = output["size"]["height"].as_f64()? / mmy;
    (mmx > 0.0 && mmy > 0.0 && (1.0..=40.0).contains(&x) && (1.0..=40.0).contains(&y))
        .then_some(Density { x, y })
}

struct DesktopWindow {
    size: PhysicalSize<u32>,
    position: Option<winit::dpi::PhysicalPosition<i32>>,
    fullscreen: Option<Fullscreen>,
    maximized: bool,
    decorated: bool,
    resizable: bool,
    resolution: (u32, u32),
}

pub struct DisplayPreview {
    pub active: Option<usize>,
    pub percent: u32,
    pub measured: bool,
    density: Density,
    monitor: Option<String>,
    desktop: Option<DesktopWindow>,
}
impl Default for DisplayPreview {
    fn default() -> Self {
        Self { active: None, percent: 100, measured: false,
            density: Density { x: 96.0 / 25.4, y: 96.0 / 25.4 }, monitor: None, desktop: None }
    }
}
impl DisplayPreview {
    fn detect(&mut self, window: &Window) {
        self.monitor = window.current_monitor().and_then(|m| m.name());
        let detected = std::process::Command::new("kscreen-doctor").arg("-j").output().ok()
            .filter(|o| o.status.success())
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| kscreen_density(&v, self.monitor.as_deref()));
        self.measured = detected.is_some();
        self.density = detected.unwrap_or(Density { x: 96.0 / 25.4, y: 96.0 / 25.4 });
    }

    pub fn apply(&mut self, index: Option<usize>, window: &Window, renderer: &mut Renderer,
        device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
        if let Some(index) = index {
            let panel = *PANELS.get(index).ok_or_else(|| anyhow::anyhow!("Unknown display"))?;
            let old_resolution = (renderer.iw, renderer.ih);
            renderer.resize_shader_resolution(device, queue, panel.pixels)?;
            if self.desktop.is_none() {
                self.desktop = Some(DesktopWindow { size: window.inner_size(),
                    position: window.outer_position().ok(), fullscreen: window.fullscreen(),
                    maximized: window.is_maximized(), decorated: window.is_decorated(),
                    resizable: window.is_resizable(), resolution: old_resolution });
            }
            self.detect(window);
            self.active = Some(index);
            renderer::set_display_internal_resolution(Some(panel.pixels));
            window.set_fullscreen(None);
            window.set_maximized(false);
            window.set_decorations(false);
            window.set_resizable(false);
            self.size_window(window);
            window.set_title(&format!("Panzerrechnersystem - Adafruit #{}", panel.id));
        } else {
            if let Some(desktop) = self.desktop.as_ref() {
                renderer.resize_shader_resolution(device, queue, desktop.resolution)?;
            }
            self.active = None;
            renderer::set_display_internal_resolution(None);
            if let Some(desktop) = self.desktop.take() {
                window.set_resizable(desktop.resizable);
                window.set_decorations(desktop.decorated);
                let _ = window.request_inner_size(desktop.size);
                if let Some(pos) = desktop.position { window.set_outer_position(pos); }
                window.set_maximized(desktop.maximized);
                window.set_fullscreen(desktop.fullscreen);
            }
            window.set_title("Panzerrechnersystem");
        }
        Ok(())
    }

    fn size_window(&self, window: &Window) {
        if let Some(index) = self.active {
            let target = self.density.panel_size(PANELS[index], self.percent);
            // Winit converts physical pixels using the current Wayland scale factor. Requesting
            // logical pixel counts here would enlarge the preview by HAL's 170% desktop scale.
            let _ = window.request_inner_size(target);
            println!("[display] Adafruit #{}: {}x{} render, {}x{} physical window px, {}% size, monitor {:?}, measured={}",
                PANELS[index].id, PANELS[index].pixels.0, PANELS[index].pixels.1,
                target.width, target.height, self.percent, self.monitor, self.measured);
        }
    }

    pub fn calibrate(&mut self, forward: bool, window: &Window) {
        self.percent = (self.percent as i32 + if forward { 1 } else { -1 }).clamp(25, 300) as u32;
        self.size_window(window);
    }

    pub fn refresh_monitor(&mut self, window: &Window) {
        if self.active.is_some() {
            self.detect(window);
            self.size_window(window);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn studio() -> serde_json::Value {
        serde_json::json!({"outputs": [{"name":"DP-3", "connected":true,"enabled":true,
            "size":{"width":5120,"height":2880},"sizeMM":{"width":595,"height":335},
            "rotation":1,"scale":1.7}]})
    }
    #[test]
    fn hal_physical_size_is_independent_of_desktop_scale() {
        let mut value = studio();
        for scale in [1.0, 1.7, 2.0] {
            value["outputs"][0]["scale"] = serde_json::json!(scale);
            let density = kscreen_density(&value, Some("DP-3")).unwrap();
            assert_eq!(density.panel_size(PANELS[0], 100), PhysicalSize::new(603, 452));
            assert_eq!(density.panel_size(PANELS[1], 100), PhysicalSize::new(818, 463));
            assert_eq!(density.panel_size(PANELS[2], 100), PhysicalSize::new(929, 559));
        }
    }
    #[test]
    fn missing_or_wrong_monitor_never_claims_a_physical_measurement() {
        assert!(kscreen_density(&studio(), Some("HDMI-1")).is_none());
        let mut value = studio();
        value["outputs"][0]["sizeMM"]["width"] = serde_json::json!(0);
        assert!(kscreen_density(&value, Some("DP-3")).is_none());
    }
    #[test]
    fn rotation_swaps_physical_axes_and_calibration_scales_both_axes() {
        let mut value = studio();
        value["outputs"][0]["rotation"] = serde_json::json!(2);
        value["outputs"][0]["size"] = serde_json::json!({"width":2880,"height":5120});
        let d = kscreen_density(&value, None).unwrap();
        let full = d.panel_size(PANELS[1], 100);
        let half = d.panel_size(PANELS[1], 50);
        assert!((full.width as i32 - half.width as i32 * 2).abs() <= 1);
        assert!((full.height as i32 - half.height as i32 * 2).abs() <= 1);
    }
}
