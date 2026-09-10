//! pixel.Kpfw — a 400×300 mini workbench that packages the Kessel "crush" (downscale → cold grade →
//! ordered Bayer dither → posterize) behind live sliders, with a Tiger I for a mascot/icon.
//!
//! The effect is the same one the game bakes into portraits and title art (see the engine's
//! `pixelate.rs`), ported here so it can be dialled interactively. Output tone is clamped into a
//! narrow band — a few points ABOVE black for the fill and a few points BELOW white for the elements
//! — which is also the app's own palette (near-black panels, near-white controls).
//!
//! Drop any image onto the window to process it; the embedded Tiger (Bundesarchiv Nordfrankreich) is
//! the default. "Speichern" writes the crushed PNG (plus a ×4 nearest upscale) next to the app.

#![windows_subsystem = "windows"]

use eframe::egui;
use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use image::{ImageBuffer, Rgba, RgbaImage};

const SRC_TIGER: &[u8] = include_bytes!("../assets/tiger.jpg");
const APP_ICON: &[u8] = include_bytes!("../assets/icon.png");

// 4×4 ordered Bayer — identical to shaders/crush.wgsl and the engine's pixelate.rs.
const BAYER: [f32; 16] = [
    0.0, 8.0, 2.0, 10.0,
    12.0, 4.0, 14.0, 6.0,
    3.0, 11.0, 1.0, 9.0,
    15.0, 7.0, 13.0, 5.0,
];

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn grain_hash(x: u32, y: u32) -> f32 {
    let n = x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263));
    let n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65_535.0
}

/// The crush grade for one pixel (cold shadow lift → S-curve contrast → Bayer dither → posterize →
/// static grain), returning a value in [0,1]. Matches `pixelate::crush_pixel`.
fn crush_pixel(mut m: [f32; 3], x: u32, y: u32, levels: f32, dither: f32, grain: f32) -> [f32; 3] {
    let luma = 0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2];
    let shadow = 1.0 - smoothstep(0.0, 0.5, luma);
    let lift = [-0.01, 0.01, 0.05];
    for i in 0..3 {
        m[i] = (m[i] + lift[i] * shadow).clamp(0.0, 1.0);
        m[i] = ((m[i] - 0.5) * 1.15 + 0.5).clamp(0.0, 1.0);
    }
    let bi = ((y & 3) * 4 + (x & 3)) as usize;
    let ordered = (BAYER[bi] + 0.5) / 16.0 - 0.5;
    for i in 0..3 {
        m[i] += ordered * dither / levels.max(1.0);
        m[i] = (m[i] * levels + 0.5).floor() / levels;
    }
    let g = (grain_hash(x, y) - 0.5) * grain;
    for i in 0..3 {
        m[i] = (m[i] + g).clamp(0.0, 1.0);
    }
    m
}

#[derive(Clone)]
struct Params {
    width: u32,     // downscale target width (lower = chunkier)
    levels: u32,    // posterize steps / channel
    dither: f32,    // Bayer strength
    grain: f32,     // static film grain amplitude
    black_lift: u8, // output floor: fill sits this many 8-bit points above black
    white_drop: u8, // output ceiling: elements sit this many points below white
    desaturate: bool,
}

impl Default for Params {
    fn default() -> Self {
        // "5 points above black for fill, 5 below white for the elements."
        Self { width: 96, levels: 5, dither: 1.0, grain: 0.03, black_lift: 5, white_drop: 5, desaturate: true }
    }
}

/// Crush `src` to a small buffer per the params. Preserves aspect; remaps the final tone into
/// [black_lift, 255-white_drop].
fn process(src: &RgbaImage, p: &Params) -> RgbaImage {
    let w = p.width.max(2);
    let h = ((w as u64 * src.height() as u64) / src.width().max(1) as u64).max(1) as u32;
    let small = image::imageops::resize(src, w, h, image::imageops::FilterType::CatmullRom);

    let floor = p.black_lift as f32;
    let span = (255.0 - p.white_drop as f32 - floor).max(1.0);
    let levels = p.levels.max(2) as f32;

    let mut out: RgbaImage = ImageBuffer::new(w, h);
    for (x, y, px) in small.enumerate_pixels() {
        let mut m = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
        if p.desaturate {
            let l = 0.299 * m[0] + 0.587 * m[1] + 0.114 * m[2];
            m = [l, l, l];
        }
        let c = crush_pixel(m, x, y, levels, p.dither, p.grain);
        let map = |v: f32| (floor + v * span).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([map(c[0]), map(c[1]), map(c[2]), px[3]]));
    }
    out
}

fn to_color_image(img: &RgbaImage) -> ColorImage {
    let size = [img.width() as usize, img.height() as usize];
    ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}

struct App {
    src: RgbaImage,
    params: Params,
    tex: Option<TextureHandle>,
    processed: RgbaImage,
    dirty: bool,
    status: String,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let src = image::load_from_memory(SRC_TIGER)
            .expect("embedded tiger decodes")
            .to_rgba8();
        let params = Params::default();
        let processed = process(&src, &params);
        Self { src, params, tex: None, processed, dirty: true, status: String::new() }
    }

    fn recompute(&mut self, ctx: &egui::Context) {
        self.processed = process(&self.src, &self.params);
        let ci = to_color_image(&self.processed);
        match &mut self.tex {
            Some(t) => t.set(ci, TextureOptions::NEAREST),
            None => self.tex = Some(ctx.load_texture("preview", ci, TextureOptions::NEAREST)),
        }
        self.dirty = false;
    }

    fn take_dropped(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            if let Some(path) = f.path {
                match image::open(&path) {
                    Ok(img) => {
                        self.src = img.to_rgba8();
                        self.dirty = true;
                        self.status = format!("Geladen: {}", path.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
                    }
                    Err(e) => self.status = format!("Fehler: {e}"),
                }
            }
        }
    }

    fn save(&mut self) {
        let dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let out = dir.join("pixelkpfw_out.png");
        let up = dir.join("pixelkpfw_out.x4.png");
        let r1 = self.processed.save(&out);
        let scaled = image::imageops::resize(
            &self.processed, self.processed.width() * 4, self.processed.height() * 4,
            image::imageops::FilterType::Nearest,
        );
        let r2 = scaled.save(&up);
        self.status = match (r1, r2) {
            (Ok(_), Ok(_)) => format!("Gespeichert → {}", out.display()),
            _ => "Speichern fehlgeschlagen".into(),
        };
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.take_dropped(ctx);

        egui::SidePanel::left("controls")
            .exact_width(188.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(Color32::from_gray(5)).inner_margin(8.0))
            .show(ctx, |ui| {
                // Keep the slider bar short so its value box + label stay on one line inside the panel.
                ui.spacing_mut().slider_width = 66.0;
                ui.heading("pixel.Kpfw");
                ui.label(egui::RichText::new("Kessel-Crush").small().weak());
                ui.separator();

                let p = &mut self.params;
                let mut changed = false;
                changed |= ui.add(egui::Slider::new(&mut p.width, 12..=300).text("Raster")).changed();
                changed |= ui.add(egui::Slider::new(&mut p.levels, 2..=12).text("Stufen")).changed();
                changed |= ui.add(egui::Slider::new(&mut p.dither, 0.0..=1.0).text("Dither")).changed();
                changed |= ui.add(egui::Slider::new(&mut p.grain, 0.0..=0.10).text("Korn")).changed();
                ui.add_space(2.0);
                changed |= ui.add(egui::Slider::new(&mut p.black_lift, 0..=20).text("Füllung")).changed();
                changed |= ui.add(egui::Slider::new(&mut p.white_drop, 0..=20).text("Elemente")).changed();
                changed |= ui.checkbox(&mut p.desaturate, "Entsättigen").changed();
                if changed { self.dirty = true; }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Speichern").clicked() { self.save(); }
                    if ui.button("Zurücksetzen").clicked() {
                        self.params = Params::default();
                        self.dirty = true;
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Bild hierher ziehen").small().weak());
                if !self.status.is_empty() {
                    ui.label(egui::RichText::new(&self.status).small());
                }
            });

        if self.dirty { self.recompute(ctx); }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_gray(5)).inner_margin(6.0))
            .show(ctx, |ui| {
            if let Some(tex) = &self.tex {
                let avail = ui.available_size();
                let (iw, ih) = (self.processed.width() as f32, self.processed.height() as f32);
                let scale = (avail.x / iw).min(avail.y / ih).max(0.01);
                let size = egui::vec2(iw * scale, ih * scale);
                // Centre the nearest-upscaled preview.
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                });
            }
        });
    }
}

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(APP_ICON).expect("icon decodes").to_rgba8();
    let (width, height) = (img.width(), img.height());
    egui::IconData { rgba: img.into_raw(), width, height }
}

fn main() -> eframe::Result<()> {
    // App palette embodies the tone rule: fill a few points above black, elements a few below white.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([400.0, 300.0])
            .with_max_inner_size([400.0, 300.0])
            .with_resizable(false)
            .with_title("pixel.Kpfw")
            // app_id must match the .desktop file's basename so Wayland/KWin shows the Tiger icon.
            .with_app_id("pixel-kpfw")
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "pixel.Kpfw",
        options,
        Box::new(|cc| {
            let mut v = egui::Visuals::dark();
            v.panel_fill = Color32::from_gray(5);
            v.window_fill = Color32::from_gray(5);
            v.extreme_bg_color = Color32::from_gray(5);
            v.faint_bg_color = Color32::from_gray(12);
            v.override_text_color = Some(Color32::from_gray(250));
            cc.egui_ctx.set_visuals(v);
            Ok(Box::new(App::new(cc)))
        }),
    )
}
