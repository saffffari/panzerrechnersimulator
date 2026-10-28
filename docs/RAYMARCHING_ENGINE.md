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
   Grass cards cast into the near cascade, the sky-cover map and the point faces through an
   alpha-tested, wind-bent depth program (the far cascade's texels are coarser than a blade).
2. **G-buffer** (`rm_gbuffer.wgsl`): linear albedo + material kind, world normal + roughness,
   emission + metallic, depth. Terrain uses the shared `terrain_material.wgsl` (snow cover,
   substrate, detail layers). Flame, soot and cubic-smoke grain pools become media; glass panes
   are drawn forward later (step 6b).
2b. **Sky cache** (`rm_sky_cache.wgsl`): the physical atmosphere and cloud march evaluated by
   direction into a 768×288 equirectangular map (azimuth across, elevation -20..90°), one phase
   of a sixteen-phase texel pattern per frame and every texel after a sky jump (time-of-day cut,
   weather override). Misses and reflections sample it; the solar and secondary discs are drawn
   analytically from the same transmitted irradiance through the cache's cloud transmittance
   (`rm_sky_discs`), because a disc narrower than a cache texel would smear. `KESSEL_RM_SKY=
   width[,phases]` overrides.
3. **Froxel media** (`rm_volume.wgsl`, compute): a 200×113×96 grid, slices exponential in radial
   distance over 0.25..320 m, one jittered sample per cell per frame. Each cell evaluates the
   weather layer, the cabin haze, the simulation's analytic plumes (muzzle, breech, fire smoke
   supports) and the **parcels** binned into its coarse 32×18×24 cell: flame grains merged into a
   temperature field (white core, deep-red fringe), soot and smoke grains, blast fireballs, spark
   glow and tracer strips. In-scatter: the key beam through the cascades *and* a 6-step march
   through the medium toward the sky; isotropic sky through the cover map; every clustered light
   in the cell (the two shadowed ones through their faces plus a 4-step march, the rest with
   local extinction). Reprojected temporal history (α 0.10 weather … 0.5 emission). Each cell
   also records a hot-gas measure (fire, fireballs) for the heat haze. The weather layer carries
   **spindrift**: wind above ~3 m/s lifts loose snow into a streaky layer under 0.9 m over the
   ground, the surface height read back from the sky-cover map (fading at the map's edge).
4. **Integration** (`rm_integrate.wgsl`): per column front-to-back, a per-slice table of
   in-scattered radiance, transmittance and integrated heat.
5. **Deferred shade** (`rm_shade.wgsl`): GGX/Lambert. Key beam: cascade PCF plus an 8-step
   screen-space ray-marched contact test. Skylight: hemisphere bands × vertical cover ×
   8-direction screen-space ray-marched occlusion. Lamps/fires/flashes: the clustered list of the
   pixel's cell, depth faces or contact march, extinction marched through the froxel grid toward
   the source. Emission. Then the integrated medium composites over the pixel; beyond 320 m an
   analytic homogeneous weather layer lit by sun and sky. **Heat haze**: the integrated heat
   along the ray refracts the view (a climbing noise displacement of the G-buffer lookup, up to
   six native pixels through a fireball) — a presentation of refraction by the froxel heat field.
6. **Reflections** (`rm_reflect.wgsl`): screen-space ray-marched reflections on glossy and
   metallic pixels, Fresnel and roughness weighted, cached-sky fallback above the horizon,
   fogged.
6b. **Glass panes** (`rm_glass.wgsl`): the port slits, sight and eyepiece glass and instrument
   covers, drawn forward over the lit frame back to front, depth-tested, never writing depth. A
   thin laminated dielectric: Fresnel at the entry face, absorption along the refracted path
   through the slab's thickness (thinnest local extent), glints from the key beam and the
   clustered lights through a slightly wavy surface, grime that blocks and scatters, the
   reflection marched through the depth buffer with the cached sky beyond.
7. **HDR points** (`rm_points.wgsl`, `rm_sparks.wgsl`): one-native-pixel snow and dust lit by sun,
   sky and the clustered lights; fire grains near the eye as self-luminous embers; the game's
   analytic spark showers, hot rims, pressure vents, cook-offs, ground ejecta and one-pixel tracer
   strips (the established cosmetic ballistics of `weapon_particle_math.rs`), all seen through the
   integrated medium.
8. **Eyepiece glass** (`rm_lens.wgsl`, TZF and binoculars only): barrel refraction, lateral colour,
   field softening, grime and hairline scratches that refract. A presentation of the optic.
9. Exposure meter, bloom, **veiling glare**, crush, blit. The glare (`stylized_bloom.wgsl`
   `fs_glare`, this chain only) is the eye's intraocular scatter by the CIE general disability
   glare equation (L_v = E·5/θ², Vos & van den Berg 1999): four octave rings of twelve taps
   (1.5–12°) over a box-filtered pyramid of the blurred bright pass, each ring 2.2 % of the
   source luminance per octave, halved by `GLARE_SCALE` so a low sun leaves the field playable.
   The display stage's rod signal is reproduced in a cool mesopic hue
   (`observer::ROD_DISPLAY_HUE`, luminance-neutral); cones keep every source's colour.

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
- **Events** (`Event`, `track_events`): a heavy exterior blast record, the gun's own report (its
  flash mesh lighting at the muzzle) and a heavy armour strike are registered once by seed and
  kept for 24 s, so their sequel outlives the simulation's 1.85 s burst record. Every shape is a
  deterministic function of (seed, age) and the frame's wind. A detonation: a white-hot core for
  45 ms, three turbulent fireball lobes rising and parting over 1.3 s (yellow-white core to
  orange, emission ×6 over the first version), five soot puffs drifting downwind for 14 s, and —
  when the simulation also recorded ground ejecta there — a ring of blown snow sweeping out to
  16 m in 1.6 s, a column of snow and earth for 7 s, and 48 burning-residue embers around the
  crater. Fragment streaks (1,800 samples: 180 shards × 10 trailing samples, 250–1500 m/s with
  drag, most slow) for a quarter second, 64 burning fragments arcing out at 15–55 m/s and coming
  to rest on the ground for seconds. A muzzle report: the 88's blast lifts a snow cloud ahead of
  the muzzle for 3 s, and for 0.22 s the muzzle smoke supports are shaped into the brake's
  forward jet and two lateral lobes (`fr.report`). An armour strike: 420 molten droplets off the
  plate (20–80 m/s, cooling white→red over a second) and a puff of paint smoke.
- Grains the simulation throws from a detonation are read as its products: incandescent for the
  first quarter second, then the dark smoke of the burst (emission falls to zero, extinction
  ×1.6, colour to soot); the frame-filling orange cloud of the first showcase is gone. The
  simulation's own heavy-blast sparks draw every third shard as a four-sample dotted streak.
- Spark and tracer samples are one native pixel each with radiance by projected area, capped at
  six pixels of area so a shard passing the eye does not bloom into a blob.
- Light impact records (small arms, machine-gun reports) are 0.14 s glows the size of a fist.
- The game's self-luminous effect sphere meshes (muzzle flash, MG flash, impact flash, the
  expanding blast fireball pool) are not rasterized; each becomes a torn volumetric flash
  parcel sized by its bounds and driven by its emissive state, so a flash never reads as a
  glowing ball silhouette and a camera inside a blast sees fire, not an inner surface.
- Dense local plumes saturate at 0.6/m extinction so a detonation cloud stays translucent at the
  froxel scale instead of blacking out the frame.
- Blood grains and dried stains are the game's own instanced geometry (dark, roughness 0.08) and
  go through the G-buffer, so they catch lamp highlights and screen-space reflections.

## Measured frame cost (4090, 800x450, `gfxbench` exterior orbit over the Act-I city, 240 frames)

Measured alone on the GPU (`nvidia-smi` idle; concurrent captures or another session's game
inflate every number several times over, as a 27 ms and a 495 ms run during sibling-session
captures showed).

| | ms |
| --- | --- |
| raymarch wall per frame (render + wait) | 13.4 avg, 13.6 p50, 21.7 p99, 8.9 min (GPU shared with another session's capture at ~34 % the whole run; the min is the closest to a clean figure) |
| shader backend, same bench, msaa4 (earlier solo run) | 251 avg (its orbit also pays the raster optic tiles) |
| CPU prepare (lights, parcels, events, embers, binning) | 0.54 |
| CPU encode (draw submission, 16 scene traversals) | 0.94 |
| GPU rm-sky (one phase of the direction cache) | 3.5 (was 4.4 per-miss) |
| GPU rm-gbuffer | 2.0 |
| GPU rm-shadow-far / near / sky-cover | 0.74 / 0.53 / 0.40 |
| GPU rm-point-face (12 faces) | 0.37 |
| GPU rm-volume / integrate / shade / reflect / glass / points | 0.42 / 0.20 / 0.07 / 0.01 / 0.002 / 0.01 |
| GPU bloom + glare pyramid | 0.08 |

GPU pass timestamps are the engine's `KESSEL_GPU_PROFILE=1` readback; the compute and fullscreen
figures are implausibly small next to the raster ones and are quoted as reported, not trusted.
The direction cache took the sky off the per-pixel path: its cost is now fixed per frame
(768×288/16 texels) whatever the view holds. WGSL `discard` only demotes an invocation, so the
cache's skipped phases return early explicitly (the first version ran the whole march for every
texel and cost 9.8 ms).

## A/B against the shader backend

`raymarch-showcase` also runs with `KESSEL_RENDERER=shader` (`captures/shader_showcase_v1_*`). The
same simulation content drives both: the 88 mm HE burst's exterior flame-grain pool is a blocky
orange cloud on the shader backend and, here, a quarter second of fire then the dark smoke of
the burst around the event's own fireball, sparks and dust; the muzzle report is a yellow disc
there (its point light scattering in the muzzle smoke supports) and a brake-shaped flash with a
snow cloud here; the burning wreck is discrete orange grain dots there and tongues of flame with
embers and heat haze here. The grains' count and lifetime remain the game's effect content.

## Captures

`raymarch-showcase PREFIX` (needs `KESSEL_RENDERER=raymarch`) writes sixteen stills on the Canyon
v2 workshop with weather and clock overridden per scene (`captures/rm_showcase_final2_*.png` and the
`.json` receipt): burning wreck at clear sunset (near, deck), 88 mm muzzle report, HE detonation
at +0.05/+0.3/+1.1/+3/+8 s, the report's snow cloud at +0.5 s, AP strike sparks on the hull, moonlit night wreck and Tiger, the
commander's cabin lamps at night, blizzard coax tracers through the TZF and from outside, the
Tiger in the storm. `KESSEL_SHOWCASE_SET=palette` writes eighteen stills of the same fixtures from
two observers across dawn/noon/dusk/night × clear/dry overcast/blizzard, plus light snow on a 5 m/s wind
(`captures/rm_palette_v7_*.png`) for judging colour; `KESSEL_SHOWCASE_SCENES=a,b` filters either
set. `screenshot` accepts `KESSEL_SHOT_WEATHER=clear|clouds|storm[:N]`,
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
  by simulation state, not combustion solvers; the event sequels (soot puffs, snow ring and
  column, fragment and droplet kinematics, the brake lobes) are authored motions seeded by the
  simulation's records, not fluid or fragmentation solvers.
- The sky cache is 0.47° per texel, marched with 20/4 cloud steps and refreshed over sixteen frames; clouds and stars are that
  soft, the discs are analytic. Below -20° elevation the cache's bottom row stands in.
- Glass panes: no refraction offset of what is seen through the pane, no second-interface
  bounce, a scalar (mean) transmittance for the blend, grime from a fixed noise field.
- Grass shadows: cards are absent from the far cascade; the sky-cover map sees the canopy, and
  blades ask it from 0.55 m above themselves.
- Heat haze is a screen-space displacement by integrated heat, not ray bending through a
  refractive-index field; spindrift is a density field over the cover map's surface, not
  saltating particles, and ends at the map's 48 m edge.
- The veiling glare is the CIE veil at half strength over a four-ring pyramid; the eye's ciliary
  corona and lenticular halo are not modelled.

## Open items

- The cupola periscope eyepieces are glass panes over the periscope housing, not relayed views;
  the raster optic tiles (an offscreen view per periscope) do not run on this backend.
- Track ruts on terrain: on branch `terrain-detail` the ground is the detail surface
  (docs/TERRAIN_DETAIL.md): ruts, boot prints and craters are geometry in every pass and the
  terrain LOD replaces the coarse mesh; without a `terrain.detail` block the coarse mesh is still
  drawn without the renderer's track map.
- History reprojection for the froxel grid uses the camera only; moving emitters smear up to
  ~10 frames.
- No temporal filter on the deferred shade; dither hides the per-pixel jitter of the contact and
  occlusion marches but a settled frame is not noise-free.
- Clear noon is the weakest palette: the act's clear-weather aerosol (0.0006/m) keeps the sky
  milky toward the sun and the dead-grass cards read as dark specks on snow; both are content
  (weather profile, grass albedo) rather than backend behaviour.
- Blood grains still go through the G-buffer only; no pooling or spray medium of their own.
- Spatial audio and the density of wreckage/equipment on the maps were done by sibling sessions
  in their own worktrees (`claude/canyon-wreckage-20261027`, `claude/spatial-audio-20261027`).
