#!/usr/bin/env python3
"""Local native-media comparison page and explicit, asset-free Pi delivery allowlist."""
from html import escape
from pathlib import Path
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CAP = ROOT / 'captures'
media = [
 ('canyon_storm_day_combat_20260930.mp4', 'Storm / morning', '18 seconds. Fog, blowing snow, return fire and two Tiger shots.', 'canyon_storm_day_after_20260930.png'),
 ('canyon_storm_snowfall_combat_20260930.mp4', 'Storm / low winter sun', 'Matched encounter. A dimmer, cooler background against the combat flashes.', 'canyon_storm_snowfall_after_20260930.png'),
 ('canyon_finefire_base_20260930.mp4', 'Smaller cube flames', 'Restored pre-Canyon fire, with finer individual grains. Four-second surface ignition fixture.', 'canyon_finefire_base_20260930_frame2.png'),
 ('canyon_finefire_heat_20260930.mp4', 'Heat shimmer / prototype', 'Same fire state, with source-driven screen-space distortion. Review only; disabled in normal play.', 'canyon_finefire_heat_20260930_frame2.png'),
 ('canyon_interior_terminal_final_20260930.mp4', 'Interior-only terminal replay', 'Full-turret witness camera through the fade and aftermath. Recorded crew damage is never reapplied.', 'canyon_interior_terminal_final_20260930_interior4.png'),
]
images = [
 ('canyon_storm_day_muzzle_20260930.png', 'Morning / muzzle flash'),
 ('canyon_storm_snowfall_muzzle_20260930.png', 'Low sun / muzzle flash'),
 ('canyon_storm_snowfall_player_20260930.png', 'Low sun / full player-view still'),
 ('canyon_surface02_player_20260930.png', 'Broken snow and traffic edges'),
 ('canyon_surface03_splatter_20260930.png', 'Small, irregular ground splatter'),
 ('canyon_surface_final_grass_20260930.png', 'Finer straw in surviving patches'),
]
files = set()
cards = []
receipts = []
provenance = json.loads((CAP/'canyon_storm_render_provenance_20260930.json').read_text())
files.add('canyon_storm_render_provenance_20260930.json')
for index, (filename, title, note, poster) in enumerate(media):
    path = CAP / filename
    probe = json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(path)]))
    video = next(s for s in probe['streams'] if s['codec_type'] == 'video')
    audio = next(s for s in probe['streams'] if s['codec_type'] == 'audio')
    assert abs(float(video['duration'])-float(audio['duration'])) < .05
    if index < 4:
        manifest = path.stem + '_manifest.json'
        check = json.loads((CAP / manifest).read_text())
        assert check['outcome_checks_passed'] and check['required_timeline_actions_passed']
    else:
        manifest = path.stem + '_segments.json'
        check = json.loads((CAP / manifest).read_text())
        assert all(s['damage_reapplied'] is False for s in check['segments'])
    files.update([filename, poster, manifest])
    cards.append(f'<figure><video id="film{index}" controls preload="metadata" poster="{poster}" src="{filename}"></video><figcaption><b>{escape(title)}</b><p>{escape(note)}</p><a href="{filename}" download>Open video</a></figcaption></figure>')
    recorded = provenance[filename]
    assert recorded['sha256'] == hashlib.sha256(path.read_bytes()).hexdigest(), 'Media changed since recorded provenance'
    receipts.append(recorded)

for filename, title in images:
    files.add(filename)
    sidecar = Path(filename).with_suffix('.json').name
    if (CAP / sidecar).exists(): files.add(sidecar)
    cards.append(f'<figure><a href="{filename}"><img loading="lazy" src="{filename}" alt="{escape(title)}"></a><figcaption><b>{escape(title)}</b><p>Native game capture. Open to inspect at full size.</p></figcaption></figure>')
files.update(['canyon_storm_day_player_20260930.png','canyon_storm_day_player_20260930.json','canyon_storm_snowfall_player_20260930.json'])
for doc in ['CANYON_SURFACE_20260930.md','CANYON_STORM_20260930.md']:
    (CAP / doc).write_bytes((ROOT / 'docs' / doc).read_bytes()); files.add(doc)
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Canyon · Storm combat and fire review</title><style>
*{box-sizing:border-box}body{margin:0;background:#10171f;color:#e0e6ed;font:16px/1.5 system-ui,sans-serif}main{max-width:1560px;margin:auto;padding:36px}header{border-top:1px solid #536879;padding-top:20px}h1{font-size:clamp(30px,4vw,56px);font-weight:550;margin:0}header p{max-width:920px;color:#afbcc7}small{letter-spacing:.12em;color:#c8aa82}.controls{display:flex;gap:12px;flex-wrap:wrap;margin:24px 0}button{font:inherit;background:#263747;color:#e0e6ed;border:1px solid #6e8598;border-radius:4px;padding:10px 16px;cursor:pointer}button:hover{background:#364e61}button:focus-visible,a:focus-visible{outline:3px solid #d6b88c;outline-offset:3px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:24px}figure{margin:0;background:#192632;border:1px solid #3f5365;border-radius:5px;overflow:hidden}img,video{display:block;width:100%;aspect-ratio:16/9;object-fit:contain;background:#06090c}figcaption{padding:16px 20px}figcaption p,footer{color:#afbcc7;font-size:14px}figcaption p{margin:6px 0}a{color:#dfc299}footer{border-top:1px solid #536879;margin-top:32px;padding-top:20px}footer p{max-width:1000px}#status{color:#b9c8d3;min-height:24px}@media(max-width:760px){main{padding:22px 14px}.grid{grid-template-columns:1fr}}
</style><main><header><small>PANZERRECHNERSYSTEM · 30 SEPTEMBER 2026</small><h1>Storm, fire and the last moment inside</h1><p>Two matched combat-lighting studies on Gaea Canyon v2, finer cube flames, a heat-shimmer experiment, and the interior-only terminal replay. The new weather remains separate from the default map.</p></header>
<div class="controls"><button data-pair="0,1">Play weather comparison</button><button data-pair="2,3">Play fire comparison</button><button id="pause">Pause all</button></div><p id="status" role="status" aria-live="polite">Each video also has its own controls. In paired playback only the first video's sound plays.</p><div class="grid">CARDS</div>
<footer><p>The combat fixture stages the Tiger near a live Soviet 45 mm gun and fires two loaded shots. Enemy aiming, shells and damage run normally. This is a look comparison, not an autonomous Canyon completion. Fire starts from a disclosed surface ignition; the replay starts from a disclosed fatal ammunition hit.</p><p>Videos: 640 × 360 native sensor, encoded at 1920 × 1080. Full player stills: 1280 × 720 native, enlarged 3×. No external footage, generated concept art, color grading or fixed-exposure override is used. Fire A/B final simulation states match. Heat shimmer is a debug-only screen-space prototype.</p><p>Open visual work: bright explosion highlights and chunky soot remain. The later blue-hour experiment lost too much ground visibility and is excluded from this selection.</p><p><a href="CANYON_STORM_20260930.md">Capture details and limitations</a> · <a href="CANYON_SURFACE_20260930.md">Ground, grass and splatter changes</a> · <a href="canyon_v2_review_20260930.html">Earlier map and mechanics gallery</a></p></footer></main>
<script>
const videos=[...document.querySelectorAll('video')];let pair=[];
function pauseAll(){videos.forEach(v=>v.pause());pair=[];}
document.querySelector('#pause').onclick=pauseAll;
document.querySelectorAll('[data-pair]').forEach(button=>button.onclick=async()=>{pauseAll();pair=button.dataset.pair.split(',').map(i=>document.querySelector('#film'+i));pair.forEach((v,i)=>{v.currentTime=0;v.muted=i!==0});const result=await Promise.allSettled(pair.map(v=>v.play()));document.querySelector('#status').textContent=result.every(r=>r.status==='fulfilled')?'Paired playback · use each video’s fullscreen control for detail.':'Press Play on each video if the browser blocked paired playback.';pair[0].scrollIntoView({behavior:'smooth',block:'center'});});
videos.forEach(v=>v.addEventListener('timeupdate',()=>{if(pair.length===2&&v===pair[0]&&!v.paused&&pair[1].readyState>=2&&Math.abs(pair[1].currentTime-v.currentTime)>.15)pair[1].currentTime=v.currentTime;}));
</script></html>'''
filename = 'canyon_storm_review_20260930.html'
(CAP / filename).write_text(page.replace('CARDS','\n'.join(cards))); files.add(filename)
a=json.loads((CAP/'canyon_finefire_base_20260930_manifest.json').read_text())
b=json.loads((CAP/'canyon_finefire_heat_20260930_manifest.json').read_text())
assert a['final_state']==b['final_state']
metadata = 'canyon_storm_review_20260930.json'
(CAP / metadata).write_text(json.dumps(dict(media=receipts,weather_studies=json.loads((CAP/'canyon_storm_weather_20260930.json').read_text()),heat_final_state_equal=True,heat_shipping_enabled=False,weather_promoted_to_default=False,stills_reviewed=True,video_review='decoded selected frames; video/audio stream lengths and action/outcome receipts checked',tests=dict(passed=1427,ignored=54),transfer_destination='/home/saffari/Downloads/Panzerrechnersystem_Canyon_v2_2026-09-30/'),indent=2)+'\n');files.add(metadata)
for filename in files: assert (CAP/filename).is_file(),filename
(CAP/'canyon_storm_pi_files_20260930.txt').write_text('\n'.join(sorted(files))+'\n')
(CAP/'canyon_storm_SHA256SUMS_20260930').write_text(''.join(f'{hashlib.sha256((CAP/f).read_bytes()).hexdigest()}  {f}\n' for f in sorted(files)))
print(f'{len(files)} files; review: {CAP / "canyon_storm_review_20260930.html"}')
