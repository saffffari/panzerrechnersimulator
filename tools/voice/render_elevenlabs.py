#!/usr/bin/env python3
"""Render a crew line list with the ElevenLabs text-to-speech API. UNTESTED: written without an
account or a key, never run against the service (docs/CREW_VOICE_OPTIONS_20261108.md).

    ELEVENLABS_API_KEY=... python3 tools/voice/render_elevenlabs.py LINES.json OUT_DIR [--model eleven_v3]

LINES.json is a list of objects {"id", "role", "register", "de"} (the same lines as
tools/voice/crew_lines.json). VOICES maps each role to an ElevenLabs voice id you have chosen or
designed (a deep German male voice per crewman); fill it in before running. Output: one 16-bit
44.1 kHz mono WAV per line, OUT_DIR/<id>_<role>.wav, dry (no radio chain; run
tools/voice/crew_chain.py on them). The key is read from the environment only and is never printed
or written anywhere. Nothing is sent unless you run this yourself.
"""
import argparse
import json
import os
import sys
import time
import urllib.request
import wave
from pathlib import Path

API = "https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=pcm_44100"
# Fill in: role -> voice id (from the voice library or Voice Design). Left empty on purpose.
VOICES = {"KDT": "", "RS": "", "LS": "", "FA": "", "FU": ""}
# Steadier delivery for calm reports; more variation (not more pitch) for urgent ones. Eleven v3
# also reads audio tags in the text, e.g. "[urgent] Panzer, drei Uhr!"; tags are not added here.
SETTINGS = {"calm": dict(stability=0.6, similarity_boost=0.8, style=0.1, use_speaker_boost=True),
            "urgent": dict(stability=0.35, similarity_boost=0.8, style=0.45, use_speaker_boost=True)}


def render(key, voice, text, model, settings):
    body = json.dumps({"text": text, "model_id": model, "language_code": "de",
                       "voice_settings": settings}).encode()
    req = urllib.request.Request(API.format(voice=voice), data=body, method="POST",
                                 headers={"xi-api-key": key, "Content-Type": "application/json",
                                          "Accept": "audio/pcm"})
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lines")
    ap.add_argument("out")
    ap.add_argument("--model", default="eleven_v3", help="eleven_v3 or eleven_multilingual_v2")
    args = ap.parse_args()
    key = os.environ.get("ELEVENLABS_API_KEY", "")
    if not key:
        sys.exit("set ELEVENLABS_API_KEY in the environment (it is never printed)")
    missing = [r for r, v in VOICES.items() if not v]
    if missing:
        sys.exit(f"fill in VOICES for {missing} first")
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for line in json.load(open(args.lines, encoding="utf-8")):
        path = out / f"{line['id']}_{line['role']}.wav"
        if path.exists():
            continue
        pcm = render(key, VOICES[line["role"]], line["de"], args.model, SETTINGS[line.get("register", "calm")])
        with wave.open(str(path), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(44100)
            w.writeframes(pcm)
        print(f"{path.name}: {len(pcm) / 88200:.2f} s")
        time.sleep(0.3)


if __name__ == "__main__":
    main()
