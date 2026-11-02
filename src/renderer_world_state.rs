//! Physical world clocks and retained pressure/weather state. GPU histories are rebuilt.
use super::*;
use anyhow::{ensure,Result};
use crate::expedition_world_save::StateFields;
use serde::{Serialize,Deserialize};
#[derive(Serialize,Deserialize)]
struct BatchState {prototype:usize, capacity:u32, dynamic:bool, moving:bool, mats:Vec<glam::Mat4>, hidden:Vec<u64>,flags:f32,emissive:f32}
#[derive(Serialize,Deserialize)]
struct MeshMaterial {emissive:f32,own:bool,window:bool,finish:[f32;4],focal:f32,mirror:glam::Vec3}

impl Renderer {
    pub(crate) fn retained_instance_pool_count(&self)->usize {self.inst_batches.len()}
    pub(crate) fn export_world_physics(&self)->Result<StateFields> {
        let mut fields=crate::capture_world_fields!(self; sky_clock, sky_time_scale, day_night_cycle, day_phase_seconds, authored_clock_start, weather_offset, theatre_weather, weather_override, weather_original, user_sky_phase, precipitation, fog_density, overcast, storm, atmosphere_mood, twilight, weather_wind, weather_wind_world, weather_shelter, weather_seed, weather_act_id, weather_progress, pressure_fronts, interior_light, interior_haze, optic_mask, optic_eye, optic_blast, vision_boost, crew_ko, crew_ko_dir, concussion, observer_flash_source, muzzle_hint, player_snow_load, world_snow_load, wear, whitewash, fog_bank, atmo_enabled, atmo_haze, atmo_dust, atmo_origin, reticle, optic_reticle_fov);
        fields.put("pixel_particles",&*self.pixel_particle_field.borrow())?;
        fields.put("draw_state",&self.export_draw_state()?)?;
        Ok(fields)
    }
    pub(crate) fn import_world_physics(&mut self,mut fields:StateFields)->Result<()> {
        *self.pixel_particle_field.borrow_mut()=fields.take("pixel_particles")?;
        self.import_draw_state(fields.take("draw_state")?)?;
        crate::restore_world_fields!(self,fields; sky_clock, sky_time_scale, day_night_cycle, day_phase_seconds, authored_clock_start, weather_offset, theatre_weather, weather_override, weather_original, user_sky_phase, precipitation, fog_density, overcast, storm, atmosphere_mood, twilight, weather_wind, weather_wind_world, weather_shelter, weather_seed, weather_act_id, weather_progress, pressure_fronts, interior_light, interior_haze, optic_mask, optic_eye, optic_blast, vision_boost, crew_ko, crew_ko_dir, concussion, observer_flash_source, muzzle_hint, player_snow_load, world_snow_load, wear, whitewash, fog_bank, atmo_enabled, atmo_haze, atmo_dust, atmo_origin, reticle, optic_reticle_fov);
        ensure!(self.sky_clock.elapsed_s.is_finite()&&self.sky_clock.elapsed_s>=0.0&&self.weather_offset.is_finite(),"invalid retained sky/weather clock");
        self.refresh_celestial();self.reset_history();Ok(())
    }
}

impl Renderer {
    fn export_draw_state(&self)->Result<StateFields> {
        let mut fields=crate::capture_world_fields!(self; lights, lights_full, lights_offered, exhaust_puffs, smoke_puffs, breech_gas, heat_sources, pending_stamps, track_atlas);
        fields.put("batches",&self.inst_batches.iter().map(|b|BatchState {prototype:b.prototype,capacity:b.object_id_capacity,dynamic:b.raster_only_dynamic,moving:b.moving_rt_pool,mats:b.cpu_mats.clone(),hidden:b.hidden.words.clone(),flags:b.flags,emissive:b.emissive}).collect::<Vec<_>>())?;
        fields.put("materials",&self.meshes.iter().map(|m|MeshMaterial {emissive:m.emissive,own:m.emissive_own,window:m.emissive_win,finish:m.vehicle_finish,focal:m.periscope_focal_m,mirror:m.mirror_normal}).collect::<Vec<_>>())?;
        fields.put("fine_ids",&crate::expedition_world_save::ordered_map::Ref(&self.fine_vfx.particle_ids))?;
        fields.put("weapon_bursts",&self.weapon_particles.bursts)?;fields.put("weapon_segments",&self.weapon_particles.segments)?;
        fields.put("rut_pixels",&self.read_retained_ruts()?)?;
        Ok(fields)
    }
    fn import_draw_state(&mut self,mut fields:StateFields)->Result<()> {
        let batches:Vec<BatchState>=fields.take("batches")?;
        let materials:Vec<MeshMaterial>=fields.take("materials")?;
        ensure!(batches.len()==self.inst_batches.len()&&materials.len()==self.meshes.len(),
            "retained drawing source layout mismatch: pools saved {} rebuilt {}, meshes saved {} rebuilt {}",
            batches.len(),self.inst_batches.len(),materials.len(),self.meshes.len());
        let device=self.pose_device.clone();
        for (i,saved) in batches.into_iter().enumerate() {
            let old=&self.inst_batches[i];
            ensure!(saved.prototype==old.prototype&&(!(saved.dynamic||saved.moving)||saved.capacity==old.object_id_capacity)&&saved.dynamic==old.raster_only_dynamic&&saved.moving==old.moving_rt_pool,"retained instance source mismatch at {i}");
            ensure!(saved.mats.iter().all(|m|m.is_finite()),"nonfinite instance transform");
            if saved.dynamic {ensure!(self.set_dynamic_instanced_mats(i,&saved.mats),"dynamic pool restore rejected");}
            else if saved.moving {ensure!(self.set_moving_instanced_mats(i,&saved.mats),"moving pool restore rejected");}
            else {self.set_instanced_mats(&device,i,&saved.mats);}
            self.inst_batches[i].flags=saved.flags;self.inst_batches[i].emissive=saved.emissive;
            if !saved.dynamic&&!saved.moving {for j in 0..saved.mats.len() {let hidden=saved.hidden.get(j/64).is_some_and(|bits|bits&(1u64<<(j%64))!=0);
                ensure!(self.set_instance_visible(InstanceRef {batch:i,instance:j as u32},!hidden),"instance visibility restore rejected");}}
        }
        for (m,saved) in self.meshes.iter_mut().zip(materials) {m.emissive=saved.emissive;m.emissive_own=saved.own;m.emissive_win=saved.window;m.vehicle_finish=saved.finish;m.periscope_focal_m=saved.focal;m.mirror_normal=saved.mirror;}
        self.fine_vfx.particle_ids=fields.take("fine_ids")?;
        self.weapon_particles.bursts=fields.take("weapon_bursts")?;self.weapon_particles.segments=fields.take("weapon_segments")?;
        let pixels:Vec<u8>=fields.take("rut_pixels")?;
        crate::restore_world_fields!(self,fields; lights, lights_full, lights_offered, exhaust_puffs, smoke_puffs, breech_gas, heat_sources, pending_stamps, track_atlas);
        let rows=self.track_atlas.saved_rows();
        ensure!(rows<=track_ruts::ATLAS_RES&&pixels.len()==(rows*track_ruts::ATLAS_RES) as usize,"invalid retained track atlas");
        if rows>0 {self.instance_queue.write_texture(wgpu::TexelCopyTextureInfo {texture:self.track_view.texture(),mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},&pixels,
            wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(track_ruts::ATLAS_RES),rows_per_image:Some(rows)},wgpu::Extent3d {width:track_ruts::ATLAS_RES,height:rows,depth_or_array_layers:1});}
        self.instance_queue.write_texture(wgpu::TexelCopyTextureInfo {texture:&self.track_lookup,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},bytemuck::cast_slice(&self.track_atlas.saved_lookup()),
            wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(track_ruts::LOOKUP_RES*4),rows_per_image:Some(track_ruts::LOOKUP_RES)},wgpu::Extent3d {width:track_ruts::LOOKUP_RES,height:track_ruts::LOOKUP_RES,depth_or_array_layers:1});
        self.rt_batches_dirty=true;self.rt_static_dirty=true;self.rt_surface_dirty=true;Ok(())
    }
    /// Save only the used rows of the bounded R8 material history. Pending unrendered stamps
    /// remain a separate queue, so a save never consumes or applies them twice.
    fn read_retained_ruts(&self)->Result<Vec<u8>> {
        let rows=self.track_atlas.saved_rows();if rows==0 {return Ok(Vec::new());}
        ensure!(rows<=track_ruts::ATLAS_RES,"invalid live rut atlas size");
        let width=track_ruts::ATLAS_RES;let size=u64::from(width)*u64::from(rows);
        let buffer=self.pose_device.create_buffer(&wgpu::BufferDescriptor {label:Some("save retained track history"),size,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
        let mut encoder=self.pose_device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo {texture:self.track_view.texture(),mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},
            wgpu::TexelCopyBufferInfo {buffer:&buffer,layout:wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(width),rows_per_image:Some(rows)}},wgpu::Extent3d {width,height:rows,depth_or_array_layers:1});
        self.instance_queue.submit([encoder.finish()]);let slice=buffer.slice(..);let (tx,rx)=std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read,move |result|{let _=tx.send(result);});
        self.pose_device.poll(wgpu::PollType::wait_indefinitely())?;rx.recv()??;
        let result=slice.get_mapped_range().to_vec();buffer.unmap();Ok(result)
    }
}

/// Validate both the existing prefix and every future reference before touching GPU state.
fn validate_clone_journal(current:&[(usize,usize)], saved:&[(usize,usize)], mesh_count:usize)->Result<()> {
    ensure!(saved.starts_with(current),"retained source allocation prefix differs");
    ensure!(saved.len()<=1_000_000,"retained source allocation count exceeds limit");
    let base=current.first().map_or(mesh_count,|(_,target)|*target);
    ensure!(base.checked_add(current.len())==Some(mesh_count),"live source clone allocation is not contiguous");
    for (offset,&(source,target)) in saved.iter().enumerate() {
        ensure!(base.checked_add(offset)==Some(target)&&source<target,
            "invalid source clone {offset}: source {source}, target {target}");
    }
    Ok(())
}
impl Renderer {
    pub(crate) fn export_actor_source_journal(&self)->Vec<(usize,usize)> {self.actor_mesh_clones.clone()}
    pub(crate) fn restore_actor_source_journal(&mut self,saved:&[(usize,usize)])->Result<()> {
        validate_clone_journal(&self.actor_mesh_clones,saved,self.meshes.len())?;
        let pending=&saved[self.actor_mesh_clones.len()..];
        for &(source,target) in pending {
            ensure!(self.clone_actor_mesh(source)==target,"retained source clone allocated another identity");
        }
        Ok(())
    }
}
#[cfg(test)]
mod source_journal_tests {
    use super::*;
    #[test]
    fn mission_flare_and_later_vehicle_allocations_preserve_interleaved_identity() {
        let initial=[(2,100),(3,101),(4,102)];
        let saved=[(2,100),(3,101),(4,102),(8,103),(8,104),(2,105),(3,106),(4,107)];
        assert!(validate_clone_journal(&initial,&saved,103).is_ok());
        assert!(validate_clone_journal(&saved,&saved,108).is_ok());
        let incorrect=[(2,100),(3,101),(4,102),(2,103)];
        assert!(validate_clone_journal(&incorrect,&saved,104).is_err());
        assert!(validate_clone_journal(&[],&[(4,100),(101,101)],100).is_err());
        assert!(validate_clone_journal(&[],&[(4,101)],100).is_err());
    }
}
