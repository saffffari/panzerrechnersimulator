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
        let pos = Vec2::new(g.player.pos.x, g.player.pos.z);
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
            reloads: 0,
            phase: "approach",
        }
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
                self.reload_at = None;
                self.mouse_velocity = Vec2::ZERO;
            } else {
                self.phase = "reload_stalled";
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
        } else if let Some((at, gun)) = target {
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
            let carrot = self.route[(self.cursor + 1).min(self.route.len() - 1)];
            let to = carrot - here;
            let error = (to.x.atan2(to.y) - g.player.heading + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let mut input = TankInput {
                drive: if error.abs() > 0.6 { 0.16 } else { 0.60 },
                steer: (error * 2.2).clamp(-1.0, 1.0),
                ..Default::default()
            };
            if t - self.last_check > 12.0 {
                if here.distance(self.check_pos) < 2.0 {
                    self.reverse_until = t + 3.0;
                }
                self.check_pos = here;
                self.last_check = t;
            }
            if t < self.reverse_until {
                self.phase = "backing_off";
                input.drive = -0.55;
                input.steer = 0.0;
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
        if !g.engine_on && self.phase == "advance" {
            input.ignition = true;
        }
        input
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
                    && self.los_clear(eye, *centre)
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
            "god":self.god,"return_fire":self.playtest_return_fire(),"target_in_sight":self.canyon_pilot_visible_target().map(|(p,g)|serde_json::json!({"position":p.to_array(),"gun":g}))})
    }
}
