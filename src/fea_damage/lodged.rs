//! Lodged rounds and fragments: whether a stopped body is held, the force that holds it, and how a
//! crewman gets it out. docs/FEA_DAMAGE_MODEL_20261102.md, "Stage 3".
//!
//! Physics, each a named reduced model:
//! * A body embedded only by its tapered nose is not held by friction: the elastic recovery of the
//!   cavity pushes it back out when the taper's half-angle exceeds the friction angle
//!   (`tan α > μ`), which holds for every ogive and cap here. It is retained only when the
//!   cylindrical body behind the nose went into the hole (`depth > nose length`).
//! * The grip on the retained length is Coulomb friction on the residual contact pressure of the
//!   recovered cavity: `F = μ · p · π d · (depth − nose)`. The lattice supplies `p` from its final
//!   contact forces; before it runs, a named fraction of the struck material's yield stands in.
//! * Removal: one man's sustained two-handed pull, the toolbox bar at its lever ratio, or blows of
//!   a sledge on a drift, each delivering a stated share of its kinetic energy to the sliding work
//!   `F × slide`. Otherwise a workshop job.

use serde::{Deserialize, Serialize};

use crate::ballistics::ShellKind;

/// Static friction of an oxidised steel body in a steel bore (named estimate within the handbook
/// range for dry, scaled steel on steel).
pub const FRICTION_STEEL: f32 = 0.3;
/// Residual contact pressure over yield of the struck material when no lattice solve supplies it
/// (named stage-1 estimate).
pub const RESIDUAL_GRIP_FRACTION: f32 = 0.05;
/// Sustained two-handed pull of one man, N (named ergonomic estimate).
pub const HAND_PULL_N: f32 = 400.0;
/// Lever ratio of the toolbox bar (named estimate).
pub const BAR_ADVANTAGE: f32 = 10.0;
/// Kinetic energy of one sledge blow: a 5 kg head at 8 m/s, J.
pub const SLEDGE_BLOW_J: f32 = 160.0;
/// Share of a blow that does sliding work on the stuck body (the rest rings and deforms; named).
pub const SLEDGE_EFFICIENCY: f32 = 0.5;
/// More blows than this is not a field job.
pub const MAX_FIELD_BLOWS: u32 = 60;
/// A sledge blow cannot shift a body whose grip exceeds this (the drift's peak force over the
/// few millimetres a blow can drive it; named).
pub const MAX_BLOW_GRIP_N: f32 = 250_000.0;

/// Length of the nose taper in calibres, by projectile kind (named estimates from typical WWII
/// profiles: a pointed AP ogive, a blunter armour-piercing cap, a sub-calibre carrier's nose).
pub fn nose_length_calibres(kind: ShellKind) -> f32 {
    match kind {
        ShellKind::Ap => 1.2,
        ShellKind::Apcbc => 1.0,
        ShellKind::Apcr => 1.0,
        ShellKind::He => 0.9,
        ShellKind::Heat => 0.5,
    }
}

/// A body of nose length `nose_m` that stopped `depth_m` into a hole is retained by its bore.
pub fn retained(depth_m: f32, nose_m: f32) -> bool {
    depth_m > nose_m
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Removal {
    /// Pulled by hand.
    Hand,
    /// Levered out with the toolbox bar.
    Bar,
    /// Driven out with this many sledge blows on a drift.
    Blows(u32),
    /// Beyond field tools, or not reachable without dismantling: a workshop job.
    Workshop,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Extraction {
    /// Force to start it moving, N.
    pub force_n: f32,
    /// Distance it must slide against that grip, m.
    pub slide_m: f32,
    /// Sliding work, J.
    pub work_j: f32,
    pub removal: Removal,
}

/// The extraction of a body of diameter `diameter_m` held over `held_m` of its length by a contact
/// pressure `grip_pa`. `reachable` is false when it sits inside machinery.
pub fn extraction(grip_pa: f32, diameter_m: f32, held_m: f32, reachable: bool) -> Extraction {
    let held = held_m.max(0.0);
    let force = FRICTION_STEEL * grip_pa.max(0.0) * std::f32::consts::PI * diameter_m.max(0.0) * held;
    let work = force * held;
    let removal = if !reachable {
        Removal::Workshop
    } else if force <= HAND_PULL_N {
        Removal::Hand
    } else if force <= HAND_PULL_N * BAR_ADVANTAGE {
        Removal::Bar
    } else if force <= MAX_BLOW_GRIP_N {
        let blows = (work / (SLEDGE_BLOW_J * SLEDGE_EFFICIENCY)).ceil().max(1.0) as u32;
        if blows <= MAX_FIELD_BLOWS { Removal::Blows(blows) } else { Removal::Workshop }
    } else {
        Removal::Workshop
    };
    Extraction { force_n: force, slide_m: held, work_j: work, removal }
}

/// Extraction from the lattice's measured normal grip on the body (`Lattice::body_grip_normal_n`).
pub fn extraction_from_grip(grip_normal_n: f32, held_m: f32, reachable: bool) -> Extraction {
    let held = held_m.max(0.0);
    let force = FRICTION_STEEL * grip_normal_n.max(0.0);
    let work = force * held;
    let removal = if !reachable {
        Removal::Workshop
    } else if force <= HAND_PULL_N {
        Removal::Hand
    } else if force <= HAND_PULL_N * BAR_ADVANTAGE {
        Removal::Bar
    } else if force <= MAX_BLOW_GRIP_N {
        let blows = (work / (SLEDGE_BLOW_J * SLEDGE_EFFICIENCY)).ceil().max(1.0) as u32;
        if blows <= MAX_FIELD_BLOWS { Removal::Blows(blows) } else { Removal::Workshop }
    } else {
        Removal::Workshop
    };
    Extraction { force_n: force, slide_m: held, work_j: work, removal }
}

/// Stage-1 grip before a lattice solve: a named fraction of the struck material's yield.
pub fn estimated_grip_pa(yield_stress_pa: f32) -> f32 {
    RESIDUAL_GRIP_FRACTION * yield_stress_pa.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nose_only_embed_is_not_held_and_a_body_embed_is() {
        let d = 0.045;
        let nose = nose_length_calibres(ShellKind::Ap) * d;
        assert!(!retained(0.8 * nose, nose));
        assert!(retained(nose + 0.002, nose));
    }

    #[test]
    fn removal_escalates_with_grip_and_is_never_free() {
        let d = 0.076;
        let light = extraction(1.0e4, d, 0.01, true);
        let medium = extraction(1.0e6, d, 0.01, true);
        let heavy = extraction(4.0e7, d, 0.02, true);
        let stuck = extraction(8.0e8, d, 0.06, true);
        assert_eq!(light.removal, Removal::Hand);
        assert_eq!(medium.removal, Removal::Bar);
        assert!(matches!(heavy.removal, Removal::Blows(n) if n >= 1));
        assert_eq!(stuck.removal, Removal::Workshop);
        assert!(light.force_n < medium.force_n && medium.force_n < heavy.force_n);
        assert_eq!(extraction(1.0e4, d, 0.01, false).removal, Removal::Workshop);
        assert!(heavy.work_j > 0.0 && heavy.slide_m == 0.02);
    }
}
