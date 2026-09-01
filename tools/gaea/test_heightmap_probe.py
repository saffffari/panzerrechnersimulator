#!/usr/bin/env python3

from __future__ import annotations

import binascii
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zlib


MODULE_PATH = Path(__file__).with_name("heightmap_probe.py")
SPEC = importlib.util.spec_from_file_location("heightmap_probe", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", binascii.crc32(kind + payload) & 0xFFFF_FFFF)


def write_png(path: Path, width: int, height: int, *, bit_depth: int, color_type: int, rows: list[bytes]) -> None:
    assert len(rows) == height
    ihdr = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, 0)
    scanlines = b"".join(b"\x00" + row for row in rows)
    path.write_bytes(
        probe.PNG_SIGNATURE
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(scanlines))
        + png_chunk(b"IEND", b"")
    )


def write_heightmap(path: Path, values: list[list[int]], *, bit_depth: int = 16) -> None:
    if bit_depth == 16:
        rows = [b"".join(struct.pack(">H", value) for value in row) for row in values]
    else:
        rows = [bytes(value >> 8 for value in row) for row in values]
    write_png(path, len(values[0]), len(values), bit_depth=bit_depth, color_type=0, rows=rows)


def write_drape(path: Path, resolution: int) -> None:
    rows = [bytes([32 + row, 64, 96] * resolution) for row in range(resolution)]
    write_png(path, resolution, resolution, bit_depth=8, color_type=2, rows=rows)


def write_sidecar(path: Path, **updates: object) -> None:
    value: dict[str, object] = {
        "schema": probe.SCHEMA,
        "version": probe.VERSION,
        "orientation": probe.ORIENTATION,
        "meters_per_side": 4.0,
        "elev_min": -10.0,
        "elev_max": 90.0,
        "drape": "ground.png",
    }
    value.update(updates)
    path.write_text(json.dumps(value), encoding="utf-8")


def read_npy_f32(path: Path) -> tuple[int, list[float]]:
    payload = path.read_bytes()
    assert payload[:8] == b"\x93NUMPY\x01\x00"
    header_len = struct.unpack_from("<H", payload, 8)[0]
    header = payload[10 : 10 + header_len].decode("ascii")
    shape = header.split("'shape': (", 1)[1].split(")", 1)[0]
    resolution = int(shape.split(",", 1)[0])
    values = list(struct.unpack_from(f"<{resolution * resolution}f", payload, 10 + header_len))
    return resolution, values


class HeightmapProbeTests(unittest.TestCase):
    def make_probe(self, root: Path, *, bit_depth: int = 16) -> Path:
        source = root / "source"
        source.mkdir()
        write_heightmap(
            source / "heightmap.png",
            [
                [0, 8192, 16384],
                [24576, 32768, 40960],
                [49152, 57344, 65535],
            ],
            bit_depth=bit_depth,
        )
        write_drape(source / "ground.png", 3)
        write_sidecar(source / "heightmap.json")
        return source / "heightmap.json"

    def test_checked_in_schema_and_example_match_runtime_contract(self) -> None:
        schema = json.loads(probe.SCHEMA_PATH.read_text(encoding="utf-8"))
        example = json.loads(probe.EXAMPLE_PATH.read_text(encoding="utf-8"))
        self.assertEqual(schema["properties"]["schema"]["const"], probe.SCHEMA)
        self.assertEqual(schema["properties"]["version"]["const"], probe.VERSION)
        self.assertEqual(schema["properties"]["orientation"]["const"], probe.ORIENTATION)
        self.assertEqual(set(schema["required"]), probe.SIDECAR_FIELDS - {"drape"})
        self.assertEqual(example["schema"], probe.SCHEMA)
        self.assertEqual(example["version"], probe.VERSION)
        self.assertEqual(example["orientation"], probe.ORIENTATION)

    def test_validate_accepts_exact_16_bit_square_north_up_contract(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            sidecar = self.make_probe(Path(temp))
            result = probe.validate_probe(sidecar)
            self.assertEqual(result["resolution"], 3)
            self.assertEqual(result["spacing"], 2.0)
            self.assertEqual(result["heightmap"]["bit_depth"], 16)
            self.assertEqual(result["heightmap"]["color_type"], 0)
            self.assertIsNotNone(result["heightmap"]["decoded"])

    def test_validate_rejects_8_bit_heightmaps_and_unsafe_sidecars(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            sidecar = self.make_probe(root, bit_depth=8)
            with self.assertRaisesRegex(probe.ProbeError, "native 16-bit greyscale"):
                probe.validate_probe(sidecar)
            write_sidecar(sidecar, drape="../escape.png")
            with self.assertRaisesRegex(probe.ProbeError, "may not escape"):
                probe.validate_probe(sidecar)
            write_sidecar(sidecar, elev_min=10.0, elev_max=10.0)
            with self.assertRaisesRegex(probe.ProbeError, "greater than"):
                probe.validate_probe(sidecar)

    def test_cook_emits_row_major_f32_payload_and_manifest_fragment_without_activation(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "Cargo.toml").write_text("[package]\nname='fixture'\nversion='0.0.0'\n", encoding="utf-8")
            sidecar = self.make_probe(root)
            act_dir = root / "assets/worldgen/act1"
            act_dir.mkdir(parents=True)
            manifest = act_dir / "manifest.v1.json"
            manifest.write_text(
                json.dumps(
                    {
                        "schema": "kessel.act",
                        "version": 1,
                        "terrain": {
                            "res": 2,
                            "spacing": 1.0,
                            "heights": "heights.npy",
                            "texture": "assets/old.png",
                        },
                    }
                ),
                encoding="utf-8",
            )
            original_manifest = manifest.read_bytes()
            output = act_dir / "gaea-probe"
            receipt = probe.cook_probe(sidecar, manifest, output, repo_root=root)

            self.assertEqual(manifest.read_bytes(), original_manifest, "cook must not activate itself")
            self.assertFalse(receipt["activated"])
            self.assertEqual(
                receipt["manifest_terrain"],
                {
                    "res": 3,
                    "spacing": 2.0,
                    "heights": "gaea-probe/heights.npy",
                    "texture": "assets/worldgen/act1/gaea-probe/ground.png",
                },
            )
            self.assertEqual(
                json.loads((output / "terrain.fragment.json").read_text(encoding="utf-8")),
                receipt["manifest_terrain"],
            )
            resolution, heights = read_npy_f32(output / "heights.npy")
            self.assertEqual(resolution, 3)
            self.assertAlmostEqual(heights[4], 0.0, places=6, msg="centre source sample is runtime y=0")
            source_samples = [0, 8192, 16384, 24576, 32768, 40960, 49152, 57344, 65535]
            expected = [(sample - 32768) / 65535.0 * 100.0 for sample in source_samples]
            for actual, wanted in zip(heights, expected):
                self.assertAlmostEqual(actual, wanted, places=5, msg="PNG rows must remain +Z row-major")
            self.assertEqual(len(heights), 9)
            with self.assertRaisesRegex(probe.ProbeError, "refusing to overwrite"):
                probe.cook_probe(sidecar, manifest, output, repo_root=root)


if __name__ == "__main__":
    unittest.main()
