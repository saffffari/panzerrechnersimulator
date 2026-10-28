//! The tank: pose, turret/gun lay, drive over the heightfield, and the per-part transforms the
//! renderer needs. Geometry is procedural low-poly boxes for now (the real Tiger comes from the
//! Blender rebuild, Task 8). Tank-local frame: +Z forward, +Y up, +X tank-left (physical right is
//! −X), origin at ground contact. Used for both the player Tiger and the T-34 enemy.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec2, Vec3};

use crate::gltf_load::CpuMesh;
use crate::prims::box_mesh;
use crate::terrain::Heightfield;

/// Fallback muzzle-clearance distance for the hitscan near-plane (so a shot never self-hits the
/// firing tank). Real barrel length lives per-tank in `Parts::gun_len`.
pub const GUN_LEN: f32 = 3.0;

const TRAVERSE: f32 = 0.10472; // 6 deg/s
                               // AI turret slew is faster than the hand-cranked player traverse so an enemy can keep its gun on a
                               // target while its hull is maneuvering/flanking (otherwise a 6 deg/s turret can never catch up to a
                               // bearing that's changing as the tank drives past). ~23 deg/s — brisk, but not an instant snap.
const AI_TRAVERSE: f32 = 0.80;
const AI_ELEV_RATE: f32 = crate::anti_infantry_ai::GUN_ELEVATION_RATE;
pub(crate) const ELEV_RATE: f32 = 0.06981; // 4 deg/s
const GUN_MIN: f32 = crate::anti_infantry_ai::GUN_MIN_PITCH;
const GUN_MAX: f32 = crate::anti_infantry_ai::GUN_MAX_PITCH;
pub const TOP_SPEED: f32 = 12.0; // m/s
const ACCEL: f32 = 6.0;
const TURN_RATE: f32 = 0.7; // rad/s (neutral steer)
/// Measured contact-line centers in the baked Tiger support profile (+X is left).
pub(crate) const TIGER_TRACK_HALF_SPACING: f32 = crate::tank_support::TIGER_TRACK_X;

/// Physical belt availability, independent of whether the engine is running. Commands are gated
/// upstream by engine/transmission/operator state; losing a belt never shuts the engine down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TrackTraction {
    pub left: bool,
    pub right: bool,
}

impl TrackTraction {
    pub const INTACT: Self = Self { left: true, right: true };

    pub fn any(self) -> bool { self.left || self.right }

    pub fn gate_commands(self, drivetrain_available: bool, operator_available: bool,
        drive: f32, steer: f32) -> (f32, f32) {
        if drivetrain_available && operator_available && self.any() {
            (drive, steer)
        } else {
            (0.0, 0.0)
        }
    }
}

// The player's main gun is now manually reloaded (breech eject + ram, driven from the Loader
// station in game.rs), so fired() only locks the gun for its recoil/return cycle. Enemies set
// their own cadence directly; the travel-lock in can_fire still forbids firing on the move.
const RETURN_TO_BATTERY: f32 = 1.0;

// Visual-only KwK recoil. Ballistics always use `gun_model()` at the laid, in-battery pose; the
// renderer may use `visual_gun_model()` to show the barrel and breech running independently of the
// authoritative muzzle. Keeping this curve time-based makes the violent first stroke identical at
// 30, 60 and 120 Hz.
const GUN_RECOIL_SLAM_END: f32 = 0.055;
const GUN_RECOIL_HOLD_END: f32 = 0.095;
const GUN_RECOIL_RETURN_END: f32 = 0.620;
pub(crate) const GUN_RECOIL_END: f32 = 0.760;
const GUN_RECOIL_TRAVEL: f32 = 0.320;
const GUN_RECOIL_SETTLE: f32 = 0.015;

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed local distance behind the in-battery pose. Positive values run toward gun-local -Z;
/// the tiny negative tail is the recuperator's damped forward overshoot.
fn gun_recoil_distance(age: f32) -> f32 {
    if !age.is_finite() || age < 0.0 || age >= GUN_RECOIL_END {
        return 0.0;
    }
    if age < GUN_RECOIL_SLAM_END {
        let t = age / GUN_RECOIL_SLAM_END;
        // Fast pressure stroke: most of the travel lands early, but the curve remains continuous
        // from the exact firing pose at t=0.
        return GUN_RECOIL_TRAVEL * (1.0 - (1.0 - t).powi(3));
    }
    if age < GUN_RECOIL_HOLD_END {
        return GUN_RECOIL_TRAVEL;
    }
    if age < GUN_RECOIL_RETURN_END {
        let t = (age - GUN_RECOIL_HOLD_END) / (GUN_RECOIL_RETURN_END - GUN_RECOIL_HOLD_END);
        return GUN_RECOIL_TRAVEL * (1.0 - smoothstep01(t));
    }

    let t = (age - GUN_RECOIL_RETURN_END) / (GUN_RECOIL_END - GUN_RECOIL_RETURN_END);
    -GUN_RECOIL_SETTLE * (std::f32::consts::TAU * t).sin() * (1.0 - t).powi(2)
}

/// Which enemy vehicle a `Tank` is. Drives the CASEMATE rig (an SU-100 has no rotating turret — its
/// fighting compartment is welded to the hull, so it aims by turning the whole hull) and the
/// kind-aware penetration + overwatch AI in `game.rs`. The player Tiger carries `T34` here purely as a
/// harmless default: only `Su100` changes the turret/gun transforms, and the player's kind is never
/// read by the enemy AI / penetration paths.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind {
    T34,
    Su100,
    Kv2,
    Isu152,
    Is2,
    Is3,
}

impl EnemyKind {
    pub fn name(self) -> &'static str {
        match self {
            EnemyKind::T34 => "T-34",
            EnemyKind::Su100 => "SU-100",
            EnemyKind::Kv2 => "KV-2",
            EnemyKind::Isu152 => "ISU-152",
            EnemyKind::Is2 => "IS-2",
            EnemyKind::Is3 => "IS-3",
        }
    }
    /// A casemate rig has NO rotating turret — azimuth comes entirely from turning the hull.
    pub fn casemate(self) -> bool {
        matches!(self, EnemyKind::Su100 | EnemyKind::Isu152)
    }
}

/// A tank hull's oriented footprint on the ground plane. `half_extents` are the local half-width
/// (+/-X, including tracks) and half-length (+/-Z); `heading` follows the same +Z-forward yaw used
/// by [`Tank`]. Keeping this geometry independent of the render meshes makes contact tests cheap,
/// deterministic, and available in headless training.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullFootprint {
    pub center: Vec2,
    pub half_extents: Vec2,
    pub heading: f32,
}

/// Minimum-translation contact returned by [`yaw_obb_contact`]. `normal` points from the first
/// footprint toward the second; moving the second footprint by [`Self::minimum_translation`]
/// separates the pair along the shallowest SAT axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullContact {
    pub normal: Vec2,
    pub depth: f32,
}

impl HullContact {
    pub fn minimum_translation(self) -> Vec2 {
        self.normal * self.depth
    }
}

/// Deterministic XZ yaw-OBB contact using the separating-axis theorem. Edge-touching footprints
/// are not penetrating and return `None`. Invalid/non-finite inputs also return `None`, keeping a
/// bad pose from injecting NaNs into the simulation. For exactly coincident centres, the stable
/// axis order (first hull X, first hull Z, second hull X, second hull Z) breaks the direction tie.
pub fn yaw_obb_contact(a: HullFootprint, b: HullFootprint) -> Option<HullContact> {
    if !a.center.is_finite()
        || !b.center.is_finite()
        || !a.half_extents.is_finite()
        || !b.half_extents.is_finite()
        || !a.heading.is_finite()
        || !b.heading.is_finite()
        || a.half_extents.cmple(Vec2::ZERO).any()
        || b.half_extents.cmple(Vec2::ZERO).any()
    {
        return None;
    }

    let axes = |heading: f32| {
        let (sin, cos) = heading.sin_cos();
        // Mat4::from_rotation_y(heading) maps local +X to (cos, -sin) in XZ and local +Z to
        // (sin, cos), matching Tank::forward/hull_model exactly.
        [Vec2::new(cos, -sin), Vec2::new(sin, cos)]
    };
    let a_axes = axes(a.heading);
    let b_axes = axes(b.heading);
    let delta = b.center - a.center;
    let mut best_depth = f32::INFINITY;
    let mut best_normal = Vec2::ZERO;

    for axis in [a_axes[0], a_axes[1], b_axes[0], b_axes[1]] {
        let a_radius = a.half_extents.x * a_axes[0].dot(axis).abs()
            + a.half_extents.y * a_axes[1].dot(axis).abs();
        let b_radius = b.half_extents.x * b_axes[0].dot(axis).abs()
            + b.half_extents.y * b_axes[1].dot(axis).abs();
        let signed_distance = delta.dot(axis);
        let overlap = a_radius + b_radius - signed_distance.abs();
        if overlap <= 0.0 {
            return None;
        }
        if overlap < best_depth {
            best_depth = overlap;
            best_normal = if signed_distance < 0.0 { -axis } else { axis };
        }
    }

    Some(HullContact {
        normal: best_normal,
        depth: best_depth,
    })
}

/// Per-tank rig geometry: pivot points and crew-eye offsets, extracted from the Blender bake so the
/// baked hull/turret/gun GLBs articulate correctly. Tiger and T-34 differ, so this is not global.
#[derive(Clone, Copy)]
pub struct Parts {
    pub turret_pivot: Vec3, // hull-local: turret ring / traverse axis
    pub gun_trunnion: Vec3, // turret-local: gun elevation axis
    pub gun_len: f32,       // gun-local +Z distance to the muzzle
    // TURRET-local: the commander's cupola is mounted on the turret roof on a real Tiger, so his
    // view rotates with BOTH hull steering and turret traverse (not hull-only).
    pub commander_eye: Vec3,
    pub center_h: f32, // hull-local Y of the hit-test centre
    /// Collision-only local XZ footprint half-extents (half-width including tracks, half-length).
    /// This does not scale or otherwise alter the authored render/ballistic model.
    pub hull_half_extents: Vec2,
}

/// Tiger: exact values from the baked model (turret_pivot / gun_pivot / POV cameras, engine frame).
pub const TIGER_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 0.524, 0.019),
    gun_trunnion: Vec3::new(0.004, 1.845, 1.040), // gun_pivot − turret_pivot
    gun_len: 4.79,
    commander_eye: Vec3::new(0.560, 2.414, -0.485), // POV_CMD_CLOSED − turret_pivot
    center_h: 1.5,
    hull_half_extents: Vec2::new(2.060_804_5, 3.563_222),
};

/// Source-registered telescope exit immediately beyond the front mantlet bore, in gun frame.
/// The source has no historical lens prescription; this locates the existing optic camera, it
/// does not assert a TZF variant or force the barrel into the resulting field of view.
pub const TIGER_SIGHT_OBJECTIVE: Vec3 = Vec3::new(0.494174, 0.153528, 0.5611);
/// The coaxial MG34's port in the source mantlet, gun frame: the through-hole on the gunner's
/// right that mirrors the sight aperture (centre found by ray-casting `tiger_gun.glb`, 2026-10-28:
/// x -0.524..-0.468, y 0.128..0.180), its muzzle at the mantlet face (z 0.51-0.56) plus a centimetre.
pub const TIGER_COAX_PORT: Vec3 = Vec3::new(-0.497, 0.153, 0.575);
/// Center of the named `gunner sight lens` in the unmodified turret export.
pub const TIGER_SIGHT_EYEPIECE: Vec3 = Vec3::new(0.4966112, 1.9940362, 0.5398246);

/// T-34: estimated rig (enemy, only ever seen at fog range; refine against a screenshot).
/// Measured from the tools/bake_t34.py pivot report (new_tanks T-34/76 1942 bake, 2026-07-02):
/// hull origin at ground-contact footprint center; turret origin on the ring; gun at the trunnion.
pub const T34_PARTS: Parts = Parts {
    turret_pivot: crate::anti_infantry_ai::F34_RIG.turret_pivot,
    gun_trunnion: crate::anti_infantry_ai::F34_RIG.trunnion,
    gun_len: crate::anti_infantry_ai::F34_RIG.barrel_length,
    commander_eye: Vec3::new(0.0, 0.62, -0.35), // turret-local: under the roof hatch, aft of the ring
    center_h: 1.4,
    hull_half_extents: Vec2::new(1.785_375, 3.531_021_5),
};

/// SU-100: measured engine-space pivots from the bake (`su100_cameras.json`). A CASEMATE tank
/// destroyer — the fixed fighting compartment has NO turret ring (`turret_pivot` unused/zero), so the
/// gun trunnion + commander eye are HULL-local, and the 100 mm D-10S is a long barrel (`gun_len`).
pub const SU100_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 0.0, 0.0), // fixed casemate — no traverse
    gun_trunnion: Vec3::new(0.20, 1.75, 2.35),
    gun_len: 4.75,
    commander_eye: Vec3::new(0.78, 2.30, -0.28),
    center_h: 1.40,
    hull_half_extents: Vec2::new(1.55, 3.20),
};

pub const KV2_PARTS: Parts = Parts {
    turret_pivot: crate::anti_infantry_ai::M10T_RIG.turret_pivot,
    gun_trunnion: crate::anti_infantry_ai::M10T_RIG.trunnion,
    gun_len: crate::anti_infantry_ai::M10T_RIG.barrel_length,
    commander_eye: Vec3::new(0.62, 1.62, -0.48),
    center_h: 1.65,
    hull_half_extents: Vec2::new(1.92, 3.35),
};

pub const ISU152_PARTS: Parts = Parts {
    turret_pivot: Vec3::ZERO,
    gun_trunnion: Vec3::new(0.18, 1.92, 2.25),
    gun_len: 4.35,
    commander_eye: Vec3::new(0.72, 2.30, -0.35),
    center_h: 1.45,
    hull_half_extents: Vec2::new(1.86, 3.45),
};

pub const IS2_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.72, -0.12),
    gun_trunnion: Vec3::new(0.0, 0.62, 0.92),
    gun_len: 4.75,
    commander_eye: Vec3::new(0.55, 0.92, -0.42),
    center_h: 1.45,
    hull_half_extents: Vec2::new(1.87, 3.45),
};

pub const IS3_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.58, -0.18),
    gun_trunnion: Vec3::new(0.0, 0.50, 1.02),
    gun_len: 4.75,
    commander_eye: Vec3::new(0.54, 0.72, -0.48),
    center_h: 1.35,
    hull_half_extents: Vec2::new(1.92, 3.52),
};

pub fn parts_for(kind: EnemyKind) -> Parts {
    match kind {
        EnemyKind::T34 => T34_PARTS,
        EnemyKind::Su100 => SU100_PARTS,
        EnemyKind::Kv2 => KV2_PARTS,
        EnemyKind::Isu152 => ISU152_PARTS,
        EnemyKind::Is2 => IS2_PARTS,
        EnemyKind::Is3 => IS3_PARTS,
    }
}

fn load_parts(hull: &str, turret: &str, gun: &str) -> Option<[CpuMesh; 3]> {
    Some([
        crate::gltf_load::load_glb(hull).ok()?,
        crate::gltf_load::load_glb(turret).ok()?,
        crate::gltf_load::load_glb(gun).ok()?,
    ])
}

#[path = "tiger_exterior_finishes.rs"]
mod exterior_finishes;

/// Exact authored running-belt steel nodes, shared with the reviewed finishes. The ten
/// Turret_* spare shoes are stored armor, not moving belts, and are excluded by source identity.
pub(crate) fn tiger_track_source_nodes() -> &'static [&'static str] {
    static NODES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NODES.get_or_init(|| exterior_finishes::TRACK_STEEL.iter().copied()
        .filter(|name|name.starts_with("Hull_")).collect()).as_slice()
}

/// Reviewed exterior armor/deck/barrel nodes from the current source Tiger. Material.001 is also
/// shared by tracks, engine internals and breech steel, so it must never be repainted globally.
/// Cabin surfaces have separate material identities and their own user-selected gunmetal finish.
const TIGER_PAINTED_NODES: &[&str] = &[
    "Hull_0619", "Hull_0708", "Hull_1174", "Hull_0706", "Hull_0707", "Hull_0076",
    "Hull_0171", "Hull_0696", "Hull_0697", "Hull_0975", "Drivers Hatch Pivot",
    "Turret_0101", "Turret_0105", "Turret_0159", "Turret_0115", "Turret_0116",
    "Turret_0239", "Turret_0240", "Turret_0241", "escape hatch", "gunner_hatch",
    "Gun_0001", "Gun_0007", "Gun_0008", "cupola hatch disc.001",
];

pub(crate) fn tiger_paint_finish(node: &str, material: Option<&str>) -> Option<crate::gltf_load::MaterialOverride> {
    // Alex's 2026-09-09 choice: muted gunmetal cabin instead of historical ivory paint.
    // Named painted surfaces are dielectric pigment; bare machinery retains its source steel.
    // Select named surface materials, never the shared atlas or bright instrument faces.
    let linear = |v: f32| ((v / 255.0 + 0.055) / 1.055).powf(2.4);
    if matches!(material, Some("Interior Paint" | "Turret Floor")) {
        let (swatch, rgb, metallic, roughness) = if material == Some("Turret Floor") {
            (2, [58.0, 70.0, 64.0], 0.0, 0.84)
        } else {
            (1, [104.0, 110.0, 118.0], 0.0, 0.78)
        };
        return Some(crate::gltf_load::MaterialOverride {
            swatch,
            base: [linear(rgb[0]), linear(rgb[1]), linear(rgb[2]), 1.0],
            metallic,
            roughness,
        });
    }
    if material != Some("Material.001") { return None; }
    if exterior_finishes::TRACK_STEEL.contains(&node) || exterior_finishes::CABLE_EXHAUST_STEEL.contains(&node) {
        let track=exterior_finishes::TRACK_STEEL.contains(&node);
        let rgb=if track {[58.0,58.0,56.0]} else {[73.0,72.0,68.0]};
        return Some(crate::gltf_load::MaterialOverride {
            swatch: if track {3} else {4},
            base:[linear(rgb[0]),linear(rgb[1]),linear(rgb[2]),1.0],
            metallic:if track {0.48} else {0.60}, roughness:if track {0.90} else {0.82},
        });
    }
    if !TIGER_PAINTED_NODES.contains(&node) && !exterior_finishes::PAINTED.contains(&node) { return None; }
    // Charcoal German-grey paint, stored as linear reflectance for the glTF material pipeline.
    // Dielectric pigment remains distinct from bare track/breech steel; no lighting gain is added.
    Some(crate::gltf_load::MaterialOverride {
        swatch: 0,
        base: [linear(72.0), linear(74.0), linear(78.0), 1.0],
        metallic: 0.0,
        roughness: 0.75,
    })
}

pub(crate) fn load_painted_tiger(path: &str) -> anyhow::Result<CpuMesh> {
    if path=="assets/tanks/tiger/baked/tiger_hull.glb" {
        Ok(crate::gltf_load::load_glb_partitioned_roots_with_source_addition(path,&[],
            Some(tiger_paint_finish),Some((crate::interior_rig::RACK_FITTING_GLB,"Turret_0125")))?.remove(0))
    } else {crate::gltf_load::load_glb_with_material_override(path,tiger_paint_finish)}
}

/// Compatibility hook for the existing source-steel/optic callers. The former whole-atlas tint
/// darkened dials and unrelated materials. Finishes now happen by explicit source-node/material
/// selection during Tiger import; these already classified meshes stay unchanged.
pub fn repaint_german_grey(m: CpuMesh) -> CpuMesh { m }

/// The Schutzglas vision-block meshes (hull visor / cupola blocks / TZF), baked as separate
/// parts so the renderer can draw them SEE-THROUGH (GLASS mode, params.w = 7). Missing bake →
/// no glass parts (the authored ports are simply open). Each shares its parent part's origin.
pub fn load_tiger_glass() -> [Option<CpuMesh>; 3] {
    // The PORT set: direct-view slit glass (GLASS RIGHT PORT / Left Port Glass) — see-through.
    let load = |n: &str| {
        crate::gltf_load::load_glb(&format!("assets/tanks/tiger/baked/tiger_port_{n}.glb")).ok()
    };
    [load("hull"), load("turret"), load("gun")]
}

/// The RTT screen meshes (mirror periscopes + the TZF lens): faces UV-mapped to atlas tiles.
pub fn load_tiger_rtt() -> [Option<CpuMesh>; 3] {
    let load = |n: &str| {
        crate::gltf_load::load_glb(&format!("assets/tanks/tiger/baked/tiger_rtt_{n}.glb")).ok()
    };
    [load("hull"), load("turret"), load("gun")]
}

pub fn build_tiger() -> [CpuMesh; 3] {
    (|| Some([
        load_painted_tiger("assets/tanks/tiger/baked/tiger_hull.glb").ok()?,
        load_painted_tiger("assets/tanks/tiger/baked/tiger_turret.glb").ok()?,
        load_painted_tiger("assets/tanks/tiger/baked/tiger_gun.glb").ok()?,
    ]))()
    .unwrap_or_else(|| {
        eprintln!("[panzerrechnersystem] tiger GLBs missing — using box placeholder");
        let body = [120, 122, 104];
        let dark = [74, 76, 66];
        [
            box_mesh(
                Vec3::new(-1.85, 0.35, -3.15),
                Vec3::new(1.85, 1.7, 3.15),
                body,
            ),
            box_mesh(Vec3::new(-1.5, 0.0, -1.8), Vec3::new(1.5, 0.85, 1.6), body),
            box_mesh(
                Vec3::new(-0.13, -0.13, 0.0),
                Vec3::new(0.13, 0.13, 4.6),
                dark,
            ),
        ]
    })
}

pub fn build_t34() -> [CpuMesh; 3] {
    load_parts(
        "assets/tanks/T34-76/baked/t34_hull.glb",
        "assets/tanks/T34-76/baked/t34_turret.glb",
        "assets/tanks/T34-76/baked/t34_gun.glb",
    )
    .unwrap_or_else(|| {
        eprintln!("[panzerrechnersystem] t34 GLBs missing — using box placeholder");
        let body = [84, 92, 78];
        let dark = [56, 62, 52];
        [
            box_mesh(Vec3::new(-1.5, 0.3, -3.0), Vec3::new(1.5, 1.35, 3.0), body),
            box_mesh(Vec3::new(-1.05, 0.0, -1.1), Vec3::new(1.05, 0.6, 1.2), body),
            box_mesh(Vec3::new(-0.1, -0.1, 0.0), Vec3::new(0.1, 0.1, 3.2), dark),
        ]
    })
}

/// Load the SU-100's two baked GLBs: the hull (hull+tracks+casemate+cupola, one rigid mesh, origin at
/// ground contact) and the 100 mm gun (origin at the trunnion, barrel along +Z).
fn load_su100() -> Option<[CpuMesh; 2]> {
    Some([
        crate::gltf_load::load_glb("assets/tanks/su100/baked/su100_hull.glb").ok()?,
        crate::gltf_load::load_glb("assets/tanks/su100/baked/su100_gun.glb").ok()?,
    ])
}

/// SU-100 tank destroyer: a CASEMATE rig, so there are only TWO real meshes (hull+casemate, and the
/// 100 mm gun). To keep the uniform `[hull, turret, gun]` triple the enemy render path indexes, the
/// middle "turret" slot is a tiny hidden placeholder — `game.rs` hides it for a casemate tank. Prefers
/// the baked GLBs; falls back to procedural boxes (a squat casemate hull + a long 100 mm barrel) if
/// the assets are missing.
pub fn build_su100() -> [CpuMesh; 3] {
    let dark = [48, 56, 46];
    // A degenerate near-zero box for the unused turret slot (always hidden by the casemate render path).
    let placeholder = || box_mesh(Vec3::splat(-0.02), Vec3::splat(0.02), dark);
    if let Some([hull, gun]) = load_su100() {
        return [hull, placeholder(), gun];
    }
    eprintln!("[panzerrechnersystem] su100 GLBs missing — using box placeholder");
    let body = [70, 82, 64]; // Soviet green
    [
        // Low hull with a taller sloped-forward casemate suggested by the raised body height.
        box_mesh(
            Vec3::new(-1.55, 0.30, -3.2),
            Vec3::new(1.55, 1.85, 3.2),
            body,
        ),
        placeholder(),
        // The long 100 mm D-10S barrel.
        box_mesh(
            Vec3::new(-0.11, -0.11, 0.0),
            Vec3::new(0.11, 0.11, 4.75),
            dark,
        ),
    ]
}

fn hidden_part() -> CpuMesh {
    box_mesh(Vec3::splat(-0.015), Vec3::splat(0.015), [42, 48, 38])
}

/// Join several same-material primitives into one rigid rig part. Heavy tanks currently have no
/// separately baked hull/turret/gun assets: the wreck-library GLBs are complete flattened vehicles,
/// so using one as a "hull" retains an immovable second turret and barrel. A deliberately coherent
/// low-poly rig is preferable until named-node bakes exist: every visible vertex then belongs to the
/// same three transforms that aiming, muzzle FX and ballistics use.
fn merge_meshes<const N: usize>(meshes: [CpuMesh; N]) -> CpuMesh {
    let mut meshes = meshes.into_iter();
    let mut out = meshes
        .next()
        .expect("merge_meshes requires at least one primitive");
    for mesh in meshes {
        let base = out.vertices.len() as u32;
        out.vertices.extend(mesh.vertices);
        out.indices
            .extend(mesh.indices.into_iter().map(|index| base + index));
        out.min = out.min.min(mesh.min);
        out.max = out.max.max(mesh.max);
    }
    out
}

/// KV-2: broad KV running gear, an unmistakably tall rotating box turret, and a separately elevating
/// 152 mm howitzer. No complete KV-1 donor is retained, so there is exactly one turret and one gun.
pub fn build_kv2() -> [CpuMesh; 3] {
    let body = [74, 86, 66];
    let turret = [70, 82, 62];
    let dark = [44, 50, 40];
    [
        merge_meshes([
            box_mesh(
                Vec3::new(-1.72, 0.28, -3.35),
                Vec3::new(1.72, 1.30, 3.35),
                body,
            ),
            box_mesh(
                Vec3::new(-1.50, 1.12, -2.72),
                Vec3::new(1.50, 1.62, 2.82),
                body,
            ),
            box_mesh(
                Vec3::new(-1.92, 0.18, -3.28),
                Vec3::new(-1.55, 1.08, 3.28),
                body,
            ),
            box_mesh(
                Vec3::new(1.55, 0.18, -3.28),
                Vec3::new(1.92, 1.08, 3.28),
                body,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-1.48, 0.0, -1.28),
                Vec3::new(1.48, 2.02, 1.42),
                turret,
            ),
            box_mesh(
                Vec3::new(-1.34, 2.02, -1.12),
                Vec3::new(1.34, 2.16, 1.20),
                turret,
            ),
            box_mesh(
                Vec3::new(0.52, 2.16, -0.56),
                Vec3::new(0.96, 2.48, -0.12),
                turret,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-0.56, -0.44, -0.30),
                Vec3::new(0.56, 0.44, 0.30),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.16, -0.16, 0.0),
                Vec3::new(0.16, 0.16, 3.12),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.25, -0.24, 3.04),
                Vec3::new(0.25, 0.24, KV2_PARTS.gun_len),
                dark,
            ),
        ]),
    ]
}

/// Broad, low assault-gun silhouette for the ML-20S. The geometry is code-native because no
/// ISU-152 asset ships in the library, but its dimensions and 152 mm barrel are distinct from SU-100.
pub fn build_isu152() -> [CpuMesh; 3] {
    [
        box_mesh(
            Vec3::new(-1.86, 0.26, -3.45),
            Vec3::new(1.86, 2.15, 3.45),
            [68, 78, 60],
        ),
        hidden_part(),
        box_mesh(
            Vec3::new(-0.18, -0.18, 0.0),
            Vec3::new(0.18, 0.18, 4.35),
            [42, 48, 38],
        ),
    ]
}

/// IS-2: stepped cast-turret silhouette and long D-25T, kept as three honest articulated parts. The
/// shipped IS-2 GLB has useful source hierarchy, but `load_glb` intentionally flattens it; treating
/// that complete vehicle as the hull made the visible gun ignore the AI's lay.
pub fn build_is2() -> [CpuMesh; 3] {
    let body = [66, 78, 60];
    let turret = [63, 75, 57];
    let dark = [40, 47, 37];
    [
        merge_meshes([
            box_mesh(
                Vec3::new(-1.64, 0.24, -3.45),
                Vec3::new(1.64, 1.24, 3.45),
                body,
            ),
            box_mesh(
                Vec3::new(-1.47, 1.12, -2.56),
                Vec3::new(1.47, 1.72, 2.86),
                body,
            ),
            box_mesh(
                Vec3::new(-1.87, 0.18, -3.36),
                Vec3::new(-1.55, 1.03, 3.36),
                body,
            ),
            box_mesh(
                Vec3::new(1.55, 0.18, -3.36),
                Vec3::new(1.87, 1.03, 3.36),
                body,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-1.30, 0.0, -1.32),
                Vec3::new(1.30, 0.66, 1.28),
                turret,
            ),
            box_mesh(
                Vec3::new(-1.08, 0.66, -1.10),
                Vec3::new(1.08, 1.02, 1.02),
                turret,
            ),
            box_mesh(
                Vec3::new(-1.18, 0.18, -1.72),
                Vec3::new(1.18, 0.72, -1.14),
                turret,
            ),
            box_mesh(
                Vec3::new(0.46, 1.02, -0.48),
                Vec3::new(0.84, 1.28, -0.10),
                turret,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-0.52, -0.36, -0.28),
                Vec3::new(0.52, 0.36, 0.32),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.13, -0.13, 0.0),
                Vec3::new(0.13, 0.13, 4.48),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.23, -0.21, 4.40),
                Vec3::new(0.23, 0.21, IS2_PARTS.gun_len),
                dark,
            ),
        ]),
    ]
}

/// Late-war pike-nose threat: lower hull and flatter stepped turret are their own articulated rig,
/// rather than a complete IS-2 donor hidden beneath a second turret and gun.
pub fn build_is3() -> [CpuMesh; 3] {
    let body = [62, 74, 58];
    let turret = [59, 71, 55];
    let dark = [38, 45, 35];
    [
        merge_meshes([
            box_mesh(
                Vec3::new(-1.70, 0.22, -3.52),
                Vec3::new(1.70, 1.14, 3.52),
                body,
            ),
            box_mesh(
                Vec3::new(-1.44, 1.04, -2.48),
                Vec3::new(1.44, 1.58, 2.42),
                body,
            ),
            box_mesh(
                Vec3::new(-0.72, 1.14, 2.42),
                Vec3::new(0.72, 1.46, 3.34),
                body,
            ),
            box_mesh(
                Vec3::new(-1.92, 0.16, -3.42),
                Vec3::new(-1.58, 0.98, 3.42),
                body,
            ),
            box_mesh(
                Vec3::new(1.58, 0.16, -3.42),
                Vec3::new(1.92, 0.98, 3.42),
                body,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-1.42, 0.0, -1.40),
                Vec3::new(1.42, 0.42, 1.38),
                turret,
            ),
            box_mesh(
                Vec3::new(-1.12, 0.42, -1.12),
                Vec3::new(1.12, 0.72, 1.08),
                turret,
            ),
            box_mesh(
                Vec3::new(-1.24, 0.14, -1.72),
                Vec3::new(1.24, 0.52, -1.20),
                turret,
            ),
            box_mesh(
                Vec3::new(0.46, 0.72, -0.48),
                Vec3::new(0.80, 0.94, -0.14),
                turret,
            ),
        ]),
        merge_meshes([
            box_mesh(
                Vec3::new(-0.50, -0.32, -0.26),
                Vec3::new(0.50, 0.32, 0.32),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.13, -0.13, 0.0),
                Vec3::new(0.13, 0.13, 4.48),
                dark,
            ),
            box_mesh(
                Vec3::new(-0.23, -0.20, 4.40),
                Vec3::new(0.23, 0.20, IS3_PARTS.gun_len),
                dark,
            ),
        ]),
    ]
}

#[derive(Clone)]
pub struct Tank {
    pub pos: Vec3, // ground contact
    pub heading: f32,
    /// Nose-up pitch and left-up roll of the hull (radians) on its supports (`tank_support`: the
    /// player Tiger rests on terrain, building floors and rubble). AI/spawn poses use a bounded
    /// six-contact terrain fit; all attached geometry and gun lay inherit the resulting hull pose.
    pub pitch: f32,
    pub roll: f32,
    pub speed: f32,
    pub turret_yaw: f32, // rel hull
    pub gun_pitch: f32,
    pub reload: f32,
    /// Seconds since the most recent discharge while the visual recoil cycle is active. It is kept
    /// separate from `reload`: the latter is the authoritative return-to-battery firing lock.
    pub(crate) shot_age: f32,
    pub alive: bool,
    pub hp: f32,
    pub parts: Parts,
    /// Vehicle type. Governs the CASEMATE rig (SU-100 = no turret) plus the kind-aware penetration +
    /// overwatch AI in `game.rs`. Defaults to `T34`; the enemy spawn overrides it per swarm member,
    /// and the player Tiger keeps the default (never read for the player).
    pub kind: EnemyKind,
    /// Whether this tank may drive over the rim of an open cellar's pit and fall in: true for the
    /// player's Tiger (the crew chooses where to drive), false for AI tanks, which have no
    /// navigation and would otherwise drop into every pit their route crosses. Neither climbs a
    /// face higher than `terrain::TANK_STEP_M` out of one.
    pub drops_into_pits: bool,
    // A tank is NOT an HP sponge — armor either holds (bounce, no effect) or is holed (a real
    // consequence). For the PLAYER Tiger the discrete penetration damage-state (individual crewmen +
    // modules, the spall model, fire) lives on `Game` (player-only, richer than the enemies need);
    // enemies just die on a penetration. `hp` is kept only as a derived 0..100 readout so headless
    // telemetry still reads sensibly; it is never chipped by a bounce.
}

pub const MAX_HP: f32 = 100.0;

impl Tank {
    pub fn new(x: f32, z: f32, heading: f32, hf: &Heightfield, parts: Parts) -> Self {
        let mut tank = Self {
            pos: Vec3::new(x, hf.height_at(x, z), z),
            heading,
            pitch: 0.0,
            roll: 0.0,
            speed: 0.0,
            turret_yaw: 0.0,
            gun_pitch: 0.0,
            reload: 0.0,
            shot_age: GUN_RECOIL_END,
            alive: true,
            hp: MAX_HP,
            parts,
            kind: EnemyKind::T34,
            drops_into_pits: false,
        };
        crate::vehicle_ground::settle(&mut tank, |x,z|hf.vehicle_height_at(x,z));
        tank
    }

    /// AI lay: slew the turret/gun toward a world target at the tank's own traverse/elevation
    /// rates (no instant snap). Returns true when the gun is laid within firing tolerance.
    pub fn ai_aim(&mut self, target: Vec3, dt: f32) -> bool {
        let hull = self.hull_model();
        // Azimuth follows the actual barrel line; elevation rotates about the trunnion, which
        // can be over a metre above the turret ring. Ring-based pitch aims a KV-2 above infantry.
        let pivot = self
            .turret_model()
            .transform_point3(self.parts.gun_trunnion);
        let delta = target - pivot;
        if !delta.is_finite() || delta.length_squared() < 1e-8 {
            return false;
        }
        let aim = delta.normalize();
        let local = hull.inverse().transform_vector3(aim);
        let want_yaw = local.x.atan2(local.z);

        let mut dyaw = want_yaw - self.turret_yaw;
        while dyaw > PI {
            dyaw -= TAU;
        }
        while dyaw < -PI {
            dyaw += TAU;
        }
        let step_y = AI_TRAVERSE * dt;
        self.turret_yaw += dyaw.clamp(-step_y, step_y);

        let target_local =
            self.turret_model().inverse().transform_point3(target) - self.parts.gun_trunnion;
        let want_pitch = target_local
            .y
            .atan2(target_local.x.hypot(target_local.z))
            .clamp(GUN_MIN, GUN_MAX);
        let dpitch = want_pitch - self.gun_pitch;
        self.gun_pitch = crate::anti_infantry_ai::slew_elevation(self.gun_pitch, target_local, dt);

        // Firing tolerance is loose enough that a moving, flanking tank actually gets shots off
        // (its accuracy is then spoiled by the speed-scaled dispersion at the fire site, not by an
        // over-strict "perfectly laid" gate). ~2.9 deg yaw / 2.3 deg pitch.
        dyaw.abs() < 0.05 && dpitch.abs() < 0.04
    }

    /// Casemate lay (SU-100): the gun has NO independent traverse, so AZIMUTH is the hull's job (the
    /// overwatch maneuver turns the hull onto the target). This only ELEVATES the gun toward `target`
    /// at the AI elevation rate, holding `turret_yaw` welded at 0, and reports laid = the hull is
    /// pointed at the target (within tolerance) AND the gun elevation has settled. Used for `EnemyKind::Su100`.
    pub fn casemate_aim(&mut self, target: Vec3, dt: f32) -> bool {
        self.turret_yaw = 0.0; // welded casemate — never traverses
        let hull = self.hull_model();
        // Elevation is measured about the trunnion; azimuth error is the hull's, closed by steering.
        let pivot = hull.transform_point3(self.parts.gun_trunnion);
        let aim = (target - pivot).normalize_or_zero();
        let local = hull.inverse().transform_vector3(aim);
        let yaw_err = local.x.atan2(local.z); // hull azimuth error to the target
        let want_pitch = local.y.clamp(-0.999, 0.999).asin().clamp(GUN_MIN, GUN_MAX);
        let dpitch = want_pitch - self.gun_pitch;
        let step_p = AI_ELEV_RATE * dt;
        self.gun_pitch = (self.gun_pitch + dpitch.clamp(-step_p, step_p)).clamp(GUN_MIN, GUN_MAX);
        yaw_err.abs() < 0.05 && dpitch.abs() < 0.04
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.heading.sin(), 0.0, self.heading.cos())
    }

    /// Cheap, kind-correct ground-plane hull geometry for vehicle contacts and avoidance. The
    /// footprint comes from `Parts`, so the player Tiger remains Tiger-sized even though its
    /// legacy `kind` field defaults to `EnemyKind::T34`.
    pub fn hull_footprint(&self) -> HullFootprint {
        HullFootprint {
            center: Vec2::new(self.pos.x, self.pos.z),
            half_extents: self.parts.hull_half_extents,
            heading: self.heading,
        }
    }

    /// Hull-right on the ground plane (used to spawn a dismounted soldier beside the tank).
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// World-space forward of the TURRET (hull heading + turret traverse). Used for the commander
    /// view so it sweeps with turret rotation, not just hull steering.
    pub fn turret_forward(&self) -> Vec3 {
        self.turret_model().transform_vector3(Vec3::Z).normalize()
    }

    /// drive/steer/traverse/elevate are command rates in [-1, 1].
    pub fn update(
        &mut self,
        drive: f32,
        steer: f32,
        traverse: f32,
        elevate: f32,
        dt: f32,
        hf: &Heightfield,
    ) {
        self.update_with_track_traction(drive, steer, traverse, elevate, dt, hf, TrackTraction::INTACT);
    }

    /// The damaged-player drive retains the ordinary terrain/contact path. With one broken belt,
    /// the intact belt can creep in either direction around the stationary opposite contact line.
    /// This is a bounded differential-drive approximation, not a track slip/tension simulation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_with_track_traction(
        &mut self,
        drive: f32,
        steer: f32,
        traverse: f32,
        elevate: f32,
        dt: f32,
        hf: &Heightfield,
        traction: TrackTraction,
    ) {
        let before = self.pos;
        match (traction.left, traction.right) {
            (true, true) => {
                // Preserve the established healthy drivetrain and every enemy's handling.
                let target = drive * TOP_SPEED;
                let d = (target - self.speed).clamp(-ACCEL * dt, ACCEL * dt);
                self.speed += d;
                self.heading += steer * TURN_RATE * dt;
                self.pos += self.forward() * self.speed * dt;
            }
            (false, false) => self.speed = 0.0,
            (left, _) => {
                // Faster right belt turns toward +heading; faster left toward -heading. A
                // half-rate yaw cap expresses the drag of the thrown track without giving the
                // surviving belt normal road speed around the measured track contact radius.
                let yaw_sign = if left { -1.0 } else { 1.0 };
                let command = (drive + yaw_sign * steer).clamp(-1.0, 1.0);
                let limit = TURN_RATE * 0.5 * TIGER_TRACK_HALF_SPACING;
                self.speed = self.speed.clamp(-limit, limit);
                let target = command * limit;
                let accel = ACCEL * 0.5;
                let duration = dt.max(0.0);
                let ramp = ((target - self.speed).abs() / accel).min(duration);
                let after_ramp = self.speed + (target - self.speed).signum() * accel * ramp;
                // Integrate the acceleration ramp, including a target reached within this frame.
                // Splitting a frame must not change the position of the stationary track.
                let distance = (self.speed + after_ramp) * 0.5 * ramp
                    + after_ramp * (duration - ramp);
                self.speed = after_ramp;
                let broken_side = Vec3::new(yaw_sign * TIGER_TRACK_HALF_SPACING, 0.0, 0.0);
                let anchor = self.pos + Mat4::from_rotation_y(self.heading).transform_vector3(broken_side);
                self.heading += yaw_sign * distance / TIGER_TRACK_HALF_SPACING;
                self.pos = anchor - Mat4::from_rotation_y(self.heading).transform_vector3(broken_side);
            }
        }
        self.finish_update(before, traverse, elevate, dt, hf);
    }

    /// Player-only stateful gearbox/engine path. Every hull-attached visual and ballistic source
    /// uses this same physical chassis pose; ordinary AI updates keep their existing controller.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_player_powertrain(&mut self, state: &mut crate::powertrain::Powertrain,
        controls: crate::powertrain::Controls, traverse:f32, elevate:f32, dt:f32, hf:&Heightfield) {
        let before=self.pos;
        let motion=state.advance(dt,self.speed,self.pitch,controls);
        self.speed=motion.speed;
        if controls.left_track != controls.right_track {
            let side=if controls.left_track {-1.0}else{1.0};
            let broken=Vec3::X*side*TIGER_TRACK_HALF_SPACING;
            let anchor=self.pos+Mat4::from_rotation_y(self.heading).transform_vector3(broken);
            self.heading+=motion.yaw;
            self.pos=anchor-Mat4::from_rotation_y(self.heading).transform_vector3(broken);
        } else {
            let midpoint=self.heading+motion.yaw*0.5;
            self.pos+=Vec3::new(midpoint.sin(),0.0,midpoint.cos())*motion.distance;
            self.heading+=motion.yaw;
        }
        self.finish_update(before,traverse,elevate,dt,hf);
    }

    fn finish_update(&mut self, before:Vec3, traverse:f32, elevate:f32, dt:f32, hf:&Heightfield) {
        // An open cellar's excavation face is a vertical step: no tank climbs more than its rated
        // obstacle out of the pit, and only a tank that may fall in drives over its rim.
        // Its tracks bridge a trench narrower than its crossing span.
        let gated = crate::terrain_opening::gate_edges_bridging(
            &hf.openings,
            |x, z| hf.vehicle_height_at(x, z),
            glam::Vec2::new(before.x, before.z),
            glam::Vec2::new(self.pos.x, self.pos.z),
            before.y,
            crate::terrain::TANK_STEP_M,
            (!self.drops_into_pits).then_some(crate::terrain::TANK_STEP_M),
            crate::terrain::TANK_BRIDGE_SPAN_M,
        );
        if gated.x != self.pos.x || gated.y != self.pos.z {
            self.speed = 0.0;
        }
        self.pos.x = gated.x;
        self.pos.z = gated.y;
        crate::vehicle_ground::settle(self, |x,z|hf.vehicle_height_at(x,z));

        self.turret_yaw += traverse * TRAVERSE * dt;
        self.gun_pitch = (self.gun_pitch + elevate * ELEV_RATE * dt).clamp(GUN_MIN, GUN_MAX);
        if self.reload > 0.0 {
            self.reload -= dt;
        }
        if self.shot_age < GUN_RECOIL_END {
            self.shot_age = (self.shot_age + dt.max(0.0)).min(GUN_RECOIL_END);
        }
    }

    /// Travel-locked: the Tiger cannot fire on the move (no stabiliser).
    pub fn can_fire(&self) -> bool {
        self.alive && self.reload <= 0.0 && self.speed.abs() < 0.6
    }

    pub fn fired(&mut self) {
        self.reload = RETURN_TO_BATTERY;
        self.present_fired();
    }

    /// Arm only the presentation recoil for a discharge whose gameplay state is authoritative
    /// elsewhere (LAN snapshots and AI guns with their own reload cadence). This deliberately does
    /// not touch reload, ammunition, ballistics, or damage.
    pub fn present_fired(&mut self) {
        self.shot_age = 0.0;
    }

    /// Elapsed visual-shot time for camera concussion and other presentation consumers. Once the
    /// bounded recoil cycle has settled, no stale shot state leaks into later frames.
    pub fn shot_age(&self) -> Option<f32> {
        (self.shot_age < GUN_RECOIL_END).then_some(self.shot_age)
    }

    pub fn hull_model(&self) -> Mat4 {
        let level = Mat4::from_translation(self.pos) * Mat4::from_rotation_y(self.heading);
        if self.pitch == 0.0 && self.roll == 0.0 {
            return level;
        }
        // Nose-up pitch turns +Z toward +Y (a rotation of -pitch about +X); left-up roll lifts +X.
        level * Mat4::from_rotation_x(-self.pitch) * Mat4::from_rotation_z(self.roll)
    }
    pub fn turret_model(&self) -> Mat4 {
        if self.kind.casemate() {
            // Casemate TD (SU-100): the fighting compartment is welded to the hull — there is no
            // turret ring. The "turret" transform IS the hull, so `gun_model` (turret · trunnion ·
            // pitch) elevates the gun straight off the hull and all azimuth comes from turning the
            // hull. `turret_forward` (used by the stealth sightline) then reads the hull front.
            self.hull_model()
        } else {
            self.hull_model()
                * Mat4::from_translation(self.parts.turret_pivot)
                * Mat4::from_rotation_y(self.turret_yaw)
        }
    }
    pub fn gun_model(&self) -> Mat4 {
        self.turret_model()
            * Mat4::from_translation(self.parts.gun_trunnion)
            * Mat4::from_rotation_x(-self.gun_pitch)
    }

    /// Render transform for the recoiling gun group. The ballistic muzzle deliberately continues
    /// to use `gun_model()`, so a fired shell never inherits a presentation-only barrel offset.
    pub fn visual_gun_model(&self) -> Mat4 {
        self.gun_model()
            * Mat4::from_translation(Vec3::new(0.0, 0.0, -gun_recoil_distance(self.shot_age)))
    }

    /// (muzzle world position, gun forward direction).
    pub fn muzzle(&self) -> (Vec3, Vec3) {
        let g = self.gun_model();
        (
            g.transform_point3(Vec3::new(0.0, 0.0, self.parts.gun_len)),
            g.transform_vector3(Vec3::Z).normalize(),
        )
    }

    /// Coaxial MG34: mounted COAXIALLY with the main gun (same axis as the barrel). Returns (port
    /// world position, fire direction). The direction is the GUN's own forward, so the coax fires
    /// exactly PARALLEL to the main gun — laying the gun on a target also lays the coax. This is the
    /// AI tanks' generic emitter, out along the barrel; the player's Tiger fires from its source
    /// mantlet port (`tiger_coax_muzzle`).
    pub fn coax_muzzle(&self) -> (Vec3, Vec3) {
        let g = self.gun_model();
        let pos = g.transform_point3(Vec3::new(-0.22, -0.16, self.parts.gun_len * 0.72));
        let dir = g.transform_vector3(Vec3::Z).normalize();
        (pos, dir)
    }

    /// The player's Tiger's coaxial MG34 at its real mantlet port (`TIGER_COAX_PORT`), firing along
    /// the gun. It was placed 72 % of the way out along the barrel, ahead of an older sight eye; the
    /// sight's objective now sits at the mantlet, and that emitter put every MG flash in the TZF's
    /// field next to the 88's muzzle (2026-10-28).
    pub fn tiger_coax_muzzle(&self) -> (Vec3, Vec3) {
        let g = self.gun_model();
        (g.transform_point3(TIGER_COAX_PORT), g.transform_vector3(Vec3::Z).normalize())
    }

    /// Hull / bow MG34: the radio-operator's forward machine gun in the FRONT HULL, right side.
    /// Returns (port world position, fire direction). Fires forward along the HULL heading —
    /// independent of turret lay — so it only bears on what the hull faces.
    pub fn hull_mg_muzzle(&self) -> (Vec3, Vec3) {
        let h = self.hull_model();
        let pos = h.transform_point3(Vec3::new(-0.72, 1.15, 2.95));
        let dir = h.transform_vector3(Vec3::Z).normalize();
        (pos, dir)
    }

    /// Player Tiger's exterior sight objective. The crew's physical eye remains inside the cabin.
    pub fn gunsight(&self) -> Vec3 {
        self.gun_model().transform_point3(TIGER_SIGHT_OBJECTIVE)
    }

    /// Commander's eye at the cupola, mounted on the TURRET roof — rotates with both hull steering
    /// and turret traverse (a real Tiger cupola turns with the turret ring it sits on).
    pub fn commander_eye(&self) -> Vec3 {
        self.turret_model()
            .transform_point3(self.parts.commander_eye)
    }

    /// Centre of mass, for hit tests.
    pub fn center(&self) -> Vec3 {
        if self.pitch == 0.0 && self.roll == 0.0 {
            return self.pos + Vec3::new(0.0, self.parts.center_h, 0.0);
        }
        self.hull_model()
            .transform_point3(Vec3::new(0.0, self.parts.center_h, 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiger_finishes_keep_exterior_cabin_and_instruments_distinct() {
        for node in ["Hull_0619", "Turret_0101", "Turret_0105", "Gun_0001", "Gun_0008"] {
            let paint = tiger_paint_finish(node, Some("Material.001")).unwrap();
            assert_eq!((paint.metallic, paint.roughness), (0.0, 0.75));
            let cabin = tiger_paint_finish(node, Some("Interior Paint")).unwrap();
            assert_ne!(paint.swatch, cabin.swatch);
            assert_eq!(cabin.metallic, 0.0, "pigmented cabin paint is not bare metal");
            assert!(cabin.base != paint.base);
        }
        for node in ["Breech", "Gun_0035", "TURRET FLOOR", "Turret_0088", "unknown replacement panel"] {
            assert!(tiger_paint_finish(node, Some("Material.001")).is_none());
            for material in ["Illuminated Dial", "dial red", "seats_black", "brass", "glass"] {
                assert!(tiger_paint_finish(node, Some(material)).is_none());
            }
        }
        let floor = tiger_paint_finish("TURRET FLOOR", Some("Turret Floor")).unwrap();
        assert_eq!(floor.swatch, 2);
    }

    #[test]
    fn exterior_hardware_and_running_gear_have_distinct_reviewed_finishes() {
        for node in ["Hull_0164", "Hull_0323", "Hull_0861", "Hull_1229", "Turret_0078", "cupola hatch handle.001"] {
            let m=tiger_paint_finish(node,Some("Material.001")).unwrap();
            assert_eq!(m.swatch,0,"painted exterior {node}");assert_eq!(m.metallic,0.0);
        }
        for node in ["Hull_0003", "Hull_0007", "Turret_0096"] {
            let m=tiger_paint_finish(node,Some("Material.001")).unwrap();
            assert_eq!(m.swatch,3,"rough track steel {node}");assert!(m.roughness>=0.9);
        }
        for node in exterior_finishes::PAINTED.iter().chain(exterior_finishes::TRACK_STEEL) {
            for material in ["Illuminated Dial","glass","brass","seats_black"] {
                assert!(tiger_paint_finish(node,Some(material)).is_none());
            }
        }
    }

    #[test]
    fn actual_tiger_paint_changes_only_selected_source_primitives_and_preserves_geometry() {
        use crate::gltf_load::{CpuImage, load_glb};
        fn pixel(image: &CpuImage, uv: [f32; 2]) -> [u8; 4] {
            let x = ((uv[0] * image.width as f32).floor() as u32).min(image.width - 1);
            let y = ((uv[1] * image.height as f32).floor() as u32).min(image.height - 1);
            let at = ((y * image.width + x) * 4) as usize;
            image.rgba[at..at + 4].try_into().unwrap()
        }
        fn material(mesh: &CpuMesh, i: usize) -> ([u8; 4], f32, f32) {
            let uv = mesh.vertices[i].uv;
            let base = pixel(mesh.texture.as_ref().unwrap(), uv);
            let mr = mesh.metallic_roughness_texture.as_ref().map(|m| pixel(m, uv)).unwrap_or([255; 4]);
            (base, f32::from(mr[2]) / 255.0 * mesh.metallic_factor,
                f32::from(mr[1]) / 255.0 * mesh.roughness_factor)
        }
        fn collect(node: gltf::Node<'_>, out: &mut Vec<(usize, Option<crate::gltf_load::MaterialOverride>)>) {
            if let Some(mesh) = node.mesh() {
                for primitive in mesh.primitives() {
                    let count = primitive.get(&gltf::Semantic::Positions).unwrap().count();
                    let selected = tiger_paint_finish(node.name().unwrap_or(""), primitive.material().name());
                    out.push((count, selected));
                }
            }
            for child in node.children() { collect(child, out); }
        }
        for part in ["hull", "turret", "gun", "hatch"] {
            let path = format!("assets/tanks/tiger/baked/tiger_{part}.glb");
            let raw = load_glb(&path).expect("source Tiger mesh");
            let painted = crate::gltf_load::load_glb_with_material_override(&path,tiger_paint_finish)
                .expect("paint-only source Tiger mesh");
            assert_eq!(raw.indices, painted.indices);
            assert_eq!(raw.vertices.len(), painted.vertices.len());
            assert!(raw.vertices.iter().zip(&painted.vertices).all(|(a,b)|
                a.pos == b.pos && a.normal == b.normal && a.tangent == b.tangent));
            assert_eq!(raw.normal_texture.is_some(), painted.normal_texture.is_some());
            let bytes = crate::resource::read(&path).unwrap();
            let doc = gltf::Gltf::from_slice(&bytes).unwrap();
            let mut sources = Vec::new();
            for node in doc.default_scene().or_else(|| doc.scenes().next()).unwrap().nodes() {
                collect(node, &mut sources);
            }
            let (mut first, mut changed, mut retained) = (0, 0, 0);
            for (count, selected) in sources {
                let actual = material(&painted, first);
                if let Some(finish) = selected {
                    let expected = match finish.swatch {
                        0 => [72, 74, 78, 255],
                        1 => [104, 110, 118, 255],
                        2 => [58, 70, 64, 255],
                        3 => [58, 58, 56, 255],
                        4 => [73, 72, 68, 255],
                        _ => panic!("unreviewed Tiger finish"),
                    };
                    assert_eq!(actual.0, expected, "{part}: selected finish must keep its own swatch");
                    assert!((actual.1 - finish.metallic).abs() <= 0.5/255.0 + 1e-6);
                    assert!((actual.2 - finish.roughness).abs() <= 0.5/255.0 + 1e-6);
                    changed += 1;
                } else {
                    let original = material(&raw, first);
                    assert_eq!(actual.0, original.0, "{part}: unrelated source color changed");
                    assert!((actual.1 - original.1).abs() <= 0.5/255.0 + 1e-6);
                    assert!((actual.2 - original.2).abs() <= 0.5/255.0 + 1e-6);
                    retained += 1;
                }
                first += count;
            }
            assert_eq!(first, painted.vertices.len());
            assert!(changed > 0, "{part}: selected source finish missing");
            if part=="hatch" { assert_eq!(retained,0,"entire exterior cupola hatch assembly is painted"); }
            else { assert!(retained>0,"{part}: unrelated interior/material identities must remain"); }
        }
    }

    use crate::terrain::Heightfield;

    #[test]
    fn ai_trunnion_lay_reaches_real_barrel_axis_for_rigs_and_elevation_differences() {
        for (kind, parts) in [
            (EnemyKind::T34, T34_PARTS),
            (EnemyKind::Kv2, KV2_PARTS),
            (EnemyKind::Is2, IS2_PARTS),
            (EnemyKind::Is3, IS3_PARTS),
            (EnemyKind::T34, TIGER_PARTS),
        ] {
            for ground_y in [-12.0, 0.0, 123.0] {
                for target_ground_delta in [-3.0, 0.0, 3.0] {
                    let mut tank = Tank::new(17.0, -31.0, 1.1, &flat(), parts);
                    tank.kind = kind;
                    tank.pos.y = ground_y;
                    tank.turret_yaw = -0.7;
                    let bearing = 1.1_f32 + 0.65;
                    let target = tank.pos
                        + Vec3::new(
                            bearing.sin() * 60.0,
                            target_ground_delta + 0.9,
                            bearing.cos() * 60.0,
                        );
                    for _ in 0..180 {
                        tank.ai_aim(target, 1.0 / 60.0);
                    }
                    let (muzzle, direction) = tank.muzzle();
                    assert!(
                        direction.dot((target - muzzle).normalize()) > 0.99999,
                        "{kind:?} ground={ground_y} target delta={target_ground_delta}"
                    );
                }
            }
        }
        // Terrain currently translates an upright hull; these tests do not claim hull pitch/roll.
    }

    #[test]
    fn ai_trunnion_elevation_and_shared_training_rig_preserve_rate_and_muzzle() {
        for (kind, parts, rig) in [
            (EnemyKind::T34, T34_PARTS, crate::anti_infantry_ai::F34_RIG),
            (EnemyKind::Kv2, KV2_PARTS, crate::anti_infantry_ai::M10T_RIG),
        ] {
            let mut tank = Tank::new(5.0, 9.0, 0.4, &flat(), parts);
            tank.kind = kind;
            tank.turret_yaw = 0.3;
            tank.gun_pitch = 0.15;
            let proxy = rig.gun(tank.pos, tank.heading, tank.turret_yaw, tank.gun_pitch);
            assert!(
                (proxy.transform_point3(Vec3::Z * rig.barrel_length) - tank.muzzle().0).length()
                    < 1e-5
            );
            let old = tank.gun_pitch;
            tank.ai_aim(tank.pos + Vec3::new(0.0, -10.0, 60.0), 0.05);
            assert!((tank.gun_pitch - old).abs() <= AI_ELEV_RATE * 0.05 + 1e-6);
        }
    }

    #[test]
    fn track_traction_single_side_retains_power_but_engine_and_operator_gates_still_apply() {
        for tracks in [TrackTraction::INTACT, TrackTraction { left: true, right: false },
            TrackTraction { left: false, right: true }] {
            assert_eq!(tracks.gate_commands(true, true, 0.8, -0.5), (0.8, -0.5));
            assert_eq!(tracks.gate_commands(false, true, 0.8, -0.5), (0.0, 0.0));
            assert_eq!(tracks.gate_commands(true, false, 0.8, -0.5), (0.0, 0.0));
        }
        assert_eq!(TrackTraction { left: false, right: false }
            .gate_commands(true, true, 1.0, 1.0), (0.0, 0.0));
    }

    #[test]
    fn track_traction_healthy_motion_keeps_existing_rates() {
        let hf = flat();
        let mut tank = Tank::new(0.0, 0.0, 0.4, &hf, TIGER_PARTS);
        tank.speed = 2.0;
        tank.update_with_track_traction(0.8, -0.3, 0.5, -0.25, 0.2, &hf, TrackTraction::INTACT);
        approx(tank.speed, 3.2, 1e-6);
        approx(tank.heading, 0.4 - 0.3 * TURN_RATE * 0.2, 1e-6);
        assert!((tank.pos - tank.forward() * 3.2 * 0.2).length() < 1e-6);
        approx(tank.turret_yaw, 0.5 * TRAVERSE * 0.2, 1e-6);
        approx(tank.gun_pitch, -0.25 * ELEV_RATE * 0.2, 1e-6);
    }

    #[test]
    fn track_traction_broken_side_stays_stationary_and_opposite_sides_mirror() {
        let hf = flat();
        let mut states = Vec::new();
        for left in [false, true] {
            let traction = TrackTraction { left, right: !left };
            let sign = if left { -1.0 } else { 1.0 };
            let mut tank = Tank::new(0.0, 0.0, 0.0, &hf, TIGER_PARTS);
            let contact = Vec3::X * sign * TIGER_TRACK_HALF_SPACING;
            for _ in 0..240 {
                tank.update_with_track_traction(1.0, 0.0, 0.0, 0.0, 1.0 / 120.0, &hf, traction);
                let anchor = tank.pos + Mat4::from_rotation_y(tank.heading).transform_vector3(contact);
                assert!((anchor - contact).length() < 2e-5);
                assert!(tank.speed <= TURN_RATE * 0.5 * TIGER_TRACK_HALF_SPACING + 1e-6);
            }
            assert!(tank.pos.z > 0.5 && tank.heading.abs() > 0.3);
            states.push(tank);
        }
        approx(states[0].pos.x, -states[1].pos.x, 2e-5);
        approx(states[0].pos.z, states[1].pos.z, 2e-5);
        approx(states[0].heading, -states[1].heading, 2e-5);
    }

    #[test]
    fn track_traction_pivot_has_consistent_time_integration_and_can_reverse() {
        let hf = flat();
        let traction = TrackTraction { left: false, right: true };
        let simulate = |dt: f32, steps: usize| {
            let mut tank = Tank::new(0.0, 0.0, 0.0, &hf, TIGER_PARTS);
            for command in [1.0, -1.0, 0.0] {
                for _ in 0..steps {
                    tank.update_with_track_traction(command, 0.0, 0.0, 0.0, dt, &hf, traction);
                }
                if command == -1.0 { assert!(tank.speed < -0.5); }
            }
            assert!(tank.speed.abs() < 1e-6);
            tank
        };
        let coarse = simulate(0.25, 4);
        let fine = simulate(1.0 / 120.0, 120);
        assert!((coarse.pos - fine.pos).length() < 3e-5);
        approx(coarse.heading, fine.heading, 3e-5);
    }

    #[test]
    fn track_traction_both_broken_blocks_chassis_but_keeps_manual_gun_controls() {
        let hf = flat();
        let mut tank = Tank::new(0.0, 0.0, 0.4, &hf, TIGER_PARTS);
        tank.speed = 5.0;
        tank.update_with_track_traction(1.0, 1.0, 1.0, 1.0, 0.2, &hf,
            TrackTraction { left: false, right: false });
        assert_eq!(tank.speed, 0.0);
        assert_eq!(tank.pos, Vec3::ZERO);
        approx(tank.heading, 0.4, 1e-6);
        assert!(tank.turret_yaw > 0.0 && tank.gun_pitch > 0.0);
        // Repair restores the actual drive path, without changing gun or engine state here.
        tank.update_with_track_traction(1.0, 0.0, 0.0, 0.0, 0.2, &hf, TrackTraction::INTACT);
        assert!(tank.speed > 1.0 && tank.pos.length() > 0.2);
    }

    /// A dead-flat heightfield at y=0, so a tank grounds at its spawn (no assets needed).
    fn flat() -> Heightfield {
        Heightfield {
            res: 2,
            spacing: 1000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
            openings: Default::default(),
        }
    }

    fn su100(x: f32, z: f32, heading: f32) -> Tank {
        let mut t = Tank::new(x, z, heading, &flat(), SU100_PARTS);
        t.kind = EnemyKind::Su100;
        t
    }

    fn approx(a: f32, b: f32, epsilon: f32) {
        assert!(
            (a - b).abs() <= epsilon,
            "expected {a:.6} ~= {b:.6} (epsilon {epsilon:.6})"
        );
    }

    fn footprint(x: f32, z: f32, heading: f32, half_x: f32, half_z: f32) -> HullFootprint {
        HullFootprint {
            center: Vec2::new(x, z),
            half_extents: Vec2::new(half_x, half_z),
            heading,
        }
    }

    #[test]
    fn yaw_obb_separated_hulls_have_no_contact() {
        let a = footprint(0.0, 0.0, 0.0, 2.0, 3.0);
        let b = footprint(4.01, 0.0, 0.0, 2.0, 3.0);
        assert_eq!(yaw_obb_contact(a, b), None);
    }

    #[test]
    fn yaw_obb_side_contact_returns_shallowest_translation() {
        let a = footprint(0.0, 0.0, 0.0, 2.0, 3.0);
        let b = footprint(3.5, 0.0, 0.0, 2.0, 3.0);
        let contact = yaw_obb_contact(a, b).expect("side-overlapping hulls must contact");
        approx(contact.normal.x, 1.0, 1e-6);
        approx(contact.normal.y, 0.0, 1e-6);
        approx(contact.depth, 0.5, 1e-6);
        assert!(
            yaw_obb_contact(
                a,
                HullFootprint {
                    center: b.center + contact.minimum_translation() + Vec2::X * 1e-4,
                    ..b
                }
            )
            .is_none(),
            "moving B by the contact MTV must separate the side overlap"
        );
    }

    #[test]
    fn yaw_obb_head_on_contact_uses_longitudinal_axis() {
        let a = footprint(0.0, 0.0, 0.0, 2.0, 3.0);
        let b = footprint(0.0, 5.25, 0.0, 2.0, 3.0);
        let contact = yaw_obb_contact(a, b).expect("head-on hulls must contact");
        approx(contact.normal.x, 0.0, 1e-6);
        approx(contact.normal.y, 1.0, 1e-6);
        approx(contact.depth, 0.75, 1e-6);
    }

    #[test]
    fn yaw_obb_45_degree_contact_has_stable_mtv() {
        let a = footprint(0.0, 0.0, 0.0, 1.0, 2.0);
        let b = footprint(2.0, 0.0, std::f32::consts::FRAC_PI_4, 1.0, 2.0);
        let contact = yaw_obb_contact(a, b).expect("rotated hulls must contact");
        let expected_depth = 1.0 + 3.0 * std::f32::consts::FRAC_1_SQRT_2 - 2.0;
        approx(contact.normal.x, 1.0, 1e-6);
        approx(contact.normal.y, 0.0, 1e-6);
        approx(contact.depth, expected_depth, 1e-6);
        assert!(
            yaw_obb_contact(
                a,
                HullFootprint {
                    center: b.center + contact.minimum_translation() + Vec2::X * 1e-4,
                    ..b
                }
            )
            .is_none(),
            "the rotated contact MTV must separate the pair"
        );
    }

    #[test]
    fn yaw_obb_coincident_hulls_return_finite_deterministic_contact() {
        let hull = footprint(4.0, -7.0, 0.37, 2.0, 3.0);
        let contact = yaw_obb_contact(hull, hull).expect("coincident hulls must contact");
        assert!(contact.normal.is_finite());
        assert!(contact.depth.is_finite());
        assert!(contact.minimum_translation().is_finite());
        approx(contact.normal.length(), 1.0, 1e-6);
        approx(contact.depth, 4.0, 1e-5);
    }

    #[test]
    fn tank_footprint_is_driven_by_parts_not_legacy_enemy_kind() {
        let tiger = Tank::new(12.0, -8.0, 0.4, &flat(), TIGER_PARTS);
        let t34 = Tank::new(12.0, -8.0, 0.4, &flat(), T34_PARTS);
        assert_eq!(tiger.kind, EnemyKind::T34, "documents the legacy default");
        assert_eq!(
            tiger.hull_footprint().half_extents,
            TIGER_PARTS.hull_half_extents
        );
        assert_eq!(
            t34.hull_footprint().half_extents,
            T34_PARTS.hull_half_extents
        );
        assert_ne!(
            tiger.hull_footprint().half_extents,
            t34.hull_footprint().half_extents
        );
    }

    #[test]
    fn visual_gun_recoil_slams_holds_returns_and_settles() {
        approx(gun_recoil_distance(0.0), 0.0, 1e-6);
        assert!(
            gun_recoil_distance(GUN_RECOIL_SLAM_END * 0.5) > GUN_RECOIL_TRAVEL * 0.80,
            "the pressure stroke should slam through most of its travel in the first half"
        );
        approx(
            gun_recoil_distance(GUN_RECOIL_SLAM_END),
            GUN_RECOIL_TRAVEL,
            1e-6,
        );
        approx(
            gun_recoil_distance(GUN_RECOIL_HOLD_END - 1e-5),
            GUN_RECOIL_TRAVEL,
            1e-6,
        );

        // The recuperator return is monotonic until it reaches battery at 0.62 s.
        let mut previous = GUN_RECOIL_TRAVEL;
        for i in 1..=64 {
            let t = GUN_RECOIL_HOLD_END
                + (GUN_RECOIL_RETURN_END - GUN_RECOIL_HOLD_END) * i as f32 / 64.0;
            let distance = gun_recoil_distance(t);
            assert!(distance <= previous + 1e-6);
            assert!(distance >= -1e-6);
            previous = distance;
        }
        approx(gun_recoil_distance(GUN_RECOIL_RETURN_END), 0.0, 1e-6);

        // The final recuperator wobble may cross the battery pose, but never by more than 1.5 cm
        // and is exactly settled at the bounded end of the cycle.
        for i in 0..=64 {
            let t =
                GUN_RECOIL_RETURN_END + (GUN_RECOIL_END - GUN_RECOIL_RETURN_END) * i as f32 / 64.0;
            assert!(gun_recoil_distance(t).abs() <= GUN_RECOIL_SETTLE + 1e-6);
        }
        approx(gun_recoil_distance(GUN_RECOIL_END), 0.0, 1e-6);
        approx(gun_recoil_distance(f32::INFINITY), 0.0, 1e-6);
    }

    #[test]
    fn presentation_only_discharge_never_mutates_reload_authority() {
        let mut tank = Tank::new(0.0, 0.0, 0.0, &flat(), TIGER_PARTS);
        tank.reload = 4.25;
        tank.present_fired();
        assert_eq!(tank.reload, 4.25);
        assert_eq!(tank.shot_age(), Some(0.0));
    }

    #[test]
    fn visual_recoil_never_moves_the_authoritative_muzzle() {
        let mut tank = Tank::new(12.0, -7.0, 0.43, &flat(), TIGER_PARTS);
        tank.turret_yaw = -0.31;
        tank.gun_pitch = 0.12;
        let ballistic_before = tank.muzzle();
        let sight_before = tank.gunsight();
        assert!(tank.shot_age().is_none());

        tank.fired();
        assert_eq!(tank.shot_age(), Some(0.0));
        tank.update(0.0, 0.0, 0.0, 0.0, GUN_RECOIL_SLAM_END, &flat());

        let laid = tank.gun_model();
        let visual = tank.visual_gun_model();
        let laid_origin = laid.transform_point3(Vec3::ZERO);
        let visual_origin = visual.transform_point3(Vec3::ZERO);
        let gun_forward = laid.transform_vector3(Vec3::Z).normalize();
        let visual_offset = visual_origin - laid_origin;
        approx(visual_offset.length(), GUN_RECOIL_TRAVEL, 1e-5);
        approx(visual_offset.dot(gun_forward), -GUN_RECOIL_TRAVEL, 1e-5);

        let ballistic_after = tank.muzzle();
        assert!(tank.gunsight().distance(sight_before)<1e-5,
            "the telescope objective follows elevation but must not recoil with the barrel");
        assert!(ballistic_after.0.distance(ballistic_before.0) < 1e-5);
        assert!(ballistic_after.1.distance(ballistic_before.1) < 1e-5);

        tank.update(0.0, 0.0, 0.0, 0.0, GUN_RECOIL_END, &flat());
        assert!(tank.shot_age().is_none());
        let settled = tank.visual_gun_model().transform_point3(Vec3::ZERO);
        assert!(settled.distance(tank.gun_model().transform_point3(Vec3::ZERO)) < 1e-6);
    }

    #[test]
    fn su100_is_a_turretless_casemate() {
        assert!(EnemyKind::Su100.casemate());
        assert!(!EnemyKind::T34.casemate());
        // A casemate has NO turret ring: the turret transform IS the hull, even with a nonzero
        // `turret_yaw` scribbled in — a welded compartment can't traverse.
        let mut t = su100(10.0, -20.0, 0.7);
        t.turret_yaw = 0.9; // would rotate a real turret; must be ignored by a casemate
        assert_eq!(
            t.turret_model(),
            t.hull_model(),
            "casemate turret must equal the hull transform"
        );
        // The gun still elevates: raising the pitch lifts the muzzle in Y off the trunnion.
        let low = {
            t.gun_pitch = 0.0;
            t.muzzle().0
        };
        let high = {
            t.gun_pitch = 0.20;
            t.muzzle().0
        };
        assert!(
            high.y > low.y + 0.3,
            "elevating the casemate gun must raise the muzzle"
        );

        // Contrast: a real turret (T-34) DOES rotate off the hull when the turret traverses.
        let mut r = Tank::new(0.0, 0.0, 0.0, &flat(), T34_PARTS); // kind defaults to T34
        r.turret_yaw = 0.9;
        assert_ne!(
            r.turret_model(),
            r.hull_model(),
            "a turreted tank must swing off the hull"
        );
    }

    #[test]
    fn casemate_aim_elevates_and_lays_when_the_hull_faces_the_target() {
        // Hull already pointed at a target dead ahead (+Z): azimuth is satisfied, so with the gun
        // elevation converged the shot lays; `turret_yaw` stays welded at 0 throughout.
        let mut t = su100(0.0, 0.0, 0.0);
        let target = t.center() + Vec3::new(0.0, 0.0, 400.0); // 400 m dead ahead, same height
        let mut laid = false;
        for _ in 0..600 {
            laid = t.casemate_aim(target, 1.0 / 60.0);
            if laid {
                break;
            }
        }
        assert!(
            laid,
            "with the hull on-target the casemate gun must eventually lay"
        );
        assert_eq!(
            t.turret_yaw, 0.0,
            "the casemate never traverses its (nonexistent) turret"
        );

        // Hull turned 90° AWAY: the azimuth error is huge, so the gun can NEVER report laid (the
        // maneuver, not this aim, must turn the hull) — proving azimuth is the hull's job.
        let mut off = su100(0.0, 0.0, std::f32::consts::FRAC_PI_2);
        let tgt = off.center() + Vec3::new(0.0, 0.0, 400.0);
        let mut ever = false;
        for _ in 0..600 {
            ever |= off.casemate_aim(tgt, 1.0 / 60.0);
        }
        assert!(
            !ever,
            "a casemate whose hull is 90° off the target can't lay by elevation alone"
        );
    }

    fn assert_articulated_heavy(kind: EnemyKind, parts: Parts, [hull, turret, gun]: [CpuMesh; 3]) {
        assert!(
            hull.max.y <= parts.turret_pivot.y + 0.001,
            "{} hull contains geometry above its turret ring; likely a flattened donor",
            kind.name()
        );
        assert!(
            turret.vertices.len() > 24,
            "{} needs visible articulated turret geometry",
            kind.name()
        );
        assert!(
            gun.vertices.len() > 24,
            "{} needs visible articulated gun geometry",
            kind.name()
        );
        assert!(
            (gun.max.z - parts.gun_len).abs() < 0.001,
            "{} visual barrel endpoint must coincide with its ballistic muzzle",
            kind.name()
        );

        let mut tank = Tank::new(14.0, -9.0, 0.35, &flat(), parts);
        tank.kind = kind;
        tank.turret_yaw = 0.72;
        tank.gun_pitch = 0.11;
        let visual_tip = tank
            .gun_model()
            .transform_point3(Vec3::new(0.0, 0.0, gun.max.z));
        assert!(
            visual_tip.distance(tank.muzzle().0) < 0.001,
            "{} visible gun must follow the aiming/muzzle transform",
            kind.name()
        );
    }

    #[test]
    fn heavy_visual_rigs_are_single_and_articulated() {
        assert_articulated_heavy(EnemyKind::Kv2, KV2_PARTS, build_kv2());
        assert_articulated_heavy(EnemyKind::Is2, IS2_PARTS, build_is2());
        assert_articulated_heavy(EnemyKind::Is3, IS3_PARTS, build_is3());
    }
}
