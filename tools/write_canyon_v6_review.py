#!/usr/bin/env python3
"""Self-contained extended-run gallery and explicit Pi media allowlist."""
from html import escape
from pathlib import Path
import hashlib
import json
import subprocess

ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures'
PAGE='canyon_extended_review_20260930.html'
MEDIA=[
 ('Panzerrechnersystem_Canyon_Extended_Highlights_20260930.mp4','Extended highlights',
  'All five engagements and complete manual reloads, with brief chapter labels and the actual result.',
  'canyon_v6_extended137_20260930_cut00_preview.png'),
 ('Panzerrechnersystem_Canyon_First_Person_Uncut_20260930.mp4','103 seconds / uninterrupted first person',
  'Two T-34 engagements with the walk, extraction, loading and ramming sequence. Original speed and game audio.',
  'canyon_v6_run_sight_contact_20260930.png'),
 ('canyon_v6_extended137_20260930.mp4','Clean extended footage',
  'The same chronological excerpts without editorial captions. About six minutes from the 17-minute attempt.',
  'canyon_v6_extended137_20260930_cut04_preview.png'),
 ('canyon_v6_combat_fixture_20260930.mp4','Combat effects / separate staged review',
  'Two staged loaded shots near a live AT gun. Warm muzzle illumination and larger pale propellant smoke.',
  'canyon_v6_combat_muzzle_20260930.png'),
 ('canyon_v6_interior_terminal_20260930.mp4','Interior aftermath / separate staged review',
  'Recorded ammunition penetration, yellow fragment trails and denser surface blood. This is not the AI run’s ending.',
  'canyon_v6_interior_terminal_20260930_interior4.png'),
 ('canyon_interior_terminal_final_20260930.mp4','Previous interior / lighting reference',
  'The earlier dark terminal look retained for comparison; this clip predates the current effect changes.',
  'canyon_interior_terminal_final_20260930_interior4.png'),
]


def main():
    files=set()
    videos=[]
    receipts=[]
    for filename,title,note,poster in MEDIA:
        path=CAP/filename
        probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-of','json',str(path)]))
        v=next(s for s in probe['streams'] if s['codec_type']=='video')
        a=next(s for s in probe['streams'] if s['codec_type']=='audio')
        assert abs(float(v['duration'])-float(a['duration']))<.06,(filename,v['duration'],a['duration'])
        assert (v['width'],v['height'])==(1920,1080)
        files.update([filename,poster])
        seconds=float(v['duration']);duration=f'{int(seconds)//60}:{round(seconds)%60:02}'
        videos.append(f'<figure><video controls preload="metadata" poster="{poster}" src="{filename}"></video><figcaption><div class="line"><b>{escape(title)}</b><span>{duration}</span></div><p>{escape(note)}</p><a href="{filename}" download>Open / download video</a></figcaption></figure>')
        receipts.append(dict(file=filename,seconds=seconds,fps=v['avg_frame_rate'],
            video_codec=v['codec_name'],audio_codec=a['codec_name'],bytes=path.stat().st_size,
            sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    images=[]
    segments=json.loads((CAP/'canyon_v6_extended137_20260930_segments.json').read_text())['segments']
    for pattern in ['canyon_v6_extended137_20260930_cut*_preview.png','canyon_v6_run_*_20260930.png',
                    'canyon_v6_aerial_*_20260930.png','canyon_v6_aftermath_position_20260930.png',
                    'canyon_v6_ground_film_20260930.png','canyon_v6_combat_*_20260930.png',
                    'canyon_v6_interior_terminal_20260930_interior*.png']:
        for path in sorted(CAP.glob(pattern)):
            title=path.stem.removeprefix('canyon_v6_').replace('_20260930','').replace('_',' ')
            if '_cut' in path.stem:
                index=int(path.stem.split('_cut')[1].split('_')[0])
                title=segments[index]['label']
            elif 'interior_terminal' in path.stem:
                title='Interior witness / phase '+path.stem[-1]
            else:
                title=title.removeprefix('run ').replace('combat fixture','Combat fixture / overview').capitalize()
            files.add(path.name)
            images.append(f'<figure><a href="{path.name}"><img loading="lazy" src="{path.name}" alt="{escape(title)}"></a><figcaption>{escape(title)}</figcaption></figure>')
    sidecars=['canyon_v6_extended137_20260930_result.json','canyon_v6_extended137_20260930_segments.json',
        'canyon_v6_extended137_20260930_cuts.json','canyon_v6_extended137_20260930_trace.jsonl',
        'canyon_v6_attempt137_20260930_result.json','canyon_v6_attempt211_20260930_result.json',
        'canyon_v6_extended_edit_20260930.json','canyon_v6_render_source_20260930.json',
        'canyon_v6_stills_20260930.json','canyon_v6_combat_fixture_20260930_manifest.json',
        'canyon_v6_interior_terminal_20260930_segments.json','canyon_v6_interior_terminal_20260930_motion.json',
        'canyon_v6_decode_20260930.json','canyon_v6_README_20260930.txt',
        'canyon_v6_delivery_part1_20260930.sha256','canyon_v6_delivery_part2_20260930.sha256',
        'canyon_v6_delivery_part3_20260930.sha256']
    files.update(sidecars)
    (CAP/'CANYON_EXTENDED_20260930.md').write_bytes((ROOT/'docs/CANYON_EXTENDED_20260930.md').read_bytes())
    files.add('CANYON_EXTENDED_20260930.md')
    receipt='canyon_v6_review_20260930.json'
    (CAP/receipt).write_text(json.dumps(dict(media=receipts,images=len(images),
        editor=json.loads((CAP/'canyon_v6_extended_edit_20260930.json').read_text()),
        review='Native game frames visually sampled; media decoded and stream durations checked. No audio audition or interactive HTML review claimed.',
        pi_directory='/home/saffari/Downloads/Panzerrechnersystem_Canyon_Extended_2026-09-30/'),indent=2)+'\n')
    files.add(receipt)
    page='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Panzerrechnersystem · Extended Canyon Run</title><style>
*{box-sizing:border-box}body{margin:0;background:#10161d;color:#e8e8e1;font:16px/1.55 system-ui,sans-serif}main{max-width:1500px;margin:auto;padding:40px 28px}header{border-top:1px solid #566372;padding-top:24px}small{letter-spacing:.17em;color:#b8b8a9}h1{font-size:clamp(34px,4.4vw,62px);line-height:1.08;font-weight:560;margin:18px 0}h2{font-weight:500;margin:42px 0 18px}p{max-width:950px;color:#b7c1c9}.stats{display:flex;gap:35px;flex-wrap:wrap;border-block:1px solid #3a4651;padding:20px 0;margin:28px 0}.stats b{font-size:30px;font-weight:500;display:block}.stats span{color:#abb8c2;font-size:14px}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:22px}figure{margin:0;background:#18222c;border:1px solid #394a5b;border-radius:5px;overflow:hidden}video,img{display:block;width:100%;aspect-ratio:16/9;object-fit:contain;background:#06090d}figcaption{padding:15px 18px}.line{display:flex;justify-content:space-between;gap:14px}.line span{color:#bbaa8c}figcaption p{font-size:14px;margin:7px 0}a{color:#e3be87}a:focus-visible,summary:focus-visible{outline:3px solid #d8bf90;outline-offset:3px}.stills{grid-template-columns:repeat(3,minmax(0,1fr))}.stills figcaption{font-size:13px;color:#aebbc5}details{border-top:1px solid #465463;margin-top:30px;padding-top:20px}summary{cursor:pointer}footer{margin-top:36px;font-size:14px;color:#abb8c2}@media(max-width:850px){.stills{grid-template-columns:repeat(2,minmax(0,1fr))}}@media(max-width:620px){main{padding:25px 14px}.grid,.stills{grid-template-columns:1fr}.stats{gap:20px}h1{font-size:38px}}
</style><main><header><small>PANZERRECHNERSYSTEM / CANYON V2 / 30 SEPTEMBER 2026</small><h1>One Tiger. Five engagements.</h1><p>A fresh seeded attempt through the Gaea canyon. Cupola travel, aiming, fire and full manual reloads from the player’s position, with native game audio.</p></header>
<div class="stats"><div><b>17:15</b><span>actual attempt duration</span></div><div><b>65%</b><span>maximum route progress</span></div><div><b>3 + 2</b><span>T-34s + AT guns destroyed</span></div><div><b>5</b><span>complete manual reloads</span></div></div>
<div class="grid">VIDEOS</div><p>The Tiger survived. The test pilot stalled in the industrial section and did not clear the course. Eleven T-34s had entered the simulation by the end, with eight still alive. The standalone terminal clip is a separate effect review.</p>
<h2>Stills / player views, map and effects</h2><p>Open any frame at full size. Aerials use reduced fog for layout inspection; ground views retain the heavy storm. All images come from the game.</p><div class="grid stills">STILLS</div>
<details><summary>Capture details and actual run evidence</summary><p>640 × 360 native rendering, enlarged to 1920 × 1080. Highlights retain original speed. The clean extended film contains chronological excerpts; the 103-second encounter has no cuts. Interior fixture stills are 960 × 540.</p><p>The explicit test crew knows the route and uses geometry-assisted sight and aim within 420 m. Damage, finite ammunition, physical loading and adaptive arrivals remain active. This is not a human playtest or a demonstrated full-level completion. The second unfilmed seed also stalled; see the receipts.</p><p><a href="CANYON_EXTENDED_20260930.md">Changes and limitations</a> · <a href="canyon_v6_extended137_20260930_result.json">Actual run result</a> · <a href="canyon_v6_extended137_20260930_segments.json">Cut timing</a> · <a href="canyon_v6_review_20260930.json">Media receipt</a></p></details>
<footer>Owner-private game review. No source asset library or external reference footage is included.</footer></main></html>'''
    (CAP/PAGE).write_text(page.replace('VIDEOS','\n'.join(videos)).replace('STILLS','\n'.join(images)))
    files.add(PAGE)
    for name in files:
        assert (CAP/name).is_file(),name
    (CAP/'canyon_v6_pi_files_20260930.txt').write_text('\n'.join(sorted(files))+'\n')
    (CAP/'canyon_v6_SHA256SUMS_20260930').write_text(''.join(f'{hashlib.sha256((CAP/f).read_bytes()).hexdigest()}  {f}\n' for f in sorted(files)))
    print(json.dumps(dict(files=len(files),stills=len(images),videos=len(MEDIA),page=str(CAP/PAGE))))


if __name__=='__main__':
    main()
