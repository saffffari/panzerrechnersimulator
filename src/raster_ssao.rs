//! Ambient-only raster SSAO: opaque geometry writes its ambient term alongside HDR in one draw.
//! A bounded deterministic depth kernel removes occluded ambient before glass/optic composition.
use super::{Camera, HDR, DEPTH, Vertex, RenderWorkStats};

pub(super) struct RasterSsao {
    pub ambient: wgpu::TextureView,
    pub mesh: wgpu::RenderPipeline,
    pub mesh_inst: wgpu::RenderPipeline,
    pipe: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    height: u32,
    enabled: bool,
}
impl RasterSsao {
    pub fn new(device:&wgpu::Device,width:u32,height:u32,depth:&wgpu::TextureView,
        mesh_layout:&wgpu::PipelineLayout,mesh_shader:&wgpu::ShaderModule)->Self {
        let ambient=device.create_texture(&wgpu::TextureDescriptor {
            label:Some("raster-ambient"),size:wgpu::Extent3d{width,height,depth_or_array_layers:1},
            mip_level_count:1,sample_count:1,dimension:wgpu::TextureDimension::D2,format:HDR,
            usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats:&[],
        }).create_view(&Default::default());
        let attrs=wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4];
        let instance_attrs=wgpu::vertex_attr_array![4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
        let buffers=[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>() as u64,
            step_mode:wgpu::VertexStepMode::Vertex,attributes:&attrs},
            wgpu::VertexBufferLayout{array_stride:64,step_mode:wgpu::VertexStepMode::Instance,attributes:&instance_attrs}];
        let mesh_pipe=|inst:bool| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some(if inst {"raster-ambient-inst"}else{"raster-ambient-mesh"}),layout:Some(mesh_layout),
            vertex:wgpu::VertexState{module:mesh_shader,entry_point:Some(if inst{"vs_inst"}else{"vs"}),
                compilation_options:Default::default(),buffers:&buffers[..if inst{2}else{1}]},
            primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},
            depth_stencil:Some(wgpu::DepthStencilState{format:DEPTH,depth_write_enabled:true,
                depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),
            multisample:Default::default(),fragment:Some(wgpu::FragmentState{module:mesh_shader,
                entry_point:Some("fs_ambient"),compilation_options:Default::default(),
                targets:&[Some(super::color_target(HDR)),Some(super::color_target(HDR))]}),
            multiview:None,cache:None,
        });
        let mesh=mesh_pipe(false);let mesh_inst=mesh_pipe(true);
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:Some("raster-ssao-layout"),entries:&[
                wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::FRAGMENT,
                    ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Depth,
                        view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
                wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::FRAGMENT,
                    ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Float{filterable:false},
                        view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
            ],
        });
        let bind=device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("raster-ssao-bind"),layout:&layout,
            entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(depth)},
                wgpu::BindGroupEntry{binding:1,resource:wgpu::BindingResource::TextureView(&ambient)}]});
        let pl=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("raster-ssao-pl"),
            bind_group_layouts:&[&layout],push_constant_ranges:&[wgpu::PushConstantRange{
                stages:wgpu::ShaderStages::FRAGMENT,range:0..80}]});
        let shader=super::shader(device,"raster-ssao",include_str!("../shaders/raster_ssao.wgsl"));
        let pipe=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{
            label:Some("raster-ssao"),layout:Some(&pl),
            vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            primitive:Default::default(),depth_stencil:None,multisample:Default::default(),
            fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState{format:HDR,write_mask:wgpu::ColorWrites::RED | wgpu::ColorWrites::GREEN | wgpu::ColorWrites::BLUE,
                    blend:Some(wgpu::BlendState{color:wgpu::BlendComponent{src_factor:wgpu::BlendFactor::One,
                        dst_factor:wgpu::BlendFactor::One,operation:wgpu::BlendOperation::ReverseSubtract},
                        alpha:wgpu::BlendComponent::REPLACE})})]}),multiview:None,cache:None,
        });
        let enabled=std::env::var("PANZER_SSAO").ok().as_deref()!=Some("0");
        println!("[renderer] ambient-only SSAO: {} (64 deterministic depth samples)",if enabled{"on"}else{"off"});
        Self{ambient,mesh,mesh_inst,pipe,bind,height,enabled}
    }
    pub fn encode(&self,encoder:&mut wgpu::CommandEncoder,color:&wgpu::TextureView,camera:&Camera,interior:bool,work:&mut RenderWorkStats) {
        if !self.enabled {return;}
        let mut parameters=[0.0_f32;20];
        parameters[..16].copy_from_slice(&camera.relative_view_proj().inverse().to_cols_array());
        parameters[16..].copy_from_slice(&[if interior {0.60} else {2.20},0.90,self.height as f32/(2.0*(camera.fov_y*0.5).tan()),0.0]);
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor{label:Some("raster-ssao-ambient-only"),
            color_attachments:&[Some(super::attachment(color,wgpu::LoadOp::Load))],
            depth_stencil_attachment:None,timestamp_writes:None,occlusion_query_set:None});
        pass.set_pipeline(&self.pipe);pass.set_bind_group(0,&self.bind,&[]);
        pass.set_push_constants(wgpu::ShaderStages::FRAGMENT,0,bytemuck::cast_slice(&parameters));
        pass.draw(0..3,0..1);work.fullscreen_raster_passes+=1;
    }
}
