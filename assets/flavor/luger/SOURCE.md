# P08 Luger runtime asset

`baked/luger_runtime.glb` preserves the geometry, scene hierarchy, proportions, UVs, and
base/metallic-roughness material channels from the local authored source `luger.glb`.

- Source SHA-256: `b4c7d2cfef91f880fe4cd31bbdd7d75e8aedf1e491cd01e2a30147d6f35f56de`
- Runtime bake: `python3 tools/build_luger_runtime.py --source <source> --output <output>`
- Bake changes: 4096 px textures reduced to 512 px, base alpha forced opaque, normal map
  omitted, and material set to opaque/single-sided.
- Source geometry visible in the default scene: 7,026 vertices / 7,547 indexed triangles.
- Authored size: 0.032524 m wide, 0.139961 m high, 0.220000 m long; +Z is muzzle-forward.

The source's ownership/license provenance is not recorded in this repository. Its catalog
license therefore remains `kessel-provenance-unrecorded` / redistribution review required.
This note documents the transformation and does not grant redistribution rights.
