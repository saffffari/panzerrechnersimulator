//! Component damage of the player's Tiger from source-confirmed strikes on its outside: what a shell breaks where
//! it lands (penetrating or not), what each failure still lets the machine do, and whether its one
//! crewman can put it right in the field. Pure and deterministic: the projectile's seed decides.
//!
//! Sources (docs/TIGER_DAMAGE_REPAIR_20260928.md): the Tigerfibel D 656/27 (1 Aug 1943) and
//! Lt. Zabel's report on one 3./s.Pz.Abt. 503 Tiger, 10–11 Feb 1943: 227 anti-tank-rifle hits,
//! 14 of 4.5/5.7 cm and 11 of 7.62 cm, none through, yet a cupola block went opaque, both MGs were
//! shot to pieces, a 7.62 cm mantlet hit sheared the gun mount, a hit on the bow plate tore off
//! the spare links, the turret ring jammed for a while, the radio failed from shock, road wheels
//! were holed and two torsion bars broke — and the tank still drove 60 km. This establishes possible
//! failure modes, not per-hit frequencies. Remote shock failures need a structural transfer model
//! and are not sampled from a whole-vehicle lottery. Firing trials (NIIBT
//! Kubinka 1943, the 1944 Soviet field trial) add the turret-ring jam and the knocked-out driver's
//! block. Probabilities below are tuning anchored to those cases, not measured rates.
use glam::Vec3;

use super::ModuleKind;

/// The specific failure of a component. It decides what the component can still do and whether
/// the crewman can put it right in the field ([`Fault::field_repair`]).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Fault {
    #[default]
    None,
    /// Recoil brake losing oil: the oil safety cuts the firing circuit ("Wenn die Ölsicherung
    /// ausgeschaltet hat, darf nicht geschossen werden", TF p.56). Crew tops up the 5.1 l.
    RecoilLeak,
    /// Mantlet hit sheared the mount bolts; the gun stays in full recoil (Zabel). Workshop.
    MountSheared,
    /// Hydraulic traverse fault (TF p.63): no powered traverse. Crew: "Kupplung abklopfen".
    HydraulicFault,
    /// A fragment wedged in the 10 mm turret-ring gap: no traverse at all. Crew knocks it free
    /// from outside (inference; the hydraulic drive has a relief valve for a blocked turret).
    RingJammed,
    /// Ring bolts or bearing wrecked: no traverse. Workshop with a gantry.
    RingWrecked,
    /// Sight knocked out of true by shock ("wandern sie durch die Erschütterung aus", TF p.79):
    /// the reticle no longer shows where the gun points. Crew boresights through the bore.
    SightOutOfTrue,
    /// Sight optics smashed: no reticle. Workshop (optics were scarce).
    SightSmashed,
    /// MG jacket or barrel bent: barrel change from the spare box (TF pp.58–61).
    MgBarrel,
    /// MG shot to pieces (Zabel: both). The armourer's job.
    MgWrecked,
    /// Radio knocked out by shock (Zabel). Crew re-seats plugs and fuses (TF p.48).
    RadioShock,
    /// Legacy spall/fragment damage with no finer diagnosis; the table in `repair_time` decides.
    Spall,
}

/// A spare part a repair consumes.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Spare {
    VisionBlock,
    TrackLink,
}

/// How the crewman puts a fault right in the field.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct FieldRepair {
    pub seconds: f32,
    /// Done from outside the vehicle rather than from the fighting compartment.
    pub outside: bool,
    pub spare: Option<Spare>,
}

impl Fault {
    /// `None` means no field repair: the workshop's job, or nothing to repair.
    pub fn field_repair(self) -> Option<FieldRepair> {
        let inside = |seconds| Some(FieldRepair { seconds, outside: false, spare: None });
        match self {
            // Times are tuning; the sources give procedures but no durations.
            Fault::RecoilLeak => inside(20.0),
            Fault::HydraulicFault => inside(3.0),
            Fault::RingJammed => Some(FieldRepair { seconds: 12.0, outside: true, spare: None }),
            Fault::SightOutOfTrue => inside(25.0),
            Fault::MgBarrel => inside(6.0),
            Fault::RadioShock => inside(2.0),
            Fault::None | Fault::MountSheared | Fault::RingWrecked | Fault::SightSmashed
            | Fault::MgWrecked | Fault::Spall => None,
        }
    }

    /// Whether the component still does its job with this fault present.
    pub fn functional(self) -> bool {
        matches!(self, Fault::None | Fault::SightOutOfTrue)
    }

    /// Worse failures are never replaced by milder ones: 0 intact, 1 still working, 2 out but
    /// field-repairable, 3 the workshop's job.
    pub fn severity(self) -> u8 {
        match (self, self.field_repair()) {
            (Fault::None, _) => 0,
            (fault, _) if fault.functional() => 1,
            (_, Some(_)) => 2,
            (_, None) => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Fault::None => "intact",
            Fault::RecoilLeak => "recoil brake leaking",
            Fault::MountSheared => "gun mount sheared",
            Fault::HydraulicFault => "traverse hydraulics fault",
            Fault::RingJammed => "fragment jamming the turret ring",
            Fault::RingWrecked => "turret ring wrecked",
            Fault::SightOutOfTrue => "sight knocked out of true",
            Fault::SightSmashed => "sight smashed",
            Fault::MgBarrel => "MG barrel bent",
            Fault::MgWrecked => "MG shot to pieces",
            Fault::RadioShock => "radio knocked out",
            Fault::Spall => "fragment damage",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Side {
    /// Hull-local +x: the driver's side.
    Left,
    Right,
}
impl Side {
    pub fn of(x: f32) -> Side { if x >= 0.0 { Side::Left } else { Side::Right } }
    pub fn index(self) -> usize { match self { Side::Left => 0, Side::Right => 1 } }
    pub fn track(self) -> ModuleKind {
        match self { Side::Left => ModuleKind::LeftTrack, Side::Right => ModuleKind::RightTrack }
    }
}

/// Where on the Tiger a strike landed.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Zone {
    /// Road wheels and the track run along one side; `track` when the strike is on the track
    /// band itself (bottom or top run) rather than on a wheel.
    RunningGear { side: Side, track: bool },
    Sprocket(Side),
    Idler(Side),
    /// The driver's front plate, where 1942–43 crews hung their spare links.
    BowPlate,
    BallMount,
    DriverVisor,
    Mantlet,
    /// A direct source-triangle hit on the separately retained gunner telescope assembly.
    GunnerSight,
    /// The band where turret meets hull roof.
    TurretRing,
    Cupola { block: usize },
    HullWall,
    TurretWall,
}

/// Hull-local height of the hull roof (the driver's and radio operator's hatch apertures).
pub const HULL_ROOF_Y: f32 = 2.0;
/// Height of the ring band above the roof in which a fragment can wedge into the gap.
const RING_BAND_M: f32 = 0.04;
/// Cupola block centres, turret-local (`tiger_periscopes::CUPOLA_CENTERS`).
pub const BLOCKS: usize = super::tiger_periscopes::CUPOLA_NAMES.len();

/// Classify a source-confirmed strike. The narrow zones remain declared geometric envelopes,
/// not measured material failure models. A coarse armor OBB contact must never be passed directly:
/// its empty upper corners previously manufactured ring, mantlet and optical failures.
pub fn zone(hull_local: Vec3, turret_local: Vec3, on_turret_box: bool) -> Zone {
    let p = hull_local;
    let turret = on_turret_box || p.y > HULL_ROOF_Y;
    if turret {
        let q = turret_local;
        let ring_radius=Vec3::new(q.x,0.0,q.z).length();
        if (p.y-HULL_ROOF_Y).abs() < RING_BAND_M && (ring_radius-1.45).abs()<0.10 {
            return Zone::TurretRing;
        }
        if let Some((block,_))=super::tiger_periscopes::CUPOLA_CENTERS.iter().enumerate()
            .filter(|(_,center)|center.distance(q)<0.11)
            .min_by(|(_,a),(_,b)|a.distance_squared(q).total_cmp(&b.distance_squared(q))) {
            return Zone::Cupola {block};
        }
        // Only the support immediately around the gun trunnion, not the entire front turret wall.
        if q.x.abs()<0.30 && (q.y-1.845).abs()<0.25 && (1.25..1.75).contains(&q.z) {
            return Zone::Mantlet;
        }
        return Zone::TurretWall;
    }
    let front = p.z > 2.3;
    let rear = p.z < -2.9;
    let outer = p.x.abs() > 1.25;
    if p.y < 1.15 && outer {
        let side = Side::of(p.x);
        if p.z > 2.9 { return Zone::Sprocket(side); }
        if rear { return Zone::Idler(side); }
        let track = p.y < 0.35 || p.y > 0.95;
        return Zone::RunningGear { side, track };
    }
    if front && p.y >= 1.2 {
        if (p.x - 0.535).abs() < 0.3 && (p.y - 1.74).abs() < 0.26 { return Zone::DriverVisor; }
        if (p.x + 0.547).abs() < 0.3 && (p.y - 1.78).abs() < 0.26 { return Zone::BallMount; }
        return Zone::BowPlate;
    }
    Zone::HullWall
}

/// Articulated source frames disambiguate the optic/shutter and gun from nearby armor. Other
/// zones still require both the exact surface contact and their narrow geometric envelope.
pub fn source_zone(hull_local:Vec3,turret_local:Vec3,source_frame:usize)->Zone {
    match source_frame {
        8=>Zone::GunnerSight,
        5|9|10|11=>Zone::DriverVisor,
        2|12|13=>Zone::Mantlet,
        _=>zone(hull_local,turret_local,!matches!(source_frame,0|3|5|9|10|11|14)),
    }
}

/// Kinetic energy deposited normal to the actual contacted source surface. Residual penetrator
/// energy is removed before projecting; grazing kinetic strikes cannot act like normal hits.
/// Filler pressure/structural shock is not guessed here; the separate penetration solver owns it.
pub fn deposited_normal_energy_kj(mass_kg:f32,speed:f32,residual_speed:f32,cosine:f32)->f32 {
    if !mass_kg.is_finite() || !speed.is_finite() || !residual_speed.is_finite() || !cosine.is_finite()
        || mass_kg<=0.0 || speed<=0.0 {return 0.0;}
    0.0005*mass_kg*(speed*speed-residual_speed.max(0.0).powi(2)).max(0.0)*cosine.clamp(0.0,1.0).powi(2)
}

/// Confirm only the bounded neighborhood behind the coarse armor envelope along the actual
/// incoming path. A missing source surface cannot create a component fault. The outer envelope
/// and penetration decision are deliberately left to the existing armor solver.
pub fn confirm_source_contact(source:&super::surface_fire::CabinFire,
    models:&[glam::Mat4;super::surface_fire::FRAMES],point:Vec3,incoming:Vec3)
    ->Option<super::surface_fire::Contact> {
    if !point.is_finite() || !incoming.is_finite() || incoming.length_squared()<0.5 {return None;}
    let direction=incoming.normalize();
    source.sweep(point-direction*0.10,point+direction*1.25,models)
}

/// What struck.
#[derive(Copy, Clone, Debug)]
pub struct Strike {
    pub calibre_mm: f32,
    /// Deposited normal kinetic energy at the confirmed source contact (not incident total KE).
    pub energy_kj: f32,
    pub penetrated: bool,
    pub high_explosive: bool,
}

/// Weapon classes the sources describe: anti-tank rifles and heavy MGs; 37/45 mm guns;
/// 57/76 mm guns; 85 mm and heavier.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Class { Rifle, Light, Medium, Heavy }
impl Strike {
    pub fn class(self) -> Class {
        match self.calibre_mm {
            c if c < 20.0 => Class::Rifle,
            c if c < 50.0 => Class::Light,
            c if c < 80.0 => Class::Medium,
            _ => Class::Heavy,
        }
    }
}

/// One consequence of a strike.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Effect {
    Fault(ModuleKind, Fault),
    /// Road wheels holed, arms or torsion bars broken: the side loses this much of its condition.
    RunningGear(Side, f32),
    BreakTrack(Side),
    /// A cupola block goes opaque; with `holder` its mounting is smashed and it cannot be swapped.
    VisionBlock { block: usize, holder: bool },
    DriverVisorJam,
    /// Spare links hung on the bow plate torn off.
    SpareLinksLost(u8),
}

/// The error a strike knocks into the sight: 2–5 mils, in a direction the shell decides.
/// The magnitude is tuning; TF p.79 only says sights wander out of true under shock.
pub fn sight_error(seed: u32) -> glam::Vec2 {
    let angle = roll(seed, 30) * std::f32::consts::TAU;
    let mils = 2.0 + 3.0 * roll(seed, 31);
    glam::Vec2::new(angle.cos(), angle.sin()) * mils * 1.0e-3
}

/// Deterministic [0, 1) roll for one question about one strike.
pub fn roll(seed: u32, salt: u32) -> f32 {
    let mut h = seed ^ salt.wrapping_mul(0x9E37_79B9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Everything one strike does to the outside of the Tiger, and the shock it sends through it.
pub fn effects(zone: Zone, strike: Strike, seed: u32) -> Vec<Effect> {
    use Class::*;
    let class = strike.class();
    let chance = |salt: u32, p: f32| roll(seed, salt) < p;
    let mut out = Vec::new();
    // Conservative eligibility thresholds [engineering estimates], not new historical rates.
    // The old conditional outcome table is retained only after local contact and meaningful
    // work are established. Zero/invalid or merely tangential contacts cause no component failure.
    let minimum_kj=match zone {
        Zone::Cupola {..}|Zone::GunnerSight=>0.05,
        Zone::DriverVisor|Zone::BallMount=>0.5,
        Zone::RunningGear {..}|Zone::Sprocket(_)|Zone::Idler(_)=>2.0,
        Zone::BowPlate=>20.0,
        Zone::Mantlet=>80.0,
        Zone::TurretRing=>10.0,
        Zone::HullWall|Zone::TurretWall=>return out,
    };
    if !strike.energy_kj.is_finite() || strike.energy_kj<minimum_kj {return out;}
    let work=(strike.energy_kj/minimum_kj-1.0).clamp(0.0,1.0);
    let chance=|salt:u32,p:f32|chance(salt,p*work);
    match zone {
        Zone::RunningGear { side, track } => {
            let wheels = match class { Rifle => 0.01, Light => 0.08, Medium => 0.14, Heavy => 0.25 };
            out.push(Effect::RunningGear(side, wheels*work));
            // Anti-tank rifles needed 4–5 hits on one pin, so they never break a track here;
            // a 45 mm hit at 30–200 m did. High explosive under the pannier wrecks the links.
            let breaks = match (class, track) {
                (Rifle, _) => 0.0,
                (Light, true) => 0.7, (Light, false) => 0.25,
                (Medium, true) => 0.85, (Medium, false) => 0.4,
                (Heavy, true) => 0.95, (Heavy, false) => 0.6,
            } + if strike.high_explosive && class >= Medium { 0.1 } else { 0.0 };
            if chance(1, breaks) { out.push(Effect::BreakTrack(side)); }
        }
        Zone::Sprocket(side) if class >= Light => {
            out.push(Effect::RunningGear(side, if class >= Medium { 0.3 } else { 0.2 }));
            if chance(2, 0.75) { out.push(Effect::BreakTrack(side)); }
        }
        Zone::Idler(side) if class >= Light => {
            // Zabel: the idler "worked itself out of its mount"; a slack track comes off.
            out.push(Effect::RunningGear(side, 0.12));
            if chance(3, 0.5) { out.push(Effect::BreakTrack(side)); }
        }
        Zone::Sprocket(side) | Zone::Idler(side) => out.push(Effect::RunningGear(side, 0.01)),
        // Bow plate is not a spare-link hit. The current source set has no separately
        // identified hung-link assembly; do not remove inventory from an arbitrary plate strike.
        Zone::DriverVisor => {
            if chance(5, if class == Rifle { 0.5 } else { 0.85 }) { out.push(Effect::DriverVisorJam); }
        }
        Zone::BallMount => {
            let wrecked = class >= Light && chance(6, 0.6);
            if wrecked { out.push(Effect::Fault(ModuleKind::HullMg, Fault::MgWrecked)); }
            else if chance(7, 0.35) { out.push(Effect::Fault(ModuleKind::HullMg, Fault::MgBarrel)); }
        }
        Zone::Mantlet => {
            let (sheared,leak)=match class {
                Rifle=>(0.0,0.0),Light=>(0.0,0.15),Medium=>(0.25,0.4),Heavy=>(0.6,0.3),
            };
            let r=roll(seed,8);
            if r<sheared*work {out.push(Effect::Fault(ModuleKind::MainGun,Fault::MountSheared));}
            else if r<(sheared+leak)*work {out.push(Effect::Fault(ModuleKind::MainGun,Fault::RecoilLeak));}
            // Coax and telescope are separate parts. A shield/support strike is not proof that
            // either was hit; penetrating fragments still test their actual internal volumes.
        }
        Zone::GunnerSight=>{
            if chance(9,1.0) {out.push(Effect::Fault(ModuleKind::GunnerSight,Fault::SightSmashed));}
        }
        Zone::TurretRing => {
            let (wrecked, jammed) = match class {
                Rifle => (0.0, 0.15), Light => (0.0, 0.5), Medium => (0.1, 0.6), Heavy => (0.4, 0.5),
            };
            let r = roll(seed, 13);
            if r < wrecked*work { out.push(Effect::Fault(ModuleKind::TurretTraverse, Fault::RingWrecked)); }
            else if r < (wrecked + jammed)*work { out.push(Effect::Fault(ModuleKind::TurretTraverse, Fault::RingJammed)); }
        }
        Zone::Cupola { block } => {
            let (opaque, holder) = match class {
                Rifle => (0.8, 0.0), Light => (1.0, 0.5), Medium | Heavy => (1.0, 0.8),
            };
            if chance(14, opaque) {
                out.push(Effect::VisionBlock { block, holder: chance(15, holder) });
            }
            // Neighbouring blocks are independent contacts; no free adjacent failure is
            // invented without a propagated fracture/debris solution.
        }
        _ => {}
    }
    // No remote radio/sight lottery: neither impulse transfer nor fastener/glass strain is
    // currently solved. Internal component damage remains the real penetrating fragment path.
    out
}

/// A cupola vision block.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Block {
    #[default]
    Clear,
    /// Cracked and gone opaque; the crewman can swap in a spare from inside.
    Opaque,
    /// Its mounting smashed: jammed, opaque, and not swappable in the field (Zabel's 4.5 cm hit).
    Wrecked,
}

/// Exterior condition and spare stowage of the player's Tiger.
#[derive(Clone, Debug, PartialEq)]
pub struct Exterior {
    /// Road-wheel, swing-arm and torsion-bar condition per side (left, right): 1 intact, 0 at the
    /// worst this model allows — still mobile, as Zabel's Tiger was after two broken torsion bars.
    pub running_gear: [f32; 2],
    pub blocks: [Block; BLOCKS],
    /// Early tanks carried 4 spare vision blocks in two boxes in the turret [T1 citing J&D].
    pub spare_blocks: u8,
    /// Links hung on the bow plate, the improvised stowage of 1942–43 (Zabel's were bar-clamped
    /// there); factory turret stowage came only in April 1943.
    pub spare_links: u8,
    /// The driver's visor shutter jammed ("Im Winter und bei Beschuß klemmt sie manchmal", TF p.40).
    pub visor_jammed: bool,
    /// How long the driver has been working the jammed visor free.
    pub visor_freeing_s: f32,
    /// Where, turret-local, a fragment is wedged in the ring gap; knocked free from outside.
    pub ring_jam: Option<Vec3>,
    /// How far (radians: traverse, elevation) the sight's line has wandered from the bore.
    pub sight_error: glam::Vec2,
}

pub const SPARE_BLOCKS: u8 = 4;
/// Seconds to free a jammed visor at the driver's station (tuning; TF p.40 gives the procedure).
pub const VISOR_FREE_S: f32 = 10.0;
pub const SPARE_LINKS: u8 = 6;

impl Default for Exterior {
    fn default() -> Self {
        Self { running_gear: [1.0; 2], blocks: [Block::Clear; BLOCKS], spare_blocks: SPARE_BLOCKS,
            spare_links: SPARE_LINKS, visor_jammed: false, visor_freeing_s: 0.0, ring_jam: None, sight_error: glam::Vec2::ZERO }
    }
}

impl Exterior {
    /// Top-speed factor of the damaged running gear: never below half, since the tank stays
    /// mobile with wheels missing and arms hanging. Tuning.
    pub fn speed_factor(&self) -> f32 {
        0.5 + 0.5 * self.running_gear[0].min(self.running_gear[1])
    }
    /// A side with worse running gear drags; positive pulls toward the left side. Tuning.
    pub fn steer_pull(&self) -> f32 {
        (self.running_gear[1] - self.running_gear[0]) * 0.08
    }
    /// Ride roughness in 0..1 from the damaged running gear, for the crewman's senses.
    pub fn roughness(&self) -> f32 {
        1.0 - self.running_gear[0].min(self.running_gear[1])
    }
    pub fn block_clear(&self, block: usize) -> bool { self.blocks.get(block) == Some(&Block::Clear) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strike(calibre_mm: f32) -> Strike {
        Strike { calibre_mm, energy_kj: if calibre_mm<20.0 {30.0}else{1800.0}, penetrated: false, high_explosive: false }
    }
    fn hull(x: f32, y: f32, z: f32) -> Zone { zone(Vec3::new(x, y, z), Vec3::ZERO, false) }
    fn share(zone: Zone, calibre: f32, want: impl Fn(&Effect) -> bool) -> f32 {
        (0..4000u32).filter(|&seed| effects(zone, strike(calibre), seed.wrapping_mul(2654435761)).iter().any(&want))
            .count() as f32 / 4000.0
    }

    #[test]
    fn strikes_are_zoned_by_where_they_land_on_the_real_tiger() {
        assert_eq!(hull(2.06, 0.6, 0.0), Zone::RunningGear { side: Side::Left, track: false });
        assert_eq!(hull(-2.06, 0.2, 0.5), Zone::RunningGear { side: Side::Right, track: true });
        assert_eq!(hull(1.9, 0.6, 3.4), Zone::Sprocket(Side::Left));
        assert_eq!(hull(-1.9, 0.6, -3.4), Zone::Idler(Side::Right));
        assert_eq!(hull(0.535, 1.74, 3.56), Zone::DriverVisor);
        assert_eq!(hull(-0.547, 1.78, 3.56), Zone::BallMount);
        assert_eq!(hull(0.0, 1.5, 3.56), Zone::BowPlate);
        assert_eq!(hull(2.06, 1.6, 0.0), Zone::HullWall);
        // Empty coarse hull corners must not manufacture a turret-ring contact.
        assert_eq!(zone(Vec3::new(2.06, 2.1, 0.0), Vec3::new(2.06, 1.58, 0.0), false), Zone::TurretWall);
        assert_eq!(zone(Vec3::new(1.45,2.01,0.0),Vec3::new(1.45,1.49,0.0),true),Zone::TurretRing);
        assert_eq!(zone(Vec3::new(0.0, 2.4, 3.5), Vec3::new(0.0, 1.88, 1.6), false), Zone::Mantlet);
        assert_eq!(zone(Vec3::new(1.5, 2.4, 0.0), Vec3::new(1.5, 1.88, 0.0), false), Zone::TurretWall);
        let block = 3;
        let at = super::super::tiger_periscopes::CUPOLA_CENTERS[block];
        assert_eq!(zone(Vec3::new(0.0, 2.9, 0.0), at, true), Zone::Cupola { block });
    }

    #[test]
    fn anti_tank_rifles_crack_glass_but_never_break_track_and_45mm_breaks_track() {
        let track = Zone::RunningGear { side: Side::Left, track: true };
        assert_eq!(share(track, 14.5, |e| matches!(e, Effect::BreakTrack(_))), 0.0);
        let broken = share(track, 45.0, |e| *e == Effect::BreakTrack(Side::Left));
        assert!((0.62..0.78).contains(&broken), "{broken}");
        let cracked = share(Zone::Cupola { block: 2 }, 14.5, |e| matches!(e, Effect::VisionBlock { block: 2, holder: false }));
        assert!(cracked > 0.7, "{cracked}");
        assert_eq!(share(Zone::Cupola { block: 2 }, 14.5, |e| matches!(e, Effect::VisionBlock { holder: true, .. })), 0.0);
        // A 45 mm hit on the drum can smash the block's mounting (Zabel's cupola).
        assert!(share(Zone::Cupola { block: 2 }, 45.0, |e| matches!(e, Effect::VisionBlock { holder: true, .. })) > 0.3);
    }

    #[test]
    fn a_76mm_mantlet_hit_can_shear_the_mount_or_start_a_leak_and_they_exclude_each_other() {
        let sheared = share(Zone::Mantlet, 76.2, |e| *e == Effect::Fault(ModuleKind::MainGun, Fault::MountSheared));
        let leaking = share(Zone::Mantlet, 76.2, |e| *e == Effect::Fault(ModuleKind::MainGun, Fault::RecoilLeak));
        assert!((0.2..0.3).contains(&sheared) && (0.35..0.45).contains(&leaking), "{sheared} {leaking}");
        for seed in 0..2000 {
            let gun = effects(Zone::Mantlet, strike(76.2), seed).into_iter()
                .filter(|e| matches!(e, Effect::Fault(ModuleKind::MainGun, _))).count();
            assert!(gun <= 1);
        }
        assert_eq!(share(Zone::Mantlet, 14.5, |e| matches!(e, Effect::Fault(ModuleKind::MainGun, _))), 0.0);
    }

    #[test]
    fn energy_and_incidence_gate_failures_without_remote_component_lotteries() {
        assert_eq!(deposited_normal_energy_kj(6.3,680.0,0.0,0.0),0.0);
        let normal=deposited_normal_energy_kj(6.3,680.0,0.0,1.0);
        assert!((deposited_normal_energy_kj(6.3,680.0,0.0,0.5)-normal*0.25).abs()<0.001);
        assert_eq!(deposited_normal_energy_kj(6.3,680.0,680.0,1.0),0.0);
        for seed in 0..4096 {
            for z in [Zone::HullWall,Zone::TurretWall,Zone::BowPlate,Zone::Mantlet] {
                let e=effects(z,strike(76.2),seed);
                assert!(!e.iter().any(|e|matches!(e,Effect::Fault(ModuleKind::Engine|ModuleKind::Radio
                    |ModuleKind::GunnerSight|ModuleKind::CoaxMg,_))),"{z:?}: {e:?}");
            }
            for energy in [0.0,-1.0,f32::NAN] {
                let mut s=strike(76.2);s.energy_kj=energy;
                assert!(effects(Zone::Mantlet,s,seed).is_empty());
                assert!(effects(Zone::RunningGear {side:Side::Right,track:true},s,seed).is_empty());
            }
        }
    }
    #[test]
    fn exact_optic_and_shutter_frames_are_local_and_cupola_roof_is_not_glass() {
        assert_eq!(source_zone(Vec3::ZERO,Vec3::ZERO,8),Zone::GunnerSight);
        assert_eq!(source_zone(Vec3::ZERO,Vec3::ZERO,10),Zone::DriverVisor);
        assert_eq!(zone(Vec3::new(0.56,3.2,-0.42),Vec3::new(0.56,2.68,-0.42),true),Zone::TurretWall);
        let direct=effects(Zone::GunnerSight,strike(14.5),7);
        assert_eq!(direct,vec![Effect::Fault(ModuleKind::GunnerSight,Fault::SightSmashed)]);
    }

    #[test]
    fn source_confirmation_rejects_empty_envelope_and_keeps_articulated_contact() {
        use glam::Mat4;
        let mesh=crate::prims::box_mesh(Vec3::new(-0.2,-0.2,-0.03),Vec3::new(0.2,0.2,0.03),[80,80,80]);
        let source=super::super::surface_fire::CabinFire::new(&[(8,&mesh)]);
        let frame=Mat4::from_rotation_translation(glam::Quat::from_rotation_y(0.7),Vec3::new(4.,2.,-7.));
        let mut models=[Mat4::IDENTITY;super::super::surface_fire::FRAMES];models[8]=frame;
        let direction=frame.transform_vector3(-Vec3::Z);
        let point=frame.transform_point3(Vec3::new(0.,0.,0.5));
        let hit=confirm_source_contact(&source,&models,point,direction).unwrap();
        assert_eq!(hit.frame,8);assert!((hit.point.z-0.03).abs()<1e-5);
        assert!(confirm_source_contact(&source,&models,point+frame.transform_vector3(Vec3::X),direction).is_none());
        assert!(confirm_source_contact(&source,&models,point,Vec3::ZERO).is_none());
    }

    #[test]
    fn effects_are_deterministic_per_shell() {
        for seed in [0, 7, 99, 123_456] {
            assert_eq!(effects(Zone::TurretRing, strike(76.2), seed), effects(Zone::TurretRing, strike(76.2), seed));
        }
    }

    #[test]
    fn repairable_faults_are_exactly_the_documented_field_fixes() {
        for fault in [Fault::RecoilLeak, Fault::HydraulicFault, Fault::RingJammed, Fault::SightOutOfTrue,
            Fault::MgBarrel, Fault::RadioShock] {
            assert!(fault.field_repair().is_some(), "{fault:?}");
        }
        for fault in [Fault::MountSheared, Fault::RingWrecked, Fault::SightSmashed, Fault::MgWrecked, Fault::None] {
            assert!(fault.field_repair().is_none(), "{fault:?} is the workshop's job");
        }
        assert!(Fault::RingJammed.field_repair().unwrap().outside);
        assert!(Fault::SightOutOfTrue.functional(), "an untrue sight still shows a reticle");
        assert!(!Fault::RecoilLeak.functional());
    }

    #[test]
    #[ignore = "requires GPU and source Tiger assets"]
    fn without_power_the_gunner_cranks_the_turret_by_hand_and_a_jammed_ring_holds_it() -> anyhow::Result<()> {
        use super::super::{Game, Mode, Station, TankInput};
        let instance = crate::create_instance();
        let adapter = crate::request_adapter(&instance, None);
        let (device, queue) = crate::request_device(&adapter);
        let (mut renderer, mut game) = Game::build(&device, &queue, false)?;
        game.mute_audio();
        game.mode = Mode::Tank;
        game.station = Station::Gunner;
        game.switch_t = 0.0;
        let dt = 1.0 / 60.0;
        let mut traverse_for = |game: &mut Game, seconds: f32| {
            let start = game.player.turret_yaw;
            for _ in 0..(seconds / dt) as usize {
                game.lay_yaw = start + 1.2; // a target far to the side
                game.update(&TankInput::default(), dt, &mut renderer);
            }
            (game.player.turret_yaw - start).abs().to_degrees()
        };
        game.engine_on = false;
        let by_hand = traverse_for(&mut game, 10.0);
        assert!((12.0..18.0).contains(&by_hand), "hand traverse ~1.5 deg/s: {by_hand}");
        game.engine_on = true;
        let powered = traverse_for(&mut game, 5.0);
        assert!((25.0..35.0).contains(&powered), "powered traverse ~6 deg/s: {powered}");
        game.debug_set_module_fault(ModuleKind::TurretTraverse, Fault::HydraulicFault);
        let faulty = traverse_for(&mut game, 10.0);
        assert!((12.0..18.0).contains(&faulty), "a hydraulic fault leaves the handwheel: {faulty}");
        game.debug_set_module_fault(ModuleKind::TurretTraverse, Fault::RingJammed);
        let jammed = traverse_for(&mut game, 10.0);
        assert!(jammed < 1.0, "a jammed ring holds the turret: {jammed}");
        Ok(())
    }

    #[test]
    fn damaged_running_gear_slows_and_drags_but_never_immobilises() {
        let mut exterior = Exterior::default();
        assert_eq!(exterior.speed_factor(), 1.0);
        exterior.running_gear = [0.0, 1.0];
        assert_eq!(exterior.speed_factor(), 0.5);
        assert!(exterior.steer_pull() > 0.0);
        assert_eq!(exterior.roughness(), 1.0);
    }
}
