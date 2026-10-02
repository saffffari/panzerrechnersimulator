#!/usr/bin/env bash
# MAIN release only. Never builds, regenerates assets, or falls back to another checkout.
# First launch: 800x450, screen Bayer 8, MSAA 4, high quality, fullscreen.
# Later graphics-menu choices persist in this launcher's separate settings file.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: launch_winter_playtest.sh [--play | --menu | --check | --help]
  --play   Start a fresh Canyon v2 run (default).
  --menu   Open the title menu, including graphics settings and Canyon v2.
  --check  Read-only release/asset/config checks; never build or start the game.
Graphics settings persist under artifacts/runtime/winter-playtest-state.
EOF
}
MODE=play
if (( $# > 1 )); then usage >&2; exit 2; fi
case "${1:---play}" in
    --play) ;;
    --menu) MODE=menu ;;
    --check) MODE=check ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
esac

readonly PANZER_MAIN_ROOT=/home/alex/panzer/canyon-v2-20260930
readonly PANZER_BINARY="$PANZER_MAIN_ROOT/target/release/panzerrechnersystem"
readonly PANZER_MANIFEST="$PANZER_MAIN_ROOT/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json"
readonly PANZER_RUNTIME="$PANZER_MAIN_ROOT/artifacts/runtime"
readonly PANZER_CONFIG="$PANZER_RUNTIME/winter-playtest-state/graphics.v1.json"
for dependency in python3 sha256sum flock realpath; do
    command -v "$dependency" >/dev/null || { printf 'Missing dependency: %s\n' "$dependency" >&2; exit 1; }
done
SCRIPT_PATH=$(realpath -- "${BASH_SOURCE[0]}")
if [[ "$SCRIPT_PATH" != "$PANZER_MAIN_ROOT/tools/launch_winter_playtest.sh" ]]; then
    printf 'This launcher must remain in MAIN: %s/tools/launch_winter_playtest.sh\n' "$PANZER_MAIN_ROOT" >&2
    exit 1
fi
cd "$PANZER_MAIN_ROOT"
if [[ ! -f "$PANZER_BINARY" || ! -x "$PANZER_BINARY" || -L "$PANZER_BINARY" ]]; then
    printf 'MAIN release is missing, not executable, or a symlink: %s\nBuild and validate MAIN before launching.\n' "$PANZER_BINARY" >&2
    exit 1
fi

# Validation is deliberately read-only, including when no runtime directory exists.
python3 - "$PANZER_MAIN_ROOT" "$PANZER_BINARY" "$PANZER_MANIFEST" "$PANZER_CONFIG" <<'PY'
import json
import sys
from pathlib import Path

root, binary, manifest, config = map(Path, sys.argv[1:])
with binary.open('rb') as stream:
    if stream.read(4) != b'\x7fELF':
        raise SystemExit(f'Not a native Linux release binary: {binary}')
doc = json.loads(manifest.read_text())
if doc.get('act_id') != 1 or doc.get('terrain', {}).get('grass_season') != 'winter':
    raise SystemExit(f'Expected the authored winter Act-I Canyon v2 manifest: {manifest}')
required = [manifest.parent / doc['terrain']['heights']]
required.extend(root / value for value in doc['assets'])
required.extend(root / doc['terrain'][key] for key in ('texture', 'rock_map', 'snow_map', 'ground_class'))
required.extend(root / f'assets/tanks/tiger/baked/tiger_{part}.glb' for part in ('hull', 'turret', 'gun'))
required.extend(root / f'catalog/maps/canyon_v2_survey_20260930.{suffix}' for suffix in ('json', 'png'))
missing = [str(path) for path in required if not path.is_file() or path.stat().st_size == 0]
if missing:
    raise SystemExit('Missing required MAIN assets:\n' + '\n'.join(missing))
if config.exists():
    settings = json.loads(config.read_text())
    if not isinstance(settings, dict) or settings.get('version') != 1:
        raise SystemExit(f'Unsupported saved graphics config: {config}')
    print(f'Saved preferences preserved: {config}')
    print(f"Saved display: {settings.get('resolution')}, fullscreen={settings.get('fullscreen')}, dither={settings.get('dither_pattern')}")
else:
    print('First-launch display: 800x450, screen_bayer8, fullscreen; settings not created by --check.')
print(f'Validated {len(required)} required asset paths; no rendering or gameplay validation performed.')
PY
PANZER_BINARY_SHA=$(sha256sum -- "$PANZER_BINARY")
printf 'MAIN release: %s\nSHA256: %s\n' "$PANZER_BINARY" "${PANZER_BINARY_SHA%% *}"
if [[ "$MODE" == check ]]; then exit 0; fi

mkdir -p "$PANZER_RUNTIME/winter-playtest-state"
exec 9>"$PANZER_RUNTIME/panzerrechnersystem.lock"
if ! flock -n 9; then
    printf 'A MAIN game launched through the standard launcher is already running.\n' >&2
    if command -v notify-send >/dev/null; then notify-send 'Winter playtest' 'A MAIN game is already running.' || true; fi
    exit 1
fi

# Initialize once. Do not overwrite settings subsequently saved through the game menu.
python3 - "$PANZER_CONFIG" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists():
    initial = {
        'version': 1, 'resolution': [800, 450], 'window': [1280, 720],
        'fullscreen': True, 'vsync': True, 'shadows': 2048,
        'full_transport': True, 'optics': 1, 'fps': False,
        'exposure_ev': 0.0, 'saturation': 1.0, 'warmth': 0.0, 'gamma': 1.0,
        'storm': None, 'sun_phase': 0, 'sun_speed': 1.0, 'dither_pattern': 3,
        'display': {'width': 800, 'height': 450, 'palette_levels': 31.0,
                    'ordered_dither': 1.0, 'retro_strength': 1.0, 'ldr_dither': 1.0,
                    'ambient_white_balance': False, 'ambient_white_balance_strength': 1.0},
    }
    temporary = path.with_suffix('.json.init')
    temporary.write_text(json.dumps(initial, indent=2) + '\n')
    temporary.replace(path)
PY

export KESSEL_ASSET_ROOT="$PANZER_MAIN_ROOT"
export KESSEL_RENDERER=shader KESSEL_GRAPHICS_PRESET=high
export KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8
export KESSEL_GEOMETRY_AA=msaa4 KESSEL_FULLSCREEN=1
export KESSEL_SOURCE_VISIBILITY=maps
export KESSEL_GRAPHICS_CONFIG="$PANZER_CONFIG"
export KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_TRANSPORT_QUALITY=full
export KESSEL_CANYON_V2=1
unset KESSEL_CANYON KESSEL_CANYON_MANIFEST KESSEL_CANYON_SEED
unset KESSEL_COOP KESSEL_SINGLEPLAYER KESSEL_PLAYTEST KESSEL_PLAYTEST_MANIFEST
unset KESSEL_HOP_ACT KESSEL_HOP_POS KESSEL_HOP_MANIFEST KESSEL_WINTER_COMBAT
if [[ "$MODE" == play ]]; then export KESSEL_CANYON=1; fi

PANZER_LOG="$PANZER_RUNTIME/winter-playtest-$(date +%Y%m%d-%H%M%S)-$$.log"
{
    printf '[launcher] timestamp=%s mode=%s launcher_pid=%s\n' "$(date --iso-8601=seconds)" "$MODE" "$$"
    printf '[launcher] binary=%s sha256=%s\n' "$PANZER_BINARY" "${PANZER_BINARY_SHA%% *}"
    printf '[launcher] asset_root=%s manifest=%s\n' "$KESSEL_ASSET_ROOT" "$PANZER_MANIFEST"
    printf '[launcher] renderer=shader preset=high geometry_aa=msaa4 visibility=maps config=%s\n' "$PANZER_CONFIG"
    printf '[launcher] saved graphics preferences follow:\n'
    cat -- "$PANZER_CONFIG"
} >"$PANZER_LOG"
ln -sfn "$(basename -- "$PANZER_LOG")" "$PANZER_RUNTIME/winter-playtest-latest.log"
"$PANZER_BINARY" >>"$PANZER_LOG" 2>&1 &
PANZER_GAME_PID=$!
printf '[launcher] game_pid=%s\n' "$PANZER_GAME_PID" >>"$PANZER_LOG"
printf 'Winter playtest PID %s; log: %s\n' "$PANZER_GAME_PID" "$PANZER_LOG"
trap 'kill -TERM "$PANZER_GAME_PID" 2>/dev/null || true' TERM INT
PANZER_STATUS=0
wait "$PANZER_GAME_PID" || PANZER_STATUS=$?
printf '[launcher] exit_status=%s timestamp=%s\n' "$PANZER_STATUS" "$(date --iso-8601=seconds)" >>"$PANZER_LOG"
if (( PANZER_STATUS != 0 )); then
    printf 'Winter playtest exited with status %s. Log: %s\n' "$PANZER_STATUS" "$PANZER_LOG" >&2
    if command -v notify-send >/dev/null; then notify-send -u critical 'Winter playtest stopped' "$PANZER_LOG" || true; fi
fi
exit "$PANZER_STATUS"
