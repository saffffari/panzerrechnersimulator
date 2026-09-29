//! Bounded, ordinary-AI contacts for the authored winter combat playtest.
use super::*;

fn winter_occupied_hull(mode: Mode, couch_active: bool, crew: &crate::lan::CrewFrame) -> bool {
    mode == Mode::Tank || (couch_active && crate::lan::LanRole::ALL.into_iter().any(|role|
        crew.occupied(role) && !crew.input(role).flag(crate::lan::F_DISMOUNTED)))
}

impl Game {
    pub(super) fn winter_foot_target(&self) -> bool {
        self.mode == Mode::Foot && !(self.winter_encounter.is_some()
            && winter_occupied_hull(self.mode,self.couch_crew_active,&self.couch_crew_frame))
    }
    /// Standalone authored courtyard encounter. Crews begin on their own patrols; source
    /// geometry and ordinary stealth determine contact. Campaign saves/other acts are untouched.
    pub fn enable_live_playtest_encounter(&mut self) {
        if self.act.id != 1 || self.act.season != crate::world::Season::Winter
            || self.act.spawn.distance(Vec2::new(1850.0, 250.0)) > 80.0 {
            eprintln!("[winter-combat] local courtyard encounter requires the authored winter approach map");
            return;
        }
        self.playtest_setup();
        self.set_playtest_return_fire(true);
        let mut encounter = winter_encounter_ai::WinterEncounter::new();
        let mut placed = [false; winter_encounter_ai::SLOTS];
        // Only adjust a fixed waypoint or muster point for physical support/clearance. Neither
        // current player position nor a desired sightline participates in placement.
        let clear_near = |game: &Self, point: Vec2| [Vec2::ZERO, Vec2::X*16.0, -Vec2::X*16.0,
            Vec2::Y*16.0, -Vec2::Y*16.0, Vec2::X*32.0, -Vec2::X*32.0].into_iter()
            .map(|offset| point + offset).find(|&p| game.winter_patrol_point_clear(p));
        for platoon in 0..2 {
            let mut route = encounter.route(platoon);
            let Some(adjusted) = route.iter().map(|&point| clear_near(self, point))
                .collect::<Option<Vec<_>>>() else { continue; };
            route.copy_from_slice(&adjusted);
            encounter.set_route(platoon, route);
            // The leader musters on the beat's first corner, his wingmen in column behind him.
            // Without a leader the platoon is not fielded at all.
            for place in 0..stealth::PLATOON_SIZE {
                let i = winter_encounter_ai::slot(platoon, place);
                if !winter_encounter_ai::fielded(i) || (place > 0 && !placed[winter_encounter_ai::slot(platoon, 0)]) {
                    continue;
                }
                let (muster, heading) = encounter.start(i);
                let Some(p) = clear_near(self, muster) else { continue; };
                let mut enemy = Tank::new(p.x,p.y,heading,&self.hf,tank::T34_PARTS);
                enemy.kind = tank::EnemyKind::T34;
                enemy.reload = 8.0 + i as f32 * 3.0;
                self.enemies[i] = enemy;
                self.enemy_skill[i] = 38;
                placed[i] = true;
            }
        }
        for i in 0..self.enemies.len() {
            let active = placed.get(i).copied().unwrap_or(false);
            self.enemy_active[i] = active;
            self.enemy_concealed[i] = false;
            if !active {
                self.enemies[i].alive = false;
                self.enemies[i].pos = Vec3::new(-3000.0 - i as f32 * 10.0,0.0,-3000.0);
            }
        }
        // Riders and escorts go into action with their own tank, not from their authored seeds.
        self.muster_tank_escorts();
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.winter_encounter = Some(encounter);
        self.begin_winter_mission();
        println!("[winter-combat] {} T-34s in platoons of {:?} (leader first, radio in the leader only); no initial player fix; sight/sound contact and return fire ON",
            placed.iter().filter(|&&active| active).count(), winter_encounter_ai::PLATOON_TANKS);
    }

    fn winter_patrol_point_clear(&self, p: Vec2) -> bool {
        let footprint = tank::T34_PARTS.hull_half_extents.length() + 0.5;
        if p.abs().max_element() > self.act.half_extent_m - footprint - 8.0 { return false; }
        let ground = self.hf.height_at(p.x,p.y);
        let min = Vec3::new(p.x-footprint,ground+0.15,p.y-footprint);
        let max = Vec3::new(p.x+footprint,ground+2.7,p.y+footprint);
        let overlaps = |a: Vec3,b: Vec3| min.cmple(b).all() && max.cmpge(a).all();
        !self.story_cover.iter().any(|&(a,b)| overlaps(a,b))
            && !self.destructibles.iter().any(|d| d.alive && overlaps(d.min,d.max))
            && !self.occluders.iter().any(|o| overlaps(o.min,o.max))
            && [-1.0,1.0].into_iter().all(|x| [-1.0,1.0].into_iter().all(|z|
                (self.hf.height_at(p.x+x*footprint,p.y+z*footprint)-ground).abs() < 1.2))
    }

    /// Called only by successful actual weapon discharges, never by a trigger press.
    pub(super) fn winter_report_discharge(&mut self, origin: Vec3, range: f32) {
        if self.winter_encounter.is_none() { return; }
        for i in 0..winter_encounter_ai::SLOTS {
            if !self.enemies[i].alive || !self.enemy_active[i] { continue; }
            let listener = self.enemies[i].center();
            if listener.distance(origin) > range { continue; }
            let obstructed = !self.los_clear(listener,origin);
            self.winter_encounter.as_mut().unwrap().hear(i,listener,origin,range,obstructed);
        }
    }

    pub(super) fn winter_enemy_orders(&mut self, dt: f32) -> [Option<winter_encounter_ai::Order>; winter_encounter_ai::SLOTS] {
        if self.winter_encounter.is_none() { return [None; winter_encounter_ai::SLOTS]; }
        self.winter_encounter.as_mut().unwrap().tick(dt);
        let live = |i: usize| i < self.enemies.len() && self.enemies[i].alive && self.enemy_active[i];
        let hulls = std::array::from_fn(|i| live(i).then(|| winter_encounter_ai::Hull {
            pos: self.enemies[i].center(), heading: self.enemies[i].heading }));
        let observations = std::array::from_fn(|i| {
            let visible = live(i) && self.player.alive
                && self.winter_encounter.as_ref().unwrap().visible(i)
                && self.stealth.spotted[i] && self.stealth.meters[i] >= 0.9;
            // The platoon's shared knowledge; the encounter lets only a leader act on it.
            let pl = &self.stealth.platoons[self.stealth.platoon_index_of(i)];
            let radio_fix = matches!(pl.state,AlertState::Engaged|AlertState::Searching).then_some(pl.last_known);
            winter_encounter_ai::Observation {
                visible: visible.then(|| self.threat_center()), radio_fix, on_foot:self.winter_foot_target(),
            }
        });
        self.winter_encounter.as_mut().unwrap().orders(&hulls, &observations, dt)
    }

    /// Local footprint probes keep cautious orders out of walls and steep ground; they do not
    /// grant perception through cover. The existing hull solver/vehicle avoidance remains final.
    pub(super) fn winter_safe_maneuver(&self, i: usize, drive: f32, steer: f32) -> (f32,f32) {
        if drive.abs() < 0.001 { return (drive,steer); }
        let enemy = &self.enemies[i];
        let clear = |offset:f32| {
            let heading = enemy.heading + offset;
            let dir = Vec2::new(heading.sin(),heading.cos()) * drive.signum();
            [6.0,12.0].into_iter().all(|distance|
                self.winter_patrol_point_clear(Vec2::new(enemy.pos.x,enemy.pos.z)+dir*distance))
        };
        if clear(0.0) { return (drive,steer); }
        let direction = if clear(-0.65) { -0.55 } else if clear(0.65) { 0.55 }
            else { if i%2 == 0 { -0.55 } else { 0.55 } };
        (0.0,direction)
    }

    /// Debug/capture telemetry only; it is deliberately not a player-facing enemy marker API.
    pub fn winter_encounter_snapshot(&self) -> Option<serde_json::Value> {
        self.winter_encounter.as_ref().map(|ai| serde_json::json!({
            "elapsed_s":ai.elapsed,"sounds_heard":ai.sounds_heard,
            "confirmed_sightings":ai.confirmed_sightings,
            "crews":(0..winter_encounter_ai::SLOTS).filter(|&i| winter_encounter_ai::fielded(i)).map(|i| serde_json::json!({
                "slot":i,"platoon":winter_encounter_ai::platoon_of(i),"role":ai.role(i).name(),
                "alive":self.enemies[i].alive,"phase":ai.phase(i).name(),
                "position":self.enemies[i].pos.to_array(),"visible_now":ai.visible(i),
                "detection":self.stealth.meters[i],
                "main_gun_shots":self.enemy_main_gun_shot_sequence(i),
            })).collect::<Vec<_>>()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winter_dismounted_host_does_not_hide_a_seated_remote_gunner_vehicle() {
        use crate::lan::{CrewFrame,LanRole,F_DISMOUNTED};
        let mut frame = CrewFrame::default();
        frame.roster[LanRole::Commander as usize] = 10;
        frame.inputs[LanRole::Commander as usize].flags = F_DISMOUNTED;
        assert!(!winter_occupied_hull(Mode::Foot,true,&frame));
        frame.roster[LanRole::Gunner as usize] = 20;
        frame.inputs[LanRole::Gunner as usize].post = LanRole::Gunner;
        assert!(winter_occupied_hull(Mode::Foot,true,&frame));
        assert!(!winter_occupied_hull(Mode::Foot,false,&frame),"no change to non-couch foot target");
        frame.inputs[LanRole::Gunner as usize].flags = F_DISMOUNTED;
        assert!(!winter_occupied_hull(Mode::Foot,true,&frame));
        assert!(winter_occupied_hull(Mode::Tank,true,&frame));
    }

    #[test]
    #[ignore = "requires GPU and source winter assets; optional KESSEL_LIVE_COMBAT_REVIEW_PREFIX saves evidence"]
    fn winter_contacts_are_quiet_until_actual_sound_or_sight() -> anyhow::Result<()> {
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let path = crate::act_manifest::playtest_manifest_path();
        crate::act_manifest::select_runtime_manifest(Some(path.clone()))?;
        let manifest = crate::act_manifest::ActManifest::load(&path)?;
        let map = crate::world::resolve_manifest(crate::world::act(1), &manifest)?;
        let (mut renderer, mut game) = Game::build_playtest_act(&device, &queue, false, &map)?;
        game.mute_audio();
        game.enable_live_playtest_encounter();
        let mission = game.winter_mission_snapshot().expect("winter mission must initialize");
        assert!(mission["eligible_rounds"].as_u64().unwrap_or(0) >= 2,"{mission}");
        assert_eq!(mission["empty_start_cradles"].as_array().map(Vec::len),Some(2),"{mission}");
        // Observation fixture only: preserve the observer if unrelated infantry shoot. The
        // shipped setup never enables god mode, seeds a sighting or forces an enemy discharge.
        game.god = true;
        assert!(game.campaign.is_none());
        assert_eq!(game.playtest_return_fire(),Some(true));
        let fielded: Vec<usize> = (0..winter_encounter_ai::SLOTS).filter(|&i| winter_encounter_ai::fielded(i)).collect();
        assert_eq!(game.enemy_active.iter().filter(|&&active| active).count(),
            winter_encounter_ai::PLATOON_TANKS.iter().sum::<usize>(), "every platoon musters on the real map");
        assert!(fielded.iter().all(|&i| game.enemy_active[i]));
        assert!(game.stealth.meters.iter().all(|&meter| meter == 0.0));
        // Riders/escorts start at their own tank, not across the map.
        for &i in &fielded {
            for soldier in game.soldiers.iter().filter(|s| s.escort_enemy == Some(i)) {
                assert!(soldier.alive && soldier.pos.distance(game.enemies[i].pos) < 10.0,
                    "tank {i}'s escort musters beside it");
            }
        }
        let initial: Vec<_> = game.enemies.iter().map(|e| e.pos).collect();
        let mut worst_station = 0.0f32;
        game.mode = Mode::Foot;
        game.foot.speed_frac = 0.0;
        let mut quiet_phases = Vec::new();
        for position in [Vec2::new(1850.0,250.0),Vec2::new(1880.0,165.0)] {
            game.foot.pos = Vec3::new(position.x,game.hf.height_at(position.x,position.y),position.y);
            // Same source terrain/occluders, hull movement, detection, lay and projectile
            // creation as live play. Calling enemy_ai isolates contact logic from unrelated
            // player locomotion/infantry; no free-camera gameplay claim is made for this test.
            for frame in 0..120*30 {
                game.enemy_ai(1.0/30.0);
                // On the real map, with real obstacle avoidance, wingmen keep station.
                if frame > 30*30 {
                    for &i in &fielded {
                        if let winter_encounter_ai::Role::Wingman { leader, place } = game.winter_encounter.as_ref().unwrap().role(i) {
                            let gap = game.enemies[i].pos.distance(game.enemies[leader].pos)
                                - winter_encounter_ai::SPACING_M * place as f32;
                            worst_station = worst_station.max(gap);
                        }
                    }
                }
            }
            assert!(fielded.iter().all(|&i| game.enemy_main_gun_shot_sequence(i) == 0));
            assert!(game.stealth.meters.iter().all(|&meter| meter == 0.0));
            assert_eq!(game.winter_encounter.as_ref().unwrap().confirmed_sightings,0);
            quiet_phases.push(game.winter_encounter_snapshot());
        }
        assert!(fielded.iter().all(|&i| game.enemies[i].pos.distance(initial[i]) > 5.0), "source patrols must move");
        assert!(worst_station < 45.0, "wingmen drift at most {worst_station:.0} m beyond their interval");
        // An actual muzzle report now interrupts quiet immediately. This invokes the normal
        // discharge implementation (fixture bypasses the human loading/input gesture only).
        game.mode = Mode::Tank;
        game.fire(&mut renderer);
        assert_eq!(game.gun_shots,1);
        assert!(game.winter_encounter.as_ref().unwrap().sounds_heard > 0);
        game.enemy_ai(1.0/30.0);
        let sound_snapshot = game.winter_encounter_snapshot();
        assert!(fielded.iter().any(|&i| game.winter_encounter.as_ref().unwrap().phase(i) == winter_encounter_ai::Phase::Investigate));
        let mut shot_at = None;
        for frame in 0..180*30 {
            game.enemy_ai(1.0/30.0);
            if fielded.iter().any(|&i| game.enemy_main_gun_shot_sequence(i) > 0) {
                shot_at = Some(frame as f32/30.0); break;
            }
        }
        assert!(shot_at.is_some(), "source patrols must investigate and acquire an exposed Tiger after its real gunshot: {:?}", game.winter_encounter_snapshot());
        let shots: Vec<_> = fielded.iter().map(|&i| game.enemy_main_gun_shot_sequence(i)).collect();
        assert!(game.projectiles.iter().any(|p| matches!(p.shooter,ProjectileShooter::Enemy(_))),
            "normal AI must create real return-fire projectiles");
        if let Ok(prefix) = std::env::var("KESSEL_LIVE_COMBAT_REVIEW_PREFIX") {
            std::fs::write(format!("{prefix}.json"), serde_json::to_vec_pretty(&serde_json::json!({
                "schema":"panzer.live-winter-combat-review.v2","adapter":adapter.get_info().name,
                "manifest":path,"quiet_seconds_each":120,"quiet_phases":quiet_phases,
                "sound_report":sound_snapshot,"first_return_fire_after_shot_s":shot_at,
                "enemy_main_gun_discharges":shots,"final":game.winter_encounter_snapshot(),
                "scope":"Source Game enemy_ai updates with actual terrain/cover, ordinary perception/lay/fire/projectiles; two staged stationary foot poses and one normal discharge implementation fixture. God preserves observation only. No input-flow or complete mission claim."
            }))?)?;
        }
        crate::act_manifest::select_runtime_manifest(None)?;
        Ok(())
    }
}
