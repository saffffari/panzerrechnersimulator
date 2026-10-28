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
