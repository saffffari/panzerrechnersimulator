//! Read-only gameplay snapshot for the opt-in playtest session log (`session_log.rs`).
//! Nothing here feeds the simulation; it only reports state the game already holds.
use super::*;

impl Game {
    pub fn session_telemetry(&self) -> serde_json::Value {
        let tiger = Vec2::new(self.player.pos.x, self.player.pos.z);
        let enemies: Vec<serde_json::Value> = self
            .enemies
            .iter()
            .enumerate()
            .filter(|(i, _)| self.enemy_active.get(*i).copied().unwrap_or(true))
            .map(|(i, e)| {
                serde_json::json!({
                    "slot": i,
                    "kind": format!("{:?}", e.kind),
                    "alive": e.alive,
                    "range_m": Vec2::new(e.pos.x, e.pos.z).distance(tiger),
                    "speed_m_s": e.speed,
                    "detection": self.stealth.meters.get(i).copied(),
                })
            })
            .collect();
        let living_enemies = self.enemies.iter().enumerate()
            .filter(|(i, e)| e.alive && self.enemy_active.get(*i).copied().unwrap_or(true)).count();
        let nearest_enemy_m = self.enemies.iter().enumerate()
            .filter(|(i, e)| e.alive && self.enemy_active.get(*i).copied().unwrap_or(true))
            .map(|(_, e)| Vec2::new(e.pos.x, e.pos.z).distance(tiger))
            .fold(f32::INFINITY, f32::min);
        let soviet = self.soviet_soldier_count.min(self.soldiers.len());
        let soldiers_alive = self.soldiers[..soviet].iter().filter(|s| s.alive).count();
        let soldiers_fleeing = self.soldiers[..soviet].iter().filter(|s| s.alive && s.is_fleeing()).count();
        let nearest_soldier_m = self.soldiers[..soviet].iter().filter(|s| s.alive)
            .map(|s| Vec2::new(s.pos.x, s.pos.z).distance(tiger)).fold(f32::INFINITY, f32::min);
        let finite = |v: f32| v.is_finite().then_some(v);
        serde_json::json!({
            "mode": if self.mode == Mode::Foot { "Foot" } else { "Tank" },
            "station": format!("{:?}", self.station),
            "commander_outside": self.commander_outside,
            "hatch_open": self.hatch_open,
            "optic_composite": self.optic_composite_active(),
            "tiger": {
                "alive": self.player.alive,
                "position": self.player.pos.to_array(),
                "heading_rad": self.player.heading,
                "pitch_rad": self.player.pitch,
                "roll_rad": self.player.roll,
                "turret_yaw_rad": self.player.turret_yaw,
                "speed_m_s": self.player.speed,
                "gear": self.player_powertrain.gear,
                "rpm": self.player_powertrain.rpm,
                "throttle": self.player_powertrain.throttle,
                "engine_load": self.player_powertrain.load,
                "drive_force_n": self.player_powertrain.drive_force_n,
                "resistance_force_n": self.player_powertrain.resistance_force_n,
                "campaign_fuel_l": self.campaign.as_ref().map(|c| c.fuel_liters),
                "interior_fire": self.fire,
                "modules_out": self.modules.iter().filter(|m| !m.functional)
                    .map(|m| format!("{:?}", m.kind)).collect::<Vec<_>>(),
                "crew_fit": self.crew.iter().filter(|c| c.ok).count(),
            },
            "ammunition": {
                "rack_rounds": self.debug_rack_left(),
                "rack_slots": self.slot_taken.len(),
                "main_gun_rounds": remaining_main_gun_rounds(&self.slot_taken, self.breech),
                "breech": format!("{:?}", self.breech),
                "loose_rounds": self.debug_loose_count(),
            },
            "combat": {
                "projectiles_in_flight": self.projectiles.len(),
                "living_enemy_vehicles": living_enemies,
                "nearest_enemy_vehicle_m": finite(nearest_enemy_m),
                "soviet_infantry_alive": soldiers_alive,
                "soviet_infantry_routed": soldiers_fleeing,
                "nearest_soviet_infantry_m": finite(nearest_soldier_m),
                "wrecks": self.wrecks.iter().filter(|w| w.t_body.is_some()).count(),
                "enemies": enemies,
            },
            "on_foot_hp": self.foot.hp,
            "body_burn": self.player_burn.heat,
            "run": {
                "main_gun_shots": self.gun_shots,
                "hits_by_player": self.stat_hits_by_player,
                "kills_by_player": self.stat_kills_by_player,
                "hits_on_player": self.stat_hits_on_player,
                "penetrations_on_player": self.stat_pens_on_player,
                "live_blasts": self.blasts.iter().filter(|b| b.t > 0.0).count(),
                "mission_beat": self.mission_current_beat(),
                "mission_finished": self.mission_finished(),
                "death_cause": self.game_over().map(|c| format!("{c:?}")),
            },
            "canyon": self.canyon_snapshot(),
            "crew_solo": self.crew_solo_session_json(),
        })
    }
}
