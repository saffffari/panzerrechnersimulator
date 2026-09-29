#!/usr/bin/env python3
"""Generated sound for the cellar scenes (cellars and scenes, 29 Sep 2026).

One cue, deterministic and noise-only (no recording, no music, no voice):

- ``pressure_lamp_hiss``: the steady hiss of a kerosene pressure lamp's burner (R2 section 4.2:
  "the pressure lamp's hiss"; Dibold via Beevor ch. 22: a field-hospital gas lamp). Vaporised
  fuel leaves the jet as a turbulent flow; its sound is broadband noise with most of its energy
  between about 1 and 6 kHz over a faint low roar of the mantle's flame. Made here as white noise
  shaped in the frequency domain over one whole period, so the 6 s clip loops without a seam.

The clip is written to ``assets/sounds/cellars_20260929/`` (untracked, like every sound asset) and
described by a receipt next to it; the tableau catalog names the file. Its level is the scene
emitter's authored gain, outside the per-voice calibration of ``sound_levels`` (the ledger's
authored-emitter row). Rerunning the tool rewrites the same bytes on the same platform and checks
them against the receipt.

    python3 tools/worldgen/cellar_scene_audio_20260929.py
"""
from __future__ import annotations

import hashlib
import json
import subprocess
import wave
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets/sounds/cellars_20260929"
SR = 44100
SEED = 0x20260929


def hiss(seconds: float = 6.0) -> np.ndarray:
    n = int(seconds * SR)
    rng = np.random.default_rng(SEED)
    spectrum = np.fft.rfft(rng.normal(0.0, 1.0, n))
    f = np.fft.rfftfreq(n, 1.0 / SR)
    # Burner jet: a broad band 1-6 kHz (second-order shoulders), a faint roar below 300 Hz, nothing
    # above 12 kHz. Shaping the whole period's spectrum keeps the clip periodic (a seamless loop).
    jet = 1.0 / (1.0 + (1000.0 / np.maximum(f, 1.0)) ** 2) / (1.0 + (f / 6000.0) ** 2)
    roar = 0.18 / (1.0 + (f / 300.0) ** 2) * (f > 40.0)
    shaped = spectrum * (jet + roar) * (f < 12000.0)
    pcm = np.fft.irfft(shaped, n)
    return pcm * (0.2 / float(np.max(np.abs(pcm))))


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    pcm16 = np.rint(np.clip(hiss(), -1.0, 1.0) * 32767).astype("<i2")
    raw = pcm16.tobytes()
    wav = OUT / "pressure_lamp_hiss.generated.wav"
    target = OUT / "pressure_lamp_hiss.ogg"
    with wave.open(str(wav), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(SR)
        f.writeframes(raw)
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i", str(wav),
                    "-c:a", "libvorbis", "-q:a", "5", str(target)], check=True)
    decoded = np.frombuffer(subprocess.check_output(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(target), "-f", "f32le",
         "-acodec", "pcm_f32le", "-"]), dtype="<f4")
    assert np.isfinite(decoded).all() and float(np.max(np.abs(decoded))) < 0.3
    receipt = {
        "asset": target.relative_to(ROOT).as_posix(),
        "intent": "Steady kerosene pressure-lamp burner hiss, looped; not a recording.",
        "license": "Original generated game sound; no external recording or music used.",
        "source": "tools/worldgen/cellar_scene_audio_20260929.py: seeded NumPy white noise shaped over one "
                  "period in the frequency domain (1-6 kHz jet band, faint roar below 300 Hz)",
        "pcm_sha256": hashlib.sha256(raw).hexdigest(),
        "ogg_sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
        "seconds": len(pcm16) / SR,
        "decoded_peak": float(np.max(np.abs(decoded))),
        "decoded_rms": float(np.sqrt(np.mean(decoded ** 2))),
        "calibration": "Authored scene-emitter gain only; outside the per-voice sound_levels calibration.",
    }
    (OUT / "pressure_lamp_hiss.receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    wav.unlink()
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
