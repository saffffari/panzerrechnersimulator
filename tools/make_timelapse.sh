#!/bin/bash
# Evolution timelapse: film every champion checkpoint (same deterministic arena staging,
# 28 s segment each), caption with generation + fitness, concat chronologically.
set -e
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
cd "$PROJECT_ROOT"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/kessel-timelapse.XXXXXX")
OUTPUT=${1:-"$PROJECT_ROOT/evolution_timelapse.mp4"}
FONT=${KESSEL_TIMELAPSE_FONT:-/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf}
trap 'rm -rf "$WORK"' EXIT

i=0
for ck in $(ls assets/ai/checkpoints/gen_*.bin | sort); do
    base=$(basename "$ck" .bin)               # gen_0148_f921
    gen=$(echo "$base" | cut -d_ -f2 | sed 's/^0*//'); gen=${gen:-0}
    fit=$(echo "$base" | cut -d_ -f3 | tr -d f)
    seg=$(printf "%s/seg_%03d.mp4" "$WORK" "$i")
    echo "── filming $base (gen $gen, fitness $fit)"
    cargo run --release -- duel "$WORK/film" "$ck" 0 28 2>/dev/null | grep -E 'over:' || true
    ffmpeg -y -loglevel error -i "$WORK/film/duel.mp4" \
      -vf "drawtext=fontfile=$FONT:text='GENERATION $gen — fitness $fit':x=24:y=24:fontsize=42:fontcolor=white:borderw=3:bordercolor=black" \
      -c:v libx264 -pix_fmt yuv420p -crf 22 "$seg"
    echo "file '$seg'" >> "$WORK/concat.txt"
    i=$((i+1))
done

ffmpeg -y -loglevel error -f concat -safe 0 -i "$WORK/concat.txt" -c copy "$WORK/evolution_timelapse.mp4"
cp "$WORK/evolution_timelapse.mp4" "$OUTPUT"
echo "── done: $OUTPUT ($i segments)"
