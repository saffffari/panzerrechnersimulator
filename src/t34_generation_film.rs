//! `t34-gen-film PREFIX SEED SECONDS WEIGHTS [CAPTION]`: one training episode of a T-34 policy as a
//! film, so the generations of `tools/t34_canyon_es.py` can be watched learning. The episode is
//! fielded exactly as `t34-canyon-eval` fields it (the Canyon's opening deployment for SEED, the
//! policy over the doctrine's orders, the rule-based Canyon pilot taking the Tiger along its
//! route for SECONDS), from an action-following aerial camera: the Tiger and the living crews
//! within 900 m seen from behind them at 70-140 m, the Tiger ahead.
//!
//! The picture is the game's own pixel grid (the internal resolution; the locked 1024x576) and
//! nothing finer: frames are read back at native size and ffmpeg repeats every pixel as one whole
//! block, `2880 / height` times (5x of 1024x576 is the owner's 5120x2880 panel; "standard
//! resolution"). The soundtrack is the game's own mix at the Tiger. One policy and one seed per
//! process, as the trainer runs them (`t34_canyon_training`: later episodes in one process drift),
//! so every generation's film starts from the same world. `tools/t34_generation_films.sh` runs one
//! per generation and joins them.
use crate::{acoustics, act_manifest, game::{CanyonPilot, Game, TankInput}, graphics, ui::Align, world};
use glam::Vec3;
use std::io::Write;

const FPS: u32 = 30;
const LINGER_S: f32 = 2.5;

/// The whole factor that brings the internal grid's height to 2880 (at least 1), as the range
/// editor's recorder scales (`game_fea_range::display_scale`).
fn display_scale(ih: u32) -> u32 {
    (2880 / ih.max(1)).max(1)
}

pub fn run(prefix: &str, seed: u64, secs: f32, weights: &str, caption: Option<&str>) -> anyhow::Result<()> {
    anyhow::ensure!(secs.is_finite() && secs > 0.0, "positive finite duration required");
    std::env::set_var("KESSEL_AUDIO_CAPTURE", "1");
    let manifest = crate::canyon::manifest_path();
    act_manifest::select_runtime_manifest(Some(manifest.clone()))?;
    graphics::set_active_preset(graphics::GraphicsPreset::High);
    let instance = crate::create_instance();
    let adapter = crate::request_adapter(&instance, None);
    let (device, queue) = crate::request_device(&adapter);
    let (mut r, mut g) = Game::build_playtest_act(&device, &queue, false, &world::act(1))?;
    g.set_defer_rtt_to_render(true);
    g.enable_canyon_course_seeded(seed)?;
    g.enable_singleplayer_crew();
    let net = crate::ai::Mlp::load_t34(weights)?;
    g.enemy_policy = Some(net);
    g.enemy_policy_over_orders = true;
    g.reset_enemy_ai_summary();
    let mut pilot = CanyonPilot::new(&g);
    g.update(&TankInput::default(), 0.0, &mut r);

    // The picture's sky. By default the episode is filmed under a clear sky at local noon of its
    // own date: no storm, snowfall, fog bank or cloud deck in the frames. The simulation keeps the
    // attempt's own weather fronts (the crews' sight in the storm is the encounter's, set in
    // `Game::update`); the frame's weather is swapped in for the render and put back after it.
    // The sky clock is moved once, here. `KESSEL_T34_FILM_WEATHER=episode` films the episode's own.
    let clear = std::env::var("KESSEL_T34_FILM_WEATHER").as_deref() != Ok("episode");
    if clear {
        let site = r.sky_clock.site;
        let iso = r.sky_clock.start.to_iso();
        let date: Vec<u32> = iso.get(..10).unwrap_or("1942-12-20").split('-').filter_map(|v| v.parse().ok()).collect();
        anyhow::ensure!(date.len() == 3, "unreadable sky clock start {iso}");
        let noon = crate::ephemeris::UtcInstant::from_calendar(date[0] as i32, date[1], date[2], 12, 0, 0.0)
            .plus_seconds(-site.longitude_deg_east * 240.0);
        println!("[t34-film] clear noon picture: sky clock {} -> {} (the episode's weather still runs the simulation)", iso, noon.to_iso());
        r.set_sky_clock(crate::celestial_sky::SkyClock::new(noon, site));
    }
    let (iw, ih) = (r.iw, r.ih);
    let scale = display_scale(ih);
    let (ow, oh) = (iw * scale, ih * scale);
    anyhow::ensure!(ow % 2 == 0 && oh % 2 == 0, "output {ow}x{oh} must be even");
    // 4:4:4 by default: with 4:2:0 a 5x5 block would share chroma with its neighbour along one
    // output pixel. KESSEL_T34_FILM_PIXFMT=yuv420p for players without 4:4:4 HEVC.
    let pix_fmt = std::env::var("KESSEL_T34_FILM_PIXFMT").unwrap_or_else(|_| "yuv444p".into());
    let silent = format!("{prefix}_silent.mp4");
    // NVENC by default; `KESSEL_T34_FILM_ENCODER=x264` encodes on the processor (the card's
    // encoder refuses to open when its memory is taken, as with several players and Blender up).
    let x264 = std::env::var("KESSEL_T34_FILM_ENCODER").as_deref() == Ok("x264");
    let codec: &[&str] = if x264 { &["-c:v", "libx264", "-preset", "veryfast", "-crf", "18"] }
        else { &["-c:v", "hevc_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "29", "-tag:v", "hvc1"] };
    let mut encoder = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-f", "rawvideo", "-pixel_format", "rgba",
            "-video_size", &format!("{iw}x{ih}"), "-framerate", &FPS.to_string(), "-i", "pipe:0", "-an",
            "-vf", &format!("scale={ow}:{oh}:flags=neighbor,format={pix_fmt}")])
        .args(codec)
        .args([
            "-movflags", "+faststart", &silent])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    let mut pipe = encoder.stdin.take().ok_or_else(|| anyhow::anyhow!("encoder stdin unavailable"))?;
    println!("[t34-film] {weights} seed {seed}: {iw}x{ih} game pixels x{scale} -> {ow}x{oh}, {FPS} fps");

    let dt = 1.0 / 60.0;
    let per_step = (acoustics::SAMPLE_RATE / 60) as usize;
    let mut samples: Vec<f32> = Vec::new();
    let mut frames = 0u64;
    let mut outcome = "time_limit";
    let mut linger = 0.0f32;
    // The camera's smoothed focus and distance: the actor changes (a crew dies, another closes)
    // without a cut.
    let mut focus_s: Option<Vec3> = None;
    let mut dist_s = 110.0f32;
    let mut back_s: Option<Vec3> = None;
    let probe = std::env::var_os("KESSEL_T34_FILM_PROBE").is_some();
    let began = std::time::Instant::now();
    let steps = (secs / dt).ceil() as usize;
    let mut state = g.canyon_ai_state(&pilot, 0.0);
    // The fielded crews numbered 1.. in slot order, for the marks over their hulls.
    let slot_order: Vec<usize> = (0..g.enemies.len()).filter(|&i| g.enemies[i].alive).collect();
    for step in 0..steps {
        let t = step as f32 * dt;
        let input = pilot.step(&mut g, &r, t, dt);
        g.update(&input, dt, &mut r);
        if step % 8 == 0 { crate::simulation_uploads::drain(&device, &queue)?; }
        samples.extend(g.render_audio_capture_frames(per_step)?);
        if step % 2 == 0 {
            // Aerial, over the platoon's shoulder: the living crews within 900 m of the Tiger (their
            // centroid, pulled a quarter of the way toward the Tiger) seen from behind them, the Tiger
            // ahead in the frame. Close enough that a T-34 reads on the 1024x576 grid (70-140 m, the
            // crews' own spread), so their formation and its changes are the picture; the Tiger is
            // the mark they close on. Without living crews the Tiger itself, from its stern quarter.
            g.clear_crew_ko();
            let p = g.player.center();
            let near: Vec<Vec3> = g.enemies.iter().filter(|e| e.alive && e.center().distance(p) < 900.0)
                .map(|e| e.center()).collect();
            let (target, spread, toward) = if near.is_empty() {
                let stern = -Vec3::new(g.player.heading.sin(), 0.0, g.player.heading.cos());
                (p, 0.0, -stern)
            } else {
                let c = near.iter().copied().sum::<Vec3>() / near.len() as f32;
                let spread = near.iter().map(|e| e.distance(c)).fold(0.0f32, f32::max) * 2.0;
                let flat = Vec3::new(p.x - c.x, 0.0, p.z - c.z);
                (c + (p - c) * 0.25, spread, if flat.length() > 1.0 { flat.normalize() } else { Vec3::Z })
            };
            let want = (spread * 0.5 + 40.0).clamp(60.0, 115.0);
            let k = 1.0 - (-dt * 2.0 / 0.8).exp();
            let focus = match focus_s { Some(f) => f + (target - f) * k, None => target };
            focus_s = Some(focus);
            dist_s += (want - dist_s) * k;
            let want_back = -toward * 0.82 + Vec3::Y * 0.46 + toward.cross(Vec3::Y) * 0.22;
            let back = match back_s { Some(b) => (b + (want_back - b) * k * 0.5).normalize(), None => want_back.normalize() };
            back_s = Some(back);
            let eye = focus + back * dist_s;
            r.camera.pos = eye;
            // The view leans from the crews toward the Tiger, so its mark stays in the frame's
            // upper part while the crews sit in the lower.
            let look_at = focus + (p - focus) * 0.35;
            r.camera.set_forward(look_at - eye);
            if probe && step % 120 == 0 {
                let crews: Vec<String> = g.enemies.iter().filter(|e| e.alive).map(|e| format!("{:.0}m", e.center().distance(p))).collect();
                println!("[t34-film-probe] t {:.0} eye {:.0?} focus {:.0?} dist {:.0} crews {:?}", t, eye, focus, dist_s, crews);
            }
            r.camera.fov_y = 52f32.to_radians();
            // An observer in the open (as the range's free camera outside the hull): no optic
            // mask or reticle from the player's station, none of the compartment's light, haze or
            // shelter laid over the frame.
            r.reticle = false;
            r.optic_mask = 0.0;
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere(0.0, 0.0, Vec3::ZERO);
            // Marks at game pixels, so the fight reads at a glance: the
            // Tiger's place and range (it stands beyond the fog's reach for most of an episode),
            // each crew's slot number over its hull, and the clock, the crews' state, shots and
            // hits along the bottom; the caption (the generation) top left.
            r.ui_reset();
            let vp = r.camera.view_proj();
            let project = |w: Vec3| -> Option<(f32, f32)> {
                let c = vp * w.extend(1.0);
                if c.w <= 0.1 { return None; }
                let (x, y) = (c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
                (x > -0.05 && x < 1.05 && y > -0.05 && y < 1.05).then_some((x, y))
            };
            let mark = [0.98, 0.86, 0.35, 0.95];
            let dim = [0.92, 0.94, 0.97, 0.9];
            if let Some((x, y)) = project(p + Vec3::Y * 2.2) {
                let range = near.first().map_or(0.0, |_| near.iter().map(|e| e.distance(p)).sum::<f32>() / near.len() as f32);
                r.draw_text_bold(&format!("TIGER {range:.0} m"), x, y - 0.05, 0.028, mark, Align::Center);
                r.draw_text_bold("|", x, y - 0.026, 0.028, mark, Align::Center);
            }
            for (i, e) in g.enemies.iter().enumerate() {
                if !e.alive { continue; }
                if let Some((x, y)) = project(e.center() + Vec3::Y * 3.2) {
                    let n = slot_order.iter().position(|&s| s == i).map_or(i + 1, |k| k + 1);
                    r.draw_text_bold(&n.to_string(), x, y - 0.03, 0.03, dim, Align::Center);
                }
            }
            let alive = g.enemies.iter().filter(|e| e.alive).count();
            let their_shots: u64 = state["enemy_ai"]["shots_front_side_rear"].as_array()
                .map_or(0, |a| a.iter().filter_map(|v| v.as_u64()).sum());
            let status = format!("{:02}:{:02}   crews {alive}   their shots {their_shots}   hits on the Tiger {}   Tiger shots {}   kills {}",
                (t as u32) / 60, (t as u32) % 60,
                state["incoming_hits"].as_u64().unwrap_or(0), state["shots"].as_u64().unwrap_or(0),
                state["kills"].as_u64().unwrap_or(0));
            r.draw_text(&status, 0.012, 0.965, 0.026, dim, Align::Left);
            if let Some(text) = caption {
                r.draw_text_bold(text, 0.012, 0.03, 0.034, dim, Align::Left);
            }
            let weather = (r.storm, r.precipitation, r.fog_density, r.overcast, r.low_clouds.clone());
            if clear {
                r.storm = 0.0;
                r.precipitation = 0.0;
                r.fog_density = 0.00035;
                r.overcast = 0.0;
                r.low_clouds = None;
            }
            let rgba = r.render_to_rgba_with_hud(&device, &queue, iw, ih)?;
            (r.storm, r.precipitation, r.fog_density, r.overcast, r.low_clouds) = weather;
            pipe.write_all(&rgba)?;
            frames += 1;
        }
        if step % 60 == 0 { state = g.canyon_ai_state(&pilot, t + dt); }
        if g.game_over().is_some() {
            outcome = "knocked_out";
            linger += dt;
            if linger > LINGER_S { break; }
        } else if g.canyon_finished().is_some() {
            outcome = "completed";
            break;
        }
    }
    crate::simulation_uploads::drain(&device, &queue)?;
    drop(pipe);
    anyhow::ensure!(encoder.wait()?.success(), "film encoder failed");
    anyhow::ensure!(frames > 0, "no frames filmed");

    acoustics::attenuate_over_peak(&mut samples, 0.94);
    let wav = format!("{prefix}_soundtrack.wav");
    acoustics::write_wav_stereo(&wav, &samples, acoustics::SAMPLE_RATE)?;
    let output = format!("{prefix}.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-i", &silent, "-i", &wav, "-map", "0:v:0", "-map", "1:a:0",
            "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest", "-movflags", "+faststart", &output])
        .status()?;
    anyhow::ensure!(status.success(), "film mux failed");
    // A clip is saved once, with its sound (CLAUDE.md): the silent picture and the wav go.
    let _ = std::fs::remove_file(&silent);
    let _ = std::fs::remove_file(&wav);
    let receipt = serde_json::json!({"schema": "panzer.t34-generation-film.v1", "film": output, "weights": weights,
        "caption": caption, "seed": seed, "seconds": secs, "outcome": outcome, "tiger_alive": g.player.alive,
        "picture_weather": if clear { "clear sky at local noon (picture only; the simulation ran the episode's own fronts)" } else { "the episode's own" },
        "frames": frames, "fps": FPS, "game_pixels": [iw, ih], "scale": scale, "output_pixels": [ow, oh], "codec": if x264 { "libx264 crf 18" } else { "hevc_nvenc cq 29" }, "pix_fmt": pix_fmt,
        "camera": "aerial over the platoon's shoulder: the living crews' centroid pulled a quarter toward the Tiger, seen from behind them at 70-140 m; the game's first-person mix at the Tiger as the soundtrack",
        "fielding": "as t34-canyon-eval: enable_canyon_course_seeded, the policy over the doctrine's orders, the rule-based Canyon pilot drives the Tiger",
        "final": state, "enemy_ai": g.enemy_ai_summary(), "manifest": manifest,
        "wall_seconds": began.elapsed().as_secs_f64()});
    std::fs::write(format!("{prefix}_film.json"), serde_json::to_vec_pretty(&receipt)?)?;
    println!("[t34-film] {output}: {frames} frames, {outcome}, {:.0} s wall", began.elapsed().as_secs_f64());
    Ok(())
}
