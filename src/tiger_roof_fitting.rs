//! Bounded authoring correction for the source roof fitting ahead of the cupola.
//!
//! Its cap originally reaches the objective height and occupies the usable horizon of one
//! healthy port. Retain the complete cap and its original joint to the pedestal: shorten only
//! the pedestal above its fixed foot, then translate the cap by the same 55 mm. This is a game
//! asset fit correction, not a historically measured reconstruction or an occlusion exemption.
use glam::{Mat4,Vec3};

pub const PEDESTAL: &str = "Turret_0115";
pub const CAP: &str = "Turret_0116";
pub const LOWERING_M: f32 = 0.055;
const FOOT_Y: f32 = 2.4110783;
const JOINT_Y: f32 = 2.5122697;

/// World-space source-root correction. All other source nodes are untouched.
pub fn source_transform(name:&str)->Mat4 {
    match name {
        PEDESTAL => Mat4::from_translation(Vec3::Y*FOOT_Y)
            *Mat4::from_scale(Vec3::new(1.0,1.0-LOWERING_M/(JOINT_Y-FOOT_Y),1.0))
            *Mat4::from_translation(-Vec3::Y*FOOT_Y),
        CAP => Mat4::from_translation(-Vec3::Y*LOWERING_M),
        _ => Mat4::IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowered_cap_keeps_its_shape_joint_and_fixed_pedestal_foot() {
        let raw=crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_turret.glb",&[PEDESTAL,CAP]).unwrap();
        let edited=crate::gltf_load::load_glb_partitioned_roots_with_source_transforms(
            "assets/tanks/tiger/baked/tiger_turret.glb",&[PEDESTAL,CAP],None,None,
            Some(source_transform)).unwrap();
        let (pedestal,cap)=(&raw[1],&raw[2]);
        let (shortened,lowered)=(&edited[1],&edited[2]);
        assert!((pedestal.min.y-FOOT_Y).abs()<0.00001);
        assert!((pedestal.max.y-JOINT_Y).abs()<0.00001);
        assert!((shortened.min.y-pedestal.min.y).abs()<0.00001);
        assert!((shortened.max.y-(pedestal.max.y-LOWERING_M)).abs()<0.00001);
        assert!((lowered.min.y-(cap.min.y-LOWERING_M)).abs()<0.00001);
        assert!((lowered.max-lowered.min).abs_diff_eq(cap.max-cap.min,0.00001));
        assert!(lowered.max.y<2.481 && lowered.min.y>2.449,
            "cap remains above the actual 2.407-2.427m roof mounting surface");
        for (original,corrected) in raw.iter().zip(&edited) {
            assert_eq!(original.indices,corrected.indices);
            assert_eq!(original.vertices.len(),corrected.vertices.len());
            assert_eq!(original.texture.as_ref().unwrap().rgba,
                corrected.texture.as_ref().unwrap().rgba);
        }
        // No other source geometry changes, including the roof opening and armor.
        for (a,b) in raw[0].vertices.iter().zip(&edited[0].vertices) {
            assert_eq!(a.pos,b.pos);assert_eq!(a.normal,b.normal);
        }
    }
}
