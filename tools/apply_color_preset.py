#!/usr/bin/env python3
"""Copy a saved FEA range state with one named display-saturation change."""
import argparse
import copy
import json
import math
from pathlib import Path


def apply(state, preset):
    if preset.get("schema") != "panzer.display-color-preset.v1":
        raise ValueError("expected a panzer.display-color-preset.v1 preset")
    saturation = preset.get("display_saturation")
    if not isinstance(saturation, (int, float)) or not math.isfinite(saturation) or not 0 <= saturation <= 2:
        raise ValueError("display saturation must be finite and between 0 and 2")
    grade = state.get("look", {}).get("grade")
    if not isinstance(grade, list) or len(grade) != 4:
        raise ValueError("input must be a saved FEA range state with look.grade[4]")
    result = copy.deepcopy(state)
    result["look"]["grade"][1] = saturation
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preset", type=Path)
    parser.add_argument("state", type=Path)
    parser.add_argument("output", type=Path, help="new state path; existing files are never overwritten")
    args = parser.parse_args()
    result = apply(json.loads(args.state.read_text()), json.loads(args.preset.read_text()))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(args.output)


if __name__ == "__main__":
    main()
