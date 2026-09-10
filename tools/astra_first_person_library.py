#!/usr/bin/env python3
"""Aggregate first-person recordings without rendering or inferring quality approval.

Only final muxed videos with actual capture receipts are playable. Reviews are an
append-only sidecar, bound to both video and receipt SHA-256, never a run label.
"""
import argparse
import datetime as dt
import hashlib
import html
import json
import os
from pathlib import Path
import subprocess
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_STEM = "first_person_library_20260905"
QA_NAME = "first_person_qa_20260905.json"
RECEIPT_SCHEMA = "kessel.first-person-mechanic-record.v1"
QA_SCHEMA = "kessel.first-person-media-qa.v1"
DECISIONS = {"pass", "fail", "pending"}


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    temp = path.with_name(path.name + f".{os.getpid()}.tmp")
    temp.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
    temp.replace(path)


def resolve_local(root, value, within=None):
    """Receipts may use repo-relative or absolute paths, but cannot link outside it."""
    if not isinstance(value, str) or not value:
        raise ValueError("missing local path")
    path = Path(value)
    path = (path if path.is_absolute() else root / path).resolve()
    if not path.is_relative_to((within or root).resolve()):
        raise ValueError(f"path is outside the allowed local directory: {value}")
    return path


def load_qa(path):
    if not path.exists():
        return {"schema": QA_SCHEMA, "reviews": []}
    data = json.loads(path.read_text())
    if data.get("schema") != QA_SCHEMA or not isinstance(data.get("reviews"), list):
        raise ValueError("unsupported QA sidecar schema")
    for item in data["reviews"]:
        for field in ("video_sha256", "capture_sha256"):
            if not isinstance(item.get(field), str) or len(item[field]) != 64:
                raise ValueError(f"QA review requires a SHA-256 in {field}")
            int(item[field], 16)
        for field in ("visual_coverage", "audio_quality", "art_finish"):
            if item.get(field) not in DECISIONS:
                raise ValueError(f"QA {field} must be pass, fail, or pending")
        if not isinstance(item.get("covered_mechanics"), list):
            raise ValueError("QA covered_mechanics must be an explicit list of ledger IDs")
        for field in ("scenario_id", "reviewer", "evidence", "note", "reviewed_utc"):
            if not isinstance(item.get(field), str) or not item[field].strip():
                raise ValueError(f"QA requires {field}")
    return data


def bound_review(qa, recording):
    # Sidecar order is its revision order. A filename is only a human-facing hint.
    matches = [q for q in qa["reviews"]
               if q["scenario_id"] == recording["scenario_id"]
               and q["video_sha256"] == recording.get("video_sha256")
               and q["capture_sha256"] == recording.get("capture_sha256")]
    return matches[-1] if matches else None


def inspect_streams(path, receipt):
    info = json.loads(subprocess.check_output(
        ["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
        stderr=subprocess.STDOUT, timeout=20))
    videos = [s for s in info.get("streams", []) if s.get("codec_type") == "video"]
    audios = [s for s in info.get("streams", []) if s.get("codec_type") == "audio"]
    errors = []
    if len(videos) != 1 or len(audios) != 1:
        errors.append("Expected one video stream and one gameplay audio stream")
    v, a = (videos or [{}])[0], (audios or [{}])[0]
    dimensions = [v.get("width"), v.get("height")]
    if dimensions != receipt.get("encoded_resolution"):
        errors.append("Encoded dimensions disagree with the capture receipt")
    fraction = v.get("avg_frame_rate", v.get("r_frame_rate", "0/1")).split("/")
    fps = float(fraction[0]) / max(float(fraction[-1]), 1)
    if abs(fps - float(receipt.get("fps", 0))) > 0.01:
        errors.append("Encoded frame rate disagrees with the capture receipt")
    if a.get("channels") != 2 or int(a.get("sample_rate", 0)) != 44100:
        errors.append("Expected stereo 44.1 kHz gameplay mix")
    duration = float(info.get("format", {}).get("duration", 0))
    expected = float(receipt.get("scenario", {}).get("seconds", 0))
    if abs(duration - expected) > 0.06:
        errors.append("Encoded duration disagrees with the frozen scenario")
    return {"passed": not errors, "errors": errors, "dimensions": dimensions,
            "fps": fps, "duration_s": duration, "audio_channels": a.get("channels"),
            "audio_sample_rate": a.get("sample_rate"), "video_codec": v.get("codec_name"),
            "audio_codec": a.get("codec_name"),
            "meaning": "Container/stream agreement only; not listening or visual approval"}


def completed_outcome(receipt):
    checks = receipt.get("outcome_checks")
    # A new receipt with no checks is not assumed to validate a mechanic.
    return (receipt.get("outcome_checks_passed") is True and isinstance(checks, list)
            and bool(checks) and all(c.get("passed") is True for c in checks))


def outcome_status(receipt):
    if completed_outcome(receipt):
        return "pass"
    if receipt.get("outcome_checks_passed") is False or any(
            c.get("passed") is False for c in receipt.get("outcome_checks", [])):
        return "fail"
    return "pending"


def fully_reviewed(record):
    qa = record.get("qa") or {}
    return (record.get("streams", {}).get("passed") is True
            and record.get("outcomes_passed") is True
            and record.get("first_person") is True
            and qa.get("visual_coverage") == "pass" and qa.get("audio_quality") == "pass")


def preference(record):
    qa = record.get("qa") or {}
    technical = record.get("streams", {}).get("passed") is True
    outcome = record.get("outcomes_passed") is True
    failure = "fail" in (qa.get("visual_coverage"), qa.get("audio_quality"))
    if fully_reviewed(record):
        tier = 4
    elif technical and outcome and not failure and qa.get("visual_coverage") == "pass":
        tier = 3
    elif technical and outcome and not failure:
        tier = 2
    elif technical:
        tier = 1
    else:
        tier = 0
    return tier, record.get("recorded_utc", ""), record.get("video", "")


def group_for(scenario_id):
    if any(s in scenario_id for s in ("playtest", "sourced_", "cellar")):
        return "World and exploration"
    if any(s in scenario_id for s in ("mixed_combat", "soft_target", "incoming_")):
        return "Combat and consequences"
    if any(s in scenario_id for s in ("foot_", "panzerfaust", "dismount")):
        return "On foot"
    return "Inside the Tiger"


def aggregate(root, qa_path):
    captures = root / "captures/astra"
    qa = load_qa(qa_path)
    warnings, queues, queue_cases = [], [], []
    scenarios = {}
    for path in sorted((root / "assets/review/first_person").glob("*.json")):
        try:
            spec = json.loads(path.read_text())
            scenarios[spec["id"]] = {"id": spec["id"], "title": spec["title"],
                "mechanics": spec.get("mechanics", []), "definition": str(path),
                "group": group_for(spec["id"]), "attempts": [], "recordings": []}
        except (OSError, ValueError, KeyError) as exc:
            warnings.append(f"Scenario {path.name}: {exc}")
    for path in sorted((root / "artifacts").glob("astra-first-person-*/queue.json")):
        try:
            state = json.loads(path.read_text())
            if state.get("schema") != "kessel.first-person-review-queue.v1":
                continue
            queues.append({"path": str(path), "run_id": state["run_id"],
                           "created_utc": state.get("created_utc"),
                           "binary_sha256": state.get("binary_sha256")})
            for case in state["cases"]:
                item = {k: v for k, v in case.items() if k != "stream_info"}
                item.update(run_id=state["run_id"], queue=str(path),
                            binary_sha256=state.get("binary_sha256"))
                queue_cases.append(item)
                sid = case["id"]
                scenario = scenarios.setdefault(sid, {"id": sid, "title": case["title"],
                    "mechanics": case.get("mechanics", []), "group": group_for(sid),
                    "attempts": [], "recordings": []})
                scenario["attempts"].append(item)
        except (OSError, ValueError, KeyError) as exc:
            warnings.append(f"Queue {path}: {exc}")
    records, seen = [], set()
    for path in sorted(captures.glob("*_manifest.json")):
        try:
            receipt_bytes = path.read_bytes()
            data = json.loads(receipt_bytes)
            if data.get("schema") != RECEIPT_SCHEMA:
                continue
            if data.get("first_person") is not True:
                raise ValueError("receipt does not attest an actual first-person camera")
            sid = data["scenario"]["id"]
            video = resolve_local(root, data.get("video"), captures)
            if video.suffix.lower() != ".mp4" or "_video_only" in video.stem:
                raise ValueError("not a final muxed MP4")
            if not video.is_file():
                raise ValueError("final video is missing")
            if str(video) in seen:
                raise ValueError("duplicate receipts reference the same final video")
            matching = [c for c in queue_cases if c["id"] == sid and (
                c.get("receipt") == str(path) or c.get("video") == str(video)
                or video.name == f'mechanics_{c["run_id"]}_{sid}.mp4')]
            case = matching[-1] if matching else {}
            if case.get("status") == "recording":
                warnings.append(f"Skipping active recording: {video.name}")
                continue
            stat = video.stat()
            sha = digest(video)
            streams = inspect_streams(video, data)
            after = video.stat()
            if (stat.st_size, stat.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
                raise ValueError("video changed during inspection; deferred until stable")
            record = {"scenario_id": sid, "title": data["scenario"]["title"],
                "video": str(video), "video_sha256": sha, "bytes": stat.st_size,
                "receipt": str(path), "capture_sha256": hashlib.sha256(receipt_bytes).hexdigest(),
                "recorded_utc": case.get("finished_utc") or dt.datetime.fromtimestamp(
                    stat.st_mtime, dt.timezone.utc).isoformat(),
                "run_id": case.get("run_id", "standalone capture"),
                "binary_sha256": case.get("binary_sha256"),
                "queue_status": case.get("status"), "first_person": data.get("first_person"),
                "native_resolution": data.get("native_resolution"),
                "streams": streams, "outcomes_passed": completed_outcome(data),
                "outcome_status": outcome_status(data),
                "outcome_checks": data.get("outcome_checks", []),
                "camera": data.get("camera"), "audio_source": data.get("audio_source"),
                "limitations": data.get("limitations", []),
                "setup": data["scenario"].get("setup", {}),
                "setup_results": data.get("setup_results"),
                "world_manifest": data["scenario"].get("world_manifest")}
            for key, value in (("poster", data.get("poster")),
                               ("contact_sheet", case.get("contact_sheet"))):
                if value:
                    try:
                        media_path = resolve_local(root, value, captures)
                        if media_path.is_file():
                            record[key] = str(media_path)
                    except ValueError as exc:
                        warnings.append(str(exc))
            record["qa"] = bound_review(qa, record)
            records.append(record)
            seen.add(str(video))
            scenario = scenarios.setdefault(sid, {"id": sid, "title": record["title"],
                "mechanics": data["scenario"].get("mechanics", []), "group": group_for(sid),
                "attempts": [], "recordings": []})
            scenario["recordings"].append(record)
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
            warnings.append(f"Receipt {path.name}: {exc}")
    for scenario in scenarios.values():
        scenario["recordings"].sort(key=preference, reverse=True)
        scenario["preferred_video"] = (scenario["recordings"][0]["video"]
                                       if scenario["recordings"] else None)
    ledger_path = root / "artifacts/astra-audit-20260905/first-person-mechanic-coverage.json"
    ledger = json.loads(ledger_path.read_text())
    coverage = []
    for row in ledger["mechanics"]:
        ids = list(dict.fromkeys(([row["scenario"]] if row.get("scenario") else [])
                               + row.get("scenarios", [])))
        candidates = [r for r in records if r["scenario_id"] in ids]
        reviewed = [r for r in candidates if fully_reviewed(r)
                    and row["id"] in r["qa"]["covered_mechanics"]]
        visual = [r for r in candidates if (r.get("qa") or {}).get("visual_coverage") == "pass"
                  and r["outcomes_passed"] and r["streams"]["passed"]
                  and row["id"] in r["qa"]["covered_mechanics"]]
        status = ("reviewed" if reviewed else "visual_only" if visual else
                  "recorded_unapproved" if candidates else "scripted" if ids else "not_mapped")
        coverage.append({"id": row["id"], "mechanic": row["mechanic"], "status": status,
                         "scenarios": ids, "recordings": len(candidates),
                         "gap": row.get("gap"), "ledger_observed": row.get("observed"),
                         "approved_videos": [r["video"] for r in reviewed]})
    bound = {(r["scenario_id"], r["video_sha256"], r["capture_sha256"]) for r in records}
    unbound = [q for q in qa["reviews"] if
               (q["scenario_id"], q["video_sha256"], q["capture_sha256"]) not in bound]
    if unbound:
        warnings.append(f"{len(unbound)} QA entries are unbound to current media/receipts; no approval applied")
    return {"schema": "kessel.first-person-media-library.v1", "generated_utc": now(),
            "root": str(root), "qa_path": str(qa_path), "ledger_path": str(ledger_path),
            "policy": "Stream validity, scenario outcomes, visual coverage, audio quality and art finish are independent. Reviews bind to actual video and receipt SHA-256. Frozen captures do not prove current build behavior.",
            "summary": {"ledger_mechanics": len(coverage),
                "reviewed_mechanics": sum(c["status"] == "reviewed" for c in coverage),
                "visual_only_mechanics": sum(c["status"] == "visual_only" for c in coverage),
                "recordings": len(records), "recorded_scenarios": sum(bool(s["recordings"]) for s in scenarios.values()),
                "scenarios": len(scenarios),
                "outcome_failed_recordings": sum(r["outcome_status"] == "fail" for r in records),
                "outcome_unchecked_recordings": sum(r["outcome_status"] == "pending" for r in records),
                "stream_failed_recordings": sum(not r["streams"]["passed"] for r in records),
                "fully_reviewed_recordings": sum(fully_reviewed(r) for r in records)},
            "queues": queues, "coverage": coverage,
            "scenarios": sorted(scenarios.values(), key=lambda s: (s["group"], s["title"])),
            "unbound_qa": unbound, "warnings": warnings}


def href(value, output, root):
    path = resolve_local(root, value)
    return quote(os.path.relpath(path, output.parent), safe="/.")


def render_page(index, output, json_path):
    esc = lambda s: html.escape(str(s), quote=True)
    root = Path(index["root"])
    link = lambda p: esc(href(p, output, root))
    def badge(label, status):
        return f'<span class="badge {esc(status)}">{esc(label)}: {esc(status)}</span>'

    def recording(r):
        q = r.get("qa") or {}
        poster = f' poster="{link(r["poster"])}"' if r.get("poster") else ""
        # preload=none prevents loading many long videos on this low-load library page.
        media = f'<video controls playsinline preload="none"{poster} src="{link(r["video"])}"></video>'
        states = badge("Scenario", r["outcome_status"])
        states += badge("Streams", "pass" if r["streams"]["passed"] else "fail")
        states += badge("Visual", q.get("visual_coverage", "pending"))
        states += badge("Audio review", q.get("audio_quality", "pending"))
        states += badge("Art finish", q.get("art_finish", "pending"))
        note = f'<p class="review">{esc(q["note"])}</p>' if q else '<p class="review">Picture and sound have not been reviewed for this exact recording.</p>'
        if r["outcome_status"] == "pending":
            note += '<p class="subtle">This diagnostic receipt has no complete scenario outcome checks.</p>'
        if q:
            note += f'<p class="subtle">Review: {esc(q["reviewer"])} · {esc(q["evidence"])} · {esc(q["reviewed_utc"])}</p>'
        dims = r["streams"]["dimensions"]
        facts = f'{r["streams"]["duration_s"]:.1f}s · {dims[0]} × {dims[1]} · {r["streams"]["fps"]:g} fps · stereo gameplay stream'
        if not r["streams"]["passed"]:
            facts += " · STREAM CHECK FAILED"
        links = f'<a href="{link(r["receipt"])}">Capture receipt</a> · <a href="{link(r["video"])}">Open MP4</a>'
        if r.get("contact_sheet"):
            links += f' · <a href="{link(r["contact_sheet"])}">Contact sheet</a>'
        failures = [c for c in r["outcome_checks"] if c.get("passed") is not True]
        failure_text = (f'<p class="failure">Failed scenario checks</p><pre>{esc(json.dumps(failures, indent=2))}</pre>' if failures else "")
        provenance = {k: r[k] for k in ("video_sha256", "capture_sha256", "binary_sha256", "native_resolution",
                       "camera", "audio_source", "setup", "setup_results", "world_manifest", "limitations")}
        return (f'<div class="recording">{media}<p class="badges">{states}</p>{note}'
                f'<p class="subtle">{esc(r["run_id"])} · {esc(r["recorded_utc"])}<br>{esc(facts)}</p>'
                f'<p>{links}</p>{failure_text}<details><summary>Frozen setup and evidence</summary>'
                f'<pre>{esc(json.dumps(provenance, indent=2, ensure_ascii=False))}</pre></details></div>')

    cards = []
    for s in index["scenarios"]:
        history = ""
        if s["recordings"]:
            primary = recording(s["recordings"][0])
            if len(s["recordings"]) > 1:
                history = (f'<details class="history"><summary>Earlier / alternate evidence ({len(s["recordings"])-1})</summary>'
                           + "".join(recording(r) for r in s["recordings"][1:]) + "</details>")
        else:
            primary = '<p class="empty">No completed recording with a capture receipt.</p>'
        attempts = ""
        if s["attempts"]:
            attempts = '<details><summary>Run attempts and failures</summary><ul>'
            for a in s["attempts"]:
                attempts += f'<li>{esc(a["run_id"])}: <strong>{esc(a["status"])}</strong>'
                if a.get("error"):
                    attempts += f'<pre>{esc(a["error"])}</pre>'
                attempts += f' · <a href="{link(a["queue"])}">Queue receipt</a></li>'
            attempts += "</ul></details>"
        search = (s["title"] + " " + s["id"] + " " + " ".join(s["mechanics"])).lower()
        cards.append(f'<article id="{esc(s["id"])}" data-group="{esc(s["group"])}" data-search="{esc(search)}" '
                     f'data-recorded="{int(bool(s["recordings"]))}"><p class="eyebrow">{esc(s["group"])}</p>'
                     f'<h2>{esc(s["title"])}</h2><p class="subtle"><code>{esc(s["id"])}</code></p>'
                     f'{primary}{history}{attempts}</article>')
    status_labels = {"reviewed": "Visual + audio approved", "visual_only": "Visual checked; audio pending",
                     "recorded_unapproved": "Recorded; not approved", "scripted": "Scripted; no recording",
                     "not_mapped": "No mapped capture scenario"}
    rows = []
    for c in index["coverage"]:
        refs = " · ".join(f'<a href="#{esc(s)}" class="scenario-link">{esc(s)}</a>' for s in c["scenarios"])
        rows.append(f'<tr><td>{esc(c["mechanic"])}</td><td>{esc(status_labels[c["status"]])}</td>'
                    f'<td>{refs or "—"}</td><td>{esc(c["gap"] or "")}</td></tr>')
    summary = index["summary"]
    stats = [(summary["recordings"], "complete recordings"),
             (f'{summary["reviewed_mechanics"]} / {summary["ledger_mechanics"]}', "mechanics with visual + audio approval"),
             (summary["visual_only_mechanics"], "visual checks; sound pending"),
             (summary["outcome_failed_recordings"], "recordings with failed scenario checks")]
    stat_html = "".join(f'<div><strong>{esc(n)}</strong><span>{esc(label)}</span></div>' for n, label in stats)
    warning_html = ("<details><summary>Import notices</summary><ul>" +
                    "".join(f'<li>{esc(w)}</li>' for w in index["warnings"]) + "</ul></details>") if index["warnings"] else ""
    page = '''<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; media-src 'self' file:; img-src 'self' file:; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<title>Panzerrechnersystem · First-person video library</title><style>
:root{color-scheme:dark}*{box-sizing:border-box}body{margin:0;background:#101716;color:#e6e6dd;font:15px/1.5 system-ui,sans-serif}
header,.toolbar,main,.coverage,footer{max-width:1500px;margin:auto;padding:26px 32px}header{padding-top:44px;padding-bottom:12px}
h1{font-size:clamp(28px,4vw,42px);line-height:1.15;margin:5px 0 18px;font-weight:650}h2{font-size:21px;line-height:1.25;margin:7px 0 12px}
p{margin:10px 0}header>p{max-width:960px}.eyebrow{color:#afbda2;font-size:12px;text-transform:uppercase;letter-spacing:.14em}
.subtle{color:#a3b2ae;font-size:13px}.stats{display:grid;grid-template-columns:repeat(4,1fr);gap:15px;margin-top:26px}
.stats>div{border-top:1px solid #52635b;padding-top:12px}.stats strong{display:block;font-size:30px;font-weight:550}.stats span{font-size:13px;color:#a3b2ae}
.toolbar{display:flex;flex-wrap:wrap;gap:14px;align-items:center;padding-top:16px;padding-bottom:10px}.toolbar input,.toolbar select{background:#202b27;color:inherit;border:1px solid #52635b;padding:10px;border-radius:3px;font:inherit}
.toolbar input[type=search]{flex:1;min-width:220px}.toolbar label{white-space:nowrap}.toolbar input[type=checkbox]{accent-color:#b2c99f}
main{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,535px),1fr));gap:24px;align-items:start}
article{background:#1b2522;border:1px solid #34463e;padding:21px;border-radius:4px;scroll-margin-top:12px}article[hidden]{display:none}
video{width:100%;aspect-ratio:16/9;background:#000;image-rendering:pixelated;display:block;border-radius:2px}
a{color:#b9d6cc;text-underline-offset:3px}a:hover{color:white}.badge{font-size:11px;padding:3px 7px;border:1px solid #59605d;border-radius:3px;display:inline-block}
.badges{display:flex;flex-wrap:wrap;gap:5px}.pass{color:#c4dfb6;background:#2b4232;border-color:#496346}.fail{color:#f2b29c;background:#432c28;border-color:#795246}.pending{color:#d5c394;background:#373429;border-color:#625b43}
.review{font-size:15px}.failure{color:#f2b29c}.empty{padding:24px 0;color:#a3b2ae}details{margin-top:16px}summary{cursor:pointer;color:#c6d1cb}summary:hover{color:#fff}
.history>.recording{padding-top:24px;border-top:1px solid #46564f;margin-top:20px}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:12px/1.5 ui-monospace,monospace;color:#bbc9c2;max-height:330px;overflow:auto}
.coverage{border-top:1px solid #46564f}.table-wrap{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:13px}th,td{text-align:left;vertical-align:top;padding:12px 14px;border-bottom:1px solid #34463e}td:first-child{min-width:230px}td:last-child{min-width:300px}th{color:#a3b2ae;font-weight:500}code{overflow-wrap:anywhere}
footer{font-size:13px;color:#a3b2ae;padding-bottom:50px}@media(max-width:640px){header,.toolbar,main,.coverage,footer{padding-left:16px;padding-right:16px}.stats{grid-template-columns:repeat(2,1fr)}article{padding:15px}}
</style></head><body><header><p class="eyebrow">Panzerrechnersystem · Development evidence</p>
<h1>First-person video library</h1><p>Actual player cameras and the gameplay sound mix. Press play for sound; nothing starts automatically. These are frozen recordings of the builds named in their receipts.</p>
<p class="subtle">Scenario checks confirm recorded state changes. Stream checks confirm the file format. Visual coverage, listening review and art finish are separate decisions. Preferred clips favor reviewed passing evidence; earlier and failed attempts remain available.</p>
<div class="stats">STATS</div></header>
<div class="toolbar"><input id="search" type="search" placeholder="Find a mechanic, station or scenario" aria-label="Search scenarios">
<select id="group" aria-label="Scenario group"><option value="">All areas</option><option>Inside the Tiger</option><option>On foot</option><option>Combat and consequences</option><option>World and exploration</option></select>
<label><input id="unrecorded" type="checkbox"> Show unrecorded scenarios</label></div><main>CARDS</main>
<section class="coverage"><details><summary>Full 54-mechanic coverage ledger</summary><p class="subtle">A mechanic is approved only when a matching-SHA review explicitly covers its ledger ID, visual and audio reviews pass, and its associated recording passes scenario and stream checks. A shared scenario does not approve every related mechanic.</p>
<div class="table-wrap"><table><thead><tr><th>Mechanic</th><th>Evidence</th><th>Mapped scenario</th><th>Source ledger gap</th></tr></thead><tbody>ROWS</tbody></table></div></details></section>
<footer><p>Snapshot: GENERATED · <a href="INDEX">Library data</a> · <a href="QA">SHA-bound reviews</a> · <a href="LEDGER">Source coverage ledger</a></p>
<p>Refresh the library with <code>python tools/astra_first_person_library.py build</code>. Refreshing never changes a quality decision. No new rendering or audio generation occurs.</p>WARNINGS</footer>
<script>
const search=document.getElementById('search'),group=document.getElementById('group'),unrecorded=document.getElementById('unrecorded');
function filter(){const query=search.value.toLowerCase();document.querySelectorAll('article').forEach(card=>{card.hidden=(!unrecorded.checked&&card.dataset.recorded==='0')||(group.value&&card.dataset.group!==group.value)||!card.dataset.search.includes(query);});}
[search,group,unrecorded].forEach(el=>el.addEventListener('input',filter));filter();
document.querySelectorAll('video').forEach(video=>video.addEventListener('play',()=>{document.querySelectorAll('video').forEach(other=>{if(other!==video)other.pause();});}));
document.querySelectorAll('.scenario-link').forEach(a=>a.addEventListener('click',()=>{search.value='';group.value='';unrecorded.checked=true;filter();}));
</script></body></html>'''
    replacements = {"STATS": stat_html, "CARDS": "\n".join(cards), "ROWS": "\n".join(rows),
                    "GENERATED": esc(index["generated_utc"]), "INDEX": link(str(json_path)),
                    "QA": link(index["qa_path"]), "LEDGER": link(index["ledger_path"]),
                    "WARNINGS": warning_html}
    # Token replacement in a single pass prevents user-controlled titles from acting as tokens.
    import re
    return re.sub(r"\b(?:STATS|CARDS|ROWS|GENERATED|INDEX|QA|LEDGER|WARNINGS)\b",
                  lambda m: replacements[m.group()], page)


def add_review(root, qa_path, manifest, visual, audio, art, reviewer, evidence, note, mechanics):
    path = resolve_local(root, str(manifest), root / "captures/astra")
    receipt = json.loads(path.read_text())
    if receipt.get("schema") != RECEIPT_SCHEMA:
        raise ValueError("review must reference an actual first-person capture receipt")
    video = resolve_local(root, receipt.get("video"), root / "captures/astra")
    if video.suffix != ".mp4" or "_video_only" in video.stem:
        raise ValueError("review must reference the final muxed video")
    ledger = json.loads((root / "artifacts/astra-audit-20260905/first-person-mechanic-coverage.json").read_text())
    unknown = set(mechanics) - {m["id"] for m in ledger["mechanics"]}
    if unknown:
        raise ValueError(f"unknown covered mechanic IDs: {sorted(unknown)}")
    data = load_qa(qa_path)
    item = {"scenario_id": receipt["scenario"]["id"], "video": str(video),
            "video_sha256": digest(video), "capture": str(path), "capture_sha256": digest(path),
            "reviewed_utc": now(), "reviewer": reviewer, "evidence": evidence, "note": note,
            "visual_coverage": visual, "audio_quality": audio, "art_finish": art,
            "covered_mechanics": sorted(set(mechanics))}
    if not all(item[k] in DECISIONS for k in ("visual_coverage", "audio_quality", "art_finish")):
        raise ValueError("invalid quality decision")
    data["reviews"].append(item)
    write_json(qa_path, data)
    return item


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--qa", type=Path, help="separate QA JSON; default captures/astra/" + QA_NAME)
    sub = parser.add_subparsers(dest="command", required=True)
    build = sub.add_parser("build", help="read existing receipts and streams; never render")
    build.add_argument("--stem", default=DEFAULT_STEM, help="flat output basename")
    review = sub.add_parser("review", help="append an explicit human/agent review of exact media bytes")
    review.add_argument("--manifest", required=True, type=Path)
    review.add_argument("--visual", choices=sorted(DECISIONS), required=True)
    review.add_argument("--audio", choices=sorted(DECISIONS), required=True)
    review.add_argument("--art", choices=sorted(DECISIONS), default="pending")
    review.add_argument("--reviewer", required=True)
    review.add_argument("--evidence", required=True, help="e.g. watched-and-listened or reported-contact-sheet-review")
    review.add_argument("--note", required=True)
    review.add_argument("--mechanic", action="append", default=[], help="explicit covered ledger ID; repeat")
    args = parser.parse_args()
    root = args.root.resolve()
    captures = root / "captures/astra"
    qa_path = resolve_local(root, str(args.qa or captures / QA_NAME), captures)
    if args.command == "review":
        item = add_review(root, qa_path, args.manifest, args.visual, args.audio, args.art,
                          args.reviewer, args.evidence, args.note, args.mechanic)
        print(json.dumps(item, indent=2))
        return
    import re
    if not re.fullmatch(r"[A-Za-z0-9_-]+", args.stem):
        parser.error("stem must be a flat basename using letters, digits, underscores or hyphens")
    captures.mkdir(parents=True, exist_ok=True)
    if not qa_path.exists():
        write_json(qa_path, load_qa(qa_path))
    index = aggregate(root, qa_path)
    json_path, html_path = captures / f"{args.stem}.json", captures / f"{args.stem}.html"
    write_json(json_path, index)
    temp = html_path.with_suffix(".html.tmp")
    temp.write_text(render_page(index, html_path, json_path))
    temp.replace(html_path)
    print(json.dumps({"html": str(html_path), "index": str(json_path), "qa": str(qa_path),
                      "summary": index["summary"], "warnings": index["warnings"]}, indent=2))


if __name__ == "__main__":
    main()
