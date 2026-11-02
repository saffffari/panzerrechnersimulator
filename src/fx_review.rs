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
    game::{Game, Station, TankInput},
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

/// `KESSEL_FX_REVIEW_PARTS=fire,strikes,flash,cabin,kill` selects the parts (all by default).
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
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut stills = Stills { device: &device, queue: &queue, prefix: prefix.to_owned(), shots: Vec::new() };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, 0.0);
    let wreck = g.debug_stage_burning_t34(WRECK.x, WRECK.y).context("fixture needs a T-34")?;
    let idle = TankInput::default();

    // Part 1: the burning wreck, eight seconds into its fire, at dusk and at night.
    r.set_weather_override(Some(0.0));
    clock(&mut r, "1942-12-21T12:40:00Z")?;
    step(&mut g, &mut r, 480, &idle);
    let origin = g.enemies[wreck].pos;
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
        clock(&mut r, "1942-12-21T19:30:00Z")?;
        let cold = g.debug_stage_cold_wreck(WRECK.x + 45.0, WRECK.y, 0.0).context("fixture needs a third T-34")?;
        step(&mut g, &mut r, 30, &idle);
        let origin = g.enemies[cold].pos;
        let hull = origin + Vec3::Y * 1.1;
        let eye = origin + Vec3::new(-4.5, 1.5, -6.5);
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
        g.debug_place_tiger(TIGER, 0.0);
        g.station = Station::Commander;
        g.commander_outside = false;
        step(&mut g, &mut r, 30, &idle);
        stills.still(&mut r, "cabin_night", "commander's post at night: dome and dial lamps, their glare")?;
        g.station = Station::Gunner;
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
    clock(&mut r, "1942-12-21T12:40:00Z")?;
    let heading = (WRECK - TIGER).to_angle();
    g.debug_place_tiger(TIGER, std::f32::consts::FRAC_PI_2 - heading);
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
        if let Some(path) = capture.as_deref() {
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
        let effects = this.get("analytic-thousandfold-sparks").copied().unwrap_or(0.0)
            + this.get("native-pixel-hdr-effects").copied().unwrap_or(0.0)
            + this.get("rm-points").copied().unwrap_or(0.0);
        if effects > worst.0 {
            worst = (effects, serde_json::json!({"frame": frame, "frames_after_kill": frame as i64 - kill_frame as i64,
                "sparks_ms": this.get("analytic-thousandfold-sparks"), "grains_ms": this.get("native-pixel-hdr-effects"),
                "points_ms": this.get("rm-points"), "sparks": r.debug_weapon_particle_receipt(), "grains": r.debug_fine_vfx_receipt(),
                "raymarch": r.raymarch_stats().map(|s| format!("{s:?}"))}));
        }
        for (label, ms) in this { passes.entry(label).or_default().push(ms); }
    }
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
        "kill_profile": {"frames": frames, "kill_frame": kill_frame, "wall_ms": {"p50": p50, "p95": p95, "max": max},
            "passes": pass_rows, "worst_effects_frame": worst.1},
        "disclosure": "Staged fixtures (banished enemies, held fire, one burning T-34 at ignition load 0.75, one live T-34 killed by the debug hook through the real kill route, scripted gun lays). Weather and clock overridden. Observer cameras for parts 1 and 2; the gunner's own frame for part 3. No image grading."
    }))?)?;
    println!("[fx-review] {} stills, receipt {prefix}.json", stills.shots.len());
    Ok(())
}
