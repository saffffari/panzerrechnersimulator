#!/usr/bin/env python3
"""Validate and stage a Gaea 2 heightfield for Kessel without activating it.

The tool deliberately never edits an act manifest. ``validate`` proves the sidecar and PNG are
the exact format the engine expects. ``cook`` writes a new, isolated row-major float32 ``.npy``
plus the precise ``terrain`` fragment consumed by ``manifest.v1.json``; a human or a later
reviewed change must explicitly activate that fragment.
"""

from __future__ import annotations

import argparse
import array
import binascii
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import shutil
import struct
import sys
import tempfile
from typing import Any
import zlib


SCHEMA = "kessel.terrain-heightmap"
VERSION = 1
ORIENTATION = "north-up"
COOK_SCHEMA = "kessel.terrain-heightmap-cook"
MAX_DIMENSION = 16_384
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
SIDECAR_FIELDS = {
    "schema",
    "version",
    "orientation",
    "meters_per_side",
    "elev_min",
    "elev_max",
    "drape",
}
REPO_ROOT = Path(__file__).resolve().parents[2]
SCHEMA_PATH = REPO_ROOT / "catalog/schemas/terrain-heightmap.v1.schema.json"
EXAMPLE_PATH = REPO_ROOT / "catalog/examples/terrain-heightmap.v1.example.json"


class ProbeError(ValueError):
    """A deterministic, user-actionable contract failure."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ProbeError(message)


def _safe_relative(raw: Any, field: str) -> PurePosixPath:
    _require(isinstance(raw, str) and bool(raw), f"{field} must be a non-empty path string")
    _require("\\" not in raw and ":" not in raw and "\x00" not in raw, f"{field} must use a portable relative path")
    path = PurePosixPath(raw)
    _require(not path.is_absolute(), f"{field} must be relative")
    _require(all(part not in ("", ".", "..") for part in path.parts), f"{field} may not escape its root")
    return path


def _number(value: Any, field: str) -> float:
    _require(isinstance(value, (int, float)) and not isinstance(value, bool), f"{field} must be a number")
    number = float(value)
    _require(math.isfinite(number), f"{field} must be finite")
    return number


def load_sidecar(path: Path) -> dict[str, Any]:
    path = path.resolve()
    _require(path.name == "heightmap.json", "sidecar must be named heightmap.json")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ProbeError(f"cannot read sidecar {path}: {error}") from error
    _require(isinstance(value, dict), "heightmap.json root must be an object")
    unknown = sorted(set(value) - SIDECAR_FIELDS)
    _require(not unknown, f"heightmap.json contains unsupported fields: {', '.join(unknown)}")
    required = SIDECAR_FIELDS - {"drape"}
    missing = sorted(required - set(value))
    _require(not missing, f"heightmap.json is missing required fields: {', '.join(missing)}")
    _require(value["schema"] == SCHEMA, f"schema must be {SCHEMA!r}")
    _require(value["version"] == VERSION and not isinstance(value["version"], bool), f"version must be {VERSION}")
    _require(value["orientation"] == ORIENTATION, f"orientation must be {ORIENTATION!r}")
    meters_per_side = _number(value["meters_per_side"], "meters_per_side")
    elev_min = _number(value["elev_min"], "elev_min")
    elev_max = _number(value["elev_max"], "elev_max")
    _require(meters_per_side > 0.0, "meters_per_side must be positive")
    _require(elev_max > elev_min, "elev_max must be greater than elev_min")
    drape = value.get("drape")
    if drape is not None:
        drape_path = _safe_relative(drape, "drape")
        _require(drape_path.suffix.lower() == ".png", "drape must name a PNG")
    return {
        "schema": SCHEMA,
        "version": VERSION,
        "orientation": ORIENTATION,
        "meters_per_side": meters_per_side,
        "elev_min": elev_min,
        "elev_max": elev_max,
        **({"drape": drape} if drape is not None else {}),
    }


def _png_channels(color_type: int) -> int:
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color_type)
    _require(channels is not None, f"unsupported PNG color type {color_type}")
    return channels


def _valid_png_depth(color_type: int, bit_depth: int) -> bool:
    return bit_depth in {
        0: {1, 2, 4, 8, 16},
        2: {8, 16},
        3: {1, 2, 4, 8},
        4: {8, 16},
        6: {8, 16},
    }.get(color_type, set())


def _parse_png(path: Path, *, decode_height: bool) -> dict[str, Any]:
    try:
        payload = path.read_bytes()
    except OSError as error:
        raise ProbeError(f"cannot read PNG {path}: {error}") from error
    _require(payload.startswith(PNG_SIGNATURE), f"{path} is not a PNG")
    offset = len(PNG_SIGNATURE)
    ihdr: tuple[int, int, int, int, int, int, int] | None = None
    idat: list[bytes] = []
    saw_iend = False
    chunk_index = 0
    while offset < len(payload):
        _require(offset + 12 <= len(payload), f"{path} has a truncated PNG chunk")
        length = struct.unpack_from(">I", payload, offset)[0]
        kind = payload[offset + 4 : offset + 8]
        end = offset + 12 + length
        _require(end <= len(payload), f"{path} has a truncated {kind!r} chunk")
        data = payload[offset + 8 : offset + 8 + length]
        expected_crc = struct.unpack_from(">I", payload, offset + 8 + length)[0]
        actual_crc = binascii.crc32(kind + data) & 0xFFFF_FFFF
        _require(actual_crc == expected_crc, f"{path} has a bad {kind.decode('ascii', 'replace')} CRC")
        if chunk_index == 0:
            _require(kind == b"IHDR", f"{path} does not begin with IHDR")
        if kind == b"IHDR":
            _require(ihdr is None and length == 13, f"{path} has an invalid IHDR")
            ihdr = struct.unpack(">IIBBBBB", data)
        elif kind == b"IDAT":
            idat.append(data)
        elif kind == b"IEND":
            _require(length == 0, f"{path} has an invalid IEND")
            saw_iend = True
            offset = end
            break
        offset = end
        chunk_index += 1
    _require(ihdr is not None and saw_iend and offset == len(payload), f"{path} is incomplete or has trailing data")
    _require(bool(idat), f"{path} has no IDAT payload")
    width, height, bit_depth, color_type, compression, filter_method, interlace = ihdr
    _require(2 <= width <= MAX_DIMENSION and 2 <= height <= MAX_DIMENSION, f"{path} dimensions must be 2..{MAX_DIMENSION}")
    _require(compression == 0 and filter_method == 0, f"{path} uses unsupported PNG compression/filter method")
    _require(interlace == 0, f"{path} must be non-interlaced")
    _require(_valid_png_depth(color_type, bit_depth), f"{path} has invalid bit depth {bit_depth} for color type {color_type}")
    channels = _png_channels(color_type)
    row_bytes = (width * channels * bit_depth + 7) // 8
    expected_size = height * (row_bytes + 1)
    _require(expected_size <= 512 * 1024 * 1024, f"{path} decoded payload exceeds the 512 MiB probe limit")
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(b"".join(idat), expected_size + 1)
    except zlib.error as error:
        raise ProbeError(f"{path} has an invalid compressed payload: {error}") from error
    _require(
        decoder.eof and not decoder.unconsumed_tail and not decoder.unused_data,
        f"{path} has an incomplete or overlong compressed payload",
    )
    _require(len(raw) == expected_size, f"{path} decoded payload is {len(raw)} bytes; expected {expected_size}")
    for row_index in range(height):
        _require(raw[row_index * (row_bytes + 1)] <= 4, f"{path} row {row_index} has an invalid PNG filter")

    decoded: bytearray | None = None
    if decode_height:
        _require(bit_depth == 16 and color_type == 0, f"{path} must be native 16-bit greyscale")
        bytes_per_pixel = 2
        decoded = bytearray(width * height * bytes_per_pixel)
        previous = bytearray(row_bytes)
        for row_index in range(height):
            source_offset = row_index * (row_bytes + 1)
            filter_type = raw[source_offset]
            _require(filter_type <= 4, f"{path} row {row_index} uses invalid PNG filter {filter_type}")
            scanline = raw[source_offset + 1 : source_offset + 1 + row_bytes]
            current = bytearray(row_bytes)
            for index, byte in enumerate(scanline):
                left = current[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
                above = previous[index]
                upper_left = previous[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
                if filter_type == 0:
                    predictor = 0
                elif filter_type == 1:
                    predictor = left
                elif filter_type == 2:
                    predictor = above
                elif filter_type == 3:
                    predictor = (left + above) // 2
                else:
                    p = left + above - upper_left
                    pa = abs(p - left)
                    pb = abs(p - above)
                    pc = abs(p - upper_left)
                    predictor = left if pa <= pb and pa <= pc else above if pb <= pc else upper_left
                current[index] = (byte + predictor) & 0xFF
            target_offset = row_index * row_bytes
            decoded[target_offset : target_offset + row_bytes] = current
            previous = current
    return {
        "path": path,
        "width": width,
        "height": height,
        "bit_depth": bit_depth,
        "color_type": color_type,
        "sha256": hashlib.sha256(payload).hexdigest(),
        "decoded": decoded,
    }


def validate_probe(sidecar_path: Path) -> dict[str, Any]:
    sidecar_path = sidecar_path.resolve()
    meta = load_sidecar(sidecar_path)
    heightmap_path = sidecar_path.with_name("heightmap.png")
    _require(heightmap_path.is_file() and not heightmap_path.is_symlink(), "heightmap.png must be a regular sibling file")
    heightmap = _parse_png(heightmap_path, decode_height=True)
    _require(heightmap["width"] == heightmap["height"], "heightmap.png must be square")
    drape: dict[str, Any] | None = None
    if "drape" in meta:
        drape_path = sidecar_path.parent.joinpath(*PurePosixPath(meta["drape"]).parts)
        _require(drape_path.is_file() and not drape_path.is_symlink(), "drape must be a regular file inside the delivery")
        try:
            drape_path.resolve().relative_to(sidecar_path.parent)
        except ValueError as error:
            raise ProbeError("drape may not escape the sidecar directory") from error
        drape = _parse_png(drape_path, decode_height=False)
        _require(drape["width"] == drape["height"], "georeferenced drape must be square")
    resolution = int(heightmap["width"])
    spacing = meta["meters_per_side"] / (resolution - 1)
    return {
        "metadata": meta,
        "heightmap": heightmap,
        "drape": drape,
        "resolution": resolution,
        "spacing": spacing,
    }


def _npy_header(resolution: int) -> bytes:
    text = f"{{'descr': '<f4', 'fortran_order': False, 'shape': ({resolution}, {resolution}), }}"
    raw = text.encode("ascii")
    padding = (16 - ((10 + len(raw) + 1) % 16)) % 16
    header = raw + b" " * padding + b"\n"
    _require(len(header) <= 0xFFFF, "NPY v1 header is too large")
    return b"\x93NUMPY\x01\x00" + struct.pack("<H", len(header)) + header


def _write_heights_npy(path: Path, probe: dict[str, Any]) -> tuple[float, float, float]:
    resolution = probe["resolution"]
    meta = probe["metadata"]
    pixels: bytearray = probe["heightmap"]["decoded"]
    row_bytes = resolution * 2
    centre_offset = (resolution // 2) * row_bytes + (resolution // 2) * 2
    centre_sample = struct.unpack_from(">H", pixels, centre_offset)[0]
    relief = meta["elev_max"] - meta["elev_min"]
    centre_elevation = meta["elev_min"] + centre_sample / 65535.0 * relief
    min_height = math.inf
    max_height = -math.inf
    with path.open("wb") as output:
        output.write(_npy_header(resolution))
        for row_index in range(resolution):
            start = row_index * row_bytes
            samples = array.array("H")
            samples.frombytes(pixels[start : start + row_bytes])
            if sys.byteorder == "little":
                samples.byteswap()
            heights = array.array(
                "f",
                (
                    meta["elev_min"] + sample / 65535.0 * relief - centre_elevation
                    for sample in samples
                ),
            )
            _require(heights.itemsize == 4, "this Python build does not provide 32-bit float arrays")
            if heights:
                min_height = min(min_height, min(heights))
                max_height = max(max_height, max(heights))
            if sys.byteorder != "little":
                heights.byteswap()
            output.write(heights.tobytes())
    return centre_elevation, min_height, max_height


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _project_logical(path: Path, repo_root: Path) -> str:
    try:
        relative = path.resolve().relative_to(repo_root.resolve())
    except ValueError as error:
        raise ProbeError(f"cooked runtime resource must remain inside the repository: {path}") from error
    logical = PurePosixPath(relative.as_posix())
    _require(logical.parts and logical.parts[0] == "assets", "cooked runtime resource must live below assets/")
    return logical.as_posix()


def cook_probe(
    sidecar_path: Path,
    manifest_path: Path,
    output_dir: Path,
    *,
    texture_logical: str | None = None,
    repo_root: Path = REPO_ROOT,
) -> dict[str, Any]:
    sidecar_path = sidecar_path.resolve()
    probe = validate_probe(sidecar_path)
    repo_root = repo_root.resolve()
    manifest_path = manifest_path.resolve()
    output_dir = output_dir.resolve()
    _require(manifest_path.name == "manifest.v1.json", "--manifest must name manifest.v1.json")
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ProbeError(f"cannot read act manifest {manifest_path}: {error}") from error
    _require(
        isinstance(manifest, dict)
        and manifest.get("schema") == "kessel.act"
        and manifest.get("version") == 1
        and not isinstance(manifest.get("version"), bool),
        "--manifest must be a kessel.act/v1 manifest",
    )
    _require(isinstance(manifest.get("terrain"), dict), "act manifest has no terrain object")
    try:
        relative_output = output_dir.relative_to(manifest_path.parent)
    except ValueError as error:
        raise ProbeError("--output-dir must be inside the act manifest directory") from error
    _require(relative_output.parts and all(part not in ("", ".", "..") for part in relative_output.parts),
             "--output-dir must be a new child directory below the act manifest")
    _require(not output_dir.exists(), f"refusing to overwrite existing output directory: {output_dir}")
    _project_logical(output_dir, repo_root)

    source_drape = probe["drape"]
    if source_drape is not None:
        _require(texture_logical is None, "--texture-logical is only valid when the sidecar omits drape")
        cooked_texture_name = "ground.png"
        cooked_texture_logical = _project_logical(output_dir / cooked_texture_name, repo_root)
    else:
        _require(texture_logical is not None, "cook needs either sidecar.drape or --texture-logical")
        texture_path = _safe_relative(texture_logical, "--texture-logical")
        cooked_texture_logical = texture_path.as_posix()
        _require((repo_root / cooked_texture_logical).is_file(), f"--texture-logical does not exist: {cooked_texture_logical}")
        cooked_texture_name = None

    parent = output_dir.parent
    parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=f".{output_dir.name}.staging-", dir=parent))
    try:
        heights_path = stage / "heights.npy"
        centre_elevation, min_height, max_height = _write_heights_npy(heights_path, probe)
        if source_drape is not None:
            shutil.copyfile(source_drape["path"], stage / cooked_texture_name)
        shutil.copyfile(sidecar_path, stage / "heightmap.source.json")
        heights_ref = (relative_output / "heights.npy").as_posix()
        terrain_fragment = {
            "res": probe["resolution"],
            "spacing": probe["spacing"],
            "heights": heights_ref,
            "texture": cooked_texture_logical,
        }
        (stage / "terrain.fragment.json").write_text(
            json.dumps(terrain_fragment, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        receipt = {
            "schema": COOK_SCHEMA,
            "version": 1,
            "activated": False,
            "source": {
                "sidecar": sidecar_path.name,
                "sidecar_sha256": _sha256(sidecar_path),
                "heightmap": "heightmap.png",
                "heightmap_sha256": probe["heightmap"]["sha256"],
                **(
                    {
                        "drape": probe["metadata"]["drape"],
                        "drape_sha256": source_drape["sha256"],
                    }
                    if source_drape is not None
                    else {}
                ),
            },
            "decoded": {
                "resolution": probe["resolution"],
                "spacing_m": probe["spacing"],
                "encoded_elevation_range_m": [
                    probe["metadata"]["elev_min"],
                    probe["metadata"]["elev_max"],
                ],
                "centre_source_elevation_m": centre_elevation,
                "runtime_min_height_m": min_height,
                "runtime_max_height_m": max_height,
                "orientation": ORIENTATION,
                "height_datum": "centre-sample-zero",
            },
            "output": {
                "heights": heights_ref,
                "heights_sha256": _sha256(heights_path),
                "texture": cooked_texture_logical,
            },
            "manifest": _project_logical(manifest_path, repo_root),
            "manifest_terrain": terrain_fragment,
        }
        (stage / "cook.receipt.json").write_text(
            json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        os.replace(stage, output_dir)
    except Exception:
        shutil.rmtree(stage, ignore_errors=True)
        raise
    return receipt


def public_summary(probe: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": probe["metadata"]["schema"],
        "version": probe["metadata"]["version"],
        "orientation": probe["metadata"]["orientation"],
        "resolution": probe["resolution"],
        "meters_per_side": probe["metadata"]["meters_per_side"],
        "spacing_m": probe["spacing"],
        "elevation_range_m": [probe["metadata"]["elev_min"], probe["metadata"]["elev_max"]],
        "heightmap_sha256": probe["heightmap"]["sha256"],
        "drape": probe["metadata"].get("drape"),
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate", help="validate sidecar + PNGs without writing")
    validate_parser.add_argument("sidecar", type=Path)
    validate_parser.add_argument("--json", action="store_true", help="emit machine-readable summary")
    cook_parser = subparsers.add_parser("cook", help="stage NPY + manifest fragment without activation")
    cook_parser.add_argument("sidecar", type=Path)
    cook_parser.add_argument("--manifest", type=Path, required=True)
    cook_parser.add_argument("--output-dir", type=Path, required=True)
    cook_parser.add_argument("--texture-logical", help="existing repo-relative texture when sidecar omits drape")
    cook_parser.add_argument("--json", action="store_true", help="emit machine-readable receipt")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "validate":
            summary = public_summary(validate_probe(args.sidecar))
            if args.json:
                print(json.dumps(summary, indent=2, sort_keys=True))
            else:
                print(
                    f"VALID {summary['resolution']}x{summary['resolution']} north-up heightfield; "
                    f"{summary['meters_per_side']:.3f} m/side, {summary['spacing_m']:.6f} m/sample"
                )
        else:
            receipt = cook_probe(
                args.sidecar,
                args.manifest,
                args.output_dir,
                texture_logical=args.texture_logical,
            )
            if args.json:
                print(json.dumps(receipt, indent=2, sort_keys=True))
            else:
                print(f"COOKED (not activated) {args.output_dir}")
                print(json.dumps(receipt["manifest_terrain"], indent=2, sort_keys=True))
        return 0
    except ProbeError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
