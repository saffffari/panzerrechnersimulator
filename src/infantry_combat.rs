//! Shared renderer-free mixed-team combat authority. Training uses these same movement, weapon,
//! ray/body and blast rules as runtime integration. Cover is a caller-supplied live AABB registry;
//! these are game abstractions, not a validated human-wound or exterior blast solver.
use crate::foot_ai::{
    FootAction, FootObservation, GoalKind, ObservationInput, PolicyRole, RelativeCue,
    WeaponSelection,
};
use glam::Vec3;
pub const RIFLE_DAMAGE: f32 = 34.0;
pub const TURN_RATE_RAD_S: f32 = 3.2;
pub const RIFLE_INTERVAL: f32 = 1.6;
pub const RIFLE_RANGE: f32 = 120.0;
pub const INFANTRY_SENSOR_RANGE: f32 = 45.0;
pub const RIFLE_RELOAD: f32 = 3.0;
pub const MG_INTERVAL: f32 = 0.075;

/// Trigger release cannot reset the remaining bolt cycle. Shared by training and live tanks;
/// otherwise a rapidly toggling policy can fire faster than the physical cyclic rate.
pub fn cycle_machine_gun(timer: &mut f32, ammunition: &mut u32, trigger: bool, dt: f32) -> u32 {
    if !dt.is_finite() || dt <= 0.0 {
        return 0;
    }
    *timer -= dt;
    if !trigger || *ammunition == 0 {
        *timer = timer.max(0.0);
        return 0;
    }
    let mut rounds = 0;
    while *timer <= 0.0 && *ammunition > 0 {
        *timer += MG_INTERVAL;
        *ammunition -= 1;
        rounds += 1;
    }
    rounds
}
pub const MG_RANGE: f32 = 900.0;
pub const MG_HEAT_GAIN: f32 = 0.28;
pub const MG_HEAT_LOSS: f32 = 0.45;
pub const MG_RESUME: f32 = 0.35;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    German,
    Soviet,
}
/// Visual pose capability is part of the shared trainer/runtime physical contract. The selected
/// Soviet source is absent; its rigid fallback cannot truthfully receive a crouched hitbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseSupport {
    StandingOnly,
    InstantCrouch,
    GermanArticulatedV1,
}
pub const CROUCH_STAGES: usize = 5;
pub const CROUCH_TRANSITION_S: f32 = 0.4;
/// Measured from the evaluated, fixed-root German derivative (aim and relaxed agree).
pub const GERMAN_BODY_HEIGHTS: [f32; CROUCH_STAGES] = [1.75, 1.5875, 1.425, 1.2625, 1.1];
pub const GERMAN_EYE_HEIGHTS: [f32; CROUCH_STAGES] =
    [1.6478746, 1.4853747, 1.3228745, 1.1603746, 0.9978746];
pub const GERMAN_EYE_X: f32 = 0.00067461;
pub const GERMAN_EYE_FORWARDS: [f32; CROUCH_STAGES] = [
    0.00620151,
    -0.04879849,
    -0.10379849,
    -0.15879849,
    -0.21379849,
];
impl Team {
    pub fn role(self) -> PolicyRole {
        match self {
            Self::German => PolicyRole::GermanRifle,
            Self::Soviet => PolicyRole::SovietRifle,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    Rifle,
    P08,
    MachineGun,
}
#[derive(Clone, Copy, Debug)]
pub struct Cover {
    pub min: Vec3,
    pub max: Vec3,
}

/// The hard cover the infantry model queries: a plain list of boxes (training), or the game
/// world's spatial index over thousands of building modules. Both answer the same two questions
/// exactly as the list scan does.
pub trait CoverSet {
    /// Nearest `ray_box` entry distance along a unit `direction` within `max_distance`.
    fn first_hit(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32>;
    /// Is there a box whose XZ footprint widened by `margin` holds `point` strictly inside, and
    /// whose height range overlaps `(y0, y1)`?
    fn occupies(&self, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool;
}

/// The occupancy rule of one box (strict inequalities, as the movement gate always used).
pub fn cover_occupies(c: Cover, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool {
    point.x > c.min.x - margin
        && point.x < c.max.x + margin
        && point.z > c.min.z - margin
        && point.z < c.max.z + margin
        && y0 < c.max.y
        && y1 > c.min.y
}

impl CoverSet for [Cover] {
    fn first_hit(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        self.iter()
            .filter_map(|&b| ray_box(origin, direction, b, max_distance))
            .min_by(f32::total_cmp)
    }

    fn occupies(&self, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool {
        self.iter().any(|&c| cover_occupies(c, point, margin, y0, y1))
    }
}

impl CoverSet for Vec<Cover> {
    fn first_hit(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        self.as_slice().first_hit(origin, direction, max_distance)
    }

    fn occupies(&self, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool {
        self.as_slice().occupies(point, margin, y0, y1)
    }
}

impl<const N: usize> CoverSet for [Cover; N] {
    fn first_hit(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        self.as_slice().first_hit(origin, direction, max_distance)
    }

    fn occupies(&self, point: Vec3, margin: f32, y0: f32, y1: f32) -> bool {
        self.as_slice().occupies(point, margin, y0, y1)
    }
}
#[derive(Clone, Debug)]
pub struct Combatant {
    pub id: u32,
    pub team: Team,
    pub pos: Vec3,
    pub yaw: f32,
    pub hp: f32,
    pub crouched: bool,
    pub crouch_requested: bool,
    pub crouch_phase: f32,
    pub pose_support: PoseSupport,
    pub aiming: bool,
    pub velocity: Vec3,
    pub cooldown: f32,
    pub reload: f32,
    pub magazine: u32,
    pub reserve: u32,
    pub suppression: f32,
    /// A source-derived surface or exposed anatomical aim point in a perception-only snapshot.
    /// It is never inserted into the live actor/damage list. Ordinary humans retain their pose.
    pub perception_point: Option<Vec3>,
    pub last_seen: Option<Vec3>,
    /// Posture captured by the same visible observation as `last_seen`, never a hidden live lookup.
    pub last_seen_crouched: Option<bool>,
    pub last_seen_body_height: Option<f32>,
    pub last_seen_torso_offset: Option<Vec3>,
    pub memory_s: f32,
}
impl Combatant {
    pub fn new(id: u32, team: Team, pos: Vec3, yaw: f32) -> Self {
        Self {
            id,
            team,
            pos,
            yaw,
            hp: 100.0,
            crouched: false,
            crouch_requested: false,
            crouch_phase: 0.0,
            pose_support: if team == Team::German {
                PoseSupport::GermanArticulatedV1
            } else {
                PoseSupport::StandingOnly
            },
            aiming: false,
            velocity: Vec3::ZERO,
            cooldown: 0.5,
            reload: 0.0,
            magazine: 5,
            reserve: 30,
            suppression: 0.0,
            perception_point: None,
            last_seen: None,
            last_seen_crouched: None,
            last_seen_body_height: None,
            last_seen_torso_offset: None,
            memory_s: 0.0,
        }
    }
    pub fn alive(&self) -> bool {
        self.hp > 0.0
    }
    pub fn eye(&self) -> Vec3 {
        if let Some(point) = self.perception_point { return point; }
        if self.pose_support == PoseSupport::GermanArticulatedV1 {
            self.pos
                + Vec3::Y * GERMAN_EYE_HEIGHTS[self.crouch_stage()]
                + self.forward() * GERMAN_EYE_FORWARDS[self.crouch_stage()]
                - Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()) * GERMAN_EYE_X
        } else {
            self.pos
                + Vec3::Y
                    * if self.pose_support == PoseSupport::InstantCrouch && self.crouched {
                        1.05
                    } else {
                        1.6
                    }
        }
    }
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos())
    }
    pub fn body_height(&self) -> f32 {
        match self.pose_support {
            PoseSupport::StandingOnly => 1.75,
            PoseSupport::InstantCrouch => body_height(self.crouched),
            PoseSupport::GermanArticulatedV1 => GERMAN_BODY_HEIGHTS[self.crouch_stage()],
        }
    }
    pub fn torso_offset(&self) -> Vec3 {
        if let Some(point) = self.perception_point { return point-self.pos; }
        let retreat = if self.pose_support == PoseSupport::GermanArticulatedV1 {
            -0.22 * self.crouch_stage() as f32 / 4.0
        } else {
            0.0
        };
        Vec3::Y * (self.body_height() * (1.15 / 1.75)) + self.forward() * retreat
    }
    pub fn crouch_stage(&self) -> usize {
        match self.pose_support {
            PoseSupport::StandingOnly => 0,
            PoseSupport::InstantCrouch => {
                if self.crouched {
                    4
                } else {
                    0
                }
            }
            PoseSupport::GermanArticulatedV1 => {
                (self.crouch_phase.clamp(0.0, 1.0) * 4.0).round() as usize
            }
        }
    }
    fn advance_stance(&mut self, requested: bool, dt: f32) {
        self.crouch_requested = requested;
        self.crouch_phase = match self.pose_support {
            PoseSupport::StandingOnly => 0.0,
            PoseSupport::InstantCrouch => {
                if requested {
                    1.0
                } else {
                    0.0
                }
            }
            PoseSupport::GermanArticulatedV1 => (self.crouch_phase
                + if requested {
                    dt / CROUCH_TRANSITION_S
                } else {
                    -dt / CROUCH_TRANSITION_S
                })
            .clamp(0.0, 1.0),
        };
        self.crouched = match self.pose_support {
            PoseSupport::InstantCrouch => requested,
            _ => self.crouch_stage() >= 2,
        };
    }
    /// Preserve the standing torso aim fraction within the observed posture's gameplay hitbox.
    /// Navigation/learned horizontal cues continue to use remembered feet, not this elevated point.
    pub fn last_observed_aim_point(&self) -> Option<Vec3> {
        Some(self.last_seen? + self.last_seen_torso_offset?)
    }
}
pub fn body_height(crouched: bool) -> f32 {
    if crouched {
        1.1
    } else {
        1.75
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub shooter: u32,
    pub team: Team,
    pub origin: Vec3,
    pub direction: Vec3,
    pub range: f32,
    pub weapon: Weapon,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Hit {
    pub actor_id: Option<u32>,
    pub damage: f32,
    pub friendly: bool,
    pub killed: bool,
    pub distance: f32,
}
pub fn ray_box(origin: Vec3, direction: Vec3, bounds: Cover, max_distance: f32) -> Option<f32> {
    let mut near = 0.001f32;
    let mut far = max_distance;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-7 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
        } else {
            let a = (bounds.min[axis] - origin[axis]) / direction[axis];
            let b = (bounds.max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= near).then_some(near)
}
pub fn first_cover<C: CoverSet + ?Sized>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    cover: &C,
) -> Option<f32> {
    cover.first_hit(origin, direction, max_distance)
}
pub fn clear_segment<C: CoverSet + ?Sized>(origin: Vec3, target: Vec3, cover: &C) -> bool {
    let delta = target - origin;
    let distance = delta.length();
    distance > 0.001
        && first_cover(
            origin,
            delta / distance,
            (distance - 0.002).max(0.001),
            cover,
        )
        .is_none()
}
pub fn small_arms_damage(weapon: Weapon, armored: bool) -> f32 {
    if armored {
        0.0
    } else {
        match weapon {
            Weapon::Rifle => RIFLE_DAMAGE,
            Weapon::P08 | Weapon::MachineGun => 100.0,
        }
    }
}
/// Common broad exterior HE gameplay law. The source must be a real terminal blast, and the
/// caller supplies actual line-of-sight blocking. Filler mass scales the declared seven-metre
/// reference support by cube-root similarity; this does not model indirect fragments round cover.
pub fn he_blast_damage(distance_m: f32, filler_kg: f32, blocked: bool) -> f32 {
    if blocked || !distance_m.is_finite() || !filler_kg.is_finite() || filler_kg <= 0.0 {
        return 0.0;
    }
    let radius = 7.0 * (filler_kg / 0.9).cbrt();
    if distance_m >= radius {
        0.0
    } else {
        (200.0 * (1.0 - distance_m.max(0.0) / radius)).max(0.0)
    }
}
pub fn resolve_hitscan<C: CoverSet + ?Sized>(shot: Shot, actors: &mut [Combatant], cover: &C) -> Hit {
    let direction = shot.direction.normalize_or_zero();
    if direction == Vec3::ZERO {
        return Hit::default();
    }
    let mut nearest = first_cover(shot.origin, direction, shot.range, cover).unwrap_or(shot.range);
    let mut target = None;
    for (i, actor) in actors.iter().enumerate() {
        if !actor.alive() || actor.id == shot.shooter {
            continue;
        }
        let half = Vec3::new(0.25, 0.0, 0.25);
        let height = actor.body_height();
        let bounds = Cover {
            min: actor.pos - half,
            max: actor.pos + half + Vec3::Y * height,
        };
        if let Some(t) = ray_box(shot.origin, direction, bounds, nearest) {
            if t < nearest {
                nearest = t;
                target = Some(i);
            }
        }
    }
    let mut hit = Hit {
        distance: nearest,
        ..Hit::default()
    };
    if let Some(i) = target {
        let actor = &mut actors[i];
        hit.damage = small_arms_damage(shot.weapon, false).min(actor.hp.max(0.0));
        hit.actor_id = Some(actor.id);
        hit.friendly = actor.team == shot.team;
        actor.hp = (actor.hp - hit.damage).max(0.0);
        hit.killed = !actor.alive();
        actor.suppression = 1.0;
    }
    hit
}
pub fn blast<C: CoverSet + ?Sized>(
    point: Vec3,
    filler_kg: f32,
    team: Team,
    actors: &mut [Combatant],
    cover: &C,
) -> Vec<Hit> {
    let mut hits = Vec::new();
    for actor in actors.iter_mut().filter(|a| a.alive()) {
        let target = actor.pos + actor.torso_offset() * (0.9 / 1.15);
        let distance = point.distance(target);
        let damage = he_blast_damage(distance, filler_kg, !clear_segment(point, target, cover))
            .min(actor.hp);
        if damage > 0.0 {
            actor.hp -= damage;
            actor.suppression = 1.0;
            hits.push(Hit {
                actor_id: Some(actor.id),
                damage,
                friendly: actor.team == team,
                killed: !actor.alive(),
                distance,
            });
        }
    }
    hits
}
fn cue(origin: Vec3, yaw: f32, target: Vec3, range: f32) -> RelativeCue {
    let delta = Vec3::new(target.x - origin.x, 0.0, target.z - origin.z);
    let d = delta.length();
    let dir = delta.normalize_or_zero();
    RelativeCue {
        forward: dir.dot(Vec3::new(yaw.sin(), 0.0, -yaw.cos())),
        right: dir.dot(Vec3::new(yaw.cos(), 0.0, yaw.sin())),
        distance: (d / range).clamp(0.0, 1.0),
    }
}
/// Cheap eligibility shared by observation and the runtime's more expensive world LOS query.
/// Eye distance, horizontal FOV and boundary comparisons match the sensor's existing rules.
pub fn sight_candidate(actor: &Combatant, target: &Combatant) -> bool {
    if !target.alive() || target.team == actor.team {
        return false;
    }
    let delta = target.eye() - actor.eye();
    delta.length() <= INFANTRY_SENSOR_RANGE
        && actor
            .forward()
            .dot(Vec3::new(delta.x, 0.0, delta.z).normalize_or_zero())
            > 0.342
}

/// Sight is range/FOV/cover gated. Hidden exact target positions/HP never enter the observation;
/// last-known coordinates expire after five seconds. The objective is an assigned map waypoint.
pub fn observe<C: CoverSet + ?Sized>(
    actor: &mut Combatant,
    others: &[Combatant],
    cover: &C,
    objective: Vec3,
    dt: f32,
) -> FootObservation {
    actor.memory_s = (actor.memory_s - dt).max(0.0);
    if actor.memory_s <= 0.0 {
        actor.last_seen = None;
        actor.last_seen_crouched = None;
        actor.last_seen_body_height = None;
        actor.last_seen_torso_offset = None;
    }
    let target = others
        .iter()
        .filter(|b| sight_candidate(actor, b))
        .filter(|b| clear_segment(actor.eye(), b.eye(), cover))
        .min_by(|a, b| {
            a.pos
                .distance_squared(actor.pos)
                .total_cmp(&b.pos.distance_squared(actor.pos))
        });
    if let Some(target) = target {
        actor.last_seen = Some(target.pos);
        actor.last_seen_crouched = Some(target.crouched);
        actor.last_seen_body_height = Some(target.body_height());
        actor.last_seen_torso_offset = Some(target.torso_offset());
        actor.memory_s = 5.0;
    }
    let ally = others
        .iter()
        .filter(|b| {
            b.alive()
                && b.team == actor.team
                && b.id != actor.id
                && b.pos.distance(actor.pos) <= INFANTRY_SENSOR_RANGE
        })
        .min_by(|a, b| {
            a.pos
                .distance_squared(actor.pos)
                .total_cmp(&b.pos.distance_squared(actor.pos))
        });
    let goal = actor.last_seen.unwrap_or(objective);
    let mut probes = [1.0; 8];
    for (i, value) in probes.iter_mut().enumerate() {
        let a = actor.yaw + i as f32 * std::f32::consts::FRAC_PI_4;
        *value = first_cover(
            actor.pos + Vec3::Y * 0.6,
            Vec3::new(a.sin(), 0.0, -a.cos()),
            6.0,
            cover,
        )
        .unwrap_or(6.0)
            / 6.0;
    }
    let local_velocity = cue(Vec3::ZERO, actor.yaw, actor.velocity, 1.0);
    ObservationInput {
        role: actor.team.role(),
        goal: cue(actor.pos, actor.yaw, goal, INFANTRY_SENSOR_RANGE),
        goal_kind: GoalKind::Intercept,
        hp_fraction: actor.hp / 100.0,
        velocity_forward: local_velocity.forward * actor.velocity.length() / 1.7,
        velocity_right: local_velocity.right * actor.velocity.length() / 1.7,
        crouched: actor.crouched,
        aiming: actor.aiming,
        ammo_fraction: actor.magazine as f32 / 5.0,
        reload_fraction: actor.reload / RIFLE_RELOAD,
        detected: target.is_some(),
        suppression: actor.suppression,
        stamina_fraction: 1.0,
        tiger: RelativeCue::default(),
        tiger_mountable: false,
        target: actor
            .last_seen
            .map(|p| cue(actor.pos, actor.yaw, p, INFANTRY_SENSOR_RANGE))
            .unwrap_or_default(),
        target_visible: target.is_some(),
        target_last_known: actor.last_seen.is_some(),
        target_hp_fraction: if target.is_some() { 1.0 } else { 0.0 },
        target_armored: false,
        nearest_ally: ally
            .map(|a| cue(actor.pos, actor.yaw, a.pos, INFANTRY_SENSOR_RANGE))
            .unwrap_or_default(),
        nearest_ally_alive: ally.is_some(),
        cover_clearance: probes,
        terrain_slope_forward: 0.0,
        terrain_slope_right: 0.0,
        can_interact: false,
        can_jump: false,
        can_fire: actor.cooldown <= 0.0
            && actor.reload <= 0.0
            && actor.magazine > 0
            && target.is_some(),
        can_reload: actor.magazine < 5 && actor.reserve > 0 && actor.reload <= 0.0,
        weapon: WeaponSelection::Rifle,
    }
    .encode()
}
pub fn apply_action<C: CoverSet + ?Sized>(
    actor: &mut Combatant,
    obs: &FootObservation,
    action: FootAction,
    cover: &C,
    dt: f32,
    tick: u64,
) -> Option<Shot> {
    if !actor.alive() || !dt.is_finite() || dt <= 0.0 {
        return None;
    }
    actor.cooldown = (actor.cooldown - dt).max(0.0);
    actor.suppression = (actor.suppression - dt * 0.8).max(0.0);
    if actor.reload > 0.0 {
        actor.reload = (actor.reload - dt).max(0.0);
        if actor.reload == 0.0 {
            let n = (5 - actor.magazine).min(actor.reserve);
            actor.magazine += n;
            actor.reserve -= n;
        }
    }
    let action = action.bounded();
    actor.advance_stance(action.crouch, dt);
    actor.aiming = action.aim && obs.values()[26] > 0.0;
    actor.yaw += action.turn * TURN_RATE_RAD_S * dt;
    let velocity = (actor.forward() * action.forward
        + Vec3::new(actor.yaw.cos(), 0.0, actor.yaw.sin()) * action.strafe)
        .clamp_length_max(1.0)
        * (1.7 - 0.6 * actor.crouch_stage() as f32 / 4.0);
    let proposed = actor.pos + velocity * dt;
    let occupied = cover.occupies(
        proposed,
        0.25,
        actor.pos.y,
        actor.pos.y + actor.body_height(),
    );
    actor.velocity = if occupied { Vec3::ZERO } else { velocity };
    actor.pos += actor.velocity * dt;
    if action.reload && actor.reload <= 0.0 && actor.magazine < 5 && actor.reserve > 0 {
        actor.reload = RIFLE_RELOAD;
    }
    if action.fire
        && actor.aiming
        && actor.cooldown <= 0.0
        && actor.reload <= 0.0
        && actor.magazine > 0
    {
        let aim_point = actor.last_observed_aim_point()?;
        actor.magazine -= 1;
        actor.cooldown = RIFLE_INTERVAL;
        let delta = aim_point - actor.eye();
        // The horizontal forward vector has unit length: adding Y requires tan(pitch)=dy/dxz,
        // not sin(pitch)=dy/full_distance. Keep azimuth entirely under the learned turn action.
        let vertical = delta.y / Vec3::new(delta.x, 0.0, delta.z).length().max(0.1);
        let right = Vec3::new(actor.yaw.cos(), 0.0, actor.yaw.sin());
        let j = |n: u64| {
            let mut n = n.wrapping_mul(0x9e3779b97f4a7c15);
            n ^= n >> 29;
            (n >> 40) as f32 / 8_388_608.0 - 1.0
        };
        let spread = 0.011 - 0.004 * actor.crouch_stage() as f32 / 4.0;
        let dir = (actor.forward()
            + Vec3::Y * vertical
            + right * j(tick ^ actor.id as u64) * spread
            + Vec3::Y * j(tick ^ 0xabc ^ actor.id as u64) * spread)
            .normalize();
        return Some(Shot {
            shooter: actor.id,
            team: actor.team,
            origin: actor.eye(),
            direction: dir,
            range: RIFLE_RANGE,
            weapon: Weapon::Rifle,
        });
    }
    None
}

#[cfg(test)]
mod cycling_tests {
    use super::*;
    #[test]
    fn trigger_chatter_cannot_bypass_bolt_cycle_at_game_or_training_tick_rates() {
        for hz in [20u32, 120, 240] {
            for chatter in [false, true] {
                let mut timer = 0.0;
                let mut ammo = 1000;
                let mut shots = 0;
                for tick in 0..hz * 3 {
                    shots += cycle_machine_gun(
                        &mut timer,
                        &mut ammo,
                        !chatter || tick % 2 == 0,
                        1.0 / hz as f32,
                    );
                }
                assert!(
                    shots <= 41,
                    "{shots} rounds exceeded the 800 rpm cyclic budget at {hz} Hz"
                );
                assert_eq!(ammo, 1000 - shots);
                if !chatter {
                    assert!(shots >= 39);
                }
            }
        }
        let mut timer = 0.060;
        let mut ammo = 1;
        assert_eq!(cycle_machine_gun(&mut timer, &mut ammo, false, 0.010), 0);
        assert_eq!(cycle_machine_gun(&mut timer, &mut ammo, true, 0.010), 0);
        assert_eq!(ammo, 1);
    }
}

#[cfg(test)]
mod pose_tests {
    use super::*;

    #[test]
    fn sight_candidate_preserves_eye_range_fov_and_living_enemy_gates() {
        let observer = Combatant::new(1, Team::Soviet, Vec3::ZERO, 0.0);
        // Equal standing eye offsets make the exact 45 m boundary representable.
        let mut target = Combatant::new(2, Team::Soviet, Vec3::NEG_Z * 45.0, 0.0);
        target.team = Team::German;
        assert!(sight_candidate(&observer, &target));
        target.pos.z = -45.001;
        assert!(!sight_candidate(&observer, &target));
        target.pos = Vec3::new(0.0, 40.0, -30.0);
        assert!(!sight_candidate(&observer, &target), "range is 3D eye distance");
        for (cosine, expected) in [(0.3419_f32, false), (0.3421_f32, true)] {
            target.pos = Vec3::new((1.0 - cosine * cosine).sqrt(), 0.0, -cosine) * 20.0;
            assert_eq!(sight_candidate(&observer, &target), expected);
        }
        target.pos = Vec3::Z * 10.0;
        assert!(!sight_candidate(&observer, &target));
        target.pos = Vec3::NEG_Z * 10.0;
        target.hp = 0.0;
        assert!(!sight_candidate(&observer, &target));
        target.hp = 100.0;
        target.team = observer.team;
        assert!(!sight_candidate(&observer, &target));
    }

    #[test]
    fn requested_crouch_cannot_change_a_standing_only_body() {
        let mut actor = Combatant::new(7, Team::Soviet, Vec3::new(0.0, 3.0, 0.0), 0.0);
        let eye = actor.eye();
        for _ in 0..60 {
            actor.advance_stance(true, 1.0 / 120.0);
        }
        assert!(actor.crouch_requested);
        assert!(!actor.crouched);
        assert_eq!(actor.crouch_stage(), 0);
        assert_eq!(actor.body_height(), 1.75);
        assert_eq!(actor.eye(), eye);
    }

    #[test]
    fn articulated_transition_shares_five_physical_stages_at_training_and_game_rates() {
        for hz in [20, 120, 240] {
            let mut actor = Combatant::new(1, Team::German, Vec3::new(0.0, 2.0, 0.0), 0.0);
            let mut seen = [false; CROUCH_STAGES];
            seen[0] = true;
            for _ in 0..hz / 2 {
                actor.advance_stance(true, 1.0 / hz as f32);
                let stage = actor.crouch_stage();
                seen[stage] = true;
                assert_eq!(actor.body_height(), GERMAN_BODY_HEIGHTS[stage]);
                assert!((actor.eye().y - actor.pos.y - GERMAN_EYE_HEIGHTS[stage]).abs() < 1e-6);
            }
            assert!(seen.into_iter().all(|v| v));
            assert_eq!(actor.crouch_stage(), 4);
            for _ in 0..hz / 2 {
                actor.advance_stance(false, 1.0 / hz as f32);
            }
            assert_eq!(actor.crouch_stage(), 0);
            assert!(!actor.crouched);
            assert_eq!(actor.pos.y, 2.0, "crouching never sinks the actor root");
        }
    }

    #[test]
    fn observed_intermediate_height_is_remembered_without_hidden_pose_leaks() {
        let mut observer = Combatant::new(1, Team::Soviet, Vec3::ZERO, 0.0);
        let mut target = Combatant::new(2, Team::German, Vec3::new(0.0, 2.0, -18.0), 0.0);
        target.advance_stance(true, 0.2);
        observe(&mut observer, &[target.clone()], &[], Vec3::ZERO, 0.1);
        assert_eq!(observer.last_seen_body_height, Some(GERMAN_BODY_HEIGHTS[2]));
        let point = observer.last_observed_aim_point();
        let wall = Cover {
            min: Vec3::new(-5.0, -5.0, -6.0),
            max: Vec3::new(5.0, 10.0, -5.0),
        };
        target.advance_stance(true, 0.4);
        observe(&mut observer, &[target.clone()], &[wall], Vec3::ZERO, 1.0);
        assert_eq!(observer.last_observed_aim_point(), point);
        observe(&mut observer, &[target], &[wall], Vec3::ZERO, 4.1);
        assert_eq!(observer.last_seen_body_height, None);
        assert_eq!(observer.last_observed_aim_point(), None);
    }

    #[test]
    fn body_hits_and_blast_visibility_follow_actual_crouch_stage() {
        let standing = Combatant::new(2, Team::German, Vec3::ZERO, 0.0);
        let mut crouched = standing.clone();
        crouched.advance_stance(true, 0.4);
        let shot = Shot {
            shooter: 1,
            team: Team::Soviet,
            origin: Vec3::new(0.0, 1.3, -5.0),
            direction: Vec3::Z,
            range: 20.0,
            weapon: Weapon::Rifle,
        };
        assert_eq!(
            resolve_hitscan(shot, &mut [standing.clone()], &[]).actor_id,
            Some(2)
        );
        assert_eq!(
            resolve_hitscan(shot, &mut [crouched.clone()], &[]).actor_id,
            None
        );
        let overhead = Cover {
            min: Vec3::new(-2.0, 0.65, -3.0),
            max: Vec3::new(2.0, 2.0, -2.0),
        };
        let source = Vec3::new(0.0, 0.55, -5.0);
        assert!(blast(source, 0.9, Team::Soviet, &mut [standing], &[overhead]).is_empty());
        assert_eq!(
            blast(source, 0.9, Team::Soviet, &mut [crouched], &[overhead]).len(),
            1
        );
    }
}

#[cfg(test)]
mod rifle_aim_tests {
    use super::*;

    // These tests exercise both supported endpoint postures independently of NPC asset availability.
    fn test_actor(id: u32, team: Team, pos: Vec3, yaw: f32) -> Combatant {
        let mut actor = Combatant::new(id, team, pos, yaw);
        actor.pose_support = PoseSupport::InstantCrouch;
        actor
    }

    fn fire_action(crouched: bool) -> FootAction {
        FootAction {
            aim: true,
            fire: true,
            crouch: crouched,
            ..FootAction::default()
        }
    }

    #[test]
    fn aligned_rifle_hits_observed_standing_and_crouched_targets_on_slopes() {
        // Retain the real deterministic spread, body hitboxes and hit resolver. These modest
        // distances isolate elevation correctness from learned yaw error or range dispersion.
        for shooter_crouched in [false, true] {
            for target_crouched in [false, true] {
                for (distance, rise) in [
                    (18.0, -3.0),
                    (18.0, 0.0),
                    (18.0, 3.0),
                    (4.0, -6.0),
                    (4.0, 6.0),
                ] {
                    for tick in 0..32 {
                        let mut shooter = test_actor(1, Team::German, Vec3::ZERO, 0.0);
                        shooter.crouched = shooter_crouched;
                        shooter.cooldown = 0.0;
                        let mut target =
                            test_actor(2, Team::Soviet, Vec3::new(0.0, rise, -distance), 0.0);
                        target.crouched = target_crouched;
                        let obs = observe(
                            &mut shooter,
                            &[target.clone()],
                            &[],
                            target.pos,
                            1.0 / 120.0,
                        );
                        let point = shooter.last_observed_aim_point().unwrap();
                        assert!(
                            point.y > target.pos.y && point.y < target.pos.y + target.body_height()
                        );
                        let shot = apply_action(
                            &mut shooter,
                            &obs,
                            fire_action(shooter_crouched),
                            &[],
                            1.0 / 120.0,
                            tick,
                        )
                        .unwrap();
                        assert!(shot.direction.is_finite());
                        let hit = resolve_hitscan(shot, &mut [target], &[]);
                        assert_eq!(hit.actor_id, Some(2),
                            "postures={shooter_crouched}/{target_crouched} distance={distance} rise={rise} tick={tick}");
                        assert_eq!(hit.damage, RIFLE_DAMAGE);
                    }
                }
            }
        }
    }

    #[test]
    fn hidden_posture_and_elevation_changes_do_not_update_remembered_aim_or_allow_fire() {
        let mut observer = test_actor(1, Team::German, Vec3::ZERO, 0.0);
        observer.cooldown = 0.0;
        let mut target = test_actor(2, Team::Soviet, Vec3::new(0.0, 2.0, -18.0), 0.0);
        target.crouched = true;
        observe(&mut observer, &[target.clone()], &[], Vec3::ZERO, 0.1);
        let remembered_feet = observer.last_seen;
        let remembered_aim = observer.last_observed_aim_point();
        let wall = Cover {
            min: Vec3::new(-5.0, -10.0, -6.0),
            max: Vec3::new(5.0, 10.0, -5.0),
        };
        target.pos = Vec3::new(2.0, 3.0, -20.0);
        target.crouched = false;
        let hidden = observe(&mut observer, &[target.clone()], &[wall], Vec3::ZERO, 1.0);
        assert_eq!(hidden.values()[26], -1.0);
        assert_eq!(observer.last_seen, remembered_feet);
        assert_eq!(observer.last_seen_crouched, Some(true));
        assert_eq!(observer.last_observed_aim_point(), remembered_aim);
        assert!(
            apply_action(&mut observer, &hidden, fire_action(false), &[wall], 0.01, 0).is_none()
        );
        assert_eq!(observer.magazine, 5);
        observe(&mut observer, &[target.clone()], &[wall], Vec3::ZERO, 4.1);
        assert_eq!(observer.last_seen, None);
        assert_eq!(observer.last_seen_crouched, None);
        assert_eq!(observer.last_observed_aim_point(), None);
        observe(&mut observer, &[target.clone()], &[], Vec3::ZERO, 0.1);
        assert_eq!(observer.last_seen, Some(target.pos));
        assert_eq!(observer.last_seen_crouched, Some(false));
        assert!((observer.last_observed_aim_point().unwrap().y - target.pos.y - 1.15).abs() < 1e-6);
    }

    #[test]
    fn visible_stance_changes_update_only_observed_vertical_aim() {
        let mut observer = test_actor(1, Team::German, Vec3::ZERO, 0.0);
        let mut target = test_actor(2, Team::Soviet, Vec3::new(0.0, 0.0, -18.0), 0.0);
        let standing_obs = observe(&mut observer, &[target.clone()], &[], Vec3::ZERO, 0.1);
        let standing_aim = observer.last_observed_aim_point().unwrap();
        target.crouched = true;
        let crouched_obs = observe(&mut observer, &[target.clone()], &[], Vec3::ZERO, 0.1);
        assert_eq!(
            standing_obs.values(),
            crouched_obs.values(),
            "learned horizontal policy schema remains unchanged"
        );
        let crouched_aim = observer.last_observed_aim_point().unwrap();
        assert_eq!(standing_aim.x, crouched_aim.x);
        assert_eq!(standing_aim.z, crouched_aim.z);
        assert!(crouched_aim.y < 1.1 && crouched_aim.y < standing_aim.y);
    }

    #[test]
    fn elevation_fix_does_not_snap_learned_azimuth_to_target() {
        let mut actor = test_actor(1, Team::German, Vec3::ZERO, 0.25);
        actor.cooldown = 0.0;
        let target = test_actor(2, Team::Soviet, Vec3::new(0.0, 2.0, -18.0), 0.0);
        let obs = observe(&mut actor, &[target], &[], Vec3::ZERO, 1.0 / 120.0);
        let action = FootAction {
            turn: 0.6,
            ..fire_action(false)
        };
        let expected_yaw = actor.yaw + action.turn * 3.2 / 120.0;
        let shot = apply_action(&mut actor, &obs, action, &[], 1.0 / 120.0, 7).unwrap();
        let azimuth = shot.direction.x.atan2(-shot.direction.z);
        assert!((actor.yaw - expected_yaw).abs() < 1e-6);
        assert!(
            (azimuth - expected_yaw).abs() < 0.012,
            "only the existing spread perturbs learned yaw"
        );
        assert!(
            azimuth.abs() > 0.2,
            "a misaligned policy must still miss rather than gain aim teleportation"
        );
    }
}
