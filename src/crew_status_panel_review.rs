//! Windowless review of the actual fictional crew-status panel in the source Tiger.
//! Health fixtures use the real casualty APIs; only explicitly labelled close inspections move
//! the camera. Wide views retain the ordinary closed-hatch commander camera and source units.
use super::*;
use anyhow::{ensure,Context};
use serde_json::{json,Value};
use std::{io::Write,path::Path,process::{Command,Stdio}};
#[cfg(windows)]use std::os::windows::process::CommandExt;

const DT:f32=1.0/60.0;

fn advance(g:&mut Game,r:&mut Renderer,seconds:f32) {
    for _ in 0..(seconds/DT).ceil() as usize {g.update(&TankInput::default(),DT,r);}
}
fn projected(camera:&crate::camera::Camera,point:Vec3)->Option<[f32;2]> {
    let clip=camera.relative_view_proj()*(point-camera.pos).extend(1.0);
    if !clip.is_finite()||clip.w<=0.0 {return None;}
    let ndc=clip.truncate()/clip.w;
    (ndc.z>=0.0&&ndc.z<=1.0).then_some([ndc.x*0.5+0.5,0.5-ndc.y*0.5])
}
fn seat_for_actor(panel:&Value,actor:u32)->Option<&Value> {
    panel["seats"].as_array()?.iter().find(|seat|seat["actor_id"].as_u64()==Some(actor as u64))
}
struct Proof<'a> {
    prefix:String,device:&'a wgpu::Device,queue:&'a wgpu::Queue,
    checks:Vec<Value>,captures:Vec<Value>,
}
impl Proof<'_> {
    fn check(&mut self,label:&str,pass:bool,detail:Value) {
        println!("[crew-panel] {label}: {}",if pass {"PASS"}else{"FAIL"});
        self.checks.push(json!({"check":label,"pass":pass,"detail":detail}));
    }
    fn still(&mut self,g:&mut Game,r:&mut Renderer,name:&str,close:bool)->anyhow::Result<()> {
        // Let normal gameplay place the camera, panel and lamps. No UI overlay is drawn.
        g.update(&TankInput::default(),0.0,r);
        let ordinary=r.camera;
        let (center,normal)=g.crew_status_panel_inspection_pose();
        ensure!(center.is_finite()&&normal.is_finite()&&normal.length_squared()>0.9,"invalid physical panel inspection pose");
        let screen=projected(&ordinary,center);
        if !close {
            self.check(&format!("{name}: normal commander panel center is bottom-left"),
                screen.is_some_and(|p|p[0]>=0.0&&p[0]<0.5&&p[1]>0.5&&p[1]<=1.0),
                json!({"normalized_xy":screen,"world_center_m":center.to_array(),
                    "camera_world_m":ordinary.pos.to_array(),"camera_forward":ordinary.forward().to_array(),
                    "camera_fov_y_rad":ordinary.fov_y,"camera_override":false}));
        }else {
            // Diagnostic camera only, not a claim that the player's head can occupy this point.
            r.camera.pos=center+normal.normalize()*0.40;
            r.camera.set_forward(center-r.camera.pos);
            r.camera.fov_y=45.0_f32.to_radians();
        }
        r.ui_reset();r.reset_history();
        let warmup=crate::converge_headless_screenshot(r,self.device,self.queue,24)?;
        self.check(&format!("{name}: ordinary eye adaptation converged"),warmup.stable,
            json!({"transport_frames":warmup.transport_frames,"exposure_frames":warmup.exposure_frames}));
        let path=format!("{}_{name}.png",self.prefix);
        r.render_to_png(self.device,self.queue,&path,1280,720)?;
        self.captures.push(json!({"name":name,"path":path,"close_diagnostic_camera":close,
            "camera_world_m":r.camera.pos.to_array(),"camera_forward":r.camera.forward().to_array(),
            "ordinary_camera_world_m":ordinary.pos.to_array(),"ordinary_panel_projection":screen,
            "panel":g.crew_status_panel_receipt(),"crew":g.crew_solo_receipt(),
            "photometry":r.debug_frame_photometry(self.device,self.queue)?.to_json()}));
        r.camera=ordinary;
        Ok(())
    }
    fn tpz_still(&mut self,g:&mut Game,r:&mut Renderer)->anyhow::Result<()> {
        g.dbg_cupola_view(0.0,0.0);g.update(&TankInput::default(),0.0,r);
        let ordinary=r.camera;
        // Exact reference framing from raymarch_showcase::tpz_lamp: the existing gunner eye,
        // aimed at the actual TPZ needle, 40-degree FOV. No instrument transform or fill changes.
        let (eye,_,_)=g.gunner_interior_camera();
        let dial=g.player.turret_model().transform_point3(crate::interior_rig::TPZ_NEEDLE_POS);
        r.camera.pos=eye;r.camera.set_forward(dial-eye);r.camera.fov_y=40f32.to_radians();
        r.optic_mask=0.0;r.reticle=false;g.drive_lights(r);r.ui_reset();r.reset_history();
        let warmup=crate::converge_headless_screenshot(r,self.device,self.queue,24)?;
        self.check("TPZ: ordinary eye adaptation converged",warmup.stable,
            json!({"transport_frames":warmup.transport_frames,"exposure_frames":warmup.exposure_frames}));
        let path=format!("{}_night_tpz_gunner_eye.png",self.prefix);
        r.render_to_png(self.device,self.queue,&path,1280,720)?;
        self.captures.push(json!({"name":"night_tpz_gunner_eye","path":path,
            "close_diagnostic_camera":true,"reference":"raymarch_showcase tpz_lamp: actual gunner eye, 40-degree FOV",
            "camera_world_m":eye.to_array(),"target_world_m":dial.to_array(),
            "tpz_source_intensity":crate::interior_rig::TPZ_ILLUMINATOR_INTENSITY,
            "source_color_linear":[1.0,0.34,0.035],
            "photometry":r.debug_frame_photometry(self.device,self.queue)?.to_json()}));
        r.camera=ordinary;Ok(())
    }
    fn video(&mut self,g:&mut Game,r:&mut Renderer,wounded:u32)->anyhow::Result<Value> {
        const FPS:u32=24;const FRAMES:u32=FPS*6;
        let path=format!("{}_pulse_and_selection.mp4",self.prefix);
        let mut command=Command::new("ffmpeg");
        command.args(["-hide_banner","-loglevel","error","-n","-f","rawvideo","-pixel_format","rgba",
            "-video_size","1280x720","-framerate","24","-i","-","-an","-c:v","libx264",
            "-preset","fast","-crf","18","-pix_fmt","yuv420p",&path]).stdin(Stdio::piped());
        #[cfg(windows)]command.creation_flags(0x0800_0000);
        let mut encoder=command.spawn().context("start crew-panel video encoder")?;
        let mut pipe=encoder.stdin.take().context("crew-panel encoder stdin")?;
        g.cancel_crew_panel_selection();g.dbg_cupola_view(0.0,0.0);g.update(&TankInput::default(),0.0,r);
        r.reset_history();crate::converge_headless_screenshot(r,self.device,self.queue,24)?;
        let source_index=1usize;let destination_index=4usize; // living gunner -> hover dead driver; no assignment request
        let target=|g:&Game,index:usize|->anyhow::Result<Vec3> {
            let value=g.crew_status_panel_receipt();
            let xyz=value["seats"][index]["lens_turret_local"].as_array().context("panel lens position missing")?;
            ensure!(xyz.len()==3,"invalid panel lens position");
            let p=Vec3::new(xyz[0].as_f64().context("lens x")? as f32,
                xyz[1].as_f64().context("lens y")? as f32,xyz[2].as_f64().context("lens z")? as f32);
            Ok(g.player.turret_model().transform_point3(p))
        };
        let mut samples=Vec::new();let mut clicked=false;let mut picked=None;
        let mut amber_on=false;let mut amber_off=false;let mut wire_seen=false;
        let start_time=g.crew_solo.panel_clock_s();
        let capture=(||->anyhow::Result<()> {
            for frame in 0..FRAMES {
                // Exact rational 60Hz ->24fps cadence (2,3 alternating), with no accelerated time.
                for _ in (frame*60/FPS)..((frame+1)*60/FPS) {g.update(&TankInput::default(),DT,r);}
                if frame==FPS {
                    let point=target(g,source_index)?;face_commander_at(g,r,point);
                    picked=g.crew_status_panel_hover_index(r.camera.pos,r.camera.forward());
                    if picked==Some(source_index) {clicked=g.crew_status_panel_click(r.camera.pos,r.camera.forward());}
                }
                if frame==FPS+FPS/2 {let point=target(g,destination_index)?;face_commander_at(g,r,point);}
                let close=frame>=FPS*3;
                if close {
                    let (center,normal)=g.crew_status_panel_inspection_pose();
                    r.camera.pos=center+normal*0.40;
                    r.camera.set_forward(target(g,destination_index)?-r.camera.pos);
                    r.camera.fov_y=45f32.to_radians();
                }
                // The exact same live callback recomputes finite source power and source-shadow
                // registrations after a labelled diagnostic camera change; no visual test colors.
                g.drive_lights(r);
                if frame==FPS || frame==FPS*3 {r.reset_history();}
                let states=g.crew_status_panel_states();
                let selected=g.crew_panel_selected.and_then(|a|states.iter().position(|s|s.actor_id==Some(a)));
                let hovered=g.crew_status_panel_hover_index(r.camera.pos,r.camera.forward());
                let panel=g.crew_status_panel.receipt(&states,g.crew_solo.panel_clock_s(),
                    if g.cabin_lights {1.0}else{0.0},selected,hovered);
                if let Some(pulse)=seat_for_actor(&panel,wounded).and_then(|s|s["pulse"].as_bool()) {
                    amber_on|=pulse;amber_off|=!pulse;
                }
                wire_seen|=selected.zip(hovered).is_some_and(|(a,b)|a!=b);
                r.ui_reset();
                let label=if close {"DIAGNOSTIC CLOSE CAMERA | SAME LIVE CREW AND LIGHTS"}
                    else if frame>=FPS {"ORDINARY COMMANDER HEAD LOOK | LMB SELECT, HOVER ONLY"}
                    else {"ORDINARY COMMANDER VIEW | ACTUAL WOUNDED / DEAD CREW"};
                r.draw_text(label,0.02,0.025,0.021,[1.0,1.0,1.0,1.0],crate::ui::Align::Left);
                let pixels=r.render_to_rgba_with_hud(self.device,self.queue,1280,720)?;
                pipe.write_all(&pixels)?;
                if frame%6==0 || frame==FPS || frame+1==FRAMES {
                    samples.push(json!({"frame":frame,"playback_seconds":frame as f32/FPS as f32,
                        "simulation_seconds":g.crew_solo.panel_clock_s()-start_time,"diagnostic_close_camera":close,
                        "camera_world_m":r.camera.pos.to_array(),"camera_forward":r.camera.forward().to_array(),"panel":panel}));
                }
            }Ok(())
        })();
        drop(pipe);
        if capture.is_err() {let _=encoder.kill();}
        let encoded=encoder.wait().context("wait for crew-panel encoder")?.success();
        capture?;
        self.check("video: real commander-eye LMB selected living gunner",clicked&&g.crew_panel_selected.is_some(),
            json!({"picked_seat_index":picked,"click_consumed":clicked,"selected_actor":g.crew_panel_selected,"transfer_requested":false}));
        self.check("video: actual wounded clock showed both pulse phases",amber_on&&amber_off,json!({"on":amber_on,"off":amber_off}));
        self.check("video: source-occluded selected-to-hover wire was active",wire_seen,json!({"wire_active":wire_seen}));
        self.check("video: encoder completed",encoded,json!({"path":path,"frames":FRAMES,"fps":FPS}));
        g.cancel_crew_panel_selection();g.dbg_cupola_view(0.0,0.0);g.update(&TankInput::default(),0.0,r);
        Ok(json!({"path":path,"encoded":encoded,"frames":FRAMES,"fps":FPS,"seconds":6,
            "simulation_hz":60,"playback_speed":1,"audio":"silent diagnostic",
            "disclosure":"First 3 seconds use the ordinary commander eye and permitted head look. Last 3 seconds use a labelled inspection camera. Wound and death are staged casualties. Selection uses one real occluded LMB bridge click; destination is hovered only; no seat assignment is requested. No forced color, lamp phase or extra light.",
            "samples":samples}))
    }
}
fn face_commander_at(g:&mut Game,r:&mut Renderer,target:Vec3) {
    g.dbg_cupola_view(0.0,0.0);g.update(&TankInput::default(),0.0,r);
    let base=r.camera.yaw;let mut wanted=r.camera;wanted.set_forward(target-wanted.pos);
    let yaw=(wanted.yaw-base+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
    g.dbg_cupola_view(yaw,(wanted.pitch+0.10).clamp(-1.2,1.0));g.update(&TankInput::default(),0.0,r);
}
fn set_phase(g:&mut Game,r:&mut Renderer,night:bool) {
    g.night=night;
    r.set_sky_clock(crate::celestial_sky::SkyClock::for_act(1,if night {
        crate::celestial_sky::ClockPhase::Night
    }else{crate::celestial_sky::ClockPhase::Day}));
    advance(g,r,0.25);
}
fn await_pulse(g:&mut Game,r:&mut Renderer,actor:u32,on:bool)->bool {
    // Advance the actual simulation clock until the physical wounded lamp reaches its phase.
    // Never assign a test colour, emissive value or visual-only wound.
    for _ in 0..240 {
        let panel=g.crew_status_panel_receipt();
        if seat_for_actor(&panel,actor).is_some_and(|s|s["pulse"].as_bool()==Some(on)) {return true;}
        g.update(&TankInput::default(),DT,r);
    }
    false
}

pub fn run(requested:&str)->anyhow::Result<()> {
    let prefix=crate::capture_target(requested);
    let names=["noon_healthy_wide","noon_healthy_close","night_healthy_wide","night_healthy_close",
        "noon_mixed_on_wide","noon_mixed_on_close","noon_mixed_off_close",
        "night_mixed_on_wide","night_mixed_on_close","night_mixed_off_close","night_tpz_gunner_eye"];
    let receipt_path=format!("{prefix}.json");
    ensure!(!Path::new(&receipt_path).exists()&&!Path::new(&format!("{prefix}_pulse_and_selection.mp4")).exists()
        &&names.iter().all(|name|!Path::new(&format!("{prefix}_{name}.png")).exists()),
        "crew panel review prefix exists; choose a new prefix");
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
    let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let scene=crate::tank_editor_scene::TankEditorScene::new(crate::tank_editor_scene::TankAssetId::TigerI);
    let (mut r,mut g)=Game::build_tank_editor(&device,&queue,&scene)?;
    g.mute_audio();g.set_defer_rtt_to_render(true);g.enable_singleplayer_crew();g.set_cabin_lights(true);
    g.chase_cam=false;g.hatch_open=false;g.hatch_t=0.0;g.commander_outside=false;
    g.jump_station(Station::Loader);advance(&mut g,&mut r,3.0);
    g.jump_station(Station::Commander);advance(&mut g,&mut r,3.0);
    let mut proof=Proof{prefix,device:&device,queue:&queue,checks:vec![],captures:vec![]};
    proof.check("ordinary closed commander view",g.station==Station::Commander&&!g.commander_outside&&g.mode==Mode::Tank,
        json!({"station":format!("{:?}",g.station),"commander_outside":g.commander_outside}));
    for (label,night) in [("noon",false),("night",true)] {
        set_phase(&mut g,&mut r,night);
        let panel=g.crew_status_panel_receipt();
        proof.check(&format!("{label}: all five actual healthy people have healthy lamps"),
            panel["seats"].as_array().is_some_and(|seats|seats.len()==5&&seats.iter().all(|s|s["condition"]=="healthy"&&s["actor_id"].is_number())),panel);
        proof.still(&mut g,&mut r,&format!("{label}_healthy_wide"),false)?;
        proof.still(&mut g,&mut r,&format!("{label}_healthy_close"),true)?;
    }
    let wounded=g.singleplayer_actor_id(CrewRole::Loader).context("loader actor absent")?;
    let dead=g.singleplayer_actor_id(CrewRole::Driver).context("driver actor absent")?;
    g.debug_crew_solo_stage_wound(CrewRole::Loader,crate::first_aid::WoundKind::Fracture,crate::first_aid::Region::Arm,0.4);
    proof.check("actual driver casualty staged",g.debug_crew_solo_kia(CrewRole::Driver),json!({"actor_id":dead,"ballistic_test":false}));
    advance(&mut g,&mut r,1.0);
    for (label,night) in [("noon",false),("night",true)] {
        set_phase(&mut g,&mut r,night);
        proof.check(&format!("{label}: actual wounded pulse on reached"),await_pulse(&mut g,&mut r,wounded,true),g.crew_status_panel_receipt());
        let panel=g.crew_status_panel_receipt();
        proof.check(&format!("{label}: indicators follow real wounded and dead actors"),
            seat_for_actor(&panel,wounded).is_some_and(|s|s["condition"]=="wounded")
                &&seat_for_actor(&panel,dead).is_some_and(|s|s["condition"]=="dead")
                &&!g.debug_crew_solo_man(CrewRole::Loader).dead&&g.debug_crew_solo_man(CrewRole::Driver).dead,
            json!({"panel":panel,"wounded_actor":wounded,"dead_actor":dead}));
        proof.still(&mut g,&mut r,&format!("{label}_mixed_on_wide"),false)?;
        proof.still(&mut g,&mut r,&format!("{label}_mixed_on_close"),true)?;
        proof.check(&format!("{label}: actual wounded pulse off reached"),await_pulse(&mut g,&mut r,wounded,false),g.crew_status_panel_receipt());
        proof.still(&mut g,&mut r,&format!("{label}_mixed_off_close"),true)?;
    }
    proof.tpz_still(&mut g,&mut r)?;
    let video=proof.video(&mut g,&mut r,wounded)?;
    let passed=proof.checks.iter().all(|c|c["pass"]==true);
    let receipt=json!({"schema":"panzer.crew-status-panel-review.v2","pass":passed,
        "disclosure":"Fictional diegetic game health display, not historical Tiger equipment. Wound and death were explicitly staged through real casualty APIs, not simulated ballistics. Close views use labelled diagnostic cameras; wide views use ordinary gameplay camera. No body or vehicle geometry was hidden.",
        "checks":proof.checks,"captures":proof.captures,"video":video,"final_crew":g.crew_solo_receipt()});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(passed,"crew panel review failed; inspect receipt");Ok(())
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn panel_projection_distinguishes_bottom_left_and_behind_camera() {
        let camera=crate::camera::Camera::new(16.0/9.0);
        let p=projected(&camera,Vec3::new(-0.4,-0.3,-2.0)).unwrap();
        assert!(p[0]<0.5&&p[1]>0.5);assert!(projected(&camera,Vec3::Z).is_none());
    }
}
