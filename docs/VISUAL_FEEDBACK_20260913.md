# Visual feedback — September 13, 2026

This change follows the supplied snow/P08 screenshot. Work is confined to the isolated shader
checkout. The existing live process has not been restarted; recorded GPU reviews use the rebuilt
binary in short separate headless runs.

## Implemented and GPU reviewed

1. **Snow bands:** the sun shadow filter compared sloped receiver texels at a single depth,
   making the terrain shadow itself. Plane-correct reference depths remove the repeated curves
   and lines while keeping caster/contact shadows. The material and terrain geometry are unchanged.
2. **Sight notch:** the real rear V-notch is present in the source mesh. At 640x360 and 45-degree
   aim FOV, the old 52 cm origin distance left zero native raster samples visible around the
   front post. A 40 cm hold retains two gap pixels, one on each side. Source geometry, animation,
   FOV and the sight-line height remain unchanged; no overlay crosshair was added.
3. **Dithering:** a fixed monochrome 4x4 threshold covers less than half a linear 8-bit output
   step before rounding. It softens colour contours without frame noise or stronger posterization.
   The existing unused uniform lane at byte32 now controls this shader-only operation. Uniform
   size56 and camera exposure offset44 remain unchanged; the physical renderer sets it to zero.

## Evidence

- `captures/astra/act1_p08_snow_before_ne_aim_20260913.png`: original binary, ordinary aim input.
- `captures/astra/act1_p08_snow_fixed_ne_aim_20260913.png`: shadow and sight fixes.
- `captures/astra/act1_p08_snow_dither_ne_aim_20260913.png`: final rendering with subtle dither.
- Adjacent two-second MP4s and manifests are actual rendered footage at the same declared camera.
- `captures/astra/ground_receiver_plane_fix_20260913.md`: independent shadow diagnosis and review.

These captures show that the snow bands are gone, contact shadows remain and the sight gaps
are visible. They do not establish interactive frame-rate equivalence, every weather/lighting
condition, or acceptance of the still-running old session.

## Verification

- Release build succeeded after the final shader changes.
- Three source P08 tests passed, including native-pixel occlusion against the actual source mesh
  and preserved source triangles/magazine/toggle joints. The canonical source asset root was set;
  source checks were exercised, not skipped.
- Four raster shader/ABI tests, two camera/bloom tests, three display quantization tests and
  five temporal contract tests passed. Actual GPU pipeline creation and the short captures passed.
- Shadow regression:12 sloped-plane/caster cases x4096 subtexel phases passed. Corrected visibility
  remains1.0 for a clear plane, and a nearer caster still casts a shadow.
- Ten of11 broader physical-truth tests passed. The pre-existing
  `aperture_and_screen_overlay_proxies_cannot_reappear` test flags the string `glass_pipe` inside
  existing optional `raster_glass_pipe` and `raster_glow_glass_pipe` names. The same fields are in
  the saved renderer source before these changes; this task neither introduced nor renamed them.
  The failing legacy contract is recorded, not represented as a passing check.
- Scoped whitespace checks passed. Build/test logs and pre-edit source snapshots are under
  `artifacts/visual-feedback-20260913/`.

## Environment revision

The next `tools/launch_shader_playtest.sh --playtest` opens
`assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json`.
An explicit `KESSEL_PLAYTEST_MANIFEST` still takes precedence. The original map can be selected
with `KESSEL_PLAYTEST_MANIFEST=/home/alex/panzer/tank_simulator/assets/worldgen/act1/manifest.v1.json`.

The additive revision has55 new objects in five deliberately different groups: near bare snags,
broken timber boundary/cache, ruined field shelter, east field edge and west exit remnant. Within
150 m of spawn, authored placement count goes8→63 and unique source meshes3→17. The separate runtime
story layer is unchanged and excluded from those counts. All13,014 original placements, terrain,
navigation routes, markers and wires are preserved. The generator and exact clearance/provenance
receipt live beside the revision; regenerate with `python3 tools/worldgen/build_winter_approach_revision.py`.

Actual RTX4090/Vulkan reviews loaded the revision successfully. Both wide walking/aiming and close
boundary/cache clips passed their three outcome checks. The new tree, fallen wood, fence and cache
silhouettes are visible and grounded in the inspected views; no intersecting runtime grove was
visible in those views. Ground shadow bands remain absent. Captures:

- `captures/astra/act1_winter_variety_20260913.mp4` and its manifest.
- `captures/astra/act1_winter_variety_walk_20260913.png` (wide ordinary walking view).
- `captures/astra/act1_winter_variety_near_20260913.mp4` and its manifest.
- `captures/astra/act1_winter_variety_near_walk_20260913.png` (close boundary/cache view).

This is the first composition pass around the starting area, not a rebuilt entire city or an
exhaustive collision/performance review. The running old game was preserved; the new binary and
revision take effect on the next shader Playtest launch. Preflight reports no errors and only
the existing optional German-infantry checkpoint warning. Final source hashes and verification
summary are in `artifacts/visual-feedback-20260913/final-verification.json`.
