//! Sample-clock delivery for transients generated during a live simulation catch-up batch.
//! The simulation cannot play sound into the past. If a frame arrives late, move the entire
//! batch together to the first available sample, preserving the spacing between real events.
use std::sync::Arc;
use std::time::Duration;
use rodio::Source;
use super::{EarBus,master::MasterTelemetry};

pub(super) struct Event {
    pub at_s:f64,
    pub samples:Vec<f32>,
    pub volume:f32,
    pub bus:EarBus,
}
pub(super) struct Batch {pub start_s:f64,pub end_s:f64,pub now_s:f64,pub events:Vec<Event>}
#[derive(Default)]
pub(super) struct Timeline {
    pub batch:Option<Batch>,
    anchor:Option<(f64,u64)>,
    last_end_s:Option<f64>,
}
impl Timeline {
    pub fn begin(&mut self,start_s:f64,dt:f64) {
        self.batch=Some(Batch{start_s,end_s:start_s+dt,now_s:start_s,events:Vec::new()});
    }
    pub fn set_time(&mut self,time_s:f64) {
        if let Some(batch)=self.batch.as_mut() {batch.now_s=time_s.clamp(batch.start_s,batch.end_s);}
    }
    pub fn finish(&mut self,now_frame:u64)->Vec<(u64,Event)> {
        let Some(mut batch)=self.batch.take() else {return Vec::new();};
        let sr=crate::acoustics::SAMPLE_RATE as f64;
        if self.last_end_s.is_none_or(|last|(last-batch.start_s).abs()>0.002) {
            self.anchor=Some((batch.start_s,now_frame));
        }
        self.last_end_s=Some(batch.end_s);
        batch.events.sort_by(|a,b|a.at_s.total_cmp(&b.at_s));
        let Some(first)=batch.events.first() else {return Vec::new();};
        let (anchor_s,mut anchor_frame)=self.anchor.unwrap_or((batch.start_s,now_frame));
        let offset=|at:f64|((at-anchor_s).max(0.0)*sr).round() as u64;
        let first_frame=anchor_frame.saturating_add(offset(first.at_s));
        // A missed device deadline moves every event by the same amount. No events are thrown
        // away, time-compressed into a single hit, or sped up to recover render performance.
        anchor_frame=anchor_frame.saturating_add(now_frame.saturating_sub(first_frame));
        self.anchor=Some((anchor_s,anchor_frame));
        batch.events.into_iter().map(|event|(anchor_frame.saturating_add(offset(event.at_s)),event)).collect()
    }
}

/// Align the first pull to the common master frame. Afterwards waiting is measured in consumed
/// samples, so pausing a Sink freezes its pending event just like an already audible tail.
pub(super) struct AtFrame<S> {
    source:S,clock:Arc<MasterTelemetry>,target:u64,remaining:Option<u64>,
}
impl<S> AtFrame<S> {
    pub fn new(source:S,clock:Arc<MasterTelemetry>,target:u64)->Self {
        Self{source,clock,target,remaining:None}
    }
}
impl<S:Iterator<Item=f32>> Iterator for AtFrame<S> {
    type Item=f32;
    fn next(&mut self)->Option<f32> {
        let remaining=self.remaining.get_or_insert_with(||self.target.saturating_sub(self.clock.frames())*2);
        if *remaining>0 {*remaining-=1;Some(0.0)} else {self.source.next()}
    }
}
impl<S:Source<Item=f32>> Source for AtFrame<S> {
    fn current_frame_len(&self)->Option<usize>{None}
    fn channels(&self)->u16{2}
    fn sample_rate(&self)->u32{crate::acoustics::SAMPLE_RATE}
    fn total_duration(&self)->Option<Duration>{None}
}

#[cfg(test)] mod tests {
    use super::*;
    fn event(at_s:f64)->Event {Event{at_s,samples:vec![0.1,0.1],volume:1.0,bus:EarBus::World}}
    #[test] fn stalled_batch_retains_exact_mg_spacing_on_the_master_sample_clock() {
        let mut clock=Timeline::default();clock.begin(0.0,0.25);
        clock.batch.as_mut().unwrap().events=vec![event(0.0),event(0.075),event(0.15),event(0.225)];
        let at:Vec<_>=clock.finish(10000).into_iter().map(|v|v.0).collect();
        assert_eq!(at,vec![10000,13308,16615,19923]);
        clock.begin(0.25,0.25);
        clock.batch.as_mut().unwrap().events=vec![event(0.3),event(0.375),event(0.45)];
        let next:Vec<_>=clock.finish(21025).into_iter().map(|v|v.0).collect();
        assert_eq!(next,vec![23230,26538,29845]);
        clock.begin(0.5,0.25);
        clock.batch.as_mut().unwrap().events=vec![event(0.525),event(0.6),event(0.675)];
        let late:Vec<_>=clock.finish(50000).into_iter().map(|v|v.0).collect();
        assert_eq!(late,vec![50000,53307,56615]);
    }
}
