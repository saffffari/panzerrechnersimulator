#!/usr/bin/env bash
# The T-34 training's generations as films (docs/T34_EYES.md): one `t34-gen-film` episode per
# champion in $RUN (the July seed policy first, then gen_000, gen_001, ...), the same seed and
# length for each, captioned with the generation and its best fitness, then joined back to back.
# Picture: the game's 1024x576 pixels repeated 5x (5120x2880, the owner's panel; "standard
# resolution"). Nothing starts while the owner's game or editor is live (checked before each film).
#
#   tools/t34_generation_films.sh [SEED] [SECS]      (default 29, the receipt seed; 90 s, the
#                                                      training episode's length)
# Env: RUN (the ES run, default /home/alex/panzer/t34-eyes-bin/es3), BIN (default this checkout's
# release binary), ONLY (a glob over the weight files, e.g. 'gen_01*').
set -euo pipefail
SEED=${1:-29}
SECS=${2:-90}
HERE=$(cd "$(dirname "$0")/.." && pwd)
RUN=${RUN:-/home/alex/panzer/t34-eyes-bin/es3}
BIN=${BIN:-$HERE/target/release/panzerrechnersystem}
ROOT=/windows/shop/panzerrechnersystem
CAPTURES=/home/alex/panzer/captures
STAMP=$(date +%Y%m%d-%H%M)
OWNER_GAME='target/(release|debug)/[p]anzerrechnersystem( fea-range| edit)?$|[l]aunch_demo'
wait_for_owner() { while pgrep -f "$OWNER_GAME" >/dev/null; do sleep 30; done; }

# The locked look (tools/launch_demo_20261101.sh): deltap-real, 2,048 colours, 1024x576.
for v in $(env | grep -o '^KESSEL_[A-Z0-9_]*'); do unset "$v"; done
export KESSEL_ASSET_ROOT=$ROOT
export KESSEL_CANYON_MANIFEST=$ROOT/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json
export KESSEL_CANYON_V2=1 KESSEL_CANYON_T34S=5 KESSEL_CANYON_GUNS=0
export KESSEL_RENDERER=raymarch KESSEL_GRAPHICS_PRESET=high KESSEL_INTERNAL_RES=1024x576
export KESSEL_LOOK=deltap-real KESSEL_REAL_STEPS=15,15,7
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 KESSEL_SOURCE_VISIBILITY=maps
export KESSEL_TRANSPORT_QUALITY=full KESSEL_DAY_CYCLE_SCALE=6 KESSEL_SKY_START=07:00
export KESSEL_CAPTURES_DIR=$CAPTURES
CONFIG=$HERE/artifacts/runtime/demo-state/graphics.deltap-real.v1.json
[ -s "$CONFIG" ] && export KESSEL_GRAPHICS_CONFIG=$CONFIG

# weights|caption, one per line: the seed policy, then every champion with its best fitness.
LIST=$(python3 - "$RUN" "${ONLY:-gen_*.bin}" <<'PY'
import glob, os, re, sys
run, only = sys.argv[1], sys.argv[2]
best = {}
for line in open(os.path.join(run, "run.log")):
    m = re.match(r"\[es\] gen (\d+) .*?: best (-?[\d.]+)", line)
    if m: best[int(m.group(1))] = float(m.group(2))
seed = os.path.join(run, "gen0.bin")
if os.path.exists(seed) and only == "gen_*.bin":
    print(f"{seed}|SEED POLICY  before training")
for path in sorted(glob.glob(os.path.join(run, only))):
    m = re.search(r"gen_(\d+)_", os.path.basename(path))
    if not m: continue
    g = int(m.group(1))
    fit = best.get(g)
    print(f"{path}|GEN {g:02d}" + (f"  best {fit:+.1f}" if fit is not None else ""))
PY
)
[ -n "$LIST" ] || { echo "no weights in $RUN" >&2; exit 1; }
mkdir -p "$CAPTURES"
CONCAT=$(mktemp)
n=0
while IFS='|' read -r weights caption; do
    name=$(printf 't34-gen-%s-s%s-%02d' "$STAMP" "$SEED" "$n")
    wait_for_owner
    echo "[films] $caption  <- $weights"
    ( cd "$HERE" && nice -n 10 "$BIN" t34-gen-film "$name" "$SEED" "$SECS" "$weights" "$caption" ) \
        2>&1 | grep -E '^\[t34-film\]|panicked|Error|error:' || true
    clip="$CAPTURES/$(basename "$HERE")-$name.mp4"
    if [ ! -s "$clip" ]; then
        # The card's encoder would not open (its memory taken): once more on the processor.
        echo "[films] retrying $name with libx264"
        wait_for_owner
        ( cd "$HERE" && KESSEL_T34_FILM_ENCODER=x264 nice -n 10 "$BIN" t34-gen-film "$name" "$SEED" "$SECS" "$weights" "$caption" ) \
            2>&1 | grep -E '^\[t34-film\]|panicked|Error|error:' || true
    fi
    [ -s "$clip" ] && echo "file '$clip'" >> "$CONCAT" || echo "[films] missing $clip" >&2
    n=$((n + 1))
done <<< "$LIST"
OUT="$CAPTURES/$(basename "$HERE")-t34-gens-progression-$STAMP-s$SEED.mp4"
if [ -s "$CONCAT" ]; then
    ffmpeg -v error -nostdin -y -f concat -safe 0 -i "$CONCAT" -c copy -movflags +faststart "$OUT"
    echo "[films] joined $(wc -l < "$CONCAT") clips -> $OUT"
    # The same progression at four times speed, silent (every fourth game frame, the pixels kept).
    FAST="${OUT%.mp4}-4x.mp4"
    ffmpeg -v error -nostdin -y -i "$OUT" -an -vf "setpts=0.25*PTS,fps=30" -c:v hevc_nvenc -preset p5 -rc vbr -cq 29 \
        -pix_fmt yuv444p -tag:v hvc1 -movflags +faststart "$FAST" && echo "[films] 4x time-lapse -> $FAST"
fi
rm -f "$CONCAT"
