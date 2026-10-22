//! Native-pixel HDR presentation of cosmetic parcels. Parent simulation/source lights are unchanged.
//! Sample count follows projected area; only parent transforms are uploaded, samples run on GPU.
use super::*;
use glam::{Mat4, Vec2, Vec3, Vec4};

const MAX_PER_DRAW: u32 = 65_536;
const MAX_PER_VIEW: u64 = 8_388_608;
const FIRE_FLAG: f32 = 5.0;
const SOOT_FLAG: f32 = 6.0;
#[derive(Clone, Copy)]
struct Material { color:[f32;3], soot:bool, minimum:u32, density:u32 }
#[repr(C)]
#[derive(Clone,Copy,bytemuck::Pod,bytemuck::Zeroable)]
struct Instance { model:[[f32;4];4], emission:[f32;4], parameters:[f32;4] }
#[repr(C)]
#[derive(Clone,Copy,bytemuck::Pod,bytemuck::Zeroable)]
struct ViewUniform { view_proj:[[f32;4];4], eye:[f32;4], extent:[f32;4] }
struct Draw { first:u32, parents:u32, grains:u32 }
#[derive(Default,Clone)]
struct Receipt {
    extent:[u32;2], parents:u64, rendered_grains:u64, requested_grains:u64,
    required_grains:u64, saturated_parents:u64, density_reduced_parents:u64,
    fire_grains:u64,soot_grains:u64,maximum_density_multiplier:u32,
    omitted_projected_area_px:f64, omitted_emission_area_rgb:[f64;3],
    near_plane_intersections:u64, maximum_weight:f32, missing_particle_ids:u64,
    parent_records:u64, draw_calls:u64, upload_bytes:u64,
    gpu_buffer_allocations:u64, instance_capacity:u64, lifetime_buffer_allocations:u64,
}
struct Parent { depth:f32, area:f32, required:u64, desired:u64, count:u32, instance:Instance }
pub(super) struct Frame {
    bind:wgpu::BindGroup, uniform:wgpu::Buffer, buffer:Option<wgpu::Buffer>, capacity:usize,
    draws:Vec<Draw>, instances:Vec<Instance>, parents:Vec<Parent>, receipt:Receipt,
    lifetime_buffer_allocations:u64, active:bool,
}
pub(super) struct FineVfx {
    layout:wgpu::BindGroupLayout, pipeline:wgpu::RenderPipeline,
    materials:HashMap<usize,Material>, particle_ids:HashMap<usize,Vec<[u32;4]>>,
    frames:HashMap<usize,Frame>,
}

fn shader_source()->String {
    let shared=include_str!("../shaders/stylized_smoke.wgsl");
    let prefix=shared.split("struct VsOut").next().expect("shared medium functions");
    let transport=shared.split("    let fog_distance=").nth(1).expect("shared fog transport")
        .rsplit_once('}').unwrap().0.replace("in.pos.xy","native_pixel");
    format!("{prefix}\nfn grain_medium(world:vec3<f32>,native_pixel:vec2<f32>)->vec4<f32>{{\nlet origin=sm.cam_pos;let scene_distance=length(world-origin);let ray=(world-origin)/max(scene_distance,0.0001);\nlet fog_distance={transport}\n}}\n{}",include_str!("../shaders/fine_vfx.wgsl"))
}
impl FineVfx {
    pub fn new(device:&wgpu::Device,sky:&wgpu::BindGroupLayout,environment:&wgpu::BindGroupLayout,cabin:&wgpu::BindGroupLayout)->Self {
        let layout=device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {label:Some("native-grain-view"),entries:&[
            wgpu::BindGroupLayoutEntry {binding:0,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Depth,view_dimension:wgpu::TextureViewDimension::D2,multisampled:false},count:None},
            wgpu::BindGroupLayoutEntry {binding:1,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Buffer {ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
            wgpu::BindGroupLayoutEntry {binding:2,visibility:wgpu::ShaderStages::VERTEX,ty:wgpu::BindingType::Buffer {ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
        ]});
        let pipeline_layout=device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {label:Some("native-grain-layout"),bind_group_layouts:&[&layout,sky,environment,cabin],push_constant_ranges:&[]});
        let shader=super::shader(device,"native-hdr-grains",&shader_source());
        let attrs=wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4];
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:Some("native-hdr-grains"),layout:Some(&pipeline_layout),
            vertex:wgpu::VertexState {module:&shader,entry_point:Some("vs_grain"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout {array_stride:96,step_mode:wgpu::VertexStepMode::Instance,attributes:&attrs}]},
            primitive:wgpu::PrimitiveState {cull_mode:None,..Default::default()},depth_stencil:None,multisample:Default::default(),
            fragment:Some(wgpu::FragmentState {module:&shader,entry_point:Some("fs_grain"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState {format:hdr_format(),blend:Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})]}),multiview:None,cache:None,
        });
        Self {layout,pipeline,materials:HashMap::new(),particle_ids:HashMap::new(),frames:HashMap::new()}
    }
}
struct SourceGeometry { vertices:Vec<Vec3>, triangles:Vec<[usize;3]> }
fn source_geometry(soot:bool)->&'static SourceGeometry {
    static CUBE:OnceLock<SourceGeometry>=OnceLock::new();static SOOT:OnceLock<SourceGeometry>=OnceLock::new();
    let slot=if soot {&SOOT}else{&CUBE};slot.get_or_init(|| {
        let mesh=if soot {crate::prims::faceted_sphere_mesh([255;3]).transformed(Mat4::from_scale(Vec3::splat(0.5)))}
            else {crate::prims::box_mesh(Vec3::splat(-0.5),Vec3::splat(0.5),[255;3])};
        let mut vertices=Vec::new();let mut triangles=Vec::new();
        for tri in mesh.indices.chunks_exact(3) {
            triangles.push([0,1,2].map(|i| {
                let p=Vec3::from_array(mesh.vertices[tri[i] as usize].pos);
                vertices.iter().position(|&v|v==p).unwrap_or_else(||{vertices.push(p);vertices.len()-1})
            }));
        }
        SourceGeometry {vertices,triangles}
    })
}
/// Homogeneous z>=0 is the actual WGPU near plane. Crossing edges retain their exact
/// line/plane intersection; no entire crossing source triangle is discarded.
fn clip_near(triangle:[Vec4;3])->([Vec4;4],usize) {
    let mut out=[Vec4::ZERO;4];let mut count=0;
    let mut previous=triangle[2];
    for current in triangle {
        let previous_inside=previous.z>=0.0;let current_inside=current.z>=0.0;
        if previous_inside != current_inside {
            let t=previous.z/(previous.z-current.z);
            out[count]=previous.lerp(current,t);out[count].z=0.0;count+=1;
        }
        if current_inside {out[count]=current;count+=1;}
        previous=current;
    }
    (out,count)
}
fn convex_projected_area(points:&mut Vec<Vec2>)->f32 {
    points.sort_by(|a,b|a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();if points.len()<3{return 0.0;}
    let mut hull:Vec<Vec2>=Vec::with_capacity(points.len()*2);
    for &p in points.iter() {
        while hull.len()>=2 && (hull[hull.len()-1]-hull[hull.len()-2]).perp_dot(p-hull[hull.len()-1])<=0.0 {hull.pop();}
        hull.push(p);
    }
    let lower=hull.len();
    for &p in points[..points.len()-1].iter().rev() {
        while hull.len()>lower && (hull[hull.len()-1]-hull[hull.len()-2]).perp_dot(p-hull[hull.len()-1])<=0.0 {hull.pop();}
        hull.push(p);
    }
    hull.windows(2).map(|edge|edge[0].perp_dot(edge[1])).sum::<f32>().abs()*0.5
}
fn projected_area(model:Mat4,camera:&Camera,extent:[u32;2],soot:bool)->(f32,bool) {
    projected_area_with_view(model,camera.pos,camera.relative_view_proj(),extent,soot)
}
fn projected_area_with_view(model:Mat4,eye:Vec3,relative_vp:Mat4,extent:[u32;2],soot:bool)->(f32,bool) {
    let geometry=source_geometry(soot);
    let mut relative=model;relative.w_axis=(model.w_axis.truncate()-eye).extend(1.0);
    let vp=relative_vp*relative;
    let mut clip=[Vec4::ZERO;12];
    for (i,p) in geometry.vertices.iter().enumerate(){clip[i]=vp*p.extend(1.0);}
    let clips=&clip[..geometry.vertices.len()];
    let behind=clips.iter().filter(|p|p.z<0.0).count();
    if behind==clips.len(){return (0.0,false);}
    let scale=Vec2::new(extent[0] as f32,extent[1] as f32)*0.5;
    if behind==0 {
        let projected=clip.map(|p|p.truncate().truncate()/p.w.max(f32::MIN_POSITIVE)*scale);
        let area=geometry.triangles.iter().map(|t| {
            let a=projected[t[0]];let b=projected[t[1]];let c=projected[t[2]];
            (b-a).perp_dot(c-a).abs()*0.25
        }).sum();
        return (area,false);
    }
    let mut points=Vec::with_capacity(geometry.triangles.len()*4);
    for triangle in &geometry.triangles {
        let (polygon,count)=clip_near(triangle.map(|i|clip[i]));
        points.extend(polygon[..count].iter().filter(|p|p.w>0.0)
            .map(|p|p.truncate().truncate()/p.w*scale));
    }
    // Near clipping opens the original closed surface. Its projected convex hull includes the
    // source-derived cut face, unlike halving the remaining front/back triangle areas.
    (convex_projected_area(&mut points),true)
}
fn sample_request(area:f32,minimum:u32)->(u64,u64) {
    let required=((area as f64*2.0).ceil() as u64).max(1);
    let wanted=required.max(u64::from(minimum));
    (required,wanted.checked_next_power_of_two().unwrap_or(u64::MAX))
}
fn dense_sample_request(area:f32,minimum:u32,density:u32)->(u64,u64) {
    // The producer's minimum is optional quadrature, not additional particles or radiance.
    // Do not rasterize 64*8 samples of a parcel smaller than one native pixel: those quads
    // repeatedly shade the same pixel and each repeats the full medium integral. Bound this
    // minimum by its actual projected pixel support. Required coverage remains unchanged;
    // area/count weights and the stable source sample prefix still conserve parcel energy.
    let visible_minimum=minimum.min(area.ceil().max(1.0) as u32);
    let (required,desired)=sample_request(area,visible_minimum);
    (required,desired.saturating_mul(u64::from(density.max(1))))
}
fn sample_chunks(total:u32)->impl Iterator<Item=(u32,u32)> {
    (0..total).step_by(MAX_PER_DRAW as usize).map(move |offset|(offset,(total-offset).min(MAX_PER_DRAW)))
}
fn particle_seed(id:[u32;4])->u32 {
    let mut hash=0x7f4a_7c15u32;
    for word in id {hash^=word;hash=hash.wrapping_mul(0x9e37_79b9);hash^=hash>>16;}
    hash & 0x00ff_ffff // exactly representable in the existing f32 instance lane
}
fn buffer_capacity(current:usize,needed:usize)->usize {
    if needed<=current {current}else{needed.max(64).next_power_of_two()}
}
/// First discard optional oversampling, never necessary coverage. If necessary coverage itself
/// cannot fit, admit only whole parents at valid weight and expose all omitted presentation energy.
/// This failure mode changes the rendered quadrature and is not an accepted capture.
fn allocate_counts(parents:&mut [Parent],budget:u64) {
    let required=parents.iter().fold(0u64,|sum,p|sum.saturating_add(p.required));
    let desired=parents.iter().fold(0u64,|sum,p|sum.saturating_add(p.desired));
    if desired<=budget {for p in parents {p.count=p.desired as u32;}return;}
    if required<=budget {
        let available=budget-required;let extras=desired-required;let mut assigned=required;
        for p in parents.iter_mut() {
            let extra=(((p.desired-p.required) as u128*available as u128)/extras as u128) as u64;
            p.count=(p.required+extra) as u32;assigned+=extra;
        }
        for p in parents.iter_mut() {
            if assigned==budget {break;}
            if u64::from(p.count)<p.desired {p.count+=1;assigned+=1;}
        }
    } else {
        let mut remaining=budget;
        // The array is globally far-to-near. Keep nearer whole parents first during an explicit
        // saturation failure; omitted distant presentation is counted rather than hidden.
        for p in parents.iter_mut().rev() {
            p.count=if p.required<=remaining {remaining-=p.required;p.required as u32}else{0};
        }
    }
}
impl FineVfx {
    /// Linear colour of a registered grain batch (the raymarch backend turns these into media).
    pub(super) fn material_color(&self,batch:usize)->Option<[f32;3]> {self.materials.get(&batch).map(|m|m.color)}
    pub(super) fn copy_registrations_from(&mut self,other:&Self) {
        self.materials.clone_from(&other.materials);self.particle_ids.clone_from(&other.particle_ids);
    }
}
impl Renderer {
    pub(crate) fn set_fine_vfx_batch(&mut self,batch:usize,color:[u8;3],soot:bool,minimum:u32) {
        if !self.shader_backend {return;}
        self.fine_vfx.materials.insert(batch,Material {color:color.map(|v|crate::rt_surface::srgb_to_linear(v as f32/255.0)),soot,minimum:if soot {1}else{minimum},density:1});
        self.set_batch_flags(batch,if soot {SOOT_FLAG}else{FIRE_FLAG});
    }
    /// Increase quadrature density inside the same parcel. Shader weight remains area/count;
    /// source illumination, grain pixel dimensions, plume support and sample identities stay fixed.
    pub(crate) fn set_fine_vfx_density(&mut self,batch:usize,density:u32)->bool {
        if !self.shader_backend {return true;}
        let Some(material)=self.fine_vfx.materials.get_mut(&batch) else{return false;};
        material.density=density.clamp(1,16);true
    }
    pub(super) fn is_fine_vfx_batch(&self,batch:usize)->bool {self.shader_backend && self.fine_vfx.materials.contains_key(&batch)}
    pub(crate) fn set_fine_vfx_particle_ids(&mut self,batch:usize,ids:&[[u32;4]])->bool {
        if !self.shader_backend {return true;}
        let Some(b)=self.inst_batches.get(batch) else{return false;};
        if !self.fine_vfx.materials.contains_key(&batch) || ids.len()!=b.inst_count as usize {return false;}
        let stored=self.fine_vfx.particle_ids.entry(batch).or_default();stored.clear();stored.extend_from_slice(ids);true
    }
    pub(super) fn prepare_fine_vfx_view(&self,device:&wgpu::Device,queue:&wgpu::Queue,
        camera:&Camera,extent:[u32;2],depth:&wgpu::TextureView,smoke:&wgpu::Buffer,previous:Option<Frame>)->Frame {
        let mut allocations=0;
        let mut frame=previous.unwrap_or_else(|| {
            allocations+=1;
            let uniform=device.create_buffer(&wgpu::BufferDescriptor {label:Some("native-grain-camera"),size:std::mem::size_of::<ViewUniform>() as u64,
                usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
            let bind=device.create_bind_group(&wgpu::BindGroupDescriptor {label:Some("native-grain-view"),layout:&self.fine_vfx.layout,entries:&[
                wgpu::BindGroupEntry {binding:0,resource:wgpu::BindingResource::TextureView(depth)},
                wgpu::BindGroupEntry {binding:1,resource:smoke.as_entire_binding()},wgpu::BindGroupEntry {binding:2,resource:uniform.as_entire_binding()},
            ]});
            Frame {bind,uniform,buffer:None,capacity:0,draws:Vec::new(),instances:Vec::new(),parents:Vec::new(),receipt:Receipt::default(),lifetime_buffer_allocations:0,active:true}
        });
        let view=ViewUniform {view_proj:camera.relative_view_proj().to_cols_array_2d(),eye:camera.pos.extend(0.0).to_array(),extent:[extent[0] as f32,extent[1] as f32,0.0,0.0]};
        queue.write_buffer(&frame.uniform,0,bytemuck::bytes_of(&view));
        frame.parents.clear();frame.instances.clear();frame.draws.clear();frame.active=true;
        let mut receipt=Receipt {extent,..Default::default()};
        let relative_vp=camera.relative_view_proj();
        let planes=frustum_planes(relative_vp);
        let mut source_batches:Vec<_>=self.fine_vfx.materials.iter().collect();source_batches.sort_by_key(|(i,_)|**i);
        for (&batch,material) in source_batches {
            let b=&self.inst_batches[batch];let p=&self.inst_prototypes[b.prototype];
            let ids=self.fine_vfx.particle_ids.get(&batch);
            for (ordinal,model) in b.cpu_mats.iter().take(b.inst_count as usize).enumerate() {
                let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
                for i in 0..8 {let point=model.transform_point3(Vec3::new(if i&1==0 {p.local_min.x}else{p.local_max.x},if i&2==0 {p.local_min.y}else{p.local_max.y},if i&4==0 {p.local_min.z}else{p.local_max.z}))-camera.pos;lo=lo.min(point);hi=hi.max(point);}
                if !aabb_in_frustum(&planes,lo,hi){continue;}
                let (area,near)=projected_area_with_view(*model,camera.pos,relative_vp,extent,material.soot);
                if area<=0.0 || !area.is_finite(){continue;}
                let (required,desired)=dense_sample_request(area,material.minimum,material.density);
                receipt.maximum_density_multiplier=receipt.maximum_density_multiplier.max(material.density);
                receipt.parents+=1;receipt.requested_grains=receipt.requested_grains.saturating_add(desired);
                receipt.required_grains=receipt.required_grains.saturating_add(required);
                receipt.near_plane_intersections+=u64::from(near);
                let seed=ids.and_then(|ids|ids.get(ordinal)).map(|&id|particle_seed(id)).unwrap_or_else(||{receipt.missing_particle_ids+=1;0});
                frame.parents.push(Parent {depth:camera.pos.distance_squared(model.w_axis.truncate()),area,required,desired,count:0,
                    instance:Instance {model:model.to_cols_array_2d(),emission:[material.color[0],material.color[1],material.color[2],if material.soot {-1.0}else{b.emissive}],
                        parameters:[area,0.0,seed as f32,0.0]}});
            }
        }
        frame.parents.sort_by(|a,b|b.depth.total_cmp(&a.depth));
        allocate_counts(&mut frame.parents,MAX_PER_VIEW);
        for parent in &frame.parents {
            if parent.count==0 {
                receipt.saturated_parents+=1;receipt.omitted_projected_area_px+=parent.area as f64;
                if parent.instance.emission[3]>=0.0 {
                    let e=parent.instance.emission[3] as f64;let radiance=e+(e-3.0).max(0.0)*0.15;
                    for c in 0..3 {receipt.omitted_emission_area_rgb[c]+=parent.instance.emission[c] as f64*radiance*parent.area as f64;}
                }
                continue;
            }
            receipt.density_reduced_parents+=u64::from(u64::from(parent.count)<parent.desired);
            receipt.rendered_grains+=u64::from(parent.count);
            if parent.instance.emission[3]>=0.0 {receipt.fire_grains+=u64::from(parent.count);}
            else {receipt.soot_grains+=u64::from(parent.count);}
            receipt.maximum_weight=receipt.maximum_weight.max(parent.area/parent.count as f32);
            // Split a very close parent into bounded GPU draws without collapsing its whole
            // projected emission into an overbright pixel or restarting its local sample prefix.
            for (offset,grains) in sample_chunks(parent.count) {
                let mut instance=parent.instance;instance.parameters[1]=parent.count as f32;instance.parameters[3]=offset as f32;
                let first=frame.instances.len() as u32;frame.instances.push(instance);
                if let Some(last)=frame.draws.last_mut().filter(|d|d.grains==grains) {last.parents+=1;}
                else {frame.draws.push(Draw {first,parents:1,grains});}
            }
        }
        assert!(receipt.rendered_grains<=MAX_PER_VIEW);
        let needed=frame.instances.len();let capacity=buffer_capacity(frame.capacity,needed);
        if capacity>frame.capacity {
            frame.buffer=Some(device.create_buffer(&wgpu::BufferDescriptor {label:Some("native-grain-parent-instances"),size:(capacity*std::mem::size_of::<Instance>()) as u64,
                usage:wgpu::BufferUsages::VERTEX|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false}));
            frame.capacity=capacity;allocations+=1;
        }
        if needed>0 {queue.write_buffer(frame.buffer.as_ref().unwrap(),0,bytemuck::cast_slice(&frame.instances));}
        frame.lifetime_buffer_allocations+=allocations;
        receipt.gpu_buffer_allocations=allocations;receipt.instance_capacity=frame.capacity as u64;
        receipt.lifetime_buffer_allocations=frame.lifetime_buffer_allocations;
        receipt.parent_records=needed as u64;receipt.draw_calls=frame.draws.len() as u64;
        receipt.upload_bytes=(needed*std::mem::size_of::<Instance>()+std::mem::size_of::<ViewUniform>()) as u64;
        frame.receipt=receipt;frame
    }
    pub(super) fn prepare_fine_vfx(&mut self,device:&wgpu::Device,queue:&wgpu::Queue) {
        if !self.shader_backend {return;}
        let mut frames=std::mem::take(&mut self.fine_vfx.frames);
        for frame in frames.values_mut(){frame.active=false;}
        let main=self.prepare_fine_vfx_view(device,queue,&self.camera,[self.iw,self.ih],&self.depth_view,&self.smoke_buf,frames.remove(&usize::MAX));
        frames.insert(usize::MAX,main);self.prepare_fine_vfx_ports(device,queue,&mut frames);
        self.fine_vfx.frames=frames;
    }
    pub(super) fn encode_fine_vfx(&self,encoder:&mut wgpu::CommandEncoder,key:usize,color:&wgpu::TextureView,work:&mut RenderWorkStats) {
        let Some(frame)=self.fine_vfx.frames.get(&key) else{return;};if !frame.active || frame.draws.is_empty(){return;}
        let mut pass=encoder.begin_render_pass(&wgpu::RenderPassDescriptor {label:Some("native-pixel-hdr-effects"),color_attachments:&[Some(attachment(color,wgpu::LoadOp::Load))],depth_stencil_attachment:None,timestamp_writes:self.gpu_profile.as_ref().and_then(|p|p.render("native-pixel-hdr-effects")),occlusion_query_set:None});
        let [width,height]=frame.receipt.extent;
        pass.set_viewport(0.0,0.0,width as f32,height as f32,0.0,1.0);
        pass.set_scissor_rect(0,0,width,height);
        pass.set_pipeline(&self.fine_vfx.pipeline);pass.set_bind_group(0,&frame.bind,&[]);
        pass.set_bind_group(1,&self.sky_visibility.sampling_bind,&[]);pass.set_bind_group(2,&self.physical_environment_bind,&[]);pass.set_bind_group(3,&self.cabin_shadows.sampling_bind,&[]);
        pass.set_vertex_buffer(0,frame.buffer.as_ref().unwrap().slice(..));
        for draw in &frame.draws {pass.draw(0..draw.grains*6,draw.first..draw.first+draw.parents);work.scene_instanced_draws+=1;}
    }
    pub(crate) fn debug_fine_vfx_receipt(&self)->serde_json::Value {
        let mut views:Vec<_>=self.fine_vfx.frames.iter().filter(|(_,f)|f.active).map(|(&key,frame)| {
            let r=&frame.receipt;
            serde_json::json!({"view":if key==usize::MAX {"main".into()}else{format!("port{key}")},"counts":{
                "extent":r.extent,"parents":r.parents,"rendered_grains":r.rendered_grains,"requested_grains":r.requested_grains,
                "required_grains":r.required_grains,"fire_grains":r.fire_grains,"soot_grains":r.soot_grains,
                "maximum_density_multiplier":r.maximum_density_multiplier,"saturated_parents":r.saturated_parents,"density_reduced_parents":r.density_reduced_parents,
                "omitted_projected_area_px":r.omitted_projected_area_px,"omitted_emission_area_rgb":r.omitted_emission_area_rgb,
                "near_plane_intersections":r.near_plane_intersections,"maximum_weight":r.maximum_weight,"missing_particle_ids":r.missing_particle_ids,
                "parent_records":r.parent_records,"draw_calls":r.draw_calls,"upload_bytes":r.upload_bytes,
                "gpu_buffer_allocations":r.gpu_buffer_allocations,"instance_capacity":r.instance_capacity,"lifetime_buffer_allocations":r.lifetime_buffer_allocations}})
        }).collect();
        views.sort_by(|a,b|a["view"].as_str().cmp(&b["view"].as_str()));
        serde_json::json!({"path":"one-native-pixel HDR quads; shared raster medium lighting and attenuation","max_grains_per_draw":MAX_PER_DRAW,"max_grains_per_view":MAX_PER_VIEW,"views":views,"transparency":"parents globally sorted back to front across all materials/counts; no per-grain order sort"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_coverage_scales_with_resolution_depth_fov_and_port_extent() {
        let mut camera=Camera::new(16.0/9.0);camera.pos=Vec3::ZERO;camera.set_forward(Vec3::Z);
        let model=Mat4::from_scale_rotation_translation(Vec3::splat(0.1),glam::Quat::IDENTITY,Vec3::Z*2.0);
        let a=projected_area(model,&camera,[640,360],false).0;
        let b=projected_area(model,&camera,[960,540],false).0;
        let port=projected_area(model,&camera,[64,36],false).0;
        assert!((b/a-2.25).abs()<1e-4);assert!((port/a-0.01).abs()<1e-5);
        let farther=Mat4::from_translation(Vec3::Z*2.0)*model;
        assert!(projected_area(farther,&camera,[640,360],false).0<a*0.26);
        camera.fov_y*=0.5;assert!(projected_area(model,&camera,[640,360],false).0>a*3.9);
    }
    #[test]
    fn grain_density_conserves_projected_energy_and_smoke_optical_depth() {
        for area in [0.001_f32,0.3,1.0,15.0,100.0,2000.0,32000.0] {
            let (required,n)=sample_request(area,64);assert!(n>=required);
            let weight=area/n as f32;assert!(weight<=0.500001);
            assert!((weight*n as f32-area).abs()<area*1e-6+1e-7);
            let t=(-weight as f64).exp().powi(n as i32);
            assert!((t-(-(area as f64)).exp()).abs()<1e-7);
        }
        assert!(sample_request(100_000.0,64).0>u64::from(MAX_PER_DRAW));
    }
    #[test]
    fn eightfold_fire_density_preserves_area_energy_opacity_and_pixel_size() {
        for area in [0.001_f32,0.3,1.0,15.0,100.0,2000.0,32000.0] {
            for minimum in [1,64] {
                let (required,base)=dense_sample_request(area,minimum,1);
                let (dense_required,dense)=dense_sample_request(area,minimum,8);
                assert_eq!(dense_required,required);assert_eq!(dense,base*8);
                let weight=area/dense as f32;
                assert!(weight<=0.500001);
                assert!((weight*dense as f32-area).abs()<area*1e-6+1e-7);
                let opacity=1.0-(-(weight as f64)*dense as f64).exp();
                assert!((opacity-(1.0-(-(area as f64)).exp())).abs()<1e-7);
            }
        }
        let shader=include_str!("../shaders/fine_vfx.wgsl");
        assert!(shader.contains("let edge=(pixel+corners[corner])/grain_cam.extent.xy"));
        assert!(shader.contains("out.weight=parameters.x/parameters.y"));
    }
    #[test]
    fn subpixel_fire_uses_projected_sampling_without_hot_pixels_or_energy_loss() {
        for area in [0.00001_f32,0.001,0.05,0.1,0.49,0.5,0.51,0.99,1.0,1.01,15.0,32.0,32000.0] {
            let (old_required,old_base)=sample_request(area,64);
            let (required,samples)=dense_sample_request(area,64,8);
            assert_eq!(required,old_required,"the minimum coverage budget must never shrink");
            assert!(samples>=required && samples<=old_base*8);
            let weight=f64::from(area)/samples as f64;
            assert!(weight<=0.0625,"8x coverage cannot concentrate a parcel into a hot pixel");
            let radiance=[0.0001_f64,0.2,4.0,9.0];
            for emission in radiance {
                let old=emission*f64::from(area)/(old_base*8) as f64*(old_base*8) as f64;
                let new=emission*weight*samples as f64;
                assert!((old-new).abs()<1e-9+old.abs()*1e-12);
            }
            let old_transmission=(-f64::from(area)/(old_base*8) as f64).exp().powi((old_base*8) as i32);
            let new_transmission=(-weight).exp().powi(samples as i32);
            assert!((old_transmission-new_transmission).abs()<1e-11);
            if area<=0.5 {assert_eq!(samples,8,"subpixel cells do not need 512 transport evaluations");}
            if area>=32.0 {assert_eq!(samples,old_base*8,"resolved parcels retain the previous quadrature");}
        }
    }
    #[test]
    fn near_plane_clips_crossing_triangles_and_reconstructs_source_cut_face() {
        let (polygon,n)=clip_near([Vec4::new(-1.0,-1.0,-1.0,1.0),Vec4::new(1.0,-1.0,1.0,1.0),Vec4::new(0.0,1.0,1.0,1.0)]);
        assert_eq!(n,4);assert!(polygon[..n].iter().all(|p|p.z>=0.0));
        let mut points=polygon[..n].iter().map(|p|p.truncate().truncate()/p.w).collect();
        assert!((convex_projected_area(&mut points)-1.5).abs()<1e-6);
        let mut camera=Camera::new(16.0/9.0);camera.set_forward(Vec3::Z);camera.znear=0.75;
        let (area,crossing)=projected_area(Mat4::from_translation(Vec3::Z),&camera,[640,360],false);
        let focal=180.0/(camera.fov_y*0.5).tan();let expected=(focal/camera.znear).powi(2);
        assert!(crossing);assert!((area/expected-1.0).abs()<2e-5,"{area} vs {expected}");
        let (behind,_) = projected_area(Mat4::from_translation(Vec3::NEG_Z),&camera,[640,360],false);
        assert_eq!(behind,0.0);
    }
    #[test]
    fn camera_relative_projection_survives_large_shared_translation() {
        let mut camera=Camera::new(16.0/9.0);camera.set_forward(Vec3::Z);
        let model=Mat4::from_translation(Vec3::new(0.25,0.0,2.0));
        let base=projected_area(model,&camera,[800,450],false).0;
        let shift=Vec3::new(1_000_000.0,2_000_000.0,-1_000_000.0);camera.pos+=shift;
        let translated=projected_area(Mat4::from_translation(shift)*model,&camera,[800,450],false).0;
        assert!((translated/base-1.0).abs()<1e-6);
    }
    fn parent(area:f32,minimum:u32)->Parent {
        let (required,desired)=sample_request(area,minimum);
        Parent {depth:0.0,area,required,desired,count:0,instance:Instance {
            model:Mat4::IDENTITY.to_cols_array_2d(),emission:[1.0;4],parameters:[0.0;4]}}
    }
    #[test]
    fn small_soot_reduces_work_without_losing_optical_depth() {
        for area in [0.001_f32,0.1,0.49] {
            let (_,old)=sample_request(area,64);let (_,new)=sample_request(area,1);
            assert_eq!(old,64);assert_eq!(new,1);
            let before=(-(area as f64)/old as f64).exp().powi(old as i32);
            let after=(-(area as f64)/new as f64).exp().powi(new as i32);
            assert!((before-after).abs()<1e-12);
        }
    }
    #[test]
    fn hard_budget_removes_optional_work_then_reports_whole_omissions_without_hot_pixels() {
        let mut p:Vec<_>=(0..100).map(|_|parent(0.1,64)).collect();
        allocate_counts(&mut p,150);
        assert_eq!(p.iter().map(|p|u64::from(p.count)).sum::<u64>(),150);
        assert!(p.iter().all(|p|p.count>0 && p.area/p.count as f32<=0.5));
        // The former fallback-to-one could exceed a cap once more parents arrived after exhaustion.
        allocate_counts(&mut p,40);
        assert_eq!(p.iter().map(|p|u64::from(p.count)).sum::<u64>(),40);
        assert_eq!(p.iter().filter(|p|p.count==0).count(),60);
        assert!(p.iter().filter(|p|p.count>0).all(|p|u64::from(p.count)>=p.required));
        let mut impossible=[parent(100.0,1),parent(0.1,1)];allocate_counts(&mut impossible,3);
        assert_eq!(impossible[0].count,0);assert_eq!(impossible[1].count,1);
        allocate_counts(&mut p,0);assert!(p.iter().all(|p|p.count==0));
    }
    #[test]
    fn close_parent_splits_contiguous_prefix_without_brightening_or_restarting_samples() {
        let mut p=[parent(100_000.0,64)];allocate_counts(&mut p,MAX_PER_VIEW);
        let chunks:Vec<_>=sample_chunks(p[0].count).collect();
        assert!(chunks.len()>1);assert_eq!(chunks[0].0,0);
        assert_eq!(chunks.iter().map(|(_,n)|n).sum::<u32>(),p[0].count);
        for pair in chunks.windows(2){assert_eq!(pair[0].0+pair[0].1,pair[1].0);}
        assert!(chunks.iter().all(|(_,n)|*n<=MAX_PER_DRAW));
        let weight=p[0].area/p[0].count as f32;assert!(weight<=0.5);
        let sum=chunks.iter().map(|(_,n)|*n as f64*weight as f64).sum::<f64>();
        assert!((sum-p[0].area as f64).abs()<0.01);
    }
    #[test]
    fn buffer_growth_reuses_existing_storage_and_particle_ids_ignore_page_order() {
        assert_eq!(buffer_capacity(0,1),64);assert_eq!(buffer_capacity(64,0),64);
        assert_eq!(buffer_capacity(64,63),64);assert_eq!(buffer_capacity(64,65),128);
        assert_eq!(buffer_capacity(128,65),128);
        let id=[0x12u32,0x34,0x56,0x78];let seed=particle_seed(id);
        assert_eq!(seed as f32 as u32,seed);
        let reordered=[id,[1,2,3,4],id];assert_eq!(particle_seed(reordered[0]),particle_seed(reordered[2]));
        for lane in 0..4 {let mut changed=id;changed[lane]+=1;assert_ne!(particle_seed(changed),seed);}
    }
    #[test]
    fn hdr_grain_shader_validates_with_shared_medium_and_native_pixel_contract() {
        let source=shader_source();
        assert!(source.contains("pixel+corners[corner]"));
        assert!(source.contains("let index=vertex/6u+u32(parameters.w)"));
        assert!(source.contains("in.position.z>=textureLoad(depth_tex,pixel,0)"));
        assert!(source.contains("cabin_volume_light_shadow(source.xyz,source.w,point)"));
        let source=super::super::terrain_material_shader_source(&source);
        let module=wgpu::naga::front::wgsl::parse_str(&source).unwrap_or_else(|e|panic!("{}",e.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(wgpu::naga::valid::ValidationFlags::all(),wgpu::naga::valid::Capabilities::all()).validate(&module).unwrap();
    }
}
