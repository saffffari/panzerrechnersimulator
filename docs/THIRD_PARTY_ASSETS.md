# Third-party asset release policy

This file records the evidence and packaging policy for third-party runtime assets. It is an
engineering release gate, not a substitute for the original license agreement or legal advice.

## OpenGameArt CC0 crew character

Panzerrechnersystem includes **Rigged Lowpoly WW2 Soldier** by **nisu**, downloaded from
<https://opengameart.org/content/rigged-lowpoly-ww2-soldier>. The source page designates the asset
as **CC0 1.0 Universal**. The original FBX, texture, complete CC0 legal text, source URL, retrieval
date, hashes, conversion receipts, preserved rigged GLB, and deformation-applied runtime poses are
tracked together under
`assets/flavor/Characters/german-ww2-soldier-cc0/`.

CC0 permits copying, modification, and redistribution without attribution. Panzerrechnersystem nevertheless
credits nisu and retains provenance. This older crew family is no longer part of the live runtime
model closure after the AdrielCZ Panzertruppe replacement. Its reviewed self-contained GLBs remain
available as historical fallback material; the FBX, separate source texture, and rigged authoring
mesh must not be copied into the cooked runtime model tree.

## OpenGameArt CC0 pocket compass

The handheld compass derives from **Lowpoly survival item pack** by **Artjom Shebunin (artie31)**,
published at <https://opengameart.org/content/lowpoly-survival-item-pack> under **CC0 1.0 Universal**.
The runtime derivative retains the actual body and separate needle, normalizes the diameter to
105 mm, and adds an ivory dial, grey-green casing, physical cardinal letters and graduations.
`assets/props/hand_compass_artie31/` tracks only the self-contained runtime GLB, exact checksum
receipt, creator attribution and original readme. The original ZIP and Blender authoring inputs
remain in the canonical source warehouse. The model cook selects this compass instead of the
unused modern Pizza pistol magazine; the P08 now uses its own articulated source magazine.

## Sketchfab CC BY 4.0 Soviet infantry character

Panzerrechnersystem's selected Soviet infantry source is **WW2 Soviet Soldier** by **Michael Jones**
(`mikhailgolubkov09`):
<https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931>.
The source page designates it **Creative Commons Attribution 4.0 International**
([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)). Panzerrechnersystem's adaptations add skin
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
  source and CC BY 4.0 license, and state that Panzerrechnersystem changed the model by rigging it, adding the
  rifle, and baking animation poses.
- Runtime packages may include the reviewed rigged/posed derivatives and required attribution.
  Preserve the original GLB in source-control provenance, not as an ambiguously renamed or
  re-encoded download.

## Sketchfab CC BY 4.0 Tiger crew kit

Panzerrechnersystem's replacement Tiger crew derives from **Free Tiger And Crew Kit** by **AdrielCZ**:
<https://sketchfab.com/3d-models/free-tiger-and-crew-kit-6290669ab2cb49599f1606507626af39>.
The source page designates it **Creative Commons Attribution 4.0 International**. The reviewed
archive SHA-256 is
`ecc46d04c5016f07b54838e65b56705486917f2ccda2fce5708900e406183483`.

The archive is already rigged but contains no animations. Panzerrechnersystem removes the bundled Tiger, assigns
the five supplied face textures to permanent crew identities, authors standing, seated and relaxed
station poses, bakes static runtime meshes, combines the crew materials into loader-compatible
atlases, and exports the included handheld field glasses as a separate physical prop. First-person
arms and binocular-hand pose frames remain deferred until they can be registered without a stand-in.
The archive's duplicate PNG member
names must be recovered only through `tools/extract_tiger_crew_kit.py`; ordinary extraction silently
loses faces and material channels.

Release policy:

- Keep the original download in the private asset warehouse; do not commit the raw archive.
- Track only the deterministic extractor, attribution, conversion receipts, rigged authoring master,
  and optimized self-contained runtime GLBs under
  `assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4/`.
- Shipped credits must name “Free Tiger And Crew Kit,” AdrielCZ, link the source and CC BY 4.0, and
  identify Panzerrechnersystem's modifications.
- Do not ship or catalog the kit's bundled Tiger; Panzerrechnersystem uses its separately reviewed Tiger asset.

## Sketchfab CC BY 4.0 Enigma prop

Panzerrechnersystem's physical Enigma prop derives from **Enigma Machine** by **Ashish0096**:
<https://sketchfab.com/3d-models/enigma-machine-30b85c037a164174a109ad6f002b9c47>.
The source page designates it **Creative Commons Attribution 4.0 International**. Panzerrechnersystem reduces the
147,636-triangle authoring model to 18,676 triangles while retaining every source mesh object,
combines its materials into one base-color/metallic-roughness/normal atlas set, converts the DirectX
normal maps to the glTF convention, and packages a self-contained runtime GLB. The current renderer
does not transmit through the small glass panel, so the adaptation represents it as a dark smooth
opaque dielectric and records that limitation rather than claiming physical refraction.

The raw FBX and 4K textures remain in the private asset warehouse. Attribution, exact input hashes,
the deterministic bake, output hash, full license text, and the optimized derivative live under
`assets/flavor/Props/enigma-machine-ashish0096-cc-by-4/`. A shipped build containing the prop must
retain its title, creator, source link, CC BY 4.0 link, and modification notice.

## Sketchfab CC BY 4.0 skeleton character

Panzerrechnersystem's casualty anatomy derivative uses **Skeleton Character PSX** by **Vinrax**:
<https://sketchfab.com/3d-models/skeleton-character-psx-ece576bbed4b4364911c7596d828a558>.
The source page designates it **Creative Commons Attribution 4.0 International**. The reviewed
archive SHA-256 is
`65825015009305fcc65db9eb524720306497c03ffadfdb2f1d0f733cb116e40d`.

Panzerrechnersystem preserves the source's native 27-bone skin in an authoring GLB, rotates only existing arm
bones into a relaxed pose, and bakes a static 1.53 m runtime mesh without decimating its 1,596
triangles. The eleven-part casualty crosswalk is a deterministic render partition with a measured
maximum landmark error of 0.171724 m; it is approximate compatibility with the independently
authored clothed crew, not exact anatomical registration or medical segmentation.

The raw ZIP, nested archive, FBX, and loose texture remain in the private asset warehouse. A shipped
build containing the runtime derivative must credit “Skeleton Character PSX,” Vinrax, the source
URL, CC BY 4.0, and the pose/normalization/partition modifications. Rebuilds must pass the exact
outer-archive, nested-archive, FBX, texture, topology, rig, and repeatability gates recorded under
`assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4/`.

## Sketchfab CC BY 4.0 city bridge

Panzerrechnersystem's reviewed river-crossing asset derives from **Stylised Low Poly City Metal Bridge** by
**remidoes3d**:
<https://sketchfab.com/3d-models/stylised-low-poly-city-metal-bridge-674aee9cfd7b478b93c2f3e2d7026a0c>.
The source page designates it **Creative Commons Attribution 4.0 International**. Panzerrechnersystem retains all
7,384 source triangles, packs the four materials into one embedded atlas, and derives the placement
origin from the actual road slab so the driveable deck is glTF Y=0. The unrelated source AO image
is not baked into the color texture.

The raw FBX and 4K textures remain in the private asset warehouse. Attribution, exact input hashes,
the deterministic bake, output hash, full license text, and optimized derivative live under
`assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/`. The bridge is an authored scene
asset and does not enter a shipping runtime catalog until a map or prefab actually references it.

## Sketchfab CC BY 4.0 Soviet infantry character

Panzerrechnersystem's selected Soviet infantry source is **WW2 Soviet Soldier** by **Michael Jones**
(`mikhailgolubkov09`):
<https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931>.
The source page designates it **Creative Commons Attribution 4.0 International**
([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)). Panzerrechnersystem's adaptations add skin
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
  source and CC BY 4.0 license, and state that Panzerrechnersystem changed the model by rigging it, adding the
  rifle, and baking animation poses.
- Runtime packages may include the reviewed rigged/posed derivatives and required attribution.
  Preserve the original GLB in source-control provenance, not as an ambiguously renamed or
  re-encoded download.

## Pizza Doggy packs

Panzerrechnersystem uses selected models from locally purchased Pizza Doggy packs. The purchase archive retained
on the development workstation includes Pizza Doggy's **Game Asset License Agreement**. The
agreement permits using and modifying the assets in commercial and non-commercial games. It does
not permit reselling, sharing, or redistributing the assets by themselves, including edited copies
or inclusion in asset packs, templates, or general-purpose bundles.

The vendor also publishes the license update and product pages online:

- <https://pizzadoggy.itch.io/psx-mega-pack-2/devlog/961920/updated-the-license-agreement>
- <https://pizzadoggy.itch.io/psx-tech>

Release policy:

- Cook only the reviewed catalog closure needed by Panzerrechnersystem; never ship the source archives, Blender
  libraries, alternate FBX/OBJ/DAE copies, previews, thumbnails, or the full authoring library.
- Include cataloged models only as integrated Panzerrechnersystem game resources. Do not publish the cooked
  model directory as an asset download, SDK, template, or reusable library.
- Keep each cooked entry's asset ID, source family, byte hash, and license ID in the runtime manifest
  so every shipped file remains auditable.
- Owner-private audio follows the narrower workflow in `docs/PIZZA_AUDIO_BANK.md`: Git records only
  an exact source-relative selection and policy; a local cooker hashes/copies the selected OGGs into
  a gitignored receipt-bound bank; runtime playback requires an explicit opt-in and keeps the
  procedural sound identity audible beneath every licensed layer. The public package omits the bank.
- Do not add a new Pizza Doggy pack or asset family to a public build until its purchase and license
  evidence have been reviewed and recorded.
- Re-review the current vendor terms before the first public or commercial release.

Attribution is appreciated by the vendor but is not required by the retained agreement. Panzerrechnersystem
should nevertheless credit **Pizza Doggy** in the shipped credits and documentation.

## Panzerrechnersystem local and secondary assets

Catalog entries marked `kessel-provenance-unrecorded` remain blocked from public distribution.
Their presence in a developer build does not establish redistribution rights. Each one must be
replaced, removed, or assigned a verified license record before a public package is approved.
