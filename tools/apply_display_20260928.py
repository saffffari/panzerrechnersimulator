#!/usr/bin/env python3
"""Apply the 2026-09-28 display treatment to a winter-courtyard manifest_edits.json.

The courtyard's display profile lives in an ignored asset file, so a checkout that merges this
branch still needs these values (the launcher's KESSEL_INTERNAL_RES must agree; it is the outer
override). The script is idempotent and writes a timestamped backup next to the file first.

    python3 tools/apply_display_20260928.py [path/to/manifest_edits.json] [--dry-run]

1280x720 scales by exactly 4 onto a 5120x2880 display (960x540 was 5.33x, so nearest-neighbour
pixels alternated between 5 and 6 screen pixels). Finer pixels hide the ordered dither, so its
amplitude rises from 0.35 to 0.55 of a palette step and the crushed image is no longer blended
15% back toward the smooth one (retro 0.85 -> 1.0). Palette depth and LDR dither are unchanged.
"""
import json
import pathlib
import shutil
import sys
import time

DEFAULT = pathlib.Path(__file__).resolve().parent.parent / (
    "assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest_edits.json")

DISPLAY = {"width": 1280, "height": 720, "ordered_dither": 0.55, "retro_strength": 1.0}


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    dry = "--dry-run" in sys.argv
    path = pathlib.Path(args[0]) if args else DEFAULT
    raw = path.read_text()
    data = json.loads(raw)
    display = data.get("display_tuning")
    if not isinstance(display, dict):
        print(f"{path}: no display_tuning profile to update", file=sys.stderr)
        return 1
    changes = []
    for key, value in DISPLAY.items():
        if display.get(key) != value:
            changes.append(f"display_tuning.{key}: {display.get(key)} -> {value}")
            display[key] = value
    if not changes:
        print(f"{path}: already applied")
        return 0
    print("\n".join(changes))
    if dry:
        return 0
    backup = path.with_name(f"{path.name}.before-display-{time.strftime('%Y%m%d-%H%M%S')}")
    shutil.copy2(path, backup)
    path.write_text(json.dumps(data, indent=2) + ("\n" if raw.endswith("\n") else ""))
    print(f"wrote {path} (backup {backup.name})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
