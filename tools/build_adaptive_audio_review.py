#!/usr/bin/env python3
"""Local listening page for the real device-free adaptive Audio graph receipt."""
from __future__ import annotations
import array
import hashlib
from html import escape
import json
import math
from pathlib import Path
import re
import sys
import wave

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / 'captures'


def capture_file(path: Path) -> Path:
    path = path.resolve(strict=True)
    if path.parent != CAP.resolve() or not path.is_file():
        raise ValueError(f'Expected a regular file directly in captures: {path}')
    return path


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def build(argument: str) -> Path:
    if re.fullmatch(r'[A-Za-z0-9_]+', argument):
        path = CAP / f'{argument}_receipt.json'
    else:
        path = Path(argument)
        if not path.is_absolute():
            path = ROOT / path
    path = capture_file(path)
    data = json.loads(path.read_text())
    if data.get('schema') != 'panzer.adaptive-audio-review.v1' or data.get('not_gameplay_capture') is not True:
        raise ValueError('Expected the adaptive Audio graph fixture receipt')
    if not path.name.endswith('_receipt.json'):
        raise ValueError('Expected *_receipt.json')
    stem = path.name.removesuffix('_receipt.json')
    output = CAP / f'{stem}_adaptive_review.html'
    validation = ROOT / 'artifacts/runtime' / f'{stem}_adaptive_page.json'
    if output.exists() or validation.exists():
        raise FileExistsError(f'Preserve previous review: {output}')
    media = []
    seen = set()

    def inspect(file: str) -> tuple[Path, dict]:
        file = Path(file)
        file = capture_file(file if file.is_absolute() else ROOT / file)
        if file.suffix.lower() != '.wav' or file in seen:
            raise ValueError(f'Expected unique WAV: {file}')
        seen.add(file)
        with wave.open(str(file), 'rb') as stream:
            params = stream.getparams()
            raw = stream.readframes(params.nframes)
        if (params.nchannels, params.sampwidth, params.framerate, params.comptype) != (2, 2, 44100, 'NONE'):
            raise ValueError(f'Unexpected PCM format: {file}: {params}')
        if params.nframes <= 0 or len(raw) != params.nframes * 4:
            raise ValueError(f'Incomplete PCM: {file}')
        samples = array.array('h', raw)
        if sys.byteorder != 'little':
            samples.byteswap()
        peak = max(abs(v) for v in samples) / 32768.0
        rms = math.sqrt(sum((v / 32768.0) ** 2 for v in samples) / len(samples))
        row = dict(file=str(file), sha256=digest(file), frames=params.nframes,
                   sample_rate=params.framerate, channels=params.nchannels,
                   seconds=params.nframes / params.framerate, complete_pcm=True,
                   full_scale_samples=sum(v in (-32768, 32767) for v in samples),
                   peak_dbfs=20 * math.log10(max(peak, 1e-12)),
                   rms_dbfs=20 * math.log10(max(rms, 1e-12)))
        media.append(row)
        return file, row

    transition, transition_meta = inspect(data['transitions_file'])
    death, death_meta = inspect(data['death']['file'])
    nights = [(bed, *inspect(bed['file'])) for bed in data['night_beds']]
    solar = [(item, *inspect(item['file'])) for item in data.get('solar_cues', [])]
    expected_audio_count = 2 + len(nights) + len(solar)
    times = {key: float(data['death'][key]) for key in
             ['fatal_at_s', 'black_at_s', 'text_midpoint_at_s', 'cue_end_at_s']}
    ordered = [times[key] for key in ['fatal_at_s', 'black_at_s', 'text_midpoint_at_s', 'cue_end_at_s']]
    if not all(math.isfinite(v) for v in ordered) or not 0 <= ordered[0] < ordered[1] < ordered[2] < ordered[3] <= death_meta['seconds']:
        raise ValueError(f'Invalid death timeline: {times}')
    passed = all(value is True for value in data['checks'].values())
    passed &= all(value is True for value in data['death']['checks'].values())

    def audio(file: Path, name: str, ident: str = '') -> str:
        return f'<audio controls preload="metadata" id="{escape(ident, quote=True)}" aria-label="{escape(name, quote=True)}" src="{escape(file.name, quote=True)}"></audio>'

    def stamp(t: float) -> str:
        return f'{int(t)//60}:{int(t)%60:02d}'

    timeline = ''.join(f'<button type="button" data-time="{float(phase["start_s"]):.4f}">{stamp(float(phase["start_s"]))} · {escape(phase["label"])}</button>' for phase in data['phases'])
    night_cards = ''.join(f'<article><h3>Winter night {i}</h3>{audio(file, f"Winter night {i}")}<p>{meta["seconds"]:.0f} seconds; includes the loop join.</p><details><summary>Selected library source</summary><code>{escape(bed["id"])}</code></details></article>' for i, (bed, file, meta) in enumerate(nights, 1))
    solar_cards = ''.join(f'<article><h3>{escape(item["event"].capitalize())}</h3>{audio(file, item["event"])}<p>One-shot; {meta["seconds"]:.0f} seconds including tail.</p></article>' for item, file, meta in solar)
    check_rows = ''.join(f'<tr><td>{escape(key.replace("_", " "))}</td><td>{"PASS" if value is True else "FAIL"}</td></tr>' for key, value in data['death']['checks'].items())
    status = 'Technical checks passed; listening acceptance pending.' if passed else 'Diagnostic capture: at least one technical check failed.'
    page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Panzer · adaptive audio review</title>
<style>:root{font:16px/1.5 system-ui;color-scheme:dark;background:#101510;color:#e3e6da}body{max-width:1100px;margin:auto;padding:32px 24px 60px}h1{font-size:30px;margin-bottom:12px}h2{font-size:23px}h3{font-size:17px}p{color:#bcc6b6;max-width:880px}section{border-top:1px solid #364133;padding-top:22px;margin-top:28px}audio{width:100%}article{background:#1b241b;border:1px solid #364133;border-radius:6px;padding:20px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:15px}button{border:1px solid #506047;border-radius:4px;background:#263323;color:#e4eadb;padding:9px 12px;text-align:left;cursor:pointer}.timeline{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}.note{padding:15px 18px;background:#1b241b;border-left:3px solid #95a17f}.death-status{background:#050705;border:1px solid #364133;padding:18px;margin:15px 0;color:#ced6c5}details,footer{font-size:13px;color:#a9b59f}code{overflow-wrap:anywhere}table{border-collapse:collapse}td{border-bottom:1px solid #364133;padding:7px 16px 7px 0}a{color:#c2dba5}@media(max-width:750px){.grid{grid-template-columns:1fr}body{padding:24px 16px}}</style>
<h1>Adaptive audio review</h1><p>Context changes, winter-night loops and the death chord through the real, device-free Audio mixer. These are scripted listening fixtures, not gameplay recordings. No playback normalization or replacement soundtrack was applied.</p>
''' + f'<p class="note">{escape(status)} One audio player runs at a time; nothing plays automatically.</p>' + f'''
<section><h2>Context transitions</h2><p>Night outside, cabin, driving, nearby enemies, combat, aftermath, a new night, then whiteout outside and inside followed by clearing. The buttons seek within the actual recorded timeline.</p>{audio(transition, 'Context transitions', 'transitions')}<div class="timeline">{timeline}</div></section>
<section><h2>Winter-night variations</h2><p>Each selected cue runs long enough to include its loop transition through the same transport and master.</p><div class="grid">{night_cards}</div></section>
<section><h2>Daylight transitions</h2><div class="grid">{solar_cards}</div></section>
<section><h2>Fatal impact and the continuing chord</h2><p>A staged ammunition explosion starts at the fatal event. Battlefield audio fades to silence; the prepared heavenly chord starts only at the memorial text reveal and then plays its entire source and tail. The following status shows receipt timing only; it is not a capture of the game’s death screen.</p>{audio(death, 'Fatal impact and death chord', 'death')}<div class="death-status" id="death-status">Before the fatal event</div>
<p>Impact: {times['fatal_at_s']:.2f}s · battlefield cut: {times['black_at_s']:.2f}s · text midpoint cue: {times['text_midpoint_at_s']:.2f}s · chord end: {times['cue_end_at_s']:.2f}s.</p><details><summary>Death-route checks</summary><table>{check_rows}</table></details></section>
<footer><p><a href="{escape(path.name, quote=True)}">Actual capture receipt</a>. Technical metrics establish completeness and routing; they do not establish that the sounds match the references or are accepted.</p></footer>
<script>
const players=[...document.querySelectorAll('audio')];players.forEach(active=>active.addEventListener('play',()=>players.forEach(other=>{{if(other!==active)other.pause()}})));
const transition=document.getElementById('transitions');document.querySelectorAll('[data-time]').forEach(button=>button.addEventListener('click',()=>{{transition.currentTime=Number(button.dataset.time)}}));
const death=document.getElementById('death'),status=document.getElementById('death-status'),times={json.dumps(times)};
function updateDeath(){{const t=death.currentTime;status.textContent=t<times.fatal_at_s?'Before the fatal event':t<times.black_at_s?'Fatal impact: battlefield fades; chord has not started':t<times.text_midpoint_at_s?'Black and silence; waiting for memorial text':t<times.cue_end_at_s?'Memorial text revealed; full heavenly chord plays':'Chord finished; final silence'}}
death.addEventListener('timeupdate',updateDeath);death.addEventListener('seeked',updateDeath);death.addEventListener('ended',updateDeath);
</script></html>'''
    links=[]
    for file,label in [("adaptive_weapon_candidates_v2_20261002_weapon_review.html","Main-gun and MG comparisons"),
                       ("adaptive_death_card_final_20261002.png","Rendered death card")]:
        if (CAP/file).is_file(): links.append(f'<a href="{escape(file)}">{escape(label)}</a>')
    if links: page=page.replace('<h1>Adaptive audio review</h1>', '<h1>Adaptive audio review</h1><p>'+' · '.join(links)+'</p>')
    output.write_text(page)
    validation.parent.mkdir(parents=True, exist_ok=True)
    validation.write_text(json.dumps(dict(schema='panzer.adaptive-audio-page-validation.v1',
        page=str(output), receipt=str(path), receipt_sha256=digest(path), audio_count=expected_audio_count,
        media=media, all_media_complete=True, technical_checks_passed=passed,
        autoplay=False, normalization=False, gameplay_capture=False,
        subjective_listening_accepted=False), indent=2) + '\n')
    print(output)
    return output


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: build_adaptive_audio_review.py PREFIX_OR_RECEIPT')
    build(sys.argv[1])
