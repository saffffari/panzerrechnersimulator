# Panzerrechnersystem

The current product name is **Panzerrechnersystem**. The Cargo package, default game executable,
window title, menu logo, Worldbuilder title, desktop shortcut, and new package display names use
that spelling. The executable and command spelling is `panzerrechnersystem`.

On Linux, use `tools/launch_panzerrechnersystem.sh` for the prebuilt release, or add `--build` after
source changes. `tools/install_linux_desktop.sh` installs the desktop menu and Playtest action.
`tools/launch_kessel.sh` remains a forwarding compatibility command.

The following names retain their existing values because they identify persistent data or
historical evidence:

- `KESSEL_*` environment variables, serialized `kessel.*` schemas, network protocol magic,
  checkpoints, save locations, and renderer cache paths.
- Asset filenames, source attribution, catalog identifiers, Blender object names, and provenance
  tags already referenced by reviewed payloads.
- Dated captures, audits, release notes, transfer archives, historical Unreal design documents,
  and immutable macOS owner-build receipts. Existing names in those records describe their
  original version; they are not the current product name.
- The macOS bundle identifier, `KesselReleaseScope` metadata key, and entitlements filename.
  New scaffold packages use `Panzerrechnersystem.app` and `Contents/MacOS/panzerrechnersystem`;
  validation still accepts the old executable in archived bundles.

The German word *Kessel* in Stalingrad and Halbe history means an encirclement. Those place and
historical descriptions retain their correct wording independently of the product rename.
