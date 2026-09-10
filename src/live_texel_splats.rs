//! Source-face diffuse texel rasterization using the live probe carriers. A separate bind layout
//! omits compute output resources, so render attachments can never alias bound storage textures.
use wgpu::{BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, TextureView};

pub fn source() -> String {
    let header=super::SHADER.split("@compute").next().expect("probe compute entry");
    let header=header.lines().filter(|line| !line.starts_with("@group(0) @binding(13)")
        && !line.starts_with("@group(0) @binding(14)")).collect::<Vec<_>>().join("\n");
    format!("{header}\n{}",include_str!("../shaders/live_texel_splats.wgsl"))
}

pub struct Gpu {layout:BindGroupLayout,pipeline:wgpu::RenderPipeline}
impl Gpu {
    pub fn new(device:&wgpu::Device, entries:&[wgpu::BindGroupLayoutEntry])->Self {
        let entries:Vec<_>=entries[..13].iter().cloned().map(|mut entry| {
            entry.visibility=wgpu::ShaderStages::VERTEX_FRAGMENT;entry}).collect();
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:Some("source-face-texel-layout"),entries:&entries});
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:Some("source-face-texel-layout"),bind_group_layouts:&[&layout],push_constant_ranges:&[]});
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:Some("source-face-texel-presentation"),source:wgpu::ShaderSource::Wgsl(source().into())});
        let target=Some(wgpu::ColorTargetState {format:wgpu::TextureFormat::Rgba16Float,
            blend:None,write_mask:wgpu::ColorWrites::ALL});
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("source-face-texel-presentation"),layout:Some(&pipeline_layout),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
            // The eye depth/ID textures are tested explicitly, and never replaced by probe depth.
            depth_stencil:None,multisample:Default::default(),
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs"),targets:&[target.clone(),target],compilation_options:Default::default()}),
            multiview:None,cache:None});
        Self {layout,pipeline}
    }
    pub fn bindings(&self,device:&wgpu::Device,uniform:&wgpu::Buffer,views:&[&TextureView])->BindGroup {
        assert_eq!(views.len(),12);
        let mut entries=vec![BindGroupEntry {binding:0,resource:uniform.as_entire_binding()}];
        entries.extend(views.iter().enumerate().map(|(i,view)|BindGroupEntry {
            binding:i as u32+1,resource:BindingResource::TextureView(view)}));
        device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("source-face-texel-bind"),
            layout:&self.layout,entries:&entries})
    }
    pub fn encode(&self,encoder:&mut wgpu::CommandEncoder,bind:&BindGroup,
                  output:&TextureView,mask:&TextureView,profile:Option<&crate::gpu_profile::GpuProfile>) {
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("source-face-texel-presentation"),color_attachments:&[Some(super::super::attachment(output,wgpu::LoadOp::Load)),
                Some(super::super::attachment(mask,wgpu::LoadOp::Load))],
            depth_stencil_attachment:None,occlusion_query_set:None,
            timestamp_writes:profile.and_then(|p|p.render("source-face-texel-presentation"))});
        pass.set_pipeline(&self.pipeline);pass.set_bind_group(0,bind,&[]);
        pass.draw(0..6,0..6*super::SIDE*super::SIDE);
    }
}

#[cfg(test)] mod tests {
    #[test]
    fn source_face_splat_shader_validates_without_attachment_aliases() {
        let source=super::source();
        assert!(!source.contains("texture_storage_2d"));
        assert!(source.contains("source_position(face,vec2<f32>(pixel)+corners[c],d)"));
        assert!(source.contains("abs(dot(input.center-position,n))>0.004"));
        let module=wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
    }
}
