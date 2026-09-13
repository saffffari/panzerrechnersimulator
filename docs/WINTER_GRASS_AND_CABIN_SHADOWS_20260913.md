# Winter grass and cabin lamp shadows — 13 September 2026

The shader branch now uses dedicated dry winter tufts: 12/7/4 splayed blades per near/middle/far clump, with mixed tip heights, randomized instance height/width/rotation and world-fixed irregular density patches. Snow gaps remain. The previous square summer-canopy pads are absent in winter. Three opaque instanced draws retain wind and tread compression without physical strand collisions. Winter reserves 21,504 roots and at most 475,136 triangles; the reviewed near walk submits 216,916 grass triangles across its complete visible window. Summer ground-class distribution and geometry are unchanged.

The supplied cupola screenshot preceded the earlier sun fix. A fresh matched view still showed interior lamp self-shadow stripes. Lamp cubemap filtering now compares each of four texel centers against the corresponding perspective receiver-plane depth and uses the raster triangle normal. Source casters, lamp intensity, bias, 512px map size and static caching remain unchanged. The corrected matched poster removes the repeated striations while preserving fittings' shadows. Broad angular surface shading remains visible.

Validation: release build; 14 grass tests; Naga validation; 12 sun and 324 lamp receiver-plane numerical cases with 4096 sampling phases each, including true nearer occluders; actual RTX 4090 Vulkan near-walk and cupola/hatch replays, all outcome assertions passed. Images inspected at matching camera positions.

- [Winter grass after](/home/alex/panzer/shader-graphics/captures/astra/winter_grass_fuller_walk_20260913.png)
- [Winter grass before](/home/alex/panzer/shader-graphics/captures/astra/winter_grass_before_walk_20260913.png)
- [Cupola after](/home/alex/panzer/shader-graphics/captures/astra/cupola_lamp_corrected_20260913.png)
- [Cupola before](/home/alex/panzer/shader-graphics/captures/astra/cupola_lamp_before_20260913.png)
- Machine-readable evidence: `artifacts/visual-feedback-20260913/grass-and-cabin-validation.json` and `cabin-receiver-plane.json`.

Near-walk mean render/readback increased from 6.68 to 7.23 ms. These fixed-clock captures are not interactive frame-rate measurements. The older cupola baseline overlapped a CPU test build, so its improved tail latency cannot support a performance improvement claim. This work remains daytime; Alex's additional snow/lamplight reference is preserved under `/home/alex/panzer/rew_ref/act1_snow_lamplight_reference_20260913.png`. Night mechanics, sleeping and shelter are deferred.
