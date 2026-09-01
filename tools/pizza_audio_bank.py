#!/usr/bin/env python3
"""Validate and cook Kessel's owner-private Pizza Doggy audio layer.

The tracked selection manifest names an exact, byte-pinned reviewed OGG closure.  ``cook`` reads that
closure only from ``KESSEL_PIZZA_AUDIO_ROOT`` and writes a fresh, gitignored runtime bank plus a
receipt. It never scans a source pack while copying. ``fingerprints`` writes an owner-private
candidate report without editing the manifest; ``candidates`` remains the separate filename
discovery step used before an operator promotes an exact path and reviewed fingerprint.

Pizza Doggy source audio and cooked OGGs are owner-private game-integration material.  They must not
be committed, attached to an issue, published as an asset bank, or included in the public package.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any, Iterable


SELECTION_SCHEMA = "kessel.pizza-audio-selection"
SELECTION_VERSION = 1
RECEIPT_SCHEMA = "kessel.pizza-audio-bank-receipt"
RECEIPT_VERSION = 1
DEFAULT_MANIFEST = Path("catalog/pizza_audio_selection.v1.json")
SOURCE_ROOT_ENV = "KESSEL_PIZZA_AUDIO_ROOT"
LICENSE_ACK = "owner-private-game-integration-only"
RECEIPT_NAME = "BANK-RECEIPT.json"

ID_RE = re.compile(r"^[a-z0-9]+(?:[._-][a-z0-9]+)*$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
ALLOWED_PACKS = frozenset(
    {
        "Echoes - Audio Super Kit v2.0.1",
        "ROT - Horror Audio Bundle v1.8",
        "Rust & Blood - SFX Library v1.1d",
    }
)
ALLOWED_EVENTS = frozenset(
    {
        "armor_clang_1",
        "armor_clang_2",
        "brass_clatter",
        "breech_clack",
        "cook_off",
        "hammer_strike",
        "impact_ground",
        "kill_explosion",
        "luger_shot",
        "pickup",
        "rifle_shot",
    }
)
PROTECTED_EVENTS = frozenset(
    {
        "ambience_exterior",
        "ambience_interior",
        "enemy_gun",
        "engine_death",
        "engine_loop",
        "impact_tank",
        "near_miss",
        "penetration_hit",
        "player_gun_ext_1",
        "player_gun_ext_2",
        "player_gun_ext_3",
        "player_gun_ext_4",
        "player_gun_int_1",
        "player_gun_int_2",
        "shell_shriek",
        "turret_pop",
        "turret_rip",
        "turret_traverse_loop",
    }
)


class AudioBankError(RuntimeError):
    """A deterministic, user-facing selection or bank validation failure."""


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")


def sha256_bytes(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_relative(raw: Any, field: str, *, suffix: str | None = None) -> PurePosixPath:
    if not isinstance(raw, str) or not raw or raw.strip() != raw:
        raise AudioBankError(f"{field} must be a non-empty, trimmed string")
    if "\\" in raw or any(ord(char) < 32 for char in raw):
        raise AudioBankError(f"{field} must use clean forward-slash path syntax: {raw!r}")
    path = PurePosixPath(raw)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise AudioBankError(f"{field} must be a canonical relative path: {raw!r}")
    if suffix is not None and path.suffix.casefold() != suffix.casefold():
        raise AudioBankError(f"{field} must end in {suffix}: {raw!r}")
    return path


def finite_gain(value: Any, field: str, maximum: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise AudioBankError(f"{field} must be a finite number")
    gain = float(value)
    if not math.isfinite(gain) or gain <= 0.0 or gain > maximum:
        raise AudioBankError(f"{field} must be in (0, {maximum}]")
    return gain


def reviewed_fingerprint(
    variant: dict[str, Any], field: str
) -> tuple[int, str] | None:
    """Return a reviewed source fingerprint, or ``None`` for an explicit pending pair."""
    if "expected_bytes" not in variant or "expected_sha256" not in variant:
        raise AudioBankError(
            f"{field} must declare expected_bytes and expected_sha256 (use null for pending review)"
        )
    size = variant["expected_bytes"]
    digest = variant["expected_sha256"]
    if size is None and digest is None:
        return None
    if size is None or digest is None:
        raise AudioBankError(
            f"{field} expected_bytes/expected_sha256 must both be reviewed values or both null"
        )
    if isinstance(size, bool) or not isinstance(size, int) or size <= 0:
        raise AudioBankError(f"{field}.expected_bytes must be a positive integer")
    if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
        raise AudioBankError(f"{field}.expected_sha256 must be lowercase SHA-256")
    return size, digest


def load_manifest(path: Path) -> tuple[dict[str, Any], bytes]:
    try:
        blob = path.read_bytes()
        value = json.loads(blob)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AudioBankError(f"cannot read selection manifest {path}: {error}") from error
    if not isinstance(value, dict):
        raise AudioBankError("selection manifest root must be an object")
    validate_manifest(value)
    return value, blob


def validate_manifest(manifest: dict[str, Any]) -> None:
    if manifest.get("schema") != SELECTION_SCHEMA or manifest.get("version") != SELECTION_VERSION:
        raise AudioBankError("unsupported Pizza audio selection schema/version")
    selection_id = manifest.get("selection_id")
    if not isinstance(selection_id, str) or not ID_RE.fullmatch(selection_id):
        raise AudioBankError("selection_id must be a stable lowercase identifier")
    if manifest.get("source_root_env") != SOURCE_ROOT_ENV:
        raise AudioBankError(f"source_root_env must remain {SOURCE_ROOT_ENV}")
    if manifest.get("runtime_bank") != "assets/sounds/pizza_private":
        raise AudioBankError("runtime_bank must remain the gitignored private bank path")

    license_policy = manifest.get("license_policy")
    required_license = {
        "license_id": "pizzadoggy-local-purchase",
        "owner_private": True,
        "game_integration_only": True,
        "standalone_redistribution": False,
    }
    if not isinstance(license_policy, dict) or any(
        license_policy.get(key) != value for key, value in required_license.items()
    ):
        raise AudioBankError("license_policy does not preserve the owner-private integration boundary")

    runtime_policy = manifest.get("runtime_policy")
    if not isinstance(runtime_policy, dict):
        raise AudioBankError("runtime_policy must be an object")
    if runtime_policy.get("explicit_opt_in_env") != "KESSEL_ENABLE_PIZZA_AUDIO":
        raise AudioBankError("Pizza audio must retain its dedicated explicit opt-in")
    if runtime_policy.get("procedural_base") != "always":
        raise AudioBankError("Pizza cues may layer only; the procedural base must remain always on")
    maximum_gain = finite_gain(
        runtime_policy.get("maximum_layer_gain"),
        "runtime_policy.maximum_layer_gain",
        0.35,
    )
    maximum_variants = runtime_policy.get("maximum_variants_per_event")
    if isinstance(maximum_variants, bool) or not isinstance(maximum_variants, int):
        raise AudioBankError("maximum_variants_per_event must be an integer")
    if maximum_variants < 1 or maximum_variants > 8:
        raise AudioBankError("maximum_variants_per_event must be in 1..=8")

    protected = manifest.get("protected_runtime_identities")
    if not isinstance(protected, list) or set(protected) != PROTECTED_EVENTS:
        raise AudioBankError("protected_runtime_identities drifted from the vehicle/combat core")

    mappings = manifest.get("mappings")
    if not isinstance(mappings, list) or not mappings:
        raise AudioBankError("mappings must be a non-empty array")
    seen_events: set[str] = set()
    seen_cues: set[str] = set()
    seen_sources: set[str] = set()
    for mapping_index, mapping in enumerate(mappings):
        context = f"mappings[{mapping_index}]"
        if not isinstance(mapping, dict):
            raise AudioBankError(f"{context} must be an object")
        event = mapping.get("event")
        if event not in ALLOWED_EVENTS or event in PROTECTED_EVENTS:
            raise AudioBankError(f"{context}.event is not an allowed Pizza layer target: {event!r}")
        if event in seen_events:
            raise AudioBankError(f"duplicate event mapping: {event}")
        seen_events.add(event)
        event_max = finite_gain(
            mapping.get("maximum_layer_gain"), f"{context}.maximum_layer_gain", maximum_gain
        )
        if event_max > maximum_gain:
            raise AudioBankError(f"{context}.maximum_layer_gain exceeds the global bound")
        variants = mapping.get("variants")
        if not isinstance(variants, list) or len(variants) > maximum_variants:
            raise AudioBankError(f"{context}.variants must contain at most {maximum_variants} entries")
        for variant_index, variant in enumerate(variants):
            vctx = f"{context}.variants[{variant_index}]"
            if not isinstance(variant, dict):
                raise AudioBankError(f"{vctx} must be an object")
            cue_id = variant.get("cue_id")
            if not isinstance(cue_id, str) or not ID_RE.fullmatch(cue_id):
                raise AudioBankError(f"{vctx}.cue_id must be a stable lowercase identifier")
            if cue_id in seen_cues:
                raise AudioBankError(f"duplicate cue_id: {cue_id}")
            seen_cues.add(cue_id)
            source = safe_relative(variant.get("source_relative"), f"{vctx}.source_relative", suffix=".ogg")
            if source.parts[0] not in ALLOWED_PACKS:
                raise AudioBankError(f"{vctx}.source_relative uses an unreviewed pack: {source.parts[0]!r}")
            source_string = source.as_posix()
            if source_string in seen_sources:
                raise AudioBankError(f"duplicate selected source: {source_string}")
            seen_sources.add(source_string)
            reviewed_fingerprint(variant, vctx)
            finite_gain(variant.get("gain"), f"{vctx}.gain", event_max)

    candidates = manifest.get("candidate_review")
    if not isinstance(candidates, list):
        raise AudioBankError("candidate_review must be an array")
    seen_candidates: set[str] = set()
    for candidate_index, candidate in enumerate(candidates):
        context = f"candidate_review[{candidate_index}]"
        if not isinstance(candidate, dict):
            raise AudioBankError(f"{context} must be an object")
        candidate_id = candidate.get("candidate_id")
        if not isinstance(candidate_id, str) or not ID_RE.fullmatch(candidate_id):
            raise AudioBankError(f"{context}.candidate_id must be a stable lowercase identifier")
        if candidate_id in seen_candidates:
            raise AudioBankError(f"duplicate candidate_id: {candidate_id}")
        seen_candidates.add(candidate_id)
        if candidate.get("pack") not in ALLOWED_PACKS:
            raise AudioBankError(f"{context}.pack is not one of the retained purchased packs")
        targets = candidate.get("target_events")
        if not isinstance(targets, list) or not targets or not set(targets) <= ALLOWED_EVENTS:
            raise AudioBankError(f"{context}.target_events contains an unsupported target")
        for field in ("include_any", "exclude_any"):
            terms = candidate.get(field)
            if not isinstance(terms, list) or not all(
                isinstance(term, str) and term.strip() == term and term for term in terms
            ):
                raise AudioBankError(f"{context}.{field} must be a non-empty string array")
        if candidate.get("review_status") != "unresolved-owner-audition-required":
            raise AudioBankError(f"{context}.review_status must remain explicitly unresolved")


def selected_variants(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    selected: list[dict[str, Any]] = []
    for mapping in manifest["mappings"]:
        for variant in mapping["variants"]:
            selected.append({"event": mapping["event"], **variant})
    return sorted(selected, key=lambda item: (item["event"], item["cue_id"]))


def resolve_source_root(value: str | os.PathLike[str] | None = None) -> Path:
    raw = os.fspath(value) if value is not None else os.environ.get(SOURCE_ROOT_ENV)
    if not raw:
        raise AudioBankError(
            f"{SOURCE_ROOT_ENV} is not set; point it at the read-only Pizza _extracted directory"
        )
    root = Path(raw).resolve()
    if not root.is_dir():
        raise AudioBankError(f"{SOURCE_ROOT_ENV} does not name a directory: {root}")
    return root


def source_path(root: Path, relative: str, field: str = "source_relative") -> Path:
    relative_path = safe_relative(relative, field, suffix=".ogg")
    candidate = root.joinpath(*relative_path.parts)
    try:
        resolved = candidate.resolve(strict=True)
    except OSError as error:
        raise AudioBankError(f"selected source is missing: {relative_path}") from error
    if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
        raise AudioBankError(f"selected source escapes or is not a regular file: {relative_path}")
    return resolved


def pending_fingerprint_cues(manifest: dict[str, Any]) -> list[str]:
    return [
        selected["cue_id"]
        for selected in selected_variants(manifest)
        if reviewed_fingerprint(selected, f"cue {selected['cue_id']}") is None
    ]


def fingerprint_sources(manifest: dict[str, Any], root: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for selected in selected_variants(manifest):
        source = source_path(root, selected["source_relative"])
        size = source.stat().st_size
        digest = sha256_file(source)
        records.append({**selected, "source_path": source, "bytes": size, "sha256": digest})
    return records


def validate_sources(manifest: dict[str, Any], root: Path) -> list[dict[str, Any]]:
    pending = pending_fingerprint_cues(manifest)
    if pending:
        raise AudioBankError(
            "selected source fingerprints are pending owner review for "
            f"{', '.join(pending)}; run the fingerprints command and review its candidate report"
        )
    records = fingerprint_sources(manifest, root)
    for record in records:
        expected = reviewed_fingerprint(record, f"cue {record['cue_id']}")
        assert expected is not None
        if (record["bytes"], record["sha256"]) != expected:
            raise AudioBankError(
                f"selected source bytes do not match the reviewed fingerprint: {record['cue_id']}"
            )
    return records


def expected_runtime_file(cue_id: str) -> str:
    if not ID_RE.fullmatch(cue_id):
        raise AudioBankError(f"unsafe cue_id cannot form a runtime path: {cue_id!r}")
    return f"cues/{cue_id}.ogg"


def receipt_for(
    manifest: dict[str, Any], manifest_blob: bytes, records: Iterable[dict[str, Any]]
) -> dict[str, Any]:
    pending = pending_fingerprint_cues(manifest)
    if pending:
        raise AudioBankError(
            "cannot issue a bank receipt while reviewed source fingerprints are pending for "
            f"{', '.join(pending)}"
        )
    files = [
        {
            "bytes": record["bytes"],
            "cue_id": record["cue_id"],
            "event": record["event"],
            "gain": record["gain"],
            "runtime_file": expected_runtime_file(record["cue_id"]),
            "sha256": record["sha256"],
        }
        for record in records
    ]
    return {
        "schema": RECEIPT_SCHEMA,
        "version": RECEIPT_VERSION,
        "selection_id": manifest["selection_id"],
        "selection_manifest_sha256": sha256_bytes(manifest_blob),
        "license_policy": manifest["license_policy"],
        "runtime_policy": manifest["runtime_policy"],
        "file_count": len(files),
        "total_bytes": sum(record["bytes"] for record in files),
        "files": files,
    }


def expected_bank_files(receipt: dict[str, Any]) -> set[str]:
    expected = {RECEIPT_NAME}
    for index, record in enumerate(receipt.get("files", [])):
        if not isinstance(record, dict):
            raise AudioBankError(f"receipt files[{index}] must be an object")
        path = safe_relative(record.get("runtime_file"), f"receipt files[{index}].runtime_file", suffix=".ogg")
        if not path.parts or path.parts[0] != "cues":
            raise AudioBankError(f"receipt files[{index}] must live below cues/")
        expected.add(path.as_posix())
    return expected


def validate_receipt(
    receipt: dict[str, Any], manifest: dict[str, Any], manifest_blob: bytes
) -> None:
    if receipt.get("schema") != RECEIPT_SCHEMA or receipt.get("version") != RECEIPT_VERSION:
        raise AudioBankError("unsupported Pizza audio bank receipt schema/version")
    if receipt.get("selection_id") != manifest["selection_id"]:
        raise AudioBankError("bank receipt selection_id does not match the tracked manifest")
    if receipt.get("selection_manifest_sha256") != sha256_bytes(manifest_blob):
        raise AudioBankError("bank receipt was cooked from a different selection manifest")
    if receipt.get("license_policy") != manifest["license_policy"]:
        raise AudioBankError("bank receipt license policy does not match the tracked manifest")
    if receipt.get("runtime_policy") != manifest["runtime_policy"]:
        raise AudioBankError("bank receipt runtime policy does not match the tracked manifest")
    files = receipt.get("files")
    selected = selected_variants(manifest)
    pending = pending_fingerprint_cues(manifest)
    if pending:
        raise AudioBankError(
            "tracked selection has pending reviewed source fingerprints for "
            f"{', '.join(pending)}"
        )
    if not isinstance(files, list) or len(files) != len(selected):
        raise AudioBankError("bank receipt file closure does not match the tracked selection")
    total_bytes = 0
    for index, (record, variant) in enumerate(zip(files, selected)):
        if not isinstance(record, dict):
            raise AudioBankError(f"bank receipt files[{index}] must be an object")
        expected_fields = {
            "bytes": variant["expected_bytes"],
            "cue_id": variant["cue_id"],
            "event": variant["event"],
            "gain": variant["gain"],
            "runtime_file": expected_runtime_file(variant["cue_id"]),
            "sha256": variant["expected_sha256"],
        }
        if any(record.get(key) != value for key, value in expected_fields.items()):
            raise AudioBankError(
                f"bank receipt files[{index}] mapping does not match the tracked selection"
            )
        size = record.get("bytes")
        digest = record.get("sha256")
        if isinstance(size, bool) or not isinstance(size, int) or size <= 0:
            raise AudioBankError(f"bank receipt files[{index}].bytes must be positive")
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            raise AudioBankError(f"bank receipt files[{index}].sha256 must be lowercase SHA-256")
        total_bytes += size
    if receipt.get("file_count") != len(selected):
        raise AudioBankError("bank receipt file_count is inconsistent")
    if receipt.get("total_bytes") != total_bytes:
        raise AudioBankError("bank receipt total_bytes is inconsistent")


def validate_bank(bank: Path, manifest: dict[str, Any], manifest_blob: bytes) -> dict[str, Any]:
    bank = bank.resolve()
    receipt_path = bank / RECEIPT_NAME
    try:
        receipt = json.loads(receipt_path.read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AudioBankError(f"cannot read bank receipt {receipt_path}: {error}") from error
    if not isinstance(receipt, dict):
        raise AudioBankError("bank receipt root must be an object")
    validate_receipt(receipt, manifest, manifest_blob)
    entries = list(bank.rglob("*"))
    for path in entries:
        if path.is_symlink():
            raise AudioBankError(f"bank closure may not contain symlinks: {path.relative_to(bank)}")
        if not path.is_file() and not path.is_dir():
            raise AudioBankError(f"bank closure contains a non-file entry: {path.relative_to(bank)}")
    actual = {
        path.relative_to(bank).as_posix()
        for path in entries
        if path.is_file()
    }
    expected = expected_bank_files(receipt)
    if actual != expected:
        raise AudioBankError(
            f"bank closure mismatch; missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
        )
    for index, record in enumerate(receipt["files"]):
        relative = safe_relative(record["runtime_file"], f"receipt files[{index}].runtime_file", suffix=".ogg")
        path = bank.joinpath(*relative.parts)
        if path.is_symlink() or not path.is_file():
            raise AudioBankError(f"bank cue is not a regular copied file: {relative}")
        size = path.stat().st_size
        digest = sha256_file(path)
        if size != record["bytes"] or digest != record["sha256"]:
            raise AudioBankError(f"bank cue bytes do not match receipt: {relative}")
    return receipt


def cook_bank(
    manifest: dict[str, Any],
    manifest_blob: bytes,
    source_root: Path,
    output: Path,
    *,
    license_ack: str,
) -> dict[str, Any]:
    if license_ack != LICENSE_ACK:
        raise AudioBankError(f"cook requires --ack-license {LICENSE_ACK}")
    output = output.resolve()
    source_root = source_root.resolve()
    if output == source_root or output.is_relative_to(source_root):
        raise AudioBankError("cook output may not touch the read-only Pizza source library")
    if output.exists():
        raise AudioBankError(f"cook output must be a new path; refusing to overwrite {output}")
    records = validate_sources(manifest, source_root)
    if not records:
        raise AudioBankError("selection contains no reviewed OGG variants")
    receipt = receipt_for(manifest, manifest_blob, records)
    output.mkdir(parents=True, exist_ok=False)
    try:
        for record in records:
            runtime = PurePosixPath(expected_runtime_file(record["cue_id"]))
            destination = output.joinpath(*runtime.parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(record["source_path"], destination, follow_symlinks=True)
            os.chmod(destination, 0o644)
            os.utime(destination, (0, 0))
        receipt_path = output / RECEIPT_NAME
        receipt_path.write_bytes(canonical_json(receipt))
        os.chmod(receipt_path, 0o644)
        os.utime(receipt_path, (0, 0))
        validate_bank(output, manifest, manifest_blob)
    except BaseException:
        shutil.rmtree(output, ignore_errors=True)
        raise
    return receipt


def candidate_matches(candidate: dict[str, Any], source_root: Path) -> list[str]:
    pack_root = source_root / candidate["pack"]
    if not pack_root.is_dir():
        return []
    include = [term.casefold() for term in candidate["include_any"]]
    exclude = [term.casefold() for term in candidate["exclude_any"]]
    matches: list[str] = []
    for path in sorted(pack_root.rglob("*")):
        if not path.is_file() or path.suffix.casefold() != ".ogg":
            continue
        relative = path.relative_to(source_root).as_posix()
        folded = relative.casefold().replace("_", " ").replace("-", " ")
        if any(term in folded for term in include) and not any(term in folded for term in exclude):
            matches.append(relative)
    return matches


def write_candidate_report(
    manifest: dict[str, Any], source_root: Path, output: Path | None = None
) -> dict[str, Any]:
    report = {
        "schema": "kessel.pizza-audio-candidate-report",
        "version": 1,
        "selection_id": manifest["selection_id"],
        "owner_private": True,
        "candidates": [
            {
                "candidate_id": candidate["candidate_id"],
                "target_events": candidate["target_events"],
                "matches": candidate_matches(candidate, source_root),
            }
            for candidate in manifest["candidate_review"]
        ],
    }
    if output is not None:
        resolved_output = output.resolve()
        resolved_source = source_root.resolve()
        if resolved_output == resolved_source or resolved_output.is_relative_to(resolved_source):
            raise AudioBankError("candidate report may not touch the read-only Pizza source library")
        if output.exists():
            raise AudioBankError(f"candidate report output exists: {output}")
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(canonical_json(report))
    return report


def private_fingerprint_report_path(repo_root: Path, output: Path) -> Path:
    candidate = output if output.is_absolute() else repo_root / output
    resolved = candidate.resolve()
    private_root = (repo_root / "artifacts/audio").resolve()
    if not resolved.is_relative_to(private_root):
        raise AudioBankError(
            "fingerprint reports must stay below the gitignored artifacts/audio directory"
        )
    return resolved


def write_fingerprint_report(
    manifest: dict[str, Any],
    manifest_blob: bytes,
    source_root: Path,
    output: Path,
) -> dict[str, Any]:
    """Write observed hashes for owner review without editing or blessing the manifest."""
    resolved_output = output.resolve()
    resolved_source = source_root.resolve()
    if resolved_output == resolved_source or resolved_output.is_relative_to(resolved_source):
        raise AudioBankError("fingerprint report may not touch the read-only Pizza source library")
    if output.exists() or output.is_symlink():
        raise AudioBankError(f"fingerprint report output exists: {output}")
    records = fingerprint_sources(manifest, source_root)
    report = {
        "schema": "kessel.pizza-audio-fingerprint-candidate-report",
        "version": 1,
        "selection_id": manifest["selection_id"],
        "selection_manifest_sha256": sha256_bytes(manifest_blob),
        "owner_private": True,
        "review_status": "candidate-only-not-manifest-pinned",
        "file_count": len(records),
        "files": [
            {
                "bytes": record["bytes"],
                "cue_id": record["cue_id"],
                "event": record["event"],
                "sha256": record["sha256"],
                "source_relative": record["source_relative"],
            }
            for record in records
        ],
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(canonical_json(report))
    return report


def probe_duration(ffprobe: str, path: Path) -> float:
    result = subprocess.run(
        [
            ffprobe,
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return float(result.stdout.strip())


def render_audition(
    bank: Path,
    manifest: dict[str, Any],
    manifest_blob: bytes,
    output_dir: Path,
    *,
    seconds_per_cue: float = 1.5,
) -> Path:
    receipt = validate_bank(bank, manifest, manifest_blob)
    ffmpeg = shutil.which("ffmpeg")
    ffprobe = shutil.which("ffprobe")
    if not ffmpeg or not ffprobe:
        raise AudioBankError("ffmpeg and ffprobe are required for the private audition")
    bank = bank.resolve()
    output_dir = output_dir.resolve()
    if output_dir == bank or output_dir.is_relative_to(bank):
        raise AudioBankError("audition output may not modify the exact runtime bank closure")
    if output_dir.exists():
        raise AudioBankError(f"audition output must be a new directory: {output_dir}")
    if not 0.25 <= seconds_per_cue <= 3.0:
        raise AudioBankError("seconds_per_cue must be in 0.25..=3.0")
    output_dir.mkdir(parents=True, exist_ok=False)
    master = output_dir / "pizza_audio_private_audition.wav"
    cues_csv = output_dir / "AUDITION_CUES.csv"
    try:
        with tempfile.TemporaryDirectory(prefix="kessel-pizza-audio-") as temporary:
            temporary_root = Path(temporary)
            silence = temporary_root / "silence.wav"
            subprocess.run(
                [
                    ffmpeg,
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=44100:cl=stereo",
                    "-t",
                    "0.3",
                    "-y",
                    str(silence),
                ],
                check=True,
            )
            concat_entries: list[Path] = []
            cue_rows: list[dict[str, Any]] = []
            cursor = 0.0
            for index, record in enumerate(receipt["files"], start=1):
                source = bank.joinpath(*PurePosixPath(record["runtime_file"]).parts)
                duration = min(seconds_per_cue, probe_duration(ffprobe, source))
                segment = temporary_root / f"cue-{index:03d}.wav"
                fade_start = max(0.0, duration - min(0.12, duration * 0.25))
                subprocess.run(
                    [
                        ffmpeg,
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-i",
                        str(source),
                        "-t",
                        f"{duration:.6f}",
                        "-af",
                        f"volume={record['gain']},afade=t=out:st={fade_start:.6f}:d={duration - fade_start:.6f}",
                        "-ar",
                        "44100",
                        "-ac",
                        "2",
                        "-y",
                        str(segment),
                    ],
                    check=True,
                )
                rendered_duration = probe_duration(ffprobe, segment)
                cue_rows.append(
                    {
                        "index": index,
                        "start_seconds": f"{cursor:.6f}",
                        "end_seconds": f"{cursor + rendered_duration:.6f}",
                        "event": record["event"],
                        "cue_id": record["cue_id"],
                        "runtime_file": record["runtime_file"],
                        "runtime_gain": record["gain"],
                    }
                )
                concat_entries.extend((segment, silence))
                cursor += rendered_duration + 0.3
            concat_file = temporary_root / "concat.txt"
            concat_file.write_text(
                "".join(f"file '{path.as_posix()}'\n" for path in concat_entries),
                encoding="utf-8",
            )
            subprocess.run(
                [
                    ffmpeg,
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                    str(concat_file),
                    "-c",
                    "copy",
                    "-y",
                    str(master),
                ],
                check=True,
            )
        with cues_csv.open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=list(cue_rows[0]))
            writer.writeheader()
            writer.writerows(cue_rows)
        (output_dir / "README_PRIVATE.txt").write_text(
            "OWNER-PRIVATE PIZZA DOGGY CANDIDATE AUDITION\n"
            "Do not commit, publish, or redistribute this review render or its source cues.\n"
            "AUDITION_CUES.csv labels each segment and records the bounded runtime layer gain.\n",
            encoding="utf-8",
        )
    except BaseException:
        shutil.rmtree(output_dir, ignore_errors=True)
        raise
    return master


def repo_root_from(value: str) -> Path:
    root = Path(value).resolve()
    if not (root / "Cargo.toml").is_file() or not (root / "src").is_dir():
        raise AudioBankError(f"not a Kessel repository root: {root}")
    return root


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--repo-root", default=".")
    result.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("audit", help="validate tracked policy without requiring private sources")
    candidates = commands.add_parser("candidates", help="write a read-only private filename shortlist")
    candidates.add_argument("--output", type=Path)
    fingerprints = commands.add_parser(
        "fingerprints", help="write observed selected-source hashes for explicit owner review"
    )
    fingerprints.add_argument("--source-root")
    fingerprints.add_argument("--output", required=True, type=Path)
    validate = commands.add_parser("validate-sources", help="verify the exact selected source closure")
    validate.add_argument("--source-root")
    cook = commands.add_parser("cook", help="copy the exact selected OGG closure to a fresh bank")
    cook.add_argument("--source-root")
    cook.add_argument("--output", required=True, type=Path)
    cook.add_argument("--ack-license", required=True)
    bank = commands.add_parser("validate-bank", help="verify a cooked bank and receipt")
    bank.add_argument("--bank", required=True, type=Path)
    audition = commands.add_parser("audition", help="render a short owner-private labeled WAV review")
    audition.add_argument("--bank", required=True, type=Path)
    audition.add_argument("--output", required=True, type=Path)
    audition.add_argument("--seconds-per-cue", type=float, default=1.5)
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        repo_root = repo_root_from(args.repo_root)
        manifest_path = Path(args.manifest)
        if not manifest_path.is_absolute():
            manifest_path = repo_root / manifest_path
        manifest, manifest_blob = load_manifest(manifest_path)
        selected = selected_variants(manifest)
        unresolved = len(manifest["candidate_review"])
        if args.command == "audit":
            pending = len(pending_fingerprint_cues(manifest))
            print(
                f"[pizza-audio] POLICY VALID: {len(selected)} exact OGGs selected, "
                f"{unresolved} unresolved candidate groups, {pending} pending source fingerprints; "
                "no private source access"
            )
        elif args.command == "candidates":
            root = resolve_source_root()
            output = args.output
            if output is not None and not output.is_absolute():
                output = repo_root / output
            report = write_candidate_report(manifest, root, output)
            total = sum(len(candidate["matches"]) for candidate in report["candidates"])
            print(f"[pizza-audio] PRIVATE CANDIDATES: {total} filename matches")
            for candidate in report["candidates"]:
                print(f"  {candidate['candidate_id']}: {len(candidate['matches'])}")
        elif args.command == "fingerprints":
            root = resolve_source_root(args.source_root)
            output = private_fingerprint_report_path(repo_root, args.output)
            report = write_fingerprint_report(manifest, manifest_blob, root, output)
            print(
                f"[pizza-audio] PRIVATE FINGERPRINT CANDIDATE: {report['file_count']} exact OGGs "
                f"-> {output}; tracked manifest unchanged"
            )
        elif args.command == "validate-sources":
            root = resolve_source_root(args.source_root)
            records = validate_sources(manifest, root)
            print(
                f"[pizza-audio] SOURCES VALID: {len(records)} exact OGGs, "
                f"{sum(record['bytes'] for record in records)} bytes"
            )
        elif args.command == "cook":
            root = resolve_source_root(args.source_root)
            output = args.output if args.output.is_absolute() else repo_root / args.output
            receipt = cook_bank(
                manifest,
                manifest_blob,
                root,
                output,
                license_ack=args.ack_license,
            )
            print(
                f"[pizza-audio] COOKED OWNER-PRIVATE BANK: {receipt['file_count']} OGGs, "
                f"{receipt['total_bytes']} bytes -> {output}"
            )
        elif args.command == "validate-bank":
            bank = args.bank if args.bank.is_absolute() else repo_root / args.bank
            receipt = validate_bank(bank, manifest, manifest_blob)
            print(f"[pizza-audio] BANK VALID: {receipt['file_count']} exact OGGs")
        else:
            bank = args.bank if args.bank.is_absolute() else repo_root / args.bank
            output = args.output if args.output.is_absolute() else repo_root / args.output
            master = render_audition(
                bank,
                manifest,
                manifest_blob,
                output,
                seconds_per_cue=args.seconds_per_cue,
            )
            print(f"[pizza-audio] PRIVATE AUDITION: {master}")
        return 0
    except (AudioBankError, OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"[pizza-audio] ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
