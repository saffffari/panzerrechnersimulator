#!/usr/bin/env python3
"""Acquire and inspect the explicitly reviewed free Polyfork transport prototype.

The source, publisher license and metadata remain private under assets/vehicles. This is not
the historical FAMO recovery tractor and does not establish towing or armor specifications.
No paid edition, account, API key, remote script, or renderer is used.

  python tools/import_halftrack_prototype.py download assets/vehicles/polyfork_halftrack_20261030
  blender --background --factory-startup --python tools/import_halftrack_prototype.py -- bake <folder>
"""
from __future__ import annotations
import argparse
import hashlib
import html
import json
from pathlib import Path
import re
import struct
import sys
import urllib.request
import zlib

ASSET_ID = "armoured-half-track-3deded"
SOURCE_URL = f"https://polyfork.dev/cdn/{ASSET_ID}.glb"
METADATA_URL = f"https://polyfork.dev/api/assets/{ASSET_ID}"
LICENSE_URL = "https://polyfork.dev/licensing"
PAGE_URL = f"https://polyfork.dev/asset/{ASSET_ID}"
MAX_BYTES = 32 * 1024 * 1024
PIVOTS = ("steer-fl", "steer-fr", "wheel-fl", "wheel-fr", "sprockets", "idlers")
EXPECTED_SOURCE_SHA256 = "c9bf60a49e6127374d96e60ea5201aa25b8b501be203bbfc86e70b1673566c90"

def digest(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()

def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "Panzerrechnersystem-AssetReview/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        blob = response.read(MAX_BYTES + 1)
    if len(blob) > MAX_BYTES:
        raise ValueError("asset response exceeds bounded download size")
    return blob

def write_json(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8", newline="\n") as file:
        json.dump(value, file, indent=2)
        file.write("\n")

def palette_png(palette: list[tuple], tile: int) -> bytes:
    """Encode linear glTF colors as sRGB texels; Blender generated images pack raw values."""
    def encode(v: float) -> int:
        v = max(0.0, min(1.0, v))
        return round(255 * (12.92*v if v <= 0.0031308 else 1.055*v**(1.0/2.4)-0.055))
    row = b"".join(bytes([encode(c[0]), encode(c[1]), encode(c[2]), round(255*c[3])]) * tile for c in palette)
    data = (b"\0" + row) * tile
    def chunk(kind: bytes, value: bytes) -> bytes:
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", len(palette)*tile, tile, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(data)) + chunk(b"IEND", b"")

def inspect(path: Path) -> dict:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import runtime_asset_cook
    blob = path.read_bytes()
    parsed = runtime_asset_cook.parse_glb(blob, str(path))
    if parsed["dependencies"]:
        raise ValueError("prototype GLB must be self contained")
    document = parsed["document"]
    triangles = 0
    for mesh in document.get("meshes", []):
        for primitive in mesh["primitives"]:
            if primitive.get("mode", 4) != 4:
                raise ValueError("expected triangle primitives")
            index = primitive.get("indices", primitive["attributes"]["POSITION"])
            count = document["accessors"][index]["count"]
            if count % 3:
                raise ValueError("invalid triangle primitive")
            triangles += count // 3
    if triangles > 50000 or triangles < 500:
        raise ValueError("unexpected free-prototype triangle count")
    node_names = [n.get("name", "") for n in document.get("nodes", [])]
    missing = [n for n in PIVOTS if n not in node_names]
    if missing:
        raise ValueError(f"source is missing named moving parts: {missing}")
    bounds = parsed["bounds"]
    dimensions = [b-a for a,b in zip(bounds["min"], bounds["max"])]
    if not (2.0 < dimensions[0] < 3.5 and 1.5 < dimensions[1] < 3.5 and 5.0 < dimensions[2] < 7.5):
        raise ValueError(f"unexpected source dimensions: {dimensions}")
    return {"bytes": len(blob), "sha256": digest(blob), "triangles": triangles,
        "node_count": len(node_names), "mesh_count": len(document.get("meshes", [])),
        "materials": len(document.get("materials", [])), "images": len(document.get("images", [])),
        "bounds": bounds, "dimensions_m": dimensions, "moving_nodes": list(PIVOTS),
        "all_node_names": node_names, "external_dependencies": []}

def download(folder: Path) -> None:
    project = Path(__file__).resolve().parent.parent
    allowed = (project / "assets" / "vehicles").resolve()
    folder = folder.resolve()
    if not folder.is_relative_to(allowed) or folder == allowed:
        raise ValueError("destination must be a new folder inside private assets/vehicles")
    if folder.exists():
        raise FileExistsError("source package already exists; no overwrites")
    metadata_blob = fetch(METADATA_URL)
    metadata = json.loads(metadata_blob)
    if metadata.get("id") != ASSET_ID or metadata.get("free") is not True or metadata.get("price_usd") != 0:
        raise ValueError("publisher no longer advertises this exact free model")
    if metadata.get("download", {}).get("glb") != SOURCE_URL or metadata["download"].get("auth") != "none":
        raise ValueError("download identity/authentication changed; review required")
    license_blob = fetch(LICENSE_URL)
    license_text = " ".join(html.unescape(re.sub(r"<[^>]+>", " ", license_blob.decode("utf-8"))).split())
    if "personal and commercial projects" not in license_text or "Your game can ship the models inside it" not in license_text:
        raise ValueError("publisher license changed; manual review required")
    model_blob = fetch(SOURCE_URL)
    if digest(model_blob) != EXPECTED_SOURCE_SHA256:
        raise ValueError("source differs from the reviewed 2026-10-30 file; review required")
    folder.mkdir(parents=True, exist_ok=False)
    source = folder / "source"
    source.mkdir()
    for name, blob in (("publisher_metadata.json", metadata_blob), ("publisher_license.html", license_blob), (f"{ASSET_ID}.glb", model_blob)):
        with (source / name).open("xb") as file:
            file.write(blob)
    report = inspect(source / f"{ASSET_ID}.glb")
    receipt = {"schema": "panzer.reviewed-halftrack-prototype", "version": 1, "retrieved_date": "2026-10-30",
        "publisher": "Polyfork / Lucas Martinic", "asset_id": ASSET_ID, "source_url": SOURCE_URL,
        "page_url": PAGE_URL, "license_url": LICENSE_URL, "commercial_game_use": True,
        "standalone_redistribution": False, "attribution_required": False,
        "intended_use": "generic low-poly personnel/supply transport prototype",
        "historical_famo_model": False, "towing_capacity_verified": False, "engine_integrated": False,
        "physics_note": "Publisher mass is an estimate; do not admit it as a historical vehicle specification.",
        "metadata_sha256": digest(metadata_blob), "license_sha256": digest(license_blob),
        "source_static_inspection": report}
    write_json(folder / "admission.json", receipt)
    print(json.dumps({"downloaded": str(folder), "triangles": report["triangles"], "sha256": report["sha256"], "engine_integrated": False}))

def bake(folder: Path) -> None:
    import bpy
    folder = folder.resolve()
    receipt = json.loads((folder / "admission.json").read_text(encoding="utf-8"))
    source = folder / "source" / f"{ASSET_ID}.glb"
    if digest(source.read_bytes()) != receipt["source_static_inspection"]["sha256"]:
        raise ValueError("reviewed source hash changed")
    output = folder / "baked"
    if output.exists():
        raise FileExistsError("baked directory already exists; no overwrites")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(source))
    # Preserve pivots, object separation, vertex colors and topology. No artistic reconstruction.
    names = set(bpy.data.objects.keys())
    if any(name not in names for name in PIVOTS):
        raise ValueError("Blender import lost authored pivots")
    bpy.context.scene.unit_settings.system = "METRIC"
    bpy.context.scene.unit_settings.scale_length = 1.0
    # Kessel's loader consumes textured albedo, not COLOR_0. Convert this source's
    # face-constant palette to a padded texture; never silently import a white vehicle.
    faces = []
    colors = set()
    for obj in bpy.data.objects:
        if obj.type != "MESH":
            continue
        mesh = obj.data
        attribute = mesh.color_attributes.active_color
        if attribute is None:
            raise ValueError(f"source mesh {obj.name} has no vertex colors")
        for face in mesh.polygons:
            face_colors = []
            for loop in face.loop_indices:
                index = loop if attribute.domain == "CORNER" else mesh.loops[loop].vertex_index
                face_colors.append(tuple(round(float(v), 6) for v in attribute.data[index].color))
            if any(max(abs(a-b) for a,b in zip(face_colors[0], other)) > 0.0001 for other in face_colors[1:]):
                raise ValueError("palette bake requires face-constant colors")
            color = face_colors[0]
            colors.add(color)
            faces.append((obj, face.index, color))
    palette = sorted(colors)
    if not 1 <= len(palette) <= 128:
        raise ValueError("unexpected source palette size")
    tile = 16
    width = len(palette) * tile
    output.mkdir()
    palette_path = output / "source_palette_srgb.png"
    with palette_path.open("xb") as file:
        file.write(palette_png(palette, tile))
    image = bpy.data.images.load(str(palette_path))
    image.colorspace_settings.name = "sRGB"
    image.pack()
    material = bpy.data.materials.new("halftrack_palette_albedo")
    material.use_nodes = True
    shader = material.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Metallic"].default_value = 0.0
    shader.inputs["Roughness"].default_value = 0.85
    texture = material.node_tree.nodes.new("ShaderNodeTexImage")
    texture.image = image
    texture.interpolation = "Closest"
    material.node_tree.links.new(texture.outputs["Color"], shader.inputs["Base Color"])
    for obj in bpy.data.objects:
        if obj.type == "MESH":
            obj.data.materials.clear()
            obj.data.materials.append(material)
            obj.data.uv_layers.new(name="palette_uv")
    for obj, index, color in faces:
        uv = ((palette.index(color) + 0.5) / len(palette), 0.5)
        for loop in obj.data.polygons[index].loop_indices:
            obj.data.uv_layers.active.data[loop].uv = uv
        obj.data.polygons[index].material_index = 0
    for obj in bpy.data.objects:
        if obj.type == "MESH":
            for attribute in list(obj.data.color_attributes):
                obj.data.color_attributes.remove(attribute)
    blend = output / "halftrack_transport_prototype.blend"
    glb = output / "halftrack_transport_prototype.glb"
    bpy.ops.wm.save_as_mainfile(filepath=str(blend))
    bpy.ops.export_scene.gltf(filepath=str(glb), export_format="GLB", export_yup=True,
        export_apply=False, export_materials="EXPORT", export_animations=False)
    report = inspect(glb)
    if report["triangles"] != receipt["source_static_inspection"]["triangles"]:
        raise ValueError("normalization changed source topology count")
    report.update({"blender_version": bpy.app.version_string, "blend_sha256": digest(blend.read_bytes()),
        "source_sha256": receipt["source_static_inspection"]["sha256"],
        "normalization": "Original object hierarchy, authored pivots and metric scale preserved; face-constant source colors encoded into a padded albedo palette for the engine loader",
        "palette_colors": len(palette), "palette_texture_size": [width, tile],
        "palette_encoding": "linear glTF COLOR_0 to sRGB PNG, then standard sRGB albedo sampling",
        "engine_integrated": False, "gpu_visual_verification": False})
    write_json(output / "inspection.json", report)
    print(json.dumps({"baked": str(output), "triangles": report["triangles"], "sha256": report["sha256"]}))

def main() -> None:
    argv = sys.argv[sys.argv.index("--")+1:] if "--" in sys.argv else sys.argv[1:]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("download", "bake", "inspect"))
    parser.add_argument("path", type=Path)
    args = parser.parse_args(argv)
    if args.action == "download": download(args.path)
    elif args.action == "bake": bake(args.path)
    else: print(json.dumps(inspect(args.path), indent=2))

if __name__ == "__main__": main()
