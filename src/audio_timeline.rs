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
    /// Start frame of the latest event already handed to the mixer.
    pending_until_frame:u64,
}
/// Lead beyond one batch that is only ever left behind by an earlier late batch.
const EXCESS_LEAD_S:f64=0.030;
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
        let (anchor_s,anchor_frame)=self.anchor.unwrap_or((batch.start_s,now_frame));
        let offset=|at:f64|((at-anchor_s).max(0.0)*sr).round() as u64;
        let first_frame=anchor_frame.saturating_add(offset(first.at_s));
        // A late batch leaves every later event scheduled that far ahead. Keep that lead while
        // earlier events are still waiting, so a continuous burst retains its spacing; at the
        // first gap, return to the ordinary one-batch lead instead of staying late for good.
        let max_lead=((batch.end_s-batch.start_s).max(0.0)+EXCESS_LEAD_S)*sr;
        let (anchor_s,anchor_frame)=if first_frame.saturating_sub(now_frame) as f64>max_lead
            && self.pending_until_frame<=now_frame {
            (batch.start_s,now_frame)
        } else {
            // A missed device deadline moves every event by the same amount. No events are thrown
            // away, time-compressed into a single hit, or sped up to recover render performance.
            (anchor_s,anchor_frame.saturating_add(now_frame.saturating_sub(first_frame)))
        };
        let offset=|at:f64|((at-anchor_s).max(0.0)*sr).round() as u64;
        self.anchor=Some((anchor_s,anchor_frame));
        let events:Vec<_>=batch.events.into_iter().map(|event|(anchor_frame.saturating_add(offset(event.at_s)),event)).collect();
        if let Some(last)=events.last() {self.pending_until_frame=self.pending_until_frame.max(last.0);}
        events
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
    #[test] fn lead_left_by_one_long_batch_survives_a_burst_and_ends_at_the_first_gap() {
        // Wall clock and simulation advance together; each batch is finished just after the
        // wall interval it simulates. `now` is the master frame at that moment.
        let now=|end_s:f64|(end_s*44100.0).round() as u64+1000;
        let tick=1.0/60.0;
        let mut clock=Timeline::default();
        let mut run=|start:f64,dt:f64,at:&[f64]|->Vec<(u64,u64)> {
            clock.begin(start,dt);
            clock.batch.as_mut().unwrap().events=at.iter().map(|at|event(*at)).collect();
            let finished=now(start+dt);
            clock.finish(finished).into_iter().map(|v|(v.0,v.0-finished)).collect()
        };
        assert_eq!(run(0.0,tick,&[0.0])[0].1,0,"ordinary lead before the hitch");
        // One half-second frame containing seven 800 rpm rounds.
        let hitch_start=tick;
        let rounds:Vec<f64>=(0..7).map(|i|hitch_start+i as f64*0.075).collect();
        let mut frames:Vec<u64>=run(hitch_start,0.5,&rounds).into_iter().map(|v|v.0).collect();
        // The burst continues over ordinary 60 Hz frames: one round every 75 ms.
        let mut start=hitch_start+0.5;
        let mut next=hitch_start+7.0*0.075;
        let mut lead=0;
        for _ in 0..40 {
            let inside:Vec<f64>=if next<start+tick {let at=next;next+=0.075;vec![at]} else {Vec::new()};
            for (frame,ahead) in run(start,tick,&inside) {frames.push(frame);lead=ahead;}
            start+=tick;
        }
        assert!(frames.len()>=14);
        for pair in frames.windows(2) {
            assert!((3307..=3308).contains(&(pair[1]-pair[0])),"burst spacing changed: {frames:?}");
        }
        assert!(lead>44100/3,"the burst keeps the late batch's lead while rounds are pending: {lead}");
        // Trigger released for one second, then a single new event.
        for _ in 0..60 {assert!(run(start,tick,&[]).is_empty());start+=tick;}
        let after=run(start,tick,&[start+0.004]);
        assert!(after[0].1<=(0.004*44100.0_f64).round() as u64+1,
            "lead must return to its position inside the batch after a gap: {after:?}");
        // Later ordinary batches stay there.
        start+=tick;
        let later=run(start,tick,&[start+0.010]);
        assert!(later[0].1<44100/20,"latency ratcheted again: {later:?}");
    }
}
