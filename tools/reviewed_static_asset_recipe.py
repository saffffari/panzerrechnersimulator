#!/usr/bin/env python3
"""Pure-Python authority shared by the reviewed Enigma and bridge bakes.

This module deliberately has no Blender dependency.  The headless bake scripts, their unit tests,
and the post-export GLB verifier all consume the same immutable source hashes and coordinate
conventions, so a similarly named download cannot silently replace the reviewed CC BY source.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence


RECIPE_VERSION = 1
ATLAS_SIZE_PX = 1024
ATLAS_GRID_SIZE = 2
ATLAS_TILE_SIZE_PX = ATLAS_SIZE_PX // ATLAS_GRID_SIZE


@dataclass(frozen=True)
class ReviewedAsset:
    key: str
    title: str
    creator: str
    sketchfab_uid: str
    source_url: str
    archive_filename: str
    archive_bytes: int
    archive_sha256: str
    source_filename: str
    source_sha256: str
    texture_sha256: Mapping[str, str]
    baker_filename: str
    output_filename: str
    report_filename: str


ENIGMA = ReviewedAsset(
    key="enigma",
    title="Enigma Machine",
    creator="Ashish0096",
    sketchfab_uid="30b85c037a164174a109ad6f002b9c47",
    source_url=(
        "https://sketchfab.com/3d-models/"
        "enigma-machine-30b85c037a164174a109ad6f002b9c47"
    ),
    archive_filename="enigma-machine.zip",
    archive_bytes=128_097_287,
    archive_sha256="f8a692201e7cc91f6990a1bce37cbf093ee7af005de419402c8122ff7cff88dd",
    source_filename="low.fbx",
    source_sha256="bf1a983d7cc73ff21be771c2d2101bd575cab403a4b2497c44b743b467c2c5f0",
    texture_sha256={
        "low_box_Base_color.png": (
            "3d5d953340200b993b004b39ae2af56cf82c98f8c19ba791279a52a0018d4bb0"
        ),
        "low_box_Metallic.png": (
            "b8e10bd717033a4530b53a53322d66501b2e79158a4b63fb29bbd4c09b031881"
        ),
        "low_box_Normal_DirectX.png": (
            "3b5e12f2b79a58e536fb26f7c4697e0de924fb6fc6a8928bddaf2710b2afc9cb"
        ),
        "low_box_Mixed_AO.png": (
            "10354f06cc21709e89e9c969b98a25290b29aa4c4144f55c93fe90591630f776"
        ),
        "low_box_Roughness.png": (
            "d33502ea2ba97e1a6408344b65477a1120bc9ce34b733c264b07d2f751f8ab43"
        ),
        "low_mec_Base_color.png": (
            "7135b2390d6b22771621bd85d694fe04b06cd1f42cec1797b4c7858434825b1c"
        ),
        "low_mec_Metallic.png": (
            "4108348a1541638b9bacede8aea2606371bb7e9a4437747e4e415dc2d67cb569"
        ),
        "low_mec_Normal_DirectX.png": (
            "2c9f376790df6fbbb0386b407bccdf66e785f55e37ff03a6d2c9250fb7c66e3e"
        ),
        "low_mec_Mixed_AO.png": (
            "3ba8b38ca98a5f85e5e296f0266b85a48f790c012730c25cba0057ecaafee4d4"
        ),
        "low_mec_Roughness.png": (
            "9a9035e6cab3bff7b2666c46384eb080c734ccace2eb43061fa4f3e34ee09da7"
        ),
    },
    baker_filename="bake_enigma_machine.py",
    output_filename="enigma_machine_runtime.glb",
    report_filename="enigma_machine_runtime.report.json",
)


BRIDGE = ReviewedAsset(
    key="bridge",
    title="Stylised Low Poly City Metal Bridge",
    creator="remidoes3d",
    sketchfab_uid="674aee9cfd7b478b93c2f3e2d7026a0c",
    source_url=(
        "https://sketchfab.com/3d-models/"
        "stylised-low-poly-city-metal-bridge-674aee9cfd7b478b93c2f3e2d7026a0c"
    ),
    archive_filename="stylised-low-poly-city-metal-bridge.zip",
    archive_bytes=22_783_315,
    archive_sha256="672588061b44fc71fe65fbdc79ebcb8a4fc29933183176ddf02552cef641a1db",
    source_filename="Pont.fbx",
    source_sha256="f9b016b4c9c195b5418b404a07b163f6495b2e4642967ebb857cf9152a236432",
    texture_sha256={
        "Pont_Pont1_BaseColor.png": (
            "e8480d9b37a72543dab7728a2161ac20ad60820ccbdea5b6a5dde4b2e41e4a1c"
        ),
        "Pont_Pont2_BaseColor.png": (
            "7e111bd7f843e5381ef22ca1818ba5dffb43d8765b75173cfa4e9130a60fa04f"
        ),
        "Pont_Pont3_BaseColor.png": (
            "398565e22f5a9ba98dd5ffd7bf4c1f57c50ec5499276f8e9894b8f9a7f862774"
        ),
        "Pont_Pont4_BaseColor.png": (
            "332556861ba1784b7197df8a65fbbae6997ff01d2d25ebda7f78f3a0c206628e"
        ),
        "internal_ground_ao_texture.jpeg": (
            "9e48236e38069c8dfcb19758fc1261898dc2c609b42fd22c58fe6669acb5040a"
        ),
    },
    baker_filename="bake_city_metal_bridge.py",
    output_filename="city_metal_bridge_runtime.glb",
    report_filename="city_metal_bridge_runtime.report.json",
)


ASSETS = {asset.key: asset for asset in (ENIGMA, BRIDGE)}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def file_record(path: Path, *, logical_path: str) -> dict[str, Any]:
    """Return an auditable record for one repository tool file."""

    path = path.resolve()
    if not path.is_file():
        raise ValueError(f"tool receipt target is not a file: {path}")
    if Path(logical_path).is_absolute() or ".." in Path(logical_path).parts:
        raise ValueError(f"unsafe tool receipt path: {logical_path!r}")
    return {
        "path": logical_path,
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def source_archive_record(asset: ReviewedAsset) -> dict[str, Any]:
    """Return the independently re-hashed original-download receipt.

    The bakes consume extracted members and hash-gate every one of them below.  Keeping the outer
    archive receipt as a separate immutable record preserves the exact Sketchfab download without
    pretending that the ZIP itself is a Blender input.
    """

    return {
        "filename": asset.archive_filename,
        "bytes": asset.archive_bytes,
        "sha256": asset.archive_sha256,
        "role": "original downloaded archive; every extracted member is separately hash-gated",
    }


def expected_input_sha256(asset: ReviewedAsset) -> dict[str, str]:
    return {
        asset.source_filename: asset.source_sha256,
        **dict(sorted(asset.texture_sha256.items())),
    }


def expected_tool_paths(asset: ReviewedAsset) -> dict[str, str]:
    return {
        "baker": f"tools/{asset.baker_filename}",
        "shared_recipe": "tools/reviewed_static_asset_recipe.py",
        "shared_blender": "tools/reviewed_static_blender.py",
        "offline_verifier": "tools/verify_reviewed_static_glb.py",
    }


def toolchain_receipt(asset: ReviewedAsset) -> dict[str, dict[str, Any]]:
    tools_directory = Path(__file__).resolve().parent
    return {
        role: file_record(
            tools_directory.parent / logical_path,
            logical_path=logical_path,
        )
        for role, logical_path in expected_tool_paths(asset).items()
    }


def verify_toolchain_receipt(
    asset: ReviewedAsset, records: object
) -> dict[str, dict[str, Any]]:
    expected_paths = expected_tool_paths(asset)
    if not isinstance(records, dict) or set(records) != set(expected_paths):
        raise ValueError(
            f"{asset.key} tool receipt set changed: expected {sorted(expected_paths)}, "
            f"got {sorted(records) if isinstance(records, dict) else type(records).__name__}"
        )
    repository_root = Path(__file__).resolve().parent.parent
    verified: dict[str, dict[str, Any]] = {}
    for role, logical_path in expected_paths.items():
        record = records.get(role)
        if not isinstance(record, dict) or record.get("path") != logical_path:
            raise ValueError(f"{asset.key} {role} tool path changed")
        actual = file_record(
            repository_root / logical_path,
            logical_path=logical_path,
        )
        if record != actual:
            raise ValueError(
                f"stale {asset.key} {role} tool receipt: expected {record}, got {actual}"
            )
        verified[role] = actual
    return verified


def validate_reviewed_inputs(
    asset: ReviewedAsset, source_path: Path, texture_directory: Path
) -> dict[str, str]:
    """Reject missing, renamed, or byte-different reviewed source inputs."""

    if source_path.name != asset.source_filename:
        raise ValueError(
            f"{asset.key} source must be named {asset.source_filename!r}, got {source_path.name!r}"
        )
    if not source_path.is_file():
        raise ValueError(f"reviewed source file does not exist: {source_path}")
    if not texture_directory.is_dir():
        raise ValueError(f"reviewed texture directory does not exist: {texture_directory}")

    observed = {asset.source_filename: sha256(source_path)}
    if observed[asset.source_filename] != asset.source_sha256:
        raise ValueError(
            f"unreviewed {asset.key} source bytes: expected {asset.source_sha256}, "
            f"got {observed[asset.source_filename]}"
        )
    for filename, expected_hash in sorted(asset.texture_sha256.items()):
        path = texture_directory / filename
        if not path.is_file():
            raise ValueError(f"required reviewed texture is missing: {path}")
        observed[filename] = sha256(path)
        if observed[filename] != expected_hash:
            raise ValueError(
                f"unreviewed {asset.key} texture bytes for {filename}: "
                f"expected {expected_hash}, got {observed[filename]}"
            )
    return observed


def atlas_offset(tile_index: int) -> tuple[float, float]:
    """Return the lower-left UV offset for a row-major 2x2 atlas tile.

    Tile zero is lower-left, one lower-right, two upper-left, and three upper-right.  This explicit
    convention avoids depending on PNG scan-line order or Blender UI terminology.
    """

    if tile_index < 0 or tile_index >= ATLAS_GRID_SIZE * ATLAS_GRID_SIZE:
        raise ValueError(f"atlas tile index outside 2x2 grid: {tile_index}")
    return (
        (tile_index % ATLAS_GRID_SIZE) / ATLAS_GRID_SIZE,
        (tile_index // ATLAS_GRID_SIZE) / ATLAS_GRID_SIZE,
    )


def remap_uv(uv: Sequence[float], tile_index: int) -> tuple[float, float]:
    offset_u, offset_v = atlas_offset(tile_index)
    return (
        float(uv[0]) / ATLAS_GRID_SIZE + offset_u,
        float(uv[1]) / ATLAS_GRID_SIZE + offset_v,
    )


def select_bridge_deck_plane(
    horizontal_faces: Iterable[tuple[float, float]], *, relative_area_tolerance: float = 1.0e-5
) -> float:
    """Choose the upper face of the bridge's two equal-area structural deck slab faces.

    Each input is ``(world_z, world_area_m2)`` for a horizontal polygon.  The reviewed source has
    one broad road slab with equal-area top and bottom polygons.  Selecting the higher of only the
    maximum-area cohort is robust to the smaller horizontal truss/footing faces while remaining an
    auditable geometry-derived origin rather than a hand-tuned placement offset.
    """

    faces = [(float(z), float(area)) for z, area in horizontal_faces if area > 0.0]
    if not faces:
        raise ValueError("bridge contains no positive-area horizontal faces")
    maximum_area = max(area for _, area in faces)
    cohort = [
        z
        for z, area in faces
        if abs(area - maximum_area) <= maximum_area * relative_area_tolerance
    ]
    if len(cohort) < 2:
        raise ValueError("bridge deck slab did not expose matching broad top and bottom faces")
    deck_z = max(cohort)
    if not -1.45 <= deck_z <= -1.43:
        raise ValueError(f"reviewed bridge deck moved unexpectedly: z={deck_z:.9f} m")
    return deck_z


def blender_dimensions_to_gltf(dimensions_xyz: Sequence[float]) -> tuple[float, float, float]:
    """Blender Z-up dimensions to glTF/Kessel Y-up dimensions."""

    if len(dimensions_xyz) != 3:
        raise ValueError("expected three dimensions")
    return (
        float(dimensions_xyz[0]),
        float(dimensions_xyz[2]),
        float(dimensions_xyz[1]),
    )
