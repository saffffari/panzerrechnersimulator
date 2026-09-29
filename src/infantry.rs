//! ABSITZEN enemy infantry — Russian soldiers that patrol set routes with a vision cone and a
//! detection meter, turn to face a contact (the diegetic tell), raise the alarm when fully detected,
//! and fire (lethal both ways). Pure game-logic (no wgpu), same discipline as `foot.rs`: `game.rs`
//! grounds each soldier on the heightfield and advances its shared static-pose animation per frame.
//! If the complete authored pose library is unavailable, game.rs falls back atomically to the two
//! legacy mesh slots (procedural body + rifle) rather than mixing representations.

use glam::{Vec2, Vec3};

use crate::foot_ai::{
    decide_or_expert, FootAction, FootObservation, FootPolicy, GoalKind, ObservationInput,
    PolicyRole, RelativeCue, WeaponSelection,
};
use crate::game::ray_terrain;
use crate::soviet_animation::{SovietAnimationInput, SovietAnimator, SovietPose};
use crate::terrain::Heightfield;

/// Eye height of a standing soldier (m).
pub const EYE_H: f32 = 1.6;

const VIEW_RANGE: f32 = 45.0; // m — clean standing sight range (scaled by player visibility)
const HALF_FOV_COS: f32 = 0.6428; // cos(50°) — half the vision cone
const GAIN_RATE: f32 = 0.9; // detection accrual scalar (full standing LOS ≈ 1.6 s)
const DECAY_RATE: f32 = 0.35; // detection bleed-off with no contact (~1 s behind cover resets Suspicious)
const HEAR_RATE: f32 = 0.28; // continuous "hears you moving nearby" trickle
const PATROL_SPEED: f32 = 1.4; // m/s walking the route
const INVEST_SPEED: f32 = 1.1; // m/s easing toward a suspicious point
const HUNT_SPEED: f32 = 1.7; // m/s advancing on the last-seen point while alarmed
const NEAR_RANGE: f32 = 8.0; // m — inside this, range falloff is 1.0
const FAR_VIS: f32 = 0.15; // range falloff at the edge of VIEW_RANGE
const AIM_WINDUP: f32 = 0.5; // s — shoulders the rifle before the first shot (the 2nd tell)
const INF_RELOAD: f32 = 1.6; // s — between shots
const LOUD_NOISE: f32 = 60.0; // event strength above this = a gunshot
const GUNSHOT_STARTLE: f32 = 50.0; // m — inside this a gunshot forces Alarmed (wider radius: drawn)

const SUSPICIOUS_ON: f32 = 0.4;
const ALARMED_ON: f32 = 1.0;
const UNAWARE_OFF: f32 = 0.15;

/// A discrete loud event (running footfalls, and especially a Luger shot) that soldiers sample.
/// Lives in the `Game.noise` ring; decays by `ttl`.
#[derive(Clone, Copy, Default)]
pub struct NoiseEvent {
    pub pos: Vec3,
    pub strength: f32, // audible radius in metres
    pub ttl: f32,
}

/// What a soldier will do this frame, from most to least aware.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alert {
    Unaware,
    Suspicious,
    Alarmed,
}

impl Alert {
    pub fn name(self) -> &'static str {
        match self {
            Alert::Unaware => "UNAWARE",
            Alert::Suspicious => "SUSPICIOUS",
            Alert::Alarmed => "ALARMED",
        }
    }
}

/// One in-flight infantry shot, resolved in `game.rs` against the foot player.
#[derive(Clone, Copy, Debug)]
pub struct InfantryShot {
    pub from: Vec3,
    pub dir: Vec3,
    pub range: f32,
}

fn relative_cue(origin: Vec3, yaw: f32, target: Vec3, range: f32) -> RelativeCue {
    let delta = Vec3::new(target.x - origin.x, 0.0, target.z - origin.z);
    let distance = delta.length();
    let dir = if distance > 1.0e-5 {
        delta / distance
    } else {
        Vec3::ZERO
    };
    let forward = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    RelativeCue {
        forward: dir.dot(forward).clamp(-1.0, 1.0),
        right: dir.dot(right).clamp(-1.0, 1.0),
        distance: (distance / range.max(0.01)).clamp(0.0, 1.0),
    }
}

fn terrain_slope(hf: &Heightfield, origin: Vec3, direction: Vec3) -> f32 {
    let direction = Vec3::new(direction.x, 0.0, direction.z).normalize_or_zero();
    let ahead = origin + direction;
    (hf.height_at(ahead.x, ahead.z) - hf.height_at(origin.x, origin.z)).clamp(-1.0, 1.0)
}

fn signed_yaw_delta(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn scripted_action(
    before_pos: Vec3,
    before_yaw: f32,
    after_pos: Vec3,
    after_yaw: f32,
    dt: f32,
    alert: Alert,
    target_visible: bool,
    can_fire: bool,
) -> FootAction {
    let delta = Vec3::new(after_pos.x - before_pos.x, 0.0, after_pos.z - before_pos.z);
    let forward = Vec3::new(before_yaw.sin(), 0.0, -before_yaw.cos());
    let right = Vec3::new(before_yaw.cos(), 0.0, before_yaw.sin());
    let speed = match alert {
        Alert::Unaware => PATROL_SPEED,
        Alert::Suspicious => INVEST_SPEED,
        Alert::Alarmed => HUNT_SPEED,
    };
    let turn_rate = match alert {
        Alert::Unaware => 2.0,
        Alert::Suspicious => 2.2,
        Alert::Alarmed => 3.2,
    };
    let travel = (speed * dt).max(1.0e-5);
    FootAction {
        forward: (delta.dot(forward) / travel).clamp(-1.0, 1.0),
        strafe: (delta.dot(right) / travel).clamp(-1.0, 1.0),
        turn: (signed_yaw_delta(before_yaw, after_yaw) / (turn_rate * dt).max(1.0e-5))
            .clamp(-1.0, 1.0),
        aim: alert == Alert::Alarmed && target_visible,
        fire: can_fire,
        weapon: WeaponSelection::Rifle,
        ..FootAction::default()
    }
}

/// The learned rifle policy may decline a legal shot but can never manufacture one.  Non-rifle
/// action channels remain owned by the scripted Soviet doctrine and are zeroed at this boundary.
pub(crate) fn gate_soviet_action(
    proposed: FootAction,
    target_visible: bool,
    can_fire: bool,
) -> FootAction {
    FootAction {
        forward: proposed.forward.clamp(-1.0, 1.0),
        strafe: proposed.strafe.clamp(-1.0, 1.0),
        turn: proposed.turn.clamp(-1.0, 1.0),
        aim: proposed.aim && target_visible,
        fire: proposed.fire && target_visible && can_fire,
        weapon: WeaponSelection::Rifle,
        ..FootAction::default()
    }
}

/// One policy seam invocation.  The existing perception/state machine remains the teacher and the
/// physical authority; this record exposes the observation, the bounded action actually applied,
/// and the scripted correction for optional DAgger collection.
#[derive(Clone, Copy, Debug)]
pub struct SoldierPolicyStep {
    pub observation: FootObservation,
    pub action: FootAction,
    pub expert_action: FootAction,
    pub shot: Option<InfantryShot>,
}

pub struct Soldier {
    pub pos: Vec3,
    pub yaw: f32,         // forward = (sin, 0, -cos), same convention as the foot player
    pub route: [Vec3; 4], // patrol waypoints (a repeated waypoint = an idle scanning sentry)
    pub leg: usize,
    pub alert: Alert,
    pub detect: f32,  // 0..1 detection meter (NO on-screen bar — diegetic only)
    pub invest: Vec3, // last-seen / last-heard point
    pub has_invest: bool,
    pub alive: bool,
    /// Brief non-lethal suppression (used by near impacts); direct Luger/MG hits now kill.
    pub flinch_t: f32,
    /// Deterministic corpse loot: one or two loose 9 mm rounds. `looted` makes collection one-shot.
    pub ammo_drop: u8,
    pub looted: bool,
    /// Some riflemen stay physically grouped around an enemy tank. Game code updates their patrol
    /// loop from that tank's live transform before stepping the ordinary awareness state machine.
    pub escort_enemy: Option<usize>,
    pub fire_cd: f32,
    pub aim_windup: f32,
    /// Physical rifle-shoulder state selected by the authoritative doctrine/policy gate.  Rendering
    /// reads this instead of assuming every alarmed soldier has a legal sight picture.
    pub aiming: bool,
    /// Exact articulated stage selected by the shared combat authority; never a body scale.
    pub crouch_stage: usize,
    /// Horizontal speed measured from the authoritative resolved transform. Pose selection reads
    /// this value, so a blocked or flinching soldier cannot keep playing a locomotion cycle merely
    /// because an AI command requested movement.
    motion_speed_m_s: f32,
    /// One timing state per actor, selecting frames from the single shared 17-pose Soviet library.
    /// Geometry is never duplicated here; the renderer groups all soldiers by this selected pose.
    animator: SovietAnimator,
    pub tick: u32,          // per-soldier deterministic hash counter
    pub alarm_signal: bool, // set the frame it enters Alarmed; game.rs consumes it to cascade
    pub body_slot: usize,
    pub rifle_slot: usize,
}

impl Soldier {
    pub fn new(
        pos: Vec3,
        yaw: f32,
        route: [Vec3; 4],
        seed: u32,
        body_slot: usize,
        rifle_slot: usize,
    ) -> Self {
        Self {
            pos,
            yaw,
            route,
            leg: 0,
            alert: Alert::Unaware,
            detect: 0.0,
            invest: pos,
            has_invest: false,
            alive: true,
            flinch_t: 0.0,
            ammo_drop: 1 + ((seed.wrapping_mul(1103515245) >> 30) & 1) as u8,
            looted: false,
            escort_enemy: None,
            fire_cd: 0.0,
            aim_windup: 0.0,
            aiming: false,
            crouch_stage: 0,
            motion_speed_m_s: 0.0,
            animator: SovietAnimator::new(seed),
            tick: seed.wrapping_mul(2654435761),
            alarm_signal: false,
            body_slot,
            rifle_slot,
        }
    }

    /// Ground forward vector.
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }

    pub fn assign_escort(&mut self, enemy: usize) {
        self.escort_enemy = Some(enemy);
    }

    /// Move the patrol objective with its tank without changing the actor's physical pose.
    /// Mixed combat owns translation through its shared action/collision update.
    pub fn set_escort_route(&mut self, route: [Vec3; 4]) {
        self.route = route;
    }

    pub fn reanchor_escort(&mut self, route: [Vec3; 4], hf: &Heightfield, dt: f32) {
        self.set_escort_route(route);
        if !self.alive {
            return;
        }
        let to = Vec3::new(route[0].x - self.pos.x, 0.0, route[0].z - self.pos.z);
        let dist = to.length();
        if dist > 10.0 {
            let next = hf.walk_toward(self.pos, route[0]);
            let to = Vec3::new(next.x - self.pos.x, 0.0, next.z - self.pos.z);
            let before = self.pos;
            self.pos += to.normalize_or_zero() * 3.4 * dt;
            self.pos = hf.ground_walker(before, self.pos);
        }
    }

    /// Muster a reused renderer slot onto a newly streamed campaign tank contact.
    pub fn activate_escort(&mut self, route: [Vec3; 4], hf: &Heightfield) {
        self.route = route;
        self.leg = 0;
        self.pos = route[0];
        self.pos.y = hf.height_at(self.pos.x, self.pos.z);
        let to = route[1] - route[0];
        if to.x * to.x + to.z * to.z > 1e-6 {
            self.yaw = to.x.atan2(-to.z);
        }
        self.alive = true;
        self.looted = false;
        self.alert = Alert::Unaware;
        self.detect = 0.0;
        self.invest = self.pos;
        self.has_invest = false;
        self.flinch_t = 0.0;
        self.fire_cd = 0.0;
        self.aim_windup = 0.0;
        self.aiming = false;
        self.motion_speed_m_s = 0.0;
        self.animator.revive();
        self.alarm_signal = false;
    }

    /// One terminal path for pistol, MG and scripted soft-target hits.
    pub fn kill(&mut self) -> bool {
        if !self.alive {
            return false;
        }
        self.alive = false;
        self.flinch_t = 0.0;
        self.fire_cd = 0.0;
        self.aim_windup = 0.0;
        self.aiming = false;
        self.motion_speed_m_s = 0.0;
        self.animator.trigger_death();
        self.alarm_signal = false;
        true
    }

    /// Keep infantry alive and moving in the same world while the player is mounted. They cannot
    /// damage the buttoned Tiger, so perception/fire against the foot avatar stays dormant.
    pub fn update_passive(&mut self, hf: &Heightfield, dt: f32) {
        if !self.alive {
            self.motion_speed_m_s = 0.0;
            return;
        }
        let before_pos = self.pos;
        self.tick = self.tick.wrapping_add(1);
        self.fire_cd = (self.fire_cd - dt).max(0.0);
        self.flinch_t = (self.flinch_t - dt).max(0.0);
        self.detect = (self.detect - DECAY_RATE * dt).max(0.0);
        if self.detect < UNAWARE_OFF {
            self.alert = Alert::Unaware;
        }
        if self.flinch_t <= 0.0 {
            self.patrol(dt, hf);
        }
        self.motion_speed_m_s = measured_ground_speed(before_pos, self.pos, dt);
    }

    /// Present the shared mixed-team combat authority without running a second movement/fire AI.
    pub fn apply_combat_pose(
        &mut self,
        pos: Vec3,
        yaw: f32,
        velocity: Vec3,
        aiming: bool,
        crouch_stage: usize,
        fired: bool,
    ) {
        self.pos = pos;
        self.yaw = yaw;
        self.motion_speed_m_s = Vec3::new(velocity.x, 0.0, velocity.z).length();
        self.aiming = aiming;
        self.crouch_stage = crouch_stage.min(crate::infantry_combat::CROUCH_STAGES - 1);
        if aiming {
            self.alert = Alert::Alarmed;
            self.detect = 1.0;
        }
        if fired {
            self.animator.trigger_fire();
        }
    }

    /// Advance only lightweight animation timing and return the shared pose selected for this
    /// soldier. The input is derived wholly from the live physical/combat state.
    pub fn advance_animation(&mut self, dt: f32) -> SovietPose {
        self.animator.advance(
            dt,
            SovietAnimationInput {
                speed_m_s: self.motion_speed_m_s,
                aiming: self.aiming,
                alive: self.alive,
                flinch: self.flinch_t > 0.0,
            },
        )
    }

    /// Externally forced awareness (alarm cascade from a neighbour or a gunshot).
    pub fn raise_alarm(&mut self, at: Vec3) {
        self.detect = self.detect.max(0.75);
        self.invest = at;
        self.has_invest = true;
        if self.alert == Alert::Unaware {
            self.alert = Alert::Suspicious;
        }
    }

    /// Advance one frame. Returns a shot if it fires this frame. `prof` = the player's
    /// `(noise_radius, visibility)`; `cover` = coarse LOS-blocking cylinders `(centre_xz, radius)`.
    pub fn update(
        &mut self,
        player_eye: Vec3,
        prof: (f32, f32),
        noise: &[NoiseEvent],
        cover: &[(Vec2, f32)],
        hf: &Heightfield,
        dt: f32,
    ) -> Option<InfantryShot> {
        self.update_controlled(player_eye, 1.0, prof, noise, cover, hf, dt, None)
            .shot
    }

    /// Step the unchanged Soviet perception/doctrine teacher, then—when a role-safe KSL2 policy is
    /// installed—let that shared policy replace only locomotion, body turn, rifle aim and the choice
    /// to take an already-legal shot.  It cannot create visibility, skip reload/windup, alter damage,
    /// or fire at a last-known/hidden position.
    #[allow(clippy::too_many_arguments)]
    pub fn update_controlled(
        &mut self,
        player_eye: Vec3,
        player_hp_fraction: f32,
        prof: (f32, f32),
        noise: &[NoiseEvent],
        cover: &[(Vec2, f32)],
        hf: &Heightfield,
        dt: f32,
        policy: Option<&FootPolicy>,
    ) -> SoldierPolicyStep {
        self.update_controlled_with_visibility(player_eye, player_hp_fraction, prof, noise, cover, hf, dt, policy, false)
    }

    /// Live oriented doors are tested by Game against the current leaf transforms. The result gates
    /// both the doctrine observer and learned observer, without inventing circular cover geometry.
    #[allow(clippy::too_many_arguments)]
    pub fn update_controlled_with_visibility(
        &mut self, player_eye: Vec3, player_hp_fraction: f32, prof: (f32, f32),
        noise: &[NoiseEvent], cover: &[(Vec2, f32)], hf: &Heightfield, dt: f32,
        policy: Option<&FootPolicy>, external_occluded: bool,
    ) -> SoldierPolicyStep {
        // Death and the authored near-impact flinch are hard physical constraints, not policy
        // suggestions. A learned motor command cannot walk or shoulder the rifle through either.
        let motor_locked = !self.alive || self.flinch_t > 0.0;
        let policy = if !motor_locked { policy } else { None };
        let before_pos = self.pos;
        let before_yaw = self.yaw;
        let before_fire_cd = self.fire_cd;
        let scripted_shot = self.update_scripted(player_eye, prof, noise, cover, hf, dt, external_occluded);

        let eye = before_pos + Vec3::Y * EYE_H;
        let to_player = player_eye - eye;
        let player_distance = to_player.length();
        let target_visible = !external_occluded && player_distance > 0.05
            && player_distance < VIEW_RANGE * prof.1
            && self
                .forward_from(before_yaw)
                .dot(Vec3::new(to_player.x, 0.0, to_player.z).normalize_or_zero())
                > HALF_FOV_COS
            && !segment_blocked(eye, player_eye, hf, cover);

        // A contact may become the motor goal only while physically visible.  Once LOS is lost the
        // learned policy gets the same last-known point as the scripted doctrine, never the live
        // (hidden) foot-player transform.
        let goal_world = if self.alert != Alert::Unaware && target_visible {
            player_eye
        } else {
            match self.alert {
                Alert::Unaware => self.route[self.leg],
                Alert::Suspicious | Alert::Alarmed if self.has_invest => self.invest,
                Alert::Suspicious | Alert::Alarmed => self.pos,
            }
        };
        let goal_kind = if self.alert == Alert::Unaware {
            GoalKind::Hold
        } else {
            GoalKind::Intercept
        };
        let visible_target = if target_visible {
            player_eye
        } else {
            self.invest
        };
        let can_fire = scripted_shot.is_some() && target_visible;
        let observation = ObservationInput {
            role: PolicyRole::SovietRifle,
            goal: relative_cue(before_pos, before_yaw, goal_world, VIEW_RANGE),
            goal_kind,
            hp_fraction: if self.alive { 1.0 } else { 0.0 },
            velocity_forward: 0.0,
            velocity_right: 0.0,
            crouched: false,
            aiming: self.aiming,
            ammo_fraction: 1.0,
            reload_fraction: (self.fire_cd / INF_RELOAD).clamp(0.0, 1.0),
            detected: self.alert == Alert::Alarmed,
            suppression: (self.flinch_t / 0.8).clamp(0.0, 1.0),
            stamina_fraction: 1.0,
            tiger: RelativeCue::default(),
            tiger_mountable: false,
            target: if target_visible || self.has_invest {
                relative_cue(before_pos, before_yaw, visible_target, VIEW_RANGE)
            } else {
                RelativeCue::default()
            },
            target_visible,
            target_last_known: !target_visible && self.has_invest,
            // Hidden targets contribute no live health information to the learned policy.
            target_hp_fraction: if target_visible {
                player_hp_fraction.clamp(0.0, 1.0)
            } else {
                0.0
            },
            target_armored: false,
            nearest_ally: RelativeCue::default(),
            nearest_ally_alive: false,
            cover_clearance: [1.0; 8],
            terrain_slope_forward: terrain_slope(hf, before_pos, self.forward_from(before_yaw)),
            terrain_slope_right: terrain_slope(
                hf,
                before_pos,
                Vec3::new(before_yaw.cos(), 0.0, before_yaw.sin()),
            ),
            can_interact: false,
            can_jump: false,
            can_fire,
            can_reload: false,
            weapon: WeaponSelection::Rifle,
        }
        .encode();

        let mut expert_action = scripted_action(
            before_pos,
            before_yaw,
            self.pos,
            self.yaw,
            dt,
            self.alert,
            target_visible,
            can_fire,
        );
        if motor_locked {
            expert_action = FootAction {
                weapon: WeaponSelection::Rifle,
                ..FootAction::default()
            };
        }

        let Some(policy) = policy else {
            self.aiming = expert_action.aim;
            self.motion_speed_m_s = measured_ground_speed(before_pos, self.pos, dt);
            if scripted_shot.is_some() {
                self.animator.trigger_fire();
            }
            return SoldierPolicyStep {
                observation,
                action: expert_action,
                expert_action,
                shot: scripted_shot,
            };
        };

        let proposed = decide_or_expert(Some(policy), &observation, expert_action);
        let action = gate_soviet_action(proposed, target_visible, can_fire);

        // The teacher already advanced perception, route-leg, cooldown and alarm state. Replace only
        // its physical pose with the bounded learned motor output.
        self.pos = before_pos;
        self.yaw = before_yaw;
        let turn_rate = match self.alert {
            Alert::Unaware => 2.0,
            Alert::Suspicious => 2.2,
            Alert::Alarmed => 3.2,
        };
        self.yaw += action.turn * turn_rate * dt;
        let speed = match self.alert {
            Alert::Unaware => PATROL_SPEED,
            Alert::Suspicious => INVEST_SPEED,
            Alert::Alarmed => HUNT_SPEED,
        };
        let forward = self.forward();
        let right = Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let wish = (forward * action.forward + right * action.strafe).clamp_length_max(1.0);
        let before = self.pos;
        self.pos += wish * speed * dt;
        // No navigation: an open cellar's pit is neither walked into nor climbed out of.
        self.pos = hf.ground_walker(before, self.pos);
        self.aiming = action.aim;

        let shot = if action.fire {
            scripted_shot
        } else {
            // A learned hold-fire decision does not consume the authoritative rifle cooldown.
            if scripted_shot.is_some() {
                self.fire_cd = (before_fire_cd - dt).max(0.0);
            }
            None
        };
        self.motion_speed_m_s = measured_ground_speed(before_pos, self.pos, dt);
        if shot.is_some() {
            self.animator.trigger_fire();
        }
        SoldierPolicyStep {
            observation,
            action,
            expert_action,
            shot,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_scripted(
        &mut self,
        player_eye: Vec3,
        prof: (f32, f32),
        noise: &[NoiseEvent],
        cover: &[(Vec2, f32)],
        hf: &Heightfield,
        dt: f32,
        external_occluded: bool,
    ) -> Option<InfantryShot> {
        if !self.alive {
            return None;
        }
        self.tick = self.tick.wrapping_add(1);
        if self.fire_cd > 0.0 {
            self.fire_cd -= dt;
        }
        self.alarm_signal = false;
        // Near impacts can still suppress briefly. Direct weapon hits use `kill`.
        if self.flinch_t > 0.0 {
            self.flinch_t -= dt;
            self.aim_windup = 0.0;
            return None;
        }

        let (noise_radius, visibility) = prof;
        let eye = self.pos + Vec3::Y * EYE_H;

        // ── Vision ────────────────────────────────────────────────────────────
        let to = player_eye - eye;
        let dist = to.length();
        let eff_range = VIEW_RANGE * visibility;
        let mut gain = 0.0;
        let mut saw = false;
        if dist > 0.05 && dist < eff_range {
            let dir = to / dist;
            let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
            let cone = self.forward().dot(flat);
            if !external_occluded && cone > HALF_FOV_COS && !segment_blocked(eye, player_eye, hf, cover) {
                let k_cone = ((cone - HALF_FOV_COS) / (1.0 - HALF_FOV_COS)).clamp(0.0, 1.0);
                let k_range = if dist <= NEAR_RANGE {
                    1.0
                } else {
                    let t = ((dist - NEAR_RANGE) / (eff_range - NEAR_RANGE)).clamp(0.0, 1.0);
                    1.0 + (FAR_VIS - 1.0) * t
                };
                gain = k_cone * k_range * visibility;
                saw = gain > 0.0;
            }
        }

        // ── Hearing: continuous ambient loudness + discrete loud events ─────────
        let mut heard = false;
        let mut forced_alarm = false;
        if noise_radius > 0.0 {
            let d = Vec2::new(player_eye.x - self.pos.x, player_eye.z - self.pos.z).length();
            if d < noise_radius {
                self.detect += HEAR_RATE * dt;
                self.invest = Vec3::new(player_eye.x, self.pos.y, player_eye.z);
                self.has_invest = true;
                heard = true;
            }
        }
        for ev in noise {
            if ev.ttl <= 0.0 || ev.strength <= 0.0 {
                continue;
            }
            let d = Vec2::new(ev.pos.x - self.pos.x, ev.pos.z - self.pos.z).length();
            if d < ev.strength {
                let gain = (ev.strength / (d + 4.0)) * 1.2 * dt;
                // A gunshot force-alarms only the NEAR field; distant patrols within the wider
                // audible radius are DRAWN, not committed: their meter is ceilinged just under
                // Alarmed so sound alone can't flip the whole map (a Luger event is strength 120
                // over 3 s — uncapped it saturated everyone within earshot to Alarmed).
                if ev.strength >= LOUD_NOISE && d >= GUNSHOT_STARTLE {
                    self.detect = (self.detect + gain).min((ALARMED_ON - 0.1).max(self.detect));
                } else {
                    self.detect += gain;
                    if ev.strength >= LOUD_NOISE {
                        forced_alarm = true;
                    }
                }
                self.invest = ev.pos;
                self.has_invest = true;
                heard = true;
            }
        }

        if saw {
            self.detect += gain * GAIN_RATE * dt;
            self.invest = Vec3::new(player_eye.x, self.pos.y, player_eye.z);
            self.has_invest = true;
        } else if !heard {
            self.detect -= DECAY_RATE * dt;
        }
        if forced_alarm {
            self.detect = self.detect.max(ALARMED_ON);
        }
        self.detect = self.detect.clamp(0.0, 1.0);

        // ── State machine + behaviour ───────────────────────────────────────────
        let mut shot = None;
        match self.alert {
            Alert::Unaware => {
                self.patrol(dt, hf);
                if self.detect >= SUSPICIOUS_ON {
                    self.alert = Alert::Suspicious;
                }
            }
            Alert::Suspicious => {
                // Halt patrol; turn to face the contact (THE tell) and slow-walk toward it.
                let tgt = if self.has_invest {
                    self.invest
                } else {
                    player_eye
                };
                self.face(tgt, 2.2, dt);
                self.approach(tgt, INVEST_SPEED, dt, hf);
                if self.detect >= ALARMED_ON {
                    self.alert = Alert::Alarmed;
                    self.aim_windup = AIM_WINDUP;
                    self.alarm_signal = true;
                } else if self.detect < UNAWARE_OFF {
                    self.alert = Alert::Unaware;
                }
            }
            Alert::Alarmed => {
                let tgt = if saw {
                    player_eye
                } else if self.has_invest {
                    self.invest
                } else {
                    player_eye
                };
                self.face(tgt, 3.2, dt); // swing onto the target fast
                if self.aim_windup > 0.0 {
                    self.aim_windup -= dt;
                } else if saw && self.fire_cd <= 0.0 {
                    shot = Some(self.fire(player_eye));
                    self.fire_cd = INF_RELOAD;
                }
                if !saw {
                    // Lost sight: hunt toward the last-seen point, then relax.
                    self.approach(self.invest, HUNT_SPEED, dt, hf);
                    if self.detect < 0.6 {
                        self.alert = Alert::Suspicious;
                    }
                }
            }
        }

        shot
    }

    fn forward_from(&self, yaw: f32) -> Vec3 {
        Vec3::new(yaw.sin(), 0.0, -yaw.cos())
    }

    fn fire(&mut self, player_eye: Vec3) -> InfantryShot {
        let from = self.pos + Vec3::Y * EYE_H;
        let to = player_eye - from;
        let range = to.length().max(0.01);
        let base = to / range;
        // Deterministic dispersion — generous at range so a fleeing player outruns one shooter.
        let j = |n: u32| ((((n.wrapping_mul(2654435761)) >> 8) & 0xffff) as f32 / 32768.0) - 1.0;
        let disp = 0.02 + 0.02 * (range / VIEW_RANGE).min(1.0);
        // Jitter in a basis PERPENDICULAR to the shot — world-axis jitter collapsed the spread
        // to near-zero when firing along ±X (the X component ran down the barrel).
        let mut side = base.cross(Vec3::Y);
        if side.length_squared() < 1e-6 {
            side = Vec3::X; // firing straight up/down — any horizontal works
        }
        let side = side.normalize_or_zero();
        let vert = side.cross(base);
        let dir =
            (base + side * (j(self.tick) * disp) + vert * (j(self.tick.wrapping_add(53)) * disp))
                .normalize_or_zero();
        InfantryShot {
            from,
            dir,
            range: range + 2.0,
        }
    }

    /// Ease `yaw` toward facing `target` at up to `rate` rad/s.
    fn face(&mut self, target: Vec3, rate: f32, dt: f32) {
        let dx = target.x - self.pos.x;
        let dz = target.z - self.pos.z;
        if dx * dx + dz * dz > 1e-4 {
            let want = dx.atan2(-dz); // forward = (sin, 0, -cos)
            let mut dh = want - self.yaw;
            while dh > std::f32::consts::PI {
                dh -= std::f32::consts::TAU;
            }
            while dh < -std::f32::consts::PI {
                dh += std::f32::consts::TAU;
            }
            self.yaw += dh.clamp(-rate * dt, rate * dt);
        }
    }

    /// Walk toward `target` at `speed`, grounded on the heightfield.
    fn approach(&mut self, target: Vec3, speed: f32, dt: f32, hf: &Heightfield) {
        // Round an open cellar or trench on the way, never into its rim.
        let target = hf.walk_toward(self.pos, target);
        let to = Vec3::new(target.x - self.pos.x, 0.0, target.z - self.pos.z);
        let d = to.length();
        let before = self.pos;
        if d > 0.4 {
            self.pos += (to / d) * speed * dt;
        }
        self.pos = hf.ground_walker(before, self.pos);
    }

    /// Walk the patrol route; a route whose waypoints coincide becomes an idle scanning sentry.
    fn patrol(&mut self, dt: f32, hf: &Heightfield) {
        let wp = self.route[self.leg];
        let to = Vec3::new(wp.x - self.pos.x, 0.0, wp.z - self.pos.z);
        let d = to.length();
        if d < 1.0 {
            self.leg = (self.leg + 1) % self.route.len();
            // Idle scan (a sentry that never travels far still sweeps its gaze).
            // Wrapped phase — the raw tick starts at a hashed ~1e9 where f32 ULP >> 0.02 and
            // the sine froze; modulo keeps the argument small (6283 ticks = 20 clean periods).
            self.yaw += 0.6 * ((self.tick % 6283) as f32 * 0.02).sin() * dt;
        } else {
            // Round an open cellar or trench on the way, never into its rim.
            let next = hf.walk_toward(self.pos, wp);
            let step = Vec3::new(next.x - self.pos.x, 0.0, next.z - self.pos.z).normalize_or_zero();
            self.face(self.pos + step, 2.0, dt);
            let before = self.pos;
            self.pos += step * PATROL_SPEED * dt;
            self.pos = hf.ground_walker(before, self.pos);
        }
        self.pos.y = hf.height_at(self.pos.x, self.pos.z);
    }
}

fn measured_ground_speed(before: Vec3, after: Vec3, dt: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 {
        return 0.0;
    }
    let delta = Vec2::new(after.x - before.x, after.z - before.z);
    let speed = delta.length() / dt;
    if speed.is_finite() {
        speed
    } else {
        0.0
    }
}

/// Is the straight segment `a→b` blocked by terrain or any cover cylinder? The v1 LOS model:
/// buildings aren't in the heightfield, so coarse cylinders over the wall clusters stand in for
/// them (§6) — breaking behind a cluster actually hides you.
pub(crate) fn segment_blocked(a: Vec3, b: Vec3, hf: &Heightfield, cover: &[(Vec2, f32)]) -> bool {
    let to = b - a;
    let d = to.length();
    if d < 1e-3 {
        return false;
    }
    let dir = to / d;
    if ray_terrain(a, dir, hf, d).is_some() {
        return true;
    }
    let a2 = Vec2::new(a.x, a.z);
    let b2 = Vec2::new(b.x, b.z);
    for (c, r) in cover {
        // A sentry inside a coarse cover cylinder must be able to see out of its own volume. The
        // same cylinder still blocks observers whose ray starts outside it.
        if (a2 - *c).length_squared() <= r * r {
            continue;
        }
        if seg_circle_hit(a2, b2, *c, *r) {
            return true;
        }
    }
    false
}

/// Does the 2D segment `a→b` pass within `r` of `c`? (closest-point-on-segment distance test).
fn seg_circle_hit(a: Vec2, b: Vec2, c: Vec2, r: f32) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-6 {
        ((c - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t - c).length_squared() < r * r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_heightfield() -> Heightfield {
        Heightfield {
            res: 3,
            spacing: 10.0,
            heights: vec![0.0; 9],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(),
        }
    }

    #[test]
    fn mixed_escort_objective_update_never_adds_a_second_translation() {
        let mut soldier = Soldier::new(Vec3::ZERO, 0.3, [Vec3::ZERO; 4], 7, 0, 1);
        soldier.leg = 2;
        let route = [Vec3::new(30.0, 0.0, 0.0); 4];
        for _ in 0..120 {
            soldier.set_escort_route(route);
        }
        assert_eq!(soldier.route, route);
        assert_eq!(soldier.pos, Vec3::ZERO);
        assert_eq!(soldier.yaw, 0.3);
        assert_eq!(soldier.leg, 2);
        assert_eq!(soldier.motion_speed_m_s, 0.0);
        // The old passive escort path keeps its explicitly separate catch-up behavior.
        soldier.reanchor_escort(route, &flat_heightfield(), 0.5);
        assert!((soldier.pos.x - 1.7).abs() < 1e-6);
    }

    #[test]
    fn direct_kill_is_terminal_and_loot_is_one_or_two_rounds() {
        let route = [Vec3::ZERO; 4];
        for seed in 1..16 {
            let mut soldier = Soldier::new(Vec3::ZERO, 0.0, route, seed, 0, 1);
            assert!((1..=2).contains(&soldier.ammo_drop));
            assert!(soldier.kill());
            assert!(!soldier.alive);
            assert!(soldier.animator.is_dead());
            assert!(!soldier.kill());
        }
    }

    #[test]
    fn reused_escort_revives_its_terminal_pose_state() {
        let hf = flat_heightfield();
        let mut soldier = Soldier::new(Vec3::ZERO, 0.0, [Vec3::ZERO; 4], 7, 0, 1);
        assert!(soldier.kill());
        let route = [
            Vec3::new(4.0, 0.0, -3.0),
            Vec3::new(4.0, 0.0, -8.0),
            Vec3::new(6.0, 0.0, -8.0),
            Vec3::new(6.0, 0.0, -3.0),
        ];
        soldier.activate_escort(route, &hf);
        assert!(soldier.alive);
        assert!(!soldier.animator.is_dead());
        assert_eq!(soldier.motion_speed_m_s, 0.0);
    }

    #[test]
    fn physical_legal_shot_triggers_the_authored_fire_sequence() {
        let hf = flat_heightfield();
        let mut soldier = Soldier::new(Vec3::ZERO, 0.0, [Vec3::ZERO; 4], 11, 0, 1);
        soldier.alert = Alert::Alarmed;
        soldier.detect = 1.0;
        soldier.aim_windup = 0.0;
        let target = Vec3::new(0.0, EYE_H, -10.0);
        let step =
            soldier.update_controlled(target, 1.0, (0.0, 1.0), &[], &[], &hf, 1.0 / 60.0, None);
        assert!(step.shot.is_some());
        assert_eq!(
            soldier.animator.sequence(),
            crate::soviet_animation::SovietSequenceKind::Fire
        );
    }

    #[test]
    fn measured_pose_speed_is_horizontal_and_fail_closed() {
        assert!(
            (measured_ground_speed(Vec3::ZERO, Vec3::new(3.0, 99.0, 4.0), 2.0) - 2.5).abs() < 1e-6
        );
        assert_eq!(measured_ground_speed(Vec3::ZERO, Vec3::ONE, 0.0), 0.0);
        assert_eq!(
            measured_ground_speed(Vec3::ZERO, Vec3::splat(f32::NAN), 1.0),
            0.0
        );
    }

    #[test]
    fn guard_can_see_out_of_the_cover_volume_containing_its_eye() {
        let hf = flat_heightfield();
        let cover = [(Vec2::ZERO, 2.0)];
        let guard_eye = Vec3::new(0.0, EYE_H, 0.0);
        let outside_target = Vec3::new(10.0, EYE_H, 0.0);
        assert!(!segment_blocked(guard_eye, outside_target, &hf, &cover));
    }

    #[test]
    fn same_cover_still_hides_occupants_from_exterior_observers() {
        let hf = flat_heightfield();
        let cover = [(Vec2::ZERO, 2.0)];
        let exterior_eye = Vec3::new(-10.0, EYE_H, 0.0);
        assert!(segment_blocked(
            exterior_eye,
            Vec3::new(0.0, EYE_H, 0.0),
            &hf,
            &cover
        ));
        assert!(segment_blocked(
            exterior_eye,
            Vec3::new(10.0, EYE_H, 0.0),
            &hf,
            &cover
        ));
    }

    #[test]
    fn leaving_own_cover_does_not_ignore_a_later_cover_volume() {
        let hf = flat_heightfield();
        let cover = [(Vec2::ZERO, 2.0), (Vec2::new(6.0, 0.0), 1.0)];
        assert!(segment_blocked(
            Vec3::new(0.0, EYE_H, 0.0),
            Vec3::new(10.0, EYE_H, 0.0),
            &hf,
            &cover
        ));
    }

    #[test]
    fn live_door_occlusion_gates_scripted_and_learned_observation_together() {
        let hf = flat_heightfield();
        let mut soldier = Soldier::new(Vec3::ZERO, 0.0, [Vec3::ZERO;4], 7, 0, 1);
        let target = Vec3::new(0.0, EYE_H, -8.0);
        for _ in 0..30 {
            let step = soldier.update_controlled_with_visibility(target, 1.0, (1.0,1.0), &[], &[], &hf, 0.05, None, true);
            assert!(step.shot.is_none());
            assert!(!step.action.fire && !step.action.aim);
            assert_eq!(soldier.detect, 0.0);
            assert!(!soldier.has_invest);
        }
        soldier.yaw = 0.0;
        let _ = soldier.update_controlled_with_visibility(target, 1.0, (1.0,1.0), &[], &[], &hf, 0.05, None, false);
        assert!(soldier.detect > 0.0, "opening the actual leaf must restore ordinary perception");
    }

    #[test]
    fn learned_rifle_trigger_cannot_fire_without_visible_legal_los() {
        let proposed = FootAction {
            aim: true,
            fire: true,
            weapon: WeaponSelection::AntiTank,
            ..FootAction::default()
        };
        let hidden = gate_soviet_action(proposed, false, true);
        assert!(!hidden.aim);
        assert!(!hidden.fire);
        assert_eq!(hidden.weapon, WeaponSelection::Rifle);

        let reloading = gate_soviet_action(proposed, true, false);
        assert!(reloading.aim);
        assert!(!reloading.fire);

        let legal = gate_soviet_action(proposed, true, true);
        assert!(legal.aim && legal.fire);
    }
}
