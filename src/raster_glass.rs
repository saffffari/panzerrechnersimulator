//! Cheap, deterministic glazing for the explicit shader renderer. The opaque scene/sky are
//! already present; panes add only a weak premultiplied sheen and never replace scene depth.
use super::{DEPTH, HDR, PushData, RenderWorkStats, Renderer};
use glam::Mat4;

fn glass_depth() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::LessEqual,
        stencil: Default::default(),
        bias: Default::default(),
    }
}

fn glass_target() -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format: HDR,
        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    }
}

pub(super) fn pipeline(
    device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, glow: bool,
) -> wgpu::RenderPipeline {
    let mut targets=vec![Some(glass_target())];
    if glow { targets.push(Some(wgpu::ColorTargetState {format:HDR,blend:None,write_mask:wgpu::ColorWrites::ALPHA})); }
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("raster-glass"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<super::Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3,
                    2 => Float32x2, 3 => Float32x4],
            }],
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(glass_depth()),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if glow {"fs_ambient"} else {"fs"}),
            targets: &targets,
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    })
}

fn ordinary_glass(visible: bool, glass: bool, optical_kind: u32, optic_panel: bool,
    optical_capture: bool, view_local: bool) -> bool {
    visible && glass && optical_kind == 0 && !optic_panel && !(optical_capture && view_local)
}

impl Renderer {
    /// Shared by the player's scene and objective captures. Sorting is in that scene's own
    /// projected depth; it never borrows the main eye for a differently oriented objective.
    pub(super) fn encode_raster_glass(&self, encoder: &mut wgpu::CommandEncoder,
        camera_bind: &wgpu::BindGroup, view_proj: Mat4, color: &wgpu::TextureView,
        depth: &wgpu::TextureView, optical_capture: bool, work: &mut RenderWorkStats) {
        let Some(pipeline) = (if optical_capture {&self.raster_glass_pipe}else{&self.raster_glow_glass_pipe}) else { return; };
        let planes = super::frustum_planes(view_proj);
        let mut panes: Vec<_> = self.meshes.iter().enumerate().filter_map(|(index, mesh)| {
            if !ordinary_glass(mesh.visible, mesh.glass, mesh.periscope_kind,
                self.raster_optic_panel(index), optical_capture, mesh.raster_only)
                || !super::aabb_in_frustum(&planes, mesh.world_min, mesh.world_max) {
                return None;
            }
            let center = (mesh.world_min + mesh.world_max) * 0.5;
            let clip = view_proj * center.extend(1.0);
            let sort_depth = clip.z / clip.w.max(0.0001);
            Some((sort_depth, index))
        }).collect();
        if panes.is_empty() { return; }
        panes.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut attachments=vec![Some(super::attachment(color,wgpu::LoadOp::Load))];
        if !optical_capture {if let Some(ao)=&self.raster_ssao {attachments.push(Some(super::attachment(&ao.ambient,wgpu::LoadOp::Load)));}}
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("raster-glass-sheen"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, camera_bind, &[]);
        pass.set_bind_group(2, &self.shadow_bind, &[]);
        pass.set_bind_group(3, &self.track_bind, &[]);
        for (_, index) in panes {
            let mesh = &self.meshes[index];
            let push = PushData { model: mesh.model.to_cols_array_2d(),
                params: [0.0, mesh.metallic_factor, mesh.roughness_factor, 8.0] };
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT, 0,
                bytemuck::bytes_of(&push));
            pass.set_bind_group(1, mesh.tex_bind.as_ref(), &[]);
            pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
            pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            work.scene_mesh_draws += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_preserves_opaque_depth_and_uses_premultiplied_compositing() {
        let depth = glass_depth();
        assert!(!depth.depth_write_enabled);
        assert_eq!(depth.depth_compare, wgpu::CompareFunction::LessEqual);
        let blend = glass_target().blend.unwrap();
        assert_eq!(blend.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::OneMinusSrcAlpha);
        assert_eq!(blend.alpha, wgpu::BlendComponent::OVER);
    }

    #[test]
    fn glass_pass_cannot_replace_optic_image_or_draw_view_local_capture_geometry() {
        assert!(ordinary_glass(true, true, 0, false, false, false));
        assert!(ordinary_glass(true, true, 0, false, true, false));
        assert!(!ordinary_glass(true, true, 0, true, false, false));
        for kind in 1..=3 {
            assert!(!ordinary_glass(true, true, kind, false, false, false));
        }
        assert!(!ordinary_glass(false, true, 0, false, false, false));
        assert!(!ordinary_glass(true, false, 0, false, false, false));
        assert!(!ordinary_glass(true, true, 0, false, true, true));
        assert!(ordinary_glass(true, true, 0, false, false, true));
    }
}
