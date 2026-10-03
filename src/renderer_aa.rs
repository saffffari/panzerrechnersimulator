//! Native-grid geometry coverage. Lighting/materials use the existing source shader unchanged.
//! Four hardware coverage samples resolve in linear HDR before tone mapping and Bayer dithering.
//! Legacy medium/particle consumers receive minimum depth: conservative, with lost background
//! coverage limited to a mixed native pixel. This is not per-sample volumetric integration.
use super::*;

pub(super) const SAMPLES:u32=4;
pub(super) fn enabled()->bool {
    match std::env::var("KESSEL_GEOMETRY_AA").ok().as_deref() {
        Some("off"|"1"|"none")=>false,
        Some("msaa4"|"4")|None=>true,
        Some(other)=>{eprintln!("[renderer] unknown geometry AA {other:?}; using msaa4");true}
    }
}

pub(super) struct Targets {
    pub color:wgpu::TextureView,
    pub ambient:wgpu::TextureView,
    pub depth:wgpu::TextureView,
    depth_bind:wgpu::BindGroup,
}
impl Targets {
    fn new(device:&wgpu::Device,width:u32,height:u32,layout:&wgpu::BindGroupLayout)->Self {
        let view=|label,format|device.create_texture(&wgpu::TextureDescriptor {
            label:Some(label),size:wgpu::Extent3d{width,height,depth_or_array_layers:1},
            mip_level_count:1,sample_count:SAMPLES,dimension:wgpu::TextureDimension::D2,format,
            usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats:&[],
        }).create_view(&Default::default());
        let color=view("native-aa-color",HDR);
        let ambient=view("native-aa-ambient",HDR);
        let depth=view("native-aa-depth",DEPTH);
        let depth_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:Some("native-aa-depth-resolve"),layout,
            entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&depth)}],
        });
        Self{color,ambient,depth,depth_bind}
    }
}
pub(super) struct GeometryAa {
    pub main:Targets,
    // Each port resolves before the next one reuses this scratch. Array layer ownership stays
    // with RasterOptics; no port shares another port's resolved color/depth.
    pub optic:Targets,
    pub mesh:wgpu::RenderPipeline,
    pub mesh_inst:wgpu::RenderPipeline,
    pub ambient_mesh:wgpu::RenderPipeline,
    pub ambient_inst:wgpu::RenderPipeline,
    pub sky:wgpu::RenderPipeline,
    resolve_depth:wgpu::RenderPipeline,
}
impl GeometryAa {
    pub fn new(device:&wgpu::Device,width:u32,height:u32,
        mesh_layout:&wgpu::PipelineLayout,mesh_shader:&wgpu::ShaderModule,
        sky_layout:&wgpu::PipelineLayout,sky_shader:&wgpu::ShaderModule)->Self {
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:Some("native-aa-depth-layout"),entries:&[wgpu::BindGroupLayoutEntry {
                binding:0,visibility:wgpu::ShaderStages::FRAGMENT,
                ty:wgpu::BindingType::Texture{sample_type:wgpu::TextureSampleType::Depth,
                    view_dimension:wgpu::TextureViewDimension::D2,multisampled:true},count:None,
            }],
        });
        let main=Targets::new(device,width,height,&layout);
        let optic=Targets::new(device,width.clamp(256,640),height.clamp(128,360),&layout);
        let attrs=wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4];
        let inst_attrs=wgpu::vertex_attr_array![4=>Float32x4,5=>Float32x4,6=>Float32x4,7=>Float32x4];
        let buffers=[wgpu::VertexBufferLayout{array_stride:std::mem::size_of::<Vertex>() as u64,
            step_mode:wgpu::VertexStepMode::Vertex,attributes:&attrs},
            wgpu::VertexBufferLayout{array_stride:64,step_mode:wgpu::VertexStepMode::Instance,attributes:&inst_attrs}];
        let pipeline=|inst:bool,ambient:bool| {
            let mut targets=vec![Some(color_target(HDR))];
            if ambient {targets.push(Some(color_target(HDR)));}
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label:Some("native-aa-mesh"),layout:Some(mesh_layout),
                vertex:wgpu::VertexState{module:mesh_shader,entry_point:Some(if inst{"vs_inst"}else{"vs"}),
                    buffers:&buffers[..if inst{2}else{1}],compilation_options:Default::default()},
                primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},
                depth_stencil:Some(depth_state(true,wgpu::CompareFunction::Less)),
                multisample:wgpu::MultisampleState{count:SAMPLES,..Default::default()},
                fragment:Some(wgpu::FragmentState{module:mesh_shader,
                    entry_point:Some(if ambient{"fs_ambient"}else{"fs"}),targets:&targets,
                    compilation_options:Default::default()}),multiview:None,cache:None,
            })
        };
        let mesh=pipeline(false,false);let mesh_inst=pipeline(true,false);
        let ambient_mesh=pipeline(false,true);let ambient_inst=pipeline(true,true);
        let sky=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("native-aa-sky"),layout:Some(sky_layout),
            vertex:wgpu::VertexState{module:sky_shader,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            primitive:Default::default(),depth_stencil:Some(depth_state(false,wgpu::CompareFunction::Equal)),
            multisample:wgpu::MultisampleState{count:SAMPLES,..Default::default()},
            fragment:Some(wgpu::FragmentState{module:sky_shader,entry_point:Some("fs"),
                targets:&[Some(color_target(HDR))],compilation_options:Default::default()}),multiview:None,cache:None,
        });
        let resolve_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:Some("native-aa-depth-pipeline-layout"),bind_group_layouts:&[&layout],push_constant_ranges:&[],
        });
        let shader=shader(device,"native-aa-depth",include_str!("../shaders/raster_aa_depth.wgsl"));
        let resolve_depth=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("native-aa-nearest-depth"),layout:Some(&resolve_layout),
            vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            primitive:Default::default(),depth_stencil:Some(depth_state(true,wgpu::CompareFunction::Always)),
            multisample:Default::default(),fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),
                targets:&[],compilation_options:Default::default()}),multiview:None,cache:None,
        });
        println!("[renderer] native geometry AA:4xMSAA; linear HDR resolve; conservative nearest depth for media");
        Self{main,optic,mesh,mesh_inst,ambient_mesh,ambient_inst,sky,resolve_depth}
    }
    pub fn resolve(&self,encoder:&mut wgpu::CommandEncoder,optic:bool,depth:&wgpu::TextureView,
        viewport:Option<[u32;2]>,work:&mut RenderWorkStats) {
        let targets=if optic{&self.optic}else{&self.main};
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label:Some("native-aa-depth-resolve"),color_attachments:&[],
            depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:depth,
                depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
            timestamp_writes:None,occlusion_query_set:None,
        });
        if let Some([width,height])=viewport {
            // The opaque/sky passes populated only this top-left native-pixel rectangle.
            // Fragment position still addresses the identical MSAA texel; the full attachment
            // clear above leaves unused scratch pixels at sky depth without resolving them.
            pass.set_viewport(0.0,0.0,width as f32,height as f32,0.0,1.0);
            pass.set_scissor_rect(0,0,width,height);
        }
        pass.set_pipeline(&self.resolve_depth);pass.set_bind_group(0,&targets.depth_bind,&[]);
        pass.draw(0..3,0..1);work.fullscreen_raster_passes+=1;
    }
}
impl Renderer {
    pub(crate) fn geometry_aa_receipt(&self)->serde_json::Value {
        serde_json::json!({"geometry_samples":if self.geometry_aa.is_some(){SAMPLES}else{1},
            "native_resolution":[self.iw,self.ih],"resolve":"linear HDR before tone mapping and native dither",
            "views":"main opaque meshes plus each optical exterior opaque scene and sky",
            "depth":"nearest sample; conservative mixed-edge approximation for single-sample media/particles",
            "remaining_single_sample":"transparent glass, optical entrance panels, hover outline, media and particles",
            "temporal_accumulation":false,"display_upscale":"nearest unchanged"})
    }
}
fn depth_state(write:bool,compare:wgpu::CompareFunction)->wgpu::DepthStencilState {
    wgpu::DepthStencilState {format:DEPTH,depth_write_enabled:write,depth_compare:compare,
        stencil:Default::default(),bias:Default::default()}
}

#[cfg(test)]
mod tests {
    #[test]
    fn close_optic_receiver_plane_remains_valid_below_old_world_area_cutoff() {
        use glam::Vec3;
        // Analytic planar receiver, sampled by perspective rays at adjacent pixel centres.
        // Its plane is valid at a held eyepiece as well as a normal cabin view. This witnesses
        // the old absolute m^4 cutoff rejecting small projected pixels without GPU timing.
        let normal=Vec3::new(0.31,0.52,-0.8).normalize();
        let mut old_rejected=0;
        for (distance,fov,height) in [(0.025_f32,14_f32,450_f32),(0.15,14.0,450.0),
            (0.5,68.0,450.0),(0.025,14.0,1080.0)] {
            let point=Vec3::Z*distance;
            let step=2.0*(fov.to_radians()*0.5).tan()/height;
            let on_plane=|x:f32,y:f32| {
                let ray=Vec3::new(x,y,1.0);
                ray*(normal.dot(point)/normal.dot(ray))
            };
            let p=on_plane(0.0,0.0);
            let dx=on_plane(step,0.0)-p;
            let dy=on_plane(0.0,step)-p;
            if dx.cross(dy).length_squared()<=1e-16 {old_rejected+=1;}
            let recovered=dx.normalize().cross(dy.normalize()).normalize();
            assert!(recovered.dot(normal).abs()>0.99999,
                "real planar receiver lost its normal at {distance}m/{fov}deg/{height}p");
            // The plane predicts the depth of a neighboring shadow texel with the same
            // normal regardless of this player's native pixel footprint or optic zoom.
            let light=Vec3::new(0.17,0.21,-0.13);
            let ray=(p-light+Vec3::new(0.001,-0.001,0.0)).normalize();
            let expected=normal.dot(p-light)/normal.dot(ray);
            let predicted=recovered.dot(p-light)/recovered.dot(ray);
            assert!((predicted-expected).abs()<0.000001);
        }
        assert!(old_rejected>=2,"the regression must witness the old ADS failure");
    }

    #[test]
    fn native_aa_depth_is_conservative_for_partial_armor_and_clear_sky() {
        let nearest=|samples:[f32;4]|samples.into_iter().fold(1.0_f32,f32::min);
        assert_eq!(nearest([1.0;4]),1.0);
        assert_eq!(nearest([0.72;4]),0.72);
        assert_eq!(nearest([1.0,0.28,1.0,1.0]),0.28);
        assert_eq!(nearest([0.7,0.45,0.8,0.9]),0.45);
        let depth=super::depth_state(true,wgpu::CompareFunction::Always);
        assert!(depth.depth_write_enabled);
        assert_eq!(super::SAMPLES,4);
    }
}
