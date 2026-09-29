#!/usr/bin/env bash
# Current winter study, with Singleplayer and Co-op buttons in its title menu.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
export KESSEL_PLAYTEST_MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json"
export KESSEL_INTERNAL_RES=1280x720
export KESSEL_WINTER_COMBAT=1
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --menu "$@"
