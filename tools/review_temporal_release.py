#!/usr/bin/env python3
"""Validate and index the local native film set, preserving every original export."""
import argparse,html,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/'target/temporal-impact-20261104'
OUT=Path('/windows/shop/panzerrechnersystem/captures/videos')
STILLS=OUT.parent/'screenshots'
def esc(x):return html.escape(str(x),quote=True)
def verify(path,expected=None):
 p=Path(path);assert p.exists(),p
 d=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(p)]))
 v=next(s for s in d['streams'] if s['codec_type']=='video')
 if expected is not None:assert int(v['nb_frames'])==expected,(p,v['nb_frames'],expected)
 subprocess.run(['ffmpeg','-v','error','-i',str(p),'-f','null','-'],check=True)
 return dict(path=str(p),bytes=p.stat().st_size,video=v,audio=[s for s in d['streams'] if s['codec_type']=='audio'],duration=float(d['format']['duration']),full_decode=True)
def main():
 slow=json.loads((RUN/'cinematic-release/finished-report.json').read_text());real=json.loads((RUN/'realtime-release/report.json').read_text());assert slow['complete'] and real['complete']
 byname={c['name'].replace('realtime-release','cinematic-release'):c for c in real['clips']}
 receipt=[];cards=[]
 titles={'heat-front-section':'HEAT · front plate cutaway','hesh-front-section':'HESH · front plate cutaway','apfsds-front-section':'APFSDS · front plate cutaway','heat-mantlet-interior':'HEAT · through the mantlet','heat-bow-interior':'HEAT · driver and radio operator','apcr-mantlet-crew-section':'APCR · mantlet, gunner and commander'}
 for index,c in enumerate(slow['clips']):
  r=byname[c['name']];s=Path(c['cinematic_audio_video']);rt=Path(r['video']);receipt.extend([verify(s,c['frames']),verify(rt,r['frames'])])
  poster=STILLS/(c['name']+'-poster.jpg')
  subprocess.run(['ffmpeg','-v','error','-y','-ss','1.0','-i',str(s),'-frames:v','1',str(poster)],check=True)
  key=c['name'].split('20261104-')[-1];title=titles[key]
  cards.append(f'''<article><h2>{esc(title)}</h2><div class="switch"><button onclick="swap({index},'{esc(s.name)}','Slow inspection')">Slow inspection</button><button onclick="swap({index},'{esc(rt.name)}','Real time')">Real time</button><span id="label{index}">Slow inspection</span></div><video id="clip{index}" controls preload="metadata" poster="../screenshots/{esc(poster.name)}" src="{esc(s.name)}"></video><p><a href="{esc(s.name)}">Download slow version</a> · <a href="{esc(rt.name)}">Download real-time version</a></p></article>''')
 ai=[]
 for name,label in [('voxel-runtime-ai-interior-drive-fire-final-20261104','Driving and firing'),('voxel-runtime-ai-interior-hit-final-20261104','Taking hits')]:
  path=OUT/(name+'.mp4');meta=json.loads((OUT/(name+'_receipt.json')).read_text());receipt.append(verify(path));o=meta['outcome']
  ai.append(f'<article><h2>{label}</h2><video controls preload="metadata" src="{esc(path.name)}"></video><p>{o["distance_driven_m"]:.1f} m driven · {o["tiger_main_gun_shots"]} Tiger shots · {o["hits_on_tiger"]} hits received. Separate native AI runs.</p></article>')
 battle=OUT/'voxel-runtime-army-timelapse-night-balanced-20261104.mp4';meta=json.loads(battle.with_suffix('.json').read_text());receipt.append(verify(battle,2700));counts=meta['counts']
 assert meta['complete'] and not meta['scan'] and meta['native_grid']==[1920,1080]
 for kind,count in [('tank_knockout',counts['tank_wrecks']),('infantry_down',counts['infantry_down'])]:
  events=[e for e in meta['events'] if e['kind']==kind];ids=[e['id'] for e in events]
  assert len(ids)==len(set(ids))==count,(kind,len(ids),count)
  for e in events:
   arrival=((e['id']%48)//12)*180 if kind=='tank_knockout' else ((e['id']%768)//192)*180
   assert e['time']>=arrival,(kind,e)
 content=f'''<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Panzer · field and interior films</title><style>body{{margin:0;background:#111;color:#eee;font:16px system-ui}}main{{max-width:1800px;margin:auto;padding:28px}}h1{{font-size:34px}}h2{{font-size:22px;margin:0 0 14px}}p{{color:#bbb;line-height:1.5}}a{{color:#b7d9ff}}video{{display:block;width:100%;background:#050505;image-rendering:pixelated}}.grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,560px),1fr));gap:24px}}article{{background:#202020;padding:18px;margin:24px 0}}button{{padding:10px 16px;border:1px solid #777;background:#111;color:white;cursor:pointer}}.switch{{display:flex;gap:8px;align-items:center;margin-bottom:12px}}.switch span{{margin-left:auto;color:#aaa}}summary{{cursor:pointer}}.hero{{padding:20px}}</style></head><body><main><h1>Field and interior films</h1><p><a href="../screenshots/voxel-runtime-daylight-openfield-20261104-gallery.html">Daylight color studies</a> · <a href="voxel-runtime-color-20261104-gallery.html">Saturation copies of the earlier films</a></p><article class="hero"><h2>Open-field battle · three day/night cycles</h2><p>96 tanks and 1,536 infantry enter in four opposing waves. A slow 35 m dolly follows the field through 90 seconds.</p><video controls preload="metadata" poster="../screenshots/voxel-runtime-army-timelapse-night-balanced-20261104-opening.png" src="{battle.name}"></video><p>{counts['tank_wrecks']} tank wrecks · {counts['infantry_down']:,} downed infantry. Persistent battlefield remains; silent timelapse.</p><p><a href="{battle.name}">Download battle timelapse</a></p></article><h1>Impact studies</h1><p>Real-time opening, steady slow inspection, then a longer real-time aftermath. The slow versions use deeper sustained impacts and native contact sounds.</p><div class="grid">{''.join(cards)}</div><h1>Inside the Tiger · native AI combat</h1><div class="grid">{''.join(ai)}</div><details><summary>Simulation notes</summary><p>The HEAT, HESH and APFSDS profiles are experimental game approximations. The section shots advance material states and swept contacts sequentially; they are not validated wound or armour experiments. Real-time companions are separate seeded replays of the same setup. The army timelapse uses the native movement, infantry combat and ballistic tables with coarser tank hit boxes and penetration knockouts. Its astronomical clock is accelerated independently of combat. No casualty quota is injected.</p><p><a href="{battle.with_suffix('.json').name}">Battle receipt</a> · <a href="voxel-runtime-film-release-20261104-validation.json">Media validation</a></p></details></main><script>function swap(i,src,label){{const v=document.getElementById('clip'+i);v.pause();v.src=src;document.getElementById('label'+i).textContent=label;v.load()}}</script></body></html>'''
 dest=OUT/'voxel-runtime-film-release-20261104-gallery.html';dest.write_text(content)
 (OUT/'voxel-runtime-film-release-20261104-validation.json').write_text(json.dumps(dict(complete=True,files=receipt,battle=counts),indent=2));print(dest)
if __name__=='__main__':main()
