#!/usr/bin/env python3
"""CPU-only tests for the deterministic non-GLB runtime payload."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import struct
import sys
import tempfile
import unittest

import runtime_payload as payload


REPO_ROOT = Path(__file__).resolve().parent.parent


class RuntimePayloadTests(unittest.TestCase):
    def test_tracked_catalog_matches_manifest_derived_closure(self) -> None:
        catalog = payload.validate_catalog(
            REPO_ROOT, REPO_ROOT / payload.DEFAULT_CATALOG
        )
        self.assertEqual(catalog["file_count"], 21)
        self.assertEqual(
            catalog["total_bytes"],
            sum(entry["expected"]["bytes"] for entry in catalog["files"]),
        )
        runtime_paths = [entry["runtime_path"] for entry in catalog["files"]]
        self.assertEqual(runtime_paths, sorted(runtime_paths))
        self.assertEqual(len(runtime_paths), len(set(runtime_paths)))
        self.assertIn("terrain/stalingrad_100km_height16.png", runtime_paths)
        self.assertIn("THIRD_PARTY_DATA/CREDITS.txt", runtime_paths)
        self.assertIn("fonts/OFL-1.1.txt", runtime_paths)
        self.assertIn("fonts/LICENSES.md", runtime_paths)
        self.assertIn("THIRD_PARTY_NOTICES/PIZZA-DOGGY.txt", runtime_paths)
        self.assertIn("THIRD_PARTY_NOTICES/RUST-DEPENDENCIES.txt", runtime_paths)

    def test_closure_has_no_authoring_preview_audio_or_pizza_payload(self) -> None:
        candidates = payload.discover_closure(REPO_ROOT)
        sources = [candidate.source.casefold() for candidate in candidates]
        self.assertFalse(any("assets/pizza/" in source for source in sources))
        self.assertFalse(any("preview" in source or "thumb" in source for source in sources))
        self.assertFalse(any(source.endswith((".wav", ".ogg", ".blend", ".fbx")) for source in sources))
        self.assertFalse(any("manifest.json" in source for source in sources))

    def test_path_escape_and_authoring_formats_are_rejected(self) -> None:
        with self.assertRaises(payload.PayloadError):
            payload.safe_relative("../secret.json", "test")
        with self.assertRaises(payload.PayloadError):
            payload.reject_authoring_path(
                payload.Candidate(
                    "assets/raw/tank.blend",
                    "raw/tank.blend",
                    "bad",
                    ("test",),
                )
            )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            assets = root / "assets"
            assets.mkdir()
            internal = assets / "internal"
            internal.mkdir()
            (assets / "catalog").symlink_to(internal, target_is_directory=True)
            with self.assertRaises(payload.PayloadError):
                payload.destination(assets, "catalog/runtime_payload.v1.json")

    def test_dangling_symlink_outputs_are_refused(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dangling = root / "payload-output"
            dangling.symlink_to(root / "missing-target", target_is_directory=True)
            with self.assertRaises(payload.PayloadError):
                payload.cook(
                    REPO_ROOT,
                    REPO_ROOT / payload.DEFAULT_CATALOG,
                    dangling,
                    payload.OWNER_DEV_SCOPE,
                )
            self.assertFalse((root / "missing-target").exists())

    def test_macos_packager_has_explicit_scope_and_portable_checksum_guards(self) -> None:
        script = (REPO_ROOT / "tools/package_macos.sh").read_text(encoding="utf-8")
        self.assertIn('--release-scope SCOPE', script)
        self.assertIn('RELEASE_SCOPE="owner-dev"', script)
        self.assertIn('KesselReleaseScope', script)
        self.assertIn('[ "$RELEASE_SCOPE" != "public" ]', script)
        self.assertIn('[ -L "$OUT_DIR" ]', script)
        self.assertIn('[ ! -L "$APP_OUT" ]', script)
        self.assertIn('[ ! -L "$DMG_OUT" ]', script)
        self.assertIn('[ ! -L "$DMG_SHA_OUT" ]', script)
        self.assertIn('/usr/bin/shasum -a 256 "$DMG_BASENAME"', script)
        self.assertIn('/usr/bin/shasum -a 256 -c "$DMG_BASENAME.sha256"', script)
        self.assertNotIn('/usr/bin/shasum -a 256 "$DMG_OUT"', script)

    @unittest.skipUnless(sys.platform == "darwin", "macOS packager behavior")
    def test_macos_packager_refuses_dangling_output_symlinks(self) -> None:
        script = REPO_ROOT / "tools/package_macos.sh"

        def run(out_dir: Path, no_dmg: bool = False) -> subprocess.CompletedProcess[str]:
            command = [
                "/bin/bash",
                str(script),
                "--out-dir",
                str(out_dir),
                "--dry-run",
                "--offline",
            ]
            if no_dmg:
                command.append("--no-dmg")
            return subprocess.run(
                command,
                cwd=REPO_ROOT,
                check=False,
                capture_output=True,
                text=True,
            )

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            actual = root / "actual"
            actual.mkdir()
            symlinked_out = root / "out-link"
            symlinked_out.symlink_to(actual, target_is_directory=True)
            result = run(symlinked_out, no_dmg=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("output directory may not be a symlink", result.stderr)

            app_out = root / "app-out"
            app_out.mkdir()
            (app_out / "Kessel.app").symlink_to(root / "missing-app", target_is_directory=True)
            result = run(app_out, no_dmg=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("existing app or symlink", result.stderr)

            dmg_out = root / "dmg-out"
            dmg_out.mkdir()
            (dmg_out / "Kessel-0.1.0-arm64.dmg").symlink_to(root / "missing-dmg")
            result = run(dmg_out)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("existing DMG or symlink", result.stderr)

            checksum_out = root / "checksum-out"
            checksum_out.mkdir()
            (checksum_out / "Kessel-0.1.0-arm64.dmg.sha256").symlink_to(
                root / "missing-checksum"
            )
            result = run(checksum_out)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("existing checksum or symlink", result.stderr)

    def test_public_release_is_blocked_but_owner_dev_mode_is_allowed(self) -> None:
        catalog = payload.validate_catalog(
            REPO_ROOT, REPO_ROOT / payload.DEFAULT_CATALOG
        )
        blockers = payload.redistribution_blockers(REPO_ROOT, catalog)
        by_code = {blocker["code"]: blocker for blocker in blockers}
        self.assertEqual(by_code["unresolved-non-pizza-model-provenance"]["count"], 16)
        self.assertEqual(
            by_code["mapzen-terrain-provenance-and-license-unverified"]["count"], 1
        )
        self.assertEqual(by_code["rust-dependency-license-notices-unreviewed"]["count"], 330)
        self.assertEqual(by_code["pizza-vendor-terms-recheck-required"]["count"], 1)
        self.assertNotIn("incomplete-terrain-source-metadata", by_code)
        payload.validate_release_mode(blockers, payload.OWNER_DEV_SCOPE)
        with self.assertRaises(payload.PayloadError):
            payload.validate_release_mode(blockers, payload.PUBLIC_SCOPE)

    def test_terrain_derivatives_have_explicit_non_project_provenance(self) -> None:
        catalog = payload.validate_catalog(
            REPO_ROOT, REPO_ROOT / payload.DEFAULT_CATALOG
        )
        terrain = [entry for entry in catalog["files"] if entry["kind"].startswith("terrain-")]
        self.assertTrue(terrain)
        for entry in terrain:
            self.assertNotEqual(entry["license"], payload.PROJECT_LICENSE)
            self.assertIn(entry["data_source"], catalog["data_sources"])

        self.assertEqual(
            catalog["data_sources"]["sentinel-2-tci-act2"]["source_product_ids"],
            ["S2C_33UVU_20250402_0_L2A"],
        )
        self.assertEqual(
            catalog["data_sources"]["cop-dem-glo30-act3"]["source_product_ids"],
            ["Copernicus_DSM_COG_10_N52_00_E013_00_DEM"],
        )
        self.assertEqual(
            catalog["data_sources"]["mapzen-terrain-tiles-stalingrad"]["license"],
            payload.MAPZEN_TERRAIN_LICENSE,
        )
        self.assertEqual(
            catalog["licenses"][payload.MAPZEN_TERRAIN_LICENSE][
                "public_redistribution_status"
            ],
            "blocked-provenance-and-license-unverified",
        )

    def test_direct_non_glb_literals_are_fully_classified(self) -> None:
        inventory = payload.discover_direct_reference_inventory(REPO_ROOT)
        self.assertEqual(len(inventory), len(payload.REVIEWED_DIRECT_REFERENCES))
        self.assertTrue(any(item["disposition"] == "required-runtime" for item in inventory))
        self.assertTrue(any(item["disposition"].startswith("excluded-") for item in inventory))

    def test_cargo_lock_inventory_is_deterministic_and_honestly_unreviewed(self) -> None:
        packages = payload.external_cargo_packages(REPO_ROOT)
        notice = payload.render_cargo_dependency_inventory(REPO_ROOT)
        self.assertEqual(len(packages), 330)
        self.assertEqual(notice, payload.render_cargo_dependency_inventory(REPO_ROOT))
        text = notice.decode("utf-8")
        self.assertIn("External locked packages: 330", text)
        self.assertIn(payload.CARGO_LICENSE_REVIEW_STATUS, text)
        self.assertIn("not a completed licence/notice audit", text)

    def test_npy_validator_checks_manifest_dimensions_and_payload_length(self) -> None:
        header = str(
            {
                "descr": "<f4",
                "fortran_order": False,
                "shape": (2, 2),
            }
        ).encode("ascii")
        header += b" " * ((64 - ((10 + len(header) + 1) % 64)) % 64) + b"\n"
        blob = b"\x93NUMPY\x01\x00" + struct.pack("<H", len(header)) + header
        blob += struct.pack("<4f", 0.0, 1.0, 2.0, 3.0)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "heights.npy"
            path.write_bytes(blob)
            payload.validate_npy(path, 2, "fixture")
            with self.assertRaises(payload.PayloadError):
                payload.validate_npy(path, 3, "fixture")
            path.write_bytes(blob[:-1])
            with self.assertRaises(payload.PayloadError):
                payload.validate_npy(path, 2, "fixture")

    def test_runtime_verifier_recomputes_bytes_hashes_and_canonical_hash_list(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data_path = root / "worldgen/act1/data.json"
            data_path.parent.mkdir(parents=True)
            data_path.write_bytes(b"{}\n")
            digest = payload.sha256_file(data_path)

            catalog = {
                "data_sources": {},
                "direct_resource_inventory": {},
                "files": [
                    {
                        "expected": {"bytes": 3, "sha256": digest},
                        "kind": "fixture",
                        "license": payload.PROJECT_LICENSE,
                        "required_by": ["test"],
                        "runtime_path": "worldgen/act1/data.json",
                        "source": "fixture.json",
                    }
                ],
                "licenses": {},
                "schema": payload.CATALOG_SCHEMA,
                "software_dependencies": {},
                "third_party_asset_policy": {},
                "version": payload.CATALOG_VERSION,
            }
            catalog_path = root / "catalog/runtime_payload.v1.json"
            catalog_path.parent.mkdir(parents=True)
            catalog_path.write_bytes(payload.canonical_json(catalog))
            catalog_digest = payload.sha256_file(catalog_path)

            manifest = {
                "blockers": [],
                "catalog_sha256": catalog_digest,
                "data_sources": {},
                "direct_resource_inventory": {},
                "file_count": 1,
                "files": [
                    {
                        "bytes": 3,
                        "kind": "fixture",
                        "license": payload.PROJECT_LICENSE,
                        "path": "worldgen/act1/data.json",
                        "required_by": ["test"],
                        "sha256": digest,
                        "source": "fixture.json",
                    }
                ],
                "licenses": {},
                "public_release_ready": True,
                "release_scope": payload.OWNER_DEV_SCOPE,
                "schema": payload.RUNTIME_SCHEMA,
                "software_dependencies": {},
                "third_party_asset_policy": {},
                "total_bytes": 3,
                "version": payload.RUNTIME_VERSION,
            }
            (root / "runtime-payload.manifest.json").write_bytes(
                payload.canonical_json(manifest)
            )
            (root / "RUNTIME-PAYLOAD.sha256").write_text(
                f"{catalog_digest}  catalog/runtime_payload.v1.json\n"
                f"{digest}  worldgen/act1/data.json\n",
                encoding="utf-8",
            )
            payload.verify_runtime(root)
            data_path.write_bytes(b"[]\n")
            with self.assertRaises(payload.PayloadError):
                payload.verify_runtime(root)


if __name__ == "__main__":
    unittest.main()
