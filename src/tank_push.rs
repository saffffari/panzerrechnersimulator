//! The Tiger's push against what stands in its way (2026-09-29): every obstacle gives way by its
//! own strength against the tank's real push, or holds, and the tank's speed pays for it.
//!
//! **The push.** The Tiger's tractive effort is the smaller of what the engine delivers through
//! the gears and what the tracks can hold on the ground:
//! - engine: `DRIVE_EFFICIENCY x 478 kW / v`, with v no lower than the first gear's speed at rated
//!   revolutions (2.8 km/h, docs/TIGER_I_SPEC.md), which gives 369 kN at a crawl;
//! - ground: `mu x m g cos(slope)`, with the tractive coefficient `mu` = 0.5 for steel tracks on
//!   frozen, compacted ground [I: J. Y. Wong, *Theory of Ground Vehicles*, 4th ed. (Wiley 2008),
//!   ch. 2-3, gives 0.3-0.7 for tracked vehicles from snow to firm soil; the middle is taken],
//!   281 kN for the 57.25 t tank on the level.
//! So at a crawl the tracks, not the engine, limit the push; at speed the engine does.
//!
//! **What resists** (each an engineering estimate [I] from a named material strength):
//! - a tree bends until its trunk breaks: the moment `MOR x pi d^3 / 32` of a green trunk of
//!   diameter d (modulus of rupture 40 MPa: US Forest Products Laboratory, *Wood Handbook*
//!   FPL-GTR-282 (2021), table 5-3a, green wood of the common broadleaf and pine species 34-50 MPa),
//!   pushed at the bow's height;
//! - a free-standing brick wall or pier (a garden or yard wall, a gate pier, a wall stub) bends and
//!   overturns about its base: its flexural tensile strength (0.1 MPa, EN 1996-1-1 table 3.6 for
//!   general-purpose mortar, the lower end for old lime mortar) over its section plus its own
//!   weight's restoring moment;
//! - a load-bearing brick wall held by its floors and cross walls is punched through where the bow
//!   meets it: its shear strength (0.15 MPa, EN 1996-1-1 table 3.4, 0.1-0.3 MPa) over the thickness
//!   times the perimeter of the bow's contact;
//! - timber boards, a timber pole, sheet iron: their bending strength, small against the push;
//! - a floor slab, landing or stair met edge-on slides off its bearing against friction on its own
//!   weight (0.6, masonry on masonry);
//! - bunker concrete does not give way.
//! An obstacle absorbs the work `F x delta` to fail, `delta` the travel it takes (a wall is driven
//! through its own thickness, a trunk bends 0.3 m before it snaps). While the tank presses
//! on, its speed follows `m dv/dt = F_push(v) - F_resist - rolling - m g sin(slope)`: one weaker than the push
//! is ploughed through at a slower speed; a stronger one takes the tank's kinetic energy and stops
//! it unless that energy (a ram) completes the work first. What the obstacle absorbed stays
//! absorbed (a second ram finishes a cracked wall). Nothing here is a scripted stop.

/// Combat weight (kg): `tank_support::TIGER_MASS_KG`.
pub const MASS_KG: f32 = crate::tank_support::TIGER_MASS_KG;
const G: f32 = 9.81;
/// First gear at rated revolutions: 2.8 km/h (docs/TIGER_I_SPEC.md).
pub const FIRST_GEAR_M_S: f32 = 2.8 / 3.6;
/// Tractive coefficient of the tracks on frozen, compacted ground [I] (Wong, 0.3-0.7).
pub const TRACTION_MU: f32 = 0.5;
/// Modulus of rupture of green trunk wood (Pa) [I] (Wood Handbook, 34-50 MPa).
pub const WOOD_MOR_PA: f32 = 40.0e6;
/// Flexural tensile strength of old brick masonry (Pa) [I] (EN 1996-1-1, table 3.6).
pub const BRICK_FLEXURAL_PA: f32 = 0.1e6;
/// Shear strength of old brick masonry (Pa) [I] (EN 1996-1-1, table 3.4, 0.1-0.3 MPa).
pub const BRICK_SHEAR_PA: f32 = 0.15e6;
/// Density of brickwork (kg/m^3).
pub const BRICK_DENSITY: f32 = 1800.0;
/// Density of concrete and stone (kg/m^3).
pub const CONCRETE_DENSITY: f32 = 2400.0;
/// Friction of a slab or stair on its masonry bearing [I].
pub const BEARING_FRICTION: f32 = 0.6;
/// Height above the ground where the bow meets an upright obstacle: the top of the Tiger's lower
/// nose plate, about 1.2 m [I from the baked hull].
pub const BOW_CONTACT_H_M: f32 = 1.2;
/// The bow's contact on a wall: the hull front's width and the height of its front plates.
pub const BOW_WIDTH_M: f32 = 3.4;
pub const BOW_HEIGHT_M: f32 = 1.6;
/// A wall holds its force until the bow has driven the punched or toppled piece through its own
/// thickness (never less than this) [I]; a trunk bends this far before it snaps [I].
pub const WALL_CRUSH_M: f32 = 0.10;
pub const TRUNK_BEND_M: f32 = 0.30;
/// A timber board, sheet or pole's bending resistance at the bow and its failure travel [I].
pub const TIMBER_RESIST_N: f32 = 30_000.0;
pub const TIMBER_BEND_M: f32 = 0.30;

/// The Tiger's tractive effort (N) at `speed` (m/s) on a slope `pitch` (radians).
pub fn tractive_force(speed: f32, pitch: f32) -> f32 {
    let engine = crate::tank_support::DRIVE_EFFICIENCY * crate::tank_support::TIGER_ENGINE_W
        / speed.abs().max(FIRST_GEAR_M_S);
    let grip = TRACTION_MU * MASS_KG * G * pitch.cos().max(0.0);
    engine.min(grip)
}

/// What the drive has left to push with at `speed` on a slope `pitch` (positive uphill in the
/// direction of travel): the tractive effort less rolling resistance and the weight's pull down
/// the slope.
pub fn drive_force(speed: f32, pitch: f32) -> f32 {
    tractive_force(speed, pitch) - rolling_force(pitch) - MASS_KG * G * pitch.sin()
}

/// Rolling resistance of the running gear (N) (`tank_support::ROLLING_RESISTANCE`).
pub fn rolling_force(pitch: f32) -> f32 {
    crate::tank_support::ROLLING_RESISTANCE * MASS_KG * G * pitch.cos().max(0.0)
}

/// What an obstacle offers the bow: the force it holds against the push and the work it absorbs
/// before it fails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resistance {
    pub force_n: f32,
    pub work_j: f32,
}

impl Resistance {
    pub const fn new(force_n: f32, travel_m: f32) -> Self {
        Self { force_n, work_j: force_n * travel_m }
    }

    /// Does not give way at all (bunker concrete).
    pub const UNYIELDING: Resistance = Resistance { force_n: f32::INFINITY, work_j: f32::INFINITY };
}

/// A trunk of diameter `d` (m) pushed at the bow's height.
pub fn tree(d: f32) -> Resistance {
    let moment = WOOD_MOR_PA * std::f32::consts::PI * d.max(0.01).powi(3) / 32.0;
    Resistance::new(moment / BOW_CONTACT_H_M, TRUNK_BEND_M)
}

/// A free-standing brick wall or pier of thickness `t`, width `w` and height `h` (m) pushed over
/// about its base at the bow's height (or its top, if lower).
pub fn free_standing_brick(t: f32, w: f32, h: f32) -> Resistance {
    let (t, w, h) = (t.max(0.05), w.max(0.05), h.max(0.05));
    let lever = BOW_CONTACT_H_M.min(h).max(0.3);
    let section = BRICK_FLEXURAL_PA * w.min(BOW_WIDTH_M.max(t)) * t * t / 6.0;
    let weight = BRICK_DENSITY * t * w * h * G * t * 0.5;
    Resistance::new((section + weight) / lever, t.max(WALL_CRUSH_M))
}

/// A load-bearing brick wall of thickness `t`, held by the floors and walls it carries, which the
/// bow meets over `w` by `h` of its face: the bow punches a plug the size of its front out of the
/// wall, sheared round its perimeter through the thickness; a wall piece (a module of the grammar)
/// holds the share of the plug's perimeter that its share of the bow's contact area carries, so
/// the pieces a bow straddles hold one plug between them.
pub fn restrained_brick(t: f32, w: f32, h: f32) -> Resistance {
    restrained(BRICK_SHEAR_PA, t, w, h)
}

fn restrained(shear_pa: f32, t: f32, w: f32, h: f32) -> Resistance {
    let plug = 2.0 * (BOW_WIDTH_M + BOW_HEIGHT_M);
    let share = (w.clamp(0.0, BOW_WIDTH_M) * h.clamp(0.0, BOW_HEIGHT_M)) / (BOW_WIDTH_M * BOW_HEIGHT_M);
    // Past the peak the sheared plug softens: on average half the peak over the thickness [I].
    Resistance::new(shear_pa * plug * share.max(0.05) * t.max(0.05), 0.5 * t.max(2.0 * WALL_CRUSH_M))
}

/// Shear strength of the period's plain and lightly reinforced concrete (Pa) [I] (about the design
/// tensile strength of a C12-C16 concrete, 0.8-1.2 MPa).
pub const CONCRETE_SHEAR_PA: f32 = 1.0e6;

/// A load-bearing concrete wall or a stone plinth held in the building, punched through like
/// [`restrained_brick`] at the concrete's shear strength.
pub fn restrained_concrete(t: f32, w: f32, h: f32) -> Resistance {
    restrained(CONCRETE_SHEAR_PA, t, w, h)
}

/// A loose stone or concrete block (a flight of front steps, a kerb) of the given size pushed over
/// or along: its weight's restoring moment about its toe, or its sliding friction, whichever is
/// less.
pub fn loose_block(t: f32, w: f32, h: f32) -> Resistance {
    let weight = CONCRETE_DENSITY * t.max(0.05) * w.max(0.05) * h.max(0.05) * G;
    let tip = weight * 0.5 * t.max(0.05) / BOW_CONTACT_H_M.min(h.max(0.05)).max(0.3);
    Resistance::new(tip.min(BEARING_FRICTION * weight), 0.25)
}

/// A slab, landing or stair of volume `v` (m^3) met edge-on: pushed off its bearing.
pub fn slab_off_bearing(volume_m3: f32, density: f32) -> Resistance {
    Resistance::new(BEARING_FRICTION * density * volume_m3.max(0.0) * G, 0.25)
}

/// Timber boards, a pole or sheet iron.
pub const TIMBER: Resistance = Resistance { force_n: TIMBER_RESIST_N, work_j: TIMBER_RESIST_N * TIMBER_BEND_M };

/// One frame of the tank pressing on an obstacle: the tank's speed after `dt` and the work the
/// obstacle absorbed. `speed` is the signed speed along the push, `absorbed` what the obstacle had
/// already taken. The obstacle holds its force over the travel; the tank's momentum and its push
/// work against it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PushStep {
    pub speed: f32,
    pub work_j: f32,
    pub gave_way: bool,
}

pub fn press(speed: f32, pitch: f32, obstacle: Resistance, absorbed: f32, dt: f32) -> PushStep {
    let dir = if speed >= 0.0 { 1.0 } else { -1.0 };
    let v = speed.abs();
    if dt <= 0.0 || v <= 0.0 {
        return PushStep { speed, work_j: 0.0, gave_way: false };
    }
    if !obstacle.force_n.is_finite() {
        // Unyielding: the bow stops at it and nothing is deformed.
        return PushStep { speed: 0.0, work_j: 0.0, gave_way: false };
    }
    let remaining = (obstacle.work_j - absorbed).max(0.0);
    let net = drive_force(v, pitch) - obstacle.force_n;
    if net <= 0.0 && v < STATIC_CREEP_M_S {
        // Pressing against something stronger than the push with no momentum to speak of: static
        // equilibrium, nothing moves and nothing is deformed (the drive's creep from rest is not
        // energy the obstacle can take).
        return PushStep { speed: 0.0, work_j: 0.0, gave_way: false };
    }
    // Work the obstacle takes over this frame's travel at the current speed.
    let travel = v * dt;
    let take = obstacle.force_n * travel;
    if take >= remaining {
        // It gives way within the frame: the tank loses the remaining work's worth of energy.
        let push = drive_force(v, pitch) * (remaining / obstacle.force_n.max(1.0));
        let energy = 0.5 * MASS_KG * v * v + push - remaining;
        let after = (2.0 * energy.max(0.0) / MASS_KG).sqrt();
        return PushStep { speed: dir * after, work_j: remaining, gave_way: true };
    }
    let accel = net / MASS_KG;
    let after = v + accel * dt;
    // Only the travel actually made deforms the obstacle: to rest, v^2 / 2|a|.
    let travel = if after < 0.0 { v * v / (2.0 * -accel) } else { 0.5 * (v + after) * dt };
    let work = obstacle.force_n * travel;
    PushStep { speed: dir * after.max(0.0), work_j: work.min(remaining), gave_way: false }
}

/// Below this speed a tank pressing on something stronger than its push is at rest against it.
pub const STATIC_CREEP_M_S: f32 = 0.15;

/// Tractive coefficient lost and drag gained when a share of the weight rests on the belly: the
/// tracks carry only what the belly does not (their grip falls with it) and the belly slides on
/// what carries it (steel on brick, rubble or frozen ground, 0.5 [I]).
pub const BELLY_FRICTION: f32 = 0.5;

/// The drag of a hull resting `share` of its weight on its belly, as an obstacle that never gives
/// way: the grip the tracks lost plus the belly's friction.
pub fn belly_drag(share: f32, pitch: f32) -> Resistance {
    let weight = MASS_KG * G * pitch.cos().max(0.0) * share.clamp(0.0, 1.0);
    Resistance { force_n: (TRACTION_MU + BELLY_FRICTION) * weight, work_j: f32::INFINITY }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_is_grip_limited_at_a_crawl_and_power_limited_at_speed() {
        let crawl = tractive_force(0.5, 0.0);
        assert!((crawl - TRACTION_MU * MASS_KG * G).abs() < 1.0, "{crawl}");
        assert!((270e3..290e3).contains(&crawl));
        // At 10 m/s the engine gives 0.6 x 478 kW / 10 m/s = 28.7 kN.
        assert!((tractive_force(10.0, 0.0) - 28_680.0).abs() < 50.0);
    }

    #[test]
    fn small_trees_fall_and_only_a_big_trunk_holds_the_tiger() {
        let push = tractive_force(0.0, 0.0) - rolling_force(0.0);
        for (d, falls) in [(0.08, true), (0.15, true), (0.25, true), (0.35, true), (0.45, false), (0.6, false)] {
            assert_eq!(tree(d).force_n < push, falls, "trunk {d} m: {} N against {push} N", tree(d).force_n);
        }
        // A 15 cm tree costs a crawling Tiger almost nothing.
        let step = press(1.0, 0.0, tree(0.15), 0.0, 1.0 / 60.0);
        assert!(step.speed > 0.99);
    }

    #[test]
    fn a_garden_wall_is_ploughed_and_a_load_bearing_wall_needs_a_ram() {
        let push = tractive_force(0.0, 0.0) - rolling_force(0.0);
        let garden = free_standing_brick(0.25, 4.0, 2.0);
        assert!(garden.force_n < 0.2 * push, "{garden:?}");
        let pier = free_standing_brick(0.54, 0.54, 2.5);
        assert!(pier.force_n < 0.1 * push, "{pier:?}");
        let bearing = restrained_brick(0.51, 4.0, 3.2);
        assert!(bearing.force_n > push, "{bearing:?}");
        // Crawling at first-gear speed the Tiger stops against it without breaking through.
        let (mut v, mut absorbed) = (FIRST_GEAR_M_S, 0.0);
        for _ in 0..600 {
            let s = press(v, 0.0, bearing, absorbed, 1.0 / 60.0);
            assert!(!s.gave_way);
            v = s.speed;
            absorbed += s.work_j;
        }
        assert!(v < 1.0e-3 && absorbed < bearing.work_j);
        // A ram: the wall takes the tank's momentum; at 2 m/s (7 km/h) it holds, cracked; a second
        // ram finishes it; one at 5 m/s goes straight through and comes out slower.
        let ram = |speed: f32, absorbed: f32| {
            let (mut v, mut absorbed) = (speed, absorbed);
            for _ in 0..600 {
                let s = press(v, 0.0, bearing, absorbed, 1.0 / 60.0);
                v = s.speed;
                absorbed += s.work_j;
                if s.gave_way {
                    return (true, v, absorbed);
                }
            }
            (false, v, absorbed)
        };
        let (broke, v, cracked) = ram(2.0, 0.0);
        assert!(!broke && v == 0.0 && cracked > 0.5 * 0.5 * MASS_KG * 4.0, "first ram {cracked} J");
        let (broke, _, _) = ram(2.0, cracked);
        assert!(broke, "the second ram finishes the cracked wall");
        let (broke, v, _) = ram(5.0, 0.0);
        assert!(broke && v > 1.0 && v < 5.0, "after the ram {v} m/s");
    }
}
