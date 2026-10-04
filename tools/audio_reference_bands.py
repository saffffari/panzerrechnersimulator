#!/usr/bin/env python3
"""Octave-band level of one event in a recording, with the recording's own background removed.

The listening references are film soundtrack: an engine drone runs under everything. An event's
spectrum is therefore taken as (power in the event window) minus (power in a quiet window of the
same file), per octave band, floored at zero. Bands are the engine's nine octave centres, so a
difference between a reference and a capture of the game is directly a per-band correction.

usage: audio_reference_bands.py FILE EVENT_FROM EVENT_TO [BG_FROM BG_TO] [--json]
"""
import json
import subprocess
import sys

import numpy as np

CENTRES = [31.5, 63, 125, 250, 500, 1000, 2000, 4000, 8000]
RATE = 44100


def load(path):
    raw = subprocess.run(['ffmpeg', '-v', 'error', '-i', path, '-ac', '1', '-ar', str(RATE), '-f', 'f32le', '-'],
        capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32).astype(np.float64)


def band_power(x):
    """Mean power in each octave band (Hann-windowed frames, averaged)."""
    n = 4096
    if len(x) < n:
        x = np.pad(x, (0, n-len(x)))
    hop = n//2
    frames = [x[i:i+n]*np.hanning(n) for i in range(0, len(x)-n+1, hop)]
    spectrum = np.mean([np.abs(np.fft.rfft(f))**2 for f in frames], axis=0)/(n*n/4)
    freqs = np.fft.rfftfreq(n, 1/RATE)
    return np.array([spectrum[(freqs >= c/np.sqrt(2)) & (freqs < c*np.sqrt(2))].sum() for c in CENTRES])


def event_bands(path, start, end, bg=None):
    x = load(path)
    event = band_power(x[int(start*RATE):int(end*RATE)])
    if bg:
        event = np.maximum(event-band_power(x[int(bg[0]*RATE):int(bg[1]*RATE)]), event*1e-3)
    return 10*np.log10(event+1e-20)


def envelope(path, start, end, step=0.05):
    """Broadband level every `step` seconds, dB relative to the window's loudest step."""
    x = load(path)[int(start*RATE):int(end*RATE)]
    k = int(step*RATE)
    rms = np.array([np.sqrt(np.mean(x[i:i+k]**2)+1e-20) for i in range(0, len(x)-k+1, k)])
    return 20*np.log10(rms/rms.max())


if __name__ == '__main__':
    args = [a for a in sys.argv[1:] if not a.startswith('--')]
    path, start, end = args[0], float(args[1]), float(args[2])
    bg = (float(args[3]), float(args[4])) if len(args) >= 5 else None
    bands = event_bands(path, start, end, bg)
    shape = bands-bands.max()
    if '--json' in sys.argv:
        print(json.dumps(dict(file=path, window=[start, end], background=bg, centres_hz=CENTRES,
            band_db=[round(float(v), 2) for v in bands], shape_db=[round(float(v), 2) for v in shape])))
    else:
        print(' '.join(f'{c:>7}' for c in CENTRES))
        print(' '.join(f'{v:7.1f}' for v in shape), ' (dB re loudest band)')
        print('envelope', ' '.join(f'{v:.0f}' for v in envelope(path, start, end)))
