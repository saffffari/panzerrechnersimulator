//! Windowless geography, ordinary travel and mission-transaction review. Explicit inspection
//! cameras and a route-following input driver are fixtures, never reported as human playtesting.
use super::*;
use anyhow::{ensure,Context,Result};
use serde_json::json;
use std::{path::Path,io::Write};
use std::process::{Child,Command,Stdio};

/// Actual rendered simulation samples. Start/end excerpts retain their source timestamps in
/// the receipt; this is neither a slideshow nor a claim of uninterrupted human play.
struct TravelClip {process:Child,frames:Vec<f32>,width:u32,height:u32}
impl TravelClip {
 fn start(path:&str,width:u32,height:u32)->Result<Self>{
  let process=Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-f","rawvideo","-pixel_format","rgba","-video_size",&format!("{width}x{height}"),"-framerate","12","-i","pipe:0","-an","-c:v","libx264","-threads","2","-preset","fast","-crf","17","-pix_fmt","yuv420p","-movflags","+faststart",path]).stdin(Stdio::piped()).spawn()?;
  Ok(Self{process,frames:Vec::new(),width,height})
 }
 fn sample(&mut self,time:f32,r:&mut Renderer,d:&wgpu::Device,q:&wgpu::Queue)->Result<()>{
  let rgba=r.render_to_rgba(d,q,self.width,self.height)?;
  self.process.stdin.as_mut().unwrap().write_all(&rgba)?;self.frames.push(time);Ok(())
 }
 fn finish(&mut self)->Result<()>{drop(self.process.stdin.take());ensure!(self.process.wait()?.success(),"travel clip encoding failed");Ok(())}
}
impl Drop for TravelClip {fn drop(&mut self){if !matches!(self.process.try_wait(),Ok(Some(_))){let _=self.process.kill();let _=self.process.wait();}}}

pub fn run(requested:&str,seconds:f32,autonomous_gunner:bool)->Result<()> {
    ensure!(seconds.is_finite()&&(0.0..=3600.).contains(&seconds),"review duration must be 0Ã¢â‚¬â€œ3600 seconds");
    let prefix=crate::capture_target(requested);let receipt=format!("{prefix}.json");
    ensure!(!Path::new(&receipt).exists(),"review prefix exists");
    let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    crate::act_manifest::select_runtime_manifest(Some(crate::expedition_theatre::MANIFEST.into()))?;
    let (mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
    g.mute_audio();let camp=g.expedition_initial_camp_position()?;g.enable_expedition(1944,camp)?;g.enable_expedition_theatre(&mut r)?;g.debug_begin_crew_battle_audit();
    for _ in 0..30 {g.update(&TankInput::default(),1./60.,&mut r);}
    let mut checks=Vec::new();let mut captures=Vec::new();
    let mut check=|name:&str,pass:bool,evidence:serde_json::Value|{println!("[theatre] {name}: {}",if pass{"PASS"}else{"FAIL"});checks.push(json!({"check":name,"pass":pass,"evidence":evidence}));};
    let t=g.theatre.as_ref().unwrap();
    check("one continuous theatre and retained finite roster",t.spec.extent_m==12000.&&t.sites.len()==12&&g.expedition_state().unwrap().people.len()==16,g.expedition_theatre_receipt().unwrap());
    check("all ammunition rewards own physical finite carts",t.ammo_carts.len()>=6,json!({"carts":t.ammo_carts,"cartridges":t.ammo_carts.iter().map(|c|c.remaining.iter().filter(|r|**r).count()).sum::<usize>()}));
    check("two existing reserves are field survivors",[14,15].iter().all(|id|g.expedition_state().unwrap().person(crate::expedition::PersonId(*id)).is_some_and(|p|matches!(p.location,crate::expedition::PersonLocation::OnFoot{..}))),json!(g.expedition_status()));
    let saved=r.camera;
    let photo=|r:&mut Renderer,name:&str,captures:&mut Vec<serde_json::Value>|->Result<()> {
        r.ui_reset();r.reset_history();let convergence=crate::converge_headless_screenshot(r,&device,&queue,24)?;
        let path=format!("{prefix}_{name}.png");r.render_to_png(&device,&queue,&path,1280,720)?;
        captures.push(json!({"path":path,"inspection_camera":true,"eye":r.camera.pos.to_array(),"converged":convergence.stable}));Ok(())
    };
    for (name,p) in [("support_camp",camp),("west_yard",Vec3::new(-3400.,g.hf.height_at(-3400.,2200.),2200.)),("east_village",Vec3::new(2900.,g.hf.height_at(2900.,1700.),1700.)),("north_relief",Vec3::new(-1500.,g.hf.height_at(-1500.,-3500.),-3500.))] {
        r.camera.pos=p+Vec3::new(35.,20.,-40.);r.camera.set_forward(p+Vec3::Y*2.-r.camera.pos);r.camera.fov_y=68f32.to_radians();photo(&mut r,name,&mut captures)?;
    }
    for (i,b) in g.theatre.as_ref().unwrap().crossings.iter().enumerate(){let p=Vec3::new(b.center.x,b.deck_y,b.center.y);r.camera.pos=p+Vec3::new(60.,20.,-45.);r.camera.set_forward(p-r.camera.pos);photo(&mut r,&format!("bridge_{i}"),&mut captures)?;}
    r.camera=saved;g.dbg_cupola_view(0.,-0.15);g.update(&TankInput::default(),0.,&mut r);
    g.set_map_held(true);ensure!(g.toggle_hand_flashlight(),"physical reading torch unavailable");
    g.update(&TankInput::default(),1./60.,&mut r);
    ensure!(g.prepare_held_map(&mut r,true),"map failed to open");r.ui_reset();g.draw_expedition_map(&mut r);
    r.render_held_map_texture(&device,&queue,g.held_map_slot());
    photo(&mut r,"physical_map",&mut captures)?;
    r.debug_dump_held_map_ink(&device,&queue,&format!("{prefix}_map_ink.png"))?;
    r.debug_dump_manual_albedo(&device,&queue,&format!("{prefix}_map_albedo.png"))?;
    let lighting=r.debug_manual_lighting(&device,&queue)?;
    check("physical map opens detailed surveyed area",g.expedition_map_click(Vec2::new((0.72-0.17)/0.66,(0.375-0.34)/0.59))&&g.theatre.as_ref().unwrap().map_zone==Some(10),json!({"area":g.theatre.as_ref().unwrap().map_zone,"extent_m":1200}));
    r.ui_reset();g.draw_expedition_map(&mut r);r.render_held_map_texture(&device,&queue,g.held_map_slot());photo(&mut r,"area_map",&mut captures)?;
    // This is the same normalized paper cursor consumed by a live LMB press.
    let slots=g.expedition_state().unwrap().preparation_slots;
    check("paper button accepts exactly one dispatch",g.expedition_map_click(Vec2::new(0.5,(0.89-0.34)/0.59))&&g.expedition_state().unwrap().preparation_slots+1==slots,json!(g.expedition_theatre_receipt()));
    check("duplicate Enter does not spend another dispatch",g.expedition_accept_order().is_err()&&g.expedition_state().unwrap().preparation_slots+1==slots,json!(g.expedition_status()));
    g.set_map_held(false);g.prepare_held_map(&mut r,false);g.toggle_hand_flashlight();
    let initial=g.player.pos;let started=std::time::Instant::now();let mut samples=Vec::new();let mut max_roll=0f32;let mut nonfinite=false;
    // Ordinary commander cruise/steer/ignition inputs, aimed down the first authored route.
    // No invulnerability, combat resets, AI holds, speed boosts or outcome injection.
    g.command_driver_cruise(1);g.command_driver_cruise(1);
    let road=g.theatre.as_ref().unwrap().spec.roads[0].clone();let mut waypoint=1usize;
    let mut combat_stop=false;let mut simulated=0f32;let mut combat_checkpoints=Vec::new();
    let mut next_firing_halt=0f32;let mut firing_halt_until=0f32;
    let video_path=format!("{prefix}_travel_excerpts.mp4");
    let mut video=if seconds>0.{Some(TravelClip::start(&video_path,r.iw,r.ih)?)}else{None};
    let travel_result=(||->Result<()> {
    for tick in 0..(seconds*60.)as usize {
        let target=Vec2::from_array(road[waypoint.min(road.len()-1)]);let here=Vec2::new(g.player.pos.x,g.player.pos.z);
        // The order/map supplies the objective location. Stop inside its marked occupation
        // area to fight and hold it; blindly driving through cannot exercise a mission.
        let holding_order=g.theatre.as_ref().and_then(|t|t.active.and_then(|id|t.spec.sites.iter().find(|s|s.id==id)))
            .is_some_and(|site|here.distance(Vec2::from_array(site.position))<80.);
        if here.distance(target)<30.&&!holding_order&&waypoint+1<road.len(){waypoint+=1;}
        let toward=(target-here).normalize_or_zero();let heading=toward.x.atan2(toward.y);
        let error=(heading-g.player.heading+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
        // Optional explicit K experiment. The ordinary gunner's own perception selects its
        // target; the test driver has no enemy-position oracle and grants no protection.
        if autonomous_gunner {
            let at=tick as f32/60.;
            let contact=g.crew_solo.gunner_target.is_some();
            // A sighted tank is not a reason to remain parked indefinitely. The scripted
            // driver gives the gunner eight seconds to lay, then moves for twelve seconds
            // before considering another halt. Ordinary cruise brakes/acceleration apply.
            if contact&&at>=next_firing_halt {firing_halt_until=at+8.;next_firing_halt=at+20.;}
            let halt=holding_order||at<firing_halt_until;
            if halt!=combat_stop {
                for _ in 0..2 {g.command_driver_cruise(if halt{-1}else{1});}
                combat_stop=halt;
            }
        }
        let input=TankInput{steer:if combat_stop{0.}else{(error*1.5).clamp(-1.,1.)},
            fire_order:autonomous_gunner&&tick==0&&!g.crew_solo.fire_at_will,
            ignition:tick==0&&!g.engine_on,..Default::default()};
        g.debug_crew_battle_clock(tick as u64,(tick+1) as f64/60.);
        g.update(&input,1./60.,&mut r);crate::simulation_uploads::drain(&device,&queue)?;
        simulated=(tick+1)as f32/60.;
        let at=tick as f32/60.;
        if tick%5==0 {
            if at<10.||at>=seconds-10. {
                if let Some(v)=video.as_mut(){v.sample(at,&mut r,&device,&queue)?;}
            } else {
                r.render_offscreen_frame(&device,&queue)?;
            }
        }
        max_roll=max_roll.max(g.player.roll.abs());nonfinite|=!g.player.pos.is_finite()||g.enemies.iter().any(|e|!e.pos.is_finite());
        if tick%600==0 {g.sync_expedition_now()?;samples.push(json!({"seconds":tick as f32/60.,"position":g.player.pos.to_array(),"alive":g.controlled_soldier_alive(),"speed":g.player.speed,"status":g.expedition_status(),"phase":g.expedition_theatre_receipt()}));println!("[theatre] ordinary travel {:.0}/{seconds}s, {:.0}m",tick as f32/60.,g.player.pos.distance(initial));}
        if autonomous_gunner&&(tick+1)%18000==0 {
            let snapshot=g.export_expedition_world(&r,g.expedition_world_recipe()?)
                .with_context(||format!("capture battlefield at {simulated:.1}s"))?;
            let path=format!("{prefix}_checkpoint_{}s.pzexp",(tick+1)/60);
            crate::expedition_world_save::save(Path::new(&path),&snapshot)?;
            let loaded=crate::expedition_world_save::load(Path::new(&path))?;
            let exact=loaded.owners.iter().zip(&snapshot.owners).all(|(a,b)|a.key==b.key&&a.payload==b.payload);
            ensure!(exact,"combat checkpoint disk read changed owner state");
            let owner_sizes=snapshot.owners.iter().map(|o|json!({"owner":o.key,"bytes":o.payload.len()})).collect::<Vec<_>>();
            combat_checkpoints.push(json!({"path":path,"seconds":simulated,"owners":snapshot.owners.len(),"owner_sizes":owner_sizes,"file_bytes":std::fs::metadata(&path)?.len(),"exact_disk_roundtrip":exact}));
        }
        if g.expedition_succession_pending() {
            if let Some(v)=video.as_mut(){v.sample(at,&mut r,&device,&queue)?;}
            photo(&mut r,"relief_required",&mut captures)?;break;
        }
    } Ok(()) })();
    let travel_error=travel_result.err().map(|e|format!("{e:#}"));
    let video_error=video.as_mut().and_then(|v|v.finish().err()).map(|e|format!("{e:#}"));
    check("ordinary run and presentation completed without error",travel_error.is_none()&&video_error.is_none(),
        json!({"travel_error":travel_error,"video_error":video_error,"presentation_fps":12}));
    check("requested ordinary simulation duration completed",simulated+1./60.>=seconds,
        json!({"requested_seconds":seconds,"simulated_seconds":simulated,"relief_required":g.expedition_succession_pending()}));
    check("ordinary simulation remains finite",!nonfinite,json!({"wall_seconds":started.elapsed().as_secs_f64(),"travel_m":g.player.pos.distance(initial),"max_roll_rad":max_roll,"samples":samples}));
    if seconds>=30. {check("cruise produces meaningful travel",g.player.pos.distance(initial)>10.,json!({"start":initial.to_array(),"end":g.player.pos.to_array(),"engine":g.engine_on,"driver":g.crew_solo_receipt()}));}
    let pass=checks.iter().all(|c|c["pass"]==true);
    let out=json!({"schema":"kessel.expedition-theatre-review.v2","pass":pass,"checks":checks,"captures":captures,"map_lighting":lighting,
        "requested_seconds":seconds,"simulated_seconds":simulated,"autonomous_gunner_experiment":autonomous_gunner,"combat_checkpoints":combat_checkpoints,"travel_error":travel_error,"video_error":video_error,
        "terminal_audit":g.debug_crew_battle_snapshot(),"small_arms_audit":g.capture_small_arms_receipt(),
        "final_crew":g.crew_solo_receipt(),"final_pose":json!({"mode":format!("{:?}",g.mode),"station":format!("{:?}",g.station),"head_out":g.commander_outside,"hatch_open":g.hatch_open,"foot_hp":g.foot.hp,"fire":g.fire,"vehicle_alive":g.player.alive,"death_cause":format!("{:?}",g.death_cause)}),
        "final_modules":g.modules.iter().map(|m|json!({"kind":format!("{:?}",m.kind),"functional":m.functional,"fault":format!("{:?}",m.fault)})).collect::<Vec<_>>(),
        "video":video.as_ref().map(|v|json!({"path":video_path,"fps":12,"audio":false,"simulation_timestamps":v.frames,"scope":"Real start/end excerpts; omitted middle interval is not time-compressed footage."})),"scope":"Source geography/physical map, finite depot stocks, real order click transaction and scripted ordinary travel. Optional autonomous gunner uses the existing player-selectable K experiment and normal gunner perception. Checkpoints prove disk serialization, not fresh-world continuation (separate world review). Inspection views are explicitly staged cameras. This is not independent human feedback or a mission victory proof."});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt)?;file.write_all(&serde_json::to_vec_pretty(&out)?)?;
    ensure!(pass,"theatre review failed; inspect receipt");Ok(())
}
