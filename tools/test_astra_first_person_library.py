import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import astra_first_person_library as library


class LibraryTests(unittest.TestCase):
    def approved(self, date="2026-09-05T01:00:00Z"):
        return {"scenario_id": "fp_driver", "video": "old.mp4", "video_sha256": "a" * 64,
                "capture_sha256": "b" * 64, "recorded_utc": date, "first_person": True,
                "streams": {"passed": True}, "outcomes_passed": True,
                "qa": {"visual_coverage": "pass", "audio_quality": "pass",
                       "covered_mechanics": ["drive"]}}

    def test_reviews_bind_to_both_actual_hashes_and_scenario(self):
        record = self.approved()
        entry = dict(record["qa"], scenario_id=record["scenario_id"],
                     video_sha256=record["video_sha256"], capture_sha256=record["capture_sha256"])
        qa = {"reviews": [entry]}
        self.assertEqual(library.bound_review(qa, record), entry)
        renamed = dict(record, video="renamed.mp4")
        self.assertEqual(library.bound_review(qa, renamed), entry)
        for key in ("video_sha256", "capture_sha256", "scenario_id"):
            changed = dict(record, **{key: "different"})
            self.assertIsNone(library.bound_review(qa, changed))

    def test_latest_passing_review_wins_over_new_unreviewed_or_failed(self):
        old = self.approved()
        latest = self.approved("2026-09-05T02:00:00Z")
        latest["qa"] = None
        self.assertGreater(library.preference(old), library.preference(latest))
        latest["qa"] = copy.deepcopy(old["qa"])
        self.assertGreater(library.preference(latest), library.preference(old))
        latest["outcomes_passed"] = False
        self.assertFalse(library.fully_reviewed(latest))
        self.assertGreater(library.preference(old), library.preference(latest))
        latest["outcomes_passed"] = True
        latest["qa"]["audio_quality"] = "fail"
        self.assertFalse(library.fully_reviewed(latest))
        self.assertGreater(library.preference(old), library.preference(latest))

    def test_empty_or_failed_checks_cannot_prove_coverage(self):
        for checks in (None, [], [{"passed": False}], [{}]):
            self.assertFalse(library.completed_outcome({"outcome_checks_passed": True,
                                                        "outcome_checks": checks}))
        self.assertTrue(library.completed_outcome({"outcome_checks_passed": True,
                                                   "outcome_checks": [{"passed": True}]}))

    def test_local_links_reject_external_paths_and_escape_filenames(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            with self.assertRaises(ValueError):
                library.resolve_local(root, "/etc/passwd")
            with self.assertRaises(ValueError):
                library.resolve_local(root, "../outside.mp4")
            output = root / "captures/astra/index.html"
            self.assertEqual(library.href(str(output.parent / 'clip <one>.mp4'), output, root),
                             "clip%20%3Cone%3E.mp4")

    def fixture(self, root):
        captures = root / "captures/astra"
        captures.mkdir(parents=True)
        (root / "assets/review/first_person").mkdir(parents=True)
        ledger_dir = root / "artifacts/astra-audit-20260905"
        ledger_dir.mkdir(parents=True)
        library.write_json(ledger_dir / "first-person-mechanic-coverage.json", {
            "mechanics": [{"id": "drive", "mechanic": "Drive", "scenario": "fp_driver"},
                          {"id": "air", "mechanic": "Dust", "scenario": "fp_driver"}]})
        spec = {"id": "fp_driver", "title": "Driver <test>", "seconds": 2, "mechanics": ["drive"]}
        library.write_json(root / "assets/review/first_person/driver.json", spec)
        run = root / "artifacts/astra-first-person-test"
        run.mkdir()
        library.write_json(run / "queue.json", {"schema": "kessel.first-person-review-queue.v1",
            "run_id": "test", "cases": [{"id": "fp_driver", "title": spec["title"],
                                            "status": "recording_failed"}]})
        video = captures / "mechanics_test_fp_driver.mp4"
        video.write_bytes(b"test-video-bytes")
        manifest = captures / "mechanics_test_fp_driver_manifest.json"
        library.write_json(manifest, {"schema": library.RECEIPT_SCHEMA, "scenario": spec,
            "video": str(video), "first_person": True, "encoded_resolution": [1920, 1080],
            "fps": 24, "outcome_checks_passed": True, "outcome_checks": [{"passed": True}]})
        return captures, manifest

    @patch.object(library, "inspect_streams", return_value={"passed": True,
                 "dimensions": [1920, 1080], "fps": 24, "duration_s": 2})
    def test_orphan_receipts_recovered_and_coverage_requires_explicit_scope(self, _probe):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            captures, manifest = self.fixture(root)
            qa_path = captures / "qa.json"
            library.add_review(root, qa_path, manifest, "pass", "pass", "pending",
                               "reviewer", "watched and listened", "Driver verified", ["drive"])
            index = library.aggregate(root, qa_path)
            self.assertEqual(index["summary"]["recordings"], 1)
            self.assertEqual(index["summary"]["reviewed_mechanics"], 1)
            self.assertEqual(index["coverage"][1]["status"], "recorded_unapproved")
            self.assertEqual(index["scenarios"][0]["recordings"][0]["queue_status"], "recording_failed")
            page = library.render_page(index, captures / "index.html", captures / "index.json")
            self.assertIn("Driver &lt;test&gt;", page)
            self.assertIn('controls playsinline preload="none"', page)
            self.assertNotIn("autoplay", page)
            self.assertNotIn("<audio", page)
            self.assertNotIn("https://", page)
            # A replacement at exactly the same filename must lose all prior approval.
            (captures / "mechanics_test_fp_driver.mp4").write_bytes(b"replacement-media")
            replaced = library.aggregate(root, qa_path)
            self.assertEqual(replaced["summary"]["reviewed_mechanics"], 0)
            self.assertEqual(len(replaced["unbound_qa"]), 1)

    @patch.object(library, "inspect_streams", return_value={"passed": True})
    def test_active_queue_and_unmuxed_video_are_never_exposed(self, _probe):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            captures, manifest = self.fixture(root)
            queue = root / "artifacts/astra-first-person-test/queue.json"
            state = json.loads(queue.read_text())
            state["cases"][0]["status"] = "recording"
            library.write_json(queue, state)
            index = library.aggregate(root, captures / "qa.json")
            self.assertEqual(index["summary"]["recordings"], 0)
            _probe.assert_not_called()
            state["cases"][0]["status"] = "recording_failed"
            library.write_json(queue, state)
            receipt = json.loads(manifest.read_text())
            receipt["video"] = str(captures / "test_video_only.mp4")
            library.write_json(manifest, receipt)
            Path(receipt["video"]).write_bytes(b"unfinished")
            index = library.aggregate(root, captures / "qa.json")
            self.assertEqual(index["summary"]["recordings"], 0)
            _probe.assert_not_called()


if __name__ == "__main__":
    unittest.main()
