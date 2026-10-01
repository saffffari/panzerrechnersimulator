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
        r.render_to_png(&device, &queue, &path, 1920, 1080)?;
        shots.push(serde_json::json!({"name":name,"path":path,"eye":r.camera.pos.to_array(),
            "target":target.to_array(),"exposure":settled.measured_exposure}));
    }
    let frozen_state = g.debug_wreck_fire_report(idx);
    let mut encoder = Command::new("ffmpeg").args(["-v","error","-nostdin","-y",
        "-f","rawvideo","-pixel_format","rgba","-video_size","1920x1080",
        "-framerate","30","-i","pipe:0","-an","-c:v","libx264","-preset","veryfast",
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
        encoder.stdin.as_mut().unwrap().write_all(&r.render_to_rgba(&device,&queue,1920,1080)?)?;
    }
    drop(encoder.stdin.take());
    anyhow::ensure!(encoder.wait()?.success(), "wreck film encoding failed");
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"panzer.wreck-fire-review.v1", "manifest":manifest,
        "adapter":adapter.get_info().name, "renderer":format!("{:?}",graphics::renderer_backend()),
        "internal_resolution":[r.iw,r.ih],"state":frozen_state,"shots":shots,
        "video_s":12,"video_audio":false,
        "disclosure":"Staged single intact-turret T-34 wreck, ignition load 0.75, secondary cookoff disabled. Standard fire evolution and renderer; eight seconds settled before stills. Observer cameras, not a player run. Weather and display from the Canyon manifest. No image grading."
    }))?)?;
    Ok(())
}
