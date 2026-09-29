//! Spatial indexes over the world's static physical boxes (urban intro F2, 2026-09-28): the
//! destruction records (every building-module proxy among them), the stealth sight occluders and
//! the story-layer hard cover. Built once per scene; queries hand back candidates in the order the
//! former linear scans visited them, and callers keep their exact tests, so every answer equals the
//! scan's while its cost follows the boxes near the query rather than the building count.
use super::*;
use crate::spatial_grid::BoxIndex;

/// Grid cell of the world indexes: about one featured house, so a query near a street block
/// meets a handful of building groups.
pub(super) const WORLD_INDEX_CELL_M: f32 = 16.0;

#[derive(Default)]
pub(super) struct WorldIndex {
    /// `Game::destructibles`, grouped by building instance.
    pub structures: BoxIndex,
    /// Each record's box by record index (records are static boxes; only `alive` changes).
    pub structure_bounds: Vec<(Vec3, Vec3)>,
    /// `Game::occluders[..base_occluders]` as built (a collapse only lowers an occluder's top).
    pub occluders: BoxIndex,
    pub occluder_count: usize,
    /// `Game::story_cover`.
    pub story: BoxIndex,
}

impl WorldIndex {
    pub(super) fn build(
        destructibles: &[Destructible],
        occluders: &[stealth::Occluder],
        story: &[(Vec3, Vec3)],
        buildings: &crate::building_set::BuildingSet,
    ) -> Self {
        let owner = |index: usize| buildings.owner(index).map(|(instance, _)| instance as u32);
        let structures = BoxIndex::build(
            WORLD_INDEX_CELL_M,
            destructibles
                .iter()
                .enumerate()
                .map(|(index, d)| (index as u32, d.min, d.max, owner(index))),
        );
        let mut occluder_owner = vec![None; occluders.len()];
        for instance in 0..buildings.len() {
            for module in 0..buildings.get(instance).layout().placements().len() {
                for binding in buildings.bindings(instance, module) {
                    if let Some(slot) = binding.occluder_index.and_then(|i| occluder_owner.get_mut(i)) {
                        *slot = Some(instance as u32);
                    }
                }
            }
        }
        let occluder_index = BoxIndex::build(
            WORLD_INDEX_CELL_M,
            occluders
                .iter()
                .enumerate()
                .map(|(index, o)| (index as u32, o.min, o.max, occluder_owner[index])),
        );
        let story_index = BoxIndex::build(
            WORLD_INDEX_CELL_M,
            story.iter().enumerate().map(|(index, &(min, max))| (index as u32, min, max, None)),
        );
        Self {
            structures,
            structure_bounds: destructibles.iter().map(|d| (d.min, d.max)).collect(),
            occluders: occluder_index,
            occluder_count: occluders.len(),
            story: story_index,
        }
    }
}

impl Game {
    /// Destruction records whose box may overlap the XZ square of half-size `reach` around
    /// `center`, ascending (the linear scan's order).
    pub(super) fn structures_near(&self, center: Vec3, reach: f32) -> Vec<usize> {
        let mut keys = Vec::new();
        let xz = Vec2::new(center.x, center.z);
        self.world_index
            .structures
            .keys_in_rect(xz - Vec2::splat(reach), xz + Vec2::splat(reach), &mut keys);
        debug_assert_eq!(self.world_index.structures.len(), self.destructibles.len());
        keys.into_iter().map(|key| key as usize).collect()
    }

    /// Blast candidates within `reach` of `point`, in the order the former scan of every record
    /// processed them: a building module stands for all of its proxies through its lowest record
    /// (the first the scan met; a module's proxies stand and fall together), and a module is a
    /// candidate when any of its proxies is.
    pub(super) fn blast_candidates(&self, point: Vec3, reach: f32) -> Vec<usize> {
        let mut candidates: Vec<usize> = self
            .structures_near(point, reach)
            .into_iter()
            .map(|index| match self.buildings.owner(index) {
                Some((instance, module)) => self
                    .buildings
                    .bindings(instance, module)
                    .iter()
                    .filter_map(|binding| binding.destructible_index)
                    .min()
                    .unwrap_or(index),
                None => index,
            })
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    /// Nearest standing destruction record along `o + d t`, `t <= max_t` (`ray_aabb` semantics, the
    /// lowest record on a tie).
    pub(super) fn indexed_ray_destructible(&self, o: Vec3, d: Vec3, max_t: f32) -> Option<(f32, usize)> {
        let length = d.length();
        if !(length.is_finite() && (length - 1.0).abs() <= 1.0e-3 && max_t.is_finite()) {
            // The index measures metres along a unit direction; anything else takes the scan.
            return self.linear_ray_destructible(o, d, max_t);
        }
        self.world_index
            .structures
            .nearest_on_segment(o, o + d * max_t, |key| {
                let ds = &self.destructibles[key as usize];
                if ds.alive {
                    ray_aabb(o, d, ds.min, ds.max, max_t)
                } else {
                    None
                }
            })
            .map(|(t, key)| (t, key as usize))
    }

    /// Nearest story-layer hard cover along `o + d t` (`ray_aabb` semantics).
    pub(super) fn indexed_ray_story_cover(&self, o: Vec3, d: Vec3, max_t: f32) -> Option<f32> {
        let length = d.length();
        if !(length.is_finite() && (length - 1.0).abs() <= 1.0e-3 && max_t.is_finite()) {
            return self
                .story_cover
                .iter()
                .filter_map(|&(min, max)| ray_aabb(o, d, min, max, max_t))
                .min_by(f32::total_cmp);
        }
        self.world_index
            .story
            .nearest_on_segment(o, o + d * max_t, |key| {
                let (min, max) = self.story_cover[key as usize];
                ray_aabb(o, d, min, max, max_t)
            })
            .map(|(t, _)| t)
    }

    /// Story cover a soldier moving `old → attempted` can touch, in `story_cover` order (the swept
    /// resolver's own broad phase reaches its 0.28 m body radius; `reach` must cover it).
    pub(super) fn story_cover_near_move(&self, old: Vec3, attempted: Vec3, reach: f32) -> Vec<(Vec3, Vec3)> {
        let mut keys = Vec::new();
        let min = Vec2::new(old.x.min(attempted.x), old.z.min(attempted.z)) - Vec2::splat(reach);
        let max = Vec2::new(old.x.max(attempted.x), old.z.max(attempted.z)) + Vec2::splat(reach);
        self.world_index.story.keys_in_rect(min, max, &mut keys);
        keys.into_iter().map(|key| self.story_cover[key as usize]).collect()
    }

    /// Does any sight occluder break `from → to`? The indexed base plus the wreck occluders
    /// appended during play.
    pub(super) fn occluders_block(&self, from: Vec3, to: Vec3) -> bool {
        let base = self.world_index.occluder_count.min(self.occluders.len());
        self.world_index.occluders.any_on_segment(from, to, |key| {
            self.occluders
                .get(key as usize)
                .is_some_and(|occluder| occluder.blocks_segment(from, to))
        }) || stealth::los_blocked(&self.occluders[base..], from, to)
    }

    /// Does a standing destruction record or story cover lie on the sight line `eye → target`
    /// beyond `near_cull` (the on-foot target test)?
    pub(super) fn structure_or_story_on_sight(&self, eye: Vec3, target: Vec3, near_cull: f32) -> bool {
        let delta = target - eye;
        let distance = delta.length();
        if distance <= 0.0 {
            return false;
        }
        let direction = delta / distance;
        self.world_index.structures.any_on_segment(eye, target, |key| {
            let structure = &self.destructibles[key as usize];
            structure.alive
                && ray_aabb_near(eye, direction, structure.min, structure.max, distance, near_cull)
                    .is_some()
        }) || self.world_index.story.any_on_segment(eye, target, |key| {
            let (min, max) = self.story_cover[key as usize];
            ray_aabb_near(eye, direction, min, max, distance, near_cull).is_some()
        })
    }
}

/// The infantry model's hard cover in the live world: every standing destruction record, the story
/// cover and the armour boxes of the live tanks — the list `mixed_cover` used to rebuild every
/// frame — answered through the spatial indexes. It owns its snapshot (standing flags, tank boxes)
/// so the combat loop may mutate actors while it is held.
pub(super) struct WorldCover {
    index: std::sync::Arc<WorldIndex>,
    standing: Vec<bool>,
    story: std::sync::Arc<Vec<(Vec3, Vec3)>>,
    vehicles: Vec<crate::infantry_combat::Cover>,
}

impl WorldCover {
    fn unit(direction: Vec3, max_distance: f32) -> bool {
        (direction.length() - 1.0).abs() <= 1.0e-3 && max_distance.is_finite()
    }
}

impl crate::infantry_combat::CoverSet for WorldCover {
    fn first_hit(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        use crate::infantry_combat::{ray_box, Cover};
        let structure = |key: u32| {
            let key = key as usize;
            let (min, max) = self.index.structure_bounds[key];
            self.standing[key]
                .then(|| ray_box(origin, direction, Cover { min, max }, max_distance))
                .flatten()
        };
        let story = |key: u32| {
            let (min, max) = self.story[key as usize];
            ray_box(origin, direction, Cover { min, max }, max_distance)
        };
        let (structures, story_hit) = if Self::unit(direction, max_distance) {
            let end = origin + direction * max_distance;
            (
                self.index.structures.nearest_on_segment(origin, end, structure).map(|(t, _)| t),
                self.index.story.nearest_on_segment(origin, end, story).map(|(t, _)| t),
            )
        } else {
            (
                (0..self.standing.len() as u32).filter_map(structure).min_by(f32::total_cmp),
                (0..self.story.len() as u32).filter_map(story).min_by(f32::total_cmp),
            )
        };
        [structures, story_hit, self.vehicles.first_hit(origin, direction, max_distance)]
            .into_iter()
            .flatten()
            .min_by(f32::total_cmp)
    }

    fn occupies(&self, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool {
        use crate::infantry_combat::{cover_occupies, Cover};
        let xz = Vec2::new(point.x, point.z);
        let (lo, hi) = (xz - Vec2::splat(margin), xz + Vec2::splat(margin));
        let mut keys = Vec::new();
        self.index.structures.keys_in_rect(lo, hi, &mut keys);
        let structures = keys.iter().any(|&key| {
            let (min, max) = self.index.structure_bounds[key as usize];
            self.standing[key as usize] && cover_occupies(Cover { min, max }, point, margin, y0, y1)
        });
        structures || {
            self.index.story.keys_in_rect(lo, hi, &mut keys);
            keys.iter().any(|&key| {
                let (min, max) = self.story[key as usize];
                cover_occupies(Cover { min, max }, point, margin, y0, y1)
            })
        } || self.vehicles.occupies(point, margin, y0, y1)
    }
}

impl Game {
    /// The infantry model's hard cover this frame (see [`WorldCover`]).
    pub(super) fn world_cover(&self, vehicles: Vec<crate::infantry_combat::Cover>) -> WorldCover {
        WorldCover {
            index: std::sync::Arc::clone(&self.world_index),
            standing: self.destructibles.iter().map(|d| d.alive).collect(),
            story: std::sync::Arc::clone(&self.story_cover_shared),
            vehicles,
        }
    }
}
