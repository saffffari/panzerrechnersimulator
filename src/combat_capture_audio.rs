//! External observer microphone mix: live source voices, propagation and the shared reproduction
//! curve. This is an offline capture mix, without the crew's headset or physiological ear effects.
use crate::{acoustics, audio, reproduction, sound_levels, sound_propagation as propagation};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Gun { caliber_mm: f32, seed: u32 },
    Armour { penetrated: bool },
    Destruction { catastrophic: bool },
}

#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub at_s: f32,
    pub range_m: f32,
    pub bearing_deg: f32,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug)]
pub struct FlightFrame {
    pub segment: crate::game::FlightSegment,
    pub listener: glam::Vec3,
    pub forward: glam::Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct EngineFrame {
    pub at_s: f32,
    pub range_m: f32,
    pub bearing_deg: f32,
    pub t34: bool,
    pub speed_mps: f32,
}

fn propagated(
    stereo: &[f32],
    emission: propagation::Emission,
    range: f32,
    bearing: f32,
) -> Vec<f32> {
    let ear = propagation::propagate(
        &emission,
        &propagation::Path {
            distance_m: range,
            atmosphere: propagation::active_atmosphere(),
            occluded: false,
            enclosure: propagation::Enclosure::OnFoot,
            headset: false,
        },
    );
    let mut mono: Vec<_> = stereo
        .chunks_exact(2)
        .map(|c| (c[0] + c[1]) * 0.5)
        .collect();
    propagation::SpectralTransfer::fit(&ear.transfer_db, acoustics::SAMPLE_RATE as f32)
        .apply_mono(&mut mono);
    let mut out = acoustics::spatialize_mono(&mono, bearing, 0.0);
    let gain = reproduction::voice_gain(ear.laf_db, reproduction::laf_max_dbfs(&out));
    for s in &mut out {
        *s *= gain;
    }
    out
}

fn mix(dst: &mut [f32], source: &[f32], at: f32) {
    let offset = (at.max(0.0) * acoustics::SAMPLE_RATE as f32).round() as usize * 2;
    if offset < dst.len() {
        for (to, from) in dst[offset..].iter_mut().zip(source) {
            *to += *from;
        }
    }
}

pub fn render(
    seconds: f32,
    cues: &[Cue],
    engines: &[EngineFrame],
    weather_time: f32,
    flights: &[FlightFrame],
) -> anyhow::Result<(Vec<f32>, serde_json::Value)> {
    let sample_frames = (seconds * acoustics::SAMPLE_RATE as f32).round() as usize;
    let mut out = Vec::with_capacity(sample_frames * 2);
    // Use the actual continuously filtered weather voice and its physical wind-level law.
    // The old fixed ambience loop was an unrelated bright noise floor beneath every explosion.
    let weather = audio::Audio::new_capture();
    let block = acoustics::SAMPLE_RATE as usize / 10;
    while out.len() / 2 < sample_frames {
        let at = out.len() as f32 / (2 * acoustics::SAMPLE_RATE) as f32;
        let state = crate::storm_weather::sample(2.5, weather_time + at);
        weather.set_weather_ambience(state.storm, 0.0, state.wind);
        let count = block.min(sample_frames - out.len() / 2);
        out.extend(
            weather
                .render_capture_frames(count)
                .ok_or_else(|| anyhow::anyhow!("weather capture unavailable"))?,
        );
    }
    let tiger = acoustics::synth_procedural_dry(acoustics::ProceduralSound::EngineLoop);
    let t34 = acoustics::synth_procedural_dry(acoustics::ProceduralSound::T34ExteriorLoop);
    for frame in engines {
        let source = if frame.t34 { &t34 } else { &tiger };
        let level = if frame.t34 {
            &sound_levels::T34_EXTERIOR_10M
        } else {
            &sound_levels::TIGER_EXTERIOR_10M
        };
        // Same 17 dB idle-to-moving span as the engine source tables; this observer does not
        // synthesize transmission telemetry. Each block uses the actual tank's speed and range.
        let offset_db = -17.0 * (1.0 - (frame.speed_mps.abs() / 5.0).clamp(0.0, 1.0));
        let voice = propagated(
            source,
            propagation::Emission::from_level(level).shifted(offset_db),
            frame.range_m,
            frame.bearing_deg,
        );
        let start = (frame.at_s * acoustics::SAMPLE_RATE as f32).round() as usize;
        let part: Vec<_> = (0..block * 2)
            .map(|i| voice[(start * 2 + i) % voice.len()])
            .collect();
        mix(&mut out, &part, frame.at_s);
    }
    for cue in cues {
        let (source, emission) = match cue.kind {
            Kind::Gun { caliber_mm, seed } => {
                let shell = if caliber_mm > 80.0 {
                    &crate::ballistics::PZGR_39
                } else {
                    &crate::ballistics::BR_350B
                };
                let charge = sound_levels::propellant_charge_kg(
                    shell.caliber_mm,
                    shell.mass_kg,
                    shell.muzzle_velocity,
                );
                let braked = sound_levels::gun_is_braked(shell.caliber_mm, shell.muzzle_velocity);
                (
                    acoustics::synth_gun_report(caliber_mm, 0.0, 0.0, seed),
                    propagation::Emission::gun(charge, braked, 90.0),
                )
            }
            Kind::Armour { penetrated } => (
                acoustics::synth_procedural_dry(if penetrated {
                    acoustics::ProceduralSound::Penetration
                } else {
                    acoustics::ProceduralSound::ImpactTank
                }),
                propagation::Emission::from_level(&sound_levels::EXTERIOR_ARMOUR_STRIKE_10M),
            ),
            Kind::Destruction { catastrophic } => (
                acoustics::synth_procedural_dry(if catastrophic {acoustics::ProceduralSound::TurretPop}
                    else {acoustics::ProceduralSound::KillExplosion}),
                propagation::Emission::blast(if catastrophic {sound_levels::AMMUNITION_DETONATION_TNT_KG}
                    else {sound_levels::VEHICLE_KILL_TNT_KG}),
            ),
        };
        if matches!(cue.kind,Kind::Destruction{catastrophic:true}) {
            let tear=propagated(&acoustics::synth_procedural_dry(acoustics::ProceduralSound::TurretRip),
                propagation::Emission::from_level(&sound_levels::EXTERIOR_ARMOUR_STRIKE_10M),
                cue.range_m,cue.bearing_deg);
            mix(&mut out,&tear,cue.at_s+cue.range_m/propagation::speed_of_sound());
        }
        let voice = propagated(&source, emission, cue.range_m, cue.bearing_deg);
        mix(
            &mut out,
            &voice,
            cue.at_s + cue.range_m / propagation::speed_of_sound(),
        );
    }
    // One continuous oscillator/filter per real projectile. Retain fractional sample timing;
    // the initial propagation delay is held for the stream, exactly as in live playback.
    let mut flight_voices: std::collections::HashMap<u64,(crate::shell_flight_audio::Voice,f32,usize)> = std::collections::HashMap::new();
    for frame in flights {
        let segment=&frame.segment;
        let params=segment.params(frame.listener,frame.forward);
        let range=((segment.from+segment.to)*0.5).distance(frame.listener);
        let (voice,arrival,frames)=flight_voices.entry(segment.id).or_insert_with(||(
            crate::shell_flight_audio::Voice::new(segment.seed),segment.at_s+range/propagation::speed_of_sound(),0));
        let path=propagation::Path {distance_m:range,atmosphere:propagation::active_atmosphere(),
            occluded:false,enclosure:propagation::Enclosure::OnFoot,headset:false};
        let chunk=voice.render_propagated_segment(&params,&path,segment.first,segment.last);
        mix(&mut out,&chunk,*arrival+*frames as f32/acoustics::SAMPLE_RATE as f32);
        *frames+=chunk.len()/2;
    }
    // One lookahead master limiter preserves the pressure front; no stacked tanh saturation.
    let (out, reduction_db) = reproduction::master_offline(&out);
    Ok((
        out,
        serde_json::json!({"kind":"external_observer_microphone",
        "shared_source_synthesis":true,"shared_propagation_and_reproduction":true,
        "armour_cues":cues.iter().filter(|c|matches!(c.kind,Kind::Armour{..})).count(),
        "gun_cues":cues.iter().filter(|c|matches!(c.kind,Kind::Gun{..})).count(),
        "destruction_cues":cues.iter().filter(|c|matches!(c.kind,Kind::Destruction{..})).count(),
        "catastrophic_cues":cues.iter().filter(|c|matches!(c.kind,Kind::Destruction{catastrophic:true})).count(),
        "whistle_projectiles":flight_voices.len(),"whistle_segments":flights.len(),
        "whistle_flights":flight_voices.iter().map(|(id,(_,at,frames))|serde_json::json!({
            "projectile_id":id,"arrival_s":at,"audible_duration_s":*frames as f32/acoustics::SAMPLE_RATE as f32})).collect::<Vec<_>>(),
        "master_reduction_db":reduction_db,"cues":cues.iter().map(|cue|serde_json::json!({
            "at_s":cue.at_s,"range_m":cue.range_m,"bearing_deg":cue.bearing_deg,
            "kind":format!("{:?}",cue.kind)})).collect::<Vec<_>>(),
        "limits":"Offline external microphone: clear exterior paths and side-on gun source calibration; incoming armor acoustic origin uses tank center, within the vehicle envelope. Engine timbre is looped, level follows observed speed/range at 10 Hz. No headset, crew hearing reflex or tinnitus."}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_mix_keeps_strikes_and_impulses_above_the_weather() {
        let cues = [
            Cue {
                at_s: 1.0,
                range_m: 22.0,
                bearing_deg: -25.0,
                kind: Kind::Gun {
                    caliber_mm: 88.0,
                    seed: 112,
                },
            },
            Cue {
                at_s: 2.5,
                range_m: 22.0,
                bearing_deg: 10.0,
                kind: Kind::Armour { penetrated: false },
            },
            Cue {
                at_s: 4.0,
                range_m: 22.0,
                bearing_deg: 0.0,
                kind: Kind::Destruction { catastrophic: true },
            },
        ];
        let (mix, receipt) = render(6.0, &cues, &[], 0.0, &[]).unwrap();
        assert_eq!(receipt["gun_cues"], 1);
        assert_eq!(receipt["armour_cues"], 1);
        assert_eq!(receipt["destruction_cues"], 1);
        assert!(mix.iter().all(|s| s.is_finite() && s.abs() < 1.0));
        let rms_at = |at: f32, duration: f32| {
            let start = (at * acoustics::SAMPLE_RATE as f32) as usize * 2;
            let end = start + (duration * acoustics::SAMPLE_RATE as f32) as usize * 2;
            acoustics::rms(&mix[start..end])
        };
        let bed = rms_at(0.4, 0.3).max(1e-7);
        for cue in cues {
            let event = rms_at(cue.at_s + cue.range_m / propagation::speed_of_sound(), 0.2);
            assert!(
                event > bed * 10.0,
                "event {:?} lost beneath wind: {event} vs {bed}",
                cue.kind
            );
        }
        if let Ok(prefix) = std::env::var("KESSEL_COMBAT_AUDIO_REVIEW_PREFIX") {
            let path = format!("{prefix}.wav");
            assert!(!std::path::Path::new(&path).exists());
            acoustics::write_wav_stereo(path, &mix, acoustics::SAMPLE_RATE).unwrap();
            std::fs::write(
                format!("{prefix}.json"),
                serde_json::to_vec_pretty(&receipt).unwrap(),
            )
            .unwrap();
        }
    }
}
