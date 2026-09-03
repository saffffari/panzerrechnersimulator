#!/usr/bin/env python3
"""Pure-Python contract shared by the Soviet infantry Blender bake and its tests.

This module intentionally has no :mod:`bpy` dependency.  It describes the authored pose-frame
set, the animation timing contract, attribution text, file hashing, and the small amount of GLB
inspection needed to make the Blender pipeline fail closed when an export loses its skin or atlas.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import tempfile
from typing import Any, Iterable, Optional


ASSET_ID = "character.infantry.soviet_ww2_soldier_michael_jones"
ASSET_TITLE = "WW2 Soviet Soldier"
ASSET_AUTHOR = "Michael Jones"
ASSET_AUTHOR_HANDLE = "mikhailgolubkov09"
ASSET_SOURCE_PAGE = (
    "https://sketchfab.com/3d-models/ww2-soviet-soldier-331f36489ce440549dc1a6acb82fd931"
)
ASSET_LICENSE_NAME = "Creative Commons Attribution 4.0 International"
ASSET_LICENSE_SPDX = "CC-BY-4.0"
ASSET_LICENSE_URL = "https://creativecommons.org/licenses/by/4.0/"
RECIPE_SCHEMA = "kessel.soviet-infantry-pose-recipe"
BAKE_SCHEMA = "kessel.soviet-infantry-bake"
ANIMATION_SCHEMA = "kessel.soviet-infantry-animation-set"
ASSET_SCHEMA = "kessel.third-party-asset"
TARGET_HEIGHT_M = 1.72
SKELETAL_ACTION_PREFIX = "kessel_soviet_"
ANIMATION_FPS = 30
T_POSE_WIDTH_TO_HEIGHT = 0.82
A_POSE_WIDTH_TO_HEIGHT = 0.50

# A pose bake must not merely stay inside a character-sized bounding box.  Bad automatic weights
# can pull disconnected triangle islands through that box while retaining superficially plausible
# dimensions.  These limits therefore measure local edge strain and separation across vertices
# that occupied the same point in the supplied rest mesh.  They are deliberately generous enough
# for low-poly elbow/knee skinning but reject the long spikes and opened UV seams produced by an
# incompatible rest pose.
DEFORMATION_LIMITS: dict[str, float] = {
    "maximum_edge_stretch_ratio": 8.0,
    "p99_edge_stretch_ratio": 3.0,
    "minimum_edge_compression_ratio": 0.04,
    "p01_edge_compression_ratio": 0.18,
    "maximum_filtered_edge_posed_height_ratio": 0.02,
    "maximum_weld_separation_height_ratio": 0.02,
}


@dataclass(frozen=True)
class PoseFrame:
    """One deterministic evaluated pose exported as static runtime geometry.

    ``phase`` is a normalized gait phase. ``weapon_pose`` selects explicit hand/rifle targets in
    the Blender driver. ``post_tilt_degrees`` is applied after limb-pose evaluation—on the
    skeletal root for the authoring clip and on evaluated geometry for static runtime frames—so
    the death fall is independent of donor FBX bone-axis conventions.
    """

    name: str
    gait: str
    phase: float
    weapon_pose: str
    body_bob: float = 0.0
    recoil_m: float = 0.0
    post_tilt_degrees: float = 0.0
    reaction: str = "none"


POSE_FRAMES: tuple[PoseFrame, ...] = (
    PoseFrame("idle_00", "idle", 0.00, "low_ready", 0.000),
    PoseFrame("idle_01", "idle", 0.50, "low_ready", 0.006),
    PoseFrame("walk_00", "walk", 0.00, "low_ready", 0.000),
    PoseFrame("walk_01", "walk", 0.25, "low_ready", 0.018),
    PoseFrame("walk_02", "walk", 0.50, "low_ready", 0.000),
    PoseFrame("walk_03", "walk", 0.75, "low_ready", 0.018),
    PoseFrame("run_00", "run", 0.00, "run", 0.010),
    PoseFrame("run_01", "run", 0.25, "run", 0.040),
    PoseFrame("run_02", "run", 0.50, "run", 0.010),
    PoseFrame("run_03", "run", 0.75, "run", 0.040),
    PoseFrame("aim_00", "idle", 0.00, "aim", 0.000),
    PoseFrame("fire_01", "idle", 0.00, "aim", -0.003, 0.055),
    PoseFrame("fire_02", "idle", 0.00, "aim", 0.000, 0.018),
    PoseFrame("death_00", "idle", 0.00, "flinch", -0.012, 0.000, 0.0, "hit"),
    PoseFrame("death_01", "idle", 0.00, "flinch", -0.035, 0.000, 32.0, "collapse"),
    PoseFrame("death_02", "idle", 0.00, "loose", -0.060, 0.000, 68.0, "fall"),
    PoseFrame("death_03", "idle", 0.00, "loose", -0.075, 0.000, 91.0, "rest"),
)


# Sequence entries are (pose-frame name, integer hold frames). Frame files may be shared between
# sequences; e.g. firing starts from the same aimed pose used by the steady aim state. Integer
# frames are authoritative because Blender's glTF exporter samples clips on this same 30 Hz grid.
ANIMATION_SEQUENCES: dict[str, dict[str, Any]] = {
    "idle": {
        "loop": True,
        "frames": (("idle_00", 17), ("idle_01", 17)),
    },
    "walk": {
        "loop": True,
        "frames": tuple((f"walk_{index:02d}", 4) for index in range(4)),
    },
    "run": {
        "loop": True,
        "frames": tuple((f"run_{index:02d}", 3) for index in range(4)),
    },
    "aim": {
        "loop": True,
        "frames": (("aim_00", 5),),
    },
    "fire": {
        "loop": False,
        "frames": (("aim_00", 2), ("fire_01", 2), ("fire_02", 3)),
    },
    "death": {
        "loop": False,
        "frames": (
            ("death_00", 2),
            ("death_01", 4),
            ("death_02", 5),
            ("death_03", 30),
        ),
    },
}


REQUIRED_DONOR_BONES: tuple[str, ...] = (
    "Hips",
    "Spine",
    "Spine1",
    "Spine2",
    "Neck",
    "Head",
    "LeftShoulder",
    "LeftArm",
    "LeftForeArm",
    "LeftHand",
    "RightShoulder",
    "RightArm",
    "RightForeArm",
    "RightHand",
    "LeftUpLeg",
    "LeftLeg",
    "LeftFoot",
    "RightUpLeg",
    "RightLeg",
    "RightFoot",
)


def sha256_file(path: Path) -> str:
    """Return the real SHA-256 of ``path``; missing files are an error, never a placeholder."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def file_record(path: Path, *, logical_path: Optional[str] = None) -> dict[str, Any]:
    if not path.is_file():
        raise FileNotFoundError(path)
    return {
        "path": logical_path if logical_path is not None else path.name,
        "bytes": path.stat().st_size,
        "sha256": sha256_file(path),
    }


def preserve_source_bytes(source: Path, destination: Path) -> None:
    """Copy an operator-supplied source atomically without ever replacing other bytes."""

    source = source.expanduser().resolve()
    destination = destination.expanduser().resolve()
    if source == destination:
        if not source.is_file():
            raise FileNotFoundError(source)
        return
    source_hash = sha256_file(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        if destination.is_file() and sha256_file(destination) == source_hash:
            return
        raise RuntimeError(
            f"preserved source already exists with different bytes: {destination}; "
            "refusing to overwrite provenance authority"
        )

    temporary_path = None
    try:
        with tempfile.NamedTemporaryFile(
            prefix=f".{destination.name}.",
            suffix=".tmp",
            dir=destination.parent,
            delete=False,
        ) as temporary:
            temporary_path = Path(temporary.name)
        shutil.copyfile(source, temporary_path)
        if sha256_file(temporary_path) != source_hash:
            raise RuntimeError("byte-for-byte source archive verification failed")
        try:
            # The temporary file is in the destination directory, so this hard-link publication
            # is atomic and cannot replace a destination created by another process after the
            # existence check above.
            os.link(temporary_path, destination)
        except FileExistsError:
            if destination.is_file() and sha256_file(destination) == source_hash:
                return
            raise RuntimeError(
                f"preserved source appeared with different bytes: {destination}; "
                "refusing to overwrite provenance authority"
            ) from None
    finally:
        if temporary_path is not None and temporary_path.exists():
            temporary_path.unlink()


def pose_by_name(name: str) -> PoseFrame:
    for frame in POSE_FRAMES:
        if frame.name == name:
            return frame
    raise KeyError(name)


def classify_source_posture(width_m: float, height_m: float) -> dict[str, Any]:
    """Classify the unrigged rest silhouette without claiming semantic mesh labels.

    The selected Soviet source is visibly authored in a separated A-pose. A T-pose can reuse the
    donor's rest surface for interpolated weight transfer; an A-pose needs the donor joints refit
    before bone heat; a narrow arms-down/weapon-holding mesh uses the conservative compact path.
    Keeping the thresholds here makes this consequential auto-mode decision testable without
    Blender and records the measured ratio in every bake receipt.
    """

    width = float(width_m)
    height = float(height_m)
    if width <= 0.0 or height <= 0.0:
        raise ValueError("source posture dimensions must be positive")
    ratio = width / height
    if ratio >= T_POSE_WIDTH_TO_HEIGHT:
        classification = "t_pose"
        recommended_mode = "donor-transfer"
    elif ratio >= A_POSE_WIDTH_TO_HEIGHT:
        classification = "a_pose"
        recommended_mode = "a-pose"
    else:
        classification = "compact_standing"
        recommended_mode = "compact-standing"
    return {
        "classification": classification,
        "recommended_rigging_mode": recommended_mode,
        "width_to_height": round(ratio, 6),
        "a_pose_width_to_height": A_POSE_WIDTH_TO_HEIGHT,
        "t_pose_width_to_height": T_POSE_WIDTH_TO_HEIGHT,
    }


def expected_action_names() -> tuple[str, ...]:
    return tuple(f"{SKELETAL_ACTION_PREFIX}{name}" for name in ANIMATION_SEQUENCES)


def expected_action_durations_s() -> dict[str, float]:
    return {
        f"{SKELETAL_ACTION_PREFIX}{name}": round(
            sum(hold_frames for _, hold_frames in sequence["frames"])
            / ANIMATION_FPS,
            9,
        )
        for name, sequence in ANIMATION_SEQUENCES.items()
    }


def validate_recipe() -> None:
    names = [frame.name for frame in POSE_FRAMES]
    if len(names) != len(set(names)):
        raise ValueError("pose-frame names are not unique")
    known = set(names)
    for sequence_name, sequence in ANIMATION_SEQUENCES.items():
        frames = sequence["frames"]
        if not frames:
            raise ValueError(f"animation {sequence_name!r} has no frames")
        for frame_name, hold_frames in frames:
            if frame_name not in known:
                raise ValueError(
                    f"animation {sequence_name!r} references missing frame {frame_name!r}"
                )
            if not isinstance(hold_frames, int) or hold_frames <= 0:
                raise ValueError(
                    f"animation {sequence_name!r} has invalid integer hold frames"
                )
    if ANIMATION_SEQUENCES["death"]["loop"]:
        raise ValueError("death animation must not loop")
    if ANIMATION_SEQUENCES["fire"]["loop"]:
        raise ValueError("fire animation must not loop")


def animation_manifest(*, pose_directory: str = "baked") -> dict[str, Any]:
    validate_recipe()
    return {
        "schema": ANIMATION_SCHEMA,
        "version": 1,
        "asset_id": ASSET_ID,
        "runtime_mode": "static GLB pose-frame switching",
        "coordinate_system": {
            "gltf_up": "+Y",
            "gltf_forward": "+Z",
            "floor_axis": "Y",
            "floor_value_m": 0.0,
            "target_standing_height_m": TARGET_HEIGHT_M,
        },
        "poses": [
            {
                **asdict(frame),
                "path": f"{pose_directory}/soviet_ww2_soldier_{frame.name}.glb",
            }
            for frame in POSE_FRAMES
        ],
        "sequences": {
            name: {
                "loop": bool(sequence["loop"]),
                "skeletal_action": f"{SKELETAL_ACTION_PREFIX}{name}",
                "frames": [
                    {
                        "pose": frame_name,
                        "hold_frames": hold_frames,
                        "duration_s": hold_frames / ANIMATION_FPS,
                    }
                    for frame_name, hold_frames in sequence["frames"]
                ],
            }
            for name, sequence in ANIMATION_SEQUENCES.items()
        },
    }


def attribution_record(
    source_glb: Path,
    *,
    logical_source_path: str,
    retrieved: str,
) -> dict[str, Any]:
    """Build attribution from a file that actually exists and was supplied by the operator."""

    record = {
        "schema": ASSET_SCHEMA,
        "version": 1,
        "asset_id": ASSET_ID,
        "title": ASSET_TITLE,
        "author": ASSET_AUTHOR,
        "author_handle": ASSET_AUTHOR_HANDLE,
        "source_page": ASSET_SOURCE_PAGE,
        "retrieved": retrieved,
        "license": {
            "name": ASSET_LICENSE_NAME,
            "spdx": ASSET_LICENSE_SPDX,
            "url": ASSET_LICENSE_URL,
            "attribution_required": True,
        },
        "preserved_source": file_record(source_glb, logical_path=logical_source_path),
        "identity_note": (
            "The operator supplied this downloaded GLB for the cited Sketchfab page. "
            "The offline bake records its bytes and SHA-256 but does not claim to authenticate "
            "Sketchfab or infer remote provenance from geometry."
        ),
    }
    validate_hash_records(record)
    return record


def attribution_markdown(record: dict[str, Any]) -> str:
    source = record["preserved_source"]
    return f"""# {ASSET_TITLE}

- Creator: **{ASSET_AUTHOR}** (`{ASSET_AUTHOR_HANDLE}`)
- Source: <{ASSET_SOURCE_PAGE}>
- Retrieved: {record['retrieved']}
- License: **{ASSET_LICENSE_NAME}** (`{ASSET_LICENSE_SPDX}`), <{ASSET_LICENSE_URL}>
- Preserved source: `{source['path']}`
- Preserved source SHA-256: `{source['sha256']}`

## Attribution

“{ASSET_TITLE}” by {ASSET_AUTHOR} / {ASSET_AUTHOR_HANDLE}, used under CC BY 4.0.
Kessel adds donor-derived skin weights, a low-poly rifle, authored poses, and static runtime pose
frames. The original downloaded GLB remains byte-for-byte in `source/`; the conversion does not
replace or obscure the original creator attribution.

## Runtime conversion

`authoring/soviet_ww2_soldier_rigged.glb` is the editable, skinned master. Runtime pose files under
`baked/` are evaluated copies of that master because Kessel's current GLB loader flattens nodes and
does not play skeletal animation. Rebuild them with `tools/bake_soviet_infantry.py`; the bake report
records the actual source, donor, Blender, and output hashes.
"""


def license_notice() -> str:
    return f"""{ASSET_TITLE} by {ASSET_AUTHOR} / {ASSET_AUTHOR_HANDLE}
Source: {ASSET_SOURCE_PAGE}

This third-party model is licensed under the Creative Commons Attribution 4.0 International
license (CC BY 4.0): {ASSET_LICENSE_URL}

You may share and adapt the material under the license terms. Attribution must identify the
creator, provide the license link, and indicate that Kessel made rigging, rifle, pose, and runtime
bake adaptations. This notice is a concise attribution record; the linked Creative Commons legal
code is authoritative.
"""


def stable_json(data: Any) -> str:
    return json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def _walk_dicts(value: Any) -> Iterable[dict[str, Any]]:
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk_dicts(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            yield from _walk_dicts(child)


def validate_hash_records(value: Any) -> None:
    """Reject invented source/output hash sentinels in every SHA-bearing record."""

    for record in _walk_dicts(value):
        if "sha256" not in record:
            continue
        digest = record["sha256"]
        if not isinstance(digest, str) or len(digest) != 64:
            raise ValueError(f"invalid SHA-256 record: {digest!r}")
        if digest == "0" * 64:
            raise ValueError("all-zero SHA-256 placeholders are not provenance")
        try:
            int(digest, 16)
        except ValueError as error:
            raise ValueError(f"non-hex SHA-256 record: {digest!r}") from error


def verify_bake_receipt(asset_root: Path, report: dict[str, Any]) -> dict[str, Any]:
    """Re-hash every shipped/generated asset record and reject stale or extra pose files."""

    validate_hash_records(report)
    root = asset_root.resolve()
    records = [
        report["asset"]["preserved_source"],
        report["sources"]["preserved_soviet_glb"],
        report["rigged_master"],
        report["animation_manifest"],
        *report.get("metadata_files", []),
        *report["static_poses"],
    ]
    verified = {}
    for record in records:
        logical = Path(str(record["path"]))
        if logical.is_absolute():
            raise ValueError(f"asset receipt path must be relative: {logical}")
        resolved = (root / logical).resolve()
        try:
            resolved.relative_to(root)
        except ValueError as error:
            raise ValueError(f"asset receipt path escapes root: {logical}") from error
        if not resolved.is_file():
            raise FileNotFoundError(resolved)
        actual = file_record(resolved, logical_path=logical.as_posix())
        if actual["bytes"] != int(record["bytes"]):
            raise ValueError(f"asset receipt byte count is stale: {logical}")
        if actual["sha256"] != str(record["sha256"]):
            raise ValueError(f"asset receipt SHA-256 is stale: {logical}")
        previous = verified.get(logical.as_posix())
        if previous is not None and previous != actual:
            raise ValueError(f"conflicting duplicate asset receipt: {logical}")
        verified[logical.as_posix()] = actual

    expected_pose_paths = {
        f"baked/soviet_ww2_soldier_{frame.name}.glb" for frame in POSE_FRAMES
    }
    reported_pose_paths = {str(record["path"]) for record in report["static_poses"]}
    if reported_pose_paths != expected_pose_paths:
        raise ValueError("static pose receipt set does not match the authored recipe")
    actual_pose_paths = {
        path.relative_to(root).as_posix()
        for path in (root / "baked").glob("soviet_ww2_soldier_*.glb")
        if path.is_file()
    }
    if actual_pose_paths != expected_pose_paths:
        raise ValueError("baked directory has stale, missing, or extra Soviet pose GLBs")
    expected_authoring = {"authoring/soviet_ww2_soldier_rigged.glb"}
    actual_authoring = {
        path.relative_to(root).as_posix()
        for path in (root / "authoring").glob("*.glb")
        if path.is_file()
    }
    if actual_authoring != expected_authoring:
        raise ValueError("authoring directory has stale, missing, or extra GLBs")
    return {
        "verified_file_count": len(verified),
        "verified_paths": sorted(verified),
        "static_pose_count": len(expected_pose_paths),
        "unexpected_pose_count": 0,
        "status": "passed",
    }


def _percentile(values: Iterable[float], fraction: float) -> float:
    ordered = sorted(float(value) for value in values)
    if not ordered:
        raise ValueError("cannot calculate a percentile of no samples")
    if fraction <= 0.0:
        return ordered[0]
    if fraction >= 1.0:
        return ordered[-1]
    position = fraction * (len(ordered) - 1)
    lower = int(position)
    upper = min(len(ordered) - 1, lower + 1)
    blend = position - lower
    return ordered[lower] * (1.0 - blend) + ordered[upper] * blend


def deformation_quality_report(
    *,
    rest_edge_lengths: Iterable[float],
    posed_edge_lengths: Iterable[float],
    weld_separations_m: Iterable[float],
    height_m: float,
) -> dict[str, Any]:
    """Summarize local skin deformation and return explicit pass/fail violations.

    The edge arrays are paired by topology.  Authored edges below one-thousandth of character
    height are excluded because a sub-millimetre button/bevel edge can report a huge ratio while
    moving an invisible absolute distance; widespread damage remains covered by percentiles and
    coincident authored vertices by weld-separation samples. Keeping this policy in the
    pure-Python recipe makes the acceptance thresholds testable without Blender.
    """

    rest = [float(value) for value in rest_edge_lengths]
    posed = [float(value) for value in posed_edge_lengths]
    if len(rest) != len(posed):
        raise ValueError(
            f"rest/posed deformation sample counts differ: {len(rest)} != {len(posed)}"
        )
    if height_m <= 0.0:
        raise ValueError("deformation height must be positive")
    ratio_threshold = max(1.0e-7, height_m * 1.0e-3)
    ratios = [
        current / authored
        for authored, current in zip(rest, posed)
        if authored > ratio_threshold
    ]
    filtered_posed_lengths = [
        current
        for authored, current in zip(rest, posed)
        if authored <= ratio_threshold
    ]
    if not ratios:
        raise ValueError("deformation check has no non-degenerate topology edges")
    welds = [max(0.0, float(value)) for value in weld_separations_m]
    maximum_weld = max(welds, default=0.0)
    maximum_filtered_posed = max(filtered_posed_lengths, default=0.0)
    metrics = {
        "edge_sample_count": len(rest),
        "ratio_edge_sample_count": len(ratios),
        "filtered_short_edge_count": len(filtered_posed_lengths),
        "weld_sample_count": len(welds),
        "maximum_edge_stretch_ratio": max(ratios),
        "p99_edge_stretch_ratio": _percentile(ratios, 0.99),
        "minimum_edge_compression_ratio": min(ratios),
        "p01_edge_compression_ratio": _percentile(ratios, 0.01),
        "maximum_filtered_edge_posed_m": maximum_filtered_posed,
        "maximum_filtered_edge_posed_height_ratio": (
            maximum_filtered_posed / height_m
        ),
        "maximum_weld_separation_m": maximum_weld,
        "maximum_weld_separation_height_ratio": maximum_weld / height_m,
    }
    violations = []
    for metric in (
        "maximum_edge_stretch_ratio",
        "p99_edge_stretch_ratio",
        "maximum_filtered_edge_posed_height_ratio",
        "maximum_weld_separation_height_ratio",
    ):
        if metrics[metric] > DEFORMATION_LIMITS[metric]:
            violations.append(
                f"{metric}={metrics[metric]:.6f} exceeds {DEFORMATION_LIMITS[metric]:.6f}"
            )
    for metric in (
        "minimum_edge_compression_ratio",
        "p01_edge_compression_ratio",
    ):
        if metrics[metric] < DEFORMATION_LIMITS[metric]:
            violations.append(
                f"{metric}={metrics[metric]:.6f} is below {DEFORMATION_LIMITS[metric]:.6f}"
            )
    serialized_metrics = {
        name: int(value) if name.endswith("_count") else round(float(value), 8)
        for name, value in metrics.items()
    }
    return {
        **serialized_metrics,
        "limits": dict(DEFORMATION_LIMITS),
        "passed": not violations,
        "violations": violations,
    }


def read_glb_json(path: Path) -> dict[str, Any]:
    """Read the JSON chunk of a binary glTF 2.0 file without third-party dependencies."""

    raw = path.read_bytes()
    if len(raw) < 20:
        raise ValueError(f"GLB is too short: {path}")
    magic, version, declared_length = struct.unpack_from("<4sII", raw, 0)
    if magic != b"glTF" or version != 2:
        raise ValueError(f"not a binary glTF 2.0 file: {path}")
    if declared_length != len(raw):
        raise ValueError(
            f"GLB length mismatch for {path}: header={declared_length}, actual={len(raw)}"
        )
    offset = 12
    while offset + 8 <= len(raw):
        chunk_length, chunk_type = struct.unpack_from("<II", raw, offset)
        offset += 8
        end = offset + chunk_length
        if end > len(raw):
            raise ValueError(f"truncated GLB chunk in {path}")
        if chunk_type == 0x4E4F534A:  # JSON
            return json.loads(raw[offset:end].rstrip(b" \t\r\n\x00").decode("utf-8"))
        offset = end
    raise ValueError(f"GLB has no JSON chunk: {path}")


def validate_glb_contract(
    path: Path,
    *,
    require_skin: bool,
    require_animations: bool,
    expected_animation_names: Iterable[str] = (),
    expected_animation_durations_s: Optional[dict[str, float]] = None,
) -> dict[str, Any]:
    document = read_glb_json(path)
    meshes = document.get("meshes", [])
    nodes = document.get("nodes", [])
    accessors = document.get("accessors", [])
    materials = document.get("materials", [])
    textures = document.get("textures", [])
    images = document.get("images", [])
    buffer_views = document.get("bufferViews", [])
    if not meshes:
        raise ValueError(f"GLB contains no meshes: {path}")
    if not nodes:
        raise ValueError(f"GLB contains no scene nodes: {path}")
    if not materials:
        raise ValueError(f"GLB contains no materials: {path}")
    primitive_count = 0
    for mesh_index, mesh in enumerate(meshes):
        primitives = mesh.get("primitives", [])
        if not primitives:
            raise ValueError(f"GLB mesh {mesh_index} has no primitives")
        for primitive in primitives:
            primitive_count += 1
            attributes = primitive.get("attributes", {})
            position = attributes.get("POSITION")
            if not isinstance(position, int) or not (0 <= position < len(accessors)):
                raise ValueError(f"GLB mesh {mesh_index} lacks a valid POSITION accessor")
            material = primitive.get("material")
            if not isinstance(material, int) or not (0 <= material < len(materials)):
                raise ValueError(f"GLB mesh {mesh_index} lacks a valid material reference")
    mesh_nodes = [node for node in nodes if isinstance(node.get("mesh"), int)]
    referenced_meshes = {int(node["mesh"]) for node in mesh_nodes}
    if any(index < 0 or index >= len(meshes) for index in referenced_meshes):
        raise ValueError("GLB node references an invalid mesh")
    if referenced_meshes != set(range(len(meshes))):
        raise ValueError("GLB does not place every mesh on a scene node")

    skins = document.get("skins", [])
    has_skin = bool(skins)
    if has_skin != require_skin:
        raise ValueError(
            f"GLB skin contract failed for {path}: expected {require_skin}, got {has_skin}"
        )
    joint_nodes = set()
    if require_skin:
        for skin_index, skin in enumerate(skins):
            joints = skin.get("joints", [])
            if not joints or any(
                not isinstance(joint, int) or not (0 <= joint < len(nodes))
                for joint in joints
            ):
                raise ValueError(f"GLB skin {skin_index} has no valid joint nodes")
            joint_nodes.update(int(joint) for joint in joints)
            inverse_bind = skin.get("inverseBindMatrices")
            if not isinstance(inverse_bind, int) or not (0 <= inverse_bind < len(accessors)):
                raise ValueError(f"GLB skin {skin_index} lacks valid inverse bind matrices")
        for node in mesh_nodes:
            skin_index = node.get("skin")
            if not isinstance(skin_index, int) or not (0 <= skin_index < len(skins)):
                raise ValueError("GLB mesh node is not bound to a valid skin")
            mesh = meshes[int(node["mesh"])]
            for primitive in mesh["primitives"]:
                attributes = primitive["attributes"]
                for semantic in ("JOINTS_0", "WEIGHTS_0"):
                    accessor_index = attributes.get(semantic)
                    if not isinstance(accessor_index, int) or not (
                        0 <= accessor_index < len(accessors)
                    ):
                        raise ValueError(
                            f"GLB skinned mesh lacks a valid {semantic} accessor"
                        )
    elif any("skin" in node for node in mesh_nodes):
        raise ValueError("static GLB mesh node unexpectedly references a skin")

    animations = document.get("animations", [])
    has_animations = bool(animations)
    if has_animations != require_animations:
        raise ValueError(
            f"GLB animation contract failed for {path}: "
            f"expected {require_animations}, got {has_animations}"
        )
    names = {str(animation.get("name", "")) for animation in animations}
    missing = set(expected_animation_names) - names
    if missing:
        raise ValueError(f"GLB is missing skeletal actions: {sorted(missing)}")
    animation_spans = {}
    animation_interpolations = {}
    animation_channel_count = 0
    for animation in animations:
        animation_name = str(animation.get("name", ""))
        samplers = animation.get("samplers", [])
        channels = animation.get("channels", [])
        if not samplers or not channels:
            raise ValueError(
                f"GLB action {animation_name!r} has no samplers or channels"
            )
        referenced_samplers = set()
        for channel in channels:
            sampler_index = channel.get("sampler")
            if not isinstance(sampler_index, int) or not (
                0 <= sampler_index < len(samplers)
            ):
                raise ValueError(f"GLB action {animation_name!r} has a dangling channel")
            referenced_samplers.add(sampler_index)
            target = channel.get("target", {})
            target_node = target.get("node")
            if not isinstance(target_node, int) or not (0 <= target_node < len(nodes)):
                raise ValueError(f"GLB action {animation_name!r} targets an invalid node")
            if require_skin and target_node not in joint_nodes:
                raise ValueError(f"GLB action {animation_name!r} targets a non-joint node")
            if target.get("path") not in {"translation", "rotation", "scale"}:
                raise ValueError(f"GLB action {animation_name!r} has an invalid target path")
            animation_channel_count += 1
        if referenced_samplers != set(range(len(samplers))):
            raise ValueError(f"GLB action {animation_name!r} contains unused samplers")
        time_accessors = []
        interpolation_counts: dict[str, int] = {}
        for sampler in samplers:
            accessor_index = sampler.get("input")
            if not isinstance(accessor_index, int) or not (0 <= accessor_index < len(accessors)):
                raise ValueError(
                    f"GLB action {animation_name!r} has an invalid time accessor"
                )
            accessor = accessors[accessor_index]
            output_index = sampler.get("output")
            if not isinstance(output_index, int) or not (0 <= output_index < len(accessors)):
                raise ValueError(
                    f"GLB action {animation_name!r} has an invalid output accessor"
                )
            interpolation = str(sampler.get("interpolation", "LINEAR"))
            if interpolation not in {"LINEAR", "STEP"}:
                raise ValueError(
                    f"GLB action {animation_name!r} uses unsupported {interpolation} "
                    "interpolation"
                )
            interpolation_counts[interpolation] = (
                interpolation_counts.get(interpolation, 0) + 1
            )
            minimum = accessor.get("min")
            maximum = accessor.get("max")
            if not minimum or not maximum:
                raise ValueError(
                    f"GLB action {animation_name!r} time accessor lacks min/max"
                )
            time_accessors.append((float(minimum[0]), float(maximum[0])))
        if not time_accessors:
            raise ValueError(f"GLB action {animation_name!r} has no usable time samples")
        if time_accessors:
            start_s = min(start for start, _ in time_accessors)
            end_s = max(end for _, end in time_accessors)
            animation_spans[animation_name] = {
                "start_s": round(start_s, 8),
                "end_s": round(end_s, 8),
                "duration_s": round(end_s - start_s, 8),
            }
            animation_interpolations[animation_name] = interpolation_counts
    if expected_animation_durations_s is not None:
        for animation_name, expected_duration in expected_animation_durations_s.items():
            if animation_name not in animation_spans:
                raise ValueError(f"GLB action {animation_name!r} has no exported time span")
            span = animation_spans[animation_name]
            if abs(float(span["start_s"])) > 1.0e-5:
                raise ValueError(
                    f"GLB action {animation_name!r} starts at {span['start_s']:.8f}s, not zero"
                )
            if abs(float(span["duration_s"]) - float(expected_duration)) > 1.0e-4:
                raise ValueError(
                    f"GLB action {animation_name!r} duration {span['duration_s']:.8f}s "
                    f"does not match {float(expected_duration):.8f}s"
                )
    textured_materials = 0
    for material in materials:
        texture_info = material.get("pbrMetallicRoughness", {}).get("baseColorTexture")
        if texture_info is None:
            continue
        texture_index = texture_info.get("index")
        if not isinstance(texture_index, int) or not (0 <= texture_index < len(textures)):
            raise ValueError("GLB material has an invalid base-color texture")
        source_index = textures[texture_index].get("source")
        if not isinstance(source_index, int) or not (0 <= source_index < len(images)):
            raise ValueError("GLB texture has an invalid image source")
        image = images[source_index]
        embedded = isinstance(image.get("bufferView"), int) and (
            0 <= int(image["bufferView"]) < len(buffer_views)
        )
        data_uri = str(image.get("uri", "")).startswith("data:")
        if not embedded and not data_uri:
            raise ValueError("GLB base-color atlas is not self-contained")
        textured_materials += 1
    if textured_materials == 0:
        raise ValueError("GLB has no material-to-texture-to-image atlas chain")
    return {
        "mesh_count": len(meshes),
        "primitive_count": primitive_count,
        "mesh_node_count": len(mesh_nodes),
        "skin_count": len(skins),
        "animation_names": sorted(names),
        "animation_spans_s": animation_spans,
        "animation_interpolations": animation_interpolations,
        "animation_channel_count": animation_channel_count,
        "material_count": len(materials),
        "textured_material_count": textured_materials,
        "texture_count": len(textures),
        "image_count": len(images),
    }


validate_recipe()
