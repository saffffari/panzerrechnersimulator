#!/usr/bin/env python3
"""Build a local listening page from the actual weapon-timbre receipt and its nine WAVs."""
from __future__ import annotations
from pathlib import Path
from html import escape
import hashlib
import json
import re
import sys
import wave

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"


def inside_capture(path: Path) -> Path:
    resolved = path.resolve(strict=True)
    if resolved.parent != CAP.resolve() or not resolved.is_file():
        raise ValueError(f"Expected a regular file directly in captures: {path}")
    return resolved


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build(argument: str) -> Path:
    if re.fullmatch(r"[A-Za-z0-9_]+", argument):
        receipt_path = CAP / f"{argument}_receipt.json"
    else:
        receipt_path = Path(argument)
        if not receipt_path.is_absolute():
            receipt_path = ROOT / receipt_path
    receipt_path = inside_capture(receipt_path)
    if not receipt_path.name.endswith("_receipt.json"):
        raise ValueError("Expected the actual *_receipt.json capture receipt")
    receipt = json.loads(receipt_path.read_text())
    if receipt.get("schema") != "panzer.weapon-timbre-candidates.v1":
        raise ValueError("This page requires the per-round weapon candidate receipt")
    rows = receipt.get("takes", [])
    expected = {
        *(('main_gun', candidate, route)
          for candidate in ('previous_modal', 'pressure_body', 'compact_pressure')
          for route in ('buttoned', 'head_out')),
        *(('machine_gun', candidate, 'buttoned')
          for candidate in ('previous', 'body_and_crack', 'sharper')),
    }
    keyed = {}
    media = []
    for row in rows:
        key = (row.get('kind'), row.get('candidate'), row.get('route'))
        if key not in expected or key in keyed:
            raise ValueError(f"Unexpected or duplicate take: {key}")
        file = Path(row['file'])
        file = inside_capture(file if file.is_absolute() else ROOT / file)
        if file.suffix.lower() != '.wav':
            raise ValueError(f"Expected WAV: {file}")
        with wave.open(str(file), 'rb') as stream:
            params = stream.getparams()
            pcm = stream.readframes(params.nframes)
        if (params.nchannels, params.sampwidth, params.framerate, params.comptype) != (2, 2, 44100, 'NONE'):
            raise ValueError(f"Unexpected capture PCM format: {file}: {params}")
        if not params.nframes or len(pcm) != params.nframes * params.nchannels * params.sampwidth:
            raise ValueError(f"Incomplete PCM payload: {file}")
        if row['kind'] == 'machine_gun' and (row.get('legacy_loop_used') is not False
                or row.get('rounds') != 24 or row.get('event_count') != 48
                or row.get('hull_headset_and_hearing_enabled') is not True):
            raise ValueError(f"MG take does not prove the actual per-round route: {file}")
        keyed[key] = (row, file)
        media.append(dict(file=file.name, sha256=digest(file), channels=params.nchannels,
                          sample_rate_hz=params.framerate, frames=params.nframes,
                          duration_s=params.nframes / params.framerate, complete_pcm_payload=True))
    if set(keyed) != expected:
        raise ValueError(f"Missing takes: {expected - set(keyed)}")
    stem = receipt_path.name.removesuffix('_receipt.json')
    output = CAP / f'{stem}_weapon_review.html'
    validation = ROOT / 'artifacts/runtime' / f'{stem}_weapon_page.json'
    if output.exists() or validation.exists():
        raise FileExistsError(f"Preserve the previous review: {output}")

    def player(kind: str, candidate: str, route: str, label: str) -> str:
        row, file = keyed[(kind, candidate, route)]
        title = escape(label)
        return (f'<article><h3>{title}</h3>'
                f'<audio controls preload="metadata" aria-label="{title}" src="{escape(file.name, quote=True)}"></audio>'
                f'<p class="route">{escape("Inside · headset on" if route == "buttoned" else "Head out · headset on")}</p>'
                '<details><summary>Measurement and route</summary><dl>'
                f'<dt>Peak</dt><dd>{float(row["peak_dbfs"]):.2f} dBFS</dd>'
                f'<dt>Short-term A-weighted level</dt><dd>{float(row["measured_laf_dbfs"]):.2f} dBFS</dd>'
                f'<dt>Route</dt><dd>{escape(row["playback"])}</dd></dl></details></article>')

    guns = [('previous_modal', 'Previous · rejected modal source'),
            ('pressure_body', 'Candidate A · pressure body'),
            ('compact_pressure', 'Candidate B · compact pressure')]
    sections = []
    for route, title in [('buttoned', 'Main gun · inside'), ('head_out', 'Main gun · head out')]:
        sections.append(f'<section><h2>{title}</h2><p>One shot at 0.25 seconds. Report only; recoil and reload mechanisms are excluded.</p><div class="takes">'
                        + ''.join(player('main_gun', key, route, label) for key, label in guns) + '</div></section>')
    sections.append('<section><h2>MG34 · inside</h2><p>24 rounds at 800 RPM. Each round includes its local receiver and separately propagated muzzle report. These are not the old repeating MG loop.</p><div class="takes">'
                    + ''.join(player('machine_gun', key, 'buttoned', label) for key, label in [
                        ('previous', 'Previous · per-round source'),
                        ('body_and_crack', 'Candidate A · body and crack'),
                        ('sharper', 'Candidate B · sharper attack')]) + '</div></section>')
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzer · weapon sound candidates</title><style>
:root{color-scheme:dark;font:16px/1.5 system-ui;background:#121512;color:#e1e2d8}body{max-width:1180px;margin:auto;padding:34px 24px 60px}h1{font-size:30px;margin:0 0 12px}h2{font-size:22px}h3{font-size:16px;margin:0 0 18px}p{max-width:900px;color:#bdc4b9}.takes{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:16px}article{padding:20px;background:#1c231c;border:1px solid #354033;border-radius:7px}audio{width:100%}section{border-top:1px solid #354033;padding-top:22px;margin-top:30px}.route,details{font-size:13px;color:#aeb9a9}summary{cursor:pointer}dl{margin-bottom:0}dt{margin-top:8px;font-weight:600}dd{margin:0;overflow-wrap:anywhere}a{color:#c4dcad}.note{padding:16px 20px;border-left:3px solid #82926e;background:#192018}footer{margin-top:30px;font-size:13px}@media(max-width:850px){.takes{grid-template-columns:1fr}body{padding:24px 16px}}
</style><h1>Weapon sound candidates</h1>
<p>Compare the previous sound with two new candidates at the same physical level mapping. Playback levels are unaltered; these files are not loudness-normalized. Only one take plays at a time.</p>
<p class="note">Each take starts with a fresh listener. The normal headset, hearing response and middle-ear reflex remain enabled and evolve during playback. This is a controlled mixer capture, not a recording of a complete firefight. A technically valid take is not an accepted sound.</p>
''' + ''.join(sections) + '''<section><h2>What to listen for</h2><p>For the main gun, compare the initial pressure hit and its decay, especially whether a pitched note remains. For the MG, compare lower body, upper crack and separation between rounds. Candidate A is the working recommendation for review, not a settled choice.</p><p>The MG uses a declared two-metre exterior muzzle path through the buttoned hull, plus the local receiver path. Its muzzle delay follows winter air propagation. The geometry is a listening fixture, not an exact coax-location measurement. No supplied film or reference WAV was imported into these sources.</p></section>
''' + f'<footer><a href="{escape(receipt_path.name, quote=True)}">Actual capture receipt</a> · Source and route details are recorded for every take.</footer>' + '''<script>
const players=[...document.querySelectorAll('audio')];players.forEach(active=>active.addEventListener('play',()=>players.forEach(other=>{if(other!==active)other.pause()})));
</script></html>'''
    output.write_text(page)
    validation.parent.mkdir(parents=True, exist_ok=True)
    validation.write_text(json.dumps(dict(schema='panzer.weapon-page-validation.v1', page=str(output),
        receipt=str(receipt_path), receipt_sha256=digest(receipt_path), audio_count=len(media),
        media=media, all_audio_same_capture_parent=True, autoplay=False, normalization=False,
        subjective_acceptance='not established'), indent=2) + '\n')
    print(output)
    return output


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: build_weapon_timbre_review.py PREFIX_OR_CAPTURE_RECEIPT')
    build(sys.argv[1])
