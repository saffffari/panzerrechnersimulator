//! Bounded GPU acceptance of the actual isolated Game factory, plus its shared workshop HUD.
use super::*;
use serde_json::json;

pub fn draw_workshop_status(renderer: &mut Renderer) {
    renderer.fill_rect(0.23, 0.022, 0.54, 0.037, [0.0, 0.0, 0.0, 0.42]);
    renderer.draw_text_bold(
        "TANK WORKSHOP · ESC: SETTINGS",
        0.5,
        0.033,
        0.019,
        [0.86, 0.88, 0.79, 0.95],
        ui::Align::Center,
    );
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let paths = [
        format!("{prefix}_exterior.png"),
        format!("{prefix}_loader.png"),
        format!("{prefix}.json"),
    ];
    for path in &paths {
        anyhow::ensure!(
            !std::path::Path::new(path).exists(),
            "capture exists; choose a fresh prefix: {path}"
        );
    }
    let scene_path = std::env::var_os("KESSEL_TANK_EDITOR_SCENE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            tank_editor_scene::default_scene_path(tank_editor_scene::TankAssetId::TigerI)
        });
    let scene =
        tank_editor_scene::load_or_new(&scene_path, tank_editor_scene::TankAssetId::TigerI)?;
    // A dedicated still process must not emit simultaneous hardware-device audio while it advances
    // the real idle simulation. This changes audio output only; no fake sound or soundtrack is used.
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let preset = std::env::var("KESSEL_GRAPHICS_PRESET")
        .ok()
        .and_then(|value| graphics::GraphicsPreset::parse(&value))
        .unwrap_or_else(|| graphics::GraphicsPreset::for_adapter(adapter.get_info().device_type));
    graphics::set_active_preset(preset);
    let (device, queue) = request_device(&adapter);
    let (mut renderer, mut game) = Game::build_tank_editor(&device, &queue, &scene)?;
    anyhow::ensure!(
        renderer.shader_backend,
        "workshop acceptance requires the shader graphics backend"
    );
    game.set_defer_rtt_to_render(true);
    game.verify_tank_editor_scene(&scene)?;
    let constructed = game.tank_editor_status();
    game.jump_station(game::Station::Loader);
    game.debug_loader_face_breech();
    // The idle period exceeds the ordinary first ambient-boom timer and verifies that inactive
    // slots stay inactive, objectives never complete, and saved source state survives updates.
    for _ in 0..1_320 {
        game.update(&TankInput::default(), 1.0 / 60.0, &mut renderer);
    }
    game.verify_tank_editor_scene(&scene)?;
    let after_idle = game.tank_editor_status();
    let output_size = (renderer.iw * 3, renderer.ih * 3);
    let native_size = (renderer.iw, renderer.ih);
    let target = game.player.center();
    renderer.camera.pos = target + Vec3::new(8.0, 4.5, -11.0);
    renderer
        .camera
        .set_forward((target - renderer.camera.pos).normalize());
    renderer.camera.fov_y = 52.0f32.to_radians();
    renderer.reticle = false;
    renderer.optic_mask = 0.0;
    renderer.interior_light = 0.0;
    renderer.interior_haze = 0.0;
    renderer.weather_shelter = 0.0;
    renderer.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    let exterior_camera = camera_record(&renderer);
    let exterior_warmup = converge_headless_screenshot(&mut renderer, &device, &queue, 8)?;
    anyhow::ensure!(
        exterior_warmup.stable,
        "workshop exterior exposure did not stabilize"
    );
    renderer.ui_reset();
    draw_workshop_status(&mut renderer);
    renderer.render_to_png_hud(&device, &queue, &paths[0], output_size.0, output_size.1)?;

    // Restore the actual station camera through ordinary updates, without a render-only interior
    // witness. Gaze helpers aim the player's eyes; they do not move props, limbs, or mechanisms.
    game.debug_loader_face_breech();
    for _ in 0..24 {
        game.update(&TankInput::default(), 1.0 / 60.0, &mut renderer);
        game.refresh_rtt_periscopes(&mut renderer);
    }
    anyhow::ensure!(
        game.station == game::Station::Loader && game.mode == game::Mode::Tank,
        "workshop Loader capture did not retain the actual player station"
    );
    game.verify_tank_editor_scene(&scene)?;
    let loader_camera = camera_record(&renderer);
    let loader_warmup = converge_headless_screenshot(&mut renderer, &device, &queue, 8)?;
    anyhow::ensure!(
        loader_warmup.stable,
        "workshop Loader exposure did not stabilize"
    );
    renderer.ui_reset();
    draw_workshop_status(&mut renderer);
    if let Some(label) = game.hover_label_text() {
        renderer.draw_text(
            label.as_ref(),
            0.5,
            0.86,
            0.024,
            [0.92, 0.89, 0.72, 1.0],
            ui::Align::Center,
        );
    }
    renderer.render_to_png_hud(&device, &queue, &paths[1], output_size.0, output_size.1)?;
    let receipt = json!({
        "schema":"kessel.tank-editor-play-acceptance", "version":1, "passed":true,
        "constructor":"Game::build_tank_editor", "adapter":adapter.get_info().name,
        "shader_backend":renderer.shader_backend, "native_size":native_size, "output_size":output_size,
        "source_document":scene_path, "source_document_exists":scene_path.is_file(),
        "scene":scene.to_value()?, "construction":constructed, "after_22s_idle":after_idle,
        "final":game.tank_editor_status(), "exterior_camera":exterior_camera, "loader_camera":loader_camera,
        "captures":paths[..2],
        "scope":"Actual isolated game factory and updates; exterior is a labeled review camera, Loader uses the real player eye. No battlefield, encounter, projectile, or crew outcome was injected. Images still require visual review.",
    });
    std::fs::write(&paths[2], serde_json::to_vec_pretty(&receipt)?)?;
    println!("[tank-workshop] acceptance PASS: {}", paths[2]);
    Ok(())
}

fn camera_record(renderer: &Renderer) -> serde_json::Value {
    json!({"position":renderer.camera.pos.to_array(), "forward":renderer.camera.forward().to_array(),
        "fov_y_radians":renderer.camera.fov_y})
}
