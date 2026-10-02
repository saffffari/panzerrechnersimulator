//! Executable coverage/depth contract for the actual production resolve shader.
use wgpu::naga;

#[test]
fn native_geometry_aa_resolve_shader_validates() {
    let src=include_str!("../shaders/raster_aa_depth.wgsl");
    let module=naga::front::wgsl::parse_str(src).unwrap();
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all())
        .validate(&module).unwrap();
    let input=module.global_variables.iter().find(|(_,v)|v.name.as_deref()==Some("source_depth")).unwrap().1;
    assert!(matches!(module.types[input.ty].inner,naga::TypeInner::Image {
        dim:naga::ImageDimension::D2,arrayed:false,class:naga::ImageClass::Depth{multi:true}}));
}

#[test]
#[ignore = "requires a graphics device; run explicitly during the coordinated GPU test slot"]
fn native_geometry_aa_gpu_resolves_real_coverage_and_conservative_depth() {
    pollster::block_on(async {
        let instance=wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter=instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();
        let (device,queue)=adapter.request_device(&wgpu::DeviceDescriptor::default()).await.unwrap();
        let texture=|label,format,samples,usage|device.create_texture(&wgpu::TextureDescriptor {
            label:Some(label),size:wgpu::Extent3d{width:4,height:1,depth_or_array_layers:1},
            mip_level_count:1,sample_count:samples,dimension:wgpu::TextureDimension::D2,format,
            usage:wgpu::TextureUsages::RENDER_ATTACHMENT|usage,view_formats:&[],
        });
        let color=texture("aa-test-color",wgpu::TextureFormat::Rgba16Float,4,wgpu::TextureUsages::empty());
        let resolved=texture("aa-test-resolved",wgpu::TextureFormat::Rgba16Float,1,wgpu::TextureUsages::COPY_SRC);
        let depth=texture("aa-test-depth",wgpu::TextureFormat::Depth32Float,4,wgpu::TextureUsages::TEXTURE_BINDING);
        let resolved_depth=texture("aa-test-resolved-depth",wgpu::TextureFormat::Depth32Float,1,wgpu::TextureUsages::COPY_SRC);
        let views=[&color,&resolved,&depth,&resolved_depth].map(|t|t.create_view(&Default::default()));
        // Three source cases, in separate native pixels: sky miss, one covered sample, fully
        // covered source geometry. This exercises the hardware sample mask, not a CPU copy of min.
        let source=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("coverage-fixture"),source:wgpu::ShaderSource::Wgsl(r#"
            @vertex fn vs(@builtin(vertex_index)i:u32)->@builtin(position)vec4<f32>{
                let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
                return vec4(p[i],0.0,1.0);
            }
            struct Out{@location(0)color:vec4<f32>,@builtin(frag_depth)depth:f32}
            @fragment fn fs(@builtin(position)p:vec4<f32>,@builtin(sample_index)s:u32)->Out{
                if(p.x<1.0){discard;}
                if(p.x<2.0 && s!=2u){discard;}
                return Out(vec4(1.0,0.0,0.0,1.0),select(0.75,0.25,p.x<2.0));
            }"#.into())});
        let depth_state=|compare|wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,
            depth_write_enabled:true,depth_compare:compare,stencil:Default::default(),bias:Default::default()};
        let render=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("coverage-fixture"),layout:None,
            vertex:wgpu::VertexState{module:&source,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            fragment:Some(wgpu::FragmentState{module:&source,entry_point:Some("fs"),compilation_options:Default::default(),
                targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::Rgba16Float,blend:None,write_mask:wgpu::ColorWrites::ALL})]}),
            primitive:Default::default(),depth_stencil:Some(depth_state(wgpu::CompareFunction::Less)),
            multisample:wgpu::MultisampleState{count:4,..Default::default()},multiview:None,cache:None,
        });
        let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("production-aa-depth"),
            source:wgpu::ShaderSource::Wgsl(include_str!("../shaders/raster_aa_depth.wgsl").into())});
        let resolve=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("production-aa-depth"),layout:None,
            vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
            fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[]}),
            primitive:Default::default(),depth_stencil:Some(depth_state(wgpu::CompareFunction::Always)),
            multisample:Default::default(),multiview:None,cache:None,
        });
        let bind=device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("aa-depth-input"),
            layout:&resolve.get_bind_group_layout(0),entries:&[wgpu::BindGroupEntry{binding:0,resource:wgpu::BindingResource::TextureView(&views[2])}]});
        let readback=device.create_buffer(&wgpu::BufferDescriptor{label:Some("aa-readback"),size:512,
            usage:wgpu::BufferUsages::MAP_READ|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        let mut encoder=device.create_command_encoder(&Default::default());
        {
            let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:Some("real-multisample-fixture"),color_attachments:&[Some(wgpu::RenderPassColorAttachment {
                    view:&views[0],resolve_target:Some(&views[1]),depth_slice:None,
                    ops:wgpu::Operations{load:wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),store:wgpu::StoreOp::Store}})],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment{view:&views[2],
                    depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
                timestamp_writes:None,occlusion_query_set:None,
            });pass.set_pipeline(&render);pass.draw(0..3,0..1);
        }
        {
            let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label:Some("actual-depth-resolve"),color_attachments:&[],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment{view:&views[3],
                    depth_ops:Some(wgpu::Operations{load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
                timestamp_writes:None,occlusion_query_set:None,
            });pass.set_pipeline(&resolve);pass.set_bind_group(0,&bind,&[]);pass.draw(0..3,0..1);
        }
        for (offset,texture) in [(0,&resolved),(256,&resolved_depth)] {
            encoder.copy_texture_to_buffer(texture.as_image_copy(),wgpu::TexelCopyBufferInfo{buffer:&readback,
                layout:wgpu::TexelCopyBufferLayout{offset,bytes_per_row:Some(256),rows_per_image:Some(1)}},
                wgpu::Extent3d{width:4,height:1,depth_or_array_layers:1});
        }
        queue.submit([encoder.finish()]);
        let (tx,rx)=std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read,move|result|tx.send(result).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();rx.recv().unwrap().unwrap();
        let bytes=readback.slice(..).get_mapped_range();
        let reds:Vec<_>=(0..4).map(|i|u16::from_le_bytes(bytes[i*8..i*8+2].try_into().unwrap())).collect();
        assert_eq!(reds,[0x0000,0x3400,0x3c00,0x3c00],"HDR coverage must be0,1/4,1,1");
        let depths:Vec<_>=(0..4).map(|i|f32::from_le_bytes(bytes[256+i*4..260+i*4].try_into().unwrap())).collect();
        assert_eq!(depths,[1.0,0.25,0.75,0.75],"mixed armor pixel must retain nearest depth");
        println!("AA GPU receipt: {:?}, coverage={reds:?}, depth={depths:?}",adapter.get_info());
    });
}
