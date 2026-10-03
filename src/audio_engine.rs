//! Streaming procedural Maybach. Combustion cadence follows the existing authored RPM mapping;
//! the same source's hull/rumble filters and decay times stay in output Hz/seconds. No new source
//! energy, sub-bass oscillator, or per-source loudness lift is introduced. A whole-voice LAF table
//! conserves the existing calibrated engine level. Optional recorded loops keep their old route.
use rodio::Source;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const RATE: u32 = crate::acoustics::SAMPLE_RATE;
const SEED: u32 = 3_801;
const SPEEDS: [f32; 7] = [0.6, 1.0, 1.4, 2.0, 2.8, 3.6, 4.5];
const LOADS:[f32;3]=[0.0,0.4,1.0];

pub(super) struct Control {
    speed: AtomicU32,
    load: AtomicU32,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            speed: AtomicU32::new(1.0f32.to_bits()),
            load: AtomicU32::new(0.4f32.to_bits()),
        }
    }
}
impl Control {
    pub fn set_speed(&self, speed: f32) {
        let speed = if speed.is_finite() {
            speed.clamp(0.5, 5.0)
        } else {
            1.0
        };
        self.speed.store(speed.to_bits(), Ordering::Relaxed);
    }
    pub fn set_load(&self,load:f32){
        self.load.store(if load.is_finite(){load.clamp(0.0,1.0).to_bits()}else{0.4f32.to_bits()},Ordering::Relaxed);
    }
}

pub(super) struct EngineSource {
    control: Arc<Control>,
    synthesis: crate::acoustics::EngineSynthesis,
    speed: f32,
    load:f32,
    right: Option<f32>,
}
impl EngineSource {
    pub fn new(control: Arc<Control>) -> Self {
        Self {
            speed: f32::from_bits(control.speed.load(Ordering::Relaxed)),
            load:f32::from_bits(control.load.load(Ordering::Relaxed)),
            control,
            synthesis: crate::acoustics::EngineSynthesis::new(SEED),
            right: None,
        }
    }
}
impl Iterator for EngineSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let target = f32::from_bits(self.control.speed.load(Ordering::Relaxed));
        // Sample-rate smoothing prevents simulation frame steps in the combustion clock.
        self.speed += (1.0 - (-1.0 / (0.020 * RATE as f32)).exp()) * (target - self.speed);
        let load=f32::from_bits(self.control.load.load(Ordering::Relaxed));
        self.load+=(load-self.load)*(1.0-(-1.0/(0.035*RATE as f32)).exp());
        let [left, right] = self.synthesis.frame_loaded(self.speed,self.load);
        self.right = Some(right);
        Some(left)
    }
}
impl Source for EngineSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

fn render(speed:f32,warmup_frames:usize,frames:usize)->Vec<f32>{render_loaded(speed,0.4,warmup_frames,frames)}
fn render_loaded(speed:f32,load:f32,warmup_frames:usize,frames:usize)->Vec<f32> {
    let mut voice = crate::acoustics::EngineSynthesis::new(SEED);
    for _ in 0..warmup_frames {
        voice.frame_loaded(speed,load);
    }
    (0..frames).flat_map(|_| voice.frame_loaded(speed,load)).collect()
}

/// Measured level of the complete procedural source, including all existing components. No stem
/// is independently normalised or mixed louder. One-second warmup removes filter startup bias.
pub(super) fn measured_laf(speed:f32)->f32 {measured_laf_loaded(speed,0.4)}
pub(super) fn measured_laf_loaded(speed:f32,load:f32)->f32 {
    static TABLE:OnceLock<[[f32;7];3]>=OnceLock::new();
    let table=TABLE.get_or_init(|| LOADS.map(|load|SPEEDS.map(|speed|
        crate::reproduction::laf_max_dbfs(&render_loaded(speed,load,RATE as usize,RATE as usize*4)))));
    let x=speed.clamp(SPEEDS[0],SPEEDS[6]).ln();
    let i=SPEEDS.iter().rposition(|s|s.ln()<=x).unwrap_or(0).min(5);
    let f=((x-SPEEDS[i].ln())/(SPEEDS[i+1].ln()-SPEEDS[i].ln())).clamp(0.0,1.0);
    let load=if load.is_finite(){load.clamp(0.0,1.0)}else{0.4};
    let j=if load<0.4{0}else{1};let g=(load-LOADS[j])/(LOADS[j+1]-LOADS[j]);
    let at=|j:usize|table[j][i]+f*(table[j][i+1]-table[j][i]);at(j)+g*(at(j+1)-at(j))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibrated_group_level_survives_rpm_and_load_changes() {
        for (speed, load, rpm) in [
            (0.0, 0.0, Some(0.0)),
            (0.1, 1.0, None),
            (0.4, 0.4, None),
            (0.9, 0.9, None),
            (1.0, 1.0, Some(1.0)),
        ] {
            let mut telemetry = super::super::EngineTelemetry::automatic(speed, 0.8, load, true);
            if let Some(rpm) = rpm {
                telemetry.rpm_norm = rpm;
            }
            let (ear_level, rate) = super::super::engine_mix_targets(telemetry);
            let gain = crate::reproduction::voice_gain(ear_level, measured_laf(rate));
            let mut samples = render(rate, RATE as usize * 2, RATE as usize * 4);
            for sample in &mut samples {
                *sample *= gain;
            }
            let got = crate::reproduction::laf_max_dbfs(&samples);
            let expected = crate::reproduction::tone_map_dbfs(ear_level);
            eprintln!(
                "engine group rate={rate:.3} load={load:.2} LAF={got:.2} target={expected:.2}"
            );
            assert!(
                (got - expected).abs() < 0.8,
                "{got} vs {expected} at {rate}"
            );
        }
    }
    #[test]
    fn streaming_source_is_deterministic_finite_and_frame_partition_independent() {
        let run = || {
            let control = Arc::new(Control::default());
            let mut source = EngineSource::new(control.clone());
            let mut result = Vec::new();
            for speed in [0.8, 2.4, f32::NAN, f32::INFINITY, 0.6] {
                control.set_speed(speed);
                result.extend(source.by_ref().take(4_410));
            }
            result
        };
        let a = run();
        assert_eq!(a, run());
        assert!(a.iter().all(|v| v.is_finite()));
        let control = Arc::new(Control::default());
        let mut whole = EngineSource::new(control.clone());
        let mut divided = EngineSource::new(control);
        let all: Vec<_> = whole.by_ref().take(8_820).collect();
        let mut parts: Vec<_> = divided.by_ref().take(3_017).collect();
        parts.extend(divided.by_ref().take(5_803));
        assert_eq!(all, parts);
    }
    #[test]
    fn live_engine_shuts_down_to_exact_silence_without_changing_ear_level_law() {
        let audio = super::super::Audio::new_capture();
        // Installed optional recordings retain their old route; default is the procedural source.
        let mut telemetry = super::super::EngineTelemetry::automatic(0.25, 0.7, 0.6, true);
        for _ in 0..120 {
            audio.set_engine_at_ear(telemetry, 100.0, 1.0 / 60.0);
            audio.render_capture_frames(735);
        }
        let heard = audio.render_capture_frames(44_100).unwrap();
        assert!(heard.iter().any(|s| s.abs() > 1e-4));
        assert!(
            (crate::reproduction::laf_max_dbfs(&heard) - crate::reproduction::tone_map_dbfs(100.0))
                .abs()
                < 1.0
        );
        telemetry.running = false;
        for _ in 0..420 {
            audio.set_engine_at_ear(telemetry, 100.0, 1.0 / 60.0);
            audio.render_capture_frames(735);
        }
        assert!(audio
            .render_capture_frames(4_410)
            .unwrap()
            .iter()
            .all(|s| *s == 0.0));
    }
}
