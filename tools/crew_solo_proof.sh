#!/usr/bin/env bash
# Run the headless solo-crew proof on Windows (Git Bash) with the launcher's shader backend and
# the repository's own runtime folders. Usage: tools/crew_solo_proof.sh [PREFIX] [ASSET_ROOT]
#   PREFIX      capture prefix (default captures/crew_solo_<date>)
#   ASSET_ROOT  checkout holding the full asset library (default: this checkout)
set -euo pipefail
cd "$(dirname "$0")/.."
PREFIX="${1:-captures/crew_solo_$(date +%Y%m%d)}"
ASSET_ROOT="${2:-$PWD}"
mkdir -p artifacts/runtime/home captures
rm -f "${PREFIX}"*.png "${PREFIX}.json"
env HOME="$PWD/artifacts/runtime/home" \
    XDG_DATA_HOME="$PWD/artifacts/runtime/data" \
    XDG_CACHE_HOME="$PWD/artifacts/runtime/cache" \
    XDG_CONFIG_HOME="$PWD/artifacts/runtime/config" \
    KESSEL_ASSET_ROOT="$ASSET_ROOT" \
    KESSEL_RENDERER=shader KESSEL_GRAPHICS_PRESET=high \
    KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 \
    target/release/panzerrechnersystem.exe crew-solo "$PREFIX"
