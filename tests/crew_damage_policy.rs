#[path = "../src/crew_damage.rs"]
mod crew_damage;
#[path = "../src/crew_physics.rs"]
mod crew_physics;

use crew_damage::{
    resolve_crew_damage, swept_sphere_obb_fraction, swept_sphere_oriented_box_fraction,
    CrewDamageInput, CrewHitVolume, CrewTissueOutcome, MAX_BLOOD_CUBES_PER_EVENT,
};
use crew_physics::CrewBodyPart;
use glam::{Quat, Vec3};

#[test]
fn close_filled_shell_can_destroy_an_unpierced_head_from_pressure_work() {
    let result = resolve_crew_damage(CrewDamageInput {
        part: CrewBodyPart::Head,
        fragment_energy_j: 0.0,
        explosive_energy_j: 65.0 / 1000.0 * 4.184e6,
        blast_distance_m: crew_damage::MIN_BLAST_DISTANCE_M,
    });
    assert!(matches!(
        result.outcome,
        CrewTissueOutcome::Detachment | CrewTissueOutcome::Pulverized
    ));
    assert!(result.blast_work_j > 0.0);
}

#[test]
fn exact_rotated_part_contact_drives_bounded_deterministic_destruction() {
    let forearm = CrewHitVolume::new(
        CrewBodyPart::RightForearm,
        Vec3::new(0.33, 0.82, 0.08),
        Quat::from_rotation_x(-0.95),
        CrewBodyPart::RightForearm.half_extents(),
    );
    let from = Vec3::new(-0.7, 0.82, 0.08);
    let to = Vec3::new(0.8, 0.82, 0.08);
    let fraction = swept_sphere_obb_fraction(from, to, 0.003, forearm)
        .expect("fragment should cross the posed forearm");
    let wound = from.lerp(to, fraction);
    assert!(wound.x < forearm.center.x);

    let first = resolve_crew_damage(CrewDamageInput {
        part: forearm.part,
        fragment_energy_j: 95.0,
        explosive_energy_j: 246_856.0,
        blast_distance_m: 0.32,
    });
    let repeat = resolve_crew_damage(CrewDamageInput {
        part: forearm.part,
        fragment_energy_j: 95.0,
        explosive_energy_j: 246_856.0,
        blast_distance_m: 0.32,
    });
    assert_eq!(first, repeat);
    assert_eq!(first.outcome, CrewTissueOutcome::Pulverized);
    assert!(first.detach_joint && first.hide_mesh_part);
    assert!(first.blood_cube_count <= MAX_BLOOD_CUBES_PER_EVENT);
    assert_eq!(first.blood_cube_count % 16, 0);
}

#[test]
fn gap_between_limb_and_torso_remains_a_real_miss() {
    let arm = CrewHitVolume::new(
        CrewBodyPart::LeftUpperArm,
        Vec3::new(-0.32, 1.0, 0.0),
        Quat::IDENTITY,
        CrewBodyPart::LeftUpperArm.half_extents(),
    );
    let torso = CrewHitVolume::new(
        CrewBodyPart::Torso,
        Vec3::new(0.0, 1.04, 0.0),
        Quat::IDENTITY,
        CrewBodyPart::Torso.half_extents(),
    );
    let from = Vec3::new(-0.255, 0.60, -0.4);
    let to = Vec3::new(-0.255, 0.60, 0.4);
    assert!(swept_sphere_obb_fraction(from, to, 0.002, arm).is_none());
    assert!(swept_sphere_obb_fraction(from, to, 0.002, torso).is_none());
}

#[test]
fn rotated_live_cartridge_uses_its_narrow_visible_obb_not_the_retired_rack_sphere() {
    let center = Vec3::new(0.42, 0.91, -0.27);
    let rotation = Quat::from_rotation_y(0.63) * Quat::from_rotation_x(-0.28);
    let half_extents = Vec3::new(0.071, 0.071, 0.465);
    let along_axis_from = center + rotation * Vec3::new(0.0, 0.0, -1.0);
    let along_axis_to = center + rotation * Vec3::new(0.0, 0.0, 1.0);
    assert!(swept_sphere_oriented_box_fraction(
        along_axis_from,
        along_axis_to,
        0.003,
        center,
        rotation,
        half_extents,
    )
    .is_some());

    // Nine centimetres off the round's local axis is still deep inside the old 1.70 m rack proxy,
    // but outside the rendered 14.13 cm cartridge plus this 3 mm fragment radius.
    let miss_offset = rotation * Vec3::new(0.09, 0.0, 0.0);
    assert!(swept_sphere_oriented_box_fraction(
        along_axis_from + miss_offset,
        along_axis_to + miss_offset,
        0.003,
        center,
        rotation,
        half_extents,
    )
    .is_none());
}
