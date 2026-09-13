#!/usr/bin/env python3
"""Join reviewed native gameplay takes, preserving their visible staging labels and audio."""
import json
from pathlib import Path
import subprocess
from make_share_clips import digest, verify

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
CAP = ROOT / 'captures/astra'
# Source frame intervals, 24 fps. Each take retains its own capture caption.
SHOTS = [
    ('Panzer_Cupola_Entry_20260913_v1.mp4', 0, 92, 'Enter through the open cupola and close it'),
    ('Panzer_Crew_Compartment_20260913_v1.mp4', 0, 144, 'Five occupied LAN posts; four other crew idle'),
    ('Panzer_T34_Target_Reload_20260913_v1.mp4', 0, 432, 'Practice target hit and complete manual reload'),
    ('showcase_gunnery_20260913_v1.mp4', 186, 250, 'MMB coaxial MG burst through the main sight'),
    ('showcase_combat_close_20260913_v1.mp4', 0, 168, 'Staged close starting position; live enemy AI maneuvering'),
    ('showcase_impact_76mm_stopped_20260913_v1.mp4', 24, 120, 'Controlled 76 mm terminal contact, no firing vehicle'),
    ('showcase_impact_45mm_stopped_20260913_v1.mp4', 24, 120, 'Controlled 45 mm terminal contact, no firing vehicle'),
    ('Panzer_Fire_Extinguisher_20260913_v1.mp4', 0, 284, 'Staged initial fire, actual extinguisher and suppression'),
]

def main():
    out = CAP / 'Panzer_Showcase_20260913_v3.mp4'
    assert not out.exists(), out
    cmd = ['ffmpeg', '-hide_banner', '-v', 'warning', '-n']
    filters = []
    shots = []
    elapsed = 0
    for i, (filename, start, end, label) in enumerate(SHOTS):
        source = CAP / filename
        assert source.is_file(), source
        cmd += ['-threads', '1', '-i', str(source)]
        duration = (end - start) / 24
        filters += [f'[{i}:v]trim=start_frame={start}:end_frame={end},setpts=PTS-STARTPTS[v{i}]',
                    f'[{i}:a]atrim=start={start/24:.9f}:end={end/24:.9f},asetpts=PTS-STARTPTS,'
                    f'apad=whole_dur={duration:.9f},atrim=duration={duration:.9f},'
                    f'afade=t=in:d=0.02,afade=t=out:st={duration-.02:.9f}:d=0.02[a{i}]']
        shots.append({'source':str(source), 'source_sha256':digest(source), 'source_frames':[start,end],
                      'timeline_seconds':[elapsed/24,(elapsed+end-start)/24], 'description':label})
        elapsed += end-start
    filters.append(''.join(f'[v{i}][a{i}]' for i in range(len(SHOTS))) +
                   f'concat=n={len(SHOTS)}:v=1:a=1[v][a]')
    cmd += ['-filter_complex_threads','1','-filter_complex',';'.join(filters),'-map','[v]','-map','[a]',
            '-c:v','libx264','-preset','medium','-crf','18','-threads','4','-pix_fmt','yuv420p',
            '-r','24','-fps_mode','cfr','-c:a','aac','-b:a','192k','-ar','44100','-ac','2',
            '-movflags','+faststart','-metadata','title=PANZERRECHNERSIMULATOR - Development showcase - 13 September 2026',str(out)]
    receipt = {'output':str(out),'shots':shots,'frames':elapsed,'seconds':elapsed/24,'command':cmd,
               'scope':'Edited real game captures with scripted controls. Staging remains visibly labeled. No generated gameplay, replacement soundtrack, AI crew operation or continuous encounter claim.'}
    (HERE/'showcase-edit-list.v3.json').write_text(json.dumps(receipt,indent=2)+'\n')
    subprocess.run(cmd,check=True)
    receipt['verification'] = verify(out,elapsed,24)
    receipt['output_sha256'] = digest(out)
    (HERE/'showcase-verification.v3.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(out,elapsed/24,out.stat().st_size,flush=True)

if __name__ == '__main__':
    main()
