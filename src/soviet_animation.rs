//! Deterministic pose-frame animation for the shared Soviet infantry asset.
//!
//! The current runtime GLB loader does not play skeletal animation.  The Blender pipeline instead
//! exports the exact 17 evaluated poses below as static GLBs.  Every Soviet soldier selects from
//! this one shared table; `soldier_id` affects only cyclic timing and can never select alternate
//! geometry, materials, or textures.

/// Directory produced by `tools/bake_soviet_infantry.py`.
pub const SOVIET_POSE_DIRECTORY: &str = "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked";

/// Exact number of static poses declared by `tools/soviet_character_recipe.py`.
pub const SOVIET_POSE_COUNT: usize = 17;

/// The Blender/glTF bake samples every authored sequence on this fixed grid. Runtime pose holds
/// are exact integer frame counts divided by this rate, never separately hand-tuned seconds.
pub const SOVIET_ANIMATION_FPS: f32 = 30.0;

const fn hold_frames(frames: u16) -> f32 {
    frames as f32 / SOVIET_ANIMATION_FPS
}

/// Walking begins above this measured ground speed.
pub const WALK_THRESHOLD_M_S: f32 = 0.12;
/// The existing Soviet alarmed advance is 1.7 m/s, so it selects the authored run cycle.
pub const RUN_THRESHOLD_M_S: f32 = 1.55;

/// A non-lethal hit reuses the authored hit pose for the same two-frame hold as `death_00`.
pub const FLINCH_HOLD_S: f32 = hold_frames(2);

/// One of the 17 static, shared runtime meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SovietPose {
    Idle00 = 0,
    Idle01 = 1,
    Walk00 = 2,
    Walk01 = 3,
    Walk02 = 4,
    Walk03 = 5,
    Run00 = 6,
    Run01 = 7,
    Run02 = 8,
    Run03 = 9,
    Aim00 = 10,
    Fire01 = 11,
    Fire02 = 12,
    Death00 = 13,
    Death01 = 14,
    Death02 = 15,
    Death03 = 16,
}

pub const SOVIET_POSES: [SovietPose; SOVIET_POSE_COUNT] = [
    SovietPose::Idle00,
    SovietPose::Idle01,
    SovietPose::Walk00,
    SovietPose::Walk01,
    SovietPose::Walk02,
    SovietPose::Walk03,
    SovietPose::Run00,
    SovietPose::Run01,
    SovietPose::Run02,
    SovietPose::Run03,
    SovietPose::Aim00,
    SovietPose::Fire01,
    SovietPose::Fire02,
    SovietPose::Death00,
    SovietPose::Death01,
    SovietPose::Death02,
    SovietPose::Death03,
];

pub const SOVIET_POSE_NAMES: [&str; SOVIET_POSE_COUNT] = [
    "idle_00", "idle_01", "walk_00", "walk_01", "walk_02", "walk_03", "run_00", "run_01", "run_02",
    "run_03", "aim_00", "fire_01", "fire_02", "death_00", "death_01", "death_02", "death_03",
];

/// Literal paths keep asset-catalog and packaging audits independent of runtime string assembly.
pub const SOVIET_POSE_PATHS: [&str; SOVIET_POSE_COUNT] = [
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_idle_00.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_idle_01.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_walk_00.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_walk_01.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_walk_02.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_walk_03.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_run_00.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_run_01.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_run_02.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_run_03.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_aim_00.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_fire_01.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_fire_02.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_death_00.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_death_01.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_death_02.glb",
    "assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/baked/soviet_ww2_soldier_death_03.glb",
];

impl SovietPose {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        SOVIET_POSE_NAMES[self.index()]
    }

    pub const fn path(self) -> &'static str {
        SOVIET_POSE_PATHS[self.index()]
    }
}

/// The six sequences authored by the bake recipe.  Flinch is a short pose override rather than a
/// seventh sequence, which keeps this runtime contract exactly aligned with the source recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SovietSequenceKind {
    Idle = 0,
    Walk = 1,
    Run = 2,
    Aim = 3,
    Fire = 4,
    Death = 5,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SovietPoseStep {
    pub pose: SovietPose,
    pub duration_s: f32,
}

const IDLE_STEPS: [SovietPoseStep; 2] = [
    SovietPoseStep {
        pose: SovietPose::Idle00,
        duration_s: hold_frames(17),
    },
    SovietPoseStep {
        pose: SovietPose::Idle01,
        duration_s: hold_frames(17),
    },
];

const WALK_STEPS: [SovietPoseStep; 4] = [
    SovietPoseStep {
        pose: SovietPose::Walk00,
        duration_s: hold_frames(4),
    },
    SovietPoseStep {
        pose: SovietPose::Walk01,
        duration_s: hold_frames(4),
    },
    SovietPoseStep {
        pose: SovietPose::Walk02,
        duration_s: hold_frames(4),
    },
    SovietPoseStep {
        pose: SovietPose::Walk03,
        duration_s: hold_frames(4),
    },
];

const RUN_STEPS: [SovietPoseStep; 4] = [
    SovietPoseStep {
        pose: SovietPose::Run00,
        duration_s: hold_frames(3),
    },
    SovietPoseStep {
        pose: SovietPose::Run01,
        duration_s: hold_frames(3),
    },
    SovietPoseStep {
        pose: SovietPose::Run02,
        duration_s: hold_frames(3),
    },
    SovietPoseStep {
        pose: SovietPose::Run03,
        duration_s: hold_frames(3),
    },
];

const AIM_STEPS: [SovietPoseStep; 1] = [SovietPoseStep {
    pose: SovietPose::Aim00,
    duration_s: hold_frames(5),
}];

const FIRE_STEPS: [SovietPoseStep; 3] = [
    SovietPoseStep {
        pose: SovietPose::Aim00,
        duration_s: hold_frames(2),
    },
    SovietPoseStep {
        pose: SovietPose::Fire01,
        duration_s: hold_frames(2),
    },
    SovietPoseStep {
        pose: SovietPose::Fire02,
        duration_s: hold_frames(3),
    },
];

const DEATH_STEPS: [SovietPoseStep; 4] = [
    SovietPoseStep {
        pose: SovietPose::Death00,
        duration_s: hold_frames(2),
    },
    SovietPoseStep {
        pose: SovietPose::Death01,
        duration_s: hold_frames(4),
    },
    SovietPoseStep {
        pose: SovietPose::Death02,
        duration_s: hold_frames(5),
    },
    SovietPoseStep {
        pose: SovietPose::Death03,
        duration_s: hold_frames(30),
    },
];

impl SovietSequenceKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walk => "walk",
            Self::Run => "run",
            Self::Aim => "aim",
            Self::Fire => "fire",
            Self::Death => "death",
        }
    }

    pub const fn loops(self) -> bool {
        matches!(self, Self::Idle | Self::Walk | Self::Run | Self::Aim)
    }

    pub const fn steps(self) -> &'static [SovietPoseStep] {
        match self {
            Self::Idle => &IDLE_STEPS,
            Self::Walk => &WALK_STEPS,
            Self::Run => &RUN_STEPS,
            Self::Aim => &AIM_STEPS,
            Self::Fire => &FIRE_STEPS,
            Self::Death => &DEATH_STEPS,
        }
    }

    pub fn duration_s(self) -> f32 {
        self.steps().iter().map(|frame| frame.duration_s).sum()
    }
}

pub const SOVIET_SEQUENCE_KINDS: [SovietSequenceKind; 6] = [
    SovietSequenceKind::Idle,
    SovietSequenceKind::Walk,
    SovietSequenceKind::Run,
    SovietSequenceKind::Aim,
    SovietSequenceKind::Fire,
    SovietSequenceKind::Death,
];

/// Authoritative physical state sampled by the animator once per simulation update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SovietAnimationInput {
    pub speed_m_s: f32,
    pub aiming: bool,
    pub alive: bool,
    pub flinch: bool,
}

impl Default for SovietAnimationInput {
    fn default() -> Self {
        Self {
            speed_m_s: 0.0,
            aiming: false,
            alive: true,
            flinch: false,
        }
    }
}

/// Lightweight per-soldier timing state.  Static geometry remains shared through `SovietPose`.
#[derive(Clone, Debug)]
pub struct SovietAnimator {
    soldier_id: u32,
    sequence: SovietSequenceKind,
    sequence_time_s: f32,
    flinch_time_s: f32,
    flinch_latched: bool,
    death_latched: bool,
}

impl SovietAnimator {
    pub fn new(soldier_id: u32) -> Self {
        let mut animator = Self {
            soldier_id,
            sequence: SovietSequenceKind::Idle,
            sequence_time_s: 0.0,
            flinch_time_s: 0.0,
            flinch_latched: false,
            death_latched: false,
        };
        animator.sequence_time_s = animator.phase_offset_s(SovietSequenceKind::Idle);
        animator
    }

    pub const fn soldier_id(&self) -> u32 {
        self.soldier_id
    }

    pub const fn sequence(&self) -> SovietSequenceKind {
        self.sequence
    }

    pub const fn sequence_time_s(&self) -> f32 {
        self.sequence_time_s
    }

    pub const fn is_dead(&self) -> bool {
        self.death_latched
    }

    pub fn is_flinching(&self) -> bool {
        !self.death_latched && self.flinch_time_s > 0.0
    }

    /// Start one authored rifle-shot sequence.  Repeated triggers do not restart an in-flight shot.
    pub fn trigger_fire(&mut self) -> bool {
        if self.death_latched
            || self.flinch_time_s > 0.0
            || self.sequence == SovietSequenceKind::Fire
        {
            return false;
        }
        self.sequence = SovietSequenceKind::Fire;
        self.sequence_time_s = 0.0;
        true
    }

    /// Start terminal collapse.  Once latched, only `revive` may leave the death sequence.
    pub fn trigger_death(&mut self) -> bool {
        if self.death_latched {
            return false;
        }
        self.death_latched = true;
        self.flinch_time_s = 0.0;
        self.sequence = SovietSequenceKind::Death;
        self.sequence_time_s = 0.0;
        true
    }

    /// Reset a pooled soldier to a living, deterministically dephased idle.
    pub fn revive(&mut self) {
        *self = Self::new(self.soldier_id);
    }

    /// Advance from authoritative movement/combat state and return the selected shared pose.
    pub fn advance(&mut self, dt_s: f32, input: SovietAnimationInput) -> SovietPose {
        let mut remaining_s = finite_non_negative(dt_s);

        if !input.alive {
            self.trigger_death();
        }
        if self.death_latched {
            let end = SovietSequenceKind::Death.duration_s();
            self.sequence_time_s = (self.sequence_time_s + remaining_s).min(end);
            return self.pose();
        }

        // `flinch` is normally a timer in Soldier.  Edge-latching prevents a multi-frame true input
        // from restarting the 80 ms reaction forever.
        if input.flinch && !self.flinch_latched {
            self.flinch_time_s = FLINCH_HOLD_S;
            self.enter_base_sequence(input);
        }
        self.flinch_latched = input.flinch;

        if self.flinch_time_s > 0.0 {
            if remaining_s < self.flinch_time_s {
                self.flinch_time_s -= remaining_s;
                return SovietPose::Death00;
            }
            remaining_s -= self.flinch_time_s;
            self.flinch_time_s = 0.0;
        }

        if self.sequence == SovietSequenceKind::Fire {
            let fire_end = SovietSequenceKind::Fire.duration_s();
            let to_end = (fire_end - self.sequence_time_s).max(0.0);
            if remaining_s < to_end {
                self.sequence_time_s += remaining_s;
                return self.pose();
            }
            remaining_s -= to_end;
            self.enter_base_sequence(input);
        } else {
            let desired = base_sequence(input);
            if self.sequence != desired {
                self.enter_sequence(desired);
            }
        }

        self.sequence_time_s += remaining_s;
        if self.sequence.loops() {
            self.sequence_time_s %= self.sequence.duration_s();
        }
        self.pose()
    }

    pub fn pose(&self) -> SovietPose {
        if self.is_flinching() {
            return SovietPose::Death00;
        }
        pose_at(self.sequence, self.sequence_time_s)
    }

    pub fn pose_index(&self) -> usize {
        self.pose().index()
    }

    pub fn pose_name(&self) -> &'static str {
        self.pose().name()
    }

    pub fn pose_path(&self) -> &'static str {
        self.pose().path()
    }

    /// Public for deterministic replay/contract tests.  This changes only playback time.
    pub fn phase_offset_s(&self, sequence: SovietSequenceKind) -> f32 {
        if !sequence.loops() {
            return 0.0;
        }
        deterministic_phase_unit(self.soldier_id) * sequence.duration_s()
    }

    fn enter_base_sequence(&mut self, input: SovietAnimationInput) {
        self.enter_sequence(base_sequence(input));
    }

    fn enter_sequence(&mut self, sequence: SovietSequenceKind) {
        self.sequence = sequence;
        self.sequence_time_s = self.phase_offset_s(sequence);
    }
}

fn base_sequence(input: SovietAnimationInput) -> SovietSequenceKind {
    if input.aiming {
        return SovietSequenceKind::Aim;
    }
    let speed = finite_non_negative(input.speed_m_s.abs());
    if speed >= RUN_THRESHOLD_M_S {
        SovietSequenceKind::Run
    } else if speed >= WALK_THRESHOLD_M_S {
        SovietSequenceKind::Walk
    } else {
        SovietSequenceKind::Idle
    }
}

fn pose_at(sequence: SovietSequenceKind, time_s: f32) -> SovietPose {
    let steps = sequence.steps();
    let duration_s = sequence.duration_s();
    let mut local_s = finite_non_negative(time_s);
    if sequence.loops() {
        local_s %= duration_s;
    } else if local_s >= duration_s {
        return steps[steps.len() - 1].pose;
    }

    let mut boundary_s = 0.0;
    for step in steps {
        boundary_s += step.duration_s;
        if local_s < boundary_s {
            return step.pose;
        }
    }
    steps[steps.len() - 1].pose
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// A fixed integer mix keeps phase stable across machines, runs, and frame partitions.
pub fn deterministic_phase_unit(soldier_id: u32) -> f32 {
    let mut mixed = soldier_id.wrapping_add(0x9e37_79b9);
    mixed ^= mixed >> 16;
    mixed = mixed.wrapping_mul(0x7feb_352d);
    mixed ^= mixed >> 15;
    mixed = mixed.wrapping_mul(0x846c_a68b);
    mixed ^= mixed >> 16;
    (mixed & 0x00ff_ffff) as f32 / 16_777_216.0
}
