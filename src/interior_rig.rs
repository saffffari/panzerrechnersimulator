//! The Tiger interior RIG — engine-space constants extracted from the authored interior
//! (`assets/tanks/tiger/baked/tiger_interior.json` + the bake's aperture recon). Checked in as
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

/// A cut-open vision aperture: a REAL hole in the armor (bake-time boolean). Daylight pours in
/// along `normal` (outward); the lighting pass turns each into a sun-driven interior point light.
pub struct Aperture {
    pub frame: Frame,
    pub pos: Vec3,    // at the opening, local to its frame
    pub normal: Vec3, // OUTWARD through the hole, local to its frame
}

/// The 12 direct-view holes (driver visor, hull ports, turret side slits, the 7-slit cupola ring)
/// + the 2 roof periscope shafts (gunner + loader — vertical shafts, sky feed, sun-dot n/a).
pub const APERTURES: &[Aperture] = &[
    // driver's visor — the wide front slit
    Aperture {
        frame: Frame::Hull,
        pos: Vec3::new(-0.448, 1.749, 2.119),
        normal: Vec3::new(0.0, -0.016, 1.0),
    },
    // hull front-right port (radio-op side)
    Aperture {
        frame: Frame::Hull,
        pos: Vec3::new(0.546, 1.737, 2.386),
        normal: Vec3::new(0.0, 0.158, 0.987),
    },
    // hull right side port
    Aperture {
        frame: Frame::Hull,
        pos: Vec3::new(0.913, 1.944, 1.994),
        normal: Vec3::new(0.866, 0.0, 0.5),
    },
    // turret side slits (right / left)
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(1.015, 2.022, 0.563),
        normal: Vec3::new(0.975, 0.0, 0.221),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(-1.031, 2.037, 0.432),
        normal: Vec3::new(-0.947, 0.0, 0.322),
    },
    // the cupola vision-slit ring (7 blocks at slit height, slight down-tilt outward)
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.564, 2.457, -0.083),
        normal: Vec3::new(0.0, -0.172, 0.985),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.298, 2.457, -0.211),
        normal: Vec3::new(-0.770, -0.172, 0.614),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.830, 2.457, -0.211),
        normal: Vec3::new(0.770, -0.172, 0.614),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.232, 2.457, -0.499),
        normal: Vec3::new(-0.960, -0.172, -0.219),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.896, 2.457, -0.499),
        normal: Vec3::new(0.960, -0.172, -0.219),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.416, 2.457, -0.730),
        normal: Vec3::new(-0.427, -0.172, -0.888),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.712, 2.457, -0.730),
        normal: Vec3::new(0.427, -0.172, -0.888),
    },
    // roof periscope shafts (gunner / loader): vertical light wells — treated as up-facing
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(0.496, 1.994, 0.542),
        normal: Vec3::new(0.0, 1.0, 0.0),
    },
    Aperture {
        frame: Frame::Turret,
        pos: Vec3::new(-0.705, 2.251, 0.403),
        normal: Vec3::new(0.0, 1.0, 0.0),
    },
];

/// The real interior lamps (the authenticity whitelist): the dome lamp, the TPZ dial illuminator,
/// and the red ignition indicator. `(frame, pos, color, base intensity, radius)`.
pub const LAMPS: &[(Frame, Vec3, Vec3, f32, f32)] = &[
    // Innenbeleuchtung dome lamp — dim warm, mostly a night source
    // Boosted 0.45→1.5 + warmer + wider (playtest: "can't see anything") — still the one
    // real dome lamp doing the work, just an honest wattage now.
    (
        Frame::Hull,
        Vec3::new(0.0, 2.730, 0.016),
        Vec3::new(1.0, 0.72, 0.42),
        1.5,
        3.4,
    ),
    // TPZ dial illuminator — small warm-orange pool on the dial face (rides the turret)
    (
        Frame::Turret,
        Vec3::new(0.841, 1.752, 0.598),
        Vec3::new(1.0, 0.50, 0.08),
        0.30,
        0.8,
    ),
    // ignition indicator — tiny red point at the driver's panel
    (
        Frame::Hull,
        Vec3::new(1.136, 1.676, 2.112),
        Vec3::new(1.0, 0.10, 0.05),
        0.25,
        0.6,
    ),
];

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

/// Handwheel hubs (turret-local): Gun wheel1 (elevation, spins with gun pitch, axis ≈ X) and
/// wheel2 (traverse assist, spins with turret yaw).
pub const WHEEL1_POS: Vec3 = Vec3::new(0.255, 1.317, 0.333);
pub const WHEEL2_POS: Vec3 = Vec3::new(0.912, 1.949, -0.386);

/// The 65 rack-shell slots (from the `Shells` collection): the engine instances the textured
/// round mesh at each. `(Frame, pos, quat_engine [x,y,z,w])` — quat = blender XYZ-euler swizzled
/// to engine axes ((x,z,-y) component swap). Slots deplete as ammunition is spent.
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
