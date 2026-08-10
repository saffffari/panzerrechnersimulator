#!/usr/bin/env python3
"""gen_world.py — stage 1 of the world→Blender authoring pipeline (LIVE again as of 2026-07-02:
the homeland acts are built as a Blender-first collaboration — procedural base here, the user
fine-tunes in Blender, per kessel-worldgen-workflow. Act 1's runtime district still assembles
in-engine via src/worldgen.rs, which was ported from this file's city generator).

Generates a complete act map as a placement manifest. Act 1: real-DEM terrain window, a ruined
city district of kitbashed PSX-module buildings WITH walkable interiors, landmark blockouts, an
underground bunker, street scatter, spawn/goal markers. Act 2: the spring ODERBRUCH — polder
field grid with hedgerows and drainage ditches carved into the real Copernicus DEM, a N-S canal,
villages of intact pitched-roof houses, the leveled Manor, the Seelow-scarp forest, landmark
blockouts + the water-tower hero, spawn/goal markers. Deterministic: same seed → same world.

Stage 2 (`assemble_blend.py`, headless bpy) turns the manifest into a .blend the user fine-tunes
in Blender.

Usage:  python3 tools/worldgen/gen_world.py [act] [outdir]
Output: <outdir>/manifest.v1.json + heights.npy
"""
import json
import math
import os
import sys

import numpy as np

MANIFEST_SCHEMA = "kessel.act"
MANIFEST_VERSION = 1
MANIFEST_FILE = "manifest.v1.json"

# ── Act configs (mirrors src/world.rs) ────────────────────────────────────────────────────────────
ACTS = {
    1: dict(
        name="Die Stadt", half_extent=2200.0, west_offset=2000.0, seed=0x57511917, ruination=0.72,
        spawn=(1850.0, 250.0), goal=(-1950.0, -400.0),
        # Real winter Sentinel-2 of Volgograd (Jan 2024), warped to this exact window (UTM 38N,
        # centre 48.708 N 44.485776 E ± 2200 m) — the snow whiteout with the city grid + Volga.
        texture="assets/terrain/act1_rgb/act1_ground_hires.png",  # stage-1.5 detail-splat (make_ground_tex.py)
        color=(0.82, 0.86, 0.92, 1.0),
        terrain_res=1024, battle_sites=2, logistics_sites=1,
        overview=dict(pos=(1600.0, 420.0, -1150.0), target=(300.0, 10.0, 0.0)),
        landmarks=[
            ("GrainElevator", 900.0, 1100.0, "Getreidesilo"),
            ("Monument", 300.0, -150.0, "Springbrunnen"),
            ("Church", -400.0, 600.0, "Kasaner Kirche"),
            ("RailStation", -900.0, -300.0, "Hauptbahnhof"),
            ("Monument", -1600.0, 250.0, "Mamajew-Hügel"),
        ],
    ),
    # Act 2 — Die Heimkehr: the ODERBRUCH below the Seelow Heights, full-bloom spring.
    # Mirrors src/world.rs act(2); landmark NAMES are the hero-GLB slug contract (world::slug).
    # Terrain: pre-cropped 5.2 km UTM window of the Copernicus GLO-30 DEM (act2_window_dem.npy,
    # produced by gdalwarp from the tile in assets/terrain/act2/), spring Sentinel-2 TCI draped
    # on top in Blender (act2_window_tci.png, same window).
    2: dict(
        name="Die Heimkehr", half_extent=2600.0, seed=0x0A176B22, ruination=0.50,
        spawn=(2300.0, -250.0), goal=(-2250.0, 350.0),
        gen="spring",
        dem="assets/terrain/act2/act2_window_dem.npy",
        texture="assets/terrain/act2/act2_ground_hires.png",  # stage-1.5 detail-splat (make_ground_tex.py)
        tci_npy="assets/terrain/act2/act2_window_tci.npy",  # same raster as uint8 HxWx3 for classification
        color=(0.33, 0.42, 0.24, 1.0),  # spring green fallback where the imagery isn't shown
        terrain_res=1536, battle_sites=3, logistics_sites=2,
        overview=dict(pos=(1500.0, 420.0, -1250.0), target=(-300.0, 20.0, 250.0)),
        canal=True,
        landmarks=[
            ("Church", 1300.0, 500.0, "Village Church"),
            ("Windmill", 700.0, -600.0, "The Windmill"),
            ("Manor", 100.0, 200.0, "The Old House"),
            ("WaterTower", -800.0, -400.0, "Water Tower"),
            ("Bridge", -1500.0, 500.0, "Canal Bridge"),
        ],
    ),
    # Act 3 — Kameraden: HALBE, the Kessel von Halbe (docs/RESEARCH_ACTS.md: 52.1065 N 13.6963 E).
    # A small town in deep pine forest cut by firebreaks — the woods mask dominates, so the tree
    # sampling is throttled (woods_stride/woods_keep) to keep the .blend workable.
    3: dict(
        name="Kameraden", half_extent=2000.0, seed=0x2716E5A9, ruination=0.30,
        spawn=(1700.0, 200.0), goal=(-1650.0, -200.0),
        gen="spring",
        dem="assets/terrain/act3/act3_window_dem.npy",
        texture="assets/terrain/act3/act3_ground_hires.png",  # stage-1.5 detail-splat (make_ground_tex.py)
        tci_npy="assets/terrain/act3/act3_window_tci.npy",
        color=(0.24, 0.32, 0.18, 1.0),
        woods_stride=2, woods_keep=0.80, pine_frac=0.85,
        terrain_res=1280, battle_sites=4, logistics_sites=2,
        dark_forest=dict(v=60, var=45),
        gamma=0.95, saturation=1.25,  # keep the pine sea dark; act-2's grade washed it out
        overview=dict(pos=(1250.0, 380.0, 950.0), target=(-100.0, 45.0, -150.0)),
        landmarks=[
            ("Church", 1000.0, -300.0, "Town Church"),
            ("RailStation", 400.0, 400.0, "Rail Station"),
            ("FactoryChimney", -200.0, -500.0, "Brickworks"),
            ("WreckField", -900.0, 300.0, "The Blocked Highway"),
            ("Monument", -1500.0, -150.0, "Forest Cemetery"),
        ],
    ),
}

# The N-S drainage canal the Act-2 bridge crosses (engine x of its centreline) and its meander.
CANAL_X = -1500.0
CANAL_MEANDER = 40.0

# ── Asset kit (paths verified on disk) ────────────────────────────────────────────────────────────
MP2 = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/"
MP = "assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/"
NAT = "assets/pizza/_extracted/PSX Nature v1.7.3/PSX Nature (Tree branches separated)/Models/GLB (recommended)/"
FPS = "assets/pizza/_extracted/Modular Retro FPS Kit v1.5/Modular Retro FPS Kit/Models/other-formats/GLB/"

KIT = {
    "wall": MP2 + "Modular Structures/wall_hr_1.glb",
    "wall_win": MP2 + "Modular Structures/wall_hr_1_m1a_1.glb",
    "wall_hole": MP2 + "Modular Structures/wall_hr_1_hole_1.glb",
    "doorway": MP2 + "Modular Structures/doorway_hr_1.glb",
    "window": MP2 + "Modular Structures/window_hr_1.glb",
    "floor": MP2 + "Modular Structures/floor_ceiling_hr_1.glb",
    "floor_hole": MP2 + "Modular Structures/floor_ceiling_hr_2_hole.glb",
    "roof_pitch": MP2 + "Modular Structures/roof_hr_3_middle_angle.glb",
    "roof_flat": MP2 + "Modular Structures/roof_mh_4_a_1.glb",
    "pillar": MP2 + "Modular Structures/pillar_hr_2.glb",
    "pillar_broken": MP2 + "Modular Structures/pillar_hr_1_broken.glb",
    "stairs": MP + "Modular Structures/stairs_5_concrete.glb",  # (Mega Pack 1 carries the stairs)
    "rubble_field": MP2 + "Debris & Misc/debris_bricks_mx_1.glb",
    "rubble_stack": MP2 + "Debris & Misc/bricks_stacked_mx_1.glb",
    "brick": MP2 + "Debris & Misc/brick_mx_1.glb",
    "warehouse": MP2 + "Buildings/warehouse_mx_1.glb",
    "garage": MP2 + "Buildings/garage_hl_1.glb",
    "shed": MP2 + "Buildings/shed_ax_1.glb",
    "brick_bld": "assets/flavor/buildings/exports/BrickBuilding.glb",
    "industrial_bld": "assets/flavor/buildings/exports/IndustrialBuilding.glb",
    # interiors
    "furn_bed": MP + "Furniture/bed_1.glb",
    "furn_table": MP + "Furniture/coffee_table_1.glb",
    "furn_chair": MP + "Furniture/chair_mp_1.glb",
    "furn_cabinet": MP + "Furniture/bedside_table_1.glb",
    # nature (countryside fringe)
    "pine": NAT + "pine_tree_n_1.glb",
    "pine2": NAT + "pine_tree_n_2.glb",
    "stone": NAT + "stone_1.glb",
    # bunker (Retro FPS kit)
    "fps_wall": FPS + "wall_2.glb",
    "fps_floor": FPS + "floor_1.glb",
    "fps_stairs": FPS + "stairs_1.glb",
    "fps_tunnel": FPS + "tunnel_1.glb",
    "fps_tunnel_end": FPS + "tunnel_1_end.glb",
    "fps_crate": FPS + "crate_1.glb",
    "fps_barrel": FPS + "metal_barrel_fps_1.glb",
    "fps_lamp": FPS + "ceiling_lamp_fps_1.glb",
    # spring countryside (act 2)
    "tree_a": NAT + "tree_1.glb",
    "tree_b": NAT + "tree_5.glb",
    "tree_c": NAT + "tree_7.glb",
    "tree_d": NAT + "tree_8.glb",
    "wheat": NAT + "wheat_1.glb",
    "grass": NAT + "grass_2.glb",
    "fence_wood": MP2 + "Modular Props/fence_a_1.glb",
    "fence_wood2": MP2 + "Modular Props/fence_b_1.glb",
    "fence_concrete": MP2 + "Modular Props/concrete_fence_hr_1.glb",
    "water_tower": MP2 + "Structures/water_tower_hm_1.glb",
    # scale-reference tanks (the half-res Tiger bake; engine-convention origins, see src/tank.rs)
    "tiger_hull": "assets/tanks/tiger/baked/half/tiger_hull.glb",
    "tiger_turret": "assets/tanks/tiger/baked/half/tiger_turret.glb",
    "tiger_gun": "assets/tanks/tiger/baked/half/tiger_gun.glb",
    # ── worldgen v2 (kessel-map-design): roads, wires, logistics, battle scars, nature variety ──
    "asphalt": MP2 + "Modular Structures/asphalt_hr_1.glb",          # 4×4 m slab
    "asphalt_lg": MP2 + "Modular Structures/asphalt_hr_1_large.glb", # 12×12 m slab
    "container": MP2 + "Large Props & Machinery/shipping_container_mx_1.glb",
    "container2": MP2 + "Large Props & Machinery/shipping_container_mx_2.glb",
    "container_open": MP2 + "Large Props & Machinery/shipping_container_mx_1_hollow_1.glb",
    "crate": MP2 + "Props/wooden_crate_2_a.glb",
    "crate2": MP2 + "Props/wooden_crate_hx_3.glb",
    "barrel": MP2 + "Props/metal_barrel_hr_1.glb",
    "barrel2": MP2 + "Props/metal_barrel_hr_2.glb",
    "deadman": "assets/flavor/Characters/deadman-psx/baked/deadman_prone.glb",
    "tree_e": NAT + "tree_6.glb",
    "tree_f": NAT + "tree_9.glb",
    "fern": NAT + "fern_1.glb",
    "fern2": NAT + "fern_2.glb",
    "log": NAT + "tree_log_1.glb",
    "stump": NAT + "tree_stump_1.glb",
    "stone2": NAT + "stone_2.glb",
    "stone3": NAT + "stone_3.glb",
    "stone4": NAT + "stone_4.glb",
    "stone5": NAT + "stone_5.glb",
    # destroyed REAL vehicles beyond the playable roster (FBX→GLB bakes, bbox-bottom origins)
    "wreck_panther": "assets/tanks/wrecks/panther.glb",
    "wreck_tiger2": "assets/tanks/wrecks/tiger2.glb",
    "wreck_pz3": "assets/tanks/wrecks/pz3.glb",
    "wreck_kv1": "assets/tanks/wrecks/kv1.glb",
    "wreck_t34": "assets/tanks/wrecks/t34_85.glb",
    "wreck_is2": "assets/tanks/wrecks/is2.glb",
}

WRECK_KEYS = ["wreck_panther", "wreck_tiger2", "wreck_pz3", "wreck_kv1", "wreck_t34", "wreck_is2"]
TREE_KEYS = ["tree_a", "tree_b", "tree_c", "tree_d", "tree_e", "tree_f"]

# Roads (worldgen v2): dirt in the open country (stamped into the ground texture by
# make_ground_tex.py), asphalt slabs inside towns, telephone poles + sagging wires alongside.
ROAD_W = 7.0          # flattened-ground half-width (m)
POLE_PITCH = 36.0     # telephone-pole spacing along roads (m)
POLE_H = 7.2          # pole-top wire attachment height (m)
TOWN_R = 140.0        # road within this range of a building cluster = "in town" (gets slabs)

# TIGER_PARTS from src/tank.rs — hull-local pivots (x, y-up, z-forward), yawed at placement.
TIGER_TURRET_PIVOT = (0.0, 0.524, 0.019)
TIGER_GUN_PIVOT = (0.004, 0.524 + 1.845, 0.019 + 1.040)  # turret_pivot + gun_trunnion

# Origin-to-base lift (m at scale 1). PSX Nature GLBs are authored Z-up with arbitrary offsets;
# assemble_blend.py normalizes their library copies (upright, bbox bottom-centre at origin), so no
# lift is needed there. Kept as a hook for packs with non-ground origins that AREN'T normalized.
BASE_LIFT = {}

CELL = 4.0     # module cell (wall_hr_1 is 4.00 m wide)
WALL_H = 3.0   # storey height
STREET = 10.0  # street gap
MIN_FC, MAX_FC = 3, 5
PITCH = MAX_FC * CELL + STREET

# DEM constants (assets/terrain/stalingrad_100km_meta.json)
MPP = 25.220273854682294
DEM = "assets/terrain/stalingrad_100km.npy"


def hash01(n: int) -> float:
    x = (n * 2654435761) & 0xFFFFFFFF
    x ^= x >> 15
    x = (x * 2246822519) & 0xFFFFFFFF
    x ^= x >> 13
    return ((x >> 8) & 0xFFFF) / 65536.0


class World:
    def __init__(self, act_id, act):
        self.act_id = act_id
        self.act = act
        self.seed = act["seed"]
        self.assets = []          # unique GLB paths, indexed
        self.asset_idx = {}
        self.placements = []      # [asset_idx, x, y, z, rot_y_deg, scale | [sx,sy,sz]]
        self.markers = []         # {name, kind, x, z}
        self.wires = []           # polylines [[x,y,z], ...] the assembler turns into curves
        self.roads = []           # segments [x0,z0,x1,z1] (features.json + texture stamping)
        self.craters = []         # [x, z, radius, depth]
        self.town_pts = []        # building-cluster centroids (asphalt-road test)
        self.load_terrain()

    # ── terrain ──
    @staticmethod
    def upsample_grid(grid, res):
        """Bilinear-upsample a square heightfield to res² (craters/berms need finer cells than
        the 30 m DEM source; no real information added, just carving resolution)."""
        H = grid.shape[0]
        if H == res:
            return grid
        g = (np.arange(res, dtype=np.float32)) / (res - 1) * (H - 1)
        i0 = np.floor(g).astype(int).clip(0, H - 2)
        f = (g - i0).astype(np.float32)
        a = grid[i0][:, i0]
        b = grid[i0][:, i0 + 1]
        c = grid[i0 + 1][:, i0]
        d = grid[i0 + 1][:, i0 + 1]
        fy = f[:, None]
        fx = f[None, :]
        return a * (1 - fy) * (1 - fx) + b * (1 - fy) * fx + c * fy * (1 - fx) + d * fy * fx

    def load_terrain(self):
        a = self.act
        res = a.get("terrain_res", 192)
        if "dem" in a:  # pre-cropped act window (already the right extent, row 0 = north)
            grid = np.load(a["dem"]).astype(np.float32)
            grid -= float(grid[grid.shape[0] // 2, grid.shape[0] // 2])  # re-zero at centre
            self.heights = self.upsample_grid(grid, res)
            self.res = res
            self.spacing = a["half_extent"] * 2.0 / (res - 1)
        else:
            dem = np.load(DEM)
            h, w = dem.shape
            cx, cy = w // 2, h // 2
            sx = cx - int(a["west_offset"] / MPP)  # west = -x
            half_px = int(a["half_extent"] / MPP)
            src = 192
            xs = np.linspace(sx - half_px, sx + half_px, src).astype(int).clip(0, w - 1)
            ys = np.linspace(cy - half_px, cy + half_px, src).astype(int).clip(0, h - 1)
            grid = dem[np.ix_(ys, xs)].astype(np.float32)
            grid -= float(dem[cy, sx])  # spawn height = 0, like terrain.rs
            self.heights = self.upsample_grid(grid, res)
            self.res = res
            self.spacing = (half_px * 2 * MPP) / (res - 1)
        # world-coordinate grids for vectorized carving (craters, road flattening)
        half = (self.res - 1) * 0.5
        ax = (np.arange(self.res, dtype=np.float32) - half) * self.spacing
        self._gx = ax[None, :]   # east per column
        self._gz = ax[:, None]   # south per row

    # ── terrain shaping (worldgen v2: compartmentalize the open field) ──
    def micro_relief(self, amp1=1.3, amp2=0.5, wl1=420.0, wl2=140.0):
        """Two octaves of smooth value noise — gentle berms and hollows that give hull-down
        lines without destroying the act's real macro-terrain."""
        def octave(amp, wl, salt):
            n = max(4, int(self.res * self.spacing / wl))
            g = np.array([[hash01((self.seed ^ salt ^ (j * 7919 + i) * 2654435761) & 0xFFFFFFFF)
                           for i in range(n)] for j in range(n)], np.float32)
            return (self.upsample_grid(g, self.res) - 0.5) * 2.0 * amp
        self.heights += octave(amp1, wl1, 0x0BE1) + octave(amp2, wl2, 0x0BE2)

    def add_craters(self, n, r_min=4.0, r_max=13.0, d_scale=1.0):
        """Shell/bomb craters: gaussian depression + raised rim, carved into the heightfield and
        recorded for the ground-texture scorch stamp. THE fields-are-shelled-out pass."""
        he = self.act["half_extent"]
        for k in range(n):
            cid = 0xC0A7 + k
            x = (self.rnd(cid, 1) - 0.5) * 2 * (he - 60.0)
            z = (self.rnd(cid, 2) - 0.5) * 2 * (he - 60.0)
            r = r_min + self.rnd(cid, 3) * (r_max - r_min)
            d = (0.55 + self.rnd(cid, 4) * 1.2) * d_scale * (r / r_max + 0.45)
            self.crater_at(x, z, r, d)

    def crater_at(self, x, z, r, d):
        q2 = ((self._gx - x) ** 2 + (self._gz - z) ** 2) / (r * r)
        m = q2 < 9.0
        q2m = q2[m]
        # depression + rim (rim peaks around 1.3 r, decays outward)
        self.heights[m] += (-d * np.exp(-q2m * 1.8)
                            + d * 0.35 * np.exp(-(np.sqrt(q2m) - 1.4) ** 2 * 2.2))
        self.craters.append([round(x, 1), round(z, 1), round(r, 2), round(d, 2)])

    def flatten_road(self, x0, z0, x1, z1):
        """Grade the ground along a road segment toward the line between its endpoint heights."""
        h0, h1 = self.ground(x0, z0), self.ground(x1, z1)
        dx, dz = x1 - x0, z1 - z0
        L2 = dx * dx + dz * dz
        if L2 < 1.0:
            return
        t = np.clip(((self._gx - x0) * dx + (self._gz - z0) * dz) / L2, 0.0, 1.0)
        px = x0 + t * dx
        pz = z0 + t * dz
        dist = np.sqrt((self._gx - px) ** 2 + (self._gz - pz) ** 2)
        w = np.clip(1.0 - dist / ROAD_W, 0.0, 1.0) ** 1.5
        target = h0 + t * (h1 - h0)
        self.heights += (target - self.heights) * w * 0.85

    def seg_dist(self, x, z, s):
        x0, z0, x1, z1 = s
        dx, dz = x1 - x0, z1 - z0
        L2 = dx * dx + dz * dz
        t = 0.0 if L2 < 1.0 else max(0.0, min(1.0, ((x - x0) * dx + (z - z0) * dz) / L2))
        return math.hypot(x - (x0 + t * dx), z - (z0 + t * dz))

    def road_dist(self, x, z):
        return min((self.seg_dist(x, z, s) for s in self.roads), default=1e9)

    def build_road_network(self, nodes):
        """Minimum-spanning-tree dirt-road net over the act's places (landmarks, clusters,
        spawn/goal), graded into the terrain. Deterministic; consumed by placements, the wires
        pass, and make_ground_tex's dirt stamp."""
        if len(nodes) < 2:
            return
        in_tree = [0]
        dist = [math.hypot(n[0] - nodes[0][0], n[1] - nodes[0][1]) for n in nodes]
        parent = [0] * len(nodes)
        while len(in_tree) < len(nodes):
            best, bd = -1, 1e18
            for i in range(len(nodes)):
                if i not in in_tree and dist[i] < bd:
                    best, bd = i, dist[i]
            in_tree.append(best)
            a, b = nodes[parent[best]], nodes[best]
            self.roads.append([round(a[0], 1), round(a[1], 1), round(b[0], 1), round(b[1], 1)])
            for i in range(len(nodes)):
                d = math.hypot(nodes[i][0] - nodes[best][0], nodes[i][1] - nodes[best][1])
                if i not in in_tree and d < dist[i]:
                    dist[i] = d
                    parent[i] = best
        for s in self.roads:
            self.flatten_road(*s)

    def dress_roads(self):
        """Asphalt slabs where a road runs through town; telephone poles + sagging wires along
        every road (offset to the right of travel)."""
        for si, s in enumerate(self.roads):
            x0, z0, x1, z1 = s
            L = math.hypot(x1 - x0, z1 - z0)
            if L < 20.0:
                continue
            ux, uz = (x1 - x0) / L, (z1 - z0) / L
            yaw = math.atan2(ux, uz)
            # right-hand normal for the pole line
            nx, nz = uz, -ux
            k = 0
            while k * 4.0 < L:  # asphalt only for the stretch actually inside a town
                t = k * 4.0 + 2.0
                x, z = x0 + ux * t, z0 + uz * t
                if any((x - tx) ** 2 + (z - tz) ** 2 < TOWN_R ** 2 for tx, tz in self.town_pts):
                    self.place("asphalt", x, self.ground(x, z) + 0.04, z, yaw)
                k += 1
            # poles + wires
            n_poles = int(L / POLE_PITCH)
            prev_top = None
            for k in range(n_poles + 1):
                t = min(k * POLE_PITCH, L)
                x = x0 + ux * t + nx * 5.0
                z = z0 + uz * t + nz * 5.0
                gy = self.ground(x, z)
                lean = (self.rnd(0x901E + si * 97 + k, 1) - 0.5) * 0.12
                self.place("pillar", x, gy - 0.2, z, yaw + lean, [0.22, 2.55, 0.22])
                top = [round(x, 2), round(gy + POLE_H, 2), round(z, 2)]
                if prev_top is not None:
                    mid = [round((prev_top[0] + top[0]) / 2, 2),
                           round((prev_top[1] + top[1]) / 2 - 0.8, 2),
                           round((prev_top[2] + top[2]) / 2, 2)]
                    self.wires.append([prev_top, mid, top])
                prev_top = top

    # ── war scars (worldgen v2): battle sites, logistics dumps, the dead ──
    def wreck(self, key, x, z, yaw, wid):
        """One destroyed vehicle, sunk slightly, with scorch rubble and a crater nearby."""
        self.place(key, x, self.ground(x, z) - 0.25, z, yaw)
        self.place("rubble_field", x + 3.0, self.ground(x + 3.0, z + 2.0) - 0.1, z + 2.0,
                   self.rnd(wid, 8) * math.tau, 1.1)

    def battle_site(self, x, z, sid):
        """Site of a previous engagement: mixed wrecks, craters, the dead, burnt stumps."""
        n_wr = 2 + int(self.rnd(sid, 1) * 3)  # 2..4 vehicles
        for k in range(n_wr):
            a = self.rnd(sid, 10 + k) * math.tau
            d = 8.0 + self.rnd(sid, 40 + k) * 42.0
            wx, wz = x + math.cos(a) * d, z + math.sin(a) * d
            key = WRECK_KEYS[int(self.rnd(sid, 70 + k) * len(WRECK_KEYS)) % len(WRECK_KEYS)]
            self.wreck(key, wx, wz, self.rnd(sid, 100 + k) * math.tau, sid * 13 + k)
        for k in range(3 + int(self.rnd(sid, 2) * 4)):  # the dead
            a = self.rnd(sid, 200 + k) * math.tau
            d = 4.0 + self.rnd(sid, 230 + k) * 50.0
            dx, dz = x + math.cos(a) * d, z + math.sin(a) * d
            self.place_ground("deadman", dx, dz, self.rnd(sid, 260 + k) * math.tau)
        for k in range(4):  # burnt stumps + a fallen log
            a = self.rnd(sid, 300 + k) * math.tau
            d = 10.0 + self.rnd(sid, 330 + k) * 45.0
            sx_, sz_ = x + math.cos(a) * d, z + math.sin(a) * d
            self.place_ground("stump" if k % 2 == 0 else "log", sx_, sz_,
                              self.rnd(sid, 360 + k) * math.tau, 1.0 + self.rnd(sid, 390 + k))
        # concentrated shelling around the site (also carves the heightfield)
        for k in range(5 + int(self.rnd(sid, 3) * 5)):
            a = self.rnd(sid, 400 + k) * math.tau
            d = self.rnd(sid, 430 + k) * 60.0
            self.crater_at(x + math.cos(a) * d, z + math.sin(a) * d,
                           3.5 + self.rnd(sid, 460 + k) * 5.0, 0.8 + self.rnd(sid, 490 + k) * 0.8)

    def logistics_site(self, x, z, sid):
        """An abandoned/shelled supply dump: container rows, crate stacks, barrels, a wrecked
        hauler, hard cover the player can actually use."""
        yaw = self.rnd(sid, 1) * math.pi
        c, s = math.cos(yaw), math.sin(yaw)
        for k in range(3 + int(self.rnd(sid, 2) * 3)):  # container rows
            lx = (k % 3) * 4.0 - 4.0
            lz = (k // 3) * 12.0 - 6.0
            wx = x + lx * c + lz * s
            wz = z - lx * s + lz * c
            key = ["container", "container2", "container_open"][int(self.rnd(sid, 10 + k) * 3) % 3]
            self.place_ground(key, wx, wz, yaw + (self.rnd(sid, 40 + k) - 0.5) * 0.15)
        for k in range(8):
            a = self.rnd(sid, 100 + k) * math.tau
            d = 6.0 + self.rnd(sid, 130 + k) * 18.0
            key = ["crate", "crate2", "barrel", "barrel2"][int(self.rnd(sid, 160 + k) * 4) % 4]
            self.place_ground(key, x + math.cos(a) * d, z + math.sin(a) * d,
                              self.rnd(sid, 190 + k) * math.tau)
        wk = WRECK_KEYS[int(self.rnd(sid, 5) * len(WRECK_KEYS)) % len(WRECK_KEYS)]
        self.wreck(wk, x + 24.0, z - 10.0, self.rnd(sid, 6) * math.tau, sid * 7)
        for k in range(3):
            self.crater_at(x + (self.rnd(sid, 300 + k) - 0.5) * 50.0,
                           z + (self.rnd(sid, 330 + k) - 0.5) * 50.0,
                           4.0 + self.rnd(sid, 360 + k) * 4.0, 1.0)

    def boulders_and_rocks(self, n_boulders, n_rocks):
        """Big glacial boulders (hard cover) + small rock scatter."""
        he = self.act["half_extent"]
        for k in range(n_boulders):
            bid = 0xB0D0 + k
            x = (self.rnd(bid, 1) - 0.5) * 2 * (he - 100.0)
            z = (self.rnd(bid, 2) - 0.5) * 2 * (he - 100.0)
            if self.road_dist(x, z) < 10.0:
                continue
            key = ["stone", "stone2", "stone3"][int(self.rnd(bid, 3) * 3) % 3]
            self.place_ground(key, x, z, self.rnd(bid, 4) * math.tau,
                              1.6 + self.rnd(bid, 5) * 2.4, sink=0.4)
        for k in range(n_rocks):
            rid = 0xB0E0 + k
            x = (self.rnd(rid, 1) - 0.5) * 2 * (he - 80.0)
            z = (self.rnd(rid, 2) - 0.5) * 2 * (he - 80.0)
            key = ["stone4", "stone5", "stone3"][int(self.rnd(rid, 3) * 3) % 3]
            self.place_ground(key, x, z, self.rnd(rid, 4) * math.tau,
                              0.5 + self.rnd(rid, 5) * 0.9)

    def ground(self, x, z):
        half = (self.res - 1) * 0.5
        gx = min(max(x / self.spacing + half, 0), self.res - 1)
        gz = min(max(z / self.spacing + half, 0), self.res - 1)
        x0, z0 = int(gx), int(gz)
        x1, z1 = min(x0 + 1, self.res - 1), min(z0 + 1, self.res - 1)
        tx, tz = gx - x0, gz - z0
        g = self.heights
        a = g[z0, x0] * (1 - tx) + g[z0, x1] * tx
        b = g[z1, x0] * (1 - tx) + g[z1, x1] * tx
        return float(a * (1 - tz) + b * tz)

    # ── placement primitives ──
    def place(self, key, x, y, z, rot=0.0, scale=1.0):
        path = KIT[key]
        if path not in self.asset_idx:
            if not os.path.exists(path):
                print(f"[gen] MISSING asset {path}")
                return
            self.asset_idx[path] = len(self.assets)
            self.assets.append(path)
        s = [round(v, 3) for v in scale] if isinstance(scale, (list, tuple)) else round(scale, 3)
        self.placements.append([self.asset_idx[path], round(x, 2), round(y, 2), round(z, 2),
                                round(math.degrees(rot), 1), s])

    def rnd(self, bid, salt):
        return hash01((self.seed ^ (bid * 0x9E3779B9) ^ (salt * 0x85EBCA77)) & 0xFFFFFFFF)

    def place_ground(self, key, x, z, rot=0.0, scale=1.0, sink=0.0):
        """Place with the mesh BASE at ground level (BASE_LIFT corrects mid-trunk origins)."""
        y = self.ground(x, z) + BASE_LIFT.get(key, 0.0) * scale - sink
        self.place(key, x, y, z, rot, scale)

    # ── buildings with interiors ──
    def building(self, cx, cz, bid, r):
        base = self.ground(cx, cz) - 0.3
        fcx = MIN_FC + int(self.rnd(bid, 2) * (MAX_FC - MIN_FC + 1))
        fcz = MIN_FC + int(self.rnd(bid, 3) * (MAX_FC - MIN_FC + 1))
        fcx, fcz = min(fcx, MAX_FC), min(fcz, MAX_FC)
        hx, hz = fcx * CELL * 0.5, fcz * CELL * 0.5
        storeys = 2 if self.rnd(bid, 4) < (1.0 - r) * 0.45 else 1

        for s in range(storeys):
            y = base + s * WALL_H
            ground = s == 0
            # perimeter walls
            for i in range(fcx):
                wx = cx - hx + (i + 0.5) * CELL
                self.wall_seg(wx, y, cz - hz, 0.0, bid, s * 40 + i, r, ground)
                self.wall_seg(wx, y, cz + hz, 0.0, bid, s * 40 + 10 + i, r, ground)
            for j in range(fcz):
                wz = cz - hz + (j + 0.5) * CELL
                self.wall_seg(cx - hx, y, wz, math.pi / 2, bid, s * 40 + 20 + j, r, ground)
                self.wall_seg(cx + hx, y, wz, math.pi / 2, bid, s * 40 + 30 + j, r, ground)
            # corner pillars
            for k, (px, pz) in enumerate([(cx - hx, cz - hz), (cx + hx, cz - hz),
                                          (cx - hx, cz + hz), (cx + hx, cz + hz)]):
                v = self.rnd(bid, 300 + s * 4 + k)
                if v < 0.15 * r:
                    continue
                self.place("pillar_broken" if v < 0.4 * r else "pillar", px, y, pz,
                           self.rnd(bid, 900 + k) * math.tau if v < 0.4 * r else 0.0)
            # floor slabs (some shell-holed on upper floors)
            for gi in range(fcx):
                for gj in range(fcz):
                    fx = cx - hx + (gi + 0.5) * CELL
                    fz = cz - hz + (gj + 0.5) * CELL
                    holed = s > 0 and self.rnd(bid, 1300 + gi * 8 + gj) < 0.25 * r
                    self.place("floor_hole" if holed else "floor", fx, y, fz)
            # INTERIOR: a partition wall row with a doorway, splitting the plan into two rooms
            if fcx >= 4:
                pj = fcz // 2
                wz = cz - hz + pj * CELL
                door_i = 1 + int(self.rnd(bid, 1400 + s) * (fcx - 2))
                for i in range(fcx):
                    wx = cx - hx + (i + 0.5) * CELL
                    if self.rnd(bid, 1500 + s * 16 + i) < 0.30 * r:
                        continue  # ruination ate this partition segment
                    self.place("doorway" if i == door_i else "wall", wx, y, wz, 0.0)
            # furniture in ground-floor rooms (the scavengeable interior)
            if ground:
                nfurn = 2 + int(self.rnd(bid, 1600) * 4)
                for f in range(nfurn):
                    fx = cx - hx + CELL + self.rnd(bid, 1700 + f) * (2 * hx - 2 * CELL)
                    fz = cz - hz + CELL + self.rnd(bid, 1750 + f) * (2 * hz - 2 * CELL)
                    kind = ["furn_bed", "furn_table", "furn_chair", "furn_cabinet"][int(self.rnd(bid, 1800 + f) * 4) % 4]
                    self.place(kind, fx, base + 0.05, fz, self.rnd(bid, 1850 + f) * math.tau)
        # stairs up (2-storey buildings get an interior staircase)
        if storeys == 2:
            self.place("stairs", cx - hx + CELL * 0.9, base, cz + hz - CELL * 0.9, math.pi)
        # roof — mostly gone under heavy ruination
        top = base + storeys * WALL_H
        if self.rnd(bid, 9) > 0.25 + 0.6 * r:
            flat = self.rnd(bid, 8) < 0.4
            for gi in range(fcx):
                for gj in range(fcz):
                    fx = cx - hx + (gi + 0.5) * CELL
                    fz = cz - hz + (gj + 0.5) * CELL
                    if flat:
                        if gi % 2 == 0:
                            self.place("roof_flat", fx + CELL * 0.5, top - 0.05, fz)
                    else:
                        self.place("roof_pitch", fx, top - 0.05, fz)
        # street rubble ring
        n = 1 + int(r * 5)
        for k in range(n):
            a = self.rnd(bid, 400 + k) * math.tau
            d = max(hx, hz) + 1.0 + self.rnd(bid, 450 + k) * (STREET * 0.5)
            x, z = cx + math.cos(a) * d, cz + math.sin(a) * d
            kind = ["rubble_field", "rubble_stack", "brick"][int(self.rnd(bid, 500 + k) * 3) % 3]
            self.place(kind, x, self.ground(x, z) - 0.1, z, self.rnd(bid, 550 + k) * math.tau,
                       0.8 + self.rnd(bid, 600 + k) * 0.6)

    def wall_seg(self, x, y, z, rot, bid, key, r, ground):
        v = self.rnd(bid, 100 + key)
        if v < 0.08 + 0.30 * r:
            self.place("rubble_stack", x, self.ground(x, z) - 0.1, z, self.rnd(bid, 700 + key) * math.tau)
            return
        w = self.rnd(bid, 200 + key)
        if w < 0.25 * r:
            k = "wall_hole"
        elif ground and w < 0.25 * r + 0.10:
            k = "doorway"
        elif w < 0.25 * r + 0.30:
            k = "wall_win" if ground else "window"
        else:
            k = "wall"
        self.place(k, x, y, z, rot)
        if k == "wall_hole":
            self.place("rubble_field", x, self.ground(x, z) - 0.1, z, self.rnd(bid, 800 + key) * math.tau)

    # ── landmark blockouts (placeholders the user replaces with hero GLBs) ──
    def landmark(self, kind, x, z, name):
        base = self.ground(x, z) - 0.3
        self.markers.append(dict(name=name, kind=kind, x=x, z=z, y=base))
        if kind == "Church":
            # nave: 3×5 cells, 2 storeys + a 6-storey bell tower at the west end
            for s in range(2):
                for i in range(3):
                    for j in range(5):
                        self.place("floor", x + (i - 1) * CELL, base + s * WALL_H, z + (j - 2) * CELL)
                for j in range(5):
                    self.place("wall", x - 1.5 * CELL, base + s * WALL_H, z + (j - 2) * CELL, math.pi / 2)
                    self.place("wall", x + 1.5 * CELL, base + s * WALL_H, z + (j - 2) * CELL, math.pi / 2)
            for s in range(6):
                for (dx, dz, rr) in [(0, -1, 0), (0, 1, 0), (-1, 0, math.pi / 2), (1, 0, math.pi / 2)]:
                    self.place("wall", x + dx * CELL * 0.5 * (2 if dx else 1),
                               base + s * WALL_H, z - 3 * CELL + dz * CELL * 0.5 * (2 if dz else 1), rr)
            self.place("roof_pitch", x, base + 6 * WALL_H, z - 3 * CELL)
        elif kind == "GrainElevator":
            self.place("warehouse", x, base, z, 0.0, 2.6)
        elif kind == "RailStation":
            self.place("warehouse", x, base, z, math.pi / 2, 1.4)
            self.place("garage", x + 22.0, base, z, math.pi / 2, 1.0)
        elif kind == "FactoryChimney":
            self.place("pillar", x, base, z, 0.0, 7.0)
        elif kind == "Monument":
            self.place("rubble_stack", x, base, z, 0.0, 2.0)
            self.place("pillar", x, base + 0.4, z, 0.0, 1.6)
        elif kind == "WaterTower":
            self.place_ground("water_tower", x, z, 0.0, 1.0)
        elif kind == "WreckField":
            # The Blocked Highway: a column of dead Tigers along the road axis (NE-SW at Halbe),
            # some with the turret blown off beside the hull, rubble in between. A grim waypoint.
            ax, az = 0.82, -0.57  # unit-ish highway direction
            for k in range(5):
                d = (k - 2) * 26.0
                wx = x + ax * d + (self.rnd(0xDEAD, 10 + k) - 0.5) * 10.0
                wz = z + az * d + (self.rnd(0xDEAD, 40 + k) - 0.5) * 10.0
                yaw = math.atan2(ax, az) + (self.rnd(0xDEAD, 70 + k) - 0.5) * 1.6
                self.place("tiger_hull", wx, self.ground(wx, wz) - 0.1, wz, yaw)
                if self.rnd(0xDEAD, 100 + k) < 0.5:  # turret thrown clear
                    tx = wx + (self.rnd(0xDEAD, 130 + k) - 0.5) * 12.0
                    tz = wz + 5.0 + self.rnd(0xDEAD, 160 + k) * 5.0
                    self.place("tiger_turret", tx, self.ground(tx, tz) + 0.2, tz,
                               self.rnd(0xDEAD, 190 + k) * math.tau)
                else:  # turret still mounted, slewed
                    self.tiger_turret_on(wx, wz, yaw, slew=(self.rnd(0xDEAD, 220 + k) - 0.5) * 2.0)
                kind_r = ["rubble_field", "rubble_stack"][k % 2]
                self.place(kind_r, wx + 8.0, self.ground(wx + 8.0, wz) - 0.1, wz - 6.0,
                           self.rnd(0xDEAD, 250 + k) * math.tau, 1.2)
        elif kind == "Windmill":
            self.windmill(x, z)
        elif kind == "Manor":
            self.manor(x, z)
        elif kind == "Bridge":
            self.bridge(x, z)
        else:
            self.place("shed", x, base, z, 0.0, 1.5)

    # ── the bunker (Retro FPS kit, sunk below the terrain, entered by stairs) ──
    def bunker(self, x, z):
        top = self.ground(x, z)
        depth = top - 5.0
        self.markers.append(dict(name="Bunker", kind="Bunker", x=x, z=z, y=top))
        # entrance stair shaft
        self.place("fps_stairs", x, depth, z, 0.0)
        # corridor of tunnel segments heading north
        for k in range(4):
            self.place("fps_tunnel", x, depth, z - 6.0 - k * 4.0, 0.0)
        self.place("fps_tunnel_end", x, depth, z - 24.0, 0.0)
        # a store room off the corridor: floor + walls + loot + lamps
        rx, rz = x + 8.0, z - 14.0
        for i in range(3):
            for j in range(3):
                self.place("fps_floor", rx + (i - 1) * 4.0, depth, rz + (j - 1) * 4.0)
        for i in range(3):
            self.place("fps_wall", rx + (i - 1) * 4.0, depth, rz - 6.0, 0.0)
            self.place("fps_wall", rx + (i - 1) * 4.0, depth, rz + 6.0, 0.0)
            self.place("fps_wall", rx + 6.0, depth, rz + (i - 1) * 4.0, math.pi / 2)
        for k in range(5):
            self.place("fps_crate", rx - 3 + hash01(k * 77) * 6, depth + 0.05, rz - 3 + hash01(k * 131) * 6,
                       hash01(k * 313) * math.tau)
        for k in range(3):
            self.place("fps_barrel", rx + 3 - hash01(k * 991) * 2, depth + 0.05, rz + 2, 0.0)
        self.place("fps_lamp", rx, depth + 2.8, rz)

    # ══ ACT-2 SPRING COUNTRYSIDE (Oderbruch polder below the Seelow scarp) ══════════════════════
    #
    # The dressing is driven by the act's REAL spring Sentinel-2 imagery (the same raster that is
    # draped on the terrain), so the 3D world and the ground texture agree: hedgerow trees stand
    # on the actual field boundaries, copses on the actual dark-canopy patches, farmsteads on the
    # actual building clusters, and the visible farm lanes stay clear. Thresholds calibrated
    # against assets/terrain/act2/act2_window_tci.npy (debug-classification loop, 2026-07-02).

    def classify_imagery(self):
        """Segment the TCI window into masks in image space (row 0 = north, col 0 = west)."""
        img = np.load(self.act["tci_npy"]).astype(np.float32)
        H, W = img.shape[:2]

        def box(a, k):
            p = np.pad(a.astype(np.float32), k // 2, mode="edge")
            return sum(p[i:i + H, j:j + W] for i in range(k) for j in range(k)) / (k * k)

        r, g, b = box(img[..., 0], 5), box(img[..., 1], 5), box(img[..., 2], 5)
        v = np.maximum(np.maximum(r, g), b)
        vs = img.max(2)
        sats = (vs - img.min(2)) / np.maximum(vs, 1)
        var = box(vs * vs, 5) - box(vs, 5) ** 2  # canopy texture: high in forest, low in crops

        white = (vs > 185) & (sats < 0.35)                 # white farm complex / big roofs
        redroof = (img[..., 0] > img[..., 1] + 30) & (vs > 100) & (img[..., 0] > 110)
        bldg = box(white | redroof, 3) > 0.3
        darkveg = (g >= r) & (g >= b) & (v < 68) & ~bldg
        woods = box(darkveg & (box(var, 5) > 185), 3) > 0.4
        darkcrop = darkveg & ~woods                        # young winter cereal — smooth, dark
        greenfld = (g >= r * 0.98) & (v >= 68) & ~darkveg & ~bldg
        fld = np.zeros((H, W), np.float32)                 # 0 bare plough, 1 green, 2 dark crop
        fld[greenfld] = 1.0
        fld[darkcrop] = 2.0
        # Deep-forest acts (Halbe): pine canopy is so dark it loses green dominance entirely —
        # classify DARK+TEXTURED as forest and DARK+SMOOTH as water (lakes stay unplanted).
        water = np.zeros_like(bldg)
        df = self.act.get("dark_forest")
        if df:
            dark = (v < df["v"]) & ~bldg
            var_s = box(var, 5)
            woods = woods | (dark & (var_s > df["var"]))
            water = dark & (var_s <= df["var"])
        fs = box(fld, 3)
        edge = (np.abs(np.gradient(fs)[0]) + np.abs(np.gradient(fs)[1])) > 0.22
        hedge = edge & ~woods & ~bldg & (box(bldg, 7) < 0.03)
        road = box((vs - box(vs, 9)) > 14, 3) > 0.35       # thin pale farm lanes (top-hat)
        keep_clear = (box(road | bldg, 3) > 0.15) | (box(water, 3) > 0.2)
        self.img_res = (H, W)
        return dict(bldg=bldg, woods=woods, darkcrop=darkcrop, greenfld=greenfld,
                    hedge=hedge, keep_clear=keep_clear, water=water)

    def tiger(self, x, z, yaw):
        """A parked low-poly Tiger for scale: hull grounded, turret + gun on their rig pivots,
        the whole rig yawed together (turret facing hull-forward, gun level)."""
        base = self.ground(x, z)
        c, s = math.cos(yaw), math.sin(yaw)

        def yawed(p):  # hull-local (x, y, z-forward) → world offset under yaw about +Y
            return (p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c)

        tp, gp = yawed(TIGER_TURRET_PIVOT), yawed(TIGER_GUN_PIVOT)
        self.place("tiger_hull", x, base, z, yaw)
        self.place("tiger_turret", x + tp[0], base + tp[1], z + tp[2], yaw)
        self.place("tiger_gun", x + gp[0], base + gp[1], z + gp[2], yaw)

    def tiger_turret_on(self, x, z, hull_yaw, slew=0.0):
        """A turret still mounted on a (wreck) hull placed at ground−0.1, slewed off-axis."""
        base = self.ground(x, z) - 0.1
        c, s = math.cos(hull_yaw), math.sin(hull_yaw)
        p = TIGER_TURRET_PIVOT
        tp = (p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c)
        self.place("tiger_turret", x + tp[0], base + tp[1], z + tp[2], hull_yaw + slew)

    def px_to_world(self, i, j):
        """Image (col i, row j) → engine (x, z). The raster covers exactly the act window."""
        H, W = self.img_res
        he = self.act["half_extent"]
        return (i / (W - 1)) * 2 * he - he, (j / (H - 1)) * 2 * he - he

    def farmstead(self, pts):
        """One real building cluster → a farmstead: houses on the cluster's own pixels, a fence,
        a couple of yard trees. Big clusters (the white agro-complex) get warehouse volumes."""
        H, W = self.img_res
        cj = sum(p[0] for p in pts) / len(pts)
        ci = sum(p[1] for p in pts) / len(pts)
        cx, cz = self.px_to_world(ci, cj)
        fid = (0xF000 + int(cj) * W + int(ci)) & 0xFFFFFFFF
        self.town_pts.append((cx, cz))
        if len(pts) > 90:  # the industrial farm complex
            self.place_ground("warehouse", cx, cz, 0.0, 1.8)
            self.place_ground("garage", cx + 26.0, cz + 14.0, math.pi / 2, 1.1)
            self.place_ground("shed", cx - 24.0, cz + 10.0, 0.0, 1.2)
            self.place_ground("container", cx + 12.0, cz - 18.0, 0.3)
            self.place_ground("container2", cx + 16.5, cz - 18.0, 0.3)
            return
        # SEVERAL structures per cluster, spaced apart (kessel-map-design: never one building)
        n = max(3, min(2 + len(pts) // 6, 7))
        placed = []
        for k in range(n * 2):  # oversample attempts; spacing rejects the crowded ones
            if len(placed) >= n:
                break
            pj, pi = pts[int(self.rnd(fid, 10 + k) * len(pts)) % len(pts)]
            hx, hz = self.px_to_world(pi, pj)
            hx += (self.rnd(fid, 40 + k) - 0.5) * 26.0
            hz += (self.rnd(fid, 70 + k) - 0.5) * 26.0
            if any((hx - qx) ** 2 + (hz - qz) ** 2 < 24.0 ** 2 for qx, qz in placed):
                continue
            placed.append((hx, hz))
            v = self.rnd(fid, 500 + k)
            if v < 0.62:
                self.house(hx, hz, (fid * 31 + k) & 0xFFFFFFFF, self.act["ruination"] * 0.85)
            elif v < 0.82:  # a barn/shed
                self.place_ground("shed", hx, hz, self.rnd(fid, 530 + k) * math.tau,
                                  1.2 + self.rnd(fid, 560 + k) * 0.7)
            else:           # a garage/outbuilding
                self.place_ground("garage", hx, hz, self.rnd(fid, 590 + k) * math.pi, 1.0)
        # yard clutter: crates, barrels, the odd cart-load
        for k in range(2 + int(self.rnd(fid, 300) * 4)):
            key = ["crate", "crate2", "barrel", "barrel2"][int(self.rnd(fid, 310 + k) * 4) % 4]
            self.place_ground(key, cx + (self.rnd(fid, 340 + k) - 0.5) * 44.0,
                              cz + (self.rnd(fid, 370 + k) - 0.5) * 44.0,
                              self.rnd(fid, 400 + k) * math.tau)
        for k in range(1 + n // 2):  # yard trees
            self.tree(cx + (self.rnd(fid, 200 + k) - 0.5) * 50.0,
                      cz + (self.rnd(fid, 230 + k) - 0.5) * 50.0, fid * 17 + k, big=True)

    @staticmethod
    def mask_clusters(mask):
        """8-connected components of a boolean mask → list of pixel lists. Deterministic order."""
        from collections import deque
        H, W = mask.shape
        lbl = np.full(mask.shape, False)
        out = []
        for (j, i) in np.argwhere(mask):
            if lbl[j, i]:
                continue
            q = deque([(j, i)])
            lbl[j, i] = True
            pts = []
            while q:
                cj, ci = q.popleft()
                pts.append((cj, ci))
                for dj in (-1, 0, 1):
                    for di in (-1, 0, 1):
                        nj, ni = cj + dj, ci + di
                        if 0 <= nj < H and 0 <= ni < W and mask[nj, ni] and not lbl[nj, ni]:
                            lbl[nj, ni] = True
                            q.append((nj, ni))
            out.append(pts)
        return out

    def carve_channel(self, axis, coord, width, depth, meander=0.0, x_min=None):
        """Carve a drainage channel into the heightfield: a gaussian dip along a N-S ('x') or
        E-W ('z') line. Must run BEFORE placements so ground() sees the carved banks."""
        half = (self.res - 1) * 0.5
        for j in range(self.res):
            wz = (j - half) * self.spacing
            for i in range(self.res):
                wx = (i - half) * self.spacing
                if x_min is not None and wx < x_min:
                    continue
                if axis == "x":  # N-S channel at x = coord, meandering with z
                    c = coord + meander * math.sin(wz / 700.0)
                    d = wx - c
                else:  # E-W channel at z = coord
                    d = wz - coord
                self.heights[j, i] -= depth * math.exp(-(d / width) ** 2)

    def tree(self, x, z, tid, big=False):
        """Six deciduous species, random yaw, wide scale spread (kessel-map-design: variation)."""
        kind = TREE_KEYS[int(self.rnd(tid, 5) * len(TREE_KEYS)) % len(TREE_KEYS)]
        s = (1.4 if big else 0.8) + self.rnd(tid, 6) * (1.8 if big else 1.3)
        self.place_ground(kind, x, z, self.rnd(tid, 7) * math.tau, s)

    def hedgerow(self, x0, z0, x1, z1, hid):
        """A field-boundary tree line with gaps — the Oderbruch's windbreak hedges."""
        length = math.hypot(x1 - x0, z1 - z0)
        n = max(2, int(length / 14.0))
        for k in range(n):
            if self.rnd(hid, 10 + k) < 0.25:
                continue  # gap
            t = (k + 0.5) / n
            jx = (self.rnd(hid, 60 + k) - 0.5) * 6.0
            jz = (self.rnd(hid, 90 + k) - 0.5) * 6.0
            self.tree(x0 + (x1 - x0) * t + jx, z0 + (z1 - z0) * t + jz, hid * 131 + k)

    def fence_run(self, x0, z0, x1, z1, fid, key="fence_wood"):
        """Axis-aligned fence segments (4 m modules) with a gate gap somewhere."""
        length = math.hypot(x1 - x0, z1 - z0)
        n = max(1, int(length / 4.0))
        rot = 0.0 if abs(x1 - x0) > abs(z1 - z0) else math.pi / 2
        gate = int(self.rnd(fid, 3) * n)
        for k in range(n):
            if k == gate:
                continue
            t = (k + 0.5) / n
            x, z = x0 + (x1 - x0) * t, z0 + (z1 - z0) * t
            self.place_ground(key, x, z, rot)

    def house(self, cx, cz, bid, r):
        """A small intact-ish village house: 2-3 × 3 cells, one storey, pitched roof, fenced
        garden. Reuses wall_seg so ruination still bites (holes, the odd collapsed segment)."""
        base = self.ground(cx, cz) - 0.3
        fcx = 2 + int(self.rnd(bid, 2) * 2)  # 2..3
        fcz = 3
        hx, hz = fcx * CELL * 0.5, fcz * CELL * 0.5
        for i in range(fcx):
            wx = cx - hx + (i + 0.5) * CELL
            self.wall_seg(wx, base, cz - hz, 0.0, bid, i, r, True)
            self.wall_seg(wx, base, cz + hz, 0.0, bid, 10 + i, r, True)
        for j in range(fcz):
            wz = cz - hz + (j + 0.5) * CELL
            self.wall_seg(cx - hx, base, wz, math.pi / 2, bid, 20 + j, r, True)
            self.wall_seg(cx + hx, base, wz, math.pi / 2, bid, 30 + j, r, True)
        for gi in range(fcx):
            for gj in range(fcz):
                self.place("floor", cx - hx + (gi + 0.5) * CELL, base, cz - hz + (gj + 0.5) * CELL)
        if self.rnd(bid, 9) > 0.35 + 0.55 * r:  # few structures fully standing (kessel-map-design)
            for gi in range(fcx):
                for gj in range(fcz):
                    self.place("roof_pitch", cx - hx + (gi + 0.5) * CELL, base + WALL_H - 0.05,
                               cz - hz + (gj + 0.5) * CELL)
        # a couple of furniture pieces inside
        for f in range(2):
            fx = cx - hx + CELL * 0.8 + self.rnd(bid, 1700 + f) * (2 * hx - CELL * 1.6)
            fz = cz - hz + CELL * 0.8 + self.rnd(bid, 1750 + f) * (2 * hz - CELL * 1.6)
            kind = ["furn_bed", "furn_table", "furn_chair", "furn_cabinet"][int(self.rnd(bid, 1800 + f) * 4) % 4]
            self.place(kind, fx, base + 0.05, fz, self.rnd(bid, 1850 + f) * math.tau)
        # fenced garden plot behind (north side), with a tree or two
        gz0, gz1 = cz - hz - 14.0, cz - hz
        self.fence_run(cx - hx, gz0, cx + hx, gz0, bid * 7 + 1)
        self.fence_run(cx - hx, gz0, cx - hx, gz1, bid * 7 + 2)
        self.fence_run(cx + hx, gz0, cx + hx, gz1, bid * 7 + 3)
        if self.rnd(bid, 40) < 0.7:
            self.tree(cx + (self.rnd(bid, 41) - 0.5) * 2 * hx, gz0 + 4.0 + self.rnd(bid, 42) * 8.0,
                      bid * 13 + 5)

    def village(self, cx, cz, n_houses, vid, r):
        """Houses along an E-W street through (cx, cz), alternating sides."""
        for k in range(n_houses):
            side = 1 if k % 2 == 0 else -1
            x = cx + (k // 2 - n_houses // 4) * 42.0 + (self.rnd(vid, 20 + k) - 0.5) * 10.0
            z = cz + side * (16.0 + self.rnd(vid, 50 + k) * 6.0)
            self.house(x, z, (vid * 31 + k) & 0xFFFFFFFF, r + (self.rnd(vid, 80 + k) - 0.5) * 0.2)
        # village trees along the street
        for k in range(n_houses):
            self.tree(cx + (k - n_houses / 2) * 38.0, cz + (self.rnd(vid, 200 + k) - 0.5) * 8.0,
                      vid * 17 + k, big=True)

    def manor(self, x, z):
        """The Old House — the leveled home at the heart of the act. A big two-storey house at
        r≈0.9 (mostly rubble and standing chimney-corners), a barn, a walled courtyard, and the
        orchard still blooming in rows: the cruel spring contrast."""
        bid = 0xA1FE
        self.building(x, z, bid, 0.9)
        self.place_ground("shed", x + 26.0, z + 8.0, math.pi / 2, 1.8)  # the barn
        # courtyard wall (concrete fence ring with gaps where it fell)
        for k in range(10):
            if self.rnd(bid, 500 + k) < 0.3:
                continue
            self.place_ground("fence_concrete", x - 18.0 + k * 4.0, z - 20.0, 0.0)
        for k in range(8):
            if self.rnd(bid, 550 + k) < 0.3:
                continue
            self.place_ground("fence_concrete", x - 18.0, z - 20.0 + k * 4.0, math.pi / 2)
        # the orchard: regular 7 m rows SW of the house — instantly readable from the turret
        for gi in range(5):
            for gj in range(4):
                ox = x - 45.0 + gi * 7.0 + (self.rnd(bid, 600 + gi * 8 + gj) - 0.5) * 1.5
                oz = z + 18.0 + gj * 7.0 + (self.rnd(bid, 650 + gi * 8 + gj) - 0.5) * 1.5
                self.place_ground("tree_a", ox, oz, self.rnd(bid, 700 + gi * 8 + gj) * math.tau, 1.3)
        # rubble spill
        for k in range(8):
            a = self.rnd(bid, 800 + k) * math.tau
            d = 8.0 + self.rnd(bid, 850 + k) * 18.0
            rx, rz = x + math.cos(a) * d, z + math.sin(a) * d
            kind = ["rubble_field", "rubble_stack", "brick"][int(self.rnd(bid, 900 + k) * 3) % 3]
            self.place(kind, rx, self.ground(rx, rz) - 0.1, rz, self.rnd(bid, 950 + k) * math.tau,
                       0.9 + self.rnd(bid, 990 + k) * 0.5)

    def windmill(self, x, z):
        """Bockmühle blockout: a 4-storey 2×2-cell tower + pitched cap. Hero-modeled later
        (assets/hero/act2/the_windmill.glb replaces it in-engine)."""
        base = self.ground(x, z) - 0.3
        half = CELL * 0.5
        for s in range(4):
            y = base + s * WALL_H
            self.place("wall", x, y, z - half, 0.0)
            self.place("wall", x, y, z + half, 0.0)
            self.place("wall", x - half, y, z, math.pi / 2)
            self.place("wall", x + half, y, z, math.pi / 2)
            self.place("floor", x, y, z)
        self.place("roof_pitch", x, base + 4 * WALL_H, z)

    def bridge(self, x, z):
        """Plank deck over the canal: floor slabs spanning E-W, fence rails both sides."""
        bank = max(self.ground(x - 30.0, z), self.ground(x + 30.0, z))
        n = 12  # 48 m of deck, clearing the carved banks
        for k in range(n):
            dx = (k - n / 2 + 0.5) * CELL
            self.place("floor", x + dx, bank + 0.1, z)
        for k in range(n):
            dx = (k - n / 2 + 0.5) * CELL
            self.place("fence_wood", x + dx, bank + 0.15, z - 2.2, 0.0)
            self.place("fence_wood", x + dx, bank + 0.15, z + 2.2, 0.0)

    def generate_spring(self):
        a = self.act
        r = a["ruination"]
        he = a["half_extent"]
        # 1. CLASSIFY the real spring imagery; masks drive terrain shaping AND placements.
        m = self.classify_imagery()
        H, W = self.img_res
        clear = m["keep_clear"]
        clusters = [pts for pts in self.mask_clusters(m["bldg"]) if len(pts) >= 3]
        centroids = []
        for pts in clusters:
            cj = sum(p[0] for p in pts) / len(pts)
            ci = sum(p[1] for p in pts) / len(pts)
            centroids.append(self.px_to_world(ci, cj))
        # 2. TERRAIN SHAPING (before ANY placement samples ground):
        #    berms/hollows → shelled-out craters → graded road net → the canal.
        self.micro_relief(amp1=a.get("relief_amp", 1.3))
        self.add_craters(int(220 + r * 500))
        nodes = [tuple(a["spawn"]), tuple(a["goal"])]
        nodes += [(lx, lz) for (_, lx, lz, _) in a["landmarks"]]
        nodes += centroids
        self.build_road_network(nodes)
        if a.get("canal"):
            self.carve_channel("x", CANAL_X, 14.0, 2.4, meander=CANAL_MEANDER)
            for dz in (-1400.0, -200.0, 1000.0):
                self.carve_channel("z", dz, 8.0, 1.1, x_min=CANAL_X + 30.0)
        # 3. HEDGEROWS on the actual field boundaries (20 m sampling stride, jittered).
        for j in range(0, H, 2):
            for i in range(0, W, 2):
                if not m["hedge"][j, i] or clear[j, i]:
                    continue
                hid = (0x1000 + j * W + i) & 0xFFFFFFFF
                if self.rnd(hid, 1) < 0.30:
                    continue
                x, z = self.px_to_world(i, j)
                x += (self.rnd(hid, 2) - 0.5) * 14.0
                z += (self.rnd(hid, 3) - 0.5) * 14.0
                if self.road_dist(x, z) < 6.0:
                    continue
                self.tree(x, z, hid)
        # 4. WOODS on the actual copses / scarp canopy, with UNDERSTORY (ferns, logs, stumps).
        wstride = a.get("woods_stride", 2)
        wkeep = a.get("woods_keep", 0.88)
        for j in range(0, H, wstride):
            for i in range(0, W, wstride):
                if not m["woods"][j, i]:
                    continue
                wid = (0x2000 + j * W + i) & 0xFFFFFFFF
                if self.rnd(wid, 1) > wkeep:
                    continue
                x, z = self.px_to_world(i, j)
                x += (self.rnd(wid, 2) - 0.5) * 12.0
                z += (self.rnd(wid, 3) - 0.5) * 12.0
                if self.road_dist(x, z) < 5.0:
                    continue
                v = self.rnd(wid, 4)
                pf = a.get("pine_frac", 0.40)  # Halbe is pine country; the Oderbruch is mixed
                if v < pf:
                    self.place_ground("pine" if v < pf * 0.5 else "pine2", x, z,
                                      self.rnd(wid, 5) * math.tau, 0.8 + self.rnd(wid, 6) * 1.3)
                else:
                    self.tree(x, z, wid, big=True)
                u = self.rnd(wid, 9)
                if u < 0.22:  # forest-floor understory beside the tree
                    ukey = ["fern", "fern2", "log", "stump"][int(u * 40.0) % 4]
                    self.place_ground(ukey, x + (self.rnd(wid, 10) - 0.5) * 9.0,
                                      z + (self.rnd(wid, 11) - 0.5) * 9.0,
                                      self.rnd(wid, 12) * math.tau, 0.9 + self.rnd(wid, 13) * 0.8)
        # 5. PASTURE tufts on the actual green fields (the dark winter cereal stays flat).
        for j in range(0, H, 4):
            for i in range(0, W, 4):
                if not m["greenfld"][j, i] or clear[j, i]:
                    continue
                gid = (0x3000 + j * W + i) & 0xFFFFFFFF
                if self.rnd(gid, 1) < 0.72:
                    continue
                x, z = self.px_to_world(i, j)
                x += (self.rnd(gid, 2) - 0.5) * 24.0
                z += (self.rnd(gid, 3) - 0.5) * 24.0
                if self.road_dist(x, z) < 5.0:
                    continue
                self.place_ground("grass", x, z, self.rnd(gid, 4) * math.tau,
                                  1.8 + self.rnd(gid, 5) * 0.8)
        # 5b. LONE FIELD TREES — sparse accents across the crop land.
        for j in range(0, H, 5):
            for i in range(0, W, 5):
                if not (m["greenfld"][j, i] or m["darkcrop"][j, i]) or clear[j, i]:
                    continue
                tid = (0x6000 + j * W + i) & 0xFFFFFFFF
                if self.rnd(tid, 1) < 0.95:
                    continue
                x, z = self.px_to_world(i, j)
                x += (self.rnd(tid, 2) - 0.5) * 30.0
                z += (self.rnd(tid, 3) - 0.5) * 30.0
                if self.road_dist(x, z) < 6.0:
                    continue
                self.tree(x, z, tid, big=True)
        # 4c. GROUND-CLASS MASK export — one source of truth for runtime ground cover: the engine
        #     (and a Blender particle/GN scatter) can sample this for grass density instead of
        #     shipping mass grass as placements. Palette: R=pasture, G=woods, B=dark crop.
        try:
            import subprocess
            H2, W2 = self.img_res
            out = np.zeros((3, H2, W2), np.uint8)
            out[0][m["greenfld"]] = 255
            out[1][m["woods"]] = 255
            out[2][m["darkcrop"]] = 255
            raw = os.path.join(os.path.dirname(self.act["tci_npy"]), "ground_class.bin")
            out.tofile(raw)
            open(raw[:-4] + ".hdr", "w").write(
                f"ENVI\nsamples = {W2}\nlines = {H2}\nbands = 3\nheader offset = 0\n"
                "file type = ENVI Standard\ndata type = 1\ninterleave = bsq\nbyte order = 0\n")
            png = raw[:-4] + ".png"
            subprocess.run(["gdal_translate", "-q", "-of", "PNG", raw, png], check=True)
            os.remove(raw)
            os.remove(raw[:-4] + ".hdr")
            print(f"[gen] ground-class mask → {png}")
        except Exception as e:  # gdal is a dev-box tool; the mask is optional
            print(f"[gen] ground-class mask skipped: {e}")
        # 6. FARMSTEADS + VILLAGES on the actual building clusters (≥3 structures each).
        for pts in clusters:
            self.farmstead(pts)
        print(f"[gen] imagery: {len(clusters)} building clusters → farmsteads/villages")
        # 7. ROAD DRESSING: asphalt slabs through the towns, poles + wires along every road.
        self.dress_roads()
        # 8. WAR SCARS: battle sites + abandoned logistics dumps, biased toward the road net.
        segs = [s for s in self.roads if math.hypot(s[2] - s[0], s[3] - s[1]) > 150.0]
        for k in range(a.get("battle_sites", 3)):
            sid = 0xBA77 + k * 131
            s = segs[int(self.rnd(sid, 0) * len(segs)) % len(segs)] if segs else [0, 0, 0, 0]
            t = 0.2 + self.rnd(sid, 1) * 0.6
            bx = s[0] + (s[2] - s[0]) * t + (self.rnd(sid, 2) - 0.5) * 160.0
            bz = s[1] + (s[3] - s[1]) * t + (self.rnd(sid, 3) - 0.5) * 160.0
            self.battle_site(bx, bz, sid)
        for k in range(a.get("logistics_sites", 2)):
            sid = 0x106B + k * 173
            s = segs[int(self.rnd(sid, 0) * len(segs)) % len(segs)] if segs else [0, 0, 0, 0]
            t = 0.25 + self.rnd(sid, 1) * 0.5
            lx = s[0] + (s[2] - s[0]) * t + (self.rnd(sid, 2) - 0.5) * 90.0
            lz = s[1] + (s[3] - s[1]) * t + (self.rnd(sid, 3) - 0.5) * 90.0
            self.logistics_site(lx, lz, sid)
        # 9. BOULDERS (hard cover) + small rock scatter.
        self.boulders_and_rocks(int(he / 45.0), int(he / 14.0))
        # 10. CRATER DEBRIS — some shell holes keep their thrown-out spoil.
        for k, (cx_, cz_, cr, _cd) in enumerate(self.craters):
            if self.rnd(0xC0DE + k, 1) < 0.18:
                self.place("rubble_field", cx_ + cr * 0.7, self.ground(cx_ + cr * 0.7, cz_) - 0.1,
                           cz_, self.rnd(0xC0DE + k, 2) * math.tau, 0.7 + self.rnd(0xC0DE + k, 3) * 0.5)
        # 11. LANDMARKS at the act's navigation positions (the world.rs contract), each with a
        #     parked Tiger for scale (east bank offset at the bridge — not in the canal).
        for li, (kind, x, z, name) in enumerate(a["landmarks"]):
            self.landmark(kind, x, z, name)
            dx, dz = (44.0, 16.0) if kind == "Bridge" else (26.0, 18.0)
            self.tiger(x + dx, z + dz, self.rnd(0x71E0 + li, 1) * math.tau)
        # 12. spawn + goal markers
        sx, sz = a["spawn"]
        gx, gz = a["goal"]
        self.markers.append(dict(name="SPAWN", kind="Spawn", x=sx, z=sz, y=self.ground(sx, sz)))
        self.markers.append(dict(name="ZIEL (rotes X)", kind="Goal", x=gx, z=gz, y=self.ground(gx, gz)))

    # ── generation driver ──
    def generate(self):
        if self.act.get("gen") == "spring":
            self.generate_spring()
            return
        a = self.act
        r = a["ruination"]
        he = a["half_extent"]
        # 0. TERRAIN SHAPING (worldgen v2): berms/hollows + the shelled-out city approaches, and
        #    a simple road net between the landmarks (the city streets grade themselves).
        self.micro_relief(amp1=1.1)
        self.add_craters(int(220 + r * 500))
        nodes = [tuple(a["spawn"]), tuple(a["goal"])] + [(lx, lz) for (_, lx, lz, _) in a["landmarks"]]
        self.build_road_network(nodes)
        self.town_pts = [(600.0, 100.0)]  # the district core gets asphalt
        # 1. THE CITY DISTRICT: dense blocks around the map centre-east (the start of the escape).
        for bj in range(-7, 8):
            for bi in range(-7, 8):
                cx = 600.0 + bi * PITCH
                cz = 100.0 + bj * PITCH
                bid = ((bj & 0xFFFF) * 73856093 ^ (bi & 0xFFFF) * 19349663) & 0xFFFFFFFF
                # thin out toward the district edge for a ragged, shelled skyline
                edge = max(abs(bi), abs(bj)) / 7.0
                if self.rnd(bid, 1) < 0.25 * edge:
                    continue
                self.building(cx, cz, bid, min(1.0, r + 0.15 * edge))
        # 2. WEST FRINGE: villages + pines thinning toward the goal (the city gives way to country).
        for k in range(60):
            x = -600.0 - hash01(self.seed ^ (k * 7919)) * 1400.0
            z = (hash01(self.seed ^ (k * 104729)) - 0.5) * 3600.0
            v = hash01(self.seed ^ (k * 65537))
            if v < 0.30:
                bid = (0xB00 + k) & 0xFFFFFFFF
                self.building(x, z, bid, r * 0.6)
            elif v < 0.45:
                self.place("shed", x, self.ground(x, z) - 0.2, z, hash01(k * 31) * math.tau)
            else:
                self.place("pine" if v < 0.8 else "pine2", x, self.ground(x, z) - 0.1, z,
                           hash01(k * 17) * math.tau, 0.8 + hash01(k * 13) * 0.7)
        # 3. LANDMARKS at the act's navigation positions, each with a parked Tiger for scale.
        for li, (kind, x, z, name) in enumerate(a["landmarks"]):
            self.landmark(kind, x, z, name)
            self.tiger(x + 26.0, z + 18.0, self.rnd(0x71E0 + li, 1) * math.tau)
        # 4. THE BUNKER: under the factory edge of the district.
        self.bunker(320.0, -420.0)
        # 5. WAR SCARS (worldgen v2): roads dressed, battle sites, a shelled supply dump, cover.
        self.dress_roads()
        segs = [s for s in self.roads if math.hypot(s[2] - s[0], s[3] - s[1]) > 150.0]
        for k in range(a.get("battle_sites", 2)):
            sid = 0xBA77 + k * 131
            s = segs[int(self.rnd(sid, 0) * len(segs)) % len(segs)] if segs else [0, 0, 0, 0]
            t = 0.2 + self.rnd(sid, 1) * 0.6
            self.battle_site(s[0] + (s[2] - s[0]) * t + (self.rnd(sid, 2) - 0.5) * 160.0,
                             s[1] + (s[3] - s[1]) * t + (self.rnd(sid, 3) - 0.5) * 160.0, sid)
        for k in range(a.get("logistics_sites", 1)):
            sid = 0x106B + k * 173
            s = segs[int(self.rnd(sid, 0) * len(segs)) % len(segs)] if segs else [0, 0, 0, 0]
            t = 0.25 + self.rnd(sid, 1) * 0.5
            self.logistics_site(s[0] + (s[2] - s[0]) * t + (self.rnd(sid, 2) - 0.5) * 90.0,
                                s[1] + (s[3] - s[1]) * t + (self.rnd(sid, 3) - 0.5) * 90.0, sid)
        self.boulders_and_rocks(int(he / 55.0), int(he / 18.0))
        # 6. spawn + goal markers
        sx, sz = a["spawn"]
        gx, gz = a["goal"]
        self.markers.append(dict(name="SPAWN", kind="Spawn", x=sx, z=sz, y=self.ground(sx, sz)))
        self.markers.append(dict(name="ZIEL (rotes X)", kind="Goal", x=gx, z=gz, y=self.ground(gx, gz)))

    def write(self, outdir):
        os.makedirs(outdir, exist_ok=True)
        np.save(os.path.join(outdir, "heights.npy"), self.heights)
        terrain = dict(res=self.res, spacing=self.spacing, heights="heights.npy")
        if "color" in self.act:
            terrain["color"] = list(self.act["color"])
        if "texture" in self.act:
            terrain["texture"] = self.act["texture"]
        if "gamma" in self.act:
            terrain["gamma"] = self.act["gamma"]
        if "saturation" in self.act:
            terrain["saturation"] = self.act["saturation"]
        manifest = dict(
            schema=MANIFEST_SCHEMA, version=MANIFEST_VERSION,
            act_id=self.act_id,
            act=self.act["name"], seed=self.seed,
            terrain=terrain,
            assets=self.assets, placements=self.placements, markers=self.markers,
            wires=self.wires,
        )
        if "overview" in self.act:
            manifest["overview"] = self.act["overview"]
        with open(os.path.join(outdir, MANIFEST_FILE), "w") as f:
            json.dump(manifest, f)
        # sidecar consumed by make_ground_tex.py (dirt-road + crater-scorch stamping)
        with open(os.path.join(outdir, "features.json"), "w") as f:
            json.dump(dict(roads=self.roads, craters=self.craters,
                           town_pts=[[round(x, 1), round(z, 1)] for x, z in self.town_pts]), f)
        print(f"[gen] act '{self.act['name']}': {len(self.placements)} placements, "
              f"{len(self.assets)} unique assets, {len(self.roads)} road segs, "
              f"{len(self.craters)} craters, {len(self.wires)} wire spans, "
              f"terrain {self.res}² @ {self.spacing:.1f} m → {outdir}/{MANIFEST_FILE}")


if __name__ == "__main__":
    act_id = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    outdir = sys.argv[2] if len(sys.argv) > 2 else "worldgen_out"
    w = World(act_id, ACTS[act_id])
    w.generate()
    w.write(outdir)
