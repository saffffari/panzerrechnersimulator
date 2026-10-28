//! One fuel-consumption/refill path for campaign persistence and the standalone Playtest tank.
//! Campaign objectives are deliberately absent from standalone state.
use super::*;

pub(super) fn uses_campaign_channel(act: u8, authored_mode: Option<&str>) -> bool {
    act == 2 && !matches!(authored_mode, Some("scene" | "playtest"))
}

fn consumed_liters(speed_mps: f32, dt: f32) -> f32 {
    if !speed_mps.is_finite() || !dt.is_finite() {
        return 0.0;
    }
    // Existing authored campaign consumption law. This is a gameplay calibration, not a measured
    // Maybach fuel map. Keeping both modes on this exact law makes the quick-match test meaningful.
    (0.018 + speed_mps.abs() * 0.010) * dt.max(0.0)
}

fn drain_tank(liters: &mut f32, speed_mps: f32, dt: f32) -> bool {
    *liters = (*liters - consumed_liters(speed_mps, dt)).max(0.0);
    *liters > 0.0
}

fn transfer_from_can(liters: &mut f32, can: &mut PortableItem) -> f32 {
    let accepted = accepted_portable_fuel_liters(Some(*liters), can.fuel_liters());
    let drained = can.drain_fuel_liters(accepted);
    *liters = (*liters + drained).min(TIGER_FUEL_CAPACITY_L);
    drained
}

impl Game {
    pub(super) fn seed_playtest_fuel_cans(&mut self) {
        let rear = self
            .player
            .hull_model()
            .transform_point3(Vec3::new(-1.0, 0.0, -4.0));
        let positions = [
            Vec2::new(rear.x, rear.z),
            Vec2::new(-68.0, 100.0),
            Vec2::new(-113.0, 42.0),
            Vec2::new(235.0, 145.0),
        ];
        for (index, pickup) in self
            .pickups
            .iter_mut()
            .filter(|p| p.kind == PickupKind::Fuel)
            .enumerate()
        {
            let point = positions[index % positions.len()];
            pickup.pos = Vec3::new(point.x, self.hf.height_at(point.x, point.y), point.y);
            pickup.portable = Some(PortableItem::full_jerry_can());
            pickup.collected = false;
        }
    }

    pub fn tank_fuel_liters(&self) -> Option<f32> {
        self.campaign
            .as_ref()
            .map(|runtime| runtime.fuel_liters)
            .or(self.standalone_fuel_liters)
    }

    /// Read the real permanent can slots; useful for checking transfer conservation in review.
    pub fn portable_fuel_telemetry(&self) -> serde_json::Value {
        let cans: Vec<_> = self
            .pickups
            .iter()
            .enumerate()
            .filter_map(|(index, pickup)| {
                let can = pickup.portable?;
                Some(
                    serde_json::json!({"index":index,"world_m":pickup.pos.to_array(),
                "liters":can.fuel_liters(),"held":self.held_item==Some(index),
                "grounded":can.grounded,"mass_kg":can.mass_kg()}),
                )
            })
            .collect();
        let total: f32 = self
            .pickups
            .iter()
            .filter_map(|p| p.portable)
            .map(|can| can.fuel_liters())
            .sum();
        serde_json::json!({"fuel_can_total_liters":total,"held_fuel_can":self.held_item.is_some(),
            "fuel_can_count":cans.len(),"fuel_cans":cans})
    }

    pub(super) fn refuel_from_pickup(&mut self, index: usize) -> f32 {
        let Some(can) = self
            .pickups
            .get_mut(index)
            .and_then(|pickup| pickup.portable.as_mut())
        else {
            return 0.0;
        };
        let fuel = self
            .campaign
            .as_mut()
            .map(|runtime| &mut runtime.fuel_liters)
            .or(self.standalone_fuel_liters.as_mut());
        let Some(fuel) = fuel else {
            return 0.0;
        };
        let drained = transfer_from_can(fuel, can);
        if let Some(runtime) = &mut self.campaign {
            runtime.inventory.fuel_liters = runtime.fuel_liters as u16;
        }
        drained
    }

    pub(super) fn update_tiger_fuel(&mut self, dt: f32) {
        if !self.engine_on || !self.player.alive {
            return;
        }
        let speed = self.player.speed;
        let fuel = self
            .campaign
            .as_mut()
            .map(|runtime| &mut runtime.fuel_liters)
            .or(self.standalone_fuel_liters.as_mut());
        if let Some(fuel) = fuel {
            if !drain_tank(fuel, speed, dt) {
                self.engine_on = false;
                self.engine_crank_t = 0.0;
                self.acoustics.starved = true;
            }
        }
    }

    /// Explicit capture fixture. Move one existing real can and the foot actor behind the parked
    /// Tiger; lower only the standalone review tank's fuel. Pickup and transfer are NOT performed
    /// here: the capture must call ordinary interact_portable_item with the actual first-person
    /// camera. The returned record documents every staging change.
    pub fn debug_stage_playtest_refuel(&mut self) -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(
            self.campaign.is_none() && self.standalone_fuel_liters.is_some(),
            "refuel fixture requires standalone Playtest"
        );
        let index = self
            .pickups
            .iter()
            .position(|pickup| pickup.kind == PickupKind::Fuel)
            .ok_or_else(|| anyhow::anyhow!("no allocated real jerry-can pickup"))?;
        let fuel_before = self.standalone_fuel_liters.unwrap();
        self.standalone_fuel_liters = Some(400.0);
        self.engine_on = false;
        self.engine_crank_t = 0.0;
        self.player.speed = 0.0;
        self.mode = Mode::Foot;
        self.climb.active = false;
        self.climb.anim = false;
        self.foot.vel_y = 0.0;
        self.foot.airborne = false;
        self.foot.crouch_t = 0.0;
        if let Some(held) = self.held_item.take() {
            if let Some(item) = self.pickups[held].portable.as_mut() {
                item.put_down();
            }
        }
        let model = self.player.hull_model();
        let actor = model.transform_point3(Vec3::new(0.0, 0.0, -4.35));
        let can = model.transform_point3(Vec3::new(-0.75, 0.0, -3.75));
        self.dbg_place_foot(actor.x, actor.z);
        let can = Vec3::new(can.x, self.hf.height_at(can.x, can.z), can.z);
        self.pickups[index].pos = can;
        self.pickups[index].portable = Some(PortableItem::full_jerry_can());
        self.pickups[index].collected = false;
        self.dbg_look_foot_at(can + Vec3::Y * 0.31);
        Ok(
            serde_json::json!({"fixture":"standalone_rear_service","review_only":true,
            "fuel_before_fixture_l":fuel_before,"fuel_after_fixture_l":400.0,"can_fuel_l":20.0,
            "pickup_index":index,"can_world_m":can.to_array(),"eye_world_m":self.foot.eye().to_array(),
            "engine_on":false,"expected_interactions":["ordinary E picks up physical can","ordinary E at rear transfers 20 L","ordinary E places empty can"],
            "interactions_executed_by_fixture":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_and_playtest_climate_do_not_inherit_campaign_channel_geometry() {
        assert!(uses_campaign_channel(2, None));
        assert!(uses_campaign_channel(2, Some("level")));
        assert!(!uses_campaign_channel(2, Some("scene")));
        assert!(!uses_campaign_channel(2, Some("playtest")));
        assert!(!uses_campaign_channel(1, None));
    }
    #[test]
    fn standalone_and_campaign_share_burn_and_stop_at_empty() {
        let mut campaign = 1.0;
        let mut standalone = 1.0;
        assert!(drain_tank(&mut campaign, 5.0, 10.0));
        assert!(drain_tank(&mut standalone, 5.0, 10.0));
        assert_eq!(campaign, standalone);
        assert!((campaign - 0.32).abs() < 1e-6);
        assert!(!drain_tank(&mut standalone, 5.0, 20.0));
        assert_eq!(standalone, 0.0);
        assert_eq!(consumed_liters(5.0, -1.0), 0.0);
    }
    #[test]
    fn partial_refuel_conserves_tank_and_physical_can_contents() {
        let mut fuel = 535.5;
        let mut can = PortableItem::full_jerry_can();
        assert_eq!(transfer_from_can(&mut fuel, &mut can), 4.5);
        assert_eq!(fuel, TIGER_FUEL_CAPACITY_L);
        assert_eq!(can.fuel_liters(), 15.5);
        assert_eq!(transfer_from_can(&mut fuel, &mut can), 0.0);
        assert_eq!(fuel + can.fuel_liters(), 555.5);
        fuel = 400.0;
        assert_eq!(transfer_from_can(&mut fuel, &mut can), 15.5);
        assert_eq!(can.fuel_liters(), 0.0);
        assert_eq!(can.contents, crate::foot::PortableContents::Empty);
    }
}
