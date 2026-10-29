//! `main-gun-candidates <outdir>`: the own 88 rendered device-free through the live capture graph
//! from three places (buttoned gunner, commander head-out, an observer 50 m off the flank), for
//! the shipping voice (`current`, the exact layer stack of `Game::play_player_main_gun_report`)
//! and the candidates of `audio::MainGunVoice`, each measured (LAF,max, peak, limiter, how long it
//! holds near its peak, band balance) and run through the ear model once and five times
//! (temporary threshold shift, tinnitus). CPU only. Writes `mg_candidate_{X}_{place}.wav` and
//! `mg_current_{place}.wav` plus a JSON receipt.
use super::*;
use crate::audio::{Audio, MainGunVoice, Sfx};
use crate::sound_propagation::{self as prop, Atmosphere, Enclosure, Path};

const SR: usize = crate::acoustics::SAMPLE_RATE as usize;
const TICKS: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Place { Buttoned, HeadOut, Ext50 }

impl Place {
    fn label(self) -> &'static str { match self { Self::Buttoned => "buttoned", Self::HeadOut => "headout", Self::Ext50 => "ext50" } }
}

fn place_ear(place: Place) -> (prop::EarLevel, f32, crate::acoustics::ReverbParams) {
    use crate::sound_levels as L;
    match place {
        Place::Buttoned => (prop::at_ear(L::OWN_88_BUTTONED.lpk_db, L::OWN_88_BUTTONED.laf_db, L::OWN_88_BUTTONED.spectrum,
            L::OWN_88_BUTTONED.a_duration_ms, true), 0.0, crate::acoustics::ReverbParams::STEEL_INTERIOR),
        Place::HeadOut => {
            let mut ear = prop::at_ear(L::OWN_88_HEAD_OUT.lpk_db, L::OWN_88_HEAD_OUT.laf_db, L::OWN_88_HEAD_OUT.spectrum,
                L::OWN_88_HEAD_OUT.a_duration_ms, true);
            // As `own_gun_path`: the muzzle ~4 m ahead of the cupola, head-out, through the headset.
            ear.transfer_db = prop::propagate(&prop::Emission::gun(2.5, true, 90.0), &Path { distance_m: 4.0,
                atmosphere: Atmosphere::WINTER, occluded: false, enclosure: Enclosure::HeadOut, headset: true }).transfer_db;
            (ear, 10.0, crate::acoustics::ReverbParams::OPEN_SNOW)
        }
        Place::Ext50 => (prop::propagate(&prop::Emission::gun(2.5, true, 90.0), &Path { distance_m: 50.0,
            atmosphere: Atmosphere::WINTER, occluded: false, enclosure: Enclosure::OnFoot, headset: false }),
            60.0, crate::acoustics::ReverbParams::OPEN_SNOW),
    }
}

/// The shipping stack, as `play_player_main_gun_report` plays it.
fn play_current(audio: &Audio, place: Place, seq: u32) {
    let (mut ear, bearing, reverb) = place_ear(place);
    let pick = Sfx::PlayerGunInt1;
    let blast = Sfx::PLAYER_GUNS_EXT[seq as usize % Sfx::PLAYER_GUNS_EXT.len()];
    let mut outside = ear;
    outside.laf_db += OUTSIDE_BLAST_LAYER_DB;
    outside.lpk_db += OUTSIDE_BLAST_LAYER_DB;
    match place {
        Place::Buttoned => {
            ear.transfer_db = crate::audio::CABIN_GUN_MAIN_LOSS_DB;
            audio.play_cabin_gun_layer(pick, &ear, bearing, true);
            outside.transfer_db = crate::audio::CABIN_GUN_OUTSIDE_LOSS_DB;
            audio.play_cabin_gun_layer(blast, &outside, bearing, true);
            let mut inside = place_ear(place).0;
            inside.laf_db += INSIDE_RECORDING_LAYER_DB;
            inside.lpk_db += INSIDE_RECORDING_LAYER_DB;
            inside.transfer_db = crate::audio::CABIN_GUN_INSIDE_LOSS_DB;
            audio.play_world_at_ear(Sfx::PlayerGunInt2, &inside, 0.0, crate::acoustics::ReverbParams::DIRECT);
        }
        Place::HeadOut => {
            audio.play_world_at_ear(pick, &ear, bearing, reverb);
            audio.play_world_at_ear(blast, &outside, bearing, reverb);
        }
        Place::Ext50 => {
            audio.play_world_at_ear(pick, &ear, bearing, reverb);
            audio.play_world_at_ear(blast, &outside, bearing, reverb);
        }
    }
    if place != Place::Ext50 { play_recoil(audio, seq, place == Place::HeadOut); }
}

fn play_recoil(audio: &Audio, seq: u32, head_out: bool) {
    audio.play_mechanical(crate::acoustics::MechanicalParams { cue: crate::acoustics::MechanicalCue::MainGunRecoil,
        intensity: 1.15, bearing_deg: 4.0, seed: seq ^ 0xB8EE_C001 }, if head_out { 0.50 } else { 0.78 });
}

/// A candidate voice as `Game::play_main_gun_voice` plays it.
fn play_candidate(audio: &Audio, voice: MainGunVoice, place: Place, seq: u32) -> bool {
    let (ear, bearing, reverb) = place_ear(place);
    let Some(mono) = audio.main_gun_voice(voice) else { return false };
    let ear = main_gun_presented_ear(ear, place != Place::Ext50, place == Place::Buttoned);
    audio.play_world_mono_in_room_peak_capped(&mono, &ear, bearing, crate::audio::Room::Parametric(reverb), MAIN_GUN_PEAK_CAP_DBFS);
    if place != Place::Ext50 { play_recoil(audio, seq, place == Place::HeadOut); }
    true
}

/// The owner's picks as the game plays them: `Inside` buttoned, `Outside` head-out (through the
/// open-air path) and for an outside observer (the propagation queue's voice, no firer trim).
fn play_final(audio: &Audio, place: Place, seq: u32) -> bool {
    let (ear, bearing, reverb) = place_ear(place);
    match place {
        Place::Buttoned => {
            let Some(mono) = audio.main_gun_voice(MainGunVoice::Inside((seq % 2) as u8)) else { return false };
            let ear = main_gun_presented_ear(ear, true, true);
            audio.play_world_mono_in_room_peak_capped(&mono, &ear, bearing, crate::audio::Room::Parametric(crate::acoustics::ReverbParams::STEEL_INTERIOR), MAIN_GUN_PEAK_CAP_DBFS);
            play_recoil(audio, seq, false);
        }
        Place::HeadOut => {
            let Some(mono) = audio.main_gun_voice(MainGunVoice::Outside) else { return false };
            let ear = main_gun_presented_ear(ear, true, false);
            audio.play_world_mono_in_room_peak_capped(&mono, &ear, bearing, crate::audio::Room::Parametric(reverb), MAIN_GUN_PEAK_CAP_DBFS);
            play_recoil(audio, seq, true);
        }
        Place::Ext50 => {
            let Some(mono) = audio.main_gun_voice(MainGunVoice::Outside) else { return false };
            audio.play_world_mono_in_room(&mono, &ear, bearing, crate::audio::Room::Parametric(reverb));
        }
    }
    true
}

fn analyse(samples: &[f32]) -> serde_json::Value {
    let mono: Vec<f32> = samples.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
    let hop = SR / 100;
    let env: Vec<f32> = mono.chunks(hop).map(|c| 10.0 * (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32 + 1e-20).log10()).collect();
    let top = env.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let held = |db: f32| env.iter().filter(|e| **e > top - db).count() as f32 * 0.01;
    let energy = |v: &[f32]| v.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().max(1e-30);
    let total = energy(&mono);
    let above = |hz: f32| energy(&crate::audio::hit_high_pass(&mono, hz)) / total;
    serde_json::json!({
        "laf_max_dbfs": crate::reproduction::laf_max_dbfs(samples),
        "peak_dbfs": crate::reproduction::peak_dbfs(samples),
        "seconds_within_3db_of_peak": held(3.0), "seconds_within_10db": held(10.0), "seconds_within_20db": held(20.0),
        "energy_share_above_250_hz": above(250.0), "energy_share_above_1_khz": above(1000.0), "energy_share_above_4_khz": above(4000.0),
    })
}

fn hearing_after(place: Place, shots: usize) -> serde_json::Value {
    let (ear, bearing, _) = place_ear(place);
    let mut hearing = crate::hearing::Hearing::default();
    let dt = 1.0 / TICKS as f32;
    for _ in 0..shots {
        hearing.impulse_with_envelope(crate::hearing::ear_impulses(&ear, bearing), 0.0, None);
        for _ in 0..(8 * TICKS) { hearing.advance(dt, dt, 100.0); }
    }
    let shift = hearing.ears.iter().map(|e| e.temporary_db()).fold(0.0f32, f32::max);
    serde_json::json!({"shots": shots, "temporary_shift_db": shift, "tinnitus": hearing.render().tinnitus.map(|(s, p)| serde_json::json!({"sensation_db": s, "pitch_hz": p}))})
}

pub(crate) fn run(outdir: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(outdir)?;
    // The owner picked (2026-10-29): `final` renders the picks; A/B/C stay available by argument.
    let all = std::env::args().any(|a| a == "--candidates");
    let voices: Vec<Option<MainGunVoice>> = if all { vec![None, Some(MainGunVoice::A), Some(MainGunVoice::B), Some(MainGunVoice::C), Some(MainGunVoice::Outside)] }
        else { vec![None, Some(MainGunVoice::Outside)] };
    let places = [Place::Buttoned, Place::HeadOut, Place::Ext50];
    let mut cases = Vec::new();
    for voice in voices {
        for place in places {
            let audio = Audio::new_capture();
            let ok = match voice { None => { play_current(&audio, place, 1); true }
                Some(MainGunVoice::Outside) => play_final(&audio, place, 1), Some(v) => play_candidate(&audio, v, place, 1) };
            anyhow::ensure!(ok, "{voice:?} is not installed (assets/sounds/main_gun_candidates under KESSEL_ASSET_ROOT)");
            let mut pcm = Vec::new();
            for _ in 0..(4 * TICKS) { pcm.extend(audio.render_capture_frames(SR / TICKS).ok_or_else(|| anyhow::anyhow!("capture"))?); }
            let limiter = audio.master_telemetry()["deepest_gain_reduction_db"].as_f64().unwrap_or(0.0);
            let name = match voice { None => format!("mg_current_{}", place.label()),
                Some(MainGunVoice::Outside) => match place { Place::Buttoned => "mg_inside_final_buttoned".to_string(),
                    Place::HeadOut => "mg_headout_final".to_string(), Place::Ext50 => "mg_ext50_final".to_string() },
                Some(v) => format!("mg_candidate_{}_{}", v.label(), place.label()) };
            crate::acoustics::write_wav_stereo(format!("{outdir}/{name}.wav"), &pcm, SR as u32)?;
            let mut m = analyse(&pcm);
            m["limiter_db"] = limiter.into();
            println!("[main-gun] {name}: LAF {:.1} dBFS, peak {:.1}, limiter {:.1} dB, {:.2} s within 3 dB, {:.2} s within 10 dB, >1 kHz {:.2}",
                m["laf_max_dbfs"].as_f64().unwrap_or(f64::NAN), m["peak_dbfs"].as_f64().unwrap_or(f64::NAN), limiter,
                m["seconds_within_3db_of_peak"].as_f64().unwrap_or(f64::NAN), m["seconds_within_10db"].as_f64().unwrap_or(f64::NAN),
                m["energy_share_above_1_khz"].as_f64().unwrap_or(f64::NAN));
            cases.push(serde_json::json!({"wav": format!("{name}.wav"), "voice": voice.map_or("current", |v| v.label()), "place": place.label(), "measured": m}));
        }
    }
    let mut hearing: Vec<_> = places.iter().flat_map(|p| [1, 5].map(|n| { let mut h = hearing_after(*p, n); h["place"] = p.label().into(); h })).collect();
    // A hatch open, head inside (SL §1 row 2): the ears' answer there too.
    {
        use crate::sound_levels as L;
        let ear = prop::at_ear(L::OWN_88_HATCH_OPEN.lpk_db, L::OWN_88_HATCH_OPEN.laf_db, L::OWN_88_HATCH_OPEN.spectrum, L::OWN_88_HATCH_OPEN.a_duration_ms, true);
        let mut h = crate::hearing::Hearing::default();
        let dt = 1.0 / TICKS as f32;
        for _ in 0..5 { h.impulse_with_envelope(crate::hearing::ear_impulses(&ear, 0.0), 0.0, None); for _ in 0..(8 * TICKS) { h.advance(dt, dt, 100.0); } }
        let shift = h.ears.iter().map(|e| e.temporary_db()).fold(0.0f32, f32::max);
        hearing.push(serde_json::json!({"place": "hatch_open", "shots": 5, "temporary_shift_db": shift, "tinnitus": h.render().tinnitus.is_some()}));
    }
    for h in &hearing { println!("[main-gun] ears {} after {} shot(s): shift {:.1} dB, tinnitus {}", h["place"], h["shots"],
        h["temporary_shift_db"].as_f64().unwrap_or(0.0), h["tinnitus"]); }
    let receipt = serde_json::json!({"what": "own 88 voice candidates, live capture graph, CPU only", "cases": cases, "hearing": hearing,
        "presentation_trim_db": MAIN_GUN_PRESENT_TRIM_DB});
    std::fs::write(format!("{outdir}/main_gun_candidates_receipt.json"), serde_json::to_string_pretty(&receipt)?)?;
    Ok(())
}
