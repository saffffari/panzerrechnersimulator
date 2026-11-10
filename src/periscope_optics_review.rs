//! `periscope-optics PREFIX`: headless stills of the commander's front vision block as the eye
//! meets it — held (RMB), from the seat, and from 30 and 60 degrees off the block's axis — and of
//! the turret's right direct-view slit block, held and from 50 degrees off its axis, by
//! day with the cabin lamps off and at night with them on. Raymarch backend (set when
//! KESSEL_RENDERER is unset), the one the range and the demo launcher run. Each still prints the
//! eye's angle off the block's inner-window normal so the vignetting can be read against
//! `relay_stop_chain_matches_the_complete_housing`.
//! By day it first works the commander's RMB through the game itself (`focus_check`): one click
//! takes the eye to the front block and keeps it there, the mouse turns the eye within the block's
//! field up to its limit, and a second click steps back out facing that block; PASS/FAIL.
//! Each light also takes the driver's seat at the visor (`_driver`, and `_driver_lean_lamps`: leaning
//! in with the cabin lamps lit). `KESSEL_PERISCOPE_FLICKER=N` adds N played frames per view for a
//! frame-to-frame comparison.
use crate::game::{self, tiger_periscopes};
use crate::{world, DayPhase, TankInput};
use glam::Vec3;

pub fn run(prefix: &str) -> anyhow::Result<()> {
    if std::env::var_os("KESSEL_RENDERER").is_none() {
        std::env::set_var("KESSEL_RENDERER", "raymarch");
    }
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    for night in [false, true] {
        let light = if night { "night_lamps" } else { "day" };
        if std::env::var("KESSEL_PERISCOPE_VIEWS").is_ok_and(|o| !o.contains(light)) { continue; }
        let (mut r, mut g) = game::Game::build_act(&device, &queue, night, &world::act(0))?;
        g.mute_audio();
        g.debug_banish_enemies();
        g.set_cabin_lights(night);
        g.station = game::Station::Commander;
        for _ in 0..8 { g.update(&TankInput::default(), 1.0 / 60.0, &mut r); }
        r.apply_campaign_atmosphere(1, true, if night { DayPhase::Night } else { DayPhase::Day });
        r.set_weather_seed(world::act(1).seed);
        r.reticle = false;
        r.optic_mask = 0.0;
        r.optic_blast = 0.0;
        r.weather_shelter = 1.0;
        if !night && std::env::var("KESSEL_PERISCOPE_VIEWS").map_or(true, |o| o.contains("focus")) {
            focus_check(&mut g, &mut r, &device, &queue, prefix)?;
            if let Some(n) = std::env::var("KESSEL_PERISCOPE_FLICKER").ok().and_then(|v| v.parse().ok()) {
                flicker_frames(&mut g, &mut r, &device, &queue, prefix, n)?;
            }
        }
        let tm = g.player.turret_model();
        let block = r.raster_optic_entries()[0].map(|c| tm.inverse().transform_point3(c));
        let centre = block.iter().copied().sum::<Vec3>() * 0.25;
        let mut axis = (block[1] - block[0]).cross(block[2] - block[0]).normalize();
        if axis.dot(centre - tiger_periscopes::COMMANDER_EYE) < 0.0 { axis = -axis; }
        let side = axis.cross(Vec3::Y).normalize();
        let (held_eye, held_forward, held_fov) = tiger_periscopes::close_view(0);
        let off_axis = |deg: f32, distance: f32| {
            let (s, c) = deg.to_radians().sin_cos();
            centre - axis * (c * distance) + side * (s * distance)
        };
        let mut views: Vec<(&str, Vec3, Vec3, f32)> = vec![
            ("held", held_eye, held_eye + held_forward, held_fov),
            ("seat", tiger_periscopes::COMMANDER_EYE, centre, 70f32.to_radians()),
            ("off30", off_axis(30.0, 0.20), centre, 50f32.to_radians()),
            ("off60", off_axis(60.0, 0.20), centre, 50f32.to_radians()),
        ];
        // The turret's right direct-view slit (its closed glass block, no relay).
        if let Some(port) = tiger_periscopes::side_ports::ports().map(|p| p[0]) {
            let side = port.outward.cross(Vec3::Y).normalize();
            let (s, c) = 50f32.to_radians().sin_cos();
            views.push(("port_held", port.eye, port.eye + port.outward, 30f32.to_radians()));
            views.push(("port_off50", port.inner_center - port.outward * (c * 0.25) + side * (s * 0.25),
                port.inner_center, 50f32.to_radians()));
        }
        // KESSEL_PERISCOPE_VIEWS="day_seat,night_lamps_off60" renders only those stills.
        let only = std::env::var("KESSEL_PERISCOPE_VIEWS").ok();
        for (tag, eye, target, fov) in views {
            if only.as_ref().is_some_and(|o| !o.split(',').any(|v| v == format!("{light}_{tag}"))) { continue; }
            r.camera.pos = tm.transform_point3(eye);
            r.camera.set_forward(tm.transform_point3(target) - r.camera.pos);
            r.camera.fov_y = fov;
            r.camera.znear = r.camera.znear.min(0.005);
            // The game's per-frame port sweep ran for its own head; this still has its own eye.
            for port in 0..r.raster_optic_entries().len() { r.set_raster_optic_occluded(port, false); }
            r.set_active_view(4);
            r.reset_history();
            let warmup = crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
            anyhow::ensure!(warmup.stable, "{light} {tag} exposure did not settle");
            let (block_centre, block_axis) = match tiger_periscopes::side_ports::ports().map(|p| p[0]) {
                Some(port) if tag.starts_with("port") => (port.inner_center, port.outward),
                _ => (centre, axis),
            };
            let angle = (block_centre - eye).normalize().dot(block_axis).clamp(-1.0, 1.0).acos().to_degrees();
            r.render_to_png(&device, &queue, &format!("{prefix}_{light}_{tag}.png"), r.iw * 3, r.ih * 3)?;
            println!("[periscope-optics] {light}_{tag}: eye {:.3} m from the glass, {angle:.0} deg off its axis",
                eye.distance(block_centre));
        }
        // The driver's own eye at the visor, as the game places it (its glass and the cabin lamps
        // reflected in it).
        // `_driver_lean_lamps`: leaning in to the visor (RMB held) with the cabin lamps lit by day.
        for (tag, lean, lamps) in [("driver", false, night), ("driver_lean_lamps", true, true)] {
            if !only.as_ref().map_or(true, |o| o.split(',').any(|v| v == format!("{light}_{tag}"))) { continue; }
            g.station = game::Station::Driver;
            g.set_cabin_lights(lamps);
            for _ in 0..60 { g.update(&TankInput { aim: lean, ..Default::default() }, 1.0 / 60.0, &mut r); }
            r.set_active_view(4);
            r.reset_history();
            let warmup = crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
            anyhow::ensure!(warmup.stable, "{light} {tag} exposure did not settle");
            r.render_to_png(&device, &queue, &format!("{prefix}_{light}_{tag}.png"), r.iw * 3, r.ih * 3)?;
            println!("[periscope-optics] {light}_{tag}: the driver's seat at the visor; hatches {}",
                g.roof_hatch_receipt());
            g.station = game::Station::Commander;
            g.set_cabin_lights(night);
        }
    }
    Ok(())
}

/// Drive the commander's RMB and mouse through `Game::update`: the toggle, and the eye turning
/// within the block's field while zoomed.
fn focus_check(g: &mut game::Game, r: &mut crate::renderer::Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue, prefix: &str) -> anyhow::Result<()> {
    let dt = 1.0 / 60.0;
    let mut frames = |g: &mut game::Game, r: &mut crate::renderer::Renderer, n: usize, aim: bool, dx: f32, dy: f32| {
        for _ in 0..n { g.update(&TankInput { aim, look_dx: dx, look_dy: dy, ..Default::default() }, dt, r); }
    };
    let state = |g: &game::Game| {
        let f = g.cupola_focus_receipt();
        let gaze = [f["gaze"][0].as_f64().unwrap_or(0.0) as f32, f["gaze"][1].as_f64().unwrap_or(0.0) as f32];
        (f["index"].as_u64().map(|i| i as usize), f["phase"].as_f64().unwrap_or(0.0) as f32, gaze)
    };
    let limit = tiger_periscopes::BLOCK_GAZE_LIMIT;
    frames(g, r, 10, false, 0.0, 0.0);
    // The head faces the front block (index 0) from the seat by default.
    frames(g, r, 1, true, 0.0, 0.0);
    frames(g, r, 60, false, 0.0, 0.0);
    let (index, phase, _) = state(g);
    let toggled_in = index == Some(0) && phase >= 1.0;
    println!("[periscope-optics] focus: one RMB click -> block {index:?}, phase {phase:.2} ({})",
        if toggled_in { "PASS: stays at the block after release" } else { "FAIL" });
    r.set_active_view(4);
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 24)?;
    r.render_to_png(device, queue, &format!("{prefix}_day_focus_centre.png"), r.iw * 3, r.ih * 3)?;
    // The mouse moved right and down: the eye turns that way at the same block.
    frames(g, r, 5, false, 20.0, 10.0);
    let (index, _, gaze) = state(g);
    let turned = index == Some(0) && gaze[0] > 0.0 && gaze[1] < 0.0;
    println!("[periscope-optics] focus: mouse right/down -> block {index:?}, gaze {:.1}/{:.1} deg ({})",
        gaze[0].to_degrees(), gaze[1].to_degrees(), if turned { "PASS: looks within the block" } else { "FAIL" });
    // Pushed far past the field: the eye stops at the block's limit, still at block 0.
    frames(g, r, 10, false, 400.0, -400.0);
    let (index, _, gaze) = state(g);
    let clamped = index == Some(0) && (gaze[0] - limit[0]).abs() < 1e-5 && (gaze[1] - limit[2]).abs() < 1e-5;
    println!("[periscope-optics] focus: pushed far right/up -> block {index:?}, gaze {:.1}/{:.1} deg, limit {:.1}/{:.1} ({})",
        gaze[0].to_degrees(), gaze[1].to_degrees(), limit[0].to_degrees(), limit[2].to_degrees(),
        if clamped { "PASS: held at the edge of the block's field" } else { "FAIL" });
    r.set_active_view(4);
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 24)?;
    r.render_to_png(device, queue, &format!("{prefix}_day_focus_edge_right_up.png"), r.iw * 3, r.ih * 3)?;
    // A second click steps back out, the head left facing the block.
    frames(g, r, 1, true, 0.0, 0.0);
    frames(g, r, 60, false, 0.0, 0.0);
    let (index, phase, _) = state(g);
    let tm = g.player.turret_model();
    let to = (tm.transform_point3(tiger_periscopes::CUPOLA_CENTERS[0]) - r.camera.pos).normalize();
    let facing = to.dot(r.camera.forward());
    let out = index.is_none() && phase <= 0.0 && facing > 0.95;
    println!("[periscope-optics] focus: second click -> block {index:?}, phase {phase:.2}, facing block 0 {facing:.3} ({})",
        if out { "PASS: zoomed out, facing the block" } else { "FAIL" });
    anyhow::ensure!(toggled_in && turned && clamped && out, "cupola focus check failed");
    Ok(())
}

/// KESSEL_PERISCOPE_FLICKER=N: N consecutive played frames (native grid) from the commander's seat
/// facing the front block with the engine idling, then N driving, then N zoomed into the block,
/// for a frame-to-frame comparison of the block (`{prefix}_flicker_{seat,drive,held}_NNN.png`).
fn flicker_frames(g: &mut game::Game, r: &mut crate::renderer::Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue, prefix: &str, n: usize) -> anyhow::Result<()> {
    let dt = 1.0 / 60.0;
    for (tag, drive, click) in [("seat", 0.0, false), ("drive", 0.5, false), ("held", 0.0, true)] {
        if click {
            g.update(&TankInput { aim: true, ..Default::default() }, dt, r);
            for _ in 0..60 { g.update(&TankInput::default(), dt, r); }
        }
        for _ in 0..30 { g.update(&TankInput { drive, ..Default::default() }, dt, r); }
        for i in 0..n {
            g.update(&TankInput { drive, ..Default::default() }, dt, r);
            r.render_to_png(device, queue, &format!("{prefix}_flicker_{tag}_{i:03}.png"), r.iw, r.ih)?;
            let work = r.raster_optic_work_receipt();
            let port0 = work["ports"].as_array().and_then(|p| p.iter().find(|v| v["port"] == 0)).cloned();
            let ports: Vec<String> = work["ports"].as_array().map_or(Vec::new(), |p| p.iter()
                .map(|v| format!("{}{}", v["port"], if v["refresh"] == true { "" } else { "s" })).collect());
            println!("[periscope-optics] flicker {tag} {i:03}: camera {:?} port0 age {} ports [{}]",
                r.camera.pos.to_array(),
                port0.as_ref().map_or(serde_json::Value::Null, |p| p["capture_age_frames"].clone()), ports.join(" "));
        }
    }
    Ok(())
}
