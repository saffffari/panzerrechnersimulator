//! Affine support refinement for a prescribed smoke carrier, not a fluid solve.
//! Stretching each source sphere into an ellipsoid along its own source path has an exact
//! Jacobian. Divide density by that Jacobian to preserve its integrated optical mass,
//! including the original radial envelope/noise expressed in normalized source coordinates.
use glam::Vec3;
#[derive(Clone,Copy,Debug)]
pub(super) struct Sweep {pub center:Vec3,pub half_axis:Vec3,pub radius:f32,pub density:f32}
pub(super) fn from_sphere(start:Vec3,end:Vec3,radius:f32,density:f32)->Sweep {
    let radius=radius.max(0.02);let half_axis=(end-start)*0.5;
    let stretch=1.0+half_axis.length()/radius;
    Sweep {center:(start+end)*0.5,half_axis,radius,density:density.max(0.0)/stretch}
}
#[cfg(test)]mod tests {use super::*;
    #[test]fn swept_smoke_keeps_source_connected_and_optical_mass_under_wind(){
        let root=Vec3::new(30.0,2.0,-41.0);
        for wind in [Vec3::ZERO,Vec3::new(18.0,0.0,-7.0),Vec3::new(-35.0,0.0,1.0)] {
            let end=root+wind*0.28*3.15+Vec3::Y*3.15*1.2;
            let s=from_sphere(root,end,2.792,0.188);
            let stretch=1.0+s.half_axis.length()/s.radius;
            let dir=s.half_axis.normalize_or_zero();
            let to_source=|p:Vec3|{let d=p-s.center;(d-dir*d.dot(dir)*(1.0-1.0/stretch))/s.radius};
            assert!(to_source(root).length()<1.0);assert!(to_source(end).length()<1.0);
            let original_mass=0.188*s.radius.powi(3);
            assert!((s.density*s.radius.powi(3)*stretch-original_mass).abs()<1e-5);
            for i in 0..101 {assert!(to_source(root.lerp(end,i as f32/100.0)).length()<1.0);}
            // Any normalized noise coordinate maps back unchanged: normalization preserves
            // the integral of the entire textured density, not only a uniform approximation.
            for q in [Vec3::new(0.2,0.3,0.4),Vec3::new(-0.3,0.1,-0.6)] {
                let world=s.center+(q+dir*q.dot(dir)*(stretch-1.0))*s.radius;
                assert!(to_source(world).distance(q)<1e-5);
            }
        }
    }
    #[test]fn zero_length_sweep_is_the_existing_sphere(){
        let p=Vec3::new(1.0,2.0,3.0);let s=from_sphere(p,p,0.8,0.2);
        assert_eq!(s.center,p);assert_eq!(s.half_axis,Vec3::ZERO);assert_eq!(s.density,0.2);
    }
}
