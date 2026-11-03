//! `cabin-panels PREFIX`: stills of the Tiger's front stations, the radio operator's sets (hull
//! right) and the driver's instrument panel (hull left), with the cabin lamps on at dusk. The
//! armor-scars review's scene (Canyon v2 winter workshop, act 1, the Tiger alone), the solo crew in
//! their posts and the player at the loader's station. Observer cameras inside the hull, placed
//! in the hull's frame; no section cut. `KESSEL_CABIN_SHOTS` adds or replaces views:
//! `name:ex,ey,ez:ax,ay,az:fov;...` (hull-local eye, hull-local aim point, degrees).
//! `KESSEL_CABIN_CLOCK` sets the UTC instant (default 1942-12-21T13:50Z, 16:50 local, dusk);
//! `KESSEL_CABIN_TUNING=REAR,FRONT` sets the radio sets' tuning (0-1); `KESSEL_CABIN_LAMPS=0` takes the stills with the cabin lamps switched off.
//! Requires KESSEL_RENDERER=raymarch.
use super::*;
use crate::{act_manifest, celestial_sky, ephemeris, graphics, world};
use serde_json::json;

const DEFAULT_SHOTS: &[(&str, [f32; 3], [f32; 3], f32)] = &[
    // From behind the driver's shoulder across to the radio operator's sets.
    ("radio", [0.25, 1.62, 1.35], [-0.75, 1.62, 2.1], 62.0),
    // The radio sets close, from the gearbox hump.
    ("radio_close", [-0.05, 1.70, 1.55], [-0.70, 1.65, 2.15], 50.0),
    // From behind the radio operator's shoulder across to the driver's panel.
    ("driver", [-0.30, 1.62, 1.35], [1.10, 1.62, 2.1], 62.0),
    ("driver_close", [0.35, 1.72, 1.55], [1.15, 1.70, 2.10], 50.0),
    // From the turret basket forward into the hull: both stations and the spill into the turret.
    ("overview", [0.0, 1.95, 0.25], [0.0, 1.45, 2.2], 78.0),
];

fn parse_v3(s: &str) -> Option<[f32; 3]> {
    let v: Vec<f32> = s.split(',').filter_map(|c| c.trim().parse().ok()).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn shots() -> Vec<(String, [f32; 3], [f32; 3], f32)> {
    let mut shots: Vec<_> = DEFAULT_SHOTS.iter().map(|&(n, e, a, f)| (n.to_string(), e, a, f)).collect();
    if let Ok(spec) = std::env::var("KESSEL_CABIN_SHOTS") {
        let extra: Vec<_> = spec.split(';').filter(|s| !s.trim().is_empty()).filter_map(|s| {
            let p: Vec<&str> = s.split(':').collect();
            if p.len() != 4 { return None; }
            Some((p[0].trim().to_string(), parse_v3(p[1])?, parse_v3(p[2])?, p[3].trim().parse().ok()?))
        }).collect();
        if !extra.is_empty() { shots = extra; }
    }
    shots
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let prefix_owned = super::game_fea_section::shared_capture_prefix(prefix);
    let prefix = prefix_owned.as_str();
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "cabin-panels requires KESSEL_RENDERER=raymarch");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let clock = std::env::var("KESSEL_CABIN_CLOCK").unwrap_or_else(|_| "1942-12-21T13:50:00Z".into());
    let instant = ephemeris::UtcInstant::parse_iso(&clock)?;
    let lamps = std::env::var("KESSEL_CABIN_LAMPS").as_deref() != Ok("0");
    let idle = TankInput::default();
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.god = true;
    g.enable_singleplayer_crew();
    g.debug_place_tiger(super::game_fea_review::TIGER_AT, 0.0);
    r.set_weather_override(Some(0.0));
    r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    g.mode = Mode::Tank;
    g.station = Station::Loader;
    g.switch_t = 0.0;
    g.set_cabin_lights(lamps);
    // `KESSEL_CABIN_TUNING=REAR,FRONT` (0-1 each): the radio sets' tuning groundwork.
    if let Ok(spec) = std::env::var("KESSEL_CABIN_TUNING") {
        for (set, t) in spec.split(',').filter_map(|s| s.trim().parse::<f32>().ok()).take(2).enumerate() {
            g.cabin_panels.set_radio_tuning(set, t);
        }
    }
    for _ in 0..90 { g.update(&idle, 1.0 / 60.0, &mut r); }
    let (cw, ch) = (r.iw * 3, r.ih * 3);
    let mut stills = Vec::new();
    for (name, eye, at, fov) in shots() {
        g.update(&idle, 1.0 / 60.0, &mut r);
        let hull = g.player.hull_model();
        let (eye_w, at_w) = (hull.transform_point3(Vec3::from(eye)), hull.transform_point3(Vec3::from(at)));
        r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
        r.camera.pos = eye_w;
        r.camera.set_forward(at_w - eye_w);
        r.camera.fov_y = fov.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.reset_history();
        let settled = crate::converge_headless_screenshot(&mut r, &device, &queue, 24)?;
        let path = format!("{prefix}_{name}.png");
        r.render_to_png(&device, &queue, &path, cw, ch)?;
        println!("[cabin-panels] {path} (exposure {:?})", settled.measured_exposure);
        stills.push(json!({"name": name, "path": path, "eye_hull": eye, "at_hull": at, "fov": fov,
            "exposure": settled.measured_exposure}));
    }
    let receipt = json!({"clock": clock, "cabin_lamps": lamps, "stills": stills});
    std::fs::write(format!("{prefix}_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    Ok(())
}
