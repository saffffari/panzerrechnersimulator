//! Deterministic commander doctrine for the six- and ten-tank T-34 swarm labs.
//!
//! This module deliberately owns no game, rendering, or policy-network state. The simulation feeds
//! it coarse battlefield signals and identity-bearing combat events; it returns stable per-tank
//! orders and causal training credit that can be tested without a GPU.

#![allow(dead_code)] // Foundation module; game integration lands in a later bounded change.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Commander decisions are sampled at a fixed cadence so caller frame rate cannot change doctrine.
pub const COMMANDER_TICK_S: f32 = 0.5;
/// A phase must remain active this long before another phase transition is legal.
pub const MIN_PHASE_DWELL_S: f32 = 2.0;
/// Larger deltas are rejected without mutating state. This bounds one update to four commander ticks.
pub const MAX_UPDATE_DT_S: f32 = 2.0;
/// A shot directed at the exposed Fix tank must precede its loss closely enough to be causal.
pub const TARGET_SHOT_TO_LOSS_S: f32 = 3.0;
/// A flank payoff must follow the Fix tank's loss inside this window to earn sacrifice credit.
pub const SACRIFICE_PAYOFF_WINDOW_S: f32 = 10.0;
/// Training credit for one causally useful sacrifice. Raw tank losses are worth zero.
pub const USEFUL_SACRIFICE_POINTS: u32 = 120;

const MIN_PHASE_DWELL_TICKS: u32 = 4;
const TIME_EPSILON: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectileId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EngagementId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwarmRole {
    /// Draw and hold the target's attention while the flanks form.
    Fix,
    /// Lay the screen after the target commits to the Fix tank.
    Smoke,
    LeftFlank,
    RightFlank,
    /// Preserve depth and reinforce whichever arm stalls.
    Support,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwarmPhase {
    Form,
    Fix,
    Mask,
    Assault,
    Exploit,
}

/// Stable commander intent for one tank. Movement waypoints remain a game-layer concern; these
/// flags define which tactical job the executor is permitted to perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwarmOrder {
    pub tank_index: usize,
    pub role: SwarmRole,
    pub phase: SwarmPhase,
    pub active: bool,
    pub exposed: bool,
    pub may_fire: bool,
    pub may_smoke: bool,
    pub sacrifice_allowed: bool,
    /// Changes only on a phase transition or a loss-driven reassignment.
    pub revision: u64,
}

/// Commander inputs for this caller update.
///
/// `formation_ready`, `flankers_ready`, and `smoke_effective` are volatile current state. They are
/// sampled only at commander ticks and are never latched. The other fields are event pulses: they
/// wait for the next commander tick, are accepted only by their owning phase, and are cleared on a
/// phase transition so an early event cannot advance a later phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoctrineSignals {
    pub formation_ready: bool,
    pub flankers_ready: bool,
    pub target_committed_to_fix: bool,
    pub smoke_effective: bool,
    pub flank_penetration: bool,
    pub target_disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlankPayoff {
    SidePenetration,
    RearPenetration,
    TargetKill,
}

/// The opponent's shot at one swarm tank. `event_id` and `projectile_id` must each be globally
/// stable for the engagement; both identities are required again if this projectile causes a loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetShotEvent {
    pub event_id: EventId,
    pub projectile_id: ProjectileId,
    pub engagement_id: EngagementId,
    pub target_id: TargetId,
    pub aimed_at_tank_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotCausality {
    pub shot_event_id: EventId,
    pub projectile_id: ProjectileId,
}

/// An authoritative tank loss. Mines, collisions, fire, and otherwise unlinked losses use `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TankLossEvent {
    pub event_id: EventId,
    pub tank_index: usize,
    pub engagement_id: EngagementId,
    pub caused_by: Option<ShotCausality>,
}

/// A qualifying flank result must name the exact Fix loss it exploited. Event identity is globally
/// deduplicated, and target/engagement identity must match that loss's originating target shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlankPayoffEvent {
    pub event_id: EventId,
    pub shooter_tank_index: usize,
    pub engagement_id: EngagementId,
    pub target_id: TargetId,
    pub linked_loss_event_id: EventId,
    pub payoff: FlankPayoff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsefulSacrifice {
    pub tank_index: usize,
    pub target_shot_event_id: EventId,
    pub projectile_id: ProjectileId,
    pub loss_event_id: EventId,
    pub payoff_event_id: EventId,
    pub payoff_shooter_tank_index: usize,
    pub engagement_id: EngagementId,
    pub target_id: TargetId,
    pub loss_time_s: f32,
    pub payoff_time_s: f32,
    pub payoff: FlankPayoff,
    pub points: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    NoTransition,
    Transitioned,
    RejectedDeltaTime,
}

impl UpdateOutcome {
    pub fn transitioned(self) -> bool {
        self == Self::Transitioned
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordRejection {
    DuplicateEventId,
    DuplicateProjectileId,
    InvalidTankIndex,
    TankNotOperational,
    NotExposedFix,
    InvalidPayoffShooter,
    IdentityMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetShotOutcome {
    Rejected(RecordRejection),
    Recorded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TankLossOutcome {
    Rejected(RecordRejection),
    RecordedUnlinked,
    RecordedCausal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlankPayoffOutcome {
    Rejected(RecordRejection),
    RecordedUnlinked,
    Credited(UsefulSacrifice),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctrineError {
    UnsupportedTankCount(usize),
}

impl fmt::Display for DoctrineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTankCount(count) => {
                write!(
                    f,
                    "swarm doctrine supports exactly 6 or 10 tanks, got {count}"
                )
            }
        }
    }
}

impl std::error::Error for DoctrineError {}

pub const SIX_TANK_ROLES: [SwarmRole; 6] = [
    SwarmRole::Fix,
    SwarmRole::Smoke,
    SwarmRole::LeftFlank,
    SwarmRole::LeftFlank,
    SwarmRole::RightFlank,
    SwarmRole::RightFlank,
];

pub const TEN_TANK_ROLES: [SwarmRole; 10] = [
    SwarmRole::Fix,
    SwarmRole::Fix,
    SwarmRole::Smoke,
    SwarmRole::Smoke,
    SwarmRole::LeftFlank,
    SwarmRole::LeftFlank,
    SwarmRole::RightFlank,
    SwarmRole::RightFlank,
    SwarmRole::Support,
    SwarmRole::Support,
];

/// Exact, index-stable role assignment used by training and evaluation.
pub fn roles_for(tank_count: usize) -> Result<Vec<SwarmRole>, DoctrineError> {
    match tank_count {
        6 => Ok(SIX_TANK_ROLES.to_vec()),
        10 => Ok(TEN_TANK_ROLES.to_vec()),
        other => Err(DoctrineError::UnsupportedTankCount(other)),
    }
}

#[derive(Clone, Copy, Debug)]
struct TargetedFix {
    shot_event_id: EventId,
    projectile_id: ProjectileId,
    tank_index: usize,
    engagement_id: EngagementId,
    target_id: TargetId,
    shot_time_s: f64,
}

#[derive(Clone, Copy, Debug)]
struct PendingSacrifice {
    tank_index: usize,
    target_shot_event_id: EventId,
    projectile_id: ProjectileId,
    loss_event_id: EventId,
    engagement_id: EngagementId,
    target_id: TargetId,
    loss_time_s: f64,
    deadline_s: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct CurrentSignals {
    formation_ready: bool,
    flankers_ready: bool,
    smoke_effective: bool,
}

impl From<DoctrineSignals> for CurrentSignals {
    fn from(signals: DoctrineSignals) -> Self {
        Self {
            formation_ready: signals.formation_ready,
            flankers_ready: signals.flankers_ready,
            smoke_effective: signals.smoke_effective,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CommanderEvents {
    target_committed_to_fix: bool,
    assault_succeeded: bool,
}

impl CommanderEvents {
    fn absorb(&mut self, signals: DoctrineSignals) {
        self.target_committed_to_fix |= signals.target_committed_to_fix;
        self.assault_succeeded |= signals.flank_penetration || signals.target_disabled;
    }
}

/// Pure commander state. All time is simulation time supplied to [`DoctrineState::update`].
#[derive(Clone, Debug)]
pub struct DoctrineState {
    roles: Vec<SwarmRole>,
    alive: Vec<bool>,
    phase: SwarmPhase,
    phase_ticks: u32,
    now_s: f64,
    commander_accumulator_s: f64,
    order_revision: u64,
    active_fix: Option<usize>,
    orders: Vec<SwarmOrder>,
    pending_commander_events: CommanderEvents,
    phase_events: CommanderEvents,
    seen_event_ids: BTreeSet<EventId>,
    seen_projectile_ids: BTreeSet<ProjectileId>,
    targeted_fixes: BTreeMap<ProjectileId, TargetedFix>,
    pending_sacrifices: Vec<PendingSacrifice>,
    useful_sacrifices: Vec<UsefulSacrifice>,
}

impl DoctrineState {
    pub fn new(tank_count: usize) -> Result<Self, DoctrineError> {
        let roles = roles_for(tank_count)?;
        let alive = vec![true; roles.len()];
        let active_fix = roles.iter().position(|role| *role == SwarmRole::Fix);
        let mut state = Self {
            roles,
            alive,
            phase: SwarmPhase::Form,
            phase_ticks: 0,
            now_s: 0.0,
            commander_accumulator_s: 0.0,
            order_revision: 0,
            active_fix,
            orders: Vec::new(),
            pending_commander_events: CommanderEvents::default(),
            phase_events: CommanderEvents::default(),
            seen_event_ids: BTreeSet::new(),
            seen_projectile_ids: BTreeSet::new(),
            targeted_fixes: BTreeMap::new(),
            pending_sacrifices: Vec::new(),
            useful_sacrifices: Vec::new(),
        };
        state.rebuild_orders();
        Ok(state)
    }

    pub fn phase(&self) -> SwarmPhase {
        self.phase
    }

    pub fn phase_age_s(&self) -> f32 {
        self.phase_ticks as f32 * COMMANDER_TICK_S
    }

    pub fn now_s(&self) -> f32 {
        self.now_s as f32
    }

    pub fn roles(&self) -> &[SwarmRole] {
        &self.roles
    }

    pub fn orders(&self) -> &[SwarmOrder] {
        &self.orders
    }

    pub fn order_for(&self, tank_index: usize) -> Option<&SwarmOrder> {
        self.orders.get(tank_index)
    }

    pub fn active_fix(&self) -> Option<usize> {
        self.active_fix
    }

    pub fn exposed_fix(&self) -> Option<usize> {
        self.orders
            .iter()
            .find(|order| order.active && order.exposed)
            .map(|order| order.tank_index)
    }

    pub fn order_revision(&self) -> u64 {
        self.order_revision
    }

    pub fn useful_sacrifices(&self) -> &[UsefulSacrifice] {
        &self.useful_sacrifices
    }

    pub fn useful_sacrifice_score(&self) -> u32 {
        (self.useful_sacrifices.len() as u32).saturating_mul(USEFUL_SACRIFICE_POINTS)
    }

    pub fn pending_sacrifice_count(&self) -> usize {
        self.pending_sacrifices.len()
    }

    /// Advance simulation time and sample commander decisions on fixed 0.5-second boundaries.
    /// Invalid, negative, or larger-than-two-second deltas are rejected without any state change.
    pub fn update(&mut self, dt: f32, signals: DoctrineSignals) -> UpdateOutcome {
        if !dt.is_finite() || !(0.0..=MAX_UPDATE_DT_S).contains(&dt) {
            return UpdateOutcome::RejectedDeltaTime;
        }

        self.pending_commander_events.absorb(signals);
        if dt == 0.0 {
            return UpdateOutcome::NoTransition;
        }

        let sampled_phase = self.phase;
        let current = CurrentSignals::from(signals);
        let dt = f64::from(dt);
        self.now_s += dt;
        self.expire_causal_windows();

        let tick_s = f64::from(COMMANDER_TICK_S);
        let total = self.commander_accumulator_s + dt;
        let tick_count = ((total + TIME_EPSILON) / tick_s).floor() as u32;
        debug_assert!(
            tick_count <= 4,
            "bounded update exceeded four commander ticks"
        );
        self.commander_accumulator_s = (total - f64::from(tick_count) * tick_s).max(0.0);

        let mut transitioned = false;
        for _ in 0..tick_count {
            let events = std::mem::take(&mut self.pending_commander_events);
            self.absorb_phase_events(events);
            self.phase_ticks = self.phase_ticks.saturating_add(1);

            // Current-state values belong to the phase in which the caller sampled them. If this
            // update crosses a transition, later ticks must wait for a fresh caller sample.
            let sampled_current = if self.phase == sampled_phase {
                current
            } else {
                CurrentSignals::default()
            };
            transitioned |= self.try_advance_phase(sampled_current);
        }

        if transitioned {
            UpdateOutcome::Transitioned
        } else {
            UpdateOutcome::NoTransition
        }
    }

    /// Record the target firing specifically at a tank. Only the one living, currently exposed Fix
    /// tank can establish causal sacrifice evidence. Event and projectile IDs are each single-use.
    pub fn record_target_shot(&mut self, event: TargetShotEvent) -> TargetShotOutcome {
        if self.seen_event_ids.contains(&event.event_id) {
            return TargetShotOutcome::Rejected(RecordRejection::DuplicateEventId);
        }
        if self.seen_projectile_ids.contains(&event.projectile_id) {
            return TargetShotOutcome::Rejected(RecordRejection::DuplicateProjectileId);
        }
        let Some(order) = self.order_for(event.aimed_at_tank_index) else {
            return TargetShotOutcome::Rejected(RecordRejection::InvalidTankIndex);
        };
        if !order.active {
            return TargetShotOutcome::Rejected(RecordRejection::TankNotOperational);
        }
        if !order.exposed || !order.sacrifice_allowed || order.role != SwarmRole::Fix {
            return TargetShotOutcome::Rejected(RecordRejection::NotExposedFix);
        }

        // Commit only after the complete event and projectile classification succeeds.
        self.seen_event_ids.insert(event.event_id);
        self.seen_projectile_ids.insert(event.projectile_id);
        self.targeted_fixes.insert(
            event.projectile_id,
            TargetedFix {
                shot_event_id: event.event_id,
                projectile_id: event.projectile_id,
                tank_index: event.aimed_at_tank_index,
                engagement_id: event.engagement_id,
                target_id: event.target_id,
                shot_time_s: self.now_s,
            },
        );
        self.pending_commander_events.target_committed_to_fix = true;
        self.expire_causal_windows();
        TargetShotOutcome::Recorded
    }

    /// Mark one tank lost. A valid loss always changes the force state and is deduplicated; only a
    /// loss naming the exact tracked shot and projectile returns `RecordedCausal` and opens credit.
    pub fn record_tank_loss(&mut self, event: TankLossEvent) -> TankLossOutcome {
        if self.seen_event_ids.contains(&event.event_id) {
            return TankLossOutcome::Rejected(RecordRejection::DuplicateEventId);
        }
        let Some(alive) = self.alive.get(event.tank_index).copied() else {
            return TankLossOutcome::Rejected(RecordRejection::InvalidTankIndex);
        };
        if !alive {
            return TankLossOutcome::Rejected(RecordRejection::TankNotOperational);
        }

        let causal_shot = event.caused_by.and_then(|cause| {
            self.targeted_fixes
                .get(&cause.projectile_id)
                .copied()
                .filter(|shot| {
                    shot.projectile_id == cause.projectile_id
                        && shot.shot_event_id == cause.shot_event_id
                        && shot.tank_index == event.tank_index
                        && shot.engagement_id == event.engagement_id
                        && self.now_s >= shot.shot_time_s
                        && self.now_s - shot.shot_time_s
                            <= f64::from(TARGET_SHOT_TO_LOSS_S) + TIME_EPSILON
                })
        });

        // A valid authoritative loss is recorded even when its cause is a mine, collision, fire,
        // or an unrecognized projectile. Only its sacrifice classification differs.
        self.seen_event_ids.insert(event.event_id);
        if let Some(shot) = causal_shot {
            self.pending_sacrifices.push(PendingSacrifice {
                tank_index: event.tank_index,
                target_shot_event_id: shot.shot_event_id,
                projectile_id: shot.projectile_id,
                loss_event_id: event.event_id,
                engagement_id: shot.engagement_id,
                target_id: shot.target_id,
                loss_time_s: self.now_s,
                deadline_s: self.now_s + f64::from(SACRIFICE_PAYOFF_WINDOW_S),
            });
        }

        self.alive[event.tank_index] = false;
        self.targeted_fixes
            .retain(|_, shot| shot.tank_index != event.tank_index);
        if self.active_fix == Some(event.tank_index) {
            self.active_fix = self.next_living_fix();
        }
        self.order_revision = self.order_revision.wrapping_add(1);
        self.rebuild_orders();
        self.expire_causal_windows();
        if causal_shot.is_some() {
            TankLossOutcome::RecordedCausal
        } else {
            TankLossOutcome::RecordedUnlinked
        }
    }

    /// Record a side/rear penetration or target kill. The shooter must be a living Left/Right
    /// flanker currently permitted to fire, and the event must explicitly name the matching loss,
    /// engagement, and target. A valid unlinked payoff is still recorded as an Assault-success
    /// event, while malformed attribution is rejected without consuming its identity.
    pub fn record_flank_payoff(&mut self, event: FlankPayoffEvent) -> FlankPayoffOutcome {
        if self.seen_event_ids.contains(&event.event_id) {
            return FlankPayoffOutcome::Rejected(RecordRejection::DuplicateEventId);
        }
        let Some(order) = self.order_for(event.shooter_tank_index) else {
            return FlankPayoffOutcome::Rejected(RecordRejection::InvalidTankIndex);
        };
        if !order.active {
            return FlankPayoffOutcome::Rejected(RecordRejection::TankNotOperational);
        }
        if !order.may_fire || !matches!(order.role, SwarmRole::LeftFlank | SwarmRole::RightFlank) {
            return FlankPayoffOutcome::Rejected(RecordRejection::InvalidPayoffShooter);
        }

        // Locate the named, still-eligible loss by stable identity. If it exists, conflicting
        // engagement/target attribution is malformed and rejected without consuming the event ID.
        let pending_index = self.pending_sacrifices.iter().position(|pending| {
            pending.loss_event_id == event.linked_loss_event_id
                && pending.loss_time_s <= self.now_s + TIME_EPSILON
                && self.now_s <= pending.deadline_s + TIME_EPSILON
        });
        if let Some(index) = pending_index {
            let pending = self.pending_sacrifices[index];
            if pending.engagement_id != event.engagement_id || pending.target_id != event.target_id
            {
                return FlankPayoffOutcome::Rejected(RecordRejection::IdentityMismatch);
            }
        }

        // A valid flank event remains authoritative without a sacrifice link: dedupe it and let it
        // advance Assault, but award no sacrifice points.
        self.seen_event_ids.insert(event.event_id);
        self.pending_commander_events.assault_succeeded = true;
        let Some(pending_index) = pending_index else {
            self.expire_causal_windows();
            return FlankPayoffOutcome::RecordedUnlinked;
        };

        let pending = self.pending_sacrifices.remove(pending_index);
        let credit = UsefulSacrifice {
            tank_index: pending.tank_index,
            target_shot_event_id: pending.target_shot_event_id,
            projectile_id: pending.projectile_id,
            loss_event_id: pending.loss_event_id,
            payoff_event_id: event.event_id,
            payoff_shooter_tank_index: event.shooter_tank_index,
            engagement_id: pending.engagement_id,
            target_id: pending.target_id,
            loss_time_s: pending.loss_time_s as f32,
            payoff_time_s: self.now_s as f32,
            payoff: event.payoff,
            points: USEFUL_SACRIFICE_POINTS,
        };
        self.useful_sacrifices.push(credit);
        self.expire_causal_windows();
        FlankPayoffOutcome::Credited(credit)
    }

    fn absorb_phase_events(&mut self, events: CommanderEvents) {
        match self.phase {
            SwarmPhase::Fix => {
                self.phase_events.target_committed_to_fix |= events.target_committed_to_fix;
            }
            SwarmPhase::Assault => {
                self.phase_events.assault_succeeded |= events.assault_succeeded;
            }
            _ => {}
        }
    }

    fn try_advance_phase(&mut self, current: CurrentSignals) -> bool {
        if self.phase_ticks < MIN_PHASE_DWELL_TICKS {
            return false;
        }
        let next = match self.phase {
            SwarmPhase::Form if current.formation_ready => Some(SwarmPhase::Fix),
            SwarmPhase::Fix
                if self.phase_events.target_committed_to_fix && current.flankers_ready =>
            {
                Some(SwarmPhase::Mask)
            }
            SwarmPhase::Mask if current.smoke_effective => Some(SwarmPhase::Assault),
            SwarmPhase::Assault if self.phase_events.assault_succeeded => Some(SwarmPhase::Exploit),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };

        self.phase = next;
        self.phase_ticks = 0;
        self.phase_events = CommanderEvents::default();
        self.pending_commander_events = CommanderEvents::default();
        self.order_revision = self.order_revision.wrapping_add(1);
        self.rebuild_orders();
        true
    }

    fn next_living_fix(&self) -> Option<usize> {
        self.roles
            .iter()
            .enumerate()
            .find(|(index, role)| **role == SwarmRole::Fix && self.alive[*index])
            .map(|(index, _)| index)
    }

    fn expire_causal_windows(&mut self) {
        self.targeted_fixes.retain(|_, shot| {
            self.now_s - shot.shot_time_s <= f64::from(TARGET_SHOT_TO_LOSS_S) + TIME_EPSILON
        });
        self.pending_sacrifices
            .retain(|pending| self.now_s <= pending.deadline_s + TIME_EPSILON);
    }

    fn rebuild_orders(&mut self) {
        let phase = self.phase;
        let revision = self.order_revision;
        let active_fix = self.active_fix;
        self.orders = self
            .roles
            .iter()
            .copied()
            .enumerate()
            .map(|(tank_index, role)| {
                let active = self.alive[tank_index];
                let exposed = active
                    && active_fix == Some(tank_index)
                    && matches!(phase, SwarmPhase::Fix | SwarmPhase::Mask);
                let may_fire = active
                    && (exposed || matches!(phase, SwarmPhase::Assault | SwarmPhase::Exploit));
                let may_smoke = active && role == SwarmRole::Smoke && phase == SwarmPhase::Mask;
                SwarmOrder {
                    tank_index,
                    role,
                    phase,
                    active,
                    exposed,
                    may_fire,
                    may_smoke,
                    sacrifice_allowed: exposed,
                    revision,
                }
            })
            .collect();

        debug_assert!(
            self.orders.iter().filter(|order| order.exposed).count() <= 1,
            "doctrine may expose at most one Fix tank"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGAGEMENT: EngagementId = EngagementId(7);
    const TARGET: TargetId = TargetId(99);

    fn shot(event: u64, projectile: u64, tank_index: usize) -> TargetShotEvent {
        TargetShotEvent {
            event_id: EventId(event),
            projectile_id: ProjectileId(projectile),
            engagement_id: ENGAGEMENT,
            target_id: TARGET,
            aimed_at_tank_index: tank_index,
        }
    }

    fn loss(
        event: u64,
        tank_index: usize,
        shot_event: Option<u64>,
        projectile: Option<u64>,
    ) -> TankLossEvent {
        TankLossEvent {
            event_id: EventId(event),
            tank_index,
            engagement_id: ENGAGEMENT,
            caused_by: shot_event
                .zip(projectile)
                .map(|(shot_event, projectile)| ShotCausality {
                    shot_event_id: EventId(shot_event),
                    projectile_id: ProjectileId(projectile),
                }),
        }
    }

    fn payoff(event: u64, shooter: usize, linked_loss: u64, kind: FlankPayoff) -> FlankPayoffEvent {
        FlankPayoffEvent {
            event_id: EventId(event),
            shooter_tank_index: shooter,
            engagement_id: ENGAGEMENT,
            target_id: TARGET,
            linked_loss_event_id: EventId(linked_loss),
            payoff: kind,
        }
    }

    fn enter_fix(state: &mut DoctrineState) {
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S,
                DoctrineSignals {
                    formation_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Fix);
    }

    fn enter_assault_after_commitment(state: &mut DoctrineState) {
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S,
                DoctrineSignals {
                    flankers_ready: true,
                    target_committed_to_fix: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Mask);
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S,
                DoctrineSignals {
                    smoke_effective: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Assault);
    }

    fn advance_time(state: &mut DoctrineState, mut seconds: f32) {
        while seconds > 0.0 {
            let dt = seconds.min(MAX_UPDATE_DT_S);
            assert_ne!(
                state.update(dt, DoctrineSignals::default()),
                UpdateOutcome::RejectedDeltaTime
            );
            seconds -= dt;
        }
    }

    #[test]
    fn exact_six_and_ten_assignments_are_deterministic() {
        assert_eq!(roles_for(6).unwrap(), SIX_TANK_ROLES);
        assert_eq!(roles_for(10).unwrap(), TEN_TANK_ROLES);
        assert_eq!(roles_for(8), Err(DoctrineError::UnsupportedTankCount(8)));

        let six = DoctrineState::new(6).unwrap();
        assert_eq!(six.roles(), &SIX_TANK_ROLES);
        let ten = DoctrineState::new(10).unwrap();
        assert_eq!(ten.roles(), &TEN_TANK_ROLES);
    }

    #[test]
    fn phases_and_orders_stay_sticky_until_commander_dwell_expires() {
        let mut state = DoctrineState::new(6).unwrap();
        let original_orders = state.orders().to_vec();
        let original_revision = state.order_revision();
        let ready = DoctrineSignals {
            formation_ready: true,
            ..DoctrineSignals::default()
        };

        assert_eq!(
            state.update(MIN_PHASE_DWELL_S - 0.01, ready),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Form);
        assert_eq!(state.order_revision(), original_revision);
        assert_eq!(state.orders(), original_orders);

        assert_eq!(state.update(0.01, ready), UpdateOutcome::Transitioned);
        assert_eq!(state.phase(), SwarmPhase::Fix);
        assert_eq!(state.order_revision(), original_revision + 1);
        let fix_orders = state.orders().to_vec();

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    target_committed_to_fix: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S - COMMANDER_TICK_S,
                DoctrineSignals {
                    flankers_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_ne!(state.orders(), fix_orders);
        assert_eq!(state.phase(), SwarmPhase::Mask);
    }

    #[test]
    fn only_one_fix_is_exposed_and_the_second_replaces_a_loss() {
        let mut ten = DoctrineState::new(10).unwrap();
        enter_fix(&mut ten);
        assert_eq!(ten.exposed_fix(), Some(0));
        assert_eq!(ten.orders().iter().filter(|order| order.exposed).count(), 1);

        assert_eq!(
            ten.record_tank_loss(loss(10, 0, None, None)),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(ten.active_fix(), Some(1));
        assert_eq!(ten.exposed_fix(), Some(1));
        assert_eq!(ten.orders().iter().filter(|order| order.exposed).count(), 1);

        let mut six = DoctrineState::new(6).unwrap();
        enter_fix(&mut six);
        assert_eq!(six.exposed_fix(), Some(0));
        assert_eq!(
            six.record_tank_loss(loss(11, 0, None, None)),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(six.active_fix(), None);
        assert_eq!(six.exposed_fix(), None);
    }

    #[test]
    fn volatile_readiness_and_smoke_are_sampled_not_latched() {
        let mut state = DoctrineState::new(10).unwrap();
        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    formation_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S - COMMANDER_TICK_S,
                DoctrineSignals::default()
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Form);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    formation_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Fix);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    flankers_ready: true,
                    target_committed_to_fix: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S - COMMANDER_TICK_S,
                DoctrineSignals::default()
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Fix);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    flankers_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Mask);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    smoke_effective: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S - COMMANDER_TICK_S,
                DoctrineSignals::default()
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Mask);
    }

    #[test]
    fn pre_mask_smoke_and_pre_assault_penetration_do_not_leak_forward() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    smoke_effective: true,
                    target_committed_to_fix: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::NoTransition
        );
        assert_eq!(
            state.update(
                MIN_PHASE_DWELL_S - COMMANDER_TICK_S,
                DoctrineSignals {
                    flankers_ready: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Mask);

        assert_eq!(
            state.update(MIN_PHASE_DWELL_S, DoctrineSignals::default()),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Mask);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    smoke_effective: true,
                    flank_penetration: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Assault);
        assert_eq!(
            state.update(MIN_PHASE_DWELL_S, DoctrineSignals::default()),
            UpdateOutcome::NoTransition
        );
        assert_eq!(state.phase(), SwarmPhase::Assault);

        assert_eq!(
            state.update(
                COMMANDER_TICK_S,
                DoctrineSignals {
                    flank_penetration: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::Transitioned
        );
        assert_eq!(state.phase(), SwarmPhase::Exploit);
    }

    #[test]
    fn identity_linked_sacrifice_scores_after_a_valid_flanker_payoff() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(100, 1000, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(200, 0, Some(100), Some(1000))),
            TankLossOutcome::RecordedCausal
        );
        assert_eq!(state.pending_sacrifice_count(), 1);
        assert_eq!(state.useful_sacrifice_score(), 0);

        enter_assault_after_commitment(&mut state);
        let FlankPayoffOutcome::Credited(credit) =
            state.record_flank_payoff(payoff(300, 4, 200, FlankPayoff::SidePenetration))
        else {
            panic!("identity-linked side penetration should credit the Fix loss");
        };
        assert_eq!(credit.tank_index, 0);
        assert_eq!(credit.target_shot_event_id, EventId(100));
        assert_eq!(credit.projectile_id, ProjectileId(1000));
        assert_eq!(credit.loss_event_id, EventId(200));
        assert_eq!(credit.payoff_event_id, EventId(300));
        assert_eq!(credit.payoff_shooter_tank_index, 4);
        assert_eq!(state.useful_sacrifice_score(), USEFUL_SACRIFICE_POINTS);
    }

    #[test]
    fn mine_or_unlinked_loss_after_a_targeted_shot_scores_zero() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(101, 1001, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(201, 0, None, None)),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(state.pending_sacrifice_count(), 0);
        enter_assault_after_commitment(&mut state);
        assert_eq!(
            state.record_flank_payoff(payoff(301, 4, 201, FlankPayoff::RearPenetration)),
            FlankPayoffOutcome::RecordedUnlinked
        );
        assert_eq!(state.useful_sacrifice_score(), 0);

        let mut mismatched = DoctrineState::new(10).unwrap();
        enter_fix(&mut mismatched);
        assert_eq!(
            mismatched.record_target_shot(shot(111, 1011, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            mismatched.record_tank_loss(loss(211, 0, Some(111), Some(9_999))),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(mismatched.pending_sacrifice_count(), 0);
        enter_assault_after_commitment(&mut mismatched);
        assert_eq!(
            mismatched.record_flank_payoff(payoff(311, 4, 211, FlankPayoff::SidePenetration)),
            FlankPayoffOutcome::RecordedUnlinked
        );
        assert_eq!(mismatched.useful_sacrifice_score(), 0);
    }

    #[test]
    fn fix_smoke_and_dead_flanker_payoffs_score_zero() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(102, 1002, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(202, 0, Some(102), Some(1002))),
            TankLossOutcome::RecordedCausal
        );
        enter_assault_after_commitment(&mut state);

        assert_eq!(
            state.record_flank_payoff(payoff(302, 1, 202, FlankPayoff::SidePenetration)),
            FlankPayoffOutcome::Rejected(RecordRejection::InvalidPayoffShooter)
        );
        assert_eq!(
            state.record_flank_payoff(payoff(303, 2, 202, FlankPayoff::RearPenetration)),
            FlankPayoffOutcome::Rejected(RecordRejection::InvalidPayoffShooter)
        );
        assert_eq!(
            state.record_tank_loss(loss(203, 4, None, None)),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(
            state.record_flank_payoff(payoff(304, 4, 202, FlankPayoff::TargetKill)),
            FlankPayoffOutcome::Rejected(RecordRejection::TankNotOperational)
        );
        assert_eq!(state.pending_sacrifice_count(), 1);
        assert_eq!(state.useful_sacrifice_score(), 0);
    }

    #[test]
    fn duplicate_payoff_event_scores_once() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(103, 1003, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(204, 0, Some(103), Some(1003))),
            TankLossOutcome::RecordedCausal
        );
        enter_assault_after_commitment(&mut state);

        let event = payoff(305, 4, 204, FlankPayoff::RearPenetration);
        assert!(matches!(
            state.record_flank_payoff(event),
            FlankPayoffOutcome::Credited(_)
        ));
        assert_eq!(
            state.record_flank_payoff(event),
            FlankPayoffOutcome::Rejected(RecordRejection::DuplicateEventId)
        );
        assert_eq!(state.useful_sacrifice_score(), USEFUL_SACRIFICE_POINTS);
        assert_eq!(state.useful_sacrifices().len(), 1);
    }

    #[test]
    fn rejected_shot_keeps_event_and_projectile_ids_reusable() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);

        let mut corrected = shot(400, 4_000, 1); // the reserve Fix is not exposed
        assert_eq!(
            state.record_target_shot(corrected),
            TargetShotOutcome::Rejected(RecordRejection::NotExposedFix)
        );
        corrected.aimed_at_tank_index = 0;
        assert_eq!(
            state.record_target_shot(corrected),
            TargetShotOutcome::Recorded
        );

        let duplicate_projectile = shot(401, 4_000, 0);
        assert_eq!(
            state.record_target_shot(duplicate_projectile),
            TargetShotOutcome::Rejected(RecordRejection::DuplicateProjectileId)
        );
        let corrected_projectile = shot(401, 4_001, 0);
        assert_eq!(
            state.record_target_shot(corrected_projectile),
            TargetShotOutcome::Recorded
        );
    }

    #[test]
    fn invalid_loss_and_shooter_keep_ids_reusable_then_unlinked_events_dedupe() {
        let mut loss_state = DoctrineState::new(10).unwrap();
        enter_fix(&mut loss_state);
        let mut corrected_loss = loss(410, 99, None, None);
        assert_eq!(
            loss_state.record_tank_loss(corrected_loss),
            TankLossOutcome::Rejected(RecordRejection::InvalidTankIndex)
        );
        corrected_loss.tank_index = 0;
        assert_eq!(
            loss_state.record_tank_loss(corrected_loss),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(
            loss_state.record_tank_loss(corrected_loss),
            TankLossOutcome::Rejected(RecordRejection::DuplicateEventId)
        );

        let mut payoff_state = DoctrineState::new(10).unwrap();
        enter_fix(&mut payoff_state);
        enter_assault_after_commitment(&mut payoff_state);
        let mut corrected_payoff = payoff(420, 0, 9_999, FlankPayoff::SidePenetration);
        assert_eq!(
            payoff_state.record_flank_payoff(corrected_payoff),
            FlankPayoffOutcome::Rejected(RecordRejection::InvalidPayoffShooter)
        );
        corrected_payoff.shooter_tank_index = 4;
        assert_eq!(
            payoff_state.record_flank_payoff(corrected_payoff),
            FlankPayoffOutcome::RecordedUnlinked
        );
        assert_eq!(
            payoff_state.record_flank_payoff(corrected_payoff),
            FlankPayoffOutcome::Rejected(RecordRejection::DuplicateEventId)
        );
        assert_eq!(payoff_state.useful_sacrifice_score(), 0);
    }

    #[test]
    fn two_candidates_credit_the_explicitly_linked_loss_not_the_oldest() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(104, 1004, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(205, 0, Some(104), Some(1004))),
            TankLossOutcome::RecordedCausal
        );
        assert_eq!(
            state.record_target_shot(shot(105, 1005, 1)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(206, 1, Some(105), Some(1005))),
            TankLossOutcome::RecordedCausal
        );
        assert_eq!(state.pending_sacrifice_count(), 2);
        enter_assault_after_commitment(&mut state);

        let FlankPayoffOutcome::Credited(credit) =
            state.record_flank_payoff(payoff(306, 4, 206, FlankPayoff::TargetKill))
        else {
            panic!("payoff must select its explicit linked loss");
        };
        assert_eq!(credit.tank_index, 1);
        assert_eq!(credit.loss_event_id, EventId(206));
        assert_eq!(state.pending_sacrifice_count(), 1);

        let FlankPayoffOutcome::Credited(older) =
            state.record_flank_payoff(payoff(307, 5, 205, FlankPayoff::SidePenetration))
        else {
            panic!("the older candidate remains independently creditable");
        };
        assert_eq!(older.tank_index, 0);
        assert_eq!(state.useful_sacrifice_score(), 2 * USEFUL_SACRIFICE_POINTS);
    }

    #[test]
    fn mismatched_target_or_engagement_cannot_claim_a_pending_loss() {
        let mut state = DoctrineState::new(10).unwrap();
        enter_fix(&mut state);
        assert_eq!(
            state.record_target_shot(shot(106, 1006, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            state.record_tank_loss(loss(207, 0, Some(106), Some(1006))),
            TankLossOutcome::RecordedCausal
        );
        enter_assault_after_commitment(&mut state);

        let mut wrong_target = payoff(308, 4, 207, FlankPayoff::SidePenetration);
        wrong_target.target_id = TargetId(TARGET.0 + 1);
        assert_eq!(
            state.record_flank_payoff(wrong_target),
            FlankPayoffOutcome::Rejected(RecordRejection::IdentityMismatch)
        );
        let mut wrong_engagement = payoff(308, 4, 207, FlankPayoff::SidePenetration);
        wrong_engagement.engagement_id = EngagementId(ENGAGEMENT.0 + 1);
        assert_eq!(
            state.record_flank_payoff(wrong_engagement),
            FlankPayoffOutcome::Rejected(RecordRejection::IdentityMismatch)
        );
        assert_eq!(state.pending_sacrifice_count(), 1);
        assert_eq!(state.useful_sacrifice_score(), 0);

        assert!(matches!(
            state.record_flank_payoff(payoff(308, 4, 207, FlankPayoff::SidePenetration)),
            FlankPayoffOutcome::Credited(_)
        ));
    }

    #[test]
    fn stale_shot_and_late_payoff_never_score() {
        let mut stale = DoctrineState::new(10).unwrap();
        enter_fix(&mut stale);
        assert_eq!(
            stale.record_target_shot(shot(107, 1007, 0)),
            TargetShotOutcome::Recorded
        );
        advance_time(&mut stale, TARGET_SHOT_TO_LOSS_S + COMMANDER_TICK_S);
        assert_eq!(
            stale.record_tank_loss(loss(208, 0, Some(107), Some(1007))),
            TankLossOutcome::RecordedUnlinked
        );
        assert_eq!(stale.pending_sacrifice_count(), 0);

        let mut late = DoctrineState::new(10).unwrap();
        enter_fix(&mut late);
        assert_eq!(
            late.record_target_shot(shot(108, 1008, 0)),
            TargetShotOutcome::Recorded
        );
        assert_eq!(
            late.record_tank_loss(loss(209, 0, Some(108), Some(1008))),
            TankLossOutcome::RecordedCausal
        );
        enter_assault_after_commitment(&mut late);
        advance_time(
            &mut late,
            SACRIFICE_PAYOFF_WINDOW_S - 2.0 * MIN_PHASE_DWELL_S + COMMANDER_TICK_S,
        );
        assert_eq!(late.pending_sacrifice_count(), 0);
        assert_eq!(
            late.record_flank_payoff(payoff(310, 4, 209, FlankPayoff::TargetKill)),
            FlankPayoffOutcome::RecordedUnlinked
        );
        assert_eq!(late.useful_sacrifice_score(), 0);
    }

    #[test]
    fn partitioned_time_matches_one_bounded_update_and_oversized_dt_is_rejected() {
        let ready = DoctrineSignals {
            formation_ready: true,
            ..DoctrineSignals::default()
        };
        let mut whole = DoctrineState::new(10).unwrap();
        let mut partitioned = DoctrineState::new(10).unwrap();

        assert_eq!(
            whole.update(MIN_PHASE_DWELL_S, ready),
            UpdateOutcome::Transitioned
        );
        for i in 0..4 {
            let expected = if i == 3 {
                UpdateOutcome::Transitioned
            } else {
                UpdateOutcome::NoTransition
            };
            assert_eq!(partitioned.update(COMMANDER_TICK_S, ready), expected);
        }
        assert_eq!(whole.phase(), partitioned.phase());
        assert_eq!(whole.phase_age_s(), partitioned.phase_age_s());
        assert_eq!(whole.order_revision(), partitioned.order_revision());
        assert_eq!(whole.orders(), partitioned.orders());
        assert!((whole.now_s() - partitioned.now_s()).abs() < 1.0e-6);

        let phase = whole.phase();
        let now = whole.now_s();
        let revision = whole.order_revision();
        let orders = whole.orders().to_vec();
        assert_eq!(
            whole.update(
                f32::MAX,
                DoctrineSignals {
                    flankers_ready: true,
                    target_committed_to_fix: true,
                    smoke_effective: true,
                    flank_penetration: true,
                    ..DoctrineSignals::default()
                }
            ),
            UpdateOutcome::RejectedDeltaTime
        );
        assert_eq!(whole.phase(), phase);
        assert_eq!(whole.now_s(), now);
        assert_eq!(whole.order_revision(), revision);
        assert_eq!(whole.orders(), orders);
    }
}
