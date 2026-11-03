#!/usr/bin/env bash
# Overnight / whenever T-34 training, safe to leave running while you play.
#
#   tools/train_overnight.sh [GENS]      (default 12 generations, then receipts; docs/T34_EYES.md)
#
# Runs tools/t34_overnight.sh (resumes from the last finished generation in
# /home/alex/panzer/t34-eyes-bin/es3) in its own process group. A watchdog checks every 2 s for the
# owner's game or editor (a windowed panzerrechnersystem with no harness arguments, or the demo
# launcher): the moment one opens, every training process is frozen (SIGSTOP: no CPU, nothing lost),
# and it is thawed (SIGCONT) when the window closes. Ctrl+C stops everything.
# Log: artifacts/t34-eyes-20261102/overnight-<date>.log
set -uo pipefail
GENS=${1:-12}
HERE=$(cd "$(dirname "$0")/.." && pwd)
LOG_DIR=$HERE/artifacts/t34-eyes-20261102
mkdir -p "$LOG_DIR"
LOG=$LOG_DIR/overnight-$(date +%Y%m%d-%H%M).log
# Anchored on the binary path so a cargo build of the crate does not count; bracketed so this
# script's own command line never matches.
LIVE='target/(release|debug)/[p]anzerrechnersystem( fea-range| edit)?$|[l]aunch_demo'
live() { pgrep -f "$LIVE" >/dev/null; }

setsid nice -n 15 bash "$HERE/tools/t34_overnight.sh" "$GENS" >>"$LOG" 2>&1 &
PGID=$!
echo "[train] started (process group $PGID), $GENS generations; log: $LOG"
trap 'echo "[train] stopping"; kill -CONT -"$PGID" 2>/dev/null; kill -TERM -"$PGID" 2>/dev/null; exit 130' INT TERM

frozen=0
while kill -0 "$PGID" 2>/dev/null; do
  if live; then
    if [ $frozen = 0 ]; then kill -STOP -"$PGID" 2>/dev/null; frozen=1; echo "[train] $(date +%H:%M) game/editor open: frozen"; fi
  elif [ $frozen = 1 ]; then
    kill -CONT -"$PGID" 2>/dev/null; frozen=0; echo "[train] $(date +%H:%M) game/editor closed: resumed"
  fi
  sleep 2
done
wait "$PGID"; status=$?
echo "[train] finished (exit $status). Last lines:"; tail -5 "$LOG"
exit $status
