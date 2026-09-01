#!/usr/bin/env python3
"""Re-hash and validate a completed AdrielCZ Panzer crew bake without Blender."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import tiger_crew_recipe as recipe


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "asset_root",
        type=Path,
        help="assets/flavor/Characters/german-panzertruppe-adrielcz-cc-by-4",
    )
    parser.add_argument(
        "--extraction-root",
        type=Path,
        help="optionally re-hash the private deterministic source extraction too",
    )
    arguments = parser.parse_args()
    root = arguments.asset_root.expanduser().resolve()
    report_path = root / "bake.report.json"
    if not report_path.is_file():
        raise SystemExit(f"Tiger crew bake receipt does not exist: {report_path}")
    extraction_root = (
        arguments.extraction_root.expanduser().resolve()
        if arguments.extraction_root is not None
        else None
    )
    report = json.loads(report_path.read_text(encoding="utf-8"))
    verification = recipe.verify_bake_receipt(
        root,
        report,
        extraction_root=extraction_root,
    )
    print(recipe.stable_json(verification), end="")


if __name__ == "__main__":
    main()
