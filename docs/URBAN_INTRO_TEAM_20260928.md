# Urban intro: city team brief (28 September 2026)

The owner decided (28 September 2026): "the intro to the game is going to be urban combat", and
asked for a team of agents to build the city and its component buildings. This brief is the
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

All under `/home/alex/panzer/tank_simulator/.claude/worktrees/`:

| Role | Worktree | Branch |
|---|---|---|
| Integration (integrator, story, verify) | `urban-intro-20260928` | `claude/urban-intro-20260928` |
| Runtime (F1, then F2) | `urban-runtime-20260928` | `claude/urban-runtime-20260928` |
| Grammar (G) | `urban-grammar-20260928` | `claude/urban-grammar-20260928` |
| District (L) | `urban-district-20260928` | `claude/urban-district-20260928` |

- Each worktree is branched from the contract commit and has `assets/`, `source_assets/` and a warm
  `target/` (reflinked).
- Research agents write only to `/home/alex/.claude/jobs/ca78baf9/tmp/research/`. The integrator
  copies their documents into `docs/research/`.

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
- **F1, runtime core** (runtime worktree):
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
- **F2, runtime at scale** (runtime worktree, after F1):
  - Building modules and their rubble render through instanced prototypes, one per kit asset, with
    per-instance visibility for failed modules, not one loose mesh per module.
  - The TLAS instance count is reported and never silently exceeded.
  - Crush, projectile, foot-collision and line-of-sight queries against thousands of modules go
    through spatial grids, not linear scans.
  - Cellars: a physically honest terrain opening inside a footprint, so the heightfield's surface,
    collision and walk surface are removed there and the building's own walls and floors bound
    the space. Otherwise the report explains exactly why not.
  - Measure with `winter-bench` on a many-building test manifest.
- **G, grammar** (grammar worktree, after R1):
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
    and look at them.
- **L, district** (district worktree, after R1):
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
- **I, integrator** (integration worktree, after F2, G, L):
  - Merges the three branches and commits the research documents.
  - Builds and runs the full unit suite.
  - Runs the district tool and renders the district: spawn street view, Tiger gunner view, aerial
    overview.
  - Adds `--urban` to `tools/launch_winter_session.sh` without changing its default.
  - Writes `docs/URBAN_INTRO_20260928.md` and updates PHYSICAL_TRUTH_AUDIT and STATUS.
- **S, story** (integration worktree, after I and R2):
  - Builds 3 to 5 of R2's tableaux as building-attached scene content: authored in a room's local
    frame and referenced by building instance and room tag, so they move with the building.
  - Provenance in metadata, no text, a capture of each.
- **V, verify** (integration worktree, after S):
  - Adversarial check of the whole branch:
    - build and tests;
    - look at every capture;
    - performance with `winter-bench` on the urban workshop;
    - physical-truth compliance, provenance, determinism (rerun the district tool and compare).
  - Fixes small defects with commits and reports the rest.

## 7. Contract changes

(append here: date, role, change, reason)
