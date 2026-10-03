#[allow(dead_code)]
#[path = "../src/resource.rs"]
mod resource;

#[allow(dead_code)]
#[path = "../src/foot_ai.rs"]
mod foot_ai;

use foot_ai::*;

fn observation(role: PolicyRole) -> FootObservation {
    ObservationInput {
        role,
        goal: RelativeCue {
            forward: 0.8,
            right: 0.2,
            distance: 0.6,
        },
        goal_kind: if role == PolicyRole::TigerCrew {
            GoalKind::Search
        } else {
            GoalKind::Intercept
        },
        hp_fraction: 0.9,
        velocity_forward: 0.2,
        velocity_right: -0.1,
        crouched: false,
        aiming: false,
        ammo_fraction: 0.7,
        reload_fraction: 0.0,
        detected: false,
        suppression: 0.1,
        stamina_fraction: 0.8,
        tiger: RelativeCue {
            forward: -0.7,
            right: 0.1,
            distance: 0.5,
        },
        tiger_mountable: false,
        target: RelativeCue {
            forward: 0.7,
            right: -0.05,
            distance: 0.3,
        },
        target_visible: true,
        target_last_known: true,
        target_hp_fraction: 0.8,
        target_armored: false,
        nearest_ally: RelativeCue {
            forward: 0.1,
            right: 0.4,
            distance: 0.2,
        },
        nearest_ally_alive: true,
        cover_clearance: [0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2],
        terrain_slope_forward: 0.1,
        terrain_slope_right: -0.1,
        can_interact: false,
        can_jump: false,
        can_fire: true,
        can_reload: false,
        weapon: if role == PolicyRole::TigerCrew {
            WeaponSelection::Sidearm
        } else {
            WeaponSelection::Rifle
        },
    }
    .encode()
}

#[test]
fn observation_is_egocentric_finite_bounded_and_role_explicit() {
    let input = ObservationInput {
        role: PolicyRole::TigerCrew,
        goal: RelativeCue {
            forward: f32::INFINITY,
            right: -3.0,
            distance: 8.0,
        },
        ..base_input(PolicyRole::TigerCrew)
    };
    let encoded = input.encode();
    assert_eq!(encoded.role(), PolicyRole::TigerCrew);
    assert_eq!(encoded.values().len(), OBS_DIM);
    assert!(encoded
        .values()
        .iter()
        .all(|value| value.is_finite() && (-1.0..=1.0).contains(value)));
    assert_ne!(
        observation(PolicyRole::TigerCrew).values(),
        observation(PolicyRole::SovietRifle).values()
    );
}

#[test]
fn actions_are_bounded_and_cover_every_physical_control() {
    let bounded = FootAction {
        forward: 4.0,
        strafe: -9.0,
        turn: f32::NAN,
        crouch: true,
        aim: true,
        fire: true,
        reload: true,
        interact: true,
        jump: true,
        weapon: WeaponSelection::AntiTank,
    }
    .bounded();
    assert_eq!(bounded.forward, 1.0);
    assert_eq!(bounded.strafe, -1.0);
    assert_eq!(bounded.turn, 0.0);
    assert!(bounded.crouch && bounded.aim && bounded.fire && bounded.reload);
    assert!(bounded.interact && bounded.jump);
    assert_eq!(bounded.weapon, WeaponSelection::AntiTank);

    let unavailable = ActionAvailability {
        can_interact: false,
        can_jump: false,
        can_fire: false,
        can_reload: false,
    };
    assert_eq!(unavailable.illegal_attempts(bounded), 4);
}

#[test]
fn policy_seed_mutation_and_inference_are_bit_deterministic() {
    let a = FootPolicy::seeded(PolicyRole::TigerCrew, 42, 0.1);
    let b = FootPolicy::seeded(PolicyRole::TigerCrew, 42, 0.1);
    assert_eq!(a.weights(), b.weights());
    assert_eq!(
        a.forward_raw(&observation(PolicyRole::TigerCrew)),
        b.forward_raw(&observation(PolicyRole::TigerCrew))
    );

    let mutation_a = a.mutated(99, 0.12);
    let mutation_b = b.mutated(99, 0.12);
    assert_eq!(mutation_a.weights(), mutation_b.weights());
    assert_ne!(mutation_a.weights(), a.weights());
    assert!(mutation_a
        .weights()
        .iter()
        .all(|weight| weight.is_finite() && (-6.0..=6.0).contains(weight)));
    assert!(a.decide(&observation(PolicyRole::SovietRifle)).is_err());

    let sanitized = FootPolicy::seeded(PolicyRole::TigerCrew, 42, f32::NAN);
    assert!(sanitized.weights().iter().all(|weight| *weight == 0.0));
}

#[test]
fn role_safe_ksl2_roundtrip_rejects_schema_role_and_length_mismatches() {
    let tiger = FootPolicy::seeded(PolicyRole::TigerCrew, 7, 0.08);
    let bytes = tiger.to_artifact_bytes(31, 412.5);
    assert_eq!(&bytes[0..4], b"KSL2");
    assert_eq!(bytes[6], PolicyRole::TigerCrew as u8);
    assert_eq!(
        u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        OBSERVATION_SCHEMA_HASH
    );
    assert_eq!(
        u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        ACTION_SCHEMA_HASH
    );
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 51);
    assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 32);
    assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 10);
    let (loaded, metadata) =
        FootPolicy::from_artifact_bytes(&bytes, PolicyRole::TigerCrew).unwrap();
    assert_eq!(loaded.weights(), tiger.weights());
    assert_eq!(metadata.role, PolicyRole::TigerCrew);
    assert_eq!(metadata.generation, 31);
    assert_eq!(metadata.fitness, 412.5);
    assert_eq!(metadata.observation_schema_hash, OBSERVATION_SCHEMA_HASH);
    assert_eq!(metadata.action_schema_hash, ACTION_SCHEMA_HASH);
    assert!(FootPolicy::from_artifact_bytes(&bytes, PolicyRole::SovietRifle).is_err());

    let mut bad_schema = bytes.clone();
    bad_schema[8] ^= 0x80;
    assert!(FootPolicy::from_artifact_bytes(&bad_schema, PolicyRole::TigerCrew).is_err());
    let mut unknown_flags = bytes.clone();
    unknown_flags[7] = 1;
    assert!(FootPolicy::from_artifact_bytes(&unknown_flags, PolicyRole::TigerCrew).is_err());
    let mut bad_action_schema = bytes.clone();
    bad_action_schema[16] ^= 0x40;
    assert!(FootPolicy::from_artifact_bytes(&bad_action_schema, PolicyRole::TigerCrew).is_err());
    let mut bad_topology = bytes.clone();
    bad_topology[24] ^= 1;
    assert!(FootPolicy::from_artifact_bytes(&bad_topology, PolicyRole::TigerCrew).is_err());
    let mut non_finite_fitness = bytes.clone();
    non_finite_fitness[40..44].copy_from_slice(&f32::INFINITY.to_le_bytes());
    assert!(FootPolicy::from_artifact_bytes(&non_finite_fitness, PolicyRole::TigerCrew).is_err());
    assert!(
        FootPolicy::from_artifact_bytes(&bytes[..bytes.len() - 1], PolicyRole::TigerCrew).is_err()
    );

    let sanitized_fitness = tiger.to_artifact_bytes(32, f32::NAN);
    let (_, sanitized_metadata) =
        FootPolicy::from_artifact_bytes(&sanitized_fitness, PolicyRole::TigerCrew).unwrap();
    assert_eq!(sanitized_metadata.fitness, 0.0);

    let path = std::env::temp_dir().join(format!(
        "kessel-foot-ai-{}-{}.bin",
        std::process::id(),
        metadata.generation
    ));
    tiger.save(path.to_str().unwrap(), 31, 412.5).unwrap();
    let (disk, disk_meta) =
        FootPolicy::load(path.to_str().unwrap(), PolicyRole::TigerCrew).unwrap();
    assert_eq!(disk.weights(), tiger.weights());
    assert_eq!(disk_meta, metadata);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn transition_v1_roundtrip_keeps_expert_action_reward_and_events() {
    let obs = observation(PolicyRole::TigerCrew);
    let action = micro_expert(&obs);
    let transition = FootTransition {
        episode_id: 0x1234,
        step: 17,
        seed: 0xC0DE,
        act: 1,
        site_id: 3,
        agent_id: 9,
        role: PolicyRole::TigerCrew,
        expert: ExpertSource::TigerMissionExpert,
        observation: obs,
        action: FootAction {
            fire: false,
            ..action
        },
        expert_action: action,
        reward: 55.25,
        next_observation: obs,
        terminated: false,
        truncated: false,
        events: TransitionEvents {
            map_searched: true,
            target_damage: 4.0,
            blocked: true,
            ..TransitionEvents::default()
        },
    };
    let encoded = transition.to_json_line().unwrap();
    assert!(encoded.contains(TRANSITION_SCHEMA));
    let decoded = FootTransition::from_json_line(&encoded).unwrap();
    assert_eq!(decoded, transition);

    let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    value["action_schema_hash"] = serde_json::json!("fnv1a64:0000000000000000");
    assert!(FootTransition::from_json_line(&value.to_string()).is_err());

    let mut unbounded: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    unbounded["action"]["forward"] = serde_json::json!(1.01);
    assert!(FootTransition::from_json_line(&unbounded.to_string()).is_err());

    let mut wrong_role_bits: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    wrong_role_bits["observation"][0] = serde_json::json!(-1.0);
    assert!(FootTransition::from_json_line(&wrong_role_bits.to_string()).is_err());

    let mut wrong_expert: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    wrong_expert["expert"] = serde_json::json!("soviet_doctrine_expert");
    assert!(FootTransition::from_json_line(&wrong_expert.to_string()).is_err());
}

#[test]
fn rewards_prefer_completed_physical_work_and_real_damage() {
    let tiger_success = tiger_reward(TigerRewardSignal {
        goal_progress_m: 15.0,
        map_searched: true,
        returned_with_map: true,
        remounted: true,
        mission_complete: true,
        survived: true,
        ..TigerRewardSignal::default()
    });
    let tiger_failure = tiger_reward(TigerRewardSignal {
        kia: true,
        damage_taken: 100.0,
        illegal_actions: 4,
        friendly_fire_damage: 20.0,
        noisy_shots: 8,
        ..TigerRewardSignal::default()
    });
    assert!(tiger_success.total > 190.0);
    assert!(tiger_failure.total < -200.0);

    let soviet_success = soviet_reward(SovietRewardSignal {
        real_damage_dealt: 100.0,
        foot_kia: true,
        map_denied: true,
        squad_survival_fraction: 0.75,
        ..SovietRewardSignal::default()
    });
    let soviet_cheat = soviet_reward(SovietRewardSignal {
        shots_without_legal_target: 10,
        friendly_fire_damage: 30.0,
        illegal_actions: 5,
        ..SovietRewardSignal::default()
    });
    assert!(soviet_success.total > 200.0);
    assert!(soviet_cheat.total < -80.0);

    let finite_guard = tiger_reward(TigerRewardSignal {
        goal_progress_m: f32::NAN,
        damage_taken: f32::INFINITY,
        friendly_fire_damage: f32::NAN,
        ..TigerRewardSignal::default()
    });
    assert!(finite_guard.total.is_finite());
}

#[test]
fn micro_curricula_train_both_roles_with_distinct_held_out_receipts() {
    let config = MicroTrainerConfig {
        epochs: 128,
        train_episodes: 8,
        held_out_episodes: 6,
        ..MicroTrainerConfig::default()
    };
    let reports = train_both_micro_curricula(config).unwrap();
    assert_eq!(reports[0].role, PolicyRole::TigerCrew);
    assert_eq!(reports[1].role, PolicyRole::SovietRifle);
    assert_ne!(reports[0].policy.weights(), reports[1].policy.weights());
    for report in &reports {
        eprintln!(
            "{} train_loss={:.6} held-out: mse={:.6} agreement={:.4} success={:.4} illegal={:.4} score={:.3}",
            report.role.label(),
            report.training_loss,
            report.held_out.action_mse,
            report.held_out.action_agreement,
            report.held_out.success_rate,
            report.held_out.illegal_action_rate,
            report.held_out.evaluation_score
        );
        assert!(report.training_loss.is_finite());
        assert!(
            report.held_out.action_mse < 0.24,
            "{} MSE {}",
            report.role.label(),
            report.held_out.action_mse
        );
        assert!(
            report.held_out.action_agreement >= 0.75,
            "{} agreement {}",
            report.role.label(),
            report.held_out.action_agreement
        );
        assert!(
            report.held_out.success_rate >= 0.80,
            "{} success rate {}",
            report.role.label(),
            report.held_out.success_rate
        );
        assert!(
            report.held_out.illegal_action_rate <= 0.04,
            "{} illegal action rate {}",
            report.role.label(),
            report.held_out.illegal_action_rate
        );
    }

    let repeat = train_both_micro_curricula(config).unwrap();
    assert_eq!(reports[0].policy.weights(), repeat[0].policy.weights());
    assert_eq!(reports[1].policy.weights(), repeat[1].policy.weights());
    assert_eq!(reports[0].held_out, repeat[0].held_out);
    assert_eq!(reports[1].held_out, repeat[1].held_out);
}

#[test]
fn checkpoint_names_cannot_alias_vehicle_or_opposing_foot_roles() {
    assert_ne!(
        PolicyRole::TigerCrew.checkpoint_path(),
        PolicyRole::SovietRifle.checkpoint_path()
    );
    assert!(PolicyRole::TigerCrew
        .checkpoint_path()
        .contains("foot_tiger"));
    assert!(PolicyRole::SovietRifle
        .checkpoint_path()
        .contains("foot_soviet"));
}

fn base_input(role: PolicyRole) -> ObservationInput {
    ObservationInput {
        role,
        goal: RelativeCue::default(),
        goal_kind: GoalKind::Hold,
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
        weapon: if role == PolicyRole::TigerCrew {
            WeaponSelection::Sidearm
        } else {
            WeaponSelection::Rifle
        },
    }
}
