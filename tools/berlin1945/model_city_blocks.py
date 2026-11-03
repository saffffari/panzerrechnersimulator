"""Generic Berlin city-centre buildings of 1945 for the blocks around the Reichstag.

    blender --background --python tools/berlin1945/model_city_blocks.py -- OUT_DIR [PREVIEW_PREFIX]

The building grammar (src/building_grammar.rs) builds Stalingrad typologies at quarter-turn yaws
only; the Dorotheenstadt's street grid runs at 83.7 deg (Unter den Linden, Dorotheenstrasse,
OSM), so these blocks are modelled shells the map can turn to the street. Three types of the
district (the Linden and Dorotheenstadt fronts in the 1931 Pariser Platz aerial, Bundesarchiv
146-1998-010-21, and Khaldei's May 1945 view down Unter den Linden):
- mietshaus5: a five-storey rendered tenement/commercial house with a shop ground floor;
- palais3: a three-storey stone-faced palais or ministry house with tall storeys;
- office5: a five-storey stone-faced office or hotel building;
each 14 or 24 m along the street, 16 m deep, in three 1945 conditions (bl_city.py): gutted,
collapsed, roofed_damaged. Frame: the street front faces -Y (engine +Z, the grammar's convention),
side walls are blind party walls, origin at the footprint centre at ground level.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bl_kit as K  # noqa: E402
import bl_city as C  # noqa: E402

STYLES = {
    'mietshaus5': dict(storeys=5, storey_h=3.9, ground_h=4.8, bay=3.6, w_frac=0.42, wall_mat='plaster',
                       trim_mat='plaster_dark', shop=True),
    'palais3': dict(storeys=3, storey_h=5.4, ground_h=5.8, bay=4.4, w_frac=0.40, wall_mat='sandstone_light',
                    trim_mat='sandstone', shop=False),
    'office5': dict(storeys=5, storey_h=4.3, ground_h=5.2, bay=3.3, w_frac=0.50, wall_mat='render_grey',
                    trim_mat='sandstone', shop=True),
}
WIDTHS = (14.0, 24.0)
DEPTH = 16.0
CONDS = ('gutted', 'collapsed', 'roofed_damaged')


def variants():
    for style in STYLES:
        for w in WIDTHS:
            for cond in CONDS:
                yield style, w, cond


def build(style, w, cond):
    name = f'berlin_{style}_{int(w)}_{cond}'
    m = K.Model(name, 'tools/berlin1945/model_city_blocks.py')
    st = STYLES[style]
    C.shell(m, -w/2, w/2, -DEPTH/2, DEPTH/2, st['storeys'], cond, name, storey_h=st['storey_h'],
            ground_h=st['ground_h'], bay=st['bay'], w_frac=st['w_frac'], wall_mat=st['wall_mat'],
            trim_mat=st['trim_mat'], faces=('s', 'n'), party=('e', 'w'),
            roof='mansard', shop=st['shop'], collapse_side='s' if K.h01(name) < 0.5 else 'n')
    return m


def main():
    a = K.args()
    out = a[0] if a else 'assets/worlds/berlin1945'
    prev = a[1] if len(a) > 1 else None
    for style, w, cond in variants():
        K.clear_scene()
        m = build(style, w, cond)
        cams = None
        if prev and w == 24.0:
            cams = (f'{prev}_{style}_{cond}', [('front', (-14.0, -40.0, 6.0), (0.0, 0.0, 9.0), 35)])
        m.export(out, cams)


if __name__ == '__main__':
    main()
