//! Bounded, accelerated visual review of the actual brawl AI and Tank movement path.
//! Optional inverted placement is a disclosed contact fixture, never an organic combat claim.
use anyhow::{ensure, Result};
use glam::Vec3;
use serde_json::json;
use std::{fs, io::Write, path::Path, process::Command};

pub fn run(requested_prefix: &str, seconds: f32, inverted_fixture: bool) -> Result<()> {
    ensure!(seconds.is_finite() && (10.0..=180.0).contains(&seconds), "duration must be 10–180 seconds");
    let prefix = crate::capture_target(requested_prefix);
    let frames = seconds.ceil() as u32;
    let receipt_path = format!("{prefix}.json");
    let video_path = format!("{prefix}.mp4");
    ensure!(!Path::new(&receipt_path).exists() && !Path::new(&video_path).exists()
        && (0..frames).all(|i| !Path::new(&format!("{prefix}_{i:04}.png")).exists()),
        "review prefix already exists; preserving its captures");
    if let Some(parent) = Path::new(&prefix).parent() { fs::create_dir_all(parent)?; }
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1"); // before Game construction: no desktop audio device
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = crate::game::Game::build_act(&device, &queue, false, &crate::world::act(0))?;
    g.mute_audio();
    g.reset_match(0xB4A1, 220.0);
    g.brawl = true;
    g.god = true;
    g.enemy_policy = None;
    let fighters = [2usize, 3, 4, 5];
    ensure!(g.enemies.len() > 5, "arena lacks the four existing brawl actors");
    for (i, e) in g.enemies.iter_mut().enumerate() {
        if !fighters.contains(&i) { e.alive = false; e.pos = Vec3::splat(-3000.0); }
    }
    for (k, &i) in fighters.iter().enumerate() {
        let west = i % 2 == 0;
        let (x,z) = (if west { -95.0 } else { 95.0 }, -20.0 + k as f32 * 30.0);
        g.enemies[i].pos = Vec3::new(x, g.hf.vehicle_height_at(x,z), z);
        g.enemies[i].heading = if west { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
        g.enemies[i].reload = 1.5 + k as f32 * 1.2;
    }
    if inverted_fixture { g.enemies[2].roll = std::f32::consts::PI; }
    g.player.pos = Vec3::new(0.0, g.hf.vehicle_height_at(0.0,-400.0), -400.0);
    let dt = 1.0 / 60.0;
    let mut samples = Vec::new();
    let mut file_index = 0;
    for step in 0..(seconds * 60.0).ceil() as u32 {
        g.update(&crate::game::TankInput::default(), dt, &mut r);
        if step % 60 != 0 { continue; }
        let center = if inverted_fixture {g.enemies[2].hull_model().transform_point3(Vec3::Y)}
            else {fighters.iter().map(|&i| g.enemies[i].center()).sum::<Vec3>() / 4.0};
        let spread = fighters.iter().map(|&i| g.enemies[i].center().distance(center)).fold(0.0_f32,f32::max);
        let eye = if inverted_fixture {center + Vec3::new(-0.65,0.3,-0.70).normalize()*23.0}
            else {center + Vec3::new(-0.38,0.72,-0.72).normalize() * (spread * 0.9 + 45.0).clamp(70.0,180.0)};
        r.camera.pos = eye; r.camera.set_forward(center-eye); r.camera.fov_y = 52f32.to_radians();
        // Game remains in tank mode for its referee; this observer is physically outside.
        r.optic_mask = 0.0; r.reticle = false; r.interior_light = 0.0; r.interior_haze = 0.0;
        r.weather_shelter = 0.0; r.rt_mix = 1.0;
        r.set_interior_atmosphere(0.0,0.0,Vec3::ZERO);
        r.set_live_texel_probe_hull(g.player.hull_model(),false);
        g.clear_crew_ko();
        r.reset_history();
        for _ in 0..4 { r.render_offscreen(&device,&queue); }
        let path = format!("{prefix}_{file_index:04}.png");
        r.render_to_png(&device,&queue,&path,r.iw,r.ih)?;
        samples.push(json!({"simulation_seconds":(step+1) as f32*dt,"frame":path,"eye":eye.to_array(),
            "vehicles":fighters.map(|i| {let e=&g.enemies[i]; json!({"id":format!("enemy:{i}"),
                "kind":e.kind.name(),"alive":e.alive,"position":e.pos.to_array(),"heading":e.heading,
                "pitch":e.pitch,"roll":e.roll,"up_y":e.hull_model().transform_vector3(Vec3::Y).y,
                "speed_mps":e.speed,"overturned":e.overturned(),"turret_attached":g.enemy_turret_attached(i),
                "fire_control":g.brawl_fire_snapshot(i),
                "origin_above_terrain_m":e.pos.y-g.hf.height_at(e.pos.x,e.pos.z),
                "source_body_clearance_m":if e.overturned() {crate::vehicle_ground::rolled_support_height_for_parts(e,g.enemy_turret_attached(i),
                    |x,z|g.hf.vehicle_height_at(x,z)).map(|h|e.pos.y-h)} else {None}})})}));
        file_index += 1;
    }
    let status = Command::new("ffmpeg").args(["-hide_banner","-loglevel","error","-n","-framerate","12",
        "-i",&format!("{prefix}_%04d.png"),"-c:v","libx264","-pix_fmt","yuv420p",&video_path]).status();
    let video_written = status.as_ref().is_ok_and(|s|s.success());
    let inverted_zero_shots = !inverted_fixture || g.enemy_main_gun_shot_sequence(2)==0;
    let receipt = json!({"schema":"panzer.vehicle-motion-review.v1","adapter":adapter.get_info().name,
        "inverted_fixture_zero_shots":if inverted_fixture {Some(inverted_zero_shots)} else {None},
        "simulation_hz":60,"sample_hz":1,"playback_fps":12,"playback_speed":12,
        "audio":"silent diagnostic; offline audio selected before Game construction",
        "seed":0xB4A1u32,"seconds":seconds,"policy":"actual Game brawl AI; no neural policy",
        "fixture":if inverted_fixture {"enemy:2 deliberately placed upside down at start"} else {"normal four-vehicle brawl staging"},
        "scope":"Actual movement/contact/AI path. Referee Tiger invincible. Sparse renders, accelerated silent playback; source terrain and atmosphere retained. Rolled support is inelastic, not six-axis crash physics.",
        "video":if video_written {Some(&video_path)} else {None},"samples":samples});
    fs::OpenOptions::new().write(true).create_new(true).open(&receipt_path)?
        .write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    ensure!(video_written,"ffmpeg failed; PNGs and diagnostic receipt were retained");
    ensure!(inverted_zero_shots,"overturned actor fired; review evidence retained");
    println!("[vehicle-motion-review] {file_index} frames, 12x playback: {video_path}; {receipt_path}");
    Ok(())
}
