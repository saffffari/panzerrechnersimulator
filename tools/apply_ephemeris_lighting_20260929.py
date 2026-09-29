#!/usr/bin/env python3
"""Give a workshop's environment_lighting record the date and time the ephemeris lights.

Version 2 of the record (src/environment_authoring.rs, 2026-09-29) carries an instant of Universal
Time; the Sun and the Moon come from the ephemeris for the act's place, and the crewman's eye
adapts to what they give. A version-1 record's sun azimuth/elevation, camera exposure and twilight
can no longer light or expose anything; this tool replaces them with the instant each workshop
authored (docs/PHYSICAL_TRUTH_AUDIT.md, "Sky from an ephemeris"):

- act1-winter-courtyard-20260925: "15:00 local solar time during Wintergewitter" (7.5 deg) is
  1942-12-21T12:00Z (15:00 local apparent time on the solstice: 7.41 deg, compass 220.9 deg).
- act1-urban-intro-20260928: the same 7.5 deg storm dusk in January 1943 is
  1943-01-15T12:27Z (the afternoon Sun at 7.5 deg, compass 225.3 deg).
- act1-winter-encounter-20260925-staged: its 22 deg winter sun is noon on 26 Jan 1943
  (1943-01-26T09:15Z, 22.4 deg).
- ruined-house-courtyard-20260925: its 48 deg sun cannot stand over Stalingrad in winter; noon on
  the solstice (1942-12-21T09:00Z, 17.9 deg) is the nearest winter instant.

The fog, cloud cover and storm are kept. The file is backed up before it changes; running the tool
again is harmless.

    python3 tools/apply_ephemeris_lighting_20260929.py [manifest_edits.json ...] [--utc ISO] [--dry-run]
"""
import json
import pathlib
import shutil
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKSHOPS = ROOT / "assets/worldgen/workshops"
INSTANTS = {
    "act1-winter-courtyard-20260925": "1942-12-21T12:00:00Z",
    "act1-urban-intro-20260928": "1943-01-15T12:27:00Z",
    "act1-winter-encounter-20260925-staged": "1943-01-26T09:15:00Z",
    "ruined-house-courtyard-20260925": "1942-12-21T09:00:00Z",
}
RETIRED = ("sun_azimuth_deg", "sun_elevation_deg", "camera_exposure", "twilight")


def migrate(path: pathlib.Path, utc: str | None, dry: bool) -> int:
    data = json.loads(path.read_text())
    record = data.get("environment_lighting")
    if record is None:
        print(f"{path}: no environment_lighting record")
        return 0
    instant = utc or INSTANTS.get(path.parent.name)
    if instant is None:
        print(f"{path}: no known instant for {path.parent.name}; pass --utc", file=sys.stderr)
        return 1
    changed = []
    for key in RETIRED:
        if key in record:
            changed.append(f"remove {key}={record.pop(key)}")
    if record.get("utc") != instant:
        changed.append(f"utc: {record.get('utc')} -> {instant}")
        record["utc"] = instant
    if record.get("version") != 2:
        changed.append(f"version: {record.get('version')} -> 2")
        record["version"] = 2
    if not changed:
        print(f"{path}: already applied ({instant})")
        return 0
    print(f"{path}:\n  " + "\n  ".join(changed))
    if dry:
        return 0
    backup = path.with_name(f"{path.name}.before-ephemeris-{time.strftime('%Y%m%d-%H%M%S')}")
    shutil.copy2(path, backup)
    path.write_text(json.dumps(data, indent=2) + "\n")
    return 0


def main() -> int:
    args = sys.argv[1:]
    dry = "--dry-run" in args
    utc = None
    if "--utc" in args:
        utc = args[args.index("--utc") + 1]
        if not utc.endswith("Z"):
            print("--utc must be Universal Time ending in Z", file=sys.stderr)
            return 2
    paths = [pathlib.Path(a) for a in args if a.endswith(".json")]
    if not paths:
        paths = [WORKSHOPS / name / "manifest_edits.json" for name in INSTANTS]
        paths = [p for p in paths if p.exists()]
    status = 0
    for path in paths:
        status |= migrate(path, utc, dry)
    return status


if __name__ == "__main__":
    sys.exit(main())
