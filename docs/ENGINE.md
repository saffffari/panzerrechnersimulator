# Kessel — ENGINE (the new authority)

> **This document supersedes the engine-specific half of the old docs.** Everything that named
> Unreal Engine, UE5.8, Lumen, Niagara, MegaLights, Datasmith, Learning Agents, Blueprints, MCP,
> Live Coding, `.uasset`, `MPC_Retro`, or `r.*` CVars describes the **abandoned Windows attempt**.
> The *vision* docs (`DESIGN_PILLARS`, `GAME_DESIGN`, `ART_BIBLE`, `STALINGRAD_CAMPAIGN`,
> `DESTRUCTION_ARCHITECTURE`, `AUDIO_ARCHITECTURE`, `ML_AI_TRAINING`, `TIGER_I_SPEC`, `CONTROLS`,
> `CREW_SEATS`) remain canonical for *what the game is* — they are engine-agnostic. This doc is
> canonical for *how it is built*.

---

## 0. Why we left Unreal (the lesson that drives the design)

The Windows/UE attempt failed not on the game but on the engine. Read the last audit
(`AUDIT_2026-06-30.md`): every blocker was **incidental engine complexity**, never a game
problem —

- the player drove a placeholder cube because Datasmith dumped **1,526 loose `.uasset`s** at
  `Content/` root, never wired to the pawn;
- the tank stood still because the pawn spawned **buried in terrain** and the swept move
  hard-stopped on it; "FindTeleportSpot… intersecting geometry" **32×/session**;
- enemies flew into space from a streaming-trace miss + `AdjustIfPossibleButAlwaysSpawn`;
- the whole live-edit loop depended on an **`unreal-mcp` bridge that silently didn't load**
  because Claude was launched from the wrong directory, and on a **Live Coding compile dance**.

None of that is *Stalingrad*. All of it is *Unreal*. The engine was the enemy. So the new plan
deletes the entire failure class: **a small, bespoke engine we fully own, on Linux, in Rust.**
No editor to fight, no import pipeline that dumps assets in the wrong place, no plugin bridge,
no hidden GameMode possession rules. `cargo run`. The whole renderer is ~a few thousand lines we
understand completely.

---

## 1. The engine in one paragraph

**Kessel** is a custom desktop engine written in **Rust** on **`wgpu`** (Metal on macOS, Vulkan on
Linux). The live renderer draws at **320×180 or 640×360** in HDR, applies the PS1 crush last, and
nearest-neighbour upscales. The Vulkan/4090 path currently performs hardware ray queries for direct
sun visibility and one stochastic sky-visibility sample; the Metal/laptop path is forward raster.
Neither live path currently computes path-traced indirect GI, ray-traced reflections, or
geometry-shadowed volumetric scattering. **"Ray-traced PS1" is the target architecture, not a
description of those missing features.** Vertex snap was removed because it damaged close/interior
geometry; affine texture interpolation remains an intentional style choice.

## 2. The three synergies (why this is elegant, not a compromise)

The aesthetic and the technology pull in the **same** direction — each PS1 trait buys budget for
the target path tracer:

1. **Low internal resolution → path tracing is cheap.** PT cost scales with pixel count. At
   ~115k internal pixels (320×360… 480×270) vs 8.3M at 4K, the RTX 4090 has **~70× headroom**.
   The intended spend is GI, ray-traced shadows, reflections, and volumetric in-scatter. The live
   renderer has not yet spent that full budget.
2. **Low-poly PSX assets → BVH traversal is fast.** The whole art library is authentic low-poly
   (the PSX Mega Packs). Fewer triangles ⇒ a shallower, faster acceleration structure ⇒ more
   rays per frame.
3. **Dither + color-quantize → hides path-tracer noise.** Few-sample path tracing is noisy; the
   PS1 crush quantizes to 5 levels/channel and Bayer-dithers — which **masks the noise for
   free**. The crush *is* a denoiser. We can run very few samples per pixel and the aesthetic
   eats the variance.

Render expensive in HDR, **crush last**. A muzzle flash blooms in HDR, *then* quantizes — so the
bloom survives into the chunky frame as blown-out clusters of lit pixels. GI, fog, and reflections
read **through** the PS1 filter instead of being replaced by it. The order is the whole trick.

## 3. Target renderer pipeline (order of operations)

This is the required destination, not the current implementation receipt. Per frame, at internal
resolution `Wi×Hi`:

```
1. RASTER G-BUFFER  (vertex shader does PS1 vertex-snap + affine warp)
   → albedo, world-normal, world-pos/depth, material-id, emissive   [Wi×Hi, HDR]
2. COMPUTE LIGHTING  (the path tracer; reads the G-buffer)
   for each pixel:
     • dim cold sun + ray-traced sun shadow (trace shadow ray vs BVH)
     • warm emissive sources → sampled, traced for visibility → bounced GI onto snow/steel
     • 1–2 indirect bounces (cosine-sampled, BVH trace)
     • ray-march the volumetric fog along the primary ray: cool-albedo whiteout +
       per-emitter in-scatter (god-rays); forward phase g≈0.3
   → HDR lit color, temporally accumulated (reset on large camera motion)
3. POST / PS1 CRUSH  (full-screen, at internal res — so dither dots are one render-pixel)
   tonemap (HDR→LDR, keep emissive bloom) → posterize to ColorLevels (5..32) →
   Bayer-4×4 / blue-noise ordered dither → optional palette-snap toward the ART_BIBLE swatches
4. UPSCALE  nearest-neighbor blit Wi×Hi → window (integer-ish, pixels stay square)
5. (optional) diegetic CRT-green HUD / debug overlay composited at display res
```

The "trace a ray" call in step 2 is an abstraction. **v1 = a software BVH** traversed in a WGSL
compute shader (works on any GPU, zero experimental deps, plenty fast at this resolution). **v2 =
wgpu hardware ray-query** (`VK_KHR_ray_query`, which this 4090 fully supports incl. SER) swapped
in behind the same interface for a speed boost. The current Vulkan ray-query pass is only a direct
lighting slice and the current portable compute pass traces against a heightfield plus tank OBBs;
neither is the software triangle-BVH transport described above.

## 4. What does NOT need a giant engine here

The game is an FTL-style single-axis escape with low entity counts and a slow, deliberate camera.
That shape means the "engine plumbing" is modest and all available as mainstream Rust crates:

| Concern | Solution |
|---|---|
| Window / input | `winit` 0.30 |
| GPU | `wgpu` 27 (→ Vulkan); WGSL shaders, hot-reloadable |
| Math | `glam` |
| GPU data | `bytemuck` |
| Model load | `gltf` (the asset library already ships **1,266 `.glb`**) |
| Images | `image` (PNG heightfield, textures, screenshot output) |
| Audio | `kira` (spatial + effects → the "sound is the primary sensor" pillar) — *later* |
| Physics | custom heightfield tracked-vehicle controller (abstracted realism) or `rapier3d` — *later* |
| ECS / world | simple struct-of-arrays / `hecs` — *later* |
| Parallelism | `rayon` (BVH build, headless sim) |
| ML inference | hand-rolled MLP forward pass; training via `candle` or external PyTorch vs the headless sim — *later* |

**The ML thread gets dramatically simpler than under Unreal.** No Learning Agents → PyTorch
bridge, no accelerated PIE. Headless self-play is just *running the sim struct in a loop with no
renderer* — the 24-thread CPU does millions of steps/sec. The sim *is* the RL substrate; the
agent uses the same control surface as the player (`CONTROLS.md`).

## 5. Verification model (how we see the work without an editor)

- **Windowed:** `cargo run` opens a window on the user's Wayland/X11 session — the user watches
  the game live.
- **Headless screenshot:** `cargo run -- screenshot <out.png> [frames]` renders offscreen (pure
  `wgpu`, no window) and writes a PNG. This is how the *agent* self-verifies the look (read the
  PNG back) and how we get deterministic regression stills. The internal low-res target is the
  screenshot source, so what we capture is exactly what ships (pre-upscale).

## 6. Repo layout

```
kessel/
  Cargo.toml            # single binary crate (split into a workspace later if needed)
  src/                  # engine + game (Rust)
  shaders/              # WGSL (raster, compute path tracer, post crush)
  assets/
    pizza/              # PSX Mega Packs — the primary visual language (~1,243 meshes, ~2,400 tex)
    terrain/            # the REAL Stalingrad 100km DEM (height16 PNG + npy + glb + meta)
    tanks/              # tiger/ (hi-res to rebuild), T34-76/, KV-1/, T-34 enemy source
    flavor/             # secondary set-dressing (buildings, trains, weapons, props…)
    baked/              # engine-ready output of the bake pipeline (generated)
  tools/                # Blender headless python: asset bake, low-res Tiger rebuild
  docs/                 # design corpus (this file is the engine authority)
```

## 7. Hardware target (the dev box = the spec)

RTX 4090 (full Vulkan 1.4 hardware ray tracing: `ray_query`, `acceleration_structure`,
`ray_tracing_pipeline`, invocation reorder/SER), i9-12900K (24 threads), 125 GB RAM, Arch/CachyOS,
Wayland. We are **not** perf-constrained; low-poly + low-res are **stylistic**, and they happen to
make the path tracer trivial. Ship target is "modern desktop with an RT-class GPU."

## 8. Old-doc → new-engine concept map

| Old (UE) doc concept | New (Kessel engine) |
|---|---|
| `MasterRetro` dial in `MPC_Retro` | a uniform pushed to the post/crush shader (keyboard / debug UI) |
| `PP_Retro` post material (posterize + Bayer dither) | `shaders/post_crush.wgsl` |
| `MF_VertexSnap` WPO | vertex-snap in the raster vertex shader |
| Lumen HW-RT GI + Reflections | **target:** shared-BSDF path tracer (software BVH → hardware ray-query); **live:** direct sun/sky visibility only |
| MegaLights (many shadowed lights) | **target:** emissive-geometry/light sampling with traced visibility; **live High:** every admitted positioned light gets a TLAS visibility ray, but sources are still bounded point models rather than sampled emitter geometry |
| Volumetric Fog + light shafts | **target:** geometry-shadowed participating-media transport; **live:** depth-bounded analytic density marching |
| Niagara blizzard / embers / tracers | GPU particles (compute) — later |
| Datasmith import → `Content/` | `gltf` loader → `assets/baked/` manifest |
| Learning Agents + NNE | headless Rust sim + `candle`/external PPO; MLP inference in-engine |
| `r.ScreenPercentage` / internal res | the internal render-target size (`Wi×Hi`) |

## 9. Build & run

```
cargo run                          # windowed, user-visible
cargo run -- screenshot shot.png   # headless render → PNG (agent verification)
cargo run --release                # the real thing
```

## 10. Roadmap (milestones — see the task list)

- **M-A** window + low-res internal target + nearest upscale + screenshot path. *(the spine)*
- **M-B** GLB loader + raster G-buffer (vertex-snap/affine) + PS1 crush post. *(the signature look)*
- **M-C** real Stalingrad heightfield + volumetric whiteout fog + camera + prop dressing.
- **M-D** compute-BVH path-traced light: GI + RT shadows + fog in-scatter (dither hides the noise).
  - *Portable interim:* raster PBR/direct lights plus a compute visibility modulation against the
    terrain heightfield and tank OBBs. This is an approximation and may not be called path tracing.
  - *Vulkan slice:* the G-buffer combine pass uses the actual triangle TLAS for one direct-sun
    shadow ray and one sky-visibility ray per surface sample with temporal accumulation. This is
    stochastic ray-traced direct lighting, not GI or a complete path tracer.
  - *Completed correction:* screen-space color gathering and ambient floors are removed from High;
    each admitted local source gets triangle-TLAS visibility. The analytic portable contact pass is
    disabled rather than presented as equivalent ray tracing.
  - *Required next slice:* expose actual scene triangles, material/texture data and modeled emitter
    geometry at secondary hits; integrate the same triangle traversal into participating media;
    build an honest portable triangle-BVH path or retain plainly labelled forward raster on Laptop.
- **M-E** Tiger first-person interior + tracked drive + gunner optic + fire (muzzle/tracer/impact)
  + one T-34 target = **first playable vertical slice**.
- then: audio (kira), destruction, the campaign axis (`STALINGRAD_CAMPAIGN.md`), ML SU-100 hunters.
