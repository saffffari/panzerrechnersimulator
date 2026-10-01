//! Persistent snow/soil compaction at a fixed 12.5 cm sample spacing. Footprints come from the
//! drawn hull's lowest 5 cm, rather than enlarging a decal to hide a coarse world texture.
use crate::gltf_load::CpuMesh;
use glam::{Mat4, Vec2, Vec3};
use std::collections::HashMap;

pub const CELL_M: f32 = 0.125;
pub const PAGE_RES: u32 = 512;
pub const PAGE_M: f32 = CELL_M * PAGE_RES as f32;
pub const ATLAS_PAGES: u32 = 16;
pub const ATLAS_RES: u32 = PAGE_RES * ATLAS_PAGES;
pub const LOOKUP_RES: u32 = 256;

#[derive(Clone, Copy, Debug)]
pub struct Footprint {
    /// Hull-local (x,z) bounding rectangles of the low left and right tread surfaces.
    pub bands: [(Vec2, Vec2); 2],
}

impl Footprint {
    pub fn from_hull(mesh: &CpuMesh) -> Option<Self> {
        let floor = mesh
            .vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::INFINITY, f32::min);
        let mut bands = [(Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)); 2];
        for v in mesh.vertices.iter().filter(|v| v.pos[1] <= floor + 0.05) {
            let p = Vec2::new(v.pos[0], v.pos[2]);
            let side = usize::from(p.x >= 0.0);
            bands[side].0 = bands[side].0.min(p);
            bands[side].1 = bands[side].1.max(p);
        }
        // Missing/placeholder hulls have no pair of lower tread bands; do not invent one.
        if bands
            .iter()
            .any(|(lo, hi)| !lo.is_finite() || hi.x - lo.x < 0.1 || hi.y - lo.y < 0.5)
            || bands[0].1.x >= -0.25
            || bands[1].0.x <= 0.25
        {
            return None;
        }
        Some(Self { bands })
    }

    pub fn quads_for_model(self, hull_model: Mat4) -> [[Vec2; 4]; 2] {
        self.bands.map(|(lo, hi)| {
            [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)].map(|p| {
                let world = hull_model.transform_point3(Vec3::new(p.x, 0.0, p.y));
                Vec2::new(world.x, world.z)
            })
        })
    }

    #[cfg(test)]
    fn quads(self, position: Vec3, heading: f32) -> [[Vec2; 4]; 2] {
        self.quads_for_model(Mat4::from_translation(position) * Mat4::from_rotation_y(heading))
    }
}

/// Pages never evict older tracks. At capacity, untouched pages remain uncompressed. This bounded
/// material history is separate from collision and contains no geometric terrain displacement.
#[derive(Default)]
pub struct Atlas {
    pages: HashMap<(i32, i32), u32>,
    pub new_pages: Vec<((u32, u32), u32)>,
}

impl Atlas {
    fn page(&mut self, key: (i32, i32)) -> Option<u32> {
        let bias = LOOKUP_RES as i32 / 2;
        if key.0 < -bias || key.1 < -bias || key.0 >= bias || key.1 >= bias {
            return None;
        }
        if let Some(page) = self.pages.get(&key) {
            return Some(*page);
        }
        let page = self.pages.len() as u32;
        if page >= ATLAS_PAGES * ATLAS_PAGES {
            return None;
        }
        self.pages.insert(key, page);
        self.new_pages
            .push((((key.0 + bias) as u32, (key.1 + bias) as u32), page + 1));
        Some(page)
    }

    pub fn stamp(&mut self, quad: [Vec2; 4], strength: f32, verts: &mut Vec<[f32; 5]>) {
        let lo = quad
            .iter()
            .copied()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min);
        let hi = quad
            .iter()
            .copied()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
        for z in (lo.y / PAGE_M).floor() as i32..=(hi.y / PAGE_M).floor() as i32 {
            for x in (lo.x / PAGE_M).floor() as i32..=(hi.x / PAGE_M).floor() as i32 {
                let origin = Vec2::new(x as f32, z as f32) * PAGE_M;
                let polygon = clip_to_page(&quad, origin);
                if polygon.len() < 3 {
                    continue;
                }
                let Some(page) = self.page((x, z)) else {
                    continue;
                };
                let atlas_origin =
                    Vec2::new((page % ATLAS_PAGES) as f32, (page / ATLAS_PAGES) as f32);
                let uv = |p: Vec2| ((p - origin) / PAGE_M + atlas_origin) / ATLAS_PAGES as f32;
                for i in 1..polygon.len() - 1 {
                    for p in [polygon[0], polygon[i], polygon[i + 1]] {
                        let p = uv(p);
                        verts.push([p.x, p.y, 0.0, 0.0, strength]);
                    }
                }
            }
        }
    }
}

fn clip_to_page(quad: &[Vec2; 4], origin: Vec2) -> Vec<Vec2> {
    let mut polygon = quad.to_vec();
    for (axis, bound, sign) in [
        (0, origin.x, 1.0),
        (0, origin.x + PAGE_M, -1.0),
        (1, origin.y, 1.0),
        (1, origin.y + PAGE_M, -1.0),
    ] {
        let input = std::mem::take(&mut polygon);
        if input.is_empty() {
            break;
        }
        let mut previous = *input.last().unwrap();
        for current in input {
            let a = (previous[axis] - bound) * sign;
            let b = (current[axis] - bound) * sign;
            if (a >= 0.0) != (b >= 0.0) {
                polygon.push(previous.lerp(current, a / (a - b)));
            }
            if b >= 0.0 {
                polygon.push(current);
            }
            previous = current;
        }
    }
    polygon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "GPU source-width snow compaction; run with KESSEL_RENDERER=shader KESSEL_NO_RT=1"]
    fn gpu_rut_pixels_stay_inside_source_tread_bands() -> anyhow::Result<()> {
        use wgpu::util::DeviceExt;
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let mut renderer = super::super::Renderer::new(&device, &queue, &[]);
        renderer.track_half = 2600.0; // Largest campaign, same cell size as the arena.
        let mut footprints = Vec::new();
        for (path, parts, pos, yaw) in [
            (
                "assets/tanks/tiger/baked/tiger_hull.glb",
                crate::tank::TIGER_PARTS,
                Vec3::new(16.0, 0.0, 16.0),
                0.0,
            ),
            (
                "assets/tanks/T34-76/baked/t34_hull.glb",
                crate::tank::T34_PARTS,
                Vec3::new(40.0, 0.0, 40.0),
                0.73,
            ),
        ] {
            let hull = crate::gltf_load::load_glb(path)?;
            footprints.push((Footprint::from_hull(&hull).unwrap(), pos, yaw));
            renderer.register_track_footprint(parts, &hull);
            for _ in 0..8 {
                renderer.stamp_tank_tracks(
                    Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw),
                    parts,
                    0.28,
                );
            }
        }
        assert_eq!(renderer.track_atlas.pages.get(&(0, 0)), Some(&0));
        let readback = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("track-width-readback"),
            contents: &vec![0u8; (PAGE_RES * PAGE_RES) as usize],
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.flush_track_stamps(&device, &mut encoder);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: renderer.track_view.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(PAGE_RES),
                    rows_per_image: Some(PAGE_RES),
                },
            },
            wgpu::Extent3d {
                width: PAGE_RES,
                height: PAGE_RES,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = readback.slice(..).get_mapped_range();
        let mut touched = 0;
        for z in 0..PAGE_RES {
            for x in 0..PAGE_RES {
                let world = (Vec2::new(x as f32, z as f32) + Vec2::splat(0.5)) * CELL_M;
                let expected = footprints.iter().any(|(footprint, pos, yaw)| {
                    let (s, c) = yaw.sin_cos();
                    let p = world - Vec2::new(pos.x, pos.z);
                    let local = Vec2::new(p.dot(Vec2::new(c, -s)), p.dot(Vec2::new(s, c)));
                    footprint
                        .bands
                        .iter()
                        .any(|(lo, hi)| local.cmpge(*lo).all() && local.cmple(*hi).all())
                });
                let actual = data[(z * PAGE_RES + x) as usize] > 0;
                assert_eq!(actual, expected, "GPU footprint at {world:?}");
                touched += usize::from(actual);
            }
        }
        assert!(
            touched > 500,
            "both track pairs visibly stamp the production atlas: {touched}"
        );
        eprintln!(
            "GPU checked {} cells; {touched} source-covered cells, no widened edges",
            PAGE_RES * PAGE_RES
        );
        drop(data);
        readback.unmap();
        Ok(())
    }

    #[test]
    fn rut_widths_are_measured_from_both_baked_hulls() {
        for (path, expected_width, expected_center) in [
            ("assets/tanks/tiger/baked/tiger_hull.glb", 0.78, 1.54),
            ("assets/tanks/T34-76/baked/t34_hull.glb", 0.7065, 1.4321),
        ] {
            if !std::path::Path::new(path).exists() {
                continue;
            }
            let mesh = crate::gltf_load::load_glb(path).unwrap();
            let footprint =
                Footprint::from_hull(&mesh).expect("source hull has two lower tread bands");
            for (lo, hi) in footprint.bands {
                eprintln!(
                    "{path}: width={} center={} z={}..{}",
                    hi.x - lo.x,
                    (lo.x + hi.x) * 0.5,
                    lo.y,
                    hi.y
                );
                assert!((hi.x - lo.x - expected_width).abs() < 0.025);
                assert!(((lo.x + hi.x).abs() * 0.5 - expected_center).abs() < 0.025);
            }
            for yaw in [0.0, 0.4, 1.2, 2.8] {
                let quads = footprint.quads(Vec3::new(42.0, 0.0, -81.0), yaw);
                for ((lo, hi), quad) in footprint.bands.into_iter().zip(quads) {
                    assert!((quad[0].distance(quad[1]) - (hi.x - lo.x)).abs() < 1e-5);
                }
            }
        }
    }

    #[test]
    fn page_boundaries_preserve_the_footprint_area_without_drawing_into_other_pages() {
        let quad = Footprint {
            bands: [(Vec2::new(1.04, -2.0), Vec2::new(1.82, 2.2)); 2],
        }
        .quads(Vec3::new(-1.5, 0.0, 64.0), 0.7)[0];
        let mut atlas = Atlas::default();
        let mut verts = Vec::new();
        atlas.stamp(quad, 0.28, &mut verts);
        assert!(atlas.pages.len() >= 2);
        let area: f32 = verts
            .chunks_exact(3)
            .map(|t| {
                let p = |i: usize| Vec2::new(t[i][0], t[i][1]) * ATLAS_RES as f32 * CELL_M;
                (p(1) - p(0)).perp_dot(p(2) - p(0)).abs() * 0.5
            })
            .sum();
        assert!((area - 0.78 * 4.2).abs() < 0.002, "area {area}");
        let old = atlas.pages.clone();
        atlas.stamp(quad, 0.28, &mut Vec::new());
        assert_eq!(old, atlas.pages);
    }

    #[test]
    fn banked_hull_projects_its_tracks_without_restoring_flat_width() {
        let footprint = Footprint {
            bands: [(Vec2::new(1.15, -2.0), Vec2::new(1.93, 2.2)); 2],
        };
        let quad = footprint.quads_for_model(Mat4::from_rotation_z(45f32.to_radians()))[0];
        assert!((quad[0].distance(quad[1]) - 0.78 * 45f32.to_radians().cos()).abs() < 1e-6);
    }

    #[test]
    fn atlas_budget_never_evicts_old_tracks_or_aliases_new_pages() {
        let mut atlas = Atlas::default();
        for z in 0..ATLAS_PAGES as i32 {
            for x in 0..ATLAS_PAGES as i32 {
                assert!(atlas.page((x, z)).is_some());
            }
        }
        let old = atlas.pages.clone();
        assert_eq!(atlas.page((-1, -1)), None);
        assert_eq!(atlas.page((0, 0)), Some(0));
        assert_eq!(old, atlas.pages);
        assert_eq!(atlas.new_pages.len(), 256);
    }
}
