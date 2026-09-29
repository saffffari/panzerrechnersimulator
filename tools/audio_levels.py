#!/usr/bin/env python3
"""Measure what the player receives from a capture: the short-term A-weighted level (LAF, IEC 61672
A-weighting and fast 125 ms time weighting of the mean power of both channels, 0 dBFS(A) = a
full-scale 1 kHz sine) and the sample peak, over time, from a 16-bit stereo WAV or a video's audio
track. The meter mirrors `reproduction::LafMeter` (same bilinear A-weighting sections).

usage: audio_levels.py <file.wav|file.mp4> [window_s=0.25] [--events label:start:end ...]

Prints a timeline (LAF max and sample peak per window) and, for each labelled interval, the
LAF,max and sample peak inside it.
"""
import math
import subprocess
import sys
import wave

import numpy as np

RATE = 44100


def read_audio(path):
    if path.lower().endswith(".wav"):
        with wave.open(path, "rb") as w:
            assert w.getsampwidth() == 2 and w.getnchannels() == 2, "16-bit stereo WAV expected"
            rate = w.getframerate()
            data = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    else:
        rate = RATE
        raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-vn", "-f", "s16le", "-ac", "2",
                              "-ar", str(rate), "-"], check=True, capture_output=True).stdout
        data = np.frombuffer(raw, dtype="<i2")
    stereo = data.reshape(-1, 2).astype(np.float64) / 32767.0
    return rate, stereo


def a_weighting_sections(fs):
    def bilinear(pole_hz):
        s = -2.0 * math.pi * pole_hz
        return (2.0 * fs + s) / (2.0 * fs - s)
    p1, p2, p3, p4 = (bilinear(20.598997), bilinear(107.65265), bilinear(737.86223),
                      bilinear(12194.217))
    sections = [((1.0, -2.0, 1.0), (-(p1 + p1), p1 * p1)),
                ((1.0, -2.0, 1.0), (-(p2 + p3), p2 * p3)),
                ((1.0, 2.0, 1.0), (-(p4 + p4), p4 * p4))]
    w = 2.0 * math.pi * 1000.0 / fs
    z = complex(math.cos(w), -math.sin(w))
    h = 1.0
    for b, a in sections:
        h *= (b[0] + b[1] * z + b[2] * z * z) / (1.0 + a[0] * z + a[1] * z * z)
    return sections, 1.0 / abs(h)


def biquad(x, b, a):
    """Transposed direct form II over a 1-D array (a plain loop; fast enough for a film)."""
    y = np.empty_like(x)
    z1 = z2 = 0.0
    b0, b1, b2 = b
    a1, a2 = a
    for i, v in enumerate(x.tolist()):
        out = b0 * v + z1
        z1 = b1 * v - a1 * out + z2
        z2 = b2 * v - a2 * out
        y[i] = out
    return y


def laf_dbfs(stereo, rate):
    sections, gain = a_weighting_sections(rate)
    power = np.zeros(len(stereo))
    for ch in range(2):
        y = stereo[:, ch] * gain
        for b, a in sections:
            y = biquad(y, b, a)
        power += 0.5 * y * y
    alpha = 1.0 - math.exp(-1.0 / (0.125 * rate))
    # Fast time weighting: y[n] = y[n-1] + alpha (p[n] - y[n-1]), a one-pole low-pass.
    laf = biquad(power, (alpha, 0.0, 0.0), (-(1.0 - alpha), 0.0))
    return 10.0 * np.log10(np.maximum(laf, 1e-30) / 0.5)


def peak_db(x):
    return 20.0 * math.log10(max(float(np.max(np.abs(x))), 1e-12))


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    path = args[0]
    window = 0.25
    events = []
    rest = args[1:]
    if rest and not rest[0].startswith("--"):
        window = float(rest[0])
        rest = rest[1:]
    if rest and rest[0] == "--events":
        for spec in rest[1:]:
            label, start, end = spec.split(":")
            events.append((label, float(start), float(end)))
    rate, stereo = read_audio(path)
    laf = laf_dbfs(stereo, rate)
    n = int(round(window * rate))
    print(f"{path}: {len(stereo) / rate:.2f} s at {rate} Hz; "
          f"overall LAF,max {laf.max():.1f} dBFS(A), sample peak {peak_db(stereo):.1f} dBFS")
    print("   t (s)   LAF,max dBFS(A)   peak dBFS")
    for start in range(0, len(stereo), n):
        seg = slice(start, min(start + n, len(stereo)))
        print(f"{start / rate:8.2f}   {laf[seg].max():15.1f}   {peak_db(stereo[seg]):9.1f}")
    for label, a, b in events:
        seg = slice(int(a * rate), int(b * rate))
        print(f"[event] {label:28s} {a:6.2f}-{b:6.2f} s  LAF,max {laf[seg].max():6.1f} dBFS(A)  "
              f"peak {peak_db(stereo[seg]):6.1f} dBFS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
