# Raster weapon effects

The shader build now routes live weapon presentation through `src/weapon_vfx.rs`.
Damage and projectile collision remain in their existing systems. No VDB, ray query,
ray tracing, particle collision bodies, or external effect assets are used.

The palette is an artistic energy cue shared by both factions: P08 yellow; rifles
amber; MGs orange; cannons below 80 mm red-orange; 80 mm and heavier cannon/rocket
effects hot red.
It does not represent historical tracer compositions. Emissive instance pages use
the renderer's existing own-color glow mask and bloom, which the retired weapon
boxes did not supply.

## Actual weapon hooks

- Player, allied and enemy cannon discharge: `spawn_authoritative_projectile` emits
  the muzzle grains; rendering samples every available live projectile's actual
  position, velocity and traveled distance. Retired shells leave no frozen trail.
- Cannon terminal contact: ordinary AP contact becomes sparks/dust; a resolved HE
  terminal emits a layered fire/smoke burst. Kills, cookoffs and armed Panzerfaust
  detonation use the shared `spawn_blast` presentation hook.
- Coax and hull MG: every `fire_mg_round` event gets a moving tracer, ending at the
  resolved hit/cover point. Existing cadence, heat, ammunition and damage are unchanged.
- P08: `fire_luger` uses the actual visible barrel tip and existing hitscan endpoint.
- Infantry and mixed infantry/tank-MG combat: `resolve_infantry_shot` and
  `mixed_apply_hitscan` use the resolved shot corridor. The older enemy burst path
  also uses its selected hit/miss destination.
- Panzerfaust: the actual bomb state supplies its trail; the launch hook emits both
  forward smoke and backblast grains. The existing fuze and damage rules are retained.

Small-arms damage is still hitscan. Its visible transport is cosmetic, normally
350 m/s for P08 and 760 m/s for rifle/MG. Very close shots have a minimum 55 ms visual
flight so they survive a few native frames; no tail extends past its chosen endpoint.
Cannon flight is never slowed for presentation.

## Fixed work limits

- Eight shared raster instance pages per viewport for all weapon trails and bursts.
- 160 concurrent cosmetic small-arms flights; at most 128 authoritative cannon
  trails submitted, plus the single active Panzerfaust.
- 48 burst emitters. Near explosions use 3,072 fine grains, mid-distance 1,024 and
  far-distance 256. Cannon muzzle clouds use at most 768; small muzzle clouds 48.
- All bursts share a 24,576-grain frame budget, spent nearest-first. Each page is
  preallocated for at most 25,152 instances, including trail reserve. Matrices are
  reused without growth. Burst pools recycle cosmetic slots under saturation.
- Sparks cool through yellow/orange/red stages into matte soot; delayed expansion
  lobes and independent grain expiry replace a single expanding emissive cube.
  Smoke lasts up to 3.3 seconds and receives no emissive shortcut.

These are code limits, not a live FPS guarantee. The five focused CPU tests passed,
including a 240-frame sustained-fire/overlapping-burst stress pass with finite
transforms, unchanged vector capacities and bounded instance counts.

## Render review

An ignored test builds the actual isolated source Tiger and creates five native
640×360 / nearest-scaled stills. Cannon, P08, infantry and MG frames invoke actual
discharge hooks; the explosion/soot frames explicitly stage only the cosmetic
kill/cookoff emitter, without manufacturing a gameplay kill. The receipt records
the per-page submitted counts and this scope.

```bash
KESSEL_RENDERER=shader \
KESSEL_ASSET_ROOT=/home/alex/panzer/tank_simulator \
KESSEL_INTERNAL_RES=640x360 KESSEL_GRAPHICS_PRESET=high \
KESSEL_ENABLE_PIZZA_AUDIO=1 \
cargo test --release --locked --bin panzerrechnersystem \
  weapon_vfx_gpu_source_tiger_review -- --ignored --nocapture
```

The default output prefix is `captures/astra/weapon_vfx_20260913_v1`; set
`KESSEL_WEAPON_VFX_REVIEW_PREFIX` to a fresh flat capture path for another run.
Both host reviews succeeded on an NVIDIA GeForce RTX 4090 at native 640×360.
All five v1 and v2 images and their receipts were inspected. The first review
exposed broken subpixel small-arms trails and overly black exterior smoke. The
correction uses a bounded one-native-pixel trail-width floor and normal sun/ambient
lighting for matte, zero-emission smoke. The five CPU tests passed again afterward.

The final [v2 receipt](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2.json)
records eight shared pages and the same fixed pool limits. The reviewed frames are:

- [Cannon flight](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2_cannon_flight.png):
  continuous orange-red glow follows the actual shell ahead of the muzzle.
- [Barrel smoke](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2_barrel_smoke.png):
  768 matte gray grains remain readable against the tank and studio floor.
- [Explosion](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2_explosion.png):
  3,072 grains form a bright layered core and a broken soot boundary.
- [Cooling explosion](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2_explosion_soot.png):
  3,072 non-emissive grains retain visible shaded facets and loose edges.
- [Small arms](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v2_small_arms.png):
  P08, rifle and MG streaks remain connected and visibly yellow/orange at native resolution.

The final appearance is a stylized expanding particle cloud, not a fluid or VDB
simulation. These diagnostic stills establish appearance and renderer validation;
the motion and bounds tests establish finite, advancing trails. They do not establish
a live battle FPS benchmark. No perceptual audio audition was performed.

The later five-tier palette refinement has its own inspected
[v3 color comparison](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v3_palette.png)
and [v3 receipt](/home/alex/panzer/shader-graphics/captures/astra/weapon_vfx_20260913_v3.json).
It uses the production streak geometry and material pages in a labeled render-only
chart, with no simulated shots or damage. Pistol/rifle/MG shades progress from pale
yellow through amber; the two cannon tiers have a stronger orange-to-red difference
and visible bloom. The receipt records exact RGB/emission values. Its highest-tier
label names the representative 88 mm weapon; code applies that tier to all cannons
at least 80 mm and rockets. The v2 effects review predates this final palette split.
All five focused CPU tests passed after the split, including strictly increasing
red bias across the five weapon tiers; the eight-page budget is unchanged.

Set `KESSEL_WEAPON_VFX_PALETTE_ONLY=1` along with a fresh
`KESSEL_WEAPON_VFX_REVIEW_PREFIX` to render only this comparison with the same
ignored GPU review command.

## VDB source audit

The local asset audit searched `/citadel/3D/library` and found Cloudscapes cloud
VDBs, but no fire, explosion or barrel-smoke VDB source. The official
[JangaFX free VDB page](https://jangafx.com/software/embergen/download/free-vdb-animations)
was checked on 2026-09-13. It lists CC0 assets including Ground Explosion
(661 MB, 200 frames), Gasoline Explosion (751 MB, 131 frames), and Simple Fire
(64.7 MB, 101 frames).

Those online sources were found, not downloaded or imported. The current game
uses procedural raster grains and contains no VDB simulation or VDB-derived asset.
