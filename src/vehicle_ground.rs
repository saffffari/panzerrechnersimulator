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

pub(crate) fn settle(tank: &mut crate::tank::Tank, height_at: impl Fn(f32,f32)->f32) {
    if let Some(pose) = solve(Vec2::new(tank.pos.x,tank.pos.z),tank.heading,
        tank.parts.hull_half_extents,height_at) {
        tank.pos.y=pose.height;tank.pitch=pose.pitch;tank.roll=pose.roll;
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
