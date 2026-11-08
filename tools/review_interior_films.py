#!/usr/bin/env python3
"""Verify the offline camera films and build a local gallery from their native receipt."""
from pathlib import Path
import argparse, json, subprocess, html
from PIL import Image, ImageDraw, ImageFont
p=argparse.ArgumentParser();p.add_argument('run',type=Path);p.add_argument('--partial',action='store_true');a=p.parse_args()
r=json.loads((a.run/'report.json').read_text());assert r['complete'] or a.partial
validation=a.run/'video-validation.json'
cached={c['video']:c for c in json.loads(validation.read_text()).get('clips',[])} if validation.exists() else {}
checks=[];cards=[];poster_images=[]
font=ImageFont.truetype('/usr/share/fonts/TTF/DejaVuSans.ttf',18)
for c in r['clips']:
 f=Path(c['video']);stat=f.stat();check=cached.get(str(f))
 if not check or check.get('bytes')!=stat.st_size or check.get('mtime_ns')!=stat.st_mtime_ns:
  meta=json.loads(subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0','-count_frames','-show_entries','stream=width,height,r_frame_rate,nb_read_frames,pix_fmt:format=duration','-of','json',str(f)]))
  v=meta['streams'][0]
  assert int(v['nb_read_frames'])==c['frames'],(f,v,c['frames'])
  assert v['width']==1920 and v['height']==1080 and v['r_frame_rate']=='60/1'
  subprocess.run(['ffmpeg','-v','error','-i',str(f),'-map','0:v:0','-f','null','-'],check=True,stdout=subprocess.DEVNULL)
  check={'video':str(f),'bytes':stat.st_size,'mtime_ns':stat.st_mtime_ns,'frames':v['nb_read_frames'],'pixels':[v['width'],v['height']],'fps':v['r_frame_rate'],'decode':'pass'}
 poster_time=5.0
 if c.get('plate_deformation',{}).get('recorded'):
  target=c['plate_deformation']['birth_simulation_s']+0.7*c['plate_deformation']['solve_seconds'];elapsed=0.0
  for seg in c['segments']:
   span=seg['frames']*seg['step_seconds']*2
   if target<=elapsed+span:
    poster_time=seg['start_frame']/r['fps']+(target-elapsed)*seg['speed_denominator'];break
   elapsed+=span
 poster=Path(c['preview']).with_name(c['name']+'-review.jpg')
 if not poster.exists() or poster.stat().st_mtime_ns<stat.st_mtime_ns:
  subprocess.run(['ffmpeg','-v','error','-y','-ss',str(poster_time),'-i',str(f),'-frames:v','1','-update','1',str(poster)],check=True)
 poster_images.append((c,poster));checks.append(check)
 title=f"{c['shell']} · camera {c['camera_index']:02d}"
 timing=' → '.join(f"{segment['speed_denominator']}× ({segment['frames']/r['fps']:.2f} s)" for segment in c['segments'])
 note=' Cutaway uses steady lighting without volumetric glow to reveal the plate deformation.' if c.get('plate_deformation') else ''
 cards.append(f'''<article><h2>{html.escape(title)}</h2><p>{html.escape(c['camera']['name'])}</p><video controls playsinline preload="none" poster="../screenshots/{poster.name}" src="{f.name}"></video><p>{c['range_m']:g} m · {timing} · {c['video_seconds']:.2f} s{note} · <a href="{f.name}" download>Download MP4</a></p></article>''')
columns=2;rows=(len(poster_images)+1)//2
sheet=Image.new('RGB',(1280,rows*390),'#141414');draw=ImageDraw.Draw(sheet)
for i,(c,f) in enumerate(poster_images):
 x=(i%2)*640;y=(i//2)*390
 sheet.paste(Image.open(f).resize((640,360)),(x,y+30));draw.text((x+8,y+5),f"{c['shell']} — CAM {c['camera_index']:02d}",fill='white',font=font)
prefix=r['clips'][0]['name'].split('-76mm')[0]
contact=poster_images[0][1].parent/(prefix+'-contact-sheet.jpg');sheet.save(contact,quality=94)
index=Path(r['clips'][0]['video']).parent/(prefix+'-gallery.html')
index.write_text('''<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Panzer — interior impact films</title><style>body{background:#141414;color:#eee;font:16px system-ui;margin:32px auto;max-width:1500px;padding:0 20px}h1{font-size:28px}main{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,570px),1fr));gap:24px}article{background:#222;padding:18px}h2{margin:0;font-size:21px}p{color:#bbb;line-height:1.5}video{width:100%;background:black}a{color:#fff}</style><h1>Interior front-impact films</h1><p>Interior and shot-plane cutaway views using the existing Tiger, crew, materials and lamps. Native reference simulation: 10 mm anatomy, 6,000-particle armour budget, eight cells across the projectile. Every frame rendered individually at native 960 × 540, enlarged to 1920 × 1080; 60 fps, silent. Interior shots use 128× impact and 16× aftermath. Cutaways include a 4096× span showing recorded intermediate lattice states. The game model is not an independently validated ballistic experiment.</p><main>'''+''.join(cards)+'</main>')
if not r['complete']:
 total=len(json.loads((a.run/'plan.json').read_text())['jobs'])
 index.write_text(index.read_text().replace('<main>',f'<p><strong>{len(checks)} of {total} clips ready. The remaining clips are still rendering.</strong></p><main>'))
validation.write_text(json.dumps({'complete':r['complete'],'clips':checks,'gallery':str(index),'contact_sheet':str(contact)},indent=2))
print(json.dumps({'verified_clips':len(checks),'gallery':str(index),'contact_sheet':str(contact)},indent=2))
