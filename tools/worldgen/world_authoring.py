#!/usr/bin/env python3
"""Versioned, non-destructive world/scene authoring shared by the CLI and editor MCP.

No renderer, Blender, shell evaluation, or source-world overwrite is involved. A transaction
publishes a new revision only after its complete manifest, terrain and source references validate.
"""
from __future__ import annotations

import argparse
import contextlib
import copy
import datetime as dt
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import struct
import zlib
import sys
import uuid

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
REVISION_ROOT = "assets/worldgen/revisions"
MAX_PLACEMENTS = 250_000
MAX_OPERATIONS = 4096


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def repo_path(raw, *, exists=True):
    if not isinstance(raw, str) or "\\" in raw:
        raise ValueError("expected canonical repository-relative path")
    p = Path(raw)
    if p.is_absolute() or any(x in (".", "..") for x in raw.split("/")):
        raise ValueError(f"unsafe path: {raw}")
    resolved = (ROOT / p).resolve()
    if not resolved.is_relative_to(ROOT.resolve()):
        raise ValueError(f"path escapes repository: {raw}")
    if exists and not resolved.is_file():
        raise ValueError(f"missing source: {raw}")
    return resolved


def load_json(raw):
    p = repo_path(raw)
    if p.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("authoring JSON exceeds 64 MiB bound")
    return json.loads(p.read_text(), parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))


def finite(value, name, low=-10000, high=10000):
    if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value) or not low <= value <= high:
        raise ValueError(f"{name} must be finite in [{low},{high}]")
    return float(value)


def vec3(value, name, low=-10000, high=10000):
    if not isinstance(value, list) or len(value) != 3:
        raise ValueError(f"{name} requires three numbers")
    return [finite(x, name, low, high) for x in value]


def model_path(value):
    if not value.startswith("assets/") or Path(value).suffix.lower() != ".glb":
        raise ValueError(f"not an asset GLB: {value}")
    repo_path(value)
    return value


def rotate(p, yaw):
    angle = math.radians(yaw)
    c, s = math.cos(angle), math.sin(angle)
    return [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]


def relocate(p, anchor, yaw):
    return [a + b for a, b in zip(rotate(p, yaw), anchor)]


def validate_environment(env):
    if not isinstance(env, dict):
        raise ValueError("environment must be an object")
    for key in ("sound_emitters", "authored_lights"):
        values = env.get(key, [])
        if not isinstance(values, list) or len(values) > 256:
            raise ValueError(f"{key} must contain at most 256 sources")
        ids = set()
        for value in values:
            ident = value.get("id", "")
            if not re.fullmatch(r"[A-Za-z0-9_-]+", ident) or ident in ids:
                raise ValueError(f"invalid or duplicate {key} id {ident!r}")
            ids.add(ident)
            if key == "sound_emitters":
                asset = value.get("asset", "")
                if not asset.startswith("assets/sounds/") or not asset.endswith(".ogg"):
                    raise ValueError("scene sound must be a cooked assets/sounds/*.ogg")
                repo_path(asset)
                vec3(value.get("position_m"), "sound position")
                finite(value.get("volume"), "volume", 0, 4)
                finite(value.get("radius_m"), "radius_m", .5, 2000)
                finite(value.get("duration_s"), "duration_s", 0, 3600)
                if value.get("playback") not in ("loop", "once", "reenter"):
                    raise ValueError("unknown playback policy")
            else:
                if value.get("kind") != "point":
                    raise ValueError("current runtime scene sources support point lights only")
                vec3(value.get("position"), "light position")
                vec3(value.get("direction"), "light direction")
                vec3(value.get("tint_rgb"), "light tint", 0, 100)
                for name, lo, hi in (("temperature_k",800,40000),("radiant_intensity",0,1e6),
                    ("source_radius_m",.01,100),("range_m",.25,10000),("cone_angle_deg",1,179),("flicker",0,1)):
                    finite(value.get(name), name, lo, hi)
                if not isinstance(value.get("enabled"), bool):
                    raise ValueError("light enabled must be boolean")
    return env


def validate_scene(scene):
    if scene.get("schema") != "kessel.scene-block" or scene.get("version") != 1:
        raise ValueError("expected kessel.scene-block v1")
    if scene.get("anchor") != {"origin_m":[0,0,0],"forward":"+z","grounding":"rigid_anchor"}:
        raise ValueError("scene must preserve its local rigid datum")
    terrain = scene.get("terrain", {})
    res = terrain.get("res", 0)
    if not isinstance(res, int) or not 2 <= res <= 513:
        raise ValueError("scene resolution must be 2..513")
    spacing = finite(terrain.get("spacing_m"), "terrain spacing", .0001, 100)
    finite(terrain.get("apron_m"), "terrain apron", 2 * spacing, 500)
    if terrain.get("datum") != "anchor_ground" or len(terrain.get("heights_m", [])) != res * res:
        raise ValueError("scene terrain datum or sample count invalid")
    for height in terrain["heights_m"]:
        finite(height, "terrain height", -1000, 1000)
    items = scene.get("items", [])
    if not 1 <= len(items) <= MAX_PLACEMENTS:
        raise ValueError("scene requires bounded non-empty items")
    ids = set()
    for item in items:
        model_path(item["asset"])
        for key in ("dx","dy","dz","deg"):
            finite(item.get(key), key)
        vec3(item.get("s"), "scale", .00001, 1000)
        if "id" in item:
            if not re.fullmatch(r"[A-Za-z0-9_-]+", item["id"]) or item["id"] in ids:
                raise ValueError("scene item IDs must be valid and unique")
            ids.add(item["id"])
    provenance = scene.get("metadata", {}).get("provenance", {})
    if provenance.get("placement_class") not in ("exact_landmark","district_event","attested_condition"):
        raise ValueError("scene needs explicit provenance class")
    for key in ("source_ids","source_urls","placement_constraints"):
        if not provenance.get(key) or not all(isinstance(x,str) and x for x in provenance[key]):
            raise ValueError(f"scene requires provenance.{key}")
    if any(not x.startswith(("http://","https://")) for x in provenance["source_urls"]):
        raise ValueError("source URLs must be HTTP(S)")
    if any(not re.fullmatch(r"[a-z0-9_]+",x) for x in provenance["placement_constraints"]):
        raise ValueError("placement constraints must be lowercase tags")
    for key in ("sockets","connectors"):
        ids = set()
        for item in scene.get(key, []):
            if not item.get("id") or item["id"] in ids or not item.get("kind"):
                raise ValueError(f"invalid/duplicate {key} ID")
            ids.add(item["id"])
            vec3(item.get("position_m"), key)
            finite(item.get("heading_deg"), "heading")
    validate_environment(scene.get("environment", {}))
    return scene


def placement_ids(manifest):
    ids = manifest.get("placement_ids")
    if ids is None:
        seen, ids = {}, []
        for p in manifest["placements"]:
            key = digest([manifest["assets"][p[0]], *p[1:]])[:20]
            occurrence = seen.get(key, 0)
            seen[key] = occurrence + 1
            ids.append(f"p_{key}_{occurrence}")
    if not isinstance(ids,list) or not all(isinstance(x,str) and re.fullmatch(r"[A-Za-z0-9_-]+",x) for x in ids) or len(ids) != len(manifest["placements"]) or len(set(ids)) != len(ids):
        raise ValueError("placement_ids must uniquely cover every placement")
    return ids


def load_world(raw):
    value = load_json(raw)
    if value.get("schema") != "kessel.act" or value.get("version") != 1 or value.get("act_id") not in (1,2,3):
        raise ValueError("expected kessel.act v1 with climate act_id1..3")
    if not 0 <= len(value.get("placements", [])) <= MAX_PLACEMENTS:
        raise ValueError("world placement budget exceeded")
    for asset in value["assets"]:
        model_path(asset)
    for p in value["placements"]:
        if len(p) != 6 or not isinstance(p[0], int) or not 0 <= p[0] < len(value["assets"]):
            raise ValueError("malformed placement")
        for v in p[1:5]:
            finite(v, "placement coordinate")
        vec3(p[5] if isinstance(p[5],list) else [p[5]]*3, "placement scale", .00001, 1000)
    value["placement_ids"] = placement_ids(value)
    kinds = [m.get("kind") for m in value.get("markers", [])]
    if kinds.count("Spawn") != 1 or kinds.count("Goal") != 1 or not any(k not in ("Spawn","Goal") for k in kinds):
        raise ValueError("world requires exactly one Spawn/Goal and a landmark")
    base = repo_path(raw).parent
    relative = value["terrain"]["heights"]
    if Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise ValueError("terrain must be adjacent to its versioned manifest")
    height_path = repo_path((base / relative).relative_to(ROOT).as_posix())
    if height_path.stat().st_size > 64 * 1024 * 1024: raise ValueError("terrain exceeds 64 MiB bound")
    heights = np.load(height_path, allow_pickle=False, mmap_mode="r")
    if heights.dtype != np.float32 or heights.ndim != 2 or heights.shape != (value["terrain"]["res"],)*2 or not np.isfinite(heights).all():
        raise ValueError("terrain requires square finite float32 samples matching declared resolution")
    finite(value["terrain"]["spacing"], "world terrain spacing", .0001, 1000)
    repo_path(value["terrain"]["texture"])
    edits_path = base / "manifest_edits.json"
    edits = json.loads(edits_path.read_text()) if edits_path.exists() else {}
    if edits_path.exists():
        if not isinstance(edits.get("ops"),list):raise ValueError("manifest_edits.json ops must be an array")
        if edits.get("base_placements",len(value["placements"]))!=len(value["placements"]):raise ValueError("overlay base placement count mismatch")
        if edits.get("base_markers",len(value["markers"]))!=len(value["markers"]):raise ValueError("overlay base marker count mismatch")
    validate_environment(edits)
    return value, heights.copy(), edits


def file_hash(path):
    h=hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024*1024),b""): h.update(chunk)
    return h.hexdigest()


def world_hash(raw):
    # Includes the sidecar edit content, so stale clients cannot overwrite another author's edits.
    path = repo_path(raw)
    sidecar = path.parent / "manifest_edits.json"
    manifest=json.loads(path.read_text())
    terrain_path=repo_path((path.parent / manifest["terrain"]["heights"]).relative_to(ROOT).as_posix())
    return digest([manifest, json.loads(sidecar.read_text()) if sidecar.exists() else None, file_hash(terrain_path)])


def materialize_edits(manifest, edits):
    placements = manifest["placements"]
    ids = manifest["placement_ids"]
    if edits.get("ops") and edits.get("base_placements") != len(placements):
        raise ValueError("overlay base count mismatch; rebase explicitly before cooking")
    if any(op.get("op", "").startswith("marker_") for op in edits.get("ops", [])) and edits.get("base_markers") != len(manifest["markers"]):
        raise ValueError("overlay marker count mismatch")
    dead, marker_dead = set(), set()
    for op in edits.get("ops", []):
        kind = op["op"]
        if kind in ("set","delete"):
            i = op["i"]
            if not isinstance(i,int) or not 0 <= i < len(placements):
                raise ValueError("orphaned placement edit")
            if kind == "delete": dead.add(i)
            else: placements[i][1:] = [op[k] for k in ("x","y","z","deg","s")]
        elif kind == "add":
            asset = model_path(op["asset"])
            if asset not in manifest["assets"]: manifest["assets"].append(asset)
            placements.append([manifest["assets"].index(asset),*[op[k] for k in ("x","y","z","deg","s")]])
            ids.append("edit_" + digest(op)[:20] + f"_{len(ids)}")
        elif kind == "marker_add":
            manifest["markers"].append({k:op[k] for k in ("name","kind","x","y","z")})
        elif kind in ("marker_set","marker_delete"):
            if not 0 <= op["i"] < len(manifest["markers"]): raise ValueError("orphaned marker edit")
            if kind == "marker_delete": marker_dead.add(op["i"])
            else: manifest["markers"][op["i"]].update({k:op[k] for k in ("x","y","z")})
        else: raise ValueError(f"unsupported overlay op {kind}")
    manifest["placements"] = [p for i,p in enumerate(placements) if i not in dead]
    manifest["placement_ids"] = [p for i,p in enumerate(ids) if i not in dead]
    manifest["markers"] = [p for i,p in enumerate(manifest["markers"]) if i not in marker_dead]
    edits["ops"] = []
    edits["base_placements"] = len(manifest["placements"])
    edits["base_markers"] = len(manifest["markers"])


def sample_height(heights, spacing, x, z):
    n = len(heights)
    gx, gz = [max(0,min(n-1,p/spacing+(n-1)/2)) for p in (x,z)]
    ix, iz = int(gx), int(gz)
    fx, fz = gx-ix, gz-iz
    return float((heights[iz,ix]*(1-fx)+heights[iz,min(ix+1,n-1)]*fx)*(1-fz)
        +(heights[min(iz+1,n-1),ix]*(1-fx)+heights[min(iz+1,n-1),min(ix+1,n-1)]*fx)*fz)


def stamp_terrain(heights, spacing, terrain, anchor, yaw):
    n = len(heights); half = (terrain["res"]-1)*terrain["spacing_m"]/2
    reach = (half+terrain["apron_m"])*math.sqrt(2)
    x0,z0 = [max(0,math.floor((p-reach)/spacing+(n-1)/2)) for p in (anchor[0],anchor[2])]
    x1,z1 = [min(n-1,math.ceil((p+reach)/spacing+(n-1)/2)) for p in (anchor[0],anchor[2])]
    local = np.asarray(terrain["heights_m"],np.float32).reshape((terrain["res"],)*2)
    touched = 0; core_samples = 0
    for iz in range(z0,z1+1):
        for ix in range(x0,x1+1):
            p = rotate([(ix-(n-1)/2)*spacing-anchor[0],0,(iz-(n-1)/2)*spacing-anchor[2]],-yaw)
            outside = math.hypot(max(abs(p[0])-half,0),max(abs(p[2])-half,0))
            if outside >= terrain["apron_m"]: continue
            if outside == 0: core_samples += 1
            t = max(0,1-outside/terrain["apron_m"]); weight=t*t*(3-2*t)
            target = anchor[1]+sample_height(local,terrain["spacing_m"],max(-half,min(half,p[0])),max(-half,min(half,p[2])))
            heights[iz,ix] += (target-float(heights[iz,ix]))*weight
            touched += 1
    if not core_samples: raise ValueError("scene terrain core lies outside destination map")
    return touched


def add_scene(manifest, heights, edits, scene, source, anchor, yaw, instance_id, *, diagnostic=False):
    validate_scene(scene)
    provenance = scene["metadata"]["provenance"]
    if provenance["placement_class"] == "exact_landmark" and not diagnostic:
        exact = provenance["exact_anchor"]
        if manifest["act_id"] != exact["act_id"] or math.hypot(anchor[0]-exact["x"],anchor[2]-exact["z"]) > exact["tolerance_m"]:
            raise ValueError("exact landmark cannot be relocated into this campaign position")
    if not diagnostic:
        tags = set(manifest.get("authoring",{}).get("compatibility_tags",[]))
        if not set(provenance["placement_constraints"]).issubset(tags):
            raise ValueError("destination lacks the scene's sourced placement compatibility tags")
    stamp_terrain(heights,manifest["terrain"]["spacing"],scene["terrain"],anchor,yaw)
    for i,item in enumerate(scene["items"]):
        asset=item["asset"]
        if asset not in manifest["assets"]: manifest["assets"].append(asset)
        p=relocate([item["dx"],item["dy"],item["dz"]],anchor,yaw)
        manifest["placements"].append([manifest["assets"].index(asset),*p,item["deg"]+yaw,item["s"]])
        manifest["placement_ids"].append(f"{instance_id}_{item.get('id',str(i))}")
    for key,position in (("sound_emitters","position_m"),("authored_lights","position")):
        for source_value in scene.get("environment",{}).get(key,[]):
            value=copy.deepcopy(source_value)
            value["id"]=f"{instance_id}_{value['id']}"
            value[position]=relocate(value[position],anchor,yaw)
            if key=="authored_lights": value["direction"]=rotate(value["direction"],yaw)
            edits.setdefault(key,[]).append(value)
    records=manifest.setdefault("authoring",{}).setdefault("scene_instances",[])
    records.append({"id":instance_id,"source":source,"source_sha256":digest(scene),"anchor_m":anchor,"yaw_deg":yaw,
        "diagnostic_relocation":diagnostic,"provenance":provenance,"sockets":[dict(p,position_m=relocate(p["position_m"],anchor,yaw),heading_deg=p["heading_deg"]+yaw) for p in scene.get("sockets",[])]})


def publish(name, manifest, heights, edits, *, source=None, recipe=None):
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,63}",name):
        raise ValueError("revision name must be a short lowercase slug")
    manifest=copy.deepcopy(manifest); edits=copy.deepcopy(edits)
    manifest["placement_ids"]=placement_ids(manifest)
    edits.update(base_placements=len(manifest["placements"]),base_markers=len(manifest["markers"]))
    edits.setdefault("ops", [])
    manifest["terrain"].update(heights="heights.npy",res=len(heights))
    stamp=dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    relative=f"{REVISION_ROOT}/{name}/{stamp}_{uuid.uuid4().hex[:8]}"
    output=repo_path(relative,exists=False)
    staging=output.with_name(".pending_"+output.name); staging.mkdir(parents=True,exist_ok=False)
    pending_relative=staging.relative_to(ROOT).as_posix()
    try:
        np.save(staging/"heights.npy",np.asarray(heights,dtype=np.float32))
        (staging/"manifest.v1.json").write_bytes(canonical(manifest)+b"\n")
        (staging/"manifest_edits.json").write_bytes(canonical(edits)+b"\n")
        load_world(pending_relative+"/manifest.v1.json")
        receipt={"schema":"kessel.world-revision","version":1,"created_utc":stamp,"source":source,
            "recipe":recipe,"manifest_sha256":digest(manifest),"terrain_sha256":hashlib.sha256(heights.astype('<f4').tobytes()).hexdigest(),
            "placements":len(manifest["placements"]),"unique_assets":len(manifest["assets"]),
            "sounds":len(edits.get("sound_emitters",[])),"lights":len(edits.get("authored_lights",[])),
            "status":"validated_cpu_only","rendered":False,
            "asset_closure":[{"path":a,"bytes":repo_path(a).stat().st_size} for a in manifest["assets"]]}
        (staging/"revision.v1.json").write_bytes(canonical(receipt)+b"\n")
    except Exception as error:
        (staging/"FAILED.txt").write_text(str(error))
        raise
    staging.rename(output)
    return {"manifest":relative+"/manifest.v1.json","revision":relative+"/revision.v1.json",**receipt}


def inspect_world(path, offset=0, limit=40):
    world, heights, edits=load_world(path)
    offset=int(offset);limit=max(0,min(100,int(limit)))
    return {"manifest":path,"revision_hash":world_hash(path),"act_id":world["act_id"],"name":world["act"],
        "terrain":{"res":len(heights),"spacing":world["terrain"]["spacing"],"min":float(heights.min()),"max":float(heights.max())},
        "placements_total":len(world["placements"]),"placements":[{"id":ident,"asset":world["assets"][p[0]],"transform":p[1:]} for ident,p in zip(world["placement_ids"][offset:offset+limit],world["placements"][offset:offset+limit])],
        "markers":world["markers"],"environment":{k:edits.get(k,[]) for k in ("sound_emitters","authored_lights")},"authoring":world.get("authoring",{})}


def fork_world(source, name):
    world,heights,edits=load_world(source)
    # Terrain instances are already applied by runtime on load. Keep their relative source data;
    # no resampling or destructive flattening is required for an ordinary version fork.
    return publish(name,world,heights,edits,source={"manifest":source,"hash":world_hash(source)},recipe={"mode":"fork"})


def apply_transaction(source, expected_hash, name, operations):
    if world_hash(source)!=expected_hash: raise ValueError("revision conflict: read the current world before applying edits")
    if not isinstance(operations,list) or len(operations)>MAX_OPERATIONS: raise ValueError("operation budget exceeded")
    world,heights,edits=load_world(source)
    if edits.get("scene_blocks"):
        # Existing terrain overlays must not be silently baked twice when making a new sculpt.
        raise ValueError("transaction requires explicit scene-terrain overlay bake; fork remains available")
    materialize_edits(world,edits)
    for op in operations:
        kind=op.get("op")
        if kind in ("move","remove"):
            if op["id"] not in world["placement_ids"]: raise ValueError(f"unknown placement ID {op['id']}")
            i=world["placement_ids"].index(op["id"])
            if kind=="remove":
                world["placement_ids"].pop(i);world["placements"].pop(i)
            else:
                world["placements"][i][1:]=[*vec3(op["position_m"],"position"),finite(op["heading_deg"],"heading"),vec3(op["scale"],"scale",.00001,1000)]
        elif kind=="place":
            model_path(op["asset"])
            if op["id"] in world["placement_ids"]: raise ValueError("duplicate placement identity")
            if op["asset"] not in world["assets"]:world["assets"].append(op["asset"])
            world["placements"].append([world["assets"].index(op["asset"]),*vec3(op["position_m"],"position"),finite(op["heading_deg"],"heading"),vec3(op["scale"],"scale",.00001,1000)])
            world["placement_ids"].append(op["id"])
        elif kind in ("sound","light"):
            key="sound_emitters" if kind=="sound" else "authored_lights"
            validate_environment({key:[op["value"]]})
            edits[key]=[v for v in edits.get(key,[]) if v["id"]!=op["value"]["id"]]+[op["value"]]
        elif kind=="sculpt":
            center=vec3(op["center_m"],"sculpt center");radius=finite(op["radius_m"],"radius",.5,128);amount=finite(op["amount_m"],"amount",-10,10)
            n=len(heights);spacing=world["terrain"]["spacing"]
            axis=(np.arange(n,dtype=np.float32)-(n-1)/2)*spacing
            distance=np.hypot(axis[None,:]-center[0],axis[:,None]-center[2])/radius
            weight=np.clip(1-distance,0,1);heights+=amount*weight*weight*(3-2*weight)
        elif kind=="stamp_scene":
            anchor=vec3(op["anchor_m"],"scene anchor");scene=load_json(op["source"])
            add_scene(world,heights,edits,scene,op["source"],anchor,finite(op["heading_deg"],"heading"),op["id"],diagnostic=world.get("authoring",{}).get("mode") in ("scene","playtest"))
        else: raise ValueError(f"unknown authoring operation {kind!r}")
    return publish(name,world,heights,edits,source={"manifest":source,"hash":expected_hash},recipe={"mode":"transaction","operations":operations})


def cook_scene(source, name, act_id=2):
    scene=validate_scene(load_json(source));terrain=scene["terrain"]
    half=(terrain["res"]-1)*terrain["spacing_m"]/2+terrain["apron_m"]+20
    spacing=min(terrain["spacing_m"],2.0);res=min(1025,2*math.ceil(half/spacing)+1)
    spacing=2*half/(res-1);heights=np.zeros((res,res),np.float32)
    world={"schema":"kessel.act","version":1,"act_id":act_id,"act":scene["name"],"seed":int(digest(scene)[:8],16),
        "terrain":{"res":res,"spacing":spacing,"heights":"heights.npy","texture":f"assets/terrain/act{act_id}/act{act_id}_ground_hires.png" if act_id!=1 else "assets/terrain/act1_rgb/act1_ground_hires.png"},
        "assets":[],"placements":[],"placement_ids":[],"markers":[],"authoring":{"mode":"scene","campaign_canonical":False,"source":source}}
    edits={};add_scene(world,heights,edits,scene,source,[0,0,0],0,"scene",diagnostic=True)
    sockets={s["id"]:s for s in scene["sockets"]}
    entry=sockets.get("first_person_entry",{"position_m":[0,1.65,half-8]})["position_m"]
    target=sockets.get("first_person_look_target",{"position_m":[0,1.65,0]})["position_m"]
    world["authoring"]["first_person"]={"eye_m":entry,"target_m":target,"source":"authored scene sockets; grounded foot entry must be validated in game"}
    world["markers"]=[{"name":"Scene entry","kind":"Spawn","x":entry[0],"y":sample_height(heights,spacing,entry[0],entry[2]),"z":entry[2]},
        {"name":"Scene exit","kind":"Goal","x":entry[0],"y":0,"z":min(half-2,entry[2]+5)},
        {"name":"Scene study","kind":"Monument","x":0,"y":0,"z":0}]
    return publish(name,world,heights,edits,source={"scene":source,"hash":digest(scene)},recipe={"mode":"scene","act_id":act_id})


def asset_search(query="", limit=40):
    needle=query.lower();limit=max(1,min(100,int(limit)));out=[]
    catalog=json.loads((ROOT/"catalog/runtime_assets.v1.json").read_text())
    known={a["source"]:a for a in catalog["assets"]}
    for directory,folders,files in os.walk(ROOT/"assets/pizza/_extracted",followlinks=False):
        folders[:]=sorted(f for f in folders if not f.startswith(".") and "backup" not in f.lower())
        for filename in sorted(files):
            if not filename.lower().endswith(".glb") or filename.endswith("_orig.glb"):continue
            path=(Path(directory)/filename).relative_to(ROOT).as_posix()
            if needle not in path.lower():continue
            entry=known.get(path,{})
            out.append({"asset":path,"name":Path(filename).stem,"catalog_id":entry.get("id"),"bounds_m":entry.get("bounds_m"),"owned_source":"Pizza bundle","bounds_verified":bool(entry)})
            if len(out)>=limit:return out
    return out


def generator_module():
    spec=importlib.util.spec_from_file_location("kessel_versioned_generator",ROOT/"tools/worldgen/gen_world.py")
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module


def bare_tree(source):
    """Keep actual branch/trunk nodes; preserve all source geometry bytes and the original GLB."""
    data=repo_path(source).read_bytes()
    if len(data)>16*1024*1024 or data[:4]!=b"glTF":raise ValueError("invalid bounded source GLB")
    length,kind=struct.unpack_from("<II",data,12)
    if kind!=0x4e4f534a:raise ValueError("GLB JSON chunk missing")
    document=json.loads(data[20:20+length]);nodes=document["nodes"]
    leaves={i for i,node in enumerate(nodes) if "leaves" in node.get("name","").lower()}
    if not leaves:raise ValueError("source has no separable leaf node")
    for node in nodes:
        if "children" in node:node["children"]=[i for i in node["children"] if i not in leaves]
    for scene in document["scenes"]:scene["nodes"]=[i for i in scene["nodes"] if i not in leaves]
    packed=canonical(document);packed+=b" "*((-len(packed))%4)
    tail=data[20+length:]
    result=struct.pack("<4sII",b"glTF",2,20+len(packed)+len(tail))+struct.pack("<II",len(packed),kind)+packed+tail
    key=hashlib.sha256(result).hexdigest()[:16]
    relative=f"assets/worldgen/derived/{key}/{Path(source).stem}_bare.glb"
    output=repo_path(relative,exists=False);output.parent.mkdir(parents=True,exist_ok=True)
    if not output.exists():
        output.write_bytes(result)
        output.with_suffix(".source.json").write_bytes(canonical({"source":source,"source_sha256":hashlib.sha256(data).hexdigest(),"operation":"exclude named leaves from scene graph; unchanged binary geometry","excluded_nodes":sorted(leaves)})+b"\n")
    elif output.read_bytes()!=result:raise ValueError("derived asset identity collision")
    return relative


def write_png(path, rgb):
    """Small stdlib PNG encoder; source images are decoded only by the existing GDAL tool."""
    a=np.asarray(rgb,np.uint8);height,width=a.shape[:2]
    def chunk(kind,data):return struct.pack(">I",len(data))+kind+data+struct.pack(">I",zlib.crc32(kind+data)&0xffffffff)
    body=b"".join(b"\0"+row.tobytes() for row in a)
    path.write_bytes(b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",struct.pack(">IIBBBBB",width,height,8,2,0,0,0))+chunk(b"IDAT",zlib.compress(body,6))+chunk(b"IEND",b""))


def playtest_ground(world):
    # Preserve Pizza's actual RGB. Roads and grove litter select their source material in world
    # coordinates; this is an authored material blend, not an exposure or hue substitution.
    spec=importlib.util.spec_from_file_location("kessel_ground_tiles",ROOT/"tools/worldgen/make_ground_tex.py")
    module=importlib.util.module_from_spec(spec)
    sys.path.insert(0,str(ROOT/"tools/worldgen"))
    try:spec.loader.exec_module(module)
    finally:sys.path.pop(0)
    base="source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/"
    names=["grass_7.png","dirt_5.png","dirt_2.png"]
    tiles=[module.read_png(str(repo_path(base+name))) for name in names]
    n=1024;axis=np.linspace(-384,384,n,dtype=np.float32);x=axis[None,:];z=axis[:,None]
    road=np.full((n,n),1e6,np.float32)
    for x0,z0,x1,z1 in world.roads:
        dx,dz=x1-x0,z1-z0;t=np.clip(((x-x0)*dx+(z-z0)*dz)/max(1,dx*dx+dz*dz),0,1)
        road=np.minimum(road,np.hypot(x-x0-t*dx,z-z0-t*dz))
    dirt=np.clip((8-road)/3,0,1)
    grove=np.clip(1-(((x-220)/72)**2+((z-100)/80)**2),0,1)
    fields=[]
    for tile in tiles:
        ix=(np.floor((axis+384)/24*tile.shape[1]).astype(int)%tile.shape[1])
        iz=(np.floor((axis+384)/24*tile.shape[0]).astype(int)%tile.shape[0])
        fields.append(tile[iz[:,None],ix[None,:]].astype(np.float32))
    # Blend reflectance in linear light, then encode the resulting albedo once.
    def linear(v):v=v/255;return np.where(v<=.04045,v/12.92,((v+.055)/1.055)**2.4)
    grass,soil,litter=map(linear,fields)
    rgb=grass*(1-grove[...,None])+litter*grove[...,None]
    rgb=rgb*(1-dirt[...,None])+soil*dirt[...,None]
    rgb=np.where(rgb<=.0031308,rgb*12.92,1.055*rgb**(1/2.4)-.055)
    rgb=np.round(np.clip(rgb,0,1)*255).astype(np.uint8)
    key=hashlib.sha256(rgb.tobytes()).hexdigest()[:16]
    relative=f"assets/worldgen/derived/{key}/playtest_ground.png"
    path=repo_path(relative,exists=False);path.parent.mkdir(parents=True,exist_ok=True)
    if not path.exists():
        write_png(path,rgb)
        path.with_suffix(".source.json").write_bytes(canonical({"sources":[base+name for name in names],"mapping":"24 m tile repeat; graded road soil and grove leaf litter","color":"source RGB; linear reflectance blend; no exposure remap"})+b"\n")
    return relative


def make_playtest(name="playtest", seed=20260905, install_default=False):
    gw=generator_module();world=gw.World.__new__(gw.World)
    world.act_id=2;world.seed=int(seed)&0xffffffff;world.act={"name":"Playtest — authored diagnostic composite","ruination":.5,"half_extent":384.0}
    for key in ("assets","placements","markers","wires","roads","craters","town_pts","tactical"):setattr(world,key,[])
    world.asset_idx={};world.res=385;world.spacing=2.0
    axis=(np.arange(world.res,dtype=np.float32)-(world.res-1)/2)*world.spacing
    world._gx=axis[None,:];world._gz=axis[:,None]
    world.heights=(.6*np.sin(world._gx/90)+.45*np.cos(world._gz/110)+.25*np.sin((world._gx+world._gz)/45)).astype(np.float32)
    world.heights-=1.6*np.exp(-((world._gx-160)/5)**2)*np.exp(-((world._gz-110)/110)**4)
    world.roads=[[0,280,0,-310],[-170,90,120,90],[0,145,215,145],[-110,90,-110,-85],[-130,-40,110,-40]]
    for road in world.roads:world.flatten_road(*road)
    zones=[]
    def pad(x,z,hx,hz):
        height=world.ground(x,z);distance=np.hypot(np.maximum(abs(world._gx-x)-hx,0),np.maximum(abs(world._gz-z)-hz,0));weight=np.clip(1-distance/6,0,1)
        world.heights+=(height-world.heights)*weight
    oldcwd=Path.cwd()
    try:
        os.chdir(ROOT)
        # Validate every recipe source before calling the legacy generator, which otherwise skips
        # missing files. Nothing from the opaque-building/container/wreck recipes is used here.
        required=["wall","wall_win","wall_hole","doorway","window","floor","floor_hole","roof_pitch","roof_flat","pillar","pillar_broken","stairs","rubble_field","rubble_stack","brick","furn_bed","furn_table","furn_chair","furn_cabinet","crate","crate2","barrel","barrel2","tree_b","tree_c","stone","stone2","stone3","stone4","stone5","log","stump","fern","fern2","fence_wood"]
        for key in required:model_path(gw.KIT[key])
        for key in ("tree_b","tree_c"):gw.KIT[key]=bare_tree(gw.KIT[key])
        for i,(kind,x,z) in enumerate([("garage",-65,90),("warehouse",-110,30),("shed",235,135)]):
            pad(x,z,16,12);world.modular_outbuilding(kind,x,z,0xAA00+i,ruination=.08)
            zones.append({"id":kind,"center_m":[x,world.ground(x,z),z],"entrance_m":[x,world.ground(x,z)+1.65,z+({"garage":6,"warehouse":8,"shed":4}[kind])+3],"purpose":"actual modular floor, doorway and interior side bays; traversal needs in-game validation"})
        for i,(x,z) in enumerate([(-70,-60),(0,-65),(70,-55),(-110,-100),(45,-110)]):
            pad(x,z,14,14);world.building(x,z,0xBB00+i,.62)
        # Open bankline and a loose bare grove, not a uniform forest or summer canopy.
        for i in range(52):
            angle=world.rnd(i,42)*math.tau;radius=15+world.rnd(i,43)*57
            x=220+math.cos(angle)*radius;z=85+math.sin(angle)*radius
            if world.road_dist(x,z)<9:continue
            world.place_ground("tree_b" if i%2 else "tree_c",x,z,world.rnd(i,44)*math.tau,.8+world.rnd(i,45)*.35)
        for i in range(22):
            x=132+world.rnd(i,55)*12;z=-50+i*10
            if world.road_dist(x,z)>10:world.place_ground("tree_c",x,z,world.rnd(i,54)*math.tau,.65+world.rnd(i,56)*.3)
        for i in range(150):
            x=175+world.rnd(i,60)*120;z=25+world.rnd(i,61)*140
            if world.road_dist(x,z)<8:continue
            key=("stone","stone2","stone3","stone4","stone5","log","stump","fern","fern2")[i%9]
            world.place_ground(key,x,z,world.rnd(i,62)*math.tau,.35+world.rnd(i,63)*.55)
        for x in (-110,0,110):
            for z in (-180,-240,-300):
                # Earth berms stop the sight line behind targets. Actual opponents are spawned by
                # ordinary Game systems; no mesh-only dummy tank passes as an interactive target.
                world.heights+=2.4*np.exp(-((world._gx-x)/14)**4-((world._gz-(z-15))/4)**2)
        for i in range(14):
            x=-100+i*8;z=123
            if abs(x)>10:world.place_ground("fence_wood",x,z,0,1)
        texture=playtest_ground(world)
    finally:os.chdir(oldcwd)
    for label,kind,x,z in [("Playtest entry","Spawn",0,180),("Range end","Goal",0,-325),("Garage","Monument",-65,90),("Warehouse","Monument",-110,30),("Bare grove","Monument",220,100),("Ruined street","Monument",0,-60)]:
        world.markers.append({"name":label,"kind":kind,"x":x,"y":world.ground(x,z),"z":z})
    manifest={"schema":"kessel.act","version":1,"act_id":2,"act":world.act["name"],"seed":world.seed,"terrain":{"res":world.res,"spacing":world.spacing,"heights":"heights.npy","texture":texture},"assets":world.assets,"placements":world.placements,"markers":world.markers,"wires":[],"authoring":{"mode":"playtest","campaign_canonical":False,"historical_claim":"diagnostic composite; not a reconstructed place or date","map_extent_m":768,"zones":zones,"roads":world.roads,"target_lanes":{"x_m":[-110,0,110],"z_m":[-180,-240,-300]},"first_person":{"eye_m":[0,world.ground(0,180)+1.65,180],"target_m":[0,world.ground(0,120)+1.65,120]},"season":"April-inspired bare deciduous structure with emerging ground vegetation; authored population"}}
    result=publish(name,manifest,world.heights,{},recipe={"mode":"playtest","seed":world.seed,"version":1})
    if install_default:
        # One-time seed installation only. Later revisions are selected explicitly; no existing
        # menu world, user edits or saves can be replaced by this operation.
        destination=ROOT/"assets/worlds/astra-20260905/playtest"
        if destination.exists():raise ValueError("default Playtest already exists; new revision is preserved at "+result["manifest"])
        stage=destination.with_name(".pending_playtest_"+uuid.uuid4().hex[:8]);stage.parent.mkdir(parents=True,exist_ok=True)
        shutil.copytree(repo_path(result["manifest"]).parent,stage)
        (stage/"manifest.v1.json").rename(stage/"manifest.json")
        stage.rename(destination)
        result["default_manifest"]=(destination/"manifest.json").relative_to(ROOT).as_posix()
    return result


def regenerate_act(source, name, seed):
    """Generate a new Pizza-only layout on the selected act's actual macro-heightfield.

    Existing edits remain preserved in the source revision. This explicit new seed recipe starts
    a new layout, retains authored navigation markers, and never attempts an index-overlay rebase.
    It does not pretend that generic kit buildings are exact reconstructions of named landmarks.
    """
    previous,heights,old_edits=load_world(source)
    if old_edits.get("scene_blocks"):raise ValueError("bake source scene terrain overlays before regeneration")
    gw=generator_module();world=gw.World.__new__(gw.World)
    world.act_id=previous["act_id"];world.seed=int(seed)&0xffffffff
    world.act={"name":previous["act"]+" — regenerated study","ruination":.72 if world.act_id==1 else .48,"half_extent":(len(heights)-1)*previous["terrain"]["spacing"]/2}
    for key in ("assets","placements","wires","roads","craters","town_pts","tactical"):setattr(world,key,[])
    world.markers=copy.deepcopy(previous["markers"]);world.asset_idx={};world.res=len(heights)
    world.spacing=previous["terrain"]["spacing"];world.heights=heights.copy()
    axis=(np.arange(world.res,dtype=np.float32)-(world.res-1)/2)*world.spacing
    world._gx=axis[None,:];world._gz=axis[:,None]
    # Small procedural relief respects the retained macro geography; it is not new survey data.
    phase=(world.seed%1000)*.01
    world.heights+=(.14*np.sin(world._gx/41+phase)*np.cos(world._gz/53-phase)).astype(np.float32)
    nodes=[(m["x"],m["z"]) for m in world.markers]
    world.build_road_network(nodes)
    oldcwd=Path.cwd()
    try:
        os.chdir(ROOT)
        for road in world.roads:world.flatten_road(*road)
        gw.KIT["tree_b"]=bare_tree(gw.KIT["tree_b"]);gw.KIT["tree_c"]=bare_tree(gw.KIT["tree_c"])
        for i,marker in enumerate(world.markers):
            if marker["kind"] in ("Spawn","Goal"):continue
            cx,cz=marker["x"],marker["z"]
            for j in range(16 if world.act_id==1 else 9):
                x=cx+(j%4-1.5)*36;z=cz+(j//4-1.5)*36
                if abs(x)>world.act["half_extent"]-50 or abs(z)>world.act["half_extent"]-50:continue
                # Local bounded pad: no full-map array allocation for every house.
                ix=int(x/world.spacing+(world.res-1)/2);iz=int(z/world.spacing+(world.res-1)/2)
                span=math.ceil(22/world.spacing);x0=max(0,ix-span);x1=min(world.res,ix+span+1);z0=max(0,iz-span);z1=min(world.res,iz+span+1)
                wx=world._gx[:,x0:x1];wz=world._gz[z0:z1,:]
                distance=np.hypot(np.maximum(abs(wx-x)-14,0),np.maximum(abs(wz-z)-14,0));weight=np.clip(1-distance/7,0,1)
                view=world.heights[z0:z1,x0:x1];view+=(world.ground(x,z)-view)*weight
                bid=0xCA00+i*97+j
                if j%4==0:world.modular_outbuilding("warehouse" if j==0 else "shed",x,z,bid,ruination=world.act["ruination"]*.3)
                else:world.building(x,z,bid,world.act["ruination"])
            # Sparse bare roadside growth. The same April guidance applies to both later acts.
            for j in range(32):
                angle=world.rnd(i*100+j,85)*math.tau;radius=95+world.rnd(i*100+j,86)*140
                x=cx+math.cos(angle)*radius;z=cz+math.sin(angle)*radius
                if max(abs(x),abs(z))>world.act["half_extent"]-20 or world.road_dist(x,z)<10:continue
                world.place_ground("tree_b" if j%2 else "tree_c",x,z,angle,.7+world.rnd(j,87)*.4)
                if j%3==0:world.place_ground("stump" if j%2 else "log",x+3,z+2,angle,.7)
                if j%2==0:world.place_ground("stone" if j%3 else "stone3",x-3,z-2,angle,.45)
    finally:os.chdir(oldcwd)
    for marker in world.markers:marker["y"]=world.ground(marker["x"],marker["z"])
    manifest={"schema":"kessel.act","version":1,"act_id":world.act_id,"act":world.act["name"],"seed":world.seed,
        "terrain":dict(previous["terrain"],heights="heights.npy"),"assets":world.assets,"placements":world.placements,"markers":world.markers,"wires":[],
        "authoring":{"mode":"level","campaign_canonical":False,"compatibility_tags":[],"source_manifest":source,
            "geography":"source macro-heightfield and navigation markers retained; new sub-meter relief and modular layout are authored inference",
            "historical_scope":"Act 3 source may be the Halbe prototype; this regeneration does not establish the canonical Berlin finale",
            "material_limit":"source ground drape retained; regenerated building/road layout requires a new surveyed ground-class cook",
            "roads":world.roads,"previous_overlay_preserved_at":source,"not_rebased_ops":len(old_edits.get("ops",[]))}}
    return publish(name,manifest,world.heights,{},source={"manifest":source,"hash":world_hash(source)},recipe={"mode":"regenerate_act","seed":world.seed,"version":1})


def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest="command",required=True)
    p=sub.add_parser("inspect");p.add_argument("manifest")
    p=sub.add_parser("assets");p.add_argument("query",nargs="?",default="")
    p=sub.add_parser("fork");p.add_argument("source");p.add_argument("name")
    p=sub.add_parser("scene");p.add_argument("source");p.add_argument("name");p.add_argument("--act",type=int,choices=[1,2,3],default=2)
    p=sub.add_parser("playtest");p.add_argument("name",nargs="?",default="playtest");p.add_argument("--seed",type=int,default=20260905);p.add_argument("--install-default",action="store_true")
    p=sub.add_parser("regenerate");p.add_argument("source");p.add_argument("name");p.add_argument("--seed",type=int,required=True)
    p=sub.add_parser("apply");p.add_argument("source");p.add_argument("expected_hash");p.add_argument("name");p.add_argument("operations")
    args=parser.parse_args()
    if args.command=="inspect":out=inspect_world(args.manifest)
    elif args.command=="assets":out=asset_search(args.query)
    elif args.command=="fork":out=fork_world(args.source,args.name)
    elif args.command=="scene":out=cook_scene(args.source,args.name,args.act)
    elif args.command=="playtest":out=make_playtest(args.name,args.seed,args.install_default)
    elif args.command=="regenerate":out=regenerate_act(args.source,args.name,args.seed)
    else:out=apply_transaction(args.source,args.expected_hash,args.name,load_json(args.operations))
    print(json.dumps(out,indent=2,allow_nan=False))


if __name__=="__main__": main()
