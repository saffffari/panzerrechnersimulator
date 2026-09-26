//! Conservative source-surface collision for static authored tank props only.
//! Half-metre occupied cells preserve the empty space around a long gun and inside an open hull;
//! they are not calibrated armor plates and never replace the playable vehicle's damage mesh.
use crate::gltf_load::CpuMesh;
use glam::{IVec3, Mat4, Vec3};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, OnceLock};

type Bounds = (Vec3, Vec3);
type Key = (String, usize, usize, [u32; 6]);
static CACHE: OnceLock<Mutex<HashMap<Key, Vec<Bounds>>>> = OnceLock::new();

pub(super) fn accepts(path: &str) -> bool {
    super::RETIRED_HALF_TIGER_GLBS.contains(&path)
        || super::CURRENT_TIGER_PART_GLBS.contains(&path)
        || super::RETIRED_WRECK_GLBS.iter().any(|(p, _)| *p == path)
        || (path.starts_with("assets/authoring/battlefield/")
            && (path.ends_with("_wreck_burnt.glb") || path.ends_with("_wreck_detail.glb")
                || path.rsplit('/').next().is_some_and(|name| matches!(name,
                    "tiger_wreck_20260925_hull.glb" | "tiger_wreck_20260925_turret.glb"
                    | "tiger_wreck_20260925_gun.glb" | "tiger_wreck_20260925_turret_detached.glb"
                    | "tiger_wreck_20260925_gun_detached.glb"))))
}

fn overlaps(triangle: [Vec3; 3], min: Vec3, max: Vec3) -> bool {
    let half = (max - min) * 0.5;
    let center = (min + max) * 0.5;
    let p = triangle.map(|v| v - center);
    let edges = [p[1] - p[0], p[2] - p[1], p[0] - p[2]];
    let separated = |axis: Vec3| {
        let a = p[0].dot(axis);
        let b = p[1].dot(axis);
        let c = p[2].dot(axis);
        let radius = half.dot(axis.abs()) + 1e-6;
        a.min(b).min(c) > radius || a.max(b).max(c) < -radius
    };
    // Bounds already overlap by construction; these are the remaining separating axes.
    !separated(edges[0].cross(edges[1]))
        && edges.iter().all(|edge| [Vec3::X, Vec3::Y, Vec3::Z]
            .into_iter().all(|axis| !separated(edge.cross(axis))))
}

fn local_proxies(mesh: &CpuMesh) -> Vec<Bounds> {
    // Source tanks are <12m; adapt only for an unusually large authored variant to retain a
    // bounded grid. Scanning source triangles is startup work cached once per asset geometry.
    let cell = 0.5_f32.max((mesh.max - mesh.min).max_element() / 48.0);
    let mut occupied = BTreeSet::new(); // (y,z,x): deterministic contiguous X runs
    for triangle in mesh.indices.chunks_exact(3) {
        let p: [Vec3; 3] = std::array::from_fn(|i|
            Vec3::from(mesh.vertices[triangle[i] as usize].pos));
        let lo = p[0].min(p[1]).min(p[2]);
        let hi = p[0].max(p[1]).max(p[2]);
        let a = (lo / cell).floor().as_ivec3();
        let b = (hi / cell).floor().as_ivec3();
        for y in a.y..=b.y { for z in a.z..=b.z { for x in a.x..=b.x {
            let key = (y, z, x);
            if occupied.contains(&key) { continue; }
            let min = IVec3::new(x, y, z).as_vec3() * cell;
            if overlaps(p, min, min + Vec3::splat(cell)) { occupied.insert(key); }
        }}}
    }
    let mut out = Vec::new();
    let mut cells = occupied.into_iter().peekable();
    while let Some((y, z, first)) = cells.next() {
        let mut last = first;
        while cells.peek() == Some(&(y, z, last + 1)) { cells.next(); last += 1; }
        let min = (IVec3::new(first, y, z).as_vec3() * cell).max(mesh.min);
        let max = (IVec3::new(last + 1, y + 1, z + 1).as_vec3() * cell).min(mesh.max);
        // Thin authored plates must still be hittable by a swept foot or shell.
        let center = (min + max) * 0.5;
        let half = ((max - min) * 0.5).max(Vec3::splat(0.01));
        out.push((center - half, center + half));
    }
    out
}

pub(super) fn proxies(path: &str, mesh: &CpuMesh, model: Mat4) -> Vec<Bounds> {
    let bounds = [mesh.min.x, mesh.min.y, mesh.min.z, mesh.max.x, mesh.max.y, mesh.max.z];
    let key = (path.to_owned(), mesh.vertices.len(), mesh.indices.len(), bounds.map(f32::to_bits));
    let local = {
        let mut cache = CACHE.get_or_init(Mutex::default).lock().expect("wreck collision cache");
        cache.entry(key).or_insert_with(|| local_proxies(mesh)).clone()
    };
    super::proxy_boxes(model, local)
}

/// The three runtime spare Tigers reuse the same cached source cells. Only their small world
/// bound lists move when a spare is taken and its visible mesh becomes the previous vehicle's hulk.
pub(crate) struct StaticTankCollider {
    local: [Vec<Bounds>; 2],
    bounds: Vec<Bounds>,
    center: Vec3,
}
impl StaticTankCollider {
    pub(crate) fn new(hull: &CpuMesh, turret: &CpuMesh, models: [Mat4; 2]) -> Self {
        let local = [
            proxies(super::CURRENT_TIGER_PART_GLBS[0], hull, Mat4::IDENTITY),
            proxies(super::CURRENT_TIGER_PART_GLBS[1], turret, Mat4::IDENTITY),
        ];
        let mut result = Self { local, bounds: Vec::new(), center: Vec3::ZERO };
        result.set_pose(models);
        result
    }
    pub(crate) fn set_pose(&mut self, models: [Mat4; 2]) {
        self.center = models[0].transform_point3(Vec3::ZERO);
        self.bounds.clear();
        for (local, model) in self.local.iter().zip(models) {
            self.bounds.extend(super::proxy_boxes(model, local.iter().copied()));
        }
    }
    pub(crate) fn resolve_walk(&self, old: Vec3, attempted: Vec3) -> Vec3 {
        if old.distance_squared(self.center) > 196.0
            && attempted.distance_squared(self.center) > 196.0 { return attempted; }
        crate::building_runtime::resolve_walk_xz_against_bounds(old, attempted, &self.bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contains(boxes: &[Bounds], p: Vec3) -> bool {
        boxes.iter().any(|(a,b)| p.cmpge(*a).all() && p.cmple(*b).all())
    }
    #[test]
    fn source_surface_proxies_keep_open_hull_and_space_beside_long_barrel_clear() {
        let source = super::super::merge_solid_meshes([
            crate::prims::box_mesh(Vec3::new(-2.0,0.0,-3.0),Vec3::new(-1.8,2.0,3.0),[40;3]),
            crate::prims::box_mesh(Vec3::new(1.8,0.0,-3.0),Vec3::new(2.0,2.0,3.0),[40;3]),
            crate::prims::box_mesh(Vec3::new(-2.0,0.0,-3.0),Vec3::new(2.0,0.1,3.0),[40;3]),
            crate::prims::box_mesh(Vec3::new(-0.08,1.8,3.0),Vec3::new(0.08,1.96,8.0),[40;3]),
        ], [40;3]);
        let boxes=local_proxies(&source);
        assert!(contains(&boxes,Vec3::new(-1.9,1.0,0.0)));
        assert!(contains(&boxes,Vec3::new(0.0,1.9,6.0)));
        assert!(!contains(&boxes,Vec3::new(1.0,1.0,6.0)), "no gun-length invisible wall");
        assert!(!contains(&boxes,Vec3::new(0.0,1.0,0.0)), "open wreck cavity is not a solid box");
        assert!(boxes.len()<200);
    }
    #[test]
    fn only_explicit_static_tank_asset_families_are_selected() {
        assert!(accepts("assets/tanks/tiger/baked/half/tiger_hull.glb"));
        assert!(accepts("assets/authoring/battlefield/pz3_wreck_burnt.glb"));
        assert!(!accepts("assets/tanks/tiger/baked/tiger_part_hatchlever.glb"));
        assert!(!accepts("assets/authoring/battlefield/scorched_earth.glb"));
        assert!(accepts("assets/authoring/battlefield/tiger_wreck_20260925_turret_detached.glb"));
        assert!(!accepts("assets/authoring/battlefield/tiger_wreck_20260925_crew_a.glb"));
    }
    #[test]
    fn actual_tiger_static_hull_and_gun_have_bounded_transformed_collision() {
        for part in ["hull", "gun"] {
            let path = format!("assets/tanks/tiger/baked/half/tiger_{part}.glb");
            let source = super::super::load_module(&path).unwrap();
            let pose=Mat4::from_translation(Vec3::new(30.0,4.0,-12.0))*Mat4::from_rotation_y(0.7);
            let local=proxies(&path,&source,Mat4::IDENTITY);
            let moved=proxies(&path,&source,pose);
            assert!(!local.is_empty() && local.len()<300, "{part}: {} proxies",local.len());
            assert_eq!(local.len(),moved.len());
            for (a,b) in local {
                assert!(contains(&moved,pose.transform_point3((a+b)*0.5)));
            }
            eprintln!("static {part} source triangles={} proxies={}",source.indices.len()/3,moved.len());
        }
    }
    #[test]
    fn spare_collision_moves_to_the_abandoned_hulk_without_leaving_a_ghost() {
        let hull=crate::prims::box_mesh(Vec3::new(-1.5,0.0,-3.0),Vec3::new(1.5,1.8,3.0),[40;3]);
        let turret=crate::prims::box_mesh(Vec3::new(-1.0,1.8,-1.0),Vec3::new(1.0,2.6,1.0),[40;3]);
        let mut collider=StaticTankCollider::new(&hull,&turret,[Mat4::IDENTITY;2]);
        let old=Vec3::new(-2.0,0.0,0.0);let attempt=Vec3::new(-1.2,0.0,0.0);
        assert!(collider.resolve_walk(old,attempt).x < -1.5);
        let shift=Vec3::new(25.0,2.0,-30.0);
        collider.set_pose([Mat4::from_translation(shift);2]);
        assert_eq!(collider.resolve_walk(old,attempt),attempt);
        assert!(collider.resolve_walk(old+shift,attempt+shift).x < shift.x-1.5);
    }
}
