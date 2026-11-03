//! `ai-film PREFIX ACT COUNT SEED MAX_SECS [T34_WEIGHTS] [TIGER_WEIGHTS]`: one AI-against-AI
//! battle, the neural Tiger pilot and its embodied auto-crew (`showreel::TigerPilot`, the
//! singleplayer crew aboard) against COUNT neural T-34s, in one continuous ordinary simulation at
//! 120 Hz, filmed by an event-following camera director:
//!
//! - a T-34 killed through its ammunition: a fixed wide frame sized for its turret's whole throw
//!   from the solved launch velocity (drag ignored), held until the turret lies still;
//! - the Tiger penetrated: an interior observer lens aimed at the new hole, then a close outside
//!   shot of the struck plate; a strike that does not penetrate: the struck plate from outside;
//! - otherwise a fixed rotation of wide, chase, interior and enemy shots.
//!
//! If the Tiger dies, its death's shots follow as the app plays them (`main::render_death`): the
//! cook-off death (black, outside, the dead) or the penetration death (black, the impact with the
//! armour cut away, the interior, the aerial shot).
//!
//! Staging, disclosed in the frame and the receipt: the battle opens as the crew-battle witness
//! opens it (`crew_battle_capture`): the T-34s on the showreel's seeded ring round the Tiger
//! (`stage_showreel_t34_gauntlet_at`), every T-34 told where the Tiger is each tick
//! (`debug_engage_all`). Nothing after that is scripted: every shot, hit, penetration, kill and
//! cook-off is the ordinary simulation's. Frames at the renderer's grid, written x5 by nearest
//! neighbour (4:4:4, CRF 8, 30 fps); the ordinary gameplay audio recorded offline on the
//! simulation clock (its listener is the Tiger's crewman under every shot). Camera cuts restart the
//! renderer's history and re-render a few frames before the kept one (more samples, no filter).
//! `KESSEL_AI_FILM_SCAN=1` runs the same battle without rendering and prints its events.
use super::*;
use crate::{acoustics, graphics, lan::LanRole};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Child, Command, Stdio};

const SIM_HZ: u32 = 120;
const FPS: u32 = 30;
const STEP: f32 = 1.0 / SIM_HZ as f32;
const TICKS_PER_FRAME: u32 = SIM_HZ / FPS;
const FRAME_DT: f32 = 1.0 / FPS as f32;
/// Stereo samples in one film frame.
const FRAME_SAMPLES: usize = 2 * (acoustics::SAMPLE_RATE / FPS) as usize;
/// Renders discarded after a cut (the transport's history restarts on the new view).
const CUT_SETTLE: u32 = 6;
/// Renders discarded before each death-replay frame (the replay cameras move every frame).
const REPLAY_SETTLE: u32 = 3;

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn sample_count(tick: u64) -> usize {
    (((tick + 1) * u64::from(acoustics::SAMPLE_RATE) / u64::from(SIM_HZ))
        - (tick * u64::from(acoustics::SAMPLE_RATE) / u64::from(SIM_HZ))) as usize
}

fn parse_seed(raw: &str) -> anyhow::Result<u32> {
    Ok(match raw.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16)?,
        None => raw.parse()?,
    })
}

struct Encoder {
    child: Child,
    picture: String,
    frames: u64,
    sound: Vec<f32>,
}

impl Encoder {
    fn start(picture: &str, w: u32, h: u32) -> anyhow::Result<Self> {
        let k = std::env::var("KESSEL_AI_FILM_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(5u32).clamp(1, 8);
        let preset = std::env::var("KESSEL_AI_FILM_PRESET").unwrap_or_else(|_| "fast".into());
        let child = Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pixel_format", "rgba",
                "-video_size", &format!("{w}x{h}"), "-framerate", &FPS.to_string(), "-i", "pipe:0", "-an",
                "-vf", &format!("scale=iw*{k}:ih*{k}:flags=neighbor"), "-c:v", "libx264", "-preset", &preset,
                "-crf", "8", "-pix_fmt", "yuv444p", "-movflags", "+faststart", picture])
            .stdin(Stdio::piped())
            .spawn()
            .context("starting ffmpeg")?;
        Ok(Self { child, picture: picture.into(), frames: 0, sound: Vec::new() })
    }

    fn push(&mut self, rgba: &[u8], sound: Option<&[f32]>) -> anyhow::Result<()> {
        self.child.stdin.as_mut().context("encoder input")?.write_all(rgba)?;
        self.frames += 1;
        if let Some(sound) = sound {
            self.sound.extend_from_slice(sound);
        }
        // Picture and sound the same length: pad with silence or trim to this frame's end.
        self.sound.resize(self.frames as usize * FRAME_SAMPLES, 0.0);
        Ok(())
    }

    fn finish(mut self, path: &str) -> anyhow::Result<f32> {
        drop(self.child.stdin.take());
        ensure!(self.child.wait()?.success(), "video encoder failed");
        acoustics::attenuate_over_peak(&mut self.sound, 0.94);
        let wav = format!("{}.wav", path.trim_end_matches(".mp4"));
        acoustics::write_wav_stereo(&wav, &self.sound, acoustics::SAMPLE_RATE)?;
        let status = Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", &self.picture, "-i", &wav,
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest",
                "-movflags", "+faststart", path])
            .status()?;
        ensure!(status.success(), "sound mux failed");
        let _ = std::fs::remove_file(&self.picture);
        let _ = std::fs::remove_file(&wav);
        Ok(self.frames as f32 / FPS as f32)
    }
}

/// What the battle did during one tick, read from the ordinary state before and after it.
#[derive(Clone, Copy, Debug)]
enum Event {
    Kill { enemy: usize, catastrophic: bool },
    Penetrated { scar: Option<crate::armor_damage::ArmorScar> },
    Struck { scar: Option<crate::armor_damage::ArmorScar> },
    CrewLost { living: usize },
    Fire,
    FirstRound,
    TigerDestroyed { cookoff: bool },
}

#[derive(Clone, Copy, Debug)]
enum Shot {
    Wide,
    Chase,
    Interior,
    EnemyChase,
    PenInside(crate::armor_damage::ArmorScar, u32),
    PlateOutside(crate::armor_damage::ArmorScar, u32, bool),
    KillWide(usize),
    KillNear(usize),
}

impl Shot {
    fn label(self) -> &'static str {
        match self {
            Shot::Wide => "wide",
            Shot::Chase => "chase",
            Shot::Interior => "interior",
            Shot::EnemyChase => "enemy-chase",
            Shot::PenInside(..) => "penetration-inside",
            Shot::PlateOutside(..) => "struck-plate-outside",
            Shot::KillWide(_) => "catastrophic-kill-wide",
            Shot::KillNear(_) => "kill-near",
        }
    }

    /// The renderer's per-view history and exposure slot (1-4; 0 stays the game's own view).
    fn view_slot(self) -> usize {
        match self {
            Shot::Wide | Shot::Chase | Shot::EnemyChase | Shot::KillNear(_) => 1,
            Shot::KillWide(_) => 2,
            Shot::PlateOutside(..) => 3,
            Shot::Interior | Shot::PenInside(..) => 4,
        }
    }
}

const ROTATION: [(Shot, f32); 6] = [
    (Shot::Wide, 6.0),
    (Shot::Interior, 4.0),
    (Shot::Chase, 5.0),
    (Shot::EnemyChase, 4.0),
    (Shot::Wide, 5.0),
    (Shot::Interior, 4.0),
];

struct Director {
    shot: Shot,
    age: f32,
    hold: f32,
    rotation: usize,
    queue: Vec<(Shot, f32)>,
    cut: bool,
    kill_frame: Option<(Vec3, Vec3, f32)>,
    kill_settled_at: Option<f32>,
    smooth: Option<(Vec3, Vec3)>,
    last_plate: f32,
    pens: u32,
    hits: u32,
}

impl Director {
    fn new() -> Self {
        Self { shot: ROTATION[0].0, age: 0.0, hold: ROTATION[0].1, rotation: 0, queue: Vec::new(), cut: true,
            kill_frame: None, kill_settled_at: None, smooth: None, last_plate: -100.0, pens: 0, hits: 0 }
    }

    fn set(&mut self, shot: Shot, hold: f32) {
        self.shot = shot;
        self.age = 0.0;
        self.hold = hold;
        self.cut = true;
        self.smooth = None;
        self.kill_frame = None;
        self.kill_settled_at = None;
    }

    fn routine(&self) -> bool {
        matches!(self.shot, Shot::Wide | Shot::Chase | Shot::Interior | Shot::EnemyChase)
    }

    fn in_kill_wide(&self) -> bool {
        matches!(self.shot, Shot::KillWide(_))
    }

    fn on_event(&mut self, event: Event, now: f32) {
        match event {
            Event::Kill { enemy, catastrophic: true } => {
                if self.in_kill_wide() && self.age < 3.0 {
                    // The first throw is still in its fireball: this one follows it.
                    self.queue.insert(0, (Shot::KillWide(enemy), 16.0));
                    self.queue.insert(1, (Shot::KillNear(enemy), 3.5));
                } else {
                    // A penetration's shots cut short by the kill are shown after it.
                    let mut later: Vec<(Shot, f32)> = match self.shot {
                        Shot::PenInside(scar, n) => vec![(Shot::PenInside(scar, n), 3.0), (Shot::PlateOutside(scar, n, true), 2.5)],
                        Shot::PlateOutside(scar, n, pen) => vec![(Shot::PlateOutside(scar, n, pen), 2.5)],
                        _ => Vec::new(),
                    };
                    later.extend(self.queue.drain(..).filter(|(s, _)| matches!(s, Shot::PenInside(..) | Shot::PlateOutside(..))));
                    self.set(Shot::KillWide(enemy), 16.0);
                    self.queue = vec![(Shot::KillNear(enemy), 3.5)];
                    self.queue.extend(later);
                }
            }
            Event::Kill { enemy, catastrophic: false } => {
                if !self.in_kill_wide() {
                    self.set(Shot::KillNear(enemy), 4.0);
                    self.queue.clear();
                }
            }
            Event::Penetrated { scar } => {
                self.pens += 1;
                if let Some(scar) = scar {
                    if self.in_kill_wide() {
                        // Not over a turret's throw: the hole and its chips after it.
                        self.queue.push((Shot::PenInside(scar, self.pens), 3.0));
                        self.queue.push((Shot::PlateOutside(scar, self.pens, true), 2.5));
                    } else {
                        self.set(Shot::PenInside(scar, self.pens), 3.2);
                        self.queue = vec![(Shot::PlateOutside(scar, self.pens, true), 2.8)];
                    }
                    self.last_plate = now;
                }
            }
            Event::Struck { scar } => {
                self.hits += 1;
                if let Some(scar) = scar.filter(|_| self.routine() && now - self.last_plate > 7.0) {
                    self.set(Shot::PlateOutside(scar, self.hits, false), 2.4);
                    self.queue.clear();
                    self.last_plate = now;
                }
            }
            Event::CrewLost { .. } => {
                if self.routine() && !matches!(self.shot, Shot::Interior) {
                    self.set(Shot::Interior, 4.0);
                }
            }
            Event::Fire | Event::FirstRound | Event::TigerDestroyed { .. } => {}
        }
    }

    fn advance(&mut self, g: &Game) {
        self.age += FRAME_DT;
        if let Shot::KillWide(enemy) = self.shot {
            let settled = g.debug_thrown_turrets().iter().any(|&(i, settled, _, _)| i == enemy && settled);
            if settled && self.kill_settled_at.is_none() {
                self.kill_settled_at = Some(self.age);
            }
            if self.kill_settled_at.is_some_and(|at| self.age - at >= 2.5) {
                self.hold = self.hold.min(self.age);
            }
        }
        // A shot of a T-34 that has since died, or the enemy chase with none left, ends early.
        if matches!(self.shot, Shot::EnemyChase) && !g.enemies.iter().any(|e| e.alive) {
            self.hold = self.hold.min(self.age);
        }
        if self.age >= self.hold {
            if !self.queue.is_empty() {
                let (shot, hold) = self.queue.remove(0);
                self.set(shot, hold);
            } else {
                self.rotation = (self.rotation + 1) % ROTATION.len();
                let (shot, hold) = ROTATION[self.rotation];
                self.set(shot, hold);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SavedView {
    camera: crate::camera::Camera,
    reticle: bool,
    optic_mask: f32,
    optic_blast: f32,
    crew_ko: f32,
    light: f32,
    haze: f32,
    rt_mix: f32,
    shelter: f32,
    precipitation: f32,
}

impl SavedView {
    fn save(r: &Renderer) -> Self {
        Self { camera: r.camera, reticle: r.reticle, optic_mask: r.optic_mask, optic_blast: r.optic_blast,
            crew_ko: r.crew_ko, light: r.interior_light, haze: r.interior_haze, rt_mix: r.rt_mix,
            shelter: r.weather_shelter, precipitation: r.precipitation }
    }

    fn restore(self, r: &mut Renderer) {
        r.camera = self.camera;
        r.reticle = self.reticle;
        r.optic_mask = self.optic_mask;
        r.optic_blast = self.optic_blast;
        r.crew_ko = self.crew_ko;
        r.interior_light = self.light;
        r.interior_haze = self.haze;
        r.rt_mix = self.rt_mix;
        r.weather_shelter = self.shelter;
        r.precipitation = self.precipitation;
        r.set_interior_atmosphere_enabled(self.light > 0.5);
        r.set_active_view(0);
    }
}

/// The crew-battle witness's detached interior lens (`crew_battle_capture::interior_witness`),
/// optionally turned onto `aim`: the turret's rear corner on the loader's side, or the hull's
/// front compartment.
fn interior_lens(g: &Game, r: &mut Renderer, hull: bool, aim: Option<Vec3>, slot: usize) {
    let (parent, eye, target, fov) = if hull {
        (g.player.hull_model(), Vec3::new(0.0, 1.75, 0.45), Vec3::new(0.0, 1.02, 1.78), 88.0_f32)
    } else {
        (g.player.turret_model(), Vec3::new(-0.45, 2.34, -0.84), Vec3::new(0.0, 1.45, 0.18), 80.0_f32)
    };
    r.camera.pos = parent.transform_point3(eye);
    let default_target = parent.transform_point3(target);
    let look = match aim {
        // Toward the hole, keeping some of the lens's own composition (the crew and the breech).
        Some(point) => {
            let to_hole = (point - r.camera.pos).normalize_or_zero();
            let to_default = (default_target - r.camera.pos).normalize_or_zero();
            r.camera.pos + (to_hole * 0.75 + to_default * 0.25).normalize_or_zero()
        }
        None => default_target,
    };
    r.camera.set_forward(look - r.camera.pos);
    r.camera.fov_y = fov.to_radians();
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.crew_ko = 0.0;
    r.weather_shelter = 1.0;
    r.set_active_view(slot);
    g.configure_crew_render_view(LanRole::Loader, false, r);
    g.drive_singleplayer_crew_rigs_for_view(None, r);
    g.apply_cinematic_camera_impulses(r);
    let mut bounded = parent.inverse().transform_point3(r.camera.pos);
    bounded.y = bounded.y.min(eye.y);
    r.camera.pos = parent.transform_point3(bounded);
}

/// An observer camera outside the vehicles (the crew-battle overhead witness's settings).
fn outside_lens(g: &Game, r: &mut Renderer, eye: Vec3, target: Vec3, fov_deg: f32, slot: usize) {
    r.camera.pos = eye;
    r.camera.set_forward(target - eye);
    r.camera.fov_y = fov_deg.to_radians();
    r.reticle = false;
    r.optic_mask = 0.0;
    r.optic_blast = 0.0;
    r.crew_ko = 0.0;
    r.interior_light = 0.0;
    r.interior_haze = 0.0;
    r.rt_mix = 1.0;
    r.weather_shelter = 0.0;
    r.set_interior_atmosphere_enabled(false);
    r.set_active_view(slot);
    g.drive_singleplayer_crew_rigs_for_view(None, r);
}

fn nearest_live(g: &Game, active: &[usize]) -> Option<usize> {
    let tiger = g.player.center();
    active.iter().copied().filter(|&i| g.enemies[i].alive)
        .min_by(|&a, &b| g.enemies[a].center().distance_squared(tiger).total_cmp(&g.enemies[b].center().distance_squared(tiger)))
}

fn lift(g: &Game, p: Vec3, clearance: f32) -> Vec3 {
    Vec3::new(p.x, p.y.max(g.hf.height_at(p.x, p.z) + clearance), p.z)
}

/// The struck plate's centre and outward normal (world), from the scar the strike recorded.
fn scar_world(g: &Game, scar: crate::armor_damage::ArmorScar) -> (Vec3, Vec3) {
    let (parent, _, pos, _) = g.armor_scar_frame(scar);
    (parent.transform_point3(pos), parent.transform_vector3(scar.local_normal()).normalize_or_zero())
}

/// The fixed frame for a T-34's whole-load throw: from the side of the turret's flight, far enough
/// that the solved apex (v²/2g, drag ignored) and the landing (launch speed × flight time) fit.
fn kill_wide_frame(g: &Game, enemy: usize, aspect: f32) -> (Vec3, Vec3, f32) {
    let w = &g.wrecks[enemy];
    let ring = w.t_initial.transform_point3(Vec3::ZERO);
    let tiger = g.player.center();
    let fallback_speed = game_cookoff::enemy_cookoff_throw(g.enemies[enemy].kind).map_or(30.0, |(v, _, _)| v);
    let velocity = w.t_body.as_ref().map_or(Vec3::Y * fallback_speed, |b| b.velocity);
    let vy = velocity.y.max(8.0);
    let flat = Vec3::new(velocity.x, 0.0, velocity.z);
    let flight = 2.0 * vy / 9.81;
    let apex = vy * vy / 19.62;
    let land = ring + flat * flight;
    let mid = (ring + land) * 0.5;
    let line = Vec3::new(ring.x - tiger.x, 0.0, ring.z - tiger.z).normalize_or(Vec3::Z);
    let along = if flat.length() > 1.0 { flat.normalize() } else { line };
    let mut perp = Vec3::new(-along.z, 0.0, along.x);
    // Stand on the Tiger's side of the throw, so the fight's survivor is not behind the wreck.
    if perp.dot(Vec3::new(tiger.x - mid.x, 0.0, tiger.z - mid.z)) < 0.0 {
        perp = -perp;
    }
    let fov = 46.0f32;
    let tan_v = (0.5 * fov).to_radians().tan();
    let tan_h = tan_v * aspect.max(0.5);
    // Headroom for the whole load's cloud, which rises above the throw.
    let top = apex * 1.35 + 30.0;
    let bottom = -8.0;
    let half_v = 0.5 * (top - bottom);
    let distance = (half_v / tan_v).max((0.5 * (land - ring).length() + 35.0) / tan_h).max(110.0);
    let target = Vec3::new(mid.x, ring.y + bottom + half_v, mid.z);
    // The first stand (side of the throw first, then turned round the wreck in 30-degree steps,
    // at eye height and then from rooftop height) whose sight lines to the wreck, the throw's
    // middle height and the landing are not cut by hard cover, terrain or trees (`los_clear`).
    let turned = |v: Vec3, a: f32| Vec3::new(v.x * a.cos() - v.z * a.sin(), 0.0, v.x * a.sin() + v.z * a.cos());
    let mut stands = Vec::new();
    for height in [6.0f32, 26.0] {
        for k in [0.0f32, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0] {
            stands.push(lift(g, mid + turned(perp, k * std::f32::consts::FRAC_PI_6) * distance + Vec3::Y * height, height));
        }
    }
    let sees = |eye: Vec3| g.los_clear(eye, ring + Vec3::Y * 2.5) && g.los_clear(eye, mid + Vec3::Y * (0.6 * apex))
        && g.los_clear(eye, land + Vec3::Y * 2.0);
    let eye = stands.iter().copied().find(|&e| sees(e)).unwrap_or(stands[0]);
    (eye, target, fov)
}

fn aim_shot(dir: &mut Director, g: &Game, r: &mut Renderer, active: &[usize]) {
    let tiger = g.player.center();
    let actor = nearest_live(g, active).map(|i| g.enemies[i].center());
    let toward = actor.unwrap_or(tiger + g.player.forward() * 40.0);
    let line = Vec3::new(toward.x - tiger.x, 0.0, toward.z - tiger.z).normalize_or(Vec3::Z);
    let side = Vec3::new(line.z, 0.0, -line.x);
    let follow = |dir: &mut Director, eye: Vec3, target: Vec3| -> (Vec3, Vec3) {
        let (e, t) = match dir.smooth {
            Some((e, t)) => {
                let k = 1.0 - (-FRAME_DT * 2.0).exp();
                (e.lerp(eye, k), t.lerp(target, k))
            }
            None => (eye, target),
        };
        dir.smooth = Some((e, t));
        (e, t)
    };
    let shot = dir.shot;
    match shot {
        Shot::Wide => {
            let mid = (tiger + toward) * 0.5;
            let sep = tiger.distance(toward);
            let d = (sep * 0.75 + 30.0).clamp(45.0, 150.0);
            let eye = lift(g, mid + side * d * 0.85 - line * d * 0.35 + Vec3::Y * (d * 0.32 + 6.0), 3.0);
            let (e, t) = follow(dir, eye, mid + Vec3::Y * 1.5);
            outside_lens(g, r, e, t, 50.0, 1);
        }
        Shot::Chase => {
            // Over the Tiger's shoulder toward the nearest T-34, the Tiger kept in the frame.
            let eye = lift(g, tiger - line * 13.0 + side * 5.0 + Vec3::Y * 4.0, 2.0);
            let target = tiger + line * 18.0 + Vec3::Y * 1.0;
            let (e, t) = follow(dir, eye, target);
            outside_lens(g, r, e, t, 50.0, 1);
        }
        Shot::EnemyChase => match nearest_live(g, active) {
            Some(i) => {
                let e = &g.enemies[i];
                let c = e.center();
                let eye = lift(g, c - e.forward() * 11.0 - e.right() * 4.0 + Vec3::Y * 3.5, 2.0);
                let target = c + (tiger - c).normalize_or_zero() * 20.0 + Vec3::Y * 0.8;
                let (ee, t) = follow(dir, eye, target);
                outside_lens(g, r, ee, t, 50.0, 1);
            }
            None => {
                let eye = lift(g, tiger + side * 30.0 + Vec3::Y * 9.0, 3.0);
                outside_lens(g, r, eye, tiger, 50.0, 1);
            }
        },
        Shot::Interior => interior_lens(g, r, false, None, 4),
        Shot::PenInside(scar, _) => {
            let (point, _) = scar_world(g, scar);
            let local = g.player.hull_model().inverse().transform_point3(point);
            // A hole in the hull's front compartment is watched from the hull's own lens.
            let hull = !scar.is_turret() && local.z > 1.0;
            interior_lens(g, r, hull, Some(point), 4);
        }
        Shot::PlateOutside(scar, _, _) => {
            let (point, normal) = scar_world(g, scar);
            let up = Vec3::Y;
            let tangent = normal.cross(up).normalize_or(Vec3::X);
            let eye = lift(g, point + normal * 4.2 + up * 1.0 + tangent * 0.8, 0.8);
            outside_lens(g, r, eye, point, 30.0, 3);
        }
        Shot::KillWide(enemy) => {
            // Fixed once the turret is in the air (its launch velocity solved); provisional before.
            let launched = g.wrecks[enemy].t_body.is_some();
            let frame = match dir.kill_frame {
                Some(frame) => frame,
                None => {
                    let frame = kill_wide_frame(g, enemy, r.camera.aspect);
                    if launched {
                        dir.kill_frame = Some(frame);
                        dir.cut = true;
                        // Held for the solved flight (2 vy / g) and 3 s more.
                        let vy = g.wrecks[enemy].t_body.as_ref().map_or(30.0, |b| b.velocity.y.max(8.0));
                        dir.hold = dir.hold.min((2.0 * vy / 9.81 + 3.0).clamp(8.0, 16.0));
                    }
                    frame
                }
            };
            outside_lens(g, r, frame.0, frame.1, frame.2, 2);
        }
        Shot::KillNear(enemy) => {
            let c = g.enemies[enemy].center();
            let turret = g.debug_enemy_turret_pop_position(enemy);
            let focus = turret.map_or(c, |t| c.lerp(t, 0.5));
            let spread = turret.map_or(0.0, |t| t.distance(c));
            let to_tiger = Vec3::new(tiger.x - c.x, 0.0, tiger.z - c.z).normalize_or(Vec3::Z);
            let across = Vec3::new(to_tiger.z, 0.0, -to_tiger.x);
            let d = (spread * 0.9 + 28.0).clamp(28.0, 90.0);
            let eye = lift(g, focus + across * d + to_tiger * d * 0.35 + Vec3::Y * (d * 0.22 + 4.0), 3.0);
            outside_lens(g, r, eye, focus + Vec3::Y * 1.5, 45.0, 1);
        }
    }
}

fn caption(dir: &Director, g: &Game, battle_s: f32, staged_note: bool) -> String {
    let clock = format!("T+{battle_s:05.1} S");
    let what = match dir.shot {
        Shot::Wide => "WIDE".to_string(),
        Shot::Chase => "BEHIND THE TIGER".to_string(),
        Shot::Interior => "INTERIOR OBSERVER LENS, TURRET REAR".to_string(),
        Shot::EnemyChase => "BEHIND THE NEAREST T-34".to_string(),
        Shot::PenInside(_, n) => format!("PENETRATION {n} OF THE TIGER: INTERIOR LENS TURNED ON THE HOLE{}",
            g.fea.receipts.last().map(|c| format!(" ({})", c.shell.to_uppercase())).unwrap_or_default()),
        Shot::PlateOutside(_, n, true) => format!("PENETRATION {n}: THE STRUCK PLATE FROM OUTSIDE"),
        Shot::PlateOutside(_, n, false) => format!("STRIKE {n} (NOT THROUGH): THE STRUCK PLATE FROM OUTSIDE"),
        Shot::KillWide(i) => format!("T-34 #{i} KILLED THROUGH ITS AMMUNITION: FIXED FRAME {:.0} M OFF",
            dir.kill_frame.map_or(0.0, |(eye, _, _)| eye.distance(g.enemies[i].center()))),
        Shot::KillNear(i) => format!("T-34 #{i} KNOCKED OUT"),
    };
    if staged_note {
        format!("{clock}  {what}  |  STAGED START: T-34S ON A SEEDED RING, TIGER A KNOWN CONTACT; NOTHING AFTER IS SCRIPTED")
    } else {
        format!("{clock}  {what}")
    }
}

struct Frame<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    w: u32,
    h: u32,
    banner: String,
}

impl Frame<'_> {
    fn finish(&self, r: &mut Renderer, caption: &str, settle: u32) -> anyhow::Result<Vec<u8>> {
        for _ in 0..settle {
            r.render_offscreen(self.device, self.queue);
        }
        r.ui_reset();
        r.fill_rect(0.015, 0.026, 0.0105 * self.banner.len() as f32 + 0.03, 0.048, [0.0, 0.0, 0.0, 0.62]);
        r.draw_text(&self.banner, 0.026, 0.050, 0.020, [0.88, 0.86, 0.80, 1.0], crate::ui::Align::Left);
        if !caption.is_empty() {
            r.fill_rect(0.015, 0.922, 0.0105 * caption.len() as f32 + 0.03, 0.052, [0.0, 0.0, 0.0, 0.62]);
            r.draw_text(caption, 0.026, 0.948, 0.020, [0.88, 0.86, 0.80, 1.0], crate::ui::Align::Left);
        }
        r.render_to_rgba_with_hud(self.device, self.queue, self.w, self.h)
    }
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let usage = "ai-film PREFIX ACT COUNT SEED MAX_SECS [T34_WEIGHTS] [TIGER_WEIGHTS]";
    let prefix = args.first().context(usage)?.clone();
    let act: u8 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(1);
    let count: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(3);
    let seed = args.get(3).map(|s| parse_seed(s)).transpose()?.unwrap_or(20_261_102);
    let max_secs: f32 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(90.0);
    let t34_weights = args.get(5).map(String::as_str).unwrap_or("assets/ai/weights_academy.bin");
    let tiger_weights = args.get(6).map(String::as_str).unwrap_or("assets/ai/weights_tiger.bin");
    let ring = env_f32("KESSEL_AI_FILM_RING", 105.0);
    let scan = std::env::var("KESSEL_AI_FILM_SCAN").as_deref() == Ok("1");
    ensure!((1..=6).contains(&count), "COUNT must be 1..=6");
    ensure!(scan || graphics::raymarch_backend_enabled(), "ai-film requires KESSEL_RENDERER=raymarch");
    let receipt_path = format!("{prefix}_receipt.json");
    let video_path = format!("{prefix}.mp4");
    ensure!(scan || (!std::path::Path::new(&receipt_path).exists() && !std::path::Path::new(&video_path).exists()),
        "preserving an existing film; choose a new prefix");
    let started = std::time::Instant::now();
    let t34_path = crate::resource::resolve(t34_weights)?;
    let tiger_path = crate::resource::resolve(tiger_weights)?;
    let t34 = crate::ai::Mlp::load_t34(t34_path.to_str().context("T-34 policy path")?)
        .with_context(|| format!("loading T-34 policy {t34_weights}"))?;
    let tiger = crate::ai::Mlp::load(tiger_path.to_str().context("Tiger policy path")?)
        .with_context(|| format!("loading Tiger policy {tiger_weights}"))?;
    if act == 1 {
        let requested = crate::act_manifest::playtest_manifest_path();
        ensure!(crate::act_manifest::select_runtime_manifest(Some(requested))? == Some(1),
            "act 1 requires the Act I manifest");
    }
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &crate::world::act(act))?;

    // The crew-battle witness's opening (`crew_battle_capture::run_crew_battle_capture`).
    g.god = false;
    g.brawl = false;
    g.targets_only = false;
    let center = if g.act.id == 0 { Vec2::new(210.0, -210.0) } else { g.act.spawn };
    let active = g.stage_showreel_t34_gauntlet_at(count, seed, ring, center);
    if g.act.id != 0 {
        let to_goal = g.act.goal - g.act.spawn;
        g.player.heading = to_goal.x.atan2(to_goal.y);
    }
    g.enemy_policy = Some(t34.clone());
    g.set_playtest_return_fire(true);
    g.enable_singleplayer_crew();
    g.debug_begin_crew_battle_audit();
    g.begin_audio_capture();
    g.set_defer_rtt_to_render(true);
    let staged: Vec<Value> = active.iter().map(|&i| json!({"enemy": i, "at": g.enemies[i].center().to_array(),
        "range_m": g.enemies[i].center().distance(g.player.center())})).collect();
    println!("[ai-film] act {act} ({}), {count} T-34s on a {ring:.0} m ring, seed {seed:#010x}, Tiger at {:?}{}",
        g.act.name, g.player.center(), if scan { ", scan only" } else { "" });

    let frame = Frame { device: &device, queue: &queue, w: r.iw, h: r.ih,
        banner: format!("AI COMBAT  ACT {act}  TIGER: NEURAL PILOT + AI CREW  VS  {count} NEURAL T-34  SEED {seed:#010x}") };
    let mut encoder = if scan { None } else { Some(Encoder::start(&format!("{prefix}_picture.mp4"), r.iw, r.ih)?) };
    let mut pilot = crate::showreel::TigerPilot::new(&tiger);
    let mut director = Director::new();
    let mut events: Vec<Value> = Vec::new();
    let mut shots: Vec<Value> = Vec::new();
    let mut tick: u64 = 0;
    let mut battle_s = 0.0f32;
    let mut over_at: Option<f32> = None;
    let mut death: Option<bool> = None;
    let mut last_shot_label = "";
    loop {
        let mut sound = Vec::with_capacity(FRAME_SAMPLES);
        let mut frame_events = Vec::new();
        for _ in 0..TICKS_PER_FRAME {
            let alive: Vec<bool> = active.iter().map(|&i| g.enemies[i].alive).collect();
            let (hits, pens) = (g.stat_hits_on_player, g.stat_pens_on_player);
            let scars = g.armor_scars.scars().len();
            let crew = g.singleplayer_living_crew().unwrap_or(0);
            let (fire, first, tiger_alive) = (g.fire_active, g.cookoff.first_s.is_some(), g.player.alive);
            let now = (tick + 1) as f64 / f64::from(SIM_HZ);
            g.debug_crew_battle_clock(tick + 1, now);
            g.debug_engage_all();
            let input = pilot.input_with_dt(&mut g, tick as usize, STEP);
            g.update(&input, STEP, &mut r);
            sound.extend(g.render_audio_capture_frames(sample_count(tick))?);
            tick += 1;
            battle_s = tick as f32 * STEP;
            let fresh: Vec<crate::armor_damage::ArmorScar> = g.armor_scars.scars().get(scars..).map(|s| s.to_vec()).unwrap_or_default();
            let newest = |want_hole: bool| {
                use crate::armor_damage::ScarKind;
                fresh.iter().copied().find(|s| if want_hole { s.kind == ScarKind::Hole } else { s.kind != ScarKind::Pock })
                    .or_else(|| fresh.first().copied())
                    .or_else(|| g.armor_scars.scars().last().copied())
            };
            for (k, &i) in active.iter().enumerate() {
                if alive[k] && !g.enemies[i].alive {
                    frame_events.push(Event::Kill { enemy: i, catastrophic: g.capture_catastrophic(Some(i)) });
                }
            }
            if g.stat_pens_on_player > pens {
                frame_events.push(Event::Penetrated { scar: newest(true) });
            } else if g.stat_hits_on_player > hits {
                frame_events.push(Event::Struck { scar: newest(false) });
            }
            let living = g.singleplayer_living_crew().unwrap_or(0);
            if living < crew {
                frame_events.push(Event::CrewLost { living });
            }
            if g.fire_active && !fire {
                frame_events.push(Event::Fire);
            }
            if g.cookoff.first_s.is_some() && !first {
                frame_events.push(Event::FirstRound);
            }
            if tiger_alive && !g.player.alive {
                frame_events.push(Event::TigerDestroyed { cookoff: g.cookoff_death() });
                break;
            }
        }
        for &event in &frame_events {
            let detail = match event {
                Event::Kill { enemy, catastrophic } => json!({"kind": "kill", "enemy": enemy, "catastrophic": catastrophic,
                    "range_m": g.enemies[enemy].center().distance(g.player.center()),
                    "turret": g.debug_enemy_turret_dynamics(enemy)}),
                Event::Penetrated { scar } | Event::Struck { scar } => json!({
                    "kind": if matches!(event, Event::Penetrated { .. }) { "tiger_penetrated" } else { "tiger_struck" },
                    "scar": scar.map(|s| format!("{:?} {:?} at {:?} mm, radius {} mm", s.facing, s.kind, s.local_mm, s.radius_mm)),
                    "chain": g.fea.receipts.last().map(|c| json!({"shell": c.shell, "terminal": format!("{:?}", c.terminal),
                        "knocked_out": c.knocked_out.iter().map(|k| k.name()).collect::<Vec<_>>()})),
                    "hits": g.stat_hits_on_player, "pens": g.stat_pens_on_player, "tiger_hp": g.player.hp}),
                Event::CrewLost { living } => json!({"kind": "crew_lost", "living": living}),
                Event::Fire => json!({"kind": "fire_in_tiger"}),
                Event::FirstRound => json!({"kind": "first_round_ignites", "cookoff": g.cookoff_receipt()}),
                Event::TigerDestroyed { cookoff } => json!({"kind": "tiger_destroyed", "cookoff_death": cookoff,
                    "cause": format!("{:?}", g.death_cause), "replay_has_impact": g.replay_has_impact()}),
            };
            println!("[ai-film] t={battle_s:.2}s {detail}");
            let mut entry = detail;
            entry["battle_s"] = json!(battle_s);
            entry["film_s"] = json!(encoder.as_ref().map_or(0.0, |e| e.frames as f32 / FPS as f32));
            events.push(entry);
            director.on_event(event, battle_s);
            if let Event::TigerDestroyed { cookoff } = event {
                death = Some(cookoff);
            }
        }
        if death.is_some() {
            // The ticks of this last frame up to the death are not filmed (at most 25 ms).
            break;
        }
        if over_at.is_none() && active.iter().all(|&i| !g.enemies[i].alive) {
            over_at = Some(battle_s);
        }
        let quiet = !matches!(director.shot, Shot::KillWide(_) | Shot::KillNear(_)) && director.queue.is_empty();
        if over_at.is_some_and(|t| battle_s - t >= 5.0) && quiet {
            break;
        }
        if battle_s >= max_secs && (quiet || battle_s >= max_secs + 20.0) {
            break;
        }
        director.advance(&g);
        if let Some(enc) = encoder.as_mut() {
            let saved = SavedView::save(&r);
            aim_shot(&mut director, &g, &mut r, &active);
            let settle = if director.cut {
                r.reset_history();
                director.cut = false;
                CUT_SETTLE
            } else {
                0
            };
            if director.shot.label() != last_shot_label || settle > 0 {
                shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "battle_s": battle_s, "shot": director.shot.label()}));
                last_shot_label = director.shot.label();
            }
            let text = caption(&director, &g, battle_s, battle_s < 6.0);
            let rgba = frame.finish(&mut r, &text, settle)?;
            enc.push(&rgba, Some(&sound))?;
            saved.restore(&mut r);
            g.drive_singleplayer_crew_rigs_for_view(g.singleplayer_embodied_role(), &mut r);
            if enc.frames % (FPS as u64 * 5) == 0 {
                println!("[ai-film] film {:.1} s, battle {battle_s:.1} s, shot {}, hits {}, pens {}, crew {:?}, T-34s up {}, wall {:.0} s",
                    enc.frames as f32 / FPS as f32, director.shot.label(), g.stat_hits_on_player, g.stat_pens_on_player,
                    g.singleplayer_living_crew(), active.iter().filter(|&&i| g.enemies[i].alive).count(),
                    started.elapsed().as_secs_f32());
            }
        } else if tick % (u64::from(SIM_HZ) * 10) < u64::from(TICKS_PER_FRAME) {
            println!("[ai-film] scan battle {battle_s:.1} s, hits {}, pens {}, crew {:?}, T-34s up {}, wall {:.0} s",
                g.stat_hits_on_player, g.stat_pens_on_player, g.singleplayer_living_crew(),
                active.iter().filter(|&&i| g.enemies[i].alive).count(), started.elapsed().as_secs_f32());
        }
    }

    // The Tiger's death, as the app plays it (`main::render_death`).
    let mut death_shots = Vec::new();
    if let (Some(cookoff), Some(enc)) = (death, encoder.as_mut()) {
        let (w, h) = (frame.w, frame.h);
        let black = vec![0u8; (w * h * 4) as usize];
        if cookoff {
            // Black from the bang while it rings, the world held; the bang's sound as the app drains it.
            death_shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "shot": "cookoff-death: black"}));
            let mut audio_debt = 0.0f64;
            let mut t = 0.0;
            for _ in 0..(COOKOFF_BLACK_S * FPS as f32).round() as u32 {
                t += FRAME_DT;
                g.advance_terminal_audio(FRAME_DT, &r, t);
                audio_debt += acoustics::SAMPLE_RATE as f64 / FPS as f64;
                let n = audio_debt.floor();
                audio_debt -= n;
                let sound = g.render_audio_capture_frames(n as usize)?;
                enc.push(&black, Some(&sound))?;
            }
            // Outside, the world running on, silent as in the app; then the dead from above.
            for (label, seconds, crew) in [("cookoff-death: outside (app shot)", COOKOFF_THROW_S, false),
                ("cookoff-death: the dead (app shot)", COOKOFF_CREW_S, true)] {
                death_shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "shot": label}));
                r.set_active_view(0);
                r.reset_history();
                let frames = (seconds * FPS as f32).round() as u32;
                for f in 0..frames {
                    for _ in 0..TICKS_PER_FRAME {
                        g.update(&TankInput::default(), STEP, &mut r);
                        let _ = g.render_audio_capture_frames(sample_count(tick))?;
                        tick += 1;
                    }
                    let phase = f as f32 / frames as f32;
                    if crew { g.render_cookoff_crew(phase, &mut r) } else { g.render_cookoff_outside(phase, &mut r) }
                    let text = if crew { "THE COOK-OFF DEATH AS THE APP PLAYS IT: THE DEAD, FROM ABOVE THE OPEN RING" }
                        else { "THE COOK-OFF DEATH AS THE APP PLAYS IT: OUTSIDE, THE STOWAGE GOES" };
                    let rgba = frame.finish(&mut r, text, if f == 0 { CUT_SETTLE } else { 0 })?;
                    enc.push(&rgba, None)?;
                }
            }
        } else if g.replay_has_impact() {
            death_shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "shot": "penetration-death: black"}));
            for _ in 0..(crate::IMPACT_BLACK_S * FPS as f32).round() as u32 {
                enc.push(&black, None)?;
            }
            r.set_active_view(0);
            r.reset_replay_history();
            let mut elapsed = 0.0f32;
            for (shot, seconds, label) in [
                (0u8, crate::IMPACT_SECONDS, "THE PENETRATION DEATH AS THE APP PLAYS IT: THE IMPACT, THE ARMOUR CUT AWAY (SLOW)"),
                (1, crate::REPLAY_DUR, "THE PENETRATION DEATH AS THE APP PLAYS IT: THE INTERIOR"),
                (2, crate::DEATH_EXTERIOR_SECONDS, "THE PENETRATION DEATH AS THE APP PLAYS IT: WHERE THE ROUND CAME FROM"),
            ] {
                death_shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "shot": label}));
                let frames = (seconds * FPS as f32).round() as u32;
                for f in 0..frames {
                    let phase = f as f32 / frames as f32;
                    match shot {
                        0 => g.render_replay_impact(phase, &mut r),
                        1 => { g.end_replay_cutaway(&mut r); g.render_replay(phase, &mut r); }
                        _ => { g.end_replay_exterior(&mut r); g.render_replay_exterior(phase, &mut r); }
                    }
                    elapsed += FRAME_DT;
                    for _ in 0..REPLAY_SETTLE {
                        r.render_replay_offscreen(&device, &queue, elapsed, crate::renderer::ReplayCapturePolicy::Current);
                    }
                    let rgba = frame.finish(&mut r, label, 0)?;
                    enc.push(&rgba, None)?;
                }
            }
            g.end_replay_cutaway(&mut r);
            g.end_replay_exterior(&mut r);
        } else {
            // No recorded impact (the crew lost another way): the Tiger from outside a while.
            death_shots.push(json!({"film_s": enc.frames as f32 / FPS as f32, "shot": "tiger lost: outside"}));
            let tiger = g.player.center();
            let eye = lift(&g, tiger + Vec3::new(22.0, 8.0, -14.0), 3.0);
            r.reset_history();
            for f in 0..(6 * FPS) {
                let mut sound = Vec::with_capacity(FRAME_SAMPLES);
                for _ in 0..TICKS_PER_FRAME {
                    g.update(&TankInput::default(), STEP, &mut r);
                    sound.extend(g.render_audio_capture_frames(sample_count(tick))?);
                    tick += 1;
                }
                let saved = SavedView::save(&r);
                outside_lens(&g, &mut r, eye, tiger, 45.0, 1);
                let rgba = frame.finish(&mut r, "THE TIGER IS LOST", if f == 0 { CUT_SETTLE } else { 0 })?;
                enc.push(&rgba, Some(&sound))?;
                saved.restore(&mut r);
            }
        }
    }

    let outcome = json!({"tiger_alive": g.player.alive, "death_cause": format!("{:?}", g.death_cause),
        "cookoff_death": g.cookoff_death(), "t34_survivors": active.iter().filter(|&&i| g.enemies[i].alive).count(),
        "hits_on_tiger": g.stat_hits_on_player, "pens_on_tiger": g.stat_pens_on_player,
        "crew_living": g.singleplayer_living_crew(), "battle_s": battle_s, "cookoff": g.cookoff_receipt(),
        "catastrophic_kills": active.iter().filter(|&&i| g.capture_catastrophic(Some(i))).count()});
    println!("[ai-film] outcome {outcome}");
    let seconds = match encoder {
        Some(enc) => Some(enc.finish(&video_path)?),
        None => None,
    };
    let receipt = json!({
        "schema": "panzer.ai-film.v1", "video": seconds.map(|_| video_path.clone()), "seconds": seconds,
        "act": {"id": g.act.id, "name": g.act.name}, "count": count, "seed": seed, "ring_m": ring, "max_secs": max_secs,
        "weights": {"t34": t34_weights, "tiger": tiger_weights}, "staged": staged,
        "staging": "the crew-battle witness's opening: T-34s on the showreel's seeded ring round the Tiger (stage_showreel_t34_gauntlet_at), every T-34 told where the Tiger is each tick (debug_engage_all); the Tiger driven by the neural pilot with its embodied auto-crew (showreel::TigerPilot), the singleplayer crew aboard; nothing after the opening is scripted",
        "film": {"grid": [frame.w, frame.h], "scale": 5, "fps": FPS, "simulation_hz": SIM_HZ,
            "audio": "ordinary gameplay audio recorded offline on the simulation clock; the listener is the Tiger's crewman under every shot; the death's shots after the black are silent as in the app",
            "cuts": format!("history reset and {CUT_SETTLE} discarded renders at each cut; death replay frames {REPLAY_SETTLE} discarded renders each")},
        "events": events, "shots": shots, "death_shots": death_shots, "outcome": outcome,
        "wall_s": started.elapsed().as_secs_f32(),
    });
    if !scan {
        std::fs::write(&receipt_path, serde_json::to_string_pretty(&receipt)?)?;
    }
    println!("[ai-film] done in {:.0} s: {}", started.elapsed().as_secs_f32(), seconds.map_or("scan only".into(), |s| format!("{video_path} ({s:.1} s)")));
    Ok(())
}
