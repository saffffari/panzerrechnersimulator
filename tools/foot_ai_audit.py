#!/usr/bin/env python3
"""Audit Kessel campaign runs and learned on-foot transition JSONL.

The auditor deliberately has no engine or ML-framework dependency, so the same
quality gate can run on a developer laptop, on Hal, or in CI.  It accepts the
live campaign summary schema (``kessel.campaign_run.v2``) and the exact JSON
mapping emitted by ``FootTransition::to_json_line``
(``kessel.foot_transition.v1``).
"""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from dataclasses import dataclass, field
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import Any, Iterable, Iterator, Mapping, Sequence


AUDIT_SCHEMA = "kessel.foot_ai_audit.v1"
CAMPAIGN_SCHEMA = "kessel.campaign_run.v2"
TRANSITION_SCHEMA = "kessel.foot_transition.v1"

# These strings and hashes mirror src/foot_ai.rs.  Keeping them here makes the
# data contract independently checkable without compiling or importing Rust.
OBSERVATION_SCHEMA = (
    "kessel.foot_observation.v1|role[2],goal_fr_dist[3],goal_kind[4],"
    "own_hp_vel_crouch_aim_ammo_reload_detect_suppress_stamina[10],"
    "tank_fr_dist_mount[4],target_fr_dist_visible_last_hp_armored[7],"
    "ally_fr_dist_alive[4],cover8,slope2,can_interact_jump_fire_reload[4],weapon[3]"
)
ACTION_SCHEMA = (
    "kessel.foot_action.v1|forward,strafe,turn,crouch,aim,fire,reload,"
    "interact,jump,weapon(sidearm|rifle|antitank)"
)
OBS_DIM = 51
ACTION_DIM = 10
OBS_CROUCHED_INDEX = 12
OBS_DETECTED_INDEX = 16

ACTION_FIELDS = (
    "forward",
    "strafe",
    "turn",
    "crouch",
    "aim",
    "fire",
    "reload",
    "interact",
    "jump",
    "weapon",
)
CONTINUOUS_ACTION_FIELDS = ("forward", "strafe", "turn")
BOOLEAN_ACTION_FIELDS = ("crouch", "aim", "fire", "reload", "interact", "jump")
WEAPONS = {"sidearm", "rifle", "antitank"}
ROLES = {"tiger_crew", "soviet_rifle"}
EXPERTS = {
    "tiger_mission_expert",
    "soviet_doctrine_expert",
    "human_demonstration",
}


def fnv1a64(text: str) -> str:
    value = 0xCBF29CE484222325
    for byte in text.encode("utf-8"):
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"fnv1a64:{value:016x}"


EXPECTED_OBSERVATION_SCHEMA_HASH = fnv1a64(OBSERVATION_SCHEMA)
EXPECTED_ACTION_SCHEMA_HASH = fnv1a64(ACTION_SCHEMA)


@dataclass(frozen=True)
class LocatedRecord:
    value: Mapping[str, Any]
    source: str
    line: int

    @property
    def location(self) -> str:
        return f"{self.source}:{self.line}"


def _rate(numerator: int | float, denominator: int | float) -> float | None:
    if denominator == 0:
        return None
    return float(numerator) / float(denominator)


def _finite_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def _integer(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _number(value: Any, default: float = 0.0) -> float:
    return float(value) if _finite_number(value) else default


def _mapping(value: Any) -> Mapping[str, Any]:
    return value if isinstance(value, Mapping) else {}


def _sequence(value: Any) -> Sequence[Any]:
    return value if isinstance(value, list) else []


def _canonical_json(value: Any) -> str:
    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
        sort_keys=True,
    )


def _walk_nonfinite(value: Any, path: str = "$") -> Iterator[str]:
    if isinstance(value, bool) or value is None:
        return
    if isinstance(value, (int, float)):
        if not math.isfinite(value):
            yield path
        return
    if isinstance(value, Mapping):
        for key in sorted(value, key=str):
            yield from _walk_nonfinite(value[key], f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            yield from _walk_nonfinite(item, f"{path}[{index}]")


def _faction_for_role(role: str) -> str:
    return "german" if role in {"tiger_crew", "tiger_campaign"} else "soviet"


def _site_sort_key(site: int | str) -> tuple[int, int | str]:
    return (0, site) if isinstance(site, int) else (1, str(site))


@dataclass
class GroupAccumulator:
    faction: str
    role: str
    act: int | str
    site: int | str
    episodes: set[str] = field(default_factory=set)
    outcomes: Counter[str] = field(default_factory=Counter)
    experts: Counter[str] = field(default_factory=Counter)
    transition_episode_state: dict[str, dict[str, bool]] = field(default_factory=dict)
    transition_count: int = 0
    foot_distance_m: float = 0.0
    foot_distance_samples: int = 0
    foot_distance_unknown_transitions: int = 0
    foot_time_s: float = 0.0
    foot_time_samples: int = 0
    foot_time_unknown_transitions: int = 0
    maps_collected: int = 0
    map_searches: int = 0
    returned_with_map: int = 0
    remounts: int = 0
    map_denials: int = 0
    episodes_with_map: set[str] = field(default_factory=set)
    episodes_with_search: set[str] = field(default_factory=set)
    episodes_with_return: set[str] = field(default_factory=set)
    episodes_with_remount: set[str] = field(default_factory=set)
    remount_unknown_episodes: set[str] = field(default_factory=set)
    shots: int = 0
    hits: int = 0
    target_kias: int = 0
    actor_kias: int = 0
    expert_fire_labels: int = 0
    crouch_samples: int = 0
    crouched_samples: int = 0
    crouch_action_samples: int = 0
    crouch_actions: int = 0
    detection_samples: int = 0
    detected_samples: int = 0
    blocked_events: int = 0
    illegal_action_events: int = 0
    friendly_fire_events: int = 0
    friendly_fire_damage: float = 0.0

    def transition_state(self, episode: str) -> dict[str, bool]:
        return self.transition_episode_state.setdefault(
            episode,
            {
                "terminated": False,
                "truncated": False,
                "actor_kia": False,
                "target_kia": False,
                "map_searched": False,
                "returned_with_map": False,
                "remounted": False,
            },
        )

    def finalize(self) -> dict[str, Any]:
        outcomes = self.outcomes.copy()
        for state in self.transition_episode_state.values():
            if state["actor_kia"]:
                outcome = "actor_kia"
            elif state["target_kia"] and (state["terminated"] or state["truncated"]):
                outcome = "target_kia"
            elif state["remounted"] and (state["terminated"] or state["truncated"]):
                outcome = "remounted"
            elif state["truncated"]:
                outcome = "truncated"
            elif state["terminated"]:
                outcome = "terminated"
            else:
                outcome = "unterminated"
            outcomes[outcome] += 1

        return {
            "faction": self.faction,
            "role": self.role,
            "act": self.act,
            "site": self.site,
            "episode_count": len(self.episodes),
            "transition_count": self.transition_count,
            "outcomes": dict(sorted(outcomes.items())),
            "experts": dict(sorted(self.experts.items())),
            "foot": {
                "distance_m": self.foot_distance_m,
                "distance_known_samples": self.foot_distance_samples,
                "distance_unknown_transition_records": self.foot_distance_unknown_transitions,
                "time_s": self.foot_time_s,
                "time_known_samples": self.foot_time_samples,
                "time_unknown_transition_records": self.foot_time_unknown_transitions,
            },
            "objectives": {
                "maps_collected": self.maps_collected,
                "map_searches": self.map_searches,
                "returns_with_map": self.returned_with_map,
                "remounts": self.remounts,
                "map_denials": self.map_denials,
                "episodes_with_map": len(self.episodes_with_map),
                "episodes_with_search": len(self.episodes_with_search),
                "episodes_with_return": len(self.episodes_with_return),
                "episodes_with_remount": len(self.episodes_with_remount),
                "remount_unknown_episodes": len(self.remount_unknown_episodes),
            },
            "combat": {
                "shots": self.shots,
                "hits": self.hits,
                "target_kias": self.target_kias,
                "actor_kias": self.actor_kias,
                "expert_fire_labels": self.expert_fire_labels,
                "hit_rate": _rate(self.hits, self.shots),
            },
            "crouch": {
                "state_samples": self.crouch_samples,
                "crouched_samples": self.crouched_samples,
                "state_rate": _rate(self.crouched_samples, self.crouch_samples),
                "action_samples": self.crouch_action_samples,
                "action_count": self.crouch_actions,
                "action_rate": _rate(self.crouch_actions, self.crouch_action_samples),
            },
            "detection": {
                "samples": self.detection_samples,
                "detected_samples": self.detected_samples,
                "rate": _rate(self.detected_samples, self.detection_samples),
            },
            "events": {
                "blocked": self.blocked_events,
                "blocked_rate": _rate(self.blocked_events, self.transition_count),
                "illegal_action": self.illegal_action_events,
                "illegal_action_rate": _rate(self.illegal_action_events, self.transition_count),
                "friendly_fire": self.friendly_fire_events,
                "friendly_fire_rate": _rate(self.friendly_fire_events, self.transition_count),
                "friendly_fire_damage": self.friendly_fire_damage,
            },
        }


class AuditBuilder:
    def __init__(self, inputs: list[dict[str, Any]] | None = None) -> None:
        self.inputs = inputs or []
        self.total_lines = 0
        self.campaign_records = 0
        self.transition_records = 0
        self.invalid_records = 0
        self.unknown_schema_records = 0
        self.errors: list[str] = []
        self.warnings: list[str] = []
        self.contract_violations: list[str] = []
        self.finite_violations: list[str] = []
        self.bound_violations: list[str] = []
        self.dimension_violations: list[str] = []
        self.groups: dict[tuple[str, str, int | str, int | str], GroupAccumulator] = {}
        self.obs_dimensions: defaultdict[str, Counter[int]] = defaultdict(Counter)
        self.next_obs_dimensions: defaultdict[str, Counter[int]] = defaultdict(Counter)
        self.action_dimensions: defaultdict[str, Counter[int]] = defaultdict(Counter)
        self.expert_action_dimensions: defaultdict[str, Counter[int]] = defaultdict(Counter)
        self.transition_states: dict[str, dict[str, bool]] = {}
        self.transition_key_hashes: defaultdict[str, list[str]] = defaultdict(list)
        self.provided_hash_count = 0
        self.provided_hash_mismatches = 0
        self.unhashable_transitions = 0
        self.soviet_rifle_fire_events = 0
        self.soviet_rifle_action_fire_events = 0
        self.soviet_rifle_expert_fire_events = 0
        self.soviet_rifle_campaign_fire_events = 0
        self.campaign_remount_inference_avoided = 0

    def group(
        self,
        faction: str,
        role: str,
        act: int | str,
        site: int | str,
    ) -> GroupAccumulator:
        key = (faction, role, act, site)
        if key not in self.groups:
            self.groups[key] = GroupAccumulator(faction, role, act, site)
        return self.groups[key]

    def problem(self, target: list[str], located: LocatedRecord, message: str) -> None:
        target.append(f"{located.location}: {message}")

    def add_record(self, located: LocatedRecord) -> None:
        self.total_lines += 1
        schema = located.value.get("schema")
        for path in _walk_nonfinite(located.value):
            self.problem(self.finite_violations, located, f"non-finite value at {path}")
        if schema == CAMPAIGN_SCHEMA:
            self.campaign_records += 1
            self.add_campaign(located)
        elif schema == TRANSITION_SCHEMA:
            self.transition_records += 1
            self.add_transition(located)
        else:
            self.invalid_records += 1
            self.unknown_schema_records += 1
            self.problem(self.errors, located, f"unsupported schema {schema!r}")

    def add_campaign(self, located: LocatedRecord) -> None:
        record = located.value
        seed = record.get("seed", f"{located.source}:{located.line}")
        episode = f"campaign:{seed}"
        act_value = record.get("act", 1)
        act: int | str = act_value if _integer(act_value) else 1
        if "act" not in record:
            self.warnings.append(
                f"{located.location}: campaign_run.v2 has no act field; inferred Act 1"
            )
        group = self.group("german", "tiger_campaign", act, "campaign")
        group.episodes.add(episode)
        outcome = record.get("outcome", "unknown")
        group.outcomes[str(outcome)] += 1

        distance = _mapping(record.get("distance_m")).get("foot")
        if _finite_number(distance):
            group.foot_distance_m += float(distance)
            group.foot_distance_samples += 1
        else:
            self.problem(self.contract_violations, located, "missing/invalid distance_m.foot")
        duration = _mapping(record.get("time_s")).get("foot")
        if _finite_number(duration):
            group.foot_time_s += float(duration)
            group.foot_time_samples += 1
        else:
            self.problem(self.contract_violations, located, "missing/invalid time_s.foot")

        maps = _sequence(record.get("maps"))
        group.maps_collected += len(maps)
        group.map_searches += len(maps)
        if maps:
            group.episodes_with_map.add(episode)
            group.episodes_with_search.add(episode)

        # campaign_run.v2 has dismounts and map pickups, but no explicit return or
        # remount event.  Do not silently turn matching counts into false labels.
        dismounts = record.get("dismounts")
        if _integer(dismounts) and dismounts > 0:
            group.remount_unknown_episodes.add(episode)
            self.campaign_remount_inference_avoided += 1

        trajectory = _sequence(record.get("foot_trajectory"))
        for sample in trajectory:
            sample = _mapping(sample)
            crouched = sample.get("crouched")
            visible = sample.get("visible")
            if isinstance(crouched, bool):
                group.crouch_samples += 1
                group.crouched_samples += int(crouched)
            if isinstance(visible, bool):
                group.detection_samples += 1
                group.detected_samples += int(visible)

        for shot in _sequence(record.get("pedestrian_shots")):
            shot = _mapping(shot)
            weapon = shot.get("weapon")
            if weapon == "infantry_rifle":
                shot_role = "soviet_rifle"
                self.soviet_rifle_fire_events += 1
                self.soviet_rifle_campaign_fire_events += 1
            elif weapon == "tank_coax_mg":
                shot_role = "soviet_tank_mg"
            else:
                shot_role = "unknown_shooter"
            shot_group = self.group("soviet", shot_role, act, "campaign")
            shot_group.episodes.add(episode)
            shot_group.shots += 1
            shot_group.hits += int(shot.get("hit") is True)
            shot_group.target_kias += int(shot.get("killed") is True)

    def _require_transition_contract(self, located: LocatedRecord) -> None:
        record = located.value
        expected_text = {
            "observation_schema_hash": EXPECTED_OBSERVATION_SCHEMA_HASH,
            "action_schema_hash": EXPECTED_ACTION_SCHEMA_HASH,
        }
        for key, expected in expected_text.items():
            if record.get(key) != expected:
                self.problem(
                    self.contract_violations,
                    located,
                    f"{key} {record.get(key)!r} != {expected!r}",
                )
        for key in ("episode_id", "step", "seed", "act", "site_id", "agent_id"):
            if not _integer(record.get(key)) or record[key] < 0:
                self.problem(self.contract_violations, located, f"missing/invalid non-negative {key}")
        if record.get("role") not in ROLES:
            self.problem(self.contract_violations, located, f"invalid role {record.get('role')!r}")
        if record.get("expert") not in EXPERTS:
            self.problem(self.contract_violations, located, f"invalid expert {record.get('expert')!r}")
        for key in ("terminated", "truncated"):
            if not isinstance(record.get(key), bool):
                self.problem(self.contract_violations, located, f"missing/non-boolean {key}")
        if not _finite_number(record.get("reward")):
            self.problem(self.contract_violations, located, "missing/non-finite reward")

    def _observation(
        self,
        located: LocatedRecord,
        role: str,
        primary_key: str,
        aliases: tuple[str, ...],
        dimension_counter: defaultdict[str, Counter[int]],
    ) -> list[float] | None:
        value: Any = None
        found_key: str | None = None
        for key in (primary_key, *aliases):
            if key in located.value:
                value = located.value[key]
                found_key = key
                break
        if found_key is not None and found_key != primary_key:
            self.warnings.append(
                f"{located.location}: used tolerant alias {found_key!r} for {primary_key}"
            )
        if not isinstance(value, list):
            self.problem(self.dimension_violations, located, f"missing/list-required {primary_key}")
            return None
        dimension_counter[role][len(value)] += 1
        if len(value) != OBS_DIM:
            self.problem(
                self.dimension_violations,
                located,
                f"{primary_key} dimension {len(value)} != {OBS_DIM}",
            )
        output: list[float] = []
        for index, item in enumerate(value):
            if not _finite_number(item):
                self.problem(
                    self.contract_violations,
                    located,
                    f"{primary_key}[{index}] is not a finite number",
                )
                return None
            numeric = float(item)
            if numeric < -1.0 - 1.0e-6 or numeric > 1.0 + 1.0e-6:
                self.problem(
                    self.bound_violations,
                    located,
                    f"{primary_key}[{index}]={numeric:g} outside [-1, 1]",
                )
            output.append(numeric)
        return output

    def _action(
        self,
        located: LocatedRecord,
        role: str,
        primary_key: str,
        aliases: tuple[str, ...],
        dimension_counter: defaultdict[str, Counter[int]],
    ) -> dict[str, Any] | None:
        raw: Any = None
        found_key: str | None = None
        for key in (primary_key, *aliases):
            if key in located.value:
                raw = located.value[key]
                found_key = key
                break
        if found_key is not None and found_key != primary_key:
            self.warnings.append(
                f"{located.location}: used tolerant alias {found_key!r} for {primary_key}"
            )
        if isinstance(raw, list):
            dimension_counter[role][len(raw)] += 1
            if len(raw) != ACTION_DIM:
                self.problem(
                    self.dimension_violations,
                    located,
                    f"{primary_key} dimension {len(raw)} != {ACTION_DIM}",
                )
                return None
            if not all(_finite_number(value) for value in raw):
                self.problem(
                    self.contract_violations,
                    located,
                    f"numeric {primary_key} contains non-finite value",
                )
                return None
            values = [float(value) for value in raw]
            for index, value in enumerate(values):
                if value < -1.0 - 1.0e-6 or value > 1.0 + 1.0e-6:
                    self.problem(
                        self.bound_violations,
                        located,
                        f"{primary_key}[{index}]={value:g} outside [-1, 1]",
                    )
            weapon = "sidearm" if values[9] < -0.33 else "antitank" if values[9] > 0.33 else "rifle"
            return {
                "forward": values[0],
                "strafe": values[1],
                "turn": values[2],
                "crouch": values[3] > 0.0,
                "aim": values[4] > 0.0,
                "fire": values[5] > 0.0,
                "reload": values[6] > 0.0,
                "interact": values[7] > 0.0,
                "jump": values[8] > 0.0,
                "weapon": weapon,
            }
        if not isinstance(raw, Mapping):
            self.problem(
                self.dimension_violations,
                located,
                f"{primary_key} must be an object or 10-value list",
            )
            return None

        present = sum(field in raw for field in ACTION_FIELDS)
        dimension_counter[role][present] += 1
        if present != ACTION_DIM:
            missing = [field for field in ACTION_FIELDS if field not in raw]
            self.problem(
                self.dimension_violations,
                located,
                f"{primary_key} logical dimension {present} != {ACTION_DIM}; missing {missing}",
            )
        action: dict[str, Any] = {}
        valid = present == ACTION_DIM
        for key in CONTINUOUS_ACTION_FIELDS:
            value = raw.get(key)
            if not _finite_number(value):
                self.problem(
                    self.contract_violations,
                    located,
                    f"{primary_key}.{key} is not finite numeric",
                )
                valid = False
                continue
            numeric = float(value)
            if numeric < -1.0 - 1.0e-6 or numeric > 1.0 + 1.0e-6:
                self.problem(
                    self.bound_violations,
                    located,
                    f"{primary_key}.{key}={numeric:g} outside [-1, 1]",
                )
            action[key] = numeric
        for key in BOOLEAN_ACTION_FIELDS:
            value = raw.get(key)
            if not isinstance(value, bool):
                self.problem(
                    self.contract_violations,
                    located,
                    f"{primary_key}.{key} is not boolean",
                )
                valid = False
            else:
                action[key] = value
        weapon = raw.get("weapon")
        if weapon not in WEAPONS:
            self.problem(
                self.contract_violations,
                located,
                f"invalid {primary_key}.weapon {weapon!r}",
            )
            valid = False
        else:
            action["weapon"] = weapon
        return action if valid else None

    def _events(self, located: LocatedRecord) -> dict[str, Any]:
        raw = located.value.get("events")
        if not isinstance(raw, Mapping):
            self.problem(self.contract_violations, located, "events must be an object")
            raw = {}
        events: dict[str, Any] = {}
        for key in (
            "map_searched",
            "returned_with_map",
            "remounted",
            "target_kia",
            "map_denied",
            "actor_kia",
            "blocked",
            "illegal_action",
        ):
            value = raw.get(key)
            if not isinstance(value, bool):
                self.problem(self.contract_violations, located, f"events.{key} is not boolean")
                value = False
            events[key] = value
        for key in ("target_damage", "friendly_fire_damage"):
            value = raw.get(key)
            if not _finite_number(value):
                self.problem(self.contract_violations, located, f"events.{key} is not finite numeric")
                value = 0.0
            elif value < 0.0:
                self.problem(self.contract_violations, located, f"events.{key} is negative")
            events[key] = float(value)
        return events

    def add_transition(self, located: LocatedRecord) -> None:
        self._require_transition_contract(located)
        record = located.value
        role = record.get("role") if record.get("role") in ROLES else "unknown_role"
        faction = _faction_for_role(role)
        act: int | str = record.get("act") if _integer(record.get("act")) else "unknown"
        site: int | str = record.get("site_id") if _integer(record.get("site_id")) else "unknown"
        episode_id = record.get("episode_id", f"{located.source}:{located.line}")
        agent_id = record.get("agent_id", "unknown")
        episode = f"transition:{role}:{episode_id}:{agent_id}"
        group = self.group(faction, role, act, site)
        group.episodes.add(episode)
        group.transition_count += 1
        if isinstance(record.get("expert"), str):
            group.experts[record["expert"]] += 1

        observation = self._observation(
            located, role, "observation", ("obs",), self.obs_dimensions
        )
        next_observation = self._observation(
            located,
            role,
            "next_observation",
            ("next_obs",),
            self.next_obs_dimensions,
        )
        action = self._action(
            located,
            role,
            "action",
            ("policy_action",),
            self.action_dimensions,
        )
        expert_action = self._action(
            located,
            role,
            "expert_action",
            ("teacher_action", "label_action"),
            self.expert_action_dimensions,
        )
        events = self._events(located)

        terminated = record.get("terminated") is True
        truncated = record.get("truncated") is True
        state = self.transition_states.setdefault(
            episode, {"terminated": False, "truncated": False}
        )
        state["terminated"] |= terminated
        state["truncated"] |= truncated
        group_state = group.transition_state(episode)
        group_state["terminated"] |= terminated
        group_state["truncated"] |= truncated
        for key in (
            "actor_kia",
            "target_kia",
            "map_searched",
            "returned_with_map",
            "remounted",
        ):
            group_state[key] |= events[key]

        distance_delta = record.get("foot_distance_delta_m", record.get("distance_delta_m"))
        if _finite_number(distance_delta):
            group.foot_distance_m += float(distance_delta)
            group.foot_distance_samples += 1
        else:
            group.foot_distance_unknown_transitions += 1
        dt_s = record.get("dt_s")
        if _finite_number(dt_s):
            group.foot_time_s += float(dt_s)
            group.foot_time_samples += 1
        else:
            group.foot_time_unknown_transitions += 1

        if events["map_searched"]:
            group.map_searches += 1
            group.episodes_with_search.add(episode)
        if events["returned_with_map"]:
            group.maps_collected += 1
            group.returned_with_map += 1
            group.episodes_with_map.add(episode)
            group.episodes_with_return.add(episode)
        if events["remounted"]:
            group.remounts += 1
            group.episodes_with_remount.add(episode)
        group.map_denials += int(events["map_denied"])

        if action is not None:
            group.crouch_action_samples += 1
            group.crouch_actions += int(action["crouch"])
            group.shots += int(action["fire"])
        actual_soviet_fire = bool(
            role == "soviet_rifle"
            and action is not None
            and action["fire"]
            and action["weapon"] == "rifle"
        )
        expert_soviet_fire = bool(
            role == "soviet_rifle"
            and expert_action is not None
            and expert_action["fire"]
            and expert_action["weapon"] == "rifle"
        )
        if expert_action is not None:
            group.expert_fire_labels += int(expert_action["fire"])
        self.soviet_rifle_action_fire_events += int(actual_soviet_fire)
        self.soviet_rifle_expert_fire_events += int(expert_soviet_fire)
        self.soviet_rifle_fire_events += int(actual_soviet_fire or expert_soviet_fire)
        if observation is not None and len(observation) > OBS_DETECTED_INDEX:
            group.crouch_samples += 1
            group.crouched_samples += int(observation[OBS_CROUCHED_INDEX] > 0.0)
            group.detection_samples += 1
            group.detected_samples += int(observation[OBS_DETECTED_INDEX] > 0.0)

        group.hits += int(events["target_damage"] > 0.0)
        group.target_kias += int(events["target_kia"])
        group.actor_kias += int(events["actor_kia"])
        group.blocked_events += int(events["blocked"])
        group.illegal_action_events += int(events["illegal_action"])
        group.friendly_fire_events += int(events["friendly_fire_damage"] > 0.0)
        group.friendly_fire_damage += events["friendly_fire_damage"]

        canonical = self._canonical_transition(
            record,
            role,
            observation,
            action,
            expert_action,
            next_observation,
            events,
            terminated,
            truncated,
        )
        if canonical is None:
            self.unhashable_transitions += 1
            return
        digest = hashlib.sha256(_canonical_json(canonical).encode("utf-8")).hexdigest()
        key = (
            f"{canonical['role']}:{canonical['episode_id']}:"
            f"{canonical['agent_id']}:{canonical['step']}"
        )
        self.transition_key_hashes[key].append(digest)
        provided = record.get("transition_hash", record.get("deterministic_hash"))
        if provided is not None:
            self.provided_hash_count += 1
            normalized = str(provided).removeprefix("sha256:")
            if normalized != digest:
                self.provided_hash_mismatches += 1
                self.problem(
                    self.contract_violations,
                    located,
                    "provided transition hash does not match canonical payload",
                )

    def _canonical_transition(
        self,
        record: Mapping[str, Any],
        role: str,
        observation: list[float] | None,
        action: dict[str, Any] | None,
        expert_action: dict[str, Any] | None,
        next_observation: list[float] | None,
        events: dict[str, Any],
        terminated: bool,
        truncated: bool,
    ) -> dict[str, Any] | None:
        integer_fields = ("episode_id", "step", "seed", "act", "site_id", "agent_id")
        if (
            observation is None
            or next_observation is None
            or action is None
            or expert_action is None
        ):
            return None
        if len(observation) != OBS_DIM or len(next_observation) != OBS_DIM:
            return None
        if any(not _integer(record.get(key)) or record[key] < 0 for key in integer_fields):
            return None
        if role not in ROLES or record.get("expert") not in EXPERTS:
            return None
        if not _finite_number(record.get("reward")):
            return None
        return {
            "schema": TRANSITION_SCHEMA,
            "observation_schema_hash": record.get("observation_schema_hash"),
            "action_schema_hash": record.get("action_schema_hash"),
            "episode_id": int(record["episode_id"]),
            "step": int(record["step"]),
            "seed": int(record["seed"]),
            "act": int(record["act"]),
            "site_id": int(record["site_id"]),
            "agent_id": int(record["agent_id"]),
            "role": role,
            "expert": record["expert"],
            "observation": observation,
            "action": {key: action[key] for key in ACTION_FIELDS},
            "expert_action": {key: expert_action[key] for key in ACTION_FIELDS},
            "reward": float(record["reward"]),
            "next_observation": next_observation,
            "terminated": terminated,
            "truncated": truncated,
            "events": events,
        }

    def finish(self) -> dict[str, Any]:
        groups = [group.finalize() for group in self.groups.values()]
        groups.sort(
            key=lambda item: (
                item["faction"],
                item["role"],
                (0, item["act"]) if isinstance(item["act"], int) else (1, str(item["act"])),
                _site_sort_key(item["site"]),
            )
        )

        transition_episodes = len(self.transition_states)
        terminal_episodes = sum(
            state["terminated"] or state["truncated"]
            for state in self.transition_states.values()
        )
        terminal_coverage = _rate(terminal_episodes, transition_episodes)
        duplicate_key_count = sum(
            len(hashes) > 1 for hashes in self.transition_key_hashes.values()
        )
        duplicate_record_count = sum(
            max(0, len(hashes) - 1) for hashes in self.transition_key_hashes.values()
        )
        hash_conflicts = sum(
            len(set(hashes)) > 1 for hashes in self.transition_key_hashes.values()
        )
        canonical_entries = sorted(
            f"{key}:{digest}"
            for key, hashes in self.transition_key_hashes.items()
            for digest in hashes
        )
        dataset_hash = (
            hashlib.sha256("\n".join(canonical_entries).encode("utf-8")).hexdigest()
            if canonical_entries
            else None
        )

        dimension_roles: dict[str, Any] = {}
        all_roles = sorted(
            set(self.obs_dimensions)
            | set(self.next_obs_dimensions)
            | set(self.action_dimensions)
            | set(self.expert_action_dimensions)
        )
        for role in all_roles:
            dimension_roles[role] = {
                "observation": dict(sorted(self.obs_dimensions[role].items())),
                "next_observation": dict(sorted(self.next_obs_dimensions[role].items())),
                "action": dict(sorted(self.action_dimensions[role].items())),
                "expert_action": dict(sorted(self.expert_action_dimensions[role].items())),
            }

        event_transition_count = sum(group.transition_count for group in self.groups.values())
        blocked = sum(group.blocked_events for group in self.groups.values())
        illegal = sum(group.illegal_action_events for group in self.groups.values())
        friendly_fire = sum(group.friendly_fire_events for group in self.groups.values())
        foot_distance = sum(group.foot_distance_m for group in self.groups.values())
        foot_distance_samples = sum(group.foot_distance_samples for group in self.groups.values())
        foot_time = sum(group.foot_time_s for group in self.groups.values())
        foot_time_samples = sum(group.foot_time_samples for group in self.groups.values())
        shots = sum(group.shots for group in self.groups.values())
        hits = sum(group.hits for group in self.groups.values())
        target_kias = sum(group.target_kias for group in self.groups.values())
        actor_kias = sum(group.actor_kias for group in self.groups.values())

        tensor_consistent = not self.dimension_violations
        finite_ok = not self.finite_violations
        bounds_ok = not self.bound_violations
        deterministic_ok = (
            self.transition_records > 0
            and self.unhashable_transitions == 0
            and hash_conflicts == 0
            and self.provided_hash_mismatches == 0
        )
        contracts_ok = not self.contract_violations
        input_ok = self.invalid_records == 0
        soviet_training_fire_events = (
            self.soviet_rifle_expert_fire_events + self.soviet_rifle_campaign_fire_events
        )
        gates = {
            "recognized_records": {
                "passed": self.campaign_records + self.transition_records > 0,
                "count": self.campaign_records + self.transition_records,
            },
            "valid_json_and_supported_schema": {
                "passed": input_ok,
                "invalid_records": self.invalid_records,
            },
            "foot_transitions_present": {
                "passed": self.transition_records > 0,
                "count": self.transition_records,
            },
            "soviet_rifle_events_present": {
                "passed": soviet_training_fire_events > 0,
                "count": soviet_training_fire_events,
                "executed_action_fire": self.soviet_rifle_action_fire_events,
                "expert_action_fire": self.soviet_rifle_expert_fire_events,
                "campaign_infantry_rifle_fire": self.soviet_rifle_campaign_fire_events,
                "definition": "positive Soviet-rifle expert_action label, or campaign infantry_rifle shot",
            },
            "transition_schema_contract": {
                "passed": contracts_ok,
                "violations": len(self.contract_violations),
            },
            "tensor_dimensions_consistent": {
                "passed": tensor_consistent,
                "violations": len(self.dimension_violations),
                "expected_observation": OBS_DIM,
                "expected_action": ACTION_DIM,
            },
            "finite_values": {
                "passed": finite_ok,
                "violations": len(self.finite_violations),
            },
            "bounded_policy_values": {
                "passed": bounds_ok,
                "violations": len(self.bound_violations),
                "bounds": [-1.0, 1.0],
            },
            "terminal_coverage": {
                "passed": transition_episodes > 0 and terminal_episodes == transition_episodes,
                "terminal_episodes": terminal_episodes,
                "transition_episodes": transition_episodes,
                "rate": terminal_coverage,
            },
            "deterministic_transition_hashes": {
                "passed": deterministic_ok,
                "unhashable_transitions": self.unhashable_transitions,
                "conflicting_keys": hash_conflicts,
                "provided_hash_mismatches": self.provided_hash_mismatches,
            },
        }
        passed = all(gate["passed"] for gate in gates.values())
        outcome_totals: Counter[str] = Counter()
        for group in groups:
            outcome_totals.update(group["outcomes"])
        all_episode_tokens = {
            episode
            for accumulator in self.groups.values()
            for episode in accumulator.episodes
        }

        return {
            "schema": AUDIT_SCHEMA,
            "passed": passed,
            "inputs": self.inputs,
            "records": {
                "total_nonempty_lines": self.total_lines,
                "campaign_runs": self.campaign_records,
                "foot_transitions": self.transition_records,
                "invalid": self.invalid_records,
                "unknown_schema": self.unknown_schema_records,
            },
            "summary": {
                "episode_count": len(all_episode_tokens),
                "outcomes": dict(sorted(outcome_totals.items())),
                "foot": {
                    "distance_m": foot_distance,
                    "distance_known_samples": foot_distance_samples,
                    "time_s": foot_time,
                    "time_known_samples": foot_time_samples,
                },
                "objectives": {
                    "maps_collected": sum(group.maps_collected for group in self.groups.values()),
                    "map_searches": sum(group.map_searches for group in self.groups.values()),
                    "returns_with_map": sum(
                        group.returned_with_map for group in self.groups.values()
                    ),
                    "remounts": sum(group.remounts for group in self.groups.values()),
                    "campaign_remount_inference_avoided": self.campaign_remount_inference_avoided,
                },
                "combat": {
                    "shots": shots,
                    "hits": hits,
                    "target_kias": target_kias,
                    "actor_kias": actor_kias,
                    "hit_rate": _rate(hits, shots),
                    "soviet_rifle_fire_events": self.soviet_rifle_fire_events,
                    "soviet_rifle_action_fire_events": self.soviet_rifle_action_fire_events,
                    "soviet_rifle_expert_fire_events": self.soviet_rifle_expert_fire_events,
                    "soviet_rifle_campaign_fire_events": self.soviet_rifle_campaign_fire_events,
                },
                "events": {
                    "transition_count": event_transition_count,
                    "blocked": blocked,
                    "blocked_rate": _rate(blocked, event_transition_count),
                    "illegal_action": illegal,
                    "illegal_action_rate": _rate(illegal, event_transition_count),
                    "friendly_fire": friendly_fire,
                    "friendly_fire_rate": _rate(friendly_fire, event_transition_count),
                },
            },
            "groups": groups,
            "tensor_contracts": {
                "expected": {
                    "observation_dimension": OBS_DIM,
                    "action_dimension": ACTION_DIM,
                    "observation_schema_hash": EXPECTED_OBSERVATION_SCHEMA_HASH,
                    "action_schema_hash": EXPECTED_ACTION_SCHEMA_HASH,
                },
                "roles": dimension_roles,
                "dimension_consistent": tensor_consistent,
                "dimension_violations": self.dimension_violations[:50],
                "finite": finite_ok,
                "finite_violations": self.finite_violations[:50],
                "bounded": bounds_ok,
                "bound_violations": self.bound_violations[:50],
                "schema_contract_violations": self.contract_violations[:50],
            },
            "transitions": {
                "episodes": transition_episodes,
                "count": self.transition_records,
                "terminal_episodes": terminal_episodes,
                "terminal_coverage_rate": terminal_coverage,
                "canonical_hash_algorithm": "sha256-canonical-foot-transition-v1",
                "dataset_hash": f"sha256:{dataset_hash}" if dataset_hash else None,
                "hashed_transitions": len(canonical_entries),
                "unique_hashes": len({entry.rsplit(":", 1)[1] for entry in canonical_entries}),
                "duplicate_key_count": duplicate_key_count,
                "duplicate_record_count": duplicate_record_count,
                "conflicting_key_count": hash_conflicts,
                "provided_hash_count": self.provided_hash_count,
                "provided_hash_mismatches": self.provided_hash_mismatches,
                "unhashable_transitions": self.unhashable_transitions,
            },
            "quality_gates": gates,
            "errors": self.errors[:50],
            "warnings": self.warnings[:50],
        }


def read_jsonl(paths: Iterable[Path]) -> tuple[list[LocatedRecord], list[dict[str, Any]], list[str]]:
    records: list[LocatedRecord] = []
    inputs: list[dict[str, Any]] = []
    errors: list[str] = []
    for path in paths:
        input_summary = {"path": str(path), "nonempty_lines": 0, "parsed_records": 0}
        inputs.append(input_summary)
        try:
            with path.open("r", encoding="utf-8") as stream:
                for line_number, raw_line in enumerate(stream, start=1):
                    line = raw_line.strip()
                    if not line:
                        continue
                    input_summary["nonempty_lines"] += 1
                    try:
                        value = json.loads(line)
                    except (json.JSONDecodeError, ValueError) as error:
                        errors.append(f"{path}:{line_number}: invalid JSON: {error}")
                        continue
                    if not isinstance(value, Mapping):
                        errors.append(f"{path}:{line_number}: JSONL record must be an object")
                        continue
                    records.append(LocatedRecord(value, str(path), line_number))
                    input_summary["parsed_records"] += 1
        except OSError as error:
            errors.append(f"{path}: cannot read input: {error}")
    return records, inputs, errors


def audit_paths(paths: Iterable[Path]) -> dict[str, Any]:
    records, inputs, read_errors = read_jsonl(paths)
    builder = AuditBuilder(inputs)
    for error in read_errors:
        builder.total_lines += 1
        builder.invalid_records += 1
        builder.errors.append(error)
    for record in records:
        builder.add_record(record)
    return builder.finish()


def audit_records(records: Iterable[Mapping[str, Any]]) -> dict[str, Any]:
    """Audit in-memory records; primarily useful to tests and training harnesses."""

    materialized = list(records)
    builder = AuditBuilder(
        [{"path": "<memory>", "nonempty_lines": len(materialized), "parsed_records": len(materialized)}]
    )
    for line, record in enumerate(materialized, start=1):
        if not isinstance(record, Mapping):
            builder.total_lines += 1
            builder.invalid_records += 1
            builder.errors.append(f"<memory>:{line}: record must be an object")
            continue
        builder.add_record(LocatedRecord(record, "<memory>", line))
    return builder.finish()


def render_text(report: Mapping[str, Any]) -> str:
    status = "PASS" if report["passed"] else "FAIL"
    records = report["records"]
    transitions = report["transitions"]
    summary = report["summary"]
    foot = summary["foot"]
    combat = summary["combat"]
    events = summary["events"]
    terminal_rate = transitions["terminal_coverage_rate"]
    terminal_text = "n/a" if terminal_rate is None else f"{terminal_rate * 100.0:.1f}%"
    distance_text = (
        f"{foot['distance_m']:.1f} m" if foot["distance_known_samples"] else "n/a"
    )
    time_text = f"{foot['time_s']:.1f} s" if foot["time_known_samples"] else "n/a"
    failed_gates = [name for name, gate in report["quality_gates"].items() if not gate["passed"]]
    lines = [
        f"Foot AI audit: {status}",
        (
            f"Records: {records['campaign_runs']} campaign, {records['foot_transitions']} "
            f"transitions, {records['invalid']} invalid"
        ),
        (
            f"Transition episodes: {transitions['episodes']}; terminal "
            f"{transitions['terminal_episodes']}/{transitions['episodes']} ({terminal_text})"
        ),
        f"Known foot travel: {distance_text}; known foot time: {time_text}",
        (
            f"Objectives: {summary['objectives']['map_searches']} searches, "
            f"{summary['objectives']['returns_with_map']} returns, "
            f"{summary['objectives']['remounts']} remounts"
        ),
        (
            f"Combat: {combat['shots']} shots, {combat['hits']} hits, "
            f"{combat['target_kias']} target KIAs, {combat['actor_kias']} actor KIAs; "
            f"Soviet-rifle fire={combat['soviet_rifle_fire_events']} "
            f"(action={combat['soviet_rifle_action_fire_events']}, "
            f"expert={combat['soviet_rifle_expert_fire_events']})"
        ),
        (
            f"Event rates: blocked={_format_rate(events['blocked_rate'])}, "
            f"illegal={_format_rate(events['illegal_action_rate'])}, "
            f"friendly-fire={_format_rate(events['friendly_fire_rate'])}"
        ),
        (
            f"Tensor contract: {'PASS' if report['tensor_contracts']['dimension_consistent'] else 'FAIL'}; "
            f"finite={'PASS' if report['tensor_contracts']['finite'] else 'FAIL'}; "
            f"bounded={'PASS' if report['tensor_contracts']['bounded'] else 'FAIL'}"
        ),
        (
            f"Hashes: {transitions['hashed_transitions']} canonical, "
            f"{transitions['conflicting_key_count']} conflicting keys; "
            f"dataset={transitions['dataset_hash'] or 'n/a'}"
        ),
    ]
    if failed_gates:
        lines.append("Failed gates: " + ", ".join(failed_gates))
    lines.append("Groups:")
    for group in report["groups"]:
        lines.append(
            "  "
            f"{group['faction']}/{group['role']}/act-{group['act']}/site-{group['site']}: "
            f"episodes={group['episode_count']} transitions={group['transition_count']} "
            f"shots={group['combat']['shots']} searches={group['objectives']['map_searches']} "
            f"remounts={group['objectives']['remounts']}"
        )
    return "\n".join(lines)


def _format_rate(value: float | None) -> str:
    return "n/a" if value is None else f"{value * 100.0:.2f}%"


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", type=Path, help="Campaign/foot-transition JSONL")
    parser.add_argument(
        "--format",
        choices=("text", "json"),
        default="text",
        help="stdout format (default: text)",
    )
    parser.add_argument("--json-output", type=Path, help="also save machine-readable JSON")
    parser.add_argument("--pretty", action="store_true", help="indent JSON output")
    parser.add_argument(
        "--report-only",
        action="store_true",
        help="return success even when a quality gate fails",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    report = audit_paths(args.inputs)
    indent = 2 if args.pretty else None
    json_text = json.dumps(report, ensure_ascii=False, sort_keys=True, indent=indent)
    if args.json_output:
        args.json_output.parent.mkdir(parents=True, exist_ok=True)
        args.json_output.write_text(json_text + "\n", encoding="utf-8")
    if args.format == "json":
        print(json_text)
    else:
        print(render_text(report))
    return 0 if report["passed"] or args.report_only else 2


if __name__ == "__main__":
    sys.exit(main())
