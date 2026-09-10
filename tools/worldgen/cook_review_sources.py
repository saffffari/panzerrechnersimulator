#!/usr/bin/env python3
"""Version a small source-placement calibration lane onto the existing Playtest map.

The speakers play the existing metal cue as test audio. This is not a reconstructed
historical event or natural ambience, and no archive or current Playtest is overwritten.
"""
import hashlib
import json
from pathlib import Path
import struct

import world_authoring as author

BASE = "assets/pizza/_extracted/PSX Mega Pack II v1.8/PSX Mega Pack II/Models/GLB (recommended)/"
SPEAKER = BASE + "Props/speaker_mx_1.glb"
BARREL = BASE + "Props/metal_barrel_hr_1.glb"
LAMP = BASE + "Light Sources/lamp_mx_1_a_off.glb"
CUE = "assets/sounds/hammer_strike.ogg"


def bounds(asset):
    with author.repo_path(asset).open("rb") as stream:
        if stream.read(4) != b"glTF":
            raise ValueError("expected GLB")
        stream.read(8)
        size, kind = struct.unpack("<II", stream.read(8))
        if kind != 0x4E4F534A:
            raise ValueError("missing GLB JSON")
        doc = json.loads(stream.read(size))
    # These selected props have one root, identity transforms and named, bounded primitives.
    # Reject different geometry rather than assuming a bounding box ignores node transforms.
    if any(any(k in n for k in ("matrix", "translation", "rotation", "scale")) for n in doc["nodes"]):
        raise ValueError("review fixture bounds need explicit transformed-node support")
    accessors = [doc["accessors"][p["attributes"]["POSITION"]]
                 for m in doc["meshes"] for p in m["primitives"]]
    return ([min(a["min"][i] for a in accessors) for i in range(3)],
            [max(a["max"][i] for a in accessors) for i in range(3)])


def cook(source="assets/worlds/astra-20260905/playtest/manifest.json"):
    world, heights, _ = author.load_world(source)
    ops = []
    geometry = {a: bounds(a) for a in (SPEAKER, BARREL, LAMP)}
    slots = []
    for ident, x, z in [("left", -4.0, 73.0), ("right", 4.0, 77.0)]:
        ground = author.sample_height(heights, world["terrain"]["spacing"], x, z)
        bmin, bmax = geometry[BARREL]
        barrel_y = ground - bmin[1]
        table_y = barrel_y + bmax[1]
        smin, smax = geometry[SPEAKER]
        speaker_scale = 0.55
        speaker_y = table_y - smin[1] * speaker_scale
        source_y = speaker_y + (smin[1] + smax[1]) * speaker_scale * 0.5
        lmin, lmax = geometry[LAMP]
        lamp_x, lamp_z = x, z + (0.65 if ident == "right" else -0.65)
        lamp_y = ground + 3.0 - lmax[1]
        for suffix, asset, pos, scale in [
            ("barrel", BARREL, [x, barrel_y, z], [1, 1, 1]),
            ("speaker", SPEAKER, [x, speaker_y, z], [speaker_scale]*3),
            ("lamp", LAMP, [lamp_x, lamp_y, lamp_z], [1, 1, 1]),
        ]:
            ops.append({"op":"place", "id":f"review-source-{ident}-{suffix}",
                        "asset":asset, "position_m":pos, "heading_deg":0, "scale":scale})
        sound = {"id":f"review_source_{ident}", "asset":CUE,
                 "position_m":[x,source_y,z], "volume":0.6, "radius_m":4.0,
                 "duration_s":0.0, "playback":"once",
                 "diagnostic_only":True,
                 "meaning":"Metal test cue played by the modeled calibration speaker; not a live strike or historical ambience"}
        light = {"id":f"review_lamp_{ident}", "kind":"point",
                 "position":[lamp_x,lamp_y+lmin[1]-0.04,lamp_z], "direction":[0,-1,0],
                 "temperature_k":3200, "tint_rgb":[1,1,1], "radiant_intensity":0.03,
                 "source_radius_m":0.025, "range_m":12, "cone_angle_deg":45,
                 "flicker":0, "enabled":True}
        ops.extend([{"op":"sound", "value":sound}, {"op":"light", "value":light}])
        slots.append({"id":ident, "sound":sound, "light":light})
    result = author.apply_transaction(source, author.world_hash(source), "playtest-source-review", ops)
    cue_path = author.repo_path(CUE)
    result["diagnostic_sources"] = {"cue":CUE, "cue_sha256":hashlib.sha256(cue_path.read_bytes()).hexdigest(),
        "cue_bytes":cue_path.stat().st_size, "source_bounds_m":geometry, "slots":slots,
        "scope":"Explicit source-placement calibration lane; not campaign content, historical sound or a live hammer strike",
        "light_scope":"Authored bounded point source below an owned fixture; radiance calibration and fixture support art remain pending review",
        "first_person_entry_xz":[-14,75], "first_person_look_target_xz":[14,75]}
    return result


if __name__ == "__main__":
    output = author.ROOT / "artifacts/astra-audit-20260905/source-placement-review-cook.json"
    if output.exists():
        raise SystemExit("receipt already exists; call cook() explicitly for a new version and choose a new receipt path")
    result = cook()
    output.write_text(json.dumps(result, indent=2)+"\n")
    print(result["manifest"])
