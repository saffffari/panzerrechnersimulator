# macOS distribution scaffold

`tools/package_macos.sh` builds an Apple-Silicon `Kessel.app`, signs and verifies it, and creates a
compressed DMG with native macOS tools. It combines two independently locked closures:

- `catalog/runtime_assets.v1.json` drives the reviewed GLB model cooker.
- `catalog/runtime_payload.v1.json` locks every non-GLB file required by Garage and the three
  campaign acts, the UI fonts/notices, and the bundled third-party data/software/asset notices.

Neither cooker scans/copies the asset warehouse. Missing files, changed bytes, new manifest
dependencies, unsafe paths, or stale hashes stop packaging before Cargo runs.

The current reviewed model closure contains 139 GLBs totaling 233,434,384 bytes: 120 integrated
Pizza Doggy models totaling 79,558,992 bytes, exactly 16 retained current Tiger/T-34 models totaling
152,018,608 bytes, one 1,125,872-byte authored P08 bake, and two CC0 crew poses totaling 730,912
bytes. The 16 tank models and P08 bake are the 17 GLBs still carrying unresolved project provenance.

Default outputs:

- `dist/macos/Kessel.app`
- `dist/macos/Kessel-<version>-arm64.dmg`
- `dist/macos/Kessel-<version>-arm64.dmg.sha256`

Packaging outputs are immutable snapshots: validation proves that an app is internally consistent,
but does not compare its embedded catalog with the current source catalog. The ignored
`dist/macos` artifact present when the 134-GLB closure landed still embeds the older 147-GLB,
491,705,164-byte model closure. It remains internally valid but is stale and must not be presented
as a build of the current source. Because the packager refuses to overwrite outputs, use a fresh
directory such as `--out-dir dist/macos-<commit>` (recommended), or deliberately archive/remove the
old output before rebuilding.

The owner-development checkpoint for source commit `f8496b9` is in
`dist/macos-f8496b9-final/`. Its app occupies 627,716 KiB (about 613 MiB), and its DMG is
534,839,582 bytes (about 510 MiB) with SHA-256
`2d57a97bf0e686e2868b0bc146f5efd2bced11992bc5f7904cb13257911d3d72`. Both artifacts pass the
independent app/resource/signature and DMG/signature/checksum validators. With
`KESSEL_ASSET_ROOT` unset and a working directory outside the repository, that packaged executable
loaded Garage/Acts 1–3 from `Contents/Resources`, rendered one Garage frame plus Metal
dawn/day/dusk/night frames for all three acts, and produced 12 exact 2x-nearest atmosphere frames
plus an exact 3x-nearest Garage frame. This is an owner/developer receipt, not a friend/public
release.

The bundle layout is:

```text
Kessel.app/Contents/MacOS/kessel
Kessel.app/Contents/Info.plist
Kessel.app/Contents/Resources/assets/
  models/
  fonts/{VT323-Regular.ttf,PressStart2P-Regular.ttf,LICENSES.md,OFL-1.1.txt}
  catalog/runtime_assets.v1.json
  catalog/runtime_payload.v1.json
  THIRD_PARTY_DATA/CREDITS.txt
  THIRD_PARTY_NOTICES/CC-BY-4.0-ASSETS.txt
  THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt
  THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt
  worldgen/act{1,2,3}/{manifest.v1.json,heights.npy}
  terrain/stalingrad_100km_height16.png
  terrain/act1_rgb/act1_ground_hires.png
  terrain/act{2,3}/{actN_ground_hires.png,ground_class.png}
  tanks/tiger/baked/{tiger_rtt_screens.json,tiger_ports.json}
  runtime-assets.manifest.json
  runtime-payload.manifest.json
  MANIFEST.sha256
  RUNTIME-PAYLOAD.sha256
  PACKAGE-MANIFEST.sha256
```

The non-GLB payload currently contains 21 files totaling 399,649,424 bytes. Paths below the
repository's `assets/` prefix are preserved verbatim below `Contents/Resources/assets`, so a
logical load such as `assets/worldgen/act2/heights.npy` resolves to
`Contents/Resources/assets/worldgen/act2/heights.npy`.

The two Tiger JSON files are the canonical tracked packaging inputs derived from the older Hal bake
outputs. They intentionally have canonical JSON text with one terminal line feed and are not
described as byte-identical authoritative copies of ignored source-tree files. Their exact tracked
bytes and installed logical paths are locked in the payload catalog.

Run a read-only prerequisite/catalog check first:

```sh
tools/package_macos.sh --release-scope owner-dev --dry-run
```

Build an ad-hoc-signed local artifact:

```sh
tools/package_macos.sh --release-scope owner-dev
```

`owner-dev` is the default, but spelling it out in saved build commands makes intent auditable. It
allows ad-hoc or Developer-ID signing and is not redistribution clearance. Do not send or describe
an owner/developer artifact as a friend/public build.

Developer-ID signing is a separate choice and does not silently change release scope:

```sh
tools/package_macos.sh \
  --release-scope owner-dev \
  --sign-identity "Developer ID Application: Example Name (TEAMID)"
```

Every artifact intended for friends or any other public distribution must instead use
`--release-scope public`. Notarization additionally requires that scope:

```sh
xcrun notarytool store-credentials kessel-notary
tools/package_macos.sh \
  --release-scope public \
  --sign-identity "Developer ID Application: Example Name (TEAMID)" \
  --notary-profile kessel-notary
```

That public command is intentionally refused today. The exact current public blockers are:

- 17 local GLBs still marked `kessel-provenance-unrecorded`: 16 retained current Tiger/T-34 models
  and the authored P08 bake.
- Mapzen-based Garage/Act 1 terrain whose exact provenance and applicable redistribution grant have
  not been verified.
- 330 Cargo.lock packages inventoried deterministically but not yet supplied with a reviewed set of
  applicable licence texts, copyright notices, and obligations for the actual macOS binary.
- A required re-check of current Pizza Doggy terms and retained purchase evidence before the first
  public or commercial release.

Owner/developer packaging remains available while those gates are open. A signature does not waive
them, and an ad-hoc signature does not bypass `--release-scope public` validation.

## Terrain data provenance and notices

Terrain imagery and elevation are not labeled project-owned. The locked catalog records these exact
data-source IDs:

- Act 1 drape: Copernicus Sentinel-2 scenes `S2B_38UMU_20240117_0_L2A` and
  `S2B_38UMV_20240117_0_L2A`, acquired 2024-01-17.
- Act 2 drape/class mask: `S2C_33UVU_20250402_0_L2A`, acquired 2025-04-02. Act 2 elevation:
  Copernicus GLO-30 `Copernicus_DSM_COG_10_N52_00_E014_00_DEM`.
- Act 3 drape/class mask: `S2A_33UVT_20250404_1_L2A`, acquired 2025-04-04. Act 3 elevation:
  Copernicus GLO-30 `Copernicus_DSM_COG_10_N52_00_E013_00_DEM`.
- Garage and Act 1 elevation: believed to be Mapzen Terrain Tiles Terrarium data from the public
  `elevation-tiles-prod` S3 service at zoom 12. The exact fetch date, tile keys, and upstream pixel
  layer were not retained. This is a provider inference, not verified release provenance.

The high-resolution drapes are modified Sentinel TCI windows produced through
`tools/worldgen/make_ground_tex.py`; the Act 2/3 class masks derive from those same windows. Required
Sentinel modified-data notices, WorldDEM-30 credit/liability text, and Mapzen's complete composite
source credit list ship as `Contents/Resources/assets/THIRD_PARTY_DATA/CREDITS.txt`. Mapzen
attribution is bundled as useful notice information but is not treated as a redistribution grant;
the public gate remains closed. The source lock
also preserves the official [Sentinel legal notice](https://ads.atmosphere.copernicus.eu/licences/ec-sentinel),
[Copernicus DEM product page](https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM),
[WorldDEM-30 licence](https://dataspace.copernicus.eu/sites/default/files/media/files/2025-06/copernicus_contributing_mission_data_access_v2_cop_dem_licenses.pdf),
[AWS Terrain Tiles registry](https://registry.opendata.aws/terrain-tiles/), and
[Tilezen/joerd attribution](https://github.com/tilezen/joerd/blob/master/docs/attribution.md).

`assets/terrain/stalingrad_100km_meta.json` is not a runtime load. Garage's decoder constants are
compiled in `src/terrain.rs`; the package therefore carries the required height PNG but does not
copy that authoring/source metadata JSON. The surviving untracked legacy metadata and internal
development note are insufficient evidence for public clearance.

## Fonts, Rust dependencies, and Pizza Doggy credit

Both UI font binaries are hash-locked alongside their copyright records and the complete SIL Open
Font License 1.1 text. They ship together below `assets/fonts/`, satisfying the notice bundle rather
than relying on font metadata being discoverable.

`THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt` is deterministically generated from `Cargo.lock` and its
generated bytes are locked in the payload catalog. It inventories all 330 external locked packages,
including target-specific entries. It deliberately says it is not a completed licence audit; public
mode stays blocked until notices for the dependencies actually linked into the release binary are
reviewed and bundled.

`THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt` credits Pizza Doggy and records the integrated-game-only
packaging policy: no source archives, complete authoring trees, alternate formats, previews, or
standalone cooked-asset distribution. It is a policy summary, not a substitute for the retained
purchase agreement. The first public/commercial release still requires the recorded terms re-check.

No Homebrew utility is required. The script uses `hdiutil`, `codesign`, `xcrun`, `otool`, `lipo`,
`plutil`, `ditto`, and `shasum` from macOS/Xcode Command Line Tools.

## What is intentionally excluded

The closure excludes raw Pizza archives and non-GLB Pizza trees; FBX/OBJ/glTF/Blender and generator
inputs; previews, thumbnails, screenshots and user images; legacy manifests and terrain-source
rasters/arrays; and AI training checkpoints/test artifacts. The friend-facing campaign currently
uses scripted AI, so the command-line neuroevolution/showreel weight files are not campaign runtime
dependencies. UI pixel fonts are compiled into the executable and also ship with their locked font
notices. Recorded sounds are omitted because they are optional, explicitly opt-in, and presently
either owner-private Pizza Doggy integration material or carry unverified/non-commercial provenance;
every gameplay cue has the procedural runtime fallback/base. The local
`assets/sounds/pizza_private/` bank and `BANK-RECEIPT.json` are never payload-discovery inputs.

If a worldbuilder `manifest_edits.json` appears, a manifest gains another non-GLB dependency, or a
new quoted non-GLB `assets/...` literal appears anywhere in `src/*.rs`,
`tools/runtime_payload.py validate` reports the catalog/inventory as stale or unreviewed. Every
current direct literal is classified as required runtime, optional fallback, legacy diagnostic,
developer tooling, or optional authoring input. Review a new dependency or exclusion rationale,
then generate a candidate without overwriting the lock:

```sh
tools/runtime_payload.py lock --output /tmp/runtime_payload.v1.json
```

Review the path, classification, byte count and SHA-256 before replacing the tracked catalog. Never
refresh a hash merely to silence drift.

## Verification layers

`runtime-assets.manifest.json` and `MANIFEST.sha256` cover cooked GLBs.
`runtime-payload.manifest.json` records the non-GLB source mapping, per-file SHA-256, byte count,
aggregate byte total, direct-resource inventory, release scope, licence/data-source records, and
release blockers.
`RUNTIME-PAYLOAD.sha256` independently covers those payload files and its bundled catalog. Finally
`PACKAGE-MANIFEST.sha256` covers the complete assets tree, including fonts and both closures. App
validation recomputes every layer and requires the tree to equal the exact union of the two reviewed
source catalogs plus their fixed receipt files. An extra file is rejected even if somebody adds it
to `PACKAGE-MANIFEST.sha256`; in particular, an owner-private Pizza audio bank can never turn a
local app into a validated distributable payload by re-signing it.

The chosen scope is also stamped into `Info.plist` as `KesselReleaseScope`; app verification requires
that value to match the locked runtime manifest. Signing therefore cannot silently relabel an
owner/developer payload as public.

The DMG checksum contains only the DMG basename, so it remains verifiable after the two files are
moved or downloaded together. Existing files and dangling symlinks at the output app, DMG, checksum,
or output-directory path are refused rather than overwritten or followed.

These checks prove a deterministic, byte-verified bundle input and resource layout; they do not by
themselves prove a self-contained playable release. A newly assembled final `.app` still must be
launched from inside its bundle and exercise Garage plus Acts 1-3 before that claim is made. The
read-only `--dry-run` deliberately creates and launches nothing.

Use `tools/package_macos.sh --help` for all inputs, modes, refusal conditions, and outputs.
