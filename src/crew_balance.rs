//! Bounded muscular response to the actual chassis acceleration. This is an authored
//! balance controller, not a measured biomechanical model; there is no idle oscillation.
use glam::Vec3;

#[derive(Clone,Copy,Debug,Default, serde::Serialize,serde::Deserialize)]
pub(super) struct Balance {
    pub offset:Vec3,
    velocity:Vec3,
    pub acceleration:Vec3,
}
impl Balance {
    /// A chassis velocity step initially leaves the person's mass behind. Gun impulse is
    /// supplied once per actual discharge, independently of the renderer's sampling rate.
    pub fn impulse(&mut self,chassis_delta_velocity:Vec3) {
        if chassis_delta_velocity.is_finite() {
            self.velocity-=chassis_delta_velocity.clamp_length_max(0.5);
        }
    }
    pub fn step(&mut self,acceleration:Vec3,dt:f32) {
        if !dt.is_finite() || dt<=0.0 || !acceleration.is_finite() {return;}
        self.acceleration=acceleration.clamp_length_max(35.0);
        let dt=dt.min(0.1);let steps=(dt*240.0).ceil().max(1.0)as usize;
        let h=dt/steps as f32;let w=std::f32::consts::TAU*2.4;
        for _ in 0..steps {
            self.velocity+=(-self.acceleration-self.velocity*(2.0*0.8*w)-self.offset*w*w)*h;
            self.offset+=self.velocity*h;
            let limited=self.offset.clamp(Vec3::new(-0.065,-0.075,-0.065),Vec3::new(0.065,0.025,0.065));
            for axis in 0..3 {if limited[axis]!=self.offset[axis] {self.velocity[axis]=0.0;}}
            self.offset=limited;
        }
    }
}

/// Acceleration of a support point on the sprung rigid hull, including angular motion.
pub(super) fn at_point(linear:Vec3,angular_accel:Vec3,angular_velocity:Vec3,point:Vec3)->Vec3 {
    linear+angular_accel.cross(point)+angular_velocity.cross(angular_velocity.cross(point))
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn parked_crew_has_no_invented_bob() {
        let mut b=Balance::default();for _ in 0..600 {b.step(Vec3::ZERO,1.0/60.0);}
        assert_eq!(b.offset,Vec3::ZERO);
    }
    #[test]fn acceleration_and_discharge_produce_a_bounded_decaying_response() {
        let mut b=Balance::default();b.impulse(Vec3::NEG_Z*0.14);
        b.step(Vec3::ZERO,1.0/60.0);assert!(b.offset.z>0.0);
        for _ in 0..120 {b.step(Vec3::ZERO,1.0/60.0);}
        assert!(b.offset.length()<1e-4);
        for _ in 0..120 {b.step(Vec3::new(30.0,30.0,30.0),1.0/60.0);}
        assert!(b.offset.x<0.0&&b.offset.y<0.0&&b.offset.z<0.0);
        assert!(b.offset.length()<0.13);
    }
    #[test]fn response_is_stable_across_frame_intervals() {
        let mut coarse=Balance::default();let mut fine=Balance::default();
        for _ in 0..30 {coarse.step(Vec3::new(2.0,3.0,-1.0),1.0/30.0);}
        for _ in 0..120 {fine.step(Vec3::new(2.0,3.0,-1.0),1.0/120.0);}
        assert!(coarse.offset.distance(fine.offset)<0.0001);
    }
    #[test]fn hull_rotation_accelerates_each_seat_at_its_actual_position() {
        assert_eq!(at_point(Vec3::ZERO,Vec3::X,Vec3::ZERO,Vec3::Z*2.0),Vec3::NEG_Y*2.0);
    }
}

impl Balance {pub(super) fn validate_world_state(&self)->anyhow::Result<()> {
    anyhow::ensure!(self.offset.is_finite()&&self.velocity.is_finite()&&self.acceleration.is_finite(),"invalid crew balance state");Ok(())
}}
