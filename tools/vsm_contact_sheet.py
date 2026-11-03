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
    "processes": "flow / rust carried / dirt carried / mud / heat / soot / oil / standing water: the simulated amount, violet none -> white most",
    "class": "class: painted steel slate | track brown | exhaust & cables red | running gear yellow | gun violet | floor green | seat dark brown | instruments cyan | magenta no record",
    "open": "openness (0.6 / 2 / 6 cm reach): flat face pale | crease, weld, recess blue | convex edge, corner orange to red",
    "sky": "sky: share of the visible hemisphere that reaches open sky, violet 0 -> blue -> green -> yellow -> white 1",
    "enclosure": "enclosure: share of the hemisphere free for 25 cm, violet enclosed -> white open",
    "edge": "edge: nearest sharp convex edge, hue by angle (blue shallow facet, orange 90 deg, red sharp), fading over 2.5 cm; dark ring 4 mm out",
    "thickness": "thickness behind the face (log): violet 2 mm sheet, blue 1 cm, green 3 cm, yellow 8 cm, white 25 cm",
    "cells": "cells: one colour per 1 cm cell with a record, black where none",
}


LAYER_LEGEND = [
    ("substrate", "the part's metal by class: rolled plate or sheet steel, track manganese, heat-scaled exhaust, the oiled breech"),
    ("primer", "red oxide (RAL 8012) over all painted steel"),
    ("topcoat", "Dunkelgrau RAL 7021 outside, Elfenbein RAL 1001 in the fighting compartment; spray thickness varies"),
    ("rounding", "edges rounded by paint build-up and wear: the shading normal over a 2.5-6 mm fillet"),
    ("chips", "exposed convex edges chipped through the topcoat (primer halo) to steel; more where proud, low and forward"),
    ("contact", "the crew's hands and boots: paint worn through on edges and high points, steel polished, flats burnished"),
    ("corrosion", "rust on exposed steel by wetness (sky, trapped water, the floor's snow), creeping under broken paint"),
    ("deposits", "grime packed in creases and enclosed corners, hands' grime, dust on what faces up, fumes' soot under the roof"),
    ("weather", "UV chalking: paint paler and flatter where the sky sees it"),
    ("streaks", "rain and melt simulated running down the faces: washed paler, dirt and rust laid down where the water carried them"),
    ("mud", "clods simulated off the moving tracks, flown ballistically: spatter where they strike (winter grit-laden slush)"),
    ("heat", "paint burnt by the exhaust (simulated distance from the pipes) and the barrel's firing heat toward the muzzle"),
    ("soot", "the exhaust plume simulated as buoyant turbulent gas, soot where it touches; propellant soot on brake and mantlet"),
    ("oil", "the final drives' seals weeping, the oil simulated running down the hubs and the nose"),
]


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
        layered = any(c.startswith("lit_") for _, c, _ in items)
        if layered:
            cols = 5
        rows = (n + cols - 1) // cols
        sheet = Image.new("RGB", (cols * w, rows * h), (18, 18, 20))
        draw = ImageDraw.Draw(sheet)
        for i, (_, channel, s) in enumerate(items):
            img = Image.open(s["path"]).convert("RGB")
            x, y = (i % cols) * w, (i // cols) * h
            sheet.paste(img, (x, y))
            label = {"off": "current finish", "before": "before: the former noise finish"}.get(channel, channel.replace("_", " "))
            if channel.startswith("lit_") or channel.startswith("albedo_"):
                kind, layer = channel.split("_", 1)
                k = [l for l, _ in LAYER_LEGEND].index(layer) + 1
                label = f"{k} {layer}" + ("" if kind == "lit" else " (albedo, unlit)")
            tw = draw.textlength(label, font=font)
            draw.rectangle([x + 10, y + 10, x + 26 + tw, y + 48], fill=(0, 0, 0))
            draw.text((x + 18, y + 13), label, font=font, fill=(240, 240, 235))
        # Legend tile.
        i = len(items)
        x, y = (i % cols) * w, (i // cols) * h
        note = items[0][2].get("note", "").split(":")[0]
        draw.text((x + 24, y + 24), f"{view.replace('_', ' ')}: {note}", font=font, fill=(240, 240, 235))
        ty = y + 80
        legend = [f"{i + 1} {l}: {t}" for i, (l, t) in enumerate(LAYER_LEGEND)] if layered else list(LEGENDS.values())
        if layered:
            legend.insert(0, "Each layer adds to those before it. Top rows lit at noon; bottom rows the same layers' base colour, unlit.")
        for text in legend:
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
