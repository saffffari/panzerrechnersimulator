//! Ink for the printed strips (src/order_printer.rs): one retained 1:3 albedo per strip mesh
//! (the strip on the machine, the strip in hand), each repainted only when its text changes.
//! Print is ordinary non-emissive albedo under the cabin's and the camp's real light, never a HUD.
use std::sync::Arc;

/// The strip's ink is 1:3 like the paper (76 x 228 mm), so glyphs keep their shape.
pub const RECEIPT_INK_WIDTH: u32 = 384;
pub const RECEIPT_INK_HEIGHT: u32 = 1152;
/// Characters per printed line on the 76 mm strip.
pub const RECEIPT_CHARS_PER_LINE: usize = 24;
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
}

impl ReceiptSurface {
    fn new(renderer: &super::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, label: &str) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
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
        Self { view, material, last_ink: None }
    }
}

/// The teleprinter's print: capitals, a rule under the title, a rule between the small print
/// and the body, the lines in the order they came out. Only `printed` lines carry ink; the rest
/// of the paper is blank until the machine reaches them.
pub fn draw_receipt(r: &mut super::Renderer, ink: &ReceiptInk) {
    let paper = [0.89, 0.87, 0.80, 1.0];
    let black = [0.10, 0.10, 0.11, 1.0];
    let faded = [0.32, 0.30, 0.27, 1.0];
    r.fill_rect(0.0, 0.0, 1.0, 1.0, paper);
    // The strip is 1:3; a line is 0.026 of its height (6 mm of paper), the type 0.021 (4.8 mm).
    let line_h = 0.026;
    let mut y = 0.018;
    let mut n = 0;
    let row = |r: &mut super::Renderer, text: &str, bold: bool, color: [f32; 4], y: &mut f32| {
        for line in crate::ui::wrap_text(text, RECEIPT_CHARS_PER_LINE) {
            if bold { r.draw_text_bold(&line, 0.06, *y, 0.019, color, crate::ui::Align::Left); }
            else { r.draw_text(&line, 0.06, *y, 0.021, color, crate::ui::Align::Left); }
            *y += line_h;
        }
    };
    if ink.printed > n { row(r, ink.title, true, black, &mut y); }
    n += 1;
    if ink.printed > n {
        r.fill_rect(0.06, y + 0.002, 0.88, 0.0012, faded);
        y += 0.008;
        row(r, ink.heading, false, faded, &mut y);
        r.fill_rect(0.06, y + 0.004, 0.88, 0.0012, faded);
        y += 0.012;
    }
    n += 1;
    for line in ink.lines {
        if ink.printed <= n { break; }
        if line.is_empty() { y += line_h * 0.5; } else { row(r, line, false, black, &mut y); }
        n += 1;
    }
    if ink.printed > ink.lines.len() + 1 {
        // The machine's end-of-message mark.
        r.draw_text("+ + +", 0.5, y + 0.006, 0.021, faded, crate::ui::Align::Center);
    }
}

impl super::Renderer {
    /// Paint each strip's ink when it changed and bind it to its mesh: `inks[0]` for the strip
    /// on the machine, `inks[1]` for the strip in hand. Stationary paper costs nothing; a new
    /// line repaints one 1.7 MB target once.
    pub fn render_order_strip_ink(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, slots: [usize; 2], inks: [Option<ReceiptInk>; 2]) {
        assert!(slots.iter().all(|&slot| slot < self.meshes.len()), "order strip slots must belong to this scene");
        while self.order_receipt_surfaces.len() < 2 {
            let label = if self.order_receipt_surfaces.is_empty() { "order-strip-ink-machine" } else { "order-strip-ink-hand" };
            let surface = ReceiptSurface::new(self, device, queue, label);
            self.order_receipt_surfaces.push(surface);
        }
        for (index, ink) in inks.iter().enumerate() {
            let Some(ink) = ink else { continue };
            self.ui_reset();
            draw_receipt(self, ink);
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
            self.ui.render_over(device, &mut encoder, &surface.view, FORMAT, WIDTH, HEIGHT);
            queue.submit(Some(encoder.finish()));
            surface.last_ink = Some(key);
        }
        self.ui.clear();
    }
}
