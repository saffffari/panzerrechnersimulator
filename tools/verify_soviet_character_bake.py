#!/usr/bin/env python3
"""Re-hash and validate a completed Soviet character bake without Blender."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import soviet_character_recipe as recipe


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "asset_root",
        type=Path,
        help="assets/flavor/Characters/soviet-ww2-soldier-cc-by-4",
    )
    arguments = parser.parse_args()
    root = arguments.asset_root.expanduser().resolve()
    report_path = root / "bake.report.json"
    if not report_path.is_file():
        raise SystemExit(f"bake receipt does not exist: {report_path}")
    report = json.loads(report_path.read_text(encoding="utf-8"))
    verification = recipe.verify_bake_receipt(root, report)
    print(recipe.stable_json(verification), end="")


if __name__ == "__main__":
    main()
