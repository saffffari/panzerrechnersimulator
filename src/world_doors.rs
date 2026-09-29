//! Source-authored Pizza door leaves. The same moving model matrices feed primary rendering,
//! ray-visible geometry, interaction, swept foot contact and line-of-sight blocking.
//! Door frames are retained; a known embedded static leaf is removed by subtree partition only.
use crate::{gltf_load::CpuMesh, renderer::Renderer};
use glam::{Mat4, Vec2, Vec3};

pub const LEAF_PATH: &str = concat!(
    "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
    "Doors & Gates/door_hr_14.glb"
);
const REACH: f32 = 2.2;
const OPEN_ANGLE: f32 = 110.0 * std::f32::consts::PI / 180.0;
const SPEED: f32 = 2.4;
// Source trim reaches +0.21 m; the live feature jamb proxy reaches +0.23 m.
const HINGE_Z: f32 = 0.23 + 0.025 + 0.005;
const FOOT_RADIUS: f32 = 0.28;
const LEAF_MIN: Vec3 = Vec3::new(-1.0, 0.0, -0.025);
const LEAF_MAX: Vec3 = Vec3::new(0.0, 2.1, 0.025);

#[derive(Clone, Debug)]
pub struct Frame {
    pub model: Mat4,
    pub width: f32,
    pub height: f32,
    pub initially_open: bool,
    /// Existing BuildingState door key, absent for independently authored manifest frames.
    pub feature_id: Option<String>,
}
impl Frame {
    pub fn from_asset(path: &str, model: Mat4) -> Option<Self> {
        let stem = std::path::Path::new(path).file_stem()?.to_str()?;
        let (width, height) = match stem {
            "doorway_hr_1" => (1.6, 2.1),
            "doorway_hr_2_wide" => (2.2, 2.5),
            _ => return None,
        };
        Some(Self {
            model,
            width,
            height,
            initially_open: false,
            feature_id: None,
        })
    }
}
/// Only this known descendant is detached; jambs, lintel and all their materials stay in place.
pub fn load_frame(path: &str) -> anyhow::Result<Option<CpuMesh>> {
    if Frame::from_asset(path, Mat4::IDENTITY).is_none() {
        return Ok(None);
    }
    if std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        == Some("doorway_hr_1")
    {
        Ok(Some(
            crate::gltf_load::load_glb_partitioned_subtrees(path, &["metal_door_1"])?.remove(0),
        ))
    } else {
        Ok(Some(crate::gltf_load::load_glb(path)?))
    }
}
#[derive(Clone, Debug)]
struct Door {
    frame: Frame,
    angle: f32,
    target: f32,
    alive: bool,
    blocked: bool,
    locked: bool,
}
impl Door {
    fn models_at(&self, angle: f32) -> [Mat4; 2] {
        let half = self.frame.width * 0.5;
        std::array::from_fn(|side| {
            let (x, base, sign) = if side == 0 {
                (-half, std::f32::consts::PI, 1.0)
            } else {
                (half, 0.0, -1.0)
            };
            self.frame.model
                * Mat4::from_translation(Vec3::new(x, 0.012, HINGE_Z))
                * Mat4::from_rotation_y(base + angle * sign)
                * Mat4::from_scale(Vec3::new(
                    half - 0.004,
                    (self.frame.height - 0.024) / 2.1,
                    1.0,
                ))
        })
    }
    fn models(&self) -> [Mat4; 2] {
        self.models_at(self.angle)
    }
}
#[derive(Default)]
pub struct WorldDoors {
    doors: Vec<Door>,
    batches: Vec<usize>,
}
impl WorldDoors {
    pub fn new(frames: Vec<Frame>) -> anyhow::Result<Self> {
        // Storage follows the authored frames. There is no shared 512-door GPU reservation:
        // initialize allocates a fixed two-leaf pool per frame from the renderer's shared
        // instance budget. Campaign Act I already contains 576 manifest frames plus its
        // three feature doors, so a separate subsystem cap rejected a valid campaign scene.
        for f in &frames {
            let up = f.model.y_axis.truncate();
            anyhow::ensure!(
                f.model.is_finite()
                    && f.model.determinant().abs() > 1e-5
                    && up.normalize_or_zero().dot(Vec3::Y) > 0.995
                    && f.width > 0.5
                    && f.height > 1.5,
                "door frame must retain an upright, finite source transform"
            );
        }
        Ok(Self {
            doors: frames
                .into_iter()
                .map(|frame| {
                    let angle = if frame.initially_open {
                        -OPEN_ANGLE
                    } else {
                        0.0
                    };
                    Door {
                        frame,
                        angle,
                        target: angle,
                        alive: true,
                        blocked: false,
                        locked: false,
                    }
                })
                .collect(),
            batches: Vec::new(),
        })
    }
    pub fn initialize(
        &mut self,
        r: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        if self.doors.is_empty() {
            return Ok(());
        }
        let mesh = crate::gltf_load::load_glb(LEAF_PATH)?;
        anyhow::ensure!(
            (mesh.min - Vec3::new(-1.0, 0.0, 0.0)).length() < 0.001
                && (mesh.max - Vec3::new(0.0, 2.1, 0.05)).length() < 0.001,
            "Pizza door source hinge/bounds changed"
        );
        // Center its thickness around the authored x=0 vertical hinge without changing source art.
        let mesh = mesh.transformed(Mat4::from_translation(Vec3::new(0.0, 0.0, -0.025)));
        let prototype = r.register_instanced_prototype(device, queue, &mesh);
        // Each pair has its own two reserved IDs. Destroying one frame cannot reassign another
        // door's history/RT identity, and an empty pair leaves no hidden off-world geometry.
        for _ in &self.doors {
            self.batches.push(
                r.add_moving_instanced_pool(device, prototype, 2)
                    .ok_or_else(|| {
                        anyhow::anyhow!("physical moving door pool allocation failed")
                    })?,
            );
        }
        self.upload(r);
        Ok(())
    }
    pub fn upload(&self, r: &mut Renderer) {
        for (&batch, door) in self.batches.iter().zip(&self.doors) {
            let models = door.models();
            let matrices = if door.alive { &models[..] } else { &[] };
            assert!(
                r.set_moving_instanced_mats(batch, matrices),
                "fixed physical door capacity"
            );
        }
    }
    pub fn ray(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<(usize, f32)> {
        self.doors
            .iter()
            .enumerate()
            .filter(|(_, d)| d.alive)
            .flat_map(|(index, door)| {
                door.models().into_iter().filter_map(move |model| {
                    let inv = model.inverse();
                    let o = inv.transform_point3(origin);
                    let d = inv.transform_vector3(direction);
                    slab(o, d, LEAF_MIN, LEAF_MAX, max_distance)
                        .map(|(distance, _)| (index, distance))
                })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
    pub fn blocks_segment(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let distance = delta.length();
        distance > 0.002 && self.ray(from, delta / distance, distance - 0.001).is_some()
    }
    pub fn hover(&self, origin: Vec3, direction: Vec3) -> Option<usize> {
        // Centimetre hand tolerance covers the real centre seam; ballistic rays remain exact OBBs.
        let direction = direction.normalize_or_zero();
        self.doors
            .iter()
            .enumerate()
            .filter(|(_, door)| door.alive)
            .flat_map(|(index, door)| {
                door.models().into_iter().filter_map(move |model| {
                    let inv = model.inverse();
                    let pad = Vec3::new(0.012 / model.x_axis.truncate().length(), 0.008, 0.008);
                    slab(
                        inv.transform_point3(origin),
                        inv.transform_vector3(direction),
                        LEAF_MIN - pad,
                        LEAF_MAX + pad,
                        REACH,
                    )
                    .map(|(distance, _)| (index, distance))
                })
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|hit| hit.0)
    }
    pub fn interaction_point(&self, index: usize, observer: Vec3) -> Option<Vec3> {
        self.doors.get(index).filter(|d| d.alive).map(|door| {
            door.models()
                .iter()
                .map(|model| {
                    let local = model
                        .inverse()
                        .transform_point3(observer)
                        .clamp(LEAF_MIN, LEAF_MAX);
                    model.transform_point3(local)
                })
                .min_by(|a, b| {
                    a.distance_squared(observer)
                        .total_cmp(&b.distance_squared(observer))
                })
                .unwrap()
        })
    }
    pub fn label(&self, index: usize) -> &'static str {
        let d = &self.doors[index];
        if d.locked {
            "DOOR LOCKED"
        } else if d.blocked {
            "DOOR BLOCKED - STEP BACK [E]"
        } else if d.target.abs() > 0.01 {
            "CLOSE DOOR [E]"
        } else {
            "OPEN DOOR [E]"
        }
    }
    pub fn toggle(&mut self, index: usize, _observer: Vec3) -> bool {
        let Some(door) = self.doors.get_mut(index).filter(|d| d.alive) else {
            return false;
        };
        if door.locked {
            return true;
        } // consume E so a locked door cannot trigger a pickup behind it
        door.target = if door.target.abs() > 0.01 {
            0.0
        } else {
            // A surface-mounted hinge opens toward the authored front. Reversing its side merely
            // because the player is behind it would drive the leaf through the solid jamb.
            -OPEN_ANGLE
        };
        door.blocked = false;
        true
    }
    pub fn update(&mut self, dt: f32, blockers: &[(Vec3, f32, f32)]) {
        let duration = dt.clamp(0.0, 0.1);
        for door in &mut self.doors {
            if !door.alive || door.locked || (door.target - door.angle).abs() < 1e-5 {
                continue;
            }
            let delta = (door.target - door.angle).clamp(-SPEED * duration, SPEED * duration);
            let steps = (delta.abs() / 0.02).ceil().max(1.0) as usize;
            door.blocked = false;
            for _ in 0..steps {
                let next = door.angle + delta / steps as f32;
                if door.models_at(next).iter().any(|&model| {
                    blockers.iter().any(|&(feet, radius, height)| {
                        cylinder_overlaps(model, feet, radius, height)
                    })
                }) {
                    door.blocked = true;
                    break;
                }
                door.angle = next;
            }
        }
    }
    /// Rotated leaf OBBs, not their much wider world-AABB shadows, block the swept soldier.
    pub fn resolve_walk(&self, old: Vec3, attempt: Vec3, height: f32) -> Vec3 {
        let mut position = old;
        let mut remaining = attempt - old;
        for _ in 0..3 {
            let mut closest: Option<(f32, Vec3)> = None;
            for door in self.doors.iter().filter(|door| door.alive) {
                if Vec2::new(
                    old.x - door.frame.model.w_axis.x,
                    old.z - door.frame.model.w_axis.z,
                )
                .length()
                    > door.frame.width * door.frame.model.x_axis.truncate().length() * 2.0
                        + (attempt - old).length()
                        + 1.0
                {
                    continue;
                }
                for model in door.models() {
                    let inv = model.inverse();
                    let origin = inv.transform_point3(position);
                    let top = inv.transform_point3(position + Vec3::Y * height);
                    if top.y < LEAF_MIN.y || origin.y > LEAF_MAX.y {
                        continue;
                    }
                    let sx = model.x_axis.truncate().length();
                    let sz = model.z_axis.truncate().length();
                    let min = LEAF_MIN - Vec3::new(FOOT_RADIUS / sx, 0.0, FOOT_RADIUS / sz);
                    let max = LEAF_MAX + Vec3::new(FOOT_RADIUS / sx, 0.0, FOOT_RADIUS / sz);
                    let ray = inv.transform_vector3(remaining);
                    let mid = Vec3::new(origin.x, (LEAF_MIN.y + LEAF_MAX.y) * 0.5, origin.z);
                    if let Some((t, n)) = slab(mid, Vec3::new(ray.x, 0.0, ray.z), min, max, 1.0) {
                        if n.length_squared() > 0.5 && closest.is_none_or(|(old, _)| t < old) {
                            closest = Some((t, inv.transpose().transform_vector3(n).normalize()));
                        }
                    }
                }
            }
            let Some((t, normal)) = closest else {
                position += remaining;
                remaining = Vec3::ZERO;
                break;
            };
            position += remaining * (t - 0.0001).max(0.0) + normal * 0.0001;
            remaining *= 1.0 - t;
            remaining -= normal * remaining.dot(normal);
        }
        // If three contacts exhaust the slide budget, retain the last safe point.
        position.y = attempt.y;
        position
    }
    /// Apply (alive, locked) to every feature door named in `states`, in one pass over the doors.
    pub fn sync_feature_states(
        &mut self,
        states: &std::collections::HashMap<String, (bool, bool)>,
    ) {
        for door in &mut self.doors {
            if let Some(&(alive, locked)) = door
                .frame
                .feature_id
                .as_ref()
                .and_then(|id| states.get(id))
            {
                door.alive = alive;
                door.locked = locked;
            }
        }
    }

    pub fn set_feature_alive(&mut self, id: &str, alive: bool) {
        for d in &mut self.doors {
            if d.frame.feature_id.as_deref() == Some(id) {
                d.alive = alive;
            }
        }
    }
    pub fn set_feature_locked(&mut self, id: &str, locked: bool) {
        for d in &mut self.doors {
            if d.frame.feature_id.as_deref() == Some(id) {
                d.locked = locked;
            }
        }
    }
    pub fn feature_targets(&self) -> Vec<(String, bool)> {
        self.doors
            .iter()
            .filter_map(|d| {
                d.frame
                    .feature_id
                    .clone()
                    .map(|id| (id, d.angle.abs() > 1.40))
            })
            .collect()
    }
    pub fn restore_feature(&mut self, id: &str, open: bool, alive: bool) {
        for door in &mut self.doors {
            if door.frame.feature_id.as_deref() == Some(id) {
                door.angle = if open { -OPEN_ANGLE } else { 0.0 };
                door.target = door.angle;
                door.alive = alive;
                door.blocked = false;
            }
        }
    }

    pub fn reset(&mut self) {
        for d in &mut self.doors {
            d.angle = if d.frame.initially_open {
                -OPEN_ANGLE
            } else {
                0.0
            };
            d.target = d.angle;
            d.alive = true;
            d.blocked = false;
            d.locked = false;
        }
    }
    pub fn telemetry(&self) -> serde_json::Value {
        let aperture_blocked = self
            .doors
            .iter()
            .filter(|d| d.alive)
            .filter(|d| {
                let from = d.frame.model.transform_point3(Vec3::new(0.2, 1.0, -1.0));
                let to = d.frame.model.transform_point3(Vec3::new(0.2, 1.0, 1.5));
                self.blocks_segment(from, to)
            })
            .count();
        serde_json::json!({"pairs":self.doors.len(),"blocked_apertures":aperture_blocked,"live_leaves":self.doors.iter().filter(|d|d.alive).count()*2,
            "open_pairs":self.doors.iter().filter(|d|d.alive&&d.angle.abs()>1.6).count(),
            "moving_pairs":self.doors.iter().filter(|d|d.alive&&(d.target-d.angle).abs()>0.001).count(),
            "blocked_pairs":self.doors.iter().filter(|d|d.blocked).count(),"physical_moving_rt":!self.batches.is_empty(),"locked_pairs":self.doors.iter().filter(|d|d.alive&&d.locked).count()})
    }
}
fn cylinder_overlaps(model: Mat4, feet: Vec3, radius: f32, height: f32) -> bool {
    let inv = model.inverse();
    let local = inv.transform_point3(feet);
    let top = inv.transform_point3(feet + Vec3::Y * height);
    if top.y < LEAF_MIN.y || local.y > LEAF_MAX.y {
        return false;
    }
    let closest = Vec3::new(
        local.x.clamp(LEAF_MIN.x, LEAF_MAX.x),
        local.y,
        local.z.clamp(LEAF_MIN.z, LEAF_MAX.z),
    );
    let world = model.transform_point3(closest);
    Vec2::new(world.x - feet.x, world.z - feet.z).length() < radius + 0.005
}
/// Slab parameter retains world metres when the original direction was unit length. A start
/// already inside has no manufactured normal, allowing a loaded-in overlap to escape.
fn slab(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3, limit: f32) -> Option<(f32, Vec3)> {
    if !origin.is_finite() || !direction.is_finite() || direction.length_squared() < 1e-12 {
        return None;
    }
    let mut enter = 0.0_f32;
    let mut exit = limit;
    let mut normal = Vec3::ZERO;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-9 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let a = (min[axis] - origin[axis]) / direction[axis];
        let b = (max[axis] - origin[axis]) / direction[axis];
        let near = a.min(b);
        if near > enter {
            enter = near;
            normal = Vec3::ZERO;
            normal[axis] = -direction[axis].signum();
        }
        exit = exit.min(a.max(b));
        if enter > exit {
            return None;
        }
    }
    (exit >= 0.0 && enter <= limit).then_some((enter, normal))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doors() -> WorldDoors {
        WorldDoors::new(vec![
            Frame::from_asset("doorway_hr_1.glb", Mat4::IDENTITY).unwrap()
        ])
        .unwrap()
    }
    #[test]
    fn campaign_sized_door_set_keeps_tail_interaction_and_collision() {
        // Act I has 576 authored frames plus three feature doors. Exercise the last pair,
        // beyond the former 512-frame guard, instead of only checking construction succeeds.
        let frames = (0..579)
            .map(|index| {
                Frame::from_asset(
                    "doorway_hr_1.glb",
                    Mat4::from_translation(Vec3::new(index as f32 * 4.0, 0.0, 0.0)),
                )
                .unwrap()
            })
            .collect();
        let mut doors = WorldDoors::new(frames).unwrap();
        let from = Vec3::new(578.0 * 4.0 + 0.2, 0.0, 1.0);
        let to = Vec3::new(from.x, 0.0, -1.0);
        assert_eq!(doors.hover(from + Vec3::Y, -Vec3::Z), Some(578));
        assert!(doors.blocks_segment(from + Vec3::Y, to + Vec3::Y));
        assert!(doors.resolve_walk(from, to, 1.8288).z > 0.27);
        assert!(doors.toggle(578, from));
        for _ in 0..20 {
            doors.update(0.05, &[]);
        }
        assert!(!doors.blocks_segment(from + Vec3::Y, to + Vec3::Y));
        assert!(doors.resolve_walk(from, to, 1.8288).distance(to) < 1e-4);
        assert_eq!(doors.doors[0].angle, 0.0);
    }
    #[test]
    fn hinged_pair_fills_aperture_and_preserves_actual_hinge() {
        let doors = doors();
        let door = &doors.doors[0];
        let closed = door.models();
        let open = door.models_at(OPEN_ANGLE);
        for side in 0..2 {
            for y in [0.0, 1.0, 2.1] {
                let hinge = Vec3::new(0.0, y, 0.0);
                assert!(
                    closed[side]
                        .transform_point3(hinge)
                        .distance(open[side].transform_point3(hinge))
                        < 1e-5
                );
            }
        }
        assert!(doors.blocks_segment(Vec3::new(0.2, 1.0, -1.0), Vec3::new(0.2, 1.0, 1.0)));
        assert!(!doors.blocks_segment(Vec3::new(1.0, 1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)));
    }
    #[test]
    fn outward_hinge_sweep_clears_the_authored_frame_and_its_collision_margin() {
        let doors = doors();
        for step in 0..=110 {
            let angle = -(step as f32).to_radians();
            for model in doors.doors[0].models_at(angle) {
                for x in [LEAF_MIN.x, LEAF_MAX.x] {
                    for z in [LEAF_MIN.z, LEAF_MAX.z] {
                        assert!(model.transform_point3(Vec3::new(x, 1.0, z)).z >= 0.2349);
                    }
                }
            }
        }
    }

    #[test]
    fn reversible_open_close_changes_walk_and_los_together() {
        let mut doors = doors();
        let from = Vec3::new(0.2, 0.0, 1.0);
        let to = Vec3::new(0.2, 0.0, -1.0);
        assert!(doors.resolve_walk(from, to, 1.8288).z > 0.27);
        assert_eq!(doors.hover(Vec3::new(0.2, 1.0, 1.0), -Vec3::Z), Some(0));
        doors.toggle(0, from);
        for _ in 0..20 {
            doors.update(0.05, &[]);
        }
        assert!(!doors.blocks_segment(from + Vec3::Y, to + Vec3::Y));
        assert!(doors.resolve_walk(from, to, 1.8288).distance(to) < 1e-4);
        doors.toggle(0, from);
        for _ in 0..20 {
            doors.update(0.05, &[]);
        }
        assert!(doors.blocks_segment(from + Vec3::Y, to + Vec3::Y));
    }
    #[test]
    fn closing_stops_before_passing_through_soldier_then_resumes() {
        let mut doors = doors();
        doors.toggle(0, Vec3::Z);
        for _ in 0..20 {
            doors.update(0.05, &[]);
        }
        doors.toggle(0, Vec3::Z);
        let blocker = (Vec3::new(0.15, 0.0, HINGE_Z), 0.28, 1.8288);
        for _ in 0..30 {
            doors.update(0.1, &[blocker]);
        }
        assert!(doors.doors[0].blocked);
        assert!(doors.doors[0].angle.abs() > 0.1);
        assert!(!doors.doors[0]
            .models()
            .iter()
            .any(|&m| cylinder_overlaps(m, blocker.0, blocker.1, blocker.2)));
        for _ in 0..30 {
            doors.update(0.1, &[]);
        }
        assert!(doors.doors[0].angle.abs() < 1e-4);
    }
    #[test]
    fn rotated_scaled_leaf_does_not_become_a_world_aabb_barrier() {
        let model = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(0.63),
            Vec3::new(12.0, 3.0, -8.0),
        ) * Mat4::from_scale(Vec3::splat(1.2));
        let mut frame = Frame::from_asset("doorway_hr_2_wide.glb", model).unwrap();
        frame.initially_open = true;
        let doors = WorldDoors::new(vec![frame]).unwrap();
        let from = model.transform_point3(Vec3::new(0.0, 0.0, 1.0));
        let to = model.transform_point3(Vec3::new(0.0, 0.0, -1.0));
        assert!(!doors.blocks_segment(from + Vec3::Y, to + Vec3::Y));
        assert!(doors.resolve_walk(from, to, 1.8288).distance(to) < 1e-4);
    }
    #[test]
    fn actual_pizza_leaf_and_frame_partition_keep_source_art() {
        let leaf = crate::gltf_load::load_glb(LEAF_PATH).unwrap();
        assert!((leaf.min.x + 1.0).abs() < 0.001);
        assert!(leaf.max.x.abs() < 0.001);
        let path = LEAF_PATH.replace(
            "Doors & Gates/door_hr_14.glb",
            "Modular Structures/doorway_hr_1.glb",
        );
        let full = crate::gltf_load::load_glb(&path).unwrap();
        let parts =
            crate::gltf_load::load_glb_partitioned_subtrees(&path, &["metal_door_1"]).unwrap();
        let remainder = load_frame(&path).unwrap().unwrap();
        assert_eq!(
            full.indices.len(),
            parts[0].indices.len() + parts[1].indices.len()
        );
        assert_eq!(remainder.indices.len(), parts[0].indices.len());
        assert!(!parts[1].indices.is_empty());
        assert!(remainder.texture.is_some() && leaf.texture.is_some());
    }
}
