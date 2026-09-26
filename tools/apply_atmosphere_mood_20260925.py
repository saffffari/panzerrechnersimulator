#!/usr/bin/env python3
"""Apply the 2026-09-25 atmosphere values to a winter-courtyard manifest_edits.json.

The courtyard's authored lighting lives in an ignored asset file, so a checkout that merges the
atmosphere branch still needs these values. The script is idempotent and writes a timestamped
backup next to the file before changing it.

    python3 tools/apply_atmosphere_mood_20260925.py [path/to/manifest_edits.json] [--dry-run]
"""
import json
import pathlib
import shutil
import sys
import time

DEFAULT = pathlib.Path(__file__).resolve().parent.parent / (
    "assets/worldgen/workshops/act1-winter-courtyard-20260925/manifest_edits.json")

# Stalingrad, mid-December, 15:00 local solar time: the sun stands about 7.5 degrees high.
ENVIRONMENT = {"sun_elevation_deg": 7.5, "twilight": 0.15, "fog_density": 0.0035}
# Tuned against the old 22-degree daylight; at dusk they floodlit the yard.
LIGHTS = {
    "winter_entrance": {"radiant_intensity": 11.0},
    "winter_courtyard_gate": {"radiant_intensity": 22.0, "range_m": 9.0},
}


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    dry = "--dry-run" in sys.argv
    path = pathlib.Path(args[0]) if args else DEFAULT
    raw = path.read_text()
    data = json.loads(raw)
    changes = []
    env = data["environment_lighting"]
    for key, value in ENVIRONMENT.items():
        if env.get(key) != value:
            changes.append(f"environment_lighting.{key}: {env.get(key)} -> {value}")
            env[key] = value
    found = set()
    for light in data.get("authored_lights", []):
        wanted = LIGHTS.get(light.get("id"))
        if not wanted:
            continue
        found.add(light["id"])
        for key, value in wanted.items():
            if light.get(key) != value:
                changes.append(f"{light['id']}.{key}: {light.get(key)} -> {value}")
                light[key] = value
    missing = sorted(set(LIGHTS) - found)
    if missing:
        print(f"warning: lights not found: {', '.join(missing)}", file=sys.stderr)
    if not changes:
        print(f"{path}: already applied")
        return 0
    print("\n".join(changes))
    if dry:
        return 0
    backup = path.with_name(f"{path.name}.before-atmosphere-{time.strftime('%Y%m%d-%H%M%S')}")
    shutil.copy2(path, backup)
    path.write_text(json.dumps(data, indent=2) + ("\n" if raw.endswith("\n") else ""))
    print(f"wrote {path} (backup {backup.name})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
