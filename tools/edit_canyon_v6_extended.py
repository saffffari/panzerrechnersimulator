#!/usr/bin/env python3
"""Chaptered extended edit plus an uninterrupted first-person encounter from the real attempt."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]
CAP=ROOT/'captures'
PREFIX=CAP/'canyon_v6_extended137_20260930'


def stamp(t):
    n=round(t*100)
    return f'{n//360000}:{n//6000%60:02}:{n//100%60:02}.{n%100:02}'


def main():
    source=PREFIX.with_suffix('.mp4')
    probe=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams','-of','json',str(source)]))
    video=next(s for s in probe['streams'] if s['codec_type']=='video')
    duration=float(video['duration'])
    expected=json.loads((CAP/'canyon_v6_attempt137_20260930_result.json').read_text())
    actual=json.loads(Path(f'{PREFIX}_result.json').read_text())
    assert actual['outcome']==expected['outcome']
    assert actual['final']==expected['final'], 'probe and film simulation differ; review before editing'
    segments=json.loads(Path(f'{PREFIX}_segments.json').read_text())['segments']
    events=[]
    def title(a,b,style,text):
        events.append(f'Dialogue: 0,{stamp(a)},{stamp(b)},{style},,0,0,0,,{text}')
    title(0,5,'Heading',r'PANZERRECHNERSYSTEM\NEXTENDED CANYON RUN')
    title(0,5,'Subtitle','SEED 137 / FIRST-PERSON GAME CAPTURE')
    for s in segments:
        start=s['film_start_s'];sim=s['simulation_start_s']
        title(start,start+3.5,'Chapter',f'{int(sim)//60:02}:{int(sim)%60:02} / '+s['label'].upper())
    title(duration+.6,duration+8,'ResultHeading','ATTEMPT ENDED / NAVIGATION STALL')
    title(duration+.6,duration+8,'Results',r'17:15 elapsed / 65% maximum route progress\N3 T-34s and 2 AT guns destroyed\N5 shots / 5 complete manual reloads\NTiger still alive / 8 of 11 admitted T-34s remain')
    title(duration+.6,duration+8,'Disclosure',r'Rule-based test crew / known route / geometry-assisted aim\NNormal damage, finite ammunition and adaptive reinforcements / no automatic repair')
    ass=CAP/'canyon_v6_extended137_20260930.ass'
    ass.write_text('''[Script Info]
ScriptType: v4.00+
PlayResX: 1920
PlayResY: 1080
WrapStyle: 2

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Heading,DejaVu Sans,43,&H00EAE8E2,&H00FFFFFF,&HAA10100E,&H990B0D0D,1,0,0,0,100,100,1,0,1,2,1,7,64,64,64,1
Style: Subtitle,DejaVu Sans,23,&H00D8D8CD,&H00FFFFFF,&HAA10100E,&H990B0D0D,0,0,0,0,100,100,0,0,1,2,1,7,66,64,186,1
Style: Chapter,DejaVu Sans,25,&H00EAE8E2,&H00FFFFFF,&HAA10100E,&H990B0D0D,1,0,0,0,100,100,0,0,1,2,1,9,64,64,64,1
Style: ResultHeading,DejaVu Sans,45,&H00EAE8E2,&H00FFFFFF,&H0010100E,&H000B0D0D,1,0,0,0,100,100,1,0,1,0,0,8,68,68,220,1
Style: Results,DejaVu Sans,32,&H00D8D8CD,&H00FFFFFF,&H0010100E,&H000B0D0D,0,0,0,0,100,100,0,0,1,0,0,5,68,68,0,1
Style: Disclosure,DejaVu Sans,24,&H00AAAFA8,&H00FFFFFF,&H0010100E,&H000B0D0D,0,0,0,0,100,100,0,0,1,0,0,2,68,68,175,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
'''+ '\n'.join(events)+'\n')
    output=CAP/'Panzerrechnersystem_Canyon_Extended_Highlights_20260930.mp4'
    filters=f'[0:v]tpad=stop_mode=add:stop_duration=8:color=black,ass={ass}[v];[0:a]apad=pad_dur=8[a]'
    command=['ffmpeg','-v','warning','-nostdin','-n','-i',str(source),'-filter_complex',filters,
        '-map','[v]','-map','[a]','-t',str(duration+8),'-c:v','libx264','-preset','fast','-crf','18',
        '-pix_fmt','yuv420p','-c:a','aac','-b:a','192k','-movflags','+faststart',str(output)]
    subprocess.run(command,check=True)
    encounter=next(s for s in segments if s['label'].startswith('T-34 pair'))
    uncut=CAP/'Panzerrechnersystem_Canyon_First_Person_Uncut_20260930.mp4'
    subprocess.run(['ffmpeg','-v','warning','-nostdin','-n','-ss',str(encounter['film_start_s']),
        '-i',str(source),'-t','103','-c:v','libx264','-preset','fast','-crf','18','-pix_fmt','yuv420p',
        '-c:a','aac','-b:a','192k','-movflags','+faststart',str(uncut)],check=True)
    receipt=dict(source=source.name,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
        highlight=output.name,highlight_seconds=duration+8,clean_first_person=source.name,
        uninterrupted_encounter=uncut.name,uninterrupted_simulation_seconds=[517,620],
        probe_and_recording_final_states_identical=True,actual_result=actual,
        editing='Chronological excerpts at original speed, native audio, brief editorial chapters and eight-second result card. No staged death in the actual run.',command=command)
    (CAP/'canyon_v6_extended_edit_20260930.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(output,flush=True)
    print(uncut,flush=True)


if __name__=='__main__':
    main()
