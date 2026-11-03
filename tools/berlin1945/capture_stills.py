#!/usr/bin/env python3
"""Headless engine stills of the Berlin 1945 map from the viewpoints of the reference photographs,
each set beside a panel naming the photograph and its viewpoint (the photographs themselves are
not copied: their Wikimedia Commons pages are linked in docs/BERLIN_REICHSTAG_1945.md).

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/berlin1945/capture_stills.py \
        [--shots a,b] [--out DIR] [--tag TAG] [--res 1280x720] [--raw]

Writes <out>/berlin1945_<tag>_<shot>.png (the engine frame) and, unless --raw,
<out>/berlin1945_<tag>_<shot>_compare.jpg (the frame beside its reference description).
The camera stands where the photographer stood, in the map's frame (x east, n north of the
Reichstag's centre; the engine's z is -n), at eye or roof height.
"""
import argparse
import json
import math
import os
import subprocess
import sys
import textwrap
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
WORKSHOP = 'assets/worldgen/workshops/berlin-reichstag-v1/manifest.v1.json'
GATE = (108.8, -258.2)

# name: (eye x, eye height, eye n), (target x, target height, target n), reference text
SHOTS = {
    'reichstag_west_bu8573': ((-120.0, 1.7, -10.0), (-46.0, 20.0, 0.0),
        'IWM BU 8573, Sgt Hewitt, No 5 AFPU, 3 June 1945 (public domain). From Koenigsplatz at ground level, a little '
        'south of the axis, looking east at the west portico: six Corinthian columns, the pediment, "DEM  EUTSCHEN  '
        'OLKE" with two letters shot away, the Germania group above, the dome\'s bare ribs, windows bricked up with '
        'firing slits, brick blast walls before the portico.'),
    'reichstag_wnw_khaldei': ((-175.0, 1.6, 95.0), (-20.0, 16.0, 5.0),
        'Yevgeny Khaldei, May 1945 (public domain): the west front from the rubble of Koenigsplatz, west-north-west '
        'of the building, looking east-south-east. The north-west tower large at left (flat-topped since the 1941 '
        'flak conversion), the portico centre-right, the south-west tower at right, the dome frame behind.'),
    'reichstag_south_bu8922': ((66.0, 2.0, -168.0), (0.0, 24.0, 0.0),
        'IWM BU 8922, Capt Malindine, AFPU, 12 July 1945 (public domain): the south front with both south towers '
        '(flat-topped) and the dome frame, seen from the south across a wide paved road with tram tracks near the '
        'Brandenburg Gate.'),
    'gate_west_thurgood': ((10.0, 1.7, -268.0), GATE[0:1]+(11.0,)+GATE[1:2],
        'Sgt Malcolm Thurgood, Alsos Mission, 1945 (AIP Emilio Segre Visual Archives, CC0): the gate\'s west face '
        'from the Platz vor dem Brandenburger Tor at the end of the Charlottenburger Chaussee, looking east: six '
        'Doric columns, the attic with a shell hole, the wrecked Quadriga, the gatehouses\' colonnades at both sides.'),
    'gate_reichstag_from_south_thurgood': ((88.0, 1.7, -400.0), (60.0, 14.0, -150.0),
        'Thurgood, 1945 (CC0): from the south along Hermann-Goering-Strasse looking north: the gate and its south '
        'gatehouse at right, the Reichstag\'s south front with towers and dome frame at left in the distance.'),
    'gate_east_donath331': ((205.0, 14.0, -312.0), (105.0, 12.0, -240.0),
        'Otto Donath, 1945, Bundesarchiv Bild 183-M1015-331 (CC BY-SA 3.0 de): an elevated view from Pariser Platz, '
        'south-east of the gate, looking north-west: the attic and frieze, the wrecked Quadriga, the south '
        'gatehouse\'s pediment (left), the north gatehouse\'s colonnade (right), the Reichstag\'s south front beyond.'),
    'kroll_donath322': ((-430.0, 1.7, 55.0), (-500.0, 10.0, 10.0),
        'Otto Donath, 1945, Bundesarchiv Bild 183-M1015-322 (CC BY-SA 3.0 de): a wrecked heavy flak gun on its '
        'cruciform platform before the gutted Kroll Opera, shattered trees, the stage house gable with its arch.'),
    'aerial_west_awm': ((520.0, 220.0, -420.0), (-120.0, 0.0, -60.0),
        'Australian War Memorial P04054.006, 1945 (public domain): an aerial view looking west over central Berlin: '
        'the Brandenburg Gate and the ruined Pariser Platz in the foreground, the burnt-out Reichstag beyond, the '
        'Spree and the Lehrter Bahnhof in the distance.'),
    'moltke_bridge': ((-560.0, 3.0, 330.0), (-497.0, -1.0, 365.0),
        'No 1945 photograph of the Moltke Bridge was found (docs/BERLIN_REICHSTAG_1945.md). The bridge from the '
        'Spree\'s south bank west of it, looking east: the barricade at its south end, the blown southern river arch.'),
}


def ground(H, x, n):
    """Bilinear terrain height at map (x, n) on the workshop's grid (centred, square)."""
    res = H.shape[0]
    half = 900.0
    sp = 2*half/(res-1)
    fx, fz = (x+half)/sp, (-n+half)/sp
    i, j = int(min(max(math.floor(fx), 0), res-2)), int(min(max(math.floor(fz), 0), res-2))
    tx, tz = fx-i, fz-j
    return float(H[j, i]*(1-tx)*(1-tz)+H[j, i+1]*tx*(1-tz)+H[j+1, i]*(1-tx)*tz+H[j+1, i+1]*tx*tz)


def angles(eye, target):
    """Engine recon yaw/pitch (yaw 0 looks along -Z, north; 90 along +X, east)."""
    dx, dy, dz = target[0]-eye[0], target[1]-eye[1], target[2]-eye[2]
    yaw = math.degrees(math.atan2(dx, -dz))
    pitch = math.degrees(math.atan2(dy, math.hypot(dx, dz)))
    return yaw, pitch


def panel(text, w, h, title):
    im = Image.new('RGB', (w, h), (22, 22, 22))
    d = ImageDraw.Draw(im)
    try:
        font = ImageFont.truetype('/usr/share/fonts/TTF/DejaVuSans.ttf', max(18, w//42))
        bold = ImageFont.truetype('/usr/share/fonts/TTF/DejaVuSans-Bold.ttf', max(20, w//36))
    except OSError:
        font = bold = ImageFont.load_default()
    y = 30
    d.text((30, y), title, fill=(235, 235, 235), font=bold)
    y += int(bold.size*2.0) if hasattr(bold, 'size') else 40
    for line in textwrap.wrap(text, width=max(int(w/(font.size*0.55)), 30) if hasattr(font, 'size') else 60):
        d.text((30, y), line, fill=(200, 200, 200), font=font)
        y += int(font.size*1.4) if hasattr(font, 'size') else 16
    return im


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--shots')
    ap.add_argument('--out', type=Path, default=Path('/home/alex/panzer/captures'))
    ap.add_argument('--tag', default='v1')
    ap.add_argument('--res', default='1280x720')
    ap.add_argument('--fog', default='0.0012')
    ap.add_argument('--raw', action='store_true')
    ap.add_argument('--bin', type=Path, default=ROOT/'target/release/panzerrechnersystem')
    a = ap.parse_args()
    root = Path(os.environ.get('KESSEL_ASSET_ROOT', '/windows/shop/panzerrechnersystem'))
    a.out.mkdir(parents=True, exist_ok=True)
    names = a.shots.split(',') if a.shots else list(SHOTS)
    heights = np.load(root/WORKSHOP.replace('manifest.v1.json', 'heights.npy'))
    for name in names:
        eye, target, text = SHOTS[name]
        # the map's (x, height, n) to the engine's (x, y, z)
        # heights are above the ground at the eye and at the target
        e = (eye[0], ground(heights, eye[0], eye[2])+eye[1], -eye[2])
        t = (target[0], ground(heights, target[0], target[2])+target[1], -target[2])
        yaw, pitch = angles(e, t)
        out = a.out/f'berlin1945_{a.tag}_{name}.png'
        env = dict(os.environ, KESSEL_ASSET_ROOT=str(root), KESSEL_RECON_MANIFEST=str(root/WORKSHOP),
                   KESSEL_SHOT_CANYON='1', KESSEL_INTERNAL_RES=a.res,
                   # seed 19: the run's clock offset is zero (src/canyon.rs variation), so every still is at
                   # the authored 1945-04-30T15:30Z
                   KESSEL_CANYON_SEED='19', XDG_RUNTIME_DIR='/run/user/1000')
        env.pop('KESSEL_CAPTURES_DIR', None)
        env.pop('KESSEL_CAPTURE_4K_JPG', None)
        cmd = ['nice', '-n', '10', str(a.bin), 'screenshot', str(out), str(e[0]), str(e[1]), str(e[2]),
               f'{yaw:.2f}', f'{pitch:.2f}', '0.00005' if name.startswith('aerial') else a.fog]
        log = out.with_suffix('.log')
        with log.open('w') as f:
            r = subprocess.run(cmd, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT)
        print(json.dumps(dict(shot=name, exit=r.returncode, out=str(out), eye=e, yaw=round(yaw, 2), pitch=round(pitch, 2))))
        if r.returncode != 0 or not out.exists() or a.raw:
            continue
        frame = Image.open(out).convert('RGB')
        W = 3840
        fh = int(frame.height*(W/2)/frame.width)
        left = frame.resize((W//2, fh), Image.NEAREST)
        sheet = Image.new('RGB', (W, fh), (0, 0, 0))
        sheet.paste(left, (0, 0))
        sheet.paste(panel(text, W//2, fh, 'Reference photograph and viewpoint'), (W//2, 0))
        sheet.save(a.out/f'berlin1945_{a.tag}_{name}_compare.jpg', quality=90)


if __name__ == '__main__':
    main()
