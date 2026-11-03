#!/usr/bin/env python3
"""Contact sheets for the vehicle surface model's review stills (fx-review part `materials`).

One sheet per view: every still of that view in the order the harness rendered it, each labelled,
plus a legend tile. Stills are laid at their internal size (no resampling); write JPEG for flipping.

    python3 tools/vsm_contact_sheet.py captures/vsm_20261102_p1b captures/vsm_20261102_p1b_sheet
"""
import glob, json, os, re, sys
from PIL import Image, ImageDraw, ImageFont

FONT = "/usr/share/fonts/TTF/DejaVuSans.ttf"
BOLD = "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"

LEGENDS = {
    "class": "class: painted steel slate | track brown | exhaust & cables red | running gear yellow | gun violet | floor green | seat dark brown | instruments cyan | magenta no record",
    "open": "openness (0.6 / 2 / 6 cm reach): flat face pale | crease, weld, recess blue | convex edge, corner orange to red",
    "sky": "sky: share of the visible hemisphere that reaches open sky, violet 0 -> blue -> green -> yellow -> white 1",
    "enclosure": "enclosure: share of the hemisphere free for 25 cm, violet enclosed -> white open",
    "edge": "edge: nearest sharp convex edge, hue by angle (blue shallow facet, orange 90 deg, red sharp), fading over 2.5 cm; dark ring 4 mm out",
    "thickness": "thickness behind the face (log): violet 2 mm sheet, blue 1 cm, green 3 cm, yellow 8 cm, white 25 cm",
    "cells": "cells: one colour per 1 cm cell with a record, black where none",
}


def main(prefix, out_prefix, cols=4):
    receipt = json.load(open(prefix + ".json"))
    shots = [s for s in receipt["stills"] if "path" in s]
    views = {}
    for s in shots:
        m = re.search(r"materials_(.+?)_(\d+)([a-z_]+)$", s["name"])
        if not m:
            continue
        views.setdefault(m.group(1), []).append((int(m.group(2)), m.group(3), s))
    font = ImageFont.truetype(BOLD, 26)
    small = ImageFont.truetype(FONT, 20)
    written = []
    for view, items in views.items():
        items.sort()
        first = Image.open(items[0][2]["path"])
        w, h = first.size
        n = len(items) + 1
        rows = (n + cols - 1) // cols
        sheet = Image.new("RGB", (cols * w, rows * h), (18, 18, 20))
        draw = ImageDraw.Draw(sheet)
        for i, (_, channel, s) in enumerate(items):
            img = Image.open(s["path"]).convert("RGB")
            x, y = (i % cols) * w, (i // cols) * h
            sheet.paste(img, (x, y))
            label = "current finish" if channel == "off" else channel.replace("_", " ")
            tw = draw.textlength(label, font=font)
            draw.rectangle([x + 10, y + 10, x + 26 + tw, y + 48], fill=(0, 0, 0))
            draw.text((x + 18, y + 13), label, font=font, fill=(240, 240, 235))
        # Legend tile.
        i = len(items)
        x, y = (i % cols) * w, (i // cols) * h
        note = items[0][2].get("note", "").split(":")[0]
        draw.text((x + 24, y + 24), f"{view.replace('_', ' ')}: {note}", font=font, fill=(240, 240, 235))
        ty = y + 80
        for key, text in LEGENDS.items():
            words, line = text.split(" "), ""
            for word in words:
                trial = (line + " " + word).strip()
                if draw.textlength(trial, font=small) > w - 48:
                    draw.text((x + 24, ty), line, font=small, fill=(200, 200, 195)); ty += 26; line = word
                else:
                    line = trial
            draw.text((x + 24, ty), line, font=small, fill=(200, 200, 195)); ty += 36
        path = f"{out_prefix}_{view}.jpg"
        sheet.save(path, quality=92)
        written.append(path)
        print(path)
    return written


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
