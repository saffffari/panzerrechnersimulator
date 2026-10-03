#!/usr/bin/env python3
"""Verify completed hand-item mechanic captures and create a flat local review page.

  python tools/build_hand_item_review.py captures/hand_items_review_v1_20261002.html \
    captures/cigarette_ritual_v1_20261002 captures/held_flashlight_v1_20261002

Mechanic receipts are PREFIX_manifest.json, not PREFIX.json. Missing/incomplete
media prevents publishing. Completed failed checks remain labelled diagnostics.
This tool never runs the game, changes media, serves files or polls for completion.
"""
from __future__ import annotations

import argparse
from html import escape
import json
import math
from pathlib import Path
import subprocess
import sys
import wave

from build_layered_ai_review import (
    digest, flat_path, inspect_audio, inspect_image, inspect_video, load_json, url,
)

SCHEMA = "kessel.first-person-mechanic-record.v1"
LABELS = {
    "hand_pack_count": "Cigarettes left in packet", "hand_drawn": "Cigarettes drawn",
    "hand_total_unconsumed": "Unconsumed cigarettes", "hand_lit": "Cigarette lit",
    "hand_lighter_flame": "Lighter flame", "hand_puffs": "Puffs taken",
    "hand_exhale": "Exhale strength", "hand_burn_remaining": "Tobacco remaining",
    "hand_equipped": "Cigarette equipped", "hand_flashlight_equipped": "Flashlight equipped",
    "main_gun_shots": "Main-gun shots", "mg_rounds": "MG rounds", "tank_alive": "Tank alive",
}
TITLES = {"cigarette_ritual_v1": "Cigarette and lighter", "held_flashlight_v1": "Held flashlight"}


def finite(value: object, name: str, *, positive: bool = False) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"Missing or nonfinite {name}: {value!r}")
    if positive and value <= 0:
        raise ValueError(f"Expected positive {name}: {value!r}")
    return float(value)


def receipt_path(argument: str) -> Path:
    path = flat_path(argument, existing=False)
    if path.suffix != ".json":
        path = path.with_name(path.name + "_manifest.json")
    return flat_path(path)


def inspect_case(path: Path) -> dict:
    record = load_json(path)
    if record.get("schema") != SCHEMA or record.get("media_recorded") is not True:
        raise ValueError(f"Not a completed first-person mechanic recording: {path}")
    if record.get("first_person") is not True:
        raise ValueError(f"This review requires the ordinary first-person camera: {path}")
    scenario = record.get("scenario")
    if not isinstance(scenario, dict) or not isinstance(scenario.get("id"), str):
        raise ValueError(f"Missing mechanic scenario: {path}")
    seconds = finite(scenario.get("seconds"), "scenario.seconds", positive=True)
    if record.get("native_resolution") != [800, 450] or record.get("encoded_resolution") != [800, 450] or record.get("fps") != 24:
        raise ValueError(f"Expected native 800x450 at 24 fps: {path}")
    frames = record.get("frames")
    if not isinstance(frames, int) or isinstance(frames, bool) or frames != round(seconds * 24):
        raise ValueError(f"Receipt frame count differs from capture duration: {path}")
    checks = record.get("outcome_checks")
    if not isinstance(checks, list) or not checks or not all(
            isinstance(check, dict) and isinstance(check.get("passed"), bool)
            and isinstance(check.get("requirement"), dict)
            and isinstance(check["requirement"].get("metric"), str) for check in checks):
        raise ValueError(f"Missing or malformed state checks: {path}")
    actions_passed = record.get("required_timeline_actions_passed")
    if not isinstance(actions_passed, bool):
        raise ValueError(f"Missing required-action result: {path}")
    passed = all(check["passed"] for check in checks) and actions_passed
    if record.get("outcome_checks_passed") is not passed:
        raise ValueError(f"Receipt pass flag disagrees with state/action checks: {path}")
    video = inspect_video(flat_path(record["video"]), {
        "native_resolution": [800, 450], "output_resolution": [800, 450],
        "duration_s": seconds, "fps": 24,
    })
    if video["frames"] != frames or video["audio_sample_rate"] != 44100 or video["audio_channels"] != 2:
        raise ValueError(f"MP4 picture/audio metadata differs from capture contract: {path}")
    audio = inspect_audio(flat_path(record["audio"]), seconds)
    if record.get("sample_frames") != audio["frames"]:
        raise ValueError(f"WAV frame count differs from mixer receipt: {path}")
    peak = finite(record.get("raw_mix_peak"), "raw_mix_peak")
    gain = finite(record.get("export_fixed_gain"), "export_fixed_gain")
    guard = finite(record.get("export_peak_guard_gain"), "export_peak_guard_gain")
    if peak < 0 or gain != 1.0 or not 0 < guard <= 1:
        raise ValueError(f"Unexpected mixer/export gain contract: {path}")
    poster = inspect_image(flat_path(record["poster"]))
    kia = record.get("kia_started_s")
    if kia is not None and not 0 <= finite(kia, "kia_started_s") <= seconds:
        raise ValueError(f"Out-of-range listener outcome: {path}")
    return {
        "receipt": path.name, "receipt_sha256": digest(path), "scenario": scenario,
        "passed": passed, "checks": checks, "required_timeline_actions_passed": actions_passed,
        "video": video, "audio": audio, "poster": poster,
        "initial_state": record.get("initial_state", {}), "final_state": record.get("final_state", {}),
        "first_person": True, "audio_source": record.get("audio_source"),
        "raw_mix_peak": peak, "export_fixed_gain": gain, "export_peak_guard_gain": guard,
        "kia_started_s": kia, "finite_pcm_audio": True,
        "subjective_visual_and_audio_acceptance": False,
    }


def show(value: object) -> str:
    if isinstance(value, bool):
        return "yes" if value else "no"
    if isinstance(value, float):
        return f"{value:.4g}"
    return str(value) if value is not None else "not recorded"


def expected(requirement: dict) -> str:
    if "equals" in requirement:
        return show(requirement["equals"])
    unit = "change " if "min_delta" in requirement or "max_delta" in requirement else ""
    low = requirement.get("min_delta" if unit else "min_value")
    high = requirement.get("max_delta" if unit else "max_value")
    if low is not None and high is not None:
        return f"{unit}{show(low)} to {show(high)}"
    return f"{unit}≥ {show(low)}" if low is not None else f"{unit}≤ {show(high)}"


def card(row: dict, index: int) -> str:
    scenario = row["scenario"]
    title = TITLES.get(scenario["id"], str(scenario.get("title", scenario["id"])))
    status = "State checks passed" if row["passed"] else "Diagnostic capture — checks failed"
    count = sum(check["passed"] for check in row["checks"])
    context = f"{scenario.get('station', 'Player')} station · {'Night' if scenario.get('night') else 'Day'} · {show(scenario['seconds'])} seconds"
    table = []
    for check in row["checks"]:
        req = check["requirement"]
        name = LABELS.get(req["metric"], req["metric"].replace("_", " ").capitalize())
        when = f"{show(req['at_s'])} s" if "at_s" in req else "End"
        observed = check.get("delta") if "min_delta" in req or "max_delta" in req else check.get("observed")
        values = [name, when, expected(req), show(observed), "Pass" if check["passed"] else "Fail"]
        table.append("<tr>" + "".join(f"<td>{escape(value)}</td>" for value in values) + "</tr>")
    action = "accepted" if row["required_timeline_actions_passed"] else "failed; see capture receipt"
    summary = f"{count}/{len(row['checks'])} state checks passed. Required timeline actions: {action}."
    detail = ("Finite packet, held lighter, ignition, three puffs, tobacco recession, exhale and stow."
              if scenario["id"].startswith("cigarette") else "Source flashlight body, moving beam and stow.")
    kia = f" Listener death recorded at {row['kia_started_s']:.2f} seconds." if row["kia_started_s"] is not None else ""
    guard = (f" Export peak guard applied {row['export_peak_guard_gain']:.4g} gain."
             if row["export_peak_guard_gain"] < 1 else " No export peak attenuation was needed.")
    return f'''<section aria-labelledby="case-{index}"><h2 id="case-{index}">{escape(title)}</h2>
<p class="context">{escape(context)}</p><p>{escape(detail)}</p>
<p class="status {'ok' if row['passed'] else 'fail'}"><strong>{escape(status)}</strong> · {escape(summary + kia)}</p>
<video controls playsinline preload="metadata" width="800" height="450" poster="{url(row['poster']['file'])}" aria-label="{escape(title, quote=True)}"><source src="{url(row['video']['file'])}" type="video/mp4">Your browser cannot play this video.</video>
<details><summary>Recorded state checks</summary><div class="table"><table><thead><tr><th>Check</th><th>When</th><th>Required</th><th>Observed</th><th>Result</th></tr></thead><tbody>{''.join(table)}</tbody></table></div></details>
<details><summary>Audio only</summary><audio controls preload="metadata" src="{url(row['audio']['file'])}" aria-label="{escape(title + ' soundtrack', quote=True)}"></audio><p>{escape(str(row['audio_source']) + guard)}</p></details>
<p class="links"><a href="{url(row['video']['file'])}">Video</a> · <a href="{url(row['audio']['file'])}">Original soundtrack</a> · <a href="{url(row['receipt'])}">Capture receipt</a></p></section>'''


def render(rows: list[dict], evidence_name: str) -> str:
    cards = "\n".join(card(row, index) for index, row in enumerate(rows, 1))
    return '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzer · hand items</title><style>
:root{color-scheme:dark;font:16px/1.5 system-ui,sans-serif;background:#111513;color:#eeeade}*{box-sizing:border-box}body{max-width:1676px;margin:auto;padding:32px 24px 65px}h1{font-size:clamp(29px,4vw,43px);line-height:1.12;letter-spacing:-.03em}h2{font-size:24px;margin:0 0 6px}p{color:#c6ccc5;max-width:1050px}a{color:#e2d6a1}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,680px),1fr));gap:28px}section{min-width:0;border-top:1px solid #485048;padding-top:22px}.context,.links{font-size:14px}.status{font-size:14px;border-left:3px solid #92a581;padding:10px;background:#1b241e}.status.fail{border-color:#e0a577;color:#f3c0a7}video{display:block;width:100%;max-width:800px;height:auto;aspect-ratio:16/9;background:#000}body.pixel video{image-rendering:pixelated}audio{display:block;width:100%;margin-top:14px}details{margin:16px 0;font-size:14px}summary,label{cursor:pointer}.controls{display:flex;flex-wrap:wrap;gap:8px 22px;list-style:none;padding:0;max-width:1350px;font-size:14px}.controls li{padding:5px 0}kbd{font:600 13px ui-monospace,monospace;border:1px solid #596159;border-radius:3px;padding:2px 5px;color:#f7f1dc;white-space:nowrap}.table{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:13px;margin-top:12px}th,td{text-align:left;border-bottom:1px solid #3b443c;padding:7px}footer{border-top:1px solid #485048;padding-top:18px;margin-top:32px;font-size:14px}@media(max-width:600px){body{padding:22px 14px}.controls{display:block}}
</style><body class="pixel"><header><h1>Hand items in the tank</h1>
<p>Native 800 × 450 game recordings at 24 fps, using scripted ordinary inputs and the game’s own audio mixer. The capture receipts disclose the setup and enemy-fire policy. State checks are reported below; they do not establish visual or listening acceptance.</p>
<ul class="controls"><li><kbd>C</kbd> draw cigarette / inspect packet</li><li>Hold <kbd>RMB</kbd> raise lit lighter; press <kbd>LMB</kbd> to ignite cigarette</li><li>Release <kbd>RMB</kbd> stow lighter</li><li><kbd>LMB</kbd> puff when lit</li><li><kbd>E</kbd> pick up a reachable packet</li><li><kbd>F</kbd> flashlight</li><li><kbd>Q</kbd> stow hand item</li><li><kbd>Shift+C</kbd> compass</li></ul>
<p class="context">These are the available controls. The two clips cover the declared cigarette and flashlight actions; they do not test world pickup or compass interaction.</p>
<label><input id="pixels" type="checkbox" checked> Sharp pixel scaling</label></header><div class="grid">''' + cards + '''</div>
<footer><p>The source packet and disposable-style lighter are modern stylized props, not verified period equipment. Smoke uses bounded fine particles carried in cabin or exterior air; ventilation, fluid flow and meaningful distant ember illumination are not simulated. Judge moving smoke, tip alignment, flame size, beam/shadows and sound in the clips.</p>
<p>The selected MP4s were fully decoded, and their frame counts, native dimensions and complete finite PCM soundtracks verified before this page was created. No audio normalization or media editing was added by this page generator. <a href="''' + url(evidence_name) + '''">Media verification</a></p></footer>
<script>const players=[...document.querySelectorAll('video,audio')];players.forEach(active=>active.addEventListener('play',()=>players.forEach(other=>{if(other!==active)other.pause()})));document.getElementById('pixels').addEventListener('change',event=>document.body.classList.toggle('pixel',event.target.checked));</script></body></html>'''


def build(output: str, prefixes: list[str]) -> dict:
    output_path = flat_path(output, existing=False)
    if output_path.suffix != ".html":
        raise ValueError("Output must be a fresh .html file directly in captures")
    evidence = output_path.with_name(output_path.stem + "_verification.json")
    if output_path.exists() or evidence.exists():
        raise FileExistsError(f"Existing review preserved; choose a new output: {output_path}")
    paths = [receipt_path(prefix) for prefix in prefixes]
    if not paths or len(set(paths)) != len(paths):
        raise ValueError("Select each completed mechanic capture once")
    rows = [inspect_case(path) for path in paths]
    payload = {"schema": "panzer.hand-item-review.v1", "page": output_path.name,
               "all_media_verified": True, "all_state_checks_passed": all(row["passed"] for row in rows),
               "autoplay": False, "game_run": False, "media_modified": False,
               "subjective_visual_or_listening_acceptance": False, "cases": rows}
    page = render(rows, evidence.name)
    with evidence.open("x", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2, allow_nan=False)
        stream.write("\n")
    with output_path.open("x", encoding="utf-8") as stream:
        stream.write(page)
    return {"page": str(output_path), "verification": str(evidence), "cases": len(rows),
            "passed_cases": sum(row["passed"] for row in rows), "videos_fully_decoded": len(rows)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("output", help="Fresh HTML path directly in captures")
    parser.add_argument("prefixes", nargs="+", help="Mechanic prefixes or completed *_manifest.json files")
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
