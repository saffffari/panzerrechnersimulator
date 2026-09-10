#!/usr/bin/env bash
# Launch the verified local release from its asset root. Use --build after source changes.
set -euo pipefail
SOURCE_PATH=${BASH_SOURCE[0]}
while [[ -L "$SOURCE_PATH" ]]; do
    SOURCE_DIR=$(CDPATH= cd -P -- "$(dirname -- "$SOURCE_PATH")" && pwd)
    LINK_TARGET=$(readlink "$SOURCE_PATH")
    if [[ "$LINK_TARGET" = /* ]]; then SOURCE_PATH=$LINK_TARGET; else SOURCE_PATH=$SOURCE_DIR/$LINK_TARGET; fi
done
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "$SOURCE_PATH")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

if [[ "${1:-}" == --videos ]]; then
    exec xdg-open "$REPO_ROOT/captures/astra/first_person_library_20260905.html"
fi

# Existing environment identifiers remain compatible with development recipes.
export KESSEL_GRAPHICS_PRESET="${KESSEL_GRAPHICS_PRESET:-high}"
export KESSEL_ENABLE_PIZZA_AUDIO="${KESSEL_ENABLE_PIZZA_AUDIO-1}"
# Validated solo play budget: sharp pixel-art output with stable optical lighting.
# Developers can still opt into full transport or another internal size explicitly.
export KESSEL_TRANSPORT_QUALITY="${KESSEL_TRANSPORT_QUALITY:-gameplay}"
export KESSEL_INTERNAL_RES="${KESSEL_INTERNAL_RES:-640x360}"
export KESSEL_DIFFUSE_RECONSTRUCTION="${KESSEL_DIFFUSE_RECONSTRUCTION:-1}"
if [[ "${1:-}" == --build ]]; then
    shift
    if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then source "$HOME/.cargo/env"; fi
    cargo build --release --locked --bin panzerrechnersystem
fi
BINARY="$REPO_ROOT/target/release/panzerrechnersystem"
if [[ ! -x "$BINARY" ]]; then
    MESSAGE="Panzerrechnersystem needs a release build. Run: $SCRIPT_DIR/launch_panzerrechnersystem.sh --build"
    echo "$MESSAGE" >&2
    if command -v kdialog >/dev/null 2>&1; then kdialog --title Panzerrechnersystem --error "$MESSAGE"; fi
    exit 1
fi
if [[ "${1:-}" == --playtest ]]; then
    export KESSEL_PLAYTEST=1
    shift
elif [[ "${1:-}" == --menu ]]; then
    unset KESSEL_PLAYTEST
    shift
fi
# Diagnostic commands keep their normal terminal output and may run independently.
if (( $# )); then exec "$BINARY" "$@"; fi
mkdir -p "$REPO_ROOT/artifacts/runtime"
exec 9>"$REPO_ROOT/artifacts/runtime/panzerrechnersystem.lock"
if ! flock -n 9; then
    if command -v notify-send >/dev/null 2>&1; then notify-send Panzerrechnersystem 'The game is already running.'; fi
    exit 0
fi
LOG="$REPO_ROOT/artifacts/runtime/panzerrechnersystem-$(date +%Y%m%d-%H%M%S)-$$.log"
ln -sfn "$(basename "$LOG")" "$REPO_ROOT/artifacts/runtime/latest.log"
"$BINARY" >"$LOG" 2>&1 &
GAME_PID=$!
trap 'kill -TERM "$GAME_PID" 2>/dev/null || true' TERM INT
STATUS=0
wait "$GAME_PID" || STATUS=$?
if (( STATUS != 0 )); then
    MESSAGE="Panzerrechnersystem exited with status $STATUS. Launch log: $LOG"
    echo "$MESSAGE" >&2
    if command -v kdialog >/dev/null 2>&1; then kdialog --title Panzerrechnersystem --error "$MESSAGE"; fi
fi
exit "$STATUS"
