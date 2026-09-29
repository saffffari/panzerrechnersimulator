#!/usr/bin/env bash
# Current winter study, with Singleplayer and Co-op buttons in its title menu.
# --urban plays the urban-intro district around the spawn instead of the winter courtyard
# (docs/URBAN_INTRO_20260928.md). Its workshop is generated, not tracked: when it is missing, the
# deterministic district tool (tools/worldgen/urban_intro_district.py) writes it before the launch
# (about 15 s, byte-identical on every run; with --check it is only reported).
# Every other option passes through to launch_shader_playtest.sh.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
URBAN_MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json"
MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json"
CHECK=0
PASS=()
for arg in "$@"; do
    case "$arg" in
        --urban) MANIFEST="$URBAN_MANIFEST" ;;
        --check) CHECK=1; PASS+=("$arg") ;;
        *) PASS+=("$arg") ;;
    esac
done
if [[ "$MANIFEST" == "$URBAN_MANIFEST" && ! -f "$MANIFEST" ]] && (( ! CHECK )); then
    echo "urban workshop missing; generating it with tools/worldgen/urban_intro_district.py" >&2
    python3 "$PROJECT_ROOT/tools/worldgen/urban_intro_district.py" >&2
fi
if [[ ! -f "$MANIFEST" ]]; then
    echo "missing $MANIFEST" >&2
    if [[ "$MANIFEST" == "$URBAN_MANIFEST" ]]; then
        echo 'generate the urban workshop with: python3 tools/worldgen/urban_intro_district.py' >&2
    fi
    exit 2
fi
export KESSEL_PLAYTEST_MANIFEST="$MANIFEST"
export KESSEL_INTERNAL_RES=1280x720
export KESSEL_WINTER_COMBAT=1
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --menu ${PASS[@]+"${PASS[@]}"}
