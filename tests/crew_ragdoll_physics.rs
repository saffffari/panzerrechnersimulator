#[path = "../src/crew_physics.rs"]
mod crew_physics;

use glam::{Quat, Vec3};

#[test]
fn measured_contact_energy_and_speed_set_conservative_body_momentum() {
    let half = Vec3::new(0.247, 0.765, 0.123);
    let motion = crew_physics::impact_motion(
        half,
        Vec3::Y,
        Vec3::ZERO,
        Vec3::new(0.0, 0.18, 0.10),
        Vec3::X * 100.0,
        50.0,
        Vec3::Z,
    );

    let expected_speed = (2.0 * 50.0 / 100.0) / crew_physics::BODY_MASS_KG;
    assert!((motion.linear_velocity.x - expected_speed).abs() < 1.0e-6);
    assert!(motion.linear_velocity.y.abs() < 1.0e-6);
    assert!(motion.linear_velocity.z.abs() < 1.0e-6);
    assert!(
        motion.angular_velocity.dot(Vec3::Y).abs() < 1.0e-6,
        "the unslotted body must not receive a spine-axis pirouette"
    );
}

#[test]
fn stronger_same_speed_contact_carries_more_motion_without_authored_vertical_launch() {
    let solve = |energy| {
        crew_physics::impact_motion(
            Vec3::new(0.247, 0.765, 0.123),
            Vec3::Y,
            Vec3::ZERO,
            Vec3::new(0.18, 0.31, 0.0),
            Vec3::new(-90.0, 0.0, 0.0),
            energy,
            Vec3::Z,
        )
    };
    let weak = solve(0.24);
    let heavy = solve(96.0);

    assert!(heavy.linear_velocity.length() > weak.linear_velocity.length() * 100.0);
    assert!(weak.linear_velocity.y.abs() < 1.0e-6);
    assert!(heavy.linear_velocity.y.abs() < 1.0e-6);
}

#[test]
fn long_axis_spin_dies_much_faster_than_a_legitimate_fall() {
    let initial = Vec3::new(2.0, 6.0, -1.0);
    let after = crew_physics::damp_angular_velocity(initial, Vec3::Y, 0.35);

    let swing_ratio =
        Vec3::new(after.x, 0.0, after.z).length() / Vec3::new(initial.x, 0.0, initial.z).length();
    let twist_ratio = after.y.abs() / initial.y.abs();
    assert!(swing_ratio > 0.55);
    assert!(twist_ratio < 0.03);
    assert!(twist_ratio < swing_ratio * 0.05);
}

#[test]
fn upright_limp_body_falls_on_hit_side_and_horizontal_body_can_sleep() {
    let hit_side = Vec3::Z;
    let upright = crew_physics::topple_acceleration(Vec3::Y, hit_side, 1.53);
    assert!(upright.dot(hit_side) > 0.30);

    let already_down = crew_physics::topple_acceleration(Vec3::Z, hit_side, 1.53);
    assert_eq!(already_down, Vec3::ZERO);
}

#[test]
fn anatomical_segments_have_stable_ids_and_sum_to_one_body_mass() {
    assert_eq!(
        crew_physics::CrewBodyPart::ALL.len(),
        crew_physics::CREW_BODY_PART_COUNT
    );
    let mass: f32 = crew_physics::CrewBodyPart::ALL
        .iter()
        .map(|part| part.mass_kg())
        .sum();
    assert!((mass - crew_physics::BODY_MASS_KG).abs() < 1.0e-4);
    for (index, part) in crew_physics::CrewBodyPart::ALL.iter().enumerate() {
        assert_eq!(part.index(), index);
        assert!(part.half_extents().min_element() > 0.0);
    }
}

#[test]
fn exact_contact_position_selects_head_arm_and_leg_instead_of_one_body_pivot() {
    use crew_physics::CrewBodyPart::*;
    let segments = crew_physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
    assert_eq!(
        crew_physics::nearest_body_part(&segments, Head.rest_center(), 1.0),
        Head
    );
    assert_eq!(
        crew_physics::nearest_body_part(&segments, LeftForearm.rest_center(), 1.0),
        LeftForearm
    );
    assert_eq!(
        crew_physics::nearest_body_part(&segments, RightShin.rest_center(), 1.0),
        RightShin
    );
}

#[test]
fn seated_posts_begin_compact_without_an_upright_casualty_frame() {
    use crew_physics::CrewBodyPart::*;
    let seated = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.80);
    assert!(crew_physics::max_joint_error(&seated, 0.80) < 0.002);
    let thigh_axis = seated[LeftThigh.index()].rot * Vec3::Y;
    let forearm_axis = seated[RightForearm.index()].rot * Vec3::Y;
    assert!(
        thigh_axis.z < -0.80,
        "thigh must already be folded into the seat"
    );
    assert!(
        forearm_axis.z < -0.75,
        "forearm must already rest toward the controls"
    );
    assert!(seated[Head.index()].pos.y < 1.15);
    assert!(seated[LeftShin.index()].pos.y < seated[Pelvis.index()].pos.y);
}

#[test]
fn close_filler_blast_throws_harder_and_fragment_impulse_starts_in_hit_limb() {
    use crew_physics::CrewBodyPart::*;
    let mut close = crew_physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
    let mut far = close;
    let contact = LeftForearm.rest_center() + Vec3::X * 0.02;
    let close_result = crew_physics::apply_authoritative_impact(
        &mut close,
        contact,
        Vec3::X * 120.0,
        90.0,
        Vec3::new(-0.30, 0.75, 0.0),
        65.0 / 1000.0 * 4.184e6,
        1.0,
    );
    let far_result = crew_physics::apply_authoritative_impact(
        &mut far,
        contact,
        Vec3::X * 120.0,
        90.0,
        Vec3::new(-1.60, 0.75, 0.0),
        65.0 / 1000.0 * 4.184e6,
        1.0,
    );

    assert_eq!(close_result.hit_part, LeftForearm);
    assert!(close_result.blast_impulse_ns.length() > far_result.blast_impulse_ns.length() * 4.0);
    assert!(
        close[LeftForearm.index()].vel.x > close[Torso.index()].vel.x,
        "measured fragment momentum must enter the intersected limb before joints redistribute it"
    );
    assert!(close_result.blast_impulse_ns.length() > 35.0);
}

#[test]
fn confined_ammunition_blast_can_throw_one_body_at_six_mps_but_never_exceeds_it() {
    let saturated = crew_physics::confined_blast_impulse(
        Vec3::ZERO,
        Vec3::new(0.45, 0.05, 0.0),
        90_000_000.0,
        1.0,
    );
    let launch_speed = saturated.length() / crew_physics::BODY_MASS_KG;
    assert!((launch_speed - 6.0).abs() < 1.0e-5);

    let farther =
        crew_physics::confined_blast_impulse(Vec3::ZERO, Vec3::new(2.0, 0.05, 0.0), 652_288.0, 1.0);
    assert!(farther.length() < saturated.length());
    assert!(farther.dot(Vec3::X) > 0.0);
}

#[test]
fn blast_pressure_reaches_each_segment_independently_instead_of_moving_a_mannequin() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.82);
    let source = segments[LeftForearm.index()].pos - Vec3::X * 0.18;
    let impulses =
        crew_physics::confined_segment_blast_impulses(&segments, source, 652_288.0, 0.82);
    let scalar_total: f32 = impulses.iter().map(|impulse| impulse.length()).sum();
    assert!(scalar_total <= crew_physics::BODY_MASS_KG * 6.0 + 1.0e-4);
    assert!(impulses[LeftForearm.index()].length() > impulses[Torso.index()].length());

    let contact = segments[LeftForearm.index()].pos;
    crew_physics::apply_authoritative_impact_to_part(
        &mut segments,
        LeftForearm,
        contact,
        Vec3::ZERO,
        0.0,
        source,
        652_288.0,
        0.82,
    );
    assert!(
        (segments[LeftForearm.index()].vel - segments[Torso.index()].vel).length() > 0.25,
        "local pressure must begin articulating the limb before a cabin collision"
    );
}

#[test]
fn confined_br471_side_burst_delivers_a_hard_bounded_whole_body_throw() {
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.82);
    let source = Vec3::new(-0.78, 0.88, 0.02);
    let impulses =
        crew_physics::confined_segment_blast_impulses(&segments, source, 652_704.0, 0.82);
    for (segment, impulse) in segments.iter_mut().zip(impulses) {
        segment.vel += impulse / segment.part.mass_kg();
    }
    let speed = crew_physics::center_of_mass_velocity(&segments).length();
    assert!(
        speed > 3.5,
        "a close filled 122 mm burst in the sealed turret must throw, got {speed:.2} m/s"
    );
    assert!(
        speed <= 6.0 + 1.0e-4,
        "the hard launch cap must remain respected"
    );
}

#[test]
fn pulverized_limb_shares_exact_impulse_without_creating_momentum_or_energy() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.82);
    for segment in &mut segments {
        segment.vel = Vec3::new(0.18, -0.04, 0.07);
    }
    let contact = segments[LeftForearm.index()].pos + Vec3::Y * 0.04;
    let impulse = crew_physics::apply_fragment_impulse_to_part(
        &mut segments,
        LeftForearm,
        contact,
        Vec3::new(115.0, 4.0, -3.0),
        135.0,
        0.82,
    );
    let momentum = |body: &[crew_physics::CrewSegment; crew_physics::CREW_BODY_PART_COUNT]| {
        body.iter()
            .filter(|segment| segment.present)
            .map(|segment| segment.vel * segment.part.mass_kg())
            .sum::<Vec3>()
    };
    let kinetic = |body: &[crew_physics::CrewSegment; crew_physics::CREW_BODY_PART_COUNT]| {
        body.iter()
            .filter(|segment| segment.present)
            .map(|segment| 0.5 * segment.part.mass_kg() * segment.vel.length_squared())
            .sum::<f32>()
    };
    let momentum_before = momentum(&segments);
    let energy_before = kinetic(&segments);
    let torso_before = segments[Torso.index()].vel;
    let mut impulses = [Vec3::ZERO; crew_physics::CREW_BODY_PART_COUNT];
    impulses[LeftForearm.index()] = impulse;
    let mut pulverized = [false; crew_physics::CREW_BODY_PART_COUNT];
    pulverized[LeftForearm.index()] = true;

    let fraction =
        crew_physics::couple_pulverized_fragment_impulses(&mut segments, &impulses, &pulverized);
    assert!(fraction > 0.0);
    assert!((momentum(&segments) - momentum_before).length() < 2.0e-4);
    assert!(kinetic(&segments) <= energy_before + 1.0e-3);
    assert!(segments[Torso.index()].vel.x > torso_before.x);

    let removed_velocity = segments[LeftForearm.index()].vel;
    segments[LeftForearm.index()].present = false;
    assert!(removed_velocity.x > torso_before.x);
    assert!(crew_physics::center_of_mass_velocity(&segments).x > torso_before.x);
}

#[test]
fn pulverized_impulse_coupling_is_a_noop_without_surviving_anatomy() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.82);
    for segment in &mut segments {
        segment.present = segment.part == LeftForearm;
    }
    let before = segments;
    let mut impulses = [Vec3::ZERO; crew_physics::CREW_BODY_PART_COUNT];
    impulses[LeftForearm.index()] = Vec3::X * 2.0;
    let mut pulverized = [false; crew_physics::CREW_BODY_PART_COUNT];
    pulverized[LeftForearm.index()] = true;
    assert_eq!(
        crew_physics::couple_pulverized_fragment_impulses(&mut segments, &impulses, &pulverized,),
        0.0
    );
    assert_eq!(segments, before);
}

#[test]
fn joints_are_limp_but_bounded_and_removed_or_detached_parts_are_not_reconnected() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
    let rest_error = crew_physics::max_joint_error(&segments, 1.0);
    assert!(rest_error < 0.0021, "rest joint error {rest_error}");

    segments[LeftForearm.index()].pos += Vec3::new(-0.55, 0.20, 0.30);
    crew_physics::solve_joint_constraints(&mut segments, 1.0, 1.0 / 120.0, 12);
    assert!(crew_physics::max_joint_error(&segments, 1.0) < 0.018);
    assert!(
        segments[LeftUpperArm.index()]
            .pos
            .distance(LeftUpperArm.rest_center())
            > 0.01,
        "the connected upper arm must receive motion instead of leaving a rigid torso mesh"
    );

    segments[RightForearm.index()].detached = true;
    segments[RightForearm.index()].pos += Vec3::X;
    let detached = segments[RightForearm.index()].pos;
    crew_physics::solve_joint_constraints(&mut segments, 1.0, 1.0 / 120.0, 12);
    assert_eq!(segments[RightForearm.index()].pos, detached);

    let mut branch = crew_physics::rest_segments(Vec3::ZERO, Quat::IDENTITY, 1.0);
    branch[LeftUpperArm.index()].detached = true; // shoulder severed, elbow still connected
    branch[LeftUpperArm.index()].pos += Vec3::new(-0.45, 0.10, 0.0);
    let forearm_before = branch[LeftForearm.index()].pos;
    crew_physics::solve_joint_constraints(&mut branch, 1.0, 1.0 / 120.0, 12);
    assert!(branch[LeftForearm.index()].pos.distance(forearm_before) > 0.10);
    assert!(
        branch[LeftUpperArm.index()]
            .pos
            .distance(branch[LeftForearm.index()].pos)
            < 0.50,
        "breaking a shoulder must carry the still-attached forearm branch"
    );

    segments[Head.index()].present = false;
    let absent = segments[Head.index()].pos;
    crew_physics::solve_joint_constraints(&mut segments, 1.0, 1.0 / 120.0, 12);
    assert_eq!(segments[Head.index()].pos, absent);
}

#[test]
fn ball_joint_iterations_do_not_average_independent_limb_rotation() {
    use crew_physics::CrewBodyPart::*;
    let mut few = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.80);
    few[LeftForearm.index()].angular_vel = Vec3::new(7.0, -2.0, 4.0);
    few[Torso.index()].angular_vel = Vec3::new(-1.0, 0.5, 0.0);
    let mut many = few;

    crew_physics::solve_joint_constraints(&mut few, 0.80, 1.0 / 240.0, 1);
    crew_physics::solve_joint_constraints(&mut many, 0.80, 1.0 / 240.0, 16);

    assert_eq!(
        few[LeftForearm.index()].angular_vel,
        many[LeftForearm.index()].angular_vel
    );
    assert_eq!(
        few[Torso.index()].angular_vel,
        many[Torso.index()].angular_vel
    );
    assert!(
        (many[LeftForearm.index()].angular_vel - many[Torso.index()].angular_vel).length() > 8.0,
        "a ball joint must leave the dead arm free to rotate relative to the torso"
    );
}

#[test]
fn one_grounded_hand_cannot_freeze_an_upright_body() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.80);
    segments[LeftForearm.index()].grounded = true;
    assert!(!crew_physics::has_stable_floor_support(&segments));

    segments[Pelvis.index()].grounded = true;
    assert!(crew_physics::has_stable_floor_support(&segments));

    for segment in &mut segments {
        segment.present = false;
        segment.grounded = false;
    }
    segments[Torso.index()].present = true;
    segments[Torso.index()].grounded = true;
    assert!(
        crew_physics::has_stable_floor_support(&segments),
        "a lone catastrophic central remnant must still be able to settle"
    );
}

#[test]
fn fragment_hit_propagates_through_limp_chain_without_exploding_constraints() {
    use crew_physics::CrewBodyPart::*;
    let mut segments = crew_physics::seated_segments(Vec3::ZERO, Quat::IDENTITY, 0.80);
    crew_physics::apply_authoritative_impact(
        &mut segments,
        LeftUpperArm.rest_center() * 0.80,
        Vec3::new(95.0, 8.0, 20.0),
        120.0,
        Vec3::new(-0.35, 0.65, 0.0),
        65.0 / 1000.0 * 4.184e6,
        0.80,
    );
    for _ in 0..240 {
        crew_physics::integrate_free_motion(&mut segments, 1.0 / 240.0);
        crew_physics::solve_joint_constraints(&mut segments, 0.80, 1.0 / 240.0, 8);
    }

    assert!(segments.iter().all(|segment| {
        segment.pos.is_finite()
            && segment.rot.is_finite()
            && segment.vel.is_finite()
            && segment.angular_vel.is_finite()
    }));
    assert!(crew_physics::max_joint_error(&segments, 0.80) < 0.030);
    assert!(crew_physics::maximum_surface_speed(&segments, 0.80) < 18.1);
    assert!(
        segments[LeftUpperArm.index()]
            .rot
            .dot(segments[Torso.index()].rot)
            .abs()
            < 0.9999,
        "hit arm must articulate independently of the torso"
    );
}
