//! Continuous ordinary-Game route proof. Only the initial grounded foot pose is staged.
//! No waypoint teleport, capsule bypass, held-shell injection, or forced pickup is permitted.
use super::*;
use serde_json::{json, Value};

const DT: f32 = 1.0 / 60.0;

fn pose(game: &Game, renderer: &Renderer, seconds: f32) -> Value {
    json!({"seconds":seconds,"feet_m":game.foot.pos.to_array(),
        "eye_m":renderer.camera.pos.to_array(),"yaw":game.foot.yaw,"pitch":game.foot.pitch,
        "foot_hp":game.foot.hp,"airborne":game.foot.airborne,"crouch":game.foot.crouch_t,
        "on_foot":game.mode==Mode::Foot,"station":format!("{:?}",game.station),
        "cabin_walk_m":game.walk_pos.to_array(),"loader_crouch":game.loader_crouch_t,
        "climbing":game.climb.active,"cupola_open":game.access_hatch_open(4),"carried_loose_index":match game.carry_mesh {
            Some(CarriedMesh::LooseShell(index))=>Some(index),_=>None},
        "active_world_rounds":game.loose.iter().filter(|s|s.active&&s.world_space).count(),
        "tank_alive":game.player.alive,"mission":game.winter_mission_snapshot()})
}

fn mouse_toward(game: &Game, target: Vec3, flat: bool) -> (f32,f32,f32) {
    let delta=target-game.foot.eye();
    let yaw=delta.x.atan2(-delta.z);
    let pitch=if flat {0.0} else {delta.y.atan2(Vec2::new(delta.x,delta.z).length())};
    let error=(yaw-game.foot.yaw).sin().atan2((yaw-game.foot.yaw).cos());
    (error.clamp(-0.055,0.055)/crate::foot::SENS,
        -(pitch-game.foot.pitch).clamp(-0.04,0.04)/crate::foot::SENS,error)
}

fn walk_to(game: &mut Game, renderer: &mut Renderer, target: Vec3, carry: Option<usize>,
    seconds: &mut f32, route: &str, samples: &mut Vec<Value>, distance: &mut f32)
    -> anyhow::Result<()> {
    let initial=Vec2::new(game.foot.pos.x-target.x,game.foot.pos.z-target.z).length();
    let max_ticks=((initial/crate::foot::WALK+12.0)/DT).ceil() as usize;
    let mut best=initial;let mut no_progress=0usize;
    for tick in 0..max_ticks {
        let remaining=Vec2::new(game.foot.pos.x-target.x,game.foot.pos.z-target.z).length();
        if remaining<0.095 {return Ok(());}
        anyhow::ensure!(game.mode==Mode::Foot&&!game.climb.active,"route unexpectedly left ordinary foot movement");
        anyhow::ensure!(game.player.alive&&game.foot.hp>0.0,"actor killed during route");
        if let Some(index)=carry {anyhow::ensure!(game.carry_mesh==Some(CarriedMesh::LooseShell(index)),
            "physical cartridge {index} was lost while walking {route}");}
        let (look_dx,look_dy,yaw_error)=mouse_toward(game,target,true);
        let drive=if yaw_error.abs()<0.17 {1.0}else{0.0};
        let old=game.foot.pos;
        game.update(&TankInput{drive,look_dx,look_dy,mg_coax:carry.is_some(),fire2:carry.is_some(),
            ..Default::default()},DT,renderer);
        *seconds+=DT;
        *distance+=Vec2::new(game.foot.pos.x-old.x,game.foot.pos.z-old.z).length();
        if tick%60==0 {let mut state=pose(game,renderer,*seconds);state["route"]=json!(route);
            state["target_m"]=json!(target.to_array());state["remaining_m"]=json!(remaining);
            state["requested_drive"]=json!(drive);samples.push(state);}
        if remaining<best-0.08 {best=remaining;no_progress=0;}
        else if drive>0.0 {no_progress+=1;}
        anyhow::ensure!(no_progress<180,"continuous route stalled at {} toward {} in {route}; remaining={remaining:.3}m",
            game.foot.pos,target);
    }
    anyhow::bail!("waypoint deadline exceeded in {route}: {} toward {target}",game.foot.pos)
}

fn hold_tick(game: &mut Game, renderer: &mut Renderer, mut input: TankInput,
    index: usize, seconds: &mut f32) -> anyhow::Result<()> {
    input.mg_coax=true; input.fire2=true;
    game.update(&input,DT,renderer); *seconds+=DT;
    anyhow::ensure!(game.carry_mesh==Some(CarriedMesh::LooseShell(index)),
        "physical AP {index} lost during boarding/cabin movement");
    anyhow::ensure!(game.player.alive&&game.foot.hp>0.0,"actor killed during boarding");
    Ok(())
}

fn aim_held(game: &mut Game, renderer: &mut Renderer, target: Vec3, index: usize,
    seconds: &mut f32, crouch: bool) -> anyhow::Result<()> {
    for _ in 0..120 {
        let delta=target-renderer.camera.pos;
        let yaw=delta.x.atan2(-delta.z);
        let pitch=delta.y.atan2(Vec2::new(delta.x,delta.z).length());
        let error=(yaw-renderer.camera.yaw).sin().atan2((yaw-renderer.camera.yaw).cos());
        hold_tick(game,renderer,TankInput{look_dx:error.clamp(-0.055,0.055)/crate::foot::SENS,
            look_dy:-(pitch-renderer.camera.pitch).clamp(-0.04,0.04)/crate::foot::SENS,
            crouch,..Default::default()},index,seconds)?;
        if (renderer.camera.forward()-(target-renderer.camera.pos).normalize_or_zero()).length()<0.005 {
            return Ok(());
        }
    }
    anyhow::bail!("ordinary gaze could not reach boarding/stow target {target}")
}

fn space_climb_held(game: &mut Game, renderer: &mut Renderer, index: usize,
    seconds: &mut f32, name: &str, samples: &mut Vec<Value>) -> anyhow::Result<()> {
    hold_tick(game,renderer,TankInput::default(),index,seconds)?;
    hold_tick(game,renderer,TankInput{jump:true,..Default::default()},index,seconds)?;
    let mut sample=pose(game,renderer,*seconds);sample["route"]=json!(name);samples.push(sample);
    anyhow::ensure!(game.climb.active,"Space did not begin {name} at {}",game.foot.pos);
    for _ in 0..480 {
        hold_tick(game,renderer,TankInput::default(),index,seconds)?;
        if !game.climb.active {
            let mut sample=pose(game,renderer,*seconds);sample["route"]=json!(name);samples.push(sample);
            println!("[winter-route] {name} complete at {}",game.foot.pos);
            return Ok(());
        }
    }
    anyhow::bail!("ordinary {name} climb did not finish")
}

fn cabin_walk_to(game: &mut Game, renderer: &mut Renderer, target: Vec3,
    index: usize, seconds: &mut f32, samples: &mut Vec<Value>) -> anyhow::Result<()> {
    let mut best=f32::MAX;let mut stalled=0;
    for tick in 0..1200 {
        let remaining=Vec2::new(game.walk_pos.x-target.x,game.walk_pos.z-target.z).length();
        if remaining<0.045 {return Ok(());}
        anyhow::ensure!(game.mode==Mode::Tank&&game.station==Station::Loader,"lost walking cabin view");
        let target_world=game.player.turret_model().transform_point3(target);
        let delta=target_world-renderer.camera.pos;
        let yaw=delta.x.atan2(-delta.z);
        let error=(yaw-renderer.camera.yaw).sin().atan2((yaw-renderer.camera.yaw).cos());
        let drive=if error.abs()<0.17 {1.0}else{0.0};
        hold_tick(game,renderer,TankInput{drive,crouch:true,
            look_dx:error.clamp(-0.055,0.055)/crate::foot::SENS,
            look_dy:renderer.camera.pitch.clamp(-0.04,0.04)/crate::foot::SENS,
            ..Default::default()},index,seconds)?;
        if tick%30==0 {let mut sample=pose(game,renderer,*seconds);
            sample["route"]=json!("crouched_cabin_walk");sample["target_local_m"]=json!(target.to_array());samples.push(sample);}
        if remaining<best-0.03 {best=remaining;stalled=0;}else if drive>0.0 {stalled+=1;}
        anyhow::ensure!(stalled<180,"ordinary cabin walk stalled at {} toward {target}",game.walk_pos);
    }
    anyhow::bail!("cabin walking deadline exceeded toward {target}")
}

fn return_board_and_stow(game: &mut Game, renderer: &mut Renderer, index: usize,
    seconds: &mut f32, distance: &mut f32, samples: &mut Vec<Value>, receipt: &mut Value)
    -> anyhow::Result<()> {
    let hull=game.player.hull_model();
    let ground_approach=hull.transform_point3(Vec3::new(-3.1,0.0,2.6));
    walk_to(game,renderer,ground_approach,Some(index),seconds,"tank_climb_approach",samples,distance)?;
    aim_held(game,renderer,game.player.center(),index,seconds,false)?;
    let ground_y=game.foot.pos.y;
    space_climb_held(game,renderer,index,seconds,"ground_to_chassis",samples)?;
    anyhow::ensure!(game.mode==Mode::Foot&&game.foot.pos.y>ground_y+0.3,"first climb did not reach chassis");
    let chassis_y=game.foot.pos.y;
    aim_held(game,renderer,game.player.center(),index,seconds,false)?;
    space_climb_held(game,renderer,index,seconds,"chassis_to_turret",samples)?;
    anyhow::ensure!(game.mode==Mode::Foot&&game.foot.pos.y>chassis_y+0.3,"second climb did not reach turret");
    let cupola=game.access_hatch_center(4);
    let roof_target=cupola-game.player.turret_model().transform_vector3(Vec3::X)*0.72;
    walk_to(game,renderer,roof_target,Some(index),seconds,"roof_to_cupola",samples,distance)?;
    aim_held(game,renderer,cupola,index,seconds,false)?;
    receipt["cupola_target"]=json!(game.space_hatch_target(renderer));
    receipt["before_entry"]=pose(game,renderer,*seconds);
    anyhow::ensure!(game.space_hatch_target(renderer)==Some(4),"open cupola not reachable after real roof walk at {}",game.foot.pos);
    space_climb_held(game,renderer,index,seconds,"cupola_passage",samples)?;
    anyhow::ensure!(game.mode==Mode::Tank&&game.station==Station::Commander,"cupola passage did not reach Commander");
    hold_tick(game,renderer,TankInput{jump:true,..Default::default()},index,seconds)?;
    anyhow::ensure!(game.mode==Mode::Tank&&game.station==Station::Loader,"Space did not descend into cabin");
    receipt["cabin_arrival"]=pose(game,renderer,*seconds);
    let empty=game.winter_mission.as_ref().map(|m|m.empty_cradles.clone()).unwrap_or_default();
    let mut tried=Vec::new();
    // Commander descent leaves the body on the left of the physical breech. Walk behind
    // it before crossing the basket; the collision clamp must remain active throughout.
    cabin_walk_to(game,renderer,Vec3::new(-0.61,0.0,-0.55),index,seconds,samples)?;
    cabin_walk_to(game,renderer,Vec3::new(0.61,0.0,-0.55),index,seconds,samples)?;
    for position in [Vec3::new(0.61,0.0,0.08),Vec3::new(0.61,0.0,-0.55),Vec3::new(-0.61,0.0,-0.55)] {
        cabin_walk_to(game,renderer,position,index,seconds,samples)?;
        for &slot in &empty {
            if !game.slot_taken[slot] {continue;}
            let (frame,p,_)=crate::interior_rig::SHELL_SLOTS[slot];
            let parent=if frame==crate::interior_rig::Frame::Hull {game.player.hull_model()}else{game.player.turret_model()};
            aim_held(game,renderer,parent.transform_point3(p),index,seconds,true)?;
            let selected=game.empty_rack_under_aim(renderer.camera.pos,renderer.camera.forward(),false);
            tried.push(json!({"aimed_slot":slot,"selected_slot":selected,"pose":pose(game,renderer,*seconds)}));
            if let Some(selected)=selected {
                game.update(&TankInput{crouch:true,..Default::default()},DT,renderer);*seconds+=DT;
                receipt["stow_views"]=json!(tried);receipt["stowed_slot"]=json!(selected);
                receipt["after_release"]=pose(game,renderer,*seconds);
                anyhow::ensure!(game.carry_mesh.is_none()&&!game.slot_taken[selected]&&!game.loose[index].active,
                    "ordinary grip release failed finite physical stow");
                anyhow::ensure!(game.winter_mission_snapshot().unwrap()["distinct_stowed_rounds"]==json!(1),
                    "mission did not observe this actual wreck cartridge stow");
                return Ok(());
            }
        }
    }
    receipt["stow_views"]=json!(tried);
    anyhow::bail!("neither mission AP cradle reachable from continuous crouched cabin path")
}

#[test]
#[ignore="GPU and source winter scene; walks the complete staged route and carries real finite wreck ammo"]
fn winter_source_route_walks_rooms_picks_real_ap_and_carries_it_home() -> anyhow::Result<()> {
    let repo=std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_path=std::env::var("KESSEL_WINTER_ROUTE_MANIFEST").unwrap_or_else(|_|
        repo.join("assets/worldgen/workshops/act1-winter-encounter-20260925-staged/manifest.v1.json").to_string_lossy().into());
    let path=std::fs::canonicalize(&manifest_path)?;
    let route_receipt=repo.join("artifacts/authoring/winter-encounter-20260925/receipt.v1.json");
    let document:Value=serde_json::from_slice(&std::fs::read(&route_receipt)?)?;
    let routes=document["routes"].as_array().ok_or_else(||anyhow::anyhow!("missing route receipt"))?;
    anyhow::ensure!(routes.len()==6,"fixture must retain all six continuous route sections");
    let prefix=std::env::var("KESSEL_WINTER_ROUTE_PREFIX").unwrap_or_else(|_|
        repo.join("captures/astra/winter_route_20260925_v1").to_string_lossy().into());
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}.json")).exists(),"use a fresh route review prefix");
    crate::act_manifest::select_runtime_manifest(Some(path.clone()))?;
    let manifest=crate::act_manifest::ActManifest::load(&path)?;
    let act=crate::world::resolve_manifest(crate::world::act(1),&manifest)?;
    let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let (mut renderer,mut game)=Game::build_playtest_act(&device,&queue,false,&act)?;
    game.playtest_setup();game.debug_hold_enemy_fire();game.mute_audio();
    game.set_defer_rtt_to_render(true);
    // The ordinary mission initializer selects actual available AP cradles; no test hand/rack
    // completion is injected. Safe practice separates geometry from the live AI regression.
    let boarding=std::env::var_os("KESSEL_WINTER_ROUTE_BOARDING").is_some();
    if boarding {
        // Disclosed departure setup: work the real Commander hatch control, allow its
        // physical opening, then stage the sole starting Foot pose beside the tank.
        game.jump_station(Station::Commander);
        game.update(&TankInput{hatch_toggle:true,..Default::default()},DT,&mut renderer);
        for _ in 0..90 {game.update(&TankInput::default(),DT,&mut renderer);}
        anyhow::ensure!(game.access_hatch_open(4),"departure setup could not open cupola");
    }
    game.begin_winter_mission();
    game.mode=Mode::Foot;
    game.foot.pos=Vec3::new(1853.8,game.hf.height_at(1853.8,248.0),248.0);
    game.foot.airborne=false;game.foot.vel_y=0.0;
    game.foot.yaw=0.0;game.foot.pitch=0.0;
    renderer.camera.pos=game.foot.eye();
    game.update(&TankInput::default(),DT,&mut renderer);
    let initial=pose(&game,&renderer,0.0);
    let initial_rounds=game.loose.iter().filter(|s|s.active&&s.world_space).count();
    let initial_pistol=game.foot.total_p08_rounds();
    let mut seconds=0.;let mut walked=0.;let mut samples=Vec::new();let mut sections=Vec::new();
    let mut pickup:Option<Value>=None;let mut carried=None;let mut captures=Vec::new();
    let mut boarding_receipt=json!({"requested":boarding,"passed":false});
    let mut route_passed=false;
    let result=(||->anyhow::Result<()> {
        for section in routes {
            let name=section["id"].as_str().unwrap();
            let points=section["points_m"].as_array().unwrap();
            for point in points {
                let p=point.as_array().unwrap();
                walk_to(&mut game,&mut renderer,Vec3::new(p[0].as_f64().unwrap() as f32,
                    p[1].as_f64().unwrap() as f32,p[2].as_f64().unwrap() as f32),carried,
                    &mut seconds,name,&mut samples,&mut walked)?;
                anyhow::ensure!(!game.foot.airborne
                    &&(game.foot.pos.y-p[1].as_f64().unwrap() as f32).abs()<0.31,
                    "authored route waypoint reached wrong support height in {name}: feet={} expected_y={}",
                    game.foot.pos,p[1]);
            }
            sections.push(json!({"id":name,"completed_s":seconds,"end":pose(&game,&renderer,seconds)}));
            println!("[winter-route] {name} reached in {seconds:.2}s at {}",game.foot.pos);
            if name=="house_to_wreck" {
                let (index,target)=game.loose.iter().enumerate().filter(|(_,s)|s.active&&s.world_space&&!s.nature_he
                    &&Vec2::new(s.pos.x-1907.0,s.pos.z-218.0).length()<12.0)
                    .min_by(|(_,a),(_,b)|a.pos.distance_squared(game.foot.pos).total_cmp(&b.pos.distance_squared(game.foot.pos)))
                    .map(|(i,s)|(i,s.pos)).ok_or_else(||anyhow::anyhow!("no finite AP body beside the actual wreck"))?;
                for _ in 0..90 {
                    let(dx,dy,_) = mouse_toward(&game,target,false);
                    game.update(&TankInput{look_dx:dx,look_dy:dy,..Default::default()},DT,&mut renderer);seconds+=DT;
                    if (renderer.camera.forward()-(target-renderer.camera.pos).normalize()).length()<0.005 {break;}
                }
                let before=pose(&game,&renderer,seconds);
                game.update(&TankInput{mg_coax:true,fire2:true,..Default::default()},DT,&mut renderer);seconds+=DT;
                // The ordinary ray can meet a neighbouring cartridge's visible body before
                // the aimed centre in this close-packed stack. Retain its actual identity;
                // selecting a specific array index is not the player's interaction contract.
                let actual=match game.carry_mesh {
                    Some(CarriedMesh::LooseShell(i))=>Some(i),_=>None};
                pickup=Some(json!({"aimed_index":index,"aimed_position_m":target.to_array(),
                    "actual_index":actual,"actual_source_position_m":actual.map(|i|game.loose[i].pos.to_array()),
                    "before":before,"after":pose(&game,&renderer,seconds),"ordinary_lmb":true}));
                let actual=actual.ok_or_else(||anyhow::anyhow!(
                    "no physical AP was picked up by LMB at {} aiming {target}",game.foot.pos))?;
                let source=&game.loose[actual];
                anyhow::ensure!(source.world_space&&!source.nature_he
                    &&Vec2::new(source.pos.x-1907.0,source.pos.z-218.0).length()<12.0,
                    "ordinary LMB did not select an AP cartridge belonging to the nearby wreck");
                anyhow::ensure!(!source.active,"picked source cartridge duplicated in world");
                carried=Some(actual);
            }
            renderer.reset_history();let capture=format!("{prefix}_{name}.png");
            renderer.render_to_png(&device,&queue,&capture,1280,720)?;captures.push(capture);
        }
        let index=carried.ok_or_else(||anyhow::anyhow!("route completed without physical salvage"))?;
        anyhow::ensure!(game.carry_mesh==Some(CarriedMesh::LooseShell(index))&&!game.loose[index].active,
            "same physical cartridge did not reach the tank");
        anyhow::ensure!(game.loose.iter().filter(|s|s.active&&s.world_space).count()==initial_rounds-1,
            "finite world ammunition accounting changed unexpectedly");
        anyhow::ensure!(game.foot.total_p08_rounds()==initial_pistol,"carry input discharged the sidearm");
        route_passed=true;
        if boarding {
            return_board_and_stow(&mut game,&mut renderer,index,&mut seconds,&mut walked,
                &mut samples,&mut boarding_receipt)?;
            boarding_receipt["passed"]=json!(true);
            renderer.reset_history();let capture=format!("{prefix}_stowed.png");
            renderer.render_to_png(&device,&queue,&capture,1280,720)?;captures.push(capture);
        }
        Ok(())
    })();
    if result.is_err() {
        renderer.reset_history();let capture=format!("{prefix}_blocked.png");
        renderer.render_to_png(&device,&queue,&capture,1280,720)?;captures.push(capture);
    }
    let receipt=json!({"schema":"kessel.continuous-winter-route.v1","manifest":path,"route_receipt":route_receipt,
        "adapter":adapter.get_info().name,"passed":result.is_ok(),"error":result.as_ref().err().map(ToString::to_string),
        "scope":"Offscreen actual Game update, initial grounded Foot pose staged once at1853.8,248. Ordinary heading input and drive thereafter, real collisions retained. Practice targets hold fire; audio muted; this is not native OS input, live AI combat or perceptual sound review. Optional boarding begins with cupola opened by normal hatch control before initial Foot staging; see boarding_stow result.",
        "initial":initial,"final":pose(&game,&renderer,seconds),"simulation_seconds":seconds,"horizontal_distance_m":walked,
        "sections_completed":sections,"pickup":pickup,"same_cartridge_carried_home":route_passed,
        "initial_world_rounds":initial_rounds,"position_writes_after_initial_staging":0,
        "samples":samples,"captures":captures,"boarding_stow":boarding_receipt});
    std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&receipt)?)?;
    result
}
