#!/usr/bin/env python3
"""Assemble verified current-game clips, retaining their original game audio and provenance."""
import hashlib,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures/astra'; OUT=ROOT/'artifacts/showcase-20260925'; OUT.mkdir(exist_ok=True)
FONT='/usr/share/fonts/noto/NotoSans-Regular.ttf'
if not Path(FONT).exists():
    FONT=subprocess.check_output(['fc-match','-f','%{file}','sans'],text=True)
chapters=[
 ('wreck_v1', 'WINTER AFTERMATH', 'Ruined courtyard, burned armour and sparse winter ground cover'),
 ('household_v2','A HOUSEHOLD REFUGE','September 1942 testimony / imagined winter remains / layout inferred'),
 ('ward_v2','THE FACTORY CELLARS','December 1942 diary / separate ward and doctor cellars / layout inferred'),
 ('gumrak_v1','GUMRAK / BURNED STORES','Late November 1942 diary / cold aftermath and local arrangement inferred'),
 ('reload_v1','PHYSICAL RELOAD','Hold, flick to ram, close the breech, then fire'),
 ('pistol_v1','EXPOSED COMMANDER','RMB draws the P08 / its trigger stays separate from the main gun'),
 ('map_v1','THE PAPER MAP','Zoom, draw and place a pin without losing your markings'),
 ('fire_v1','INSIDE THE FURNACE','Staged ignition / orange source light, smoke and the ordinary game sound'),
]
def digest(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def run(args):
    with (OUT/'encode.log').open('a') as log:
        subprocess.run(['ffmpeg','-hide_banner','-loglevel','warning','-nostdin','-y',*args],check=True,stdout=log,stderr=log)
def probe(p):return json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-show_format','-of','json',str(p)],text=True))
segments=[]; receipts=[]; at=0
# A short, self-contained opening. The wind is sourced from the first gameplay clip.
intro=CAP/'showcase_20260925_edit_intro.mp4'
textfile=OUT/'intro.txt';textfile.write_text('PANZERRECHNERSYSTEM\nWINTER DEVELOPMENT SHOWCASE\n25 September 2026\n\nCurrent engine footage\nScripted player inputs / original game audio')
run(['-f','lavfi','-i','color=c=0x11171b:s=1920x1080:r=24:d=4','-i',str(CAP/'showcase_20260925_wreck_v1.mp4'),'-filter_complex',f"[0:v]drawbox=x=180:y=220:w=6:h=580:color=0xbd8857:t=fill,drawtext=fontfile='{FONT}':textfile='{textfile}':fontsize=44:fontcolor=0xe0e1de:x=240:y=255:line_spacing=23[v];[1:a]atrim=0:4,asetpts=PTS-STARTPTS,afade=t=in:d=0.4,afade=t=out:st=3.7:d=0.3[a]",'-map','[v]','-map','[a]','-t','4','-c:v','libx264','-preset','fast','-crf','18','-pix_fmt','yuv420p','-c:a','aac','-b:a','192k',str(intro)])
segments.append(intro);at=4
for slug,title,subtitle in chapters:
    source=CAP/f'showcase_20260925_{slug}.mp4'; assert source.is_file(),source
    p=probe(source);duration=float(p['format']['duration']); assert any(x['codec_type']=='audio' for x in p['streams'])
    texts=[]
    for name,value in [('title',title),('subtitle',subtitle)]:
        path=OUT/f'{slug}_{name}.txt';path.write_text(value);texts.append(path)
    target=CAP/f'showcase_20260925_edit_{slug}.mp4'
    title_until=3 if slug=='fire_v1' else 4
    vf=f"drawbox=x=0:y=884:w=iw:h=196:color=black@0.72:t=fill:enable='lt(t,{title_until})',drawtext=fontfile='{FONT}':textfile='{texts[0]}':fontsize=34:fontcolor=0xe0e1de:x=64:y=917:enable='lt(t,{title_until})',drawtext=fontfile='{FONT}':textfile='{texts[1]}':fontsize=25:fontcolor=0xc0c8c8:x=64:y=971:enable='lt(t,{title_until})'"
    run(['-i',str(source),'-vf',vf,'-af',f'afade=t=in:d=0.08,afade=t=out:st={max(0,duration-.12)}:d=0.12','-c:v','libx264','-preset','fast','-crf','18','-pix_fmt','yuv420p','-c:a','aac','-b:a','192k',str(target)])
    receipt=CAP/f'showcase_20260925_{slug}_manifest.json'
    receipts.append({'chapter':title,'start_s':at,'duration_s':duration,'source':str(source.relative_to(ROOT)),'sha256':digest(source),'capture_receipt':str(receipt.relative_to(ROOT)) if receipt.exists() else None,'note':subtitle})
    at+=duration;segments.append(target)
listing=OUT/'concat.txt';listing.write_text(''.join(f"file '{p}'\n" for p in segments))
meta=OUT/'chapters.ffmeta'
meta.write_text(';FFMETADATA1\ntitle=Panzerrechnersystem - Winter development showcase\ncomment=Current engine; staged starting conditions and scripted inputs; ordinary game audio. Historical layouts are inferred.\n'+''.join(f"[CHAPTER]\nTIMEBASE=1/1000\nSTART={round(x['start_s']*1000)}\nEND={round((x['start_s']+x['duration_s'])*1000)}\ntitle={x['chapter']}\n" for x in receipts))
final=CAP/'panzer_winter_showcase_20260925.mp4'
run(['-f','concat','-safe','0','-i',str(listing),'-i',str(meta),'-map_metadata','1','-map_chapters','1','-c:v','copy','-af','aresample=async=1:first_pts=0','-c:a','aac','-b:a','192k','-movflags','+faststart',str(final)])
report={'schema':'panzer.showcase-edit.v1','output':str(final),'sha256':digest(final),'probe':probe(final),'assembly_time_binary_sha256':digest(ROOT/'target/release/panzerrechnersystem'),'chapters':receipts,'evidence_boundary':'Current Game frames and original game audio. Inputs and start positions scripted; source clips disclose staging. No archival footage, score, exact historical floor-plan claim or performance benchmark.'}
(OUT/'showcase.receipt.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'output':str(final),'duration_s':report['probe']['format']['duration'],'bytes':final.stat().st_size}))
