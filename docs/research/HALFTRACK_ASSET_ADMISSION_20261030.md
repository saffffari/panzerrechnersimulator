# Halftrack transport prototype: asset admission review

30 October 2026. This records downloaded bytes and a CPU Blender conversion. It does not claim
live vehicle integration, a visual approval, or a historical Tiger-recovery capability.

## Selected source

- **Armoured Half-Track**, Polyfork / Lucas Martinic.
- [Publisher page](https://polyfork.dev/asset/armoured-half-track-3deded).
- [Public metadata](https://polyfork.dev/api/assets/armoured-half-track-3deded).
- [Publisher license](https://polyfork.dev/licensing), reviewed 30 October 2026.
- The free low-poly edition permits modification and commercial game use, including shipping
  inside a game. Attribution is optional. Standalone asset redistribution and use in a competing
  commercial asset generator are excluded. The paid HD edition was not downloaded.
- Downloaded from the API-declared, unauthenticated GLB URL. No account, credential or payment used.

The model is a generic, approximately six-metre armored personnel carrier. It is **not an
identified Sd.Kfz.9 FAMO** and is unsuitable as evidence for Tiger towing capacity. The publisher's
9,500 kg mass is explicitly an estimate. Use it to prototype transport, boarding, cargo and moving
parts; establish a separately sourced heavy-recovery vehicle specification before towing a Tiger.

## Private files and verified result

Root: `assets/vehicles/polyfork_halftrack_20261030/` (confirmed Git-ignored).

| File | Purpose |
|---|---|
| `source/armoured-half-track-3deded.glb` | Unmodified publisher source |
| `source/publisher_metadata.json` | Full retrieved metadata and download provenance |
| `source/publisher_license.html` | Retrieved license at acquisition |
| `admission.json` | Source/license/metadata hashes and intended-use limitations |
| `baked/halftrack_transport_prototype.blend` | Metric Blender master retaining part hierarchy |
| `baked/halftrack_transport_prototype.glb` | Self-contained textured runtime candidate |
| `baked/inspection.json` | Blender version, dimensions, topology, nodes and output hashes |

Source SHA-256: `c9bf60a49e6127374d96e60ea5201aa25b8b501be203bbfc86e70b1673566c90`.
Runtime candidate SHA-256: `9a8ba1ed19f1e025c2203c2b31436fd15145e66ce1e024a7c08958948435557f`.

Both versions have 2,652 triangles, six mesh objects and fourteen scene nodes. The measured bounds
are approximately **2.30 × 2.55 × 6.04 m**, with the ground at Y=0. Six authored moving pivots survive:
`steer-fl`, `steer-fr`, `wheel-fl`, `wheel-fr`, `sprockets`, and `idlers`. Tools remain a named subtree.

The engine loader does not read glTF vertex colors. The conversion therefore maps the eight
face-constant source colors onto one padded 128×16 embedded albedo texture and one material.
Linear glTF colors are explicitly converted to sRGB PNG values; all eight resulting RGB swatches
match the source palette. The first conversion encoded linear values directly and was rejected;
its files are preserved under `baked_linear_palette_rejected/`, outside the runtime asset path.
Topology, metric scale and object hierarchy remain intact. No external textures are required.
The source GLB is preserved unchanged; the resulting Blender file is independently editable.

`tools/import_halftrack_prototype.py` uses the existing `runtime_asset_cook.parse_glb` inspector.
It refuses destination overwrites, external GLB dependencies, changed reviewed source bytes,
missing pivots, out-of-range dimensions and an unexpected topology count. Blender 5.1.2 completed
the import, texture conversion, master save and GLB export without a render or game launch.

The optional game-dev CLI is absent. Its canonical-package/admission workflow was not run; this
was the explicitly authorized existing-project import workflow. Do not describe the receipt as a
Game Development Studio admission lock or a successful engine render.

## Alternatives checked

- No matching halftrack, FAMO, Hanomag or Sd.Kfz filename was found in the documented local
  `C:/shop/ts_assets` collection or current project assets.
- [Free3D SdKfz 251](https://free3d.com/3d-model/sdkfz-251-ausf-armored-car-516679.html):
  Personal Use license; not admitted.
- [TurboSquid FAMO cargo model](https://www.turbosquid.com/3d-models/sdkfz9-famo-cargo-version-2449832):
  paid and marked editorial; not purchased or downloaded.
- [OpenGameArt Halftrack](https://opengameart.org/content/halftrack): original-author CC-BY option,
  but modern/science-fiction styling and no materials or UVs; not selected.
- Known game-ripped Sketchfab uploads and noncommercial printable models were excluded.

## Remaining integration work

The optional `src/halftrack.rs` adapter partitions the original asset into six textured pages:
hull, two front wheel assemblies, paired sprockets, paired idlers and tools. It preserves the
source frame (+Y up, +Z forward), reads all six authored pivot transforms including ancestors,
and composes front steering with the nested wheel rotation. Wheel spin is an externally supplied
pose, not evidence of a driving simulation. The rear pairs share source objects and therefore
cannot rotate left/right independently. Track belts remain part of the static hull geometry.

`src/halftrack_capture.rs` offers CPU loader inspection and an optional windowless renderer
review. It loads the private asset before allocating a GPU, constructs neither Game nor Audio,
and captures four external views with optional cab/cargo detail. It uses the standard renderer
and its default sky, with a disclosed neutral floor. Its receipt records transforms, bounds,
renderer, camera positions and limitations. No new model dependency is added to ordinary Game
startup. Both commands refuse existing output names. These are review tools, not a playable
recovery mode; source cab views do not establish traversable interiors or functional seats.

Still required:

1. Compile and run the optional loader/renderer reviews, then inspect the actual captures.
2. Define source-derived passenger positions, cargo bounds, boarding contacts and collision.
3. Add the driving/fuel/damage model and role assignment without inventing a heavy towing rating.
4. Add the runtime path to the private package closure only when a live game feature uses it.
5. Keep source, Blender masters, textures, package receipts and generated builds off GitHub.
