#!/usr/bin/env bash
# Open one reusable historical assembly with its saved review camera.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
SCENE=${1:-household}
case "$SCENE" in
    household|ward|gumrak) ;;
    *) echo 'Usage: launch_historical_editor.sh [household|ward|gumrak] [--check|--build]' >&2; exit 2 ;;
esac
if (( $# )); then shift; fi
mapfile -t SETTINGS < <(python3 - "$PROJECT_ROOT" "$SCENE" <<'PY'
import json,sys
from pathlib import Path
root=Path(sys.argv[1])
index=json.loads((root/'artifacts/authoring/historical-20260925/workshops.json').read_text())
scene=next(s for s in index['scenes'] if s['slug']==sys.argv[2])
manifest=root/scene['manifest']
assert manifest.is_file(), manifest
view=scene['views'][0]
print(manifest)
print(','.join(str(x) for x in view['eye_m']+view['target_m']))
PY
)
[[ ${#SETTINGS[@]} == 2 ]] || { echo 'Historical workshop index is missing or invalid.' >&2; exit 1; }
export KESSEL_PLAYTEST_MANIFEST=${SETTINGS[0]}
export KESSEL_EDITOR_START_VIEW=${SETTINGS[1]}
export PANZER_EDITOR_LIVE_DIR="$PROJECT_ROOT/artifacts/authoring/live-historical-$SCENE-20260925"
export KESSEL_INTERNAL_RES=960x540
unset KESSEL_WINTER_COMBAT
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --editor "$@"
