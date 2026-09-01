#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile
import zlib


MODULE_PATH = Path(__file__).with_name("extract_tiger_crew_kit.py")
SPEC = importlib.util.spec_from_file_location("extract_tiger_crew_kit", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
extractor = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(extractor)


class TigerCrewExtractionTests(unittest.TestCase):
    def test_duplicate_members_are_preserved_by_reviewed_crc(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "crew.zip"
            first = b"first face"
            second = b"second face"
            rig = b"reviewed rig"
            duplicate_name = "textures/repeated.png"
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_STORED) as target:
                    target.writestr(extractor.SOURCE_FBX, rig)
                    target.writestr(duplicate_name, first)
                    target.writestr(duplicate_name, second)

            original = (
                extractor.ARCHIVE_SHA256,
                extractor.SOURCE_FBX_CRC32,
                extractor.EXPECTED_MEMBER_COUNT,
                extractor.SEMANTIC_DUPLICATES,
            )
            try:
                extractor.ARCHIVE_SHA256 = hashlib.sha256(archive.read_bytes()).hexdigest()
                extractor.SOURCE_FBX_CRC32 = zlib.crc32(rig)
                extractor.EXPECTED_MEMBER_COUNT = 3
                extractor.SEMANTIC_DUPLICATES = {
                    (duplicate_name, zlib.crc32(first)): "textures/reviewed/first.png",
                    (duplicate_name, zlib.crc32(second)): "textures/reviewed/second.png",
                }
                output = root / "out"
                receipt = extractor.extract(archive, output)
            finally:
                (
                    extractor.ARCHIVE_SHA256,
                    extractor.SOURCE_FBX_CRC32,
                    extractor.EXPECTED_MEMBER_COUNT,
                    extractor.SEMANTIC_DUPLICATES,
                ) = original

            self.assertEqual((output / "textures/reviewed/first.png").read_bytes(), first)
            self.assertEqual((output / "textures/reviewed/second.png").read_bytes(), second)
            self.assertEqual(len(receipt["members"]), 3)
            self.assertTrue((output / "extraction.receipt.json").is_file())

    def test_unsafe_archive_member_is_rejected(self) -> None:
        for name in ("../escape", "/absolute", "textures/../escape"):
            with self.subTest(name=name):
                with self.assertRaises(RuntimeError):
                    extractor.safe_member_path(name)


if __name__ == "__main__":
    unittest.main()
