#!/usr/bin/env python3
"""make_ground_tex.py — stage 1.5: high-res ground textures for the act .blends.

The Sentinel-2 drape is 10 m/px — georeferenced and correct, but mush at tank range. This
compositor keeps the satellite as the LOW-FREQUENCY color (field boundaries, roads, villages stay
exactly where they are) and multiplies in HIGH-FREQUENCY grain from the PSX Textures pack, chosen
per ground class from gen_world's imagery classification (grass on pasture, plough on bare fields,
forest floor under canopy, smooth water, snow variants for the winter act). The PSX tiles are
authored near-black (kessel-dark-psx-albedos), so each tile is normalized to its own mean and used
as a RATIO field — only its texture survives, never its exposure.

Output: assets/terrain/act<N>/act<N>_ground_hires.png (default 4096², ~1.3 m/px), consumed by
gen_world.py ACTS[n]["texture"] → assemble_blend.py drape. Same window as the DEM: UVs unchanged.

Usage: python3 tools/worldgen/make_ground_tex.py [act] [size]
Needs: numpy + the gdal CLI (dev-box tools, same as the window-crop prep).
"""
import math
import os
import subprocess
import sys
import tempfile

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_world as gw

TEX = "assets/pizza/_extracted/PSX Textures v3.1/PSX Textures/256/Color Maps/"

# class → detail tile. Order matters below (later = higher priority at a pixel).
ACT_TILES = {
    "spring": {
        "bare": "dirt_5.png",
        "greenfld": "grass_7.png",
        "darkcrop": "grass_10.png",
        "woods": "dirt_2.png",       # needle/leaf-litter floor under the canopy
        "water": None,               # keep the satellite's smooth dark water
    },
    "winter": {
        "bare": "snow_1.png",
        "dark": "concrete_tx_2.png", # urban blocks / cleared roads read dark through snow
        "bright": "snow_2.png",
    },
}


def read_png(path):
    """PNG → HxWx3 uint8 via gdal ENVI (no PIL on this box). Parses the .hdr properly."""
    with tempfile.TemporaryDirectory() as td:
        raw = os.path.join(td, "t.bin")
        subprocess.run(["gdal_translate", "-q", "-of", "ENVI", path, raw], check=True)
        hdr = {}
        for line in open(raw[:-4] + ".hdr"):
            if "=" in line:
                k, v = line.split("=", 1)
                hdr[k.strip()] = v.strip()
        W, H, B = int(hdr["samples"]), int(hdr["lines"]), int(hdr["bands"])
        inter = hdr.get("interleave", "bsq").lower()
        a = np.fromfile(raw, dtype=np.uint8)
        if inter == "bsq":
            img = a.reshape(B, H, W).transpose(1, 2, 0)
        elif inter == "bil":
            img = a.reshape(H, B, W).transpose(0, 2, 1)
        else:  # bip
            img = a.reshape(H, W, B)
    if img.shape[2] == 1:
        img = np.repeat(img, 3, axis=2)
    return np.ascontiguousarray(img[..., :3])


def write_png(img, path):
    with tempfile.TemporaryDirectory() as td:
        raw = os.path.join(td, "o.bin")
        np.ascontiguousarray(img.transpose(2, 0, 1)).tofile(raw)
        open(raw[:-4] + ".hdr", "w").write(
            f"ENVI\nsamples = {img.shape[1]}\nlines = {img.shape[0]}\nbands = 3\n"
            "header offset = 0\nfile type = ENVI Standard\ndata type = 1\n"
            "interleave = bsq\nbyte order = 0\n")
        subprocess.run(["gdal_translate", "-q", "-of", "PNG", raw, path], check=True)


def ratio_field(tile, out_size):
    """Tile a 256² detail texture across the output at 1 texel = 1 output pixel, normalized to
    its per-channel mean → a grain field around 1.0. Clamped so no texel over/under-drives."""
    t = tile.astype(np.float32)
    mean = t.reshape(-1, 3).mean(0)
    r = t / np.maximum(mean, 1.0)
    reps = out_size // t.shape[0] + 1
    field = np.tile(r, (reps, reps, 1))[:out_size, :out_size]
    return np.clip(field, 0.62, 1.65)  # tight: grain, not blotch — the crush dither adds the rest


def upsample_bilinear(img, out_size):
    """HxWxC float bilinear upsample to out_size² (numpy gather; no scipy on this box)."""
    H, W = img.shape[:2]
    g = (np.arange(out_size, dtype=np.float32) + 0.5) / out_size
    gy = g * (H - 1)
    gx = g * (W - 1)
    y0 = np.floor(gy).astype(int).clip(0, H - 2)
    x0 = np.floor(gx).astype(int).clip(0, W - 2)
    fy = (gy - y0)[:, None, None]
    fx = (gx - x0)[None, :, None]
    a = img[y0][:, x0]
    b = img[y0][:, x0 + 1]
    c = img[y0 + 1][:, x0]
    d = img[y0 + 1][:, x0 + 1]
    return a * (1 - fy) * (1 - fx) + b * (1 - fy) * fx + c * fy * (1 - fx) + d * fy * fx


def upsample_nearest_idx(idx, out_size):
    H, W = idx.shape
    gy = ((np.arange(out_size) + 0.5) / out_size * H).astype(int).clip(0, H - 1)
    gx = ((np.arange(out_size) + 0.5) / out_size * W).astype(int).clip(0, W - 1)
    return idx[gy][:, gx]


def build(act_id, out_size):
    act = gw.ACTS[act_id]
    tci_png = {1: "assets/terrain/act1_rgb/act1_window_tci.png",
               2: "assets/terrain/act2/act2_window_tci.png",
               3: "assets/terrain/act3/act3_window_tci.png"}[act_id]
    sat = read_png(tci_png).astype(np.float32)

    if act.get("gen") == "spring":
        w = gw.World.__new__(gw.World)
        w.act = act
        w.seed = act["seed"]
        m = w.classify_imagery()
        # priority stack: bare < green < darkcrop < woods < water
        idx = np.zeros(m["greenfld"].shape, np.uint8)          # 0 bare
        idx[m["greenfld"]] = 1
        idx[m["darkcrop"]] = 2
        idx[m["woods"]] = 3
        idx[m["water"]] = 4
        tiles = ACT_TILES["spring"]
        order = ["bare", "greenfld", "darkcrop", "woods", "water"]
    else:  # winter act 1: classify on brightness alone
        v = sat.max(2)
        idx = np.ones(v.shape, np.uint8)                       # 1 = snow (default)
        idx[v < 95] = 0                                        # 0 = dark urban/roads
        idx[v > 170] = 2                                       # 2 = bright open snowfield
        tiles = ACT_TILES["winter"]
        order = ["dark", "bare", "bright"]

    if idx.shape[0] != sat.shape[0]:  # classifier ran on the npy; sizes match the png anyway
        raise SystemExit("mask/sat size mismatch")

    print(f"[gtex] act {act_id}: classes " +
          ", ".join(f"{k}={float((idx == i).mean()) * 100:.0f}%" for i, k in enumerate(order)))

    sat_up = upsample_bilinear(sat, out_size)
    idx_up = upsample_nearest_idx(idx, out_size)
    out = sat_up.copy()
    for i, key in enumerate(order):
        name = tiles.get(key)
        if name is None:
            continue  # class keeps the plain satellite color (water)
        tile = read_png(TEX + name)
        field = ratio_field(tile, out_size)
        mask = idx_up == i
        out[mask] = sat_up[mask] * field[mask]

    # ── stamp worldgen features (features.json from gen_world): dirt roads + crater scorch ──
    he = act["half_extent"]
    feat_path = f"assets/worldgen/act{act_id}/features.json"
    if os.path.exists(feat_path):
        import json
        feat = json.load(open(feat_path))
        S = out_size
        px_per_m = (S - 1) / (2 * he)

        def to_px(x, z):
            return (x + he) * px_per_m, (z + he) * px_per_m

        dirt = ratio_field(read_png(TEX + "dirt_5.png"), out_size)
        town_pts = feat.get("town_pts", [])
        for (x0, z0, x1, z1) in feat.get("roads", []):
            ax_, ay_ = to_px(x0, z0)
            bx_, by_ = to_px(x1, z1)
            w_px = 3.4 * px_per_m
            lo_x = int(max(0, min(ax_, bx_) - w_px - 2)); hi_x = int(min(S, max(ax_, bx_) + w_px + 2))
            lo_y = int(max(0, min(ay_, by_) - w_px - 2)); hi_y = int(min(S, max(ay_, by_) + w_px + 2))
            if hi_x <= lo_x or hi_y <= lo_y:
                continue
            gx = np.arange(lo_x, hi_x, dtype=np.float32)[None, :]
            gy = np.arange(lo_y, hi_y, dtype=np.float32)[:, None]
            dx, dy = bx_ - ax_, by_ - ay_
            L2 = max(dx * dx + dy * dy, 1.0)
            t = np.clip(((gx - ax_) * dx + (gy - ay_) * dy) / L2, 0.0, 1.0)
            dist = np.sqrt((gx - (ax_ + t * dx)) ** 2 + (gy - (ay_ + t * dy)) ** 2)
            w = np.clip(1.0 - dist / w_px, 0.0, 1.0)[..., None] ** 0.8
            mid = ((x0 + x1) / 2, (z0 + z1) / 2)
            in_town = any((mid[0] - tx) ** 2 + (mid[1] - tz) ** 2 < 140.0 ** 2 for tx, tz in town_pts)
            col = np.array([112.0, 110.0, 106.0] if in_town else [132.0, 116.0, 90.0], np.float32)
            sub = out[lo_y:hi_y, lo_x:hi_x].astype(np.float32)
            road_pix = col * dirt[lo_y:hi_y, lo_x:hi_x]
            out[lo_y:hi_y, lo_x:hi_x] = np.clip(sub * (1 - w) + road_pix * w, 0, 255).astype(np.float32)
        for (cx_, cz_, cr, cd) in feat.get("craters", []):
            px_, py_ = to_px(cx_, cz_)
            r_px = max(cr * px_per_m, 2.0)
            lo_x = int(max(0, px_ - r_px * 2)); hi_x = int(min(S, px_ + r_px * 2))
            lo_y = int(max(0, py_ - r_px * 2)); hi_y = int(min(S, py_ + r_px * 2))
            if hi_x <= lo_x or hi_y <= lo_y:
                continue
            gx = np.arange(lo_x, hi_x, dtype=np.float32)[None, :]
            gy = np.arange(lo_y, hi_y, dtype=np.float32)[:, None]
            q = np.sqrt((gx - px_) ** 2 + (gy - py_) ** 2) / r_px
            scorch = (np.exp(-q * q * 1.6) * 0.62)[..., None]   # burnt bowl
            ring = (np.exp(-(q - 1.25) ** 2 * 3.0) * 0.22)[..., None]  # thrown-earth ring
            sub = out[lo_y:hi_y, lo_x:hi_x].astype(np.float32)
            sub = sub * (1 - scorch) + sub * 0.30 * scorch      # darken bowl
            sub = sub * (1 - ring) + np.array([124, 104, 82], np.float32) * ring
            out[lo_y:hi_y, lo_x:hi_x] = np.clip(sub, 0, 255)
        print(f"[gtex] stamped {len(feat.get('roads', []))} roads, {len(feat.get('craters', []))} craters")
    else:
        print(f"[gtex] no {feat_path} — run gen_world first for road/crater stamps")

    out = np.clip(out, 0, 255).astype(np.uint8)
    dst = os.path.join(os.path.dirname(tci_png), f"act{act_id}_ground_hires.png")
    write_png(out, dst)
    print(f"[gtex] {out_size}² ({act['half_extent'] * 2 / out_size:.2f} m/px) → {dst}")


if __name__ == "__main__":
    act_id = int(sys.argv[1]) if len(sys.argv) > 1 else 2
    size = int(sys.argv[2]) if len(sys.argv) > 2 else 4096
    build(act_id, size)
