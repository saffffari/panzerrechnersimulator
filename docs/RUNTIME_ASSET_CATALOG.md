# Runtime GLB catalog and model cook

`catalog/runtime_assets.v1.json` is the tracked semantic lock for Kessel's current runtime model
closure. It is deliberately not a mirror of `assets/pizza`: the catalog selects 155 self-contained
GLBs (241,657,292 source bytes) instead of copying the multi-gigabyte authoring library.

The audited scope is:

- 66 unique GLBs referenced by the three versioned act manifests and their tracked worldbuilder
  overlays. The current on-disk truth is 56 Pizza GLBs and 10 retained non-Pizza GLBs.
- 64 additional Pizza GLBs loaded by code-authored city, interior, and gameplay paths, including the
  P08 magazine, loose cartridge, and ejected-casing props.
- 35 non-Pizza runtime GLBs in total: 16 current Tiger/T-34 models, the compact authored P08 runtime
  bake, 15 CC-BY-4.0 Panzertruppe static poses, physical binoculars, the Enigma machine, and the
  anatomy skeleton. Ten tank models overlap the act-manifest set. Rigged masters, FBX files,
  textures, alignment notes, and conversion receipts remain authoring inputs and are not copied
  into the runtime model cook.

Every entry locks a stable ID, source and canonical runtime path, SHA-256 and byte size, measured
scene-space bounds/dimensions/origin relationship, semantic tags, collision/destruction classes,
render range, and an explicit LOD hook. The 18 crew/prop/anatomy derivatives carry reviewed
CC-BY-4.0 records; Pizza Doggy models retain their integrated-game-only purchase policy; 16 local Tiger/T-34 models and the
authored P08 bake still have unresolved provenance, so public redistribution remains blocked
pending that review. The P08 bake is locally ignored and must not be staged or redistributed until
its source ownership is resolved; `assets/flavor/luger/SOURCE.md` records the exact source/bake hashes.

The reviewed city bridge is an authoring-library asset, not a live runtime dependency, and is
deliberately absent from this closure. The crew's rigged authoring master and every report/source
document are likewise excluded even though they remain in the repository for reproducibility.

The model cook is one half of the application payload. Packaged builds consume
`runtime-assets.manifest.json` through `src/resource.rs`: source callers keep their stable
`assets/...` names while the resolver maps reviewed GLBs to `models/<stable-id>.glb` and rejects an
uncataloged packaged GLB. `catalog/runtime_payload.v1.json` independently locks terrain, ground
textures, fonts, notices, gameplay data, UI data, and Tiger metadata. Together the two exact
closures form the owner-development app payload; public redistribution remains blocked by the
provenance and notice gates above. Optional SU-100 and other non-live hero GLBs remain outside this
selected model set and use the engine's fallbacks.

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
