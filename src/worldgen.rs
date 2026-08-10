//! worldgen.rs — the ACT-SCALE in-engine world generator (Rust port of tools/worldgen/gen_world.py,
//! which stays only as the reference spec). Generates the full act map as INSTANCED placements:
//! a ruined city district of kitbashed PSX modules WITH walkable interiors (partition walls,
//! doorways, furniture, stairs, shell-holed upper floors), a countryside fringe (villages, pines),
//! an underground bunker (Retro FPS kit), and street rubble — thousands of placements of a few
//! dozen unique modules, spatially bucketed so `Renderer::add_instanced` batches cull well.
//!
//! Scope notes (v1): purely VISUAL/static. The hand-tuned combat block from `city::generate` keeps
//! its LOS occluders / destructible walls / emissive windows; this district surrounds it as the
//! act-scale world. Occluders + destructibility for the wide district come later (they need the
//! stealth system's cost model revisited at scale). Deterministic: counter-hash, no `rand`.

#![allow(dead_code)] // retained as an explicit dev/test generator, never a campaign fallback

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};

use crate::gltf_load::CpuMesh;
use crate::terrain::Heightfield;

// ── Module kit (paths verified on disk; MISSING assets are logged and skipped, never fatal) ──────
const MP2: &str =
    "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/";
const MP: &str =
    "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/";
const NAT: &str = "assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/";
const FPS: &str = "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/";

/// The kit keys. Paths assembled in `path_of` — one place to retarget an asset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kit {
    Wall,
    WallWin,
    WallHole,
    Doorway,
    Window,
    Floor,
    FloorHole,
    RoofPitch,
    RoofFlat,
    Pillar,
    PillarBroken,
    Stairs,
    RubbleField,
    RubbleStack,
    Brick,
    Warehouse,
    Garage,
    Shed,
    FurnBed,
    FurnTable,
    FurnChair,
    FurnCabinet,
    Pine,
    Pine2,
    Stone,
    FpsWall,
    FpsFloor,
    FpsStairs,
    FpsTunnel,
    FpsTunnelEnd,
    FpsCrate,
    FpsBarrel,
    // Environmental-storytelling vocabulary. These are deliberately small in number: each
    // module is instanced many times, so a tableau can read as specific without becoming a
    // draw-call-per-prop scene. Every path below was verified against the shipped asset library.
    Gravel,
    Bench,
    TrashCan,
    Bucket,
    TinCan,
    SupplyCrate,
    EmptySupplyCrate,
    WoodCrate,
    Board,
    Plank,
    Luggage,
    Bottle,
    Radio,
    FirstAid,
    Jerrycan,
    Toolbox,
    Tire,
    Axe,
    Shovel,
    HandSaw,
    Meat,
    CannedFood,
    MailBox,
    Grass,
    Grass2,
    Stump,
    Fence,
    Barricade,
    Chimney,
    Deadman,
    GraveCross,
    WreckT34,
    WreckPz3,
    WreckPanther,
    WreckTiger2,
    WreckIs2,
}

fn path_of(k: Kit) -> String {
    use Kit::*;
    match k {
        Wall => format!("{MP2}Modular Structures/wall_hr_1.glb"),
        WallWin => format!("{MP2}Modular Structures/wall_hr_1_m1a_1.glb"),
        WallHole => format!("{MP2}Modular Structures/wall_hr_1_hole_1.glb"),
        Doorway => format!("{MP2}Modular Structures/doorway_hr_1.glb"),
        Window => format!("{MP2}Modular Structures/window_hr_1.glb"),
        Floor => format!("{MP2}Modular Structures/floor_ceiling_hr_1.glb"),
        FloorHole => format!("{MP2}Modular Structures/floor_ceiling_hr_2_hole.glb"),
        RoofPitch => format!("{MP2}Modular Structures/roof_hr_3_middle_angle.glb"),
        RoofFlat => format!("{MP2}Modular Structures/roof_mh_4_a_1.glb"),
        Pillar => format!("{MP2}Modular Structures/pillar_hr_2.glb"),
        PillarBroken => format!("{MP2}Modular Structures/pillar_hr_1_broken.glb"),
        Stairs => format!("{MP}Modular Structures/stairs_5_concrete.glb"),
        RubbleField => format!("{MP2}Debris & Misc/debris_bricks_mx_1.glb"),
        RubbleStack => format!("{MP2}Debris & Misc/bricks_stacked_mx_1.glb"),
        Brick => format!("{MP2}Debris & Misc/brick_mx_1.glb"),
        Warehouse => format!("{MP2}Buildings/warehouse_mx_1.glb"),
        Garage => format!("{MP2}Buildings/garage_hl_1.glb"),
        Shed => format!("{MP2}Buildings/shed_ax_1.glb"),
        FurnBed => format!("{MP}Furniture/bed_1.glb"),
        FurnTable => format!("{MP}Furniture/coffee_table_1.glb"),
        FurnChair => format!("{MP}Furniture/chair_mp_1.glb"),
        FurnCabinet => format!("{MP}Furniture/bedside_table_1.glb"),
        Pine => format!("{NAT}pine_tree_n_1.glb"),
        Pine2 => format!("{NAT}pine_tree_n_2.glb"),
        Stone => format!("{NAT}stone_1.glb"),
        FpsWall => format!("{FPS}wall_2.glb"),
        FpsFloor => format!("{FPS}floor_1.glb"),
        FpsStairs => format!("{FPS}stairs_1.glb"),
        FpsTunnel => format!("{FPS}tunnel_1.glb"),
        FpsTunnelEnd => format!("{FPS}tunnel_1_end.glb"),
        FpsCrate => format!("{FPS}crate_1.glb"),
        FpsBarrel => format!("{FPS}metal_barrel_fps_1.glb"),
        Gravel => format!("{MP2}Debris & Misc/gravel_pile_hr_1.glb"),
        Bench => format!("{MP2}Props/bench_mx_1.glb"),
        // The pack has no municipal bin. Its narrow galvanized barrel is the period-correct
        // silhouette; Bucket/TinCan are progressively cheaper fallbacks at placement sites.
        TrashCan => format!("{MP2}Props/metal_barrel_hr_4.glb"),
        Bucket => format!("{MP2}Props/bucket_mx_1.glb"),
        TinCan => format!("{MP}Small Props/tin_can_mp_1_rusty.glb"),
        SupplyCrate => format!("{MP}Large Props/supply_crate_1.glb"),
        EmptySupplyCrate => format!("{MP}Large Props/supply_crate_1_empty.glb"),
        WoodCrate => format!("{MP2}Props/wooden_crate_1.glb"),
        Board => format!("{MP2}Props/wooden_board_2.glb"),
        Plank => format!("{MP2}Props/wooden_plank_1.glb"),
        Luggage => format!("{MP2}Props/luggage_mp_1.glb"),
        Bottle => format!("{MP2}Props/glass_bottle_mx_3.glb"),
        Radio => format!("{MP2}Props/handheld_fm_radio_etx_1.glb"),
        FirstAid => format!("{MP2}Props/first_aid_kit_hr_1.glb"),
        Jerrycan => format!("{MP2}Props/jerrycan_mx_1.glb"),
        Toolbox => format!("{MP2}Props/toolbox_mx_1.glb"),
        Tire => format!("{MP2}Props/tire_1.glb"),
        Axe => format!("{MP2}Weapons & Tools/axe_mx_1.glb"),
        Shovel => format!("{MP2}Weapons & Tools/shovel_mx_1.glb"),
        HandSaw => format!("{MP}Items & Weapons/hand_saw_1.glb"),
        Meat => format!("{MP}Items & Weapons/meat_1.glb"),
        CannedFood => format!("{MP}Items & Weapons/canned_food_mp_5_rusty.glb"),
        MailBox => format!("{MP2}Props/mail_box_mx_1.glb"),
        Grass => format!("{NAT}grass_1.glb"),
        Grass2 => format!("{NAT}grass_3.glb"),
        Stump => format!("{NAT}tree_stump_1.glb"),
        Fence => format!("{MP2}Modular Props/fence_b_1.glb"),
        Barricade => format!("{MP2}Modular Props/barricade_a_2.glb"),
        Chimney => format!("{MP2}Large Props & Machinery/chimney_a_2.glb"),
        Deadman => "assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb".into(),
        GraveCross => format!("{MP}Small Props/cross_1.glb"),
        WreckT34 => "assets/tanks/wrecks/t34_85.glb".into(),
        WreckPz3 => "assets/tanks/wrecks/pz3.glb".into(),
        WreckPanther => "assets/tanks/wrecks/panther.glb".into(),
        WreckTiger2 => "assets/tanks/wrecks/tiger2.glb".into(),
        WreckIs2 => "assets/tanks/wrecks/is2.glb".into(),
    }
}

const CELL: f32 = 4.0; // wall_hr_1 is 4.00 m wide
const WALL_H: f32 = 3.0; // storey height
const STREET: f32 = 10.0;
const MIN_FC: i32 = 3;
const MAX_FC: i32 = 5;
const PITCH: f32 = MAX_FC as f32 * CELL + STREET;
/// Spatial bucket size for instanced batches — the culling granularity.
const BUCKET: f32 = 96.0;

/// The generated world: per-module instanced placements, bucketed. `Game::build_act` feeds each
/// (mesh, transforms) pair to `Renderer::add_instanced` — except `tree_batches` (the pines),
/// which route through `trees::Trees::register_batch` so every trunk is DESTRUCTIBLE
/// (hinge-fall — docs/DESTRUCTION.md §5) while still drawing on the instanced path.
pub struct GenWorld {
    pub batches: Vec<(CpuMesh, Vec<Mat4>)>,
    pub tree_batches: Vec<(CpuMesh, Vec<Mat4>)>,
    /// Static hard-cover bounds authored by the overlay (wrecks, barricades, sheds). Tiny debris
    /// remains traversable; these boxes are handed to LOS, shell and tank-contact systems.
    pub cover_aabbs: Vec<(Vec3, Vec3)>,
    pub placements: usize,
}

fn story_hard_cover(k: Kit) -> bool {
    matches!(
        k,
        Kit::Wall
            | Kit::WallWin
            | Kit::WallHole
            | Kit::Doorway
            | Kit::Pillar
            | Kit::PillarBroken
            | Kit::Warehouse
            | Kit::Garage
            | Kit::Shed
            | Kit::FpsWall
            | Kit::FpsTunnel
            | Kit::FpsTunnelEnd
            | Kit::Fence
            | Kit::Barricade
            | Kit::Chimney
            | Kit::WreckT34
            | Kit::WreckPz3
            | Kit::WreckPanther
            | Kit::WreckTiger2
            | Kit::WreckIs2
    )
}

fn transformed_aabb(mesh: &CpuMesh, model: Mat4) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8u32 {
        let local = Vec3::new(
            if corner & 1 == 0 {
                mesh.min.x
            } else {
                mesh.max.x
            },
            if corner & 2 == 0 {
                mesh.min.y
            } else {
                mesh.max.y
            },
            if corner & 4 == 0 {
                mesh.min.z
            } else {
                mesh.max.z
            },
        );
        let world = model.transform_point3(local);
        min = min.min(world);
        max = max.max(world);
    }
    (min, max)
}

/// Deterministic [0,1) counter hash (project pattern, avalanched — mirrors city.rs / gen_world.py).
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

/// Load a kit module, normalizing PSX Nature GLBs: they're authored Z-up with arbitrary scene
/// offsets (each model parked side-by-side in the pack's master scene) — raw they render lying on
/// their side, displaced tens of metres and sunk into the terrain. Mirror the Blender pipeline's
/// normalization (tools/worldgen/assemble_blend.py "PSX Nature" block): upright via +90° about X,
/// then bbox bottom-centre to the origin so instances ground cleanly. Also used by the arena
/// (act 0) to dress its pine ring outside the instanced-district path.
pub fn load_kit(k: Kit) -> Option<CpuMesh> {
    load_module(&path_of(k))
}

/// Path-keyed variant of `load_kit` — the engine-side manifest loader (homeland acts) pulls
/// arbitrary GLBs from the act manifests and needs the same PSX Nature normalization.
pub fn load_module(path: &str) -> Option<CpuMesh> {
    // The NAT (Nature separated) pack used to need an upright+re-origin fix-up here — the GLBs
    // shipped lying on their sides with showcase-scene offsets. The files themselves are now
    // corrected on disk (tools/worldgen/fix_sideways_glbs.py, originals kept as *_orig.glb), so
    // EVERY consumer — engine, worldbuilder, thumbnails, the Blender build — sees them upright
    // without special-casing. Compensating here again would lay them back down.
    crate::gltf_load::load_glb(path).ok()
}

struct Gen<'a> {
    hf: &'a Heightfield,
    seed: u32,
    cache: HashMap<Kit, Option<CpuMesh>>,
    /// (kit, bucket) → per-instance transforms.
    buckets: HashMap<(Kit, i32, i32), Vec<Mat4>>,
    placements: usize,
    missing: usize,
}

impl Gen<'_> {
    fn rnd(&self, id: u32, salt: u32) -> f32 {
        hash01(self.seed ^ id.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA77))
    }

    fn loaded(&mut self, k: Kit) -> bool {
        if !self.cache.contains_key(&k) {
            let m = load_kit(k);
            if m.is_none() {
                self.missing += 1;
                println!("[worldgen] MISSING {}", path_of(k));
            }
            self.cache.insert(k, m);
        }
        self.cache.get(&k).map(Option::is_some).unwrap_or(false)
    }

    fn place(&mut self, k: Kit, x: f32, y: f32, z: f32, rot: f32, scale: f32) {
        self.place_pose(k, x, y, z, rot, 0.0, 0.0, scale);
    }

    /// General pose used by the story layer for toppled chairs, abandoned tools and wreck-adjacent
    /// clutter. The district generator keeps using `place`, so its established transforms do not
    /// change. Rotation order is yaw (world Y), then local pitch/roll.
    #[allow(clippy::too_many_arguments)]
    fn place_pose(
        &mut self,
        k: Kit,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        pitch: f32,
        roll: f32,
        scale: f32,
    ) {
        if !self.loaded(k) {
            return;
        }
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(scale),
            Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll),
            Vec3::new(x, y, z),
        );
        let bx = (x / BUCKET).floor() as i32;
        let bz = (z / BUCKET).floor() as i32;
        self.buckets.entry((k, bx, bz)).or_default().push(m);
        self.placements += 1;
    }

    /// Place the first available asset in a semantic fallback chain. This keeps a tableau legible
    /// on reduced installations: e.g. a missing Tiger-II wreck becomes a Pz.III, then a rubble heap;
    /// a missing trash barrel becomes the FPS barrel, then a bucket. Missing optional libraries are
    /// still logged once by `loaded`, never fatal.
    #[allow(clippy::too_many_arguments)]
    fn place_first_pose(
        &mut self,
        choices: &[Kit],
        x: f32,
        z: f32,
        y_offset: f32,
        yaw: f32,
        pitch: f32,
        roll: f32,
        scale: f32,
    ) {
        let mut chosen = None;
        for &k in choices {
            if self.loaded(k) {
                chosen = Some(k);
                break;
            }
        }
        if let Some(k) = chosen {
            let y = self.ground(x, z) + y_offset;
            self.place_pose(k, x, y, z, yaw, pitch, roll, scale);
        }
    }

    fn place_first(
        &mut self,
        choices: &[Kit],
        x: f32,
        z: f32,
        y_offset: f32,
        yaw: f32,
        scale: f32,
    ) {
        self.place_first_pose(choices, x, z, y_offset, yaw, 0.0, 0.0, scale);
    }

    /// Counter-hash scatter in a disk. The item order, angle, radius, scale and yaw all derive from
    /// `(act seed, patch id, item index)`, so adding a manifest edit cannot perturb these placements.
    #[allow(clippy::too_many_arguments)]
    fn scatter_patch(
        &mut self,
        cx: f32,
        cz: f32,
        patch_id: u32,
        count: u32,
        radius: f32,
        choices: &[Kit],
        min_scale: f32,
        max_scale: f32,
        y_offset: f32,
        exclude_r: f32,
    ) {
        if choices.is_empty() {
            return;
        }
        for i in 0..count {
            let a = self.rnd(patch_id, i * 7 + 1) * std::f32::consts::TAU;
            let d = self.rnd(patch_id, i * 7 + 2).sqrt() * radius;
            let x = cx + a.cos() * d;
            let z = cz + a.sin() * d;
            if x * x + z * z < exclude_r * exclude_r {
                continue;
            }
            let ki = (self.rnd(patch_id, i * 7 + 3) * choices.len() as f32) as usize;
            let k = choices[ki.min(choices.len() - 1)];
            let scale = min_scale + self.rnd(patch_id, i * 7 + 4) * (max_scale - min_scale);
            let yaw = self.rnd(patch_id, i * 7 + 5) * std::f32::consts::TAU;
            self.place_first(
                &[k, Kit::RubbleField, Kit::Brick],
                x,
                z,
                y_offset,
                yaw,
                scale,
            );
        }
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        self.hf.height_at(x, z)
    }

    // ── one kitbashed building WITH interior (partitions, furniture, stairs) ──
    #[allow(clippy::too_many_arguments)]
    fn building(&mut self, cx: f32, cz: f32, bid: u32, r: f32) {
        let base = self.ground(cx, cz) - 0.3;
        let fcx = (MIN_FC + (self.rnd(bid, 2) * (MAX_FC - MIN_FC + 1) as f32) as i32).min(MAX_FC);
        let fcz = (MIN_FC + (self.rnd(bid, 3) * (MAX_FC - MIN_FC + 1) as f32) as i32).min(MAX_FC);
        let (hx, hz) = (fcx as f32 * CELL * 0.5, fcz as f32 * CELL * 0.5);
        let storeys = if self.rnd(bid, 4) < (1.0 - r) * 0.45 {
            2
        } else {
            1
        };

        for s in 0..storeys {
            let y = base + s as f32 * WALL_H;
            let ground = s == 0;
            for i in 0..fcx {
                let wx = cx - hx + (i as f32 + 0.5) * CELL;
                self.wall_seg(wx, y, cz - hz, 0.0, bid, (s * 40 + i) as u32, r, ground);
                self.wall_seg(
                    wx,
                    y,
                    cz + hz,
                    0.0,
                    bid,
                    (s * 40 + 10 + i) as u32,
                    r,
                    ground,
                );
            }
            for j in 0..fcz {
                let wz = cz - hz + (j as f32 + 0.5) * CELL;
                let hpi = std::f32::consts::FRAC_PI_2;
                self.wall_seg(
                    cx - hx,
                    y,
                    wz,
                    hpi,
                    bid,
                    (s * 40 + 20 + j) as u32,
                    r,
                    ground,
                );
                self.wall_seg(
                    cx + hx,
                    y,
                    wz,
                    hpi,
                    bid,
                    (s * 40 + 30 + j) as u32,
                    r,
                    ground,
                );
            }
            for (k, (px, pz)) in [
                (cx - hx, cz - hz),
                (cx + hx, cz - hz),
                (cx - hx, cz + hz),
                (cx + hx, cz + hz),
            ]
            .iter()
            .enumerate()
            {
                let v = self.rnd(bid, 300 + (s * 4 + k as i32) as u32);
                if v < 0.15 * r {
                    continue; // blown away
                }
                if v < 0.4 * r {
                    self.place(
                        Kit::PillarBroken,
                        *px,
                        y,
                        *pz,
                        self.rnd(bid, 900 + k as u32) * std::f32::consts::TAU,
                        1.0,
                    );
                } else {
                    self.place(Kit::Pillar, *px, y, *pz, 0.0, 1.0);
                }
            }
            // floor slabs; upper storeys can be shell-holed
            for gi in 0..fcx {
                for gj in 0..fcz {
                    let fx = cx - hx + (gi as f32 + 0.5) * CELL;
                    let fz = cz - hz + (gj as f32 + 0.5) * CELL;
                    let holed = s > 0 && self.rnd(bid, 1300 + (gi * 8 + gj) as u32) < 0.25 * r;
                    self.place(
                        if holed { Kit::FloorHole } else { Kit::Floor },
                        fx,
                        y,
                        fz,
                        0.0,
                        1.0,
                    );
                }
            }
            // INTERIOR partition row with a doorway → two walkable rooms
            if fcx >= 4 {
                let pj = fcz / 2;
                let wz = cz - hz + pj as f32 * CELL;
                let door_i = 1 + (self.rnd(bid, 1400 + s as u32) * (fcx - 2) as f32) as i32;
                for i in 0..fcx {
                    let wx = cx - hx + (i as f32 + 0.5) * CELL;
                    if self.rnd(bid, 1500 + (s * 16 + i) as u32) < 0.30 * r {
                        continue; // ruination ate this partition segment
                    }
                    self.place(
                        if i == door_i { Kit::Doorway } else { Kit::Wall },
                        wx,
                        y,
                        wz,
                        0.0,
                        1.0,
                    );
                }
            }
            // scavengeable ground-floor furniture
            if ground {
                let nf = 2 + (self.rnd(bid, 1600) * 4.0) as u32;
                for f in 0..nf {
                    let fx = cx - hx + CELL + self.rnd(bid, 1700 + f) * (2.0 * hx - 2.0 * CELL);
                    let fz = cz - hz + CELL + self.rnd(bid, 1750 + f) * (2.0 * hz - 2.0 * CELL);
                    let kind = [
                        Kit::FurnBed,
                        Kit::FurnTable,
                        Kit::FurnChair,
                        Kit::FurnCabinet,
                    ][(self.rnd(bid, 1800 + f) * 4.0) as usize % 4];
                    self.place(
                        kind,
                        fx,
                        base + 0.05,
                        fz,
                        self.rnd(bid, 1850 + f) * std::f32::consts::TAU,
                        1.0,
                    );
                }
            }
        }
        if storeys == 2 {
            self.place(
                Kit::Stairs,
                cx - hx + CELL * 0.9,
                base,
                cz + hz - CELL * 0.9,
                std::f32::consts::PI,
                1.0,
            );
        }
        // roof — mostly gone under heavy ruination
        let top = base + storeys as f32 * WALL_H;
        if self.rnd(bid, 9) > 0.25 + 0.6 * r {
            let flat = self.rnd(bid, 8) < 0.4;
            for gi in 0..fcx {
                for gj in 0..fcz {
                    let fx = cx - hx + (gi as f32 + 0.5) * CELL;
                    let fz = cz - hz + (gj as f32 + 0.5) * CELL;
                    if flat {
                        if gi % 2 == 0 {
                            self.place(Kit::RoofFlat, fx + CELL * 0.5, top - 0.05, fz, 0.0, 1.0);
                        }
                    } else {
                        self.place(Kit::RoofPitch, fx, top - 0.05, fz, 0.0, 1.0);
                    }
                }
            }
        }
        // street rubble ring
        let n = 1 + (r * 5.0) as u32;
        for k in 0..n {
            let a = self.rnd(bid, 400 + k) * std::f32::consts::TAU;
            let d = hx.max(hz) + 1.0 + self.rnd(bid, 450 + k) * (STREET * 0.5);
            let (x, z) = (cx + a.cos() * d, cz + a.sin() * d);
            let kind = [Kit::RubbleField, Kit::RubbleStack, Kit::Brick]
                [(self.rnd(bid, 500 + k) * 3.0) as usize % 3];
            self.place(
                kind,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 550 + k) * std::f32::consts::TAU,
                0.8 + self.rnd(bid, 600 + k) * 0.6,
            );
        }
    }

    fn wall_seg(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        rot: f32,
        bid: u32,
        key: u32,
        r: f32,
        ground: bool,
    ) {
        let v = self.rnd(bid, 100 + key);
        if v < 0.08 + 0.30 * r {
            self.place(
                Kit::RubbleStack,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 700 + key) * std::f32::consts::TAU,
                1.0,
            );
            return;
        }
        let w = self.rnd(bid, 200 + key);
        let k = if w < 0.25 * r {
            Kit::WallHole
        } else if ground && w < 0.25 * r + 0.10 {
            Kit::Doorway
        } else if w < 0.25 * r + 0.30 {
            if ground {
                Kit::WallWin
            } else {
                Kit::Window
            }
        } else {
            Kit::Wall
        };
        self.place(k, x, y, z, rot, 1.0);
        if k == Kit::WallHole {
            self.place(
                Kit::RubbleField,
                x,
                self.ground(x, z) - 0.1,
                z,
                self.rnd(bid, 800 + key) * std::f32::consts::TAU,
                1.0,
            );
        }
    }

    /// The underground bunker: stair shaft down, tunnel corridor, stocked store room (Retro FPS kit).
    fn bunker(&mut self, x: f32, z: f32) {
        let depth = self.ground(x, z) - 5.0;
        self.place(Kit::FpsStairs, x, depth, z, 0.0, 1.0);
        for k in 0..4 {
            self.place(Kit::FpsTunnel, x, depth, z - 6.0 - k as f32 * 4.0, 0.0, 1.0);
        }
        self.place(Kit::FpsTunnelEnd, x, depth, z - 24.0, 0.0, 1.0);
        let (rx, rz) = (x + 8.0, z - 14.0);
        for i in -1..=1 {
            for j in -1..=1 {
                self.place(
                    Kit::FpsFloor,
                    rx + i as f32 * 4.0,
                    depth,
                    rz + j as f32 * 4.0,
                    0.0,
                    1.0,
                );
            }
        }
        for i in -1..=1i32 {
            self.place(Kit::FpsWall, rx + i as f32 * 4.0, depth, rz - 6.0, 0.0, 1.0);
            self.place(Kit::FpsWall, rx + i as f32 * 4.0, depth, rz + 6.0, 0.0, 1.0);
            self.place(
                Kit::FpsWall,
                rx + 6.0,
                depth,
                rz + i as f32 * 4.0,
                std::f32::consts::FRAC_PI_2,
                1.0,
            );
        }
        for k in 0..5u32 {
            self.place(
                Kit::FpsCrate,
                rx - 3.0 + hash01(k * 77) * 6.0,
                depth + 0.05,
                rz - 3.0 + hash01(k * 131) * 6.0,
                hash01(k * 313) * std::f32::consts::TAU,
                1.0,
            );
        }
        for k in 0..3u32 {
            self.place(
                Kit::FpsBarrel,
                rx + 3.0 - hash01(k * 991) * 2.0,
                depth + 0.05,
                rz + 2.0,
                0.0,
                1.0,
            );
        }
    }
}

// ── Environmental-storytelling overlay ─────────────────────────────────────────────────────────
//
// The manifest is the authority for macro layout, but a macro layout can still be visually mute.
// This layer is intentionally ADDITIVE and independent of manifest presence: a small vocabulary of
// verified assets is recombined into wordless, act-specific tableaus, then a lighter deterministic
// clutter pass joins those authored beats along the east→west route. Nothing here creates gameplay
// state or readable text. It is just geometry whose arrangement tells the story.

fn route_clutter(g: &mut Gen<'_>, act: &crate::world::ActMap, exclude_r: f32) {
    let delta = act.goal - act.spawn;
    let dir = delta.normalize_or_zero();
    let side = glam::Vec2::new(-dir.y, dir.x);
    let count = if act.homeland { 300u32 } else { 220u32 };
    let winter = [
        Kit::RubbleField,
        Kit::Brick,
        Kit::Gravel,
        Kit::Board,
        Kit::TrashCan,
        Kit::TinCan,
        Kit::WoodCrate,
        Kit::Bottle,
    ];
    let spring = [
        Kit::Grass,
        Kit::Grass2,
        Kit::Stump,
        Kit::Board,
        Kit::Bucket,
        Kit::TinCan,
        Kit::WoodCrate,
        Kit::Bottle,
    ];
    let choices = if act.homeland {
        &spring[..]
    } else {
        &winter[..]
    };

    for i in 0..count {
        // Keep the literal road centre readable: clutter tells you where people travelled without
        // turning the visual layer into an invisible collision wall.
        let t = 0.035 + g.rnd(0xC17A_0000 + act.id as u32, i * 11 + 1) * 0.93;
        let along = (g.rnd(0xC17A_1000 + act.id as u32, i * 11 + 2) - 0.5) * 70.0;
        let side_sign = if g.rnd(0xC17A_2000 + act.id as u32, i * 11 + 3) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let lateral = side_sign * (38.0 + g.rnd(0xC17A_3000 + act.id as u32, i * 11 + 4) * 235.0);
        let p = act.spawn + delta * t + dir * along + side * lateral;
        if p.x.abs() > act.half_extent_m - 35.0 || p.y.abs() > act.half_extent_m - 35.0 {
            continue;
        }
        if p.length_squared() < exclude_r * exclude_r {
            continue;
        }
        let ki = (g.rnd(0xC17A_4000 + act.id as u32, i * 11 + 5) * choices.len() as f32) as usize
            % choices.len();
        let k = choices[ki];
        let scale = 0.72 + g.rnd(0xC17A_5000 + act.id as u32, i * 11 + 6) * 0.58;
        let yaw = g.rnd(0xC17A_6000 + act.id as u32, i * 11 + 7) * std::f32::consts::TAU;
        let fallback = if act.homeland {
            Kit::Grass
        } else {
            Kit::RubbleField
        };
        g.place_first(&[k, fallback, Kit::Brick], p.x, p.y, -0.06, yaw, scale);
    }

    // Every landmark acquires a human-scale apron: bins, chairs, cans, boards and vegetation. The
    // macro silhouette still navigates; the last thirty metres finally look inhabited and fought in.
    for (li, lm) in act.landmarks.iter().enumerate() {
        let household = if act.homeland {
            [
                Kit::Grass,
                Kit::Grass2,
                Kit::FurnChair,
                Kit::Bench,
                Kit::Bucket,
                Kit::WoodCrate,
            ]
        } else {
            [
                Kit::RubbleField,
                Kit::Brick,
                Kit::FurnChair,
                Kit::TrashCan,
                Kit::Board,
                Kit::WoodCrate,
            ]
        };
        g.scatter_patch(
            lm.pos.x,
            lm.pos.y,
            0x1A00_0000 ^ ((act.id as u32) << 16) ^ li as u32,
            18,
            34.0,
            &household,
            0.78,
            1.18,
            -0.05,
            exclude_r,
        );
        let a = g.rnd(0x1A10_0000 + act.id as u32, li as u32) * std::f32::consts::TAU;
        let d = 18.0 + g.rnd(0x1A20_0000 + act.id as u32, li as u32) * 10.0;
        let (tx, tz) = (lm.pos.x + a.cos() * d, lm.pos.y + a.sin() * d);
        if tx * tx + tz * tz >= exclude_r * exclude_r {
            g.place_first(
                &[Kit::TrashCan, Kit::FpsBarrel, Kit::Bucket],
                tx,
                tz,
                -0.04,
                a,
                0.82,
            );
        }
    }
}

fn act1_story(g: &mut Gen<'_>, exclude_r: f32) {
    // HAUPTBAHNHOF FIELD HOSPITAL: orderly bed rows dissolve into luggage and bodies toward the
    // west. The arrangement reads triage, evacuation and abandonment without a label or cutscene.
    let (hx, hz) = (-842.0, -258.0);
    for row in 0..3u32 {
        for col in 0..5u32 {
            let x = hx + col as f32 * 3.15;
            let z = hz + row as f32 * 3.4;
            let yaw = if row == 2 && col >= 3 {
                1.18
            } else {
                std::f32::consts::FRAC_PI_2
            };
            g.place_first(
                &[Kit::FurnBed, Kit::Board, Kit::FpsCrate],
                x,
                z,
                0.02,
                yaw,
                0.92,
            );
            if (row + col) % 2 == 0 {
                g.place_first(
                    &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
                    x + 0.3,
                    z,
                    0.06,
                    yaw,
                    1.0,
                );
            }
        }
    }
    for i in 0..7u32 {
        g.place_first(
            &[Kit::Luggage, Kit::WoodCrate, Kit::RubbleStack],
            hx - 8.0 - i as f32 * 2.2,
            hz + 2.0 + (i & 1) as f32 * 1.3,
            0.02,
            g.rnd(0xA101, i) * std::f32::consts::TAU,
            0.9,
        );
    }
    for i in 0..4u32 {
        g.place_first(
            &[Kit::FirstAid, Kit::FpsCrate, Kit::WoodCrate],
            hx + 2.0 + i as f32 * 2.4,
            hz - 4.0,
            0.04,
            0.0,
            0.9,
        );
    }
    g.place_first(
        &[Kit::Radio, Kit::FpsCrate],
        hx + 4.5,
        hz - 2.8,
        0.08,
        0.3,
        1.0,
    );

    // THE ABSURD DROP FIELD: intact canisters give way to split empties and useless household
    // debris. A few jerrycans sit tantalisingly among far more cans and boxes.
    let (dx, dz) = (-1340.0, -860.0);
    g.scatter_patch(
        dx,
        dz,
        0xA110,
        22,
        43.0,
        &[
            Kit::SupplyCrate,
            Kit::EmptySupplyCrate,
            Kit::FpsBarrel,
            Kit::WoodCrate,
        ],
        0.82,
        1.18,
        -0.04,
        exclude_r,
    );
    g.scatter_patch(
        dx,
        dz,
        0xA111,
        34,
        36.0,
        &[
            Kit::TinCan,
            Kit::CannedFood,
            Kit::Bottle,
            Kit::Board,
            Kit::Luggage,
        ],
        0.75,
        1.15,
        -0.01,
        exclude_r,
    );
    for i in 0..3u32 {
        g.place_first(
            &[Kit::Jerrycan, Kit::FpsBarrel, Kit::WoodCrate],
            dx - 12.0 + i as f32 * 8.5,
            dz + 5.0 - i as f32 * 3.0,
            0.0,
            g.rnd(0xA112, i) * std::f32::consts::TAU,
            1.0,
        );
    }

    // THE LAST BUTCHER SITE: meat, saw, axe and a blackened bucket around a plank work surface.
    // The missing animal is the point — hunger has reduced the column to tools and scraps.
    let (bx, bz) = (-615.0, 825.0);
    g.place_first(
        &[Kit::Board, Kit::Plank, Kit::WoodCrate],
        bx,
        bz,
        0.02,
        0.7,
        1.1,
    );
    g.place_first_pose(
        &[Kit::Axe, Kit::Shovel, Kit::Board],
        bx + 0.8,
        bz - 0.4,
        0.08,
        0.4,
        0.0,
        0.7,
        1.0,
    );
    g.place_first_pose(
        &[Kit::HandSaw, Kit::Axe, Kit::Plank],
        bx - 0.9,
        bz + 0.2,
        0.06,
        1.0,
        0.0,
        0.5,
        1.0,
    );
    for i in 0..5u32 {
        g.place_first(
            &[Kit::Meat, Kit::CannedFood, Kit::TinCan],
            bx - 1.0 + i as f32 * 0.45,
            bz - 0.8,
            0.04,
            g.rnd(0xA120, i) * std::f32::consts::TAU,
            0.9,
        );
    }
    g.place_first(
        &[Kit::Bucket, Kit::FpsBarrel],
        bx + 2.0,
        bz + 0.7,
        0.0,
        0.0,
        0.9,
    );
    g.place_first(
        &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
        bx + 3.2,
        bz - 1.4,
        0.04,
        2.2,
        1.0,
    );
    g.scatter_patch(
        bx,
        bz,
        0xA121,
        13,
        12.0,
        &[Kit::TinCan, Kit::Bottle, Kit::Board, Kit::Brick],
        0.72,
        1.05,
        -0.02,
        exclude_r,
    );

    // THE TURRET LINE: three T-34 hulls buried into the old front, with fresh Soviet stores laid
    // over older German rubble. The two material layers encode autumn attack beneath winter return.
    for i in 0..3u32 {
        let x = 510.0 + i as f32 * 38.0;
        let z = 365.0 + (i & 1) as f32 * 12.0;
        g.place_first(
            &[Kit::WreckT34, Kit::WreckPz3, Kit::RubbleStack],
            x,
            z,
            -0.85,
            -std::f32::consts::FRAC_PI_2 + g.rnd(0xA130, i) * 0.35,
            1.0,
        );
        g.scatter_patch(
            x,
            z,
            0xA131 + i,
            14,
            15.0,
            &[
                Kit::FpsCrate,
                Kit::SupplyCrate,
                Kit::RubbleField,
                Kit::Brick,
                Kit::Deadman,
            ],
            0.82,
            1.12,
            -0.04,
            exclude_r,
        );
    }

    // ROSSOSHKA'S ERASED VILLAGE: only chimney stems retain the house grid. Domestic scraps and
    // fresh rubble sit between them; repetition makes the absent buildings visible.
    let (vx, vz) = (-1640.0, 610.0);
    for row in 0..3u32 {
        for col in 0..3u32 {
            let x = vx + col as f32 * 14.0;
            let z = vz + row as f32 * 13.0;
            g.place_first(
                &[Kit::Chimney, Kit::PillarBroken, Kit::RubbleStack],
                x,
                z,
                -0.12,
                g.rnd(0xA140, row * 3 + col) * 0.12,
                0.34,
            );
            g.scatter_patch(
                x,
                z,
                0xA150 + row * 3 + col,
                7,
                7.5,
                &[
                    Kit::RubbleField,
                    Kit::Brick,
                    Kit::FurnChair,
                    Kit::TinCan,
                    Kit::Board,
                ],
                0.72,
                1.06,
                -0.05,
                exclude_r,
            );
        }
    }
}

fn act2_story(g: &mut Gen<'_>, exclude_r: f32) {
    // THE OLD HOUSE AFTER BREAKFAST: a room's furniture remains in plan, but there are no walls.
    // A luggage trail points west and spring grass has begun to occupy the domestic outline.
    let (hx, hz) = (430.0, 205.0);
    g.place_first(&[Kit::FurnTable, Kit::WoodCrate], hx, hz, 0.02, 0.15, 1.0);
    for (i, (ox, oz, yaw)) in [
        (-2.0, 0.0, 1.57),
        (2.0, 0.0, -1.57),
        (0.0, -2.0, 0.0),
        (0.0, 2.0, 3.14),
    ]
    .into_iter()
    .enumerate()
    {
        let tipped = i == 3;
        g.place_first_pose(
            &[Kit::FurnChair, Kit::FpsCrate],
            hx + ox,
            hz + oz,
            0.03,
            yaw,
            if tipped { 0.85 } else { 0.0 },
            0.0,
            1.0,
        );
    }
    g.place_first(
        &[Kit::FurnBed, Kit::Board],
        hx + 7.0,
        hz + 4.0,
        0.03,
        1.57,
        1.0,
    );
    g.place_first(
        &[Kit::FurnCabinet, Kit::WoodCrate],
        hx - 6.0,
        hz + 4.0,
        0.03,
        0.0,
        1.0,
    );
    g.place_first(
        &[Kit::MailBox, Kit::WoodCrate],
        hx - 10.0,
        hz - 8.0,
        0.0,
        0.3,
        1.0,
    );
    for i in 0..9u32 {
        g.place_first(
            &[Kit::Luggage, Kit::WoodCrate, Kit::Board],
            hx - 4.0 - i as f32 * 2.5,
            hz - 3.0 + (i & 1) as f32 * 1.2,
            0.01,
            g.rnd(0xA201, i) * 6.28,
            0.9,
        );
    }
    g.scatter_patch(
        hx,
        hz,
        0xA202,
        42,
        28.0,
        &[
            Kit::Grass,
            Kit::Grass2,
            Kit::Bottle,
            Kit::TinCan,
            Kit::Board,
        ],
        0.72,
        1.22,
        -0.05,
        exclude_r,
    );

    // CANAL BRIDGE REPAIR THAT NEVER FINISHED: fresh timber, tools and fuel surround two mired
    // wrecks. Barricades point the player toward the intact approach while telling why it matters.
    let (cx, cz) = (-1490.0, 515.0);
    g.place_first(
        &[Kit::WreckT34, Kit::WreckPz3, Kit::RubbleStack],
        cx - 14.0,
        cz + 8.0,
        -0.28,
        0.3,
        1.0,
    );
    g.place_first(
        &[Kit::WreckIs2, Kit::WreckT34, Kit::RubbleStack],
        cx + 18.0,
        cz - 10.0,
        -0.42,
        2.7,
        1.0,
    );
    for i in 0..10u32 {
        g.place_first(
            &[Kit::Plank, Kit::Board, Kit::WoodCrate],
            cx - 12.0 + i as f32 * 2.7,
            cz + (i % 3) as f32 * 1.1,
            0.02,
            1.57 + g.rnd(0xA210, i) * 0.18,
            1.0,
        );
    }
    for i in 0..4u32 {
        g.place_first(
            &[Kit::Barricade, Kit::Fence, Kit::RubbleStack],
            cx - 18.0 + i as f32 * 12.0,
            cz - 18.0,
            0.0,
            0.0,
            1.0,
        );
    }
    g.scatter_patch(
        cx,
        cz,
        0xA211,
        32,
        30.0,
        &[
            Kit::Jerrycan,
            Kit::Toolbox,
            Kit::Tire,
            Kit::Shovel,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::Grass,
        ],
        0.8,
        1.14,
        -0.02,
        exclude_r,
    );

    // THE BOYS' TRENCH: no labels, just undersized domestic traces among fighting tools — wheel
    // rims, shovels, civilian luggage and bodies behind a thin improvised line.
    let (tx, tz) = (-690.0, -805.0);
    for i in 0..8u32 {
        g.place_first(
            &[Kit::Fence, Kit::Barricade, Kit::RubbleStack],
            tx + i as f32 * 4.0,
            tz,
            -0.08,
            0.0,
            0.9,
        );
    }
    g.scatter_patch(
        tx + 14.0,
        tz + 5.0,
        0xA220,
        34,
        22.0,
        &[
            Kit::Tire,
            Kit::Shovel,
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::TinCan,
        ],
        0.78,
        1.08,
        -0.03,
        exclude_r,
    );
    g.scatter_patch(
        tx + 14.0,
        tz + 5.0,
        0xA221,
        24,
        27.0,
        &[Kit::Grass, Kit::Grass2, Kit::Stump],
        0.72,
        1.3,
        -0.07,
        exclude_r,
    );
}

fn act3_story(g: &mut Gen<'_>, exclude_r: f32) {
    // THE JAMMED FOREST ROAD: every vehicle and every dropped possession faces west. The geometry
    // itself supplies direction and scale before the player ever consults the map.
    let (jx, jz) = (-860.0, 315.0);
    let wrecks = [
        Kit::WreckPz3,
        Kit::WreckPanther,
        Kit::WreckT34,
        Kit::WreckTiger2,
        Kit::WreckIs2,
    ];
    for (i, wreck) in wrecks.into_iter().enumerate() {
        let x = jx + i as f32 * 19.0;
        let z = jz + if i & 1 == 0 { -3.5 } else { 3.5 };
        g.place_first(
            &[wreck, Kit::WreckPz3, Kit::RubbleStack],
            x,
            z,
            -0.12,
            -std::f32::consts::FRAC_PI_2 + g.rnd(0xA301, i as u32) * 0.22,
            1.0,
        );
    }
    g.scatter_patch(
        jx + 38.0,
        jz,
        0xA302,
        74,
        62.0,
        &[
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Tire,
            Kit::Jerrycan,
            Kit::Board,
            Kit::TinCan,
            Kit::Deadman,
            Kit::Toolbox,
        ],
        0.75,
        1.14,
        -0.03,
        exclude_r,
    );
    g.scatter_patch(
        jx + 38.0,
        jz,
        0xA303,
        42,
        68.0,
        &[Kit::Grass, Kit::Grass2, Kit::Stump, Kit::RubbleField],
        0.72,
        1.3,
        -0.07,
        exclude_r,
    );

    // THE AUTOBAHN BLOCKING LINE: a deliberate anti-tank closure, distinct from the refugee jam.
    // Wrecks are nose-in across the carriageway; tools and opened crates behind them imply a crew
    // that prepared the position and then disappeared. This second road scene now lives far enough
    // away that it must be found on its own rather than triggering with the column above.
    let (ax, az) = (650.0, -300.0);
    for (i, (wreck, ox, oz, yaw)) in [
        (Kit::WreckT34, -22.0, -5.0, 0.18),
        (Kit::WreckPanther, -7.0, 3.5, -0.12),
        (Kit::WreckPz3, 10.0, -3.0, 0.10),
        (Kit::WreckIs2, 26.0, 4.0, -0.16),
    ]
    .into_iter()
    .enumerate()
    {
        g.place_first(
            &[wreck, Kit::Barricade, Kit::RubbleStack],
            ax + ox,
            az + oz,
            -0.10,
            yaw + std::f32::consts::FRAC_PI_2,
            1.0,
        );
        g.place_first(
            &[Kit::Barricade, Kit::Fence, Kit::Board],
            ax + ox + 5.0,
            az + oz - 7.0,
            -0.04,
            yaw,
            1.0,
        );
        if i & 1 == 0 {
            g.place_first(
                &[Kit::Deadman, Kit::Luggage, Kit::RubbleField],
                ax + ox - 2.5,
                az + oz + 5.5,
                0.02,
                yaw,
                1.0,
            );
        }
    }
    g.scatter_patch(
        ax,
        az - 10.0,
        0xA306,
        42,
        38.0,
        &[
            Kit::EmptySupplyCrate,
            Kit::Toolbox,
            Kit::Tire,
            Kit::Shovel,
            Kit::Board,
            Kit::TinCan,
        ],
        0.78,
        1.12,
        -0.03,
        exclude_r,
    );

    // THE DRY TIGER: visually intact heavy armour, every fuel vessel empty, tow attempt abandoned.
    let (tx, tz) = (-1265.0, -75.0);
    g.place_first(
        &[Kit::WreckTiger2, Kit::WreckPanther, Kit::RubbleStack],
        tx,
        tz,
        -0.08,
        -std::f32::consts::FRAC_PI_2,
        1.0,
    );
    for i in 0..8u32 {
        g.place_first(
            &[Kit::Jerrycan, Kit::EmptySupplyCrate, Kit::FpsBarrel],
            tx - 8.0 + i as f32 * 2.1,
            tz + 6.0 + (i & 1) as f32,
            0.0,
            g.rnd(0xA310, i) * 6.28,
            0.9,
        );
    }
    for i in 0..5u32 {
        g.place_first(
            &[Kit::Plank, Kit::Board, Kit::WoodCrate],
            tx + 6.0 + i as f32 * 2.0,
            tz - 4.0,
            0.01,
            -1.57,
            1.0,
        );
    }
    g.place_first(
        &[Kit::Toolbox, Kit::WoodCrate],
        tx + 3.5,
        tz + 5.0,
        0.02,
        0.4,
        1.0,
    );
    g.place_first(
        &[Kit::Tire, Kit::FpsBarrel],
        tx + 6.0,
        tz + 5.5,
        0.0,
        0.2,
        1.0,
    );

    // WALDFRIEDHOF: small identical crosses follow the same firebreak grid used as roads. Spring
    // grass interrupts the rows, making the battlefield/cemetery identity visible in one frame.
    let (gx, gz) = (-1460.0, -245.0);
    for row in 0..6u32 {
        for col in 0..9u32 {
            let x = gx + col as f32 * 2.45;
            let z = gz + row as f32 * 2.85;
            g.place_first(
                &[Kit::GraveCross, Kit::Stump, Kit::Brick],
                x,
                z,
                -0.02,
                0.0,
                0.82,
            );
            if (row * 9 + col) % 3 == 0 {
                g.place_first(
                    &[Kit::Grass, Kit::Grass2, Kit::Stone],
                    x + 0.7,
                    z + 0.5,
                    -0.07,
                    g.rnd(0xA320, row * 9 + col) * 6.28,
                    0.78,
                );
            }
        }
    }

    // A second, smaller column at the rail halt mixes military wreckage with bedroom furniture —
    // civilians and army sharing the same narrowing road, the signature horror of Halbe.
    let (cx, cz) = (420.0, 455.0);
    g.place_first(&[Kit::WreckPz3, Kit::RubbleStack], cx, cz, -0.1, -1.57, 1.0);
    g.place_first(
        &[Kit::WreckPanther, Kit::WreckT34, Kit::RubbleStack],
        cx + 22.0,
        cz + 2.5,
        -0.1,
        -1.45,
        1.0,
    );
    g.scatter_patch(
        cx + 10.0,
        cz,
        0xA330,
        38,
        34.0,
        &[
            Kit::FurnBed,
            Kit::FurnChair,
            Kit::Luggage,
            Kit::WoodCrate,
            Kit::Deadman,
            Kit::Board,
        ],
        0.8,
        1.08,
        -0.03,
        exclude_r,
    );
}

/// Deterministic, additive environmental-storytelling layer for the campaign acts.
///
/// Unlike `generate`, this must be called even when a worldbuilder manifest loaded successfully:
/// manifests own the macro world; this function supplies the always-on human evidence and
/// act-specific micro-dioramas. Output uses the same `(mesh, transforms)` spatial buckets as the
/// district generator, so callers feed it through `Renderer::add_instanced` / `Trees::register_batch`.
/// Act 0 / the dev garage intentionally returns an empty layer.
pub fn generate_story_overlay(
    act: &crate::world::ActMap,
    hf: &Heightfield,
    exclude_r: f32,
) -> GenWorld {
    if act.id == 0 || act.garage {
        return GenWorld {
            batches: Vec::new(),
            tree_batches: Vec::new(),
            cover_aabbs: Vec::new(),
            placements: 0,
        };
    }

    let mut g = Gen {
        hf,
        seed: act.seed ^ 0xE57A_0A11,
        cache: HashMap::new(),
        buckets: HashMap::new(),
        placements: 0,
        missing: 0,
    };
    route_clutter(&mut g, act, exclude_r);
    match act.id {
        1 => act1_story(&mut g, exclude_r),
        2 => act2_story(&mut g, exclude_r),
        3 => act3_story(&mut g, exclude_r),
        _ => {}
    }

    // HashMap uses a per-process random state; sort drained bucket keys so the GPU registration
    // order is deterministic too, not merely the generated transforms within each bucket.
    let mut drained: Vec<_> = g.buckets.drain().collect();
    drained.sort_by_key(|((k, bx, bz), _)| (*k as u16, *bx, *bz));
    let mut batches = Vec::with_capacity(drained.len());
    let mut tree_batches = Vec::new();
    let mut cover_aabbs = Vec::new();
    for ((k, _bx, _bz), mats) in drained {
        if let Some(Some(mesh)) = g.cache.get(&k) {
            if story_hard_cover(k) {
                cover_aabbs.extend(mats.iter().map(|model| transformed_aabb(mesh, *model)));
            }
            if matches!(k, Kit::Pine | Kit::Pine2) {
                tree_batches.push((mesh.clone(), mats));
            } else {
                batches.push((mesh.clone(), mats));
            }
        }
    }
    println!(
        "[worldgen/story] act {} '{}': {} placements in {} deterministic instanced buckets{}",
        act.id,
        act.name,
        g.placements,
        batches.len() + tree_batches.len(),
        if g.missing > 0 {
            format!(", {} optional assets missing/fell back", g.missing)
        } else {
            String::new()
        },
    );
    GenWorld {
        batches,
        tree_batches,
        cover_aabbs,
        placements: g.placements,
    }
}

/// Generate the act-scale district for `act`. `exclude_r` keeps a clear circle around the origin
/// where the hand-tuned combat block (city::generate + dressing) already lives — the district
/// surrounds it rather than stacking on top.
pub fn generate(act: &crate::world::ActMap, hf: &Heightfield, exclude_r: f32) -> GenWorld {
    let mut g = Gen {
        hf,
        seed: act.seed ^ 0x00D1_57C1, // distinct stream from city.rs (same act seed family)
        cache: HashMap::new(),
        buckets: HashMap::new(),
        placements: 0,
        missing: 0,
    };
    let r = act.ruination;

    // 1. THE CITY DISTRICT: a dense shelled grid centred east of the origin (the escape's start),
    //    thinning raggedly at its edge. Blocks inside the exclusion circle are skipped.
    for bj in -7i32..=7 {
        for bi in -7i32..=7 {
            let cx = 600.0 + bi as f32 * PITCH;
            let cz = 100.0 + bj as f32 * PITCH;
            if (cx * cx + cz * cz).sqrt() < exclude_r {
                continue;
            }
            let bid = ((bj as u32 & 0xffff).wrapping_mul(73_856_093))
                ^ ((bi as u32 & 0xffff).wrapping_mul(19_349_663));
            let edge = bi.abs().max(bj.abs()) as f32 / 7.0;
            if g.rnd(bid, 1) < 0.25 * edge {
                continue;
            }
            g.building(cx, cz, bid, (r + 0.15 * edge).min(1.0));
        }
    }
    // 2. WEST FRINGE: villages + pines thinning toward the goal.
    for k in 0..60u32 {
        let x = -600.0 - hash01(g.seed ^ k.wrapping_mul(7919)) * 1400.0;
        let z = (hash01(g.seed ^ k.wrapping_mul(104_729)) - 0.5) * 3600.0;
        let v = hash01(g.seed ^ k.wrapping_mul(65_537));
        if v < 0.30 {
            g.building(x, z, 0xB00 + k, r * 0.6);
        } else if v < 0.45 {
            let gy = g.ground(x, z) - 0.2;
            g.place(
                Kit::Shed,
                x,
                gy,
                z,
                hash01(k * 31) * std::f32::consts::TAU,
                1.0,
            );
        } else {
            let gy = g.ground(x, z) - 0.1;
            let kind = if v < 0.8 { Kit::Pine } else { Kit::Pine2 };
            g.place(
                kind,
                x,
                gy,
                z,
                hash01(k * 17) * std::f32::consts::TAU,
                0.8 + hash01(k * 13) * 0.7,
            );
        }
    }
    // 3. A couple of landmark-adjacent big volumes (warehouse district anchor + garage row).
    let gy = g.ground(880.0, 1080.0) - 0.3;
    g.place(Kit::Warehouse, 880.0, gy, 1080.0, 0.0, 2.2);
    let gy2 = g.ground(-880.0, -320.0) - 0.3;
    g.place(
        Kit::Garage,
        -880.0,
        gy2,
        -320.0,
        std::f32::consts::FRAC_PI_2,
        1.2,
    );
    // 4. THE BUNKER, under the district's south-west edge.
    g.bunker(320.0, -420.0);

    // Drain buckets → (mesh, transforms) batches. Pines drain into their own list — the game
    // registers those through the destructible-trees subsystem instead of raw add_instanced.
    let mut batches = Vec::new();
    let mut tree_batches = Vec::new();
    let mut per_kit: HashMap<Kit, usize> = HashMap::new();
    for ((k, _bx, _bz), mats) in g.buckets.drain() {
        *per_kit.entry(k).or_default() += mats.len();
        if let Some(Some(mesh)) = g.cache.get(&k) {
            if matches!(k, Kit::Pine | Kit::Pine2) {
                tree_batches.push((mesh.clone(), mats));
            } else {
                batches.push((mesh.clone(), mats));
            }
        }
    }
    println!(
        "[worldgen] act {} '{}': {} placements in {} instanced batches ({} unique modules{})",
        act.id,
        act.name,
        g.placements,
        batches.len(),
        per_kit.len(),
        if g.missing > 0 {
            format!(", {} MISSING", g.missing)
        } else {
            String::new()
        }
    );
    GenWorld {
        batches,
        tree_batches,
        cover_aabbs: Vec::new(),
        placements: g.placements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pizza_worldgen_assets_installed() -> bool {
        if crate::resource::is_file(path_of(Kit::Wall)) {
            true
        } else {
            eprintln!("skipping: untracked Pizza worldgen assets are not installed");
            false
        }
    }

    fn flat_hf() -> Heightfield {
        Heightfield {
            res: 16,
            spacing: 400.0,
            heights: vec![0.0; 256],
            min_h: 0.0,
            max_h: 0.0,
        }
    }

    #[test]
    fn generates_a_dense_deterministic_district() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let act = crate::world::act(1);
        let a = generate(&act, &hf, 250.0);
        let b = generate(&act, &hf, 250.0);
        assert!(
            a.placements > 3000,
            "act-scale district should be thousands of placements (got {})",
            a.placements
        );
        assert_eq!(a.placements, b.placements, "same seed → identical world");
        // Bucketing keeps batches cullable: many batches, none holding the whole world.
        assert!(
            a.batches.len() > 30,
            "expected spatially bucketed batches (got {})",
            a.batches.len()
        );
        let biggest = a.batches.iter().map(|(_, m)| m.len()).max().unwrap_or(0);
        assert!(
            biggest < a.placements / 2,
            "no single batch should hold most of the world"
        );
        // The exclusion circle stays clear (the hand-tuned combat block lives there).
        for (_, mats) in &a.batches {
            for m in mats {
                let p = m.transform_point3(glam::Vec3::ZERO);
                let horiz = (p.x * p.x + p.z * p.z).sqrt();
                assert!(
                    horiz > 230.0 || p.y < -1.0,
                    "placement at {p:?} inside the exclusion circle"
                );
            }
        }
    }

    #[test]
    fn story_overlay_is_always_additive_bucketed_and_act_specific() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let exclude_r = 320.0;
        let mut counts = Vec::new();
        for id in 1..=3 {
            let act = crate::world::act(id);
            let w = generate_story_overlay(&act, &hf, exclude_r);
            assert!(
                w.placements > 300,
                "act {id} should add hundreds of human-scale story props (got {})",
                w.placements
            );
            assert!(
                w.batches.len() > 20,
                "act {id} story props should remain spatially bucketed (got {})",
                w.batches.len()
            );
            assert!(
                !w.cover_aabbs.is_empty(),
                "act {id} should register its wrecks and barricades as hard cover"
            );
            assert!(w
                .cover_aabbs
                .iter()
                .all(|(min, max)| { min.is_finite() && max.is_finite() && max.cmpgt(*min).any() }));
            let biggest = w
                .batches
                .iter()
                .map(|(_, mats)| mats.len())
                .max()
                .unwrap_or(0);
            assert!(
                biggest < w.placements / 3,
                "act {id} unexpectedly collapsed most story props into one batch"
            );
            for (_, mats) in &w.batches {
                for m in mats {
                    let p = m.transform_point3(Vec3::ZERO);
                    assert!(
                        p.x * p.x + p.z * p.z >= (exclude_r - 0.1) * (exclude_r - 0.1),
                        "act {id} story placement {p:?} invaded the protected combat core"
                    );
                }
            }
            counts.push(w.placements);
        }
        assert!(
            counts[0] != counts[1] || counts[1] != counts[2],
            "acts should not receive a copy-pasted overlay: {counts:?}"
        );

        let arena = generate_story_overlay(&crate::world::act(0), &hf, exclude_r);
        assert_eq!(
            arena.placements, 0,
            "training arena stays free of campaign dioramas"
        );
        assert!(arena.batches.is_empty() && arena.tree_batches.is_empty());
    }

    #[test]
    fn story_overlay_transform_stream_is_deterministic() {
        if !pizza_worldgen_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let act = crate::world::act(1);
        let a = generate_story_overlay(&act, &hf, 320.0);
        let b = generate_story_overlay(&act, &hf, 320.0);
        assert_eq!(a.placements, b.placements);
        assert_eq!(a.batches.len(), b.batches.len());
        for ((_, am), (_, bm)) in a.batches.iter().zip(&b.batches) {
            assert_eq!(am.len(), bm.len());
            for (ax, bx) in am.iter().zip(bm) {
                assert_eq!(
                    ax.to_cols_array(),
                    bx.to_cols_array(),
                    "same act seed must produce a byte-identical transform stream"
                );
            }
        }
    }
}
