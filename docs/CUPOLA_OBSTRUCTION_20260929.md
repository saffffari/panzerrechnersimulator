# Cupola lower-field obstruction — 29 September 2026

The owner reported that one vision block was covered across its lower half. The station,
direction and whether it followed a hit were not confirmed. The investigation reproduced a
matching obstruction in the **healthy front-left commander block**, index 1 / `Turret_0139`,
without combat, blood, foam or damage injection. This does not identify the owner's exact pane.

## Source geometry and renderer defect

All ten blocked samples in a 5×5 held-field optical trace hit the retained source roof cap
`Turret_0116`, triangles 299–308, about 0.169–0.193 m beyond the optical exit. Fifteen samples,
including the centre and upper field, clear it. The cap is a fixed root node in the source
turret, about 0.331 m wide and 0.031 m high, above `Turret_0115`. It is not a misplaced animated
lid or a generated damage mask. The asset does not encode its historical component name;
these observations do not establish historically correct cap or optic placement.

The old raster mapping differed from the source optical path. It centred a virtual observer
on the slanted entry axis and rotated that axis toward the horizon. It consequently discarded
the actual held eye's upward direction. Replaying that mapping against the same source cap
cleared only ten of twenty-five sampled rays and blocked the centre row. The baseline film
visibly reproduces the cap across slightly more than half the field.

The seven commander registrations now explicitly preserve incident ray direction and use the
actual observer pupil. Their four-lens/two-mirror source relay already preserves those
directions in the independent optical tests. The normal seated view changes consistently with
its actual eye position; it is no longer forced to centre on the horizon. Loader, driver,
radio and gunner registrations retain their existing mapping. No cap, glass, casing, camera
pose, focus timing or field of view was altered.

The raster exterior camera still uses one objective-centred pupil 8 mm beyond the glass.
It does not reproduce every ray's lateral exit position, finite internal aperture rejection
or glass/lens aberrations. Nearby silhouettes may therefore differ from the source path.
This is an angular-mapping correction within a declared raster approximation, not a full
optical solver or historical prescription.

## Repair distinction

Cupola damage uses whole-block `Clear`, `Opaque` or `Wrecked` state; it does not have a
lower-half damage flag. Deposited blood or suppressant can separately obscure part of an
optical surface. The clean reproduction here excludes those causes. A hammer or replacement
glass cannot remove an exterior roof cap.

The audit also found separate existing damage limitations: the raster block flag suppresses
the relay image without an obvious dedicated opaque-pane fallback, and the flag is not
propagated into the ray-traced optical transport. They were not used to explain or repair this
healthy lower-field obstruction.

## Evidence

- Baseline source/build: `90393a7`; binary and SHA-256 retained under
  `artifacts/immersion-20260929/panzerrechnersystem-vision-baseline-90393a7` and
  `vision-baseline.sha256`.
- Independent source trace: `artifacts/immersion-20260929/cupola-block1-source-rays.json`.
- Prior raster trace: `artifacts/immersion-20260929/cupola-block1-raster-source-rays.json`.
- [Baseline focused view](../captures/astra/cupola_lower_field_20260929_baseline_leftfront.png)
  and [film](../captures/astra/cupola_lower_field_20260929_baseline.mp4), with the unchanged
  front block as comparison. Six state checks pass; those checks alone do not prove the image.
- Focused comparison: `catalog/shader_reviews/cupola_lower_field_20260929.v1.json`.
- Seven-port ordinary/focus/release review:
  `catalog/shader_reviews/cupola_seven_fields_20260929.v1.json`. Read-only telemetry records
  acquired index, focus phase and all block damage states. It cannot grant focus or clear damage.

## Final verification

- Release builds. Binary SHA-256:
  `d705311b440cb5e3678999b42124b47d554e71f3b551aa1362eb1099043edbb1`.
- Unit suite: **1,404 passed, 0 failed, 51 ignored**. Ten focused raster optics tests and
  the independent source optical field/reciprocity/obstruction regression pass.
- The [corrected focused film](../captures/astra/cupola_lower_field_20260929_corrected.mp4)
  passes six state checks. Its [held frame](../captures/astra/cupola_lower_field_20260929_corrected_leftfront.png)
  shows a clear centre with the cap retained across roughly the lower third. The baseline
  cap crossed the centre and covered slightly more than half the image.
- The [seven-port film](../captures/astra/cupola_seven_fields_20260929_corrected.mp4) passes
  **37/37 checks** and all required actions: each actual index acquires full focus and releases,
  with all seven block states `Clear`. All seven held centres were visually inspected and
  show upright exterior views. Ordinary/released pairs for blocks 0, 1, 3 and 6 return to
  matching framing. Ordinary block 1 still sees the roof cap at its downward eye bearing.
  The sky-heavy block 3 agrees with recorded tank roll; the corrected view follows the tank.
- The identical focused fixture passes on Laptop settings at 576×324; its held image retains
  the same centre clearance and lower cap. The initial 320×180 review was refused by the
  recorder's existing 640×360/576×324 export guard, before recording. The failed log is retained;
  no guard was relaxed. This is a preset comparison, not a default-resolution laptop benchmark.
- The RTX 4090 physical High scene frame compiles the ray-query/combine/volume path and renders:
  `captures/astra/cupola_mapping_20260929_physical_high.png`. It was visually inspected; its
  pre-existing bright/noisy cabin is not accepted as the new atmosphere direction.
- Full workspace tests still stop at the pre-existing missing `crate::resource` integration
  import; the physical-truth contract remains 17 passes and the existing `glass_pipe` substring
  failure. Full format checking reports existing repository style differences. Diff whitespace
  checks and the review builder's Python compilation pass.
- The [local comparison page](../captures/astra/immersion_review_20260929.html#cupola) was
  refreshed and visually checked in Chrome; both five-second media files load correctly.

These checks do not claim a native mouse playtest of this rebuilt binary, historical validation
of the asset, or identification of a different vision block the owner may have meant.
