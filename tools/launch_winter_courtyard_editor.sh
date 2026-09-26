#!/usr/bin/env bash
# Open the saved Act 1 winter study using the shader worktree's ordinary single-window launcher.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
export KESSEL_PLAYTEST_MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json"
export PANZER_EDITOR_LIVE_DIR="$PROJECT_ROOT/artifacts/authoring/live-winter-20260925"
export KESSEL_INTERNAL_RES=960x540
export KESSEL_EDITOR_START_VIEW=1861,10,198,1880,8,165
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --editor "$@"
