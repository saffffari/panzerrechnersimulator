//! `raymarch-showcase PREFIX`: a fixed set of look-development stills for the raymarch backend
//! (docs/RAYMARCHING_ENGINE.md), staged on the Canyon v2 winter workshop with the same headless
//! fixtures the fire and wreck reviews use. Observer cameras, not a player run; weather and clock
//! are overridden per scene and named in the receipt. No image grading.
use crate::{
    act_manifest, celestial_sky, ephemeris,
    game::{Game, Station, TankInput},
    graphics, world,
};
use anyhow::Context;
use glam::{Vec2, Vec3};
use std::fs;

const TIGER: Vec2 = Vec2::new(-450.0, -410.0);
const WRECK: Vec2 = Vec2::new(-465.0, -394.0);

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    cw: u32,
    ch: u32,
    shots: Vec<serde_json::Value>,
    observer: Option<(Vec3, Vec3, f32)>,
}

impl Rig<'_> {
    fn clock(&self, r: &mut crate::renderer::Renderer, utc: &str) -> anyhow::Result<()> {
        let instant = ephemeris::UtcInstant::parse_iso(utc)?;
        r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
        Ok(())
    }
    /// Observer camera outside the vehicle.
    /// Observer camera outside the vehicle, re-applied before every still (the game's own
    /// station camera takes over on each update).
    fn observe(&mut self, r: &mut crate::renderer::Renderer, eye: Vec3, target: Vec3, fov_deg: f32) {
        self.observer = Some((eye, target, fov_deg));
        self.view(r, eye, target, fov_deg);
    }
    /// The game's own station camera (gunner sight, commander post).
    fn station_camera(&mut self) { self.observer = None; }
    fn view(&self, r: &mut crate::renderer::Renderer, eye: Vec3, target: Vec3, fov_deg: f32) {
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.fov_y = fov_deg.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    }
    fn still(&mut self, r: &mut crate::renderer::Renderer, name: &str, note: &str) -> anyhow::Result<()> {
        // KESSEL_SHOWCASE_SCENES=a,b,c renders only the named stills (the staging still runs).
        if let Ok(only) = std::env::var("KESSEL_SHOWCASE_SCENES") {
            if !only.split(',').any(|scene| scene.trim() == name) { return Ok(()); }
        }
        if let Some((eye, target, fov)) = self.observer { self.view(r, eye, target, fov); }
        r.reset_history();
        let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 20)?;
        let path = format!("{}_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, self.cw, self.ch)?;
        let stats = r.raymarch_stats().map(|s| format!("{s:?}"));
        self.shots.push(serde_json::json!({
            "name": name, "path": path, "note": note, "eye": r.camera.pos.to_array(),
            "clock_utc": r.sky_clock.now().to_iso(), "exposure": settled.measured_exposure,
            "overcast": r.overcast, "storm": r.storm, "fog_density_m_inv": r.fog_density,
            "precipitation": r.precipitation, "raymarch": stats,
        }));
        Ok(())
    }
}

fn step(g: &mut Game, r: &mut crate::renderer::Renderer, frames: u32, input: &TankInput) {
    for _ in 0..frames { g.update(input, 1.0 / 60.0, r); }
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    // The shader backend may run the same scenes for an A/B (KESSEL_RENDERER=shader).
    anyhow::ensure!(graphics::shader_backend_enabled(), "raymarch-showcase requires KESSEL_RENDERER=raymarch or shader");
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("palette") { return run_palette(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("haze") { return run_haze(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("res") { return run_resolution(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("ao") { return run_occlusion(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("wear") { return run_wear(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("clouds") { return run_clouds(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("shafts") { return run_shafts(prefix); }
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("snow") { return run_snow(prefix); }
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_wreck_sunset_near.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let wreck = g.debug_stage_burning_t34(WRECK.x, WRECK.y).context("fixture needs a T-34")?;
    let idle = TankInput::default();

    // Scene A: a burning wreck in clear low sun. Eight seconds of fire evolution first.
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T12:40:00Z")?;
    step(&mut g, &mut r, 480, &idle);
    let origin = g.enemies[wreck].pos;
    let deck = origin + Vec3::Y * 1.8;
    rig.observe(&mut r, origin + Vec3::new(10.0, 1.7, -12.0), deck, 58.0);
    rig.still(&mut r, "wreck_sunset_near", "burning T-34, clear sky, sun 3 deg; volumetric flame, embers, soot, fire light on snow")?;
    rig.observe(&mut r, origin + Vec3::new(4.5, 2.6, -4.5), deck, 62.0);
    rig.still(&mut r, "wreck_sunset_deck", "same fire from the deck: temperature field, flame tongues, embers")?;

    // Scene B: an 88 mm HE round into the snow forty-five metres ahead of the Tiger.
    let tiger = g.player.pos;
    let forward = g.player.forward();
    let impact = {
        let p = tiger + forward * 45.0;
        Vec3::new(p.x, g.hf.height_at(p.x, p.z), p.z)
    };
    let side = impact + Vec3::new(-20.0, 3.0, 9.0) - forward * 3.0;
    g.debug_set_he_loaded(true);
    g.debug_lay_gun_at(impact + Vec3::Y * 0.3);
    step(&mut g, &mut r, 30, &idle);
    g.debug_fire_laid(&mut r);
    let (muzzle, _) = g.player.muzzle();
    let flight_s = muzzle.distance(impact) / 790.0;
    let flight_frames = (flight_s * 60.0).round() as u32;
    step(&mut g, &mut r, 2, &idle);
    rig.observe(&mut r, tiger + Vec3::new(-7.0, 2.4, -3.0) + forward * 2.0, muzzle + forward * 3.0, 60.0);
    rig.still(&mut r, "he_muzzle", "88 mm discharge: brake-shaped flash, plume lit by its own flash, tracer")?;
    step(&mut g, &mut r, 28, &idle);
    rig.observe(&mut r, tiger + Vec3::new(-9.0, 2.0, 4.0) + forward * 4.0, muzzle + forward * 5.0 - Vec3::Y * 0.8, 58.0);
    rig.still(&mut r, "he_muzzle_cloud", "88 mm report at +0.5 s: the blast's snow cloud ahead of the muzzle, smoke supports")?;
    step(&mut g, &mut r, flight_frames.saturating_sub(30) + 3, &idle);
    rig.observe(&mut r, side, impact + Vec3::Y * 1.2, 58.0);
    rig.still(&mut r, "he_impact_flash", "HE detonation at +0.05 s: fireball core, flash light on the snow, ejecta")?;
    step(&mut g, &mut r, 14, &idle);
    rig.still(&mut r, "he_impact_bloom", "HE detonation at +0.3 s: rolling fireball, sparks, soot column")?;
    step(&mut g, &mut r, 48, &idle);
    rig.still(&mut r, "he_impact_smoke", "HE detonation at +1.1 s: soot and lingering embers")?;
    step(&mut g, &mut r, 114, &idle);
    rig.observe(&mut r, side + Vec3::new(-6.0, 2.0, 3.0), impact + Vec3::Y * 4.0, 60.0);
    rig.still(&mut r, "he_impact_dust", "HE detonation at +3 s: snow and earth column, burning fragments on the ground")?;
    step(&mut g, &mut r, 300, &idle);
    rig.still(&mut r, "he_impact_plume", "HE detonation at +8 s: soot drifting with the wind")?;

    // Scene C: an AP round into the wreck hull: armour impact sparks, hot rim, ricochet.
    g.debug_set_he_loaded(false);
    g.debug_lay_gun_at(origin + Vec3::Y * 1.3);
    step(&mut g, &mut r, 30, &idle);
    g.debug_fire_laid(&mut r);
    let flight = (g.player.muzzle().0.distance(origin) / 770.0 * 60.0).round() as u32;
    step(&mut g, &mut r, flight + 4, &idle);
    rig.observe(&mut r, origin + Vec3::new(-6.0, 1.6, -9.0), origin + Vec3::Y * 1.3, 55.0);
    rig.still(&mut r, "ap_hull_sparks", "AP strike on the burning hull at +0.07 s: spark shower, hot rim, fragment glow")?;

    // Scene D: night. Moonlight, stars, the fire as the only warm source.
    rig.clock(&mut r, "1942-12-21T19:30:00Z")?;
    step(&mut g, &mut r, 60, &idle);
    rig.observe(&mut r, origin + Vec3::new(-13.0, 2.0, -15.0), deck, 55.0);
    rig.still(&mut r, "wreck_night", "clear moonlit night: fire light against moonlit snow and stars")?;
    rig.observe(&mut r, tiger + Vec3::new(9.0, 2.2, -14.0), tiger + Vec3::Y * 1.6 + (origin - tiger) * 0.3, 52.0);
    rig.still(&mut r, "tiger_night_firelight", "the Tiger lit by the distant fire and the moon")?;

    // Scene E: the commander's cabin at night: lamps with depth faces, haze, reflections.
    g.station = Station::Commander;
    g.commander_outside = false;
    rig.station_camera();
    step(&mut g, &mut r, 30, &idle);
    rig.still(&mut r, "cabin_night", "commander's post: dome and dial lamps with their own shadows and haze glow")?;

    // Scene F: blizzard tracers. Coax MG held from the gunner's sight, seen from outside.
    r.set_weather_override(Some(3.0));
    rig.clock(&mut r, "1942-12-21T09:30:00Z")?;
    g.station = Station::Gunner;
    g.debug_lay_gun_at(origin + Vec3::Y * 1.0);
    let coax = TankInput { coax: true, mg_coax: true, ..TankInput::default() };
    rig.station_camera();
    step(&mut g, &mut r, 70, &coax);
    rig.still(&mut r, "blizzard_coax_optic", "TZF sight in a blizzard: coax tracers through snow, eyepiece glass, Bayer crush")?;
    step(&mut g, &mut r, 6, &coax);
    rig.observe(&mut r, tiger + Vec3::new(6.0, 2.6, 4.0), tiger + forward * 12.0 + Vec3::Y * 1.5, 60.0);
    rig.still(&mut r, "blizzard_tracers", "outside in the storm: tracer strips, their heads lighting the snow, the wreck's glow in fog")?;
    step(&mut g, &mut r, 60, &idle);
    rig.observe(&mut r, tiger + Vec3::new(-14.0, 3.0, -2.0), tiger + Vec3::Y * 1.5, 50.0);
    rig.still(&mut r, "blizzard_tiger", "the Tiger in the blizzard: weather layer, snow grains, key beam through storm cloud")?;

    // Scene G: the Tiger drives on in clear low sun and leaves its ruts; seen low from behind.
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T12:40:00Z")?;
    let drive = TankInput { drive: 1.0, ..TankInput::default() };
    step(&mut g, &mut r, 360, &drive);
    step(&mut g, &mut r, 30, &idle);
    let here = g.player.pos;
    let heading = g.player.forward();
    let side = heading.cross(Vec3::Y).normalize_or_zero();
    let ground = g.hf.height_at(here.x, here.z);
    rig.observe(&mut r, Vec3::new(here.x, ground, here.z) - heading * 7.0 + side * 5.0 + Vec3::Y * 2.4, here + Vec3::Y * 0.9 - heading * 1.0, 60.0);
    rig.still(&mut r, "tiger_ruts", "after 6 s of driving: tracks in the snowpack, ruts behind, the hull lit from the snow below")?;

    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixtures (banished enemies, held fire, one burning T-34 at ignition load 0.75, scripted gun lays). Weather and clock overridden per scene. Observer cameras, not a player run. No image grading."
    }))?)?;
    println!("[raymarch-showcase] {} stills -> {prefix}_*.png", rig.shots.len());
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=palette`: the same fixtures through the day and the weather from two
/// observers, for judging colour across clear, overcast, dusk, night and storm. Nothing is
/// graded; what differs between stills is the ephemeris and the weather profile.
fn run_palette(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_noon_clear_wide.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let wreck = g.debug_stage_burning_t34(WRECK.x, WRECK.y).context("fixture needs a T-34")?;
    let idle = TankInput::default();
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    step(&mut g, &mut r, 480, &idle);
    let origin = g.enemies[wreck].pos;
    let tiger = g.player.pos;
    let forward = g.player.forward();
    let wide = (tiger + Vec3::new(-14.0, 3.2, -2.0), tiger + forward * 30.0 + Vec3::Y * 1.0, 62.0);
    let close = (tiger + Vec3::new(9.0, 2.2, -14.0), tiger + Vec3::Y * 1.6 + (origin - tiger) * 0.3, 52.0);
    // (name, UTC instant, storm_weather intensity or None for a dry overcast, note)
    let scenes = [
        ("dawn_clear", "1942-12-21T05:25:00Z", Some(0.0), "sun rising, clear"),
        ("noon_clear", "1942-12-21T09:00:00Z", Some(0.0), "midday, clear"),
        ("noon_overcast", "1942-12-21T09:00:00Z", None, "midday under cloud, dry"),
        ("dusk_clear", "1942-12-21T12:40:00Z", Some(0.0), "sun 3 deg, clear"),
        ("dusk_overcast", "1942-12-21T12:40:00Z", None, "sun 3 deg under cloud, dry"),
        ("night_clear", "1942-12-21T19:30:00Z", Some(0.0), "moon and stars, clear"),
        ("night_overcast", "1942-12-21T19:30:00Z", None, "night under cloud: the fire is the light"),
        ("blizzard", "1942-12-21T09:30:00Z", Some(3.0), "storm layer, snow, key beam through cloud"),
        ("windy_snow", "1942-12-21T09:30:00Z", Some(1.0), "light snow on a 5 m/s wind: spindrift over the ground"),
    ];
    for (name, utc, weather, note) in scenes {
        // A dry overcast is not on the storm scale (every intensity above zero snows), so it is
        // the act's own weather with the cloud cover set by hand, re-applied after each update.
        let overcast = |r: &mut crate::renderer::Renderer| {
            if weather.is_none() { r.overcast = 0.82; r.storm = 0.0; r.fog_density = 0.0009; r.precipitation = 0.0; }
        };
        r.set_weather_override(weather);
        overcast(&mut r);
        rig.clock(&mut r, utc)?;
        step(&mut g, &mut r, 30, &idle);
        overcast(&mut r);
        rig.observe(&mut r, wide.0, wide.1, wide.2);
        rig.still(&mut r, &format!("{name}_wide"), note)?;
        overcast(&mut r);
        rig.observe(&mut r, close.0, close.1, close.2);
        rig.still(&mut r, &format!("{name}_tiger"), note)?;
    }
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.palette.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixtures (banished enemies, held fire, one burning T-34). Clock and weather overridden per scene. Observer cameras, not a player run. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=haze`: the Tiger seen from 3, 10, 30 and 100 m, sun behind the
/// observer, in clear, dry overcast and blizzard weather, for judging the medium between the
/// eye and a dark object. With `KESSEL_RM_PROBE=400,225` the hull pixel at the frame centre
/// prints its lit radiance and the medium column (transmittance and in-scatter per distance).
fn run_haze(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_act_3m.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    rig.clock(&mut r, "1942-12-21T10:30:00Z")?;
    step(&mut g, &mut r, 30, &idle);
    let tiger = g.player.pos + Vec3::Y * 1.5;
    // The sun at 10:30 UTC stands south-south-west; the observer looks along it from behind.
    let sun = r.sun_dir.normalize_or_zero();
    let back = -Vec3::new(sun.x, 0.0, sun.z).normalize_or_zero();
    // "act" first: the act's own weather profile and wind, untouched (Act I: 0.0068/m ground
    // medium, storm 0.98, a 5.4 m/s exposed wind), which is what a player sees.
    let weathers: [(&str, Option<f32>, bool); 4] = [("act", None, false), ("clear", Some(0.0), false), ("overcast", None, true), ("blizzard", Some(3.0), false)];
    for (wname, weather, dry_overcast) in weathers {
        let overcast = |r: &mut crate::renderer::Renderer| {
            if dry_overcast { r.overcast = 0.82; r.storm = 0.0; r.fog_density = 0.0009; r.precipitation = 0.0; }
        };
        r.set_weather_override(weather);
        overcast(&mut r);
        step(&mut g, &mut r, 30, &idle);
        for d in [3.0f32, 10.0, 30.0, 100.0] {
            overcast(&mut r);
            let eye = tiger - back * d + Vec3::Y * (0.3 + d * 0.06);
            rig.observe(&mut r, eye, tiger, 40.0);
            rig.still(&mut r, &format!("{wname}_{d:.0}m"), &format!("{wname}: the Tiger at {d:.0} m, sun behind the observer"))?;
        }
    }
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.haze.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixture (banished enemies). Clock and weather overridden. Observer cameras. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=res`: the stills for judging an internal resolution (run once per
/// `KESSEL_INTERNAL_RES`): a live T-34 at the longest clear range up to 1,500 m through the TZF, the Tiger at 25 m in low sun,
/// the burning wreck at dusk, the commander's cabin at night. Same fixtures at every grid.
fn run_resolution(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_t34_sight.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    // KESSEL_RES_MENU_SWITCH=WxH: switch the grid through the graphics menu's own path after the
    // act is built (the in-game route), so the stills check that path on this backend.
    if let Ok(size) = std::env::var("KESSEL_RES_MENU_SWITCH") {
        let (w, h) = size.split_once('x').context("KESSEL_RES_MENU_SWITCH=WxH")?;
        let mut settings = crate::graphics_menu::Settings::from_renderer(&r);
        settings.resolution = (w.parse()?, h.parse()?);
        settings.apply(&mut r, &device, &queue)?;
        println!("[raymarch-showcase] graphics menu switched the grid to {}x{}", r.iw, r.ih);
    }
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: 2400, ch: 1350, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T10:30:00Z")?;
    step(&mut g, &mut r, 30, &idle);
    // A live T-34 broadside on the bearing, nearest the Tiger's front, whose sight line clears the
    // terrain to the longest range up to 1,500 m (the workshop's hills hide most bearings),
    // laid on through the gunner's sight.
    let tiger = g.player.pos;
    let forward = g.player.forward();
    let eye = tiger + Vec3::Y * 2.3;
    // The heightfield (and the drawn terrain) ends `half` metres from the origin; beyond it the
    // lookup clamps and a tank would stand under the drawn ground.
    let half = (g.hf.res as f32 - 1.0) * 0.5 * g.hf.spacing - 60.0;
    // The whole hull in view: the sight line to the ground under the target clears every crest
    // on the way by half a metre.
    let clear = |g: &Game, far: Vec3| {
        if far.x.abs() > half || far.z.abs() > half { return false; }
        let target = Vec3::new(far.x, g.hf.height_at(far.x, far.z), far.z);
        (1..400).all(|i| {
            let p = eye.lerp(target, i as f32 / 400.0);
            g.hf.height_at(p.x, p.z) < p.y - 0.5 * (i as f32 / 400.0).min(1.0 - i as f32 / 400.0) * 2.0
        })
    };
    let heading = forward.x.atan2(-forward.z);
    let mut best = (0.0f32, Vec3::ZERO);
    for k in 0..72 {
        let offset = ((k + 1) / 2) as f32 * 5f32.to_radians() * if k % 2 == 0 { 1.0 } else { -1.0 };
        let dir = Vec3::new((heading + offset).sin(), 0.0, -(heading + offset).cos());
        let reach = (6..=30).rev().map(|n| n as f32 * 50.0).find(|&d| clear(&g, tiger + dir * d)).unwrap_or(0.0);
        if reach > best.0 + 1.0 { best = (reach, tiger + dir * reach); }
        if reach >= 1500.0 { break; }
    }
    anyhow::ensure!(best.0 >= 300.0, "no bearing clears the terrain beyond 300 m");
    let far = best.1;
    let t34 = g.debug_first_t34().context("fixture needs a T-34")?;
    g.debug_place_enemy(t34, far.x, far.z, 1.57);
    g.debug_set_enemy_active(t34, true);
    step(&mut g, &mut r, 20, &idle);
    let target = g.enemies[t34].pos + Vec3::Y * 1.2;
    g.debug_lay_gun_at(target);
    g.station = Station::Gunner;
    rig.station_camera();
    step(&mut g, &mut r, 40, &idle);
    // The target's measured range and the width a 6.7 m hull subtends at this internal grid.
    let range = (g.enemies[t34].pos - r.camera.pos).length();
    let hull_px = 6.7 / range * r.ih as f32 / (2.0 * (r.camera.fov_y * 0.5).tan());
    let field_deg = r.camera.fov_y.to_degrees();
    // Where the hull centre sits in the sight: degrees right of and above the sight axis (the
    // gun is laid with its superelevation, so the target sits below the reticle's tip).
    let to_t34 = (g.enemies[t34].pos + Vec3::Y * 1.2 - r.camera.pos).normalize();
    let (fwd, right) = (r.camera.forward(), r.camera.right());
    let up = right.cross(fwd);
    let off = (to_t34.dot(right).atan2(to_t34.dot(fwd)).to_degrees(), to_t34.dot(up).atan2(to_t34.dot(fwd)).to_degrees());
    rig.still(&mut r, "t34_sight", &format!(
        "a live T-34 broadside at {range:.0} m through the TZF ({field_deg:.1} deg field): its 6.7 m hull spans {hull_px:.1} internal pixels, {:.2} deg right and {:.2} deg up of the sight axis", off.0, off.1))?;
    // Staging check: the same T-34 from above.
    let t34_pos = g.enemies[t34].pos;
    rig.observe(&mut r, t34_pos + Vec3::new(0.0, 160.0, 220.0), t34_pos, 50.0);
    rig.still(&mut r, "t34_staging_high", "the sight's T-34 from 270 m up and back")?;
    rig.station_camera();
    rig.observe(&mut r, tiger + Vec3::new(-14.0, 3.2, -2.0) + forward * -10.0, tiger + Vec3::Y * 1.4, 50.0);
    rig.still(&mut r, "tiger_25m", "the Tiger at about 25 m, mid-morning sun")?;
    let wreck = g.debug_stage_burning_t34(WRECK.x, WRECK.y).context("fixture needs a second T-34")?;
    rig.clock(&mut r, "1942-12-21T12:40:00Z")?;
    step(&mut g, &mut r, 300, &idle);
    let origin = g.enemies[wreck].pos;
    rig.observe(&mut r, origin + Vec3::new(10.0, 1.7, -12.0), origin + Vec3::Y * 1.8, 58.0);
    rig.still(&mut r, "wreck_dusk", "the burning wreck at sunset")?;
    rig.clock(&mut r, "1942-12-21T19:30:00Z")?;
    g.station = Station::Commander;
    g.commander_outside = false;
    rig.station_camera();
    step(&mut g, &mut r, 60, &idle);
    rig.still(&mut r, "cabin_night", "the commander's post at night")?;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.res.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixtures (banished enemies, held fire, a live T-34 at the longest clear range up to 1,500 m, one burning T-34). Clock and weather overridden. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=ao`: the views where occlusion the frame cannot see matters, for the
/// ray-query A/B (`KESSEL_RM_SKIP=32768` turns the ray queries off): the commander's post and the
/// loader's at noon with the hatch shut and open, the Tiger's rear under the turret bustle, its
/// running gear from low beside it, the hull from beneath the barrel.
fn run_occlusion(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_commander_noon_shut.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    step(&mut g, &mut r, 30, &idle);
    for (station, name) in [(Station::Commander, "commander"), (Station::Loader, "loader")] {
        for (open, state) in [(false, "shut"), (true, "open")] {
            g.station = station;
            g.commander_outside = false;
            g.hatch_open = open;
            rig.station_camera();
            step(&mut g, &mut r, 40, &idle);
            rig.still(&mut r, &format!("{name}_noon_{state}"), &format!("the {name}'s post at noon, hatch {state}"))?;
        }
    }
    g.hatch_open = false;
    g.station = Station::Commander;
    step(&mut g, &mut r, 20, &idle);
    let p = g.player.pos;
    let f = g.player.forward();
    let side = f.cross(Vec3::Y).normalize_or_zero();
    for (name, eye, at, note) in [
        ("rear_bustle", p - f * 5.5 + Vec3::Y * 1.6, p + Vec3::Y * 1.8, "the rear plate and engine deck under the turret bustle, from behind at 1.6 m"),
        ("running_gear", p + side * 4.2 + Vec3::Y * 0.7, p + side * 1.6 + Vec3::Y * 0.6, "the running gear and the fender's underside from 0.7 m beside the hull"),
        ("under_barrel", p + f * 7.0 + side * 1.5 + Vec3::Y * 1.2, p + Vec3::Y * 1.6, "the glacis and mantlet from in front, under the barrel"),
    ] {
        rig.observe(&mut r, eye, at, 60.0);
        rig.still(&mut r, name, note)?;
    }
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.ao.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "ray_queries_off": std::env::var("KESSEL_RM_SKIP").ok().and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v & 32768 != 0),
        "disclosure": "Staged fixture (banished enemies, held fire). Clear noon, sky clock pinned. Observer and station cameras. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=wear`: wear and grime on the Tiger (`surface_wear`). The four crew posts
/// inside clean (every load zero) and after a fight (the campaign's starting loads, then ten
/// rounds fired and a minute at speed through the game's own update), the hull outside before and
/// after that drive, and the commander's vision blocks before and after a wipe.
fn run_wear(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_clean_commander.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    step(&mut g, &mut r, 30, &idle);
    let loads = |g: &Game| {
        let [m, i, e, gl] = g.surface_wear_loads();
        format!("wear: mechanical {m:.2}, interior grime {i:.2}, exterior grime {e:.2}, glass {gl:.2}")
    };
    // The same pose for both states: the gun laid on a point ahead, the Tiger put back after the
    // drive.
    let aim = g.player.pos + g.player.forward() * 400.0 + Vec3::Y * 2.0;
    g.debug_set_he_loaded(false);
    g.debug_lay_gun_at(aim);
    step(&mut g, &mut r, 10, &idle);
    let stations = [(Station::Commander, "commander"), (Station::Loader, "loader"), (Station::Driver, "driver"), (Station::Radioman, "radioman")];
    let mut posts = |g: &mut Game, r: &mut crate::renderer::Renderer, rig: &mut Rig, state: &str| -> anyhow::Result<()> {
        for (station, name) in stations {
            g.station = station;
            g.commander_outside = false;
            g.hatch_open = false;
            rig.station_camera();
            step(g, r, 40, &idle);
            let note = format!("the {name}'s post at noon, {state}; {}", loads(g));
            rig.still(r, &format!("{state}_{name}"), &note)?;
        }
        // The commander's post again with the hatch open: daylight on the turret's inside.
        g.station = Station::Commander;
        g.hatch_open = true;
        rig.station_camera();
        step(g, r, 40, &idle);
        rig.still(r, &format!("{state}_commander_open"), &format!("the commander's post, hatch open, {state}; {}", loads(g)))?;
        g.hatch_open = false;
        Ok(())
    };
    let outside = |g: &Game| {
        let p = g.player.pos;
        let f = g.player.forward();
        let side = f.cross(Vec3::Y).normalize_or_zero();
        (p + side * 5.5 - f * 2.0 + Vec3::Y * 1.4, p + Vec3::Y * 0.9 - f * 0.5)
    };
    g.debug_set_wear([0.0; 4]);
    step(&mut g, &mut r, 2, &idle);
    posts(&mut g, &mut r, &mut rig, "clean")?;
    let (eye, at) = outside(&g);
    rig.observe(&mut r, eye, at, 55.0);
    rig.still(&mut r, "clean_outside", &format!("the hull from the side, clean; {}", loads(&g)))?;
    // KESSEL_WEAR_EXTREME=1: every load at 1 in the same pose (no rounds, no drive), to see the
    // masks alone.
    if std::env::var("KESSEL_WEAR_EXTREME").as_deref() == Ok("1") {
        g.debug_set_wear([1.0; 4]);
        step(&mut g, &mut r, 2, &idle);
        posts(&mut g, &mut r, &mut rig, "worn")?;
        let (eye, at) = outside(&g);
        rig.observe(&mut r, eye, at, 55.0);
        rig.still(&mut r, "worn_outside", &format!("the hull from the side, every load at 1; {}", loads(&g)))?;
        return Ok(());
    }
    // The fight: the campaign's starting state, ten rounds, a minute at speed.
    g.debug_set_wear(crate::surface_wear::CAMPAIGN_START);
    g.station = Station::Gunner;
    for _ in 0..10 {
        g.debug_set_he_loaded(false);
        g.debug_fire_laid(&mut r);
        step(&mut g, &mut r, 90, &idle);
    }
    let drive = TankInput { drive: 1.0, ..TankInput::default() };
    step(&mut g, &mut r, 3600, &drive);
    step(&mut g, &mut r, 120, &idle);
    g.debug_place_tiger(TIGER, 0.0);
    g.debug_set_he_loaded(false);
    g.debug_lay_gun_at(aim);
    step(&mut g, &mut r, 120, &idle);
    posts(&mut g, &mut r, &mut rig, "fought")?;
    let (eye, at) = outside(&g);
    rig.observe(&mut r, eye, at, 55.0);
    rig.still(&mut r, "fought_outside", &format!("the hull from the side after ten rounds and a minute at speed; {}", loads(&g)))?;
    // The commander's vision blocks, filmed, then wiped.
    g.station = Station::Commander;
    g.hatch_open = false;
    rig.station_camera();
    step(&mut g, &mut r, 30, &idle);
    rig.still(&mut r, "glass_filmed", &format!("the commander's vision blocks before a wipe; {}", loads(&g)))?;
    g.wipe_glass();
    step(&mut g, &mut r, 2, &idle);
    rig.still(&mut r, "glass_wiped", &format!("the commander's vision blocks after a wipe; {}", loads(&g)))?;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.wear.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixture (banished enemies, held fire). Clear noon, sky clock pinned. The fight is ten rounds fired into the open and a minute at full throttle through the game's update. Station and observer cameras. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=clouds`: the cloud layer in the world. Broken cloud at noon over the field
/// and thirty seconds later (the shadows move with the layer), a shadow's edge on the Tiger, shafts
/// through a gap toward a low sun, and the low weather: a fog bank over the field and a T-34 at
/// 800 m in it. `KESSEL_RM_SKIP=131072` renders the same without per-position cloud shadows.
fn run_clouds(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_field_noon.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    r.set_weather_override(None);
    // Broken cloud with a little haze, set again after every update (the act's weather would
    // otherwise restore its own).
    let weather = |r: &mut crate::renderer::Renderer, cover: f32, haze: f32| {
        r.overcast = cover; r.storm = 0.0; r.fog_density = haze; r.precipitation = 0.0;
    };
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    step(&mut g, &mut r, 30, &idle);
    weather(&mut r, 0.5, 0.0012);
    let tiger = g.player.pos;
    let f = g.player.forward();
    let side = f.cross(Vec3::Y).normalize_or_zero();
    let note = |r: &crate::renderer::Renderer, what: &str| format!("{what}; cloud cover {:.2}, haze {:.4}/m", r.overcast, r.fog_density);
    rig.observe(&mut r, tiger - f * 70.0 + side * 40.0 + Vec3::Y * 45.0, tiger + f * 120.0 - side * 30.0, 60.0);
    let text = note(&r, "broken cloud over the field at noon: cloud shadows on the snow");
    rig.still(&mut r, "field_noon", &text)?;
    for _ in 0..30 { step(&mut g, &mut r, 60, &idle); weather(&mut r, 0.5, 0.0012); }
    rig.observe(&mut r, tiger - f * 70.0 + side * 40.0 + Vec3::Y * 45.0, tiger + f * 120.0 - side * 30.0, 60.0);
    let text = note(&r, "the same view thirty seconds later: the shadows have moved with the layer");
    rig.still(&mut r, "field_noon_30s", &text)?;
    rig.observe(&mut r, tiger + side * 11.0 - f * 4.0 + Vec3::Y * 2.2, tiger + Vec3::Y * 1.3, 55.0);
    let text = note(&r, "the Tiger at 11 m under broken cloud");
    rig.still(&mut r, "hull_noon", &text)?;
    // Shafts: a low sun behind a broken layer, haze to carry them.
    rig.clock(&mut r, "1942-12-21T12:25:00Z")?;
    step(&mut g, &mut r, 30, &idle);
    weather(&mut r, 0.45, 0.006);
    let sun = r.sun_dir;
    let toward = Vec3::new(sun.x, 0.0, sun.z).normalize_or_zero();
    // Just above the horizon toward the sun: the far haze lit and shadowed by the layer above it.
    rig.observe(&mut r, tiger - toward * 25.0 + Vec3::Y * 4.0, tiger + toward * 400.0 + Vec3::Y * 30.0, 80.0);
    let text = note(&r, "toward a low sun through a broken layer: shafts in the haze");
    rig.still(&mut r, "shafts_low_sun", &text)?;
    // The low weather: fog banks pooled in the low ground under an overcast morning.
    rig.clock(&mut r, "1942-12-21T07:30:00Z")?;
    let floor = tiger.y - 6.0;
    let banks = [0.03, 0.55, 30.0, floor];
    r.fog_bank = banks;
    step(&mut g, &mut r, 30, &idle);
    weather(&mut r, 0.8, 0.0015);
    let bank_note = |r: &crate::renderer::Renderer, what: &str| format!(
        "{what}; fog banks {:.3}/m over {:.0} % of the low ground, {:.0} m deep above y {:.1}; cloud cover {:.2}",
        r.fog_bank[0], r.fog_bank[1] * 100.0, r.fog_bank[2], r.fog_bank[3], r.overcast);
    rig.observe(&mut r, tiger - f * 40.0 + side * 25.0 + Vec3::Y * 30.0, tiger + f * 200.0, 65.0);
    let text = bank_note(&r, "fog banks over the field on an overcast morning");
    rig.still(&mut r, "fog_bank", &text)?;
    // A T-34 broadside at 800 m on a bearing whose sight line clears the terrain, seen from the
    // commander's height on the Tiger, with the banks and without.
    // Beside the Tiger at its commander's height (inside the turret is the turret).
    let eye = tiger + side * 4.0 + Vec3::Y * 2.9;
    let heading = f.x.atan2(-f.z);
    let clear = |g: &Game, at: Vec3| {
        let target = Vec3::new(at.x, g.hf.height_at(at.x, at.z), at.z);
        (1..400).all(|i| {
            let t = i as f32 / 400.0;
            let q = eye.lerp(target, t);
            g.hf.height_at(q.x, q.z) < q.y - 0.5 * t.min(1.0 - t) * 2.0
        })
    };
    let mut spot = None;
    for k in 0..72 {
        let offset = ((k + 1) / 2) as f32 * 5f32.to_radians() * if k % 2 == 0 { 1.0 } else { -1.0 };
        let dir = Vec3::new((heading + offset).sin(), 0.0, -(heading + offset).cos());
        if clear(&g, tiger + dir * 800.0) { spot = Some(tiger + dir * 800.0); break; }
    }
    let spot = spot.context("no bearing clears the terrain to 800 m")?;
    let t34 = g.debug_first_t34().context("fixture needs a T-34")?;
    g.debug_place_enemy(t34, spot.x, spot.z, 1.57);
    g.debug_set_enemy_active(t34, true);
    step(&mut g, &mut r, 20, &idle);
    weather(&mut r, 0.8, 0.0015);
    let t34_pos = g.enemies[t34].pos;
    let range = (t34_pos - eye).length();
    // Floor the banks at the T-34's ground so it stands in one where the field covers it.
    r.fog_bank = [banks[0], banks[1], banks[2], t34_pos.y - 6.0];
    rig.observe(&mut r, eye, t34_pos + Vec3::Y * 1.2, 12.0);
    let text = bank_note(&r, &format!("a T-34 broadside at {range:.0} m through 12 degrees from beside the Tiger at its commander's height, in the fog banks"));
    rig.still(&mut r, "t34_800m_banks", &text)?;
    r.fog_bank = [0.0; 4];
    let text = bank_note(&r, &format!("the same T-34 at {range:.0} m without fog banks"));
    rig.still(&mut r, "t34_800m_clear", &text)?;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.clouds.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "cloud_shadows_off": std::env::var("KESSEL_RM_SKIP").ok().and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v & 131072 != 0),
        "disclosure": "Staged fixture (banished enemies, held fire). Cloud cover and haze set by hand per scene; sky clock pinned. Observer cameras. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=shafts`: light shafts in the medium. Outside: a low sun through a burning
/// wreck's smoke, and toward the sun over the Tiger in haze. Inside: the commander's post after
/// three rounds (their fumes in the compartment) with the turret turned so the front vision block
/// faces a low sun, the same post with the hatch open at noon, and the shut post twice in
/// consecutive frames to measure flicker.
fn run_shafts(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_smoke_low_sun.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    r.set_weather_override(None);
    let weather = |r: &mut crate::renderer::Renderer, cover: f32, haze: f32| {
        r.overcast = cover; r.storm = 0.0; r.fog_density = haze; r.precipitation = 0.0;
    };
    // A low sun, clear, haze to carry the shafts.
    rig.clock(&mut r, "1942-12-21T12:15:00Z")?;
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    let wreck = g.debug_stage_burning_t34(WRECK.x, WRECK.y).context("fixture needs a T-34")?;
    step(&mut g, &mut r, 300, &idle);
    weather(&mut r, 0.0, 0.004);
    let sun = r.sun_dir;
    let toward = Vec3::new(sun.x, 0.0, sun.z).normalize_or_zero();
    let origin = g.enemies[wreck].pos;
    let note = |r: &crate::renderer::Renderer, what: &str| format!("{what}; sun {:.1} deg up, haze {:.4}/m, cloud cover {:.2}",
        r.sun_dir.y.asin().to_degrees(), r.fog_density, r.overcast);
    let stand = origin - toward * 45.0;
    let stand = Vec3::new(stand.x, g.hf.height_at(stand.x, stand.z) + 2.0, stand.z);
    rig.observe(&mut r, stand, origin + Vec3::Y * 4.0, 60.0);
    let text = note(&r, "toward a low sun through the burning wreck's smoke");
    rig.still(&mut r, "smoke_low_sun", &text)?;
    let tiger = g.player.pos;
    rig.observe(&mut r, tiger - toward * 18.0 + Vec3::Y * 1.6, tiger + toward * 60.0 + Vec3::Y * 4.0, 70.0);
    let text = note(&r, "toward a low sun over the Tiger in haze");
    rig.still(&mut r, "tiger_low_sun", &text)?;
    // Inside: three rounds' fumes, the turret turned so its front faces the sun.
    g.debug_set_he_loaded(false);
    g.debug_lay_gun_at(tiger + toward * 300.0 + Vec3::Y * 2.0);
    step(&mut g, &mut r, 60, &idle);
    for _ in 0..3 {
        g.debug_fire_laid(&mut r);
        step(&mut g, &mut r, 60, &idle);
    }
    weather(&mut r, 0.0, 0.004);
    g.station = Station::Commander;
    g.commander_outside = false;
    g.hatch_open = false;
    rig.station_camera();
    step(&mut g, &mut r, 20, &idle);
    weather(&mut r, 0.0, 0.004);
    let text = note(&r, "the commander's post after three rounds, the turret's front toward a low sun");
    rig.still(&mut r, "cabin_low_sun", &text)?;
    let text = note(&r, "the same, the next still (flicker)");
    rig.still(&mut r, "cabin_low_sun_next", &text)?;
    // The hatch open at noon.
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    g.hatch_open = true;
    step(&mut g, &mut r, 30, &idle);
    weather(&mut r, 0.0, 0.004);
    let text = note(&r, "the commander's post, hatch open at noon, after three rounds");
    rig.still(&mut r, "cabin_hatch_noon", &text)?;
    g.station = Station::Loader;
    rig.station_camera();
    step(&mut g, &mut r, 20, &idle);
    weather(&mut r, 0.0, 0.004);
    let text = note(&r, "from the loader's post: the commander's hatch open at noon, the fumes in the turret");
    rig.still(&mut r, "loader_hatch_noon", &text)?;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.shafts.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixture (banished enemies, held fire, one burning T-34, three rounds fired into the open). Clock, cloud and haze set by hand; sky clock pinned. Observer and station cameras. No image grading."
    }))?)?;
    Ok(())
}

/// `raymarch-cabin PREFIX`: the commander's post shut, as a player sees it on the act's own
/// weather: the vision blocks, the cabin medium at rest, then while the tank drives (two pairs of
/// consecutive frames, no history reset between them, so their difference is the frame-to-frame
/// flicker); then the gunner's, driver's and radio operator's posts at rest, the gunner's
/// two-pass eyepiece composite, and the gunner's sight on the move (its jolt rate). Short by design (one process, a few hundred frames).
pub fn run_cabin(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_commander.png")).exists(),
        "preserving existing capture; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    rig.clock(&mut r, "1942-12-21T09:00:00Z")?;
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    g.station = Station::Commander;
    g.commander_outside = false;
    g.hatch_open = false;
    rig.station_camera();
    step(&mut g, &mut r, 40, &idle);
    let note = |r: &crate::renderer::Renderer, what: &str| format!("{what}; cloud cover {:.2}, haze {:.4}/m, precipitation {:.2}",
        r.overcast, r.fog_density, r.precipitation);
    let text = note(&r, "the commander's post at noon, hatch shut, at rest");
    rig.still(&mut r, "commander", &text)?;
    let path = format!("{prefix}_commander_next.png");
    r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
    rig.shots.push(serde_json::json!({ "name": "commander_next", "path": path, "note": "the next frame (flicker)" }));
    // Drive: every frame simulated and rendered, so the histories accumulate in motion.
    let drive = TankInput { drive: 1.0, ..TankInput::default() };
    for _ in 0..150 {
        g.update(&drive, 1.0 / 60.0, &mut r);
        r.render_offscreen(&device, &queue);
    }
    let speed = g.player.speed;
    let path = format!("{prefix}_commander_driving.png");
    r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
    rig.shots.push(serde_json::json!({ "name": "commander_driving", "path": path, "speed_m_s": speed,
        "note": note(&r, "the same post after 2.5 s at full throttle, every frame rendered") }));
    g.update(&drive, 1.0 / 60.0, &mut r);
    let path = format!("{prefix}_commander_driving_next.png");
    r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
    rig.shots.push(serde_json::json!({ "name": "commander_driving_next", "path": path, "note": "one game step later (flicker in motion)" }));
    // The other optics at rest: the gunner's sight, the driver's and the radio operator's posts.
    step(&mut g, &mut r, 60, &idle);
    for (station, name) in [(Station::Gunner, "gunner"), (Station::Driver, "driver"), (Station::Radioman, "radioman")] {
        g.station = station;
        step(&mut g, &mut r, 40, &idle);
        let text = note(&r, &format!("the {name}'s post at rest"));
        rig.still(&mut r, name, &text)?;
    }
    // The gunner's eyepiece composite as the live loop draws it: the optic pass, then the
    // interior pass around the eyepiece, which reuses the optic pass's far maps and sky.
    g.station = Station::Gunner;
    step(&mut g, &mut r, 40, &idle);
    if g.optic_composite_active() {
        let saved = (r.camera, r.reticle, r.optic_mask, r.precipitation, r.weather_shelter);
        r.begin_coop_shadow_frame(g.player.center());
        r.weather_shelter = 1.0;
        g.configure_crew_render_view(crate::lan::LanRole::Gunner, true, &mut r);
        r.set_active_view(0);
        r.render_optic_snapshot(&device, &queue);
        let (eye, forward, fov) = g.gunner_interior_camera();
        r.camera.pos = eye;
        r.camera.set_forward(forward);
        r.camera.fov_y = fov;
        r.camera.zfar = crate::PROTECTED_INTERIOR_FAR_M;
        r.precipitation = 0.0;
        r.reticle = false;
        r.optic_mask = 4.0;
        g.configure_crew_render_view(crate::lan::LanRole::Gunner, false, &mut r);
        r.set_active_view(3);
        r.rt_skip = true;
        r.set_coop_shadow_reuse(true);
        let path = format!("{prefix}_gunner_composite.png");
        r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
        let stats = r.raymarch_stats().map(|s| format!("{s:?}"));
        r.rt_skip = false;
        r.set_coop_shadow_reuse(false);
        r.end_coop_shadow_frame();
        (r.camera, r.reticle, r.optic_mask, r.precipitation, r.weather_shelter) = saved;
        rig.shots.push(serde_json::json!({ "name": "gunner_composite", "path": path, "raymarch": stats,
            "note": "the live loop's two-pass eyepiece composite; the interior pass reuses the optic pass's far cascade, sky cover and sky cache" }));
    }
    // The gunner's sight on the move: the view's frame-to-frame turn while driving, split into
    // the steady part (a 9-frame running mean) and the jolt about it (KESSEL_SIGHT_DAMPING_S).
    g.station = Station::Gunner;
    step(&mut g, &mut r, 30, &idle);
    let mut forwards = Vec::new();
    for _ in 0..300 {
        g.update(&drive, 1.0 / 60.0, &mut r);
        forwards.push(r.camera.forward());
    }
    let turns: Vec<f32> = forwards.windows(2).map(|w| w[0].angle_between(w[1]) * 60.0).collect();
    let mean_turn = turns.iter().sum::<f32>() / turns.len().max(1) as f32;
    let jolt = (turns.windows(9).map(|w| {
        let m = w.iter().sum::<f32>() / 9.0;
        (w[4] - m).powi(2)
    }).sum::<f32>() / turns.len().saturating_sub(8).max(1) as f32).sqrt();
    let path = format!("{prefix}_gunner_driving.png");
    r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
    rig.shots.push(serde_json::json!({ "name": "gunner_driving", "path": path,
        "sight_turn_mean_deg_s": mean_turn.to_degrees(), "sight_jolt_rms_deg_s": jolt.to_degrees(),
        "speed_m_s": g.player.speed, "damping_s": std::env::var("KESSEL_SIGHT_DAMPING_S").ok(),
        "note": "the gunner's sight after 5 s at full throttle; the view's turn rate and its jolt about a 9-frame mean" }));
    println!("[raymarch-cabin] gunner sight driving: mean turn {:.2} deg/s, jolt rms {:.2} deg/s, speed {:.1} m/s",
        mean_turn.to_degrees(), jolt.to_degrees(), g.player.speed);
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-cabin.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "skip": std::env::var("KESSEL_RM_SKIP").ok(),
        "disclosure": "Staged fixture (banished enemies, held fire). The act's own weather; clock set to 09:00 UTC and pinned. The game's commander camera. No image grading."
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=snow`: snow lying on the Tiger over time (`snow_cover`): the clean hull,
/// after two minutes of steady snowfall, after a fast drive, after a shot, after a traverse;
/// each from behind and from above the hull, the sky clock pinned. The world keeps its winter cover.
fn run_snow(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_0_clean.png")).exists(),
        "preserving existing showcase; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new(), observer: None };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let idle = TankInput::default();
    // Steady snowfall from a grey midday sky (storm_weather intensity 1).
    r.set_weather_override(Some(1.0));
    rig.clock(&mut r, "1942-12-21T09:30:00Z")?;
    // The same light for every still: the series spans three simulated minutes, which the day
    // cycle (FULL_DAY_S of play for the whole day) would otherwise carry into sunset.
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    g.debug_set_snow_cover(0.0, crate::snow_cover::WINTER_WORLD_LOAD);
    step(&mut g, &mut r, 30, &idle);
    let follow = |g: &Game| {
        let p = g.player.pos;
        let f = g.player.forward();
        let side = f.cross(Vec3::Y).normalize_or_zero();
        (p - f * 6.5 + side * 3.0 + Vec3::Y * 4.2, p + Vec3::Y * 1.3 + f * 1.0)
    };
    // A high three-quarter view from the other side shows the deck, turret roof and fenders.
    let above = |g: &Game| {
        let p = g.player.pos;
        let f = g.player.forward();
        let side = f.cross(Vec3::Y).normalize_or_zero();
        (p - f * 4.0 - side * 4.5 + Vec3::Y * 8.5, p + Vec3::Y * 1.2)
    };
    let still = |g: &Game, r: &mut crate::renderer::Renderer, rig: &mut Rig, name: &str, note: &str| -> anyhow::Result<()> {
        let note = format!("{note}; Tiger snow load {:.2}", g.player_snow_cover());
        let (eye, at) = follow(g);
        rig.observe(r, eye, at, 55.0);
        rig.still(r, name, &note)?;
        let (eye, at) = above(g);
        rig.observe(r, eye, at, 50.0);
        rig.still(r, &format!("{name}_top"), &format!("{note}; from above"))
    };
    still(&g, &mut r, &mut rig, "0_clean", "the clean hull at the start")?;
    // Two minutes of snowfall, rendering once a second so frame-driven state stays live.
    for i in 0..7200u32 {
        g.update(&idle, 1.0 / 60.0, &mut r);
        if i % 60 == 59 { r.render_offscreen(&device, &queue); }
    }
    still(&g, &mut r, &mut rig, "1_after_2min_snowfall", "after two minutes of steady snowfall")?;
    let drive = TankInput { drive: 1.0, ..TankInput::default() };
    step(&mut g, &mut r, 480, &drive);
    step(&mut g, &mut r, 90, &idle);
    still(&g, &mut r, &mut rig, "2_after_fast_drive", "after eight seconds at full throttle")?;
    for _ in 0..3600u32 { g.update(&idle, 1.0 / 60.0, &mut r); }
    let before_shot = g.player_snow_cover();
    g.debug_set_he_loaded(false);
    g.debug_fire_laid(&mut r);
    step(&mut g, &mut r, 120, &idle);
    still(&g, &mut r, &mut rig, "3_after_shot", &format!("one more minute of snowfall (load {before_shot:.2}), then a shot"))?;
    let traverse = TankInput { traverse: 1.0, ..TankInput::default() };
    step(&mut g, &mut r, 240, &traverse);
    step(&mut g, &mut r, 30, &idle);
    still(&g, &mut r, &mut rig, "4_after_traverse", "after four seconds of turret traverse")?;
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.raymarch-showcase.snow.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "shots": rig.shots,
        "disclosure": "Staged fixture (banished enemies). Weather overridden to steady snowfall; the Tiger starts bare, the world at its winter load. Observer camera. No image grading."
    }))?)?;
    Ok(())
}
