# Raymarch backend (`KESSEL_RENDERER=raymarch`)

Branch `raymarching`, Windows / RTX 4090 / Vulkan through wgpu 27. Updated 2026-10-27.
Implementation receipt for `src/renderer_raymarch.rs` and `shaders/rm_*.wgsl`; the physical-truth
rules in `CLAUDE.md` apply, and every approximation below is named again in
`docs/PHYSICAL_TRUTH_AUDIT.md` ("Raymarch backend, 2026-10-27").

## Decision: rebuilt frame pipeline, borrowed scene containers

The shader (`src/renderer.rs`, raster/hybrid) and physical (ray-query) backends stay runnable for
A/B; `tools\launch_windows.ps1 -Renderer raymarch` selects the new one, `-Renderer shader` the
old. `graphics::shader_backend_enabled()` is true for both raster-fed backends, so the game feeds
the same effect data (flame/soot/smoke grain pools, analytic burst and tracer records, pixel
particles, raster optics registrations) to either.

What is **reused**: the scene containers the game fills (mesh and instance GPU buffers, material
bind groups, the campaign terrain material, lights, plumes, pixel particles, burst records), the
physical atmosphere solver (`physical_environment.wgsl`, drawn depth-gated at primary misses and
sampled on the CPU for the key beam and the sky bands), and the shared display stage (the
crewman's eye/exposure meter, bloom, `crush.wgsl` with its ACES, Bayer-8 dither, 31-level
posterize, reticle and optic masks, then the nearest blit). Every pass between the scene and the
display stage is new. The old renderer's cabin cubemaps, visibility atlas, raster optic tiles,
fine-grain and analytic-spark passes do not run and are not prepared.

## The frame (800x450 internal)

1. **Visibility maps** (`rm_depth.wgsl`): far sun cascade (±240 m, 2048²), near cascade (±20 m or
   ±6 m inside, 1024²), a top-down orthographic *sky-cover* map (±48 m, 512²), and six 256² faces
   for each of the two strongest lights near the eye. All from the scene's own triangles.
2. **G-buffer** (`rm_gbuffer.wgsl`): linear albedo + material kind, world normal + roughness,
   emission + metallic, depth. Terrain uses the shared `terrain_material.wgsl` (snow cover,
   substrate, detail layers). Flame, soot and cubic-smoke grain pools and glass panes are not
   rasterized; the first three become media, panes are open (see Open items).
3. **Froxel media** (`rm_volume.wgsl`, compute): a 200×113×96 grid, slices exponential in radial
   distance over 0.25..320 m, one jittered sample per cell per frame. Each cell evaluates the
   weather layer, the cabin haze, the simulation's analytic plumes (muzzle, breech, fire smoke
   supports) and the **parcels** binned into its coarse 32×18×24 cell: flame grains merged into a
   temperature field (white core, deep-red fringe), soot and smoke grains, blast fireballs, spark
   glow and tracer strips. In-scatter: the key beam through the cascades *and* a 6-step march
   through the medium toward the sky; isotropic sky through the cover map; every clustered light
   in the cell (the two shadowed ones through their faces plus a 4-step march, the rest with
   local extinction). Reprojected temporal history (α 0.10 weather … 0.5 emission).
4. **Integration** (`rm_integrate.wgsl`): per column front-to-back, a per-slice table of
   in-scattered radiance and transmittance.
5. **Deferred shade** (`rm_shade.wgsl`): GGX/Lambert. Key beam: cascade PCF plus an 8-step
   screen-space ray-marched contact test. Skylight: hemisphere bands × vertical cover ×
   8-direction screen-space ray-marched occlusion. Lamps/fires/flashes: the clustered list of the
   pixel's cell, depth faces or contact march, extinction marched through the froxel grid toward
   the source. Emission. Then the integrated medium composites over the pixel; beyond 320 m an
   analytic homogeneous weather layer lit by sun and sky.
6. **Reflections** (`rm_reflect.wgsl`): screen-space ray-marched reflections on glossy and
   metallic pixels, Fresnel and roughness weighted, sky-hemisphere fallback, fogged.
7. **HDR points** (`rm_points.wgsl`, `rm_sparks.wgsl`): one-native-pixel snow and dust lit by sun,
   sky and the clustered lights; fire grains near the eye as self-luminous embers; the game's
   analytic spark showers, hot rims, pressure vents, cook-offs, ground ejecta and one-pixel tracer
   strips (the established cosmetic ballistics of `weapon_particle_math.rs`), all seen through the
   integrated medium.
8. **Eyepiece glass** (`rm_lens.wgsl`, TZF and binoculars only): barrel refraction, lateral colour,
   field softening, grime and hairline scratches that refract. A presentation of the optic.
9. Exposure meter, bloom, crush, blit (shared).

### Lights

Every light the game offers (`Renderer::set_lights`, up to 448) is kept and clustered, instead of
the 32-light budget the other backends admit. The backend adds up to 64 synthetic emitters:
blast flashes (muzzle report, HE detonation, small impacts), spark-shower glow, hot rims, tracer
heads. Gather range follows the renderer's radiometric scale (cutoff 2e-7 irradiance; a cabin lamp
is ~1e-4 W/sr, sunlit snow ~500). A light's own housing is excluded from its shadow faces.

### Fire, explosions, blood

- Fire: the game's flame-grain pools (up to ~200k grains) are merged into cells (8–45 cm, growing
  with distance) and become emitting, slightly absorbing parcels; emission is a function of the
  accumulated density (the temperature field), broken into tongues by a shared drifting noise.
  Grains within 90 m are also drawn as embers. Soot grains become absorbing parcels.
- A heavy exterior blast record away from the gun is a fireball (0.9 s turbulent shell, cooling
  from white to orange, then a 1.7 s rising soot column) and a flash light; the same record at
  the muzzle (within 2.5 m of the plume origin) is an 80 ms prolate flash lobe along the bore;
  light impact records (small arms, machine-gun reports) are 0.14 s glows the size of a fist.
- The game's self-luminous effect sphere meshes (muzzle flash, MG flash, impact flash, the
  expanding blast fireball pool) are not rasterized; each becomes a torn volumetric flash
  parcel sized by its bounds and driven by its emissive state, so a flash never reads as a
  glowing ball silhouette and a camera inside a blast sees fire, not an inner surface.
- Dense local plumes saturate at 0.6/m extinction so a detonation cloud stays translucent at the
  froxel scale instead of blacking out the frame.
- Blood grains and dried stains are the game's own instanced geometry (dark, roughness 0.08) and
  go through the G-buffer, so they catch lamp highlights and screen-space reflections.

## Measured frame cost (4090, 800x450, `gfxbench` exterior orbit over the Act-I city, 120 frames)

| | ms |
| --- | --- |
| wall per frame (render + wait) | 20.3 avg, 17.7 p50, 32.1 p99 |
| CPU prepare (lights, parcels, embers, binning) | 0.45 |
| CPU encode (draw submission, 16 scene traversals) | 0.85 |
| GPU rm-gbuffer | 4.4 |
| GPU rm-sky (physical atmosphere at misses) | 4.4 |
| GPU rm-shadow-far / near / sky-cover | 1.3 / 0.4 / 0.9 |
| GPU rm-point-face (12 faces) | 0.7 |
| GPU rm-volume / integrate / shade / reflect / points | 0.31 / 0.04 / 0.07 / 0.02 / 0.01 |

GPU pass timestamps are the engine's `KESSEL_GPU_PROFILE=1` readback; the compute and fullscreen
figures are implausibly small next to the raster ones and are quoted as reported, not trusted.
The largest known costs are the physical sky evaluated per miss pixel and the four extra scene
traversals for visibility maps; both are first optimization targets (sky at half resolution or
cached per direction; cascade/cover maps on a cadence).

## A/B against the shader backend

`raymarch-showcase` also runs with `KESSEL_RENDERER=shader` (`captures/shader_showcase_v1_*`). The
same simulation content drives both: the 88 mm HE burst's exterior flame-grain pool is a blocky
orange cloud on the shader backend and a smooth volumetric one here; the muzzle report is a
yellow disc there (its point light scattering in the muzzle smoke supports) and a softer
cream one here; the burning wreck is discrete orange grain dots there and tongues of flame with
embers here. Explosion scale and duration are the game's effect content, not a renderer
property; the `he_*` stills are kept as they are for that reason.

## Captures

`raymarch-showcase PREFIX` (needs `KESSEL_RENDERER=raymarch`) writes thirteen stills on the Canyon
v2 workshop with weather and clock overridden per scene (`captures/rm_showcase_v2_*.png` and the
`.json` receipt): burning wreck at clear sunset (near, deck), 88 mm muzzle report, HE detonation
at +0.05/+0.3/+1.1 s, AP strike sparks on the hull, moonlit night wreck and Tiger, the
commander's cabin lamps at night, blizzard coax tracers through the TZF and from outside, the
Tiger in the storm. `screenshot` accepts `KESSEL_SHOT_WEATHER=clear|clouds|storm[:N]`,
`KESSEL_SHOT_FOG`, `KESSEL_SKY_UTC`. `KESSEL_RM_LIVE_SHADERS=1` reloads `shaders/rm_*.wgsl` from
disk at pipeline creation; `KESSEL_RM_DEBUG=1` prints source radiance, lights and binning stats.

## Named approximations (also in the ledger)

- Sky visibility is a single top-down depth map: cover above a point, no horizontal openings
  (a window lights nothing from the side; 8 % residual sky inside cover).
- Contact shadows and ambient occlusion are screen-space marches against the depth buffer;
  off-screen and thin geometry is missed.
- Only two lights have depth faces per frame; the others are unshadowed except by the screen-
  space contact test and the medium.
- The analytic weather layer beyond 320 m has no geometric occlusion.
- Fire parcels are merged grains, not the grains themselves; the froxel grid (4 native pixels) is
  the flame's spatial resolution; embers add the grain detail.
- Spark and tracer glow in the medium is an aggregate per burst, not per sample.
- Screen-space reflections see only what the frame sees; misses fall back to the sky hemisphere.
- The eyepiece glass is a presentation; its grime and dispersion are authored.
- The flame temperature field, fireball shell and muzzle-flash shapes are authored shapes driven
  by simulation state, not combustion solvers.

## Open items

- Glass panes and periscope glass are not drawn (open holes where the shader backend draws
  Fresnel sheen); the raster optic tiles (cupola periscopes) do not run.
- Grass still casts no shadow and is absent from the sky-cover map.
- Track ruts on terrain are not sampled in the G-buffer.
- History reprojection for the froxel grid uses the camera only; moving emitters smear up to
  ~10 frames.
- No temporal filter on the deferred shade; dither hides the per-pixel jitter of the contact and
  occlusion marches but a settled frame is not noise-free.
- Shader backend baseline for the same bench is not yet recorded on this machine.
- Spatial audio and the density of wreckage/equipment on the maps are outside this backend
  (separate sessions proposed).
