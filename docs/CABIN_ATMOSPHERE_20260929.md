# Cabin practical lighting — 29 September 2026

The requested direction is a darker, oppressive steel interior with small practical sources,
retaining the PSX presentation and the ability to operate the gun. This pass corrects the
lamps' source power and geometry visibility. Cabin paint, exposure, eye adaptation, daylight
apertures, camera sequence and tone mapping are unchanged in the comparison.

## Source correction

The dome, four annunciators and three alternative breech-lamp colours now each have one
finite-cube record. The visible mesh and positioned source share dimensions, sRGB-decoded
colour, emission and state. The dome's existing small flicker drives both outputs. Sources
and visible lenses persist independently of the observer's station; only the active breech
colour occupies its lens.

For cube half-size `h`, emitting area is `24 h²`. The existing diffuse mesh emission has
total flux `π L A`; the isotropic point representation therefore receives intensity `L A / 4`.
This conserves integrated power. It does not reproduce a cube's directional projected area
or its exact near field. Radiances and dimensions are authored inputs, not measurements of
historical Tiger fixtures. The TPZ dial and ignition lamp retain their authored powers because
their emitting surface area and radiance have not been registered.

At peak state the dome's intensity becomes 0.000100602 from 0.0084, an 83.5-fold correction
to its agreement with the visible 54 mm cube. The ready lamp changes from 0.0007 to
0.000139392. Decoding the green lenses from sRGB also removes the overly pale point-source
spectrum. These ratios are source-power corrections, not promises about displayed brightness:
daylight, visibility and the existing eye response still determine the image.

## Geometry and transport limits

The raster fixture-shadow bank covers eight stable source identities: dome, TPZ, ignition,
four annunciators and the active breech lamp. Surfaces and local smoke use the same fixture
visibility. A source's own emitting cube is excluded only from its own shadow map: other
fixtures, the fan and all surrounding machinery remain casters. The 5 mm near plane retains
the fan underside 27 mm above the dome centre, previously inside the 35 mm clipped region.

This extends the existing cube-map approximation: 512-square faces, 8 m range and filtered
depth comparisons. Local smoke evaluates fixture visibility at its existing optical-depth
centroid, not at every integration step. World fog's positioned-light omission is unchanged.
The two static/composed shadow banks allocate 96 MiB at eight lights, previously 24 MiB.
Inactive sources can skip drawing, but stationary cache reuse must be measured rather than
assumed.

Physical High retains its existing source-ray endpoint shortened by the source radius. This
can omit an occluder within that radius, including the fan's 27 mm separation from the dome.
The new source radius is the actual cube's circumscribed radius, 46.77 mm for the dome;
it is not adjusted to conceal this remaining near-field disagreement. Raster and ray-traced
transport are not claimed identical.

## Reproducible evaluation

`catalog/shader_reviews/cabin_doom_sweep_20260929.v1.json` runs 11.5 seconds in a healthy
Act 0 Tiger. It visits loader breech, ammunition rack and tray, then gunner and lower commander
views. Ordinary LMB at 6.3 seconds fires the initially chambered round. There are no forced
lamp, exposure, hatch or damage changes. The sequence is a controlled player-view capture,
not a completed native mouse playtest.

Media and receipts are flat under `captures/astra/`, prefixes
`cabin_doom_20260929_baseline` and `cabin_doom_20260929_candidate`. Each recipe seals the
binary, fixture, command, environment and outputs. The baseline binary SHA-256 is
`d705311b440cb5e3678999b42124b47d554e71f3b551aa1362eb1099043edbb1`, including the cupola
correction. The paired films are included in `immersion_review_20260929.html#cabin`.

Performance uses a separate 96-frame fenced stationary renderer run and a six-second
stationary/moving capture, with identical inputs on each binary. Capture timings include
readback; neither measurement is interactive FPS. Baseline stationary median is 26.12 ms
(p99 30.87 ms), capture stationary median 29.80 ms (p95 31.19 ms), and capture moving median
31.77 ms (p95 35.12 ms). The stationary baseline rebuilds twelve static shadow faces per frame.

Candidate binary SHA-256:
`92b5b4190a30ebddb9db804447776acf60edffde8d39bde6d54d690c45f79411`.
Its stationary renderer median is 31.62 ms (p99 34.93 ms), up 21.1%. Five active fixtures
render thirty static cube faces per frame, costing 6.538 ms GPU against the baseline's
2.488 ms. Capture/readback medians are 44.54 ms stationary and 38.88 ms moving (p95 59.11
and 46.04 ms). This is one bounded pair; CPU tests were concurrent during candidate capture,
so its tail variance is not isolated. Full recipes and raw samples are retained.

The read-only cache audit found four nearby moving German escort actors while the vehicle
pose stays bit-stable. Their instanced pose pages participate in the static bank, so valid
moving shadows force immutable cabin machinery to redraw. A follow-up should move those
pages into dynamic composition, retaining every caster and clearing departed shadows by
restoring static depth first. Far members of broad pose-page bounds can also cause unnecessary
invalidation. This pass does not implement that cache correction.

## Observed result and gates

The six matching frames were visually inspected. The broad amber wall/ceiling wash recedes;
the breech has a stronger dark silhouette, brass retains small highlights, and the lower
commander view has deeper shadows. The ready lens and sight remain distinct. The actual shot
still gives a brief bright cabin flash. The rack is substantially darker; a complete reload
in the native window is still needed to judge its practical readability during interaction.
No further paint or exposure change was necessary for this first atmosphere pass.

Whole-frame decoded display-luma means (including bright lenses and optics; these are not
physical luminance measurements or an artistic threshold):

| View | Before | After |
| --- | ---: | ---: |
| Breech | 0.3525 | 0.2526 |
| Rack | 0.2558 | 0.1546 |
| Tray | 0.3751 | 0.2845 |
| Gunner | 0.1979 | 0.1922 |
| Shot | 0.4681 | 0.4830 |
| Lower commander | 0.2250 | 0.1631 |

Both comparison videos played to the end in the local Chrome review page. Soundtrack export
retains unity gain; no new subjective listening acceptance is claimed for this lighting pass.

- Release build passed; final game unit suite **1,407 passed, 0 failed, 51 ignored**.
- Composed raster shader suite **14 passed**, including main/optic smoke and shared uniform ABI.
- High640 candidate and Laptop576 identical-input sweeps each passed **15 state checks** and
  all **6 timeline actions**, including one actual shot. Both contact sheets were inspected.
- Stationary/moving performance fixture passed **9 checks**, with the same 15.76 m displacement.
- Physical High compiled and rendered the full scene on RTX 4090. Its exposure converged after
  24 transport warmup frames plus 13 exposure frames, but the image has heavy speckling and
  clipped panes. This is a construction gate only, **not accepted appearance**. The screenshot
  uses the commander's default look, not the sweep's lower-view pose; no matched Physical
  baseline was captured. Source-radius endpoint visibility remains unresolved above.
- Full workspace tests still stop at the existing missing `crate::resource` import in two
  integration harnesses. The physical-truth contract retains its existing `glass_pipe` substring
  failure (**17 pass, 1 fail**). Workspace formatting still reports widespread pre-existing
  differences. These are not represented as green gates.
- `git diff --check` and review-page Python compilation passed. One new near-plane test initially
  compared a requested 6 mm offset against an `f32` world point whose represented offset is
  6.000519 mm; it now uses the actual represented distance, retaining the original tolerance.

Logs are under `artifacts/immersion-20260929/cabin-*`. The new build has not been restarted or
accepted through native mouse input. Shader High is the visually reviewed runtime path here.
