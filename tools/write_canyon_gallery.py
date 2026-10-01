#!/usr/bin/env python3
"""Package the real render stills and source-derived paper drawing in a local review gallery."""
from pathlib import Path
import json
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'captures'
shots=json.loads((OUT/'canyon_gallery_20260930.json').read_text())['shots']
items=[]
names={
 'player_approach':('First approach','Cupola view approaching the first defended bend.'),
 'player_settlement':('The saddle approach','Cupola view along the northern arm of the canyon.'),
 'player_factory':('Factory corridor','The broad middle valley and its ruined buildings.'),
 'player_breakout':('Final settlement','Close buildings and armour on the eastern approach.'),
 'aerial_course_plan':('The whole course','A north-up aerial of the winding canyon. Reduced fog for inspection.'),
 'aerial_settlement_detail':('Northern settlement','An oblique view across the upper valley. Reduced fog for inspection.'),
 'aerial_factory_detail':('Factory from above','Spacing, buildings and lines of approach. Reduced fog for inspection.'),
 'held_paper':('The map in your hands','The drawing on the actual folded paper, with its physical compass.'),
}
for shot in shots:
 name=shot['name']
 if name not in names:continue
 title,caption=names[name]
 items.append(dict(src=Path(shot['output']).name,title=title,caption=caption,
                  kind='aerial' if name.startswith('aerial') else 'map' if name=='held_paper' else 'player'))
items.append(dict(src='../catalog/maps/canyon_survey_20260930.png',title='Canyon field drawing',
 caption='20 m contours, real buildings and trees, north arrow, 500 m scale, and the planned route. No enemy locations.',kind='map'))
html='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Canyon Run · terrain and field map</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#151c1d;color:#e3e5df;font:16px/1.5 system-ui,sans-serif}main{max-width:1440px;margin:auto;padding:44px 36px}header{border-top:1px solid #637271;padding-top:20px;display:flex;gap:36px;justify-content:space-between;align-items:end}small{letter-spacing:.16em;text-transform:uppercase;color:#a7b8b2;font-size:11px}h1{font-size:clamp(36px,5vw,64px);font-weight:480;line-height:1.05;margin:12px 0}header p{max-width:420px;color:#aeb8b5}nav{display:flex;flex-wrap:wrap;gap:8px;margin:30px 0 24px}button,.link{background:transparent;border:1px solid #667570;color:#d8e0d9;border-radius:4px;padding:9px 17px;font:inherit;cursor:pointer;text-decoration:none}button:hover,.link:hover,button[aria-pressed=true]{background:#d6c7a6;color:#192323;border-color:#d6c7a6}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:28px 24px}figure{margin:0;background:#202a2b;border:1px solid #3a4746;border-radius:6px;overflow:hidden}figure[hidden]{display:none}figure button{display:block;padding:0;border:0;border-radius:0;width:100%;background:#101716}figure img{display:block;width:100%;aspect-ratio:16/9;object-fit:contain}figcaption{padding:17px 20px 20px}h2{font-size:19px;font-weight:550;margin:4px 0}figcaption p{font-size:14px;color:#aeb8b5;margin:4px 0 0}footer{border-top:1px solid #485552;padding:24px 0;margin-top:35px;color:#aeb8b5;font-size:14px}footer p{max-width:960px}dialog{width:min(96vw,1700px);max-height:96vh;border:1px solid #647570;border-radius:7px;background:#111819;color:#dce2dc;padding:12px}dialog::backdrop{background:#000d}dialog img{display:block;width:100%;max-height:76vh;object-fit:contain}.bar{display:flex;gap:10px;align-items:center;padding:14px 4px 0}.bar p{margin:0;flex:1}.bar a{color:#dfc994}button:focus-visible,a:focus-visible{outline:3px solid #d5ba72;outline-offset:4px}@media(max-width:740px){main{padding:26px 16px}header{display:block}.grid{grid-template-columns:1fr}.bar{flex-wrap:wrap}}
</style><main><header><div><small>Panzerrechnersystem · Gaea canyon</small><h1>Canyon Run</h1><span>Terrain views &amp; the field drawing</span></div><p>A 4.93 km route through the snow: sheltered approach, ruined settlement, saddle, factory and eastern breakout.</p></header>
<nav aria-label="View categories"><button aria-pressed="true" data-filter="all">All views</button><button aria-pressed="false" data-filter="player">Player views</button><button aria-pressed="false" data-filter="aerial">Aerials</button><button aria-pressed="false" data-filter="map">Paper map</button><a class="link" href="../catalog/maps/canyon_survey_20260930.svg" download>Download editable drawing</a></nav><div class="grid" id="grid"></div>
<footer><p>Real game renders, captured at 640 × 360 and enlarged to 1920 × 1080. Player viewpoints are staged in the actual cupola with normal winter weather. Aerials are inspection cameras with reduced physical fog density. The drawing comes from the terrain and placed objects, and is integrated into the held paper map.</p><p>The adaptive defence can admit new T-34 platoons throughout a run, with up to 12 living tanks. Death ends the Canyon attempt. The 1–2 hour duration remains a pacing target awaiting full armed playtests.</p></footer></main>
<dialog id="viewer"><img id="large" alt=""><div class="bar"><button id="prev" aria-label="Previous image">←</button><button id="next" aria-label="Next image">→</button><p id="description"></p><a id="original" target="_blank">Open original</a><button id="close">Close</button></div></dialog>
<script>const items=ITEMS;let current=0;const grid=document.querySelector('#grid'),viewer=document.querySelector('#viewer');
items.forEach((item,i)=>{const figure=document.createElement('figure');figure.dataset.kind=item.kind;const button=document.createElement('button');button.setAttribute('aria-label','Enlarge '+item.title);const image=document.createElement('img');image.src=item.src;image.alt=item.title;image.loading='lazy';button.append(image);button.onclick=()=>open(i);const caption=document.createElement('figcaption');const label=document.createElement('small');label.textContent=item.kind==='player'?'Player view':item.kind==='aerial'?'Aerial inspection':'Paper map';const title=document.createElement('h2');title.textContent=item.title;const p=document.createElement('p');p.textContent=item.caption;caption.append(label,title,p);figure.append(button,caption);grid.append(figure)});
function show(i){current=(i+items.length)%items.length;const item=items[current];large.src=item.src;large.alt=item.title;description.textContent=item.title+' — '+item.caption;original.href=item.src}
function open(i){show(i);viewer.showModal()}
document.querySelectorAll('[data-filter]').forEach(button=>button.onclick=()=>{document.querySelectorAll('[data-filter]').forEach(b=>b.setAttribute('aria-pressed',String(b===button)));document.querySelectorAll('figure').forEach(f=>f.hidden=button.dataset.filter!=='all'&&f.dataset.kind!==button.dataset.filter)});
document.querySelector('#close').onclick=()=>viewer.close();prev.onclick=()=>show(current-1);next.onclick=()=>show(current+1);viewer.addEventListener('keydown',e=>{if(e.key==='ArrowLeft')show(current-1);if(e.key==='ArrowRight')show(current+1)});
</script></html>'''
html=html.replace('ITEMS',json.dumps(items).replace('<','\\u003c'))
path=OUT/'canyon_gallery_20260930.html';path.write_text(html)
for item in items:assert (OUT/item['src']).is_file(),item['src']
print(path, len(items),'views')
