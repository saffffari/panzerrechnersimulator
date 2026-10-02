//! Disclosed eight-second mechanical fixtures on the canonical winter map. No outcome injection.
use super::*;
use anyhow::{ensure,Context};
use serde_json::{json,Value};
use std::{io::Write,path::Path,process::{Child,Command,Stdio}};
const SECONDS:f32=8.0;
struct Encoder(Child);
impl Drop for Encoder {fn drop(&mut self){if !matches!(self.0.try_wait(),Ok(Some(_))) {let _=self.0.kill();let _=self.0.wait();}}}

fn input_for(key:&str,t:f32)->TankInput {
    let mut crew=crate::lan::CrewFrame::default();
    for (index,role) in [crate::lan::LanRole::Commander,crate::lan::LanRole::Gunner,
        crate::lan::LanRole::Driver,crate::lan::LanRole::Loader].into_iter().enumerate() {
        crew.roster[role as usize]=index as u32+1;crew.inputs[role as usize].post=role;
    }
    let driving=match key {"uphill"=>t<7.0,"manual"=>(0.5..6.5).contains(&t),
        _=>(0.5..2.3).contains(&t)||(3.4..6.2).contains(&t)};
    crew.inputs[crate::lan::LanRole::Driver as usize].drive=if driving {if key=="manual" {0.6}else{1.0}}else{0.0};
    if key=="manual" {
        let gunner=&mut crew.inputs[crate::lan::LanRole::Gunner as usize];
        gunner.traverse=if (1.0..3.0).contains(&t){0.45}else if (4.0..6.0).contains(&t){-0.45}else{0.0};
        gunner.elevate=if (1.0..2.0).contains(&t){0.5}else if (4.0..5.0).contains(&t){-0.5}else{0.0};
    }
    if key=="broken" && (t*60.0).round() as u32==390 {
        crew.inputs[crate::lan::LanRole::Driver as usize].flags=crate::lan::F_IGNITION;
    }
    TankInput {coop:Some(crew),coop_host:true,couch_crew:true,..Default::default()}
}

fn terrain_site(g:&Game,course:&crate::canyon::CourseSpec,key:&str)->anyhow::Result<(Vec2,f32,Value)> {
    if key!="uphill" {return Ok((course.start,course.heading,json!({"choice":"authored course start"})));}
    let mut candidates=Vec::new();
    for pair in course.route.windows(2) {
        let length=pair[0].distance(pair[1]);if length<4.0 {continue;}
        let f=(pair[1]-pair[0])/length;let right=Vec2::new(f.y,-f.x);
        for n in 0..(length/5.0).floor() as usize {
            let p=pair[0]+f*(n as f32*5.0);
            let height=|v:Vec2|g.hf.vehicle_height_at(v.x,v.y);
            let pitch=((height(p+f*3.0)-height(p-f*3.0))/6.0).atan();
            let roll=((height(p+right*1.54)-height(p-right*1.54))/3.08).atan();
            if !(5.0..16.0).contains(&pitch.to_degrees()) || roll.abs()>8.0_f32.to_radians() {continue;}
            let mut clear=true;
            for step in 0..=10 {
                let q=p+f*step as f32*4.0;
                let center=Vec3::new(q.x,height(q)+1.2,q.y);
                let local_grade=((height(q+f*3.0)-height(q-f*3.0))/6.0).atan();
                if local_grade.abs()>22.0_f32.to_radians() || q.abs().max_element()>g.act.half_extent_m-12.0 {clear=false;break;}
                if g.occluders.iter().any(|o|center.x>o.min.x-2.4&&center.x<o.max.x+2.4
                    &&center.z>o.min.z-2.4&&center.z<o.max.z+2.4&&center.y>o.min.y-1.5&&center.y<o.max.y+1.5) {
                    clear=false;break;
                }
            }
            if clear {candidates.push(((pitch.to_degrees()-10.0).abs(),p,f.x.atan2(f.y),pitch,roll));}
        }
    }
    candidates.sort_by(|a,b|a.0.total_cmp(&b.0));
    let (_,p,heading,pitch,roll)=candidates.first().copied().context("no source-terrain uphill strip met geometry-only fixture criteria")?;
    Ok((p,heading,json!({"choice":"deterministic authored-route terrain scan; closest10deg ascent",
        "candidate_count":candidates.len(),"pitch_deg":pitch.to_degrees(),"roll_deg":roll.to_degrees(),
        "lookahead_m":40,"scope":"source terrain samples and existing world cover bounds; no landscape edit or outcome search"})))
}
fn sample(g:&Game,t:f32)->Value {
    let p=&g.player_powertrain;
    json!({"at_s":t,"position_m":g.player.pos.to_array(),"heading_rad":g.player.heading,
        "pitch_deg":g.player.pitch.to_degrees(),"roll_deg":g.player.roll.to_degrees(),"speed_mps":g.player.speed,
        "automatic":p.automatic,"gear":p.gear,"rpm":p.rpm,"throttle":p.throttle,"load":p.load,
        "drive_force_n":p.drive_force_n,"resistance_force_n":p.resistance_force_n,
        "shift_remaining_s":p.shift_remaining,"engine_on":g.engine_on,"engine_crank_remaining_s":g.engine_crank_t,
        "broken_track_drive":p.broken_track_drive.map(|d|json!({"attempted_belt_speed_mps":d.belt_speed_mps,"load":d.load})),
        "left_track_functional":g.module_ok(ModuleKind::LeftTrack),"right_track_functional":g.module_ok(ModuleKind::RightTrack),
        "running_gear":g.capture_running_gear(),"gun_mechanism":g.capture_gun_mechanism()})
}

pub fn run(prefix:&str,key:&str,preview:bool)->anyhow::Result<()> {
    ensure!(["uphill","manual","broken","snow"].contains(&key),"case must be uphill, manual, broken or snow");
    let prefix=crate::capture_target(prefix);
    for suffix in [".json",".mp4","_picture.mp4","_soundtrack.wav","_contact.jpg","_0.png","_1.png","_2.png"] {
        ensure!(!Path::new(&format!("{prefix}{suffix}")).exists(),"review output exists: {prefix}{suffix}");
    }
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");std::env::set_var("KESSEL_INTERNAL_RES","800x450");
    let manifest_path=crate::resource::resolve(crate::canyon::V2_MANIFEST)?;
    let manifest=crate::act_manifest::ActManifest::load(&manifest_path)?;
    let course=crate::canyon::from_edits(manifest.edits())?.context("Canyon course missing")?;
    crate::act_manifest::select_runtime_manifest(Some(manifest_path.clone()))?;
    let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
    let(device,queue)=crate::request_device(&adapter);
    let(mut r,mut g)=Game::build_playtest_act(&device,&queue,false,&crate::world::act(1))?;
    g.reset_match(0xD21A_1001,250.0);g.set_playtest_return_fire(false);g.set_defer_rtt_to_render(true);
    let(position,heading,site)=terrain_site(&g,&course,key)?;
    g.player.pos=Vec3::new(position.x,g.hf.vehicle_height_at(position.x,position.y),position.y);
    g.player.heading=heading;g.engine_on=true;g.god=false;
    if key=="uphill" {g.player.speed=5.0;g.player_powertrain.gear=6;g.player_powertrain.rpm=2600.0;}
    if key=="broken" {g.debug_set_module_fault(ModuleKind::LeftTrack,tiger_damage::Fault::Spall);}
    g.stage_capture_crew_view(key!="manual");
    if key=="manual" {
        g.set_lan_role(crate::lan::LanRole::Driver);g.switch_t=0.0;
        ensure!(g.debug_face_driver_control("steering"),"source steering control camera target unavailable");
        g.toggle_hand_torch();
    }
    r.apply_campaign_atmosphere(1,true,DayPhase::Day);r.set_weather_seed(g.act.seed);
    r.set_weather_override(Some(if key=="snow" {0.65}else{0.25}));r.display_dither_pattern=3.0;
    ensure!([r.iw,r.ih]==[800,450],"requested native800x450 was not applied");
    let mut warm=input_for(key,0.0);warm.coop.as_mut().unwrap().inputs[crate::lan::LanRole::Driver as usize].drive=0.0;
    g.update(&warm,1.0/60.0,&mut r);crate::simulation_uploads::drain(&device,&queue)?;
    r.begin_coop_shadow_frame(g.player.center());
    let warmup=crate::converge_headless_screenshot(&mut r,&device,&queue,8);
    r.end_coop_shadow_frame();
    let warmup=warmup?;
    g.begin_audio_capture();
    let initial_snow=g.capture_track_snow_receipt();
    let initial=sample(&g,0.0);let start=g.player.pos;let start_heading=g.player.heading;
    let raw=format!("{prefix}_picture.mp4");let fps=if preview {5}else{30};let stride=60/fps;
    let mut encoder=Encoder(Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-f","rawvideo",
        "-pixel_format","rgba","-video_size","800x450","-framerate",&fps.to_string(),"-i","pipe:0","-an",
        "-vf","scale=iw:ih:in_range=full:out_range=tv:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
        "-c:v","libx264","-threads","2","-preset","fast","-crf","12","-pix_fmt","yuv420p","-movflags","+faststart",&raw])
        .stdin(Stdio::piped()).spawn()?);
    let mut pipe=encoder.0.stdin.take().context("encoder stdin")?;
    let mut samples=Vec::new();let mut pcm=Vec::new();let mut events=Vec::new();let mut pictures=Vec::new();
    let mut downshifts=0;let mut last_gear=g.player_powertrain.gear;let mut active_ticks=0;let mut released_ticks=0;
    let mut max_turret_motion=0.0f32;let mut max_elevation_motion=0.0f32;
    let initial_yaw=g.player.turret_yaw;let initial_pitch=g.player.gun_pitch;
    for tick in 0..480u32 {
        let t=tick as f32/60.0;
        if key=="manual" && [30,210].contains(&tick) {
            let accepted=g.driver_shift_gear(1);events.push(json!({"at_s":t,"kind":"manual_upshift","accepted":accepted}));
        }
        g.update(&input_for(key,t),1.0/60.0,&mut r);
        if tick%8==0 {crate::simulation_uploads::drain(&device,&queue)?;}
        let gear=g.player_powertrain.gear;if gear<last_gear {downshifts+=1;}last_gear=gear;
        let active=g.player_powertrain.broken_track_drive.iter().any(|d|d.load>0.001&&d.belt_speed_mps.abs()>0.005);
        active_ticks+=u32::from(active);released_ticks+=u32::from((2.6..3.2).contains(&t)&&!active);
        max_turret_motion=max_turret_motion.max((g.player.turret_yaw-initial_yaw).abs());
        max_elevation_motion=max_elevation_motion.max((g.player.gun_pitch-initial_pitch).abs());
        pcm.extend(g.render_audio_capture_frames(735)?);samples.push(sample(&g,t+1.0/60.0));
        if tick%stride==0 {
            let rgba=if key=="snow" {
                // Declared side observer: the driver still uses ordinary inputs and source terrain.
                let hull=g.player.hull_model();
                r.camera.pos=hull.transform_point3(Vec3::new(7.0,1.45,-1.8+t*0.35));
                let target=hull.transform_point3(Vec3::new(0.0,0.40,0.0));
                r.camera.set_forward((target-r.camera.pos).normalize());r.camera.fov_y=55.0_f32.to_radians();
                r.optic_mask=0.0;r.reticle=false;r.weather_shelter=0.0;
                g.refresh_track_snow_render(&mut r);
                r.render_to_rgba(&device,&queue,800,450)?
            } else {crate::mechanic_capture::gameplay_native_frame(&mut g,&mut r,&device,&queue)?};
            pipe.write_all(&rgba)?;
            let candidates=[48,270,414].map(|v|v/stride*stride);
            if let Some(index)=candidates.iter().position(|&v|v==tick) {
                let path=format!("{prefix}_{index}.png");image::save_buffer(&path,&rgba,800,450,image::ColorType::Rgba8)?;
                pictures.push(json!({"at_s":t,"path":path}));
            }
        }
    }
    drop(pipe);ensure!(encoder.0.wait()?.success(),"video encoding failed");
    let wav=format!("{prefix}_soundtrack.wav");crate::acoustics::write_wav_stereo(&wav,&pcm,crate::acoustics::SAMPLE_RATE)?;
    let video=format!("{prefix}.mp4");ensure!(Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-i",&raw,"-i",&wav,
        "-map","0:v:0","-map","1:a:0","-c:v","copy","-c:a","aac","-b:a","192k","-movflags","+faststart",&video]).status()?.success(),"audio mux failed");
    let contact=format!("{prefix}_contact.jpg");ensure!(Command::new("ffmpeg").args(["-v","error","-nostdin","-n","-i",&video,
        "-vf","fps=0.5,scale=800:450:flags=neighbor,tile=2x2","-frames:v","1","-q:v","2",&contact]).status()?.success(),"contact sheet failed");
    let mut checks=vec![json!({"name":"ordinary_player_alive","passed":g.player.alive}),
        json!({"name":"native800x450","passed":([r.iw,r.ih]==[800,450])}),
        json!({"name":"physical_translation","passed":g.player.pos.distance(start)>0.2,"distance_m":g.player.pos.distance(start)})];
    if key=="uphill" {checks.push(json!({"name":"automatic_downshift_observed","passed":downshifts>0,"count":downshifts}));}
    if key=="manual" {checks.push(json!({"name":"two_manual_changes_accepted","passed":events.len()==2&&events.iter().all(|e|e["accepted"]==true)}));
        checks.push(json!({"name":"actual_turret_and_elevation_motion","passed":max_turret_motion>0.03&&max_elevation_motion>0.02,
            "turret_rad":max_turret_motion,"elevation_rad":max_elevation_motion}));}
    if key=="broken" {checks.push(json!({"name":"one_track_drives_and_released_source_stops","passed":active_ticks>30&&released_ticks>15
        &&(g.player.heading-start_heading).abs()>0.03&&!g.module_ok(ModuleKind::LeftTrack)&&g.module_ok(ModuleKind::RightTrack),
        "active_ticks":active_ticks,"silent_release_ticks":released_ticks}));
        checks.push(json!({"name":"ordinary_engine_shutoff","passed":!g.engine_on&&g.engine_crank_t<=0.0}));}
    if key=="snow" {
        let snow=g.capture_track_snow_receipt();
        checks.push(json!({"name":"source_track_pickup_and_live_shedding","passed":
            snow["selected_track_nodes"].as_u64().unwrap_or(0)>0
            && snow["emitted_flecks"].as_u64().unwrap_or(0)>0
            && snow["snow_fraction_left_right"][0].as_f64().unwrap_or(0.0)>0.02}));
    }
    let passed=checks.iter().all(|c|c["passed"]==true);
    let mut receipt=json!({"schema":"panzer.winter-drive-review.v1","case":key,"passed":passed,"checks":checks,
        "seconds":SECONDS,"fps":fps,"native_extent":[800,450],"dither":"screen Bayer8, one threshold/native pixel",
        "world_manifest":manifest_path,"site":site,"initial":initial,"events":events,"video":video,"wav":wav,"contact_sheet":contact,"stills":pictures});
    receipt["setup"]=json!({"return_fire":false,"shots_requested":false,"invulnerability":false,
        "initial_rolling_speed_mps":if key=="uphill" {5.0}else{0.0},"initial_gear":if key=="uphill" {6}else{1},
        "initial_left_track_broken":key=="broken","weather_override":if key=="snow" {0.65}else{0.25},"crew":"four disclosed occupied co-op stations",
        "view":if key=="snow" {"disclosed moving exterior side observer"}else if key=="manual" {"Driver looking at source steering wheel"}else{"head-out Commander"},
        "ordinary_handheld_torch_on":key=="manual",
        "scope":"mechanical diagnostic, not a combat encounter; no terrain edits, no forced shift or damage result"});
    receipt["track_snow"]=json!({"initial":initial_snow,"final":g.capture_track_snow_receipt()});
    receipt["samples_60hz"]=json!(samples);receipt["audio"]=json!(g.audio_capture_metadata());
    receipt["warmup"]=json!({"transport_frames":warmup.transport_frames,"exposure_frames":warmup.exposure_frames,"stable":warmup.stable});
    receipt["limitations"]=json!(["Recorded with the ordinary device-free audio graph; does not measure ALSA callback starvation.",
        "Broken-side belt speed is gear/RPM-derived attempted shaft motion, not a solved loose-track chain.",
        "Source clatter timbre/level and clutch/drag dynamics are labeled authored estimates."]);
    std::fs::write(format!("{prefix}.json"),serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(passed,"mechanical observations incomplete; retained receipt identifies the failed checks");Ok(())
}

#[cfg(test)]mod tests {use super::*;
    #[test]fn controller_has_no_shots_and_broken_case_releases_and_restarts_drive(){
        for tick in 0..480 {let t=tick as f32/60.0;for key in ["uphill","manual","broken"] {
            let input=input_for(key,t);assert!(!input.fire&&!input.fire2&&!input.coax&&!input.mg_coax);
            let crew=input.coop.unwrap();assert!(crew.inputs.iter().all(|i|!i.flag(crate::lan::F_FIRE_MAIN)));}}
        let drive=|t|input_for("broken",t).coop.unwrap().inputs[crate::lan::LanRole::Driver as usize].drive;
        assert!(drive(1.0)>0.0&&drive(4.0)>0.0);assert_eq!(drive(2.8),0.0);
        assert!(input_for("broken",6.5).coop.unwrap().inputs[crate::lan::LanRole::Driver as usize].flag(crate::lan::F_IGNITION));
        let ignition_edges=(0..480).filter(|&tick|input_for("broken",tick as f32/60.0)
            .coop.unwrap().inputs[crate::lan::LanRole::Driver as usize].flag(crate::lan::F_IGNITION)).count();
        assert_eq!(ignition_edges,1,"an ignition edge must not become a second crank request");
    }
}
