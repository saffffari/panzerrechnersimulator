//! Bounded source-scene profiles. Timings include simulation, scene submission and a GPU fence;
//! screenshots and timestamp readback occur outside the measured interval. These are deliberately
//! heavier than a pipelined native frame and are not vsync/desktop presentation measurements.
use super::*;

pub fn run(mode: &str, frames: u32, output: &str) -> anyhow::Result<()> {
    anyhow::ensure!((60..=240).contains(&frames), "winter-bench requires 60..240 frames");
    anyhow::ensure!(["dense","combat","fire","coop2"].contains(&mode),"unknown winter-bench scenario");
    let instance=create_instance(); let adapter=request_adapter(&instance,None);
    let info=adapter.get_info(); graphics::set_active_preset(graphics::GraphicsPreset::for_adapter(info.device_type));
    let (device,queue)=request_device(&adapter);
    let path=act_manifest::playtest_manifest_path();
    let act=act_manifest::select_runtime_manifest(Some(path.clone()))?.unwrap();
    let (mut r,mut g)=Game::build_playtest_act(&device,&queue,false,&world::act(act))?;
    // A canyon course workshop fields its own defence (src/canyon.rs); any other map the winter one.
    if g.enable_canyon_course().is_err() { g.enable_live_playtest_encounter(); }
    g.mute_audio(); g.set_defer_rtt_to_render(true);
    // Initial conditions are disclosed in the receipt. God mode prevents a short benchmark from
    // replacing the measured workload with a death card; no play-session policy is changed.
    g.god=true;
    let coop=mode=="coop2";
    let stress=mode!="dense";
    let unbounded=std::env::var("KESSEL_WINTER_BENCH_UNBOUNDED").as_deref()==Ok("1");
    let mut crew=lan::CrewFrame::default();
    for (role,id) in [(lan::LanRole::Commander,1),(lan::LanRole::Gunner,2)] {
        crew.roster[role as usize]=id; crew.inputs[role as usize].post=role;
    }
    let input=TankInput { coop:coop.then_some(crew),coop_host:coop,couch_crew:coop,..TankInput::default() };
    if stress {
        for (i,(x,z)) in [(1820.0,120.0),(1900.0,100.0),(1960.0,190.0)].into_iter().enumerate() {
            g.debug_place_enemy(i,x,z,0.0);
        }
    }
    if mode=="fire" || coop { g.station=game::Station::Loader; g.debug_start_fire(); }
    g.update(&input,1.0/120.0,&mut r);
    let (mut totals,mut simulation,mut submit,mut fences)=(Vec::new(),Vec::new(),Vec::new(),Vec::new());
    let mut gpu=gpu_profile::GpuProfileSummary::default();
    let mut scenes=0u64; let mut shadows=0u64; let mut draws=0u64;
    let mut peak_scene_draws=0u64; let mut peak_rounds=0usize;
    let mut peak_fire_patches=0usize; let mut peak_wall_fire_patches=0usize;
    let warmup=24;
    for i in 0..frames+warmup {
        r.begin_gpu_profile_frame(); r.reset_render_work_stats();
        let begin=Instant::now(); let sim_begin=Instant::now();
        if stress && i%48==0 {
            for ei in 0..3 { g.debug_enemy_fire_at_player(ei); }
        }
        g.update(&input,1.0/120.0,&mut r);
        anyhow::ensure!(g.player.alive,"benchmark workload died; reject timing rather than measuring a death screen");
        let sim=elapsed_ms(sim_begin);
        let draw_begin=Instant::now();
        let saved=(r.camera,r.reticle,r.optic_mask,r.precipitation,r.weather_shelter,
            r.interior_light,r.interior_haze,r.rt_mix,r.interior_atmosphere_enabled());
        if coop {
            if !unbounded { r.begin_coop_shadow_frame(g.player.center()); }
            let shadow_ready=g.refresh_rtt_periscopes(&mut r);
            profile_campaign_role_submit(&g,&mut r,&device,&queue,lan::LanRole::Commander,
                0,false,!unbounded && g.protected_crew_render_view(lan::LanRole::Commander),
                saved.0,saved.3,!unbounded && shadow_ready);
            if unbounded { r.set_coop_shadow_reuse(false); }
            // Actual expensive two-person pairing: commander and gunner (optic + surround).
            let (eye,fwd,fov,mask,reticle)=g.coop_view_camera(lan::LanRole::Gunner);
            r.camera=saved.0; r.camera.pos=eye; r.camera.set_forward(fwd); r.camera.fov_y=fov;
            r.optic_mask=mask; r.reticle=reticle; r.precipitation=saved.3;
            r.weather_shelter=1.0; g.configure_crew_render_view(lan::LanRole::Gunner,true,&mut r);
            r.set_active_view(1); r.rt_skip=true; r.set_coop_shadow_reuse(!unbounded);
            r.render_optic_snapshot(&device,&queue);
            let (eye,fwd,fov)=g.gunner_interior_camera();
            r.camera=saved.0; r.camera.pos=eye; r.camera.set_forward(fwd); r.camera.fov_y=fov;
            if !unbounded { r.camera.zfar=PROTECTED_INTERIOR_FAR_M; r.precipitation=0.0; }
            r.optic_mask=4.0; r.reticle=false;
            g.configure_crew_render_view(lan::LanRole::Gunner,false,&mut r);
            r.set_active_view(3); r.set_coop_shadow_reuse(!unbounded); r.render_offscreen(&device,&queue);
            if !unbounded { r.end_coop_shadow_frame(); }
        } else if mode=="fire" {
            if !unbounded { r.begin_coop_shadow_frame(g.player.center()); }
            let ready=g.refresh_rtt_periscopes(&mut r); r.set_coop_shadow_reuse(!unbounded && ready);
            if !unbounded && g.protected_main_interior_view() {
                r.camera.zfar=PROTECTED_INTERIOR_FAR_M; r.precipitation=0.0;
            }
            r.render_offscreen(&device,&queue);
            if !unbounded { r.end_coop_shadow_frame(); }
        } else {
            let a=i as f32/(frames+warmup) as f32*std::f32::consts::TAU;
            let center=Vec3::new(1880.0,10.0,178.0);
            r.camera.pos=center+Vec3::new(24.0*a.cos(),5.0,24.0*a.sin());
            r.camera.set_forward(center-r.camera.pos); r.camera.fov_y=65_f32.to_radians();
            r.interior_light=0.0; r.interior_haze=0.0; r.rt_mix=1.0; r.weather_shelter=0.0;
            r.set_interior_atmosphere_enabled(false); r.optic_mask=0.0; r.reticle=false;
            r.render_offscreen(&device,&queue);
        }
        let submitted=elapsed_ms(draw_begin); let wait=Instant::now();
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        let fenced=elapsed_ms(wait); let total=elapsed_ms(begin);
        let queries=r.read_gpu_profile_frame(&device,&queue)?;
        if i>=warmup {
            totals.push(total); simulation.push(sim); submit.push(submitted); fences.push(fenced); gpu.record(queries);
            let work=r.render_work_stats(); scenes+=work.scene_renders as u64;
            shadows+=work.shadow_passes as u64;
            let frame_draws=work.scene_mesh_draws+work.scene_instanced_draws;
            draws+=frame_draws as u64; peak_scene_draws=peak_scene_draws.max(frame_draws);
            peak_rounds=peak_rounds.max(g.debug_loose_count());
            let (patches,walls,_)=g.debug_fire_spread();
            peak_fire_patches=peak_fire_patches.max(patches);
            peak_wall_fire_patches=peak_wall_fire_patches.max(walls);
        }
        if i+1==frames+warmup {
            let tag=std::env::var("KESSEL_WINTER_BENCH_TAG").unwrap_or_else(|_|"v2".into());
            anyhow::ensure!(tag.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'),"unsafe benchmark capture tag");
            // Captures are flat in captures/ (CLAUDE.md).
            let still=format!("captures/winter_bench_{mode}_{}_{tag}.png",if unbounded {"unbounded"}else{"shipping"});
            let (w,h)=(r.iw,r.ih); r.render_to_png(&device,&queue,&still,w,h)?;
        }
        r.camera=saved.0;r.reticle=saved.1;r.optic_mask=saved.2;r.precipitation=saved.3;
        r.weather_shelter=saved.4;r.interior_light=saved.5;r.interior_haze=saved.6;r.rt_mix=saved.7;
        r.set_interior_atmosphere_enabled(saved.8);r.rt_skip=false;r.set_active_view(0);
    }
    let (batches,instances)=r.instanced_stats(); let timing=cpu_profile_stats(&totals);
    let tlas=r.tlas_demand(); let (overflow_frames,missing_now)=r.tlas_overflow_stats();
    let (blases,tlas_slots)=r.rt_stats(); let (lights_offered,lights_admitted)=r.light_budget_stats();
    let report=serde_json::json!({"schema":"panzer.winter-benchmark/v2","scenario":mode,
        "variant":if unbounded {"no_shared_shadow_or_protected_clipping"} else {"shipping"},
        "adapter":info.name,"backend":format!("{:?}",info.backend),"manifest":path,
        "internal_resolution":[r.iw,r.ih],"human_views":if coop{2}else{1},
        "warmup_frames":warmup,"timed_frames":frames,"simulation_dt_s":1.0/120.0,
        "frame_total":timing,"simulation":cpu_profile_stats(&simulation),
        "render_submission":cpu_profile_stats(&submit),"queue_tail_wait":cpu_profile_stats(&fences),
        "work_per_frame":{"scene_renders":scenes as f64/frames as f64,
            "shadow_passes":shadows as f64/frames as f64,"scene_draws":draws as f64/frames as f64,
            "peak_scene_draws":peak_scene_draws},"scene":{"instance_batches":batches,"instances":instances,
            "active_loose_rounds_peak":peak_rounds,"surface_fire_patches_peak":peak_fire_patches,
            "surface_wall_fire_patches_peak":peak_wall_fire_patches,
            "tlas":{"needed":tlas.needed(),"capacity":tlas.capacity,"scene_mesh_slots":tlas.mesh_slots,
                "live_instances":tlas.instanced_live,"reserved_instance_ids":tlas.instanced_reserved,
                "overflow_frames":overflow_frames,"missing_instances_last_frame":missing_now,
                "blas":blases,"slot_layout_end":tlas_slots},
            "point_lights":{"offered_last_frame":lights_offered,"admitted_last_frame":lights_admitted,
                "budget":renderer::N_LIGHTS}},"gpu":gpu.report(),
        "mission":g.winter_mission_snapshot(),"contacts":g.winter_encounter_snapshot(),
        "scope":"Offscreen fenced scene frame. Two-person commander+gunner includes optic and interior pass, one simulation and rolling periscope RTT. Per-view crew-rig visibility updates, final desktop cell blits, HUD, input polling and presentation are excluded. Stress initial enemy positions and scheduled fire are fixtures, not AI validation. Audio muted. God mode holds the measured workload alive. 960x540 is full source resolution per view before native top/bottom crops. No claim of universal worst case or native frame-rate certification."});
    let out=std::path::Path::new(output); if let Some(parent)=out.parent(){std::fs::create_dir_all(parent)?;}
    std::fs::write(out,serde_json::to_vec_pretty(&report)?)?;
    println!("[winter-bench] {mode}: {}",report["frame_total"]);
    println!("[winter-bench] receipt {}",out.display()); Ok(())
}
