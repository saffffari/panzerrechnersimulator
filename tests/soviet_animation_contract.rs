#[path = "../src/soviet_animation.rs"]
mod soviet_animation;

use std::collections::HashSet;

use soviet_animation::{
    SovietAnimationInput, SovietAnimator, SovietPose, SovietSequenceKind, SOVIET_POSES,
    SOVIET_POSE_COUNT, SOVIET_POSE_DIRECTORY, SOVIET_POSE_NAMES, SOVIET_POSE_PATHS,
    SOVIET_SEQUENCE_KINDS,
};

fn input(speed_m_s: f32, aiming: bool) -> SovietAnimationInput {
    SovietAnimationInput {
        speed_m_s,
        aiming,
        ..SovietAnimationInput::default()
    }
}

#[test]
fn transitions_follow_speed_aim_flinch_and_alive_authority() {
    let mut animator = SovietAnimator::new(7);

    animator.advance(0.0, input(0.0, false));
    assert_eq!(animator.sequence(), SovietSequenceKind::Idle);

    animator.advance(0.0, input(1.0, false));
    assert_eq!(animator.sequence(), SovietSequenceKind::Walk);

    animator.advance(0.0, input(1.7, false));
    assert_eq!(animator.sequence(), SovietSequenceKind::Run);

    animator.advance(0.0, input(1.7, true));
    assert_eq!(animator.sequence(), SovietSequenceKind::Aim);
    assert_eq!(animator.pose(), SovietPose::Aim00);

    let mut flinch = input(0.0, false);
    flinch.flinch = true;
    assert_eq!(animator.advance(0.016, flinch), SovietPose::Death00);
    assert!(animator.is_flinching());
    animator.advance(0.070, flinch);
    assert!(
        !animator.is_flinching(),
        "a held input must not retrigger flinch"
    );
    assert_eq!(animator.sequence(), SovietSequenceKind::Idle);

    let mut dead = SovietAnimationInput::default();
    dead.alive = false;
    assert_eq!(animator.advance(0.0, dead), SovietPose::Death00);
    assert_eq!(animator.sequence(), SovietSequenceKind::Death);
    assert!(animator.is_dead());
}

#[test]
fn death_is_terminal_holds_the_rest_pose_and_never_loops() {
    let mut animator = SovietAnimator::new(11);
    assert!(animator.trigger_death());
    assert!(!animator.trigger_death());
    assert_eq!(animator.pose(), SovietPose::Death00);

    animator.advance(0.081, SovietAnimationInput::default());
    assert_eq!(animator.pose(), SovietPose::Death01);
    animator.advance(0.121, SovietAnimationInput::default());
    assert_eq!(animator.pose(), SovietPose::Death02);
    animator.advance(20.0, SovietAnimationInput::default());
    assert_eq!(animator.pose(), SovietPose::Death03);

    let held_time = animator.sequence_time_s();
    animator.advance(1000.0, input(2.0, true));
    assert_eq!(animator.sequence(), SovietSequenceKind::Death);
    assert_eq!(animator.pose(), SovietPose::Death03);
    assert_eq!(animator.sequence_time_s(), held_time);

    animator.revive();
    assert!(!animator.is_dead());
    assert_eq!(animator.sequence(), SovietSequenceKind::Idle);
}

#[test]
fn fire_uses_authored_timing_then_returns_to_aim() {
    let mut animator = SovietAnimator::new(3);
    animator.advance(0.0, input(0.0, true));
    assert!(animator.trigger_fire());
    assert!(
        !animator.trigger_fire(),
        "an in-flight shot must not restart"
    );
    assert_eq!(animator.pose(), SovietPose::Aim00);

    animator.advance(0.055, input(0.0, true));
    assert_eq!(animator.pose(), SovietPose::Fire01);
    animator.advance(0.070, input(0.0, true));
    assert_eq!(animator.pose(), SovietPose::Fire02);
    animator.advance(0.111, input(0.0, true));
    assert_eq!(animator.sequence(), SovietSequenceKind::Aim);
    assert_eq!(animator.pose(), SovietPose::Aim00);
}

#[test]
fn gait_phase_is_deterministic_per_id_but_geometry_is_always_shared() {
    let mut first = SovietAnimator::new(42);
    let mut replay = SovietAnimator::new(42);
    assert_eq!(first.soldier_id(), 42);
    first.advance(0.0, input(1.0, false));
    replay.advance(0.0, input(1.0, false));
    assert_eq!(
        first.sequence_time_s().to_bits(),
        replay.sequence_time_s().to_bits()
    );
    assert_eq!(first.pose(), replay.pose());

    let offsets: HashSet<u32> = (0..24)
        .map(|id| {
            let mut animator = SovietAnimator::new(id);
            animator.advance(0.0, input(1.0, false));
            animator.sequence_time_s().to_bits()
        })
        .collect();
    assert!(
        offsets.len() >= 20,
        "soldier IDs should visibly dephase the shared gait"
    );

    for pose in SOVIET_POSES {
        assert_eq!(pose.path(), SOVIET_POSE_PATHS[pose.index()]);
    }
    assert_eq!(first.pose_index(), first.pose().index());
    assert_eq!(first.pose_name(), first.pose().name());
    assert_eq!(first.pose_path(), first.pose().path());
}

#[test]
fn rust_pose_and_sequence_contract_matches_the_python_bake_recipe() {
    let recipe = include_str!("../tools/soviet_character_recipe.py");
    assert_eq!(SOVIET_POSE_COUNT, 17);
    assert_eq!(
        recipe.matches("    PoseFrame(\"").count(),
        SOVIET_POSE_COUNT,
        "the Rust table and Python recipe must add/remove poses together"
    );
    assert_eq!(
        SOVIET_SEQUENCE_KINDS.map(SovietSequenceKind::name),
        ["idle", "walk", "run", "aim", "fire", "death"]
    );
    assert!(recipe.contains("pose_directory: str = \"baked\""));
    assert!(recipe.contains("f\"{pose_directory}/soviet_ww2_soldier_{frame.name}.glb\""));

    // The recipe deliberately uses comprehensions for the regular gaits and literals for the
    // one-shots.  Keep these source-side timing declarations tied to the Rust table below.
    for timing_declaration in [
        "((\"idle_00\", 0.55), (\"idle_01\", 0.55))",
        "tuple((f\"walk_{index:02d}\", 0.135) for index in range(4))",
        "tuple((f\"run_{index:02d}\", 0.090) for index in range(4))",
        "((\"aim_00\", 0.18),)",
        "((\"aim_00\", 0.055), (\"fire_01\", 0.070), (\"fire_02\", 0.110))",
        "(\"death_00\", 0.080)",
        "(\"death_01\", 0.120)",
        "(\"death_02\", 0.180)",
        "(\"death_03\", 1.000)",
    ] {
        assert!(
            recipe.contains(timing_declaration),
            "Python bake timing drifted: {timing_declaration}"
        );
    }

    for (index, pose) in SOVIET_POSES.into_iter().enumerate() {
        assert_eq!(pose.index(), index);
        assert_eq!(pose.name(), SOVIET_POSE_NAMES[index]);
        assert!(recipe.contains(&format!("PoseFrame(\"{}\"", pose.name())));
        assert_eq!(
            pose.path(),
            format!(
                "{SOVIET_POSE_DIRECTORY}/soviet_ww2_soldier_{}.glb",
                pose.name()
            )
        );
    }

    let exact_sequences: &[(SovietSequenceKind, bool, &[(SovietPose, f32)])] = &[
        (
            SovietSequenceKind::Idle,
            true,
            &[(SovietPose::Idle00, 0.55), (SovietPose::Idle01, 0.55)],
        ),
        (
            SovietSequenceKind::Walk,
            true,
            &[
                (SovietPose::Walk00, 0.135),
                (SovietPose::Walk01, 0.135),
                (SovietPose::Walk02, 0.135),
                (SovietPose::Walk03, 0.135),
            ],
        ),
        (
            SovietSequenceKind::Run,
            true,
            &[
                (SovietPose::Run00, 0.090),
                (SovietPose::Run01, 0.090),
                (SovietPose::Run02, 0.090),
                (SovietPose::Run03, 0.090),
            ],
        ),
        (SovietSequenceKind::Aim, true, &[(SovietPose::Aim00, 0.18)]),
        (
            SovietSequenceKind::Fire,
            false,
            &[
                (SovietPose::Aim00, 0.055),
                (SovietPose::Fire01, 0.070),
                (SovietPose::Fire02, 0.110),
            ],
        ),
        (
            SovietSequenceKind::Death,
            false,
            &[
                (SovietPose::Death00, 0.080),
                (SovietPose::Death01, 0.120),
                (SovietPose::Death02, 0.180),
                (SovietPose::Death03, 1.000),
            ],
        ),
    ];

    for (kind, loops, expected) in exact_sequences {
        assert_eq!(
            kind.loops(),
            *loops,
            "{} loop contract drifted",
            kind.name()
        );
        let actual: Vec<_> = kind
            .steps()
            .iter()
            .map(|step| (step.pose, step.duration_s))
            .collect();
        assert_eq!(actual, *expected, "{} frame timing drifted", kind.name());
    }

    assert!(recipe.contains("if ANIMATION_SEQUENCES[\"death\"][\"loop\"]:"));
    assert!(recipe.contains("if ANIMATION_SEQUENCES[\"fire\"][\"loop\"]:"));
}
