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
    /// A manifest revision that lays the encounter in its streets (`street_encounter` in its edits,
    /// the urban-intro district) gets the street layout (`street_encounter`); a revision without
    /// one (the courtyard workshop) keeps the open-ground encounter exactly.
    pub fn enable_live_playtest_encounter(&mut self) {
        if !self.winter_encounter_map() {
            eprintln!("[winter-combat] local courtyard encounter requires the authored winter approach map");
            return;
        }
        self.playtest_setup();
        self.set_playtest_return_fire(true);
        let placed = self.field_winter_encounter();
        self.begin_winter_mission();
        println!("[winter-combat] {placed} T-34s in platoons of {:?} (leader first, radio in the leader only) on {}; no initial player fix; sight/sound contact and return fire ON",
            winter_encounter_ai::PLATOON_TANKS,
            if self.street_plan.is_some() { "street beats" } else { "open-ground beats" });
    }

    fn winter_encounter_map(&self) -> bool {
        self.act.id == 1 && self.act.season == crate::world::Season::Winter
            && self.act.spawn.distance(Vec2::new(1850.0, 250.0)) <= 80.0
    }

    /// The first level's opening fight (docs/ACT1_FIRST_LEVEL.md): on a campaign Act 1 whose map
    /// lays the street encounter (the urban-intro district), the district's defenders stand in the
    /// streets round the spawn exactly as in the winter session: the two T-34 platoons on their
    /// street beats with their riders, and the two 45 mm guns sited by the field-of-fire probe.
    /// The director raises no route contact until their armour is destroyed. Returns whether the
    /// opening was fielded; any other map keeps the ordinary campaign start.
    pub(super) fn field_campaign_street_opening(&mut self) -> bool {
        if self.campaign.is_none() || self.street_plan.is_none() || !self.winter_encounter_map() {
            return false;
        }
        let placed = self.field_winter_encounter();
        if placed == 0 {
            self.winter_encounter = None;
            return false;
        }
        println!("[director] district opening: {placed} T-34s on the street beats, 45 mm guns in the streets;             route contacts wait until the district's armour is destroyed");
        true
    }

    /// Place the winter encounter's crews and guns on the current map (street layout when the map
    /// lays one, else the courtyard's open ground) and install its AI. Returns the tanks fielded.
    fn field_winter_encounter(&mut self) -> usize {
        let street_plan = self.street_plan.clone();
        self.street_siting = None;
        if street_plan.is_none() && self.buildings.len() > usize::from(self.buildings.featured().is_some()) {
            eprintln!("[winter-combat] this revision places buildings but lays no street encounter: the courtyard's \
                open-ground layout is used (regenerate a district workshop with tools/worldgen/urban_intro_district.py)");
        }
        let mut encounter = match &street_plan {
            Some(plan) => {
                let started = std::time::Instant::now();
                let nav = self.build_street_nav(plan);
                let (w, h) = nav.size();
                println!("[winter-combat] street district: {} of {} cells clear for a T-34 ({:.0} ms)",
                    nav.clear_cells(), w * h, started.elapsed().as_secs_f64() * 1e3);
                winter_encounter_ai::WinterEncounter::in_streets(plan.beats.clone(), nav)
            }
            None => winter_encounter_ai::WinterEncounter::new(),
        };
        let placed = self.muster_winter_platoons(&mut encounter);
        if let Some(plan) = &street_plan {
            // In the streets the rifle battalion's anti-tank platoon is sited by the field-of-fire
            // probe on the built district (street_encounter): from rubble or beside building
            // corners, covering the tram street and the Tiger's ways out north, flanking where the
            // streets allow; never where it sees the spawn or the house.
            match self.site_street_at_guns(plan, &[encounter.route(0), encounter.route(1)]) {
                Ok(siting) => {
                    println!("[at-guns] street siting {}", siting.receipt);
                    let sites = [0, 1].map(|g| (siting.guns[g].at, siting.guns[g].heading));
                    self.street_siting = Some(siting);
                    self.field_at_guns(&sites);
                }
                Err(error) => eprintln!("[at-guns] {error:#}: the guns are not fielded"),
            }
        } else {
            // The rifle battalion's anti-tank platoon: two 45 mm guns sited for flanking fire on the
            // route north out of the courtyard, the way the Tiger must go to reach the tanks. Sites
            // were chosen on the real terrain for their field of fire over that route (the west one
            // sees all of it, the east one most), beyond point blank of the spawn and the house;
            // their sectors face the route, never the player.
            let route_mid = Vec2::new(1855.0, -60.0);
            let sites = [Vec2::new(1555.0, -80.0), Vec2::new(1930.0, -95.0)]
                .map(|site| self.winter_clear_near(site).unwrap_or(site))
                .map(|site| (site, (route_mid.x - site.x).atan2(route_mid.y - site.y)));
            self.field_at_guns(&sites);
        }
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.winter_encounter = Some(encounter);
        placed.iter().filter(|&&active| active).count()
    }

    /// Field the encounter's platoons on their beats: each leader on his beat's first corner, his
    /// wingmen in column behind him, every waypoint and muster point moved only as far as physical
    /// support and clearance require; slots not fielded are parked dead off the map. Riders and
    /// escorts go into action with their own tank. Returns which slots are fielded.
    pub(super) fn muster_winter_platoons(&mut self, encounter: &mut winter_encounter_ai::WinterEncounter)
        -> [bool; winter_encounter_ai::SLOTS] {
        let mut placed = [false; winter_encounter_ai::SLOTS];
        let clear_near = |game: &Self, point: Vec2| game.winter_clear_near(point);
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
        placed
    }

    /// Only adjust a fixed waypoint or muster point for physical support/clearance. Neither
    /// current player position nor a desired sightline participates in placement.
    pub(super) fn winter_clear_near(&self, point: Vec2) -> Option<Vec2> {
        [Vec2::ZERO, Vec2::X*16.0, -Vec2::X*16.0, Vec2::Y*16.0, -Vec2::Y*16.0, Vec2::X*32.0, -Vec2::X*32.0]
            .into_iter().map(|offset| point + offset).find(|&p| self.winter_patrol_point_clear(p))
    }

    pub(super) fn winter_patrol_point_clear(&self, p: Vec2) -> bool {
        let footprint = tank::T34_PARTS.hull_half_extents.length() + 0.5;
        if p.abs().max_element() > self.act.half_extent_m - footprint - 8.0 { return false; }
        let ground = self.hf.height_at(p.x,p.y);
        let min = Vec3::new(p.x-footprint,ground+0.15,p.y-footprint);
        let max = Vec3::new(p.x+footprint,ground+2.7,p.y+footprint);
        !self.box_region_blocked(min,max)
            && [-1.0,1.0].into_iter().all(|x| [-1.0,1.0].into_iter().all(|z|
                (self.hf.height_at(p.x+x*footprint,p.y+z*footprint)-ground).abs() < 1.2))
    }

    /// Does any story cover, standing destruction record or sight occluder overlap the box? The
    /// candidates come from the spatial indexes (a district holds some 136,000 records and 148,000
    /// occluders, and every moving T-34 probes up to six boxes a frame); each keeps the scan's
    /// exact overlap test, so the answer equals `box_region_blocked_linear`'s.
    pub(super) fn box_region_blocked(&self, min: Vec3, max: Vec3) -> bool {
        let overlaps = |a: Vec3,b: Vec3| min.cmple(b).all() && max.cmpge(a).all();
        let (lo,hi) = (Vec2::new(min.x,min.z),Vec2::new(max.x,max.z));
        let mut keys = Vec::new();
        self.world_index.story.keys_in_rect(lo,hi,&mut keys);
        if keys.iter().any(|&key| { let (a,b) = self.story_cover[key as usize]; overlaps(a,b) }) { return true; }
        self.world_index.structures.keys_in_rect(lo,hi,&mut keys);
        if keys.iter().any(|&key| { let d = &self.destructibles[key as usize]; d.alive && overlaps(d.min,d.max) }) { return true; }
        self.world_index.occluders.keys_in_rect(lo,hi,&mut keys);
        let base = self.world_index.occluder_count.min(self.occluders.len());
        keys.iter().any(|&key| self.occluders.get(key as usize).is_some_and(|o| overlaps(o.min,o.max)))
            || self.occluders[base..].iter().any(|o| overlaps(o.min,o.max))
    }

    /// The former scan of every story box, record and occluder: the oracle of `box_region_blocked`.
    #[cfg(test)]
    pub(super) fn box_region_blocked_linear(&self, min: Vec3, max: Vec3) -> bool {
        let overlaps = |a: Vec3,b: Vec3| min.cmple(b).all() && max.cmpge(a).all();
        self.story_cover.iter().any(|&(a,b)| overlaps(a,b))
            || self.destructibles.iter().any(|d| d.alive && overlaps(d.min,d.max))
            || self.occluders.iter().any(|o| overlaps(o.min,o.max))
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
        self.at_guns_hear(origin, range);
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
        // In the streets a leader closing on a report halts where the reported area comes into view:
        // the same sight test the crews see by, to a hull standing there.
        let mut encounter = self.winter_encounter.take().unwrap();
        let sees = |eye: Vec3, at: Vec2| self.los_clear(eye,
            Vec3::new(at.x, self.hf.height_at(at.x, at.y) + tank::TIGER_PARTS.center_h, at.y));
        let orders = encounter.orders(&hulls, &observations, dt, &sees);
        self.winter_encounter = Some(encounter);
        orders
    }

    /// Local footprint probes keep cautious orders out of walls and steep ground; they do not
    /// grant perception through cover. The existing hull solver/vehicle avoidance remains final.
    pub(super) fn winter_safe_maneuver(&self, i: usize, drive: f32, steer: f32) -> (f32,f32) {
        if drive.abs() < 0.001 { return (drive,steer); }
        let enemy = &self.enemies[i];
        // In the streets a tank drives routes measured clear for its width and room either side
        // (`street_route_cell_clear`); the veto there keeps its own width clear over the next metres
        // of travel, on the district's ways.
        let streets = self.winter_encounter.as_ref().is_some_and(|encounter| encounter.streets().is_some());
        let clear = |offset:f32| {
            let heading = enemy.heading + offset;
            let dir = Vec2::new(heading.sin(),heading.cos()) * drive.signum();
            let at = Vec2::new(enemy.pos.x,enemy.pos.z);
            if streets { [4.0,8.0].into_iter().all(|distance| self.street_way_clear(at, at+dir*distance)) }
            else { [6.0,12.0].into_iter().all(|distance| self.winter_patrol_point_clear(at+dir*distance)) }
        };
        if clear(0.0) { return (drive,steer); }
        let direction = if clear(-0.65) { -0.55 } else if clear(0.65) { 0.55 }
            else { if i%2 == 0 { -0.55 } else { 0.55 } };
        (0.0,direction)
    }

    /// Apply the existing source-derived static-cover and terrain clearance to the *resolved*
    /// Soviet motion, including contact pushes. Orders alone could previously be safe while
    /// speed inertia or another tank pushed the hull through a wall. Conservative hull boxes
    /// retain the documented street collision approximation, not a new invisible arena barrier.
    pub(super) fn constrain_winter_enemy_motion(&mut self, i: usize, previous: Vec3) {
        if self.winter_encounter.is_none() { return; }
        let requested = self.enemies[i].pos;
        let from = Vec2::new(previous.x, previous.z);
        let to = Vec2::new(requested.x, requested.z);
        if from.distance_squared(to) < 1e-10 { return; }
        let forward = Vec2::new(self.enemies[i].heading.sin(), self.enemies[i].heading.cos());
        let length = (self.enemies[i].parts.hull_half_extents.y
            - self.enemies[i].parts.hull_half_extents.x).max(0.0);
        let clear = |p: Vec2| [-length, 0.0, length].into_iter()
            .all(|offset| self.street_way_clear(from, p + forward * offset));
        let resolved = clip_winter_motion(from, to, clear);
        if resolved.distance_squared(to) > 1e-10 {
            self.enemies[i].pos = Vec3::new(resolved.x, self.hf.vehicle_height_at(resolved.x,resolved.y),resolved.y);
            self.enemies[i].speed = 0.0;
        }
    }

    pub(super) fn constrain_winter_soldier_motion(&mut self, i: usize, previous: Vec3) {
        if self.winter_encounter.is_none() || !self.soldiers[i].alive { return; }
        let target = self.soldiers[i].pos;
        let near = self.story_cover_near_move(previous,target,0.5);
        let p = crate::building_runtime::resolve_walk_xz_against_bounds(previous,target,&near);
        let p = self.buildings.resolve_walk_xz_body(previous,p,crate::foot::STANDING_HEIGHT);
        let p = self.world_doors.resolve_walk(previous,p,crate::foot::STANDING_HEIGHT);
        // A walking rifleman cannot step straight up a canyon face. Tank escort routes
        // are grounded too; this runtime currently has them walking beside their tank.
        let a = self.hf.height_at(previous.x,previous.z);
        let b = self.hf.height_at(p.x,p.z);
        self.soldiers[i].pos = if (b-a).abs() > 0.45 { previous }
            else { Vec3::new(p.x,b,p.z) };
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
            })).collect::<Vec<_>>(),
            "at_guns":self.at_gun_snapshot(),
            "layout":if ai.streets().is_some() { "streets" } else { "open_ground" },
            "streets":self.street_encounter_snapshot(),
        }))
    }
}

/// Finite sweep prevents a thin obstruction from being skipped by a large frame or shove.
fn clip_winter_motion(from: Vec2, to: Vec2, mut clear: impl FnMut(Vec2) -> bool) -> Vec2 {
    let n = (from.distance(to) / 0.25).ceil().max(1.0) as usize;
    let mut safe = from;
    for k in 1..=n.min(256) {
        let next = from.lerp(to, k as f32 / n as f32);
        if !clear(next) { return safe; }
        safe = next;
    }
    safe
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enemy_inertia_and_shoves_cannot_skip_a_thin_world_obstacle() {
        let clear = |p: Vec2| p.x < 2.0 || p.x > 2.5;
        let clipped = clip_winter_motion(Vec2::ZERO, Vec2::new(5.0,0.0), clear);
        assert!(clipped.x < 2.0 && clipped.x >= 1.75);
        assert_eq!(clip_winter_motion(Vec2::ZERO, Vec2::Y*5.0,clear), Vec2::Y*5.0);
    }

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
    #[ignore = "requires GPU and source winter assets"]
    fn at_guns_wait_in_ambush_then_strike_the_tigers_exterior_and_high_explosive_wrecks_them() -> anyhow::Result<()> {
        use super::at_guns::{GunState, CREW, FIRST_CREW_SOLDIER};
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
        // Both guns stand on clear ground with their crews beside them.
        for g in 0..2 {
            let gun = game.at_guns[g].expect("gun fielded");
            assert_eq!(gun.state, GunState::Hidden);
            for k in 0..CREW {
                let soldier = &game.soldiers[FIRST_CREW_SOLDIER + g * CREW + k];
                assert!(soldier.alive && soldier.pos.distance(gun.pos) < 3.0, "crew {k} at gun {g}");
            }
        }
        // Quiet: the Tiger sits at the spawn for two minutes; no gun fires.
        let dt = 1.0 / 30.0;
        for _ in 0..120 * 30 { game.enemy_ai(dt); }
        assert!(game.at_guns.iter().flatten().all(|gun| gun.shots == 0), "{}", game.at_gun_snapshot());
        // Isolate the guns: the tank platoons withdraw, so every strike below is a 45 mm round.
        for enemy in game.enemies.iter_mut() { enemy.alive = false; }
        // First the ordinary round only: both guns' tungsten is spent elsewhere.
        for gun in game.at_guns.iter_mut().flatten() { gun.apcr = 0; }
        // The Tiger comes out of the courtyard into gun A's sector, 250 m off its flank. In the
        // streets the gun is laid on its field of fire over the streets: the Tiger comes into that
        // field (a street point the gun sees and hits) about 250 m off.
        let gun_a = game.at_guns[0].unwrap();
        let ahead = Vec3::new(gun_a.heading.sin(), 0.0, gun_a.heading.cos());
        let spot = if game.street_siting.is_some() {
            game.street_at_gun_stage(0, 250.0).expect("gun A's field of fire has room for the Tiger")
        } else { gun_a.pos + ahead * 250.0 };
        game.player.pos = Vec3::new(spot.x, game.hf.height_at(spot.x, spot.z), spot.z);
        game.player.heading = if game.street_siting.is_some() { super::at_guns::bearing(game.player.pos, gun_a.pos) }
            else { gun_a.heading } + std::f32::consts::FRAC_PI_2; // flank toward the gun
        game.player.speed = 0.0;
        game.mode = Mode::Tank;
        let hits_before = game.stat_hits_on_player;
        game.fire(&mut renderer); // the fight has started
        for _ in 0..60 * 30 {
            game.enemy_ai(dt);
            game.advance_projectiles(dt, &mut renderer);
        }
        let fired = game.at_guns[0].unwrap().shots;
        assert!(fired >= 3, "gun A engages once the fight is on and the Tiger is in its sector: {}", game.at_gun_snapshot());
        assert!(game.projectiles.iter().all(|p| !matches!(p.shooter, ProjectileShooter::AtGun(1)))
            || game.at_guns[1].unwrap().shots > 0, "gun B only fires on what it can see in its sector");
        assert!(game.stat_hits_on_player > hits_before, "real 45 mm rounds reached the Tiger");
        assert_eq!(game.stat_pens_on_player, 0, "the ordinary round never goes through the side (Kubinka)");
        let receipt = game.exterior_damage_receipt();
        println!("{receipt}");
        assert!(game.exterior.running_gear.iter().any(|&c| c < 1.0)
            || game.modules.iter().any(|m| m.fault != super::tiger_damage::Fault::None),
            "but what it strikes on the outside breaks: {receipt}");
        assert!(game.player.alive);
        // A Tiger HE shell bursting on the gun wrecks it (fired from 40 m out so that only the
        // burst, not the terrain between, is under test).
        let target = gun_a.pos + Vec3::Y * 0.6;
        let ahead = Vec3::new(gun_a.heading.sin(), 0.0, gun_a.heading.cos());
        let origin = gun_a.pos + ahead * 40.0 + Vec3::Y * 1.2;
        let direction = (target - origin).normalize();
        game.spawn_authoritative_projectile(ProjectileShooter::Player, &crate::ballistics::SPRGR_88, origin, direction, 17);
        for _ in 0..3 * 30 { game.advance_projectiles(dt, &mut renderer); }
        assert_eq!(game.at_guns[0].unwrap().state, GunState::Wrecked, "{}", game.at_gun_snapshot());

        // Gun B's artillerymen fall; a rifleman 25 m off runs to the gun and serves it, worse.
        for k in 0..CREW { game.kill_soldier(FIRST_CREW_SOLDIER + CREW + k, "fixture"); }
        let gun_b = game.at_guns[1].unwrap();
        let behind = gun_b.pos - Vec3::new(gun_b.heading.sin(), 0.0, gun_b.heading.cos()) * 25.0;
        let behind = Vec3::new(behind.x, game.hf.height_at(behind.x, behind.z), behind.z);
        game.soldiers[0].activate_escort([behind; 4], &game.hf);
        // A spot in its sector it can actually see (the courtyard's buildings screen some of it; in
        // the streets, a street point in its field of fire).
        let eye = gun_b.pos + Vec3::Y * 1.1;
        let spot = game.street_at_gun_stage(1, 200.0).or_else(|| (0..9).flat_map(|a| (0..6).map(move |r| (a, r))).find_map(|(a, r)| {
            let yaw = gun_b.heading + (a as f32 - 4.0) * 0.12;
            let p = gun_b.pos + Vec3::new(yaw.sin(), 0.0, yaw.cos()) * (180.0 + r as f32 * 20.0);
            let p = Vec3::new(p.x, game.hf.height_at(p.x, p.z), p.z);
            game.los_clear(eye, p + Vec3::Y * 1.5).then_some(p)
        })).expect("gun B's sector has open ground it can see");
        game.player.pos = spot;
        game.player.heading = super::at_guns::bearing(spot, gun_b.pos) + std::f32::consts::FRAC_PI_2;
        let input = TankInput::default();
        for frame in 0..90 * 30 {
            game.update_infantry(&input, dt, &mut renderer);
            game.enemy_ai(dt);
            game.advance_projectiles(dt, &mut renderer);
            if std::env::var("KESSEL_DEBUG_RIFLEMAN").is_ok() && frame % 60 == 0 {
                // KESSEL_DEBUG_RIFLEMAN=1: the rifleman's walk to the gun, every two seconds.
                let s = &game.soldiers[0];
                println!("[rifleman] t {:5.1} pos ({:7.2},{:6.2},{:7.2}) route0 ({:7.2},{:7.2}) alive {} alert {:?} opening {:?}",
                    frame as f32 * dt, s.pos.x, s.pos.y, s.pos.z, s.route[0].x, s.route[0].z, s.alive, s.alert,
                    game.hf.openings.at(s.pos.x, s.pos.z).is_some());
            }
        }
        let served = game.at_guns[1].unwrap();
        assert!(served.riflemen.contains(&Some(0)), "the rifleman took the gun: {}", game.at_gun_snapshot());
        assert!(served.shots > 0, "and it fired again: {}", game.at_gun_snapshot());
        crate::act_manifest::select_runtime_manifest(None)?;
        Ok(())
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
