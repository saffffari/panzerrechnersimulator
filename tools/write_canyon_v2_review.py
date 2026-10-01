#!/usr/bin/env python3
"""Flat, portable review of actual Canyon v2 renderer output."""
from pathlib import Path
from html import escape
import json
import shutil

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'captures'
receipt=json.loads((OUT/'canyon_v2_gallery_20260930.json').read_text())
cards=[]
for s in receipt['shots']:
    if s['exit_code']:raise ValueError(f"Capture failed: {s['name']}")
    path=Path(s['output'])
    if not path.is_file():raise FileNotFoundError(path)
    cards.append(f'<figure data-kind="{"aerial" if s["name"].startswith("aerial") else "player"}"><a href="{path.name}"><img loading="lazy" src="{path.name}" alt="{escape(s["title"])}"></a><figcaption><b>{escape(s["title"])}</b><p>{escape(s["type"])}{ " · reduced physical fog for inspection" if s["name"].startswith("aerial") else ""}</p></figcaption></figure>')
paper='canyon_v2_survey_20260930'
for ext in ['png','svg','json']:
    shutil.copy2(ROOT/'catalog/maps'/f'{paper}.{ext}',OUT/f'{paper}.{ext}')
cards.append(f'<figure data-kind="map"><a href="{paper}.png"><img loading="lazy" src="{paper}.png" alt="Canyon v2 paper survey"></a><figcaption><b>In-game paper drawing</b><p>Derived from the actual Gaea terrain, route and placed buildings. <a href="{paper}.svg">Editable SVG</a></p></figcaption></figure>')
before=[]
for name in ['player_settlement','settlement_yard','aerial_settlement_detail']:
    path=OUT/f'canyon_v2_pre_reference_{name}_20260930.png'
    if path.exists():before.append(f'<figure data-kind="before"><a href="{path.name}"><img loading="lazy" src="{path.name}" alt="Before reference pass: {name}"></a><figcaption><b>Before · {escape(name.replace("_"," "))}</b><p>Identical camera before the new road, yard and overcast authoring.</p></figcaption></figure>')
clips=[]
for name,title,kind,note in [
    ('reload','Manual case extraction and cartridge ramming','mechanics','Extraction, ramming and two shots pass their state checks. Heavy-mechanism audio is present; listening review remains pending.'),
    ('hammer_grip','Hammer grip and swing','mechanics','The head is rolled into the swing plane. State checks and sampled frames reviewed.'),
    ('sight_exit','Gunsight exit and cupola access','mechanics','Exit beside the gun sight, then reach the cupola through the interior. State checks and sampled frames reviewed.'),
    ('fire','Spreading fire and burning audio','diagnostics','Spread checks pass. Flames move, but the soot particles still read as cubes; this effect needs another visual pass.'),
    ('terminal','Recorded fatal shot and interior replay','diagnostics','The actual fatal record is replayed without another damage application. Some interior highlights clip; this is not lighting approval.'),
    ('cabin_day_player','Daylight training cabin','diagnostics','Ordinary player rendering and state checks pass. These training views remain brighter than the intended Canyon cabin treatment.'),
    ('cabin_night_player','Night training cabin','diagnostics','Visual issue: practical lights and reflected highlights clip to white. State checks pass, but the lighting has not passed visual acceptance.'),
]:
    path=OUT/f'canyon_v2_{name}_20260930.mp4'
    if not path.exists():raise FileNotFoundError(path)
    if name!='terminal':
        checks=json.loads((OUT/f'canyon_v2_{name}_20260930_manifest.json').read_text())
        if not checks['outcome_checks_passed'] or not checks['required_timeline_actions_passed']:
            raise ValueError(f'Mechanic state checks failed: {name}')
    else:
        checks=json.loads((OUT/'canyon_v2_terminal_20260930_segments.json').read_text())
        if not all(s.get('damage_reapplied') is False for s in checks['segments']):
            raise ValueError('Replay damage check failed')
    clips.append(f'<figure data-kind="{kind}"><video controls preload="metadata" src="{path.name}"></video><figcaption><b>{title}</b><p>{escape(note)}</p><p>Staged native fixture; not a completed player or AI Canyon run.</p></figcaption></figure>')
menu=OUT/'canyon_v2_menu_20260930.png'
if menu.exists():clips.append(f'<figure data-kind="mechanics"><a href="{menu.name}"><img loading="lazy" src="{menu.name}" alt="Native title menu with Canyon Run v2 selected"></a><figcaption><b>Separate Canyon Run v2 entry</b><p>Native title-menu capture. Desktop launcher also installed on HAL.</p></figcaption></figure>')
audio='canyon_v2_shell_pass_20260930_comparison.wav'
if (OUT/audio).exists():clips.append(f'<figure data-kind="mechanics"><figcaption><b>Incoming shell pass</b><p>Isolated synthesized 76.2 mm pass: head out, then buttoned. Shared game audio mixer.</p><audio controls preload="metadata" src="{audio}"></audio></figcaption></figure>')
html='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzerrechnersystem · Canyon v2 review</title><style>
*{box-sizing:border-box}body{margin:0;background:#101820;color:#dce4e9;font:16px/1.5 system-ui,sans-serif}main{max-width:1520px;margin:auto;padding:36px}header{border-top:1px solid #526474;padding-top:22px}h1{font-size:clamp(32px,5vw,62px);font-weight:520;margin:0}header p{max-width:850px;color:#acbbc5}small{color:#c6a87b;letter-spacing:.13em}nav{display:flex;gap:8px;flex-wrap:wrap;margin:26px 0}button{font:inherit;color:inherit;background:transparent;border:1px solid #667987;border-radius:4px;padding:8px 16px;cursor:pointer}button[aria-pressed=true]{background:#dce4e9;color:#101820}a{color:#dfc696}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:24px}figure{margin:0;background:#1b2832;border:1px solid #3b4f5d;border-radius:5px;overflow:hidden}figure[hidden]{display:none}img,video{display:block;width:100%;aspect-ratio:16/9;object-fit:contain;background:#080e14}figcaption{padding:16px 20px}figcaption p{font-size:14px;color:#acbbc5;margin:4px 0}audio{width:100%;margin-top:12px}footer{border-top:1px solid #526474;margin-top:32px;padding-top:20px;color:#acbbc5}footer p{max-width:1000px}button:focus-visible,a:focus-visible{outline:3px solid #dbbb83;outline-offset:3px}@media(max-width:760px){main{padding:22px 14px}.grid{grid-template-columns:1fr}}
</style><main><header><small>ACTUAL GAME RENDERS · 30 SEPTEMBER 2026</small><h1>Canyon v2</h1><p>Winter roads, connected ruined plots and bare tree lines across the 4.93 km Gaea course. Click any image to inspect the full render.</p></header>
<nav aria-label="Review categories"><button data-filter="all" aria-pressed="true">All</button><button data-filter="player" aria-pressed="false">Ground &amp; player views</button><button data-filter="aerial" aria-pressed="false">Aerials</button><button data-filter="map" aria-pressed="false">Paper map</button><button data-filter="before" aria-pressed="false">Before reference pass</button><button data-filter="mechanics" aria-pressed="false">Mechanic checks</button><button data-filter="diagnostics" aria-pressed="false">Open visual issues</button></nav><div class="grid">CARDS</div>
<footer><p>Map stills use the game renderer at 1280 × 720, enlarged to 3840 × 2160. Cupola and ground viewpoints are staged; aerials use less physical fog for terrain inspection. Mechanic clips use 640 × 360 internal resolution, encoded at 1920 × 1080. No AI-generated renders or external reference footage are included.</p><p>The original Canyon v1 remains available. Adaptive enemy placement is seeded independently of this authored geography; a 1–2 hour run remains a pacing target, not a completed playtest claim. The release build and 1,422 host tests pass; ignored tests, subjective audio review and long-run playability remain outside that result.</p><p>Open work includes the diagnostic lighting/effect issues above, hot penetration rims, continued ricochet flight and reproduction of the reported protruding round.</p><p>Visual direction: <a href="https://www.youtube.com/watch?v=yrrso5JDR5I">user-supplied BFBS footage</a>. Winter surface and spatial reference only; WWII vehicles, architecture and damage remain separate. The supplied Reddit angle could not be played through its human-verification page.</p></footer></main>
<script>document.querySelectorAll('[data-filter]').forEach(b=>b.onclick=()=>{document.querySelectorAll('[data-filter]').forEach(x=>x.setAttribute('aria-pressed',String(x===b)));document.querySelectorAll('figure').forEach(f=>f.hidden=b.dataset.filter!=='all'&&f.dataset.kind!==b.dataset.filter)})</script></html>'''
path=OUT/'canyon_v2_review_20260930.html'
path.write_text(html.replace('CARDS','\n'.join(cards+before+clips)))
print(path)
