//! Cached ink for four physical book faces. Print is ordinary non-emissive albedo, never a HUD
//! rectangle. The turning leaf and stationary pages retain separate surfaces during a page turn.
use std::sync::Arc;

/// A5 landscape at 248 dpi: 2048 x 1448 (was 768 x 1024 portrait), so a plate keeps its
/// line weight through the page and the loupe; four retained surfaces, 47 MB.
const WIDTH: u32 = 2048;
const HEIGHT: u32 = 1448;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(super) struct PageSurface {
    view: wgpu::TextureView,
    material: Arc<wgpu::BindGroup>,
    last_ink: Option<u64>,
    /// The same ink in the display-resolution image registry, for the top-down overlay.
    ui_handle: u32,
}

impl PageSurface {
    fn new(renderer: &mut super::Renderer, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("repair-manual-page"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let ui_handle = renderer.ui.import_image_view(device, &view);
        let mr = super::upload_texture(
            device,
            queue,
            "manual-paper-mr",
            wgpu::TextureFormat::Rgba8Unorm,
            [255, 255, 0, 255],
            &None,
        );
        let normal = super::upload_texture(
            device,
            queue,
            "manual-paper-normal",
            wgpu::TextureFormat::Rgba8Unorm,
            [128, 128, 255, 255],
            &None,
        );
        let material = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("repair-manual-page-material"),
            layout: &renderer.material_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
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
                    resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                },
            ],
        }));
        Self {
            view,
            material,
            last_ink: None,
            ui_handle,
        }
    }
}

impl super::Renderer {
    /// The four page faces' ink as display-resolution images (left, right, the turning leaf's
    /// front and back), once the book has been painted: what the top-down overlay draws.
    pub fn manual_page_images(&self) -> Option<[u32; 4]> {
        (self.repair_manual_surfaces.len() >= 4)
            .then(|| std::array::from_fn(|i| self.repair_manual_surfaces[i].ui_handle))
    }
    pub fn debug_dump_manual_albedo(&self,device:&wgpu::Device,queue:&wgpu::Queue,
        path:&str)->anyhow::Result<()> {
        self.raymarch.as_ref().ok_or_else(||anyhow::anyhow!("manual albedo requires raymarch backend"))?
            .debug_albedo_png(device,queue,path)
    }
    pub fn debug_dump_manual_ink(&self, device: &wgpu::Device, queue: &wgpu::Queue,
        path: &str) -> anyhow::Result<()> {
        let surface = self.repair_manual_surfaces.first()
            .ok_or_else(|| anyhow::anyhow!("manual ink has not been rendered"))?;
        let bytes = super::debug_read_light_target(device,queue,surface.view.texture())?;
        image::save_buffer(path,&bytes,WIDTH,HEIGHT,image::ColorType::Rgba8)?;
        Ok(())
    }
    /// Read-only capture diagnostics. Never called by the live frame loop.
    pub fn debug_manual_lighting(&self, device: &wgpu::Device, queue: &wgpu::Queue)
        -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({"eye":self.camera.pos.to_array(),
            "forward":self.camera.forward().to_array(),
            "lights":self.lights.iter().map(|l| serde_json::json!({"pos":l.pos.to_array(),
                "power":l.radiant_intensity,"color":l.color.to_array()})).collect::<Vec<_>>(),
            "surfaces":self.raymarch.as_ref().map(|b| b.debug_surface_samples(device,queue)).transpose()?}))
    }
    /// Call before scene/HUD drawing. These page textures are retained across display resize and
    /// repainted only when their ink changes; the turn animation changes transforms, not textures.
    pub fn render_repair_manual_pages(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slots: [usize; crate::repair_manual::PART_COUNT],
        state: &crate::repair_manual::ManualState,
    ) {
        // The overlay book is paper over the frame, not in it: its pages never enter the
        // scene's light history, so a page turn there resets nothing.
        let in_scene = !crate::repair_manual::overlay_mode();
        if !state.held {
            if self.repair_manual_presentation.take().is_some() && in_scene {
                self.reset_history();
            }
            return;
        }
        let presentation = state.presentation_key();
        if self.repair_manual_presentation != Some(presentation) {
            // Ink is albedo. The raymarcher's indirect-light history contains the previous
            // printed surface, so retaining it literally prints the old words over the new
            // page. Pose changes also invalidate that surface correspondence. Camera motion
            // alone is deliberately absent from this key and keeps normal temporal lighting.
            if in_scene {
                self.reset_history();
            }
            self.repair_manual_presentation = Some(presentation);
        }
        assert!(
            slots.iter().all(|&slot| slot < self.meshes.len()),
            "manual slots must belong to scene"
        );
        while self.repair_manual_surfaces.len() < 4 {
            let surface = PageSurface::new(self, device, queue);
            self.repair_manual_surfaces.push(surface);
        }
        let pages = state.page_bindings();
        for index in 0..4 {
            if !state.visible_part(index + 1) {
                continue;
            }
            self.ui_reset();
            crate::repair_manual::draw_page(self, pages[index], state);
            let key = self.ui.queued_content_fingerprint();
            let surface = &mut self.repair_manual_surfaces[index];
            self.meshes[slots[index + 1]].tex_bind = Arc::clone(&surface.material);
            if surface.last_ink == Some(key) {
                self.ui.clear();
                continue;
            }
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("repair-manual-ink"),
            });
            {
                let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("repair-manual-page-clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &surface.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.90,
                                g: 0.87,
                                b: 0.76,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            self.ui
                .render_over(device, &mut encoder, &surface.view, FORMAT, WIDTH, HEIGHT);
            queue.submit(Some(encoder.finish()));
            surface.last_ink = Some(key);
        }
        self.ui.clear();
    }
}
