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
   screen-space ray-marched contact test. Skylight and **indirect light** (2026-10-28): a
   screen-space horizon gather (eight slices around the pixel, eight samples each out to 6 m of
   screen reach, no distance limit on the bounce) integrates one bounce from the surfaces the
   frame sees: each sample that raises a slice's horizon owns the cosine-weighted band
   pi/8 (c² - h²) between the horizons and contributes the previous frame's lit radiance there
   (reprojected through the previous view). The bands sum to the share of the hemisphere those
   surfaces cover; the sky bands light the rest (the upwelling-ground band, now snow 0.80 in
   winter and earth 0.30 in spring, stands in for off-screen ground, halved). A near-field
   horizon fading over 0.6..3 m is the contact occlusion. The GGX lobe takes its directional
   albedo of the same environment (the split-sum fit of Karis 2014), less the share the
   reflection pass draws in the mirror direction, so a rough metal takes sky and bounce light
   too. The snow under a hull lights its underside and a sunlit slope warms the shadow beside it;
   nothing off-screen bounces. Terrain takes the tank-track map as ruts (darkened, polished, the
   shading normal tilted by the map's gradient). `KESSEL_RM_PROBE=x,y` prints the lit radiance,
   previous-frame radiance and G-buffer of one internal pixel each frame (renderer units, before
   exposure). Measured on the noon Tiger still (green): the shaded hull plate 0.0039 without
   the bounce, 0.0068 with it, the sunlit flat snow 0.171; the plate's irradiance is 54 % of the
   flat snow's, the share a vertical face over snow should receive. The shade pass costs 0.17 ms
   with the gather (0.07 before).
   gradient). Lamps/fires/flashes: the clustered list of the
   pixel's cell, depth faces or contact march, extinction marched through the froxel grid toward
   the source. Emission. Then the integrated medium composites over the pixel; beyond 320 m a
   six-step march through the world medium and its fog banks, lit by the sky and by the sun
   through the cloud shadow. **Heat haze**: the integrated heat
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

## Near-camera haze (diagnosed 2026-10-28)

Report: "the haze is a bit too strong close to camera, it's just washing out everything close
range." Measured, not dialled: `KESSEL_SHOWCASE_SET=haze` renders the Tiger from 3, 10, 30 and
100 m with the sun behind the observer in the act's own weather and in clear, dry overcast and
blizzard overrides; `KESSEL_RM_PROBE=400,225` reads back the integrated medium column at the
frame centre. Transmittance per distance (after the fix; the medium values were the same
before):

| weather | 1 m | 3 m | 10 m | 30 m | 100 m |
| --- | --- | --- | --- | --- | --- |
| act (Canyon v2, sigma 0.0020/m) | 0.998 | 0.994 | 0.981 | 0.943 | 0.820 |
| clear (0.0006/m) | 0.9995 | 0.998 | 0.995 | 0.984 | 0.947 |
| dry overcast (0.0009/m) | 0.999 | 0.998 | 0.992 | 0.976 | 0.922 |
| blizzard (storm 3, about 0.04/m) | 0.97 | 0.89 | 0.67 | 0.30 | 0.02 |

Each column is exp(-sigma d) of the configured extinction: the first froxel slices do not
over-count, the history does not hold stale light, the cabin term does not leak, and at 3 m
the medium's in-scatter is 5 % of the hull's own radiance. What washed the near field out was
the display stage: `crush.wgsl` `eye_range` drew every pixel more than 1.5 stops from the
adapted level in toward it at 0.32 of the distance, in every view. A dark tank on snow sits
about 4 stops below the key, so it was lifted almost two stops toward grey, and the darkest
pixels of a daylight frame sat at sRGB 40-48 instead of 8-23. It was written (2026-10-04) for a
man in a dark cabin looking at bright ports; it now applies only there (a buttoned-up view that
is not the gun sight's or the binoculars' full-field picture, `eye_range_strength` in the crush
uniform). Measured on the act weather stills: frame black level (1st percentile luma, sRGB code)
47.9 to 23.3 at 3 m and 39.3 to 8.0 at 10 m; hull centre 51 to 30 at 10 m; frame contrast
(95th minus 5th percentile) 140 to 171 at 10 m. A moonlit night now shows bright blue-grey snow
under a black Tiger (frame mean 28 to 80); an overcast night is darker (29 to 17). Two smaller
contributors were also corrected: the multiple-scattering stand-in (an isotropic share of the
phase function) now grows with local density instead of applying 35 % to thin aerosol, and
spindrift lifts only in blowing-snow conditions (wind over 5 m/s, or snowing in wind), at 0.012/m.
`KESSEL_EYE_RANGE=knee,slope` overrides the draw-in for review (`1.5,1.0` disables it).
Captures: `captures/rm_haze_fixed_*` (after), `rm_haze_before_*` and `rm_haze2_before_act_*`
(before), `rm_haze_fix_act_before_after.png` (the act weather side by side).

## Snow on surfaces (2026-10-28)

Request: "we also need a snow effect where snow like hits surfaces like the tank and brushes off
or collects". `src/snow_cover.rs` keeps two loads, from 0 (bare) to 1 (every face that can hold
snow is covered). The player's Tiger starts bare, fills under snowfall and sheds snow when it
drives, traverses or fires. The world (wrecks, buildings, rubble, props, scatter, and every
vehicle but the player's) starts at 0.7 in a winter act and only fills. Snowfall is the
renderer's precipitation in a winter act (1 is steady snowfall) and zero otherwise. Per update:

| term | rate |
| --- | --- |
| snowfall | precipitation / 120 s x (1 - load): 63 % of full cover after two minutes of steady snowfall |
| air stream | 0.5/s x ((speed - 2 m/s) / 10 m/s)^2 x load: nothing below 2 m/s, 0.5/s at 12 m/s |
| turret traverse | 0.4/s per rad/s of traverse (to 2 rad/s) x load |
| a shot | keeps 55 % (muzzle blast over the glacis, recoil) |

The rates are presentation estimates, not snow-physics measurements.

**Drawing.** A mesh's load rides its push constant (the Tiger's parts are flagged with
`mark_snow_vehicle`; every other mesh and instance takes the world load). The G-buffer turns it
into the share of the face that holds snow (`rm_common.wgsl` `rm_snow_lying`): faces tilted less
than about 63 degrees from horizontal start to hold it and those under about 32 degrees hold it
fully; a breakup field at 10-40 cm, sampled in the mesh's own frame so the pattern rides the
hull and the turret instead of sliding over them, leaves bare patches at low load and continuous
cover at high load. The share goes to the shade pass in `g0.a`, which multiplies it by the
face's openness to falling snow (the sky-cover map: the deck under the turret overhang, the
inside of the hull and the ground under a roof stay bare) and lays a matte layer over the
material: albedo 0.86/0.89/0.93, roughness 0.85, metallic 0. Glass panes take the same cover on
their upper face: it blocks the view; from below, a few centimetres of snow pass a tenth of the
light above.

**Hooks.** `Game::player_snow_cover()` gives the Tiger's load for the spotting model, where a
snow-covered hull has less contrast against snow; nothing reads it yet. The vision blocks and the
sight window are near-vertical and collect no snow in this model, so snow does not yet obscure an
optic; crew brushing an optic clear is a follow-up.

**Time series** (`KESSEL_SHOWCASE_SET=snow`, the act's Tiger on the Canyon v2 workshop, steady
snowfall from a grey 09:30 sky, the sky clock pinned so every still has the same light; each still
from behind and from above):

| still | Tiger load |
| --- | --- |
| clean hull | 0.00 |
| after two minutes of snowfall | 0.58 |
| after eight seconds at full throttle | 0.20 |
| one more minute of snowfall | 0.43 |
| after a shot | 0.25 |
| after four seconds of traverse | 0.23 |

Seen from above, at 0.58 the deck, the fenders and the turret roof read white; at 0.20-0.25 the
cover breaks into patches over all three, and after the traverse the turret's patches have turned
with it. From behind, the fenders carry most of what is visible.
Captures: `captures/rm_snow_v5_*.png` with the `.json` receipt, `rm_snow_v5_sheet.png`. The first
series (`rm_snow_v2`) let the day clock run through its three simulated minutes and its last still
was lit by a 1.8 degree sun; `rm_snow_v3` sampled the breakup field in world space, so the pattern
would have slid over a moving hull.

**Cost.** By construction: one three-octave value noise per G-buffer fragment of a face that can
hold snow on a mesh with a load, and one nine-tap sky-cover lookup per shaded pixel that holds
snow. `KESSEL_RM_SKIP=16384` sets every load to zero for the A/B. Measured interleaved (two runs
each, `gfxbench` 800x450, the Act-I city at its winter load) while another application held the
GPU: the shade pass 0.174 and 0.164 ms with snow, 0.164 and 0.170 ms without; the G-buffer's
run-to-run spread (6.0 to 8.6 ms) under that contention hides its share. The cost is below what
these runs resolve; an idle-GPU rerun is owed.

## Internal resolution (2026-10-28)

Question: "we could also potentially do well to reduce pixel res, what would that buy us graphics
wise?" The presets are 800x450 (default), 640x360, 480x272 and 320x180. `tools/launch_windows.ps1
-Resolution WxH` sets the grid and writes it into the saved graphics preferences, which the game
applies at startup and which would otherwise replace it. The graphics menu's "Render resolution"
cycles the same grids in play; `KESSEL_RES_MENU_SWITCH=WxH` drives that path headless (build at
one grid, switch through the menu's own apply), and it renders on this backend.

**What scales with the grid.** The per-pixel passes do: G-buffer fill, shade, reflections, glass,
lens, crush, bloom and glare, the points. So does the froxel volume, which is a quarter of the grid
on each side (200x113x96 at 800x450, 80x45x96 at 320x180): a smaller grid also coarsens fire, smoke
and fog-shaft detail. What does not scale: the sky cache (768x288 over sixteen frames), the sun
cascades (2048/1024), the sky-cover map (512), the point-light cube faces (256), the geometry
submission (sixteen scene traversals per frame) and the CPU's light, parcel and event preparation.
At 800x450 those fixed costs are most of the frame (table above), so a smaller grid can save at
most the per-pixel share.

Measured (`gfxbench`, `KESSEL_GPU_PROFILE=1`, 240 frames each) while another application held the
GPU at 96-100 %: every raster pass is inflated about threefold against the idle figures above
and the frame totals sit near 30 ms at every grid, so the totals do not separate the grids; the
pass split does.

| ms | 320x180 | 480x272 | 640x360 | 800x450 | 1280x720 |
| --- | --- | --- | --- | --- | --- |
| frame avg (contended) | 31.5 | 32.9 | 31.9 | 30.3 | 41.3 |
| G-buffer | 7.6 | 7.2 | 7.0 | 6.2 | 6.6 |
| sky cache / sky cover / far cascade | 3.5 / 2.2 / 2.8 | 3.5 / 2.2 / 2.6 | 3.4 / 2.1 / 2.6 | 3.5 / 2.3 / 2.1 | 3.5 / 2.2 / 3.3 |
| froxel volume | 0.08 | 0.14 | 0.23 | 0.39 | 6.7 |
| shade | 0.06 | 0.08 | 0.12 | 0.17 | 0.38 |
| integrate | 0.04 | 0.03 | 0.04 | 1.19 | 0.13 |

The G-buffer does not shrink with the grid: it is bound by geometry (vertices and draws), not by
fragments. The fixed passes do not move. The per-pixel passes scale with area as expected and add
up to well under 2 ms at 800x450 (0.2 ms at 320x180). A smaller grid therefore buys about a
millisecond, and there is little to reinvest; 1280x720 is where the volume starts to cost (its
froxel grid is 320x180x96). The ways to buy frame time on this backend are the fixed passes: the
sky cache's march (3.5 ms), the far cascade and the sky-cover map's geometry passes, and the
G-buffer's draw count. An idle-GPU rerun is still owed for the absolute figures.

**What it costs to look at.** `KESSEL_SHOWCASE_SET=res` renders the same stills at each grid, output
2400x1350: a live T-34 broadside through the TZF at the longest range up to 1,500 m whose sight line
clears the terrain (1,500 m on the Canyon v2 workshop, laid with the gun's superelevation, so it
sits 0.85 degrees below the reticle tip), the Tiger at 25 m, the burning wreck at sunset, the
commander's post at night (`captures/rm_res_sheet.png`, `rm_resv2_t34_sheet.png`).

| grid | T-34 hull at 1,500 m in the 25 degree TZF field |
| --- | --- |
| 800x450 | 4.5 px |
| 640x360 | 3.6 px |
| 480x272 | 2.7 px |
| 320x180 | 1.8 px |

This fixture's T-34 is pale with snow against a bright snowfield in clear air; its pixels sit
within about 10 % of the background's luminance, a faint smudge at 800x450 and 640x360, a single
darker pixel at 480x272, gone at 320x180. The TZF reticle is drawn per internal pixel in the crush
pass: at 640x360 the flanking triangles become bars, at 480x272 two of the six drop out, at
320x180 they merge into blocks. On the Tiger at 25 m the road wheels and their hubs read at
640x360 and above, the wheels still read at 480x272, and at 320x180 they merge into blobs while
the ordered dither turns into visible blocks on the hull side and the grass stalks into brown
squares. The light, colour and composition of every still hold at every grid.

**Display-resolution optic (design, not built).** The live gunner view is a two-pass composite:
the sight picture renders as its own frame (`render_optic_snapshot`), then the cabin pass seats
it in the eyepiece circle. Two changes would decouple the optic from the grid: render the sight
frame at twice the cabin grid (a second target set kept alive while the sight is up, about 40 MB at
1600x900), and draw the reticle in the final blit at window resolution, positioned from the
eyepiece circle the cabin pass already knows. With a 1600x900 sight frame the T-34 at 1,500 m
would span 9 px.

**Separate finding.** The Tiger's own barrel and muzzle brake appear in the right of the TZF field
on this backend (the stills show it at every grid): the sight camera sits left of the gun and
nothing hides the gun from it. Whether the real TZF 9b field shows the barrel needs the sight's
mounting geometry; until then it is recorded here, not hidden.

## Occlusion and reflections by ray query (2026-10-28)

Request: "we need ambient occlusion and real time reflections too", beyond screen space. On the
High preset the raymarch backend now asks the adapter for `EXPERIMENTAL_RAY_QUERY`
(`graphics::raymarch_ray_queries_wanted`; `KESSEL_NO_RT=1` or `KESSEL_RM_RT=0` vetoes it). The
renderer then builds the scene's BLASes at load (1,975 meshes in 34 ms on the Canyon v2 workshop)
and rebuilds the TLAS every frame (8,671 instances on the workshop, 18,227 over the Act-I city),
without the physical backend's surface pack or deferred integrator (`Renderer::rt_scene` against
`rt_on`). The shade and reflection passes bind it; without the feature they compile against
`rm_rt_stub.wgsl` and run as before.

**Occlusion.** Each pixel traces two cosine-distributed rays per frame to 4 m against the scene
(any hit; glass excluded) and keeps the share that escapes. The share is averaged over frames
through the reprojected history (the new estimate weighs 0.15; a history texel whose recorded eye
distance disagrees with the reprojected point's is dropped). Where the rays find more cover than the
screen-space horizon gather saw, their open share replaces it and the near-field contact term; the
difference is cover the frame does not show, and it returns the surface's own radiance of the last
frame. `KESSEL_RM_SKIP=32768` turns the ray queries off for the A/B.

**Reflections.** The screen-space march runs first. Where it misses, the reflected ray is traced to
80 m: a hit the frame shows (its depth agrees) takes the lit frame there; a hit it does not show is
a grey surface (albedo 0.3) facing the ray, lit by the sun through the cascades and by the sky
through the cover map; a clear ray sees the sky. The reflected radiance is then averaged over
frames through the reflecting surface's reprojection (the new estimate weighs 0.2), which turned
the rough lobe's per-frame jitter from sparkle into a steady gloss. The roughness budget is the
existing gloss weight: surfaces rougher than about 0.8 take no reflection (the armour's paint is
metallic 1 at roughness 1 and reflects nothing).

**A divergence, found and fixed.** The bounce light reads the previous frame. That frame used to
be copied after the reflection pass, so reflections fed the next frame's bounce; with ray-query
reflections the cabin's loop gain passed one and the loader's view climbed to a saturated green
over a few hundred frames (a probed pixel's green channel at 400 renderer units against sunlit
snow's 0.17). The bounce source is now its own copy of the shaded frame, taken before
reflections, glass and points. A first estimate for the unseen cover, the mean radiance of the
seen cover, also diverged when a single lamp sample stood for a whole covered hemisphere.

Measured on `KESSEL_SHOWCASE_SET=ao` (noon, clear, sky clock pinned; mean sRGB luma of the
2400x1350 still, screen-space only against ray queries):

| view | screen-space only | ray queries |
| --- | --- | --- |
| commander's post, hatch shut | 78.1 | 39.4 |
| commander's post, hatch open | 77.6 | 51.8 |
| loader's post, hatch shut | 131.5 | 51.8 |
| loader's post, hatch open | 132.1 | 51.9 |
| the rear under the turret bustle | 99.9 | 98.8 |
| the running gear from 0.7 m | 95.8 | 94.2 |
| the glacis from under the barrel | 141.2 | 137.1 |

The closed cabin was lit by skylight leaking through walls the frame could not see; it is now lit
through its vision blocks and, with the commander's hatch open, through the hatch onto the roof.
Outside, the snow under the hull's nose and the faces under the bustle darken; little else moves,
because the screen-space gather already sees most of an exterior. The loader's hatch state does
not change the loader's view (the open hatch is the commander's). Captures:
`captures/rm_ao8_*` (ray queries), `rm_ao7off_*` (off), `rm_ao8_compare.png`.

Two things the darker cabin shows. With reflections turned off it turns green: the breech's ready
lamp and the annunciators glow in the cabin haze and, at the eye's raised adaptation, dominate
the frame (their power is authored; this backend may not change it). And the cabin by day is now
dark enough to reopen the question the eye's range draw-in answers ("Near-camera haze").

**Cost.** Under the same outside GPU load as above, the shade pass's timestamps moved from 0.164-0.184
to 0.175-0.184 ms and the reflection pass's from 0.012 to 0.016 ms, interleaved over three runs
each; the frame totals (34-41 ms) are noise. These fullscreen timestamps under-report (the table
above says so); an idle-GPU rerun, with `KESSEL_RM_RT=0` to price the TLAS rebuild, is owed.

## Surface wear (2026-10-28)

Request: "these surfaces need more damage, like scratches, smudges, and grime, especially inside
tank". `src/surface_wear.rs` keeps four loads for the player's Tiger, each 0 (factory clean) to 1:

| load | grows with | start of the campaign |
| --- | --- | --- |
| mechanical wear (chipped edges, scratches) | each main-gun round (0.4 % of what remains) | 0.55 |
| interior grime (soot, oil, propellant residue) | time crewed (toward full over 40 min) and each round (3 %) | 0.35 |
| exterior grime (slush and earth from the tracks) | distance driven (toward full over 3 km) | 0.30 |
| glass grime (the film on the vision blocks) | each round (2 %) and distance (8 km); a wipe leaves 0.05 | 0.25 |

The rates are presentation estimates, not measurements. `Game::wipe_glass` is the crew's wipe;
nothing in play calls it yet.

**Drawing.** The hull and turret meshes carry both their outside and the crew compartment, so a face
counts as inside when it lies within 3 m of the compartment's centre (0.9 m above the turret frame's
origin, `fr.vehicle`) and faces it. In the G-buffer, for the player's meshes only (push `extra.y`),
with masks in the mesh's own frame so they ride the hull and turret: scratch strokes in two
directions gathered in patches, outside densest on faces that are stood and worked on, inside on the
floor and in the band at seat and hand height, lighter where they cut the paint; dark smudges inside
at seat and hand height; dust and grit on faces
that look up (the compartment's floor and shelves; the deck outside); slush and earth on the
running gear and lower hull outside. In the shade pass, within 4.5 m of that centre: paint worn
through to steel on creases (where a pixel's normal turns against its neighbours' at the same
depth), and grime gathered in corners (the near-field occlusion). The glass panes' fixed film
scales with the glass load (0.25 is its former strength). `KESSEL_RM_SKIP=65536` zeroes every
load for the A/B.

**Captures** (`KESSEL_SHOWCASE_SET=wear`, noon, clear, the sky clock pinned, the same pose for
both states): the commander's, loader's, driver's and radioman's posts and the commander's with the
hatch open, clean (every load 0) and after a fight (the campaign's start, ten rounds fired, a
minute at full throttle through the game's update: mechanical 0.57, interior 0.52, exterior 0.37,
glass 0.40); the hull outside before and after; the driver's vision block before and after a wipe
(`captures/rm_wear12_*`, `rm_wear12_sheet.png`). `KESSEL_WEAR_EXTREME=1` renders every load at 1 in
the clean pose instead of the fight (`rm_wear12x_*`), to see the masks alone. Outside the wear reads
clearly: slush on the road wheels, streaks and scratches on the hull side, the fenders' edges worn
light. Inside, in the shut cabin by day, most of it falls in the dark: the interior is lit through
the vision blocks only, and darkening dark paint shows little at this resolution; the scratches and
the dust on lit faces are what carries. After the fight the gun's mechanism stands differently, so
the interior pairs also differ there. The glass film barely shows by day: dust on glass in front of
bright snow is low in contrast, and both states read as the same bright pane.

Two first attempts were wrong and are recorded so they are not repeated: an oily, glossier smudge
made the matte turret ceiling mirror the vision blocks, and scratch strokes on the turret's tall
inner wall merged into pale vertical bars at this resolution.

**Cost.** Not measured: the GPU was held by another application throughout. By construction, five
value-noise lookups per G-buffer fragment of the player's meshes, and four neighbour normal and
depth reads per shaded pixel of the vehicle.

## Clouds and low fog in the world (2026-10-28)

Question: "do we have dynamic volumetric clouds and fog? low enough to be perceived within the
environment? that also cause shadows?" Before this, no: the cloud layer (900 to 2,200 m in winter,
cover from the weather's overcast) lived only in the sky, and the key beam carried the cloud's
attenuation along the eye's own sun ray as one factor for the whole frame, so a cloud darkened
everything or nothing.

**Cloud shadows.** A 256x256 map over 4 km around the eye (`rm_cloud_shadow.wgsl`, 16 m texels,
snapped to its grid) holds the layer's transmittance toward the key body from the eye's altitude,
twelve samples through the layer of the same field, extinction and motion the sky marches. The
frame now carries the clear-sky beam and `rm_sun_visibility` multiplies in the map wherever the key
light is asked for: surfaces, the froxel medium, glass, reflections, sparks and snow points. The
sun's disc, which the sky cache's cloud alpha already attenuates, was attenuated twice before (the
eye's beam times that alpha) and is now attenuated once. `KESSEL_RM_SKIP=131072` restores the old
single factor for the A/B.

With half cover and a 1.3 km thick layer, the December noon sun (18 degrees up) crosses about 4 km
of layer, so most of the ground lies in a cloud's shadow and the lit patches are where gaps line
up along that slant: on the field still the frame's mean falls from 137 to 105 and a lit patch
crosses the hill. Thirty seconds later the patches have moved with the layer.

**Fog banks.** `Renderer::fog_bank` (extinction inside a bank, coverage, depth, floor altitude) puts
patches of dense low fog into the world medium (`rm_medium.wgsl` `rm_fog_bank_density`): a
two-scale footprint field drifting with the wind, a height profile that pools them below the floor
plus their depth, so hills stand out of them. They enter the froxel volume like any other world
medium. Beyond the grid's 320 m the weather layer is no longer one homogeneous segment: six steps
through the same medium, each lit by the sky and by the key beam through the cloud shadow above
it, so a bank swallows what stands in it at any range and the far haze is no longer lit by a sun
the clouds hide.

**Captures** (`KESSEL_SHOWCASE_SET=clouds`): broken cloud over the field at noon and thirty seconds
later, the Tiger at 11 m under it (`captures/rm_clouds2_0_*`, against `rm_clouds2_131072_*` with the
old single factor); fog banks (0.03/m over 55 % of the low ground, 30 m deep) on an overcast
morning (`rm_clouds3_fog_bank`); a T-34 broadside at 800 m in the banks and without them, from
beside the Tiger at its commander's height (`rm_clouds4_t34_800m_banks`, `_clear`): without banks
it reads through the haze, in them it is a ghost.

**Not done, and why.** The froxel grid still ends at 320 m: there is no second cascade, the far
segment is a six-step march without geometric occlusion. Shafts through cloud gaps do not show:
the shadow edges are kilometres apart, outside the grid, and the far march is too coarse to draw
them (`rm_clouds4_shafts_low_sun`); they belong with the light-shaft work. The act's weather does
not drive the fog banks yet: they are a renderer field the showcase sets, and no mission director
transition moves them. The cloud layer does not reach the sky-cover map or the ambient bands:
skylight under a cloud is the frame's mean sky, not shadowed per position.

**Cost.** The cloud map's pass read 0.035 ms per frame under outside GPU load (a fullscreen
timestamp, which under-reports); the far march is six density samples per pixel beyond 320 m.

## Shadow tiers, many lights, and light through the periscopes (2026-10-28)

Questions: "do all objects in the world cast shadows?" and "can we improve the look of the
periscopes and their emission of light to tank interior with raymarching method?"

**Who casts into what** (`Backend::draw_scene` for the depth passes, `Renderer::batch_in_ray_scene`
and `rt_update` for the TLAS):

| object tier | far sun cascade (2048, 240 m around the eye) | near sun cascade (1024, 20 m; 6 m from the cabin) | sky-cover map (512, 48 m) | point-light cubes (the two strongest lights, 256) | ray queries (TLAS, High) |
| --- | --- | --- | --- | --- | --- |
| terrain, buildings, rubble, vehicles, props, the Tiger inside and out | yes | yes | yes | yes, except a light's own housing | yes |
| grass (alpha-tested instances) | no (coarser than a blade) | yes | yes | yes | no |
| glass panes (ports, vision blocks, sight and eyepiece glass) | no | no | no | no | no (the mask passes them) |
| periscope windows and mirrors | yes | yes | yes | yes | windows pass a ray, mirrors fold it |
| lamp lenses (own-colour emitters) | no | no | no | no | yes (shadow rays stop 0.15 m short of a lamp) |
| muzzle and impact flash meshes | not drawn (they are volumes) | | | | visible ones are in the TLAS |
| smoke, flame and soot grains, glass sheen | no (they are media) | no | no | no | no |
| raster-only dynamic instance batches | no | no | no | no | no |

Beyond the far cascade's 240 m nothing shadows the sun except the cloud layer (above). Every lamp
but the two strongest had no shadow at all, only a short screen-space contact march.

**Many lights.** With ray queries, each lamp without depth faces now takes one shadow ray per
shaded pixel toward its source, stopped 0.15 m (or four source radii) short of it, because the
emitter sits inside its own housing. On the commander's post at night the ceiling around the dome
lamp falls dark (the fan housing it hangs under shades it) and the frame's mean falls from 45.9 to
35.3 (`captures/rm_lamps1_0_cabin_night` against `rm_lamps1_32768_*`, which also drops the ray-traced
occlusion). The froxel medium's in-scatter of those lamps is still unshadowed (the volume pass does
not bind the TLAS).

**Periscopes.** The occlusion rays (above) now follow the periscopes' real optical path: a ray that
meets a periscope window passes it, one that meets a mirror reflects about the mirror's plane (its
normal from the mesh, in world space each frame), up to four segments. What a periscope shows, it
also lets in. On the daytime cabin stills the change is small (mean 39.4 to 39.0 at the commander's
post, 51.8 to 50.5 at the loader's): the apertures are small against the cabin's open hemisphere.
Named approximation: the light let in is the sky band around the receiving surface's normal, not
the radiance of the direction the periscope looks along.

**The hatch.** An open hatch already lights the cabin as the area light it is: occlusion rays escape
through it and bring in skylight (the commander's post, hatch open, 39 to 52 mean). The sun through
it is the cascades' business; the near cascade's 6 m inside the cabin covers it.

**Not done.** The cupola's eyepieces are still glass panes over the housings, not relayed views
(the raster optic tiles are a shader-backend path); the TZF is the full frame at the sight's field
with the lens presentation, the reticle drawn per internal pixel and the own barrel in its right
edge ("Internal resolution"); its relay, exit pupil and eye relief are not modelled. Light through a
periscope carries no image of the outside.

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
(`captures/rm_palette_v7_*.png`) for judging colour; `KESSEL_SHOWCASE_SET=haze` and `=snow`
write the series of "Near-camera haze" and "Snow on surfaces"; `KESSEL_SHOWCASE_SCENES=a,b`
filters any set. `screenshot` accepts `KESSEL_SHOT_WEATHER=clear|clouds|storm[:N]`,
`KESSEL_SHOT_FOG`, `KESSEL_SKY_UTC`. `KESSEL_RM_LIVE_SHADERS=1` reloads `shaders/rm_*.wgsl` from
disk at pipeline creation; `KESSEL_RM_DEBUG=1` prints source radiance, lights and binning stats.

## Named approximations (also in the ledger)

- Sky visibility is a single top-down depth map: cover above a point, no horizontal openings
  (a window lights nothing from the side; 8 % residual sky inside cover).
- Contact shadows are screen-space marches against the depth buffer. With ray queries the
  occlusion is traced to 4 m (cover beyond counts as open; unseen cover returns the surface's own
  last-frame radiance); without them it is screen-space and misses off-screen geometry.
- Ray-query reflections: an off-screen hit is a grey (0.3) surface facing the ray, lit by sun and
  sky; the reflection history follows the reflecting surface, so a moving reflection trails; the
  glass panes' own reflection march stays screen-space with the sky beyond.
- Only two lights have depth faces per frame. With ray queries the others take one shadow ray
  per shaded pixel (stopped 0.15 m short of the source); without them they are unshadowed except
  by the screen-space contact test. The froxel medium's lamp in-scatter is unshadowed for them
  either way.
- Beyond 320 m the world medium is a six-step march without geometric occlusion.
- Cloud shadows: a flat layer crossed from the eye's altitude in twelve samples, a 16 m map over
  4 km around the eye (clamped beyond); the cloud does not shadow the skylight.
- Fog banks: a drifting two-scale footprint field times a height profile below a floor altitude
  the weather sets, not a boundary-layer simulation.
- Fire parcels are merged grains, not the grains themselves; the froxel grid (4 native pixels) is
  the flame's spatial resolution; embers add the grain detail.
- Spark and tracer glow in the medium is an aggregate per burst, not per sample.
- Without ray queries, screen-space reflections see only what the frame sees; misses fall back to
  the sky hemisphere.
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
- Surface wear: four scalar loads for the player's Tiger (no per-face history); procedural masks
  in the mesh frame, not painted or simulated wear; inside and outside told apart by facing the
  compartment's centre; worn edges from neighbouring normals in screen space (silhouette-adjacent
  creases flicker with the view); the rates are presentation estimates.
- Lying snow: one scalar load per vehicle (the player's Tiger) or for the world (everything
  else, enemy vehicles included, at the winter's 0.7 even while they drive), not a per-face
  accumulation; leeward faces fill no faster than windward ones and hot parts (engine deck,
  exhausts, a fired barrel) do not melt it; a coverage layer on the material, not a depth field,
  so it has no thickness at an edge and casts no shadow; flakes vanish at a surface (no contact
  puffs) and shed snow does not fall as particles; the rates are presentation estimates.

## Open items

- The cupola periscope eyepieces are glass panes over the periscope housing, not relayed views;
  the raster optic tiles (an offscreen view per periscope) do not run on this backend.
- Ruts are shading only: the terrain triangles are not displaced, so a rut has no depth in the
  silhouette and casts no shadow of its own. Vehicles sink 0.30 m into the winter snowpack
  (`vehicle_ground::WINTER_TRACK_SINKAGE_M`, sim side), but the snow is not banked against the
  hull or ploughed by the belly.
- Indirect light comes only from what is on screen, one frame late; a probe grid (world-space,
  lit by the clustered lights and the cached sky) is the route to off-screen bounce.
- Headless captures: `screenshot` accepts `KESSEL_SHOT_SIM_S=seconds` to run the simulation
  (idle inputs, 60 Hz, a frame rendered every quarter second) before the capture, so a still
  can follow a few seconds of play. The platoon proof's exterior stills had to run with
  `KESSEL_NO_RT=1` because the exact RT surface pack (physical backend only; the raymarch and
  shader backends never build it) panicked on an instanced transform with a singular linear
  part ("RT batch N instance M ... transform is non-invertible", Canyon v2, about 2 s in).
  The pack now traces such an instance as a 1 mm point at its translation (no ray meets it,
  the raster path draws nothing either way) and prints one `[rt] batch ... degenerate
  transform` line per batch with the matrix, so the writer can be found; see the open items
  for the trigger.
- The trigger, as far as it is known: the platoon branch's headless proof (`KESSEL_PLATOON=2`,
  Canyon v2, physical backend, about 2 s into the run) reported "RT batch 4798 instance 703".
  The same path on this branch (physical backend, Canyon v2 manifest, `KESSEL_SHOT_SIM_S=3`,
  RT on, 18,198 TLAS instances over about 1,100 pages) runs clean and prints no degenerate
  batch, and the batch index there is four times this branch's page count, so the writer is
  most likely one of the platoon branch's own instance pools (ally rigs or their effects). On
  that branch the guard turns the panic into one `[rt] batch` line naming the batch and matrix.
- The veiling glare and its pyramid exist only on the raster-fed backends (the stylized bloom
  shader); the physical backend's bloom has no such pass and could not start on the branch
  between 8cf48cf and this fix.
- The Tiger reads dark in every daylight still because it is painted dark: `tank::tiger_paint_finish`
  sets the charcoal German grey sRGB (72, 74, 78), linear reflectance 0.065-0.076 (the owner's
  2026-09-09 choice; RAL 7021 Panzergrau is about that), against snow at 0.90. At equal
  irradiance the hull returns 7 % of what the snow returns; no lighting pass changes that ratio.
  The historical answer for a December 1942 Tiger is the winter whitewash (Kalkfarbe over the
  grey, worn at edges and running gear), a content decision for the owner, not a renderer one.
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
