# Atmosphere pass: darker, moodier, a dark Tiger — 25 September 2026

Alex asked for the game to be "more moody, dark, and atmospheric", with the tank feeling "really
dark". Every change below darkens the image by correcting light transport or by changing a real
emitter, the medium or the time of day, as `CLAUDE.md` requires. There is no post veil, black
crush, vignette, interior-only exposure or view-state energy. Camera exposure stays at 1.25 and is
identical in every station.

## What changed

1. **Environment visibility (the dark tank).** The stylized backend's sky and ground fill used to
   reach every surface, so a buttoned turret, a closed hull and a roofed room were lit as though
   they stood in open snow. `src/sky_visibility.rs` now rasterizes the real scene geometry around
   the camera from 32 equal-area directions: 24 over the sky dome and 8 over the ground
   hemisphere. Each direction is a 384 px orthographic depth tile of one atlas. Surfaces weight
   those tiles by their cosine lobe for diffuse fill and by a roughness-widened mirror lobe for the
   metal environment reflection. Closed armour now blocks the sky; an open hatch, a vision slit or
   a doorway admits exactly what its geometry allows. Terrain is the ground emitter and never an
   occluder. With nothing in the way the result is exactly the previous `hemisphere()`, so open
   ground is unchanged.
2. **Solar-elevation daylight law.** One shared snippet, `shaders/solar_daylight.wgsl`, drives
   everything that depends on the sun's height: sky-dome radiance, diffuse fill, fog in-scatter
   and the direct beam. Every factor equals 1 at the Act I reference sun (31°), so that map keeps
   its authored balance.
   - Sky and fill use the CIE overcast horizontal illuminance fit, E ≈ 0.3 + 21·sin h klx.
   - The beam uses Kasten–Young air mass with per-channel Meinel transmittance, so a low sun
     loses blue first.
   - The beam's cloud transmittance is the authored value raised to the slant-path ratio
     (Beer–Lambert through the layer), so a low sun crosses more cloud.
3. **Fog meets the sky.** World-fog in-scatter was a constant (~0.52) brighter than the sky it
   fades into, which bleached every distance to white. It is now the horizon sky radiance
   (`horizon_sky_radiance`), so aerial perspective converges on the real horizon.
4. **Gun haze inside armour.** Cabin haze, breech gas and muzzle plumes were lit by a constant
   0.31 grey ambient and an unshadowed sun term. They now use the same visibility atlas (fog
   pipeline group 1): horizon-sky in-scatter weighted by visible sky and ground, and a beam gated
   by the sky lobe about the sun. Firing inside a closed turret fills it with lamp-lit haze, not a
   grey veil.
5. **Snow flakes in the scene's light.** Flakes were drawn after tone mapping at a fixed display
   white of 0.80, which read as stars against a dark sky. Each flake is now
   `ACES(exposure × 0.9 × ((sky + ground)/2 + sun/4))` from the same stylized environment
   (`stylized_environment` in `renderer.rs` mirrors the WGSL).
6. **Winter courtyard lighting (ignored asset file).**
   - Sun: 22° → **7.5°**, twilight 0 → **0.15**. 22° is unreachable in a Stalingrad December
     (48.7° N; noon ≈ 18°); 7.5° is about 15:00 local solar time during Wintergewitter.
   - Fog: base density 0.0018 → **0.0035 m⁻¹**, about 850 m visibility. Weather fronts still
     thin it to about 0.0017 in lulls and thicken it to about 0.006 in fronts.
   - Lamps: the two floodlit fixtures tuned for 22° daylight (`winter_entrance` 26 → 11,
     `winter_courtyard_gate` 58 → 22, range 12 → 9 m). The four small house lamps keep their
     authored values.

   `manifest_edits.json` is gitignored. Apply these values to another checkout with
   `python3 tools/apply_atmosphere_mood_20260925.py <path/to/manifest_edits.json>`, which is
   idempotent and writes a backup.

## Evidence

Headless stills, same cameras before and after (`captures/`, ignored):

- `mood_20260925_compare_exterior.png`, `mood_20260925_compare_interior.png` (side by side).
- Measured pixels, "far" view:
  - Sky: (209,213,214) → (90,98,102).
  - Lamp-lit snow: warm, as a 2600 K source on snow should be.
  - Everything outside the lamp pools falls to cold blue-grey.
- Buttoned commander: steel falls to near-black. The vision blocks show the dim exterior instead
  of white, and the amber dome and dial lamps key the breech. The driver's station is black except
  for the visor slit.

Tests:

- Release binary suite: **1,122 passed, 0 failed, 27 ignored**. That is the earlier 1,114 plus 8
  new: atlas geometry, direction quadrature, snap reuse, daylight law and the particle mirror.
- 23 integration crates pass, including the raster shader/ABI contract. Its binding set now
  includes group 2 bindings 6 and 7.
- Four integration tests fail identically on the untouched snapshot `638fce1`; verified by
  running them there:
  - `display_quantization` (crush source string);
  - `live_fx_contract` (`MAX_LAYERS`);
  - `physical_truth_contract` (`glass_pipe`);
  - `protective_glass_transport` (a missing port GLB).
- `foot_ai_contract` and `combat_training` do not compile before or after this pass
  (`crate::resource` in `src/foot_ai.rs`).

Performance: one 240-frame `winter-bench dense` run per build on the RTX 4090, 960×540, with no
other game running.

| Build | Mean frame | p95 frame |
|---|---|---|
| Before | 11.34 ms | 14.90 ms |
| After | 12.02 ms | 15.80 ms |

Of the added time, GPU queue wait is +0.44 ms and CPU submission +0.18 ms. This is indicative, not
a sealed profile. The atlas redraws only when its inputs change:

- The static instanced scenery is cached per footprint cell.
- Moving meshes are drawn over a copy of that cache.
- A parked tank costs nothing after the first frame.
- Inside the tank one rebuild draws about 173 meshes (one 32-instance draw each) plus at most 11
  static batches.

## Knobs

| What | Where |
|---|---|
| Overall brightness | `environment_lighting.camera_exposure` in the courtyard manifest (1.25). Changes every station equally. |
| Time of day | `sun_elevation_deg`, `twilight`. |
| Haze | `fog_density`. |
| Cabin | The dome lamp (`interior_rig.rs`, 0.168 effective) is the only practical key light in a buttoned turret. Brighten it there, not with fill. |

## Not done

- The footprint is ±4.5 m in a vehicle and ±12 m on foot. Beyond it, and for occluders more than
  10 m past it, the environment counts as open.
- The haze beam uses the nearest sky directions (cos⁸ lobe), not the sun cascade.
- No interreflection. The inside of the hull receives no light bounced off its own lit steel.
- Needs a human playthrough to judge readability at the ammunition rack and breech. The captures
  show them keyed only by the amber lamps.
