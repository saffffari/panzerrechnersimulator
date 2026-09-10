#!/usr/bin/env python3
"""Bounded, sequential first-person capture queue with recoverable receipts and a video gallery.

The game binary owns cameras, mechanics and sound. This tool schedules it, checks the encoded
streams, and makes contact sheets for human/agent review; a completed encoding is not visual QA.
"""
import argparse
import datetime as dt
import fcntl
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
CAPTURES = ROOT / "captures/astra"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    temp = path.with_suffix(".tmp")
    temp.write_text(json.dumps(value, indent=2) + "\n")
    temp.replace(path)


def probe(path):
    return json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)
    ]))


def check_streams(path, spec):
    info = probe(path)
    video = [s for s in info["streams"] if s["codec_type"] == "video"]
    audio = [s for s in info["streams"] if s["codec_type"] == "audio"]
    if len(video) != 1 or len(audio) != 1:
        raise RuntimeError("capture must contain exactly one video and one audio stream")
    v, a = video[0], audio[0]
    if (v["width"], v["height"], v["r_frame_rate"]) != (1920, 1080, "24/1"):
        raise RuntimeError("expected sharp 3x upscale of 640x360 at 24 fps")
    if a["channels"] != 2 or int(a["sample_rate"]) != 44100:
        raise RuntimeError("expected actual stereo gameplay mix at 44100 Hz")
    if abs(float(info["format"]["duration"]) - spec["seconds"]) > 0.06:
        raise RuntimeError("encoded duration differs from scenario")
    return info


def contact_sheet(video, output, frames):
    indices = sorted(set(round((frames - 1) * i / 5) for i in range(6)))
    select = "+".join(f"eq(n\\,{i})" for i in indices)
    subprocess.run([
        "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", str(video),
        "-vf", f"select={select},scale=640:360:flags=neighbor,tile=3x2",
        "-frames:v", "1", str(output)
    ], check=True)


def gallery(run_id, state):
    esc = html.escape
    cards = []
    for case in state["cases"]:
        media = ""
        if case.get("video"):
            media = (f'<video controls preload="metadata" poster="{esc(Path(case["poster"]).name)}" '
                     f'src="{esc(Path(case["video"]).name)}"></video>')
        mechanics = ", ".join(case["mechanics"])
        receipt = case.get("receipt")
        link = f'<a href="{esc(Path(receipt).name)}">Capture receipt</a>' if receipt else ""
        cards.append(f'<article><h2>{esc(case["title"])}</h2>{media}'
                     f'<p class="state">{esc(case["status"].replace("_", " "))}</p>'
                     f'<p>{esc(mechanics.replace("_", " "))}</p>'
                     f'<p>{esc(case.get("error", ""))}</p>{link}</article>')
    page = '''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Panzer — first-person mechanic review</title><style>
*{box-sizing:border-box}body{margin:0;padding:32px;background:#111718;color:#e4e6d9;font:16px/1.5 system-ui}
header{max-width:1000px;margin-bottom:32px}h1{font-size:30px;margin:0 0 12px}h2{font-size:19px}
main{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,520px),1fr));gap:24px}
article{background:#1b2425;padding:20px;border:1px solid #344244}video{width:100%;image-rendering:pixelated;background:#000}
p{margin:10px 0}.state{color:#e8c880}a{color:#a5d9e3}code{font-size:.85em}</style>
<header><h1>Panzer — first-person mechanic review</h1>
<p>Actual player views with synchronized gameplay sound. Rendered at 640 × 360 and enlarged to
1920 × 1080 with sharp pixels. Play a video to enable sound.</p>
<p>“Recorded pending visual review” confirms the streams and scenario checks passed. Visual quality
and whether the mechanic reads clearly are reviewed separately. Failed or unrecorded cases remain listed.</p>
<p>Run: <code>RUN_ID</code></p></header><main>CARDS</main>'''
    path = CAPTURES / f"mechanics_{run_id}_index.html"
    path.write_text(page.replace("RUN_ID", esc(run_id)).replace("CARDS", "\n".join(cards)))
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--case", action="append", default=[], help="scenario id; repeat to select")
    parser.add_argument("--deadline", help="UTC ISO timestamp; do not start a case after this time")
    parser.add_argument("--pause-ms", type=int, default=60)
    parser.add_argument("--idle-factor", type=float, default=3.0,
                        help="render-time rest multiplier; lower for authorized unattended batches")
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--preflight-only", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_]+", args.run_id):
        parser.error("run-id must contain letters, digits and underscores only")
    deadline = dt.datetime.fromisoformat(args.deadline.replace("Z", "+00:00")) if args.deadline else None
    if deadline and deadline.tzinfo is None:
        parser.error("deadline must include timezone")
    output = ROOT / "artifacts" / f"astra-first-person-{args.run_id}"
    CAPTURES.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    state_path = output / "queue.json"
    with (ROOT / "artifacts/astra-first-person-render.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        binary = output / "panzerrechnersystem"
        if state_path.exists():
            if not args.resume:
                parser.error("run already exists; choose a new run-id or use --resume")
            state = json.loads(state_path.read_text())
            # Preserve immutable queues recorded before the product rename.
            if not binary.exists() and (output / "kessel").exists():
                binary = output / "kessel"
            if digest(binary) != state["binary_sha256"]:
                raise RuntimeError("saved executable changed since this queue began")
        else:
            shutil.copy2(ROOT / "target/release/panzerrechnersystem", binary)
            cases = []
            for source in sorted((ROOT / "assets/review/first_person").glob("*.json")):
                spec = json.loads(source.read_text())
                if args.case and spec["id"] not in args.case:
                    continue
                saved = output / source.name
                shutil.copy2(source, saved)
                checked = subprocess.run([str(binary), "mechanic-validate", str(saved)], cwd=ROOT,
                                         capture_output=True, text=True)
                cases.append({"id": spec["id"], "title": spec["title"],
                              "mechanics": spec["mechanics"], "scenario": str(saved),
                              "source_sha256": digest(saved),
                              "status": "queued" if checked.returncode == 0 else "preflight_failed",
                              "error": checked.stderr.strip() if checked.returncode else ""})
            unknown = set(args.case) - {c["id"] for c in cases}
            if unknown or not cases:
                parser.error(f"no matching scenarios or unknown selections: {sorted(unknown)}")
            state = {"schema": "kessel.first-person-review-queue.v1", "run_id": args.run_id,
                     "binary_sha256": digest(binary), "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
                     "deadline": args.deadline, "cases": cases}
            write_json(state_path, state)
        print(gallery(args.run_id, state), flush=True)
        if args.preflight_only:
            return
        environment = dict(os.environ, KESSEL_INTERNAL_RES="640x360",
                           KESSEL_CAPTURE_FRAME_PAUSE_MS=str(args.pause_ms),
                           KESSEL_CAPTURE_IDLE_FACTOR=str(args.idle_factor))
        for case in state["cases"]:
            if case["status"] not in ("queued", "recording"):
                continue
            if deadline and dt.datetime.now(dt.timezone.utc) >= deadline:
                break
            spec = json.loads(Path(case["scenario"]).read_text())
            prefix = CAPTURES / f'mechanics_{args.run_id}_{case["id"]}'
            video = prefix.with_suffix(".mp4")
            if video.exists():
                case["status"] = "existing_output_needs_review"
                case["error"] = "Found an output from an interrupted run; preserve it and review before rerendering."
                write_json(state_path, state)
                continue
            case["status"] = "recording"
            case["started_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
            write_json(state_path, state)
            gallery(args.run_id, state)
            print(f'recording {case["id"]}', flush=True)
            began = time.monotonic()
            try:
                with (output / f'{case["id"]}.log').open("w") as log:
                    subprocess.run([str(binary), "mechanic-review", str(prefix), case["scenario"]],
                                   cwd=ROOT, env=environment, stdout=log, stderr=subprocess.STDOUT,
                                   timeout=2400, check=True)
                info = check_streams(video, spec)
                receipt = Path(str(prefix) + "_manifest.json")
                recorded = json.loads(receipt.read_text())
                sheet = Path(str(prefix) + "_review.png")
                contact_sheet(video, sheet, recorded["frames"])
                case.update(status="recorded_pending_visual_review", video=str(video),
                            poster=str(prefix.with_suffix(".png")), receipt=str(receipt),
                            contact_sheet=str(sheet), stream_info=info, video_sha256=digest(video))
            except (subprocess.SubprocessError, OSError, ValueError, RuntimeError) as exc:
                case["status"] = "recording_failed"
                case["error"] = str(exc)
                # A failed outcome check can still leave useful, complete diagnostic media.
                # Retain and label it; never turn a failed state check into a successful case.
                receipt = Path(str(prefix) + "_manifest.json")
                if video.exists() and receipt.exists():
                    try:
                        info = check_streams(video, spec)
                        recorded = json.loads(receipt.read_text())
                        sheet = Path(str(prefix) + "_review.png")
                        contact_sheet(video, sheet, recorded["frames"])
                        case.update(video=str(video), poster=str(prefix.with_suffix(".png")),
                                    receipt=str(receipt), contact_sheet=str(sheet),
                                    stream_info=info, video_sha256=digest(video))
                    except (subprocess.SubprocessError, OSError, ValueError, RuntimeError):
                        pass
            case["elapsed_seconds"] = time.monotonic() - began
            case["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
            write_json(state_path, state)
            gallery(args.run_id, state)
            print(f'{case["id"]}: {case["status"]}', flush=True)


if __name__ == "__main__":
    main()
