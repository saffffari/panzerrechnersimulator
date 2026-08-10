# ACT 1 — DIE STADT: map strategy (2026-07-07)

Companion to `docs/SPEC.md` (design authority). This doc is the build plan for the Act 1 map —
the act the whole game now focuses on. Premise sharpened 2026-07-07: **a lone Tiger crew escaping
the Kessel — not just from death, but from Soviet captivity and the revenge of the people whose
city you helped annihilate months earlier. You retreat WEST back through the wake of your own
destruction. Some fates are worse than death.** No dialogue, no readable text ([[env-storytelling]]);
the environments carry everything.

## What exists today (critique)

The current generation (`tools/worldgen/gen_world.py` act 1 + `src/city.rs` core + `src/game.rs`
`build_act`) gives us: a 4.4×4.4 km real-DEM window, spawn E → goal W (~3.8 km), one 15×15
procedural block grid, the hand-tuned destructible 7×5 core, 5 landmark blockouts, 1 bunker,
2 seeded battle sites + 1 logistics dump, west-fringe scatter, MST roads + wires, craters, and
6 enemies at fixed constants. Strong foundations — real terrain, deterministic seeding, landmark
navigation, a destructible core. But measured against the brief:

1. **It's a field with a city in it, not a route tree.** Nothing shapes movement. Open steppe is
   isotropic: when you can drive anywhere, every run becomes the same beeline west. A directional
   tree needs *impassability and cost* — corridors and chokepoints the player reads and chooses
   between. (Real Stalingrad hands us the tools: the city is a ribbon cut perpendicular by
   **balkas** — steep frozen gullies — plus rail embankments, and the factory belt.)
2. **One environment scale.** Small/medium/large, indoors/outdoors/underground was the brief;
   today there is effectively one scale (block-grid ruins from ~10 modular pieces at uniform
   ruination 0.72) and one underground space (the bunker). The three "war scar" sites are
   themselves procedural scatter, not authored scenes.
3. **The story timeline is missing.** "Retreating through the wake of your own destruction" is a
   *two-layer* timeline in every scene: what the German advance did in autumn (your layer), and
   what the Soviet winter counteroffensive has done to it since (the layer that's hunting you).
   Nothing in the current gen encodes either layer, let alone both.
4. **Zero per-run variance.** All 6 enemies are compile-time constants clustered in one ~700 m
   pocket; the other ~4 km of map is a hike. Same seed → byte-identical run, every time.
5. **Authority is split.** `game.rs` ignores the manifest's SPAWN/ZIEL markers (its own constants
   win), and enemy positions live in the `SWARM` array — so the editor can't touch the two most
   strategic layers of the map.

## The plan

### 1. Route graph first (macro skeleton)

Author 4 west-bound corridors on the real DEM, separated by genuine impassables (balka walls,
rail embankments, rubble dams, marked minefields — all buildable from kit + terrain edits):

- **The rail line** — Hauptbahnhof → marshalling yard → embankment cuttings. Cover and grade,
  but chokepointed (a flak position owns a cutting; a hospital train never left the yard).
- **The balka line** — the gully network. Concealment, no long sightlines, ambush country;
  infantry territory, dismount-heavy, the Tiger barely fits the crossings.
- **The city spine** — the boulevard through the block grid and the destructible core. Fast and
  exposed; the swarm lives here. Landmarks tower over it.
- **The steppe fringe** — south, open, empty of enemies, brutal on fuel and weather, and it
  *looks* like the smart route until it isn't.

Corridors cross at 3–4 **junction nodes** — those crossings are the branches of the directional
tree, and the only places the player must pass through. Same start, same red X, many paths.

### 2. Encounter director, not fixed spawns (per-run RNG)

Keep the counter-hash determinism, but key it on a **run seed** (act seed ⊕ run counter). The
map never changes; the war on it does:

- **Anchors + templates.** We author encounter *anchor sites* along corridors in the editor;
  encounters are authored *templates* (overwatch pair, dug-in picket, convoy on road R, infantry
  screen in balka B, T-34-turret pillbox — a real Stalingrad practice). Per run, the seed picks
  WHICH anchors are live (e.g. 2 of 4 corridors hot), which template, what strength, patrol
  timings. Authored quality, roguelike variety — and the neuroevolution brains drop straight
  into the templates later.
- **Other RNG dials per run:** which wrecks hold fuel/salvage (economy pressure re-routes you),
  snowfall density (sightline changes → ties to the night look), flare/patrol event timings.

### 3. Environmental story sites — three scales, two timelines

Every site is layered: **your autumn** (German casings, burnt-out advance, the fountain's dancing
children pocked by *your* shrapnel) under **their winter** (fresh Soviet tracks over it, your
supply dump re-flagged, your field hospital's tents collapsed and looted, POW columns marched
east past your own signage). Objects only, never text. All from verified history: the grain
elevator siege, the Barmaley fountain, the frozen horses eaten to bone, Pitomnik's abandoned
wounded, tank-turret pillboxes, the univermag basement, the marches toward Beketovka.

- **Small (~10–40 placements):** a sentry frozen standing at his post; the horse pit; a
  boot-swap corpse pair; an undelivered mail sack spilled in a crater.
- **Medium (~50–150):** field-hospital tent row with a triage yard; a shot column in a gully;
  the logistics dump *after* the Soviets found it; a burned village from the September advance.
- **Large (2–4 total):** the grain-elevator battlefield (your fight, months cold, now a Soviet
  observation post); the rail yard + hospital train; the Mamajew slope. The destructible
  `city.rs` core stays as the large in-city set piece.
- **Underground:** cellar network beneath 2–3 city blocks (shelter remnants); a sewer crossing
  under a hot junction (risk/reward bypass); the balka dugout warren (a former German HQ,
  looted). The bunker kit already proves the method.

By the time the game is beaten — and it's hard — most of these have been found. That's the
retention curve: the map is fixed, the war varies, the stories are the collectibles.

### 4. Tooling this needs (WORLDBUILDER)

- **DONE 2026-07-07:** full library in the browser (1,267 assets, all thumbnails pre-baked),
  visible-cursor editing, drag-drop any library asset into any act (`ensure_world_asset`).
- **DONE 2026-07-25 — PREFABS:** Shift+LMB multi-select (group grab/rotate/scale around the
  centroid), F6 saves the selection as `assets/worldgen/prefabs/prefab_NN.json` (relative
  placements, per-piece height-above-ground so stamps re-drape over terrain), browsable under a
  "Prefabs" category with composite-baked thumbnails, stamped by drag-drop/V as a selected group.
- **DONE 2026-07-25 — MARKERS:** manifest markers render as color pylons (Spawn green, Goal red,
  anchors orange, landmarks cyan), pick/grab/delete like placements, M drops enemy anchors
  (`AnchorT34`/`AnchorSu100`/`AnchorKv2`/`AnchorIs2`/`AnchorIs3`/`AnchorIsu152`, `...City` for
  in-city behavior) and `Site` bookmarks; saved as `marker_*` ops in the same edits overlay.
  Also: F10 play-hop (save → launch the game into the act at the editor camera).
- **Zones (still open):** paint corridors, impassables, and story-site bounds into the manifest
  (a `zones` section) for the encounter director to read. Encounter ANCHORS exist now as markers;
  zones become useful when the director itself lands (§2).
- **Point lights** (queued renderer feature) — cellars, dugouts, the night look.

### 5. Single source of truth

**DONE 2026-07-25 for markers:** `world::act` reads Spawn/Goal/landmark positions from the act
manifest (+ the editor's `manifest_edits.json` overlay) via `editor::load_markers`, and
`game::build_act` replaces the `SWARM` constants with manifest `Anchor*` markers when present
(vehicle from the kind string, count capped at the 6 fixed rigs, constants pad the rest — the
cap lifts when the encounter director replaces fixed spawns per §2). The compiled-in constants
remain the fallback for a box without `assets/`. Zones remain open, with the director.
