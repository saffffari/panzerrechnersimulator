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

    #[test]
    fn world_voices_are_placed_by_their_ear_level_not_their_waveform() {
        // Two different waveforms at the same ear level play at the same short-term level; the
        // same waveform 20 dB quieter at the ear plays 20 dB quieter (linear part of the curve).
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
        assert!((gun - quieter - 20.0).abs() < 0.5, "{gun} {quieter}");
    }
}

impl Audio {
    /// The dry world source for `sfx`: the verified recording where one replaces it, otherwise the
    /// deterministic procedural identity, with an opted-in v1 character layer mixed in at its
    /// receipt-bound share (so the layer is inside the calibrated level, not added on top).
    pub(super) fn world_source(&self, sfx: Sfx) -> Option<Arc<[f32]>> {
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
        let key = format!("world:{sfx:?}:{}", mono.len());
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
    fn direct_waveform(&self, sfx: Sfx) -> Option<Vec<f32>> {
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

    /// Diagnostics: the master stage's limiter telemetry.
    pub fn master_telemetry(&self) -> serde_json::Value {
        self.master.snapshot()
    }

    /// Frames of latency the master stage adds (the limiter's lookahead).
    pub fn master_latency_frames(&self) -> usize {
        reproduction::MASTER_LATENCY_FRAMES
    }
}
