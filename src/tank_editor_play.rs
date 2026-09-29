//! Isolated, playable Tiger workshop. This is not the legacy gunnery yard or a campaign attempt.
use super::*;
use crate::tank_editor_scene::{TankAssetId, TankEditorScene};

pub(super) const FLOOR_HALF_M: f32 = 64.0;

pub(super) fn act() -> crate::world::ActMap {
    let mut act = crate::world::act(0);
    act.name = "Tank Workshop";
    act.caption = "Isolated vehicle inspection and play";
    act.season = crate::world::Season::Spring;
    act.half_extent_m = FLOOR_HALF_M;
    act.west_offset_m = 0.0;
    act.spawn = Vec2::ZERO;
    act.goal = Vec2::new(0.0, FLOOR_HALF_M);
    act.goal_radius_m = 0.0;
    act.garage = true;
    act.landmarks.clear();
    act.common_enemies.clear();
    act.boss = None;
    act.command_platoon = false;
    act.final_battle = false;
    act
}

pub(super) fn flat_floor() -> (CpuMesh, Heightfield) {
    let hf = Heightfield {
        res: 33,
        spacing: FLOOR_HALF_M * 2.0 / 32.0,
        heights: vec![0.0; 33 * 33],
        min_h: 0.0,
        max_h: 0.0,
    };
    let mesh = crate::prims::box_mesh(
        Vec3::new(-FLOOR_HALF_M, -0.25, -FLOOR_HALF_M),
        Vec3::new(FLOOR_HALF_M, 0.0, FLOOR_HALF_M),
        [74, 76, 79],
    );
    (mesh, hf)
}

impl Game {
    /// Normal Tiger player simulation in a clean studio. Source-only tank previews cannot enter
    /// this factory: their missing gameplay rigs must never be replaced by a disguised Tiger.
    pub fn build_tank_editor(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &TankEditorScene,
    ) -> anyhow::Result<(Renderer, Game)> {
        scene.validate()?;
        anyhow::ensure!(
            scene.tank == TankAssetId::TigerI,
            "{} has source inspection only; full crew Play is currently available for Tiger I",
            scene.tank.descriptor().label
        );
        Self::build_act_with_attempt(device, queue, false, &act(), None, false, Some(scene))
    }

    pub fn is_tank_editor_play(&self) -> bool {
        self.tank_editor_scene.is_some()
    }

    /// Called before the first rendered frame; reusable combat GPU slots remain allocated for
    /// shared renderer invariants, but no such actor, wreck, pickup, or soldier is active/visible.
    pub(super) fn finish_tank_editor_build(&mut self, renderer: &mut Renderer) {
        for rigs in [
            &self.slots.t34,
            &self.slots.su100,
            &self.slots.kv2,
            &self.slots.isu152,
            &self.slots.is2,
            &self.slots.is3,
        ] {
            for rig in rigs {
                for &slot in rig {
                    renderer.set_visible(slot, false);
                }
            }
        }
        for &slot in &self.slots.flame {
            renderer.set_visible(slot, false);
        }
        self.apply_tank_editor_atmosphere(renderer);
        self.drive_lights(renderer);
        println!("[tank-workshop] isolated Tiger I: flat {}m studio, {} props, {} lamps; no world, active enemies, infantry, pickups, grass, or campaign",
            FLOOR_HALF_M * 2.0, self.tiger_dressing.len(),
            self.tank_editor_scene.as_ref().map_or(0, |scene| scene.lights.len()));
    }

    pub(super) fn apply_tank_editor_atmosphere(&self, renderer: &mut Renderer) {
        let Some(scene) = &self.tank_editor_scene else {
            return;
        };
        let lighting = if scene.lighting.enabled {
            scene.lighting
        } else {
            TankEditorScene::new(TankAssetId::TigerI).lighting
        };
        renderer.precipitation = 0.0;
        renderer.atmosphere_mood = 0.5;
        // The studio's sky is the act clock's: a dated record starts it, the ephemeris lights
        // it, and the observer adapts to it. No studio sun direction or exposure exists.
        if let Some(utc) = lighting.utc {
            if renderer.sky_clock.start != utc {
                renderer.set_sky_clock(crate::celestial_sky::SkyClock::new(utc, renderer.sky_clock.site));
            }
        }
        renderer.fog_density = lighting.fog_density;
        renderer.overcast = lighting.overcast;
        renderer.storm = lighting.storm;
    }

    pub(super) fn tank_editor_lights(&self, renderer: &Renderer) -> Vec<PointLight> {
        let Some(scene) = &self.tank_editor_scene else {
            return Vec::new();
        };
        let mut lights: Vec<_> = scene
            .world_lights(&self.player)
            .into_iter()
            .filter_map(|light| {
                let score =
                    crate::environment_authoring::preview_light_score(&light, renderer.camera.pos)?;
                let mut hash = 0x811c9dc5u32;
                for byte in light.id.bytes() {
                    hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
                }
                let phase =
                    hash as f32 * 0.000_001 + renderer.time * (5.1 + (hash & 7) as f32 * 0.31);
                let flicker =
                    1.0 + light.flicker * (phase.sin() * 0.22 + (phase * 2.37).sin() * 0.10 - 0.14);
                Some((
                    score,
                    PointLight {
                        pos: light.position,
                        source_radius: light.source_radius_m,
                        color: light.linear_rgb(),
                        radiant_intensity: light.radiant_intensity * flicker.max(0.2),
                    },
                ))
            })
            .collect();
        lights.sort_by(|a, b| b.0.total_cmp(&a.0));
        lights.truncate(crate::renderer::N_LIGHTS);
        lights.into_iter().map(|(_, light)| light).collect()
    }

    /// Machine-readable separation from an Act Playtest; useful for startup and editor Play QA.
    pub fn tank_editor_status(&self) -> serde_json::Value {
        serde_json::json!({
            "isolated": self.is_tank_editor_play(), "act_id": self.act.id,
            "active_enemies": self.enemy_active.iter().filter(|active| **active).count(),
            "soldiers": self.soldiers.len(), "world_pickups": self.pickups.len(),
            "grass": self.grass.is_some(), "campaign": self.campaign.is_some(),
            "campaign_channel": self.campaign_channel,
            "props": self.tiger_dressing.len() + usize::from(self.enigma_dressing.is_some()),
            "generic_props": self.tiger_dressing.len(),
            "enigma_override": self.enigma_dressing.as_ref().map(|prop| serde_json::json!({
                "asset":prop.asset, "frame":prop.frame.as_str(),
                "world_model":prop.visual_world_model(&self.player).to_cols_array(),
            })),
            "world_trees":self.trees.count(), "world_scatter":self.scatter.count(),
            "world_occluders":self.occluders.len(), "world_structures":self.destructibles.len(),
            "mission_hud":self.mission_hud().is_some(), "campaign_route":self.campaign_map_state().is_some(),
            "ambient_combat_reports":self.ambient_tick,
            "lights": self.tank_editor_scene.as_ref().map_or(0, |scene| scene.lights.len()),
        })
    }

    /// Exercise the production state, not just the metadata template used by the constructor.
    pub fn verify_tank_editor_scene(&self, authored: &TankEditorScene) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.tank_editor_scene.as_ref() == Some(authored),
            "workshop discarded edited scene"
        );
        anyhow::ensure!(
            self.campaign.is_none()
                && !self.campaign_channel
                && self.standalone_fuel_liters.is_none()
                && !self.act_complete
                && self.mission_hud().is_none()
                && self.campaign_map_state().is_none(),
            "workshop acquired campaign/route state"
        );
        anyhow::ensure!(
            self.enemies.iter().all(|enemy| !enemy.alive)
                && self.enemy_active.iter().all(|active| !active)
                && self.soldiers.is_empty()
                && self.pickups.is_empty()
                && self.allies.is_empty()
                && self.abandoned.is_empty()
                && self.campaign_hulks.is_empty(),
            "workshop acquired battlefield actors"
        );
        anyhow::ensure!(
            self.grass.is_none()
                && self.trees.count() == 0
                && self.scatter.count() == 0
                && self.occluders.is_empty()
                && self.story_cover.is_empty()
                && self.destructibles.is_empty()
                && self.buildings.is_empty(),
            "workshop acquired battlefield scenery"
        );
        anyhow::ensure!(
            self.ambient_tick == 0,
            "workshop scheduled an ambient battlefield explosion"
        );
        let expected: Vec<_> = authored
            .dressing
            .props
            .iter()
            .filter(|prop| prop.asset != ENIGMA_PROP_PATH)
            .collect();
        anyhow::ensure!(
            self.tiger_dressing.iter().eq(expected),
            "workshop generic prop content differs from saved scene"
        );
        anyhow::ensure!(
            self.enigma_dressing.as_ref()
                == authored
                    .dressing
                    .props
                    .iter()
                    .find(|prop| prop.asset == ENIGMA_PROP_PATH),
            "Enigma did not use the saved transform override"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workshop_metadata_has_no_campaign_world_or_muster() {
        let a = act();
        assert_eq!(a.id, 0);
        assert_eq!(a.spawn, Vec2::ZERO);
        assert!(a.garage);
        assert!(a.landmarks.is_empty() && a.common_enemies.is_empty() && a.boss.is_none());
        assert!(!a.command_platoon && !a.final_battle);
        assert_eq!(a.season, crate::world::Season::Spring);
    }

    #[test]
    fn workshop_floor_matches_actual_tiger_ground_contacts() {
        let (floor, hf) = flat_floor();
        assert_eq!(floor.max.y, 0.0);
        assert_eq!(floor.min.x, -FLOOR_HALF_M);
        let tank = Tank::new(0.0, 0.0, 0.0, &hf, tank::TIGER_PARTS);
        assert!(tank.pos.is_finite());
        for p in [Vec2::ZERO, Vec2::new(3.0, 4.0), Vec2::new(-6.0, -9.0)] {
            assert_eq!(hf.height_at(p.x, p.y), floor.max.y);
        }
    }
}
