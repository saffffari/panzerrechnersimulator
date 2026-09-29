# Urban intro: the city centre around the spawn — 28-29 September 2026

The owner decided that "the intro to the game is going to be urban combat", asked for a team of
agents to build the city and its component buildings, and then asked the team to "go through the
historical images and use them to inform the 3d structures and environments within the maps". This
document records what the team built, as integrated on the branch `claude/urban-intro-20260928`
(integrator's branch `worktree-wf_3c8013a3-a0c-8`), how to run it, the evidence, how the
photographs shaped it, and what it does not do yet. The team brief with its contract changes is
`docs/URBAN_INTRO_TEAM_20260928.md`; `CLAUDE.md`'s two principles govern everything below.

## What exists

- **Research** (`docs/research/`, copied verbatim; the photographs stay in
  `/home/alex/panzer/reference/wartime_photos_20260928/`):
  - `STALINGRAD_URBAN_FABRIC_20260928.md` (R1): the centre's building types, blocks, streets,
    condition in January 1943 and defensive changes, every number cited and graded, ending in the
    machine-readable `urban_fabric_parameters` table the grammar and the district read.
  - `PHOTO_REFERENCE_ANALYSIS_20260928.md` (P): all 180 reference photographs catalogued and
    looked at, and 72 numbered visual rules (G for the grammar, L for the district, S for the
    story, V for the verifier, M for the maps beyond), each citing its photographs; §4 names the
    photographs every render should be compared against.
  - `URBAN_INTRO_TABLEAUX_20260928.md` (R2): eleven wordless scenes for the district, ranked, with
    provenance, sources and what must not be shown. S built ranks 1 to 5 (below); 6 to 11 are not
    built.
- **The grammar** (`src/building_grammar.rs`, G): `blueprint_for` turns a spec into a validated
  blueprint for four typologies (merchant brick house, 1930s apartment block, wooden house,
  workshop) in four conditions (intact, burned, shelled, partly collapsed), from R1's documented
  dimensions: raised ground floors over semi-basements, stair halls with dog-leg stairs, rooms
  tagged by use, support edges rooted in footings, collapse clusters per storey, section and
  building. A condition is a state of the structural model: burned timber is absent from the
  blueprint and `condition_damage` fails modules through the ordinary damage path. The kit
  (`src/building_kit.rs`) holds 40 assets built only from reviewed Pizza GLBs, some cut or
  composed at load, each with collision proxies generated from its source triangles and a
  measured error bound.
- **The district** (`tools/worldgen/urban_intro_district.py`, L): a deterministic fork of the
  winter-courtyard workshop into `assets/worldgen/workshops/act1-urban-intro-20260928/`, 585 ×
  600 m around the spawn (x 1555..2140, z -100..500): a 24 m tram street, side streets and lanes,
  the old road, a square, 226 plots with 203 building instances (80 merchant houses, 18 apartment
  blocks, 68 wooden houses, 37 workshops; mostly burned, as R1 finds for January 1943) and 23
  razed plots, flat pads with aprons, rubble mounds by mass balance, a fallen facade closing a side
  street, 34 craters and 3,644 dressing operations (drifts, debris, yard walls, fences, poles and
  wires). The courtyard house, the mission points and the spawn at (1850, 250) are unchanged.
- **The runtime** (`src/building_runtime.rs`, `building_set.rs`, `building_render.rs`,
  `spatial_grid.rs`, `world_index.rs`; F1 and F2, then the integration): every
  `building_instances` placement is a live building with its own state, saves keyed by (act,
  instance) and LAN packets for changed buildings only. Modules and their rubble render as
  instances of one prototype per kit asset; every query that meets thousands of modules (crush,
  blasts, shell and cover rays, sight lines, foot collision, doors) goes through spatial grids that
  answer exactly what a scan answers. Walking, stairs, shelter, support under the feet, cover and
  container search come from the compiled layout.

### What the integration added (29 September)

- **The kit meets the runtime.** G's new assets had no runtime shape, so no district building
  could be instantiated. Every kit asset's bounds, collision proxies and measured error are now
  generated from its built mesh into the tracked table `src/building_kit_shapes.rs`
  (`cargo run -- urban-kit-shapes`; a test rebuilds it from the installed meshes and fails on
  drift), and the walking roles are declared per asset and measured against the mesh: floor tops,
  straight flights and the dog-leg's three segments. The featured modular house (in the legacy city block at
  (-30, 80)) now uses the generated proxies too; its hand-declared boxes had left 0.3 m of each doorway's visible brick
  jamb open to feet, stood its 0.16 m window wall 0.42 m deep, and made roofs and stairs their
  bounding boxes.
- **G's frame is the contract** (brief §7): a blueprint's local y = 0 is grade on the pad. Raised
  ground floors stand on their plinths, and the runtime walks the grammar's external steps from
  the pad through a threshold over the wall base into the house. Feet from the ground may not step
  onto a floor more than 0.45 m above them.
- **Dog-leg stairs** are three walking segments (flight, landing, flight). Because the flights of
  stacked storeys share their corridors in plan, a segment carries and gates only the feet within
  1 m of its surface. A failed suspended floor no longer carries feet.
- **Rubble** (P G17): a failed module leaves one mound, textured with its own material, holding its
  solid volume times a bulking of 1.5 for masonry and concrete, 0.6 for timber and 0.05 for sheet
  iron. It replaces seven grey boxes per collision box, which with the generated proxies had buried
  a collapsed workshop under hundreds of pale blocks.
- **A grammar defect found by walking**: the first-floor landing started 0.13 m short of the stair
  arriving from the thicker-walled ground storey, a hole feet fell through. The landing now reaches
  the arriving flight (`GRAMMAR_REVISION` 2).
- **The T-34 patrol probe** (`winter_patrol_point_clear`) scanned every record and occluder,
  up to six times a frame for each moving tank; on the district it now takes its candidates from
  the spatial indexes, with the same answers (2,000 probes compared) at 1.5 µs instead of 292.5 µs.
- **The featured house's door leaves** fill the doorway's measured 1.6 m opening between its brick
  jambs; they had been sized to the 2.2 m lintel and reached 0.3 m into the jambs.
- The runtime's test revisions (`tools/worldgen/urban_runtime_test_manifest.py`,
  `urban_scale_test_manifest.py`) place grammar houses instead of the 8 × 8 m featured-house
  stand-in, which the research limits refuse.
- `tools/launch_winter_session.sh --urban`, headless gunner and commander head-out views, and
  review cameras (`tools/worldgen/urban_intro_views.py`).

### The tableaux (S, story, 29 September)

Five of R2's scenes, ranks 1 to 5, are building-attached content. They are tracked data,
`catalog/urban_intro_tableaux.v1.json` (schema `kessel.urban-tableaux` v1, embedded in the
binary), resolved at build by `src/urban_tableaux.rs` against the live buildings and the terrain;
the procedural pieces are `src/urban_tableau_meshes.rs`. Each group of pieces is authored in the
local frame of one room of its host, named by building instance id and blueprint room id: the
origin is the centre of the room's floor cells at its walk surface (a room whose floor burned out
takes its modules' bounds and its storey's elevation), and a yard-only room faces its yard. A
piece's `rest` puts it on the room's floor, on the terrain, on the terrain's mean under it or tilted
to the terrain's slope. A piece that rests on or hangs from a module names it (`on`): it goes with
the module when the module fails, and a light or smoke on it goes out. A tableau whose host is
missing, of another typology, condition or cellar, or lacks a named room or module is skipped whole
and reported; it is never placed in part or elsewhere. Beside each scene the catalog records its
provenance: class, event and depicted windows, sources with locators, documented relationships,
inferred elements, the reference photographs by P's number with the traces they show, P's rules,
and what is not shown. Nothing reaches the player as words: the tram's route boards and the
canisters are blank, and library props whose names mark text (books, maps, papers, posters,
signs) are refused.

| Rank | Tableau | Host | What stands there |
|---|---|---|---|
| 1 | `a1_cellar_embrasure_two_layers`, the low window | `c3r2.w01`, burned five-storey block on the tram street | The one semi-basement window clear of the entrance steps (bay 9) narrowed to a slot with loose bricks and a sandbag; 26 spent 7.92 mm cases frozen into the snow below it; barbed wire on stakes along the pavement, cut in one bay; wind in the slot. On the slot's line of fire, 36 m out in the open square, the snow-capped autumn wreck of a German medium tank (the reviewed Pz III wreck at 0.72 of the source model, a Pz III's size), planks and a box round it. |
| 2 | `a1_red_cross_cellar`, the red cross over the cellar stair (outdoor version) | `c2r1.e02`, shelled two-storey merchant house with a cellar | A red-cross cloth (the Greek cross, geometry, not paint) on a pole beside the back steps, seen through the yard gate from the tram street; a blanket in the back door; a pressure lamp at the head of the sealed cellar stair, the only light (2,800 K); four dead under blankets and a tent half in a row along the wall, one whose boots are gone and whose feet are wound in rags; a hand sledge and a bucket. |
| 3 | `a1_chimney_field_dugout`, smoke from the ground | `c0r3.e02`, burned timber house on the west fringe | Behind the chimney field, a family dugout: a snow-covered earth mound, a plank-lined cut to a doorway hung with a blanket, the glow of an iron stove with its fire door open (1,850 K) and smoke from a pipe of tin cans; inside straw, bedding, a pot and a horse's lower leg before the stove. Outside a sledge with a sack, a bucket, an iron bedstead in the ash, a charred sill log and fence posts stripped of their boards. |
| 4 | `a1_tram_on_its_rails`, the tram that stopped on 23 August | `c2r3.e00`, shelled four-storey block, 133 m south of the spawn | A Kh two-axle car at its documented dimensions on the western of two tracks kept clear of snow for 30 m: panels holed by fragments, every pane gone, the roof torn over two bays, snow on the seats, a door hanging open; a horse frozen on its side beside the line; a half-sunk crate and a bucket. |
| 5 | `a1_white_canopy_canister`, white canopies | `c2r1.e00`, partly collapsed five-storey block, 137 m north of the spawn | A white parachute canopy snagged on the broken wall top over the street, its lines trailing down the facade. In the yard a supply canister split and emptied by the back cellar window, an unopened one 40 m out in the open snow, and 10 m short of it on the far rim of a shell crater a low mound of snow with a greatcoat's hem and back, a boot's toe and an empty sack. |

Choosing and checking the sites (each host's `choice` in the catalog says why it was taken):

- Rank 1 needs a semi-basement window with a long line of fire over open ground. The wreck first
  stood behind a courtyard boulder of the district's dressing; it now stands where the slot's line
  (14 degrees off square) meets no manifest placement, hard dressing or building footprint.
- Rank 4 needs a street that is level across: north of the spawn the district's pads tilt the tram
  street by up to 11 per cent across, which would tilt a car standing on its rails. The car stands
  where the grade across is 1.6 per cent, with 12.5 m of the street clear beside it.
- Rank 5's mound first lay in the bottom of a base-terrain shell crater (5.85 m radius, 1.32 m
  deep), out of sight from everywhere; it now lies on the crater's far rim.
- Rank 3's mound rises about 2.7 m behind its house's back wall and reaches over the plot
  boundary toward the neighbour's stove (the catalog's inferred siting: between the two stoves).

Bodies. The tram (113 boxes), the dugout (45) and the Pz III wreck (64) are static cover and
obstacles. The tram's boxes are its own solid parts: exact at quarter-turn yaws, 0.063 m at most
on its grade. The dugout's boxes are steps that contain its mound, standing proud of the snow by
at most 0.25 m, as the mesh measures them. The wreck's boxes are its source surface's voxel cells,
with a bound of 0.721 m. The library props are pushable where the district's own props are. The
cloth, the wire, the dead and the horse carry no collision. The ledger names every simplification
(PHYSICAL_TRUTH_AUDIT, the tableaux row).

## How to run it

```sh
python3 tools/worldgen/urban_intro_district.py            # write the workshop (deterministic)
python3 tools/worldgen/urban_intro_district.py --review   # + the daylight review copy
tools/launch_winter_session.sh --urban                    # play it (the default is still the courtyard);
                                                          # generates the workshop first if it is missing
tools/launch_current_build.sh --singleplayer --urban      # the desktop entry: build if needed, then play

# Headless renders (captures/, flat). Explicit camera: X Y Z YAW PITCH FOG (yaw 0 looks toward -Z).
KESSEL_RECON_MANIFEST=assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json \
    cargo run -- screenshot captures/urban_view.png 1846 7.9 262 0 -1 0.0035
# The Tiger's gunner sight laid up the tram street, and the commander head out of the cupola:
KESSEL_SHOT_LAY=1850,8,120 KESSEL_RECON_MANIFEST=... cargo run -- screenshot captures/urban_gunner.png
KESSEL_SHOT_LAY=1850,8,120 KESSEL_SHOT_STATION=commander KESSEL_SHOT_HEADOUT=1 \
    KESSEL_RECON_MANIFEST=... cargo run -- screenshot captures/urban_headout.png
# Review cameras in front of one building of every typology and condition:
python3 tools/worldgen/urban_intro_views.py --write-inspect      # inspection-lighting copy
python3 tools/worldgen/urban_intro_views.py --lighting inspect   # prints the commands

# Tests
cargo test --bin panzerrechnersystem -q                           # unit suite
cargo test --bin panzerrechnersystem urban_runtime_gpu_tests -- --ignored   # GPU, needs the test revision
python3 tools/worldgen/urban_runtime_test_manifest.py             # regenerate the test revision
python3 tools/worldgen/test_urban_intro_district.py              # district tool tests
python3 tools/runtime_asset_cook.py validate && python3 tools/test_runtime_asset_cook.py
cargo run -- urban-kit-shapes                                     # after any kit change
cargo run -- urban-tableaux                                       # resolve the tableaux on the workshop, report
cargo run -- urban-tableaux-describe <manifest> <instance> [module patterns]   # a host's rooms and modules
cargo test --bin panzerrechnersystem urban_tableau                # tableau catalog, resolution and meshes
python3 tools/worldgen/urban_tableau_views.py --lighting inspect --run   # tableau review captures
KESSEL_PLAYTEST_MANIFEST=assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json \
    cargo run -- winter-bench dense 60 captures/urban_winter_bench.json
```

Lighting: the workshop inherits the courtyard's authored stormy dusk (sun 7.5°, overcast 0.96,
fixed exposure 1.25). On the physical backend (the headless `screenshot` default) the city is nearly
black away from the courtyard's lamps; on the shader backend that `--urban` launches
(`KESSEL_RENDERER=shader`) the same dusk reads as a dim overcast: grey snow and sky, the ruins as
dark masses, the tableaux' lamp and stove glowing (verifier's `urban_v_shader_*_authored.png`). The review
copy (`--review`: sun 20°, exposure 8) and the inspection copy (sun 32°, less overcast, exposure 8)
change only the sun, the sky and the camera, for judging geometry; captures made with them carry
`_review` or `_inspect` in their names.

## Evidence

Captures are in this worktree's `captures/` (gitignored), made by the commands above:

- District plan: `captures/urban_i_district_plan.png`.
- Aerial overview: `urban_i_aerial_review.png`, `urban_i_aerial_inspect.png`,
  `urban_i_probe_aerial.png` (authored dusk).
- Spawn street at eye height: `urban_i_spawn_street_north_authored.png`,
  `urban_i_spawn_street_north_review.png`, `urban_i_spawn_street_south_review.png`,
  `urban_i_tram_street_north_review.png`.
- Tiger gunner sight: `urban_i_gunner_tram_street_authored.png`,
  `urban_i_gunner_tram_street_review.png`, `urban_i_gunner_review.png` (laid on the nearest enemy).
- Commander head-out: `urban_i_commander_headout_authored.png`,
  `urban_i_commander_headout_review.png`.
- Ruined interiors: `urban_i_interior_burned_merchant_inspect.png` (roofless merchant house),
  `urban_i_interior_burned_block_inspect.png` and `_review.png` (a room of a burned block).
- One facade of every typology in every condition the district holds (15):
  `urban_i_facade_<typology>_<condition>_inspect.png`; two with review lighting
  (`urban_i_facade_merchant_brick_house_intact_review.png`,
  `urban_i_facade_apartment_block_shelled_review.png`).
- Winter-bench receipt: `urban_i_winter_bench_dense.json` and `winter_bench_dense_shipping_v2.png`.
- The tableaux (S), made by `tools/worldgen/urban_tableau_views.py`, `urban_s_<camera>_inspect.png`
  (form, inspection lighting) and `urban_s_<camera>_authored.png` (the workshop's night):
  rank 1 `embrasure_street`, `embrasure_slot`, `embrasure_from_wreck`, `embrasure_interior`
  (from the ground floor over the slot to the wreck); rank 2 `redcross_gate`, `redcross_dead`,
  `redcross_hall` (the lamp at the stair head); rank 3 `dugout_gate`, `dugout_near`,
  `dugout_door`, `dugout_room`; rank 4 `tram_approach`, `tram_horse`, `tram_interior`; rank 5
  `canopy_street`, `canister_yard`, `canister_from_open`, `canister_interior`.

Measured on the district (RTX 4090, dev profile, 29 September):

| Measure | Value |
|---|---|
| Buildings / modules | 204 (203 district + the featured house) / 23,226 (largest 904) |
| Render instances | 45,385 in 2,316 pages over 152 shared prototypes, 23,181 visible at start |
| Destruction records / sight occluders | 136,386 / 148,237 |
| TLAS demand at build | 45,566 of 65,536 (0 overflow frames in winter-bench) |
| Exact-hit payload | 2,225 MiB, largest binding 644 of 2,048 MiB |
| Sealed cellars | 61 buildings |
| Build and render, headless screenshot | about 20 s |
| winter-bench dense, 60 frames, 4090 preset | frame 360.8 ms mean (p95 368.8), simulation 0.94 ms, submission 9.5 ms: GPU-bound as F2 measured on building-free scenes |
| Kit proxies | 1,207 boxes over 40 assets, errors 0.022-0.22 m (measured) |

Tests (29 September): the unit suite passes (1,224 passed, 0 failed, 37 ignored; with S's
tableau tests 1,236 passed, 0 failed, 37 ignored); `lan-verify` passes; every one of 224
typology, condition and size corners instantiates with steps to every raised floor
(`building_runtime::grammar_tests`); a soldier climbs from the street by the steps and the dog-leg
to the floor above in four buildings at four yaws, and failed stairs carry no one; through the real
game loop (GPU test) a soldier climbs the 270° test house to its first floor (4.85 m above the pad)
and stays on the entry floor (0.45 m) of the 90° house after its stairs fail; the spatial indexes
answer 6,000 random queries exactly as scans on the grammar test revision; instanced modules and
rubble follow the structural state; on the district 2,000 T-34 patrol probes answer through the
indexes exactly as the scan. The 45 mm gun encounter test passes on the courtyard workshop and fails
on the district (below). Integration test targets fail as before the merge and only there
(`display_quantization`, `live_fx_contract`, `physical_truth_contract` "glass_pipe";
`foot_ai_contract` and `combat_training` do not compile for a missing `crate::resource`).

## How the photographs shaped it

- **Open ground, not street walls.** P's first finding is that the centre in January 1943 was open
  snow with isolated shells (P L1; [04-01], [04-02], [07-01]). The district keeps R1's frontage
  occupancy; the renders from the street and the air read as isolated masses on snow with haze
  over distance. P and R1 disagree on density (the district's open-ground-to-shell distances are
  shorter than P's 50-250 m); L's receipt records the conflict.
- **Buildings.** G compared Blender previews of every typology and condition with P §4's
  photographs and changed the grammar where they disagreed: roofs of two slopes per bay, each
  resting on its own wall so a breach tears the roof open over it (P G4, G16); brick gable ends
  on masonry and boarded ones on timber; parapets and plinths at the wall's real brick scale;
  window frames on timber houses; shop doors between shop windows; partly collapsed ruins stepping
  back a bay every second storey; shelled facades keeping most of their window grid (P G11).
- **Burned timber quarters** are rows of plinths with stoves and chimneys at plot pitch (P G13, L2;
  [04-27], [04-07], [04-23]), as the aerial and the burned wooden house show.
- **Rubble** follows P G17: from failed modules only, their volume times 1.5, mostly inside the
  footprint with a berm outside, in lumps rather than gravel.
- **Integration comparison.** The renders were set against the photographs P §4 names (from its
  comparison boards `compare_typologies.jpg`, `compare_environments_district.jpg`): the burned
  merchant houses match [07-14] and [04-01] in form (roofless shells to the cornice, stacks); the
  blocks match [04-09], [04-11] and [07-15] (window grid, raised ground floor, balconies,
  stepped collapse). What the photographs show and the city does not yet model: fragment pitting
  (G14), soot (G15), curled roof iron (G16), plaster (G19), ragged course-by-course breach outlines
  (G11), snow on ledges and cornices (L8, V11), and the brightness of the photographs' overcast
  daylight against the authored dusk.

## Limits (what it does not do)

- **The tableaux (S)**: the cellars are sealed, so ranks 1 and 2 are their outdoor versions: the
  firing position behind the slot and the dressing station down the stair are not built (R2
  §4.1-4.2). Under the workshop's authored night only the lamp and the stove show; the tram, the
  wreck, the canopy and the canisters are silhouettes on starlit snow, and the captures that judge
  form use the inspection copy. No footprints, drag marks or trodden paths exist (no snow
  deformation state), so rank 5's line of tracks to the mound is not built. Cloth is static, the
  snow on the pieces is a static deposit or authored, the tram cannot be shoved (a static 12.65 t
  car), the dugout is indestructible and stops a tank, and the stove's smoke is the district
  fires' dark soot. The wind in the slot reuses one generated gust clip; R2 §5's sound design is
  not built. The Pz III wreck and the charred joist are workshop authoring assets outside the
  reviewed runtime closure (as the workshop's own wrecks are): a packaged build refuses them and
  skips ranks 1 and 3 whole until they are catalogued.

- **The 45 mm gun encounter is broken on the district.** Both gun sites conflict with buildings
  (L's receipt): gun A's sector toward the route is screened and it never fires in the encounter
  test. L proposed street sites for both guns and a T-34 loop on the district's streets; the
  encounter code was left unchanged by instruction. Choosing them is the next encounter step.
- **Cellars are sealed** under the continuous terrain (F2): 61 buildings keep cellar storeys
  that carry load and take damage but cannot be entered; a gutted building with a cellar puts feet
  on the terrain at grade. An honest opening needs terrain holes, vehicle floor support, infantry
  navigation and lowest-storey support together (brief §7).
- **Walking**: the dog-leg's segments and the steps are analytic surfaces (0.16-0.17 m and up to
  0.47 m from the treads, asset frame); feet pass through the underside of a flight or landing
  above them; balconies and rubble mounds are not walkable; AI infantry still have no navigation
  (P0 in the ledger) and walk through the buildings.
- **Rubble is render-only**: tanks, feet and shells pass through the mounds. Measured by the
  verifier: walls, slabs and stairs show 0.95-0.99 of their budget (the lumps overlap), but every
  lump keeps a 4 cm floor over the widened footprint, so thin or light pieces show more than they
  held (a timber board floor about 6 times, a crate 4.7 times).
- **The authored lighting** of the workshop (stormy dusk) leaves the city nearly black on the
  physical backend and a dim grey dusk on the shader backend the launcher uses: from the same
  camera the final images' mean luminance differs about 68 times, some six stops (0.0008 against
  0.051 linear). The intro's time of day and weather are a design decision for the owner.
- **The Tiger passes through floors, stairs and steps.** Only load-bearing walls, pillars,
  partitions and door frames are tank barriers (`BuildingRuntime::destructible_is_crush_barrier`);
  a tank stays grounded on the terrain, so it drives through the grammar's external steps and, once
  it has ground through a wall, under a raised ground floor whose slab cuts through its hull (the
  verifier drove through block `c3r2.w01` at pad height with its floor 1.0 m above).
- **The drawn terrain is not the terrain everything stands on**: the render mesh is decimated
  (512² on the 4090 preset, 256² on the laptop preset) against the 1024² collision grid, so snow
  lies up to 0.44 m (4090) and 1.4 m (laptop) off building bases and up to 1.25 m off the streets
  (ledger row, `tools/worldgen/terrain_render_gap.py`).
- **Streets do not show on the ground** in Act I's winter shading (L), the 130 standing poles are
  reviewed brick pillars rather than timber poles, and yard walls and piers are indestructible
  manifest walls (existing ledger debt).
- Proxy boxes per building are many (the featured house went from 93 to about 600 records), which
  the grids absorb (0.94 ms simulation) but which multiply sight occluders (148,237).
- The featured-house debug stand-in (`KESSEL_DEBUG_FEATURED_BLUEPRINT`) is obsolete: no typology
  accepts its 8 × 8 m two-storey spec any more.
- The WORLDBUILDER keeps `building_instances` when it saves but does not show them.
- LAN replicates at most 300 partly damaged modules per building (F1's cap): heavy shelling of one
  large block can exceed it, and that building's record is then refused and reported while the
  rest of the session continues.
- The manifest's `doorway_hr_1` doors (Act I's 576) keep leaves sized for 1.6 m in a 1.0 m opening,
  the same lintel-for-opening mistake corrected here for the featured house; outside this work.

## Verification (V, 29 September)

- **Build and tests** (at S's head plus the verifier's commits): unit suite 1,238 passed, 0 failed,
  37 ignored (S's 1,236 plus the rubble-volume and grounded-occluder tests); release builds; the four `urban_runtime_gpu_tests` pass;
  `lan-verify` passes; the district tool's tests (15) and the asset catalog (178 GLBs; 21 tests)
  pass; `urban-tableaux` places 5 of 5. The 45 mm ambush GPU test passes on the courtyard workshop
  and fails on the district (gun A fires 0 shots), as the integrator reported.
- **Determinism**: three runs of the district tool (one with `PYTHONHASHSEED=987`) write
  byte-identical outputs, identical to the integrator's and S's worktrees (`manifest_edits.json`
  sha256 `c9e8c604…`, `heights.npy` `ba3b7b74…`).
- **Driving** (`mechanic-evaluate`, simulation clock, head out): street c west and lane w2 north,
  344 m each, and the tram street past the tableau tram on its east half are clear. The tram
  street's centre line stops at the tram (by design) and, north of the spawn, at the courtyard
  workshop's Pz III wreck at (1855, 153), passable to its west; street e1 stops at the fallen
  facade's chunks (the intended closure). Brick walls grind and fall; the Tiger crossed the 12 m
  deep block `c3r2.w01` in about 9 s at pad height, under its raised floor (Limits).
- **Performance** (RTX 4090, dev profile; the workshop's display tuning sets 1280 × 720 on both
  presets):

  | Scene, backend | winter-bench dense 60: mean / p95 | gfxbench orbit at the spawn | TLAS needed of 65,536 |
  |---|---|---|---|
  | district, physical, 4090 | 363.6 / 371.2 ms | 375.6 ms | 45,635 (0 overflow frames) |
  | courtyard workshop, physical, 4090 | 344.8 / 351.6 ms | 349.8 ms | 18,805 |
  | district, shader (the launcher's), 4090 | 11.5 / 15.4 ms | 12.8 ms | no ray scene |
  | courtyard workshop, shader, 4090 | 7.2 / 10.6 ms | 9.2 ms | no ray scene |
  | district / courtyard, laptop preset (raster) | | 13.0 / 11.8 ms physical, 12.8 / 9.1 ms shader | |

  Simulation 0.94 ms against 0.52 ms, submission 9.7 against 5.0 ms (physical bench). With GPU
  timestamps (`KESSEL_GPU_PROFILE=1`) the physical G-buffer pass costs 61.7 ms on the district
  against 1.1 ms on the courtyard (passes may overlap; the unprofiled frame grows 19 ms), and the
  shader backend's main raster 2.1 against 1.5 ms and its static sky visibility 1.45 against 0.32 ms.
- **Captures**: every capture of the integrator (35) and S (43) was looked at, and 82 new stills
  and a drive video made (`captures/urban_v_*`): on foot in every street class, the square, the
  razed plots and the open ground; the Tiger's gunner sight, buttoned-up cupola and head-out views;
  inside gutted shells; each tableau from new angles, demolished (rank 2's host) and on the shader
  backend.
- **Photographs**: renders beside the photographs P §4 names, greyscale at 640 × 360 (P V1), with a
  verdict against P's rules on each board:
  `/home/alex/.claude/jobs/ca78baf9/tmp/research/photo_boards/verify_<subject>.png` (merchant
  houses, apartment blocks, wooden houses, workshops, district overview, main street, square, open
  ground, timber fringe, open winter ground, Tiger in the city, tableaux, light and distance).
  Rules the build breaks, beyond the gaps already named: V5 (the burned blocks' windows show the
  full-height spine wall, not sky), V12 (a regular skyline of complete shells), G2/G5/G6 (no arched
  heads, moulded surrounds, dentil cornices or rusticated ground floors on merchant houses; the
  string course at the floor line is there), G11/G12 (breaches
  and collapses by whole bays, no hanging slabs), G13 (no V of fallen rafters; round stacks),
  workshops without pier-and-bay fronts, and the stove smoke of rank 3 as a line of black cubes (V10).
- **Fixed**: the kit crate carried readable "DO NOT STACK" and "FRAGILE" stencils (now an unmarked
  crate, brief §7); a failed module's knee-high sight occluder stayed at the storey it fell from,
  an invisible blocker in mid-air (11,384 of 12,562 failed-module boxes at build more than 3 m up,
  up to 20 m); it now stands on the building's ground under the rubble, and the occluder index
  reaches that ground; the ledger's claim that rubble's visible volume stays below its budget
  (measured 0.95-6.0 times); the terrain render mesh's departure from the collision surface, now a
  ledger row.

## In one build with the calibrated sound (merge, 29 September)

The district was merged into the calibrated sound line (`claude/sound-overhaul-20260928`,
`docs/IMPACT_SOUND_20260928.md`): real-life sound levels and the ear model, no words in play, death
as black and silence, no Panzerfaust in Act 1 and T-34/76s in place of SU-100s all apply here.

- **Collapse beat.** The sound line's calibration had mapped the collapse beat's voice (the
  vehicle-kill boom) to a 0.71 kg TNT blast, so a Tiger grinding through a district building with the
  commander head-out took limiter-level impulses (measured: repeated −1 dBFS peaks, LAF,max
  −21.6 dBFS(A); 182-198 dB at the ear at 2-5 m). It is now masonry falling, the prop/masonry
  calibration + 16 dB [E] (ledger, structure-collapse row): the same drive keeps LAF,max near
  −35 to −42 dBFS(A), under the engine at full load.
- **Launch.** `tools/launch_winter_session.sh --urban` writes the workshop with the district tool
  when it is missing (byte-identical to the hashes above), and `tools/launch_current_build.sh
  --singleplayer --urban` builds if needed and starts the district.
- **Verification of the merge.** Unit suite 1,292 passed, 0 failed, 40 ignored; the four
  `urban_runtime_gpu_tests` pass; the district tool's 15 tests pass; the asset catalog validates
  (178 GLBs); the release calibration check `impact-mix-review` passes (23 events, 53 relations);
  `lan-verify` passes. Film `catalog/shader_reviews/urban_intro_merge_20260929.v1.json` (the winter
  session on the district, commander head-out at the spawn, the own gun head-out, then buttoned up,
  driving up the tram street and the own gun buttoned; recorded at the recorder's 640 × 360 where the
  launcher plays at 1280 × 720): no words in any frame, and the soundtrack as the calibration film on
  the arena measured it — idle bed head-out LAF,max −56.6 dBFS(A); own 88 head-out −13.6 / peak
  −1.0 dBFS; idle buttoned −55.9; driving buttoned −30.4 / −19.7; own 88 buttoned into the idling bed
  −21.2 / −2.9.
- **Not calibrated.** The tableau's slot wind, like the courtyard workshop's two generated cues, is
  an authored scene emitter played at its authored gain, outside the per-voice calibration.

## Next steps

1. S (story): R2's ranks 6 to 11, the cellar versions of ranks 1 and 2 once cellars open, rank
   5's tracks once snow can be trodden, and a catalog review of the wreck and the joist.
2. Tanks against floors, stairs and steps, and rubble as collidable ground (Limits); the terrain
   render mesh drawn from the collision grid on both presets (ledger).
3. Re-site the 45 mm guns and the T-34 routes on L's proposals and rerun the encounter test on the
   district.
4. Decide the intro's lighting and weather; then the photographs' daylight tone can be matched.
5. Honest cellars (F2's list), rubble collision from the mounds' own geometry, ledge snow and
   breach outlines (G's named gaps), a timber pole asset, and winter street tracks on the ground.

Co-authored by the urban-intro team (R1, R2, P, G, L, F1, F2, I and S), 28-29 September 2026.
