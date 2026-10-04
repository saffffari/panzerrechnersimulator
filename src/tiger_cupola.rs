//! Source-mesh cupola control: the thin crank is the target, never the spindle or lid.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};

pub const GRIP_ROOT: &str = "Turret_0160";

/// The spindle and thin grip share the source's dark grease-metal material. Preserve that
/// finish as well as the authored mesh: multiplying sRGB texels here clips the pivot pale
/// while its neighboring crank remains dark, regardless of the actual cabin lighting.
pub fn load_spindle() -> anyhow::Result<CpuMesh> {
    crate::gltf_load::load_glb("assets/tanks/tiger/baked/tiger_part_hatchlever.glb")
}

/// The grip was exported in turret space; use the lid's existing articulated frame so its
/// source seam with the independently exported spindle survives lift and swing.
pub fn into_hatch_frame(grip: CpuMesh) -> CpuMesh {
    grip.transformed(Mat4::from_translation(-super::CUPOLA_HINGE))
}

/// The lever is worked from the cupola seat with the head in, or reached up to from the
/// loader's place below it. Aiming at the grip or its pivot works the hatch; anywhere else on
/// the cupola from below still climbs up into it.
fn controls_available(station: super::Station, head_out: bool, switching: f32) -> bool {
    matches!(station, super::Station::Commander | super::Station::Loader) && !head_out && switching <= 0.0
}

fn visible_grip_hit(
    eye: Vec3, forward: Vec3, model: Mat4, grip: &CpuMesh,
    blocked: impl FnOnce(Vec3, Vec3) -> bool,
) -> bool {
    let Some(distance) = super::ray_loader_ammunition_mesh(eye, forward, model, 1.9, grip)
        else { return false; };
    // Stop just short of the grip; the shared collision source includes the grip itself.
    !blocked(eye, eye + forward * (distance - 0.002).max(0.0))
}

/// Cupola body clicks inside the tank enter the viewing station. Exterior clicks must still
/// operate its roof lid, and all other authored hatches retain their source-mesh targets.
pub fn source_hatch_target(frame: usize, inside: bool) -> Option<usize> {
    match frame {
        3 => Some(0), 4 => Some(1), 14 => Some(2), 15 => Some(3),
        6 if !inside => Some(4),
        _ => None,
    }
}

impl super::Game {
    /// Review input changes only the current operator's head direction; LMB still has to enter
    /// the cupola and work its real control through the same hover/authority path as live play.
    pub fn debug_face_cupola(&mut self, target: &str) -> bool {
        if self.mode != super::Mode::Tank
            || !matches!(self.station, super::Station::Loader | super::Station::Commander) {
            return false;
        }
        let point = match target {
            "entry" => self.player.turret_model().transform_point3(Vec3::new(0.564, 2.46, -0.42)),
            "grip" => super::cupola_hatch_model(self.player.turret_model(), self.hatch_t)
                .transform_point3(self.cupola_grip_source.center()),
            "spindle" => super::cupola_hatch_lever_model(self.player.turret_model(), self.hatch_t)
                .transform_point3(Vec3::ZERO),
            block if block.starts_with("block") => {
                let Some(index) = block[5..].parse::<usize>().ok()
                    .filter(|&i| i < super::tiger_periscopes::CUPOLA_CENTERS.len()) else { return false; };
                self.player.turret_model().transform_point3(super::tiger_periscopes::CUPOLA_CENTERS[index])
            }
            _ => return false,
        };
        let eye = self.station_eye(self.station);
        let to = (point - eye).normalize_or_zero();
        let base = self.player.turret_forward();
        self.head_yaw = (to.x.atan2(-to.z) - base.x.atan2(-base.z)
            + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.head_pitch = if self.station == super::Station::Commander {
            // Match the live commander's fixed -0.10 rad deck look and actual head limits.
            (to.y.clamp(-0.999, 0.999).asin() + 0.10).clamp(-0.5, 0.7)
        } else {
            (to.y.clamp(-0.999, 0.999).asin()
                - base.y.clamp(-0.999, 0.999).asin()).clamp(-1.2, 1.0)
        };
        true
    }

    pub(super) fn cupola_grip_hover(&self, renderer: &crate::renderer::Renderer) -> bool {
        if !controls_available(self.station, self.commander_outside, self.switch_t)
            || !self.local_can_work_cupola() || self.interior_light <= 0.02
            || self.carry_mesh.is_some() || self.carried_casing.is_some() || self.ext_equipped
            || self.compass_equipped || self.held_item.is_some() || self.activity.is_some()
            || self.climb.active || self.cupola_focus.phase > 0.0 {
            return false;
        }
        let models = self.fire_surface_models();
        // The thin grip or the pivot cylinder it turns: both are the hatch control. The grip alone
        // was too small a target (owner, 2026-10-03).
        let (eye, forward) = (renderer.camera.pos, renderer.camera.forward());
        [&self.cupola_grip_source, &self.cupola_spindle_source].into_iter().any(|part|
            visible_grip_hit(eye, forward, models[6], part,
                |from, to| self.cabin_fire.sweep(from, to, &models).is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_grip() -> CpuMesh {
        crate::gltf_load::load_glb_partitioned_roots(
            "assets/tanks/tiger/baked/tiger_turret.glb", &[GRIP_ROOT],
        ).unwrap().remove(1)
    }

    #[test]
    fn spindle_and_grip_keep_the_same_authored_grease_metal() {
        fn finish(mesh: &CpuMesh) -> ([u8; 4], f32, f32) {
            let uv = mesh.vertices[0].uv;
            let sample = |image: &crate::gltf_load::CpuImage| -> [u8; 4] {
                let x = ((uv[0] * image.width as f32).floor() as u32).min(image.width - 1);
                let y = ((uv[1] * image.height as f32).floor() as u32).min(image.height - 1);
                let start = ((y * image.width + x) * 4) as usize;
                image.rgba[start..start + 4].try_into().unwrap()
            };
            let base = sample(mesh.texture.as_ref().unwrap());
            let mr = mesh.metallic_roughness_texture.as_ref().map(sample).unwrap_or([255; 4]);
            (base, mr[2] as f32 / 255.0 * mesh.metallic_factor,
                mr[1] as f32 / 255.0 * mesh.roughness_factor)
        }
        let spindle = load_spindle().unwrap();
        let grip = source_grip();
        assert_eq!(finish(&spindle), finish(&grip),
            "source spindle and crank must share their finish without a separate albedo gain");
        let (base, metallic, roughness) = finish(&spindle);
        assert!(base[0] < 100 && base[0] == base[1] && base[1] == base[2]);
        assert!(metallic > 0.99 && (0.44..0.46).contains(&roughness));
    }

    #[test]
    fn source_grip_is_the_thin_crank_and_retains_its_closed_pose() {
        let source = source_grip();
        let size = source.max - source.min;
        assert!(size.x < 0.04 && size.y < 0.035 && size.z > 0.22 && size.z < 0.23,
            "the source target must be the thin handle, not the cylindrical spindle");
        let moved = into_hatch_frame(source.clone());
        let closed = super::super::cupola_hatch_model(Mat4::IDENTITY, 0.0);
        for (before, after) in source.vertices.iter().zip(&moved.vertices) {
            assert!(closed.transform_point3(Vec3::from(after.pos))
                .abs_diff_eq(Vec3::from(before.pos), 1e-6));
        }
        assert_eq!(source.indices, moved.indices);
    }

    #[test]
    fn actual_grip_can_be_picked_but_spindle_and_near_misses_cannot() {
        let source = source_grip();
        let center = source.center();
        let grip = into_hatch_frame(source);
        for yaw in [0.0, 0.8, 2.1] {
            let turret = Mat4::from_translation(Vec3::new(9.0, 0.5, -12.0))
                * Mat4::from_rotation_y(yaw);
            for phase in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let model = super::super::cupola_hatch_model(turret, phase);
                let target = model.transform_point3(center - super::super::CUPOLA_HINGE);
                let eye = target - model.transform_vector3(Vec3::X) * 0.4;
                let forward = (target - eye).normalize();
                assert!(visible_grip_hit(eye, forward, model, &grip, |_, _| false));
                assert!(!visible_grip_hit(eye, forward, model, &grip, |_, _| true),
                    "a wall before the actual grip must occlude its interaction");
                let miss = target + model.transform_vector3(Vec3::Y) * 0.06;
                assert!(!visible_grip_hit(eye, (miss-eye).normalize(), model, &grip, |_, _| false));
                let spindle = model.transform_point3(
                    super::super::HATCHLEVER_POS - super::super::CUPOLA_HINGE);
                let spindle_eye = spindle - model.transform_vector3(Vec3::X) * 0.4;
                assert!(!visible_grip_hit(spindle_eye, (spindle-spindle_eye).normalize(), model,
                    &grip, |_, _| false), "rotating cylinder is not the hatch control target");
            }
        }
    }

    #[test]
    fn cupola_entry_is_separate_from_hatch_operation_and_other_hatches_keep_working() {
        assert!(controls_available(super::super::Station::Loader, false, 0.0), "the lever is reachable from below");
        assert!(!controls_available(super::super::Station::Gunner, false, 0.0));
        assert!(!controls_available(super::super::Station::Commander, false, 0.1));
        assert!(!controls_available(super::super::Station::Commander, true, 0.0));
        assert!(controls_available(super::super::Station::Commander, false, 0.0));
        assert_eq!(source_hatch_target(6, true), None);
        assert_eq!(source_hatch_target(6, false), Some(4));
        for (frame, hatch) in [(3, 0), (4, 1), (14, 2), (15, 3)] {
            assert_eq!(source_hatch_target(frame, true), Some(hatch));
            assert_eq!(source_hatch_target(frame, false), Some(hatch));
        }
    }
}
