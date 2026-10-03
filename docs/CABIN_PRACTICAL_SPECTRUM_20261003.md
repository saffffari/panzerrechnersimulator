# Cabin source-spectrum correction — 2026-10-03

The user reports a brighter, yellow cabin and asks to restore a darker orange interior. This
bounded change warms the actual dome, TPZ illuminator and amber status lamps. It does not
change cabin paint, scene exposure, display gamma, environment light, geometry visibility or
shadow registration. The root renderer investigation owns the port, display and performance
regressions separately.

## History audit

The cabin fixture constants at `fca45cf` are unchanged from `b7c76fa`: practical power scale
0.003, dome half-size 14 mm and emission 1.15. Comparing `b7e4cd4` to `b7c76fa` shows a
*reduction* from scale 0.02 and dome half-size 27 mm. At full state the dome's scalar radiant
intensity fell from 0.000100602 to 0.0000040572, approximately 24.8 times lower. Those old
changes cannot explain a recent increase by added practical-source power.

The named `Interior Paint` and `Turret Floor` material overrides in `tank.rs` also retain
their earlier gunmetal values. The Git history since `b7e4cd4` contains no replacement of
the main Tiger hull/turret/gun bakes; the added rack fitting is a separate source repair.
This checks tracked source history, not arbitrary external asset edits.

`ae204a0` changed the display pipeline from linear RGBA8 quantization to sRGB quantization,
corrected a static bright port repeatedly charging flash adaptation, and bounded the rod
response at bright pixels. Those changes can recover dark surface values that the old path
lost, without adding any scene light. See `CABIN_ROD_RESPONSE_HOTFIX_20261002.md` for its
previous paired-render evidence. This history is not proof that any one of them alone causes
the user's present interactive screenshot; matched native views remain necessary.

## Source changes

| Source | Previous colour | Revised colour | Photopic source-output ratio |
| --- | --- | --- | --- |
| Dome lens and source | sRGB 255,171,66 | sRGB 255,145,48 | 0.8040 |
| Amber annunciator lens and source | sRGB 255,172,44 | sRGB 255,145,36 | 0.8002 |
| Waiting/reload breech lens and source | sRGB 255,245,20 | sRGB 255,164,32 | 0.5303 |
| TPZ dial illuminator | linear 1.0,0.50,0.08 | linear 1.0,0.29,0.035 | 0.7106 |

Ratios use the renderer's existing three-band photopic weights and sRGB decode where
applicable, at identical fixture state. They are neither measured historical lamp outputs nor
predicted screen-brightness ratios. No compensation increases the red band, intensity scalar,
emission multiplier or fixture dimensions. Green, red and blue status identities stay intact.

`FiniteCubeLamp::sample` supplies the same spectrum and finite-area power to the visible
dome/status lens and its transported light. The TPZ source retains its existing explicit
point-source registration: its emitting lens area is not yet extracted, so no new claim of
mesh/source power agreement is made for it. The original eight-light registration and all
occluding armor/machinery remain in use.

## Validation boundary

`git diff --check` passed for the source edit. Existing
`interior_rig::lighting_tests` exercise visible-versus-transported finite-source power and
spectrum agreement; the parent coordinates their compiled run. No new test merely mirrors
these four authored colour values. No build, GPU capture, native look acceptance or frame-rate
claim was performed by this sub-agent. Required visual checks are the same day/night cabin
pose with readable ports and distinct status lamps, followed by the live fullscreen view.

## Shared exposure metering follow-up

The production HDR meter geometrically averaged luminance inside its foveal, Gaussian and
full-field windows, then combined those log averages. Dark cabin pixels could therefore erase
most of the bright apertures' contribution, even with visible daylight occupying several
percent of the screen. The separate arithmetic full-field value affected only flash onset.

`shaders/exposure.wgsl` now integrates **linear** photopic and scotopic luminance inside the
same three windows and combines their arithmetic means with the existing 0.5/0.3/0.2 weights.
Pupil, pigment and neural adaptation all consume that one view meter. Their time constants,
log-domain neural state, flash onset/recovery, black-input floor and shared display mapping
are unchanged. There is no port classification or separate aperture exposure. This is a
bounded reproduction approximation, not a complete spatially resolved retinal model.

The numerical motivation used a declared synthetic 256×144 HDR field: two peripheral
rectangles occupying about 5% of the image at 1000 cd/m², the remainder at 0.00001 cd/m²,
60-degree vertical field and 16:9 aspect. The old meter was 0.0000156 cd/m² versus 24.04 cd/m²
for arithmetic integration. Applying the unchanged settled observer equations reduced cone
gain from approximately 4192 to 398. These are synthetic calculations, not measured native
gameplay results or proof of aesthetic acceptance.

`src/observer_meter_tests.rs` uses the actual production shader and generated observer
constants. Its explicit GPU test supplies HDR textures and checks black preservation, four
uniform luminances, symmetric/additive peripheral apertures, doubled source radiance,
consistent pupil/cone response and no recurring flash from steady openings. It is ignored
by default for coordinated GPU scheduling. A CPU test validates the real composed WGSL.
The older `exposure_clock_abi` harness also receives two missing flash-onset constants so its
stand-in block reflects the preceding hotfix. Root coordinates compilation and execution;
at this handoff these new checks have not been run.
