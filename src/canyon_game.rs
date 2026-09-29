//! The CANYON course in the game (src/canyon.rs): fielding its defence and running its clock.
//!
//! The defence is the winter encounter's own: the two T-34 platoons on the beats the course's street
//! encounter block names (platoon A of three in the final basin, B of two in the factory arena), driving
//! the floor the game measures clear for a T-34 (`build_street_nav`) and hearing and seeing by the
//! ordinary stealth model; one 45 mm gun per choke, chosen among that choke's candidate sites by the
//! same field-of-fire probe the street encounter uses (`probe_gun_site`) over that choke's kill-zone
//! line, with its crew of four; riflemen with the tanks, and the rest posted in the ruins. Nothing is
//! told where the Tiger is.
use super::*;
use super::street_encounter::{sample_kill_zone, SitedGun};

impl Game {
    /// Field the course on a freshly built course workshop: the Tiger at the start, the platoons,
    /// the guns and the riflemen, and start the run's clock. Needs the manifest's `canyon_course`
    /// and `street_encounter` blocks.
    pub fn enable_canyon_course(&mut self) -> anyhow::Result<()> {
        let spec = self.canyon_spec.clone()
            .ok_or_else(|| anyhow::anyhow!("this map declares no canyon course (canyon_course block)"))?;
        let plan = self.street_plan.clone()
            .ok_or_else(|| anyhow::anyhow!("the canyon course needs its street_encounter block"))?;
        anyhow::ensure!(self.act.id == 1 && self.act.season == crate::world::Season::Winter,
            "the canyon course is an Act One winter map");
        self.playtest_setup();
        self.set_playtest_return_fire(true);
        self.player = Tank::new(spec.start.x, spec.start.y, spec.heading, &self.hf, tank::TIGER_PARTS);
        self.spawn_xz = (spec.start.x, spec.start.y);
        self.street_siting = None;
        let started = std::time::Instant::now();
        let nav = self.build_street_nav(&plan);
        let (w, h) = nav.size();
        println!("[canyon] floor: {} of {} cells clear for a T-34 ({:.0} ms)", nav.clear_cells(), w * h,
            started.elapsed().as_secs_f64() * 1e3);
        let mut encounter = winter_encounter_ai::WinterEncounter::in_streets(plan.beats.clone(), nav);
        let placed = self.muster_winter_platoons(&mut encounter);
        // One gun per choke, the best the probe finds among that choke's candidates.
        let routes = [encounter.route(0), encounter.route(1)];
        let legs: Vec<(Vec2, Vec2)> = routes.iter()
            .flat_map(|beat| beat.iter().zip(beat.iter().cycle().skip(1)).map(|(&a, &b)| (a, b))).collect();
        let start_eye = Vec3::new(spec.start.x, self.hf.height_at(spec.start.x, spec.start.y) + tank::TIGER_PARTS.center_h,
            spec.start.y);
        // No gun may see the start within its opening range: the Tiger is not fired on before it has
        // moved (the courtyard's rule for its spawn).
        let watched = [start_eye];
        let mut guns: Vec<(String, SitedGun)> = Vec::new();
        let mut zones = Vec::new();
        for zone in spec.gun_zones.iter().take(at_guns::AT_GUNS) {
            let lines: Vec<_> = plan.kill_zone.iter().filter(|line| line.id == zone.kill_zone).cloned().collect();
            if lines.is_empty() {
                eprintln!("[canyon] gun zone {} names no kill-zone line {:?}", zone.id, zone.kill_zone);
                continue;
            }
            let kill_zone = sample_kill_zone(&lines, |p| self.hf.height_at(p.x, p.y));
            let mut rejected = std::collections::BTreeMap::<&str, usize>::new();
            let mut best: Option<SitedGun> = None;
            let mut fighting = 0usize;
            for &index in &zone.candidates {
                let Some(site) = plan.gun_sites.get(index) else { continue; };
                match self.probe_gun_site(site, &kill_zone, &legs, &watched) {
                    Ok((gun, _)) => {
                        fighting += 1;
                        // Not in another gun's line of fire: at least 60 m from the guns already sited.
                        if guns.iter().any(|(_, other)| other.at.distance(gun.at) < 60.0) { continue; }
                        if best.as_ref().is_none_or(|b| gun.score > b.score) { best = Some(gun); }
                    }
                    Err(why) => *rejected.entry(why).or_default() += 1,
                }
            }
            zones.push(serde_json::json!({
                "zone": zone.id, "candidates": zone.candidates.len(), "fighting": fighting, "rejected": rejected,
                "gun": best.as_ref().map(|gun| serde_json::json!({
                    "at": [gun.at.x, gun.at.y], "kind": gun.kind.name(), "on": gun.on,
                    "heading_deg": gun.heading.to_degrees(), "score": gun.score,
                    "kill_zone_points_seen": gun.field.len(),
                    "range_m": gun.field.iter().fold([f32::MAX, 0.0f32], |[n, f], p| [n.min(p.range_m), f.max(p.range_m)]),
                    "mean_flank_sine": gun.field.iter().map(|f| f.flank).sum::<f32>() / gun.field.len().max(1) as f32,
                })),
            }));
            match best {
                Some(gun) => guns.push((zone.id.clone(), gun)),
                None => eprintln!("[canyon] no candidate of gun zone {} can fight: {rejected:?}", zone.id),
            }
        }
        let sites: Vec<(Vec2, f32)> = guns.iter().map(|(_, gun)| (gun.at, gun.heading)).collect();
        self.field_at_guns(&sites);
        // Riflemen not riding with platoon A's tanks or serving a gun take the course's posts in the
        // ruins; platoon B's tanks patrol the factory arena without riders.
        let crews = at_guns::FIRST_CREW_SOLDIER..at_guns::FIRST_CREW_SOLDIER + sites.len() * at_guns::CREW;
        let riders = |i: usize| i < N_ENEMIES * 2 && winter_encounter_ai::platoon_of(i / 2) == 0
            && self.enemy_active.get(i / 2).copied().unwrap_or(false);
        // Only the Soviet riflemen: the German soldiers of the mixed-combat runtime follow them.
        let free: Vec<usize> = (0..N_SOLDIERS.min(self.soldiers.len())).filter(|&i| {
            !riders(i) && !crews.contains(&i)
        }).collect();
        let mut posted = 0usize;
        if !spec.garrison.is_empty() {
            for (k, &i) in free.iter().enumerate() {
                let post = spec.garrison[k % spec.garrison.len()];
                // A second man at a post stands a pace aside of the first.
                let round = (k / spec.garrison.len()) as f32;
                let aside = Vec2::new(post.heading.cos(), -post.heading.sin()) * (1.6 * round);
                let at = post.at + aside;
                let p = Vec3::new(at.x, self.hf.height_at(at.x, at.y), at.y);
                let soldier = &mut self.soldiers[i];
                soldier.escort_enemy = None;
                soldier.activate_escort([p; 4], &self.hf);
                posted += 1;
            }
        }
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.winter_encounter = Some(encounter);
        let fielded = placed.iter().filter(|&&active| active).count();
        let receipt = serde_json::json!({
            "start": [spec.start.x, spec.start.y], "goal": [spec.goal.x, spec.goal.y], "goal_radius_m": spec.goal_radius_m,
            "length_m": spec.length_m, "t34s": fielded, "platoon_tanks": winter_encounter_ai::PLATOON_TANKS,
            "guns": zones, "riflemen_posted": posted, "rule": "src/canyon.rs, game/canyon_game.rs",
        });
        println!("[canyon] course fielded: {receipt}");
        self.canyon_siting = Some(receipt);
        self.canyon_run = Some(crate::canyon::CanyonRun::new(spec));
        Ok(())
    }

    /// The run's clock and finish, once per simulation step.
    pub(super) fn update_canyon_run(&mut self, dt: f32) {
        let tiger = (self.player.alive && self.mode == Mode::Tank)
            .then(|| Vec2::new(self.player.pos.x, self.player.pos.z));
        let Some(run) = self.canyon_run.as_mut() else { return; };
        if run.step(dt, tiger) {
            println!("[canyon] the Tiger reached the ravine's head in {:.1} s", run.elapsed_s);
            // The fight is over: no Soviet crew fires again while the run fades out.
            self.set_playtest_return_fire(false);
        }
    }

    /// The elapsed time of a completed run, once the Tiger has reached the goal.
    pub fn canyon_finished(&self) -> Option<f32> {
        self.canyon_run.as_ref().and_then(|run| run.finished_s)
    }

    /// Film fixture (disclosed in its setup): the Tiger on its route through gun `g`'s choke about
    /// `range_m` from the gun, where the gun commander sees it, facing up the ravine; the fight has
    /// begun (the gun's crew heard it). With `lay_on_gun` the turret is laid on the gun's shield.
    /// Everything else (the platoons, the other guns, the riflemen) keeps its own state.
    pub fn debug_stage_canyon_fight(&mut self, g: usize, range_m: f32, lay_on_gun: bool) -> anyhow::Result<serde_json::Value> {
        let spec = self.canyon_run.as_ref().map(|run| run.spec.clone())
            .ok_or_else(|| anyhow::anyhow!("the canyon course is not fielded"))?;
        let gun = self.at_guns.get(g).copied().flatten().ok_or_else(|| anyhow::anyhow!("no gun {g} on the course"))?;
        let zone = spec.gun_zones.get(g).ok_or_else(|| anyhow::anyhow!("no gun zone {g}"))?;
        let plan = self.street_plan.clone().ok_or_else(|| anyhow::anyhow!("no street encounter block"))?;
        let lines: Vec<_> = plan.kill_zone.iter().filter(|line| line.id == zone.kill_zone).cloned().collect();
        let points = sample_kill_zone(&lines, |p| self.hf.height_at(p.x, p.y));
        let eye = gun.pos + Vec3::Y * 1.1;
        let reach = tank::TIGER_PARTS.hull_half_extents.length() + 0.3;
        let (point, travel) = points.iter().filter(|k| {
            let centre = Vec3::new(k.at.x, k.ground + tank::TIGER_PARTS.center_h, k.at.y);
            let p = Vec3::new(k.at.x, k.ground, k.at.y);
            self.los_clear(eye, centre) && !self.box_region_blocked(Vec3::new(p.x - reach, p.y + 0.3, p.z - reach),
                Vec3::new(p.x + reach, p.y + 2.8, p.z + reach))
        }).min_by(|a, b| (eye.distance(Vec3::new(a.at.x, a.ground, a.at.y)) - range_m).abs()
            .total_cmp(&(eye.distance(Vec3::new(b.at.x, b.ground, b.at.y)) - range_m).abs()))
            .map(|k| (k.at, k.travel)).ok_or_else(|| anyhow::anyhow!("gun {g} sees no point of its choke's route"))?;
        self.debug_place_tiger(point, travel.x.atan2(travel.y));
        if let Some(gun) = self.at_guns[g].as_mut() { gun.heard = true; }
        if lay_on_gun {
            let shield = gun.pos + Vec3::Y * 0.7;
            let aim = ballistic_aim_target(self.player.muzzle().0, shield, &crate::ballistics::PZGR_39);
            self.player.ai_aim(aim, 1000.0);
            self.lay_yaw = self.player.turret_yaw;
            self.lay_pitch = self.player.gun_pitch;
        }
        Ok(serde_json::json!({"gun": g, "zone": zone.id, "tiger_at": [point.x, point.y],
            "tiger_range_m": Vec3::new(point.x, self.hf.height_at(point.x, point.y), point.y).distance(gun.pos),
            "heading_deg": travel.x.atan2(travel.y).to_degrees(), "fight_started": true,
            "tiger_gun_laid_on_gun": lay_on_gun, "gun_at": [gun.pos.x, gun.pos.z],
            "staging": "a point of the choke's route the gun sees, facing up the ravine; platoons, other guns and riflemen untouched"}))
    }

    /// Debug/capture telemetry: the course, the run and its defenders (never a player-facing marker).
    pub fn canyon_snapshot(&self) -> Option<serde_json::Value> {
        let run = self.canyon_run.as_ref()?;
        Some(serde_json::json!({
            "elapsed_s": run.elapsed_s, "finished_s": run.finished_s,
            "tiger": [self.player.pos.x, self.player.pos.y, self.player.pos.z],
            "tiger_alive": self.player.alive,
            "goal_distance_m": Vec2::new(self.player.pos.x, self.player.pos.z).distance(run.spec.goal),
            "siting": self.canyon_siting,
            "winter": self.winter_encounter_snapshot(),
        }))
    }

    /// Test and film fixture (disclosed where used): put the Tiger at `at` on the ground, facing
    /// `heading`, at rest.
    pub fn debug_place_tiger(&mut self, at: Vec2, heading: f32) {
        self.player.pos = Vec3::new(at.x, self.hf.height_at(at.x, at.y), at.y);
        self.player.heading = heading;
        self.player.speed = 0.0;
        self.player.pitch = 0.0;
        self.player.roll = 0.0;
        self.tank_rest = None;
        self.tank_fall_vy = 0.0;
        self.tank_ground = solid_city::TankGroundState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canyon_manifest() -> Option<std::path::PathBuf> {
        let path = std::path::PathBuf::from(crate::canyon::DEFAULT_MANIFEST);
        path.is_file().then_some(path)
    }

    /// The course on the real GPU and the generated workshop: its defence is fielded (T-34s on
    /// their beats, a gun at every choke, riflemen posted), the walls stop the Tiger head-on and
    /// across, the enemy engages a Tiger in its field, reaching the goal completes the run, and a
    /// dead Tiger's retry builds the course afresh.
    #[test]
    #[ignore = "requires a GPU and the generated canyon workshop (python3 tools/worldgen/canyon_course.py)"]
    fn the_canyon_course_fields_its_defence_stops_the_tiger_at_its_walls_and_completes_at_the_goal() -> anyhow::Result<()> {
        let Some(path) = canyon_manifest() else {
            eprintln!("canyon workshop missing: generate it with python3 tools/worldgen/canyon_course.py");
            return Ok(());
        };
        let act = crate::act_manifest::select_runtime_manifest(Some(path.clone()))?.expect("act");
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let build = || Game::build_playtest_act(&device, &queue, false, &crate::world::act(act));
        let (mut r, mut g) = build()?;
        g.mute_audio();
        g.enable_canyon_course()?;
        g.enable_singleplayer_crew();
        // The Tiger takes no damage in this test (its enemies still fight): the walls and the goal
        // are driven for minutes.
        g.god = true;
        let spec = g.canyon_spec.clone().unwrap();
        let snapshot = g.canyon_snapshot().unwrap();
        println!("{}", serde_json::to_string_pretty(&snapshot["siting"])?);
        // The defence: five T-34s, three guns with crews, riflemen posted.
        let t34s = (0..g.enemies.len()).filter(|&i| g.enemy_active[i] && g.enemies[i].alive).count();
        assert_eq!(t34s, 5, "{snapshot}");
        let guns = g.at_guns.iter().flatten().count();
        assert_eq!(guns, 3, "a gun at every choke: {}", snapshot["siting"]);
        // Six riflemen in the ruins (two per group), platoon A's riders with their tanks.
        assert!(snapshot["siting"]["riflemen_posted"].as_u64().unwrap() >= 6, "{}", snapshot["siting"]);
        // Density rises toward the goal: the defenders' mean distance from the start grows from the
        // guns at the first choke to the platoon in the final basin.
        let from_start = |p: Vec2| p.distance(spec.start);
        let gun0 = g.at_guns[0].unwrap().pos;
        let platoon_a = (0..3).map(|i| from_start(Vec2::new(g.enemies[i].pos.x, g.enemies[i].pos.z))).sum::<f32>() / 3.0;
        let platoon_b = (3..5).map(|i| from_start(Vec2::new(g.enemies[i].pos.x, g.enemies[i].pos.z))).sum::<f32>() / 2.0;
        assert!(from_start(Vec2::new(gun0.x, gun0.z)) < platoon_b && platoon_b < platoon_a,
            "gun 0 {:.0} m, platoon B {platoon_b:.0} m, platoon A {platoon_a:.0} m", from_start(Vec2::new(gun0.x, gun0.z)));
        // Nothing fires on the Tiger at its start.
        let input = TankInput::default();
        for _ in 0..120 { g.update(&input, 1.0 / 60.0, &mut r); }
        assert!(g.player.alive && g.at_guns.iter().flatten().all(|gun| gun.shots == 0));

        // The walls: from the floor, drive straight at the nearest wall and then diagonally up it;
        // the Tiger never climbs more than a few metres above the floor (the talus foot).
        let mut worst_climb = 0.0f32;
        for (label, offset_deg) in [("head_on", 0.0f32), ("diagonal", 50.0), ("steep_diagonal", 70.0)] {
            for side in [-1.0f32, 1.0] {
                // The wall across the travel at the start basin: east (+1) or west (-1).
                let start = spec.start + Vec2::new(0.0, -120.0);
                g.debug_place_tiger(start, spec.heading);
                let floor = g.hf.height_at(start.x, start.y);
                let heading = spec.heading + side * (std::f32::consts::FRAC_PI_2 - offset_deg.to_radians());
                g.debug_place_tiger(start, heading);
                let drive = TankInput { drive: 1.0, ..TankInput::default() };
                let mut top = 0.0f32;
                for _ in 0..(60 * 45) {
                    g.update(&drive, 1.0 / 60.0, &mut r);
                    top = top.max(g.player.pos.y - floor);
                }
                println!("[walls] {label} side {side}: highest {top:.1} m above the floor, refused {} ({:.1}), at ({:.0}, {:.0})",
                    g.tank_ground.refused, g.tank_ground.refused_value, g.player.pos.x, g.player.pos.z);
                worst_climb = worst_climb.max(top);
            }
        }
        assert!(worst_climb < 12.0, "the Tiger climbed {worst_climb:.1} m up a wall (the rim is 25-45 m up)");

        // The enemy engages: the Tiger stood in the first gun's field of fire, with the fight begun.
        let gun = g.at_guns[0].unwrap();
        let field_point = g.canyon_siting.as_ref().unwrap()["guns"][0]["gun"]["at"].clone();
        println!("gun 0 at {field_point} heading {:.0}", gun.heading.to_degrees());
        let ahead = Vec2::new(gun.pos.x, gun.pos.z) + Vec2::new(gun.heading.sin(), gun.heading.cos()) * 220.0;
        let stage = (0..24).map(|k| {
            let a = gun.heading + (k as f32 - 12.0) * 0.06;
            Vec2::new(gun.pos.x, gun.pos.z) + Vec2::new(a.sin(), a.cos()) * (180.0 + (k % 4) as f32 * 30.0)
        }).find(|&p| {
            let eye = gun.pos + Vec3::Y * 1.1;
            let tiger = Vec3::new(p.x, g.hf.height_at(p.x, p.y) + tank::TIGER_PARTS.center_h, p.y);
            g.los_clear(eye, tiger) && g.winter_patrol_point_clear(p)
        }).unwrap_or(ahead);
        g.debug_place_tiger(stage, (gun.pos.x - stage.x).atan2(gun.pos.z - stage.y) + std::f32::consts::FRAC_PI_2);
        g.at_guns_hear(g.player.center(), 2000.0);
        for _ in 0..(60 * 40) {
            g.update(&input, 1.0 / 60.0, &mut r);
            if g.at_guns[0].is_some_and(|gun| gun.shots > 0) { break; }
        }
        assert!(g.at_guns[0].is_some_and(|gun| gun.shots > 0), "the choke's gun engages: {}", g.at_gun_snapshot());

        // Reaching the goal completes the run once, and the fight stops.
        assert_eq!(g.canyon_finished(), None);
        let short = spec.goal + (spec.start - spec.goal).normalize() * (spec.goal_radius_m + 25.0);
        g.debug_place_tiger(short, (spec.goal.x - short.x).atan2(spec.goal.y - short.y));
        g.update(&input, 1.0 / 60.0, &mut r);
        assert_eq!(g.canyon_finished(), None, "short of the head the run goes on");
        let inside = spec.goal + (spec.start - spec.goal).normalize() * (spec.goal_radius_m - 10.0);
        g.debug_place_tiger(inside, (spec.goal.x - inside.x).atan2(spec.goal.y - inside.y));
        g.update(&input, 1.0 / 60.0, &mut r);
        let finished = g.canyon_finished().expect("the run completes in the goal zone");
        assert!(finished > 0.0 && g.playtest_return_fire() == Some(false));

        // Death retries: a fresh build of the course starts the run again at the start, defended.
        drop((r, g));
        let (mut r, mut g) = build()?;
        g.mute_audio();
        g.enable_canyon_course()?;
        assert_eq!(g.canyon_finished(), None);
        assert!(Vec2::new(g.player.pos.x, g.player.pos.z).distance(spec.start) < 1.0);
        g.update(&input, 1.0 / 60.0, &mut r);
        assert!(g.canyon_snapshot().unwrap()["elapsed_s"].as_f64().unwrap() > 0.0);
        Ok(())
    }
}
