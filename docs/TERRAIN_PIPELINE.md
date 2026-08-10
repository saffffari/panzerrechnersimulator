# Terrain Pipeline — Gaea → Kessel

*The tool-agnostic hi-res heightmap path (added 2026-07-07). Any tool that writes a 16-bit
greyscale PNG + a four-number sidecar feeds the engine; Gaea is the intended authoring tool.*

## The contract (what the engine reads)

Drop two files in `assets/terrain/act<N>/`:

```
assets/terrain/act1/
  heightmap.png     # 16-bit greyscale, SQUARE (e.g. 4096×4096). Pixel 0 = elev_min, 65535 = elev_max.
  heightmap.json    # georeferencing sidecar (below)
  ground.png        # OPTIONAL drape: georeferenced RGB, 0..1 UV across the terrain (named in sidecar)
```

`heightmap.json`:
```json
{
  "meters_per_side": 8192.0,   REQUIRED — full terrain width in world metres
  "elev_min": 0.0,             elevation (m) at pixel value 0   (default 0)
  "elev_max": 140.0,           REQUIRED — elevation (m) at pixel value 65535
  "drape": "ground.png"        OPTIONAL — georeferenced ground texture; omit for a snow tint
}
```

**If `heightmap.png` exists for an act, it WINS** over the DEM window and the worldgen `heights.npy`
— it's the authored-terrain path. Absent → the old paths still work unchanged. So you can adopt
it act-by-act with zero risk to the others.

### How the engine uses it (already built, `src/terrain.rs::load_heightmap`)

- **Collision / `height_at`** keeps the heightmap's NATIVE resolution — a 4096² export grounds the
  tank on every 2 m post. Bilinear sampled, cheap (array lookup), no LOD needed.
- **Render mesh** is stride-decimated to ≤512² verts (one draw). This is the load-bearing decoupling:
  detailed terrain for *gameplay*, cheap terrain for *rendering*. (True chunked distance-LOD is the
  follow-up when we want the render mesh itself to carry more detail near the camera.)
- Re-zeroed on the grid centre so the spawn sits near y=0 (matches the DEM-window convention).

## The Gaea graph (recommended shape)

**Authenticity rule (the whole project's philosophy): real macro, invented micro.**

1. **Base = the real DEM.** Import `assets/terrain/stalingrad_100km_height16.png` (or the act's real
   DEM crop) as the base heightfield. This keeps the Volga bluffs / Seelow scarp historically true.
   Gaea `File` / `Heightfield Import` node → `Resample` to working res.
2. **Erode on top.** `Erosion` / `Wizard` for fluvial detail — this is where **balkas** (the steppe
   gully systems the spec calls out) come from; simulated flow reads far better than scripted noise.
   Keep erosion strength moderate so the real relief still dominates.
3. **Undulation.** A gentle `Perlin`/`Ridge` combined at low amplitude for the 1–50 m wavelengths the
   DEM lacks (hull-down, defilade — the gameplay scales). This is the single biggest gameplay win.
4. **Export**: `Export` node → 16-bit PNG (Raw16/PNG16), square, 4k or 8k. Set the sidecar's
   `elev_min`/`elev_max` to Gaea's reported height range for that graph (Gaea shows real-world
   metres if you set the terrain's real-world scale; otherwise pick a plausible relief, e.g. the
   DEM's own range for that window).

### Masks (next step, not yet consumed)

Gaea also exports **Flow / Wear / Deposition / Slope** masks. These are the connective tissue to
the rest of the world and are worth exporting now even though the engine doesn't read them yet:
- `make_ground_tex.py` → snow pools in flow lines, earth shows on wear/high-slope.
- `gen_world.py` → scatter/foliage density by moisture; road/embankment plausibility by slope.
Export them as greyscale PNGs alongside; wiring is a follow-up task (flagged in docs/SPEC.md Part III).

## The DIVISION OF LABOR (critical — from the design discussion)

- **Gaea owns geology**: relief, erosion, balkas, undulation, masks. Slow-changing, artistic.
- **`gen_world.py` owns gameplay stamps applied ON TOP of the heightmap**: **craters**
  (position/radius/age recorded in the manifest so the encounter director, loot, and the paper
  map can all SEE them), road/embankment cuts, cleared zones. Deterministic, seeded, machine-known.

If a crater lives only in Gaea's heightmap, the game can't reason about it. Geology in Gaea;
gameplay features stamped in the generator. That hybrid is the plan.

## Practical notes

- **Gaea is Windows-only** (.NET + DX11; unreliable under Wine). Treat it as a bake step like the
  Blender pass — author on Windows, drop the PNGs into the Linux checkout, iterate in-engine.
- **License**: Community edition export cap (~1k) is too low; **Indie** (4k/8k) is the tier. At 4k
  over an 8 km act ≈ 2 m posts — the right gameplay resolution.
- **Headless rebuilds**: `Gaea.Build <graph>.terrain` regenerates without the GUI once the graph is
  authored, so the pipeline can stay scripted after the creative pass.

## Verify loop

```
# after dropping heightmap.png + .json into assets/terrain/act1/
cargo run -- screenshot out.png <cam...>     # external recon of the new ground
cargo run -- edit 1                           # fly it in the worldbuilder
cargo run                                     # play it
```
