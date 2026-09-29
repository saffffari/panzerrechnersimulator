//! Device-free A/B fixture for the impact mix: contrast before a hit, the layers of the hit
//! itself, and the listener's hearing afterward. Every take runs through the ordinary gameplay
//! `Audio` sink graph at the game's own hit volumes and 60 Hz control cadence. Hits and the world
//! bed are explicitly staged auditions, not recorded combat or measured Tiger acoustics.
//!
//! Each scene renders twice with identical control sequences: once with the hit voice and trauma
//! layer, once without. Rodio sums independent voices, so `full - world` is the isolated hit.
use crate::{
    acoustics as a,
    audio::{Audio, EngineTelemetry},
};
use serde_json::{json, Value};
use std::path::Path;

const HZ: usize = a::SAMPLE_RATE as usize;
const TICKS: usize = 60;
const FRAMES_PER_TICK: usize = HZ / TICKS;
const SECONDS: usize = 11;
/// The hit lands on this 60 Hz control tick, leaving four seconds for the beds to settle.
const HIT_TICK: usize = 4 * TICKS;
/// Mirrors game.rs `VOL_HIT_ON_PLAYER`, the live impact-bus gain for a strike on the player.
const VOL_HIT_ON_PLAYER: f32 = 2.2;

#[derive(Clone, Copy)]
struct Scene {
    slug: &'static str,
    signature: a::ImpactSignature,
    seed: u32,
    /// Commander head-out in the open blizzard instead of buttoned up inside the turret.
    head_out: bool,
}

const SCENES: [Scene; 4] = [
    Scene { slug: "close_side_penetration", signature: a::ImpactSignature::CloseT34SidePenetration,
        seed: 0xC105_E001, head_out: false },
    Scene { slug: "marginal_ricochet", signature: a::ImpactSignature::LongRangeT34Marginal,
        seed: 0x7A11_0002, head_out: false },
    Scene { slug: "heavy_122mm_penetration", signature: a::ImpactSignature::HeavyObliquePenetration,
        seed: 0x0122_0003, head_out: false },
    Scene { slug: "close_side_penetration_head_out",
        signature: a::ImpactSignature::CloseT34SidePenetration, seed: 0xC105_E004, head_out: true },
];

/// A Tiger moving through a blizzard with the turret laying: the steady combat bed that a hit has
/// to cut through, buttoned up or with the commander head-out. Values are staged, matching the
/// live per-frame control calls.
fn world_tick(audio: &mut Audio, head_out: bool) {
    let dt = 1.0 / TICKS as f32;
    audio.set_weather_ambience(0.72, if head_out { 0.0 } else { 0.93 }, 17.0);
    audio.set_engine_telemetry(EngineTelemetry::automatic(0.22, 0.50, 0.42, true), dt);
    audio.set_turret_traverse(0.25, !head_out, dt);
    audio.set_mg(false, !head_out, dt);
}

/// The live hit event, built exactly as `resolve_authoritative_vehicle_impact` builds it for a
/// round that reaches the player's armor without a separately emitted pressure front.
fn hit_event(scene: Scene) -> (a::ImpactParams, a::DamageSoundEvent, a::ConcussionProfile, f32) {
    let (params, context) = a::impact_signature_params(scene.signature, scene.seed);
    let air_delay = params.station_dist_m / crate::pressure_front::speed_of_sound_m_s();
    let event = a::synth_damage_impact_with_pressure_delay(params, context, air_delay);
    // None of the staged scenes is a >=100 mm bounce beside the seat, so the game's separate
    // heavy-bounce fallback never applies; the resolved damage profile owns the hearing state.
    let concussion = event.profile.concussion;
    (params, event, concussion, VOL_HIT_ON_PLAYER)
}

/// The hit voice exactly as the impact bus delivers it for its first `IMPACT_FULL_S`: scaled by
/// the live gain, then held under the fixed ceiling.
fn delivered_voice(scene: Scene) -> Vec<f32> {
    let (_, event, _, volume) = hit_event(scene);
    let mut voice: Vec<f32> = event.main.iter().map(|x| x * volume).collect();
    a::attenuate_over_peak(&mut voice, a::IMPACT_CEILING);
    voice
}

/// Render one scene through the live mix policy. `with_hit=false` keeps every control identical
/// (including contrast and the hearing state after the hit) but omits the hit voice and its
/// trauma layer.
fn render(scene: Scene, with_hit: bool) -> (Vec<f32>, Value) {
    let (params, event, concussion, volume) = hit_event(scene);
    let dt = 1.0 / TICKS as f32;
    // Staged prediction: the game's read-only probe sees this round strike the hull at HIT_TICK.
    let depth = a::pre_impact_depth_db(
        0.5 * params.mass_kg * params.velocity * params.velocity,
        params.station_dist_m,
    );
    let mut contrast_db = 0.0;
    let mut audio = Audio::new_capture();
    let mut samples = Vec::with_capacity(SECONDS * HZ * 2);
    for tick in 0..SECONDS * TICKS {
        // Game order: hearing is set near the top of update(); the round that lands this frame
        // is advanced later, then the pre-hit contrast follows the remaining predictions.
        if tick > HIT_TICK && concussion.duration_s() > 0.0 {
            let elapsed = (tick - HIT_TICK) as f32 / TICKS as f32;
            let shift = a::concussion_threshold_shift(elapsed, concussion);
            let (left, right) = a::ear_threshold_shifts(shift, params.bearing_deg);
            audio.set_hearing(a::concussion_duck_profile(elapsed, concussion), left, right);
        }
        world_tick(&mut audio, scene.head_out);
        let target_db = if tick < HIT_TICK {
            a::pre_impact_world_db((HIT_TICK - tick) as f32 * dt, depth)
        } else {
            0.0
        };
        contrast_db = a::contrast_step_db(contrast_db, target_db, dt);
        audio.set_contrast(10f32.powf(contrast_db / 20.0));
        if tick == HIT_TICK && with_hit {
            audio.play_impact_stereo(event.main.clone(), volume);
            if concussion.duration_s() > 0.0 {
                let fatigue = if event.tinnitus.is_empty() {
                    a::synth_tinnitus(0.30 + concussion.tinnitus_gain, params.seed)
                } else {
                    event.tinnitus.clone()
                };
                audio.play_trauma_stereo(a::after_strike_onset(fatigue), 0.86);
            }
        }
        samples.extend(audio.render_capture_frames(FRAMES_PER_TICK).expect("device-free capture"));
    }
    let meta = json!({
        "profile": format!("{:?} / {:?}", event.profile.kind, event.profile.severity),
        "concussion": {"floor_gain": concussion.floor_gain, "hold_s": concussion.hold_s,
            "recovery_s": concussion.recovery_s, "tinnitus_gain": concussion.tinnitus_gain},
        "hit_volume": volume, "pre_hit_depth_db": depth, "peak_threshold_shift":
            a::concussion_peak_shift(concussion),
        "caliber_mm": params.caliber_mm, "penetrated": params.penetrated, "head_out": scene.head_out,
        "bearing_deg": params.bearing_deg, "station_dist_m": params.station_dist_m,
    });
    (samples, meta)
}

// ── Measurements ────────────────────────────────────────────────────────────────────────────────

fn mono(stereo: &[f32]) -> Vec<f32> {
    stereo.chunks_exact(2).map(|f| (f[0] + f[1]) * std::f32::consts::FRAC_1_SQRT_2).collect()
}

fn frames(seconds: f32) -> usize {
    (seconds.max(0.0) * HZ as f32).round() as usize
}

fn db(x: f64) -> f64 {
    20.0 * x.max(1.0e-9).log10()
}

fn rms(signal: &[f32]) -> f64 {
    if signal.is_empty() {
        return 0.0;
    }
    (signal.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / signal.len() as f64).sqrt()
}

/// RBJ biquad, direct form I, run over a mono signal.
#[derive(Clone, Copy)]
struct Biquad { b: [f64; 3], a: [f64; 2] }

impl Biquad {
    fn bandpass(f0: f64, q: f64) -> Self {
        let w0 = std::f64::consts::TAU * f0 / HZ as f64;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self { b: [alpha / a0, 0.0, -alpha / a0], a: [-2.0 * w0.cos() / a0, (1.0 - alpha) / a0] }
    }

    fn highpass(f0: f64) -> Self {
        let w0 = std::f64::consts::TAU * f0 / HZ as f64;
        let alpha = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let c = w0.cos();
        Self {
            b: [(1.0 + c) / 2.0 / a0, -(1.0 + c) / a0, (1.0 + c) / 2.0 / a0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
        }
    }

    fn run(self, x: &[f32]) -> Vec<f32> {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        x.iter()
            .map(|&s| {
                let s = f64::from(s);
                let y = self.b[0] * s + self.b[1] * x1 + self.b[2] * x2 - self.a[0] * y1 - self.a[1] * y2;
                x2 = x1;
                x1 = s;
                y2 = y1;
                y1 = y;
                y as f32
            })
            .collect()
    }
}

/// Time (ms after the hit) at which a band's 1 ms peak envelope reaches its maximum, and the
/// first time it reaches half of that maximum.
fn band_timing(isolated_mono: &[f32], band: Biquad, hit: usize, window_s: f32) -> Value {
    let y = band.run(isolated_mono);
    let block = HZ / 1000;
    let end = (hit + frames(window_s)).min(y.len());
    let envelope: Vec<f32> = y[hit..end]
        .chunks(block)
        .map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs())))
        .collect();
    let (peak_ms, peak) = envelope
        .iter()
        .enumerate()
        .fold((0usize, 0.0f32), |best, (i, &v)| if v > best.1 { (i, v) } else { best });
    let half_ms = envelope.iter().position(|&v| v >= 0.5 * peak).unwrap_or(peak_ms);
    json!({"peak_ms": peak_ms, "half_ms": half_ms, "peak": peak})
}

/// Fraction of energy above `cutoff_hz`, in dB. A level-only duck leaves this unchanged; losing
/// high hearing thresholds lowers it.
fn share_above_db(mono_window: &[f32], cutoff_hz: f64) -> f64 {
    let total = rms(mono_window);
    let high = rms(&Biquad::highpass(cutoff_hz).run(mono_window));
    db(high / total.max(1.0e-12))
}

fn hf_share_db(mono_window: &[f32]) -> f64 {
    share_above_db(mono_window, 2_000.0)
}

/// Timing, peak and crest of one hit voice's three measured layers inside `window_s`.
fn voice_layers(voice_stereo: &[f32]) -> Value {
    let voice = mono(voice_stereo);
    let bands = json!({
        "low_35_110_hz": band_timing(&voice, Biquad::bandpass(62.0, 0.9), 0, 0.30),
        "body_170_600_hz": band_timing(&voice, Biquad::bandpass(320.0, 0.75), 0, 0.30),
        "presence_1_4_khz": band_timing(&voice, Biquad::bandpass(2_000.0, 0.8), 0, 0.30),
    });
    let times: Vec<f64> = ["low_35_110_hz", "body_170_600_hz", "presence_1_4_khz"]
        .iter()
        .map(|k| bands[k]["peak_ms"].as_f64().unwrap_or(0.0))
        .collect();
    let spread = times.iter().cloned().fold(f64::MIN, f64::max)
        - times.iter().cloned().fold(f64::MAX, f64::min);
    let peak = voice_stereo.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let first = rms(&voice[..frames(0.15).min(voice.len())]);
    json!({"band_timing": bands, "band_peak_spread_ms": spread, "peak": peak,
        "rms_150ms_db": db(first), "crest_150ms_db": db(f64::from(peak) / first.max(1.0e-9)),
        "rms_150ms_over_peak_db": db(first / f64::from(peak).max(1.0e-9))})
}

fn window(signal: &[f32], start_s: f32, end_s: f32) -> &[f32] {
    let a = frames(start_s).min(signal.len());
    let b = frames(end_s).min(signal.len()).max(a);
    &signal[a..b]
}

fn measure(full: &[f32], world: &[f32]) -> Value {
    let t0 = HIT_TICK as f32 / TICKS as f32;
    let hit = HIT_TICK * FRAMES_PER_TICK;
    let full_m = mono(full);
    let world_m = mono(world);
    let isolated: Vec<f32> = full_m.iter().zip(&world_m).map(|(f, w)| f - w).collect();
    // HF share of the settled world with a 1.5 s filter warm-up before the reference window.
    let reference = window(&world_m, t0 - 1.6, t0 - 0.6);
    let reference_rms = rms(reference);
    let reference_hf = hf_share_db(window(&world_m, t0 - 3.0, t0 - 0.6));
    let reference_mid = share_above_db(window(&world_m, t0 - 3.0, t0 - 0.6), 700.0);
    let pre = rms(window(&full_m, t0 - 0.10, t0));
    let hit_window = rms(window(&full_m, t0, t0 + 0.15));
    let hit_peak = full[hit * 2..(hit + frames(0.5)) * 2]
        .iter()
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let isolated_peak = isolated[hit..hit + frames(0.5)].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let isolated_window = rms(&isolated[hit..hit + frames(0.15)]);
    let aftermath: Vec<Value> = [(0.30, 1.30), (1.3, 2.3), (2.5, 3.5), (4.5, 5.5), (6.0, 7.0)]
        .iter()
        .map(|&(a0, a1)| {
            let w = window(&world_m, t0 + a0, t0 + a1);
            json!({"window_s": [a0, a1], "world_level_db": db(rms(w) / reference_rms),
                "world_hf_share_change_db": hf_share_db(w) - reference_hf,
                "world_above_700hz_share_change_db": share_above_db(w, 700.0) - reference_mid})
        })
        .collect();
    let bands = json!({
        "low_35_110_hz": band_timing(&isolated, Biquad::bandpass(62.0, 0.9), hit, 0.30),
        "body_170_600_hz": band_timing(&isolated, Biquad::bandpass(320.0, 0.75), hit, 0.30),
        "presence_1_4_khz": band_timing(&isolated, Biquad::bandpass(2_000.0, 0.8), hit, 0.30),
    });
    let peak_times: Vec<f64> = ["low_35_110_hz", "body_170_600_hz", "presence_1_4_khz"]
        .iter()
        .map(|k| bands[k]["peak_ms"].as_f64().unwrap_or(0.0))
        .collect();
    let spread = peak_times.iter().cloned().fold(f64::MIN, f64::max)
        - peak_times.iter().cloned().fold(f64::MAX, f64::min);
    json!({
        "contrast": {
            "pre_hit_world_drop_db": db(pre / reference_rms),
            "hit_window_over_pre_hit_db": db(hit_window / pre.max(1.0e-9)),
        },
        "layers": {
            "band_timing": bands,
            "band_peak_spread_ms": spread,
            "isolated_hit_peak": isolated_peak,
            "isolated_hit_150ms_rms_db": db(isolated_window),
            "isolated_hit_150ms_crest_db": db(f64::from(isolated_peak) / isolated_window.max(1.0e-9)),
        },
        "ceiling": {
            "hit_mix_peak": hit_peak,
            "mix_peak": full.iter().fold(0.0f32, |m, v| m.max(v.abs())),
            "samples_at_or_over_full_scale": full.iter().filter(|v| v.abs() >= 1.0).count(),
        },
        "consequence": aftermath,
    })
}

/// The player's own 88 mm report (interior and exterior perspective) through the same graph, as
/// a loudness-hierarchy reference for the hits.
fn own_gun_reference() -> Value {
    let mut out = serde_json::Map::new();
    for (name, sfx) in [
        ("own_gun_interior", crate::audio::Sfx::PlayerGunInt1),
        ("own_gun_exterior", crate::audio::Sfx::PlayerGunExt1),
    ] {
        let audio = Audio::new_capture();
        audio.play(sfx);
        let samples = audio.render_capture_frames(HZ).expect("device-free capture");
        let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let first = rms(&mono(&samples)[..frames(0.15)]);
        out.insert(name.into(), json!({"peak": peak, "rms_150ms_db": db(first)}));
    }
    Value::Object(out)
}

/// Render every scene, write one unity-gain WAV per full take (clamped exactly as the device
/// clamps) and a receipt. Refuses to overwrite a prior run.
pub fn run(prefix: &str) -> anyhow::Result<Value> {
    let receipt_path = format!("{prefix}_receipt.json");
    anyhow::ensure!(!Path::new(&receipt_path).exists(), "refusing to overwrite {receipt_path}");
    for scene in SCENES {
        let wav = format!("{prefix}_{}.wav", scene.slug);
        anyhow::ensure!(!Path::new(&wav).exists(), "refusing to overwrite {wav}");
    }
    if let Some(parent) = Path::new(prefix).parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut takes = Vec::new();
    for scene in SCENES {
        let (full, meta) = render(scene, true);
        let (world, _) = render(scene, false);
        anyhow::ensure!(full.iter().chain(&world).all(|x| x.is_finite()), "nonfinite sample");
        let path = format!("{prefix}_{}.wav", scene.slug);
        a::write_wav_stereo(&path, &full, a::SAMPLE_RATE)?;
        let mut metrics = measure(&full, &world);
        let voice = delivered_voice(scene);
        metrics["voice"] = voice_layers(&voice);
        a::write_wav_stereo(format!("{prefix}_{}_voice.wav", scene.slug), &voice, a::SAMPLE_RATE)?;
        takes.push(json!({"scene": scene.slug, "path": path, "event": meta,
            "hit_at_s": HIT_TICK as f32 / TICKS as f32, "metrics": metrics}));
    }
    let receipt = json!({
        "schema": "kessel.impact-mix-review.v1",
        "sample_rate": HZ, "channels": 2, "control_rate_hz": TICKS, "export_gain": 1.0,
        "reference_levels": own_gun_reference(),
        "scope": "Staged hits through the ordinary gameplay Audio sink graph at live hit volumes; \
            world bed is a buttoned Tiger moving in a blizzard with the turret laying. No recorded \
            combat, measured Tiger acoustics, or speaker/headphone audition is claimed.",
        "takes": takes,
    });
    std::fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "writes an explicitly requested device-free impact mix review"]
    fn export_impact_mix_review() {
        let prefix = std::env::var("KESSEL_IMPACT_REVIEW_PREFIX")
            .expect("set an unused flat captures/astra prefix");
        let receipt = super::run(&prefix).unwrap();
        println!("{}", serde_json::to_string_pretty(&receipt).unwrap());
    }
}
