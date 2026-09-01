//! Articulated constrained-body math for retained Tiger crew casualties.
//!
//! A live casualty is driven by the exact fragment contact retained by `game.rs`. The contact is
//! mapped to one anatomical segment and transfers its measured momentum there. If the shell has an
//! explosive filler, a separate confined-air impulse is derived from the modeled filler energy and
//! the body's distance from the penetration point. The solver never picks an authored throw or
//! animation. Eleven independently moving rigid segments are joined into a limp body with
//! ball-joint anchor constraints; `game.rs` supplies the Tiger compartment contacts.

use glam::{Quat, Vec3};

pub const BODY_MASS_KG: f32 = 78.0;
pub const AIR_LINEAR_DAMPING_S: f32 = 0.34;
pub const AIR_SWING_DAMPING_S: f32 = 1.35;
pub const LONG_AXIS_TWIST_DAMPING_S: f32 = 11.0;
pub const CREW_BODY_PART_COUNT: usize = 11;
const SPEED_OF_SOUND_MPS: f32 = 343.0;
const BODY_PROJECTED_AREA_M2: f32 = 0.42;
/// A turret is not free field: after the direct front crosses a body, short-path reflections from
/// the roof, wall and breech load it again from nearly the same side before the pressure can vent.
/// This bounded multiplier represents those repeated impulses. Even at saturation the resulting
/// 78 kg body kinetic energy is only 1.4 kJ (about 0.22% of a BR-471 filler burst), and the separate
/// whole-body cap below prevents the reflection model from manufacturing an unbounded launch.
const CONFINED_BLAST_COUPLING: f32 = 3.00;
const MAX_WHOLE_BODY_IMPULSE_NS: f32 = BODY_MASS_KG * 6.0;

/// Stable anatomical IDs shared by damage, physics and render-slot ownership. Do not reorder:
/// deterministic captures and the fixed renderer slot arrays use `index()`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CrewBodyPart {
    Pelvis = 0,
    Torso = 1,
    Head = 2,
    LeftUpperArm = 3,
    LeftForearm = 4,
    RightUpperArm = 5,
    RightForearm = 6,
    LeftThigh = 7,
    LeftShin = 8,
    RightThigh = 9,
    RightShin = 10,
}

impl CrewBodyPart {
    pub const ALL: [Self; CREW_BODY_PART_COUNT] = [
        Self::Pelvis,
        Self::Torso,
        Self::Head,
        Self::LeftUpperArm,
        Self::LeftForearm,
        Self::RightUpperArm,
        Self::RightForearm,
        Self::LeftThigh,
        Self::LeftShin,
        Self::RightThigh,
        Self::RightShin,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// Segment centre in the 1.53 m relaxed casualty mesh, whose boots are at local Y=0.
    pub const fn rest_center(self) -> Vec3 {
        match self {
            Self::Pelvis => Vec3::new(0.0, 0.735, 0.0),
            Self::Torso => Vec3::new(0.0, 1.045, 0.0),
            Self::Head => Vec3::new(0.0, 1.405, 0.0),
            Self::LeftUpperArm => Vec3::new(-0.218, 1.000, 0.0),
            Self::LeftForearm => Vec3::new(-0.220, 0.635, 0.008),
            Self::RightUpperArm => Vec3::new(0.218, 1.000, 0.0),
            Self::RightForearm => Vec3::new(0.220, 0.635, 0.008),
            Self::LeftThigh => Vec3::new(-0.105, 0.520, 0.0),
            Self::LeftShin => Vec3::new(-0.100, 0.205, 0.004),
            Self::RightThigh => Vec3::new(0.105, 0.520, 0.0),
            Self::RightShin => Vec3::new(0.100, 0.205, 0.004),
        }
    }

    /// Conservative oriented-box half extents for collision and contact selection.
    pub const fn half_extents(self) -> Vec3 {
        match self {
            Self::Pelvis => Vec3::new(0.180, 0.145, 0.110),
            Self::Torso => Vec3::new(0.225, 0.270, 0.115),
            Self::Head => Vec3::new(0.125, 0.135, 0.125),
            Self::LeftUpperArm | Self::RightUpperArm => Vec3::new(0.068, 0.205, 0.072),
            Self::LeftForearm | Self::RightForearm => Vec3::new(0.060, 0.185, 0.064),
            Self::LeftThigh | Self::RightThigh => Vec3::new(0.092, 0.235, 0.096),
            Self::LeftShin | Self::RightShin => Vec3::new(0.078, 0.225, 0.085),
        }
    }

    /// Anthropometric mass fractions sum to one 78 kg crewman.
    pub const fn mass_fraction(self) -> f32 {
        match self {
            Self::Pelvis => 0.15,
            Self::Torso => 0.35,
            Self::Head => 0.07,
            Self::LeftUpperArm | Self::RightUpperArm => 0.03,
            Self::LeftForearm | Self::RightForearm => 0.02,
            Self::LeftThigh | Self::RightThigh => 0.10,
            Self::LeftShin | Self::RightShin => 0.065,
        }
    }

    pub const fn mass_kg(self) -> f32 {
        BODY_MASS_KG * self.mass_fraction()
    }
}

/// One independently moving anatomical rigid body. `present` and `detached` are deliberately part
/// of authoritative state now: the damage pass can remove a mesh or sever its joint without
/// changing the physics/render architecture. `detached` means this segment's incoming/parent joint
/// is broken: its own children remain attached and travel with it. A removed part is not simulated
/// or rendered; a detached but present branch remains physical.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewSegment {
    pub part: CrewBodyPart,
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    pub angular_vel: Vec3,
    pub present: bool,
    pub detached: bool,
    pub grounded: bool,
}

impl CrewSegment {
    pub fn inv_mass(self) -> f32 {
        if self.present {
            1.0 / self.part.mass_kg()
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactMotion {
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArticulatedImpact {
    pub hit_part: CrewBodyPart,
    pub fragment_impulse_ns: Vec3,
    pub blast_impulse_ns: Vec3,
}

#[derive(Clone, Copy)]
struct Joint {
    parent: CrewBodyPart,
    child: CrewBodyPart,
    parent_anchor: Vec3,
    child_anchor: Vec3,
}

// Anchors are expressed about each segment centre in the relaxed source pose. Ball joints are
// intentional: a KIA body has no muscular pose control, while coincident anchors prevent limbs from
// stretching or orbiting independently.
const JOINTS: [Joint; 10] = [
    Joint {
        parent: CrewBodyPart::Pelvis,
        child: CrewBodyPart::Torso,
        parent_anchor: Vec3::new(0.0, 0.085, 0.0),
        child_anchor: Vec3::new(0.0, -0.225, 0.0),
    },
    Joint {
        parent: CrewBodyPart::Torso,
        child: CrewBodyPart::Head,
        parent_anchor: Vec3::new(0.0, 0.245, 0.0),
        child_anchor: Vec3::new(0.0, -0.115, 0.0),
    },
    Joint {
        parent: CrewBodyPart::Torso,
        child: CrewBodyPart::LeftUpperArm,
        parent_anchor: Vec3::new(-0.218, 0.160, 0.0),
        child_anchor: Vec3::new(0.0, 0.205, 0.0),
    },
    Joint {
        parent: CrewBodyPart::LeftUpperArm,
        child: CrewBodyPart::LeftForearm,
        parent_anchor: Vec3::new(0.0, -0.185, 0.0),
        child_anchor: Vec3::new(0.0, 0.180, -0.008),
    },
    Joint {
        parent: CrewBodyPart::Torso,
        child: CrewBodyPart::RightUpperArm,
        parent_anchor: Vec3::new(0.218, 0.160, 0.0),
        child_anchor: Vec3::new(0.0, 0.205, 0.0),
    },
    Joint {
        parent: CrewBodyPart::RightUpperArm,
        child: CrewBodyPart::RightForearm,
        parent_anchor: Vec3::new(0.0, -0.185, 0.0),
        child_anchor: Vec3::new(0.0, 0.180, -0.008),
    },
    Joint {
        parent: CrewBodyPart::Pelvis,
        child: CrewBodyPart::LeftThigh,
        parent_anchor: Vec3::new(-0.105, -0.055, 0.0),
        child_anchor: Vec3::new(0.0, 0.160, 0.0),
    },
    Joint {
        parent: CrewBodyPart::LeftThigh,
        child: CrewBodyPart::LeftShin,
        parent_anchor: Vec3::new(0.0, -0.160, 0.0),
        child_anchor: Vec3::new(-0.005, 0.155, -0.004),
    },
    Joint {
        parent: CrewBodyPart::Pelvis,
        child: CrewBodyPart::RightThigh,
        parent_anchor: Vec3::new(0.105, -0.055, 0.0),
        child_anchor: Vec3::new(0.0, 0.160, 0.0),
    },
    Joint {
        parent: CrewBodyPart::RightThigh,
        child: CrewBodyPart::RightShin,
        parent_anchor: Vec3::new(0.0, -0.160, 0.0),
        child_anchor: Vec3::new(0.005, 0.155, -0.004),
    },
];

pub fn rest_segments(
    origin: Vec3,
    rotation: Quat,
    scale: f32,
) -> [CrewSegment; CREW_BODY_PART_COUNT] {
    std::array::from_fn(|index| {
        let part = CrewBodyPart::ALL[index];
        CrewSegment {
            part,
            pos: origin + rotation * (part.rest_center() * scale),
            rot: rotation,
            vel: Vec3::ZERO,
            angular_vel: Vec3::ZERO,
            present: true,
            detached: false,
            grounded: false,
        }
    })
}

fn place_joint_child(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    joint: Joint,
    child_rotation: Quat,
    scale: f32,
) {
    let parent = segments[joint.parent.index()];
    let child_index = joint.child.index();
    segments[child_index].rot = child_rotation;
    let anchor = parent.pos + parent.rot * (joint.parent_anchor * scale);
    segments[child_index].pos = anchor - child_rotation * (joint.child_anchor * scale);
}

/// Compact limp initial pose for Gunner, Loader and bow crew, whose living source mesh is seated.
/// This is a kinematic reconstruction of the relaxed mesh pages at the same joint anchors—not a
/// death animation. It prevents a one-frame upright pop before gravity and impact take ownership.
pub fn seated_segments(
    origin: Vec3,
    rotation: Quat,
    scale: f32,
) -> [CrewSegment; CREW_BODY_PART_COUNT] {
    let mut segments = rest_segments(origin, rotation, scale);
    let pelvis = CrewBodyPart::Pelvis.index();
    segments[pelvis].pos = origin + rotation * (Vec3::new(0.0, 0.58, 0.0) * scale);
    segments[pelvis].rot = rotation;

    let torso_rotation = rotation * Quat::from_rotation_x(0.12);
    place_joint_child(&mut segments, JOINTS[0], torso_rotation, scale);
    place_joint_child(
        &mut segments,
        JOINTS[1],
        rotation * Quat::from_rotation_x(-0.03),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[2],
        rotation * Quat::from_rotation_x(-0.15),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[3],
        rotation * Quat::from_rotation_x(-1.05),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[4],
        rotation * Quat::from_rotation_x(-0.15),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[5],
        rotation * Quat::from_rotation_x(-1.05),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[6],
        rotation * Quat::from_rotation_x(-1.15),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[7],
        rotation * Quat::from_rotation_x(0.08),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[8],
        rotation * Quat::from_rotation_x(-1.15),
        scale,
    );
    place_joint_child(
        &mut segments,
        JOINTS[9],
        rotation * Quat::from_rotation_x(0.08),
        scale,
    );
    segments
}

/// Map one point onto the closest current anatomical OBB. This is used for the retained lethal
/// fragment contact, rather than guessing a body part from damage severity or camera direction.
pub fn nearest_body_part(
    segments: &[CrewSegment; CREW_BODY_PART_COUNT],
    point: Vec3,
    scale: f32,
) -> CrewBodyPart {
    let mut best = CrewBodyPart::Torso;
    let mut best_distance = f32::INFINITY;
    for segment in segments {
        if !segment.present {
            continue;
        }
        let local = segment.rot.conjugate() * (point - segment.pos);
        let half = segment.part.half_extents() * scale;
        let outside = (local.abs() - half).max(Vec3::ZERO);
        let distance = outside.length_squared();
        if distance < best_distance {
            best_distance = distance;
            best = segment.part;
        }
    }
    best
}

/// Classify one source-mesh triangle centroid into the same stable body-part IDs used by physics.
pub fn classify_relaxed_mesh_point(point: Vec3) -> CrewBodyPart {
    if point.y >= 1.275 {
        return CrewBodyPart::Head;
    }
    if point.x < -0.155 && point.y >= 0.39 {
        return if point.y >= 0.79 {
            CrewBodyPart::LeftUpperArm
        } else {
            CrewBodyPart::LeftForearm
        };
    }
    if point.x > 0.155 && point.y >= 0.39 {
        return if point.y >= 0.79 {
            CrewBodyPart::RightUpperArm
        } else {
            CrewBodyPart::RightForearm
        };
    }
    if point.y < 0.67 {
        let left = point.x < 0.0;
        if point.y >= 0.315 {
            return if left {
                CrewBodyPart::LeftThigh
            } else {
                CrewBodyPart::RightThigh
            };
        }
        return if left {
            CrewBodyPart::LeftShin
        } else {
            CrewBodyPart::RightShin
        };
    }
    if point.y < 0.835 {
        CrewBodyPart::Pelvis
    } else {
        CrewBodyPart::Torso
    }
}

fn box_inertia(part: CrewBodyPart, scale: f32) -> Vec3 {
    let size = part.half_extents() * (2.0 * scale.max(1.0e-4));
    let mass = part.mass_kg();
    Vec3::new(
        mass * (size.y * size.y + size.z * size.z) / 12.0,
        mass * (size.x * size.x + size.z * size.z) / 12.0,
        mass * (size.x * size.x + size.y * size.y) / 12.0,
    )
}

fn fragment_impulse(contact_velocity: Vec3, contact_energy_j: f32) -> Vec3 {
    let speed = contact_velocity.length();
    if speed <= 1.0e-4 {
        return Vec3::ZERO;
    }
    contact_velocity / speed * (2.0 * contact_energy_j.max(0.0) / speed).clamp(0.0, 480.0)
}

fn mean_projected_area(part: CrewBodyPart, scale: f32) -> f32 {
    let half = part.half_extents() * scale.max(0.1);
    2.0 * (half.x * half.y + half.x * half.z + half.y * half.z)
}

/// Momentum coupled through the confined cabin atmosphere. `explosive_energy_j` is the shell
/// filler's modeled chemical energy, not fragment energy. We take the body's projected solid-angle
/// share, convert reflected pressure-wave energy to impulse (`2E/c`), then apply a bounded coupling
/// coefficient for an irregular clothed body in a steel enclosure. It falls off as 1/r².
pub fn confined_blast_impulse(
    blast_origin: Vec3,
    body_center: Vec3,
    explosive_energy_j: f32,
    body_scale: f32,
) -> Vec3 {
    let offset = body_center - blast_origin;
    let distance = offset.length().max(0.18);
    let area = BODY_PROJECTED_AREA_M2 * body_scale.max(0.1).powi(2);
    let intercepted_fraction =
        (area / (4.0 * std::f32::consts::PI * distance * distance)).min(0.36);
    let intercepted_energy = explosive_energy_j.max(0.0) * intercepted_fraction;
    offset.normalize_or_zero()
        * (2.0 * intercepted_energy / SPEED_OF_SOUND_MPS * CONFINED_BLAST_COUPLING)
            .min(MAX_WHOLE_BODY_IMPULSE_NS)
}

/// Pressure loading on each independent anatomical segment. The eleven mean projected areas are
/// normalized to the same 0.42 m² whole-body silhouette used by `confined_blast_impulse`, so splitting
/// the body cannot create energy. Distance and direction are then evaluated at each actual segment
/// centre. A nearby arm can therefore move before the torso while the joint chain remains bounded.
pub fn confined_segment_blast_impulses(
    segments: &[CrewSegment; CREW_BODY_PART_COUNT],
    blast_origin: Vec3,
    explosive_energy_j: f32,
    scale: f32,
) -> [Vec3; CREW_BODY_PART_COUNT] {
    let mut impulses = [Vec3::ZERO; CREW_BODY_PART_COUNT];
    let energy = explosive_energy_j.max(0.0);
    if energy <= 0.0 {
        return impulses;
    }

    let raw_area_sum: f32 = segments
        .iter()
        .filter(|segment| segment.present)
        .map(|segment| mean_projected_area(segment.part, scale))
        .sum();
    if raw_area_sum <= 1.0e-6 {
        return impulses;
    }
    let target_area = BODY_PROJECTED_AREA_M2 * scale.max(0.1).powi(2);
    let area_normalization = target_area / raw_area_sum;

    let mut scalar_impulse_sum = 0.0;
    for segment in segments.iter().filter(|segment| segment.present) {
        let offset = segment.pos - blast_origin;
        let distance = offset.length().max(0.18);
        let area = mean_projected_area(segment.part, scale) * area_normalization;
        let intercepted_fraction =
            (area / (4.0 * std::f32::consts::PI * distance * distance)).min(0.18);
        let impulse =
            2.0 * energy * intercepted_fraction / SPEED_OF_SOUND_MPS * CONFINED_BLAST_COUPLING;
        impulses[segment.part.index()] = offset.normalize_or_zero() * impulse;
        scalar_impulse_sum += impulse;
    }

    if scalar_impulse_sum > MAX_WHOLE_BODY_IMPULSE_NS {
        let normalization = MAX_WHOLE_BODY_IMPULSE_NS / scalar_impulse_sum;
        for impulse in &mut impulses {
            *impulse *= normalization;
        }
    }
    impulses
}

/// Deliver one retained fragment's momentum to its exact anatomical rigid body. This is shared by
/// the primary lethal contact and by additional arm/leg/torso intersections from the same shower.
pub fn apply_fragment_impulse_to_part(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    hit_part: CrewBodyPart,
    contact_point: Vec3,
    contact_velocity: Vec3,
    contact_energy_j: f32,
    scale: f32,
) -> Vec3 {
    let fragment_impulse_ns = fragment_impulse(contact_velocity, contact_energy_j);
    let segment = &mut segments[hit_part.index()];
    if !segment.present {
        return Vec3::ZERO;
    }
    segment.vel += fragment_impulse_ns / hit_part.mass_kg();
    let arm = contact_point - segment.pos;
    let angular_impulse = arm.cross(fragment_impulse_ns);
    let inertia = box_inertia(hit_part, scale);
    let local_angular_impulse = segment.rot.conjugate() * angular_impulse;
    let local_delta = Vec3::new(
        local_angular_impulse.x / inertia.x.max(1.0e-5),
        local_angular_impulse.y / inertia.y.max(1.0e-5),
        local_angular_impulse.z / inertia.z.max(1.0e-5),
    );
    segment.angular_vel += segment.rot * local_delta.clamp_length_max(18.0);
    fragment_impulse_ns
}

fn linear_kinetic_energy(segments: &[CrewSegment; CREW_BODY_PART_COUNT]) -> f32 {
    segments
        .iter()
        .filter(|segment| segment.present)
        .map(|segment| 0.5 * segment.part.mass_kg() * segment.vel.length_squared())
        .sum()
}

fn redistribute_pulverized_impulses(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    fragment_impulses_ns: &[Vec3; CREW_BODY_PART_COUNT],
    pulverized: &[bool; CREW_BODY_PART_COUNT],
    fraction: f32,
) {
    let fraction = fraction.clamp(0.0, 1.0);
    let total_mass: f32 = segments
        .iter()
        .filter(|segment| segment.present)
        .map(|segment| segment.part.mass_kg())
        .sum();
    if total_mass <= 1.0e-5 || fraction <= 0.0 {
        return;
    }

    let mut transferred_impulse = Vec3::ZERO;
    for part in CrewBodyPart::ALL {
        let index = part.index();
        if !segments[index].present || !pulverized[index] {
            continue;
        }
        let impulse = fragment_impulses_ns[index] * fraction;
        segments[index].vel -= impulse / part.mass_kg();
        transferred_impulse += impulse;
    }
    let shared_delta_v = transferred_impulse / total_mass;
    for segment in segments.iter_mut().filter(|segment| segment.present) {
        segment.vel += shared_delta_v;
    }
}

/// Couple the exact fragment momentum of parts that are about to be pulverized into their still
/// connected body before those mesh/physics pages are removed. Without this step a directly hit
/// forearm could receive the entire measured impulse and then disappear before a joint iteration,
/// leaving the remaining corpse implausibly planted.
///
/// The operation only redistributes momentum already present in `segments`: it subtracts a bounded
/// fraction from each doomed segment and gives the same vector impulse to the complete body by
/// mass. Total linear momentum is therefore unchanged. The largest fraction that does not increase
/// pre-failure linear kinetic energy is selected deterministically, so tissue failure cannot become
/// an unphysical energy source. The returned fraction is useful for regression telemetry.
pub fn couple_pulverized_fragment_impulses(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    fragment_impulses_ns: &[Vec3; CREW_BODY_PART_COUNT],
    pulverized: &[bool; CREW_BODY_PART_COUNT],
) -> f32 {
    let valid = CrewBodyPart::ALL.into_iter().all(|part| {
        let impulse = fragment_impulses_ns[part.index()];
        impulse.x.is_finite() && impulse.y.is_finite() && impulse.z.is_finite()
    });
    if !valid
        || !CrewBodyPart::ALL
            .into_iter()
            .any(|part| segments[part.index()].present && pulverized[part.index()])
        || !CrewBodyPart::ALL
            .into_iter()
            .any(|part| segments[part.index()].present && !pulverized[part.index()])
    {
        return 0.0;
    }
    let total_impulse: Vec3 = CrewBodyPart::ALL
        .into_iter()
        .filter(|part| segments[part.index()].present && pulverized[part.index()])
        .map(|part| fragment_impulses_ns[part.index()])
        .sum();
    if total_impulse.length_squared() <= 1.0e-10 {
        return 0.0;
    }

    let energy_before = linear_kinetic_energy(segments);
    if !energy_before.is_finite() {
        return 0.0;
    }
    let tolerance = 1.0e-4 + energy_before.abs() * 1.0e-5;
    let admissible = |fraction: f32| {
        let mut trial = *segments;
        redistribute_pulverized_impulses(&mut trial, fragment_impulses_ns, pulverized, fraction);
        linear_kinetic_energy(&trial) <= energy_before + tolerance
    };

    let fraction = if admissible(1.0) {
        1.0
    } else {
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..24 {
            let middle = 0.5 * (low + high);
            if admissible(middle) {
                low = middle;
            } else {
                high = middle;
            }
        }
        low
    };
    if fraction > 1.0e-6 {
        redistribute_pulverized_impulses(segments, fragment_impulses_ns, pulverized, fraction);
    }
    fraction
}

/// Apply the actual lethal contact plus the pressure impulse supplied by the same penetration.
pub fn apply_authoritative_impact(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    contact_point: Vec3,
    contact_velocity: Vec3,
    contact_energy_j: f32,
    blast_origin: Vec3,
    explosive_energy_j: f32,
    scale: f32,
) -> ArticulatedImpact {
    let hit_part = nearest_body_part(segments, contact_point, scale);
    apply_authoritative_impact_to_part(
        segments,
        hit_part,
        contact_point,
        contact_velocity,
        contact_energy_j,
        blast_origin,
        explosive_energy_j,
        scale,
    )
}

/// Exact-part variant used after the swept anatomical damage pass has already identified the first
/// OBB intersection. Keeping the legacy nearest-part wrapper above preserves old replay fixtures,
/// but live damage must not discard its more precise contact evidence and guess a second time.
#[allow(clippy::too_many_arguments)]
pub fn apply_authoritative_impact_to_part(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    hit_part: CrewBodyPart,
    contact_point: Vec3,
    contact_velocity: Vec3,
    contact_energy_j: f32,
    blast_origin: Vec3,
    explosive_energy_j: f32,
    scale: f32,
) -> ArticulatedImpact {
    let segment_blast_impulses =
        confined_segment_blast_impulses(segments, blast_origin, explosive_energy_j, scale);
    let blast_impulse_ns = segment_blast_impulses
        .iter()
        .copied()
        .fold(Vec3::ZERO, |sum, impulse| sum + impulse);
    for (segment, impulse) in segments.iter_mut().zip(segment_blast_impulses) {
        if segment.present {
            segment.vel += impulse / segment.part.mass_kg();
        }
    }
    let fragment_impulse_ns = apply_fragment_impulse_to_part(
        segments,
        hit_part,
        contact_point,
        contact_velocity,
        contact_energy_j,
        scale,
    );

    ArticulatedImpact {
        hit_part,
        fragment_impulse_ns,
        blast_impulse_ns,
    }
}

/// Advance free motion for one fixed substep. Constraint and compartment contact projection follow
/// in `game.rs`, where the correct moving hull/turret frame is known.
pub fn integrate_free_motion(segments: &mut [CrewSegment; CREW_BODY_PART_COUNT], dt: f32) {
    let dt = dt.max(0.0);
    for segment in segments.iter_mut().filter(|segment| segment.present) {
        segment.grounded = false;
        segment.vel *= (-AIR_LINEAR_DAMPING_S * dt).exp();
        segment.angular_vel = damp_angular_velocity(segment.angular_vel, segment.rot * Vec3::Y, dt);
        segment.vel.y -= 9.81 * dt;
        segment.pos += segment.vel * dt;
        let spin = segment.angular_vel.length();
        if spin > 1.0e-5 {
            segment.rot = (Quat::from_axis_angle(segment.angular_vel / spin, spin * dt)
                * segment.rot)
                .normalize();
        }
    }
}

fn rotate_by_vector(rotation: Quat, vector: Vec3) -> Quat {
    let angle = vector.length().min(0.30);
    if angle <= 1.0e-6 {
        rotation
    } else {
        (Quat::from_axis_angle(vector / vector.length(), angle) * rotation).normalize()
    }
}

/// Project all live joint anchors together. Detached or removed parts deliberately opt out.
pub fn solve_joint_constraints(
    segments: &mut [CrewSegment; CREW_BODY_PART_COUNT],
    scale: f32,
    dt: f32,
    iterations: usize,
) {
    let before_pos: [Vec3; CREW_BODY_PART_COUNT] = std::array::from_fn(|i| segments[i].pos);
    let scale = scale.max(1.0e-4);
    for _ in 0..iterations.max(1) {
        for joint in JOINTS {
            let ai = joint.parent.index();
            let bi = joint.child.index();
            if !segments[ai].present || !segments[bi].present || segments[bi].detached {
                continue;
            }
            let ra = segments[ai].rot * (joint.parent_anchor * scale);
            let rb = segments[bi].rot * (joint.child_anchor * scale);
            let error = (segments[bi].pos + rb) - (segments[ai].pos + ra);
            let wa = segments[ai].inv_mass();
            let wb = segments[bi].inv_mass();
            let weight = (wa + wb).max(1.0e-6);
            let correction = error * 0.72;
            let move_a = correction * (wa / weight);
            let move_b = -correction * (wb / weight);
            segments[ai].pos += move_a;
            segments[bi].pos += move_b;

            let ra_len2 = ra.length_squared().max(0.0025 * scale * scale);
            let rb_len2 = rb.length_squared().max(0.0025 * scale * scale);
            segments[ai].rot =
                rotate_by_vector(segments[ai].rot, ra.cross(move_a) / ra_len2 * 0.34);
            segments[bi].rot =
                rotate_by_vector(segments[bi].rot, rb.cross(move_b) / rb_len2 * 0.34);

            // This is a ball joint: it constrains the two attachment points, not the relative
            // orientation of the connected bones.  The previous implementation averaged angular
            // velocity here on every position-solver iteration.  At high-speed cabin substepping
            // that could run 128 times in one rendered frame and rapidly turn eleven independent
            // bodies back into one rotating mannequin.  Per-segment air/tissue damping is already
            // time based in `integrate_free_motion`; never make it iteration-count dependent here.
        }
    }

    if dt > 1.0e-5 {
        for (index, segment) in segments.iter_mut().enumerate() {
            if segment.present {
                let constraint_velocity = (segment.pos - before_pos[index]) / dt;
                segment.vel += constraint_velocity * 0.62;
                segment.vel = segment.vel.clamp_length_max(18.0);
                segment.angular_vel = segment.angular_vel.clamp_length_max(18.0);
            }
        }
    }
}

pub fn max_joint_error(segments: &[CrewSegment; CREW_BODY_PART_COUNT], scale: f32) -> f32 {
    let mut max_error: f32 = 0.0;
    for joint in JOINTS {
        let a = segments[joint.parent.index()];
        let b = segments[joint.child.index()];
        if !a.present || !b.present || b.detached {
            continue;
        }
        let pa = a.pos + a.rot * (joint.parent_anchor * scale);
        let pb = b.pos + b.rot * (joint.child_anchor * scale);
        max_error = max_error.max(pa.distance(pb));
    }
    max_error
}

pub fn center_of_mass(segments: &[CrewSegment; CREW_BODY_PART_COUNT]) -> Vec3 {
    let mut weighted = Vec3::ZERO;
    let mut mass = 0.0;
    for segment in segments.iter().filter(|segment| segment.present) {
        let m = segment.part.mass_kg();
        weighted += segment.pos * m;
        mass += m;
    }
    if mass > 1.0e-5 {
        weighted / mass
    } else {
        Vec3::ZERO
    }
}

pub fn center_of_mass_velocity(segments: &[CrewSegment; CREW_BODY_PART_COUNT]) -> Vec3 {
    let mut weighted = Vec3::ZERO;
    let mut mass = 0.0;
    for segment in segments.iter().filter(|segment| segment.present) {
        let m = segment.part.mass_kg();
        weighted += segment.vel * m;
        mass += m;
    }
    if mass > 1.0e-5 {
        weighted / mass
    } else {
        Vec3::ZERO
    }
}

/// A corpse may sleep only after its central mass has actually come down onto a supported pose.
/// A hand or shin touching the floor while the torso is still upright is not a stable rest state.
/// A single surviving central remnant is allowed to settle by itself after catastrophic tissue
/// loss, since there is no remaining joint chain whose support polygon can be evaluated.
pub fn has_stable_floor_support(segments: &[CrewSegment; CREW_BODY_PART_COUNT]) -> bool {
    let present_count = segments.iter().filter(|segment| segment.present).count();
    let grounded_count = segments
        .iter()
        .filter(|segment| segment.present && segment.grounded)
        .count();
    if present_count == 0 {
        return false;
    }
    if present_count == 1 {
        return grounded_count == 1;
    }
    let central_grounded = [CrewBodyPart::Pelvis, CrewBodyPart::Torso]
        .into_iter()
        .any(|part| {
            let segment = segments[part.index()];
            segment.present && segment.grounded
        });
    central_grounded && grounded_count >= 2
}

pub fn maximum_surface_speed(segments: &[CrewSegment; CREW_BODY_PART_COUNT], scale: f32) -> f32 {
    segments
        .iter()
        .filter(|segment| segment.present)
        .map(|segment| {
            segment.vel.length()
                + segment.angular_vel.length() * segment.part.half_extents().length() * scale
        })
        .fold(0.0, f32::max)
}

/// Legacy whole-body contact retained for focused regression tests and old replay compatibility.
pub fn impact_motion(
    half_extents: Vec3,
    body_axis: Vec3,
    center: Vec3,
    contact_point: Vec3,
    contact_velocity: Vec3,
    contact_energy_j: f32,
    collapse_axis: Vec3,
) -> ImpactMotion {
    let impulse = fragment_impulse(contact_velocity, contact_energy_j);
    let linear_velocity = impulse / BODY_MASS_KG;

    let half_extents = half_extents.max(Vec3::splat(1.0e-4));
    let size = half_extents * 2.0;
    let inertia = Vec3::new(
        BODY_MASS_KG * (size.y * size.y + size.z * size.z) / 12.0,
        BODY_MASS_KG * (size.x * size.x + size.z * size.z) / 12.0,
        BODY_MASS_KG * (size.x * size.x + size.y * size.y) / 12.0,
    );
    let arm = (contact_point - center).clamp(-half_extents, half_extents);
    let angular_impulse = arm.cross(impulse);
    let mut angular_velocity = Vec3::new(
        angular_impulse.x / inertia.x.max(1.0e-4),
        angular_impulse.y / inertia.y.max(1.0e-4),
        angular_impulse.z / inertia.z.max(1.0e-4),
    );

    let body_axis = body_axis.normalize_or_zero();
    angular_velocity -= body_axis * angular_velocity.dot(body_axis);
    let collapse_nudge = 0.24 + (contact_energy_j / 120.0).clamp(0.0, 1.0) * 0.22;
    angular_velocity = (angular_velocity + collapse_axis.normalize_or_zero() * collapse_nudge)
        .clamp_length_max(7.5);

    ImpactMotion {
        linear_velocity,
        angular_velocity,
    }
}

/// Air damping is anisotropic: legitimate swing remains, unsupported long-axis spin dies quickly.
pub fn damp_angular_velocity(angular_velocity: Vec3, body_axis: Vec3, dt: f32) -> Vec3 {
    let body_axis = body_axis.normalize_or_zero();
    let twist = body_axis * angular_velocity.dot(body_axis);
    let swing = angular_velocity - twist;
    let dt = dt.max(0.0);
    swing * (-AIR_SWING_DAMPING_S * dt).exp() + twist * (-LONG_AXIS_TWIST_DAMPING_S * dt).exp()
}

/// Legacy whole-body toppling helper retained for deterministic replay compatibility tests.
pub fn topple_acceleration(body_axis: Vec3, fallback_axis: Vec3, body_length: f32) -> Vec3 {
    let up = body_axis.normalize_or_zero();
    let upright = up.dot(Vec3::Y).clamp(-1.0, 1.0);
    if upright.abs() <= 0.30 {
        return Vec3::ZERO;
    }
    let lean = (1.0 - upright * upright).max(0.0).sqrt();
    let axis = if lean > 0.025 {
        up.cross(Vec3::NEG_Y).normalize_or_zero()
    } else {
        fallback_axis.normalize_or_zero()
    };
    let alpha = (1.5 * 9.8 / body_length.max(0.5)) * lean.max(0.035);
    axis * alpha
}
