#!/usr/bin/env python3
"""Preview images of a crew anatomy volume: slice montages and a voxel cutaway.

    python3 -I tools/anatomy/render_anatomy_previews.py VOLUME.kvox OUT_PREFIX [--table JSON]

Writes OUT_PREFIX_axial.png, _coronal.png, _sagittal.png, _cutaway_left.png and _cutaway_front.png
(refusing to overwrite existing files). Slices are drawn with nearest-neighbour scaling, the figure's anatomical left on
the image's right (viewed from the front for coronal slices, from below for axial slices, from
the figure's right for sagittal slices), so the images read like radiological views.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import kvox  # noqa: E402

REPO = HERE.parents[1]
BG = (24, 24, 28)
INK = (225, 225, 230)


def palette(table: dict) -> np.ndarray:
    lut = np.zeros((256, 3), dtype=np.uint8)
    for row in table["labels"]:
        c = row["colour"].lstrip("#")
        lut[row["id"]] = [int(c[i:i + 2], 16) for i in (0, 2, 4)]
    lut[0] = BG
    return lut


def font(size: int):
    for name in ("DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def tile(img: np.ndarray, scale: int, title: str) -> Image.Image:
    h, w = img.shape[:2]
    im = Image.fromarray(img).resize((w * scale, h * scale), Image.NEAREST)
    out = Image.new("RGB", (w * scale, h * scale + 22), BG)
    out.paste(im, (0, 22))
    ImageDraw.Draw(out).text((4, 3), title, fill=INK, font=font(14))
    return out


def grid_sheet(tiles: list[Image.Image], cols: int, title: str, legend: Image.Image | None) -> Image.Image:
    rows = (len(tiles) + cols - 1) // cols
    tw = max(t.width for t in tiles)
    th = max(t.height for t in tiles)
    pad = 10
    width = cols * (tw + pad) + pad + (legend.width + pad if legend else 0)
    height = max(rows * (th + pad) + pad + 34, (legend.height + 44) if legend else 0)
    sheet = Image.new("RGB", (width, height), BG)
    ImageDraw.Draw(sheet).text((pad, 8), title, fill=INK, font=font(18))
    for n, t in enumerate(tiles):
        r, c = divmod(n, cols)
        sheet.paste(t, (pad + c * (tw + pad), 34 + pad + r * (th + pad)))
    if legend:
        sheet.paste(legend, (cols * (tw + pad) + pad, 44))
    return sheet


def legend_image(table: dict, present: set[int]) -> Image.Image:
    rows = [r for r in table["labels"] if r["id"] in present and r["id"] != 0]
    f = font(14)
    im = Image.new("RGB", (330, 20 * len(rows) + 8), BG)
    d = ImageDraw.Draw(im)
    for n, r in enumerate(rows):
        c = r["colour"].lstrip("#")
        rgb = tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
        d.rectangle([4, 4 + 20 * n, 20, 18 + 20 * n], fill=rgb, outline=(90, 90, 90))
        d.text((28, 3 + 20 * n), f"{r['id']:2d} {r['key']}", fill=INK, font=f)
    return im


def montages(labels: np.ndarray, header: dict, table: dict, prefix: str, scale: int):
    lut = palette(table)
    ny, nz, nx = labels.shape
    xs, ys, zs = kvox.voxel_centres(header)
    present = set(np.unique(labels).tolist())
    legend = legend_image(table, present)
    inside = labels != 0
    yrows = np.flatnonzero(inside.any(axis=(1, 2)))
    # axial: viewed from below (figure's left on the right, front at the top)
    levels = header.get("landmarks", {}).get("vertebral_levels_m", {})

    def level(y):
        for name, (lo, hi) in levels.items():
            if lo <= y < hi:
                return " (" + name + ")"
        return ""
    picks = np.linspace(yrows.min() + 4, yrows.max() - 3, 16).round().astype(int)
    tiles = []
    for j in picks[::-1]:
        sl = labels[j][::-1, :]                    # z down the image: front at the top
        tiles.append(tile(lut[sl], scale, f"axial y={ys[j]:.3f} m{level(ys[j])}"))
    out = {}
    out["axial"] = grid_sheet(tiles, 4, "axial slices, head to feet (from below: front up, figure's left on the right)", legend)
    # coronal: viewed from the front (figure's left on the right of the image)
    zcols = np.flatnonzero(inside.any(axis=(0, 2)))
    picks = np.linspace(zcols.min() + 2, zcols.max() - 2, 8).round().astype(int)
    tiles = []
    for k in picks:
        sl = labels[:, k, :][::-1, :]
        tiles.append(tile(lut[sl], max(2, scale - 2), f"coronal z={zs[k]:+.3f} m"))
    out["coronal"] = grid_sheet(tiles, 4, "coronal slices, back to front (from the front: figure's left on the right)", legend)
    # sagittal: viewed from the figure's right (front on the right of the image)
    xcols = np.flatnonzero(inside.any(axis=(0, 1)))
    mid = header.get("landmarks", {}).get("midline_x_m", float(xs[xcols].mean()))
    want = [mid - 0.24, mid - 0.17, mid - 0.10, mid - 0.06, mid - 0.03, mid, mid + 0.03, mid + 0.06, mid + 0.10, mid + 0.17]
    tiles = []
    for x in want:
        i = int(np.argmin(np.abs(xs - x)))
        sl = labels[:, :, i][::-1, :]
        tiles.append(tile(lut[sl], max(2, scale - 2), f"x={xs[i]:+.3f}"))
    out["sagittal"] = grid_sheet(tiles, 10, "sagittal slices, figure's right to left (from its right: front on the right)", legend)
    for name, im in out.items():
        save(im, f"{prefix}_{name}.png")


def cutaway(labels: np.ndarray, header: dict, table: dict, prefix: str, px: int, z_cut: float = -0.008,
            variant: str = "left"):
    """Oblique front view of the voxels (painter's algorithm, exposed faces only). "left": the front
    of the figure's left half is removed down to the coronal plane z = z_cut, so the organs,
    skeleton and brain show beside the intact skin of its right half. "front": the front of the
    head and trunk (above the crotch, between the arms) is removed on both sides. Depth recedes up
    and to the right; the visible faces are the front (+Z), the top (+Y) and the figure's left (+X)."""
    lut = palette(table).astype(np.float64)
    ny, nz, nx = labels.shape
    xs, ys, zs = kvox.voxel_centres(header)
    mid = header.get("landmarks", {}).get("midline_x_m", 0.0)
    keep = labels != 0
    if variant == "left":
        cut = (zs[None, :, None] > z_cut) & (xs[None, None, :] > mid)
        what = "the front of the figure's left half removed"
    else:
        crotch = header.get("landmarks", {}).get("crotch_y_m", 0.7)
        cut = ((zs[None, :, None] > z_cut) & (ys[:, None, None] > crotch + 0.02)
               & (np.abs(xs[None, None, :] - mid) < 0.15))
        what = "the front of the head and trunk removed"
    keep &= ~cut
    p = np.pad(keep, 1)
    open_front = keep & ~p[1:-1, 2:, 1:-1]   # +k neighbour empty
    open_top = keep & ~p[2:, 1:-1, 1:-1]     # +j neighbour empty
    open_side = keep & ~p[1:-1, 1:-1, 2:]    # +i neighbour empty
    vis = open_front | open_top | open_side
    jj, kk, ii = np.nonzero(vis)
    order = np.lexsort((ii, jj, kk))          # back (small k) first, then bottom to top, left to right
    s = px
    a, b = 0.45, 0.30                          # receding depth: right and up per voxel of depth

    def P(x, y, d):
        return (x * s + d * s * a, (ny - y) * s - d * s * b)

    w = int((nx + nz * a + 4) * s)
    h = int((ny + nz * b + 4) * s)
    im = Image.new("RGB", (w, h), BG)
    draw = ImageDraw.Draw(im)
    off = (2 * s, (nz * b + 2) * s)
    for n in order:
        i, j, k = int(ii[n]), int(jj[n]), int(kk[n])
        d = nz - k - 1
        col = lut[labels[j, k, i]]
        faces = []
        if open_side[j, k, i]:
            faces.append(([(i + 1, j, d), (i + 1, j, d + 1), (i + 1, j + 1, d + 1), (i + 1, j + 1, d)], 0.62))
        if open_top[j, k, i]:
            faces.append(([(i, j + 1, d), (i + 1, j + 1, d), (i + 1, j + 1, d + 1), (i, j + 1, d + 1)], 0.82))
        if open_front[j, k, i]:
            faces.append(([(i, j, d), (i + 1, j, d), (i + 1, j + 1, d), (i, j + 1, d)], 1.0))
        for corners, shade in faces:
            pts = [(P(*c)[0] + off[0], P(*c)[1] + off[1]) for c in corners]
            draw.polygon(pts, fill=tuple((col * shade).astype(int)))
    title = Image.new("RGB", (im.width, 30), BG)
    ImageDraw.Draw(title).text((8, 6), f"{what} (z > {z_cut:+.3f} m)", fill=INK, font=font(15))
    sheet = Image.new("RGB", (im.width, im.height + 30), BG)
    sheet.paste(title, (0, 0))
    sheet.paste(im, (0, 30))
    save(sheet, f"{prefix}_cutaway_{variant}.png")


def save(im: Image.Image, path: str):
    p = Path(path)
    if p.exists():
        raise SystemExit(f"refusing to overwrite {p}")
    p.parent.mkdir(parents=True, exist_ok=True)
    im.save(p, optimize=True)
    print("wrote", p)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("volume")
    ap.add_argument("prefix")
    ap.add_argument("--table", default=str(REPO / "catalog/crew_anatomy_tissues.v1.json"))
    ap.add_argument("--scale", type=int, default=6)
    ap.add_argument("--cutaway-px", type=int, default=5)
    ap.add_argument("--only", choices=["slices", "cutaway"], default=None)
    args = ap.parse_args(argv)
    header, channels = kvox.read(args.volume)
    table = json.loads(Path(args.table).read_text(encoding="utf-8"))
    labels = channels["label"]
    if args.only != "cutaway":
        montages(labels, header, table, args.prefix, args.scale)
    if args.only != "slices":
        cutaway(labels, header, table, args.prefix, args.cutaway_px, variant="left")
        cutaway(labels, header, table, args.prefix, args.cutaway_px, variant="front")
    return 0


if __name__ == "__main__":
    sys.exit(main())
