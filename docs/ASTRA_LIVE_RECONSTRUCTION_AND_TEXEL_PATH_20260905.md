# Live lighting reconstruction and the texel renderer

The optional spatial stage runs in the actual game Renderer. It reduces local variation in the
accumulated diffuse-light carrier before the original texture is applied. It is **not texel
splatting**, does not replace the current transport estimator, and is not enabled by default.
The next decision depends on frame-aligned moving video, not a standalone prototype image.

A second opt-in implementation now captures **actual live scene geometry and transport** into
one hull-local probe and can reuse validated diffuse samples in the player view. It is also not
source-face splatting. Its concrete implementation, limitations and queued proof are below.

## What is producing the current image

| Live stage | Current data and behavior | Consequence for the reported noise |
|---|---|---|
| `gbuf.wgsl` | Rasterized primary geometry, source albedo, normal/roughness and object ID. Source textures use nearest sampling; terrain also has its own material footprint treatment. | Original texture detail and silhouettes must remain intact. Filtering the finished frame would also filter this authored information. |
| `combine.wgsl` | One finite-solar-disk sample, visible local lights, one diffuse direction guided by actual daylight apertures, and one GGX reflection direction. Secondary hits reconstruct actual source triangles/materials. Outputs demodulated diffuse, specular, and immediate emission separately. | Diffuse includes direct and indirect terms. Pixel/frame hash samples can leave sparse illumination even when the underlying wall is a flat material. |
| `accum.wgsl` | Same-ID/depth/normal 3×3 spatial estimate, rigid-object reprojection, separate diffuse/specular histories, capped age 64 and minimum current weight 0.04. | Existing history helps, but is not a complete variance-guided denoiser. A moving indirect occluder may lag without a source-input change. Rapidly moving instance IDs bypass it entirely. |
| `resolve.wgsl` | Original albedo × diffuse + independent specular + immediate emission. | This is the correct bounded insertion point for a diffuse-only experiment. |
| `sky.wgsl` / `physical_environment.wgsl` | Visible sky uses deterministic atmosphere quadrature and a world-space cloud density field; the surface histories do not denoise sky pixels. | A noisy-looking cloud pattern, quadrature artifact, surface-lit horizon and temporal volume error need separate attribution. More diffuse history cannot fix every sky artifact. |
| `smoke.wgsl` / `volume_accum.wgsl` | Stochastic source-light estimation, normalized spatial/temporal source reconstruction, current physical opacity retained. | Continuous light shafts are separate from particles. Source reconstruction can still fail when reprojection or source-change decisions reject useful history. |
| `crush.wgsl` | Camera response, optional fixed-phase ordered dither and color quantization, followed by integer nearest presentation. Clean preset bypasses ordered treatment. | Ordered dither shapes an already resolved gradient. It cannot turn incorrect transport samples into correct illumination. |

The retained v4/v5 layer-attribution work is in
`artifacts/astra-audit-20260905/light-layer-comparison-v4-v5.json` and
`docs/ASTRA_RENDERING_DIRECTION_2026-09-05.md`. Its measured improvement is a particular paired
frame, not proof of stable moving gameplay. The v9 receipt called `exterior_hdr` actually shows
the cupola interior in its saved first frame. Its G0 is largely flat gray while diffuse lighting
is mottled; its name alone is not evidence of an exterior-sky test. The v10.1 driver receipt also
contains the then-opaque sight surface. Both were inspected and must not stand in for current
working optics or current outdoor appearance.

## Specific distance regression corrected

The finite-surface branch of `volume_accum.wgsl` still clamped projected depth to 0.9999, even though
the corresponding surface-history branch had already removed that cap. With near 0.025 m/far 6000 m,
that value represents about 240 m. A valid 750 m wall therefore appeared incompatible with its own
previous depth and repeatedly lost volume history.

The volume branch now retains the full finite projection range and explicitly rejects a previous
sky sample. `tests/sky_volume_sampling.rs::finite_volume_history_keeps_far_terrain_and_rejects_disocclusions`
reads the bound from the actual WGSL call, independently projects surfaces from 10 m to 4500 m, and
requires correct correspondence plus rejection of nearer occluders/sky. Float32 arithmetic was
checked independently; the coordinator owns the Cargo and GPU gates.

This change does not adjust density, light power, cloud colors, opacity or artistic dither. It does
not fix unrelated sky issues or the remaining finite-depth precision limit of the projection.

## Bounded live reconstruction implementation

Files: `src/surface_reconstruction.rs`, `shaders/surface_reconstruction.wgsl`, and narrow
post-accumulation/pre-resolve wiring in `src/renderer.rs`.

Enable only with `KESSEL_DIFFUSE_RECONSTRUCTION=1`. Absent/0 leaves the old path and allocates no
new reconstruction resources. Forward rendering and accumulation-off do not run the stage.

The pass takes the current carrier metadata, original accumulated diffuse, normal/roughness,
depth and ID. Two normalized 5×5 gathers at pixel spacing 1 then 2 produce a separate RGBA16F target.
Camera-relative tangent-plane tests avoid subtracting large world positions while rejecting
disconnected parallel surfaces. ID, normal, roughness and carrier checks keep incompatible
surfaces separate. Both steps use the **original** temporal lighting as the contrast guide, so the
second step does not progressively redefine an edge using its own smoothed guide.

This is a spatial contrast heuristic. It does not manufacture a temporal variance estimate from
history age, claim additional independent samples, or claim globally unbiased radiance across
unequal lighting neighborhoods. A constant light field and its tint are preserved; true lighting
edges on the same surface may soften and are an explicit video rejection criterion. The kernel
does not solve poorly sampled aperture integrals by itself.

The shader has no albedo, specular, emission or volume bindings. Glass, sky, emissive carriers,
invalid normals and rapidly moving instance IDs pass through exactly. The original temporal age
is retained in output alpha. Two scratch/output textures are shared only as sequential per-view
transients; each view resolves immediately. They never bind as previous-frame history. Raw
diffuse/specular/volume histories and per-view rejection data are unchanged.

Optional layer readback adds `diffuse_reconstructed` and `diffuse_reconstruction_active`; the raw
`diffuse_accumulated` layer remains available alongside it. GPU profiling names the two actual
passes `diffuse-reconstruct-1` and `diffuse-reconstruct-2`. Nothing is added to product UI.

The CPU gate parses/validates the shader, checks the uniform layout and camera-origin independence,
and checks live ordering/history wiring. The explicitly ignored
`actual_gpu_reconstruction_preserves_edges_bypasses_and_current_transients` test executes the real
GPU module on 16×16 analytic fields: constant light, noise, ID/normal/depth discontinuities,
glass/emissive/dynamic/sky bypass, and a light step. It also reads back the original input history
to ensure it was not overwritten. This small GPU gate is still separate from visual acceptance.

## Exact moving A/B to run next

`artifacts/astra-reconstruction-ab-20260905-v12/queue.json` contains two arms of one shared 8-second
first-person scenario, `fp_diffuse_reconstruction_hatch_motion_v12.json`. It looks around the cabin,
opens the cupola hatch, moves again, and closes it. Both videos retain actual game audio and use
640×360 internal pixels with integer 3× encoding to 1920×1080. Both use one frozen binary and the
same camera, timeline, lights, sampling budget and fixed diagnostic exposure.

Run the analytic GPU gate first. Then compare the clean preset before a second equal-setting
retro comparison. Inspect texture edges, small controls, steel shading, transient response and
both raw/reconstructed readbacks. Record pass timings. A slow, smeared or trailing result remains
opt-in even if its still image is attractive. This pair does not replace an exterior horizon
pan, an on-foot texture-detail walk, working glass optics, moving crew, or a long gameplay run.

## What actual texel splatting needs

### Implemented live probe checkpoint, integrated v13 source gates passed

Files: `src/live_texel_probe.rs`, `shaders/live_texel_probe.wgsl`, the shared primary draw helper
and post-history wiring in `src/renderer.rs`, and one actual hull/view setter in `src/game.rs`.
`KESSEL_LIVE_TEXEL_PROBE=1` allocates the experiment on a ray-query device. Absent/0 creates none
of its targets. Forward, accumulation-off, exterior/relay views and menus keep the eye path.

`Renderer::encode_primary_scene` now serves both eye and probe. The same live mesh/instance
buffers, source materials, visibility flags, exact transforms, frustum/page selection, full
R32Uint object identity and optical modes are submitted. A physical probe intentionally omits
view-local raster-only props. It does not load another model, substitute a curated scene, or
reintroduce the static prototype's ambient lighting. Probe shading uses the **same compiled
combine pipeline**, current TLAS/material atlas, source lights, apertures and physical atmosphere
bind group. Diffuse, specular and immediate carriers are retained separately at full HDR precision.

The initial actual eye position becomes a fixed hull-local origin, without snapping into armor.
Six explicit face frames avoid the ±Y world-up singularity. One 64×64 face is captured per active
rendered frame, with near 0.025 m/far 8 m. The budget is independent of eye image resolution.
Leaving the cabin invalidates the probe; moving the eye more than 2 m relocates it. Nearby rigid
mesh transform/visibility, lamp and weather changes invalidate old faces. Faces also expire after
0.25 s of simulation time. Multiple eye/RTT submissions cannot spend another face in the same
renderer frame. These are a bounded single-probe policy, not a finished world-grid transition.

The optional compute stage samples only after the eye's independent histories and optional
spatial reconstruction are complete. It writes a separate diffuse target. An accepted sample
must match the eye's full object ID, current source material, normal, roughness, finite depth,
local tangent plane and bounded surface footprint. Separate walls inside one hull ID therefore
cannot pass merely because their ID agrees. Cached normal is transformed through its **capture**
hull frame; positions are reconstructed in the small hull frame rather than stored as absolute
RGBA16Float world positions. The original eye depth, material pixels, specular, optical transport,
emission and volume remain unchanged. A rejected sample retains all four channels of the exact
eye diffuse input. No probe output feeds either original temporal history.

The combined diffuse carrier still includes view-dependent Fresnel factors and distance-dependent
ray offsets. This checkpoint therefore limits reuse to nonmetal surfaces (metallic ≤0.05),
roughness ≥0.45, eye/probe ray agreement ≥0.98, and nongrazing views. This bounds an approximation;
it does not mathematically make the carrier camera-independent. A 3×3 normalized gather filters
only eligible probe samples. Probe transport is currently raw; there is **no probe temporal
reconstruction**, source-face quad presentation, multiple-probe blend, or full moving-instance
history. Actor pixels bypass reuse via bit 23, but changed actor occlusion of lighting can remain
stale for the 0.25 s freshness bound. Frequent hatch/light changes can deliberately reduce reuse.
These limitations must remain visible in evaluation rather than being hidden with a beauty shot.

Actual source-material check: the Tiger GLBs contain `Interior Paint` metallic 0/roughness 0.5,
alongside metal, floor and seat materials. The current flattening loader ignores scalar factors
without an MR texture, so these parts presently use runtime 0/0.9 defaults. The 0.45 floor covers
both; no source material was edited to make the experiment pass. Material flattening itself is a
separate outstanding graphics issue.

Optional `debug_capture_light_layers` now includes `diffuse_probe_reused`, `probe_reuse_mask`,
activation state and a probe receipt. The mask's R channel means accepted reuse; zero means exact
eye fallback. It also dumps all six native faces of G0/G1/G2, full integer IDs, depth and all three
transport carriers as typed raw arrays. Face matrices, hull origin, validity times and update counts
are recorded. Readback performs no new render. GPU profiles name `live-probe-gbuffer-N`,
`live-probe-transport`, and `live-probe-diffuse-reuse`; original work counters include the actual
extra draws and passes.

Four CPU tests cover WGSL/1,408-byte uniform ABI, six-face local/world correspondence, shared
primary/transport/history wiring and source invalidation. The explicitly ignored GPU test
`actual_gpu_probe_reuses_matching_surfaces_and_keeps_every_fallback_exact` runs the actual compute
pipeline on 15 analytic cases: matching planes, authored roughness 0.5, polished exclusion,
different IDs, two planes sharing an ID, normal/material mismatch, metal, glass, optical relay,
moving IDs, sky, stale face, inactive path and a wide view change. Legal GPU render-attachment
clears produce the input/depth fields; the test checks actual output, reuse mask and unchanged
original history. This is separate from live-view coverage and temporal visual acceptance.

`artifacts/astra-live-probe-ab-20260905/queue.json` queues frame-matched, full-audio first-person
head/hatch and moving-hull comparisons on one frozen post-v12 executable. The parent owns all
Cargo/GPU work. This lane ran only Rust parsing/formatting and a scoped diff check. Subsequent
read-only inspection of the parent's `v13-unit-tests-fixed.log` records 748 + 39 passing tests;
`v13-integration-tests.log` records groups of 48, 3, 11, 7 and 5 passing tests. The ignored actual
probe GPU gate passed in 0.21 s and the v13 release completed. Exact paths and source hashes are
recorded in `artifacts/astra-audit-20260905/live-probe-source-checkpoint-20260905.json`. This does
not establish live scene coverage, appearance or runtime cost; the video comparison remains pending.

One identified next numerical test is a thin same-ID parallel sheet: the current plane tolerance
is 6 mm + 4 mm per metre (14 mm at 2 m), while the GPU wrong-plane fixture separates surfaces by
1 m. Before default use, tighten the tolerance against measured live reconstruction precision
and add a 1 cm same-ID sheet case. Frozen v13 is unchanged so its comparison remains reproducible.

### Translation-independent visible sky rays

`sky.wgsl` now reconstructs ray directions from perspective VP orientation rows. It no longer
subtracts two far-apart absolute world positions obtained from inverse VP. No camera ABI, cloud
model, density, palette or source intensity changes. The independent float32 investigation is
recorded in `artifacts/astra-audit-20260905/sky-direction-precision-20260905.json`; its large-origin
errors are numerical estimates, not measured rendered displacement. The regression
`visible_sky_rays_keep_subpixel_precision_at_large_world_origins` validates the combined shader
and compares actual glam matrix rows against independent double camera bases through a 1e6 m
origin. A real moving horizon/sky capture is still required to attribute any visual improvement.

### Remaining migration

The existing `src/texelsplat.rs` is a static, single-model prototype. It captures one 256²×6 cubemap
from a 2 m snapped origin, substitutes unshadowed ambient/sun lighting and baked-in 24-band OKLab
styling, then draws camera-facing quads with fixed 2.2× expansion. It does not use the shipping
surface/volume transport, full live world, three-probe transition, or moving instance lifecycle.
These omissions make it unsuitable as a direct replacement for the current game frame.

Dylan Ebert's reference separates fixed-origin cubemap capture, texel shading and world-quad
presentation. Grid/previous/eye probes handle transitions and newly visible regions; eye fallback
can still shimmer. His quad construction uses cube-face depth and neighboring-depth-aware
expansion. Specular materials and animated geometry remain listed limitations. His reported demo
timing measures his scene and shading, not this game's nested transport workload.
([Article](https://dylanebert.com/texel-splatting/),
[implementation](https://github.com/dylanebert/texel-splatting))

The following remains the larger migration plan. The live probe above partially implements
steps 1–3 and a conservative sampling form of step 4; it does not complete source-face splats:

1. **Make the live primary scene reusable.** Factor the current mesh/instance draw submission into
   a bounded capture interface accepting a camera and target G-buffer. Preserve mesh/material
   bindings, visibility flags, geometry IDs, current animation transforms and physical optical
   modes. Capture actual world content into a stable grid probe; never load a substitute GLB for
   a look-development command. Keep the existing eye raster as the authoritative depth/ID
   reference during integration.
2. **Separate the transport kernel from its screen entrypoint.** The existing `combine.wgsl`
   reconstructs primary world position from an eye depth texture. Extract its source-triangle,
   visibility, aperture, atmosphere and diffuse evaluation to reusable functions whose primary
   input explicitly supplies position/normal/material. Invoke the same kernel for a probe texel.
   Do not bring the prototype's ambient fill into the game. Capture-camera position and actual
   eye position must be separate inputs; Fresnel/specular/glass cannot become camera-independent
   merely because diffuse uses a fixed probe.
3. **Own stable probe identities and update budgets.** Each `(probe generation, face, texel)` needs
   its own temporal signal and invalidation. Capture source mesh/instance identity and geometry
   revision; moving geometry must invalidate/reproject its texels. Keep grid origin in the hull
   frame for the moving cabin and world frame outdoors, with an explicit transition between them.
   Schedule fixed amounts of capture and tracing work, rather than multiplying the expensive
   current transport by all probes on every frame. Start from measured work, not the prototype's
   advertised timing. Camera cuts, zoom, world regeneration, doors and destroyed assets must have
   defined invalidation.
4. **Reconstruct cube-face footprints and retain eye visibility.** Replace the fixed camera-facing
   quad enlargement with source-face depth corners and bounded neighbor-aware expansion. Reject
   splats whose projected depth/ID conflicts with the current eye raster, then fill disocclusion
   from the eye path. This is essential inside the Tiger: a probe on the wrong side of the breech
   must not paint light through it. Avoid encoding large IDs into 8-bit normal alpha as the old
   prototype does; the live world needs its full integer identity.
5. **Compose view-dependent and volume signals in the eye view.** Initial stable splats should carry
   opaque diffuse/material appearance. Keep exact eye-view glass/periscope transport and sharp
   specular independent. Re-evaluate volume opacity against authoritative eye depth; a probe's
   integrated camera ray is not the new eye's smoke segment. Apply one-pixel world particles and
   pressure optics after surface reconstruction under their existing depth rules.
6. **Apply art direction after stable lighting.** Preserve full color; expose tonal/dither treatment
   deliberately instead of importing mandatory 24-band hue shifts/black outlines. Use one internal
   pixel grid and integer nearest presentation. Require the actual moving daytime and nighttime
   game to pass before replacing the current renderer default.

These steps create a shipping path for perspective-stable texel presentation; stable Monte Carlo
lighting remains a separate reconstruction/sampling responsibility. They can share stable probe
addresses, but one is not a substitute for the other.

## Primary references and limits

Lucas Pope's documented Obra Dinn experiments concern how dither follows camera rotation and
avoids visible pattern swimming. He compared shifted screen mapping with cube/sphere mapping
and supersampling. That presentation work does not demonstrate real-time convergence of this
game's stochastic illumination.
([November 2017 development log](https://dukope.com/devlogs/obra-dinn/tig-32/))

NVIDIA's NRD is a practical alternative to developing the eventual full diffuse/specular denoiser
here. It expects suitable normal/roughness, linear depth, motion and separated signal data;
several modes also use hit-distance information that our current output does not retain. It is
not a native volume/transparency solution. The repository documents API-native integration or
an NRI bridge. A Rust/wgpu integration therefore needs explicit resource/dispatch interop and
signal conversion; it is not a shader-name substitution. Our bounded pass is not NRD and makes
none of its quality/performance claims.
([NRD official repository and integration guide](https://github.com/NVIDIA-RTX/NRD))

No Kex-specific implementation claim is made: this audit did not identify a primary Kex source
establishing that it uses the particular method proposed here. Cultic remains the user's visual
reference; the technical plan is based on the inspected renderer and the primary sources above.

## Post-v13 precision and startup correction (source, awaiting coordinator gates)

The first actual probe-enabled v13 startup failed: its camera bind supplied only the uniform,
although the real camera layout also requires the terrain-detail array and sampler. The bounded
analytic test used a simplified one-binding layout and did not cover this production integration.
The next source now passes the existing real terrain resources and uses the shared three-binding
layout in the analytic fixture. An actual enabled Renderer startup is still a required gate.
The initial fixed exposure 2.25 was too dark in the recorded cabin; those failed/off receipts are
retained. A new A/B must measure ordinary cabin exposure and use that same gain in both arms.
The accepted override upper bound now matches the existing meter maximum (360); emitters, gains,
and meter law are unchanged. This is a diagnostic override, not a new appearance default.

The old plane tolerance admitted a 1 cm adjacent sheet at 2 m. It is now a fixed **4 mm**, and the
actual GPU reuse fixture includes a same-ID parallel sheet 1 cm behind the true eye surface.
Before tightening it, numerical analysis found substantial large-world cancellation in primary
raster depth. Main and probe primary projection now use camera-relative vectors, subtracting the
model and eye translations before adding transformed vertex coordinates. Deferred reconstruction
adds the eye origin back only after unprojection. World geometry, ray-query origins, shadow/frustum
matrices, optics metadata, materials, and [0,1] depth conventions are unchanged.

Smoke, snow/particles, and pressure projection use matching relative matrices. Surface and volume
histories use relative inverse/previous VP plus double-precision rebased rigid deltas; static
instanced geometry receives the explicit eye-origin shift. CameraUniform retains its byte layout;
AccumUniform adds one vec4 (256 bytes total), checked against both actual WGSL layouts.

`tests/camera_precision.rs` contains independent f64 rigid-motion and plane comparisons plus an
ignored real GPU Depth32Float rasterization gate sharing the live primary projection helper.
Its scenes include the reported replay eye `[0.85,2.1686,179.831]`, the survey origin near
`[985,5.49,1205]`, and `[5000,4,5000]`; each uses far=24 and far=6000 and .25–7.5 m surfaces.
This proves a numerical bound only after the coordinator runs it. It does not by itself prove
that a whitewashed replay or visible noise has been fixed. The same first-person scene, raw layer
readbacks, and ordinary game camera still need visual review. Source-face splats remain next.

## Source-face diffuse cells (post-v14 source; gates pending)

`KESSEL_LIVE_TEXEL_SPLATS=1` now allocates the live probe and selects an actual raster presentation
pass in `src/live_texel_splats.rs` / `shaders/live_texel_splats.wgsl`. With both probe switches
absent, this remains off. The original probe-only switch keeps its gathered-diffuse behavior.
The pass uses the existing full-precision probe depth/ID and HDR diffuse carriers; it does not
run the old isolated beauty-shot renderer or impose a palette.

Each source cubemap texel emits a six-vertex cell whose corners follow source-camera rays to the
measured tangent plane. The cell retains exactly one source-pixel footprint, with no screen-space
billboard growth. All corners must be valid together to avoid stretched triangles. The fragment
then checks the original eye's full object ID, actual reconstructed tangent plane (4 mm), normal,
material and view-alignment guards. A 1 cm parallel plate sharing an ID is rejected. Optical,
polished, metal and moving-instance surfaces remain on the eye path. Original eye albedo,
specular, emission, volume and depth are never replaced. Discarded fragments leave an exact copy
of the original diffuse/history-age carrier. The raster bind excludes both storage outputs, so
neither render attachment aliases a shader-bound storage resource.

This is **source-face diffuse texel presentation**, not completed probe denoising or a full
renderer replacement. The raw source radiance can still change when a face updates; the surface
cells deliberately preserve those source values. No temporal stability or appearance success is
claimed. Submission is currently one draw of 24,576 cells / 147,456 vertices per active eye;
invalid cells are rejected in the shader, and actual timing must be measured.

The ignored GPU test `actual_gpu_source_face_cells_preserve_fallback_and_move_with_source` uses
the same production-layout fixture as gathered reuse and runs its material/ID/plane rejection
cases through the actual raster pass. Additional HDR source-cell stripes must retain discrete
values and change selected cells correctly when the eye translates 16 cm. Shader validation
checks the real composed vertex/fragment module. These tests are prepared, not claimed passed.
`artifacts/astra-source-texel-ab-20260905/queue.json` defines the three-arm first-person comparison
with audio and explicitly requires measured ordinary exposure before dispatch.

An additional history correction records each view's near/far range and resets both surface and
volume history when it changes. A protected 24 m cabin projection and a 6000 m replay projection
have different finite depth values despite identical camera pose; those histories cannot mix.
The source regression includes the actual 180 m cabin location and both projection ranges.

## Captured-face temporal statistics and lifecycle diagnostics (post-v15 source)

`KESSEL_LIVE_TEXEL_TEMPORAL=1` now selects the independent same-source-pixel temporal pass in
`src/probe_temporal.rs` / `shaders/probe_temporal.wgsl`, immediately after actual probe transport.
It retains RGBA32F first moments/effective capture count and RGB second raw moments/population
variance, plus exact previous source depth, full ID, material and normal. There is no spatial
filter, radiance clipping, invented variance or energy rescaling. After 32 accepted captures it
uses a 1/32 running update. A changed source generation or relocated anchor resets history;
wrong identity, a 1 cm parallel plate, material changes and moving instances reject the old
sample. Captured source and reconstructed diffuse remain separate arrays. Both gathered reuse
and source-cell presentation consume the reconstructed carrier when this switch is set.

The actual v14 film passed startup but **failed visual acceptance**: its mask was all zero,
145 faces had been captured, and only the latest face remained valid. Thus its output never
reused probe light. The source fingerprint incorrectly included invisible nearby mesh transforms;
those cannot affect the physical TLAS. The next source excludes invisible, raster-only and
BLAS-less meshes while preserving membership invalidation when a physical object becomes visible.
Receipts now distinguish geometry and illumination resets and record changed visible mesh IDs.
This is a diagnosed invalidation bug; the next real run must establish whether it explains all
of the observed resets. Small real light-power changes are now hashed exactly instead of falling
inside a 0.01 quantization bin. Geometry guards and the 0.25 s freshness policy are unchanged.

The v15 source-cell GPU fixture separately failed before dispatch because its synthetic HDR
stripe upload required COPY_DST. That capability is now added only to test source arrays;
production retained-geometry destinations explicitly request it for the real history copies.
The original production capture arrays otherwise keep their previous usage. No appearance pass
is claimed from either failed fixture or zero-coverage film.

New ignored GPU gate:
`actual_gpu_probe_statistics_converge_and_reset_on_source_or_geometry_change`. It checks real
alternating transport values against the mean and second moments, then source reset, full-ID
change, one-centimetre same-ID geometry change, material change, moving-ID exclusion and
preparation without submission. Raw/statistical arrays and per-face history flips are included
in the existing single-submission diagnostics. Tests and the actual game run remain coordinator
work; this paragraph records implemented source, not a passed render.

A separate TLAS audit found that four empty blood-stain pools reserve 65,536 physical TLAS slots
from `object_id_capacity`, consuming the entire requested TLAS limit before considering other
geometry. Flying blood, foam and fire pools are raster-only and excluded. Stable custom-data IDs
are independent of TLAS slot indices; the cap omits tail entries and does not wrap IDs. The
appropriate next fix is compact live physical moving entries with stable custom IDs, rather than
raising the cap or dropping world geometry. That enumeration change is not yet implemented here.

The same diagnostic now preserves actual full eye IDs as little-endian `g3_ids.r32u` and the original G2 passthrough/metal carrier. Its bounded emissive-source inventory records current regular mesh indices, matrices, world bounds and flags; emissive batches include active counts and first four transforms. Completed immediate-emission pixels are paired with their unmodified eye IDs and summarized by pixel count, radiance sum and peak. This identifies a surviving emissive polygon from actual carriers without changing exposure or transport. Names are not invented: the renderer retains source slot indices, not authored object names. The capture records list limits/counts, non-finite metadata flags and TLAS slot/capacity counts. G2 gains COPY_SRC solely for this opt-in readback.

After the v16 snapshot, live TLAS compaction is implemented in Renderer: physical ranges use active counts, while custom-data ID reservations and exact source records remain unchanged. Moving count changes rebuild that layout and clear all vacated slots, including removal of the last live instance; transform-only motion retains the narrower update path. CPU planning/slot-image regressions exercise empty pools, changing poses, shrinking tails and identity uniqueness. The new diagnostic separates live physical instance count from reserved source-ID count. This remains source-only pending coordinator tests and an actual Game run. The real 65,536 active-slot hardware bound remains; the fix removes empty reservations, not that bound. Count changes currently rewrite the static tail too, a known CPU tradeoff rather than an unmeasured speed claim.
