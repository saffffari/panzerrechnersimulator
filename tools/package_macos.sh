#!/bin/bash
# Build, sign, verify, and package Kessel for Apple Silicon macOS.
# Bash 3.2 compatible; intentionally depends only on macOS-native tools plus Python and Rust.

set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"

APP_NAME="Kessel"
BUNDLE_ID="com.alexandersaffari.kessel"
VERSION=""
BUILD_VERSION=""
MIN_MACOS="13.0"
TARGET_TRIPLE="aarch64-apple-darwin"
OUT_DIR="$REPO_ROOT/dist/macos"
BINARY_PATH=""
BINARY_OVERRIDDEN=0
SKIP_BUILD=0
OFFLINE=0
NO_DMG=0
DRY_RUN=0
RELEASE_SCOPE="owner-dev"
VALIDATE_ONLY_APP=""
VERIFY_DMG=""
SIGN_IDENTITY="${KESSEL_CODESIGN_IDENTITY:-}"
NOTARY_PROFILE="${KESSEL_NOTARY_PROFILE:-}"
ENTITLEMENTS="$REPO_ROOT/packaging/Kessel.entitlements"

PLIST_TEMPLATE="$REPO_ROOT/packaging/Info.plist.in"
COOKER="$REPO_ROOT/tools/runtime_asset_cook.py"
CATALOG="$REPO_ROOT/catalog/runtime_assets.v1.json"
PAYLOAD_TOOL="$REPO_ROOT/tools/runtime_payload.py"
PAYLOAD_CATALOG="$REPO_ROOT/catalog/runtime_payload.v1.json"
FONT_REGULAR="$REPO_ROOT/assets/fonts/VT323-Regular.ttf"
FONT_BOLD="$REPO_ROOT/assets/fonts/PressStart2P-Regular.ttf"

usage() {
    cat <<'EOF'
Usage: tools/package_macos.sh [options]

Builds a thin arm64 Kessel.app targeting macOS 13, cooks the reviewed GLB model closure and locked
non-GLB Garage/three-act payload into Contents/Resources/assets, signs and verifies the bundle, and
(unless --no-dmg) creates a UDZO DMG.

Build inputs:
  Cargo.lock, Cargo.toml, src/                 Rust application source
  catalog/runtime_assets.v1.json              Reviewed runtime GLB lock
  tools/runtime_asset_cook.py                 Offline validator/model cooker
  catalog/runtime_payload.v1.json             Garage + three-act non-GLB payload lock
  tools/runtime_payload.py                    Offline payload validator/copier
  assets/fonts/{VT323,PressStart2P}*.ttf       Required compile-time and bundled fonts
  Every GLB locked by the runtime catalog      Required; missing/drifted files are fatal
  packaging/Info.plist.in                      Bundle metadata template
  packaging/Kessel.entitlements               Default empty hardened-runtime entitlements

Default outputs:
  dist/macos/Kessel.app
    Contents/MacOS/kessel                     Thin arm64 Mach-O
    Contents/Resources/assets                 Cooked models + logical campaign/runtime data
  dist/macos/Kessel-<version>-arm64.dmg
  dist/macos/Kessel-<version>-arm64.dmg.sha256

Options:
  --out-dir DIR           Output directory (default: dist/macos).
  --version X.Y.Z         CFBundleShortVersionString (default: Cargo package version).
  --build-version N[.N]   CFBundleVersion (default: --version value).
  --bundle-id ID          Bundle identifier (default: com.alexandersaffari.kessel).
  --app-name NAME         Display/bundle name (default: Kessel; executable remains kessel).
  --skip-build            Package an existing binary rather than invoking Cargo.
  --binary PATH           Existing arm64 binary; valid only with --skip-build.
  --offline               Pass --offline to Cargo. Catalog validation/cooking is always offline.
  --no-dmg                Produce and verify only Kessel.app.
  --release-scope SCOPE   Required distribution intent: owner-dev (default) or public.
                          Use public for every friend/public artifact, independent of signing.
  --sign-identity ID      Developer ID Application identity. If omitted, use ad-hoc signing.
                          Environment alternative: KESSEL_CODESIGN_IDENTITY.
  --entitlements PATH     Entitlements plist (default: packaging/Kessel.entitlements).
  --notary-profile NAME   notarytool keychain profile; requires a real signing identity and DMG.
                          Environment alternative: KESSEL_NOTARY_PROFILE.
  --dry-run               Read-only prerequisite/catalog validation and exact build-plan output.
                          Creates no app, staging directory, DMG, signature, or Cargo artifact.
  --validate-only APP     Verify an existing .app; performs no build or mutation.
  --verify-dmg DMG        With --validate-only, also verify the DMG image/signature.
  -h, --help              Show this help.

Signing and notarization:
  Signing identity and release scope are independent. A supplied Developer ID is signed with
  hardened runtime and a trusted timestamp in either scope. A supplied notary profile requires
  --release-scope public and triggers `xcrun notarytool submit --wait`, stapling, and validation
  only after every public-release blocker is cleared.
  Without an identity, the app and DMG receive local ad-hoc signatures with hardened runtime;
  such artifacts are for local testing and will still trigger Gatekeeper for other users.

Refusals:
  The script never overwrites an existing app, DMG, or checksum. It refuses absent fonts,
  missing/drifted catalog GLBs or payload files, non-arm64 or non-macOS-13 binaries, unexpected
  dylib paths, invalid metadata/signatures, incomplete resource hashes, symlinked output targets,
  or --release-scope public while redistribution provenance is unresolved.

Release warning:
  The locked runtime is suitable for owner/developer testing. Public redistribution remains blocked
  by unreviewed non-Pizza model provenance, unverified Mapzen terrain provenance/licensing,
  incomplete Rust dependency licence notices, and the required Pizza Doggy terms re-check.
  Font copyright/OFL text, the Cargo.lock inventory, terrain notices, and Pizza credit/policy ship.
EOF
}

die() {
    printf 'package_macos: ERROR: %s\n' "$*" >&2
    exit 1
}

note() {
    printf 'package_macos: %s\n' "$*"
}

print_command() {
    printf '  '
    printf '%q ' "$@"
    printf '\n'
}

require_file() {
    [ -f "$1" ] || die "required file is absent: $1"
    [ -s "$1" ] || die "required file is empty: $1"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command is unavailable: $1"
}

plist_value() {
    /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null || true
}

find_cargo() {
    if command -v cargo >/dev/null 2>&1; then
        command -v cargo
        return
    fi
    if command -v rustup >/dev/null 2>&1; then
        rustup which cargo 2>/dev/null && return
    fi
    if [ -x "${HOME}/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo" ]; then
        printf '%s\n' "${HOME}/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo"
        return
    fi
    return 1
}

validate_metadata() {
    printf '%s' "$BUNDLE_ID" | grep -Eq '^[A-Za-z0-9]+([.-][A-Za-z0-9]+)+$' \
        || die "invalid bundle identifier: $BUNDLE_ID"
    printf '%s' "$VERSION" | grep -Eq '^[0-9]+([.][0-9]+){1,2}$' \
        || die "version must be numeric X.Y or X.Y.Z: $VERSION"
    printf '%s' "$BUILD_VERSION" | grep -Eq '^[0-9]+([.][0-9]+){0,2}$' \
        || die "build version must be numeric N, N.N, or N.N.N: $BUILD_VERSION"
    printf '%s' "$APP_NAME" | grep -Eq '^[A-Za-z0-9._ -]+$' \
        || die "app name contains unsupported characters: $APP_NAME"
}

validate_binary() {
    local binary="$1"
    local archs minos bad_dylibs
    require_file "$binary"
    /usr/bin/file "$binary" | grep -q 'Mach-O 64-bit executable arm64' \
        || die "binary is not a 64-bit arm64 Mach-O: $binary"
    archs="$(/usr/bin/lipo -archs "$binary")"
    [ "$archs" = "arm64" ] || die "binary must be thin arm64, found: $archs"
    minos="$(/usr/bin/xcrun vtool -show-build "$binary" | awk '$1 == "minos" { print $2; exit }')"
    [ -n "$minos" ] || die "binary has no readable LC_BUILD_VERSION minimum"
    case "$minos" in
        13|13.*) ;;
        *) die "binary minimum macOS must be 13.x, found: $minos" ;;
    esac
    bad_dylibs="$(/usr/bin/otool -L "$binary" | awk '
        NR > 1 {
            dep = $1
            if (dep !~ /^\/System\/Library\// && dep !~ /^\/usr\/lib\//) print dep
        }
    ')"
    [ -z "$bad_dylibs" ] || die "binary has unbundled/non-system dylibs:\n$bad_dylibs"
}

verify_runtime_manifest() {
    local assets_root="$1"
    "$PYTHON_BIN" - "$assets_root" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
manifest_path = root / "runtime-assets.manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
if manifest.get("schema") != "kessel.runtime-assets" or manifest.get("version") != 1:
    raise SystemExit("invalid runtime-assets.manifest.json schema/version")
assets = manifest.get("assets")
if not isinstance(assets, list) or manifest.get("asset_count") != len(assets) or not assets:
    raise SystemExit("runtime asset manifest count is empty or inconsistent")
if manifest.get("missing_dependencies"):
    raise SystemExit("runtime model closure has missing external dependencies")
for entry in assets:
    path = root / entry["path"]
    if not path.is_file():
        raise SystemExit(f"missing cooked model: {entry['path']}")
    blob = path.read_bytes()
    if len(blob) != entry["bytes"] or hashlib.sha256(blob).hexdigest() != entry["sha256"]:
        raise SystemExit(f"cooked model hash/size mismatch: {entry['path']}")
print(f"resource manifest: {len(assets)} cooked GLBs verified")
PY
}

verify_runtime_payload() {
    local assets_root="$1"
    "$PYTHON_BIN" "$PAYLOAD_TOOL" verify --assets-root "$assets_root" \
        || die "non-GLB runtime payload verification failed"
}

verify_app() {
    local app="$1"
    local binary assets plist signature_info local_text release_scope payload_scope
    [ -d "$app" ] || die "app bundle is absent: $app"
    binary="$app/Contents/MacOS/kessel"
    assets="$app/Contents/Resources/assets"
    plist="$app/Contents/Info.plist"
    require_file "$plist"
    /usr/bin/plutil -lint "$plist" >/dev/null || die "Info.plist is invalid"
    [ "$(plist_value "$plist" CFBundleExecutable)" = "kessel" ] \
        || die "Info.plist CFBundleExecutable is not kessel"
    [ "$(plist_value "$plist" CFBundlePackageType)" = "APPL" ] \
        || die "Info.plist CFBundlePackageType is not APPL"
    [ "$(plist_value "$plist" LSMinimumSystemVersion)" = "$MIN_MACOS" ] \
        || die "Info.plist minimum macOS is not $MIN_MACOS"
    [ "$(plist_value "$plist" LSApplicationCategoryType)" = "public.app-category.games" ] \
        || die "Info.plist category is not Games"
    local_text="$(plist_value "$plist" NSLocalNetworkUsageDescription)"
    [ -n "$local_text" ] || die "Info.plist lacks NSLocalNetworkUsageDescription"
    release_scope="$(plist_value "$plist" KesselReleaseScope)"
    case "$release_scope" in
        owner-dev|public) ;;
        *) die "Info.plist has an invalid/missing KesselReleaseScope" ;;
    esac
    if [ -n "$EXPECTED_BUNDLE_ID" ]; then
        [ "$(plist_value "$plist" CFBundleIdentifier)" = "$EXPECTED_BUNDLE_ID" ] \
            || die "Info.plist bundle identifier differs from $EXPECTED_BUNDLE_ID"
    fi
    if [ -n "$EXPECTED_VERSION" ]; then
        [ "$(plist_value "$plist" CFBundleShortVersionString)" = "$EXPECTED_VERSION" ] \
            || die "Info.plist version differs from $EXPECTED_VERSION"
    fi
    if [ -n "$EXPECTED_RELEASE_SCOPE" ]; then
        [ "$release_scope" = "$EXPECTED_RELEASE_SCOPE" ] \
            || die "Info.plist release scope differs from $EXPECTED_RELEASE_SCOPE"
    fi

    validate_binary "$binary"
    require_file "$assets/fonts/VT323-Regular.ttf"
    require_file "$assets/fonts/PressStart2P-Regular.ttf"
    require_file "$assets/fonts/LICENSES.md"
    require_file "$assets/fonts/OFL-1.1.txt"
    require_file "$assets/THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt"
    require_file "$assets/THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt"
    require_file "$assets/catalog/runtime_assets.v1.json"
    require_file "$assets/catalog/runtime_payload.v1.json"
    require_file "$assets/runtime-assets.manifest.json"
    require_file "$assets/runtime-payload.manifest.json"
    require_file "$assets/RUNTIME-PAYLOAD.sha256"
    require_file "$assets/MANIFEST.sha256"
    require_file "$assets/PACKAGE-MANIFEST.sha256"
    (cd "$assets" && /usr/bin/shasum -a 256 -c MANIFEST.sha256 >/dev/null) \
        || die "cooker model hash manifest failed"
    (cd "$assets" && /usr/bin/shasum -a 256 -c PACKAGE-MANIFEST.sha256 >/dev/null) \
        || die "package resource hash manifest failed"
    verify_runtime_manifest "$assets"
    verify_runtime_payload "$assets"
    payload_scope="$("$PYTHON_BIN" - "$assets/runtime-payload.manifest.json" <<'PY'
import json
from pathlib import Path
import sys
value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(value.get("release_scope", ""))
PY
)"
    [ "$payload_scope" = "$release_scope" ] \
        || die "Info.plist and runtime payload release scopes differ"

    /usr/bin/codesign --verify --deep --strict --verbose=2 "$app" \
        || die "codesign verification failed: $app"
    signature_info="$(/usr/bin/codesign -dv --verbose=4 "$app" 2>&1)"
    case "$signature_info" in
        *runtime*) ;;
        *) die "app signature does not enable hardened runtime" ;;
    esac
    note "verified app: $app"
}

verify_dmg() {
    local dmg="$1"
    require_file "$dmg"
    /usr/bin/hdiutil verify "$dmg" >/dev/null || die "DMG image verification failed: $dmg"
    /usr/bin/codesign --verify --verbose=2 "$dmg" || die "DMG signature verification failed: $dmg"
    note "verified DMG: $dmg"
}

generate_plist() {
    local destination="$1"
    "$PYTHON_BIN" - "$PLIST_TEMPLATE" "$destination" "$APP_NAME" "$BUNDLE_ID" "$VERSION" "$BUILD_VERSION" <<'PY'
from pathlib import Path
import sys
from xml.sax.saxutils import escape

template, destination, app_name, bundle_id, version, build_version = sys.argv[1:]
text = Path(template).read_text(encoding="utf-8")
values = {
    "@APP_NAME@": escape(app_name),
    "@BUNDLE_ID@": escape(bundle_id),
    "@VERSION@": escape(version),
    "@BUILD_VERSION@": escape(build_version),
}
for marker, value in values.items():
    text = text.replace(marker, value)
if "@" in text:
    raise SystemExit("unexpanded marker remains in Info.plist")
Path(destination).write_text(text, encoding="utf-8")
PY
    /usr/bin/plutil -lint "$destination" >/dev/null
    /usr/libexec/PlistBuddy -c "Add :KesselReleaseScope string $RELEASE_SCOPE" "$destination" \
        >/dev/null || die "cannot stamp KesselReleaseScope in Info.plist"
    /usr/bin/plutil -lint "$destination" >/dev/null
}

generate_package_manifest() {
    local assets_root="$1"
    local manifest="$assets_root/PACKAGE-MANIFEST.sha256"
    (
        cd "$assets_root"
        find . -type f ! -name PACKAGE-MANIFEST.sha256 -print | LC_ALL=C sort | \
            while IFS= read -r resource; do
                /usr/bin/shasum -a 256 "$resource"
            done
    ) > "$manifest"
    [ -s "$manifest" ] || die "resource hash manifest is empty"
}

sign_app() {
    local app="$1"
    if [ -n "$SIGN_IDENTITY" ]; then
        /usr/bin/codesign --force --options runtime --timestamp \
            --entitlements "$ENTITLEMENTS" --sign "$SIGN_IDENTITY" "$app"
    else
        /usr/bin/codesign --force --options runtime \
            --entitlements "$ENTITLEMENTS" --sign - "$app"
    fi
}

sign_dmg() {
    local dmg="$1"
    if [ -n "$SIGN_IDENTITY" ]; then
        /usr/bin/codesign --force --timestamp --sign "$SIGN_IDENTITY" "$dmg"
    else
        /usr/bin/codesign --force --sign - "$dmg"
    fi
}

cleanup_stage() {
    if [ -n "${STAGE_DIR:-}" ] && [ -d "$STAGE_DIR" ]; then
        case "$STAGE_DIR" in
            "${TMPDIR:-/tmp}"/kessel-macos.*) /bin/rm -rf -- "$STAGE_DIR" ;;
            *) printf 'package_macos: refusing unexpected cleanup path: %s\n' "$STAGE_DIR" >&2 ;;
        esac
    fi
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --out-dir)
            [ "$#" -ge 2 ] || die "--out-dir requires a value"
            OUT_DIR="$2"; shift 2 ;;
        --version)
            [ "$#" -ge 2 ] || die "--version requires a value"
            VERSION="$2"; shift 2 ;;
        --build-version)
            [ "$#" -ge 2 ] || die "--build-version requires a value"
            BUILD_VERSION="$2"; shift 2 ;;
        --bundle-id)
            [ "$#" -ge 2 ] || die "--bundle-id requires a value"
            BUNDLE_ID="$2"; shift 2 ;;
        --app-name)
            [ "$#" -ge 2 ] || die "--app-name requires a value"
            APP_NAME="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        --binary)
            [ "$#" -ge 2 ] || die "--binary requires a value"
            BINARY_PATH="$2"; BINARY_OVERRIDDEN=1; shift 2 ;;
        --offline) OFFLINE=1; shift ;;
        --no-dmg) NO_DMG=1; shift ;;
        --release-scope)
            [ "$#" -ge 2 ] || die "--release-scope requires owner-dev or public"
            RELEASE_SCOPE="$2"; shift 2 ;;
        --sign-identity)
            [ "$#" -ge 2 ] || die "--sign-identity requires a value"
            SIGN_IDENTITY="$2"; shift 2 ;;
        --entitlements)
            [ "$#" -ge 2 ] || die "--entitlements requires a value"
            ENTITLEMENTS="$2"; shift 2 ;;
        --notary-profile)
            [ "$#" -ge 2 ] || die "--notary-profile requires a value"
            NOTARY_PROFILE="$2"; shift 2 ;;
        --dry-run) DRY_RUN=1; shift ;;
        --validate-only)
            [ "$#" -ge 2 ] || die "--validate-only requires an .app path"
            VALIDATE_ONLY_APP="$2"; shift 2 ;;
        --verify-dmg)
            [ "$#" -ge 2 ] || die "--verify-dmg requires a DMG path"
            VERIFY_DMG="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown option: $1 (use --help)" ;;
    esac
done

[ "$(uname -s)" = "Darwin" ] || die "macOS is required"
case "$RELEASE_SCOPE" in
    owner-dev|public) ;;
    *) die "--release-scope must be owner-dev or public (found: $RELEASE_SCOPE)" ;;
esac
require_command awk
require_command find
require_command grep
require_command sort
require_file "$PLIST_TEMPLATE"
require_file "$ENTITLEMENTS"
require_file "$PAYLOAD_TOOL"
/usr/bin/plutil -lint "$ENTITLEMENTS" >/dev/null || die "entitlements plist is invalid"

PYTHON_BIN="$(command -v python3 || true)"
[ -n "$PYTHON_BIN" ] || die "python3 is required by the runtime asset cooker"
EXPECTED_BUNDLE_ID=""
EXPECTED_VERSION=""
EXPECTED_RELEASE_SCOPE=""

if [ -n "$VALIDATE_ONLY_APP" ]; then
    [ "$DRY_RUN" -eq 0 ] || die "--dry-run and --validate-only are mutually exclusive"
    EXPECTED_BUNDLE_ID=""
    EXPECTED_VERSION=""
    verify_app "$VALIDATE_ONLY_APP"
    if [ -n "$VERIFY_DMG" ]; then
        verify_dmg "$VERIFY_DMG"
    fi
    exit 0
fi

[ -z "$VERIFY_DMG" ] || die "--verify-dmg is valid only with --validate-only"
if [ -z "$VERSION" ]; then
    VERSION="$(awk -F '"' '/^version[[:space:]]*=/ { print $2; exit }' "$REPO_ROOT/Cargo.toml")"
fi
[ -n "$BUILD_VERSION" ] || BUILD_VERSION="$VERSION"
validate_metadata

require_file "$COOKER"
require_file "$CATALOG"
require_file "$PAYLOAD_CATALOG"
require_file "$FONT_REGULAR"
require_file "$FONT_BOLD"

if [ "$BINARY_OVERRIDDEN" -eq 1 ] && [ "$SKIP_BUILD" -eq 0 ]; then
    die "--binary requires --skip-build"
fi
if [ -n "$NOTARY_PROFILE" ] && [ -z "$SIGN_IDENTITY" ]; then
    die "notarization requires --sign-identity (ad-hoc signatures cannot be notarized)"
fi
if [ -n "$NOTARY_PROFILE" ] && [ "$RELEASE_SCOPE" != "public" ]; then
    die "notarization requires --release-scope public"
fi
if [ -n "$NOTARY_PROFILE" ] && [ "$NO_DMG" -eq 1 ]; then
    die "this scaffold notarizes the DMG; remove --no-dmg or omit --notary-profile"
fi

CARGO_BIN="$(find_cargo || true)"
if [ "$SKIP_BUILD" -eq 0 ]; then
    [ -n "$CARGO_BIN" ] || die "cargo is unavailable (install/select the stable Rust toolchain)"
    BINARY_PATH="$REPO_ROOT/target/$TARGET_TRIPLE/release/kessel"
else
    [ -n "$BINARY_PATH" ] || BINARY_PATH="$REPO_ROOT/target/$TARGET_TRIPLE/release/kessel"
    require_file "$BINARY_PATH"
    validate_binary "$BINARY_PATH"
fi

APP_OUT="$OUT_DIR/$APP_NAME.app"
DMG_BASENAME="$(printf '%s' "$APP_NAME" | tr ' ' '-')-$VERSION-arm64.dmg"
DMG_OUT="$OUT_DIR/$DMG_BASENAME"
DMG_SHA_OUT="$DMG_OUT.sha256"

if [ -L "$OUT_DIR" ]; then
    die "output directory may not be a symlink: $OUT_DIR"
fi
if [ -e "$OUT_DIR" ] && [ ! -d "$OUT_DIR" ]; then
    die "output path exists but is not a directory: $OUT_DIR"
fi
[ ! -e "$APP_OUT" ] && [ ! -L "$APP_OUT" ] \
    || die "refusing to overwrite existing app or symlink: $APP_OUT"
if [ "$NO_DMG" -eq 0 ]; then
    [ ! -e "$DMG_OUT" ] && [ ! -L "$DMG_OUT" ] \
        || die "refusing to overwrite existing DMG or symlink: $DMG_OUT"
    [ ! -e "$DMG_SHA_OUT" ] && [ ! -L "$DMG_SHA_OUT" ] \
        || die "refusing to overwrite existing checksum or symlink: $DMG_SHA_OUT"
fi

note "validating the reviewed runtime model closure (read-only)"
(cd "$REPO_ROOT" && "$PYTHON_BIN" "$COOKER" validate)
note "validating the locked Garage + three-act non-GLB runtime payload (read-only)"
(cd "$REPO_ROOT" && "$PYTHON_BIN" "$PAYLOAD_TOOL" validate \
    --release-scope "$RELEASE_SCOPE")

if [ "$DRY_RUN" -eq 1 ]; then
    note "DRY RUN: cataloged GLBs and the locked Garage + three-act payload are present and valid"
    if [ "$SKIP_BUILD" -eq 0 ]; then
        CARGO_ARGS=(build --release --locked --target "$TARGET_TRIPLE")
        [ "$OFFLINE" -eq 0 ] || CARGO_ARGS+=(--offline)
        print_command env "MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS" \
            "PATH=$(dirname "$CARGO_BIN"):$PATH" "$CARGO_BIN" "${CARGO_ARGS[@]}"
    else
        print_command "verify-existing-binary" "$BINARY_PATH"
    fi
    print_command "$PYTHON_BIN" "$COOKER" cook --output \
        "<staging>/$APP_NAME.app/Contents/Resources/assets"
    print_command "$PYTHON_BIN" "$PAYLOAD_TOOL" cook --output \
        "<staging>/$APP_NAME.app/Contents/Resources/assets" \
        --release-scope "$RELEASE_SCOPE"
    if [ -n "$SIGN_IDENTITY" ]; then
        print_command codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP_OUT"
    else
        print_command codesign --force --options runtime --sign - "$APP_OUT"
    fi
    print_command "verify-app" "$APP_OUT"
    if [ "$NO_DMG" -eq 0 ]; then
        print_command hdiutil create -format UDZO -volname "$APP_NAME" "$DMG_OUT"
        print_command "verify-dmg" "$DMG_OUT"
    fi
    if [ -n "$NOTARY_PROFILE" ]; then
        print_command xcrun notarytool submit "$DMG_OUT" --keychain-profile "$NOTARY_PROFILE" --wait
        print_command xcrun stapler staple "$DMG_OUT"
    fi
    note "DRY RUN outputs would be:"
    printf '  %s\n' "$APP_OUT"
    if [ "$NO_DMG" -eq 0 ]; then
        printf '  %s\n  %s\n' "$DMG_OUT" "$DMG_SHA_OUT"
    fi
    note "No files or build artifacts were created."
    exit 0
fi

if [ "$SKIP_BUILD" -eq 0 ]; then
    note "building thin arm64 release with MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS"
    CARGO_ARGS=(build --release --locked --target "$TARGET_TRIPLE")
    [ "$OFFLINE" -eq 0 ] || CARGO_ARGS+=(--offline)
    env "MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS" "PATH=$(dirname "$CARGO_BIN"):$PATH" \
        "$CARGO_BIN" "${CARGO_ARGS[@]}"
fi
validate_binary "$BINARY_PATH"

STAGE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/kessel-macos.XXXXXX")"
trap cleanup_stage EXIT
trap 'exit 130' HUP INT TERM
APP_STAGE="$STAGE_DIR/$APP_NAME.app"
CONTENTS="$APP_STAGE/Contents"
ASSETS_STAGE="$CONTENTS/Resources/assets"
/bin/mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
/usr/bin/install -m 0755 "$BINARY_PATH" "$CONTENTS/MacOS/kessel"
generate_plist "$CONTENTS/Info.plist"

note "cooking reviewed runtime GLBs into the bundle"
(cd "$REPO_ROOT" && "$PYTHON_BIN" "$COOKER" cook --output "$ASSETS_STAGE")
note "copying the locked Garage + three-act non-GLB payload at logical asset paths"
(cd "$REPO_ROOT" && "$PYTHON_BIN" "$PAYLOAD_TOOL" cook --output "$ASSETS_STAGE" \
    --release-scope "$RELEASE_SCOPE")
/bin/mkdir -p "$ASSETS_STAGE/catalog"
/usr/bin/install -m 0644 "$CATALOG" "$ASSETS_STAGE/catalog/runtime_assets.v1.json"
generate_package_manifest "$ASSETS_STAGE"

EXPECTED_BUNDLE_ID="$BUNDLE_ID"
EXPECTED_VERSION="$VERSION"
EXPECTED_RELEASE_SCOPE="$RELEASE_SCOPE"
sign_app "$APP_STAGE"
verify_app "$APP_STAGE"

/bin/mkdir -p "$OUT_DIR"
[ ! -L "$OUT_DIR" ] && [ -d "$OUT_DIR" ] \
    || die "output directory became a symlink or non-directory: $OUT_DIR"
/usr/bin/ditto "$APP_STAGE" "$APP_OUT"
verify_app "$APP_OUT"

if [ "$NO_DMG" -eq 0 ]; then
    DMG_ROOT="$STAGE_DIR/dmg-root"
    DMG_STAGE="$STAGE_DIR/$DMG_BASENAME"
    /bin/mkdir -p "$DMG_ROOT"
    /usr/bin/ditto "$APP_STAGE" "$DMG_ROOT/$APP_NAME.app"
    /bin/ln -s /Applications "$DMG_ROOT/Applications"
    /usr/bin/hdiutil create -quiet -format UDZO -imagekey zlib-level=9 \
        -volname "$APP_NAME" -srcfolder "$DMG_ROOT" "$DMG_STAGE"
    sign_dmg "$DMG_STAGE"
    verify_dmg "$DMG_STAGE"

    if [ -n "$NOTARY_PROFILE" ]; then
        note "submitting DMG for notarization"
        /usr/bin/xcrun notarytool submit "$DMG_STAGE" --keychain-profile "$NOTARY_PROFILE" --wait
        /usr/bin/xcrun stapler staple "$DMG_STAGE"
        /usr/bin/xcrun stapler validate "$DMG_STAGE"
    fi

    /usr/bin/ditto "$DMG_STAGE" "$DMG_OUT"
    (
        cd "$OUT_DIR"
        /usr/bin/shasum -a 256 "$DMG_BASENAME" > "$DMG_BASENAME.sha256"
        /usr/bin/shasum -a 256 -c "$DMG_BASENAME.sha256" >/dev/null
    )
    verify_dmg "$DMG_OUT"
fi

note "DONE"
printf '  App: %s\n' "$APP_OUT"
if [ "$NO_DMG" -eq 0 ]; then
    printf '  DMG: %s\n  SHA-256: %s\n' "$DMG_OUT" "$DMG_SHA_OUT"
fi
if [ "$RELEASE_SCOPE" = "owner-dev" ]; then
    note "This is an owner/developer artifact. Do not describe or send it as a friend/public build."
elif [ -z "$SIGN_IDENTITY" ]; then
    note "Public-scope clearance passed, but this ad-hoc signature still causes Gatekeeper friction."
fi
note "Release scope: $RELEASE_SCOPE"
