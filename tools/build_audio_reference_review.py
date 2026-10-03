#!/usr/bin/env python3
"""Local listening comparison; original references and unnormalized game captures."""
from pathlib import Path
import hashlib
import html
import json
import shutil
import sys

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / 'captures'
prefix = sys.argv[1]
assert Path(prefix).name == prefix and prefix.replace('_', '').isalnum()
output = CAP / f'{prefix}_reference_review.html'
assert not output.exists(), f'Preserve prior review: {output}'
receipt = json.loads((CAP / f'{prefix}_receipt.json').read_text())

def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

references = {}
for source_name in (
    'main_gun_and_reload_perfect.wav', 'main_gun_firing_inside.wav',
    'reloading.wav', 'mg_inside.wav',
    'incoming_tank_round_non_prenetrating.wav',
    'heavy_incoming_round_non_penetrating.wav',
    'incoming_tank_round_miss.wav', 'engine_and_hatch_closing.wav',
    'front_mg_and_interior_impact_sound.wav',
):
    source = Path('/home/alex/Recordings') / source_name
    target = CAP / f'listening_reference_{source_name}'
    if not target.exists():
        shutil.copyfile(source, target)
    assert digest(source) == digest(target), f'Reference differs: {target}'
    references[source_name] = dict(source=str(source), file=target.name, sha256=digest(target))

def player(file, caption):
    return (f'<div class="take"><p>{html.escape(caption)}</p>'
            f'<audio controls preload="metadata" src="{html.escape(file, quote=True)}"></audio></div>')

pairs = [
    ('Main gun inside', 'main_gun_and_reload_perfect.wav', 'own_gun_buttoned_ears',
     'Complete reference: shot and reload.',
     'Game report through the live mixer and hearing model. Isolated event; excludes recoil mechanisms and reload.'),
    ('Machine gun inside', 'mg_inside.wav', 'mg34_burst_buttoned',
     'Interior MG reference.',
     'Game MG34 burst with the interior transfer, at 800 rounds per minute.'),
    ('Stopped armor hit', 'incoming_tank_round_non_prenetrating.wav', 'hit_distant_frontal_ricochet',
     'User-labeled non-penetrating impact.',
     'Game frontal ricochet fixture. Not the same caliber, angle or recording position as the reference.'),
]
sections = []
for title, ref, event, ref_caption, game_caption in pairs:
    game_file = f'{prefix}_{event}.wav'
    assert (CAP / game_file).is_file(), game_file
    sections.append(f'<section><h2>{html.escape(title)}</h2><div class="pair">'
                    + player(references[ref]['file'], 'REFERENCE · ' + ref_caption)
                    + player(game_file, 'CURRENT GAME · ' + game_caption) + '</div></section>')
extra = [
    ('reloading.wav', 'Isolated reload — mechanical scrape, seating and contacts'),
    ('heavy_incoming_round_non_penetrating.wav', 'Heavy non-penetrating hit'),
    ('main_gun_firing_inside.wav', 'Alternate interior gun shot'),
    ('incoming_tank_round_miss.wav', 'Incoming round passage / miss'),
    ('engine_and_hatch_closing.wav', 'Engine and hatch closure'),
    ('front_mg_and_interior_impact_sound.wav', 'Front MG and interior armor strikes'),
]
extra_html = ''.join(player(references[name]['file'], label) for name, label in extra)
output.write_text('''<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzer · interior sound reference review</title>
<style>
:root{color-scheme:dark;font:16px/1.55 system-ui;background:#111411;color:#dddcd2}
body{max-width:1080px;margin:0 auto;padding:38px 24px 64px}h1{font-size:30px;margin-bottom:10px}
h2{font-size:21px;margin:0 0 18px}.intro{max-width:850px;color:#bcbdb4}section{border-top:1px solid #353b33;margin-top:34px;padding-top:25px}
.pair,.more{display:grid;grid-template-columns:1fr 1fr;gap:22px}.take{background:#1b201b;padding:18px;border-radius:6px}.take p{margin:0 0 16px;font-size:14px;min-height:44px}audio{width:100%}small{color:#a8aea2}a{color:#c4d3af}summary{cursor:pointer;margin-bottom:20px;font-size:20px}footer{margin-top:35px}
@media(max-width:650px){.pair,.more{grid-template-columns:1fr}body{padding:22px 15px}}
</style><h1>Interior sound reference review</h1>
<p class="intro">Compare the supplied recordings with captured game events. Each game take uses the ordinary playback graph. These are staged listening fixtures, not a recording of a complete firefight.</p>
<p class="intro">The files retain their original levels. They are not loudness-matched, so compare the attack, body and decay as well as apparent volume. Reference recordings are listening targets, not measured sound-pressure levels.</p>
''' + ''.join(sections) + '<section><details><summary>Additional reference recordings</summary><div class="more">'
    + extra_html + '</div></details></section>'
    + f'<footer><small>Current export: {html.escape(prefix)}. '
    + f'<a href="{html.escape(prefix)}_receipt.json">Capture provenance and technical checks</a>. '
    + 'Technical checks do not establish a subjective match to the references.</small></footer>'
    + '''<script>document.querySelectorAll('audio').forEach(a=>a.addEventListener('play',()=>document.querySelectorAll('audio').forEach(b=>{if(b!==a)b.pause()})))</script></html>''')
manifest = dict(page=str(output), reference_copies=references,
                game_receipt=str(CAP / f'{prefix}_receipt.json'),
                source_levels='unchanged; not loudness-matched',
                subjective_reference_match='not established')
(ROOT / 'artifacts/runtime' / f'{prefix}_reference_review.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(output)
