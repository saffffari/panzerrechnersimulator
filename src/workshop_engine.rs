//! Source engine-bay hinges. The catalog records the visible clearance derivatives and
//! their original source. Scene/collision registration uses these same moving triangles.
//! An open cover is access, never an engine-health repair or an inventory refill.
use super::*;
use serde::{Serialize,Deserialize};
#[path="workshop_sweep.rs"]
pub mod sweep;
#[path="workshop_engine_review.rs"]
pub mod review;

pub const FIRST_ENGINE_FRAME:usize=state::FIRST_ROTOR_FRAME+8;
const IDS:[&str;4]=["lid","left_lock","right_lock","rear_latch"];
const CATALOG:&str="catalog/tiger_engine_service.v1.json";

#[derive(Clone,Deserialize)]
struct Assembly {
    id:String,parent_frame:String,parent_assembly:Option<String>,files:Vec<String>,
    hinge_hull_m:[f32;3],axis_hull:[f32;3],closed_angle_deg:f32,open_angle_deg:f32,
    source_contact_hull_m:[f32;3],
}
#[derive(Deserialize)] struct Removed {hull:Vec<String>,turret:Vec<String>}
#[derive(Deserialize)] struct Fixed {file:String,parent_frame:String}
#[derive(Clone,Deserialize)] struct ServicePart {
    source_root:String,file:String,source_contact_turret_m:[f32;3],withdrawal_axis_turret:[f32;3],
    withdrawal_path_turret_m:Vec<[f32;3]>,
}
#[derive(Deserialize)] struct Catalog {
    schema:String,asset_root:String,service_enabled:bool,source_hull:String,source_turret:String,
    removed_roots:Removed,fixed_replacements:Vec<Fixed>,assemblies:Vec<Assembly>,
    service_parts:Vec<ServicePart>,
}
#[derive(Clone,Copy,Debug,PartialEq,Serialize,Deserialize)]
pub struct Grip {pub assembly:usize,pub opening:bool}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
pub struct EngineAccessState {pub fraction:[f32;4],pub grip:Option<Grip>}
impl EngineAccessState {
    pub fn validate(&self)->Result<()> {
        ensure!(self.fraction.iter().all(|v|v.is_finite()&&(0.0..=1.0).contains(v)),"invalid engine-cover articulation");
        ensure!(self.fraction[0]==0.0||self.fraction[1..].iter().all(|v|*v==1.0),"open cover has an engaged latch");
        ensure!(self.grip.is_none_or(|g|g.assembly<4),"invalid engine-cover grip");Ok(())
    }
    fn begin_grip(&mut self,index:usize) {
        let fraction=self.fraction[index];
        let opening=if fraction==0.0 {true}else if fraction==1.0 {false}
            else {self.grip.filter(|g|g.assembly==index).is_none_or(|g|!g.opening)};
        self.grip=Some(Grip{assembly:index,opening});
    }
}
struct Moving {def:Assembly,slots:Vec<usize>,meshes:Vec<CpuMesh>}
pub(super) struct EngineAccess {
    pub state:EngineAccessState,pub service_enabled:bool,
    moving:Vec<Moving>,fixed:Vec<(usize,usize)>,
    service_parts:[ServicePart;2],
    /// Diagnostic evidence only; never consulted for collision or serialized as authority.
    last_sweep:std::cell::RefCell<Option<serde_json::Value>>,
}
impl EngineAccess {
    pub(super) fn install(scene:&mut Vec<CpuMesh>,slots:&Slots)->Result<(Self,Vec<Bank>)> {
        let text=crate::resource::read(CATALOG)?;let catalog:Catalog=serde_json::from_slice(&text)?;
        ensure!(catalog.schema=="panzer.tiger-engine-service.v1"&&catalog.assemblies.len()==4,"engine service catalog schema");
        ensure!(catalog.asset_root=="assets/tanks/tiger/workshop_engine/","unexpected engine derivative root");
        for (source,roots,slot) in [(&catalog.source_hull,&catalog.removed_roots.hull,slots.tiger[0]),
            (&catalog.source_turret,&catalog.removed_roots.turret,slots.tiger[1])] {
            if roots.is_empty(){continue;}
            let names=roots.iter().map(String::as_str).collect::<Vec<_>>();
            let mut original=crate::gltf_load::load_glb_partitioned_roots_with_material_override(source,&names,Some(crate::tank::tiger_paint_finish))?;
            original.remove(0);detach_exact(&mut scene[slot],&original)?;
        }
        let load=|file:&str|->Result<CpuMesh> {
            ensure!(!file.contains('/')&&!file.contains('\\')&&file.ends_with(".glb"),"engine derivative must be a catalog basename");
            Ok(crate::gltf_load::load_glb_partitioned_roots_with_material_override(
                &format!("{}{file}",catalog.asset_root),&[],Some(crate::tank::tiger_paint_finish))?.remove(0))
        };
        let mut fixed=Vec::new();
        for entry in &catalog.fixed_replacements {
            let frame=match entry.parent_frame.as_str(){"hull"=>0,"turret"=>1,_=>anyhow::bail!("unknown fixed engine frame")};
            let slot=scene.len();scene.push(load(&entry.file)?);fixed.push((frame,slot));
        }
        let mut moving=Vec::new();
        for (i,id) in IDS.into_iter().enumerate() {
            let def=catalog.assemblies.iter().find(|a|a.id==id).context("missing engine assembly")?.clone();
            ensure!(def.parent_frame=="hull"&&def.closed_angle_deg==0.0
                &&def.open_angle_deg.is_finite()&&def.open_angle_deg.abs()<=120.0,"invalid engine hinge frame/range");
            ensure!(if i==0 {def.parent_assembly.is_none()}else{def.parent_assembly.as_deref()==Some("lid")},"unexpected hinge hierarchy");
            ensure!(Vec3::from(def.hinge_hull_m).is_finite()&&Vec3::from(def.source_contact_hull_m).is_finite()
                &&(Vec3::from(def.axis_hull).length()-1.0).abs()<1e-4,"invalid source hinge");
            let mut assembly_slots=Vec::new();let mut meshes=Vec::new();
            for file in &def.files {let mesh=load(file)?;assembly_slots.push(scene.len());scene.push(mesh.clone());meshes.push(mesh);}
            ensure!(!meshes.is_empty(),"empty engine assembly");moving.push(Moving{def,slots:assembly_slots,meshes});
        }
        let service_parts=["Turret_0132","Turret_0153"].map(|root|catalog.service_parts.iter()
            .find(|p|p.source_root==root).cloned().with_context(||format!("missing source service part {root}")));
        let [plate,retainer]=service_parts;let service_parts=[plate?,retainer?];
        let mut banks=Vec::new();
        for part in &service_parts {
            ensure!(catalog.removed_roots.turret.contains(&part.source_root),"service part still in fixed turret");
            ensure!(Vec3::from(part.source_contact_turret_m).is_finite()
                &&(Vec3::from(part.withdrawal_axis_turret).length()-1.0).abs()<1e-4,"invalid carrier contact/path");
            banks.push(make_bank(scene,vec![load(&part.file)?],None,Mat4::IDENTITY));
        }
        Ok((Self{state:EngineAccessState::default(),service_enabled:catalog.service_enabled,moving,fixed,service_parts,
            last_sweep:std::cell::RefCell::new(None)},banks))
    }
    fn hinge_delta(&self,index:usize,fraction:f32)->Mat4 {
        let d=&self.moving[index].def;let hinge=Vec3::from(d.hinge_hull_m);
        Mat4::from_translation(hinge)*Mat4::from_axis_angle(d.axis_hull.into(),d.open_angle_deg.to_radians()*fraction)
            *Mat4::from_translation(-hinge)
    }
    fn local_models(&self,fraction:[f32;4])->[Mat4;4] {
        let lid=self.hinge_delta(0,fraction[0]);[lid,lid*self.hinge_delta(1,fraction[1]),
            lid*self.hinge_delta(2,fraction[2]),lid*self.hinge_delta(3,fraction[3])]
    }
    pub(super) fn models(&self,hull:Mat4)->[Mat4;4] {self.local_models(self.state.fraction).map(|m|hull*m)}
    pub(super) fn append_models(&self,models:&mut [Mat4;surface_fire::FRAMES]) {
        let posed=self.models(models[0]);models[FIRST_ENGINE_FRAME..FIRST_ENGINE_FRAME+4].copy_from_slice(&posed);
    }
    pub(super) fn surfaces<'a>(&self,scene:&'a [CpuMesh])->Vec<(usize,&'a CpuMesh)> {
        self.moving.iter().enumerate().flat_map(|(i,a)|a.slots.iter().map(move|&s|(FIRST_ENGINE_FRAME+i,&scene[s])))
            .chain(self.fixed.iter().map(|&(f,s)|(f,&scene[s]))).collect()
    }
    pub(super) fn configure(&self,r:&mut Renderer) {
        for a in &self.moving {for &slot in &a.slots {r.mark_temporal_dynamic(slot);r.set_visible(slot,true);}}
    }
    pub(super) fn present(&self,r:&mut Renderer,hull:Mat4,turret:Mat4) {
        let models=self.models(hull);
        for (i,a) in self.moving.iter().enumerate(){for &slot in &a.slots {r.set_model(slot,models[i]);}}
        for &(frame,slot) in &self.fixed {r.set_model(slot,if frame==0{hull}else{turret});}
    }
    fn contact(&self,index:usize,hull:Mat4)->Vec3 {self.models(hull)[index].transform_point3(self.moving[index].def.source_contact_hull_m.into())}
    pub(super) fn carrier_contact(&self,mount:usize,turret:Mat4)->Vec3 {
        turret.transform_point3(self.service_parts[mount-24].source_contact_turret_m.into())
    }
    pub(super) fn carrier_model(&self,mount:usize,extraction:f32,turret:Mat4)->Mat4 {
        let path=&self.service_parts[mount-24].withdrawal_path_turret_m;
        let mut remaining=path.iter().map(|p|Vec3::from(*p).length()).sum::<f32>()*extraction;
        let mut offset=Vec3::ZERO;
        for p in path {let delta=Vec3::from(*p);let length=delta.length();
            if length>0.0 {offset+=delta*(remaining/length).clamp(0.0,1.0);remaining=(remaining-length).max(0.0);}}
        turret*Mat4::from_translation(offset)
    }
    pub(super) fn carrier_ready(&self,mount:usize,state:&WorkshopSnapshot)->bool {
        self.service_enabled&&state.part_at(9).is_none()&&state.part_at(11).is_none()
            &&(mount!=24||state.part_at(25).is_none())&&self.state.fraction[0]==0.0
    }
    fn clear_step(&self,index:usize,next:f32,models:&[Mat4;surface_fire::FRAMES],cabin:&surface_fire::CabinFire)->bool {
        let before=self.state.fraction;let angle=(next-before[index])*self.moving[index].def.open_angle_deg.to_radians();
        let old=self.local_models(before);let parent=if index==0 {Mat4::IDENTITY}else{old[0]};
        let hinge=parent.transform_point3(self.moving[index].def.hinge_hull_m.into());
        let axis=parent.transform_vector3(self.moving[index].def.axis_hull.into()).normalize();
        let affected=if index==0 {(0..4).collect::<Vec<_>>()}else{vec![index]};
        let excluded=affected.iter().map(|i|FIRST_ENGINE_FRAME+i).collect::<Vec<_>>();
        self.rotation_clear(old,&affected,hinge,axis,angle,models,cabin,&excluded,next==0.0)
    }
    /// Rigid source triangles against the actual stationary source, with bounded curved-path
    /// subdivision. `terminal_fit` applies only to the catalog's original closed stop.
    fn rotation_clear(&self,old:[Mat4;4],affected:&[usize],hinge:Vec3,axis:Vec3,angle:f32,
        models:&[Mat4;surface_fire::FRAMES],cabin:&surface_fire::CabinFire,excluded:&[usize],terminal_fit:bool)->bool {
        self.last_sweep.replace(None);
        if !angle.is_finite(){return false;}if angle.abs()<1e-8{return true;}
        let radius=affected.iter().flat_map(|&i|self.moving[i].meshes.iter().flat_map(move|m|m.vertices.iter()
            .map(move|v|{let p=old[i].transform_point3(Vec3::from(v.pos))-hinge;(p-axis*p.dot(axis)).length() as f64})))
            .fold(0.0_f64,f64::max);
        let Some(steps)=sweep::rotation_steps(radius,angle as f64,96)else{
            self.last_sweep.replace(Some(serde_json::json!({"reason":"subdivision_budget","radius_m":radius,"angle_rad":angle})));return false;};
        let contacts=cabin.source_contacts(models).without_body_frames(excluded);let mut obstacles=Vec::new();
        // Hull-local transforms avoid kilometre-scale world subtraction. This conservative
        // bound covers f32 source/rigid-transform arithmetic within the ten-metre source bay.
        let source_error_m=32.0*f32::EPSILON as f64*10.0;
        let rotated=|fraction:f32|Mat4::from_translation(hinge)*Mat4::from_axis_angle(axis,angle*fraction)
            *Mat4::from_translation(-hinge);
        for step in 0..steps.count {
            let a=rotated(step as f32/steps.count as f32);let b=rotated((step+1) as f32/steps.count as f32);
            for &i in affected {for (mesh_index,mesh) in self.moving[i].meshes.iter().enumerate() {for (triangle,tri) in mesh.indices.chunks_exact(3).enumerate() {
                let p=[tri[0],tri[1],tri[2]].map(|j|Vec3::from(mesh.vertices[j as usize].pos));
                let start=p.map(|v|(a*old[i]).transform_point3(v));let end=p.map(|v|(b*old[i]).transform_point3(v));
                let pad=(steps.arc_padding_m+source_error_m) as f32;
                let lo=start.into_iter().chain(end).fold(Vec3::splat(f32::INFINITY),Vec3::min)-Vec3::splat(pad);
                let hi=start.into_iter().chain(end).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max)+Vec3::splat(pad);
                if !contacts.world_triangles_into(lo,hi,&mut obstacles,4096){
                    self.last_sweep.replace(Some(serde_json::json!({"reason":"source_query_budget","assembly":i,"mesh":mesh_index,"triangle":triangle})));return false;}
                for &obstacle in &obstacles {
                    if sweep::triangle_sweep_blocked(start,end,obstacle,pad as f64)
                        &&!sweep::rotation_contact_separates(start,obstacle,hinge,axis,steps.angle_per_step,source_error_m)
                        &&!(terminal_fit&&step+1==steps.count
                            &&sweep::rotation_contact_separates(end,obstacle,hinge,axis,-steps.angle_per_step,source_error_m)){
                        self.last_sweep.replace(Some(serde_json::json!({"reason":"source_contact","assembly":i,"mesh":mesh_index,
                            "triangle":triangle,"step":step,"steps":steps.count,"hinge":hinge.to_array(),"axis":axis.to_array(),
                            "step_angle":steps.angle_per_step,"source_error_m":source_error_m,
                            "start":start.map(|p|p.to_array()),"end":end.map(|p|p.to_array()),"obstacle":obstacle.map(|p|p.to_array())})));return false;}
                }
            }}}
        }true
    }
}

impl Game {
    /// Build the fitted mechanism frames from their actual joints. Inverting a world pose
    /// four kilometres from the origin loses more precision than the source pin clearance.
    fn engine_local_source_models(&self)->[Mat4;surface_fire::FRAMES] {
        let turret=Mat4::from_translation(self.player.parts.turret_pivot)*Mat4::from_rotation_y(self.player.turret_yaw);
        let gun=turret*Mat4::from_translation(self.player.parts.gun_trunnion)*Mat4::from_rotation_x(-self.player.gun_pitch);
        let visual=gun*Mat4::from_translation(Vec3::new(0.0,0.0,-crate::tank::gun_recoil_distance(self.player.shot_age)));
        self.fire_surface_models_at(Mat4::IDENTITY,turret,gun,visual)
    }
    /// An open leaf and its handles remain solid to the ordinary gun/turret controls. Test
    /// relative motion in a nearby source frame, not a guessed yaw exclusion sector.
    pub(in crate::game) fn constrain_engine_service_laying(&mut self,previous_yaw:f32,previous_pitch:f32) {
        if self.workshop.engine.state.fraction.iter().all(|v|*v==0.0){return;}
        let requested_yaw=self.player.turret_yaw;let requested_pitch=self.player.gun_pitch;
        self.player.turret_yaw=previous_yaw;self.player.gun_pitch=previous_pitch;
        let yaw_delta=(requested_yaw-previous_yaw+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
        if self.engine_laying_clear(false,-yaw_delta){self.player.turret_yaw=requested_yaw;}
        else {self.traverse_jam_offset=0.0;}
        if self.engine_laying_clear(true,requested_pitch-previous_pitch){self.player.gun_pitch=requested_pitch;}
    }
    fn engine_laying_clear(&self,gun_only:bool,relative_angle:f32)->bool {
        if relative_angle.abs()<1e-8{return true;}
        let local=self.engine_local_source_models();let inverse=local[if gun_only{8}else{1}].inverse();
        let hull=inverse;let models=local.map(|m|inverse*m);
        let mut admitted=[false;surface_fire::FRAMES];
        for i in if gun_only {&[2,8,12,13][..]}else{&[1,2,4,6,7,8,12,13,15][..]} {admitted[*i]=true;}
        for part in &self.workshop.state.parts {
            let follows=match part.location {
                PartLocation::Mount{mount}=>if gun_only {mount==7}else{mount<16||mount>=24},
                PartLocation::VehicleRest{turret:true,..}=>!gun_only,_=>false,
            };
            if follows {for i in self.workshop.source_frames(part.bank){admitted[i]=true;}}
        }
        let excluded=(0..surface_fire::FRAMES).filter(|i|!admitted[*i]).collect::<Vec<_>>();
        self.workshop.engine.rotation_clear(self.workshop.engine.models(hull),&[0,1,2,3],Vec3::ZERO,
            if gun_only {Vec3::X}else{Vec3::Y},relative_angle,&models,&self.cabin_fire,&excluded,false)
    }
    pub(super) fn workshop_carrier_motion_allowed(&self,mount:usize,from:f32,to:f32)->bool {
        let Some(part)=self.workshop.state.part_at(mount).or_else(||self.workshop.state.held())else{return false;};
        let source=&self.workshop.banks[part.bank];let models=self.engine_local_source_models();let turret=models[1];
        let contacts=self.cabin_fire.source_contacts(&models).without_body_frames(&[FIRST_FRAME+part.bank]);let mut obstacles=Vec::new();
        let mut segments=vec![from];let corner=0.060/0.210;
        if mount==25&&((from<corner&&to>corner)||(from>corner&&to<corner)){segments.push(corner);}
        segments.push(to);let error=32.0*f32::EPSILON as f64*10.0;
        for pair in segments.windows(2) {
            let a=self.workshop.engine.carrier_model(mount,pair[0],turret);
            let b=self.workshop.engine.carrier_model(mount,pair[1],turret);
            for mesh in &source.meshes {for tri in mesh.indices.chunks_exact(3) {
                let p=[tri[0],tri[1],tri[2]].map(|i|Vec3::from(mesh.vertices[i as usize].pos));
                let start=p.map(|v|a.transform_point3(v));let end=p.map(|v|b.transform_point3(v));
                let lo=start.into_iter().chain(end).fold(Vec3::splat(f32::INFINITY),Vec3::min)-Vec3::splat(error as f32);
                let hi=start.into_iter().chain(end).fold(Vec3::splat(f32::NEG_INFINITY),Vec3::max)+Vec3::splat(error as f32);
                if !contacts.world_triangles_into(lo,hi,&mut obstacles,4096){return false;}
                for &t in &obstacles {
                    if sweep::triangle_sweep_blocked(start,end,t,error)&&!translation_separates(start,end,t,error)
                        &&!(to==0.0&&translation_separates(end,start,t,error)){return false;}
                }
            }}
        }true
    }
    /// Called after ordinary workshop person/reach/held-item gates, before other LMB actions.
    pub(super) fn route_engine_access(&mut self,down:bool,pressed:bool,dt:f32,r:&mut Renderer)->bool {
        let Some(index)=self.engine_access_target(r.camera.pos,r.camera.forward())else{return false;};
        if !self.workshop.engine.service_enabled {self.hover_label=Some("ENGINE COVER SERVICE UNAVAILABLE");return true;}
        if self.engine_on {self.hover_label=Some("STOP ENGINE BEFORE RELEASING COVER");return true;}
        if self.workshop.cooling.engine_c(&Default::default())>60.0 {self.hover_label=Some("HOT ENGINE - WAIT FOR COOLING");return true;}
        let state=&mut self.workshop.engine.state;
        // Release and press again reverses a partial stroke, so a genuine obstruction
        // cannot strand the cover half open. Holding the same press preserves direction.
        if pressed {state.begin_grip(index);}
        let opening=state.grip.filter(|g|g.assembly==index).map_or(state.fraction[index]<1.0,|g|g.opening);
        if index==0&&state.fraction[1..].iter().any(|v|*v<1.0) {
            self.hover_label=Some("RELEASE BOTH SIDE LOCKS AND REAR LATCH");return true;
        }
        if index==0&&opening&&[9,11,24,25].into_iter().any(|m|self.workshop.state.part_at(m).is_some()) {
            self.hover_label=Some("REMOVE TWO RIGHT SPARE SHOES AND CARRIER HARDWARE");return true;
        }
        if index!=0&&!opening&&state.fraction[0]>0.0 {self.hover_label=Some("LOWER COVER BEFORE CLOSING ITS LOCKS");return true;}
        self.hover_label=Some(if index==0 {if opening {"HOLD LMB - RAISE ENGINE COVER"}else{"HOLD LMB - LOWER ENGINE COVER"}}
            else if opening {"HOLD LMB - RELEASE COVER LATCH"}else{"HOLD LMB - SECURE COVER LATCH"});
        if !down||self.workshop.primary_guard {return true;}
        let current=self.workshop.engine.state.fraction[index];
        let next=(current+(if opening{1.0}else{-1.0})*dt.clamp(0.0,0.05)/(if index==0{5.0}else{1.5})).clamp(0.0,1.0);
        let models=self.engine_local_source_models();
        if self.workshop.engine.clear_step(index,next,&models,&self.cabin_fire) {
            self.workshop.engine.state.fraction[index]=next;
            if next==0.0||next==1.0 {self.workshop.primary_guard=true;self.workshop.engine.state.grip=None;}
        }else{self.hover_label=Some("OBSTRUCTED - CLEAR THE COVER'S PATH");}
        true
    }
    pub(super) fn engine_access_target(&self,eye:Vec3,look:Vec3)->Option<usize> {
        let hull=self.player.hull_model();
        (0..4).filter_map(|i|{let p=self.workshop.engine.contact(i,hull);
            (equipment::repair_in_reach(eye,look,p,0.08)&&self.workshop_clear_to(eye,p)).then_some((i,eye.distance_squared(p)))})
            .min_by(|a,b|a.1.total_cmp(&b.1)).map(|(i,_)|i)
    }
}

fn translation_separates(start:[Vec3;3],end:[Vec3;3],obstacle:[Vec3;3],error:f64)->bool {
    let o=obstacle.map(|p|p.as_dvec3());let e=o[1]-o[0];let f=o[2]-o[0];let cross=e.cross(f);
    if !cross.is_finite()||cross.length()<e.length()*f.length()*1e-5{return false;}
    let normal=cross.normalize();let a=start.map(|p|(p.as_dvec3()-o[0]).dot(normal));
    let b=end.map(|p|(p.as_dvec3()-o[0]).dot(normal));
    a.iter().any(|d|d.abs()<=error)&&a.iter().chain(&b).all(|d|d.is_finite()&&*d>=-error)
        &&(0..3).filter(|&i|a[i].abs()<=error).all(|i|b[i]-a[i]>1e-10)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn engine_access_restore_requires_released_latches_and_finite_state() {
        let shut=EngineAccessState::default();assert!(shut.validate().is_ok());
        let open=EngineAccessState{fraction:[0.45,1.0,1.0,1.0],grip:Some(Grip{assembly:0,opening:true})};
        assert!(open.validate().is_ok());
        for latch in 1..4 {let mut bad=open.clone();bad.fraction[latch]=0.99;
            assert!(bad.validate().is_err(),"raised leaf cannot coexist with an engaged latch");}
        assert!(EngineAccessState{fraction:[0.0,0.2,0.5,0.0],grip:Some(Grip{assembly:1,opening:true})}.validate().is_ok());
        for value in [f32::NAN,f32::INFINITY,-0.01,1.01] {for component in 0..4 {
            let mut bad=shut.clone();bad.fraction[component]=value;assert!(bad.validate().is_err());}}
        let mut bad=open;bad.grip=Some(Grip{assembly:4,opening:false});assert!(bad.validate().is_err());
    }
    #[test] fn carrier_terminal_fit_does_not_allow_over_insertion() {
        let surface=[Vec3::new(-2.0,-2.0,0.0),Vec3::new(2.0,-2.0,0.0),Vec3::new(0.0,2.0,0.0)];
        let fitted=[Vec3::new(-0.2,0.0,0.0),Vec3::new(0.2,0.0,0.0),Vec3::new(0.0,0.2,0.0)];
        let withdrawn=fitted.map(|p|p+Vec3::Z*0.01);
        assert!(sweep::triangle_sweep_blocked(withdrawn,fitted,surface,1e-7));
        assert!(translation_separates(fitted,withdrawn,surface,1e-7));
        assert!(!translation_separates(withdrawn,fitted,surface,1e-7));
        assert!(!translation_separates(fitted.map(|p|p-Vec3::Z*0.001),withdrawn,surface,1e-7));
    }
    #[test] fn blocked_partial_stroke_can_reverse_without_teleport_or_new_state() {
        let mut state=EngineAccessState{fraction:[0.4,1.0,1.0,1.0],grip:Some(Grip{assembly:0,opening:true})};
        let fraction=state.fraction;state.begin_grip(0);
        assert_eq!(state.fraction,fraction);assert_eq!(state.grip,Some(Grip{assembly:0,opening:false}));
        let bytes=bincode::serialize(&state).unwrap();let mut restored:EngineAccessState=bincode::deserialize(&bytes).unwrap();
        restored.validate().unwrap();restored.begin_grip(0);assert!(restored.grip.unwrap().opening);
        restored.fraction[0]=0.0;restored.begin_grip(0);assert!(restored.grip.unwrap().opening);
        restored.fraction[0]=1.0;restored.begin_grip(0);assert!(!restored.grip.unwrap().opening);
    }
    fn source_fixture()->(EngineAccess,surface_fire::CabinFire,[Mat4;surface_fire::FRAMES]) {
        let catalog:Catalog=serde_json::from_slice(&crate::resource::read(CATALOG).unwrap()).unwrap();
        let load=|path:&str|crate::gltf_load::load_glb_partitioned_roots_with_material_override(path,&[],Some(crate::tank::tiger_paint_finish)).unwrap().remove(0);
        let roots=catalog.removed_roots.hull.iter().map(String::as_str).collect::<Vec<_>>();
        let hull=crate::gltf_load::load_glb_partitioned_roots_with_material_override(&catalog.source_hull,&roots,
            Some(crate::tank::tiger_paint_finish)).unwrap().remove(0);
        let turret=crate::gltf_load::load_glb_partitioned_roots_with_material_override(&catalog.source_turret,
            &["Turret_0128","Turret_0147","Turret_0132","Turret_0153"],Some(crate::tank::tiger_paint_finish)).unwrap().remove(0);
        let gun=load("assets/tanks/tiger/baked/tiger_gun.glb");
        let moving=IDS.into_iter().map(|id| {
            let def=catalog.assemblies.iter().find(|a|a.id==id).unwrap().clone();
            let meshes=def.files.iter().map(|f|load(&format!("{}{f}",catalog.asset_root))).collect();
            Moving{def,slots:Vec::new(),meshes}
        }).collect::<Vec<_>>();
        let fixed=catalog.fixed_replacements.iter().map(|entry|(
            match entry.parent_frame.as_str(){"hull"=>0,"turret"=>1,_=>panic!("unknown fixed frame")},
            load(&format!("{}{}",catalog.asset_root,entry.file)))).collect::<Vec<_>>();
        let access=EngineAccess{state:EngineAccessState::default(),service_enabled:true,moving,fixed:Vec::new(),
            service_parts:catalog.service_parts.try_into().ok().unwrap(),last_sweep:std::cell::RefCell::new(None)};
        let mut refs=vec![(0,&hull),(1,&turret),(2,&gun)];
        for(frame,mesh)in &fixed{refs.push((*frame,mesh));}
        for(i,a)in access.moving.iter().enumerate(){for m in &a.meshes{refs.push((FIRST_ENGINE_FRAME+i,m));}}
        let cabin=surface_fire::CabinFire::new(&refs);drop(refs);
        let mut models=[Mat4::IDENTITY;surface_fire::FRAMES];
        models[1]=Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot)*Mat4::from_rotation_y(-66.0_f32.to_radians());
        models[2]=models[1]*Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion);
        (access,cabin,models)
    }
    #[test]
    #[ignore="requires private Tiger source GLBs; actual source hinge-contact regression"]
    fn engine_cover_source_latches_open_through_full_triangle_admission() {
        let(mut access,cabin,mut models)=source_fixture();
        for index in 1..4 {for step in 1..=90 {
            access.append_models(&mut models);let next=step as f32/90.0;
            assert!(access.clear_step(index,next,&models,&cabin),"{} step {step}: {:?}",IDS[index],access.last_sweep.borrow());
            access.state.fraction[index]=next;
        }}
    }
    #[test]
    #[ignore="requires private Tiger source GLBs; full open/close source contact admission"]
    fn engine_cover_source_full_service_stroke_opens_and_returns_to_original_stop() {
        let(mut access,cabin,mut models)=source_fixture();
        // Staging already-released latches isolates the long lid sweep. The preceding
        // source test proves their actual opening; closing is exercised below as well.
        access.state.fraction=[0.0,1.0,1.0,1.0];
        for opening in [true,false] {for step in 1..=300 {
            access.append_models(&mut models);
            let next=if opening {step as f32/300.0}else{1.0-step as f32/300.0};
            assert!(access.clear_step(0,next,&models,&cabin),"lid opening={opening} step{step}: {:?}",access.last_sweep.borrow());
            access.state.fraction[0]=next;
        }}
        for index in 1..4 {for step in 1..=90 {
            access.append_models(&mut models);let next=1.0-step as f32/90.0;
            assert!(access.clear_step(index,next,&models,&cabin),"{} closing step{step}: {:?}",IDS[index],access.last_sweep.borrow());
            access.state.fraction[index]=next;
        }}
        assert_eq!(access.state.fraction,[0.0;4]);
        access.state.validate().unwrap();
    }
}
