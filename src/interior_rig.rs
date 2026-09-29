//! The Tiger interior RIG — engine-space constants extracted from the authored interior
//! (`assets/tanks/tiger/baked/tiger_interior.json` + the bake's model extraction). Checked in as
//! code (no runtime JSON parse, no serde dep): the extractor is deterministic and these change
//! only when the model is re-baked, so codegen-by-hand keeps the engine dependency-free.
//!
//! Frames: `Frame::Hull` positions are hull-local (ground-contact origin); `Frame::Turret` are
//! turret-local (relative to the turret ring pivot) — place via `hull_model()` / `turret_model()`.
//! All coords are engine space: `engine = (blender_x, blender_z, -blender_y)`, +Z barrel-forward.

use glam::Vec3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    Hull,
    Turret,
}

/// The real interior lamps (the authenticity whitelist): the dome lamp, the TPZ dial illuminator,
/// and the red ignition indicator. `(frame, pos, color, radiant intensity, source radius metres)`.
/// The dome lens is fixed to the underside of the turret-roof extractor fan.
pub const TURRET_FAN_UNDERSIDE_Y: f32 = 2.2301013; // source Turret_0118, turret-local
pub const TURRET_DOME_LAMP_HALF_SIZE: f32 = 0.027;
pub const TURRET_DOME_LAMP_POS: Vec3 = Vec3::new(
    -0.01264894, TURRET_FAN_UNDERSIDE_Y - TURRET_DOME_LAMP_HALF_SIZE, -0.00222471);

/// Art-calibrated fixture output in the renderer's linear scale, not measured historical watts.
/// Daylight apertures must establish the daytime cabin's bright end. Apply the same reduction to
/// the visible dome lens and its transported light; exposure remains the ordinary measured eye.
pub const CABIN_PRACTICAL_POWER_SCALE: f32 = 0.02;
pub const TURRET_DOME_LENS_EMISSION: f32 = 1.15 * CABIN_PRACTICAL_POWER_SCALE;

/// Since the light foundation (2026-09-29) both backends hold the same spectral radiance, lit by
/// the same physical environment and mapped by the same crewman's eye, so the shader backend's
/// former x20 on these fixtures (a calibration for its old fixed exposure) multiplied their real
/// output a second time: the cabin blew out in the dome lamp's amber and the ready lamp's and
/// annunciator's green. A backend may not change an emitter's power (CLAUDE.md, second principle).
pub const SHADER_CABIN_PRACTICAL_GAIN: f32 = 1.0;
pub const fn cabin_practical_gain(shader_backend: bool) -> f32 {
    if shader_backend { SHADER_CABIN_PRACTICAL_GAIN } else { 1.0 }
}

pub const LAMPS: &[(Frame, Vec3, Vec3, f32, f32)] = &[
    // Innenbeleuchtung dome lamp — a dim amber practical beneath the turret roof fan. Its real
    // inverse-square source shapes only the nearby basket instead of flattening the compartment and
    // overpowering the TPZ/ignition lamps. The lens spectrum below matches the visible bulb mesh.
    (
        Frame::Turret,
        TURRET_DOME_LAMP_POS,
        Vec3::new(1.0, 0.41, 0.055),
        0.42 * CABIN_PRACTICAL_POWER_SCALE,
        0.090,
    ),
    // TPZ dial illuminator — small warm-orange pool on the dial face (rides the turret)
    (
        Frame::Turret,
        Vec3::new(0.841, 1.752, 0.598),
        Vec3::new(1.0, 0.50, 0.08),
        0.055 * CABIN_PRACTICAL_POWER_SCALE,
        0.015,
    ),
    // ignition indicator — tiny red point at the driver's panel
    (
        Frame::Hull,
        Vec3::new(1.136, 1.676, 2.112),
        Vec3::new(1.0, 0.10, 0.05),
        0.025 * CABIN_PRACTICAL_POWER_SCALE,
        0.012,
    ),
];

#[cfg(test)]
mod lighting_tests {
    use super::*;

    #[test]
    fn named_cabin_fixtures_emit_the_same_power_on_both_backends() {
        let physical = cabin_practical_gain(false);
        let raster = cabin_practical_gain(true);
        for &(_, _, _, intensity, _) in LAMPS {
            assert_eq!(intensity * physical, intensity);
            assert_eq!(intensity * raster, intensity, "the shader backend adds no gain");
        }
        assert_eq!(TURRET_DOME_LENS_EMISSION * raster, TURRET_DOME_LENS_EMISSION);
        assert_eq!(0.0 * raster, 0.0, "an unlit fixture stays unlit");
    }
}

/// Interior walk bounds (turret-local): the TURRET FLOOR footprint + stand height, and the breech
/// box — the single obstacle. From the extractor (`turret_floor` / `breech_box`).
pub const FLOOR_MIN: Vec3 = Vec3::new(-0.798, 0.556, -0.800); // z min/max swapped to engine order
pub const FLOOR_MAX: Vec3 = Vec3::new(0.797, 0.743, 0.796);
pub const FLOOR_STAND_Y: f32 = 0.743;
pub const BREECH_MIN: Vec3 = Vec3::new(-0.278, 1.746, -0.026);
pub const BREECH_MAX: Vec3 = Vec3::new(0.286, 2.104, 0.667);

/// The 8.8 cm round: chambered seat pose and the ready/grab pose 1.13 m back along the bore
/// (turret-local), from `Round in the chamber`.
pub const ROUND_SEAT: Vec3 = Vec3::new(0.007, 1.959, 0.725);
pub const ROUND_READY: Vec3 = Vec3::new(0.007, 1.959, -0.405);

/// TPZ needle: dial centre (turret-local) — spun about the dial-face axis by turret-vs-hull yaw.
pub const TPZ_NEEDLE_POS: Vec3 = Vec3::new(0.879, 1.752, 0.630);

/// Handwheel hubs (turret-local): Gun wheel1 is the gunner's elevation wheel. `wheel2` is the
/// commander's auxiliary/manual turret-traverse flywheel on top of the traverse gearbox, beside
/// the cupola; it is not reachable from the gunner's post. The source JSON records Blender XYZ
/// dimensions, so wheel2's thin Blender-Y dimension maps to engine -Z: that is its shaft axis.
pub const WHEEL1_POS: Vec3 = Vec3::new(0.255, 1.317, 0.333);
pub const WHEEL2_POS: Vec3 = Vec3::new(0.912, 1.949, -0.386);
pub const COMMANDER_TRAVERSE_WHEEL_AXIS: Vec3 = Vec3::new(0.0, 0.0, -1.0);
/// Captured-Tiger technical data gives about 595 commander-flywheel revolutions for one complete
/// turret revolution. Keeping the real ratio also makes powered traverse read as driven machinery.
pub const COMMANDER_TRAVERSE_TURNS_PER_TURRET_REV: f32 = 595.0;

/// The 65 rack-shell slots (from the `Shells` collection): the engine instances the textured
/// round mesh at each. `(Frame, pos, quat_engine [x,y,z,w])` — quat = blender XYZ-euler swizzled
/// to engine axes ((x,z,-y) component swap). Slots deplete as ammunition is spent.
/// One whole loader-side forward rack is HE. All other racks are AP. Source slot IDs are
/// deliberately enumerated: source export order interleaves the separate trays.
pub const HE_RACK_SLOTS: [usize; 12] = [41, 42, 43, 47, 48, 49, 59, 60, 61, 62, 63, 64];
pub fn shell_slot_is_he(index: usize) -> bool { HE_RACK_SLOTS.contains(&index) }

pub const SHELL_SLOTS: &[(Frame, Vec3, [f32; 4])] = &[
    (
        Frame::Hull,
        Vec3::new(1.561, 1.75, -0.5445),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.493, 0.6157),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.603, -0.5447),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.893, -0.5443),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.459, -0.5449),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.624, 0.6167),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.759, 0.6178),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(1.561, 1.889, 0.6191),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.187, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.436, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.312, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(1.37, 1.509, -0.685),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.889, 0.6191),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.459, -0.5449),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.759, 0.6178),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.75, -0.5445),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.893, -0.5443),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.603, -0.5447),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.493, 0.6157),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Hull,
        Vec3::new(-1.561, 1.624, 0.6167),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.365, 0.5999),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 0.9354, -0.5641),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.235, 0.5986),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.226, -0.5637),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.369, -0.5635),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.079, -0.5639),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 0.9687, 0.5965),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.187, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.312, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
    (
        Frame::Turret,
        Vec3::new(-1.436, 1.1, 0.5975),
        [0.0, 0.0, -0.0, 1.0],
    ),
];

#[cfg(test)]
mod lamp_mount_tests {
    use super::*;
    #[test]
    fn bulb_cube_top_contacts_actual_fan_underside() {
        let path="assets/tanks/tiger/baked/tiger_turret.glb";
        if !crate::resource::exists(path) {return;}
        let mut parts=crate::gltf_load::load_glb_partitioned_roots(path,&["Turret_0118"]).unwrap();
        let fan=parts.pop().unwrap();
        assert!((TURRET_DOME_LAMP_POS.y+TURRET_DOME_LAMP_HALF_SIZE-fan.min.y).abs()<1e-5);
        let centre=(fan.min+fan.max)*0.5;
        assert!((centre.x-TURRET_DOME_LAMP_POS.x).abs()<1e-5);
        assert!((centre.z-TURRET_DOME_LAMP_POS.z).abs()<1e-5);
    }
}
