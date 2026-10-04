#!/usr/bin/env bash
# Canyon run from THIS checkout's own release build, with the opt-in session log enabled.
# Same fixed 12900K / RTX 4090 profile as launch_fixed_playtest.sh. Builds nothing.
#
#   KESSEL_ASSET_ROOT   asset checkout (default: the checkout that owns this worktree, else here)
#   PANZER_BINARY       release executable (default: $CARGO_TARGET_DIR or ./target)
#   PANZER_LOG_DIR      where the session and console logs go (default: <asset root>/artifacts/runtime)
set -euo pipefail
case "${1:---play}" in
  --play) PANZER_MODE=play ;;
  --menu) PANZER_MODE=menu ;;
  *) printf 'Usage: %s [--play|--menu]\n' "$0" >&2; exit 2 ;;
esac
readonly HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
# A git worktree under <checkout>/.claude/worktrees/<name> has no large untracked assets of its own.
DEFAULT_ASSETS=$HERE
if [[ ! -d "$HERE/assets/audio/library" && "$HERE" == */.claude/worktrees/* ]]; then
  DEFAULT_ASSETS=${HERE%%/.claude/worktrees/*}
fi
readonly ASSETS=${KESSEL_ASSET_ROOT:-$DEFAULT_ASSETS}
readonly BINARY=${PANZER_BINARY:-${CARGO_TARGET_DIR:-$HERE/target}/release/panzerrechnersystem}
readonly LOG_DIR=${PANZER_LOG_DIR:-$ASSETS/artifacts/runtime}
readonly MANIFEST="$ASSETS/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json"
[[ -x "$BINARY" ]] || { printf 'No release executable at %s\n' "$BINARY" >&2; exit 1; }
[[ -f "$MANIFEST" ]] || { printf 'No canyon manifest at %s\n' "$MANIFEST" >&2; exit 1; }
mkdir -p "$LOG_DIR"
STAMP=$(date +%Y%m%d-%H%M%S)
# Remove inherited experiment flags, then set the fixed profile.
while IFS= read -r NAME; do unset "$NAME"; done < <(compgen -A variable KESSEL_)
export KESSEL_ASSET_ROOT="$ASSETS" KESSEL_PLAYTEST_MANIFEST="$MANIFEST"
export KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_GRAPHICS_PRESET=high
export KESSEL_FIXED_PLAYTEST_GRAPHICS=1 KESSEL_INTERNAL_RES=800x450 KESSEL_FULLSCREEN=1
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4
export KESSEL_SOURCE_VISIBILITY=maps KESSEL_TRANSPORT_QUALITY=full
export KESSEL_RETAIN_RIGID_SKY_DEPTH=0
export KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_CANYON_V2=1 KESSEL_AUDIO_RANGE=reference
if [[ "$PANZER_MODE" == play ]]; then export KESSEL_CANYON=1; fi
# Telemetry: one JSON line per second; one fenced GPU pass-timing frame every two seconds.
export KESSEL_SESSION_LOG="$LOG_DIR/session-$STAMP.jsonl" KESSEL_GPU_PROFILE=1
CONSOLE="$LOG_DIR/session-$STAMP.console.log"
ln -sfn "$(basename -- "$KESSEL_SESSION_LOG")" "$LOG_DIR/session-latest.jsonl"
ln -sfn "$(basename -- "$CONSOLE")" "$LOG_DIR/session-latest.console.log"
{
  printf '[launcher] checkout=%s assets=%s mode=%s\n' "$HERE" "$ASSETS" "$PANZER_MODE"
  sha256sum -- "$BINARY"
  git -C "$HERE" log -1 --format='[launcher] commit %h %s' 2>/dev/null || true
} > "$CONSOLE"
printf 'Session log:  %s\nConsole log:  %s\n' "$KESSEL_SESSION_LOG" "$CONSOLE"
cd "$HERE"
exec "$BINARY" >> "$CONSOLE" 2>&1
