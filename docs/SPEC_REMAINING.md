# Panzerrechnersystem — REMAINING SPEC TARGETS (finalization list, 2026-07-01)

> **Current campaign authority (2026-09-02):** `docs/STATUS.md` and
> `docs/LINUX_4090_HANDOFF.md` supersede this document wherever they differ. The canonical
> destination is Berlin and the unwinnable Reichstag last stand. A hidden escape by surviving crew
> to an intact cottage is only an optional, near-impossible, deliberately ambiguous hope coda.
> Earlier Halbe, hometown/pasture-as-main-ending, conventional final-tank-battle and
> one-man-protagonist concepts below are retained as historical design or implemented-prototype
> context, not the current target.

> The complete open-target list compiled at spec-finalization. The user assigns the build order;
> IDs are the reference handles. Locked design context: kessel-* memories, WORLD_AND_NAVIGATION,
> PLATOON_COMMAND, ACOUSTICS, RESEARCH_ACTS, MECHANICS, BUILD_ORDER (superseded by this list's
> ordering once the user assigns it).

## S — Simulation engines
- S1 Ballistics calibration vs docs/research/data/ballistics.json (test-standard reconciliation,
     drag/pen anchors, KwK 43 + full ammo catalogue)
- S2 Visible shell arc + gunner holdover (drop model exists; fly the arc, mil marks)
- S3 Acoustics wiring completion: distant report via speed-of-sound queue (flash→bang), buttoned
     interior muffle on outside sounds, ISO-9613 distance filter, ILD/ITD per-ear upgrade
- S4 Full interior spatial acoustic sim (cavity modes, per-plate mass-law TL) — deliberately late
- S5 Mobility/terramechanics: snow/ice/spring-mud grip, ground pressure, thrown track off-road
- S6 Powertrain: engine/transmission/FUEL consumption/overheat, cold-start ↔ Horchhalt
- S7 Weather fronts: blizzard/clear (Act 1), spring rain (2/3); sightline+sound+mud coupling

## W — World & engine plumbing
- W1 IN-ENGINE ASSEMBLY (decided 2026-07-01, supersedes blend→engine round trip): the world
     assembles IN ENGINE; the user hands hero MODELS (Blender → GLB, tank-bake conventions:
     ground-contact origin, Y-up, +Z fwd) placed at authored locations. DONE: landmark hero-swap
     loader — assets/hero/act<N>/<slug>.glb hot-replaces a proportioned placeholder tower
     (world::slug pins names; 'drop file X to replace' logged at build). REVISED (user, 2026-07-01): the user does NO Blender pass —
     ALL work happens in-engine from the existing asset library. Blender assembler removed;
     gen_world.py demoted to reference. NEW W1 = port the full generator (buildings WITH
     interiors/furniture/stairs, bunker, landmark blockouts, countryside fringe) into Rust
     in-engine worldgen at act scale.
- W2 Scene refactor: stable handles (kill Slots/FX-tail), frustum culling, instancing (prereq W1)
- W3 Acts 2/3 terrain from downloaded GLO-30 DEMs + SPRING reskin (green, foliage, no snow)
- W4 Worldgen v2: roads/rail, real Stalingrad landmark coords, Seelow field-grid/canals/scarp,
     Halbe forest/firebreaks/town, wreck fields
- W5 Act progression in-game: loading screens, spawn/goal-gate wiring, per-act restart, one-shot
     finale (world.rs data exists, unwired)
- W6 Bunker sub-levels: loading-screen interiors, on-foot exploration + loot

## N — Navigation
- N1 Compass ribbon HUD
- N2 Paper map (M): Messtischblatt-styled, spawn cross + red X, landmarks
- N3 Pencil drawing, persistent strokes
- N4 Night flashlight cone on the map

## T — Tanks & assets
- T1 new_tanks bake: FBX→split hull/turret/gun GLBs (Blender 5.1.2 FBX importer broken — older
     Blender or glTF re-download); wire T-34/85, KV-1, IS-2, Panther, Tiger IIs, Pz III
- T2 Missing vehicles: KV-2, ISU-152, IS-3, SU-100 (not in the new set) — source or kitbash
- T3 Wreck variants (fused/burnt) incl. German vehicles as dressing
- T4 Act-3 wingman Tigers — two ADDITIONAL AI-crewed Tiger Is (reuse the player bake + Parts rig,
     AI crew brains; NO new playable rig). Playable King Tiger CUT 2026-07-02 — the player keeps
     one vehicle + one interior for the whole game. (TigerII armor scheme stays in ballistics.rs
     for wrecks/dressing only.)
- T5 Hero Tiger interior from CAD (long-term, optional — no user Blender pass planned)

## C — Combat & gameplay
- C1 Loot/consumables: cigarettes, shells BY TYPE, MG ammo, pistol ammo, fuel; scavenge sites
- C2 Absitzen port to main + explorable interiors
- C3 KV-2 / ISU-152 boss encounters (hand-tuned until evolved policies land)
- C4 Enemy MG fire vs player (gravel synth ready)

## A — AI (docs/NEUROEVOLUTION.md, PLATOON_COMMAND.md)
- A1 Sim/render split + headless self-play harness (M0 — everything hangs off it)
- A2 Policy net + evolve solo T-34 → A3 squads → A4 chain of command; co-evolving Tiger; frozen
     champion shipping (train==ship)
- A5 Per-duel boss models + the armada (King-Tiger-vs-armada co-training)
- A6 Platoon command via THE ENIGMA MACHINE (locked): a physical Enigma in the Tiger (user models
     it) is the ONLY language channel in the game — typed orders -> LLM parser -> executors; typed
     replies generated from the tanks' actual sensed state (playthrough-current, not canned JAWOHL;
     light model OK; open conversation supported). Part of the environmental-storytelling rule:
     no dialogue/spoken/readable words anywhere else (kessel-envstory).

## R — Rendering
- R1 M-D compute path tracer: software BVH → GI/RT shadows/reflections/volumetric in-scatter
     (HW ray-query later behind wgpu experimental flag)
- R2 Spring rendering for homeland acts (pairs with W3)
- R3 Asset re-grade (dark albedos) + hi-res Tiger rebuild
- R4 VOLUMETRIC CLOUDS (user, 2026-07-01): Act 1 lives inside the blizzard whiteout; Act 2 opens
     onto a spring sky with real volumetric cumulus — the homecoming's cruel beauty. Pairs with
     W3/R2; later fed by the path tracer's in-scatter.
- R6 OPTICS LIGHT PASSTHROUGH (user, 2026-07-01): periscopes/gunner optics/vision slits in the
     interior model are solid — they need real-world light coming through. Acceptable non-real
     implementation: mark optic-glass surfaces as own-colour emissive driven by an outside-luminance
     uniform (sky/sun/fog state), or render-to-texture for the gunner optic proper. Needs the optic
     parts as separate meshes/nodes in the interior GLB to target them.
- R5 GPU-PARTICLE FX OVERHAUL (unified compute particles through the HDR→crush path): explosion
     fireball + smoke column + dirt/snow fountain (impacts are emissive BOXES today), 88 muzzle
     ground-blast dust ring, persistent wreck smoke columns + battle haze, MG spark showers/dirt
     spits, tracer flight arc + burnout tail (pairs with S2), track snow/dust spray, exhaust.
     Every emitter doubles as a light source once the path tracer (R1) lands.

## M — Mechanics recommended to LOCK (from docs/MECHANICS.md top-20)
Counter-battery in pencil · Klang-Bibliothek · Reading the kill · Silence Gradient · Loader's
dilemma · Angling doctrine (needs true hull-facing geometry for hits on the player — S cost) ·
Horchhalt · Walk the splash · Photograph Alignment · Intercom Roll Call. (Concussion: built.)

- W7 ENVIRONMENTAL-STORYTELLING SCENES: recreated verified historical micro-dioramas (see
     kessel-envstory + docs/research/data/envstory.json once the fleet lands) placed as landmarks /
     on-foot interior finds — the user hand-builds hero versions in Blender; worldgen places stand-ins.

## X — New concepts proposed at finalization (accept/reject)
- X1 Map-as-save: pencil markings survive death within an act — knowledge is all you keep
- X2 Rasputitsa routing: spring mud as navigation pressure (roads fast but watched; fields bog)
- X3 The Halbe column: civilian wagons/trucks trail the Act-3 breakout (vehicles only) — the
     moral stake of the finale, historically grounded
- X4 Tigerfibel pages: the real crew manual as lootable diegetic tutorials (angling, ranging)
- X5 Ammo by eye: no HUD counter — the loader knows the racks by looking

## Standing recommendation
W1 + W2 first — every Blender hour on Act 1 only ships if the engine can load it back, and the
scene refactor is what lets a 12k-object act run. All else orders freely behind them.
