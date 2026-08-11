//! Renderer-independent main-gun reload state.
//!
//! This module owns the physical identity and location of every round and case handed to it.  The
//! fixed-capacity arrays make the state bounded and straightforward to encode in a future campaign
//! or network snapshot.  It intentionally contains no timings for hand motions, no transforms, and
//! no historical probability defaults: callers provide the reliability policy and advance the
//! mechanism only when their animation/gameplay layer says an action has completed.

#![allow(dead_code)] // Foundation module; live reload integration is a later bounded change.

use std::collections::BTreeSet;

/// Probability denominator used by [`IgnitionPolicy`].  `65_536` means certain; `0` means never.
pub const PROBABILITY_SCALE: u32 = 65_536;
pub const MAX_ROUND_CONDITION: u8 = 100;

/// Stable physical identity.  A fired round becomes a case with the same identity, so firing never
/// creates or destroys an inventory item inside this model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ItemId(pub u32);

/// Shell taxonomy needed by the reload system.  Ballistics remains a consumer concern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ShellNature {
    PzGr39 = 0,
    SprGr = 1,
    PzGr40 = 2,
    Practice = 3,
}

/// One unfired, fixed round.  Condition is an explicit 0..=100 input to the deterministic ignition
/// plan; the reload core never silently improves or degrades it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundItem {
    id: ItemId,
    nature: ShellNature,
    condition: u8,
}

impl RoundItem {
    pub fn new(id: ItemId, nature: ShellNature, condition: u8) -> Result<Self, ReloadError> {
        if condition > MAX_ROUND_CONDITION {
            return Err(ReloadError::InvalidRoundCondition(condition));
        }
        Ok(Self {
            id,
            nature,
            condition,
        })
    }

    pub fn id(self) -> ItemId {
        self.id
    }

    pub fn nature(self) -> ShellNature {
        self.nature
    }

    pub fn condition(self) -> u8 {
        self.condition
    }
}

/// The surviving cartridge case after a discharge.  `heat` is a normalized serialized quantity;
/// thermal integration may cool it later without changing its identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseItem {
    id: ItemId,
    source_nature: ShellNature,
    heat: u8,
}

impl CaseItem {
    pub fn new(id: ItemId, source_nature: ShellNature, heat: u8) -> Self {
        Self {
            id,
            source_nature,
            heat,
        }
    }

    fn fired(round: RoundItem) -> Self {
        Self::new(round.id, round.nature, u8::MAX)
    }

    pub fn id(self) -> ItemId {
        self.id
    }

    pub fn source_nature(self) -> ShellNature {
        self.source_nature
    }

    pub fn heat(self) -> u8 {
        self.heat
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandItem {
    Round(RoundItem),
    Case(CaseItem),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StowedItem {
    Round(RoundItem),
    Case(CaseItem),
}

impl HandItem {
    fn id(self) -> ItemId {
        match self {
            Self::Round(item) => item.id,
            Self::Case(item) => item.id,
        }
    }

    fn stowed(self) -> StowedItem {
        match self {
            Self::Round(item) => StowedItem::Round(item),
            Self::Case(item) => StowedItem::Case(item),
        }
    }
}

impl StowedItem {
    fn id(self) -> ItemId {
        match self {
            Self::Round(item) => item.id,
            Self::Case(item) => item.id,
        }
    }

    fn in_hand(self) -> HandItem {
        match self {
            Self::Round(item) => HandItem::Round(item),
            Self::Case(item) => HandItem::Case(item),
        }
    }
}

/// Quantized local-space placement for a loose round or case.  Interpretation of the Euler values is
/// deliberately left to the integration layer; the core only preserves a compact stable pose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackedPose {
    pub position_cm: [i16; 3],
    pub rotation_q: [i16; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LooseItem {
    pub item: StowedItem,
    pub pose: PackedPose,
}

/// Caller-supplied reliability tuning.  No values here are presented as final historical odds or
/// timings.  Failure probability is linearly interpolated by round condition; the one deterministic
/// plan generated when the round enters the chamber is retained until it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IgnitionPolicy {
    healthy_failure_per_65536: u32,
    ruined_failure_per_65536: u32,
    hangfire_share_per_65536: u32,
    hangfire_delay_ticks: u32,
}

impl IgnitionPolicy {
    pub fn new(
        healthy_failure_per_65536: u32,
        ruined_failure_per_65536: u32,
        hangfire_share_per_65536: u32,
        hangfire_delay_ticks: u32,
    ) -> Result<Self, ReloadError> {
        if healthy_failure_per_65536 > PROBABILITY_SCALE
            || ruined_failure_per_65536 > PROBABILITY_SCALE
            || hangfire_share_per_65536 > PROBABILITY_SCALE
            || healthy_failure_per_65536 > ruined_failure_per_65536
            || (hangfire_share_per_65536 > 0 && hangfire_delay_ticks == 0)
        {
            return Err(ReloadError::InvalidIgnitionPolicy);
        }
        Ok(Self {
            healthy_failure_per_65536,
            ruined_failure_per_65536,
            hangfire_share_per_65536,
            hangfire_delay_ticks,
        })
    }

    pub fn healthy_failure_per_65536(self) -> u32 {
        self.healthy_failure_per_65536
    }

    pub fn ruined_failure_per_65536(self) -> u32 {
        self.ruined_failure_per_65536
    }

    pub fn hangfire_share_per_65536(self) -> u32 {
        self.hangfire_share_per_65536
    }

    pub fn hangfire_delay_ticks(self) -> u32 {
        self.hangfire_delay_ticks
    }

    fn failure_threshold(self, condition: u8) -> u32 {
        let condition = u64::from(condition.min(MAX_ROUND_CONDITION));
        let ruined = u64::from(self.ruined_failure_per_65536);
        let healthy = u64::from(self.healthy_failure_per_65536);
        (ruined - (ruined - healthy) * condition / u64::from(MAX_ROUND_CONDITION)) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IgnitionDisposition {
    Immediate,
    Misfire,
    Hangfire { delay_ticks: u32 },
}

/// Breech motion is quantized rather than time-based.  Animation code may set progress and commit the
/// terminal state without the reload core inventing a duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreechMechanism {
    Closed,
    Opening { progress: u16 },
    Open,
    Closing { progress: u16 },
    OutOfBattery,
    Jammed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChamberState {
    Empty,
    /// Round has entered the mouth but has not received the final ram action.
    Staged {
        round: RoundItem,
        ignition: IgnitionDisposition,
    },
    Live {
        round: RoundItem,
        ignition: IgnitionDisposition,
        attempts: u8,
    },
    /// A true non-delayed failure.  Ordinary opening is interlocked; the caller must explicitly use
    /// `request_open_after_misfire` after applying its chosen safety procedure.
    Misfired {
        round: RoundItem,
        attempts: u8,
        last_attempt_tick: u64,
        /// Granted only by `request_open_after_misfire`.  It remains attached to the physical
        /// chamber state if the mechanism jams while opening, so recovery cannot bypass the safety
        /// procedure and cannot forget a procedure that was already completed.
        opening_authorized: bool,
    },
    /// The ignition deadline is fixed on the first trigger attempt.  No later action rerolls it.
    SuspectedHangfire {
        round: RoundItem,
        attempts: u8,
        ignite_at_tick: u64,
    },
    FiredCase(CaseItem),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemLocation {
    Chamber,
    Hand,
    Rack(u16),
    Loose(u16),
    CaseSack(u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemAccounting {
    pub live_rounds: u32,
    pub cases: u32,
    pub total_items: u32,
    pub commissioned_items: u32,
    pub shots_fired: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerResult {
    Discharged { case: CaseItem },
    Misfire { attempts: u8 },
    HangfireArmed { ignite_at_tick: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeEvent {
    None,
    HangfireDischarged { case: CaseItem, scheduled_tick: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemiAutomaticCycle {
    EjectedToSack {
        slot: u16,
        case: CaseItem,
    },
    /// The breech is open but the case remains authoritative in the chamber, ready for manual removal.
    CaseRetained {
        case: CaseItem,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReloadError {
    CapacityTooLarge,
    InvalidRoundCondition(u8),
    InvalidIgnitionPolicy,
    DuplicateItemId(ItemId),
    RackIndexOutOfBounds(usize),
    LooseIndexOutOfBounds(usize),
    SackIndexOutOfBounds(usize),
    RackEmpty(usize),
    LooseEmpty(usize),
    SackEmpty(usize),
    RackOccupied(usize),
    LooseOccupied(usize),
    SackOccupied(usize),
    LooseStorageFull,
    CaseSackFull,
    HandOccupied,
    HandEmpty,
    WrongHandItem,
    ChamberNotEmpty,
    ChamberEmpty,
    RoundNotRammed,
    CaseObstructingBreech,
    BreechNotClosed,
    BreechNotOpen,
    MechanismBusy,
    OutOfBattery,
    Jammed,
    PendingMisfire,
    PendingHangfire { ignite_at_tick: u64 },
    NothingToExtract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvariantError {
    DuplicateItemId(ItemId),
    InvalidRoundCondition { id: ItemId, condition: u8 },
    InventoryCountMismatch { expected: u32, actual: u32 },
    OutOfBatteryWithoutFiredCase,
    FiredCaseInInvalidMechanism,
    StagedRoundWithoutOpenBreech,
    HangfireWithoutClosedBreech,
    UnauthorizedMisfireOpening,
    ClosedOnStagedRound,
    ClosingOnObstruction,
}

#[derive(Clone, Copy)]
enum ItemKind {
    Round,
    Case,
}

/// Bounded authoritative state.  Capacities are compile-time choices so gameplay variants can size
/// racks, loose storage, and the receiving sack without this module claiming a historical capacity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadSystem<const RACK: usize, const LOOSE: usize, const SACK: usize> {
    mechanism: BreechMechanism,
    chamber: ChamberState,
    hand: Option<HandItem>,
    rack: [Option<StowedItem>; RACK],
    loose: [Option<LooseItem>; LOOSE],
    case_sack: [Option<CaseItem>; SACK],
    reliability_seed: u64,
    ignition_policy: IgnitionPolicy,
    commissioned_items: u32,
    shots_fired: u32,
}

impl<const RACK: usize, const LOOSE: usize, const SACK: usize> ReloadSystem<RACK, LOOSE, SACK> {
    pub fn new(
        reliability_seed: u64,
        ignition_policy: IgnitionPolicy,
    ) -> Result<Self, ReloadError> {
        if RACK > u16::MAX as usize || LOOSE > u16::MAX as usize || SACK > u16::MAX as usize {
            return Err(ReloadError::CapacityTooLarge);
        }
        Ok(Self {
            mechanism: BreechMechanism::Closed,
            chamber: ChamberState::Empty,
            hand: None,
            rack: [None; RACK],
            loose: [None; LOOSE],
            case_sack: [None; SACK],
            reliability_seed,
            ignition_policy,
            commissioned_items: 0,
            shots_fired: 0,
        })
    }

    pub fn mechanism(&self) -> BreechMechanism {
        self.mechanism
    }

    pub fn chamber(&self) -> ChamberState {
        self.chamber
    }

    pub fn hand(&self) -> Option<HandItem> {
        self.hand
    }

    pub fn rack(&self) -> &[Option<StowedItem>; RACK] {
        &self.rack
    }

    pub fn loose(&self) -> &[Option<LooseItem>; LOOSE] {
        &self.loose
    }

    pub fn case_sack(&self) -> &[Option<CaseItem>; SACK] {
        &self.case_sack
    }

    pub fn reliability_seed(&self) -> u64 {
        self.reliability_seed
    }

    pub fn ignition_policy(&self) -> IgnitionPolicy {
        self.ignition_policy
    }

    /// Commission one item directly into a rack slot.  This is an initialization/load boundary, not
    /// an ordinary move: the conservation baseline increases only here and in the other `commission_*`
    /// methods.
    pub fn commission_rack_item(
        &mut self,
        slot: usize,
        item: StowedItem,
    ) -> Result<(), ReloadError> {
        self.validate_external_item(item)?;
        self.ensure_unique(item.id())?;
        let target = self
            .rack
            .get(slot)
            .ok_or(ReloadError::RackIndexOutOfBounds(slot))?;
        if target.is_some() {
            return Err(ReloadError::RackOccupied(slot));
        }
        self.rack[slot] = Some(item);
        self.commissioned_items += 1;
        Ok(())
    }

    pub fn commission_loose_item(
        &mut self,
        slot: usize,
        item: StowedItem,
        pose: PackedPose,
    ) -> Result<(), ReloadError> {
        self.validate_external_item(item)?;
        self.ensure_unique(item.id())?;
        let target = self
            .loose
            .get(slot)
            .ok_or(ReloadError::LooseIndexOutOfBounds(slot))?;
        if target.is_some() {
            return Err(ReloadError::LooseOccupied(slot));
        }
        self.loose[slot] = Some(LooseItem { item, pose });
        self.commissioned_items += 1;
        Ok(())
    }

    pub fn commission_sack_case(&mut self, slot: usize, case: CaseItem) -> Result<(), ReloadError> {
        self.ensure_unique(case.id)?;
        let target = self
            .case_sack
            .get(slot)
            .ok_or(ReloadError::SackIndexOutOfBounds(slot))?;
        if target.is_some() {
            return Err(ReloadError::SackOccupied(slot));
        }
        self.case_sack[slot] = Some(case);
        self.commissioned_items += 1;
        Ok(())
    }

    /// Establish an already-loaded starting gun without creating a free extra rack round.
    pub fn commission_chambered_round(&mut self, round: RoundItem) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Closed {
            return Err(ReloadError::BreechNotClosed);
        }
        if self.chamber != ChamberState::Empty {
            return Err(ReloadError::ChamberNotEmpty);
        }
        self.validate_round(round)?;
        self.ensure_unique(round.id)?;
        self.chamber = ChamberState::Live {
            round,
            ignition: self.plan_ignition(round),
            attempts: 0,
        };
        self.commissioned_items += 1;
        Ok(())
    }

    pub fn grab_from_rack(&mut self, slot: usize) -> Result<(), ReloadError> {
        self.require_empty_hand()?;
        let item = *self
            .rack
            .get(slot)
            .ok_or(ReloadError::RackIndexOutOfBounds(slot))?;
        let item = item.ok_or(ReloadError::RackEmpty(slot))?;
        self.rack[slot] = None;
        self.hand = Some(item.in_hand());
        Ok(())
    }

    pub fn grab_loose(&mut self, slot: usize) -> Result<(), ReloadError> {
        self.require_empty_hand()?;
        let item = *self
            .loose
            .get(slot)
            .ok_or(ReloadError::LooseIndexOutOfBounds(slot))?;
        let item = item.ok_or(ReloadError::LooseEmpty(slot))?;
        self.loose[slot] = None;
        self.hand = Some(item.item.in_hand());
        Ok(())
    }

    pub fn grab_case_from_sack(&mut self, slot: usize) -> Result<(), ReloadError> {
        self.require_empty_hand()?;
        let case = *self
            .case_sack
            .get(slot)
            .ok_or(ReloadError::SackIndexOutOfBounds(slot))?;
        let case = case.ok_or(ReloadError::SackEmpty(slot))?;
        self.case_sack[slot] = None;
        self.hand = Some(HandItem::Case(case));
        Ok(())
    }

    pub fn place_hand_in_rack(&mut self, slot: usize) -> Result<(), ReloadError> {
        let target = self
            .rack
            .get(slot)
            .ok_or(ReloadError::RackIndexOutOfBounds(slot))?;
        if target.is_some() {
            return Err(ReloadError::RackOccupied(slot));
        }
        let item = self.hand.ok_or(ReloadError::HandEmpty)?;
        self.rack[slot] = Some(item.stowed());
        self.hand = None;
        Ok(())
    }

    pub fn place_hand_loose(&mut self, pose: PackedPose) -> Result<u16, ReloadError> {
        let slot = self
            .loose
            .iter()
            .position(Option::is_none)
            .ok_or(ReloadError::LooseStorageFull)?;
        let item = self.hand.ok_or(ReloadError::HandEmpty)?;
        self.loose[slot] = Some(LooseItem {
            item: item.stowed(),
            pose,
        });
        self.hand = None;
        Ok(slot as u16)
    }

    pub fn place_hand_case_in_sack(&mut self) -> Result<u16, ReloadError> {
        let case = match self.hand {
            Some(HandItem::Case(case)) => case,
            Some(HandItem::Round(_)) => return Err(ReloadError::WrongHandItem),
            None => return Err(ReloadError::HandEmpty),
        };
        let slot = self
            .case_sack
            .iter()
            .position(Option::is_none)
            .ok_or(ReloadError::CaseSackFull)?;
        self.case_sack[slot] = Some(case);
        self.hand = None;
        Ok(slot as u16)
    }

    pub fn request_open(&mut self) -> Result<(), ReloadError> {
        match self.mechanism {
            BreechMechanism::Closed => {}
            BreechMechanism::OutOfBattery => return Err(ReloadError::OutOfBattery),
            BreechMechanism::Jammed => return Err(ReloadError::Jammed),
            _ => return Err(ReloadError::MechanismBusy),
        }
        match self.chamber {
            ChamberState::Misfired { .. } => return Err(ReloadError::PendingMisfire),
            ChamberState::SuspectedHangfire { ignite_at_tick, .. } => {
                return Err(ReloadError::PendingHangfire { ignite_at_tick })
            }
            ChamberState::FiredCase(_) => return Err(ReloadError::OutOfBattery),
            ChamberState::Staged { .. } => return Err(ReloadError::RoundNotRammed),
            _ => {}
        }
        self.mechanism = BreechMechanism::Opening { progress: 0 };
        Ok(())
    }

    /// Explicit escape hatch for a safety policy owned by the integration layer.  No wait is implied
    /// here; calling this is the deliberate authorization that ordinary `request_open` refuses.
    pub fn request_open_after_misfire(&mut self) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Closed {
            return Err(ReloadError::BreechNotClosed);
        }
        let ChamberState::Misfired {
            round,
            attempts,
            last_attempt_tick,
            ..
        } = self.chamber
        else {
            return Err(ReloadError::PendingMisfire);
        };
        self.chamber = ChamberState::Misfired {
            round,
            attempts,
            last_attempt_tick,
            opening_authorized: true,
        };
        self.mechanism = BreechMechanism::Opening { progress: 0 };
        Ok(())
    }

    pub fn request_close(&mut self) -> Result<(), ReloadError> {
        match self.mechanism {
            BreechMechanism::Open => {}
            BreechMechanism::Jammed => return Err(ReloadError::Jammed),
            _ => return Err(ReloadError::BreechNotOpen),
        }
        match self.chamber {
            ChamberState::Staged { .. } => return Err(ReloadError::RoundNotRammed),
            ChamberState::FiredCase(_) => return Err(ReloadError::CaseObstructingBreech),
            ChamberState::Misfired { .. } => return Err(ReloadError::PendingMisfire),
            ChamberState::SuspectedHangfire { ignite_at_tick, .. } => {
                return Err(ReloadError::PendingHangfire { ignite_at_tick })
            }
            _ => {}
        }
        self.mechanism = BreechMechanism::Closing { progress: 0 };
        Ok(())
    }

    pub fn set_motion_progress(&mut self, progress: u16) -> Result<(), ReloadError> {
        self.mechanism = match self.mechanism {
            BreechMechanism::Opening { .. } => BreechMechanism::Opening { progress },
            BreechMechanism::Closing { .. } => BreechMechanism::Closing { progress },
            BreechMechanism::Jammed => return Err(ReloadError::Jammed),
            _ => return Err(ReloadError::MechanismBusy),
        };
        Ok(())
    }

    pub fn finish_open(&mut self) -> Result<(), ReloadError> {
        if !matches!(self.mechanism, BreechMechanism::Opening { .. }) {
            return Err(ReloadError::MechanismBusy);
        }
        self.mechanism = BreechMechanism::Open;
        Ok(())
    }

    pub fn finish_close(&mut self) -> Result<(), ReloadError> {
        if !matches!(self.mechanism, BreechMechanism::Closing { .. }) {
            return Err(ReloadError::MechanismBusy);
        }
        self.mechanism = BreechMechanism::Closed;
        Ok(())
    }

    /// Move the hand-held round into the chamber mouth.  This commits its deterministic ignition plan
    /// once; [`ram_staged_round`](Self::ram_staged_round) is a separate physical action.
    pub fn seat_hand_round(&mut self) -> Result<IgnitionDisposition, ReloadError> {
        if self.mechanism != BreechMechanism::Open {
            return Err(ReloadError::BreechNotOpen);
        }
        if self.chamber != ChamberState::Empty {
            return Err(ReloadError::ChamberNotEmpty);
        }
        let round = match self.hand {
            Some(HandItem::Round(round)) => round,
            Some(HandItem::Case(_)) => return Err(ReloadError::WrongHandItem),
            None => return Err(ReloadError::HandEmpty),
        };
        let ignition = self.plan_ignition(round);
        self.hand = None;
        self.chamber = ChamberState::Staged { round, ignition };
        Ok(ignition)
    }

    pub fn ram_staged_round(&mut self) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Open {
            return Err(ReloadError::BreechNotOpen);
        }
        let ChamberState::Staged { round, ignition } = self.chamber else {
            return Err(ReloadError::RoundNotRammed);
        };
        self.chamber = ChamberState::Live {
            round,
            ignition,
            attempts: 0,
        };
        Ok(())
    }

    pub fn extract_round_to_hand(&mut self) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Open {
            return Err(ReloadError::BreechNotOpen);
        }
        self.require_empty_hand()?;
        let round = match self.chamber {
            ChamberState::Staged { round, .. }
            | ChamberState::Live { round, .. }
            | ChamberState::Misfired { round, .. } => round,
            ChamberState::SuspectedHangfire { ignite_at_tick, .. } => {
                return Err(ReloadError::PendingHangfire { ignite_at_tick })
            }
            _ => return Err(ReloadError::NothingToExtract),
        };
        self.chamber = ChamberState::Empty;
        self.hand = Some(HandItem::Round(round));
        Ok(())
    }

    pub fn extract_case_to_hand(&mut self) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Open {
            return Err(ReloadError::BreechNotOpen);
        }
        self.require_empty_hand()?;
        let ChamberState::FiredCase(case) = self.chamber else {
            return Err(ReloadError::NothingToExtract);
        };
        self.chamber = ChamberState::Empty;
        self.hand = Some(HandItem::Case(case));
        Ok(())
    }

    pub fn trigger(&mut self, now_tick: u64) -> Result<TriggerResult, ReloadError> {
        if self.mechanism != BreechMechanism::Closed {
            return Err(match self.mechanism {
                BreechMechanism::OutOfBattery => ReloadError::OutOfBattery,
                BreechMechanism::Jammed => ReloadError::Jammed,
                _ => ReloadError::BreechNotClosed,
            });
        }
        match self.chamber {
            ChamberState::Live {
                round,
                ignition,
                attempts,
            } => {
                let attempts = attempts.saturating_add(1);
                match ignition {
                    IgnitionDisposition::Immediate => Ok(self.discharge(round)),
                    IgnitionDisposition::Misfire => {
                        self.chamber = ChamberState::Misfired {
                            round,
                            attempts,
                            last_attempt_tick: now_tick,
                            opening_authorized: false,
                        };
                        Ok(TriggerResult::Misfire { attempts })
                    }
                    IgnitionDisposition::Hangfire { delay_ticks } => {
                        let ignite_at_tick = now_tick.saturating_add(u64::from(delay_ticks));
                        self.chamber = ChamberState::SuspectedHangfire {
                            round,
                            attempts,
                            ignite_at_tick,
                        };
                        Ok(TriggerResult::HangfireArmed { ignite_at_tick })
                    }
                }
            }
            ChamberState::Misfired {
                round, attempts, ..
            } => {
                // The plan is already a non-delayed misfire.  Re-striking records an attempt but
                // cannot secretly reroll the round into success or hangfire.
                let attempts = attempts.saturating_add(1);
                self.chamber = ChamberState::Misfired {
                    round,
                    attempts,
                    last_attempt_tick: now_tick,
                    // A new strike is a new safety event.  Opening must be authorized again.
                    opening_authorized: false,
                };
                Ok(TriggerResult::Misfire { attempts })
            }
            ChamberState::SuspectedHangfire { ignite_at_tick, .. } => {
                Err(ReloadError::PendingHangfire { ignite_at_tick })
            }
            ChamberState::Empty => Err(ReloadError::ChamberEmpty),
            ChamberState::Staged { .. } => Err(ReloadError::RoundNotRammed),
            ChamberState::FiredCase(_) => Err(ReloadError::OutOfBattery),
        }
    }

    /// Advance deterministic delayed ignition.  Calling this repeatedly before or after the deadline
    /// cannot produce another roll or another discharge.
    pub fn advance_to_tick(&mut self, now_tick: u64) -> TimeEvent {
        let ChamberState::SuspectedHangfire {
            round,
            ignite_at_tick,
            ..
        } = self.chamber
        else {
            return TimeEvent::None;
        };
        if now_tick < ignite_at_tick {
            return TimeEvent::None;
        }
        let TriggerResult::Discharged { case } = self.discharge(round) else {
            unreachable!("discharge always produces a fired case")
        };
        TimeEvent::HangfireDischarged {
            case,
            scheduled_tick: ignite_at_tick,
        }
    }

    /// Commit the normal semi-automatic return/ejection.  A full receiving sack never deletes the
    /// case: the mechanism opens and leaves the same case in the chamber for manual extraction.
    pub fn complete_semiautomatic_cycle(&mut self) -> Result<SemiAutomaticCycle, ReloadError> {
        if self.mechanism != BreechMechanism::OutOfBattery {
            return Err(ReloadError::OutOfBattery);
        }
        let ChamberState::FiredCase(case) = self.chamber else {
            return Err(ReloadError::OutOfBattery);
        };
        self.mechanism = BreechMechanism::Open;
        if let Some(slot) = self.case_sack.iter().position(Option::is_none) {
            self.case_sack[slot] = Some(case);
            self.chamber = ChamberState::Empty;
            Ok(SemiAutomaticCycle::EjectedToSack {
                slot: slot as u16,
                case,
            })
        } else {
            Ok(SemiAutomaticCycle::CaseRetained { case })
        }
    }

    /// A jam preserves every physical item.  Clearing it restores a caller-selected stable mechanism
    /// state only when that state is structurally compatible with the chamber.
    pub fn jam(&mut self) {
        self.mechanism = BreechMechanism::Jammed;
    }

    pub fn clear_jam_to(&mut self, target: BreechMechanism) -> Result<(), ReloadError> {
        if self.mechanism != BreechMechanism::Jammed {
            return Err(ReloadError::Jammed);
        }
        if !matches!(target, BreechMechanism::Closed | BreechMechanism::Open) {
            return Err(ReloadError::MechanismBusy);
        }
        let old = self.mechanism;
        self.mechanism = target;
        if self.validate_structure().is_err() {
            self.mechanism = old;
            return Err(ReloadError::MechanismBusy);
        }
        Ok(())
    }

    pub fn locate(&self, id: ItemId) -> Option<ItemLocation> {
        if chamber_id(self.chamber) == Some(id) {
            return Some(ItemLocation::Chamber);
        }
        if self.hand.is_some_and(|item| item.id() == id) {
            return Some(ItemLocation::Hand);
        }
        if let Some(slot) = self
            .rack
            .iter()
            .position(|entry| entry.is_some_and(|item| item.id() == id))
        {
            return Some(ItemLocation::Rack(slot as u16));
        }
        if let Some(slot) = self
            .loose
            .iter()
            .position(|entry| entry.is_some_and(|placed| placed.item.id() == id))
        {
            return Some(ItemLocation::Loose(slot as u16));
        }
        self.case_sack
            .iter()
            .position(|entry| entry.is_some_and(|case| case.id == id))
            .map(|slot| ItemLocation::CaseSack(slot as u16))
    }

    pub fn accounting(&self) -> ItemAccounting {
        let mut accounting = ItemAccounting {
            commissioned_items: self.commissioned_items,
            shots_fired: self.shots_fired,
            ..ItemAccounting::default()
        };
        self.visit_items(|_, kind, _| {
            accounting.total_items += 1;
            match kind {
                ItemKind::Round => accounting.live_rounds += 1,
                ItemKind::Case => accounting.cases += 1,
            }
        });
        accounting
    }

    /// Audit the two invariants integration code may assert after any action or decoded snapshot:
    /// every identity is present once, and the count still equals the commissioned baseline.
    pub fn validate(&self) -> Result<ItemAccounting, InvariantError> {
        self.validate_structure()?;
        let mut ids = BTreeSet::new();
        let mut bad_condition = None;
        let mut duplicate = None;
        self.visit_items(|id, _, condition| {
            if !ids.insert(id) {
                duplicate = Some(id);
            }
            if let Some(condition) = condition {
                if condition > MAX_ROUND_CONDITION {
                    bad_condition = Some((id, condition));
                }
            }
        });
        if let Some(id) = duplicate {
            return Err(InvariantError::DuplicateItemId(id));
        }
        if let Some((id, condition)) = bad_condition {
            return Err(InvariantError::InvalidRoundCondition { id, condition });
        }
        let accounting = self.accounting();
        if accounting.total_items != self.commissioned_items {
            return Err(InvariantError::InventoryCountMismatch {
                expected: self.commissioned_items,
                actual: accounting.total_items,
            });
        }
        Ok(accounting)
    }

    fn validate_structure(&self) -> Result<(), InvariantError> {
        match self.mechanism {
            BreechMechanism::OutOfBattery
                if !matches!(self.chamber, ChamberState::FiredCase(_)) =>
            {
                return Err(InvariantError::OutOfBatteryWithoutFiredCase)
            }
            BreechMechanism::Closed if matches!(self.chamber, ChamberState::Staged { .. }) => {
                return Err(InvariantError::ClosedOnStagedRound)
            }
            BreechMechanism::Closing { .. }
                if matches!(
                    self.chamber,
                    ChamberState::Staged { .. }
                        | ChamberState::FiredCase(_)
                        | ChamberState::Misfired { .. }
                        | ChamberState::SuspectedHangfire { .. }
                ) =>
            {
                return Err(InvariantError::ClosingOnObstruction)
            }
            _ => {}
        }
        if matches!(self.chamber, ChamberState::FiredCase(_))
            && !matches!(
                self.mechanism,
                BreechMechanism::OutOfBattery | BreechMechanism::Open | BreechMechanism::Jammed
            )
        {
            return Err(InvariantError::FiredCaseInInvalidMechanism);
        }
        if matches!(self.chamber, ChamberState::Staged { .. })
            && !matches!(
                self.mechanism,
                BreechMechanism::Open | BreechMechanism::Jammed
            )
        {
            return Err(InvariantError::StagedRoundWithoutOpenBreech);
        }
        if matches!(self.chamber, ChamberState::SuspectedHangfire { .. })
            && !matches!(
                self.mechanism,
                BreechMechanism::Closed | BreechMechanism::Jammed
            )
        {
            return Err(InvariantError::HangfireWithoutClosedBreech);
        }
        if matches!(
            self.chamber,
            ChamberState::Misfired {
                opening_authorized: false,
                ..
            }
        ) && matches!(
            self.mechanism,
            BreechMechanism::Opening { .. } | BreechMechanism::Open
        ) {
            return Err(InvariantError::UnauthorizedMisfireOpening);
        }
        Ok(())
    }

    fn require_empty_hand(&self) -> Result<(), ReloadError> {
        if self.hand.is_some() {
            Err(ReloadError::HandOccupied)
        } else {
            Ok(())
        }
    }

    fn validate_round(&self, round: RoundItem) -> Result<(), ReloadError> {
        if round.condition > MAX_ROUND_CONDITION {
            Err(ReloadError::InvalidRoundCondition(round.condition))
        } else {
            Ok(())
        }
    }

    fn validate_external_item(&self, item: StowedItem) -> Result<(), ReloadError> {
        match item {
            StowedItem::Round(round) => self.validate_round(round),
            StowedItem::Case(_) => Ok(()),
        }
    }

    fn ensure_unique(&self, id: ItemId) -> Result<(), ReloadError> {
        if self.locate(id).is_some() {
            Err(ReloadError::DuplicateItemId(id))
        } else {
            Ok(())
        }
    }

    fn plan_ignition(&self, round: RoundItem) -> IgnitionDisposition {
        let value = mix64(
            self.reliability_seed
                ^ u64::from(round.id.0).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (u64::from(round.nature as u8) << 48)
                ^ (u64::from(round.condition) << 56),
        );
        let failure_roll = (value & 0xffff) as u32;
        if failure_roll >= self.ignition_policy.failure_threshold(round.condition) {
            return IgnitionDisposition::Immediate;
        }
        let kind_roll = ((value >> 16) & 0xffff) as u32;
        if kind_roll < self.ignition_policy.hangfire_share_per_65536 {
            IgnitionDisposition::Hangfire {
                delay_ticks: self.ignition_policy.hangfire_delay_ticks,
            }
        } else {
            IgnitionDisposition::Misfire
        }
    }

    fn discharge(&mut self, round: RoundItem) -> TriggerResult {
        let case = CaseItem::fired(round);
        self.chamber = ChamberState::FiredCase(case);
        self.mechanism = BreechMechanism::OutOfBattery;
        self.shots_fired = self.shots_fired.saturating_add(1);
        TriggerResult::Discharged { case }
    }

    fn visit_items(&self, mut visit: impl FnMut(ItemId, ItemKind, Option<u8>)) {
        match self.chamber {
            ChamberState::Staged { round, .. }
            | ChamberState::Live { round, .. }
            | ChamberState::Misfired { round, .. }
            | ChamberState::SuspectedHangfire { round, .. } => {
                visit(round.id, ItemKind::Round, Some(round.condition));
            }
            ChamberState::FiredCase(case) => visit(case.id, ItemKind::Case, None),
            ChamberState::Empty => {}
        }
        if let Some(item) = self.hand {
            visit_hand_item(item, &mut visit);
        }
        for item in self.rack.iter().flatten().copied() {
            visit_stowed_item(item, &mut visit);
        }
        for placed in self.loose.iter().flatten().copied() {
            visit_stowed_item(placed.item, &mut visit);
        }
        for case in self.case_sack.iter().flatten().copied() {
            visit(case.id, ItemKind::Case, None);
        }
    }
}

fn chamber_id(chamber: ChamberState) -> Option<ItemId> {
    match chamber {
        ChamberState::Staged { round, .. }
        | ChamberState::Live { round, .. }
        | ChamberState::Misfired { round, .. }
        | ChamberState::SuspectedHangfire { round, .. } => Some(round.id),
        ChamberState::FiredCase(case) => Some(case.id),
        ChamberState::Empty => None,
    }
}

fn visit_hand_item(item: HandItem, visit: &mut impl FnMut(ItemId, ItemKind, Option<u8>)) {
    match item {
        HandItem::Round(round) => visit(round.id, ItemKind::Round, Some(round.condition)),
        HandItem::Case(case) => visit(case.id, ItemKind::Case, None),
    }
}

fn visit_stowed_item(item: StowedItem, visit: &mut impl FnMut(ItemId, ItemKind, Option<u8>)) {
    match item {
        StowedItem::Round(round) => visit(round.id, ItemKind::Round, Some(round.condition)),
        StowedItem::Case(case) => visit(case.id, ItemKind::Case, None),
    }
}

/// SplitMix64 finalizer: deterministic, dependency-free, and used only to choose a cached plan.
fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestReload = ReloadSystem<4, 3, 2>;

    fn immediate_policy() -> IgnitionPolicy {
        IgnitionPolicy::new(0, 0, 0, 0).unwrap()
    }

    fn forced_misfire_policy() -> IgnitionPolicy {
        IgnitionPolicy::new(PROBABILITY_SCALE, PROBABILITY_SCALE, 0, 0).unwrap()
    }

    fn forced_hangfire_policy(delay: u32) -> IgnitionPolicy {
        IgnitionPolicy::new(
            PROBABILITY_SCALE,
            PROBABILITY_SCALE,
            PROBABILITY_SCALE,
            delay,
        )
        .unwrap()
    }

    fn round(id: u32, condition: u8) -> RoundItem {
        RoundItem::new(ItemId(id), ShellNature::PzGr39, condition).unwrap()
    }

    fn open<const R: usize, const L: usize, const S: usize>(system: &mut ReloadSystem<R, L, S>) {
        system.request_open().unwrap();
        system.set_motion_progress(u16::MAX / 2).unwrap();
        system.finish_open().unwrap();
    }

    fn close<const R: usize, const L: usize, const S: usize>(system: &mut ReloadSystem<R, L, S>) {
        system.request_close().unwrap();
        system.set_motion_progress(u16::MAX).unwrap();
        system.finish_close().unwrap();
    }

    fn seat_and_ram<const R: usize, const L: usize, const S: usize>(
        system: &mut ReloadSystem<R, L, S>,
    ) {
        system.seat_hand_round().unwrap();
        system.ram_staged_round().unwrap();
    }

    #[test]
    fn constructors_reject_unbounded_or_invalid_inputs() {
        assert_eq!(
            RoundItem::new(ItemId(1), ShellNature::SprGr, 101),
            Err(ReloadError::InvalidRoundCondition(101))
        );
        assert_eq!(
            IgnitionPolicy::new(2, 1, 0, 0),
            Err(ReloadError::InvalidIgnitionPolicy)
        );
        assert_eq!(
            IgnitionPolicy::new(0, 0, 1, 0),
            Err(ReloadError::InvalidIgnitionPolicy)
        );
        assert_eq!(
            IgnitionPolicy::new(PROBABILITY_SCALE + 1, PROBABILITY_SCALE + 1, 0, 0),
            Err(ReloadError::InvalidIgnitionPolicy)
        );
    }

    #[test]
    fn duplicate_ids_are_rejected_at_every_commission_boundary() {
        let mut system = TestReload::new(7, immediate_policy()).unwrap();
        let item = StowedItem::Round(round(10, 100));
        system.commission_rack_item(0, item).unwrap();
        assert_eq!(
            system.commission_loose_item(0, item, PackedPose::default()),
            Err(ReloadError::DuplicateItemId(ItemId(10)))
        );
        assert_eq!(
            system.commission_sack_case(0, CaseItem::new(ItemId(10), ShellNature::PzGr39, 0)),
            Err(ReloadError::DuplicateItemId(ItemId(10)))
        );
        assert_eq!(
            system.commission_chambered_round(round(10, 100)),
            Err(ReloadError::DuplicateItemId(ItemId(10)))
        );
        assert_eq!(system.validate().unwrap().total_items, 1);
    }

    #[test]
    fn empty_and_out_of_bounds_sources_are_distinct_and_non_mutating() {
        let mut system = TestReload::new(8, immediate_policy()).unwrap();
        assert_eq!(system.grab_from_rack(0), Err(ReloadError::RackEmpty(0)));
        assert_eq!(
            system.grab_from_rack(4),
            Err(ReloadError::RackIndexOutOfBounds(4))
        );
        assert_eq!(system.grab_loose(0), Err(ReloadError::LooseEmpty(0)));
        assert_eq!(
            system.grab_loose(3),
            Err(ReloadError::LooseIndexOutOfBounds(3))
        );
        assert_eq!(
            system.grab_case_from_sack(0),
            Err(ReloadError::SackEmpty(0))
        );
        assert_eq!(
            system.grab_case_from_sack(2),
            Err(ReloadError::SackIndexOutOfBounds(2))
        );
        assert_eq!(system.validate().unwrap().total_items, 0);
    }

    #[test]
    fn normal_semiautomatic_cycle_preserves_one_identity_end_to_end() {
        let mut system = TestReload::new(11, immediate_policy()).unwrap();
        let id = ItemId(21);
        system
            .commission_rack_item(0, StowedItem::Round(round(id.0, 100)))
            .unwrap();
        assert_eq!(system.locate(id), Some(ItemLocation::Rack(0)));

        system.grab_from_rack(0).unwrap();
        assert_eq!(system.locate(id), Some(ItemLocation::Hand));
        open(&mut system);
        assert_eq!(
            system.seat_hand_round().unwrap(),
            IgnitionDisposition::Immediate
        );
        assert_eq!(system.locate(id), Some(ItemLocation::Chamber));
        assert_eq!(system.request_close(), Err(ReloadError::RoundNotRammed));
        system.ram_staged_round().unwrap();
        close(&mut system);

        let TriggerResult::Discharged { case } = system.trigger(50).unwrap() else {
            panic!("immediate policy must discharge")
        };
        assert_eq!(case.id(), id);
        assert_eq!(system.mechanism(), BreechMechanism::OutOfBattery);
        assert_eq!(system.locate(id), Some(ItemLocation::Chamber));

        assert_eq!(
            system.complete_semiautomatic_cycle().unwrap(),
            SemiAutomaticCycle::EjectedToSack { slot: 0, case }
        );
        assert_eq!(system.mechanism(), BreechMechanism::Open);
        assert_eq!(system.chamber(), ChamberState::Empty);
        assert_eq!(system.locate(id), Some(ItemLocation::CaseSack(0)));
        let accounting = system.validate().unwrap();
        assert_eq!(accounting.live_rounds, 0);
        assert_eq!(accounting.cases, 1);
        assert_eq!(accounting.total_items, 1);
        assert_eq!(accounting.commissioned_items, 1);
        assert_eq!(accounting.shots_fired, 1);
    }

    #[test]
    fn every_shell_nature_preserves_identity_when_it_becomes_a_case() {
        let natures = [
            ShellNature::PzGr39,
            ShellNature::SprGr,
            ShellNature::PzGr40,
            ShellNature::Practice,
        ];
        for (offset, nature) in natures.into_iter().enumerate() {
            let id = ItemId(200 + offset as u32);
            let mut system = TestReload::new(12, immediate_policy()).unwrap();
            system
                .commission_chambered_round(RoundItem::new(id, nature, 100).unwrap())
                .unwrap();

            let TriggerResult::Discharged { case } = system.trigger(0).unwrap() else {
                panic!("immediate policy must discharge {nature:?}")
            };
            assert_eq!(case.id(), id);
            assert_eq!(case.source_nature(), nature);
            assert_eq!(system.locate(id), Some(ItemLocation::Chamber));
            let accounting = system.validate().unwrap();
            assert_eq!(accounting.live_rounds, 0);
            assert_eq!(accounting.cases, 1);
            assert_eq!(accounting.total_items, 1);
            assert_eq!(accounting.commissioned_items, 1);
            assert_eq!(accounting.shots_fired, 1);
        }
    }

    #[test]
    fn starting_chamber_is_not_a_free_extra_round() {
        let mut system = TestReload::new(1, immediate_policy()).unwrap();
        system.commission_chambered_round(round(1, 100)).unwrap();
        system
            .commission_rack_item(0, StowedItem::Round(round(2, 100)))
            .unwrap();
        let accounting = system.validate().unwrap();
        assert_eq!(accounting.live_rounds, 2);
        assert_eq!(accounting.commissioned_items, 2);
        assert_eq!(system.locate(ItemId(1)), Some(ItemLocation::Chamber));
        assert_eq!(system.locate(ItemId(2)), Some(ItemLocation::Rack(0)));
    }

    #[test]
    fn full_case_sack_retains_fired_case_for_manual_extraction() {
        type TinySack = ReloadSystem<3, 1, 1>;
        let mut system = TinySack::new(2, immediate_policy()).unwrap();
        system
            .commission_sack_case(0, CaseItem::new(ItemId(30), ShellNature::SprGr, 0))
            .unwrap();
        system.commission_chambered_round(round(31, 100)).unwrap();
        let TriggerResult::Discharged { case } = system.trigger(0).unwrap() else {
            panic!()
        };
        assert_eq!(
            system.complete_semiautomatic_cycle().unwrap(),
            SemiAutomaticCycle::CaseRetained { case }
        );
        assert_eq!(system.locate(ItemId(31)), Some(ItemLocation::Chamber));
        assert_eq!(
            system.request_close(),
            Err(ReloadError::CaseObstructingBreech)
        );

        system.grab_case_from_sack(0).unwrap();
        system.place_hand_in_rack(0).unwrap();
        system.extract_case_to_hand().unwrap();
        system.place_hand_in_rack(1).unwrap();
        assert_eq!(system.locate(ItemId(30)), Some(ItemLocation::Rack(0)));
        assert_eq!(system.locate(ItemId(31)), Some(ItemLocation::Rack(1)));
        assert_eq!(system.validate().unwrap().total_items, 2);
    }

    #[test]
    fn rack_loose_hand_and_sack_moves_are_transactional_and_conservative() {
        let mut system = TestReload::new(3, immediate_policy()).unwrap();
        system
            .commission_rack_item(0, StowedItem::Round(round(40, 90)))
            .unwrap();
        system
            .commission_loose_item(
                0,
                StowedItem::Case(CaseItem::new(ItemId(41), ShellNature::SprGr, 20)),
                PackedPose {
                    position_cm: [12, 34, -56],
                    rotation_q: [1, 2, 3],
                },
            )
            .unwrap();
        system.grab_from_rack(0).unwrap();
        assert_eq!(
            system.place_hand_case_in_sack(),
            Err(ReloadError::WrongHandItem)
        );
        assert_eq!(system.locate(ItemId(40)), Some(ItemLocation::Hand));
        let loose_slot = system.place_hand_loose(PackedPose::default()).unwrap();
        assert_eq!(loose_slot, 1);
        system.grab_loose(0).unwrap();
        assert_eq!(system.place_hand_case_in_sack().unwrap(), 0);
        assert_eq!(system.locate(ItemId(40)), Some(ItemLocation::Loose(1)));
        assert_eq!(system.locate(ItemId(41)), Some(ItemLocation::CaseSack(0)));
        assert_eq!(system.validate().unwrap().total_items, 2);
    }

    #[test]
    fn standard_open_and_close_interlocks_reject_unsafe_states() {
        let mut system = TestReload::new(4, immediate_policy()).unwrap();
        assert_eq!(system.trigger(0), Err(ReloadError::ChamberEmpty));
        open(&mut system);
        assert_eq!(system.trigger(0), Err(ReloadError::BreechNotClosed));
        system
            .commission_loose_item(0, StowedItem::Round(round(50, 100)), PackedPose::default())
            .unwrap();
        system.grab_loose(0).unwrap();
        system.seat_hand_round().unwrap();
        assert_eq!(system.request_close(), Err(ReloadError::RoundNotRammed));
        system.ram_staged_round().unwrap();
        close(&mut system);
        system.trigger(0).unwrap();
        assert_eq!(system.request_open(), Err(ReloadError::OutOfBattery));
        assert_eq!(system.trigger(1), Err(ReloadError::OutOfBattery));
        assert!(system.validate().is_ok());
    }

    #[test]
    fn misfire_is_cached_and_restrike_never_rerolls() {
        let mut a = TestReload::new(0xA11C_E, forced_misfire_policy()).unwrap();
        let mut b = TestReload::new(0xA11C_E, forced_misfire_policy()).unwrap();
        a.commission_chambered_round(round(60, 12)).unwrap();
        b.commission_chambered_round(round(60, 12)).unwrap();
        assert_eq!(a.chamber(), b.chamber());
        assert_eq!(a.trigger(100), Ok(TriggerResult::Misfire { attempts: 1 }));
        assert_eq!(a.trigger(120), Ok(TriggerResult::Misfire { attempts: 2 }));
        assert_eq!(a.request_open(), Err(ReloadError::PendingMisfire));
        a.request_open_after_misfire().unwrap();
        a.finish_open().unwrap();
        a.extract_round_to_hand().unwrap();
        assert_eq!(a.locate(ItemId(60)), Some(ItemLocation::Hand));
        assert_eq!(a.validate().unwrap().shots_fired, 0);

        assert_eq!(b.trigger(100), Ok(TriggerResult::Misfire { attempts: 1 }));
        assert_eq!(a.accounting().total_items, b.accounting().total_items);
    }

    #[test]
    fn unauthorized_misfire_jam_recovery_cannot_open_and_is_transactional() {
        let mut system = TestReload::new(13, forced_misfire_policy()).unwrap();
        system.commission_chambered_round(round(61, 100)).unwrap();
        assert_eq!(
            system.trigger(10),
            Ok(TriggerResult::Misfire { attempts: 1 })
        );
        assert!(matches!(
            system.chamber(),
            ChamberState::Misfired {
                opening_authorized: false,
                ..
            }
        ));

        system.jam();
        let before_recovery = system.clone();
        assert_eq!(
            system.clear_jam_to(BreechMechanism::Open),
            Err(ReloadError::MechanismBusy)
        );
        assert_eq!(system, before_recovery);
        let accounting = system.validate().unwrap();
        assert_eq!(accounting.live_rounds, 1);
        assert_eq!(accounting.total_items, accounting.commissioned_items);
    }

    #[test]
    fn authorized_misfire_opening_survives_a_jam_and_recovers_open() {
        let mut system = TestReload::new(14, forced_misfire_policy()).unwrap();
        system.commission_chambered_round(round(62, 100)).unwrap();
        assert_eq!(
            system.trigger(10),
            Ok(TriggerResult::Misfire { attempts: 1 })
        );
        system.request_open_after_misfire().unwrap();
        assert!(matches!(
            system.chamber(),
            ChamberState::Misfired {
                opening_authorized: true,
                ..
            }
        ));

        system.jam();
        system.clear_jam_to(BreechMechanism::Open).unwrap();
        assert_eq!(system.mechanism(), BreechMechanism::Open);
        assert!(matches!(
            system.chamber(),
            ChamberState::Misfired {
                opening_authorized: true,
                ..
            }
        ));
        let accounting = system.validate().unwrap();
        assert_eq!(accounting.live_rounds, 1);
        assert_eq!(accounting.total_items, accounting.commissioned_items);
    }

    #[test]
    fn hangfire_has_one_fixed_deadline_and_one_delayed_discharge() {
        let mut system = TestReload::new(5, forced_hangfire_policy(37)).unwrap();
        system.commission_chambered_round(round(70, 100)).unwrap();
        assert_eq!(
            system.trigger(1_000),
            Ok(TriggerResult::HangfireArmed {
                ignite_at_tick: 1_037
            })
        );
        assert_eq!(
            system.trigger(1_001),
            Err(ReloadError::PendingHangfire {
                ignite_at_tick: 1_037
            })
        );
        assert_eq!(
            system.request_open(),
            Err(ReloadError::PendingHangfire {
                ignite_at_tick: 1_037
            })
        );
        assert_eq!(system.advance_to_tick(1_036), TimeEvent::None);
        let TimeEvent::HangfireDischarged {
            case,
            scheduled_tick,
        } = system.advance_to_tick(1_100)
        else {
            panic!("deadline must produce the cached delayed ignition")
        };
        assert_eq!(scheduled_tick, 1_037);
        assert_eq!(case.id(), ItemId(70));
        assert_eq!(system.advance_to_tick(2_000), TimeEvent::None);
        assert_eq!(system.accounting().shots_fired, 1);
        assert_eq!(system.validate().unwrap().total_items, 1);
    }

    #[test]
    fn condition_changes_probability_threshold_without_unbounded_values() {
        let policy = IgnitionPolicy::new(10, 1_010, 0, 0).unwrap();
        assert_eq!(policy.failure_threshold(100), 10);
        assert_eq!(policy.failure_threshold(0), 1_010);
        assert_eq!(policy.failure_threshold(50), 510);
    }

    #[test]
    fn deterministic_plan_survives_unload_and_reseat() {
        let mut system = TestReload::new(0xDEAD_BEEF, forced_hangfire_policy(9)).unwrap();
        system
            .commission_rack_item(0, StowedItem::Round(round(80, 55)))
            .unwrap();
        system.grab_from_rack(0).unwrap();
        open(&mut system);
        let first = system.seat_hand_round().unwrap();
        system.extract_round_to_hand().unwrap();
        let second = system.seat_hand_round().unwrap();
        assert_eq!(first, second);
        assert_eq!(second, IgnitionDisposition::Hangfire { delay_ticks: 9 });
        assert_eq!(system.validate().unwrap().total_items, 1);
    }

    #[test]
    fn failed_placements_leave_the_item_in_hand() {
        type Small = ReloadSystem<1, 0, 0>;
        let mut system = Small::new(8, immediate_policy()).unwrap();
        system
            .commission_rack_item(0, StowedItem::Round(round(90, 100)))
            .unwrap();
        system.grab_from_rack(0).unwrap();
        assert_eq!(
            system.place_hand_loose(PackedPose::default()),
            Err(ReloadError::LooseStorageFull)
        );
        assert_eq!(
            system.place_hand_case_in_sack(),
            Err(ReloadError::WrongHandItem)
        );
        assert_eq!(system.locate(ItemId(90)), Some(ItemLocation::Hand));
        assert_eq!(system.validate().unwrap().total_items, 1);
    }

    #[test]
    fn jam_and_clear_preserve_items_and_reject_incompatible_recovery() {
        let mut system = TestReload::new(9, immediate_policy()).unwrap();
        system
            .commission_rack_item(0, StowedItem::Round(round(100, 100)))
            .unwrap();
        system.grab_from_rack(0).unwrap();
        open(&mut system);
        system.seat_hand_round().unwrap();
        system.jam();
        assert_eq!(
            system.clear_jam_to(BreechMechanism::Closed),
            Err(ReloadError::MechanismBusy)
        );
        system.clear_jam_to(BreechMechanism::Open).unwrap();
        system.ram_staged_round().unwrap();
        assert_eq!(system.validate().unwrap().total_items, 1);
    }

    #[test]
    fn every_successful_action_keeps_accounting_equal_to_baseline() {
        let mut system = TestReload::new(10, immediate_policy()).unwrap();
        for id in 0..3 {
            system
                .commission_rack_item(id as usize, StowedItem::Round(round(110 + id, 100)))
                .unwrap();
        }
        assert_eq!(system.validate().unwrap().total_items, 3);
        system.grab_from_rack(0).unwrap();
        assert_eq!(system.validate().unwrap().total_items, 3);
        open(&mut system);
        seat_and_ram(&mut system);
        assert_eq!(system.validate().unwrap().total_items, 3);
        close(&mut system);
        system.trigger(0).unwrap();
        assert_eq!(system.validate().unwrap().total_items, 3);
        system.complete_semiautomatic_cycle().unwrap();
        assert_eq!(system.validate().unwrap().total_items, 3);
        system.grab_case_from_sack(0).unwrap();
        system.place_hand_in_rack(3).unwrap();
        let accounting = system.validate().unwrap();
        assert_eq!(accounting.live_rounds, 2);
        assert_eq!(accounting.cases, 1);
        assert_eq!(accounting.total_items, accounting.commissioned_items);
    }
}
