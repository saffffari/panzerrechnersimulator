#[allow(dead_code)]
#[path = "../src/resource.rs"]
mod resource;
#[allow(dead_code)]
#[path = "../src/anti_infantry_ai.rs"]
mod anti_infantry_ai;
#[allow(dead_code)]
#[path = "../src/ballistics.rs"]
mod ballistics;
#[allow(dead_code)]
#[path = "../src/combat_training.rs"]
mod combat_training;
#[allow(dead_code)]
#[path = "../src/foot_ai.rs"]
mod foot_ai;
#[allow(dead_code)]
#[path = "../src/infantry_combat.rs"]
mod infantry_combat;
use glam::Vec3;
use infantry_combat::*;
#[test]
fn rifles_damage_both_teams_and_nearest_friendly_can_block_a_shot() {
    for team in [Team::German, Team::Soviet] {
        let enemy = if team == Team::German {
            Team::Soviet
        } else {
            Team::German
        };
        let mut actors = vec![
            Combatant::new(1, team, Vec3::new(0.0, 0.0, -5.0), 0.0),
            Combatant::new(2, enemy, Vec3::new(0.0, 0.0, -10.0), 0.0),
        ];
        let shot = Shot {
            shooter: 0,
            team,
            origin: Vec3::new(0.0, 1.0, 0.0),
            direction: Vec3::NEG_Z,
            range: 50.0,
            weapon: Weapon::Rifle,
        };
        let hit = resolve_hitscan(shot, &mut actors, &[]);
        assert_eq!(hit.actor_id, Some(1));
        assert!(hit.friendly);
        assert_eq!(hit.damage, 34.0);
        assert_eq!(actors[1].hp, 100.0);
        actors[0].pos.x = 3.0;
        let hit = resolve_hitscan(shot, &mut actors, &[]);
        assert_eq!(hit.actor_id, Some(2));
        assert!(!hit.friendly);
        assert_eq!(hit.damage, 34.0);
    }
}
#[test]
fn cover_stops_rifle_and_he_and_small_arms_never_damage_armor() {
    let wall = Cover {
        min: Vec3::new(-2.0, 0.0, -6.0),
        max: Vec3::new(2.0, 3.0, -5.0),
    };
    let mut actors = vec![Combatant::new(
        1,
        Team::Soviet,
        Vec3::new(0.0, 0.0, -7.0),
        0.0,
    )];
    let hit = resolve_hitscan(
        Shot {
            shooter: 0,
            team: Team::German,
            origin: Vec3::Y,
            direction: Vec3::NEG_Z,
            range: 30.0,
            weapon: Weapon::MachineGun,
        },
        &mut actors,
        &[wall],
    );
    assert_eq!(hit.damage, 0.0);
    assert!(blast(
        Vec3::new(0.0, 1.0, -4.0),
        0.9,
        Team::German,
        &mut actors,
        &[wall]
    )
    .is_empty());
    assert!(
        blast(
            Vec3::new(0.0, 1.0, -4.0),
            0.9,
            Team::German,
            &mut actors,
            &[]
        )[0]
        .damage
            > 0.0
    );
    for weapon in [Weapon::Rifle, Weapon::P08, Weapon::MachineGun] {
        assert_eq!(small_arms_damage(weapon, true), 0.0);
    }
}
#[test]
fn invisible_enemy_position_and_hp_do_not_leak_into_policy_observation() {
    let mut observer = Combatant::new(0, Team::German, Vec3::ZERO, 0.0);
    let mut enemy = Combatant::new(1, Team::Soviet, Vec3::new(0.0, 0.0, 15.0), 0.0);
    let a = observe(&mut observer, &[enemy.clone()], &[], Vec3::NEG_Z * 5.0, 0.1);
    enemy.pos = Vec3::new(15.0, 0.0, 25.0);
    enemy.hp = 1.0;
    let b = observe(&mut observer, &[enemy], &[], Vec3::NEG_Z * 5.0, 0.1);
    assert_eq!(a.values(), b.values());
    assert_eq!(a.values()[26], -1.0);
}
#[test]
fn new_german_artifact_is_role_checked_without_changing_old_dimensions() {
    let p = foot_ai::FootPolicy::seeded(foot_ai::PolicyRole::GermanRifle, 9, 0.04);
    let bytes = p.to_artifact_bytes(1, 2.0);
    assert_eq!(bytes[6], 3);
    assert_eq!(foot_ai::OBS_DIM, 51);
    assert!(
        foot_ai::FootPolicy::from_artifact_bytes(&bytes, foot_ai::PolicyRole::TigerCrew).is_err()
    );
    assert_eq!(
        foot_ai::FootPolicy::from_artifact_bytes(&bytes, foot_ai::PolicyRole::GermanRifle)
            .unwrap()
            .0
            .weights(),
        p.weights()
    );
}
#[test]
fn competitive_episodes_contain_multiple_germans_and_real_bidirectional_fire() {
    let policies = combat_training::Policies {
        german: None,
        soviet: None,
        tank: None,
    };
    let ep = combat_training::Episode::new(8, false);
    assert_eq!(
        ep.actors.iter().filter(|a| a.team == Team::German).count(),
        4
    );
    let a = combat_training::run_episode(8, 30.0, &policies, false);
    let b = combat_training::run_episode(8, 30.0, &policies, false);
    assert_eq!(a.json(), b.json());
    assert!(a.rifle_shots[0] > 0 && a.rifle_shots[1] > 0, "{:?}", a);
    assert!(a.damage[0] > 0.0 && a.damage[1] > 0.0, "{:?}", a);
}

#[test]
fn combat_warm_start_updates_the_network_without_accepting_mixed_roles_or_nan_labels() {
    use foot_ai::{FootAction,FootObservation,FootPolicy,PolicyRole};
    let obs=FootObservation::from_values(PolicyRole::GermanRifle,[0.0;foot_ai::OBS_DIM]);
    let label=FootAction{aim:true,fire:true,..Default::default()};
    let mut policy=FootPolicy::seeded(PolicyRole::GermanRifle,718,0.08);
    let before=policy.forward_raw(&obs);
    let loss=policy.fit_combat_observations(&[(obs,label);8],40,0.05).unwrap();
    assert!(loss.is_finite() && policy.weights().iter().all(|v|v.is_finite()));
    assert!(policy.forward_raw(&obs)[5]>before[5]);
    let protected=policy.weights().to_vec();
    let wrong=FootObservation::from_values(PolicyRole::SovietRifle,[0.0;foot_ai::OBS_DIM]);
    assert!(policy.fit_combat_observations(&[(wrong,label)],1,0.05).is_err());
    assert!(policy.fit_combat_observations(&[(obs,FootAction{turn:f32::NAN,..label})],1,0.05).is_err());
    assert_eq!(policy.weights(),protected);
}
#[test]
fn tank_policy_requests_produce_real_mg_rounds_and_he_projectiles() {
    let mut force = anti_infantry_ai::Policy::doctrine();
    let output_bias = anti_infantry_ai::WEIGHTS - anti_infantry_ai::ACT;
    force.weights[output_bias + 3] = 2.0;
    force.weights[output_bias + 4] = 2.0;
    let policies = combat_training::Policies {
        german: None,
        soviet: None,
        tank: Some(force),
    };
    let mut aggregate = [0.0; 3];
    for seed in 0..8 {
        let m = combat_training::run_episode(seed, 25.0, &policies, true);
        aggregate[0] += m.mg_shots as f32;
        aggregate[1] += m.he_shots as f32;
        aggregate[2] += m.tank_damage;
    }
    assert!(
        aggregate.iter().all(|x| *x > 0.0),
        "MG rounds, HE rounds, tank inflicted actual damage: {aggregate:?}"
    );
}
#[test]
fn tank_artifact_rejects_dimension_schema_and_nonfinite_weights() {
    assert!(anti_infantry_ai::Policy::new(vec![0.0; 2]).is_err());
    assert!(anti_infantry_ai::Policy::new(vec![f32::NAN; anti_infantry_ai::WEIGHTS]).is_err());
}

#[test]
fn high_explosive_alone_reaches_terminal_contact_and_causes_shared_blast_damage() {
    let mut force = anti_infantry_ai::Policy::doctrine();
    let output_bias = anti_infantry_ai::WEIGHTS - anti_infantry_ai::ACT;
    force.weights[output_bias + 3] = -5.0;
    force.weights[output_bias + 4] = 5.0;
    let policies = combat_training::Policies {
        german: None,
        soviet: None,
        tank: Some(force),
    };
    let mut shells = 0;
    let mut detonations = 0;
    let mut he_damage = 0.0;
    for seed in 0..16 {
        let m = combat_training::run_episode(seed, 30.0, &policies, true);
        assert_eq!(m.mg_shots, 0);
        assert_eq!(m.mg_damage, 0.0);
        shells += m.he_shots;
        detonations += m.he_detonations;
        he_damage += m.he_damage;
    }
    assert!(
        shells > 0 && detonations > 0 && he_damage > 0.0,
        "shells={shells} detonations={detonations} HE damage={he_damage}"
    );
}
