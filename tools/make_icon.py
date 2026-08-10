#!/usr/bin/env python3
"""Regenerate the desktop/taskbar icon: a flat white pixel-Tiger silhouette on black.
Usage: python3 tools/make_icon.py  →  writes assets/ui/kessel_icon.png (800×800).
Needs ImageMagick (`magick`). Deterministic — edit the rects to reshape the tank."""
import subprocess, tempfile, os
import numpy as np

W, H = 48, 23
g = np.zeros((H, W), dtype=np.uint8)
def rect(x0, x1, y0, y1, v=255): g[y0:y1+1, x0:x1+1] = v
# Tiger I, facing right: cupola / turret mass / long 88 + muzzle brake / hull slab / tracks.
rect(13, 17, 2, 3); rect(11, 25, 4, 4); rect(10, 26, 5, 9)   # cupola + turret roof + box
rect(7, 10, 6, 9)                                            # rear stowage bin
rect(26, 43, 6, 7); rect(44, 46, 5, 8)                       # 88 + muzzle brake
rect(2, 45, 10, 14)                                          # hull slab
rect(1, 46, 15, 20)                                          # track run
g[15, 1] = g[15, 46] = g[20, 1] = g[20, 46] = 0              # round the track ends
for x in (5, 11, 17, 23, 29, 35, 41): rect(x, x+2, 17, 19, 0)  # road-wheel rhythm

out = os.path.join(os.path.dirname(__file__), "..", "assets", "ui", "kessel_icon.png")
with tempfile.NamedTemporaryFile("w", suffix=".pgm", delete=False) as f:
    f.write(f"P2\n{W} {H}\n255\n" + "\n".join(" ".join(map(str, row)) for row in g))
    pgm = f.name
subprocess.run(["magick", pgm, "-scale", "1600%", "-background", "black",
                "-gravity", "center", "-extent", "800x800", os.path.abspath(out)], check=True)
os.unlink(pgm)
print("wrote", os.path.abspath(out))
