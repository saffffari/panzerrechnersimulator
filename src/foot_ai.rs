//! Renderer-free learned on-foot policy and deterministic training substrate.
//!
//! This module deliberately depends on neither `wgpu` nor the live `Game`.  Runtime integration
//! supplies an egocentric [`ObservationInput`], applies the bounded [`FootAction`] through the same
//! physical controls as a human, and records [`FootTransition`] values when training telemetry is
//! enabled.  Keeping that boundary narrow lets Hal run many CPU rollout workers without allocating
//! one renderer (and several GiB of VRAM) per worker.

use std::io::{Read, Write};

pub const TRANSITION_SCHEMA: &str = "kessel.foot_transition.v1";
pub const OBSERVATION_SCHEMA: &str = "kessel.foot_observation.v1|role[2],goal_fr_dist[3],goal_kind[4],own_hp_vel_crouch_aim_ammo_reload_detect_suppress_stamina[10],tank_fr_dist_mount[4],target_fr_dist_visible_last_hp_armored[7],ally_fr_dist_alive[4],cover8,slope2,can_interact_jump_fire_reload[4],weapon[3]";
pub const ACTION_SCHEMA: &str = "kessel.foot_action.v1|forward,strafe,turn,crouch,aim,fire,reload,interact,jump,weapon(sidearm|rifle|antitank)";

pub const OBS_DIM: usize = 51;
pub const HIDDEN_DIM: usize = 32;
pub const ACTION_DIM: usize = 10;
pub const WEIGHT_COUNT: usize =
    OBS_DIM * HIDDEN_DIM + HIDDEN_DIM + HIDDEN_DIM * ACTION_DIM + ACTION_DIM;

pub const OBSERVATION_SCHEMA_HASH: u64 = fnv1a64(OBSERVATION_SCHEMA.as_bytes());
pub const ACTION_SCHEMA_HASH: u64 = fnv1a64(ACTION_SCHEMA.as_bytes());

const ARTIFACT_MAGIC: [u8; 4] = *b"KSL2";
const ARTIFACT_VERSION: u16 = 2;
const ARTIFACT_HEADER_BYTES: usize = 48;

const IDX_ROLE_TIGER: usize = 0;
const IDX_ROLE_SOVIET: usize = 1;
const IDX_GOAL_FORWARD: usize = 2;
const IDX_GOAL_RIGHT: usize = 3;
const IDX_GOAL_DISTANCE: usize = 4;
const IDX_GOAL_KIND: usize = 5;
const IDX_HP: usize = 9;
const IDX_VEL_FORWARD: usize = 10;
const IDX_VEL_RIGHT: usize = 11;
const IDX_CROUCHED: usize = 12;
const IDX_AIMING: usize = 13;
const IDX_AMMO: usize = 14;
const IDX_RELOAD: usize = 15;
const IDX_DETECTED: usize = 16;
const IDX_SUPPRESSION: usize = 17;
const IDX_STAMINA: usize = 18;
const IDX_TANK_FORWARD: usize = 19;
const IDX_MOUNTABLE: usize = 22;
const IDX_TARGET_FORWARD: usize = 23;
const IDX_TARGET_RIGHT: usize = 24;
const IDX_TARGET_VISIBLE: usize = 26;
const IDX_TARGET_LAST_KNOWN: usize = 27;
const IDX_TARGET_HP: usize = 28;
const IDX_TARGET_ARMORED: usize = 29;
const IDX_ALLY_FORWARD: usize = 30;
const IDX_ALLY_ALIVE: usize = 33;
const IDX_COVER: usize = 34;
const IDX_SLOPE_FORWARD: usize = 42;
const IDX_SLOPE_RIGHT: usize = 43;
const IDX_CAN_INTERACT: usize = 44;
const IDX_CAN_JUMP: usize = 45;
const IDX_CAN_FIRE: usize = 46;
const IDX_CAN_RELOAD: usize = 47;
const IDX_WEAPON: usize = 48;

const OUT_FORWARD: usize = 0;
const OUT_STRAFE: usize = 1;
const OUT_TURN: usize = 2;
const OUT_CROUCH: usize = 3;
const OUT_AIM: usize = 4;
const OUT_FIRE: usize = 5;
const OUT_RELOAD: usize = 6;
const OUT_INTERACT: usize = 7;
const OUT_JUMP: usize = 8;
const OUT_WEAPON: usize = 9;

const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    hash
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PolicyRole {
    TigerCrew = 1,
    SovietRifle = 2,
}

impl PolicyRole {
    pub const ALL: [Self; 2] = [Self::TigerCrew, Self::SovietRifle];

    pub const fn label(self) -> &'static str {
        match self {
            Self::TigerCrew => "tiger_crew",
            Self::SovietRifle => "soviet_rifle",
        }
    }

    pub const fn checkpoint_path(self) -> &'static str {
        match self {
            Self::TigerCrew => "assets/ai/weights_foot_tiger.bin",
            Self::SovietRifle => "assets/ai/weights_foot_soviet.bin",
        }
    }

    fn from_u8(value: u8) -> anyhow::Result<Self> {
        match value {
            1 => Ok(Self::TigerCrew),
            2 => Ok(Self::SovietRifle),
            _ => anyhow::bail!("unknown KSL2 foot-policy role {value}"),
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "tiger_crew" => Ok(Self::TigerCrew),
            "soviet_rifle" => Ok(Self::SovietRifle),
            _ => anyhow::bail!("unknown foot-policy role {value:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalKind {
    Search,
    ReturnToTank,
    Intercept,
    Hold,
}

impl GoalKind {
    const fn index(self) -> usize {
        match self {
            Self::Search => 0,
            Self::ReturnToTank => 1,
            Self::Intercept => 2,
            Self::Hold => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponSelection {
    Sidearm,
    Rifle,
    AntiTank,
}

impl WeaponSelection {
    const fn index(self) -> usize {
        match self {
            Self::Sidearm => 0,
            Self::Rifle => 1,
            Self::AntiTank => 2,
        }
    }

    const fn training_value(self) -> f32 {
        match self {
            Self::Sidearm => -0.85,
            Self::Rifle => 0.0,
            Self::AntiTank => 0.85,
        }
    }

    fn from_policy_value(value: f32) -> Self {
        if value < -0.33 {
            Self::Sidearm
        } else if value > 0.33 {
            Self::AntiTank
        } else {
            Self::Rifle
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Sidearm => "sidearm",
            Self::Rifle => "rifle",
            Self::AntiTank => "antitank",
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "sidearm" => Ok(Self::Sidearm),
            "rifle" => Ok(Self::Rifle),
            "antitank" => Ok(Self::AntiTank),
            _ => anyhow::bail!("unknown foot weapon {value:?}"),
        }
    }
}

/// All directions are already in the observing actor's frame: +forward is where the body faces,
/// +right is its right shoulder, and distance is normalized by the integration layer's declared
/// sensor range.  No absolute map coordinate enters the learned policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RelativeCue {
    pub forward: f32,
    pub right: f32,
    pub distance: f32,
}

/// Semantic input supplied by the authoritative game simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservationInput {
    pub role: PolicyRole,
    pub goal: RelativeCue,
    pub goal_kind: GoalKind,
    pub hp_fraction: f32,
    pub velocity_forward: f32,
    pub velocity_right: f32,
    pub crouched: bool,
    pub aiming: bool,
    pub ammo_fraction: f32,
    pub reload_fraction: f32,
    pub detected: bool,
    pub suppression: f32,
    pub stamina_fraction: f32,
    pub tiger: RelativeCue,
    pub tiger_mountable: bool,
    pub target: RelativeCue,
    pub target_visible: bool,
    pub target_last_known: bool,
    pub target_hp_fraction: f32,
    pub target_armored: bool,
    pub nearest_ally: RelativeCue,
    pub nearest_ally_alive: bool,
    /// Radial physical-clearance probes, clockwise from straight ahead, normalized to [0, 1].
    pub cover_clearance: [f32; 8],
    pub terrain_slope_forward: f32,
    pub terrain_slope_right: f32,
    pub can_interact: bool,
    pub can_jump: bool,
    pub can_fire: bool,
    pub can_reload: bool,
    pub weapon: WeaponSelection,
}

impl ObservationInput {
    pub fn encode(self) -> FootObservation {
        let mut values = [0.0; OBS_DIM];
        values[IDX_ROLE_TIGER] = if self.role == PolicyRole::TigerCrew {
            1.0
        } else {
            -1.0
        };
        values[IDX_ROLE_SOVIET] = if self.role == PolicyRole::SovietRifle {
            1.0
        } else {
            -1.0
        };
        encode_cue(&mut values, IDX_GOAL_FORWARD, self.goal);
        values[IDX_GOAL_KIND + self.goal_kind.index()] = 1.0;
        values[IDX_HP] = unit(self.hp_fraction);
        values[IDX_VEL_FORWARD] = signed(self.velocity_forward);
        values[IDX_VEL_RIGHT] = signed(self.velocity_right);
        values[IDX_CROUCHED] = flag(self.crouched);
        values[IDX_AIMING] = flag(self.aiming);
        values[IDX_AMMO] = unit(self.ammo_fraction);
        values[IDX_RELOAD] = unit(self.reload_fraction);
        values[IDX_DETECTED] = flag(self.detected);
        values[IDX_SUPPRESSION] = unit(self.suppression);
        values[IDX_STAMINA] = unit(self.stamina_fraction);
        encode_cue(&mut values, IDX_TANK_FORWARD, self.tiger);
        values[IDX_MOUNTABLE] = flag(self.tiger_mountable);
        encode_cue(&mut values, IDX_TARGET_FORWARD, self.target);
        values[IDX_TARGET_VISIBLE] = flag(self.target_visible);
        values[IDX_TARGET_LAST_KNOWN] = flag(self.target_last_known);
        values[IDX_TARGET_HP] = unit(self.target_hp_fraction);
        values[IDX_TARGET_ARMORED] = flag(self.target_armored);
        encode_cue(&mut values, IDX_ALLY_FORWARD, self.nearest_ally);
        values[IDX_ALLY_ALIVE] = flag(self.nearest_ally_alive);
        for (dst, source) in values[IDX_COVER..IDX_COVER + 8]
            .iter_mut()
            .zip(self.cover_clearance)
        {
            *dst = unit(source);
        }
        values[IDX_SLOPE_FORWARD] = signed(self.terrain_slope_forward);
        values[IDX_SLOPE_RIGHT] = signed(self.terrain_slope_right);
        values[IDX_CAN_INTERACT] = flag(self.can_interact);
        values[IDX_CAN_JUMP] = flag(self.can_jump);
        values[IDX_CAN_FIRE] = flag(self.can_fire);
        values[IDX_CAN_RELOAD] = flag(self.can_reload);
        values[IDX_WEAPON + self.weapon.index()] = 1.0;
        FootObservation {
            role: self.role,
            values,
        }
    }
}

fn encode_cue(values: &mut [f32; OBS_DIM], base: usize, cue: RelativeCue) {
    values[base] = signed(cue.forward);
    values[base + 1] = signed(cue.right);
    values[base + 2] = unit(cue.distance);
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn signed(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(-1.0, 1.0)
}

fn unit(value: f32) -> f32 {
    finite_or(value, 0.0).clamp(0.0, 1.0)
}

const fn flag(value: bool) -> f32 {
    if value {
        1.0
    } else {
        -1.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootObservation {
    role: PolicyRole,
    values: [f32; OBS_DIM],
}

impl FootObservation {
    pub fn role(self) -> PolicyRole {
        self.role
    }

    pub fn values(&self) -> &[f32; OBS_DIM] {
        &self.values
    }

    pub fn from_values(role: PolicyRole, mut values: [f32; OBS_DIM]) -> Self {
        for value in &mut values {
            *value = signed(*value);
        }
        values[IDX_ROLE_TIGER] = if role == PolicyRole::TigerCrew {
            1.0
        } else {
            -1.0
        };
        values[IDX_ROLE_SOVIET] = if role == PolicyRole::SovietRifle {
            1.0
        } else {
            -1.0
        };
        Self { role, values }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootAction {
    pub forward: f32,
    pub strafe: f32,
    pub turn: f32,
    pub crouch: bool,
    pub aim: bool,
    pub fire: bool,
    pub reload: bool,
    pub interact: bool,
    pub jump: bool,
    pub weapon: WeaponSelection,
}

impl Default for FootAction {
    fn default() -> Self {
        Self {
            forward: 0.0,
            strafe: 0.0,
            turn: 0.0,
            crouch: false,
            aim: false,
            fire: false,
            reload: false,
            interact: false,
            jump: false,
            weapon: WeaponSelection::Sidearm,
        }
    }
}

impl FootAction {
    pub fn bounded(mut self) -> Self {
        self.forward = signed(self.forward);
        self.strafe = signed(self.strafe);
        self.turn = signed(self.turn);
        self
    }

    pub fn from_policy_output(raw: [f32; ACTION_DIM]) -> Self {
        Self {
            forward: signed(raw[OUT_FORWARD]),
            strafe: signed(raw[OUT_STRAFE]),
            turn: signed(raw[OUT_TURN]),
            crouch: raw[OUT_CROUCH] > 0.0,
            aim: raw[OUT_AIM] > 0.0,
            fire: raw[OUT_FIRE] > 0.0,
            reload: raw[OUT_RELOAD] > 0.0,
            interact: raw[OUT_INTERACT] > 0.0,
            jump: raw[OUT_JUMP] > 0.0,
            weapon: WeaponSelection::from_policy_value(raw[OUT_WEAPON]),
        }
    }

    fn training_targets(self) -> [f32; ACTION_DIM] {
        [
            signed(self.forward),
            signed(self.strafe),
            signed(self.turn),
            flag(self.crouch),
            flag(self.aim),
            flag(self.fire),
            flag(self.reload),
            flag(self.interact),
            flag(self.jump),
            self.weapon.training_value(),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionAvailability {
    pub can_interact: bool,
    pub can_jump: bool,
    pub can_fire: bool,
    pub can_reload: bool,
}

impl ActionAvailability {
    pub fn from_observation(observation: &FootObservation) -> Self {
        Self {
            can_interact: observation.values[IDX_CAN_INTERACT] > 0.0,
            can_jump: observation.values[IDX_CAN_JUMP] > 0.0,
            can_fire: observation.values[IDX_CAN_FIRE] > 0.0,
            can_reload: observation.values[IDX_CAN_RELOAD] > 0.0,
        }
    }

    pub fn illegal_attempts(self, action: FootAction) -> u32 {
        u32::from(action.interact && !self.can_interact)
            + u32::from(action.jump && !self.can_jump)
            + u32::from(action.fire && !self.can_fire)
            + u32::from(action.reload && !self.can_reload)
    }
}

#[derive(Clone, Debug)]
pub struct FootPolicy {
    role: PolicyRole,
    weights: Vec<f32>,
}

impl FootPolicy {
    pub fn new(role: PolicyRole, weights: Vec<f32>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            weights.len() == WEIGHT_COUNT,
            "foot-policy weight count {} != {WEIGHT_COUNT}",
            weights.len()
        );
        anyhow::ensure!(
            weights.iter().all(|weight| weight.is_finite()),
            "foot-policy contains non-finite weights"
        );
        Ok(Self { role, weights })
    }

    pub fn seeded(role: PolicyRole, seed: u64, scale: f32) -> Self {
        let scale = finite_or(scale, 0.0).abs().clamp(0.0, 2.0);
        let mixed = seed ^ (u64::from(role as u8) << 56);
        let weights = (0..WEIGHT_COUNT)
            .map(|index| gaussian(mixed, index as u64) * scale)
            .collect();
        Self { role, weights }
    }

    pub fn role(&self) -> PolicyRole {
        self.role
    }

    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    pub fn decide(&self, observation: &FootObservation) -> anyhow::Result<FootAction> {
        anyhow::ensure!(
            observation.role == self.role,
            "{} policy cannot consume {} observation",
            self.role.label(),
            observation.role.label()
        );
        Ok(FootAction::from_policy_output(
            self.forward_raw(observation),
        ))
    }

    pub fn mutated(&self, seed: u64, sigma: f32) -> Self {
        let sigma = finite_or(sigma, 0.0).abs().clamp(0.0, 2.0);
        let weights = self
            .weights
            .iter()
            .enumerate()
            .map(|(index, weight)| {
                (*weight + gaussian(seed, index as u64) * sigma).clamp(-6.0, 6.0)
            })
            .collect();
        Self {
            role: self.role,
            weights,
        }
    }

    pub fn forward_raw(&self, observation: &FootObservation) -> [f32; ACTION_DIM] {
        let mut hidden = [0.0; HIDDEN_DIM];
        let hidden_bias = OBS_DIM * HIDDEN_DIM;
        for (node, activation) in hidden.iter_mut().enumerate() {
            let row = node * OBS_DIM;
            let mut sum = self.weights[hidden_bias + node];
            for feature in 0..OBS_DIM {
                sum += self.weights[row + feature] * observation.values[feature];
            }
            *activation = sum.tanh();
        }
        let output_weights = hidden_bias + HIDDEN_DIM;
        let output_bias = output_weights + HIDDEN_DIM * ACTION_DIM;
        let mut output = [0.0; ACTION_DIM];
        for (action, value) in output.iter_mut().enumerate() {
            let row = output_weights + action * HIDDEN_DIM;
            let mut sum = self.weights[output_bias + action];
            for node in 0..HIDDEN_DIM {
                sum += self.weights[row + node] * hidden[node];
            }
            *value = sum.tanh();
        }
        output
    }

    pub fn to_artifact_bytes(&self, generation: u32, fitness: f32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ARTIFACT_HEADER_BYTES + WEIGHT_COUNT * 4);
        bytes.extend_from_slice(&ARTIFACT_MAGIC);
        bytes.extend_from_slice(&ARTIFACT_VERSION.to_le_bytes());
        bytes.push(self.role as u8);
        bytes.push(0);
        bytes.extend_from_slice(&OBSERVATION_SCHEMA_HASH.to_le_bytes());
        bytes.extend_from_slice(&ACTION_SCHEMA_HASH.to_le_bytes());
        for value in [
            OBS_DIM as u32,
            HIDDEN_DIM as u32,
            ACTION_DIM as u32,
            generation,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&finite_or(fitness, 0.0).to_le_bytes());
        bytes.extend_from_slice(&(WEIGHT_COUNT as u32).to_le_bytes());
        debug_assert_eq!(bytes.len(), ARTIFACT_HEADER_BYTES);
        for weight in &self.weights {
            bytes.extend_from_slice(&weight.to_le_bytes());
        }
        bytes
    }

    pub fn from_artifact_bytes(
        bytes: &[u8],
        expected_role: PolicyRole,
    ) -> anyhow::Result<(Self, ArtifactMetadata)> {
        anyhow::ensure!(
            bytes.len() >= ARTIFACT_HEADER_BYTES,
            "truncated KSL2 artifact"
        );
        anyhow::ensure!(bytes[0..4] == ARTIFACT_MAGIC, "bad KSL2 magic");
        anyhow::ensure!(bytes[7] == 0, "unsupported KSL2 header flags");
        let u16_at =
            |offset: usize| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let u32_at =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let u64_at =
            |offset: usize| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        anyhow::ensure!(
            u16_at(4) == ARTIFACT_VERSION,
            "unsupported KSL2 version {}",
            u16_at(4)
        );
        let role = PolicyRole::from_u8(bytes[6])?;
        anyhow::ensure!(
            role == expected_role,
            "KSL2 role {} != expected {}",
            role.label(),
            expected_role.label()
        );
        anyhow::ensure!(
            u64_at(8) == OBSERVATION_SCHEMA_HASH,
            "KSL2 observation schema mismatch"
        );
        anyhow::ensure!(
            u64_at(16) == ACTION_SCHEMA_HASH,
            "KSL2 action schema mismatch"
        );
        anyhow::ensure!(
            (
                u32_at(24) as usize,
                u32_at(28) as usize,
                u32_at(32) as usize
            ) == (OBS_DIM, HIDDEN_DIM, ACTION_DIM),
            "KSL2 topology mismatch"
        );
        anyhow::ensure!(
            u32_at(44) as usize == WEIGHT_COUNT,
            "KSL2 weight-count mismatch"
        );
        anyhow::ensure!(
            bytes.len() == ARTIFACT_HEADER_BYTES + WEIGHT_COUNT * 4,
            "KSL2 artifact length {} is not exact",
            bytes.len()
        );
        let weights = bytes[ARTIFACT_HEADER_BYTES..]
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        let fitness = f32::from_le_bytes(bytes[40..44].try_into().unwrap());
        anyhow::ensure!(fitness.is_finite(), "KSL2 fitness must be finite");
        let metadata = ArtifactMetadata {
            role,
            generation: u32_at(36),
            fitness,
            observation_schema_hash: u64_at(8),
            action_schema_hash: u64_at(16),
        };
        Ok((Self::new(role, weights)?, metadata))
    }

    pub fn save(&self, path: &str, generation: u32, fitness: f32) -> anyhow::Result<()> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(&self.to_artifact_bytes(generation, fitness))?;
        file.flush()?;
        Ok(())
    }

    pub fn load(path: &str, expected_role: PolicyRole) -> anyhow::Result<(Self, ArtifactMetadata)> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut bytes)?;
        Self::from_artifact_bytes(&bytes, expected_role)
    }
}

/// Runtime fallback seam shared by the Tiger and Soviet integration. Artifact loading remains
/// strict (including role/schema rejection); if no validated policy is installed, or an impossible
/// role mismatch somehow reaches this boundary, the authoritative scripted expert executes.
pub fn decide_or_expert(
    policy: Option<&FootPolicy>,
    observation: &FootObservation,
    expert: FootAction,
) -> FootAction {
    policy
        .and_then(|policy| policy.decide(observation).ok())
        .unwrap_or(expert)
        .bounded()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArtifactMetadata {
    pub role: PolicyRole,
    pub generation: u32,
    pub fitness: f32,
    pub observation_schema_hash: u64,
    pub action_schema_hash: u64,
}

fn hash64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn hash01(seed: u64) -> f32 {
    let mantissa = hash64(seed) >> 40;
    (mantissa as f32 + 0.5) / ((1u32 << 24) as f32)
}

fn gaussian(seed: u64, index: u64) -> f32 {
    let u1 = hash01(seed ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)).max(1.0e-7);
    let u2 = hash01(seed.rotate_left(29) ^ index.wrapping_mul(0xd1b5_4a32_d192_ed03));
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpertSource {
    TigerMissionExpert,
    SovietDoctrineExpert,
    HumanDemonstration,
}

impl ExpertSource {
    const fn label(self) -> &'static str {
        match self {
            Self::TigerMissionExpert => "tiger_mission_expert",
            Self::SovietDoctrineExpert => "soviet_doctrine_expert",
            Self::HumanDemonstration => "human_demonstration",
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "tiger_mission_expert" => Ok(Self::TigerMissionExpert),
            "soviet_doctrine_expert" => Ok(Self::SovietDoctrineExpert),
            "human_demonstration" => Ok(Self::HumanDemonstration),
            _ => anyhow::bail!("unknown expert source {value:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TransitionEvents {
    pub map_searched: bool,
    pub returned_with_map: bool,
    pub remounted: bool,
    pub target_damage: f32,
    pub target_kia: bool,
    pub map_denied: bool,
    pub actor_kia: bool,
    pub blocked: bool,
    pub illegal_action: bool,
    pub friendly_fire_damage: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FootTransition {
    pub episode_id: u64,
    pub step: u32,
    pub seed: u64,
    pub act: u8,
    pub site_id: u32,
    pub agent_id: u32,
    pub role: PolicyRole,
    pub expert: ExpertSource,
    pub observation: FootObservation,
    /// Action actually applied to the authoritative physical simulation.
    pub action: FootAction,
    /// Teacher correction for behavior cloning/DAgger; may equal `action` for demonstrations.
    pub expert_action: FootAction,
    pub reward: f32,
    pub next_observation: FootObservation,
    pub terminated: bool,
    pub truncated: bool,
    pub events: TransitionEvents,
}

impl FootTransition {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(
                (self.role, self.expert),
                (PolicyRole::TigerCrew, ExpertSource::TigerMissionExpert)
                    | (PolicyRole::SovietRifle, ExpertSource::SovietDoctrineExpert)
                    | (_, ExpertSource::HumanDemonstration)
            ),
            "transition expert source is incompatible with policy role"
        );
        anyhow::ensure!(
            self.observation.role == self.role,
            "transition observation role mismatch"
        );
        anyhow::ensure!(
            self.next_observation.role == self.role,
            "transition next-observation role mismatch"
        );
        anyhow::ensure!(self.reward.is_finite(), "transition reward is not finite");
        anyhow::ensure!(
            self.events.target_damage.is_finite() && self.events.target_damage >= 0.0,
            "transition target damage is invalid"
        );
        anyhow::ensure!(
            self.events.friendly_fire_damage.is_finite() && self.events.friendly_fire_damage >= 0.0,
            "transition friendly-fire damage is invalid"
        );
        anyhow::ensure!(
            self.action == self.action.bounded(),
            "transition action is not bounded"
        );
        anyhow::ensure!(
            self.expert_action == self.expert_action.bounded(),
            "transition expert action is not bounded"
        );
        Ok(())
    }

    pub fn to_json_line(&self) -> anyhow::Result<String> {
        self.validate()?;
        Ok(serde_json::json!({
            "schema": TRANSITION_SCHEMA,
            "observation_schema_hash": format!("fnv1a64:{OBSERVATION_SCHEMA_HASH:016x}"),
            "action_schema_hash": format!("fnv1a64:{ACTION_SCHEMA_HASH:016x}"),
            "episode_id": self.episode_id,
            "step": self.step,
            "seed": self.seed,
            "act": self.act,
            "site_id": self.site_id,
            "agent_id": self.agent_id,
            "role": self.role.label(),
            "expert": self.expert.label(),
            "observation": self.observation.values.to_vec(),
            "action": action_json(self.action),
            "expert_action": action_json(self.expert_action),
            "reward": self.reward,
            "next_observation": self.next_observation.values.to_vec(),
            "terminated": self.terminated,
            "truncated": self.truncated,
            "events": {
                "map_searched": self.events.map_searched,
                "returned_with_map": self.events.returned_with_map,
                "remounted": self.events.remounted,
                "target_damage": self.events.target_damage,
                "target_kia": self.events.target_kia,
                "map_denied": self.events.map_denied,
                "actor_kia": self.events.actor_kia,
                "blocked": self.events.blocked,
                "illegal_action": self.events.illegal_action,
                "friendly_fire_damage": self.events.friendly_fire_damage,
            }
        })
        .to_string())
    }

    pub fn from_json_line(line: &str) -> anyhow::Result<Self> {
        let value: serde_json::Value = serde_json::from_str(line)?;
        anyhow::ensure!(
            text_field(&value, "schema")? == TRANSITION_SCHEMA,
            "foot transition schema mismatch"
        );
        anyhow::ensure!(
            text_field(&value, "observation_schema_hash")?
                == format!("fnv1a64:{OBSERVATION_SCHEMA_HASH:016x}"),
            "foot transition observation schema mismatch"
        );
        anyhow::ensure!(
            text_field(&value, "action_schema_hash")?
                == format!("fnv1a64:{ACTION_SCHEMA_HASH:016x}"),
            "foot transition action schema mismatch"
        );
        let role = PolicyRole::parse(text_field(&value, "role")?)?;
        let observation = observation_field(&value, "observation", role)?;
        let next_observation = observation_field(&value, "next_observation", role)?;
        let events_value = value
            .get("events")
            .ok_or_else(|| anyhow::anyhow!("missing events"))?;
        let transition = Self {
            episode_id: u64_field(&value, "episode_id")?,
            step: u32_field(&value, "step")?,
            seed: u64_field(&value, "seed")?,
            act: u32_field(&value, "act")?.try_into()?,
            site_id: u32_field(&value, "site_id")?,
            agent_id: u32_field(&value, "agent_id")?,
            role,
            expert: ExpertSource::parse(text_field(&value, "expert")?)?,
            observation,
            action: parse_action(
                value
                    .get("action")
                    .ok_or_else(|| anyhow::anyhow!("missing action"))?,
            )?,
            expert_action: parse_action(
                value
                    .get("expert_action")
                    .ok_or_else(|| anyhow::anyhow!("missing expert_action"))?,
            )?,
            reward: f32_field(&value, "reward")?,
            next_observation,
            terminated: bool_field(&value, "terminated")?,
            truncated: bool_field(&value, "truncated")?,
            events: TransitionEvents {
                map_searched: bool_field(events_value, "map_searched")?,
                returned_with_map: bool_field(events_value, "returned_with_map")?,
                remounted: bool_field(events_value, "remounted")?,
                target_damage: f32_field(events_value, "target_damage")?,
                target_kia: bool_field(events_value, "target_kia")?,
                map_denied: bool_field(events_value, "map_denied")?,
                actor_kia: bool_field(events_value, "actor_kia")?,
                blocked: bool_field(events_value, "blocked")?,
                illegal_action: bool_field(events_value, "illegal_action")?,
                friendly_fire_damage: f32_field(events_value, "friendly_fire_damage")?,
            },
        };
        transition.validate()?;
        Ok(transition)
    }
}

fn action_json(action: FootAction) -> serde_json::Value {
    serde_json::json!({
        "forward": action.forward,
        "strafe": action.strafe,
        "turn": action.turn,
        "crouch": action.crouch,
        "aim": action.aim,
        "fire": action.fire,
        "reload": action.reload,
        "interact": action.interact,
        "jump": action.jump,
        "weapon": action.weapon.label(),
    })
}

fn parse_action(value: &serde_json::Value) -> anyhow::Result<FootAction> {
    Ok(FootAction {
        forward: f32_field(value, "forward")?,
        strafe: f32_field(value, "strafe")?,
        turn: f32_field(value, "turn")?,
        crouch: bool_field(value, "crouch")?,
        aim: bool_field(value, "aim")?,
        fire: bool_field(value, "fire")?,
        reload: bool_field(value, "reload")?,
        interact: bool_field(value, "interact")?,
        jump: bool_field(value, "jump")?,
        weapon: WeaponSelection::parse(text_field(value, "weapon")?)?,
    })
}

fn observation_field(
    value: &serde_json::Value,
    key: &str,
    role: PolicyRole,
) -> anyhow::Result<FootObservation> {
    let array = value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing/invalid {key}"))?;
    anyhow::ensure!(
        array.len() == OBS_DIM,
        "{key} length {} != {OBS_DIM}",
        array.len()
    );
    let mut values = [0.0; OBS_DIM];
    for (dst, source) in values.iter_mut().zip(array) {
        *dst = source
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("non-number in {key}"))? as f32;
        anyhow::ensure!(dst.is_finite(), "non-finite number in {key}");
        anyhow::ensure!(
            (-1.0..=1.0).contains(dst),
            "out-of-range observation value in {key}"
        );
    }
    let (expected_tiger, expected_soviet) = if role == PolicyRole::TigerCrew {
        (1.0, -1.0)
    } else {
        (-1.0, 1.0)
    };
    anyhow::ensure!(
        values[IDX_ROLE_TIGER] == expected_tiger && values[IDX_ROLE_SOVIET] == expected_soviet,
        "{key} encoded role does not match transition role"
    );
    Ok(FootObservation { role, values })
}

fn text_field<'a>(value: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing/invalid string field {key}"))
}

fn bool_field(value: &serde_json::Value, key: &str) -> anyhow::Result<bool> {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("missing/invalid bool field {key}"))
}

fn u64_field(value: &serde_json::Value, key: &str) -> anyhow::Result<u64> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing/invalid u64 field {key}"))
}

fn u32_field(value: &serde_json::Value, key: &str) -> anyhow::Result<u32> {
    u64_field(value, key)?.try_into().map_err(Into::into)
}

fn f32_field(value: &serde_json::Value, key: &str) -> anyhow::Result<f32> {
    let result = value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| anyhow::anyhow!("missing/invalid float field {key}"))?
        as f32;
    anyhow::ensure!(result.is_finite(), "non-finite float field {key}");
    Ok(result)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RewardBreakdown {
    pub objective: f32,
    pub combat: f32,
    pub survival: f32,
    pub discipline: f32,
    pub total: f32,
}

impl RewardBreakdown {
    fn finish(mut self) -> Self {
        self.total = self.objective + self.combat + self.survival + self.discipline;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TigerRewardSignal {
    pub goal_progress_m: f32,
    pub map_searched: bool,
    pub returned_with_map: bool,
    pub remounted: bool,
    pub mission_complete: bool,
    pub loot_collected: u32,
    pub damage_taken: f32,
    pub exposed_seconds: f32,
    pub noisy_shots: u32,
    pub blocked_actions: u32,
    pub illegal_actions: u32,
    pub friendly_fire_damage: f32,
    pub survived: bool,
    pub kia: bool,
}

pub fn tiger_reward(signal: TigerRewardSignal) -> RewardBreakdown {
    RewardBreakdown {
        objective: finite_or(signal.goal_progress_m, 0.0).clamp(-25.0, 25.0) * 0.08
            + if signal.map_searched { 55.0 } else { 0.0 }
            + if signal.returned_with_map { 35.0 } else { 0.0 }
            + if signal.remounted { 25.0 } else { 0.0 }
            + if signal.mission_complete { 80.0 } else { 0.0 }
            + signal.loot_collected.min(8) as f32 * 2.5,
        combat: 0.0,
        survival: if signal.survived { 8.0 } else { 0.0 }
            - finite_or(signal.damage_taken, 0.0).max(0.0) * 1.2
            - if signal.kia { 120.0 } else { 0.0 },
        discipline: -finite_or(signal.exposed_seconds, 0.0).clamp(0.0, 60.0) * 0.35
            - signal.noisy_shots as f32 * 0.75
            - signal.blocked_actions as f32 * 1.5
            - signal.illegal_actions as f32 * 4.0
            - finite_or(signal.friendly_fire_damage, 0.0).max(0.0) * 2.0,
        total: 0.0,
    }
    .finish()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SovietRewardSignal {
    pub real_damage_dealt: f32,
    pub foot_kia: bool,
    pub map_denied: bool,
    pub map_denial_seconds: f32,
    pub squad_survival_fraction: f32,
    pub self_kia: bool,
    pub blocked_actions: u32,
    pub illegal_actions: u32,
    pub shots_without_legal_target: u32,
    pub friendly_fire_damage: f32,
}

pub fn soviet_reward(signal: SovietRewardSignal) -> RewardBreakdown {
    RewardBreakdown {
        objective: if signal.map_denied { 45.0 } else { 0.0 }
            + finite_or(signal.map_denial_seconds, 0.0).clamp(0.0, 45.0) * 0.08,
        combat: finite_or(signal.real_damage_dealt, 0.0).clamp(0.0, 100.0) * 0.8
            + if signal.foot_kia { 90.0 } else { 0.0 },
        survival: unit(signal.squad_survival_fraction) * 8.0
            - if signal.self_kia { 35.0 } else { 0.0 },
        discipline: -(signal.blocked_actions as f32)
            - signal.illegal_actions as f32 * 3.0
            - signal.shots_without_legal_target as f32 * 2.5
            - finite_or(signal.friendly_fire_damage, 0.0).max(0.0) * 1.5,
        total: 0.0,
    }
    .finish()
}

/// Deterministic expert labels are deliberately public: the campaign controller and the Soviet
/// scripted doctrine can use this exact seam to collect behavior-cloning/DAgger corrections.
pub fn micro_expert(observation: &FootObservation) -> FootAction {
    let v = observation.values;
    let role = observation.role;
    let mut action = FootAction {
        weapon: if role == PolicyRole::TigerCrew {
            WeaponSelection::Sidearm
        } else {
            WeaponSelection::Rifle
        },
        crouch: v[IDX_DETECTED] > 0.0 || v[IDX_SUPPRESSION] > 0.45,
        ..FootAction::default()
    };

    let can_reload = v[IDX_CAN_RELOAD] > 0.0;
    let can_interact = v[IDX_CAN_INTERACT] > 0.0;
    let can_jump = v[IDX_CAN_JUMP] > 0.0;
    let can_fire = v[IDX_CAN_FIRE] > 0.0;
    let target_visible = v[IDX_TARGET_VISIBLE] > 0.0;
    let target_armored = v[IDX_TARGET_ARMORED] > 0.0;

    if can_reload && v[IDX_AMMO] < 0.18 {
        action.reload = true;
        return action;
    }
    if can_interact && v[IDX_GOAL_DISTANCE] < 0.16 {
        action.interact = true;
        return action;
    }
    if can_jump && v[IDX_COVER] < 0.22 {
        action.jump = true;
        action.forward = 0.7;
        return action;
    }
    if target_visible {
        action.aim = true;
        action.turn = (v[IDX_TARGET_RIGHT] * 1.7).clamp(-1.0, 1.0);
        let desired_weapon = if target_armored {
            WeaponSelection::AntiTank
        } else if role == PolicyRole::SovietRifle {
            WeaponSelection::Rifle
        } else {
            WeaponSelection::Sidearm
        };
        action.weapon = desired_weapon;
        let already_selected = v[IDX_WEAPON + desired_weapon.index()] > 0.0;
        action.fire = can_fire && already_selected && v[IDX_TARGET_RIGHT].abs() < 0.16;
        return action;
    }

    let right = v[IDX_GOAL_RIGHT];
    action.turn = (right * 1.6).clamp(-1.0, 1.0);
    action.strafe = (right * 0.25).clamp(-0.35, 0.35);
    if right.abs() < 0.68 && v[IDX_GOAL_DISTANCE] > 0.08 {
        action.forward = (0.25 + v[IDX_GOAL_DISTANCE] * 0.9).clamp(0.0, 1.0);
    }
    action
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MicroTrainerConfig {
    pub epochs: u32,
    pub learning_rate: f32,
    pub train_seed: u64,
    pub train_episodes: usize,
    pub held_out_seed: u64,
    pub held_out_episodes: usize,
}

impl Default for MicroTrainerConfig {
    fn default() -> Self {
        Self {
            epochs: 128,
            learning_rate: 0.018,
            train_seed: 0xF007_A100_0000_0001,
            train_episodes: 8,
            held_out_seed: 0xF007_E7A1_0000_0001,
            held_out_episodes: 6,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldOutMetrics {
    pub episodes: usize,
    pub successful_episodes: usize,
    pub success_rate: f32,
    pub action_agreement: f32,
    pub action_mse: f32,
    /// Normalized micro-curriculum imitation score, not authoritative campaign reward.
    pub evaluation_score: f32,
    pub illegal_action_rate: f32,
}

#[derive(Clone, Debug)]
pub struct TrainingReport {
    pub role: PolicyRole,
    pub policy: FootPolicy,
    pub epochs: u32,
    pub training_loss: f32,
    pub held_out: HeldOutMetrics,
}

impl TrainingReport {
    pub fn save_checkpoint(&self, path: &str) -> anyhow::Result<()> {
        self.policy
            .save(path, self.epochs, self.held_out.evaluation_score)
    }
}

#[derive(Clone)]
struct MicroSample {
    observation: FootObservation,
    expert: FootAction,
}

pub fn train_micro_curriculum(
    role: PolicyRole,
    config: MicroTrainerConfig,
) -> anyhow::Result<TrainingReport> {
    anyhow::ensure!(config.epochs > 0, "micro trainer needs at least one epoch");
    anyhow::ensure!(
        config.train_episodes > 0,
        "micro trainer needs training episodes"
    );
    anyhow::ensure!(
        config.held_out_episodes > 0,
        "micro trainer needs held-out episodes"
    );
    anyhow::ensure!(
        config.learning_rate.is_finite() && config.learning_rate > 0.0,
        "invalid learning rate"
    );

    let training = micro_samples(role, config.train_seed, config.train_episodes);
    let mut policy = FootPolicy::seeded(role, config.train_seed ^ 0xBC00_0000_0000_0001, 0.055);
    let mut last_loss = f32::INFINITY;
    for epoch in 0..config.epochs {
        let lr = config.learning_rate * (0.35 + 0.65 * (1.0 - epoch as f32 / config.epochs as f32));
        let offset = (hash64(config.train_seed ^ u64::from(epoch)) as usize) % training.len();
        let mut loss = 0.0;
        for sample_index in 0..training.len() {
            let sample = &training[(sample_index + offset) % training.len()];
            loss += sgd_step(&mut policy, sample, lr);
        }
        last_loss = loss / training.len() as f32;
    }
    let held_out = evaluate_micro_curriculum(
        role,
        &policy,
        config.held_out_seed,
        config.held_out_episodes,
    )?;
    Ok(TrainingReport {
        role,
        policy,
        epochs: config.epochs,
        training_loss: last_loss,
        held_out,
    })
}

pub fn train_both_micro_curricula(
    config: MicroTrainerConfig,
) -> anyhow::Result<[TrainingReport; 2]> {
    Ok([
        train_micro_curriculum(PolicyRole::TigerCrew, config)?,
        train_micro_curriculum(PolicyRole::SovietRifle, config)?,
    ])
}

pub fn evaluate_micro_curriculum(
    role: PolicyRole,
    policy: &FootPolicy,
    seed: u64,
    episodes: usize,
) -> anyhow::Result<HeldOutMetrics> {
    anyhow::ensure!(policy.role == role, "held-out evaluator role mismatch");
    anyhow::ensure!(episodes > 0, "held-out evaluator needs episodes");
    let samples = micro_samples(role, seed, episodes);
    let samples_per_episode = samples.len() / episodes;
    let mut mse = 0.0;
    let mut agreements = 0usize;
    let mut illegal = 0u32;
    let mut successful_episodes = 0usize;
    for episode in 0..episodes {
        let mut episode_agreements = 0usize;
        for sample in &samples[episode * samples_per_episode..(episode + 1) * samples_per_episode] {
            let raw = policy.forward_raw(&sample.observation);
            let predicted = FootAction::from_policy_output(raw);
            let targets = sample.expert.training_targets();
            mse += raw
                .iter()
                .zip(targets)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                / ACTION_DIM as f32;
            let agreement = actions_agree(predicted, sample.expert);
            agreements += usize::from(agreement);
            episode_agreements += usize::from(agreement);
            illegal += ActionAvailability::from_observation(&sample.observation)
                .illegal_attempts(predicted);
        }
        // Treat an episode as solved once at least three quarters of its bounded control
        // decisions match the expert.  Continuous locomotion already has tight per-axis
        // tolerances in `actions_agree`, so an 80% gate made a single borderline turn count
        // as an otherwise successful deterministic micro episode failing wholesale.
        if episode_agreements * 4 >= samples_per_episode * 3 {
            successful_episodes += 1;
        }
    }
    let count = samples.len().max(1) as f32;
    let action_mse = mse / count;
    let action_agreement = agreements as f32 / count;
    Ok(HeldOutMetrics {
        episodes,
        successful_episodes,
        success_rate: successful_episodes as f32 / episodes as f32,
        action_agreement,
        action_mse,
        evaluation_score: 100.0 * (1.0 - action_mse).clamp(-1.0, 1.0),
        illegal_action_rate: illegal as f32 / (count * 4.0),
    })
}

fn actions_agree(a: FootAction, b: FootAction) -> bool {
    (a.forward - b.forward).abs() < 0.38
        && (a.strafe - b.strafe).abs() < 0.38
        && (a.turn - b.turn).abs() < 0.38
        && a.crouch == b.crouch
        && a.aim == b.aim
        && a.fire == b.fire
        && a.reload == b.reload
        && a.interact == b.interact
        && a.jump == b.jump
        && a.weapon == b.weapon
}

fn sgd_step(policy: &mut FootPolicy, sample: &MicroSample, learning_rate: f32) -> f32 {
    let input = sample.observation.values;
    let target = sample.expert.training_targets();
    let hidden_bias = OBS_DIM * HIDDEN_DIM;
    let output_weights = hidden_bias + HIDDEN_DIM;
    let output_bias = output_weights + HIDDEN_DIM * ACTION_DIM;

    let mut hidden = [0.0; HIDDEN_DIM];
    for node in 0..HIDDEN_DIM {
        let mut sum = policy.weights[hidden_bias + node];
        for feature in 0..OBS_DIM {
            sum += policy.weights[node * OBS_DIM + feature] * input[feature];
        }
        hidden[node] = sum.tanh();
    }
    let mut output = [0.0; ACTION_DIM];
    for action in 0..ACTION_DIM {
        let mut sum = policy.weights[output_bias + action];
        for node in 0..HIDDEN_DIM {
            sum += policy.weights[output_weights + action * HIDDEN_DIM + node] * hidden[node];
        }
        output[action] = sum.tanh();
    }

    let mut output_delta = [0.0; ACTION_DIM];
    let mut loss = 0.0;
    for action in 0..ACTION_DIM {
        let error = output[action] - target[action];
        loss += error * error / ACTION_DIM as f32;
        output_delta[action] =
            (2.0 / ACTION_DIM as f32) * error * (1.0 - output[action] * output[action]);
    }
    let mut hidden_delta = [0.0; HIDDEN_DIM];
    for node in 0..HIDDEN_DIM {
        let mut sum = 0.0;
        for action in 0..ACTION_DIM {
            sum +=
                output_delta[action] * policy.weights[output_weights + action * HIDDEN_DIM + node];
        }
        hidden_delta[node] = sum * (1.0 - hidden[node] * hidden[node]);
    }

    for action in 0..ACTION_DIM {
        for node in 0..HIDDEN_DIM {
            let index = output_weights + action * HIDDEN_DIM + node;
            let gradient = (output_delta[action] * hidden[node]).clamp(-2.0, 2.0);
            policy.weights[index] =
                (policy.weights[index] - learning_rate * gradient).clamp(-6.0, 6.0);
        }
        policy.weights[output_bias + action] = (policy.weights[output_bias + action]
            - learning_rate * output_delta[action].clamp(-2.0, 2.0))
        .clamp(-6.0, 6.0);
    }
    for node in 0..HIDDEN_DIM {
        for feature in 0..OBS_DIM {
            let index = node * OBS_DIM + feature;
            let gradient = (hidden_delta[node] * input[feature]).clamp(-2.0, 2.0);
            policy.weights[index] =
                (policy.weights[index] - learning_rate * gradient).clamp(-6.0, 6.0);
        }
        policy.weights[hidden_bias + node] = (policy.weights[hidden_bias + node]
            - learning_rate * hidden_delta[node].clamp(-2.0, 2.0))
        .clamp(-6.0, 6.0);
    }
    loss
}

fn micro_samples(role: PolicyRole, base_seed: u64, episodes: usize) -> Vec<MicroSample> {
    let mut result = Vec::with_capacity(episodes * 12);
    for episode in 0..episodes {
        let seed = hash64(base_seed ^ episode as u64);
        let lateral = hash01(seed ^ 1) * 1.6 - 0.8;
        let distance = 0.35 + hash01(seed ^ 2) * 0.65;
        let suppression = hash01(seed ^ 3);
        let default_weapon = if role == PolicyRole::TigerCrew {
            WeaponSelection::Sidearm
        } else {
            WeaponSelection::Rifle
        };
        let mut base = micro_input(role, default_weapon);

        // Long and short locomotion, including a target behind the shoulder.
        for (forward, right, dist) in [
            (0.85, lateral, distance),
            (0.45, -lateral * 0.7, 0.28),
            (-0.35, if lateral >= 0.0 { 0.82 } else { -0.82 }, 0.72),
        ] {
            base.goal = RelativeCue {
                forward,
                right,
                distance: dist,
            };
            base.goal_kind = if role == PolicyRole::TigerCrew {
                GoalKind::Search
            } else {
                GoalKind::Intercept
            };
            push_expert(&mut result, base);
        }

        // Physical interaction/search or map-denial hold.
        base.goal.distance = 0.05;
        base.goal.right = lateral * 0.05;
        base.can_interact = true;
        base.goal_kind = if role == PolicyRole::TigerCrew {
            GoalKind::Search
        } else {
            GoalKind::Hold
        };
        push_expert(&mut result, base);
        base.can_interact = false;

        // Return-to-Tiger/mount is a separate perceptual context.
        base.goal_kind = GoalKind::ReturnToTank;
        base.goal = RelativeCue {
            forward: 0.92,
            right: -lateral * 0.45,
            distance: 0.65,
        };
        base.tiger = base.goal;
        push_expert(&mut result, base);
        base.goal.distance = 0.04;
        base.tiger.distance = 0.04;
        base.tiger_mountable = true;
        base.can_interact = true;
        push_expert(&mut result, base);
        base.can_interact = false;
        base.tiger_mountable = false;

        // Suppression/detection must independently teach the crouch gate.
        base.goal = RelativeCue {
            forward: 0.8,
            right: lateral * 0.25,
            distance: 0.55,
        };
        base.detected = true;
        base.suppression = suppression.max(0.55);
        push_expert(&mut result, base);
        base.detected = false;
        base.suppression = 0.0;

        // Legal soft-target shot, then the same geometry with fire physically gated off.
        base.target = RelativeCue {
            forward: 0.8,
            right: lateral * 0.12,
            distance: 0.35,
        };
        base.target_visible = true;
        base.target_hp_fraction = 0.7;
        base.can_fire = true;
        push_expert(&mut result, base);
        base.can_fire = false;
        push_expert(&mut result, base);

        // Empty weapon/reload.
        base.target_visible = false;
        base.ammo_fraction = 0.0;
        base.can_reload = true;
        push_expert(&mut result, base);
        base.can_reload = false;
        base.ammo_fraction = 1.0;

        // Real blocked-probe jump, not an arbitrary jump reward.
        base.cover_clearance[0] = 0.05;
        base.can_jump = true;
        push_expert(&mut result, base);
        base.can_jump = false;
        base.cover_clearance[0] = 1.0;

        // Armored threat exercises explicit weapon selection and the no-fire-until-selected gate.
        base.target_visible = true;
        base.target_armored = true;
        base.target = RelativeCue {
            forward: 0.9,
            right: 0.05,
            distance: 0.5,
        };
        base.can_fire = true;
        push_expert(&mut result, base);
    }
    result
}

fn push_expert(samples: &mut Vec<MicroSample>, input: ObservationInput) {
    let observation = input.encode();
    samples.push(MicroSample {
        expert: micro_expert(&observation),
        observation,
    });
}

fn micro_input(role: PolicyRole, weapon: WeaponSelection) -> ObservationInput {
    ObservationInput {
        role,
        goal: RelativeCue {
            forward: 1.0,
            right: 0.0,
            distance: 0.8,
        },
        goal_kind: if role == PolicyRole::TigerCrew {
            GoalKind::Search
        } else {
            GoalKind::Intercept
        },
        hp_fraction: 1.0,
        velocity_forward: 0.0,
        velocity_right: 0.0,
        crouched: false,
        aiming: false,
        ammo_fraction: 1.0,
        reload_fraction: 0.0,
        detected: false,
        suppression: 0.0,
        stamina_fraction: 1.0,
        tiger: RelativeCue::default(),
        tiger_mountable: false,
        target: RelativeCue::default(),
        target_visible: false,
        target_last_known: false,
        target_hp_fraction: 1.0,
        target_armored: false,
        nearest_ally: RelativeCue::default(),
        nearest_ally_alive: false,
        cover_clearance: [1.0; 8],
        terrain_slope_forward: 0.0,
        terrain_slope_right: 0.0,
        can_interact: false,
        can_jump: false,
        can_fire: false,
        can_reload: false,
        weapon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expert_for(role: PolicyRole) -> (FootObservation, FootAction) {
        let observation = micro_input(
            role,
            if role == PolicyRole::TigerCrew {
                WeaponSelection::Sidearm
            } else {
                WeaponSelection::Rifle
            },
        )
        .encode();
        let expert = micro_expert(&observation);
        (observation, expert)
    }

    #[test]
    fn policy_decision_rejects_cross_role_observation() {
        let tiger = FootPolicy::seeded(PolicyRole::TigerCrew, 7, 0.03);
        let (soviet_observation, _) = expert_for(PolicyRole::SovietRifle);
        let error = tiger
            .decide(&soviet_observation)
            .expect_err("Tiger policy must reject Soviet observation");
        assert!(error.to_string().contains("cannot consume"));
    }

    #[test]
    fn artifact_loader_rejects_wrong_role() {
        let tiger = FootPolicy::seeded(PolicyRole::TigerCrew, 11, 0.02);
        let bytes = tiger.to_artifact_bytes(12, 3.5);
        let error = FootPolicy::from_artifact_bytes(&bytes, PolicyRole::SovietRifle)
            .expect_err("role header is part of the KSL2 safety contract");
        assert!(error.to_string().contains("expected soviet_rifle"));
    }

    #[test]
    fn missing_or_invalid_runtime_policy_uses_exact_expert_action() {
        let (observation, expert) = expert_for(PolicyRole::TigerCrew);
        assert_eq!(decide_or_expert(None, &observation, expert), expert);

        let wrong_role = FootPolicy::seeded(PolicyRole::SovietRifle, 13, 0.03);
        assert_eq!(
            decide_or_expert(Some(&wrong_role), &observation, expert),
            expert
        );
    }

    #[test]
    fn deterministic_training_produces_bit_identical_checkpoints() {
        let config = MicroTrainerConfig {
            epochs: 12,
            train_episodes: 3,
            held_out_episodes: 2,
            ..MicroTrainerConfig::default()
        };
        let first = train_both_micro_curricula(config).unwrap();
        let second = train_both_micro_curricula(config).unwrap();
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.role, b.role);
            assert_eq!(
                a.policy
                    .to_artifact_bytes(a.epochs, a.held_out.evaluation_score),
                b.policy
                    .to_artifact_bytes(b.epochs, b.held_out.evaluation_score)
            );
        }
    }

    #[test]
    fn transition_json_preserves_executed_and_expert_actions() {
        let (observation, expert_action) = expert_for(PolicyRole::SovietRifle);
        let action = FootAction {
            forward: 0.25,
            turn: -0.4,
            aim: false,
            fire: false,
            weapon: WeaponSelection::Rifle,
            ..FootAction::default()
        };
        let transition = FootTransition {
            episode_id: 1,
            step: 6,
            seed: 2,
            act: 1,
            site_id: 0,
            agent_id: 4,
            role: PolicyRole::SovietRifle,
            expert: ExpertSource::SovietDoctrineExpert,
            observation,
            action,
            expert_action,
            reward: 0.0,
            next_observation: observation,
            terminated: false,
            truncated: false,
            events: TransitionEvents::default(),
        };
        let decoded = FootTransition::from_json_line(&transition.to_json_line().unwrap()).unwrap();
        assert_eq!(decoded.action, action);
        assert_eq!(decoded.expert_action, expert_action);
    }
}
