//! Four-bank post-fire stowage inspection, not replacement of deformed ammunition trays.
//! Contacts are measured on the existing fixed source trays; certification never supplies rounds.
use super::*;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[path = "cold_recommission_review.rs"]
pub mod review;

const INSPECTION_SECONDS: f32 = 2.0; // Explicit interaction pacing, not a historical repair duration.
const CONTACT_ERROR_M: f32 = 0.003;
// Measured primitive-0 triangle centroids in assets/tanks/tiger/baked/tiger_hull.glb.
// The source repair record identifies these four banks among 16 fixed trays; inspecting these
// accessible bank faces does not certify every tray individually or repair a deformed support.
const CONTACTS: [Vec3; 4] = [
    Vec3::new(-1.109214728, 1.874793353, 0.945266950),
    Vec3::new(1.109214754, 1.854039730, 0.945651216),
    Vec3::new(-1.109214754, 1.850550104, -0.870045576),
    Vec3::new(1.109214728, 1.850619167, -0.840207126),
];
const NORMALS: [Vec3; 4] = [Vec3::X, Vec3::NEG_X, Vec3::X, Vec3::NEG_X];
const SOURCE_ROOTS: [&str; 4] = ["Hull_0559", "Hull_0560", "Hull_0735", "Hull_0736"];
const SOURCE_TRIANGLES: [usize; 4] = [436, 435, 435, 437];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct RackInspectionState {
    pub progress_s: [f32; 4],
    pub active_contact: Option<u8>,
    pub primary_down: bool,
    pub last_rack_functional: bool,
}
impl Default for RackInspectionState {
    fn default() -> Self {
        Self {
            progress_s: [0.0; 4],
            active_contact: None,
            primary_down: false,
            last_rack_functional: true,
        }
    }
}
impl RackInspectionState {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.progress_s
                .iter()
                .all(|v| v.is_finite() && (0.0..=INSPECTION_SECONDS).contains(v)),
            "invalid rack inspection progress"
        );
        if let Some(index) = self.active_contact {
            ensure!(
                index < 4 && self.primary_down,
                "invalid rack inspection grip"
            );
            ensure!(
                self.progress_s[index as usize] < INSPECTION_SECONDS,
                "completed rack bank still gripped"
            );
        }
        Ok(())
    }
    fn invalidate(&mut self) {
        self.progress_s = [0.0; 4];
        self.active_contact = None;
        // Keep button state: a fresh press is required after the hazard clears.
    }
    fn observe(&mut self, rack_functional: bool, hazard: bool) {
        if hazard || (self.last_rack_functional && !rack_functional) {
            self.invalidate();
        }
        self.last_rack_functional = rack_functional;
    }
    fn advance(
        &mut self,
        down: bool,
        pressed: bool,
        target: Option<usize>,
        ready: bool,
        dt: f32,
    ) -> bool {
        let edge = pressed && !self.primary_down;
        self.primary_down = down;
        if !down {
            self.active_contact = None;
            return false;
        }
        if edge {
            self.active_contact = target
                .filter(|&i| i < 4 && self.progress_s[i] < INSPECTION_SECONDS)
                .map(|i| i as u8);
        }
        let Some(index) = self.active_contact.map(usize::from) else {
            return false;
        };
        if !ready || target != Some(index) || !dt.is_finite() {
            self.active_contact = None;
            return false;
        }
        self.progress_s[index] =
            (self.progress_s[index] + dt.clamp(0.0, 0.05)).min(INSPECTION_SECONDS);
        if self.progress_s[index] >= INSPECTION_SECONDS {
            self.active_contact = None;
        }
        self.progress_s.iter().all(|v| *v >= INSPECTION_SECONDS)
    }
}

fn supported_contact(
    cabin: &surface_fire::CabinFire,
    models: &[Mat4; surface_fire::FRAMES],
    eye: Vec3,
    look: Vec3,
    index: usize,
) -> bool {
    if index >= 4 || !eye.is_finite() || !look.is_finite() {
        return false;
    }
    let point = models[0].transform_point3(CONTACTS[index]);
    if !equipment::repair_in_reach(eye, look, point, 0.055) {
        return false;
    }
    let direction = (point - eye).normalize_or_zero();
    // Require actual retained hull steel at the measured contact, including its inward-facing
    // normal. A missing tray or a different nearer source surface cannot become an invisible button.
    cabin
        .sweep(eye, point + direction * 0.006, models)
        .is_some_and(|hit| {
            hit.frame == 0
                && hit.point.distance(CONTACTS[index]) <= CONTACT_ERROR_M
                && hit.normal.dot(NORMALS[index]) > 0.85
        })
}

impl Game {
    /// Call every ordinary workshop tick, including when the input owner has occupied hands.
    pub(super) fn advance_rack_inspection_safety(&mut self) {
        let functional = self.module_ok(ModuleKind::AmmoRack);
        let hazard = self.fire_active
            || !self.fire.is_finite()
            || self.fire > 0.0
            || self.ammo_fire.burning_round().is_some()
            || self.cabin_has_combustion();
        self.workshop.rack_inspection.observe(functional, hazard);
        if self.engine_on
            || self.engine_crank_t > 0.0
            || self.player.speed.abs() >= 0.15
            || !self.controlled_soldier_alive()
            || self.climb.active
        {
            self.workshop.rack_inspection.active_contact = None;
        }
    }
    /// Optional direct casualty hook; tick observation also catches new loss and fire.
    pub(in crate::game) fn invalidate_rack_inspection(&mut self) {
        self.workshop.rack_inspection.invalidate();
        self.workshop.rack_inspection.last_rack_functional = self.module_ok(ModuleKind::AmmoRack);
    }
    pub(super) fn rack_inspection_target(&self, eye: Vec3, look: Vec3) -> Option<usize> {
        if self.expedition_runtime.is_none() || self.module_ok(ModuleKind::AmmoRack) {
            return None;
        }
        let models = self.fire_surface_models();
        (0..4)
            .filter_map(|index| {
                let point = models[0].transform_point3(CONTACTS[index]);
                let delta = point - eye;
                let length = delta.length();
                (supported_contact(&self.cabin_fire, &models, eye, look, index)
                    && self
                        .nearest_cover_ray(eye, delta.normalize_or_zero(), (length - 0.02).max(0.0))
                        .is_none())
                .then_some((index, length))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }
    pub(super) fn route_rack_inspection(
        &mut self,
        down: bool,
        pressed: bool,
        dt: f32,
        r: &mut Renderer,
    ) -> bool {
        self.advance_rack_inspection_safety();
        let target = self.rack_inspection_target(r.camera.pos, r.camera.forward());
        let hands_free = self.workshop.state.held().is_none()
            && !self.carried_tools()
            && self.carry_mesh.is_none()
            && self.carried_casing.is_none()
            && self.held_item.is_none()
            && !self.ext_equipped
            && !self.map_held
            && !self.repair_manual.held
            && !self.compass_equipped
            && !self.panzerfaust_claimed;
        let hazard = self.fire_active
            || !self.fire.is_finite()
            || self.fire > 0.0
            || self.ammo_fire.burning_round().is_some()
            || self.cabin_has_combustion();
        let ready = self.expedition_runtime.is_some()
            && self.controlled_soldier_alive()
            && !self.climb.active
            && self.player.speed.abs() < 0.15
            && !self.engine_on
            && self.engine_crank_t <= 0.0
            && !hazard
            && hands_free
            && !self.workshop.primary_guard;
        let complete = self
            .workshop
            .rack_inspection
            .advance(down, pressed, target, ready, dt);
        let Some(index) = target else {
            return false;
        };
        self.hover_label = Some(if hazard {
            "STOWAGE UNSAFE - EXTINGUISH FIRE AND REMOVE IGNITED AMMUNITION"
        } else if self.engine_on || self.engine_crank_t > 0.0 {
            "STOP ENGINE BEFORE STOWAGE INSPECTION"
        } else if !hands_free {
            "FREE YOUR HANDS TO INSPECT STOWAGE"
        } else if self.workshop.rack_inspection.progress_s[index] >= INSPECTION_SECONDS {
            "BANK CHECKED - INSPECT THE OTHER BANKS"
        } else {
            [
                "HOLD LMB - INSPECT FRONT RIGHT STOWAGE",
                "HOLD LMB - INSPECT FRONT LEFT STOWAGE",
                "HOLD LMB - INSPECT REAR RIGHT STOWAGE",
                "HOLD LMB - INSPECT REAR LEFT STOWAGE",
            ][index]
        });
        if complete {
            if let Some(rack) = self
                .modules
                .iter_mut()
                .find(|module| module.kind == ModuleKind::AmmoRack)
            {
                rack.functional = true;
                rack.fault = tiger_damage::Fault::None;
                self.workshop.rack_inspection.last_rack_functional = true;
                self.workshop.primary_guard = true;
                self.recompute_hp();
                self.hover_label = Some("STOWAGE INSPECTED - CLEARED FOR USE");
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_is_four_distinct_reachable_contacts_with_persistent_interruption() -> Result<()> {
        let mut state = RackInspectionState::default();
        state.observe(false, false);
        for _ in 0..8 {
            assert!(!state.advance(true, true, Some(0), true, 0.05));
        }
        let interrupted = state.progress_s[0];
        assert!(!state.advance(true, false, None, false, 0.05));
        assert_eq!(state.progress_s[0], interrupted);
        let bytes = bincode::serialize(&state)?;
        let mut restored: RackInspectionState = bincode::deserialize(&bytes)?;
        restored.validate()?;
        assert_eq!(state, restored);
        // Keeping the button held while looking back cannot silently re-grip.
        assert!(!restored.advance(true, false, Some(0), true, 0.05));
        assert_eq!(restored.progress_s[0], interrupted);
        for index in 0..4 {
            restored.advance(false, false, None, true, 0.05);
            for step in 0..45 {
                let complete = restored.advance(true, step == 0, Some(index), true, 0.05);
                if complete {
                    assert_eq!(index, 3);
                }
            }
            assert_eq!(restored.progress_s[index], INSPECTION_SECONDS);
            if index < 3 {
                assert!(restored.progress_s[index + 1] < INSPECTION_SECONDS);
            }
        }
        restored.validate()?;
        Ok(())
    }

    #[test]
    fn full_source_basket_blocks_nearby_pose_but_all_four_trays_have_floor_reach() -> Result<()> {
        let hull = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_hull.glb")?;
        let fitting =
            crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_rack_fitting.glb")?;
        let turret = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_turret.glb")?;
        let gun = crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_gun.glb")?;
        let source =
            surface_fire::CabinFire::new(&[(0, &hull), (0, &fitting), (1, &turret), (2, &gun)]);
        let mut models = [Mat4::IDENTITY; surface_fire::FRAMES];
        models[1] = Mat4::from_translation(crate::tank::TIGER_PARTS.turret_pivot);
        models[2] = models[1] * Mat4::from_translation(crate::tank::TIGER_PARTS.gun_trunnion);
        let height = crate::interior_rig::FLOOR_STAND_Y + 1.29 - 0.5;
        let accepted = [(-0.35, -0.40), (0.35, -0.40), (-0.45, 0.15), (0.55, 0.15)];
        for (index, (x, z)) in accepted.into_iter().enumerate() {
            let old = Vec3::new(
                CONTACTS[index].x.signum() * 0.60,
                height,
                if index < 2 { 0.60 } else { -0.60 },
            );
            let old_eye = models[1].transform_point3(old);
            assert!(
                !supported_contact(
                    &source,
                    &models,
                    old_eye,
                    (CONTACTS[index] - old_eye).normalize(),
                    index
                ),
                "full basket occlusion must remain authoritative"
            );
            let walk = Vec3::new(x, 0.0, z);
            let bounded = clamp_interior_walk(walk, walk);
            assert!(bounded.distance(walk) < 0.001);
            // The torso cannot start inside the existing padded breech footprint either.
            let lo = crate::interior_rig::BREECH_MIN;
            let hi = crate::interior_rig::BREECH_MAX;
            assert!(!(x > lo.x - 0.16 && x < hi.x + 0.16 && z > lo.z - 0.16 && z < hi.z + 0.16));
            let eye = models[1].transform_point3(Vec3::new(x, height, z));
            assert!(
                supported_contact(
                    &source,
                    &models,
                    eye,
                    (CONTACTS[index] - eye).normalize(),
                    index
                ),
                "bank {index} has no source-supported normal crouched floor reach"
            );
        }
        Ok(())
    }

    #[test]
    fn new_fire_or_rack_loss_invalidates_checks_without_rearming_held_input() {
        let mut state = RackInspectionState {
            progress_s: [1.0; 4],
            active_contact: Some(1),
            primary_down: true,
            last_rack_functional: false,
        };
        state.observe(false, true);
        assert_eq!(state.progress_s, [0.0; 4]);
        assert_eq!(state.active_contact, None);
        assert!(!state.advance(true, false, Some(1), true, 1.0));
        state.progress_s = [1.0; 4];
        state.last_rack_functional = true;
        state.observe(false, false);
        assert_eq!(state.progress_s, [0.0; 4]);
    }

    #[test]
    fn persisted_inspection_rejects_nonfinite_progress_and_invalid_grip() {
        for value in [f32::NAN, f32::INFINITY, -0.1, INSPECTION_SECONDS + 0.1] {
            let mut state = RackInspectionState::default();
            state.progress_s[2] = value;
            assert!(state.validate().is_err());
        }
        let mut state = RackInspectionState::default();
        state.active_contact = Some(0);
        assert!(state.validate().is_err());
        state.primary_down = true;
        state.active_contact = Some(4);
        assert!(state.validate().is_err());
    }

    #[test]
    fn four_inspection_points_are_actual_named_tray_triangles_and_blockers_reject_reach(
    ) -> Result<()> {
        let parts = crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_hull.glb",
            &SOURCE_ROOTS,
        )?;
        let models = [Mat4::IDENTITY; surface_fire::FRAMES];
        for index in 0..4 {
            let mesh = &parts[index + 1];
            ensure!(mesh.indices.len() / 3 == 788, "source tray changed");
            let tri = &mesh.indices[SOURCE_TRIANGLES[index] * 3..SOURCE_TRIANGLES[index] * 3 + 3];
            let p = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(mesh.vertices[i as usize].pos));
            assert!(((p[0] + p[1] + p[2]) / 3.0 - CONTACTS[index]).length() < 2e-6);
            let eye = CONTACTS[index] + NORMALS[index] * 0.35;
            let direction = -NORMALS[index];
            let source = surface_fire::CabinFire::new(&[(0, mesh)]);
            assert!(supported_contact(&source, &models, eye, direction, index));
            assert!(!supported_contact(&source, &models, eye, -direction, index));
            assert!(!supported_contact(
                &source,
                &models,
                CONTACTS[index] + NORMALS[index] * 2.0,
                direction,
                index
            ));
            let empty = surface_fire::CabinFire::new(&[]);
            assert!(
                !supported_contact(&empty, &models, eye, direction, index),
                "missing tray is no button"
            );
            let middle = eye.lerp(CONTACTS[index], 0.5);
            let blocker = crate::prims::box_mesh(
                middle - Vec3::splat(0.04),
                middle + Vec3::splat(0.04),
                [80, 80, 80],
            );
            let obstructed = surface_fire::CabinFire::new(&[(0, mesh), (1, &blocker)]);
            assert!(!supported_contact(
                &obstructed,
                &models,
                eye,
                direction,
                index
            ));
        }
        Ok(())
    }
}
