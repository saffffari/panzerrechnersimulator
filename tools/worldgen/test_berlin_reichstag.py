#!/usr/bin/env python3
"""Checks on the Berlin 1945 generator's pure parts (no asset root needed).

    python3 tools/worldgen/test_berlin_reichstag.py          (pytest also collects it)
"""
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import berlin_reichstag as B  # noqa: E402


def test_oriented_box_of_a_turned_plot():
    a = math.radians(B.GRID_DEG)
    pts = [B.rot(x, n, B.GRID_DEG) for x, n in ((-12, -8), (12, -8), (12, 8), (-12, 8))]
    cx, cn, L, S, ang = B.obb([(x+100, n-50) for x, n in pts])
    assert abs(cx-100) < 0.5 and abs(cn+50) < 0.5
    assert abs(L-24) < 1.0 and abs(S-16) < 1.0
    assert min(abs(((ang-B.GRID_DEG+90) % 180)-90), 99) < 1.5, ang
    assert a > 0


def test_lots_overlap_by_separating_axes():
    p = B.lot_polygon(0, 0, 20, 16, 0)
    assert B.polygons_overlap(p, B.lot_polygon(10, 0, 20, 16, 0))
    assert not B.polygons_overlap(p, B.lot_polygon(21, 0, 20, 16, 0))
    assert not B.polygons_overlap(p, B.lot_polygon(0, 30, 20, 16, 45))


def test_the_route_passes_round_the_flooded_cut():
    route = B.course_route()
    assert len(route) > 80, 'the course machinery releases platoons only past knot 60'
    for (x, n) in route:
        for (ax, an), (bx, bn) in zip(B.DITCH, B.DITCH[1:]):
            dx, dn = bx-ax, bn-an
            t = max(0.0, min(1.0, ((x-ax)*dx+(n-an)*dn)/(dx*dx+dn*dn)))
            d = math.hypot(x-(ax+t*dx), n-(an+t*dn))
            assert d > B.DITCH_W/2+1.0, (x, n, d)
    assert all(abs(x) < B.HALF and abs(n) < B.HALF for x, n in route)


def test_mission_beats_stay_on_the_route():
    sheet = B.mission_sheet(B.course_route())
    assert [b['id'] for b in sheet['beats']] == ['chaussee', 'reichstag', 'square', 'all_tanks']
    rel = sheet['beats'][2]['pacing']['release']
    assert 0.5 < rel['fraction'] <= 1.0


if __name__ == '__main__':
    for name, fn in list(globals().items()):
        if name.startswith('test_'):
            fn()
            print('ok', name)
