//! Parametric ruined-Stalingrad city GENERATOR. Assembles modular building pieces (PSX Mega Pack II
//! v1.8) into a deterministic grid of shelled-out Soviet city blocks and hands the caller a flat
//! list of already-world-transformed, terrain-grounded meshes. The scene builder (game.rs::build)
//! calls `city::generate(...)` and extends its scene Vec with the result — same load-once/clone-per-
//! placement pattern as the hand-placed dressing already in game.rs.
//!
//! Geometry facts, PROBED from the real GLBs (not guessed): `wall_hr_1.glb` is exactly 4.00 m wide
//! (X), 3.00 m tall (Y), 0.40 m thick (Z), origin centred in X, base at Y=0, wall-plane along X.
//! `floor_ceiling_hr_1.glb` is a 4×4 m slab. So the module grid cell is 4.0 m and a storey is 3.0 m.
//! (The generator re-measures these at runtime from the loaded wall/floor and logs them; the consts
//! below are only the fallback if the asset is missing.)
//!
//! Determinism: no `rand` crate — every choice comes from an integer hash of (seed, block-id, salt)
//! that mirrors the `hash01` counter hashing used in game.rs, with an added avalanche so the many
//! multi-dimensional keys decorrelate. Same seed → byte-identical city.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};

use crate::gltf_load::CpuMesh;
use crate::prims::box_mesh;
use crate::terrain::Heightfield;

// ── Module kit paths (PSX Mega Pack II v1.8) ─────────────────────────────────────────────────────
macro_rules! kit {
    ($p:literal) => {
        concat!(
            "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/",
            $p
        )
    };
}
macro_rules! mega3 {
    ($p:literal) => {
        concat!(
            "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/",
            $p
        )
    };
}
const WALL: &str = kit!("Modular Structures/wall_hr_1.glb");
const WALL_WINDOW: &str = concat!(
    "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/",
    "Modular Structures/window_1_single_frame_only.glb"
);
// The verified frame is 4.06×3.00×0.20 m; normalize only its local width to the 4 m city grid.
const WALL_WINDOW_X_SCALE: f32 = 0.985_221_7;
const WALL_HOLE: &str = kit!("Modular Structures/wall_hr_1_hole_1.glb"); // shell-holed variant
const DOORWAY: &str = kit!("Modular Structures/doorway_hr_1.glb");
const WINDOW: &str = kit!("Modular Structures/window_hr_1.glb");
const FLOOR: &str = kit!("Modular Structures/floor_ceiling_hr_1.glb");
const ROOF_PITCH: &str = kit!("Modular Structures/roof_hr_3_middle_angle.glb");
const ROOF_FLAT: &str = kit!("Modular Structures/roof_mh_4_a_1.glb"); // 8×4 (two-cell) flat roof
const PILLAR: &str = kit!("Modular Structures/pillar_hr_2.glb");
const PILLAR_BROKEN: &str = kit!("Modular Structures/pillar_hr_1_broken.glb");
const RUBBLE_FIELD: &str = kit!("Debris & Misc/debris_bricks_mx_1.glb"); // flat scattered brick field
const RUBBLE_STACK: &str = kit!("Debris & Misc/bricks_stacked_mx_1.glb"); // small toppled brick stack
const BRICK: &str = kit!("Debris & Misc/brick_mx_1.glb"); // single brick
const FURN_BED: &str = mega3!("Furniture/bed_1.glb");
const FURN_TABLE: &str = mega3!("Furniture/coffee_table_1.glb");
const FURN_CHAIR: &str = mega3!("Furniture/chair_mp_1.glb");
const FURN_CABINET: &str = mega3!("Furniture/bedside_table_1.glb");

// ── Layout tuning (metres) ───────────────────────────────────────────────────────────────────────
/// Street gap between blocks — wide enough to drive a Tiger down (hull ~3.7 m + slop).
pub const STREET_WIDTH: f32 = 10.0;
const MIN_FC: i32 = 3; // smallest building footprint, in grid cells per side
const MAX_FC: i32 = 5; // largest; also sets the uniform block pitch so the street grid stays regular
const DEFAULT_CELL: f32 = 4.0; // fallback module width if wall_hr_1 fails to load
const DEFAULT_WALL_H: f32 = 3.0; // fallback storey height
const BUILDING_SINK: f32 = 0.3; // buildings sink so they don't float on a sloped DEM
const RUBBLE_SINK: f32 = 0.1; // rubble beds slightly into the snow

/// A single placed city piece: an already world-transformed, terrain-grounded mesh ready to push
/// straight into the renderer's scene Vec.
pub struct CityPiece {
    pub mesh: CpuMesh,
    /// Optional exact structural proxies for visibly open modules. `None` means the ordinary mesh
    /// AABB contract; a frame-only window supplies jambs, sill, and header so gameplay never turns
    /// its aperture back into an invisible sealed wall.
    pub structure_proxies: Option<Vec<(Vec3, Vec3)>>,
    /// `Some(intensity)` marks this as a NIGHT-lit window pane or streetlamp head: the scene builder
    /// flags its renderer slot as an emissive window (the shader keeps the mesh's OWN colour and
    /// scales it by the `night` uniform) and only shows it at Nacht. `None` = ordinary structural /
    /// rubble piece. The glow COLOUR rides in the mesh's own 1×1 tint (warm-yellow / cold-blue / teal
    /// windows, warm lamp heads).
    pub window: Option<f32>,
}

/// Parameters for one generated city. `center` packs the world (x, z) of the grid centre in its
/// (x, y) — the engine is Y-up so there's no city-plane "y" to specify.
pub struct CityParams {
    pub center: Vec2,
    pub blocks_x: i32,
    pub blocks_z: i32,
    pub seed: u32,
    pub ruination: f32, // 0 = pristine, 1 = utterly shelled
}

/// Live-city generation result. The ordinary modular buildings have guaranteed open entrances,
/// ground-floor room partitions and furniture. They surround one deliberately empty city block
/// where `building_runtime` installs the validated enterable/destructible vertical slice.
pub struct LiveCity {
    pub pieces: Vec<CityPiece>,
    pub featured_building_center: Vec2,
}

impl Default for CityParams {
    fn default() -> Self {
        Self {
            center: Vec2::ZERO,
            blocks_x: 3,
            blocks_z: 3,
            seed: 0x5751_1917,
            ruination: 0.6,
        }
    }
}

/// Deterministic [0,1) hash of a counter. Keeps game.rs's recognisable core
/// (`n.wrapping_mul(2654435761) >> 8 & 0xffff`) but adds an avalanche pass so the many combined
/// (seed, block, salt) keys this generator feeds it don't produce visibly correlated streaks.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

/// Build the city. Returns a flat list of grounded, world-space pieces; the scene builder just does
/// `for p in city::generate(&params, &hf) { scene.push(p.mesh); }`.
pub fn generate(params: &CityParams, hf: &Heightfield) -> Vec<CityPiece> {
    generate_internal(params, hf, false).0
}

/// Generate the live city while reserving its southern-centre block for the compiled building.
/// Keeping the reservation here prevents a random shell or the former opaque landmark path from
/// occupying the same footprint.
pub fn generate_live(params: &CityParams, hf: &Heightfield) -> LiveCity {
    let (pieces, featured_building_center) = generate_internal(params, hf, true);
    LiveCity {
        pieces,
        featured_building_center,
    }
}

fn generate_internal(
    params: &CityParams,
    hf: &Heightfield,
    reserve_feature: bool,
) -> (Vec<CityPiece>, Vec2) {
    let mut ctx = Ctx {
        hf,
        seed: params.seed,
        cache: HashMap::new(),
        out: Vec::new(),
        missing: 0,
    };

    // Probe the real module size from the loaded wall/floor (fall back to the catalogue guess).
    let (cell, wall_h) = ctx.module_dims();
    let pitch = MAX_FC as f32 * cell + STREET_WIDTH; // uniform block spacing → regular street grid
    let r = params.ruination.clamp(0.0, 1.0);

    let cx0 = params.center.x;
    let cz0 = params.center.y;
    let feature_bi = params.blocks_x.max(1) / 2;
    let feature_bj = 0;
    let mut featured_building_center = params.center;
    for bj in 0..params.blocks_z.max(0) {
        for bi in 0..params.blocks_x.max(0) {
            let cx = cx0 + (bi as f32 - (params.blocks_x - 1) as f32 * 0.5) * pitch;
            let cz = cz0 + (bj as f32 - (params.blocks_z - 1) as f32 * 0.5) * pitch;
            // A well-mixed per-block id so neighbouring blocks look nothing alike.
            let id = (bj as u32).wrapping_mul(73_856_093) ^ (bi as u32).wrapping_mul(19_349_663);
            if reserve_feature && bi == feature_bi && bj == feature_bj {
                featured_building_center = Vec2::new(cx, cz);
            } else {
                ctx.block(cx, cz, id, cell, wall_h, r);
            }
            // A streetlamp at this block's SW street intersection — a regular lamp grid down the
            // streets whose warm heads halo and bloom at night.
            ctx.street_lamp(cx - pitch * 0.5, cz - pitch * 0.5);
        }
    }

    println!(
        "[panzerrechnersystem] city: {} pieces, module {:.2}m×{:.2}m (cell×storey), {}×{} blocks, seed {:#x}, ruination {:.2}{}",
        ctx.out.len(),
        cell,
        wall_h,
        params.blocks_x,
        params.blocks_z,
        params.seed,
        r,
        if ctx.missing > 0 { format!(", {} asset(s) MISSING", ctx.missing) } else { String::new() },
    );
    (ctx.out, featured_building_center)
}

struct Ctx<'a> {
    hf: &'a Heightfield,
    seed: u32,
    cache: HashMap<&'static str, Option<CpuMesh>>,
    out: Vec<CityPiece>,
    missing: usize,
}

impl Ctx<'_> {
    /// Deterministic [0,1) draw for (block id, salt). Distinct salts give independent decisions.
    fn rnd(&self, id: u32, salt: u32) -> f32 {
        hash01(self.seed ^ id.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA77))
    }

    /// Ensure `path` is cached; returns whether it loaded. A missing/misnamed asset is logged once
    /// and then skipped everywhere it's referenced (never a crash).
    fn load(&mut self, path: &'static str) -> bool {
        if !self.cache.contains_key(path) {
            let m = crate::gltf_load::load_glb(path).ok();
            if m.is_none() {
                self.missing += 1;
                println!("[panzerrechnersystem] city: MISSING asset {path}");
            }
            self.cache.insert(path, m);
        }
        self.cache.get(path).map(Option::is_some).unwrap_or(false)
    }

    /// Clone the cached mesh, bake translate·rotateY·scale, and push it. No-op if the asset is
    /// missing.
    fn place(&mut self, path: &'static str, x: f32, y: f32, z: f32, rot: f32, scale: f32) {
        self.place_scaled(path, x, y, z, rot, Vec3::splat(scale));
    }

    /// Non-uniform placement is reserved for source modules with a measured grid-normalization
    /// contract. Rotation precedes local scale so facade width remains correct on every side.
    fn place_scaled(&mut self, path: &'static str, x: f32, y: f32, z: f32, rot: f32, scale: Vec3) {
        if !self.load(path) {
            return;
        }
        let mesh = match self.cache.get(path) {
            Some(Some(m)) => m.clone(),
            _ => return,
        };
        let xf = Mat4::from_translation(Vec3::new(x, y, z))
            * Mat4::from_rotation_y(rot)
            * Mat4::from_scale(scale);
        let structure_proxies = if path == WALL_WINDOW {
            Some(window_frame_structure_proxies(xf))
        } else if path == DOORWAY {
            Some(doorway_structure_proxies(xf))
        } else {
            None
        };
        self.out.push(CityPiece {
            mesh: mesh.transformed(xf),
            structure_proxies,
            window: None,
        });
    }

    /// Push a NIGHT-lit emissive piece (window pane / lamp head). The glow colour is baked into the
    /// mesh's 1×1 tint; `intensity` drives how hard it blooms. Off at Tag, glowing at Nacht.
    fn emit_window(&mut self, mesh: CpuMesh, intensity: f32) {
        self.out.push(CityPiece {
            mesh,
            structure_proxies: None,
            window: Some(intensity),
        });
    }

    /// A lit window pane proud of a facade wall segment. Deterministically lit (~half the panes) with
    /// a hashed colour, so a building reads as a mosaic of warm-yellow / cold-blue / teal lit windows
    /// against the dark — the rest of the facade stays the module's dark glass (the unlit half).
    /// `outward` is the facade's outward unit normal (±X / ±Z); the pane sits just proud of the wall.
    fn facade_window(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        wall_h: f32,
        outward: Vec3,
        id: u32,
        key: u32,
    ) {
        if self.rnd(id, 1000 + key) > 0.55 {
            return; // this pane is dark (unlit flat) — leave the module's own glass
        }
        let color = window_color(self.rnd(id, 1100 + key));
        let intensity = 4.5 + 3.2 * self.rnd(id, 1200 + key); // varied brightness → the facade twinkles
        let cy = y + wall_h * 0.55; // window band a little above mid-storey
        let center = Vec3::new(x, cy, z) + outward * 0.30; // just proud of the wall's outer face
        self.emit_window(
            window_box(center, outward, 0.62, 0.5, 0.06, color),
            intensity,
        );
    }

    /// A streetlamp: a slim dark post grounded on the terrain + a warm emissive head that halos and
    /// blooms at night (dark by day). Placed along the streets by `generate`.
    fn street_lamp(&mut self, x: f32, z: f32) {
        let gy = self.hf.height_at(x, z);
        let h = 4.2;
        let post = box_mesh(
            Vec3::new(x - 0.09, gy, z - 0.09),
            Vec3::new(x + 0.09, gy + h, z + 0.09),
            [38, 36, 32],
        );
        self.out.push(CityPiece {
            mesh: post,
            structure_proxies: None,
            window: None,
        });
        let head = box_mesh(
            Vec3::new(x - 0.30, gy + h - 0.22, z - 0.30),
            Vec3::new(x + 0.30, gy + h + 0.18, z + 0.30),
            [255, 205, 130], // warm sodium-lamp glow
        );
        self.emit_window(head, 7.0);
    }

    /// Terrain height at (x,z) for a loose ground prop, bedded slightly into the snow.
    fn ground(&self, x: f32, z: f32) -> f32 {
        self.hf.height_at(x, z) - RUBBLE_SINK
    }

    /// Measure the true module cell width / storey height from the loaded wall (fall back to the
    /// catalogue guess if it's missing).
    fn module_dims(&mut self) -> (f32, f32) {
        if self.load(WALL) {
            if let Some(Some(m)) = self.cache.get(WALL) {
                let cell = m.max.x - m.min.x;
                let wall_h = m.max.y - m.min.y;
                if cell > 0.5 && wall_h > 0.5 {
                    return (cell, wall_h);
                }
            }
        }
        (DEFAULT_CELL, DEFAULT_WALL_H)
    }

    /// Generate one city block as an assembled ruined modular building. Opaque whole-building
    /// landmarks were retired: they could not satisfy the enterable-room contract.
    fn block(&mut self, cx: f32, cz: f32, id: u32, cell: f32, wall_h: f32, r: f32) {
        // Ground the whole building rigidly off its centre height (minus sink) so its wall ring
        // reads as one structure sitting on the ground, rather than each wall bobbing on the slope.
        let base = self.hf.height_at(cx, cz) - BUILDING_SINK;

        let fc_x = MIN_FC + (self.rnd(id, 2) * (MAX_FC - MIN_FC + 1) as f32) as i32;
        let fc_z = MIN_FC + (self.rnd(id, 3) * (MAX_FC - MIN_FC + 1) as f32) as i32;
        let fc_x = fc_x.clamp(MIN_FC, MAX_FC);
        let fc_z = fc_z.clamp(MIN_FC, MAX_FC);
        self.building(cx, cz, base, fc_x, fc_z, id, cell, wall_h, r);
    }

    /// Assemble an enterable modular building: perimeter walls (with ruination variants +
    /// blown-open gaps), one guaranteed exterior doorway, a ground-floor two-room layout with
    /// furniture, corner pillars, floor slabs, an optional roof, and rubble at the base of damaged
    /// walls and in the surrounding street.
    #[allow(clippy::too_many_arguments)]
    fn building(
        &mut self,
        cx: f32,
        cz: f32,
        base: f32,
        fc_x: i32,
        fc_z: i32,
        id: u32,
        cell: f32,
        wall_h: f32,
        r: f32,
    ) {
        let hx = fc_x as f32 * cell * 0.5;
        let hz = fc_z as f32 * cell * 0.5;
        // Keep the exterior entrance and the interior-room doorway on one obvious traversal line.
        // Unlike the incidental random doorway variants, these two openings survive every seed and
        // ruination level, so no ordinary block can become a sealed decorative shell.
        let entrance_i = (fc_x - 1) / 2;

        // Intact-ish buildings occasionally get a second storey; heavy ruination keeps them low.
        let storeys = if self.rnd(id, 4) < (1.0 - r) * 0.35 {
            2
        } else {
            1
        };

        for s in 0..storeys {
            let y = base + s as f32 * wall_h;
            let ground = s == 0;

            // South (−Z) and north (+Z) edges: walls run along X (rot 0). A surviving/windowed wall
            // gets a chance at a lit pane proud of its outward face (−Z south, +Z north).
            for i in 0..fc_x {
                let wx = cx - hx + (i as f32 + 0.5) * cell;
                let ik = i as u32;
                let k_s = edge_key(s, 0, ik);
                let k_n = edge_key(s, 1, ik);
                if ground && i == entrance_i {
                    self.place(DOORWAY, wx, y, cz - hz, 0.0, 1.0);
                } else if self.wall_seg(wx, y, cz - hz, 0.0, id, k_s, r, ground) {
                    self.facade_window(wx, y, cz - hz, wall_h, Vec3::NEG_Z, id, k_s);
                }
                if self.wall_seg(wx, y, cz + hz, 0.0, id, k_n, r, ground) {
                    self.facade_window(wx, y, cz + hz, wall_h, Vec3::Z, id, k_n);
                }
            }
            // West (−X) and east (+X) edges: walls run along Z (rot 90°); panes face −X / +X.
            for j in 0..fc_z {
                let wz = cz - hz + (j as f32 + 0.5) * cell;
                let jk = j as u32;
                let k_w = edge_key(s, 2, jk);
                let k_e = edge_key(s, 3, jk);
                if self.wall_seg(
                    cx - hx,
                    y,
                    wz,
                    std::f32::consts::FRAC_PI_2,
                    id,
                    k_w,
                    r,
                    ground,
                ) {
                    self.facade_window(cx - hx, y, wz, wall_h, Vec3::NEG_X, id, k_w);
                }
                if self.wall_seg(
                    cx + hx,
                    y,
                    wz,
                    std::f32::consts::FRAC_PI_2,
                    id,
                    k_e,
                    r,
                    ground,
                ) {
                    self.facade_window(cx + hx, y, wz, wall_h, Vec3::X, id, k_e);
                }
            }

            // Corner pillars (some toppled/broken/gone under ruination).
            let corners = [
                (cx - hx, cz - hz),
                (cx + hx, cz - hz),
                (cx - hx, cz + hz),
                (cx + hx, cz + hz),
            ];
            for (k, (px, pz)) in corners.iter().enumerate() {
                self.pillar(*px, y, *pz, id, s as u32 * 4 + k as u32, r);
            }

            // Floor slab under this storey.
            for gi in 0..fc_x {
                for gj in 0..fc_z {
                    let fx = cx - hx + (gi as f32 + 0.5) * cell;
                    let fz = cz - hz + (gj as f32 + 0.5) * cell;
                    self.place(FLOOR, fx, y, fz, 0.0, 1.0);
                }
            }

            if ground {
                self.ground_floor_interior(cx, cz, base, fc_x, fc_z, entrance_i, cell, hx, hz, id);
            }
        }

        // Roof over the top storey — but Stalingrad's rooflines are mostly gone, so high ruination
        // usually leaves the building open to the sky.
        let top_y = base + storeys as f32 * wall_h;
        if self.rnd(id, 9) > 0.25 + 0.6 * r {
            let flat = self.rnd(id, 8) < 0.4;
            for gi in 0..fc_x {
                for gj in 0..fc_z {
                    let fx = cx - hx + (gi as f32 + 0.5) * cell;
                    let fz = cz - hz + (gj as f32 + 0.5) * cell;
                    if flat {
                        // The 8×4 flat roof spans two cells in X; place on even columns only.
                        if gi % 2 == 0 {
                            self.place(ROOF_FLAT, fx + cell * 0.5, top_y - 0.05, fz, 0.0, 1.0);
                        }
                    } else {
                        self.place(ROOF_PITCH, fx, top_y - 0.05, fz, 0.0, 1.0);
                    }
                }
            }
        }

        // Rubble in the surrounding street / at the footprint edge.
        self.scatter_rubble(cx, cz, hx.max(hz), id, r);
    }

    /// Divide the ground floor into two traversable rooms and furnish both sides without touching
    /// the straight doorway-to-doorway circulation lane. These props are deliberately below the
    /// city's structural-height threshold: they add readable inhabited interiors without turning
    /// their union bounds into phantom tank/infantry walls.
    #[allow(clippy::too_many_arguments)]
    fn ground_floor_interior(
        &mut self,
        cx: f32,
        cz: f32,
        base: f32,
        fc_x: i32,
        fc_z: i32,
        entrance_i: i32,
        cell: f32,
        hx: f32,
        hz: f32,
        id: u32,
    ) {
        // Split after the southern half's final cell. Even the minimum 3-cell footprint leaves one
        // 4 m-deep entry room and an 8 m-deep rear room.
        let southern_rows = (fc_z / 2).max(1);
        let partition_z = cz - hz + southern_rows as f32 * cell;
        for i in 0..fc_x {
            let x = cx - hx + (i as f32 + 0.5) * cell;
            self.place(
                if i == entrance_i { DOORWAY } else { WALL },
                x,
                base,
                partition_z,
                0.0,
                1.0,
            );
        }

        // Mirror the domestic/sitting clusters per building while keeping both clusters in the
        // corner cells, well clear of the aligned central entrance route.
        let rear_side = if self.rnd(id, 1600) < 0.5 { -1.0 } else { 1.0 };
        let rear_x = cx + rear_side * (hx - cell * 0.5);
        let rear_z = partition_z + cell * 0.5;
        self.place(
            FURN_BED,
            rear_x,
            base + 0.05,
            rear_z,
            if rear_side < 0.0 {
                0.0
            } else {
                std::f32::consts::PI
            },
            1.0,
        );
        self.place(
            FURN_CABINET,
            rear_x - rear_side * 1.75,
            base + 0.05,
            rear_z,
            0.0,
            1.0,
        );

        let entry_side = -rear_side;
        let entry_x = cx + entry_side * (hx - cell * 0.5);
        let entry_z = cz - hz + cell * 0.5;
        let sitting_rot = if self.rnd(id, 1601) < 0.5 {
            0.0
        } else {
            std::f32::consts::FRAC_PI_2
        };
        self.place(FURN_TABLE, entry_x, base + 0.05, entry_z, sitting_rot, 1.0);
        self.place(
            FURN_CHAIR,
            entry_x - entry_side * 1.20,
            base + 0.05,
            entry_z,
            -entry_side * std::f32::consts::FRAC_PI_2,
            1.0,
        );
    }

    /// Place one perimeter wall segment: roll for a blown-open gap, else pick an intact / windowed /
    /// shell-holed / doorway variant. Damaged walls leave a brick pile at their base. Returns whether
    /// the segment left an intact or windowed facade that a lit night pane can sit on (false for a
    /// gap, doorway or shell-hole — no lit window over a breach).
    #[allow(clippy::too_many_arguments)]
    fn wall_seg(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        rot: f32,
        id: u32,
        key: u32,
        r: f32,
        ground: bool,
    ) -> bool {
        let v = self.rnd(id, 100 + key);
        // Blown-open gap: delete the wall, drop a toppled brick stack in the breach.
        let del_p = 0.08 + 0.30 * r;
        if v < del_p {
            self.place(
                RUBBLE_STACK,
                x,
                self.ground(x, z),
                z,
                self.rnd(id, 700 + key) * std::f32::consts::TAU,
                1.0,
            );
            return false;
        }

        // Variant selection among the surviving walls.
        let w = self.rnd(id, 200 + key);
        let path = if w < 0.25 * r {
            WALL_HOLE
        } else if ground && w < 0.25 * r + 0.10 {
            DOORWAY
        } else if w < 0.25 * r + 0.30 {
            if ground {
                WALL_WINDOW
            } else {
                WINDOW
            }
        } else {
            WALL
        };
        if path == WALL_WINDOW {
            self.place_scaled(path, x, y, z, rot, Vec3::new(WALL_WINDOW_X_SCALE, 1.0, 1.0));
        } else {
            self.place(path, x, y, z, rot, 1.0);
        }

        // Shell-holed walls spill brick at their feet.
        if path == WALL_HOLE {
            self.place(
                RUBBLE_FIELD,
                x,
                self.ground(x, z),
                z,
                self.rnd(id, 800 + key) * std::f32::consts::TAU,
                1.0,
            );
        }

        // A lit pane can glow behind an intact or windowed wall — not a hole, doorway or gap.
        path == WALL || path == WALL_WINDOW || path == WINDOW
    }

    /// Corner pillar: intact, broken (toppled stump), or gone entirely under ruination.
    fn pillar(&mut self, x: f32, y: f32, z: f32, id: u32, key: u32, r: f32) {
        let v = self.rnd(id, 300 + key);
        if v < 0.4 * r {
            if v < 0.15 * r {
                return; // blown away
            }
            self.place(
                PILLAR_BROKEN,
                x,
                y,
                z,
                self.rnd(id, 900 + key) * std::f32::consts::TAU,
                1.0,
            );
        } else {
            self.place(PILLAR, x, y, z, 0.0, 1.0);
        }
    }

    /// Scatter rubble in the street ring around a footprint of half-extent `rad`. Count and spread
    /// grow with ruination.
    fn scatter_rubble(&mut self, cx: f32, cz: f32, rad: f32, id: u32, r: f32) {
        let n = (1.0 + r * 5.0) as i32;
        for k in 0..n {
            let ku = k as u32;
            let a = self.rnd(id, 400 + ku) * std::f32::consts::TAU;
            let dist = rad + 1.0 + self.rnd(id, 450 + ku) * (STREET_WIDTH * 0.5);
            let x = cx + a.cos() * dist;
            let z = cz + a.sin() * dist;
            let path = match (self.rnd(id, 500 + ku) * 3.0) as i32 {
                0 => RUBBLE_FIELD,
                1 => RUBBLE_STACK,
                _ => BRICK,
            };
            let scale = 0.8 + self.rnd(id, 600 + ku) * 0.6;
            self.place(
                path,
                x,
                self.ground(x, z),
                z,
                self.rnd(id, 550 + ku) * std::f32::consts::TAU,
                scale,
            );
        }
    }
}

pub(crate) fn window_frame_structure_proxies(model: Mat4) -> Vec<(Vec3, Vec3)> {
    // Source-space bounds were measured from window_1_single_frame_only. Keep the actual clear
    // x=(-0.56,0.56), y=(0.90,2.50) aperture open after the deterministic X normalization/yaw.
    [
        (Vec3::new(-2.03, 0.0, -0.21), Vec3::new(-0.56, 3.0, 0.21)),
        (Vec3::new(0.56, 0.0, -0.21), Vec3::new(2.03, 3.0, 0.21)),
        (Vec3::new(-0.56, 0.0, -0.21), Vec3::new(0.56, 0.90, 0.21)),
        (Vec3::new(-0.56, 2.50, -0.21), Vec3::new(0.56, 3.0, 0.21)),
    ]
    .into_iter()
    .map(|(min, max)| transformed_aabb(model, min, max))
    .collect()
}

/// Exact doorway collision for the verified 4×3 m module. The visible 1.6×2.1 m opening stays
/// empty; only its jambs and header are physical. This must never fall back to the source mesh's
/// union AABB, which would make every apparently open entrance an invisible wall.
pub(crate) fn doorway_structure_proxies(model: Mat4) -> Vec<(Vec3, Vec3)> {
    [
        (Vec3::new(-2.0, 0.0, -0.21), Vec3::new(-0.80, 3.0, 0.21)),
        (Vec3::new(0.80, 0.0, -0.21), Vec3::new(2.0, 3.0, 0.21)),
        (Vec3::new(-0.80, 2.10, -0.21), Vec3::new(0.80, 3.0, 0.21)),
    ]
    .into_iter()
    .map(|(min, max)| transformed_aabb(model, min, max))
    .collect()
}

fn transformed_aabb(model: Mat4, min: Vec3, max: Vec3) -> (Vec3, Vec3) {
    let mut world_min = Vec3::splat(f32::INFINITY);
    let mut world_max = Vec3::splat(f32::NEG_INFINITY);
    for corner in 0..8 {
        let point = Vec3::new(
            if corner & 1 == 0 { min.x } else { max.x },
            if corner & 2 == 0 { min.y } else { max.y },
            if corner & 4 == 0 { min.z } else { max.z },
        );
        let point = model.transform_point3(point);
        world_min = world_min.min(point);
        world_max = world_max.max(point);
    }
    (world_min, world_max)
}

/// Pack (storey, edge, index) into a unique per-segment salt: storey∈0..2, edge∈0..4, index∈0..MAX_FC.
fn edge_key(storey: i32, edge: u32, idx: u32) -> u32 {
    ((storey as u32) * 4 + edge) * 8 + idx
}

/// The colour of a lit window pane, from a [0,1) hash: mostly warm-yellow domestic light, a scatter
/// of cold-blue (a television / fluorescent tube), some teal, and a few deep warm amber — the varied
/// mosaic of the locked reference. Returned as an sRGB tint baked into the pane's 1×1 texture; the
/// mesh shader decodes it to linear and drives it into HDR so it blooms at night.
fn window_color(r: f32) -> [u8; 3] {
    if r < 0.60 {
        [255, 196, 120] // warm yellow — the majority of lit flats
    } else if r < 0.80 {
        [120, 150, 255] // cold blue
    } else if r < 0.92 {
        [90, 220, 205] // teal
    } else {
        [255, 150, 85] // deep warm amber
    }
}

/// A thin axis-aligned box standing in for a glowing window pane, facing `outward` (a ±X or ±Z unit
/// normal). Thin along the facing axis, `half_w`×`half_h` across the wall.
fn window_box(
    center: Vec3,
    outward: Vec3,
    half_w: f32,
    half_h: f32,
    t: f32,
    color: [u8; 3],
) -> CpuMesh {
    let ext = if outward.x.abs() > 0.5 {
        Vec3::new(t, half_h, half_w) // wall faces ±X → thin in X, spans Z
    } else {
        Vec3::new(half_w, half_h, t) // wall faces ±Z → thin in Z, spans X
    };
    box_mesh(center - ext, center + ext, color)
}

/// Build a low COLLAPSED-RUBBLE heap mesh spanning the world footprint of a felled wall/facade
/// (`min`/`max` = the intact piece's world AABB) — a deterministic cluster of staggered, flat-shaded
/// low boxes rising from `min.y` so a knocked-down wall reads as a heap of masonry, not a slab.
/// Used by game.rs's destructibility system: pre-allocated hidden at the same footprint as each
/// destructible piece and revealed the instant the intact wall drops (the engine can't add meshes at
/// runtime). Reuses the same box primitive + counter-hash determinism as the rest of the generator
/// (no rand), one merged mesh with a cold grey-brown rubble tint so it clones/uploads cheaply.
pub fn rubble_heap(min: Vec3, max: Vec3, seed: u32) -> CpuMesh {
    let base = min.y;
    let cx = (min.x + max.x) * 0.5;
    let cz = (min.z + max.z) * 0.5;
    let fx = (max.x - min.x).max(0.6);
    let fz = (max.z - min.z).max(0.6);
    let along_x = fx >= fz; // the wall runs along its longer horizontal axis
    let run = if along_x { fx } else { fz }; // heap length along the wall's run
    let thick = (if along_x { fz } else { fx }).max(0.6) + 1.0; // spread a bit wider than the thin wall
    let tint = [96, 90, 82]; // cold grey-brown masonry
    const N: u32 = 7;
    let mut verts = Vec::new();
    let mut inds: Vec<u32> = Vec::new();
    let mut omin = Vec3::splat(f32::INFINITY);
    let mut omax = Vec3::splat(f32::NEG_INFINITY);
    for k in 0..N {
        let h = |s: u32| hash01(seed ^ k.wrapping_mul(0x9E37_79B9) ^ s.wrapping_mul(0x85EB_CA77));
        // Position each chunk along the run (−0.5..0.5 of the length) with a little jitter.
        let t = (k as f32 + 0.5) / N as f32 - 0.5;
        let along = t * run * 0.9 + (h(1) - 0.5) * 0.6;
        let across = (h(2) - 0.5) * thick * 0.5;
        let bw = 0.45 + h(3) * 0.85; // half-length along the run
        let bd = 0.35 + h(4) * (thick * 0.35); // half-depth across
        let bh = 0.30 + h(5) * 0.65; // heap height (all well below tank-centre height)
        let (dx, dz) = if along_x {
            (along, across)
        } else {
            (across, along)
        };
        let bmin = Vec3::new(cx + dx - bw, base - 0.10, cz + dz - bd);
        let bmax = Vec3::new(cx + dx + bw, base + bh, cz + dz + bd);
        let b = box_mesh(bmin, bmax, tint);
        let off = verts.len() as u32;
        verts.extend_from_slice(&b.vertices);
        inds.extend(b.indices.iter().map(|i| i + off));
        omin = omin.min(bmin);
        omax = omax.max(bmax);
    }
    CpuMesh::basic(
        verts,
        inds,
        Some(crate::prims::solid_color(tint)),
        omin,
        omax,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point_in_any(point: Vec3, proxies: &[(Vec3, Vec3)]) -> bool {
        proxies
            .iter()
            .any(|(min, max)| (point.cmpge(*min) & point.cmple(*max)).all())
    }

    fn pizza_city_assets_installed() -> bool {
        if crate::resource::is_file(WALL) {
            true
        } else {
            eprintln!("skipping: untracked Pizza city assets are not installed");
            false
        }
    }

    /// A flat stub heightfield at y=0 everywhere (bilinear of all-zero samples).
    fn flat_hf() -> Heightfield {
        Heightfield {
            res: 16,
            spacing: 50.0,
            heights: vec![0.0; 16 * 16],
            min_h: 0.0,
            max_h: 0.0,
        }
    }

    #[test]
    fn generates_plausible_city() {
        if !pizza_city_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let params = CityParams {
            center: Vec2::new(0.0, 0.0),
            blocks_x: 2,
            blocks_z: 2,
            seed: 0xC0FF_EE01,
            ruination: 0.6,
        };
        let pieces = generate(&params, &hf);

        // A handful of blocks should assemble into a substantial pile of modules.
        assert!(
            pieces.len() > 50,
            "expected a dense city (>50 pieces) for a {}×{} grid, got {}",
            params.blocks_x,
            params.blocks_z,
            pieces.len()
        );

        // No piece may land wildly outside the expected footprint. Bound = grid half-span (block
        // pitch) + a building half-width + street scatter reach + margin. cell≈4.0 here (real asset).
        let cell = DEFAULT_CELL;
        let pitch = MAX_FC as f32 * cell + STREET_WIDTH;
        let half_x = (params.blocks_x - 1) as f32 * 0.5 * pitch;
        let half_z = (params.blocks_z - 1) as f32 * 0.5 * pitch;
        let reach = MAX_FC as f32 * cell * 0.5 + STREET_WIDTH + 6.0;
        let max_x = params.center.x + half_x + reach;
        let min_x = params.center.x - half_x - reach;
        let max_z = params.center.y + half_z + reach;
        let min_z = params.center.y - half_z - reach;

        for (i, p) in pieces.iter().enumerate() {
            let c = p.mesh.center();
            assert!(
                c.x >= min_x && c.x <= max_x && c.z >= min_z && c.z <= max_z,
                "piece {i} centre {:?} outside city region x[{min_x},{max_x}] z[{min_z},{max_z}]",
                c
            );
            // Flat terrain at y=0: nothing should be far below ground or absurdly tall.
            assert!(
                c.y > -5.0 && c.y < 25.0,
                "piece {i} centre y={} implausible",
                c.y
            );
        }
    }

    #[test]
    fn deterministic_same_seed() {
        if !pizza_city_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let params = CityParams {
            center: Vec2::ZERO,
            blocks_x: 2,
            blocks_z: 2,
            seed: 42,
            ruination: 0.7,
        };
        let a = generate(&params, &hf);
        let b = generate(&params, &hf);
        assert_eq!(
            a.len(),
            b.len(),
            "same seed must produce the same piece count"
        );
    }

    #[test]
    fn shallow_window_frame_normalizes_to_the_city_grid_on_both_facade_axes() {
        if !crate::resource::is_file(WALL_WINDOW) {
            eprintln!("skipping: Pizza frame-only window is not installed");
            return;
        }
        let hf = flat_hf();
        let mut ctx = Ctx {
            hf: &hf,
            seed: 1,
            cache: HashMap::new(),
            out: Vec::new(),
            missing: 0,
        };
        ctx.place_scaled(
            WALL_WINDOW,
            0.0,
            0.0,
            0.0,
            0.0,
            Vec3::new(WALL_WINDOW_X_SCALE, 1.0, 1.0),
        );
        ctx.place_scaled(
            WALL_WINDOW,
            0.0,
            0.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            Vec3::new(WALL_WINDOW_X_SCALE, 1.0, 1.0),
        );
        assert_eq!(ctx.out.len(), 2);
        for (index, piece) in ctx.out.iter().enumerate() {
            let extent = piece.mesh.max - piece.mesh.min;
            assert!(
                (extent.y - 3.0).abs() < 0.01,
                "window {index} changed storey height: {extent:?}"
            );
            assert!(
                (extent.x.max(extent.z) - 4.0).abs() < 0.025,
                "window {index} missed the 4 m facade grid: {extent:?}"
            );
            assert!(
                extent.x.min(extent.z) < 0.22,
                "window {index} is not the shallow 0.20 m frame: {extent:?}"
            );
            let proxies = piece
                .structure_proxies
                .as_ref()
                .expect("frame-only window must carry exact gameplay proxies");
            assert_eq!(proxies.len(), 4);
            let aperture = Vec3::new(0.0, 1.6, 0.0);
            assert!(
                proxies
                    .iter()
                    .all(|(min, max)| !(aperture.cmpge(*min) & aperture.cmple(*max)).all()),
                "window {index} proxies invisibly sealed its visible aperture"
            );
        }
    }

    #[test]
    fn doorway_proxies_keep_the_visible_aperture_walkable_on_both_facade_axes() {
        for yaw in [0.0, std::f32::consts::FRAC_PI_2] {
            let model = Mat4::from_rotation_y(yaw);
            let proxies = doorway_structure_proxies(model);
            assert_eq!(proxies.len(), 3);

            let aperture = model.transform_point3(Vec3::new(0.0, 1.4, 0.0));
            let jamb = model.transform_point3(Vec3::new(-1.4, 1.4, 0.0));
            let header = model.transform_point3(Vec3::new(0.0, 2.7, 0.0));
            assert!(
                !point_in_any(aperture, &proxies),
                "yaw {yaw} invisibly sealed the open doorway"
            );
            assert!(point_in_any(jamb, &proxies), "yaw {yaw} lost its jamb");
            assert!(point_in_any(header, &proxies), "yaw {yaw} lost its header");

            let old = model.transform_point3(Vec3::new(0.0, 0.0, -1.0));
            let attempted = model.transform_point3(Vec3::new(0.0, 0.0, 1.0));
            let resolved =
                crate::building_runtime::resolve_walk_xz_against_bounds(old, attempted, &proxies);
            assert!(
                resolved.distance(attempted) < 0.01,
                "yaw {yaw} doorway stopped a player in its aperture: {resolved:?}"
            );
        }
    }

    #[test]
    fn every_ordinary_footprint_has_two_open_doorways_and_furniture() {
        if !pizza_city_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let mut ctx = Ctx {
            hf: &hf,
            seed: 0x1A7E_210B,
            cache: HashMap::new(),
            out: Vec::new(),
            missing: 0,
        };

        for fc_x in MIN_FC..=MAX_FC {
            for fc_z in MIN_FC..=MAX_FC {
                let before = ctx
                    .out
                    .iter()
                    .filter(|piece| {
                        piece
                            .structure_proxies
                            .as_ref()
                            .is_some_and(|boxes| boxes.len() == 3)
                    })
                    .count();
                let id = (fc_x as u32) << 16 | fc_z as u32;
                ctx.building(
                    0.0,
                    0.0,
                    0.0,
                    fc_x,
                    fc_z,
                    id,
                    DEFAULT_CELL,
                    DEFAULT_WALL_H,
                    1.0,
                );
                let after = ctx
                    .out
                    .iter()
                    .filter(|piece| {
                        piece
                            .structure_proxies
                            .as_ref()
                            .is_some_and(|boxes| boxes.len() == 3)
                    })
                    .count();
                assert!(
                    after >= before + 2,
                    "{fc_x}×{fc_z} footprint lacked its exterior/partition door pair"
                );
            }
        }

        for path in [FURN_BED, FURN_TABLE, FURN_CHAIR, FURN_CABINET] {
            let mesh = ctx
                .cache
                .get(path)
                .and_then(Option::as_ref)
                .unwrap_or_else(|| panic!("ground-floor furnishing was not placed: {path}"));
            assert!(
                mesh.max.y - mesh.min.y < 2.0,
                "furniture must stay below structural proxy height: {path}"
            );
        }
    }

    #[test]
    fn aligned_exterior_and_partition_doors_form_a_clear_ground_floor_route() {
        if !pizza_city_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let mut ctx = Ctx {
            hf: &hf,
            seed: 0xC1EA_4A7E,
            cache: HashMap::new(),
            out: Vec::new(),
            missing: 0,
        };
        let fc_x = MIN_FC;
        let fc_z = MIN_FC;
        ctx.building(
            0.0,
            0.0,
            0.0,
            fc_x,
            fc_z,
            7,
            DEFAULT_CELL,
            DEFAULT_WALL_H,
            0.0,
        );

        // Mirror game.rs's exact structural registration: low furniture/floors stay decorative;
        // doorway frames contribute their three exact boxes and other tall modules their AABB.
        let mut bounds = Vec::new();
        for piece in &ctx.out {
            let (min, max) = (piece.mesh.min, piece.mesh.max);
            let tall = piece.window.is_none()
                && max.y - min.y > 2.0
                && (max.x - min.x).max(max.z - min.z) > 1.5;
            if !tall {
                continue;
            }
            if let Some(proxies) = &piece.structure_proxies {
                bounds.extend(proxies.iter().copied());
            } else {
                bounds.push((min, max));
            }
        }

        let hz = fc_z as f32 * DEFAULT_CELL * 0.5;
        let old = Vec3::new(0.0, 0.0, -hz - 1.0);
        let attempted = Vec3::new(0.0, 0.0, 1.0);
        let resolved =
            crate::building_runtime::resolve_walk_xz_against_bounds(old, attempted, &bounds);
        assert!(
            resolved.distance(attempted) < 0.01,
            "aligned room route was blocked by hidden collision: {resolved:?}"
        );
    }

    #[test]
    fn live_city_deterministically_reserves_the_southern_centre_block() {
        if !pizza_city_assets_installed() {
            return;
        }
        let hf = flat_hf();
        let params = CityParams {
            center: Vec2::new(12.0, 40.0),
            blocks_x: 3,
            blocks_z: 3,
            seed: 0x51CE_0001,
            ruination: 0.5,
        };
        let full = generate(&params, &hf);
        let live_a = generate_live(&params, &hf);
        let live_b = generate_live(&params, &hf);
        let pitch = MAX_FC as f32 * DEFAULT_CELL + STREET_WIDTH;

        assert_eq!(
            live_a.featured_building_center,
            Vec2::new(params.center.x, params.center.y - pitch)
        );
        assert_eq!(
            live_a.featured_building_center,
            live_b.featured_building_center
        );
        assert_eq!(live_a.pieces.len(), live_b.pieces.len());
        assert!(
            live_a.pieces.len() < full.len(),
            "reserving a full authored block must remove its generated shell pieces"
        );
    }
}
