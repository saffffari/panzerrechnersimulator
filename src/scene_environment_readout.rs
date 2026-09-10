//! Read-only world-mechanic evidence and disclosed placement at the existing compiled building.
//! Wire as a child of scene_environment so source/sink state remains private to that subsystem.
use super::*;

impl Game {
    pub fn world_mechanic_status(&self, r: &Renderer) -> serde_json::Value {
        let can = self.pickups.iter().enumerate().find_map(|(index, p)|
            p.portable.map(|item| (index, p.pos, item)));
        let building = self.modular_building.as_ref();
        let shelf_empty = building.map(|b| b.state().containers.iter().any(|(id, state)|
            id.as_str() == "container.store.shelf"
                && matches!(state, crate::building_blueprint::ContainerRuntimeState::Emptied)));
        let emitters: Vec<_> = self.scene_environment.emitters.iter().map(|e| {
            let gains = e.voice.as_ref().map(|v| [
                f32::from_bits(v.parameters.left.load(Ordering::Relaxed)),
                f32::from_bits(v.parameters.right.load(Ordering::Relaxed)),
            ]);
            serde_json::json!({"id":e.source.id,"asset":e.source.asset,
                "world_m":e.source.position.to_array(),"inside":e.trigger.inside,
                "played_once":e.trigger.played_once,"loop_active":e.trigger.loop_active,
                "voice_active":e.voice.as_ref().is_some_and(|v| !v.sink.empty()),
                "failed":e.failed,"receiver_gains":gains})
        }).collect();
        let mut out = serde_json::json!({
            "feature_building_present":building.is_some(),
            "feature_storage_emptied":shelf_empty,
            "feature_storage_searchable":building.is_some_and(|b|
                b.can_search_storage(self.foot.pos,r.camera.pos,r.camera.forward())),
            "feature_shelter":self.building_weather_shelter_at(r.camera.pos),
            "weather_shelter":r.weather_shelter,"precipitation":r.precipitation,
            "supply_ap_rounds":self.foot.supplies.ap_88mm,
            "supply_he_rounds":self.foot.supplies.he_88mm,
            "supply_medical":self.foot.supplies.medical,
            "supply_rations":self.foot.supplies.rations,
            "scene_emitter_count":emitters.len(),
            "scene_emitter_failures":self.scene_environment.emitters.iter().filter(|e|e.failed).count(),
            "scene_emitters_played_once":self.scene_environment.emitters.iter().filter(|e|e.trigger.played_once).count(),
            "scene_active_voices":self.scene_environment.emitters.iter().filter(|e|
                e.voice.as_ref().is_some_and(|v|!v.sink.empty())).count(),
            "scene_active_lights":self.scene_environment.lights(r.time).count(),
            "scene_emitters":emitters,
        });
        if let Some((index, pos, item)) = can {
            let fields=serde_json::json!({"review_can_index":index,
                "review_can_liters":item.fuel_liters(),"review_can_mass_kg":item.mass_kg(),
                "review_can_grounded":item.grounded,"review_can_equipped":item.equipped,
                "review_can_speed_m_s":item.velocity.length(),
                "review_can_x_m":pos.x,"review_can_y_m":pos.y,"review_can_z_m":pos.z,
                "review_can_above_ground_m":pos.y-self.hf.height_at(pos.x,pos.z)});
            out.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        }
        out
    }

    /// A review starts at an actual live feature-building anchor. This changes only the disclosed
    /// actor/camera start; it does not open/search a container, modify damage, roof or loot state,
    /// move the building, grant supplies, or change weather. Subsequent inputs are ordinary play.
    pub fn debug_stage_feature_building(&mut self, fixture: &str) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(self.standalone_fuel_liters.is_some(), "feature review requires Playtest setup");
        let building=self.modular_building.as_ref()
            .ok_or_else(||anyhow::anyhow!("this world has no live compiled feature building"))?;
        let center=building.world_center();
        let (feet, target) = match fixture {
            "entry" => {
                let p=center+Vec3::new(-2.0,0.0,-6.5);
                let p=Vec3::new(p.x,self.hf.height_at(p.x,p.z),p.z);
                (p,Vec3::new(center.x-2.0,p.y+1.7,center.z-2.0))
            },
            "shelf" => {
                let storage=building.storage_world_position()
                    .ok_or_else(||anyhow::anyhow!("actual shelf is destroyed or already empty"))?;
                let p=center+Vec3::new(2.2,0.1,1.6);
                anyhow::ensure!(building.can_search_storage(p,p+Vec3::Y*1.7,storage-(p+Vec3::Y*1.7)),
                    "actual shelf is not reachable from its authored interior search anchor");
                (p,storage)
            },
            _ => anyhow::bail!("feature fixture must be entry or shelf"),
        };
        self.mode=Mode::Foot;
        self.climb.active=false;
        self.climb.anim=false;
        self.foot.pos=feet;
        self.foot.vel_y=0.0;
        self.foot.airborne=false;
        self.foot.crouch_t=0.0;
        self.dbg_look_foot_at(target);
        Ok(serde_json::json!({"fixture":fixture,"review_only":true,
            "actual_building_center_m":center.to_array(),"foot_world_m":feet.to_array(),
            "look_target_world_m":target.to_array(),"interactions_executed_by_fixture":false,
            "scope":"Existing compiled Act-I building; standalone mechanics, no campaign objectives"}))
    }
}
