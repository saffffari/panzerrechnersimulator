#!/usr/bin/env python3
"""Stage an authoritative saved audio scan, with reversible JSON promotion.

Source recordings are never modified. Existing verified playback derivatives are
reused; newly enabled content gets a content-addressed, untrimmed derivative.
"""
from __future__ import annotations

import argparse
from collections import Counter
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from catalog_panzer_sounds import apply_filename_usage, import_enabled, preserve_musical_history, sha256
from merge_soundscape_cue_policy import merge_sorted
from suggest_soundscape_cue_policy import TRANSITION_RISER_ID


def write_json(path: Path, data: dict) -> None:
    path.write_text(json.dumps(data, indent=2, allow_nan=False) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scan", type=Path, required=True)
    parser.add_argument("--stage", type=Path, required=True, help="New directory; existing directories are refused")
    parser.add_argument("--promote", action="store_true", help="After validation, replace the four live JSON documents")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    library = root / "assets/audio/library"
    paths = {
        "manifest.json": library / "manifest.json",
        "policy.json": root / "catalog/soundscape_cue_policy_20261002.json",
        "proposal.json": root / "catalog/soundscape_runtime_bindings_20261002.json",
        "soundscape.json": library / "soundscape.json",
    }
    stage = args.stage.resolve()
    stage.mkdir(parents=True, exist_ok=False)
    backup = stage / "backup"
    backup.mkdir()
    old, hashes = {}, {}
    for name, path in paths.items():
        shutil.copy2(path, backup / name)
        old[name] = json.loads((backup / name).read_text())
        hashes[name] = sha256(backup / name)
    write_json(stage / "backup_receipt.json", {name: {"live_path": str(paths[name]), "sha256": digest}
                                             for name, digest in hashes.items()})

    manifest = copy.deepcopy(json.loads(args.scan.read_text()))
    inventory = manifest["inventory"]
    if inventory["warnings"] or inventory["failures"]:
        raise ValueError("Saved source scan has failures or warnings; no promotion")
    originally_enabled = sum(a["default_enabled"] for a in manifest["assets"])
    preserve_musical_history(manifest, old["manifest.json"])
    for asset in manifest["assets"]:
        apply_filename_usage(asset)
    expected_enabled = sum(a["default_enabled"] for a in manifest["assets"])
    inventory["roles"] = dict(sorted(Counter(a["role"] for a in manifest["assets"]).items()))
    inventory["scope"] = "authoritative completed full sorted source scan; current folder and filename intent"
    inventory["saved_scan_sha256"] = sha256(args.scan)
    existing_playback = {p.name: sha256(p) for p in (library / "clips").glob("*.wav")}
    import_enabled(manifest, library)
    if inventory["warnings"] or inventory["failures"] or inventory["enabled_derived_files"] != expected_enabled:
        raise ValueError("Not every eligible source has a verified playback derivative")
    for name, digest in existing_playback.items():
        if sha256(library / "clips" / name) != digest:
            raise ValueError(f"Existing playback changed: {name}")
    write_json(stage / "manifest.json", manifest)
    subprocess.run([sys.executable, str(root / "tools/suggest_soundscape_cue_policy.py"),
                    "--manifest", str(stage / "manifest.json"), "--library-root", str(library),
                    "--output", str(stage / "policy.json"),
                    "--bindings-output", str(stage / "proposal.json")], check=True)
    proposal = json.loads((stage / "proposal.json").read_text())
    config, merge_receipt = merge_sorted(old["proposal.json"], old["soundscape.json"], proposal, manifest)
    write_json(stage / "soundscape.json", config)
    write_json(stage / "merge_receipt.json", merge_receipt)

    assets = {a["id"]: a for a in manifest["assets"]}
    cues = {c["asset"]: c for c in config["cues"]}
    eligible = {a["id"] for a in manifest["assets"] if a["default_enabled"]}
    assert len(cues) == expected_enabled and set(cues) == eligible
    assert all(assets[key]["playback_available"] for key in cues)
    assert all(key in cues and cues[key].get("enabled", True)
               for ids in config["events"].values() for key in ids)
    assert not any(a["default_enabled"] for a in manifest["assets"] if a["role"] in {"scene", "unused"})
    assert not any(name in config["events"] for name in ("incoming_riser", "incoming_hit_riser", "incoming_tank_riser"))
    for key, cue in cues.items():
        intent = assets[key]["filename_usage"]
        if intent["roles"] == ["combat"]:
            assert cue["role"] == "combat" and cue["role_aliases"] == ["combat"]
        if "night" in intent["contexts"]:
            assert all("night" in tags for alternatives in cue["role_contexts"].values() for tags in alternatives)
        if assets[key]["source_path"].startswith(("sunrise/", "sunset/", "dusk/")):
            assert not cue["loop"]
            assert assets[key]["loop"]["end_frame_exclusive"] == assets[key]["playback_format"]["frames"]
        if not assets[key]["musical_metadata"]["key"]["sources"]:
            assert assets[key]["musical_key"] is None
    riser = cues.get(TRANSITION_RISER_ID)
    assert riser and riser["riser_peak_s"] == 5.404943 and riser["riser_lead_s"] == 4.0
    assert config["events"]["intensity_transition_riser"] == [TRANSITION_RISER_ID]
    assert all(sha256(paths[name]) == digest for name, digest in hashes.items()), "Live JSON changed during staging"

    receipt = {
        "created_utc": datetime.now(timezone.utc).isoformat(), "source_scan": str(args.scan.resolve()),
        "source_scan_sha256": sha256(args.scan), "stage": str(stage),
        "source_files": inventory["source_files"], "unique_assets": len(assets),
        "enabled_cues": sum(c.get("enabled", True) for c in cues.values()),
        "saved_scan_enabled_before_authoring": originally_enabled,
        "finite_foreground_phrase_assignments": [{"asset": a["id"], "path": a["source_path"],
            **a["runtime_assignment"]} for a in assets.values()
            if a.get("runtime_assignment", {}).get("finite_foreground_phrase")],
        "deferred_effects": config.get("deferred_effects", []),
        "disabled_assets": [{"id": a["id"], "path": a["source_path"], "role": a["role"]}
                            for a in assets.values() if not a["default_enabled"]],
        "source_roles": inventory["roles"], "runtime_roles": dict(Counter(c["role"] for c in cues.values())),
        "named_effect_registry": config["named_effect_registry"], "events": config["events"],
        "new_playback_files": sorted(p.name for p in (library / "clips").glob("*.wav") if p.name not in existing_playback),
        "existing_playback_files_verified_unchanged": len(existing_playback),
        "backups": {name: {"path": str(backup / name), "sha256": digest} for name, digest in hashes.items()},
        "outputs": {name: {"staged": str(stage / name), "live": str(path), "sha256": sha256(stage / name)}
                    for name, path in paths.items()},
        "checks": {"all_enabled_have_verified_playback": True, "no_unresolved_runtime_cues": True,
                   "scene_and_unused_inactive": True, "combat_filename_cannot_join_calm_music": True,
                   "night_filename_context_enforced": True, "solar_cues_finite_full_source": True,
                   "unknown_musical_keys_not_invented": True, "intensity_riser_timing_preserved": True,
                   "existing_playback_unchanged": True, "old_live_json_unchanged_during_staging": True},
        "merge": merge_receipt, "promoted": False, "listened": False,
        "acceptance_limits": ["No auditory acceptance or gameplay bindings implied by metadata import",
                              "Numerical fade/rack/loop suggestions remain provisional unless explicitly authored",
                              "Original recordings unchanged; recorded mix and rights were not independently verified"],
    }
    write_json(stage / "receipt.json", receipt)
    if args.promote:
        # Prepare all replacements before replacing any live JSON. Exact prior bytes are
        # retained above; config is last, so no new selection can refer to an old manifest.
        for name, path in paths.items():
            pending = path.with_suffix(".json.sorted-pending")
            if pending.exists():
                raise FileExistsError(pending)
            shutil.copyfile(stage / name, pending)
        for name, path in paths.items():
            os.replace(path.with_suffix(".json.sorted-pending"), path)
            assert sha256(path) == receipt["outputs"][name]["sha256"]
        receipt["promoted"] = True
        receipt["promoted_utc"] = datetime.now(timezone.utc).isoformat()
        write_json(stage / "receipt.json", receipt)
    print(json.dumps({k: receipt[k] for k in ("stage", "source_files", "unique_assets", "enabled_cues", "promoted")}, indent=2))


if __name__ == "__main__":
    main()
