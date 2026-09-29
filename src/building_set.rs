//! Every building instance of an act, as `Game` holds them.
//!
//! The set keeps the instances in instance-id order (the order LAN and saves agree on), maps each
//! of `Game`'s loose destructible records to its (instance, module), answers the world-space
//! physical queries across instances, routes door keys, and persists per-instance state keyed by
//! (act, instance id). It has no renderer dependency, so everything here is unit-tested.
//!
//! Broad phase: each query visits only instances whose world box (`BuildingRuntime::world_bounds`)
//! is near the query. That is a linear scan over instances, not a spatial grid; a district of a few
//! hundred buildings costs a few hundred box tests per query.

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};

use crate::building_blueprint::{SemanticId, StateError, StructuralRenderDelta};
use crate::building_runtime::{BuildingLight, BuildingRuntime, ModuleBinding};

/// Margin added around an instance's world box for queries that move a soldier (radius and step).
const QUERY_MARGIN_M: f32 = 1.0;
/// Publishes a changed instance stays hot, i.e. goes out in every publish before the rotating
/// rest: 0.3 s at 20 publishes per second.
const NET_HOT_PUBLISHES: u8 = 6;

/// FNV-1a 32 of an instance id: the LAN check that a record addresses the same building.
pub fn instance_key(instance_id: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in instance_id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Result of applying one host building sync on a client.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct NetApply {
    /// Instances whose state changed; `Game` reconciles their bindings quietly.
    pub changed: Vec<usize>,
    /// Refused records and mismatches, each reported once.
    pub errors: Vec<String>,
}

#[derive(Default)]
pub struct BuildingSet {
    instances: Vec<BuildingRuntime>,
    /// Destructible index → (instance, module).
    owners: Vec<Option<(u32, u32)>>,
    /// Host: last revision seen by `net_sync`, publishes left hot, and (revision, differs from
    /// baseline, record) caches so an unchanged instance costs nothing per publish.
    net_last_revision: Vec<u32>,
    net_hot: Vec<u8>,
    net_cache: Vec<Option<(u32, bool, Option<crate::lan::NetBuildingRecord>)>>,
    /// Client: (host revision, local revision right after applying it) per instance.
    net_applied: Vec<Option<(u32, u32)>>,
    net_reported: BTreeSet<String>,
}

impl BuildingSet {
    /// Take ownership of the act's instances. Instance ids must be unique; the set orders the
    /// instances by id so that every peer and every save addresses them identically.
    pub fn new(mut instances: Vec<BuildingRuntime>) -> anyhow::Result<Self> {
        instances.sort_by(|a, b| a.instance_id().cmp(b.instance_id()));
        let mut ids = BTreeSet::new();
        for instance in &instances {
            anyhow::ensure!(
                ids.insert(instance.instance_id().to_owned()),
                "building instance id {:?} is used twice in one act",
                instance.instance_id()
            );
        }
        let count = instances.len();
        Ok(Self {
            instances,
            owners: Vec::new(),
            net_last_revision: vec![0; count],
            net_hot: vec![0; count],
            net_cache: vec![None; count],
            net_applied: vec![None; count],
            net_reported: BTreeSet::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    pub fn instances(&self) -> &[BuildingRuntime] {
        &self.instances
    }

    pub fn get(&self, instance: usize) -> &BuildingRuntime {
        &self.instances[instance]
    }

    pub fn get_mut(&mut self, instance: usize) -> &mut BuildingRuntime {
        &mut self.instances[instance]
    }

    pub fn index_of(&self, instance_id: &str) -> Option<usize> {
        self.instances
            .binary_search_by(|instance| instance.instance_id().cmp(instance_id))
            .ok()
    }

    /// The featured house, when the act has one.
    pub fn featured(&self) -> Option<&BuildingRuntime> {
        self.index_of(crate::building_runtime::FEATURE_INSTANCE_ID)
            .map(|index| &self.instances[index])
    }

    // ── Destructible ownership ───────────────────────────────────────────────────────────────

    /// Register one of `Game`'s destructible records as a proxy of (instance, module).
    pub fn bind_destructible(
        &mut self,
        instance: usize,
        module: usize,
        destructible_index: usize,
        rubble_slot: usize,
        occluder_index: usize,
        min: Vec3,
        max: Vec3,
    ) {
        self.instances[instance].bind_destructible(
            module,
            destructible_index,
            rubble_slot,
            occluder_index,
            min,
            max,
        );
        if self.owners.len() <= destructible_index {
            self.owners.resize(destructible_index + 1, None);
        }
        self.owners[destructible_index] = Some((instance as u32, module as u32));
    }

    /// (instance, module) of a destructible record, when it belongs to a building.
    pub fn owner(&self, destructible_index: usize) -> Option<(usize, usize)> {
        self.owners
            .get(destructible_index)
            .copied()
            .flatten()
            .map(|(instance, module)| (instance as usize, module as usize))
    }

    pub fn bindings(&self, instance: usize, module: usize) -> Vec<ModuleBinding> {
        self.instances[instance].bindings(module)
    }

    pub fn destructible_is_crush_barrier(&self, destructible_index: usize) -> bool {
        self.owner(destructible_index).is_some_and(|(instance, _)| {
            self.instances[instance].destructible_is_crush_barrier(destructible_index)
        })
    }

    pub fn integrity_for_destructible(&self, destructible_index: usize) -> Option<(u16, u16)> {
        let (instance, _) = self.owner(destructible_index)?;
        self.instances[instance].integrity_for_destructible(destructible_index)
    }

    /// Damage the module behind a destructible record through its instance's `BuildingState`.
    pub fn apply_damage_to_destructible(
        &mut self,
        destructible_index: usize,
        damage: f32,
    ) -> Option<(usize, Result<StructuralRenderDelta, StateError>)> {
        let (instance, module) = self.owner(destructible_index)?;
        Some((
            instance,
            self.instances[instance].apply_module_damage(module, damage),
        ))
    }

    /// Ground height the rubble and collapse effects of an instance fall to.
    pub fn ground_y(&self, instance: usize) -> f32 {
        self.instances[instance].world_center().y
    }

    // ── Queries across instances ─────────────────────────────────────────────────────────────

    fn near_xz(&self, instance: usize, min: Vec2, max: Vec2, margin: f32) -> bool {
        let (lo, hi) = self.instances[instance].world_bounds();
        max.x >= lo.x - margin && min.x <= hi.x + margin && max.y >= lo.z - margin && min.y <= hi.z + margin
    }

    fn near_point(&self, instance: usize, point: Vec3, margin: f32) -> bool {
        let xz = Vec2::new(point.x, point.z);
        self.near_xz(instance, xz, xz, margin)
    }

    /// Resolve one on-foot move against every nearby instance's live walls and stair corridors.
    pub fn resolve_walk_xz(&self, old: Vec3, attempted: Vec3) -> Vec3 {
        let min = Vec2::new(old.x.min(attempted.x), old.z.min(attempted.z));
        let max = Vec2::new(old.x.max(attempted.x), old.z.max(attempted.z));
        let mut resolved = attempted;
        for (index, instance) in self.instances.iter().enumerate() {
            if self.near_xz(index, min, max, QUERY_MARGIN_M) {
                resolved = instance.resolve_walk_xz(old, resolved);
            }
        }
        resolved
    }

    /// Feet surface of the instance whose walk envelope holds the point; `None` outside every
    /// building (the terrain carries the feet) and where that building has no support left.
    pub fn walkable_surface_y(&self, world_pos: Vec3, previous_feet_y: f32) -> Option<f32> {
        self.instances
            .iter()
            .enumerate()
            .filter(|(index, _)| self.near_point(*index, world_pos, 0.0))
            .find(|(_, instance)| instance.contains_walk_point(world_pos))
            .and_then(|(_, instance)| instance.walkable_surface_y(world_pos, previous_feet_y))
    }

    pub fn unsupported_upper_position(&self, world_pos: Vec3) -> bool {
        self.instances.iter().enumerate().any(|(index, instance)| {
            self.near_point(index, world_pos, 0.0) && instance.unsupported_upper_position(world_pos)
        })
    }

    pub fn shelter_at(&self, world_pos: Vec3) -> f32 {
        self.instances
            .iter()
            .enumerate()
            .filter(|(index, _)| self.near_point(*index, world_pos, 0.0))
            .map(|(_, instance)| instance.shelter_at(world_pos))
            .fold(0.0, f32::max)
    }

    /// Wall cover of the instances near a sight line from `from` to `to`, for the infantry model.
    pub fn append_infantry_cover(&self, cover: &mut Vec<(Vec2, f32)>, from: Vec3, to: Vec3) {
        let min = Vec2::new(from.x.min(to.x), from.z.min(to.z));
        let max = Vec2::new(from.x.max(to.x), from.z.max(to.z));
        for (index, instance) in self.instances.iter().enumerate() {
            if self.near_xz(index, min, max, QUERY_MARGIN_M) {
                instance.append_infantry_cover(cover, from.y, to.y);
            }
        }
    }

    pub fn interior_lights(&self) -> Vec<BuildingLight> {
        self.instances
            .iter()
            .flat_map(BuildingRuntime::interior_lights)
            .collect()
    }

    /// The first container, over nearby instances, the soldier can search from here.
    pub fn searchable_container(
        &self,
        feet: Vec3,
        eye: Vec3,
        view_dir: Vec3,
    ) -> Option<(usize, SemanticId)> {
        self.instances
            .iter()
            .enumerate()
            .filter(|(index, _)| self.near_point(*index, feet, QUERY_MARGIN_M))
            .find_map(|(index, instance)| {
                instance
                    .searchable_container(feet, eye, view_dir)
                    .map(|container| (index, container))
            })
    }

    // ── Doors ────────────────────────────────────────────────────────────────────────────────

    pub fn door_frames(&self) -> Vec<crate::world_doors::Frame> {
        self.instances
            .iter()
            .flat_map(BuildingRuntime::door_frames)
            .collect()
    }

    /// (door key, open, alive, locked) for every door of every instance.
    pub fn physical_door_states(&self) -> Vec<(String, bool, bool, bool)> {
        self.instances
            .iter()
            .flat_map(BuildingRuntime::physical_door_states)
            .collect()
    }

    /// Route a door key (`<instance id>/<door id>`) to its instance.
    pub fn set_physical_door_open(&mut self, key: &str, open: bool) {
        let Some((instance_id, _)) = key.split_once('/') else { return };
        if let Some(index) = self.index_of(instance_id) {
            self.instances[index].set_physical_door_open(key, open);
        }
    }

    // ── Campaign persistence, keyed by (act, instance id) ────────────────────────────────────

    /// Replace every instance's state with its saved snapshot, when the checkpoint has one. Import
    /// is identity, layout and support checked per instance; any failure rejects the act.
    pub fn import_campaign(
        &mut self,
        act_id: u8,
        persistent: &crate::campaign::PersistentSnapshot,
    ) -> anyhow::Result<usize> {
        let mut imported = 0;
        for instance in &mut self.instances {
            if let Some(saved) = persistent.building(act_id, instance.instance_id()) {
                instance.replace_from_snapshot(act_id, saved).map_err(|error| {
                    anyhow::anyhow!("building {}: {error}", instance.instance_id())
                })?;
                imported += 1;
            }
        }
        Ok(imported)
    }

    /// Write the state of every instance the player has changed into the checkpoint, and remove the
    /// entry of any instance that is back at its instantiation baseline.
    pub fn export_campaign(&self, act_id: u8, out: &mut crate::campaign::PersistentSnapshot) {
        for instance in &self.instances {
            if instance.differs_from_baseline() {
                out.upsert_building(instance.export_snapshot(act_id));
            } else {
                out.remove_building(act_id, instance.instance_id());
            }
        }
    }

    // ── LAN: per-instance sync of changed state only ─────────────────────────────────────────

    /// Host: the building state of one publish. Only instances that differ from their baseline
    /// are listed and carry a record; instances changed within the last `NET_HOT_PUBLISHES`
    /// publishes come first so every publish carries them.
    pub fn net_sync(&mut self, act: u8) -> crate::lan::NetBuildingSync {
        let mut hot = Vec::new();
        let mut cold = Vec::new();
        let mut touched = Vec::new();
        for (index, instance) in self.instances.iter().enumerate() {
            let revision = instance.revision();
            if revision != self.net_last_revision[index] {
                self.net_last_revision[index] = revision;
                self.net_hot[index] = NET_HOT_PUBLISHES;
            }
            let is_hot = self.net_hot[index] > 0;
            self.net_hot[index] = self.net_hot[index].saturating_sub(1);
            let cached = match &self.net_cache[index] {
                Some((cached_revision, differs, record)) if *cached_revision == revision => {
                    (*differs, record.clone())
                }
                _ => {
                    let differs = instance.differs_from_baseline();
                    let record = differs.then(|| net_record(index, instance));
                    self.net_cache[index] = Some((revision, differs, record.clone()));
                    (differs, record)
                }
            };
            let (differs, record) = cached;
            if !differs {
                continue;
            }
            touched.push(u16::try_from(index).unwrap_or(u16::MAX));
            let record = record.expect("a changed instance has a record");
            if is_hot {
                hot.push(record);
            } else {
                cold.push(record);
            }
        }
        crate::lan::NetBuildingSync {
            act,
            instance_count: u16::try_from(self.instances.len()).unwrap_or(u16::MAX),
            touched,
            hot: hot.len(),
            records: hot.into_iter().chain(cold).collect(),
        }
    }

    /// Client: install the host's building state. Every record newer than the last one applied to
    /// its instance (or arriving after a local, predicted change) is rebuilt into a full snapshot
    /// against the instance's own baseline and imported through the support-checked
    /// `replace_from_snapshot`; every instance the host does not list returns to its baseline.
    pub fn apply_net_sync(&mut self, act: u8, sync: &crate::lan::NetBuildingSync) -> NetApply {
        let mut apply = NetApply::default();
        if usize::from(sync.instance_count) != self.instances.len() {
            self.report(
                &mut apply,
                format!(
                    "host replicates {} buildings in act {}, this scene has {}",
                    sync.instance_count,
                    sync.act,
                    self.instances.len()
                ),
            );
            return apply;
        }
        let mut changed = BTreeSet::new();
        for record in &sync.records {
            let index = usize::from(record.instance);
            let Some(instance) = self.instances.get_mut(index) else { continue };
            let newer = self.net_applied[index].is_none_or(|(host, local)| {
                (record.revision.wrapping_sub(host) as i32) > 0 || instance.revision() != local
            });
            if !newer {
                continue;
            }
            let result = snapshot_from_record(instance, act, record)
                .and_then(|snapshot| instance.replace_from_snapshot(act, &snapshot));
            match result {
                Ok(()) => {
                    self.net_applied[index] = Some((record.revision, instance.revision()));
                    changed.insert(index);
                }
                Err(error) => {
                    let message = format!("building {}: {error}", instance.instance_id());
                    self.report(&mut apply, message);
                }
            }
        }
        for index in 0..self.instances.len() {
            if sync.touched.binary_search(&(index as u16)).is_ok() {
                continue;
            }
            let instance = &mut self.instances[index];
            if instance.differs_from_baseline() {
                let baseline = instance.baseline_snapshot(act);
                match instance.replace_from_snapshot(act, &baseline) {
                    Ok(()) => {
                        self.net_applied[index] = None;
                        changed.insert(index);
                    }
                    Err(error) => {
                        let message = format!("building {} baseline: {error}", instance.instance_id());
                        self.report(&mut apply, message);
                    }
                }
            }
        }
        apply.changed = changed.into_iter().collect();
        apply
    }

    fn report(&mut self, apply: &mut NetApply, message: String) {
        if self.net_reported.insert(message.clone()) {
            apply.errors.push(message);
        }
    }
}

/// The LAN record of one changed instance, relative to its baseline.
fn net_record(index: usize, instance: &BuildingRuntime) -> crate::lan::NetBuildingRecord {
    let current = instance.export_snapshot(0);
    let baseline = instance.baseline_snapshot(0);
    let modules = current.module_integrity.len();
    let mut failed = vec![0u8; modules.div_ceil(8)];
    let mut partial = Vec::new();
    for (module, (&now, &base)) in current
        .module_integrity
        .iter()
        .zip(&baseline.module_integrity)
        .enumerate()
    {
        if now == 0 {
            failed[module / 8] |= 1 << (module % 8);
        } else if now != base || current.fractional_damage[module] != 0 {
            partial.push((
                u16::try_from(module).unwrap_or(u16::MAX),
                now,
                current.fractional_damage[module],
            ));
        }
    }
    crate::lan::NetBuildingRecord {
        instance: u16::try_from(index).unwrap_or(u16::MAX),
        instance_key: instance_key(instance.instance_id()),
        layout_hash: instance.layout().layout_hash(),
        baseline_hash: instance.baseline_hash(),
        revision: instance.revision(),
        module_count: u16::try_from(modules).unwrap_or(u16::MAX),
        failed,
        partial,
        doors: current.door_states,
        containers: current.container_states,
    }
}

/// Rebuild a full snapshot from a LAN record on the client's own baseline, after checking that
/// the record addresses this very instance, layout and condition.
fn snapshot_from_record(
    instance: &BuildingRuntime,
    act: u8,
    record: &crate::lan::NetBuildingRecord,
) -> anyhow::Result<crate::campaign::PersistentBuildingSnapshot> {
    anyhow::ensure!(
        record.instance_key == instance_key(instance.instance_id()),
        "record addresses another instance"
    );
    anyhow::ensure!(
        record.layout_hash == instance.layout().layout_hash(),
        "layout mismatch: host {:016x}, here {:016x}",
        record.layout_hash,
        instance.layout().layout_hash()
    );
    anyhow::ensure!(
        record.baseline_hash == instance.baseline_hash(),
        "condition baseline mismatch: host {:016x}, here {:016x}",
        record.baseline_hash,
        instance.baseline_hash()
    );
    let mut snapshot = instance.baseline_snapshot(act);
    let modules = snapshot.module_integrity.len();
    anyhow::ensure!(usize::from(record.module_count) == modules, "module count mismatch");
    anyhow::ensure!(
        record.doors.len() == snapshot.door_states.len()
            && record.containers.len() == snapshot.container_states.len(),
        "door or container count mismatch"
    );
    for module in 0..modules {
        if record.failed[module / 8] & (1 << (module % 8)) != 0 {
            snapshot.module_integrity[module] = 0;
        }
    }
    let max_integrity: Vec<u16> = instance
        .layout()
        .placements()
        .iter()
        .map(|placement| placement.max_integrity)
        .collect();
    for &(module, integrity, fraction) in &record.partial {
        let module = usize::from(module);
        anyhow::ensure!(
            module < modules && integrity <= max_integrity[module],
            "partial entry out of range"
        );
        snapshot.module_integrity[module] = integrity;
        snapshot.fractional_damage[module] = fraction;
    }
    snapshot.door_states = record.doors.clone();
    snapshot.container_states = record.containers.clone();
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building_placement::{BuildingPlacement, BuildingSpec, Condition, Typology};
    use crate::building_runtime::feature_blueprint;

    fn placement(id: &str, x: f32, z: f32, yaw_deg: f32) -> BuildingPlacement {
        BuildingPlacement {
            id: id.to_owned(),
            spec: BuildingSpec {
                typology: Typology::MerchantBrickHouse,
                seed: 7,
                width_m: 8.0,
                depth_m: 8.0,
                storeys: 2,
                cellar: false,
                condition: Condition::Intact,
            },
            x,
            z,
            yaw_deg,
        }
    }

    /// Three featured houses at different yaws plus the legacy featured instance.
    pub(crate) fn district() -> BuildingSet {
        let mut instances = vec![BuildingRuntime::featured(Vec2::new(-30.0, 110.0), 2.0).unwrap()];
        for (id, x, z, yaw) in [
            ("urban_intro.test.b1", 40.0, 0.0, 0.0),
            ("urban_intro.test.b2", 60.0, 0.0, 90.0),
            ("urban_intro.test.b3", 40.0, 30.0, 270.0),
        ] {
            instances.push(
                BuildingRuntime::from_placement(feature_blueprint(), &placement(id, x, z, yaw), 5.0)
                    .unwrap(),
            );
        }
        let mut set = BuildingSet::new(instances).unwrap();
        // Bind every proxy as Game does: one destructible record per derived solid.
        let mut destructible = 0;
        for instance in 0..set.len() {
            for module in 0..set.get(instance).layout().placements().len() {
                set.get_mut(instance).bind_intact_slot(module, module, Vec3::ZERO, Vec3::ONE);
                for (min, max) in set.get(instance).module_proxies(module).to_vec() {
                    set.bind_destructible(instance, module, destructible, 0, 0, min, max);
                    destructible += 1;
                }
            }
        }
        set
    }

    #[test]
    fn instances_are_ordered_by_id_and_ids_are_unique() {
        let set = district();
        let ids: Vec<_> = set.instances().iter().map(|b| b.instance_id().to_owned()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert_eq!(set.index_of("urban_intro.test.b2"), Some(ids.iter().position(|i| i == "urban_intro.test.b2").unwrap()));
        assert!(set.featured().is_some());
        let twice = vec![
            BuildingRuntime::from_placement(feature_blueprint(), &placement("urban_intro.test.x", 0.0, 0.0, 0.0), 0.0).unwrap(),
            BuildingRuntime::from_placement(feature_blueprint(), &placement("urban_intro.test.x", 20.0, 0.0, 0.0), 0.0).unwrap(),
        ];
        assert!(BuildingSet::new(twice).is_err());
    }

    /// Damage through one instance's destructible records fails that instance's modules only, and
    /// walking, support and shelter change in that instance only.
    #[test]
    fn damage_collapse_and_walking_are_per_instance() {
        let mut set = district();
        let b2 = set.index_of("urban_intro.test.b2").unwrap();
        let b1 = set.index_of("urban_intro.test.b1").unwrap();
        // Every destructible record resolves to its own instance and module.
        let owned: Vec<_> = (0..10_000).filter_map(|i| set.owner(i).map(|owner| (i, owner))).collect();
        assert!(owned.len() > 4 * 40);
        for &(destructible, (instance, module)) in &owned {
            assert!(set
                .bindings(instance, module)
                .iter()
                .any(|binding| binding.destructible_index == Some(destructible)));
        }
        // The stair of b2 fails through a destructible record of its stair module.
        let stair = set.get(b2).layout().placement(&SemanticId::new("module.stair.main").unwrap()).unwrap().instance_index;
        let (record, _) = owned.iter().find(|(_, owner)| *owner == (b2, stair)).unwrap();
        let (instance, delta) = set.apply_damage_to_destructible(*record, 1_000.0).unwrap();
        assert_eq!(instance, b2);
        assert!(!delta.unwrap().module_updates.is_empty());
        assert!(set.get(b2).differs_from_baseline());
        assert!(!set.get(b1).differs_from_baseline());
        // Walking the entry → landing route in the blueprint frame (entry floor at local y 0.4):
        // b1's stair still carries the feet to its landing, b2's does not.
        let climb = |set: &BuildingSet, index: usize| {
            let at = |z: f32| set.get(index).world_point(Vec3::new(-2.0, 0.4, z));
            let mut feet = at(-3.75);
            for step in 1..=92 {
                let target = at(-3.75 + step as f32 * 0.04);
                let attempted = Vec3::new(target.x, feet.y, target.z);
                let resolved = set.resolve_walk_xz(feet, attempted);
                match set.walkable_surface_y(resolved, feet.y) {
                    Some(surface) => feet = Vec3::new(resolved.x, surface, resolved.z),
                    None => return feet,
                }
            }
            feet
        };
        let top_b1 = climb(&set, b1);
        assert!((top_b1.y - (5.0 + 3.0)).abs() < 0.08, "b1 climbs to its landing: {top_b1:?}");
        let b3 = set.index_of("urban_intro.test.b3").unwrap();
        let top_b3 = climb(&set, b3);
        assert!((top_b3.y - (5.0 + 3.0)).abs() < 0.08, "the 270° house climbs too: {top_b3:?}");
        let top_b2 = climb(&set, b2);
        assert!(top_b2.y < 5.0 + 0.5, "b2's failed stair carries no one: {top_b2:?}");
        // Feet left on b2's former stair are unsupported; b1's stair still holds them.
        let on_b2_stair = set.get(b2).world_point(Vec3::new(-2.0, 1.9, -2.0));
        let on_b1_stair = set.get(b1).world_point(Vec3::new(-2.0, 1.9, -2.0));
        assert!(set.unsupported_upper_position(on_b2_stair));
        assert!(!set.unsupported_upper_position(on_b1_stair));
    }

    #[test]
    fn a_collapsed_roof_opens_its_own_building_to_the_weather_only() {
        let mut set = district();
        let b1 = set.index_of("urban_intro.test.b1").unwrap();
        let b3 = set.index_of("urban_intro.test.b3").unwrap();
        let roof = set.get(b1).layout().placement(&SemanticId::new("module.roof.north").unwrap()).unwrap().instance_index;
        set.get_mut(b1).apply_module_damage(roof, 1_000.0).unwrap();
        let upper = |set: &BuildingSet, index: usize| {
            set.get(index).world_point(Vec3::new(2.0, 3.4 + 1.7, 2.0))
        };
        assert_eq!(set.shelter_at(upper(&set, b1)), 0.0);
        assert_eq!(set.shelter_at(upper(&set, b3)), 1.0);
        assert_eq!(set.shelter_at(Vec3::new(500.0, 5.0, 500.0)), 0.0);
        // The lamps under that roof went out with it; the other instances keep theirs.
        assert_eq!(set.interior_lights().len(), 4 * 4 - 2);
    }

    #[test]
    fn door_keys_route_to_their_own_instance() {
        let mut set = district();
        let keys: Vec<_> = set.physical_door_states().into_iter().map(|state| state.0).collect();
        assert_eq!(keys.len(), 4 * 3);
        assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), keys.len(), "door keys are unique");
        assert_eq!(
            set.door_frames().iter().filter_map(|frame| frame.feature_id.clone()).collect::<Vec<_>>(),
            keys
        );
        let b3 = set.index_of("urban_intro.test.b3").unwrap();
        let key = format!("{}/door.ground.internal", set.get(b3).instance_id());
        set.set_physical_door_open(&key, false);
        let closed: Vec<_> = set.physical_door_states().into_iter().filter(|state| !state.1).map(|state| state.0).collect();
        assert_eq!(closed, vec![key]);
        assert!(set.get(b3).differs_from_baseline());
        assert_eq!(set.instances().iter().filter(|b| b.differs_from_baseline()).count(), 1);
    }

    /// A long single-storey hall of `cells` 4 m bays: foundations, perimeter walls and one exterior
    /// door. It is not the featured house: the runtime derives its floor, walls and door from its
    /// own layout. 60 bays give 182 modules, beyond the old 96-module LAN cap.
    pub(crate) fn hall_blueprint(cells: usize) -> crate::building_blueprint::BuildingBlueprint {
        use crate::building_blueprint::*;
        let id = |value: &str| SemanticId::new(value).unwrap();
        let at = |x: f32, z: f32, yaw: f32| LocalTransform {
            translation_m: [x, 0.0, z],
            rotation_degrees: [0.0, yaw, 0.0],
            scale: [1.0; 3],
        };
        let x_of = |cell: usize| (cell as f32 - (cells as f32 - 1.0) * 0.5) * 4.0;
        let module = |name: String, kind, asset: &str, transform, integrity| ModuleDef {
            id: id(&name),
            kind,
            floor_id: Some(id("floor.ground")),
            room_ids: vec![id("room.hall")],
            asset_id: id(asset),
            transform,
            max_integrity: integrity,
        };
        let mut modules = Vec::new();
        let mut support_edges = Vec::new();
        for cell in 0..cells {
            let x = x_of(cell);
            let foundation = format!("module.foundation.c{cell:03}");
            modules.push(module(foundation.clone(), ModuleKind::Foundation, "pizza.structure.floor", at(x, 0.0, 0.0), 200));
            modules.push(module(format!("module.wall.north.c{cell:03}"), ModuleKind::LoadBearingWall, "pizza.structure.wall", at(x, 2.0, 0.0), 120));
            support_edges.push(SupportEdge { supporter: id(&foundation), dependent: id(&format!("module.wall.north.c{cell:03}")) });
            if cell == 0 {
                modules.push(module("module.door".to_owned(), ModuleKind::DoorFrame, "pizza.structure.doorway", at(x, -2.0, 0.0), 90));
            } else {
                modules.push(module(format!("module.wall.south.c{cell:03}"), ModuleKind::LoadBearingWall, "pizza.structure.wall", at(x, -2.0, 0.0), 120));
                support_edges.push(SupportEdge { supporter: id(&foundation), dependent: id(&format!("module.wall.south.c{cell:03}")) });
            }
        }
        for (name, cell, dx) in [("module.wall.west", 0, -2.0), ("module.wall.east", cells - 1, 2.0)] {
            modules.push(module(name.to_owned(), ModuleKind::LoadBearingWall, "pizza.structure.wall", at(x_of(cell) + dx, 0.0, 90.0), 120));
            support_edges.push(SupportEdge { supporter: id(&format!("module.foundation.c{cell:03}")), dependent: id(name) });
        }
        let module_ids = modules.iter().map(|m| m.id.clone()).collect();
        BuildingBlueprint {
            id: id("building.test.hall"),
            revision: 1,
            seed: 7,
            floors: vec![FloorDef { id: id("floor.ground"), level: 0, elevation_m: 0.0, room_ids: vec![id("room.hall")] }],
            rooms: vec![RoomDef {
                id: id("room.hall"),
                floor_id: id("floor.ground"),
                tags: vec![id("tag.workshop")],
                module_ids,
                connector_ids: vec![id("connector.exterior")],
            }],
            modules,
            connectors: vec![ConnectorDef {
                id: id("connector.exterior"),
                kind: ConnectorKind::ExteriorExit,
                a: SpaceRef::Exterior,
                b: SpaceRef::Room(id("room.hall")),
                module_id: Some(id("module.door")),
                door_id: Some(id("door.exterior")),
                stair_id: None,
                transform: LocalTransform::default(),
            }],
            doors: vec![DoorDef {
                id: id("door.exterior"),
                connector_id: id("connector.exterior"),
                module_id: id("module.door"),
                initial_state: DoorInitialState::Open,
                key_item_id: None,
            }],
            stairs: Vec::new(),
            containers: Vec::new(),
            loot_sockets: Vec::new(),
            loot_tables: Vec::new(),
            support_edges,
            collapse_clusters: Vec::new(),
        }
    }

    fn hall(id: &str, x: f32, z: f32, yaw: f32) -> BuildingRuntime {
        let mut spec = placement(id, x, z, yaw);
        spec.spec.width_m = 240.0;
        spec.spec.depth_m = 4.0;
        BuildingRuntime::from_placement(hall_blueprint(60), &spec, 3.0).unwrap()
    }

    fn states(set: &BuildingSet) -> Vec<(Vec<u16>, Vec<u8>, Vec<u8>)> {
        set.instances()
            .iter()
            .map(|b| {
                let s = b.export_snapshot(1);
                (s.module_integrity, s.door_states, s.container_states)
            })
            .collect()
    }

    #[test]
    fn a_non_featured_blueprint_derives_its_own_floor_walls_and_door() {
        let building = hall("urban_intro.test.hall", 0.0, 0.0, 90.0);
        assert_eq!(building.layout().placements().len(), 182);
        // Turned a quarter: the 240 m hall runs along world z, its floor on the pad at 3 m.
        let inside = building.world_point(Vec3::new(37.0, 0.4, 0.3));
        assert!((building.walkable_surface_y(inside, 3.0).unwrap() - 3.0).abs() < 1e-4);
        assert!(building.walkable_surface_y(building.world_point(Vec3::new(0.0, 0.4, 3.0)), 3.0).is_none());
        // Walking out through the long wall is blocked; through the door it is not.
        let wall_start = building.world_point(Vec3::new(40.0, 0.4, -1.0));
        let wall_end = building.world_point(Vec3::new(40.0, 0.4, -3.0));
        assert!(building.resolve_walk_xz(wall_start, wall_end).distance(wall_end) > 1.0);
        let door_x = (0.0 - 59.0 * 0.5) * 4.0;
        let door_start = building.world_point(Vec3::new(door_x, 0.4, -1.0));
        let door_end = building.world_point(Vec3::new(door_x, 0.4, -3.0));
        assert!(building.resolve_walk_xz(door_start, door_end).distance(door_end) < 1e-3);
        // No roof, no upper storey: open to the weather, and nothing to fall from.
        assert_eq!(building.shelter_at(building.world_point(Vec3::new(0.0, 2.0, 0.0))), 0.0);
        assert!(!building.unsupported_upper_position(building.world_point(Vec3::new(0.0, 0.45, 0.0))));
        assert_eq!(building.door_frames().len(), 1);
    }

    /// Host damage, doors and searches reach the client through the real building packets; the
    /// client's state equals the host's in every instance, including a 182-module building.
    #[test]
    fn lan_sync_replicates_changed_instances_through_the_wire() {
        let with_hall = || {
            let mut instances: Vec<BuildingRuntime> = district().instances.drain(..).collect();
            instances.push(hall("urban_intro.test.hall", 200.0, 0.0, 0.0));
            BuildingSet::new(instances).unwrap()
        };
        let mut host = with_hall();
        let mut client = with_hall();
        // Nothing changed: an empty touched set and no records.
        let quiet = host.net_sync(1);
        assert!(quiet.touched.is_empty() && quiet.records.is_empty());
        assert_eq!(quiet.instance_count as usize, host.len());

        let b2 = host.index_of("urban_intro.test.b2").unwrap();
        let b3 = host.index_of("urban_intro.test.b3").unwrap();
        let hall_index = host.index_of("urban_intro.test.hall").unwrap();
        let stair = host.get(b2).layout().placement(&SemanticId::new("module.stair.main").unwrap()).unwrap().instance_index;
        host.get_mut(b2).apply_module_damage(stair, 1_000.0).unwrap();
        let door_key = format!("{}/door.ground.internal", host.get(b3).instance_id());
        host.set_physical_door_open(&door_key, false);
        let shelf = SemanticId::new("container.store.shelf").unwrap();
        assert!(!host.get_mut(b3).search_container(&shelf).unwrap().is_empty());
        // The hall: fail 40 walls outright and chip 70 more modules.
        for module in 0..host.get(hall_index).layout().placements().len() {
            let damage = if module % 4 == 1 { 1_000.0 } else if module % 2 == 0 { 7.5 } else { 0.0 };
            host.get_mut(hall_index).apply_module_damage(module, damage).unwrap();
        }
        let sync = host.net_sync(1);
        assert_eq!(sync.touched.len(), 3);
        assert_eq!(sync.hot, 3, "freshly changed instances are hot");
        let hall_record = sync.records.iter().find(|r| usize::from(r.instance) == hall_index).unwrap();
        assert_eq!(hall_record.module_count, 182);
        assert!(hall_record.partial.len() > 60 && hall_record.failed.iter().any(|b| *b != 0));
        let (received, refused) = crate::lan::building_wire_round_trip(&sync, 0);
        assert!(refused.is_empty());
        let received = received.unwrap();
        let applied = client.apply_net_sync(1, &received);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(applied.changed, vec![b2, b3, hall_index].into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>());
        // Equal state everywhere (failed modules' sub-point remainders are not replicated).
        assert_eq!(states(&client), states(&host));
        // Replaying the same sync changes nothing.
        assert!(client.apply_net_sync(1, &received).changed.is_empty());

        // A client-side misprediction on an instance the host never touched is undone...
        let b1 = client.index_of("urban_intro.test.b1").unwrap();
        let roof = client.get(b1).layout().placement(&SemanticId::new("module.roof.south").unwrap()).unwrap().instance_index;
        client.get_mut(b1).apply_module_damage(roof, 1_000.0).unwrap();
        // ...and one on a touched instance is replaced by the host's record of the same revision.
        let slab = client.get(b2).layout().placement(&SemanticId::new("module.upper.slab.nw").unwrap()).unwrap().instance_index;
        client.get_mut(b2).apply_module_damage(slab, 1_000.0).unwrap();
        let again = crate::lan::building_wire_round_trip(&host.net_sync(1), 0).0.unwrap();
        let applied = client.apply_net_sync(1, &again);
        assert_eq!(applied.changed, vec![b1, b2]);
        assert_eq!(states(&client), states(&host));

        // A host building back at its baseline resets on the client too.
        host.set_physical_door_open(&door_key, true);
        let mut fresh = with_hall();
        let only_door = crate::lan::building_wire_round_trip(&host.net_sync(1), 0).0.unwrap();
        assert_eq!(only_door.touched.len(), 3, "the searched shelf keeps b3 changed");
        fresh.apply_net_sync(1, &only_door);
        assert_eq!(states(&fresh), states(&host));
    }

    #[test]
    fn lan_sync_refuses_records_for_another_scene_and_reports_once() {
        let mut host = district();
        let mut client = district();
        let b1 = host.index_of("urban_intro.test.b1").unwrap();
        host.get_mut(b1).apply_module_damage(0, 1_000.0).unwrap();
        let mut sync = host.net_sync(1);
        sync.records[0].baseline_hash ^= 1;
        let first = client.apply_net_sync(1, &sync);
        assert!(first.changed.is_empty());
        assert_eq!(first.errors.len(), 1);
        assert!(first.errors[0].contains("baseline"), "{:?}", first.errors);
        assert!(client.apply_net_sync(1, &sync).errors.is_empty(), "reported once");
        assert!(!client.get(b1).differs_from_baseline());
        // A scene with a different instance count ignores the whole sync.
        let mut smaller = BuildingSet::new(vec![BuildingRuntime::featured(Vec2::ZERO, 0.0).unwrap()]).unwrap();
        let mismatch = smaller.apply_net_sync(1, &host.net_sync(1));
        assert!(mismatch.changed.is_empty() && mismatch.errors.len() == 1);
    }

    #[test]
    fn a_changed_instance_stays_hot_for_a_few_publishes_then_rotates() {
        let mut host = district();
        let b1 = host.index_of("urban_intro.test.b1").unwrap();
        host.get_mut(b1).apply_module_damage(3, 1_000.0).unwrap();
        let hot: Vec<_> = (0..(NET_HOT_PUBLISHES as usize + 2)).map(|_| host.net_sync(1).hot).collect();
        assert_eq!(&hot[..NET_HOT_PUBLISHES as usize], vec![1; NET_HOT_PUBLISHES as usize].as_slice());
        assert_eq!(&hot[NET_HOT_PUBLISHES as usize..], &[0, 0]);
        assert_eq!(host.net_sync(1).records.len(), 1, "a cold changed instance keeps its record");
    }

    #[test]
    fn campaign_state_is_keyed_by_act_and_instance_and_only_changed_instances_are_saved() {
        let mut set = district();
        let b2 = set.index_of("urban_intro.test.b2").unwrap();
        let slab = set.get(b2).layout().placement(&SemanticId::new("module.upper.slab.ne").unwrap()).unwrap().instance_index;
        set.get_mut(b2).apply_module_damage(slab, 1_000.0).unwrap();
        let mut out = crate::campaign::PersistentSnapshot {
            tiger: crate::campaign::TigerSnapshot::fresh(1),
            inventory: Default::default(),
            buildings: Vec::new(),
        };
        set.export_campaign(1, &mut out);
        assert_eq!(out.buildings.len(), 1);
        assert_eq!(out.buildings[0].instance_id, "urban_intro.test.b2");
        // A fresh act restores exactly that instance; the others stay at their baseline.
        let mut fresh = district();
        assert_eq!(fresh.import_campaign(1, &out).unwrap(), 1);
        assert_eq!(fresh.get(b2).export_snapshot(1), set.get(b2).export_snapshot(1));
        assert_eq!(fresh.instances().iter().filter(|b| b.differs_from_baseline()).count(), 1);
        // Another act's entries never apply here.
        let mut other_act = district();
        assert_eq!(other_act.import_campaign(2, &out).unwrap(), 0);
        // An instance back at its baseline removes its stale entry.
        let intact = district();
        intact.export_campaign(1, &mut out);
        assert!(out.buildings.is_empty());
    }
}
