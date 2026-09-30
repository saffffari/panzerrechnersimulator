#!/usr/bin/env bash
# Current winter study, with Singleplayer and Co-op buttons in its title menu.
# --urban plays the urban-intro district around the spawn instead of the winter courtyard
# (docs/URBAN_INTRO_20260928.md). Its workshop is generated, not tracked: when it is missing, the
# deterministic district tool (tools/worldgen/urban_intro_district.py) writes it before the launch
# (about 15 s, byte-identical on every run; with --check it is only reported). A workshop written by an
# older version of the tool is regenerated the same way, unless it was edited by hand.
# --canyon plays the CANYON playtest course (docs/CANYON_COURSE_20260929.md) directly (with --menu,
# the title menu, whose Canyon entry starts it). Every session writes the course's workshop with
# tools/worldgen/canyon_course.py (about 6 s) when it is missing or was written by an older tool, so
# the title menu's Canyon entry loads from any session.
# Every other option passes through to launch_shader_playtest.sh.
set -euo pipefail
SCRIPT_DIR=$(CDPATH= cd -P -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
URBAN_MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-urban-intro-20260928/manifest.v1.json"
CANYON_MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/canyon-course/manifest.v1.json"
MANIFEST="$PROJECT_ROOT/assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest.v1.json"
CHECK=0
CANYON=0
MENU=0
PASS=()
for arg in "$@"; do
    case "$arg" in
        --urban) MANIFEST="$URBAN_MANIFEST" ;;
        --canyon) CANYON=1 ;;
        --check) CHECK=1; PASS+=("$arg") ;;
        --menu) MENU=1; PASS+=("$arg") ;;
        *) PASS+=("$arg") ;;
    esac
done
if [[ "$MANIFEST" == "$URBAN_MANIFEST" && ! -f "$MANIFEST" ]] && (( ! CHECK )); then
    echo "urban workshop missing; generating it with tools/worldgen/urban_intro_district.py" >&2
    python3 "$PROJECT_ROOT/tools/worldgen/urban_intro_district.py" >&2
elif [[ "$MANIFEST" == "$URBAN_MANIFEST" ]] && (( ! CHECK )); then
    # A workshop written by an older tool lacks what the game now reads from it (tool version 2: the
    # street encounter). The tool rewrites only its own unmodified output; a hand-edited workshop is
    # kept and reported, and the game then lays the courtyard's open-ground encounter.
    RECEIPT="$(dirname "$URBAN_MANIFEST")/urban-intro.receipt.json"
    WANT=$(sed -n 's/^TOOL_VERSION = \([0-9]*\).*/\1/p' "$PROJECT_ROOT/tools/worldgen/urban_intro_district.py")
    HAVE=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("tool_version", 0))' "$RECEIPT" 2>/dev/null || echo 0)
    if [[ -n "$WANT" && "$HAVE" != "$WANT" ]]; then
        echo "urban workshop is tool version $HAVE, the tool is $WANT; regenerating it" >&2
        python3 "$PROJECT_ROOT/tools/worldgen/urban_intro_district.py" >&2 \
            || echo "urban workshop kept as it is (hand-edited?); regenerate with --force to take the new tool" >&2
    fi
fi
if (( ! CHECK )); then
    # Every session keeps the title menu's Canyon entry playable: the course is written when missing
    # or from an older tool (about 6 s, once). Only a --canyon session fails without it.
    CANYON_RECEIPT="$(dirname "$CANYON_MANIFEST")/canyon-course.receipt.json"
    CANYON_WANT=$(sed -n 's/^TOOL_VERSION = \([0-9]*\).*/\1/p' "$PROJECT_ROOT/tools/worldgen/canyon_course.py")
    CANYON_HAVE=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("tool_version", 0))' "$CANYON_RECEIPT" 2>/dev/null || echo 0)
    if [[ ! -f "$CANYON_MANIFEST" || "$CANYON_HAVE" != "$CANYON_WANT" ]]; then
        echo "canyon workshop missing or from tool version $CANYON_HAVE; generating it with tools/worldgen/canyon_course.py" >&2
        if ! python3 "$PROJECT_ROOT/tools/worldgen/canyon_course.py" >&2; then
            if (( CANYON )); then exit 1; fi
            echo "canyon workshop not written; the title menu's Canyon entry will not load" >&2
        fi
    fi
fi
if (( CANYON )); then
    if [[ ! -f "$CANYON_MANIFEST" ]]; then
        echo "missing $CANYON_MANIFEST" >&2
        echo 'generate the canyon workshop with: python3 tools/worldgen/canyon_course.py' >&2
        exit 2
    fi
    # The course is its own map (its display and weather are its own); the session's other entries
    # (Singleplayer, Co-op) keep the courtyard or, with --urban, the district.
    export KESSEL_CANYON_MANIFEST="$CANYON_MANIFEST"
    if (( ! MENU )); then
        export KESSEL_CANYON=1
    fi
elif [[ -f "$CANYON_MANIFEST" ]]; then
    # The title menu's Canyon entry finds the course wherever the session started.
    export KESSEL_CANYON_MANIFEST="$CANYON_MANIFEST"
fi
if [[ ! -f "$MANIFEST" ]]; then
    echo "missing $MANIFEST" >&2
    if [[ "$MANIFEST" == "$URBAN_MANIFEST" ]]; then
        echo 'generate the urban workshop with: python3 tools/worldgen/urban_intro_district.py' >&2
    fi
    exit 2
fi
export KESSEL_PLAYTEST_MANIFEST="$MANIFEST"
export KESSEL_INTERNAL_RES=1280x720
export KESSEL_WINTER_COMBAT=1
exec "$SCRIPT_DIR/launch_shader_playtest.sh" --menu ${PASS[@]+"${PASS[@]}"}
