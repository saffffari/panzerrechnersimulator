//! Headless display comparisons from one already-rendered HDR/depth/particle frame.
//! No world update, transport pass, bloom pass, eye metering, history reset or frame increment.
use super::*;
use anyhow::{ensure, Context};
use serde_json::json;

#[derive(Clone, Copy)]
struct Look {
    key: &'static str,
    label: &'static str,
    crush: CrushUniform,
}
fn looks(base: CrushUniform) -> [Look; 6] {
    let mut clean = base;
    clean.retro = 0.0;
    clean.ldr_dither = 0.0;
    let mut mild = base;
    mild.levels = 31.0;
    mild.dither = 0.35;
    mild.retro = 0.65;
    mild.ldr_dither = 1.0;
    let mut ordered15 = base;
    ordered15.levels = 15.0;
    ordered15.dither = 0.80;
    ordered15.retro = 1.0;
    ordered15.ldr_dither = 1.0;
    let mut ordered7 = base;
    ordered7.levels = 7.0;
    ordered7.dither = 1.0;
    ordered7.retro = 1.0;
    ordered7.ldr_dither = 1.0;
    let mut balanced = base;
    balanced.display_grade[0] *= 1.08;
    balanced.display_grade[1] *= 1.06;
    balanced.display_grade[2] += 0.025;
    [
        Look {
            key: "current",
            label: "Current display treatment",
            crush: base,
        },
        Look {
            key: "clean",
            label: "Clean continuous tone; no ordered or LDR dither",
            crush: clean,
        },
        Look {
            key: "mild31",
            label: "31 levels; ordered0.35; treatment0.65",
            crush: mild,
        },
        Look {
            key: "ordered15",
            label: "15 levels; ordered0.80; full treatment",
            crush: ordered15,
        },
        Look {
            key: "ordered7",
            label: "7 levels; ordered1.00; full treatment",
            crush: ordered7,
        },
        Look {
            key: "balanced",
            label: "Current treatment; reproduction exposure×1.08 saturation×1.06 warmth+0.025",
            crush: balanced,
        },
    ]
}
fn read_buffer(device: &wgpu::Device, buffer: &wgpu::Buffer) -> anyhow::Result<Vec<u8>> {
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let bytes = slice.get_mapped_range().to_vec();
    buffer.unmap();
    Ok(bytes)
}
impl Renderer {
    /// Re-run only the actual display stage against the most recently rendered native frame.
    /// The original GPU eye response (including rods) remains bit-identical for all six looks.
    /// Both the CPU settings and the GPU crush uniform are preserved after this diagnostic.
    pub(crate) fn debug_capture_frozen_display_looks(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prefix: &str,
    ) -> anyhow::Result<serde_json::Value> {
        ensure!(
            !self.menu_mode,
            "look comparison requires an already-rendered world frame"
        );
        let uniform_size = std::mem::size_of::<CrushUniform>() as u64;
        let uniform_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("look-review-original-uniform"),
            size: uniform_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let native_extent = wgpu::Extent3d {
            width: self.iw,
            height: self.ih,
            depth_or_array_layers: 1,
        };
        let original_ldr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("look-review-original-ldr"),
            size: native_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("look-review-snapshot"),
        });
        encoder.copy_texture_to_texture(
            self.ldr_tex.as_image_copy(),
            original_ldr.as_image_copy(),
            native_extent,
        );
        encoder.copy_buffer_to_buffer(&self.crush_buf, 0, &uniform_readback, 0, uniform_size);
        queue.submit([encoder.finish()]);
        let original = read_buffer(device, &uniform_readback)?;
        let base: CrushUniform = bytemuck::pod_read_unaligned(&original);
        let treatments = looks(base);
        for look in &treatments {
            ensure!(
                !std::path::Path::new(&format!("{prefix}_{}.png", look.key)).exists(),
                "look capture exists; use a fresh prefix"
            );
        }
        ensure!(
            !std::path::Path::new(&format!("{prefix}_matrix.png")).exists(),
            "look matrix exists"
        );
        let unchanged_frame = self.frame;
        let unchanged_time = self.time;
        let result = (|| -> anyhow::Result<serde_json::Value> {
            let format = wgpu::TextureFormat::Rgba8UnormSrgb;
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("look-review-native"),
                size: wgpu::Extent3d {
                    width: self.iw,
                    height: self.ih,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
            let unpadded = self.iw * 4;
            let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("look-review-native-readback"),
                size: (padded * self.ih) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut matrix = image::RgbaImage::new(self.iw * 3, self.ih * 2);
            let mut receipts = Vec::new();
            for (index, look) in treatments.iter().enumerate() {
                queue.write_buffer(&self.crush_buf, 0, bytemuck::bytes_of(&look.crush));
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("look-review-display-only"),
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("look-review-crush"),
                        color_attachments: &[Some(attachment(
                            &self.ldr_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        ))],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.crush_pipe);
                    pass.set_bind_group(0, &self.crush_bind, &[]);
                    pass.draw(0..3, 0..1);
                }
                // Preserve the actual last-frame particle positions and the ordinary display pass.
                // Reusing their existing buffers changes neither source geometry nor visibility.
                if self.pixel_particle_count.get() > 0 {
                    if let Some(pipeline) = &self.pixel_particle_pipe {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("look-review-frozen-world-particles"),
                            color_attachments: &[Some(attachment(
                                &self.ldr_view,
                                wgpu::LoadOp::Load,
                            ))],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, &self.pixel_particle_bind, &[]);
                        if self.shader_backend {
                            pass.set_bind_group(1, &self.sky_visibility.sampling_bind, &[]);
                            pass.set_bind_group(2, &self.cabin_shadows.sampling_bind, &[]);
                        } else {
                            pass.set_bind_group(
                                1,
                                self.combine_rt_bind
                                    .as_ref()
                                    .unwrap_or(&self.smoke_empty_visibility_bind),
                                &[],
                            );
                        }
                        pass.set_vertex_buffer(0, self.pixel_particle_vertices.slice(..));
                        pass.draw(0..6, 0..self.pixel_particle_count.get());
                    }
                }
                self.blit(device, &mut encoder, &output_view, format);
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &output,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded),
                            rows_per_image: Some(self.ih),
                        },
                    },
                    wgpu::Extent3d {
                        width: self.iw,
                        height: self.ih,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit([encoder.finish()]);
                let raw = read_buffer(device, &readback)?;
                let mut pixels = Vec::with_capacity((unpadded * self.ih) as usize);
                for row in raw.chunks_exact(padded as usize) {
                    pixels.extend_from_slice(&row[..unpadded as usize]);
                }
                let image = image::RgbaImage::from_raw(self.iw, self.ih, pixels)
                    .context("native look image shape")?;
                let path = format!("{prefix}_{}.png", look.key);
                ensure_output_parent(&path)?;
                image.save(&path)?;
                image::imageops::replace(
                    &mut matrix,
                    &image,
                    (index % 3) as i64 * self.iw as i64,
                    (index / 3) as i64 * self.ih as i64,
                );
                receipts.push(json!({"key":look.key,"label":look.label,"path":path,"matrix_cell":[index%3,index/3],
                    "palette_levels":look.crush.levels,"ordered_dither":look.crush.dither,"retro_strength":look.crush.retro,
                    "ldr_dither":look.crush.ldr_dither,"display_grade":look.crush.display_grade}));
            }
            let matrix_path = format!("{prefix}_matrix.png");
            matrix.save(&matrix_path)?;
            ensure!(
                self.frame == unchanged_frame && self.time.to_bits() == unchanged_time.to_bits(),
                "display review advanced renderer state"
            );
            Ok(
                json!({"schema":"panzer.frozen-display-looks.v1","native_resolution":[self.iw,self.ih],"matrix":matrix_path,"looks":receipts,
                "frozen_frame":unchanged_frame,"scene_time_s":unchanged_time,"camera":self.camera.pos.to_array(),
                "eye":{"cone_exposure":base.exposure,"rod_exposure":base.rod_exposure,"rod_inverse_saturation":base.rod_inverse_saturation},
                "scope":"One existing HDR/depth frame and original GPU eye mapping. Only crush and the existing frozen particle display overlay rerun. No scene, light, bloom, transport, exposure metering, simulation or audio advancement. Clean/mild/15/7/current/balanced are review options; saved defaults unchanged.",
                "limit":"Native display-stage stills omit output-resolution UI. This preserves the already-rendered sensor noise and convergence state; it does not establish transport convergence."}),
            )
        })();
        queue.write_buffer(&self.crush_buf, 0, &original);
        let mut restore = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("look-review-restore-native-frame"),
        });
        restore.copy_texture_to_texture(
            original_ldr.as_image_copy(),
            self.ldr_tex.as_image_copy(),
            native_extent,
        );
        queue.submit([restore.finish()]);
        result
    }
    /// Two output-native RGBA/sRGB frames from identical HDR, eye state, particles and camera.
    /// Index0 uses Bayer4; index1 fixed64blue-noise. The only uniform difference is byte56.
    /// Does not advance any simulation, light transport, camera, exposure or audio state.
    pub(crate) fn debug_capture_frozen_dither_pair(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<[Vec<u8>;2]> {
        self.debug_capture_frozen_dither_comparison(device, queue, 1)
    }

    /// Same frozen native frame: current Bayer4 versus blue noise (1), sphere Bayer8 (2), or native screen Bayer8 (3).
    /// Mode4 compares native screen Bayer8 with continuous tone, disabling both palette and LDR dither.
    /// Mode2 follows camera rotation through a world-oriented angular pattern and resolves four
    /// post-process threshold samples per native pixel. It does not supersample the scene.
    pub(crate) fn debug_capture_frozen_dither_comparison(
        &mut self, device: &wgpu::Device, queue: &wgpu::Queue, alternate_mode: u32,
    ) -> anyhow::Result<[Vec<u8>;2]> {
        ensure!((1..=4).contains(&alternate_mode), "unknown dither comparison mode");
        ensure!(
            !self.menu_mode,
            "look comparison requires an already-rendered world frame"
        );
        let uniform_size = std::mem::size_of::<CrushUniform>() as u64;
        let uniform_readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("look-review-original-uniform"),
            size: uniform_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let native_extent = wgpu::Extent3d {
            width: self.iw,
            height: self.ih,
            depth_or_array_layers: 1,
        };
        let original_ldr = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("look-review-original-ldr"),
            size: native_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: LDR,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("look-review-snapshot"),
        });
        encoder.copy_texture_to_texture(
            self.ldr_tex.as_image_copy(),
            original_ldr.as_image_copy(),
            native_extent,
        );
        encoder.copy_buffer_to_buffer(&self.crush_buf, 0, &uniform_readback, 0, uniform_size);
        queue.submit([encoder.finish()]);
        let original = read_buffer(device, &uniform_readback)?;
        let base: CrushUniform = bytemuck::pod_read_unaligned(&original);
        let mut ordered=base;ordered._grade_pad[0]=if alternate_mode==4 {3.0}else{0.0};
        let mut alternate=base;alternate._grade_pad[0]=alternate_mode as f32;
        if alternate_mode==4 {alternate.retro=0.0;alternate.ldr_dither=0.0;}
        let (key,label)=match alternate_mode {
            4 => ("continuous","Continuous tone, no palette or LDR dither"),
            3 => ("screen_bayer8","Native screen Bayer8"),
            2 => ("sphere_bayer8","Camera-stable sphere Bayer8"),
            _ => ("blue_noise64","fixedblue64"),
        };
        let treatments=[Look{key:if alternate_mode==4 {"screen_bayer8"}else{"bayer4"},
            label:if alternate_mode==4 {"Native screen Bayer8"}else{"Bayer4"},crush:ordered},
            Look{key,label,crush:alternate}];
        let unchanged_frame = self.frame;
        let unchanged_time = self.time;
        let result = (|| -> anyhow::Result<[Vec<u8>;2]> {
            let format = wgpu::TextureFormat::Rgba8UnormSrgb;
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("look-review-native"),
                size: wgpu::Extent3d {
                    width: self.iw,
                    height: self.ih,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
            let unpadded = self.iw * 4;
            let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("look-review-native-readback"),
                size: (padded * self.ih) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut frames=[Vec::new(),Vec::new()];
            for (index, look) in treatments.iter().enumerate() {
                queue.write_buffer(&self.crush_buf, 0, bytemuck::bytes_of(&look.crush));
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("look-review-display-only"),
                });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("look-review-crush"),
                        color_attachments: &[Some(attachment(
                            &self.ldr_view,
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        ))],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.crush_pipe);
                    pass.set_bind_group(0, &self.crush_bind, &[]);
                    pass.draw(0..3, 0..1);
                }
                // Preserve the actual last-frame particle positions and the ordinary display pass.
                // Reusing their existing buffers changes neither source geometry nor visibility.
                if self.pixel_particle_count.get() > 0 {
                    if let Some(pipeline) = &self.pixel_particle_pipe {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("look-review-frozen-world-particles"),
                            color_attachments: &[Some(attachment(
                                &self.ldr_view,
                                wgpu::LoadOp::Load,
                            ))],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        pass.set_pipeline(pipeline);
                        pass.set_bind_group(0, &self.pixel_particle_bind, &[]);
                        if self.shader_backend {
                            pass.set_bind_group(1, &self.sky_visibility.sampling_bind, &[]);
                            pass.set_bind_group(2, &self.cabin_shadows.sampling_bind, &[]);
                        } else {
                            pass.set_bind_group(
                                1,
                                self.combine_rt_bind
                                    .as_ref()
                                    .unwrap_or(&self.smoke_empty_visibility_bind),
                                &[],
                            );
                        }
                        pass.set_vertex_buffer(0, self.pixel_particle_vertices.slice(..));
                        pass.draw(0..6, 0..self.pixel_particle_count.get());
                    }
                }
                self.blit(device, &mut encoder, &output_view, format);
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &output,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded),
                            rows_per_image: Some(self.ih),
                        },
                    },
                    wgpu::Extent3d {
                        width: self.iw,
                        height: self.ih,
                        depth_or_array_layers: 1,
                    },
                );
                queue.submit([encoder.finish()]);
                let raw = read_buffer(device, &readback)?;
                let mut pixels = Vec::with_capacity((unpadded * self.ih) as usize);
                for row in raw.chunks_exact(padded as usize) {
                    pixels.extend_from_slice(&row[..unpadded as usize]);
                }
                frames[index]=pixels;
            }
            ensure!(
                self.frame == unchanged_frame && self.time.to_bits() == unchanged_time.to_bits(),
                "display review advanced renderer state"
            );
            Ok(frames)
        })();
        queue.write_buffer(&self.crush_buf, 0, &original);
        let mut restore = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("look-review-restore-native-frame"),
        });
        restore.copy_texture_to_texture(
            original_ldr.as_image_copy(),
            self.ldr_tex.as_image_copy(),
            native_extent,
        );
        queue.submit([restore.finish()]);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_variants_preserve_gpu_eye_and_physical_frame_lanes() {
        let mut base: CrushUniform = bytemuck::Zeroable::zeroed();
        base.exposure = 0.217;
        base.rod_exposure = 0.63;
        base.rod_inverse_saturation = 0.027;
        base.mask = 4.0;
        base.concussion = 0.4;
        base.frame = 812.0;
        base.display_grade = [1.0, 1.0, 0.0, 1.0];
        for look in looks(base) {
            let c = look.crush;
            assert_eq!(c.exposure.to_bits(), base.exposure.to_bits());
            assert_eq!(c.rod_exposure.to_bits(), base.rod_exposure.to_bits());
            assert_eq!(
                c.rod_inverse_saturation.to_bits(),
                base.rod_inverse_saturation.to_bits()
            );
            assert_eq!(c.mask, base.mask);
            assert_eq!(c.concussion, base.concussion);
            assert_eq!(c.frame, base.frame);
        }
        let variants = looks(base);
        assert_eq!(variants[0].crush.display_grade, base.display_grade);
        assert_eq!(variants[1].crush.retro, 0.0);
        assert_eq!(variants[1].crush.ldr_dither, 0.0);
    }
}
