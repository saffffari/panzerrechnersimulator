#!/usr/bin/env python3
"""Verify selected live combat captures and build a flat local review page.

Usage:
  python tools/build_layered_ai_review.py captures/layered_ai_review_v1.html \
    captures/layered_winter_night_v1_20261002 \
    captures/layered_winter_contact_v1_20261002 \
    captures/layered_winter_blizzard_v1_20261002

A named combat-scene-suite case uses PREFIX.json exactly. Only SCENE=all appends
_<scene_key> and writes PREFIX_suite.json. Both case and suite receipts are accepted.
No game is run, media altered, server started, or failed observation hidden. Outputs
are created only after every selected receipt and referenced media file verifies.
"""
from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
from html import escape
import json
import math
from pathlib import Path
import subprocess
import sys
from urllib.parse import quote
import wave

ROOT = Path(__file__).resolve().parent.parent
CAPTURES = ROOT / "captures"
CASE_SCHEMA = "panzer.combat-scene-case.v1"
SUITE_SCHEMA = "panzer.combat-scene-suite.v1"


def flat_path(value: str | Path, *, existing: bool = True) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = (CAPTURES / path) if len(path.parts) == 1 else (ROOT / path)
    path = path.resolve(strict=existing)
    if path.parent != CAPTURES.resolve() or (existing and not path.is_file()):
        raise ValueError(f"Expected a regular file directly in captures: {path}")
    return path


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def load_json(path: Path) -> dict:
    with path.open(encoding="utf-8") as stream:
        data = json.load(stream)
    if not isinstance(data, dict):
        raise ValueError(f"Receipt must be an object: {path}")
    return data


def selected_receipts(arguments: list[str]) -> list[Path]:
    selected = []
    for argument in arguments:
        path = flat_path(argument, existing=False)
        if path.suffix != ".json":
            case = path.with_suffix(".json")
            suite = path.with_name(path.name + "_suite.json")
            if case.is_file():
                path = case
            elif suite.is_file():
                path = suite
            else:
                raise FileNotFoundError(f"No completed receipt for {argument}: expected {case.name} or {suite.name}")
        path = flat_path(path)
        data = load_json(path)
        if data.get("schema") == SUITE_SCHEMA:
            cases = data.get("cases")
            if not isinstance(cases, list) or not cases:
                raise ValueError(f"Empty or malformed suite: {path}")
            for case in cases:
                if not isinstance(case, dict) or not isinstance(case.get("receipt"), str):
                    raise ValueError(f"Suite lacks a case receipt: {path}")
                selected.append(flat_path(case["receipt"]))
        elif data.get("schema") == CASE_SCHEMA:
            selected.append(path)
        else:
            raise ValueError(f"Not a combat-scene-suite case/suite receipt: {path}")
    if len(set(selected)) != len(selected):
        raise ValueError("A capture was selected more than once; choose each case once")
    return selected


def run_json(command: list[str]) -> dict:
    result = subprocess.run(command, check=True, capture_output=True, text=True)
    return json.loads(result.stdout)


def number(data: dict, field: str, *, optional: bool = False) -> float | None:
    value = data.get(field)
    if value is None and optional:
        return None
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise ValueError(f"Invalid {field}: {value!r}")
    return value


def inspect_video(path: Path, record: dict) -> dict:
    probe = run_json(["ffprobe", "-v", "error", "-count_frames", "-show_streams", "-show_format", "-of", "json", str(path)])
    videos = [s for s in probe.get("streams", []) if s.get("codec_type") == "video"]
    audios = [s for s in probe.get("streams", []) if s.get("codec_type") == "audio"]
    if len(videos) != 1 or len(audios) != 1:
        raise ValueError(f"Expected one picture and one audio stream: {path}")
    video, audio = videos[0], audios[0]
    native = record.get("native_resolution")
    output = record.get("output_resolution")
    actual = [video.get("width"), video.get("height")]
    if actual != native or actual != output or not all(isinstance(v, int) and v > 0 for v in actual):
        raise ValueError(f"Native/output/video resolutions differ: {path}: {native}, {output}, {actual}")
    duration = number(record, "duration_s")
    fps = number(record, "fps")
    if not duration or not fps:
        raise ValueError(f"Empty capture: {path}")
    frames = int(video["nb_read_frames"])
    rate = float(Fraction(video["avg_frame_rate"]))
    if frames != round(duration * fps) or abs(rate - fps) > 0.001:
        raise ValueError(f"Frame count/rate does not match receipt: {path}")
    actual_duration = float(probe["format"]["duration"])
    if abs(actual_duration - duration) > max(0.15, 2.0 / fps):
        raise ValueError(f"Media duration does not match receipt: {path}")
    subprocess.run(["ffmpeg", "-v", "error", "-xerror", "-nostdin", "-threads", "2", "-i", str(path),
                    "-f", "null", "-"], check=True, capture_output=True)
    return {"file": path.name, "sha256": digest(path), "bytes": path.stat().st_size,
            "resolution": actual, "frames": frames, "fps": rate, "duration_s": actual_duration,
            "audio_codec": audio.get("codec_name"), "audio_sample_rate": int(audio["sample_rate"]),
            "audio_channels": audio.get("channels"), "full_decode_passed": True}


def inspect_audio(path: Path, duration: float) -> dict:
    with wave.open(str(path), "rb") as stream:
        params = stream.getparams()
        if params.comptype != "NONE" or params.nchannels != 2 or params.sampwidth != 2 or params.framerate != 44100:
            raise ValueError(f"Expected stereo 44.1kHz PCM16 soundtrack: {path}")
        remaining = params.nframes
        while remaining:
            frames = min(remaining, 65536)
            if len(stream.readframes(frames)) != frames * params.nchannels * params.sampwidth:
                raise ValueError(f"Truncated soundtrack: {path}")
            remaining -= frames
        seconds = params.nframes / params.framerate
        if not params.nframes or abs(seconds - duration) > 0.1:
            raise ValueError(f"Soundtrack duration does not match video: {path}")
    return {"file": path.name, "sha256": digest(path), "frames": params.nframes,
            "sample_rate": params.framerate, "channels": params.nchannels,
            "duration_s": seconds, "complete_pcm": True}


def inspect_image(path: Path) -> dict:
    probe = run_json(["ffprobe", "-v", "error", "-show_streams", "-of", "json", str(path)])
    stream = next((s for s in probe.get("streams", []) if s.get("codec_type") == "video"), None)
    if not stream or stream.get("width", 0) <= 0 or stream.get("height", 0) <= 0:
        raise ValueError(f"Invalid contact image: {path}")
    subprocess.run(["ffmpeg", "-v", "error", "-xerror", "-nostdin", "-threads", "2", "-i", str(path),
                    "-frames:v", "1", "-f", "null", "-"], check=True, capture_output=True)
    return {"file": path.name, "sha256": digest(path), "resolution": [stream["width"], stream["height"]],
            "full_decode_passed": True}


def observed_firing(counters: dict) -> str:
    own = counters["tiger_shots"] + counters["player_mg_rounds"]
    enemy = counters["enemy_shots"] + counters["course_defender_discharges"]
    if own and enemy:
        return "Fire exchanged"
    if enemy:
        return "Incoming fire observed"
    if own:
        return "Own fire only"
    return "No firing observed"


def inspect_case(receipt: Path) -> dict:
    record = load_json(receipt)
    if record.get("schema") != CASE_SCHEMA or record.get("media_recorded") is not True:
        raise ValueError(f"Not a completed media capture; preserve its diagnostic receipt and finish capture first: {receipt}")
    checks = record.get("checks")
    if not isinstance(checks, dict) or not checks or not all(isinstance(v, bool) for v in checks.values()):
        raise ValueError(f"Missing or malformed observation checks: {receipt}")
    all_checks = all(checks.values())
    if not isinstance(record.get("passed"), bool) or record["passed"] != all_checks:
        raise ValueError(f"Receipt pass flag disagrees with its checks: {receipt}")
    scene = record.get("scene")
    if not isinstance(scene, dict) or not isinstance(scene.get("key"), str):
        raise ValueError(f"Missing scene context: {receipt}")
    files = {key: flat_path(record[key]) for key in ("video", "soundtrack", "contact_sheet")}
    video = inspect_video(files["video"], record)
    audio = inspect_audio(files["soundtrack"], float(record["duration_s"]))
    contact = inspect_image(files["contact_sheet"])
    counters = {key: number(record, key) for key in ("tiger_shots", "enemy_shots", "incoming_hits", "outgoing_hits")}
    for key in ("course_defender_discharges", "player_mg_rounds", "resulting_enemy_wrecks"):
        counters[key] = number(record, key, optional=True)
        if counters[key] is None:
            # Older completed receipts may lack these extra counters. Never guess a zero.
            raise ValueError(f"Missing required layered-review counter {key}: {receipt}")
    kia = number(record, "listener_kia_at_s", optional=True)
    if "listener_kia_at_s" not in record or (kia is not None and kia > float(record["duration_s"]) + 0.1):
        raise ValueError(f"Missing or out-of-range listener outcome: {receipt}")
    lanes = set()
    maximum_layers = 0
    for sample in record.get("samples", []):
        voices = (sample.get("soundscape") or {}).get("voices") or []
        advancing = [v for v in voices if isinstance(v.get("frames"), (int, float)) and v["frames"] > 0]
        lanes.update(str(v["lane"]) for v in advancing if v.get("lane"))
        maximum_layers = max(maximum_layers, len(advancing))
    return {"receipt": receipt.name, "receipt_sha256": digest(receipt), "scene": scene,
            "passed": record["passed"], "checks": checks, "counters": counters,
            "firing": observed_firing(counters), "listener_kia_at_s": kia,
            "maximum_travel_m": number(record, "maximum_travel_m"),
            "video": video, "audio": audio, "contact": contact,
            "scope": record.get("scope", "Automated native capture; inspect the receipt for staging."),
            "layered_sources": {"lanes_observed": sorted(lanes), "maximum_submitted_at_once": maximum_layers,
                                "perceptual_audibility_verified": False},
            "forced_hits": record.get("forced_hits"), "forced_kills": record.get("forced_kills"),
            "forced_fire": record.get("forced_fire"), "invulnerability": record.get("invulnerability"),
            "offline_audio_replacement": (record.get("audio") or {}).get("offline_replacement"),
            "offline_gain_added": (record.get("audio") or {}).get("offline_gain_added")}


def url(name: str) -> str:
    return escape(quote(name), quote=True)


def card(row: dict, index: int) -> str:
    scene = row["scene"]
    titles = {"winter_canyon_night_drive": "Winter night drive",
              "winter_canyon_course": "Winter contact",
              "winter_canyon_blizzard_close": "Blizzard encounter"}
    title = titles.get(scene["key"], scene["key"].replace("_", " ").capitalize())
    checks = row["checks"]
    success = sum(checks.values())
    failures = [key.replace("_", " ") for key, passed in checks.items() if not passed]
    status = f"{success}/{len(checks)} checks passed"
    if failures:
        status = "Diagnostic capture · " + status
    else:
        status = "Observation checks passed · " + status
    c = row["counters"]
    observed = " · ".join(f"{label}: {c[key]:g}" for key, label in
        [("tiger_shots", "Tiger main-gun shots"), ("player_mg_rounds", "MG rounds"),
         ("enemy_shots", "Enemy tank shots"), ("course_defender_discharges", "Other defender shots"),
         ("incoming_hits", "Incoming hits"), ("outgoing_hits", "Outgoing hits"), ("resulting_enemy_wrecks", "Enemy wrecks")])
    kia = row["listener_kia_at_s"]
    outcome = f"Listener killed at {kia:.2f}s; the recording continues after death." if kia is not None else "No listener death recorded during this clip."
    outcome += f" Maximum tank travel: {row['maximum_travel_m']:.1f} m."
    dimensions = row["video"]["resolution"]
    context = " · ".join(str(v) for v in [scene.get("day_phase"), scene.get("camera") or "Occupied camera",
        f"{row['video']['duration_s']:.1f}s", f"{dimensions[0]} × {dimensions[1]} native"] if v)
    check_rows = "".join(f"<li class=\"{'pass' if passed else 'fail'}\">{'Pass' if passed else 'Fail'} — {escape(key.replace('_', ' '))}</li>" for key, passed in checks.items())
    fail_summary = f'<p class="fail">Unmet: {escape("; ".join(failures))}.</p>' if failures else ""
    layers = row["layered_sources"]
    lane_line = (f"Submitted library layers: {', '.join(layers['lanes_observed'])}; up to {layers['maximum_submitted_at_once']} voices together. "
                 "This is a playback-cursor observation; listen to judge balance and audibility.")
    return f'''<section aria-labelledby="scene-{index}">
<h2 id="scene-{index}">{escape(title)}</h2><p class="context">{escape(context)}</p>
<p>{escape(str(scene.get("purpose", "")))}</p>
<p class="status {'ok' if row['passed'] else 'warning'}">{escape(status)}</p>{fail_summary}
<video controls preload="metadata" playsinline width="{dimensions[0]}" height="{dimensions[1]}" aria-label="{escape(title, quote=True)}"><source src="{url(row['video']['file'])}" type="video/mp4">Your browser cannot play this video.</video>
<p class="observed"><strong>{escape(row['firing'])}.</strong> {escape(observed)}</p>
<p>{escape(outcome)}</p>
<details><summary>Audio only</summary><audio controls preload="metadata" aria-label="{escape(title + ' soundtrack', quote=True)}" src="{url(row['audio']['file'])}"></audio><p>{escape(lane_line)}</p></details>
<details><summary>Six frames and capture details</summary><img loading="lazy" alt="Six sampled frames from {escape(title, quote=True)}" src="{url(row['contact']['file'])}"><p>{escape(str(row['scope']))}</p><ul class="checks">{check_rows}</ul></details>
<p class="links"><a href="{url(row['video']['file'])}">Video file</a> · <a href="{url(row['audio']['file'])}">Original soundtrack</a> · <a href="{url(row['receipt'])}">Capture receipt</a></p>
</section>'''


def render(rows: list[dict], validation_name: str) -> str:
    cards = "\n".join(card(row, index) for index, row in enumerate(rows, 1))
    return '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzer · layered game scenes</title><style>
:root{color-scheme:dark;font:16px/1.5 system-ui,sans-serif;background:#101311;color:#eeeade}*{box-sizing:border-box}
body{max-width:1720px;margin:auto;padding:35px 24px 70px}h1{font-size:clamp(28px,4vw,42px);line-height:1.15;letter-spacing:-.03em;margin:10px 0 18px}h2{font-size:24px;margin:0 0 8px}p{color:#c4c7bf;max-width:980px}a{color:#d8d3a6}header{margin-bottom:30px}label{cursor:pointer;font-size:14px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,730px),1fr));gap:28px}section{min-width:0;border-top:1px solid #3a423a;padding-top:23px}.context,.links{font-size:14px;color:#aab4a8}.status{font-size:14px;padding:8px 12px;border-left:3px solid #869b74;background:#19211b}.warning{border-color:#dcab76}.fail{color:#efb29c}.pass{color:#b9c6af}video{display:block;width:100%;max-width:800px;height:auto;aspect-ratio:16/9;background:#000}body.pixel video{image-rendering:pixelated}audio{display:block;width:100%;margin-top:14px}img{display:block;width:100%;height:auto;margin-top:14px}.observed{font-size:14px}details{margin:14px 0;font-size:14px}summary{cursor:pointer}.checks{padding-left:20px;columns:2}footer{border-top:1px solid #3a423a;margin-top:35px;padding-top:16px;font-size:14px}@media(max-width:600px){body{padding:25px 16px}.checks{columns:1}}
</style><body class="pixel"><header><h1>Layered sound in the game</h1>
<p>Actual native game captures with live AI, ordinary vehicle controls and the game audio mixer. Initial placements, weather and automated crew are set by the scene harness. These are simulation-clock recordings, not a human playthrough or a real-time performance benchmark.</p>
<p>Each card reports its observed firing, damage and survival. Passing capture checks does not establish that the mix or visuals are accepted.</p>
<label><input id="pixels" type="checkbox" checked> Sharp pixel scaling</label></header>
<div class="grid">''' + cards + '''</div><footer><p>Video, soundtrack and contact sheets were checked for completeness before this page was written. Nothing plays automatically. <a href="''' + url(validation_name) + '''">Media verification receipt</a></p></footer>
<script>const players=[...document.querySelectorAll('video,audio')];players.forEach(active=>active.addEventListener('play',()=>players.forEach(other=>{if(other!==active)other.pause()})));document.getElementById('pixels').addEventListener('change',event=>document.body.classList.toggle('pixel',event.target.checked));</script></body></html>'''


def build(output: str | Path, prefixes: list[str]) -> dict:
    output = flat_path(output, existing=False)
    if output.suffix.lower() != ".html":
        raise ValueError("Output must be a fresh .html directly in captures")
    evidence = output.with_name(output.stem + "_verification.json")
    if output.exists() or evidence.exists():
        raise FileExistsError(f"Existing review preserved; choose a fresh output: {output}")
    receipts = selected_receipts(prefixes)
    rows = [inspect_case(receipt) for receipt in receipts]
    page = render(rows, evidence.name)
    payload = {"schema": "panzer.layered-ai-review.v1", "page": output.name,
               "case_count": len(rows), "all_media_verified": True,
               "all_observation_checks_passed": all(row["passed"] for row in rows),
               "autoplay": False, "normalization_added": False,
               "human_playthrough": False, "real_time_performance_test": False,
               "subjective_listening_or_visual_acceptance": False, "cases": rows}
    # Exclusive creation protects earlier results and races; inputs/media are never changed.
    with evidence.open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2)
        stream.write("\n")
    with output.open("x", encoding="utf-8") as stream:
        stream.write(page)
    return {"page": str(output), "verification": str(evidence), "cases": len(rows),
            "passed_cases": sum(row["passed"] for row in rows), "videos_decoded": len(rows)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("output", help="Fresh HTML filename or captures path")
    parser.add_argument("prefixes", nargs="+", help="Capture prefixes or case/suite JSON receipts")
    args = parser.parse_args()
    try:
        result = build(args.output, args.prefixes)
    except (OSError, ValueError, KeyError, TypeError, wave.Error, subprocess.SubprocessError) as exc:
        print(f"Review not written: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
