//! Visible-surface selection mask and a crisp one-pixel silhouette. Uses actual hovered mesh IDs.
use super::{Renderer, HDR, DEPTH, Vertex, PushData, RenderWorkStats};
pub(super) struct HoverOutline { mask:wgpu::TextureView, mesh:wgpu::RenderPipeline,
    edge:wgpu::RenderPipeline, bind:wgpu::BindGroup }
impl HoverOutline {
    pub fn new(device:&wgpu::Device,width:u32,height:u32,layout:&wgpu::PipelineLayout,shader:&wgpu::ShaderModule)->Self {
        let mask=device.create_texture(&wgpu::TextureDescriptor{label:Some("hover-mask"),
            size:wgpu::Extent3d{width,height,depth_or_array_layers:1},mip_level_count:1,sample_count:1,
            dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::R8Unorm,
            usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::TEXTURE_BINDING,view_formats:&[]})
            .create_view(&Default::default());
        let mesh=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("hover-mesh-mask"),layout:Some(layout),
            vertex:wgpu::VertexState{module:shader,entry_point:Some("vs"),compilation_options:Default::default(),
                buffers:&[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>() as u64,
                    step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4]}]},
            primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH,depth_write_enabled:false,
                depth_compare:wgpu::CompareFunction::LessEqual,stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),fragment:Some(wgpu::FragmentState{module:shader,entry_point:Some("fs_hover"),
                compilation_options:Default::default(),targets:&[Some(super::color_target(wgpu::TextureFormat::R8Unorm))]}),
            multiview:None,cache:None});
        let bgl=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("hover-edge-bgl"),
            entries:&[super::tex_entry(0)]});
        let bind=device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("hover-edge-bind"),layout:&bgl,
            entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&mask)}]});
        let pl=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("hover-edge-pl"),
            bind_group_layouts:&[&bgl],push_constant_ranges:&[]});
        let s=super::shader(device,"hover-outline",include_str!("../shaders/hover_outline.wgsl"));
        let edge=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("hover-edge"),layout:Some(&pl),
            vertex:wgpu::VertexState{module:&s,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            primitive:Default::default(),depth_stencil:None,multisample:Default::default(),
            fragment:Some(wgpu::FragmentState{module:&s,entry_point:Some("fs"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState{format:HDR,blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
            multiview:None,cache:None});
        Self{mask,mesh,edge,bind}
    }
}
impl Renderer {
    pub fn set_hovered_meshes(&mut self, meshes:&[usize]) {
        self.hovered_meshes.clear();
        self.hovered_meshes.extend(meshes.iter().copied().filter(|&id|id<self.meshes.len()));
        self.hovered_meshes.sort_unstable();self.hovered_meshes.dedup();
    }
    pub(super) fn encode_hover_outline(&self,encoder:&mut wgpu::CommandEncoder,work:&mut RenderWorkStats) {
        let Some(outline)=&self.hover_outline else{return;};
        if self.hovered_meshes.is_empty() || self.reticle || self.optic_mask>0.5 {return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor{label:Some("hover-visible-mask"),
            color_attachments:&[Some(super::attachment(&outline.mask,wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)))],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment{view:&self.depth_view,
                depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Load,store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:None,occlusion_query_set:None});
        pass.set_pipeline(&outline.mesh);pass.set_bind_group(0,&self.camera_bind,&[]);
        pass.set_bind_group(2,&self.shadow_bind,&[]);pass.set_bind_group(3,&self.track_bind,&[]);
        for &id in &self.hovered_meshes {
            let m=&self.meshes[id];if !m.visible {continue;}
            let push=PushData{model:m.model.to_cols_array_2d(),params:[0.0,m.metallic_factor,m.roughness_factor,0.0]};
            pass.set_push_constants(wgpu::ShaderStages::VERTEX_FRAGMENT,0,bytemuck::bytes_of(&push));
            pass.set_bind_group(1,m.tex_bind.as_ref(),&[]);pass.set_vertex_buffer(0,m.vbuf.slice(..));
            pass.set_index_buffer(m.ibuf.slice(..),wgpu::IndexFormat::Uint32);pass.draw_indexed(0..m.index_count,0,0..1);
            work.scene_mesh_draws+=1;
        }
        drop(pass);
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor{label:Some("hover-silhouette"),
            color_attachments:&[Some(super::attachment(&self.hdr_view,wgpu::LoadOp::Load))],
            depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None});
        pass.set_pipeline(&outline.edge);pass.set_bind_group(0,&outline.bind,&[]);pass.draw(0..3,0..1);
        work.fullscreen_raster_passes+=1;
    }
}
