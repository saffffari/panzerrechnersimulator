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


def merge_sorted(previous: dict, current: dict, refreshed: dict, manifest: dict) -> tuple[dict, dict]:
    """Authoritative completed full scan: absent/unused/unbound assets cannot stay active."""
    out, receipt = merge(previous, current, refreshed)
    assets = {a["id"]: a for a in manifest["assets"]}
    original_live = {c["asset"]: c for c in current.get("cues", [])}
    eligible = {key for key, a in assets.items() if a.get("default_enabled")
                and a.get("playback_available") and a.get("role") not in {"unused", "scene"}}
    fresh = {c["asset"]: c for c in refreshed.get("cues", [])}
    before = {c["asset"]: c for c in out["cues"]}
    out["cues"] = [c for c in out["cues"] if c["asset"] in eligible and c["asset"] in fresh]
    routes = ("role", "contexts", "role_aliases", "role_contexts")
    route_overrides = []
    for cue in out["cues"]:
        # Latest explicit folder/filename routing takes precedence over old broad pool aliases.
        # Gain, rack, fades and other authored treatment remain three-way merged.
        for field in routes:
            if cue.get(field) != fresh[cue["asset"]].get(field):
                route_overrides.append({"asset": cue["asset"], "field": field})
            if field in fresh[cue["asset"]]:
                cue[field] = copy.deepcopy(fresh[cue["asset"]][field])
        if assets[cue["asset"]].get("explicit_filename_loop"):
            for field in ("loop", "crossfade_s", "crossfade_curve"):
                cue[field] = copy.deepcopy(fresh[cue["asset"]][field])
        if assets[cue["asset"]].get("runtime_assignment", {}).get("finite_foreground_phrase"):
            cue["loop"] = False
            cue["crossfade_s"] = 0.0
            cue["crossfade_curve"] = None
            for field in ("gain_db", "rack"):
                if field in original_live.get(cue["asset"], {}):
                    cue[field] = copy.deepcopy(original_live[cue["asset"]][field])
    live_ids = {c["asset"] for c in out["cues"] if c.get("enabled", True)}
    deprecated = {"incoming_riser", "incoming_hit_riser", "incoming_tank_riser", "surprise_contact"}
    events = copy.deepcopy(current.get("events", {}))
    events.update(copy.deepcopy(refreshed.get("events", {})))
    events = {name: [key for key in ids if key in live_ids] for name, ids in events.items()
              if name not in deprecated and isinstance(ids, list)}
    out["events"] = {name: ids for name, ids in events.items() if ids}
    out["effect_registry"] = [key for key in refreshed.get("effect_registry", []) if key in live_ids]
    out["named_effect_registry"] = {name: [key for key in ids if key in assets]
        for name, ids in refreshed.get("named_effect_registry", {}).items()}
    out["deferred_effects"] = copy.deepcopy(refreshed.get("deferred_effects", []))
    out["additional_folder_bindings"] = [a for a in refreshed.get("additional_folder_bindings", [])
        if a.get("asset") in live_ids]
    out["manifest_sha256"] = refreshed["manifest_sha256"]
    receipt.update(retired_cue_ids=sorted(set(before)-{c["asset"] for c in out["cues"]}),
        source_routing_overrides=route_overrides,
        retired_events={name: ids for name, ids in current.get("events", {}).items()
                        if name not in out["events"]},
        retired_event_members={name: sorted(set(ids)-set(out["events"].get(name, [])))
            for name, ids in current.get("events", {}).items()
            if set(ids)-set(out["events"].get(name, []))},
        retained_unresolved_cue_ids=[], merged_cue_count=len(out["cues"]),
        enabled_cue_count=len(live_ids), authoritative_full_scan=True)
    return out, receipt


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
