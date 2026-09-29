#!/usr/bin/env bash
# Desktop-shortcut entry for this checkout: build it if the code changed, then start the winter
# map (singleplayer by default; pass --menu for the title menu). Every argument is forwarded to
# launch_winter_session.sh, so `--singleplayer --urban` plays the urban intro district (its
# workshop is generated first if it is missing); without a mode argument, --singleplayer is used.
# `--menu --campaign` opens the title menu on the first level (the Act 1 campaign in the district).
# A shortcut has no terminal, so a slow build and any failure are reported as desktop
# notifications with the log path.
set -uo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
MODE_GIVEN=0
URBAN=0
for arg in "$@"; do
    case "$arg" in
        --menu|--singleplayer|--coop|--playtest|--editor|--garage) MODE_GIVEN=1 ;;
        --urban|--campaign) URBAN=1 ;;
    esac
done
(( MODE_GIVEN )) || set -- --singleplayer "$@"
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
if (( URBAN )) && [[ ! -f "$ROOT/assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json" ]]; then
    notify "Generating the urban intro district…" "first launch only — the game opens when it finishes"
fi
LAUNCH_LOG="$ROOT/artifacts/runtime/current-launch-$STAMP.log"
"$SCRIPT_DIR/launch_winter_session.sh" "$@" >"$LAUNCH_LOG" 2>&1
STATUS=$?
if (( STATUS != 0 )); then
    notify -u critical "The game did not start or exited with status $STATUS" "$(tail -n 2 "$LAUNCH_LOG")"
fi
exit "$STATUS"
