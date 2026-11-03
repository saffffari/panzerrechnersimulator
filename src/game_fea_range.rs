//! `fea-range`: the owner's FEA range, a windowed review editor for the layered damage model
//! (docs/FEA_DAMAGE_MODEL_20261102.md) built like the world editor (`edit`): the Canyon v2 winter
//! workshop's snow field (act 1), the player's Tiger with its five-man solo crew at the review spot
//! (`game_fea_review::TIGER_AT`), rendered by the shipping raymarch renderer and simulated by the
//! game's own update. Nothing here is a second simulation:
//!
//! - **Shot**: a round of any catalogue shell (`ballistics`) is launched by
//!   `Game::debug_launch_round` 9 m short of the first vehicle surface on its line, with the speed,
//!   flown distance and age of a shot from the chosen range (the impact velocity is the shell's own
//!   `v(x) = v0·e^(−kx)` at that range; setting a velocity sets the range that gives it). Its line
//!   is aimed by clicking the tank (the camera's ray: an interior surface seen through the section
//!   is aimed along the same line from outside) or at a crewman, and turned by azimuth/elevation.
//!   The ordinary flight sweep, terminal resolver, layered chain, voxel lattice, fragment tracing,
//!   ignition and the crew's voxel tissue resolve it.
//! - **Run and timeline**: a run fires the shot (on a fresh Tiger unless switched off) and records
//!   every frame it renders: the first span one fragment step (1/480 s) per frame, then real time
//!   at 30 frames a second. The timeline scrubs the recorded frames; "live here" re-simulates the
//!   same shot, seed and knobs on a fresh Tiger to that frame, step for step, so the camera and
//!   section can move about a solved state. Measured 2026-11-03 (`--shots`): the game does not
//!   repeat bit for bit, so this is a sibling of the recorded outcome, not a copy of it: the
//!   chain and the scars agreed, the crew's tissue and the surface blood did not (a re-run differs
//!   from the next re-run as well). The status line says so.
//! - **Section**: the film section (`Renderer::set_film_section`), a camera-only plane through the
//!   Tiger; shadows, light, ray queries and every simulation see the whole closed tank.
//! - **Sky**: the renderer's sky clock held at the chosen date and local time (Stalingrad), the
//!   weather override, the eye's exposure automatic or held.
//! - **Tissue**: review-only overrides of the crew lattice's authored constants
//!   (`crew_tissue::TissueTuning`), never a shipping default; P prints them for hand-over.
//!
//! `fea-range --shots PREFIX` renders the editor headless (stills of the interface, a short run,
//! a scrubbed frame and a live re-simulation with its agreement check). Requires the raymarch
//! renderer (set when KESSEL_RENDERER is unset).
use super::game_fea_review as review;
use super::*;
use crate::{act_manifest, celestial_sky, ephemeris, graphics, world};
use std::collections::HashSet;
use glam::Vec4;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// One fixed hot-fragment step: the slow-motion frame.
const SLOW_DT: f32 = 1.0 / 480.0;
const TICK: f32 = 1.0 / 60.0;
/// Real-time frames are recorded at 30 a second: two game ticks each.
const REAL_TICKS_PER_FRAME: u32 = 2;
/// Recorded frames are kept in memory up to this many bytes; a run stops recording past it.
const RECORD_BUDGET_BYTES: usize = 1_500_000_000;
/// After this long with nothing changing the window renders ten frames a second (the owner plays
/// on the same machine).
const IDLE_AFTER_S: f32 = 2.0;
/// Where the section's offset is measured from (hull frame): the fighting compartment's middle.
const SECTION_PIVOT: Vec3 = Vec3::new(0.0, 1.4, 0.0);
/// The vehicle's box for the section's meshes (hull frame), as the film's engine section.
const TANK_LO: Vec3 = Vec3::new(-2.6, -0.4, -4.8);
const TANK_HI: Vec3 = Vec3::new(2.6, 4.0, 8.5);
/// Stalingrad is three hours ahead of UTC in the campaign's clock (`game_fea_film`: 18:30 local is
/// 15:30Z).
const LOCAL_UTC_OFFSET_H: f64 = 3.0;

fn shells() -> [&'static crate::ballistics::ShellSpec; 16] {
    use crate::ballistics as b;
    [&b::BR_240_M42, &b::BR_240, &b::BR_240P, &b::BR_271_ZIS2, &b::BR_350B, &b::OF_350_F34, &b::BR_354P, &b::BR_365,
        &b::BR_412, &b::BR_471, &b::OF_530, &b::BR_540, &b::PZGR_39, &b::PZGR_40, &b::SPRGR_88, &b::PZGR_39_43]
}

/// Range (m) at which `shell` arrives at `v` m/s by its own decay law (0 at or above the muzzle).
fn range_for_velocity(shell: &crate::ballistics::ShellSpec, v: f32) -> f32 {
    if v >= shell.muzzle_velocity || shell.drag_k <= 0.0 {
        return 0.0;
    }
    ((shell.muzzle_velocity / v.max(1.0)).ln() / shell.drag_k).clamp(0.0, 6_000.0)
}

/// What the shot is aimed at.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    /// A point picked on the tank (hull frame).
    Point(Vec3),
    /// A crewman's body where he is at the moment of firing.
    Man(CrewRole),
}

#[derive(Clone, Copy, Debug)]
struct ShotSetup {
    shell: usize,
    range_m: f32,
    target: Target,
    /// Where the round comes from, hull frame: bearing (0 = from ahead, +90 = from the left, the
    /// hull's +X) and elevation (+ = from above), degrees.
    from_az: f32,
    from_el: f32,
    seed: u32,
}

impl ShotSetup {
    fn spec(&self) -> &'static crate::ballistics::ShellSpec {
        shells()[self.shell % shells().len()]
    }
    /// The round's travel direction, hull frame.
    fn dir_hull(&self) -> Vec3 {
        let (az, el) = (self.from_az.to_radians(), self.from_el.to_radians());
        -Vec3::new(el.cos() * az.sin(), el.sin(), el.cos() * az.cos())
    }
    fn set_dir_hull(&mut self, d: Vec3) {
        let from = -d.normalize_or_zero();
        self.from_az = from.x.atan2(from.z).to_degrees();
        self.from_el = from.y.clamp(-1.0, 1.0).asin().to_degrees();
    }
    fn impact_velocity(&self) -> f32 {
        self.spec().velocity_at(self.range_m)
    }
}

#[derive(Clone, Copy, Debug)]
struct SceneSetup {
    racks_loaded: bool,
    cabin_lamps: bool,
    fresh_each_run: bool,
    slow_s: f32,
    run_s: f32,
}

#[derive(Clone, Copy, Debug)]
struct Section {
    on: bool,
    /// Normal into the removed half, hull frame, as yaw (about +Y from +Z) and pitch.
    yaw: f32,
    pitch: f32,
    /// Offset of the plane from `SECTION_PIVOT` along its normal (m).
    offset: f32,
}

impl Section {
    fn normal_hull(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos())
    }
    fn set_normal_hull(&mut self, n: Vec3) {
        let n = n.normalize_or_zero();
        self.yaw = n.x.atan2(n.z).to_degrees();
        self.pitch = n.y.clamp(-1.0, 1.0).asin().to_degrees();
    }
}

#[derive(Clone, Copy, Debug)]
struct Sky {
    /// Days after 1942-12-21.
    day: i32,
    /// Local time, hours.
    local_h: f32,
    weather: f32,
    /// None: the eye's own automatic exposure; Some: held at this exposure.
    exposure: Option<f32>,
}

impl Sky {
    fn instant(&self) -> ephemeris::UtcInstant {
        ephemeris::UtcInstant::from_calendar(1942, 12, 21, 0, 0, 0.0)
            .plus_seconds(self.day as f64 * 86_400.0 + (self.local_h as f64 - LOCAL_UTC_OFFSET_H) * 3_600.0)
    }
}

/// A recorded frame: its sim time after the shot and the frame index of the run's schedule.
struct Frame {
    t: f32,
    rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// The present state, the camera free; the simulation held unless `live_sim`.
    Live,
    /// A run in progress: `frame` frames recorded of `total`.
    Recording { frame: u32, total: u32 },
    /// Showing a recorded frame.
    Scrub(usize),
}

/// Every adjustable row of the side panel.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Knob {
    Shell, Range, Velocity, FromAz, FromEl, TargetKind, Seed,
    Racks, Lamps, Fresh, SlowSpan, RunLength,
    SectionOn, SectionPreset, SectionYaw, SectionPitch, SectionOffset,
    Day, Time, Weather, Exposure,
    Tear(usize), Stretch(usize), Forward, Outward, Reach, Landing, TissueAuthored,
}

const TISSUE_NAMES: [&str; 4] = ["SKIN", "MUSCLE", "BONE", "FLUID"];

fn panel_rows() -> Vec<(Option<&'static str>, Knob)> {
    let mut rows = vec![
        (Some("SHOT"), Knob::Shell), (None, Knob::Range), (None, Knob::Velocity), (None, Knob::FromAz),
        (None, Knob::FromEl), (None, Knob::TargetKind), (None, Knob::Seed),
        (Some("SCENE"), Knob::Racks), (None, Knob::Lamps), (None, Knob::Fresh), (None, Knob::SlowSpan), (None, Knob::RunLength),
        (Some("SECTION (CAMERA ONLY)"), Knob::SectionOn), (None, Knob::SectionPreset), (None, Knob::SectionYaw),
        (None, Knob::SectionPitch), (None, Knob::SectionOffset),
        (Some("SKY"), Knob::Day), (None, Knob::Time), (None, Knob::Weather), (None, Knob::Exposure),
    ];
    rows.push((Some("TISSUE (REVIEW OVERRIDE)"), Knob::Tear(0)));
    for k in 1..4 {
        rows.push((None, Knob::Tear(k)));
    }
    for k in 0..4 {
        rows.push((None, Knob::Stretch(k)));
    }
    rows.extend([(None, Knob::Forward), (None, Knob::Outward), (None, Knob::Reach), (None, Knob::Landing), (None, Knob::TissueAuthored)]);
    rows
}

// Panel layout (normalized, top-left origin).
const PANEL_X: f32 = 0.006;
const PANEL_W: f32 = 0.27;
const PANEL_TOP: f32 = 0.05;
const ROW_H: f32 = 0.0205;
const HEADER_H: f32 = 0.024;
const TEXT: f32 = 0.0155;
const MINUS_X: f32 = PANEL_X + PANEL_W - 0.046;
const PLUS_X: f32 = PANEL_X + PANEL_W - 0.023;
const BTN_W: f32 = 0.019;
// Bottom bar.
const BAR_Y: f32 = 0.905;
const TRACK_X: f32 = 0.30;
const TRACK_W: f32 = 0.685;
const TRACK_Y: f32 = 0.955;
const TRACK_H: f32 = 0.026;
const BUTTONS: [(&str, f32); 6] = [("RESET", 0.30), ("RUN", 0.365), ("LIVE", 0.42), ("PLAY", 0.475), ("LIVE HERE", 0.53), ("SIM", 0.61)];
const BUTTON_W: f32 = 0.05;
const BUTTON_H: f32 = 0.034;

/// The panel's row geometry: (y of the row's centre, header above it).
fn row_layout() -> Vec<(f32, Option<&'static str>, Knob)> {
    let mut y = PANEL_TOP;
    let mut out = Vec::new();
    for (header, knob) in panel_rows() {
        if header.is_some() {
            y += HEADER_H;
        }
        out.push((y + ROW_H * 0.5, header, knob));
        y += ROW_H;
    }
    out
}

/// The range editor's state: the game, its renderer and everything the panel sets.
struct Range {
    device: wgpu::Device,
    queue: wgpu::Queue,
    r: Renderer,
    g: Game,
    shot: ShotSetup,
    scene: SceneSetup,
    section: Section,
    sky: Sky,
    tuning: crew_tissue::TissueTuning,
    mode: Mode,
    frames: Vec<Frame>,
    /// The setup the recorded frames were run with (for "live here").
    recorded: Option<(ShotSetup, SceneSetup, crew_tissue::TissueTuning)>,
    /// Sim time since the last shot (s), as the live state stands.
    shot_t: f32,
    /// The live simulation runs in real time (otherwise held).
    live_sim: bool,
    playing: bool,
    play_clock: f32,
    // Fly camera.
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    fov_deg: f32,
    speed: f32,
    // Display of recorded frames.
    frame_tex: wgpu::Texture,
    frame_image: u32,
    shown_frame: Option<usize>,
    status: String,
    help: bool,
    muted: bool,
    idle_s: f32,
    /// The last chain summary for the status line.
    last_chain: String,
}

fn fmt_kj(v: f32) -> String {
    if v >= 1.0e6 { format!("{:.2} MJ/M3", v / 1.0e6) } else { format!("{:.1} KJ/M3", v / 1.0e3) }
}

impl Range {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> anyhow::Result<Self> {
        act_manifest::select_runtime_manifest(Some(crate::canyon::manifest_path()))?;
        let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
        let frame_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fea-range-frame"),
            size: wgpu::Extent3d { width: r.iw, height: r.ih, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let frame_image = r.import_diagnostic_image(&device, &frame_tex.create_view(&wgpu::TextureViewDescriptor::default()));
        // The review scene as `fea-play` and `fea-film` set it: enemies away and holding fire, the
        // Tiger at the review spot on the snow field.
        g.debug_banish_enemies();
        g.debug_hold_enemy_fire();
        g.targets_only = true;
        g.god = false;
        g.debug_place_tiger(review::TIGER_AT, 0.0);
        let sky = Sky { day: 0, local_h: 13.5, weather: 0.0, exposure: None };
        let mut range = Self {
            device, queue, r, g,
            shot: ShotSetup { shell: 3, range_m: 400.0, target: Target::Man(CrewRole::Loader), from_az: -90.0, from_el: 0.0,
                seed: 0x5EC7_A000 },
            scene: SceneSetup { racks_loaded: false, cabin_lamps: true, fresh_each_run: true, slow_s: 0.25, run_s: 4.0 },
            section: Section { on: false, yaw: 0.0, pitch: 90.0, offset: 0.5 },
            sky,
            tuning: crew_tissue::TissueTuning::AUTHORED,
            mode: Mode::Live,
            frames: Vec::new(),
            recorded: None,
            shot_t: 0.0,
            live_sim: false,
            playing: false,
            play_clock: 0.0,
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_deg: 50.0,
            speed: 4.0,
            frame_tex,
            frame_image,
            shown_frame: None,
            status: "RESET: a fresh Tiger and crew".into(),
            help: false,
            muted: false,
            idle_s: 0.0,
            last_chain: String::new(),
        };
        range.apply_sky();
        range.fresh();
        // The first camera: outside, off the right front, the Tiger in the middle.
        let hull = range.g.player.hull_model();
        let eye = hull.transform_point3(Vec3::new(-7.5, 3.4, 6.5));
        range.look_at(eye, hull.transform_point3(Vec3::new(0.0, 1.4, 0.0)));
        Ok(range)
    }

    fn look_at(&mut self, eye: Vec3, at: Vec3) {
        let mut c = crate::camera::Camera::new(1.0);
        c.set_forward(at - eye);
        self.eye = eye;
        self.yaw = c.yaw;
        self.pitch = c.pitch;
    }

    fn forward(&self) -> Vec3 {
        let mut c = crate::camera::Camera::new(1.0);
        c.yaw = self.yaw;
        c.pitch = self.pitch;
        c.forward()
    }

    fn apply_sky(&mut self) {
        let clock = celestial_sky::SkyClock::new(self.sky.instant(), self.r.sky_clock.site);
        self.r.set_sky_clock(clock);
        self.r.set_weather_override(Some(self.sky.weather));
        self.r.exposure_override = self.sky.exposure;
    }

    fn apply_tuning(&self) {
        // Review-only: the override is set only while it differs from the authored constants.
        if self.tuning == crew_tissue::TissueTuning::AUTHORED {
            crew_tissue::set_review_tuning(None);
        } else {
            crew_tissue::set_review_tuning(Some(self.tuning));
        }
    }

    fn step(&mut self, dt: f32) {
        self.g.update(&TankInput::default(), dt, &mut self.r);
    }

    /// A fresh Tiger and crew at the review spot (`game_fea_film::fresh_tiger`), settled for a
    /// second of game time.
    fn fresh(&mut self) {
        self.g.enable_singleplayer_crew();
        self.g.reset_match(self.shot.seed, 0.0);
        self.g.dbg_reset_foot();
        self.g.player_burn.extinguish();
        self.g.debug_banish_enemies();
        self.g.debug_hold_enemy_fire();
        self.g.targets_only = true;
        self.g.god = false;
        self.g.debug_place_tiger(review::TIGER_AT, 0.0);
        if !self.scene.racks_loaded {
            self.g.debug_empty_ammunition();
        }
        self.g.set_cabin_lights(self.scene.cabin_lamps);
        self.g.jump_station(Station::Commander);
        if self.muted {
            self.g.mute_audio();
        }
        for _ in 0..60 {
            self.step(TICK);
        }
        self.shot_t = 0.0;
        self.last_chain.clear();
    }

    /// The shot's aim point and travel direction (world).
    fn aim_line(&self) -> (Vec3, Vec3) {
        let hull = self.g.player.hull_model();
        let dir = hull.transform_vector3(self.shot.dir_hull()).normalize_or_zero();
        let aim = match self.shot.target {
            Target::Point(p) => hull.transform_point3(p),
            Target::Man(role) => self.g.crew_solo_man_point(role),
        };
        (aim, dir)
    }

    /// The first vehicle surface on the shot's line coming in from 40 m out (the round's entry),
    /// and its normal: the round is launched 9 m short of it, so it meets everything on its line.
    fn entry(&self) -> Option<(Vec3, Vec3)> {
        let (aim, dir) = self.aim_line();
        let models = self.g.fire_surface_models();
        let from = aim - dir * 40.0;
        let to = aim + dir * 0.02;
        let contact = self.g.cabin_fire.sweep(from, to, &models)?;
        let normal = models[contact.frame].transform_vector3(contact.normal).normalize_or_zero();
        Some((from.lerp(to, contact.fraction), normal))
    }

    fn fire(&mut self) {
        let (aim, dir) = self.aim_line();
        let target = self.entry().map_or(aim, |(p, _)| p);
        let shell = self.shot.spec();
        self.g.debug_launch_round(shell, false, target, dir, self.shot.range_m, self.shot.seed);
        self.shot_t = 0.0;
    }

    fn slow_frames(&self, scene: &SceneSetup) -> u32 {
        (scene.slow_s / SLOW_DT).round() as u32
    }

    fn total_frames(&self, scene: &SceneSetup) -> u32 {
        let slow = self.slow_frames(scene);
        let rest = ((scene.run_s - slow as f32 * SLOW_DT).max(0.0) * 30.0).round() as u32;
        slow + rest
    }

    /// Advance the run's schedule by its frame `k`: one fragment step in the slow span, two ticks
    /// after it.
    fn advance_frame(&mut self, k: u32, scene: &SceneSetup) {
        if k < self.slow_frames(scene) {
            self.step(SLOW_DT);
            self.shot_t += SLOW_DT;
        } else {
            for _ in 0..REAL_TICKS_PER_FRAME {
                self.step(TICK);
            }
            self.shot_t += TICK * REAL_TICKS_PER_FRAME as f32;
        }
    }

    fn reset(&mut self) {
        self.mode = Mode::Live;
        self.playing = false;
        self.fresh();
        self.status = "RESET: a fresh Tiger and crew".into();
    }

    fn run(&mut self) {
        self.apply_tuning();
        if self.scene.fresh_each_run {
            self.fresh();
        }
        self.frames.clear();
        self.shown_frame = None;
        self.recorded = self.scene.fresh_each_run.then_some((self.shot, self.scene, self.tuning));
        self.fire();
        let total = self.total_frames(&self.scene);
        self.mode = Mode::Recording { frame: 0, total };
        self.playing = false;
        self.status = format!("RUN: {} at {:.0} m ({:.0} m/s)", self.shot.spec().name, self.shot.range_m, self.shot.impact_velocity());
    }

    /// The live state at recorded frame `index`: the recorded run repeated step for step.
    fn live_at(&mut self, index: usize) {
        let Some((shot, scene, tuning)) = self.recorded else {
            self.status = "LIVE HERE needs a run on a fresh Tiger (FRESH EACH RUN on)".into();
            return;
        };
        let (keep_shot, keep_scene, keep_tuning) = (self.shot, self.scene, self.tuning);
        self.shot = shot;
        self.scene = scene;
        self.tuning = tuning;
        self.apply_tuning();
        let started = std::time::Instant::now();
        self.fresh();
        self.fire();
        for k in 0..=index as u32 {
            self.advance_frame(k, &scene);
        }
        self.shot = keep_shot;
        self.scene = keep_scene;
        self.tuning = keep_tuning;
        self.apply_tuning();
        self.mode = Mode::Live;
        self.playing = false;
        self.status = format!("LIVE at {:.4} s: the same shot re-run in {:.1} s (the game does not repeat bit for bit: tissue detail can differ from the recording)",
            self.shot_t, started.elapsed().as_secs_f32());
    }

    /// The camera, the section and the crew drawing for this frame.
    fn pose_view(&mut self) {
        self.apply_sky();
        self.g.drive_armor_scars(&mut self.r);
        self.g.drive_singleplayer_crew_rigs_for_view(None, &mut self.r);
        self.r.camera.pos = self.eye;
        self.r.camera.yaw = self.yaw;
        self.r.camera.pitch = self.pitch;
        self.r.camera.fov_y = self.fov_deg.to_radians();
        self.r.optic_mask = 0.0;
        self.r.reticle = false;
        // Inside the hull's box the eye keeps the compartment's light and air; outside it is
        // an observer in the open (as `fea-film`'s Inside and Outside views).
        let local = self.g.player.hull_model().inverse().transform_point3(self.eye);
        let inside = local.x.abs() < 1.6 && local.y > 0.4 && local.y < 2.9 && local.z > -3.2 && local.z < 3.4;
        if !inside {
            self.r.interior_light = 0.0;
            self.r.interior_haze = 0.0;
            self.r.weather_shelter = 0.0;
            self.r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
        }
        match self.section_plane() {
            Some(plane) => {
                let meshes = self.section_meshes();
                self.r.set_film_section(Some(plane), &meshes);
            }
            None => self.r.set_film_section(None, &[]),
        }
    }

    fn section_plane(&self) -> Option<Vec4> {
        if !self.section.on {
            return None;
        }
        let hull = self.g.player.hull_model();
        let n_local = self.section.normal_hull();
        let n = hull.transform_vector3(n_local).normalize_or_zero();
        let p = hull.transform_point3(SECTION_PIVOT + n_local * self.section.offset);
        Some(n.extend(n.dot(p)))
    }

    fn section_meshes(&self) -> Vec<usize> {
        let hull = self.g.player.hull_model();
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for bits in 0..8u32 {
            let local = Vec3::new(if bits & 1 == 0 { TANK_LO.x } else { TANK_HI.x }, if bits & 2 == 0 { TANK_LO.y } else { TANK_HI.y },
                if bits & 4 == 0 { TANK_LO.z } else { TANK_HI.z });
            let p = hull.transform_point3(local);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        self.r.solid_meshes_within(lo, hi)
    }

    /// The camera ray through the window point `cursor` (0..1).
    fn ray(&self, cursor: [f32; 2]) -> (Vec3, Vec3) {
        let f = self.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(f).normalize_or_zero();
        let t = (self.fov_deg.to_radians() * 0.5).tan();
        let aspect = self.r.iw as f32 / self.r.ih.max(1) as f32;
        let x = (cursor[0] * 2.0 - 1.0) * t * aspect;
        let y = (1.0 - cursor[1] * 2.0) * t;
        (self.eye, (f + right * x + up * y).normalize())
    }

    /// The window point of a world point (None behind the camera).
    fn project(&self, p: Vec3) -> Option<[f32; 2]> {
        let mut c = crate::camera::Camera::new(self.r.iw as f32 / self.r.ih.max(1) as f32);
        c.pos = self.eye;
        c.yaw = self.yaw;
        c.pitch = self.pitch;
        c.fov_y = self.fov_deg.to_radians();
        let clip = c.view_proj() * p.extend(1.0);
        if clip.w <= 1.0e-4 {
            return None;
        }
        Some([(clip.x / clip.w + 1.0) * 0.5, (1.0 - clip.y / clip.w) * 0.5])
    }

    /// Aim at the vehicle surface under the cursor (beyond the section's cut, if the eye is in
    /// the removed half); the direction follows the camera's ray unless `keep_dir`.
    fn pick(&mut self, cursor: [f32; 2], keep_dir: bool) {
        let (mut from, dir) = self.ray(cursor);
        if let Some(plane) = self.section_plane() {
            let n = plane.truncate();
            let side = n.dot(from) - plane.w;
            let denom = n.dot(dir);
            if side > 0.0 && denom < 0.0 {
                from += dir * (side / -denom + 0.002);
            }
        }
        let models = self.g.fire_surface_models();
        let to = from + dir * 60.0;
        match self.g.cabin_fire.sweep(from, to, &models) {
            Some(contact) => {
                let hit = from.lerp(to, contact.fraction);
                let hull_inv = self.g.player.hull_model().inverse();
                self.shot.target = Target::Point(hull_inv.transform_point3(hit));
                if !keep_dir {
                    self.shot.set_dir_hull(hull_inv.transform_vector3(dir));
                }
                self.status = format!("AIM: picked at {:.2?} (hull frame)", hull_inv.transform_point3(hit).to_array());
            }
            None => self.status = "AIM: no vehicle surface under the cursor".into(),
        }
    }

    // ── The panel's knobs ───────────────────────────────────────────────────────────────────────

    fn knob_label(&self, knob: Knob) -> (String, String) {
        let t = &self.tuning;
        let a = crew_tissue::TissueTuning::AUTHORED;
        let mark = |changed: bool| if changed { " *" } else { "" };
        match knob {
            Knob::Shell => ("SHELL".into(), format!("{} {:.0} MM", self.shot.spec().name, self.shot.spec().caliber_mm)),
            Knob::Range => ("RANGE".into(), format!("{:.0} M", self.shot.range_m)),
            Knob::Velocity => ("IMPACT VELOCITY".into(), format!("{:.0} M/S (MUZZLE {:.0})", self.shot.impact_velocity(), self.shot.spec().muzzle_velocity)),
            Knob::FromAz => ("FROM BEARING".into(), format!("{:+.0} DEG {}", self.shot.from_az, match self.shot.from_az.round() as i32 {
                -20..=20 => "AHEAD", 21..=159 => "LEFT", -159..=-21 => "RIGHT", _ => "BEHIND" })),
            Knob::FromEl => ("FROM ELEVATION".into(), format!("{:+.0} DEG", self.shot.from_el)),
            Knob::TargetKind => ("AIM AT".into(), match self.shot.target {
                Target::Point(p) => format!("POINT {:.2} {:.2} {:.2}", p.x, p.y, p.z),
                Target::Man(role) => format!("{:?}", role).to_uppercase(),
            }),
            Knob::Seed => ("SEED".into(), format!("{:08X}", self.shot.seed)),
            Knob::Racks => ("RACKS".into(), if self.scene.racks_loaded { "LOADED" } else { "EMPTY" }.into()),
            Knob::Lamps => ("CABIN LAMPS".into(), if self.scene.cabin_lamps { "ON" } else { "OFF" }.into()),
            Knob::Fresh => ("FRESH TIGER EACH RUN".into(), if self.scene.fresh_each_run { "YES" } else { "NO (STACK)" }.into()),
            Knob::SlowSpan => ("1/480 S SPAN".into(), format!("{:.3} S", self.scene.slow_s)),
            Knob::RunLength => ("RUN LENGTH".into(), format!("{:.1} S", self.scene.run_s)),
            Knob::SectionOn => ("SECTION".into(), if self.section.on { "ON" } else { "OFF" }.into()),
            Knob::SectionPreset => ("PRESET".into(), "TOP LEFT RIGHT FRONT REAR".into()),
            Knob::SectionYaw => ("NORMAL YAW".into(), format!("{:+.0} DEG", self.section.yaw)),
            Knob::SectionPitch => ("NORMAL PITCH".into(), format!("{:+.0} DEG", self.section.pitch)),
            Knob::SectionOffset => ("OFFSET".into(), format!("{:+.2} M", self.section.offset)),
            Knob::Day => ("DATE".into(), self.sky.instant().to_iso()[..10].to_string()),
            Knob::Time => ("LOCAL TIME".into(), format!("{:02}:{:02}", self.sky.local_h.floor() as i32, ((self.sky.local_h.fract()) * 60.0).round() as i32 % 60)),
            Knob::Weather => ("WEATHER".into(), format!("{:.1}", self.sky.weather)),
            Knob::Exposure => ("EXPOSURE".into(), match self.sky.exposure { None => "AUTO (EYE)".into(), Some(e) => format!("HELD {e:.0}") }),
            Knob::Tear(k) => (format!("TEAR {}", TISSUE_NAMES[k]), format!("{}{}", fmt_kj(t.tear_j_m3[k]), mark(t.tear_j_m3[k] != a.tear_j_m3[k]))),
            Knob::Stretch(k) => (format!("STRETCH {}", TISSUE_NAMES[k]), format!("{:.3}{}", t.critical_stretch[k], mark(t.critical_stretch[k] != a.critical_stretch[k]))),
            Knob::Forward => ("FORWARD SHARE".into(), format!("{:.2}{}", t.forward, mark(t.forward != a.forward))),
            Knob::Outward => ("OUTWARD SHARE".into(), format!("{:.2}{}", t.outward, mark(t.outward != a.outward))),
            Knob::Reach => ("ACTIVE REACH".into(), format!("{:.3} M{}", t.active_reach_m, mark(t.active_reach_m != a.active_reach_m))),
            Knob::Landing => ("LANDING BREAKUP".into(), format!("X{:.2}{}", t.landing_breakup, mark(t.landing_breakup != a.landing_breakup))),
            Knob::TissueAuthored => ("RESTORE AUTHORED".into(), if *t == a { "IN FORCE".into() } else { "CLICK".into() }),
        }
    }

    /// Adjust `knob` by `dir` steps (+1/−1); `fine` and `coarse` scale the step.
    fn adjust(&mut self, knob: Knob, dir: f32, fine: bool, coarse: bool) {
        let s = dir * if fine { 0.1 } else if coarse { 10.0 } else { 1.0 };
        // A multiplicative step for energies and factors: 1.25 per click.
        let ratio = 1.25f32.powf(s);
        let n = shells().len() as i32;
        match knob {
            // The range is kept; the new shell's own velocity at it follows.
            Knob::Shell => self.shot.shell = (self.shot.shell as i32 + dir as i32).rem_euclid(n) as usize,
            Knob::Range => self.shot.range_m = (self.shot.range_m + 50.0 * s).clamp(0.0, 6_000.0),
            Knob::Velocity => {
                let v = (self.shot.impact_velocity() + 10.0 * s).max(50.0);
                self.shot.range_m = range_for_velocity(self.shot.spec(), v);
            }
            Knob::FromAz => self.shot.from_az = ((self.shot.from_az + 5.0 * s + 180.0).rem_euclid(360.0)) - 180.0,
            Knob::FromEl => self.shot.from_el = (self.shot.from_el + 1.0 * s).clamp(-60.0, 80.0),
            Knob::TargetKind => {
                let order = [CrewRole::Commander, CrewRole::Gunner, CrewRole::Loader, CrewRole::Driver, CrewRole::Radioman];
                self.shot.target = match self.shot.target {
                    Target::Point(_) => Target::Man(order[if dir > 0.0 { 0 } else { 4 }]),
                    Target::Man(role) => {
                        let i = order.iter().position(|r| *r == role).unwrap_or(0) as i32 + dir as i32;
                        if (0..5).contains(&i) { Target::Man(order[i as usize]) } else {
                            // Past either end: the man's present point, as a picked point.
                            Target::Point(self.g.player.hull_model().inverse().transform_point3(self.g.crew_solo_man_point(role)))
                        }
                    }
                };
            }
            Knob::Seed => self.shot.seed = self.shot.seed.wrapping_add_signed(s as i32),
            Knob::Racks => self.scene.racks_loaded = !self.scene.racks_loaded,
            Knob::Lamps => {
                self.scene.cabin_lamps = !self.scene.cabin_lamps;
                self.g.set_cabin_lights(self.scene.cabin_lamps);
            }
            Knob::Fresh => self.scene.fresh_each_run = !self.scene.fresh_each_run,
            Knob::SlowSpan => self.scene.slow_s = (self.scene.slow_s + 0.05 * s).clamp(0.0, 2.0),
            Knob::RunLength => self.scene.run_s = (self.scene.run_s + 0.5 * s).clamp(0.1, 60.0),
            Knob::SectionOn => self.section.on = !self.section.on,
            Knob::SectionPreset => {
                let presets = [Vec3::Y, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];
                let n = self.section.normal_hull();
                let now = presets.iter().position(|p| p.dot(n) > 0.999).map_or(-1, |i| i as i32);
                let next = (now + dir as i32).rem_euclid(presets.len() as i32) as usize;
                self.section.set_normal_hull(presets[next]);
                self.section.offset = if next == 0 { 0.5 } else { 0.0 };
                self.section.on = true;
            }
            Knob::SectionYaw => self.section.yaw = ((self.section.yaw + 5.0 * s + 180.0).rem_euclid(360.0)) - 180.0,
            Knob::SectionPitch => self.section.pitch = (self.section.pitch + 5.0 * s).clamp(-90.0, 90.0),
            Knob::SectionOffset => self.section.offset = (self.section.offset + 0.02 * s).clamp(-6.0, 6.0),
            Knob::Day => self.sky.day = (self.sky.day + s as i32).clamp(-365, 365),
            Knob::Time => self.sky.local_h = (self.sky.local_h + 0.25 * s).rem_euclid(24.0),
            Knob::Weather => self.sky.weather = (self.sky.weather + 0.1 * s).clamp(0.0, 1.0),
            Knob::Exposure => {
                // A third of a stop per click from the eye's present value; below 1 returns to auto.
                let now = self.sky.exposure.unwrap_or_else(|| self.metered_exposure().unwrap_or(1000.0));
                let next = now * 2f32.powf(s / 3.0);
                self.sky.exposure = if dir < 0.0 && self.sky.exposure.is_none() { Some(next) }
                    else if next < 1.0 { None } else { Some(next) };
            }
            Knob::Tear(k) => self.tuning.tear_j_m3[k] = (self.tuning.tear_j_m3[k] * ratio).clamp(100.0, 1.0e9),
            Knob::Stretch(k) => self.tuning.critical_stretch[k] = (self.tuning.critical_stretch[k] * 1.1f32.powf(s)).clamp(0.001, 10.0),
            Knob::Forward => self.tuning.forward = (self.tuning.forward + 0.05 * s).clamp(0.0, 2.0),
            Knob::Outward => self.tuning.outward = (self.tuning.outward + 0.05 * s).clamp(0.0, 2.0),
            Knob::Reach => self.tuning.active_reach_m = (self.tuning.active_reach_m + 0.01 * s).clamp(0.01, 0.6),
            Knob::Landing => self.tuning.landing_breakup = (self.tuning.landing_breakup * ratio).clamp(0.01, 1.0e4),
            Knob::TissueAuthored => self.tuning = crew_tissue::TissueTuning::AUTHORED,
        }
        self.apply_tuning();
    }

    /// The eye's present automatic exposure (to start a held exposure from).
    fn metered_exposure(&mut self) -> Option<f32> {
        self.r.debug_read_active_exposure(&self.device, &self.queue).ok().map(|[e, _]| e)
    }

    /// The knob set as Rust constants and JSON, for the FEA session (P).
    fn print_tuning(&self) -> String {
        let t = &self.tuning;
        let text = format!(
            "const TEAR_J_M3: [f32; 4] = [{:.1}, {:.1}, {:.1}, {:.1}];\nconst CRITICAL_STRETCH: [f32; 4] = [{:.4}, {:.4}, {:.4}, {:.4}];\n\
             const FORWARD: f32 = {:.3};\nconst OUTWARD: f32 = {:.3};\nconst ACTIVE_REACH_M: f32 = {:.4};\n// landing breakup factor x{:.3} (1.0 authored)\n",
            t.tear_j_m3[0], t.tear_j_m3[1], t.tear_j_m3[2], t.tear_j_m3[3], t.critical_stretch[0], t.critical_stretch[1],
            t.critical_stretch[2], t.critical_stretch[3], t.forward, t.outward, t.active_reach_m, t.landing_breakup);
        let json = serde_json::json!({"tear_j_m3": t.tear_j_m3, "critical_stretch": t.critical_stretch, "forward": t.forward,
            "outward": t.outward, "active_reach_m": t.active_reach_m, "landing_breakup": t.landing_breakup,
            "shot": {"shell": self.shot.spec().name, "range_m": self.shot.range_m, "impact_mps": self.shot.impact_velocity(),
                "from_az_deg": self.shot.from_az, "from_el_deg": self.shot.from_el, "seed": self.shot.seed,
                "target": format!("{:?}", self.shot.target)}});
        let path = "target/fea_range_tuning.json";
        let _ = std::fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default());
        println!("[fea-range] tissue knobs (review override; written to {path}):\n{text}");
        text
    }

    /// A digest of the solved state (the crew's bodies and tissue, the chain, the scars) to check
    /// a re-simulation against its recording.
    fn digest(&self) -> String {
        let bodies = self.g.debug_crew_solo_bodies();
        let chain: Vec<String> = self.g.fea.receipts.iter().map(|c| c.result.summary()).collect();
        let scars = self.g.armor_scars.scars().len();
        let tissue = self.g.crew_corpse_render.borrow().as_ref().map(|c| c.receipt()["tissue"].clone());
        format!("bodies={bodies} chain={chain:?} scars={scars} tissue={}", tissue.unwrap_or_default())
    }

    // ── Per-frame ──────────────────────────────────────────────────────────────────────────────

    /// Advance whatever is running by one window frame of `dt` seconds.
    fn tick(&mut self, dt: f32) {
        match self.mode {
            Mode::Recording { frame, total } => {
                let scene = self.scene;
                self.advance_frame(frame, &scene);
                self.pose_view();
                match self.r.render_to_rgba(&self.device, &self.queue, self.r.iw, self.r.ih) {
                    Ok(rgba) => {
                        let bytes = rgba.len() * (self.frames.len() + 1);
                        self.frames.push(Frame { t: self.shot_t, rgba });
                        let next = frame + 1;
                        if next >= total || bytes > RECORD_BUDGET_BYTES {
                            self.mode = Mode::Scrub(self.frames.len() - 1);
                            self.status = format!("RUN DONE: {} frames, {:.2} s{}", self.frames.len(), self.shot_t,
                                if bytes > RECORD_BUDGET_BYTES { " (memory budget reached)" } else { "" });
                        } else {
                            self.mode = Mode::Recording { frame: next, total };
                        }
                    }
                    Err(e) => {
                        self.status = format!("RUN: capture failed: {e:#}");
                        self.mode = Mode::Live;
                    }
                }
                self.idle_s = 0.0;
            }
            Mode::Scrub(i) if self.playing => {
                // Playback at the recording's own pace: slow-motion frames at 30 a second (1/16
                // speed), real-time frames at 30 a second.
                self.play_clock += dt * 30.0;
                let advance = self.play_clock.floor() as usize;
                self.play_clock -= advance as f32;
                let next = i + advance;
                if next >= self.frames.len() {
                    self.mode = Mode::Scrub(self.frames.len().saturating_sub(1));
                    self.playing = false;
                } else {
                    self.mode = Mode::Scrub(next);
                }
                self.idle_s = 0.0;
            }
            Mode::Live if self.live_sim => {
                let steps = (dt / TICK).round().clamp(1.0, 4.0) as u32;
                for _ in 0..steps {
                    self.step(TICK);
                }
                self.shot_t += steps as f32 * TICK;
                self.idle_s = 0.0;
            }
            _ => {}
        }
        if let Some(c) = self.g.fea.receipts.last() {
            self.last_chain = format!("{} {:?}: {}", c.shell, c.terminal, c.result.summary());
        }
    }

    /// Show recorded frame `i` in the frame image (uploaded once per change).
    fn show_frame(&mut self, i: usize) {
        if self.shown_frame == Some(i) {
            return;
        }
        let Some(frame) = self.frames.get(i) else { return };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.frame_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &frame.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.r.iw * 4), rows_per_image: Some(self.r.ih) },
            wgpu::Extent3d { width: self.r.iw, height: self.r.ih, depth_or_array_layers: 1 },
        );
        self.shown_frame = Some(i);
    }

    // ── The interface ──────────────────────────────────────────────────────────────────────────

    fn dot(&mut self, p: [f32; 2], size: f32, color: [f32; 4]) {
        let aspect = self.r.iw as f32 / self.r.ih.max(1) as f32;
        self.r.fill_rect(p[0] - size * 0.5 / aspect, p[1] - size * 0.5, size / aspect, size, color);
    }

    fn line(&mut self, a: Vec3, b: Vec3, color: [f32; 4]) {
        for k in 0..=40 {
            if let Some(p) = self.project(a.lerp(b, k as f32 / 40.0)) {
                self.dot(p, 0.0035, color);
            }
        }
    }

    /// The shot's gizmo (its line in from 6 m out, the entry and aim points) and the section's
    /// outline, over the scene.
    fn draw_gizmos(&mut self) {
        let (aim, dir) = self.aim_line();
        let entry = self.entry();
        let start = entry.map_or(aim, |(p, _)| p);
        self.line(start - dir * 6.0, start, [1.0, 0.55, 0.1, 0.95]);
        self.line(start, aim, [1.0, 0.85, 0.3, 0.6]);
        if let Some(p) = self.project(aim) {
            self.dot(p, 0.012, [1.0, 0.9, 0.2, 1.0]);
        }
        if let Some((e, n)) = entry {
            if let Some(p) = self.project(e) {
                self.dot(p, 0.009, [1.0, 0.3, 0.1, 1.0]);
            }
            self.line(e, e + n * 0.6, [0.4, 0.8, 1.0, 0.8]);
        }
        if let Some(plane) = self.section_plane() {
            let n = plane.truncate();
            let centre = self.g.player.hull_model().transform_point3(SECTION_PIVOT);
            let on = centre - n * (n.dot(centre) - plane.w);
            let u = n.cross(if n.y.abs() > 0.9 { Vec3::X } else { Vec3::Y }).normalize_or_zero();
            let v = n.cross(u).normalize_or_zero();
            let c = [0.3, 1.0, 0.6, 0.7];
            let k = 3.5;
            let corners = [on + (u + v) * k, on + (u - v) * k, on + (-u - v) * k, on + (-u + v) * k];
            for i in 0..4 {
                self.line(corners[i], corners[(i + 1) % 4], c);
            }
            self.line(on, on + n * 0.8, c);
        }
    }

    fn draw_ui(&mut self, cursor: [f32; 2]) {
        self.r.ui_reset();
        let scrubbing = matches!(self.mode, Mode::Scrub(_));
        // A recorded frame (or, while recording, the frame just captured) fills the window.
        let shown = match self.mode {
            Mode::Scrub(i) => Some(i),
            Mode::Recording { .. } => self.frames.len().checked_sub(1),
            Mode::Live => None,
        };
        match shown {
            Some(i) => {
                self.show_frame(i);
                self.r.draw_image(self.frame_image, 0.0, 0.0, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0]);
            }
            None if self.mode == Mode::Live => self.draw_gizmos(),
            None => {}
        }
        let ink = [0.90, 0.88, 0.82, 1.0];
        let dim = [0.62, 0.60, 0.55, 1.0];
        let accent = [1.0, 0.72, 0.30, 1.0];
        // Status line.
        let mode = match self.mode {
            Mode::Live => if self.live_sim { "LIVE, SIM RUNNING" } else { "LIVE, SIM HELD" }.to_string(),
            Mode::Recording { frame, total } => format!("RECORDING {frame}/{total}"),
            Mode::Scrub(i) => format!("RECORDED FRAME {}/{}", i + 1, self.frames.len()),
        };
        let t = match self.mode {
            Mode::Scrub(i) => self.frames.get(i).map_or(0.0, |f| f.t),
            _ => self.shot_t,
        };
        self.r.fill_rect(0.0, 0.0, 1.0, 0.042, [0.0, 0.0, 0.0, 0.62]);
        self.r.draw_text(&format!("FEA RANGE   {mode}   T = {:.4} S AFTER THE SHOT   {}", t, self.status.to_uppercase()),
            0.008, 0.026, TEXT, ink, crate::ui::Align::Left);
        let alive = CrewRole::ALL.iter().filter(|&&role| !self.g.crew_solo.man(role).dead).count();
        let chain: String = self.last_chain.chars().take(150).collect::<String>().to_uppercase();
        self.r.draw_text(&format!("CREW ALIVE {alive}/5   {chain}"), 0.30, 0.060, TEXT * 0.85, dim, crate::ui::Align::Left);
        if self.section.on {
            self.r.draw_text("SECTION FOR THE CAMERA ONLY: LIGHT AND PHYSICS SEE THE WHOLE TANK", 0.30, 0.083, TEXT * 0.85, [0.5, 1.0, 0.7, 1.0], crate::ui::Align::Left);
        }
        // Side panel.
        let layout = row_layout();
        let bottom = layout.last().map_or(0.5, |r| r.0 + ROW_H);
        self.r.fill_rect(PANEL_X, PANEL_TOP, PANEL_W, bottom - PANEL_TOP + 0.006, [0.02, 0.02, 0.02, 0.70]);
        for (y, header, knob) in layout {
            if let Some(h) = header {
                self.r.draw_text_bold(h, PANEL_X + 0.006, y - ROW_H * 0.5 - 0.004, TEXT, accent, crate::ui::Align::Left);
            }
            let hover = cursor[0] >= PANEL_X && cursor[0] <= PANEL_X + PANEL_W && (cursor[1] - y).abs() <= ROW_H * 0.5;
            if hover {
                self.r.fill_rect(PANEL_X, y - ROW_H * 0.5, PANEL_W, ROW_H, [1.0, 1.0, 1.0, 0.08]);
            }
            let (label, value) = self.knob_label(knob);
            self.r.draw_text(&label, PANEL_X + 0.010, y + 0.006, TEXT * 0.9, dim, crate::ui::Align::Left);
            self.r.draw_text(&value, PANEL_X + 0.105, y + 0.006, TEXT * 0.9, ink, crate::ui::Align::Left);
            for (x, sign) in [(MINUS_X, "-"), (PLUS_X, "+")] {
                self.r.fill_rect(x, y - ROW_H * 0.42, BTN_W, ROW_H * 0.84, [1.0, 1.0, 1.0, 0.12]);
                self.r.draw_text_bold(sign, x + BTN_W * 0.5, y + 0.006, TEXT, ink, crate::ui::Align::Center);
            }
        }
        // Bottom bar: buttons and the timeline.
        self.r.fill_rect(TRACK_X - 0.006, BAR_Y - 0.008, TRACK_W + 0.012, 1.0 - BAR_Y + 0.008, [0.02, 0.02, 0.02, 0.70]);
        for (label, x) in BUTTONS {
            let lit = match label {
                "PLAY" => self.playing,
                "SIM" => self.live_sim,
                "LIVE" => self.mode == Mode::Live,
                _ => false,
            };
            let w = if label == "LIVE HERE" { BUTTON_W * 1.5 } else { BUTTON_W };
            self.r.fill_rect(x, BAR_Y, w, BUTTON_H, if lit { [1.0, 0.72, 0.3, 0.5] } else { [1.0, 1.0, 1.0, 0.14] });
            let text = if label == "PLAY" && self.playing { "PAUSE" } else { label };
            self.r.draw_text_bold(text, x + w * 0.5, BAR_Y + BUTTON_H * 0.68, TEXT * 0.9, ink, crate::ui::Align::Center);
        }
        let speed = format!("FLY {:.1} M/S   FOV {:.0}", self.speed, self.fov_deg);
        self.r.draw_text(&speed, TRACK_X + TRACK_W, BAR_Y + BUTTON_H * 0.68, TEXT * 0.85, dim, crate::ui::Align::Right);
        self.r.fill_rect(TRACK_X, TRACK_Y, TRACK_W, TRACK_H, [1.0, 1.0, 1.0, 0.10]);
        let n = self.frames.len();
        if n > 0 {
            // The slow-motion span is drawn in amber, real time in grey.
            let slow = self.frames.iter().filter(|f| f.t <= self.recorded.map_or(self.scene.slow_s, |r| r.1.slow_s) + 1.0e-5).count();
            let w = TRACK_W / n as f32;
            self.r.fill_rect(TRACK_X, TRACK_Y + TRACK_H * 0.7, w * slow as f32, TRACK_H * 0.3, [1.0, 0.6, 0.2, 0.7]);
            self.r.fill_rect(TRACK_X + w * slow as f32, TRACK_Y + TRACK_H * 0.7, w * (n - slow) as f32, TRACK_H * 0.3, [0.7, 0.7, 0.7, 0.6]);
            let at = match self.mode {
                Mode::Scrub(i) => Some(i),
                Mode::Recording { .. } => Some(n - 1),
                Mode::Live => None,
            };
            if let Some(i) = at {
                self.r.fill_rect(TRACK_X + w * i as f32, TRACK_Y - 0.004, w.max(0.002), TRACK_H + 0.008, [1.0, 0.9, 0.4, 1.0]);
            }
            let last = self.frames.last().map_or(0.0, |f| f.t);
            self.r.draw_text(&format!("{n} FRAMES   {:.2} S   AMBER: 1/480 S STEPS (1/16 SPEED)   GREY: REAL TIME, 30 FPS", last),
                TRACK_X + 0.004, TRACK_Y + TRACK_H * 0.55, TEXT * 0.75, ink, crate::ui::Align::Left);
        } else {
            self.r.draw_text("NO RUN YET: AIM, THEN RUN (ENTER)", TRACK_X + 0.004, TRACK_Y + TRACK_H * 0.62, TEXT * 0.8, dim, crate::ui::Align::Left);
        }
        if scrubbing {
            self.r.draw_text("RECORDED FRAME: THE CAMERA AND SECTION MOVE ONLY IN LIVE (L = LIVE HERE)", 0.30, 0.106, TEXT * 0.85, accent, crate::ui::Align::Left);
        }
        if self.help {
            let lines = [
                "RMB HOLD: LOOK    WASD/QE: FLY    SHIFT: FAST    CTRL: SLOW    WHEEL: FLY SPEED    [ ]: FOV",
                "LMB ON THE TANK: AIM (DIRECTION FROM THE CAMERA)    CTRL+LMB: AIM, KEEP DIRECTION",
                "SHIFT+LMB DRAG: TURN THE SHOT    ALT+LMB DRAG: SECTION (UP/DOWN OFFSET, LEFT/RIGHT YAW)",
                "C: SECTION ON/OFF    V: SECTION FACING THE CAMERA    PANEL: CLICK - / +, OR WHEEL OVER A ROW",
                "ENTER: RUN    BACKSPACE: RESET    SPACE: PLAY/PAUSE (OR SIM IN LIVE)    LEFT/RIGHT: FRAME    HOME/END",
                "L: LIVE AT THIS FRAME (THE SHOT RE-RUN; DETAIL CAN DIFFER)    TAB: LIVE/RECORDING    P: PRINT TISSUE KNOBS    M: MUTE",
                "F12: SCREENSHOT TO CAPTURES    H: THIS HELP    ESC TWICE: QUIT",
            ];
            self.r.fill_rect(0.30, 0.13, 0.66, 0.03 * lines.len() as f32 + 0.02, [0.0, 0.0, 0.0, 0.75]);
            for (k, l) in lines.iter().enumerate() {
                self.r.draw_text(l, 0.31, 0.155 + 0.03 * k as f32, TEXT * 0.9, ink, crate::ui::Align::Left);
            }
        } else {
            self.r.draw_text("H: HELP", 0.995, 0.026, TEXT, dim, crate::ui::Align::Right);
        }
    }

    /// A click on the panel or bar; true when it was used.
    fn click_ui(&mut self, cursor: [f32; 2], fine: bool, coarse: bool) -> bool {
        if cursor[0] >= PANEL_X && cursor[0] <= PANEL_X + PANEL_W {
            for (y, _, knob) in row_layout() {
                if (cursor[1] - y).abs() <= ROW_H * 0.5 {
                    let dir = if cursor[0] >= PLUS_X { 1.0 } else if cursor[0] >= MINUS_X { -1.0 } else { 1.0 };
                    self.adjust(knob, dir, fine, coarse);
                    return true;
                }
            }
            if cursor[1] <= row_layout().last().map_or(0.0, |r| r.0 + ROW_H) {
                return true;
            }
        }
        if cursor[1] >= BAR_Y && cursor[1] <= BAR_Y + BUTTON_H {
            for (label, x) in BUTTONS {
                let w = if label == "LIVE HERE" { BUTTON_W * 1.5 } else { BUTTON_W };
                if cursor[0] >= x && cursor[0] <= x + w {
                    self.button(label);
                    return true;
                }
            }
        }
        if self.track_hit(cursor).is_some() {
            return true;
        }
        false
    }

    fn track_hit(&self, cursor: [f32; 2]) -> Option<usize> {
        let n = self.frames.len();
        (n > 0 && cursor[0] >= TRACK_X && cursor[0] <= TRACK_X + TRACK_W && cursor[1] >= TRACK_Y - 0.01
            && cursor[1] <= TRACK_Y + TRACK_H + 0.01)
            .then(|| (((cursor[0] - TRACK_X) / TRACK_W * n as f32) as usize).min(n - 1))
    }

    fn button(&mut self, label: &str) {
        match label {
            "RESET" => self.reset(),
            "RUN" => self.run(),
            "LIVE" => {
                if matches!(self.mode, Mode::Recording { .. }) {
                    return;
                }
                self.mode = Mode::Live;
                self.playing = false;
            }
            "PLAY" => {
                if !self.frames.is_empty() {
                    if let Mode::Scrub(i) = self.mode {
                        if i + 1 >= self.frames.len() && !self.playing {
                            self.mode = Mode::Scrub(0);
                        }
                    } else if !matches!(self.mode, Mode::Recording { .. }) {
                        self.mode = Mode::Scrub(0);
                    }
                    self.playing = !self.playing;
                    self.play_clock = 0.0;
                }
            }
            "LIVE HERE" => {
                if let Mode::Scrub(i) = self.mode {
                    self.live_at(i);
                }
            }
            "SIM" => self.live_sim = !self.live_sim,
            _ => {}
        }
    }

    /// Render a frame into `view`: the live scene, or a recorded frame, and the interface over it.
    fn render(&mut self, view: &wgpu::TextureView, format: wgpu::TextureFormat, w: u32, h: u32, cursor: [f32; 2]) {
        if self.mode == Mode::Live {
            self.pose_view();
            self.r.render_to_surface(&self.device, &self.queue, view, format);
        }
        self.draw_ui(cursor);
        self.r.render_hud_over_surface(&self.device, &self.queue, view, format, w, h);
    }
}

// ── The window ─────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    /// Shift+LMB: turning the shot's direction.
    Turn,
    /// Alt+LMB: moving the section (up/down: offset; left/right: yaw).
    Section,
    /// LMB on the timeline.
    Track,
}

struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    range: Range,
}

#[derive(Default)]
struct Input {
    keys: HashSet<KeyCode>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    cursor: [f32; 2],
    look: bool,
    mouse: Vec2,
    drag: Option<(Drag, [f32; 2])>,
}

struct App {
    state: Option<WindowState>,
    input: Input,
    last: Option<std::time::Instant>,
    active: Option<std::time::Instant>,
    escape: Option<std::time::Instant>,
    error: Option<String>,
}

impl App {
    fn busy(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            let r = &s.range;
            matches!(r.mode, Mode::Recording { .. }) || r.playing || r.live_sim
        }) || self.input.look || !self.input.keys.is_empty() || self.input.drag.is_some()
            || self.active.is_some_and(|t| t.elapsed().as_secs_f32() < IDLE_AFTER_S)
    }

    fn key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else { return };
        let rg = &mut st.range;
        match code {
            KeyCode::Escape => {
                if self.escape.is_some_and(|t| t.elapsed().as_secs_f32() < 2.0) {
                    event_loop.exit();
                } else {
                    self.escape = Some(std::time::Instant::now());
                    rg.status = "ESC AGAIN TO QUIT".into();
                }
            }
            KeyCode::Enter | KeyCode::NumpadEnter => rg.run(),
            KeyCode::Backspace => rg.reset(),
            KeyCode::Space => {
                if rg.mode == Mode::Live {
                    rg.live_sim = !rg.live_sim;
                } else {
                    rg.button("PLAY");
                }
            }
            KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                let n = rg.frames.len();
                if n > 0 && !matches!(rg.mode, Mode::Recording { .. }) {
                    let step = if self.input.shift { 10 } else { 1 } as i64 * if code == KeyCode::ArrowLeft { -1 } else { 1 };
                    let at = match rg.mode { Mode::Scrub(i) => i as i64, _ => n as i64 - 1 };
                    rg.mode = Mode::Scrub((at + step).clamp(0, n as i64 - 1) as usize);
                    rg.playing = false;
                }
            }
            KeyCode::Home | KeyCode::End => {
                let n = rg.frames.len();
                if n > 0 && !matches!(rg.mode, Mode::Recording { .. }) {
                    rg.mode = Mode::Scrub(if code == KeyCode::Home { 0 } else { n - 1 });
                    rg.playing = false;
                }
            }
            KeyCode::Tab => {
                rg.mode = match rg.mode {
                    Mode::Live if !rg.frames.is_empty() => Mode::Scrub(rg.frames.len() - 1),
                    Mode::Scrub(_) => Mode::Live,
                    m => m,
                };
                rg.playing = false;
            }
            KeyCode::KeyL => rg.button("LIVE HERE"),
            KeyCode::KeyC => rg.section.on = !rg.section.on,
            KeyCode::KeyV => {
                let n = rg.g.player.hull_model().inverse().transform_vector3(-rg.forward());
                rg.section.set_normal_hull(n);
                rg.section.offset = 0.0;
                rg.section.on = true;
            }
            KeyCode::KeyP => {
                rg.print_tuning();
                rg.status = "TISSUE KNOBS PRINTED (TERMINAL) AND WRITTEN TO TARGET/FEA_RANGE_TUNING.JSON".into();
            }
            KeyCode::KeyM => {
                rg.muted = true;
                rg.g.mute_audio();
                rg.status = "AUDIO OFF FOR THIS SESSION".into();
            }
            KeyCode::KeyH => rg.help = !rg.help,
            KeyCode::BracketLeft => rg.fov_deg = (rg.fov_deg - 5.0).max(5.0),
            KeyCode::BracketRight => rg.fov_deg = (rg.fov_deg + 5.0).min(110.0),
            KeyCode::F12 => {
                let path = crate::capture_target(&format!("fea_range_{}.png", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())));
                let (w, h) = (rg.r.iw * 3, rg.r.ih * 3);
                if rg.mode == Mode::Live {
                    rg.pose_view();
                }
                rg.draw_ui(self.input.cursor);
                rg.status = match rg.r.render_to_png_hud(&rg.device, &rg.queue, &path, w, h) {
                    Ok(()) => format!("SCREENSHOT {path}"),
                    Err(e) => format!("SCREENSHOT FAILED {e:#}"),
                };
            }
            _ => {}
        }
    }

    fn frame(&mut self) {
        let now = std::time::Instant::now();
        let dt = self.last.map_or(1.0 / 60.0, |t| (now - t).as_secs_f32()).min(0.1);
        self.last = Some(now);
        let input = &mut self.input;
        let Some(st) = self.state.as_mut() else { return };
        let rg = &mut st.range;
        // Fly camera.
        if input.look {
            rg.yaw += input.mouse.x * 0.0025;
            rg.pitch = (rg.pitch - input.mouse.y * 0.0025).clamp(-1.55, 1.55);
        }
        input.mouse = Vec2::ZERO;
        let f = rg.forward();
        let right = f.cross(Vec3::Y).normalize_or_zero();
        let mut v = Vec3::ZERO;
        let held = |k: KeyCode| input.keys.contains(&k);
        if !input.ctrl || input.look {
            if held(KeyCode::KeyW) { v += f; }
            if held(KeyCode::KeyS) { v -= f; }
            if held(KeyCode::KeyD) { v += right; }
            if held(KeyCode::KeyA) { v -= right; }
            if held(KeyCode::KeyE) { v += Vec3::Y; }
            if held(KeyCode::KeyQ) { v -= Vec3::Y; }
        }
        let boost = if input.shift { 4.0 } else if input.ctrl { 0.25 } else { 1.0 };
        rg.eye += v.normalize_or_zero() * rg.speed * boost * dt;
        rg.tick(dt);
        let frame = match st.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                st.surface.configure(&rg.device, &st.config);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        rg.render(&view, st.config.format, st.config.width, st.config.height, input.cursor);
        frame.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Panzerrechnersystem — FEA range");
        let attrs = if std::env::var("KESSEL_FEA_RANGE_WINDOWED").as_deref() == Ok("1") {
            attrs.with_inner_size(winit::dpi::PhysicalSize::new(1920, 1080))
        } else {
            attrs.with_fullscreen(Some(Fullscreen::Borderless(None)))
        };
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let built = (|| -> anyhow::Result<WindowState> {
            let size = window.inner_size();
            let instance = crate::create_instance();
            let surface = instance.create_surface(window.clone())?;
            let adapter = crate::request_adapter(&instance, Some(&surface));
            println!("[fea-range] adapter: {}", adapter.get_info().name);
            let (device, queue) = crate::request_device(&adapter);
            let caps = surface.get_capabilities(&adapter);
            let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT, format,
                width: size.width.max(1), height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync, alpha_mode: caps.alpha_modes[0],
                view_formats: vec![], desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            let range = Range::new(device, queue)?;
            Ok(WindowState { window: window.clone(), surface, config, range })
        })();
        match built {
            Ok(st) => {
                st.window.request_redraw();
                self.state = Some(st);
            }
            Err(e) => {
                eprintln!("[fea-range] start failed: {e:#}");
                self.error = Some(format!("{e:#}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.state.is_none() {
            return;
        }
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.active = Some(std::time::Instant::now());
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(st) = self.state.as_mut() {
                    if size.width > 0 && size.height > 0 {
                        st.config.width = size.width;
                        st.config.height = size.height;
                        st.surface.configure(&st.range.device, &st.config);
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.input.shift = m.state().shift_key();
                self.input.ctrl = m.state().control_key();
                self.input.alt = m.state().alt_key();
            }
            WindowEvent::Focused(false) => {
                self.input.keys.clear();
                self.input.look = false;
                self.input.drag = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let Some(st) = self.state.as_mut() else { return };
                let c = [(position.x as f32 / st.config.width.max(1) as f32).clamp(0.0, 1.0),
                    (position.y as f32 / st.config.height.max(1) as f32).clamp(0.0, 1.0)];
                if let Some((drag, last)) = self.input.drag {
                    let (dx, dy) = (c[0] - last[0], c[1] - last[1]);
                    let rg = &mut st.range;
                    match drag {
                        Drag::Turn => {
                            rg.shot.from_az = ((rg.shot.from_az - dx * 180.0 + 180.0).rem_euclid(360.0)) - 180.0;
                            rg.shot.from_el = (rg.shot.from_el - dy * 90.0).clamp(-60.0, 80.0);
                        }
                        Drag::Section => {
                            rg.section.offset = (rg.section.offset - dy * 4.0).clamp(-6.0, 6.0);
                            rg.section.yaw = ((rg.section.yaw + dx * 180.0 + 180.0).rem_euclid(360.0)) - 180.0;
                        }
                        Drag::Track => {
                            if let Some(i) = rg.track_hit([c[0], TRACK_Y + TRACK_H * 0.5]) {
                                rg.mode = Mode::Scrub(i);
                                rg.playing = false;
                            }
                        }
                    }
                    self.input.drag = Some((drag, c));
                }
                self.input.cursor = c;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if dy == 0.0 {
                    return;
                }
                let (c, fine, coarse) = (self.input.cursor, self.input.ctrl, self.input.shift);
                let Some(st) = self.state.as_mut() else { return };
                let rg = &mut st.range;
                if c[0] >= PANEL_X && c[0] <= PANEL_X + PANEL_W {
                    if let Some((_, _, knob)) = row_layout().into_iter().find(|(y, _, _)| (c[1] - y).abs() <= ROW_H * 0.5) {
                        rg.adjust(knob, dy.signum(), fine, coarse);
                    }
                } else if rg.track_hit(c).is_some() {
                    let n = rg.frames.len();
                    let at = match rg.mode { Mode::Scrub(i) => i as i64, _ => n as i64 - 1 };
                    rg.mode = Mode::Scrub((at - dy.signum() as i64 * if coarse { 10 } else { 1 }).clamp(0, n as i64 - 1) as usize);
                    rg.playing = false;
                } else {
                    rg.speed = (rg.speed * 1.2f32.powf(dy)).clamp(0.05, 200.0);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    MouseButton::Right => {
                        self.input.look = pressed;
                        if let Some(st) = self.state.as_ref() {
                            if pressed {
                                let _ = st.window.set_cursor_grab(CursorGrabMode::Locked)
                                    .or_else(|_| st.window.set_cursor_grab(CursorGrabMode::Confined));
                                st.window.set_cursor_visible(false);
                            } else {
                                let _ = st.window.set_cursor_grab(CursorGrabMode::None);
                                st.window.set_cursor_visible(true);
                            }
                        }
                    }
                    MouseButton::Left if pressed => {
                        let (c, shift, ctrl, alt) = (self.input.cursor, self.input.shift, self.input.ctrl, self.input.alt);
                        let Some(st) = self.state.as_mut() else { return };
                        let rg = &mut st.range;
                        if let Some(i) = rg.track_hit(c) {
                            if !matches!(rg.mode, Mode::Recording { .. }) {
                                rg.mode = Mode::Scrub(i);
                                rg.playing = false;
                                self.input.drag = Some((Drag::Track, c));
                            }
                        } else if rg.click_ui(c, ctrl, shift) {
                        } else if alt {
                            self.input.drag = Some((Drag::Section, c));
                            rg.section.on = true;
                        } else if shift {
                            self.input.drag = Some((Drag::Turn, c));
                        } else if rg.mode == Mode::Live {
                            rg.pick(c, ctrl);
                        } else {
                            rg.status = "AIM IN LIVE (TAB OR THE LIVE BUTTON)".into();
                        }
                    }
                    MouseButton::Left => self.input.drag = None,
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                if event.state == ElementState::Pressed {
                    let repeat = !self.input.keys.insert(code);
                    if !repeat || matches!(code, KeyCode::ArrowLeft | KeyCode::ArrowRight) {
                        self.key(code, event_loop);
                    }
                } else {
                    self.input.keys.remove(&code);
                }
            }
            WindowEvent::RedrawRequested => {
                self.frame();
                if self.busy() {
                    event_loop.set_control_flow(ControlFlow::Poll);
                    if let Some(st) = &self.state {
                        st.window.request_redraw();
                    }
                } else {
                    // Nothing changing: ten frames a second (the transport still converges).
                    event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now()
                        + std::time::Duration::from_millis(100)));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(st) = &self.state {
            if self.busy() || self.last.is_none_or(|t| t.elapsed().as_millis() >= 100) {
                st.window.request_redraw();
            }
        }
    }

    fn device_event(&mut self, _l: &ActiveEventLoop, _id: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        if let winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.input.look {
                self.input.mouse += Vec2::new(dx as f32, dy as f32);
                self.active = Some(std::time::Instant::now());
            }
        }
    }
}

fn prepare_environment() {
    // The shipping renderer and the owner's review look, unless chosen otherwise.
    if std::env::var_os("KESSEL_RENDERER").is_none() {
        std::env::set_var("KESSEL_RENDERER", "raymarch");
    }
    if std::env::var_os("KESSEL_LOOK").is_none() {
        std::env::set_var("KESSEL_LOOK", "deltap-real");
    }
    graphics::set_active_preset(graphics::GraphicsPreset::High);
}

/// `fea-range` (window) or `fea-range --shots PREFIX` (headless stills and the agreement check).
pub fn run(shots: Option<&str>) -> anyhow::Result<()> {
    prepare_environment();
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "fea-range requires KESSEL_RENDERER=raymarch");
    if let Some(prefix) = shots {
        return headless_shots(prefix);
    }
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { state: None, input: Input::default(), last: None, active: None, escape: None, error: None };
    event_loop.run_app(&mut app)?;
    if let Some(e) = app.error {
        anyhow::bail!(e);
    }
    Ok(())
}

/// Headless verification: the interface live, with the section, a short run and a scrubbed
/// frame, and a live re-simulation at that frame compared with the recording's state there.
fn headless_shots(prefix: &str) -> anyhow::Result<()> {
    let prefix = super::game_fea_section::shared_capture_prefix(prefix);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let mut rg = Range::new(device, queue)?;
    let (w, h) = (1920, 1080);
    let mut checks = Vec::new();
    let still = |rg: &mut Range, name: &str, converge: bool| -> anyhow::Result<String> {
        if rg.mode == Mode::Live {
            rg.pose_view();
            if converge {
                rg.r.reset_history();
                crate::converge_headless_screenshot(&mut rg.r, &rg.device, &rg.queue, 16)?;
            }
        }
        rg.draw_ui([0.1, 0.1]);
        let path = format!("{prefix}_{name}.png");
        rg.r.render_to_png_hud(&rg.device, &rg.queue, &path, w, h)?;
        Ok(path)
    };
    // 1. Live, the default shot (57 mm at the loader through the right turret side), help shown.
    rg.help = true;
    let p1 = still(&mut rg, "live", true)?;
    rg.help = false;
    // 2. The section: the top cut 1.9 m above the ground, the eye above the left side, the
    // exposure held at six times the eye's own on the open view (the closed interior is dark
    // beside the daylit snow; a camera's choice, as `fea-film`'s engine section).
    rg.section = Section { on: true, yaw: 0.0, pitch: 90.0, offset: 0.5 };
    let hull = rg.g.player.hull_model();
    rg.look_at(hull.transform_point3(Vec3::new(3.0, 6.2, 0.4)), hull.transform_point3(Vec3::new(-0.25, 1.0, -0.2)));
    rg.fov_deg = 46.0;
    rg.pose_view();
    let metered = crate::converge_headless_screenshot(&mut rg.r, &rg.device, &rg.queue, 8)?.measured_exposure;
    rg.sky.exposure = Some(metered.unwrap_or(1000.0) * 6.0);
    let p2 = still(&mut rg, "section", true)?;
    // 3. A short run: 0.1 s in 1/480 s steps, then real time to 0.5 s; the state's digest at a
    // middle frame.
    rg.scene.slow_s = 0.1;
    rg.scene.run_s = 0.5;
    rg.run();
    let probe = 36usize;
    let mut recorded_digest = None;
    let started = std::time::Instant::now();
    while matches!(rg.mode, Mode::Recording { .. }) {
        rg.tick(1.0 / 60.0);
        if rg.frames.len() == probe + 1 && recorded_digest.is_none() {
            recorded_digest = Some(rg.digest());
        }
    }
    let run_s = started.elapsed().as_secs_f32();
    println!("[fea-range] run: {} frames in {run_s:.1} s; {}", rg.frames.len(), rg.last_chain);
    checks.push(serde_json::json!({"check": "run recorded", "pass": rg.frames.len() > probe, "frames": rg.frames.len(), "wall_s": run_s}));
    rg.mode = Mode::Scrub(probe.min(rg.frames.len().saturating_sub(1)));
    let p3 = still(&mut rg, "timeline", false)?;
    // 4. Live at that frame: the run repeated step for step, and its state against the recording's.
    rg.live_at(probe);
    let live_digest = rg.digest();
    let agree = recorded_digest.as_deref() == Some(live_digest.as_str());
    println!("[fea-range] {} live re-simulation agrees with the recording at frame {probe}", if agree { "PASS" } else { "FAIL" });
    if !agree {
        println!("[fea-range] recorded: {}\n[fea-range] live:     {live_digest}", recorded_digest.as_deref().unwrap_or("-"));
    }
    checks.push(serde_json::json!({"check": "live here agrees with the recording", "pass": agree, "frame": probe,
        "recorded": recorded_digest, "live": live_digest}));
    // The same re-simulation again, unrendered both times: whether the run itself repeats.
    rg.live_at(probe);
    let again = rg.digest();
    let repeat = again == live_digest;
    println!("[fea-range] {} a second re-simulation agrees with the first", if repeat { "PASS" } else { "FAIL" });
    checks.push(serde_json::json!({"check": "live here repeats itself", "pass": repeat, "again": again}));
    let p4 = still(&mut rg, "live_here", true)?;
    // 5. The panel's tissue knobs changed and printed (review override set, then restored).
    rg.adjust(Knob::Tear(0), 1.0, false, false);
    let printed = rg.print_tuning();
    let set = crew_tissue::tissue_tuning() != crew_tissue::TissueTuning::AUTHORED;
    rg.adjust(Knob::TissueAuthored, 1.0, false, false);
    let restored = crew_tissue::tissue_tuning() == crew_tissue::TissueTuning::AUTHORED;
    checks.push(serde_json::json!({"check": "tissue override set and restored", "pass": set && restored, "printed": printed}));
    let receipt = serde_json::json!({"stills": [p1, p2, p3, p4], "checks": checks});
    std::fs::write(format!("{prefix}_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[fea-range] receipt {prefix}_receipt.json");
    Ok(())
}
