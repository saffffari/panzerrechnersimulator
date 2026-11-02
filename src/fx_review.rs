//! `fx-review PREFIX [frames]`: the fire, spark and explosion effects as the owner sees them
//! under a display look (`KESSEL_LOOK`), staged on the Canyon v2 winter workshop with the
//! showcase's fixtures (`raymarch_showcase.rs`). Three parts, one receipt:
//!
//! 1. The burning T-34 from the showcase's observer camera at dusk and at night, rendered at the
//!    internal grid (one PNG pixel per frame pixel), for judging the fire's glow.
//! 2. Three physically different strikes on the same hull, each from the real projectile path
//!    (laid, fired, resolved by the armour solver): an 88 mm AP round near the plate normal, the
//!    same round at a grazing angle (a ricochet), and an 88 mm HE round. Two stills each, at the
//!    strike's first exposure and a quarter second on.
//! 3. The gunner's eyepiece frame (optic pass, then the cabin with its relayed periscopes) across a
//!    catastrophic T-34 kill 30 m ahead, every frame fenced with the GPU pass timestamps
//!    (`KESSEL_GPU_PROFILE=1`): per-pass median, p95 and worst, and the effect receipts of the
//!    frame whose effect passes cost the most.
//!
//! Observer cameras, not a player run; weather and clock overridden and named in the receipt.
//! No image grading; the look is whatever `KESSEL_LOOK` selects, so the same prefix can be run
//! with the look unset for an A/B.
use crate::{
    act_manifest, celestial_sky, ephemeris,
    game::{Arc, CrewRole, Game, Station, TankInput},
    graphics, world,
};
use anyhow::Context;
use glam::{Vec2, Vec3};
use std::fs;

const TIGER: Vec2 = Vec2::new(-450.0, -410.0);
const WRECK: Vec2 = Vec2::new(-465.0, -394.0);

fn step(g: &mut Game, r: &mut crate::renderer::Renderer, frames: u32, input: &TankInput) {
    for _ in 0..frames { g.update(input, 1.0 / 60.0, r); }
}

fn clock(r: &mut crate::renderer::Renderer, utc: &str) -> anyhow::Result<()> {
    let instant = ephemeris::UtcInstant::parse_iso(utc)?;
    r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    Ok(())
}

/// `KESSEL_FX_REVIEW_CLOCK=dusk|blue|night|<UTC ISO>` sets the strike and kill parts' clock (the
/// fire part keeps its dusk and night pair). Stalingrad, 21 December 1942: `dusk` is the sun 3 deg
/// up (12:40 UTC, as the fire part), `blue` the blue hour with the sun about 5 deg under (13:40
/// UTC), `night` the moonlit night of the other parts (19:30 UTC).
fn part_clock(default: &str) -> String {
    match std::env::var("KESSEL_FX_REVIEW_CLOCK").ok().as_deref() {
        Some("dusk") => "1942-12-21T12:40:00Z".to_owned(),
        Some("blue") => "1942-12-21T13:40:00Z".to_owned(),
        Some("night") => "1942-12-21T19:30:00Z".to_owned(),
        Some(iso) => iso.to_owned(),
        None => default.to_owned(),
    }
}

/// `KESSEL_FX_REVIEW_FILM=1`: every frame of each strike and of the kill, as rendered, to a 60 fps
/// H.264 clip beside the stills (`<prefix>_<name>.mp4`); the frames are staged in the system temp
/// directory and removed after the encode. In film mode the stills are frames of the clip (no
/// history reset, no convergence pass), so the fire's history runs on as in play.
struct Film { dir: std::path::PathBuf, frames: u32, out: String }

impl Film {
    fn start(prefix: &str, name: &str) -> anyhow::Result<Option<Film>> {
        if std::env::var("KESSEL_FX_REVIEW_FILM").as_deref() != Ok("1") { return Ok(None); }
        let dir = std::env::temp_dir().join(format!("fx_review_{}_{name}", std::process::id()));
        fs::create_dir_all(&dir)?;
        Ok(Some(Film { dir, frames: 0, out: format!("{prefix}_{name}.mp4") }))
    }
    /// The path of the next frame; `still` names a still to copy this frame to as well.
    fn frame(&mut self) -> String {
        let path = self.dir.join(format!("{:05}.png", self.frames));
        self.frames += 1;
        path.to_string_lossy().into_owned()
    }
    fn last_frame(&self) -> std::path::PathBuf { self.dir.join(format!("{:05}.png", self.frames.saturating_sub(1))) }
    fn finish(self) -> anyhow::Result<String> {
        let status = std::process::Command::new("ffmpeg").args([
            "-v", "error", "-nostdin", "-y", "-framerate", "60",
            "-i", &format!("{}/%05d.png", self.dir.display()),
            "-an", "-c:v", "libx264", "-preset", "medium", "-crf", "17", "-pix_fmt", "yuv420p", &self.out,
        ]).status().context("ffmpeg")?;
        anyhow::ensure!(status.success(), "ffmpeg failed encoding {}", self.out);
        let _ = fs::remove_dir_all(&self.dir);
        println!("[fx-review] {} ({} frames at 60 fps)", self.out, self.frames);
        Ok(self.out)
    }
}

fn observe(r: &mut crate::renderer::Renderer, eye: Vec3, target: Vec3, fov_deg: f32) {
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

struct Stills<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    shots: Vec<serde_json::Value>,
}

impl Stills<'_> {
    /// One still at the internal grid from the observer camera set just before, after the headless
    /// transport convergence (the fire's history is reset so the frame is its own).
    fn still(&mut self, r: &mut crate::renderer::Renderer, name: &str, note: &str) -> anyhow::Result<()> {
        r.reset_history();
        let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 20)?;
        let path = format!("{}_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, r.iw, r.ih)?;
        let stats = r.raymarch_stats().map(|s| format!("{s:?}"));
        self.shots.push(serde_json::json!({
            "name": name, "path": path, "note": note, "eye": r.camera.pos.to_array(),
            "clock_utc": r.sky_clock.now().to_iso(), "exposure": settled.measured_exposure,
            "raymarch": stats, "sparks": r.debug_weapon_particle_receipt(),
        }));
        println!("[fx-review] {path}");
        Ok(())
    }
    /// A still that is a frame of a clip: the file copied, the receipt row as for `still`.
    fn frame_still(&mut self, r: &mut crate::renderer::Renderer, film: &Film, name: &str, note: &str) -> anyhow::Result<()> {
        let path = format!("{}_{name}.png", self.prefix);
        fs::copy(film.last_frame(), &path)?;
        let stats = r.raymarch_stats().map(|s| format!("{s:?}"));
        self.shots.push(serde_json::json!({
            "name": name, "path": path, "note": format!("{note}; frame {} of {}", film.frames - 1, film.out),
            "eye": r.camera.pos.to_array(), "clock_utc": r.sky_clock.now().to_iso(),
            "raymarch": stats, "sparks": r.debug_weapon_particle_receipt(),
        }));
        println!("[fx-review] {path}");
        Ok(())
    }
}

/// One strike on the wreck: the Tiger parked at `from` (map), the gun laid on `aim`, the round
/// fired and flown, then two stills from `eye` toward `look`.
#[allow(clippy::too_many_arguments)]
fn strike(stills: &mut Stills, g: &mut Game, r: &mut crate::renderer::Renderer, idle: &TankInput,
    name: &str, from: Vec2, he: bool, aim: Vec3, eye: Vec3, look: Vec3, note: &str) -> anyhow::Result<()> {
    let wreck = Vec2::new(aim.x, aim.z);
    let heading = (wreck - from).to_angle();
    // The Tiger's heading convention is the game's; the gun is laid on the point regardless.
    g.debug_place_tiger(from, heading);
    g.debug_set_he_loaded(he);
    // The hull settles on its springs after the placement; the lay is taken on the settled hull.
    step(g, r, 60, idle);
    g.debug_lay_gun_at(aim);
    step(g, r, 6, idle);
    g.debug_fire_laid(r);
    let (muzzle, _) = g.player.muzzle();
    let flight = (muzzle.distance(aim) / if he { 790.0 } else { 770.0 } * 60.0).round() as u32;
    if let Some(mut film) = Film::start(&stills.prefix, name)? {
        // From the discharge to a second and a half after the strike, every frame from `eye`.
        observe(r, eye, look, 55.0);
        r.reset_history();
        for frame in 0..flight + 4 + 90 {
            step(g, r, 1, idle);
            observe(r, eye, look, 55.0);
            let path = film.frame();
            r.render_to_png(stills.device, stills.queue, &path, r.iw, r.ih)?;
            if frame + 1 == flight + 4 { stills.frame_still(r, &film, &format!("{name}_070ms"), &format!("{note}; +0.07 s"))?; }
            if frame + 1 == flight + 15 { stills.frame_still(r, &film, &format!("{name}_250ms"), &format!("{note}; +0.25 s"))?; }
        }
        film.finish()?;
        step(g, r, 60, idle);
        return Ok(());
    }
    step(g, r, flight + 4, idle);
    observe(r, eye, look, 55.0);
    stills.still(r, &format!("{name}_070ms"), &format!("{note}; +0.07 s"))?;
    step(g, r, 11, idle);
    observe(r, eye, look, 55.0);
    stills.still(r, &format!("{name}_250ms"), &format!("{note}; +0.25 s"))?;
    // Let the strike's records expire before the next one so the stills do not overlap.
    step(g, r, 150, idle);
    Ok(())
}

/// `KESSEL_FX_REVIEW_PARTS=fire,strikes,flash,cabin,tiger,cabin_hit,burnout,kill` selects the parts (all by default);
/// `KESSEL_FX_REVIEW_CLOCK` the strike and kill clock, `KESSEL_FX_REVIEW_FILM=1` the clips.
fn part(name: &str) -> bool {
    match std::env::var("KESSEL_FX_REVIEW_PARTS") {
        Ok(list) => list.split(',').any(|p| p.trim() == name),
        Err(_) => true,
    }
}

pub fn run(prefix: &str, frames: u32) -> anyhow::Result<()> {
    anyhow::ensure!(graphics::shader_backend_enabled(), "fx-review requires KESSEL_RENDERER=raymarch");
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}.json")).exists(),
        "preserving existing review; choose a new prefix");
    if !crate::gpu_profile::requested() {
        println!("[fx-review] KESSEL_GPU_PROFILE is not 1: part 3 reports wall time only");
    }
    // `KESSEL_FX_REVIEW_WORLD=arena`: the act-0 training arena (a 1 km square of snow, cover and
    // pines) instead of the Canyon act, whose 30,000 destructibles and sky occluder take a minute
    // of CPU to build before the first frame. The fixtures are the same tanks at the same
    // offsets from the arena's spawn; the receipt names the world.
    let arena = std::env::var("KESSEL_FX_REVIEW_WORLD").as_deref() == Ok("arena");
    let manifest = if arena { "arena (world::act(0))".to_owned() } else {
        let path = crate::canyon::manifest_path();
        act_manifest::select_runtime_manifest(Some(path.clone()))?;
        path.display().to_string()
    };
    let (tiger, wreck) = if arena {
        // 220 m up the field from the spawn, past the seeded infantry group that stands at it.
        let site = world::act(0).spawn + Vec2::new(0.0, 220.0);
        (site, site + (WRECK - TIGER))
    } else { (TIGER, WRECK) };
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let built = std::time::Instant::now();
    let (mut r, mut g) = if arena { Game::build_act(&device, &queue, false, &world::act(0))? }
        else { Game::build_playtest_act(&device, &queue, false, &world::act(1))? };
    println!("[fx-review] world built in {:.1} s ({manifest})", built.elapsed().as_secs_f64());
    let mut stills = Stills { device: &device, queue: &queue, prefix: prefix.to_owned(), shots: Vec::new() };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(tiger, 0.0);
    let burning = g.debug_stage_burning_t34(wreck.x, wreck.y).context("fixture needs a T-34")?;
    let idle = TankInput::default();

    // Part 1: the burning wreck, eight seconds into its fire, at dusk and at night.
    r.set_weather_override(Some(0.0));
    clock(&mut r, "1942-12-21T12:40:00Z")?;
    step(&mut g, &mut r, 480, &idle);
    let origin = g.enemies[burning].pos;
    let deck = origin + Vec3::Y * 1.8;
    if part("fire") {
        observe(&mut r, origin + Vec3::new(10.0, 1.7, -12.0), deck, 58.0);
        stills.still(&mut r, "fire_dusk", "burning T-34, clear sky, sun 3 deg; volumetric flame, embers, soot")?;
        clock(&mut r, "1942-12-21T19:30:00Z")?;
        step(&mut g, &mut r, 60, &idle);
        observe(&mut r, origin + Vec3::new(10.0, 1.7, -12.0), deck, 58.0);
        stills.still(&mut r, "fire_night", "the same fire under a clear moonlit night")?;
        observe(&mut r, origin + Vec3::new(4.5, 2.6, -4.5), deck, 62.0);
        stills.still(&mut r, "fire_night_deck", "the night fire from the deck: tongues, embers")?;
    }

    // The 88's muzzle at dusk, as the showcase frames it (he_muzzle): the flash the owner likes.
    if part("flash") {
        clock(&mut r, "1942-12-21T12:40:00Z")?;
        g.debug_place_tiger(TIGER, 0.0);
        step(&mut g, &mut r, 30, &idle);
        let tiger = g.player.pos;
        let forward = g.player.forward();
        let impact = tiger + forward * 45.0;
        g.debug_set_he_loaded(true);
        g.debug_lay_gun_at(Vec3::new(impact.x, g.hf.height_at(impact.x, impact.z) + 0.3, impact.z));
        step(&mut g, &mut r, 30, &idle);
        g.debug_fire_laid(&mut r);
        let (muzzle, _) = g.player.muzzle();
        step(&mut g, &mut r, 2, &idle);
        observe(&mut r, tiger + Vec3::new(-7.0, 2.4, -3.0) + forward * 2.0, muzzle + forward * 3.0, 60.0);
        stills.still(&mut r, "flash_dusk", "88 mm HE discharge at +2 frames: the muzzle flash and its glare")?;
        step(&mut g, &mut r, 240, &idle);
    }

    // Part 2: three strikes at night on a cold, knocked-out hull 45 m from the burning one, the
    // camera looking away from the fire, so the sparks carry the frame. The hull faces +x
    // (heading 0); its left flank is -z.
    if part("strikes") {
        clock(&mut r, &part_clock("1942-12-21T19:30:00Z"))?;
        let cold = g.debug_stage_cold_wreck(wreck.x + 45.0, wreck.y, 0.0).context("fixture needs a third T-34")?;
        step(&mut g, &mut r, 30, &idle);
        let origin = g.enemies[cold].pos;
        let hull = origin + Vec3::Y * 1.1;
        // `KESSEL_FX_REVIEW_EYE=x,y,z` moves the observer (metres from the struck hull's origin;
        // the default looks from the south-west side).
        let eye = origin + std::env::var("KESSEL_FX_REVIEW_EYE").ok().and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|c| c.trim().parse().ok()).collect();
            (p.len() == 3).then(|| Vec3::new(p[0], p[1], p[2]))
        }).unwrap_or(Vec3::new(-4.5, 1.5, -6.5));
        strike(&mut stills, &mut g, &mut r, &idle, "strike_ap_normal",
            Vec2::new(origin.x, origin.z - 60.0), false, hull, eye, hull,
            "88 mm AP from 60 m square on the left flank: near-normal strike")?;
        strike(&mut stills, &mut g, &mut r, &idle, "strike_ap_oblique",
            Vec2::new(origin.x - 55.0, origin.z - 18.0), false, hull, eye, hull,
            "88 mm AP from 58 m at some 70 deg off the flank: a grazing strike, ricochet")?;
        strike(&mut stills, &mut g, &mut r, &idle, "strike_he",
            Vec2::new(origin.x, origin.z - 60.0), true, hull, eye, hull,
            "88 mm HE from 60 m square on the flank: exterior detonation on the plate")?;
    }

    // The commander's post at night: the lamps and their glare, the game's own camera.
    if part("cabin") {
        clock(&mut r, "1942-12-21T19:30:00Z")?;
        g.debug_place_tiger(tiger, 0.0);
        g.station = Station::Commander;
        g.commander_outside = false;
        step(&mut g, &mut r, 30, &idle);
        stills.still(&mut r, "cabin_night", "commander's post at night: dome and dial lamps, their glare")?;
        g.station = Station::Gunner;
    }
    // Part 4: rounds on the player's Tiger from a live T-34 through the normal return-fire path
    // (`debug_enemy_fire_at_player`), the observer outside: one square on the left flank from
    // 60 m abeam, one on the glacis from 70 m ahead. Film mode films each; otherwise two stills.
    if part("tiger") {
        clock(&mut r, &part_clock("1942-12-21T13:05:00Z"))?;
        g.debug_place_tiger(tiger, 0.0);
        step(&mut g, &mut r, 60, &idle);
        let forward = g.player.forward();
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let centre = g.player.center();
        let shots = [
            ("tiger_flank", centre - right * 60.0, centre - right * 9.0 - forward * 7.0 + Vec3::Y * 2.0,
                "76 mm from a T-34 60 m abeam, square on the Tiger's left flank"),
            ("tiger_glacis", centre + forward * 70.0 + right * 12.0, centre + forward * 9.0 - right * 7.0 + Vec3::Y * 2.2,
                "76 mm from a T-34 70 m ahead and a little right, on the glacis"),
        ];
        for (name, from, eye, note) in shots {
            let heading = (centre.x - from.x).atan2(centre.z - from.z);
            let live = g.debug_stage_live_t34(from.x, from.z, heading).context("fixture needs a live T-34")?;
            g.debug_hold_enemy_fire();
            step(&mut g, &mut r, 30, &idle);
            g.enemies[live].reload = 0.0;
            anyhow::ensure!(g.debug_enemy_fire_at_player(live), "the T-34 did not fire");
            g.debug_hold_enemy_fire();
            let flight = (from.distance(centre) / 660.0 * 60.0).round() as u32;
            if let Some(mut film) = Film::start(&stills.prefix, name)? {
                observe(&mut r, eye, centre, 50.0);
                r.reset_history();
                for frame in 0..flight + 4 + 90 {
                    step(&mut g, &mut r, 1, &idle);
                    observe(&mut r, eye, centre, 50.0);
                    let path = film.frame();
                    r.render_to_png(&device, &queue, &path, r.iw, r.ih)?;
                    if frame + 1 == flight + 4 { stills.frame_still(&mut r, &film, &format!("{name}_070ms"), &format!("{note}; +0.07 s"))?; }
                    if frame + 1 == flight + 15 { stills.frame_still(&mut r, &film, &format!("{name}_250ms"), &format!("{note}; +0.25 s"))?; }
                }
                film.finish()?;
            } else {
                step(&mut g, &mut r, flight + 4, &idle);
                observe(&mut r, eye, centre, 50.0);
                stills.still(&mut r, &format!("{name}_070ms"), &format!("{note}; +0.07 s"))?;
                step(&mut g, &mut r, 11, &idle);
                observe(&mut r, eye, centre, 50.0);
                stills.still(&mut r, &format!("{name}_250ms"), &format!("{note}; +0.25 s"))?;
            }
            step(&mut g, &mut r, 90, &idle);
        }
    }
    // Part 5 (`cabin_hit`): the inside of the Tiger as a solid shot comes through the hull side
    // at the radio operator's post (the crew-solo fixture's wounding lane, raked rearward), seen
    // from the commander's own post inside with the living crew at theirs. Film mode films 110
    // frames from the round's launch; otherwise two stills, 2 and 10 frames after the strike.
    if part("cabin_hit") {
        clock(&mut r, &part_clock("1942-12-21T13:05:00Z"))?;
        g.debug_place_tiger(tiger, 0.0);
        g.enable_singleplayer_crew();
        g.station = Station::Commander;
        g.commander_outside = false;
        step(&mut g, &mut r, 90, &idle);
        // An observer inside the compartment a metre behind the radio operator and a little
        // above him, in the hull's frame, looking down at his post (from the cupola the bow post
        // hides under the turret ring; the player's own eye on the walk to him sits inside the
        // seat frames). The cabin's light, haze and shelter stay as the game sets them; the
        // player stays at the cupola, clear of the cone.
        let strike_frame = 12u32;
        let mut strike = serde_json::Value::Null;
        let man = g.debug_crew_solo_man_point(CrewRole::Radioman);
        let hull = g.player.hull_model();
        let eye = man + hull.transform_vector3(Vec3::new(0.0, 0.45, -1.1));
        let look = |r: &mut crate::renderer::Renderer| { r.camera.pos = eye; r.camera.set_forward(man + Vec3::Y * 0.1 - eye); r.camera.fov_y = 85f32.to_radians(); };
        if let Some(mut film) = Film::start(&stills.prefix, "cabin_hit")? {
            look(&mut r);
            r.reset_history();
            for frame in 0..110 {
                if frame == strike_frame {
                    strike = g.debug_crew_solo_strike_raked(CrewRole::Radioman, false, Arc::Side, 0.3, 35.0);
                }
                step(&mut g, &mut r, 1, &idle);
                look(&mut r);
                let path = film.frame();
                r.render_to_png(&device, &queue, &path, r.iw, r.ih)?;
                if frame == strike_frame + 2 { stills.frame_still(&mut r, &film, "cabin_hit_2f", "the hull side holed at the radio operator's post, +2 frames, from the commander's post")?; }
                if frame == strike_frame + 10 { stills.frame_still(&mut r, &film, "cabin_hit_10f", "the same, +10 frames")?; }
                if frame == strike_frame + 60 { stills.frame_still(&mut r, &film, "cabin_hit_60f", "the same, a second on")?; }
            }
            film.finish()?;
        } else {
            step(&mut g, &mut r, strike_frame, &idle);
            strike = g.debug_crew_solo_strike_raked(CrewRole::Radioman, false, Arc::Side, 0.3, 35.0);
            step(&mut g, &mut r, 2, &idle);
            look(&mut r);
            stills.still(&mut r, "cabin_hit_2f", "the hull side holed at the radio operator's post, +2 frames, from the commander's post")?;
            step(&mut g, &mut r, 8, &idle);
            look(&mut r);
            stills.still(&mut r, "cabin_hit_10f", "the same, +10 frames")?;
        }
        stills.shots.push(serde_json::json!({"name": "cabin_hit_strike", "strike": strike}));
    }
    // Part 6 (`burnout`): a live T-34 killed through its ammunition with the catastrophic roll
    // primed, from an observer 14 m off: the stowage's burn jetting from the breach and the
    // holes, the ring venting, the turret going, the plume. Film mode films five seconds.
    if part("burnout") {
        clock(&mut r, &part_clock("1942-12-21T13:05:00Z"))?;
        g.debug_place_tiger(tiger, 0.0);
        let forward = g.player.forward();
        let right = Vec3::new(-forward.z, 0.0, forward.x);
        let target_pos = g.player.pos + forward * 30.0 + right * 7.0;
        let live = g.debug_stage_live_t34(target_pos.x, target_pos.z, 1.2).context("fixture needs a live T-34")?;
        g.debug_hold_enemy_fire();
        step(&mut g, &mut r, 60, &idle);
        anyhow::ensure!(g.debug_prime_enemy_turret_pop(live), "the catastrophic roll could not be primed");
        let centre = g.enemies[live].center();
        let eye = centre + Vec3::new(-9.0, 2.5, -10.0);
        let kill_frame = 12u32;
        if let Some(mut film) = Film::start(&stills.prefix, "burnout")? {
            observe(&mut r, eye, centre + Vec3::Y * 0.8, 50.0);
            r.reset_history();
            for frame in 0..300 {
                if frame == kill_frame {
                    let ring = g.enemies[live].center() + Vec3::Y * 0.9;
                    anyhow::ensure!(g.debug_catastrophic_kill(live, ring - forward * 1.2, -forward), "the kill did not take");
                }
                step(&mut g, &mut r, 1, &idle);
                observe(&mut r, eye, centre + Vec3::Y * 0.8, 50.0);
                let path = film.frame();
                r.render_to_png(&device, &queue, &path, r.iw, r.ih)?;
                for (at, name, note) in [(30u32, "burnout_05s", "half a second: the breach jetting"), (78, "burnout_13s", "1.3 s: the ring venting"), (120, "burnout_20s", "two seconds: the turret up"), (240, "burnout_40s", "four seconds: the plume")] {
                    if frame == kill_frame + at { stills.frame_still(&mut r, &film, name, note)?; }
                }
            }
            film.finish()?;
        } else {
            step(&mut g, &mut r, kill_frame, &idle);
            let ring = g.enemies[live].center() + Vec3::Y * 0.9;
            anyhow::ensure!(g.debug_catastrophic_kill(live, ring - forward * 1.2, -forward), "the kill did not take");
            for (at, name, note) in [(30u32, "burnout_05s", "half a second: the breach jetting"), (48, "burnout_13s", "1.3 s: the ring venting"), (42, "burnout_20s", "two seconds: the turret up"), (120, "burnout_40s", "four seconds: the plume")] {
                step(&mut g, &mut r, at, &idle);
                observe(&mut r, eye, centre + Vec3::Y * 0.8, 50.0);
                stills.still(&mut r, name, note)?;
            }
        }
        stills.shots.push(serde_json::json!({"name": "burnout_wreck", "turret_pop_position": g.debug_enemy_turret_pop_position(live).map(|p| p.to_array())}));
    }
    if !part("kill") {
        fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "panzer.fx-review.v1", "manifest": manifest, "internal_resolution": [r.iw, r.ih],
            "look": std::env::var("KESSEL_LOOK").ok(),
            "stills": stills.shots}))?)?;
        println!("[fx-review] {} stills, receipt {prefix}.json", stills.shots.len());
        return Ok(());
    }

    // Part 3: the gunner's frame, the Tiger facing the burning wreck 22 m off so the periscopes
    // and the TZF hold its fire grains, across a catastrophic kill 30 m ahead and right of it.
    clock(&mut r, &part_clock("1942-12-21T12:40:00Z"))?;
    let heading = (wreck - tiger).to_angle();
    g.debug_place_tiger(tiger, std::f32::consts::FRAC_PI_2 - heading);
    let forward = g.player.forward();
    let right = Vec3::new(-forward.z, 0.0, forward.x);
    let target_pos = g.player.pos + forward * 30.0 + right * 7.0;
    let live = g.debug_stage_live_t34(target_pos.x, target_pos.z, 1.2).context("fixture needs a live T-34")?;
    g.debug_hold_enemy_fire();
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    g.station = Station::Gunner;
    g.debug_lay_gun_at(g.enemies[live].center());
    step(&mut g, &mut r, 60, &idle);
    anyhow::ensure!(g.optic_composite_active(), "the gunner's eyepiece composite is not active");
    let mut passes: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    let mut walls = Vec::new();
    let mut worst = (0.0f64, serde_json::Value::Null);
    let kill_frame = 12u32;
    let mut captured = false;
    let _ = &mut captured;
    let mut film = Film::start(prefix, "kill_eyepiece")?;
    for frame in 0..frames {
        if frame == kill_frame {
            let ring = g.enemies[live].center() + Vec3::Y * 0.9;
            anyhow::ensure!(g.debug_catastrophic_kill(live, ring - forward * 1.2, -forward), "the kill did not take");
        }
        g.update(&idle, 1.0 / 60.0, &mut r);
        crate::simulation_uploads::drain(&device, &queue)?;
        r.begin_gpu_profile_frame();
        let t0 = std::time::Instant::now();
        // The frame of the blast's first exposure is the still.
        let capture = if frame == kill_frame + 2 { Some(format!("{prefix}_blast_eyepiece_2f.png")) }
            else if frame == kill_frame + 10 { Some(format!("{prefix}_blast_eyepiece_10f.png")) } else { None };
        let (iw, ih) = (r.iw, r.ih);
        if let Some(film) = film.as_mut() {
            let path = film.frame();
            crate::raymarch_showcase::eyepiece_frame(&mut g, &mut r, &device, &queue, Some((&path, iw, ih)))?;
            if let Some(still) = capture.as_deref() {
                fs::copy(&path, still)?;
                captured = true;
                println!("[fx-review] {still} (frame {} of the clip)", film.frames - 1);
            }
        } else if let Some(path) = capture.as_deref() {
            crate::raymarch_showcase::eyepiece_frame(&mut g, &mut r, &device, &queue, Some((path, iw, ih)))?;
            captured = true;
            println!("[fx-review] {path}");
        } else {
            crate::raymarch_showcase::eyepiece_frame(&mut g, &mut r, &device, &queue, None)?;
        }
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        walls.push(t0.elapsed().as_secs_f64() * 1000.0);
        let mut this: std::collections::BTreeMap<String, f64> = Default::default();
        for (label, ms) in r.read_gpu_profile_frame(&device, &queue)? { *this.entry(label).or_default() += ms; }
        let effects = this.get("analytic-streaked-sparks").copied().unwrap_or(0.0)
            + this.get("native-pixel-hdr-effects").copied().unwrap_or(0.0)
            + this.get("rm-points").copied().unwrap_or(0.0);
        if effects > worst.0 {
            worst = (effects, serde_json::json!({"frame": frame, "frames_after_kill": frame as i64 - kill_frame as i64,
                "sparks_ms": this.get("analytic-streaked-sparks"), "grains_ms": this.get("native-pixel-hdr-effects"),
                "points_ms": this.get("rm-points"), "sparks": r.debug_weapon_particle_receipt(), "grains": r.debug_fine_vfx_receipt(),
                "raymarch": r.raymarch_stats().map(|s| format!("{s:?}"))}));
        }
        for (label, ms) in this { passes.entry(label).or_default().push(ms); }
    }
    let clip = match film { Some(film) => Some(film.finish()?), None => None };
    let stats = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.95), v[v.len() - 1], v.iter().sum::<f64>())
    };
    let (p50, p95, max, sum) = stats(&mut walls);
    println!("[fx-review] {} gunner frames @ {}x{} | fenced wall mean {:.2} ms | p50 {p50:.2} p95 {p95:.2} max {max:.2}",
        walls.len(), r.iw, r.ih, sum / walls.len() as f64);
    let mut rows: Vec<_> = passes.iter_mut().map(|(label, v)| {
        let n = v.len();
        let (p50, p95, max, sum) = stats(v);
        (sum / frames as f64, label.clone(), n, p50, p95, max)
    }).collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut pass_rows = Vec::new();
    for (mean, label, n, p50, p95, max) in &rows {
        println!("[fx-review] {label:<30} mean {mean:7.3} ms/frame | on {n:4}/{frames} frames: p50 {p50:7.3} p95 {p95:7.3} max {max:7.3}");
        pass_rows.push(serde_json::json!({"pass": label, "mean_ms": mean, "frames": n, "p50_ms": p50, "p95_ms": p95, "max_ms": max}));
    }
    if !worst.1.is_null() { println!("[fx-review] worst effects frame: {}", worst.1); }

    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.fx-review.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "look": std::env::var("KESSEL_LOOK").ok(), "stills": stills.shots,
        "kill_profile": {"frames": frames, "kill_frame": kill_frame, "clip": clip, "clock_utc": r.sky_clock.now().to_iso(),
            "wall_ms": {"p50": p50, "p95": p95, "max": max},
            "passes": pass_rows, "worst_effects_frame": worst.1},
        "disclosure": "Staged fixtures (banished enemies, held fire, one burning T-34 at ignition load 0.75, one live T-34 killed by the debug hook through the real kill route, scripted gun lays). Weather and clock overridden. Observer cameras for parts 1 and 2; the gunner's own frame for part 3. No image grading."
    }))?)?;
    println!("[fx-review] {} stills, receipt {prefix}.json", stills.shots.len());
    Ok(())
}
