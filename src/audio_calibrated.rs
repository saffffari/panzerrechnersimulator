//! Calibrated playback: every voice here is placed by its physical level at the listener's ear
//! (`sound_levels` → `sound_propagation`) through the one reproduction curve
//! (`reproduction::tone_map_dbfs`). A waveform is normalised by its own measured short-term
//! A-weighted maximum, so no source carries a gain of its own: the level a voice plays at is
//! `tone_map(LAF at the ear) − LAF,max of the rendered waveform`.
//!
//! World voices are rendered at hear-time: the dry source passes the fitted spectral transfer of
//! its propagation (air, ground, barrier, hull, headset), the head-relative pan and head shadow,
//! and the listener's environment tail; the result is measured and placed.

use std::sync::Arc;

use super::{Audio, EarBus, Sfx};
use crate::acoustics::{self, ReverbParams};
use crate::reproduction::{self, laf_max_dbfs};
use crate::sound_propagation::{EarLevel, SpectralTransfer};

/// Where a measured waveform's LAF,max lies, so repeated renders of the same source can measure
/// a short window around it instead of the whole buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct MeasuredSource {
    /// LAF,max of the dry source played centred, dBFS(A).
    pub laf_dbfs: f32,
    /// Frame of the maximum.
    pub max_frame: usize,
}

/// Measure a mono source played centred at equal power: LAF,max and where it occurs.
pub(super) fn measure_mono(mono: &[f32]) -> MeasuredSource {
    let mut meter = reproduction::LafMeter::new(reproduction::SAMPLE_RATE);
    let g = std::f32::consts::FRAC_1_SQRT_2;
    let (mut best, mut at) = (f32::NEG_INFINITY, 0usize);
    for (frame, &x) in mono.iter().enumerate() {
        let level = meter.push(x * g, x * g);
        if level > best {
            best = level;
            at = frame;
        }
    }
    MeasuredSource { laf_dbfs: best, max_frame: at }
}

/// LAF,max of a stereo render, measured over a window around the expected maximum. The meter
/// starts 0.4 s early (3.2 fast time constants: < 0.2 dB of lost history) and runs 0.6 s past
/// the source's maximum, which covers the delay of the environment tail.
pub(super) fn measure_render_window(stereo: &[f32], max_frame: usize) -> f32 {
    let rate = reproduction::SAMPLE_RATE as usize;
    let frames = stereo.len() / 2;
    let start = max_frame.saturating_sub(rate * 2 / 5).min(frames);
    let end = (max_frame + rate * 3 / 5).min(frames).max(start);
    if end - start < rate / 4 {
        return laf_max_dbfs(stereo);
    }
    laf_max_dbfs(&stereo[start * 2..end * 2])
}

/// Physical level (dB(A) at the operator's ear) of a hand-scale control or foley key. These
/// sources are not in the research report: `sound_levels::HAND_MECHANISM`, `QUIET_HANDLING` and
/// `TIGER_STARTER` are estimates, named in the audit. Callers keep a relative effort factor
/// (their former linear volume, as 20·log10) around the class level [E, authored].
pub fn control_level_dba(key: &str) -> f32 {
    use crate::sound_levels as levels;
    match key {
        "hatch_stop" | "hammer_strike" | "breech" => levels::HAND_MECHANISM.laf_db,
        // A lid rolling on its hinge squeals below its stop.
        "hatch_move" => levels::HAND_MECHANISM.laf_db - 6.0,
        "traverse_jam" => levels::TURRET_TRAVERSE_HYDRAULIC.laf_db + 4.0,
        "starter" => levels::TIGER_STARTER.laf_db,
        "map_open" | "map_close" | "compass_equip" | "compass_stow" => {
            levels::QUIET_HANDLING.laf_db - 10.0
        }
        "trigger" | "p08_mag_out" | "p08_mag_in" | "p08_toggle_pull" | "p08_toggle_close" => {
            levels::QUIET_HANDLING.laf_db + 10.0
        }
        _ => levels::QUIET_HANDLING.laf_db,
    }
}

/// The effort factor of a legacy linear volume, in dB (clamped to ±40 dB).
pub fn effort_db(volume: f32) -> f32 {
    if volume.is_finite() && volume > 0.0 { (20.0 * volume.log10()).clamp(-40.0, 12.0) } else { -200.0 }
}

/// Render a dry mono world source for the ear: spectral transfer, pan and head shadow, then the
/// listener's environment tail. The broadband level is left to the caller's placement.
pub(super) fn render_for_ear(mono: &[f32], ear: &EarLevel, bearing_deg: f32, reverb: ReverbParams)
    -> Vec<f32>
{
    let mut filtered = mono.to_vec();
    SpectralTransfer::fit(&ear.transfer_db, reproduction::SAMPLE_RATE).apply_mono(&mut filtered);
    let dry = acoustics::spatialize_mono(&filtered, bearing_deg, 0.0);
    acoustics::schroeder_reverb(&dry, reverb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound_levels as levels;
    use crate::sound_propagation::{propagate, Atmosphere, Emission, Enclosure, Path};

    /// Isolated source/path audit. It deliberately leaves hearing healthy to distinguish source
    /// timbre and hull transfer from the actual crew's accumulated damage in gameplay captures.
    #[test]
    fn destruction_pressure_path_review() {
        let Ok(prefix) = std::env::var("KESSEL_DESTRUCTION_PATH_REVIEW_PREFIX") else { return; };
        let mut rows = Vec::new();
        for distance_m in [2.0, 22.0] {
            for (path_name, enclosure, headset) in [
                ("same_compartment", Enclosure::OnFoot, true),
                ("through_hull", Enclosure::Buttoned, true),
                ("observer", Enclosure::OnFoot, false),
            ] {
                for (name, sfx, emission) in [
                    ("armour", Sfx::ImpactTank, Emission::from_level(&levels::EXTERIOR_ARMOUR_STRIKE_10M)),
                    ("kill", Sfx::KillExplosion, Emission::blast(levels::VEHICLE_KILL_TNT_KG)),
                    ("catastrophic", Sfx::TurretPop, Emission::blast(levels::AMMUNITION_DETONATION_TNT_KG)),
                ] {
                    let audio = Audio::new_capture();
                    let path = Path { distance_m, atmosphere: Atmosphere::WINTER, occluded: false,
                        enclosure, headset };
                    let ear = propagate(&emission, &path);
                    let reverb = if path_name == "observer" { ReverbParams::OPEN_SNOW }
                        else { ReverbParams::STEEL_INTERIOR };
                    audio.play_world_at_ear(sfx, &ear, 0.0, reverb);
                    let mut out = audio.render_capture_frames(3 * 44_100).unwrap();
                    out.drain(..reproduction::MASTER_LATENCY_FRAMES * 2);
                    assert!(out.iter().all(|v| v.is_finite()));
                    let file = format!("{prefix}_{name}_{path_name}_{distance_m:.0}m.wav");
                    assert!(!std::path::Path::new(&file).exists(), "preserve review: {file}");
                    acoustics::write_wav_stereo(&file, &out, acoustics::SAMPLE_RATE).unwrap();
                    let master = audio.master_telemetry();
                    rows.push(serde_json::json!({"source":name,"path":path_name,"distance_m":distance_m,
                        "ear_laf_dba":ear.laf_db,"ear_lpk_db":ear.lpk_db,"transfer_db":ear.transfer_db,
                        "output_laf_dbfs":laf_max_dbfs(&out),"output_peak_dbfs":reproduction::peak_dbfs(&out),
                        "master":master,"wav":file}));
                }
            }
        }
        let file = format!("{prefix}.json");
        assert!(!std::path::Path::new(&file).exists(), "preserve review: {file}");
        std::fs::write(file, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    }

    #[test]
    #[ignore = "timing diagnostic"]
    fn print_world_render_cost() {
        let audio = Audio::new_capture();
        let mono = audio.world_source(Sfx::KillExplosion).unwrap();
        let ear = propagate(&Emission::blast(0.71), &Path { distance_m: 400.0,
            atmosphere: Atmosphere::WINTER, occluded: true, enclosure: Enclosure::Buttoned,
            headset: true });
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let rendered = render_for_ear(&mono, &ear, 40.0, ReverbParams::STEEL_INTERIOR);
            let _ = measure_render_window(&rendered, 2000);
        }
        println!("render+measure {:.2} ms per event, {} frames",
            start.elapsed().as_secs_f64() * 100.0, mono.len());
        let _ = levels::TABLE.len();
    }

    /// The own gun at the ear through the headset, as the game plays it.
    fn own_gun_ear(level: &levels::SourceLevel) -> f32 {
        propagate(&Emission::from_level(level), &Path::at_ear(true)).laf_db
    }

    #[test]
    fn own_gun_recording_crest_matches_the_exposure_constant() {
        let audio = Audio::new_capture();
        let Some(mono) = audio.feedback_source("gun_interior") else {
            eprintln!("verified bank absent: crest check skipped");
            return;
        };
        let stereo: Vec<f32> = mono.iter().flat_map(|v| [*v, *v]).collect();
        let crest = reproduction::peak_dbfs(&stereo) - laf_max_dbfs(&stereo);
        assert!((crest - reproduction::CREST_GUN_INTERIOR_DB).abs() < 0.05,
            "measured crest {crest} vs constant {}", reproduction::CREST_GUN_INTERIOR_DB);
    }

    #[test]
    fn own_gun_peaks_near_full_scale_buttoned_and_head_out_meets_the_limiter() {
        let render = |sfx: Sfx, level: &levels::SourceLevel| {
            let audio = Audio::new_capture();
            audio.play_at_ear(sfx, own_gun_ear(level));
            let out = audio.render_capture_frames(3 * 44_100).unwrap();
            (out, audio.master_telemetry())
        };
        let verified = Audio::new_capture().feedback_source("gun_interior").is_some();
        let (buttoned, t_buttoned) = render(Sfx::PlayerGunInt1, &levels::OWN_88_BUTTONED);
        let (head_out, t_head_out) = render(Sfx::PlayerGunExt1, &levels::OWN_88_HEAD_OUT);
        let peak = reproduction::peak_dbfs(&buttoned);
        if verified {
            // The owner-verified recording: its peak stays near full scale, unlimited.
            assert!((-2.5..=-1.0).contains(&peak), "buttoned peak {peak} dBFS");
            assert_eq!(t_buttoned["limited_frames"], 0, "the buttoned gun is not limited");
            // Head-out its LAF is 13 dB higher physically, ~6 dB after the curve: it meets the
            // limiter, which trims its peak (SL §11: ~6 dB).
            assert!(t_head_out["deepest_gain_reduction_db"].as_f64().unwrap() < -2.0);
        }
        assert!(reproduction::peak_dbfs(&head_out) <= 20.0 * reproduction::LIMITER_CEILING.log10() + 0.01);
        // Short-term level: the curve puts head-out 6.5 dB above buttoned (13 dB physical); the
        // limiter, holding the head-out peak at −1 dBFS with its 200 ms release, takes part of
        // that back. Head-out stays louder; the ear model carries the rest (182 dB vs 163 dB).
        let difference = laf_max_dbfs(&head_out) - laf_max_dbfs(&buttoned);
        let curve = reproduction::tone_map_dbfs(own_gun_ear(&levels::OWN_88_HEAD_OUT))
            - reproduction::tone_map_dbfs(own_gun_ear(&levels::OWN_88_BUTTONED));
        assert!((curve - 6.5).abs() < 0.1, "{curve}");
        if verified {
            assert!(difference > 1.0 && difference < curve, "head-out minus buttoned {difference}");
        }
    }

    #[test]
    fn the_engine_bed_lands_at_its_ear_level_at_any_revs_with_no_cap() {
        use crate::audio::EngineTelemetry;
        let level = |telemetry: EngineTelemetry, ear: f32| {
            let audio = Audio::new_capture();
            for _ in 0..240 {
                audio.set_engine_at_ear(telemetry, ear, 1.0 / 60.0);
                audio.render_capture_frames(735).unwrap();
            }
            let mut out = Vec::new();
            for _ in 0..120 {
                audio.set_engine_at_ear(telemetry, ear, 1.0 / 60.0);
                out.extend(audio.render_capture_frames(735).unwrap());
            }
            let mut meter = reproduction::LafMeter::new(reproduction::SAMPLE_RATE);
            let mut sum = 0.0f64;
            for f in out.chunks_exact(2) { sum += 10f64.powf(f64::from(meter.push(f[0], f[1])) / 10.0); }
            (10.0 * (sum / (out.len() / 2) as f64).log10()) as f32
        };
        let idle = EngineTelemetry::automatic(0.0, 0.0, 0.0, true);
        let road = EngineTelemetry::automatic(0.8, 0.9, 0.5, true);
        for telemetry in [idle, road] {
            for ear in [93.0, 110.0, 115.0] {
                let got = level(telemetry, ear);
                let want = reproduction::tone_map_dbfs(ear);
                assert!((got - want).abs() < 2.0, "rpm {} at {ear} dB(A): {got} vs {want}",
                    telemetry.rpm_norm);
            }
        }
        // The own gun sits ~15 dB (LAF) above the running bed (110 dB(A) cross-country), 12 dB
        // above the road bed (115); physically 29 and 24 dB (SL §11 rule 3).
        let gun = reproduction::tone_map_dbfs(levels::OWN_88_BUTTONED.laf_db);
        let bed = reproduction::tone_map_dbfs(levels::TIGER_INTERIOR_CROSS_COUNTRY.laf_db);
        assert!((gun - bed - 14.5).abs() < 0.01);
        let road = reproduction::tone_map_dbfs(levels::TIGER_INTERIOR_ROAD.laf_db);
        assert!((gun - road - 12.0).abs() < 0.01);
    }

    #[test]
    fn world_voices_are_placed_by_their_ear_level_not_their_waveform() {
        // Two different waveforms at the same ear level play at the same short-term level; the
        // same waveform 20 dB quieter at the ear follows the selected global reproduction curve.
        let ear_at = |laf: f32| {
            let mut ear = propagate(&Emission::from_level(&levels::GUN_76_F34_SIDE_10M),
                &Path::at_ear(false));
            ear.laf_db = laf;
            ear
        };
        let level = |sfx: Sfx, laf: f32| {
            let audio = Audio::new_capture();
            audio.play_world_at_ear(sfx, &ear_at(laf), 0.0, ReverbParams::OPEN_SNOW);
            let out = audio.render_capture_frames(3 * 44_100).unwrap();
            laf_max_dbfs(&out)
        };
        let gun = level(Sfx::EnemyGun, 100.0);
        let ground = level(Sfx::ImpactGround, 100.0);
        assert!((gun - ground).abs() < 0.5, "same ear level, same digital level: {gun} {ground}");
        assert!((gun - reproduction::tone_map_dbfs(100.0)).abs() < 0.5, "{gun}");
        let quieter = level(Sfx::EnemyGun, 80.0);
        let expected = reproduction::tone_map_dbfs(100.0) - reproduction::tone_map_dbfs(80.0);
        assert!((gun - quieter - expected).abs() < 0.5, "{gun} {quieter}, expected {expected}");
    }

    #[test]
    fn small_arm_variants_keep_calibration_and_buttoned_hull_filters_the_crack() {
        let audio = Audio::new_capture();
        let emission = Emission::from_level(&levels::RIFLE_1M_SIDE);
        let path = Path { distance_m:30.0, atmosphere:Atmosphere::WINTER, occluded:false,
            enclosure:Enclosure::OnFoot, headset:false };
        let open = propagate(&emission,&path);
        let closed = propagate(&emission,&Path { enclosure:Enclosure::Buttoned, headset:true, ..path });
        assert!(open.laf_db-closed.laf_db > 35.0);
        assert!(closed.transfer_db[7]-closed.transfer_db[2] > 25.0);
        let mut fingerprints = std::collections::HashSet::new();
        for _ in 0..12 {
            let mono=audio.world_source(Sfx::RifleShot).unwrap();
            let key=format!("world:RifleShot:{:016x}",super::super::buffer_checksum(&mono));
            assert!(fingerprints.insert(key.clone()));
            let measured=audio.measured_world_source(key,&mono);
            let rendered=render_for_ear(&mono,&open,22.0,ReverbParams::OPEN_SNOW);
            let level=measure_render_window(&rendered,measured.max_frame);
            let gain=reproduction::voice_gain(open.laf_db,level);
            let placed:Vec<_>=rendered.iter().map(|x|x*gain).collect();
            assert!((laf_max_dbfs(&placed)-reproduction::tone_map_dbfs(open.laf_db)).abs()<0.15);
        }
        assert_eq!(audio.level_cache.borrow().len(),12,"same-duration variants must not share a stale measurement");
    }
}

impl Audio {
    /// The dry world source for `sfx`: the verified recording where one replaces it, otherwise the
    /// deterministic procedural identity, with an opted-in v1 character layer mixed in at its
    /// receipt-bound share (so the layer is inside the calibrated level, not added on top).
    pub(super) fn world_source(&self, sfx: Sfx) -> Option<Arc<[f32]>> {
        if let Some(report) = self.small_arm_reports.next(sfx) { return Some(report); }
        let base = super::feedback::replacement_key(sfx)
            .and_then(|key| self.feedback_source(key))
            .or_else(|| self.world_mono.get(sfx.index()).cloned())?;
        let Some(layer) = self.next_pizza_layer(sfx) else { return Some(base) };
        let len = base.len().max(layer.world_mono.len());
        let mixed: Vec<f32> = (0..len)
            .map(|i| base.get(i).copied().unwrap_or(0.0)
                + layer.gain * layer.world_mono.get(i).copied().unwrap_or(0.0))
            .collect();
        Some(Arc::from(mixed))
    }

    fn measured_world_source(&self, key: String, mono: &[f32]) -> MeasuredSource {
        if let Some(found) = self.level_cache.borrow().get(&key) {
            return *found;
        }
        let measured = measure_mono(mono);
        self.level_cache.borrow_mut().insert(key, measured);
        measured
    }

    /// Play a world source at its physical level at the ear. `bearing_deg` is head-relative;
    /// `ear` comes from `sound_propagation::propagate` for the live path.
    pub fn play_world_at_ear(&self, sfx: Sfx, ear: &EarLevel, bearing_deg: f32, reverb: ReverbParams) {
        let Some(mono) = self.world_source(sfx) else { return };
        let key = if super::small_arms::is_variant(sfx) {
            format!("world:{sfx:?}:{:016x}", super::buffer_checksum(&mono))
        } else { format!("world:{sfx:?}:{}", mono.len()) };
        let measured = self.measured_world_source(key, &mono);
        self.place_world_render(&mono, measured, ear, bearing_deg, reverb, EarBus::World);
    }

    /// Play a synthesized mono world voice (a parametric report, a small-arms contact) at its
    /// physical level at the ear.
    pub fn play_world_mono_at_ear(&self, mono: &[f32], ear: &EarLevel, bearing_deg: f32,
        reverb: ReverbParams)
    {
        if mono.is_empty() {
            return;
        }
        let measured = measure_mono(mono);
        self.place_world_render(mono, measured, ear, bearing_deg, reverb, EarBus::World);
    }

    fn place_world_render(&self, mono: &[f32], measured: MeasuredSource, ear: &EarLevel,
        bearing_deg: f32, reverb: ReverbParams, bus: EarBus)
    {
        if !ear.laf_db.is_finite() || !measured.laf_dbfs.is_finite() {
            return;
        }
        let rendered = render_for_ear(mono, ear, bearing_deg, reverb);
        let level = measure_render_window(&rendered, measured.max_frame);
        let gain = reproduction::voice_gain(ear.laf_db, level);
        self.play_stereo_on(rendered, gain, bus);
    }

    /// Play an already spatialised stereo buffer (a strike on the listener's own vehicle, a local
    /// contact) at the physical level `ear_laf_dba` on `bus`.
    pub fn play_stereo_at_ear(&self, samples: Vec<f32>, ear_laf_dba: f32, bus: EarBus) {
        if samples.is_empty() {
            return;
        }
        let gain = reproduction::voice_gain(ear_laf_dba, laf_max_dbfs(&samples));
        self.play_stereo_on(samples, gain, bus);
    }

    /// The direct (non-spatial) waveform `play(sfx)` starts: the verified recording where one
    /// replaces it, otherwise the loaded clip.
    pub(crate) fn direct_waveform(&self, sfx: Sfx) -> Option<Vec<f32>> {
        if let Some(mono) = super::feedback::replacement_key(sfx).and_then(|k| self.feedback_source(k)) {
            return Some(mono.iter().flat_map(|v| [*v, *v]).collect());
        }
        let clip = self.clips.get(sfx.index()).and_then(|c| c.as_ref())?;
        let samples: Vec<f32> = clip.source().map(|s: i16| f32::from(s) / f32::from(i16::MAX)).collect();
        Some(if clip.channels == 1 { samples.iter().flat_map(|v| [*v, *v]).collect() } else { samples })
    }

    /// Measured LAF,max (dBFS(A)) of `play(sfx)`'s waveform at unity gain, cached.
    pub fn direct_laf_dbfs(&self, sfx: Sfx) -> f32 {
        let key = format!("direct:{sfx:?}");
        if let Some(found) = self.level_cache.borrow().get(&key) {
            return found.laf_dbfs;
        }
        let laf = self.direct_waveform(sfx).map_or(f32::NEG_INFINITY, |w| laf_max_dbfs(&w));
        self.level_cache.borrow_mut().insert(key, MeasuredSource { laf_dbfs: laf, max_frame: 0 });
        laf
    }

    /// Where `play_at_ear(sfx)`'s sound begins (s after its first sample) and its envelope from
    /// there, cached: the impulse the voice carries reaches the ears when its sound begins (the
    /// verified gun recordings open with ~50 ms of near-silence) and the recording, as heard,
    /// drives the middle-ear reflex.
    pub fn direct_drive(&self, sfx: Sfx) -> Option<(f32, crate::hearing::DriveEnvelope)> {
        let key = format!("direct:{sfx:?}");
        if let Some(found) = self.drive_cache.borrow().get(&key) {
            return Some(found.clone());
        }
        let drive = crate::hearing::DriveEnvelope::of_voice(&self.direct_waveform(sfx)?);
        self.drive_cache.borrow_mut().insert(key, drive.clone());
        Some(drive)
    }

    /// Play `sfx` at the ear, not spatialised (the own gun, the listener's own weapons), at the
    /// physical level `ear_laf_dba`. The same waveform `play` would use, placed by calibration.
    pub fn play_at_ear(&self, sfx: Sfx, ear_laf_dba: f32) {
        let gain = reproduction::voice_gain(ear_laf_dba, self.direct_laf_dbfs(sfx));
        if gain <= 0.0 {
            return;
        }
        if self.play_feedback_sfx(sfx, gain) {
            return;
        }
        self.play_at_volume_without_layers(sfx, gain);
    }

    /// Measured LAF,max (dBFS(A)) of an interleaved stereo waveform, cached under `key`.
    pub(super) fn cached_laf_dbfs(&self, key: &str, waveform: impl FnOnce() -> Vec<f32>) -> f32 {
        if let Some(found) = self.level_cache.borrow().get(key) {
            return found.laf_dbfs;
        }
        let laf = laf_max_dbfs(&waveform());
        self.level_cache.borrow_mut()
            .insert(key.to_string(),
                MeasuredSource { laf_dbfs: laf, max_frame: 0 });
        laf
    }

    /// Linear gain placing a loaded loop/clip at `ear_laf_dba`.
    pub(super) fn clip_gain(&self, key: &str, clip: &super::Clip, ear_laf_dba: f32) -> f32 {
        let laf = self.cached_laf_dbfs(key, || {
            let samples: Vec<f32> =
                clip.source().map(|s: i16| f32::from(s) / f32::from(i16::MAX)).collect();
            if clip.channels == 1 { samples.iter().flat_map(|v| [*v, *v]).collect() } else { samples }
        });
        reproduction::voice_gain(ear_laf_dba, laf)
    }

    /// Diagnostics: the master stage's limiter telemetry.
    pub fn master_telemetry(&self) -> serde_json::Value {
        self.master.snapshot()
    }

    /// Frames of latency the master stage adds (the limiter's lookahead).
    #[cfg(test)]
    pub fn master_latency_frames(&self) -> usize {
        reproduction::MASTER_LATENCY_FRAMES
    }
}
