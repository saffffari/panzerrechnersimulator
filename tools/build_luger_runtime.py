#!/usr/bin/env python3
"""Bake the authored P08 GLB into a compact, opaque runtime viewmodel.

The source model carries three 4096px embedded textures (about 41 MiB total),
including a semi-transparent base color.  Geometry and scene transforms are
preserved verbatim; only embedded images and material flags are normalized.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import struct
from pathlib import Path

from PIL import Image


GLB_MAGIC = 0x46546C67
GLB_VERSION = 2
JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942
SOURCE_SHA256 = "b4c7d2cfef91f880fe4cd31bbdd7d75e8aedf1e491cd01e2a30147d6f35f56de"


def parse_glb(path: Path) -> tuple[dict, bytes]:
    payload = path.read_bytes()
    magic, version, declared_length = struct.unpack_from("<III", payload, 0)
    if magic != GLB_MAGIC or version != GLB_VERSION or declared_length != len(payload):
        raise ValueError(f"{path} is not a valid GLB 2.0 file")

    offset = 12
    document = None
    binary = None
    while offset < len(payload):
        chunk_length, chunk_type = struct.unpack_from("<II", payload, offset)
        offset += 8
        chunk = payload[offset : offset + chunk_length]
        offset += chunk_length
        if chunk_type == JSON_CHUNK:
            document = json.loads(chunk.rstrip(b" \x00"))
        elif chunk_type == BIN_CHUNK:
            binary = bytes(chunk)

    if document is None or binary is None:
        raise ValueError(f"{path} does not contain JSON and BIN chunks")
    return document, binary


def encode_png(payload: bytes, size: int, force_opaque: bool) -> bytes:
    with Image.open(io.BytesIO(payload)) as source:
        image = source.convert("RGBA")
        image = image.resize((size, size), Image.Resampling.LANCZOS)
        if force_opaque:
            red, green, blue, _alpha = image.split()
            image = Image.merge("RGBA", (red, green, blue, Image.new("L", image.size, 255)))
        output = io.BytesIO()
        image.save(output, format="PNG", compress_level=9, optimize=False)
        return output.getvalue()


def neutral_normal_png() -> bytes:
    image = Image.new("RGBA", (1, 1), (128, 128, 255, 255))
    output = io.BytesIO()
    image.save(output, format="PNG", compress_level=9, optimize=False)
    return output.getvalue()


def image_view_for_texture(document: dict, texture_info: dict | None) -> int | None:
    if not texture_info:
        return None
    texture = document["textures"][texture_info["index"]]
    image = document["images"][texture["source"]]
    return image.get("bufferView")


def bake(source: Path, output: Path, texture_size: int) -> None:
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        raise ValueError(
            f"unexpected source digest {digest}; expected the reviewed authored P08 {SOURCE_SHA256}"
        )

    document, old_binary = parse_glb(source)
    replacement_views: dict[int, bytes] = {}

    for material in document.get("materials", []):
        pbr = material.setdefault("pbrMetallicRoughness", {})
        base_view = image_view_for_texture(document, pbr.get("baseColorTexture"))
        metal_view = image_view_for_texture(document, pbr.get("metallicRoughnessTexture"))
        normal_view = image_view_for_texture(document, material.get("normalTexture"))

        if base_view is not None:
            view = document["bufferViews"][base_view]
            start = view.get("byteOffset", 0)
            payload = old_binary[start : start + view["byteLength"]]
            replacement_views[base_view] = encode_png(payload, texture_size, True)
        if metal_view is not None:
            view = document["bufferViews"][metal_view]
            start = view.get("byteOffset", 0)
            payload = old_binary[start : start + view["byteLength"]]
            replacement_views[metal_view] = encode_png(payload, texture_size, False)
        if normal_view is not None:
            replacement_views[normal_view] = neutral_normal_png()

        base_factor = list(pbr.get("baseColorFactor", [1.0, 1.0, 1.0, 1.0]))
        while len(base_factor) < 4:
            base_factor.append(1.0)
        base_factor[3] = 1.0
        pbr["baseColorFactor"] = base_factor
        material["alphaMode"] = "OPAQUE"
        material["doubleSided"] = False
        material.pop("alphaCutoff", None)
        material.pop("normalTexture", None)

    new_binary = bytearray()
    for index, view in enumerate(document.get("bufferViews", [])):
        while len(new_binary) % 4:
            new_binary.append(0)
        old_start = view.get("byteOffset", 0)
        payload = replacement_views.get(
            index, old_binary[old_start : old_start + view["byteLength"]]
        )
        view["byteOffset"] = len(new_binary)
        view["byteLength"] = len(payload)
        new_binary.extend(payload)

    document["buffers"][0]["byteLength"] = len(new_binary)
    asset = document.setdefault("asset", {})
    extras = asset.setdefault("extras", {})
    extras["kesselRuntimeBake"] = {
        "source": "assets/flavor/luger/luger.glb",
        "sourceSha256": SOURCE_SHA256,
        "textureSize": texture_size,
        "changes": [
            "base color forced opaque",
            "base and metallic-roughness textures downsampled",
            "normal texture removed",
        ],
    }

    json_payload = json.dumps(document, separators=(",", ":"), sort_keys=True).encode("utf-8")
    json_payload += b" " * ((-len(json_payload)) % 4)
    binary_payload = bytes(new_binary)
    binary_payload += b"\x00" * ((-len(binary_payload)) % 4)
    total_length = 12 + 8 + len(json_payload) + 8 + len(binary_payload)
    glb = bytearray(struct.pack("<III", GLB_MAGIC, GLB_VERSION, total_length))
    glb.extend(struct.pack("<II", len(json_payload), JSON_CHUNK))
    glb.extend(json_payload)
    glb.extend(struct.pack("<II", len(binary_payload), BIN_CHUNK))
    glb.extend(binary_payload)

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(glb)
    print(f"Baked {source} ({source.stat().st_size:,} bytes)")
    print(f"   -> {output} ({output.stat().st_size:,} bytes)")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--texture-size", type=int, default=512)
    args = parser.parse_args()
    if args.texture_size < 64 or args.texture_size > 2048:
        parser.error("--texture-size must be between 64 and 2048")
    bake(args.source, args.output, args.texture_size)


if __name__ == "__main__":
    main()
