# Third-party asset release policy

This file records the evidence and packaging policy for third-party runtime assets. It is an
engineering release gate, not a substitute for the original license agreement or legal advice.

## OpenGameArt CC0 crew character

Kessel includes **Rigged Lowpoly WW2 Soldier** by **nisu**, downloaded from
<https://opengameart.org/content/rigged-lowpoly-ww2-soldier>. The source page designates the asset
as **CC0 1.0 Universal**. The original FBX, texture, complete CC0 legal text, source URL, retrieval
date, hashes, conversion receipts, preserved rigged GLB, and deformation-applied runtime poses are
tracked together under
`assets/flavor/Characters/german-ww2-soldier-cc0/`.

CC0 permits copying, modification, and redistribution without attribution. Kessel nevertheless
credits nisu and retains provenance. Runtime packages may include the reviewed self-contained GLBs;
the FBX and separate source texture are reproducibility inputs and must not be copied into the
cooked runtime model tree.

## Sketchfab CC BY 4.0 Soviet infantry character

Kessel's selected Soviet infantry source is **WW2 Soviet Soldier** by **Michael Jones**
(`mikhailgolubkov09`):
<https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931>.
The source page designates it **Creative Commons Attribution 4.0 International**
([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)). Kessel's adaptations add skin
weights derived from the existing 49-bone donor rig, a generated low-poly rifle, authored motion
poses, skeletal action data, and deformation-applied static runtime frames.

The authenticated Sketchfab download must be supplied by an operator; repository tooling must not
bypass login or infer that arbitrary geometry is this model. `tools/bake_soviet_infantry.py`
preserves the supplied GLB bytes unchanged at
`assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/source/ww2_soviet_soldier_original.glb`,
records its actual byte count and SHA-256, and writes the attribution, license notice, donor/output
hashes, Blender version, rigging method, and runtime-pose receipt beside it. Blender may repack the
embedded atlas in derived GLBs; the unchanged preserved source remains the provenance authority.

Release policy:

- Track only this exact character family under
  `assets/flavor/Characters/soviet-ww2-soldier-cc-by-4/`; do not use this exception to expose other
  local flavor assets.
- Do not mark the asset release-ready until the real authenticated source, generated `asset.json`,
  `SOURCE.md`, `LICENSE.txt`, and `bake.report.json` are present and their hashes validate.
- Before packaging, run `python3 tools/verify_soviet_character_bake.py` with
  `assets/flavor/Characters/soviet-ww2-soldier-cc-by-4`; it re-hashes every recorded
  source/metadata/rig/pose file and rejects missing, stale, or extra GLBs.
- Shipped credits must identify “WW2 Soviet Soldier,” Michael Jones / `mikhailgolubkov09`, link the
  source and CC BY 4.0 license, and state that Kessel changed the model by rigging it, adding the
  rifle, and baking animation poses.
- Runtime packages may include the reviewed rigged/posed derivatives and required attribution.
  Preserve the original GLB in source-control provenance, not as an ambiguously renamed or
  re-encoded download.

## Pizza Doggy packs

Kessel uses selected models from locally purchased Pizza Doggy packs. The purchase archive retained
on the development workstation includes Pizza Doggy's **Game Asset License Agreement**. The
agreement permits using and modifying the assets in commercial and non-commercial games. It does
not permit reselling, sharing, or redistributing the assets by themselves, including edited copies
or inclusion in asset packs, templates, or general-purpose bundles.

The vendor also publishes the license update and product pages online:

- <https://pizzadoggy.itch.io/psx-mega-pack-2/devlog/961920/updated-the-license-agreement>
- <https://pizzadoggy.itch.io/psx-tech>

Release policy:

- Cook only the reviewed catalog closure needed by Kessel; never ship the source archives, Blender
  libraries, alternate FBX/OBJ/DAE copies, previews, thumbnails, or the full authoring library.
- Include cataloged models only as integrated Kessel game resources. Do not publish the cooked
  model directory as an asset download, SDK, template, or reusable library.
- Keep each cooked entry's asset ID, source family, byte hash, and license ID in the runtime manifest
  so every shipped file remains auditable.
- Do not add a new Pizza Doggy pack or asset family to a public build until its purchase and license
  evidence have been reviewed and recorded.
- Re-review the current vendor terms before the first public or commercial release.

Attribution is appreciated by the vendor but is not required by the retained agreement. Kessel
should nevertheless credit **Pizza Doggy** in the shipped credits and documentation.

## Kessel local and secondary assets

Catalog entries marked `kessel-provenance-unrecorded` remain blocked from public distribution.
Their presence in a developer build does not establish redistribution rights. Each one must be
replaced, removed, or assigned a verified license record before a public package is approved.
