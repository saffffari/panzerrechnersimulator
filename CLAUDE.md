# CLAUDE.md

## First principle — environmental storytelling

**Panzerrechnersystem is an environmental-storytelling game that tells one coherent story
without a single spoken or written word, exclusively through the environments the player explores.**

Every authored scene must therefore satisfy two non-negotiable rules:

1. The player can understand its human event from space, objects, bodies, damage, absence, weather,
   sound, and the consequences of action—never an exposition line, collectible diary, subtitle, or
   readable lore note.
2. The scene is grounded in a specific real event or a well-attested recurring condition from the
   campaign's history. Record the source provenance in authoring metadata; do not invent atrocities,
   victims, or melodrama merely to make the game darker. The project is an anti-war work about a
   tank commander encountering the offensive's aftermath. Horror comes from historical truth and
   restrained material evidence.

Treat scene blocks as movable authored evidence, not fixed map coordinates: detailed interiors and
hand-placed storytelling must survive procedural relocation, terrain rebakes, and route changes.
Source truth attaches to the tableau itself. It does not make the player's vehicle, adjacent route,
or compressed campaign chronology historically literal; validate those separately and never imply
that one crew witnessed exact events which occurred years or hundreds of kilometres apart.

## Second principle — physical truth, with no hidden cheats

**Panzerrechnersystem's deliberately low pixel resolution buys simulation and light-transport fidelity. It is
never permission to replace either one with an artistically convenient fake.**

For the shipping renderer and gameplay simulation:

1. Light originates at modeled emitters or the environment, follows one shared physically based
   transport model, is occluded by the rendered scene geometry, and interacts with participating
   media through extinction and scattering. Never brighten a view with camera-distance fill,
   unshadowed proxy lights, painted screen-space bounce, analytic "god-ray" cones, or a post-process
   veil presented as physical atmosphere. If the scene is too dark, change a real emitter's power,
   its material, exposure, or the physical medium—not the laws of the image.
2. The 4090 and laptop presets may change internal resolution, rays/samples per pixel, denoising,
   update cadence, and convergence time. They may not change the scene's emitters, occluders,
   material response, damage outcome, vehicle dynamics, perception truth, or any other physical law.
3. Simulation results come from the authoritative modeled state. A visible shell, fragment, crew
   body, building part, tree, carried item, or vehicle may use a numerically discretized solver, but
   no hidden primitive may contradict its visible geometry or manufacture a scripted result. Any
   unavoidable collision/solver simplification must be generated from the source geometry, carry a
   measured error bound, and be named plainly in code and status reports; never call it exact.
4. Do not label direct-light ray queries, SSAO, screen-space color gathering, analytic OBB traces,
   post effects, or temporal filtering as path tracing, GI, volumetric transport, or physical
   simulation. Implemented status describes what the live code actually computes, including its
   limits. Vision documents describe targets only.
5. Every new graphics or simulation shortcut requires an explicit debug-only gate and a regression
   proving it is disabled in both shipping presets. A performance problem is solved by profiling,
   fewer samples, lower resolution, better acceleration structures, or less work—not by silently
   changing truth.

`docs/PHYSICAL_TRUTH_AUDIT.md` is the live debt ledger and release gate. Update it in the same change
that lands or removes a renderer/simulation approximation; never let a vision document promote a
target into implemented status.

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Panzerrechnersystem** — a lone-Tiger Stalingrad escape sim. A prior Unreal/Windows attempt died of engine
friction (see `docs/AUDIT_2026-06-30.md`), so this is a full rebuild: a small custom **Rust +
wgpu** engine (→ Vulkan on Linux) we fully own. No editor, no import pipeline, no plugin bridge —
`cargo run`. `docs/ENGINE.md` is the engine authority; the other `docs/*.md` files are largely
**vision docs carried over from the abandoned Unreal attempt** — engine-agnostic in intent, but
some (`KNOWN_ISSUES.md`) describe features/bugs from that old codebase and have NOT been
reconciled with current Rust source. (`CONTROLS.md` and `DESTRUCTION.md` ARE current — rebuilt
2026-07-06.) When a doc conflicts with `src/`, trust `src/`.

## Commands

```sh
cargo build                                        # compile check (no assets needed)
cargo run                                           # windowed play — see gotcha below
cargo run -- screenshot out.png                     # headless render → PNG, gunner-optic-on-enemy view
cargo run -- screenshot out.png X Y Z YAW PIT FOG   # headless, explicit external recon camera
cargo run -- fire prefix                            # headless: forces a shot, writes prefix_{muzzle,tracer,impact}.png
cargo run -- drive prefix                           # headless: drives west ~2.5s, writes prefix_{before,after}.png
cargo run -- crush prefix                           # headless: track crush — grinds through the demo wall, writes
                                                    #   prefix_{before,grind,through}.png + asserts the crossing (PASS/FAIL)
cargo run -- treefall prefix                        # headless: destructible trees (arena) — fir under tracks, pine
                                                    #   shoved over, tree shot down; 4 PNGs + PASS/FAIL asserts
cargo run -- overpen prefix                         # headless: pen ray — AP kills a T-34 THROUGH the demo wall
                                                    #   (wall survives), two HE bursts then demolish it; asserts
cargo run -- koop prefix                            # headless: KOOP splitscreen + STÜTZPUNKT take-&-hold verify — walks
                                                    #   the mission phases with a synthesized couch CrewFrame, writes
                                                    #   per-seat views + a composed split PNG, PASS/FAIL asserts
cargo run -- absitzen outdir                        # headless: on-foot slice — dismount, walk, loot, Luger, stealth beats
cargo run -- lasttiger                              # headless: the Last Tiger loop — bail burning, survive cook-off, remount a spare
cargo run -- climb                                  # headless: the mount/dismount climb ritual (3 timed moves each way) + jump
cargo run --release -- train [gens] [pop] [secs] [acts]  # neuroevolution: ES over enemy policy, acts="0,1" = mean fitness across maps
cargo run --release -- duel outdir [weights] [act]  # film a T-34 duel (aerial cam → mp4); act 0 = the training arena
cargo run --release -- train-foot-ai [epochs] [dir] # deterministic CPU KSL2 foot candidates; default assets/ai/foot_candidates/epochs_<epochs>; refuses overwrite
cargo run --release -- campaign-ai-run out.jsonl 1 seed max_s t34.bin tiger.bin [foot.jsonl]
                                                    # physical Act-I rollout; optional role-separated DAgger JSONL
cargo test --bin panzerrechnersystem                             # unit tests (currently audio.rs asset-path guards)
cargo run --release                                 # optimized
cargo run -- edit 2                                 # live WORLDBUILDER on act 2's manifest: fly
                                                    #   (WASD+mouse, Q/E, Shift), LMB pick (+Shift
                                                    #   multi; groups G/R/T rigidly), X del, C dup,
                                                    #   V spawn, M drop marker (Shift+M kind —
                                                    #   enemy anchors/Site; pylons are movable and
                                                    #   AUTHORITATIVE: world::act spawn/goal/
                                                    #   landmarks + game enemy anchors read them),
                                                    #   B scatter brush (browser asset or selected
                                                    #   mix; organic clumps, +Shift erase, wheel
                                                    #   radius), F6 selection -> prefab (stamped
                                                    #   from the browser's Prefabs category), F5 save ->
                                                    #   manifest_edits.json (engine + blender both
                                                    #   apply), F10 play-hop into the act at the
                                                    #   camera, auto-reloads on regeneration
```

World → Blender authoring (the homeland acts are built as a Blender collaboration — procedural
base generated here, fine-tuned by hand in Blender; Act 1's runtime district still assembles
in-engine via `src/worldgen.rs`):

```sh
python3 tools/worldgen/gen_world.py 2 assets/worldgen/act2       # stage 1: placement manifest
python3 tools/worldgen/make_ground_tex.py 2 8192                 # stage 1.5: hi-res ground texture
                                                                 #   (satellite color × per-class PSX grain)
blender --background --python tools/worldgen/assemble_blend.py -- \
  assets/worldgen/act2/manifest.v1.json assets/worldgen/act2/kessel_act2.blend [overview.png]
blender --background <world.blend> --python tools/worldgen/render_shot.py -- \
  out.png PX PY PZ TX TY TZ [lens]                               # verify a spot (ENGINE coords)
```

All simulator-generated screenshots, rendered stills, posters, contact sheets, and videos belong
in the repository-root `captures/` directory, with no subdirectories. Bare engine output names and
prefixes are routed there automatically. For tools that require an explicit output directory, pass
`captures`. Use a descriptive command/run prefix, seed, date, or frame identifier so a new capture
cannot silently replace another one. Do not mix runtime textures, packaged artwork, build-cache
previews, or downloaded visual references into this folder.

**Headless screenshot mode is the primary verification loop**, not the windowed one — an agent
(or you) renders a PNG and reads it back to confirm a change, deterministically, with no display
needed. All three subcommands build the full `Game` (terrain + tanks + FX) then render exactly the
internal low-res buffer that ships, before upscale.

**Windowed `cargo run` gotcha:** on this dev box the live window has intermittently died to
external causes (a KWin tiling script forcibly resizing the exclusive-fullscreen surface was one
confirmed root cause, since removed) — if a live launch dies with exit 144 / no output, that's an
environment/compositor issue, not necessarily a code regression; verify via headless screenshot
first.

No lint command is configured beyond `cargo build`'s warnings.

## Architecture

### The rendering thesis (drives every rendering decision)

Render the scene at **full fidelity in HDR first** (real lighting/materials/reflections), **then**
crush it to a PS1 frame as the *last* step (posterize, ordered+animated dither, nearest upscale —
the vertex snap was REMOVED 2026-07-04: it mangled close/interior geometry; high-res meshes render
faithfully at the low res). Never skip the crush for anything — richness added upstream (PBR, shadows,
reflections) still reads through the filter; that's the entire point of "ray-traced PS1." See
`docs/ENGINE.md` §2 for why low internal res + low-poly + dither is a genuine synergy, not a
compromise.

Per-frame pipeline (`src/renderer.rs`): sky/fog → raster mesh pass at internal resolution
(640×360, HDR `Rgba16Float`, push-constant per-draw model matrix + emissive params) →
`shaders/crush.wgsl` (ACES tonemap → cold color grade → posterize → dither → optional CRT-green
reticle) → nearest-neighbor blit to the window/PNG. `shaders/mesh.wgsl` does affine
UV warp (no vertex snap — removed) and lambert-ish shading; a full PBR path (metallic-roughness +
normal maps, feeding the same HDR target) is an active work area — check `shaders/mesh.wgsl` and
`src/gltf_load.rs`/`src/renderer.rs` material plumbing before assuming flat-Lambert-only.

### Module map

- `src/main.rs` — winit event loop, GPU device setup, CLI subcommand dispatch, raw input →
  `TankInput`. Mouse is captured/hidden and drives commander free-look; keys are polled per-frame
  (not event-per-press) via a `Keys` struct.
- `src/game.rs` — `Game` owns the world: heightfield, player `Tank`, enemy `Tank`, view-mode state,
  FX timers, audio. `Game::build()` assembles the CPU-side scene mesh list (terrain, set-dressing
  props, tank parts, FX placeholders) and hands it to `Renderer::new`; slot indices into that flat
  mesh list are computed once at build time and stored in `Slots` — **anything added to the scene
  list must go in before the tank/FX pushes**, or those indices drift. `Game::update()` runs
  physics, AI, camera placement, and pushes per-frame transforms into the renderer.
- `src/tank.rs` — `Tank` (pose/turret-yaw/gun-pitch/hp/reload) and the per-tank `Parts` rig
  (turret pivot, gun trunnion, gun length, commander-eye offset — all in the model's local frame).
  Tiger and T-34 have distinct `Parts` constants sourced from the Blender bake's pivot/camera
  extraction. `build_tiger()`/`build_t34()` load baked GLBs (see Assets below) with a
  procedural-box fallback if a bake is missing.
- `src/renderer.rs` — GPU resource owner: HDR/depth/LDR render targets, the mesh pipeline, the
  crush pipeline, per-mesh GPU buffers (`MeshGpu`) with `set_model`/`set_visible`/`set_emissive`.
- `src/camera.rs` — yaw/pitch/fov camera; `set_forward(dir)` derives yaw/pitch from a world
  direction (used for "look down the gun barrel" and turret-relative basis).
- `src/gltf_load.rs` — minimal GLB → `CpuMesh` loader; bakes node transforms, extracts base-color
  (and, where present, PBR) textures.
- `src/terrain.rs` — loads a windowed crop of the real Stalingrad 100 km DEM into a heightfield;
  `height_at(x, z)` bilinear-samples it for grounding.
- `src/audio.rs` — self-contained, optional (`Audio::new() -> Option<Audio>`, never panics if no
  device). Two independent looping systems: `Ambience` (Interior/Exterior, crossfaded on state
  change) and a generic `LayerState` pattern (e.g. the distress drone) that fades a single loop
  in/out on a bool condition. Looping clips are pre-processed at load (`seamless_loop_blend`) —
  the tail is equal-power-crossfaded into the head once, so `repeat_infinite()` has no audible
  seam. One-shot `Sfx` variants are fire-and-forget: each `play()` call gets its own detached
  `Sink`, so overlapping effects layer naturally (this is how "stacked" gun-fire layers work).

### Assets

`assets/` (~5.4 GB after the 2026-08-10 source-material archive: PSX Mega Pack libraries, the real
Stalingrad DEM, and baked runtime art) is
**gitignored** — it's large, pre-existing, and mostly binary; it lives on disk untracked rather
than in history. This means a fresh clone or a `git worktree` will NOT have `assets/` — either
symlink it in (`ln -s /path/to/assets assets`) or point an agent at the existing checkout.

The reviewed runtime GLB closure is tracked in `catalog/runtime_assets.v1.json`. Validate it offline
with `python3 tools/runtime_asset_cook.py validate`; cook it into a new ignored directory with
`python3 tools/runtime_asset_cook.py cook --output target/runtime-assets/v1`. See
`docs/RUNTIME_ASSET_CATALOG.md` before changing the selected set or its license metadata. This is a
model-only foundation, not a runnable package: the engine still needs packaged-resource path
resolution plus terrain, audio, fonts, gameplay data and metadata alongside the cooked GLBs.

Raw tank sources (`.blend`/`.fbx`, ~100+ MB each) are **not** loaded directly by the engine. A
Blender headless bake (see `docs/ENGINE.md` and prior bake scripts under scratchpad dirs — the
pipeline isn't yet promoted to a checked-in `tools/` script) exports low-poly-or-full-res
hull/turret/gun GLBs to `assets/tanks/<tank>/baked/`, with:
- **origins matched to the rig**: hull origin at ground contact; turret/gun origins at their
  respective pivots (so the engine can rotate them in place via `Tank::turret_model()` /
  `gun_model()`);
- axis convention `engine = (blender_x, blender_z, -blender_y)` (Blender Z-up → engine Y-up,
  barrel-forward = engine +Z), via Blender's default glTF Y-up export;
- a sibling `<tank>_cameras.json` recording the model's authored crew-camera world positions
  (used to derive `Parts` eye offsets — though the shipped gunsight is offset off the *gun*, not
  the raw interior camera, since the interior camera renders the turret's own backfaces).

`src/tank.rs`'s `TIGER_PARTS`/`T34_PARTS` constants are hand-derived from that bake's reported
pivot coordinates — if a tank is re-baked with different origins, those constants need updating
to match.

### Deterministic pseudo-randomness

No `rand` crate dependency. Anywhere randomness is needed (enemy aim dispersion, scene-dressing
scatter, audio stinger timing/selection), it's a counter-seeded integer hash
(`n.wrapping_mul(2654435761) >> 8 ...`) rather than a stateful RNG — keep this pattern for
consistency rather than introducing `rand`.
