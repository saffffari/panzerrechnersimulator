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

#[derive(Default)]
pub struct BuildingSet {
    instances: Vec<BuildingRuntime>,
    /// Destructible index → (instance, module).
    owners: Vec<Option<(u32, u32)>>,
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
        Ok(Self {
            instances,
            owners: Vec::new(),
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
