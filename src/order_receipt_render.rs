//! Ink for the printed strips (src/order_printer.rs): one retained 1:3 albedo per strip mesh
//! (the strip on the machine, the strip in hand), each repainted only when its text changes.
//! Print is ordinary non-emissive albedo under the cabin's and the camp's real light, never a HUD.
use std::sync::Arc;

/// The sheet's ink is 1.4:1 like the paper (210 x 150 mm), so glyphs keep their shape.
pub const RECEIPT_INK_WIDTH: u32 = 1024;
pub const RECEIPT_INK_HEIGHT: u32 = 731;
/// Characters per printed line on the 210 mm sheet (a page teleprinter's ten to the inch,
/// inside 20 mm margins, set in the manual's grotesque so the crush can read it).
pub const RECEIPT_CHARS_PER_LINE: usize = 56;
const WIDTH: u32 = RECEIPT_INK_WIDTH;
const HEIGHT: u32 = RECEIPT_INK_HEIGHT;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// What a strip says: the title, the small print, the wrapped lines, and how many of them
/// (title and small print counted as the first two) the machine has printed so far.
pub struct ReceiptInk<'a> {
    pub title: &'a str,
    pub heading: &'a str,
    pub lines: &'a [String],
    pub printed: usize,
}

pub(super) struct ReceiptSurface {
    view: wgpu::TextureView,
    material: Arc<wgpu::BindGroup>,
    last_ink: Option<u64>,
    /// The same ink in the image registry (nearest-sampled), for the top-down overlay.
    ui_handle: u32,
    /// The texture's size: the print sheet, or the pixel sheet in internal pixels.
    size: (u32, u32),
}

/// The pixel sheet: this share of the internal grid's width, 1.4:1 like the paper.
pub const SHEET_OVERLAY_SHARE: f32 = 0.46;
pub fn sheet_pixels(grid: (u32, u32)) -> (u32, u32) {
    let w = (SHEET_OVERLAY_SHARE * grid.0.max(64) as f32).round().max(64.0);
    (w as u32, (w / 1.4).round().max(48.0) as u32)
}

impl ReceiptSurface {
    fn new(renderer: &mut super::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, label: &str, size: (u32, u32)) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let ui_handle = renderer.ui.import_image_view(device, &view);
        let mr = super::upload_texture(device, queue, "order-strip-mr", wgpu::TextureFormat::Rgba8Unorm, [255, 255, 0, 255], &None);
        let normal = super::upload_texture(device, queue, "order-strip-normal", wgpu::TextureFormat::Rgba8Unorm, [128, 128, 255, 255], &None);
        let material = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("order-strip-material"),
            layout: &renderer.material_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&mr) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&normal) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&renderer.sampler) },
            ],
        }));
        Self { view, material, last_ink: None, ui_handle, size }
    }
}

/// The teleprinter's print on the pixel sheet: the 5 x 7 pixel face, the title two dots, one-
/// pixel rules, the lines re-wrapped to the sheet's width. Only `printed` lines carry ink.
pub fn draw_receipt_pixel(r: &mut super::Renderer, ink: &ReceiptInk, size: (u32, u32)) {
    use crate::pixel_font::{draw_at, width_px};
    let (fw, fh) = (size.0.max(64) as f32, size.1.max(48) as f32);
    let paper = [0.89, 0.87, 0.80, 1.0];
    let black = [0.10, 0.10, 0.11, 1.0];
    let faded = [0.32, 0.30, 0.27, 1.0];
    r.fill_rect(0.0, 0.0, 1.0, 1.0, paper);
    let m: i32 = 10;
    let wrap = (((size.0 as i32 - 2 * m) / 6).max(10) as usize).min(RECEIPT_CHARS_PER_LINE);
    let rule = |r: &mut super::Renderer, y: i32| r.fill_rect(m as f32 / fw, y as f32 / fh, (size.0 as i32 - 2 * m) as f32 / fw, 1.0 / fh, faded);
    let mut y = 8;
    let mut n = 0;
    if ink.printed > n {
        draw_at(r, ink.title, m, y, 2, black, size);
        y += 18;
    }
    n += 1;
    if ink.printed > n {
        rule(r, y);
        y += 4;
        for line in crate::ui::wrap_text(ink.heading, wrap) { draw_at(r, &line, m, y, 1, faded, size); y += 9; }
        rule(r, y + 1);
        y += 6;
    }
    n += 1;
    for line in ink.lines {
        if ink.printed <= n { break; }
        if line.is_empty() { y += 4; } else {
            for part in crate::ui::wrap_text(line, wrap) { draw_at(r, &part, m, y, 1, black, size); y += 9; }
        }
        n += 1;
    }
    if ink.printed > ink.lines.len() + 1 {
        let mark = "+ + +";
        draw_at(r, mark, (size.0 as i32 - width_px(mark, 1) as i32) / 2, y + 4, 1, faded, size);
    }
}

/// The torn-off sheet read top-down on the pixel grid: its retained ink one texel per pixel,
/// risen from the foot of the frame by `rise` (0 to 1), a hard offset shadow, its two torn
/// edges as pixel teeth, the paper scaled by the reading light at hand (the overlay is not lit
/// by the scene; a named scalar, docs/PHYSICAL_TRUTH_AUDIT.md).
pub fn draw_order_sheet_overlay(r: &mut super::Renderer, handle: u32, rise: f32, light: f32) {
    let grid = (r.iw, r.ih);
    let (iw, ih) = (grid.0.max(64) as f32, grid.1.max(64) as f32);
    let light = light.clamp(0.0, 1.0);
    let (w, h) = sheet_pixels(grid);
    let (w, h) = (w as i32, h as i32);
    let u = ((ih / 360.0).round() as i32).max(1);
    let p = rise.clamp(0.0, 1.0);
    let ease = p * p * (3.0 - 2.0 * p);
    let x = (grid.0 as i32 - w) / 2;
    let y = (0.15 * ih + (1.0 - ease) * ih).round() as i32;
    let paper = [0.89 * light, 0.87 * light, 0.80 * light, 1.0];
    let rect = |r: &mut super::Renderer, x: i32, y: i32, w: i32, h: i32, c: [f32; 4]| {
        r.fill_rect(x as f32 / iw, y as f32 / ih, w as f32 / iw, h as f32 / ih, c);
    };
    rect(r, x + 3 * u, y + 3 * u, w, h, [0.0, 0.0, 0.0, 0.5]);
    r.draw_image(handle, x as f32 / iw, y as f32 / ih, w as f32 / iw, h as f32 / ih, [light, light, light, 1.0]);
    // The torn edges: teeth of paper past the top and the foot of the print (the roll was torn
    // at the slot both times), their heights from the project's counter-seeded hash.
    let hash = |n: u32| ((n.wrapping_mul(2654435761) >> 8) & 0xff) as f32 / 255.0;
    let tooth = 3 * u;
    let mut k = 0u32;
    let mut tx = x;
    while tx < x + w {
        let tw = tooth.min(x + w - tx);
        let top = u * (1 + (hash(k + 11) * 2.0) as i32);
        let foot = u * (1 + (hash(k + 97) * 2.0) as i32);
        rect(r, tx, y - top, tw, top, paper);
        rect(r, tx, y + h, tw, foot, paper);
        tx += tooth;
        k += 1;
    }
    r.request_pixel_ink();
}

/// The teleprinter's print: capitals, a rule under the title, a rule between the small print
/// and the body, the lines in the order they came out. Only `printed` lines carry ink; the rest
/// of the paper is blank until the machine reaches them.
pub fn draw_receipt(r: &mut super::Renderer, ink: &ReceiptInk) {
    let paper = [0.89, 0.87, 0.80, 1.0];
    let black = [0.10, 0.10, 0.11, 1.0];
    let faded = [0.32, 0.30, 0.27, 1.0];
    r.fill_rect(0.0, 0.0, 1.0, 1.0, paper);
    // A line is 0.042 of the sheet's height (6.3 mm of paper), the type 0.034 (5 mm).
    let line_h = 0.042;
    let mut y = 0.030;
    let mut n = 0;
    let row = |r: &mut super::Renderer, text: &str, bold: bool, color: [f32; 4], y: &mut f32| {
        for line in crate::ui::wrap_text(text, RECEIPT_CHARS_PER_LINE) {
            let face = if bold { crate::ui::FACE_MANUAL_BOLD } else { crate::ui::FACE_MANUAL };
            r.draw_text_face(&line, 0.07, *y, if bold { 0.040 } else { 0.036 }, color, crate::ui::Align::Left, face);
            *y += line_h;
        }
    };
    if ink.printed > n { row(r, ink.title, true, black, &mut y); }
    n += 1;
    if ink.printed > n {
        r.fill_rect(0.07, y + 0.004, 0.86, 0.002, faded);
        y += 0.012;
        row(r, ink.heading, false, faded, &mut y);
        r.fill_rect(0.07, y + 0.004, 0.86, 0.002, faded);
        y += 0.016;
    }
    n += 1;
    for line in ink.lines {
        if ink.printed <= n { break; }
        if line.is_empty() { y += line_h * 0.5; } else { row(r, line, false, black, &mut y); }
        n += 1;
    }
    if ink.printed > ink.lines.len() + 1 {
        // The machine's end-of-message mark.
        r.draw_text_face("+ + +", 0.5, y + 0.008, 0.034, faded, crate::ui::Align::Center, crate::ui::FACE_MANUAL);
    }
}

impl super::Renderer {
    /// The two strips' ink as display-resolution images (the strip on the machine, the strip in
    /// hand), once painted: what the top-down sheet overlay draws.
    pub fn order_sheet_images(&self) -> Option<[u32; 2]> {
        (self.order_receipt_surfaces.len() >= 2)
            .then(|| [self.order_receipt_surfaces[0].ui_handle, self.order_receipt_surfaces[1].ui_handle])
    }
    /// Paint each strip's ink when it changed and bind it to its mesh: `inks[0]` for the strip
    /// on the machine, `inks[1]` for the strip in hand. Stationary paper costs nothing; a new
    /// line repaints one 1.7 MB target once.
    pub fn render_order_strip_ink(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, slots: [usize; 2], inks: [Option<ReceiptInk>; 2]) {
        assert!(slots.iter().all(|&slot| slot < self.meshes.len()), "order strip slots must belong to this scene");
        // The pixel sheet is exactly the internal pixels it covers; the print sheet 1024 wide.
        let pixel = crate::repair_manual::overlay_mode();
        let size = if pixel { sheet_pixels((self.iw, self.ih)) } else { (WIDTH, HEIGHT) };
        if self.order_receipt_surfaces.first().is_some_and(|s| s.size != size) {
            self.order_receipt_surfaces.clear();
        }
        while self.order_receipt_surfaces.len() < 2 {
            let label = if self.order_receipt_surfaces.is_empty() { "order-strip-ink-machine" } else { "order-strip-ink-hand" };
            let surface = ReceiptSurface::new(self, device, queue, label, size);
            self.order_receipt_surfaces.push(surface);
        }
        for (index, ink) in inks.iter().enumerate() {
            let Some(ink) = ink else { continue };
            self.ui_reset();
            if pixel { draw_receipt_pixel(self, ink, size); } else { draw_receipt(self, ink); }
            let key = self.ui.queued_content_fingerprint();
            let changed = self.order_receipt_surfaces[index].last_ink != Some(key);
            if changed {
                // New ink is a new albedo under the temporal history, as a turned page is.
                self.reset_history();
            }
            let surface = &mut self.order_receipt_surfaces[index];
            self.meshes[slots[index]].tex_bind = Arc::clone(&surface.material);
            if !changed {
                self.ui.clear();
                continue;
            }
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("order-strip-ink") });
            {
                let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("order-strip-clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.89, g: 0.87, b: 0.80, a: 1.0 }), store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            self.ui.render_over(device, &mut encoder, &surface.view, FORMAT, size.0, size.1);
            queue.submit(Some(encoder.finish()));
            surface.last_ink = Some(key);
        }
        self.ui.clear();
    }
}
