# Stylised Low Poly City Metal Bridge

- Original title: **Stylised Low Poly City Metal Bridge**
- Creator: **remidoes3d**
- Source: <https://sketchfab.com/3d-models/stylised-low-poly-city-metal-bridge-674aee9cfd7b478b93c2f3e2d7026a0c>
- Sketchfab UID: `674aee9cfd7b478b93c2f3e2d7026a0c`
- Retrieved: 2026-08-30
- License: **Creative Commons Attribution 4.0 International** (`CC-BY-4.0`)
- License: <https://creativecommons.org/licenses/by/4.0/>
- Reviewed source FBX SHA-256:
  `f9b016b4c9c195b5418b404a07b163f6495b2e4642967ebb857cf9152a236432`

The unmodified FBX and four 4K base-color textures remain in the private local/Hal asset warehouse.
They are not committed. The reviewed source is 7,384 triangles and measures approximately
19.978 m × 4.065 m × 3.508 m in its Blender X/Y/Z axes. It contains no preview or ground-plane
geometry.

## Kessel conversion

`baked/city_metal_bridge_runtime.glb` retains all 7,384 source triangles. The four source material
textures are resized into one embedded 1024² 2×2 base-color atlas, and the geometry is joined into
one material for Kessel's current first-material-wins GLB loader. The unrelated
`internal_ground_ao_texture.jpeg` is not referenced by the reviewed bridge mesh and is not used to
bake fake shading.

The origin is geometry-derived rather than hand-tuned: the bake finds the equal-area broad faces of
the road slab and places its upper driveable surface at glTF Y=0. The structure extends about
0.311 m below that plane and 3.198 m above it. The bridge's 19.978 m long axis is glTF X; its width
is glTF Z. This makes terrain stitching and vehicle contact placement explicit and repeatable.

Rebuild from the repository root with the exact reviewed inputs:

```sh
blender --background --factory-startup --python-exit-code 1 \
  --python tools/bake_city_metal_bridge.py -- \
  /path/to/Pont.fbx /path/to/textures \
  assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked

python3 tools/verify_reviewed_static_glb.py bridge \
  assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked/city_metal_bridge_runtime.glb \
  assets/flavor/Structures/city-metal-bridge-remidoes3d-cc-by-4/baked/city_metal_bridge_runtime.report.json
```

The generated report is the authority for exact input/output hashes, Blender version, topology,
bounds, atlas construction, material assignment, deck detection, and placement origin.
