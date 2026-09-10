#!/usr/bin/env python3
"""Fork an immutable Playtest revision with a reserved, functional compiled-house anchor."""
import argparse
import json
import re

import numpy as np
import world_authoring as world


def cook(source, name):
    manifest, heights, edits = world.load_world(source)
    if manifest.get("authoring", {}).get("mode") != "playtest":
        raise ValueError("functional Playtest cook requires an explicitly authored Playtest source")
    if "runtime_feature_building" in manifest["authoring"]:
        raise ValueError("source already has a runtime feature anchor; preserve and edit that revision")
    world.materialize_edits(manifest, edits)
    if edits.get("scene_blocks"):
        raise ValueError("scene terrain overlays require an explicit bake before pad sculpting")
    cx, cz, reserve = -55.0, 155.0, 18.0
    marker_name = "Functional house"
    # The recipe reserves its empty neighborhood without removing any owned placement.
    for placement in manifest["placements"]:
        if abs(placement[1]-cx) < reserve and abs(placement[3]-cz) < reserve:
            raise ValueError("authored placement occupies the proposed feature reserve")
    if any(m["name"] == marker_name for m in manifest["markers"]):
        raise ValueError("source already uses the feature marker name")
    spacing = manifest["terrain"]["spacing"]
    ground = world.sample_height(heights, spacing, cx, cz)
    axis = (np.arange(len(heights), dtype=np.float32)-(len(heights)-1)*0.5)*spacing
    distance = np.hypot(np.maximum(abs(axis[None,:]-cx)-6,0),
                        np.maximum(abs(axis[:,None]-cz)-6,0))
    weight = np.clip(1-distance/6,0,1)
    weight = weight*weight*(3-2*weight)
    before = heights.copy()
    heights += (ground-heights)*weight
    source_text = (world.ROOT / "src/building_runtime.rs").read_text()
    runtime_assets = []
    for constant, body in re.findall(r"const (\w+_PATH): &str = concat!\((.*?)\);",source_text,re.S):
        path = "".join(re.findall(r'"([^"]+)"',body))
        asset = world.repo_path(world.model_path(path))
        runtime_assets.append({"constant":constant,"path":path,"bytes":asset.stat().st_size,
            "sha256":world.file_hash(asset)})
    if len(runtime_assets) != 9:
        raise ValueError("review the actual feature blueprint asset closure after source changes")
    manifest["markers"].append({"name":marker_name,"kind":"Monument","x":cx,"y":ground,"z":cz})
    manifest["authoring"]["runtime_feature_building"] = {
        "blueprint":"feature-v1","marker_name":marker_name,"reserved_half_extent_m":reserve,
        "editor_representation":"semantic marker; Game assembles the existing live Pizza blueprint",
        "initial_center_xz":[cx,cz],"anchor_authority":"effective editor marker XZ; runtime grounds against actual heightfield",
        "runtime_asset_closure":runtime_assets,
        "scope":"nonhistorical functional house: actual shelves, roof shelter, stairs, support damage and lights"}
    manifest["authoring"]["zones"].append({"id":"functional_house","center_m":[cx,ground,cz],
        "entrance_m":[cx-2,ground+1.65,cz-6.5],"purpose":"live compiled building mechanics and interior traversal"})
    recipe = {"mode":"functional-playtest-house","version":1,"marker_name":marker_name,
        "pad_center_xz":[cx,cz],"pad_half_extent_m":6,"apron_m":6,"max_height_change_m":float(np.max(abs(heights-before))),
        "source_world_hash":world.world_hash(source),"runtime_assets":runtime_assets}
    return world.publish(name,manifest,heights,edits,source=source,recipe=recipe)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source",default="assets/worlds/astra-20260905/playtest/manifest.json")
    parser.add_argument("--name",default="playtest-functional-house")
    parser.add_argument("--receipt",default="artifacts/astra-audit-20260905/playtest-functional-house-cook.json")
    args=parser.parse_args()
    receipt=world.repo_path(args.receipt,exists=False)
    if receipt.exists():raise ValueError("preserving previous cook receipt; specify a new path")
    result=cook(args.source,args.name)
    receipt.parent.mkdir(parents=True,exist_ok=True)
    receipt.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({"manifest":result["manifest"],"receipt":str(receipt),"status":"CPU cook only; runtime proof pending"}))


if __name__=="__main__":main()
