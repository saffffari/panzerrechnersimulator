//! Opt-in film staging and a bounded, read-only ledger of ordinary projectile contacts.
//! No terminal event, casualty, armor roll, or damage is manufactured by these helpers.
use super::*;
use serde_json::{json, Value};

const MAX_CONTACTS: usize = 256;

#[derive(Default)]
pub(super) struct BattleAudit {
    tick: u64,
    now_s: f64,
    contacts: Vec<Value>,
    overflow: usize,
}

impl Game {
    pub fn debug_begin_crew_battle_audit(&mut self) {
        self.crew_battle_audit = Some(BattleAudit::default());
    }

    pub fn debug_crew_battle_clock(&mut self, tick: u64, now_s: f64) {
        if let Some(audit) = &mut self.crew_battle_audit {
            audit.tick = tick;
            audit.now_s = now_s;
        }
    }

    pub(super) fn observe_crew_battle_terminal(
        &mut self,
        projectile: &AuthoritativeProjectile,
        contact: ProjectileContact,
        vehicle: Option<&(VehicleArmorContact, crate::ballistics::ImpactEvent)>,
    ) {
        let Some(audit) = &mut self.crew_battle_audit else {
            return;
        };
        if audit.contacts.len() >= MAX_CONTACTS {
            audit.overflow += 1;
            return;
        }
        let armor = vehicle.map(|(armor, event)| {
            json!({
                "vehicle":format!("{:?}",armor.vehicle), "part":format!("{:?}",armor.part),
                "facing":format!("{:?}",armor.facing), "normal_world":armor.world_normal.to_array(),
                "point_part_local_m":armor.local_point.to_array(), "plate":event.plate.name,
                "outcome":format!("{:?}",event.terminal), "penetration_mm":event.pen_mm,
                "effective_armor_mm":event.effective_mm, "impact_angle_deg":event.impact_angle_deg,
            })
        });
        audit.contacts.push(json!({"tick":audit.tick,"tick_end_s":audit.now_s,
            "projectile_id":projectile.id,"shooter":format!("{:?}",projectile.shooter),
            "shell":projectile.shell.name,"caliber_mm":projectile.shell.caliber_mm,
            "projectile_seed":projectile.seed,"muzzle_world_m":projectile.origin.to_array(),
            "impact_world_m":contact.pos.to_array(),"flight_s":projectile.age_s,
            "actual_range_m":projectile.distance_m,"velocity_m_s":projectile.velocity.to_array(),
            "contact_kind":format!("{:?}",contact.kind),"armor":armor,
            "crew_after_terminal":self.crew.iter().map(|person|json!({
                "identity":person.role.name(),"alive":person.ok})).collect::<Vec<_>>(),
            "scope":"Observed after ordinary swept terminal resolution; delayed physical fragments continue on subsequent ticks"}));
    }

    pub fn debug_crew_battle_snapshot(&self) -> Value {
        json!({"tick":self.crew_battle_audit.as_ref().map(|a|a.tick),
            "contacts":self.crew_battle_audit.as_ref().map(|a|&a.contacts),
            "contacts_overflow":self.crew_battle_audit.as_ref().map_or(0,|a|a.overflow),
            "tiger_alive":self.player.alive,"tiger_hp":self.player.hp,
            "tiger_world_m":self.player.pos.to_array(),"tiger_heading_rad":self.player.heading,
            "player_shots":self.main_gun_shot_sequence(),
            "hits_on_tiger":self.stat_hits_on_player,"penetrations_on_tiger":self.stat_pens_on_player,
            "embodied_identity":self.singleplayer_embodied_role().map(CrewRole::name),
            "crew":self.crew.iter().map(|p|json!({"identity":p.role.name(),"alive":p.ok})).collect::<Vec<_>>(),
            "retained_casualties":self.couch_casualties.iter().map(|c|json!({
                "actor_id":c.peer_id,"identity":format!("{:?}",c.identity),"post":format!("{:?}",c.role)
            })).collect::<Vec<_>>(),
            "active_projectiles":self.projectiles.iter().map(|p|json!({"id":p.id,
                "shooter":format!("{:?}",p.shooter),"shell":p.shell.name,
                "origin_m":p.origin.to_array(),"position_m":p.pos.to_array(),"age_s":p.age_s
            })).collect::<Vec<_>>(),
            "enemies":self.enemies.iter().enumerate().filter(|(i,_)|self.enemy_active[*i])
                .map(|(i,e)|json!({"index":i,"kind":e.kind.name(),"alive":e.alive,
                    "position_m":e.pos.to_array(),"heading_rad":e.heading,
                    "shots":self.enemy_main_gun_shot_sequence(i)})).collect::<Vec<_>>()})
    }

    /// Before recording only: park one real T-34 at a measured hull-relative bearing. The world's
    /// terrain/cover and the target armor stay intact; blocked or steep lanes are rejected.
    pub fn debug_stage_crew_battle_shooter(
        &mut self,
        range_m: f32,
        bearing_deg: f32,
        turret_target: bool,
    ) -> anyhow::Result<(usize, Vec3, Value)> {
        anyhow::ensure!(
            self.act.id == 1 && self.campaign.is_none() && self.player.alive,
            "crew firing film requires a living Tiger on actual Act I outside campaign progression"
        );
        anyhow::ensure!(
            range_m.is_finite() && (25.0..=600.0).contains(&range_m) && bearing_deg.is_finite(),
            "invalid controlled firing geometry"
        );
        let bearing = self.player.heading + bearing_deg.to_radians();
        let offset = Vec3::new(bearing.sin(), 0.0, bearing.cos()) * range_m;
        let position = self.player.pos + offset;
        let active =
            self.stage_showreel_t34_crossfire_wave(&[Vec2::new(position.x, position.z)], 0);
        let index = active[0];
        self.targets_only = true; // range fixture: only its explicitly requested muzzle discharge
        self.brawl = false;
        self.god = false;
        self.set_playtest_return_fire(true);
        self.debug_hold_enemy_fire();
        let target = if turret_target {
            self.player
                .turret_model()
                .transform_point3(Vec3::new(0.45, 1.95, 0.30))
        } else {
            self.player
                .hull_model()
                .transform_point3(Vec3::new(0.0, 1.18, 1.70))
        };
        let muzzle = self.enemies[index].muzzle().0;
        anyhow::ensure!(
            self.los_clear(muzzle, target),
            "controlled T-34 lane is blocked by actual world cover; refusing to remove cover"
        );
        let slope = (target.y - muzzle.y).atan2(range_m).to_degrees();
        anyhow::ensure!(
            (-6.0..=14.0).contains(&slope),
            "controlled lane exceeds T-34 elevation envelope"
        );
        self.hatch_open = false;
        self.hatch_t = 0.0;
        self.commander_outside = false;
        self.driver_hatch = false;
        self.force_buttoned = false;
        self.force_driver = false;
        self.chase_cam = false;
        Ok((
            index,
            target,
            json!({"actor_index":index,"vehicle":"T-34-76",
            "shell":enemy_shell(EnemyKind::T34).name,"requested_range_m":range_m,
            "hull_relative_bearing_deg":bearing_deg,"target":"requested geometric aim point; not a forced contact",
            "target_world_m":target.to_array(),"shooter_world_m":self.enemies[index].pos.to_array(),
            "tiger_world_m":self.player.pos.to_array(),"target_frame":if turret_target{"turret"}else{"hull"},
            "slope_deg":slope,"initial_lane_clear":true,
            "staging":"Real parked T-34; normal rate-limited lay, normal muzzle dispersion, ordinary BR-350B flight/earliest contact/armor/anatomy. Trigger time scripted; no selected damage or terminal injection."}),
        ))
    }

    /// Normal rate-limited tank gun lay during the visible preparation interval. Gravity correction
    /// uses this gun's actual ammunition and muzzle position; it does not change the projectile.
    pub fn debug_aim_crew_battle_shooter(&mut self, index: usize, target: Vec3, dt: f32) -> bool {
        let Some(enemy) = self.enemies.get_mut(index) else {
            return false;
        };
        if !enemy.alive || enemy.kind != EnemyKind::T34 {
            return false;
        }
        let aim = ballistic_aim_target(enemy.muzzle().0, target, enemy_shell(enemy.kind));
        enemy.ai_aim(aim, dt)
    }

    /// Recording only: traverse the turret to a hull-relative yaw (radians, as the gunner's lay
    /// would leave it), keeping the mouse-lay target in step so nothing unwinds it. The commander's
    /// camera, and so the acoustic listener's head, rides the turret.
    pub fn debug_traverse_turret_to(&mut self, yaw_rad: f32) {
        if !yaw_rad.is_finite() {
            return;
        }
        self.player.turret_yaw = yaw_rad;
        self.lay_yaw = yaw_rad;
    }

    pub fn debug_launch_crew_battle_shot(&mut self, index: usize) -> Option<u64> {
        if !self.enemy_return_fire_allowed()
            || !self
                .enemies
                .get(index)
                .is_some_and(|e| e.alive && e.kind == EnemyKind::T34)
        {
            return None;
        }
        let id = self.next_projectile_id;
        self.enemy_fire(index);
        self.projectiles.iter().any(|p| p.id == id).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_t34_lay_and_full_flight_reach_the_target_at_all_filming_bearings() {
        let hf = Heightfield {
            res: 2,
            spacing: 2000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(), detail: None,
        };
        for (range, bearing, turret) in [
            (30.0, 90.0_f32, true),
            (100.0, 90.0, true),
            (100.0, 130.0, true),
            (300.0, 0.0, false),
            (80.0, 180.0, false),
        ] {
            let tiger = Tank::new(0.0, 0.0, 0.37, &hf, tank::TIGER_PARTS);
            let angle = tiger.heading + bearing.to_radians();
            let mut shooter = Tank::new(
                angle.sin() * range,
                angle.cos() * range,
                angle + std::f32::consts::PI,
                &hf,
                tank::T34_PARTS,
            );
            let target = if turret {
                tiger
                    .turret_model()
                    .transform_point3(Vec3::new(0.45, 1.95, 0.30))
            } else {
                tiger
                    .hull_model()
                    .transform_point3(Vec3::new(0.0, 1.18, 1.70))
            };
            for _ in 0..360 {
                let aim =
                    ballistic_aim_target(shooter.muzzle().0, target, &crate::ballistics::BR_350B);
                shooter.ai_aim(aim, 1.0 / 120.0);
            }
            let (muzzle, direction) = shooter.muzzle();
            let mut projectile = AuthoritativeProjectile::new(
                1,
                ProjectileShooter::Enemy(0),
                &crate::ballistics::BR_350B,
                muzzle,
                direction,
                23,
            );
            let mut contact = None;
            for _ in 0..720 {
                let step = projectile.swept_step(PROJECTILE_FIXED_DT);
                contact = swept_vehicle_armor_contact(
                    &tiger,
                    ProjectileVehicle::Player,
                    crate::ballistics::ArmorScheme::TigerI,
                    step.start,
                    step.end,
                );
                if contact.is_some() {
                    break;
                }
                projectile.commit_step(step);
            }
            assert!(
                contact.is_some(),
                "un-dispersed real T34 lay missed {range}m / {bearing}deg"
            );
        }
    }

    #[test]
    fn film_audit_is_opt_in_and_does_not_replace_terminal_resolution() {
        let source = include_str!("game.rs");
        let terminal = source
            .split("fn resolve_authoritative_terminal(")
            .nth(1)
            .unwrap()
            .split("fn resolve_authoritative_vehicle_impact(")
            .next()
            .unwrap();
        assert!(
            terminal
                .find("self.resolve_authoritative_vehicle_impact(")
                .unwrap()
                < terminal.find("self.observe_crew_battle_terminal(").unwrap()
        );
        assert!(source.contains("crew_battle_audit: None"));
        assert_eq!(
            enemy_shell(EnemyKind::T34).name,
            crate::ballistics::BR_350B.name
        );
    }
}
