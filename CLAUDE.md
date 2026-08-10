# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Kessel** — a lone-Tiger Stalingrad escape sim. A prior Unreal/Windows attempt died of engine
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
cargo test --bin kessel                             # unit tests (currently audio.rs asset-path guards)
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

`assets/` (~6.3 GB: PSX Mega Pack model libraries, the real Stalingrad DEM, raw tank sources) is
**gitignored** — it's large, pre-existing, and mostly binary; it lives on disk untracked rather
than in history. This means a fresh clone or a `git worktree` will NOT have `assets/` — either
symlink it in (`ln -s /path/to/assets assets`) or point an agent at the existing checkout.

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
