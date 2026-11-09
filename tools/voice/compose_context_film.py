#!/usr/bin/env python3
"""Mix crew callouts over an ai-film and burn German + English subtitles; standard resolution.

    python3 tools/voice/compose_context_film.py FILM.mp4 CUES.json LINES_DIR OUT.mp4

FILM.mp4: the `ai-film` picture at the game's grid (KESSEL_AI_FILM_SCALE=1) with the game's own
audio. CUES.json: tools/voice/crew_callouts.py's cues, each with `file` (the processed line, in
LINES_DIR) and `start`/`dur` filled in by `schedule`. The game audio is ducked 5 dB under each line
(40 ms attack, 300 ms release); the lines sit in the centre. Subtitles: the German line, the
English under it, bottom centre, while the line plays. Picture: the grid repeated exactly 5x by
nearest neighbour (1024x576 -> 5120x2880), 4:4:4; HEVC NVENC at constant quality 29, or x264 CRF
18 if NVENC will not open.
"""
import json
import subprocess
import sys
import wave
from pathlib import Path

import numpy as np

SR = 48000


def ff_audio(path, extra=()):
    cmd = ["ffmpeg", "-v", "error", "-nostdin", "-i", str(path), *extra, "-ac", "2", "-ar", str(SR), "-f", "f32le", "-"]
    return np.frombuffer(subprocess.run(cmd, capture_output=True, check=True).stdout, dtype="<f4").reshape(-1, 2).copy()


def ass_time(t):
    h, rem = divmod(max(t, 0.0), 3600)
    m, s = divmod(rem, 60)
    return f"{int(h)}:{int(m):02d}:{s:05.2f}"


def write_ass(cues, path):
    head = """[Script Info]
ScriptType: v4.00+
PlayResX: 5120
PlayResY: 2880
WrapStyle: 0
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: DE,Inter,104,&H00F2F2F2,&H000000FF,&H00000000,&H64000000,1,0,0,0,100,100,0,0,1,7,0,2,200,200,250,1
Style: EN,Inter,80,&H00B9C3CC,&H000000FF,&H00000000,&H64000000,0,1,0,0,100,100,0,0,1,6,0,2,200,200,140,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
"""
    names = {"KDT": "Kommandant", "RS": "Richtschütze", "LS": "Ladeschütze", "FA": "Fahrer", "FU": "Funker"}
    lines = []
    for c in cues:
        a, b = ass_time(c["start"]), ass_time(c["start"] + max(c["dur"], 1.4) + 0.4)
        lines.append(f"Dialogue: 0,{a},{b},DE,,0,0,0,,{names[c['role']]}: {c['de']}")
        lines.append(f"Dialogue: 0,{a},{b},EN,,0,0,0,,{c['en']}")
    Path(path).write_text(head + "\n".join(lines) + "\n", encoding="utf-8")


def main():
    film, cues_path, lines_dir, out = sys.argv[1:5]
    cues = json.load(open(cues_path, encoding="utf-8"))
    game = ff_audio(film)
    n = len(game)
    voice = np.zeros(n, np.float32)
    for c in cues:
        line = ff_audio(Path(lines_dir) / c["file"])[:, 0]
        i = int(c["start"] * SR)
        j = min(n, i + len(line))
        if i < n:
            voice[i:j] += line[: j - i]
    # Duck the game under the voice: presence per 10 ms frame (held over syllable gaps for 120 ms),
    # smoothed with a 40 ms attack and a 300 ms release.
    hop = SR // 100
    frames = (n + hop - 1) // hop
    padded = np.zeros(frames * hop, np.float32)
    padded[:n] = voice
    rms = np.sqrt((padded.reshape(frames, hop) ** 2).mean(axis=1))
    present = np.convolve((rms > 1e-3).astype(np.float32), np.ones(12), mode="full")[:frames] > 0
    env_f = np.zeros(frames, np.float32)
    e, a, r = 0.0, np.exp(-1 / 4.0), np.exp(-1 / 30.0)
    for k in range(frames):
        t = float(present[k])
        e = t + (e - t) * (a if t > e else r)
        env_f[k] = e
    env = np.repeat(env_f, hop)[:n]
    gain = 10 ** (-5 * env / 20)
    mix = game * gain[:, None] + voice[:, None] * 0.89
    peak = np.abs(mix).max()
    if peak > 0.97:
        mix *= 0.97 / peak
    wav = Path(out).with_suffix(".mix.wav")
    with wave.open(str(wav), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((np.clip(mix, -1, 1) * 32767).round().astype("<i2").tobytes())
    ass = Path(out).with_suffix(".ass")
    write_ass(cues, ass)
    vf = f"scale=iw*5:ih*5:flags=neighbor,format=yuv444p,ass={ass}"
    base = ["ffmpeg", "-v", "error", "-nostdin", "-y", "-i", film, "-i", str(wav), "-map", "0:v:0", "-map", "1:a:0",
            "-vf", vf]
    tail = ["-c:a", "aac", "-b:a", "256k", "-shortest", "-movflags", "+faststart", out]
    nvenc = base + ["-c:v", "hevc_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", "29", "-b:v", "0", "-profile:v", "rext",
                    "-pix_fmt", "yuv444p", "-tag:v", "hvc1"] + tail
    if subprocess.run(nvenc).returncode != 0:
        print("[compose] NVENC failed to open; libx264 CRF 18", flush=True)
        subprocess.run(base + ["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv444p"] + tail, check=True)
        print("[compose] encoder: libx264 crf 18 yuv444p")
    else:
        print("[compose] encoder: hevc_nvenc cq 29 yuv444p (rext)")
    wav.unlink()
    print(f"[compose] wrote {out}; subtitles {ass}")


if __name__ == "__main__":
    main()
