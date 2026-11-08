# Two places in Pozdnyakova's testimony

Original, editable voxel concept models created locally for Panzer on 7 November 2026. These are interpretation studies for environmental storytelling and destruction fixtures, not measured reconstructions or integrated game assets.

| Concept | Files | Physical envelope | Voxel pitch |
|---|---|---|---|
| The Window — Solnechnaya | `the-window-solnechnaya/model.vox`, `model.voxels.json`, `model.glb`, `model.blend` | 12 × 10 × 7.3 m | 0.1 m |
| Water Under Fire | `water-under-fire/model.vox`, `model.voxels.json`, `model.glb`, `model.blend` | 14 × 11 × 4 m | 0.1 m |

The first scene links a burned two-storey shell, escape window, cellar bedding, interrupted domestic objects, and partially collapsed courtyard trench. The second links a shelter edge, exposed collection path, containers, railway cistern, and dark water spill. Its anonymous crouched figure provides scale and indicates water collection; it is not a portrait or a documented pose. No named death is posed in either scene.

## Source and interpretation

The primary testimony is Agrafena Petrovna Pozdnyakova's interview with the Commission on the History of the Great Patriotic War, Stalingrad, **14 March 1943**. The Russian publication gives the archive reference **NA IRI RAN, F. 2, R. III, Op. 5, D. 22, fols. 66–71**. Consult the [Russian edited extract](https://daily.afisha.ru/archive/vozduh/books/stalingradskaya-bitva-glazami-ee-ochevidcev/) and [English edited edition, chapter 18](https://erenow.org/ww/stalingrad-city-that-defeated-third-reich/18.php). This work used published transcriptions, not the original manuscript scan. The Commission's wartime institutional context remains relevant.

**Attested:** The family sheltered in a small two-storey house on Solnechnaya Street. The block burned in September; with doors closed, children were passed through a window and adults escaped after them. They abandoned possessions, then occupied courtyard trenches. Subsequent shelling killed her husband and daughter and buried survivors. Separately, she describes collecting railway-tank water under fire, sometimes contaminated with fuel oil; some collectors did not return.

**Interpreted:** All dimensions, wall arrangement, terrain, roof/floor remnants, furniture, bucket design, clothing, footprints, damage pattern, cistern type, colors and studio light. No source establishes the precise wagon or building shape shown. The two studies are not asserted to be adjacent places or simultaneous events. The September house scene uses ash and bare earth rather than a generic winter blanket. Nothing from historical photographs was copied into the geometry or materials.

Local source index: `docs/research/environmental-storytelling-report-source.md`, **A1-01 — Burned cellar household**, lines 82–87, and **A1-02 — Oil-water railway siding**, lines 89–94. Those report entries guided discovery; the interview itself supplies the historical evidence. The `.vox` colors are original, with no external textures, scans or generated service output.

## Editable data contract

`model.voxels.json` uses schema `panzer.original-concept-voxels.v1`:

- `voxel_size_m`: metres per grid cell, currently 0.1.
- `origin_m`: lower corner of cell `[0,0,0]`, currently `[0,0,0]`.
- Coordinate system: right-handed **Z-up**, X/Y ground plane. Cells occupy `[origin + index * pitch, origin + (index + 1) * pitch)`; centers add half a cell on each axis.
- `dimensions_cells`: enclosing integer X/Y/Z grid dimensions.
- `palette`: zero-based visual material IDs, each `{name, rgba:[r,g,b,a]}`. RGB values are sRGB bytes. They classify appearance only; they are **not** calibrated density, strength, temperature or failure properties.
- `parts`: zero-based semantic part names.
- `voxels`: unique integer rows `[x, y, z, palette_index, part_index]`. Empty cells are omitted. This is occupied volume, including enclosed voxels, suitable for independently defined sparse-grid fixtures.

`model.vox` is MagicaVoxel version 150, with identical cells and palette, colors indexed from 1 as that format requires. It does not carry the semantic part IDs; preserve JSON for that information. `.vox` has no metric scale metadata; use this pitch explicitly.

`model.glb` uses metres and glTF **Y-up**: source `[x,y,z]` becomes `[x,z,-y]`. It contains named mesh parts and only exposed cell faces; enclosed volume remains in the voxel sources. RGB palette values are converted from sRGB to linear base-color factors for glTF/Blender. `model.blend` retains the mesh parts, materials and disposable studio scene. It was produced in a separate factory-startup Blender process; the user's interactive scene was untouched.

## Rebuild and verification

Run from the repository root:

```bash
blender --background --factory-startup --threads 8 --python tools/concepts/athena_build_primary_accounts.py
python tools/concepts/athena_validate_primary_accounts.py
blender --background --factory-startup --threads 4 --python tools/concepts/athena_import_check.py
```

Generator seed: `19430917`. Blender version used: **5.1.2**. Previews: Cycles CPU, 24 samples, 1600×1200, orthographic, AgX; final filenames end `20261107-r2.png` in `/windows/shop/panzerrechnersystem/captures/screenshots/`. The earlier unsuffixed previews are retained as first-pass inspection images.

Each model's `validation.json` records counts, SHA-256 hashes, file sizes and preview paths. `readback-validation.json` independently decodes actual JSON, `.vox` and GLB bytes, checks cells/palette, coordinate conversion, normals, indices, dimensions and triangle counts. There are no external binary dependencies in the GLBs.

A separate clean Blender GLB import also passed for both models, preserving the eight named parts and exact metric extents within floating-point tolerance. Results are in `blender-import-validation.json`. House: 184,107 occupied cells and 126,060 surface triangles; cistern: 87,267 occupied cells and 125,252 triangles. GLBs are 7.03 MB and 6.79 MB respectively.

These are **raw validated concept assets**, not canonical `game-dev` packages: that CLI was absent, and no installation or remote generator was used. Native engine import, collision, FEA material calibration, animation, LODs and frame-rate budgets remain untested. The surface export is intentionally simple and should not be described as game-ready performance.

## Provenance and rights

All voxel geometry, palette, materials and generator code are original local work for this project. No third-party models, textures, photographs or text extracts are embedded. Historical references are cited rather than republished. Distribution licensing follows the owner's project policy; this folder does not grant a new public license or relicense the historical publications.
