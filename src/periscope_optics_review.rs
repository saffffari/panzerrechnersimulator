//! `periscope-optics PREFIX`: headless stills of the commander's front vision block as the eye
//! meets it — held (RMB), from the seat, and from 30 and 60 degrees off the block's axis — and of
//! the turret's right direct-view slit block, held and from 50 degrees off its axis, by
//! day with the cabin lamps off and at night with them on. Raymarch backend (set when
//! KESSEL_RENDERER is unset), the one the range and the demo launcher run. Each still prints the
//! eye's angle off the block's inner-window normal so the vignetting can be read against
//! `relay_stop_chain_matches_the_complete_housing`.
//! By day it first works the commander's RMB through the game itself (`focus_check`): one press
//! takes the eye to the front block and keeps it there, a held press swept sideways steps to the
//! next block on the ring, and a plain press steps back out facing that block; PASS/FAIL.
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
    }
    Ok(())
}

/// Drive the commander's RMB through `Game::update` and assert the toggle and the block step.
fn focus_check(g: &mut game::Game, r: &mut crate::renderer::Renderer, device: &wgpu::Device,
    queue: &wgpu::Queue, prefix: &str) -> anyhow::Result<()> {
    let dt = 1.0 / 60.0;
    let mut frames = |g: &mut game::Game, r: &mut crate::renderer::Renderer, n: usize, aim: bool, dx: f32| {
        for _ in 0..n { g.update(&TankInput { aim, look_dx: dx, ..Default::default() }, dt, r); }
    };
    let state = |g: &game::Game| {
        let f = g.cupola_focus_receipt();
        (f["index"].as_u64().map(|i| i as usize), f["phase"].as_f64().unwrap_or(0.0) as f32)
    };
    frames(g, r, 10, false, 0.0);
    // The head faces the front block (index 0) from the seat by default.
    frames(g, r, 1, true, 0.0);
    frames(g, r, 60, false, 0.0);
    let (index, phase) = state(g);
    let toggled_in = index == Some(0) && phase >= 1.0;
    println!("[periscope-optics] focus: one RMB click -> block {index:?}, phase {phase:.2} ({})",
        if toggled_in { "PASS: stays at the block after release" } else { "FAIL" });
    // Held RMB, mouse swept right 200 px over five frames: one step to the right-hand neighbour.
    let right = tiger_periscopes::adjacent_block(0, 1);
    frames(g, r, 5, true, 40.0);
    frames(g, r, 30, true, 0.0);
    frames(g, r, 30, false, 0.0);
    let (index, phase) = state(g);
    let stepped = index == Some(right) && phase >= 1.0;
    println!("[periscope-optics] focus: held RMB swept right -> block {index:?} (expected {right}), phase {phase:.2} ({})",
        if stepped { "PASS: stepped and stays zoomed" } else { "FAIL" });
    r.set_active_view(4);
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 24)?;
    r.render_to_png(device, queue, &format!("{prefix}_day_focus_stepped_right.png"), r.iw * 3, r.ih * 3)?;
    // Swept back left past the front block: the left-hand neighbour of block 0.
    let left = tiger_periscopes::adjacent_block(0, -1);
    frames(g, r, 1, true, 0.0);
    frames(g, r, 10, true, -30.0);
    frames(g, r, 20, true, 0.0);
    frames(g, r, 30, false, 0.0);
    let (index, _) = state(g);
    let back = index == Some(left);
    println!("[periscope-optics] focus: held RMB swept left 300 px -> block {index:?} (expected {left}, two steps) ({})",
        if back { "PASS" } else { "FAIL" });
    // A plain click while at a block steps back out, the head left facing that block.
    frames(g, r, 1, true, 0.0);
    frames(g, r, 60, false, 0.0);
    let (index, phase) = state(g);
    let tm = g.player.turret_model();
    let to = (tm.transform_point3(tiger_periscopes::CUPOLA_CENTERS[left]) - r.camera.pos).normalize();
    let facing = to.dot(r.camera.forward());
    let out = index.is_none() && phase <= 0.0 && facing > 0.95;
    println!("[periscope-optics] focus: second click -> block {index:?}, phase {phase:.2}, facing block {left} {facing:.3} ({})",
        if out { "PASS: zoomed out, facing the block" } else { "FAIL" });
    anyhow::ensure!(toggled_in && stepped && back && out, "cupola focus toggle check failed");
    Ok(())
}
