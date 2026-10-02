//! Motion-driven running-gear sound. Ground samples and track travel come from the live vehicle;
//! the audible contact kernels are authored modal approximations, not a solved Tiger suspension.
//! A stationary vehicle produces no track excitation. Engine-off coasting still turns the tracks.
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use glam::Vec3;
use rodio::Source;

const RATE:u32=crate::acoustics::SAMPLE_RATE;
const LINK_PITCH_M:f32=0.13; // authored cadence estimate, not recovered from the rendered mesh
#[derive(Clone, Copy, Debug, Default)]
pub struct Motion { pub speed:[f32;2], pub terrain_velocity:[f32;2] }

#[derive(Default)]
pub struct MotionSampler {
    previous:Option<([Vec3;14],[f32;14])>,
    pub distance_m:f32,
    pub ground_excitation_m:f32,
}
impl MotionSampler {
    pub fn sample(&mut self, hull:glam::Mat4, dt:f32, height:impl Fn(f32,f32)->f32)->Motion {
        let points=std::array::from_fn(|i| hull.transform_point3(Vec3::new(
            if i<7 {-1.46} else {1.46},0.0,(i%7) as f32*0.64-1.92)));
        let ground=points.map(|p|height(p.x,p.z)-p.y);
        if !dt.is_finite() || !(0.001..=0.25).contains(&dt)
            || points.iter().any(|p|!p.is_finite())
            || ground.iter().any(|v|!v.is_finite()) {
            // A missing/invalid sample breaks the measured travel interval. Do not retain NaNs
            // or bridge the gap on recovery: the next valid pose establishes a fresh baseline.
            self.previous=None;
            return Motion::default();
        }
        let Some((previous,heights))=self.previous.replace((points,ground)) else{return Motion::default()};
        if points.iter().zip(previous).any(|(a,b)|a.distance(b)>3.0) {return Motion::default();}
        let mut output=Motion::default();
        for i in 0..14 {
            let side=i/7;
            let delta=points[i]-previous[i];
            let travel=delta.x.hypot(delta.z);
            output.speed[side]+=travel/(dt*7.0);
            // Remove rigid hull translation using each sample's local ground-to-hull separation.
            // This is measured terrain excitation, not a claim about wheel compression forces.
            output.terrain_velocity[side]+=(ground[i]-heights[i]).abs()/(dt*7.0);
        }
        for s in 0..2 {
            output.speed[s]=output.speed[s].clamp(0.0,15.0);
            output.terrain_velocity[s]=output.terrain_velocity[s].clamp(0.0,3.0);
        }
        self.distance_m+=(output.speed[0]+output.speed[1])*0.5*dt;
        self.ground_excitation_m+=(output.terrain_velocity[0]+output.terrain_velocity[1])*0.5*dt;
        output
    }
}

pub(super) struct Control { speed:[AtomicU32;2], rough:[AtomicU32;2] }
impl Default for Control { fn default()->Self {Self {speed:std::array::from_fn(|_|AtomicU32::new(0)),rough:std::array::from_fn(|_|AtomicU32::new(0))}} }
impl Control {
    fn set(&self,m:Motion){for i in 0..2 {
        let finite=|value:f32,max:f32| if value.is_finite(){value.clamp(0.0,max)}else{0.0};
        self.speed[i].store(finite(m.speed[i],15.0).to_bits(),Ordering::Relaxed);
        self.rough[i].store(finite(m.terrain_velocity[i],3.0).to_bits(),Ordering::Relaxed);
    }}
}
#[derive(Default)]
pub(super) struct State { sink:Option<rodio::Sink>,control:Arc<Control>,loop_control:Arc<super::continuous::Control>,base_gain:f32 }
impl State {
    pub(super) fn set_master_volume(&self,master:f32){
        self.loop_control.set_gain(self.base_gain*master);
    }
    pub(super) fn set_paused(&self,paused:bool){
        if let Some(sink)=&self.sink {if paused {sink.pause();} else {sink.play();}}
    }
}

struct GearSource {
    control:Arc<Control>,phase:[f32;2],speed:[f32;2],rough:[f32;2],
    fast:[f32;2],slow:[f32;2],ring:[f32;2],ring_velocity:[f32;2],
    count:[u32;2],right:Option<f32>,
}
impl GearSource {
    fn new(control:Arc<Control>)->Self {Self {control,phase:[0.0,0.37],speed:[0.0;2],rough:[0.0;2],
        fast:[0.0;2],slow:[0.0;2],ring:[0.0;2],ring_velocity:[0.0;2],count:[0;2],right:None}}
    fn frame(&mut self)->[f32;2] {
        let mut out=[0.0;2];let dt=1.0/RATE as f32;
        for i in 0..2 {
            let target=f32::from_bits(self.control.speed[i].load(Ordering::Relaxed));
            let rough=f32::from_bits(self.control.rough[i].load(Ordering::Relaxed));
            self.speed[i]+=(target-self.speed[i])*(1.0-(-dt/0.02).exp());
            self.rough[i]+=(rough-self.rough[i])*(1.0-(-dt/0.035).exp());
            self.phase[i]+=self.speed[i]*dt/LINK_PITCH_M;
            if self.phase[i]>=1.0 {
                self.phase[i]-=1.0;self.count[i]=self.count[i].wrapping_add(1);
                let hash=self.count[i].wrapping_mul(1664525).wrapping_add(1013904223+i as u32*7919);
                let variation=0.70+(hash>>16) as f32/65535.0*0.60;
                let force=variation*(0.12+self.rough[i].min(0.55)*0.7);
                self.fast[i]+=force;self.slow[i]+=force;
                self.ring_velocity[i]+=force*180.0;
            }
            self.fast[i]*=(-dt/0.003).exp(); self.slow[i]*=(-dt/0.026).exp();
            // Damped local contact mode plus a broad compression/rebound. No free-running hiss.
            let omega=std::f32::consts::TAU*(if i==0 {287.0} else {313.0});
            self.ring_velocity[i]+=(-omega*omega*self.ring[i]-105.0*self.ring_velocity[i])*dt;
            self.ring[i]+=self.ring_velocity[i]*dt;
            out[i]=0.58*(self.fast[i]-self.slow[i])+0.23*self.ring[i];
        }
        // Opposite track reaches both ears through the hull; retain modest lateral differences.
        [out[0]*0.8+out[1]*0.2,out[1]*0.8+out[0]*0.2]
    }
}
impl Iterator for GearSource {type Item=f32;fn next(&mut self)->Option<f32>{
    if let Some(v)=self.right.take(){return Some(v)} let [l,r]=self.frame();self.right=Some(r);Some(l)
}}
impl Source for GearSource {
    fn channels(&self)->u16{2} fn sample_rate(&self)->u32{RATE}
    fn current_frame_len(&self)->Option<usize>{None} fn total_duration(&self)->Option<Duration>{None}
}
fn reference_laf()->f32 {
    static LEVEL:OnceLock<f32>=OnceLock::new();
    *LEVEL.get_or_init(|| {let control=Arc::new(Control::default());control.set(Motion {speed:[3.0;2],terrain_velocity:[0.10;2]});
        let mut source=GearSource::new(control);let _:Vec<_>=source.by_ref().take(RATE as usize*2).collect();
        crate::reproduction::laf_max_dbfs(&source.take(RATE as usize*6).collect::<Vec<_>>())})
}
impl super::Audio {
    pub fn set_running_gear(&self,motion:Motion,engine_ear_dba:f32,alive:bool) {
        let mut state=self.running_gear_state.borrow_mut();
        state.control.set(if alive {motion} else {Motion::default()});
        if state.sink.is_none() && alive && motion.speed.iter().any(|s|*s>0.01) {
            if let Ok(sink)=self.create_sink(){
                // Gain starts at zero and reaches the calibrated target on the sample clock.
                sink.append(super::continuous::Continuous::new(GearSource::new(state.control.clone()),
                    state.loop_control.clone()));state.sink=Some(sink);
            }
        }
        if state.sink.is_some() {
            // The existing measured interior/exterior drivetrain law already includes tracks.
            // Place the nominal contact component12dB below it, using the same listener path estimate.
            // Terrain excitation changes that component; this is not a calibrated energy split.
            // Relative -12dB contact budget and waveform are authored, not a measured Tiger split.
            state.base_gain=if alive {crate::reproduction::voice_gain(engine_ear_dba-12.0,reference_laf())}else{0.0};
            state.set_master_volume(self.master_volume);
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test]fn actual_motion_and_terrain_are_required_for_excitation(){
        let mut s=MotionSampler::default();let flat=|_:f32,_:f32|0.0;
        s.sample(glam::Mat4::IDENTITY,1.0/60.0,flat);
        let still=s.sample(glam::Mat4::IDENTITY,1.0/60.0,flat);
        assert_eq!(still.speed,[0.0;2]);assert_eq!(still.terrain_velocity,[0.0;2]);
        let road=s.sample(glam::Mat4::from_translation(Vec3::Z*0.05),1.0/60.0,flat);
        assert!((road.speed[0]-3.0).abs()<0.0001);assert_eq!(road.terrain_velocity,[0.0;2]);
        let mut r=MotionSampler::default();let terrain=|_:f32,z:f32|(z*3.0).sin()*0.10;
        r.sample(glam::Mat4::IDENTITY,1.0/60.0,terrain);
        let rough=r.sample(glam::Mat4::from_translation(Vec3::Z*0.05),1.0/60.0,terrain);
        assert!(rough.terrain_velocity[0]>0.1);
        let teleported=r.sample(glam::Mat4::from_translation(Vec3::Z*100.0),1.0/60.0,terrain);
        assert_eq!(teleported.speed,[0.0;2]);
    }
    #[test]fn streaming_contacts_are_finite_silent_at_rest_and_decay_after_stopping(){
        let c=Arc::new(Control::default());let mut s=GearSource::new(c.clone());
        assert!(s.by_ref().take(1000).all(|v|v==0.0));
        c.set(Motion{speed:[3.0,2.0],terrain_velocity:[0.2,0.05]});
        let moving:Vec<_>=s.by_ref().take(RATE as usize*2).collect();
        assert!(moving.iter().all(|v|v.is_finite()));assert!(moving.iter().any(|v|v.abs()>0.01));
        c.set(Motion::default());let stopped:Vec<_>=s.take(RATE as usize*4).collect();
        assert!(stopped[stopped.len()-1000..].iter().all(|v|v.abs()<1e-10));
    }
    #[test]fn invalid_motion_breaks_the_interval_and_recovers_without_poisoning_travel(){
        let dt=1.0/60.0;
        for invalid in 0..3 {
            let mut sampler=MotionSampler::default();
            sampler.sample(glam::Mat4::IDENTITY,dt,|_,_|0.0);
            let bad=match invalid {
                0=>sampler.sample(glam::Mat4::IDENTITY,dt,|_,_|f32::NAN),
                1=>sampler.sample(glam::Mat4::from_translation(Vec3::splat(f32::NAN)),dt,|_,_|0.0),
                _=>sampler.sample(glam::Mat4::IDENTITY,f32::NAN,|_,_|0.0),
            };
            assert_eq!(bad.speed,[0.0;2]);
            let recovered=sampler.sample(glam::Mat4::from_translation(Vec3::Z),dt,|_,_|0.0);
            assert_eq!(recovered.speed,[0.0;2],"do not bridge an invalid interval");
            let moving=sampler.sample(glam::Mat4::from_translation(Vec3::Z*1.05),dt,|_,_|0.0);
            assert!((moving.speed[0]-3.0).abs()<0.0001);
            assert!((sampler.distance_m-0.05).abs()<0.0001);
            assert_eq!(sampler.ground_excitation_m,0.0);
        }
        let control=Arc::new(Control::default());
        control.set(Motion{speed:[f32::NAN,f32::INFINITY],terrain_velocity:[f32::NAN,-1.0]});
        assert!(GearSource::new(control).take(1000).all(|v|v==0.0));
    }
    #[test]fn live_track_voice_obeys_pause_volume_and_death_without_recreation(){
        let mut audio=super::super::Audio::new_capture();
        let motion=Motion{speed:[3.0,2.0],terrain_velocity:[0.1,0.05]};
        audio.set_running_gear(motion,110.0,true);
        let moving=audio.render_capture_frames(RATE as usize).unwrap();
        assert!(moving.iter().any(|v|v.abs()>1e-5));
        audio.set_paused(true);
        assert!(audio.running_gear_state.borrow().sink.as_ref().unwrap().is_paused());
        let paused=audio.render_capture_frames(RATE as usize/10).unwrap();
        // Rodio polls controls every 5 ms; allow the ordinary master lookahead to drain too.
        assert!(paused[1000..].iter().all(|v|v.abs()<1e-7));
        audio.set_master_volume(0.0);
        audio.set_paused(false);
        let muted=audio.render_capture_frames(RATE as usize/10).unwrap();
        assert!(muted[1000..].iter().all(|v|v.abs()<1e-7));
        audio.set_master_volume(1.0);
        let resumed=audio.render_capture_frames(RATE as usize/10).unwrap();
        assert!(resumed[1000..].iter().any(|v|v.abs()>1e-5));
        audio.set_running_gear(motion,110.0,false);
        audio.set_master_volume(0.5);
        let dead=audio.render_capture_frames(RATE as usize/10).unwrap();
        assert!(dead[1000..].iter().all(|v|v.abs()<1e-7));
        audio.set_running_gear(motion,110.0,true);
        audio.set_silenced(true);
        assert_eq!(audio.render_capture_frames(1).unwrap(),vec![0.0,0.0]);
        assert_eq!(audio.capture_metadata().unwrap()["sinks_created"],1);
    }
}
