# Panzerrechnersystem engine: current architecture

Updated September 5, 2026 UTC from the HAL source and release tests. This is the implementation
reference; `DESIGN_PILLARS.md`, `ART_BIBLE.md` and the current user direction define the experience.
`PHYSICAL_TRUTH_AUDIT.md` records numerical/modeling limitations. The earlier Unreal/Windows engine
was abandoned; its blueprints, Lumen, Niagara and Unreal-specific instructions are historical.

## Runtime and platform

The active game is a custom Rust engine using wgpu 27 and winit. Linux/Vulkan on HAL's RTX 4090 is
the authoritative platform. Metal and the Laptop forward renderer remain fallback/history paths.
The code owns its renderer, heightfield/vehicle simulation, projectile and damage models, audio,
procedural world assembly, campaign, neural policies, and editor.

The integrated source is `src/`; WGSL shaders are in `shaders/`; reviewed runtime assets are in
`assets/`. The larger purchased/source warehouse is preserved separately in `source_assets/`.
`resource.rs` resolves development/cooked resources; merely finding a source file does not prove
that the game loads or presents it. `docs/LINUX_4090_HANDOFF.md` describes transfer provenance and
platform requirements. Preserve existing private assets and checkpoints.

## Live High rendering

The visual language is abstract pixel 3D with physically motivated transport, not photorealism.
Render scene radiance in HDR, apply camera response and optional retro treatment, then upscale with
nearest sampling. The selected pixel grid and retro strength are separate controls.

The current High frame comprises:

1. Raster G-buffer visibility, source textures, normals/materials and object IDs.
2. Physical sky evaluated only where depth records a primary miss.
3. Deferred surface lighting using the hardware triangle TLAS. Exact committed secondary hits
   reconstruct source triangles, barycentrics, materials and texture data. One diffuse and one GGX
   secondary estimator evaluate hit emission, shadowed direct sources and environment lighting.
   Bounded dielectric entry/exit transport handles modeled direct-view glass.
4. Separate temporal histories for diffuse and specular, followed by HDR resolution.
5. Participating media with scene-visibility queries, extinction, source sampling and bounded
   scattering. Compact local smoke/haze use single scattering; the homogeneous world snow layer
   also includes bounded higher-order paths. A per-view volume history filters effective source
   radiance while retaining the current opacity.
6. Explicit world-space snow grains with finite-shutter/ray-cone filtering, geometry admission,
   source illumination and extinction. This is a major current performance bottleneck.
7. Bloom, measured exposure, camera/retro response, and nearest upscale. UI and optics have their
   own remaining presentation debt.

This is a raster-primary, bounded hybrid renderer. It has implemented indirect lighting and
reflections; it is **not** a general converged camera path tracer. Exact source reconstruction is
also distinct from a fully correct transport estimator. Remaining work includes deeper transport,
robust ray origins, secondary texture filtering, geometric/energy error bounds and temporal
acceptance. The first September audit fixed atlas packing that silently omitted some secondary
texture data in release builds; source inspection alone had not caught it.

High uses 640×360 by default. Couch views use 320×180 each; one couch frame contains five scene
renders and possibly a periscope refresh. `KESSEL_INTERNAL_RES` overrides the grid without changing
source density or the selected quality tier. `KESSEL_NO_RT=1` is a diagnostic/fallback switch, not
an accepted replacement for High transport. `KESSEL_UNGATED_SKY=1` restores the old sky ordering
for comparisons; the sky integrand is unchanged.

Low pixel count reduces primary sample count, but does not guarantee a frame budget. Repeated
atmosphere integration per snow grain, nested source/volume sampling, scene complexity and multiple
views can dominate. Quantization can disguise some noise; it is not proof of convergence or a
substitute for a denoiser. Measure costs instead of inferring GPU headroom from a pixel ratio.

## Simulation, AI and sound

`game.rs` owns live state and integrates tank/crew/foot control, projectiles, damage, effects,
world state and audio events. `ballistics.rs`/`armor_damage.rs` provide game terminal models;
`crew_physics.rs` and related modules simulate internal contacts and bodies with documented
simplifications. Their correctness and calibration must be assessed independently of rendered FX.

Tank policy inference currently uses a small CPU MLP (`ai.rs`); training/showreel pipelines have
separate workloads. Future GPU neural budgets should be based on the implemented workload rather
than assumed contention. Fixed-step GPU/headless demonstrations are not a complete playability test.

`audio.rs` uses rodio/CPAL, procedural buffers and optional local recorded layers. `acoustics.rs`
contains deterministic parametric game DSP, stereo/head-shadow processing and impact/fragment cues.
There is no general wave/pressure solver, measured Tiger cavity response or geometry-derived
impulse-response pipeline in the current game. Editor `spatial_audio` schemas exist, but their
playback consumer is still missing from gameplay. See the September acoustic audit for the actual
event chain and research limits.

The owner Linux launcher now defaults to the verified preselected Pizza bank when available, while
respecting `KESSEL_ENABLE_PIZZA_AUDIO=0`. Its eleven pistol/rifle/explosion layers are an initial
integration, not a completed Pizza-based sound design. Raw source or standalone licensed audio must
remain outside a distributable code payload.

## World and authoring

The shipping campaign reads versioned act manifests and overlays. The old procedural generator
also supports tools/tests; changes to it alone do not necessarily change the campaign. Use the
actual manifest consumer, scene source paths and captured game view as evidence.

The editor supports object placement, grouping/prefabs, terrain/world overlays, lights and sound
metadata. A saved schema is not an implemented runtime feature. The researched scene catalog,
relocatable composed historical scenes, runtime sound emitters and the full owned asset browser
remain work for the next playable section. Companion audit documents record concrete gaps.

## Verification and reproducible commands

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo build --release --locked
cargo test --release --locked

# Actual campaign spawn/contact path, CPU/API timings and opt-in GPU pass timestamps:
KESSEL_GPU_PROFILE=1 target/release/panzerrechnersystem cpuprof solo 120 \
  artifacts/performance/solo-contact.json 13897573272538644481 contact high

# Exterior orbit; does not measure the normal cabin view or compositor frame pacing:
KESSEL_GPU_PROFILE=1 KESSEL_INTERNAL_RES=320x180 \
  target/release/panzerrechnersystem gfxbench tag 120 exterior_orbit.png high

# Owner launch, builds current source and enables reviewed local Pizza layers:
tools/launch_panzerrechnersystem.sh
```

GPU timestamp readback is confined to explicit profiling frames; ordinary gameplay does not stall
for these queries. Pass totals do not include all GPU work and may overlap. Keep timing, setup,
asset loading, audiovisual correctness and actual window/input acceptance separate.

New simulator-made PNG/MP4 files go in the flat `captures/` directory. Preserve the original paths
of retained media. Film FPS describes encoded playback, not measured rendering speed. Existing
staged showreels may assemble their soundtrack offline; label them clearly and use the live mixer
when accepting an acoustic fix.

The staged development plan and fresh measured results are in
[ASTRA_NEXT_STAGE_2026-09-05.md](ASTRA_NEXT_STAGE_2026-09-05.md).
