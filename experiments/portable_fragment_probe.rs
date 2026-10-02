//! Native-driver compilation isolation; never used by production renderer.
use std::collections::BTreeMap;
use wgpu::naga;
#[path="../src/observer.rs"] mod observer;
fn visibility_shader(group:u32,geometry:u32,instances:u32)->String {
    include_str!("../shaders/scene_visibility.wgsl").replace("// @environment-optical-path",include_str!("../shaders/scene_environment_visibility.wgsl")).replace("SV_GROUP",&group.to_string())
        .replace("SV_GEOMETRY_BINDING",&geometry.to_string()).replace("SV_INSTANCE_BINDING",&instances.to_string())
}
fn compose(source:&str)->String {
    source.replace("// @observer-constants",&observer::wgsl_constants())
        .replace("// @raster-particle-lighting",include_str!("../shaders/raster_particle_lighting.wgsl"))
        .replace("// @camera-relative-projection",include_str!("../shaders/camera_relative.wgsl"))
        .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"))
        .replace("// @sky-visibility-sampling",include_str!("../shaders/sky_visibility_sampling.wgsl"))
        .replace("// @cabin-visibility-sampling",include_str!("../shaders/cabin_visibility.wgsl"))
        .replace("// @weather-density",include_str!("../shaders/weather_volume.wgsl"))
        .replace("// @cabin-medium",include_str!("../shaders/cabin_medium.wgsl"))
        .replace("// @scene-visibility-mesh", &visibility_shader(2,9,10))
        .replace("// @scene-visibility-smoke", &visibility_shader(3,3,4))
        .replace("// @scene-visibility-particles", &visibility_shader(2,3,4))
        .replace("// @scene-light-visibility",include_str!("../shaders/scene_light_visibility.wgsl"))
}
fn body(source:&mut String,name:&str,replacement:&str) {
    let start=source.find(&format!("fn {name}(" )).expect(name);let open=source[start..].find('{').unwrap()+start;
    let mut depth=1;let mut end=open+1;
    for (i,c) in source[open+1..].char_indices(){if c=='{' {depth+=1;}if c=='}'{depth-=1;if depth==0 {end=open+1+i;break;}}}
    source.replace_range(open+1..end,replacement);
}
fn main(){
    let variant=std::env::args().nth(1).unwrap_or_else(||"full".into());
    let mut source=compose(include_str!("../shaders/stylized_mesh.wgsl"));
    if variant=="no-medium" {body(&mut source,"source_background_medium_transmittance"," return 1.0; ");}
    if variant=="one-ray" {body(&mut source,"shade"," return unoccluded(vec4(scene_environment_transmission(in.world,vec3(0.0,1.0,0.0)),1.0)); ");}
    if variant=="no-optics" {body(&mut source,"scene_light_transmission"," let h=scene_visibility_trace(origin,direction,0.0001,distance_m); return vec3(select(0.0,1.0,h.status==SV_CLEAR)); ");
        body(&mut source,"scene_point_transmission"," let d=source-receiver;let h=scene_visibility_trace(receiver,normalize(d),0.0001,length(d));return vec3(select(0.0,1.0,h.status==SV_CLEAR)); ");}
    let module=naga::front::wgsl::parse_str(&source).unwrap();
    if variant=="spv" {
        let info=naga::valid::Validator::new(naga::valid::ValidationFlags::all(),naga::valid::Capabilities::all()).validate(&module).unwrap();
        let words=naga::back::spv::write_vec(&module,&info,&Default::default(),Some(&naga::back::spv::PipelineOptions{shader_stage:naga::ShaderStage::Fragment,entry_point:"fs".into()})).unwrap();
        let bytes:Vec<u8>=words.iter().flat_map(|w|w.to_le_bytes()).collect();
        std::fs::write("/tmp/production_visibility.spv",bytes).unwrap();return;
    }

    let instance=wgpu::Instance::new(&wgpu::InstanceDescriptor {backends:wgpu::Backends::VULKAN,..Default::default()});
    let adapter=pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions{power_preference:wgpu::PowerPreference::HighPerformance,..Default::default()})).unwrap();
    eprintln!("variant={variant}, adapter={:?}",adapter.get_info());
    let(device,_queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {label:Some("portable fragment compile isolation"),required_features:wgpu::Features::PUSH_CONSTANTS,
        required_limits:wgpu::Limits{max_push_constant_size:128,..Default::default()},..Default::default()})).unwrap();
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut groups:BTreeMap<u32,Vec<wgpu::BindGroupLayoutEntry>>=BTreeMap::new();
    for (_,v) in module.global_variables.iter(){let Some(b)=&v.binding else{continue;};
        let ty=match v.space {
            naga::AddressSpace::Uniform=>wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},
            naga::AddressSpace::Storage{access}=>wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:!access.contains(naga::StorageAccess::STORE)},has_dynamic_offset:false,min_binding_size:None},
            naga::AddressSpace::Handle=>match module.types[v.ty].inner {
                naga::TypeInner::Sampler{comparison}=>wgpu::BindingType::Sampler(if comparison{wgpu::SamplerBindingType::Comparison}else{wgpu::SamplerBindingType::Filtering}),
                naga::TypeInner::Image{dim,arrayed,class}=>wgpu::BindingType::Texture {sample_type:match class {
                    naga::ImageClass::Depth{..}=>wgpu::TextureSampleType::Depth,
                    naga::ImageClass::Sampled{kind:naga::ScalarKind::Uint,..}=>wgpu::TextureSampleType::Uint,
                    _=>wgpu::TextureSampleType::Float{filterable:!(b.group==2&&b.binding==11)},},
                    view_dimension:match(dim,arrayed){(naga::ImageDimension::D2,false)=>wgpu::TextureViewDimension::D2,(naga::ImageDimension::D2,true)=>wgpu::TextureViewDimension::D2Array,(naga::ImageDimension::Cube,true)=>wgpu::TextureViewDimension::CubeArray,_=>panic!("image")},multisampled:false},
                _=>panic!("handle")},_=>continue};
        groups.entry(b.group).or_default().push(wgpu::BindGroupLayoutEntry{binding:b.binding,visibility:wgpu::ShaderStages::VERTEX_FRAGMENT,ty,count:None});
    }
    let layouts:Vec<_>=(0..4).map(|g|device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:None,entries:groups.get(&g).unwrap()})).collect();
    let layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:None,bind_group_layouts:&layouts.iter().collect::<Vec<_>>(),push_constant_ranges:&[wgpu::PushConstantRange{stages:wgpu::ShaderStages::VERTEX_FRAGMENT,range:0..80}]});
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("production portable fragment isolated"),source:wgpu::ShaderSource::Wgsl(source.into())});
    let attributes=wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4];
    let now=std::time::Instant::now();eprintln!("creating production mesh pipeline");
    let _pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("production mesh isolated"),layout:Some(&layout),
        vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),buffers:&[wgpu::VertexBufferLayout{array_stride:48,step_mode:wgpu::VertexStepMode::Vertex,attributes:&attributes}],compilation_options:Default::default()},
        fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::Rgba16Float,blend:None,write_mask:wgpu::ColorWrites::ALL})],compilation_options:Default::default()}),
        primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None});
    let error=pollster::block_on(device.pop_error_scope());eprintln!("compile {:?}; validation={error:?}",now.elapsed());assert!(error.is_none());
    let _check=device.create_buffer(&wgpu::BufferDescriptor{label:Some("post compile device health"),size:4,usage:wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
}
