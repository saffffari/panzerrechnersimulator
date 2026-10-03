//! Actual loader friction and single cartridge contacts. Modal frequencies/reference levels
//! are authored foley estimates, not measurements of the supplied museum recording.
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use rodio::Source;
const RATE:u32=crate::acoustics::SAMPLE_RATE;
const DT:f32=1.0/RATE as f32;
pub const LOADER_FRICTION_REFERENCE_DBA:f32=78.0;

#[derive(Clone,Copy,Debug,Default)]
pub struct LoaderMotion { pub extraction_mps:f32, pub ram_mps:f32 }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum CartridgeKind { EmptyCase, LiveRound }
#[derive(Clone,Copy,Debug)]
pub struct CartridgeContact {
    pub kind:CartridgeKind, pub normal_speed_mps:f32, pub tangential_speed_mps:f32,
    pub mass_kg:f32, pub bearing_deg:f32, pub seed:u32,
}
fn finite(v:f32,lo:f32,hi:f32)->f32 {if v.is_finite(){v.clamp(lo,hi)}else{0.0}}
fn random(seed:u32)->f32 {let mut x=seed.wrapping_mul(747796405).wrapping_add(2891336453);
    x=((x>>((x>>28)+4))^x).wrapping_mul(277803737);((x>>22)^x)as f32/u32::MAX as f32}
fn contact_energy(p:&CartridgeContact)->f32 {
    let normal=finite(p.normal_speed_mps,0.0,15.0);
    let sliding=finite(p.tangential_speed_mps,0.0,10.0);
    0.5*finite(p.mass_kg,0.0,50.0)*(normal*normal+0.018*sliding*sliding)
}
/// Relative energy scaling around the prior short handling-contact estimate. This is at-ear
/// foley calibration, not a claim that all collision kinetic energy becomes airborne sound.
pub fn cartridge_contact_level_dba(p:&CartridgeContact)->f32 {
    let energy=contact_energy(p);
    if energy<=0.000_01 {return f32::NEG_INFINITY;}
    82.0+10.0*(energy/3.5).log10()
}
pub(crate) fn synth_contact(p:CartridgeContact)->Vec<f32> {
    if contact_energy(&p)<=0.000_01 {return Vec::new();}
    let empty=p.kind==CartridgeKind::EmptyCase;
    let duration=if empty {0.72}else{0.32};
    let modes=if empty {[(431.0,0.125,0.19),(1097.0,0.155,0.28),(2371.0,0.110,0.32),(4133.0,0.065,0.20)]}
        else {[(119.0,0.033,0.38),(307.0,0.046,0.31),(811.0,0.025,0.18),(1909.0,0.012,0.08)]};
    let detune=0.97+0.06*random(p.seed);
    let normal=finite(p.normal_speed_mps,0.0,15.0);
    let sliding=finite(p.tangential_speed_mps,0.0,10.0);
    let contact_damping=(0.6+normal.min(2.0)*0.2).clamp(0.6,1.0);
    let pan=finite(p.bearing_deg,-36000.0,36000.0).to_radians().sin()*0.55;
    let angle=(pan+1.0)*std::f32::consts::FRAC_PI_4;
    let mut out=Vec::with_capacity((duration*RATE as f32)as usize*2);
    for i in 0..(duration*RATE as f32)as usize {
        let t=i as f32*DT;let attack=1.0-(-t/0.00035).exp();
        let tail=((duration-t)/0.028).clamp(0.0,1.0);
        let ring=modes.iter().enumerate().map(|(j,(hz,decay,weight))|
            weight*(std::f32::consts::TAU*hz*detune*t).sin()*(-t/(decay*contact_damping)).exp()
                *(0.85+0.30*random(p.seed^(j as u32+1)*997))).sum::<f32>();
        // Finite sliding rasp belongs to this contact; no late invented impacts/bounces.
        let rasp=((t*std::f32::consts::TAU*1733.0).sin()+(t*std::f32::consts::TAU*2711.0).sin()*0.35)
            *(-t/0.024).exp()*sliding.min(2.0)*0.045;
        // Floor/body knock belongs to this actual contact, below the case-wall ring.
        let floor_hz=if empty{178.0}else{105.0};
        let floor=(std::f32::consts::TAU*floor_hz*t).sin()*(-t/0.018).exp()
            *if empty{0.27}else{0.35};
        let value=attack*tail*(ring+rasp+floor);
        out.extend([value*angle.cos(),value*angle.sin()]);
    }
    if out.len()>=2 {let n=out.len();out[n-2..].fill(0.0);}
    out
}

#[derive(Default)]
struct Control { speeds:[AtomicU32;2], bearing:AtomicU32 }
impl Control {
    fn set(&self,m:LoaderMotion,bearing:f32) {
        for (at,speed)in self.speeds.iter().zip([m.extraction_mps,m.ram_mps]) {
            at.store(finite(speed,-3.0,3.0).to_bits(),Ordering::Relaxed);
        }
        self.bearing.store(finite(bearing,-36000.0,36000.0).to_bits(),Ordering::Relaxed);
    }
}
#[derive(Default)]
pub(super) struct State { sink:Option<rodio::Sink>,control:Arc<Control>,
    gain_control:Arc<super::continuous::Control>,base_gain:f32 }
impl State {
    pub(super) fn set_master_volume(&self,m:f32){self.gain_control.set_gain(self.base_gain*m);}
    pub(super) fn set_paused(&self,p:bool){if let Some(s)=&self.sink {if p{s.pause()}else{s.play()}}}
}
struct FrictionSource {control:Arc<Control>,speed:[f32;2],phase:[f32;2],pulse:[f32;2],
    body:[f32;2],ring:[f32;2],velocity:[f32;2],counter:[u32;2],edge_phase:[[f32;2];2],right:Option<f32>}
impl FrictionSource {
    fn new(control:Arc<Control>)->Self {Self {control,speed:[0.0;2],phase:[0.0,0.3],pulse:[0.0;2],
        body:[0.0;2],ring:[0.0;2],velocity:[0.0;2],counter:[0;2],edge_phase:[[0.0;2];2],right:None}}
    fn frame(&mut self)->[f32;2] {
        let mut mono=0.0;
        for i in 0..2 {
            let target=f32::from_bits(self.control.speeds[i].load(Ordering::Relaxed));
            self.speed[i]+=(target-self.speed[i])*(1.0-(-DT/0.012).exp());
            let speed=self.speed[i].abs();
            self.phase[i]+=speed*DT/0.0035;
            if self.phase[i]>=1.0 {self.phase[i]-=1.0;self.counter[i]=self.counter[i].wrapping_add(1);
                let contact=(0.06+0.13*random(self.counter[i]^((i as u32+1)*733)))
                    *(speed/0.35).min(1.6).sqrt();
                self.pulse[i]+=contact;self.velocity[i]+=contact*850.0;
            }
            self.pulse[i]*=(-DT/0.0016).exp();
            self.body[i]+=(self.pulse[i]-self.body[i])*(1.0-(-DT/0.008).exp());
            let w=std::f32::consts::TAU*if i==0{1073.0}else{733.0};
            self.velocity[i]+=(-w*w*self.ring[i]-240.0*self.velocity[i])*DT;
            self.ring[i]+=self.velocity[i]*DT;
            // Brass sliding on steel: motion-driven stick/slip excites several close upper bands.
            // Frequency/roughness drift with the actual speed and surface contact; zero target
            // lets the velocity smoothing release it instead of leaving a looping screech.
            let active=(speed/0.35).min(1.7).sqrt();
            let rough=random(self.counter[i]^((i as u32+1)*1877));
            let frequencies=if i==0{[1370.0,2280.0]}else{[970.0,1840.0]};
            let mut scrape_edge=0.0;
            for (j,hz) in frequencies.into_iter().enumerate(){
                self.edge_phase[i][j]=(self.edge_phase[i][j]
                    +std::f32::consts::TAU*hz*(1.0+0.025*rough+0.04*speed)*DT)
                    %std::f32::consts::TAU;
                scrape_edge+=self.edge_phase[i][j].sin()*if j==0{0.045}else{0.030};
            }
            // Ram brass/breech friction is more prominent in the supplied reload target.
            // The separate +6dB authored rendering bias never creates sound at zero speed.
            let contact_mix=if i==1 {2.0}else{1.0};
            mono+=((self.pulse[i]-self.body[i])*0.8+self.ring[i]*0.45
                +scrape_edge*active*(0.55+0.45*rough))*contact_mix;
        }
        let b=f32::from_bits(self.control.bearing.load(Ordering::Relaxed)).to_radians();
        let pan=(1.0+b.sin()*0.45)*std::f32::consts::FRAC_PI_4;
        [mono*pan.cos(),mono*pan.sin()]
    }
}
impl Iterator for FrictionSource {type Item=f32;fn next(&mut self)->Option<f32>{
    if let Some(r)=self.right.take(){return Some(r)}let [l,r]=self.frame();self.right=Some(r);Some(l)}}
impl Source for FrictionSource {fn channels(&self)->u16{2}fn sample_rate(&self)->u32{RATE}
    fn current_frame_len(&self)->Option<usize>{None}fn total_duration(&self)->Option<Duration>{None}}
fn reference_laf()->f32 {
    static LEVEL:OnceLock<f32>=OnceLock::new();*LEVEL.get_or_init(||{
        let c=Arc::new(Control::default());c.set(LoaderMotion{extraction_mps:0.35,ram_mps:0.0},0.0);
        let mut s=FrictionSource::new(c);for _ in 0..RATE {s.next();}
        crate::reproduction::laf_max_dbfs(&s.take(RATE as usize*2).collect::<Vec<_>>())})
}
impl super::Audio {
    pub fn set_loader_motion(&self,motion:LoaderMotion,ear_laf_dba:f32,bearing_deg:f32) {
        let mut state=self.loader_motion.borrow_mut();state.control.set(motion,bearing_deg);
        let moving=[motion.extraction_mps,motion.ram_mps].iter().any(|v|v.is_finite()&&v.abs()>0.002);
        if moving { self.protect_interaction(0.4); }
        if state.sink.is_none()&&!moving {return;}
        if state.sink.is_none()&&moving {if let Ok(sink)=self.create_sink(){
            sink.append(super::continuous::Continuous::new(FrictionSource::new(state.control.clone()),state.gain_control.clone()));
            state.sink=Some(sink);
        }}
        // Reproduction-only close handling lift; physical ear level is unchanged.
        state.base_gain=crate::reproduction::voice_gain(ear_laf_dba,reference_laf())
            *10.0_f32.powf(10.0/20.0);
        state.set_master_volume(self.master_volume);
    }
    /// One real swept MG case/floor contact, distinct from an 88 mm empty case.
    pub fn play_mg_case_contact(&self,normal_mps:f32,slide_mps:f32,bearing_deg:f32,seed:u32) {
        let energy=0.5*0.012*(finite(normal_mps,0.0,15.0).powi(2)
            +0.018*finite(slide_mps,0.0,10.0).powi(2));
        if energy<=0.000_01 {return;}
        let mut pcm=Vec::new();let pan=(finite(bearing_deg,-36000.0,36000.0).to_radians().sin()*0.55+1.0)
            *std::f32::consts::FRAC_PI_4;
        for i in 0..(RATE as f32*0.26) as usize {
            let t=i as f32*DT;let attack=1.0-(-t/0.00025).exp();
            let ring=[(2790.0,0.045,0.30),(4730.0,0.035,0.25),(7310.0,0.018,0.12)]
                .into_iter().enumerate().map(|(j,(hz,decay,gain))|
                    (std::f32::consts::TAU*hz*(0.98+0.04*random(seed+j as u32))*t).sin()
                    *(-t/decay).exp()*gain).sum::<f32>();
            let body=(std::f32::consts::TAU*370.0*t).sin()*(-t/0.007).exp()*0.16;
            let x=(ring+body)*attack*((0.26-t)/0.025).clamp(0.0,1.0);
            pcm.extend([x*pan.cos(),x*pan.sin()]);
        }
        self.protect_interaction(0.35);
        // Close small brass contact at the operator, an explicit authored energy calibration.
        self.play_handling_at_ear(pcm,74.0+10.0*(energy/0.012).log10());
    }

    pub fn play_cartridge_contact(&self,contact:CartridgeContact,ear_laf_dba:f32) {
        self.protect_interaction(0.8);
        self.play_handling_at_ear(synth_contact(contact),ear_laf_dba);
    }
}
#[cfg(test)]mod tests {
    use super::*;
    fn energy(x:&[f32])->f32{x.iter().map(|v|v*v).sum()}
    #[test]fn loader_friction_requires_motion_and_stops_and_reverses(){
        let c=Arc::new(Control::default());let mut s=FrictionSource::new(c.clone());
        assert!(s.by_ref().take(1000).all(|v|v==0.0));
        for speed in [0.3,-0.3] {c.set(LoaderMotion{extraction_mps:speed,ram_mps:0.0},0.0);
            let pcm=s.by_ref().take(RATE as usize).collect::<Vec<_>>();
            assert!(pcm.iter().all(|v|v.is_finite()));assert!(energy(&pcm)>0.1);
        }
        c.set(LoaderMotion::default(),0.0);let pcm=s.take(RATE as usize).collect::<Vec<_>>();
        assert!(energy(&pcm[pcm.len()-1000..])<1e-8);
    }
    #[test]fn case_and_live_round_contacts_are_causal_distinct_and_energy_scaled(){
        let p=CartridgeContact{kind:CartridgeKind::EmptyCase,normal_speed_mps:1.0,tangential_speed_mps:0.0,
            mass_kg:7.0,bearing_deg:65.0,seed:41};
        let a=synth_contact(p);let b=synth_contact(CartridgeContact{kind:CartridgeKind::LiveRound,..p});
        assert_eq!(&a[..2],&[0.0,0.0]);assert_eq!(&a[a.len()-2..],&[0.0,0.0]);
        assert!(a.iter().all(|v|v.is_finite()&&v.abs()<1.0));assert_ne!(a,b);
        assert!((cartridge_contact_level_dba(&CartridgeContact{normal_speed_mps:2.0,..p})
            -cartridge_contact_level_dba(&p)-6.0206).abs()<0.001);
        assert!(synth_contact(CartridgeContact{normal_speed_mps:0.0,..p}).is_empty());
        assert!(synth_contact(CartridgeContact{normal_speed_mps:f32::NAN,..p}).is_empty());
        let (l,r)=crate::acoustics::channel_energy(&a);assert!(r>l);
        assert_ne!(a,synth_contact(CartridgeContact{seed:42,..p}));
    }
}
