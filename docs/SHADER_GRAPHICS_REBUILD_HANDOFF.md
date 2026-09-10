# Panzerrechnersystem — shader graphics rebuild handoff

## Assignment

Build a new rasterized, shader-based graphics backend for the actual game on the isolated
`codex/shader-graphics-rebuild` feature branch. Alex explicitly authorized a checkpoint and a
parallel graphics branch. The existing checkout continues Playtest repairs. Do not replace its
executable, desktop shortcut, launcher settings, assets, or current branch.

This is an implementation assignment, not a request for another research document or a standalone
pretty scene. Start with a small playable vertical slice, prove the look in motion, then migrate
the remaining rendering systems. Preserve the checkpoint renderer for comparisons until the new
backend passes the acceptance checks below. Do not merge or switch Alex's default launcher to it
without showing him the actual result.

## Working directories and commands

- Playtest checkout: `/home/alex/panzer/tank_simulator`
- Graphics worktree: `/home/alex/panzer/shader-graphics`
- Feature branch: `codex/shader-graphics-rebuild`
- Handoff in the worktree: `docs/SHADER_GRAPHICS_REBUILD_HANDOFF.md`
- Read `AGENTS.md` in that worktree before editing.
- Build output must stay in the graphics worktree's own `target/`; never share the Playtest target.
- Existing proprietary/local assets are retained in the Playtest checkout. Read them through the
  supported resource resolver; do not move, recook in place, or bulk-copy them into Git.

Initial build and test commands, from the graphics worktree:

```bash
export CARGO_TARGET_DIR=/home/alex/panzer/shader-graphics/target
export KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator
cargo test --locked --bin panzerrechnersystem
cargo build --release --locked --bin panzerrechnersystem
```

`KESSEL_ASSET_ROOT` accepts a repository/resources root as well as an assets directory. It also
roots non-asset resources, so branch-specific authored resources must be handled explicitly.
Some older tools may still open relative paths directly: audit them before running; do not fix
this by redirecting the current Playtest checkout. Use branch-local copies for any asset you edit.

Add an explicit new-backend selector and a branch-local launch command when the backend exists.
Do not call an existing setting the new backend. Until that implementation lands, this branch
still contains the checkpoint renderer. Invoke this worktree's binary directly for diagnostics.

The current user launch command remains:

```bash
/home/alex/panzer/tank_simulator/tools/launch_panzerrechnersystem.sh --build --playtest
```

It builds and launches the current Playtest checkout, not this graphics worktree. Source changes
do not hot-reload into an already running process.

## What Alex actually wants

The visual target is **perspective-stable 3D pixel art**. Crisp shapes, deliberate color clusters,
legible tank machinery, readable scenery through working optics, and appealing simple lighting.
A sophisticated lighting implementation is not an acceptance criterion. A low-resolution image
of a noisy realistic renderer is not the requested art direction.

Alex's reference is [Dylan Ebert's Texel Splatting demo](https://dylanebert.com/texel-splatting/).
Open the site and its live demo, move with WASD and mouse, and inspect translation, turning,
close surfaces, lighting changes, and animation. Do not infer motion quality from still images
or claim to have watched a demo you could not operate. The article describes fixed/grid-snapped
probe origins, world-space texel quads, and disocclusion tradeoffs. Its shading description also
uses shadow rays: reproduce the desired appearance, not a dependency on that exact integrator.
Use shadow maps or another raster technique for this branch's shadows.

Alex's old actual-game reference images:

- `/home/alex/Pictures/Screenshots/next phase deliverables for a massive development effort/Screenshot_20260717_050503.png`
- `/home/alex/Pictures/Screenshots/next phase deliverables for a massive development effort/Screenshot_20260717_050653.png`

Those show clearly readable cabin geometry and detailed exterior views through the periscopes.
They are visual acceptance references, not proof that reverting the whole game to July is safe.

Existing videos are indexed at:
`/home/alex/panzer/tank_simulator/captures/astra/first_person_library_20260905.html`.
Alex particularly liked the faulty-primer / unload / replace / fire video and the main gun sound.
Reuse those videos for context; do not rerender the entire library. New short captures are needed
only to demonstrate new changes and regressions.

## Rendering requirements

1. **No required hardware RT or ray-query path.** New-backend initialization and execution must
   work without ray-query features, BLAS/TLAS construction, or the current stochastic bounce
   integrator. Shader source, adapter requests, and an actual run must establish this.
2. **Stable pixel art in motion.** Design sampling in surface/world/probe space deliberately.
   Nearest-neighbor presentation alone does not solve perspective shimmer. Handle thin geometry,
   animated tanks/infantry, probe transitions, disocclusion, and close cabin surfaces. Keep source
   geometry/depth as visibility authority; no holes, stale splats, or light bleeding through steel.
3. **Readable materials and lighting.** Tiger exterior is painted German grey, not bare metallic
   black. Preserve light cabin paint, brass, steel mechanisms, rubber, tracks, textures and normals
   as appropriate. Avoid global texture-atlas recolors. Use practical cabin lights, readable
   diffuse lighting, controlled highlights, and inexpensive directional/local shadows.
4. **No eye-adjustment effect.** Alex explicitly rejected bright vision ports causing rapid
   adaptation that blacks out the cabin. Keep exposure stable while looking around and changing
   stations. Calibrate the new renderer's scale; do not copy the old exposure number blindly.
5. **Working optics from ordinary player views.** Cupola periscopes, driver vision ports, loader
   optics, and gunner sight must show the correct exterior clearly. No station-dependent dead
   screens, misleading tint differences, nearby-pane temporal smearing, or excessive bloom.
   Raster render-to-texture cameras/relay projections are acceptable; align them to authored
   optics and preserve their view directions, fields of view, magnification and masks.
6. **Lightweight effects.** Replace expensive lighting/volumetric integration with deterministic
   shader effects, depth-aware particles/billboards, or bounded simple volume evaluation where
   useful. Smoke, muzzle/breech gas, fire, clouds, fog, foam and blood must remain visible and
   spatially correct. Do not approximate a convincing effect by making the scene opaque, noisy,
   or full of glitter. Opening the cupola must not collapse frame rate.
7. **60 FPS target on HAL's 60 Hz display.** Alex accepts roughly 50 while development continues;
   above 30 is a temporary minimum, not the architectural goal. Measure GPU and CPU separately.
   Report resolution, window size, adapter, scene, open/closed hatches, and competing workloads.
   nnUNet/Manifold may be active; do not terminate or modify those jobs. Show p50/p95 frame times,
   not just a favorable still scene or a headless readback average.
8. **Preserve game behavior and English UI.** Do not simplify the physical reload, armor, AI,
   damage, audio, controls, haptics, pause, or saving to make a rendering benchmark look good.

## Existing code and migration hazards

Start with `src/renderer.rs`, `src/gltf_load.rs`, `src/resource.rs`, `src/main.rs`, `src/game.rs`,
`src/tiger_periscopes.rs`, and the WGSL modules in `shaders/`.

The public `Renderer` facade is heavily used by game/editor/capture code. Initially preserve mesh
indices, model/visibility/emissive updates, materials and optical tags, camera fields, resolution,
render/capture/UI entry points, and instancing handles. Extract a backend boundary incrementally.
`CpuMesh` includes geometry, bounds, base-color/MR/normal images, scalar factors, normals, UVs and
tangent handedness. Authored geometry also feeds physical collision: changing transforms or part
ordering can break hatches, reload alignment, blood, foam and turret roof support.

Static pages, fixed-capacity dynamic pools and transient raster-only instances are live gameplay
interfaces. Preserve capacity checks, empty-pool behavior, active counts, bounds and object IDs.

The old High path roughly runs G-buffer -> ray-query combine -> diffuse/specular history ->
optional reconstruction -> HDR resolve -> volumetrics -> bloom -> distortion/crush -> particles
-> blit -> display-resolution UI. Its forward alternative is incomplete: `mesh.wgsl` omits
important environment/local lighting and forward glass is opaque. Selecting Laptop is not a
complete new renderer or a valid optics migration.

`render_rtt_tile` and `set_mesh_rtt_screen` are compatibility **no-ops**; stale comments describe
an older periscope implementation. Current physical periscopes use the High optical solver.
Implement and verify the replacement; do not assume the old camera atlas still renders.
`main::render_optic_composite` still actively renders exterior gunner ADS separately, followed by
the interior eyepiece mask. Preserve reticle/camera alignment and restore camera state afterward.

There are five view contexts, split-screen rules, camera-relative projection and close-range
near-plane behavior. Prove solo Playtest first, then migrate those consumers explicitly rather
than silently breaking them.

`src/texelsplat.rs` is an isolated six-face prototype, not a finished game renderer.
`live_texel_probe.rs`, `live_texel_splats.rs` and `probe_temporal.rs` are opt-in live-probe work
that still reuses expensive lighting. Reuse useful algorithms, not the existing performance
cost or an assumption that they already solve animation/disocclusion.

Keep effect inputs such as `set_smoke_slot`, `set_breech_gas`, `set_interior_atmosphere`, weather,
positioned lights and simulation ages. Replace their rendering internally. Use scene depth and
consistent compositing; do not double-apply fog through optical views. Pause must freeze effects.

## Checkpoint status — do not confuse source repair with acceptance

The checkpoint includes accumulated work since the September 3 commit, plus recent Playtest
repairs. It is a recovery checkpoint, **not a known-good release**. The integrated game test
target passed **860 tests, 0 failures, 5 ignored** and the release binary built successfully
(18.20 seconds). Receipts in the Playtest checkout are
`artifacts/play-readiness-20260909/recovery-integration-tests.log` and
`artifacts/play-readiness-20260909/recovery-release-build.log`. These are CPU/build checks;
no new whole-game visual or end-to-end player acceptance is implied by this handoff.

Recent source repairs include English HUD/prompts, six actual T-34-76 practice targets with return
fire off, stationary practice AI, direct cannon/infantry collision, spent-case alignment and grip,
reload gesture fixes, larger represented loose-ammo capacity, turret-roof dismount support, upright
held weapon orientation, painted exterior material selection, recorded Pizza lever audio, foam
cleanup, fixed exposure, and restoration of metallic environment light in the old renderer.
The breech lamp must report contents: green loaded, red spent, distinct bright yellow empty.

Known reports requiring actual validation include reload from ordinary mouse input, ammo reach,
case extraction, main-gun infantry hits, periscope sharpness/tint/off-axis visibility, interior
readability, ground/wall glitter, open-cupola frame time, roof exit and held weapons. Do not mark
these resolved solely because a helper function test passes. The mop request has no suitable
confirmed asset or completed cleaning interaction; Playtest foam expires instead.

The latest old-renderer fixed exposure of 320 came from a limited set of HDR receipts; night and
other weather were not accepted. Do not carry that calibration unexamined into a new lighting
model. The old performance mode had removed metallic environment response and made the Tiger
black; keep material errors separate from renderer architecture decisions.

Kampagne development is deferred in the main task until Playtest works. Existing saving is a
sector-entry checkpoint, not an exact mid-combat save. Preserve that behavior and label it honestly.

## Work sequence and acceptance

1. Record branch/commit, build the checkpoint, and inspect the short existing references. Establish
   fixed actual-game cameras for exterior Tiger, seated Loader, Commander ports and open cupola.
   Check what is already broken before changing it. Keep those captures for comparison.
2. Bring up a separately selectable non-RT backend using the existing scene/material inputs.
   First show painted Tiger exterior, a readable lit cabin and one correct live periscope in the
   actual Playtest. Keep local lights and basic shadows working before adding atmosphere.
3. Prove stable pixel sampling while translating/turning and while a tank or soldier moves.
   Add the remaining optics, then lightweight effects one at a time with before/after evidence.
4. Play through, using actual input paths: acquire round -> open breech -> extract spent case ->
   insert fresh round -> close -> aim -> fire -> repeat. Include a faulty primer. Open/close
   cupola, look through every station's optics, dismount onto the turret, jump down, walk/turn with
   P08, shoot a target, use extinguisher, pause/resume, resize, and switch window/fullscreen mode.
5. Run relevant tests and short GPU captures after each meaningful stage. Validate composed WGSL
   and actual rendering; no runtime pipeline errors, NaNs, missing meshes or new black materials.
   Measure open/closed cupola, ADS, effects and ordinary solo movement separately.
6. Deliver a branch-local launch command, concise architecture notes, benchmark receipt, and an
   indexed page of a few short before/after videos. New simulator media stays flat in that
   worktree's `captures/astra/`. Explain any remaining gaps. Let Alex review the result before
   proposing it as the default renderer or merging it into Playtest.

Commit small reviewable stages. Synchronize later Playtest repairs deliberately (inspect diffs
before cherry-picking). Do not reset shared history, delete the checkpoint, rewrite authored
assets, or replace gameplay systems while migrating their rendering. The success condition is
a game Alex can see, understand and enjoy playing, not a larger renderer feature list.
