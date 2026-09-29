//! Instanced rendering of every compiled building (urban intro F2, 2026-09-28).
//!
//! Each kit asset is uploaded once as a shared prototype in its source frame, and so is the
//! grounded rubble of each kit asset (a few seeded variants). Every building module is one instance
//! of its asset's prototype at the module's world transform, and every module with solids owns one
//! instance of a rubble prototype: the heaps its solids leave on the building's ground. The
//! structural render delta switches the two per module (intact XOR rubble), so at any moment one of
//! them is visible; hidden instances occupy no TLAS slot (`Renderer::set_instance_visible`).
//!
//! Pages group one prototype's instances of the buildings whose anchor lies in one 128 m cell, so
//! the renderer's spatial index culls a district cell by cell. The plan is renderer-independent
//! and deterministic: prototypes are ordered by (asset id, kind), pages by (prototype, cell), and
//! instances within a page by (building, module).
//!
//! Before this module, every module was a loose scene mesh with one pre-allocated rubble heap per
//! collision proxy: 103 meshes and bottom-level structures per featured house.

use std::collections::BTreeMap;

use glam::{Mat4, Vec2, Vec3, Vec4};

use crate::building_blueprint::ModuleRenderState;
use crate::building_runtime::KitMeshCache;
use crate::building_set::BuildingSet;
use crate::gltf_load::CpuMesh;

/// Horizontal page size. Matches the renderer's spatial index cell (`INST_SPATIAL_CELL_METRES`)
/// and the tight manifest page scale for mutable content.
pub const PAGE_METRES: f32 = 128.0;
/// Seeded rubble heap variants per kit asset; a module takes one by hashing (instance, module).
pub const RUBBLE_VARIANTS: u32 = 4;

/// The grounded rubble box of one solid of a failed module: the solid's XZ footprint on the
/// building's ground, a low heap whose height grows with the solid's size (at most 0.6 m). Upper
/// slabs and roofs fall to the ground rather than leaving rubble at their former storey.
pub fn grounded_rubble_bounds(min: Vec3, max: Vec3, ground_y: f32) -> (Vec3, Vec3) {
    let footprint = max - min;
    (
        Vec3::new(min.x, ground_y - 0.10, min.z),
        Vec3::new(
            max.x,
            ground_y + 0.28 + footprint.max_element().min(4.0) * 0.08,
            max.z,
        ),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrototypeKind {
    /// The kit asset's source mesh.
    Module,
    /// The grounded heaps of the asset's solids, seeded variant.
    Rubble { variant: u32 },
}

pub struct PlannedPrototype {
    pub asset_id: String,
    pub kind: PrototypeKind,
    /// In the asset's source frame (rubble: XZ of the source frame, Y above the ground).
    pub mesh: CpuMesh,
    /// The asset declares a light fixture: the visible bulb/housing takes a modest own-colour glow
    /// (the room illumination comes from `BuildingRuntime::interior_lights`).
    pub light_fixture: bool,
}

pub struct PlannedPage {
    pub prototype: usize,
    /// The 128 m cell of the buildings on this page.
    #[cfg_attr(not(test), allow(dead_code))]
    pub cell: (i32, i32),
    pub transforms: Vec<Mat4>,
    /// Initial visibility: an intact instance is hidden when its module has failed, a rubble
    /// instance while its module stands.
    pub hidden: Vec<bool>,
    /// (building instance, module) of each page instance.
    pub owners: Vec<(u32, u32)>,
}

/// One instance of a planned page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageSlot {
    pub page: u32,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleSlots {
    pub intact: PageSlot,
    /// Present exactly when the module has solids (collision, damage and sight proxies).
    pub rubble: Option<PageSlot>,
}

pub struct BuildingRenderPlan {
    pub prototypes: Vec<PlannedPrototype>,
    pub pages: Vec<PlannedPage>,
    /// Slots of every module, indexed [building instance][compiled placement].
    pub modules: Vec<Vec<ModuleSlots>>,
}

impl BuildingRenderPlan {
    /// Instances in all pages.
    pub fn instance_count(&self) -> usize {
        self.pages.iter().map(|page| page.transforms.len()).sum()
    }

    /// Instances initially visible: one per module whose intact mesh or rubble shows.
    pub fn visible_count(&self) -> usize {
        self.pages
            .iter()
            .flat_map(|page| &page.hidden)
            .filter(|hidden| !**hidden)
            .count()
    }
}

/// The rubble transform of a module: the module's world transform with its heap standing on the
/// building's ground. Modules with solids turn only about +Y (the runtime refuses others), so the
/// module's X and Z axes are horizontal; the heap keeps them (and a window's X scale), keeps the
/// module's vertical scale on a vertical +Y axis, and stands at the ground height.
pub fn grounded_transform(module_world: Mat4, ground_y: f32) -> Mat4 {
    // Building frames and module transforms turn only about +Y, so the y axis is vertical and
    // carries the module's vertical scale: a taller module's rubble mound holds its larger volume.
    let mut model = module_world;
    model.y_axis = Vec4::new(0.0, module_world.y_axis.length(), 0.0, 0.0);
    model.w_axis.y = ground_y;
    model
}

/// Keep a failed module's grounded mound off an open cellar's excavation edge (cellars and scenes,
/// 29 Sep 2026). A mound is one instance at one height, so it cannot drape over the edge. One
/// grounded on the pad (a perimeter module's) moves outward, perpendicular to the excavation edge
/// its module stands on or beyond, until it clears the excavation rather than hang over it; one
/// grounded inside (on the live floor over the cellar, or on the cellar floor) moves, and narrows
/// where it is wider than the excavation, to lie within it. `footprint` is the mound's XZ bounds in
/// its asset frame, `rect` the excavation's world XZ bounds. Building frames turn only by quarter
/// turns about +Y, so the footprint's world bounds are exact.
pub fn rubble_clear_of_opening(
    transform: Mat4,
    footprint: (Vec2, Vec2),
    rect: (Vec2, Vec2),
    inside: bool,
) -> Mat4 {
    const MARGIN_M: f32 = 0.05;
    let (lo, hi) = footprint;
    let (mut min, mut max) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for (x, z) in [(lo.x, lo.y), (hi.x, lo.y), (lo.x, hi.y), (hi.x, hi.y)] {
        let p = transform.transform_point3(Vec3::new(x, 0.0, z));
        min = min.min(Vec2::new(p.x, p.z));
        max = max.max(Vec2::new(p.x, p.z));
    }
    let (r0, r1) = rect;
    if inside {
        let (r0, r1) = (r0 + Vec2::splat(MARGIN_M), r1 - Vec2::splat(MARGIN_M));
        let centre = (min + max) * 0.5;
        let (mut scale, mut shift) = (Vec2::ONE, Vec2::ZERO);
        for axis in 0..2 {
            let (a0, a1, b0, b1) = (min[axis], max[axis], r0[axis], r1[axis]);
            if a1 - a0 > b1 - b0 {
                scale[axis] = ((b1 - b0) / (a1 - a0)).max(0.0);
                shift[axis] = (b0 + b1) * 0.5 - centre[axis];
            } else if a0 < b0 {
                shift[axis] = b0 - a0;
            } else if a1 > b1 {
                shift[axis] = b1 - a1;
            }
        }
        let c = Vec3::new(centre.x, 0.0, centre.y);
        return Mat4::from_translation(Vec3::new(shift.x, 0.0, shift.y))
            * Mat4::from_translation(c)
            * Mat4::from_scale(Vec3::new(scale.x, 1.0, scale.y))
            * Mat4::from_translation(-c)
            * transform;
    }
    if max.x <= r0.x || min.x >= r1.x || max.y <= r0.y || min.y >= r1.y {
        return transform;
    }
    let module = Vec2::new(transform.w_axis.x, transform.w_axis.z);
    let out = [
        (module.x >= r1.x, Vec2::new(r1.x - min.x + MARGIN_M, 0.0)),
        (module.x <= r0.x, Vec2::new(r0.x - max.x - MARGIN_M, 0.0)),
        (module.y >= r1.y, Vec2::new(0.0, r1.y - min.y + MARGIN_M)),
        (module.y <= r0.y, Vec2::new(0.0, r0.y - max.y - MARGIN_M)),
    ];
    // Out through the edges the module stands beyond; a module on an edge line takes the
    // nearest way out.
    let any_beyond = out.iter().any(|(beyond, _)| *beyond);
    let shift = out
        .iter()
        .filter(|(beyond, _)| *beyond || !any_beyond)
        .map(|(_, shift)| *shift)
        .min_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap_or(Vec2::ZERO);
    Mat4::from_translation(Vec3::new(shift.x, 0.0, shift.y)) * transform
}

fn fnv1a(bytes: impl IntoIterator<Item = u8>, seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The rubble variant a module takes: a counter-seeded hash of its instance id and placement.
pub fn rubble_variant(instance_id: &str, module: usize) -> u32 {
    let hash = fnv1a(instance_id.bytes(), module as u64);
    let mixed = ((hash ^ (hash >> 29)) as u32).wrapping_mul(2_654_435_761) >> 8;
    mixed % RUBBLE_VARIANTS
}

/// Bulking of a failed module's solid volume into rubble. Masonry and concrete: about 1.5 (P G17,
/// an engineering estimate, not a photographic measurement). Timber breaks and chars into less than
/// its volume, and sheet iron and rolled steel leave almost none: roof iron lies as curled sheet
/// on the rubble (P G16), not as a heap.
fn rubble_bulking(material: crate::building_kit::KitMaterial) -> f32 {
    use crate::building_kit::KitMaterial::*;
    match material {
        Brick | Stone | Concrete => 1.5,
        Timber => 0.6,
        Metal => 0.05,
    }
}

/// The least berm a failed module's rubble spreads beyond its footprint on every side; a taller
/// module spreads half its height (P G17: gutted buildings leave an external berm along the base;
/// flattened houses are mounds of 0.3-1.5 m; walls falling outward make slopes).
const RUBBLE_BERM_M: f32 = 1.0;
/// The crest of one module's mound before its vertical scale (P G17: berms 1-2 m high; the higher
/// slopes against tall ruins build up from several failed modules).
const RUBBLE_CREST_M: f32 = 1.6;
/// Most lumps in one mound, and the smallest grid cell they are laid on (P G17: loose brick with
/// bonded lumps of 0.2-0.6 m, not uniform gravel; a lump fills 80-100 % of its cell).
const RUBBLE_LUMPS: u32 = 48;
const RUBBLE_CELL_M: f32 = 0.6;

/// Texture units per metre of a mesh (total UV edge length over total position edge length), so
/// rubble lumps carry their module's own material at its own scale.
fn uv_density(mesh: &CpuMesh) -> f32 {
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

/// One flat-shaded lump whose faces sample the source texture at `density` from `offset`.
fn textured_lump(
    min: Vec3,
    max: Vec3,
    density: f32,
    offset: [f32; 2],
    vertices: &mut Vec<crate::gltf_load::Vertex>,
    indices: &mut Vec<u32>,
) {
    let c = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
    };
    // (corners counter-clockwise seen from outside, normal, the two face axes the texture follows)
    let faces: [([usize; 4], Vec3, usize, usize); 6] = [
        ([1, 3, 7, 5], Vec3::X, 2, 1),
        ([0, 4, 6, 2], Vec3::NEG_X, 2, 1),
        ([2, 6, 7, 3], Vec3::Y, 0, 2),
        ([0, 1, 5, 4], Vec3::NEG_Y, 0, 2),
        ([4, 5, 7, 6], Vec3::Z, 0, 1),
        ([0, 2, 3, 1], Vec3::NEG_Z, 0, 1),
    ];
    for (corners, normal, u_axis, v_axis) in faces {
        let base = vertices.len() as u32;
        let tangent = normal.cross(if normal.x.abs() < 0.9 { Vec3::X } else { Vec3::Y }).normalize();
        for corner in corners {
            let p = c(corner);
            vertices.push(crate::gltf_load::Vertex {
                pos: p.to_array(),
                normal: normal.to_array(),
                uv: [offset[0] + p[u_axis] * density, offset[1] + p[v_axis] * density],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// The grounded rubble of one kit asset, in its XZ frame on the ground plane y = 0: a mound of
/// seeded lumps over the union of its solids widened by the berm, holding the solids' volume times
/// the material's bulking (P G17), textured with the asset's own material at its own scale. A
/// module's instance carries its horizontal and vertical scale (`grounded_transform`), so the
/// mound holds the module's volume. Render-only: tanks, feet and shells pass through them (named in
/// the physical-truth ledger). The visible volume (the union of the lumps above the ground) is
/// measured by `rubble_mound_visible_volume_is_measured_against_its_budget`: the lumps overlap, so
/// walls, slabs and stairs show 0.95-0.99 of their budget, but every lump keeps a 4 cm floor over
/// the berm-widened footprint, so thin or light pieces show more than they held (up to about 6
/// times for a timber board floor).
pub fn rubble_mesh(asset_id: &str, variant: u32, source: &CpuMesh) -> anyhow::Result<CpuMesh> {
    let asset = crate::building_kit::asset(asset_id)
        .ok_or_else(|| anyhow::anyhow!("kit asset {asset_id} is not in the kit"))?;
    let shape = crate::building_kit::shape(asset_id)
        .ok_or_else(|| anyhow::anyhow!("kit asset {asset_id} has no runtime shape for rubble"))?;
    anyhow::ensure!(!shape.solids.is_empty(), "kit asset {asset_id} has no solids to leave rubble");
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
    let berm = RUBBLE_BERM_M.max(0.5 * (hi.y - lo.y));
    let lo = glam::Vec2::new(lo.x, lo.z) - glam::Vec2::splat(berm);
    let hi = glam::Vec2::new(hi.x, hi.z) + glam::Vec2::splat(berm);
    let extent = hi - lo;
    let density = uv_density(source);
    let seed = fnv1a(asset_id.bytes(), u64::from(variant));
    let h = |k: u32, salt: u32| {
        let n = (seed as u32 ^ (seed >> 32) as u32)
            ^ k.wrapping_mul(0x9E37_79B9)
            ^ salt.wrapping_mul(0x85EB_CA77);
        let mixed = n.wrapping_mul(2_654_435_761);
        ((mixed ^ (mixed >> 15)).wrapping_mul(2_246_822_519) >> 8) as f32 / (1u32 << 24) as f32
    };
    // Lumps on a jittered grid over the widened footprint (at most `RUBBLE_LUMPS` cells of at
    // least `RUBBLE_CELL_M`); each rises with the mound's profile, highest over the module and
    // falling toward the berm's edge.
    let cell = RUBBLE_CELL_M.max((extent.x * extent.y / RUBBLE_LUMPS as f32).sqrt());
    let (nx, nz) = (
        ((extent.x / cell).round() as u32).max(1),
        ((extent.y / cell).round() as u32).max(1),
    );
    let (nx, nz) = if nx * nz > RUBBLE_LUMPS {
        let shrink = (RUBBLE_LUMPS as f32 / (nx * nz) as f32).sqrt();
        (((nx as f32 * shrink) as u32).max(1), ((nz as f32 * shrink) as u32).max(1))
    } else {
        (nx, nz)
    };
    let step = glam::Vec2::new(extent.x / nx as f32, extent.y / nz as f32);
    let mut lumps: Vec<(glam::Vec2, glam::Vec2, f32)> = Vec::with_capacity((nx * nz) as usize);
    for i in 0..nx {
        for j in 0..nz {
            let k = i * nz + j;
            let jitter = glam::Vec2::new(h(k, 1) - 0.5, h(k, 2) - 0.5) * step * 0.3;
            let centre = lo + step * glam::Vec2::new(i as f32 + 0.5, j as f32 + 0.5) + jitter;
            let (u, v) = ((centre.x - lo.x) / extent.x, (centre.y - lo.y) / extent.y);
            // A mound with a plateau over the module, falling to the berm's edge.
            let pyramid = (1.0 - (2.0 * u - 1.0).abs()).min(1.0 - (2.0 * v - 1.0).abs());
            let profile = (1.6 * pyramid).min(1.0);
            let half = step * glam::Vec2::new(0.40 + 0.10 * h(k, 3), 0.40 + 0.10 * h(k, 4));
            lumps.push((centre, half, profile.max(0.05) * (0.6 + 0.8 * h(k, 5))));
        }
    }
    // Scale the lump heights so their summed volume is the budget, under the crest.
    let unit: f32 = lumps.iter().map(|(_, half, top)| 4.0 * half.x * half.y * top).sum();
    let scale = if unit > 0.0 { budget / unit } else { 0.0 };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for (k, (centre, half, top)) in lumps.into_iter().enumerate() {
        let height = (top * scale).clamp(0.04, RUBBLE_CREST_M);
        let lump_min = Vec3::new(centre.x - half.x, -0.10, centre.y - half.y);
        let lump_max = Vec3::new(centre.x + half.x, height, centre.y + half.y);
        let offset = [h(k as u32, 6), h(k as u32, 7)];
        textured_lump(lump_min, lump_max, density, offset, &mut vertices, &mut indices);
        min = min.min(lump_min);
        max = max.max(lump_max);
    }
    Ok(CpuMesh::basic(vertices, indices, source.texture.clone(), min, max))
}

/// Own-colour glow of a visible light-fixture housing (the value the loose lamp meshes had).
pub const LIGHT_FIXTURE_GLOW: f32 = 2.2;

/// World AABB of a local box under `model`.
pub fn transformed_bounds(model: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut world_min = Vec3::splat(f32::INFINITY);
    let mut world_max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let point = model.transform_point3(Vec3::new(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        ));
        world_min = world_min.min(point);
        world_max = world_max.max(point);
    }
    (world_min, world_max)
}

/// Upload the plan: one renderer prototype per planned prototype, its pages with their initial
/// per-instance visibility in one packed load group, and the own-colour glow of light-fixture
/// housings. Returns the renderer batch handle of every planned page.
pub fn register(
    renderer: &mut crate::renderer::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    plan: &BuildingRenderPlan,
) -> anyhow::Result<Vec<usize>> {
    let mut batches = vec![usize::MAX; plan.pages.len()];
    let mut page = 0;
    for (index, prototype) in plan.prototypes.iter().enumerate() {
        let first = page;
        while page < plan.pages.len() && plan.pages[page].prototype == index {
            page += 1;
        }
        anyhow::ensure!(
            page > first,
            "building prototype {} ({:?}) has no page",
            prototype.asset_id,
            prototype.kind
        );
        let handle = renderer.register_instanced_prototype(device, queue, &prototype.mesh);
        let pages = &plan.pages[first..page];
        let transforms: Vec<&[Mat4]> = pages.iter().map(|p| p.transforms.as_slice()).collect();
        let hidden: Vec<&[bool]> = pages.iter().map(|p| p.hidden.as_slice()).collect();
        let handles = renderer
            .add_instanced_pages_with_hidden(device, handle, &transforms, &hidden)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "building prototype {} ({:?}) failed packed page registration",
                    prototype.asset_id,
                    prototype.kind
                )
            })?;
        anyhow::ensure!(handles.len() == pages.len(), "renderer returned the wrong page count");
        for (offset, batch) in handles.into_iter().enumerate() {
            batches[first + offset] = batch;
            if prototype.light_fixture {
                anyhow::ensure!(
                    renderer.set_batch_own_emissive(batch, LIGHT_FIXTURE_GLOW),
                    "light-fixture page {batch} refused its own-colour glow"
                );
            }
        }
    }
    anyhow::ensure!(page == plan.pages.len(), "building pages are not grouped by prototype");
    Ok(batches)
}

fn page_cell(anchor: Vec3) -> (i32, i32) {
    (
        (anchor.x / PAGE_METRES).floor() as i32,
        (anchor.z / PAGE_METRES).floor() as i32,
    )
}

/// Plan the prototypes and pages of every building instance of `set`, with the initial visibility
/// of its current state.
pub fn plan(set: &BuildingSet, cache: &mut KitMeshCache) -> anyhow::Result<BuildingRenderPlan> {
    // Prototype keys and page contents, both ordered for determinism.
    let mut prototype_keys: BTreeMap<(String, PrototypeKind), usize> = BTreeMap::new();
    struct Entry {
        key: (String, PrototypeKind),
        cell: (i32, i32),
        transform: Mat4,
        hidden: bool,
        owner: (u32, u32),
    }
    let mut entries: Vec<Entry> = Vec::new();
    // Per (instance, module): entry indices of its intact and rubble instance.
    let mut module_entries: Vec<Vec<(usize, Option<usize>)>> = Vec::with_capacity(set.len());
    for instance in 0..set.len() {
        let runtime = set.get(instance);
        let cell = page_cell(runtime.world_center());
        let states = runtime.render_snapshot().map_err(|error| {
            anyhow::anyhow!("building {}: render state: {error:?}", runtime.instance_id())
        })?;
        let placements = runtime.layout().placements();
        anyhow::ensure!(
            states.len() == placements.len(),
            "building {}: {} render states for {} placements",
            runtime.instance_id(),
            states.len(),
            placements.len()
        );
        let mut failed = vec![false; placements.len()];
        for state in &states {
            failed[state.placement_index] = state.state != ModuleRenderState::Intact;
        }
        let mut slots = Vec::with_capacity(placements.len());
        for (module, placement) in placements.iter().enumerate() {
            let asset = placement.asset_id.as_str().to_owned();
            let owner = (instance as u32, module as u32);
            let model = runtime.module_world(module);
            let intact = entries.len();
            entries.push(Entry {
                key: (asset.clone(), PrototypeKind::Module),
                cell,
                transform: model,
                hidden: failed[module],
                owner,
            });
            prototype_keys.insert((asset.clone(), PrototypeKind::Module), 0);
            let rubble = if runtime.module_proxies(module).is_empty() {
                None
            } else {
                let kind = PrototypeKind::Rubble {
                    variant: rubble_variant(runtime.instance_id(), module),
                };
                let rubble = entries.len();
                entries.push(Entry {
                    key: (asset.clone(), kind),
                    cell,
                    transform: grounded_transform(model, runtime.module_ground_y(module)),
                    hidden: !failed[module],
                    owner,
                });
                prototype_keys.insert((asset, kind), 0);
                Some(rubble)
            };
            slots.push((intact, rubble));
        }
        module_entries.push(slots);
    }

    let mut prototypes = Vec::with_capacity(prototype_keys.len());
    for (index, ((asset_id, kind), slot)) in prototype_keys.iter_mut().enumerate() {
        *slot = index;
        let mesh = match kind {
            PrototypeKind::Module => cache.mesh(asset_id)?.clone(),
            PrototypeKind::Rubble { variant } => {
                let source = cache.mesh(asset_id)?.clone();
                rubble_mesh(asset_id, *variant, &source)?
            }
        };
        prototypes.push(PlannedPrototype {
            asset_id: asset_id.clone(),
            kind: *kind,
            mesh,
            light_fixture: *kind == PrototypeKind::Module
                && crate::building_kit::shape(asset_id).is_some_and(|shape| shape.light.is_some()),
        });
    }

    // Mounds at an open cellar's edge keep off it (`rubble_clear_of_opening`), measured by their
    // own mesh's footprint.
    for entry in &mut entries {
        if !matches!(entry.key.1, PrototypeKind::Rubble { .. }) {
            continue;
        }
        let runtime = set.get(entry.owner.0 as usize);
        let Some(opening) = runtime.cellar_opening().filter(|_| runtime.cellar_is_open()) else {
            continue;
        };
        let mesh = &prototypes[prototype_keys[&entry.key]].mesh;
        let footprint = (Vec2::new(mesh.min.x, mesh.min.z), Vec2::new(mesh.max.x, mesh.max.z));
        entry.transform = match runtime.module_rubble_cell(entry.owner.1 as usize) {
            // Landed inside: within the room it fell into.
            Some(cell) => rubble_clear_of_opening(entry.transform, footprint, cell, true),
            None if (entry.transform.w_axis.y - runtime.world_center().y).abs() > 1e-4 => {
                rubble_clear_of_opening(entry.transform, footprint, (opening.min, opening.max), true)
            }
            None => rubble_clear_of_opening(
                entry.transform,
                footprint,
                (opening.min, opening.max),
                false,
            ),
        };
    }

    // Group entries into pages by (prototype, cell) in key order; remember each entry's slot.
    let mut grouped: BTreeMap<(usize, (i32, i32)), Vec<usize>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        grouped
            .entry((prototype_keys[&entry.key], entry.cell))
            .or_default()
            .push(index);
    }
    let mut entry_slot = vec![PageSlot { page: 0, index: 0 }; entries.len()];
    let mut pages = Vec::with_capacity(grouped.len());
    for ((prototype, cell), members) in grouped {
        let page = u32::try_from(pages.len())?;
        let mut planned = PlannedPage {
            prototype,
            cell,
            transforms: Vec::with_capacity(members.len()),
            hidden: Vec::with_capacity(members.len()),
            owners: Vec::with_capacity(members.len()),
        };
        for (index, member) in members.into_iter().enumerate() {
            let entry = &entries[member];
            entry_slot[member] = PageSlot { page, index: u32::try_from(index)? };
            planned.transforms.push(entry.transform);
            planned.hidden.push(entry.hidden);
            planned.owners.push(entry.owner);
        }
        pages.push(planned);
    }
    let modules = module_entries
        .into_iter()
        .map(|slots| {
            slots
                .into_iter()
                .map(|(intact, rubble)| ModuleSlots {
                    intact: entry_slot[intact],
                    rubble: rubble.map(|rubble| entry_slot[rubble]),
                })
                .collect()
        })
        .collect();
    Ok(BuildingRenderPlan { prototypes, pages, modules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_placement::{BuildingPlacement, BuildingSpec, Condition, Typology};
    use crate::building_runtime::{feature_blueprint, BuildingRuntime};
    use glam::Vec2;

    fn kit_installed() -> bool {
        let installed = crate::building_kit::KIT
            .iter()
            .all(|asset| std::path::Path::new(asset.path).is_file());
        if !installed {
            eprintln!("skipping: untracked building kit assets are not installed");
        }
        installed
    }

    fn house(id: &str, x: f32, z: f32, yaw_deg: f32, ground: f32) -> BuildingRuntime {
        let placement = BuildingPlacement {
            id: id.to_owned(),
            spec: BuildingSpec {
                typology: Typology::MerchantBrickHouse,
                seed: 11,
                width_m: 8.0,
                depth_m: 8.0,
                storeys: 2,
                cellar: false,
                condition: Condition::Intact,
            },
            x,
            z,
            yaw_deg,
        };
        BuildingRuntime::from_placement_with_condition(feature_blueprint(), &placement, ground, &[])
            .unwrap()
    }

    fn district() -> BuildingSet {
        let mut instances = vec![BuildingRuntime::featured(Vec2::new(-30.0, 110.0), 2.0).unwrap()];
        for (index, (x, z, yaw)) in
            [(40.0, 0.0, 0.0), (60.0, 0.0, 90.0), (40.0, 30.0, 180.0), (300.0, 30.0, 270.0)]
                .into_iter()
                .enumerate()
        {
            instances.push(house(&format!("urban_intro.render.b{index}"), x, z, yaw, 5.0 + index as f32));
        }
        BuildingSet::new(instances).unwrap()
    }

    #[test]
    fn grounded_rubble_bounds_keep_the_footprint_and_rest_on_the_ground() {
        let (min, max) = grounded_rubble_bounds(Vec3::new(-2.0, 7.0, -0.2), Vec3::new(2.0, 10.0, 0.2), 3.0);
        assert_eq!((min.x, min.z, max.x, max.z), (-2.0, -0.2, 2.0, 0.2));
        assert!((min.y - 2.9).abs() < 1e-6);
        assert!((max.y - (3.0 + 0.28 + 4.0 * 0.08)).abs() < 1e-6);
    }

    #[test]
    fn grounded_transform_keeps_the_horizontal_frame_and_stands_on_the_ground() {
        let module = Mat4::from_translation(Vec3::new(10.0, 6.4, -3.0))
            * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Mat4::from_scale(Vec3::new(0.985, 1.0, 1.0));
        let grounded = grounded_transform(module, 2.0);
        for local in [Vec3::new(1.0, 0.0, 0.5), Vec3::new(-2.0, 0.0, 0.1)] {
            let a = module.transform_point3(local);
            let b = grounded.transform_point3(local);
            assert!((a.x - b.x).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5);
            assert!((b.y - 2.0).abs() < 1e-5);
        }
        assert!((grounded.transform_point3(Vec3::new(0.0, 0.3, 0.0)).y - 2.3).abs() < 1e-5);
    }

    #[test]
    fn rubble_variants_are_deterministic_and_all_used() {
        let mut used = std::collections::BTreeSet::new();
        for module in 0..64 {
            let variant = rubble_variant("urban_intro.render.b0", module);
            assert_eq!(variant, rubble_variant("urban_intro.render.b0", module));
            assert!(variant < RUBBLE_VARIANTS);
            used.insert(variant);
        }
        assert_eq!(used.len() as u32, RUBBLE_VARIANTS);
    }

    /// Every module owns exactly one intact instance, every module with solids exactly one rubble
    /// instance, no two modules share one, and exactly one of the two is visible.
    #[test]
    fn every_module_has_one_intact_and_at_most_one_rubble_instance() {
        if !kit_installed() {
            return;
        }
        let set = district();
        let plan = plan(&set, &mut KitMeshCache::default()).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        let mut modules = 0;
        let mut rubble = 0;
        for (instance, slots) in plan.modules.iter().enumerate() {
            let runtime = set.get(instance);
            assert_eq!(slots.len(), runtime.layout().placements().len());
            for (module, slot) in slots.iter().enumerate() {
                modules += 1;
                assert!(seen.insert(slot.intact));
                let page = &plan.pages[slot.intact.page as usize];
                assert_eq!(page.owners[slot.intact.index as usize], (instance as u32, module as u32));
                assert_eq!(page.transforms[slot.intact.index as usize], runtime.module_world(module));
                assert!(!page.hidden[slot.intact.index as usize], "intact modules show");
                let prototype = &plan.prototypes[page.prototype];
                assert_eq!(prototype.kind, PrototypeKind::Module);
                assert_eq!(prototype.asset_id, runtime.layout().placements()[module].asset_id.as_str());
                assert_eq!(slot.rubble.is_some(), !runtime.module_proxies(module).is_empty());
                if let Some(rubble_slot) = slot.rubble {
                    rubble += 1;
                    assert!(seen.insert(rubble_slot));
                    let page = &plan.pages[rubble_slot.page as usize];
                    assert!(page.hidden[rubble_slot.index as usize], "rubble waits for failure");
                    assert!(matches!(plan.prototypes[page.prototype].kind, PrototypeKind::Rubble { .. }));
                }
            }
        }
        assert_eq!(plan.instance_count(), modules + rubble);
        assert_eq!(plan.visible_count(), modules, "exactly one instance per module shows");
        // Five featured houses: 36 modules each, of which 26 carry solids.
        assert_eq!(modules, 5 * 36);
        // One prototype per kit asset used plus its rubble variants; pages per (prototype, cell).
        let assets: std::collections::BTreeSet<_> =
            plan.prototypes.iter().map(|p| p.asset_id.clone()).collect();
        assert_eq!(
            plan.prototypes.iter().filter(|p| p.kind == PrototypeKind::Module).count(),
            assets.len()
        );
        assert!(plan.prototypes.iter().filter(|p| p.light_fixture).count() == 1);
    }

    /// Instancing reproduces the loose world-space meshes the runtime used to push: each vertex of
    /// the shared prototype under its instance transform lands where the old baked mesh had it.
    #[test]
    fn instances_reproduce_the_former_loose_module_meshes() {
        if !kit_installed() {
            return;
        }
        let set = district();
        let mut cache = KitMeshCache::default();
        let plan = plan(&set, &mut cache).unwrap();
        let mut worst = 0.0f32;
        for (instance, slots) in plan.modules.iter().enumerate() {
            let runtime = set.get(instance);
            let loose = crate::building_runtime::assemble_modules(runtime, &mut cache).unwrap();
            for (module, slot) in slots.iter().enumerate() {
                let page = &plan.pages[slot.intact.page as usize];
                let prototype = &plan.prototypes[page.prototype].mesh;
                let model = page.transforms[slot.intact.index as usize];
                let old = &loose[module].mesh;
                assert_eq!(prototype.vertices.len(), old.vertices.len());
                assert_eq!(prototype.indices, old.indices);
                for (source, baked) in prototype.vertices.iter().zip(&old.vertices) {
                    let placed = model.transform_point3(Vec3::from(source.pos));
                    worst = worst.max(placed.distance(Vec3::from(baked.pos)));
                }
            }
        }
        assert!(worst < 1.0e-4, "instanced vertices depart {worst} m from the loose meshes");
    }

    /// Rubble lies on each building's ground under the footprint of the solids it replaces.
    #[test]
    fn rubble_instances_lie_on_the_ground_under_their_module_solids() {
        if !kit_installed() {
            return;
        }
        let set = district();
        let plan = plan(&set, &mut KitMeshCache::default()).unwrap();
        for (instance, slots) in plan.modules.iter().enumerate() {
            let ground = set.ground_y(instance);
            let runtime = set.get(instance);
            for (module, slot) in slots.iter().enumerate() {
                let Some(rubble) = slot.rubble else { continue };
                let page = &plan.pages[rubble.page as usize];
                let mesh = &plan.prototypes[page.prototype].mesh;
                let model = page.transforms[rubble.index as usize];
                let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for vertex in &mesh.vertices {
                    let p = model.transform_point3(Vec3::from(vertex.pos));
                    min = min.min(p);
                    max = max.max(p);
                }
                // Lumps bed 0.10 m into the ground and rise at most the crest, both under the
                // module's vertical scale (the featured modules are unscaled vertically).
                let rise = model.y_axis.length();
                assert!((min.y - (ground - 0.10 * rise)).abs() < 1e-3, "mound bedded on the ground: {min:?}");
                assert!(max.y <= ground + RUBBLE_CREST_M * rise + 1e-3, "a mound under its crest: {max:?}");
                // Its centre stands over the module's solids, widened by the berm.
                let proxies = runtime.module_proxies(module);
                let (plo, phi) = proxies.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(lo, hi), (a, b)| (lo.min(*a), hi.max(*b)),
                );
                let center = (min + max) * 0.5;
                assert!(center.x >= plo.x - 1.5 && center.x <= phi.x + 1.5);
                assert!(center.z >= plo.z - 1.5 && center.z <= phi.z + 1.5);
            }
        }
    }

    /// Over an open cellar (cellars and scenes, 29 Sep 2026) no failed module's mound hangs over the
    /// excavation's edge: a perimeter module's mound lies clear of it on the pad, one inside lies
    /// within it, on the live floor over the cellar or on the cellar floor. The hosts are the
    /// district's doctor's cellar, burned household and cellar ward specs.
    #[test]
    fn rubble_mounds_keep_off_an_open_cellars_edge() {
        if !kit_installed() {
            return;
        }
        let mut runtimes = Vec::new();
        for (index, (typology, condition, width, depth, storeys, seed)) in [
            (Typology::MerchantBrickHouse, Condition::PartlyCollapsed, 16.0, 12.0, 2, 1_581_277_236),
            (Typology::MerchantBrickHouse, Condition::Burned, 16.0, 12.0, 2, 2_792_486_748),
            (Typology::ApartmentBlock, Condition::Shelled, 48.0, 12.0, 5, 3_818_150_729),
        ]
        .into_iter()
        .enumerate()
        {
            let placement = BuildingPlacement {
                id: format!("urban_intro.render.cellar{index}"),
                spec: BuildingSpec {
                    typology,
                    seed,
                    width_m: width,
                    depth_m: depth,
                    storeys,
                    cellar: true,
                    condition,
                },
                x: 100.0 * index as f32,
                z: 0.0,
                yaw_deg: 270.0,
            };
            let blueprint =
                crate::building_grammar::blueprint_for(&placement.spec, &placement.id).unwrap();
            let mut runtime = BuildingRuntime::from_placement(blueprint, &placement, 5.0).unwrap();
            runtime.open_cellar().unwrap();
            runtimes.push(runtime);
        }
        let set = BuildingSet::new(runtimes).unwrap();
        let plan = plan(&set, &mut KitMeshCache::default()).unwrap();
        let (mut outside, mut inside) = (0, 0);
        for (instance, slots) in plan.modules.iter().enumerate() {
            let runtime = set.get(instance);
            assert!(runtime.cellar_is_open(), "{}", runtime.instance_id());
            let opening = runtime.cellar_opening().unwrap();
            let (r0, r1) = (opening.min, opening.max);
            for (module, slot) in slots.iter().enumerate() {
                let Some(rubble) = slot.rubble else { continue };
                let page = &plan.pages[rubble.page as usize];
                let mesh = &plan.prototypes[page.prototype].mesh;
                let model = page.transforms[rubble.index as usize];
                let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for vertex in &mesh.vertices {
                    let p = model.transform_point3(Vec3::from(vertex.pos));
                    min = min.min(p);
                    max = max.max(p);
                }
                let id = runtime.instance_id();
                if (model.w_axis.y - runtime.world_center().y).abs() < 1e-4 {
                    let over = max.x > r0.x + 1e-3
                        && min.x < r1.x - 1e-3
                        && max.z > r0.y + 1e-3
                        && min.z < r1.y - 1e-3;
                    assert!(!over, "{id}: a mound on the pad hangs over the cellar: {min:?}..{max:?}");
                    outside += 1;
                } else {
                    // Within the room it fell into, never through its walls into the next.
                    let (c0, c1) = runtime.module_rubble_cell(module).unwrap_or((r0, r1));
                    let within = min.x >= c0.x - 1e-3
                        && max.x <= c1.x + 1e-3
                        && min.z >= c0.y - 1e-3
                        && max.z <= c1.y + 1e-3;
                    assert!(within, "{id}: a mound leaves its room: {min:?}..{max:?} vs {c0:?}..{c1:?}");
                    inside += 1;
                }
            }
        }
        assert!(outside > 0 && inside > 0, "{outside} outside, {inside} inside");
    }

    /// A failed module's mound holds its solids' volume times its material's bulking (P G17), in
    /// the asset frame: masonry and concrete about 1.5 times, sheet iron almost nothing.
    #[test]
    fn rubble_mounds_hold_the_failed_volume_times_the_bulking() {
        if !kit_installed() {
            return;
        }
        let mut cache = KitMeshCache::default();
        let mut checked = 0;
        for asset in crate::building_kit::KIT {
            let shape = crate::building_kit::shape(asset.id).unwrap();
            if shape.solids.is_empty() {
                continue;
            }
            let volume: f32 = shape
                .solids
                .iter()
                .map(|s| (s.max[0] - s.min[0]) * (s.max[1] - s.min[1]) * (s.max[2] - s.min[2]))
                .sum();
            let budget = volume * rubble_bulking(asset.material);
            let source = cache.mesh(asset.id).unwrap().clone();
            for variant in 0..RUBBLE_VARIANTS {
                let mound = rubble_mesh(asset.id, variant, &source).unwrap();
                let lumps = mound.vertices.len() / 24;
                assert!(lumps >= 1 && lumps <= RUBBLE_LUMPS as usize, "{}: {lumps} lumps", asset.id);
                assert!(mound.texture.is_some() == source.texture.is_some(), "{}: its own material", asset.id);
                // (volume held, volume of the 4 cm floor every lump keeps, a lump clamped)
                let (mut held, mut floor, mut clamped) = (0.0_f32, 0.0_f32, false);
                for lump in mound.vertices.chunks_exact(24) {
                    let (lo, hi) = lump.iter().fold(
                        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                        |(lo, hi), v| (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos))),
                    );
                    clamped |= hi.y >= RUBBLE_CREST_M - 1e-4 || hi.y <= 0.04 + 1e-4;
                    held += (hi.x - lo.x) * (hi.z - lo.z) * hi.y;
                    floor += (hi.x - lo.x) * (hi.z - lo.z) * 0.04;
                }
                if !clamped {
                    assert!((held - budget).abs() <= 0.02 * budget + 1e-4, "{}: {held} m3 for {budget}", asset.id);
                    checked += 1;
                }
                if asset.material == crate::building_kit::KitMaterial::Metal {
                    assert!(
                        held <= 0.1 * volume + floor + 1e-3,
                        "{}: sheet and steel leave little rubble ({held} m3 of {volume} m3)",
                        asset.id
                    );
                }
            }
        }
        assert!(checked > 20, "{checked} mounds checked");
    }

    /// The visible volume of a mound is the union of its lumps above the ground: the lumps
    /// overlap and are clamped to the crest and to a 4 cm floor, so what a player sees is not the
    /// summed budget. Measured exactly (lumps are axis-aligned boxes standing on y = -0.10, so the
    /// union above y = 0 is the integral of the tallest covering lump over the compressed XZ
    /// grid of lump edges) for every kit asset with solids and every variant, in the asset frame;
    /// a module's instance scales volume and budget alike, so the ratio holds in the world. The
    /// physical-truth ledger quotes these bounds.
    #[test]
    fn rubble_mound_visible_volume_is_measured_against_its_budget() {
        if !kit_installed() {
            return;
        }
        let mut cache = KitMeshCache::default();
        // (worst low, worst high) visible/budget over masonry, concrete and timber mounds, and the
        // worst high among mounds holding at least 1 m3 (walls, slabs, stairs, gables, wardrobes).
        let (mut low, mut high, mut high_large) = (f32::INFINITY, 0.0_f32, 0.0_f32);
        let mut measured = 0;
        for asset in crate::building_kit::KIT {
            let shape = crate::building_kit::shape(asset.id).unwrap();
            if shape.solids.is_empty() {
                continue;
            }
            let volume: f32 = shape
                .solids
                .iter()
                .map(|s| (s.max[0] - s.min[0]) * (s.max[1] - s.min[1]) * (s.max[2] - s.min[2]))
                .sum();
            let budget = volume * rubble_bulking(asset.material);
            let source = cache.mesh(asset.id).unwrap().clone();
            for variant in 0..RUBBLE_VARIANTS {
                let mound = rubble_mesh(asset.id, variant, &source).unwrap();
                let lumps: Vec<(Vec3, Vec3)> = mound
                    .vertices
                    .chunks_exact(24)
                    .map(|lump| {
                        lump.iter().fold(
                            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                            |(lo, hi), v| (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos))),
                        )
                    })
                    .collect();
                let mut xs: Vec<f32> = lumps.iter().flat_map(|(lo, hi)| [lo.x, hi.x]).collect();
                let mut zs: Vec<f32> = lumps.iter().flat_map(|(lo, hi)| [lo.z, hi.z]).collect();
                xs.sort_by(f32::total_cmp);
                zs.sort_by(f32::total_cmp);
                xs.dedup();
                zs.dedup();
                let (mut visible, mut summed) = (0.0_f64, 0.0_f64);
                for (lo, hi) in &lumps {
                    summed += f64::from((hi.x - lo.x) * (hi.z - lo.z) * hi.y.max(0.0));
                }
                for x in xs.windows(2) {
                    for z in zs.windows(2) {
                        let (cx, cz) = (0.5 * (x[0] + x[1]), 0.5 * (z[0] + z[1]));
                        let top = lumps
                            .iter()
                            .filter(|(lo, hi)| lo.x <= cx && cx <= hi.x && lo.z <= cz && cz <= hi.z)
                            .map(|(_, hi)| hi.y)
                            .fold(0.0_f32, f32::max);
                        visible += f64::from((x[1] - x[0]) * (z[1] - z[0]) * top);
                    }
                }
                assert!(
                    visible <= summed + 1e-6,
                    "{}: a union never exceeds its parts",
                    asset.id
                );
                if asset.material == crate::building_kit::KitMaterial::Metal {
                    continue; // sheet and steel keep only the 4 cm floor (tested above)
                }
                let ratio = (visible / f64::from(budget)) as f32;
                if variant == 0 {
                    println!(
                        "  {:<32} {:?} budget {:>7.3} m3 visible {:>7.3} m3 ({ratio:.3})",
                        asset.id, asset.material, budget, visible
                    );
                }
                low = low.min(ratio);
                high = high.max(ratio);
                if budget >= 1.0 {
                    high_large = high_large.max(ratio);
                }
                measured += 1;
            }
        }
        println!(
            "rubble mounds: visible volume {low:.3}-{high:.3} of the budget over {measured} mounds \
             ({high_large:.3} at most for budgets of 1 m3 and more)"
        );
        assert!(measured > 20, "{measured} mounds measured");
        // The ledger's quoted bounds (measured 2026-09-29 by the verifier): overlapping lumps keep
        // walls, slabs and stairs at 0.95-0.99 of their budget; the 4 cm floor every lump keeps over
        // the berm-widened footprint makes thin or light pieces show more than they held (a timber
        // board floor about 6 times, a crate 4.7, a shelf 3.4, a brick band 2.7, a gate leaf 2.4).
        assert!(low >= 0.94, "a mound shows {low:.3} of its failed volume");
        assert!(high <= 6.1, "a mound shows {high:.3} of its failed volume");
        assert!(
            high_large <= 1.10,
            "a mound of 1 m3 or more shows {high_large:.3} of it"
        );
    }

    #[test]
    fn plans_are_deterministic_and_page_by_prototype_and_cell() {
        if !kit_installed() {
            return;
        }
        let set = district();
        let a = plan(&set, &mut KitMeshCache::default()).unwrap();
        let b = plan(&set, &mut KitMeshCache::default()).unwrap();
        assert_eq!(a.modules, b.modules);
        assert_eq!(a.pages.len(), b.pages.len());
        for (pa, pb) in a.pages.iter().zip(&b.pages) {
            assert_eq!((pa.prototype, pa.cell, &pa.owners, &pa.hidden), (pb.prototype, pb.cell, &pb.owners, &pb.hidden));
            assert_eq!(pa.transforms, pb.transforms);
        }
        // Pages are unique per (prototype, cell) and hold buildings of that cell only.
        let keys: std::collections::BTreeSet<_> = a.pages.iter().map(|p| (p.prototype, p.cell)).collect();
        assert_eq!(keys.len(), a.pages.len());
        for page in &a.pages {
            for &(instance, _) in &page.owners {
                assert_eq!(page_cell(set.get(instance as usize).world_center()), page.cell);
            }
        }
        // The house at x = 300 sits in its own cell: its modules page apart from the others.
        let far = set.index_of("urban_intro.render.b3").unwrap();
        let far_cell = page_cell(set.get(far).world_center());
        assert!(a.pages.iter().any(|p| p.cell == far_cell));
        assert!(a.pages.iter().filter(|p| p.cell == far_cell).all(|p| p.owners.iter().all(|o| o.0 as usize == far)));
    }

    /// A building instantiated with failed modules starts with their rubble showing.
    #[test]
    fn initial_visibility_follows_the_instantiated_state() {
        if !kit_installed() {
            return;
        }
        let mut set = district();
        let b1 = set.index_of("urban_intro.render.b1").unwrap();
        let stair = set
            .get(b1)
            .layout()
            .placement(&crate::building_blueprint::SemanticId::new("module.stair.main").unwrap())
            .unwrap()
            .instance_index;
        set.get_mut(b1).apply_module_damage(stair, 10_000.0).unwrap();
        let plan = plan(&set, &mut KitMeshCache::default()).unwrap();
        let slot = plan.modules[b1][stair];
        assert!(plan.pages[slot.intact.page as usize].hidden[slot.intact.index as usize]);
        let rubble = slot.rubble.unwrap();
        assert!(!plan.pages[rubble.page as usize].hidden[rubble.index as usize]);
        // One instance per module shows, except a failed module without solids (it leaves none).
        let mut expected = 0;
        for instance in 0..set.len() {
            let runtime = set.get(instance);
            for state in runtime.render_snapshot().unwrap() {
                let module = state.placement_index;
                if state.state == ModuleRenderState::Intact
                    || !runtime.module_proxies(module).is_empty()
                {
                    expected += 1;
                }
            }
        }
        assert_eq!(plan.visible_count(), expected);
        assert!(expected < 5 * 36 || set.get(b1).render_snapshot().unwrap().iter()
            .filter(|s| s.state != ModuleRenderState::Intact).all(|s| !set.get(b1).module_proxies(s.placement_index).is_empty()));
    }
}
