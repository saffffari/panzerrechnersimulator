#!/usr/bin/env python3
"""Deterministically unpack the AdrielCZ Tiger crew kit without losing duplicate textures.

Sketchfab's archive contains several different PNGs with the same ZIP member name.  Ordinary
unzip tools overwrite those members and silently discard the alternate faces and PBR channels.
This importer verifies the exact reviewed archive, rejects unsafe paths, preserves every member
under a CRC-qualified filename, and writes a machine-readable receipt for the Blender bake.

Usage::

    python3 tools/extract_tiger_crew_kit.py SOURCE.zip OUTPUT_DIRECTORY
"""

from __future__ import annotations

from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import sys
import zipfile


ARCHIVE_SHA256 = "ecc46d04c5016f07b54838e65b56705486917f2ccda2fce5708900e406183483"
SOURCE_FBX = "source/Free Tiger and Crew FBX.fbx"
SOURCE_FBX_CRC32 = 0xC2A5FADB
EXPECTED_MEMBER_COUNT = 50

# The downloader exported distinct texture channels/faces under three repeated filenames.  These
# CRCs are part of the reviewed archive identity and give each preserved member its truthful name.
SEMANTIC_DUPLICATES = {
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Acessories.png", 0x629D8728): "textures/reviewed/accessories_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Acessories.png", 0x132E2B50): "textures/reviewed/accessories_normal.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Acessories.png", 0xDD31B97B): "textures/reviewed/accessories_metallic.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Acessories.png", 0xF02E0E03): "textures/reviewed/accessories_roughness.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Acessories.png", 0x7F56040E): "textures/reviewed/accessories_height.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0xA0489260): "textures/reviewed/face_01_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x58CD47F6): "textures/reviewed/face_02_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x943275C6): "textures/reviewed/face_03_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x28D81BB6): "textures/reviewed/face_04_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x870FFBEE): "textures/reviewed/face_05_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x78E32FFF): "textures/reviewed/arm_head_metallic.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x22E34603): "textures/reviewed/arm_head_roughness.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x03C440CB): "textures/reviewed/arm_head_height.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_ArmHead_N1.png", 0x34E405AE): "textures/reviewed/arm_head_normal.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Body_Mat_N.png", 0x0918D65D): "textures/reviewed/body_roughness.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Body_Mat_N.png", 0x3CE93893): "textures/reviewed/body_metallic.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Body_Mat_N.png", 0xD67F859C): "textures/reviewed/body_normal.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Body_Mat_N.png", 0x918EC7B7): "textures/reviewed/body_basecolor.png",
    ("textures/Tiger_Kit_Textures_PanzerTruppe_Body_Mat_N.png", 0xD22C1EA7): "textures/reviewed/body_height.png",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def safe_member_path(name: str) -> PurePosixPath:
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or any(part in {"", ".", ".."} for part in path.parts):
        raise RuntimeError(f"unsafe ZIP member path: {name!r}")
    return path


def output_name(member: PurePosixPath, ordinal: int, crc32: int, duplicate: bool) -> PurePosixPath:
    if not duplicate:
        return member
    semantic = SEMANTIC_DUPLICATES.get((member.as_posix(), crc32))
    if semantic is None:
        raise RuntimeError(
            f"reviewed duplicate has no semantic mapping: {member} ordinal={ordinal} CRC={crc32:08x}"
        )
    return PurePosixPath(semantic)


def extract(archive: Path, output_directory: Path) -> dict:
    archive_digest = sha256(archive)
    if archive_digest != ARCHIVE_SHA256:
        raise RuntimeError(
            "unreviewed Tiger crew archive: "
            f"expected SHA-256 {ARCHIVE_SHA256}, got {archive_digest}"
        )

    output_directory.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive) as source:
        members = [info for info in source.infolist() if not info.is_dir()]
        if len(members) != EXPECTED_MEMBER_COUNT:
            raise RuntimeError(
                f"reviewed archive has {EXPECTED_MEMBER_COUNT} files, found {len(members)}"
            )
        names = [safe_member_path(info.filename) for info in members]
        counts = Counter(names)
        expected_duplicate_members = {PurePosixPath(name) for name, _ in SEMANTIC_DUPLICATES}
        actual_duplicate_members = {name for name, count in counts.items() if count > 1}
        if actual_duplicate_members != expected_duplicate_members:
            raise RuntimeError(
                "reviewed duplicate-name set changed: "
                f"expected {sorted(map(str, expected_duplicate_members))}, "
                f"got {sorted(map(str, actual_duplicate_members))}"
            )
        ordinals: defaultdict[PurePosixPath, int] = defaultdict(int)
        receipt_members = []

        for info, member in zip(members, names):
            ordinal = ordinals[member]
            ordinals[member] += 1
            destination_relative = output_name(member, ordinal, info.CRC, counts[member] > 1)
            destination = output_directory.joinpath(*destination_relative.parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            with source.open(info) as incoming, destination.open("wb") as outgoing:
                shutil.copyfileobj(incoming, outgoing)
            receipt_members.append(
                {
                    "archive_name": info.filename,
                    "duplicate_ordinal": ordinal,
                    "crc32": f"{info.CRC:08x}",
                    "bytes": info.file_size,
                    "output": destination_relative.as_posix(),
                    "sha256": sha256(destination),
                }
            )

    fbx_records = [record for record in receipt_members if record["archive_name"] == SOURCE_FBX]
    if len(fbx_records) != 1 or int(fbx_records[0]["crc32"], 16) != SOURCE_FBX_CRC32:
        raise RuntimeError("reviewed rigged FBX is missing or changed")

    receipt = {
        "schema": "kessel.tiger-crew-source-extraction",
        "version": 1,
        "asset": {
            "title": "Free Tiger And Crew Kit",
            "author": "AdrielCZ",
            "source_url": "https://sketchfab.com/3d-models/free-tiger-and-crew-kit-6290669ab2cb49599f1606507626af39",
            "license": "CC-BY-4.0",
        },
        "archive": {
            "filename": archive.name,
            "sha256": archive_digest,
            "bytes": archive.stat().st_size,
        },
        "members": receipt_members,
    }
    receipt_path = output_directory / "extraction.receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return receipt


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: extract_tiger_crew_kit.py SOURCE.zip OUTPUT_DIRECTORY")
    archive = Path(sys.argv[1]).expanduser().resolve()
    output = Path(sys.argv[2]).expanduser().resolve()
    if not archive.is_file():
        raise SystemExit(f"archive does not exist: {archive}")
    receipt = extract(archive, output)
    print(
        f"extracted {len(receipt['members'])} reviewed members to {output} "
        "without duplicate-name loss"
    )


if __name__ == "__main__":
    main()
