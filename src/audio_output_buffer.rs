//! Live-only deadline isolation. Expensive mixer/ear/limiter work runs on one producer;
//! the device callback never waits for that work or allocates a sample block.
use std::sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc::{sync_channel, Receiver, TryRecvError}};
use std::time::Duration;
use rodio::Source;
const FRAMES:usize=256;
const SAMPLES:usize=FRAMES*2;
// CPAL ALSA default primes a 100 ms hardware buffer (25 ms steady periods).
// Cover that bounded startup burst plus two producer scheduling blocks.
const DEVICE_BUFFER_MS:usize=100;
const BLOCKS:usize=(crate::acoustics::SAMPLE_RATE as usize * DEVICE_BUFFER_MS).div_ceil(1000 * FRAMES)+2;
#[derive(Default)]
pub(super) struct Telemetry {pub underruns:AtomicU64,pub produced:AtomicU64,pub max_producer_ns:AtomicU64,pub consumed_frames:AtomicU64,pub first_starved_frame_plus_one:AtomicU64,pub first_starved_elapsed_us:AtomicU64}
impl Telemetry {pub fn snapshot(&self)->serde_json::Value {serde_json::json!({"starved_blocks":self.underruns.load(Ordering::Relaxed),"produced_blocks":self.produced.load(Ordering::Relaxed),"max_producer_block_ns":self.max_producer_ns.load(Ordering::Relaxed),"block_frames":FRAMES,"queued_blocks_max":BLOCKS,"buffer_basis_ms":DEVICE_BUFFER_MS,"consumed_source_rate_frames":self.consumed_frames.load(Ordering::Relaxed),"first_starved_source_rate_frame":self.first_starved_frame_plus_one.load(Ordering::Relaxed).checked_sub(1),"first_starved_elapsed_us":if self.first_starved_frame_plus_one.load(Ordering::Relaxed)==0{None}else{Some(self.first_starved_elapsed_us.load(Ordering::Relaxed))}})}}
pub(super) struct BufferedOutput {
    receiver:Receiver<[f32;SAMPLES]>, current:[f32;SAMPLES], cursor:usize,
    pub telemetry:Arc<Telemetry>, started:std::time::Instant, consumed_samples:u64,
}
impl BufferedOutput {
    pub fn new<S:Source<Item=f32>+Send+'static>(mut source:S)->Self {
        let (sender,receiver)=sync_channel(BLOCKS);
        let telemetry=Arc::new(Telemetry::default());let producer=Arc::clone(&telemetry);
        std::thread::Builder::new().name("panzer-audio-mix".into()).spawn(move || {
            loop {
                let started=std::time::Instant::now();
                let mut block=[0.0;SAMPLES];let mut any=false;
                for value in &mut block { if let Some(v)=source.next(){*value=v;any=true;}else{break;} }
                producer.max_producer_ns.fetch_max(started.elapsed().as_nanos().min(u64::MAX as u128) as u64,Ordering::Relaxed);
                if !any || sender.send(block).is_err(){break;}
                producer.produced.fetch_add(1,Ordering::Relaxed);
            }
        }).expect("audio producer thread");
        // Startup prefill is outside the device callback. No source loss if production is slow.
        let current=receiver.recv().unwrap_or([0.0;SAMPLES]);
        let warmup=std::time::Instant::now();
        while telemetry.produced.load(Ordering::Relaxed)<BLOCKS as u64
            && warmup.elapsed()<Duration::from_millis(100) {std::thread::yield_now();}
        Self{receiver,current,cursor:0,telemetry,started:std::time::Instant::now(),consumed_samples:0}
    }
}
impl Iterator for BufferedOutput {
    type Item=f32;
    fn next(&mut self)->Option<f32> {
        if self.cursor==SAMPLES {
            match self.receiver.try_recv() {
                Ok(block)=>{self.current=block;self.cursor=0;},
                Err(TryRecvError::Empty)=>{self.telemetry.underruns.fetch_add(1,Ordering::Relaxed);
                    if self.telemetry.first_starved_frame_plus_one.compare_exchange(0,self.consumed_samples/2+1,Ordering::Relaxed,Ordering::Relaxed).is_ok(){
                        self.telemetry.first_starved_elapsed_us.store(self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,Ordering::Relaxed);
                    }
                    self.current=[0.0;SAMPLES];self.cursor=0;},
                Err(TryRecvError::Disconnected)=>return None,
            }
        }
        let value=self.current[self.cursor];self.cursor+=1;self.consumed_samples+=1;
        if self.cursor==SAMPLES {self.telemetry.consumed_frames.store(self.consumed_samples/2,Ordering::Relaxed);}
        Some(value)
    }
}
impl Source for BufferedOutput {
    fn channels(&self)->u16{2} fn sample_rate(&self)->u32{crate::acoustics::SAMPLE_RATE}
    fn current_frame_len(&self)->Option<usize>{None} fn total_duration(&self)->Option<Duration>{None}
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn starved_callback_returns_whole_silent_stereo_blocks_without_waiting(){
        let (sender,receiver)=sync_channel(BLOCKS);
        let telemetry=Arc::new(Telemetry::default());
        let mut output=BufferedOutput{receiver,current:[0.0;SAMPLES],cursor:SAMPLES,telemetry:Arc::clone(&telemetry),started:std::time::Instant::now(),consumed_samples:0};
        let started=std::time::Instant::now();
        assert!(output.by_ref().take(SAMPLES).all(|v|v==0.0));
        assert!(started.elapsed()<Duration::from_millis(20),"callback waited for unavailable producer");
        assert_eq!(telemetry.underruns.load(Ordering::Relaxed),1);
        assert_eq!(telemetry.snapshot()["first_starved_source_rate_frame"],0);
        sender.send([0.25;SAMPLES]).unwrap();
        assert!(output.by_ref().take(SAMPLES).all(|v|v==0.25));
        drop(sender);assert_eq!(output.next(),None);
    }
    #[test]fn prefilled_queue_covers_hardware_startup_buffer_without_producer(){
        let (sender,receiver)=sync_channel(BLOCKS);
        for _ in 0..BLOCKS {sender.send([0.25;SAMPLES]).unwrap();}
        let telemetry=Arc::new(Telemetry::default());
        let mut output=BufferedOutput{receiver,current:[0.25;SAMPLES],cursor:0,telemetry:Arc::clone(&telemetry),started:std::time::Instant::now(),consumed_samples:0};
        let frames=(crate::acoustics::SAMPLE_RATE as usize*DEVICE_BUFFER_MS).div_ceil(1000);
        assert!(output.by_ref().take(frames*2).all(|v|v==0.25));
        assert_eq!(telemetry.underruns.load(Ordering::Relaxed),0);
        assert!(BLOCKS<=20,"queue latency must remain bounded");
    }
    #[test]fn vent_source_is_finite_has_zero_onset_and_a_finite_tail(){
        let samples=pressure_vent_mono(91);
        assert_eq!(samples[0],0.0);
        assert!(samples.iter().all(|v|v.is_finite()&&v.abs()<1.0));
        assert!(samples.iter().any(|v|v.abs()>0.01));
        assert!(samples.last().unwrap().abs()<0.001);
    }
    #[test]fn bounded_output_preserves_delivered_order_across_producer_pause(){
        let samples:Vec<f32>=(0..SAMPLES*7).map(|i|i as f32+1.0).collect();
        let mut output=BufferedOutput::new(rodio::buffer::SamplesBuffer::new(2,crate::acoustics::SAMPLE_RATE,samples.clone()));
        let mut got=Vec::new();
        while let Some(v)=output.next(){if v!=0.0{got.push(v);}else{std::thread::yield_now();}}
        assert_eq!(got,samples);
    }
}

/// One pressure release at an actual recorded armor breach. Authored timbre, not a gun report.
pub(crate) fn pressure_vent_mono(seed:u32)->Vec<f32> {
    let rate=crate::acoustics::SAMPLE_RATE as f32;
    let mut state=seed.max(1);let mut low=0.0_f32;
    (0..(rate*0.95) as usize).map(|i| {
        let t=i as f32/rate;
        state^=state<<13;state^=state>>17;state^=state<<5;
        let noise=(state as f32/u32::MAX as f32)*2.0-1.0;
        low+=(noise-low)*0.06;
        let onset=1.0-(-t/0.002).exp();
        let release=(-t/0.22).exp()*((0.95-t)/0.08).clamp(0.0,1.0);
        let hiss=(noise-low)*0.24;
        let roar=low*0.65;
        let ring=(std::f32::consts::TAU*1270.0*t).sin()*(-t/0.065).exp()*0.18;
        (roar+hiss+ring)*release*onset
    }).collect()
}
