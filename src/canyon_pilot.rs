//! Explicit offscreen AI test driver, never installed in human play. Route knowledge comes from
//! the paper-map course. Contacts use geometry-based sight within 420 m; no hidden-contact hunt.
//! It supplies ordinary drive/mouse/trigger input and native station actions. Reloading is the
//! physical interior walk, lever, casing, rack and ram sequence from the player-input showcase.
use super::*;

pub struct CanyonPilot {
    route: Vec<Vec2>,
    cursor: usize,
    mouse_velocity: Vec2,
    reload_at: Option<f32>,
    reload_waypoint: usize,
    primary_held: bool,
    last_check: f32,
    check_pos: Vec2,
    reverse_until: f32,
    /// Route watchdog: the farthest waypoint reached and when, plus the escalating
    /// alternating-side detour state used to leave a bump-and-retreat loop.
    best_cursor: usize,
    best_cursor_at: f32,
    stuck_count: u32,
    avoid_until: f32,
    avoid_side: f32,
    was_advancing: bool,
    /// Attempts at the lever-and-seat tail of one reload before the pilot drives on with the
    /// gun unusable (a stalled ritual used to hold the attempt forever).
    reload_retries: u32,
    pub reloads: u32,
    pub phase: &'static str,
}

impl CanyonPilot {
    pub fn new(g: &Game) -> Self {
        let route = g
            .canyon_spec
            .as_ref()
            .expect("Canyon AI needs the real course")
            .route
            .clone();
        Self::from_route(route, Vec2::new(g.player.pos.x, g.player.pos.z))
    }

    fn from_route(route: Vec<Vec2>, pos: Vec2) -> Self {
        assert!(!route.is_empty(), "Canyon pilot needs a route");
        let cursor = (0..route.len())
            .min_by(|&a, &b| {
                route[a]
                    .distance_squared(pos)
                    .total_cmp(&route[b].distance_squared(pos))
            })
            .unwrap();
        Self {
            route,
            cursor,
            mouse_velocity: Vec2::ZERO,
            reload_at: None,
            reload_waypoint: 0,
            primary_held: false,
            last_check: 0.0,
            check_pos: pos,
            reverse_until: 0.0,
            best_cursor: cursor,
            best_cursor_at: 0.0,
            stuck_count: 0,
            avoid_until: 0.0,
            avoid_side: -1.0,
            was_advancing: false,
            reload_retries: 0,
            reloads: 0,
            phase: "approach",
        }
    }

    /// Escalate out of a blocked spot: back off in an arc, then approach past the obstruction
    /// through a perpendicular offset carrot. Sides alternate and both the reverse and the
    /// detour grow with each consecutive failure; any real route progress resets the ladder.
    fn trigger_avoid(&mut self, t: f32) {
        self.stuck_count = (self.stuck_count + 1).min(6);
        self.avoid_side = -self.avoid_side;
        let grade = self.stuck_count.min(4) as f32;
        self.reverse_until = t + 2.5 + 0.7 * grade;
        self.avoid_until = self.reverse_until + 8.0 + 2.5 * grade;
        self.best_cursor_at = t;
    }

    fn look(&mut self, g: &Game, r: &Renderer, target: Vec3, dt: f32, input: &mut TankInput) {
        let (error, sensitivity) = g.capture_gaze_error(target, r);
        let gain = if g.station == Station::Gunner {
            12.0
        } else {
            5.0
        };
        let desired = (error * gain).clamp(Vec2::splat(-1.9), Vec2::splat(1.9));
        self.mouse_velocity +=
            (desired - self.mouse_velocity).clamp(Vec2::splat(-5.0 * dt), Vec2::splat(5.0 * dt));
        let delta = self.mouse_velocity * dt;
        input.look_dx += delta.x / sensitivity.x;
        input.look_dy += delta.y / sensitivity.y;
    }

    fn reload(&mut self, g: &mut Game, r: &Renderer, t: f32, dt: f32) -> TankInput {
        let mut input = TankInput::default();
        // Walk around the back of the breech before reaching the loader's side. A straight
        // leftward command from the eyepiece is physically blocked by the gun assembly.
        let waypoints = [
            Vec2::new(0.6, -0.6),
            Vec2::new(-0.6, -0.6),
            Vec2::new(-0.6, 0.1),
        ];
        if self.reload_waypoint < waypoints.len() {
            self.phase = "walk_to_breech";
            let target = waypoints[self.reload_waypoint];
            let (d, s) = g.capture_walk_input(target);
            input.drive = d;
            input.steer = s;
            if let Some(at) = g.capture_gaze_target(&serde_json::json!({"gaze":"cupola"})) {
                self.look(g, r, at, dt, &mut input);
            }
            if Vec2::new(g.walk_pos.x, g.walk_pos.z).distance(target) < 0.04 {
                self.reload_waypoint += 1;
            }
            self.reload_at = Some(t);
            return input;
        }
        let a = t - self.reload_at.unwrap();
        self.phase = "manual_reload";
        let mut gaze = None;
        let mut walk = None;
        if a < 1.8 {
            gaze = Some("lever");
            walk = Some(Vec2::new(-0.6, 0.1));
        } else if a < 2.0 {
            input.mg_coax = g.lever_open < LEVER_OPEN;
        } else if a < 3.7 {
            gaze = Some("breech");
        } else if a < 4.9 {
            input.mg_coax = true;
        } else if a < 7.1 {
            gaze = Some("rack_ap");
        } else if a < 12.0 {
            input.mg_coax = true;
            if a < 9.9 {
                gaze = Some("tray");
            } else if a < 9.975 {
                input.look_dy = -4.0 * dt * 120.0;
            }
        } else if a < 13.5 {
            gaze = Some("lever");
        } else if a < 13.7 {
            input.mg_coax = g.lever_open >= LEVER_OPEN;
        } else if a < 15.3 {
            gaze = Some("breech");
        } else if a < 16.5 {
            gaze = Some("cupola");
            walk = Some(Vec2::new(-0.6, -0.6));
        } else if a < 18.0 {
            gaze = Some("gunner");
            walk = Some(Vec2::new(0.6, -0.6));
        } else if a < 19.0 {
            gaze = Some("gunner");
            walk = Some(Vec2::new(0.6, 0.2));
        } else if a < 19.2 {
            input.mg_coax = g.station == Station::Loader;
        } else {
            if g.breech_loaded() && g.lever_open < LEVER_OPEN && g.station == Station::Gunner {
                self.reloads += 1;
                self.reload_retries = 0;
                self.reload_at = None;
                self.mouse_velocity = Vec2::ZERO;
            } else if self.reload_retries < 3 {
                // The lever did not close or the seat was not taken: back to the lever (the
                // ritual from 11 s on: gaze lever, close it, breech, cupola, the gunner's seat).
                self.reload_retries += 1;
                self.phase = "reload_retry";
                println!("[canyon-pilot] reload incomplete at {t:.1}s (loaded {}, lever {:.2}, station {:?}); retry {}",
                    g.breech_loaded(), g.lever_open, g.station, self.reload_retries);
                self.reload_at = Some(t - 11.0);
            } else {
                // Give the attempt back to the road rather than holding it at the breech.
                self.phase = "reload_stalled";
                println!("[canyon-pilot] reload abandoned at {t:.1}s (loaded {}, lever {:.2}, station {:?}); driving on",
                    g.breech_loaded(), g.lever_open, g.station);
                self.reload_retries = 0;
                self.reload_at = None;
                self.mouse_velocity = Vec2::ZERO;
            }
        }
        if let Some(name) = gaze {
            if let Some(target) = g.capture_gaze_target(&serde_json::json!({"gaze":name})) {
                self.look(g, r, target, dt, &mut input);
            }
        } else {
            self.mouse_velocity = Vec2::ZERO;
        }
        if let Some(target) = walk {
            let (d, s) = g.capture_walk_input(target);
            input.drive = d;
            input.steer = s;
        }
        input
    }

    pub fn step(&mut self, g: &mut Game, r: &Renderer, t: f32, dt: f32) -> TankInput {
        if g.game_over().is_some() || g.canyon_finished().is_some() {
            self.phase = "ended";
            return TankInput::default();
        }
        let target = g.canyon_pilot_visible_target();
        let mut input = if self.reload_at.is_some() {
            self.reload(g, r, t, dt)
        } else if !g.breech_loaded() {
            // Same Q action as the native gunner: leave the sight for the free-walk interior.
            g.sights_mag = 0;
            g.commander_outside = false;
            g.jump_station(Station::Loader);
            self.reload_waypoint = if g.walk_pos.x < 0.0 { 2 } else { 0 };
            self.reload_at = Some(t);
            self.mouse_velocity = Vec2::ZERO;
            self.reload(g, r, t, dt)
        } else if let Some((at, gun)) = target.filter(|_| g.lever_open < LEVER_OPEN) {
            // Only a gun that can fire is worth stopping for; with the lever still open after an
            // abandoned reload the pilot drives on rather than standing in the open.
            self.phase = "engage";
            g.commander_outside = false;
            g.request_station(Station::Gunner);
            let mut input = TankInput::default();
            if g.station_ready(Station::Gunner) {
                self.look(g, r, at, dt, &mut input);
                input.mg_coax = g.capture_aim_ready(at) && g.auto_can_fire();
                input.coax = gun.is_some() && g.capture_aim_ready(at);
            }
            input
        } else {
            self.phase = "advance";
            if g.station_ready(Station::Gunner) {
                g.request_station(Station::Loader);
            } else if g.station_ready(Station::Loader) {
                g.request_station(Station::Commander);
            }
            if !g.hatch_is_open() {
                g.toggle_hatch();
            }
            g.commander_outside = g.station_ready(Station::Commander) && g.hatch_t > 0.95;
            let here = Vec2::new(g.player.pos.x, g.player.pos.z);
            let close = (self.cursor.saturating_sub(3)..(self.cursor + 12).min(self.route.len()))
                .min_by(|&a, &b| {
                    self.route[a]
                        .distance_squared(here)
                        .total_cmp(&self.route[b].distance_squared(here))
                })
                .unwrap();
            self.cursor = self.cursor.max(close);
            // Route watchdog. Combat and reloads pause the advance; only advancing time counts.
            if !self.was_advancing {
                self.best_cursor_at = t;
                self.last_check = t;
                self.check_pos = here;
            }
            if self.cursor > self.best_cursor {
                self.best_cursor = self.cursor;
                self.best_cursor_at = t;
                if t >= self.avoid_until {
                    self.stuck_count = 0;
                }
            } else if t - self.best_cursor_at > 30.0 && t >= self.avoid_until {
                self.trigger_avoid(t);
            }
            let mut carrot = self.route[(self.cursor + 1).min(self.route.len() - 1)];
            if t >= self.reverse_until && t < self.avoid_until {
                // Approach past the obstruction: offset the carrot perpendicular to the local
                // route direction, farther out on each consecutive failure.
                let a = self.route[self.cursor.min(self.route.len() - 1)];
                let b = self.route[(self.cursor + 2).min(self.route.len() - 1)];
                let along = (b - a).normalize_or_zero();
                let perp = Vec2::new(-along.y, along.x);
                carrot += perp * self.avoid_side * (16.0 + 8.0 * self.stuck_count.min(4) as f32);
                self.phase = "detouring";
            }
            let mut input = navigation_input(carrot - here, g.player.heading);
            if t - self.last_check > 4.0 {
                if input.drive > 0.1 && here.distance(self.check_pos) < 2.0 && t >= self.avoid_until {
                    self.trigger_avoid(t);
                }
                self.check_pos = here;
                self.last_check = t;
            }
            if t < self.reverse_until {
                self.phase = "backing_off";
                input.drive = -0.55;
                // Arc the reverse so the hull rotates away from the blocked line instead of
                // backing straight down it and driving straight back in.
                input.steer = -self.avoid_side * 0.45;
            }
            let look_at = self.route[(self.cursor + 12).min(self.route.len() - 1)];
            self.look(
                g,
                r,
                Vec3::new(
                    look_at.x,
                    g.hf.height_at(look_at.x, look_at.y) + 2.7,
                    look_at.y,
                ),
                dt,
                &mut input,
            );
            input
        };
        if g.mode == Mode::Tank { (input.drive,input.steer)=g.safe_ai_tiger_drive(input.drive,input.steer); }
        self.was_advancing = matches!(self.phase, "advance" | "backing_off" | "detouring");
        // Mirror native primary-click dispatch. Held ammunition is owned by Game::update.
        if input.mg_coax && !self.primary_held {
            let eye = r.camera.pos;
            let forward = r.camera.forward();
            if g.click_articulated_hatch() {
            } else if g.primary_loader_owns_click() {
            } else if g.interact_world_door(forward, eye) || g.interact_portable_item(forward, eye)
            {
                g.consume_primary_press();
            } else if g.interact_hatch_lever(forward, eye) || g.click_station() {
            } else if g.hover_hatch() {
                g.click_hatch_or_exit();
            }
        }
        self.primary_held = input.mg_coax;
        if !g.engine_on && self.was_advancing {
            input.ignition = true;
        }
        input
    }
}

/// Pivot before accelerating into a new heading. Forward creep during a large turn cuts across
/// the obstruction the recovery is trying to clear. Tank::update supplies normal braking inertia.
fn navigation_input(to: Vec2, heading: f32) -> TankInput {
    let error = (to.x.atan2(to.y) - heading + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    TankInput {
        drive: if error.abs() > 0.6 { 0.0 } else { 0.60 },
        steer: (error * 2.2).clamp(-1.0, 1.0),
        ..Default::default()
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    #[test]
    fn turn_to_detour_brakes_before_accelerating() {
        let turning = navigation_input(Vec2::X * 24.0, 0.0);
        assert_eq!(turning.drive, 0.0);
        assert_eq!(turning.steer, 1.0);
        let aligned = navigation_input(Vec2::X * 24.0, std::f32::consts::FRAC_PI_2);
        assert_eq!(aligned.drive, 0.6);
        assert!(aligned.steer.abs() < 1e-5);
    }

    #[test]
    fn steering_takes_short_arc_across_heading_wrap() {
        let angle = -std::f32::consts::PI + 0.1;
        let input = navigation_input(Vec2::new(angle.sin(), angle.cos()), std::f32::consts::PI - 0.1);
        assert!(input.steer > 0.0 && input.steer < 0.5);
        assert!(input.drive > 0.0);
    }

    #[test]
    fn repeated_stalls_alternate_and_bound_recovery() {
        let mut pilot = CanyonPilot::from_route(vec![Vec2::ZERO, Vec2::Y * 100.0], Vec2::ZERO);
        let mut last_side = pilot.avoid_side;
        let mut last_duration = 0.0;
        for i in 0..20 {
            let t = i as f32 * 100.0;
            pilot.trigger_avoid(t);
            assert_eq!(pilot.avoid_side, -last_side);
            let duration = pilot.avoid_until - t;
            assert!(duration >= last_duration - 0.001 && duration <= 23.301);
            assert!(pilot.reverse_until > t && pilot.reverse_until < pilot.avoid_until);
            last_side = pilot.avoid_side;
            last_duration = duration;
        }
    }
}

impl Game {
    fn canyon_pilot_visible_target(&self) -> Option<(Vec3, Option<usize>)> {
        let eye = self.player.center() + Vec3::Y * 1.1;
        self.enemies
            .iter()
            .enumerate()
            .filter(|(i, e)| self.enemy_active[*i] && e.alive)
            .filter_map(|(i, e)| {
                self.capture_gaze_target(&serde_json::json!({"gaze":"enemy","enemy":i}))
                    .map(|p| (p, None, e.center()))
            })
            .chain(self.at_guns.iter().enumerate().filter_map(|(i, g)| {
                g.filter(|g| g.working())
                    .map(|g| (g.pos + Vec3::Y * 0.6, Some(i), g.pos + Vec3::Y * 0.6))
            }))
            .filter(|(at, _, centre)| {
                eye.distance(*centre) < 420.0
                    && self.optical_los_clear(eye, *centre)
                    && self
                        .street_line_of_fire_clear(eye + (*at - eye).normalize_or_zero() * 5.0, *at)
                    && self.rubble_segment_contact(eye, *at).is_none()
            })
            .min_by(|a, b| {
                a.0.distance_squared(eye)
                    .total_cmp(&b.0.distance_squared(eye))
            })
            .map(|(p, g, _)| (p, g))
    }

    pub fn canyon_ai_state(&self, pilot: &CanyonPilot, t: f32) -> serde_json::Value {
        let run = self.canyon_run.as_ref().unwrap();
        serde_json::json!({"time_s":t,"phase":pilot.phase,"position":self.player.pos.to_array(),
            "heading":self.player.heading,"speed_m_s":self.player.speed,"station":format!("{:?}",self.station),
            "progress":crate::canyon_director::route_progress(&run.spec.route,Vec2::new(self.player.pos.x,self.player.pos.z)),
            "alive":self.player.alive,"death_cause":self.game_over().map(|c|format!("{c:?}")),
            "finished_s":run.finished_s,"shots":self.gun_shots,"kills":self.stat_kills_by_player,
            "hits":self.stat_hits_by_player,"incoming_hits":self.stat_hits_on_player,"incoming_penetrations":self.stat_pens_on_player,
            "ammo":remaining_main_gun_rounds(&self.slot_taken,self.breech),"reloads":pilot.reloads,
            "breech":format!("{:?}",self.breech),"lever":self.lever_open,"carrying":self.grab,
            "walk":self.walk_pos.to_array(),"broken_modules":self.modules.iter().filter(|m|!m.functional).map(|m|m.kind.name()).collect::<Vec<_>>(),
            "live_t34s":self.campaign_ai_enemies_alive(),"total_t34s":run.director.as_ref().unwrap().total_spawned,
            "combat":self.capture_combat_receipt(),"live_at_guns":self.at_guns.iter().flatten().filter(|g|g.working()).count(),
            "navigation":{"cursor":pilot.cursor,"recoveries":pilot.stuck_count,"side":pilot.avoid_side,"reverse_until":pilot.reverse_until,"avoid_until":pilot.avoid_until},
            "ground":format!("{:?}",self.tank_ground_state()),
            "god":self.god,"return_fire":self.playtest_return_fire(),"target_in_sight":self.canyon_pilot_visible_target().map(|(p,g)|serde_json::json!({"position":p.to_array(),"gun":g}))})
    }
}
