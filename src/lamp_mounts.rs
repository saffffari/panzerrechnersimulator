//! Where the cabin's lamps are fixed (2026-10-28; owner feedback: light fixtures "hang in mid-air,
//! not attached to any surface"). Each lamp is a uniformly lit cube on a squat plinth, and the
//! plinth stands on a flat face of the Tiger's own source mesh. The faces were found by a
//! nearest-surface survey of the source GLBs (`closest_surface`, `flat_patch`) from each lamp's
//! former floating position and are fixed here; the tests check that every footprint still lies on
//! one flat face of the source.
//!
//! Moves from the floating positions: the dome lamp drops one plinth height below the fan's
//! underside; the annunciators, whose row floated before the gun's breech, go into a column on the
//! turret wall at the loader's side (the front wall there is a curved ring with lips and no room);
//! the gunner's sight lamps go 21 cm onto the turret wall beside the TPZ; the commander's 17 cm
//! onto two facets of the cupola ring beside his front vision block (the space under the block is
//! open; the block's own sill lies behind its glass); the breech lamps 1-2 cm onto the breech
//! guard's side plates; the driver's ignition lamp, which had no visible lens, onto his panel's
//! face.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec3};

/// The frame a lamp rides: the hull, the turret, or the recoiling gun (`visual_gun_model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountFrame { Hull, Turret, Gun }

/// A point on a flat face of the frame's source mesh and the face's outward normal.
#[derive(Clone, Copy, Debug)]
pub struct Mount {
    pub frame: MountFrame,
    pub surface: Vec3,
    pub normal: Vec3,
}

/// The plinth's footprint half side and its height, against the lamp's half size: a squat block a
/// little wider than the lamp and under a third of its height.
pub const PLINTH_FOOTPRINT: f32 = 1.2;
pub const PLINTH_HEIGHT: f32 = 0.6;
/// The bracket's colour: dark unpainted steel.
pub const PLINTH_SRGB: [u8; 3] = [62, 62, 58];

impl Mount {
    /// The face's horizontal tangent (any tangent on a level face), its normal, and their cross.
    fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let n = self.normal.normalize();
        let t = Vec3::Y.cross(n);
        let t = if t.length_squared() > 1e-6 { t.normalize() } else { Vec3::X };
        (t, n, t.cross(n))
    }
    fn placed(&self, along: f32, lift: f32) -> Mat4 {
        let (t, n, c) = self.basis();
        Mat4::from_cols(t.extend(0.0), n.extend(0.0), c.extend(0.0),
            (self.surface + t * along + n * lift).extend(1.0))
    }
    /// The plinth's model in its frame (`plinth_mesh` stands on its own origin), `along` metres
    /// along the face's horizontal.
    pub fn plinth(&self, along: f32) -> Mat4 { self.placed(along, 0.0) }
    /// The lamp cube's model in its frame: centred on top of its plinth.
    pub fn lamp(&self, half: f32, along: f32) -> Mat4 { self.placed(along, half * (PLINTH_HEIGHT + 1.0)) }
}

/// The plinth for a lamp of half size `half`, standing on its origin with +y the face's normal.
pub fn plinth_mesh(half: f32) -> CpuMesh {
    let side = half * PLINTH_FOOTPRINT;
    crate::prims::box_mesh(Vec3::new(-side, 0.0, -side), Vec3::new(side, half * PLINTH_HEIGHT, side), PLINTH_SRGB)
}

/// The dome lamp: on the turret fan's underside (`interior_rig::TURRET_FAN_UNDERSIDE_Y`).
pub const DOME: Mount = Mount { frame: MountFrame::Turret,
    surface: Vec3::new(-0.01264894, crate::interior_rig::TURRET_FAN_UNDERSIDE_Y, -0.00222471),
    normal: Vec3::new(0.0, -1.0, 0.0) };

/// The annunciators (fire, breech, repair, station), a column 3 cm apart on the turret wall at the
/// loader's side, fire at the top; each with its offset along the wall's horizontal.
pub const ANNUNCIATORS: [(Mount, f32); 4] = {
    const fn at(up: f32) -> (Mount, f32) {
        (Mount { frame: MountFrame::Turret, surface: Vec3::new(-0.9051, 1.7873 + up, 0.4589),
            normal: Vec3::new(0.9051, 0.0, -0.4253) }, 0.04)
    }
    [at(0.0), at(-0.03), at(-0.06), at(-0.09)]
};

/// The breech lamps, one on each side plate of the breech guard (gun frame, recoiling).
pub const BREECH: [Mount; 2] = [
    Mount { frame: MountFrame::Gun, surface: Vec3::new(0.1804, 0.12, -0.92), normal: Vec3::new(1.0, 0.0, 0.0) },
    Mount { frame: MountFrame::Gun, surface: Vec3::new(-0.1893, 0.12, -0.92), normal: Vec3::new(-1.0, 0.0, 0.0) },
];

/// The sight's loading and round lamps, per station ([loading, round], each with its offset along
/// the face): the gunner's on the turret wall beside the TPZ, left of the sight's field; the
/// commander's on two facets of the cupola ring beside his front vision block.
pub const SIGHT: [[(Mount, f32); 2]; 2] = {
    const GUNNER: Mount = Mount { frame: MountFrame::Turret,
        surface: Vec3::new(0.8843, 1.7873, 0.4972), normal: Vec3::new(-0.8708, 0.0, -0.4917) };
    const COMMANDER_LOAD: Mount = Mount { frame: MountFrame::Turret,
        surface: Vec3::new(0.4740, 2.3786, -0.0755), normal: Vec3::new(0.2358, 0.0, -0.9718) };
    const COMMANDER_ROUND: Mount = Mount { frame: MountFrame::Turret,
        surface: Vec3::new(0.4521, 2.3786, -0.0820), normal: Vec3::new(0.3271, 0.0, -0.9450) };
    [[(GUNNER, 0.01), (GUNNER, -0.02)], [(COMMANDER_LOAD, 0.0), (COMMANDER_ROUND, 0.0)]]
};

/// The driver's ignition lamp, on the face of his instrument panel.
pub const IGNITION: Mount = Mount { frame: MountFrame::Hull,
    surface: Vec3::new(1.152, 1.724, 2.102), normal: Vec3::new(-0.759, -0.051, -0.649) };

/// The closest point on the mesh's triangles to `p`, with the face normal turned toward `p`.
pub fn closest_surface(mesh: &CpuMesh, p: Vec3) -> (Vec3, Vec3, f32) {
    let mut best = (Vec3::ZERO, Vec3::Y, f32::INFINITY);
    for tri in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(mesh.vertices[tri[k] as usize].pos));
        let q = closest_on_triangle(p, a, b, c);
        let d = q.distance(p);
        if d < best.2 {
            let mut n = (b - a).cross(c - a).normalize_or_zero();
            if n.dot(p - q) < 0.0 { n = -n; }
            best = (q, n, d);
        }
    }
    best
}

/// Whether a square footprint of half side `half` about `surface`, in the plane of `normal`, lies
/// on one flat face: each of a 5 x 5 grid of points 2 mm off the plane finds the surface 2 mm away
/// with the same normal.
pub fn flat_patch(mesh: &CpuMesh, surface: Vec3, normal: Vec3, half: f32) -> bool {
    let n = normal.normalize();
    let t = Vec3::Y.cross(n);
    let t = if t.length_squared() > 1e-6 { t.normalize() } else { Vec3::X };
    let u = t.cross(n);
    (-2..=2).all(|i| (-2..=2).all(|j| {
        let p = surface + t * (i as f32 * half * 0.5) + u * (j as f32 * half * 0.5) + n * 0.002;
        let (_, face, d) = closest_surface(mesh, p);
        (d - 0.002).abs() < 0.0008 && face.dot(n) > 0.98
    }))
}

/// Ericson, Real-Time Collision Detection, 5.1.5.
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 { return a; }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 { return b; }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 { return a + ab * (d1 / (d1 - d3)); }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 { return c; }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 { return a + ac * (d2 / (d2 - d6)); }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interior_rig as rig;

    #[test]
    fn every_cabin_lamp_stands_on_a_plinth_on_one_flat_face_of_its_source_mesh() {
        let source = |frame| match frame {
            MountFrame::Hull => "assets/tanks/tiger/baked/tiger_hull.glb",
            MountFrame::Turret => "assets/tanks/tiger/baked/tiger_turret.glb",
            MountFrame::Gun => "assets/tanks/tiger/baked/tiger_gun.glb",
        };
        if !crate::resource::exists(source(MountFrame::Turret)) { return; }
        let meshes = [MountFrame::Hull, MountFrame::Turret, MountFrame::Gun]
            .map(|frame| (frame, crate::gltf_load::load_glb(source(frame)).unwrap()));
        let mesh = |frame| &meshes.iter().find(|(f, _)| *f == frame).unwrap().1;
        let mut lamps = vec![("dome", DOME, 0.0, rig::DOME_FIXTURE.half_size_m),
            ("ignition", IGNITION, 0.0, rig::IGNITION_FIXTURE.half_size_m)];
        for (i, &(mount, along)) in ANNUNCIATORS.iter().enumerate() {
            lamps.push((["fire", "breech", "repair", "station"][i], mount, along, rig::ANNUNCIATOR_FIXTURES[i].half_size_m));
        }
        for mount in BREECH { lamps.push(("breech lamp", mount, 0.0, rig::BREECH_FIXTURES[0].half_size_m)); }
        for (s, station) in SIGHT.iter().enumerate() {
            let [load, round] = [["gunner's loading", "commander's loading"], ["gunner's round", "commander's round"]].map(|n| n[s]);
            lamps.push((load, station[0].0, station[0].1, rig::SIGHT_LOAD_FIXTURES[0].half_size_m));
            lamps.push((round, station[1].0, station[1].1, rig::SIGHT_ROUND_FIXTURES[0].half_size_m));
        }
        for (name, mount, along, half) in lamps {
            let source = mesh(mount.frame);
            let foot = mount.plinth(along).transform_point3(Vec3::ZERO);
            let (_, _, d) = closest_surface(source, foot + mount.normal.normalize() * 0.002);
            assert!((d - 0.002).abs() < 0.001, "{name}: plinth {:.1} mm off its face", (d - 0.002) * 1000.0);
            assert!(flat_patch(source, foot, mount.normal, half * PLINTH_FOOTPRINT),
                "{name}: plinth footprint leaves its flat face");
            // The cube sits on the plinth: its centre one plinth height plus its half size out.
            let centre = mount.lamp(half, along).transform_point3(Vec3::ZERO);
            assert!((centre.distance(foot) - half * (PLINTH_HEIGHT + 1.0)).abs() < 1e-5);
            // Nothing of the source mesh inside the lamp.
            let (_, _, clear) = closest_surface(source, centre);
            assert!(clear > half * 0.99, "{name}: the source mesh reaches into the lamp ({:.1} mm)", clear * 1000.0);
        }
    }

    #[test]
    fn paired_lamps_sit_side_by_side_without_overlap() {
        for pair in ANNUNCIATORS.windows(2) {
            let a = pair[0].0.plinth(pair[0].1).transform_point3(Vec3::ZERO);
            let b = pair[1].0.plinth(pair[1].1).transform_point3(Vec3::ZERO);
            assert!(a.distance(b) > 2.0 * 0.012 * PLINTH_FOOTPRINT, "{a} {b}");
        }
        for station in SIGHT {
            let a = station[0].0.plinth(station[0].1).transform_point3(Vec3::ZERO);
            let b = station[1].0.plinth(station[1].1).transform_point3(Vec3::ZERO);
            assert!(a.distance(b) > 2.0 * 0.005 * PLINTH_FOOTPRINT, "{a} {b}");
        }
    }
}


