//! The tank: pose, turret/gun lay, drive over the heightfield, and the per-part transforms the
//! renderer needs. Geometry is procedural low-poly boxes for now (the real Tiger comes from the
//! Blender rebuild, Task 8). Tank-local frame: +Z forward, +Y up, +X right, origin at ground
//! contact. Used for both the player Tiger and the T-34 enemy.

use std::f32::consts::{PI, TAU};

use glam::{Mat4, Vec3};

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
const AI_ELEV_RATE: f32 = 0.40; // matching quicker AI elevation
const ELEV_RATE: f32 = 0.06981; // 4 deg/s
const GUN_MIN: f32 = -0.13963; // -8 deg
const GUN_MAX: f32 = 0.26180; // +15 deg
pub const TOP_SPEED: f32 = 12.0; // m/s
const ACCEL: f32 = 6.0;
const TURN_RATE: f32 = 0.7; // rad/s (neutral steer)
                            // The player's main gun is now MANUALLY reloaded (breech eject + ram, driven from the Loader
                            // station in game.rs), so `fired()` no longer starts a long auto-reload — it only locks the gun for
                            // the recoil run-out (return to battery). The travel-lock in `can_fire` still forbids firing on the
                            // move. Enemies set their own cadence directly (`ENEMY_RELOAD`), so this only affects the player.
const RETURN_TO_BATTERY: f32 = 1.0;

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
}

/// Tiger: exact values from the baked model (turret_pivot / gun_pivot / POV cameras, engine frame).
pub const TIGER_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 0.524, 0.019),
    gun_trunnion: Vec3::new(0.004, 1.845, 1.040), // gun_pivot − turret_pivot
    gun_len: 4.79,
    commander_eye: Vec3::new(0.560, 2.414, -0.485), // POV_CMD_CLOSED − turret_pivot
    center_h: 1.5,
};

/// T-34: estimated rig (enemy, only ever seen at fog range; refine against a screenshot).
/// Measured from the tools/bake_t34.py pivot report (new_tanks T-34/76 1942 bake, 2026-07-02):
/// hull origin at ground-contact footprint center; turret origin on the ring; gun at the trunnion.
pub const T34_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.97, 0.47),
    gun_trunnion: Vec3::new(0.0, 0.39, 0.75),
    gun_len: 3.10,
    commander_eye: Vec3::new(0.0, 0.62, -0.35), // turret-local: under the roof hatch, aft of the ring
    center_h: 1.4,
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
};

pub const KV2_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.62, -0.18),
    gun_trunnion: Vec3::new(0.0, 1.25, 1.22),
    gun_len: 3.35,
    commander_eye: Vec3::new(0.62, 1.62, -0.48),
    center_h: 1.65,
};

pub const ISU152_PARTS: Parts = Parts {
    turret_pivot: Vec3::ZERO,
    gun_trunnion: Vec3::new(0.18, 1.92, 2.25),
    gun_len: 4.35,
    commander_eye: Vec3::new(0.72, 2.30, -0.35),
    center_h: 1.45,
};

pub const IS2_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.72, -0.12),
    gun_trunnion: Vec3::new(0.0, 0.62, 0.92),
    gun_len: 4.75,
    commander_eye: Vec3::new(0.55, 0.92, -0.42),
    center_h: 1.45,
};

pub const IS3_PARTS: Parts = Parts {
    turret_pivot: Vec3::new(0.0, 1.58, -0.18),
    gun_trunnion: Vec3::new(0.0, 0.50, 1.02),
    gun_len: 4.75,
    commander_eye: Vec3::new(0.54, 0.72, -0.48),
    center_h: 1.35,
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

/// Three meshes: hull, turret (pivot-local), gun (trunnion-local). Prefers the baked low-poly GLBs;
/// falls back to procedural boxes if the assets are missing.
/// Repaint a bake's base-colour texture PANZER GREY (Dunkelgrau RAL 7021): the source Tiger model
/// is authored in Afrika-Korps sand — wrong for a Stalingrad winter. Luminance carries all the
/// baked detail (bolts, weld lines, grime); the hue is remapped onto the cool dark grey.
pub fn repaint_german_grey(mut m: CpuMesh) -> CpuMesh {
    if let Some(tex) = &mut m.texture {
        // Key on the texture's OWN mean luminance (the bake's albedo is authored much darker than
        // it reads under the snow-bounce lighting — a fixed key crushed the tank to charcoal):
        // the mean tone lands exactly ON the grey; baked detail (bolts, welds, grime) rides above
        // and below it.
        let mut mean = 0.0f32;
        for px in tex.rgba.chunks_exact(4) {
            mean += 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
        }
        mean = (mean / (tex.rgba.len() / 4) as f32).max(1.0);
        for px in tex.rgba.chunks_exact_mut(4) {
            let lum = 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
            let k = (lum / mean).clamp(0.05, 1.9);
            px[0] = (72.0 * k).min(255.0) as u8; // Dunkelgrau RAL 7021, slightly lifted for the crush
            px[1] = (74.0 * k).min(255.0) as u8;
            px[2] = (78.0 * k).min(255.0) as u8;
        }
    }
    m
}

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
    load_parts(
        "assets/tanks/tiger/baked/tiger_hull.glb",
        "assets/tanks/tiger/baked/tiger_turret.glb",
        "assets/tanks/tiger/baked/tiger_gun.glb",
    )
    .map(|parts| parts.map(repaint_german_grey))
    .unwrap_or_else(|| {
        eprintln!("[kessel] tiger GLBs missing — using box placeholder");
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
        eprintln!("[kessel] t34 GLBs missing — using box placeholder");
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
    eprintln!("[kessel] su100 GLBs missing — using box placeholder");
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
    pub speed: f32,
    pub turret_yaw: f32, // rel hull
    pub gun_pitch: f32,
    pub reload: f32,
    pub alive: bool,
    pub hp: f32,
    pub parts: Parts,
    /// Vehicle type. Governs the CASEMATE rig (SU-100 = no turret) plus the kind-aware penetration +
    /// overwatch AI in `game.rs`. Defaults to `T34`; the enemy spawn overrides it per swarm member,
    /// and the player Tiger keeps the default (never read for the player).
    pub kind: EnemyKind,
    // A tank is NOT an HP sponge — armor either holds (bounce, no effect) or is holed (a real
    // consequence). For the PLAYER Tiger the discrete penetration damage-state (individual crewmen +
    // modules, the spall model, fire) lives on `Game` (player-only, richer than the enemies need);
    // enemies just die on a penetration. `hp` is kept only as a derived 0..100 readout so headless
    // telemetry still reads sensibly; it is never chipped by a bounce.
}

pub const MAX_HP: f32 = 100.0;

impl Tank {
    pub fn new(x: f32, z: f32, heading: f32, hf: &Heightfield, parts: Parts) -> Self {
        Self {
            pos: Vec3::new(x, hf.height_at(x, z), z),
            heading,
            speed: 0.0,
            turret_yaw: 0.0,
            gun_pitch: 0.0,
            reload: 0.0,
            alive: true,
            hp: MAX_HP,
            parts,
            kind: EnemyKind::T34,
        }
    }

    /// AI lay: slew the turret/gun toward a world target at the tank's own traverse/elevation
    /// rates (no instant snap). Returns true when the gun is laid within firing tolerance.
    pub fn ai_aim(&mut self, target: Vec3, dt: f32) -> bool {
        let hull = self.hull_model();
        let pivot = hull.transform_point3(self.parts.turret_pivot);
        let aim = (target - pivot).normalize();
        let local = hull.inverse().transform_vector3(aim);
        let want_yaw = local.x.atan2(local.z);
        let want_pitch = local.y.clamp(-0.999, 0.999).asin().clamp(GUN_MIN, GUN_MAX);

        let mut dyaw = want_yaw - self.turret_yaw;
        while dyaw > PI {
            dyaw -= TAU;
        }
        while dyaw < -PI {
            dyaw += TAU;
        }
        let step_y = AI_TRAVERSE * dt;
        self.turret_yaw += dyaw.clamp(-step_y, step_y);

        let dpitch = want_pitch - self.gun_pitch;
        let step_p = AI_ELEV_RATE * dt;
        self.gun_pitch = (self.gun_pitch + dpitch.clamp(-step_p, step_p)).clamp(GUN_MIN, GUN_MAX);

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
        let target = drive * TOP_SPEED;
        let d = (target - self.speed).clamp(-ACCEL * dt, ACCEL * dt);
        self.speed += d;
        self.heading += steer * TURN_RATE * dt;
        self.pos += self.forward() * self.speed * dt;
        self.pos.y = hf.height_at(self.pos.x, self.pos.z);

        self.turret_yaw += traverse * TRAVERSE * dt;
        self.gun_pitch = (self.gun_pitch + elevate * ELEV_RATE * dt).clamp(GUN_MIN, GUN_MAX);
        if self.reload > 0.0 {
            self.reload -= dt;
        }
    }

    /// Travel-locked: the Tiger cannot fire on the move (no stabiliser).
    pub fn can_fire(&self) -> bool {
        self.alive && self.reload <= 0.0 && self.speed.abs() < 0.6
    }

    pub fn fired(&mut self) {
        self.reload = RETURN_TO_BATTERY;
    }

    pub fn hull_model(&self) -> Mat4 {
        Mat4::from_translation(self.pos) * Mat4::from_rotation_y(self.heading)
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
    /// exactly PARALLEL to the main gun — laying the gun on a target also lays the coax. The FX port
    /// is placed on the barrel line but FORWARD of the gunner's sight eye (`gunsight`, at ~0.5·gun_len)
    /// and just right/below it, so in the optic the tracers stream AWAY toward the reticle instead of
    /// blooming across the lens near-plane. Purely an emitter offset — the fire axis is the gun axis.
    pub fn coax_muzzle(&self) -> (Vec3, Vec3) {
        let g = self.gun_model();
        let pos = g.transform_point3(Vec3::new(0.22, -0.16, self.parts.gun_len * 0.72));
        let dir = g.transform_vector3(Vec3::Z).normalize();
        (pos, dir)
    }

    /// Hull / bow MG34: the radio-operator's forward machine gun in the FRONT HULL, right side.
    /// Returns (port world position, fire direction). Fires forward along the HULL heading —
    /// independent of turret lay — so it only bears on what the hull faces.
    pub fn hull_mg_muzzle(&self) -> (Vec3, Vec3) {
        let h = self.hull_model();
        let pos = h.transform_point3(Vec3::new(0.72, 1.15, 2.95));
        let dir = h.transform_vector3(Vec3::Z).normalize();
        (pos, dir)
    }

    /// Gunner's eye: mounted on the GUN, above the barrel and forward of the mantlet so it clears
    /// the solid turret shell (the authored interior POV would look at the turret's inside faces).
    /// Looks down the gun via the muzzle direction.
    pub fn gunsight(&self) -> Vec3 {
        // Offset LEFT of the barrel (−X): the Tiger's gunner sits to the left of the gun, not on
        // top of it. Above and forward of the mantlet so it clears the turret shell.
        self.gun_model()
            .transform_point3(Vec3::new(-0.55, 0.30, self.parts.gun_len * 0.5))
    }

    /// Commander's eye at the cupola, mounted on the TURRET roof — rotates with both hull steering
    /// and turret traverse (a real Tiger cupola turns with the turret ring it sits on).
    pub fn commander_eye(&self) -> Vec3 {
        self.turret_model()
            .transform_point3(self.parts.commander_eye)
    }

    /// Centre of mass, for hit tests.
    pub fn center(&self) -> Vec3 {
        self.pos + Vec3::new(0.0, self.parts.center_h, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Heightfield;

    /// A dead-flat heightfield at y=0, so a tank grounds at its spawn (no assets needed).
    fn flat() -> Heightfield {
        Heightfield {
            res: 2,
            spacing: 1000.0,
            heights: vec![0.0; 4],
            min_h: 0.0,
            max_h: 0.0,
        }
    }

    fn su100(x: f32, z: f32, heading: f32) -> Tank {
        let mut t = Tank::new(x, z, heading, &flat(), SU100_PARTS);
        t.kind = EnemyKind::Su100;
        t
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
