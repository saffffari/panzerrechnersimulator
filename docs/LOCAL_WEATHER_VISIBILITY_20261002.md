# Local weather and sight, 2 October 2026

Implemented in the adaptive-audio development checkout; interactive visual/AI acceptance remains pending.

The renderer already advected a spatial extinction field with wind (110 m broad banks, 27 m eddies). Tanks and AT guns instead used a single storm-dependent range; infantry sight was only geometry-limited. Gameplay now reads the renderer's current extinction, precipitation composition, vertical support and wind offset. It evaluates Beer–Lambert contrast along the actual observer–target segment. CPU density reproduces the existing WGSL hash/noise/bank function; the vertical-profile function is shared Rust code used by renderer uniforms and gameplay. No render weather strength, lighting, exposure or shader ABI was changed.

The CPU query averages two 48-sample directed integrations so swapping observers cannot change the answer. Render quality presets may use fewer samples, so this is common medium data and function, not a promise of bit-identical integrated pixels. Recognition requires at least 10% transmitted contrast: an explicit gameplay approximation, not a measured human-recognition model. Tank detection accrues more slowly as contrast falls. Recognition/firing, infantry sight and visible hand signals, allied targeting, and the player's binocular map marking respect the same medium. Existing FOV, silhouette, night and range limits still apply. Hearing, terrain navigation and projectile collisions do not acquire a visual-weather block.

Source-derived Tiger air columns exclude the same finite cabin spans as rendering; exterior fog resumes after a real port/hatch or source recess. The CPU DDA follows the existing 2 cm column representation and articulated hull/turret pose. These sampled source intervals are not an exact mesh Boolean or ventilation simulation. Walls and terrain remain independent geometric occluders. Roof shelter suppresses precipitation/audio using the existing roof model, but open ruins still contain bulk airborne aerosol in the renderer and AI. No clear-air bubble is inferred merely because an observer is sheltered.

`SoundscapeInput.whiteout` reports *outdoor* optical severity around the listener: four 35 m horizontal paths estimate extinction, converted to a 10%-contrast visibility distance, with smooth onset below 35 m and full intensity at 12 m. Cabin/hatch/building sound attenuation is applied separately by the audio director. Snow-free fog does not request the blowing-blizzard recording. This does not intensify the weather merely to make the heavy loop play.

The actual player main-gun discharge also calls `soundscape_main_gun_fired()` immediately after its authoritative shot counter advances. The retired, noncompiled trigger-time result path is unchanged.

Validation performed without Cargo, GPU rendering or audio-device playback:

- 8/8 CPU tests passed using the actual weather and cabin source modules, real Tiger GLBs, and cached dependency libraries. Includes moving/advection invariance, observer reciprocity, nearby consistency, analytic fog and layer clipping, CPU/WGSL density constants, source recesses, and translated/rotated cabin exclusion.
- Log: `artifacts/runtime/weather_visibility_source_cpu_20261002.log`.
- A separate optimized outdoor-only microbenchmark measured 10,000 reciprocal queries in 48.2 ms (~4.82 microseconds each). It excludes cabin DDA and is not a game frame-rate measurement.
- Integrated filters for the normal build: `weather_visibility::tests`, `renderer::cabin_medium::tests`, existing renderer world-medium tests.

The final coordinated call-site patch also applies optical visibility to winter street investigation stopping, review-pilot target selection, and visible KV-2 film cues. Geometric navigation, hearing and explicit ballistic lane checks retain their original behavior. Native player/AI tests in both local whiteout and adjacent clearing are still needed before claiming visual fairness accepted.
