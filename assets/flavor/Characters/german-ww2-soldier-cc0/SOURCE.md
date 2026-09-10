# Rigged Lowpoly WW2 Soldier

- Original title: **Rigged Lowpoly WW2 Soldier**
- Creator: **nisu**
- Source page: <https://opengameart.org/content/rigged-lowpoly-ww2-soldier>
- Original download: <https://opengameart.org/sites/default/files/lowpolysoldier.zip>
- Retrieved: 2026-08-24
- License: **CC0 1.0 Universal** (`CC0-1.0`); see [`LICENSE.txt`](LICENSE.txt)
- Original archive SHA-256:
  `a8e2f5a5777d1c9d4c18f76922d9737b88be717a455a65336a8772deb53729cb`

The creator released this model under CC0, so attribution is not required. Kessel keeps this source
record anyway so the provenance and conversion remain auditable.

## Preserved source

| File | SHA-256 |
|---|---|
| `source/LowpolySoldier.fbx` | `e5abab6089d66181f755d8878a7ee08c89a0d0358e527b379db86022626b22e0` |
| `source/LowpolySoldier_Texture.png` | `a2a956a3efcb97614cc84ca37d47207bb4a7a6d474bf547ca77eb161649635ef` |

The FBX is Kaydara FBX 7.3. It contains one 49-bone humanoid armature and two skinned mesh objects:
the full soldier body and a separate helmet. The source contains no animation clips. Its texture is
1710×2048 RGB.

## Kessel conversions

Blender 4.5.9 LTS was used to bind the source texture—which the FBX carried without an assigned
material—and export self-contained GLB 2.0 files:

- `baked/german_ww2_soldier_rigged.glb` preserves the complete armature, skin weights, helmet,
  body, UVs, texture and material for future station-specific posing.
- `baked/german_ww2_soldier_seated_static.glb` bakes a universal compact seated pose into static
  vertex positions for the current renderer.
- `baked/german_ww2_soldier_relaxed_static.glb` bakes an upright, arms-lowered, mildly asymmetric
  pose that runtime code can rotate into a fallen-body pose.

Both static files contain no skin, `JOINTS_*`, `WEIGHTS_*`, armature, or animation data. They are
glTF Y-up, face +Z, stand on local Y=0, and are normalized to 1.53 m authored height. Each matching
`.report.json` is the deterministic conversion receipt.

Rebuild from the repository root:

```sh
blender --background --factory-startup \
  --python tools/bake_crew_soldier.py -- \
  assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx \
  assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_rigged.glb \
  assets/flavor/Characters/german-ww2-soldier-cc0/baked/german_ww2_soldier_rigged.report.json

blender --background --factory-startup \
  --python tools/bake_crew_soldier_variants.py -- \
  assets/flavor/Characters/german-ww2-soldier-cc0/source/LowpolySoldier.fbx \
  assets/flavor/Characters/german-ww2-soldier-cc0/baked
```

The source artwork is unchanged apart from material binding and posing. It remains an infantry
uniform with an armband and helmet; tanker-specific clothing, headphones, cap, and insignia cleanup
are a later art pass derived from the retained rigged source.

## Steel helmet retained on the current crew (2026-09-05)

`baked/german_steel_helmet_panzertruppe_relaxed_static.glb` extracts only `Helmet_Static` from
the preserved relaxed CC0 bake. It restores the steel-helmet silhouette requested from the earlier
game reference while keeping the current AdrielCZ Panzertruppe bodies, rig, uniform and five faces.
The source helmet's 229 runtime vertices, 246 triangles, UVs and original 1710 x 2048 atlas are
preserved. Its dimensions are unchanged; a translation centers it on the current head with 20 mm
of crown clearance. Painted-steel material factors match the previous runtime reference.

The adjacent `.report.json` records exact inputs, hashes, bounds and rigid transforms fitted to
all 230 corresponding head vertices. The seated/standing fits have a maximum residual below
0.000001 m and are shared by the five face identities. Reproduce with:

```sh
python tools/fit_crew_steel_helmet.py
python tools/fit_crew_steel_helmet.py --check
```

This is a visual attachment, not a new helmet armor or injury model. A present detached head keeps
its helmet; pulverizing the head hides it. The separately licensed Vinrax injury skull is fitted
inside the current head envelope at runtime, with a 4 mm inset, while retaining its source triangles
and UVs. That render-only adjustment is approximate visual registration and does not alter the
physical head, damage thresholds, mass, or any other skeleton page.
