//! Instanced rendering of every compiled building (urban intro F2, 2026-09-28).
//!
//! Each kit asset is uploaded once as a shared prototype in its source frame, and so is the
//! rubble mound of each kit asset (a few seeded variants, `building_rubble::MoundShape`). Every
//! building module is one instance of its asset's prototype at the module's world transform, and
//! every module with solids owns one instance of a rubble prototype. The structural render delta
//! switches the two per module (intact XOR rubble), so at any moment one of them is visible;
//! hidden instances occupy no TLAS slot (`Renderer::set_instance_visible`). Where a failed module's
//! mound lies is decided when it falls (`BuildingSet::settle_rubble`: on what is already there,
//! holding its volume), and its instance takes that transform (`Renderer::set_instance_matrix`).
//!
//! Pages group one prototype's instances of the buildings whose anchor lies in one 128 m cell, so
//! the renderer's spatial index culls a district cell by cell. The plan is renderer-independent
//! and deterministic: prototypes are ordered by (asset id, kind), pages by (prototype, cell), and
//! instances within a page by (building, module).
//!
//! Before this module, every module was a loose scene mesh with one pre-allocated rubble heap per
//! collision proxy: 103 meshes and bottom-level structures per featured house.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3, Vec4};

use crate::building_blueprint::ModuleRenderState;
use crate::building_runtime::KitMeshCache;
use crate::building_set::BuildingSet;
use crate::gltf_load::CpuMesh;

/// Horizontal page size. Matches the renderer's spatial index cell (`INST_SPATIAL_CELL_METRES`)
/// and the tight manifest page scale for mutable content.
pub const PAGE_METRES: f32 = 128.0;
/// Seeded rubble mound variants per kit asset; a module takes one by hashing (instance, module).
#[cfg(test)]
pub const RUBBLE_VARIANTS: u32 = crate::building_rubble::MOUND_VARIANTS;


#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrototypeKind {
    /// The kit asset's source mesh.
    Module,
    /// The rubble mound of the asset's solids, seeded variant.
    Rubble { variant: u32 },
}

pub struct PlannedPrototype {
    pub asset_id: String,
    pub kind: PrototypeKind,
    /// In the asset's source frame (rubble: the mound's surface above its base plane y = 0).
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
    /// instance while its module stands (or while its rubble lies buried in a sealed cellar).
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

/// The rubble transform of a module before it falls: the module's world transform with its heap
/// standing on the building's ground (hidden until the module fails and its mound is laid). Modules
/// with solids turn only about +Y (the runtime refuses others), so the module's X and Z axes are
/// horizontal; the heap keeps them and a vertical +Y axis.
pub fn grounded_transform(module_world: Mat4, ground_y: f32) -> Mat4 {
    let mut model = module_world;
    model.y_axis = Vec4::new(0.0, module_world.y_axis.length(), 0.0, 0.0);
    model.w_axis.y = ground_y;
    model
}

/// The rubble variant a module takes: a counter-seeded hash of its instance id and placement.
pub fn rubble_variant(instance_id: &str, module: usize) -> u32 {
    crate::building_rubble::mound_variant(instance_id, module)
}

/// The rubble prototype of one kit asset and variant: its mound's surface
/// (`building_rubble::MoundShape::mesh`), textured with the asset's own material at its own
/// texture scale. Tanks, feet and shells meet exactly these triangles (`BuildingSet::rubble_top`).
pub fn rubble_mesh(asset_id: &str, variant: u32, source: &CpuMesh) -> anyhow::Result<CpuMesh> {
    let mound = crate::building_rubble::mound(asset_id, variant)
        .ok_or_else(|| anyhow::anyhow!("kit asset {asset_id} has no solids to leave rubble"))?;
    Ok(mound.mesh(source, variant))
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
                // A damaged, sooted or snowed variant leaves its base bay's rubble: one set of
                // rubble prototypes per base, not per variant.
                let asset = crate::building_kit::base_of(&asset).to_owned();
                let rubble = entries.len();
                // A failed module's mound lies where the pile laid it; a standing module's waits,
                // hidden, on the building's ground.
                let laid = set.rubble_transform(instance, module);
                entries.push(Entry {
                    key: (asset.clone(), kind),
                    cell,
                    transform: laid.unwrap_or_else(|| grounded_transform(model, runtime.module_ground_y(module))),
                    hidden: !(failed[module] && laid.is_some()),
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

    /// A failed module's mound lies over the footprint of the solids it replaces, on the building's
    /// ground or on the rubble already there, and holds its volume above it (`building_rubble`).
    #[test]
    fn rubble_instances_lie_on_the_ground_under_their_module_solids() {
        if !kit_installed() {
            return;
        }
        let mut set = district();
        // Fail the stair and a wall of one house, then lay their rubble on its flat ground.
        let b1 = set.index_of("urban_intro.render.b1").unwrap();
        let ground = set.ground_y(b1);
        let placements = set.get(b1).layout().placements().to_vec();
        let failed: Vec<usize> = placements
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    crate::building_blueprint::ModuleKind::Stair
                        | crate::building_blueprint::ModuleKind::LoadBearingWall
                )
            })
            .map(|p| p.instance_index)
            .take(3)
            .collect();
        for &module in &failed {
            set.get_mut(b1).apply_module_damage(module, 60_000.0).unwrap();
        }
        let (laid, _) = set.settle_rubble(b1, &|_| ground);
        assert!(!laid.is_empty());
        let plan = plan(&set, &mut KitMeshCache::default()).unwrap();
        let runtime = set.get(b1);
        for &module in &laid {
            let slot = plan.modules[b1][module].rubble.expect("a module with solids has rubble");
            let page = &plan.pages[slot.page as usize];
            assert!(!page.hidden[slot.index as usize], "a laid mound shows");
            let model = page.transforms[slot.index as usize];
            assert_eq!(Some(model), set.rubble_transform(b1, module));
            // Its base plane lies on the ground or higher (on the rubble laid before it), within the
            // volume solve's grid discretization.
            assert!(model.w_axis.y >= ground - 0.02, "{module}: base {}", model.w_axis.y);
            // Its crest region stands over the module's solids.
            let proxies = runtime.module_proxies(module);
            let (plo, phi) = proxies.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(lo, hi), (a, b)| (lo.min(*a), hi.max(*b)),
            );
            let (core_lo, core_hi) = set.rubble_mound(b1, module).unwrap().core_rect();
            let centre = (core_lo + core_hi) * 0.5;
            assert!(centre.x >= plo.x - 0.5 && centre.x <= phi.x + 0.5, "{module}");
            assert!(centre.y >= plo.z - 0.5 && centre.y <= phi.z + 0.5, "{module}");
            // The mound added its module's volume times the bulking, measured on its grid.
            let mound = set.rubble_mound(b1, module).unwrap();
            let s = model.x_axis.truncate().length() * model.y_axis.y * model.z_axis.truncate().length();
            assert!(
                (mound.added_volume - mound.shape.budget * s).abs() <= 0.02 * mound.shape.budget * s + 1e-3,
                "{module}: {} of {}",
                mound.added_volume,
                mound.shape.budget * s
            );
        }
    }

    /// Every rubble prototype is its mound's surface: the drawn triangles above the base plane
    /// hold the asset's solid volume times its material's bulking (P G17), textured with the
    /// asset's own material; sheet iron and steel leave almost nothing.
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
            let budget = volume * crate::building_rubble::rubble_bulking(asset.material);
            let source = cache.mesh(asset.id).unwrap().clone();
            for variant in 0..RUBBLE_VARIANTS {
                let mesh = rubble_mesh(asset.id, variant, &source).unwrap();
                assert!(mesh.texture.is_some() == source.texture.is_some(), "{}: its own material", asset.id);
                let mound = crate::building_rubble::mound(asset.id, variant).unwrap();
                // The prototype's first vertices are the mound's grid, in order.
                for (k, h) in mound.heights.iter().enumerate() {
                    assert!((mesh.vertices[k].pos[1] - h).abs() < 1e-6, "{}: vertex {k}", asset.id);
                }
                let held = mound.volume_above(0.0);
                assert!((held - budget).abs() <= 1e-3 * budget.max(1.0), "{}: {held} m3 for {budget}", asset.id);
                if asset.material == crate::building_kit::KitMaterial::Metal {
                    assert!(held <= 0.06 * volume + 1e-4, "{}: sheet and steel leave little rubble", asset.id);
                }
                checked += 1;
            }
        }
        assert!(checked > 80, "{checked} mounds checked");
    }

    /// What a player sees of one mound on level ground is exactly what it holds: its surface above
    /// the base plane (the apron runs below the ground). Stacked mounds add exactly their budget
    /// above what lay there before (`building_rubble::tests`). Before 2026-09-29 the mounds were
    /// overlapping lumps with a 4 cm floor that showed 0.95-6.0 times their volume.
    #[test]
    fn rubble_mound_visible_volume_is_measured_against_its_budget() {
        if !kit_installed() {
            return;
        }
        let (mut low, mut high) = (f32::INFINITY, 0.0_f32);
        for asset in crate::building_kit::KIT {
            let shape = crate::building_kit::shape(asset.id).unwrap();
            if shape.solids.is_empty() || asset.material == crate::building_kit::KitMaterial::Metal {
                continue;
            }
            for variant in 0..RUBBLE_VARIANTS {
                let mound = crate::building_rubble::mound(asset.id, variant).unwrap();
                let ratio = mound.volume_above(0.0) / mound.budget;
                low = low.min(ratio);
                high = high.max(ratio);
            }
        }
        println!("rubble mounds on level ground: visible volume {low:.4}-{high:.4} of the budget");
        assert!(low >= 0.999 && high <= 1.001, "{low}-{high}");
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
        // The instantiated state's rubble is laid before the plan (Game::build does this too).
        let ground = set.ground_y(b1);
        set.init_rubble(&|_| ground);
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
