#!/usr/bin/env python3
"""Offline regression suite for the owner-private Pizza Doggy audio cooker."""

from __future__ import annotations

import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import pizza_audio_bank as audio_bank


REPO_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = REPO_ROOT / audio_bank.DEFAULT_MANIFEST


def tracked_manifest() -> tuple[dict, bytes]:
    return audio_bank.load_manifest(MANIFEST_PATH)


def one_cue_manifest() -> tuple[dict, bytes, str, bytes]:
    manifest, _blob = tracked_manifest()
    manifest = copy.deepcopy(manifest)
    chosen = next(
        mapping
        for mapping in manifest["mappings"]
        if mapping["event"] == "luger_shot"
    )
    chosen["variants"] = chosen["variants"][:1]
    manifest["mappings"] = [chosen]
    manifest["candidate_review"] = []
    source_relative = chosen["variants"][0]["source_relative"]
    source_bytes = b"OggS\x00owner-private-fixture\x01\x02"
    chosen["variants"][0]["expected_bytes"] = len(source_bytes)
    chosen["variants"][0]["expected_sha256"] = audio_bank.sha256_bytes(source_bytes)
    audio_bank.validate_manifest(manifest)
    return manifest, audio_bank.canonical_json(manifest), source_relative, source_bytes


class PizzaAudioBankTests(unittest.TestCase):
    def combined_partition_fixture(self) -> tuple[dict, bytes, Path, dict, Path, Path, bytes]:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        manifest, blob, relative, data = one_cue_manifest()
        source_root = root / "source"
        source = source_root / relative
        source.parent.mkdir(parents=True)
        source.write_bytes(data)
        bank = root / "bank"
        receipt = audio_bank.cook_bank(manifest, blob, source_root, bank,
                                      license_ack=audio_bank.LICENSE_ACK)
        partition = bank / audio_bank.FEEDBACK_PARTITION
        partition.mkdir()
        feedback_bytes = b"OggS independently pinned feedback fixture"
        digest = audio_bank.sha256_bytes(feedback_bytes)
        feedback_source = partition / f"{digest}.ogg"
        feedback_source.write_bytes(feedback_bytes)
        selection = dict(schema="kessel.audio-feedback", version=1, owner_private=True,
                         license="pizzadoggy-local-purchase", entries=[dict(
                             key="trigger", sha256=digest, bytes=len(feedback_bytes),
                             path=f"assets/sounds/pizza_private/{audio_bank.FEEDBACK_PARTITION}/{digest}.ogg")])
        trusted_manifest = root / "trusted-feedback.json"
        trusted_manifest.write_bytes(audio_bank.canonical_json(selection))
        (partition / audio_bank.RECEIPT_NAME).write_bytes(trusted_manifest.read_bytes())
        patcher = mock.patch.object(audio_bank, "FEEDBACK_MANIFEST_PATH", trusted_manifest)
        patcher.start()
        self.addCleanup(patcher.stop)
        return manifest, blob, bank, receipt, trusted_manifest, feedback_source, feedback_bytes

    def test_verified_partition_preserves_v1_receipt_and_never_adds_feedback_cues(self) -> None:
        manifest, blob, bank, receipt, _trusted, _source, _data = self.combined_partition_fixture()
        before = {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob("*") if p.is_file()}
        self.assertEqual(audio_bank.validate_bank(bank, manifest, blob), receipt)
        self.assertEqual(receipt["file_count"], 1)
        self.assertTrue(all(r["runtime_file"].startswith("cues/") for r in receipt["files"]))
        after = {str(p.relative_to(bank)): p.read_bytes() for p in bank.rglob("*") if p.is_file()}
        self.assertEqual(after, before)

    def test_verified_partition_rejects_unknown_nested_and_missing_files(self) -> None:
        manifest, blob, bank, _receipt, _trusted, source, data = self.combined_partition_fixture()
        for parent in (bank / "unrelated_bank", source.parent / "nested"):
            parent.mkdir()
            extra = parent / "extra.ogg"
            extra.write_bytes(b"OggSunreviewed")
            with self.assertRaisesRegex(audio_bank.AudioBankError, "closure mismatch"):
                audio_bank.validate_bank(bank, manifest, blob)
            extra.unlink()
            parent.rmdir()
        source.unlink()
        with self.assertRaisesRegex(audio_bank.AudioBankError, "closure mismatch"):
            audio_bank.validate_bank(bank, manifest, blob)
        source.write_bytes(data)
        (source.parent / audio_bank.RECEIPT_NAME).unlink()
        with self.assertRaisesRegex(audio_bank.AudioBankError, "selection/receipt"):
            audio_bank.validate_bank(bank, manifest, blob)

    def test_verified_partition_rejects_tampered_source_hash_size_and_receipt(self) -> None:
        manifest, blob, bank, _receipt, trusted, source, data = self.combined_partition_fixture()
        for changed in (b"x" + data[1:], data[:-1]):
            source.write_bytes(changed)
            with self.assertRaisesRegex(audio_bank.AudioBankError, "source bytes do not match"):
                audio_bank.validate_bank(bank, manifest, blob)
        source.write_bytes(data)
        (source.parent / audio_bank.RECEIPT_NAME).write_bytes(trusted.read_bytes() + b"\n")
        with self.assertRaisesRegex(audio_bank.AudioBankError, "receipt differs"):
            audio_bank.validate_bank(bank, manifest, blob)

    def test_verified_partition_rejects_paths_outside_pinned_root(self) -> None:
        manifest, blob, bank, _receipt, trusted, source, _data = self.combined_partition_fixture()
        changed = json.loads(trusted.read_bytes())
        changed["entries"][0]["path"] = "../unreviewed.ogg"
        trusted.write_bytes(audio_bank.canonical_json(changed))
        (source.parent / audio_bank.RECEIPT_NAME).write_bytes(trusted.read_bytes())
        with self.assertRaisesRegex(audio_bank.AudioBankError, "pinned bank path"):
            audio_bank.validate_bank(bank, manifest, blob)

    def test_verified_partition_rejects_source_receipt_and_directory_symlinks(self) -> None:
        manifest, blob, bank, _receipt, trusted, source, data = self.combined_partition_fixture()
        outside = bank.parent / "same-bytes.ogg"
        outside.write_bytes(data)
        source.unlink()
        source.symlink_to(outside)
        with self.assertRaisesRegex(audio_bank.AudioBankError, "symlinks"):
            audio_bank.validate_bank(bank, manifest, blob)
        source.unlink()
        source.write_bytes(data)
        receipt = source.parent / audio_bank.RECEIPT_NAME
        receipt.unlink()
        receipt.symlink_to(trusted)
        with self.assertRaisesRegex(audio_bank.AudioBankError, "symlinks"):
            audio_bank.validate_bank(bank, manifest, blob)
        receipt.unlink()
        receipt.write_bytes(trusted.read_bytes())
        saved = bank.parent / "saved-partition"
        source.parent.rename(saved)
        (bank / audio_bank.FEEDBACK_PARTITION).symlink_to(saved, target_is_directory=True)
        with self.assertRaisesRegex(audio_bank.AudioBankError, "symlinks"):
            audio_bank.validate_bank(bank, manifest, blob)

    def test_tracked_selection_is_small_exact_and_never_targets_protected_cores(self) -> None:
        manifest, _blob = tracked_manifest()
        selected = audio_bank.selected_variants(manifest)
        self.assertEqual(len(selected), 11)
        self.assertEqual(len(manifest["candidate_review"]), 3)
        self.assertEqual(len({item["cue_id"] for item in selected}), len(selected))
        self.assertEqual(len({item["source_relative"] for item in selected}), len(selected))
        self.assertTrue({item["event"] for item in selected} <= audio_bank.ALLOWED_EVENTS)
        self.assertFalse({item["event"] for item in selected} & audio_bank.PROTECTED_EVENTS)
        self.assertFalse(any("ROT - Horror" in item["source_relative"] for item in selected))
        self.assertTrue(
            all(item["source_relative"].casefold().endswith(".ogg") for item in selected)
        )
        self.assertTrue(
            all(isinstance(item["expected_bytes"], int) and item["expected_bytes"] > 0 for item in selected)
        )
        self.assertTrue(
            all(audio_bank.SHA256_RE.fullmatch(item["expected_sha256"]) for item in selected)
        )
        self.assertTrue(
            all(
                item["source_relative"].split("/", 1)[0] in audio_bank.ALLOWED_PACKS
                for item in selected
            )
        )

    def test_policy_audit_needs_no_private_source_checkout(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            manifest, _blob = tracked_manifest()
            audio_bank.validate_manifest(manifest)
            with self.assertRaisesRegex(
                audio_bank.AudioBankError, audio_bank.SOURCE_ROOT_ENV
            ):
                audio_bank.resolve_source_root()

    def test_pending_fingerprints_fail_validation_and_cook_closed(self) -> None:
        manifest, _blob = tracked_manifest()
        manifest = copy.deepcopy(manifest)
        for mapping in manifest["mappings"]:
            for variant in mapping["variants"]:
                variant["expected_bytes"] = None
                variant["expected_sha256"] = None
        blob = audio_bank.canonical_json(manifest)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source_root.mkdir()
            with self.assertRaisesRegex(audio_bank.AudioBankError, "pending owner review"):
                audio_bank.validate_sources(manifest, source_root)
            with self.assertRaisesRegex(audio_bank.AudioBankError, "pending owner review"):
                audio_bank.cook_bank(
                    manifest,
                    blob,
                    source_root,
                    root / "bank",
                    license_ack=audio_bank.LICENSE_ACK,
                )

    def test_fingerprint_report_is_candidate_only_and_does_not_mutate_manifest(self) -> None:
        manifest, blob = tracked_manifest()
        selected = audio_bank.selected_variants(manifest)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            for index, cue in enumerate(selected, start=1):
                source = source_root.joinpath(*Path(cue["source_relative"]).parts)
                source.parent.mkdir(parents=True, exist_ok=True)
                payload = b"OggS" + index.to_bytes(2, "little") + cue["cue_id"].encode()
                source.write_bytes(payload)
            output = root / "review" / "fingerprints.private.json"
            before = MANIFEST_PATH.read_bytes()
            report = audio_bank.write_fingerprint_report(
                manifest, blob, source_root, output
            )
            self.assertEqual(report["file_count"], 11)
            self.assertEqual(report["review_status"], "candidate-only-not-manifest-pinned")
            self.assertTrue(
                all(
                    item["bytes"] > 0
                    and audio_bank.SHA256_RE.fullmatch(item["sha256"])
                    for item in report["files"]
                )
            )
            self.assertEqual(output.read_bytes(), audio_bank.canonical_json(report))
            self.assertEqual(MANIFEST_PATH.read_bytes(), before)

    def test_fingerprint_cli_output_is_confined_to_gitignored_private_tree(self) -> None:
        accepted = audio_bank.private_fingerprint_report_path(
            REPO_ROOT, Path("artifacts/audio/fingerprints.private.json")
        )
        self.assertTrue(accepted.is_relative_to((REPO_ROOT / "artifacts/audio").resolve()))
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(audio_bank.AudioBankError, "gitignored artifacts/audio"):
                audio_bank.private_fingerprint_report_path(
                    REPO_ROOT, Path(temporary) / "unsafe.json"
                )

    def test_source_bytes_must_match_reviewed_manifest_fingerprint(self) -> None:
        manifest, _blob, source_relative, source_bytes = one_cue_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            source_root = Path(temporary) / "source"
            source = source_root.joinpath(*Path(source_relative).parts)
            source.parent.mkdir(parents=True)
            source.write_bytes(source_bytes + b"drift")
            with self.assertRaisesRegex(audio_bank.AudioBankError, "reviewed fingerprint"):
                audio_bank.validate_sources(manifest, source_root)

    def test_cook_copies_only_exact_selected_ogg_and_writes_bound_receipt(self) -> None:
        manifest, blob, source_relative, source_bytes = one_cue_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source = source_root.joinpath(*Path(source_relative).parts)
            source.parent.mkdir(parents=True)
            source.write_bytes(source_bytes)
            unselected = source.with_name("unselected.ogg")
            unselected.write_bytes(b"OggSunselected")
            output = root / "bank"

            receipt = audio_bank.cook_bank(
                manifest,
                blob,
                source_root,
                output,
                license_ack=audio_bank.LICENSE_ACK,
            )

            self.assertEqual(receipt["file_count"], 1)
            self.assertEqual(receipt["total_bytes"], len(source_bytes))
            self.assertEqual(
                receipt["selection_manifest_sha256"], audio_bank.sha256_bytes(blob)
            )
            self.assertEqual(receipt["files"][0]["sha256"], audio_bank.sha256_bytes(source_bytes))
            self.assertEqual(
                {
                    path.relative_to(output).as_posix()
                    for path in output.rglob("*")
                    if path.is_file()
                },
                {
                    audio_bank.RECEIPT_NAME,
                    "cues/rust-blood.old-pistol.1.ogg",
                },
            )
            self.assertEqual(unselected.read_bytes(), b"OggSunselected")
            self.assertEqual(audio_bank.validate_bank(output, manifest, blob), receipt)

    def test_tracked_eleven_cue_selection_cooks_despite_unresolved_discovery_groups(self) -> None:
        manifest, _blob = tracked_manifest()
        manifest = copy.deepcopy(manifest)
        selected = audio_bank.selected_variants(manifest)
        self.assertEqual(len(selected), 11)
        self.assertTrue(manifest["candidate_review"])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            fixture_bytes: dict[str, bytes] = {}
            for index, cue in enumerate(selected, start=1):
                source = source_root.joinpath(*Path(cue["source_relative"]).parts)
                source.parent.mkdir(parents=True, exist_ok=True)
                payload = b"OggS" + index.to_bytes(2, "little") + cue["cue_id"].encode()
                source.write_bytes(payload)
                fixture_bytes[cue["cue_id"]] = payload
            for mapping in manifest["mappings"]:
                for variant in mapping["variants"]:
                    payload = fixture_bytes[variant["cue_id"]]
                    variant["expected_bytes"] = len(payload)
                    variant["expected_sha256"] = audio_bank.sha256_bytes(payload)
            audio_bank.validate_manifest(manifest)
            blob = audio_bank.canonical_json(manifest)
            # A discovery-only candidate may exist beside the exact closure; it must never copy.
            candidate = source_root / "Echoes - Audio Super Kit v2.0.1" / "unreviewed_button.ogg"
            candidate.parent.mkdir(parents=True, exist_ok=True)
            candidate.write_bytes(b"OggSunreviewed")

            output = root / "bank"
            receipt = audio_bank.cook_bank(
                manifest,
                blob,
                source_root,
                output,
                license_ack=audio_bank.LICENSE_ACK,
            )
            self.assertEqual(receipt["file_count"], 11)
            self.assertEqual(
                len([path for path in (output / "cues").iterdir() if path.is_file()]),
                11,
            )
            self.assertNotIn("unreviewed_button", json.dumps(receipt, sort_keys=True))
            audio_bank.validate_bank(output, manifest, blob)

    def test_cook_requires_explicit_license_ack_and_fresh_output(self) -> None:
        manifest, blob, source_relative, source_bytes = one_cue_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source = source_root.joinpath(*Path(source_relative).parts)
            source.parent.mkdir(parents=True)
            source.write_bytes(source_bytes)
            with self.assertRaisesRegex(audio_bank.AudioBankError, "ack-license"):
                audio_bank.cook_bank(
                    manifest,
                    blob,
                    source_root,
                    root / "bank",
                    license_ack="yes",
                )
            existing = root / "existing"
            existing.mkdir()
            with self.assertRaisesRegex(audio_bank.AudioBankError, "refusing to overwrite"):
                audio_bank.cook_bank(
                    manifest,
                    blob,
                    source_root,
                    existing,
                    license_ack=audio_bank.LICENSE_ACK,
                )
            with self.assertRaisesRegex(audio_bank.AudioBankError, "read-only Pizza source"):
                audio_bank.cook_bank(
                    manifest,
                    blob,
                    source_root,
                    source_root / "cooked-bank",
                    license_ack=audio_bank.LICENSE_ACK,
                )

    def test_source_and_receipt_path_traversal_fail_closed(self) -> None:
        manifest, blob, _source_relative, _source_bytes = one_cue_manifest()
        escaped = copy.deepcopy(manifest)
        escaped["mappings"][0]["variants"][0]["source_relative"] = "../escape.ogg"
        with self.assertRaisesRegex(audio_bank.AudioBankError, "canonical relative path"):
            audio_bank.validate_manifest(escaped)

        receipt = audio_bank.receipt_for(
            manifest,
            blob,
            [
                {
                    **audio_bank.selected_variants(manifest)[0],
                    "bytes": 4,
                    "sha256": audio_bank.sha256_bytes(b"test"),
                }
            ],
        )
        receipt["files"][0]["runtime_file"] = "../escape.ogg"
        with self.assertRaises(audio_bank.AudioBankError):
            audio_bank.validate_receipt(receipt, manifest, blob)

    def test_receipt_hash_and_size_must_match_tracked_fingerprint(self) -> None:
        manifest, blob, _source_relative, source_bytes = one_cue_manifest()
        selected = audio_bank.selected_variants(manifest)[0]
        receipt = audio_bank.receipt_for(
            manifest,
            blob,
            [
                {
                    **selected,
                    "bytes": len(source_bytes),
                    "sha256": audio_bank.sha256_bytes(source_bytes),
                }
            ],
        )
        receipt["files"][0]["sha256"] = audio_bank.sha256_bytes(b"other")
        with self.assertRaisesRegex(audio_bank.AudioBankError, "tracked selection"):
            audio_bank.validate_receipt(receipt, manifest, blob)

    def test_source_symlink_may_not_escape_selected_root(self) -> None:
        manifest, _blob, source_relative, source_bytes = one_cue_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            outside = root / "outside.ogg"
            outside.write_bytes(source_bytes)
            source_root = root / "source"
            source = source_root.joinpath(*Path(source_relative).parts)
            source.parent.mkdir(parents=True)
            source.symlink_to(outside)
            with self.assertRaisesRegex(audio_bank.AudioBankError, "escapes"):
                audio_bank.validate_sources(manifest, source_root)

    def test_gains_are_positive_finite_and_bounded_per_event(self) -> None:
        manifest, _blob = tracked_manifest()
        for bad in (0.0, -0.1, float("nan"), float("inf"), 0.201):
            changed = copy.deepcopy(manifest)
            next(mapping for mapping in changed["mappings"] if mapping["variants"])[
                "variants"
            ][0]["gain"] = bad
            with self.subTest(gain=bad):
                with self.assertRaises(audio_bank.AudioBankError):
                    audio_bank.validate_manifest(changed)

    def test_extra_or_modified_bank_file_invalidates_closure(self) -> None:
        manifest, blob, source_relative, source_bytes = one_cue_manifest()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source = source_root.joinpath(*Path(source_relative).parts)
            source.parent.mkdir(parents=True)
            source.write_bytes(source_bytes)
            output = root / "bank"
            audio_bank.cook_bank(
                manifest,
                blob,
                source_root,
                output,
                license_ack=audio_bank.LICENSE_ACK,
            )
            extra = output / "cues" / "extra.ogg"
            extra.write_bytes(b"OggSextra")
            with self.assertRaisesRegex(audio_bank.AudioBankError, "closure mismatch"):
                audio_bank.validate_bank(output, manifest, blob)
            extra.unlink()
            cooked = output / "cues" / "rust-blood.old-pistol.1.ogg"
            cooked.write_bytes(b"OggSmodified")
            with self.assertRaisesRegex(audio_bank.AudioBankError, "do not match receipt"):
                audio_bank.validate_bank(output, manifest, blob)
            cooked.write_bytes(source_bytes)
            symlink = output / "unreviewed-directory"
            symlink.symlink_to(output / "cues", target_is_directory=True)
            with self.assertRaisesRegex(audio_bank.AudioBankError, "may not contain symlinks"):
                audio_bank.validate_bank(output, manifest, blob)

    def test_owner_private_bank_is_ignored_and_absent_from_package_lock(self) -> None:
        ignored = subprocess.run(
            ["git", "check-ignore", "assets/sounds/pizza_private/cues/private.ogg"],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(ignored.returncode, 0, ignored.stderr)
        payload = json.loads((REPO_ROOT / "catalog/runtime_payload.v1.json").read_text())
        paths = {
            item[field]
            for item in payload["files"]
            for field in ("source", "runtime_path")
        }
        self.assertFalse(any("pizza_private" in path for path in paths))
        self.assertTrue(
            any("recordings" in exclusion.casefold() for exclusion in payload["excludes"])
        )


if __name__ == "__main__":
    unittest.main()
