//! `meadow-film PREFIX`: short review clips of the spring meadow (meadow.rs, meadow_fauna.rs) on
//! the map named by `KESSEL_RECON_MANIFEST` (KESSEL_RENDERER=raymarch): by day a Tiger driving
//! through the grass in the wind with the butterflies about, and at dusk the firefly swarms.
//! Silent clips (no audio track), 1920x1080 at 30 fps, the picture piped to ffmpeg. The camera
//! and the Tiger's start are `KESSEL_MEADOW_FILM_*` (defaults: the spring bridge map's start).
use std::io::Write;
use std::process::{Command, Stdio};

use glam::{Vec2, Vec3};

use crate::game::Game;
use crate::{act_manifest, celestial_sky, ephemeris, world, TankInput};

const W: u32 = 1920;
const H: u32 = 1080;
const FPS: u32 = 30;

fn vec3_env(name: &str, default: Vec3) -> Vec3 {
    std::env::var(name).ok().map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>())
        .filter(|p| p.len() == 3).map_or(default, |p| Vec3::new(p[0], p[1], p[2]))
}

/// One clip: `seconds` of film from `eye` (above the ground) looking at `look` (or at the Tiger),
/// the Tiger given `input` every simulation step.
#[allow(clippy::too_many_arguments)]
fn clip(path: &str, g: &mut Game, r: &mut crate::renderer::Renderer, device: &wgpu::Device, queue: &wgpu::Queue,
    utc: &str, eye: Vec3, look: Option<Vec3>, input: TankInput, seconds: f32) -> anyhow::Result<()> {
    let instant = ephemeris::UtcInstant::parse_iso(utc)?;
    r.set_sky_clock(celestial_sky::SkyClock::new(instant, r.sky_clock.site));
    let ground = |g: &Game, p: Vec3| g.hf.height_at(p.x, p.z);
    let eye = Vec3::new(eye.x, ground(g, eye) + eye.y, eye.z);
    let aim = |g: &Game, r: &mut crate::renderer::Renderer| {
        let at = look.map_or(g.player.pos + Vec3::Y * 1.6, |l| Vec3::new(l.x, ground(g, l) + l.y, l.z));
        r.camera.pos = eye;
        r.camera.set_forward(at - eye);
        r.camera.fov_y = 55f32.to_radians();
        r.reticle = false;
        r.optic_mask = 0.0;
        r.interior_light = 0.0;
        r.interior_haze = 0.0;
        r.weather_shelter = 0.0;
        r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
    };
    // Settle: a second of play, then the exposure and the transport on the first view.
    for _ in 0..60 { g.update(&TankInput::default(), 1.0 / 60.0, r); }
    aim(g, r);
    g.prepare_world_survey(r, 1.0 / 60.0);
    aim(g, r);
    r.reset_history();
    crate::converge_headless_screenshot(r, device, queue, 16)?;
    let mut encoder = Command::new("nice")
        .args(["-n", "10", "ffmpeg", "-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pixel_format", "rgba",
            "-video_size", &format!("{W}x{H}"), "-framerate", &FPS.to_string(), "-i", "pipe:0", "-an", "-c:v", "libx264",
            "-preset", "medium", "-crf", "17", "-pix_fmt", "yuv420p", "-movflags", "+faststart", path])
        .stdin(Stdio::piped())
        .spawn()?;
    let frames = (seconds * FPS as f32).round() as u32;
    for frame in 0..frames {
        for _ in 0..(60 / FPS) { g.update(&input, 1.0 / 60.0, r); }
        aim(g, r);
        let rgba = r.render_to_rgba(device, queue, W, H)?;
        encoder.stdin.as_mut().expect("encoder input").write_all(&rgba)?;
        if frame % FPS == 0 { println!("[meadow-film] {path}: {:.0} s, {}", frame / FPS, g.world_survey_grass_stats()); }
    }
    drop(encoder.stdin.take());
    anyhow::ensure!(encoder.wait()?.success(), "ffmpeg failed on {path}");
    println!("[meadow-film] wrote {path}");
    Ok(())
}

pub fn run(prefix: &str) -> anyhow::Result<()> {
    anyhow::ensure!(crate::graphics::raymarch_backend_enabled(), "meadow-film requires KESSEL_RENDERER=raymarch");
    let manifest = std::env::var_os("KESSEL_RECON_MANIFEST").ok_or_else(|| anyhow::anyhow!("meadow-film needs KESSEL_RECON_MANIFEST"))?;
    let act = act_manifest::select_runtime_manifest(Some(std::path::PathBuf::from(manifest)))?
        .ok_or_else(|| anyhow::anyhow!("the manifest did not select a campaign act"))?;
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(act))?;
    g.debug_banish_enemies();
    g.debug_hold_enemy_fire();
    let tiger = vec3_env("KESSEL_MEADOW_FILM_TIGER", Vec3::new(-640.0, 90.0, -99.0));
    g.debug_place_tiger(Vec2::new(tiger.x, tiger.z), tiger.y.to_radians());
    let drive = TankInput { drive: 1.0, ..Default::default() };
    // By day: the Tiger drives past a man standing in the meadow.
    let day_eye = vec3_env("KESSEL_MEADOW_FILM_DAY_EYE", Vec3::new(-622.0, 1.65, -107.0));
    clip(&format!("{prefix}_day.mp4"), &mut g, &mut r, &device, &queue, "1942-05-20T06:30:00Z", day_eye, None, drive, 10.0)?;
    // At dusk: the swarms over the grass, the Tiger standing.
    let dusk_eye = vec3_env("KESSEL_MEADOW_FILM_DUSK_EYE", Vec3::new(-665.0, 1.6, -95.0));
    let dusk_look = vec3_env("KESSEL_MEADOW_FILM_DUSK_LOOK", Vec3::new(-640.0, 1.2, -70.0));
    clip(&format!("{prefix}_dusk.mp4"), &mut g, &mut r, &device, &queue, "1942-05-20T18:10:00Z", dusk_eye, Some(dusk_look),
        TankInput::default(), 14.0)?;
    Ok(())
}
