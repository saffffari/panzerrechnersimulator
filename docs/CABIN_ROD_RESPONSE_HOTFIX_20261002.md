# Cabin rod-response hotfix, 2026-10-02

## Evidence and scope

The controlled settled-cabin candidate lost warm dial/lamp color and became much brighter than the baseline. Both use the same 800×450 internal resolution and fixed Bayer phase; the review PNGs are nearest-upscaled 3×. Turret mean RGB display code increased from 11.25 to 45.05 and the 90th-percentile mean-channel code from 24 to 181. Mean channel spread fell from 1.025 to 0.185. Cone exposure decreased from 4671.58 to 4156.23, so that scalar alone did not describe the regression. A conservative linear-8-bit quantization envelope could not explain 401,682 of 1,080,000 native RGB channel increases.

The actual GPU receipt `captures/hotfix_cabin_eye_diagnostic_20261002_turret_photometry.json` records:

- metered photopic field: 0.0000553732 cd/m²; mesopic coefficient: 0;
- cone exposure: 4156.22754; rod exposure: 7.86095953;
- rod inverse saturation: 0.0284730047; rod adaptation: 0.0001148153 scotopic cd/m²;
- zero flash energy and afterimage; pupil diameter: 7.85048 mm;
- HDR 90th percentile: 0.0313152 photopic cd/m²; selected bright-region mean: 43.0075 photopic cd/m².

The storage layout is valid: lane 21 is within the 32-lane buffer and does not overlap the cone/rod uniform copies. The sRGB intermediate is sampled back into linear light and encoded by the final output target. Neither was changed by this correction.

The prior steady-aperture flash-onset fix remains: a static window must not repeatedly charge the flash-blindness state. Removing that false adaptation exposed an existing display-response problem. The globally dark-adapted rod gain was applied to every pixel, including locally bright colored lamps. Its neutral additive signal approached **276.08465** before tone mapping. This overwhelmed the unchanged chromatic cone signal.

## Bounded reproduction correction

For scotopic stimulus Ys, existing rod gain G and inverse saturation S:

```
q = max(Ys, 0) / (1 + max(Ys, 0) * S)
K = (DISPLAY_KEY / DISPLAY_ADAPTATION) * TVI_cone(DISPLAY_ADAPTATION)
R_global = G * q
R_local_raw = K * q / TVI_rod(max(q, METER_MIN))
R_local = max(R_local_raw, K * min(q, 10^-3.94) / 10^-2.86)
R = min(R_global, R_local)
```

`R` remains an achromatic display-input addition to the existing cone channel. The local bound uses the existing TVI curve and display calibration; its bright-stimulus plateau is **0.0248469168**, derived from those constants rather than a new adjustable gain. The `max` closes a 16-ppm downward seam caused by rounded coefficients at the TVI absolute-threshold knee. It leaves the eye's adaptation TVI unchanged.

This is a **display reproduction approximation**, not newly measured physiology or instantaneous local physiological adaptation. It does not alter physical source powers, surfaces, light transport, occlusion, exposure history, pupil state, film shoulder, dither scale or display encoding.

One CPU helper in `src/observer.rs` and one generated WGSL helper supply all current `eye_response` consumers: `crush.wgsl`, `stylized_particles.wgsl` and `pixel_particles.wgsl`. Raster optic particles include the shared stylized-particle functions. Bloom thresholds use cone exposure and contain no separate rod response, so bloom was not modified. The generated shader uses exact algebraic branches for deep-dark and bright stimuli, avoiding logarithms/powers outside the middle TVI interval.

## Validation boundary

Four added Rust tests cover black, photopic bypass, unchanged dark samples where the new bound is inactive, the captured cabin eye state with a synthetic warm spectrum, monotonic rod response including both TVI knees, and shared valid WGSL before any display encoding. Root coordinates their build/run and visual acceptance.

Independent Python evaluation of the same formula covered **576,216** ordered stimulus samples across six gains and four saturation values, including dense knee neighborhoods and adjacent representable knee values. The worst negative adjacent difference was roundoff of 2.78e-17. Receipt: `artifacts/runtime/local_rod_response_numeric_20261002.json`. Its sample response table is a synthetic sweep in scotopic units, not a claim that the captured photopic percentiles equal those pixels' scotopic values.

The existing uniform Moon case retains its rod response; some extremely dark samples decrease modestly because the bound also limits the old pupil-normalization gain above the absolute-threshold response. Dark-night black/visibility gates remain tests to run; no blanket claim that every low pixel is bit-identical is made.

The rod contribution is nondecreasing before tone mapping. This does **not** establish general final-display luminance monotonicity for every saturated RGB spectrum: the existing peak-channel shoulder can produce small saturated-blue luminance reversals as chroma reappears. That broader tone-map limitation is outside this bounded hotfix. Existing scalar `observer::aces` tests are not evidence for the newer colored peak/log-shoulder behavior.

A successful CPU build is not visual acceptance. The required next step is root's same-pose cabin A/B plus dark exterior/fire review, followed by the native playtest. No GPU run was performed by this sub-agent.

## Final compiled and GPU fixture validation

The pending compiled-test and fixed-fixture gates above are now complete. Existing release test binaries were verified newer than `src/observer.rs`, their nonzero test listings were checked, and all requested suites passed: observer **12/12**, stylized raster **23/23**, display quantization **5/5**, pixel particle rendering **17/17**. This is **57 test executions / 33 distinct test names**, with zero failures or ignored tests; observer tests are included in two integration binaries. Exact commands, binary hashes, listings and logs are in `artifacts/runtime/hotfix_final_observer_render_tests_20261002.json`.

Root rendered and visually inspected the final day/night turret and driver fixtures. Root reported the visual gate **passed**: warm colored dials/lights remain visible and the cabin body is dark, replacing the stark achromatic regression. These are fixed-angle GPU fixtures, not a claim that native interactive playtesting is finished.

The matched pre-cap diagnostic versus final **day** fixtures preserve all **32 eye-state lanes exactly**, and every recorded HDR photometry value (mean, log mean, median, percentiles and 8×4 grid) is exactly equal, for both turret and driver. The changed output is therefore consistent with the bounded display-response correction, without an exposure-state or measured scene-light change. This verifies identical recorded HDR summaries; raw HDR textures were not dumped, so byte-identical full textures are not claimed.

| Native-pixel display metric | Turret before | Turret final | Driver before | Driver final |
| --- | ---: | ---: | ---: | ---: |
| Mean RGB code | 45.0474 | 12.6754 | 5.77684 | 4.33812 |
| Mean max-minus-min channel code | 0.184703 | 1.74255 | 0.0489917 | 0.205492 |
| Pixels changed out of 360,000 | — | 137,355 | — | 9,055 |

All six analyzed PNGs (two before-day, two final-day, two final-night) retain exact nearest-neighbor 3× blocks of the **800×450** image. The night fixtures are final inspections only; there is no matched pre-cap night pixel comparison. Metrics are RGB code-space statistics, not physical luminance measurements.

Compact comparison receipt with PNG/sidecar hashes, exact pixel metrics, equality checks and visual-acceptance boundary: `artifacts/runtime/hotfix_cabin_final_display_comparison_20261002.json`. Capture families: `captures/hotfix_cabin_eye_diagnostic_20261002_*`, `captures/hotfix_cabin_final_day_20261002_*`, and `captures/hotfix_cabin_final_night_20261002_*`.

The remaining saturated-blue peak-shoulder limitation described above still applies. No further source edits or GPU/build runs were made during this final receipt update.
