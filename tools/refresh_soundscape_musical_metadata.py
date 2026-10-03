#!/usr/bin/env python3
"""Refresh stored filename musical metadata only; never scan/modify/decode original audio."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path

from catalog_panzer_sounds import apply_musical_metadata
from merge_soundscape_cue_policy import merge

MUSIC_FIELDS = ("bpm", "musical_key", "musical_metadata")


def refresh(manifest: dict, policy: dict, proposal: dict, config: dict) -> tuple:
    manifest, policy, proposal = (copy.deepcopy(v) for v in (manifest, policy, proposal))
    previous_proposal = copy.deepcopy(proposal)
    before = {a["id"]: (a.get("bpm"), a.get("musical_key"), a.get("key")) for a in manifest["assets"]}
    for asset in manifest["assets"]:
        apply_musical_metadata(asset)
    assets = {a["id"]: a for a in manifest["assets"]}
    for document, id_field in ((policy, "id"), (proposal, "asset")):
        for cue in document["cues"]:
            asset = assets[cue[id_field]]
            for field in MUSIC_FIELDS:
                if field == "musical_key" and asset[field] is None and id_field == "asset":
                    # Generated null would look like an authored request to clear a tonic hint.
                    cue.pop(field, None)
                else:
                    cue[field] = copy.deepcopy(asset[field])
            if id_field == "id":
                bpm = asset["bpm"]
                cue["filename_bpm"] = bpm
                cue["filename_bpm_unverified"] = bpm is not None
                beats = cue["envelope"]["duration_s"] * bpm / 60 if bpm is not None else None
                cue["estimated_beats_from_filename_bpm"] = beats
                cue["four_beat_period_residual_beats"] = abs(beats - round(beats / 4) * 4) if beats is not None else None
    manifest_hash = hashlib.sha256(encoded(manifest)).hexdigest()
    policy["manifest_sha256"] = proposal["manifest_sha256"] = manifest_hash
    config, merge_receipt = merge(previous_proposal, config, proposal)
    def counts(entries):
        return {"total": len(entries), "bpm_known": sum(a["bpm"] is not None for a in entries),
                "bpm_unknown": sum(a["bpm"] is None for a in entries),
                "full_key": sum(a["musical_metadata"]["key"]["status"] == "filename_key" for a in entries),
                "tonic_only": sum(a["musical_metadata"]["key"]["status"] == "tonic_only" for a in entries),
                "key_unknown": sum(a["musical_metadata"]["key"]["status"] == "unknown" for a in entries),
                "bpm_conflicts": sum(a["musical_metadata"]["bpm"]["status"] == "conflict" for a in entries),
                "key_conflicts": sum(a["musical_metadata"]["key"]["status"] == "conflict" for a in entries)}
    receipt = {"version": 1, "source_rescan": False, "audio_decoded": False,
               "originals_modified": False, "tempo_analyzed": False, "pitch_analyzed": False,
               "manifest_sha256": manifest_hash, "all_assets": counts(list(assets.values())),
               "runtime_assets": counts([assets[c["asset"]] for c in config["cues"]]),
               "label_changes": [{"id": a["id"], "source_path": a["source_path"], "before": before[a["id"]],
                                  "after": [a["bpm"], a["musical_key"], a["key"]]} for a in assets.values()
                                 if before[a["id"]] != (a["bpm"], a["musical_key"], a["key"])],
               "merge": merge_receipt}
    return manifest, policy, proposal, config, receipt


def encoded(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", type=Path, default=Path("."))
    args = p.parse_args()
    paths = [args.root / f for f in ("assets/audio/library/manifest.json", "catalog/soundscape_cue_policy_20261002.json",
             "catalog/soundscape_runtime_bindings_20261002.json", "assets/audio/library/soundscape.json")]
    originals = [p.read_bytes() for p in paths]
    result = refresh(*(json.loads(data) for data in originals))
    backup = args.root / "artifacts/runtime/audio_library_music_metadata_before_20261002"
    backup.mkdir(parents=True, exist_ok=True)
    for path, data in zip(paths, originals):
        saved = backup / path.name
        if not saved.exists():
            saved.write_bytes(data)
    for path, data, value in zip(paths, originals, result[:4]):
        if path.read_bytes() != data:
            raise RuntimeError(f"Concurrent change detected: {path}; no overwrite")
    for path, value in zip(paths, result[:4]):
        temp = path.with_suffix(path.suffix + ".tmp")
        temp.write_bytes(encoded(value))
        temp.replace(path)
    receipt = args.root / "artifacts/runtime/audio_library_musical_metadata_validation_20261002.json"
    receipt.write_bytes(encoded(result[4]))
    print(json.dumps({k: result[4][k] for k in ("all_assets", "runtime_assets", "manifest_sha256")}, indent=2))


if __name__ == "__main__":
    main()
