# Continuous interaction silhouette correction

The selection mask used one coverage/depth sample, while the scene used four-sample MSAA. The single selected-mesh center sample was compared with the nearest of the scene's four samples. On a sloped cartridge surface, the selected center depth is farther than one scene subsample of the very same cartridge, so it failed its own depth test. Only scattered passing fragments survived, producing isolated yellow squares after edge dilation.

The corrected selection mask uses the same sample count and actual per-sample main-scene depth attachment. The mask resolves after those matched depth tests. Single-sample graphics retain the single-sample route. This does not introduce a depth bias or render through occluders.

The silhouette is drawn as a binary one-native-pixel edge after scene tone mapping/dithering into the LDR target. It therefore no longer inherits scene exposure, bloom, pressure distortion or Bayer holes. Selection targets still come from actual hovered mesh IDs; no projected vertices or point markers substitute for the object's silhouette.

The existing compact hammer/service-point cue is a separate tool/work-location symbol, not the selection outline. It remains gated by a carried repair kit and actual repair visibility/reach.

Reload lever audit: `Game::update` already dispatches `BreechOpen`/`BreechLock` on actual lever threshold crossings in the active Loader controls. Current source uses the newly dedicated steel-contact playback route. No extra unconditional callback was added, which would double the event. The current player-visible executable must be rebuilt before judging this updated source.

Pending coordinator checks: compile/pipeline validation with MSAA4 and AA off; native render of selected sloped ammunition, partial occlusion and fully occluded target; confirm one-pixel connected silhouette; actual lever audio after release build. These are implementation conclusions, not screenshot or listening acceptance.
