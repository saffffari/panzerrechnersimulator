# Runtime GLB catalog and model cook

`catalog/runtime_assets.v1.json` is the tracked semantic lock for Kessel's current runtime model
closure. It is deliberately not a mirror of `assets/pizza`: the catalog selects 137 self-contained
GLBs (232,703,472 source bytes) instead of copying the multi-gigabyte authoring library.

The audited scope is:

- 69 unique GLBs referenced by the three versioned act manifests and their tracked worldbuilder
  overlays. The current on-disk truth is 59 Pizza GLBs and 10 retained non-Pizza GLBs.
- 61 additional Pizza GLBs loaded only by normal game code, including the P08 magazine, loose
  cartridge and ejected-casing props used by the FPS reload/effects pass.
- 17 retained non-Pizza runtime GLBs in total, including the authored P08 Luger runtime bake and
  the Tiger and T-34 bakes.

Every entry locks a stable ID, source and canonical runtime path, SHA-256 and byte size, measured
scene-space bounds/dimensions/origin relationship, semantic tags, collision/destruction classes,
render range, and an explicit LOD hook. License records are intentionally conservative: the
selected Pizza pack directories do not contain their license sidecar, and provenance for local
project assets is not recorded in this repository, so redistribution remains review-required.

This is deliberately a **model-only foundation**, not a runnable application package. The engine
does not yet consume `runtime-assets.manifest.json`: it still opens the original `assets/...`
source paths, while the cook writes stable flat paths under `models/`. A distributable build still
needs an asset-root resolver/source-to-runtime remap plus a separate closure for terrain and ground
textures, audio, fonts, AI/gameplay data, UI and Tiger metadata JSON. Optional SU-100 and authored
hero GLBs are also outside this selected model set and currently use the engine's fallbacks.

## Commands

From the repository root:

```bash
python3 tools/runtime_asset_cook.py validate
python3 tools/runtime_asset_cook.py cook --output target/runtime-assets/v1
python3 tools/test_runtime_asset_cook.py
```

`validate` and the tests are offline and deterministic. Source-byte validation requires the
untracked model payload; fixture tests still run in a source-only clone. `cook` refuses an existing
output path, never rewrites source assets, and emits only byte-preserved GLB 2.0 files plus:

- `runtime-assets.manifest.json` — IDs, paths, sizes, SHA-256, source/license, semantics, bounds,
  origin, collision/destruction, render range, LOD hooks, and dependency results.
- `missing-dependencies.json` — always emitted, including when the current audited set is empty.
- `MANIFEST.bytes.tsv` and `MANIFEST.sha256` — simple copy-verification manifests.

ZIP, RAR, 7z, FBX, OBJ, DAE, MTL, TIF/TIFF, Blend/Blend1, and preview/thumbnail material are rejected
as inputs or output. GLBs with external buffers or images are reported as dependencies and rejected;
the current catalog is entirely self-contained.

`bootstrap` exists only to create a new catalog at a path that does not already exist. It will not
overwrite the reviewed catalog. Adding or changing an asset therefore requires an explicit catalog
review rather than silently broadening runtime scope.
