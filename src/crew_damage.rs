//! Deterministic anatomical trauma policy for Tiger crew.
//!
//! The terminal-fragment solver owns contact geometry and supplies the fragment's kinetic energy at
//! the swept hit. The penetration solver supplies the modeled filler energy and the affected part's
//! distance from the internal burst origin. This module only converts those physical inputs into a
//! tissue outcome and a bounded request for the existing equal-size blood-cube pool. It contains no
//! camera, presentation, random, role, peer, or gameplay-authority policy.

// Registered as a sibling private module inside `game`, alongside `crew_physics`.
use glam::{Quat, Vec3};

use super::crew_physics::CrewBodyPart;

/// Matches `game.rs`'s one-thousand-and-twenty-four-body wet-blood pool. Multiple simultaneous
/// casualties may divide this budget before spawning, but one anatomical result can never request
/// more than the complete pool.
pub const MAX_BLOOD_CUBES_PER_EVENT: usize = 1024;

/// Provisional minimum injury work at an actual anatomical contact. This is not a universal
/// lethal threshold: immediate death is decided from the affected vital region and its outcome.
pub const MIN_INJURY_WORK_J: f32 = 0.24;

/// The armor-side contact and the victim cannot be closer than roughly a body radius in the current
/// cabin representation. This clamp also prevents the spherical blast-fluence term from becoming
/// singular if a fragment starts exactly on a volume boundary.
pub const MIN_BLAST_DISTANCE_M: f32 = 0.18;

/// Only a small fraction of spherical chemical energy incident on a clothed body segment becomes
/// destructive tissue work. The remaining energy stays in the gas, heats the cabin, loads the
/// structure, or passes around the body. This is a single global coupling term, not a severity- or
/// camera-dependent artistic multiplier.
pub const CONFINED_BLAST_TISSUE_COUPLING: f32 = 0.0025;

const MAX_MODELED_ENERGY_J: f32 = 10_000_000.0;
const BLOOD_COUNT_QUANTUM: usize = 16;

/// One pose-matched anatomical collision volume in the penetration simulation's local frame.
/// Living seated and standing rigs provide these transforms from the same pose used for rendering;
/// the damage pass therefore intersects the visible limb rather than a generic torso proxy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewHitVolume {
    pub part: CrewBodyPart,
    pub center: Vec3,
    pub rotation: Quat,
    pub half_extents: Vec3,
}

impl CrewHitVolume {
    pub fn new(part: CrewBodyPart, center: Vec3, rotation: Quat, half_extents: Vec3) -> Self {
        Self {
            part,
            center,
            rotation: rotation.normalize(),
            half_extents: half_extents.abs(),
        }
    }
}

/// Earliest normalized contact of a swept spherical fragment with one anatomical OBB. The sphere
/// is handled exactly for the box's faces by Minkowski-expanding all three local half extents. The
/// rounded edge/corner overreach is conservative by at most the fragment radius, which is under
/// five millimetres for the live shower and prevents high-speed grains tunnelling between ticks.
pub fn swept_sphere_obb_fraction(
    from: Vec3,
    to: Vec3,
    fragment_radius_m: f32,
    volume: CrewHitVolume,
) -> Option<f32> {
    swept_sphere_oriented_box_fraction(
        from,
        to,
        fragment_radius_m,
        volume.center,
        volume.rotation,
        volume.half_extents,
    )
}

/// Generic form of the same swept-sphere/OBB test. Crew volumes call through the typed wrapper
/// above; live 8.8 cm cartridges use this function with the exact transform and bounds of the mesh
/// visible in the rack. Keeping one implementation prevents the damage proxy from drifting away
/// from anatomical or ammunition presentation geometry.
pub fn swept_sphere_oriented_box_fraction(
    from: Vec3,
    to: Vec3,
    fragment_radius_m: f32,
    center: Vec3,
    rotation: Quat,
    half_extents: Vec3,
) -> Option<f32> {
    if !from.is_finite()
        || !to.is_finite()
        || !center.is_finite()
        || !rotation.is_finite()
        || !half_extents.is_finite()
    {
        return None;
    }
    let inverse_rotation = rotation.normalize().conjugate();
    let local_from = inverse_rotation * (from - center);
    let local_to = inverse_rotation * (to - center);
    let delta = local_to - local_from;
    let expanded = half_extents.abs() + Vec3::splat(fragment_radius_m.max(0.0));

    let mut enter = 0.0f32;
    let mut exit = 1.0f32;
    for axis in 0..3 {
        let origin = local_from[axis];
        let travel = delta[axis];
        let half = expanded[axis];
        if travel.abs() <= 1.0e-8 {
            if origin < -half || origin > half {
                return None;
            }
            continue;
        }
        let inverse = travel.recip();
        let mut near = (-half - origin) * inverse;
        let mut far = (half - origin) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    (exit >= 0.0 && enter <= 1.0).then_some(enter.clamp(0.0, 1.0))
}

/// Ordered so monotonic-policy tests can compare outcomes directly.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CrewTissueOutcome {
    #[default]
    None = 0,
    /// Penetrating or blast injury. The anatomical segment remains attached and visible.
    Injury = 1,
    /// A joint has failed. The complete mesh segment remains physical but becomes a free body.
    Detachment = 2,
    /// The affected segment no longer has coherent mesh mass and is replaced by granular evidence.
    Pulverized = 3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewDamageInput {
    pub part: CrewBodyPart,
    /// Kinetic energy of the actual fragment at the swept anatomical contact.
    pub fragment_energy_j: f32,
    /// Chemical energy of the shell filler that detonated inside; zero for solid shot.
    pub explosive_energy_j: f32,
    /// Distance from the penetration/internal-burst origin to this anatomical part's centre.
    pub blast_distance_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewDamageResult {
    pub part: CrewBodyPart,
    pub outcome: CrewTissueOutcome,
    /// Actual non-negative contact energy admitted by the policy.
    pub fragment_work_j: f32,
    /// Estimated tissue work from modeled filler energy, spherical distance falloff, the segment's
    /// mean projected area, and the global confined-cabin coupling coefficient.
    pub blast_work_j: f32,
    pub total_work_j: f32,
    /// Feed directly to `CrewSegment::detached` for this part only.
    pub detach_joint: bool,
    /// Feed directly to `!CrewSegment::present` and hide this part's render instance only.
    pub hide_mesh_part: bool,
    /// Equal-size independent cube bodies requested at the exact wound point. Always a multiple of
    /// sixteen so the current four color pages divide deterministically.
    pub blood_cube_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct PartResistance {
    /// `None` for torso/pelvis: those central masses go from injury to local pulverization instead
    /// of becoming implausible intact free-floating blocks.
    detach_work_j: Option<f32>,
    pulverize_work_j: f32,
}

fn part_resistance(part: CrewBodyPart) -> PartResistance {
    // These are destructive work thresholds, not shell muzzle energies. Distal parts fail before
    // proximal limbs; thighs and the central body require substantially more work. The values also
    // sit well above MIN_INJURY_WORK_J, preserving a broad non-amputating injury band.
    match part {
        CrewBodyPart::Pelvis => PartResistance {
            detach_work_j: None,
            pulverize_work_j: 620.0,
        },
        CrewBodyPart::Torso => PartResistance {
            detach_work_j: None,
            pulverize_work_j: 1_250.0,
        },
        CrewBodyPart::Head => PartResistance {
            detach_work_j: Some(72.0),
            pulverize_work_j: 210.0,
        },
        CrewBodyPart::LeftUpperArm | CrewBodyPart::RightUpperArm => PartResistance {
            detach_work_j: Some(46.0),
            pulverize_work_j: 132.0,
        },
        CrewBodyPart::LeftForearm | CrewBodyPart::RightForearm => PartResistance {
            detach_work_j: Some(28.0),
            pulverize_work_j: 82.0,
        },
        CrewBodyPart::LeftThigh | CrewBodyPart::RightThigh => PartResistance {
            detach_work_j: Some(138.0),
            pulverize_work_j: 390.0,
        },
        CrewBodyPart::LeftShin | CrewBodyPart::RightShin => PartResistance {
            detach_work_j: Some(82.0),
            pulverize_work_j: 228.0,
        },
    }
}

fn finite_nonnegative_energy(value: f32) -> f32 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else {
        value.min(MAX_MODELED_ENERGY_J)
    }
}

fn finite_blast_distance(value: f32) -> f32 {
    if value.is_nan() {
        f32::INFINITY
    } else {
        value.max(MIN_BLAST_DISTANCE_M)
    }
}

/// Mean projected area of an anatomical OBB, using Cauchy's surface-area relation for a convex
/// body: mean projection = surface area / 4. `half_extents` comes from the same eleven-part proxy
/// definition used by swept fragment collision and ragdoll contact.
pub fn mean_projected_area_m2(part: CrewBodyPart) -> f32 {
    let h = part.half_extents();
    2.0 * (h.x * h.y + h.x * h.z + h.y * h.z)
}

/// Tissue work contributed by a modeled internal explosive source at an anatomical part. The
/// inverse-square term is ordinary spherical energy fluence. It is deliberately separate from the
/// exact fragment contact so solid shot remains purely kinetic.
pub fn blast_tissue_work_j(part: CrewBodyPart, explosive_energy_j: f32, distance_m: f32) -> f32 {
    let energy = finite_nonnegative_energy(explosive_energy_j);
    if energy == 0.0 {
        return 0.0;
    }
    let distance = finite_blast_distance(distance_m);
    let fluence_j_m2 = energy / (4.0 * std::f32::consts::PI * distance * distance);
    (fluence_j_m2 * mean_projected_area_m2(part) * CONFINED_BLAST_TISSUE_COUPLING)
        .min(MAX_MODELED_ENERGY_J)
}

fn smooth01(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn quantized_count(raw: f32, low: usize, high: usize) -> usize {
    let raw = raw.clamp(low as f32, high as f32);
    let count = ((raw / BLOOD_COUNT_QUANTUM as f32).round() as usize) * BLOOD_COUNT_QUANTUM;
    count.clamp(low, high).min(MAX_BLOOD_CUBES_PER_EVENT)
}

fn blood_cube_count(
    outcome: CrewTissueOutcome,
    total_work_j: f32,
    resistance: PartResistance,
) -> usize {
    match outcome {
        CrewTissueOutcome::None => 0,
        CrewTissueOutcome::Injury => {
            let upper = resistance
                .detach_work_j
                .unwrap_or(resistance.pulverize_work_j);
            let t = smooth01(
                (total_work_j - MIN_INJURY_WORK_J) / (upper - MIN_INJURY_WORK_J).max(1.0e-4),
            );
            quantized_count(96.0 + 224.0 * t, 96, 320)
        }
        CrewTissueOutcome::Detachment => {
            let detach = resistance.detach_work_j.unwrap_or(MIN_INJURY_WORK_J);
            let t = smooth01(
                (total_work_j - detach) / (resistance.pulverize_work_j - detach).max(1.0e-4),
            );
            quantized_count(384.0 + 320.0 * t, 384, 704)
        }
        CrewTissueOutcome::Pulverized => {
            let excess = (total_work_j / resistance.pulverize_work_j.max(1.0e-4) - 1.0).max(0.0);
            let t = 1.0 - (-0.70 * excess).exp();
            quantized_count(768.0 + 256.0 * t, 768, MAX_BLOOD_CUBES_PER_EVENT)
        }
    }
}

/// Convert one exact anatomical contact and its penetration event into mesh/physics consequences.
/// This function is deliberately total and deterministic: invalid negative/NaN energies become
/// zero, an invalid distance contributes no blast work, and positive infinity saturates.
pub fn resolve_crew_damage(input: CrewDamageInput) -> CrewDamageResult {
    let resistance = part_resistance(input.part);
    let fragment_work_j = finite_nonnegative_energy(input.fragment_energy_j);
    let blast_work_j =
        blast_tissue_work_j(input.part, input.explosive_energy_j, input.blast_distance_m);
    let total_work_j = (fragment_work_j + blast_work_j).min(MAX_MODELED_ENERGY_J);

    let outcome = if total_work_j < MIN_INJURY_WORK_J {
        CrewTissueOutcome::None
    } else if total_work_j >= resistance.pulverize_work_j {
        CrewTissueOutcome::Pulverized
    } else if resistance
        .detach_work_j
        .is_some_and(|threshold| total_work_j >= threshold)
    {
        CrewTissueOutcome::Detachment
    } else {
        CrewTissueOutcome::Injury
    };

    CrewDamageResult {
        part: input.part,
        outcome,
        fragment_work_j,
        blast_work_j,
        total_work_j,
        detach_joint: matches!(
            outcome,
            CrewTissueOutcome::Detachment | CrewTissueOutcome::Pulverized
        ),
        hide_mesh_part: outcome == CrewTissueOutcome::Pulverized,
        blood_cube_count: blood_cube_count(outcome, total_work_j, resistance),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinetic(part: CrewBodyPart, energy: f32) -> CrewDamageResult {
        resolve_crew_damage(CrewDamageInput {
            part,
            fragment_energy_j: energy,
            explosive_energy_j: 0.0,
            blast_distance_m: 1.0,
        })
    }

    #[test]
    fn kinetic_energy_is_monotonic_for_class_work_and_blood_count() {
        let energies = [0.0, 0.23, 0.24, 4.0, 27.9, 28.0, 55.0, 81.9, 82.0, 500.0];
        let results: Vec<_> = energies
            .into_iter()
            .map(|energy| kinetic(CrewBodyPart::LeftForearm, energy))
            .collect();
        for pair in results.windows(2) {
            assert!(pair[0].outcome <= pair[1].outcome);
            assert!(pair[0].total_work_j <= pair[1].total_work_j);
            assert!(pair[0].blood_cube_count <= pair[1].blood_cube_count);
        }
        assert_eq!(results[1].outcome, CrewTissueOutcome::None);
        assert_eq!(results[2].outcome, CrewTissueOutcome::Injury);
        assert_eq!(results[5].outcome, CrewTissueOutcome::Detachment);
        assert_eq!(results[8].outcome, CrewTissueOutcome::Pulverized);
    }

    #[test]
    fn blast_damage_increases_monotonically_as_the_part_nears_the_entry() {
        let mut previous = resolve_crew_damage(CrewDamageInput {
            part: CrewBodyPart::RightUpperArm,
            fragment_energy_j: 1.0,
            explosive_energy_j: 246_856.0, // 59 g TNT-equivalent at 4.184 kJ/g
            blast_distance_m: 4.0,
        });
        for distance in [2.0, 1.0, 0.50, 0.25, MIN_BLAST_DISTANCE_M] {
            let next = resolve_crew_damage(CrewDamageInput {
                blast_distance_m: distance,
                ..CrewDamageInput {
                    part: CrewBodyPart::RightUpperArm,
                    fragment_energy_j: 1.0,
                    explosive_energy_j: 246_856.0,
                    blast_distance_m: 4.0,
                }
            });
            assert!(next.blast_work_j >= previous.blast_work_j);
            assert!(next.total_work_j >= previous.total_work_j);
            assert!(next.outcome >= previous.outcome);
            assert!(next.blood_cube_count >= previous.blood_cube_count);
            previous = next;
        }
    }

    #[test]
    fn distal_limb_fails_before_thigh_and_core_does_not_detach_as_a_block() {
        assert_eq!(
            kinetic(CrewBodyPart::LeftForearm, 50.0).outcome,
            CrewTissueOutcome::Detachment
        );
        assert_eq!(
            kinetic(CrewBodyPart::LeftThigh, 50.0).outcome,
            CrewTissueOutcome::Injury
        );
        assert_eq!(
            kinetic(CrewBodyPart::Torso, 700.0).outcome,
            CrewTissueOutcome::Injury
        );
        assert_eq!(
            kinetic(CrewBodyPart::Torso, 1_250.0).outcome,
            CrewTissueOutcome::Pulverized
        );
    }

    #[test]
    fn mesh_and_joint_flags_match_the_anatomical_outcome() {
        let wound = kinetic(CrewBodyPart::Head, 2.0);
        assert!(!wound.detach_joint && !wound.hide_mesh_part);

        let detached = kinetic(CrewBodyPart::Head, 80.0);
        assert!(detached.detach_joint && !detached.hide_mesh_part);

        let destroyed = kinetic(CrewBodyPart::Head, 230.0);
        assert!(destroyed.detach_joint && destroyed.hide_mesh_part);
    }

    #[test]
    fn blood_requests_fit_the_existing_equal_cube_pool() {
        for part in CrewBodyPart::ALL {
            for energy in [0.0, 0.24, 10.0, 50.0, 150.0, 500.0, 2_000.0, f32::INFINITY] {
                let result = kinetic(part, energy);
                assert!(result.blood_cube_count <= MAX_BLOOD_CUBES_PER_EVENT);
                assert_eq!(result.blood_cube_count % BLOOD_COUNT_QUANTUM, 0);
                assert_eq!(
                    result.blood_cube_count == 0,
                    result.outcome == CrewTissueOutcome::None
                );
            }
        }
    }

    #[test]
    fn invalid_inputs_are_safe_and_do_not_manufacture_damage() {
        let result = resolve_crew_damage(CrewDamageInput {
            part: CrewBodyPart::Pelvis,
            fragment_energy_j: f32::NAN,
            explosive_energy_j: -5.0,
            blast_distance_m: f32::NAN,
        });
        assert_eq!(result.outcome, CrewTissueOutcome::None);
        assert_eq!(result.total_work_j, 0.0);
        assert_eq!(result.blood_cube_count, 0);
    }

    #[test]
    fn swept_fragment_hits_the_correct_rotated_limb_at_the_earliest_contact() {
        let volume = CrewHitVolume::new(
            CrewBodyPart::LeftForearm,
            Vec3::new(0.35, 0.8, -0.1),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(0.06, 0.185, 0.064),
        );
        let fraction = swept_sphere_obb_fraction(
            Vec3::new(-0.5, 0.8, -0.1),
            Vec3::new(0.8, 0.8, -0.1),
            0.003,
            volume,
        )
        .expect("the sweep should cross the rotated forearm");
        let wound = Vec3::new(-0.5, 0.8, -0.1).lerp(Vec3::new(0.8, 0.8, -0.1), fraction);
        assert!((wound.x - 0.162).abs() < 0.002, "wound={wound:?}");
    }

    #[test]
    fn anatomical_sweep_misses_empty_space_and_accepts_a_start_inside() {
        let volume = CrewHitVolume::new(
            CrewBodyPart::Head,
            Vec3::new(0.0, 1.4, 0.0),
            Quat::IDENTITY,
            Vec3::splat(0.12),
        );
        assert_eq!(
            swept_sphere_obb_fraction(
                Vec3::new(-0.5, 1.8, 0.0),
                Vec3::new(0.5, 1.8, 0.0),
                0.004,
                volume,
            ),
            None
        );
        assert_eq!(
            swept_sphere_obb_fraction(volume.center, volume.center + Vec3::X, 0.004, volume,),
            Some(0.0)
        );
    }
}
