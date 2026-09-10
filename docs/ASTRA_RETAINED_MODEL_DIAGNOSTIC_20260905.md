# Retained Tiger model diagnostic — prepared for v17

This work is **outside the frozen v16 build**. Source and JSON parse checks have been performed;
compiled tests, GPU validation, a recorded video, and visual approval remain pending. It must not
be described as a completed visual fix yet.

## What the older circles represented

`damage_readout::draw_cutaway` projected spherical component hit volumes, ammunition boxes and
anatomical region boxes/stick outlines. `Renderer::set_xray` is a compatibility no-op. The old
`astra_capture::damage_appendix` is uninvoked. Neither an old filename containing `xray` nor a
passing damage calculation proves that a full interior cutaway was rendered.

## New explicit diagnostic route

`src/damage_model_view.rs` owns a dedicated 640 × 360 raster target, depth target and bounded
512-draw instance buffer. It borrows shared GPU buffer/material handles from the actual retained
Tiger hull, turret, gun, articulated source hatch/sight parts and interior pieces. No second full
Renderer, second source geometry upload, CPU rasterizer or model-image readback/reupload is used.
Only final video export reads pixels back. The image stays nearest-sampled and exports at integer
3×, 1920 × 1080. It does not render the full world underneath the diagnostic.

The presentation removes the roof above hull Y=2.25 m and the camera-facing upper half above
Y=1.35 m, using fragment section planes on the actual transformed triangles. No new caps are
fabricated. These are expressly authored review cuts, not physical transparent armor. The source
gun/sight status can tint its proven mesh partition; engine/transmission/radio and other unmatched
component identities remain status labels. Their spherical damage proxies are never drawn.

An explicit second depth layer draws the retained eleven-page skeleton and live cartridges as an
X-ray overlay. They occlude one another, but remain visible through the sectioned tank. Skeleton
pages follow current articulated poses and retained bone attachments. The permanent person identity
selects the source pool; a survivor changing station follows the live roster's actual post. Removed
bones without a retained attachment are absent. Empty/unoccupied or dismounted roster entries do
not acquire invented bodies.

Cartridges are the project-authored 8.8 cm AP/HE meshes already used in the game, positioned from
`live_ammunition_in_frame`: rack, loaded chamber, loose rounds and the currently carried round.
Spent/taken inventory entries remain absent, and stable IDs retain their ordinary inventory meaning.
The old procedural breech proxy box is omitted because the real source gun/breech is already leased.

The diffuse view light is bounded and deterministic solely for diagnostic readability. It does not
claim to reproduce gameplay path tracing, cabin lighting, real X-ray attenuation or material damage
segmentation. The approximate skeleton fit retains its source validation limitation (maximum reviewed
landmark error 0.171724 m); this is not medical registration. The original pixel anatomy atlas can
form the final two seconds, with the existing eleven-region records and explicitly reference-only
organ colors. No organ-health model is invented.

## Prepared recording

Scenario:
`assets/review/first_person/diagnostic_retained_tiger_85mm_v17.json`

The recipe reuses the previous 85 mm BR-365 / 300 m / left-side seed 42871 contact unchanged.
There is no damage-outcome search or forced organ/module result. It stages four actual mortal couch
identities through `TankInput` before recording; the same input path keeps them registered and strips
KIA authority during the ordinary simulation update. The stationary diagnostic crew fixture only
accepts explicit contact/hatch actions, avoiding a claim that unrepresented controller channels were
exercised. The only new optical option is `diagnostic_view: retained_model`.

Root-owned build/capture command, after coordinated source and shader gates:

```sh
KESSEL_INTERNAL_RES=640x360 <frozen-executable> mechanic-review \
  captures/astra/diagnostic_retained_tiger_85mm_v17_20260905 \
  assets/review/first_person/diagnostic_retained_tiger_85mm_v17.json
```

The receipt uses `kessel.retained-model-diagnostic-record.v1`, `first_person: false`, the explicit
scope, actual draw count, live inventory count, actual four-body initial occupancy, all ordinary
state samples and the original gameplay audio sink metadata. It does not pass this orbit off as a
first-person gameplay clip. Normal review recipes remain on the original first-person route and
retain normal blackout/replay behavior. The diagnostic can inspect retained state after a death;
it does not alter the underlying fatal state.

Declared simulation checks: exactly four initial occupied living crew, nonempty initial live
ammunition, exactly one armor contact by 3 s and at the end, and the actual last caliber in the
84.9–85.5 mm range. These do not assert an injury result merely because the view is enabled.

New CPU gates cover the actual WGSL parse/validation and host/shader record sizes, source bone
pivot/scale, hull-relative precision, explicit four-peer identity binding, unchanged default solo
input, and the disclosed diagnostic recipe. Existing region/identity tests remain applicable.

Still required: compile and shader gates, a short actual capture to verify full model framing and
legible pixel labels, source bone visibility/pose review, current cartridge count correspondence,
actual impact state changes, audible sync, and art approval. Tests alone cannot validate the image.
