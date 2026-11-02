#!/usr/bin/env bash
# Linux launcher for the 2026-11-01 demo: the Windows-side work (expedition, camp, workshop, fuel
# service, halftrack, crew panel; branch claude/windows-wip-20261101) on the raymarch renderer.
# A port of tools/launch_windows.ps1's profile: assets from the Windows checkout on /windows,
# saves/caches/config kept under artifacts/runtime so nothing touches ~/.config or ~/.local.
#
#   tools/launch_demo_20261101.sh                 title menu (New Expedition / Resume Expedition / Canyon Run v2 ...)
#   tools/launch_demo_20261101.sh --expedition    straight into a new expedition: the camp, the Tiger, the halftrack
#   tools/launch_demo_20261101.sh --expedition --order 10            the order for site 10 accepted at once (M: the map)
#   tools/launch_demo_20261101.sh --expedition --order 10 --forward  and the crewed Tiger staged on the approach road,
#                                                 700 m short of the site, engine running (a disclosed playtest fixture)
#   tools/launch_demo_20261101.sh --canyon        straight into a fresh Canyon v2 winter run (missions + living crew)
#   tools/launch_demo_20261101.sh --windowed      1280x720 window instead of borderless fullscreen
#   tools/launch_demo_20261101.sh --res 640x360   render grid (default 480x270; 640x360 is an exact 8x on the 5K panel)
#   tools/launch_demo_20261101.sh --look standard the old display look at 480x270; the default is the deltaP look
#                                                 (docs/DELTAP_LOOK_20261101.md: 1024x576, dithered colour, pyramid bloom;
#                                                 the owner's pick, 2026-11-01); --look deltap-pico8 for the 32-colour frame
#   tools/launch_demo_20261101.sh --platoon       two AI Tigers under command on the Canyon run (KESSEL_PLATOON=2)
#   tools/launch_demo_20261101.sh --check         validate binary and assets, start nothing
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
ASSET_ROOT=${PANZER_ASSET_ROOT:-/windows/shop/panzerrechnersystem}
BINARY="$ROOT/target/release/panzerrechnersystem"
RUNTIME="$ROOT/artifacts/runtime"
MODE=menu RES=480x270 WINDOWED=0 PLATOON=0 LOOK=deltap SEED= CHECK=0 RES_SET=0 ORDER= FORWARD=0
while (( $# )); do
    case "$1" in
        --canyon) MODE=canyon ;;
        --expedition) MODE=expedition ;;
        --menu) MODE=menu ;;
        --windowed) WINDOWED=1 ;;
        --platoon) PLATOON=1 ;;
        --res) RES=$2; RES_SET=1; shift ;;
        --look) LOOK=$2; [[ "$LOOK" == standard ]] && LOOK=; shift ;;
        --seed) SEED=$2; shift ;;
        --order) ORDER=$2; MODE=expedition; shift ;;
        --forward) FORWARD=1; MODE=expedition ;;
        --check) CHECK=1 ;;
        -h|--help) sed -n 2,19p "$0"; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
    shift
done
if [[ -n "$LOOK" && $RES_SET == 0 ]]; then RES=1024x576; fi   # deltaP's grid: 5x on the 5120x2880 panel
cd "$ROOT"
[[ -x "$BINARY" ]] || { printf 'release binary missing: %s (cargo build --release --locked)\n' "$BINARY" >&2; exit 1; }
for f in assets/tanks/tiger/baked/tiger_hull.glb assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json \
         assets/worldgen/expedition/the_march_v1/manifest.v1.json catalog/maps/canyon_v2_survey_20260930.png; do
    [[ -s "$ASSET_ROOT/$f" ]] || { printf 'missing asset: %s/%s\n' "$ASSET_ROOT" "$f" >&2; exit 1; }
done
BUILD_ID=$(set +o pipefail; strings "$BINARY" | grep -oE 'claude/windows-wip-20261101 [0-9a-f]+ \([^)]*\)' | head -1)
printf 'binary: %s\nbuild:  %s\nassets: %s\n' "$BINARY" "${BUILD_ID:-unknown}" "$ASSET_ROOT"
if (( CHECK )); then exit 0; fi

mkdir -p "$RUNTIME"/{state,cache,config,demo-state} captures
CONFIG="$RUNTIME/demo-state/graphics${LOOK:+.$LOOK}.v1.json"
python3 - "$CONFIG" "$RES" "$WINDOWED" <<'PY'
import json, sys
from pathlib import Path
path, res, windowed = Path(sys.argv[1]), sys.argv[2], sys.argv[3] == '1'
w, h = (int(v) for v in res.split('x'))
if path.exists():
    doc = json.loads(path.read_text())
    doc['resolution'] = [w, h]; doc['display']['width'] = w; doc['display']['height'] = h
    doc['fullscreen'] = not windowed
else:
    doc = {'version': 1, 'resolution': [w, h], 'window': [1280, 720], 'fullscreen': not windowed,
           'vsync': True, 'shadows': 2048, 'full_transport': True, 'optics': 1, 'fps': False,
           'exposure_ev': 0.0, 'saturation': 1.0, 'warmth': 0.0, 'gamma': 1.0,
           'storm': None, 'sun_phase': 0, 'sun_speed': 1.0, 'dither_pattern': 3,
           'display': {'width': w, 'height': h, 'palette_levels': 31.0, 'ordered_dither': 1.0,
                       'retro_strength': 1.0, 'ldr_dither': 1.0,
                       'ambient_white_balance': False, 'ambient_white_balance_strength': 1.0}}
path.write_text(json.dumps(doc, indent=2) + '\n')
PY

# Drop inherited experiment flags so a stale shell cannot change the profile.
for v in $(env | grep -o '^KESSEL_[A-Z0-9_]*'); do unset "$v"; done
export XDG_STATE_HOME="$RUNTIME/state" XDG_CACHE_HOME="$RUNTIME/cache" XDG_CONFIG_HOME="$RUNTIME/config"
export KESSEL_ASSET_ROOT="$ASSET_ROOT"
export KESSEL_CANYON_MANIFEST="$ASSET_ROOT/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json"
export KESSEL_RENDERER=raymarch KESSEL_GRAPHICS_PRESET=high KESSEL_INTERNAL_RES="$RES"
export KESSEL_DITHER_PATTERN=screen_bayer8 KESSEL_GEOMETRY_AA=msaa4 KESSEL_SOURCE_VISIBILITY=maps
export KESSEL_GRAPHICS_CONFIG="$CONFIG" KESSEL_ENABLE_PIZZA_AUDIO=1 KESSEL_TRANSPORT_QUALITY=full
export KESSEL_CANYON_V2=1
(( WINDOWED )) || export KESSEL_FULLSCREEN=1
[[ "$MODE" == canyon ]] && export KESSEL_CANYON=1
[[ "$MODE" == expedition ]] && export KESSEL_EXPEDITION=1
[[ -n "$ORDER" ]] && export KESSEL_EXPEDITION_ORDER="$ORDER"
(( FORWARD )) && export KESSEL_EXPEDITION_FORWARD=1
(( PLATOON )) && export KESSEL_PLATOON=2
[[ -n "$SEED" ]] && export KESSEL_CANYON_SEED="$SEED"
[[ -n "$LOOK" ]] && export KESSEL_LOOK="$LOOK"

LOG="$RUNTIME/demo-$(date +%Y%m%d-%H%M%S).log"
{
    printf '[launcher] %s mode=%s res=%s look=%s windowed=%s platoon=%s order=%s forward=%s\n' "$(date --iso-8601=seconds)" "$MODE" "$RES" "${LOOK:-standard}" "$WINDOWED" "$PLATOON" "${ORDER:--}" "$FORWARD"
    env | grep '^KESSEL_' | sort
} > "$LOG"
ln -sfn "$(basename -- "$LOG")" "$RUNTIME/demo-latest.log"
printf 'log: %s\n' "$LOG"
exec "$BINARY" >> "$LOG" 2>&1
