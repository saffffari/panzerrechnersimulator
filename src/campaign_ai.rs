//! Mission-level Tiger AI and full-run balance telemetry.
//!
//! The existing evolved MLP remains a close-combat driver/gunner.  This controller sits above it
//! and owns the long-horizon campaign ritual: travel to the next generated military site, clear
//! its armour, park, dismount, recover the physical map sheet, return to the same Tiger, remount,
//! and finally drive to the newly revealed exit.  Keeping the two levels separate prevents route
//! distance and on-foot state from destabilising a combat policy with a deliberately tiny input.

#![allow(dead_code)] // The live runner is adopted incrementally by the headless campaign harness.

use glam::Vec2;
use serde_json::json;

const SITE_PARK_RADIUS_M: f32 = 18.0;
const MAP_USE_RADIUS_M: f32 = 1.85;
const MOUNT_RADIUS_M: f32 = 3.7;
const STALL_SECONDS: f32 = 45.0;
const RECOVERY_REVERSE_SECONDS: f32 = 2.6;
const RECOVERY_PIVOT_SECONDS: f32 = 1.8;
const RECOVERY_SECONDS: f32 = RECOVERY_REVERSE_SECONDS + RECOVERY_PIVOT_SECONDS;
const MAX_RECOVERY_ATTEMPTS: u8 = 4;
const TANK_PROGRESS_M: f32 = 12.0;
const RECOVERY_RESET_ADVANCE_M: f32 = 90.0;
const ESCAPE_BACKOFF_M: f32 = 30.0;
const ESCAPE_BASE_LATERAL_M: f32 = 65.0;
const ESCAPE_LATERAL_STEP_M: f32 = 65.0;
const ESCAPE_REACHED_RADIUS_M: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorMode {
    Tank,
    Foot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignAiPhase {
    Transit,
    Fight,
    Park,
    Dismount,
    FootToMap,
    SearchMap,
    ReturnToTiger,
    Remount,
    Exit,
    Recovery,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub struct CampaignAiObservation {
    pub time_s: f32,
    pub mode: ActorMode,
    pub tiger_alive: bool,
    pub foot_alive: bool,
    pub tank_pos: Vec2,
    /// Tank forward is `(sin(heading), cos(heading))`.
    pub tank_heading: f32,
    pub tank_speed_mps: f32,
    pub foot_pos: Vec2,
    /// Foot forward is `(sin(yaw), -cos(yaw))`.
    pub foot_yaw: f32,
    pub enemies_alive: usize,
    /// True when either an armoured crew or an infantryman currently has the dismounted player.
    pub foot_detected: bool,
    pub map_sheets: u8,
    pub next_guard_clear: bool,
    pub next_map_site: Option<Vec2>,
    pub active_map_case: Option<Vec2>,
    pub generated_exit: Vec2,
    pub exit_reached: bool,
    pub can_dismount: bool,
    pub can_mount: bool,
}

impl CampaignAiObservation {
    fn actor_pos(self) -> Vec2 {
        match self.mode {
            ActorMode::Tank => self.tank_pos,
            ActorMode::Foot => self.foot_pos,
        }
    }
}

/// High-level intent. `delegate_combat` asks the existing Tiger combat pilot to fill tank motion,
/// gun lay and trigger state; all other fields are deterministic expert actions suitable for
/// recording an imitation-learning curriculum.
#[derive(Clone, Copy, Debug, Default)]
pub struct CampaignAiCommand {
    pub tank_drive: f32,
    pub tank_steer: f32,
    pub delegate_combat: bool,
    pub toggle_dismount: bool,
    pub foot_forward: f32,
    pub foot_strafe: f32,
    pub foot_target_yaw: Option<f32>,
    pub crouch: bool,
    pub search_map: bool,
    pub mount_tiger: bool,
}

#[derive(Clone, Debug)]
pub struct CampaignAiController {
    phase: CampaignAiPhase,
    phase_before_fight: CampaignAiPhase,
    last_sheet_count: u8,
    last_progress_pos: Vec2,
    last_progress_time_s: f32,
    progress_target: Option<Vec2>,
    best_target_distance: f32,
    recovery_started_s: f32,
    recovery_until_s: f32,
    recovery_attempts: u8,
    recovery_turn_sign: f32,
    recovery_anchor: Vec2,
    recovery_target_distance: f32,
    escape_waypoint: Option<Vec2>,
}

impl CampaignAiController {
    pub fn new(initial: CampaignAiObservation) -> Self {
        let progress_target = route_target(initial);
        Self {
            phase: CampaignAiPhase::Transit,
            phase_before_fight: CampaignAiPhase::Transit,
            last_sheet_count: initial.map_sheets,
            last_progress_pos: initial.actor_pos(),
            last_progress_time_s: initial.time_s,
            progress_target,
            best_target_distance: progress_target
                .map(|target| initial.tank_pos.distance(target))
                .unwrap_or(f32::INFINITY),
            recovery_started_s: 0.0,
            recovery_until_s: 0.0,
            recovery_attempts: 0,
            recovery_turn_sign: 1.0,
            recovery_anchor: initial.tank_pos,
            recovery_target_distance: f32::INFINITY,
            escape_waypoint: None,
        }
    }

    pub fn phase(&self) -> CampaignAiPhase {
        self.phase
    }

    pub fn step(&mut self, obs: CampaignAiObservation) -> CampaignAiCommand {
        if !obs.tiger_alive || (obs.mode == ActorMode::Foot && !obs.foot_alive) {
            self.phase = CampaignAiPhase::Failed;
            return CampaignAiCommand::default();
        }
        if obs.exit_reached && obs.map_sheets >= 3 && obs.mode == ActorMode::Tank {
            self.phase = CampaignAiPhase::Complete;
            return CampaignAiCommand::default();
        }

        let actor = obs.actor_pos();
        let recovered_sheet = obs.map_sheets > self.last_sheet_count;
        if obs.mode == ActorMode::Foot {
            if actor.distance(self.last_progress_pos) >= 8.0 || recovered_sheet {
                self.last_progress_pos = actor;
                self.last_progress_time_s = obs.time_s;
            }
        } else {
            // Displacement alone is not route progress: seeds 4/6 circled one city block forever
            // while repeatedly moving eight metres. Measure reduction toward the actual active
            // waypoint so orbiting and wall-sliding still trip the recovery watchdog.
            let target = self.escape_waypoint.or_else(|| route_target(obs));
            let target_changed = match (self.progress_target, target) {
                (Some(previous), Some(next)) => previous.distance_squared(next) > 1.0,
                (None, None) => false,
                _ => true,
            };
            if recovered_sheet || target_changed {
                self.progress_target = target;
                self.best_target_distance = target
                    .map(|next| obs.tank_pos.distance(next))
                    .unwrap_or(f32::INFINITY);
                self.last_progress_time_s = obs.time_s;
            } else if let Some(next) = target {
                let distance = obs.tank_pos.distance(next);
                if distance + TANK_PROGRESS_M < self.best_target_distance {
                    self.best_target_distance = distance;
                    self.last_progress_time_s = obs.time_s;
                }
            }
        }
        self.last_sheet_count = obs.map_sheets;
        if recovered_sheet {
            self.recovery_attempts = 0;
            self.escape_waypoint = None;
            self.recovery_anchor = obs.tank_pos;
            self.recovery_target_distance = f32::INFINITY;
        } else if obs.mode == ActorMode::Tank && self.recovery_attempts > 0 {
            if let Some(target) = route_target(obs) {
                let distance = obs.tank_pos.distance(target);
                if distance + RECOVERY_RESET_ADVANCE_M < self.recovery_target_distance {
                    // Lateral motion is not enough: retain escalation until the Tiger has advanced
                    // well beyond the obstructing block toward its real campaign destination.
                    self.recovery_attempts = 0;
                    self.recovery_anchor = obs.tank_pos;
                    self.recovery_target_distance = distance;
                }
            }
        }
        if recovered_sheet && obs.mode == ActorMode::Foot {
            self.phase = CampaignAiPhase::ReturnToTiger;
        }

        // Any live cordon interrupts route execution. Once it is clear, resume the exact physical
        // task that was interrupted rather than resetting the whole route ritual.
        if obs.enemies_alive > 0 && obs.mode == ActorMode::Tank {
            if self.phase != CampaignAiPhase::Fight {
                self.phase_before_fight = self.phase;
            }
            self.phase = CampaignAiPhase::Fight;
            // Combat manoeuvre is owned by the evolved pilot, not the route watchdog. Restart the
            // route clock after each live-combat frame so a long engagement cannot immediately
            // trigger an obstacle recovery when the final enemy dies.
            self.last_progress_time_s = obs.time_s;
            return CampaignAiCommand {
                delegate_combat: true,
                ..Default::default()
            };
        }
        if self.phase == CampaignAiPhase::Fight {
            self.phase = if obs.next_guard_clear {
                CampaignAiPhase::Park
            } else {
                self.phase_before_fight
            };
            let target = self.escape_waypoint.or_else(|| route_target(obs));
            self.progress_target = target;
            self.best_target_distance = target
                .map(|next| obs.tank_pos.distance(next))
                .unwrap_or(f32::INFINITY);
            self.last_progress_time_s = obs.time_s;
        }

        // A deterministic reverse-and-turn pulse gets training rollouts out of trees, rubble and
        // tight courtyards. It is intentionally above the learned combat policy and is labelled in
        // telemetry so recovery examples can be excluded from imitation data if desired.
        if obs.mode == ActorMode::Tank
            && !matches!(
                self.phase,
                CampaignAiPhase::Dismount
                    | CampaignAiPhase::Remount
                    | CampaignAiPhase::Complete
                    | CampaignAiPhase::Failed
            )
            && obs.time_s - self.last_progress_time_s > STALL_SECONDS
        {
            if self.recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
                // Repeating reverse pulses forever used to turn a destroyed transmission into a
                // 900-second route timeout. Four alternating, widening attempts distinguish a hard
                // immobilisation from ordinary city clutter without consulting hidden damage state.
                self.phase = CampaignAiPhase::Failed;
                return CampaignAiCommand::default();
            }
            self.recovery_attempts += 1;
            self.recovery_turn_sign = if self.recovery_attempts % 2 == 1 {
                1.0
            } else {
                -1.0
            };
            self.recovery_anchor = obs.tank_pos;
            self.recovery_target_distance = route_target(obs)
                .map(|target| obs.tank_pos.distance(target))
                .unwrap_or(f32::INFINITY);
            self.escape_waypoint = None;
            self.phase = CampaignAiPhase::Recovery;
            self.recovery_started_s = obs.time_s;
            self.recovery_until_s = obs.time_s + RECOVERY_SECONDS;
            self.last_progress_time_s = obs.time_s;
        }
        if self.phase == CampaignAiPhase::Recovery {
            let elapsed = (obs.time_s - self.recovery_started_s).max(0.0);
            if elapsed < RECOVERY_REVERSE_SECONDS {
                return CampaignAiCommand {
                    tank_drive: -0.82,
                    tank_steer: self.recovery_turn_sign * 0.86,
                    ..Default::default()
                };
            }
            if obs.time_s < self.recovery_until_s {
                // The simple Tank integrator already supports neutral pivoting. Rotating the bow
                // clear before applying forward drive prevents it from immediately re-entering the
                // same brick face that caused the stall.
                return CampaignAiCommand {
                    tank_steer: self.recovery_turn_sign,
                    ..Default::default()
                };
            }
            if let Some(target) = route_target(obs) {
                self.escape_waypoint = Some(recovery_escape_waypoint(
                    self.recovery_anchor,
                    target,
                    self.recovery_turn_sign,
                    self.recovery_attempts,
                ));
            }
            self.last_progress_pos = obs.tank_pos;
            self.last_progress_time_s = obs.time_s;
            self.progress_target = self.escape_waypoint;
            self.best_target_distance = self
                .escape_waypoint
                .map(|waypoint| obs.tank_pos.distance(waypoint))
                .unwrap_or(f32::INFINITY);
            self.phase = if obs.map_sheets >= 3 {
                CampaignAiPhase::Exit
            } else {
                CampaignAiPhase::Transit
            };
        }

        if obs.mode == ActorMode::Foot {
            return self.step_on_foot(obs);
        }

        if let Some(waypoint) = self.escape_waypoint {
            let distance = obs.tank_pos.distance(waypoint);
            if distance > ESCAPE_REACHED_RADIUS_M {
                self.phase = if obs.map_sheets >= 3 {
                    CampaignAiPhase::Exit
                } else {
                    CampaignAiPhase::Transit
                };
                return tank_nav(obs, waypoint, if distance < 25.0 { 0.46 } else { 0.72 });
            }
            self.escape_waypoint = None;
            self.last_progress_pos = obs.tank_pos;
            self.last_progress_time_s = obs.time_s;
            self.progress_target = route_target(obs);
            self.best_target_distance = self
                .progress_target
                .map(|target| obs.tank_pos.distance(target))
                .unwrap_or(f32::INFINITY);
        }

        if obs.map_sheets >= 3 {
            self.phase = CampaignAiPhase::Exit;
            return tank_nav(obs, obs.generated_exit, 1.0);
        }

        let Some(site) = obs.next_map_site else {
            self.phase = CampaignAiPhase::Failed;
            return CampaignAiCommand::default();
        };
        let site_distance = obs.tank_pos.distance(site);
        if !obs.next_guard_clear || site_distance > SITE_PARK_RADIUS_M {
            self.phase = CampaignAiPhase::Transit;
            return tank_nav(obs, site, if site_distance < 55.0 { 0.48 } else { 1.0 });
        }

        self.phase = CampaignAiPhase::Park;
        if obs.tank_speed_mps.abs() > 0.18 {
            return CampaignAiCommand {
                tank_drive: -obs.tank_speed_mps.signum() * 0.35,
                ..Default::default()
            };
        }
        self.phase = CampaignAiPhase::Dismount;
        CampaignAiCommand {
            toggle_dismount: obs.can_dismount,
            ..Default::default()
        }
    }

    fn step_on_foot(&mut self, obs: CampaignAiObservation) -> CampaignAiCommand {
        if matches!(
            self.phase,
            CampaignAiPhase::ReturnToTiger | CampaignAiPhase::Remount
        ) {
            let distance = obs.foot_pos.distance(obs.tank_pos);
            if distance > MOUNT_RADIUS_M {
                self.phase = CampaignAiPhase::ReturnToTiger;
                let mut command = foot_nav(obs, obs.tank_pos, distance);
                command.crouch = obs.foot_detected;
                return command;
            }
            self.phase = CampaignAiPhase::Remount;
            return CampaignAiCommand {
                mount_tiger: obs.can_mount,
                ..Default::default()
            };
        }

        let Some(case) = obs.active_map_case.or(obs.next_map_site) else {
            self.phase = CampaignAiPhase::ReturnToTiger;
            return foot_nav(obs, obs.tank_pos, obs.foot_pos.distance(obs.tank_pos));
        };
        let distance = obs.foot_pos.distance(case);
        if distance > MAP_USE_RADIUS_M {
            self.phase = CampaignAiPhase::FootToMap;
            let mut command = foot_nav(obs, case, distance);
            command.crouch = obs.foot_detected;
            command
        } else {
            self.phase = CampaignAiPhase::SearchMap;
            CampaignAiCommand {
                search_map: true,
                // Searching a folio is deliberately exposed and stationary. Stay low even before
                // a rifleman finishes acquiring the player so the rollout contains useful stance
                // variation instead of a permanently upright pedestrian target.
                crouch: true,
                foot_target_yaw: Some(foot_target_yaw(obs.foot_pos, case)),
                ..Default::default()
            }
        }
    }
}

fn route_target(obs: CampaignAiObservation) -> Option<Vec2> {
    if obs.map_sheets >= 3 {
        Some(obs.generated_exit)
    } else {
        obs.next_map_site
    }
}

fn recovery_escape_waypoint(anchor: Vec2, target: Vec2, turn_sign: f32, attempt: u8) -> Vec2 {
    let direct = (target - anchor).try_normalize().unwrap_or(Vec2::Y);
    let right = Vec2::new(direct.y, -direct.x);
    let lateral =
        ESCAPE_BASE_LATERAL_M + ESCAPE_LATERAL_STEP_M * f32::from(attempt.saturating_sub(1));
    anchor - direct * ESCAPE_BACKOFF_M + right * (turn_sign * lateral)
}

fn tank_nav(obs: CampaignAiObservation, target: Vec2, max_drive: f32) -> CampaignAiCommand {
    let delta = target - obs.tank_pos;
    let desired = delta.x.atan2(delta.y);
    let error = wrap_angle(desired - obs.tank_heading);
    let alignment = (1.0 - error.abs() / 1.05).clamp(0.0, 1.0);
    CampaignAiCommand {
        // Do not creep the bow into masonry while it is still pointed away from the chosen line.
        // Neutral steering rotates in place in the deterministic tank integrator; once within 60°,
        // drive rises smoothly and retains a small anti-stiction floor.
        tank_drive: if alignment > 0.0 {
            (max_drive * alignment).max(0.14)
        } else {
            0.0
        },
        tank_steer: (error * 1.35).clamp(-1.0, 1.0),
        ..Default::default()
    }
}

fn foot_nav(obs: CampaignAiObservation, target: Vec2, distance: f32) -> CampaignAiCommand {
    let target_yaw = foot_target_yaw(obs.foot_pos, target);
    let error = wrap_angle(target_yaw - obs.foot_yaw);
    CampaignAiCommand {
        foot_forward: if error.abs() < 1.15 {
            if distance < 3.0 {
                0.42
            } else {
                1.0
            }
        } else {
            0.0
        },
        foot_strafe: (error * 0.35).clamp(-0.45, 0.45),
        foot_target_yaw: Some(target_yaw),
        ..Default::default()
    }
}

fn foot_target_yaw(from: Vec2, to: Vec2) -> f32 {
    let delta = to - from;
    delta.x.atan2(-delta.y)
}

fn wrap_angle(mut angle: f32) -> f32 {
    while angle > std::f32::consts::PI {
        angle -= std::f32::consts::TAU;
    }
    while angle < -std::f32::consts::PI {
        angle += std::f32::consts::TAU;
    }
    angle
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResourceSnapshot {
    pub main_gun_rounds: u32,
    pub fuel_liters: f32,
    pub repair_kits: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct RunFrame {
    pub time_s: f32,
    pub phase: CampaignAiPhase,
    pub mode: ActorMode,
    pub tank_pos: Vec2,
    pub foot_pos: Vec2,
    pub crouched: bool,
    pub visible_to_enemy: bool,
    pub enemies_alive: usize,
    pub map_sheets: u8,
    pub contacts: u32,
    pub kills: u32,
    pub main_gun_shots: u32,
    pub outgoing_hits: u32,
    pub outgoing_penetrations: u32,
    pub outgoing_kills: u32,
    pub incoming_hits: u32,
    pub incoming_penetrations: u32,
    pub resources: ResourceSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Success,
    TigerKia,
    CrewKia,
    FootKia,
    FuelStarved,
    AmmoStarved,
    NavigationStall,
    Timeout,
}

impl RunOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::TigerKia => "tiger_kia",
            Self::CrewKia => "crew_kia",
            Self::FootKia => "foot_kia",
            Self::FuelStarved => "fuel_starved",
            Self::AmmoStarved => "ammo_starved",
            Self::NavigationStall => "navigation_stall",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LootEvent {
    pub time_s: f32,
    pub kind: &'static str,
    pub amount: f32,
    pub pos: Vec2,
}

#[derive(Clone, Debug)]
pub struct MapEvent {
    pub sheet: u8,
    pub time_s: f32,
    pub pos: Vec2,
}

#[derive(Clone, Debug)]
pub struct FootTrajectorySample {
    pub time_s: f32,
    pub pos: Vec2,
    pub crouched: bool,
    pub visible_to_enemy: bool,
    pub enemies_alive: usize,
}

#[derive(Clone, Debug)]
pub struct PedestrianShotEvent {
    pub time_s: f32,
    pub weapon: &'static str,
    pub shooter: Vec2,
    pub target: Vec2,
    pub distance_m: f32,
    pub target_crouched: bool,
    pub target_visible: bool,
    pub hit: bool,
    pub killed: bool,
}

#[derive(Clone, Debug)]
pub struct RunTelemetry {
    pub seed: u64,
    pub outcome: Option<RunOutcome>,
    pub duration_s: f32,
    pub tank_distance_m: f32,
    pub foot_distance_m: f32,
    pub tank_time_s: f32,
    pub foot_time_s: f32,
    pub dismounts: u32,
    pub contacts: u32,
    pub kills: u32,
    pub main_gun_shots: u32,
    pub outgoing_hits: u32,
    pub outgoing_penetrations: u32,
    pub outgoing_kills: u32,
    pub incoming_hits: u32,
    pub incoming_penetrations: u32,
    pub resources_start: ResourceSnapshot,
    pub resources_end: ResourceSnapshot,
    pub ammo_fired: u32,
    pub fuel_burned_l: f32,
    pub fuel_added_l: f32,
    pub spare_parts_used: u32,
    pub loot: Vec<LootEvent>,
    pub maps: Vec<MapEvent>,
    pub foot_trajectory: Vec<FootTrajectorySample>,
    pub pedestrian_shots: Vec<PedestrianShotEvent>,
    prev: Option<RunFrame>,
    next_foot_sample_s: f32,
}

impl RunTelemetry {
    pub fn new(seed: u64, start: RunFrame) -> Self {
        Self {
            seed,
            outcome: None,
            duration_s: 0.0,
            tank_distance_m: 0.0,
            foot_distance_m: 0.0,
            tank_time_s: 0.0,
            foot_time_s: 0.0,
            dismounts: 0,
            contacts: start.contacts,
            kills: start.kills,
            main_gun_shots: 0,
            outgoing_hits: start.outgoing_hits,
            outgoing_penetrations: start.outgoing_penetrations,
            outgoing_kills: start.outgoing_kills,
            incoming_hits: start.incoming_hits,
            incoming_penetrations: start.incoming_penetrations,
            resources_start: start.resources,
            resources_end: start.resources,
            ammo_fired: 0,
            fuel_burned_l: 0.0,
            fuel_added_l: 0.0,
            spare_parts_used: 0,
            loot: Vec::new(),
            maps: Vec::new(),
            foot_trajectory: Vec::new(),
            pedestrian_shots: Vec::new(),
            prev: Some(start),
            next_foot_sample_s: start.time_s,
        }
    }

    pub fn record_frame(&mut self, frame: RunFrame) {
        if let Some(prev) = self.prev {
            let dt = (frame.time_s - prev.time_s).max(0.0);
            match frame.mode {
                ActorMode::Tank => {
                    self.tank_time_s += dt;
                    self.tank_distance_m += frame.tank_pos.distance(prev.tank_pos).min(80.0);
                }
                ActorMode::Foot => {
                    self.foot_time_s += dt;
                    self.foot_distance_m += frame.foot_pos.distance(prev.foot_pos).min(20.0);
                }
            }
            if prev.mode == ActorMode::Tank && frame.mode == ActorMode::Foot {
                self.dismounts += 1;
            }
            if frame.map_sheets > prev.map_sheets {
                self.maps.push(MapEvent {
                    sheet: frame.map_sheets,
                    time_s: frame.time_s,
                    pos: frame.foot_pos,
                });
            }
            let shots = frame.main_gun_shots.wrapping_sub(prev.main_gun_shots);
            self.main_gun_shots = self.main_gun_shots.saturating_add(shots);
            self.ammo_fired = self.main_gun_shots;
            self.fuel_burned_l +=
                (prev.resources.fuel_liters - frame.resources.fuel_liters).max(0.0);
            self.fuel_added_l +=
                (frame.resources.fuel_liters - prev.resources.fuel_liters).max(0.0);
            self.spare_parts_used = self.spare_parts_used.saturating_add(
                prev.resources
                    .repair_kits
                    .saturating_sub(frame.resources.repair_kits),
            );
            let event_pos = match frame.mode {
                ActorMode::Tank => frame.tank_pos,
                ActorMode::Foot => frame.foot_pos,
            };
            if frame.resources.main_gun_rounds > prev.resources.main_gun_rounds {
                self.loot.push(LootEvent {
                    time_s: frame.time_s,
                    kind: "main_gun_rounds",
                    amount: (frame.resources.main_gun_rounds - prev.resources.main_gun_rounds)
                        as f32,
                    pos: event_pos,
                });
            }
            if frame.resources.fuel_liters > prev.resources.fuel_liters + 0.01 {
                self.loot.push(LootEvent {
                    time_s: frame.time_s,
                    kind: "fuel_liters",
                    amount: frame.resources.fuel_liters - prev.resources.fuel_liters,
                    pos: event_pos,
                });
            }
            if frame.resources.repair_kits > prev.resources.repair_kits {
                self.loot.push(LootEvent {
                    time_s: frame.time_s,
                    kind: "spare_parts",
                    amount: (frame.resources.repair_kits - prev.resources.repair_kits) as f32,
                    pos: event_pos,
                });
            }
        }
        if frame.mode == ActorMode::Foot && frame.time_s >= self.next_foot_sample_s {
            self.foot_trajectory.push(FootTrajectorySample {
                time_s: frame.time_s,
                pos: frame.foot_pos,
                crouched: frame.crouched,
                visible_to_enemy: frame.visible_to_enemy,
                enemies_alive: frame.enemies_alive,
            });
            self.next_foot_sample_s = frame.time_s + 0.2;
        }
        self.duration_s = frame.time_s;
        self.contacts = frame.contacts;
        self.kills = frame.kills;
        self.outgoing_hits = frame.outgoing_hits;
        self.outgoing_penetrations = frame.outgoing_penetrations;
        self.outgoing_kills = frame.outgoing_kills;
        self.incoming_hits = frame.incoming_hits;
        self.incoming_penetrations = frame.incoming_penetrations;
        self.resources_end = frame.resources;
        self.prev = Some(frame);
    }

    pub fn record_loot(&mut self, event: LootEvent) {
        self.loot.push(event);
    }

    pub fn record_pedestrian_shot(&mut self, event: PedestrianShotEvent) {
        self.pedestrian_shots.push(event);
    }

    pub fn finish(&mut self, outcome: RunOutcome) {
        self.outcome = Some(outcome);
    }

    /// One self-contained JSONL record. Foot samples are deliberately retained: they are the
    /// supervised target-selection corpus for the later Soviet infantry/MG curriculum.
    pub fn to_json_line(&self) -> String {
        let vec2 = |v: Vec2| [v.x, v.y];
        json!({
            "schema": "kessel.campaign_run.v2",
            "seed": self.seed,
            "outcome": self.outcome.unwrap_or(RunOutcome::Timeout).label(),
            "duration_s": self.duration_s,
            "distance_m": {"tank": self.tank_distance_m, "foot": self.foot_distance_m},
            "time_s": {"tank": self.tank_time_s, "foot": self.foot_time_s},
            "dismounts": self.dismounts,
            "combat": {
                "waves_started": self.contacts,
                "vehicles_destroyed_total": self.kills,
                "outgoing": {"main_gun_shots": self.main_gun_shots, "hits": self.outgoing_hits, "penetrations": self.outgoing_penetrations, "kills": self.outgoing_kills},
                "incoming": {"hits": self.incoming_hits, "penetrations": self.incoming_penetrations},
            },
            "resources": {
                "start": {"ammo": self.resources_start.main_gun_rounds, "fuel_l": self.resources_start.fuel_liters, "repair": self.resources_start.repair_kits},
                "end": {"ammo": self.resources_end.main_gun_rounds, "fuel_l": self.resources_end.fuel_liters, "repair": self.resources_end.repair_kits},
                "ammo_fired": self.ammo_fired,
                "fuel_burned_l": self.fuel_burned_l,
                "fuel_added_l": self.fuel_added_l,
                "spare_parts_used": self.spare_parts_used,
            },
            "loot": self.loot.iter().map(|e| json!({"t":e.time_s,"kind":e.kind,"amount":e.amount,"pos":vec2(e.pos)})).collect::<Vec<_>>(),
            "maps": self.maps.iter().map(|e| json!({"sheet":e.sheet,"t":e.time_s,"pos":vec2(e.pos)})).collect::<Vec<_>>(),
            "foot_trajectory": self.foot_trajectory.iter().map(|s| json!({"t":s.time_s,"pos":vec2(s.pos),"crouched":s.crouched,"visible":s.visible_to_enemy,"enemies":s.enemies_alive})).collect::<Vec<_>>(),
            "pedestrian_shots": self.pedestrian_shots.iter().map(|e| json!({"t":e.time_s,"weapon":e.weapon,"shooter":vec2(e.shooter),"target":vec2(e.target),"distance_m":e.distance_m,"crouched":e.target_crouched,"visible":e.target_visible,"hit":e.hit,"killed":e.killed})).collect::<Vec<_>>(),
        }).to_string()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BalanceReport {
    pub runs: usize,
    pub success_rate: f32,
    pub median_duration_s: f32,
    pub fuel_starvation_rate: f32,
    pub ammo_starvation_rate: f32,
    pub crew_kia_rate: f32,
    /// Multipliers are recommendations for the next batch, not live difficulty rubber-banding.
    pub ammo_cache_scale: f32,
    pub fuel_cache_scale: f32,
    pub repair_cache_scale: f32,
}

impl BalanceReport {
    pub fn from_runs(runs: &[RunTelemetry]) -> Self {
        if runs.is_empty() {
            return Self::default();
        }
        let n = runs.len() as f32;
        let rate = |wanted: RunOutcome| {
            runs.iter()
                .filter(|run| run.outcome == Some(wanted))
                .count() as f32
                / n
        };
        let fuel = rate(RunOutcome::FuelStarved);
        let ammo = rate(RunOutcome::AmmoStarved);
        let crew = rate(RunOutcome::CrewKia);
        let mut durations: Vec<f32> = runs.iter().map(|run| run.duration_s).collect();
        durations.sort_by(f32::total_cmp);
        let median = durations[durations.len() / 2];
        Self {
            runs: runs.len(),
            success_rate: rate(RunOutcome::Success),
            median_duration_s: median,
            fuel_starvation_rate: fuel,
            ammo_starvation_rate: ammo,
            crew_kia_rate: crew,
            // Aim for scarcity that matters in some runs without turning a random cache roll into
            // the dominant cause of failure. Clamp each iteration to ±20% to avoid oscillation.
            ammo_cache_scale: scarcity_scale(ammo, 0.08),
            fuel_cache_scale: scarcity_scale(fuel, 0.06),
            // There are currently no repair-kit map drops, and crew loss is not evidence of spare
            // scarcity. Keep this neutral until real repair acquisition/use events exist.
            repair_cache_scale: 1.0,
        }
    }
}

fn scarcity_scale(observed_failure_rate: f32, desired_failure_rate: f32) -> f32 {
    (1.0 + (observed_failure_rate - desired_failure_rate) * 1.25).clamp(0.8, 1.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs() -> CampaignAiObservation {
        CampaignAiObservation {
            time_s: 0.0,
            mode: ActorMode::Tank,
            tiger_alive: true,
            foot_alive: true,
            tank_pos: Vec2::ZERO,
            tank_heading: 0.0,
            tank_speed_mps: 0.0,
            foot_pos: Vec2::ZERO,
            foot_yaw: 0.0,
            enemies_alive: 0,
            foot_detected: false,
            map_sheets: 0,
            next_guard_clear: false,
            next_map_site: Some(Vec2::new(0.0, 100.0)),
            active_map_case: None,
            generated_exit: Vec2::new(900.0, 0.0),
            exit_reached: false,
            can_dismount: true,
            can_mount: true,
        }
    }

    #[test]
    fn reproduced_stall_routes_keep_their_telemetry_targets() {
        let route_4 =
            crate::run_route::RunRoutePlan::generate(0xC0DE_1942_45A1_0004, 1, 1, 2_200.0, None)
                .expect("seed 4 Act-I route fixture");
        let route_6 =
            crate::run_route::RunRoutePlan::generate(0xC0DE_1942_45A1_0006, 1, 1, 2_200.0, None)
                .expect("seed 6 Act-I route fixture");
        let route_9 =
            crate::run_route::RunRoutePlan::generate(0xC0DE_1942_45A1_0009, 1, 1, 2_200.0, None)
                .expect("seed 9 Act-I route fixture");

        assert_eq!(route_4.selected_site_ids, route_6.selected_site_ids);
        assert_eq!(
            Vec2::new(
                route_4.sites[1].position.x as f32,
                route_4.sites[1].position.z as f32,
            ),
            Vec2::new(920.0, 1_080.0)
        );
        assert_eq!(
            Vec2::new(
                route_9.sites[2].position.x as f32,
                route_9.sites[2].position.z as f32,
            ),
            Vec2::new(-1_340.0, -860.0)
        );
    }

    #[test]
    fn tank_navigation_pivots_before_driving_toward_a_target_behind_it() {
        let mut o = obs();
        o.next_map_site = Some(Vec2::new(0.0, -100.0));
        let command = tank_nav(o, o.next_map_site.unwrap(), 1.0);
        assert_eq!(command.tank_drive, 0.0);
        assert!(command.tank_steer.abs() > 0.9);
    }

    #[test]
    fn repeated_city_stalls_alternate_and_widen_escape_routes() {
        let mut o = obs();
        // Exact seed 4/6 physical telemetry fixture: both tanks stopped here while taking the same
        // direct line from map one to the Grain Operations Room.
        o.tank_pos = Vec2::new(453.0, 226.0);
        o.next_map_site = Some(Vec2::new(920.0, 1_080.0));
        o.map_sheets = 1;
        let mut ai = CampaignAiController::new(o);

        o.time_s = STALL_SECONDS + 0.1;
        let first_reverse = ai.step(o);
        assert_eq!(ai.phase(), CampaignAiPhase::Recovery);
        assert!(first_reverse.tank_drive < -0.8);
        assert!(first_reverse.tank_steer > 0.8);

        o.time_s += RECOVERY_SECONDS + 0.1;
        let first_escape_command = ai.step(o);
        let first_escape = ai.escape_waypoint.expect("first lateral escape");
        assert!(first_escape_command.tank_steer.abs() > 0.0);

        o.time_s += STALL_SECONDS + 0.1;
        let second_reverse = ai.step(o);
        assert_eq!(ai.phase(), CampaignAiPhase::Recovery);
        assert!(second_reverse.tank_drive < -0.8);
        assert!(second_reverse.tank_steer < -0.8);

        o.time_s += RECOVERY_SECONDS + 0.1;
        ai.step(o);
        let second_escape = ai.escape_waypoint.expect("second lateral escape");
        let direct = (o.next_map_site.unwrap() - o.tank_pos).normalize();
        let right = Vec2::new(direct.y, -direct.x);
        let first_lateral = (first_escape - o.tank_pos).dot(right);
        let second_lateral = (second_escape - o.tank_pos).dot(right);
        assert!(first_lateral > ESCAPE_BASE_LATERAL_M - 0.1);
        assert!(second_lateral < -(ESCAPE_BASE_LATERAL_M + ESCAPE_LATERAL_STEP_M - 0.1));
    }

    #[test]
    fn circling_a_city_block_does_not_count_as_route_progress() {
        let mut o = obs();
        o.tank_pos = Vec2::new(453.0, 226.0);
        o.next_map_site = Some(Vec2::new(920.0, 1_080.0));
        o.map_sheets = 1;
        let target = o.next_map_site.unwrap();
        let radial = o.tank_pos - target;
        let mut ai = CampaignAiController::new(o);

        // Each sample moves the hull tens of metres but stays on the same radius around the site.
        // The old displacement watchdog refreshed forever on this exact kind of wall-following
        // orbit, which is why seeds 4/6 could move continuously without advancing the campaign.
        for sample in 1..=5 {
            o.time_s = sample as f32 * 10.0;
            let angle = sample as f32 * 0.025;
            let (sin, cos) = angle.sin_cos();
            o.tank_pos = target
                + Vec2::new(
                    radial.x * cos - radial.y * sin,
                    radial.x * sin + radial.y * cos,
                );
            let command = ai.step(o);
            if sample < 5 {
                assert_ne!(ai.phase(), CampaignAiPhase::Recovery);
            } else {
                assert_eq!(ai.phase(), CampaignAiPhase::Recovery);
                assert!(command.tank_drive < 0.0);
            }
        }
    }

    #[test]
    fn immobile_tank_exhausts_recovery_budget_instead_of_timing_out() {
        let mut o = obs();
        // Exact seed 9 post-combat fixture. Its log records a destroyed transmission immediately
        // before the tank became stationary at this approach to map three.
        o.tank_pos = Vec2::new(-1_380.0, -750.0);
        o.next_map_site = Some(Vec2::new(-1_340.0, -860.0));
        o.map_sheets = 2;
        let mut ai = CampaignAiController::new(o);

        for attempt in 1..=MAX_RECOVERY_ATTEMPTS {
            o.time_s += STALL_SECONDS + 0.1;
            let command = ai.step(o);
            assert_eq!(ai.phase(), CampaignAiPhase::Recovery);
            assert_eq!(ai.recovery_attempts, attempt);
            assert!(command.tank_drive < 0.0);

            o.time_s += RECOVERY_SECONDS + 0.1;
            ai.step(o);
            assert_ne!(ai.phase(), CampaignAiPhase::Failed);
        }

        o.time_s += STALL_SECONDS + 0.1;
        let command = ai.step(o);
        assert_eq!(ai.phase(), CampaignAiPhase::Failed);
        assert_eq!(command.tank_drive, 0.0);
    }

    #[test]
    fn mission_controller_performs_physical_map_ritual() {
        let mut o = obs();
        let mut ai = CampaignAiController::new(o);
        assert!(ai.step(o).tank_drive > 0.0);

        o.enemies_alive = 2;
        assert!(ai.step(o).delegate_combat);
        o.enemies_alive = 0;
        o.next_guard_clear = true;
        o.tank_pos = Vec2::new(0.0, 90.0);
        assert!(ai.step(o).toggle_dismount);

        o.mode = ActorMode::Foot;
        o.foot_pos = Vec2::new(0.0, 91.0);
        o.active_map_case = Some(Vec2::new(0.0, 100.0));
        let turn = ai.step(o);
        o.foot_yaw = turn.foot_target_yaw.expect("map target yaw");
        assert!(ai.step(o).foot_forward > 0.0);
        o.foot_pos = Vec2::new(0.0, 99.0);
        assert!(ai.step(o).search_map);

        o.map_sheets = 1;
        let turn = ai.step(o);
        o.foot_yaw = turn.foot_target_yaw.expect("return target yaw");
        assert!(ai.step(o).foot_forward > 0.0);
        o.foot_pos = o.tank_pos;
        assert!(ai.step(o).mount_tiger);
    }

    #[test]
    fn combat_interrupts_but_does_not_skip_site_search() {
        let mut o = obs();
        o.next_guard_clear = true;
        o.tank_pos = Vec2::new(0.0, 90.0);
        let mut ai = CampaignAiController::new(o);
        o.enemies_alive = 1;
        assert_eq!(ai.step(o).delegate_combat, true);
        o.enemies_alive = 0;
        assert!(ai.step(o).toggle_dismount);
        assert_eq!(ai.phase(), CampaignAiPhase::Dismount);
    }

    #[test]
    fn telemetry_keeps_foot_trajectory_and_resource_deltas() {
        let start = RunFrame {
            time_s: 0.0,
            phase: CampaignAiPhase::Transit,
            mode: ActorMode::Tank,
            tank_pos: Vec2::ZERO,
            foot_pos: Vec2::ZERO,
            crouched: false,
            visible_to_enemy: false,
            enemies_alive: 0,
            map_sheets: 0,
            contacts: 0,
            kills: 0,
            main_gun_shots: 0,
            outgoing_hits: 0,
            outgoing_penetrations: 0,
            outgoing_kills: 0,
            incoming_hits: 0,
            incoming_penetrations: 0,
            resources: ResourceSnapshot {
                main_gun_rounds: 34,
                fuel_liters: 420.0,
                repair_kits: 3,
            },
        };
        let mut log = RunTelemetry::new(7, start);
        log.record_frame(RunFrame {
            time_s: 1.0,
            mode: ActorMode::Foot,
            main_gun_shots: 1,
            foot_pos: Vec2::new(2.0, 0.0),
            crouched: true,
            visible_to_enemy: true,
            map_sheets: 1,
            resources: ResourceSnapshot {
                main_gun_rounds: 33,
                fuel_liters: 418.0,
                repair_kits: 2,
            },
            ..start
        });
        log.finish(RunOutcome::Success);
        assert_eq!(log.dismounts, 1);
        assert_eq!(log.ammo_fired, 1);
        assert_eq!(log.spare_parts_used, 1);
        assert_eq!(log.maps.len(), 1);
        assert_eq!(log.foot_trajectory.len(), 1);
        let json: serde_json::Value = serde_json::from_str(&log.to_json_line()).unwrap();
        assert_eq!(json["outcome"], "success");
        assert_eq!(json["schema"], "kessel.campaign_run.v2");
        assert_eq!(json["combat"]["outgoing"]["main_gun_shots"], 1);
        assert_eq!(json["foot_trajectory"][0]["visible"], true);
    }

    #[test]
    fn balance_changes_are_bounded_per_batch() {
        let start = RunFrame {
            time_s: 0.0,
            phase: CampaignAiPhase::Transit,
            mode: ActorMode::Tank,
            tank_pos: Vec2::ZERO,
            foot_pos: Vec2::ZERO,
            crouched: false,
            visible_to_enemy: false,
            enemies_alive: 0,
            map_sheets: 0,
            contacts: 0,
            kills: 0,
            main_gun_shots: 0,
            outgoing_hits: 0,
            outgoing_penetrations: 0,
            outgoing_kills: 0,
            incoming_hits: 0,
            incoming_penetrations: 0,
            resources: ResourceSnapshot::default(),
        };
        let runs: Vec<_> = (0..10)
            .map(|seed| {
                let mut run = RunTelemetry::new(seed, start);
                run.finish(RunOutcome::FuelStarved);
                run
            })
            .collect();
        let report = BalanceReport::from_runs(&runs);
        assert_eq!(report.fuel_cache_scale, 1.2);
        assert!((0.8..=1.2).contains(&report.ammo_cache_scale));
    }
}
