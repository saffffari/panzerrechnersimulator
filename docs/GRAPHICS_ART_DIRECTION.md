# Graphics art direction

## Authority and review status — 2026-09-09

Alex designated **every file in `/home/alex/panzer/rew_ref/`** as the visual north star for the
graphics rebuild: moody atmosphere across the full range of weather, environments, and times
of day. The review covered all **53 files: 50 still images, two GIFs, and one MP4**. All stills
were opened directly; each animation was inspected through six extracted frames. This is
sampled animation review, not a claim that continuous playback or audio was reviewed.

The currently opened shader renderer is a **prototype**, not an accepted realization of this
reference style. Captures such as `shader_loader_v3.png` and `shader_dismount_v3_roof.png` in
`captures/astra/` document implementation progress. A working shader backend, working optics,
or a fast frame alone does not establish art-direction acceptance.

**Latest user choice: the cabin must be muted gunmetal grey, darker and enclosed but readable.**
This overrides the white/light historical cabin-paint direction in older references and notes.
The Tiger exterior remains **painted German grey**. Neither choice means featureless black,
mirror-like bare metal, or a global recolor of unrelated materials.

## Visual language

Build perspective-stable 3D pixel art from purposeful color clusters, strong silhouettes, and
readable depth. Pixel size alone is insufficient. Large shapes establish the scene; selective
mid-scale detail explains foliage, buildings, equipment, and surfaces; small accents identify
edges or light. Avoid uniformly distributed noise, glitter, crawling highlights, or isolated
bright ground specks. Keep this organization coherent while turning, walking, and moving objects.

The references permit both rich detail and deliberate emptiness. Snow scene
`3255ccbe37e5bec6aa9fafa522910660.jpg` uses nearly empty ground and sky around a few building
silhouettes. `542d2112e27ed285c1a47959918e5d88.jpg` makes a barn and windmill legible through
simple dark structure against pale snow. Do not fill every empty area with grass or texture.

## Weather, light, and palette

“Moody” does not mean permanently dark, desaturated, or foggy. Each condition needs its own
coherent sky, ambient light, shadow color, visibility, surface response, and restrained accents:

- **Clear daylight:** `fe6a0ee67ab9066276556788a811b862.jpg` combines blue sky, substantial cream
  clouds, a pale house, and cool distant hills. Bright weather must retain shape and atmosphere.
- **Fog and overcast:** `d0815364c1505e7afd8b0f6d51c6386d.jpg` separates successive trees and a
  ruin through declining contrast. `fe09b66c285d8c646f7ca8db62cb4c68.jpg` uses reeds, bare trees,
  reflective marsh water, and a low gray sky. Fog reveals distance relationships.
- **Snow and whiteout:** the two snow-building references above support narrow, pale palettes,
  disappearing horizons, and sparse landmarks without losing nearby forms.
- **Pink dusk:** `acb9a5a752a00e8712164e0a83ef3ec5.jpg` combines a rose sky, violet distance,
  dark foreground vegetation, and utility-line silhouettes.
- **Rainy blue night:** `1e43d16c4b7dff5fb047de477333c38d.jpg` provides blue atmosphere, wet-road
  reflections, and small warm/cyan lights. Transfer those relationships to the game's world.
- **Localized warm night light:** `0399de3eec02290233af19d563f6381f.jpg` concentrates fire and
  lamp light against dark surroundings; `7223eec1d6422439ac1cb2a2d86d99cf.jpg` places restrained
  red reflections on rails. Light must affect nearby surfaces and weather.
- **Stormbreak:** `dcd073c2f3f3cc1ba61d6c2dfd26d461.jpg` organizes cloud masses, shafts of light,
  and illuminated fields. `599352e3f95bea3a1a3c86f16ed766bd.jpg` contrasts golden land with a
  heavy blue-gray storm ceiling. Cloud shape and illumination must carry the drama.

Keep exposure stable across cabin walls, bright ports, and station changes. A darker cabin
still needs readable controls, ammunition, mechanisms, and hands; looking outside must not
cause the interior to black out.

## Landscape and construction

Compose playable space with foreground anchors, middle-distance routes and landmarks, and
layered horizons. `d0f5855999387d133a1f9f1b71137aa6.jpg` uses ruts, grass clusters, poles, and
sagging wires to establish depth. `bc889f13a198c27aa10915b1a8d8a57d.jpg` supplies readable field
boundaries and connecting roads. `27867233f983c14de4e6900535995b4f.jpg` uses varied landforms,
framing silhouettes, and distant layers. These relationships should survive ordinary player
movement, rather than work only from one staged camera.

Terrain needs meaningful slopes, field edges, paths, vegetation groups, and settlement shapes.
Avoid a mottled flat plane terminating in a single row of tiny repeated blocks. Foliage should
form readable masses with species and seasonal character; detail density must suit the location.

Interior photograph `058f07e1acd5790980e404c4f118bc08.jpg` informs enclosure, equipment mounting,
irregular assembly, cables, and contact shadows. Preserve construction cues while honoring the
new gunmetal cabin choice. Distinguish painted steel, worn metal, brass, rubber, glass, and
fabric through value, roughness, edge treatment, and localized wear. White cabin paint is not
required. Photos and concept art inform mood, scale, and construction; their science-fiction
machines, modern cars, buildings, or other subject matter need not enter the game.

## Restrained, causal animation

`7f03a0cb8cdd736a20c33ed990bff7ee.gif` keeps its alpine composition stable while small figures
move. `7e9ada31c15afa1f2fd026d36c9bffd2.gif` ties tank movement to tracks, concentrated dirt,
and subtle crew/hull motion. `824c1c1d12e57f2bab18b3e2c82dbdba.mp4` makes snowfall especially
visible around a warm streetlamp against dark buildings. Animate causes and their effects:
ground contact, wind, precipitation, smoke, and light interaction. Particles need depth and
occlusion; avoid equally bright particles everywhere or indiscriminate full-screen movement.

## Scene acceptance matrix

These are required visual checks, **not passed results**. Show the actual game in stills and
short movement sequences, including ordinary optics where applicable. Technical correctness
and performance remain necessary foundations; acceptance also requires the reference qualities.

| Scene | Required visual result | Check in motion |
| --- | --- | --- |
| Clear daylight | Shaped clouds, readable fields and landmarks, cool distance | Walk and turn; stable foliage and surface clusters |
| Fog / overcast | Several depth layers; nearby forms retain contrast | Approach landmarks; continuous visibility changes |
| Snow / whiteout | Pale coherent palette; purposeful sparse silhouettes | Nearby terrain and optics remain legible |
| Pink dusk | Rose sky, violet distance, dark framing shapes | Stable color and exposure through camera turns |
| Rainy blue night | Wet surfaces, blue ambient light, limited colored accents | Reflections and rain remain attached to scene depth |
| Warm lights at night | Local light pools and illuminated weather | Moving effects occlude correctly and preserve darkness |
| Stormbreak | Structured cloud ceiling, light shafts, illuminated land patches | Clouds and lighting change coherently without noise |
| Gunmetal cabin | Enclosed, darker gray; differentiated equipment and materials | Reload and station changes remain readable |
| Exterior Tiger / optics | Painted German grey; exterior views match scene conditions | Hatches, viewing angles, and motion remain stable |

Review against the references before calling the renderer visually accepted. Record remaining
gaps explicitly; backend completion does not complete the graphics rebuild.
