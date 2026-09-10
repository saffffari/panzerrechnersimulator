//! Player-only partition of the Tiger's authored roof lids. The original GLBs stay unchanged:
//! closed geometry is conserved, and opening only moves complete source assemblies.

use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};

pub const DRIVER_HINGE: Vec3 = Vec3::new(1.382032, 1.9149045, 1.8893634);
// The source `gunner_hatch` object is centered on the loader's roof lid. Its hinge, not that
// object's center, comes from tiger_interior.json's authored `hatches.gunner.pos` in turret frame.
// The radio operator hinge survives verbatim as `MG Operater` in the source GLB.
pub const RADIO_HINGE: Vec3 = Vec3::new(-1.382032, 1.9149045, 1.8886715);
// Author's `turret escape hatch pivot`: Blender local X, opened downward/outward.
pub const ESCAPE_HINGE: Vec3 = Vec3::new(-0.933214, 1.669645, -0.937066);
pub const ESCAPE_AXIS: Vec3 = Vec3::new(-0.6800127, 0.0, 0.7332004);
pub const APERTURE_CENTERS: [Vec3; 4] = [
    Vec3::new(0.9768873, 2.001, 1.8890174),
    Vec3::new(-0.7044487, 2.418445, -0.1133228),
    Vec3::new(-0.9768873, 2.001, 1.8890172),
    Vec3::new(-0.935, 1.9785175, -0.937),
];
pub const APERTURE_NORMALS: [Vec3; 4] = [Vec3::Y, Vec3::Y, Vec3::Y,
    Vec3::new(-0.7332004, 0.0, -0.6800127)];
pub const APERTURE_RADII: [f32; 4] = [0.265, 0.235, 0.265, 0.265];
pub const LOADER_HINGE: Vec3 = Vec3::new(-0.7036, 2.4564, 0.1235);

pub fn load_player_tiger() -> anyhow::Result<([CpuMesh; 3], [CpuMesh; 4], CpuMesh, Vec<super::tiger_periscopes::OpticalPart>, super::tiger_gunner_sight::Source, super::tiger_driver_shutters::Source)> {
    let mut hull = crate::gltf_load::load_glb_partitioned_roots_with_material_override(
        "assets/tanks/tiger/baked/tiger_hull.glb",
        &["Drivers Hatch Pivot", "MG Operater", "Hull_0189_housing", super::tiger_driver_shutters::SHIELD_ROOT, super::tiger_driver_shutters::WHEEL_ROOT],
        Some(crate::tank::tiger_paint_finish),
    )?;
    let (turret, loader, periscopes, sight_tube, escape) = super::tiger_periscopes::load_turret()?;
    let sight=super::tiger_gunner_sight::load(sight_tube)?;
    let wheel=hull.pop().expect("Driver visor control wheel");
    let shield=hull.pop().expect("Driver outer shields");
    let shutters=super::tiger_driver_shutters::reconstruct(shield,wheel)?;
    let visor_holder=super::tiger_driver_visor::repair_holder(hull.pop().expect("driver visor holder"))?;
    let radio = hull.pop().expect("requested radio operator assembly");
    let driver = hull.pop().expect("requested driver assembly");
    let gun = crate::tank::load_painted_tiger("assets/tanks/tiger/baked/tiger_gun.glb")?;
    Ok((
        [
            super::tiger_front_optics::repair_mg_bore(hull.pop().expect("hull remainder"))?,
            turret,
            gun,
        ]
        .map(crate::tank::repaint_german_grey),
        [driver, loader, radio, escape].map(crate::tank::repaint_german_grey),
        crate::tank::repaint_german_grey(visor_holder),
        periscopes,
        sight,
        shutters,
    ))
}

#[derive(Clone, Copy, Default)]
pub struct RoofHatches {
    pub open: [bool; 4],
    pub phase: [f32; 4],
}

impl RoofHatches {
    pub fn set(&mut self, driver: bool, loader: bool, instant: bool) {
        self.open[0] = driver;
        self.open[1] = loader;
        if instant {
            self.phase = self.open.map(|open| if open { 1.0 } else { 0.0 });
        }
    }

    pub fn advance(&mut self, dt: f32) {
        for (phase, open) in self.phase.iter_mut().zip(self.open) {
            let target = if open { 1.0 } else { 0.0 };
            *phase += (target - *phase).clamp(-1.5 * dt.max(0.0), 1.5 * dt.max(0.0));
        }
    }

    pub fn models(&self, hull: Mat4, turret: Mat4) -> [Mat4; 4] {
        [
            hatch_model(hull, DRIVER_HINGE, -Vec3::Z, self.phase[0]),
            hatch_model(turret, LOADER_HINGE, Vec3::X, self.phase[1]),
            hatch_model(hull, RADIO_HINGE, Vec3::Z, self.phase[2]),
            hatch_model(turret, ESCAPE_HINGE, ESCAPE_AXIS, self.phase[3]),
        ]
    }
}

fn hatch_model(frame: Mat4, hinge: Vec3, axis: Vec3, phase: f32) -> Mat4 {
    let t = phase.clamp(0.0, 1.0);
    let ease = t * t * (3.0 - 2.0 * t);
    frame
        * Mat4::from_translation(hinge)
        * Mat4::from_axis_angle(axis, 95_f32.to_radians() * ease)
        * Mat4::from_translation(-hinge)
}

impl super::Game {
    /// Headless review control for the actual authored driver/loader lids. Opening a lid does not
    /// move the driver's eyes outside or invent an exposed crew pose. `instant` sets the current
    /// animation phase too; otherwise the ordinary update loop eases to the requested state.
    pub fn debug_set_roof_hatches(&mut self, driver: bool, loader: bool, instant: bool) {
        self.roof_hatches.set(driver, loader, instant);
    }

    pub(super) fn drive_roof_hatches(&self, renderer: &mut crate::renderer::Renderer) {
        renderer.set_model(self.slots.driver_visor_holder, self.player.hull_model());
        renderer.set_visible(self.slots.driver_visor_holder, true);
        renderer.set_model(self.slots.driver_visor_guides,self.player.hull_model());
        renderer.set_visible(self.slots.driver_visor_guides,true);
        for (slot,model) in self.slots.driver_visor_moving.into_iter().zip(self.driver_shutters.models(self.player.hull_model())) {
            renderer.set_model(slot,model);renderer.set_visible(slot,true);
        }
        let models = self.roof_hatches.models(self.player.hull_model(), self.player.turret_model());
        // The source eyepiece lived in the separate RTT export; leaving it in the static hull
        // suspended a pane below the roof after its housing had already opened.
        renderer.set_model(self.slots.driver_roof_eyepiece, models[0]);
        renderer.set_visible(self.slots.driver_roof_eyepiece, true);
        renderer.set_model(self.slots.radio_roof_eyepiece, models[2]);
        renderer.set_visible(self.slots.radio_roof_eyepiece, true);
        for (slot, model) in self.slots.roof_hatches.into_iter().zip(models) {
            renderer.set_model(slot, model);
            renderer.set_visible(slot, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lids_keep_closed_source_pose_and_fixed_hinges() {
        let hull = Mat4::from_translation(Vec3::new(3.0, 0.5, -2.0)) * Mat4::from_rotation_y(0.8);
        let turret = hull * Mat4::from_rotation_y(-0.4);
        let mut lids = RoofHatches::default();
        for (closed, frame) in lids.models(hull, turret).into_iter().zip([hull, turret, hull, turret]) {
            assert!(closed.abs_diff_eq(frame, 1e-6));
        }
        lids.set(true, true, true);
        for ((open, frame), hinge) in lids
            .models(hull, turret)
            .into_iter()
            .zip([hull, turret, hull, turret])
            .zip([DRIVER_HINGE, LOADER_HINGE, RADIO_HINGE, ESCAPE_HINGE])
        {
            assert!(open
                .transform_point3(hinge)
                .abs_diff_eq(frame.transform_point3(hinge), 1e-5));
        }
        // The driver's lid lies inward from its outside hinge; the loader lid lies aft of its
        // forward hinge. Both open upward, not through the crew compartment.
        let models = lids.models(Mat4::IDENTITY, Mat4::IDENTITY);
        assert!(models[0].transform_point3(DRIVER_HINGE - Vec3::X * 0.4).y > DRIVER_HINGE.y + 0.35);
        assert!(models[1].transform_point3(LOADER_HINGE - Vec3::Z * 0.4).y > LOADER_HINGE.y + 0.35);
    }

    #[test]
    fn lid_animation_reverses_and_does_not_snap() {
        let mut lids = RoofHatches::default();
        lids.set(true, false, false);
        lids.advance(0.1);
        assert!((lids.phase[0] - 0.15).abs() < 1e-6);
        assert_eq!(lids.phase[1], 0.0);
        lids.set(false, true, false);
        lids.advance(0.1);
        assert_eq!(lids.phase[0], 0.0);
        assert!((lids.phase[1] - 0.15).abs() < 1e-6);
    }
}
