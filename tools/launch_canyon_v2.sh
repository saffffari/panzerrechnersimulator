#!/usr/bin/env bash
# Isolated populated Canyon v2. Default: title menu; --play starts a fresh v2 run.
set -euo pipefail
PANZER_V2_ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$PANZER_V2_ROOT"
if [[ "${1:-}" == --check ]]; then
    test -x target/release/panzerrechnersystem
    test -s assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json
    test -s catalog/maps/canyon_v2_survey_20260930.json
    test -s catalog/maps/canyon_v2_survey_20260930.png
    printf 'Canyon v2 ready: %s\n' "$PANZER_V2_ROOT"
    exit 0
fi
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then source "$HOME/.cargo/env"; fi
mkdir -p captures
if ! cargo build --offline --locked --release --bin panzerrechnersystem >captures/canyon_v2_launch_build.log 2>&1; then
    command -v notify-send >/dev/null 2>&1 && notify-send -u critical 'Canyon v2 build failed' "$PANZER_V2_ROOT/captures/canyon_v2_launch_build.log"
    exit 1
fi
if [[ ! -f assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json ]]; then
    python tools/worldgen/gaea_canyon_v2.py
fi
if [[ ! -f catalog/maps/canyon_v2_survey_20260930.json || ! -f catalog/maps/canyon_v2_survey_20260930.png ]]; then
    python tools/worldgen/canyon_paper_map.py --source assets/worldgen/workshops/gaea-canyon-run-v2 \
        --output catalog/maps/canyon_v2_survey_20260930
fi
export KESSEL_RENDERER=shader
export KESSEL_GRAPHICS_PRESET="${KESSEL_GRAPHICS_PRESET:-high}"
export KESSEL_INTERNAL_RES="${KESSEL_INTERNAL_RES:-1280x720}"
export KESSEL_CANYON_V2=1
unset KESSEL_CANYON_MANIFEST KESSEL_CANYON KESSEL_COOP KESSEL_SINGLEPLAYER KESSEL_PLAYTEST KESSEL_HOP_ACT
if [[ "${1:-}" == --play ]]; then export KESSEL_CANYON=1; shift; fi
exec target/release/panzerrechnersystem "$@"
