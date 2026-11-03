//! `fp-body-review <prefix>`: the player's own body as he sees it when he looks down.
//! Review harness only (headless; no gameplay or renderer state survives it). The standing loader
//! and cupola posts and the seated driver inside the Tiger with the cabin lamps on, then the
//! dismounted walker straight down and at 45 degrees, then the same walker from in front: once as
//! his own drawn pages (head hidden for him alone) and once whole, as any other camera sees him.
//! Each still is paired with a measurement of the drawn first-person arm page: the longest posed
//! triangle edge against its source edge (`FirstPerson::audit`).
use super::*;

fn still(
    g: &Game, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, prefix: &str, name: &str,
    receipt: &mut Vec<serde_json::Value>,
) -> anyhow::Result<()> {
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 24)?;
    let path = format!("{prefix}_{name}.png");
    r.render_to_png(device, queue, &path, 1280, 720)?;
    let audit = g.first_person.as_ref().map(|f| f.audit(g, r)).unwrap_or(serde_json::Value::Null);
    println!("[fp-body] {name}: camera {:?} forward {:?} audit {audit}", r.camera.pos.to_array(), r.camera.forward().to_array());
    receipt.push(serde_json::json!({"still": name, "camera": r.camera.pos.to_array(),
        "forward": r.camera.forward().to_array(), "station": format!("{:?}", g.station),
        "mode": if g.mode == Mode::Foot {"foot"} else {"tank"}, "first_person": audit}));
    Ok(())
}

pub fn run(requested: &str) -> anyhow::Result<()> {
    let prefix = crate::capture_target(requested);
    if let Some(parent) = std::path::Path::new(&prefix).parent() { std::fs::create_dir_all(parent)?; }
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(1))?;
    g.enable_singleplayer_crew();
    g.set_cabin_lights(true); // the ordinary lamp switch: the owner's still is a lamp-lit interior
    g.mute_audio();
    g.debug_hold_enemy_fire();
    let idle = TankInput::default();
    let run_for = |g: &mut Game, r: &mut Renderer, seconds: f32| {
        for _ in 0..(seconds * 60.0) as usize { g.update(&idle, 1.0 / 60.0, r); }
    };
    let mut receipt = Vec::new();
    run_for(&mut g, &mut r, 1.0);
    // Inside: the walkable loader post, then the cupola, each looking at the floor and halfway.
    for (station, label) in [(Station::Loader, "loader"), (Station::Commander, "commander")] {
        g.jump_station(station);
        run_for(&mut g, &mut r, 1.5);
        anyhow::ensure!(g.station == station, "could not reach the {label} post");
        for (name, pitch) in [("down", -1.2f32), ("45", -0.785)] {
            g.head_yaw = 0.0;
            g.head_pitch = pitch;
            run_for(&mut g, &mut r, 0.1);
            still(&g, &mut r, &device, &queue, &prefix, &format!("{label}_{name}"), &mut receipt)?;
        }
    }
    // The seated driver: hands on his steering levers, looking down into his lap.
    g.jump_station(Station::Driver);
    run_for(&mut g, &mut r, 2.0);
    anyhow::ensure!(g.station == Station::Driver, "could not reach the driver post");
    for (name, pitch) in [("down", -1.2f32), ("45", -0.785)] {
        g.driver_look = Vec2::new(0.0, pitch);
        run_for(&mut g, &mut r, 0.1);
        still(&g, &mut r, &device, &queue, &prefix, &format!("driver_{name}"), &mut receipt)?;
    }
    // Outside: the walker on open ground beside the Tiger.
    g.station = Station::Loader;
    let side = g.player.pos + Quat::from_rotation_y(g.player.heading) * Vec3::new(4.5, 0.0, 0.0);
    g.mode = Mode::Foot;
    g.climb = Climb::default();
    g.commander_outside = false;
    g.foot.pos = Vec3::new(side.x, g.hf.height_at(side.x, side.z), side.z);
    g.foot.vel_y = 0.0;
    g.foot.airborne = false;
    g.foot.crouch_t = 0.0;
    g.foot.yaw = g.player.heading;
    run_for(&mut g, &mut r, 0.5);
    for (name, pitch) in [("foot_down", -1.45f32), ("foot_45", -0.785)] {
        g.foot.pitch = pitch;
        run_for(&mut g, &mut r, 0.1);
        still(&g, &mut r, &device, &queue, &prefix, name, &mut receipt)?;
    }
    // Third person: the same drawn body from 2.4 m in front at chest height.
    g.foot.pitch = 0.0;
    run_for(&mut g, &mut r, 0.1);
    let forward = Vec3::new(g.foot.yaw.sin(), 0.0, -g.foot.yaw.cos());
    let chest = g.foot.pos + Vec3::Y * 1.1;
    r.camera.pos = chest + forward * 2.4 + Vec3::Y * 0.2;
    r.camera.set_forward((chest - r.camera.pos).normalize());
    still(&g, &mut r, &device, &queue, &prefix, "third_person", &mut receipt)?;
    // The same man as every other camera draws him (the witness: whole source, head included,
    // arms at rest), on the same model matrix.
    if let Some(fp) = g.first_person.as_ref() {
        if let Some(model) = fp.debug_drawn_model(&r) {
            g.hide_first_person_body(&mut r);
            fp.draw_commander_witness(model, [false; 11], &mut r);
            still(&g, &mut r, &device, &queue, &prefix, "third_person_whole", &mut receipt)?;
        }
    }
    let path = format!("{prefix}.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.first-person-body-review.v1", "stills": receipt}))?)?;
    println!("[fp-body] wrote {path}");
    Ok(())
}
