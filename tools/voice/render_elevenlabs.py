#!/usr/bin/env python3
"""Render a crew line list with the ElevenLabs text-to-speech API. NOT YET RUN against the service:
the request shape is from memory of their documentation, so the first call may need a fix.

    python3 tools/voice/render_elevenlabs.py LINES.json OUT_DIR [--trial N] [--ids A,B] [--role KDT,RS]
                                             [--takes N] [--model eleven_v3] [--dry-run]

LINES.json is the list tools/voice/lines_md_to_json.py writes ({"id", "role", "int", "de", ...}).
VOICES maps each role to the voice the owner chose on 2026-11-09. Output: one 16-bit mono WAV per
line and take, OUT_DIR/<id>_<role>[_tN].wav, dry (no radio chain; tools/voice/chain_dir.py adds it).
--trial N keeps N lines per role spread over the five intensities. --dry-run sends nothing and
prints the line and character counts (characters are what the service bills).

The key is read from the environment (ELEVENLABS_API_KEY) or from ~/.config/elevenlabs/key, and is
never printed or written anywhere. Nothing is sent unless you run this.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
import wave
from pathlib import Path

API = "https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format={fmt}"
KEY_FILE = Path.home() / ".config" / "elevenlabs" / "key"
# The owner's picks, 2026-11-09. KP is the voice on the company net giving orders.
VOICES = {"KDT": "Cx9eG8CNCDtCPeLiemtm", "RS": "h1IssowVS2h4nL5ZbkkK", "LS": "LcVcltBiweqv2E4ipKan",
          "FA": "CVcPLXStXPeDxhrSflDZ", "FU": "Z2yQ1EdlDmcIgh9Pn4Lw", "KP": "FZI9WIiKSX4Tc340NLfq"}
# Tried in order until the account's plan accepts one; the first that works is kept for the run.
FORMATS = ["pcm_44100", "pcm_24000", "mp3_44100_128"]


def settings(model, level):
    """Steady for routine lines, loose for lines under fire. Eleven v3 takes stability only as
    0.0 (creative), 0.5 (natural) or 1.0 (robust); the older models take any value."""
    if model == "eleven_v3":
        return {"stability": 0.5 if level <= 3 else 0.0}
    return {"stability": 0.6 if level <= 2 else 0.35, "similarity_boost": 0.8,
            "style": 0.1 if level <= 2 else 0.45, "use_speaker_boost": True}


def read_key():
    key = os.environ.get("ELEVENLABS_API_KEY", "").strip()
    if not key and KEY_FILE.exists():
        key = KEY_FILE.read_text().strip()
    return key


def to_pcm(data, fmt):
    """(16-bit mono PCM bytes, sample rate) from one response body."""
    if fmt.startswith("pcm_"):
        return data, int(fmt.split("_")[1])
    out = subprocess.run(["ffmpeg", "-v", "error", "-i", "pipe:0", "-f", "s16le", "-ac", "1", "-ar", "44100", "pipe:1"],
                         input=data, capture_output=True, check=True).stdout
    return out, 44100


class Service:
    def __init__(self, key, model):
        self.key, self.model, self.fmt, self.plain = key, model, None, False

    def _post(self, voice, body, fmt):
        req = urllib.request.Request(API.format(voice=voice, fmt=fmt), data=json.dumps(body).encode(), method="POST",
                                     headers={"xi-api-key": self.key, "Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=180) as resp:
            return resp.read()

    def render(self, voice, text, level):
        """Try the full request, then one without voice settings or language, over each format;
        remember what worked. Raises with the service's own message when nothing does."""
        full = {"text": text, "model_id": self.model, "language_code": "de", "voice_settings": settings(self.model, level)}
        bodies = [{"text": text, "model_id": self.model}] if self.plain else [full, {"text": text, "model_id": self.model}]
        last = ""
        for fmt in ([self.fmt] if self.fmt else FORMATS):
            for n, body in enumerate(bodies):
                try:
                    data = self._post(voice, body, fmt)
                except urllib.error.HTTPError as e:
                    last = f"HTTP {e.code}: {e.read().decode(errors='replace')[:400]}"
                    if e.code in (401, 429):
                        raise RuntimeError(last)
                    continue
                if self.fmt is None:
                    print(f"  (service accepted {fmt}{', without voice settings' if n or self.plain else ''})")
                self.fmt, self.plain = fmt, self.plain or (len(bodies) > 1 and n == 1)
                return to_pcm(data, fmt)
        raise RuntimeError(last)


def trial(lines, per_role):
    """per_role lines for each role, spread evenly over the intensities present."""
    out = []
    for role in dict.fromkeys(l["role"] for l in lines):
        rows = [l for l in lines if l["role"] == role]
        levels = sorted({l["int"] for l in rows})
        picks, k = [], 0
        while len(picks) < min(per_role, len(rows)):
            lv = [l for l in rows if l["int"] == levels[k % len(levels)] and l not in picks]
            if lv:
                picks.append(lv[(k // len(levels)) * len(lv) // max(1, -(-per_role // len(levels))) % len(lv)])
            k += 1
            if k > 10 * per_role:
                break
        out += sorted(picks, key=lambda l: l["id"])
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lines")
    ap.add_argument("out")
    ap.add_argument("--model", default="eleven_v3", help="eleven_v3 or eleven_multilingual_v2")
    ap.add_argument("--trial", type=int, default=0, help="N lines per role across the intensities")
    ap.add_argument("--ids", default="", help="comma-separated line ids")
    ap.add_argument("--role", default="", help="comma-separated roles")
    ap.add_argument("--takes", type=int, default=1)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()
    lines = json.load(open(args.lines, encoding="utf-8"))
    if args.role:
        lines = [l for l in lines if l["role"] in args.role.split(",")]
    if args.ids:
        lines = [l for l in lines if l["id"] in args.ids.split(",")]
    if args.trial:
        lines = trial(lines, args.trial)
    missing = sorted({l["role"] for l in lines if not VOICES.get(l["role"])})
    if missing:
        sys.exit(f"no voice for {missing}")
    chars = sum(len(l["de"]) for l in lines) * args.takes
    print(f"{len(lines)} lines x {args.takes} take(s), {chars} characters, model {args.model}")
    if args.dry_run:
        for l in lines:
            print(f"  {l['id']} [{l['int']}] {l['de']}")
        return
    key = read_key()
    if not key:
        sys.exit(f"no key: set ELEVENLABS_API_KEY or put it in {KEY_FILE} (it is never printed)")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    service = Service(key, args.model)
    done = failed = 0
    for line in lines:
        for take in range(1, args.takes + 1):
            path = out / (f"{line['id']}_{line['role']}" + (f"_t{take}" if args.takes > 1 else "") + ".wav")
            if path.exists():
                continue
            try:
                pcm, rate = service.render(VOICES[line["role"]], line["de"], line.get("int", 2))
            except RuntimeError as e:
                failed += 1
                print(f"{path.name}: FAILED {e}")
                if failed >= 3 and done == 0:
                    sys.exit("three failures and nothing rendered; stopping so no credits are wasted")
                continue
            with wave.open(str(path), "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(rate)
                w.writeframes(pcm)
            done += 1
            print(f"{path.name}: {len(pcm) / (2 * rate):.2f} s")
            time.sleep(0.3)
    print(f"rendered {done}, failed {failed}, in {out}")


if __name__ == "__main__":
    main()
