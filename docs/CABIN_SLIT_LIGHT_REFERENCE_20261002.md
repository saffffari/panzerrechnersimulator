# Cabin slit light and material references — 2026-10-02

Status: reference preservation and read-only source audit. No render/game source changed, no
build or GPU run performed, and no additional pass added to the immediate performance hotfix.

## Provenance and preserved originals

Both user-supplied screenshot paths under `/tmp/Spectacle.sLZkOG/` had expired. The exact original
filenames were recovered from `/home/alex/Pictures/Screenshots/`. Both originals remain there.
The copies below are byte-identical, 5120 × 2196 PNGs; neither was cropped, graded or regenerated.
The earlier report that `201846` was unavailable is superseded by this recovery.

| Reference | Preserved copy, relative to this checkout | SHA-256 |
| --- | --- | --- |
| Narrow slit shaft | `captures/cabin_slit_light_reference_20261002_201918.png` | `057a8c0d9f800ad495da2e097f34711c52c1311d5292c1cc3620bec35954dcf6` |
| Interior warm/cool materials | `captures/cabin_material_reference_20261002_201846.png` | `76629c9809ba4ed7d6c0df373c083b196d2e75ae315be7619612c6de557b7382` |

Machine-readable receipt: `artifacts/runtime/cabin_reference_catalog_20261002.json`.

These are visual references, not simulator captures or measured lighting/material data. The
player UI displays “The Tank”; that alone establishes neither production provenance, vehicle
identity nor historical accuracy. Exposure, grading, original camera response and possible
production lighting are unknown. A static screenshot does not establish temporal behavior.

## Observed visual cues

`201918`: a pale, narrow beam appears to begin at the horizontal slit near the center-right and
extends diagonally down-left through faintly visible air. The apparent beam broadens inward,
with a soft textured boundary rather than a bright hard cone. Much of the cabin and foreground
stays in deep shadow. Cool green-gray equipment on the left contrasts with warmer local light
on the wall and crew at the right. A small ceiling practical has a restrained surrounding halo.

`201846`: the foreground crew member is nearly silhouetted. Small cool pale apertures contrast
with localized warm ivory illumination on the right-hand equipment and a small red indication
on the left. The warm/cool separation and retained dark areas matter more than sampling literal
RGB values from this graded image. The reference does not justify making every cabin surface
green or adding a uniform warm fill.

The atmospheric cue is restrained suspended material in a cabin. Its precise composition
(dust, smoke, mist or production haze) cannot be determined from the screenshots. Neither image
shows snow or an outdoor whiteout filling the interior.

## Acceptance checklist for a later implementation

- [ ] A shaft originates at a real opening admitted by the modeled armor and current lid pose.
  Closing or physically blocking that opening removes the corresponding shaft.
- [ ] A relay/prism/periscope image surface is not treated as a hole or an emissive area light.
  A straight beam is allowed only where the actual source geometry supports that path.
- [ ] The beam occupies only the illuminated portion of the cabin medium, with dark adjacent
  air and retained shadows. No camera-facing cone, screen veil, unshadowed fill or light-power
  increase is used to manufacture visibility.
- [ ] Cabin dust, actual breech gas, fire smoke and exterior weather retain separate supports.
  Exterior fog remains visible through apertures but does not uniformly fill the closed cabin.
- [ ] Rotating the turret, opening a lid and moving the camera produce continuous geometric
  changes, without a beam glued to the screen or appearing through intervening crew/equipment.
- [ ] Warm practical illumination and cooler exterior light follow the actual sources and
  material response. Dark readable needles and existing instrument illumination remain intact.
- [ ] Clear daylight, overcast, night and blizzard comparisons use their actual source radiance;
  the same apparent daylight shaft is not forced into every condition.
- [ ] Native 800 × 450 geometry, the approved ordered dither and crisp nearest presentation
  remain unchanged. No smoothing of the final pixel grid is needed for this atmospheric cue.
- [ ] Matched blocked/unblocked scene captures include HDR/eye state and GPU timing. Numerical
  sample-count convergence and visible acceptance are distinct from compilation success.

## Current implementation, inspected in HOTFIX

`src/cabin_medium.rs` already derives three moving hull/turret/cupola domains from source
floor/roof/lid geometry, sampled at 2 cm in XZ. Its ring interface membranes partition media,
never block light. Opening a hatch does not replace the entire cabin medium with blizzard.
This is a sampled enclosure, not an exact air/steel Boolean or a ventilation solver: columns
include armor thickness and internal solids, while actual rendered geometry clips view rays.

`src/game.rs`, near the `set_interior_atmosphere` call, supplies a small authored baseline plus
firing/fire haze (`0.025 + interior_haze * 0.78`). `shaders/stylized_smoke.wgsl::cabin_density`
uses the existing `0.010` coefficient inside those domains. Neither value is changed here.

The smoke shader already integrates cabin optical depth over `cabin_next_span` intervals,
separately from exterior weather. Its `smoke_light` obtains geometry-filtered environment,
actual sun and admitted point-light radiance, with single scattering and source-path extinction
for point sources. This is useful existing infrastructure; it is not general multiple scattering.

The central limitation is spatial sampling of illumination. In `fs`, all local cabin,
breech-gas and plume optical-depth moments are combined, then `smoke_light` is evaluated once
at their combined centroid. A thin illuminated interval may miss that centroid completely,
or a lit centroid can represent the entire integrated local medium as lit. Exact extinction
intervals do not make that one-point lighting estimate spatially exact. This is a concrete
source limitation, not a measured explanation of a new game capture in this task.

The default map visibility also has finite resolution. `src/sky_visibility.rs` uses 384-pixel
tiles and a local interior half-width of 4.5 m: approximately 23.44 mm per shadow texel. The
local visibility comparison offset is 0.35 texel (about 8.20 mm there), with a 0.5 mm floor.
These are source-map approximations, potentially material for a narrow slit. There are 32
environment directions plus separate actual-sun maps; a narrow opening may subtend less than
the environment quadrature can resolve. The map path approximates transparent glass without
general refracted or mirror-relay transport. A physically blocked relay cannot be “fixed” by
declaring its bright displayed image transmissive.

## Smallest bounded next implementation route

1. Use a disclosed clear-day fixture with an actual directly transmitting slit/opening and a
   directionally aligned existing source. Keep cabin density, source powers and eye response
   fixed. Establish that the source-map resolution itself resolves the open/closed aperture.
2. Partition the existing local integral so cabin scattering can be sampled along the actual
   cabin spans. Replace its one-centroid **direct-source** estimate with bounded per-segment
   source visibility and Beer-weighted in-scattering; do not add a second decorative beam on
   top of the old full cabin contribution. Keep exterior weather and finite source plumes
   separate, with any retained centroid estimates explicitly documented.
3. Start by resolving the direct sun contribution along cabin segments while retaining the
   existing diffuse-environment approximation once. This avoids multiplying 32 environment
   connections and all point sources by every step before a performance measurement exists.
   It is an incremental direct-shaft improvement, not a claim that all cabin transport is exact.
4. Compare bounded candidate budgets, for example 8 and 16 samples, against a 32-sample
   reference. Analytic constant-density extinction remains unchanged. A low sample count is
   accepted only if the actual narrow opening converges adequately; do not hide missed shafts
   with a larger bias, artificial density, random flicker or widened aperture.
5. If aperture resolution limits the result, evaluate a locally refined source-geometry map
   or an appropriate geometric query before accepting it. Increasing the shadow bias or
   bypassing the occluder does not solve the underlying visibility error.

This future work must be timed against the current practical renderer before adoption. It
does not authorize an immediate extra render pass, an unbounded optical solver, a lighting
retune or a performance regression in the hotfix currently being validated.
