//! Offscreen integration review in the actual expedition camp. Only the current person's
//! starting feet/look beside a cab door are staged; the vehicle and its stock are untouched.
//! Driving, braking, cab contact, identity projection and exit clearance use live Game paths.
use super::*;
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{io::Write, path::Path, process::{Child, Command, Stdio}};

/// Sample real 60 Hz states at 24 fps. The 2/3-tick cadence is bounded to less than one
/// simulation tick from each presentation deadline; no interpolation or repeated stills.
struct Clip {
    encoder: Child,
    size: [u32; 2],
    ticks: u32,
    frames: u32,
    samples: Vec<Value>,
}
impl Drop for Clip {
    fn drop(&mut self) {
        if !matches!(self.encoder.try_wait(), Ok(Some(_))) {
            let _ = self.encoder.kill();
            let _ = self.encoder.wait();
        }
    }
}
impl Clip {
    fn start(path: &str, size: [u32; 2]) -> anyhow::Result<Self> {
        ensure!(size.iter().all(|n| *n > 0 && n % 2 == 0), "video needs even native dimensions");
        let encoder = Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-n",
            "-f", "rawvideo", "-pixel_format", "rgba", "-video_size", &format!("{}x{}", size[0], size[1]),
            "-framerate", "24", "-i", "pipe:0", "-an",
            "-vf", "scale=iw:ih:in_range=full:out_range=tv:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
            "-c:v", "libx264", "-threads", "2", "-preset", "fast", "-crf", "16",
            "-pix_fmt", "yuv420p", "-movflags", "+faststart", path])
            .stdin(Stdio::piped()).spawn().context("start halftrack video encoder")?;
        Ok(Self { encoder, size, ticks: 0, frames: 0, samples: Vec::new() })
    }
    fn tick(&mut self, g: &Game, r: &mut Renderer, device: &wgpu::Device,
        queue: &wgpu::Queue, phase: &str) -> anyhow::Result<()> {
        self.ticks += 1;
        if self.ticks * 24 / 60 == self.frames { return Ok(()); }
        crate::simulation_uploads::drain(device, queue)?;
        let rgba = r.render_to_rgba(device, queue, self.size[0], self.size[1])?;
        self.encoder.stdin.as_mut().context("halftrack encoder stdin closed")?.write_all(&rgba)?;
        self.frames += 1;
        self.samples.push(json!({"frame":self.frames-1,"phase":phase,
            "simulation_seconds":self.ticks as f64/60.,"sample_deadline_seconds":self.frames as f64/24.,
            "transport":g.halftrack_receipt(),"foot_position_m":g.foot.pos.to_array(),
            "eye_m":r.camera.pos.to_array(),"forward":r.camera.forward().to_array(),
            "alive":g.controlled_soldier_alive(),"personal_stock":inventory(g)}));
        Ok(())
    }
    fn finish(&mut self) -> anyhow::Result<bool> {
        drop(self.encoder.stdin.take());
        Ok(self.encoder.wait()?.success())
    }
}

fn snapshot(g: &mut Game) -> anyhow::Result<Value> {
    g.sync_expedition_now()?;
    Ok(json!({"transport":g.halftrack_receipt(),"expedition":g.expedition_status(),
        "foot_position_m":g.foot.pos.to_array(),"alive":g.controlled_soldier_alive()}))
}
fn inventory(g: &Game) -> Value {
    json!({"p08_ready":g.foot_p08_rounds(),"p08_reserve":g.foot.ammo,
        "panzerfaust_ready":g.foot.panzerfaust_ready,"panzerfaust_spares":g.foot.panzerfaust_spares})
}
fn number(v: &Value, key: &str) -> f64 { v[key].as_f64().unwrap_or(f64::NAN) }
fn position(v: &Value) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|i|v["position_m"][i].as_f64().unwrap_or(f64::NAN) as f32))
}

pub fn run(requested: &str, video: bool) -> anyhow::Result<()> {
    let prefix=crate::capture_target(requested);
    let receipt_path=format!("{prefix}.json");
    let video_path=format!("{prefix}.mp4");
    let paths=["board","drive","brake","exit"].map(|name|format!("{prefix}_{name}.png"));
    ensure!(!Path::new(&receipt_path).exists()&&paths.iter().all(|p|!Path::new(p).exists()),
        "halftrack driving review prefix exists; use a new prefix");
    ensure!(!video || !Path::new(&video_path).exists(), "halftrack video exists; use a new prefix");
    if let Some(parent)=Path::new(&prefix).parent(){std::fs::create_dir_all(parent)?;}
    std::env::set_var("KESSEL_AUDIO_CAPTURE","1");
    crate::act_manifest::select_runtime_manifest(Some(crate::act_manifest::playtest_manifest_path()))?;
    let instance=crate::create_instance();
    let adapter=crate::request_adapter(&instance,None);
    let (device,queue)=crate::request_device(&adapter);
    let (mut r,mut g)=Game::build_expedition_act(&device,&queue,false,&crate::world::act(1))?;
    g.mute_audio();
    g.enable_expedition(1944,g.expedition_initial_camp_position()?)?;
    let person=g.expedition_state().context("expedition absent")?.controlled_person.context("no controlled person")?;
    let role=g.singleplayer_embodied_role().context("no embodied crew role")?;
    ensure!(g.singleplayer_person_id(role)==Some(person),"controlled person and physical actor differ");
    let v=g.halftrack_runtime.as_ref().context("camp transport not installed")?;
    let root=Mat4::from_translation(v.motion.position)*Mat4::from_rotation_y(v.motion.heading);
    let initial=snapshot(&mut g)?;
    let stock=inventory(&g);
    let photo=|r:&mut Renderer,path:&str|->anyhow::Result<()> {
        r.reset_history();
        let convergence=crate::converge_headless_screenshot(r,&device,&queue,24)?;
        ensure!(convergence.stable,"halftrack review eye did not converge");
        r.render_to_png(&device,&queue,path,1280,720)
    };
    let mut attempts=Vec::new();
    let mut boarded=false;
    let mut clip=if video {Some(Clip::start(&video_path,[r.iw,r.ih])?)}else{None};
    // Deterministic two-side cab access attempts, not a reset or vehicle repositioning.
    // Source geometry and camp occlusion decide whether the eye-to-door ray is reachable.
    for side in [1.,-1.] {
        let mut feet=root.transform_point3(Vec3::new(side*2.,0.,0.8));
        feet.y=g.hf.height_at(feet.x,feet.z);
        g.mode=Mode::Foot;g.foot.pos=feet;g.foot.vel_y=0.;g.foot.airborne=false;g.foot.crouch_t=0.;
        // Establish the ordinary on-foot camera/environment before the same boarding action
        // a player would make. A tank-view startup must not leak its cabin exposure into this shot.
        g.update(&TankInput::default(),1./60.,&mut r);
        let mut eye=g.foot.eye();
        let mut direction=(root.transform_point3(Vec3::new(side*0.6,1.3,0.8))-eye).normalize();
        g.foot.yaw=direction.x.atan2(-direction.z);g.foot.pitch=direction.y.asin();
        r.camera.pos=eye;r.camera.set_forward(direction);
        let hover=g.halftrack_hover(eye,direction);
        if hover.is_some() {
            if let Some(clip)=clip.as_mut() {
                r.reset_history();
                let settled=crate::converge_headless_screenshot(&mut r,&device,&queue,24)?;
                ensure!(settled.stable,"halftrack approach video exposure did not converge");
                for _ in 0..30 {
                    g.update(&TankInput::default(),1./60.,&mut r);
                    clip.tick(&g,&mut r,&device,&queue,"beside_cab")?;
                }
                // Use the actual settled eye for boarding after those ordinary on-foot ticks.
                eye=g.foot.eye();
                direction=(root.transform_point3(Vec3::new(side*0.6,1.3,0.8))-eye).normalize();
                g.foot.yaw=direction.x.atan2(-direction.z);g.foot.pitch=direction.y.asin();
                r.camera.pos=eye;r.camera.set_forward(direction);
            }
        }
        let result=g.try_board_halftrack(eye,direction,&mut r);
        attempts.push(json!({"staged_feet_m":feet.to_array(),"eye_m":eye.to_array(),
            "direction":direction.to_array(),"source_cab_hover":hover,
            "boarded":result.as_ref().is_ok_and(|ok|*ok),"error":result.as_ref().err().map(|e|format!("{e:#}"))}));
        boarded=result.unwrap_or(false);
        if boarded {break;}
    }
    let board=snapshot(&mut g)?;
    photo(&mut r,&paths[0])?;
    if boarded {
        if let Some(clip)=clip.as_mut() {
            for _ in 0..30 {
                g.update(&TankInput::default(),1./60.,&mut r);
                clip.tick(&g,&mut r,&device,&queue,"boarded")?;
            }
        }
    }
    let mut samples=Vec::new();
    if boarded {
        for tick in 0..180 {
            g.update(&TankInput{drive:0.65,..Default::default()},1./60.,&mut r);
            if let Some(clip)=clip.as_mut(){clip.tick(&g,&mut r,&device,&queue,"drive")?;}
            if tick%30==29 {samples.push(json!({"phase":"drive","seconds":(tick+1)as f32/60.,
                "state":snapshot(&mut g)?}));}
        }
    }
    let driven=snapshot(&mut g)?;
    photo(&mut r,&paths[1])?;
    let mut moving_exit_rejected=false;
    if boarded&&number(&driven["transport"],"speed_mps")>0.15 {
        moving_exit_rejected=g.try_exit_halftrack(&mut r).is_err()&&g.halftrack_occupied();
    }
    if boarded {
        for tick in 0..120 {
            // Space brakes even while the forward control remains held.
            g.update(&TankInput{drive:0.65,jump:true,..Default::default()},1./60.,&mut r);
            if let Some(clip)=clip.as_mut(){clip.tick(&g,&mut r,&device,&queue,"brake")?;}
            if tick%30==29 {samples.push(json!({"phase":"brake","seconds":(tick+1)as f32/60.,
                "state":snapshot(&mut g)?}));}
        }
    }
    let stopped=snapshot(&mut g)?;
    photo(&mut r,&paths[2])?;
    let exit_result=if boarded {g.try_exit_halftrack(&mut r)}else{Ok(false)};
    let exited=exit_result.as_ref().is_ok_and(|ok|*ok);
    let exit_error=exit_result.err().map(|e|format!("{e:#}"));
    let final_state=snapshot(&mut g)?;
    photo(&mut r,&paths[3])?;
    if exited {
        if let Some(clip)=clip.as_mut() {
            for _ in 0..120 {
                g.update(&TankInput::default(),1./60.,&mut r);
                clip.tick(&g,&mut r,&device,&queue,"on_foot_after_exit")?;
            }
        }
    }
    let video_end_state=if video {Some(snapshot(&mut g)?)}else{None};
    let video_written=if let Some(clip)=clip.as_mut(){clip.finish()?}else{false};
    let start=&initial["transport"];let moving=&driven["transport"];let stop=&stopped["transport"];
    let displacement=position(start).distance(position(moving));
    let contact_sources=start["source_registry_counts"].as_array().is_some_and(|a|
        a.len()==3&&a.iter().all(|v|v.as_u64().is_some_and(|n|n>0)));
    let checks=vec![
        json!({"check":"source cab contact boards existing person","pass":boarded
            &&board["transport"]["occupant"]==json!(person)
            &&samples.iter().all(|s|s["state"]["transport"]["occupant"]==json!(person))}),
        json!({"check":"ordinary drive moves existing transport","pass":displacement>2.
            &&number(moving,"distance_m")>2.&&number(moving,"speed_mps")>0.15,"displacement_m":displacement}),
        json!({"check":"driving consumes finite fuel","pass":number(moving,"fuel_litres")>0.
            &&number(moving,"fuel_litres")<number(start,"fuel_litres")}),
        json!({"check":"source contact controller remains available","pass":contact_sources
            &&start["source_triangles"].as_u64().is_some_and(|n|n>0)
            &&samples.iter().all(|s|s["state"]["transport"]["blocked_reason"]!="source contact query capacity")}),
        json!({"check":"moving exit is refused","pass":moving_exit_rejected}),
        json!({"check":"Space brakes against held drive","pass":number(stop,"speed_mps").abs()<0.01
            &&stop["blocked"]==false}),
        json!({"check":"source-clear exit preserves current identity","pass":exited&&!g.halftrack_occupied()
            &&g.mode==Mode::Foot&&g.controlled_soldier_alive()
            &&g.expedition_state().unwrap().controlled_person==Some(person)
            &&g.singleplayer_person_id(role)==Some(person),"error":exit_error}),
        json!({"check":"boarding and driving do not refill personal ammunition","pass":inventory(&g)==stock}),
    ];
    let video_receipt=clip.as_ref().map(|clip|json!({"path":video_path,"written":video_written,
        "fps":24,"simulation_hz":60,"playback_speed":1,"frames":clip.frames,"native_size":clip.size,
        "seconds":clip.frames as f64/24.,"simulation_ticks":clip.ticks,"frame_states":clip.samples,
        "audio":"silent; same muted diagnostic as the original still review",
        "scope":"Ordinary live camera, pose, lights and weather. Only starting feet/look beside cab staged. Additional neutral simulation before boarding, while boarded and after exit; real 60 Hz updates, actual 24 fps render_to_rgba frames. Still-image convergence does not advance simulation or become encoded freeze frames. No cargo, towing or camp activity scripted."}));
    let video_valid=!video||clip.as_ref().is_some_and(|c|video_written&&(144..=240).contains(&c.frames)
        &&c.frames==c.ticks*24/60);
    let pass=checks.iter().all(|c|c["pass"]==true)&&video_valid;
    let receipt=json!({"schema":"panzer.halftrack-driving-review.v1","pass":pass,"checks":checks,
        "person":person,"staging_attempts":attempts,"initial":initial,"board":board,"driven":driven,
        "stopped":stopped,"final":final_state,"samples":samples,"captures":paths,"initial_personal_stock":stock,
        "final_personal_stock":inventory(&g),"camp":g.installed_camp_receipt(),
        "video":video_receipt,"video_checks_passed":video_valid,"video_end_state":video_end_state,
        "scope":"Actual Act One expedition camp and source transport. Only current person's starting feet/look beside cab staged. Ordinary board, 3s drive, 2s brake, moving-exit rejection and source-clear stopped exit. Live exterior follow camera, unchanged terrain/source contact controller. No deliberate crash, collision accuracy, cargo, towing, crane, damage, historical handling or disk-resume claim."});
    let mut file=std::fs::OpenOptions::new().create_new(true).write(true).open(receipt_path)?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(pass,"halftrack driving review failed; inspect receipt");
    Ok(())
}
