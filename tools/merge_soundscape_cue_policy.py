#!/usr/bin/env python3
"""Merge refreshed suggestions without overwriting authored runtime cue overrides."""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path


def merge(previous: dict, current: dict, refreshed: dict) -> tuple[dict, dict]:
    out = copy.deepcopy(current)
    old = {c["asset"]: c for c in previous.get("cues", [])}
    live = {c["asset"]: c for c in current.get("cues", [])}
    fresh = {c["asset"]: c for c in refreshed.get("cues", [])}
    preserved = []
    merged = []
    missing = object()
    for key, new in fresh.items():
        result = copy.deepcopy(new)
        for field, value in live.get(key, {}).items():
            if value != old.get(key, {}).get(field, missing):
                result[field] = copy.deepcopy(value)
                preserved.append({"asset": key, "field": field})
        merged.append(result)
    # Preserve explicit records for temporarily missing/moving IDs; runtime cannot resolve
    # them without a manifest entry, and this merge never deletes the user's authored data.
    retained_missing = sorted(live.keys() - fresh.keys())
    merged.extend(copy.deepcopy(live[k]) for k in retained_missing)
    out["cues"] = merged
    for field, value in refreshed.items():
        if field == "cues":
            continue
        if field == "manifest_sha256" or field not in current or current[field] == previous.get(field, missing):
            out[field] = copy.deepcopy(value)
    out.setdefault("locations", [])
    return out, {"preserved_authored_fields": preserved, "retained_unresolved_cue_ids": retained_missing,
                 "added_cue_ids": sorted(fresh.keys() - live.keys()), "merged_cue_count": len(merged)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous", type=Path, required=True)
    parser.add_argument("--current", type=Path, default=Path("assets/audio/library/soundscape.json"))
    parser.add_argument("--refreshed", type=Path, default=Path("catalog/soundscape_runtime_bindings_20261002.json"))
    parser.add_argument("--receipt", type=Path, default=Path("artifacts/runtime/audio_library_tail_config_merge_20261002.json"))
    args = parser.parse_args()
    out, receipt = merge(*(json.loads(p.read_text()) for p in [args.previous, args.current, args.refreshed]))
    temp = args.current.with_suffix(args.current.suffix + ".tmp")
    temp.write_text(json.dumps(out, indent=2) + "\n")
    temp.replace(args.current)
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
