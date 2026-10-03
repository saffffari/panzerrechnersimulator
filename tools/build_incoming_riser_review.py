#!/usr/bin/env python3
"""Build a listening page from the explicit incoming-riser mixer fixture receipt."""
import html
import json
from pathlib import Path
import sys

prefix = Path(sys.argv[1])
receipt = json.loads(Path(str(prefix) + '_receipt.json').read_text())
output = Path(str(prefix) + '_review.html')
if output.exists():
    raise SystemExit(f'Preserve existing review: {output}')
labels = {
    'hit': ('Riser into armor hit', 'Four-second rise; staged non-penetrating impact at 5.0 seconds.'),
    'miss': ('Prediction fails', 'The incoming shell misses at 4.5 seconds. The riser fades; no impact is invented.'),
    'uncued': ('Impact without warning', 'The same staged armor hit at 5.0 seconds, without a riser.'),
    'fakeout': ('Suspense without an attack', 'The same riser family rises and resolves independently. No gunshot or impact.'),
}
rows = []
for item in receipt['fixtures']:
    file = Path(item['file'])
    if not file.is_file():
        raise SystemExit(f'Missing fixture: {file}')
    title, detail = labels[item['scenario']]
    rows.append(f'<article><h2>{title}</h2><p>{detail}</p><audio controls preload="metadata" src="{html.escape(file.name)}"></audio></article>')
output.write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Incoming riser listening review</title>
<style>body{background:#111512;color:#e3e8dd;font:17px/1.6 system-ui;max-width:900px;margin:60px auto;padding:0 25px}h1{font-size:34px;line-height:1.2}h2{font-size:21px}p{color:#b8c0b4}article{border-top:1px solid #465041;padding:18px 0}audio{width:100%}small{color:#adbda8}a{color:#d6d9a9}</style>
<h1>Suspense that sometimes meets a shell</h1><p>Optional risers for apparently unnoticed firing opportunities. Most attacks stay uncued; occasional free suspense uses the same recording.</p><small>Staged audio fixtures through the game's Audio mixer. These are not gameplay recordings. Same output level across clips; no autoplay or per-clip normalization.</small>
''' + '\n'.join(rows) + f'<p><a href="{html.escape(prefix.name)}_receipt.json">Timing and admission receipt</a></p></html>')
print(output)
