#!/usr/bin/env bash
# usage: shoot.sh NAME TAKE [K=V ...]   film one trailer beat into captures/trailer_NAME_TAKE_20261004*
NAME=$1; TAKE=$2; shift 2
ROOT=/home/alex/panzer/adaptive-audio-20261002
cd "$ROOT/.claude/worktrees/bug-squash-20261003"
export KESSEL_ASSET_ROOT=$ROOT KESSEL_RENDERER=shader KESSEL_NO_RT=1 KESSEL_GRAPHICS_PRESET=high KESSEL_FIXED_PLAYTEST_GRAPHICS=1
export KESSEL_INTERNAL_RES=800x450 KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 KESSEL_SOURCE_VISIBILITY=maps
export KESSEL_TRANSPORT_QUALITY=full KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_AUDIO_RANGE=reference KESSEL_DAY_NIGHT=0
for kv in "$@"; do export "$kv"; done
nice "$ROOT/.claude/worktrees/bug-squash-target/release/panzerrechnersystem" mechanic-review "captures/trailer_${NAME}_${TAKE}_20261004" "tools/trailer/scenarios/$NAME.json" 2>&1 | grep -E "^\[mechanic\]|rror|panick|wrote|rejected|refus" | cut -c1-300 | tail -12
ls -la "captures/" | grep "trailer_${NAME}_${TAKE}_" | awk '{print $5, $9}'
