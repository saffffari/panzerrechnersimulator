//! `fx-lab PREFIX SCENE`: the rebuilt fire, smoke and explosion effects' test scenes on the Canyon
//! map (docs/FX_OVERHAUL_20261109.md). Each scene is staged through the game's own routes (a
//! burning wreck, a catastrophic ammunition kill, a 76 mm HE shell on the ground), filmed from
//! fixed observer cameras, and timed. Run once as is and once with `KESSEL_FX_LEGACY=1` for the
//! old effects; `tools/fx_lab_compare.py` sets the two side by side.
//!
//! Scenes: `wreck` (a burning T-34 at 20 m), `cookoff` (an ammunition kill at 130 m), `impact`
//! (HE shells at 45 and 70 m), `fog` (a burning wreck 450 m off through the Canyon's air, and a
//! cook-off there), `three` (three burning wrecks at 60, 250 and 700 m: the shared budget).
//! Stills are 5120x2880 JPEGs, nearest neighbour from the 1024x576 grid; clips 5120x2880 H.264
//! at 30 fps. Observer cameras, not a player run. No grading.
use crate::{act_manifest, game::{Game, TankInput}, graphics, renderer::Renderer, world};
use glam::{Vec2, Vec3};
use std::{io::Write, process::{Command, Stdio}};

const DT: f32 = 1.0 / 60.0;
const SCALE: u32 = 5;

fn view(r: &mut Renderer, g: &mut Game, eye: Vec3, target: Vec3, fov_deg: f32) {
    r.camera.pos = eye;
    r.camera.set_forward(target - eye);
    r.camera.fov_y = fov_deg.to_radians();
    r.optic_mask = 0.0;
    r.reticle = false;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    r.rt_mix = 1.0;
    r.weather_shelter = g.building_weather_shelter_at(eye);
    g.hide_couch_crew_rigs(r);
    r.ui_reset();
}

/// An observer `off` from `base` horizontally, `off.y` above the ground there.
fn eye_at(lab: &Lab, base: Vec3, off: Vec3) -> Vec3 {
    let p = base + off;
    Vec3::new(p.x, lab.g.debug_ground_height(p.x, p.z) + off.y, p.z)
}

fn upscale(rgba: &[u8], w: u32, h: u32) -> image::RgbImage {
    let mut out = image::RgbImage::new(w * SCALE, h * SCALE);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let i = (((y / SCALE) * w + x / SCALE) * 4) as usize;
        *px = image::Rgb([rgba[i], rgba[i + 1], rgba[i + 2]]);
    }
    out
}

struct Film { child: std::process::Child }
impl Film {
    fn new(path: &str, w: u32, h: u32) -> anyhow::Result<Self> {
        let child = Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-y",
            "-f", "rawvideo", "-pixel_format", "rgba", "-video_size", &format!("{w}x{h}"),
            "-framerate", "30", "-i", "pipe:0", "-an",
            "-vf", &format!("scale={}:{}:flags=neighbor:in_range=full:out_range=tv:out_color_matrix=bt709,setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709", w * SCALE, h * SCALE),
            "-c:v", "libx264", "-threads", "4", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", path]).stdin(Stdio::piped()).spawn()?;
        Ok(Self { child })
    }
    fn frame(&mut self, rgba: &[u8]) -> anyhow::Result<()> { Ok(self.child.stdin.as_mut().unwrap().write_all(rgba)?) }
    fn finish(mut self) -> anyhow::Result<()> {
        drop(self.child.stdin.take());
        anyhow::ensure!(self.child.wait()?.success(), "ffmpeg failed");
        Ok(())
    }
}

/// Per-frame costs over a stretch of updates and renders.
#[derive(Default)]
struct Cost { sim_ms: Vec<f64>, gpu_ms: Vec<f64> }
impl Cost {
    fn json(&self) -> serde_json::Value {
        let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
        let p99 = |v: &[f64]| { let mut s = v.to_vec(); s.sort_by(f64::total_cmp); s.get(s.len() * 99 / 100).copied().unwrap_or(0.0) };
        serde_json::json!({"frames": self.sim_ms.len(), "simulation_mean_ms": mean(&self.sim_ms), "simulation_p99_ms": p99(&self.sim_ms),
            "gpu_frame_mean_ms": mean(&self.gpu_ms), "gpu_frame_p99_ms": p99(&self.gpu_ms)})
    }
}

struct Lab {
    device: wgpu::Device,
    queue: wgpu::Queue,
    r: Renderer,
    g: Game,
    prefix: String,
    variant: &'static str,
    shots: Vec<serde_json::Value>,
}

impl Lab {
    fn step(&mut self, n: usize) -> f64 {
        let t = std::time::Instant::now();
        for _ in 0..n { self.g.update(&TankInput::default(), DT, &mut self.r); }
        t.elapsed().as_secs_f64() * 1000.0 / n.max(1) as f64
    }
    fn rgba(&mut self) -> anyhow::Result<Vec<u8>> {
        let (w, h) = (self.r.iw, self.r.ih);
        self.r.render_to_rgba(&self.device, &self.queue, w, h)
    }
    fn still(&mut self, scene: &str, name: &str) -> anyhow::Result<()> {
        let rgba = self.rgba()?;
        let path = format!("{}-{}-{}-{}.jpg", self.prefix, scene, self.variant, name);
        let img = upscale(&rgba, self.r.iw, self.r.ih);
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path)?);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 95).encode_image(&img)?;
        let stats = self.r.raymarch_stats();
        println!("[fx-lab] {path}");
        self.shots.push(serde_json::json!({"path": path, "eye": self.r.camera.pos.to_array(),
            "plumes": stats.map(|s| s.plumes), "parcels": stats.map(|s| s.parcels), "embers": stats.map(|s| s.embers),
            "merged_grains": stats.map(|s| s.merged_grains)}));
        Ok(())
    }
    /// Timed frames at the current camera: simulation and fenced render, `n` of them.
    fn time(&mut self, n: usize, camera: &dyn Fn(&mut Lab, usize)) -> Cost {
        let mut cost = Cost::default();
        for i in 0..n {
            cost.sim_ms.push(self.step(1));
            camera(self, i);
            let t = std::time::Instant::now();
            self.r.render_offscreen(&self.device, &self.queue);
            self.device.poll(wgpu::PollType::wait_indefinitely()).ok();
            cost.gpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        cost
    }
    fn settle(&mut self) -> anyhow::Result<()> {
        self.r.reset_history();
        crate::converge_headless_screenshot(&mut self.r, &self.device, &self.queue, 24)?;
        Ok(())
    }
}

pub fn run(prefix: &str, scene: &str) -> anyhow::Result<()> {
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    let tiger = Vec2::new(-450.0, -410.0);
    g.debug_place_tiger(tiger, 0.0);
    // `KESSEL_FXLAB_VARIANT=name`: label the files and receipt (a review variant of the same effects).
    let variant: &'static str = match std::env::var("KESSEL_FXLAB_VARIANT") {
        Ok(v) if !v.is_empty() => Box::leak(v.into_boxed_str()),
        _ => if crate::fx_plumes::enabled() { "new" } else { "old" },
    };
    let mut lab = Lab { device, queue, r, g, prefix: prefix.to_string(), variant, shots: Vec::new() };
    lab.step(30);
    let ground = |lab: &Lab, x: f32, z: f32| lab.g.debug_ground_height(x, z);
    let t3 = Vec3::new(tiger.x, ground(&lab, tiger.x, tiger.y), tiger.y);
    let (w, h) = (lab.r.iw, lab.r.ih);
    let mut costs = serde_json::Map::new();
    let clip = |lab: &Lab, name: &str| format!("{}-{}-{}-{}.mp4", lab.prefix, scene, lab.variant, name);
    match scene {
        "wreck" => {
            let at = Vec2::new(tiger.x - 15.0, tiger.y + 16.0);
            let base = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y);
            let target = base + Vec3::Y * 2.5;
            let near = eye_at(&lab, base, Vec3::new(9.0, 1.7, -14.0));
            let cam = move |lab: &mut Lab, _i: usize| view(&mut lab.r, &mut lab.g, near, target, 58.0);
            cam(&mut lab, 0);
            lab.settle()?;
            costs.insert("near_camera_empty".into(), lab.time(240, &cam).json());
            lab.g.debug_stage_fx_wreck(0, at.x, at.y, 0.6, false, 0.0).ok_or_else(|| anyhow::anyhow!("no T-34"))?;
            lab.step(480);
            let cams = [("near", Vec3::new(9.0, 1.7, -14.0), 58.0), ("wide", Vec3::new(-30.0, 6.0, 38.0), 58.0), ("column", Vec3::new(55.0, 2.0, -70.0), 55.0)];
            for (name, off, fov) in cams {
                let tgt = if name == "column" { base + Vec3::Y * 25.0 } else { target };
                let e = eye_at(&lab, base, off); view(&mut lab.r, &mut lab.g, e, tgt, fov);
                lab.settle()?;
                lab.still(scene, name)?;
            }
            let mut film = Film::new(&clip(&lab, "orbit"), w, h)?;
            for f in 0..240 {
                lab.step(2);
                let a = f as f32 / 239.0 * 0.9;
                let off = glam::Quat::from_rotation_y(a) * Vec3::new(9.0, 1.7, -14.0);
                let e = eye_at(&lab, base, off); view(&mut lab.r, &mut lab.g, e, target, 58.0);
                let rgba = lab.rgba()?;
                film.frame(&rgba)?;
            }
            film.finish()?;
            costs.insert("near_camera".into(), lab.time(240, &cam).json());
        }
        "cookoff" => {
            let at = Vec2::new(tiger.x - 30.0, tiger.y + 126.0);
            let base = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y);
            let eye = t3 + Vec3::new(4.0, 3.2, 0.0);
            let target = base + Vec3::Y * 10.0;
            view(&mut lab.r, &mut lab.g, eye, target, 50.0);
            lab.settle()?;
            let cam = move |lab: &mut Lab, _i: usize| view(&mut lab.r, &mut lab.g, eye, target, 50.0);
            costs.insert("tiger_view_empty".into(), lab.time(240, &cam).json());
            lab.g.debug_stage_fx_wreck(0, at.x, at.y, 0.4, true, 0.0).ok_or_else(|| anyhow::anyhow!("no T-34"))?;
            let marks = [(0.15f32, "t0.15s"), (0.6, "t0.6s"), (2.0, "t2s"), (6.0, "t6s"), (12.0, "t12s"), (24.0, "t24s")];
            let mut film = Film::new(&clip(&lab, "kill"), w, h)?;
            let mut t = 0.0f32;
            let mut next = 0;
            let mut cost = Cost::default();
            while t < 24.05 {
                cost.sim_ms.push(lab.step(2));
                t += 2.0 * DT;
                view(&mut lab.r, &mut lab.g, eye, target, 50.0);
                let start = std::time::Instant::now();
                let rgba = lab.rgba()?;
                cost.gpu_ms.push(start.elapsed().as_secs_f64() * 1000.0);
                film.frame(&rgba)?;
                if next < marks.len() && t >= marks[next].0 {
                    lab.still(scene, marks[next].1)?;
                    next += 1;
                }
            }
            film.finish()?;
            costs.insert("kill_24s_including_readback".into(), cost.json());
            costs.insert("tiger_view_after_24s".into(), lab.time(240, &cam).json());
        }
        "impact" => {
            let eye = eye_at(&lab, t3, Vec3::new(-9.0, 2.4, -5.0));
            let fwd = Vec3::new(0.0, 0.0, 1.0);
            let target = t3 + fwd * 55.0 + Vec3::new(-20.0, 4.0, 0.0);
            view(&mut lab.r, &mut lab.g, eye, target, 45.0);
            lab.settle()?;
            let cam = move |lab: &mut Lab, _i: usize| view(&mut lab.r, &mut lab.g, eye, target, 45.0);
            costs.insert("tiger_view_empty".into(), lab.time(240, &cam).json());
            let strike = |lab: &mut Lab, ahead: f32, right: f32| {
                let p = t3 + fwd * ahead + Vec3::new(right, 0.0, 0.0);
                let incoming = Vec3::new(0.0, -20f32.to_radians().sin(), -20f32.to_radians().cos()) * 680.0;
                lab.g.debug_ground_strike(p, incoming, 6.2, 76.2, &lab.r);
            };
            strike(&mut lab, 45.0, -16.0);
            let marks = [(0.05f32, "t0.05s"), (0.3, "t0.3s"), (1.0, "t1s"), (3.0, "t3s")];
            let mut film = Film::new(&clip(&lab, "strikes"), w, h)?;
            let mut t = 0.0f32;
            let mut next = 0;
            let mut second = false;
            while t < 8.0 {
                lab.step(2);
                t += 2.0 * DT;
                if !second && t >= 4.0 { strike(&mut lab, 75.0, -30.0); second = true; }
                view(&mut lab.r, &mut lab.g, eye, target, 45.0);
                let rgba = lab.rgba()?;
                film.frame(&rgba)?;
                if next < marks.len() && t >= marks[next].0 { lab.still(scene, marks[next].1)?; next += 1; }
            }
            film.finish()?;
        }
        "fog" => {
            // A burning wreck 450 m up the valley from the Tiger, then a cook-off beside it.
            let dir = Vec2::new(std::env::var("KESSEL_FXLAB_FOG_DIR").ok().and_then(|v| v.parse().ok()).unwrap_or(-0.2f32).sin(),
                std::env::var("KESSEL_FXLAB_FOG_DIR").ok().and_then(|v| v.parse().ok()).unwrap_or(-0.2f32).cos());
            let range: f32 = std::env::var("KESSEL_FXLAB_FOG_RANGE").ok().and_then(|v| v.parse().ok()).unwrap_or(450.0);
            let at = tiger + dir * range;
            let base = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y);
            let eye = eye_at(&lab, t3, Vec3::new(5.0, 4.5, -7.0));
            let target = base + Vec3::Y * 12.0;
            let cam = move |lab: &mut Lab, _i: usize| view(&mut lab.r, &mut lab.g, eye, target, 40.0);
            cam(&mut lab, 0);
            lab.settle()?;
            costs.insert("tiger_view_empty".into(), lab.time(240, &cam).json());
            lab.g.debug_stage_fx_wreck(0, at.x, at.y, 1.0, false, 0.0).ok_or_else(|| anyhow::anyhow!("no T-34"))?;
            lab.step(600);
            view(&mut lab.r, &mut lab.g, eye, target, 40.0);
            lab.settle()?;
            lab.still(scene, "wreck450m")?;
            view(&mut lab.r, &mut lab.g, eye, target, 12.0);
            lab.settle()?;
            lab.still(scene, "wreck450m-binocular")?;
            let at2 = at + Vec2::new(dir.y, -dir.x) * 40.0;
            lab.g.debug_stage_fx_wreck(1, at2.x, at2.y, 0.2, true, 0.0).ok_or_else(|| anyhow::anyhow!("no second T-34"))?;
            let marks = [(0.4f32, "cookoff-t0.4s"), (3.0, "cookoff-t3s"), (10.0, "cookoff-t10s")];
            let mut film = Film::new(&clip(&lab, "far"), w, h)?;
            let mut t = 0.0f32;
            let mut next = 0;
            while t < 14.0 {
                lab.step(2);
                t += 2.0 * DT;
                view(&mut lab.r, &mut lab.g, eye, target, 40.0);
                let rgba = lab.rgba()?;
                film.frame(&rgba)?;
                if next < marks.len() && t >= marks[next].0 { lab.still(scene, marks[next].1)?; next += 1; }
            }
            film.finish()?;
            costs.insert("tiger_view".into(), lab.time(240, &cam).json());
        }
        "three" => {
            let eye = eye_at(&lab, t3, Vec3::new(5.0, 4.5, -7.0));
            let target = t3 + Vec3::new(0.0, 8.0, 200.0);
            let cam = move |lab: &mut Lab, _i: usize| view(&mut lab.r, &mut lab.g, eye, target, 55.0);
            cam(&mut lab, 0);
            lab.settle()?;
            costs.insert("tiger_view_empty".into(), lab.time(300, &cam).json());
            let spots = [(60.0f32, 0.2f32), (250.0, -0.25), (700.0, 0.1)];
            for (k, (range, bearing)) in spots.iter().enumerate() {
                let at = tiger + Vec2::new(bearing.sin(), bearing.cos()) * *range;
                lab.g.debug_stage_fx_wreck(k, at.x, at.y, 0.3 * k as f32, false, 30.0 * k as f32)
                    .ok_or_else(|| anyhow::anyhow!("not enough T-34s"))?;
            }
            lab.step(600);
            view(&mut lab.r, &mut lab.g, eye, target, 55.0);
            lab.settle()?;
            lab.still(scene, "tiger-view")?;
            let mut film = Film::new(&clip(&lab, "pan"), w, h)?;
            for f in 0..240 {
                lab.step(2);
                let a = (f as f32 / 239.0 - 0.5) * 0.8;
                view(&mut lab.r, &mut lab.g, eye, t3 + Vec3::new(a.sin() * 200.0, 8.0, a.cos() * 200.0), 55.0);
                let rgba = lab.rgba()?;
                film.frame(&rgba)?;
            }
            film.finish()?;
            costs.insert("tiger_view_three_wrecks".into(), lab.time(300, &cam).json());
        }
        "mg" => {
            // Bursts of 7.62 mm: the Tiger's MG onto a T-34's glacis at 120 m, then the T-34's
            // onto the Tiger's turret front (scripted rounds through the ordinary flight/contact).
            // The open ground the burning-wreck scene uses, 22 m from the Tiger.
            let at = Vec2::new(tiger.x - 15.0, tiger.y + 16.0);
            let idx = lab.g.debug_park_live_t34(0, at.x, at.y, std::f32::consts::PI)
                .ok_or_else(|| anyhow::anyhow!("no T-34"))?;
            let t34 = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y) + Vec3::Y * 1.3;
            let tiger_turret = t3 + Vec3::Y * 2.3 + Vec3::Z * 1.0;
            let mg_from = t3 + Vec3::new(0.3, 2.0, 3.0);
            let near_t34 = eye_at(&lab, t34, Vec3::new(7.0, 1.2, -6.0));
            let tiger_side = eye_at(&lab, t3, Vec3::new(5.0, 2.2, -6.0));
            let sight = t3 + Vec3::new(-0.6, 3.4, 1.0);
            let shots: [(&str, Vec3, Vec3, f32); 3] = [("onto-t34-sight", sight, t34, 25.0), ("onto-t34-near", near_t34, t34, 50.0), ("onto-tiger", tiger_side, tiger_turret, 50.0)];
            for (k, (name, eye, target, fov)) in shots.iter().enumerate() {
                view(&mut lab.r, &mut lab.g, *eye, *target, *fov);
                lab.settle()?;
                let mut film = Film::new(&clip(&lab, name), w, h)?;
                for f in 0..120u32 {
                    // 900 rounds/min: a round every 4 updates, two-second bursts with a pause.
                    for u in 0..2u32 {
                        let tick = f * 2 + u;
                        if tick % 4 == 0 && (tick / 60) % 2 == 0 {
                            if k < 2 { lab.g.debug_mg_round(mg_from, t34, None, tick); }
                            else { lab.g.debug_mg_round(t34 + Vec3::Y * 0.6, tiger_turret, Some(idx), tick); }
                        }
                        lab.step(1);
                    }
                    view(&mut lab.r, &mut lab.g, *eye, *target, *fov);
                    let rgba = lab.rgba()?;
                    film.frame(&rgba)?;
                    if f == 20 || f == 40 { lab.still(scene, &format!("{name}-f{f}"))?; }
                }
                film.finish()?;
            }
        }
        "shock" => {
            // A 76 mm HE shell 40 m out and a whole-load cook-off at 150 m, their fronts sweeping
            // the snow (filmed at the frame rate: the front crosses 5 m a frame).
            let eye = eye_at(&lab, t3, Vec3::new(-9.0, 2.4, -5.0));
            let fwd = Vec3::Z;
            let strike_at = t3 + fwd * 40.0 + Vec3::new(-12.0, 0.0, 0.0);
            let target = strike_at + Vec3::Y * 2.0;
            view(&mut lab.r, &mut lab.g, eye, target, 50.0);
            lab.settle()?;
            let incoming = Vec3::new(0.0, -20f32.to_radians().sin(), -20f32.to_radians().cos()) * 680.0;
            lab.g.debug_ground_strike(strike_at, incoming, 6.2, 76.2, &lab.r);
            let mut film = Film::new(&clip(&lab, "he40m"), w, h)?;
            for f in 0..90u32 {
                lab.step(2);
                view(&mut lab.r, &mut lab.g, eye, target, 50.0);
                let rgba = lab.rgba()?; film.frame(&rgba)?;
                if [1u32, 2, 3, 5].contains(&f) { lab.still(scene, &format!("he40m-f{f}"))?; }
            }
            film.finish()?;
            let at = Vec2::new(tiger.x - 30.0, tiger.y + 150.0);
            let base = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y);
            let target = base + Vec3::Y * 6.0;
            view(&mut lab.r, &mut lab.g, eye, target, 50.0);
            lab.g.debug_stage_fx_wreck(0, at.x, at.y, 0.4, true, 0.0).ok_or_else(|| anyhow::anyhow!("no T-34"))?;
            let mut film = Film::new(&clip(&lab, "cookoff150m"), w, h)?;
            for f in 0..120u32 {
                lab.step(1);
                view(&mut lab.r, &mut lab.g, eye, target, 50.0);
                let rgba = lab.rgba()?; film.frame(&rgba)?;
                if [3u32, 8, 15, 25].contains(&f) { lab.still(scene, &format!("cookoff150m-f{f}"))?; }
            }
            film.finish()?;
        }
        "sight" => {
            // The Tiger's 88 at a T-34 300, 600 and 900 m up the valley, seen from the gun sight's
            // place with the TZF 9's field (23 deg across, about 13 deg high at 16:9; 2.5x). An
            // observer camera there, not the eyepiece render (no reticle, no lens).
            // Up the valley on the bearing the fog scene showed clear of the ruins.
            let dir = Vec2::new((-0.2f32).sin(), (-0.2f32).cos());
            for (k, range) in [300.0f32, 600.0, 900.0].into_iter().enumerate() {
                let at = tiger + dir * range;
                let Some(idx) = lab.g.debug_park_live_t34(k, at.x, at.y, std::f32::consts::PI + 0.5) else { continue };
                let hull = Vec3::new(at.x, ground(&lab, at.x, at.y), at.y) + Vec3::Y * 1.1;
                let target = hull + Vec3::Y * 1.5;
                lab.step(30);
                // The sight's place over the Tiger's front, looking down the line of fire. The
                // round is a real Pzgr. 39 started a few metres short of the hull with the speed,
                // flown distance and age of a shot from this range; the armour solve decides.
                let line = (hull - (t3 + Vec3::Y * 2.2)).normalize_or_zero();
                let sight = eye_at(&lab, t3, Vec3::new(5.0, 4.5, -7.0));
                view(&mut lab.r, &mut lab.g, sight, target, 13.0);
                lab.settle()?;
                lab.g.debug_launch_round(&crate::ballistics::PZGR_39, true, hull, line, range, 7 + k as u32);
                let tof = 0.0f32;
                let marks = [(tof + 0.03, "hit"), (tof + 0.5, "hit+0.5s"), (tof + 3.0, "hit+3s"), (tof + 8.0, "hit+8s"), (tof + 15.0, "hit+15s")];
                let name = format!("{}m", range as u32);
                let mut film = Film::new(&clip(&lab, &name), w, h)?;
                let mut t = 0.0f32;
                let mut next = 0;
                while t < tof + 16.0 {
                    lab.step(2);
                    t += 2.0 * DT;
                    view(&mut lab.r, &mut lab.g, sight, target, 13.0);
                    let rgba = lab.rgba()?;
                    film.frame(&rgba)?;
                    if next < marks.len() && t >= marks[next].0 { lab.still(scene, &format!("{name}-{}", marks[next].1))?; next += 1; }
                }
                film.finish()?;
                lab.shots.push(serde_json::json!({"range_m": range, "enemy": idx, "alive_after": lab.g.enemies[idx].alive,
                    "hp_after": lab.g.enemies[idx].hp}));
            }
        }
        "selfhit" => {
            // Hits on the Tiger seen from its own seats, the game's own station cameras (no
            // observer camera): a 76 mm BR-350B stopped by the glacis, seen from the commander's,
            // the driver's and the loader's places, then an 85 mm BR-365 through the hull side
            // seen from the commander's. Real rounds started a few metres short of the plate with
            // the speed and age of a shot from 500 m (front) and 300 m (side).
            use crate::game::Station;
            lab.g.enable_singleplayer_crew();
            lab.g.set_cabin_lights(true);
            let hull = lab.g.player.hull_model();
            let front = hull.transform_point3(Vec3::new(0.3, 1.15, 2.9));
            let front_dir = hull.transform_vector3(Vec3::new(0.05, -0.05, -1.0)).normalize();
            let side = hull.transform_point3(Vec3::new(-1.75, 1.3, -0.4));
            let side_dir = hull.transform_vector3(Vec3::new(1.0, -0.03, 0.1)).normalize();
            let shots: [(&str, Station, &'static crate::ballistics::ShellSpec, Vec3, Vec3, f32); 4] = [
                ("bounce-commander", Station::Commander, &crate::ballistics::BR_350B, front, front_dir, 500.0),
                ("bounce-driver", Station::Driver, &crate::ballistics::BR_350B, front, front_dir, 500.0),
                ("bounce-loader", Station::Loader, &crate::ballistics::BR_350B, front, front_dir, 500.0),
                ("penetration-commander", Station::Commander, &crate::ballistics::BR_365, side, side_dir, 300.0)];
            for (k, (name, station, shell, at, dir, range)) in shots.into_iter().enumerate() {
                lab.g.jump_station(station);
                lab.step(90);
                let mut film = Film::new(&clip(&lab, name), w, h)?;
                for _ in 0..10 { lab.step(2); let rgba = lab.rgba()?; film.frame(&rgba)?; }
                lab.g.debug_launch_round(shell, false, at, dir, range, 31 + k as u32);
                for f in 0..120u32 {
                    // The frame after the strike as the game runs it after a stalled update: one
                    // 0.25 s frame, a catch-up batch of five updates before anything is drawn.
                    // (A perforation's flash comes at the plate solve's commit, 0.35 s on: frame 21.)
                    if f == 1 || f == 21 { lab.g.update_realtime(&TankInput::default(), 0.25, &mut lab.r); } else { lab.step(1); }
                    let rgba = lab.rgba()?;
                    film.frame(&rgba)?;
                    if [1u32, 2, 4, 21, 22, 24, 40, 90].contains(&f) { lab.still(scene, &format!("{name}-f{f}"))?; }
                }
                film.finish()?;
            }
        }
        other => anyhow::bail!("unknown fx-lab scene {other} (wreck, cookoff, impact, fog, three, mg, shock, sight, selfhit)"),
    }
    let receipt = serde_json::json!({"schema": "panzer.fx-lab.v1", "scene": scene, "variant": variant,
        "manifest": manifest, "adapter": adapter.get_info().name, "internal_resolution": [w, h],
        "weather": {"storm": lab.r.storm, "fog_density_m_inv": lab.r.fog_density, "wind_mps": lab.r.weather_wind_world.to_array()},
        "costs": costs, "shots": lab.shots,
        "disclosure": "Staged wrecks and strikes through the game's own routes; observer cameras; Canyon manifest weather at the run's sky start; no grading."});
    std::fs::write(format!("{}-{}-{}.json", prefix, scene, variant), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[fx-lab] {scene} {variant}: {}", serde_json::Value::Object(costs));
    Ok(())
}
