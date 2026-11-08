//! The round as a deformable body (2026-11-03, the owner: the shells must squash, bend, ricochet
//! and spin off the armour, and break to throw shrapnel, each by its kind of round).
//!
//! The projectile is a voxel lattice of its own: one particle per cubic cell, 12 across the calibre,
//! bonded to its 18 nearest neighbours. Each bond is an elastic-plastic spring (a bond-based
//! peridynamic solid, named: it fixes Poisson's ratio at 1/4) with the flow stress, linear
//! hardening, thermal softening and failure strain of the weaker of its two cells, so a hardened
//! nose, a softer body, a cap, a sheet-steel windshield, the explosive filler, or a tungsten-carbide
//! core and its carrier each answer the load by their own material. A bond fails in tension past
//! its failure strain, or when its accumulated plastic flow passes `EXHAUSTION` times that strain
//! (ductile exhaustion in shear, authored: compression alone does not part steel); broken neighbours still push apart
//! when pressed together.
//!
//! The plate meets it as a rigid-plastic slab of voxels the round's own size (the plate frame of
//! `lattice`: x along the shot's in-plane direction, z into the plate): a surface cell of the round
//! in a voxel still holding plate, and advancing into it, is pressed back along its own outward
//! normal by the plate's flow pressure, with kinetic friction along the surface (the
//! cavity-expansion picture: the target pushes normal to the nose, so an embedded symmetric nose
//! is pushed straight back and a half-embedded one is turned); the work done goes into that voxel,
//! which is gone once it has taken its volume times the flow pressure (so the nose cuts a cavity
//! the shank then passes through, and a glancing nose a gouge). The pressure is the calibrated law's resistance to this round at this speed,
//! its kinetic energy over the volume of its flat penetration (`resistance_pa`): the solve's flat
//! normal stop lands near the calibrated depth by construction; a nose that flattens meets more
//! plate and stops sooner. Within a quarter calibre of either face the pressure ramps to zero
//! (free-surface relief, authored, floored at 5 %). The voxels' cavity is this contact's only: the
//! plate's own lattice solves the plate's deformation (named).
//!
//! The calibrated chain (`chain`, `ballistics`) still decides perforation, the residual speed and
//! a ricochet's continuation; this solve decides the round's shape, the pieces it breaks into, its
//! residual mass, its tumble and its heat, and reports where its own motion disagrees with the
//! chain's.

use glam::{Mat3, Vec3};
use serde::Serialize;

use super::materials::{Material, AMBIENT_K, TAYLOR_QUINNEY};
use crate::ballistics::{ShellKind, ShellSpec};

/// Cells across the calibre (the plate lattice's own `TARGET_CELLS_ACROSS_CALIBRE` is 10).
pub const CELLS_ACROSS: f32 = 12.0;
/// Time step as a fraction of a cell's fastest elastic transit.
const DT_CFL: f32 = 0.35;
/// Free-surface relief depth, calibres (authored).
pub const FREE_SURFACE_CALIBRES: f32 = 0.25;
/// Kinetic friction of the round on the plate (the plate lattice's estimate).
pub const FRICTION: f32 = super::lattice::FRICTION_KINETIC;
/// Share of friction work that heats the round (the rest heats the plate; named estimate).
pub const FRICTION_HEAT_TO_SHOT: f32 = 0.5;
/// A bond fails when its accumulated plastic flow passes this many failure strains (authored).
pub const EXHAUSTION: f32 = 12.0;
/// Failure strain scatter (±, fraction), hashed per bond.
pub const FAILURE_SCATTER: f32 = 0.10;
/// Rifling twist in calibres per turn when a round's own is not recorded (authored).
pub const TWIST_CALIBRES: f32 = 25.0;
/// Frames kept for slow-motion review.
pub const FRAMES: usize = 160;
const MAX_STEPS: u64 = 20_000;
/// A ballistic cap's sheet thickness, m (authored: a crimped sheet-steel windshield of a few
/// millimetres); its cells, one cell thick, carry this share of a cell's mass and stiffness.
pub const WINDSHIELD_SHEET_M: f32 = 0.0025;
/// Least share of the calibrated mean pressure left static (authored).
pub const STATIC_SHARE: f32 = 0.25;
/// A shed piece moving into the vehicle slower than this stays in the plate's cavity (authored).
pub const SHRAPNEL_MIN_MPS: f32 = 20.0;
/// In play: the span over which a perforating round clears the plate (crossing times).
pub const GAME_HORIZON: f32 = 1.0;
/// The solve's span in crossing times (see `Shot::solve`).
pub const HORIZON: f32 = 2.5;

/// What a cell of the round is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Zone {
    Rod, Jet, SoftCase,
    /// The hardened nose and shoulder.
    Nose,
    /// The body behind it, tempered softer.
    Body,
    /// The base, left soft (Soviet shot: air-cooled ferrite and pearlite).
    Base,
    /// A piercing cap over the nose.
    Cap,
    /// A thin ballistic cap (windshield) ahead of the nose or cap.
    Windshield,
    /// The bursting charge in its cavity.
    Filler,
    /// A sub-calibre round's tungsten-carbide core.
    Core,
    /// A sub-calibre round's soft steel carrier.
    Carrier,
}

impl Zone {
    pub fn material(self) -> &'static Material {
        match self {
            Zone::Rod => &REVIEW_ROD, Zone::Jet=>&REVIEW_JET, Zone::SoftCase=>&CARRIER,
            Zone::Nose => &SHOT_NOSE,
            Zone::Body | Zone::Base => &SHOT_BODY,
            Zone::Cap => &PIERCING_CAP,
            Zone::Windshield => &WINDSHIELD,
            Zone::Filler => &FILLER,
            Zone::Core => &TUNGSTEN_CARBIDE,
            Zone::Carrier => &CARRIER,
        }
    }
}

// Authored material surrogates for comparative game review; not ammunition data.
pub static REVIEW_ROD:Material=Material{name:"dense ductile rod surrogate",density:19000.,
    youngs:200.0e9,yield_stress:1.5e9,hardening:1.0e9,rate_c:0.015,ref_rate:1.,melt_k:1600.,
    failure_strain:0.18,specific_heat:150.,conductivity:30.,emissivity:0.8,ke_resistance:1.,fluid:false,
    provenance:"authored dense long-rod surrogate; not a measured M829A2 material law"};
pub static REVIEW_JET:Material=Material{name:"jet material surrogate",density:8960.,
    youngs:110.0e9,yield_stress:0.15e9,hardening:0.2e9,rate_c:0.01,ref_rate:1.,melt_k:1358.,
    failure_strain:0.5,specific_heat:385.,conductivity:400.,emissivity:0.7,ke_resistance:1.,fluid:false,
    provenance:"authored deformable jet surrogate; no shaped-charge formation or hydrodynamics"};

const STEEL_C: f32 = 470.0;

pub static SHOT_NOSE: Material = Material {
    name: "hardened shot nose",
    density: 7850.0,
    youngs: 207.0e9,
    yield_stress: 1.7e9,
    hardening: 2.0e9,
    rate_c: 0.01,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.05,
    specific_heat: STEEL_C,
    conductivity: 40.0,
    emissivity: 0.8,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "generic through-hardened alloy steel near 52-55 HRC (Johnson-Cook-form flow, low elongation); authored pending the shot-steel sources",
};

pub static SHOT_BODY: Material = Material {
    name: "tempered shot body",
    density: 7850.0,
    youngs: 207.0e9,
    yield_stress: 1.1e9,
    hardening: 1.5e9,
    rate_c: 0.012,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.12,
    specific_heat: STEEL_C,
    conductivity: 40.0,
    emissivity: 0.8,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "generic alloy steel tempered to about 35-40 HRC, the softer body behind a differentially hardened nose; authored pending the shot-steel sources",
};

pub static PIERCING_CAP: Material = Material {
    name: "piercing cap",
    density: 7850.0,
    youngs: 207.0e9,
    yield_stress: 1.0e9,
    hardening: 1.5e9,
    rate_c: 0.012,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.15,
    specific_heat: STEEL_C,
    conductivity: 40.0,
    emissivity: 0.8,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "generic tough steel cap, softer and more ductile than the nose it protects; authored pending sources",
};

pub static WINDSHIELD: Material = Material {
    name: "ballistic cap (sheet steel)",
    density: 7850.0,
    youngs: 200.0e9,
    yield_stress: 0.45e9,
    hardening: 0.5e9,
    rate_c: 0.02,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.30,
    specific_heat: STEEL_C,
    conductivity: 50.0,
    emissivity: 0.8,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "rimmed low-carbon steel, ~205 BHN (WAL 762/589, the BR-354B's windshield); yield from the hardness-strength relation",
};

pub static FILLER: Material = Material {
    name: "cast TNT filler",
    density: 1650.0,
    youngs: 5.0e9,
    yield_stress: 0.02e9,
    hardening: 0.0,
    rate_c: 0.0,
    ref_rate: 1.0,
    melt_k: 354.0,
    failure_strain: 0.02,
    specific_heat: 1370.0,
    conductivity: 0.26,
    emissivity: 0.9,
    ke_resistance: 0.0,
    fluid: false,
    provenance: "cast TNT: density 1.65 g/cm3, melts at 80.8 C; modulus and strength generic for a brittle cast explosive (authored); it does not detonate here",
};

pub static TUNGSTEN_CARBIDE: Material = Material {
    name: "tungsten carbide core",
    density: 14_800.0,
    youngs: 550.0e9,
    yield_stress: 5.5e9,
    hardening: 0.0,
    rate_c: 0.0,
    ref_rate: 1.0,
    melt_k: 3000.0,
    failure_strain: 0.002,
    specific_heat: 200.0,
    conductivity: 80.0,
    emissivity: 0.6,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "cemented tungsten carbide: density, modulus and compressive strength (~5.5 GPa) generic handbook values; it barely flows, cracking in tension instead (authored failure strain)",
};

pub static CARRIER: Material = Material {
    name: "soft steel carrier",
    density: 7850.0,
    youngs: 200.0e9,
    yield_stress: 0.4e9,
    hardening: 0.6e9,
    rate_c: 0.02,
    ref_rate: 1.0,
    melt_k: 1793.0,
    failure_strain: 0.25,
    specific_heat: STEEL_C,
    conductivity: 50.0,
    emissivity: 0.8,
    ke_resistance: 1.0,
    fluid: false,
    provenance: "mild steel (generic): the sub-calibre round's carrier body",
};

/// A round's build: its proportions and the parts it is made of.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Design {
    pub homogeneous:Option<Zone>,
    pub calibre_m: f32,
    pub mass_kg: f32,
    /// Ellipsoidal nose length, calibres.
    pub nose_calibres: f32,
    /// Flat tip radius as a fraction of the calibre's radius (0: pointed, a blunt nose larger).
    pub blunt: f32,
    pub piercing_cap: bool,
    pub windshield: bool,
    pub filler_kg: f32,
    /// A sub-calibre core's radius and length, m.
    pub core: Option<(f32, f32)>,
    pub twist_calibres: f32,
    /// Rockwell C hardness of the nose, the body and the base.
    pub nose_rc: f32,
    pub body_rc: f32,
    pub base_rc: f32,
    /// Circumferential V-notches cut below the bourrelet to fix where the shot breaks.
    pub notches: u8,
}

/// The build of a kinetic round (`None` for high explosive and shaped charges, whose bodies the
/// burst decides).
pub fn design(shell: &ShellSpec) -> Option<Design> {
    let d = shell.caliber_mm * 0.001;
    let mut out = Design {
        homogeneous:None,
        calibre_m: d,
        mass_kg: shell.mass_kg,
        nose_calibres: super::lodged::nose_length_calibres(shell.kind),
        blunt: 0.0,
        piercing_cap: false,
        windshield: false,
        filler_kg: shell.filler_g * 0.001,
        core: None,
        twist_calibres: TWIST_CALIBRES,
        // Soviet shot (Watertown Arsenal WAL 762/582 and 762/589, captured 85 mm BR-365K and 76 mm
        // BR-354B): Rc ~47 at the nose falling to ~42 along the body, Rc 16.5-26 at an air-cooled
        // base; one or two notches below the bourrelet.
        nose_rc: 47.0,
        body_rc: 42.0,
        base_rc: 22.0,
        notches: 1,
    };
    if crate::ballistics::is_long_rod(shell) {
        out.calibre_m=0.030;out.nose_calibres=1.;out.notches=0;
        out.twist_calibres=1.0e9;out.homogeneous=Some(Zone::Rod);return Some(out);
    }
    if shell.kind==ShellKind::Heat {
        out.calibre_m=0.012;out.mass_kg=0.12;out.filler_kg=0.;out.nose_calibres=0.3;
        out.notches=0;out.twist_calibres=1.0e9;out.homogeneous=Some(Zone::Jet);return Some(out);
    }
    if crate::ballistics::is_hesh(shell) {
        out.homogeneous=Some(Zone::SoftCase);out.nose_calibres=0.6;out.notches=0;
        out.filler_kg=0.;return Some(out);
    }
    match shell.kind {
        ShellKind::Ap => {}
        ShellKind::Apcbc => {
            out.piercing_cap = true;
            out.windshield = true;
        }
        ShellKind::Apcr => {
            let core_d = APCR_CORE_FRACTION * d;
            out.core = Some((0.5 * core_d, CORE_LENGTH_DIAMETERS * core_d));
            out.windshield = true;
            out.filler_kg = 0.0;
        }
        ShellKind::He | ShellKind::Heat => return None,
    }
    // A blunt-nosed round with a windshield (the BR-350B): authored from its designation, pending
    // the shot-shape sources.
    if shell.name.contains("BR-350B") {
        out.blunt = 0.55;
        out.windshield = true;
    }
    // The BR-365 was blunt-nosed with a ballistic cap and no piercing cap (secondary sources);
    // the ballistic law files it as APCBC for its slope behaviour.
    if shell.name.contains("BR-365") {
        out.blunt = 0.55;
        out.piercing_cap = false;
        out.windshield = true;
    }
    // German shot: noses Rc 62-65 (captured uncapped shot, via Tank Archives), the body tempered
    // softer behind them; the US reference practice Rc 60 / 59 / 45 for the rest (WAL 762/582).
    if shell.name.starts_with("8.8cm") {
        out.nose_rc = 62.0;
        out.body_rc = 55.0;
        out.base_rc = 45.0;
        out.notches = 0;
    }
    if out.core.is_some() {
        out.notches = 0;
    }
    Some(out)
}

/// Yield of a quenched-and-tempered alloy shot steel at Rockwell C `rc` (Pa): Banerjee's fit to
/// the ASM data for AISI 4340, exp(0.0355 Rc + 5.5312) MPa (fitted over Rc 30-49; beyond it an
/// extrapolation, named).
pub fn yield_at_rc(rc: f32) -> f32 {
    (0.0355 * rc + 5.5312).exp() * 1.0e6
}

/// A zone's flow and failure: (yield Pa, linear hardening Pa, failure strain). Hardening is the
/// secant of Johnson-Cook B = 0.634 sigma0, n = 0.26 to half a strain (4340, Johnson & Cook 1983;
/// Banerjee 2005). The failure strain falls with hardness from 0.18 at Rc 20 to 0.02 at Rc 60
/// (authored: the sources give the trend, harder noses shattering more readily, not a number);
/// the soft base keeps 0.08 for its brittle ferrite-pearlite (Watertown: "brittle structures").
pub fn zone_strength(design: &Design, zone: Zone) -> (f32, f32, f32) {
    let shot = |rc: f32| {
        let y = yield_at_rc(rc);
        (y, 0.634 * y * 0.5f32.powf(0.26) / 0.5, (0.02 + 0.004 * (60.0 - rc)).clamp(0.02, 0.18))
    };
    match zone {
        Zone::Nose => shot(design.nose_rc),
        Zone::Body => shot(design.body_rc),
        Zone::Base => {
            let (y, h, f) = shot(design.base_rc);
            (y, h, f.min(0.08))
        }
        other => {
            let m = other.material();
            (m.yield_stress, m.hardening, m.failure_strain)
        }
    }
}

/// A sub-calibre core's diameter over the calibre (the chain's own value) and its length in core
/// diameters (authored).
pub const APCR_CORE_FRACTION: f32 = 0.37;
pub const CORE_LENGTH_DIAMETERS: f32 = 3.2;

/// The plate's flow pressure against this round at this speed (Pa): the kinetic energy of the body
/// that penetrates (a sub-calibre round's core alone: its carrier is stripped at the face) over the
/// volume of its calibrated flat penetration, scaled by the plate's quality.
pub fn resistance_pa(shell: &ShellSpec, design: &Design, v: f32, quality: f32) -> f32 {
    let (r, mass) = match design.core {
        Some((r, l)) => (r, (TUNGSTEN_CARBIDE.density * std::f32::consts::PI * r * r * l).min(design.mass_kg)),
        None => (0.5 * design.calibre_m, design.mass_kg),
    };
    let area = std::f32::consts::PI * r * r;
    let pen_m = (shell.pen_flat_at_v(v) * 0.001).max(1e-4);
    0.5 * mass * v * v / (area * pen_m) * quality
}

/// One strike of the round on a plate, in the plate frame.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ShotImpact {
    pub design: Design,
    pub v_in: f32,
    /// Between the shot and the plate normal, radians.
    pub obliquity_rad: f32,
    pub thickness_m: f32,
    pub resistance_pa: f32,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug)]
struct Bond {
    a: u32,
    b: u32,
    rest: f32,
    k: f32,
    yield_strain: f32,
    hardening: f32,
    melt_k: f32,
    fail: f32,
    plastic: f32,
    flow: f32,
    broken: bool,
}

/// One kept frame for slow-motion review.
#[derive(Clone, Debug)]
pub struct Frame {
    pub t_s: f32,
    pub pos: Vec<Vec3>,
    pub temp: Vec<f32>,
    /// Broken share of each cell's bonds, 0..255.
    pub damage: Vec<u8>,
    /// Plate voxels the round has cut away by now.
    pub gone: Vec<(i32, i32, i32)>,
}

/// A piece the round broke into.
#[derive(Clone, Debug, Serialize)]
pub struct Piece {
    #[serde(skip)]
    pub cells: Vec<u32>,
    pub cell_count: usize,
    pub mass_kg: f32,
    pub centre: Vec3,
    pub velocity: Vec3,
    pub spin: Vec3,
    pub temperature_k: f32,
    /// Edge of a cube of its volume, m.
    pub size_m: f32,
    pub zone: Zone,
}

/// What the solve did to the round.
#[derive(Clone, Debug, Serialize)]
pub struct ShotOutcome {
    pub cells: usize,
    pub cell_m: f32,
    pub steps: u64,
    pub duration_s: f32,
    pub mass_kg: f32,
    /// The largest piece (the round that flies on, stops or ricochets).
    pub residual_kg: f32,
    pub residual_velocity: Vec3,
    pub residual_spin: Vec3,
    /// The residual's rotation across its own axis (tumble), rad/s.
    pub tumble_rad_s: f32,
    pub pieces: Vec<Piece>,
    pub broken_bonds: usize,
    pub max_plastic: f32,
    /// The residual's length along its axis over the intact round's.
    pub length_ratio: f32,
    /// The residual's widest radius from its axis over the calibre's radius.
    pub radius_ratio: f32,
    /// Between the axes of the residual's front and rear halves, degrees.
    pub bend_deg: f32,
    pub peak_temp_k: f32,
    /// Work the plate's pressure and friction took from the round, J.
    pub plate_work_j: f32,
    pub kinetic_in_j: f32,
    pub kinetic_out_j: f32,
    /// The residual's tip depth at its deepest, m (along the plate normal).
    pub deepest_m: f32,
    pub exited_back: bool,
    pub rebounded: bool,
}

/// The round's lattice and its state.
pub struct Shot {
    pub impact: ShotImpact,
    pub h: f32,
    pub zone: Vec<Zone>,
    pub mass: Vec<f32>,
    /// Positions in the round's own frame at rest (axis +z, base at z = 0).
    pub rest: Vec<Vec3>,
    pub pos: Vec<Vec3>,
    pub vel: Vec<Vec3>,
    pub temp: Vec<f32>,
    bonds: Vec<Bond>,
    bond_start: Vec<u32>,
    bond_list: Vec<u32>,
    force: Vec<Vec3>,
    pub dt: f32,
    pub t_s: f32,
    pub frames: Vec<Frame>,
    pub length_m: f32,
    plate_work: f64,
    bonds_broken: usize,
    /// The plate's static flow pressure (Pa): the calibrated mean less half the plate's density
    /// times the striking speed squared, floored at `STATIC_SHARE` of the mean.
    pub static_pa: f32,
    /// Plate voxels the round has worked on: the resistance energy each has left (J); a voxel not
    /// listed is whole.
    plate: std::collections::HashMap<(i32, i32, i32), f32>,
}

#[inline]
fn hash01(seed: u32, i: u32) -> f32 {
    let mut x = seed ^ i.wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

const OFFSETS: [[i32; 3]; 9] = [
    [1, 0, 0], [0, 1, 0], [0, 0, 1],
    [1, 1, 0], [1, -1, 0], [1, 0, 1], [1, 0, -1], [0, 1, 1], [0, 1, -1],
];

/// The round's outline at height `z` above its base: its radius there, or none past the tip.
fn profile(d: &Design, body_m: f32, z: f32) -> Option<f32> {
    let r = 0.5 * d.calibre_m;
    let nose = d.nose_calibres * d.calibre_m;
    if z < 0.0 {
        return None;
    }
    if z <= body_m {
        return Some(r);
    }
    let u = (z - body_m) / nose;
    if u > 1.0 {
        return None;
    }
    let ellipse = r * (1.0 - u * u).max(0.0).sqrt();
    // A blunt nose is cut flat where the ellipse narrows to its tip radius.
    if d.blunt > 0.0 && ellipse < d.blunt * r {
        return None;
    }
    Some(ellipse)
}

/// Where a blunt nose's flat tip stands above the body (the ellipse's radius equals the tip's).
fn nose_end(d: &Design) -> f32 {
    let nose = d.nose_calibres * d.calibre_m;
    if d.blunt > 0.0 { nose * (1.0 - d.blunt * d.blunt).max(0.0).sqrt() } else { nose }
}

/// The cells of the round at a body length `body_m`: (rest position, zone).
fn voxelise(d: &Design, body_m: f32, h: f32) -> Vec<(Vec3, Zone)> {
    let r = 0.5 * d.calibre_m;
    let n_r = (r / h).ceil() as i32 + 1;
    let tip = body_m + nose_end(d);
    let windshield_len = if d.windshield { 0.9 * d.calibre_m } else { 0.0 };
    let top = tip + windshield_len;
    let cap_t = if d.piercing_cap { 0.14 * d.calibre_m } else { 0.0 };
    let cavity_r = 0.32 * d.calibre_m;
    let cavity_len = if d.filler_kg > 0.0 { d.filler_kg / FILLER.density / (std::f32::consts::PI * cavity_r * cavity_r) } else { 0.0 };
    // The cavity sits behind the nose's hardened shoulder, its base a calibre's third from the base.
    let cavity_z0 = 0.33 * d.calibre_m;
    let core = d.core.map(|(cr, cl)| (cr, cl, body_m + nose_end(d) * 0.6 - cl));
    let mut cells = Vec::new();
    let nz = (top / h).ceil() as i32 + 1;
    for k in 0..nz {
        let z = (k as f32 + 0.5) * h;
        for j in -n_r..n_r {
            for i in -n_r..n_r {
                let x = (i as f32 + 0.5) * h;
                let y = (j as f32 + 0.5) * h;
                let rr = (x * x + y * y).sqrt();
                let p = Vec3::new(x, y, z);
                // A notch: a V-groove a cell wide and 6 % of the calibre deep, below the bourrelet.
                let notched = (0..d.notches).any(|i| {
                    let zn = body_m - (0.3 + 0.25 * i as f32) * d.calibre_m;
                    (z - zn).abs() < 0.5 * h && rr > r - 0.06 * d.calibre_m - 0.5 * h
                });
                if notched {
                    continue;
                }
                let zone = match profile(d, body_m, z) {
                    Some(rad) if rr <= rad => {
                        let inside_core = core.is_some_and(|(cr, cl, z0)| rr <= cr && z >= z0 && z <= z0 + cl + nose_end(d) * 0.4);
                        if inside_core {
                            Zone::Core
                        } else if d.core.is_some() {
                            Zone::Carrier
                        } else if cavity_len > 0.0 && rr <= cavity_r && z >= cavity_z0 && z <= cavity_z0 + cavity_len {
                            Zone::Filler
                        } else if z > body_m && cap_t > 0.0 && rad - rr < cap_t * (1.0 - 0.6 * (z - body_m) / (nose_end(d) + 1e-6)).max(0.35) {
                            Zone::Cap
                        } else if z > body_m - 0.5 * d.calibre_m {
                            Zone::Nose
                        } else if z < 0.5 * d.calibre_m {
                            Zone::Base
                        } else {
                            Zone::Body
                        }
                    }
                    _ if d.windshield && z > body_m && z <= top => {
                        // A thin ogive shell from the shoulder to a point a windshield's length past
                        // the tip.
                        let u = ((z - body_m) / (top - body_m)).clamp(0.0, 1.0);
                        let shell_r = r * (1.0 - u * u).max(0.0).sqrt();
                        if rr <= shell_r && rr > shell_r - 1.05 * h {
                            Zone::Windshield
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                cells.push((p, d.homogeneous.unwrap_or(zone)));
            }
        }
    }
    cells
}

impl Shot {
    pub fn new(impact: ShotImpact) -> Self {
        Self::with_cells(impact, CELLS_ACROSS)
    }

    /// The round at `cells_across` cells across its calibre.
    pub fn with_cells(impact: ShotImpact, cells_across: f32) -> Self {
        let d = impact.design;
        let h = d.calibre_m / cells_across;
        // The body length that gives the round its mass (cells at their materials' densities),
        // then each cell's mass scaled so the total is exact.
        let fill = |z: Zone| if z == Zone::Windshield { (WINDSHIELD_SHEET_M / h).min(1.0) } else { 1.0 };
        let mass_of = |cells: &[(Vec3, Zone)]| cells.iter().map(|(_, z)| z.material().density * fill(*z) * h * h * h).sum::<f32>();
        let area = std::f32::consts::PI * (0.5 * d.calibre_m).powi(2);
        let density=d.homogeneous.map_or(7850.,|z|z.material().density);
        let mut body = (d.mass_kg / (density * area) - 0.5 * nose_end(&d)).max(0.5 * d.calibre_m);
        let mut cells = voxelise(&d, body, h);
        for _ in 0..4 {
            let m = mass_of(&cells);
            let density = if d.core.is_some() { 0.5 * (7850.0 + TUNGSTEN_CARBIDE.density) } else { density };
            body = (body + (d.mass_kg - m) / (density * area)).max(0.3 * d.calibre_m);
            cells = voxelise(&d, body, h);
        }
        let raw = mass_of(&cells);
        let scale = d.mass_kg / raw.max(1e-9);
        let n = cells.len();
        let zone: Vec<Zone> = cells.iter().map(|c| c.1).collect();
        let rest: Vec<Vec3> = cells.iter().map(|c| c.0).collect();
        let mass: Vec<f32> = zone.iter().map(|z| z.material().density * fill(*z) * h * h * h * scale).collect();
        let length_m = rest.iter().map(|p| p.z).fold(0.0f32, f32::max) + 0.5 * h;
        // Grid lookup for the bonds.
        let key = |p: Vec3| ((p.x / h - 0.5).round() as i32, (p.y / h - 0.5).round() as i32, (p.z / h - 0.5).round() as i32);
        let index: std::collections::HashMap<(i32, i32, i32), u32> = rest.iter().enumerate().map(|(i, &p)| (key(p), i as u32)).collect();
        let mut bonds = Vec::new();
        for (a, &p) in rest.iter().enumerate() {
            let (i, j, k) = key(p);
            for o in OFFSETS {
                let Some(&b) = index.get(&(i + o[0], j + o[1], k + o[2])) else { continue };
                let (ma, mb) = (zone[a].material(), zone[b as usize].material());
                let (sa, sb) = (zone_strength(&d, zone[a]), zone_strength(&d, zone[b as usize]));
                let (weak, weaker) = if sa.0 <= sb.0 { (sa, ma) } else { (sb, mb) };
                let e = 2.0 * ma.youngs * mb.youngs / (ma.youngs + mb.youngs);
                let rest_len = h * ((o[0] * o[0] + o[1] * o[1] + o[2] * o[2]) as f32).sqrt();
                let scatter = 1.0 + FAILURE_SCATTER * (2.0 * hash01(impact.seed, (a as u32).wrapping_mul(31) ^ b) - 1.0);
                bonds.push(Bond {
                    a: a as u32,
                    b,
                    rest: rest_len,
                    // A cubic lattice with equal springs to its 6 face and 12 edge neighbours is
                    // isotropic with E = 2.5 k / h (Poisson 1/4).
                    k: 0.4 * e * h * fill(zone[a]).min(fill(zone[b as usize])),
                    yield_strain: weak.0 / e,
                    hardening: weak.1 / e,
                    melt_k: weaker.melt_k,
                    fail: sa.2.min(sb.2) * scatter,
                    plastic: 0.0,
                    flow: 0.0,
                    broken: false,
                });
            }
        }
        let mut counts = vec![0u32; n + 1];
        for b in &bonds {
            counts[b.a as usize + 1] += 1;
            counts[b.b as usize + 1] += 1;
        }
        for i in 1..counts.len() {
            counts[i] += counts[i - 1];
        }
        let mut fill = counts.clone();
        let mut list = vec![0u32; bonds.len() * 2];
        for (bi, b) in bonds.iter().enumerate() {
            for end in [b.a, b.b] {
                list[fill[end as usize] as usize] = bi as u32;
                fill[end as usize] += 1;
            }
        }
        // Placement: the axis along the shot line, its tip half a cell short of the plate.
        let axis = Vec3::new(impact.obliquity_rad.sin(), 0.0, impact.obliquity_rad.cos());
        let turn = glam::Quat::from_rotation_arc(Vec3::Z, axis);
        let tip = turn * Vec3::new(0.0, 0.0, length_m);
        let offset = -tip; // the forward voxel face is exactly at the authoritative contact point
        let spin = std::f32::consts::TAU * impact.v_in / (d.twist_calibres * d.calibre_m);
        let pos: Vec<Vec3> = rest.iter().map(|&p| turn * p + offset).collect();
        let vel: Vec<Vec3> = rest.iter().map(|&p| {
            let radial = turn * Vec3::new(p.x, p.y, 0.0);
            axis * impact.v_in + (axis * spin).cross(radial)
        }).collect();
        let c_max = zone.iter().map(|z| (1.2 * z.material().youngs / z.material().density).sqrt()).fold(0.0f32, f32::max);
        Self {
            impact,
            h,
            zone,
            mass,
            rest,
            pos,
            vel,
            temp: vec![AMBIENT_K; n],
            bonds,
            bond_start: counts,
            bond_list: list,
            force: vec![Vec3::ZERO; n],
            dt: DT_CFL * h / c_max,
            t_s: 0.0,
            frames: Vec::new(),
            length_m,
            plate_work: 0.0,
            static_pa: (impact.resistance_pa - 0.5 * super::materials::RHA_DENSITY * impact.v_in * impact.v_in)
                .max(STATIC_SHARE * impact.resistance_pa),
            bonds_broken: 0,
            plate: std::collections::HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    fn bonds_of(&self, c: usize) -> &[u32] {
        &self.bond_list[self.bond_start[c] as usize..self.bond_start[c + 1] as usize]
    }

    /// A cell's outward normal from its intact neighbours (zero inside the round).
    fn outward(&self, c: usize) -> Vec3 {
        let mut sum = Vec3::ZERO;
        let mut intact = 0;
        for &b in self.bonds_of(c) {
            let bond = &self.bonds[b as usize];
            if bond.broken {
                continue;
            }
            intact += 1;
            let other = if bond.a as usize == c { bond.b } else { bond.a } as usize;
            sum += (self.pos[c] - self.pos[other]) / bond.rest;
        }
        if intact == 18 {
            return Vec3::ZERO;
        }
        sum.try_normalize().unwrap_or(Vec3::ZERO)
    }

    fn kinetic(&self) -> f64 {
        self.vel.iter().zip(&self.mass).map(|(v, &m)| 0.5 * m as f64 * v.length_squared() as f64).sum()
    }

    /// One step: bonds, the plate's contact, then symplectic Euler. Returns whether any cell
    /// pressed on the plate.
    fn step(&mut self) -> bool {
        let n = self.len();
        self.force.iter_mut().for_each(|f| *f = Vec3::ZERO);
        let mut heat = vec![0.0f32; n];
        // Each cell's dilatation from its intact bonds: the mean bond strain, three times.
        let mut sum = vec![0.0f32; n];
        let mut count = vec![0u16; n];
        for bond in self.bonds.iter().filter(|b| !b.broken) {
            let (a, b) = (bond.a as usize, bond.b as usize);
            let strain = (self.pos[b] - self.pos[a]).length() / bond.rest - 1.0;
            sum[a] += strain;
            sum[b] += strain;
            count[a] += 1;
            count[b] += 1;
        }
        let mut newly_broken = 0usize;
        let dilatation: Vec<f32> = sum.iter().zip(&count).map(|(&s, &c)| if c > 0 { 3.0 * s / c as f32 } else { 0.0 }).collect();
        for bond in self.bonds.iter_mut() {
            let (a, b) = (bond.a as usize, bond.b as usize);
            let d = self.pos[b] - self.pos[a];
            let len = d.length();
            if len < 1e-12 {
                continue;
            }
            let dir = d / len;
            let strain = len / bond.rest - 1.0;
            if bond.broken {
                // Broken neighbours only push apart when pressed together, from the length the
                // bond had flowed to.
                let pressed = strain - bond.plastic.min(0.0);
                if pressed < 0.0 {
                    let f = bond.k * bond.rest * pressed;
                    self.force[a] += dir * f;
                    self.force[b] -= dir * f;
                }
                continue;
            }
            let t_mean = 0.5 * (self.temp[a] + self.temp[b]);
            let soft = (1.0 - ((t_mean - AMBIENT_K) / (bond.melt_k - AMBIENT_K)).clamp(0.0, 1.0)).max(0.0);
            let limit = (bond.yield_strain + bond.hardening * bond.flow) * soft;
            // The bond yields on its deviatoric part (its strain less its ends' mean volumetric
            // share), so a confined cell under pressure alone stays elastic (named: a bond-level
            // split, not a state-based solid's von Mises return).
            let volumetric = (dilatation[a] + dilatation[b]) / 6.0;
            let deviatoric = strain - volumetric - bond.plastic;
            if deviatoric.abs() > limit {
                let excess = deviatoric.abs() - limit;
                bond.plastic += excess * deviatoric.signum();
                bond.flow += excess;
                // Plastic work at the flow force over the plastic lengthening, to heat.
                let work = bond.k * bond.rest * limit * bond.rest * excess * TAYLOR_QUINNEY;
                heat[a] += 0.5 * work;
                heat[b] += 0.5 * work;
            }
            let elastic = strain - bond.plastic;
            if strain > bond.yield_strain + bond.fail || bond.flow > EXHAUSTION * bond.fail {
                bond.broken = true;
                newly_broken += 1;
                continue;
            }
            let f = bond.k * bond.rest * elastic;
            self.force[a] += dir * f;
            self.force[b] -= dir * f;
        }
        self.bonds_broken += newly_broken;
        // Cells that were not neighbours push apart when they come within 0.9 of a cell (pieces
        // and loose cells meeting the rest of the round), through a hashed grid.
        let reach = 0.9 * self.h;
        let k_contact = 0.4 * SHOT_BODY.youngs * self.h;
        if self.bonds_broken > 0 {
            // Only cells open to a surface or a crack can meet a stranger: the rest are walled in.
            let open: Vec<u32> = (0..n as u32).filter(|&c| count[c as usize] < 18).collect();
            let cell_of = |p: Vec3| [(p.x / reach).floor() as i32, (p.y / reach).floor() as i32, (p.z / reach).floor() as i32];
            let size = (open.len() * 2).next_power_of_two().max(64);
            let slot = |k: [i32; 3]| {
                let h = (k[0] as u32).wrapping_mul(73_856_093) ^ (k[1] as u32).wrapping_mul(19_349_663) ^ (k[2] as u32).wrapping_mul(83_492_791);
                (h as usize) & (size - 1)
            };
            let mut head = vec![u32::MAX; size];
            let mut next = vec![u32::MAX; n];
            for &c in &open {
                let s = slot(cell_of(self.pos[c as usize]));
                next[c as usize] = head[s];
                head[s] = c;
            }
            for &c in &open {
                let c = c as usize;
                let p = self.pos[c];
                let [i, j, k] = cell_of(p);
                for dz in -1..=1 {
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let mut o = head[slot([i + dx, j + dy, k + dz])];
                            while o != u32::MAX {
                                let ou = o as usize;
                                o = next[ou];
                                if ou <= c || (self.rest[ou] - self.rest[c]).length_squared() < 2.5 * self.h * self.h {
                                    continue;
                                }
                                let d = self.pos[ou] - p;
                                let len = d.length();
                                if len >= reach || len < 1e-9 {
                                    continue;
                                }
                                let f = d / len * (k_contact * (reach - len));
                                self.force[c] -= f;
                                self.force[ou] += f;
                            }
                        }
                    }
                }
            }
        }
        // The plate: a cell of the round in a voxel of plate that is still there, and advancing
        // into it, is pressed back along its own outward normal at the plate's flow pressure,
        // with friction along it; the work goes into that voxel until its share of the plate's
        // resistance is spent and it is gone (the cavity and the gouge).
        let thick = self.impact.thickness_m;
        let relief = FREE_SURFACE_CALIBRES * self.impact.design.calibre_m;
        let area = self.h * self.h;
        let voxel_energy = self.impact.resistance_pa * self.h * self.h * self.h;
        let mut touched = false;
        for c in 0..n {
            let p = self.pos[c];
            if p.z <= 0.0 || p.z >= thick || count[c] >= 18 {
                continue;
            }
            let key = ((p.x / self.h).floor() as i32, (p.y / self.h).floor() as i32, (p.z / self.h).floor() as i32);
            let centre_z = (key.2 as f32 + 0.5) * self.h;
            let ramp = (centre_z / relief).min((thick - centre_z) / relief).clamp(0.05, 1.0);
            let left = *self.plate.get(&key).unwrap_or(&(voxel_energy * ramp));
            if left <= 0.0 {
                continue;
            }
            let v = self.vel[c];
            // A loose cell (no intact bond) meets the plate along its own motion.
            let normal = match self.outward(c) {
                n if n != Vec3::ZERO => n,
                _ if count[c] == 0 => v.normalize_or_zero(),
                _ => continue,
            };
            let vn = v.dot(normal);
            if vn <= 0.0 {
                continue;
            }
            touched = true;
            // Cavity expansion: a static flow pressure plus the plate's inertia at the speed the
            // surface advances into it (Forrestal's sigma_n = R + rho v_n^2), the static part set
            // so that a nose meeting the plate at half its speed squared on average gives the
            // calibrated mean (`STATIC_SHARE` floor, authored).
            let fn_ = (self.static_pa + super::materials::RHA_DENSITY * vn * vn) * ramp * area;
            let tangential = v - normal * vn;
            let slide = tangential.try_normalize().unwrap_or(Vec3::ZERO);
            let f = -normal * fn_ - slide * FRICTION * fn_;
            self.force[c] += f;
            let work = (-f.dot(v) * self.dt).max(0.0);
            self.plate_work += work as f64;
            self.plate.insert(key, left - work);
            let friction_work = FRICTION * fn_ * tangential.length() * self.dt;
            heat[c] += FRICTION_HEAT_TO_SHOT * friction_work;
        }
        for c in 0..n {
            let m = self.mass[c];
            self.vel[c] += self.force[c] / m * self.dt;
            self.pos[c] += self.vel[c] * self.dt;
            if heat[c] > 0.0 {
                self.temp[c] += heat[c] / (m * self.zone[c].material().specific_heat);
            }
        }
        self.t_s += self.dt;
        touched
    }

    fn damage(&self) -> Vec<u8> {
        (0..self.len()).map(|c| {
            let bonds = self.bonds_of(c);
            let broken = bonds.iter().filter(|&&b| self.bonds[b as usize].broken).count();
            (255 * broken / bonds.len().max(1)) as u8
        }).collect()
    }

    fn keep_frame(&mut self) {
        let damage = self.damage();
        let mut gone: Vec<(i32, i32, i32)> = self.plate.iter().filter(|(_, &e)| e <= 0.0).map(|(k, _)| *k).collect();
        gone.sort_unstable();
        self.frames.push(Frame { t_s: self.t_s, pos: self.pos.clone(), temp: self.temp.clone(), damage, gone });
    }

    /// Advance only completed CFL steps up to the requested physical time. The remainder
    /// stays as time debt; neither a final shape nor a future fracture is sampled here.
    pub fn advance_steps(&mut self, count:u64) {for _ in 0..count {self.step();}}

    pub fn advance_until(&mut self, until_s: f32) -> u64 {
        let mut steps = 0;
        while self.t_s + self.dt <= until_s + 1e-10 {
            self.step();
            steps += 1;
        }
        steps
    }

    /// Run the strike over its horizon: the time for the round to cross twice the plate and its own
    /// length and a calibre at its striking speed, two and a half times over (a fixed span, so the
    /// solve's length never depends on how its debris settles).
    pub fn solve(&mut self) -> ShotOutcome {
        self.solve_for(HORIZON)
    }

    /// The strike over `crossings` crossing times (see `solve`).
    pub fn solve_for(&mut self, crossings: f32) -> ShotOutcome {
        let kinetic_in = self.kinetic();
        let horizon = crossings * (self.length_m + 2.0 * self.impact.thickness_m + self.impact.design.calibre_m) / self.impact.v_in.max(100.0);
        let steps_total = ((horizon / self.dt) as u64).clamp(1, MAX_STEPS);
        let every = (steps_total / FRAMES as u64).max(1);
        let mut deepest = f32::MIN;
        self.keep_frame();
        for step in 1..=steps_total {
            self.step();
            let tip = self.pos.iter().map(|p| p.z).fold(f32::MIN, f32::max);
            deepest = deepest.max(tip);
            if step % every == 0 {
                self.keep_frame();
            }
        }
        self.outcome(steps_total, kinetic_in, deepest)
    }

    /// The round's pieces (cells joined by intact bonds), largest first.
    pub fn pieces(&self) -> Vec<Piece> {
        let n = self.len();
        let mut parent: Vec<u32> = (0..n as u32).collect();
        fn find(p: &mut [u32], mut x: u32) -> u32 {
            while p[x as usize] != x {
                p[x as usize] = p[p[x as usize] as usize];
                x = p[x as usize];
            }
            x
        }
        for b in self.bonds.iter().filter(|b| !b.broken) {
            let (ra, rb) = (find(&mut parent, b.a), find(&mut parent, b.b));
            if ra != rb {
                parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
        let mut groups: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
        for c in 0..n as u32 {
            groups.entry(find(&mut parent, c)).or_default().push(c);
        }
        let mut out: Vec<Piece> = groups.into_values().map(|cells| self.piece(cells)).collect();
        out.sort_by(|a, b| b.mass_kg.total_cmp(&a.mass_kg));
        out
    }

    fn piece(&self, cells: Vec<u32>) -> Piece {
        let mass: f32 = cells.iter().map(|&c| self.mass[c as usize]).sum();
        let centre = cells.iter().map(|&c| self.pos[c as usize] * self.mass[c as usize]).sum::<Vec3>() / mass;
        let velocity = cells.iter().map(|&c| self.vel[c as usize] * self.mass[c as usize]).sum::<Vec3>() / mass;
        let mut momentum = Vec3::ZERO;
        let mut inertia = Mat3::ZERO;
        let cell_i = self.h * self.h / 6.0;
        for &c in &cells {
            let (m, r) = (self.mass[c as usize], self.pos[c as usize] - centre);
            momentum += r.cross(self.vel[c as usize] - velocity) * m;
            inertia += (Mat3::IDENTITY * (r.length_squared() + cell_i) - outer(r)) * m;
        }
        let spin = if inertia.determinant().abs() > 1e-30 { inertia.inverse() * momentum } else { Vec3::ZERO };
        let temperature = cells.iter().map(|&c| self.temp[c as usize] * self.mass[c as usize]).sum::<f32>() / mass;
        let mut by_zone: Vec<(Zone, f32)> = Vec::new();
        for &c in &cells {
            let z = self.zone[c as usize];
            match by_zone.iter_mut().find(|e| e.0 == z) {
                Some(e) => e.1 += self.mass[c as usize],
                None => by_zone.push((z, self.mass[c as usize])),
            }
        }
        let zone = by_zone.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(Zone::Body, |e| e.0);
        let size = (cells.len() as f32).cbrt() * self.h;
        Piece { cell_count: cells.len(), cells, mass_kg: mass, centre, velocity, spin, temperature_k: temperature, size_m: size, zone }
    }

    fn outcome(&self, steps: u64, kinetic_in: f64, deepest: f32) -> ShotOutcome {
        let pieces = self.pieces();
        let main = &pieces[0];
        // The residual's axis: its rest axis carried by the best rotation of its cells (the polar
        // part of their deformation, by a few iterations of the polar decomposition).
        let rest_centre = main.cells.iter().map(|&c| self.rest[c as usize] * self.mass[c as usize]).sum::<Vec3>() / main.mass_kg;
        let mut apq = Mat3::ZERO;
        for &c in &main.cells {
            let (p, q) = (self.pos[c as usize] - main.centre, self.rest[c as usize] - rest_centre);
            apq += outer_pq(p, q) * self.mass[c as usize];
        }
        let rot = polar_rotation(apq);
        let axis = (rot * Vec3::Z).normalize_or_zero();
        let along: Vec<f32> = main.cells.iter().map(|&c| (self.pos[c as usize] - main.centre).dot(axis)).collect();
        let (lo, hi) = along.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &a| (lo.min(a), hi.max(a)));
        let rest_along: Vec<f32> = main.cells.iter().map(|&c| self.rest[c as usize].z).collect();
        let (rlo, rhi) = rest_along.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &a| (lo.min(a), hi.max(a)));
        let radius = main.cells.iter().map(|&c| {
            let r = self.pos[c as usize] - main.centre;
            (r - axis * r.dot(axis)).length()
        }).fold(0.0f32, f32::max);
        // Bend: the axes through the front and rear halves' centres, each from the residual's
        // centre.
        let mid = 0.5 * (rlo + rhi);
        let half_centre = |front: bool| {
            let sel: Vec<u32> = main.cells.iter().copied().filter(|&c| (self.rest[c as usize].z > mid) == front).collect();
            let m: f32 = sel.iter().map(|&c| self.mass[c as usize]).sum();
            if m <= 0.0 { return main.centre; }
            sel.iter().map(|&c| self.pos[c as usize] * self.mass[c as usize]).sum::<Vec3>() / m
        };
        let rest_half = |front: bool| {
            let sel: Vec<u32> = main.cells.iter().copied().filter(|&c| (self.rest[c as usize].z > mid) == front).collect();
            let m: f32 = sel.iter().map(|&c| self.mass[c as usize]).sum();
            if m <= 0.0 { return rest_centre; }
            sel.iter().map(|&c| self.rest[c as usize] * self.mass[c as usize]).sum::<Vec3>() / m
        };
        let (front, rear) = (half_centre(true), half_centre(false));
        let bend = {
            // The quarter points of a straight round lie on its axis; a bent one's do not.
            let a = (front - main.centre).normalize_or_zero();
            let b = (main.centre - rear).normalize_or_zero();
            let straight = (rest_half(true) - rest_half(false)).length() > 0.0;
            if straight { a.dot(b).clamp(-1.0, 1.0).acos().to_degrees() } else { 0.0 }
        };
        let tumble = (main.spin - axis * main.spin.dot(axis)).length();
        let max_plastic = self.bonds.iter().map(|b| b.flow).fold(0.0f32, f32::max);
        let kinetic_out = self.kinetic();
        let tip_min = main.cells.iter().map(|&c| self.pos[c as usize].z).fold(f32::MAX, f32::min);
        ShotOutcome {
            cells: self.len(),
            cell_m: self.h,
            steps,
            duration_s: self.t_s,
            mass_kg: self.mass.iter().sum(),
            residual_kg: main.mass_kg,
            residual_velocity: main.velocity,
            residual_spin: main.spin,
            tumble_rad_s: tumble,
            broken_bonds: self.bonds.iter().filter(|b| b.broken).count(),
            max_plastic,
            length_ratio: (hi - lo + self.h) / (rhi - rlo + self.h),
            radius_ratio: (radius + 0.5 * self.h) / (0.5 * self.impact.design.calibre_m),
            bend_deg: bend,
            peak_temp_k: self.temp.iter().copied().fold(AMBIENT_K, f32::max),
            plate_work_j: self.plate_work as f32,
            kinetic_in_j: kinetic_in as f32,
            kinetic_out_j: kinetic_out as f32,
            deepest_m: deepest.max(0.0),
            exited_back: tip_min > self.impact.thickness_m,
            rebounded: main.velocity.z < 0.0,
            pieces,
        }
    }
}

/// A piece of the round that went through the plate, as it left the back face (plate frame).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Shrapnel {
    pub pos: Vec3,
    pub vel: Vec3,
    pub spin: Vec3,
    pub size_m: f32,
    pub mass_kg: f32,
    pub temperature_k: f32,
}

impl ShotOutcome {
    /// The pieces of the round other than its residual that are in or behind the plate and moving
    /// on into the vehicle faster than `SHRAPNEL_MIN_MPS` (filler left out: it is the burst's),
    /// each placed where its path crosses the back face (a piece still in the cavity goes on along
    /// it unresisted: named).
    pub fn shrapnel(&self, thickness_m: f32) -> Vec<Shrapnel> {
        self.pieces.iter().skip(1).filter(|p| p.zone != Zone::Filler && p.velocity.z > SHRAPNEL_MIN_MPS && p.centre.z > 0.0)
            .map(|p| {
                let back = (p.centre.z - thickness_m) / p.velocity.z;
                let mut pos = p.centre - p.velocity * back;
                pos.z = thickness_m + 0.5 * p.size_m;
                Shrapnel { pos, vel: p.velocity, spin: p.spin, size_m: p.size_m, mass_kg: p.mass_kg, temperature_k: p.temperature_k }
            }).collect()
    }
}

fn outer(r: Vec3) -> Mat3 {
    Mat3::from_cols(r * r.x, r * r.y, r * r.z)
}

fn outer_pq(p: Vec3, q: Vec3) -> Mat3 {
    Mat3::from_cols(p * q.x, p * q.y, p * q.z)
}

/// The rotation of a matrix's polar decomposition (Higham's iteration).
fn polar_rotation(m: Mat3) -> Mat3 {
    let mut r = m;
    for _ in 0..20 {
        let det = r.determinant();
        if det.abs() < 1e-20 {
            return Mat3::IDENTITY;
        }
        r = (r + r.inverse().transpose()) * 0.5;
    }
    if r.determinant() < 0.0 { Mat3::IDENTITY } else { r }
}

/// A shot's strike on a plate from the calibrated event: the round's build, its speed and angle,
/// the plate's thickness and its flow pressure. `None` for rounds without a kinetic body.
pub fn impact_of(event: &crate::ballistics::ImpactEvent, seed: u32) -> Option<ShotImpact> {
    let design = design(event.shell)?;
    let quality = match event.plate.material {
        crate::ballistics::ArmorMaterial::Cast => 0.94,
        crate::ballistics::ArmorMaterial::LateGerman => 0.92,
        _ => 1.0,
    };
    let jet=event.shell.kind==ShellKind::Heat;
    let v=if jet{2500.0}else{event.striking_v};
    Some(ShotImpact {
        design,
        v_in:v,
        obliquity_rad: event.impact_angle_deg.to_radians().min(1.45),
        thickness_m: event.plate.thickness_mm * 0.001,
        resistance_pa: if crate::ballistics::is_hesh(event.shell){2.0e11}
            else {resistance_pa(event.shell, &design,v, quality)},
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ballistics::{BR_350B, BR_354P, BR_471, PZGR_39};

    fn strike(shell: &'static ShellSpec, v: f32, angle_deg: f32, plate_mm: f32) -> ShotImpact {
        let design = design(shell).unwrap();
        ShotImpact {
            design,
            v_in: v,
            obliquity_rad: angle_deg.to_radians(),
            thickness_m: plate_mm * 0.001,
            resistance_pa: resistance_pa(shell, &design, v, 1.0),
            seed: 7,
        }
    }

    #[test]
    fn each_round_is_built_of_its_parts_at_its_mass() {
        for (shell, want) in [
            (&BR_471, vec![Zone::Nose, Zone::Body, Zone::Filler]),
            (&PZGR_39, vec![Zone::Nose, Zone::Cap, Zone::Windshield, Zone::Filler]),
            (&BR_354P, vec![Zone::Core, Zone::Carrier, Zone::Windshield]),
            (&BR_350B, vec![Zone::Nose, Zone::Windshield, Zone::Filler]),
        ] {
            let shot = Shot::new(strike(shell, 700.0, 0.0, 100.0));
            let total: f32 = shot.mass.iter().sum();
            assert!((total - shell.mass_kg).abs() < 1e-3 * shell.mass_kg, "{}: {total}", shell.name);
            for z in want {
                assert!(shot.zone.contains(&z), "{} has {z:?}", shell.name);
            }
            let filler: f32 = shot.zone.iter().zip(&shot.mass).filter(|(z, _)| **z == Zone::Filler).map(|(_, m)| m).sum();
            if shell.filler_g > 0.0 && shell.kind != ShellKind::Apcr {
                assert!((filler - shell.filler_g * 0.001).abs() < 0.5 * shell.filler_g * 0.001, "{}: filler {filler}", shell.name);
            }
            assert!(shot.len() > 1000 && shot.len() < 20_000, "{}: {} cells", shell.name, shot.len());
        }
        assert!(design(&crate::ballistics::OF_530).is_none());
    }

    #[test]
    fn a_slow_round_stays_whole() {
        let mut shot = Shot::new(strike(&BR_471, 60.0, 0.0, 100.0));
        let out = shot.solve();
        assert_eq!(out.broken_bonds, 0, "{out:?}");
        // Only the point's own cell may yield a little; the round keeps its length.
        assert!(out.length_ratio > 0.99, "{}", out.length_ratio);
        assert_eq!(out.pieces.len(), 1);
        assert!(out.kinetic_out_j <= out.kinetic_in_j * 1.001);
    }

    #[test]
    fn a_hard_stop_flattens_the_nose_and_heats_it() {
        let mut shot = Shot::new(strike(&BR_350B, 650.0, 0.0, 160.0));
        let out = shot.solve();
        assert!(!out.exited_back && !out.rebounded || out.residual_velocity.z.abs() < 50.0, "{out:?}");
        assert!(out.length_ratio < 0.97, "the round shortens: {}", out.length_ratio);
        assert!(out.radius_ratio > 1.03, "the nose spreads: {}", out.radius_ratio);
        assert!(out.peak_temp_k > AMBIENT_K + 100.0, "{}", out.peak_temp_k);
        assert!(out.kinetic_out_j < 0.05 * out.kinetic_in_j);
    }

    #[test]
    fn a_tungsten_core_breaks_on_thick_plate() {
        let mut shot = Shot::new(strike(&BR_354P, 900.0, 30.0, 160.0));
        let out = shot.solve();
        let core_pieces = out.pieces.iter().filter(|p| p.zone == Zone::Core).count();
        assert!(core_pieces >= 2, "the brittle core breaks: {core_pieces} pieces of core, {} in all", out.pieces.len());
    }

    #[test]
    fn a_glancing_hit_turns_the_round_out_and_sets_it_tumbling() {
        let mut shot = Shot::new(strike(&BR_471, 700.0, 72.0, 100.0));
        let out = shot.solve();
        assert!(out.rebounded, "it leaves the face: {:?}", out.residual_velocity);
        assert!(out.tumble_rad_s > 50.0, "it tumbles: {}", out.tumble_rad_s);
        assert!(out.kinetic_out_j < out.kinetic_in_j);
        let normal = Shot::new(strike(&BR_471, 60.0, 0.0, 100.0)).solve();
        assert!(out.tumble_rad_s > 10.0 * normal.tumble_rad_s.max(0.1));
    }

    #[test]
    fn a_capped_round_through_the_plate_sends_pieces_of_itself_in() {
        let impact = strike(&PZGR_39, 770.0, 30.0, 100.0);
        let out = Shot::with_cells(impact, 8.0).solve_for(GAME_HORIZON);
        let shrapnel = out.shrapnel(impact.thickness_m);
        assert!(!shrapnel.is_empty(), "{} pieces, residual {:.2} kg", out.pieces.len(), out.residual_kg);
        for s in &shrapnel {
            assert!((s.pos.z - impact.thickness_m).abs() < s.size_m + 1e-3 && s.vel.z > SHRAPNEL_MIN_MPS);
        }
        let shed: f32 = shrapnel.iter().map(|s| s.mass_kg).sum();
        assert!(out.residual_kg + shed <= out.mass_kg + 1e-3);
    }

    #[test]
    fn the_solve_is_repeatable() {
        let a = Shot::new(strike(&BR_471, 650.0, 40.0, 80.0)).solve();
        let b = Shot::new(strike(&BR_471, 650.0, 40.0, 80.0)).solve();
        assert_eq!(a.steps, b.steps);
        assert_eq!(a.broken_bonds, b.broken_bonds);
        assert_eq!(a.residual_kg.to_bits(), b.residual_kg.to_bits());
    }

    #[test]
    #[ignore]
    fn print_game_costs() {
        for cells in [8.0, 10.0, 12.0] {
            for (shell, v, angle, plate) in [(&BR_471, 780.0, 0.0, 100.0), (&PZGR_39, 770.0, 30.0, 100.0), (&BR_354P, 950.0, 0.0, 60.0)] {
                let started = std::time::Instant::now();
                let mut shot = Shot::with_cells(strike(shell, v, angle, plate), cells);
                let n = shot.len();
                let out = shot.solve_for(GAME_HORIZON);
                println!("{cells} across {}: {n} cells {} steps {:.0} ms; residual {:.2}/{:.2} kg at {:.0} m/s, {} pieces",
                    shell.name, out.steps, started.elapsed().as_secs_f32() * 1000.0, out.residual_kg, out.mass_kg,
                    out.residual_velocity.length(), out.pieces.len());
            }
        }
    }

    #[test]
    #[ignore]
    fn print_shot_cases() {
        for (shell, v, angle, plate) in [(&BR_471, 780.0, 0.0, 100.0), (&BR_471, 700.0, 72.0, 100.0), (&BR_350B, 650.0, 0.0, 160.0),
            (&BR_354P, 900.0, 30.0, 160.0), (&PZGR_39, 770.0, 30.0, 100.0)] {
            let started = std::time::Instant::now();
            let mut shot = Shot::new(strike(shell, v, angle, plate));
            let cells = shot.len();
            let out = shot.solve();
            println!("{} {v} m/s {angle} deg {plate} mm: {cells} cells, {} steps {:.0} us in {:.0} ms; residual {:.2}/{:.2} kg at {:.0?} m/s, tumble {:.0} rad/s, {} pieces, length x{:.2} radius x{:.2} bend {:.1} deg, peak {:.0} K, plastic {:.2}, deepest {:.0} mm, exited {} rebounded {}",
                shell.name, out.steps, out.duration_s * 1e6, started.elapsed().as_secs_f32() * 1000.0, out.residual_kg, out.mass_kg,
                out.residual_velocity, out.tumble_rad_s, out.pieces.len(), out.length_ratio, out.radius_ratio, out.bend_deg, out.peak_temp_k,
                out.max_plastic, out.deepest_m * 1000.0, out.exited_back, out.rebounded);
            let mut lost: Vec<(Zone, f32, usize)> = Vec::new();
            for piece in &out.pieces[1..] {
                for &c in &piece.cells {
                    let z = shot.zone[c as usize];
                    match lost.iter_mut().find(|e| e.0 == z) {
                        Some(e) => { e.1 += shot.mass[c as usize]; e.2 += usize::from(piece.cell_count == 1); }
                        None => lost.push((z, shot.mass[c as usize], usize::from(piece.cell_count == 1))),
                    }
                }
            }
            let sizes: Vec<usize> = out.pieces.iter().take(8).map(|p| p.cell_count).collect();
            println!("    lost by zone (kg, single cells): {lost:?}; largest pieces {sizes:?}");
        }
    }
}

#[cfg(test)]
mod review_profile_tests {
    use super::*;
    #[test]
    fn modern_rounds_have_distinct_material_bodies_and_no_jet_filler(){
        let rod=design(&crate::ballistics::M829A2_REVIEW).unwrap();
        let jet=design(&crate::ballistics::HEAT_REVIEW).unwrap();
        let hesh=design(&crate::ballistics::HESH_REVIEW).unwrap();
        assert_eq!(rod.homogeneous,Some(Zone::Rod));assert!(rod.calibre_m<0.04);
        assert_eq!(jet.homogeneous,Some(Zone::Jet));assert_eq!(jet.filler_kg,0.);
        assert!(jet.mass_kg<crate::ballistics::HEAT_REVIEW.mass_kg);
        assert_eq!(hesh.homogeneous,Some(Zone::SoftCase));assert_eq!(hesh.calibre_m,0.120000005);
    }
}
