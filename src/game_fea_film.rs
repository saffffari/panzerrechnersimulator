//! `fea-film PREFIX`: video clips of the layered damage model in play
//! (docs/FEA_DAMAGE_MODEL_20261102.md). The `fea-play` review's scene (Canyon v2 winter workshop,
//! act 1, the Tiger alone with its racks empty) and its real rounds through the ordinary flight
//! sweep and terminal resolver, filmed at 1920x1080 and 30 fps:
//!
//! 1. `lodged_round`: a 45 mm AP round stops in the lower front plate and stays there.
//! 2. `turret_spall`: a 76 mm APCR round through the turret rear, from the loader's place: the
//!    lattice's chips fly, glow, come to rest and cool.
//! 3. `loader_hands`: the loader pries a chip the floor holds with the toolbox's bar, takes one
//!    and throws it out of the open hatch, takes another and lets it fall.
//! 4. `sledge`: on foot with the toolbox, the sledge knocks the lodged round out.
//! 5. `hole_cooling`: a fresh hole through the turret side, glowing at its solved local
//!    temperature and cooling.
//! 6. `crew_impact`: the living crew at their posts; a round through the side at the gunner,
//!    from the commander's place.
//! 7. `exterior_impacts`: a glacis ricochet, a 122 mm through the turret front, an 85 mm APHE
//!    through the hull front, each watched from outside.
//! 8. `burst_152`: a 152 mm BR-540 bursts in the turret; the lids hold.
//!
//! Real-time stretches carry the game's own sound: the ordinary gameplay audio sinks recorded
//! offline on the simulation clock (every game update renders exactly its span). Slow-motion and
//! time-lapse stretches are silent and carry a caption naming their time scale; slow motion is
//! one fixed fragment step (1/480 s) per frame, 1/16 speed, the finest the fragment integrator
//! steps. Disclosed harness actions as in `fea-play` (station and on-foot placement set
//! directly, the toolbox put in hand and set down by state, a lid opened by state, cabin fires put
//! out between clips), and: the sky clock is held at 10:30 for every frame, and the head turns
//! between targets are smooth interpolations of the ordinary head yaw and pitch. Requires
//! KESSEL_RENDERER=raymarch.
use super::game_fea_review as review;
use super::*;
use crate::{acoustics, act_manifest, celestial_sky, ephemeris, graphics, world};
use serde_json::json;
use std::io::Write;
use std::process::{Child, Command, Stdio};

/// Frames are shot at the renderer's own grid (`KESSEL_INTERNAL_RES`) and the file takes them
/// scaled exactly `film_scale()` times by nearest neighbour (the owner's settings, 2026-11-02:
/// 1024x576 x5 for a 5120x2880 display, 4:4:4 chroma so the one-pixel dither survives).
static FRAME: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();

fn frame() -> (u32, u32) {
    *FRAME.get().unwrap_or(&(1920, 1080))
}

/// `KESSEL_FEA_FILM_SCALE`, default 5.
fn film_scale() -> u32 {
    std::env::var("KESSEL_FEA_FILM_SCALE").ok().and_then(|v| v.parse().ok()).unwrap_or(5u32).clamp(1, 8)
}

/// A still as PNG, scaled `film_scale()` times by nearest neighbour.
fn save_still(path: &str, rgba: &[u8]) -> anyhow::Result<()> {
    let (w, h) = frame();
    let image = image::RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or_else(|| anyhow::anyhow!("still of the wrong size"))?;
    let k = film_scale();
    image::imageops::resize(&image, w * k, h * k, image::imageops::FilterType::Nearest).save(path)?;
    Ok(())
}
const FPS: u32 = 30;
/// Slow motion: one fixed hot-fragment step per film frame (1/16 speed at 30 fps).
const SLOW_DT: f32 = 1.0 / 480.0;
const TICK: f32 = 1.0 / 60.0;
/// Stereo audio samples in one film frame.
const FRAME_SAMPLES: usize = 2 * (acoustics::SAMPLE_RATE / FPS) as usize;

#[derive(Clone, Copy)]
enum View {
    /// The game's own camera (the crewman's eye).
    Game,
    /// An observer camera outside the vehicle: eye, target, vertical field of view (degrees).
    Outside(Vec3, Vec3, f32),
    /// An observer camera inside the vehicle (its interior light and air kept).
    Inside(Vec3, Vec3, f32),
    /// A cook-off death's shots as the player sees them (`Game::render_cookoff_outside`,
    /// `Game::render_cookoff_crew`) at their phase.
    CookoffOutside(f32),
    CookoffCrew(f32),
}

struct Ctx<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    audio_debt: f64,
    clock: ephemeris::UtcInstant,
    /// Draw every crewman, the player's own body too (an observer camera's view of the crew).
    all_crew: bool,
    /// A line drawn at the top of every frame while set.
    banner: Option<String>,
    /// The camera of the last frame shot (eye, forward), to tell a moving camera.
    last_camera: Option<(Vec3, Vec3)>,
}

/// Extra frames rendered from a moving camera's pose before its frame is kept: the transport's
/// history restarts when the camera moves, so these are more samples, not a filter.
const MOVING_CAMERA_SETTLE: u32 = 16;

impl Ctx<'_> {
    /// One game update and the gameplay audio of exactly its span.
    fn update(&mut self, g: &mut Game, r: &mut Renderer, input: &TankInput, dt: f32) -> anyhow::Result<Vec<f32>> {
        g.update(input, dt, r);
        self.audio_debt += dt as f64 * acoustics::SAMPLE_RATE as f64;
        let frames = self.audio_debt.floor();
        self.audio_debt -= frames;
        g.render_audio_capture_frames(frames as usize)
    }

    /// Unfilmed game time (its sound is rendered and discarded, keeping the audio clock in step).
    fn run(&mut self, g: &mut Game, r: &mut Renderer, seconds: f32, dt: f32) -> anyhow::Result<()> {
        for _ in 0..(seconds / dt).round() as usize {
            self.update(g, r, &TankInput::default(), dt)?;
        }
        Ok(())
    }

    fn aim(&self, g: &Game, r: &mut Renderer, view: View) {
        r.set_sky_clock(celestial_sky::SkyClock::new(self.clock, r.sky_clock.site));
        g.drive_armor_scars(r);
        if self.all_crew {
            g.drive_singleplayer_crew_rigs_for_view(None, r);
        }
        match view {
            View::Game => {}
            View::CookoffOutside(phase) => g.render_cookoff_outside(phase, r),
            View::CookoffCrew(phase) => g.render_cookoff_crew(phase, r),
            View::Outside(eye, at, fov) | View::Inside(eye, at, fov) => {
                r.camera.pos = eye;
                r.camera.set_forward(at - eye);
                r.camera.fov_y = fov.to_radians();
                r.optic_mask = 0.0;
                r.reticle = false;
                if matches!(view, View::Outside(..)) {
                    r.interior_light = 0.0;
                    r.interior_haze = 0.0;
                    r.weather_shelter = 0.0;
                    r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
                }
            }
        }
    }

    /// One film frame of the present state from `view`.
    fn shoot(&mut self, g: &Game, r: &mut Renderer, view: View, caption: Option<&str>) -> anyhow::Result<Vec<u8>> {
        self.aim(g, r, view);
        let camera = (r.camera.pos, r.camera.forward());
        let moved = self.last_camera.is_some_and(|(p, f)| p.distance(camera.0) > 1.0e-4 || f.dot(camera.1) < 0.999_999);
        self.last_camera = Some(camera);
        if moved {
            for _ in 0..MOVING_CAMERA_SETTLE {
                r.render_offscreen(self.device, self.queue);
            }
        }
        r.ui_reset();
        if matches!(view, View::Game) {
            g.draw_repair_cue(r);
        }
        if let Some(caption) = caption {
            r.fill_rect(0.015, 0.922, 0.012 * caption.len() as f32 + 0.03, 0.052, [0.0, 0.0, 0.0, 0.62]);
            r.draw_text(caption, 0.026, 0.948, 0.023, [0.88, 0.86, 0.80, 1.0], crate::ui::Align::Left);
        }
        if let Some(banner) = &self.banner {
            r.fill_rect(0.015, 0.026, 0.0105 * banner.len() as f32 + 0.03, 0.048, [0.0, 0.0, 0.0, 0.62]);
            r.draw_text(banner, 0.026, 0.050, 0.020, [0.88, 0.86, 0.80, 1.0], crate::ui::Align::Left);
        }
        let (w, h) = frame();
        r.render_to_rgba_with_hud(self.device, self.queue, w, h)
    }

    /// The exposure re-metered on the present scene from `view` by the eye's own meter and then
    /// held (a camera operator's exposure change between segments; no light or simulation change).
    fn meter(&mut self, g: &Game, r: &mut Renderer, view: View) -> anyhow::Result<Option<f32>> {
        self.aim(g, r, view);
        r.exposure_override = None;
        let metered = crate::converge_headless_screenshot(r, self.device, self.queue, 4)?.measured_exposure;
        r.exposure_override = metered;
        Ok(metered)
    }

    /// A camera cut: the renderer's history restarts, and its transport and the eye's exposure
    /// settle on the new view before the first kept frame (as for a still).
    fn cut(&mut self, g: &Game, r: &mut Renderer, view: View) -> anyhow::Result<()> {
        self.aim(g, r, view);
        r.reset_history();
        crate::converge_headless_screenshot(r, self.device, self.queue, 16)?;
        Ok(())
    }
}

struct Clip {
    name: String,
    path: String,
    picture: String,
    encoder: Child,
    sound: Vec<f32>,
    frames: u64,
    segments: Vec<serde_json::Value>,
}

impl Clip {
    fn start(prefix: &str, name: &str) -> anyhow::Result<Self> {
        let path = format!("{prefix}_{name}.mp4");
        let picture = format!("{prefix}_{name}_picture.mp4");
        let (w, h) = frame();
        let size = format!("{w}x{h}");
        let rate = FPS.to_string();
        let k = film_scale();
        let scale = format!("scale=iw*{k}:ih*{k}:flags=neighbor");
        let encoder = Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pixel_format", "rgba",
                "-video_size", &size, "-framerate", &rate, "-i", "pipe:0", "-an", "-vf", &scale, "-c:v", "libx264", "-preset",
                "medium", "-crf", "8", "-pix_fmt", "yuv444p", "-movflags", "+faststart", &picture])
            .stdin(Stdio::piped())
            .spawn()?;
        println!("[fea-film] recording {name}");
        Ok(Self { name: name.into(), path, picture, encoder, sound: Vec::new(), frames: 0, segments: Vec::new() })
    }

    fn mark(&mut self, label: &str, scale: &str) {
        self.segments.push(json!({"label": label, "time_scale": scale, "film_s": self.frames as f32 / FPS as f32}));
    }

    fn push(&mut self, rgba: &[u8], sound: Option<&[f32]>) -> anyhow::Result<()> {
        self.encoder.stdin.as_mut().expect("encoder input").write_all(rgba)?;
        self.frames += 1;
        let target = self.frames as usize * FRAME_SAMPLES;
        if let Some(sound) = sound {
            self.sound.extend_from_slice(sound);
        }
        // Keep picture and sound the same length: pad (silence) or trim to this frame's end.
        self.sound.resize(target, 0.0);
        Ok(())
    }

    /// Real time with sound: two 60 Hz game updates per frame.
    fn real_time(&mut self, ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, seconds: f32,
        input: &dyn Fn(u32) -> TankInput, view: &dyn Fn(&Game, f32) -> View) -> anyhow::Result<()> {
        let frames = (seconds * FPS as f32).round().max(1.0) as u32;
        for f in 0..frames {
            let mut sound = Vec::with_capacity(FRAME_SAMPLES);
            for k in 0..2 {
                sound.extend(ctx.update(g, r, &input(2 * f + k), TICK)?);
            }
            let rgba = ctx.shoot(g, r, view(g, f as f32 / frames as f32), None)?;
            self.push(&rgba, Some(&sound))?;
        }
        Ok(())
    }

    /// Silent slow motion at 1/16 speed: one fixed fragment step per frame.
    fn slow_motion(&mut self, ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, frames: u32,
        view: &dyn Fn(&Game, f32) -> View) -> anyhow::Result<()> {
        for f in 0..frames {
            ctx.update(g, r, &TankInput::default(), SLOW_DT)?;
            let rgba = ctx.shoot(g, r, view(g, f as f32 / frames as f32), Some("1/16 SPEED"))?;
            self.push(&rgba, None)?;
        }
        Ok(())
    }

    /// Silent time-lapse: `factor` seconds of game time per second of film.
    fn time_lapse(&mut self, ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, seconds: f32, factor: f32,
        caption: &str, each: &mut dyn FnMut(&mut Game), view: &dyn Fn(&Game, f32) -> View) -> anyhow::Result<()> {
        let frames = (seconds * FPS as f32).round().max(1.0) as u32;
        let per_frame = factor / FPS as f32;
        for f in 0..frames {
            ctx.run(g, r, per_frame, 1.0 / 30.0)?;
            each(g);
            let rgba = ctx.shoot(g, r, view(g, f as f32 / frames as f32), Some(caption))?;
            self.push(&rgba, None)?;
        }
        Ok(())
    }

    fn finish(mut self) -> anyhow::Result<serde_json::Value> {
        drop(self.encoder.stdin.take());
        let encoded = self.encoder.wait()?.success();
        if self.frames == 0 {
            // Nothing this clip waited for happened (its receipt says so); the film goes on.
            let _ = std::fs::remove_file(&self.picture);
            println!("[fea-film] {}: nothing to film, skipped", self.name);
            return Ok(json!({"clip": self.name, "skipped": "no frames: what it films did not happen"}));
        }
        anyhow::ensure!(encoded, "{}: video encoder failed", self.name);
        acoustics::attenuate_over_peak(&mut self.sound, 0.94);
        let wav = format!("{}.wav", self.path.trim_end_matches(".mp4"));
        acoustics::write_wav_stereo(&wav, &self.sound, acoustics::SAMPLE_RATE)?;
        let status = Command::new("nice")
            .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", &self.picture, "-i", &wav,
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest",
                "-movflags", "+faststart", &self.path])
            .status()?;
        anyhow::ensure!(status.success(), "{}: sound mux failed", self.name);
        // Saved once, with its sound: the silent picture and the soundtrack go.
        let _ = std::fs::remove_file(&self.picture);
        let _ = std::fs::remove_file(&wav);
        let seconds = self.frames as f32 / FPS as f32;
        println!("[fea-film] wrote {} ({seconds:.1} s)", self.path);
        Ok(json!({"clip": self.name, "path": self.path, "seconds": seconds, "frames": self.frames,
            "segments": self.segments}))
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The head yaw and pitch that put the loader's eye on `target` (as `loader_face_world` sets them).
fn head_toward(g: &mut Game, target: Vec3) -> (f32, f32) {
    let before = (g.head_yaw, g.head_pitch);
    g.loader_face_world(target);
    let after = (g.head_yaw, g.head_pitch);
    g.head_yaw = before.0;
    g.head_pitch = before.1;
    after
}

/// Film a smooth turn of the loader's head onto `target` over `seconds`, holding `input`.
fn turn_head(clip: &mut Clip, ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, target: Vec3, seconds: f32,
    input: TankInput, view: &dyn Fn(&Game, f32) -> View) -> anyhow::Result<()> {
    let from = (g.head_yaw, g.head_pitch);
    let to = head_toward(g, target);
    let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    let dyaw = wrap(to.0 - from.0);
    let frames = (seconds * FPS as f32).round().max(1.0) as u32;
    for f in 0..frames {
        let k = smoothstep((f + 1) as f32 / frames as f32);
        let mut sound = Vec::with_capacity(FRAME_SAMPLES);
        for _ in 0..2 {
            g.head_yaw = wrap(from.0 + dyaw * k);
            g.head_pitch = from.1 + (to.1 - from.1) * k;
            sound.extend(ctx.update(g, r, &input, TICK)?);
        }
        let rgba = ctx.shoot(g, r, view(g, (f + 1) as f32 / frames as f32), None)?;
        clip.push(&rgba, Some(&sound))?;
    }
    Ok(())
}

/// A chip's world position.
fn chip_at(g: &Game, slot: usize) -> Vec3 {
    let f = &g.hot_fragments[slot];
    blood_parent_model(&g.player, f.frame).transform_point3(f.pos)
}

/// The densest knot of resting solved chips within `reach` of `eye`: its centre and radius.
fn chip_cluster(g: &Game, eye: Vec3, reach: f32) -> Option<(Vec3, f32)> {
    let chips: Vec<Vec3> = g.hot_fragments.iter().enumerate()
        .filter(|(_, f)| f.active && f.temp_k > 0.0)
        .map(|(i, _)| chip_at(g, i)).filter(|p| p.distance(eye) < reach).collect();
    let best = chips.iter().max_by_key(|p| chips.iter().filter(|q| q.distance(**p) < 0.18).count())?;
    let near: Vec<Vec3> = chips.iter().copied().filter(|q| q.distance(*best) < 0.18).collect();
    let centre = near.iter().fold(Vec3::ZERO, |a, b| a + *b) / near.len() as f32;
    let radius = near.iter().map(|q| q.distance(centre)).fold(0.0, f32::max);
    Some((centre, radius))
}

/// Field of view (degrees) that frames a knot of `radius` at `distance` with room round it.
fn framing_fov(radius: f32, distance: f32) -> f32 {
    (2.0 * (radius.max(0.06) * 1.8 / distance.max(0.1)).atan()).to_degrees().clamp(8.0, 32.0)
}

/// An observer eye `distance` from `centre` with a clear line to it through the vehicle's own
/// source surfaces: the outward normal first, then tilted round it.
fn clear_eye(g: &Game, centre: Vec3, normal: Vec3, distance: f32) -> Vec3 {
    let models = g.fire_surface_models();
    let t = normal.any_orthonormal_vector();
    let b = normal.cross(t);
    for angle in [0.0f32, 15.0, 30.0, 45.0, 60.0] {
        for side in [t, -t, b, -b] {
            let a = angle.to_radians();
            let dir = (normal * a.cos() + side * a.sin() + Vec3::Y * 0.12).normalize();
            let far = centre + dir * 2.5;
            let blocked = g.projectile_player_armor.sweep(far, centre, &models).is_some_and(|c| c.fraction < 0.97);
            if !blocked {
                return centre + dir * distance;
            }
            if angle == 0.0 {
                break;
            }
        }
    }
    centre + normal * distance
}

/// One round that kills one man, for the voxel tissue (`crew_tissue`).
struct CasualtyStudy {
    name: &'static str,
    label: &'static str,
    clock: &'static str,
    seed: u32,
    role: CrewRole,
    /// Entry and eyes are in the turret's frame (else the hull's).
    turret: bool,
    shell: &'static crate::ballistics::ShellSpec,
    range: f32,
    /// Where the player's own body is (away from the camera).
    player: Station,
    /// The loader's roof lid opened by state (disclosed in the label): daylight into the turret.
    loader_lid: bool,
    /// The entry on the bare plate, from the man's torso (both in the frame).
    entry: fn(Vec3) -> Vec3,
    /// Candidate eyes, in order; the first with a clear line to his torso is used.
    eyes: fn(Vec3) -> Vec<Vec3>,
}

/// The cook-off study (see the clip list, 17). Disclosed: the fire's first patch is staged on a
/// real surface (`debug_start_surface_fire`) and the strike is a real round launched at the rack;
/// everything after (spread, heating, ignitions, pressure, the throw) is the game's own.
fn cookoff_study(ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, prefix: &str, name: &str, struck: bool,
    idle_input: &dyn Fn(u32) -> TankInput) -> anyhow::Result<serde_json::Value> {
    let stills = std::env::var("KESSEL_FEA_FILM_STILLS").as_deref() == Ok("1");
    let day_clock = ctx.clock;
    ctx.clock = ephemeris::UtcInstant::parse_iso("1942-12-21T09:30:00Z")?;
    g.enable_singleplayer_crew();
    fresh_tiger(ctx, g, r, 0x5EC7_C00C ^ u32::from(struck), true)?;
    g.set_cabin_lights(true);
    g.jump_station(Station::Commander);
    ctx.run(g, r, 1.0, TICK)?;
    let hull = g.player.hull_model();
    let ring = hull.transform_point3(g.player.parts.turret_pivot);
    let side = hull.transform_vector3(Vec3::X).normalize();
    let back = hull.transform_vector3(Vec3::NEG_Z).normalize();
    let ground = |g: &Game, p: Vec3| Vec3::new(p.x, p.y.max(g.hf.height_at(p.x, p.z) + 1.7), p.z);
    let wide = View::Outside(ground(g, ring + side * 24.0 + back * 9.0 + Vec3::Y * 2.5), ring + Vec3::Y * 4.0, 42.0);
    let still = |ctx: &mut Ctx, g: &Game, r: &mut Renderer, view: View, tag: &str| -> anyhow::Result<()> {
        let rgba = ctx.shoot(g, r, view, None)?;
        let path = format!("{prefix}_{name}_{tag}.png");
        save_still(&path, &rgba)?;
        println!("[fea-film] wrote {path}");
        Ok(())
    };
    ctx.banner = Some(if struck { "THE COOK-OFF: A 122 MM BR-471 INTO THE RIGHT SPONSON'S HE ROUNDS, 500 M; RACKS LOADED".into() }
        else { "THE COOK-OFF: A FIRE BESIDE THE LEFT SPONSON'S RACK; RACKS LOADED; THE CASES HEAT TO THEIR PROPELLANT'S IGNITION".into() });
    let mut film = if stills { None } else { Some(Clip::start(prefix, name)?) };
    ctx.cut(g, r, wide)?;
    match film.as_mut() {
        Some(clip) => { clip.mark("the Tiger", "real time"); clip.real_time(ctx, g, r, 1.0, idle_input, &|_, _| wide)?; }
        None => still(ctx, g, r, wide, "before")?,
    }
    let mut log = Vec::new();
    if struck {
        // Into the right sponson's group of SprGr (HE) rounds (`interior_rig::HE_RACK_SOURCE_IDS`).
        g.debug_launch_round(&crate::ballistics::BR_471, false, hull.transform_point3(Vec3::new(-1.95, 1.69, 0.62)),
            hull.transform_vector3(Vec3::new(1.0, -0.02, 0.0)), 500.0, 0x5EC7_C0DE);
    } else {
        // A first flame on the rack's fitting, inboard of the left sponson's rounds.
        let staged = g.debug_start_surface_fire("hull", Vec3::new(0.85, 1.85, 0.05), Vec3::new(1.75, 1.55, 0.05), 1.0)?;
        println!("[fea-film] {name}: fire staged {staged}");
        // Unfilmed until the hottest case is close to igniting (a minute at most of game time).
        let mut t = 0.0;
        while g.cookoff.first_s.is_none() && g.cookoff.ammo_urgency() < 0.85 && t < 300.0 {
            ctx.run(g, r, 1.0, TICK)?;
            t += 1.0;
            if (t as u32) % 10 == 0 {
                println!("[fea-film] {name}: {t:.0} s, fire {:.2}, soot {:.2}, {}", g.fire, g.interior_soot, g.cookoff_receipt());
            }
        }
        log.push(json!({"heated_s": t, "state": g.cookoff_receipt()}));
    }
    // Until the bang (filmed wide while the cases heat): then what the player sees in a cook-off
    // death (`main::cookoff_shot`): black while the bang rings, the world held; the tank from
    // outside as the stowage goes and the turret is thrown, the world running on, silent; the dead
    // from above the open ring. Stills: the outside shot at its start, at the turret's apex and at
    // its end, and the dead.
    let mut steps = 0u32;
    while !g.cookoff_death() && steps < (if struck { 6 } else { 90 }) * 60 {
        match film.as_mut() {
            Some(clip) => {
                if steps == 0 { clip.mark(if struck { "the strike" } else { "the cases heat" }, "real time"); }
                clip.real_time(ctx, g, r, 1.0 / 30.0, idle_input, &|_, _| wide)?;
                steps += 2;
            }
            None => {
                ctx.update(g, r, &TankInput::default(), TICK)?;
                steps += 1;
            }
        }
    }
    log.push(json!({"to_the_bang_s": steps as f32 * TICK, "player_died_in_it": g.cookoff_death(), "state": g.cookoff_receipt()}));
    if g.cookoff_death() {
        // Black, the world held; the bang's sound drained as the app drains it.
        let black_frames = (COOKOFF_BLACK_S * FPS as f32).round() as u32;
        let (w, h) = frame();
        let black = vec![0u8; (w * h * 4) as usize];
        let mut t = 0.0;
        for _ in 0..black_frames {
            t += 1.0 / FPS as f32;
            if let Some(clip) = film.as_mut() {
                g.advance_terminal_audio(1.0 / FPS as f32, r, t);
                ctx.audio_debt += acoustics::SAMPLE_RATE as f64 / FPS as f64;
                let n = ctx.audio_debt.floor();
                ctx.audio_debt -= n;
                let sound = g.render_audio_capture_frames(n as usize)?;
                clip.push(&black, Some(&sound))?;
            }
        }
        if let Some(clip) = film.as_mut() { clip.mark("black from the bang, while it rings", "real time"); }
        // Outside, the world running on (two updates a frame), silent as in the app.
        let frames = (COOKOFF_THROW_S * FPS as f32).round() as u32;
        let mut apex_written = false;
        if let Some(clip) = film.as_mut() { clip.mark("outside: the stowage goes, the turret is thrown", "real time, silent"); }
        ctx.cut(g, r, View::CookoffOutside(0.0))?;
        if stills { still(ctx, g, r, View::CookoffOutside(0.0), "outside_start")?; }
        for f in 0..frames {
            let rising = g.cookoff.turret.as_ref().map(|b| b.velocity.y > 0.0);
            for _ in 0..2 {
                ctx.update(g, r, &TankInput::default(), TICK)?;
            }
            let view = View::CookoffOutside(f as f32 / frames as f32);
            if let Some(clip) = film.as_mut() {
                let rgba = ctx.shoot(g, r, view, None)?;
                clip.push(&rgba, None)?;
            }
            if stills && !apex_written && rising == Some(true) && g.cookoff.turret.as_ref().is_some_and(|b| b.velocity.y <= 0.0) {
                ctx.cut(g, r, view)?;
                still(ctx, g, r, view, "outside_apex")?;
                apex_written = true;
            }
        }
        if stills { ctx.cut(g, r, View::CookoffOutside(1.0))?; still(ctx, g, r, View::CookoffOutside(1.0), "outside_end")?; }
        // The dead, from above the open ring.
        let frames = (COOKOFF_CREW_S * FPS as f32).round() as u32;
        if let Some(clip) = film.as_mut() { clip.mark("the dead, from above the open ring", "real time, silent"); }
        ctx.cut(g, r, View::CookoffCrew(0.0))?;
        if stills { still(ctx, g, r, View::CookoffCrew(0.0), "crew_start")?; }
        for f in 0..frames {
            for _ in 0..2 {
                ctx.update(g, r, &TankInput::default(), TICK)?;
            }
            if let Some(clip) = film.as_mut() {
                let rgba = ctx.shoot(g, r, View::CookoffCrew(f as f32 / frames as f32), None)?;
                clip.push(&rgba, None)?;
            }
        }
        if stills { ctx.cut(g, r, View::CookoffCrew(1.0))?; still(ctx, g, r, View::CookoffCrew(1.0), "crew_end")?; }
    }
    log.push(json!({"after_s": steps as f32 * TICK, "state": g.cookoff_receipt()}));
    ctx.banner = None;
    ctx.clock = day_clock;
    let mut receipt = match film {
        Some(clip) => clip.finish()?,
        None => json!({"clip": name, "stills": true}),
    };
    receipt["outcome"] = strike_outcome(g);
    receipt["cookoff"] = g.cookoff_receipt();
    receipt["log"] = json!(log);
    receipt["corpses"] = g.crew_corpse_render_receipt();
    Ok(receipt)
}

/// The fallen man's head (world), from his body's head segment.
fn head_point(g: &Game, role: CrewRole) -> Option<Vec3> {
    let actor = g.singleplayer_casualty_identity(role).peer_id;
    let body = g.couch_ragdolls.iter().find(|b| b.casualty.peer_id == actor)?;
    Some(body.parent_model(&g.player).transform_point3(body.segments[crew_physics::CrewBodyPart::Head.index()].pos))
}

/// Whether the line from `eye` to `target` meets none of the tank's own surfaces (the source
/// triangles fragments and fire meet), short of the last 5 %.
fn sight_clear(g: &Game, eye: Vec3, target: Vec3) -> bool {
    g.cabin_fire.sweep(eye, target, &g.fire_surface_models()).is_none_or(|c| c.fraction > 0.95)
}

/// Film a casualty study, or with KESSEL_FEA_FILM_STILLS=1 write its three stills (before, after,
/// close) instead. None when the round killed nobody.
fn casualty_study(ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, prefix: &str, study: &CasualtyStudy,
    idle_input: &dyn Fn(u32) -> TankInput) -> anyhow::Result<Option<serde_json::Value>> {
    let stills = std::env::var("KESSEL_FEA_FILM_STILLS").as_deref() == Ok("1");
    let day_clock = ctx.clock;
    ctx.clock = ephemeris::UtcInstant::parse_iso(study.clock)?;
    g.enable_singleplayer_crew();
    fresh_tiger(ctx, g, r, study.seed, false)?;
    g.set_cabin_lights(true); // disclosed: the ordinary lamp switch on
    g.jump_station(study.player);
    if study.loader_lid {
        g.roof_hatches.set(false, true, true);
    }
    ctx.run(g, r, 1.0, TICK)?;
    let frame = if study.turret { g.player.turret_model() } else { g.player.hull_model() };
    let torso = |g: &Game| g.crew_solo_man_point(study.role);
    let man_w = torso(g);
    let man = frame.inverse().transform_point3(man_w);
    let entry = (study.entry)(man);
    let aim = frame.transform_point3(entry);
    let dir = frame.transform_vector3((man - entry).normalize());
    let candidates: Vec<Vec3> = (study.eyes)(man).into_iter().map(|e| frame.transform_point3(e)).collect();
    let clear: Vec<bool> = candidates.iter().map(|&e| sight_clear(g, e, man_w)).collect();
    let eye = candidates.iter().zip(&clear).find(|(_, c)| **c).map_or(candidates[0], |(e, _)| *e);
    println!("[fea-film] {}: torso at {:?} (frame), entry {:?}, eyes clear {:?}", study.name, man, entry, clear);
    let open = View::Inside(eye, man_w, 54.0);
    // Once he has fallen: the first of a few eyes above where he lies with a clear line to him,
    // then down along that line to 0.55 m.
    let overhead = |g: &Game| {
        let target = torso(g);
        let (up, side, ahead) = (frame.transform_vector3(Vec3::Y).normalize(), frame.transform_vector3(Vec3::X).normalize(),
            frame.transform_vector3(Vec3::Z).normalize());
        [up * 0.9, up * 0.8 + side * 0.3, up * 0.8 - side * 0.3, up * 0.7 + ahead * 0.35, up * 0.7 - ahead * 0.35]
            .into_iter().map(|o| target + o).find(|&e| sight_clear(g, e, target)).unwrap_or(eye)
    };
    let close_at = |g: &Game, from: Vec3, t: f32| {
        let target = torso(g);
        let ease = t * t * (3.0 - 2.0 * t);
        let near = target + (from - target).normalize_or_zero() * 0.55;
        View::Inside(from.lerp(near, ease), target, 56.0 - 8.0 * ease)
    };
    ctx.all_crew = true;
    ctx.banner = Some(study.label.to_uppercase());
    let scars_before = g.armor_scars.scars().len();
    let alive_before = CrewRole::ALL.iter().filter(|&&role| !g.crew_solo.man(role).dead).count();
    let mut film = if stills { None } else { Some(Clip::start(prefix, study.name)?) };
    let still = |ctx: &mut Ctx, g: &Game, r: &mut Renderer, view: View, tag: &str| -> anyhow::Result<()> {
        let rgba = ctx.shoot(g, r, view, None)?;
        let path = format!("{prefix}_{}_{tag}.png", study.name);
        save_still(&path, &rgba)?;
        println!("[fea-film] wrote {path}");
        Ok(())
    };
    ctx.cut(g, r, open)?;
    match film.as_mut() {
        Some(clip) => {
            clip.mark(study.label, "real time");
            clip.real_time(ctx, g, r, 1.0, idle_input, &|_, _| open)?;
        }
        None => still(ctx, g, r, open, "before")?,
    }
    g.debug_launch_round(study.shell, false, aim, dir, study.range, study.seed ^ 1);
    match film.as_mut() {
        Some(clip) => {
            clip.mark(study.label, "1/16 speed");
            clip.slow_motion(ctx, g, r, 96, &|_, _| open)?;
            // The strike's smoke fills the turret; the camera holds until it has cleared.
            clip.mark("after, while the smoke clears", "real time");
            clip.real_time(ctx, g, r, 10.0, idle_input, &|_, _| open)?;
        }
        None => {
            for _ in 0..96 {
                ctx.update(g, r, &TankInput::default(), SLOW_DT)?;
            }
            // The strike's smoke a second on, gathered at the hole.
            ctx.run(g, r, 0.8, TICK)?;
            still(ctx, g, r, open, "smoke")?;
            ctx.run(g, r, 9.2, TICK)?;
            still(ctx, g, r, open, "after")?;
            // The lights of that frame, strongest first, and how far each is from the camera.
            let mut lights: Vec<_> = r.offered_lights().to_vec();
            lights.sort_by(|a, b| b.radiant_intensity.total_cmp(&a.radiant_intensity));
            for light in lights.iter().take(8) {
                println!("[fea-film] light {:.4} W/sr-equivalent, colour {:.2?}, radius {:.3} m, {:.2} m from the camera",
                    light.radiant_intensity, light.color, light.source_radius, light.pos.distance(r.camera.pos));
            }
            println!("[fea-film] exposure {:?}", r.exposure_override);
        }
    }
    let dead = g.crew_solo.man(study.role).dead;
    match film.as_mut() {
        Some(clip) if dead => {
            let from = overhead(g);
            ctx.cut(g, r, close_at(g, from, 0.0))?;
            clip.mark("above where he lies, and down to him", "real time");
            clip.real_time(ctx, g, r, 5.0, idle_input, &|g, t| close_at(g, from, t))?;
            clip.mark("held", "real time");
            clip.real_time(ctx, g, r, 3.0, idle_input, &|g, _| close_at(g, from, 1.0))?;
        }
        None if dead => {
            let from = overhead(g);
            let view = close_at(g, from, 0.0);
            ctx.cut(g, r, view)?;
            still(ctx, g, r, view, "above")?;
            let view = close_at(g, from, 1.0);
            ctx.cut(g, r, view)?;
            still(ctx, g, r, view, "close")?;
            // An inspection orbit round the fallen man at 0.75 m, each eye with a clear line.
            let target = torso(g);
            let up = frame.transform_vector3(Vec3::Y).normalize();
            let side = frame.transform_vector3(Vec3::X).normalize();
            let ahead = frame.transform_vector3(Vec3::Z).normalize();
            for (n, azimuth) in [0.0f32, 90.0, 180.0, 270.0].into_iter().enumerate() {
                let a = azimuth.to_radians();
                let eye = target + (side * a.cos() + ahead * a.sin()) * 0.65 + up * 0.38;
                if !sight_clear(g, eye, target) {
                    println!("[fea-film] {}: orbit {azimuth} deg blocked", study.name);
                    continue;
                }
                let view = View::Inside(eye, target, 50.0);
                ctx.cut(g, r, view)?;
                still(ctx, g, r, view, &format!("orbit{n}"))?;
            }
            // His head (and his helmet, a rigid body), from the first of sixteen eyes about it with
            // a clear line, about a metre off.
            if let Some(head) = head_point(g, study.role) {
                let eyes = (0..16).map(|k| {
                    let a = (k as f32 * 22.5).to_radians();
                    let (reach, rise) = if k % 2 == 0 { (0.85, 0.45) } else { (0.6, 0.7) };
                    head + (side * a.cos() + ahead * a.sin()) * reach + up * rise
                });
                if let Some(eye) = eyes.clone().find(|&e| sight_clear(g, e, head)) {
                    let view = View::Inside(eye, head, 40.0);
                    ctx.cut(g, r, view)?;
                    still(ctx, g, r, view, "head")?;
                } else {
                    println!("[fea-film] {}: no clear line to his head", study.name);
                }
            }
        }
        _ => println!("[fea-film] {}: the {:?} was not killed; no close view", study.name, study.role),
    }
    ctx.banner = None;
    ctx.all_crew = false;
    ctx.clock = day_clock;
    let alive_after = CrewRole::ALL.iter().filter(|&&role| !g.crew_solo.man(role).dead).count();
    let mut receipt = match film {
        Some(clip) => clip.finish()?,
        None => json!({"clip": study.name, "stills": true}),
    };
    receipt["outcome"] = strike_outcome(g);
    receipt["scar"] = json!(strike_scar(g, scars_before));
    receipt["killed"] = json!({"target": format!("{:?}", study.role), "target_dead": dead, "alive_before": alive_before, "alive_after": alive_after});
    receipt["corpses"] = g.crew_corpse_render_receipt();
    Ok(Some(receipt))
}

/// A fresh Tiger for the next strike: the ordinary match reset (whole, its racks loaded, the
/// living crew at their posts when it is on), then the film's scene as at the start: enemies
/// banished, the Tiger at the review spot, the cabin lamps as asked.
fn fresh_tiger(ctx: &mut Ctx, g: &mut Game, r: &mut Renderer, seed: u32, loaded: bool) -> anyhow::Result<()> {
    g.reset_match(seed, 0.0);
    // The match reset leaves the player's own body as it was: whole again, not burning.
    g.dbg_reset_foot();
    g.player_burn.extinguish();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.god = false;
    g.debug_place_tiger(review::TIGER_AT, 0.0);
    if !loaded {
        g.debug_empty_ammunition();
    }
    ctx.run(g, r, 1.0, TICK)
}

/// What a strike did to the Tiger, its crew and its fire, for receipts and the probe.
/// The strike's own scar: the first recorded after `before` scars (later ones are its fragments').
fn strike_scar(g: &Game, before: usize) -> Option<String> {
    g.armor_scars.scars().get(before).map(|s| format!("{:?} {:?} at {:?} mm, aperture {} mm, radius {} mm",
        s.facing, s.kind, s.local_mm, s.aperture_mm, s.radius_mm))
}

fn strike_outcome(g: &Game) -> serde_json::Value {
    let chain = g.fea.receipts.last().map(|c| json!({"shell": c.shell, "terminal": format!("{:?}", c.terminal),
        "layers": c.result.summary(),
        "knocked_out": c.knocked_out.iter().map(|k| k.name()).collect::<Vec<_>>(),
        "struck_not_breached": c.struck_not_breached.clone()}));
    let alive = CrewRole::ALL.iter().filter(|&&role| super::singleplayer_crew::role_alive(&g.crew, role)
        && !g.crew_solo.man(role).dead).count();
    json!({"chain": chain, "tiger_alive": g.player.alive, "death": g.death_cause.clone().map(|d| d.de()),
        "fire_active": g.fire_active, "fire": g.fire, "ammo_burning": g.ammo_fire.burning_round().is_some(),
        "crew_alive": alive, "modules_out": g.modules.iter().filter(|m| !m.functional).map(|m| m.kind.name()).collect::<Vec<_>>()})
}

/// `KESSEL_FEA_FILM_PROBE=1`: fire each candidate engine-bay line at a fresh, loaded Tiger with
/// its crew aboard, unfilmed, and print what followed at 1 s and 20 s. Chooses nothing.
fn probe_engine_lines(ctx: &mut Ctx, g: &mut Game, r: &mut Renderer) -> anyhow::Result<()> {
    use crate::ballistics as b;
    let lines: [(&str, &'static b::ShellSpec, f32, Vec3, Vec3); 8] = [
        ("152 BR-540 left side at the fuel tank", &b::BR_540, 500.0, Vec3::new(1.148, 1.60, -1.6), Vec3::new(-1.0, -0.03, 0.0)),
        ("152 BR-540 rear plate at the engine", &b::BR_540, 300.0, Vec3::new(0.25, 1.50, -2.6), Vec3::new(0.0, -0.03, 1.0)),
        ("122 BR-471 rear, left side compartment", &b::BR_471, 500.0, Vec3::new(1.15, 1.60, -2.4), Vec3::new(0.0, -0.02, 1.0)),
        ("85 BR-365 rear, left side compartment", &b::BR_365, 300.0, Vec3::new(1.15, 1.60, -2.4), Vec3::new(0.0, -0.02, 1.0)),
        ("122 BR-471 left rear quarter at the fuel tank", &b::BR_471, 500.0, Vec3::new(1.10, 1.60, -1.5), Vec3::new(-0.5, -0.02, 0.866)),
        ("100 BR-412 left side at the fuel tank", &b::BR_412, 500.0, Vec3::new(1.148, 1.60, -1.6), Vec3::new(-1.0, -0.03, 0.0)),
        ("152 BR-540 left side at the engine bay's rear", &b::BR_540, 500.0, Vec3::new(1.148, 1.60, -2.4), Vec3::new(-1.0, -0.03, 0.0)),
        ("76 OF-350 HE on the engine deck", &b::OF_350_F34, 300.0, Vec3::new(0.0, 1.9, -2.0), Vec3::new(0.0, -0.75, 0.66)),
    ];
    for (i, (label, shell, range, aim, dir)) in lines.into_iter().enumerate() {
        fresh_tiger(ctx, g, r, 0x5EC7_0000 + i as u32, true)?;
        g.jump_station(Station::Commander);
        ctx.run(g, r, 0.5, TICK)?;
        let hull = g.player.hull_model();
        println!("[fea-film probe] {label}");
        g.debug_launch_round(shell, false, hull.transform_point3(aim), hull.transform_vector3(dir.normalize()), range, 0x5EC7_1000 + i as u32);
        ctx.run(g, r, 1.0, TICK)?;
        println!("[fea-film probe] {label} at 1 s: {}", strike_outcome(g));
        ctx.run(g, r, 19.0, TICK)?;
        println!("[fea-film probe] {label} at 20 s: {}", strike_outcome(g));
    }
    Ok(())
}

/// Where a penetration's solved chips are: how many fly, rest loose or rest held by the steel,
/// and the nearest to the loader's eye (hull frame), for the probe and receipts.
fn chip_summary(g: &Game) -> serde_json::Value {
    let eye = g.station_eye(Station::Loader);
    let hull_inv = g.player.hull_model().inverse();
    let mut rows: Vec<(f32, serde_json::Value)> = g.hot_fragments.iter().enumerate()
        .filter(|(_, f)| f.active && f.temp_k > 0.0)
        .map(|(i, f)| {
            let p = chip_at(g, i);
            (p.distance(eye), json!({"slot": i, "hull_m": hull_inv.transform_point3(p).to_array(), "settled": f.settled,
                "lodged": f.lodged, "embed_mm": f.embed_m * 1000.0, "size_mm": f.size * 1000.0, "k": f.temp_k,
                "from_loader_eye_m": p.distance(eye)}))
        }).collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let flying = g.hot_fragments.iter().filter(|f| f.active && f.temp_k > 0.0 && !f.settled).count();
    let lodged = g.hot_fragments.iter().filter(|f| f.active && f.temp_k > 0.0 && f.lodged).count();
    json!({"solved_chips": rows.len(), "flying": flying, "lodged": lodged,
        "within_1_6_m": rows.iter().filter(|r| r.0 < 1.6).count(), "within_2_3_m": rows.iter().filter(|r| r.0 < 2.3).count(),
        "nearest": rows.into_iter().take(8).map(|r| r.1).collect::<Vec<_>>()})
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let prefix_owned = super::game_fea_section::shared_capture_prefix(prefix);
    let prefix = prefix_owned.as_str();
    anyhow::ensure!(graphics::raymarch_backend_enabled(), "fea-film requires KESSEL_RENDERER=raymarch");
    anyhow::ensure!(!std::path::Path::new(&format!("{prefix}_receipt.json")).exists(),
        "preserving an existing film; choose a new prefix");
    let started = std::time::Instant::now();
    act_manifest::select_runtime_manifest(Some(crate::canyon::manifest_path()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    let _ = FRAME.set((r.iw, r.ih));
    println!("[fea-film] frames at the renderer's grid {}x{}, written scaled x{} by nearest neighbour", r.iw, r.ih, film_scale());
    let clock = ephemeris::UtcInstant::parse_iso("1942-12-21T10:30:00Z")?;
    let mut ctx = Ctx { device: &device, queue: &queue, audio_debt: 0.0, clock, all_crew: false, banner: None,
        last_camera: None };
    let idle_input = |_: u32| TankInput::default();
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    g.targets_only = true;
    g.god = false;
    g.debug_place_tiger(review::TIGER_AT, 0.0);
    g.debug_empty_ammunition();
    r.set_weather_override(Some(0.0));
    r.set_sky_clock(celestial_sky::SkyClock::new(clock, r.sky_clock.site));
    g.begin_audio_capture();
    ctx.run(&mut g, &mut r, 1.0, TICK)?;
    if std::env::var("KESSEL_FEA_FILM_PROBE").as_deref() == Ok("1") {
        g.enable_singleplayer_crew();
        return probe_engine_lines(&mut ctx, &mut g, &mut r);
    }
    // KESSEL_FEA_FILM_PROBE=chips: the turret_spall strike unfilmed, and where its chips went.
    if std::env::var("KESSEL_FEA_FILM_PROBE").as_deref() == Ok("chips") {
        g.station = Station::Driver;
        g.switch_t = 0.0;
        ctx.run(&mut g, &mut r, 0.2, TICK)?;
        let turret = g.player.turret_model();
        // KESSEL_FEA_FILM_CHIP_LINE="x,y,z,dx,dy,dz" (turret frame) tries another line.
        let line: Vec<f32> = std::env::var("KESSEL_FEA_FILM_CHIP_LINE").ok()
            .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect()).unwrap_or_default();
        let (aim, dir) = if line.len() == 6 { (Vec3::new(line[0], line[1], line[2]), Vec3::new(line[3], line[4], line[5])) }
            else { (Vec3::new(0.45, 1.85, -1.9), Vec3::new(-0.15, -0.30, 1.0)) };
        g.debug_launch_round(&crate::ballistics::BR_354P, false, turret.transform_point3(aim),
            turret.transform_vector3(dir.normalize()), 200.0, 0x5EA1_0002);
        for seconds in [0.25f32, 1.0, 3.0] {
            ctx.run(&mut g, &mut r, if seconds == 0.25 { 0.25 } else if seconds == 1.0 { 0.75 } else { 2.0 }, TICK)?;
            println!("[fea-film probe] chips at {seconds} s: {}", chip_summary(&g));
        }
        // Can the bar reach the nearest held chips from the loader's place? (the repair job's rules)
        g.station = Station::Loader;
        g.switch_t = 0.0;
        ctx.run(&mut g, &mut r, 0.3, TICK)?;
        let eye = g.station_eye(Station::Loader);
        let loose = review::resting_solved(&g).into_iter().filter(|(i, p, _)| !g.hot_fragments[*i].lodged
            && p.distance(eye) < 2.2 && g.equipment_path_clear(eye, *p, 0.05)).count();
        println!("[fea-film probe] loose chips within 2.2 m in clear sight: {loose}");
        // Free the two nearest held chips as the bar would, let them fall and rest, then ask the
        // ordinary hover about every loose chip from the loader's eye.
        for (slot, _) in review::stuck_anywhere(&g).into_iter().take(2) {
            g.fea_free_fragment(slot);
        }
        ctx.run(&mut g, &mut r, 3.0, TICK)?;
        for (slot, point, kelvin) in review::resting_solved(&g).into_iter().filter(|(i, _, _)| !g.hot_fragments[*i].lodged) {
            let look = (point - eye).normalize_or_zero();
            let models = g.fire_surface_models();
            let blocker = g.cabin_fire.sweep(eye, point, &models).map(|h| models[h.frame].transform_point3(h.point));
            println!("[fea-film probe] loose chip {slot} at {:.0} K, {:.3} m: hover on a perfect line {:?}, first surface {:.3} m short of it",
                kelvin, eye.distance(point), g.fea_fragment_hover(eye, look).map(|h| (h.0, h.2)),
                blocker.map_or(0.0, |b| b.distance(point)));
        }
        for (slot, point) in review::stuck_anywhere(&g).into_iter().take(4) {
            let models = g.fire_surface_models();
            let blocker = g.cabin_fire.sweep(eye, point, &models).map(|h| models[h.frame].transform_point3(h.point));
            println!("[fea-film probe] held chip {slot}: {:.3} m from the eye, path clear {}, first surface {:?} ({:.3} m short of it)",
                eye.distance(point), g.equipment_path_clear(eye, point, 0.28), blocker.map(|b| b.to_array()),
                blocker.map_or(0.0, |b| b.distance(point)));
        }
        return Ok(());
    }
    let mut clips = Vec::new();
    // KESSEL_FEA_FILM_CLIPS=name,name films only those clips (the others' strikes are not fired;
    // `loader_hands` needs `turret_spall`'s chips and `sledge` needs `lodged_round`'s round).
    let only: Option<Vec<String>> = std::env::var("KESSEL_FEA_FILM_CLIPS").ok()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
    let wanted = |name: &str| only.as_ref().is_none_or(|o| o.iter().any(|w| w == name));

    // ── 1. lodged_round: the cross sections' front_lower_45ap, outside.
    if wanted("lodged_round") {
        let hull = g.player.hull_model();
        let plate = hull.transform_point3(Vec3::new(-0.04, 1.02, 3.0));
        let wide = View::Outside(hull.transform_point3(Vec3::new(-1.5, 1.45, 5.3)), plate, 38.0);
        let mut clip = Clip::start(prefix, "lodged_round")?;
        ctx.cut(&g, &mut r, wide)?;
        clip.mark("the Tiger's lower front plate", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 0.8, &idle_input, &|_, _| wide)?;
        g.debug_launch_round(&crate::ballistics::BR_240_M42, false, hull.transform_point3(Vec3::new(-0.04, 1.0, 2.0)),
            hull.transform_vector3(Vec3::new(0.0, -0.03, -1.0)), 100.0, 0x5EA1_0001);
        clip.mark("45 mm BR-240 strike", "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 64, &|_, _| wide)?;
        clip.mark("the round stays in the plate; its plate solve commits at 0.25 s", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.6, &idle_input, &|_, _| wide)?;
        if let Some(round) = g.fea.lodged.iter().find(|l| l.in_struck_plate).cloned() {
            let point = g.fea_lodged_point(round.id).unwrap_or(plate);
            let local = hull.inverse().transform_point3(point);
            let close = hull.transform_point3(local + Vec3::new(-0.32, 0.14, 0.7));
            let start = hull.transform_point3(Vec3::new(-1.5, 1.45, 5.3));
            clip.mark("closing on the lodged round", "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 2.6, &idle_input,
                &move |_, k| View::Outside(start.lerp(close, smoothstep(k)), plate.lerp(point, smoothstep(k)), 38.0 - 10.0 * smoothstep(k)))?;
            clip.real_time(&mut ctx, &mut g, &mut r, 1.2, &idle_input, &move |_, _| View::Outside(close, point, 28.0))?;
        }
        clips.push(clip.finish()?);
    }

    // ── 2. turret_spall: the 76 mm APCR in through the turret's right side (the review's line,
    // bare plate), watched from the commander's place across the turret, the chips flying toward
    // the eye (the player sits at the driver's station for the strike and lives).
    if wanted("turret_spall") {
        g.station = Station::Driver;
        g.switch_t = 0.0;
        ctx.run(&mut g, &mut r, 0.2, TICK)?;
        let turret = g.player.turret_model();
        let eye = g.station_eye(Station::Commander);
        let side = turret.transform_point3(Vec3::new(-1.10, 1.62, -0.53));
        let view_rear = View::Inside(eye, side, 64.0);
        let mut clip = Clip::start(prefix, "turret_spall")?;
        ctx.cut(&g, &mut r, view_rear)?;
        clip.mark("the turret's right side from the commander's place", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 0.6, &idle_input, &|_, _| view_rear)?;
        g.debug_launch_round(&crate::ballistics::BR_354P, false, turret.transform_point3(review::SPALL_AIM),
            turret.transform_vector3(review::SPALL_DIR.normalize()), 200.0, 0x5EA1_0002);
        clip.mark("76 mm BR-354P perforates; the lattice's chips fly", "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 112, &|_, _| view_rear)?;
        // Where the chips came to rest (or are coming to rest), for the camera.
        let centre = |g: &Game| {
            let chips: Vec<Vec3> = g.hot_fragments.iter().filter(|f| f.active && f.temp_k > 0.0)
                .map(|f| blood_parent_model(&g.player, f.frame).transform_point3(f.pos)).collect();
            if chips.is_empty() { None } else { Some(chips.iter().fold(Vec3::ZERO, |a, b| a + *b) / chips.len() as f32) }
        };
        clip.mark("the chips coming to rest", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.0, &idle_input, &|_, _| view_rear)?;
        // From the loader's place, close on the densest knot of chips (a narrow field of view).
        let eye = g.station_eye(Station::Loader);
        let (knot, radius) = chip_cluster(&g, eye, 2.2).unwrap_or((centre(&g).unwrap_or(side), 0.15));
        let fov = framing_fov(radius, knot.distance(eye));
        let close = View::Inside(eye, knot, fov);
        ctx.cut(&g, &mut r, close)?;
        clip.mark("the chips glowing where they lie", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.5, &idle_input, &|_, _| close)?;
        g.debug_extinguish_cabin();
        clip.mark("five minutes of cooling", "time-lapse x30");
        clip.time_lapse(&mut ctx, &mut g, &mut r, 10.0, 30.0, "TIME-LAPSE X30", &mut |g: &mut Game| g.debug_extinguish_cabin(),
            &move |_, _| close)?;
        clips.push(clip.finish()?);
        g.station = Station::Loader;
        g.switch_t = 0.0;
        ctx.run(&mut g, &mut r, 0.3, TICK)?;
    }

    // ── 3. loader_hands: pry a held chip loose, take one and throw it out of the open hatch, take
    // another and let it fall. The camera is the loader's eye; it follows the chip with a narrow
    // field of view (a 5 mm chip at arm's length is a few pixels in the full view).
    if wanted("loader_hands") {
        let mut clip = Clip::start(prefix, "loader_hands")?;
        let eye_of = |g: &Game| g.station_eye(Station::Loader);
        let stuck = review::stuck_in_reach(&g);
        let mut pried = Vec::new();
        if let Some(&slot) = stuck.first() {
            let point = g.fea_fragment_point(slot).expect("stuck chip");
            let close = move |g: &Game, _: f32| View::Inside(eye_of(g), point, framing_fov(0.05, point.distance(eye_of(g))));
            ctx.cut(&g, &mut r, close(&g, 0.0))?;
            g.carrying_track_kit = true; // disclosed: the toolbox put in hand
            clip.mark("the toolbox's bar on a chip the floor holds", "real time");
            turn_head(&mut clip, &mut ctx, &mut g, &mut r, point, 0.6, TankInput::default(), &close)?;
            let seconds = g.fea_fragment_job_seconds(slot);
            let mut held = 0.0;
            while held < seconds + 0.6 && g.hot_fragments[slot].lodged {
                g.loader_face_world(point);
                clip.real_time(&mut ctx, &mut g, &mut r, 1.0 / FPS as f32, &|_| review::press(), &close)?;
                held += 1.0 / FPS as f32;
            }
            clip.real_time(&mut ctx, &mut g, &mut r, 0.5, &idle_input, &close)?;
            pried.push(json!({"slot": slot, "freed": !g.hot_fragments[slot].lodged, "held_s": held}));
            g.carrying_track_kit = false; // disclosed: set down by state
            // Cooling of the freed chip to a gloved hand's limit is not filmed (a cut).
            let mut extra = 0.0f32;
            while extra < 300.0 && !game_fea::fragment_holdable(&g.hot_fragments[slot]) {
                ctx.run(&mut g, &mut r, 1.0, 1.0 / 30.0)?;
                extra += 1.0;
            }
        }
        // The chips the hand can take now, nearest first (the loader steps toward one he cannot
        // see from where he stands: the ordinary interior walk, disclosed placement).
        let eye = eye_of(&g);
        let mut takeable: Vec<(usize, Vec3)> = review::resting_solved(&g).into_iter()
            .filter(|(i, _, _)| game_fea::fragment_holdable(&g.hot_fragments[*i])
                && game_fea::fragment_pullable(&g.hot_fragments[*i]))
            .map(|(i, p, _)| (i, p)).collect();
        takeable.sort_by(|a, b| a.1.distance(eye).total_cmp(&b.1.distance(eye)));
        let hatch = g.player.turret_model().transform_point3(tiger_hatches::APERTURE_CENTERS[1] + Vec3::Y * 0.4);
        let mut thrown = None;
        for (slot, point) in takeable.clone() {
            // Follow this chip wherever it goes; once it has left the vehicle, the hatch.
            let track = move |g: &Game, fov: f32| {
                let at = if g.hot_fragments[slot].active { chip_at(g, slot) } else { hatch };
                View::Inside(eye_of(g), at, fov)
            };
            let on_floor = move |g: &Game, _: f32| track(g, framing_fov(0.05, point.distance(eye_of(g))));
            g.loader_face_world(point);
            ctx.run(&mut g, &mut r, 0.2, TICK)?;
            if g.hover != Some(Hotspot::Fragment(slot)) {
                review::step_toward(&mut g, point);
                g.loader_face_world(point);
                ctx.run(&mut g, &mut r, 0.2, TICK)?;
                if g.hover != Some(Hotspot::Fragment(slot)) {
                    continue;
                }
            }
            ctx.cut(&g, &mut r, on_floor(&g, 0.0))?;
            clip.mark("(cut) a cool chip, loose", "real time");
            turn_head(&mut clip, &mut ctx, &mut g, &mut r, point, 0.5, TankInput::default(), &on_floor)?;
            if g.hover != Some(Hotspot::Fragment(slot)) {
                continue;
            }
            clip.mark("taking the chip", "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.8, &|_| review::press(), &move |g, k| track(g, 18.0 + 12.0 * smoothstep(k)))?;
            if g.fea.carried_fragment != Some(slot) {
                continue;
            }
            g.roof_hatches.open[1] = true; // disclosed: the loader's lid opened by state
            clip.real_time(&mut ctx, &mut g, &mut r, 0.9, &|_| review::press(), &move |g, _| track(g, 30.0))?;
            // Lining the throw up with the open hatch: the game's own throw line, from the hand,
            // after each turn of the head.
            let mut aimed = false;
            clip.mark("lining the throw up with the open hatch", "real time");
            for target in review::throw_targets(&g).into_iter().take(8) {
                turn_head(&mut clip, &mut ctx, &mut g, &mut r, target, 0.45, review::press(), &move |g, _| track(g, 34.0))?;
                if review::throw_line_clear(&g, r.camera.forward()) {
                    aimed = true;
                    break;
                }
            }
            if aimed && g.throw_carried_casing(r.camera.forward()) {
                clip.mark("thrown out through the hatch", "1/16 speed");
                clip.slow_motion(&mut ctx, &mut g, &mut r, 72, &move |g, _| track(g, 40.0))?;
                thrown = Some((slot, !g.hot_fragments[slot].active));
                clip.real_time(&mut ctx, &mut g, &mut r, 1.0, &idle_input, &move |g, _| track(g, 40.0))?;
            }
            break;
        }
        // A second chip, let fall.
        for (slot, point) in takeable.into_iter().filter(|(slot, _)| thrown.is_none_or(|t| t.0 != *slot)) {
            if !g.hot_fragments[slot].active {
                continue;
            }
            let track = move |g: &Game, fov: f32| View::Inside(eye_of(g), chip_at(g, slot), fov);
            let on_floor = move |g: &Game, _: f32| track(g, framing_fov(0.05, point.distance(eye_of(g))));
            ctx.cut(&g, &mut r, on_floor(&g, 0.0))?;
            turn_head(&mut clip, &mut ctx, &mut g, &mut r, point, 0.5, TankInput::default(), &on_floor)?;
            if g.hover != Some(Hotspot::Fragment(slot)) {
                continue;
            }
            clip.mark("(cut) a second chip, taken and let fall", "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.7, &|_| review::press(), &move |g, k| track(g, 18.0 + 12.0 * smoothstep(k)))?;
            clip.real_time(&mut ctx, &mut g, &mut r, 1.6, &idle_input, &move |g, _| track(g, 30.0))?;
            break;
        }
        g.roof_hatches.open[1] = false;
        let mut receipt = clip.finish()?;
        receipt["pried"] = json!(pried);
        receipt["thrown_out"] = json!(thrown.map(|t| t.1));
        clips.push(receipt);
    }

    // ── 4. sledge: on foot in front of the Tiger, knock the lodged round out.
    if let Some(round) = g.fea.lodged.iter().find(|l| l.in_struck_plate).cloned().filter(|_| wanted("sledge")) {
        let point = g.fea_lodged_point(round.id).expect("lodged point");
        let model = g.player.hull_model();
        let local = model.inverse().transform_point3(point);
        let mut stand = model.transform_point3(Vec3::new(local.x - 0.25, 0.0, local.z + 1.15));
        stand.y = g.hf.height_at(stand.x, stand.z);
        g.mode = Mode::Foot;
        g.carrying_track_kit = true; // disclosed: the toolbox put in hand
        g.foot.pos = stand;
        let aim_foot = |g: &mut Game, r: &mut Renderer| {
            let eye = g.foot.eye();
            r.camera.pos = eye;
            r.camera.set_forward((point - eye).normalize_or_zero());
            g.foot.yaw = r.camera.yaw;
            g.foot.pitch = r.camera.pitch;
        };
        aim_foot(&mut g, &mut r);
        ctx.run(&mut g, &mut r, 0.3, TICK)?;
        let mut clip = Clip::start(prefix, "sledge")?;
        ctx.cut(&g, &mut r, View::Game)?;
        clip.mark("the sledge on the lodged round", "real time");
        let seconds = g.fea_lodged_job_seconds(round.id);
        let mut held = 0.0f32;
        let blow = TankInput { fire2: true, trigger: true, ..TankInput::default() };
        while held < seconds + 1.0 && g.fea.lodged.iter().any(|l| l.id == round.id) {
            aim_foot(&mut g, &mut r);
            clip.real_time(&mut ctx, &mut g, &mut r, 1.0 / FPS as f32, &|_| blow, &|_, _| View::Game)?;
            held += 1.0 / FPS as f32;
        }
        clip.mark("the round is out", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.2, &idle_input, &|_, _| View::Game)?;
        let crater = View::Outside(model.transform_point3(local + Vec3::new(-0.32, 0.14, 0.7)), point, 28.0);
        ctx.cut(&g, &mut r, crater)?;
        clip.mark("(cut) the crater it leaves", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.6, &idle_input, &|_, _| crater)?;
        let mut receipt = clip.finish()?;
        receipt["removed"] = json!(!g.fea.lodged.iter().any(|l| l.id == round.id));
        clips.push(receipt);
        g.carrying_track_kit = false;
        g.mode = Mode::Tank;
        g.station = Station::Driver;
        g.switch_t = 0.0;
        ctx.run(&mut g, &mut r, 0.3, TICK)?;
    }

    // ── 5. hole_cooling: a fresh hole through the turret's side, outside, as it cools.
    if wanted("hole_cooling") {
        g.station = Station::Driver;
        g.switch_t = 0.0;
        let turret = g.player.turret_model();
        let scars_before = g.armor_scars.scars().len();
        // Bare side plate on the right, a third of a metre forward of the nearest spare shoe (the
        // shoes are workshop parts outside the shell's contact; ray-cast on the drawn turret, the
        // plate is the first surface on this line).
        g.debug_launch_round(&crate::ballistics::BR_354P, false, turret.transform_point3(Vec3::new(0.0, 1.85, 0.45)),
            turret.transform_vector3(Vec3::new(1.0, -0.04, 0.0)), 200.0, 0x5EA1_0004);
        ctx.run(&mut g, &mut r, 4.0 / 60.0, TICK)?;
        let hole = g.armor_scars.scars().iter().skip(scars_before.saturating_sub(1)).copied()
            .filter(|s| s.kind == crate::armor_damage::ScarKind::Hole && s.is_turret())
            .max_by(|a, b| a.local_position().x.total_cmp(&b.local_position().x));
        if let Some(hole) = hole {
            let turret = g.player.turret_model();
            let centre = turret.transform_point3(hole.local_position());
            let normal = turret.transform_vector3(hole.local_normal()).normalize_or_zero();
            let near = View::Outside(clear_eye(&g, centre, normal, 0.6), centre, 30.0);
            let mut clip = Clip::start(prefix, "hole_cooling")?;
            ctx.cut(&g, &mut r, near)?;
            clip.mark("the fresh hole through the turret side, 70 ms on", "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 1.6, &idle_input, &|_, _| near)?;
            clip.mark("its rim and bore cooling", "time-lapse x10");
            clip.time_lapse(&mut ctx, &mut g, &mut r, 7.0, 10.0, "TIME-LAPSE X10", &mut |g: &mut Game| g.debug_extinguish_cabin(),
                &|_, _| near)?;
            clips.push(clip.finish()?);
        } else {
            println!("[fea-film] the turret-side round made no hole; hole_cooling skipped");
        }
    }

    // ── 6. crew_impact: the living crew at their posts (the ordinary solo crew, the cabin lamps
    // on), and a 76 mm APCR round through the side at the gunner, seen from the commander's
    // place under the cupola. The player sits at the driver's station (disclosed). Who is hurt,
    // and how, is the ordinary crew model's answer to the solved round and chips.
    if g.player.alive && wanted("crew_impact") {
        g.enable_singleplayer_crew();
        g.set_cabin_lights(true); // disclosed: the ordinary lamp switch on, as in the crew review
        g.jump_station(Station::Driver);
        ctx.run(&mut g, &mut r, 2.5, TICK)?;
        g.debug_extinguish_cabin();
        let before = g.debug_crew_solo_bodies();
        let torso = g.crew_solo_man_point(CrewRole::Gunner);
        let turret = g.player.turret_model();
        let local = turret.inverse().transform_point3(torso);
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        let dir = turret.transform_vector3(Vec3::new(-side, -0.03, 0.06).normalize());
        let eye = g.station_eye(Station::Commander);
        let view = View::Inside(eye, torso, 62.0);
        let mut clip = Clip::start(prefix, "crew_impact")?;
        ctx.cut(&g, &mut r, view)?;
        clip.mark("the gunner at his post, from the commander's place", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.4, &idle_input, &|_, _| view)?;
        g.debug_launch_round(&crate::ballistics::BR_354P, false, torso, dir, 200.0, 0x5EA1_0005);
        clip.mark("76 mm BR-354P through the side at the gunner", "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 96, &|_, _| view)?;
        clip.mark("after", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 4.0, &idle_input, &|_, _| view)?;
        let mut receipt = clip.finish()?;
        receipt["crew_before"] = before;
        receipt["crew_after"] = g.debug_crew_solo_bodies();
        clips.push(receipt);
        g.debug_extinguish_cabin();
    }

    // ── 7. exterior_impacts: three strikes watched from outside: a 76 mm round glancing off the
    // glacis, a 122 mm through the turret front, an 85 mm APHE through the hull front. The
    // player's station moves away from each strike between them (disclosed).
    if g.player.alive && wanted("exterior_impacts") {
        let hull = g.player.hull_model();
        let turret = g.player.turret_model();
        let strikes: [(&str, &'static crate::ballistics::ShellSpec, f32, Vec3, Vec3, Vec3, Vec3, f32, Station, u32); 3] = [
            ("76 mm BR-350B glances off the glacis", &crate::ballistics::BR_350B, 300.0,
                hull.transform_point3(Vec3::new(0.3, 1.47, 2.9)), hull.transform_vector3(Vec3::new(0.0, -0.06, -1.0)),
                hull.transform_point3(Vec3::new(-2.6, 2.4, 5.8)), hull.transform_point3(Vec3::new(0.2, 1.5, 2.9)), 34.0,
                Station::Driver, 0x5EA1_0006),
            ("122 mm BR-471 through the turret front", &crate::ballistics::BR_471, 1000.0,
                turret.transform_point3(Vec3::new(0.3, 2.3, 0.6)), turret.transform_vector3(Vec3::new(0.0, -0.02, -1.0)),
                hull.transform_point3(Vec3::new(3.4, 3.0, 7.0)), turret.transform_point3(Vec3::new(0.2, 2.2, 1.2)), 36.0,
                Station::Driver, 0x5EA1_0007),
            ("85 mm BR-365 APHE through the hull front", &crate::ballistics::BR_365, 300.0,
                hull.transform_point3(Vec3::new(-0.35, 1.75, 1.8)), hull.transform_vector3(Vec3::new(0.0, -0.02, -1.0)),
                hull.transform_point3(Vec3::new(-3.4, 2.2, 6.6)), hull.transform_point3(Vec3::new(-0.35, 1.7, 2.5)), 36.0,
                Station::Loader, 0x5EA1_0008),
        ];
        let mut clip = Clip::start(prefix, "exterior_impacts")?;
        let mut results = Vec::new();
        for (label, shell, range, aim, dir, eye, at, fov, station, seed) in strikes {
            if !g.player.alive {
                break;
            }
            g.jump_station(station);
            ctx.run(&mut g, &mut r, 0.6, TICK)?;
            let view = View::Outside(eye, at, fov);
            ctx.cut(&g, &mut r, view)?;
            clip.mark(label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.6, &idle_input, &|_, _| view)?;
            let scars_before = g.armor_scars.scars().len();
            g.debug_launch_round(shell, false, aim, dir, range, seed);
            clip.mark(label, "1/16 speed");
            clip.slow_motion(&mut ctx, &mut g, &mut r, 56, &|_, _| view)?;
            clip.mark(label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 1.8, &idle_input, &|_, _| view)?;
            results.push(json!({"strike": label, "scars_added": g.armor_scars.scars().len() as i64 - scars_before as i64,
                "newest": g.armor_scars.scars().last().map(|s| format!("{:?} {:?}", s.facing, s.kind)),
                "player_alive": g.player.alive}));
            g.debug_extinguish_cabin();
        }
        let mut receipt = clip.finish()?;
        receipt["strikes"] = json!(results);
        clips.push(receipt);
    }

    // ── 8. burst_152: a 152 mm BR-540 bursts in the turret; the lids hold. Watched from the
    // struck side, above and ahead of the round's path.
    if g.player.alive && wanted("burst_152") {
        g.jump_station(Station::Driver);
        ctx.run(&mut g, &mut r, 0.4, TICK)?;
        let turret = g.player.turret_model();
        let wide_eye = g.player.hull_model().transform_point3(Vec3::new(6.5, 3.6, 2.5));
        let target = turret.transform_point3(Vec3::new(0.0, 1.9, 0.0));
        let wide = View::Outside(wide_eye, target, 40.0);
        let mut clip = Clip::start(prefix, "burst_152")?;
        ctx.cut(&g, &mut r, wide)?;
        clip.mark("the turret, every lid shut", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 0.8, &idle_input, &|_, _| wide)?;
        let receipts_before = g.fea.receipts.len();
        g.debug_launch_round(&crate::ballistics::BR_540, false, turret.transform_point3(Vec3::new(0.0, 2.05, 0.0)),
            turret.transform_vector3(Vec3::new(-1.0, -0.03, 0.0)), 500.0, 0x5EA1_0003);
        clip.mark("152 mm BR-540 strike and burst inside", "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 64, &|_, _| wide)?;
        clip.mark("the lids hold (17 kN on a real-size lid, under the 20 kN latch)", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 3.2, &idle_input, &|_, _| wide)?;
        let mut receipt = clip.finish()?;
        receipt["compartment"] = json!(g.fea.receipts.iter().skip(receipts_before).filter_map(|c| c.compartment.clone()).last());
        receipt["lids"] = json!({"roof": g.roof_hatches.open, "cupola": g.hatch_open});
        clips.push(receipt);
    }

    // ── 9. engine_section: a 152 mm BR-540 through the left side into the fuel tank's compartment
    // of a fresh, loaded Tiger with its whole crew at their posts (the cabin lamps on), filmed in
    // a horizontal section under the roof and the engine deck. The section is the camera's only:
    // the sun cascades, ray queries, fire, fragments and crew all meet the whole closed tank, so
    // the opened view shows the closed tank's own light. Whatever follows is the game's answer.
    if wanted("engine_section") {
        // After dusk (18:30 at Stalingrad, UTC+3): the cut opens on the closed tank's own lamps,
        // which daylight on the snow round it would drown (the sky clock is held as for the rest).
        let day_clock = ctx.clock;
        ctx.clock = ephemeris::UtcInstant::parse_iso("1942-12-21T15:30:00Z")?;
        r.set_sky_clock(celestial_sky::SkyClock::new(ctx.clock, r.sky_clock.site));
        g.enable_singleplayer_crew();
        fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_2000, true)?;
        g.set_cabin_lights(true); // disclosed: the ordinary lamp switch on, as in the crew review
        g.jump_station(Station::Commander);
        ctx.run(&mut g, &mut r, 2.0, TICK)?;
        let hull = g.player.hull_model();
        // The eye's exposure metered once inside, on the lamp-lit fighting compartment, then held.
        let inside = View::Inside(g.station_eye(Station::Commander), hull.transform_point3(Vec3::new(0.0, 1.2, 1.2)), 70.0);
        ctx.aim(&g, &mut r, inside);
        r.reset_history();
        let metered = crate::converge_headless_screenshot(&mut r, &device, &queue, 16)?.measured_exposure;
        r.exposure_override = metered;
        // Everything of this Tiger above the section height (hull frame) is hidden from the camera.
        const SECTION_Y: f32 = 1.90;
        let up = hull.transform_vector3(Vec3::Y).normalize();
        let plane = up.extend(up.dot(hull.transform_point3(Vec3::new(0.0, SECTION_Y, 0.0))));
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for bits in 0..8u32 {
            let local = Vec3::new(if bits & 1 == 0 { -2.6 } else { 2.6 }, if bits & 2 == 0 { -0.4 } else { 4.0 },
                if bits & 4 == 0 { -4.8 } else { 8.5 });
            let p = hull.transform_point3(local);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let cut_meshes = r.solid_meshes_within(lo, hi);
        r.set_film_section(Some(plane), &cut_meshes);
        ctx.all_crew = true;
        ctx.banner = Some("SECTION 1.9 M ABOVE GROUND, FOR THE CAMERA ONLY: LIGHT AND PHYSICS SEE THE WHOLE TANK".into());
        // Five times the eye's metered exposure on the lamps (a film camera's choice, held).
        let held = metered.unwrap_or(1800.0) * 5.0;
        r.exposure_override = Some(held);
        let eye = hull.transform_point3(Vec3::new(-3.0, 6.2, 0.4));
        let at = hull.transform_point3(Vec3::new(0.25, 1.0, -0.2));
        let view = View::Inside(eye, at, 46.0);
        ctx.cut(&g, &mut r, view)?;
        // KESSEL_FEA_FILM_SECTION_STILL=1: the opening frame as a still, instead of the clip.
        if std::env::var("KESSEL_FEA_FILM_SECTION_STILL").as_deref() == Ok("1") {
            let rgba = ctx.shoot(&g, &mut r, view, None)?;
            let still = format!("{prefix}_engine_section_still.png");
            save_still(&still, &rgba)?;
            println!("[fea-film] wrote {still} ({} meshes cut, exposure {held:.0}; crew drawn {})", cut_meshes.len(),
                g.debug_crew_solo_visible_figures(&r));
            return Ok(());
        }
        let mut clip = Clip::start(prefix, "engine_section")?;
        clip.mark("the crew at their posts, the engine bay behind the firewall", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 2.0, &idle_input, &|_, _| view)?;
        let receipts_before = g.fea.receipts.len();
        g.debug_launch_round(&crate::ballistics::BR_540, false, hull.transform_point3(Vec3::new(1.148, 1.60, -1.6)),
            hull.transform_vector3(Vec3::new(-1.0, -0.03, 0.0).normalize()), 500.0, 0x5EC7_2001);
        clip.mark("152 mm BR-540 through the left side into the fuel tank's compartment", "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 24, &|_, _| view)?;
        // From the flash on, the exposure is re-metered at each segment and held through it.
        let mut exposures = vec![json!({"segment": "lamps", "exposure": held})];
        let e = ctx.meter(&g, &mut r, view)?;
        exposures.push(json!({"segment": "burst", "exposure": e}));
        clip.slow_motion(&mut ctx, &mut g, &mut r, 96, &|_, _| view)?;
        let at_strike = strike_outcome(&g);
        let e = ctx.meter(&g, &mut r, view)?;
        exposures.push(json!({"segment": "after the strike", "exposure": e}));
        clip.mark("after the strike", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 6.0, &idle_input, &|_, _| view)?;
        let e = ctx.meter(&g, &mut r, view)?;
        exposures.push(json!({"segment": "the next minute", "exposure": e}));
        clip.mark("the next minute", "time-lapse x10");
        clip.time_lapse(&mut ctx, &mut g, &mut r, 6.0, 10.0, "TIME-LAPSE X10", &mut |_: &mut Game| {}, &|_, _| view)?;
        let e = ctx.meter(&g, &mut r, view)?;
        exposures.push(json!({"segment": "a minute and a quarter on", "exposure": e}));
        clip.mark("a minute and a quarter on", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 2.0, &idle_input, &|_, _| view)?;
        let mut receipt = clip.finish()?;
        receipt["after_strike"] = at_strike;
        receipt["after_75_s"] = strike_outcome(&g);
        receipt["chain"] = json!(g.fea.receipts.iter().skip(receipts_before).map(|c| c.result.summary()).collect::<Vec<_>>());
        receipt["section"] = json!({"plane_world": plane.to_array(), "hull_y_m": SECTION_Y, "meshes_cut": cut_meshes.len(),
            "exposure_metered_inside": metered, "exposures": exposures, "clock_utc": "1942-12-21T15:30:00Z"});
        clips.push(receipt);
        r.set_film_section(None, &[]);
        r.exposure_override = None;
        ctx.all_crew = false;
        ctx.banner = None;
        ctx.clock = day_clock;
    }

    // ── 10. side_angles: an 85 mm BR-365 APHE at 500 m into the bare upper hull side (80 mm) at 0, 30,
    // 50 and 65 degrees, each on a fresh Tiger (racks loaded, crew aboard), watched from outside.
    // The aim point is bare plate: the source mesh's first crossing there is the side plate itself.
    if wanted("side_angles") {
        g.enable_singleplayer_crew();
        let mut clip = Clip::start(prefix, "side_angles")?;
        let mut results = Vec::new();
        for (i, angle) in [0.0f32, 30.0, 50.0, 65.0].into_iter().enumerate() {
            fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_3000 + i as u32, true)?;
            g.jump_station(Station::Commander);
            ctx.run(&mut g, &mut r, 0.4, TICK)?;
            let hull = g.player.hull_model();
            let a = angle.to_radians();
            let aim = hull.transform_point3(Vec3::new(1.75, 1.75, 1.6));
            let dir = hull.transform_vector3(Vec3::new(-a.cos(), -0.03, -a.sin()).normalize());
            let view = View::Outside(hull.transform_point3(Vec3::new(4.0, 2.4, 4.4)), aim, 26.0);
            let label = format!("85 mm BR-365 APHE, 500 m, upper hull side 80 mm at {angle:.0} deg");
            ctx.banner = Some(label.to_uppercase());
            ctx.cut(&g, &mut r, view)?;
            clip.mark(&label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.5, &idle_input, &|_, _| view)?;
            let receipts_before = g.fea.receipts.len();
            let scars_before = g.armor_scars.scars().len();
            g.debug_launch_round(&crate::ballistics::BR_365, false, aim, dir, 500.0, 0x5EC7_3100 + i as u32);
            clip.mark(&label, "1/16 speed");
            clip.slow_motion(&mut ctx, &mut g, &mut r, 48, &|_, _| view)?;
            clip.mark(&label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 1.4, &idle_input, &|_, _| view)?;
            results.push(json!({"angle_deg": angle, "outcome": strike_outcome(&g), "scar": strike_scar(&g, scars_before),
                "chain": g.fea.receipts.iter().skip(receipts_before).map(|c| c.result.summary()).collect::<Vec<_>>()}));
        }
        ctx.banner = None;
        let mut receipt = clip.finish()?;
        receipt["strikes"] = json!(results);
        clips.push(receipt);
    }

    // ── 11. turret_ring: a 76 mm BR-350B at 300 m from the left front quarter, 4.5 degrees down onto
    // the turret wall 3.5 cm above the hull roof, on a fresh Tiger. The roof's edges shadow the
    // ring band from most lines: ray-cast on the drawn meshes, this one clears the roof's edge and
    // its first surface is the turret wall inside the band; from the right a raised roof part is
    // first, and from the left a spare shoe; a steeper line meets the wall above the band.
    if wanted("turret_ring") {
        g.enable_singleplayer_crew();
        let mut clip = Clip::start(prefix, "turret_ring")?;
        let mut results = Vec::new();
        for (i, bearing) in [45.0f32].into_iter().enumerate() {
            fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_4000 + i as u32, true)?;
            g.jump_station(Station::Commander);
            ctx.run(&mut g, &mut r, 0.4, TICK)?;
            let hull = g.player.hull_model();
            let b = bearing.to_radians();
            let outward = Vec3::new(b.cos(), 0.0, b.sin());
            let plunge = 4.5f32.to_radians();
            let aim = hull.transform_point3(outward * 1.297 + Vec3::Y * 2.035);
            let dir = hull.transform_vector3((-outward * plunge.cos() - Vec3::Y * plunge.sin()).normalize());
            let view = View::Outside(hull.transform_point3(outward * 3.6 + Vec3::new(0.0, 1.7, 0.0)), aim, 24.0);
            let label = format!("76 mm BR-350B, 300 m, 4.5 deg down at the turret's base, {bearing:.0} deg off the left");
            ctx.banner = Some(label.to_uppercase());
            ctx.cut(&g, &mut r, view)?;
            clip.mark(&label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.5, &idle_input, &|_, _| view)?;
            let scars_before = g.armor_scars.scars().len();
            g.debug_launch_round(&crate::ballistics::BR_350B, false, aim, dir, 300.0, 0x5EC7_4100 + i as u32);
            clip.mark(&label, "1/16 speed");
            clip.slow_motion(&mut ctx, &mut g, &mut r, 48, &|_, _| view)?;
            clip.mark(&label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 1.6, &idle_input, &|_, _| view)?;
            results.push(json!({"bearing_deg": bearing, "outcome": strike_outcome(&g), "scar": strike_scar(&g, scars_before),
                "ring_jam": format!("{:?}", g.exterior.ring_jam)}));
        }
        ctx.banner = None;
        let mut receipt = clip.finish()?;
        receipt["strikes"] = json!(results);
        clips.push(receipt);
    }

    // ── 12. calibres: seven rounds from 45 to 152 mm, each on a fresh Tiger (racks loaded, crew
    // aboard), into the same bare spot of the upper hull side (80 mm, 0 degrees) over the left fuel
    // tank's compartment, watched close: the strike, then its hole.
    if wanted("calibres") {
        g.enable_singleplayer_crew();
        use crate::ballistics as b;
        let rounds: [(&str, &'static b::ShellSpec, f32); 7] = [
            ("45 mm BR-240P APCR, 100 m", &b::BR_240P, 100.0),
            ("57 mm BR-271 APHE, 500 m", &b::BR_271_ZIS2, 500.0),
            ("76 mm BR-354P APCR, 200 m", &b::BR_354P, 200.0),
            ("85 mm BR-365 APHE, 500 m", &b::BR_365, 500.0),
            ("100 mm BR-412 APHE, 500 m", &b::BR_412, 500.0),
            ("122 mm BR-471 APHE, 1000 m", &b::BR_471, 1000.0),
            ("152 mm BR-540 APHE, 500 m", &b::BR_540, 500.0),
        ];
        let mut clip = Clip::start(prefix, "calibres")?;
        let mut results = Vec::new();
        for (i, (label, shell, range)) in rounds.into_iter().enumerate() {
            fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_5000 + i as u32, true)?;
            g.jump_station(Station::Commander);
            ctx.run(&mut g, &mut r, 0.4, TICK)?;
            let hull = g.player.hull_model();
            let aim = hull.transform_point3(Vec3::new(1.75, 1.75, -1.6));
            let view = View::Outside(hull.transform_point3(Vec3::new(3.3, 2.05, -0.9)), aim, 20.0);
            ctx.banner = Some(format!("{label}, upper hull side 80 mm at 0 deg").to_uppercase());
            ctx.cut(&g, &mut r, view)?;
            clip.mark(label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 0.3, &idle_input, &|_, _| view)?;
            let receipts_before = g.fea.receipts.len();
            let scars_before = g.armor_scars.scars().len();
            g.debug_launch_round(shell, false, aim, hull.transform_vector3(Vec3::new(-1.0, -0.03, 0.0).normalize()), range,
                0x5EC7_5100 + i as u32);
            clip.mark(label, "1/16 speed");
            clip.slow_motion(&mut ctx, &mut g, &mut r, 32, &|_, _| view)?;
            clip.mark(label, "real time");
            clip.real_time(&mut ctx, &mut g, &mut r, 1.2, &idle_input, &|_, _| view)?;
            results.push(json!({"round": label, "outcome": strike_outcome(&g), "scar": strike_scar(&g, scars_before),
                "chain": g.fea.receipts.iter().skip(receipts_before).map(|c| c.result.summary()).collect::<Vec<_>>()}));
        }
        ctx.banner = None;
        let mut receipt = clip.finish()?;
        receipt["strikes"] = json!(results);
        clips.push(receipt);
    }

    // ── 13. side_rear_100: a 100 mm BR-412 APHE at 500 m into the bare right hull side over the
    // engine bay, a fresh Tiger, watched low from the right rear quarter; then twenty seconds on
    // the wound as its glow fades (the owner's linear glow, `renderer_scars::WOUND_GLOW_S`).
    if wanted("side_rear_100") {
        g.enable_singleplayer_crew();
        fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_6000, true)?;
        g.jump_station(Station::Commander);
        ctx.run(&mut g, &mut r, 0.4, TICK)?;
        let hull = g.player.hull_model();
        let aim = hull.transform_point3(Vec3::new(-1.75, 1.55, -2.0));
        let wide = View::Outside(hull.transform_point3(Vec3::new(-4.6, 0.9, -5.2)), aim, 30.0);
        let close = View::Outside(hull.transform_point3(Vec3::new(-2.55, 1.62, -1.55)), aim, 30.0);
        let label = "100 mm BR-412 APHE, 500 m, right hull side 80 mm over the engine bay";
        ctx.banner = Some(label.to_uppercase());
        let mut clip = Clip::start(prefix, "side_rear_100")?;
        ctx.cut(&g, &mut r, wide)?;
        clip.mark(label, "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 0.6, &idle_input, &|_, _| wide)?;
        let scars_before = g.armor_scars.scars().len();
        g.debug_launch_round(&crate::ballistics::BR_412, false, aim,
            hull.transform_vector3(Vec3::new(1.0, -0.03, 0.0).normalize()), 500.0, 0x5EC7_6001);
        clip.mark(label, "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 48, &|_, _| wide)?;
        ctx.cut(&g, &mut r, close)?;
        clip.mark("the wound, twenty seconds", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 20.0, &idle_input, &|_, _| close)?;
        ctx.banner = None;
        let mut receipt = clip.finish()?;
        receipt["outcome"] = strike_outcome(&g);
        receipt["scar"] = json!(strike_scar(&g, scars_before));
        clips.push(receipt);
    }

    // ── 14. turret_inside_85: an 85 mm BR-365 APHE at 400 m through the bare right turret side,
    // a fresh Tiger at noon with its crew at their posts and the lamps on, watched from inside at
    // the gunner's place across the breech to the loader's wall.
    if wanted("turret_inside_85") {
        let day_clock = ctx.clock;
        ctx.clock = ephemeris::UtcInstant::parse_iso("1942-12-21T09:00:00Z")?; // noon at Stalingrad (UTC+3)
        g.enable_singleplayer_crew();
        fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_7000, true)?;
        g.set_cabin_lights(true); // disclosed: the ordinary lamp switch on
        g.jump_station(Station::Commander);
        ctx.run(&mut g, &mut r, 1.0, TICK)?;
        let turret = g.player.turret_model();
        let side = turret.transform_point3(Vec3::new(-1.19, 1.74, 0.45));
        let view = View::Inside(g.station_eye(Station::Gunner) + turret.transform_vector3(Vec3::new(0.0, 0.12, -0.25)), side, 66.0);
        ctx.all_crew = true;
        let label = "85 mm BR-365 APHE, 400 m, through the right turret side, from the gunner's place";
        ctx.banner = Some(label.to_uppercase());
        let mut clip = Clip::start(prefix, "turret_inside_85")?;
        ctx.cut(&g, &mut r, view)?;
        clip.mark(label, "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.2, &idle_input, &|_, _| view)?;
        let scars_before = g.armor_scars.scars().len();
        g.debug_launch_round(&crate::ballistics::BR_365, false, turret.transform_point3(Vec3::new(0.0, 1.85, 0.45)),
            turret.transform_vector3(Vec3::new(1.0, -0.04, 0.0)), 400.0, 0x5EC7_7001);
        clip.mark(label, "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 96, &|_, _| view)?;
        clip.mark("after", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 5.0, &idle_input, &|_, _| view)?;
        ctx.banner = None;
        ctx.all_crew = false;
        let mut receipt = clip.finish()?;
        receipt["outcome"] = strike_outcome(&g);
        receipt["scar"] = json!(strike_scar(&g, scars_before));
        receipt["crew_after"] = g.debug_crew_solo_bodies();
        clips.push(receipt);
        ctx.clock = day_clock;
    }

    // ── 15. distant_152: a 152 mm BR-540 APHE at 500 m into the bare right hull side behind the
    // fighting compartment, a fresh Tiger, watched from 40 m off across the snow.
    if wanted("distant_152") {
        g.enable_singleplayer_crew();
        fresh_tiger(&mut ctx, &mut g, &mut r, 0x5EC7_8000, true)?;
        g.jump_station(Station::Commander);
        ctx.run(&mut g, &mut r, 0.4, TICK)?;
        let hull = g.player.hull_model();
        let aim = hull.transform_point3(Vec3::new(-1.75, 1.55, -1.2));
        let far = View::Outside(hull.transform_point3(Vec3::new(-34.0, 3.2, -20.0)), hull.transform_point3(Vec3::new(0.0, 1.4, -0.6)), 16.0);
        let label = "152 mm BR-540 APHE, 500 m, right hull side, from 40 m";
        ctx.banner = Some(label.to_uppercase());
        let mut clip = Clip::start(prefix, "distant_152")?;
        ctx.cut(&g, &mut r, far)?;
        clip.mark(label, "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 1.0, &idle_input, &|_, _| far)?;
        let scars_before = g.armor_scars.scars().len();
        g.debug_launch_round(&crate::ballistics::BR_540, false, aim,
            hull.transform_vector3(Vec3::new(1.0, -0.03, 0.0).normalize()), 500.0, 0x5EC7_8001);
        clip.mark(label, "1/16 speed");
        clip.slow_motion(&mut ctx, &mut g, &mut r, 64, &|_, _| far)?;
        clip.mark("after", "real time");
        clip.real_time(&mut ctx, &mut g, &mut r, 6.0, &idle_input, &|_, _| far)?;
        ctx.banner = None;
        let mut receipt = clip.finish()?;
        receipt["outcome"] = strike_outcome(&g);
        receipt["scar"] = json!(strike_scar(&g, scars_before));
        clips.push(receipt);
    }

    // ── 16. Casualty study (`crew_tissue`): one round kills one man; the camera keeps a clear line
    // to him through the tank's own surfaces, then moves in once he has fallen. The racks are
    // emptied first (disclosed on the frame): with them loaded this round reaches the ammunition
    // and the detonation, not the round, is what kills the crew.
    let studies = [
        CasualtyStudy {
            name: "tissue_loader", clock: "1942-12-21T09:00:00Z", seed: 0x5EC7_9000, role: CrewRole::Loader,
            turret: true, shell: &crate::ballistics::BR_271_ZIS2, range: 400.0, player: Station::Driver,
            loader_lid: true,
            label: "57 mm BR-271, 400 m, through the right turret side at the loader; noon, his lid open, racks emptied",
            // The bare right turret side ahead of the spare shoes (z above 0.27), at his chest.
            entry: |man| Vec3::new(-1.19, man.y + 0.04, 0.45),
            // On his side of the gun, ahead of him and behind him (the gunner and commander sit on
            // the other side, so no living man stands in the line).
            eyes: |man| vec![Vec3::new(-0.78, man.y + 0.38, 0.78), Vec3::new(-0.62, man.y + 0.45, 0.95),
                Vec3::new(-0.72, man.y + 0.42, -0.62), Vec3::new(-0.35, man.y + 0.5, -0.9)],
        },
    ];
    for study in &studies {
        if wanted(study.name) {
            if let Some(receipt) = casualty_study(&mut ctx, &mut g, &mut r, prefix, study, &idle_input)? {
                clips.push(receipt);
            }
        }
    }

    // ── 17. The cook-off (`game_cookoff`, `cabin_heat`): a fire beside the left sponson's rounds
    // until a case reaches its propellant's ignition, or a 122 mm round into that rack; watched
    // from outside, wide enough for the turret's whole throw, then the open hull from above.
    for (name, struck) in [("cookoff_fire", false), ("cookoff_struck", true)] {
        if wanted(name) {
            clips.push(cookoff_study(&mut ctx, &mut g, &mut r, prefix, name, struck, &idle_input)?);
        }
    }

    let receipt = json!({
        "clips": clips,
        "film": {"grid": [frame().0, frame().1], "scaled": film_scale(), "fps": FPS, "slow_motion": "1/16 speed: one 1/480 s fragment step per frame, silent",
            "audio": "real-time stretches: the ordinary gameplay audio sinks recorded offline on the simulation clock; slow motion and time-lapse silent"},
        "disclosed": "station and on-foot placement set directly; toolbox put in hand and set down by state; the loader's lid opened by state; cabin fires put out between clips; the sky clock held at 10:30; head turns are interpolated head yaw and pitch",
        "wall_s": started.elapsed().as_secs_f32(),
    });
    std::fs::write(format!("{prefix}_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    println!("[fea-film] {} clips in {:.0} s", receipt["clips"].as_array().map_or(0, |c| c.len()), started.elapsed().as_secs_f32());
    Ok(())
}
