# Tiger II static editor source

Complete source geometry from `/home/alex/panzer/tank_simulator/source_assets/legacy-imports/asset-warehouse-20260831/incoming/2026-08-31-new-assets/pzkpfw-vi-tiger-2-with-full-interior.zip` → `source/Tiger2int.zip` → `Tiger2int.obj`.

The archive supplies anonymous OBJ objects, normals and UV coordinates, but no material library, textures, semantic part names, pivots, hierarchy, author or license record. The matte neutral metal is explicitly provisional. Every source triangle and object split is preserved by direct OBJ-to-GLB conversion. This is an inspection/editor model, not a playable tank rig.

`source_manifest.v1.json` records hashes, exact normalization matrix, bounds and the physical-scale limitation. Rebuild with `python3 tools/bake_tank_editor_sources.py --vehicle tiger_ii` from the shader checkout.
