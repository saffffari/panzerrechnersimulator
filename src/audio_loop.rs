//! Sample-clock control for continuous machinery. Simulation targets keep their calibrated
//! steady-state gain; interpolation only removes frame-boundary steps. One-shot attacks do not
//! pass through this adapter. This cannot repair an ALSA callback underrun.
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::time::Duration;
use rodio::{Source, source::UniformSourceIterator};

const RATE: u32 = crate::acoustics::SAMPLE_RATE;
const GAIN_FRAMES: u32 = RATE / 200; // 5 ms, reaches exact zero rather than an infinite tail.
const RATE_SMOOTH: f32 = 1.0 / (0.020 * RATE as f32);

pub(super) struct Control {
    gain: AtomicU32,
    rate: AtomicU32,
    rendered_gain: AtomicU32,
}
impl Default for Control {
    fn default() -> Self { Self { gain: AtomicU32::new(0),
        rate: AtomicU32::new(1.0f32.to_bits()), rendered_gain: AtomicU32::new(0) } }
}
impl Control {
    pub(super) fn set_gain(&self, gain: f32) {
        let gain = if gain.is_finite() { gain.max(0.0) } else { 0.0 };
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }
    pub(super) fn set_rate(&self, rate: f32) {
        let rate = if rate.is_finite() { rate.clamp(0.25, 4.0) } else { 1.0 };
        self.rate.store(rate.to_bits(), Ordering::Relaxed);
    }
    pub(super) fn is_silent(&self) -> bool {
        self.gain.load(Ordering::Relaxed) == 0 && self.rendered_gain.load(Ordering::Relaxed) == 0
    }
}

pub(super) struct Continuous<S: Source<Item=f32>> {
    source: UniformSourceIterator<S, f32>,
    control: Arc<Control>,
    previous: [f32;2], next_frame: [f32;2], primed: bool,
    phase: f64, rate: f32,
    gain: f32, target_gain: f32, gain_step: f32, remaining: u32,
    right: Option<f32>,
}
impl<S: Source<Item=f32>> Continuous<S> {
    pub(super) fn new(source: S, control: Arc<Control>) -> Self {
        Self { source: UniformSourceIterator::new(source, 2, RATE), control,
            previous:[0.0;2], next_frame:[0.0;2], primed:false, phase:0.0, rate:1.0,
            gain:0.0, target_gain:0.0, gain_step:0.0, remaining:0, right:None }
    }
    fn read_frame(&mut self) -> [f32;2] {
        [self.source.next().unwrap_or(0.0), self.source.next().unwrap_or(0.0)]
    }
}
impl<S: Source<Item=f32>> Iterator for Continuous<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() { return Some(right); }
        if !self.primed {
            self.previous = self.read_frame(); self.next_frame = self.read_frame();
            self.primed = true;
        }
        let gain = f32::from_bits(self.control.gain.load(Ordering::Relaxed));
        if gain != self.target_gain {
            self.target_gain = gain; self.remaining = GAIN_FRAMES;
            self.gain_step = (gain-self.gain)/GAIN_FRAMES as f32;
        }
        if self.remaining > 0 {
            self.remaining -= 1;
            self.gain = if self.remaining == 0 { self.target_gain } else { self.gain+self.gain_step };
        }
        self.control.rendered_gain.store(self.gain.to_bits(), Ordering::Relaxed);
        let target_rate = f32::from_bits(self.control.rate.load(Ordering::Relaxed));
        self.rate += (target_rate-self.rate)*RATE_SMOOTH;
        let out = std::array::from_fn::<_,2,_>(|i|
            (self.previous[i]+(self.next_frame[i]-self.previous[i])*self.phase as f32)*self.gain);
        self.phase += self.rate as f64;
        while self.phase >= 1.0 {
            self.phase -= 1.0; self.previous = self.next_frame; self.next_frame = self.read_frame();
        }
        self.right = Some(out[1]); Some(out[0])
    }
}
impl<S: Source<Item=f32>> Source for Continuous<S> {
    fn current_frame_len(&self)->Option<usize>{None}
    fn channels(&self)->u16{2}
    fn sample_rate(&self)->u32{RATE}
    fn total_duration(&self)->Option<Duration>{None}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn constant() -> impl Source<Item=f32> {
        rodio::buffer::SamplesBuffer::new(2,RATE,vec![1.0f32;64]).repeat_infinite()
    }
    #[test]
    fn nonzero_phase_gain_changes_ramp_and_retirement_waits_for_exact_silence() {
        let control=Arc::new(Control::default());
        let mut source=Continuous::new(constant(),control.clone());
        control.set_gain(0.8);
        let start:Vec<_>=source.by_ref().take((GAIN_FRAMES*2+20) as usize).collect();
        assert!(start[0] <= 0.8/GAIN_FRAMES as f32+1e-7);
        assert!(start.windows(2).all(|w|(w[1]-w[0]).abs()<0.004));
        assert_eq!(*start.last().unwrap(),0.8);
        control.set_gain(0.0);
        assert!(!control.is_silent(),"do not stop the sink before audio reaches zero");
        let stop:Vec<_>=source.by_ref().take(GAIN_FRAMES as usize*2).collect();
        assert!(stop[0]>0.79);
        assert_eq!(*stop.last().unwrap(),0.0);
        assert!(control.is_silent());
        assert!(source.take(100).all(|s|s==0.0));
    }
    #[test]
    fn playback_changes_keep_phase_continuous_and_do_not_change_settled_gain() {
        let control=Arc::new(Control::default());control.set_gain(0.3);
        let wave:Vec<f32>=(0..4410).flat_map(|i| {
            let v=(std::f32::consts::TAU*200.0*i as f32/RATE as f32).sin();[v,v]
        }).collect();
        let mut source=Continuous::new(rodio::buffer::SamplesBuffer::new(2,RATE,wave).repeat_infinite(),control.clone());
        let _:Vec<_>=source.by_ref().take(2000).collect();
        let before=source.next().unwrap();let _=source.next();
        let phase=source.phase;
        control.set_rate(2.5);
        let after=source.next().unwrap();let _=source.next();
        assert!((after-before).abs()<0.02,"no phase reset on key edge");
        assert!(source.rate>1.0&&source.rate<1.01);
        assert!(source.phase>=phase||phase>0.99);
        let _:Vec<_>=source.by_ref().take(RATE as usize).collect();
        let pcm:Vec<_>=source.take(RATE as usize*2).collect();
        let rms=(pcm.iter().map(|x|x*x).sum::<f32>()/pcm.len() as f32).sqrt();
        assert!((rms-0.3/std::f32::consts::SQRT_2).abs()<0.002);
    }
    #[test]
    fn stereo_targets_apply_once_per_frame_and_partitioning_is_deterministic() {
        let make=|| { let control=Arc::new(Control::default());
            control.set_gain(0.7);(Continuous::new(constant(),control.clone()),control) };
        let(mut a,ca)=make();let(mut b,cb)=make();
        let mut pa:Vec<_>=a.by_ref().take(107).collect();
        let mut pb:Vec<_>=b.by_ref().take(107).collect();
        ca.set_gain(0.1);cb.set_gain(0.1);ca.set_rate(1.7);cb.set_rate(1.7);
        pa.extend(a.take(913));
        for n in [1,12,243,657] {pb.extend(b.by_ref().take(n));}
        assert_eq!(pa,pb);
        assert!(pa.chunks_exact(2).all(|s|s[0]==s[1]));
    }
}
