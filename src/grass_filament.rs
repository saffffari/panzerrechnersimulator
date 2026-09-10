//! Bounded grass mechanics: solid tapered stalks, damped bending, and persistent tread contact.
//! This is a coarse filament approximation, not a strand collision or fluid solver.
use std::collections::HashMap;

use glam::{Vec2, Vec3};
use crate::gltf_load::{CpuImage, CpuMesh, Vertex};

pub const HEIGHT: f32 = 0.48;
pub const TRACK_CELL: f32 = 0.25;
const TRACK_PAGE: i32 = 32; // 8 m × 8 m pages; one byte per 25 cm contact cell.
const TRACK_PAGE_CAPACITY: usize = 65_536; // At most 64 MiB payload; never evict old tracks.
const EMPTY: u8 = 255;

#[derive(Clone, Copy, Default)]
pub struct Contact {
    pub compression: f32,
    pub direction: Vec2,
}

#[derive(Default)]
pub struct TrackMemory {
    pages: HashMap<(i32, i32), Box<[u8; 1024]>>,
    cells: usize,
    pub revision: u64,
}

impl TrackMemory {
    fn address(x: i32, z: i32) -> ((i32, i32), usize) {
        ((x.div_euclid(TRACK_PAGE), z.div_euclid(TRACK_PAGE)),
            (z.rem_euclid(TRACK_PAGE) * TRACK_PAGE + x.rem_euclid(TRACK_PAGE)) as usize)
    }

    pub fn sample(&self, x: f32, z: f32) -> Contact {
        let (page, index) = Self::address((x / TRACK_CELL).floor() as i32, (z / TRACK_CELL).floor() as i32);
        let heading = self.pages.get(&page).map_or(EMPTY, |cells| cells[index]);
        if heading == EMPTY { return Contact::default(); }
        let angle = heading as f32 * std::f32::consts::TAU / 64.0;
        Contact { compression: 1.0, direction: Vec2::new(angle.sin(), angle.cos()) }
    }

    /// Two actual narrow tread footprints, not the deliberately widened low-resolution rut decal.
    /// Local forward is +Z, consistent with tank heading. A repeated pass preserves the first
    /// compression and updates direction only when a different heading genuinely crosses it.
    pub fn stamp(&mut self, position: Vec3, heading: f32) {
        let forward = Vec2::new(heading.sin(), heading.cos());
        let right = Vec2::new(heading.cos(), -heading.sin());
        let center = Vec2::new(position.x, position.z);
        let reach = 3.5;
        let encoded_heading = ((heading.rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::TAU * 64.0).round() as u32 % 64) as u8;
        let mut changed = false;
        for z in ((center.y - reach) / TRACK_CELL).floor() as i32..=((center.y + reach) / TRACK_CELL).ceil() as i32 {
            for x in ((center.x - reach) / TRACK_CELL).floor() as i32..=((center.x + reach) / TRACK_CELL).ceil() as i32 {
                let p = (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * TRACK_CELL - center;
                let across = p.dot(right).abs();
                let along = p.dot(forward).abs();
                if (across - 1.45).abs() > 0.40 || along > 1.85 { continue; }
                let (key, index) = Self::address(x, z);
                if !self.pages.contains_key(&key) && self.pages.len() >= TRACK_PAGE_CAPACITY { continue; }
                let page = self.pages.entry(key).or_insert_with(|| Box::new([EMPTY; 1024]));
                if page[index] != encoded_heading {
                    if page[index] == EMPTY { self.cells += 1; }
                    page[index] = encoded_heading;
                    changed = true;
                }
            }
        }
        if changed { self.revision = self.revision.wrapping_add(1); }
    }

    pub fn cell_count(&self) -> usize { self.cells }
}

#[derive(Clone, Copy, Default)]
pub struct Bend {
    pub displacement: Vec2,
    velocity: Vec2,
}

impl Bend {
    /// A critically damped spring integrates in bounded substeps, so a loading hitch cannot launch
    /// the tips. Displacement is in world metres and drives a root-anchored quadratic centerline.
    pub fn update(&mut self, target: Vec2, dt: f32) {
        let time = dt.clamp(0.0, 0.10);
        let steps = (time / 0.008).ceil().max(1.0) as u32;
        let h = time / steps as f32;
        for _ in 0..steps {
            self.velocity += ((target - self.displacement) * 64.0 - self.velocity * 16.0) * h;
            self.displacement += self.velocity * h;
        }
    }
}

/// A closed triangular stalk, 8 mm across, with three bending intervals and a pointed tip.
/// Every triangle is real geometry; the tiny palette texture has no alpha cutout or billboard.
pub fn stalk_mesh(winter: bool) -> CpuMesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let rings: Vec<[Vec3; 3]> = (0..3).map(|level| {
        let height = HEIGHT * level as f32 / 3.0;
        let radius = 0.004 * (1.0 - level as f32 * 0.24);
        std::array::from_fn(|corner| {
            let a = corner as f32 * std::f32::consts::TAU / 3.0;
            Vec3::new(a.cos() * radius, height, a.sin() * radius)
        })
    }).collect();
    let mut triangle = |a: Vec3, b: Vec3, c: Vec3| {
        let normal = (b-a).cross(c-a).normalize();
        let tangent = (b-a).normalize();
        let base = vertices.len() as u32;
        for p in [a,b,c] {
            vertices.push(Vertex { pos:p.to_array(), normal:normal.to_array(),
                uv:[0.5, 1.0-p.y/HEIGHT], tangent:[tangent.x,tangent.y,tangent.z,1.0] });
        }
        indices.extend([base,base+1,base+2]);
    };
    triangle(rings[0][0], rings[0][1], rings[0][2]); // bottom faces -Y
    for level in 0..2 {
        for corner in 0..3 {
            let next = (corner+1)%3;
            triangle(rings[level][corner], rings[level+1][corner], rings[level+1][next]);
            triangle(rings[level][corner], rings[level+1][next], rings[level][next]);
        }
    }
    for corner in 0..3 { triangle(rings[2][corner], Vec3::Y*HEIGHT, rings[2][(corner+1)%3]); }
    // Short green shoots retain their colour down to the root, so a dense stand reads as
    // vegetation instead of isolated black needles. This is albedo, never emitted light.
    let colors = if winter { [[121,111,75,255],[110,103,66,255],[85,79,51,255],[65,60,42,255]] }
        else { [[116,124,64,255],[85,111,49,255],[72,99,40,255],[63,86,36,255]] };
    let rgba = colors.into_iter().flatten().collect();
    // Conservative bounds include the maximum world wind deflection at ordinary blade scale.
    CpuMesh::basic(vertices,indices,Some(CpuImage::new(1,4,rgba)),
        Vec3::new(-0.24,0.0,-0.24),Vec3::new(0.24,HEIGHT,0.24))
}

/// Solid clusters, never crossed cards or alpha-tested noise. Close roots carry three fine
/// blades; middle roots carry five wider blades; far roots summarize four low leafy masses.
/// The coarse mesh intentionally has broad continuous coverage instead of isolated bright tips.
pub fn tuft_mesh(layer: usize, winter: bool) -> CpuMesh {
    let blades = [3, 5, 4][layer];
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    let mut triangle = |a: Vec3, b: Vec3, c: Vec3| {
        let normal = (b - a).cross(c - a).normalize();
        let tangent = (b - a).normalize();
        let base = vertices.len() as u32;
        for p in [a, b, c] {
            lo = lo.min(p);
            hi = hi.max(p);
            vertices.push(Vertex {
                pos: p.to_array(),
                normal: normal.to_array(),
                uv: [0.5, 1.0 - p.y / HEIGHT],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        indices.extend([base, base + 1, base + 2]);
    };
    for i in 0..blades {
        let angle = i as f32 * std::f32::consts::TAU / blades as f32 + 0.31;
        let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
        let root = direction * [0.065, 0.19, 0.42][layer];
        let radius = [0.010, 0.055, 0.32][layer] * (0.88 + 0.06 * i as f32);
        let height = HEIGHT * (1.0 - 0.11 * i as f32) * [1.0, 1.0, 0.70][layer];
        let base: [Vec3; 3] = std::array::from_fn(|corner| {
            let a = angle + corner as f32 * std::f32::consts::TAU / 3.0;
            root + Vec3::new(a.cos() * radius, 0.0, a.sin() * radius)
        });
        let tip = root + Vec3::Y * height + direction * [0.018, 0.09, 0.11][layer];
        triangle(base[0], base[1], base[2]);
        for corner in 0..3 {
            triangle(base[corner], tip, base[(corner + 1) % 3]);
        }
    }
    let colors = if winter {
        [
            [121, 111, 75, 255],
            [110, 103, 66, 255],
            [85, 79, 51, 255],
            [65, 60, 42, 255],
        ]
    } else if layer == 2 {
        [
            [89, 109, 52, 255],
            [82, 102, 46, 255],
            [72, 94, 40, 255],
            [65, 87, 37, 255],
        ]
    } else {
        [
            [111, 124, 62, 255],
            [88, 113, 48, 255],
            [74, 100, 40, 255],
            [65, 88, 36, 255],
        ]
    };
    let rgba = colors.into_iter().flatten().collect();
    let mut mesh = CpuMesh::basic(
        vertices,
        indices,
        Some(CpuImage::new(1, 4, rgba)),
        lo - Vec3::new(0.24, 0.0, 0.24),
        hi + Vec3::new(0.24, 0.0, 0.24),
    );
    mesh.roughness_factor = 0.98;
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_layers_are_closed_opaque_clusters_with_bounded_geometry() {
        for layer in 0..3 {
            let mesh = tuft_mesh(layer, false);
            assert_eq!(mesh.indices.len() / 3, [12, 20, 16][layer]);
            assert!(mesh
                .texture
                .as_ref()
                .unwrap()
                .rgba
                .chunks_exact(4)
                .all(|p| p[3] == 255));
            assert!(mesh.roughness_factor >= 0.95 && mesh.metallic_factor == 0.0);
            let mut edges = HashMap::new();
            let key = |v: Vec3| {
                [
                    (v.x * 1e6).round() as i32,
                    (v.y * 1e6).round() as i32,
                    (v.z * 1e6).round() as i32,
                ]
            };
            for tri in mesh.indices.chunks_exact(3) {
                let p: [Vec3; 3] =
                    std::array::from_fn(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
                assert!((p[1] - p[0]).cross(p[2] - p[0]).length() > 1e-8);
                for (a, b) in [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])] {
                    let (a, b) = (key(a), key(b));
                    *edges
                        .entry(if a < b { (a, b) } else { (b, a) })
                        .or_insert(0) += 1;
                }
            }
            assert!(edges.values().all(|&n| n == 2));
            // The footprint mask must enclose every yaw and instance width variation.
            assert!(mesh
                .vertices
                .iter()
                .all(|v| Vec2::new(v.pos[0], v.pos[2]).length() * 1.1
                    < super::super::FOOTPRINT[layer]));
        }
    }

    #[test]
    fn tread_memory_preserves_root_contacts_and_clear_track_gap() {
        let mut memory = TrackMemory::default();
        memory.stamp(Vec3::ZERO,0.0);
        assert_eq!(memory.sample(1.5,0.0).compression,1.0);
        assert_eq!(memory.sample(-1.5,0.0).compression,1.0);
        assert_eq!(memory.sample(0.0,0.0).compression,0.0);
        assert_eq!(memory.sample(2.5,0.0).compression,0.0);
        let revision = memory.revision;
        memory.stamp(Vec3::ZERO,0.0);
        assert_eq!(revision,memory.revision);
        memory.stamp(Vec3::new(150.0,0.0,150.0),1.0);
        assert_eq!(memory.sample(1.5,0.0).compression,1.0);
        memory.stamp(Vec3::ZERO,std::f32::consts::FRAC_PI_2);
        assert!(memory.sample(0.0,1.5).direction.x>0.99);
    }
    #[test]
    fn bend_is_damped_and_frame_rate_stable() {
        let mut a=Bend::default(); let mut b=Bend::default();
        for _ in 0..120 { a.update(Vec2::new(0.12,-0.07),1.0/60.0); }
        for _ in 0..60 { b.update(Vec2::new(0.12,-0.07),1.0/30.0); }
        assert!((a.displacement-b.displacement).length()<0.001);
        assert!((a.displacement-Vec2::new(0.12,-0.07)).length()<0.001);
        a.update(Vec2::ZERO,20.0);
        assert!(a.displacement.is_finite() && a.displacement.length()<0.15);
    }
    #[test]
    fn stalk_is_closed_solid_and_not_a_card() {
        let mesh=stalk_mesh(false);
        assert_eq!(mesh.indices.len()/3,16);
        assert!(mesh.vertices.iter().all(|v| Vec2::new(v.pos[0],v.pos[2]).length()<=0.00401));
        let mut edges=HashMap::new();
        let key=|v:Vec3| [(v.x*1e6).round() as i32,(v.y*1e6).round() as i32,(v.z*1e6).round() as i32];
        for tri in mesh.indices.chunks_exact(3) {
            let p: [Vec3; 3]=std::array::from_fn(|i| Vec3::from(mesh.vertices[tri[i] as usize].pos));
            for (a,b) in [(p[0],p[1]),(p[1],p[2]),(p[2],p[0])] {
                let (a,b)=(key(a),key(b));
                *edges.entry(if a<b {(a,b)} else {(b,a)}).or_insert(0)+=1;
            }
            assert!((p[1]-p[0]).cross(p[2]-p[0]).length()>1e-8);
        }
        assert!(edges.values().all(|&uses| uses==2));
        assert!(mesh.texture.unwrap().rgba.chunks_exact(4).all(|pixel|pixel[3]==255));
    }
}

#[cfg(test)]
mod shader_contract_tests {
    #[test]
    fn both_render_tiers_bend_the_same_geometry_and_correct_its_normal() {
        let block=|source: &'static str| {
            source.split("// SOLID_GRASS_BEGIN").nth(1).unwrap()
                .split("// SOLID_GRASS_END").next().unwrap()
        };
        let forward=include_str!("../shaders/mesh.wgsl");
        let deferred=include_str!("../shaders/gbuf.wgsl");
        assert_eq!(block(forward),block(deferred));
        assert!(block(forward).contains("p += local_tip * u * u"));
        assert!(block(forward).contains("n.y -= dot(derivative, n)"));
    }
}
