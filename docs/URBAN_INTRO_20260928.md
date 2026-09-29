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
    provenance, sources and what must not be shown (for the story role, not built yet).
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
  straight flights and the dog-leg's three segments. The featured courtyard house now uses the
  generated proxies too; its hand-declared boxes had left 0.3 m of each doorway's visible brick
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
- The runtime's test revisions (`tools/worldgen/urban_runtime_test_manifest.py`,
  `urban_scale_test_manifest.py`) place grammar houses instead of the 8 × 8 m featured-house
  stand-in, which the research limits refuse.
- `tools/launch_winter_session.sh --urban`, headless gunner and commander head-out views, and
  review cameras (`tools/worldgen/urban_intro_views.py`).

## How to run it

```sh
python3 tools/worldgen/urban_intro_district.py            # write the workshop (deterministic)
python3 tools/worldgen/urban_intro_district.py --review   # + the daylight review copy
tools/launch_winter_session.sh --urban                    # play it (the default is still the courtyard)

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
KESSEL_PLAYTEST_MANIFEST=assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json \
    cargo run -- winter-bench dense 60 captures/urban_winter_bench.json
```

Lighting: the workshop inherits the courtyard's authored stormy dusk (sun 7.5°, overcast 0.96,
fixed exposure 1.25), in which the city is nearly black away from the courtyard's lamps. The review
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

Measured on the district (RTX 4090, dev profile, 29 September):

| Measure | Value |
|---|---|
| Buildings / modules | 204 (203 district + courtyard house) / 23,226 (largest 904) |
| Render instances | 45,385 in 2,316 pages over 152 shared prototypes, 23,181 visible at start |
| Destruction records / sight occluders | 136,386 / 148,237 |
| TLAS demand at build | 45,566 of 65,536 (0 overflow frames in winter-bench) |
| Exact-hit payload | 2,225 MiB, largest binding 644 of 2,048 MiB |
| Sealed cellars | 61 buildings |
| Build and render, headless screenshot | about 20 s |
| winter-bench dense, 60 frames, 4090 preset | frame 360.8 ms mean (p95 368.8), simulation 0.94 ms, submission 9.5 ms: GPU-bound as F2 measured on building-free scenes |
| Kit proxies | 1,207 boxes over 40 assets, errors 0.022-0.22 m (measured) |

Tests (29 September): the unit suite passes (see the final commit's report); every one of 224
typology, condition and size corners instantiates with steps to every raised floor
(`building_runtime::grammar_tests`); a soldier climbs from the street by the steps and the dog-leg
to the floor above in four buildings at four yaws, and failed stairs carry no one; through the real
game loop (GPU test) a soldier climbs the 270° test house to its first floor (4.85 m above the pad)
and stays on the entry floor (0.45 m) of the 90° house after its stairs fail; the spatial indexes
answer 6,000 random queries exactly as scans on the grammar test revision; instanced modules and
rubble follow the structural state. The 45 mm gun encounter test passes on the courtyard workshop
and fails on the district (below).

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
- **Rubble is render-only**: tanks, feet and shells pass through the mounds, and the lumps overlap,
  so the visible volume is below the budget.
- **The authored lighting** of the workshop (stormy dusk) leaves the city nearly black; the intro's
  time of day and weather are a design decision for the owner.
- **Streets do not show on the ground** in Act I's winter shading (L), the 130 standing poles are
  reviewed brick pillars rather than timber poles, and yard walls and piers are indestructible
  manifest walls (existing ledger debt).
- Proxy boxes per building are many (the featured house went from 93 to about 600 records), which
  the grids absorb (0.94 ms simulation) but which multiply sight occluders (148,237).
- The featured-house debug stand-in (`KESSEL_DEBUG_FEATURED_BLUEPRINT`) is obsolete: no typology
  accepts its 8 × 8 m two-storey spec any more.
- The WORLDBUILDER keeps `building_instances` when it saves but does not show them.

## Next steps

1. S (story): build three to five of R2's tableaux as building-attached content in rooms of the
   district, avoiding cellars until they open.
2. V (verify): performance on the district across presets, comparison boards putting these renders
   beside P's photographs, determinism of the district tool, physical-truth compliance.
3. Re-site the 45 mm guns and the T-34 routes on L's proposals and rerun the encounter test on the
   district.
4. Decide the intro's lighting and weather; then the photographs' daylight tone can be matched.
5. Honest cellars (F2's list), rubble collision from the mounds' own geometry, ledge snow and
   breach outlines (G's named gaps), a timber pole asset, and winter street tracks on the ground.

Co-authored by the urban-intro team (R1, R2, P, G, L, F1, F2 and I), 28-29 September 2026.
