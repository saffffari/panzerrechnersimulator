#!/usr/bin/env bash
# Desktop-shortcut entry for this checkout: build it if the code changed, then start the winter
# map (singleplayer by default; pass --menu for the title menu). A shortcut has no terminal, so a
# slow build and any failure are reported as desktop notifications with the log path.
set -uo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
MODE="${1:---singleplayer}"
notify() { command -v notify-send >/dev/null 2>&1 && notify-send -a Panzerrechnersystem -i panzerrechnersystem "$@"; }
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then source "$HOME/.cargo/env"; fi
mkdir -p "$ROOT/artifacts/runtime"
STAMP=$(date +%Y%m%d-%H%M%S)
BUILD_LOG="$ROOT/artifacts/runtime/current-build-$STAMP.log"
cd "$ROOT" || exit 1
CARGO_TARGET_DIR="$ROOT/target" cargo build --release --locked --bin panzerrechnersystem >"$BUILD_LOG" 2>&1 &
BUILD_PID=$!
for _ in 1 2 3 4; do
    kill -0 "$BUILD_PID" 2>/dev/null || break
    sleep 0.5
done
if kill -0 "$BUILD_PID" 2>/dev/null; then
    notify "Building the current build…" "$(git rev-parse --abbrev-ref HEAD 2>/dev/null) — the game opens when it finishes"
fi
if ! wait "$BUILD_PID"; then
    notify -u critical "Build failed" "$BUILD_LOG"
    exit 1
fi
LAUNCH_LOG="$ROOT/artifacts/runtime/current-launch-$STAMP.log"
"$SCRIPT_DIR/launch_winter_session.sh" "$MODE" >"$LAUNCH_LOG" 2>&1
STATUS=$?
if (( STATUS != 0 )); then
    notify -u critical "The game did not start or exited with status $STATUS" "$(tail -n 2 "$LAUNCH_LOG")"
fi
exit "$STATUS"
