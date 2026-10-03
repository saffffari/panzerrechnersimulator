#!/usr/bin/env python3
"""Provisional numerical cue policies; no listening or original-file edits.

This only writes a review catalog. It does not modify soundscape.json or activate events.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import math
from pathlib import Path
import re
import wave

import numpy as np

from catalog_panzer_sounds import apply_filename_loop_instruction, db, pcm_decode, sha256
from soundscape_routing import assignment as runtime_assignment

# User-approved cue timing is content-specific, so a future filename change cannot
# transfer this measured peak to a different recording. No playback PCM is trimmed.
TRANSITION_RISER_ID = "audio_4a383409ba81f8fd25d2ab1c"


def envelope(path: Path) -> dict:
    """Read quarter-second windows, never decode an entire source into memory."""
    with wave.open(str(path), "rb") as wav:
        rate, channels, width = wav.getframerate(), wav.getnchannels(), wav.getsampwidth()
        frames = wav.getnframes()
        window_frames = max(1, round(rate / 4))
        powers, counts = [], []
        for raw in iter(lambda: wav.readframes(window_frames), b""):
            block = pcm_decode(raw, width, channels)
            powers.append(float(np.mean(np.square(block))))
            counts.append(len(block))
    p = np.asarray(powers)
    level = np.sqrt(p)
    # A numerical active-window threshold, not an auditory masking or loudness model.
    active = level >= max(1e-5, float(level.max()) / 100)
    active_levels = level[active]
    last_n = max(1, math.ceil(2 * rate / window_frames))
    head_rms = float(np.sqrt(np.average(p[:last_n], weights=counts[:last_n])))
    tail_rms = float(np.sqrt(np.average(p[-last_n:], weights=counts[-last_n:])))
    return {
        "window_s": window_frames / rate,
        "window_rms_dbfs": [db(float(x)) for x in level],
        "duration_s": frames / rate,
        "first_2s_rms_dbfs": db(head_rms), "last_2s_rms_dbfs": db(tail_rms),
        "peak_window_rms_dbfs": db(float(level.max())),
        "peak_window_at_s": int(np.argmax(level)) * window_frames / rate,
        "active_window_threshold_dbfs": db(max(1e-5, float(level.max()) / 100)),
        "active_window_fraction": float(np.mean(active)),
        "active_window_p10_dbfs": db(float(np.percentile(active_levels, 10))) if len(active_levels) else None,
        "active_window_p90_dbfs": db(float(np.percentile(active_levels, 90))) if len(active_levels) else None,
        "head_tail_level_is_not_proof_of_a_musical_phrase": True,
    }


def suggest(asset: dict, measured: dict) -> dict:
    apply_filename_loop_instruction(asset)
    role = asset["role"]
    routing = asset.get("runtime_assignment") or runtime_assignment(asset["source_path"])
    phrase = bool(routing and routing.get("finite_foreground_phrase"))
    if phrase:
        role = routing["role"]
    name = re.sub(r"[^a-z0-9#]+", "_", Path(asset["source_path"]).stem.lower()).strip("_")
    # The explicit named event stays outside the continuous combat score pool; the
    # runtime decides when an actual contact episode permits that event.
    combat_start = name == "combat_start"
    transition_riser = asset["id"] == TRANSITION_RISER_ID
    unbound_accent = bool(re.fullmatch(r"(?:surprise_contact|riser_transition)\d*", name))
    solar_events = [event for event in ("sunrise", "sunset", "dusk") if asset["source_path"].startswith(event + "/")]
    if combat_start or transition_riser:
        role = "stinger"
    elif unbound_accent and not phrase:
        role = "effect"
    bpm = asset.get("bpm")
    beats = measured["duration_s"] * bpm / 60 if bpm else None
    bar_residual = abs(beats - round(beats / 4) * 4) if beats else None
    explicit_loop = asset["explicit_filename_loop"]
    explicit_fxlp = bool(re.search(r"fxlp\d", name))
    periodic = bpm is not None and (explicit_fxlp or (bar_residual is not None and bar_residual < 0.03))
    peak = measured["peak_window_rms_dbfs"]
    tail = measured["last_2s_rms_dbfs"]
    last = measured["window_rms_dbfs"][-1]
    end_drop = tail - peak if tail is not None and peak is not None else None
    faded_end = (end_drop is not None and end_drop <= -20) and (last is None or last <= peak - 30)
    basis = []
    if transition_riser:
        looping, fade_in, fade_out = False, 0.08, 0.02
        basis.append("user-approved higher-musical-intensity transition riser; content-specific sample peak5.404943s and preferred4s lead; musical programme transition scheduling belongs to runtime; preserve natural source tail")
    elif solar_events:
        looping, fade_in, fade_out = False, 0.15, 1.0
        basis.append("user-assigned solar event: finite complete-file score cue; trigger belongs to the named solar threshold, not elapsed gameplay or mode entry; dusk is separate from sunset")
    elif phrase:
        looping, fade_in, fade_out = False, 0.02, 0.02
        basis.append("explicit finite foreground score phrase through existing programme; no physical event, arbitrary overlay or new scheduler; complete source and unknown musical metadata retained")
    elif explicit_loop:
        looping, fade_in, fade_out = True, 0.08, 1.0
        basis.append("user explicitly defines standalone _loop in original filename or alias as repeat; repeat full source regardless of inferred musical form")
        if role == "night":
            fade_in, fade_out = 5.0, 5.0
        elif role == "cabin":
            fade_in, fade_out = 2.0, 3.0
        elif role in {"effect", "stinger"}:
            fade_in, fade_out = 0.005, 0.02
            basis.append("terminal Death playback remains finite by runtime presentation rule even if this reusable cue repeats elsewhere")
        elif role == "engine_hiss":
            fade_in, fade_out = 0.08, 0.4
            basis.append("explicit repeating physical engine-damage source; runtime owns spatial gain and stopping on repair")
    elif role == "night":
        looping, fade_in, fade_out = True, 5.0, 5.0
        basis.append("explicit one-bed-per-night requirement; loop the same selected recording throughout the night")
    elif role == "blizzard":
        looping, fade_in, fade_out = True, 3.0, 4.0
        basis.append("explicit heavy-blizzard physical bed: runtime gates by actual near-whiteout weather; never joins general ambience or score pools")
    elif role == "engine_hiss":
        looping, fade_in, fade_out = False, 0.08, 0.4
        basis.append("named engine-damage source without explicit repeat marker: preserve complete finite cooldown; no invented sustain trim or generic random effect trigger")
    elif role in {"intro", "stinger", "effect"}:
        looping = False
        fade_in, fade_out = {"intro": (0.1, 2.0), "stinger": (0.005, 0.25), "effect": (0.005, 0.02)}[role]
        if combat_start:
            fade_in = 0.02
        basis.append("user-assigned one-shot category; preserve original end and do not retrigger every game update")
    elif role == "cabin":
        looping, fade_in, fade_out = True, 2.0, 3.0
        basis.append("independent cabin-bed category: retain continuous availability; a faded texture is not proof of a discrete event")
    elif role == "day":
        looping, fade_in, fade_out = True, 3.0, 4.0
        basis.append("explicit seasonal atmosphere category; loop its full source with the catalog seam policy")
    elif re.search(r"(?:^|_)(?:one_shot|start|transition|contact)(?:_|$)", name):
        looping, fade_in, fade_out = False, 0.02, 0.25
        basis.append("filename describes a start/contact/transition accent; provisional finite cue, no automatic gameplay trigger inferred")
    elif periodic:
        looping, fade_in, fade_out = True, 0.08, 1.0
        basis.append("filename BPM plus explicit loop token or near-integer four-beat period; no duration-shortening overlap")
    elif "ancient_instrument" in name:
        looping, fade_in, fade_out = False, 0.05, 1.0
        basis.append("named instrument phrase has a measured low-level ending; provisionally let the phrase finish")
    elif role == "music" or faded_end:
        looping, fade_in, fade_out = False, 0.05 if faded_end else 0.5, 1.0
        basis.append("melodic program or strongly faded ending without explicit periodic-loop evidence; provisionally play once")
    else:
        looping, fade_in, fade_out = True, 2.0, 3.0
        basis.append("texture/score-bed category lacks strong numerical one-shot-ending evidence; provisional full-file loop")
    crossfade = asset["loop"]["crossfade_s"] if looping else 0.0
    if periodic or explicit_loop:
        crossfade = 0.0
    if faded_end:
        basis.append(f"last2s window RMS is {end_drop:.2f}dB below the loudest quarter-second window; this is not musical-form recognition")
    if explicit_loop and not periodic:
        basis.append("repeat instruction honored; beat alignment/possible end trim needs authoring because complete duration is unverified or not an integer four-beat period")
    catalog_gain = asset["gain"]["suggested_db"]
    # Keep within-file dynamics; do not turn a quiet passage into a new normalization target.
    extra_gain = -3.0 if (asset["analysis"]["crest_factor_db"] or 100) < 8.0 else 0.0
    basis.append("catalog attenuation already aligns RMS/peak; gain_db is an additional runtime offset, never baked compression")
    if extra_gain:
        basis.append("dense low-crest recording gets a provisional extra3dB attenuation to retain mix headroom")
    contexts = list(asset.get("contexts", []))
    if phrase and role == "combat" and "combat" not in contexts:
        contexts.append("combat")
    single_role = combat_start or unbound_accent or transition_riser or phrase
    result = {
        "id": asset["id"], "source_path": asset["source_path"], "source_sha256": asset["sha256"],
        "role": role, "catalog_role": asset["role"], "contexts": contexts,
        "role_aliases": [role] if single_role else asset.get("role_aliases", [role]),
        "role_contexts": {role: [contexts]} if single_role else asset.get("role_contexts", {}),
        "runtime_assignment": routing,
        "folder_assignments": asset.get("folder_assignments", []),
        "historical_source_paths": asset.get("historical_source_paths", []),
        "filename_usage": asset.get("filename_usage", {}),
        "solar_events": solar_events,
        "loop": looping, "fade_in_s": fade_in, "fade_out_s": fade_out, "gain_db": extra_gain,
        "catalog_gain_db": catalog_gain, "total_source_gain_db": catalog_gain + extra_gain,
        "crossfade_s": crossfade,
        "crossfade_curve": asset["loop"]["crossfade_curve"] if crossfade else None,
        "basis": basis, "envelope": measured,
        "filename_bpm": bpm, "filename_bpm_unverified": bpm is not None,
        "bpm": bpm, "musical_key": asset.get("musical_key"),
        "musical_metadata": asset.get("musical_metadata", {}),
        "estimated_beats_from_filename_bpm": beats, "four_beat_period_residual_beats": bar_residual,
        "periodic_loop_evidence": periodic, "numerically_faded_ending": faded_end,
        "explicit_filename_loop": explicit_loop, "filename_loop_sources": asset["filename_loop_sources"],
        "loop_region_requires_authoring": explicit_loop and not periodic,
        "provisional": True, "listened": False, "originals_modified": False,
        "acceptance": ["audition attack and natural ending", "check seam or end-fade in full mixer", "compare against guns, dialogue, and quiet cabin"],
    }
    if transition_riser:
        result.update(riser_peak_s=5.404943, riser_lead_s=4.0)
    levels = measured["window_rms_dbfs"][-12:]
    abrupt = max([0.0] + [a - (b if b is not None else -160.0)
        for a, b in zip(levels, levels[1:]) if a is not None and peak is not None and a > peak - 30])
    result["terminal_quarter_second_drop_db"] = abrupt
    # Numerical dropoff can identify candidates, not prove that a recording is dry.
    # Restrict the extra rack to the ending of finite score phrases with appreciable
    # energy shortly before a steep terminal drop. Never process continuous beds/drums
    # or already-long quiet death/synth decay simply because a reverb option exists.
    energetic_eof = last is not None and peak is not None and last > peak - 18
    result["energetic_eof_window"] = energetic_eof
    tail_candidate = (not looping and (role in {"intro", "explore", "music", "combat", "threat"} or combat_start or name == "surprise_contact" or bool(solar_events))
                      and measured["duration_s"] > 4 and (abrupt > 24 or energetic_eof)
                      and end_drop is not None and end_drop > -20)
    if tail_candidate:
        result["rack"] = {"preset": "dry", "wet": 0.12 if role == "explore" else 0.08,
                          "decay_s": 1.2 if role == "explore" else 1.1,
                          "predelay_ms": 14.0, "damping_hz": 4500.0,
                          "lowpass_hz": 22050.0, "highpass_hz": 0.0,
                          "tail_only_s": min(3.0, measured["duration_s"] * 0.2)}
        result["basis"].append(f"provisional ending-only rack: terminal quarter-second drop {abrupt:.2f}dB; energetic final window={energetic_eof}; last2s RMS is only {-end_drop:.2f}dB below peak window; recorded reverb is unknown, so wet is restrained and whole-cue processing is disabled")
    else:
        result["rack"] = {"preset": "dry", "wet": 0.0}
        result["basis"].append("no additional reverb: continuous/periodic source or measured ending already quiet; recorded-room content has not been auditioned")
    return result


def named_effect_events(entries: list[dict]) -> dict[str, list[str]]:
    """Register only literal user-named semantics, without inventing gameplay call sites."""
    names = ("cigarette_pickup", "hammer_pickup", "fire_igniting", "jammed_gun_elevation",
             "jammed_traverse", "trigger_pull_not_loaded", "impact_stopped", "light_impact",
             "light_round_impact", "error", "surprise_contact")
    result = {}
    for entry in entries:
        if entry["role"] != "effect":
            continue
        name = re.sub(r"[^a-z0-9]+", "_", Path(entry["source_path"]).stem.lower()).strip("_")
        for event in names:
            matched = re.search(r"(?:^|_)" + event + r"\d*$", name)
            if event == "impact_stopped":
                matched = matched or re.search(r"(?:^|_)stopped_impact\d*$", name)
            if matched:
                result.setdefault(event, []).append(entry["id"])
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=Path("assets/audio/library/manifest.json"))
    parser.add_argument("--output", type=Path, default=Path("catalog/soundscape_cue_policy_20261002.json"))
    parser.add_argument("--library-root", type=Path, help="Playback root for a staged manifest; defaults to manifest parent")
    parser.add_argument("--bindings-output", type=Path, default=Path("catalog/soundscape_runtime_bindings_20261002.json"))
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    root = Path(manifest["source_root"])
    entries, warnings = [], []
    for asset in manifest["assets"]:
        if not asset["default_enabled"] or not asset.get("playback_available"):
            continue
        # Imported playback is content-addressed and stable even if a user moves the source.
        playback = (args.library_root or args.manifest.parent) / asset["playback_path"]
        if sha256(playback) != asset["playback_sha256"]:
            raise ValueError(f"playback hash changed: {playback}")
        entries.append(suggest(asset, envelope(playback)))
    by_role = lambda role: [c["id"] for c in entries if c["role"] == role]
    output = {
        "version": 1, "kind": "provisional_authored_cue_policy", "manifest_sha256": sha256(args.manifest),
        "rights": manifest["provenance"]["rights"], "independent_license_verification": False,
        "listened": False, "originals_modified": False, "soundscape_runtime_bindings_modified": False,
        "gain_db_contract": "additional gain on top of manifest gain.suggested_db; total_source_gain_db is informational only",
        "fade_contract": "fade_in is cue onset; fade_out is interruption/end envelope, not permission to trim the original source",
        "loop_contract": "suggestions only; parent merges explicit overrides into soundscape.json; no blanket activation of scenes",
        "events": {"run_start": by_role("intro"),
                   "death": [c["id"] for c in entries if c["source_path"].startswith("death_sounds/")],
                   "combat_start": [c["id"] for c in entries if Path(c["source_path"]).stem.lower() == "combat_start"],
                   "sunrise": [c["id"] for c in entries if "sunrise" in c["solar_events"]],
                   "sunset": [c["id"] for c in entries if "sunset" in c["solar_events"]],
                   "dusk": [c["id"] for c in entries if "dusk" in c["solar_events"]]},
        "effect_registry": by_role("effect"),
        "additional_folder_bindings": [
            {"asset": c["id"], **a} for c in entries for a in c["folder_assignments"]
            if c["role"] == c["catalog_role"] and a["default_enabled"]
            and (a["role"] != c["role"] or a["contexts"] != c["contexts"])],
        "event_trigger_limits": "effect registry has no invented gameplay triggers; death presentation routing is implemented separately from this metadata import",
        "counts": {"cues": len(entries), "roles": dict(Counter(c["role"] for c in entries)),
                   "loops": sum(c["loop"] for c in entries), "one_shots": sum(not c["loop"] for c in entries)},
        "warnings": warnings, "cues": entries,
    }
    output["events"].update(named_effect_events(entries))
    # Registry is descriptive and can retain explicitly deferred names. Only events are
    # executable; no inactive identity may remain in those event lists.
    output["named_effect_registry"] = named_effect_events(entries + [a for a in manifest["assets"]
        if not a["default_enabled"] and a.get("runtime_assignment", {}).get("enabled") is False])
    output["deferred_effects"] = [{"asset": a["id"], "source_path": a["source_path"],
        "enabled": False, "reason": a["runtime_assignment"]["reason"]}
        for a in manifest["assets"] if a.get("runtime_assignment", {}).get("enabled") is False]
    output["events"]["intensity_transition_riser"] = [c["id"] for c in entries if c["id"] == TRANSITION_RISER_ID]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2, allow_nan=False) + "\n")
    bindings = {"version": 1, "provisional": True, "listened": False,
                "manifest_sha256": output["manifest_sha256"],
                "gain_db_contract": output["gain_db_contract"],
                "cues": [{"asset": c["id"], "enabled": True,
                          **{k: c[k] for k in ("role", "contexts", "role_aliases", "role_contexts", "loop", "fade_in_s", "fade_out_s", "gain_db", "crossfade_s", "crossfade_curve", "rack")},
                          **{k: c[k] for k in ("riser_peak_s", "riser_lead_s", "bpm", "musical_key", "musical_metadata")
                             if k in c and (k != "musical_key" or c[k] is not None)}}
                         for c in entries],
                "events": output["events"], "effect_registry": output["effect_registry"],
                "named_effect_registry": output["named_effect_registry"],
                "deferred_effects": output["deferred_effects"],
                "additional_folder_bindings": output["additional_folder_bindings"],
                "limits": ["proposal only; merge into runtime bindings explicitly",
                           "death event metadata is registered; dedicated death presentation routing is implemented separately",
                           "scene recordings stay disabled until location authorship"]}
    args.bindings_output.parent.mkdir(parents=True, exist_ok=True)
    args.bindings_output.write_text(json.dumps(bindings, indent=2, allow_nan=False) + "\n")
    print(json.dumps(output["counts"], indent=2))


if __name__ == "__main__":
    main()
