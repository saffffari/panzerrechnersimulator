//! Persistent paper albedo. Ink is rendered once when it changes, then ordinary scene geometry
//! projects and lights it; there is no screen-space paper, pixel readback or per-frame texture cook.

use std::sync::Arc;

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 768;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(super) struct MapSurface {
    view: wgpu::TextureView,
    material: Arc<wgpu::BindGroup>,
    last_ink: Option<u64>,
}

impl MapSurface {
    fn new(renderer: &super::Renderer, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("held-paper-cartography"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mr = super::upload_texture(
            device,
            queue,
            "paper-mr",
            wgpu::TextureFormat::Rgba8Unorm,
            [255, 255, 0, 255],
            &None,
        );
        let normal = super::upload_texture(
            device,
            queue,
            "paper-normal",
            wgpu::TextureFormat::Rgba8Unorm,
            [128, 128, 255, 255],
            &None,
        );
        let material = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("held-paper-material"),
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
        }
    }
}

impl super::Renderer {
    /// Consume only commands queued by ui::draw_held_map_print, before drawing the scene/HUD.
    /// One 3 MiB target is reused for this renderer's lifetime. Stationary paper and mouse incur
    /// no GPU work; changing pencil strokes, pins, route sheets or the cursor repaints the albedo.
    pub fn render_held_map_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        slot: usize,
    ) {
        assert!(
            slot < self.meshes.len(),
            "held map slot must belong to this scene"
        );
        if self.held_map_surface.is_none() {
            self.held_map_surface = Some(MapSurface::new(self, device, queue));
        }
        let key = self.ui.queued_content_fingerprint();
        let surface = self.held_map_surface.as_mut().unwrap();
        self.meshes[slot].tex_bind = Arc::clone(&surface.material);
        if surface.last_ink == Some(key) {
            self.ui.clear();
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("held-paper-ink"),
        });
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("held-paper-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.78,
                            g: 0.73,
                            b: 0.60,
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
}
