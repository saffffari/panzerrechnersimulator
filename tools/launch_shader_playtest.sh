#!/usr/bin/env bash
# Branch-only launcher. Builds, logs, checkpoints and default captures stay in this worktree.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
BRANCH_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
CANONICAL_ROOT=/home/alex/panzer/tank_simulator
BUILD=0
CHECK=0
MODE=playtest
GARAGE_TANK=tiger_i
while (( $# )); do
    case "$1" in
        --build) BUILD=1 ;;
        --playtest) MODE=playtest ;;
        --menu) MODE=menu ;;
        --editor) MODE=editor ;;
        --garage) MODE=garage; shift; GARAGE_TANK="${1:?--garage requires tiger_i, tiger_ii, is2, t34_76 or panzer_iv}" ;;
        --check) CHECK=1 ;;
        --help|-h)
            echo 'Usage: launch_shader_playtest.sh [--build] [--playtest|--menu|--editor|--garage TANK] [--check]'
            echo '--check performs read-only asset/path checks; it never builds or launches.'
            exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done
if [[ "$BRANCH_ROOT" == "$CANONICAL_ROOT" ]]; then
    echo 'This launcher must run from the separate shader worktree.' >&2
    exit 2
fi
cd "$BRANCH_ROOT"
export CARGO_TARGET_DIR="$BRANCH_ROOT/target"
export KESSEL_RENDERER=shader
export KESSEL_ASSET_ROOT="$CANONICAL_ROOT"
DEFAULT_PLAYTEST_MANIFEST="$BRANCH_ROOT/assets/worldgen/revisions/act1-winter-approach/20260913-v1/manifest.v1.json"
export KESSEL_PLAYTEST_MANIFEST="${KESSEL_PLAYTEST_MANIFEST:-$DEFAULT_PLAYTEST_MANIFEST}"
export KESSEL_INTERNAL_RES="${KESSEL_INTERNAL_RES:-576x324}"
export KESSEL_GRAPHICS_PRESET=high
export KESSEL_ENABLE_PIZZA_AUDIO="${KESSEL_ENABLE_PIZZA_AUDIO-1}"
# The branch menu must not overwrite the existing campaign checkpoint.
export XDG_STATE_HOME="$BRANCH_ROOT/artifacts/shader-state"
if [[ "$MODE" == playtest ]]; then export KESSEL_PLAYTEST=1; else unset KESSEL_PLAYTEST; fi
python3 "$SCRIPT_DIR/check_shader_runtime.py" --asset-root "$KESSEL_ASSET_ROOT"
if (( CHECK )); then exit 0; fi

mkdir -p "$BRANCH_ROOT/artifacts/runtime"
exec 9>"$BRANCH_ROOT/artifacts/runtime/shader-playtest.lock"
if ! flock -n 9; then
    echo 'The shader Playtest launcher is already running.' >&2
    exit 2
fi
# Respect the existing launcher, too. Open its already-existing lock read-only and hold it for
# this run, so neither launcher can start a second game while the other owns the GPU/window.
CANONICAL_LOCK="$CANONICAL_ROOT/artifacts/runtime/panzerrechnersystem.lock"
if [[ -f "$CANONICAL_LOCK" ]]; then
    exec 8<"$CANONICAL_LOCK"
    if ! flock -n 8; then
        echo 'The current Playtest is running. Close it before launching the shader branch.' >&2
        exit 2
    fi
else
    echo "Existing Playtest lock missing: $CANONICAL_LOCK" >&2
    exit 2
fi
if (( BUILD )); then
    if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then source "$HOME/.cargo/env"; fi
    cargo build --release --locked --bin panzerrechnersystem
fi
BINARY="$CARGO_TARGET_DIR/release/panzerrechnersystem"
if [[ ! -x "$BINARY" ]]; then
    echo "Shader binary missing. Build with: $SCRIPT_DIR/launch_shader_playtest.sh --build --$MODE" >&2
    exit 1
fi
if [[ "$BINARY" -ef "$CANONICAL_ROOT/target/release/panzerrechnersystem" ]]; then
    echo 'Refusing the canonical binary; the shader branch requires its own build.' >&2
    exit 2
fi
LOG="$BRANCH_ROOT/artifacts/runtime/shader-playtest-$(date +%Y%m%d-%H%M%S)-$$.log"
echo "Shader Playtest log: $LOG"
GAME_ARGS=()
if [[ "$MODE" == editor ]]; then GAME_ARGS=(edit "$KESSEL_PLAYTEST_MANIFEST"); fi
if [[ "$MODE" == garage ]]; then GAME_ARGS=(edit "$GARAGE_TANK"); fi
"$BINARY" "${GAME_ARGS[@]}" >"$LOG" 2>&1 &
GAME_PID=$!
trap 'kill -TERM "$GAME_PID" 2>/dev/null || true' TERM INT
STATUS=0
wait "$GAME_PID" || STATUS=$?
if (( STATUS != 0 )); then echo "Shader Playtest exited with status $STATUS; see $LOG" >&2; fi
exit "$STATUS"
