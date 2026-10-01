//! User display preferences. These affect sampling and image reproduction, never world state.
use crate::renderer::{display_tuning::DisplayTuning, Renderer};
use crate::ui::Align;
use anyhow::{ensure, Context};
use serde_json::{json, Value};

pub const RESOLUTIONS: [(u32, u32); 9] = [
    (320, 180),
    (480, 272),
    (640, 360),
    (800, 450),
    (960, 540),
    (1280, 720),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
];
pub const WINDOWS: [(u32, u32); 4] = [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];
const SHADOWS: [u32; 5] = [512, 768, 1024, 1536, 2048];
pub const TABS: [&str; 6] = [
    "Display",
    "Quality",
    "Color",
    "Retro",
    "Time of day",
    "Weather",
];
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub resolution: (u32, u32),
    pub window: (u32, u32),
    pub fullscreen: bool,
    pub vsync: bool,
    pub shadows: u32,
    pub full_transport: bool,
    pub optics: usize,
    pub fps: bool,
    pub exposure_ev: f32,
    pub saturation: f32,
    pub warmth: f32,
    pub gamma: f32,
    pub display: DisplayTuning,
    pub sun_phase: u8,
    pub sun_speed: f32,
    pub storm: Option<f32>,
}
impl Settings {
    pub fn from_renderer(r: &Renderer) -> Self {
        Self {
            resolution: (r.iw, r.ih),
            window: (1280, 720),
            fullscreen: false,
            vsync: true,
            shadows: r.shadow_res,
            full_transport: r.full_transport_quality(),
            optics: 1,
            fps: false,
            exposure_ev: r.display_grade[0].log2(),
            saturation: r.display_grade[1],
            warmth: r.display_grade[2],
            gamma: r.display_grade[3],
            sun_phase: r.user_sky_phase(),
            sun_speed: r.sky_time_scale,
            storm: r.weather_override,
            display: r
                .display_tuning
                .unwrap_or_else(|| DisplayTuning::at_resolution(r.iw, r.ih)),
        }
    }
    pub fn defaults() -> Self {
        Self {
            resolution: (640, 360),
            window: (1280, 720),
            fullscreen: false,
            vsync: true,
            shadows: 2048,
            full_transport: true,
            optics: 1,
            fps: false,
            sun_phase: 0,
            sun_speed: 1.0,
            storm: None,
            exposure_ev: 0.0,
            saturation: 1.0,
            warmth: 0.0,
            gamma: 1.0,
            display: DisplayTuning::at_resolution(640, 360),
        }
    }
    pub fn apply(
        &self,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        if self.resolution != (r.iw, r.ih) || self.shadows != r.shadow_res {
            r.resize_shader_targets(device, queue, self.resolution, self.shadows)?;
        }
        let mut display = self.display;
        (display.width, display.height) = self.resolution;
        r.display_tuning = Some(display);
        r.display_grade = [
            self.exposure_ev.exp2(),
            self.saturation,
            self.warmth,
            self.gamma,
        ];
        r.set_full_transport_quality(self.full_transport);
        r.sky_time_scale = self.sun_speed;
        r.set_user_sky_phase(self.sun_phase);
        r.set_weather_override(self.storm);
        r.invalidate_title_backdrop();
        Ok(())
    }
    pub fn to_json(&self) -> Value {
        json!({"version":1,"resolution":[self.resolution.0,self.resolution.1],
        "window":[self.window.0,self.window.1],"fullscreen":self.fullscreen,"vsync":self.vsync,
        "shadows":self.shadows,"full_transport":self.full_transport,"optics":self.optics,"fps":self.fps,
        "exposure_ev":self.exposure_ev,"saturation":self.saturation,"warmth":self.warmth,"gamma":self.gamma,
        "storm":self.storm,"sun_phase":self.sun_phase,"sun_speed":self.sun_speed,"display":self.display.to_json()})
    }
    pub fn from_json(v: &Value) -> anyhow::Result<Self> {
        ensure!(v["version"] == 1, "unsupported graphics settings version");
        let pair = |k: &str| -> anyhow::Result<(u32, u32)> {
            Ok((
                u32::try_from(v[k][0].as_u64().context("missing width")?)?,
                u32::try_from(v[k][1].as_u64().context("missing height")?)?,
            ))
        };
        let boolean = |k: &str| v[k].as_bool().with_context(|| format!("invalid {k}"));
        let number = |k: &str, lo: f32, hi: f32| -> anyhow::Result<f32> {
            let n = v[k].as_f64().with_context(|| format!("invalid {k}"))? as f32;
            ensure!(n.is_finite() && (lo..=hi).contains(&n), "{k} out of range");
            Ok(n)
        };
        let resolution = pair("resolution")?;
        let window = pair("window")?;
        ensure!(
            crate::renderer::INTERNAL_RES_PRESETS.contains(&resolution),
            "unsupported render size"
        );
        ensure!(WINDOWS.contains(&window), "unsupported window size");
        let shadows = u32::try_from(v["shadows"].as_u64().context("missing shadows")?)?;
        ensure!(SHADOWS.contains(&shadows), "unsupported shadows");
        let optics = usize::try_from(v["optics"].as_u64().context("missing optics")?)?;
        ensure!((1..=4).contains(&optics), "unsupported optics budget");
        let sun_phase = match v.get("sun_phase") {
            None => 0,
            Some(n) => n.as_u64().context("invalid sun phase")?,
        };
        ensure!(sun_phase <= 4, "invalid sun phase");
        let sun_speed = if v.get("sun_speed").is_some() {
            number("sun_speed", 0.0, 144.0)?
        } else {
            1.0
        };
        Ok(Self {
            storm: if v.get("storm").is_none_or(Value::is_null) {
                None
            } else {
                Some(number("storm", 0.0, 8.0)?)
            },
            sun_phase: sun_phase as u8,
            sun_speed,
            resolution,
            window,
            shadows,
            optics,
            fullscreen: boolean("fullscreen")?,
            vsync: boolean("vsync")?,
            full_transport: boolean("full_transport")?,
            fps: boolean("fps")?,
            exposure_ev: number("exposure_ev", -3.0, 3.0)?,
            saturation: number("saturation", 0.0, 1.5)?,
            warmth: number("warmth", -1.0, 1.0)?,
            gamma: number("gamma", 0.7, 1.4)?,
            display: DisplayTuning::from_json(&v["display"])?,
        })
    }
    pub fn load(path: &std::path::Path) -> anyhow::Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(Self::from_json(&serde_json::from_slice(&bytes)?)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(&self.to_json())?)?;
        std::fs::rename(temp, path)?;
        Ok(())
    }
    pub fn adjust(&mut self, tab: usize, row: usize, up: bool) {
        fn cycle<T: Copy + PartialEq>(v: &mut T, choices: &[T], up: bool) {
            let i = choices.iter().position(|n| n == v).unwrap_or(0);
            *v = choices[(i + if up { 1 } else { choices.len() - 1 }) % choices.len()];
        }
        let step = |v: &mut f32, d: f32, lo: f32, hi: f32| {
            *v = ((*v + if up { d } else { -d }) * 100.0)
                .round()
                .clamp(lo * 100.0, hi * 100.0)
                / 100.0
        };
        match (tab, row) {
            (0, 1) => cycle(&mut self.resolution, &RESOLUTIONS, up),
            (0, 2) => cycle(&mut self.window, &WINDOWS, up),
            (0, 3) => self.fullscreen = !self.fullscreen,
            (0, 4) => self.vsync = !self.vsync,
            (1, 1) => cycle(&mut self.shadows, &SHADOWS, up),
            (1, 2) => self.full_transport = !self.full_transport,
            (1, 3) => cycle(&mut self.optics, &[1, 2, 3, 4], up),
            (1, 4) => self.fps = !self.fps,
            (2, 1) => step(&mut self.exposure_ev, 0.25, -3.0, 3.0),
            (2, 2) => step(&mut self.saturation, 0.05, 0.0, 1.5),
            (2, 3) => step(&mut self.warmth, 0.1, -1.0, 1.0),
            (2, 4) => step(&mut self.gamma, 0.05, 0.7, 1.4),
            (2, 5) => {
                let mut n = if self.display.ambient_white_balance {
                    self.display.ambient_white_balance_strength
                } else {
                    0.0
                };
                step(&mut n, 0.1, 0.0, 1.0);
                self.display.ambient_white_balance = n > 0.0;
                self.display.ambient_white_balance_strength = n;
            }
            (3, 1) => step(&mut self.display.retro_strength, 0.1, 0.0, 1.0),
            (3, 2) => cycle(
                &mut self.display.palette_levels,
                &[7.0, 15.0, 31.0, 63.0, 127.0, 255.0],
                up,
            ),
            (3, 3) => step(&mut self.display.ordered_dither, 0.1, 0.0, 1.0),
            (3, 4) => step(&mut self.display.ldr_dither, 0.1, 0.0, 1.0),
            (5, 1) => {
                let mut n = self.storm.unwrap_or(-0.25);
                step(&mut n, 0.25, -0.25, 8.0);
                self.storm = if n < 0.0 { None } else { Some(n) };
            }
            (4, 1) => cycle(&mut self.sun_phase, &[0, 1, 2, 3, 4], up),
            (4, 2) => cycle(&mut self.sun_speed, &[0.0, 1.0, 12.0, 48.0, 144.0], up),
            _ => {}
        }
        (self.display.width, self.display.height) = self.resolution;
    }
    fn rows(&self, tab: usize) -> Vec<(&'static str, String)> {
        let on = |v| if v { "On".to_owned() } else { "Off".to_owned() };
        let percent = |v: f32| format!("{:.0}%", v * 100.0);
        match tab {
            0 => vec![
                (
                    "Render resolution",
                    format!("{} x {}", self.resolution.0, self.resolution.1),
                ),
                (
                    "Window resolution",
                    format!("{} x {}", self.window.0, self.window.1),
                ),
                ("Fullscreen", on(self.fullscreen)),
                ("VSync", on(self.vsync)),
            ],
            1 => vec![
                ("Shadow resolution", format!("{}", self.shadows)),
                (
                    "Light sampling",
                    if self.full_transport {
                        "Full"
                    } else {
                        "Balanced"
                    }
                    .into(),
                ),
                ("Optic updates / frame", self.optics.to_string()),
                ("FPS counter", on(self.fps)),
            ],
            2 => vec![
                (
                    "Exposure compensation",
                    format!("{:+.2} EV", self.exposure_ev),
                ),
                ("Saturation", percent(self.saturation)),
                ("Warmth", format!("{:+.1}", self.warmth)),
                ("Display gamma", format!("{:.2}", self.gamma)),
                (
                    "Ambient white balance",
                    percent(if self.display.ambient_white_balance {
                        self.display.ambient_white_balance_strength
                    } else {
                        0.0
                    }),
                ),
            ],
            5 => vec![(
                "Storm intensity",
                self.storm
                    .map(|s| {
                        if s == 0.0 {
                            "Clear".into()
                        } else {
                            format!("{s:.2}x")
                        }
                    })
                    .unwrap_or("Authored".into()),
            )],
            4 => vec![
                (
                    "Time of day",
                    ["Authored", "Sunrise", "Day", "Sunset", "Night"][self.sun_phase as usize]
                        .into(),
                ),
                (
                    "Day-cycle speed",
                    if self.sun_speed == 0.0 {
                        "Frozen".into()
                    } else {
                        match self.sun_speed as u32 {
                            1 => "Real time".into(),
                            12 => "2 hour day".into(),
                            48 => "30 min day".into(),
                            144 => "10 min day".into(),
                            _ => format!("{:.0}x", self.sun_speed),
                        }
                    },
                ),
            ],
            _ => vec![
                ("Retro strength", percent(self.display.retro_strength)),
                (
                    "Palette levels / channel",
                    format!("{:.0}", self.display.palette_levels),
                ),
                ("Ordered dither", percent(self.display.ordered_dither)),
                ("Smooth gradients", percent(self.display.ldr_dither)),
            ],
        }
    }
}
pub fn config_path() -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("KESSEL_GRAPHICS_CONFIG") {
        return Ok(path.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".config")))
        .context("no config directory")?;
    Ok(base.join("panzerrechnersystem/graphics.v1.json"))
}
#[derive(Default)]
pub struct Menu {
    pub tab: usize,
    pub selected: usize,
    pub message: String,
    pub from_title: bool,
}
impl Menu {
    pub fn controls(&self) -> usize {
        if self.tab == 5 {
            1
        } else if self.tab == 2 {
            5
        } else if self.tab == 4 {
            2
        } else {
            4
        }
    }
    pub fn count(&self) -> usize {
        self.controls() + 4
    }
    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        if !(0.035..=0.655).contains(&x) {
            return None;
        }
        (0..self.count()).find(|&i| {
            let top = 0.24 + i as f32 * 0.065;
            y >= top && y < top + 0.056
        })
    }
    pub fn change_tab(&mut self, up: bool) {
        self.tab = (self.tab + if up { 1 } else { TABS.len() - 1 }) % TABS.len();
        self.selected = 0;
    }
    pub fn draw(&self, r: &mut Renderer, s: &Settings) {
        let ink = [0.84, 0.87, 0.85, 1.0];
        let dim = [0.49, 0.56, 0.57, 1.0];
        let gold = [1.0, 0.70, 0.28, 1.0];
        r.fill_rect(0.025, 0.04, 0.64, 0.91, [0.009, 0.014, 0.017, 0.96]);
        r.draw_text_bold("Graphics", 0.055, 0.075, 0.061, ink, Align::Left);
        r.draw_text(
            "LIVE PREVIEW / SIMULATION PAUSED",
            0.055,
            0.155,
            0.020,
            dim,
            Align::Left,
        );
        let mut rows = vec![("Page", TABS[self.tab].to_owned())];
        rows.extend(s.rows(self.tab));
        rows.extend([
            ("Save preferences", String::new()),
            ("Restore defaults", String::new()),
            ("Back", String::new()),
        ]);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = 0.24 + i as f32 * 0.065;
            let color = if i == self.selected { gold } else { ink };
            if i == self.selected {
                r.fill_rect(0.04, y - 0.003, 0.61, 0.057, [0.085, 0.10, 0.10, 0.97]);
            }
            r.draw_text(label, 0.055, y + 0.011, 0.029, color, Align::Left);
            if !value.is_empty() {
                r.draw_text(
                    &format!("< {value} >"),
                    0.635,
                    y + 0.011,
                    0.029,
                    color,
                    Align::Right,
                );
            }
        }
        let hint = match self.tab {
            0 => "RENDER SIZE CONTROLS SHARPNESS AND GPU COST",
            1 => "HIGHER VALUES USE MORE GPU TIME",
            2 => "CAMERA / DISPLAY COLOR TREATMENT",
            4 => "SUN / SKY / WORLD LIGHTING CHANGE TOGETHER",
            5 => "SNOW / FOG / WIND / CLOUDS RISE TOGETHER",
            _ => "PALETTE AND DITHER / CLEAN AT ZERO RETRO",
        };
        r.draw_text(
            if self.message.is_empty() {
                hint
            } else {
                &self.message
            },
            0.055,
            0.86,
            0.018,
            dim,
            Align::Left,
        );
        r.draw_text(
            "ARROWS ADJUST / TAB NEXT PAGE / ESC BACK",
            0.055,
            0.903,
            0.018,
            dim,
            Align::Left,
        );
        r.draw_text("Preview", 0.82, 0.91, 0.023, ink, Align::Center);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_validate_roundtrip_and_reject_corruption() {
        let s = Settings::defaults();
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
        for (k, v) in [
            ("gamma", json!(0)),
            ("optics", json!(0)),
            ("shadows", json!(999999)),
            ("resolution", json!([99999, 99999])),
            ("version", json!(2)),
        ] {
            let mut j = s.to_json();
            j[k] = v;
            assert!(Settings::from_json(&j).is_err());
        }
    }
    #[test]
    fn controls_stay_in_range_and_rows_are_clickable() {
        let mut s = Settings::defaults();
        for tab in 0..TABS.len() {
            let m = Menu {
                tab,
                ..Default::default()
            };
            for row in 0..m.count() {
                assert_eq!(m.row_at(0.3, 0.25 + row as f32 * 0.065), Some(row));
                for _ in 0..120 {
                    s.adjust(tab, row, true);
                    assert!(Settings::from_json(&s.to_json()).is_ok());
                }
                for _ in 0..120 {
                    s.adjust(tab, row, false);
                    assert!(Settings::from_json(&s.to_json()).is_ok());
                }
            }
        }
    }
    #[test]
    fn saved_preferences_reopen() {
        let dir = std::env::temp_dir().join(format!("panzer-graphics-test-{}", std::process::id()));
        let path = dir.join("graphics.json");
        let mut s = Settings::defaults();
        s.adjust(2, 1, true);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), Some(s));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod gpu_review {
    use super::*;
    #[test]
    #[ignore = "host GPU scene and render-target regression"]
    fn graphics_targets_and_color_survive_resize_without_resetting_game() -> anyhow::Result<()> {
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let (mut renderer, mut game) =
            crate::game::Game::build_act(&device, &queue, false, &crate::world::garage())?;
        game.garage_setup();
        game.mute_audio();
        game.set_defer_rtt_to_render(true);
        game.update(
            &crate::game::TankInput::default(),
            1.0 / 60.0,
            &mut renderer,
        );
        let original_position = game.player.pos;
        let original_meshes = renderer.mesh_count();
        let mut settings = Settings::from_renderer(&renderer);
        settings.resolution = (800, 450);
        settings.shadows = 512;
        settings.exposure_ev = -0.5;
        settings.saturation = 0.75;
        settings.full_transport = false;
        settings.sun_phase = 3;
        settings.sun_speed = 48.0;
        settings.apply(&mut renderer, &device, &queue)?;
        assert_eq!((renderer.iw, renderer.ih), (800, 450));
        assert_eq!(renderer.shadow_res, 512);
        assert_eq!(renderer.mesh_count(), original_meshes);
        assert_eq!(game.player.pos, original_position);
        assert_eq!(renderer.user_sky_phase(), 3);
        assert_eq!(renderer.sky_time_scale, 48.0);
        let clock = renderer.sky_clock;
        renderer.advance_sky_clock(0.25);
        assert!((renderer.sky_clock.elapsed_s - clock.elapsed_s - 12.0).abs() < 1e-6);
        let expected_grade = renderer.display_grade;
        settings.resolution = (640, 360);
        settings.shadows = 1024;
        settings.apply(&mut renderer, &device, &queue)?;
        assert_eq!(renderer.display_grade, expected_grade);
        assert!(!renderer.full_transport_quality());
        assert_eq!(renderer.mesh_count(), original_meshes);
        assert_eq!(game.player.pos, original_position);
        assert!(
            (renderer.sky_clock.elapsed_s - clock.elapsed_s - 12.0).abs() < 1e-6,
            "resolution cannot restart the day cycle"
        );
        let menu = Menu {
            tab: 2,
            ..Default::default()
        };
        renderer.ui_reset();
        menu.draw(&mut renderer, &settings);
        renderer.render_to_png_hud(
            &device,
            &queue,
            "captures/graphics_color_gpu_review_20261001.png",
            1280,
            720,
        )?;
        device.poll(wgpu::PollType::wait_indefinitely())?;
        assert!(pollster::block_on(device.pop_error_scope()).is_none());
        Ok(())
    }
}
