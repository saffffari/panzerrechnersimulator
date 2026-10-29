//! Compact analytic spark draws and one-native-pixel authoritative tracer strips.
//! No per-spark CPU transforms; retained buffers hold one80-byte record per burst/segment.
use super::*;
use glam::{Vec3,Vec4};
#[path="weapon_particle_math.rs"]
pub(crate) mod math;
pub(crate) use math::Record;

#[repr(C)]
#[derive(Clone,Copy,bytemuck::Pod,bytemuck::Zeroable)]
struct View {vp:[[f32;4];4],eye:[f32;4],extent:[f32;4]}
pub(super) struct Frame {
    bind:wgpu::BindGroup,uniform:wgpu::Buffer,buffer:wgpu::Buffer,
    draws:Vec<(u32,u32)>,tracer_first:u32,tracers:u32,extent:[u32;2],
    requested:u64,submitted:u64,omitted:u64,omitted_bursts:u32,thinned:u64,budget:u64,active:bool,
    submitted_by_kind:[u64;9],
}
pub(super) struct WeaponParticles {
    layout:wgpu::BindGroupLayout,sparks:wgpu::RenderPipeline,tracers:wgpu::RenderPipeline,
    pub(super) bursts:Vec<Record>,pub(super) segments:Vec<Record>,frames:HashMap<usize,Frame>,
}
fn shader_source()->String {
    let smoke=include_str!("../shaders/stylized_smoke.wgsl");
    let prefix=smoke.split("struct VsOut").next().expect("shared medium declarations");
    let fine=include_str!("../shaders/fine_vfx.wgsl");
    let tau=fine.split("fn grain_tau(").nth(1).unwrap().split("@fragment").next().unwrap();
    let transport=smoke.split("    let fog_distance=").nth(1).unwrap().rsplit_once('}').unwrap().0.replace("in.pos.xy","native_pixel");
    format!("{prefix}\nfn grain_medium(world:vec3<f32>,native_pixel:vec2<f32>)->vec4<f32>{{\nlet origin=sm.cam_pos;let scene_distance=length(world-origin);let ray=(world-origin)/max(scene_distance,0.0001);\nlet fog_distance={transport}\n}}\nfn grain_tau({tau}\n{}",include_str!("../shaders/weapon_particles.wgsl"))
}
impl WeaponParticles {
    pub(super) fn new(device:&wgpu::Device,sky:&wgpu::BindGroupLayout,environment:&wgpu::BindGroupLayout,cabin:&wgpu::BindGroupLayout)->Self {
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {label:Some("analytic-weapon-view"),entries:&[
            wgpu::BindGroupLayoutEntry {binding:0,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Depth,view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
            uniform_entry(1,wgpu::ShaderStages::FRAGMENT),uniform_entry(2,wgpu::ShaderStages::VERTEX),
        ]});
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {label:Some("analytic-weapon-layout"),bind_group_layouts:&[&layout,sky,environment,cabin],push_constant_ranges:&[]});
        let module=shader(device,"analytic-weapon-particles",&shader_source());
        let attrs=wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Uint32x4];
        let pipeline=|label,entry|device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some(label),layout:Some(&pipeline_layout),
            vertex:wgpu::VertexState {module:&module,entry_point:Some(entry),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout {
                array_stride:std::mem::size_of::<Record>() as u64,step_mode:wgpu::VertexStepMode::Instance,attributes:&attrs}]},
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:None,multisample:Default::default(),
            fragment:Some(wgpu::FragmentState {module:&module,entry_point:Some("fs_weapon"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState {
                format:hdr_format(),blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),
            multiview:None,cache:None,
        });
        Self {sparks:pipeline("analytic-sparks","vs_spark"),tracers:pipeline("one-pixel-tracers","vs_tracer"),layout,
            bursts:Vec::with_capacity(math::MAX_BURSTS),segments:Vec::with_capacity(math::MAX_TRACERS),frames:HashMap::new()}
    }
}
impl Renderer {
    pub(crate) fn analytic_weapon_particles_active(&self)->bool {self.shader_backend}
    pub(crate) fn has_weapon_tracers(&self)->bool {!self.weapon_particles.segments.is_empty()}
    pub(crate) fn set_analytic_weapon_particles(&mut self,bursts:&[Record],segments:&[Record]) {
        assert!(bursts.len()<=math::MAX_BURSTS && segments.len()<=math::MAX_TRACERS,"bounded weapon records");
        self.weapon_particles.bursts.clear();self.weapon_particles.bursts.extend_from_slice(bursts);
        self.weapon_particles.segments.clear();self.weapon_particles.segments.extend_from_slice(segments);
    }
    /// `budget`: the samples this view may draw. The main view keeps `MAX_SPARKS_PER_VIEW`; a relayed
    /// port takes its preset's `relay_effect_budget`, so a 240x136 port no longer draws 47 samples a
    /// pixel (1.5 million in one strike, 54 ms at the loader's ports, round 14).
    pub(super) fn prepare_weapon_particle_view(&self,device:&wgpu::Device,queue:&wgpu::Queue,camera:&Camera,
        extent:[u32;2],depth:&wgpu::TextureView,smoke:&wgpu::Buffer,previous:Option<Frame>,budget:u64)->Frame {
        let mut frame=previous.unwrap_or_else(|| {
            let uniform=device.create_buffer(&wgpu::BufferDescriptor {label:Some("analytic-weapon-camera"),size:std::mem::size_of::<View>() as u64,
                usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
            let buffer=device.create_buffer(&wgpu::BufferDescriptor {label:Some("bounded-weapon-emitter-records"),
                size:((math::MAX_BURSTS+math::MAX_TRACERS)*std::mem::size_of::<Record>()) as u64,
                usage:wgpu::BufferUsages::VERTEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
            let bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("analytic-weapon-view"),layout:&self.weapon_particles.layout,entries:&[
                wgpu::BindGroupEntry {binding:0,resource:wgpu::BindingResource::TextureView(depth)},
                wgpu::BindGroupEntry {binding:1,resource:smoke.as_entire_binding()},wgpu::BindGroupEntry {binding:2,resource:uniform.as_entire_binding()},
            ]});
            Frame {bind,uniform,buffer,draws:Vec::new(),tracer_first:0,tracers:0,extent,requested:0,submitted:0,omitted:0,omitted_bursts:0,thinned:0,budget,active:true,submitted_by_kind:[0;9]}
        });
        frame.draws.clear();frame.extent=extent;frame.active=true;frame.requested=0;frame.submitted=0;frame.omitted=0;frame.omitted_bursts=0;frame.submitted_by_kind=[0;9];
        frame.thinned=0;frame.budget=budget;
        let view=View {vp:camera.relative_view_proj().to_cols_array_2d(),eye:camera.pos.extend(0.0).to_array(),
            extent:[extent[0] as f32,extent[1] as f32,extent[1] as f32/(2.0*(camera.fov_y*0.5).tan()),0.0]};
        queue.write_buffer(&frame.uniform,0,bytemuck::bytes_of(&view));
        let planes=frustum_planes(camera.relative_view_proj());
        let mut bursts:Vec<_>=self.weapon_particles.bursts.iter().copied().filter(|r|r.active()).collect();
        bursts.sort_by(|a,b|a.origin().distance_squared(camera.pos).total_cmp(&b.origin().distance_squared(camera.pos)));
        let mut records=Vec::with_capacity(bursts.len()+self.weapon_particles.segments.len());
        let visible:Vec<_>=bursts.into_iter().filter(|burst| {
            let center=burst.origin()-camera.pos;let radius=Vec3::splat(burst.support_radius());
            aabb_in_frustum(&planes,center-radius,center+radius)
        }).collect();
        let budget=budget.min(u64::from(math::MAX_SPARKS_PER_VIEW));
        let counts=math::thinned_counts(&visible,budget);
        let mut available=budget;
        for (mut burst,drawn) in visible.into_iter().zip(counts) {
            let count=burst.meta[1];frame.requested+=u64::from(count);
            // Over budget every burst draws its share of samples (`math::thinned_counts`), each
            // carrying the emission of the samples it stands for: the shower's light is kept and
            // its density falls. Only a burst that cannot draw even its own source samples is
            // left out, nearest first kept, and counted.
            if u64::from(drawn)>available {frame.omitted+=u64::from(count);frame.omitted_bursts+=1;continue;}
            available-=u64::from(drawn);frame.thinned+=u64::from(count-drawn);burst.meta[1]=drawn;
            frame.submitted+=u64::from(drawn);frame.submitted_by_kind[burst.meta[3] as usize]+=u64::from(drawn);
            frame.draws.push((records.len() as u32,drawn));records.push(burst);
        }
        // Additive sparks do not depend on ordering; matte ground grains require far-to-near
        // emitter order. Individual grains remain an explicitly approximate unsorted mixture.
        frame.draws.sort_by(|a,b|records[b.0 as usize].origin().distance_squared(camera.pos)
            .total_cmp(&records[a.0 as usize].origin().distance_squared(camera.pos)));
        frame.tracer_first=records.len() as u32;frame.tracers=self.weapon_particles.segments.len() as u32;
        records.extend_from_slice(&self.weapon_particles.segments);
        if !records.is_empty(){queue.write_buffer(&frame.buffer,0,bytemuck::cast_slice(&records));}
        frame
    }
    pub(super) fn prepare_weapon_particles(&mut self,device:&wgpu::Device,queue:&wgpu::Queue) {
        if !self.shader_backend{return;}
        let mut frames=std::mem::take(&mut self.weapon_particles.frames);for frame in frames.values_mut(){frame.active=false;}
        // The raymarch backend draws its own sparks in the main view; only its relayed ports take these.
        if self.raymarch.is_none() {
            let main=self.prepare_weapon_particle_view(device,queue,&self.camera,[self.iw,self.ih],&self.depth_view,&self.smoke_buf,frames.remove(&usize::MAX),
                u64::from(math::MAX_SPARKS_PER_VIEW));
            frames.insert(usize::MAX,main);
        }
        self.prepare_weapon_particle_ports(device,queue,&mut frames);self.weapon_particles.frames=frames;
    }
    pub(super) fn encode_weapon_particles(&self,encoder:&mut wgpu::CommandEncoder,key:usize,color:&wgpu::TextureView,
        tracers:bool,clear:bool,work:&mut RenderWorkStats) {
        let Some(frame)=self.weapon_particles.frames.get(&key) else{return;};
        if !frame.active || (!clear && if tracers {frame.tracers==0}else{frame.draws.is_empty()}){return;}
        let label=if tracers {"native-one-pixel-tracers-no-bloom"}else{"analytic-thousandfold-sparks"};
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {label:Some(label),
            color_attachments:&[Some(attachment(color,if clear {wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)}else{wgpu::LoadOp::Load}))],
            depth_stencil_attachment:None,timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render(label)),occlusion_query_set:None});
        let [w,h]=frame.extent;pass.set_viewport(0.0,0.0,w as f32,h as f32,0.0,1.0);pass.set_scissor_rect(0,0,w,h);
        pass.set_pipeline(if tracers {&self.weapon_particles.tracers}else{&self.weapon_particles.sparks});pass.set_bind_group(0,&frame.bind,&[]);
        pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);pass.set_bind_group(2,&self.physical_environment_bind,&[]);pass.set_bind_group(3,&self.cabin_shadows.sampling_bind,&[]);
        pass.set_vertex_buffer(0,frame.buffer.slice(..));
        if tracers {if frame.tracers>0 {pass.draw(0..6,frame.tracer_first..frame.tracer_first+frame.tracers);work.scene_instanced_draws+=1;}}
        else {for &(first,count) in &frame.draws {pass.draw(0..count*6,first..first+1);work.scene_instanced_draws+=1;}}
    }
    pub(crate) fn debug_weapon_particle_receipt(&self)->serde_json::Value {
        let mut views:Vec<_>=self.weapon_particles.frames.iter().filter(|(_,f)|f.active).map(|(&key,f)|serde_json::json!({
            "view":if key==usize::MAX {"main".into()}else{format!("port{key}")},"native_extent":f.extent,
            "requested_particles":f.requested,"submitted_particles":f.submitted,"omitted_particles":f.omitted,
            "thinned_particles":f.thinned,"sample_budget":f.budget,
            "submitted_sparks":f.submitted_by_kind[..4].iter().sum::<u64>(),"submitted_pressure_jet_grains":f.submitted_by_kind[math::PRESSURE_VENT as usize],
            "submitted_full_cookoff_grains":f.submitted_by_kind[math::FULL_COOKOFF as usize],"submitted_ground_grains":f.submitted_by_kind[math::GROUND as usize],"omitted_bursts":f.omitted_bursts,
            "submitted_hot_rim_grains":f.submitted_by_kind[math::HOT_RIM as usize],
            "burst_records":f.draws.len(),"tracer_records":f.tracers,"uploaded_record_bytes":(f.draws.len()+f.tracers as usize)*80,
            "spark_emission_weight_per_old_sample":1.0/math::SPARK_MULTIPLIER as f64,
            "scope":"submitted analytic samples; dead, source-plane rejected and depth-occluded samples do not produce pixels"
        })).collect();views.sort_by(|a,b|a["view"].as_str().cmp(&b["view"].as_str()));
        serde_json::json!({"spark_multiplier":math::SPARK_MULTIPLIER,"spark_cap_per_view":math::MAX_SPARKS_PER_VIEW,
            "exterior_blast_volume_multiplier":math::EXTERIOR_VOLUME_MULTIPLIER,"tracer_width_native_pixels":1,
            "tracer_stage":"post-bloom HDR, depth-tested, shared medium attenuation; no white core",
            "full_cookoff_apex_target_m":math::COOKOFF_APEX_M,"full_cookoff_cosmetic_mass_estimate_kg":5.0,
            "full_cookoff_inherited_detonation_energy_fraction":0.002,
            "ground_ejecta_residual_energy_fraction_max":0.10,"ground_height_cap_m":math::GROUND_APEX_TARGET_M,
            "ground_mass_model":"estimated caliber cylinder and220/1600kg/m3 snow/soil; not measured crater physics",
            "views":views})
    }
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn analytic_weapon_shader_validates_with_shared_medium() {
        let source=super::super::terrain_material_shader_source(&shader_source());
        let module=wgpu::naga::front::wgsl::parse_str(&source).unwrap_or_else(|e|panic!("{}",e.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
        assert!(source.contains("grain_tau(in.world)"));assert!(source.contains("textureLoad(depth_tex,pixel,0)"));
    }
}
