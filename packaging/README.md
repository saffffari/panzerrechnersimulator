# macOS distribution scaffold

`tools/package_macos.sh` builds an Apple-Silicon `Kessel.app`, signs and verifies it, and creates a
compressed DMG with native macOS tools. It calls the repository's reviewed runtime-model cooker;
the multi-gigabyte Pizza authoring warehouse is never copied wholesale.

Default outputs:

- `dist/macos/Kessel.app`
- `dist/macos/Kessel-<version>-arm64.dmg`
- `dist/macos/Kessel-<version>-arm64.dmg.sha256`

The bundle layout is:

```text
Kessel.app/Contents/MacOS/kessel
Kessel.app/Contents/Info.plist
Kessel.app/Contents/Resources/assets/
  models/
  fonts/
  catalog/runtime_assets.v1.json
  runtime-assets.manifest.json
  MANIFEST.sha256
  PACKAGE-MANIFEST.sha256
```

Run a read-only prerequisite/catalog check first:

```sh
tools/package_macos.sh --dry-run
```

Build an ad-hoc-signed local artifact:

```sh
tools/package_macos.sh
```

Build with a Developer ID and notarize using credentials already stored by `notarytool`:

```sh
xcrun notarytool store-credentials kessel-notary
tools/package_macos.sh \
  --sign-identity "Developer ID Application: Example Name (TEAMID)" \
  --notary-profile kessel-notary
```

No Homebrew utility is required. The script uses `hdiutil`, `codesign`, `xcrun`, `otool`, `lipo`,
`plutil`, `ditto`, and `shasum` from macOS/Xcode Command Line Tools.

## Current integration boundary

The scaffold deliberately does not conceal a remaining engine change. The current executable opens
repository-relative `assets/...` paths, while the cooker writes stable model IDs under
`Contents/Resources/assets/models/`. Before the app is friend-ready, source code must resolve a
bundle resource root (and optionally `KESSEL_ASSET_ROOT` in development), consume the runtime
manifest's source-to-cooked mapping, and include/cook the non-GLB closure: terrain/ground textures,
audio, UI/gameplay data, AI data, and Tiger metadata JSON. Until then the generated app is a signed,
verified distribution scaffold rather than a self-contained playable release.

Use `tools/package_macos.sh --help` for all inputs, modes, refusal conditions, and outputs.
