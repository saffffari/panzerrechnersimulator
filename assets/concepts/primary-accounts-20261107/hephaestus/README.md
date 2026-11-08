# Hephaestus — primary-account voxel concepts

Two original, editable voxel dioramas built for Panzer on 7 November 2026. The purpose is wordless environmental storytelling through destruction, exposure, exhausted figures, empty supplies and interrupted care. These are concept studies and potential sparse-voxel engine fixtures, not finished gameplay assets or historical reconstructions.

## Models

| Model | Cell size | Grid extent | Occupied extent | Occupied cells | Surface triangles | Semantic nodes | Occupied 16³ chunks |
|---|---:|---:|---:|---:|---:|---:|---:|
| **Once in twenty-four hours — factory cellar ward** | 0.10 m | 16.4 × 14.0 × 7.6 m | 15.4 × 13.1 × 7.0 m | 229,568 | 176,300 | 45 | 198 |
| **No departure — the evacuation system after Pitomnik** | 0.15 m | 28.5 × 24.9 × 8.7 m | 27.0 × 23.4 × 6.75 m | 181,465 | 174,548 | 47 | 174 |

The cellar has a damaged factory shell, separate roof remnants and girders, an exposed ward, a small floor fire, a ration kettle, an empty arrival stretcher and an exit stair toward a second cellar threshold. Warm light has a very limited reach; the building and snow remain cold.

The airfield study places patients in **two separate aid shelters**, with cold stoves, a broken handcart, an unused stretcher, interrupted paths, a communication line and a cratered strip segment. It deliberately leaves the strip empty. This is a small environmental vignette; the narrow visible strip is not an asserted full runway width, survey or map.

Every figure is an anonymous, original block-form proxy. The scene counts are compositional choices, not casualty or patient counts. There is no explicit gore and no named victim is depicted.

## Source provenance and interpretation

### Factory cellar ward — dossier A1-08

**Primary record:** anonymous corporal, 10./Infanterie-Regiment 578, 305th Infantry Division; diary covering 22 November–18 December 1942. The surviving captured translation was received by the 62nd Army political department on 1 January 1943. Stalingrad Museum-Reserve **KP 25224**. [Archival transcription](https://stalingrad1942-1943.ru/node/2724).

The entries for 16 and 18 December describe darkness, incoming wounded, a fire directly on the floor and food only once in twenty-four hours. The 18 December entry describes finding a doctor in a different ruined-house cellar after a lengthy search. These are the attested relationships used here.

The room plan, brickwork, stairs, proportions, light placement, blankets, stretchers and vessels are artistic interpretation. The distant doorway symbolizes a route to another shelter; the record does not place the doctor within sight of this ward. The translation's wartime provenance and anonymous authorship remain part of the source boundary.

### Failed evacuation — dossier A1-07

**Primary record:** chief medical officer, Army Group Don, report dated **21 January 1943**, drawing on reports by Schulze and von Bassewitz before evacuation on 18 January. **NARA T311 R271 001189–001190**. [Archival transcription](https://stalingrad1942-1943.ru/index.php/node/2654).

The report records minus 35°C on the departure night, inadequate food and heating fuel, wounded in unit bunkers and aid posts, and disrupted evacuation after the loss of Pitomnik. Centralized collection of the wounded was described as impossible; the one remaining small airfield was under artillery fire.

This scene is a fictional composite of that failure, not a claim to reconstruct Pitomnik or identify the remaining field. Terrain, craters, shelter designs, wind marker, cart and paths are original staging. No aircraft is included: the dossier's separate aircraft photograph has weaker site attribution. Dispersed shelters preserve the report's distinction from a centralized collection point.

Local research cross-reference: `docs/research/environmental-storytelling-report-source.md`, anchors A1-07 and A1-08. No full testimony, archival photograph, third-party geometry or texture has been copied into these assets. No new third-party asset license is introduced or assigned; all mesh/voxel construction and palettes here are original project work.

## Editable formats

Each model has:

- **`.vox`** — MagicaVoxel 150, one model, editable occupied cells and palette. It does not preserve semantic layers.
- **`.voxels.json`** — canonical editable sparse source: dimensions, metres per cell, seed, source provenance, material families, semantic part names, and `[x,y,z,palette_id,part_index]` cell records.
- **`.glb`** — portable glTF 2.0 mesh for inspection. Exposed faces only, named semantic nodes, rough PBR materials, and a small emissive palette for the cellar fire. Both GLBs are under 11 MB.
- **`.validation.json`** — counts, occupied bounds, per-part voxel counts, static checks, byte sizes and SHA-256 hashes.
- **`.overview.render-receipt.json`** — actual GLB import and CPU render evidence; includes the external preview path.

Source coordinates are **right-handed Z-up**, with the grid corner at `[0,0,0]`; cells occupy `[x,x+1]` lattice intervals. The base starts at Z=0. GLB positions are in metres and **Y-up**, using `cell_size_m × [x,z,-y]`. All GLB node transforms are identity. Palette colors are recorded as sRGB; GLB base-color factors are converted to linear.

Material families are qualitative (`soil`, `cementitious`, `fired_clay`, `ferrous_metal`, `wood_or_combustion_residue`, `frozen_water`, `woven_cloth`, figure proxies, etc.). They do **not** supply calibrated densities, strengths, thermal response or fracture parameters.

## Rebuild and inspect

From `/home/alex/panzer/voxel-runtime`:

```sh
python tools/concepts/hephaestus_primary_accounts.py
blender --background --factory-startup --threads 4 \
  --python tools/concepts/hephaestus_render_concepts.py -- --scene all --samples 24
```

The generator uses Python's standard library only. A measured rebuild of both final models plus validation took **2.22 s**, peaking at **171,656 KiB RSS** on this host. This measures offline asset production, not gameplay speed.

Previews are actual geometry renders imported from the GLBs in Blender 5.1.2, using Cycles **CPU**, four threads and no GPU job. Lighting, an orthographic camera and a studio ground plane are render-only; there is no painted geometry or generated image substitution. The ground plane is not in the GLB or voxel source. All PNGs are written directly into the owner's flat screenshot folder:

`/windows/shop/panzerrechnersystem/captures/screenshots/voxel-runtime-concept-hephaestus-*.png`

The connected Blender MCP server was unavailable, so rendering used a new factory-startup background process and did not load or alter a user scene. The `game-dev` CLI was unavailable, so this is **not** a canonical `game-dev` asset package. No package installation, remote generation or paid service was used.

## Validation and next integration boundary

Validated: GLB structure and byte lengths, buffer bounds, finite positions, all triangle indices, successful Blender import, expected semantic mesh/triangle counts, VOX parsing and exact cell/palette round-trip, voxel bounds, material indices, exposed-face culling and artifact sizes. Preview review corrected unsupported snow/rubble and the figure scale difference between the two lattices.

Not yet validated: runtime import, frame budget, chunk streaming, colliders, destruction connectivity, figure rigging, LOD behavior or historical reconstruction accuracy. GLBs intentionally omit hidden faces across semantic parts; removing a part requires remeshing the sparse source to reveal newly exposed surfaces. The sparse source is the appropriate input for destruction fixtures. Do not benchmark the static GLB and call that a destructible-world benchmark.

Suggested fixture use: stream the recorded 16³ occupied chunks, assign explicit runtime materials separately, remove/modify local cells, remesh changed chunks only, and record collision/mesh-update timings independently from visual fidelity. No engine code or gameplay settings were changed by this asset task.
