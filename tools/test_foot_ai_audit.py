#!/usr/bin/env python3
"""Focused regression tests for tools/foot_ai_audit.py."""

from __future__ import annotations

import copy
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import foot_ai_audit as audit


def exact_transition(
    *,
    episode_id: int,
    step: int,
    role: str,
    agent_id: int,
    terminated: bool = False,
    fire: bool = False,
    crouch: bool = False,
    detected: bool = False,
    events: dict[str, object] | None = None,
) -> dict[str, object]:
    observation = [0.0] * audit.OBS_DIM
    observation[0 if role == "tiger_crew" else 1] = 1.0
    observation[audit.OBS_CROUCHED_INDEX] = 1.0 if crouch else -1.0
    observation[audit.OBS_DETECTED_INDEX] = 1.0 if detected else -1.0
    next_observation = observation.copy()
    base_events: dict[str, object] = {
        "map_searched": False,
        "returned_with_map": False,
        "remounted": False,
        "target_damage": 0.0,
        "target_kia": False,
        "map_denied": False,
        "actor_kia": False,
        "blocked": False,
        "illegal_action": False,
        "friendly_fire_damage": 0.0,
    }
    if events:
        base_events.update(events)
    action = {
        "forward": 0.5,
        "strafe": 0.0,
        "turn": -0.25,
        "crouch": crouch,
        "aim": fire,
        "fire": fire,
        "reload": False,
        "interact": role == "tiger_crew",
        "jump": False,
        "weapon": "sidearm" if role == "tiger_crew" else "rifle",
    }
    return {
        "schema": audit.TRANSITION_SCHEMA,
        "observation_schema_hash": audit.EXPECTED_OBSERVATION_SCHEMA_HASH,
        "action_schema_hash": audit.EXPECTED_ACTION_SCHEMA_HASH,
        "episode_id": episode_id,
        "step": step,
        "seed": 0xC0DE_0000 + episode_id,
        "act": 1,
        "site_id": 7 if role == "tiger_crew" else 8,
        "agent_id": agent_id,
        "role": role,
        "expert": (
            "tiger_mission_expert" if role == "tiger_crew" else "soviet_doctrine_expert"
        ),
        "observation": observation,
        "action": action,
        "expert_action": action.copy(),
        "reward": 1.0 if terminated else 0.1,
        "next_observation": next_observation,
        "terminated": terminated,
        "truncated": False,
        "events": base_events,
    }


def passing_records() -> list[dict[str, object]]:
    return [
        exact_transition(episode_id=10, step=0, role="tiger_crew", agent_id=1),
        exact_transition(
            episode_id=10,
            step=1,
            role="tiger_crew",
            agent_id=1,
            terminated=True,
            events={"map_searched": True, "returned_with_map": True, "remounted": True},
        ),
        exact_transition(
            episode_id=20,
            step=0,
            role="soviet_rifle",
            agent_id=4,
            fire=True,
            crouch=True,
            detected=True,
            events={"target_damage": 42.0},
        ),
        exact_transition(
            episode_id=20,
            step=1,
            role="soviet_rifle",
            agent_id=4,
            terminated=True,
            events={"target_kia": True},
        ),
    ]


class FootAiAuditTests(unittest.TestCase):
    def test_exact_exporter_mapping_passes_and_groups_both_factions(self) -> None:
        records = passing_records()
        report = audit.audit_records(records)

        self.assertTrue(report["passed"], report)
        self.assertEqual(report["records"]["foot_transitions"], 4)
        self.assertEqual(report["transitions"]["episodes"], 2)
        self.assertEqual(report["transitions"]["terminal_coverage_rate"], 1.0)
        self.assertEqual(report["summary"]["combat"]["soviet_rifle_fire_events"], 1)
        self.assertEqual(report["summary"]["objectives"]["map_searches"], 1)
        self.assertEqual(report["summary"]["objectives"]["remounts"], 1)
        self.assertEqual(
            {(group["faction"], group["role"]) for group in report["groups"]},
            {("german", "tiger_crew"), ("soviet", "soviet_rifle")},
        )
        roles = report["tensor_contracts"]["roles"]
        self.assertEqual(roles["tiger_crew"]["observation"], {audit.OBS_DIM: 2})
        self.assertEqual(roles["soviet_rifle"]["action"], {audit.ACTION_DIM: 2})
        self.assertEqual(roles["soviet_rifle"]["expert_action"], {audit.ACTION_DIM: 2})
        self.assertEqual(
            report["summary"]["combat"]["soviet_rifle_expert_fire_events"], 1
        )

    def test_canonical_dataset_hash_is_independent_of_input_order(self) -> None:
        records = passing_records()
        forwards = audit.audit_records(records)
        backwards = audit.audit_records(reversed(records))
        self.assertEqual(
            forwards["transitions"]["dataset_hash"],
            backwards["transitions"]["dataset_hash"],
        )

    def test_tolerant_observation_alias_and_numeric_action_remain_supported(self) -> None:
        records = passing_records()
        tiger = records[0]
        tiger["obs"] = tiger.pop("observation")
        tiger["next_obs"] = tiger.pop("next_observation")
        tiger["action"] = [0.5, 0.0, -0.25, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -0.85]
        report = audit.audit_records(records)

        self.assertTrue(report["passed"], report)
        self.assertEqual(report["tensor_contracts"]["roles"]["tiger_crew"]["action"], {10: 2})
        self.assertTrue(any("tolerant alias" in warning for warning in report["warnings"]))

    def test_expert_fire_label_counts_when_executed_policy_does_not_fire(self) -> None:
        records = passing_records()
        soviet = records[2]
        soviet["action"]["fire"] = False  # type: ignore[index]
        soviet["action"]["aim"] = False  # type: ignore[index]
        report = audit.audit_records(records)

        gate = report["quality_gates"]["soviet_rifle_events_present"]
        self.assertTrue(report["passed"], report)
        self.assertEqual(gate["executed_action_fire"], 0)
        self.assertEqual(gate["expert_action_fire"], 1)
        self.assertEqual(report["summary"]["combat"]["shots"], 0)

    def test_missing_expert_action_and_invalid_expert_fail_contract(self) -> None:
        records = passing_records()
        del records[0]["expert_action"]
        records[1]["expert"] = "unversioned_teacher"
        report = audit.audit_records(records)

        self.assertFalse(report["quality_gates"]["transition_schema_contract"]["passed"])
        self.assertFalse(report["quality_gates"]["tensor_dimensions_consistent"]["passed"])
        self.assertGreater(report["transitions"]["unhashable_transitions"], 0)

    def test_campaign_only_reports_metrics_but_fails_required_training_gates(self) -> None:
        campaign = {
            "schema": audit.CAMPAIGN_SCHEMA,
            "seed": 99,
            "outcome": "success",
            "distance_m": {"tank": 900.0, "foot": 31.5},
            "time_s": {"tank": 100.0, "foot": 12.0},
            "dismounts": 1,
            "maps": [{"sheet": 1, "t": 4.0, "pos": [1.0, 2.0]}],
            "foot_trajectory": [
                {"t": 1.0, "pos": [0.0, 0.0], "crouched": True, "visible": False},
                {"t": 2.0, "pos": [1.0, 0.0], "crouched": False, "visible": True},
            ],
            "pedestrian_shots": [],
        }
        report = audit.audit_records([campaign])

        self.assertFalse(report["passed"])
        self.assertFalse(report["quality_gates"]["foot_transitions_present"]["passed"])
        self.assertFalse(report["quality_gates"]["soviet_rifle_events_present"]["passed"])
        self.assertEqual(report["summary"]["foot"]["distance_m"], 31.5)
        self.assertEqual(report["summary"]["foot"]["time_s"], 12.0)
        self.assertEqual(report["summary"]["objectives"]["map_searches"], 1)
        self.assertEqual(report["summary"]["objectives"]["remounts"], 0)
        self.assertEqual(report["summary"]["objectives"]["campaign_remount_inference_avoided"], 1)

    def test_campaign_infantry_rifle_events_are_attributed_to_soviet_group(self) -> None:
        campaign = {
            "schema": audit.CAMPAIGN_SCHEMA,
            "seed": 100,
            "outcome": "actor_kia",
            "distance_m": {"foot": 5.0},
            "time_s": {"foot": 2.0},
            "dismounts": 1,
            "maps": [],
            "foot_trajectory": [],
            "pedestrian_shots": [
                {"weapon": "infantry_rifle", "hit": True, "killed": True}
            ],
        }
        report = audit.audit_records([campaign])
        soviet = next(group for group in report["groups"] if group["role"] == "soviet_rifle")
        self.assertEqual(soviet["combat"]["shots"], 1)
        self.assertEqual(soviet["combat"]["hits"], 1)
        self.assertEqual(soviet["combat"]["target_kias"], 1)
        self.assertTrue(report["quality_gates"]["soviet_rifle_events_present"]["passed"])
        self.assertFalse(report["quality_gates"]["foot_transitions_present"]["passed"])

    def test_dimension_and_bound_violations_fail_quality_gates(self) -> None:
        records = passing_records()
        records[0]["observation"] = [0.0] * (audit.OBS_DIM - 1)
        records[1]["action"]["forward"] = 1.5  # type: ignore[index]
        report = audit.audit_records(records)

        self.assertFalse(report["passed"])
        self.assertFalse(report["quality_gates"]["tensor_dimensions_consistent"]["passed"])
        self.assertFalse(report["quality_gates"]["bounded_policy_values"]["passed"])
        self.assertGreater(report["transitions"]["unhashable_transitions"], 0)

    def test_nonfinite_payload_fails_finite_and_hash_gates(self) -> None:
        records = passing_records()
        records[0]["reward"] = float("nan")
        report = audit.audit_records(records)

        self.assertFalse(report["quality_gates"]["finite_values"]["passed"])
        self.assertFalse(
            report["quality_gates"]["deterministic_transition_hashes"]["passed"]
        )
        self.assertGreater(report["transitions"]["unhashable_transitions"], 0)

    def test_conflicting_same_episode_agent_step_fails_hash_gate(self) -> None:
        records = passing_records()
        conflict = copy.deepcopy(records[0])
        conflict["reward"] = 0.75
        records.append(conflict)
        report = audit.audit_records(records)

        self.assertFalse(report["passed"])
        self.assertEqual(report["transitions"]["duplicate_key_count"], 1)
        self.assertEqual(report["transitions"]["conflicting_key_count"], 1)
        self.assertFalse(
            report["quality_gates"]["deterministic_transition_hashes"]["passed"]
        )

    def test_jsonl_cli_writes_json_and_uses_nonzero_gate_exit(self) -> None:
        records = passing_records()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "transitions.jsonl"
            source.write_text(
                "\n".join(json.dumps(record, separators=(",", ":")) for record in records) + "\n",
                encoding="utf-8",
            )
            output = root / "report.json"
            with redirect_stdout(io.StringIO()) as stdout:
                self.assertEqual(
                    audit.main(
                        [str(source), "--format", "json", "--json-output", str(output)]
                    ),
                    0,
                )
            self.assertTrue(json.loads(stdout.getvalue())["passed"])
            saved = json.loads(output.read_text(encoding="utf-8"))
            self.assertTrue(saved["passed"])
            self.assertIn("Foot AI audit: PASS", audit.render_text(saved))


if __name__ == "__main__":
    unittest.main()
