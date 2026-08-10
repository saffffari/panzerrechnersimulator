# Texel Splatting — prototype status & handoff

*Branch: `texel_splatting`. Started 2026-07-07 (overnight). Isolated from the shipped crush
renderer — `main` is untouched. This is the A/B prototype to decide whether texel splatting becomes
Kessel's renderer.*

## TL;DR — it works, and the core thesis is confirmed

`cargo run -- texel assets/tanks/tiger/baked/tiger_hull.glb <out_prefix>` renders a recognizable,
handsome texel-splatted Tiger and dollies from 22 m in to 4.5 m. **The "get close and see new
pixels" hypothesis is validated**: because the probe grid-snaps and re-captures as the camera
approaches, the Tiger unfolds progressively finer stable detail (cupola, hatches, periscope
housings, bolt lines, tools) — detail the fixed-resolution crush pipeline can never show at any
distance. The output is exactly the "3D pixel art / path traced impressionism" north star.

Reference scratch renders (this run): `scratchpad/texel_{0..3}.png` (far→near dolly).

## How to run

```
cargo run -- texel <glb> [out_prefix]        # → out_prefix_{0..3}.png, a far→near dolly
KESSEL_TEXEL_LIFT=4.5 ...                     # albedo lift for dark-PSX bakes (default 4.5)
KESSEL_TEXEL_DEBUG=1 ...                      # dump the cube G-buffer (albedo + normal strips)
```
Any GLB works (it's also the asset-preview bench in texel space). The crush A/B for the same asset:
`KESSEL_GARAGE_MODEL=<glb> cargo run -- garage out.png`.

## Architecture (src/texelsplat.rs + shaders/texel_*.wgsl)

A project-within-the-project: reuses ONLY `gltf_load`, the device setup (`crate::request_device`),
and PNG readback. The render is 100% fresh; it calls nothing in `renderer.rs`/`game.rs`.

1. **CAPTURE** (`texel_capture.wgsl`): rasterize the scene into a 6-face probe cubemap from a
   **grid-snapped** world origin near the camera (GRID = 2 m). Per texel stores **world position**
   (xyz + valid flag), world normal, albedo, and a per-draw object id. 256² per face.
   - *Key decision:* storing world **position** (not the reference's radial + cube direction)
     sidesteps the entire capture↔reconstruct convention-matching problem. The splat reads the exact
     surface point back — there is no cube parameterization to keep in sync. (The first attempt used
     radial+`texel_dir` and the tank scattered to nothing because glam's projection didn't invert
     `texel_dir`. World-position capture fixed it outright.)
2. **SHADE** (`texel_shade.wgsl`, compute): one invocation per cube texel. Sun diffuse + cold sky
   ambient, OKLab lightness posterize (24 bands, verbatim OKLab port), id/normal-discontinuity
   outlines. Camera-independent → everything it produces is perspective-stable.
3. **SPLAT** (`texel_splat.wgsl`): draw every valid texel as a camera-facing world quad centred on
   its stored position, sized to its world footprint (≈ dist × 2/size) × EXPANSION, depth-tested.
   Brute-force over all 6·256² texels; sky texels collapse offscreen in the VS.

The **grid snap** is the whole trick for progressive detail + stability: within a grid cell the
probe is fixed, so texels are world-anchored and don't shimmer; crossing a cell re-captures closer,
so detail sharpens as you approach.

## Known artifacts / limitations (all expected, all milestone-2)

- **White grazing-edge texels** along silhouettes (visible on the Tiger's right side). These are
  silhouette/disocclusion texels + quad over-expansion catching the bright ground/sky behind. The
  reference solves this with its **edge-mask** pass (per-texel neighbour-depth discontinuity →
  anisotropic expansion) — not yet ported.
- **Single fixed probe per frame.** No eye/grid/prev three-probe crossfade, so fast lateral camera
  motion within a cell is fine but crossing cells would pop (no Bayer dither blend yet).
- **No ray-traced shadows/GI.** The shade stage is where they slot in (the engine already has an RT
  BVH path in `rt.rs`); currently just sun diffuse + flat ambient. This is the biggest *beauty*
  upgrade available and the reason the pivot is worth it.
- **Disocclusion.** A single probe can't see surfaces hidden from its origin (the far side of the
  tank, interiors, newly-exposed destruction). Denser/adaptive probes are the fix (the research
  report's #1 risk item).
- Brute-force splat (6·256² instances) — fine on the 4090; a visibility-culled indirect draw is the
  perf path if needed.

## Next steps, in priority order (beauty first)

1. **Edge-mask silhouette expansion** — kills the white grazing texels; small, high-visual-payoff.
2. **Ray-traced sun shadows** in the shade stage (reuse `rt.rs`'s BVH) — the single biggest beauty
   jump; turns flat-lit texels into formed, grounded ones (contact shadow under the hull, self-shadow
   in the recesses).
3. **A real scene, not one mesh + ground** — feed it the garage (Tiger + targets + terrain) so the
   world-scale question (terrain, multiple objects, 500 m sightlines) gets tested.
4. **Multi-probe / eye-probe crossfade** — for camera motion; needed before it can drive live.
5. **Live path** — wire it behind a renderer toggle so `cargo run` can A/B crush vs texel in-window.
   Only after 1–3 make the still image unambiguously better.

## The decision this unblocks

The still images already answer the aesthetic question: **yes, this is the look, and yes, it
resolves detail the crush can't.** The open question is whether the harder cases (world scale, fast
motion, destruction, interiors — from the deep-research risk list) hold up. Steps 3–4 above are the
gates. If they do, texel splatting becomes the renderer and BMS-caliber assets + Gaea terrain get
authored against a renderer that won't waste them.
