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

/// The dome lens is fixed to the underside of the turret-roof extractor fan.
pub const TURRET_FAN_UNDERSIDE_Y: f32 = 2.2301013; // source Turret_0118, turret-local
pub const TURRET_DOME_LAMP_HALF_SIZE: f32 = 0.027;
pub const TURRET_DOME_LAMP_POS: Vec3 = Vec3::new(
    -0.01264894, TURRET_FAN_UNDERSIDE_Y - TURRET_DOME_LAMP_HALF_SIZE, -0.00222471);

/// Art-calibrated fixture output in the renderer's linear scale, not measured historical watts.
/// Daylight apertures must establish the daytime cabin's bright end. Apply the same reduction to
/// the visible dome lens and its transported light; exposure remains the ordinary measured eye.
pub const CABIN_PRACTICAL_POWER_SCALE: f32 = 0.02;

/// The visible finite cube is the source of the lamp's spectrum and total emitted power.
/// Dimensions, colours and radiances remain authored, not measured historical fixtures.
#[derive(Clone, Copy, Debug)]
pub struct FiniteCubeLamp {
    pub half_size_m: f32,
    pub srgb: [u8; 3],
    pub emission: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LampEmission {
    /// The own-colour mesh shader receives this exact value.
    pub visible_emissive: f32,
    pub linear_color: Vec3,
    pub radiant_intensity: f32,
    /// Circumscribed radius of the actual cube; not an illumination cutoff.
    pub source_radius_m: f32,
}

impl FiniteCubeLamp {
    pub fn sample(self, state: f32) -> LampEmission {
        let state = if state.is_finite() { state.max(0.0) } else { 0.0 };
        let visible_emissive = self.emission * CABIN_PRACTICAL_POWER_SCALE * state;
        // Match the sRGB texture decode used by both mesh shaders, including its linear toe.
        let linear_color = Vec3::from_array(self.srgb.map(|byte| {
            let c = f32::from(byte) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }));
        // Own-colour emission in gbuf/stylized_mesh. Current fixture states stay below the knee.
        let radiance = visible_emissive + (visible_emissive - 3.0).max(0.0) * 0.15;
        let area = 24.0 * self.half_size_m * self.half_size_m;
        // A diffuse surface emits pi*L*A. The existing isotropic point-source approximation
        // emits 4*pi*I, so I=L*A/4 conserves total power. It does not reproduce the cube's
        // direction-dependent projected area or exact near field; scene geometry owns shadows.
        LampEmission {
            visible_emissive,
            linear_color,
            radiant_intensity: radiance * area * 0.25,
            source_radius_m: self.half_size_m * 3.0_f32.sqrt(),
        }
    }
}

pub const DOME_FIXTURE: FiniteCubeLamp = FiniteCubeLamp {
    half_size_m: TURRET_DOME_LAMP_HALF_SIZE, srgb: [255, 171, 66], emission: 1.15,
};
pub const ANNUNCIATOR_FIXTURES: [FiniteCubeLamp; 4] = [
    FiniteCubeLamp { half_size_m: 0.028, srgb: [255, 46, 30], emission: 1.0 },
    FiniteCubeLamp { half_size_m: 0.028, srgb: [255, 172, 44], emission: 1.0 },
    FiniteCubeLamp { half_size_m: 0.028, srgb: [70, 128, 255], emission: 1.0 },
    FiniteCubeLamp { half_size_m: 0.028, srgb: [70, 255, 96], emission: 1.0 },
];
pub const BREECH_FIXTURES: [FiniteCubeLamp; 3] = [
    FiniteCubeLamp { half_size_m: 0.022, srgb: [60, 235, 90], emission: 2.4 },
    FiniteCubeLamp { half_size_m: 0.022, srgb: [255, 245, 20], emission: 2.4 },
    FiniteCubeLamp { half_size_m: 0.022, srgb: [235, 45, 30], emission: 2.4 },
];

pub fn dome_lamp_state(time_s: f32) -> f32 { 0.96 + 0.04 * (time_s * 5.0).sin() }

/// Since the light foundation (2026-09-29) both backends hold the same spectral radiance, lit by
/// the same physical environment and mapped by the same crewman's eye, so the shader backend's
/// former x20 on these fixtures (a calibration for its old fixed exposure) multiplied their real
/// output a second time: the cabin blew out in the dome lamp's amber and the ready lamp's and
/// annunciator's green. A backend may not change an emitter's power (CLAUDE.md, second principle).
pub const SHADER_CABIN_PRACTICAL_GAIN: f32 = 1.0;
pub const fn cabin_practical_gain(shader_backend: bool) -> f32 {
    if shader_backend { SHADER_CABIN_PRACTICAL_GAIN } else { 1.0 }
}

/// Existing source registrations whose emitting lens area/radiance has not yet been extracted.
/// Keep their authored output explicit; never invent a finite mesh to claim source agreement.
/// `(frame, pos, linear color, radiant intensity, source radius metres)`.
pub const LAMPS: &[(Frame, Vec3, Vec3, f32, f32)] = &[
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
        for fixture in std::iter::once(DOME_FIXTURE)
            .chain(ANNUNCIATOR_FIXTURES).chain(BREECH_FIXTURES) {
            for state in [0.0, 0.4, 0.5, 0.6, 1.0, 2.4, 3.5, 4.0, 6.0] {
                let a = fixture.sample(state * physical);
                let b = fixture.sample(state * raster);
                assert_eq!(a.visible_emissive, b.visible_emissive);
                assert_eq!(a.radiant_intensity, b.radiant_intensity);
                assert_eq!(a.linear_color, b.linear_color);
            }
        }
        assert_eq!(0.0 * raster, 0.0, "an unlit fixture stays unlit");
    }

    #[test]
    fn finite_cabin_fixtures_conserve_the_visible_mesh_emission() {
        for fixture in std::iter::once(DOME_FIXTURE)
            .chain(ANNUNCIATOR_FIXTURES).chain(BREECH_FIXTURES) {
            let mesh = crate::prims::box_mesh(Vec3::splat(-fixture.half_size_m),
                Vec3::splat(fixture.half_size_m), fixture.srgb);
            let area: f32 = mesh.indices.chunks_exact(3).map(|tri| {
                let [a,b,c] = [0,1,2].map(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
                (b-a).cross(c-a).length()*0.5
            }).sum();
            for state in [0.0, 0.4, 1.0, 2.4, 3.5, 4.0, 6.0] {
                let source = fixture.sample(state);
                let visible_radiance = source.linear_color * source.visible_emissive;
                let mesh_flux = visible_radiance * (std::f32::consts::PI * area);
                let point_flux = source.linear_color * source.radiant_intensity
                    * (4.0 * std::f32::consts::PI);
                assert!(mesh_flux.abs_diff_eq(point_flux, 1e-8), "{fixture:?}, state {state}");
                for vertex in &mesh.vertices {
                    assert!(Vec3::from(vertex.pos).length() <= source.source_radius_m + 1e-7);
                }
                if state == 0.0 {
                    assert_eq!(source.visible_emissive, 0.0);
                    assert_eq!(source.radiant_intensity, 0.0);
                }
            }
        }
    }

    #[test]
    fn finite_cabin_fixture_spectra_match_srgb_and_flicker_changes_both_outputs() {
        let ready = BREECH_FIXTURES[0].sample(1.0);
        assert!(ready.linear_color.abs_diff_eq(Vec3::new(0.0451862,0.8307699,0.1022417),1e-6));
        let station = ANNUNCIATOR_FIXTURES[3].sample(2.4);
        assert!(station.linear_color.abs_diff_eq(Vec3::new(0.0612461,1.0,0.1169707),1e-6));
        let dark = FiniteCubeLamp {srgb:[0,10,255], ..DOME_FIXTURE}.sample(1.0);
        assert_eq!(dark.linear_color.x,0.0);
        assert!((dark.linear_color.y-10.0/255.0/12.92).abs()<1e-8);
        assert_eq!(dark.linear_color.z,1.0);
        let high = DOME_FIXTURE.sample(dome_lamp_state(std::f32::consts::FRAC_PI_2/5.0));
        let low = DOME_FIXTURE.sample(dome_lamp_state(3.0*std::f32::consts::FRAC_PI_2/5.0));
        assert!((low.visible_emissive/high.visible_emissive-0.92).abs()<1e-6);
        assert!((low.radiant_intensity/high.radiant_intensity-0.92).abs()<1e-6);
        assert!((high.radiant_intensity-0.000100602).abs()<1e-9);
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
