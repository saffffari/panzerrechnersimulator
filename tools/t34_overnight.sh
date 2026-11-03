#!/usr/bin/env bash
# Overnight T-34 training and receipts (docs/T34_EYES.md). Resumes tools/t34_canyon_es.py from the
# two best genomes of the last finished generation in $RUN, runs $GENS more generations (two GPU
# episodes at once, none while the owner's game or editor is live: checked before each), then
# writes before/after canyon-ai-run receipts for the held-out seeds 29, 7 and 211 at 60 s and 180 s,
# one process at a time, and the arena gauntlet for the new champion.
#
#   tools/t34_overnight.sh [GENS]        (default 12)
#
# Binaries: $BIN (this branch, release) and $BASE (the main line before this branch, 79bcbfa).
set -euo pipefail
GENS=${1:-12}
# Not even the trainer's first build of state starts while he is using the machine.
# (The bracketed first letters keep the pattern from matching a command line that merely spells it.)
while pgrep -f 'release/[p]anzerrechnersystem( fea-range| edit)?$|[l]aunch_demo' >/dev/null; do sleep 30; done
HERE=$(cd "$(dirname "$0")/.." && pwd)
BINS=/home/alex/panzer/t34-eyes-bin
BIN=${BIN:-$BINS/eyes-train}
BASE=${BASE:-$BINS/baseline-79bcbfa}
RUN=${RUN:-$BINS/es3}
ROOT=/windows/shop/panzerrechnersystem
JULY=$ROOT/assets/ai/weights_academy.bin
OUT=$HERE/artifacts/t34-eyes-20261102
# The owner's standing rule (2026-11-03): nothing starts while his game or editor is live.
OWNER_GAME='release/[p]anzerrechnersystem( fea-range| edit)?$|[l]aunch_demo'
export KESSEL_ASSET_ROOT=$ROOT
export KESSEL_CANYON_MANIFEST=$ROOT/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json
export KESSEL_CANYON_V2=1 KESSEL_CANYON_T34S=5 KESSEL_CANYON_GUNS=0
export KESSEL_CAPTURES_DIR=/home/alex/panzer/captures

read -r LAST P1 P2 < <(python3 - "$RUN/es.jsonl" <<'EOF'
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
last = max(r["gen"] for r in rows)
top = sorted((r for r in rows if r["gen"] == last), key=lambda r: r["rank"])[:2]
print(last, top[0]["genome"], top[1]["genome"])
EOF
)
echo "[overnight] resuming after generation $LAST from $P1, $P2"
nice -n 10 python3 "$HERE/tools/t34_canyon_es.py" --bin "$BIN" --init "$RUN/gen0.bin" \
  --parents "$RUN/genomes/$P1.bin,$RUN/genomes/$P2.bin" --start-gen $((LAST + 1)) --gens "$GENS" \
  --july "$JULY" --out "$RUN" --pop 8 --mu 2 --workers 2 --secs 90 | tee -a "$RUN/run.log"

CHAMP=$(ls -t "$RUN"/gen_*.bin | head -1)
mkdir -p "$OUT/after" "$OUT/before"
cp "$CHAMP" "$OUT/weights_t34_eyes_candidate.bin"
echo "[overnight] champion $CHAMP"

wait_for_owner() { while pgrep -f "$OWNER_GAME" >/dev/null; do sleep 30; done; }
receipt() { # dir bin seed secs [weights]
  local dir=$1 bin=$2 seed=$3 secs=$4 weights=${5:-}
  local name=s${seed}_${secs}s
  [ -s "$OUT/$dir/${name}_result.json" ] && return 0
  wait_for_owner
  ( cd "$OUT/$dir" && KESSEL_T34_WEIGHTS=$weights nice -n 10 "$bin" canyon-ai-run "canyon_1v5_${dir}_$name" "$seed" "$secs" \
      > "$name.log" 2>&1 ) || true
  grep '^\[canyon-result\]' "$OUT/$dir/$name.log" | sed 's/^\[canyon-result\] //' > "$OUT/$dir/${name}_result.json" || true
}
for seed in 29 7 211; do
  for secs in 60 180; do
    receipt before "$BASE" $seed $secs
    receipt after "$BIN" $seed $secs "$CHAMP"
  done
done
rm -f /home/alex/panzer/captures/*canyon_1v5_*_trace.jsonl
wait_for_owner
( cd "$OUT" && nice -n 10 "$BIN" ai-gauntlet "$OUT/gauntlet_after" "$CHAMP" "$ROOT/assets/ai/weights_tiger.bin" 3 90 0 \
    > "$OUT/gauntlet_after.log" 2>&1 ) || echo "[overnight] gauntlet failed: $OUT/gauntlet_after.log"
wait_for_owner
( cd "$OUT" && nice -n 10 "$BIN" ai-gauntlet "$OUT/gauntlet_july" "$JULY" "$ROOT/assets/ai/weights_tiger.bin" 3 90 0 \
    > "$OUT/gauntlet_july.log" 2>&1 ) || echo "[overnight] gauntlet failed: $OUT/gauntlet_july.log"
echo "[overnight] done: receipts in $OUT/{before,after}, candidate $OUT/weights_t34_eyes_candidate.bin"
