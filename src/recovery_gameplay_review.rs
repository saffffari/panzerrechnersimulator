//! Explicit staged-fixture review of real recovery actions. No state assertion is substituted
//! for a successful action; failures remain in the receipt. Starting poses/item/contact are
//! labelled fixtures. Vehicle motion, cargo ownership and body identity use the live runtime.
use super::*;
use anyhow::{Context,ensure};
use serde_json::{json,Value};
use std::{io::Write,path::Path,process::{Child,Command,Stdio}};
const DT:f32=1./60.;
struct Clip {child:Child,size:[u32;2],ticks:u64,frames:u64,phases:Vec<Value>,path:String}
impl Clip {
    fn new(path:String,size:[u32;2])->Result<Self>{
        let child=Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-f","rawvideo","-pixel_format","rgba","-video_size",&format!("{}x{}",size[0],size[1]),"-framerate","12","-i","pipe:0","-an","-c:v","libx264","-threads","2","-preset","fast","-crf","17","-pix_fmt","yuv420p","-movflags","+faststart",&path]).stdin(Stdio::piped()).spawn().context("start recovery review video")?;
        Ok(Self{child,size,ticks:0,frames:0,phases:vec![],path})
    }
    fn frame(&mut self,g:&Game,r:&mut Renderer,d:&wgpu::Device,q:&wgpu::Queue,phase:&str)->Result<()>{
        self.ticks+=1;if self.ticks%5!=0{return Ok(());}
        crate::simulation_uploads::drain(d,q)?;
        r.ui_reset();
        r.draw_text(&format!("STAGED RECOVERY CHECK | {}",phase.replace('_'," ")),0.02,0.025,0.024,[1.,1.,1.,1.],crate::ui::Align::Left);
        let rgba=r.render_to_rgba_with_hud(d,q,self.size[0],self.size[1])?;
        self.child.stdin.as_mut().context("video encoder closed")?.write_all(&rgba)?;
        self.phases.push(json!({"frame":self.frames,"simulation_s":self.ticks as f64/60.,"phase":phase,"camera_m":r.camera.pos.to_array(),"carrier_m":g.halftrack_runtime.as_ref().map(|v|v.motion.position.to_array())}));self.frames+=1;Ok(())
    }
    fn finish(&mut self)->Result<Value>{drop(self.child.stdin.take());ensure!(self.child.wait()?.success(),"recovery video encoding failed");Ok(json!({"path":self.path,"fps":12,"frames":self.frames,"seconds":self.frames as f64/12.,"audio":false,"scope":"continuous60Hz simulation within each labelled phase; explicit fixture repositioning cuts between phases; convergence-only renders excluded","samples":self.phases}))}
}
impl Drop for Clip{fn drop(&mut self){if !matches!(self.child.try_wait(),Ok(Some(_))){let _=self.child.kill();let _=self.child.wait();}}}
struct Proof<'a>{prefix:String,device:&'a wgpu::Device,queue:&'a wgpu::Queue,checks:Vec<Value>,samples:Vec<Value>,captures:Vec<String>,clip:Option<Clip>}
impl Proof<'_>{
    fn check(&mut self,label:&str,pass:bool,detail:Value){println!("[recovery-review] {label}: {}",if pass{"PASS"}else{"FAIL"});self.checks.push(json!({"check":label,"pass":pass,"detail":detail}));}
    fn ticks(&mut self,g:&mut Game,r:&mut Renderer,seconds:f32,input:TankInput,label:&str)->Result<()> {
        for tick in 0..(seconds/DT).ceil()as usize {g.update(&input,DT,r);if tick%30==29{crate::simulation_uploads::drain(self.device,self.queue)?;}
            if let Some(clip)=self.clip.as_mut(){clip.frame(g,r,self.device,self.queue,label)?;}
            if tick%60==59 {self.samples.push(json!({"phase":label,"second":(tick+1)/60,"transport":g.halftrack_receipt(),"recovery":g.recovery_receipt(),"bodies":g.crew_body_recovery_receipt()}));}}
        Ok(())
    }
    fn still(&mut self,g:&mut Game,r:&mut Renderer,label:&str)->Result<()> {
        g.update(&TankInput::default(),0.,r);r.ui_reset();r.reset_history();
        let warm=crate::converge_headless_screenshot(r,self.device,self.queue,24)?;
        self.check(&format!("{label}: exposure converged"),warm.stable,json!({"camera":r.camera.pos.to_array()}));
        let path=format!("{}_{}.png",self.prefix,label);r.render_to_png(self.device,self.queue,&path,1280,720)?;self.captures.push(path);Ok(())
    }
}
fn stage_foot(g:&mut Game,r:&mut Renderer,mut feet:Vec3,look:Vec3){
    feet.y=g.expedition_vehicle_ground(feet.x,feet.z);g.mode=Mode::Foot;g.climb.active=false;g.foot.pos=feet;g.foot.vel_y=0.;g.foot.airborne=false;g.foot.crouch_t=0.;
    let direction=(look-g.foot.eye()).normalize_or_zero();g.foot.yaw=direction.x.atan2(-direction.z);g.foot.pitch=direction.y.asin();
    r.camera.pos=g.foot.eye();r.camera.set_forward(direction);g.equipment_eye=r.camera.pos;g.equipment_look=direction;
}
fn act(g:&mut Game,r:&Renderer)->Value {let hover=g.recovery_hover(r.camera.pos,r.camera.forward());let result=g.recovery_interact(r.camera.pos,r.camera.forward());
    json!({"hover":hover,"consumed":result.as_ref().is_ok_and(|v|*v),"error":result.err().map(|e|format!("{e:#}"))})}
fn fuel_witness(proof:&mut Proof<'_>,g:&mut Game,r:&mut Renderer,target:FuelTarget,can_index:usize)->Result<()> {
    let v=g.halftrack_runtime.as_mut().unwrap();
    // Deliberate consumed-fuel fixture. The existing can is not refilled or duplicated.
    let capacity=if target==FuelTarget::Carrier{40.}else{5.};
    let destination=if target==FuelTarget::Carrier{&mut v.motion.fuel}else{&mut v.recovery.state.winch_fuel_l};*destination=capacity-0.5;
    let fill=v.recovery.fill_model(v.motion.root(),target);let at=fill.w_axis.truncate();
    let mut normal=fill.transform_vector3(Vec3::X).normalize_or_zero();
    if normal.y.abs()>0.7{normal=Vec3::NEG_Z;}
    stage_foot(g,r,at+normal*0.65,at);
    g.held_item=Some(can_index);g.pickups[can_index].portable.as_mut().unwrap().equip();
    let can_before=g.pickups[can_index].portable.unwrap().fuel_liters();let destination_before=capacity-0.5;
    let click=act(g,r);proof.ticks(g,r,0.3,TankInput{fire2:true,..Default::default()},"finite_fuel_pour")?;
    let v=g.halftrack_runtime.as_ref().unwrap();let destination_after=if target==FuelTarget::Carrier{v.motion.fuel}else{v.recovery.state.winch_fuel_l};
    let can_after=g.pickups[can_index].portable.unwrap().fuel_liters();
    proof.check(if target==FuelTarget::Carrier{"carrier fill consumes exactly the held finite can"}else{"winch fill consumes exactly the held finite can"},
        destination_after>destination_before+0.1&&(can_before-can_after-(destination_after-destination_before)).abs()<0.0002,
        json!({"click":click,"can_before_l":can_before,"can_after_l":can_after,"destination_before_l":destination_before,"destination_after_l":destination_after}));
    proof.ticks(g,r,0.2,TankInput::default(),"released_fuel_pour")?;
    let v=g.halftrack_runtime.as_ref().unwrap();let stopped=if target==FuelTarget::Carrier{v.motion.fuel}else{v.recovery.state.winch_fuel_l};
    proof.check("released LMB stops finite fuel transfer",stopped==destination_after&&g.pickups[can_index].portable.unwrap().fuel_liters()==can_after,json!({"destination_l":stopped}));
    g.put_down_held_portable();Ok(())
}
pub(super) fn run(requested:&str,video:bool)->Result<()> {
    let prefix=crate::capture_target(requested);let receipt=format!("{prefix}.json");
    ensure!(!Path::new(&receipt).exists(),"recovery review prefix already exists");if let Some(p)=Path::new(&prefix).parent(){std::fs::create_dir_all(p)?;}
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
    let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);let(device,queue)=crate::request_device(&adapter);
    let(mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
    g.mute_audio();g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;g.enable_expedition_theatre(&mut r)?;
    // Isolated equipment experiment; no claim about surviving a live assault in this review.
    g.enemy_active.fill(false);
    let clip=if video{Some(Clip::new(format!("{prefix}.mp4"),[r.iw,r.ih])?)}else{None};
    let mut proof=Proof{prefix,device:&device,queue:&queue,checks:Vec::new(),samples:Vec::new(),captures:Vec::new(),clip};
    let mut root=g.halftrack_runtime.as_ref().context("recovery transport absent")?.motion.root();
    let person=g.expedition_state().unwrap().controlled_person.context("controlled identity absent")?;
    let initial_people=g.expedition_state().unwrap().people.len();
    let mut rear=root.transform_point3(Vec3::new(0.,0.,-3.8));
    stage_foot(&mut g,&mut r,rear,root.transform_point3(KIT_LOCAL));
    proof.ticks(&mut g,&mut r,0.5,TankInput::default(),"initial_open_ramp")?;
    proof.still(&mut g,&mut r,"01_open_ramp")?;
    let can_index=g.pickups.iter().position(|p|p.portable.is_some_and(PortableItem::has_fuel));
    proof.check("existing finite fuel can is available",can_index.is_some(),json!({"pickup_index":can_index}));
    if let Some(index)=can_index {fuel_witness(&mut proof,&mut g,&mut r,FuelTarget::Carrier,index)?;}
    stage_foot(&mut g,&mut r,rear,root.transform_point3(KIT_LOCAL));


    // A named single finite spare is initially in the current person's hands. Stowing and
    // unloading below still go through the exact normal interaction and timed ownership paths.
    let item=crate::expedition::ItemId(799_991);
    g.workshop.state.commission(item,super::super::super::workshop_runtime::state::PartKind::VisionBlock,0.83,Mat4::from_translation(rear).to_cols_array())?;
    g.workshop.state.parts.iter_mut().find(|p|p.item==item).unwrap().location=super::super::super::workshop_runtime::state::PartLocation::Hands{person};
    proof.ticks(&mut g,&mut r,0.1,TankInput::default(),"fixture_held_existing_spare")?;
    let cargo_click=act(&mut g,&r);proof.ticks(&mut g,&mut r,2.,TankInput::default(),"load_spare")?;
    let cargo_loaded=g.halftrack_runtime.as_ref().unwrap().recovery.state.cargo.iter().any(|c|c.item==item&&c.transfer.is_none());
    proof.check("LMB stows the same finite spare through timed transfer",cargo_loaded,cargo_click);
    proof.still(&mut g,&mut r,"02_spare_loaded")?;
    if let Some(c)=g.halftrack_runtime.as_ref().unwrap().recovery.state.cargo.iter().find(|c|c.item==item){let target=(root*c.local).transform_point3((c.min+c.max)*0.5);
        stage_foot(&mut g,&mut r,rear,target);let click=act(&mut g,&r);proof.ticks(&mut g,&mut r,2.,TankInput::default(),"unload_spare")?;
        proof.check("source-part LMB unload preserves identity and condition",g.workshop.state.parts.iter().any(|p|p.item==item&&p.condition==0.83&&matches!(p.location,super::super::super::workshop_runtime::state::PartLocation::Ground{..})),click);
    }

    stage_foot(&mut g,&mut r,rear,root.transform_point3(KIT_LOCAL));
    let rig_click=act(&mut g,&r);proof.ticks(&mut g,&mut r,12.1,TankInput::default(),"winch_rolls_down_ramp")?;
    let deployed=matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Rolling{..});
    proof.check("180kg winch rolls down the ramp instead of appearing in hands",deployed,rig_click);
    proof.still(&mut g,&mut r,"04_winch_on_ground")?;

    if deployed {
        let before=g.halftrack_runtime.as_ref().unwrap().recovery.state.clone();
        let kit=g.halftrack_runtime.as_ref().unwrap().recovery.kit_root(root,Vec3::ZERO).w_axis.truncate();
        stage_foot(&mut g,&mut r,root.transform_point3(Vec3::new(0.,0.,-5.05)),kit);
        let return_click=act(&mut g,&r);
        proof.ticks(&mut g,&mut r,12.1,TankInput::default(),"winch_returns_up_ramp")?;
        let after=&g.halftrack_runtime.as_ref().unwrap().recovery.state;
        let stowed=matches!(after.rigging,Rigging::Stowed);
        proof.check("same winch returns through source-clear ramp without resetting fuel or work",stowed&&after.winch_fuel_l==before.winch_fuel_l&&after.pull_metres==before.pull_metres,
            json!({"click":return_click,"recovery":g.recovery_receipt()}));
        proof.still(&mut g,&mut r,"04b_winch_restowed")?;
        if stowed {
            stage_foot(&mut g,&mut r,rear,root.transform_point3(KIT_LOCAL));
            let click=act(&mut g,&r);proof.ticks(&mut g,&mut r,12.1,TankInput::default(),"same_winch_redeploys")?;
            proof.check("restowed winch can be deployed again",matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Rolling{..}),click);
        }
    }

    // Explicit helper starting poses remain fixture cuts. Kit motion itself is ordinary:
    // first pull aft out of the articulated gate envelope, then push it sideways.
    if matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Rolling{..}) {
        g.recovery_stow_hands();stage_foot(&mut g,&mut r,root.transform_point3(Vec3::new(0.,0.,-5.3)),root.transform_point3(Vec3::new(0.,0.4,-4.0)));
        let resumed=g.recovery_stow_hands();proof.ticks(&mut g,&mut r,2.,TankInput{drive:-1.,..Default::default()},"pull_winch_straight_clear_of_gate")?;
        proof.ticks(&mut g,&mut r,1.,TankInput::default(),"settle_winch_beyond_gate")?;g.recovery_stow_hands();
        let kit=g.halftrack_runtime.as_ref().unwrap().recovery.kit_root(root,Vec3::ZERO).w_axis.truncate();let local=root.inverse().transform_point3(kit);
        stage_foot(&mut g,&mut r,root.transform_point3(Vec3::new(-1.2,0.,local.z)),root.transform_point3(Vec3::new(1.3,1.,local.z)));
        let side=g.recovery_stow_hands();proof.ticks(&mut g,&mut r,5.6,TankInput{drive:1.,..Default::default()},"roll_same_winch_beside_loading_aisle")?;
        proof.ticks(&mut g,&mut r,1.,TankInput::default(),"settle_same_winch_clear_of_patient")?;g.recovery_stow_hands();
        proof.check("ordinary handling moves finite winch clear of gate and stretcher aisle",resumed&&side&&stretcher_aisle_clear(g.halftrack_runtime.as_ref().unwrap()),g.recovery_receipt());
    }

    // Existing roster person 14 is staged at the ramp, then an actual source-triangle body
    // contact is resolved. This is an explicit controlled impact, not a projectile-flight test.
    let rescue=crate::expedition::PersonId(14);
    let source=g.crew_arm_animation.transport_pose_vertices(1.,0.,false).context("finite person source absent")?;
    let (lo,hi)=source.iter().map(|v|Vec3::from_array(v.pos)).fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),|(lo,hi),p|(lo.min(p),hi.max(p)));
    let v=g.halftrack_runtime.as_ref().unwrap();let inverse=root.inverse();let rear_bound=v.tailgate_source.iter().flatten().map(|&p|inverse.transform_point3(v.tailgate_model().transform_point3(p)).z).fold(v.bounds.0.z,f32::min);
    let casualty_start=root.transform_point3(Vec3::new(-0.5,0.,rear_bound-(hi-lo).length()*1.05-0.10));
    g.camp_person_set_world_position(rescue,Vec3::new(casualty_start.x,g.expedition_vehicle_ground(casualty_start.x,casualty_start.z),casualty_start.z))?;
    proof.ticks(&mut g,&mut r,0.2,TankInput::default(),"fixture_reserve_at_ramp")?;
    let shape=g.camp_field_shapes(casualty_start,casualty_start,3.).into_iter().find(|s|s.person==rescue);
    let mut patient_actor=None;
    if let Some(shape)=shape {let target=shape.segments[1].pos;let from=target+Vec3::Z*1.2;
        let contact=g.field_person_projectile_contact(from,target-Vec3::Z*0.3);
        if let Some(ProjectileContact{pos,kind:ProjectileContactKind::FieldPerson(id,part),..})=contact {
            let impact=CrewRagdollImpact{hit_part:part,point_world:pos,velocity_world:Vec3::NEG_Z*250.,energy_j:220.,blast_origin_world:pos,explosive_energy_j:0.,behind_armor:0.,blast_proximity_m:0.};
            let actor=g.retain_struck_field_person(id,impact)?;g.injure_field_actor(actor,part,pos,Vec3::NEG_Z*250.,220.,0.,0.,799_991);patient_actor=Some(actor.peer_id);
        }
    }
    proof.check("source body hit retains existing finite person without a seat",patient_actor.is_some_and(|a|g.singleplayer_person_for_actor(a)==Some(rescue)&&g.singleplayer_slot_for_actor(a).is_none()),json!({"actor":patient_actor}));
    proof.ticks(&mut g,&mut r,1.,TankInput::default(),"field_casualty_settles")?;
    if let Some(actor)=patient_actor {
        // This review already declares helper-position cuts. Derive this cut from the actual
        // fallen flesh, not a stale fixed point from the previous closer casualty fixture.
        let body=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor).unwrap();
        let flesh=g.crew_corpse_render.borrow().as_ref().context("retained patient source absent")?.world_vertices(&g.player,body,g.singleplayer_actor_source_identity(actor));
        let inverse=root.inverse();let (lo,hi)=flesh.iter().map(|p|inverse.transform_point3(*p)).fold((Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY)),|(lo,hi),p|(lo.min(p),hi.max(p)));
        ensure!(lo.is_finite()&&hi.is_finite(),"patient source contains no usable triangles");
        let helper_local=Vec3::new(lo.x-0.60,0.,(hi.z+0.25).clamp(-4.45,rear_bound-0.20));
        ensure!(helper_local.x.abs()<2.4,"source-derived helper cannot reach the loading aisle");
        let feet=root.transform_point3(helper_local);let eye=feet.with_y(g.expedition_vehicle_ground(feet.x,feet.z))+Vec3::Y*crate::foot::EYE;
        let target=flesh.chunks_exact(3).map(|t|(t[0]+t[1]+t[2])/3.).min_by(|a,b|a.distance_squared(eye).total_cmp(&b.distance_squared(eye))).context("patient source has no target triangle")?;
        stage_foot(&mut g,&mut r,feet,target);
        let grabbed=g.interact_crew_body(r.camera.forward(),r.camera.pos);proof.ticks(&mut g,&mut r,4.,TankInput{fire2:true,..Default::default()},"hold_to_load_casualty")?;
        let loaded=g.halftrack_runtime.as_ref().unwrap().recovery.state.patients.iter().any(|p|p.actor==actor&&p.loading.is_none());
        proof.check("held casualty completes loading without replacing identity",grabbed&&loaded,json!({"grabbed":grabbed,"actor":actor,"helper_fixture_local":helper_local.to_array(),"target_source_point":target.to_array(),"recovery":g.recovery_receipt()}));
        proof.still(&mut g,&mut r,"03_casualty_loaded")?;
        if loaded{
            let mut boarded=false;let mut attempts=Vec::new();
            for side in [1.,-1.]{
                stage_foot(&mut g,&mut r,root.transform_point3(Vec3::new(side*2.,0.,0.8)),root.transform_point3(Vec3::new(side*0.6,1.3,0.8)));
                let result=g.try_board_halftrack(r.camera.pos,r.camera.forward(),&mut r);
                boarded=result.as_ref().is_ok_and(|v|*v);attempts.push(json!({"boarded":boarded,"error":result.err().map(|e|e.to_string())}));if boarded{break;}
            }
            proof.check("source cab access boards with casualty aboard",boarded,json!(attempts));
            if boarded{
                let start=g.halftrack_runtime.as_ref().unwrap().motion.position;let fuel=g.halftrack_runtime.as_ref().unwrap().motion.fuel;
                proof.ticks(&mut g,&mut r,3.1,TankInput::default(),"rear_ramp_closes_before_departure")?;
                proof.ticks(&mut g,&mut r,8.,TankInput{drive:0.45,..Default::default()},"casualty_transport_drive")?;
                proof.ticks(&mut g,&mut r,2.,TankInput{jump:true,..Default::default()},"casualty_transport_brake")?;
                let before=g.export_transport_world_state()?;g.import_transport_world_state(&before)?;
                let after=g.export_transport_world_state()?;
                proof.check("loaded transport owner restores exactly",before.payload==after.payload,json!({"owner":before.key}));
                let v=g.halftrack_runtime.as_ref().unwrap();let moved=v.motion.position.distance(start);
                proof.check("loaded casualty travels with the same finite carrier",moved>2.&&v.motion.fuel<fuel&&g.recovery_owns_actor(actor),json!({"travel_m":moved,"fuel_used_l":fuel-v.motion.fuel,"actor":actor,"recovery":g.recovery_receipt()}));
                proof.still(&mut g,&mut r,"03b_casualty_transport")?;
                let result=g.try_exit_halftrack(&mut r);proof.check("stationary loaded carrier permits physical cab exit",result.as_ref().is_ok_and(|v|*v),json!({"error":result.err().map(|e|e.to_string())}));
                proof.ticks(&mut g,&mut r,3.1,TankInput::default(),"rear_ramp_reopens")?;
                root=g.halftrack_runtime.as_ref().unwrap().motion.root();rear=root.transform_point3(Vec3::new(0.,0.,-3.8));
            }
        }
        // Actual body picking removes carrier attachment; ordinary field source support remains.
        let center=g.couch_ragdolls.iter().find(|b|b.casualty.peer_id==actor).unwrap().last_physical_center;
        stage_foot(&mut g,&mut r,root.transform_point3(Vec3::new(-0.325,0.,-3.65)),center);
        let removed=g.interact_crew_body(r.camera.forward(),r.camera.pos);g.release_crew_body();
        proof.ticks(&mut g,&mut r,1.,TankInput::default(),"unload_body_source_support")?;
        proof.check("unloading retains same anatomical world body",removed&&!g.recovery_owns_actor(actor)&&g.couch_ragdolls.iter().any(|b|b.casualty.peer_id==actor&&b.world_space),g.crew_body_recovery_receipt());
    }

    // A second explicitly labelled fixture places the already-deployed system and the same
    // Tiger on a clear approach. This tests the live cable/force/resource/contact path, not travel.
    if matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Rolling{..}|Rigging::Anchored{..}) {
        let camp=g.expedition_initial_camp_position()?;let mut anchor=camp+Vec3::new(0.,0.,38.);anchor.y=g.hf.height_at(anchor.x,anchor.z)+0.1;
        g.halftrack_runtime.as_mut().unwrap().recovery.state.rigging=Rigging::Anchored{point:anchor,yaw:0.};
        if let Some(index)=can_index {fuel_witness(&mut proof,&mut g,&mut r,FuelTarget::Winch,index)?;}

        g.player.pos=anchor+Vec3::Z*12.;g.player.pos.y=g.hf.height_at(g.player.pos.x,g.player.pos.z);g.player.ride.y=g.player.pos.y;g.player.heading=0.;g.player.speed=0.;g.engine_on=false;
        stage_foot(&mut g,&mut r,anchor+Vec3::Z*1.1,anchor+Vec3::Y*0.2);let take_cable=act(&mut g,&r);
        let mut attached=false;let mut attach_results=Vec::new();
        for x in [-1.15,1.15] {for y in [0.55,0.85,1.1] {let target=g.player.hull_model().transform_point3(Vec3::new(x,y,-3.0));let feet=g.player.hull_model().transform_point3(Vec3::new(x,0.,-4.2));
            stage_foot(&mut g,&mut r,feet,target);let attempt=act(&mut g,&r);attached=matches!(g.halftrack_runtime.as_ref().unwrap().recovery.state.rigging,Rigging::Attached{..});attach_results.push(attempt);if attached{break;}}
            if attached{break;}}
        proof.check("cable attaches only through a real lower hull source contact",attached,json!({"take_cable":take_cable,"attempts":attach_results}));
        if attached {stage_foot(&mut g,&mut r,anchor+Vec3::Z*1.1,anchor+Vec3::Y*0.2);let click=act(&mut g,&r);
            let before=g.player.pos;let fuel=g.halftrack_runtime.as_ref().unwrap().recovery.state.winch_fuel_l;
            proof.ticks(&mut g,&mut r,5.,TankInput{fire2:true,..Default::default()},"anchored_winch")?;
            proof.check("rated winch moves same Tiger and spends finite fuel",g.player.pos.distance(before)>0.05&&g.halftrack_runtime.as_ref().unwrap().recovery.state.winch_fuel_l<fuel,json!({"click":click,"metres":g.player.pos.distance(before),"recovery":g.recovery_receipt()}));
            proof.still(&mut g,&mut r,"05_winch_under_load")?;
        }
    }
    // Existing physical climb controller, real Space edges and ordinary update time.
    g.recovery_stow_hands();g.recovery_stow_hands();g.release_crew_body();
    let mount=g.player.hull_model().transform_point3(Vec3::new(1.8,0.,-0.5));let look=g.player.commander_eye();stage_foot(&mut g,&mut r,mount,look);
    g.hatch_open=true;g.hatch_t=1.;g.begin_climb(true,ClimbEnd::IntoTank);
    let moves=g.dbg_complete_climb(&mut r);
    proof.check("ordinary timed climb remounts same Tiger with same commander",g.mode==Mode::Tank&&g.expedition_state().unwrap().controlled_person==Some(person),json!({"committed_climb_moves":moves}));
    proof.still(&mut g,&mut r,"06_remounted")?;
    g.sync_expedition_now()?;
    proof.check("finite roster is unchanged",g.expedition_state().unwrap().people.len()==initial_people,json!({"before":initial_people,"after":g.expedition_state().unwrap().people.len()}));
    let video=proof.clip.as_mut().map(Clip::finish).transpose()?;
    let pass=proof.checks.iter().all(|c|c["pass"]==true);
    let value=json!({"schema":"panzer.recovery-gameplay-review.v2","pass":pass,"checks":proof.checks,"samples":proof.samples,"captures":proof.captures,"video":video,
        "fixtures":["quiet existing expedition camp; enemies disabled","current soldier starting feet/look staged beside interaction surfaces","one existing finite fuel can initially equipped; destination staged0.5L below capacity to represent consumed fuel, no can refill","one named finite vision spare initially in hands, condition0.83","existing person14 moved beside ramp; explicit source-triangle impact, not projectile-flight proof","already-deployed winch and same Tiger relocated into a labelled approach fixture","remount uses existing timed climb controller with synthetic Space edges"],
        "limitations":["does not certify injury probabilities or historical recovery ratings","transport trip is a short continuous loaded drive, not a full combat evacuation or medical admission","portable winch and assisted loading are authored gameplay mechanics","failure checks remain failures; no stage outcome is force-completed"],"final_recovery":g.recovery_receipt(),"final_expedition":g.expedition_status()});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt)?;file.write_all(&serde_json::to_vec_pretty(&value)?)?;
    ensure!(pass,"recovery gameplay review has failed checks; inspect saved receipt");Ok(())
}
