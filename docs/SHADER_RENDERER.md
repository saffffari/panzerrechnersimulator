# Panzerrechnersystem shader renderer

Implemented on `codex/shader-graphics-rebuild` in `/home/alex/panzer/shader-graphics`,
from gameplay checkpoint `c9e7178`. This is an optional raster architecture for the
existing game. The physical renderer and its shaders remain available; they have
not been deleted or replaced in the canonical checkout.

From this worktree, `tools/launch_shader_playtest.sh --playtest` launches its own
release binary. Add `--build` to rebuild first, use `--menu` for the title screen,
or `--check` for read-only asset checks. The launcher selects `KESSEL_RENDERER=shader`,
640×360 internal resolution and the High detail preset. Assets resolve through
`KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator`; binaries, logs and checkpoint
state stay in this worktree. It prevents simultaneous launches with the canonical
game. The existing desktop shortcut is not changed by this branch launcher.

Playtest defaults to the actual Act 1 winter Stalingrad map, **Die Stadt**, through the isolated
September 13 winter-approach revision:55 added props in five groups, preserving its authored
terrain, city placements and spawn. It keeps the separate Playtest rules
and enemy return-fire toggle. Set `KESSEL_PLAYTEST_MANIFEST` explicitly to test another
authored map; the launcher preflight reports and validates the selected manifest.

`src/graphics.rs` separates renderer selection from detail quality: `shader`/`raster`
select this backend; `physical`/`legacy`, or an unset selector, retain the old backend.
Shader mode requests no hardware ray-query features and submits no path-tracing or
stochastic transport passes. Shared legacy infrastructure still exists in the code.

| Implemented component | Current behavior |
| --- | --- |
| `src/renderer.rs`, `shaders/stylized_mesh.wgsl` | Rasterize existing loose and instanced source meshes, with existing transforms, animation, culling and material bindings. Far and near sun-shadow maps, two cached cabin-lamp shadow cubemaps, ambient-only SSAO, six directional light bands, hemispheric ambient, bounded GGX highlights and up to 32 positioned lights. Grass, emissive fixtures, terrain detail and track marks retain their effect paths. |
| `shaders/stylized_sky.wgsl` | Depth-gated banded sky, broad shaded cloud masses, finite sun and distinct day, dawn, dusk, night and storm palettes. |
| `shaders/stylized_smoke.wgsl` | Analytic Beer–Lambert world fog and eight midpoint samples per intersecting local smoke volume, including muzzle plumes, breech gas and cabin haze. Scene depth terminates integration; output is premultiplied HDR. |
| `shaders/stylized_particles.wgsl` | Existing simulation-owned snow/dust positions rendered as depth-tested pixel particles. Snow is stable and pale; dust is muted, with no emissive glitter. |
| `src/raster_glass.rs` | Ordinary panes receive a faint premultiplied sheen, sorted far-to-near, testing opaque depth without writing depth. Registered optic panels and internal mirror/lens assemblies are excluded. |
| `src/raster_optics.rs`, `shaders/raster_optics.wgsl`, `shaders/raster_optic_particles.wgsl` | Seven cupola ports, the fixed loader port, the flush circular gunner eyepiece, two moving front roof optics and the fixed MG telescope each use source entrance geometry and a distinct objective capture. The shared scene, glass, current smoke and snow render into each visible tile before its image is composited onto the physical entrance. |

Source textures use nearest sampling and perspective-correct UV interpolation;
terrain detail remains anchored in world coordinates, and lighting bands follow
surface normals/world lights. Camera-relative projection reduces large-coordinate
jitter. The final low-resolution image uses nearest upscaling, while UI draws at
output resolution. **This is conventional triangle rasterization, not an implemented
texel-splatting renderer.** There is no surface capture/shade/splat pipeline or
claim that projected texels remain fixed-size across distance and perspective.

Camera response uses fixed exposure **1.25**, with auto-exposure off and legacy
screen posterization disabled. Since September 13, the shader backend uses a fixed monochrome
4x4 dither of less than half one linear 8-bit step before its RGBA8 output is rounded. It does
not animate grain or change exposure; the physical renderer clean endpoint remains unchanged. Looking through a bright port or changing
seats does not change the exposure. `interior_rig::cabin_practical_gain` applies a
named **20×** conversion only to cabin electrical fixtures, status lamps and their
visible lens emissions; physical-backend gain remains 1. Dome intensity is 0.168
and lens emission 0.46 before its existing slight flicker. This is an artistic
calibration between renderer scales, not a measurement of historical lamp power.

The existing runtime Tiger paint override selects reviewed exterior armor/deck/barrel
nodes using `Material.001`: sRGB (72, 74, 78), metallic 0 and roughness 0.75. It
preserves source files, geometry and unselected steel/track materials. Alex's later
2026-09-09 cabin choice gives the named `Interior Paint` surfaces a muted gunmetal
finish: sRGB (120, 126, 132), metallic 0.60, roughness 0.72. The named `Turret Floor`
is darker sRGB (86, 91, 98), metallic 0.65, roughness 0.82. Each finish has its own
atlas swatch; illuminated dials, red indicators, seats, brass and glass keep their
source materials. The final combined material pass requires fresh in-game visual review; CPU source tests preserve geometry and non-target materials.
Other material textures, metallic/roughness factors and normal detail
remain part of shading.

The approximations are explicit. The dome and turret-position dial lamps use two source-geometry 512-pixel cubemap shadows; other positioned lights remain unshadowed. Their inverse-square falloff does not solve global illumination. SSAO removes only the occluded ambient contribution, leaving direct light and emission intact. Hemisphere fill and metal environment response are occluded by the real scene geometry through a 32-direction near-field visibility atlas (`src/sky_visibility.rs`: 24 sky and 8 ground directions, 384 px orthographic tiles, ±4.5 m footprint in a vehicle and ±12 m on foot, terrain excluded as the ground emitter). This is discrete bounce-free visibility, not global illumination; beyond the footprint the environment counts as open. Sky, fill, fog in-scatter and the beam follow one solar-elevation law (`shaders/solar_daylight.wgsl`, normalized at the 31° Act I sun), and world fog converges on the horizon sky radiance. Gun haze and plumes use the same atlas; their beam term uses the nearest sky directions rather than the sun cascade. See `docs/ATMOSPHERE_MOOD_20260925.md`. Fog/smoke use bounded density and lighting models,
without multiple scattering; local media are shadowed only by the discrete visibility atlas. Particle
cabin shelter uses a conservative box rather than exact hull triangles. Transparent
mesh sorting and sheen do not model refraction or exact intersecting-glass ordering.

Optics use an upright image-plane approximation with a virtual pupil centered on
each authored port axis at the observer's normal eye relief. The objective stays
at its source position, just beyond its exterior glass; retained hoods and scene
geometry still occlude it. Cupola protective-face tilt is separate from the horizontal observation bearing. The fixed MG mounting export lacked a through-bore; a finite 23 mm runtime bore is reconstructed only through the two measured mounting components, retaining the original source files. Observer clipping and capture framing follow the visible
entrance. Off-axis pupil parallax, finite lenses/prisms, aberrations and recursive
optical transport are not simulated. All twelve registered optical surfaces are eligible; tile
resolution is bounded to 256–640 by 128–360 pixels. This is not a historically
validated optical prescription.

As of 2026-09-09, the integrated CPU suite reports **873 passed, 5 ignored**, and
all **3 raster shader/ABI tests passed** with ray-query capability disabled.
Actual 640×360 GPU media exists, including the corrected
[Loader v3 capture](../captures/astra/shader_loader_v3.png) and its
[receipt](../captures/astra/shader_loader_v3_manifest.json). These establish bounded
runtime/capture evidence, not complete interactive, night-weather or performance
acceptance. See [validation instructions and receipts](SHADER_PLAYTEST_VALIDATION.md)
and the [review gallery](../captures/astra/shader_playtest_validation_20260909.html)
for individual scenario status. Older scaffold entries can still be marked pending;
the linked run manifests identify actual recorded results.

The full visual target and condition-by-condition acceptance scenes are recorded in
[Graphics art direction](GRAPHICS_ART_DIRECTION.md), based on Alex's complete `rew_ref` folder.

## Current Playtest additions

Held rounds/cases use source-triangle collision and release-to-drop. The breech lever is a click toggle; aligned continuous grip loads a round. One complete 12-slot rack holds HE; all other rack slots hold AP, including refills. The original P08 magazine, barrel and toggle parts articulate and use distinct pack recordings. Ctrl crouches; Space jumps and accesses open hatches. All five hatches use their source assemblies, with explicit source-derived entry apertures.

A hover silhouette uses visible source mesh IDs. Six full-size Panzerfausts are stowed in a tested interior rack. The carried repair kit occupies both hands and requires nearby damaged components. Footsteps follow accepted grounded displacement and the actual supporting surface. C raises an imported CC0 physical compass. Grass uses three bounded layers to 180 m. The launcher remains at 640×360; adaptive resolution has not been introduced.
