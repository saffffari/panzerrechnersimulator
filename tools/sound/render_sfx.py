#!/usr/bin/env python3
"""Render rows of the sound prompt list (docs/SOUND_PROMPTS_20261109.md) with the ElevenLabs
sound-effect generator.

    python3 tools/sound/render_sfx.py OUT_DIR [--ids HT-06,IN-12] [--groups HT,IN,DS] [--takes N]
                                      [--influence 0.4] [--budget CREDITS] [--dry-run]

Each table row `| ID | Sound | Prompt | Sec | Takes | Notes |` becomes one request per take:
the prompt, plus the dry ending unless the row's notes say "wet"; the row's length in seconds;
looping when the notes say "loop". --takes caps the takes per row (the sheet's own number is the
ceiling). Output: OUT_DIR/<ID>_<nn>.mp3, and OUT_DIR/prompts.json recording exactly what was sent.
--budget stops the run once that many credits have been spent, read from the account before and
every ten requests (the meter lags, so it is a rough stop). --dry-run sends nothing.

The key is read from the environment (ELEVENLABS_API_KEY) or ~/.config/elevenlabs/key and is never
printed or written anywhere.
"""
import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHEET = HERE.parent.parent / "docs" / "SOUND_PROMPTS_20261109.md"
KEY_FILE = Path.home() / ".config" / "elevenlabs" / "key"
API = "https://api.elevenlabs.io"
ROW = re.compile(r"^\| ([A-Z]{2}-\d{2}) \| (.*?) \| (.*?) \| (\d+) \| (\d+) \|(.*?)\|\s*$")
DRY = "Close microphone, dry, no reverb, no echo, no music, no voices, single clean event."
# The owner's own wording for takes heard from inside (his starred generations, 2026-11-09).
INSIDE = ("Recorded inside the tank turret, tight stereo, resonant steel compartment, distinct low-frequency weight; "
          "the impact itself is outside the armour. No music, no voices.")
DRY_LOOP = "Close microphone, dry, no reverb, no echo, no music, no voices, steady and continuous."
FORMATS = ["mp3_44100_192", "mp3_44100_128"]
MODEL = "eleven_text_to_sound_v2"


def rows():
    out = []
    for raw in SHEET.read_text(encoding="utf-8").splitlines():
        m = ROW.match(raw)
        if m:
            rid, name, prompt, sec, takes, notes = m.groups()
            loop, wet = "loop" in notes, "wet" in notes
            text = prompt if wet else f"{prompt} {DRY_LOOP if loop else DRY}"
            out.append({"id": rid, "name": name, "text": text, "seconds": int(sec), "takes": int(takes), "loop": loop, "wet": wet})
    return out


def read_key():
    key = os.environ.get("ELEVENLABS_API_KEY", "").strip()
    if not key and KEY_FILE.exists():
        key = KEY_FILE.read_text().strip()
    return key


def used(key, last=0):
    """Credits used so far on the account; the last known figure if the service will not say."""
    req = urllib.request.Request(API + "/v1/user/subscription", headers={"xi-api-key": key})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read())["character_count"]
    except (urllib.error.URLError, KeyError, ValueError):
        return last


def generate(key, row, influence, state):
    body = {"text": row["text"], "duration_seconds": max(0.5, float(row["seconds"])), "prompt_influence": influence,
            "model_id": state.get("model", MODEL)}
    if row["loop"]:
        body["loop"] = True
    last = ""
    for fmt in ([state["fmt"]] if state.get("fmt") else FORMATS):
        req = urllib.request.Request(f"{API}/v1/sound-generation?output_format={fmt}", data=json.dumps(body).encode(),
                                     method="POST", headers={"xi-api-key": key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=300) as r:
                state["fmt"] = fmt
                return r.read()
        except urllib.error.HTTPError as e:
            last = f"HTTP {e.code}: {e.read().decode(errors='replace')[:400]}"
            if e.code == 429:
                time.sleep(20)
            if e.code in (401, 429):
                break
    raise RuntimeError(last)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out")
    ap.add_argument("--ids", default="")
    ap.add_argument("--groups", default="")
    ap.add_argument("--takes", type=int, default=1)
    ap.add_argument("--influence", type=float, default=0.4)
    ap.add_argument("--budget", type=int, default=0)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--inside", action="store_true", help="a take heard from inside the turret instead of the dry source; files <ID>_wNN.mp3")
    ap.add_argument("--model", default=MODEL)
    args = ap.parse_args()
    todo = rows()
    if args.inside:
        for r in todo:
            r["text"] = r["text"].replace(" " + DRY, "").replace(" " + DRY_LOOP, "") + " " + INSIDE
            r["seconds"] = max(r["seconds"], 3)
    if args.groups:
        order = args.groups.split(",")
        todo = sorted([r for r in todo if r["id"][:2] in order], key=lambda r: (order.index(r["id"][:2]), r["id"]))
    if args.ids:
        todo = [r for r in todo if r["id"] in args.ids.split(",")]
    jobs = [(r, t) for r in todo for t in range(1, min(args.takes, r["takes"]) + 1)]
    print(f"{len(jobs)} requests, {sum(r['seconds'] for r, _ in jobs)} seconds of sound")
    if args.dry_run:
        for r, t in jobs:
            print(f"  {r['id']}_{t:02d} {r['seconds']}s{' loop' if r['loop'] else ''}: {r['text']}")
        return
    key = read_key()
    if not key:
        sys.exit(f"no key: set ELEVENLABS_API_KEY or put it in {KEY_FILE}")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    log_path = out / "prompts.json"
    log = json.loads(log_path.read_text()) if log_path.exists() else {}
    start = spent_at = used(key)
    state, done, failed = {"model": args.model}, 0, 0
    for r, t in jobs:
        path = out / f"{r['id']}_{'w' if args.inside else ''}{t:02d}.mp3"
        if path.exists():
            continue
        if args.budget and spent_at - start >= args.budget:
            print(f"budget of {args.budget} credits reached; stopping")
            break
        try:
            data = generate(key, r, args.influence, state)
        except RuntimeError as e:
            failed += 1
            print(f"{path.name}: FAILED {e}")
            if failed >= 3 and done == 0:
                sys.exit("three failures and nothing rendered; stopping")
            continue
        path.write_bytes(data)
        now = used(key, spent_at) if (args.budget and done % 10 == 9) else spent_at
        log[path.name] = {"id": r["id"], "name": r["name"], "text": r["text"], "seconds": r["seconds"], "loop": r["loop"],
                          "influence": args.influence, "model": args.model}
        log_path.write_text(json.dumps(log, ensure_ascii=False, indent=1) + "\n")
        print(f"{path.name}: {r['seconds']} s  ({r['name']})")
        spent_at, done = now, done + 1
        time.sleep(0.3)
    time.sleep(5)
    print(f"rendered {done}, failed {failed}, about {used(key, spent_at) - start} credits spent (the meter lags), in {out}")


if __name__ == "__main__":
    main()
