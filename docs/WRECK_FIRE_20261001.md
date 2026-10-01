# T-34 wreck fire correction

The Canyon films use the custom renderer's **Shader raster backend**, whose surfaces use
physically based material response. The High/4090 graphics preset is independent of that choice.
The previous wreck effect was not consistently coupled to those materials: it added an
independently calibrated point source with no raster wreck/body shadow.

## Change

The exterior fire source now comes from the same admitted cube cells that are drawn: decoded
linear color, radiance, face area, centroid and support. No extra damage/flicker light multiplier
remains. Exterior surface radiance is one quarter of the prior material table, so the smaller
illumination footprint and reduced bloom follow the same emission change.

Flames attach to the rear engine deck by intersecting the loaded hull triangles. They formerly
started 0.38 m above ground, largely inside the vehicle. Up to 768 small cube candidates form
three rising, bending tongues; duplicate lattice positions do not multiply energy. Smoke starts
above the enlarged plume. The cabin fire material and cabin particle motion are unchanged.

Eight reserved world-fire shadow slots now accompany the existing eight cabin fixture slots.
Actual tank, terrain and wall geometry supply the source cubemaps. In the fixture, the hull and
ruined walls now interrupt the orange illumination instead of letting it pass through them.

## Native evidence

All artifacts are in flat `captures/`:

- `wreck_fire_before_20261001`: prior gameplay effect, with only the new capture harness added.
- `wreck_fire_review_20261001`: final effect, identical staged wreck state and first three cameras.
- `wreck_fire_comparison_20261001.mp4`: labeled 12-second side-by-side of those native films.
- `wreck_fire_gameplay_20261001`: two 12-second first-person excerpts from seed 137's actual run.
- `wreck_fire_review_20261001.html`: review page, with original/revised stills and a deck close-up.

The staged wreck has ignition load 0.75, fuel 0.99157536 and strength 0.7418552 at 8.00005 seconds.
Its old source intensity was **18.7541**; the new aggregate is **0.495976**, a **97.355% reduction**
in renderer intensity units. This is not a measured wattage or a percentage reduction in all
screen pixels. Auto exposure settles normally for each camera, and no image grading is applied.
The higher-level capture receipt asserts identical state, camera positions, target and resolution.
The old and new frames are visually inspected, including the geometry shadow on the snow.

The fixture explicitly stages one destroyed T-34, retains its turret and suppresses secondary
cook-off. It is not a gameplay kill. Its observer film is silent; the separate first-person
attempt uses native audio. The two intermediate tuning candidates remain local, outside delivery.
The fresh seed-137 attempt's final simulation JSON exactly equals the prior extended recording:
three T-34s and two AT guns destroyed, five reloads, about 65% route progress, then the same
industrial obstruction/navigation stall with the Tiger alive. The lighting change does not
manufacture a successful run. The delivered first-person film contains 720 frames and 24 seconds
of captured native audio; no audio audition is claimed.

Reproduction (run from this checkout with a fresh output prefix):

```bash
KESSEL_RENDERER=shader KESSEL_INTERNAL_RES=640x360 KESSEL_GRAPHICS_PRESET=high \
KESSEL_CANYON_MANIFEST="$PWD/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json" \
XDG_RUNTIME_DIR=/run/user/1000 \
target/release/panzerrechnersystem wreck-fire-review captures/wreck_fire_new_review
```

## Checks and limits

Release build and native RTX 4090 captures pass. Focused CPU tests: **75 fire checks** pass
(3 GPU fixtures ignored), **11 shadow checks** pass, and **28 raster/particle/bloom checks** pass.
These include cell-energy/extinction, exact hull attachment, deterministic parcel state, existing
cabin energy, cube projection/cache behavior and the expanded 272-byte CPU/WGSL shadow ABI.
`git diff --check` and the packaging script's Python compilation pass. This is scoped validation,
not a new claim that every repository test or human playtest has passed.

The flame motion, spectrum and fuel law are authored, not a combustion simulation. One aggregate
isotropic source approximates each extended fire; there is no flame self-absorption. The raster
source cubemaps have a fixed 8 m caster range and sample the centroid, not the full luminous
volume. The extra eight slots add 96 MiB to the raster shadow caches. Other transient raster
sources remain separate existing debt. See [PHYSICAL_TRUTH_AUDIT.md](PHYSICAL_TRUTH_AUDIT.md).

The Pi delivery destination is
`/home/saffari/Downloads/Panzerrechnersystem_Fire_Review_2026-10-01/`.
The content allowlist and SHA-256 roster accompany delivery; no asset library or game binary is sent.
Four videos fully decode, all nine PNGs decode, all JSON records parse, and all review-page
local links resolve. The page has static link validation rather than an interactive browser test.
