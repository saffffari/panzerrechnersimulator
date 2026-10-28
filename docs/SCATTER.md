# The strewn field — scatter system (2026-10-27, branch `scatter`)

Everything and anything strewn across the landscape: trash, debris, bodies, rubble, shell cases,
abandoned kit, wrecked-vehicle fragments, foliage, rocks, snow, at the scale of a million placed
objects per map, drawn beside the ~10 ms raymarch frame. This is the implementation receipt:
layer format, placement rules, LOD and culling design, measured costs, open items. Branch
`scatter` off `raymarching` (which also carries the Canyon v2 wreckage layer merged from
`claude/canyon-wreckage-20261027`, whose authored wrecks the field builds around).

Code: `src/scatter_field.rs` (layers, placement, tiers), `src/renderer_scatter.rs` (GPU side),
`shaders/scatter_common.wgsl`, `scatter_cull.wgsl`, `scatter_draw.wgsl`, `scatter_hiz.wgsl`.
Tools: `tools/worldgen/scatter_layers.py` (the layers), `scatter_props.py` (procedural litter
meshes), `scatter_shots.py` (the capture set). Game hooks (`src/game.rs`): one block after the
tableau registration in the act build (cover feed, pushables, loot sockets, GPU install) and the
loot-socket union in `seed_campaign_pickups`. The raymarch backend calls the cull before its
depth passes, the draws inside its cascade, sky-cover and G-buffer passes, and the Hi-Z build
after the G-buffer. The shader backend does not draw the field (open item).

## 1. Layer format: `scatter.v1.json` + `scatter_ground.v1.png`

Written by `python tools/worldgen/scatter_layers.py [--root R] [--dest WORKSHOP] [--seed N]
[--density F]` beside the workshop manifest (`assets/worldgen/workshops/gaea-canyon-run-v2/`).
`--estimate` prints the expected count per rule without writing. The engine loads the file if it
exists beside the manifest the act was built from (`KESSEL_SCATTER=0` leaves it off).

```
schema "kessel.scatter", version 1, seed, density, wind [x,z], lane_half_m, road_half_m
keep_clear: [{x, z, r, why}]                    discs the medium/large tiers and bodies stay out of
ground: {raster, texel_m, res, classes}         R = class (0 snow, 1 road, 2 rubble, 3 scorch,
                                                5 bare rock), G = snow cover 0..255
groups: {name: [{asset, w, tier, pushable, sink, conform, flat, casualty, lay, classes}]}
rules:  [{source, group, density /m², band, orient, jitter_deg, scale [a,b], slope_max,
          side, classes, companions [{group, count [a,b], radius}], lane_clear, min_intensity,
          name}]
sources:[{kind, x, z, heading_deg, half_length, half_width, radius, intensity, flight_deg,
          side, cls, burnt, origin, condition}]
```

Source kinds and where the tool derives them (nothing is invented; every source is one of the
workshop's own authored events):

| kind | from | sheds (rules) |
|---|---|---|
| `road` | the course route, one segment per 8 m span, half width 4.2 m | cases and kit on the shoulders (and sparsely on the road), snow lumps on the verges, rubble, fragments, weapons, the dead who sat down beside the road (German along the road, Soviet fewer) |
| `crater` | `crater_scar` placements | frozen-earth clods and splinters fanning outward (radial orientation), thrown snow, kit, the occasional dead |
| `scorch` | `scorched_earth` placements | burnt boards and splinters |
| `wreck` | tank, lorry, cart, sledge, horse, 45 mm and parapet placements (base + overlay), with class, side and burnt flag | fragments radially (plate, track links, wheels, scrap), kit, crates, the bailed crew by nationality, snow |
| `gun` | the 45 mm pieces (`45mm`) and every tank wreck (`76mm` Soviet, `88mm` German) | fired cases of that calibre in the ejection sector relative to the heading, boxes to the rear, small-arms cases |
| `tree` | tree, pine, stump and log placements with a canopy radius | dry grass through the snow, stones, snow lumps |
| `obstacle` | walls, fences, outposts, wrecks, buildings with heading and extents | drift tails and lumps on the lee side of the layer's wind vector |
| `fighting` | the wreckage layer's scenes (by route fraction and side), the population clusters, the garrison posts; intensity per kind, flight direction along the route (German) or against it (Soviet), toward the post for a held position | bodies oriented by the direction of flight (30 % fell backward), cases, dropped weapons and helmets, kit, clods and splinters of shellfire, barricades at the intense sites |
| `casualty` | the generator's and the wreckage layer's authored bodies | blood, kit, weapons and cases around each |
| `building` | the building instances (oriented footprints) | the rubble apron (masonry, bricks, boards radially), clods, household objects, snow |

Bands are given relative to the source's extents: a rect band `along [[k0,c0],[k1,c1]]`,
`lateral [[k0,c0],[k1,c1]]` means along ∈ [−(k0·hl+c0), k1·hl+c1] and lateral ∈ [k0·hw+c0,
k1·hw+c1] on one side (both when mirrored; the lee side of the wind when not); an annulus band
`r0`, `r1` is k·radius+c with an optional sector [centre°, half-angle°] relative to the heading.

Groups (218 entries over 211 prototypes, `scatter_layers.py::groups()`): small-arms cases,
kit (luggage, packages, tins, bottles, plates, tools, boards, mess tins, canteens, gas-mask
canisters, greatcoats), crates and boxes and barrels, weapons (rifles, helmets, stretcher),
rubble (bricks, stacked bricks, debris fields, gravel, concrete, masonry chunks, planks, scrap,
pipes), clods, shards, fragments (plate, track links, wheels, gears, springs, bolts), foliage
(grass, wheat, logs, stumps), rocks (stones; the 2–3.6 m stones as large tier), snow lumps,
drift tails, blood decals, Soviet bodies (the terminal death frames of the 17-pose character and
the six canyon casualty bakes), German bodies (the relaxed static bake laid on its back or side,
crouch frames on their side, the canyon bakes), 45/76/88 mm cases, furniture, barricades.
Anachronistic kit items (phones, keycards, cassettes, dosimeters, modern pistols) are excluded;
nothing readable is placed. Provenance and the class of each source are recorded in the file.

The procedural props (`assets/authoring/scatter/`, receipt `scatter-props.receipt.json`): fired
cases of 7.92, 7.62, 45, 76 and 88 mm (gross dimensions only), splinters, clods, masonry,
blood decals, M40 and SSh-40 helmet shapes, mess tin, canteen, gas-mask canister, Tiger and
T-34 track links, stretcher, plate fragments, burnt boards, snow lumps and drift tails.
Inferred composite shapes, not replicas.

## 2. Placement (`scatter_field.rs`)

Deterministic per 32 m cell at load, on every core (24 threads here), from the layers and the
heightfield; a run is reproducible from the seed. For each (source, rule) whose band overlaps
the cell: candidates = density · intensity · area(band bbox ∩ cell), sampled uniformly in the
bbox and accepted inside the band (so the density is per m² of band exactly). Each accepted
point:

- picks a prototype by weight, a scale in the rule's range, a yaw by the rule's orientation
  mode (random; along the source heading, either way; radial from the source centre; the
  source's flight direction, 30 % reversed; the wind), with the rule's jitter;
- rests on the heightfield's drawn triangle surface (`Heightfield::height_at`) with the local
  normal from finite differences at the object's own radius; conforms to the slope by the
  group's `conform` (1 = lies on it); is rejected above the rule's slope limit;
- reads the ground raster: rejected when its class is not allowed by the rule or the group
  (snow lumps and drifts only on snow); sinks into the contact by the group's `sink` fraction
  of its height, scaled by the snow cover (0.5 + 0.5·cover);
- keeps clear of the drivable lane (12 m half width) when the rule says so or when it is
  medium/large tier or a body, of building interiors, of the spawn and goal discs, and of other
  heavy pieces placed earlier in the same cell (overlap test among medium/large and bodies);
- places its companions (blood 1–2 and kit, weapons, cases around every body);
- is packed to 32 bytes: position, unit quaternion (4 × snorm16), f16 scale | u16 prototype,
  flags (tier, casualty, flat, ground class, hash), cell.

Tiers are decided by mass and size, not by the group's label (`effective_tier`,
`estimate_mass_kg`; feedback of 2026-10-28: a Tiger must not rock over a tin). Each prototype
gets an estimated mass from its bounding box and a material density with a fill fraction keyed
by its name (stone and masonry 1800 kg/m³ at 55 %, wood 600 at 60 %, crates and furniture 450
at 25 %, hollow or thin steel 7800 at 8 %, snow 250 at 50 %, a body 75 kg). Then:

| threshold | effect |
|---|---|
| height < 0.15 m or mass < 5 kg | **crushable**: no vehicle collision at all, whatever the group says (tins, cases, helmets, bricks, kit); the track passes over it |
| authored large but mass < 400 kg or height < 0.45 m | **cover only** (a gravel pile the track rides over; a supply crate becomes a pushable) |
| mass ≥ 400 kg and height ≥ 0.45 m | **heavy static** (boulders, logs, barricades, pallets): static mass for the hull with a resistance scaled by its own mass, never a wall |
| woody (branch, twig, sapling, stick, pole, slat, plank, board, fence, log) under 400 kg | **crushable**, whatever its size (2026-10-28, "tiny branches are no match, you just get stuck"); a log's mass is a green-wood cylinder (650 kg/m³) along its longest extent at 85 % of its smallest, so branch stubs do not count |
| woody, authored large, 400 kg or more | **a real log**: heavy static, but it resists by rolling (0.15·m·g), not sliding |

The tier is decided per placement from the scaled mass and height, so the same kit log is
crushed at scale 1 (≈ 380 kg) and rolls at scale 1.3 (≈ 840 kg).

**Heavy static** pieces join `story_cover` (shell and foot contact, LOS) at the end of the list;
the hull's push treats that range with a friction resistance of 0.6·m·g (0.15·m·g for a log)
over a 1 m shove
(`tank_push::press`): the tank's momentum pays the work, and when it is done the piece slides
1 m along the push (its GPU instance and its cover box move; `Renderer::scatter_shove`) and the
bow follows. A 2 t boulder costs a Tiger at 1 m/s about 40 % of its speed; a 420 kg log a few
per cent. **Medium** (crates, barrels, fences, benches, stretchers, concrete, stones of
1–1.5 m) are LOS cover only (`occluders`); the pushable ones among them move with their own
mass under the existing lightweight pushable model (the tank keeps m_v/(m_v+m_p) of its speed,
0.9997 for a 15 kg jerrycan). **Small** is visual.
Pushable group entries whose path resolves to a pushable profile (`manifest_scatter_profile`)
and lie within 40 m of the route go through the existing lightweight pushable path
(`scatter::Scatter::register_pages`, drawn by the CPU-paged instancing, pushed and crushed by
the Tiger as before); the rest of the field is static. Bodies offer loot sockets: the pickup
seeding unions them with the act's authored sockets. Caps (named limits, nearest the route
kept): 12 000 large cover boxes, 3 000 medium occluders, 1 500 pushables, 400 loot sockets.

Canyon v2 result (seed `0x20261027`, density 1.0; `[scatter] field:` log line):

| | |
|---|---|
| placed | 844 325 (expected 986 k before rejections; `--density` scales it) |
| by tier | 827 094 small, 12 791 medium, 4 440 large |
| bodies | 1 672 new (plus the 978 authored ones that get blood and kit); 9 530 companions |
| by group | cases 209 k, rubble 112 k, kit 108 k, clods 100 k, shards 89 k, foliage 59 k, fragments 50 k, snow 41 k, rocks 31 k, weapons 18 k, blood 6 k, crates 4.9 k, 76/88/45 mm cases 4.4/4.1/3.1 k, furniture 2 k, German bodies 1.2 k, drifts 643, Soviet bodies 509, barricades 186 |
| rejected | slope 10 k, class 36 k, lane 19 k, building 33 k, keep-clear 36, overlap 8 k (band rejection sampling 447 k) |
| cells | 784 of 32 m |
| build | 453 ms on 24 threads |
| cover | 4 440 large (all) + 3 000 medium (of 12 791) |
| pushables | 1 500 (of 29 307 eligible) |
| loot sockets | 335 |

The first layer file of the session placed 930 k (more snow lumps and crater clods, the cliff
boulders and 9 m branch cards in two groups); the retune after the first captures brought it to
the figures above. A million in the data is `--density 1.2`; the GPU side is sized by the count.

## 3. GPU side (`renderer_scatter.rs`, `shaders/scatter_*.wgsl`)

Resident data: the packed instances (32 B each), the cells (32 B: bounds, first, count), the
prototype table (96 B), one mega vertex/index buffer holding each prototype's near mesh and its
reduced mesh, and three texture-array pages (128², 256², 512² layers, full mips, nearest
sampling like the rest of the kit) assigned by each prototype's source texture size.

LODs, chosen per instance per frame by the projected diameter of its bounding sphere in native
pixels: **near** (the source mesh) above 28 px, **mid** (a grid-clustered reduction of the mesh,
6 cells across its extent, degenerate triangles dropped; the source itself when it is already
under 50 triangles) above 5 px, **pixel** (one native pixel at the object's centre and depth,
carrying the prototype's mean linear albedo, normal up) above 0.8 px; below that the object is
not drawn. The pixel LOD is the PS1 crush's own answer to the far field: a 10 cm cartridge case
is one pixel at 77 m and nothing beyond; a body stays a pixel to 1.4 km.

Cull, one compute pass of four dispatches for all four views (far cascade, near cascade,
sky-cover map, camera):
1. cells: frustum (plane test of the cell AABB) and distance;
2. instances: cell flag, tier filter (shadow views take the large tier only), bounding-sphere
   frustum test, distance, projected size → LOD, and for the camera the Hi-Z occlusion test
   against the previous frame's depth pyramid under the previous camera (max-reduced mips; the
   sphere's nearest depth against the farthest depth of the ≤ 2×2 texels its screen rectangle
   covers at the matching mip); survivors count into a (prototype, LOD) bucket;
3. scan: an exclusive prefix over the buckets of each view (one workgroup per view) that also
   writes the indirect draw arguments (index range of the prototype's LOD, instance count,
   first instance = bucket offset);
4. write: compaction of the surviving instance indices into the visible list, LOD in the top
   two bits.

Draws: buckets are ordered by texture page, so each view draws its whole field with one
`multi_draw_indexed_indirect` per page (three calls). The vertex program reads the visible
entry, the packed instance and the prototype record and builds the transform; the camera view
writes the raymarch G-buffer layout (albedo + kind 0, normal + roughness, metallic) with the
same back-face flip and alpha test as the kit's instanced program; shadow views write depth
only with the cascades' bias. Without `INDIRECT_FIRST_INSTANCE` the draw loops over buckets
with the bucket index in a push constant (the 4090 has the feature; the loop is the portable
path). After the G-buffer the depth buffer is reduced into the Hi-Z pyramid (10 mips at
800×450) for the next frame.

Memory on Canyon v2 (`[scatter] gpu:` log line): 842 825 instances in 784 cells, 200
prototypes; 27.0 MB instances and cells, 7.0 MB geometry (81.6 k near + 11.9 k mid triangles,
123 k vertices), 122.6 MB texture pages (63 × 128², 71 × 256², 66 × 512² layers with mips; the
512 page carries the character atlases), 29.0 MB cull scratch (per-view slots and visible
lists, bucket tables, indirect arguments, Hi-Z). 186 MB in all.

Debug and measurement switches: `KESSEL_SCATTER=0` (field off), `KESSEL_SCATTER_SKIP=1`
(cull runs, nothing drawn), `KESSEL_SCATTER_NO_HIZ=1` (no occlusion test),
`KESSEL_SCATTER_STATS=N` (every N frames read the bucket counts back and print the visible
counts per view and per camera LOD; `gfxbench` prints the `ScatterStats` record).

## 4. Measured cost

`gfxbench tag 120 out.png` on the Canyon v2 workshop (`KESSEL_PLAYTEST=1`,
`KESSEL_PLAYTEST_MANIFEST=…/gaea-canyon-run-v2/manifest.v1.json`, raymarch, high preset,
800×450, `KESSEL_GPU_PROFILE=1`, `KESSEL_SCATTER_STATS=60`), the harness's 22 m orbit 9 m above
the spawn, RTX 4090, Windows 11, Vulkan. One run was taken on an idle GPU (no other engine
process on the box, 19:03 on 2026-10-27, `captures/scatter_bench_on.log`):

| field on, idle GPU | |
|---|---|
| frame | avg 26.19 ms, p50 25.94, p99 41.44, min 14.75, max 43.73 |
| `scatter-cull` (4 views, 842 825 instances, 4 dispatches) | **0.074 ms** |
| `scatter-hiz` (10 mips) | **0.025 ms** |
| `rm-gbuffer` (scene + the field's camera draws) | 3.612 ms |
| `rm-shadow-far` / `rm-shadow-near` / `rm-sky-cover` (scene + the large tier) | 1.896 / 1.246 / 0.539 ms |
| visible at frame 120 | 2 780 camera (24 near, 864 mid, 1 892 pixel), 350 far cascade, 0 near cascade, 0 sky cover |

The orbit about the spawn looks across the start of the route from 9 m up, so it sees a few
thousand instances; the route viewpoints of §5 see 13–50 k (`[scatter] frame` lines in their
logs: 23 k on the road near, 42 k road far, 50 k crater near, 22 k casualties near, 13 k wreck
column near). The cull cost is independent of the view (every instance is tested for every
view); the draw cost scales with the visible count and is folded into the G-buffer and
cascade passes above, so the field-off reference and the cull-only run are needed to state
it. Pending the next GPU window (the box was handed back to the user; every engine launch is
on hold): the `KESSEL_SCATTER=0`, `KESSEL_SCATTER_SKIP=1` and `KESSEL_SCATTER_NO_HIZ=1`
variants of the same bench, and a bench from a strewn viewpoint. Under contention the same
run is meaningless (with six other engine processes on the card: 485 ms frames, cull 12 ms;
`captures/scatter_diag_profile.log`), which is why only the idle run is tabled.

Budget: the target was under 2 ms of GPU time for culling and drawing at 800×450. The cull
and Hi-Z together are 0.1 ms. The draws in the idle run are bounded by the G-buffer pass's
3.6 ms total, which also holds the whole scene; the split is pending.
## 5. Captures (`captures/`, raymarch, 800×450, clear sky)

`python tools/worldgen/scatter_shots.py --root C:\shop\panzerrechnersystem` writes the set: for
each of `road` (the route at the start), `crater` (the densest crater cluster), `treeline` (the
densest stand), `wrecks` (the densest tank column), `casualties` (the dissolved-unit field),
near (16 m, eye 2.4 m) and far (95 m, eye 14 m), day (09:20 Z) and night (18:30 Z), the same
camera with the field off (`_before`) and on (`_after`):
`captures/scatter_<site>_<near|far>_<day|night>_<before|after>.png` (40 files), the engine log
beside each (the `_after` logs carry the visible counts). The crater set was re-shot after the
clod retune (the smaller clods); its `far_night_before` is the first run's file, identical in
content since a `_before` shot has no field. Earlier probes of the session, with the first layer
file: `captures/scatter_probe_start.png` / `_before.png` (the start recon camera),
`scatter_probe_approach.png`, `scatter_check_wrecks_near_day_after.png`,
`scatter_check_casualties_near_day_after.png`.

## 6. Named approximations (also in `docs/PHYSICAL_TRUTH_AUDIT.md`)

- Contact is the heightfield surface at the object's centre with a per-group sink; an object's
  own geometry is not solved against the ground (a tilted box rests on its centre, not its low
  corner, except the slope drop applied to upright objects). Bodies "sit in the snow" by a 6 %
  sink scaled by the snow cover.
- Orientation by context is the rule's authored mode with jitter (radial fans, flight
  direction, wind), not a ballistic, gait or wind-transport result.
- Overlap is avoided only among medium/large pieces and bodies within one 32 m cell; small
  items overlap freely as litter does.
- The large tier's cover is its world AABB, not its surface; its push resistance is sliding
  friction of its estimated mass on frozen ground, and giving way is a 1 m slide along the push
  (no toppling, rolling or digging in); the medium tier's cover is LOS only; the small tier has
  no physical presence (the Tiger drives through a tin without a sound; no crunch cue yet, and
  crew on foot do not kick it). The lane keep-out is a 12 m half width about the route
  centreline.
- The field is static after load: no snow accumulates on it, nothing burns, shells do not move
  it (the pushable subset excepted).
- Draw: the reduced LOD is a vertex clustering of the source, the pixel LOD a mean-albedo
  pixel; prototypes carry no metallic-roughness or normal maps (the kit has none); shadows and
  sky cover come from the large tier only, at the reduced LOD; point-light faces take no scatter
  shadows; the Hi-Z occlusion test is the previous frame's depth under the previous camera
  (one frame of latency on disocclusion); objects beyond 1 500 m (700 m for shadows) are not
  drawn.
- Residency: the whole map's field is resident on the GPU (no streaming ring); the data target
  of a million is met in the layer, the resident count is the placed count.

## 7. Open items

- Pending the GPU window: the field-off / cull-only / no-Hi-Z bench variants (§4) and a bench
  from a strewn viewpoint; a drive through the field in the native window (pushables, crush,
  the Tiger against the large tier's cover).
- Shader backend: the field is not drawn there (the raymarch backend is the visual branch).
- Streaming by cell ring for maps larger than Canyon v2 (the campaign acts' extents fit, the
  per-view scratch grows with the instance count).
- LOS and cover consumers are linear lists; the caps above keep them cheap. A spatial index
  for `occluders`/`story_cover` would let the whole medium tier give cover.
- Snow drift against the field's own large pieces (only authored obstacles get lee drifts).
- Blood is a decal; a wound's film on the body itself is the surface-blood system's.
- A per-map authored density raster (painted) beside the rule layers, for hand-placed emphasis.
- The 512 page could drop to 256 to halve texture memory at a visible cost on bodies.
