//! Read-only first-person review status. These values expose ordinary simulation state and
//! never change station ownership, weapon readiness, repairs, hatch geometry or damage.
use super::*;

impl Game {
    pub fn mechanic_status(&self) -> serde_json::Value {
        let station = match self.station {
            Station::Commander => "commander",
            Station::Gunner => "gunner",
            Station::Loader => "loader",
            Station::Driver => "driver",
            Station::Radioman => "radio",
        };
        serde_json::json!({
            "current_station": station, "station_ready": self.station_ready(self.station),
            "main_gun_functional": self.module_ok(ModuleKind::MainGun),
            "sight_functional": self.module_ok(ModuleKind::GunnerSight),
            "traverse_functional": self.module_ok(ModuleKind::TurretTraverse),
            "coax_functional": self.module_ok(ModuleKind::CoaxMg),
            "hull_mg_functional": self.module_ok(ModuleKind::HullMg),
            "engine_functional": self.module_ok(ModuleKind::Engine),
            "transmission_functional": self.module_ok(ModuleKind::Transmission),
            "radio_functional": self.module_ok(ModuleKind::Radio),
            "right_track_functional": self.module_ok(ModuleKind::RightTrack),
            "mg_heat": self.mg_heat, "mg_overheated": self.mg_overheated,
            "mg_firing": self.mg_hot, "repair_progress_s": self.repair_t,
            "engine_crank_s": self.engine_crank_t, "gun_pitch_rad": self.player.gun_pitch,
            "commander_exposed": self.commander_outside, "driver_exposed": self.driver_hatch,
            "driver_roof_open": self.roof_hatches.open[0],
            "loader_roof_open": self.roof_hatches.open[1],
            "driver_roof_phase": self.roof_hatches.phase[0],
            "loader_roof_phase": self.roof_hatches.phase[1],
            "loose_rounds": self.loose_shells_active(), "loaded_round_he": self.he_loaded,
            "extinguisher_equipped": self.ext_equipped,
            "player_armor_contacts": self.stat_hits_on_player,
            "last_contact_penetrated": self.last_player_damage_sound.as_ref().map(|(p,_)|p.penetrated),
            "last_contact_caliber_mm": self.last_player_damage_sound.as_ref().map(|(p,_)|p.caliber_mm),
            "last_contact_internal_detonation": self.last_player_damage_sound.as_ref().map(|(_,c)|c.internal_detonation),
        })
    }
}
