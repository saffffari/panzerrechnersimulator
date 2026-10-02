//! Bounded, disclosed sustained-wreck-fire fixture. No gameplay victory is implied.
use crate::{act_manifest, game::{Game, TankInput}, graphics, world};
use glam::Vec3;
use std::{fs, io::Write, process::{Command, Stdio}};

fn view(r: &mut crate::renderer::Renderer, g: &mut Game, eye: Vec3, target: Vec3) {
    r.camera.pos = eye;
    r.camera.set_forward(target - eye);
    r.camera.fov_y = 58f32.to_radians();
    r.optic_mask = 0.0;
    r.reticle = false;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    r.weather_shelter = g.building_weather_shelter_at(eye);
    g.hide_couch_crew_rigs(r);
    r.ui_reset();
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_near.png")).exists(),
        "preserving existing fire review; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let weather_override=std::env::var("KESSEL_WRECK_REVIEW_STORM").ok()
        .map(|v|v.parse::<f32>()).transpose()?;
    if let Some(storm)=weather_override {
        anyhow::ensure!(storm.is_finite() && (0.0..=4.0).contains(&storm),"review storm must be finite0..4");
        r.set_weather_override(Some(storm));
    }
    let native=[r.iw,r.ih];
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(glam::Vec2::new(-450.0, -410.0), 0.0);
    let idx = g.debug_stage_burning_t34(-465.0, -394.0)
        .ok_or_else(|| anyhow::anyhow!("fixture needs a T-34"))?;
    let dt = 1.0 / 60.0;
    for _ in 0..480 { g.update(&TankInput::default(), dt, &mut r); }
    let origin = g.enemies[idx].pos;
    let target = origin + Vec3::Y * 1.8;
    let cameras = [
        ("near", Vec3::new(10.0, 1.7, -12.0)),
        ("reverse", Vec3::new(-10.0, 1.7, 12.0)),
        ("ground", Vec3::new(9.0, 14.0, -12.0)),
        ("deck", Vec3::new(5.0, 3.0, -5.0)),
    ];
    let mut shots = Vec::new();
    for (name, offset) in cameras {
        view(&mut r, &mut g, origin + offset, target);
        r.reset_history();
        let settled = crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
        anyhow::ensure!(settled.stable, "wreck review exposure did not settle");
        let path = format!("{prefix}_{name}.png");
        r.render_to_png(&device, &queue, &path, native[0], native[1])?;
        shots.push(serde_json::json!({"name":name,"path":path,"eye":r.camera.pos.to_array(),
            "target":target.to_array(),"exposure":settled.measured_exposure,"fine_vfx":r.debug_fine_vfx_receipt()}));
    }
    let frozen_state = g.debug_wreck_fire_report(idx);
    let mut effect_frames=0u32;let mut saturated=0u64;let mut missing=0u64;
    let mut maximum_weight=0.0f64;let mut fine_samples=Vec::new();
    let mut encoder = Command::new("ffmpeg").args(["-v","error","-nostdin","-y",
        "-f","rawvideo","-pixel_format","rgba","-video_size",&format!("{}x{}",native[0],native[1]),
        "-framerate","30","-i","pipe:0","-an",
        "-vf","scale=iw:ih:in_range=full:out_range=tv:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709",
        "-c:v","libx264","-threads","2","-preset","veryfast",
        "-crf","17","-pix_fmt","yuv420p","-movflags","+faststart",
        &format!("{prefix}.mp4")]).stdin(Stdio::piped()).spawn()?;
    view(&mut r, &mut g, origin + cameras[0].1, target);
    r.reset_history();
    crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
    for frame in 0..360 {
        for _ in 0..2 { g.update(&TankInput::default(), dt, &mut r); }
        let phase = (frame as f32 / 359.0) * 0.7;
        let offset = glam::Quat::from_rotation_y(phase) * cameras[0].1;
        view(&mut r, &mut g, origin + offset, target);
        encoder.stdin.as_mut().unwrap().write_all(&r.render_to_rgba(&device,&queue,native[0],native[1])?)?;
        let fine=r.debug_fine_vfx_receipt();
        let mut effects=false;
        if let Some(views)=fine["views"].as_array() {for view in views {
            let counts=&view["counts"];
            effects |= counts["parents"].as_u64().unwrap_or(0)>0;
            saturated+=counts["saturated_parents"].as_u64().unwrap_or(0);
            missing+=counts["missing_particle_ids"].as_u64().unwrap_or(0);
            maximum_weight=maximum_weight.max(counts["maximum_weight"].as_f64().unwrap_or(1.0));
        }}
        effect_frames+=u32::from(effects);
        fine_samples.push(serde_json::json!({"frame":frame,"at_s":(frame+1)as f32/30.0,"fine_vfx":fine}));
    }
    drop(encoder.stdin.take());
    anyhow::ensure!(encoder.wait()?.success(), "wreck film encoding failed");
    let passed=effect_frames==360 && saturated==0 && missing==0 && maximum_weight<=0.5;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"panzer.wreck-fire-review.v2", "manifest":manifest,"passed":passed,
        "checks":{"effects_every_frame":effect_frames==360,"zero_saturation":saturated==0,
            "zero_missing_identities":missing==0,"maximum_weight_le_half":maximum_weight<=0.5},
        "fine_vfx":{"effect_frames":effect_frames,"saturated_observations":saturated,
            "missing_identity_observations":missing,"maximum_weight":maximum_weight,"samples":fine_samples},
        "weather":{"override":weather_override,"storm":r.storm,"fog_density_m_inv":r.fog_density,"wind_mps":r.weather_wind_world.to_array()},
        "output_resolution":native,"dither":std::env::var("KESSEL_DITHER_PATTERN").unwrap_or_else(|_|"bayer4".into()),
        "adapter":adapter.get_info().name, "renderer":format!("{:?}",graphics::renderer_backend()),
        "internal_resolution":[r.iw,r.ih],"state":frozen_state,"shots":shots,
        "video_s":12,"video_audio":false,
        "disclosure":"Staged single intact-turret T-34 wreck, ignition load 0.75, secondary cookoff disabled. Standard fire evolution and renderer; eight seconds settled before stills. Observer cameras, not a player run. Weather and display from the Canyon manifest. No image grading."
    }))?)?;
    anyhow::ensure!(passed,"native fire sample acceptance failed; retained receipt contains observations");
    Ok(())
}
