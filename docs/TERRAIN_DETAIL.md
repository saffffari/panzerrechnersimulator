# Terrain detail (branch `terrain-detail`, 2026-10-27)

The Canyon v2 ground read smooth and flat because nothing below 4.9 m existed in its geometry:
one 1025² grid at 4.883 m, drawn whole. This branch replaces it, on the raymarch backend, with
the native Gaea surface, sub-metre relief, persistent ruts and craters, and a quadtree LOD; the
physics reads the same surface.

## 1. What the source really carries

| | value |
|---|---|
| Gaea project | `E:\gaea_maps\panzer\canyon\canyon1.terrain` (Gaea 2.0.6, build resolution 4096, tiles 1024) |
| Height export | `Export.png`: 4096² px, 16-bit, 5000 m a side → **1.221 m per pixel** |
| Vertical | 0..2500 m over 65535 steps → **3.8 cm quantisation**; 10 316 distinct values used |
| Graph | `Canyon → Erosion2 → Snowfield → Export`: the exported height is the Snowfield's output, i.e. already a snow surface |
| Old runtime grid | `heights.npy` 1025² at 4.883 m: a bilinear 4× downsample of the export (`gaea_canyon_run.py`) |
| New runtime grid | `detail/heights.4097.npy` 4097² at **1.2207 m**: the native export, one node per source pixel |

The export's own detail over the old grid: residual std 0.16 m, p99 0.41 m, max 50 m (cliffs).
That is what the 1025 grid threw away. **Nothing finer than 1.22 m exists in the source.**
An 8193² (0.61 m) grid would need a Gaea rebuild at 8192; Gaea's erosion is resolution
dependent, so that rebuild is a different terrain (the course route, pads and every placement
would move) and was not done. Everything below 1.22 m on this branch is procedural or
event-driven relief, named as such below.

Dequantisation: a 3×3 binomial average held inside each sample's own ±½-step interval (max
change 1.9 cm), so flat valley floors do not show 3.8 cm terraces and every height stays
consistent with the source value. Authored edits of the course (building pads, receipt craters,
travel-strip blends, 9 916 coarse nodes) are carried over from `heights.npy` at their authored
4.88 m definition; the 192 605 fine nodes inside them are flagged *held* and receive no
procedural relief, so foundations stand where they were cooked (`terrain under the footprint
spans` warnings: 15 before, 15 after).

Cook: `python tools/worldgen/canyon_terrain_detail.py --root <asset root>` (≈30 s), verifies the
export's sha256 against the course receipt and the runtime datum, writes
`detail/{heights.4097.npy, surface.4097.png, terrain-detail.receipt.json}` and adds
`terrain.detail` to `manifest.v1.json` (existing parsers ignore it; `heights.npy` stays the
fallback; `KESSEL_TERRAIN_DETAIL=off` selects it at runtime).

## 2. Snow depth and ground class (cooked per 1.22 m node)

Snow depth is a real quantity in metres, derived rather than painted:

* settled cover 0.40 m wherever Gaea's snow mask is set (the act's declared snowpack, same value
  as `vehicle_ground`'s winter sinkage);
* redistributed by the **Winstral maximum upwind slope Sx** over 100 m against the act's
  prevailing wind (`world::weather_wind_profile(1)`, toward +x/+z), computed on the surface plus
  box envelopes of the 249 buildings and 300 wrecks: lee deposition above a 2.5° horizon
  (0.40 m per 5°), so a drift behind an obstacle of height h ends about 20 h downwind; windward
  scour 0.40 m per 15° of exposure; spread 2 m across the wind; clamped 0..2 m;
* no snow on faces steeper than repose (normal_y 0.70 → 0.40, terrain_material's rule);
* the course's road compacted to 8 cm.

The redistribution changes the surface by its departure from the settled cover (−0.40..+1.60 m;
drifts ramp up over ~3 m from an obstacle instead of standing as a wall).

Ground class (R channel, bit 7 = held): snow 91.3 %, wind crust 3.4 %, rock/loess 4.7 %
(scoured or steeper than ~57°), frozen mud 0.14 % (the course road), rubble 0.27 % (around the
ruined buildings), trampled yards 0.27 %. G = depth (cm), B = signed course offset, A = Sx.

## 3. The surface everyone reads (`src/terrain_relief.rs`)

`Heightfield::grade_at` delegates to `Detail::height_at`, so tanks (`vehicle_height_at`), AI,
infantry grounding, shell terrain contacts and walkers all read it. It is the **0.076 m lattice**
(grid / 16), each cell split along the same (i+1, j)–(i, j+1) diagonal as before, its nodes =

`base (native triangle surface) + relief (octaves 0.31–9.77 m) + craters − ruts`.

* **Relief** per class, octave k of wavelength 1.2207 m·2^k, quintic value noise on an integer
  hash: snow waves standing across the wind (2.5:1, scaled by exposure and available depth),
  sharper wind-crust sastrugi, ridged rock, rubble heaps, frozen mud with the two old ruts of
  prior traffic at ±1.45 m of the course centreline, pocked trampled yards. Octave k has
  wavelength 1.2207 m·2^k, k = −3..3 (0.15–9.77 m; amplitudes 0.2–15 cm); a LOD level carries an
  octave as geometry when it has four lattice cells per wavelength, so the physics surface (the
  finest level) has k = −2..3 and the 0.15 m octave is a shading normal only. Classes blend
  bilinearly between nodes; held nodes get none.
* **Ruts and prints**: 12.5 cm cells in 32 m pages, persistent for the session. Every frame each
  moving tank stamps its two tread bands (the same hull-derived bands as the renderer's track
  map) with compaction = 0.75 × local snow depth, at most 0.45 m (Tiger hull clearance), rubble
  crushes 5 cm; band edges get a berm of a quarter of that. Walkers (the dismounted player and
  every live soldier) press a 0.30 × 0.12 m boot print every 0.75 m, 0.5 × depth, at most
  0.30 m. Compaction is idempotent (it reaches the firm layer once). Stamps are anti-aliased
  (a cell takes compaction in proportion to its coverage by the band); geometry, physics and the
  shading normal read the map band-limited by a 3×3 binomial at 0.1 m. (At a 0.305 m finest
  lattice an oblique 0.3 m tread wall became a few tilted triangles and a 12° sun lit them as a
  staircase; the raw map's bilinear gradient did the same per 12.5 cm cell. Both are fixed by
  the finer lattice and the band-limited normal.)
* **Craters**: every HE round that strikes terrain digs one. Cube-root scaling R = k·W^⅓
  (W = filler as TNT; k 0.8 m/kg^⅓ frozen soil … 1.3 settled snow by local depth), parabolic
  bowl 0.4 R deep, thrown rim 0.12 of the depth falling off over 0.45 R, reach 2.2 R. 88 mm HE
  (0.698 kg) in 0.40 m snow: R ≈ 0.95 m, 0.38 m deep. The wreckage layer's authored
  `crater_scar.glb` meshes (rim decals) are not carved.
* Static node heights are cached (65 536 slots keyed exactly by lattice index, invalidated by a
  new crater); ruts are read live.

## 4. LOD (`src/terrain_lod.rs`, `shaders/rm_terrain*.wgsl`)

CDLOD (Strugar 2009) on the raymarch backend. The quadtree's drawn unit is a 16×16-cell
quadrant at lattice 0.076 m·2^L, levels L = 0..11 (1.2 m … 5000 m quadrants). A node is refined
while it meets the sphere `range(L−1)` around the camera, ranges 10 m·2^L
(`KESSEL_TERRAIN_LOD_RANGE`): 0.076 m to 10 m, 0.15 m to 20 m, 0.31 m to 40 m, 1.22 m to
160 m, 4.9 m to 640 m, 19.5 m to 2.56 km; a quadrant outside its range is drawn at its parent's
level. Every vertex
evaluates the surface function on the GPU (WGSL twin of `terrain_relief`, constants generated
from the Rust tables) at its own position. In the outer quarter of each range, odd vertices slide
onto even ones and the level's own relief octave fades, so at every level boundary both sides
are exactly the coarser level's vertices: **crack-free without skirts**. Level 0 unmorphed is
the physics surface.

Selection is once per frame from the camera; each depth view (far and near sun cascades, sky
cover, the shadowed point-light faces) and the G-buffer cull the same node set against their own
frustum, so shadows and sky occlusion see the displaced surface the camera sees. The coarse mesh
(`track_recv`) is skipped by every raymarch pass while the LOD exists. Vertex normals come from
the level's static surface; fragments add, by finite differences, the octaves the level's
vertices do not carry (each faded once its wavelength falls under four pixel footprints) and the
craters' and ruts' own slopes. Open cellars are discarded in every pass.

Resident on the GPU (no paging needed at 4097²): heights R32Float + attributes Rgba8Uint
(134 MB), rut atlas R16Float 4096² (32 MB, 256 pages of 32 m), page table, crater grid: **168 MB**.
`KESSEL_RM_SKIP` bit 262144 draws no terrain at all (cost isolation; 8192 on the terrain-detail branch before integration, where 8192 is screen-space indirect light).
Streamed: rut pages (dirty pages each frame; an LRU over distance evicts pages beyond 600 m when
the 256 slots are full; the CPU keeps every page), the crater list and grid on change, each
frame's node lists.

## 5. Material at the new scale

The G-buffer terrain material keeps the Gaea rock map and the shared `terrain_material`, but snow
cover now follows the cooked depth (rock shows below a few centimetres); wind crust is a glazed,
smoother skin; frozen mud is soil (~0.1) with snow only in the hollows; rubble is brick, mortar
and ash; trampled snow is greyer; compacted snow (ruts, prints) reflects 14 % less and is smoother
(sintered); a crater shows frozen earth in its bowl and a ragged ejecta apron to 2.2 R.

## 5b. Ground material without visible tiling (2026-10-28)

**Diagnosis.** Only one source repeats: four 256-px PSX colour tiles (grass, two soils, snow)
used as linear-reflectance ratios at **2.56 m** a repeat, sampled on axis-aligned world xz with
no rotation or offset (`terrain_material.wgsl`, shared by the raymarch, shader and physical
backends). The 1.22 m Gaea drape is georeferenced and does not tile. Where it read:

| distance (800×450, 50–58° fov) | pixel footprint | what showed |
|---|---|---|
| 3–30 m | 1–4 cm | the tile's own blotches repeating every 2.56 m: 13 % of the grass and soil tiles' contrast sits in structure broader than ~0.5 m |
| 30–250 m | 4–32 cm | a 2.56 m lattice a few pixels per repeat; on mixed snow/rock ground the soil tile also drove the snow/rock *coverage boundary* (×2.8), so the grid was in the coverage itself |
| 300 m–2 km | 0.4–2.6 m | detail still fading (unity only at footprint 2.56 m); the "drift" macro term was a sum of sinusoids (λ ≈ 160–200 m, ±1.5 % on snow, ±8 % on bare earth): regular banding |

**Fix (the standard set).**
* *Hex-tiled sampling* (Heitz & Neyret 2018; Mikkelsen 2022 "practical real-time hex-tiling"):
  each vertex of a hexagonal lattice (~0.6 tile across) draws the tile with its own hashed
  rotation and offset; the three draws blend by sharpened barycentric weight, variance-preserving
  about the neutral 0.5 so the grain keeps its contrast. Explicit mip from the pixel footprint,
  so the cell seams carry no derivative discontinuity.
* *Two octaves*: the near repeat (2.56 m) and a middle-distance repeat 5.3× larger (13.6 m, not
  a power of two, so the two never realign), each faded by its own footprint (near out by
  footprint ~0.08 m, middle by ~0.6 m), so the grain hands over to the macro layer instead of
  showing as a grid.
* *High-passed tiles*: on upload (`terrain_detail.rs`) each tile loses its structure broader than
  about half a metre (three wrapped box passes, radius 24 texels); the repeat carries grain, not
  blotches. Test: the broad share at least halves, to under 6 %.
* *Macro variation from data* (raymarch terrain): snow albedo and roughness drift with the
  cooked snow depth (deeper drifted snow +3 % brighter, smoother), wind exposure (packed snow
  −4 %, glazed, roughness −0.08) and the relief field (crests scoured and glazed, troughs softer);
  only what the data does not carry (war dust and soot) drifts as three octaves of non-periodic
  value noise (37/113/331 m). The sinusoidal drift and pasture fields are now value noise on every
  backend. Shares are declared, not measured (ledger).
* *Slope-aware projection*: faces steeper than ~40° blend the two vertical projections by their
  share of the normal (`terrain_material_n`), so cliffs are not smeared along the fall line; the
  older entry (`terrain_material`, normal_y only) blends both vertical views evenly.
* *Physical breakup*: the micro-relief shading normals (0.15–0.6 m), ruts, prints, craters and
  the scatter layer's debris all break the field further; the PS1 crush's ordered dither sits on
  top and hides residual low-contrast repetition for free.

**Cost.** Texture reads per terrain pixel, Gaea snow branch: 2 before → 12 on flat ground
(2 layers × 2 octaves × 3 hex draws), up to 36 on steep faces; the raymarch terrain adds 12–36
for the mud/rubble grain; the legacy (non-Gaea) branch 4 → 24–72. All `textureSampleLevel` at an
explicit mip from a 256² array, cache-resident. GPU time: pending (the hold below).

**Verification without a device**: the three terrain programs and the shared material on the
forward, deferred and combine entries parse and validate with naga
(`detail_terrain_programs_validate`, `campaign_terrain_material_compiles_in_forward_and_deferred_entries`,
`live_physical_environment_entries_parse_and_validate_together`); the tile tests pass.

**Pending captures** (GPU on hold): `KESSEL_SHOWCASE_SET=tiling terrain-showcase PREFIX` writes
snow / rock / mud × 3 m / 30 m / 300 m / 2 km × noon / dusk (24 stills + receipt). Before = the
same harness on the pre-change material (binary built from ee7998c + the new harness in
`../panzerrechnersystem-terrain-before`, target `target_before`), after = this branch.

## 6. Verification

`terrain-showcase PREFIX` (`src/terrain_showcase.rs`): four viewpoints day and night (near
ground at the Tiger, a crater field of five 88 mm HE rounds, the canyon wall 1.8 km away from a
28 m eye, the ruts of a 12 s drive), a JSON receipt with LOD statistics, the CPU cost of a ground
query, and the GPU surface probed at 4096 lattice nodes against `Detail::height_at`. Run with
`KESSEL_TERRAIN_DETAIL=off` for the before.

Captures (all in `captures/`, 2400×1350 from the 800×450 internal frame, raymarch backend,
HIGH preset, clear weather, 10:40 UTC day / 19:30 UTC moonlit night):

| viewpoint | before (coarse 1025², `KESSEL_TERRAIN_DETAIL=off`) | after (detail) |
|---|---|---|
| near ground at the Tiger (open exposed snow, −816, 845) | `terrain_detail_before_ground_{day,night}.png`: uniform flat snow | `terrain_detail_after_ground_{day,night}.png`: snow waves and hummocks across the foreground |
| crater field (five 88 mm HE rounds, 25 s after) | `…before_craters_*`: the rounds leave the snow untouched | `…after_craters_*`: five bowls with thrown rims, frozen earth, ejecta aprons, real shadows |
| ridge 1.8 km (28 m eye, 30° fov) | `…before_ridge2km_*` | `…after_ridge2km_*`: rock on the scoured faces, snow kept in the gullies, Gaea's stratify terraces |
| ruts after a 12 s, 45 m drive | `…before_ruts_*`: no trace of the drive on this backend | `…after_ruts_*`: two continuous 0.72 m treads compacted ~0.3 m with berms; `terrain_detail_after_rutmap.png` is the rut map (40 m square, 2.5 mm per grey level) |

Receipts `terrain_detail_{before,after}.json`. Measured with the final code:

* **Physics vs rendering**: the GPU's surface (compute probe of the WGSL function) against
  `Detail::height_at` at 4096 lattice nodes around the Tiger, the crater field, the rut midpoint
  and the drive's end: **max 0.25 mm** (half-float rut atlas; 0.05 mm before any ruts existed).
* **CPU ground query** (`Heightfield::height_at`, 4096 points over 20 m): 68 ns warm (node
  cache), 1.06 µs cold.
* **LOD selection**: 0.13–0.17 ms CPU per frame. Quadrants per level at the ground view
  `[276, 207, 211, 206, 207, 207, 207, 215, 121, 16]` (level 0 = 0.076 m); G-buffer 563 quadrants
  = 288 k triangles; all depth views together ≈ 2.0 M triangles (sky cover and the far cascade
  take most). At the ridge view: 90 k G-buffer, 0.78 M depth.
* **GPU memory**: 168 MB resident (heights 67 MB, attributes 67 MB, rut atlas 34 MB incl. page
  table, crater grid 0.2 MB) plus the per-frame instance list (≤ 64 KB). CPU: 67 MB heights +
  67 MB attributes + 256 KB per touched 32 m rut page.
* **Frame cost: not yet measured on an idle GPU.** Every run on this box shared the 4090 with
  KSP and several sibling sessions' game processes (92–100 % utilisation before the bench
  started). `terrain-bench` records per-frame GPU times of the five geometry passes; with three
  configurations (no terrain `KESSEL_RM_SKIP=8192` — 262144 since integration —, coarse mesh, detail) the contended results
  are noise larger than the effect (e.g. the no-terrain run's per-frame minimum, 6.0 ms, came
  out above the coarse mesh's, 2.8 ms; medians varied from 10 to 124 ms between runs of the same
  binary). An earlier contended run attributed up to +5.6 ms to the detail terrain at the
  ground view and +0.4 ms at the ridge view; a repeat of the same configuration showed +0.0.
  The idle measurement is owed: `KESSEL_GPU_PROFILE=1` + `terrain-bench PREFIX 400` three ways
  (receipts carry the utilisation at start).


## 7. Open items

* **Pending under the GPU hold**: the tiling before/after set (section 5b), the full
  before/after showcase is done but the material changed since (re-run `terrain-showcase` for
  the after), and the terrain cost bench.
* **Frame cost on an idle GPU** (above). If the depth views prove expensive, the levers are
  the finest range (`KESSEL_TERRAIN_LOD_RANGE`) and the sky-cover map's 19 cm texels, which see
  the 0.076 m triangles as sub-texel; both keep the occluder the same in every pass.
* **Raster and shader backends** still draw the coarse 1025² mesh (now sampled from the detail
  surface at its nodes). Between nodes it departs from the physics surface by the native
  residual (std 0.16 m) plus relief; the renderer's own track map still darkens it. Porting the
  CDLOD draw to `mesh.wgsl`/`gbuf.wgsl` is the follow-up.
* **8193² source** needs a Gaea rebuild at 8192, which changes the eroded terrain under every
  placement (section 1); not done.
* **Snow redistribution** is a static cook (Winstral Sx, declared constants); no transport over
  the run, no fresh snowfall filling ruts and craters, no sintering with time.
* **The course road** lost the painted wear the Gaea drape alpha carried (cover now follows the
  cooked 8 cm depth); it reads lighter than before. A wear/ash term in the class material.
* **Parallax** in the mid-distance is not implemented; mid-distance detail is the octaves the
  geometry omits, as shading normals.
* **`vehicle_ground` sinkage** (uncommitted on `raymarching`: a constant 0.30 m under the drawn
  surface) double-counts with real ruts when merged; it should be 0 with a detail surface, or
  read `Detail::snow_depth_at`. Noted in the shared session notes.
* The wreckage layer's `crater_scar.glb` rims are decals, not carved; HE craters during play are
  carved. Crater, rut and ground-class data are exposed to `scatter` through
  `Detail::{craters, rut_at, class_at, snow_depth_at}` and `surface.4097.png`.
* Re-running `gaea_canyon_v2.py` rewrites `manifest.v1.json` without `terrain.detail`; re-run the
  detail cook after it.
