#!/usr/bin/env python3
"""One targeted category import; other source directories and existing runtime config are untouched."""
from __future__ import annotations

import argparse
from collections import Counter
import copy
import json
from pathlib import Path

from catalog_panzer_sounds import apply_filename_loop_instruction, catalog, import_enabled


def merge_targeted(existing: dict, addition: dict, categories: list[str]) -> dict:
    out = copy.deepcopy(existing)
    by_id = {a["id"]: a for a in out["assets"]}
    for entry in addition["assets"]:
        old = by_id.get(entry["id"])
        if old:
            current_paths = {entry["source_path"], *entry["aliases"]}
            history = old.get("historical_source_paths", []) + [old["source_path"], *old["aliases"]]
            entry["historical_source_paths"] = list(dict.fromkeys(p for p in history if p not in current_paths))
            entry["prior_filename_metadata"] = old["filename_metadata"]
            # The actual complete file remains one content identity; physical samples do not
            # duplicate because a source was renamed/moved into a newly assigned category.
            apply_filename_loop_instruction(entry)
        by_id[entry["id"]] = entry
    out["assets"] = sorted(by_id.values(), key=lambda a: a["source_path"])
    out["inventory"].setdefault("last_full_source_scan", {
        key: existing["inventory"][key] for key in ("source_files", "source_bytes", "unique_assets", "duplicate_files")})
    out["inventory"]["scope"] = "last full snapshot plus targeted category refresh; other source folders were not rescanned"
    out["inventory"]["unique_assets"] = len(out["assets"])
    out["inventory"]["unique_bytes"] = sum(a["bytes"] for a in out["assets"])
    out["inventory"]["roles"] = dict(sorted(Counter(a["role"] for a in out["assets"]).items()))
    out["inventory"]["enabled_derived_files"] = sum(a["default_enabled"] and a.get("playback_available", False) for a in out["assets"])
    out["inventory"]["available_derived_files"] = sum(a.get("playback_available", False) for a in out["assets"])
    out["inventory"]["targeted_refresh"] = {"categories": categories, "source_files": addition["inventory"]["source_files"],
        "source_paths": [a["source_path"] for a in addition["assets"]],
        "pending_empty_categories": [c for c in categories if not any(a["source_path"].startswith(c + "/") for a in addition["assets"])],
        "warnings": addition["inventory"]["warnings"], "failures": addition["inventory"]["failures"]}
    out["policies"]["explicit_folder_roles"].update(addition["policies"]["explicit_folder_roles"])
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("category", nargs="+")
    parser.add_argument("--manifest", type=Path, default=Path("assets/audio/library/manifest.json"))
    args = parser.parse_args()
    existing = json.loads(args.manifest.read_text())
    addition = catalog(Path(existing["source_root"]), -30, -12, folders=tuple(args.category))
    if addition["inventory"]["failures"] or addition["inventory"]["warnings"]:
        raise ValueError("target source changed or could not be decoded; existing catalog left unchanged")
    import_enabled(addition, args.manifest.parent)
    out = merge_targeted(existing, addition, args.category)
    temp = args.manifest.with_suffix(".json.tmp")
    temp.write_text(json.dumps(out, indent=2, allow_nan=False) + "\n")
    temp.replace(args.manifest)
    print(json.dumps(out["inventory"]["targeted_refresh"], indent=2))


if __name__ == "__main__":
    main()
