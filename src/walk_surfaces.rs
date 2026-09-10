//! Read-only source-triangle floor support for ordinary manifest slabs. The featured building
//! retains its damage-aware support system. This layer never infers a floor from a mesh AABB.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec2, Vec3};
use std::collections::HashMap;
const BIN: f32 = 2.0;
const STEP: f32 = 0.30;
const RADIUS: f32 = 0.28;
const MAX_TRIANGLES: usize = 250_000;
#[derive(Clone, Copy)]
struct Triangle {
    points: [Vec3; 3],
    floor: bool,
}
#[derive(Default)]
pub struct WalkSurfaces {
    triangles: Vec<Triangle>,
    bins: HashMap<(i32, i32), Vec<usize>>,
}
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub position: Vec3,
    pub velocity_y: f32,
    pub airborne: bool,
}
fn cell(v: f32) -> i32 {
    (v / BIN).floor() as i32
}
impl WalkSurfaces {
    pub fn accepts(path: &str) -> bool {
        let stem = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        stem.starts_with("floor_ceiling_") || stem.starts_with("roof_")
    }
    pub fn append(&mut self, path: &str, mesh: &CpuMesh, model: Mat4) -> anyhow::Result<()> {
        if !Self::accepts(path) {
            return Ok(());
        }
        let is_floor = std::path::Path::new(path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .starts_with("floor_ceiling_");
        let normal_model = model.inverse().transpose();
        for tri in mesh.indices.chunks_exact(3) {
            let points = std::array::from_fn(|i| {
                model.transform_point3(Vec3::from(mesh.vertices[tri[i] as usize].pos))
            });
            // Imported/procedural meshes can use different index winding conventions. Their
            // authored shading normals distinguish a slab top from its downward-facing underside.
            let local_normal: Vec3 = tri
                .iter()
                .map(|&i| Vec3::from(mesh.vertices[i as usize].normal))
                .sum();
            let normal = normal_model
                .transform_vector3(local_normal)
                .normalize_or_zero();
            if normal.y.abs() < 0.55 {
                continue;
            } // near-vertical sides remain ordinary structural cover
            anyhow::ensure!(
                self.triangles.len() < MAX_TRIANGLES,
                "source walkable triangle capacity exceeded"
            );
            let lo = points[0].min(points[1]).min(points[2]);
            let hi = points[0].max(points[1]).max(points[2]);
            let index = self.triangles.len();
            self.triangles.push(Triangle {
                points,
                floor: is_floor && normal.y > 0.55,
            });
            for z in cell(lo.z)..=cell(hi.z) {
                for x in cell(lo.x)..=cell(hi.x) {
                    self.bins.entry((x, z)).or_default().push(index);
                }
            }
        }
        Ok(())
    }
    fn candidates(&self, x: f32, z: f32, radius: f32) -> Vec<usize> {
        let mut indices = Vec::new();
        for z in cell(z - radius)..=cell(z + radius) {
            for x in cell(x - radius)..=cell(x + radius) {
                if let Some(ids) = self.bins.get(&(x, z)) {
                    indices.extend_from_slice(ids);
                }
            }
        }
        indices.sort_unstable();
        indices.dedup();
        indices
    }
    pub fn floor_y(&self, position: Vec3, low: f32, high: f32) -> Option<f32> {
        self.candidates(position.x, position.z, 0.0)
            .into_iter()
            .filter_map(|i| {
                let triangle = self.triangles[i];
                if !triangle.floor {
                    return None;
                }
                let [a, b, c] = triangle.points;
                let b = b - a;
                let c = c - a;
                let p = Vec2::new(position.x - a.x, position.z - a.z);
                let det = b.x * c.z - b.z * c.x;
                if det.abs() < 1e-10 {
                    return None;
                }
                let u = (p.x * c.z - p.y * c.x) / det;
                let v = (b.x * p.y - b.z * p.x) / det;
                if u < -1e-6 || v < -1e-6 || u + v > 1.0 + 1e-6 {
                    return None;
                }
                let y = a.y + u * b.y + v * c.y;
                (y >= low && y <= high).then_some(y)
            })
            .max_by(f32::total_cmp)
    }
    /// Lowest real horizontal/sloped surface over the entire conservative body footprint.
    /// Polygon clipping checks the triangle interior, including retessellated seams and holes.
    fn overhead_y(&self, position: Vec3, low: f32, high: f32) -> Option<f32> {
        self.candidates(position.x, position.z, RADIUS)
            .into_iter()
            .filter_map(|i| {
                let mut polygon = self.triangles[i].points.to_vec();
                for (axis, edge, positive) in [
                    (0, position.x - RADIUS, true),
                    (0, position.x + RADIUS, false),
                    (2, position.z - RADIUS, true),
                    (2, position.z + RADIUS, false),
                ] {
                    polygon = clip(&polygon, axis, edge, positive);
                }
                // Retain the part in the swept head/body interval as well: a sloped
                // roof can straddle `low` even though its lowest corner is below it.
                polygon = clip(&polygon, 1, low + 1e-5, true);
                polygon = clip(&polygon, 1, high, false);
                polygon.iter().map(|p| p.y).min_by(f32::total_cmp)
            })
            .min_by(f32::total_cmp)
    }
    pub fn resolve(&self, old: Vec3, mut motion: Motion, terrain_y: f32, standing: f32) -> Motion {
        if self.triangles.is_empty() {
            return motion;
        }
        let previous_support = self.floor_y(old, old.y - 0.04, old.y + 0.04);
        if motion.airborne && motion.velocity_y > 0.0 {
            if let Some(ceiling) = self.overhead_y(
                motion.position,
                old.y + standing - 0.02,
                motion.position.y + standing + 0.01,
            ) {
                motion.position.y = ceiling - standing - 0.005;
                motion.velocity_y = 0.0;
            }
            return motion;
        }
        let high = if motion.airborne {
            old.y + 0.02
        } else {
            old.y + STEP
        };
        let low = if motion.airborne {
            motion.position.y - 0.02
        } else {
            terrain_y - 0.02
        };
        let support = self.floor_y(motion.position, low, high);
        let leaving_ledge = !motion.airborne
            && support.is_none()
            && previous_support.is_some()
            && old.y > terrain_y + STEP;
        if leaving_ledge {
            // The terrain controller has already sampled below the slab. Restore departure
            // height BEFORE testing headroom, otherwise that same slab overlaps the artificially
            // lowered body footprint and is misclassified as a low ceiling at its edge.
            motion.position.y = old.y;
            motion.velocity_y = 0.0;
        }
        let target_y = support.unwrap_or(motion.position.y);
        let standing_position = Vec3::new(motion.position.x, target_y, motion.position.z);
        if !motion.airborne
            && self
                .overhead_y(
                    standing_position,
                    target_y + STEP + 0.01,
                    target_y + standing + 0.005,
                )
                .is_some()
        {
            // Reject a blocked step or low-ceiling entry; never squeeze the body or lower its eyes.
            motion.position = old;
            motion.velocity_y = 0.0;
            return motion;
        }
        if let Some(y) = support {
            motion.position.y = y;
            motion.velocity_y = 0.0;
            motion.airborne = false;
        } else if leaving_ledge {
            // Headroom is clear at the actual departure height; ordinary gravity acts next tick.
            motion.airborne = true;
        }
        motion
    }
}
fn clip(polygon: &[Vec3], axis: usize, edge: f32, positive: bool) -> Vec<Vec3> {
    let mut out = Vec::new();
    if polygon.is_empty() {
        return out;
    }
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let da = (a[axis] - edge) * if positive { 1.0 } else { -1.0 };
        let db = (b[axis] - edge) * if positive { 1.0 } else { -1.0 };
        if da >= 0.0 {
            out.push(a);
        }
        if (da < 0.0) != (db < 0.0) {
            out.push(a.lerp(b, da / (da - db)));
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    fn slab(x0: f32, x1: f32, low: f32, high: f32) -> CpuMesh {
        crate::prims::box_mesh(Vec3::new(x0, low, -1.0), Vec3::new(x1, high, 1.0), [80; 3])
    }
    fn grounded(p: Vec3) -> Motion {
        Motion {
            position: p,
            velocity_y: 0.0,
            airborne: false,
        }
    }
    #[test]
    fn real_pizza_floor_normals_select_the_finished_top_and_preserve_six_foot_clearance() {
        let source = concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/",
            "GLB (recommended)/Modular Structures/floor_ceiling_hr_1.glb"
        );
        let mesh = crate::gltf_load::load_glb(source).expect("retained source slab");
        let mut world = WalkSurfaces::default();
        let model = Mat4::from_translation(Vec3::new(-67.0, -0.37, 94.0));
        world.append(source, &mesh, model).unwrap();
        let at = model.transform_point3(Vec3::new(1.0, 0.0, 1.0));
        let top = world
            .floor_y(at, -0.2, 0.2)
            .expect("source top is walkable");
        assert!((top - 0.030001).abs() < 0.00002);
        assert_eq!(
            world.floor_y(at, -0.38, -0.35),
            None,
            "source slab underside must never support the feet"
        );
        let state = world.resolve(
            Vec3::new(at.x, -0.07, at.z),
            grounded(Vec3::new(at.x, -0.07, at.z)),
            -0.07,
            crate::foot::STANDING_HEIGHT,
        );
        assert!((state.position.y - top).abs() < 1e-6);
        let lintel = -0.37 + 2.5;
        assert!((lintel - top - 2.1).abs() < 0.00002);
        assert!(state.position.y + crate::foot::STANDING_HEIGHT < lintel);
    }
    #[test]
    fn real_slab_support_preserves_eye_stature_and_does_not_snap_to_upper_storey_or_underside() {
        let mut world = WalkSurfaces::default();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(-1.0, 1.0, -0.3, 0.1),
                Mat4::IDENTITY,
            )
            .unwrap();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(-1.0, 1.0, 2.7, 3.1),
                Mat4::IDENTITY,
            )
            .unwrap();
        let state = world.resolve(
            Vec3::ZERO,
            grounded(Vec3::ZERO),
            0.0,
            crate::foot::STANDING_HEIGHT,
        );
        assert!((state.position.y - 0.1).abs() < 1e-6);
        assert!((state.position.y + crate::foot::EYE - 1.8).abs() < 1e-6);
        assert_eq!(
            world.floor_y(Vec3::ZERO, 0.2, 2.9),
            None,
            "ceiling underside is not a floor"
        );
        let hop = world.resolve(
            Vec3::new(0.0, 0.7, 0.0),
            Motion {
                position: Vec3::new(0.0, 1.0, 0.0),
                velocity_y: 2.0,
                airborne: true,
            },
            0.0,
            crate::foot::STANDING_HEIGHT,
        );
        assert!(hop.airborne && hop.velocity_y == 0.0);
        assert!(hop.position.y + crate::foot::STANDING_HEIGHT < 2.7);
    }
    #[test]
    fn low_headroom_rejects_entry_and_ledge_departure_uses_gravity() {
        let mut world = WalkSurfaces::default();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(-1.0, 1.0, -0.1, 0.1),
                Mat4::IDENTITY,
            )
            .unwrap();
        world
            .append("roof_mh.glb", &slab(-1.0, 1.0, 1.8, 2.0), Mat4::IDENTITY)
            .unwrap();
        let old = Vec3::new(-1.4, 0.0, 0.0);
        let blocked = world.resolve(
            old,
            grounded(Vec3::new(-0.8, 0.0, 0.0)),
            0.0,
            crate::foot::STANDING_HEIGHT,
        );
        assert_eq!(blocked.position, old);
        let mut world = WalkSurfaces::default();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(-1.0, 1.0, 0.6, 1.0),
                Mat4::IDENTITY,
            )
            .unwrap();
        let fall = world.resolve(
            Vec3::new(0.99, 1.0, 0.0),
            grounded(Vec3::new(1.1, 0.0, 0.0)),
            0.0,
            crate::foot::STANDING_HEIGHT,
        );
        assert!(fall.airborne);
        assert_eq!(fall.position.y, 1.0);
        let landed = world.resolve(
            Vec3::new(0.0, 1.1, 0.0),
            Motion {
                position: Vec3::new(0.0, 0.95, 0.0),
                velocity_y: -2.0,
                airborne: true,
            },
            0.0,
            crate::foot::STANDING_HEIGHT,
        );
        assert!(!landed.airborne);
        assert_eq!(landed.position.y, 1.0);
    }
    #[test]
    fn holes_and_downward_terrain_descent_are_not_bridged_by_bounds() {
        let mut world = WalkSurfaces::default();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(-2.0, -0.4, 0.6, 1.0),
                Mat4::IDENTITY,
            )
            .unwrap();
        world
            .append(
                "floor_ceiling_hr_1.glb",
                &slab(0.4, 2.0, 0.6, 1.0),
                Mat4::IDENTITY,
            )
            .unwrap();
        assert_eq!(world.floor_y(Vec3::ZERO, -1.0, 1.1), None);
        let actual = grounded(Vec3::new(0.0, -0.08, 3.0));
        let descent = world.resolve(
            Vec3::new(0.0, 0.0, 3.05),
            actual,
            -0.08,
            crate::foot::STANDING_HEIGHT,
        );
        assert_eq!(descent.position, actual.position);
        assert!(!descent.airborne);
    }
}
