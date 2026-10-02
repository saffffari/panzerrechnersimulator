# Smoke continuity checkpoint — 2026-10-01

The approved flame field, palette, source power and trajectories are unchanged by this checkpoint.
Smoke remains participating matter with the existing geometry visibility and source lighting.

The prior sustained exterior plume used three independent spherical lobes at nominal ages
0.45, 1.80 and 3.15 seconds. Strong weather wind could place even the youngest lobe beyond its
radius from the emitting patch. Each lobe now occupies an affine ellipsoid along the path from
its own actual fire patch to that lobe's existing advected center. Unrelated patches are never
joined. Its density is divided by the exact affine volume Jacobian, including the textured
source-space density, so stretching does not add optical mass. The same density/support is used
for primary and secondary attenuation. Existing transient muzzle spheres remain spheres.
This is a prescribed steady carrier, not a fluid solver, chemical soot production model, or
complete turbulence/advection solution. Eight midpoint volume samples remain a numerical limit;
stretched support increases their physical spacing and still needs native visual inspection.

Local plume opacity is presented with adjacent 0.25-spaced coverage levels on the actual native
pixel grid (scaled down when smooth cabin haze shares the interval). A fixed per-pixel threshold
has no frame/time input. Opacity and premultiplied scattered radiance use the same sample, so
expected background transmittance and source radiance match the original integration. This is
an image quadrature, not extra emitting or absorbing particles, and not an exact per-frame
energy-conservation claim. World fog and baseline cabin haze are not quantized. Fire attenuation
and secondary density queries remain continuous. Each main or port view uses its own native
fragment coordinates; final output upscaling cannot enlarge the logical grain beyond one
native pixel. Grains have no separate update clock.

Cabin soot previously held position for a complete0.42–1.00s generation before jumping to a new
sample. It now rises continuously at the normal simulation cadence, keeps a stable identity
within each lifetime, and has vanishing projected-area coverage at death/birth. Birth overlaps
the upper flame support instead of forming a separate cap above it. Strength still changes
parcel count rather than enlarging grains. Source-geometry clearance still rejects blocked
paths. These prescribed soot paths do not conserve a coupled flame-to-soot chemical parcel.

CPU evidence: `artifacts/runtime/smoke_native_pixel_tests_20261001.log` validates the complete
OLD raster shaders, ABI and mean optical coverage (15/15). `smoke_continuity_tests_20261001.log`
records the incremental source tests. Actual native capture is required before visual acceptance.
The preserved baseline is `captures/smoke_baseline_wreck800_20261001_near.png` plus its video,
recorded from the preserved17:51 executable, not regenerated with the candidate.

Native checkpoint: the candidate executable SHA256 is
`19c9866299be4d27b6fff6ddf14d65ea6834c4e4f7447d51d91667ae60af48e1`.
`captures/smoke_candidate_wreck800_20261001.mp4` matches the preserved baseline environment,
manifest, cameras and sphere Bayer8 treatment. `smoke_candidate_blizzard800_20261001.mp4` uses
the disclosed2.5 storm override and screen Bayer8. Both are actual native800×450,30fps,
360frames/12s; muted, staged fire diagnostics, not combat/audio evidence. Both receipts passed
4/4 checks: effects all360frames, zero saturated parents, zero missing particle identities and
maximum per-grain weight0.07954/0.07980 (limit0.5). Submitted flame-grain count40,704–41,920 (not surviving-fragment count);
near/deck/4-frame contact sheets were inspected. The previous detached dark lobe is connected
to the source and has pixel-grain coverage; flame warmth and surface attachment remain visible.
The `_verification.json` files retain ffprobe output and executable hash. Subjective preference
and actual moving playback remain the user's review; still/receipt acceptance is not a claim
that a full smoke fluid simulation or ALSA playback has been validated.
