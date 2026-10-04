//! The CANYON course in the game (src/canyon.rs): fielding its defence and running its clock.
//!
//! The defence is the winter encounter's own: the two T-34 platoons on the beats the course's street
//! encounter block names (platoon A of three in the final basin, B of two in the factory arena), driving
//! the floor the game measures clear for a T-34 (`build_street_nav`) and hearing and seeing by the
//! ordinary stealth model; up to two 45 mm guns per choke, chosen among that choke's candidate sites by the
//! same field-of-fire probe the street encounter uses (`probe_gun_site`) over that choke's kill-zone
//! line, with its crew of four; riflemen with the tanks, and the rest posted in the ruins. Nothing is
//! told where the Tiger is. These are opening forces; the adaptive director admits fresh armour
//! through concealed, navigable approaches throughout the run, retaining every previous casualty.
use super::*;
use super::street_encounter::{sample_kill_zone, SitedGun};

fn combat_body_occupied(cover: &impl crate::infantry_combat::CoverSet, p: Vec3) -> bool {
    cover.occupies(p, 0.35, p.y + 0.08, p.y + crate::foot::STANDING_HEIGHT)
}

/// Where along the corridor (fraction of its length) the opening approach platoons stand, before
/// the seeded shift. The authored plan's own two platoons are at 60-69 and 84-92 percent.
const OPENING_PLATOON_FRACTIONS: [f32; 3] = [0.16, 0.31, 0.46];
/// How many leading legs of a platoon's beat are path-searched when it is sited.
const ROUTE_LEGS_SEARCHED: usize = 5;
/// How many candidate entries one director request examines before deferring.
const ENTRY_CANDIDATES_PER_REQUEST: usize = 6;
/// No opening platoon starts closer to the Tiger than this.
const OPENING_PLATOON_MIN_M: f32 = 450.0;

impl Game {
    /// Field the course on a freshly built course workshop: the Tiger at the start, the platoons,
    /// the guns and the riflemen, and start the run's clock. Needs the manifest's `canyon_course`
    /// and `street_encounter` blocks.
    pub fn enable_canyon_course(&mut self) -> anyhow::Result<()> {
        self.enable_canyon_course_seeded(crate::canyon::fresh_seed())
    }

    pub fn enable_canyon_course_seeded(&mut self, seed: u64) -> anyhow::Result<()> {
        let spec = self.canyon_spec.clone()
            .ok_or_else(|| anyhow::anyhow!("this map declares no canyon course (canyon_course block)"))?;
        let plan = self.street_plan.clone()
            .ok_or_else(|| anyhow::anyhow!("the canyon course needs its street_encounter block"))?;
        anyhow::ensure!(self.act.id == 1 && self.act.season == crate::world::Season::Winter,
            "the canyon course is an Act One winter map");
        self.playtest_setup();
        if let Some(audio)=&self.audio { audio.reset_soundscape(seed); }
        self.god = false;
        self.canyon_opening_fielded = false;
        self.set_playtest_return_fire(true);
        self.player = Tank::new(spec.start.x, spec.start.y, spec.heading, &self.hf, tank::TIGER_PARTS);
        self.spawn_xz = (spec.start.x, spec.start.y);
        self.street_siting = None;
        let started = std::time::Instant::now();
        let nav = self.build_street_nav(&plan);
        let (w, h) = nav.size();
        println!("[canyon] floor: {} of {} cells clear for a T-34 ({:.0} ms)", nav.clear_cells(), w * h,
            started.elapsed().as_secs_f64() * 1e3);
        // Course loops are authored on a terrain route; masonry, trees and craters can
        // invalidate individual knots after the scene is built. Project onto measured
        // clear cells and retain supported knots. The navigation grid still routes the
        // tank between them, rather than ordering straight chords through the terrain.
        let beat = |game: &Game, i: usize, phase: u64| {
            let mut points: Vec<Vec2> = crate::canyon::varied_beat(&plan.beats[i], phase)
                .into_iter().filter_map(|p| nav.nearest_clear(p,20.0)
                    .filter(|&q|game.winter_patrol_point_clear(q))).collect();
            points.dedup_by(|a,b|a.distance_squared(*b)<1.0);
            points
        };
        let beats = std::array::from_fn(|i| beat(self,i,seed ^ i as u64));
        anyhow::ensure!(beats.iter().all(|b|b.len()>=2),"canyon has no supported patrol loop");
        let mut encounter = winter_encounter_ai::WinterEncounter::in_streets(beats, nav.clone());
        while encounter.slots() < self.enemies.len() {
            encounter.append_platoon(encounter.route(0),1);
        }
        let mut placed = self.muster_winter_platoons(&mut encounter);
        for attempt in 1..16u64 {
            if placed.iter().filter(|&&v|v).count() == 5 { break; }
            for i in 0..2 {
                // `WinterEncounter::start` reads the beat's first leg; a degenerate re-roll with
                // fewer than two corners would index past its end.
                let route = beat(self,i,seed ^ i as u64 ^ (attempt << 32));
                if route.len() >= 2 { encounter.set_route(i, route); }
            }
            placed = self.muster_winter_platoons(&mut encounter);
        }
        anyhow::ensure!(placed.iter().filter(|&&v|v).count() == 5,
            "canyon seed {seed} cannot place both complete platoons on legal ground");
        // A separated pair per choke, seeded among that choke's geometrically viable candidates.
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
        for (zone_index, zone) in spec.gun_zones.iter().take(at_guns::AT_GUNS).enumerate() {
            let lines: Vec<_> = plan.kill_zone.iter().filter(|line| line.id == zone.kill_zone).cloned().collect();
            if lines.is_empty() {
                eprintln!("[canyon] gun zone {} names no kill-zone line {:?}", zone.id, zone.kill_zone);
                continue;
            }
            let kill_zone = sample_kill_zone(&lines, |p| self.hf.height_at(p.x, p.y));
            let mut rejected = std::collections::BTreeMap::<&str, usize>::new();
            let mut viable: Vec<(usize, SitedGun)> = Vec::new();
            let mut fighting = 0usize;
            for &index in &zone.candidates {
                let Some(site) = plan.gun_sites.get(index) else { continue; };
                match self.probe_gun_site(site, &kill_zone, &legs, &watched) {
                    Ok((gun, _)) => {
                        fighting += 1;
                        // Not in another gun's line of fire: at least 60 m from the guns already sited.
                        if guns.iter().any(|(_, other)| other.at.distance(gun.at) < 60.0) { continue; }
                        viable.push((index, gun));
                    }
                    Err(why) => *rejected.entry(why).or_default() += 1,
                }
            }
            // Every choice must pass the same geometry/field-of-fire probe. Choose among
            // viable positions within 60% of the best score, not one eternal optimal ambush.
            let peak = viable.iter().map(|(_,g)| g.score).fold(0.0_f32, f32::max);
            viable.retain(|(_,g)| g.score >= peak * 0.60);
            let choices = viable.len();
            viable.sort_by_key(|(index, _)|
                crate::canyon::mix(seed ^ (zone_index as u64 * 0x9e3779b9) ^ *index as u64));
            let mut selected = Vec::new();
            for (_, gun) in viable.into_iter().rev() {
                if selected.len() >= 2 || guns.len() >= at_guns::AT_GUNS { break; }
                if guns.iter().any(|(_, other)| other.at.distance(gun.at) < 60.0) { continue; }
                selected.push(serde_json::json!({
                    "at": [gun.at.x, gun.at.y], "kind": gun.kind.name(), "on": gun.on,
                    "heading_deg": gun.heading.to_degrees(), "score": gun.score,
                    "kill_zone_points_seen": gun.field.len(),
                    "range_m": gun.field.iter().fold([f32::MAX, 0.0f32], |[n, f], p| [n.min(p.range_m), f.max(p.range_m)]),
                    "mean_flank_sine": gun.field.iter().map(|f| f.flank).sum::<f32>() / gun.field.len().max(1) as f32,
                }));
                guns.push((zone.id.clone(), gun));
            }
            if selected.is_empty() {
                eprintln!("[canyon] no candidate of gun zone {} can fight: {rejected:?}", zone.id);
            }
            zones.push(serde_json::json!({
                "viable_choices": choices,
                "zone": zone.id, "candidates": zone.candidates.len(), "fighting": fighting, "rejected": rejected,
                "gun": selected.first(), "guns": selected,
            }));
        }
        let sites: Vec<(Vec2, f32)> = guns.iter().map(|(_, gun)| (gun.at, gun.heading)).collect();
        self.field_at_guns(&sites);
        // Riflemen not riding with platoon A's tanks or serving a gun take the course's posts in the
        // ruins; platoon B's tanks patrol the factory arena without riders.
        let crews = at_guns::FIRST_CREW_SOLDIER..at_guns::FIRST_CREW_SOLDIER + sites.len() * at_guns::CREW;
        let riders = |i: usize| i < N_ENEMIES * 2 && winter_encounter_ai::platoon_of(i / 2) == 0
            && self.enemy_active.get(i / 2).copied().unwrap_or(false);
        // Only the Soviet riflemen; Canyon's scene omits the unrelated allied practice squad.
        let free: Vec<usize> = (0..self.soviet_soldier_count.min(self.soldiers.len())).filter(|&i| {
            !riders(i) && !crews.contains(&i)
        }).collect();
        let mut posted = 0usize;
        // Seed a whole loose group near one post, with measured body clearance. Selecting each
        // member independently across a route band created apparent lone wandering riflemen.
        let mut defense_centers: Vec<Vec2> = spec.gun_zones.iter().filter_map(|zone| {
            let points: Vec<_> = guns.iter().filter(|(id,_)|id==&zone.id).map(|(_,g)|g.at).collect();
            (!points.is_empty()).then(||points.iter().copied().sum::<Vec2>() / points.len() as f32)
        }).collect();
        if defense_centers.is_empty() {
            defense_centers = [encounter.route(0),encounter.route(1)].iter()
                .map(|p|p.iter().copied().sum::<Vec2>()/p.len() as f32).collect();
        }
        defense_centers.sort_by(|a,b|crate::canyon_director::route_progress(&spec.route,*a)
            .total_cmp(&crate::canyon_director::route_progress(&spec.route,*b)));
        let mut groups = Vec::new();
        let mut group_centers: Vec<Vec2> = Vec::new();
        let cover = self.mixed_cover(None);
        let group_count = free.len().div_ceil(infantry_squad::GARRISON_GROUP_SIZE);
        for group in 0..group_count {
            let first = group * free.len() / group_count;
            let end = (group + 1) * free.len() / group_count;
            let members = &free[first..end];
            let center=defense_centers[group % defense_centers.len()];
            let mut candidates: Vec<_> = spec.garrison.iter().enumerate()
                .filter(|(_,post)|post.at.distance(center)<=320.0).collect();
            candidates.sort_by_key(|(j,_)| crate::canyon::mix(seed ^ 0x61726973 ^ *j as u64 ^ ((group as u64) << 32)));
            // When no post near the defended zone has clear ground for this seed, fall back to
            // the remaining posts nearest first instead of refusing to field the whole course.
            let mut farther: Vec<_> = spec.garrison.iter().enumerate()
                .filter(|(_,post)|post.at.distance(center)>320.0).collect();
            farther.sort_by(|a,b|a.1.at.distance(center).total_cmp(&b.1.at.distance(center)));
            candidates.extend(farther);
            let deployed = candidates.iter().find_map(|(_,post)| {
                if group_centers.iter().any(|p|p.distance(post.at)<35.0) { return None; }
                let origin = Vec3::new(post.at.x,self.hf.height_at(post.at.x,post.at.y)+1.5,post.at.y);
                let points=infantry_squad::deployment(post.at,post.heading,members.len(),|at| {
                    let y=self.hf.height_at(at.x,at.y);
                    let p=Vec3::new(at.x,y,at.y);
                    [Vec2::X*0.5,-Vec2::X*0.5,Vec2::Y*0.5,-Vec2::Y*0.5].iter()
                        .all(|off|(self.hf.height_at(at.x+off.x,at.y+off.y)-y).abs()<0.325)
                        && !combat_body_occupied(&cover,p)
                        && self.los_clear(origin,p+Vec3::Y*1.5)
                })?;
                Some((**post,points))
            }).ok_or_else(||anyhow::anyhow!("no clear ground for Canyon infantry group {group}"))?;
            let (post, points)=deployed;
            for (&i,at) in members.iter().zip(points) {
                let p = Vec3::new(at.x, self.hf.height_at(at.x, at.y), at.y);
                let soldier = &mut self.soldiers[i];
                soldier.escort_enemy = None;
                soldier.activate_escort([p; 4], &self.hf);
                soldier.yaw = infantry_squad::actor_yaw(post.heading);
                posted += 1;
            }
            group_centers.push(post.at);
            groups.push(infantry_squad::Squad { members: members.to_vec() });
        }
        self.mixed_combat.squads = groups;
        self.stealth = stealth::Stealth::new(self.enemies.len());
        self.winter_encounter = Some(encounter);
        let fielded = placed.iter().filter(|&&active| active).count();
        let receipt = serde_json::json!({
            "seed": seed, "deployment": "seeded compact defended areas and patrol platoons",
            "defense_centers": defense_centers.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),
            "infantry_group_centers":group_centers.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),
            "pacing": {"minimum_reinforcement_lull_s":crate::canyon_director::MIN_LULL_S,
                "reinforcement_platoon":crate::canyon_director::REINFORCEMENT_PLATOON},
            "start": [spec.start.x, spec.start.y], "goal": [spec.goal.x, spec.goal.y], "goal_radius_m": spec.goal_radius_m,
            "length_m": spec.length_m, "t34s": fielded, "platoon_tanks": winter_encounter_ai::PLATOON_TANKS,
            "patrols": self.winter_encounter.as_ref().map(|e|
                [e.route(0),e.route(1)].map(|route| route.iter().map(|p|p.to_array()).collect::<Vec<_>>())),
            "infantry_positions": free.iter().map(|&i|self.soldiers[i].pos.to_array()).collect::<Vec<_>>(),
            "infantry_groups": self.mixed_combat.squads.iter().map(|g|g.members.clone()).collect::<Vec<_>>(),
            "guns": zones, "riflemen_posted": posted, "rule": "src/canyon.rs, game/canyon_game.rs",
        });
        println!("[canyon] course fielded: {receipt}");
        self.canyon_siting = Some(receipt);
        let mut run = crate::canyon::CanyonRun::new(spec);
        run.director = Some(crate::canyon_director::Director::new(seed, fielded));
        self.canyon_run = Some(run);
        Ok(())
    }

    /// Advance the run and request reinforcements without altering any combatant's laws.
    pub(super) fn update_canyon_run(&mut self, dt: f32, r: &mut Renderer) {
        if self.canyon_run.is_none() { return; }
        if !std::mem::replace(&mut self.canyon_opening_fielded, true) {
            self.field_canyon_opening_platoons(r);
        }
        let alive = self.player.alive && self.game_over().is_none();
        let at = Vec2::new(self.player.pos.x, self.player.pos.z);
        let damage = self.modules.iter().filter(|m| !m.functional).count() as u32
            + self.crew.iter().filter(|c| !c.ok).count() as u32;
        let critical = !self.module_ok(ModuleKind::Engine) || !self.module_ok(ModuleKind::LeftTrack)
            || !self.module_ok(ModuleKind::RightTrack) || self.fire > 0.05;
        let live = self.enemies.iter().enumerate().filter(|(i,e)| e.alive && self.enemy_active[*i]).count();
        let engaged = self.enemies.iter().enumerate().filter(|(i,e)| e.alive && self.enemy_active[*i]
            && (self.stealth.spotted[*i] || e.pos.distance(self.player.pos) < 430.0)).count()
            + self.at_guns.iter().flatten().filter(|g|matches!(g.state,
                at_guns::GunState::Engaging | at_guns::GunState::ShiftingTrails)).count();
        let kills = self.enemies.iter().enumerate().filter(|(i,e)| !e.alive && self.enemy_active[*i]).count() as u32;
        let Some(run) = self.canyon_run.as_mut() else { return; };
        if !alive { run.failed = true; }
        if run.step(dt, (alive && self.mode == Mode::Tank).then_some(at)) {
            println!("[canyon] the Tiger reached the ravine's head in {:.1} s", run.elapsed_s);
            self.set_playtest_return_fire(false);
            return;
        }
        let s = crate::canyon_director::Situation {
            progress: crate::canyon_director::route_progress(&run.spec.route,at), live, engaged,
            kills, shots: self.gun_shots,
            hostile_shots: self.enemy_gun_shots.iter().sum::<u32>()
                + self.at_guns.iter().flatten().map(|g|g.shots).sum::<u32>(),
            damage, critical, alive: alive && !run.failed,
            finished: run.finished_s.is_some(),
        };
        let request = run.director.as_mut().and_then(|d| d.step(dt,s));
        if let Some(count) = request {
            if let Some((route, positions)) = self.canyon_reinforcement_entry(count,r.camera.pos) {
                self.append_canyon_platoon(route,positions,count,r);
                self.canyon_run.as_mut().unwrap().director.as_mut().unwrap().admitted(count);
            } else {
                self.canyon_run.as_mut().unwrap().director.as_mut().unwrap().deferred();
            }
        }
    }

    /// Entry candidates are fixed knots of the authored corridor. Choose only uncleared approaches
    /// ahead, with terrain/structure cover from BOTH the real eye and the Tiger. Fog and view angle
    /// never count as cover. A blocked or exposed map simply defers the request.
    fn canyon_reinforcement_entry(&self, count: usize, eye: Vec3) -> Option<(Vec<Vec2>,Vec<Vec2>)> {
        let run = self.canyon_run.as_ref()?;
        let director = run.director.as_ref()?;
        let nav = self.winter_encounter.as_ref()?.streets()?.nav();
        let source = &run.spec.route;
        let tiger = Vec2::new(self.player.pos.x,self.player.pos.z);
        let observer = Vec2::new(eye.x,eye.z);
        let mut entries: Vec<_> = (60..source.len().saturating_sub(12)).step_by(16)
            .filter(|&j| j as f32 / source.len() as f32 > director.progress + 0.035)
            .collect();
        entries.sort_by_key(|&j| crate::canyon::mix(director.seed ^ ((director.arrivals as u64)<<32) ^ j as u64));
        let in_range = entries.into_iter().filter(|&j|
            (crate::canyon_director::ENTRY_MIN_M..=1550.0).contains(&source[j].distance(tiger)));
        for j in in_range.take(ENTRY_CANDIDATES_PER_REQUEST) {
            if let Some(site) = self.canyon_platoon_site(j, count, Some(eye)) { return Some(site); }
        }
        None
    }

    /// A platoon's beat and start positions at corridor knot `j`: the beat runs back down the
    /// corridor to just short of where the Tiger is now, so the platoon comes to find the Tiger
    /// instead of circling its entry. It has no fix on the player: this is a patrol along the one
    /// road in. With `hidden_from` the start positions must be out of sight of that eye and of
    /// the Tiger and at least the director's entry distance away (a reinforcement arriving
    /// mid-run); without it they only need clear, connected ground (the opening deployment).
    fn canyon_platoon_site(&self, j: usize, count: usize, hidden_from: Option<Vec3>) -> Option<(Vec<Vec2>,Vec<Vec2>)> {
        let run = self.canyon_run.as_ref()?;
        let nav = self.winter_encounter.as_ref()?.streets()?.nav();
        let source = &run.spec.route;
        let tiger = Vec2::new(self.player.pos.x,self.player.pos.z);
        let tiger_knot = (0..source.len()).min_by(|&a,&b| source[a].distance_squared(tiger)
            .total_cmp(&source[b].distance_squared(tiger))).unwrap_or(0);
        let legs = (j.saturating_sub(tiger_knot + 8) / 10).clamp(5, 60);
        let route: Option<Vec<_>> = (0..=legs).filter(|k| j >= k*10).map(|k| source[j-k*10])
            .map(|p| nav.nearest_clear(p,12.0).filter(|&q|self.winter_patrol_point_clear(q))).collect();
        let route = route?;
        if route.len() < 2 { return None; }
        let direction = (route[1]-route[0]).normalize_or_zero();
        let positions: Option<Vec<_>> = (0..count).map(|k| {
            nav.nearest_clear(route[0]-direction*(k as f32*36.0),18.0)
                .filter(|&q|self.winter_patrol_point_clear(q))
        }).collect();
        let positions = positions?;
        let rejected = positions.iter().enumerate().any(|(i,p)| {
            positions[..i].iter().any(|q|p.distance(*q)<18.0)
            || self.enemies.iter().enumerate().any(|(k,e)|self.enemy_active[k]
                && p.distance(Vec2::new(e.pos.x,e.pos.z))<18.0)
            || hidden_from.is_some_and(|eye| {
                let observer = Vec2::new(eye.x,eye.z);
                p.distance(tiger)<crate::canyon_director::ENTRY_MIN_M
                || p.distance(observer)<crate::canyon_director::ENTRY_MIN_M
                || [Vec2::ZERO,Vec2::X*3.5,-Vec2::X*3.5,Vec2::Y*3.5,-Vec2::Y*3.5].iter().any(|off| {
                    let q=*p+*off;
                    let roof=Vec3::new(q.x,self.hf.height_at(q.x,q.y)+3.3,q.y);
                    self.los_clear(eye,roof) || self.los_clear(self.player.center()+Vec3::Y*2.0,roof)
                })
            })
        });
        if rejected { return None; }
        if positions.iter().any(|p| nav.path(*p,route[1]).is_none()) { return None; }
        // Path searches are the expensive part, so they run last and only for the first legs,
        // where the platoon forms up. A later corner the grid cannot reach is skipped by the
        // patrol itself (`WinterEncounter` walks on to the next). Searching every leg of a long
        // beat for every candidate entry stalled the frame for a third of a second.
        if route.windows(2).take(ROUTE_LEGS_SEARCHED).any(|edge|nav.path(edge[0],edge[1]).is_none()) {
            return None;
        }
        Some((route,positions))
    }

    /// Opening deployment along the whole corridor, drawn from the run seed: platoons at
    /// intervals from the first kilometre on, so no playtest minute is spent on empty ground.
    /// The authored plan fields its two platoons deep in the corridor; these fill the approach.
    fn field_canyon_opening_platoons(&mut self, r: &mut Renderer) {
        let Some((seed, knots)) = self.canyon_run.as_ref()
            .and_then(|run| Some((run.director.as_ref()?.seed, run.spec.route.len()))) else { return; };
        let tiger = Vec2::new(self.player.pos.x,self.player.pos.z);
        let mut fielded = 0usize;
        for (slot, fraction) in OPENING_PLATOON_FRACTIONS.iter().enumerate() {
            // A seeded shift of up to +/-3 percent of the route, then the nearest workable knot.
            let jitter = (crate::canyon::mix(seed ^ 0x6f70656e ^ slot as u64) % 61) as f32 / 1000.0 - 0.03;
            let centre = (((fraction + jitter).clamp(0.05, 0.95)) * knots as f32) as usize;
            let site = (0..12).flat_map(|step| [centre + step * 4, centre.saturating_sub(step * 4)])
                .filter(|&j| j >= 60 && j + 12 < knots)
                .find_map(|j| {
                    let run = self.canyon_run.as_ref()?;
                    if run.spec.route[j].distance(tiger) < OPENING_PLATOON_MIN_M { return None; }
                    self.canyon_platoon_site(j, crate::canyon_director::REINFORCEMENT_PLATOON, None)
                });
            if let Some((route, positions)) = site {
                let count = crate::canyon_director::REINFORCEMENT_PLATOON;
                self.append_canyon_platoon(route, positions, count, r);
                if let Some(d) = self.canyon_run.as_mut().and_then(|run| run.director.as_mut()) {
                    d.total_spawned += count as u32;
                }
                fielded += 1;
            }
        }
        println!("[canyon] opening deployment: {fielded} of {} approach platoons fielded",
            OPENING_PLATOON_FRACTIONS.len());
    }

    fn append_canyon_platoon(&mut self, route: Vec<Vec2>, positions: Vec<Vec2>, count: usize, r: &mut Renderer) {
        let heading=(route[1]-route[0]).x.atan2((route[1]-route[0]).y);
        let first=self.winter_encounter.as_mut().unwrap().append_platoon(route.clone(),count);
        assert_eq!(first,self.enemies.len());
        for k in 0..stealth::PLATOON_SIZE {
            let p=positions.get(k).copied().unwrap_or(route[0]);
            let mut tank=Tank::new(p.x,p.y,heading,&self.hf,tank::T34_PARTS);
            tank.alive=k<count; tank.reload=8.0+k as f32*3.0;
            self.enemies.push(tank);
            self.enemy_active.push(k<count); self.enemy_skill.push(38); self.enemy_concealed.push(false);
            self.enemy_gun_shots.push(0); self.enemy_mg_cd.push(0.0); self.wrecks.push(Wreck::default());
            for rigs in [&mut self.slots.t34,&mut self.slots.su100,&mut self.slots.kv2,
                &mut self.slots.isu152,&mut self.slots.is2,&mut self.slots.is3] {
                let source=rigs[0];
                rigs.push(source.map(|slot|r.clone_actor_mesh(slot)));
            }
            self.slots.flame.push(r.clone_actor_mesh(self.slots.flame[0]));
            self.stealth.meters.push(0.0); self.stealth.spotted.push(false);
        }
        let mut fresh=stealth::Stealth::new(stealth::PLATOON_SIZE);
        let mut platoon=fresh.platoons.remove(0);platoon.members=(first..first+stealth::PLATOON_SIZE).collect();
        self.stealth.platoons.push(platoon);
        println!("[canyon-director] {} T-34s enter at {:?}; identities {}..{}; ordinary patrol, no target fix",
            count,positions,first,first+count);
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
            let range=eye.distance(centre);
            let bearing=(k.at.x-gun.pos.x).atan2(k.at.y-gun.pos.z);
            let sector=(bearing-gun.sited+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
            range > at_guns::GROUND_M+5.0 && range <= at_guns::OPEN_FIRE_M
                && (range <= at_guns::POINT_BLANK_M || sector.abs() <= at_guns::SECTOR_HALF)
                && self.los_clear(eye, centre) && !self.box_region_blocked(Vec3::new(p.x - reach, p.y + 0.3, p.z - reach),
                Vec3::new(p.x + reach, p.y + 2.8, p.z + reach))
        }).min_by(|a, b| (eye.distance(Vec3::new(a.at.x, a.ground, a.at.y)) - range_m).abs()
            .total_cmp(&(eye.distance(Vec3::new(b.at.x, b.ground, b.at.y)) - range_m).abs()))
            .map(|k| (k.at, k.travel)).ok_or_else(|| anyhow::anyhow!("gun {g} sees no point of its choke's route"))?;
        self.debug_place_tiger(point, travel.x.atan2(travel.y));
        if let Some(gun) = self.at_guns[g].as_mut() { gun.heard = true; }
        if lay_on_gun { self.debug_lay_on_at_gun(g); }
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
            "elapsed_s": run.elapsed_s, "finished_s": run.finished_s, "failed":run.failed,
            "director":run.director.as_ref().map(|d|serde_json::json!({
                "seed":d.seed,"state":d.state,"progress":d.progress,"total_spawned":d.total_spawned,
                "arrivals":d.arrivals,"target_live":d.target_live,"credit":d.credit,
                "rest_until":d.rest_until,"next_check":d.next_check,"rejected_entries":d.rejected_entries,
                "pacing_reference_s":crate::canyon_director::TARGET_SECONDS,"live_cap":crate::canyon_director::MAX_LIVE,
            })),
            "tiger": [self.player.pos.x, self.player.pos.y, self.player.pos.z],
            "tiger_alive": self.player.alive,
            "goal_distance_m": Vec2::new(self.player.pos.x, self.player.pos.z).distance(run.spec.goal),
            "siting": self.canyon_siting,
            "winter": self.winter_encounter_snapshot(),
        }))
    }

    /// Film fixture (disclosed where used): lay the turret and gun on AT gun `g`'s shield, its upper
    /// part (the shield spans 0.30-1.18 m; what shows over a crest the gun sees across), with the
    /// Pzgr 39's ballistic aim. False when there is no such gun.
    pub fn debug_lay_on_at_gun(&mut self, g: usize) -> bool {
        let Some(gun) = self.at_guns.get(g).copied().flatten() else { return false; };
        let shield = gun.pos + Vec3::Y * 0.95;
        let aim = ballistic_aim_target(self.player.muzzle().0, shield, &crate::ballistics::PZGR_39);
        self.player.ai_aim(aim, 1000.0);
        self.lay_yaw = self.player.turret_yaw;
        self.lay_pitch = self.player.gun_pitch;
        true
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

    #[test]
    #[ignore = "requires GPU and generated Gaea canyon"]
    fn canyon_adaptive_entries_preserve_wrecks_and_render_new_actor_identities() -> anyhow::Result<()> {
        crate::act_manifest::select_runtime_manifest(Some(std::fs::canonicalize(crate::canyon::DEFAULT_MANIFEST)?))?;
        let instance=crate::create_instance();let adapter=crate::request_adapter(&instance,None);
        let(device,queue)=crate::request_device(&adapter);
        let(mut renderer,mut game)=Game::build_playtest_act(&device,&queue,false,&crate::world::act(1))?;
        game.mute_audio();game.enable_canyon_course_seeded(29)?;game.enable_singleplayer_crew();
        // Isolate armour movement from the fixed guns while preserving ordinary enemy AI.
        game.at_guns.fill(None);
        game.set_playtest_return_fire(true);
        assert!(game.canyon_paper.is_some());
        let route=game.canyon_spec.as_ref().unwrap().route.clone();
        let mut checks=Vec::new();let mut first_wreck=None;
        for index in [120usize,230,330,420] {
            game.debug_place_tiger(route[index],0.0);
            for i in 0..game.enemies.len() { if game.enemy_active[i] && game.enemies[i].alive {game.destroy_enemy(i);} }
            if first_wreck.is_none() {first_wreck=Some(game.enemies[0].pos);}
            // These are explicitly staged casualties and clock/credit advances, testing the live
            // admission + rendering path. They are not claimed player victories or timing evidence.
            let d=game.canyon_run.as_mut().unwrap().director.as_mut().unwrap();
            d.elapsed=1800.0;d.credit=4.0;d.rest_until=0.0;d.next_check=0.0;
            let before=game.enemies.len();
            game.update_canyon_run(0.1,&mut renderer);
            game.update_canyon_run(130.0,&mut renderer);
            game.update(&TankInput::default(),1.0/60.0,&mut renderer);
            renderer.render_offscreen(&device,&queue);
            assert_eq!(game.enemies[0].pos,first_wreck.unwrap(),"casualty was relocated");
            assert!(!game.enemies[0].alive && game.enemy_active[0],"wreck disappeared");
            let fresh:Vec<_>=(before..game.enemies.len()).filter(|&i|game.enemy_active[i]).collect();
            for &i in &fresh {
                assert!(game.winter_patrol_point_clear(Vec2::new(game.enemies[i].pos.x,game.enemies[i].pos.z)));
                assert!(game.enemies[i].pos.distance(game.player.pos)>=crate::canyon_director::ENTRY_MIN_M);
                assert!(!game.stealth.spotted[i]);
            }
            let initial: Vec<_>=fresh.iter().map(|&i|game.enemies[i].pos).collect();
            for _ in 0..600 {game.update(&TankInput::default(),0.1,&mut renderer);}
            let travelled:Vec<_>=fresh.iter().zip(&initial).map(|(&i,p)|game.enemies[i].pos.distance(*p)).collect();
            if !fresh.is_empty() {
                assert!(travelled[0]>3.0,"reinforcement leader did not enter under its ordinary AI: {travelled:?}");
            }
            checks.push(serde_json::json!({"route_index":index,"new_slots":fresh,"travelled_60s_m":travelled,"snapshot":game.canyon_snapshot()}));
        }
        let d=game.canyon_run.as_ref().unwrap().director.as_ref().unwrap();
        assert!(d.total_spawned>8,"real terrain never admitted sufficient reinforcements: {checks:?}");
        assert_eq!(game.enemies.len(),game.winter_encounter.as_ref().unwrap().slots());
        game.prepare_held_map(&mut renderer,true);renderer.ui_reset();
        assert!(game.draw_canyon_map(&mut renderer,&[(1,Vec2::splat(0.4))],&[],Vec2::splat(0.5)));
        renderer.render_held_map_texture(&device,&queue,game.held_map_slot());
        renderer.render_offscreen(&device,&queue);
        let backend=if crate::graphics::shader_backend_enabled(){"shader"}else{"physical"};
        std::fs::write(format!("captures/canyon_adaptive_{backend}_real_map_20260930.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "backend":backend,
            "staging":"forced casualties, fixed guns disabled, relocated Tiger, advanced pacing clock; real terrain admission, ordinary tank AI/motion, actors, wrecks and GPU render",
            "checks":checks,"final":game.canyon_snapshot()}))?)?;
        Ok(())
    }

    fn canyon_manifest() -> Option<std::path::PathBuf> {
        let path = std::path::PathBuf::from("assets/worldgen/workshops/canyon-course/manifest.v1.json");
        path.is_file().then_some(path)
    }

    #[test]
    #[ignore = "requires GPU and generated Gaea canyon"]
    fn gaea_canyon_route_drives_end_to_end_on_normal_vehicle_physics() -> anyhow::Result<()> {
        let path = std::fs::canonicalize(crate::canyon::DEFAULT_MANIFEST)?;
        let edits: serde_json::Value = serde_json::from_slice(&std::fs::read(path.with_file_name("manifest_edits.json"))?)?;
        let route: Vec<Vec2> = edits["canyon_course"]["route"].as_array().unwrap().iter()
            .map(|p|Vec2::new(p[0].as_f64().unwrap() as f32,p[1].as_f64().unwrap() as f32)).collect();
        crate::act_manifest::select_runtime_manifest(Some(path))?;
        let instance=crate::create_instance();
        let adapter=crate::request_adapter(&instance,None);
        let (device,queue)=crate::request_device(&adapter);
        let (mut renderer,mut game)=Game::build_playtest_act(&device,&queue,false,&crate::world::act(1))?;
        game.mute_audio();
        game.enable_canyon_course_seeded(29)?;
        game.enable_singleplayer_crew();
        game.jump_station(Station::Commander);
        // Route acceptance only: ordinary drive/steer, solid geometry and mortal vehicle;
        // defenders hold fire. This is not a claim of beating the armed course as a human.
        game.set_playtest_return_fire(false);
        assert!(!game.god);
        let pos=Vec2::new(game.player.pos.x,game.player.pos.z);
        let mut cursor=(0..route.len()).min_by(|&a,&b|route[a].distance_squared(pos).total_cmp(&route[b].distance_squared(pos))).unwrap();
        let mut last_progress=0usize;
        let mut samples=Vec::new();
        for step in 0..32_000usize {
            let pos=Vec2::new(game.player.pos.x,game.player.pos.z);
            let closest=(cursor..(cursor+12).min(route.len())).min_by(|&a,&b|
                route[a].distance_squared(pos).total_cmp(&route[b].distance_squared(pos))).unwrap();
            if closest>cursor { cursor=closest;last_progress=step; }
            let target=route[(cursor+1).min(route.len()-1)];
            let d=target-pos;
            let error=(d.x.atan2(d.y)-game.player.heading+std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
            let drive=if error.abs()>0.6 {0.16} else {0.60};
            game.update(&TankInput {drive,steer:(error*2.2).clamp(-1.0,1.0),..TankInput::default()},0.05,&mut renderer);
            if step%400==0 {
                println!("[canyon-drive] {:.0}% at {:?}, speed {:.2}",100.0*cursor as f32/route.len() as f32,game.player.pos,game.player.speed);
                samples.push(serde_json::json!({"time_s":step as f32*0.05,"route_point":cursor,"pos":game.player.pos.to_array(),"speed":game.player.speed}));
            }
            assert!(game.player.alive,"terrain killed the Tiger at {:?}",game.player.pos);
            assert!(step-last_progress<1200,"route stalled at point {cursor} / {}, pos {:?}, ground {:?}",route.len(),game.player.pos,game.tank_ground);
            if game.canyon_finished().is_some() {break;}
        }
        assert!(game.canyon_finished().is_some(),"route did not finish: {:?}",game.player.pos);
        let finish=game.canyon_snapshot();
        // Separately put a mortal Tiger in the first gun's observed kill zone and let the
        // ordinary Soviet gun and penetration simulation engage it; no damage is injected.
        game.enable_canyon_course_seeded(29)?;
        game.enable_singleplayer_crew();
        let stage=game.debug_stage_canyon_fight(0,220.0,false)?;
        for _ in 0..800 {game.update(&TankInput::default(),0.05,&mut renderer);if !game.player.alive {break;}}
        assert!(game.at_guns[0].is_some_and(|g|g.shots>0),"first gun never engaged: stage {stage}; {}",game.at_gun_snapshot());
        std::fs::write("captures/canyon_run_drive_verification_20260930.json",serde_json::to_vec_pretty(&serde_json::json!({
            "route_test":"ordinary drive/steer; return fire disabled; no god mode or warps",
            "samples":samples,"finish":finish,"armed_probe_staging":stage,"armed_probe":game.canyon_snapshot()
        }))?)?;
        Ok(())
    }

    #[test]
    #[ignore = "requires GPU and generated Gaea canyon"]
    fn gaea_canyon_seeds_field_complete_varied_forces_and_finish_only_alive() -> anyhow::Result<()> {
        let path = std::fs::canonicalize(crate::canyon::DEFAULT_MANIFEST)?;
        crate::act_manifest::select_runtime_manifest(Some(path))?;
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build_playtest_act(&device,&queue,false,&crate::world::act(1))?;
        game.mute_audio();
        let mut deployments = Vec::new();
        for seed in [11,29,73,101,503,2026,930,1984] {
            game.enable_canyon_course_seeded(seed)?;
            game.enable_singleplayer_crew();
            let receipt = game.canyon_siting.clone().unwrap();
            assert_eq!(receipt["t34s"],5);
            assert_eq!(game.at_guns.iter().flatten().count(),6,"seed {seed}: {receipt}");
            assert_eq!(receipt["riflemen_posted"],30);
            assert_eq!(game.soldiers.len(), CANYON_SOLDIERS, "Canyon has no allied practice squad");
            assert_eq!(game.mixed_combat.actors.len(), CANYON_SOLDIERS);
            let posted: Vec<_> = game.soldiers.iter().enumerate()
                .filter(|(i,s)| s.escort_enemy.is_none() && *i < at_guns::FIRST_CREW_SOLDIER)
                .map(|(i,s)|(i,s.pos)).collect();
            let start = game.player.pos;
            assert!(!game.god);
            for _ in 0..120 { game.update(&TankInput::default(),1.0/60.0,&mut renderer); }
            assert!(game.player.alive && game.at_guns.iter().flatten().all(|g|g.shots==0),"safe start seed {seed}");
            assert!(game.player.pos.distance(start)<1.0);
            for (i, post) in posted {
                assert!(game.soldiers[i].pos.distance(post) < 1.0, "seed {seed}: rifle post {i} drifted");
                assert!(game.mixed_combat.actors[i].pos.distance(game.soldiers[i].pos)<0.001,
                    "rendered soldier and combat actor disagree");
            }
            for (g, gun) in game.at_guns.iter().enumerate() {
                let gun=gun.as_ref().unwrap();
                for k in 0..at_guns::CREW {
                    assert!(game.soldiers[at_guns::FIRST_CREW_SOLDIER+g*at_guns::CREW+k].pos
                        .distance(gun.pos)<4.0, "seed {seed}: crew left gun {g}");
                }
            }
            deployments.push(receipt);
        }
        for pair in deployments.windows(2) {
            assert_ne!(pair[0]["guns"],pair[1]["guns"]);
            assert_ne!(pair[0]["patrols"],pair[1]["patrols"]);
            assert_ne!(pair[0]["infantry_positions"],pair[1]["infantry_positions"]);
        }
        game.enable_canyon_course_seeded(11)?;
        assert_eq!(game.canyon_siting.as_ref().unwrap(),&deployments[0],"seed reproduces exactly");
        let goal=game.canyon_spec.as_ref().unwrap().goal;
        game.debug_place_tiger(goal,0.0);
        game.player.alive=false;
        game.update_canyon_run(1.0/60.0, &mut renderer);
        assert!(game.canyon_finished().is_none());
        game.player.alive=true;
        game.update_canyon_run(1.0/60.0, &mut renderer);
        assert!(game.canyon_finished().is_none(),"death is terminal even if a debug fixture revives the hull");
        game.enable_canyon_course_seeded(11)?;
        game.debug_place_tiger(goal,0.0);
        game.update_canyon_run(1.0/60.0, &mut renderer);
        assert!(game.canyon_finished().is_some());
        game.enable_canyon_course_seeded(73)?;
        assert!(game.canyon_finished().is_none() && game.player.alive);
        // Walking outside a sound Tiger is legal; dying there is just as terminal as hull loss.
        game.mode=Mode::Foot;game.foot.hp=100.0;
        game.update_canyon_run(1.0,&mut renderer);
        assert!(!game.canyon_run.as_ref().unwrap().failed && game.game_over().is_none());
        let elapsed=game.canyon_run.as_ref().unwrap().elapsed_s;
        let director_elapsed=game.canyon_run.as_ref().unwrap().director.as_ref().unwrap().elapsed;
        game.foot.hp=0.0;
        game.update_canyon_run(1.0,&mut renderer);
        assert!(matches!(game.game_over(),Some(DeathCause::OnFoot)));
        assert!(game.canyon_run.as_ref().unwrap().failed);
        assert_eq!(game.canyon_run.as_ref().unwrap().elapsed_s,elapsed);
        assert_eq!(game.canyon_run.as_ref().unwrap().director.as_ref().unwrap().elapsed,director_elapsed);
        game.foot.hp=100.0;game.mode=Mode::Tank;game.debug_place_tiger(goal,0.0);
        game.update_canyon_run(1.0,&mut renderer);
        assert!(game.canyon_finished().is_none(),"a debug revival cannot resume a spent attempt");
        game.enable_canyon_course_seeded(73)?;
        game.mode=Mode::Foot;game.foot.hp=100.0;game.player.alive=false;
        assert!(game.game_over().is_some(),"losing the Canyon Tiger while outside must end the run");
        game.update_canyon_run(1.0,&mut renderer);
        assert!(game.canyon_run.as_ref().unwrap().failed);
        std::fs::write("captures/canyon_run_seed_verification_20260930.json",serde_json::to_vec_pretty(&deployments)?)?;
        Ok(())
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
