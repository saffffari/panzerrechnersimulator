# Urban intro: city team brief (28 September 2026)

The owner decided (28 September 2026): "the intro to the game is going to be urban combat", and
asked for a team of agents to build the city and its component buildings. The owner then asked
the team to "go through the historical images and use them to inform the 3d structures and
environments within the maps". The images are the reference set in
`/home/alex/panzer/reference/wartime_photos_20260928/`: 180 dated, sourced photographs, with
`manifest.json` captions and `index.html`. This brief is the
single reference for that team. `CLAUDE.md` governs everything below. The two principles are
wordless, sourced environmental storytelling and physical truth with no hidden cheats.

## 1. What exists (map of 28 Sep 2026, file:line at commit c1c6ee6)

- **Act 1 "The City"** (world.rs:424-450) is manifest-driven.
  - `assets/worldgen/act1/manifest.v1.json` holds 13,014 placements. 82% of its walls sit in one
    15×15 block grid of Pizza modules centred on (600, 100): 30 m pitch, about 450 m square.
  - Landmark blockouts: grain elevator (900, 1100), fountain (300, -150), Kazan church
    (-400, 600), main station (-900, -300), Mamayev Kurgan (-1600, 250).
  - Spawn (1850, 250), goal (-1950, -400). The premise is a retreat west through the ruined city
    (ACT1_MAP.md).
- **The playable winter slice.** `tools/launch_winter_session.sh` sets `KESSEL_PLAYTEST_MANIFEST`
  to `assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json`, with its
  `manifest_edits.json`.
  - A ruined courtyard house of plain manifest placements sits at about (1880, 165).
  - Mission points: HOUSE (1880, 165), WRECK (1907, 218), WITHDRAW (1870, 340)
    (winter_mission.rs:10-12).
  - The winter encounter:
    - Soviet tank platoon routes A (1700,-210)…(1880,-250) and B (1960,-360)…(1990,-250)
      (winter_encounter_ai.rs).
    - Two 45 mm anti-tank guns at (1555,-80) and (1930,-95) (live_playtest.rs, `field_at_guns`).
- **Buildings.**
  - `building_blueprint.rs` is a complete data model:
    - floors, rooms, 12 module kinds, connectors, doors, stairs, containers, loot;
    - support edges forming a DAG, and collapse clusters;
    - validation (every room reachable from an exterior exit), a compiled layout, compact damage
      state with collapse propagation, and a JSON codec.
  - `building_runtime.rs` is hard-coded to its one blueprint, the 8×8 m two-storey featured house
    (`feature_blueprint`, 1220-1868):
    - literal module ids and coordinates in `shelter_at`, `resolve_walk_xz`,
      `walkable_surface_y`, `unsupported_upper_position` and the storage search;
    - a private constructor;
    - `Game` holds a single `modular_building: Option<BuildingRuntime>` (game.rs:8193);
    - every module is a loose mesh with its own pre-allocated rubble heap.
  - Every other wall in the world is either an indestructible hard stop (manifest walls, from an
    11-stem collision whitelist in worldgen.rs:563-668) or one of at most 62 legacy destructible
    walls.
- **Infantry** have no navigation. They are grounded on the heightfield, walk through walls, and
  use hard-coded cover cylinders. That is P0 debt in PHYSICAL_TRUTH_AUDIT.
- **Scene blocks** (SCENE_BLOCKS.md, assetlib.rs:1103-1360) carry provenance.
  - Relocation replays only their terrain; stamped props become unlinked, at yaw 0 only.
  - Story sites are hard-coded coordinates with no provenance (worldgen.rs:1705-1954;
    game.rs:7876-7902).
- **Limits you must respect.**
  - The ray-tracing TLAS holds 65,536 instances; beyond that the tail is dropped
    (renderer.rs:7980-7991).
  - There is no runtime LOD.
  - Stealth line-of-sight and tank-versus-cover loops are linear scans (stealth.rs:356,
    game.rs:25591).
  - The terrain is a continuous 4.29 m heightfield, so a cellar under it is unreachable today.
- **Git.** `assets/*` is gitignored. Authored data therefore lives in tracked code, or in tracked
  tools that regenerate the untracked manifest revision deterministically. Commit the tool, not
  its output.

## 2. Where and when

- **The intro district surrounds the Act 1 spawn and the winter courtyard in the east of the act.**
  - Plan for roughly x 1450…2050, z -100…450, about 500 × 500 m; STATUS.md already asks for "a
    dense 500×500 m warzone block".
  - The district builder picks the exact extent from the terrain.
  - The courtyard house and its mission points stay working.
  - The route west to the existing grid and the landmarks stays open.
- **Setting (default; the owner may override):** the city centre of Stalingrad in January 1943:
  - pre-revolutionary brick merchant houses;
  - 1930s apartment blocks;
  - timber houses on the fringes;
  - yards, sheds, workshops, tram streets and a square.
  - Winter, snow, everything ruined by five months of fighting.
- **Historical frame.** No Tigers fought inside the pocket (docs/teams/historical-map-grounding.md
  §6). Only the buildings, the district fabric and the tableaux are sourced. The player's vehicle,
  route and compressed chronology are not literal (CLAUDE.md). Never imply one crew witnessed a
  specific event.

## 3. The contract (commit "urban intro: team contract", files below)

- **`src/building_placement.rs`**
  - `Typology`: merchant_brick_house, apartment_block, wooden_house, workshop.
  - `Condition`: intact, burned, shelled, partly_collapsed.
  - `BuildingSpec` and `BuildingPlacement`.
  - The frame convention:
    - local origin at the footprint centre at ground-floor level;
    - street facade toward local +Z;
    - world = `T(x, ground_y, z) · R_y(yaw)`;
    - yaw a multiple of 90° in round 1.
  - The strict JSON codec for the top-level `building_instances` array of a revision's
    `manifest_edits.json`:
    ```json
    "building_instances": [
      {"id": "urban_intro.b03.p2", "typology": "apartment_block", "seed": 12345,
       "width_m": 40.0, "depth_m": 12.0, "storeys": 4, "cellar": true,
       "condition": "shelled", "x": 1812.0, "z": 204.0, "yaw_deg": 90.0}
    ]
    ```
    Footprints may touch (party walls) but never overlap.
- **`src/building_grammar.rs`**
  - `limits(typology)`: provisional, to be replaced from the research.
  - `check_spec`.
  - `blueprint_for(spec, instance_id)`: pure and deterministic; every blueprint validates.
  - `condition_damage(spec, blueprint)`: damage the runtime applies through
    `BuildingState::apply_damage` at instantiation.
- **`src/building_kit.rs`**
  - The kit registry: semantic asset id → source mesh and `KitMaterial` (Brick, Stone, Timber,
    Concrete, Metal).
  - It starts with the featured house's nine Pizza modules.
- **Change control.** These files change additively. If you must break the contract, append the
  change to §7 of this file in your own commit and say so in your report; the integrator
  reconciles.

## 4. Rules for every agent

- **Physical truth (CLAUDE.md second principle):**
  - A ruin is a state of the structural model: failed modules, and burned timber absent from the
    blueprint. It is never a paint-over mesh.
  - Rubble that blocks a street is real, collidable geometry with a proxy from its mesh.
  - Every collision proxy is generated from the source geometry and carries a measured error
    bound, named plainly in code.
  - Add or update the row in `docs/PHYSICAL_TRUTH_AUDIT.md` in the same commit that lands or
    removes an approximation.
  - A performance shortcut needs a debug-only gate and a regression proving it is off in both
    shipping presets.
  - Never exceed a capacity silently (TLAS instances, LAN caps, destructible counts). Measure and
    report.
- **Storytelling (CLAUDE.md first principle):**
  - No readable text tells anything.
  - Every tableau is grounded in a cited real event or well-attested condition, with provenance in
    its authoring metadata.
  - No invented atrocities or melodrama.
- **Determinism.** Randomness is a counter-seeded integer hash (`n.wrapping_mul(2654435761)`…).
  There is no `rand` crate. Python tools seed explicitly and produce byte-identical output on a
  rerun.
- **Captures** go in your worktree's root `captures/` (gitignored, flat), with prefix `urban_…`.
  Headless screenshots are the primary verification: render, then read the PNG back and look at
  it. See CLAUDE.md for commands; `KESSEL_RECON_MANIFEST=<manifest>` renders any
  workshop/revision.
- **Build and test:**
  - `cargo build` (the dev profile is optimised).
  - `cargo test --bin panzerrechnersystem <filter>`.
  - Before handing off, run the full unit suite `cargo test --bin panzerrechnersystem -q
    2>/dev/null`. Baseline at the scaffold: 1156 passed, 0 failed, 31 ignored.
  - Known unrelated failures outside the unit suite:
    - integration tests display_quantization, live_fx_contract, physical_truth_contract
      "glass_pipe";
    - the GPU winter-mission wreck-rounds assertion (environmental).
- **The session's worktree guard:**
  - Refuses `git -C`, commands it cannot verify stay inside your worktree, and heredocs combined
    with cargo in one command.
  - Run plain, separate commands from your worktree root.
  - Write helper scripts to files and run them separately.
- **Git:**
  - Commit on your own branch, with messages in the repo's style, ending with the trailer
    `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
  - Never push. Never merge into another branch unless you are the integrator.
  - Never touch `/home/alex/panzer/tank_simulator` (the shared checkout),
    `/home/alex/panzer/shader-graphics`, or the sound-overhaul worktree.

## 5. Worktrees and branches

- **Your own worktree.** Every building agent runs in a fresh isolated git worktree that the
  workflow creates for it: its starting directory.
  - The harness refuses shell commands anywhere else, and `EnterWorktree` does not help, because
    the shell stays pinned.
  - Set up with plain, separate commands:
    1. `git reset --hard <base commit given in your prompt>`.
    2. `cp -a --reflink=always <prepared>/assets <prepared>/source_assets <prepared>/target .`, a
       reflinked copy of the art and a warm build cache from the prepared worktree named in your
       prompt.
  - Commit on your worktree's branch (`git branch --show-current`). Report the branch, the head
    commit and the worktree path (`pwd`).
- **Work chains by commits:**
  - F2 resets to F1's head.
  - The integrator starts from the contract commit and merges the reported runtime, grammar and
    district branches.
  - S and V reset to the previous head.
  - At the end, the owner's session fast-forwards `claude/urban-intro-20260928` to the verified
    head.
- **Prepared worktrees** are sources of art and cache only; never edit them. They are
  `urban-intro-20260928`, `urban-runtime-20260928`, `urban-grammar-20260928` and
  `urban-district-20260928`, all under `/home/alex/panzer/tank_simulator/.claude/worktrees/`.
- **The generated urban workshop** under `assets/` exists only in the worktree that ran the district
  tool. Downstream agents regenerate it with `python3 tools/worldgen/urban_intro_district.py`,
  which is deterministic.
- **Research and photo agents** write only to `/home/alex/.claude/jobs/ca78baf9/tmp/research/`. The
  integrator copies their documents (not the photographs) into `docs/research/`. The photographs
  stay in `/home/alex/panzer/reference/` and never go into `captures/` or `assets/`.

## 6. Roles

- **R1, urban fabric research** → `…/tmp/research/STALINGRAD_URBAN_FABRIC_20260928.md`.
  - Content:
    - the city centre's building types, with parameters: frontage and depth, bay, storey heights,
      wall thickness, floors (timber joists or concrete), roofs, semi-basements and cellars,
      window and door proportions, stairs;
    - their share and arrangement in a block;
    - the block and street pattern: block sizes, street widths, courtyards and yards, tram lines,
      poles and wires, fences;
    - the state by January 1943: what stood, rubble depth, burned timber districts where only
      stoves and chimneys stood, snow;
    - defensive changes: loopholes, blocked windows, barricades, mined streets, communication
      trenches through yards, cellars used as shelters and aid posts;
    - facts that constrain a tank in those streets: widths, rubble heights, firing positions
      above and below.
  - Every number is cited. It ends with a machine-readable parameter table for the grammar and
    the district.
- **R2, tableaux research** → `…/tmp/research/URBAN_INTRO_TABLEAUX_20260928.md`.
  - 8 to 12 wordless scenes for the intro district, grounded in the city centre in winter
    1942–43.
  - Each scene gives:
    - its provenance class: exact_landmark, district_event or attested_condition;
    - its sources, with URLs;
    - what the player sees: space, objects, bodies, damage, absence, weather, sound;
    - the typology and room it lives in, its placement constraints, and what must not be shown.
  - It builds on `assets/worldgen/scene_blocks/historical_scene_catalog.v1.json`,
    ACT_ONE_HISTORICAL_SCENES.md and ENVSTORY.md; it does not duplicate them.
- **P, photo analysis** → `…/tmp/research/PHOTO_REFERENCE_ANALYSIS_20260928.md`, with contact
  sheets in `…/tmp/research/photo_boards/`.
  - Go through every photograph in the reference set and look at each one. Weight the city, the
    steppe and pocket, winter conditions, damage and anti-tank emplacements. Use the manifest's
    caption, date, place and archive id to judge reliability.
  - Record what the 3D world must reproduce:
    - **Geometry and proportions, estimated with stated uncertainty:** storeys, window rhythm, wall
      thickness at breaks, roof forms, cornices and parapets, plinths and semi-basements.
    - **Damage morphology:**
      - standing facade shells and gutted interiors;
      - pancaked or hanging floors and joists;
      - breach shapes and loopholes;
      - rubble-cone heights against the storeys;
      - soot above windows, twisted roof iron;
      - burned timber districts reduced to stoves and chimneys.
    - **Streets and yards:** widths against vehicles, tram rails, poles and wires, fences, stumps,
      barricades, craters.
    - **Snow and weather:** depth, drifts against walls, soot and trampled paths on snow, haze and
      smoke, light.
    - **How people and bodies appear,** only as far as the tableaux need it.
  - Photographs are almost all black-and-white: they inform form, damage, composition and tone,
    never albedo. Albedo comes from physical material data.
  - End with numbered visual rules, each citing the photographs it comes from, addressed to:
    - the grammar (G);
    - the district (L);
    - the story (S);
    - the verifier (V);
    - maps beyond the intro district: the open winter ground around it, the steppe and pocket
      route west, later acts.
  - For each typology and environment, name the two or three photographs a render should be
    compared against.
- **F1, runtime core** (prepared worktree: urban-runtime):
  - `BuildingRuntime` becomes data-driven: walk, shelter, stair, upper-floor and storage queries
    come from the compiled floors, rooms, portals, stairs and slabs. The featured house behaves
    identically, proven by regression tests.
  - Public construction from (blueprint, placement, instance id).
  - Applies `condition_damage` at instantiation.
  - `Game` holds many instances: per-instance bindings for destruction, collision, walk surfaces,
    doors and lights; persistence keyed by (act, instance id); LAN caps reviewed.
  - Reads `building_instances` from `manifest_edits.json` (act_manifest.rs), gets blueprints from
    `building_grammar`, and places them on the terrain.
  - Uses `building_kit` as the single asset registry.
- **F2, runtime at scale** (prepared worktree: urban-runtime; after F1):
  - Building modules and their rubble render through instanced prototypes, one per kit asset, with
    per-instance visibility for failed modules, not one loose mesh per module.
  - The TLAS instance count is reported and never silently exceeded.
  - Crush, projectile, foot-collision and line-of-sight queries against thousands of modules go
    through spatial grids, not linear scans.
  - Cellars: a physically honest terrain opening inside a footprint, so the heightfield's surface,
    collision and walk surface are removed there and the building's own walls and floors bound
    the space. Otherwise the report explains exactly why not.
  - Measure with `winter-bench` on a many-building test manifest.
- **G, grammar** (prepared worktree: urban-grammar; after R1 and P):
  - The four typologies with all four conditions, from R1's parameters:
    - storeys and stairs;
    - rooms tagged by use: shop, dwelling, stairwell, cellar, shed;
    - doors and windows, support edges and collapse clusters;
    - burned timber absent; `condition_damage`.
  - `building_kit` entries for every module used, with material, a catalog review, and collision
    proxies from source geometry with a tested error bound. Timber-house pieces may be procedural
    meshes if no reviewed asset fits.
  - Tests over every typology × condition × size corner: validity, determinism, reachability.
  - Offline previews: render blueprints with Blender headless from the kit GLBs into `captures/`,
    and look at them beside the photographs P names for each typology and condition.
  - Every grammar rule P addresses to G is implemented, or the report says why not.
- **L, district** (prepared worktree: urban-district; after R1 and P):
  - `tools/worldgen/urban_intro_district.py` forks the winter-courtyard workshop into
    `assets/worldgen/workshops/act1-urban-intro-20260928/`. It writes:
    - streets, blocks and plots from R1's parameters;
    - `building_instances` inside the typology limits;
    - flattened pads with aprons in `heights.npy`;
    - dressing ops: rubble, craters, poles and wires, fences, barricades, wrecks, snow;
    - a receipt with provenance.
  - Deterministic, with a Python test and a tracked sample under `catalog/examples/` that a Rust
    test validates against `building_placement` and `building_grammar::limits`.
  - A plan image in `captures/`.
  - Keeps the courtyard house, the mission points and the spawn working. Documents any conflict
    with the encounter's routes and anti-tank gun sites.
  - The district's streets, yards, rubble, snow and dressing follow P's district rules. The open
    winter ground around the district follows P's rules for maps beyond it. Compare renders against
    the photographs P names.
- **I, integrator** (prepared worktree: urban-intro; after F2, G, L):
  - Merges the three branches and commits the research documents.
  - Builds and runs the full unit suite.
  - Runs the district tool and renders the district: spawn street view, Tiger gunner view, aerial
    overview.
  - Adds `--urban` to `tools/launch_winter_session.sh` without changing its default.
  - Writes `docs/URBAN_INTRO_20260928.md` and updates PHYSICAL_TRUTH_AUDIT and STATUS.
- **S, story** (prepared worktree: urban-intro; after I, R2 and P):
  - Builds 3 to 5 of R2's tableaux as building-attached scene content: authored in a room's local
    frame and referenced by building instance and room tag, so they move with the building.
  - Provenance in metadata, no text, a capture of each.
  - Physical traces match P's story rules and photographs.
- **V, verify** (prepared worktree: urban-intro; after S):
  - Adversarial check of the whole branch:
    - build and tests;
    - look at every capture;
    - performance with `winter-bench` on the urban workshop;
    - physical-truth compliance, provenance, determinism (rerun the district tool and compare);
    - comparison boards that put renders side by side with P's reference photographs, in
      `…/tmp/research/photo_boards/`, judged against P's visual rules.
  - Fixes small defects with commits and reports the rest.

## 7. Contract changes

(append here: date, role, change, reason)

- 2026-09-28, F1 (runtime core): `building_kit` gains an additive runtime-shape table (`SHAPES`,
  `shape(id)`, `KitShape`, `KitBox`, `KitFloor`, `KitStair`, `KitLight`) beside `KIT`; the
  `KitAsset` struct is unchanged. Reason: the runtime derives collision, walk surfaces, stair
  corridors, roof cover and light fixtures from the layout without loading meshes, so every asset
  used by a structural, door-frame or partition module needs a shape (the runtime refuses to
  instantiate a module without one). A test measures each shape against its GLB; the mesh cache
  refuses a mesh whose bounds no longer match its shape. G: add a shape for every kit asset you add
  (solids with a measured `solid_error_m`, `floor` for slabs, `stair` for flights, `light` for
  fixtures); the integrator reconciles this with G's own collision proxies.
- 2026-09-28, F1: the header comment of `building_kit.rs` now says the kit is the runtime's only
  registry (the private copy in `building_runtime` is gone). `tools/runtime_asset_cook.py` scans
  `src/building_kit.rs` as the building runtime's source of model paths.
- 2026-09-28, F1, frame note (no contract change): `InstanceFrame::placement` lowers a blueprint so
  its entry-floor walk surface (the top of its level-0 floor cells) lies on the pad at `ground_y`.
  A grammar blueprint whose foundation top is at local y = 0 is unaffected; the featured house
  (slab bottom at y = 0, floor at 0.4) is lowered 0.4 m. The featured house also has its entrance
  on local -Z, against the contract's street facade on +Z; it is only a stand-in for tests.
- 2026-09-28, F1, note for G: `KitMaterial` is not yet consumed by destruction; every building
  module is a brick destructible as before. `pizza.structure.window_frame` is marked `Timber`
  although in the featured house it forms the brick piers and sill of its wall; consuming it would
  turn those walls from a brick grind into a timber burst.
- 2026-09-28, F2 (runtime at scale), cellars are sealed for now (no contract change; G, L, S and
  the integrator please read). A placement with `cellar: true` builds, but its cellar storeys lie
  under the continuous terrain: they keep their structural role (support, damage, saves, LAN) and
  are unreachable. A stair that leads into a cellar carries and gates no one; the entry storey's
  holed slab carries feet over the stairwell, where the visible surface is the terrain 0.10 m
  below the slab top. Boxes wholly under the terrain are out of the tank crush gate. The build log
  names every sealed cellar. A physically honest terrain opening needs, together: (a) openings in
  `Heightfield` (axis-aligned footprints, a grid lookup, `height_at` = the excavation floor) and
  the terrain mesh clipped around them; (b) vehicle support by intact building floors and a
  step/drop limit instead of the tank's kinematic `pos.y = height_at` (tank.rs `Tank::update`),
  so a tank neither drops through an intact ground floor nor climbs out of a pit; (c) infantry
  navigation that keeps AI soldiers (grounded on `height_at`, infantry.rs) out of pits; (d) the
  runtime's lowest storey, not the entry storey, carrying feet, with the ground floor over a
  cellar able to fail, shelter and support down to the cellar floor, and rubble over a cellar
  falling to its floor; (e) debris, blood and casings landing on building floors. Any one alone
  leaves a visible hole with an invisible floor or a vehicle falling through intact geometry.
  Until then S should not author tableaux in cellars, and L may keep `cellar: true` (it is
  harmless) or set it false.
- 2026-09-28, F2: building modules render through `building_render` (one prototype per kit asset
  and per rubble variant); `BuildingRuntime::bind_intact_slot` became `bind_render(module,
  intact, rubble, ...)` with renderer `SceneRef` handles and `bind_destructible` lost its rubble
  argument (rubble is per module). `infantry_combat`'s cover functions take any `CoverSet`
  (a `[Cover]` still works). No placement, grammar or kit contract changed.
- **2026-09-28, G — local frame height.** A blueprint's y = 0 is grade at the footprint (the
  district's flattened pad), and `FloorDef::elevation_m` is each storey's finished floor. Ground
  floors stand 0.45-1.1 m above grade (1.0-1.1 m over a semi-basement), cellars 1.5-1.8 m below
  it, and raised doorways get external steps (Foundation modules) reaching up to about 2 m
  beyond the street and yard facades. Reason: R1 §3.1-3.2 and P G1, G6 document ground floors
  raised over semi-basements whose windows are the documented street-level firing positions.
  The origin stays at the footprint centre; the pad's apron must leave room for the steps.
- **2026-09-28, G — kit loading.** `building_kit::KitAsset` gains `parts`, `nominal_min`,
  `nominal_max` and `openings` (additive). Some kit assets use only part of a GLB (a window wall
  without its glazing) or are composites of several reviewed GLBs, so the runtime must build
  meshes with `building_kit::load_mesh(id)`, not by loading `path`; collision proxies come from
  `building_kit::proxies(id)` in asset space, exact under the grammar's quarter-turn transforms.
- **2026-09-28, G — doors.** Generated blueprints have no `DoorDef`s: the kit has no door
  leaves, so every doorway is an open `Doorway` or `ExteriorExit` connector whose module is the
  brick (or timber) doorway bay, a `LoadBearingWall`. Boarded and bricked-up doorways have no
  connector. A door-leaf kit module can add `DoorDef`s later.
- **2026-09-28, G — limits.** `limits()` keeps `bay_m = 4.0` as the footprint quantum shared
  with the district; merchant houses accept 1-4 storeys and wooden houses 8-12 m depth (R1
  ranges). Window rhythm is fitted per wall run inside the grammar.
- **2026-09-29, G — roof and cut kit modules.** `urban.metal.roof` is one pitched sheet slope
  (4 m run rising 1.06 m toward +X); a gabled roof is two `Roof` modules per bay,
  `module.roof.front.bNN` and `module.roof.back.bNN`, each supported only by its own eaves wall.
  `urban.brick.parapet`, `urban.metal.roof_lean` and `urban.metal.roof_vault` are gone;
  `urban.brick.wall_strip` (a 1 m brick strip cut from the wall bay: parapets, wooden-house
  plinths) and `urban.timber.gable` (a boarded gable triangle) are new. Both are source walls cut
  at load (`KitPart::clip`, `building_kit::clip_mesh`), so the runtime must keep building meshes
  with `load_mesh(id)`. Reason: squashed wall bays and whole-house roof sheets did not match the
  photographed parapets, gable ends and torn roofs (P G4, G5, G16).
- **2026-09-29, G — offline previews.** `cargo run -- urban-grammar captures` writes the kit
  (`urban_grammar_kit.json`) and one blueprint and preview JSON per typology and condition;
  `tools/worldgen/preview_blueprint.py` renders them with Blender and
  `tools/worldgen/grammar_boards.py` sets each beside the photographs of P §4.
