# Free Tiger And Crew Kit — Panzertruppe derivative

- Original title: **Free Tiger And Crew Kit**
- Creator: **AdrielCZ** (`adrielcz`)
- Source: <https://sketchfab.com/3d-models/free-tiger-and-crew-kit-6290669ab2cb49599f1606507626af39>
- Retrieved: 2026-08-30
- License: **Creative Commons Attribution 4.0 International** (`CC-BY-4.0`)
- License: <https://creativecommons.org/licenses/by/4.0/>
- Original archive SHA-256:
  `ecc46d04c5016f07b54838e65b56705486917f2ccda2fce5708900e406183483`

The original archive remains in the private local/Hal asset warehouse and is not committed. It
contains a 39-bone skinned Panzertruppe, five face variants, separate left/right arm meshes,
field glasses, a cap, holster, ammunition pouch, P38, MP40 and MG34, plus an unrelated Tiger model.
The crew source has no animation clips.

The archive assigns several different PNGs the same member name. Ordinary extraction destroys face
and material variants. `tools/extract_tiger_crew_kit.py` therefore accepts only the exact reviewed
archive hash, maps every duplicate by CRC, preserves all 50 source members, and writes an extraction
receipt. Do not replace it with an ordinary `unzip` step.

## Kessel modifications

The Blender bake deliberately excludes the bundled Tiger. It retains a rigged authoring master,
uses one of the five supplied faces for each permanent crew identity, poses the existing skin for
the physical Tiger stations, bakes evaluated static meshes for the current non-skeletal GLB loader,
and exports the included handheld field glasses as an independent physical item. Runtime meshes are
normalized to 1.53 m, glTF Y-up, face +Z, and floor at local Y=0. A unified material atlas prevents
the engine's current single-material loader from discarding the body, arms or face texture.

Rebuild from the repository root after obtaining the reviewed archive through the creator's normal
Sketchfab download flow:

```sh
python3 tools/extract_tiger_crew_kit.py \
  /path/to/free-tiger-and-crew-kit.zip /path/to/reviewed-extraction

blender --background --factory-startup \
  --python tools/bake_tiger_crew_kit.py -- \
  /path/to/reviewed-extraction \
  assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4
```

The generated receipt is the authority for the exact Blender version, topology, bounds, atlas,
role/face assignment, output hashes, and which optional animation-frame sets were produced.

The current game also attaches the preserved nisu CC0 steel helmet to each identity, restoring the
earlier reference silhouette. This is a separate source-textured mesh; none of the 15 Panzertruppe
bakes or their uniform/face atlases is modified. See
`../german-ww2-soldier-cc0/SOURCE.md` and `tools/fit_crew_steel_helmet.py` for that derivative's
source, registration and runtime limitations. The AdrielCZ source cap remains preserved in its
original archive and was not part of the original four-object crew bake.
