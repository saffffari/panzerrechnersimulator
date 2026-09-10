# WW2 Soviet Soldier

- Creator: **Michael Jones** (`mikhailgolubkov09`)
- Source: <https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931>
- Retrieved: 2026-09-05
- License: **Creative Commons Attribution 4.0 International** (`CC-BY-4.0`), <https://creativecommons.org/licenses/by/4.0/>
- Preserved source: `source/ww2_soviet_soldier_original.glb`
- Preserved source SHA-256: `5edf84ae6921e7e3f7a18b3f14b8d9c8e6dd44188e0afaf4e70370f63f96b7e7`

## Attribution

“WW2 Soviet Soldier” by Michael Jones / mikhailgolubkov09, used under CC BY 4.0.
Kessel adds donor-derived skin weights, a low-poly rifle, authored poses, and static runtime pose
frames. The original downloaded GLB remains byte-for-byte in `source/`; the conversion does not
replace or obscure the original creator attribution.

## Runtime conversion

`authoring/soviet_ww2_soldier_rigged.glb` is the editable, skinned master. Runtime pose files under
`baked/` are evaluated copies of that master because Kessel's current GLB loader flattens nodes and
does not play skeletal animation. Rebuild them with `tools/bake_soviet_infantry.py`; the bake report
records the actual source, donor, Blender, and output hashes.
