#!/usr/bin/env python3
"""Bake the repair manual's drawing plates from the owner's Tiger CAD.

The CAD (``_Reference_CAD_Tigers``, a 1/35 model-kit STEP set, SolidWorks 2018) is scaled by 35
and projected by FreeCAD's TechDraw as hidden-line-removed line art, one plate per entry of the
plate list below; a plate may explode its solids along an axis and paint chosen solids red (the
part the page is about). The SVG fragments TechDraw returns are wrapped, composited (black lines
over red fills) and rasterised with ``rsvg-convert`` to PNG under the asset root
(``assets/manual/plates/<name>.png``, not committed) with ``<name>.report.json`` beside each.

Run with FreeCAD's own Python::

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem \\
    PLATES=engine_iso,engine_side FreeCADCmd tools/bake_manual_plates.py < /dev/null

Positional arguments are not used because FreeCADCmd opens them as documents. ``PLATES`` lists
the plates to bake (all when unset); ``CAD_ROOT`` overrides the STEP folder. Hidden-line removal
of the 14-solid engine takes about five minutes per view on the i9; the 1,490-solid assembly is
not baked whole, only the solid groups a plate names.
"""
import json
import os
import re
import subprocess
import sys
import time

import FreeCAD
import Part
import TechDraw

CAD_ROOT = os.environ.get("CAD_ROOT", "/windows/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/"
                          "panzer-vi-tiger-1-35-1.snapshot.2/panzer-vi-tiger-1-35-1.snapshot.2")
ASSET_ROOT = os.environ.get("KESSEL_ASSET_ROOT", ".")
OUT_DIR = os.path.join(ASSET_ROOT, "assets", "manual", "plates")
SCALE = 35.0
ENGINE = "TG_RM5010-0B29_EngineSystem.STEP"
BASE_UNIT = "TG_RM5010-0B16_Base Unit.STEP"

# name -> (step file, view direction, x direction, solid indices or None for all, red indices,
#          explode axis or None, explode spacing mm)
PLATES = {
    "engine_iso": (ENGINE, (1, -1, 1), (1, 0, -1), None, [], None, 0),
    "engine_side": (ENGINE, (1, 0, 0), (0, 1, 0), None, [], None, 0),
    "engine_front": (ENGINE, (0, -1, 0), (1, 0, 0), None, [], None, 0),
    "engine_exploded": (ENGINE, (1, -1, 1), (1, 0, -1), None, [], (0, 0, 1), 220),
    "engine_fan_drive_red": (ENGINE, (1, -1, 1), (1, 0, -1), None, [2, 3], None, 0),
}


def wrap(fragment):
    nums = [float(v) for v in re.findall(r"(-?\d+\.?\d*)", " ".join(re.findall(r'd="([^"]+)"', fragment)))]
    xs, ys = nums[0::2], nums[1::2]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    pad = 0.04 * max(x1 - x0, y1 - y0)
    return (x0 - pad, y0 - pad, (x1 - x0) + 2 * pad, (y1 - y0) + 2 * pad)


def view_svg(doc, page, feature, direction, xdir, hidden):
    v = doc.addObject("TechDraw::DrawViewPart", "View")
    v.Source = [feature]
    v.Direction = FreeCAD.Vector(*direction)
    v.XDirection = FreeCAD.Vector(*xdir)
    v.HardHidden = hidden
    v.SmoothVisible = True
    v.Scale = 0.12
    page.addView(v)
    doc.recompute()
    svg = TechDraw.viewPartAsSvg(v)
    doc.removeObject(v.Name)
    return svg


def bake(name, spec):
    step, direction, xdir, keep, red, axis, spacing = spec
    t0 = time.time()
    shape = Part.Shape()
    shape.read(os.path.join(CAD_ROOT, step))
    m = FreeCAD.Matrix()
    m.scale(SCALE, SCALE, SCALE)
    shape = shape.transformGeometry(m)
    solids = list(shape.Solids)
    if keep is not None:
        solids = [solids[i] for i in keep]
    if axis is not None:
        # Exploded along the axis by index order: a kit's exploded view, not an assembly sequence.
        moved = []
        for n, s in enumerate(solids):
            c = s.copy()
            c.translate(FreeCAD.Vector(*axis) * (n * spacing))
            moved.append(c)
        solids = moved
    doc = FreeCAD.newDocument("plate")
    page = doc.addObject("TechDraw::DrawPage", "Page")
    tmpl = doc.addObject("TechDraw::DrawSVGTemplate", "Template")
    tmpl.Template = os.path.join(FreeCAD.getResourceDir(), "Mod/TechDraw/Templates/ISO/A3_Landscape_blank.svg")
    page.Template = tmpl
    black = doc.addObject("Part::Feature", "Black")
    black.Shape = Part.makeCompound(solids)
    layers = [("black", view_svg(doc, page, black, direction, xdir, False))]
    if red:
        red_feature = doc.addObject("Part::Feature", "Red")
        red_feature.Shape = Part.makeCompound([solids[i] for i in red if i < len(solids)])
        layers.append(("red", view_svg(doc, page, red_feature, direction, xdir, False)))
    x, y, w, h = wrap(layers[0][1])
    body = []
    for layer, fragment in layers:
        if layer == "red":
            # The named part: a red fill under its own outline, the Fibel's spot colour.
            fragment = fragment.replace('fill="none"', 'fill="#c8321e" fill-opacity="0.55"').replace('stroke="#000000"', 'stroke="#7a1a0e"')
        body.append(fragment)
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x:.2f} {y:.2f} {w:.2f} {h:.2f}">\n' + "\n".join(body) + "\n</svg>"
    os.makedirs(OUT_DIR, exist_ok=True)
    svg_path = os.path.join(OUT_DIR, f"{name}.svg")
    png_path = os.path.join(OUT_DIR, f"{name}.png")
    open(svg_path, "w").write(svg)
    subprocess.run(["rsvg-convert", "-w", "2048", "-b", "#00000000", svg_path, "-o", png_path], check=True)
    report = {"schema": "kessel.manual-plate-bake.v1", "name": name, "source": os.path.join(CAD_ROOT, step),
              "source_scale": "1/35 model-kit CAD scaled by 35; dimensions are the kit's, not a measured vehicle",
              "solids": len(solids), "red_solids": red, "exploded": {"axis": axis, "spacing_mm": spacing} if axis else None,
              "direction": direction, "seconds": round(time.time() - t0, 1), "png": png_path, "svg": svg_path,
              "projection": "FreeCAD TechDraw DrawViewPart, hidden lines removed, smooth edges visible, scale 0.12 on A3"}
    json.dump(report, open(os.path.join(OUT_DIR, f"{name}.report.json"), "w"), indent=1)
    FreeCAD.closeDocument(doc.Name)
    print(f"[plates] {name}: {len(solids)} solids, {report['seconds']} s -> {png_path}")


if __name__ == "__main__":
    wanted = [p for p in os.environ.get("PLATES", "").split(",") if p] or list(PLATES)
    for name in wanted:
        bake(name, PLATES[name])
