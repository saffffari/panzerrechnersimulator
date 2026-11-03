//! `eye-review PREFIX [canyon|bridge]`: the crewman's eye under the owner's complaint (2026-11-03,
//! "either too dark or screen is entirely white ... the exposure effect is just so blinding"),
//! measured as played: live 60 Hz frames, every one simulated and rendered, no exposure
//! convergence between them. Each scenario writes one JSON line per frame (the view's 32 eye
//! lanes, the optic eye's in the sight, and the share of the crushed frame at black and at
//! white) and named stills at the internal grid. Diagnostics only: it reads the eye's buffers and
//! the crushed frame, changes no light and no eye law.
//!
//! Canyon v2 (winter): the cabin buttoned up by day, the hatch opening and the head going out,
//! the head-out view at noon, sky and snow against the dark ground, the main gun and an HE strike
//! by day and by night, the commander's vision blocks, the driver's visor and the gunner's TZF,
//! a deep night. Bridge (spring-bridge-v2): the blue hour, 1943-05-12T18:20Z.
use crate::{act_manifest, game::{Game, Station, TankInput}, graphics, renderer::Renderer, world};
use anyhow::Context;
use glam::{Vec2, Vec3};
use std::io::Write;

const TIGER: Vec2 = Vec2::new(-450.0, -410.0);
const DT: f32 = 1.0 / 60.0;

struct Rig<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    prefix: String,
    log: std::io::BufWriter<std::fs::File>,
    scenario: String,
    t: f32,
    /// A camera direction held over the game's own (the sky/ground sweeps), re-applied after
    /// every update.
    look: Option<Vec3>,
}

/// Share of the crushed frame at black (luma code <= 2), near black (<= 10), near white (>= 240)
/// and white (>= 250), and its mean luma code. The frame is the displayed sRGB codes.
pub(crate) fn clip_stats(rgba: &[u8]) -> [f64; 5] {
    let (mut black, mut dark, mut light, mut white, mut sum) = (0usize, 0usize, 0usize, 0usize, 0.0f64);
    let n = (rgba.len() / 4).max(1);
    for p in rgba.chunks_exact(4) {
        let y = 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
        sum += y;
        if y <= 2.0 { black += 1; }
        if y <= 10.0 { dark += 1; }
        if y >= 240.0 { light += 1; }
        if y >= 250.0 { white += 1; }
    }
    let f = |c: usize| c as f64 / n as f64;
    [f(black), f(dark), f(light), f(white), sum / n as f64]
}

impl Rig<'_> {
    fn begin(&mut self, name: &str) {
        self.scenario = name.to_owned();
        self.t = 0.0;
        println!("[eye-review] scenario {name}");
    }

    /// One live frame: simulate, render as the live loop does for the station, log.
    fn frame(&mut self, g: &mut Game, r: &mut Renderer, input: &TankInput, still: Option<&str>) -> anyhow::Result<()> {
        g.update(input, DT, r);
        if let Some(dir) = self.look { r.camera.set_forward(dir); }
        let sight = g.station == Station::Gunner && g.optic_composite_active();
        let rgba = if sight {
            crate::raymarch_showcase::eyepiece_frame_rgba(g, r, self.device, self.queue)?
        } else {
            let (w, h) = (r.iw, r.ih);
            r.render_to_rgba(self.device, self.queue, w, h)?
        };
        let lanes = r.debug_read_eye_lanes(self.device, self.queue, 0)?;
        let optic = if sight { Some(r.debug_read_eye_lanes(self.device, self.queue, crate::renderer::OPTIC_EYE_VIEW)?) } else { None };
        let clip = clip_stats(&rgba);
        let mut line = serde_json::json!({
            "scenario": self.scenario, "t": self.t, "station": format!("{:?}", g.station),
            "hatch_open": g.hatch_open, "head_out": g.commander_outside, "clock_utc": r.sky_clock.now().to_iso(),
            "black": clip[0], "near_black": clip[1], "near_white": clip[2], "white": clip[3], "mean_code": clip[4],
            "lanes": lanes.to_vec(), "optic_lanes": optic.map(|l| l.to_vec()),
        });
        if let Some(name) = still {
            let path = format!("{}_{}_{name}.png", self.prefix, self.scenario);
            image::RgbaImage::from_raw(r.iw, r.ih, rgba).context("still size")?.save(&path)?;
            line["still"] = serde_json::json!(path);
            println!("[eye-review] {path}: black {:.3} white {:.3} metered {:.4} cd/m^2 cone adaptation {:.4}",
                clip[0], clip[3], lanes[15], lanes[12]);
        }
        writeln!(self.log, "{line}")?;
        self.t += DT;
        Ok(())
    }

    /// `seconds` of live frames; stills at the listed times (seconds from the run's start).
    fn run(&mut self, g: &mut Game, r: &mut Renderer, input: &TankInput, seconds: f32, stills: &[(f32, &str)]) -> anyhow::Result<()> {
        let frames = (seconds / DT).round().max(1.0) as u32;
        let start = self.t;
        for i in 0..frames {
            let at = i as f32 * DT;
            let still = stills.iter().find(|(s, _)| (at - s).abs() < 0.5 * DT || (i + 1 == frames && *s >= seconds - 0.5 * DT))
                .map(|(_, n)| *n);
            let _ = start;
            self.frame(g, r, input, still)?;
        }
        Ok(())
    }
}

/// Set the sky's instant, frozen, and give the view a fresh eye: the crewman arrives adapted to
/// the new hour, as a view does on its first frame (the scenarios jump hours apart; an eye carried
/// from noon into the night would show the noon's bleached cones, not the night).
fn clock(r: &mut Renderer, queue: &wgpu::Queue, utc: &str) -> anyhow::Result<()> {
    let instant = crate::ephemeris::UtcInstant::parse_iso(utc)?;
    r.set_sky_clock(crate::celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    r.day_night_cycle = false;
    r.sky_time_scale = 0.0;
    r.reset_active_exposure_for_headless_capture(queue);
    Ok(())
}

/// The crewman's own post: buttoned commander, head out, or another station.
fn post(g: &mut Game, station: Station, hatch: bool, head_out: bool) {
    g.station = station;
    if g.hatch_open != hatch { g.toggle_hatch(); }
    g.hatch_open = hatch;
    g.commander_outside = head_out && hatch;
}

/// A direction `yaw_deg` about the tank's heading, `pitch_deg` up.
fn look(g: &Game, yaw_deg: f32, pitch_deg: f32) -> Vec3 {
    let f = g.player.forward();
    let base = f32::atan2(f.x, f.z) + yaw_deg.to_radians();
    let p = pitch_deg.to_radians();
    Vec3::new(base.sin() * p.cos(), p.sin(), base.cos() * p.cos())
}

pub fn run(prefix: &str, map: &str) -> anyhow::Result<()> {
    let bridge = map == "bridge";
    if bridge && std::env::var_os(crate::canyon::MANIFEST_ENV).is_none() {
        let root = std::env::var("KESSEL_ASSET_ROOT").unwrap_or_else(|_| ".".into());
        std::env::set_var(crate::canyon::MANIFEST_ENV, format!("{root}/assets/worldgen/workshops/spring-bridge-v2/manifest.v1.json"));
    }
    if std::env::var_os(crate::canyon::MANIFEST_ENV).is_none() {
        std::env::set_var(crate::canyon::MANIFEST_ENV, crate::canyon::V2_MANIFEST);
    }
    act_manifest::select_runtime_manifest(Some(crate::canyon::manifest_path()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.mute_audio();
    g.enable_canyon_course_seeded(29)?;
    g.enable_singleplayer_crew();
    g.set_defer_rtt_to_render(true);
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    let log_path = format!("{prefix}_{map}_frames.jsonl");
    let mut rig = Rig { device: &device, queue: &queue, prefix: prefix.to_owned(),
        log: std::io::BufWriter::new(std::fs::File::create(&log_path)?), scenario: String::new(), t: 0.0, look: None };
    let only = std::env::var("KESSEL_EYE_REVIEW_ONLY").ok();
    let wanted = |name: &str| only.as_ref().is_none_or(|o| o.split(',').any(|s| s.trim() == name));
    let idle = TankInput::default();

    if bridge {
        clock(&mut r, &queue, "1943-05-12T18:20:00Z")?;
        post(&mut g, Station::Commander, true, true);
        for (yaw, name) in [(0.0, "ahead"), (90.0, "right"), (180.0, "behind"), (270.0, "left")] {
            rig.look = Some(look(&g, yaw, 2.0));
            rig.begin(&format!("bluehour_{name}"));
            rig.run(&mut g, &mut r, &idle, 6.0, &[(6.0, "settled")])?;
        }
        rig.look = None;
        rig.begin("bluehour_cabin");
        post(&mut g, Station::Commander, false, false);
        rig.run(&mut g, &mut r, &idle, 8.0, &[(8.0, "settled")])?;
        rig.log.flush()?;
        println!("[eye-review] log {log_path}");
        return Ok(());
    }

    g.debug_place_tiger(TIGER, 0.0);
    const NOON: &str = "1942-12-21T09:00:00Z";
    const NIGHT: &str = "1942-12-21T19:30:00Z";
    const DEEP: &str = "1942-12-21T22:00:00Z";

    // 1-2. The cabin buttoned up by day; the hatch opens, the head goes out; and back in.
    if wanted("hatch") {
        clock(&mut r, &queue, NOON)?;
        post(&mut g, Station::Commander, false, false);
        rig.begin("cabin_day");
        rig.run(&mut g, &mut r, &idle, 10.0, &[(10.0, "settled")])?;
        rig.begin("hatch_open");
        g.toggle_hatch();
        rig.run(&mut g, &mut r, &idle, 3.0, &[(0.25, "lid_0p25s"), (1.0, "lid_1s"), (3.0, "lid_3s")])?;
        g.commander_outside = true;
        rig.run(&mut g, &mut r, &idle, 15.0, &[(0.25, "out_0p25s"), (1.0, "out_1s"), (3.0, "out_3s"), (6.0, "out_6s"), (15.0, "out_15s")])?;
        rig.begin("head_in");
        post(&mut g, Station::Commander, false, false);
        rig.run(&mut g, &mut r, &idle, 20.0, &[(0.5, "in_0p5s"), (2.0, "in_2s"), (5.0, "in_5s"), (10.0, "in_10s"), (20.0, "in_20s")])?;
        // The loader's compartment by day, buttoned.
        rig.begin("loader_day");
        post(&mut g, Station::Loader, false, false);
        rig.run(&mut g, &mut r, &idle, 8.0, &[(8.0, "settled")])?;
    }
    // 3-4. Head out at noon; the sky and the snow against the dark ground and trees.
    if wanted("noon") {
        clock(&mut r, &queue, NOON)?;
        post(&mut g, Station::Commander, true, true);
        rig.begin("noon_headout");
        rig.run(&mut g, &mut r, &idle, 8.0, &[(8.0, "settled")])?;
        for yaw in [0.0f32, 90.0, 180.0, 270.0] {
            rig.look = Some(look(&g, yaw, -3.0));
            rig.begin(&format!("noon_yaw{yaw:03}"));
            rig.run(&mut g, &mut r, &idle, 5.0, &[(5.0, "settled")])?;
        }
        rig.look = Some(look(&g, 0.0, 35.0));
        rig.begin("sky_to_ground");
        rig.run(&mut g, &mut r, &idle, 6.0, &[(6.0, "sky")])?;
        rig.look = Some(look(&g, 0.0, -14.0));
        rig.run(&mut g, &mut r, &idle, 10.0, &[(0.25, "ground_0p25s"), (2.0, "ground_2s"), (10.0, "ground_10s")])?;
        rig.look = None;
    }
    // 5. The main gun and an HE strike 60 m ahead, seen head out, by day and by night.
    for (when, utc) in [("day", NOON), ("night", NIGHT)] {
        if !wanted(&format!("flash_{when}")) { continue; }
        clock(&mut r, &queue, utc)?;
        post(&mut g, Station::Commander, true, true);
        rig.begin(&format!("muzzle_{when}"));
        rig.run(&mut g, &mut r, &idle, 10.0, &[(10.0, "before")])?;
        g.debug_staged_discharge(&mut r);
        rig.run(&mut g, &mut r, &idle, 12.0, &[(1.0 / 60.0, "flash"), (0.25, "0p25s"), (1.0, "1s"), (2.0, "2s"), (4.0, "4s"), (8.0, "8s"), (12.0, "12s")])?;
        rig.begin(&format!("impact_{when}"));
        rig.run(&mut g, &mut r, &idle, 4.0, &[(4.0, "before")])?;
        let forward = g.player.forward();
        let point = g.player.pos + forward * 60.0;
        let point = Vec3::new(point.x, g.hf.height_at(point.x, point.z), point.z);
        let incoming = (-forward * 20f32.to_radians().cos() - Vec3::Y * 20f32.to_radians().sin()) * 680.0;
        g.debug_ground_strike(point, incoming, 6.2, 76.2, &r);
        rig.run(&mut g, &mut r, &idle, 12.0, &[(1.0 / 60.0, "flash"), (0.25, "0p25s"), (1.0, "1s"), (2.0, "2s"), (4.0, "4s"), (8.0, "8s"), (12.0, "12s")])?;
    }
    // 6. The optics: the vision blocks (buttoned commander), the driver's visor and the TZF by day;
    //    the TZF at night through a shot.
    if wanted("optics") {
        clock(&mut r, &queue, NOON)?;
        for (station, name) in [(Station::Commander, "periscope_day"), (Station::Driver, "visor_day"), (Station::Gunner, "sight_day")] {
            post(&mut g, station, false, false);
            rig.begin(name);
            rig.run(&mut g, &mut r, &idle, 6.0, &[(6.0, "settled")])?;
        }
        clock(&mut r, &queue, NIGHT)?;
        post(&mut g, Station::Gunner, false, false);
        rig.begin("sight_night_fire");
        rig.run(&mut g, &mut r, &idle, 10.0, &[(10.0, "before")])?;
        g.debug_staged_discharge(&mut r);
        rig.run(&mut g, &mut r, &idle, 10.0, &[(1.0 / 60.0, "flash"), (0.5, "0p5s"), (2.0, "2s"), (5.0, "5s"), (10.0, "10s")])?;
    }
    // 8. A deep night, head out and buttoned.
    if wanted("night") {
        clock(&mut r, &queue, DEEP)?;
        post(&mut g, Station::Commander, true, true);
        rig.begin("deep_night_headout");
        rig.run(&mut g, &mut r, &idle, 10.0, &[(10.0, "settled")])?;
        post(&mut g, Station::Commander, false, false);
        rig.begin("deep_night_cabin");
        rig.run(&mut g, &mut r, &idle, 10.0, &[(10.0, "settled")])?;
    }
    rig.log.flush()?;
    println!("[eye-review] log {log_path}");
    Ok(())
}
