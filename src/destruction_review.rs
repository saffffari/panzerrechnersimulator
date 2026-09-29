//! Disclosed starts and read-only evidence for existing wall/tree/cook-off gameplay.
//! A child of Game. No damage, fall, explosion, timer or ammunition outcome is assigned here.
use super::*;

const REVIEW_WALL: &str = "module.ground.west.south";

impl Game {
    fn review_wall_indices(&self) -> Vec<usize> {
        let Some(featured) = self.buildings.index_of(crate::building_runtime::FEATURE_INSTANCE_ID) else {
            return Vec::new();
        };
        let building = self.buildings.get(featured);
        self.destructibles.iter().enumerate().filter_map(|(index, _)| {
            self.buildings.owner(index)
                .filter(|&(instance, module)| instance == featured
                    && building.layout().placements()[module].module_id.as_str() == REVIEW_WALL)
                .map(|_| index)
        }).collect()
    }

    /// Place the player in front of existing, intact geometry. Ordinary subsequent driving owns
    /// contact, integrity loss, support propagation and tree hinge motion.
    pub fn debug_stage_destruction_review(&mut self, kind: &str) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(self.standalone_fuel_liters.is_some(), "destruction review requires Playtest");
        anyhow::ensure!(matches!(self.mode, Mode::Tank) && self.player.alive,
            "destruction approach requires a living tank and occupied station");
        let (start, direction, target) = match kind {
            "feature_wall" => {
                let indices = self.review_wall_indices();
                anyhow::ensure!(!indices.is_empty(), "world has no actual compiled review wall");
                anyhow::ensure!(indices.iter().all(|&i| self.destructibles[i].alive), "review wall is already damaged down");
                let (min, max) = indices.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(min,max), &i| (min.min(self.destructibles[i].min), max.max(self.destructibles[i].max)));
                let (instance, module) = self.buildings.owner(indices[0]).unwrap();
                let placement = &self.buildings.get(instance).layout().placements()[module];
                let start = Vec3::new(min.x - 10.0, 0.0, (min.z + max.z) * 0.5);
                (start, Vec3::X, serde_json::json!({"module_id":REVIEW_WALL,
                    "asset_id":placement.asset_id.as_str(),"destructible_indices":indices,
                    "world_min_m":min.to_array(),"world_max_m":max.to_array()}))
            },
            "small_tree" => {
                let targets = self.trees.review_standing_targets(self.player.pos, true);
                let mut selected = None;
                'candidates: for (index, pivot, height) in targets {
                    for direction in [Vec3::Z, Vec3::X, -Vec3::Z, -Vec3::X] {
                        let start = pivot - direction * 10.0;
                        // Reject an approach intersecting other actual solid world bounds. This is
                        // only start selection; it never removes or suppresses an obstruction.
                        let blocked = (0..=16).any(|step| {
                            let p = start + direction * (step as f32 * 0.5 + 1.5);
                            self.story_cover.iter().any(|&(min,max)| nose_contact(p, direction, min, max).is_some())
                                || self.destructibles.iter().any(|d| d.alive && nose_contact(p, direction, d.min, d.max).is_some())
                        });
                        if blocked { continue; }
                        let eye = Vec3::new(start.x, self.hf.height_at(start.x,start.z)+1.0, start.z);
                        if self.trees.ray(eye, direction, 11.0).is_none_or(|(_, hit)| hit != index) { continue; }
                        selected = Some((start, direction, serde_json::json!({"tree_index":index,
                            "pivot_world_m":pivot.to_array(),"height_m":height,"class":"small",
                            "contact_model":"existing standing-trunk cylinder and authored hinge; no fracture solver"})));
                        break 'candidates;
                    }
                }
                selected.ok_or_else(|| anyhow::anyhow!("no existing small tree has a clear bounded approach"))?
            },
            _ => anyhow::bail!("destruction_review must be feature_wall or small_tree"),
        };
        let start = Vec3::new(start.x, self.hf.height_at(start.x,start.z), start.z);
        self.player.pos = start;
        self.player.heading = direction.x.atan2(direction.z);
        self.player.turret_yaw = 0.0;
        self.player.speed = 0.0;
        Ok(serde_json::json!({"kind":kind,"review_only":true,"target":target,
            "tank_world_m":start.to_array(),"heading_rad":self.player.heading,
            "damage_or_fall_forced":false,"scope":"Existing world geometry, ordinary first-person driving"}))
    }

    pub fn destruction_mechanic_status(&self) -> serde_json::Value {
        let indices = self.review_wall_indices();
        let wall = indices.first().and_then(|&i| self.buildings.integrity_for_destructible(i));
        let wall_max_x = indices.iter().map(|&i| self.destructibles[i].max.x).reduce(f32::max);
        let trees = self.trees.review_state_counts();
        serde_json::json!({"review_wall_present":!indices.is_empty(),
            "review_wall_intact":wall.map(|(hp,_)| hp>0),"review_wall_integrity":wall.map(|(hp,_)|hp),
            "review_wall_max_integrity":wall.map(|(_,hp)|hp),
            "review_wall_crossed_m":wall_max_x.map(|x|self.player.pos.x-x),
            "destruction_failed_pieces":self.destructibles.iter().filter(|d|!d.alive).count(),
            "trees_standing":trees[0],"trees_falling":trees[1],"trees_down":trees[2],
            "nearby_tree_records":self.trees.review_nearby(self.player.pos,80.0),
            "fire_active":self.fire_active,"cookoff_remaining_s":self.ammo_fire.remaining(),
            "live_internal_rounds":self.live_ammunition_in_frame(BloodFrame::Hull).len(),
            "death_is_fuel_fire":matches!(self.death_cause,Some(DeathCause::FuelFire)),
            "ammo_rack_functional":self.is_module_functional(ModuleKind::AmmoRack),"god_mode":self.god})
    }
}
