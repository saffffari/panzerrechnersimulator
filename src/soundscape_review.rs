//! Deterministic, device-free render of the real soundscape transport and master mixer.
use crate::{
    audio::{Audio, SoundscapeInput},
    soundscape_director::Context,
};
use serde_json::json;

pub fn run(prefix: &str) -> anyhow::Result<()> {
    let library = crate::sound_library::Library::load()?;
    anyhow::ensure!(!library.cues.is_empty(), "no enabled sound library cues");
    let mut night_ids = library.pool("night", "night|winter");
    night_ids.sort();
    let mut paths = vec![format!("{prefix}_receipt.json"), format!("{prefix}_transitions.wav"),
        format!("{prefix}_death.wav")];
    paths.extend((1..=night_ids.len()).map(|i| format!("{prefix}_night_{i}.wav")));
    for path in &paths {
        anyhow::ensure!(!std::path::Path::new(path).exists(), "preserve previous review: {path}");
    }
    if let Some(parent) = std::path::Path::new(prefix).parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let audio = Audio::new_capture();
    audio.prepare_soundscape_review(1943)?;
    let rate = crate::acoustics::SAMPLE_RATE as usize;
    let mut pcm = Vec::new();
    let mut snapshots = Vec::new();
    // The context timeline is an explicit fixture, not a claim of recorded gameplay.
    let phases = [
        ("clear night outside", 14.0, false, 0.0, 0.0, false, 0.0, 1),
        ("inside parked", 14.0, true, 0.0, 0.0, false, 0.0, 1),
        ("driving inside", 10.0, true, 4.0, 0.0, false, 0.0, 1),
        ("unseen enemies nearby", 10.0, true, 2.5, 0.0, false, 0.0, 1),
        ("combat inside", 12.0, true, 0.5, 0.9, true, 0.5, 1),
        (
            "combat release and aftermath",
            28.0,
            true,
            0.0,
            0.0,
            false,
            0.2,
            1,
        ),
        ("next night outside", 14.0, false, 0.0, 0.0, false, 0.0, 2),
        ("whiteout outside", 16.0, false, 0.0, 0.0, false, 3.0, 2),
        ("whiteout inside", 12.0, true, 0.0, 0.0, false, 3.0, 2),
        ("clearing outside", 12.0, false, 0.0, 0.0, false, 0.2, 2),
    ];
    for (label, seconds, inside, speed, threat, combat, storm, night) in phases {
        let start = pcm.len() as f64 / (rate * 2) as f64;
        for tick in 0..(seconds * 20.0) as usize {
            if label == "driving inside" && tick == 40 {
                audio.play_at_ear(crate::audio::Sfx::PlayerGunInt1, crate::sound_levels::OWN_88_BUTTONED.laf_db);
                audio.soundscape_main_gun_fired();
            }
            audio.update_soundscape(
                0.05,
                SoundscapeInput {
                    context: Context {
                        night: true,
                        night_id: night,
                        speed_mps: speed,
                        threat,
                        combat,
                        location: "review_field".into(),
                    },
                    act: 1,
                    position: [0.0, 0.0],
                    shelter: if inside { 0.92 } else { 0.0 },
                    storm,
                    whiteout: if label.starts_with("whiteout") { 0.9 } else { 0.0 },
                    engine_hiss: None,
                    inside,
                    solar_altitude_deg: -10.0,
                    season: "winter",
                },
            );
            pcm.extend(audio.render_capture_frames(rate / 20).unwrap());
        }
        snapshots.push(json!({"label":label,"start_s":start,"end_s":pcm.len() as f64/(rate*2) as f64,"state":audio.soundscape_snapshot()}));
    }
    anyhow::ensure!(
        pcm.iter().all(|x| x.is_finite()),
        "nonfinite soundscape output"
    );
    let peak = pcm.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    anyhow::ensure!(
        peak > 0.001 && peak <= 1.0,
        "inaudible or clipped review ({peak})"
    );
    crate::acoustics::write_wav_stereo(format!("{prefix}_transitions.wav"), &pcm, rate as u32)?;
    // Every night recording gets a >one-cycle audition, through the same source/master.
    let mut beds = Vec::new();
    for (index, id) in night_ids.iter().enumerate() {
        let a = Audio::new_capture();
        a.prepare_soundscape_review(1943 + index as u64)?;
        let source_seconds = a.soundscape_audition(id)?;
        let seconds = source_seconds.max(47.0).ceil() as usize + 3;
        let samples = a.render_capture_frames(rate * seconds).unwrap();
        let path = format!("{prefix}_night_{}.wav", index + 1);
        crate::acoustics::write_wav_stereo(&path, &samples, rate as u32)?;
        beds.push(json!({"id":id,"file":path,"seconds":seconds,"source_seconds":source_seconds,"source":library.cues[id].path}));
    }
    let mut solar = Vec::new();
    for event in ["sunrise", "sunset", "dusk"] {
        let Some(id) = library.events.get(event).and_then(|ids|ids.first()) else { continue; };
        let a = Audio::new_capture(); a.prepare_soundscape_review(1943)?;
        let seconds = a.soundscape_audition(id)?.ceil() as usize + 4;
        let samples = a.render_capture_frames(rate * seconds).unwrap();
        let path = format!("{prefix}_{event}.wav");
        crate::acoustics::write_wav_stereo(&path, &samples, rate as u32)?;
        solar.push(json!({"event":event,"id":id,"file":path,"seconds":seconds}));
    }
    // Rodio applies Sink controls on its consumed-sample quantum. Compare cursors only after
    // that real control latency and the shared lookahead have settled, not before pausing.
    audio.set_paused(true);
    let _settle = audio.render_capture_frames(rate / 20).unwrap();
    let before = audio.soundscape_snapshot();
    let pause = audio.render_capture_frames(rate / 2).unwrap();
    anyhow::ensure!(
        pause.iter().all(|x| x.abs() < 1e-6),
        "pause did not silence"
    );
    let after = audio.soundscape_snapshot();
    anyhow::ensure!(
        before["voices"] == after["voices"],
        "pause advanced cue frames"
    );
    audio.set_paused(false);
    audio.set_silenced(true);
    anyhow::ensure!(
        audio
            .render_capture_frames(100)
            .unwrap()
            .iter()
            .all(|x| *x == 0.0),
        "KIA leaked audio"
    );
    let death = death_fixture(prefix, rate)?;
    let death_pass = death["checks"].as_object().unwrap().values().all(|v| v.as_bool() == Some(true));
    let receipt = json!({"schema":"panzer.adaptive-audio-review.v1",
        "seed":1943,"kind":"scripted context fixture through ordinary Audio sinks and master",
        "not_gameplay_capture":true,"sample_rate":rate,"channels":2,"peak":peak,
        "transitions_file":format!("{prefix}_transitions.wav"),
        "phases":snapshots,"night_beds":beds,"solar_cues":solar,"death":death,
        "checks":{"finite":true,"pause_freezes_after_control_settle":true,
            "ordinary_kia_without_terminal_silent":true,"terminal_death_pass":death_pass},
        "capture":audio.capture_metadata(),"subjective_listening_accepted":false});
    std::fs::write(
        format!("{prefix}_receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    anyhow::ensure!(death_pass, "terminal death review failed; inspect {prefix}_receipt.json");
    println!(
        "[soundscape-review] {} night loops, transitions, pause, physical KIA cut and terminal chord passed: {prefix}",
        night_ids.len()
    );
    Ok(())
}

fn death_fixture(prefix: &str, rate: usize) -> anyhow::Result<serde_json::Value> {
    use crate::acoustics::ReverbParams;
    use crate::sound_propagation::{propagate, Atmosphere, Emission, Enclosure, Path};
    let audio = Audio::new_capture();
    audio.prepare_soundscape_review(1943)?;
    let mut pcm = audio.render_capture_frames(rate / 2).unwrap();
    let fatal_at_s = pcm.len() as f64 / (rate * 2) as f64;
    // An explicit fatal-outcome audio fixture, not a resolved gameplay kill. This is the
    // existing ammunition explosion source at a declared two-metre same-compartment path.
    let ear = propagate(&Emission::blast(crate::sound_levels::AMMUNITION_DETONATION_TNT_KG),
        &Path { distance_m: 2.0, atmosphere: Atmosphere::WINTER, occluded: false,
            enclosure: Enclosure::OnFoot, headset: true });
    audio.play_world_at_ear(crate::audio::Sfx::TurretPop, &ear, 0.0, ReverbParams::STEEL_INTERIOR);
    let duration = audio.start_death_score()
        .ok_or_else(|| anyhow::anyhow!("no death chord could be selected/decoded"))?;
    anyhow::ensure!(duration.is_finite() && (1.75..=120.0).contains(&duration),
        "unexpected death cue duration: {duration}");
    let selected = audio.soundscape_snapshot();
    let repeated = audio.start_death_score();
    anyhow::ensure!(repeated == Some(duration) && selected["death"] == audio.soundscape_snapshot()["death"],
        "repeated terminal entry restarted or changed the cue");
    let black_after_s = 1.5f32;
    let mut elapsed = 0.0f32;
    let mut cursor_previous = 0.0f32;
    let mut cursor_monotonic = true;
    let mut black_energy = 0.0f64;
    let mut frames_in_black = 0usize;
    let mut snapshots = Vec::new();
    while elapsed < duration + 0.5 {
        let black = elapsed >= black_after_s;
        audio.set_listener_level((1.0 - elapsed / black_after_s).clamp(0.0, 1.0));
        if black { audio.set_silenced(true); }
        let block = audio.render_capture_frames(rate / 20).unwrap();
        if black && elapsed < duration - 0.1 {
            black_energy += block.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
            frames_in_black += block.len() / 2;
        }
        pcm.extend(block);
        elapsed += 0.05;
        if let Some(cursor) = audio.death_score_elapsed() {
            cursor_monotonic &= cursor.is_finite() && cursor + 1e-5 >= cursor_previous;
            cursor_previous = cursor;
        } else if !audio.death_score_finished() {
            cursor_monotonic = false;
        }
        if snapshots.len() == 0 || (elapsed * 20.0).round() as usize % 20 == 0 {
            snapshots.push(json!({"elapsed_s":elapsed,"cue_cursor_s":audio.death_score_elapsed(),
                "world_cut":black,"finished":audio.death_score_finished()}));
        }
    }
    audio.set_listener_level(0.0);
    audio.set_silenced(true);
    let tail = audio.render_capture_frames(rate / 4).unwrap();
    let tail_silent = tail.iter().all(|x| x.abs() < 1e-7);
    pcm.extend(tail);
    let finite = pcm.iter().all(|x| x.is_finite());
    let peak = pcm.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    let file = format!("{prefix}_death.wav");
    crate::acoustics::write_wav_stereo(&file, &pcm, rate as u32)?;
    Ok(json!({"file":file,"kind":"staged fatal-outcome Audio graph fixture; not gameplay",
        "fatal_source":"TurretPop; calibrated ammunition detonation, 2m same compartment, headset",
        "fatal_ear_laf_dba":ear.laf_db,"fatal_at_s":fatal_at_s,"cue_duration_s":duration,
        "black_at_s":fatal_at_s+f64::from(black_after_s),
        "text_midpoint_at_s":fatal_at_s+f64::from(duration)*0.5,
        "cue_end_at_s":fatal_at_s+f64::from(duration)
            +crate::reproduction::MASTER_LATENCY_FRAMES as f64/rate as f64,
        "master_latency_s":crate::reproduction::MASTER_LATENCY_FRAMES as f64/rate as f64,
        "selected":selected,
        "cursor_snapshots":snapshots,"last_cue_cursor_s":cursor_previous,
        "black_frames":frames_in_black,"black_mean_square":black_energy/(frames_in_black.max(1)*2) as f64,
        "sample_frames":pcm.len()/2,"seconds":pcm.len() as f64/(rate*2) as f64,
        "peak_dbfs":crate::reproduction::peak_dbfs(&pcm),"master":audio.master_telemetry(),
        "checks":{"finite":finite,"bounded":peak<=crate::reproduction::LIMITER_CEILING+1e-5,
            "cue_cursor_monotonic":cursor_monotonic,"cue_continues_with_world_cut_and_level_zero":black_energy>1e-8,
            "cue_reaches_complete_clip":(cursor_previous-duration).abs()<=0.051,
            "cue_finished_naturally":audio.death_score_finished(),"post_cue_silence":tail_silent},
        "display_timing_is_receipt_only":true,"subjective_listening_accepted":false}))
}
