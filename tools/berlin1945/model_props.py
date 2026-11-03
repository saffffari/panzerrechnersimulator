"""Props of the Berlin 1945 map the project's libraries lack.

    blender --background --python tools/berlin1945/model_props.py -- OUT_DIR [PREVIEW_PREFIX]

- tree_shattered_a/b/c: a Tiergarten tree broken by shellfire, crown gone, bare stubs of limbs
  (Donath's 1945 photographs at the Kroll and In den Zelten, Bundesarchiv 183-M1015-322/-321).
  Named `tree_*` so the engine fields them as live trees (src/trees.rs).
- flak88_wreck: a heavy flak gun of the 8.8 cm class wrecked on its cruciform platform, as in
  Donath's photograph in front of the Kroll ruin (183-M1015-322, archive caption
  "zerstoertes Flakgeschuetz"); proportions of the 8.8 cm Flak 36/37 (barrel about 4.9 m,
  platform arms about 6.6 m), the barrel high, a trail arm torn up.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bl_kit as K  # noqa: E402

h01 = K.h01


def tree(name, height, lean, limbs):
    m = K.Model(name, 'tools/berlin1945/model_props.py')
    r0 = 0.36
    # the trunk in three tapering drums, leaning a little
    z = 0.0
    x = y = 0.0
    seg = 3
    for k in range(seg):
        z1 = height*(k+1)/seg
        x1, y1 = x+lean*0.4, y+lean*0.15
        r_a, r_b = r0*(1-0.22*k), r0*(1-0.22*(k+1))
        m.beam((x, y, z), (x1, y1, z1), 2*r_a, 2*r_a, 'timber_charred' if k == seg-1 and h01(name) < 0.5 else 'bark')
        x, y, z = x1, y1, z1
    # the splintered top
    for k in range(4):
        a = 2*math.pi*k/4+h01(f'{name}sp{k}')
        m.beam((x, y, z-0.3), (x+0.25*math.cos(a), y+0.25*math.sin(a), z+0.5+0.8*h01(f'{name}sl{k}')), 0.14, 0.14, 'bark')
    # bare limbs, some broken short
    for k in range(limbs):
        zz = height*(0.45+0.45*h01(f'{name}lz{k}'))
        a = 2*math.pi*h01(f'{name}la{k}')
        L = 1.2+2.8*h01(f'{name}ll{k}')
        bx, by = lean*0.4*zz/height*3, 0.0
        m.beam((bx, by, zz), (bx+L*math.cos(a), by+L*math.sin(a), zz+L*0.8), 0.18, 0.18, 'bark')
        if L > 2.5:
            m.beam((bx+L*math.cos(a), by+L*math.sin(a), zz+L*0.8),
                   (bx+L*1.4*math.cos(a+0.4), by+L*1.4*math.sin(a+0.4), zz+L*1.5), 0.1, 0.1, 'bark')
    # no sidecar: a `tree_*` asset is a live tree whose trunk collision and fall the engine owns
    m.no_collision.add('live tree (src/trees.rs)')
    return m


def flak88():
    m = K.Model('flak88_wreck', 'tools/berlin1945/model_props.py')
    grey = 'steel'
    # cruciform platform: two long arms and two folding outriggers, one torn up
    m.box(-3.3, 3.3, -0.3, 0.3, 0.0, 0.45, grey, solid=True)
    m.box(-0.3, 0.3, -3.3, 1.6, 0.0, 0.45, grey, solid=True)
    m.beam((0.0, 1.6, 0.3), (0.4, 3.0, 1.6), 0.55, 0.4, grey)        # the torn-up outrigger
    for x, y in ((-3.2, 0), (3.2, 0), (0, -3.2)):
        m.cyl(x, y, 0.0, 0.25, 0.35, 0.35, grey, seg=8)               # levelling jacks
    # pedestal, the upper carriage and the cradle
    m.cyl(0, 0, 0.45, 1.25, 0.45, 0.4, grey, seg=10, solid=True)
    m.box(-0.8, 0.8, -0.55, 0.55, 1.25, 1.85, grey, solid=True)
    m.box(-0.1, 0.1, -0.7, 0.7, 1.85, 2.4, grey)
    # the barrel high, with the recuperator above it
    el = math.radians(48)
    sx, sz = math.cos(el), math.sin(el)
    p0 = (-1.4*sx, 0.0, 2.2-1.4*sz)
    p1 = (3.5*sx, 0.0, 2.2+3.5*sz)
    m.beam(p0, p1, 0.26, 0.26, grey)
    m.beam((p0[0]+0.1, 0, p0[2]+0.3), (1.5*sx, 0, 2.5+1.5*sz), 0.3, 0.3, grey)
    m.beam((-0.3, 0.45, 1.6), (0.5, 0.45, 2.2), 0.25, 0.25, 'steel_rust')    # equilibrators
    m.beam((-0.3, -0.45, 1.6), (0.5, -0.45, 2.2), 0.25, 0.25, 'steel_rust')
    m.no_collision.add('barrel and recuperator')
    return m


def main():
    a = K.args()
    out = a[0] if a else 'assets/worlds/berlin1945'
    prev = a[1] if len(a) > 1 else None
    for b in (lambda: tree('tree_shattered_a', 7.5, 0.3, 5), lambda: tree('tree_shattered_b', 4.8, -0.2, 3),
              lambda: tree('tree_shattered_c', 10.5, 0.15, 7), flak88):
        K.clear_scene()
        m = b()
        cams = (f'{prev}_{m.name}', [('side', (12.0, -14.0, 3.0), (0.0, 0.0, 2.5), 35)]) if prev else None
        m.export(out, cams)


if __name__ == '__main__':
    main()
