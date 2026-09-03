#!/bin/bash
# Evolution timelapse: film every champion checkpoint (same deterministic arena staging,
# 28 s segment each), caption with generation + fitness, concat chronologically.
set -e
cd /home/alex/tank_simulator
CAPTURES=$PWD/captures
SCRATCH=/tmp/claude-1000/-home-alex-tank-simulator/17054df4-612d-449b-90ca-413ca829d568/scratchpad
WORK=$SCRATCH/timelapse
FONT=/usr/share/fonts/TTF/DejaVuSans-Bold.ttf
mkdir -p "$WORK" "$CAPTURES"
rm -f "$WORK"/seg_*.mp4 "$WORK"/concat.txt

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
cp "$WORK/evolution_timelapse.mp4" "$CAPTURES/evolution_timelapse.mp4"
echo "── done: $CAPTURES/evolution_timelapse.mp4 ($i segments)"
