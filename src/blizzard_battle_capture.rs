//! External witness of an ordinary tank fight in coupled blizzard weather.
//! Only opening positions and automated crew controls are authored; no impacts or kills are injected.
use super::*;
use crate::combat_capture_audio::{Cue, EngineFrame, Kind};
use anyhow::{ensure, Context};
use serde_json::json;
use std::io::Write;
use std::process::{Child, Command, Stdio};

/// A failed render or pipe write must not leave ffmpeg waiting for frames in the background.
/// Partial output stays on disk for diagnosis, under the caller's collision-safe prefix.
struct EncoderChild(Child);

impl Drop for EncoderChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub fn run(prefix: &str, seconds: f32, preview: bool) -> anyhow::Result<()> {
    ensure!(
        seconds.is_finite() && (6.0..=60.0).contains(&seconds),
        "duration must be 6..60 seconds"
    );
    ensure!(
        crate::graphics::shader_backend_enabled(),
        "requires shader renderer"
    );
    for suffix in [".json", ".mp4", "_picture.mp4", "_soundtrack.wav"] {
        ensure!(
            !std::path::Path::new(&format!("{prefix}{suffix}")).exists(),
            "output already exists"
        );
    }
    if let Some(parent) = std::path::Path::new(prefix).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let instance = create_instance();
    let adapter = request_adapter(&instance, None);
    let (device, queue) = request_device(&adapter);
    let (mut r, mut g) = Game::build_act(&device, &queue, false, &world::act(0))?;
    g.mute_audio();
    g.set_defer_rtt_to_render(true);
    g.reset_match(0xB112_1001, 75.0);
    g.player.pos = Vec3::new(60.0, g.hf.height_at(60.0, -280.0), -280.0);
    // Validate each actual opening lane against terrain/tree/cover before mustering the actors.
    let positions = [glam::Vec2::new(52.0, -212.0), glam::Vec2::new(72.0, -200.0)];
    let mut lanes = Vec::new();
    for position in positions {
        let delta = position - glam::Vec2::new(g.player.pos.x, g.player.pos.z);
        lanes.push(g.debug_stage_enemy_discharge_review(
            delta.length(),
            delta.x.atan2(delta.y).to_degrees(),
            2.0,
            r.time,
        )?);
    }
    g.player.heading = 0.0;
    let active = g.stage_showreel_t34_crossfire_wave(&positions, 0xB112_1001);
    for (k, &i) in active.iter().enumerate() {
        g.enemies[i].reload = 2.0 + k as f32 * 2.5;
    }
    let policy_path = crate::resource::resolve("assets/ai/weights_academy.bin")?;
    g.enemy_policy = Some(ai::Mlp::load(policy_path.to_str().context("policy path")?)?);
    ensure!(!g.god, "capture must retain ordinary damage");
    r.apply_campaign_atmosphere(1, true, DayPhase::Day);
    r.set_weather_seed(world::act(1).seed);
    r.set_weather_override(Some(2.5));
    r.camera.aspect = 16.0 / 9.0;

    let fps = if preview { 5 } else { 30 };
    let raw = format!("{prefix}_picture.mp4");
    let mut encoder = EncoderChild(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "1280x720",
                "-framerate",
                &fps.to_string(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-threads",
                "2",
                "-preset",
                "fast",
                "-crf",
                "17",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                &raw,
            ])
            .stdin(Stdio::piped())
            .spawn()
            .context("start video encoder")?,
    );
    let mut pipe = encoder.0.stdin.take().context("encoder input")?;
    let mut events = Vec::new();
    let mut samples = Vec::new();
    let mut audio = Vec::new();
    let mut microphone_cues = Vec::new();
    let mut engine_frames = Vec::new();
    let weather_time = r.time;
    let mut tiger_cursor = game::ShotSequenceCursor::at(g.main_gun_shot_sequence());
    let mut enemy_cursors: Vec<_> = active
        .iter()
        .map(|&i| game::ShotSequenceCursor::at(g.enemy_main_gun_shot_sequence(i)))
        .collect();
    let mut previous_hits = (0, 0);
    let mut previous_alive: Vec<_> = active.iter().map(|&i| g.enemies[i].alive).collect();
    let mut crew_cd = 4.0_f32;
    let mut last_shots = g.main_gun_shot_sequence();
    let mut dead_since = None;
    let steps = (seconds * 60.0).round() as u32;
    let mut frames = 0;
    let initial = json!({"tiger":g.player.pos.to_array(),"t34s":active.iter().map(|&i|
        g.enemies[i].pos.to_array()).collect::<Vec<_>>()});
    for tick in 0..steps {
        let now = (tick + 1) as f32 / 60.0;
        crew_cd = (crew_cd - 1.0 / 60.0).max(0.0);
        let controls = if !g.player.alive {
            TankInput::default()
        } else if g.is_fire_active() {
            TankInput {
                extinguish: true,
                ..Default::default()
            }
        } else if !g.breech_loaded() {
            g.request_station(game::Station::Loader);
            g.auto_service_breech(tick % 2 == 0, 1.0 / 60.0);
            TankInput::default()
        } else {
            g.request_station(game::Station::Gunner);
            g.aim_at_enemy();
            TankInput {
                mg_coax: crew_cd <= 0.0
                    && g.station_ready(game::Station::Gunner)
                    && g.auto_can_fire(),
                ..Default::default()
            }
        };
        g.update(&controls, 1.0 / 60.0, &mut r);
        if g.main_gun_shot_sequence() != last_shots {
            last_shots = g.main_gun_shot_sequence();
            crew_cd = 6.5;
        }
        // Keep uploads bounded even between preview frames; no renderer work alters simulation.
        if tick % 8 == 0 {
            simulation_uploads::drain(&device, &queue)?;
        }
        let combat = g.capture_combat_receipt();
        let hits = (g.stat_hits_on_player, combat["hits"].as_u64().unwrap_or(0));
        let mut new_sounds = Vec::new();
        if hits != previous_hits {
            events.push(json!({"at":now,"kind":"resolved_hit","incoming":hits.0,"outgoing":hits.1,"combat":combat}));
            for _ in previous_hits.0..hits.0 {
                let penetrated = g
                    .debug_last_player_damage_sound()
                    .is_some_and(|(p, _)| p.penetrated);
                new_sounds.push((g.player.center(), Kind::Armour { penetrated }));
            }
            for _ in previous_hits.1..hits.1 {
                let position = &combat["impact"];
                let source = Vec3::new(
                    position[0].as_f64().unwrap() as f32,
                    position[1].as_f64().unwrap() as f32,
                    position[2].as_f64().unwrap() as f32,
                );
                new_sounds.push((
                    source,
                    Kind::Armour {
                        penetrated: combat["impact_kind"] == "Penetrate",
                    },
                ));
            }
            previous_hits = hits;
        }
        for (k, &i) in active.iter().enumerate() {
            if previous_alive[k] && !g.enemies[i].alive {
                events.push(json!({"at":now,"kind":"t34_destroyed","index":i}));
                new_sounds.push((g.enemies[i].center(), Kind::Destruction));
                previous_alive[k] = false;
            }
        }
        if !g.player.alive && dead_since.is_none() {
            events.push(json!({"at":now,"kind":"tiger_destroyed"}));
            dead_since = Some(now);
            new_sounds.push((g.player.center(), Kind::Destruction));
        }
        // Two observer angles in one continuous fight: opening T-34 contact, then the Tiger.
        // Switch before the flanker's next shot so the incoming impact is visible in context.
        let saved_camera = r.camera;
        let focus = if now >= 7.0 {
            g.player.center()
        } else {
            g.enemies[active[0]].center()
        };
        let eye = focus + Vec3::new(-13.0, 8.0, 15.0);
        r.camera.pos = eye;
        r.camera.set_forward(focus + Vec3::Y - eye);
        r.camera.fov_y = 60_f32.to_radians();
        for (source, kind) in new_sounds {
            microphone_cues.push(Cue {
                at_s: now,
                range_m: source.distance(r.camera.pos),
                bearing_deg: duel_camera_bearing_deg(&r, source),
                kind,
            });
        }
        if tick % 6 == 0 {
            for (tank, t34) in std::iter::once((&g.player, false))
                .chain(active.iter().map(|&i| (&g.enemies[i], true)))
                .filter(|(tank, _)| tank.alive)
            {
                engine_frames.push(EngineFrame {
                    at_s: tick as f32 / 60.0,
                    range_m: tank.center().distance(r.camera.pos),
                    bearing_deg: duel_camera_bearing_deg(&r, tank.center()),
                    t34,
                    speed_mps: tank.speed,
                });
            }
        }
        let mut record_report = |source: Vec3, caliber_mm: f32, seed: u32, tiger: bool| {
            let range_m = source.distance(r.camera.pos);
            microphone_cues.push(Cue {
                at_s: now,
                range_m,
                bearing_deg: duel_camera_bearing_deg(&r, source),
                kind: Kind::Gun { caliber_mm, seed },
            });
            audio.push(DuelGunAudioEvent {
                at: now + range_m / crate::sound_propagation::speed_of_sound(),
                caliber_mm,
                range_m,
                bearing_deg: duel_camera_bearing_deg(&r, source),
                seed,
                tiger,
            });
            events.push(
                json!({"at":now,"kind":"gun_discharge","tiger":tiger,"source":source.to_array()}),
            );
        };
        if let Some(advance) = tiger_cursor.observe(g.main_gun_shot_sequence()) {
            for sequence in advance.iter() {
                record_report(g.player.muzzle().0, 88.0, 0x8800 ^ sequence, true);
            }
        }
        for (k, &i) in active.iter().enumerate() {
            if let Some(advance) = enemy_cursors[k].observe(g.enemy_main_gun_shot_sequence(i)) {
                for sequence in advance.iter() {
                    record_report(
                        g.enemies[i].muzzle().0,
                        76.2,
                        ((i as u32) << 16) ^ sequence,
                        false,
                    );
                }
            }
        }
        if tick % (60 / fps) == 0 {
            let saved = (
                r.reticle,
                r.optic_mask,
                r.optic_blast,
                r.crew_ko,
                r.interior_light,
                r.interior_haze,
                r.weather_shelter,
            );
            r.reticle = false;
            r.optic_mask = 0.0;
            r.optic_blast = 0.0;
            r.crew_ko = 0.0;
            r.interior_light = 0.0;
            r.interior_haze = 0.0;
            r.weather_shelter = 0.0;
            r.set_interior_atmosphere_enabled(false);
            r.set_active_view(1);
            g.drive_singleplayer_crew_rigs_for_view(None, &mut r);
            // Settle the sensor at the opening observer view without advancing the fight.
            if frames == 0 {
                for _ in 0..12 {
                    r.render_to_rgba(&device, &queue, r.iw, r.ih)?;
                }
            }
            pipe.write_all(&r.render_to_rgba(&device, &queue, 1280, 720)?)?;
            (
                r.reticle,
                r.optic_mask,
                r.optic_blast,
                r.crew_ko,
                r.interior_light,
                r.interior_haze,
                r.weather_shelter,
            ) = saved;
            r.set_interior_atmosphere_enabled(g.protected_main_interior_view());
            r.set_active_view(0);
            frames += 1;
        }
        r.camera = saved_camera;
        if tick % 60 == 0 {
            samples.push(
                json!({"at":now,"tiger_alive":g.player.alive,"hp":g.player.hp,
                "incoming_hits":hits.0,"outgoing_hits":hits.1,"combat":combat,
                "enemy_alive":active.iter().map(|&i|g.enemies[i].alive).collect::<Vec<_>>(),
                "snow":r.precipitation,"fog":r.fog_density,"wind":r.weather_wind_world.to_array()}),
            );
            println!(
                "[blizzard-battle] {now:.1}s shots={} hits={hits:?} alive={}",
                audio.len(),
                g.player.alive
            );
        }
    }
    drop(pipe);
    ensure!(encoder.0.wait()?.success(), "video encoder failed");
    let duration = frames as f32 / fps as f32;
    let wav = format!("{prefix}_soundtrack.wav");
    let (soundtrack, audio_receipt) = crate::combat_capture_audio::render(
        duration,
        &microphone_cues,
        &engine_frames,
        weather_time,
    )?;
    acoustics::write_wav_stereo(&wav, &soundtrack, acoustics::SAMPLE_RATE)?;
    mux_duel_soundtrack(&raw, &wav, &format!("{prefix}.mp4"))?;
    let tiger_shots = audio.iter().filter(|e| e.tiger).count();
    let enemy_shots = audio.len() - tiger_shots;
    let passed = tiger_shots > 0 && enemy_shots > 0 && previous_hits.0 + previous_hits.1 as u32 > 0;
    let receipt = json!({"schema":"panzer.blizzard-battle.v1","passed":passed,
        "video":format!("{prefix}.mp4"),"duration_s":duration,"fps":fps,"native_resolution":[r.iw,r.ih],
        "output_resolution":[1280,720],"adapter":adapter.get_info().name,"storm_intensity":2.5,
        "validated_opening_lanes":lanes,"initial_positions":initial,"policy":policy_path,"events":events,"samples":samples,
        "tiger_shots":tiger_shots,"enemy_shots":enemy_shots,"final_combat":g.capture_combat_receipt(),
        "incoming_hits":g.stat_hits_on_player,"incoming_penetrations":g.stat_pens_on_player,
        "tiger_alive":g.player.alive,"enemy_alive":active.iter().map(|&i|g.enemies[i].alive).collect::<Vec<_>>(),
        "scope":"Fixed opening positions; alerted live baseline T-34 AI; automated Tiger direct gun lay and physical breech service with 6.5s minimum cycle. Ordinary swept shells, armor, damage, wrecks, collisions and weather. External observer; not human gameplay or real-time performance.",
        "audio":"External observer microphone mixed offline from shared gameplay source voices, weather voice, propagation and reproduction; actual gun, armor-contact and destruction events. No player headset/ear effects.",
        "audio_receipt":audio_receipt,
        "forced_impacts":false,"forced_kills":false,"invulnerability":g.god});
    std::fs::write(
        format!("{prefix}.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    ensure!(
        passed,
        "capture did not contain two-sided fire and a resolved hit; see receipt"
    );
    Ok(())
}
