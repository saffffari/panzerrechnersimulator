#!/usr/bin/env bash
# Current winter study, with Singleplayer and Co-op buttons in its title menu.
# --urban plays the urban-intro district around the spawn instead of the winter courtyard
# (docs/URBAN_INTRO_20260928.md); generate it first with python3 tools/worldgen/urban_intro_district.py.
# Every other option passes through to launch_shader_playtest.sh.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json"
PASS=()
for arg in "$@"; do
    case "$arg" in
        --urban) MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json" ;;
        *) PASS+=("$arg") ;;
    esac
done
if [[ ! -f "$MANIFEST" ]]; then
    echo "missing $MANIFEST" >&2
    echo 'generate the urban workshop with: python3 tools/worldgen/urban_intro_district.py' >&2
    exit 2
fi
export KESSEL_PLAYTEST_MANIFEST="$MANIFEST"
export KESSEL_INTERNAL_RES=1280x720
export KESSEL_WINTER_COMBAT=1
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --menu ${PASS[@]+"${PASS[@]}"}
