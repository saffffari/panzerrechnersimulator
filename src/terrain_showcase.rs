//! `terrain-showcase PREFIX`: the four terrain viewpoints of docs/TERRAIN_DETAIL.md, day and
//! night, on the Canyon v2 workshop: near ground at the Tiger, a crater field the Tiger's own HE
//! digs, a ridge 1.8 km away, and the ruts of a twelve-second drive. Run once as is and once with
//! `KESSEL_TERRAIN_DETAIL=off` (the coarse 1025 grid) for before/after. The receipt records the
//! terrain LOD statistics, the CPU cost of a ground query, and the GPU surface probed against the
//! physics surface at lattice nodes around the Tiger, the craters and the ruts. Observer cameras
//! and staged fire (banished enemies, held enemy fire, scripted gun lays); no image grading.
use crate::{
    act_manifest, celestial_sky, ephemeris,
    game::{Game, Station, TankInput},
    graphics, world,
};
use glam::{Vec2, Vec3};
use std::fs;

const TIGER: Vec2 = Vec2::new(-418.0, -408.0);
const TIGER_HEADING: f32 = 1.38;
/// Near ground: open, wind-exposed snow 25 m west of the course start (no pads, no road).
const GROUND_TIGER: Vec2 = Vec2::new(-816.0, 845.0);
/// The crater field: open snow beside the course just south of its start (the course keeps its
/// central 24 m clear of dressing), the Tiger facing down the course.
const FIELD_TIGER: Vec2 = Vec2::new(-791.0, 760.0);
const FIELD_SIDE_M: f32 = 9.0;
/// The ridge: 291 m above the course, 1.8 km away on azimuth 65 deg (found on the cooked heights).
const RIDGE_AZIMUTH_DEG: f32 = 65.0;
const RIDGE_DISTANCE_M: f32 = 1800.0;
const DAY: &str = "1942-12-21T10:40:00Z";
const NIGHT: &str = "1942-12-21T19:30:00Z";

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    cw: u32,
    ch: u32,
    shots: Vec<serde_json::Value>,
}

impl Rig<'_> {
    fn clock(&self, r: &mut crate::renderer::Renderer, utc: &str) -> anyhow::Result<()> {
        let instant = ephemeris::UtcInstant::parse_iso(utc)?;
        r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
        Ok(())
    }
    fn still(&mut self, g: &mut Game, r: &mut crate::renderer::Renderer, name: &str, eye: Vec3, target: Vec3, fov_deg: f32, note: &str) -> anyhow::Result<()> {
        for (utc, when) in [(DAY, "day"), (NIGHT, "night")] {
            eprintln!("[terrain-showcase] {name} {when}");
            self.clock(r, utc)?;
            g.update(&TankInput::default(), 1.0 / 60.0, r);
            r.camera.pos = eye;
            r.camera.set_forward(target - eye);
            r.camera.fov_y = fov_deg.to_radians();
            r.optic_mask = 0.0;
            r.reticle = false;
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
            r.reset_history();
            let settled = crate::converge_headless_screenshot(r, self.device, self.queue, 20)?;
            let path = format!("{}_{name}_{when}.png", self.prefix);
            r.render_to_png(self.device, self.queue, &path, self.cw, self.ch)?;
            self.shots.push(serde_json::json!({
                "name": name, "when": when, "path": path, "note": note, "eye": eye.to_array(), "target": target.to_array(),
                "clock_utc": utc, "exposure": settled.measured_exposure,
                "terrain_lod": r.terrain_lod_stats().map(|s| format!("{s:?}")),
            }));
        }
        Ok(())
    }
}

/// `KESSEL_SHOWCASE_SCENES=a,b` stages and renders only the named scenes.
fn wanted(scene: &str) -> bool {
    std::env::var("KESSEL_SHOWCASE_SCENES").map_or(true, |only| only.split(',').any(|s| s.trim() == scene))
}

fn step(g: &mut Game, r: &mut crate::renderer::Renderer, frames: u32, input: &TankInput) {
    for _ in 0..frames { g.update(input, 1.0 / 60.0, r); }
}

fn ground(g: &Game, p: Vec2, above: f32) -> Vec3 {
    Vec3::new(p.x, g.hf.height_at(p.x, p.y) + above, p.y)
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(graphics::shader_backend_enabled(), "terrain-showcase requires KESSEL_RENDERER=raymarch");
    if std::env::var("KESSEL_SHOWCASE_SET").as_deref() == Ok("tiling") { return run_tiling(prefix); }
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_ground_day.png")).exists(),
        "preserving existing captures; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new() };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, TIGER_HEADING);
    r.set_weather_override(Some(0.0));
    let idle = TankInput::default();
    step(&mut g, &mut r, 60, &idle);
    let detail = g.hf.detail.is_some();

    // The CPU cost of a ground query, cold and warm, over a 20 m square at the Tiger.
    let probe_points: Vec<Vec2> = (0..4096).map(|k| TIGER + Vec2::new((k % 64) as f32 * 0.311 - 10.0, (k / 64) as f32 * 0.297 - 10.0)).collect();
    let timed = |g: &Game| {
        let t = std::time::Instant::now();
        let sum: f32 = probe_points.iter().map(|p| g.hf.height_at(p.x, p.y)).sum();
        (t.elapsed().as_secs_f64() * 1e9 / probe_points.len() as f64, sum)
    };
    let (cold_ns, _) = timed(&g);
    let (warm_ns, _) = timed(&g);

    // 1. Near ground at the Tiger, placed in open snow: a low eye at the track, the snow surface
    // running to the hull.
    g.debug_place_tiger(GROUND_TIGER, TIGER_HEADING);
    step(&mut g, &mut r, 30, &idle);
    let tiger = g.player.pos;
    let forward = g.player.forward();
    let side = Vec3::new(-forward.z, 0.0, forward.x);
    let eye = ground(&g, Vec2::new(tiger.x, tiger.z) + Vec2::new(side.x, side.z) * 7.5 + Vec2::new(forward.x, forward.z) * 6.0, 1.25);
    let look = ground(&g, Vec2::new(tiger.x, tiger.z) + Vec2::new(forward.x, forward.z) * 1.5, 0.4);
    if wanted("ground") { rig.still(&mut g, &mut r, "ground", eye, look, 58.0, "near ground at the Tiger: snow waves, wind crust, drift, the track's contact")?; }
    g.debug_place_tiger(TIGER, TIGER_HEADING);
    step(&mut g, &mut r, 30, &idle);
    let tiger = g.player.pos;

    // 2. A ridge 1.8 km away across the valley.
    let a = RIDGE_AZIMUTH_DEG.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let eye = ground(&g, Vec2::new(tiger.x, tiger.z) - dir * 6.0, 28.0);
    let far = Vec2::new(eye.x, eye.z) + dir * RIDGE_DISTANCE_M;
    let target = Vec3::new(far.x, g.hf.height_at(far.x, far.y) - 40.0, far.y);
    if wanted("ridge2km") { rig.still(&mut g, &mut r, "ridge2km", eye, target, 30.0, "the canyon wall 1.8 km away: coarse LOD levels, rock on the scoured faces, drifts in the gullies")?; }

    // 3. A crater field: five 88 mm HE rounds into the snow 35-55 m down the course.
    g.debug_place_tiger(FIELD_TIGER, 0.0);
    step(&mut g, &mut r, 30, &idle);
    let (tiger, forward) = (g.player.pos, g.player.forward());
    let side = Vec3::new(-forward.z, 0.0, forward.x);
    let field = Vec2::new(tiger.x, tiger.z) + Vec2::new(forward.x, forward.z) * 45.0 + Vec2::new(side.x, side.z) * FIELD_SIDE_M;
    g.debug_set_he_loaded(true);
    for k in 0..if wanted("craters") { 5 } else { 0 } {
        let offset = Vec2::new(forward.x, forward.z) * ((k as f32 - 2.0) * 4.5) + Vec2::new(side.x, side.z) * ((k % 2) as f32 * 7.0 - 3.5);
        let aim = ground(&g, field + offset, 0.2);
        g.debug_lay_gun_at(aim);
        step(&mut g, &mut r, 30, &idle);
        g.debug_fire_laid(&mut r);
        step(&mut g, &mut r, 30, &idle);
        g.debug_reload();
    }
    if wanted("craters") { step(&mut g, &mut r, 60 * 25, &idle); }
    let craters = g.hf.detail.as_ref().map_or(0, |d| d.craters().len());
    let center = ground(&g, field, 0.0);
    let eye = center + Vec3::new(side.x, 0.0, side.z) * 13.0 - forward * 8.0 + Vec3::Y * 6.0;
    if wanted("craters") { rig.still(&mut g, &mut r, "craters", eye, center, 55.0, "five 88 mm HE craters in settled snow after 25 s: bowls, thrown rims, frozen earth")?; }

    // 4. Ruts: back at the first spot, twelve seconds of driving on, then look back along them.
    g.debug_place_tiger(TIGER, TIGER_HEADING);
    step(&mut g, &mut r, 30, &idle);
    let tiger = g.player.pos;
    g.station = Station::Driver;
    let start = g.player.pos;
    let drive = TankInput { drive: 1.0, ..TankInput::default() };
    let mut pose = Vec::new();
    for frame in 0..60 * 12 {
        g.update(&drive, 1.0 / 60.0, &mut r);
        if frame % 6 == 0 {
            pose.push([g.player.pos.x, g.player.pos.z, g.player.heading, g.player.pitch, g.player.roll, g.player.speed]);
        }
    }
    step(&mut g, &mut r, 120, &idle);
    let end = g.player.pos;
    let mid = (start + end) * 0.5;
    let back = (start - end).normalize_or_zero();
    let eye = ground(&g, Vec2::new(end.x, end.z) + Vec2::new(back.x, back.z) * 4.0 + Vec2::new(-back.z, back.x) * 4.5, 3.2);
    let look = ground(&g, Vec2::new(mid.x, mid.z), 0.0);
    // The rut map itself (12.5 cm cells, 40 m square on the drive's midpoint), 2.5 mm per level.
    if let Some(d) = g.hf.detail.clone() {
        let n = 320u32;
        let img = image::GrayImage::from_fn(n, n, |i, j| {
            let x = mid.x - 20.0 + (i as f32 + 0.5) * 0.125;
            let z = mid.z - 20.0 + (j as f32 + 0.5) * 0.125;
            image::Luma([(128.0 + d.rut_at(x, z) * 400.0).clamp(0.0, 255.0) as u8])
        });
        img.save(format!("{}_rutmap.png", rig.prefix))?;
    }
    if wanted("ruts") { rig.still(&mut g, &mut r, "ruts", eye, look, 55.0, "the Tiger's ruts after a 12 s drive: compacted snow, shoved berms, tread-band width")?; }

    // The GPU surface against the physics surface at lattice nodes (finest level, unmorphed).
    let mut agreement = serde_json::Value::Null;
    if let Some(d) = g.hf.detail.clone() {
        let lattice = d.lattice;
        let snap = |p: Vec2| ((p + d.half) / lattice).round() * lattice - d.half;
        let mut nodes: Vec<Vec2> = Vec::new();
        for c in [Vec2::new(tiger.x, tiger.z), field, Vec2::new(mid.x, mid.z), Vec2::new(end.x, end.z)] {
            for k in 0..1024 {
                nodes.push(snap(c + Vec2::new((k % 32) as f32 - 16.0, (k / 32) as f32 - 16.0) * lattice * 4.0));
            }
        }
        eprintln!("[terrain-showcase] probing {} lattice nodes", nodes.len());
        if let Some(gpu) = r.probe_terrain(&device, &queue, &nodes) {
            let worst = nodes.iter().zip(&gpu).map(|(p, h)| (d.height_at(p.x, p.y) - h).abs()).fold(0.0f32, f32::max);
            agreement = serde_json::json!({"lattice_nodes": nodes.len(), "max_abs_difference_m": worst,
                "sites": ["tiger", "crater field", "rut midpoint", "drive end"]});
            println!("[terrain-showcase] GPU vs physics surface at {} lattice nodes: max |dh| = {worst:.6} m", nodes.len());
        }
    }

    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.terrain-showcase.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "renderer": format!("{:?}", graphics::renderer_backend()), "internal_resolution": [r.iw, r.ih],
        "output_resolution": [rig.cw, rig.ch], "detail_surface": detail,
        "terrain_lod": r.terrain_lod_stats().map(|s| format!("{s:?}")),
        "ground_query_ns": {"cold": cold_ns, "warm": warm_ns, "points": probe_points.len()},
        "craters": craters, "drive_m": start.distance(end), "drive_pose_every_0_1s": pose,
        "gpu_physics_agreement": agreement, "shots": rig.shots,
        "disclosure": "Staged fixtures (banished enemies, held fire, the Tiger placed, scripted gun lays and a straight drive). Clock overridden per still, clear weather. Observer cameras, not a player run. No image grading."
    }))?)?;
    println!("[terrain-showcase] {} stills -> {prefix}_*.png (detail surface: {detail})", rig.shots.len());
    Ok(())
}

/// `terrain-bench PREFIX [FRAMES]`: the frame at the ground and ridge viewpoints, timed with GPU
/// pass timestamps (`KESSEL_GPU_PROFILE=1`). Run with the detail surface, with it and
/// `KESSEL_RM_SKIP=262144` (no terrain at all; 8192 before integration), and with `KESSEL_TERRAIN_DETAIL=off` (the coarse
/// mesh): the differences are the terrain's own cost. Only trustworthy on an idle GPU; the
/// receipt records the GPU utilisation nvidia-smi reported at the start.
pub fn bench(prefix: &str, frames: u32) -> anyhow::Result<()> {
    anyhow::ensure!(graphics::shader_backend_enabled(), "terrain-bench requires KESSEL_RENDERER=raymarch");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.debug_place_tiger(TIGER, TIGER_HEADING);
    r.set_weather_override(Some(0.0));
    let instant = ephemeris::UtcInstant::parse_iso(DAY)?;
    r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    step(&mut g, &mut r, 60, &TankInput::default());
    let utilisation = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=utilization.gpu,memory.used", "--format=csv,noheader"]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let tiger = g.player.pos;
    let forward = g.player.forward();
    let side = Vec3::new(-forward.z, 0.0, forward.x);
    let a = RIDGE_AZIMUTH_DEG.to_radians();
    let dir = Vec2::new(a.sin(), -a.cos());
    let ridge_eye = ground(&g, Vec2::new(tiger.x, tiger.z) - dir * 6.0, 28.0);
    let far = Vec2::new(ridge_eye.x, ridge_eye.z) + dir * RIDGE_DISTANCE_M;
    let views = [
        ("ground", ground(&g, Vec2::new(tiger.x, tiger.z) + Vec2::new(side.x, side.z) * 7.5 + Vec2::new(forward.x, forward.z) * 6.0, 1.25),
            ground(&g, Vec2::new(tiger.x, tiger.z) + Vec2::new(forward.x, forward.z) * 1.5, 0.4), 58.0f32),
        ("ridge2km", ridge_eye, Vec3::new(far.x, g.hf.height_at(far.x, far.y) - 40.0, far.y), 30.0),
    ];
    let mut results = Vec::new();
    for (name, eye, target, fov) in views {
        let mut gpu = crate::gpu_profile::GpuProfileSummary::default();
        let mut times = Vec::new();
        // Per frame: the passes that draw scene geometry (G-buffer, sun cascades, sky cover,
        // point faces). On a shared GPU their minimum over many frames approximates the
        // uncontended cost; the mean does not.
        const GEOMETRY: [&str; 5] = ["rm-gbuffer", "rm-shadow-far", "rm-shadow-near", "rm-sky-cover", "rm-point-face"];
        let mut geometry: std::collections::BTreeMap<&str, Vec<f64>> = GEOMETRY.iter().map(|k| (*k, Vec::new())).collect();
        let mut geometry_total = Vec::new();
        for i in 0..frames + 30 {
            g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
            r.camera.pos = eye;
            r.camera.set_forward(target - eye);
            r.camera.fov_y = fov.to_radians();
            r.optic_mask = 0.0;
            r.reticle = false;
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
            r.begin_gpu_profile_frame();
            let t0 = std::time::Instant::now();
            r.render_offscreen(&device, &queue);
            device.poll(wgpu::PollType::wait_indefinitely()).ok();
            let ms = t0.elapsed().as_secs_f32() * 1000.0;
            let frame = r.read_gpu_profile_frame(&device, &queue)?;
            if i >= 30 {
                times.push(ms);
                let mut total = 0.0;
                for k in GEOMETRY {
                    let v: f64 = frame.iter().filter(|(l, _)| l == k).map(|(_, ms)| ms).sum();
                    geometry.get_mut(k).unwrap().push(v);
                    total += v;
                }
                geometry_total.push(total);
                gpu.record(frame);
            }
        }
        times.sort_by(f32::total_cmp);
        let n = times.len();
        let avg = times.iter().sum::<f32>() / n as f32;
        println!("[terrain-bench] {name}: {n} frames avg {avg:.2} ms p50 {:.2} p99 {:.2}", times[n / 2], times[n * 99 / 100]);
        let stats = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            serde_json::json!({"min": v[0], "p10": v[v.len() / 10], "p50": v[v.len() / 2]})
        };
        let per_pass: serde_json::Map<String, serde_json::Value> = geometry.iter_mut().map(|(k, v)| (k.to_string(), stats(v))).collect();
        let total = stats(&mut geometry_total);
        println!("[terrain-bench] {name}: geometry passes GPU ms {total}");
        results.push(serde_json::json!({"view": name, "frames": n, "geometry_passes_ms": total, "geometry_per_pass_ms": per_pass, "wall_avg_ms": avg, "wall_p50_ms": times[n / 2],
            "wall_p99_ms": times[n * 99 / 100], "gpu_passes": gpu.report(),
            "terrain_lod": r.terrain_lod_stats().map(|s| format!("{s:?}")), "raymarch": r.raymarch_stats().map(|s| format!("{s:?}"))}));
    }
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.terrain-bench.v1", "adapter": adapter.get_info().name, "internal_resolution": [r.iw, r.ih],
        "detail_surface": g.hf.detail.is_some(), "rm_skip": std::env::var("KESSEL_RM_SKIP").ok(),
        "gpu_utilisation_at_start": utilisation, "views": results,
    }))?)?;
    Ok(())
}

/// `KESSEL_SHOWCASE_SET=tiling`: the ground material at 3, 30, 300 and 2000 m from a snow, a
/// rock and a frozen-mud site, at local noon and at dusk (docs/TERRAIN_DETAIL.md, "Ground
/// material"). Each eye stands on the line from the site toward the course (open ground), at
/// 1.7 m or a tenth of the distance, whichever is higher, looking at the site.
fn run_tiling(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_snow_3m_noon.png")).exists(),
        "preserving existing captures; choose a new prefix");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(), cw: r.iw * 3, ch: r.ih * 3, shots: Vec::new() };
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    // The Tiger parked out of every view.
    g.debug_place_tiger(Vec2::new(1500.0, -1500.0), 0.0);
    r.set_weather_override(Some(0.0));
    step(&mut g, &mut r, 30, &TankInput::default());
    // (name, site, a point toward open ground)
    let sites = [
        ("snow", GROUND_TIGER, Vec2::new(-791.0, 845.0)),
        ("rock", Vec2::new(-109.9, -566.4), Vec2::new(-133.7, -577.3)),
        ("mud", Vec2::new(-791.0, 700.0), Vec2::new(-791.0, 760.0)),
    ];
    for (class, site, open) in sites {
        let away = (open - site).normalize_or(Vec2::X);
        for distance in [3.0f32, 30.0, 300.0, 2000.0] {
            let eye_xz = site + away * distance;
            let eye = ground(&g, eye_xz, (distance * 0.1).max(1.7));
            let target = ground(&g, site, 0.0);
            for (utc, when) in [("1942-12-21T09:00:00Z", "noon"), ("1942-12-21T12:40:00Z", "dusk")] {
                rig.clock(&mut r, utc)?;
                g.update(&TankInput::default(), 1.0 / 60.0, &mut r);
                r.camera.pos = eye;
                r.camera.set_forward(target - eye);
                r.camera.fov_y = 50f32.to_radians();
                r.optic_mask = 0.0;
                r.reticle = false;
                r.interior_light = 0.0;
                r.interior_haze = 0.0;
                r.weather_shelter = 0.0;
                r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
                r.reset_history();
                let settled = crate::converge_headless_screenshot(&mut r, &device, &queue, 20)?;
                let name = format!("{class}_{}m_{when}", distance as u32);
                let path = format!("{prefix}_{name}.png");
                r.render_to_png(&device, &queue, &path, rig.cw, rig.ch)?;
                eprintln!("[terrain-showcase] {name}");
                rig.shots.push(serde_json::json!({"name": name, "path": path, "class": class, "distance_m": distance,
                    "eye": eye.to_array(), "target": target.to_array(), "clock_utc": utc, "exposure": settled.measured_exposure}));
            }
        }
    }
    fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "panzer.terrain-tiling.v1", "manifest": manifest, "adapter": adapter.get_info().name,
        "internal_resolution": [r.iw, r.ih], "detail_surface": g.hf.detail.is_some(), "shots": rig.shots,
        "disclosure": "Observer cameras, the Tiger parked out of view, clear weather, clock overridden per still. No image grading."
    }))?)?;
    Ok(())
}
