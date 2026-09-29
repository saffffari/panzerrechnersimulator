//! Bounded, device-free listening suite through the ordinary gameplay Audio sink graph.
//! Events below are explicitly staged auditions, not recorded combat or measured Tiger acoustics.
use crate::{
    acoustics as a,
    audio::{Audio, EngineTelemetry},
};
use serde_json::{json, Value};
use std::path::Path;

const HZ: usize = a::SAMPLE_RATE as usize;
const TICKS: usize = 60;
const FRAMES_PER_TICK: usize = HZ / TICKS;

struct Take {
    slug: &'static str,
    samples: Vec<f32>,
    graph: Value,
    cues: Value,
}

fn capture(
    slug: &'static str,
    seconds: usize,
    cues: Value,
    mut tick: impl FnMut(&mut Audio, usize),
) -> Take {
    let mut audio = Audio::new_capture();
    let mut samples = Vec::with_capacity(seconds * HZ * 2);
    for frame in 0..seconds * TICKS {
        tick(&mut audio, frame);
        samples.extend(
            audio
                .render_capture_frames(FRAMES_PER_TICK)
                .expect("capture backend is device independent"),
        );
    }
    Take {
        slug,
        samples,
        graph: audio.capture_metadata().unwrap(),
        cues,
    }
}

fn metrics(samples: &[f32]) -> Value {
    let peak = samples.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
    let (left, right) = a::channel_energy(samples);
    json!({"sample_frames":samples.len()/2,"peak":peak,"rms":a::rms(samples),
        "left_energy":left,"right_energy":right,
        "nonzero_samples":samples.iter().filter(|s| **s != 0.0).count(),
        "nonfinite_samples":samples.iter().filter(|s| !s.is_finite()).count()})
}

fn interval(samples: &[f32], start: usize, end: usize) -> &[f32] {
    &samples[start * HZ * 2..end * HZ * 2]
}

fn bullet(seed: u32, bearing: f32) -> a::ImpactParams {
    a::ImpactParams {
        shell: a::ShellClass::Mg,
        caliber_mm: 7.92,
        mass_kg: 0.012,
        velocity: 640.0,
        plate_thickness_mm: 82.0,
        effective_mm: 104.0,
        impact_angle_deg: 40.0,
        residual_v: 0.0,
        behind_armor: 0.0,
        panel: a::HullPanel::TurretSide,
        penetrated: false,
        bearing_deg: bearing,
        station_dist_m: 1.2,
        seed,
    }
}

/// Produce six WAVs and one provenance/timing/level receipt. Refuse to overwrite any prior run.
/// One common attenuation-only export gain preserves relative levels between all six auditions.
pub fn run(prefix: &str) -> anyhow::Result<()> {
    const SLUGS: [&str; 6] = [
        "weather",
        "drivetrain",
        "mechanisms",
        "armor",
        "mg_hail",
        "kia_cut",
    ];
    let receipt_path = format!("{prefix}_receipt.json");
    anyhow::ensure!(
        !Path::new(&receipt_path).exists(),
        "refusing to overwrite {receipt_path}"
    );
    for slug in SLUGS {
        anyhow::ensure!(
            !Path::new(&format!("{prefix}_{slug}.wav")).exists(),
            "refusing to overwrite prior {slug} audition"
        );
    }
    if let Some(parent) = Path::new(prefix)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let weather = capture(
        "weather",
        20,
        json!([
            {"at_s":0,"cue":"calm exterior","storm":0.05,"shelter":0,"wind_mps":2},
            {"at_s":5,"cue":"blizzard exterior","storm":1,"shelter":0,"wind_mps":24},
            {"at_s":11,"cue":"hatch shut","storm":1,"shelter":1,"wind_mps":24},
            {"at_s":16,"cue":"hatch partly opened","storm":0.8,"shelter":0.45,"wind_mps":18}
        ]),
        |audio, frame| {
            let (storm, shelter, wind) = match frame / TICKS {
                0..=4 => (0.05, 0.0, 2.0),
                5..=10 => (1.0, 0.0, 24.0),
                11..=15 => (1.0, 1.0, 24.0),
                _ => (0.8, 0.45, 18.0),
            };
            audio.set_weather_ambience(storm, shelter, wind);
        },
    );
    let drivetrain = capture(
        "drivetrain",
        18,
        json!([
            {"at_s":0,"cue":"engine idle"},{"at_s":3,"cue":"loaded pullaway"},
            {"at_s":7,"cue":"upshift and coast"},{"at_s":10,"cue":"pivot and turret traverse"},
            {"at_s":14,"cue":"engine shutoff"}
        ]),
        |audio, frame| {
            let (speed, throttle, load, running) = match frame / TICKS {
                0..=2 => (0.0, 0.0, 0.0, true),
                3..=6 => (((frame - 180) as f32 / 240.0) * 0.45, 0.85, 0.82, true),
                7..=9 => (0.58, 0.10, 0.12, true),
                10..=13 => (0.0, 0.75, 0.85, true),
                _ => (0.0, 0.0, 0.0, false),
            };
            let mut telemetry = EngineTelemetry::automatic(speed, throttle, load, running);
            if (600..840).contains(&frame) {
                telemetry.gear = 0;
                telemetry.rpm_norm = 0.73;
            }
            audio.set_engine_telemetry(telemetry, 1.0 / 60.0);
            audio.set_turret_traverse(
                if (600..840).contains(&frame) {
                    0.6
                } else {
                    0.0
                },
                true,
                1.0 / 60.0,
            );
            if frame == 840 {
                audio.play_at_volume(crate::audio::Sfx::EngineDeath, 0.4);
            }
        },
    );
    let mechanisms = capture(
        "mechanisms",
        15,
        json!([
            {"at_s":0,"cue":"starter"},{"at_s":2,"cue":"hatch opening and stop"},
            {"at_s":4,"cue":"breech open"},{"at_s":5,"cue":"casing extract"},
            {"at_s":6,"cue":"casing drop"},{"at_s":7,"cue":"shell lift"},
            {"at_s":8,"cue":"shell ram"},{"at_s":9,"cue":"breech lock"},
            {"at_s":10,"cue":"shell drop"},{"at_s":11,"cue":"recoil mechanism"},
            {"at_s":12,"cue":"P08 magazine out/in/toggle"}
        ]),
        |audio, frame| {
            audio.set_starter(frame < 108);
            if (120..180).contains(&frame) {
                let phase = (frame - 120) as f32 / 60.0;
                audio.update_hatch_sound(0, phase, phase + 1.0 / 60.0);
            }
            if frame % 60 == 0 {
                let cue = match frame / 60 {
                    4 => Some(a::MechanicalCue::BreechOpen),
                    5 => Some(a::MechanicalCue::CasingExtract),
                    6 => Some(a::MechanicalCue::CasingDrop),
                    7 => Some(a::MechanicalCue::ShellLift),
                    8 => Some(a::MechanicalCue::ShellRam),
                    9 => Some(a::MechanicalCue::BreechLock),
                    10 => Some(a::MechanicalCue::ShellDrop),
                    11 => Some(a::MechanicalCue::MainGunRecoil),
                    _ => None,
                };
                if let Some(cue) = cue {
                    audio.play_mechanical(
                        a::MechanicalParams {
                            cue,
                            intensity: 0.8,
                            bearing_deg: 55.0,
                            seed: 0xA001 + frame as u32,
                        },
                        0.6,
                    );
                }
            }
            for (at, key) in [
                (720, "p08_mag_out"),
                (750, "p08_mag_in"),
                (780, "p08_toggle_close"),
            ] {
                if frame == at {
                    audio.play_control(key, 0.6);
                }
            }
        },
    );
    // The listener's ears (`hearing.rs`), driven by each staged strike's physical level at the ear.
    let mut hearing = crate::hearing::Hearing::default();
    let armor = capture(
        "armor",
        14,
        json!([
            {"at_s":1,"cue":"frontal ricochet"},{"at_s":4,"cue":"marginal stopped turret strike"},
            {"at_s":8,"cue":"close side penetration"},
            {"at_s":8.25,"cue":"separately staged physical fragment contact"}
        ]),
        |audio, frame| {
            audio.set_engine_telemetry(EngineTelemetry::automatic(0.0, 0.0, 0.0, true), 1.0 / 60.0);
            let signature = match frame {
                60 => Some(a::ImpactSignature::DistantFrontalRicochet),
                240 => Some(a::ImpactSignature::LongRangeT34Marginal),
                480 => Some(a::ImpactSignature::CloseT34SidePenetration),
                _ => None,
            };
            if let Some(signature) = signature {
                let seed = 0xA570 + frame as u32;
                let event = a::synth_impact_signature(signature, seed);
                let bearing = a::impact_signature_params(signature, seed).0.bearing_deg;
                let ear = a::impact_signature_ear_level(signature, seed);
                a::hear_strike(&mut hearing, &ear, bearing);
                audio.play_stereo_at_ear(event.main, ear.laf_db, crate::audio::EarBus::Impact);
            }
            if frame == 495 {
                audio.play_stereo(
                    a::synth_fragment_collision(a::FragmentCollisionParams {
                        speed_mps: 16.0,
                        material: a::FragmentMaterial::ArmorPlate,
                        bearing_deg: 315.0,
                        distance_m: 0.9,
                        seed: 0x5101,
                    }),
                    0.28,
                );
            }
            hearing.advance(1.0 / 60.0, 1.0 / 60.0, 93.0);
            audio.set_ear_state(&hearing.render());
        },
    );
    let mg = capture(
        "mg_hail",
        7,
        json!([
            {"at_s":1,"cue":"12 bullet contacts at 10 per second, right panel"},
            {"at_s":4,"cue":"12 bullet contacts at 10 per second, left panel"}
        ]),
        |audio, frame| {
            if ((60..132).contains(&frame) || (240..312).contains(&frame)) && frame % 6 == 0 {
                let bearing = if frame < 200 { 75.0 } else { 285.0 };
                audio.play_stereo(
                    a::synth_mg_armor_hit(bullet(0xB000 + frame as u32, bearing)),
                    0.52,
                );
            }
        },
    );
    let mut scene = None;
    let kia = capture(
        "kia_cut",
        8,
        json!([
            {"at_s":0,"cue":"all buses active, including scene emitter and the tinnitus voice"},
            {"at_s":3,"cue":"KIA gate: exact first-sample silence"},
            {"at_s":4,"cue":"new impact requested while silenced"},
            {"at_s":5,"cue":"explicit co-op survivor resume after two seconds"}
        ]),
        |audio, frame| {
            audio.set_weather_ambience(0.9, 1.0, 22.0);
            audio.set_engine_telemetry(EngineTelemetry::automatic(0.2, 0.4, 0.6, true), 1.0 / 60.0);
            audio.set_mg(true, true, 1.0 / 60.0);
            audio.set_turret_traverse(0.5, true, 1.0 / 60.0);
            audio.set_starter((165..225).contains(&frame));
            audio.set_traverse_jam((165..240).contains(&frame));
            if (150..210).contains(&frame) {
                let phase = (frame - 150) as f32 / 60.0;
                audio.update_hatch_sound(0, phase, phase + 1.0 / 60.0);
            }
            if frame == 0 {
                use rodio::Source;
                let sink = audio.create_scene_sink().expect("offline scene sink");
                sink.append(
                    rodio::buffer::SamplesBuffer::new(
                        2,
                        a::SAMPLE_RATE,
                        a::synth_procedural_dry(a::ProceduralSound::AmbienceInterior),
                    )
                    .repeat_infinite(),
                );
                sink.set_volume(0.035);
                scene = Some(sink);
                // The trauma bus carries the ear model's tinnitus voice; set it for the gate check.
                audio.set_ear_state(&crate::hearing::EarRender {
                    shift_db: [[0.0; crate::hearing::EAR_BANDS]; 2],
                    reflex_db: 0.0,
                    conductive_db: [0.0; 2],
                    tinnitus: Some((8.0, 3_800.0)),
                });
            }
            if frame == 174 {
                let signature = a::ImpactSignature::CloseT34SidePenetration;
                let event = a::synth_impact_signature(signature, 0xAA12);
                let ear = a::impact_signature_ear_level(signature, 0xAA12);
                audio.play_stereo_at_ear(event.main, ear.laf_db, crate::audio::EarBus::Impact);
            }
            if frame == 180 {
                audio.set_silenced(true);
            }
            if frame == 240 {
                audio.play(crate::audio::Sfx::ImpactTank);
            }
            if frame == 300 {
                audio.set_silenced(false);
            }
        },
    );
    drop(scene);
    let silent = interval(&kia.samples, 3, 5);
    anyhow::ensure!(
        silent.len() == 2 * HZ * 2 && silent.iter().all(|x| *x == 0.0),
        "KIA gate leaked samples during its exact two-second hold"
    );
    anyhow::ensure!(
        a::rms(interval(&kia.samples, 2, 3)) > 1e-6 && a::rms(interval(&kia.samples, 6, 7)) > 1e-6,
        "KIA fixture did not exercise live and resumed audio"
    );
    let checks = json!({"kia_silent_frames":2*HZ,"kia_interval_s":[3,5],
        "kia_first_sample_zero":silent[0]==0.0,"kia_exact_zero":true,
        "weather_settled_intervals":{
            "calm_exterior":metrics(interval(&weather.samples,3,5)),
            "blizzard_exterior":metrics(interval(&weather.samples,9,11)),
            "blizzard_sealed":metrics(interval(&weather.samples,14,16)),
            "hatch_partly_open":metrics(interval(&weather.samples,18,20))}});
    let takes = [weather, drivetrain, mechanisms, armor, mg, kia];
    anyhow::ensure!(
        takes
            .iter()
            .all(|t| t.samples.iter().all(|x| x.is_finite())),
        "nonfinite sample in audition graph"
    );
    let max_peak = takes
        .iter()
        .flat_map(|t| &t.samples)
        .fold(0.0_f32, |p, x| p.max(x.abs()));
    let export_gain = if max_peak > 0.92 {
        0.92 / max_peak
    } else {
        1.0
    };
    let mut entries = Vec::new();
    for take in takes {
        let path = format!("{prefix}_{}.wav", take.slug);
        let scaled: Vec<f32> = take.samples.iter().map(|x| x * export_gain).collect();
        a::write_wav_stereo(&path, &scaled, a::SAMPLE_RATE)?;
        entries.push(
            json!({"name":take.slug,"path":path,"raw_metrics":metrics(&take.samples),
            "export_metrics":metrics(&scaled),"cues":take.cues,"audio_graph":take.graph}),
        );
    }
    let receipt = json!({"schema":"kessel.audio-terror-review.v1","sample_rate":HZ,"channels":2,
        "scope":"Staged listening fixtures through ordinary live Audio sink controls; no GPU, combat simulation, authentic tank recording or musician score",
        "export_gain":export_gain,"normalization":"one attenuation-only gain shared by all files; no per-cue normalization",
        "kia_policy":"Fixture explicitly resumes a surviving co-op listener after two seconds; fatal singleplayer Game remains muted",
        "checks":checks,"takes":entries});
    std::fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("[audio-terror-review] PASS: six WAVs; 88200 exact-zero KIA frames; common gain {export_gain:.5}; {receipt_path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "writes an explicitly requested device-free listening suite"]
    fn export_gameplay_sound_review() {
        let prefix = std::env::var("KESSEL_AUDIO_REVIEW_PREFIX")
            .expect("set an unused flat captures/astra prefix");
        super::run(&prefix).unwrap();
    }

    #[test]
    #[ignore = "writes a companion paper/compass listening fixture"]
    fn export_handling_sound_review() {
        use super::*;
        let prefix = std::env::var("KESSEL_AUDIO_HANDLING_PREFIX")
            .expect("set an unused flat captures/astra prefix");
        let wav = format!("{prefix}.wav");
        let receipt = format!("{prefix}_receipt.json");
        assert!(!Path::new(&wav).exists() && !Path::new(&receipt).exists());
        let keys = ["map_open", "map_close", "compass_equip", "compass_stow"];
        let bearings = [-20.0_f32, 20.0, 35.0, -35.0];
        let cues: Vec<Value> = keys
            .iter()
            .zip(bearings)
            .enumerate()
            .map(|(i, (key, bearing))| json!({"at_s":i,"key":key,"gain":0.5,"bearing_deg":bearing}))
            .collect();
        let take = capture("handling", 5, json!(cues), |audio, frame| {
            if frame % TICKS == 0 && frame / TICKS < keys.len() {
                let index = frame / TICKS;
                audio.play_control_at(keys[index], 0.5, bearings[index]);
            }
        });
        assert!(take.samples.iter().all(|x| x.is_finite() && x.abs() < 1.0));
        for second in 0..keys.len() {
            assert!(interval(&take.samples, second, second + 1)
                .iter()
                .any(|x| *x != 0.0));
        }
        a::write_wav_stereo(&wav, &take.samples, a::SAMPLE_RATE).unwrap();
        std::fs::write(
            &receipt,
            serde_json::to_vec_pretty(&json!({
                "schema":"kessel.audio-handling-review.v1",
                "scope":"Generated paper/cloth fallback cues through ordinary Audio; no recorded source or period-material claim",
                "path":wav,"sample_rate":HZ,"channels":2,"export_gain":1.0,
                "metrics":metrics(&take.samples),"cues":take.cues,"audio_graph":take.graph
            }))
            .unwrap(),
        )
        .unwrap();
        println!("[audio-handling-review] PASS: four generated controls; {receipt}");
    }
}
