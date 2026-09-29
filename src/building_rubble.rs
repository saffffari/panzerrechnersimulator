//! Rubble as solid geometry (solid city, 2026-09-29).
//!
//! A failed building module leaves one rubble mound. The mound is a heightfield surface in its kit
//! asset's frame ([`MoundShape`]): a frustum over the module's solids with flanks no steeper than
//! loose brick's angle of repose, broken into lumps, and continuing below its base as an apron so
//! it drapes over whatever it lands on. The renderer draws exactly these vertices and triangles
//! ([`MoundShape::mesh`]); collision evaluates the same triangles ([`MoundShape::height_at`]), so
//! what a tank, a soldier or a shell meets is the rendered surface (the measured difference is
//! float rounding: `tests::collision_is_the_rendered_surface`).
//!
//! Volume (P G17): a mound holds its module's solid volume times a bulking of 1.5 for masonry and
//! concrete, 0.6 for timber and 0.05 for sheet iron and steel (an engineering estimate). Where it
//! lands ([`RubblePile::deposit`]) the mound is raised until the volume it adds above everything
//! already there (terrain, the building's intact floors under it, earlier rubble) is exactly that
//! budget, so rubble stacks: the walls of five storeys failing at one place leave a ridge several
//! metres high whose flanks stay at the angle of repose, as the photographs show 1.5-3 m slopes
//! against four- and five-storey ruins (P G17, [04-04]). The pile is the union of its mounds; the
//! union of surfaces no steeper than the repose angle is itself no steeper (a maximum of Lipschitz
//! functions keeps their constant), so the pile's smooth flanks never exceed it; the lumps add a
//! local roughness of at most 0.1 m.
//!
//! Named simplifications (docs/PHYSICAL_TRUTH_AUDIT.md, "Rubble"): the repose angle of loose brick
//! (37°) is an engineering assumption (R1 §8.5 gives 35-40°); a mound's shape is a frustum per
//! module, not a granular simulation; rubble does not flow through or around standing walls (a
//! mound's apron may pass under a wall that stands); a pile is laid in the order its modules failed
//! and rebuilt in placement order after a snapshot import.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::gltf_load::{CpuMesh, Vertex};

/// Angle of repose of loose brick rubble: 35-40° (R1 §8.5, an engineering assumption).
pub const REPOSE_DEG: f32 = 37.0;
/// Least berm a mound spreads beyond its module's footprint; a taller module spreads half its
/// height (P G17: gutted buildings leave an external berm along the base).
pub const RUBBLE_BERM_M: f32 = 1.0;
/// How far a mound's flank continues below its base (at the repose slope), so a mound laid on
/// uneven rubble meets it along a slope instead of floating over a gap.
pub const APRON_DEPTH_M: f32 = 1.5;
/// The vertical skirt that closes the mesh below the apron's rim.
const SKIRT_M: f32 = 0.5;
/// Mound grid spacing target, and the most cells along one side.
const CELL_TARGET_M: f32 = 0.3;
const MAX_CELLS: usize = 48;
/// Loose bricks and bonded lumps of 3-20 bricks (0.2-0.6 m; P G17): the surface is broken by
/// lumps of up to this height (at most 0.3 of the mound's crest), fading out toward the berm.
pub const RELIEF_MAX_M: f32 = 0.10;
/// Seeded variants of each kit asset's mound (their lumps differ).
pub const MOUND_VARIANTS: u32 = 4;

/// Bulking of a failed module's solid volume into rubble. Masonry and concrete: about 1.5 (P G17,
/// an engineering estimate, not a photographic measurement). Timber breaks and chars into less than
/// its volume, and sheet iron and rolled steel leave almost none: roof iron lies as curled sheet
/// on the rubble (P G16), not as a heap.
pub fn rubble_bulking(material: crate::building_kit::KitMaterial) -> f32 {
    use crate::building_kit::KitMaterial::*;
    match material {
        Brick | Stone | Concrete => 1.5,
        Timber => 0.6,
        Metal => 0.05,
    }
}

fn tan_repose() -> f32 {
    REPOSE_DEG.to_radians().tan()
}

fn hash01(seed: u64, i: u32, j: u32) -> f32 {
    let mut z = seed ^ (u64::from(i).wrapping_mul(0x9E37_79B9_7F4A_7C15)) ^ (u64::from(j) << 32);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn fnv1a(bytes: impl IntoIterator<Item = u8>, seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Integral of max(0, y) over a triangle whose vertices are (x, y, z) with y linear over it: the
/// part above y = 0 clipped exactly (a linear function's integral over a polygon is its area times
/// the value at its centroid; each clipped piece is a triangle fan).
fn positive_volume(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let pts = [a, b, c];
    let mut poly: Vec<Vec3> = Vec::with_capacity(4);
    for k in 0..3 {
        let (p, q) = (pts[k], pts[(k + 1) % 3]);
        if p.y >= 0.0 {
            poly.push(p);
        }
        if (p.y >= 0.0) != (q.y >= 0.0) {
            let t = p.y / (p.y - q.y);
            poly.push(p + (q - p) * t);
        }
    }
    let mut volume = 0.0;
    for k in 1..poly.len().saturating_sub(1) {
        let (p0, p1, p2) = (poly[0], poly[k], poly[k + 1]);
        let area = 0.5 * ((p1.x - p0.x) * (p2.z - p0.z) - (p2.x - p0.x) * (p1.z - p0.z)).abs();
        volume += area * (p0.y + p1.y + p2.y) / 3.0;
    }
    volume
}

/// One failed module's rubble in its kit asset's frame: a heightfield over a regular XZ grid, with
/// each cell split along its (i+1, j)-(i, j+1) diagonal (the terrain's convention). Heights are
/// above the mound's base plane y = 0; the apron runs below it.
#[derive(Clone, Debug)]
pub struct MoundShape {
    pub asset_id: String,
    pub origin: Vec2,
    pub cell: Vec2,
    pub nx: usize,
    pub nz: usize,
    pub heights: Vec<f32>,
    /// The module solids' XZ union (the mound's crest region).
    #[cfg_attr(not(test), allow(dead_code))]
    pub core_min: Vec2,
    #[cfg_attr(not(test), allow(dead_code))]
    pub core_max: Vec2,
    /// Design crest height and flank slope (tan) of the smooth profile, and the lumps' amplitude.
    pub crest: f32,
    pub slope: f32,
    pub relief: f32,
    /// Volume above y = 0: the module's solid volume times its bulking (exact for the mesh).
    pub budget: f32,
}

impl MoundShape {
    fn node(&self, i: usize, j: usize) -> f32 {
        self.heights[j * self.nx + i]
    }

    fn node_xz(&self, i: usize, j: usize) -> Vec2 {
        self.origin + Vec2::new(i as f32 * self.cell.x, j as f32 * self.cell.y)
    }

    /// XZ extent of the grid in the asset frame.
    pub fn extent(&self) -> (Vec2, Vec2) {
        (
            self.origin,
            self.origin + Vec2::new((self.nx - 1) as f32 * self.cell.x, (self.nz - 1) as f32 * self.cell.y),
        )
    }

    /// Height of the rendered surface at an asset-frame XZ point (`None` outside the grid): the
    /// same triangles the mesh draws.
    pub fn height_at(&self, p: Vec2) -> Option<f32> {
        let g = (p - self.origin) / self.cell;
        let (max_i, max_j) = ((self.nx - 1) as f32, (self.nz - 1) as f32);
        if !(g.x >= 0.0 && g.y >= 0.0 && g.x <= max_i && g.y <= max_j) {
            return None;
        }
        let i = (g.x.floor() as usize).min(self.nx - 2);
        let j = (g.y.floor() as usize).min(self.nz - 2);
        let (u, v) = (g.x - i as f32, g.y - j as f32);
        let (h00, h10, h01, h11) = (self.node(i, j), self.node(i + 1, j), self.node(i, j + 1), self.node(i + 1, j + 1));
        Some(if u + v <= 1.0 {
            h00 + (h10 - h00) * u + (h01 - h00) * v
        } else {
            h11 + (h01 - h11) * (1.0 - u) + (h10 - h11) * (1.0 - v)
        })
    }

    /// Triangles of the top surface as asset-frame points.
    fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        (0..self.nz - 1).flat_map(move |j| {
            (0..self.nx - 1).flat_map(move |i| {
                let p = |ii: usize, jj: usize| {
                    let xz = self.node_xz(ii, jj);
                    Vec3::new(xz.x, self.node(ii, jj), xz.y)
                };
                [
                    [p(i, j), p(i, j + 1), p(i + 1, j)],
                    [p(i + 1, j), p(i, j + 1), p(i + 1, j + 1)],
                ]
            })
        })
    }

    /// Exact volume of the rendered surface above the plane y = `level`.
    pub fn volume_above(&self, level: f32) -> f32 {
        let shift = Vec3::new(0.0, level, 0.0);
        self.triangles()
            .map(|[a, b, c]| positive_volume(a - shift, b - shift, c - shift))
            .sum()
    }

    /// The largest slope (rise over run) of any triangle of the smooth part of the surface.
    #[cfg(test)]
    pub fn steepest_slope(&self) -> f32 {
        self.triangles()
            .map(|[a, b, c]| {
                let n = (c - a).cross(b - a);
                if n.y.abs() < 1e-9 {
                    f32::INFINITY
                } else {
                    Vec2::new(n.x, n.z).length() / n.y.abs()
                }
            })
            .fold(0.0, f32::max)
    }

    /// The mound's render mesh: the surface's vertices and triangles, textured with the module's
    /// own material at its own scale (`density` texture units per metre, from the source mesh),
    /// and a vertical skirt below the apron's rim.
    pub fn mesh(&self, source: &CpuMesh, variant: u32) -> CpuMesh {
        let density = uv_density(source);
        let seed = fnv1a(self.asset_id.bytes(), u64::from(variant) ^ 0x5EED);
        let offset = [hash01(seed, 1, 0), hash01(seed, 2, 0)];
        let mut vertices = Vec::with_capacity(self.nx * self.nz + 4 * (self.nx + self.nz));
        for j in 0..self.nz {
            for i in 0..self.nx {
                let xz = self.node_xz(i, j);
                let h = |ii: usize, jj: usize| self.node(ii.min(self.nx - 1), jj.min(self.nz - 1));
                let dhdx = (h(i + 1, j) - h(i.saturating_sub(1), j))
                    / (((i + 1).min(self.nx - 1) - i.saturating_sub(1)) as f32 * self.cell.x);
                let dhdz = (h(i, j + 1) - h(i, j.saturating_sub(1)))
                    / (((j + 1).min(self.nz - 1) - j.saturating_sub(1)) as f32 * self.cell.y);
                let normal = Vec3::new(-dhdx, 1.0, -dhdz).normalize();
                let tangent = (Vec3::X - normal * normal.dot(Vec3::X)).normalize_or_zero();
                vertices.push(Vertex {
                    pos: [xz.x, self.node(i, j), xz.y],
                    normal: normal.to_array(),
                    uv: [offset[0] + xz.x * density, offset[1] + xz.y * density],
                    tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                });
            }
        }
        let index = |i: usize, j: usize| (j * self.nx + i) as u32;
        let mut indices = Vec::with_capacity((self.nx - 1) * (self.nz - 1) * 6);
        for j in 0..self.nz - 1 {
            for i in 0..self.nx - 1 {
                let (a, b, c, d) = (index(i, j), index(i + 1, j), index(i, j + 1), index(i + 1, j + 1));
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        // Skirt: the grid's rim, walked counter-clockwise seen from above, dropped by SKIRT_M.
        let mut rim: Vec<(usize, usize, Vec3)> = Vec::new();
        for i in 0..self.nx - 1 {
            rim.push((i, 0, Vec3::NEG_Z));
        }
        for j in 0..self.nz - 1 {
            rim.push((self.nx - 1, j, Vec3::X));
        }
        for i in (1..self.nx).rev() {
            rim.push((i, self.nz - 1, Vec3::Z));
        }
        for j in (1..self.nz).rev() {
            rim.push((0, j, Vec3::NEG_X));
        }
        for k in 0..rim.len() {
            let (i0, j0, normal) = rim[k];
            let (i1, j1, _) = rim[(k + 1) % rim.len()];
            let base = vertices.len() as u32;
            for (i, j) in [(i0, j0), (i1, j1)] {
                let xz = self.node_xz(i, j);
                let top = self.node(i, j);
                for y in [top, top - SKIRT_M] {
                    vertices.push(Vertex {
                        pos: [xz.x, y, xz.y],
                        normal: normal.to_array(),
                        uv: [offset[0] + (xz.x + xz.y) * density, offset[1] + y * density],
                        tangent: [0.0, 1.0, 0.0, 1.0],
                    });
                }
            }
            // (top0, bottom0, top1, bottom1): outward faces.
            indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
        }
        let (lo, hi) = self.extent();
        let (min_h, max_h) = self
            .heights
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), h| (a.min(*h), b.max(*h)));
        CpuMesh::basic(
            vertices,
            indices,
            source.texture.clone(),
            Vec3::new(lo.x, min_h - SKIRT_M, lo.y),
            Vec3::new(hi.x, max_h, hi.y),
        )
    }
}

/// Texture units per metre of a mesh (total UV edge length over total position edge length), so
/// rubble carries its module's own material at its own scale.
pub fn uv_density(mesh: &CpuMesh) -> f32 {
    let (mut uv, mut xyz) = (0.0_f32, 0.0_f32);
    for tri in mesh.indices.chunks_exact(3) {
        for k in 0..3 {
            let (a, b) = (&mesh.vertices[tri[k] as usize], &mesh.vertices[tri[(k + 1) % 3] as usize]);
            let dp = (Vec3::from_array(a.pos) - Vec3::from_array(b.pos)).length();
            if dp > 1.0e-4 {
                xyz += dp;
                uv += ((a.uv[0] - b.uv[0]).powi(2) + (a.uv[1] - b.uv[1]).powi(2)).sqrt();
            }
        }
    }
    if xyz > 0.0 { uv / xyz } else { 0.25 }
}

/// Build the mound of a kit asset: `None` when the asset has no solids (it leaves no rubble).
pub fn build_mound(asset_id: &str, variant: u32) -> Option<MoundShape> {
    let asset = crate::building_kit::asset(asset_id)?;
    let shape = crate::building_kit::shape(asset_id)?;
    if shape.solids.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut volume = 0.0_f32;
    for solid in shape.solids {
        let (min, max) = (Vec3::from(solid.min), Vec3::from(solid.max));
        lo = lo.min(min);
        hi = hi.max(max);
        let e = max - min;
        volume += e.x * e.y * e.z;
    }
    let budget = volume * rubble_bulking(asset.material);
    let (a, b) = (hi.x - lo.x, hi.z - lo.z);
    // A frustum whose flank is at distance d (Euclidean, so its corners are quarter cones and the
    // flank's slope is the same all round) from the footprint: volume per unit crest height.
    let frustum = |w: f32| a * b + (a + b) * w + std::f32::consts::PI / 3.0 * w * w;
    let tan_r = tan_repose();
    let mut w = RUBBLE_BERM_M.max(0.5 * (hi.y - lo.y));
    let mut crest = budget / frustum(w);
    if crest > tan_r * w {
        // Too steep: spread until the flank is at the angle of repose (the volume grows
        // monotonically with w at h = tan · w).
        let (mut w_lo, mut w_hi) = (w, w.max(1.0));
        while tan_r * w_hi * frustum(w_hi) < budget {
            w_hi *= 2.0;
        }
        for _ in 0..60 {
            let mid = 0.5 * (w_lo + w_hi);
            if tan_r * mid * frustum(mid) < budget {
                w_lo = mid;
            } else {
                w_hi = mid;
            }
        }
        w = w_hi;
        crest = tan_r * w;
    }
    let slope = crest / w;
    let apron = APRON_DEPTH_M / tan_r;
    let core_min = Vec2::new(lo.x, lo.z);
    let core_max = Vec2::new(hi.x, hi.z);
    let reach = w + apron;
    let (g_lo, g_hi) = (core_min - Vec2::splat(reach), core_max + Vec2::splat(reach));
    let size = g_hi - g_lo;
    let cells = |length: f32| ((length / CELL_TARGET_M).ceil() as usize).clamp(4, MAX_CELLS);
    let (cx, cz) = (cells(size.x), cells(size.y));
    let cell = Vec2::new(size.x / cx as f32, size.y / cz as f32);
    let (nx, nz) = (cx + 1, cz + 1);
    let seed = fnv1a(asset_id.bytes(), u64::from(variant));
    let relief = RELIEF_MAX_M.min(0.3 * crest);
    let mut heights = Vec::with_capacity(nx * nz);
    for j in 0..nz {
        for i in 0..nx {
            let p = g_lo + Vec2::new(i as f32 * cell.x, j as f32 * cell.y);
            let d = (core_min - p).max(p - core_max).max(Vec2::ZERO).length();
            let smooth = if d <= w { crest - slope * d } else { -(d - w) * tan_r };
            let fade = (1.0 - d / w).clamp(0.0, 1.0);
            let lump = (hash01(seed, i as u32, j as u32) * 2.0 - 1.0) * relief * fade;
            heights.push(smooth.max(-APRON_DEPTH_M) + lump);
        }
    }
    let mut mound = MoundShape {
        asset_id: asset_id.to_owned(),
        origin: g_lo,
        cell,
        nx,
        nz,
        heights,
        core_min,
        core_max,
        crest,
        slope,
        relief,
        budget,
    };
    // Make the drawn surface hold exactly the budget above its base: scale its positive part (the
    // zero set, and so the clipping, is unchanged, so the volume scales linearly).
    let held = mound.volume_above(0.0);
    if held > 1.0e-6 {
        let k = budget / held;
        for h in &mut mound.heights {
            if *h > 0.0 {
                *h *= k;
            }
        }
    }
    Some(mound)
}

/// The mound of a kit asset and variant, built once per process.
pub fn mound(asset_id: &str, variant: u32) -> Option<Arc<MoundShape>> {
    static CACHE: OnceLock<Mutex<HashMap<(String, u32), Option<Arc<MoundShape>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (asset_id.to_owned(), variant % MOUND_VARIANTS);
    if let Some(found) = cache.lock().expect("mound cache").get(&key) {
        return found.clone();
    }
    let built = build_mound(asset_id, key.1).map(Arc::new);
    cache.lock().expect("mound cache").insert(key, built.clone());
    built
}

/// The rubble variant a module takes: a counter-seeded hash of its instance id and placement.
pub fn mound_variant(instance_id: &str, module: usize) -> u32 {
    let hash = fnv1a(instance_id.bytes(), module as u64);
    let mixed = ((hash ^ (hash >> 29)) as u32).wrapping_mul(2_654_435_761) >> 8;
    mixed % MOUND_VARIANTS
}

/// Horizontal spread and vertical scale of a module's mound. A module's instance scales its kit
/// asset non-uniformly (fitted bay, storey pitch, wall thickness); scaling the mound the same way
/// would steepen its flanks by `sy / sx`. The mound keeps the module's volume scale
/// (`sx · sy · sz`) but spreads by `f` horizontally and scales by `sy / f²` vertically, with `f`
/// the least spread that keeps its flanks within the repose angle.
pub fn mound_scale(module_world: Mat4, shape: &MoundShape) -> Vec3 {
    let sx = module_world.x_axis.truncate().length();
    let sy = module_world.y_axis.truncate().length();
    let sz = module_world.z_axis.truncate().length();
    let steep = shape.slope.max(1.0e-6) * sy / (tan_repose() * sx.min(sz).max(1.0e-6));
    let spread = steep.cbrt().max(1.0);
    Vec3::new(sx * spread, sy / (spread * spread), sz * spread)
}

/// The world transform of a module's mound with its base plane at `base_y`: the module's own
/// horizontal position and quarter-turn heading, the scale of [`mound_scale`].
pub fn mound_transform(module_world: Mat4, shape: &MoundShape, base_y: f32) -> Mat4 {
    let scale = mound_scale(module_world, shape);
    let x = module_world.x_axis.truncate().normalize_or_zero();
    let z = module_world.z_axis.truncate().normalize_or_zero();
    let translation = module_world.w_axis.truncate();
    Mat4::from_cols(
        (x * scale.x).extend(0.0),
        Vec4::new(0.0, scale.y, 0.0, 0.0),
        (z * scale.z).extend(0.0),
        Vec4::new(translation.x, base_y, translation.z, 1.0),
    )
}

/// One placed mound of a pile.
#[derive(Clone, Debug)]
pub struct PlacedMound {
    pub module: usize,
    pub shape: Arc<MoundShape>,
    pub transform: Mat4,
    inverse: Mat4,
    pub world_min: Vec3,
    pub world_max: Vec3,
    /// Volume it added above what was there before, measured on its grid (≈ its world budget).
    #[cfg_attr(not(test), allow(dead_code))]
    pub added_volume: f32,
    /// The building modules (floors, stairs) whose top carries this mound, directly or through
    /// the mounds it lies on: a load on the mound bears on them.
    pub resting_on: Vec<usize>,
}

impl PlacedMound {
    fn new(module: usize, shape: Arc<MoundShape>, transform: Mat4, added_volume: f32) -> Self {
        let (lo, hi) = shape.extent();
        let (min_h, max_h) = shape
            .heights
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), h| (a.min(*h), b.max(*h)));
        let mut world_min = Vec3::splat(f32::INFINITY);
        let mut world_max = Vec3::splat(f32::NEG_INFINITY);
        for corner in 0..8 {
            let p = transform.transform_point3(Vec3::new(
                if corner & 1 == 0 { lo.x } else { hi.x },
                if corner & 2 == 0 { min_h - SKIRT_M } else { max_h },
                if corner & 4 == 0 { lo.y } else { hi.y },
            ));
            world_min = world_min.min(p);
            world_max = world_max.max(p);
        }
        Self {
            module,
            shape,
            inverse: transform.inverse(),
            transform,
            world_min,
            world_max,
            added_volume,
            resting_on: Vec::new(),
        }
    }

    /// World height of the mound's surface over a world XZ point (`None` outside it).
    pub fn top_at(&self, xz: Vec2) -> Option<f32> {
        if xz.x < self.world_min.x || xz.x > self.world_max.x || xz.y < self.world_min.z || xz.y > self.world_max.z {
            return None;
        }
        let local = self.inverse.transform_point3(Vec3::new(xz.x, 0.0, xz.y));
        let h = self.shape.height_at(Vec2::new(local.x, local.z))?;
        Some(self.transform.w_axis.y + self.transform.y_axis.y * h)
    }

    /// World height of the lowest point of the mound's crest region (its crest less its lumps),
    /// for sight occluders: never more than the drawn rubble. The scaling that made the drawn
    /// surface hold its budget exactly is at most a few per cent, so a 5 % margin covers it.
    pub fn core_floor(&self) -> f32 {
        let lowest = (self.shape.crest * 0.95 - self.shape.relief).max(0.0);
        self.transform.w_axis.y + self.transform.y_axis.y * lowest
    }

    /// World XZ of the module's footprint (the mound's crest region).
    #[cfg(test)]
    pub fn core_rect(&self) -> (Vec2, Vec2) {
        let a = self.transform.transform_point3(Vec3::new(self.shape.core_min.x, 0.0, self.shape.core_min.y));
        let b = self.transform.transform_point3(Vec3::new(self.shape.core_max.x, 0.0, self.shape.core_max.y));
        (Vec2::new(a.x.min(b.x), a.z.min(b.z)), Vec2::new(a.x.max(b.x), a.z.max(b.z)))
    }
}

/// Cell of a pile's mound index.
const PILE_CELL_M: f32 = 4.0;

/// Every mound a building's failed modules have left, in the order they fell.
#[derive(Clone, Debug, Default)]
pub struct RubblePile {
    mounds: Vec<Option<PlacedMound>>,
    order: Vec<usize>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl RubblePile {
    pub fn new(modules: usize) -> Self {
        Self { mounds: vec![None; modules], order: Vec::new(), grid: HashMap::new() }
    }

    pub fn mound(&self, module: usize) -> Option<&PlacedMound> {
        self.mounds.get(module).and_then(Option::as_ref)
    }

    pub fn placed(&self) -> impl Iterator<Item = &PlacedMound> + '_ {
        self.order.iter().filter_map(|&m| self.mounds[m].as_ref())
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Modules in the order their mounds were laid.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    fn cells(min: Vec3, max: Vec3) -> impl Iterator<Item = (i32, i32)> {
        let (x0, x1) = ((min.x / PILE_CELL_M).floor() as i32, (max.x / PILE_CELL_M).floor() as i32);
        let (z0, z1) = ((min.z / PILE_CELL_M).floor() as i32, (max.z / PILE_CELL_M).floor() as i32);
        (x0..=x1).flat_map(move |x| (z0..=z1).map(move |z| (x, z)))
    }

    /// Highest rubble surface over a world XZ point, and the module whose mound it is.
    pub fn top_at(&self, xz: Vec2) -> Option<(f32, usize)> {
        let key = ((xz.x / PILE_CELL_M).floor() as i32, (xz.y / PILE_CELL_M).floor() as i32);
        let mut best: Option<(f32, usize)> = None;
        for &module in self.grid.get(&key).into_iter().flatten() {
            if let Some(h) = self.mounds[module as usize].as_ref().and_then(|m| m.top_at(xz)) {
                if best.map_or(true, |(b, _)| h > b) {
                    best = Some((h, module as usize));
                }
            }
        }
        best
    }

    /// Lay a failed module's mound: at the module's position, raised until the volume it adds
    /// above `support` (terrain and intact floors under it, with the floor module carrying a point
    /// when there is one) and the rubble already here equals its world budget. Returns the mound's
    /// transform.
    pub fn deposit(
        &mut self,
        module: usize,
        module_world: Mat4,
        shape: Arc<MoundShape>,
        support: &dyn Fn(Vec2) -> (f32, Option<usize>),
    ) -> Mat4 {
        let probe = mound_transform(module_world, &shape, 0.0);
        let scale_y = probe.y_axis.y;
        let volume_scale = probe.x_axis.truncate().length() * scale_y * probe.z_axis.truncate().length();
        let budget = shape.budget * volume_scale;
        // Every node of the mound's grid in the world, its trapezoid area weight, the height of
        // what lies there already and what carries it (a floor module, or an earlier mound).
        #[derive(Clone, Copy)]
        enum Carrier {
            Ground,
            Floor(usize),
            Mound(usize),
        }
        let cell_area = shape.cell.x * shape.cell.y * probe.x_axis.truncate().length() * probe.z_axis.truncate().length();
        let mut nodes: Vec<(f32, f32, f32)> = Vec::with_capacity(shape.nx * shape.nz);
        let mut carriers: Vec<Carrier> = Vec::with_capacity(shape.nx * shape.nz);
        for j in 0..shape.nz {
            for i in 0..shape.nx {
                let local = shape.node_xz(i, j);
                let world = probe.transform_point3(Vec3::new(local.x, 0.0, local.y));
                let xz = Vec2::new(world.x, world.z);
                let (ground, floor) = support(xz);
                let (below, carrier) = match self.top_at(xz) {
                    Some((h, mound)) if h > ground => (h, Carrier::Mound(mound)),
                    _ => (ground, floor.map_or(Carrier::Ground, Carrier::Floor)),
                };
                let edge_i = i == 0 || i == shape.nx - 1;
                let edge_j = j == 0 || j == shape.nz - 1;
                let weight = match (edge_i, edge_j) {
                    (true, true) => 0.25,
                    (true, false) | (false, true) => 0.5,
                    _ => 1.0,
                };
                nodes.push((scale_y * shape.node(i, j), below, weight * cell_area));
                carriers.push(carrier);
            }
        }
        let added = |base: f32| -> f32 {
            nodes.iter().map(|&(h, below, area)| (base + h - below).max(0.0) * area).sum()
        };
        let mut lo = nodes.iter().map(|&(h, below, _)| below - h).fold(f32::INFINITY, f32::min);
        let mut hi = nodes.iter().map(|&(_, below, _)| below).fold(f32::NEG_INFINITY, f32::max);
        if !lo.is_finite() || !hi.is_finite() {
            lo = 0.0;
            hi = 0.0;
        }
        while added(hi) < budget {
            hi += 1.0;
        }
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if added(mid) < budget {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let base = hi;
        let transform = mound_transform(module_world, &shape, base);
        let mut placed = PlacedMound::new(module, shape, transform, added(base));
        // What carries it: the floors under the nodes it covers, and those under the mounds there.
        let mut resting_on: Vec<usize> = Vec::new();
        for (&(h, below, _), &carrier) in nodes.iter().zip(&carriers) {
            if base + h <= below {
                continue;
            }
            match carrier {
                Carrier::Ground => {}
                Carrier::Floor(floor) => resting_on.push(floor),
                Carrier::Mound(other) => {
                    if let Some(mound) = self.mounds.get(other).and_then(Option::as_ref) {
                        resting_on.extend(mound.resting_on.iter().copied());
                    }
                }
            }
        }
        resting_on.sort_unstable();
        resting_on.dedup();
        placed.resting_on = resting_on;
        for cell in Self::cells(placed.world_min, placed.world_max) {
            self.grid.entry(cell).or_default().push(module as u32);
        }
        if self.mounds[module].is_none() {
            self.order.push(module);
        }
        self.mounds[module] = Some(placed);
        transform
    }

    /// Forget every mound (a rebuild follows).
    pub fn clear(&mut self) {
        for mound in &mut self.mounds {
            *mound = None;
        }
        self.order.clear();
        self.grid.clear();
    }
}

/// Grid spacing of a [`TopSurface`] in its mesh's frame.
pub const TOP_SURFACE_CELL_M: f32 = 0.01;

/// The top surface of a mesh as a heightfield in the mesh's own frame, generated from its triangles:
/// at every node of a regular grid the highest triangle over it. Used for rubble that is part of a
/// building from the start (the grammar's burned-floor layers). `error_m` is measured against the
/// triangles on a finer set of points ([`TopSurface::measure_error`]).
#[derive(Clone, Debug)]
pub struct TopSurface {
    origin: Vec2,
    cell: f32,
    nx: usize,
    nz: usize,
    heights: Vec<f32>,
    pub error_m: f32,
}

impl TopSurface {
    pub fn from_mesh(mesh: &CpuMesh, cell: f32) -> Option<Self> {
        let (lo, hi) = (Vec2::new(mesh.min.x, mesh.min.z), Vec2::new(mesh.max.x, mesh.max.z));
        let nx = ((hi.x - lo.x) / cell).ceil() as usize + 1;
        let nz = ((hi.y - lo.y) / cell).ceil() as usize + 1;
        if nx < 2 || nz < 2 || nx * nz > 4_000_000 {
            return None;
        }
        let mut heights = vec![f32::NEG_INFINITY; nx * nz];
        for tri in mesh.indices.chunks_exact(3) {
            let p: [Vec3; 3] = std::array::from_fn(|k| Vec3::from(mesh.vertices[tri[k] as usize].pos));
            let (tmin, tmax) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
            let i0 = (((tmin.x - lo.x) / cell).floor().max(0.0)) as usize;
            let i1 = (((tmax.x - lo.x) / cell).ceil() as usize).min(nx - 1);
            let j0 = (((tmin.z - lo.y) / cell).floor().max(0.0)) as usize;
            let j1 = (((tmax.z - lo.y) / cell).ceil() as usize).min(nz - 1);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let q = lo + Vec2::new(i as f32 * cell, j as f32 * cell);
                    if let Some(h) = triangle_height(&p, q) {
                        let slot = &mut heights[j * nx + i];
                        *slot = slot.max(h);
                    }
                }
            }
        }
        let mut surface = Self { origin: lo, cell, nx, nz, heights, error_m: 0.0 };
        surface.error_m = surface.measure_error(mesh);
        Some(surface)
    }

    /// XZ extent of the grid in the mesh's frame.
    pub fn extent(&self) -> (Vec2, Vec2) {
        (
            self.origin,
            self.origin + Vec2::new((self.nx - 1) as f32, (self.nz - 1) as f32) * self.cell,
        )
    }

    /// Height of the top surface at a point of the mesh's frame: bilinear over the covered nodes of
    /// its cell, the highest covered node where the cell is partly covered, `None` off the mesh.
    pub fn height_at(&self, p: Vec2) -> Option<f32> {
        let g = (p - self.origin) / self.cell;
        if g.x < 0.0 || g.y < 0.0 || g.x > (self.nx - 1) as f32 || g.y > (self.nz - 1) as f32 {
            return None;
        }
        let i = (g.x.floor() as usize).min(self.nx - 2);
        let j = (g.y.floor() as usize).min(self.nz - 2);
        let (u, v) = (g.x - i as f32, g.y - j as f32);
        let h = [
            self.heights[j * self.nx + i],
            self.heights[j * self.nx + i + 1],
            self.heights[(j + 1) * self.nx + i],
            self.heights[(j + 1) * self.nx + i + 1],
        ];
        if h.iter().all(|h| h.is_finite()) {
            let a = h[0] + (h[1] - h[0]) * u;
            let b = h[2] + (h[3] - h[2]) * u;
            Some(a + (b - a) * v)
        } else {
            let best = h.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            best.is_finite().then_some(best)
        }
    }

    /// Largest vertical difference between the grid's answer and the mesh's highest triangle at
    /// every triangle's centroid and its edge midpoints (where both are defined).
    pub fn measure_error(&self, mesh: &CpuMesh) -> f32 {
        let triangles: Vec<[Vec3; 3]> = mesh
            .indices
            .chunks_exact(3)
            .map(|tri| std::array::from_fn(|k| Vec3::from(mesh.vertices[tri[k] as usize].pos)))
            .collect();
        let top = |q: Vec2| {
            triangles
                .iter()
                .filter_map(|p| triangle_height(p, q))
                .fold(f32::NEG_INFINITY, f32::max)
        };
        let mut worst = 0.0f32;
        for p in &triangles {
            let centroid = (p[0] + p[1] + p[2]) / 3.0;
            for q in [centroid, (p[0] + p[1]) * 0.5, (p[1] + p[2]) * 0.5, (p[2] + p[0]) * 0.5] {
                let q2 = Vec2::new(q.x, q.z);
                let exact = top(q2);
                if !exact.is_finite() {
                    continue;
                }
                if let Some(grid) = self.height_at(q2) {
                    worst = worst.max((grid - exact).abs());
                }
            }
        }
        worst
    }
}

/// Height of a triangle over an XZ point (`None` outside it or for a vertical triangle).
fn triangle_height(p: &[Vec3; 3], q: Vec2) -> Option<f32> {
    let (a, b, c) = (Vec2::new(p[0].x, p[0].z), Vec2::new(p[1].x, p[1].z), Vec2::new(p[2].x, p[2].z));
    let det = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
    if det.abs() < 1.0e-10 {
        return None;
    }
    let l1 = ((q.x - a.x) * (c.y - a.y) - (c.x - a.x) * (q.y - a.y)) / det;
    let l2 = ((b.x - a.x) * (q.y - a.y) - (q.x - a.x) * (b.y - a.y)) / det;
    let l0 = 1.0 - l1 - l2;
    const E: f32 = -1.0e-5;
    (l0 >= E && l1 >= E && l2 >= E).then(|| l0 * p[0].y + l1 * p[1].y + l2 * p[2].y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit_assets_with_solids() -> Vec<&'static str> {
        crate::building_kit::KIT
            .iter()
            .filter(|asset| crate::building_kit::shape(asset.id).is_some_and(|s| !s.solids.is_empty()))
            .map(|asset| asset.id)
            .collect()
    }

    #[test]
    fn every_mound_holds_its_budget_and_keeps_its_flanks_within_the_repose_angle() {
        let tan_r = tan_repose();
        let mut checked = 0;
        let mut worst_ratio = 0.0f32;
        for id in kit_assets_with_solids() {
            for variant in 0..MOUND_VARIANTS {
                let mound = build_mound(id, variant).unwrap();
                let held = mound.volume_above(0.0);
                assert!((held - mound.budget).abs() <= 1.0e-3 * mound.budget.max(1.0), "{id}: {held} for {}", mound.budget);
                assert!(mound.slope <= tan_r + 1.0e-4, "{id}: design slope {}", mound.slope);
                // The drawn triangles of the smooth profile: a cone sampled on the grid is steeper
                // than the cone by the chord effect near its apex; the lumps add at most two
                // relief heights over one cell diagonal.
                let lumps = 2.0 * RELIEF_MAX_M.min(0.3 * mound.crest) * 1.42 / mound.cell.min_element();
                let steepest = mound.steepest_slope();
                worst_ratio = worst_ratio.max((steepest - lumps) / tan_r);
                assert!(steepest <= tan_r * 1.1 + lumps, "{id} v{variant}: {steepest}");
                checked += 1;
            }
        }
        println!("rubble mounds: steepest drawn flank less lump relief, over tan {REPOSE_DEG}°: {worst_ratio:.3}");
        assert!(checked > 80, "{checked}");
    }

    #[test]
    fn collision_is_the_rendered_surface() {
        // The mesh's vertices are the grid nodes and its triangles the grid's; evaluating the
        // triangles of the drawn mesh at random points must give height_at.
        if !crate::building_kit::KIT.iter().all(|a| std::path::Path::new(a.path).is_file()) {
            eprintln!("skipping: kit assets are not installed");
            return;
        }
        let mut worst = 0.0f32;
        for id in ["pizza.structure.wall", "pizza.structure.floor", "urban.brick.window", "urban.timber.floor"] {
            let source = crate::building_kit::load_mesh(id).unwrap();
            let mound = build_mound(id, 1).unwrap();
            let mesh = mound.mesh(&source, 1);
            let top_tris = (mound.nx - 1) * (mound.nz - 1) * 2;
            let tris: Vec<[Vec3; 3]> = mesh.indices[..top_tris * 3]
                .chunks_exact(3)
                .map(|t| std::array::from_fn(|k| Vec3::from(mesh.vertices[t[k] as usize].pos)))
                .collect();
            let (lo, hi) = mound.extent();
            for k in 0..400u32 {
                let q = lo + (hi - lo) * Vec2::new(hash01(7, k, 1), hash01(7, k, 2));
                let drawn = tris.iter().filter_map(|t| triangle_height(t, q)).fold(f32::NEG_INFINITY, f32::max);
                let query = mound.height_at(q).unwrap();
                worst = worst.max((drawn - query).abs());
            }
        }
        println!("rubble collision against the drawn mound triangles: worst {worst:.2e} m");
        assert!(worst < 1.0e-4, "{worst}");
    }

    fn wall_bay() -> Mat4 {
        // A 4 m bay of a five-storey block: bay 1.0, storey 3.3 / 3, wall 0.64 / 0.4.
        Mat4::from_translation(Vec3::new(10.0, 0.0, 5.0)) * Mat4::from_scale(Vec3::new(1.0, 1.1, 1.6))
    }

    #[test]
    fn a_pile_conserves_each_mound_volume_and_stacks_into_a_ridge() {
        let shape = mound("pizza.structure.wall", 0).unwrap();
        let mut pile = RubblePile::new(8);
        let flat = |_: Vec2| (0.0f32, None);
        let mut crest = Vec::new();
        for storey in 0..5 {
            let world = Mat4::from_translation(Vec3::new(0.0, 3.3 * storey as f32, 0.0)) * wall_bay();
            pile.deposit(storey, world, Arc::clone(&shape), &flat);
            let placed = pile.mound(storey).unwrap();
            let budget = shape.budget * 1.0 * 1.1 * 1.6;
            assert!((placed.added_volume - budget).abs() <= 0.01 * budget, "storey {storey}: {} of {budget}", placed.added_volume);
            crest.push(pile.top_at(Vec2::new(10.0, 5.0)).unwrap().0);
        }
        // Five storeys of one bay make a ridge: 1.5-3 m and more against the wall (P G17, [04-04]).
        assert!(crest.windows(2).all(|w| w[1] > w[0]), "{crest:?}");
        assert!(crest[4] > 1.5, "{crest:?}");
        // Its smooth flanks stay within the repose angle: sample a line across the ridge.
        let tan_r = tan_repose();
        let mut previous = None;
        for k in 0..200 {
            let x = 10.0 + (k as f32 - 100.0) * 0.05;
            let h = pile.top_at(Vec2::new(10.3, 5.0 + (x - 10.0))).map_or(0.0, |(h, _)| h).max(0.0);
            if let Some(p) = previous {
                let slope: f32 = (h - p) / 0.05;
                assert!(slope.abs() <= tan_r + 2.0 * RELIEF_MAX_M / 0.05, "slope {slope} at {x}");
            }
            previous = Some(h);
        }
    }

    #[test]
    fn a_mound_lands_on_an_intact_floor_and_drapes_over_uneven_rubble() {
        let shape = mound("pizza.structure.wall", 2).unwrap();
        let mut pile = RubblePile::new(3);
        // An intact floor (module 42) 1.0 m up under the bay.
        let floor = |xz: Vec2| {
            if (xz.x - 10.0).abs() < 6.0 && (xz.y - 5.0).abs() < 6.0 { (1.0, Some(42)) } else { (0.0, None) }
        };
        let world = Mat4::from_translation(Vec3::new(0.0, 4.0, 0.0)) * wall_bay();
        pile.deposit(0, world, Arc::clone(&shape), &floor);
        let top = pile.top_at(Vec2::new(10.0, 5.0)).unwrap().0;
        assert!(top > 1.2, "the mound lies on the floor: {top}");
        let budget = shape.budget * 1.1 * 1.6;
        assert!((pile.mound(0).unwrap().added_volume - budget).abs() <= 0.01 * budget);
        // The floor carries it; a mound laid on it bears on the floor too.
        assert_eq!(pile.mound(0).unwrap().resting_on, vec![42]);
        pile.deposit(1, Mat4::from_translation(Vec3::new(0.0, 7.3, 0.0)) * wall_bay(), Arc::clone(&shape), &floor);
        assert_eq!(pile.mound(1).unwrap().resting_on, vec![42]);
        // On bare ground nothing carries a mound but the ground.
        let mut bare = RubblePile::new(1);
        bare.deposit(0, wall_bay(), shape, &|_| (0.0, None));
        assert!(bare.mound(0).unwrap().resting_on.is_empty());
    }

    #[test]
    fn mound_scale_keeps_the_module_volume_and_the_repose_limit() {
        let shape = mound("pizza.structure.wall", 0).unwrap();
        for scale in [Vec3::new(0.7, 1.5, 1.6), Vec3::new(1.1, 1.1, 1.3), Vec3::ONE] {
            let world = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2) * Mat4::from_scale(scale);
            let s = mound_scale(world, &shape);
            assert!((s.x * s.y * s.z - scale.x * scale.y * scale.z).abs() < 1e-4);
            let steepest = shape.slope * s.y / s.x.min(s.z);
            assert!(steepest <= tan_repose() + 1e-4, "{scale:?}: {steepest}");
        }
    }

    #[test]
    fn positive_volume_clips_triangles_exactly() {
        let v = positive_volume(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 1.0));
        assert!((v - 0.5).abs() < 1e-6);
        let v = positive_volume(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0), Vec3::new(0.0, -1.0, 1.0));
        // The positive part is a tetrahedron-like wedge: area 1/8 at the apex region, height 1.
        assert!((v - 1.0 / 24.0).abs() < 1e-5, "{v}");
    }

    #[test]
    fn a_top_surface_answers_its_mesh_within_its_measured_error() {
        if !crate::building_kit::KIT.iter().all(|a| std::path::Path::new(a.path).is_file()) {
            eprintln!("skipping: kit assets are not installed");
            return;
        }
        let mesh = crate::building_kit::load_mesh("urban.rubble.layer").unwrap();
        let surface = TopSurface::from_mesh(&mesh, TOP_SURFACE_CELL_M).unwrap();
        println!("burned-floor rubble layer top surface: measured error {:.3} m", surface.error_m);
        assert!(surface.error_m < 0.03, "{}", surface.error_m);
    }
}
