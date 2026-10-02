//! Explicit capture-only CPU diagnosis. No render pass, visibility replacement or scene mutation.
use super::Renderer;
use crate::scene_visibility::{Dielectric, MaterialClass, TransmissionStatus, VisibilityHit, VisibilityRay};
use glam::{Mat4, Vec3};
use serde_json::{json, Value};

fn next_up(t: f32) -> f32 {
    if t == 0.0 { f32::from_bits(1) }
    else if t > 0.0 { f32::from_bits(t.to_bits() + 1) }
    else { f32::from_bits(t.to_bits() - 1) }
}

fn box_interval(o: Vec3, d: Vec3, min: Vec3, max: Vec3, limit: f32) -> Option<[f32; 2]> {
    let (mut lo, mut hi) = (0.0_f32, limit);
    for k in 0..3 {
        if d[k] == 0.0 {
            if o[k] < min[k] || o[k] > max[k] { return None; }
        } else {
            let a = (min[k] - o[k]) / d[k];
            let b = (max[k] - o[k]) / d[k];
            lo = lo.max(a.min(b)); hi = hi.min(a.max(b));
        }
    }
    (hi > lo).then_some([lo, hi])
}

fn merge_intervals(mut spans: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f32; 2]> = Vec::new();
    for s in spans {
        if let Some(last) = merged.last_mut() {
            if s[0] <= last[1] + 1e-5 { last[1] = last[1].max(s[1]); continue; }
        }
        merged.push(s);
    }
    merged
}

fn hash(p: glam::IVec3) -> f32 {
    let mut h = (p.x as u32).wrapping_mul(1597334677)
        ^ (p.y as u32).wrapping_mul(3812015801) ^ (p.z as u32).wrapping_mul(2798796415);
    h = (h ^ (h >> 16)).wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(3266489917);
    ((h ^ (h >> 16)) >> 8) as f32 / 16777216.0
}
fn mix(a: f32, b: f32, u: f32) -> f32 { a + (b - a) * u }
fn noise(p: Vec3) -> f32 {
    let cell = p.floor().as_ivec3(); let f = p - p.floor(); let u = f*f*(Vec3::splat(3.0)-2.0*f);
    let h = |x, y, z| hash(cell + glam::IVec3::new(x,y,z));
    mix(mix(mix(h(0,0,0),h(1,0,0),u.x),mix(h(0,1,0),h(1,1,0),u.x),u.y),
        mix(mix(h(0,0,1),h(1,0,1),u.x),mix(h(0,1,1),h(1,1,1),u.x),u.y),u.z)
}
fn weather_moment(o: Vec3, d: Vec3, limit: f32, layer: [f32; 4], sigma: f32,
    snow: f32, offset: Vec3, steps: u32) -> [f32; 2] {
    let (mut begin, mut end) = (0.0_f32, limit.max(0.0));
    if d.y.abs() < 1e-6 {
        if o.y < layer[0] || o.y > layer[1] { return [0.0;2]; }
    } else {
        let a = (layer[0]-o.y)/d.y; let b = (layer[1]-o.y)/d.y;
        begin = a.min(b).max(0.0); end = a.max(b).min(end);
    }
    if end <= begin || sigma <= 0.0 { return [0.0;2]; }
    if snow <= 0.0 { let tau=sigma*(end-begin); return [tau,tau*(begin+end)*0.5]; }
    let count = steps.clamp(8,48); let mut out=[0.0;2];
    for i in 0..count {
        let a=i as f32/count as f32; let b=(i+1) as f32/count as f32;
        let lo=begin+(end-begin)*a*a; let hi=begin+(end-begin)*b*b; let mid=(lo+hi)*0.5;
        let p=o+d*mid-offset;
        let broad=noise(p/Vec3::new(110.0,42.0,110.0));
        let eddies=noise(p/Vec3::new(27.0,13.0,27.0)+Vec3::new(19.7,3.1,-6.3));
        let x=((broad*0.72+eddies*0.28-0.18)/(0.82-0.18)).clamp(0.0,1.0);
        let banks=0.22+2.15*x*x*(3.0-2.0*x);
        let tau=sigma*mix(1.0,banks,snow.clamp(0.0,1.0))*(hi-lo);
        out[0]+=tau; out[1]+=tau*mid;
    }
    out
}

impl Renderer {
    fn review_hit(&self, h: VisibilityHit, ray: VisibilityRay) -> Value {
        let mut out=json!({"distance_m":h.t,"world_m":(ray.origin+ray.direction*h.t).to_array(),
            "instance":h.instance_id,"geometry_word_offset":h.geometry.0,"primitive":h.primitive_id,
            "material":format!("{:?}",h.material),"material_id":h.material_id,"front_face":h.front_face,
            "normal":h.normal.to_array(),"barycentrics":h.barycentrics.to_array(),"uv":h.uv.to_array()});
        if let Some(m)=self.meshes.get(h.instance_id as usize) {
            out["source_mesh"]=json!({"index":h.instance_id,"indices":m.index_count,
                "glass":m.glass,"periscope_kind":m.periscope_kind,"raster_only":m.raster_only,
                "model":m.model.to_cols_array_2d(),"bounds_min":m.world_min.to_array(),"bounds_max":m.world_max.to_array(),
                "optic_port":self.raster_optics.as_ref().and_then(|o|o.specs.iter().position(|s|s.mesh==h.instance_id as usize))});
        }
        out
    }

    fn review_transmission(&self, ray: VisibilityRay) -> anyhow::Result<Value> {
        let glass=Dielectric {material_id:1,ior:1.52,absorption_per_m:Vec3::new(0.080,0.035,0.020)};
        let result=self.scene_visibility.transmission(ray,glass)?;
        let mut out=json!({"origin":ray.origin.to_array(),"direction":ray.direction.to_array(),
            "status":format!("{:?}",result.status),"interfaces":result.interfaces,
            "transmission":result.transmission.map(|v|v.to_array()),"glass_distance_m":result.straight_inside_distance_m});
        // Replay exact nearest-hit order only to explain the production CPU status, never to
        // replace that result. Record successful pairs too when a later pair is unresolved.
        let mut segment=ray; let mut hits=Vec::new(); let mut pairs=0;
        let reason=loop {
            let Some(entry)=self.scene_visibility.trace(segment)? else { break "clear_to_environment"; };
            hits.push(self.review_hit(entry,ray));
            if matches!(entry.material,MaterialClass::Opaque|MaterialClass::AlphaTest) {break "opaque_source_occluder";}
            if entry.material!=MaterialClass::Dielectric {break "unresolved_source_material";}
            if entry.material_id!=glass.material_id {break "unmapped_dielectric_material";}
            if !entry.front_face {break "first_glass_hit_is_back_face";}
            if pairs>=16 {break "dielectric_interface_budget";}
            segment.t_min=next_up(entry.t);
            if segment.t_min>segment.t_max {break "entry_at_ray_end_without_exit";}
            let Some(exit)=self.scene_visibility.trace(segment)? else {break "entry_without_source_exit";};
            hits.push(self.review_hit(exit,ray));
            if matches!(exit.material,MaterialClass::Opaque|MaterialClass::AlphaTest) {break "opaque_source_occluder_inside_glass";}
            if exit.material!=MaterialClass::Dielectric {break "exit_material_unresolved";}
            if exit.instance_id!=entry.instance_id {break "exit_is_different_source_instance";}
            if exit.material_id!=entry.material_id {break "exit_material_mismatch";}
            if exit.front_face {break "exit_is_another_front_face";}
            out["last_entry_exit_normal_dot"]=json!(entry.normal.dot(exit.normal));
            if entry.normal.dot(exit.normal)>-0.9999 {break "nonparallel_dielectric_entry_exit";}
            pairs+=1; segment.t_min=next_up(exit.t);
            if segment.t_min>segment.t_max {break "clear_after_closed_pane";}
        };
        out["reason"]=json!(reason);
        // Keeping every unresolved sequence is essential; clear/blocked rays need only their first
        // source contact to keep a 32-direction per-pixel report reviewable.
        if result.status!=TransmissionStatus::Unresolved { hits.truncate(1); }
        out["ordered_hits"]=json!(hits);
        Ok(out)
    }

    fn review_lighting_at(&self, origin: Vec3) -> anyhow::Result<Vec<Value>> {
        let mut queries=Vec::new();
        for (i,d) in super::super::sky_visibility::directions().iter().enumerate() {
            let direction=d.truncate();
            let straight=self.review_transmission(VisibilityRay{origin,direction,t_min:0.0001,t_max:1e8})?;
            let path=self.scene_visibility.environment_path(origin,direction,
                Dielectric{material_id:1,ior:1.52,absorption_per_m:Vec3::new(0.080,0.035,0.020)})?;
            queries.push(json!({"source":format!("environment_{i}"),"origin":origin.to_array(),"direction":direction.to_array(),
                "status":format!("{:?}",path.status),"reason":path.reason,
                "outgoing_direction":path.direction.to_array(),"outgoing_origin":path.origin.to_array(),
                "throughput":path.throughput.map(|v|v.to_array()),"glass_path_m":path.glass_path_m,
                "environment_steps":path.steps.iter().map(|s|json!({"instance":s.instance,"material":format!("{:?}",s.material),
                    "event":s.event,"point":s.point.to_array(),"incoming":s.incoming.to_array(),"outgoing":s.outgoing.to_array()})).collect::<Vec<_>>(),
                "straight_connection_reference":straight}));
        }
        let mut sun=self.review_transmission(VisibilityRay{origin,direction:self.sun_dir.normalize(),t_min:0.0001,t_max:1e8})?;
        sun["source"]=json!("sun"); queries.push(sun);
        for (index,source) in self.lights.iter().enumerate() {
            if source.radiant_intensity<=0.0 || source.color==Vec3::ZERO {continue;}
            let delta=source.pos-origin;let range=delta.length();if range<=0.0001 {continue;}
            let direction=delta/range;let ray=VisibilityRay{origin,direction,t_min:0.0001,t_max:(range-0.0001).max(0.0001)};
            let mut q=self.review_transmission(ray)?;
            let emitter=self.cabin_shadows.emitter_for_source(source.pos);
            let result=self.scene_visibility.transmission_to_emitter(ray,
                Dielectric{material_id:1,ior:1.52,absorption_per_m:Vec3::new(0.080,0.035,0.020)},emitter)?;
            q["source"]=json!(format!("point_{index}"));q["source_position"]=json!(source.pos.to_array());
            q["emitting_instance"]=json!(emitter);q["status"]=json!(format!("{:?}",result.status));
            q["radiant_intensity"]=json!(source.radiant_intensity);queries.push(q);
        }
        Ok(queries)
    }

    fn review_cabin_spans(&self, origin: Vec3, direction: Vec3, limit: f32) -> Vec<[f32;2]> {
        let Some(medium)=self.cabin_medium.as_ref() else {return Vec::new();};
        let size=medium.texture_size(); let data=medium.texture_data(); let u=&self.cabin_medium_uniform;
        let mut spans=Vec::new();
        // Exhaustive column intersection is deliberately slow but bounded CPU-only reference for
        // shader DDA. It uses exactly the uploaded source-air texels, not a hull-shaped proxy.
        for domain in 0..3 {
            let grid=u.grid_min_step[domain]; if grid[3]<=0.0 {continue;}
            let model=Mat4::from_cols_array_2d(&u.world_to_local[domain]);
            let o=model.transform_point3(origin); let d=model.transform_vector3(direction);
            let dims=u.grid_size[domain];
            for z in 0..dims[1] {for x in 0..dims[0] {
                let v=data[(dims[2]*size[0]*size[1]+z*size[0]+x) as usize];
                if v[1]<v[0] {continue;}
                let min=Vec3::new(grid[0]+x as f32*grid[2],v[0],grid[1]+z as f32*grid[2]);
                let max=Vec3::new(min.x+grid[2],v[1],min.z+grid[2]);
                if let Some(span)=box_interval(o,d,min,max,limit) {spans.push(span);}
            }}
        }
        merge_intervals(spans)
    }

    fn review_fog_moment(&self, origin: Vec3, direction: Vec3, limit: f32) -> ([f32;2],Vec<[f32;2]>) {
        let medium=super::super::world_medium_profile(self.precipitation,self.storm);
        let spans=self.review_cabin_spans(origin,direction,limit);
        let (mut cursor,mut result)=(0.0,[0.0;2]);
        for span in spans.iter().copied().chain(std::iter::once([limit,limit])) {
            if span[0]>cursor {
                let m=weather_moment(origin+direction*cursor,direction,span[0]-cursor,medium.layer,
                    self.fog_density.max(0.0),medium.snow_fraction,self.weather_offset,self.transport_quality.smoke_steps()[0] as u32);
                result[0]+=m[0]; result[1]+=m[1]+m[0]*cursor;
            }
            cursor=span[1]+1e-5;
        }
        (result,spans)
    }

    fn review_main_pixels(&self) -> anyhow::Result<Vec<Value>> {
        let camera=self.camera;let forward=camera.forward();let right=camera.right();let up=right.cross(forward);
        let tangent=(camera.fov_y*0.5).tan();
        // A general sparse grid plus the centers of the visible magenta patches in the saved
        // 640x360 v9 first frame. Coordinates are normalized so native resolution is disclosed,
        // not silently substituted. These are diagnostic receiver samples, not edited rays.
        let mut samples=Vec::new();
        for y in 0..8 {for x in 0..12 {samples.push(((x as f32+0.5)/12.0,(y as f32+0.5)/8.0,"grid"));}}
        for [x,y] in [[179.0,307.0],[258.0,270.0],[350.0,268.0],[387.0,301.0],[494.0,282.0],[563.0,284.0],[255.0,164.0]] {
            samples.push((x/640.0,y/360.0,"v9_magenta_reference"));
        }
        if let Ok(path)=std::env::var("KESSEL_SOURCE_VISIBILITY_IMAGE") {
            let image=image::open(path)?.to_rgb8();let mut cells=std::collections::BTreeSet::new();
            for (x,y,pixel) in image.enumerate_pixels() {
                if pixel[0]<=220 || pixel[1]>=70 || pixel[2]<=220 {continue;}
                let ix=(x as u64*self.iw as u64/image.width() as u64) as u32;
                let iy=(y as u64*self.ih as u64/image.height() as u64) as u32;
                // Inspect every distinct native pixel represented by the diagnostic mask.
                // Picking one sample per coarse display bucket can miss the diagnostic core
                // while retaining only neighboring pixels colored by bloom/upscaling.
                if cells.insert((ix,iy)) {
                    samples.push(((ix as f32+0.5)/self.iw as f32,(iy as f32+0.5)/self.ih as f32,"actual_diagnostic_pixel"));
                }
            }
        }
        let mut pixels=Vec::new();
        for (x,y,label) in samples {
            let direction=(forward+right*((2.0*x-1.0)*tangent*camera.aspect)+up*((1.0-2.0*y)*tangent)).normalize();
            let mut ray=VisibilityRay{origin:camera.pos,direction,t_min:camera.znear/direction.dot(forward),t_max:camera.zfar};
            let mut excluded=Vec::new();
            let primary=loop {
                let Some(hit)=self.scene_visibility.trace(ray)? else {break None;};
                let omit=self.meshes.get(hit.instance_id as usize).is_some_and(|m|
                    m.glass||m.periscope_kind!=0||self.raster_optic_panel(hit.instance_id as usize));
                if !omit {break Some(hit);}
                excluded.push(self.review_hit(hit,ray));ray.t_min=next_up(hit.t);
                anyhow::ensure!(excluded.len()<1024,"main review primary optical hit budget");
            };
            let limit=primary.map_or(camera.zfar,|h|h.t);
            let (moment,spans)=self.review_fog_moment(camera.pos,direction,limit);
            let mut pixel=json!({"sample":label,"uv":[x,y],"native_pixel":[x*self.iw as f32,y*self.ih as f32],
                "direction":direction.to_array(),"opaque_distance_m":limit,
                "primary_hit":primary.map(|h|self.review_hit(h,ray)),"excluded_optical_depth_hits":excluded,
                "world_fog_optical_moment":moment,"source_air_intervals_m":spans});
            for (name,point) in [
                ("fog",(moment[0]>0.0).then(||camera.pos+direction*(moment[1]/moment[0].max(1e-8)))),
                ("surface",primary.map(|h|camera.pos+direction*h.t+h.normal*0.0001))] {
                if let Some(point)=point {
                    let mut queries=self.review_lighting_at(point)?;
                    if name=="surface" {if let Some(h)=primary {for q in &mut queries {
                        let d=q["direction"].as_array().unwrap();let direction=Vec3::new(d[0].as_f64().unwrap() as f32,d[1].as_f64().unwrap() as f32,d[2].as_f64().unwrap() as f32);
                        q["positive_geometric_hemisphere"]=json!(h.normal.dot(direction)>0.0);
                    }}}
                    pixel[format!("{name}_receiver_m")]=json!(point.to_array());
                    pixel[format!("{name}_queries")]=json!(queries);
                }
            }
            pixels.push(pixel);
        }
        Ok(pixels)
    }

    /// Refresh live source transforms and the same port cameras/uniforms used by the renderer,
    /// then execute only CPU rays. No draw, sensor convergence or image readback occurs.
    pub(crate) fn debug_review_source_visibility(&mut self, device:&wgpu::Device,queue:&wgpu::Queue,path:&str) -> anyhow::Result<Value> {
        anyhow::ensure!(!std::path::Path::new(path).exists(),"diagnostic output exists: {path}");
        self.upload_uniforms(device,queue);
        let optics=self.raster_optics.as_ref().ok_or_else(||anyhow::anyhow!("raster optics inactive"))?;
        let views=optics.prepared.borrow().clone(); let mut ports=Vec::new();
        let mut counts=std::collections::BTreeMap::<String,usize>::new();
        for view in views {
            let camera=view.camera; let right=camera.right(); let up=right.cross(camera.forward());
            let tangent=(camera.fov_y*0.5).tan(); let mut pixels=Vec::new();
            for y in [-0.65,0.0,0.65] {for x in [-0.65,0.0,0.65] {
                let direction=(camera.forward()+right*(x*tangent*camera.aspect)+up*(y*tangent)).normalize();
                let mut ray=VisibilityRay{origin:camera.pos,direction,t_min:camera.znear/direction.dot(camera.forward()),t_max:camera.zfar};
                let mut skipped=Vec::new();
                let primary=loop {
                    let hit=self.scene_visibility.trace(ray)?;
                    let Some(h)=hit else {break None;};
                    let omitted=self.meshes.get(h.instance_id as usize).is_some_and(|m|
                        m.glass||m.periscope_kind!=0||m.raster_only||self.raster_optic_panel(h.instance_id as usize));
                    if !omitted {break Some(h);}
                    skipped.push(self.review_hit(h,ray)); ray.t_min=next_up(h.t);
                    anyhow::ensure!(skipped.len()<1024,"primary ray skipped over 1024 optical surfaces");
                };
                let limit=primary.map(|h|h.t).unwrap_or(camera.zfar);
                let (moment,spans)=self.review_fog_moment(camera.pos,direction,limit);
                let mut pixel=json!({"ndc":[x,y],"direction":direction.to_array(),"opaque_distance_m":limit,
                    "primary_hit":primary.map(|h|self.review_hit(h,ray)),"excluded_optical_depth_hits":skipped,
                    "world_fog_optical_moment":moment,"source_air_intervals_m":spans});
                for (label,point) in [
                    ("fog",(moment[0]>0.0).then(||camera.pos+direction*(moment[1]/moment[0].max(1e-8)))),
                    ("surface",primary.map(|h|camera.pos+direction*h.t+h.normal*if h.front_face{0.0001}else{-0.0001}))] {
                    if let Some(point)=point {
                        let queries=self.review_lighting_at(point)?;
                        for q in &queries {let key=format!("{}:{}",q["status"].as_str().unwrap(),q["reason"].as_str().unwrap());*counts.entry(key).or_default()+=1;}
                        pixel[format!("{label}_receiver_m")]=json!(point.to_array());
                        pixel[format!("{label}_queries")]=json!(queries);
                    }
                }
                pixels.push(pixel);
            }}
            ports.push(json!({"port":view.port,"source_mesh":optics.specs[view.port].mesh,"active_native_extent":view.extent,
                "camera_m":camera.pos.to_array(),"forward":camera.forward().to_array(),"source_eye_m":view.source_eye.to_array(),
                "fov_y_deg":camera.fov_y.to_degrees(),"aspect":camera.aspect,"pixels":pixels}));
        }
        let main_pixels=self.review_main_pixels()?;
        let report=json!({"schema":"panzer.source-visibility-cpu-review.v2","frame":self.frame,"time_s":self.time,
            "native_extent":[self.iw,self.ih],"viewer_m":self.camera.pos.to_array(),"geometry_count":self.scene_visibility.geometry_count(),
            "viewer_forward":self.camera.forward().to_array(),"viewer_fov_y":self.camera.fov_y,"main_pixels":main_pixels,
            "source_triangles":self.scene_visibility.unique_triangle_count(),"query_counts":counts,"ports":ports,
            "limits":"CPU exact source visibility and source-column weather reference; no GPU draw or observed GPU depth. Opaque depth uses CPU geometric intersection and source alpha, omitting optical/view-local meshes as the optical raster does. Fog samples reproduce uploaded field/steps with ordinary CPU f32 arithmetic; GPU rounding and raster coverage can differ. Surface receiver uses geometric face-forward normal, whereas shading can use an interpolated normal. 3x3 rays per prepared port are diagnostic samples, not exhaustive pixel coverage."});
        std::fs::write(path,serde_json::to_vec_pretty(&report)?)?;
        eprintln!("CPU source visibility review: {path}; {}",report["query_counts"]);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_cupola_rim_keeps_the_nearby_glass_side_interface() {
        use crate::scene_visibility::{SceneVisibility, VisibilityInstance};
        let (_, _, parts, _, _, _) = crate::game::tiger_periscopes::load_turret().unwrap();
        let model = Mat4::from_cols_array(&[
            0.4225206971,0.0,0.9063532948,0.0, 0.0,1.0,0.0,0.0,
            -0.9063532948,0.0,0.4225206971,0.0, 60.0,4.2727451324,-279.9809875488,1.0]);
        let mut scene=SceneVisibility::new();let mut instances=Vec::new();
        for (id,part) in parts.iter().enumerate() {
            let geometry=scene.register_mesh(&part.mesh).unwrap();
            let material=match part.kind {1=>MaterialClass::Dielectric,2=>MaterialClass::Mirror,3=>MaterialClass::ThinLens,_=>MaterialClass::Opaque};
            let center=model.transform_point3(part.mesh.center());
            instances.push(VisibilityInstance{id:id as u32,geometry,model,material,material_id:1,
                bend_world:glam::Vec2::ZERO,lens_center_focal:[center.x,center.y,center.z,part.focal_m]});
        }
        scene.update_instances(&instances).unwrap();
        let path=scene.environment_path(Vec3::new(60.0175743103,6.6406259537,-278.7300720215),
            Vec3::new(0.6899647117,0.2291666865,-0.6866085529),
            Dielectric{material_id:1,ior:1.52,absorption_per_m:Vec3::new(0.080,0.035,0.020)}).unwrap();
        assert_ne!(path.status,TransmissionStatus::Unresolved,"actual source pane: {path:?}");
        assert!(path.steps.len()>=2,"{path:?}");
        assert_eq!(path.steps[0].event,"glass_entry");
        assert_eq!(path.steps[1].event,"total_internal_reflection","{path:?}");
        assert!(path.steps[1].point.distance(path.steps[0].point)<0.0001,"{path:?}");
    }
    #[test]
    fn source_column_intersections_preserve_air_gaps_and_rotated_ray_parameter() {
        let world=Mat4::from_rotation_y(0.63)*Mat4::from_translation(Vec3::new(4.0,2.0,-3.0));
        let inv=world.inverse(); let o=world.transform_point3(Vec3::new(-1.0,0.5,0.5));
        let d=world.transform_vector3(Vec3::X);
        let span=box_interval(inv.transform_point3(o),inv.transform_vector3(d),Vec3::ZERO,Vec3::ONE,8.0).unwrap();
        assert!((span[0]-1.0).abs()<1e-5&&(span[1]-2.0).abs()<1e-5);
        assert_eq!(merge_intervals(vec![[3.0,4.0],[1.0,1.5],[1.5,2.0]]),vec![[1.0,2.0],[3.0,4.0]]);
    }
    #[test]
    fn homogeneous_weather_centroid_retains_layer_and_distance() {
        assert_eq!(weather_moment(Vec3::ZERO,Vec3::X,8.0,[-1.0,1.0,0.0,0.0],0.1,0.0,Vec3::ZERO,8),[0.8,3.2]);
        assert_eq!(weather_moment(Vec3::Y*2.0,Vec3::X,8.0,[-1.0,1.0,0.0,0.0],0.1,0.0,Vec3::ZERO,8),[0.0,0.0]);
    }
}
