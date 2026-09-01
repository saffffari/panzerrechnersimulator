#!/usr/bin/env python3
"""Re-hash and structurally verify the Vinrax skeleton bake without Blender."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import skeleton_character_recipe as recipe


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "asset_root",
        type=Path,
        help="assets/flavor/Characters/skeleton-character-psx-vinrax-cc-by-4",
    )
    parser.add_argument(
        "--archive",
        type=Path,
        help="optionally re-open and hash-gate the private original archive",
    )
    arguments = parser.parse_args()
    root = arguments.asset_root.expanduser().resolve()
    report_path = root / recipe.REPORT_RELATIVE
    if not report_path.is_file():
        raise SystemExit(f"skeleton bake receipt does not exist: {report_path}")
    report = json.loads(report_path.read_text(encoding="utf-8"))
    verification = recipe.verify_bake_receipt(root, report)
    if arguments.archive is not None:
        with recipe.extract_reviewed_source(arguments.archive) as (_fbx, _texture, source):
            verification["source_archive_reverified"] = True
            verification["source_archive_sha256"] = source["archive"]["sha256"]
    else:
        verification["source_archive_reverified"] = False
    print(recipe.stable_json(verification), end="")


if __name__ == "__main__":
    main()
