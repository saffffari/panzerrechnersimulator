//! Large-world precision witnesses. The GPU gate rasterizes real Depth32Float through the same
//! helper used by both live primary geometry shaders; the reference is an independent f64 plane.
#[path = "../src/camera.rs"]
mod camera;
use camera::{Camera, relative_history_delta};
use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

#[test]
fn relative_history_preserves_rigid_motion_and_camera_translation() {
    for origin in [Vec3::ZERO, Vec3::new(0.0,0.0,180.0), Vec3::new(985.0,5.49,1205.0),
                   Vec3::new(5000.0,4.0,5000.0)] {
        let current = Mat4::from_rotation_translation(Quat::from_rotation_y(1.3), origin);
        let previous = Mat4::from_rotation_translation(Quat::from_rotation_y(1.29),
            origin + Vec3::new(-0.03,0.01,0.02));
        let eye = origin + Vec3::new(0.85,2.1686,-0.169);
        let previous_eye = eye + Vec3::new(-0.016,0.0,0.006);
        assert_eq!(relative_history_delta(current,current,eye,eye), Mat4::IDENTITY);
        let shift = relative_history_delta(current,current,eye,previous_eye);
        assert_eq!(shift.transform_point3(Vec3::ZERO), eye-previous_eye);
        for point in [Vec3::new(0.7,1.1,-2.0), Vec3::new(-1.7,2.2,1.0)] {
            // World-to-eye points are independently formed using the authored local point.
            let c = current.as_dmat4().transform_point3(point.as_dvec3()) - eye.as_dvec3();
            let p = previous.as_dmat4().transform_point3(point.as_dvec3()) - previous_eye.as_dvec3();
            let result = relative_history_delta(current,previous,eye,previous_eye)
                .transform_point3(c.as_vec3()).as_dvec3();
            assert!((result-p).length() < 0.000002, "origin={origin:?}, error={:?}",(result-p));
        }
    }
}

#[test]
fn camera_relative_projection_contract_covers_every_live_depth_consumer() {
    let helper=include_str!("../shaders/camera_relative.wgsl");
    for entry in [include_str!("../shaders/gbuf.wgsl"),include_str!("../shaders/mesh.wgsl")] {
        assert!(entry.contains("camera_relative_position(model, pos, cam.cam_pos)"));
        let source=entry.replace("// @camera-relative-projection",helper)
            .replace("// @campaign-terrain-material",include_str!("../shaders/terrain_material.wgsl"));
        let module=wgpu::naga::front::wgsl::parse_str(&source).unwrap();
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
    }
    assert!(include_str!("../shaders/combine.wgsl").contains("return cam.cam_pos + h.xyz / h.w"));
    assert!(include_str!("../shaders/smoke.wgsl").contains("scene_dist = length(sh.xyz / sh.w)"));
    for entry in [include_str!("../shaders/accum.wgsl"),include_str!("../shaders/volume_accum.wgsl")] {
        assert!(entry.contains("camera_shift: vec4<f32>"));
        assert!(entry.contains("+= u.camera_shift.xyz"));
    }
    let pressure=include_str!("../shaders/pressure_front.wgsl");
    assert!(pressure.contains("normalize(endpoint)"));
    assert!(pressure.contains("vec4(ray*travel+radial*0.03,1.0)"));
    assert!(include_str!("../shaders/live_texel_probe.wgsl").contains("abs(dot(separation, n)) > 0.004"));
}

#[test]
fn relative_primary_projection_keeps_local_depth_at_battlefield_origins() {
    for origin in [Vec3::ZERO, Vec3::new(0.0,0.0,180.0), Vec3::new(985.0,5.49,1205.0),
                   Vec3::new(5000.0,4.0,5000.0)] {
        for far in [24.0,6000.0] {
            let (eye,model) = fixture(origin,far);
            let vp=eye.relative_view_proj();
            let inverse=vp.inverse();
            let normal=Mat3::from_mat4(model)*Vec3::Z;
            for distance in [0.25,0.5,1.0,2.0,4.0,7.5] {
                let local=Vec3::new(0.85,2.1686,-0.169-distance);
                let relative=Mat3::from_mat4(model)*local+(model.w_axis.truncate()-eye.pos);
                let clip=vp*relative.extend(1.0);
                let h=inverse*(clip/clip.w);
                let expected=model.as_dmat4().transform_point3(local.as_dvec3())-eye.pos.as_dvec3();
                let error=(h.truncate().as_dvec3()/h.w as f64-expected).dot(normal.as_dvec3()).abs();
                assert!(error<0.002,"origin={origin:?} far={far} d={distance} error={error}");
            }
        }
    }
}

fn fixture(origin:Vec3,far:f32)->(Camera,Mat4) {
    // The first shifted fixture reproduces the reported replay eye [0.85,2.1686,179.831].
    let yaw=if origin.x==0.0 {0.0}else{1.3};
    let rotation=Quat::from_rotation_y(yaw);
    let model=Mat4::from_rotation_translation(rotation,origin);
    let mut eye=Camera::new(1.0);
    eye.pos=model.transform_point3(Vec3::new(0.85,2.1686,-0.169));
    eye.set_forward(rotation*-Vec3::Z);
    eye.fov_y=std::f32::consts::FRAC_PI_2;
    eye.zfar=far;
    (eye,model)
}

#[test]
#[ignore = "bounded real GPU depth rasterization; coordinator alone schedules GPU work"]
fn actual_gpu_primary_depth_preserves_nearby_planes_at_world_origins() {
    use wgpu::util::DeviceExt;
    let instance=wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter=pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference:wgpu::PowerPreference::LowPower,..Default::default()})).unwrap();
    let(device,queue)=pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    #[repr(C)] #[derive(Clone,Copy,bytemuck::Pod,bytemuck::Zeroable)]
    struct Uniform {model:[[f32;4];4],vp:[[f32;4];4],camera:[f32;4],plane:[f32;4]}
    let source=format!("{}\n{}",include_str!("../shaders/camera_relative.wgsl"),r#"
        struct Uniform {model:mat4x4<f32>,vp:mat4x4<f32>,camera:vec4<f32>,plane:vec4<f32>,};
        @group(0) @binding(0) var<uniform> u:Uniform;
        @vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
            var corners=array<vec2<f32>,6>(vec2(-1.0,-1.0),vec2(1.0,-1.0),vec2(-1.0,1.0),
                vec2(-1.0,1.0),vec2(1.0,-1.0),vec2(1.0,1.0));
            let local=u.plane.xyz+vec3(corners[i]*u.plane.w,0.0);
            return u.vp*vec4(camera_relative_position(u.model,local,u.camera.xyz),1.0);
        }"#);
    let module=device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label:Some("actual-primary-relative-depth"),source:wgpu::ShaderSource::Wgsl(source.into())});
    let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label:Some("actual-primary-relative-depth"),layout:None,
        vertex:wgpu::VertexState {module:&module,entry_point:Some("vs"),buffers:&[],compilation_options:Default::default()},
        primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},
        depth_stencil:Some(wgpu::DepthStencilState {format:wgpu::TextureFormat::Depth32Float,
            depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,
            stencil:Default::default(),bias:Default::default()}),
        multisample:Default::default(),fragment:None,multiview:None,cache:None});
    const W:u32=64;
    let depth=device.create_texture(&wgpu::TextureDescriptor {label:Some("actual-depth"),
        size:wgpu::Extent3d {width:W,height:W,depth_or_array_layers:1},mip_level_count:1,sample_count:1,
        dimension:wgpu::TextureDimension::D2,format:wgpu::TextureFormat::Depth32Float,
        usage:wgpu::TextureUsages::RENDER_ATTACHMENT|wgpu::TextureUsages::COPY_SRC,view_formats:&[]});
    let view=depth.create_view(&Default::default());
    let readback=device.create_buffer(&wgpu::BufferDescriptor {label:Some("actual-depth-readback"),
        size:u64::from(W*W*4),usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    for origin in [Vec3::ZERO,Vec3::new(0.0,0.0,180.0),Vec3::new(985.0,5.49,1205.0),Vec3::new(5000.0,4.0,5000.0)] {
      for far in [24.0,6000.0] {
        let(eye,model)=fixture(origin,far);let vp=eye.relative_view_proj();let inverse=vp.inverse();
        let normal=(Mat3::from_mat4(model)*Vec3::Z).as_dvec3();
        let mut max_error=0.0_f64;
        for distance in [0.25_f32,0.5,1.0,2.0,4.0,7.5] {
            let local=Vec3::new(0.85,2.1686,-0.169-distance);
            let reference=model.as_dmat4().transform_point3(local.as_dvec3())-eye.pos.as_dvec3();
            let uniform=Uniform {model:model.to_cols_array_2d(),vp:vp.to_cols_array_2d(),
                camera:eye.pos.extend(0.0).to_array(),plane:local.extend(distance*2.0).to_array()};
            let buffer=device.create_buffer_init(&wgpu::util::BufferInitDescriptor {label:None,
                contents:bytemuck::bytes_of(&uniform),usage:wgpu::BufferUsages::UNIFORM});
            let bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:None,
                layout:&pipeline.get_bind_group_layout(0),entries:&[wgpu::BindGroupEntry {binding:0,resource:buffer.as_entire_binding()}]});
            let mut encoder=device.create_command_encoder(&Default::default());
            {let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {label:None,color_attachments:&[],
                depth_stencil_attachment:Some(wgpu::RenderPassDepthStencilAttachment {view:&view,
                    depth_ops:Some(wgpu::Operations {load:wgpu::LoadOp::Clear(1.0),store:wgpu::StoreOp::Store}),stencil_ops:None}),
                timestamp_writes:None,occlusion_query_set:None});
             pass.set_pipeline(&pipeline);pass.set_bind_group(0,&bind,&[]);pass.draw(0..6,0..1);}
            encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo {texture:&depth,mip_level:0,
                origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::DepthOnly},
                wgpu::TexelCopyBufferInfo {buffer:&readback,layout:wgpu::TexelCopyBufferLayout {
                    offset:0,bytes_per_row:Some(W*4),rows_per_image:Some(W)}},
                wgpu::Extent3d {width:W,height:W,depth_or_array_layers:1});
            queue.submit([encoder.finish()]);let slice=readback.slice(..);let(tx,rx)=std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read,move|result|{tx.send(result).unwrap();});
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();rx.recv().unwrap().unwrap();
            let mapped=slice.get_mapped_range();
            for y in 8..56 {for x in 8..56 {
                let index=((y*W+x)*4) as usize;let d=f32::from_le_bytes(mapped[index..index+4].try_into().unwrap());
                assert!(d<1.0,"plane failed to rasterize origin={origin:?}");
                let h=inverse*Vec4::new((x as f32+0.5)/W as f32*2.0-1.0,
                    1.0-(y as f32+0.5)/W as f32*2.0,d,1.0);
                let relative=h.truncate().as_dvec3()/h.w as f64;
                let error=(relative-reference).dot(normal).abs();max_error=max_error.max(error);
                assert!(error<0.002,"origin={origin:?} far={far} distance={distance} error_m={error}");
            }}
            drop(mapped);readback.unmap();
        }
        println!("primary depth origin={origin:?} far={far} max_plane_error_mm={}",max_error*1000.0);
      }
    }
}

#[test]
fn changing_cabin_clip_range_invalidates_both_depth_histories() {
    let (mut camera,model)=fixture(Vec3::new(0.0,0.0,180.0),24.0);
    let relative=Mat3::from_mat4(model)*Vec3::new(0.85,2.1686,-2.169)
        +(model.w_axis.truncate()-camera.pos);
    let old=camera.relative_view_proj()*relative.extend(1.0);
    camera.zfar=6000.0;
    let new=camera.relative_view_proj()*relative.extend(1.0);
    assert!((old.z/old.w-new.z/new.w).abs()>0.0005,
        "same pose must expose incompatible finite depth values across the two real ranges");
    let source=include_str!("../src/renderer.rs");
    assert!(source.contains("prev_depth_range != [self.camera.znear, self.camera.zfar]"));
    assert!(source.contains("history_valid || cut"));
    assert!(source.contains("volume_history_valid || cut"));
}
