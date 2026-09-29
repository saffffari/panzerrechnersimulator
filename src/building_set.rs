//! Every building instance of an act, as `Game` holds them.
//!
//! The set keeps the instances in instance-id order (the order LAN and saves agree on), maps each
//! of `Game`'s loose destructible records to its (instance, module), answers the world-space
//! physical queries across instances, routes door keys, and persists per-instance state keyed by
//! (act, instance id). It has no renderer dependency, so everything here is unit-tested.
//!
//! Broad phase: a spatial grid over the instances' world boxes (`BuildingRuntime::world_bounds`)
//! hands each query the instances near it, in instance order, so a query's cost follows the
//! buildings around it rather than the act's building count (urban intro F2, 2026-09-28).

use std::collections::BTreeSet;

use glam::{Vec2, Vec3};

use crate::building_blueprint::{SemanticId, StateError, StructuralRenderDelta};
use crate::building_runtime::{BuildingLight, BuildingRuntime, ModuleBinding};

/// Margin added around an instance's world box for queries that move a soldier (radius and step).
const QUERY_MARGIN_M: f32 = 1.0;
/// Publishes a changed instance stays hot, i.e. goes out in every publish before the rotating
/// rest: 0.3 s at 20 publishes per second.
const NET_HOT_PUBLISHES: u8 = 6;
/// Cell of the instance grid: a street block of houses.
const INSTANCE_INDEX_CELL_M: f32 = 32.0;

/// Does the 2D segment `a → b` meet the closed rectangle `[lo, hi]`? (slab test)
fn segment_meets_rect(a: Vec2, b: Vec2, lo: Vec2, hi: Vec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for axis in 0..2 {
        if d[axis].abs() < 1e-9 {
            if a[axis] < lo[axis] || a[axis] > hi[axis] {
                return false;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[axis] - a[axis]) / d[axis], (hi[axis] - a[axis]) / d[axis]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// FNV-1a 32 of an instance id: the LAN check that a record addresses the same building.
pub fn instance_key(instance_id: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in instance_id.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// One building door and a world door frame that carries its key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorLink {
    pub instance: u32,
    pub ordinal: u32,
    pub world_door: u32,
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
    /// Grid over the instances' world boxes (key = instance index).
    instance_index: crate::spatial_grid::BoxIndex,
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
        let instance_index = crate::spatial_grid::BoxIndex::build(
            INSTANCE_INDEX_CELL_M,
            instances.iter().enumerate().map(|(index, instance)| {
                let (min, max) = instance.world_bounds();
                (index as u32, min, max, None)
            }),
        );
        Ok(Self {
            instances,
            instance_index,
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

    #[cfg_attr(not(test), allow(dead_code))]
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
        occluder_index: usize,
        min: Vec3,
        max: Vec3,
    ) {
        self.instances[instance].bind_destructible(
            module,
            destructible_index,
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

    /// Instances whose world box, widened by `margin`, overlaps the XZ rectangle, in instance order.
    fn near(&self, min: Vec2, max: Vec2, margin: f32) -> Vec<usize> {
        let mut keys = Vec::new();
        self.instance_index.keys_in_rect(min - Vec2::splat(margin), max + Vec2::splat(margin), &mut keys);
        keys.into_iter()
            .map(|key| key as usize)
            .filter(|&index| self.near_xz(index, min, max, margin))
            .collect()
    }

    fn near_point(&self, point: Vec3, margin: f32) -> Vec<usize> {
        let xz = Vec2::new(point.x, point.z);
        self.near(xz, xz, margin)
    }

    /// Resolve one on-foot move against every nearby instance's live walls and stair corridors.
    pub fn resolve_walk_xz(&self, old: Vec3, attempted: Vec3) -> Vec3 {
        let min = Vec2::new(old.x.min(attempted.x), old.z.min(attempted.z));
        let max = Vec2::new(old.x.max(attempted.x), old.z.max(attempted.z));
        let mut resolved = attempted;
        for index in self.near(min, max, QUERY_MARGIN_M) {
            resolved = self.instances[index].resolve_walk_xz(old, resolved);
        }
        resolved
    }

    /// Feet surface of the instance whose walk envelope holds the point; `None` outside every
    /// building (the terrain carries the feet) and where that building has no support left.
    pub fn walkable_surface_y(&self, world_pos: Vec3, previous_feet_y: f32) -> Option<f32> {
        self.near_point(world_pos, 0.0)
            .into_iter()
            .map(|index| &self.instances[index])
            .find(|instance| instance.contains_walk_point(world_pos))
            .and_then(|instance| instance.walkable_surface_y(world_pos, previous_feet_y))
    }

    pub fn unsupported_upper_position(&self, world_pos: Vec3) -> bool {
        self.near_point(world_pos, 0.0)
            .into_iter()
            .any(|index| self.instances[index].unsupported_upper_position(world_pos))
    }

    pub fn shelter_at(&self, world_pos: Vec3) -> f32 {
        self.near_point(world_pos, 0.0)
            .into_iter()
            .map(|index| self.instances[index].shelter_at(world_pos))
            .fold(0.0, f32::max)
    }

    /// Wall cover of the instances near a sight line from `from` to `to`, for the infantry model.
    /// The cover circles lie on an instance's walls, inside its world box; a sight line that does
    /// not pass within the query margin of that box cannot meet one, so only instances the line
    /// passes are appended.
    pub fn append_infantry_cover(&self, cover: &mut Vec<(Vec2, f32)>, from: Vec3, to: Vec3) {
        let min = Vec2::new(from.x.min(to.x), from.z.min(to.z));
        let max = Vec2::new(from.x.max(to.x), from.z.max(to.z));
        let (a, b) = (Vec2::new(from.x, from.z), Vec2::new(to.x, to.z));
        for index in self.near(min, max, QUERY_MARGIN_M) {
            let (lo, hi) = self.instances[index].world_bounds();
            let lo = Vec2::new(lo.x, lo.z) - Vec2::splat(QUERY_MARGIN_M);
            let hi = Vec2::new(hi.x, hi.z) + Vec2::splat(QUERY_MARGIN_M);
            if segment_meets_rect(a, b, lo, hi) {
                self.instances[index].append_infantry_cover(cover, from.y, to.y);
            }
        }
    }

    pub fn interior_lights(&self) -> Vec<BuildingLight> {
        let mut lights = Vec::new();
        for instance in &self.instances {
            instance.append_interior_lights(&mut lights);
        }
        lights
    }

    /// Link every building door to the world door frames carrying its key: (instance, door
    /// ordinal, world door index), sorted by instance and ordinal. `Game` syncs doors through these
    /// links every frame instead of formatting and parsing keys.
    pub fn door_links(&self, world_door: impl Fn(&str) -> Vec<usize>) -> Vec<DoorLink> {
        let mut links = Vec::new();
        for (instance, runtime) in self.instances.iter().enumerate() {
            for (ordinal, (key, ..)) in runtime.physical_door_states().into_iter().enumerate() {
                for door in world_door(&key) {
                    links.push(DoorLink {
                        instance: instance as u32,
                        ordinal: ordinal as u32,
                        world_door: door as u32,
                    });
                }
            }
        }
        links
    }

    /// The first container, over nearby instances, the soldier can search from here.
    pub fn searchable_container(
        &self,
        feet: Vec3,
        eye: Vec3,
        view_dir: Vec3,
    ) -> Option<(usize, SemanticId)> {
        self.near_point(feet, QUERY_MARGIN_M).into_iter().find_map(|index| {
            self.instances[index]
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn physical_door_states(&self) -> Vec<(String, bool, bool, bool)> {
        self.instances
            .iter()
            .flat_map(BuildingRuntime::physical_door_states)
            .collect()
    }

    /// Route a door key (`<instance id>/<door id>`) to its instance. (`Game` syncs doors through
    /// `door_links`.)
    #[cfg_attr(not(test), allow(dead_code))]
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
                    set.bind_destructible(instance, module, destructible, 0, min, max);
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

    /// Door links reach exactly the doors their keys name, and the ordinal accessors `Game` syncs
    /// through every frame answer and act as the key-based ones did.
    #[test]
    fn door_links_and_ordinal_access_match_the_keyed_door_api() {
        let mut keyed = district();
        let mut linked = district();
        let keys: Vec<String> = keyed.physical_door_states().into_iter().map(|state| state.0).collect();
        // A world door list holding every building door once, in reverse, plus an unlinked frame.
        let world: Vec<Option<String>> = std::iter::once(None)
            .chain(keys.iter().rev().cloned().map(Some))
            .collect();
        let links = linked.door_links(|key| {
            world.iter().enumerate().filter(|(_, id)| id.as_deref() == Some(key)).map(|(i, _)| i).collect()
        });
        assert_eq!(links.len(), keys.len());
        assert!(links.windows(2).all(|w| (w[0].instance, w[0].ordinal) < (w[1].instance, w[1].ordinal)));
        for link in &links {
            let runtime = linked.get(link.instance as usize);
            let state = &runtime.physical_door_states()[link.ordinal as usize];
            assert_eq!(world[link.world_door as usize].as_deref(), Some(state.0.as_str()));
            assert_eq!(runtime.door_physical_state(link.ordinal as usize), (state.1, state.2, state.3));
        }
        // Closing, reopening and a destroyed door frame behave the same through either path.
        let b3 = keyed.index_of("urban_intro.test.b3").unwrap();
        let door_module = keyed.get(b3).layout().placement(&SemanticId::new("module.ground.south.door").unwrap()).unwrap().instance_index;
        for runtime in [&mut keyed, &mut linked] {
            runtime.get_mut(b3).apply_module_damage(door_module, 10_000.0).unwrap();
        }
        for (step, open) in [(0, false), (1, true), (2, false)] {
            for link in &links {
                let key = &world[link.world_door as usize].clone().unwrap();
                let open = open ^ ((link.ordinal as usize + step) % 2 == 0);
                keyed.set_physical_door_open(key, open);
                linked
                    .get_mut(link.instance as usize)
                    .set_physical_door_open_at(link.ordinal as usize, open);
            }
            assert_eq!(keyed.physical_door_states(), linked.physical_door_states());
            let revisions = |set: &BuildingSet| set.instances().iter().map(|b| b.revision()).collect::<Vec<_>>();
            assert_eq!(revisions(&keyed), revisions(&linked));
        }
        assert!(keyed.physical_door_states().iter().any(|state| !state.2), "a destroyed door is dead");
    }

    /// A square district of `side × side` featured houses at a 20 m pitch (every fourth one with
    /// a failed stair and every seventh with a failed roof), so the density is constant and the
    /// district grows by area.
    pub(crate) fn grid_district(side: usize) -> BuildingSet {
        let mut instances = Vec::with_capacity(side * side);
        for index in 0..side * side {
            let (x, z) = ((index % side) as f32 * 20.0, (index / side) as f32 * 20.0);
            let yaw = [0.0, 90.0, 180.0, 270.0][index % 4];
            instances.push(
                BuildingRuntime::from_placement(
                    feature_blueprint(),
                    &placement(&format!("urban_intro.grid.h{index:05}"), x, z, yaw),
                    1.0 + (index % 5) as f32 * 0.25,
                )
                .unwrap(),
            );
        }
        let mut set = BuildingSet::new(instances).unwrap();
        for index in 0..set.len() {
            for (every, module) in [(4, "module.stair.main"), (7, "module.roof.south")] {
                if index % every == 0 {
                    let module = set.get(index).layout().placement(&SemanticId::new(module).unwrap()).unwrap().instance_index;
                    set.get_mut(index).apply_module_damage(module, 10_000.0).unwrap();
                }
            }
        }
        set
    }

    /// The queries as they were before the instance grid: every instance, in order, filtered by
    /// its widened world box.
    fn linear_resolve_walk(set: &BuildingSet, old: Vec3, attempted: Vec3) -> Vec3 {
        let min = Vec2::new(old.x.min(attempted.x), old.z.min(attempted.z));
        let max = Vec2::new(old.x.max(attempted.x), old.z.max(attempted.z));
        let mut resolved = attempted;
        for (index, instance) in set.instances.iter().enumerate() {
            if set.near_xz(index, min, max, QUERY_MARGIN_M) {
                resolved = instance.resolve_walk_xz(old, resolved);
            }
        }
        resolved
    }

    fn linear_surface(set: &BuildingSet, pos: Vec3, previous: f32) -> Option<f32> {
        set.instances
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                let xz = Vec2::new(pos.x, pos.z);
                set.near_xz(*index, xz, xz, 0.0)
            })
            .find(|(_, instance)| instance.contains_walk_point(pos))
            .and_then(|(_, instance)| instance.walkable_surface_y(pos, previous))
    }

    /// Every set query answers through the instance grid exactly what the scan of every instance
    /// answered, over thousands of points, moves and sight lines across a damaged district.
    #[test]
    fn instance_grid_queries_equal_the_linear_scan() {
        let set = grid_district(8);
        let h = |n: u32| (n.wrapping_mul(2_654_435_761) >> 8) as f32 / (1u32 << 24) as f32;
        for n in 0..4_000u32 {
            let p = Vec3::new(h(n * 5) * 170.0 - 10.0, 1.0 + h(n * 5 + 1) * 4.5, h(n * 5 + 2) * 170.0 - 10.0);
            let step = Vec3::new((h(n * 5 + 3) - 0.5) * 0.8, 0.0, (h(n * 5 + 4) - 0.5) * 0.8);
            assert_eq!(set.resolve_walk_xz(p, p + step), linear_resolve_walk(&set, p, p + step));
            assert_eq!(set.walkable_surface_y(p, p.y), linear_surface(&set, p, p.y));
            let set = &set;
            let near_linear = |margin: f32| {
                (0..set.len()).filter(move |&i| {
                    let xz = Vec2::new(p.x, p.z);
                    set.near_xz(i, xz, xz, margin)
                })
            };
            assert_eq!(
                set.unsupported_upper_position(p),
                near_linear(0.0).any(|i| set.get(i).unsupported_upper_position(p))
            );
            assert_eq!(
                set.shelter_at(p),
                near_linear(0.0).map(|i| set.get(i).shelter_at(p)).fold(0.0, f32::max)
            );
            let far = p + Vec3::new((h(n + 77) - 0.5) * 60.0, 0.3, (h(n + 91) - 0.5) * 60.0);
            let mut indexed = Vec::new();
            set.append_infantry_cover(&mut indexed, p, far);
            let mut linear = Vec::new();
            let (min, max) = (Vec2::new(p.x.min(far.x), p.z.min(far.z)), Vec2::new(p.x.max(far.x), p.z.max(far.z)));
            for i in 0..set.len() {
                if set.near_xz(i, min, max, QUERY_MARGIN_M) {
                    set.get(i).append_infantry_cover(&mut linear, p.y, far.y);
                }
            }
            // The sight line meets a circle of the indexed cover exactly when it meets one of
            // the full cover (the only question `infantry::segment_blocked` asks of them).
            let (a2, b2) = (Vec2::new(p.x, p.z), Vec2::new(far.x, far.z));
            let meets = |circles: &[(Vec2, f32)]| {
                circles.iter().any(|&(c, r)| {
                    let ab = b2 - a2;
                    let t = ((c - a2).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    (a2 - c).length_squared() > r * r && (a2 + ab * t - c).length_squared() < r * r
                })
            };
            assert_eq!(meets(&indexed), meets(&linear));
            assert!(indexed.len() <= linear.len());
            assert!(indexed.iter().all(|circle| linear.contains(circle)));
        }
    }

    /// Instances a local query visits stay the same as the district grows by area: 16, 64 and 256
    /// houses at the same density.
    #[test]
    fn instance_grid_work_does_not_grow_with_the_building_count() {
        let mut previous = None;
        for side in [4usize, 8, 16] {
            let set = grid_district(side);
            let mut visited = 0usize;
            for n in 0..500u32 {
                let t = n as f32 / 500.0;
                let p = Vec2::new(5.0 + 50.0 * t, 5.0 + 50.0 * (1.0 - t));
                visited += set.near(p - 1.0, p + 1.0, QUERY_MARGIN_M).len();
                visited += set.near(p, p, 0.0).len();
            }
            if let Some(previous) = previous {
                assert_eq!(visited, previous, "{} houses visited more", side * side);
            }
            previous = Some(visited);
        }
    }

    /// Micro-benchmark (run with `--ignored --nocapture`): nanoseconds per local query for 64 to
    /// 4,096 houses at constant density, grid against the scan of every instance.
    #[test]
    #[ignore = "timing micro-benchmark; prints ns per query"]
    fn instance_grid_micro_benchmark() {
        for side in [8usize, 16, 32, 64] {
            let set = grid_district(side);
            let queries: Vec<(Vec3, Vec3)> = (0..20_000u32)
                .map(|n| {
                    let t = (n % 1000) as f32 / 1000.0;
                    let p = Vec3::new(5.0 + 100.0 * t, 1.2, 5.0 + 100.0 * (1.0 - t));
                    (p, p + Vec3::new(0.05, 0.0, 0.03))
                })
                .collect();
            let time = |f: &dyn Fn(Vec3, Vec3) -> f32| {
                let start = std::time::Instant::now();
                let mut sink = 0.0;
                for &(a, b) in &queries {
                    sink += f(a, b);
                }
                std::hint::black_box(sink);
                start.elapsed().as_nanos() as f64 / queries.len() as f64
            };
            let grid_walk = time(&|a, b| set.resolve_walk_xz(a, b).x + set.walkable_surface_y(a, a.y).unwrap_or(0.0));
            let scan_walk = time(&|a, b| linear_resolve_walk(&set, a, b).x + linear_surface(&set, a, a.y).unwrap_or(0.0));
            println!(
                "building set, {:5} houses: walk + surface {:8.0} ns/query through the grid, {:8.0} ns scanning every instance",
                side * side,
                grid_walk,
                scan_walk
            );
        }
    }

    /// Micro-benchmark (run with `--ignored --nocapture`): the module-level queries `Game` runs
    /// against every building proxy — a 100 m sight line, a 300 m shell ray, a crush disc — through
    /// the grouped spatial index and by scanning every proxy, for 64 to 4,096 houses at constant
    /// density (67 proxies each).
    #[test]
    #[ignore = "timing micro-benchmark; prints ns per query"]
    fn module_index_micro_benchmark() {
        use crate::spatial_grid::BoxIndex;
        fn slab(a: Vec3, b: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
            let d = b - a;
            let (mut t0, mut t1) = (0.0f32, 1.0f32);
            for axis in 0..3 {
                if d[axis].abs() < 1e-12 {
                    if a[axis] < min[axis] || a[axis] > max[axis] {
                        return None;
                    }
                    continue;
                }
                let (mut ta, mut tb) = ((min[axis] - a[axis]) / d[axis], (max[axis] - a[axis]) / d[axis]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 > t1 {
                    return None;
                }
            }
            Some(t0 * d.length())
        }
        for side in [8usize, 16, 32, 64] {
            let set = grid_district(side);
            let mut boxes = Vec::new();
            for instance in 0..set.len() {
                for module in 0..set.get(instance).layout().placements().len() {
                    for &(min, max) in set.get(instance).module_proxies(module) {
                        boxes.push((min, max, instance as u32));
                    }
                }
            }
            let index = BoxIndex::build(
                16.0,
                boxes.iter().enumerate().map(|(key, &(min, max, group))| (key as u32, min, max, Some(group))),
            );
            // Queries inside the first 160 m of the district, which every size contains.
            let queries: Vec<(Vec3, Vec3)> = (0..4_000u32)
                .map(|n| {
                    let t = (n % 400) as f32 / 400.0;
                    let a = Vec3::new(-6.0, 2.2 + (n % 3) as f32 * 0.5, 4.0 + 150.0 * t);
                    let b = a + Vec3::new(100.0, 0.0, 30.0 * ((n % 7) as f32 / 7.0 - 0.5));
                    (a, b)
                })
                .collect();
            let time = |f: &dyn Fn(Vec3, Vec3) -> f32| {
                let start = std::time::Instant::now();
                let mut sink = 0.0;
                for &(a, b) in &queries {
                    sink += f(a, b);
                }
                std::hint::black_box(sink);
                start.elapsed().as_nanos() as f64 / queries.len() as f64
            };
            let sight_grid = time(&|a, b| f32::from(u8::from(index.any_on_segment(a, b, |k| {
                let (min, max, _) = boxes[k as usize];
                slab(a, b, min, max).is_some()
            }))));
            let sight_scan = time(&|a, b| f32::from(u8::from(boxes.iter().any(|&(min, max, _)| slab(a, b, min, max).is_some()))));
            let ray_grid = time(&|a, b| {
                let b = a + (b - a) * 3.0;
                index.nearest_on_segment(a, b, |k| {
                    let (min, max, _) = boxes[k as usize];
                    slab(a, b, min, max)
                }).map_or(0.0, |(t, _)| t)
            });
            let ray_scan = time(&|a, b| {
                let b = a + (b - a) * 3.0;
                boxes.iter().filter_map(|&(min, max, _)| slab(a, b, min, max)).fold(f32::INFINITY, f32::min)
            });
            let crush_grid = time(&|a, _| {
                let mut keys = Vec::new();
                let xz = Vec2::new(a.x + 10.0, a.z);
                index.keys_in_rect(xz - 1.4, xz + 1.4, &mut keys);
                keys.len() as f32
            });
            let crush_scan = time(&|a, _| {
                let xz = Vec2::new(a.x + 10.0, a.z);
                boxes
                    .iter()
                    .filter(|(min, max, _)| max.x >= xz.x - 1.4 && min.x <= xz.x + 1.4 && max.z >= xz.y - 1.4 && min.z <= xz.y + 1.4)
                    .count() as f32
            });
            println!(
                "{:5} houses, {:6} proxies: sight {:7.0} / {:9.0} ns, shell ray {:7.0} / {:9.0} ns, crush {:6.0} / {:8.0} ns (grid / scan)",
                side * side,
                boxes.len(),
                sight_grid,
                sight_scan,
                ray_grid,
                ray_scan,
                crush_grid,
                crush_scan
            );
        }
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
