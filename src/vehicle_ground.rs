//! Bounded rigid terrain pose for AI vehicles and initial spawns.
//! Six samples span two track contact lines; this is not a suspension simulation. The player
//! subsequently uses solid_city's fuller source-profile solve, including floors and rubble.
use glam::{Mat3, Vec2, Vec3};

/// The winter snowpack under a loaded track. Settled steppe snow of 0.40 m (declared for the
/// Act-I cauldron in December 1942, not a measured station value) compacts to a quarter of its
/// depth under any tracked vehicle of the weights fielded here (a Tiger loads its tracks at
/// about 100 kPa, a T-34 at 65; both press settled snow to the firm layer), so the tracks rest
/// 0.30 m below the drawn snow surface. One value for the player's support profile
/// (`solid_city::tank_support_at`) and the AI vehicles' heightfield rest
/// (`terrain::Heightfield::vehicle_height_at`). Named in docs/PHYSICAL_TRUTH_AUDIT.md.
pub const WINTER_SNOW_DEPTH_M: f32 = 0.40;
pub const TRACK_COMPACTION_SHARE: f32 = 0.75;
pub const WINTER_TRACK_SINKAGE_M: f32 = WINTER_SNOW_DEPTH_M * TRACK_COMPACTION_SHARE;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pose { pub height: f32, pub pitch: f32, pub roll: f32 }

fn contacts(half_extents: Vec2) -> [Vec3; 6] {
    // Contact runs are inside the extremal idler/sprocket and fender bounds. These footprint
    // fractions are a declared AI support approximation, not measured track geometry.
    let x = half_extents.x * 0.75;
    let z = half_extents.y * 0.65;
    [Vec3::new(-x,0.0,-z),Vec3::new(-x,0.0,0.0),Vec3::new(-x,0.0,z),
     Vec3::new(x,0.0,-z),Vec3::new(x,0.0,0.0),Vec3::new(x,0.0,z)]
}

pub(crate) fn solve(center: Vec2, heading: f32, half_extents: Vec2,
    height_at: impl Fn(f32,f32)->f32) -> Option<Pose> {
    if !center.is_finite() || !heading.is_finite() || !half_extents.is_finite()
        || half_extents.min_element() <= 0.0 { return None; }
    let yaw = Mat3::from_rotation_y(heading);
    let points = contacts(half_extents);
    let mut gradient = Vec2::ZERO;
    let mut moments = Vec2::ZERO;
    // Subtract the center height before fitting to retain precision on large-height maps.
    let reference = height_at(center.x, center.y);
    if !reference.is_finite() { return None; }
    for point in points {
        let horizontal = yaw * point;
        let h = height_at(center.x+horizontal.x,center.y+horizontal.z)-reference;
        if !h.is_finite() { return None; }
        gradient += Vec2::new(point.x,point.z)*h;
        moments += Vec2::new(point.x*point.x,point.z*point.z);
    }
    gradient /= moments;
    let pitch = gradient.y.atan();
    let roll = (gradient.x / (1.0+gradient.y*gradient.y).sqrt()).atan();
    let rotation = yaw * Mat3::from_rotation_x(-pitch) * Mat3::from_rotation_z(roll);
    let mut height = f32::NEG_INFINITY;
    for point in points {
        let rotated = rotation * point;
        let support = height_at(center.x+rotated.x,center.y+rotated.z)-rotated.y;
        if !support.is_finite() { return None; }
        // The rigid body must clear every sampled contact; averaging height would bury a track
        // at convex/uneven ground. Bridging between contacts remains an explicit simplification.
        height = height.max(support);
    }
    Some(Pose {height,pitch,roll})
}

/// The rigid fit, taken as the pose: spawns, placements, and the player's pass before its own
/// support solve.
pub(crate) fn settle_rigid(tank: &mut crate::tank::Tank, height_at: impl Fn(f32,f32)->f32) {
    if let Some(pose) = solve(Vec2::new(tank.pos.x,tank.pos.z),tank.heading,
        tank.parts.hull_half_extents,height_at) {
        tank.pos.y=pose.height;tank.pitch=pose.pitch;tank.roll=pose.roll;
        tank.grade_pitch=pose.pitch;
    }
}

/// Re-settle after a motion constraint or a contact push moved the hull within the frame. A hull
/// whose suspension is running keeps its integrated pose (the next frame's step meets the new
/// supports); it is re-seated on the rigid rest only where that stands more than its whole
/// jounce travel and 0.1 m above the sprung hull: a support the ride's own samples did not see
/// (the Act-II bridge deck this call is given and the drive's heightfield is not). Otherwise, the
/// rigid fit as before.
pub(crate) fn settle(tank: &mut crate::tank::Tank, height_at: impl Fn(f32,f32)->f32) {
    let xz=Vec2::new(tank.pos.x,tank.pos.z);
    if tank.ride.continues(xz) {
        if let Some(pose)=solve(xz,tank.heading,tank.parts.hull_half_extents,&height_at) {
            tank.grade_pitch=pose.pitch;
            let jounce=tank.ride_layout.as_ref().map_or(0.15,|l|l.spec.jounce_m);
            if pose.height>tank.ride.y+jounce+0.1 {
                tank.ride.reset(pose.height,pose.pitch,pose.roll,xz,tank.speed,tank.heading);
            }
        }
        tank.pos.y=tank.ride.y;tank.pitch=tank.ride.pitch;tank.roll=tank.ride.roll;
        return;
    }
    settle_rigid(tank,height_at);
}

/// One frame of an AI vehicle's sprung hull (`suspension`) on the heightfield: the rigid fit gives
/// the grade (and the start pose after a placement); each road-wheel station's support is the
/// wheel envelope over samples 0.1 m apart within its wheel radius on its track line, the end
/// stations also over the track run-up to the hull's ends (0.2 m apart, `AI_APPROACH_TAN`).
pub(crate) fn settle_ride(tank: &mut crate::tank::Tank, dt: f32, height_at: impl Fn(f32,f32)->f32) {
    use crate::suspension::{station_support, End, Ground, Layout, AI_APPROACH_TAN};
    let xz=Vec2::new(tank.pos.x,tank.pos.z);
    let Some(rigid)=solve(xz,tank.heading,tank.parts.hull_half_extents,&height_at) else { return };
    tank.grade_pitch=rigid.pitch;
    let spec=crate::vehicle_spec::for_tank(tank.kind,&tank.parts);
    if tank.ride_layout.as_ref().is_none_or(|l|!std::ptr::eq(l.spec,spec)) {
        tank.ride_layout=Some(Box::new(Layout::for_footprint(spec,tank.parts.hull_half_extents)));
        tank.ride.active=false;
    }
    if !tank.ride.continues(xz) {
        tank.ride.reset(rigid.height,rigid.pitch,rigid.roll,xz,tank.speed,tank.heading);
        tank.pos.y=rigid.height;tank.pitch=rigid.pitch;tank.roll=rigid.roll;
        return;
    }
    let layout=tank.ride_layout.as_deref().expect("layout");
    let yaw=Mat3::from_rotation_y(tank.heading);
    let cos_p=rigid.pitch.cos();
    let at=|x:f32,s:f32| { let w=yaw*Vec3::new(x,0.0,s*cos_p); height_at(xz.x+w.x,xz.y+w.z) };
    let r=spec.wheel_radius();
    let (rear,front)=layout.run;
    let rise=|s:f32| -> Option<f32> {
        if s>front && s<=layout.ends.1+1e-4 { Some((s-front)*AI_APPROACH_TAN) }
        else if s<rear && s>=layout.ends.0-1e-4 { Some((rear-s)*AI_APPROACH_TAN) }
        else { None }
    };
    let mut ground=Ground { wheel:Vec::with_capacity(layout.stations.len()), belly:Vec::with_capacity(layout.belly.len()) };
    let mut points:Vec<(f32,f32)>=Vec::with_capacity(32);
    for (i,st) in layout.stations.iter().enumerate() {
        points.clear();
        let n=(r/0.1).ceil() as i32;
        for k in -n..=n {
            let s=st.s+(k as f32*0.1).clamp(-r,r);
            points.push((s,at(st.x,s)));
        }
        let end=layout.end_of(i);
        let reach=match end { End::Front=>layout.ends.1-st.s, End::Rear=>st.s-layout.ends.0, End::Middle=>0.0 };
        let dir=if end==End::Front {1.0} else {-1.0};
        let mut d=0.2;
        while d<=reach+1e-4 {
            let s=st.s+dir*d;
            points.push((s,at(st.x,s)));
            d+=0.2;
        }
        ground.wheel.push(station_support(&points,st.s,r,end,&rise));
    }
    for &(s,clearance) in &layout.belly {
        ground.belly.push((s,clearance,at(0.0,s)));
    }
    let max_rise=(tank.speed.abs()*2.0+0.1)*dt+0.002;
    tank.ride.rise_limit(&mut ground,max_rise);
    tank.ride.step(layout,&ground,tank.speed,tank.heading,dt);
    tank.ride.xz=xz;
    if tank.ride.active {
        tank.pos.y=tank.ride.y;tank.pitch=tank.ride.pitch;tank.roll=tank.ride.roll;
    } else {
        settle_rigid(tank,height_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclined_planes_align_both_tracks_at_any_heading() {
        for gradient in [Vec2::ZERO,Vec2::new(0.25,0.0),Vec2::new(0.0,-0.35),Vec2::new(0.3,-0.2)] {
            for heading in [0.0,0.7,1.8,3.5] {
                let center=Vec2::new(12.0,-21.0);let size=Vec2::new(1.785375,3.5310215);
                let ground=|x:f32,z:f32|3.0+x*gradient.x+z*gradient.y;
                let pose=solve(center,heading,size,ground).unwrap();
                let rotation=Mat3::from_rotation_y(heading)*Mat3::from_rotation_x(-pose.pitch)*Mat3::from_rotation_z(pose.roll);
                let normal=Vec3::new(-gradient.x,1.0,-gradient.y).normalize();
                assert!((rotation*Vec3::Y-normal).length()<1e-5);
                for point in contacts(size) {
                    let p=rotation*point+Vec3::new(center.x,pose.height,center.y);
                    assert!((p.y-ground(p.x,p.z)).abs()<1e-5);
                }
            }
        }
    }

    #[test]
    fn uneven_supports_do_not_bury_sampled_track_contacts() {
        let size=Vec2::new(1.9,3.5);let ground=|x:f32,z:f32|0.18*(x*0.8).sin()+0.12*(z*1.4).cos();
        let pose=solve(Vec2::ZERO,0.4,size,ground).unwrap();
        let rotation=Mat3::from_rotation_y(0.4)*Mat3::from_rotation_x(-pose.pitch)*Mat3::from_rotation_z(pose.roll);
        for point in contacts(size) {
            let p=rotation*point+Vec3::Y*pose.height;
            assert!(p.y>=ground(p.x,p.z)-1e-6);
        }
        assert!(solve(Vec2::ZERO,0.0,size,|_,_|f32::NAN).is_none());
    }
}
