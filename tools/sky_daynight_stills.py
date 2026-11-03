#!/usr/bin/env python3
"""The sky through the day on the live maps: stills by the game's own `screenshot` path in the
owner's locked look (deltap-real, 2,048 colours, 1024x576), the eye converged per still and the
clock held at each instant (KESSEL_DAY_NIGHT=0, KESSEL_SKY_UTC). Times are chosen by the Sun's
altitude (a low-precision solar position, ample for choosing a minute); the camera's aim at a body
(KESSEL_SHOT_AIM) uses the engine's own ephemeris.

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/sky_daynight_stills.py TAG [--only day,sunset,...] [--map canyon|bridge|both] [--checkout DIR]
Sets: day (a fixed view through the afternoon and the night), sunset, sunrise, shadow (the Earth's
shadow, the antisolar sky after sunset), night (stars overhead), moon (the Moon's phases).
Outputs: /home/alex/panzer/captures/<checkout>-<TAG>_<map>_<set>_<label>.jpg; a receipt sky-daynight-<TAG>_receipt.json
"""
import argparse
import datetime as dt
import json
import math
import os
import subprocess
import time
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
ASSET_ROOT = Path(os.environ.get('KESSEL_ASSET_ROOT', '/windows/shop/panzerrechnersystem'))
CAPTURES = Path('/home/alex/panzer/captures')
SITE = (48.708, 44.513)  # act 1 (Stalingrad): both maps are act-1 workshops

DEG = math.pi / 180


def julian(t):
    return t.timestamp() / 86400.0 + 2440587.5


def sun_altaz(t, lat=SITE[0], lon=SITE[1]):
    """Low-precision Sun (Meeus ch. 25 truncated), degrees: altitude, compass azimuth."""
    jd = julian(t)
    n = jd - 2451545.0
    L = (280.460 + 0.9856474 * n) % 360
    g = (357.528 + 0.9856003 * n) % 360 * DEG
    lam = (L + 1.915 * math.sin(g) + 0.020 * math.sin(2 * g)) * DEG
    eps = (23.439 - 4e-7 * n) * DEG
    ra = math.atan2(math.cos(eps) * math.sin(lam), math.cos(lam))
    dec = math.asin(math.sin(eps) * math.sin(lam))
    return altaz(jd, ra, dec, lat, lon)


def altaz(jd, ra, dec, lat, lon):
    gmst = (280.46061837 + 360.98564736629 * (jd - 2451545.0)) % 360
    h = (gmst + lon) * DEG - ra
    la = lat * DEG
    alt = math.asin(math.sin(la) * math.sin(dec) + math.cos(la) * math.cos(dec) * math.cos(h))
    az = math.atan2(-math.cos(dec) * math.sin(h), math.sin(dec) * math.cos(la) - math.cos(dec) * math.sin(la) * math.cos(h))
    return alt / DEG, (az / DEG) % 360


def moon_altaz(t, lat=SITE[0], lon=SITE[1]):
    """Low-precision Moon (Schlyter's elements with the main perturbations), degrees; and its lit fraction."""
    d = julian(t) - 2451543.5
    N = (125.1228 - 0.0529538083 * d) * DEG
    i = 5.1454 * DEG
    w = (318.0634 + 0.1643573223 * d) * DEG
    e = 0.054900
    M = (115.3654 + 13.0649929509 * d) % 360 * DEG
    E = M + e * math.sin(M) * (1 + e * math.cos(M))
    for _ in range(5):
        E = E - (E - e * math.sin(E) - M) / (1 - e * math.cos(E))
    xv, yv = 60.2666 * (math.cos(E) - e), 60.2666 * math.sqrt(1 - e * e) * math.sin(E)
    v, r = math.atan2(yv, xv), math.hypot(xv, yv)
    lon_ecl = math.atan2(math.sin(N) * math.cos(v + w) * 1 + math.cos(N) * math.sin(v + w) * math.cos(i),
                         math.cos(N) * math.cos(v + w) - math.sin(N) * math.sin(v + w) * math.cos(i))
    lat_ecl = math.asin(math.sin(v + w) * math.sin(i))
    Ms = (356.0470 + 0.9856002585 * d) % 360 * DEG
    ws = (282.9404 + 4.70935e-5 * d) * DEG
    Ls, Lm = Ms + ws, M + w + N
    D, F = Lm - Ls, Lm - N
    lon_ecl += (-1.274 * math.sin(M - 2 * D) + 0.658 * math.sin(2 * D) - 0.186 * math.sin(Ms)
                - 0.059 * math.sin(2 * M - 2 * D) - 0.057 * math.sin(M - 2 * D + Ms) + 0.053 * math.sin(M + 2 * D)) * DEG
    lat_ecl += (-0.173 * math.sin(F - 2 * D)) * DEG
    eps = 23.4393 * DEG
    x, y, z = math.cos(lon_ecl) * math.cos(lat_ecl), math.sin(lon_ecl) * math.cos(lat_ecl), math.sin(lat_ecl)
    ye, ze = y * math.cos(eps) - z * math.sin(eps), y * math.sin(eps) + z * math.cos(eps)
    ra, dec = math.atan2(ye, x), math.asin(ze)
    alt, az = altaz(julian(t), ra, dec, lat, lon)
    alt -= math.degrees(math.asin(1 / r)) * math.cos(alt * DEG)  # topocentric parallax
    elong = math.acos(math.cos(lon_ecl - Ls) * math.cos(lat_ecl))
    return alt, az, 0.5 * (1 - math.cos(elong))


def when_sun_at(day, altitude, evening=True):
    """First minute of `day` (UTC midnight) after the Sun's transit (or before it) at `altitude`."""
    minutes = [day + dt.timedelta(minutes=m) for m in range(24 * 60)]
    alts = [sun_altaz(t)[0] for t in minutes]
    noon = int(np.argmax(alts))
    rng = range(noon, len(minutes)) if evening else range(noon, -1, -1)
    for m in rng:
        if alts[m] <= altitude:
            return minutes[m]
    return None


def iso(t):
    return t.strftime('%Y-%m-%dT%H:%M:00Z')


class Map:
    def __init__(self, name, workshop, date):
        self.name, self.date = name, date
        self.ws = ASSET_ROOT / 'assets/worldgen/workshops' / workshop
        m = json.loads((self.ws / 'manifest.v1.json').read_text())
        self.manifest = m
        self.edits = json.loads((self.ws / 'manifest_edits.json').read_text())
        self.H = np.load(self.ws / 'heights.npy')
        self.res, self.sp = m['terrain']['res'], m['terrain']['spacing']
        self.half = (self.res - 1) * self.sp / 2

    def ground(self, x, z):
        gx, gz = (x + self.half) / self.sp, (z + self.half) / self.sp
        ix, iz = int(np.clip(gx, 0, self.res - 2)), int(np.clip(gz, 0, self.res - 2))
        tx, tz = gx - ix, gz - iz
        H = self.H
        return float(H[iz, ix] * (1 - tx) * (1 - tz) + H[iz, ix + 1] * tx * (1 - tz) + H[iz + 1, ix] * (1 - tx) * tz + H[iz + 1, ix + 1] * tx * tz)


def views(mp):
    """A standing viewpoint on the map and a fixed bearing for the day sequence."""
    if mp.name == 'bridge':
        c = mp.manifest['crossings'][0]
        cx, cz = c['center']
        x, z = cx - 420, cz - 60
        # From the field west of the bridge toward the town: east, where the Earth's shadow rises.
        return (x, mp.ground(x, z) + 1.7, z), 80.0
    route = np.array(mp.edits['canyon_course']['route'])
    j = round(0.14 * (len(route) - 1))
    p, q = route[j], route[min(j + 15, len(route) - 1)]
    x, z = float(p[0]), float(p[1])
    bearing = math.degrees(math.atan2(q[0] - p[0], -(q[1] - p[1]))) % 360
    return (x, mp.ground(x, z) + 2.7, z), bearing


# The owner's live game or editor (2026-11-03: "before any compute use verify the panzer editor or
# game isn't live before starting. if its live that means im using it and don't render").
# Bracketed so the pattern never matches a shell that carries it on its own command line.
OWNER_LIVE = [r'[p]anzerrechnersystem( fea-range| edit)?$', r'[l]aunch_demo']
# A headless GPU run exits with this while the owner's window is open (main.rs window-live guard).
WINDOW_LIVE_EXIT = 75


def owner_live():
    return any(subprocess.run(['pgrep', '-f', pattern], capture_output=True).returncode == 0 for pattern in OWNER_LIVE)


def gpu_busy():
    """The owner's game or editor is live, or another game binary is on the GPU (another session's
    render): one GPU job at a time, and none while he uses the game."""
    if owner_live():
        return True
    out = subprocess.run(['pgrep', '-af', 'panzerrechnersystem'], capture_output=True, text=True).stdout
    for line in out.splitlines():
        cmd = line.split(None, 1)[1] if ' ' in line else ''
        exe = cmd.split()[0] if cmd else ''
        if exe.endswith('/panzerrechnersystem') or exe == './target/release/panzerrechnersystem' or 'pz_range_bin' in exe:
            return True
    return False


def wait_for_gpu():
    while gpu_busy():
        time.sleep(5)


def shoot(mp, tag, set_name, label, utc, eye, yaw, pitch, aim=None, fov=None, extra=None):
    # The capture router puts the checkout's name in front (sky-daynight-..., sky-daynight-base-...).
    out = f'{tag}_{mp.name}_{set_name}_{label}.png'
    env = dict(os.environ, KESSEL_RENDERER='raymarch', KESSEL_INTERNAL_RES='1024x576', KESSEL_LOOK='deltap-real',
               KESSEL_REAL_STEPS='15,15,7', KESSEL_GRAPHICS_PRESET='high', KESSEL_RECON_MANIFEST=str(mp.ws / 'manifest.v1.json'),
               KESSEL_SHOT_CANYON='1', KESSEL_CANYON_SEED='29', KESSEL_CAPTURES_DIR=str(CAPTURES), KESSEL_CAPTURE_4K_JPG='1',
               KESSEL_ASSET_ROOT=str(ASSET_ROOT), KESSEL_SKY_UTC=utc, KESSEL_DAY_NIGHT='0', KESSEL_TRANSPORT_QUALITY='full',
               KESSEL_DITHER_PATTERN='screen_bayer8', KESSEL_GEOMETRY_AA='msaa4', KESSEL_SOURCE_VISIBILITY='maps')
    if aim:
        env['KESSEL_SHOT_AIM'] = aim
    if fov:
        env['KESSEL_SHOT_FOV'] = str(fov)
    env.update(extra or {})
    ex, ey, ez = eye
    cmd = [str(ROOT / 'target/release/panzerrechnersystem'), 'screenshot', out,
           f'{ex:.2f}', f'{ey:.2f}', f'{ez:.2f}', f'{yaw:.2f}', f'{pitch:.2f}', '0.0']
    while True:
        wait_for_gpu()
        r = subprocess.run(cmd, env=env, cwd=ROOT, capture_output=True, text=True, timeout=1200)
        if r.returncode != WINDOW_LIVE_EXIT:
            break
        print(f'{label}: the owner opened a window; waiting', flush=True)
        time.sleep(30)
    lines = [l for l in r.stdout.splitlines() if l.startswith(('[sky]', '[light] photometry'))]
    record = {'map': mp.name, 'set': set_name, 'label': label, 'utc': utc, 'eye': eye, 'yaw': yaw, 'pitch': pitch,
              'aim': aim, 'fov': fov, 'ok': r.returncode == 0, 'log': lines,
              'sun_alt_approx': round(sun_altaz(dt.datetime.strptime(utc, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=dt.timezone.utc))[0], 2)}
    for l in lines:
        if l.startswith('[light] photometry'):
            try:
                record['photometry'] = json.loads(l.split(' ', 2)[2])
            except Exception:
                pass
    print(f"{mp.name} {set_name} {label} {utc}: {'ok' if record['ok'] else 'FAILED'}", *lines[:2], '' if record['ok'] else r.stderr[-600:], flush=True)
    return record


def main():
    global ROOT
    ap = argparse.ArgumentParser()
    ap.add_argument('tag')
    ap.add_argument('--only')
    ap.add_argument('--map', default='both')
    ap.add_argument('--checkout', default=None, help='the checkout whose release binary renders (a baseline worktree for the before stills)')
    a = ap.parse_args()
    ROOT = Path(a.checkout).resolve() if a.checkout else ROOT
    sets = set(a.only.split(',')) if a.only else {'day', 'sunset', 'sunrise', 'shadow', 'night', 'moon'}
    maps = []
    if a.map in ('both', 'canyon'):
        maps.append(Map('canyon', 'gaea-canyon-run-v2', dt.datetime(1943, 1, 20, tzinfo=dt.timezone.utc)))
    if a.map in ('both', 'bridge'):
        maps.append(Map('bridge', 'spring-bridge-v2', dt.datetime(1943, 5, 12, tzinfo=dt.timezone.utc)))
    records = []
    for mp in maps:
        eye, bearing = views(mp)
        if 'day' in sets:
            for alt in [99, 20, 8, 3, 0, -2, -4, -6, -9, -12, -18]:
                t = when_sun_at(mp.date, min(alt, sun_altaz(when_sun_at(mp.date, 90) or mp.date)[0] - 0.5) if alt == 99 else alt)
                label = 'noon' if alt == 99 else f'sun{alt:+03d}'
                records.append(shoot(mp, a.tag, 'day', label, iso(t), eye, bearing, 6.0))
        if 'sunset' in sets:
            for alt in [6, 2, 0, -2, -5]:
                t = when_sun_at(mp.date, alt)
                records.append(shoot(mp, a.tag, 'sunset', f'sun{alt:+03d}', iso(t), eye, 0, 0, aim='sun:4' if alt >= 0 else 'sun:8'))
        if 'sunrise' in sets:
            for alt in [-5, -2, 0, 3]:
                t = when_sun_at(mp.date, alt, evening=False)
                records.append(shoot(mp, a.tag, 'sunrise', f'sun{alt:+03d}', iso(t), eye, 0, 0, aim='sun:4' if alt >= 0 else 'sun:8'))
        if 'shadow' in sets:
            for alt in [1, -1, -3, -5]:
                t = when_sun_at(mp.date, alt)
                records.append(shoot(mp, a.tag, 'shadow', f'sun{alt:+03d}', iso(t), eye, 0, 0, aim='antisun:8'))
        if mp.name != 'bridge':
            continue  # the canyon's deck (overcast 0.98) hides the night sky; the bridge's is clear
        if 'night' in sets:
            # The deep night of the map's date, looking up at 50 deg to the north and to the south.
            t = when_sun_at(mp.date, -18) + dt.timedelta(minutes=50)
            for label, yaw in [('north', 0.0), ('south', 180.0)]:
                records.append(shoot(mp, a.tag, 'night', label, iso(t), eye, yaw, 50.0))
        if 'moon' in sets:
            # The Moon through its month: a dark-sky instant with the Moon well up on each of a run
            # of nights (the real month of May 1943; the ten-day month of play draws by the same law).
            for day in range(3, 33, 3):
                base = mp.date.replace(day=1) + dt.timedelta(days=day)
                best = None
                for m in range(0, 24 * 60, 10):
                    t = base + dt.timedelta(minutes=m)
                    s_alt = sun_altaz(t)[0]
                    m_alt, _, lit = moon_altaz(t)
                    if s_alt < -6 and m_alt > 12 and (best is None or m_alt > best[1]):
                        best = (t, m_alt, lit)
                if best is None:
                    continue
                t, m_alt, lit = best
                records.append(shoot(mp, a.tag, 'moon', f'{t:%m%d}_lit{int(round(lit * 100)):03d}', iso(t), eye, 0, 0, aim='moon:6'))
    receipt = CAPTURES / f'sky-daynight-{a.tag}_receipt.json'
    old = json.loads(receipt.read_text())['shots'] if receipt.exists() else []
    keys = {(r['map'], r['set'], r['label']) for r in records}
    merged = [r for r in old if (r['map'], r['set'], r['label']) not in keys] + records
    receipt.write_text(json.dumps({'tag': a.tag, 'binary': str(ROOT / 'target/release/panzerrechnersystem'), 'shots': merged}, indent=1) + '\n')
    print('receipt', receipt)


if __name__ == '__main__':
    main()
