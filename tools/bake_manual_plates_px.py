#!/usr/bin/env python3
"""The manual's plates for the pixel book: each works-drawing SVG from tools/bake_manual_plates.py
re-rasterised at a few small widths with the pen widened so a line is one to two pixels, then
thresholded to solid pixels (no anti-aliasing: the book is drawn on the game's pixel grid, one
texel per pixel). Black lines, the red layer solid red. Output: assets/manual/plates/px<W>/<name>.png
under KESSEL_ASSET_ROOT, with a report beside each. Not committed (assets).
"""
import json, os, re, subprocess, sys
from PIL import Image

ROOT = os.environ.get("KESSEL_ASSET_ROOT", ".")
SRC = os.path.join(ROOT, "assets/manual/plates")
WIDTHS = [96, 128, 160, 192, 224, 256]
# Pen width in SVG units so that a line is about 1.4 px at the target width: units per px times 1.4.
LINE_PX = 1.4
THRESHOLD = 0.35

def bake(name, svg_path):
    svg = open(svg_path).read()
    m = re.search(r'viewBox="([-\d.]+) ([-\d.]+) ([-\d.]+) ([-\d.]+)"', svg)
    if not m:
        print(f"[plates-px] {name}: no viewBox"); return
    units_w = float(m.group(3))
    for width in WIDTHS:
        pen = units_w / width * LINE_PX
        s = re.sub(r'stroke-width="[\d.]+"', f'stroke-width="{pen:.3f}"', svg)
        out_dir = os.path.join(SRC, f"px{width}")
        os.makedirs(out_dir, exist_ok=True)
        tmp_svg = os.path.join(out_dir, f"{name}.svg")
        open(tmp_svg, "w").write(s)
        png = os.path.join(out_dir, f"{name}.png")
        subprocess.run(["rsvg-convert", "-w", str(width), "-b", "#00000000", tmp_svg, "-o", png], check=True)
        im = Image.open(png).convert("RGBA")
        px = im.load()
        solid = 0
        for y in range(im.height):
            for x in range(im.width):
                r, g, b, a = px[x, y]
                if a / 255.0 >= THRESHOLD:
                    # Red layer pixels keep their hue; black lines stay black.
                    red = r > 90 and r > g * 2
                    px[x, y] = (200, 50, 30, 255) if red else (0, 0, 0, 255)
                    solid += 1
                else:
                    px[x, y] = (0, 0, 0, 0)
        im.save(png)
        os.remove(tmp_svg)
        json.dump({"schema": "kessel.manual-plate-px.v1", "name": name, "width": width, "height": im.height,
                   "pen_svg_units": round(pen, 3), "line_px": LINE_PX, "threshold": THRESHOLD, "solid_pixels": solid,
                   "source_svg": svg_path}, open(os.path.join(out_dir, f"{name}.report.json"), "w"), indent=1)
        print(f"[plates-px] {name} @ {width}: {im.width}x{im.height}, {solid} solid px")

names = sys.argv[1:] or sorted(f[:-4] for f in os.listdir(SRC) if f.endswith(".svg"))
for name in names:
    bake(name, os.path.join(SRC, f"{name}.svg"))
