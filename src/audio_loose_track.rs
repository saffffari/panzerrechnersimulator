//! Loose link/sprocket contact texture. The powertrain, not the broken flag alone, starts it.
//! Modal timbre and103dBA at1m/reference effort are authored estimates, not a Tiger recording.
use std::sync::Arc;
const RATE:u32=crate::acoustics::SAMPLE_RATE;
pub const REFERENCE_BELT_MPS:f32=7.0*0.145;

pub fn source_level(belt_speed_mps:f32,load:f32)->Option<f32> {
    if !belt_speed_mps.is_finite() || !load.is_finite() || belt_speed_mps<=0.005 || load<=0.001 {return None;}
    Some(103.0+10.0*(belt_speed_mps/REFERENCE_BELT_MPS).log10()+10.0*load.clamp(0.001,1.0).log10())
}

pub(super) fn samples()->Arc<[f32]> {
    let frames=RATE as usize*4;
    let mut out=Vec::with_capacity(frames*2);
    for i in 0..frames {
        let t=i as f32/RATE as f32;
        let hit=(t*7.0).floor() as u32;
        let age=(t*7.0).fract()/7.0;
        let hash=hit.wrapping_mul(1664525).wrapping_add(1013904223);
        let force=0.8+(hash>>16) as f32/65535.0*0.4;
        let attack=1.0-(-age/0.0009).exp();
        let wave=[(183.0,0.038,0.38),(421.0,0.026,0.28),(997.0,0.017,0.22),(2311.0,0.008,0.12)]
            .into_iter().map(|(frequency,decay,weight)|weight*(-age/decay).exp()
                *(std::f32::consts::TAU*frequency*age).sin()).sum::<f32>();
        let value=wave*attack*force;
        out.extend([value,value]);
    }
    // The contact bed is periodic in cadence but its last ring still crosses the loop seam.
    // Fold that seam with the same existing loop preparation used by other machinery.
    let mut pcm:Vec<i16>=out.iter().map(|v|(v.clamp(-1.0,1.0)*32767.0)as i16).collect();
    super::seamless_loop_blend(&mut pcm,2,RATE,0.04);
    pcm.into_iter().map(|v|v as f32/32767.0).collect::<Vec<_>>().into()
}

#[cfg(test)] mod tests {
    use super::*;
    #[test]fn source_requires_actual_motion_and_effort_and_scales_contact_power(){
        for (speed,load) in [(0.0,1.0),(1.0,0.0),(f32::NAN,1.0),(1.0,f32::NAN)] {
            assert_eq!(source_level(speed,load),None);
        }
        let base=source_level(REFERENCE_BELT_MPS,1.0).unwrap();
        assert!((base-103.0).abs()<1e-5);
        assert!((source_level(REFERENCE_BELT_MPS*2.0,1.0).unwrap()-base-3.0103).abs()<0.001);
        assert!((source_level(REFERENCE_BELT_MPS,0.25).unwrap()-base+6.0206).abs()<0.001);
    }
    #[test]fn contact_bed_is_finite_impulsive_and_loop_boundary_is_bounded(){
        let pcm=samples();assert!(pcm.iter().all(|v|v.is_finite()&&v.abs()<=1.0));
        let peak=pcm.iter().map(|v|v.abs()).fold(0.0,f32::max);
        let rms=(pcm.iter().map(|v|v*v).sum::<f32>()/pcm.len()as f32).sqrt();
        assert!(peak>0.2&&peak/rms>3.0,"contacts must remain impulses, not a noise bed");
        assert!((pcm[0]-pcm[pcm.len()-2]).abs()<0.025);
    }
}
