//! Per-view exact source membership and portable visibility buffer binding. Geometry is shared by
//! immutable content; transforms/visibility are read from the same objects used by raster draws.
use super::*;
use crate::scene_visibility::{MaterialClass,VisibilityInstance};
impl Renderer {
    pub(super) fn prepare_scene_visibility(&mut self,device:&wgpu::Device,queue:&wgpu::Queue) {
        if !self.raster_lighting || !self.portable_visibility {return;}
        let meshes=if self.menu_mode {&self.menu_meshes}else{&self.meshes};
        let mut instances=Vec::with_capacity(meshes.len()+self.inst_batches.iter().map(|b|b.inst_count as usize).sum::<usize>());
        for (id,m) in meshes.iter().enumerate() {
            if !m.visible || m.model.determinant().abs()<1e-12 {continue;}
            let Some(geometry)=m.visibility_geometry else {continue;};
            // The environment solver follows actual finite mirrors/lenses. Fixed endpoint
            // connections retain an explicit unsupported result for these optical materials.
            let material=match m.periscope_kind {
                3=>MaterialClass::ThinLens,
                2=>MaterialClass::Mirror,
                1=>MaterialClass::Dielectric,
                _=>if m.glass {MaterialClass::Dielectric}else{MaterialClass::Opaque},
            };
            let center=m.model.transform_point3((m.local_min+m.local_max)*0.5);
            let lens_center_focal=if m.periscope_kind==3 {[center.x,center.y,center.z,m.periscope_focal_m]}else{[0.0;4]};
            instances.push(VisibilityInstance{id:id as u32,geometry,model:m.model,material,material_id:1,bend_world:glam::Vec2::ZERO,lens_center_focal});
        }
        if !self.menu_mode {
            let mut id=0x8000_0000u32;
            for batch in &self.inst_batches {
                let Some(geometry)=self.inst_prototypes[batch.prototype].visibility_geometry else {continue;};
                for (ordinal,&packed) in batch.cpu_mats.iter().take(batch.inst_count as usize).enumerate() {
                    let grass=batch.flags>3.5 && batch.flags<4.5;
                    let bend_world=if grass {glam::Vec2::new(packed.x_axis.w,packed.z_axis.w)}else{glam::Vec2::ZERO};
                    let model=glam::Mat4::from_cols(packed.x_axis.truncate().extend(0.0),
                        packed.y_axis.truncate().extend(0.0),packed.z_axis.truncate().extend(0.0),packed.w_axis.truncate().extend(1.0));
                    let instance_id=id;id=id.checked_add(1).expect("scene visibility instance ID exhaustion");
                    if batch.is_hidden(ordinal as u32) || model.determinant().abs()<1e-12 {continue;}
                    // Alpha uses actual source texels; packed world wind bends the same source
                    // triangle vertices as vs_inst, with conservative bent traversal bounds.
                    let material=if grass {MaterialClass::AlphaTest}else{MaterialClass::Opaque};
                    instances.push(VisibilityInstance{id:instance_id,geometry,model,material,material_id:0,bend_world,lens_center_focal:[0.0;4]});
                }
            }
        }
        self.scene_visibility.update_instances(&instances).expect("invalid visible source instance");
        let upload=self.scene_visibility_gpu.prepare(device,queue,&self.scene_visibility).expect("exact visibility GPU capacity");
        if upload.bindings_changed {self.rebind_scene_visibility(device);}
    }
    fn rebind_scene_visibility(&mut self,device:&wgpu::Device) {
        use wgpu::{BindGroupEntry as E,BindingResource as R};
        let gpu=&self.scene_visibility_gpu;
        self.shadow_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:Some("shadow-and-exact-visibility"),layout:&self.shadow_bgl,entries:&[
                E{binding:0,resource:R::TextureView(&self.shadow_view)},
                E{binding:1,resource:R::Sampler(&self.shadow_sampler)},
                E{binding:2,resource:R::TextureView(&self.near_shadow_view)},
                E{binding:3,resource:self.near_shadow_buf.as_entire_binding()},
                E{binding:4,resource:R::TextureView(&self.cabin_shadows.view)},
                E{binding:5,resource:self.cabin_shadows.uniform.as_entire_binding()},
                E{binding:6,resource:R::TextureView(&self.sky_visibility.view)},
                E{binding:7,resource:self.sky_visibility.uniform.as_entire_binding()},
                E{binding:8,resource:self.environment_probe.uniform.as_entire_binding()},
                E{binding:9,resource:gpu.geometry_buffer().as_entire_binding()},
                E{binding:10,resource:gpu.instance_buffer().as_entire_binding()},
                E{binding:11,resource:R::TextureView(&self.cabin_medium_view)},
            ]});
        self.cabin_shadows.sampling_bind=device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:Some("point-and-exact-visibility"),layout:&self.cabin_shadows.sampling_bgl,entries:&[
                E{binding:0,resource:R::TextureView(&self.cabin_shadows.view)},
                E{binding:1,resource:R::Sampler(&self.shadow_sampler)},
                E{binding:2,resource:self.cabin_shadows.uniform.as_entire_binding()},
                E{binding:3,resource:gpu.geometry_buffer().as_entire_binding()},
                E{binding:4,resource:gpu.instance_buffer().as_entire_binding()},
            ]});
    }
}
