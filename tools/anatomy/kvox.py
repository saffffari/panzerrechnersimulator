"""KVOX: the crew anatomy voxel volume file format (version 1). Reader and writer.

Layout (all integers little-endian):

    offset  size  field
    0       8     magic, ASCII "KVOXANAT"
    8       4     u32 format version (1)
    12      4     u32 header length N in bytes
    16      N     header: UTF-8 JSON object, right-padded with ASCII spaces so 16 + N is a
                  multiple of 8
    16+N    ...   channel payloads, in header["channels"] order, each nx*ny*nz bytes (u8),
                  voxel (x, y, z) at index (y * nz + z) * nx + x: x fastest, then z, then y
                  (the same order as crew_tissue's grid)

The file ends after the last payload. Header keys that the format itself relies on:

    format          "kessel.crew-anatomy-voxels"
    version         1
    dims            [nx, ny, nz]
    voxel_size_m    edge length of one cubic voxel (m)
    origin_m        [x, y, z] of the minimum corner of voxel (0, 0, 0); the centre of voxel
                    (i, j, k) is origin_m + (i + 0.5, j + 0.5, k + 0.5) * voxel_size_m
    channels        [{"name", "type": "u8", "payload_sha256", ...}], the first is "label"

Everything else in the header (frame, rig binding, labels, provenance, edits) is descriptive and
is carried through unchanged by tools that do not understand it.
"""

from __future__ import annotations

import hashlib
import json
import struct
from pathlib import Path

import numpy as np

MAGIC = b"KVOXANAT"
VERSION = 1
FORMAT = "kessel.crew-anatomy-voxels"
LAYOUT = "u8 per voxel; index = (y * nz + z) * nx + x (x fastest, then z, then y)"


def payload_sha256(array: np.ndarray) -> str:
    return hashlib.sha256(np.ascontiguousarray(array, dtype=np.uint8).tobytes()).hexdigest()


def to_grid(flat: np.ndarray, dims: list[int]) -> np.ndarray:
    """Flat payload -> array indexed [y, z, x]."""
    nx, ny, nz = dims
    return flat.reshape(ny, nz, nx)


def encode(header: dict, channels: dict[str, np.ndarray]) -> bytes:
    """Serialise. `channels` maps channel name -> uint8 array shaped [ny, nz, nx]; the header's
    channel list decides their order and receives each payload's sha256."""
    nx, ny, nz = header["dims"]
    header = json.loads(json.dumps(header))  # private copy
    header.setdefault("format", FORMAT)
    header.setdefault("version", VERSION)
    header.setdefault("layout", LAYOUT)
    payloads = []
    for entry in header["channels"]:
        array = np.ascontiguousarray(channels[entry["name"]], dtype=np.uint8)
        if array.shape != (ny, nz, nx):
            raise ValueError(f"channel {entry['name']} has shape {array.shape}, expected {(ny, nz, nx)}")
        entry["type"] = "u8"
        entry["payload_sha256"] = payload_sha256(array)
        payloads.append(array.tobytes())
    text = json.dumps(header, indent=1, sort_keys=False, ensure_ascii=False).encode("utf-8")
    pad = (-(16 + len(text))) % 8
    text += b" " * pad
    return MAGIC + struct.pack("<II", VERSION, len(text)) + text + b"".join(payloads)


def decode(data: bytes, *, verify: bool = True) -> tuple[dict, dict[str, np.ndarray]]:
    if data[:8] != MAGIC:
        raise ValueError("not a KVOX anatomy volume (bad magic)")
    version, length = struct.unpack_from("<II", data, 8)
    if version != VERSION:
        raise ValueError(f"unsupported KVOX version {version}")
    header = json.loads(data[16:16 + length].decode("utf-8"))
    if header.get("format") != FORMAT:
        raise ValueError(f"unexpected format {header.get('format')!r}")
    nx, ny, nz = (int(v) for v in header["dims"])
    count = nx * ny * nz
    offset = 16 + length
    expected = offset + count * len(header["channels"])
    if len(data) != expected:
        raise ValueError(f"KVOX length {len(data)} != {expected} for dims {header['dims']} "
                         f"and {len(header['channels'])} channels")
    channels = {}
    for entry in header["channels"]:
        flat = np.frombuffer(data, dtype=np.uint8, count=count, offset=offset)
        offset += count
        array = to_grid(flat, [nx, ny, nz]).copy()
        digest = entry.get("payload_sha256")
        if verify and digest and payload_sha256(array) != digest:
            raise ValueError(f"channel {entry['name']} payload does not match its recorded sha256")
        channels[entry["name"]] = array
    return header, channels


def write(path: str | Path, header: dict, channels: dict[str, np.ndarray]) -> bytes:
    data = encode(header, channels)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".partial")
    temporary.write_bytes(data)
    temporary.replace(path)
    return data


def read(path: str | Path, *, verify: bool = True) -> tuple[dict, dict[str, np.ndarray]]:
    return decode(Path(path).read_bytes(), verify=verify)


def voxel_centres(header: dict) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Centre coordinates along x, y, z (1-D arrays)."""
    nx, ny, nz = header["dims"]
    h = float(header["voxel_size_m"])
    ox, oy, oz = (float(v) for v in header["origin_m"])
    return (ox + (np.arange(nx) + 0.5) * h, oy + (np.arange(ny) + 0.5) * h, oz + (np.arange(nz) + 0.5) * h)
