#!/usr/bin/env python3
"""Cut the trailer from the engine's own clips: trims, short crossfades, each clip's game audio."""
import subprocess, sys
from pathlib import Path
C = Path('/home/alex/panzer/adaptive-audio-20261002/.claude/worktrees/bug-squash-20261003/captures')
OUT = C/(sys.argv[1] if len(sys.argv) > 1 else 'panzerrechnersystem_trailer_v1_20261004.mp4')
# (clip, take, start s, length s)
CUTS = [
    ('daydrive', 'c', 3.5, 5.0),
    ('sight', 'c', 1.2, 7.3),
    ('reload', 'c', 1.5, 8.5),
    ('sunset', 'b', 5.0, 14.0),
    ('onfoot', 'd', 0.4, 4.2),
    ('moon', 'c', 8.0, 8.0),
    ('wintercombat', 'c', 10.0, 8.0),
]
X = 0.5       # crossfade between beats
TAIL = 1.2    # fade to black and silence at the end
inputs, parts = [], []
for i, (name, take, start, length) in enumerate(CUTS):
    inputs += ['-ss', f'{start}', '-t', f'{length}', '-i', str(C/f'trailer_{name}_{take}_20261004.mp4')]
    parts.append(f'[{i}:v]fps=24,scale=1920:1080:flags=neighbor,format=yuv420p,setpts=PTS-STARTPTS[v{i}];'
                 f'[{i}:a]aresample=48000,aformat=channel_layouts=stereo,asetpts=PTS-STARTPTS[a{i}]')
graph = ';'.join(parts)
v, a, at = 'v0', 'a0', CUTS[0][3]
for i in range(1, len(CUTS)):
    graph += f';[{v}][v{i}]xfade=transition=fade:duration={X}:offset={at - X:.3f}[xv{i}]'
    graph += f';[{a}][a{i}]acrossfade=d={X}:c1=tri:c2=tri[xa{i}]'
    v, a, at = f'xv{i}', f'xa{i}', at + CUTS[i][3] - X
graph += (f';[{v}]fade=t=in:d=1.0,fade=t=out:st={at - TAIL:.3f}:d={TAIL},tpad=stop_duration=1.0:color=black[vout]'
          f';[{a}]afade=t=in:d=1.0,afade=t=out:st={at - TAIL:.3f}:d={TAIL},apad=pad_dur=1.0,'
          f'loudnorm=I=-16:TP=-1.5:LRA=14[aout]')
cmd = ['ffmpeg', '-v', 'warning', '-nostdin', '-y'] + inputs + ['-filter_complex', graph, '-map', '[vout]',
    '-map', '[aout]', '-c:v', 'libx264', '-preset', 'slow', '-crf', '16', '-pix_fmt', 'yuv420p',
    '-c:a', 'aac', '-b:a', '224k', '-ar', '48000', '-movflags', '+faststart', str(OUT)]
subprocess.run(cmd, check=True)
print(OUT, f'{at + 1.0:.1f} s')
