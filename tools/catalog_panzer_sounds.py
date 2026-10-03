#!/usr/bin/env python3
"""Read-only, bounded-memory inventory of the user-owned Panzer PCM WAV library.

Requires numpy. Originals are only opened for reading. No perceptual listening, source
authentication, loudness standard, musical transcription, or loop acceptance is implied.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import wave

import numpy as np
from soundscape_routing import assignment as runtime_assignment


BLOCK_FRAMES = 65_536
AUDIO_EXTENSIONS = {".wav"}
FOLDER_ROLES = {"night_ambiance": "night", "calm_ambiance": "explore", "combat": "combat",
                "suspense": "threat", "interior_tank_ambiance": "cabin", "music": "music",
                "death_sounds": "stinger", "effects": "effect", "game_start_atmosphere": "intro",
                "winter_atmosphere": "day", "summer_atmosphere": "day",
                "sunrise": "stinger", "sunset": "stinger", "dusk": "stinger", "heavy_blizzard": "blizzard",
                "engine_hiss": "engine_hiss"}
FOLDER_CONTEXTS = {"winter_atmosphere": ["winter"], "summer_atmosphere": ["spring"]}


class SourceChangedError(RuntimeError):
    pass


def source_snapshot(root: Path, folders: tuple[str, ...] | None = None) -> dict[str, tuple[int, int]]:
    result = {}
    roots = [root] if folders is None else [root / folder for folder in folders]
    for scan_root in roots:
        for path in scan_root.rglob("*"):
            if path.is_file() and path.suffix.lower() in AUDIO_EXTENSIONS:
                try:
                    stat = path.stat()
                except FileNotFoundError:
                    continue
                result[path.relative_to(root).as_posix()] = (stat.st_size, stat.st_mtime_ns)
    return result


def db(value: float) -> float | None:
    """JSON null denotes exact digital silence, rather than nonstandard -Infinity."""
    return 20.0 * math.log10(value) if value > 0.0 else None


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def pcm_decode(raw: bytes, width: int, channels: int) -> np.ndarray:
    if width == 1:
        values = (np.frombuffer(raw, dtype=np.uint8).astype(np.float64) - 128.0) / 128.0
    elif width == 2:
        values = np.frombuffer(raw, dtype="<i2").astype(np.float64) / 32768.0
    elif width == 3:
        b = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        integer = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16)
        integer = (integer ^ 0x800000) - 0x800000
        values = integer.astype(np.float64) / 8388608.0
    elif width == 4:
        values = np.frombuffer(raw, dtype="<i4").astype(np.float64) / 2147483648.0
    else:
        raise ValueError(f"unsupported integer PCM width: {width}")
    return values.reshape(-1, channels)


def analyze(path: Path) -> tuple[dict, dict]:
    with wave.open(str(path), "rb") as wav:
        if wav.getcomptype() != "NONE":
            raise ValueError("only integer PCM WAV is accepted; no silent codec reinterpretation")
        channels, rate, width, frames = (
            wav.getnchannels(), wav.getframerate(), wav.getsampwidth(), wav.getnframes()
        )
        if frames <= 0 or channels <= 0 or rate <= 0:
            raise ValueError("empty/invalid WAV")
        sums = np.zeros(channels)
        squares = np.zeros(channels)
        peaks = np.zeros(channels)
        clipped = np.zeros(channels, dtype=np.int64)
        diff_squares = np.zeros(channels)
        previous = None
        seen = 0
        first = last = None
        first_active = last_active = None
        for raw in iter(lambda: wav.readframes(BLOCK_FRAMES), b""):
            block = pcm_decode(raw, width, channels)
            if first is None:
                first = block[0].copy()
            last = block[-1].copy()
            active = np.flatnonzero(np.any(block != 0.0, axis=1))
            if len(active):
                if first_active is None:
                    first_active = seen + int(active[0])
                last_active = seen + int(active[-1])
            sums += block.sum(axis=0)
            squares += np.square(block).sum(axis=0)
            peaks = np.maximum(peaks, np.max(np.abs(block), axis=0))
            # Count samples at either representable PCM endpoint; not a true-peak estimate.
            clipped += ((block <= -1.0) | (block >= 1.0 - 2.0 ** (1 - width * 8))).sum(axis=0)
            if previous is not None:
                diff_squares += np.square(block[0] - previous)
            diff_squares += np.square(np.diff(block, axis=0)).sum(axis=0)
            previous = last
            seen += len(block)
        if seen != frames:
            raise ValueError(f"truncated PCM: header {frames}, decoded {seen}")
        # Only two bounded edge buffers are held, never a whole long source.
        edge_frames = min(frames, rate * 2)
        wav.rewind()
        head = pcm_decode(wav.readframes(edge_frames), width, channels)
        wav.setpos(frames - edge_frames)
        tail = pcm_decode(wav.readframes(edge_frames), width, channels)
    rms_channels = np.sqrt(squares / frames)
    rms = float(np.sqrt(squares.sum() / (frames * channels)))
    peak = float(peaks.max())
    short = min(edge_frames, max(1, int(rate * 0.05)))
    head_rms = float(np.sqrt(np.mean(np.square(head[:short]))))
    tail_rms = float(np.sqrt(np.mean(np.square(tail[-short:]))))
    correlation_denominator = float(np.linalg.norm(head) * np.linalg.norm(tail))
    correlation = float(np.sum(head * tail) / correlation_denominator) if correlation_denominator > 0 else None
    seam = float(np.max(np.abs(first - last)))
    interior_diff_rms = float(np.sqrt(diff_squares.sum() / max(1, (frames - 1) * channels)))
    return ({
        "container": "wav", "codec": f"pcm_s{width * 8}le" if width > 1 else "pcm_u8",
        "sample_rate_hz": rate, "channels": channels, "bits_per_sample": width * 8,
        "frames": frames, "duration_s": frames / rate,
    }, {
        "method": "streamed integer PCM; combined-channel unweighted RMS and sample peaks",
        "rms_dbfs": db(rms), "sample_peak_dbfs": db(peak),
        "crest_factor_db": db(peak / rms) if rms > 0 else None,
        "channel_rms_dbfs": [db(float(x)) for x in rms_channels],
        "channel_peak_dbfs": [db(float(x)) for x in peaks],
        "dc_offset_per_channel": (sums / frames).tolist(),
        "full_scale_sample_count_per_channel": clipped.tolist(),
        "loop_seam": {
            "wrap_jump_peak_dbfs": db(seam),
            "wrap_jump_to_interior_difference_rms": seam / interior_diff_rms if interior_diff_rms > 0 else None,
            "head_50ms_rms_dbfs": db(head_rms), "tail_50ms_rms_dbfs": db(tail_rms),
            "head_tail_50ms_rms_delta_db": db(head_rms / tail_rms) if tail_rms > 0 else None,
            "head_tail_correlation_2s": correlation,
            "edge_window_frames": edge_frames,
            "leading_digital_silence_s": first_active / rate if first_active is not None else frames / rate,
            "trailing_digital_silence_s": (frames - 1 - last_active) / rate if last_active is not None else frames / rate,
            "interpretation": "diagnostic only; discontinuity is not an audition or seamless-loop certification",
        },
        "true_peak_measured": False, "lufs_measured": False, "listened": False,
    })


def filename_metadata(relative: str) -> dict:
    """Parse declared filename labels, not acoustic tempo or harmonic content."""
    clean = re.sub(r" \(\d+\)$", "", Path(relative).stem)
    bpm_labels = []
    patterns = [
        r"(?<![A-Za-z0-9.])(?:bpm)[_ -]*(\d{2,3}(?:\.\d+)?)(?![A-Za-z0-9.])",
        r"(?<![A-Za-z0-9.])(\d{2,3}(?:\.\d+)?)[_ -]*bpm(?![A-Za-z0-9.])",
        r"(?<![A-Za-z0-9.])(?:mus|drm|fxlp|syn|top|bs|perc)(\d{2,3}(?:\.\d+)?)(?![A-Za-z0-9.])",
        r"(?<![A-Za-z0-9.])(\d{2,3}(?:\.\d+)?)(?![A-Za-z0-9.])",
    ]
    for pattern in patterns:
        for match in re.finditer(pattern, clean, re.IGNORECASE):
            value = float(match.group(1))
            # Standalone numbers follow the existing vendor tempo convention. Explicit BPM
            # labels can cover slower/faster tempos; sample indices and808 are not BPM.
            explicit = 'bpm' in match.group().lower()
            if not ((20 <= value <= 400) if explicit else (60 <= value <= 249)):
                continue
            label = {"value": int(value) if value.is_integer() else value, "token": match.group()}
            if label not in bpm_labels:
                bpm_labels.append(label)
    key_labels = []
    pattern = r"(?<![A-Za-z0-9])([A-G])([#b♯♭]?)(?:[_ -]*(major|minor|maj|min|m))?(?![A-Za-z0-9#♯♭])"
    for match in re.finditer(pattern, clean, re.IGNORECASE):
        mode_token = match.group(3)
        # A lowercase standalone article is not a musical label. Explicit lowercase keys
        # such as c#minor remain accepted, as do uppercase bare pitch-class labels.
        if match.group(1).islower() and mode_token is None:
            continue
        tonic = match.group(1).upper() + match.group(2).lower().replace('♯', '#').replace('♭', 'b')
        mode = ("major" if mode_token == "M" or mode_token and mode_token.lower() in {"maj", "major"}
                else "minor" if mode_token else None)
        label = {"tonic": tonic, "mode": mode, "token": match.group()}
        if label not in key_labels:
            key_labels.append(label)
    bpms = sorted({x["value"] for x in bpm_labels})
    bpm = bpms[0] if len(bpms) == 1 else None
    resolved = resolve_key_labels(key_labels)
    return {
        "bpm": {"value": bpm, "basis": "filename label only" if bpm_labels else "not supplied",
                "confidence": "filename_label_unverified" if bpm is not None else None,
                "evidence": bpm_labels[0]["token"] if bpm_labels else None, "labels": bpm_labels,
                "conflict": len(bpms) > 1},
        "key": {"tonic": resolved["tonic"], "mode": resolved["mode"],
                "basis": "filename note label only" if key_labels else "not supplied",
                "confidence": "filename_label_unverified" if resolved["tonic"] else None,
                "evidence": key_labels[0]["token"] if key_labels else None, "labels": key_labels,
                "conflict": resolved["conflict"]},
        "tempo_analyzed": False, "pitch_analyzed": False,
    }


def resolve_key_labels(labels: list[dict]) -> dict:
    pitch = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
    def pc(tonic):
        return (pitch[tonic[0]] + (1 if tonic[1:] == '#' else -1 if tonic[1:] == 'b' else 0)) % 12
    tonics = {pc(x["tonic"]) for x in labels}
    modes = {x["mode"] for x in labels if x["mode"] is not None}
    conflict = len(tonics) > 1 or len(modes) > 1
    tonic = labels[0]["tonic"] if labels and not conflict else None
    mode = next(iter(modes)) if len(modes) == 1 and not conflict else None
    return {"tonic": tonic, "mode": mode, "conflict": conflict,
            "musical_key": f"{tonic} {mode}" if tonic and mode else None}


def apply_musical_metadata(asset: dict) -> None:
    """Preserve labels across verified same-content renames; conflicting labels stay unknown."""
    sources = list(dict.fromkeys([asset["source_path"], *asset.get("aliases", []),
                                 *asset.get("historical_source_paths", [])]))
    tempos, keys = [], []
    for index, relative in enumerate(sources):
        parsed = filename_metadata(relative)
        source_kind = "current_filename" if index == 0 else "current_alias" if relative in asset.get("aliases", []) else "same_content_historical_filename"
        for name, dest in [("bpm", tempos), ("key", keys)]:
            dest.extend({**label, "source_path": relative, "source_kind": source_kind}
                        for label in parsed[name]["labels"])
    bpms = sorted({x["value"] for x in tempos})
    bpm = bpms[0] if len(bpms) == 1 else None
    resolved = resolve_key_labels(keys)
    asset["filename_metadata"] = filename_metadata(asset["source_path"])
    asset["bpm"] = bpm
    asset["musical_key"] = resolved["musical_key"]
    asset["key"] = (resolved["tonic"] + (" " + resolved["mode"] if resolved["mode"] else "")) if resolved["tonic"] else None
    asset["musical_metadata"] = {
        "version": 1, "tempo_analyzed": False, "pitch_analyzed": False,
        "resolution": "unanimous filename labels across same-content paths; conflicts are unknown; bare notes never imply major/minor",
        "bpm": {"value": bpm, "status": "conflict" if len(bpms) > 1 else "filename_label" if bpm is not None else "unknown",
                "certainty": "filename_declared_unverified" if bpm is not None else None, "values": bpms, "sources": tempos},
        "key": {**resolved, "status": "conflict" if resolved["conflict"] else "filename_key" if resolved["musical_key"] else "tonic_only" if resolved["tonic"] else "unknown",
                "certainty": "filename_declared_unverified" if resolved["tonic"] else None, "sources": keys},
    }


def preserve_musical_history(result: dict, previous: dict) -> None:
    previous_by_id = {a["id"]: a for a in previous.get("assets", [])}
    for asset in result["assets"]:
        old = previous_by_id.get(asset["id"])
        if old and old.get("sha256") == asset.get("sha256"):
            current = {asset["source_path"], *asset.get("aliases", [])}
            paths = [*old.get("historical_source_paths", []), old["source_path"], *old.get("aliases", [])]
            asset["historical_source_paths"] = list(dict.fromkeys(x for x in paths if x not in current))
        apply_musical_metadata(asset)
        if asset["bpm"] is not None and asset["loop"]["enabled"]:
            asset["loop"]["crossfade_s"] = 0.0
            asset["loop"]["crossfade_curve"] = None
        apply_filename_loop_instruction(asset)


def apply_filename_loop_instruction(asset: dict) -> None:
    """A standalone filename/alias `_loop` token is the user's explicit repeat instruction.

    This does not activate an otherwise disabled asset or invent a shorter musical segment.
    A terminal death presentation is a runtime finite-playback exception, not altered metadata.
    """
    sources = []
    for relative in dict.fromkeys([asset["source_path"], *asset.get("aliases", []), *asset.get("historical_source_paths", [])]):
        stem = re.sub(r" \(\d+\)$", "", Path(relative).stem.lower())
        if re.search(r"(?:^|_)loop(?:_|$)", stem):
            sources.append(relative)
    asset["explicit_filename_loop"] = bool(sources)
    asset["filename_loop_sources"] = sources
    if sources:
        asset["loop"]["enabled"] = True
        asset["loop"]["decision_basis"] = "user-defined standalone _loop filename/alias token; repeat complete source; no invented trim"
        # Preserve the explicitly authored complete-file period even when BPM is absent
        # from the canonical name (it may occur only in an alias). Runtime edge envelopes
        # can suppress a click without consuming overlapping source frames.
        asset["loop"]["crossfade_s"] = 0.0
        asset["loop"]["crossfade_curve"] = None


def filename_usage(relative: str) -> dict:
    """Explicit current filename intent only; historical musical labels cannot reopen a role."""
    name = context_token(re.sub(r" \(\d+\)$", "", Path(relative).stem))
    tokens = set(name.split("_"))
    contexts = [tag for tag in ("night", "winter") if tag in tokens]
    if "inside" in tokens and "tank" in tokens:
        contexts.append("inside")
    if "combat" in tokens:
        contexts.append("combat")
    roles = None
    if Path(relative).parts[0].lower() == "music":
        if "combat" in tokens:
            roles = ["combat"]
        elif "driving" in tokens:
            roles = ["explore", "driving"] if "calm" in tokens else ["driving"]
            if "calm" not in tokens:
                contexts.append("driving")
        elif "calm" in tokens:
            roles = ["explore"]
    return {"roles": roles, "contexts": contexts,
            "basis": "explicit standalone current filename tokens; not inferred enemy awareness or acoustic analysis",
            "unbound_descriptive_tokens": [tag for tag in ("lonelytank", "layer") if tag in tokens]}


def apply_filename_usage(asset: dict) -> None:
    """Reapply current source/alias folder intent to a saved full scan without rescanning PCM."""
    assignments = [folder_assignment(path) for path in [asset["source_path"], *asset.get("aliases", [])]]
    asset["folder_assignments"] = assignments
    primary = assignments[0]
    asset["role"] = primary["role"]
    asset["contexts"] = primary["contexts"]
    memberships = role_memberships(assignments)
    for assignment in assignments:
        if not assignment["default_enabled"]:
            continue
        for role in assignment["filename_usage"]["roles"] or []:
            contexts = memberships["role_contexts"].setdefault(role, [])
            if assignment["contexts"] not in contexts:
                contexts.append(assignment["contexts"])
    memberships["role_aliases"] = list(memberships["role_contexts"])
    asset.update(memberships)
    asset["filename_usage"] = primary["filename_usage"]
    if primary["role"] in {"unused", "scene"}:
        asset["default_enabled"] = False
        asset["explicit_import_requested"] = False
    assignment = runtime_assignment(asset["source_path"])
    asset.pop("runtime_assignment", None)
    if assignment:
        asset["runtime_assignment"] = assignment
        if assignment.get("enabled") is False:
            asset["default_enabled"] = False
            asset["explicit_import_requested"] = False


def classify(relative: str) -> tuple[str, list[str], str]:
    lower = relative.lower()
    folder = lower.split("/")[0]
    if folder == "unused":
        return "unused", ["do_not_activate"], "user explicitly excluded this folder from playback"
    if folder == "engine_hiss" or re.search(r"(?:^|_)(?:broken_engine(?:_hiss)?|engine_hiss|hissbroken)(?:_|$)", Path(lower).stem):
        return "engine_hiss", ["vehicle", "engine_damage", "hiss"], "explicit user broken-engine hiss filename/folder assignment; trigger requires actual source damage state"
    if folder == "heavy_blizzard" or re.search(r"(?:^|_)heavy_blizzard(?:_|$)", Path(lower).stem):
        return "blizzard", ["weather", "heavy_blizzard"], "explicit user heavy-blizzard folder/filename assignment"
    if folder in FOLDER_ROLES:
        role = FOLDER_ROLES[folder]
        intent = filename_usage(relative)
        if intent["roles"]:
            role = intent["roles"][0]
        tags = ["tank_cabin"] if role == "cabin" else [role, "user_selected_folder"]
        return role, tags, "user folder assignment"
    if folder in {"scene", "scenes"}:
        parts = Path(relative).parts
        return "scene", [context_token(parts[1])] if len(parts) > 2 else [], "user scene folder; explicit location binding required"
    if any(x in lower for x in ("coffee", "cafe", "playground", "airport", "farmhouse", "restaurant", "crowd", "conversation")):
        return "scene_bed", ["location_bed", "recording_content_unverified", "requires_context_review"], "filename inference"
    if any(x in lower for x in ("footsteps", "match_stick", "coin_box")):
        return "world_effect_candidate", ["foley", "requires_context_review"], "filename inference"
    if any(x in lower for x in ("kick", "snare", "hihat", "percussion", "_perc_", "_prc_")):
        return "score_percussion", ["percussion", "one_shot_or_loop_unverified"], "filename inference"
    if any(x in lower for x in ("_drum_", "_drm", "riddim", "_top")):
        return "score_rhythm", ["rhythmic_texture", "tempo_sync_requires_review"], "filename inference"
    if any(x in lower for x in ("_mus", "_syn", "_bass", "_bs", "_piano", "_guitar", "_chord", "tonal", "_synth")):
        return "score_tonal", ["tonal", "key_sync_requires_review"], "filename inference"
    if any(x in lower for x in ("wind", "birds", "campfire", "rain", "breeze", "field_atmos")):
        return "environment_bed_candidate", ["ambience", "location_season_review"], "filename inference"
    if any(x in lower for x in ("riser", "_hit", "_one_shot", "_fx_")):
        return "score_accent", ["accent", "requires_audition"], "filename inference"
    return "texture_candidate", ["unreviewed_texture", "do_not_autoactivate"], "filename inference"


def context_token(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.lower()).strip("_")


def folder_assignment(relative: str) -> dict:
    parts = Path(relative).parts
    category = parts[0]
    role, _, basis = classify(relative)
    intent = filename_usage(relative)
    return {"source_path": relative, "category_folder": category, "role": role,
            "contexts": list(dict.fromkeys(FOLDER_CONTEXTS.get(category, [])
                + [context_token(p) for p in parts[1:-1] if context_token(p)] + intent["contexts"])),
            "filename_usage": intent,
            "default_enabled": category in FOLDER_ROLES, "basis": basis}


def role_memberships(assignments: list[dict]) -> dict:
    """One content ID can serve several authored roles without duplicating PCM or pool weight."""
    contexts = {}
    for index, assignment in enumerate(assignments):
        if index and not assignment["default_enabled"]:
            continue
        alternatives = contexts.setdefault(assignment["role"], [])
        if assignment["contexts"] not in alternatives:
            alternatives.append(assignment["contexts"])
    return {"role_aliases": list(contexts), "role_contexts": contexts}


def catalog(source_root: Path, night_target_dbfs: float, peak_limit_dbfs: float,
            enabled_sources: tuple[str, ...] = (), allow_moving: bool = False,
            folders: tuple[str, ...] | None = None) -> dict:
    if folders is not None and any(folder not in FOLDER_ROLES for folder in folders):
        raise ValueError("targeted refresh accepts only named top-level category folders")
    snapshot = source_snapshot(source_root, folders)
    paths = sorted(source_root / relative for relative in snapshot)
    requested = set(enabled_sources)
    available = {p.relative_to(source_root).as_posix() for p in paths}
    if not requested <= available:
        raise ValueError(f"enabled source is not a relative file in this library: {sorted(requested - available)}")
    grouped: dict[str, list[Path]] = defaultdict(list)
    warnings = []
    for path in paths:
        try:
            grouped[sha256(path)].append(path)
        except FileNotFoundError:
            warnings.append(f"source moved before hashing; skipped: {path.relative_to(source_root)}")
    assets = []
    failures = []
    for digest, duplicates in sorted(grouped.items()):
        duplicates.sort(key=lambda p: (p.relative_to(source_root).parts[0] not in FOLDER_ROLES,
                                       not p.relative_to(source_root).as_posix().startswith("night_ambiance/"),
                                       bool(re.search(r" \(\d+\)$", p.stem)), str(p.relative_to(source_root))))
        remaining = [p for p in duplicates if p.exists()]
        if not remaining:
            warnings.append(f"source moved after hashing; skipped content: {digest}")
            continue
        path = remaining[0]
        relative = path.relative_to(source_root).as_posix()
        try:
            format_info, analysis = analyze(path)
        except FileNotFoundError:
            warnings.append(f"source moved before decoding; skipped: {relative}")
            continue
        except Exception as error:
            failures.append({"path": relative, "error": str(error), "sha256": digest})
            continue
        role, tags, role_basis = classify(relative)
        night = role == "night"
        selected_folder = relative.split("/")[0] in FOLDER_ROLES or role in {"blizzard", "engine_hiss"}
        explicitly_enabled = any(p.relative_to(source_root).as_posix() in requested for p in duplicates)
        duration = format_info["duration_s"]
        gain = 0.0
        gain_target = -26.0 if role in {"explore", "combat", "threat", "music", "intro", "stinger"} else night_target_dbfs
        if selected_folder and analysis["rms_dbfs"] is not None:
            gain = min(0.0, gain_target - analysis["rms_dbfs"], peak_limit_dbfs - analysis["sample_peak_dbfs"])
        correlation = analysis["loop_seam"]["head_tail_correlation_2s"]
        filename = filename_metadata(relative)
        crossfade = min(2.0, duration * 0.1) if selected_folder and filename["bpm"]["value"] is None else 0.0
        if night:
            crossfade = 2.0
        looping = selected_folder and role not in {"effect", "stinger", "intro", "engine_hiss"}
        if not looping:
            crossfade = 0.0
        tonic = filename["key"]["tonic"]
        mode = filename["key"]["mode"]
        category_folder = Path(relative).parts[0]
        folder_contexts = list(Path(relative).parts[1:-1])
        contexts = list(dict.fromkeys(FOLDER_CONTEXTS.get(category_folder, [])
                      + [context_token(value) for value in folder_contexts if context_token(value)]))
        assignments = [folder_assignment(p.relative_to(source_root).as_posix()) for p in duplicates]
        assets.append({
            "id": "audio_" + digest[:24], "sha256": digest,
            "source_path": relative,
            "aliases": [p.relative_to(source_root).as_posix() for p in duplicates[1:]],
            "folder_assignments": assignments, **role_memberships(assignments),
            "bytes": snapshot[relative][0], "role": role, "tags": tags, "role_basis": role_basis,
            "category_folder": category_folder, "folder_contexts": folder_contexts,
            "contexts": contexts,
            "context_alias_basis": "user-approved summer folder uses current spring runtime season; source folder unchanged"
                if category_folder == "summer_atmosphere" else None,
            "default_enabled": selected_folder or explicitly_enabled, "explicit_import_requested": explicitly_enabled,
            "selection_weight": 1.0,
            "format": format_info, "filename_metadata": filename,
            "bpm": filename["bpm"]["value"],
            "key": (tonic + (" " + mode if mode else "")) if tonic else None,
            "scene_context": {"location_tags": [tag for tag in ("coffee", "cafe", "playground", "airport", "farmhouse", "restaurant", "crowd", "conversation", "cave", "forest", "arctic") if tag in relative.lower()],
                              "content_verified_by_listening": False,
                              "can_be_layered_with": ["room_tone", "spot_effect", "music", "dialogue"],
                              "binding_required": not selected_folder},
            "analysis": analysis,
            "gain": {"suggested_db": gain, "apply_automatically": selected_folder,
                     "basis": "attenuation-only RMS/peak alignment for user-assigned folder" if selected_folder else "unchanged until role and listening review",
                     "target_rms_dbfs": gain_target if selected_folder else None,
                     "sample_peak_limit_dbfs": peak_limit_dbfs if selected_folder else None,
                     "runtime_bus_gain_is_separate": True},
            "loop": {"enabled": looping, "start_frame": 0, "end_frame_exclusive": format_info["frames"],
                     "crossfade_s": crossfade,
                     "crossfade_curve": "equal_gain" if crossfade > 0 and correlation is not None and correlation > 0.3 else "equal_power" if crossfade > 0 else None,
                     "trimmed": False, "audition_required": True,
                     "decision_basis": "user folder explicitly assigned one-shot role; full source retained" if selected_folder and not looping else "full original duration; BPM-labelled period preserved without overlap" if selected_folder and filename["bpm"]["value"] is not None else "full original duration; bounded edge overlap suggested from ambience role and measured edge correlation" if selected_folder else "not activated; filename loop claims require role review"},
            "rights": {"declaration": "user_declared_owned_and_authorized_for_game", "source_vendor_declared": "Splice",
                       "independent_license_document_verified": False},
        })
    for entry in assets:
        apply_filename_usage(entry)
        apply_musical_metadata(entry)
        # Alias-provided tempo has the same exact-period requirement as a primary label.
        if entry["bpm"] is not None and entry["loop"]["enabled"]:
            entry["loop"]["crossfade_s"] = 0.0
            entry["loop"]["crossfade_curve"] = None
        apply_filename_loop_instruction(entry)
    ids = [a["id"] for a in assets]
    if len(ids) != len(set(ids)):
        raise ValueError("stable ID collision; extend content hash before writing")
    if source_snapshot(source_root, folders) != snapshot:
        if not allow_moving:
            raise SourceChangedError("source files moved or changed during analysis; existing manifest remains untouched")
        warnings.append("source library was changing; this is a partial snapshot; rerun after organization finishes")
        kept = []
        for entry in assets:
            if (source_root / entry["source_path"]).exists():
                kept.append(entry)
            else:
                warnings.append(f"source moved after decoding; skipped: {entry['source_path']}")
        assets = kept
    return {
        "version": 1, "schema": "panzer.audio.library.v1", "source_root": str(source_root),
        "provenance": {"declaration_date": "2026-10-02", "rights": "user explicitly confirms game usage rights",
                       "originals_modified": False, "listened": False,
                       "scope": "content inventory and numerical PCM analysis; not perceptual approval or license authentication"},
        "inventory": {"source_files": len(paths), "source_bytes": sum(size for size, _ in snapshot.values()),
                      "unique_assets": len(assets), "duplicate_files": sum(len(a["aliases"]) for a in assets),
                      "unique_bytes": sum(a["bytes"] for a in assets),
                      "roles": dict(sorted(Counter(a["role"] for a in assets).items())),
                      "failures": failures, "warnings": warnings},
        "policies": {"night": {"pool_role": "night", "one_bed_per_night": True,
                               "deduplicate_content": True, "new_run_seed_changes_selection": True,
                               "avoid_immediate_repeat_when_possible": True,
                               "weather_gain_and_combat_ducking_are_runtime_policy": True},
                     "explicit_folder_roles": FOLDER_ROLES,
                     "other_assets": "catalog only; no automatic cave, fauna, foley or musical activation outside user-assigned folders"},
        "assets": sorted(assets, key=lambda a: a["source_path"]),
    }


def import_enabled(manifest: dict, library_root: Path, create_enabled: bool = True) -> None:
    """Derive only enabled clips; attenuation belongs to the runtime, never to these samples."""
    root = Path(manifest["source_root"])
    recipe = {"tool": "ffmpeg", "sample_rate_hz": 44_100, "channels": 2,
              "codec": "pcm_s16le", "dither": "triangular", "gain_baked_db": 0.0,
              "trimmed": False}
    for entry in manifest["assets"]:
        source = root / entry["source_path"]
        playback_path = f"clips/{entry['id']}.wav"
        target = library_root / playback_path
        metadata_path = target.with_suffix(".source.json")
        existing = json.loads(metadata_path.read_text()) if metadata_path.exists() else None
        current = (existing is not None and existing.get("source_sha256") == entry["sha256"]
                   and existing.get("recipe") == recipe and target.exists()
                   and existing.get("playback_sha256") == sha256(target))
        if not current and not (create_enabled and entry["default_enabled"]):
            entry["playback_available"] = False
            continue
        if not current:
            # Refuse to replace an unregistered file, even under our content-derived filename.
            if target.exists() and existing is None:
                raise ValueError(f"unregistered output already exists: {target}")
            if not source.exists():
                manifest["inventory"]["warnings"].append(f"source moved before import; skipped: {entry['source_path']}")
                entry["playback_available"] = False
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(prefix=target.stem + "_", suffix=".wav", dir=target.parent, delete=False) as temporary:
                temporary_path = Path(temporary.name)
            command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
                       "-i", str(source), "-map", "0:a:0", "-map_metadata", "-1",
                       "-af", "aresample=44100:dither_method=triangular", "-ac", "2",
                       "-c:a", "pcm_s16le", "-bitexact", str(temporary_path)]
            try:
                subprocess.run(command, check=True, timeout=120)
                temporary_path.replace(target)
            finally:
                temporary_path.unlink(missing_ok=True)
            existing = {"source_sha256": entry["sha256"], "source_path": str(source),
                        "playback_sha256": sha256(target), "recipe": recipe,
                        "command": command[:-1] + [str(target)]}
            metadata_path.write_text(json.dumps(existing, indent=2) + "\n")
        playback_format, playback_analysis = analyze(target)
        entry["playback_format"] = playback_format
        entry["playback_path"] = playback_path
        entry["playback_available"] = True
        entry["playback_sha256"] = existing["playback_sha256"]
        entry["playback_bytes"] = target.stat().st_size
        entry["playback_analysis"] = playback_analysis
        entry["loop"]["end_frame_exclusive"] = playback_format["frames"]
        entry["loop"]["frame_coordinate_system"] = "playback sample frames"
        entry["derivation"] = recipe
    manifest["inventory"]["enabled_derived_files"] = sum(a["default_enabled"] and a.get("playback_available", False) for a in manifest["assets"])
    manifest["inventory"]["available_derived_files"] = sum(a.get("playback_available", False) for a in manifest["assets"])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("/home/alex/panzer/panzer_sounds"))
    parser.add_argument("--output", type=Path, default=Path("assets/audio/library/manifest.json"))
    parser.add_argument("--night-target-rms-dbfs", type=float, default=-30.0)
    parser.add_argument("--night-peak-limit-dbfs", type=float, default=-12.0)
    parser.add_argument("--import-enabled", action="store_true", help="derive only default-enabled clips as stereo44.1k PCM16 WAV")
    parser.add_argument("--enable-source", action="append", default=[], metavar="RELATIVE_PATH",
                        help="explicitly enable this relative source (repeatable); keeps inferred role until runtime binding overrides it")
    args = parser.parse_args()
    for attempt in range(3):
        try:
            result = catalog(args.source.resolve(strict=True), args.night_target_rms_dbfs, args.night_peak_limit_dbfs,
                             tuple(args.enable_source), allow_moving=attempt == 2)
            break
        except (SourceChangedError, FileNotFoundError) as error:
            if attempt == 2:
                raise SystemExit(f"library still changing after three bounded attempts: {error}") from error
            print(f"source changed; retrying complete snapshot ({attempt + 1}/3): {error}", file=sys.stderr)
            time.sleep(0.5)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists():
        preserve_musical_history(result, json.loads(args.output.read_text()))
    import_enabled(result, args.output.parent, create_enabled=args.import_enabled)
    temporary = args.output.with_suffix(args.output.suffix + ".tmp")
    temporary.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    temporary.replace(args.output)
    print(json.dumps(result["inventory"], indent=2))
    if result["inventory"]["failures"]:
        raise SystemExit("catalog includes failed assets; review failures before runtime adoption")


if __name__ == "__main__":
    main()
