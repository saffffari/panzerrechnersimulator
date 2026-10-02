# Cabin atmosphere and portable source visibility checkpoint — 2026-10-01

This revision removes camera-dependent weather exclusion and the portable renderer's unshadowed
light fallbacks. It does **not** establish complete light transport or final visual acceptance.
Native gameplay captures remain the acceptance evidence.

## Cabin and exterior air

The three-domain cabin support is built from the loaded source hull, turret, cupola and closed
hatch assemblies, plus explicitly source-derived ring interface membranes. It is a 2 cm XZ
source-projected floor/roof interval representation, packed into a 174 × 329 × 3 RG32Float atlas.
The 384-byte uniform carries actual hull/turret inverse poses and rejection bounds. The medium
query uses source columns, not an AABB, a camera flag, or the rejected convex envelope. Earlier
convex measurements showed 40.3 cm of exterior free-space exclusion ahead of the driver's visor;
that candidate is not used.

Camera rays integrate exterior weather only outside these domains and cabin aerosol only inside.
A ray looking through a viewport or open hatch resumes the same exterior blizzard after leaving
the cabin. Source-to-receiver point connections also integrate exterior and cabin Beer extinction.
The weather density and its moving wind field are unchanged. Cabin density uses the existing
physical aerosol coefficient of 0.018 m^-1 multiplied by the existing concentration/extinction
factors; there is no exposure compensation or added cabin fill.

Limits: these columns represent compartment support, not a complete air/steel Boolean. Real
opaque surfaces terminate visible rays. Nearest-column XZ sampling has a 1 cm half-cell error in
each horizontal axis; this is not a global topology/Hausdorff proof. Open-hatch turbulent exchange,
pressure flow and ventilation are not simulated. Local breech/plume self-extinction on every
source-to-surface connection is not yet included. Raster smoke still uses one optical-depth
centroid per support, rather than multiple visibility samples through each plume.

## Geometry visibility

The portable renderer registers actual position/index/UV/source-alpha data once, with exact
content-checked deduplication, and rebuilds/refits visible instance membership per view. Terrain,
armor, moving hatch/weapon parts, visible local bodies and solid instanced geometry participate.
Hidden or zero-volume raster instances do not. Small CPU-deformed actors use private, bounded
BLAS refits; clones get copy-on-write identities. Solid grass uses the same packed world wind
and quadratic source-vertex bend as the raster vertex shader; conservative bent BVH bounds keep
that geometry traversable outside its rest bounds.

Surfaces, raster smoke and particles query this geometry for the Sun/Moon beam, each of the
32 physical environment-probe directions, and each selected point source. There is no all-lit
out-of-atlas edge, inactive-atlas fallback, or 512 m point cutoff. Environment connections use a
finite 100,000 km numerical ray endpoint, beyond the loaded game scene. The old point and sky
map banks now allocate only dummy textures; they no longer render duplicate visibility work.

Point rays end at an exact registered emitting surface, or the compact source position. They
never shorten by a clustered fire's enclosing radius, which could otherwise skip intervening
armor. The radius still regularizes source intensity and remains a compact emitter approximation;
this is not sampling of every actual flame surface. A transient source without a registered
emitting surface may self-occlude in its own opaque surrogate geometry. It must be reviewed in
the native firing/fire captures, not silently bypassed by excluding nearby armor.

Fixed point-source and key-beam connections through closed parallel protective panes use IOR
1.52 and RGB absorption (0.080, 0.035, 0.020) m^-1. Their existing straight-connection helper
measures actual entry/exit interfaces and applies Fresnel twice and Beer over the straight inside
segment. It still omits lateral displacement and cannot solve arbitrary finite source connections
through a prism/lens/mirror. Those cases remain explicit HDR magenta diagnostics. Environment
radiance now has a separate direction-returning source optical path, described below. A capture
showing any remaining diagnostic pixels fails optical acceptance.
The alpha query uses level-0 nearest/repeat source alpha with the existing 0.5 threshold; filtered
mipmap/footprint parity remains a numerical material limitation.

## Deliberately incomplete transport

Exact direct visibility is not complete global illumination. Opaque ground now blocks a
connection rather than allowing the probe's averaged ground color to shine through it. The
portable path still lacks the actual surface-to-surface radiance at that hit, including important
interior bounce light. A dark cabin is not evidence that this missing indirect transport has been
solved. Environment radiance itself is still sampled by the existing 32-direction probe and raster
media use bounded single-scatter approximations. The shared 32-source admission budget still
omits lower-ranked source contributions when over capacity. These remain explicit approximation
limits and must not be described as a fully physically based final renderer.

## Validation at the integration checkpoint

- Final composed portable surface, smoke and particle WGSL validates without hardware ray-query
  capability: 16/16 `tests/stylized_raster.rs` tests pass, including updated binding/ABI checks.
- Main release binary built successfully after the final bend, emitter-endpoint and actor GPU
  buffer copy-on-write fixes. Main test binary source visibility16/16, cabin medium3/3, running
  gear4/4 and balanced reproduction1/1 passed. Apollo's separate GPU packed-payload parity
  test checked512 rays with bending/private pose uploads.
- Cabin source extraction/pose/visor/grille regressions pass in the module's focused three-test
  suite; provenance receipt: `artifacts/runtime/cabin_medium_source_columns_20261001.json`.
- Native render/clip acceptance is pending; compile and Naga success do not prove appearance,
  point-source self-occlusion behavior, optical diagnostic share or usable frame time.

### Native checkpoint failure

The first actual RTX 4090 Vulkan preview failed before emitting frames: NVIDIA NVVM shader
compilation returned error3 and the resulting device loss made the subsequent shadow-map
texture invalid. The isolated portable BVH compute test also reproduces this on driver615.71.09,
so this is not a valid native render checkpoint yet. Prior512-ray GPU evidence was llvmpipe
OpenGL; it establishes payload behavior on that implementation, not NVIDIA compatibility.
Logs: `artifacts/runtime/blizzard_inside_exact_visibility_probe_v8_20261001.log` and
`artifacts/runtime/scene_visibility_nvidia_vulkan_parity_20261001.log`. The native shader
compiler issue must be fixed and retested before gameplay acceptance.

### Driver fix and bounded numerical acceptance

The NVIDIA compiler trigger was the alpha-texture validation expression `0xffffffffu / height`.
An equivalent division-free 16-bit-limb multiplication-overflow check preserves the validation and
source-alpha behavior. Its CPU equivalence regression covers 10,049 input pairs against u64
multiplication. The complete production surface fragment then compiled on RTX 4090/Vulkan in
3.35 s with no validation error (`portable_fragment_nvidia_full_20261001.log`).

The first native compute parity run exposed a grazing ray 26.08 nm from a bent source-triangle
edge: separate CPU operations and GPU contraction classified its boundary differently. General
parity now uses 512 strict points away from triangle boundaries. Eight additional rays at ±1 µm
and ±10 µm around the original edge require strict hit/material/transmission agreement. The two
original center cases are separately checked against a double-precision projection of the actual
f32 source edge and must lie within 1 µm; either boundary classification is accepted there, with
matching physical hit distance/material/transmission if hit. This is a specific fixture's measured
numerical boundary, not a global error bound or a relaxation for ordinary rays.

The NVIDIA test passes all 512 strict general rays, eight strict edge neighbours and two bounded
centers. It also verifies private pose uploads and real bending. In the optimized shader each
iteration exercises 41 early opaque blocks and 88 ordered optical candidates. Receipt:
`artifacts/runtime/scene_visibility_nvidia_anyhit_branches_parity_20261001.log`.

### Source-preserving performance changes

A spare TLAS node word stores whether it contains any dielectric/unresolved descendants. A ray
that misses those flagged supports can terminate on its first actual opaque or alpha-covered
triangle; it does not need a nearest opaque hit. Mixed leaves check the actual optical instance's
conservative root bounds so a neighbouring large hull cannot force every ray into the optical
solver. Registered emitting-surface endpoints keep the ordered solver. Optical candidates keep
all interface ordering, Fresnel/Beer and unsupported-status behavior. Missing optimization
metadata also keeps ordered traversal. No caster or light sample is discarded by this change.

Exactly zero radiance sources now skip their zero-contribution geometry queries. Quadrature
normalization weights are still accumulated before that skip, so this cannot brighten remaining
samples. The parent is separately sizing each optical raster target from the real projected
aperture; that changes sampling resolution, not geometry or lighting.

Native optical/video acceptance remains incomplete: the first actual scene frame showed magenta
optical diagnostics and per-port full-resolution work made capture impractically slow. That
attempt is diagnostic evidence only. CPU source-interface diagnosis and the optimized native
capture must pass before a video is described as the completed renderer.


### Main-view diagnosis and directional environment transport

The earlier CPU report inspected only prepared exterior port captures. Its396 clear/blocked
connections did not cover the magenta main-image patches. The expanded report samples103 main
pixels (a12x8 grid plus seven normalized v9 patch centers), separately tracing opaque-surface and
fog-centroid receivers. It showed valid closed WINDOW entry/exit pairs followed by actual finite
RELAY_LENS source faces, plus one front-to-side pane exit. No material was relabeled to hide this.
The matching native v11 frame retains those diagnostics. Files:
`captures/game_blizzard_main_source_review_v11_20261001_source_visibility.json` and
`captures/game_blizzard_main_frame_v11_20261001_frame.png`. Timestamp evidence gives9.56s for
that whole one-frame process;1.82s from pre-frame CPU report to saved1920x1080 PNG, including
640x360 rendering/readback/encoding. The supplied `KESSEL_INTERNAL_RES=960` was rejected because
the parser requires `WIDTHxHEIGHT`; the CPU receipt records the actual640x360 fallback. There
was no convergence loop or GPU pass timing, so this
is not a gameplay frame-rate claim.

Environment paths now follow exact source triangles through Snell refraction, total internal
reflection, actual mirror planes, and finite ideal relay lenses. The returned exit direction selects
the outgoing environment cell; the receiver's integration/phase direction and weight stay fixed.
Parallel exits retain the original lane exactly. Bent exits use the nearest of the existing32
cone-mean probe cells, a coarse angular approximation. A dark original lane cannot skip an optic
which could redirect toward a lit lane. Mirrors use the existing declared0.90 coating factor and
ideal lenses the declared0.96 factor; neither is a measured historical material claim. The finite
lens focal length/center come from the same reconstructed source prescription as the RT path.

The estimator follows the transmitted branch at ordinary Fresnel interfaces, and the reflected
branch at total internal reflection. It does not sum Fresnel split multipaths, dispersion, lens
aberrations, nested glass, a receiver starting inside a dielectric, or diffuse surface bounce.
Background atmosphere remains the existing camera probe approximation; the new path's Beer
length is measured only inside actual glass. Fixed point/key-beam endpoint transport remains
separate and retains unsupported diagnostics. Up to64 optical events are admitted; unfinished
paths are unresolved, never silently clear/black.

Native validation caught an internal shared-diagonal crack in the projected triangle test:
NVIDIA FMA contraction could make both adjacent faces reject a ray. Canonical endpoint ordering
now makes shared-edge signs exactly antisymmetric. The strict GPU prism-face regression includes
that diagonal, separate from the earlier outer-edge numerical-boundary test.

Validation:21 CPU tests pass, including analytic parallel Beer/Snell, prism exit, TIR, lens
reciprocity, mirror direction, and an opaque blocker after a lens.16 composed shader tests pass.
RTX4090/Vulkan passes522 geometry rays (520 strict plus two explicitly bounded outer-edge
centers) and448 strict optical paths:320 clear,64 real opaque terminations,64 open-pane failures.
All448 also assert the prism's internal shared-face coverage and distance. Evidence:
`artifacts/runtime/environment_and_geometry_nvidia_parity_20261001.log`.


### Bounded rim-continuation correction and current acceptance

The v13 winter frame contains 3,042 display pixels passing the diagnostic mask
(R > 220, G < 70, B > 220), versus 64,350 in v11. Both were actually rendered at
640x360 and saved at1920x1080. The v13 pre-report-to-PNG interval was1.67 seconds;
this includes readback and PNG encoding, is one frame, and is not a gameplay FPS result.
The winter terrain material also changed between those frames, so the comparison is not an
isolated overall image-quality benchmark. Magenta remains a failed visual acceptance signal.

The v15 CPU report samples every distinct native pixel covered by that display mask:
338 such pixels, plus103 general/reference samples. At native pixel181.5,295.5,
environment direction18 enters the real WINDOW46 almost at its rim. The old0.1mm secondary
ray cutoff skipped a source side interface roughly26 micrometres from entry, making a later
mirror incorrectly appear inside the glass. The new continuation excludes only the previous
source triangle and its exactly edge-sharing, equally oriented coplanar source neighbor.
Zero-length coincident events are not separate transport segments. There is no metric slab
that removes nearby side faces, panes or armor. Beer still starts at the geometric interface.
A standalone regression loads the current Tiger optical assets, applies the actual captured
model matrix and receiver ray, and now observes the side total-internal-reflection event.

CPU validation:23 portable source-visibility tests pass, including640 optical fixture rays,
40-micrometre panes, a side event closer than0.1mm, and real armor2 micrometres behind glass.
The exact Tiger rim asset regression passes, and16 composed raster shader tests pass. The last
NVIDIA run matched CPU on every640 optical ray and passed522 ordinary geometry rays, but one
shared-diagonal double event still failed the aggregate expectation. The subsequent coplanar
neighbor correction passes CPU checks; its final NVIDIA rerun and native frame are PENDING
because the user's interactive play session has priority. Do not promote this checkpoint to a
GPU-validated fix until that repeat passes. Logs:

- `artifacts/runtime/optical_secondary_source_cpu_tests_20261001.log`
- `artifacts/runtime/actual_cupola_rim_cpu_regression_20261001.log`
- `artifacts/runtime/optical_secondary_composed_tests_20261001.log`
- `artifacts/runtime/optical_secondary_source_nvidia_parity_20261001.log` (pre-final correction;
  records the failed aggregate, not a passing receipt)

A separate general-grid fog receiver still reaches real WINDOW45 then MIRROR44 while connecting
to point source2. Finite reflected/refracted point and key-beam boundary-value transport is not
implemented. That path remains explicitly unresolved; environment-direction transport does not
solve it. No general caustic solver was added during this bounded repair.

### Production and performance handoff

Keep the playable mesh renderer as the primary game path. Clouds, fog, smoke and dense fire can
use numerical volume integration while opaque vehicles, people and buildings retain their real
meshes. Shared source geometry must continue to determine shadowing and containment. The portable
full-scene BVH is a correctness/reference and potential high-quality path until measured GPU
pass budgets establish a practical setting; this work does not justify silently making its
current query count the shipping default.

The next bounded measurement should use an actual fixed winter battle and the user's800x450
internal target, with warmed GPU timestamps for primary surfaces, optical subviews, volume
integration and presentation reported separately. Then reduce samples/update cadence or improve
acceleration while checking identical scene emitters/occluders. Hardware ray queries are a
potential acceleration path only after their existing source exclusions, alpha handling and
animated poses match the portable registry. Missing diffuse bounce, split Fresnel paths and
finite specular endpoint connections remain named quality limits, not claims of complete GI.
