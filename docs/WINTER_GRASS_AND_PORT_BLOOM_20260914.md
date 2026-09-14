# Winter grass and daylight aperture bloom

The shader worktree adds fuller dry winter grass and a daylight halo from visible tank openings. Fixed exposure, source materials and the pixel grid are retained.

Grass uses 18/8/5 blades in near/middle/far clumps. Near stems and selected middle stems have bent geometry, with short and tall straw mixed around each root. Nearby root density increases while the world-fixed winter patch noise preserves snow gaps. All three layers remain instanced, opaque and free of strand physics. Winter allocations are capped at 1,536/8,192/12,288 roots and 931,840 triangles, below the existing summer bound of 983,040. Actual submitted work depends on the view.

Outdoor bloom uses the existing four passes and two half-resolution targets. RGB retains the compact lamp halo; alpha carries a broader near-neutral daylight halo. The bright pass samples visible depth and optical-pane identity while the physical camera is inside. A distant visible scene or sky can seed the halo; nearby closed hatch geometry blocks it. Optical relays bypass the distance classification. The extra broad blur and depth work are skipped for exterior cameras. The composite preserves the source image inside apertures and optic glass, restricting the new broad halo to their frame and cabin surroundings. This prevents a contrast-reducing white veil in focused periscope views. Interaction outlines render after bloom.

This is lens bloom, not a volumetric shaft or surface illumination. Openings are approximated from a smooth 4–7 metre depth threshold, so very close exterior scenery may contribute less. Radiance controls the effect at night; no white-light constant, automatic exposure or temporal accumulation is added.

Targeted CPU checks passed: 15 grass tests and 7 shader/ABI tests. A release build completed successfully. Before/after captures use the same 640×360 scene resolution, 1920×1080 output and declared gameplay fixtures. Their profiling includes render submission, GPU completion and full-size readback; it is not native interactive FPS. The user's existing game remains a separate running process during capture, so timing comparisons also share GPU load.

Fixtures: `catalog/shader_reviews/daylight_port_bloom.v1.json`, `daylight_optics_bloom.v1.json`, `night_port_bloom.v1.json`, `daylight_port_act1_bloom.v1.json`, and the existing `act1_winter_variety_near.v1.json`. Media belongs in `captures/astra/`; build and checks in `artifacts/visual-feedback-20260914/`.

## Reviewed result

- All 27 declared gameplay outcome checks passed across the five review scenarios. After protecting aperture image cores, the periscope, actual Act 1 driver hatch, and night scenarios were rerun: all 19 checks passed again.
- Open hatch edges bloom into the cabin; closing the lid removes the roof-opening halo. The still-visible driver periscope continues to glow. Night opening glow remains dim.
- Periscope frame glow is visible from the cabin. Focused sight images retain their original contrast: a fixed sky crop in the before/final focus renders has the same mean RGB, 219.67/221.50/222.02. This corrected an extra white veil caught during intermediate QA.
- Near winter grass has visibly fuller, bent clumps with varied heights and scattered snow gaps. Far coverage persists through the snowy haze.
- Reviewed final media: `captures/astra/daylight_port_act1_final_20260914.mp4`, `daylight_optics_final_20260914.mp4`, `night_port_final_20260914.mp4`, and `winter_grass_after_20260914.mp4`. Matching `_manifest.json` files contain state and profiling receipts.

The initial identical-fixture comparison measured median render-plus-readback times of 10.42 to 11.15 ms for the driver hatch, 8.07 to 8.90 ms for the grass walk, and 12.38 to 13.14 ms for periscopes. With the final core-preservation correction, the periscope median was 12.97 ms (p95 15.82 ms). These are shared-GPU capture observations, not an isolated GPU benchmark or a promise of interactive FPS. Every measured capture reported ray queries disabled and four bloom passes. No additional bloom targets or passes were introduced.

Final release SHA-256: `3bde8d0bbcba38e5f55b5bbb4bf4b7a18f136fc108a0cf615aede41cb4a2575c`. Build log: `artifacts/visual-feedback-20260914/build-final.log`. The existing interactive process was preserved on its prior executable; these changes are ready on the next launch.

After closing the current game, launch on the AMOLED with:

```sh
/home/alex/panzer/shader-graphics/tools/launch_shader_amoled.sh
```
