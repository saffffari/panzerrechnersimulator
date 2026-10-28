//! `armor-scars PREFIX`: the armor-wound review (docs/ARMOR_SCARS.md). A scripted headless scene
//! on the Canyon v2 winter workshop fires a fixed set of canonical rounds at the player's Tiger
//! and at a live T-34 through the ordinary flight sweep and terminal resolver, then captures the
//! wounds near and far, by day and by night, fresh and cold, before and after the field patch, and
//! after a campaign checkpoint round trip. Each round starts 9 m short of its target carrying the
//! speed, flown distance and time of flight of its nominal range (the catalogue's range law); the
//! last 25 m are flown and swept against the real surfaces. Observer cameras; no image grading.
use super::*;
use crate::{act_manifest, celestial_sky, ephemeris, graphics, world};
use anyhow::Context;
use std::fs;

const TIGER_AT: Vec2 = Vec2::new(-450.0, -410.0);
const LEAD_M: f32 = 9.0;

struct Shot {
    name: &'static str,
    shell: &'static crate::ballistics::ShellSpec,
    range_m: f32,
    /// Aim point and travel direction in the target part's frame.
    local_aim: Vec3,
    local_dir: Vec3,
    turret: bool,
}

const fn shot(name: &'static str, shell: &'static crate::ballistics::ShellSpec, range_m: f32,
    local_aim: Vec3, local_dir: Vec3, turret: bool) -> Shot {
    Shot { name, shell, range_m, local_aim, local_dir, turret }
}

impl Game {
    /// Headless-only (review harness): a cannon round of `shell` LEAD_M short of `target`,
    /// travelling along `dir`, with the speed, flown distance and age of a shot from `range_m`.
    pub fn debug_launch_round(&mut self, shell: &'static crate::ballistics::ShellSpec, from_player: bool,
        target: Vec3, dir: Vec3, range_m: f32, seed: u32) -> u64 {
        let dir = dir.normalize_or_zero();
        let flown = (range_m - LEAD_M).max(0.0);
        let id = self.next_projectile_id;
        self.next_projectile_id = id.wrapping_add(1).max(1);
        let shooter = if from_player { ProjectileShooter::Player } else { ProjectileShooter::Enemy(usize::MAX) };
        let mut projectile = AuthoritativeProjectile::new(id, shooter, shell, target - dir * LEAD_M, dir, seed);
        projectile.distance_m = flown;
        projectile.age_s = shell.time_of_flight(flown);
        projectile.velocity = dir * shell.velocity_at(flown);
        self.projectiles.push(projectile);
        id
    }

    /// Headless-only: a burst of rifle-calibre machine-gun rounds from `range_m` along `dir`,
    /// spread over `spread_m` round `target` (a deterministic disc pattern).
    pub fn debug_launch_burst(&mut self, target: Vec3, dir: Vec3, rounds: u32, spread_m: f32, range_m: f32) {
        let dir = dir.normalize_or_zero();
        let side = dir.cross(Vec3::Y).try_normalize().unwrap_or(Vec3::X);
        let up = side.cross(dir);
        for i in 0..rounds {
            // A burst's dispersion: a hashed, roughly normal spread (no regular pattern).
            let h = |k: u32| crate::armor_damage::position_seed(Vec3::new(i as f32, k as f32, rounds as f32)) as f32 / u32::MAX as f32;
            let radius = spread_m * 0.5 * (-2.0 * (1.0 - h(1) * 0.98).ln()).sqrt();
            let angle = h(2) * std::f32::consts::TAU;
            let aim = target + (side * angle.cos() + up * angle.sin()) * radius;
            let origin = aim - dir * range_m;
            let shot = crate::infantry_combat::Shot {
                shooter: u32::MAX - 7, team: crate::infantry_combat::Team::Soviet,
                origin, direction: dir, range: range_m + 30.0,
                weapon: crate::infantry_combat::Weapon::MachineGun,
            };
            self.launch_small_arm(shot, 0.0, false, None);
        }
    }

    /// Headless-only: a Tiger with no stowed, chambered or loose rounds. A penetration into a
    /// stowed Tiger detonates its racks from almost any plate; the review studies the plate.
    pub fn debug_empty_ammunition(&mut self) {
        self.slot_taken.iter_mut().for_each(|taken| *taken = true);
        self.breech = Breech::Empty;
        self.loose.iter_mut().for_each(|shell| shell.active = false);
    }

    /// Headless-only: put out the cabin and crew fires (the review then studies the next hole
    /// without the heat haze and firelight of a burning compartment).
    pub fn debug_extinguish_cabin(&mut self) {
        self.fire_active = false;
        self.fire = 0.0;
        self.player_burn.extinguish();
        self.cabin_fire.reset();
    }

    /// Headless-only: an intact, crewed, silent T-34 standing at (x, z).
    pub fn debug_stage_live_t34(&mut self, x: f32, z: f32, heading: f32) -> Option<usize> {
        let idx = self.debug_first_t34()?;
        self.debug_place_enemy(idx, x, z, heading);
        self.enemy_active[idx] = true;
        Some(idx)
    }
}

/// `KESSEL_SCAR_TRACE=1` prints every terminal contact (review diagnosis only).
pub(super) fn trace_enabled() -> bool {
    std::env::var("KESSEL_SCAR_TRACE").as_deref() == Ok("1")
}

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    cw: u32,
    ch: u32,
    shots: Vec<serde_json::Value>,
    stills: Vec<serde_json::Value>,
    view: Option<(Vec3, Vec3, f32)>,
    /// The view is inside the vehicle: keep the game's own interior light and cabin medium.
    inside: bool,
}

impl Rig<'_> {
    fn clock(&self, r: &mut Renderer, utc: &str) -> anyhow::Result<()> {
        let instant = ephemeris::UtcInstant::parse_iso(utc)?;
        r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
        Ok(())
    }
    fn apply(&self, r: &mut Renderer) {
        let Some((eye, target, fov)) = self.view else { return };
        r.camera.pos = eye;
        r.camera.set_forward(target - eye);
        r.camera.fov_y = fov.to_radians();
        r.optic_mask = 0.0;
        r.reticle = false;
        if self.inside { return; }
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    }
    fn still(&mut self, g: &Game, r: &mut Renderer, name: &str, note: &str, eye: Vec3, target: Vec3, fov: f32) -> anyhow::Result<()> {
        if let Ok(only) = std::env::var("KESSEL_SCAR_SCENES") {
            if !only.split(',').any(|scene| name.starts_with(scene.trim())) { return Ok(()); }
        }
        self.view = Some((eye, target, fov));
        // The game's own scar push runs in update; re-apply it for this frame's transforms.
        g.drive_armor_scars(r);
        self.apply(r);
        r.reset_history();
        let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 16)?;
        let path = format!("{}_{name}.png", self.prefix);
        r.render_to_png(self.device, self.queue, &path, self.cw, self.ch)?;
        let stats = r.raymarch_stats();
        println!("[armor-scars] {path}: {note}");
        self.stills.push(serde_json::json!({
            "name": name, "path": path, "note": note, "eye": eye.to_array(), "target": target.to_array(),
            "clock_utc": r.sky_clock.now().to_iso(), "exposure": settled.measured_exposure,
            "scars": stats.map(|s| s.scars), "near_patches": stats.map(|s| s.scar_patches),
            "open_holes": stats.map(|s| s.scar_holes), "leak_emitters": stats.map(|s| s.scar_leaks),
        }));
        Ok(())
    }
}

fn step(g: &mut Game, r: &mut Renderer, frames: u32) {
    let idle = TankInput::default();
    for _ in 0..frames { g.update(&idle, 1.0 / 60.0, r); }
}

fn scar_json(scar: &crate::armor_damage::ArmorScar) -> serde_json::Value {
    serde_json::json!({
        "facing": format!("{:?}", scar.facing), "kind": format!("{:?}", scar.kind), "class": format!("{:?}", scar.class),
        "local_mm": scar.local_mm, "radius_mm": scar.radius_mm, "loss_mm": scar.loss_mm,
        "depth_mm": scar.depth_m() * 1000.0, "plate_mm": scar.plate_mm, "aperture_mm": scar.aperture_mm,
        "back_radius_mm": scar.back_radius_mm, "petals": scar.petals, "petal_mm": scar.petal_mm, "plug": scar.plug,
        "obliquity_deg": scar.obliquity_cos().acos().to_degrees(),
    })
}

/// Fire one canonical round at a part of `target` (None: the player's Tiger) and fly it out.
fn fire(g: &mut Game, r: &mut Renderer, rig: &mut Rig, target: Option<usize>, s: &Shot, seed: u32) -> anyhow::Result<()> {
    let tank = match target { Some(i) => g.enemies[i].clone(), None => g.player.clone() };
    let frame = if s.turret { tank.turret_model() } else { tank.hull_model() };
    let aim = frame.transform_point3(s.local_aim);
    let dir = frame.transform_vector3(s.local_dir).normalize_or_zero();
    let field = |g: &Game| match target { Some(i) => g.enemies[i].scars.clone(), None => g.armor_scars.clone() };
    let before = field(g).revision();
    let (hits, pens) = (g.stat_hits_on_player, g.stat_pens_on_player);
    if s.shell.caliber_mm < 20.0 {
        g.debug_launch_burst(aim, dir, 24, 0.32, 40.0);
        step(g, r, 30);
    } else {
        g.debug_launch_round(s.shell, target.is_some(), aim, dir, s.range_m, seed);
        step(g, r, 6);
    }
    let after = field(g);
    let changed = after.revision() != before;
    let newest = after.scars().iter().enumerate()
        .max_by(|a, b| after.struck_at(a.0).total_cmp(&after.struck_at(b.0)).then(a.0.cmp(&b.0))).map(|(_, s)| *s);
    println!("[armor-scars] {} -> {}: {} scars (changed {changed}), newest {:?}", s.name,
        target.map_or("Tiger".into(), |i| format!("T-34 #{i}")), after.scars().len(), newest.map(|s| (s.kind, s.radius_mm, s.aperture_mm)));
    rig.shots.push(serde_json::json!({
        "shot": s.name, "shell": s.shell.name, "range_m": s.range_m, "target": target.map_or("Tiger".into(), |i| format!("T-34 #{i}")),
        "aim_world": aim.to_array(), "dir_world": dir.to_array(), "scar_field_changed": changed,
        "scars_after": after.scars().len(), "newest": newest.as_ref().map(scar_json),
        "tiger_hits_delta": g.stat_hits_on_player - hits, "tiger_pens_delta": g.stat_pens_on_player - pens,
        "target_alive": target.map_or(g.player.alive, |i| g.enemies[i].alive),
    }));
    anyhow::ensure!(changed, "{} left no mark", s.name);
    anyhow::ensure!(!g.capture_catastrophic(None) || target.is_some(), "{} blew the Tiger apart; restage it", s.name);
    Ok(())
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "armor-scars requires KESSEL_RENDERER=raymarch");
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_receipt.json")).exists(),
        "preserving an existing review; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3,
        shots: Vec::new(), stills: Vec::new(), view: None, inside: false };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.god = false;
    g.debug_place_tiger(TIGER_AT, 0.0);
    g.debug_empty_ammunition();
    let tiger_pos = g.player.pos;
    let t34 = g.debug_stage_live_t34(TIGER_AT.x + 16.0, TIGER_AT.y + 24.0, std::f32::consts::FRAC_PI_2)
        .context("fixture needs a T-34")?;
    r.set_weather_override(Some(0.0));
    rig.clock(&mut r, "1942-12-21T10:30:00Z")?;
    step(&mut g, &mut r, 60);

    // Views in the target's own frame, so they follow the settled pose.
    let tiger_view = |g: &Game, eye: Vec3, at: Vec3| (g.player.hull_model().transform_point3(eye), g.player.hull_model().transform_point3(at));
    let t34_view = |g: &Game, eye: Vec3, at: Vec3| (g.enemies[t34].hull_model().transform_point3(eye), g.enemies[t34].hull_model().transform_point3(at));
    let tiger_left_near = (Vec3::new(-5.2, 1.9, -0.4), Vec3::new(-1.8, 1.35, -1.85));
    let tiger_front_near = (Vec3::new(-1.6, 1.9, 6.2), Vec3::new(0.0, 1.2, 3.0));
    let tiger_turret_near = (Vec3::new(2.2, 2.9, -5.0), Vec3::new(0.3, 2.0, -1.8));
    let tiger_mid = (Vec3::new(-11.0, 3.0, 6.0), Vec3::new(-0.6, 1.3, 0.6));
    let tiger_far = (Vec3::new(-38.0, 5.0, 26.0), Vec3::new(0.0, 1.3, 0.0));
    let t34_side_near = (Vec3::new(-4.2, 1.6, 0.4), Vec3::new(-1.5, 1.0, 0.0));
    let t34_front_near = (Vec3::new(-1.2, 1.8, 6.0), Vec3::new(0.0, 1.0, 2.6));
    let t34_mid = (Vec3::new(-10.0, 3.0, 5.0), Vec3::new(0.0, 1.0, 0.3));

    macro_rules! tiger_still {
        ($name:expr, $note:expr, $view:expr, $fov:expr) => {{
            let (eye, at) = tiger_view(&g, $view.0, $view.1);
            rig.still(&g, &mut r, $name, $note, eye, at, $fov)?;
        }};
    }
    macro_rules! t34_still {
        ($name:expr, $note:expr, $view:expr, $fov:expr) => {{
            let (eye, at) = t34_view(&g, $view.0, $view.1);
            rig.still(&g, &mut r, $name, $note, eye, at, $fov)?;
        }};
    }

    tiger_still!("day_before_tiger_left", "unmarked Tiger hull side, day", tiger_left_near, 55.0);
    t34_still!("day_before_t34_side", "unmarked T-34 hull side, day", t34_side_near, 55.0);

    // The canonical set at the Tiger. Directions in the hull/turret frame (+Z forward, +X right).
    let at_tiger = [
        shot("76mm_bounce_500m_front", &crate::ballistics::BR_350B, 500.0, Vec3::new(0.35, 1.30, 3.4), Vec3::new(-0.12, -0.05, -1.0), false),
        shot("mg_peppering_front", &MG_MARKER, 120.0, Vec3::new(-0.45, 1.25, 3.4), Vec3::new(0.15, -0.08, -1.0), false),
        shot("76mm_he_side", &crate::ballistics::OF_350_F34, 400.0, Vec3::new(-2.4, 1.40, -1.6), Vec3::new(1.0, -0.15, 0.18), false),
        // The engine compartment's side: a clean hole with no ammunition rack behind it (a hit
        // over the sponson racks detonates them, as it should).
        shot("88mm_pen_1500m_side", &crate::ballistics::PZGR_39, 1490.0, Vec3::new(-2.4, 1.55, -2.30), Vec3::new(1.0, -0.06, -0.04), false),
        // The turret's rear plate (the escape hatch side): no ready rack behind it.
        shot("76mm_apcr_200m_turret", &crate::ballistics::BR_354P, 200.0, Vec3::new(0.45, 1.85, -1.9), Vec3::new(-0.15, -0.30, 1.0), true),
    ];
    let mut seed = 0x5CA2_0001u32;
    let mut hole_age_stills = false;
    for s in &at_tiger {
        if s.name == "76mm_apcr_200m_turret" { g.debug_extinguish_cabin(); }
        fire(&mut g, &mut r, &mut rig, None, s, seed)?;
        seed = seed.wrapping_add(0x9E37);
        if s.name == "76mm_apcr_200m_turret" {
            // Inside the turret after the round came through its rear plate.
            // 2.5 s on, once the spall has come to rest; any fire it lit is put out for the view.
            step(&mut g, &mut r, 150);
            g.debug_extinguish_cabin();
            step(&mut g, &mut r, 2);
            if let Some(hole) = g.armor_scars.scars().iter().copied()
                .find(|s| s.kind == crate::armor_damage::ScarKind::Hole && s.is_turret()) {
                let turret = g.player.turret_model();
                let at = turret.transform_point3(hole.local_position() - hole.local_normal() * hole.plate_m());
                // The loader's own eye, turned to look aft at the hole.
                let eye = g.station_eye(Station::Loader);
                let _ = turret;
                rig.inside = true;
                rig.still(&g, &mut r, "day_tiger_interior_turret_hole", "the turret-rear hole from inside, 2.5 s after: back-face scab, daylight through the bore, spall pocks", eye, at, 70.0)?;
                rig.inside = false;
            }
        }
        if s.name == "88mm_pen_1500m_side" && !hole_age_stills {
            hole_age_stills = true;
            // Straight onto the fresh hole from 1.4 m along its normal, up-wind of the burst.
            if let Some(hole) = g.armor_scars.scars().iter().rev().find(|s| s.kind == crate::armor_damage::ScarKind::Hole).copied() {
                let hull = g.player.hull_model();
                let at = hull.transform_point3(hole.local_position());
                let eye = hull.transform_point3(hole.local_position() + hole.local_normal() * 1.4 + Vec3::new(0.0, 0.35, 0.3));
                rig.still(&g, &mut r, "day_hot_hole_0s1", "88 mm hole 0.1 s after the strike: rim incandescent", eye, at, 40.0)?;
                step(&mut g, &mut r, 60);
                rig.still(&g, &mut r, "day_hot_hole_1s", "the same hole at 1.1 s: rim cooling", eye, at, 40.0)?;
                step(&mut g, &mut r, 180);
                rig.still(&g, &mut r, "day_hot_hole_4s", "the same hole at 4 s: dull red fading", eye, at, 40.0)?;
            }
        }
    }

    // The canonical set at the T-34, fired by the Tiger.
    let at_t34 = [
        shot("76mm_bounce_500m_glacis", &crate::ballistics::BR_350B, 500.0, Vec3::new(0.35, 1.28, 2.80), Vec3::new(-0.1, 0.02, -1.0), false),
        shot("mg_peppering_turret", &MG_MARKER, 120.0, Vec3::new(0.0, 0.55, 0.9), Vec3::new(0.12, -0.05, -1.0), true),
        shot("88mm_he_side", &crate::ballistics::SPRGR_88, 400.0, Vec3::new(-2.2, 1.62, -1.2), Vec3::new(1.0, -0.22, 0.15), false),
        shot("88mm_apcr_200m_turret", &crate::ballistics::PZGR_40, 200.0, Vec3::new(-1.3, 0.45, 0.1), Vec3::new(1.0, -0.10, -0.1), true),
        shot("88mm_pen_1500m_side", &crate::ballistics::PZGR_39, 1490.0, Vec3::new(-2.2, 1.62, 0.7), Vec3::new(1.0, -0.22, -0.12), false),
    ];
    for s in &at_t34 {
        fire(&mut g, &mut r, &mut rig, Some(t34), s, seed)?;
        seed = seed.wrapping_add(0x9E37);
    }
    step(&mut g, &mut r, 90);

    // Day, near/mid/far.
    tiger_still!("day_tiger_left_near", "Tiger left: HE dish with fragment pits, 88 mm hole and plug side", tiger_left_near, 50.0);
    // Every crater and hole close up, square to its plate, on both vehicles.
    for vehicle in [None, Some(t34)] {
        let (field, tank) = match vehicle {
            None => (g.armor_scars.clone(), g.player.clone()),
            Some(i) => (g.enemies[i].scars.clone(), g.enemies[i].clone()),
        };
        for (index, scar) in field.scars().iter().copied().enumerate().filter(|(_, s)| s.kind != crate::armor_damage::ScarKind::Pock) {
            let frame = if scar.is_turret() { tank.turret_model() } else { tank.hull_model() };
            let at = frame.transform_point3(scar.local_position());
            let out = frame.transform_vector3(scar.local_normal()).normalize_or_zero();
            let eye = at + out * (scar.radius_m() * 9.0).max(0.8) + Vec3::Y * 0.2 + out.cross(Vec3::Y).normalize_or_zero() * 0.2;
            let who = vehicle.map_or("tiger", |_| "t34");
            rig.still(&g, &mut r, &format!("day_{who}_closeup{index}"), "a wound square to its plate", eye, at, 45.0)?;
        }
    }
    tiger_still!("day_tiger_front_near", "Tiger front: 76 mm bounce crater and MG pocks", tiger_front_near, 50.0);
    tiger_still!("day_tiger_turret_near", "Tiger turret right: 76 mm APCR core hole", tiger_turret_near, 50.0);
    tiger_still!("day_tiger_mid", "Tiger at 12 m: decal path for small marks", tiger_mid, 50.0);
    tiger_still!("day_tiger_far", "Tiger at 45 m", tiger_far, 40.0);
    t34_still!("day_t34_side_near", "T-34 side: HE dish, 88 mm hole", t34_side_near, 55.0);
    t34_still!("day_t34_front_near", "T-34 glacis: 76 mm bounce, turret MG pocks", t34_front_near, 55.0);
    t34_still!("day_t34_mid", "T-34 at 11 m", t34_mid, 50.0);

    // Night.
    rig.clock(&mut r, "1942-12-21T17:30:00Z")?;
    step(&mut g, &mut r, 30);
    tiger_still!("night_tiger_left_near", "night: Tiger left", tiger_left_near, 50.0);
    tiger_still!("night_tiger_far", "night: Tiger at 45 m", tiger_far, 40.0);
    t34_still!("night_t34_side_near", "night: T-34 side", t34_side_near, 55.0);

    // Field patch over the hull hole (the ritual's state), by day.
    rig.clock(&mut r, "1942-12-21T10:30:00Z")?;
    let patched = g.debug_patch_first_armor_hole();
    step(&mut g, &mut r, 30);
    tiger_still!("day_tiger_left_patched", "spare track shoes seated over the hull hole", tiger_left_near, 50.0);

    // Campaign checkpoint round trip: the scar field as saved and loaded.
    let saved = g.armor_scars.scars().to_vec();
    let checkpoint = format!("{prefix}_checkpoint.json");
    let loaded = crate::campaign_save::round_trip_armor_scars_for_review(&saved, std::path::Path::new(&checkpoint))?;
    anyhow::ensure!(loaded == saved, "checkpoint round trip changed the scar field");
    g.armor_scars = crate::armor_damage::ArmorField::from_scars(loaded.clone());
    step(&mut g, &mut r, 10);
    tiger_still!("day_tiger_left_loaded", "after checkpoint save and load: same wounds, now cold and a day weathered", tiger_left_near, 50.0);

    // Cost: frames with and without the wounds, same view.
    let (eye, at) = tiger_view(&g, tiger_left_near.0, tiger_left_near.1);
    rig.view = Some((eye, at, 50.0));
    // Wall time per frame, and with KESSEL_GPU_PROFILE=1 the GPU time of every pass.
    let time_frames = |r: &mut Renderer, rig: &Rig| -> anyhow::Result<(f32, serde_json::Value)> {
        rig.apply(r);
        crate::converge_headless_screenshot(r, rig.device, rig.queue, 8)?;
        let mut summary = crate::gpu_profile::GpuProfileSummary::default();
        let start = std::time::Instant::now();
        for _ in 0..60 {
            rig.apply(r);
            r.begin_gpu_profile_frame();
            r.render_offscreen(rig.device, rig.queue);
            summary.record(r.read_gpu_profile_frame(rig.device, rig.queue)?);
        }
        rig.device.poll(wgpu::PollType::wait_indefinitely())?;
        Ok((start.elapsed().as_secs_f32() * 1000.0 / 60.0, summary.report()))
    };
    g.drive_armor_scars(&mut r);
    let (with_scars, gpu_with) = time_frames(&mut r, &rig)?;
    let stats = r.raymarch_stats();
    for slot in [g.slots.tiger[0], g.slots.tiger[1]] { r.set_mesh_scars(slot, &[]); }
    for rig_slots in [&g.slots.t34, &g.slots.su100, &g.slots.kv2, &g.slots.isu152, &g.slots.is2, &g.slots.is3] {
        for slots in rig_slots.iter() { r.set_mesh_scars(slots[0], &[]); r.set_mesh_scars(slots[1], &[]); }
    }
    let (without, gpu_without) = time_frames(&mut r, &rig)?;

    let receipt = serde_json::json!({
        "prefix": prefix, "manifest": manifest.display().to_string(), "renderer": "raymarch",
        "internal_res": [r.iw, r.ih], "tiger_at": tiger_pos.to_array(), "t34_index": t34,
        "lead_m": LEAD_M, "tiger_ammunition": "racks, breech and loose rounds emptied (a penetration otherwise detonates them)",
        "tiger_cabin_fire": "put out before the turret-rear shot so its interior still is not a burning compartment",
        "nominal_1500m_shots_flown_at_m": 1490.0, "shots": rig.shots, "stills": rig.stills, "patched": patched,
        "tiger_scars": g.armor_scars.scars().iter().map(scar_json).collect::<Vec<_>>(),
        "t34_scars": g.enemies[t34].scars.scars().iter().map(scar_json).collect::<Vec<_>>(),
        "t34_alive": g.enemies[t34].alive, "tiger_alive": g.player.alive,
        "checkpoint": checkpoint, "checkpoint_scars": loaded.len(),
        "gpu_passes_with_scars": gpu_with, "gpu_passes_without_scars": gpu_without,
        "gpu_busy_note": "other sessions share this GPU; compare passes, not totals",
        "cost_ms_per_frame": {"with_scars": with_scars, "without_scars": without,
            "scar_records": stats.map(|s| s.scars), "near_patches": stats.map(|s| s.scar_patches)},
    });
    fs::write(format!("{prefix}_receipt.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[armor-scars] receipt {prefix}_receipt.json; frame {with_scars:.2} ms with wounds, {without:.2} ms without");
    Ok(())
}

/// Stands for "a machine-gun burst" in the shot table; `fire` routes calibres below 20 mm to
/// the small-arms flight (the actual 7.62 mm profile), never through this record.
static MG_MARKER: crate::ballistics::ShellSpec = crate::ballistics::ShellSpec {
    name: "7.62 mm DT machine-gun burst (24 rounds)",
    kind: crate::ballistics::ShellKind::Ap,
    caliber_mm: 7.62,
    mass_kg: 0.0096,
    muzzle_velocity: 840.0,
    filler_g: 0.0,
    drag_k: 0.0,
    pen_ref_mm: 0.0,
    ref_range_m: 0.0,
};
