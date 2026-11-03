"""Work out a recon-camera line for `kessel screenshot` from the act's own heightfield.

    python3 tools/worldgen/cam.py ACT EYE_X EYE_Z TARGET_X TARGET_Z [EYE_H] [PITCH]

Prints the six camera arguments the engine expects (X Y Z YAW PITCH FOG). Heights come from the
generated `heights.npy`, so the eye sits a real distance above the real ground instead of being
guessed and landing underneath it — which renders black and costs a full four-minute frame.

EYE_H defaults to 2.6 m, roughly a Tiger commander's eye in the open hatch.
"""

import math
import os
import sys

import numpy as np


def load(act):
    base = f"assets/worldgen/act{act}"
    h = np.load(os.path.join(base, "heights.npy"))
    import json
    man = json.load(open(os.path.join(base, "manifest.v1.json")))
    return h, float(man["terrain"]["spacing"])


def height_at(h, spacing, x, z):
    """Bilinear sample, matching the engine's centred heightfield convention."""
    res = h.shape[0]
    half = (res - 1) * spacing * 0.5
    fx = (x + half) / spacing
    fz = (z + half) / spacing
    i0, j0 = int(np.clip(fx, 0, res - 2)), int(np.clip(fz, 0, res - 2))
    tx, tz = fx - i0, fz - j0
    tx, tz = float(np.clip(tx, 0, 1)), float(np.clip(tz, 0, 1))
    h00, h10 = h[j0, i0], h[j0, i0 + 1]
    h01, h11 = h[j0 + 1, i0], h[j0 + 1, i0 + 1]
    return float((h00 * (1 - tx) + h10 * tx) * (1 - tz) + (h01 * (1 - tx) + h11 * tx) * tz)


def main():
    act = int(sys.argv[1])
    ex, ez = float(sys.argv[2]), float(sys.argv[3])
    tx, tz = float(sys.argv[4]), float(sys.argv[5])
    eye_h = float(sys.argv[6]) if len(sys.argv) > 6 else 2.6
    pitch_override = float(sys.argv[7]) if len(sys.argv) > 7 else None

    h, spacing = load(act)
    gy = height_at(h, spacing, ex, ez)
    ty = height_at(h, spacing, tx, tz)
    ey = gy + eye_h

    dx, dz = tx - ex, tz - ez
    dist = math.hypot(dx, dz)
    yaw = math.degrees(math.atan2(dx, dz))
    pitch = pitch_override if pitch_override is not None else math.degrees(
        math.atan2((ty + 1.0) - ey, max(dist, 1e-3)))

    print(f"{ex:.1f} {ey:.1f} {ez:.1f} {yaw:.1f} {pitch:.1f} 0.0")
    print(f"# ground {gy:.1f} m at eye, {ty:.1f} m at target, {dist:.0f} m apart",
          file=sys.stderr)


if __name__ == "__main__":
    main()
