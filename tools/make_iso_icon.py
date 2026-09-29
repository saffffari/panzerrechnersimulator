#!/usr/bin/env python3
"""Regenerate the game icon: a hand-placed 16x16 isometric Tiger I, scaled by whole pixels.

    python3 tools/make_iso_icon.py [--snow] [--install]

Writes assets/ui/panzer_iso_icon.png (16x16) and nearest-neighbour copies at 32, 48, 64, 128,
256 and 512. --snow uses the snow-roofed alternate. --install also puts every size in the user's
hicolor theme as `panzerrechnersystem` (the name the desktop entries use), keeping any older icon
of that name beside it as `*.png.pre-iso`.

16 px is the floor: at 12 px the 88 can no longer clear the bow, and the long gun is what makes
the silhouette a Tiger. Every standard size above it is an exact multiple, so the pixels stay
square. The grid is the source; edit it, not the PNGs. 2:1 isometric, light from the upper left,
hull 4 x 2 steps, turret 2 x 1, the in-game charcoal German grey (sRGB 72, 74, 78) shaded per face.
"""
import pathlib
import shutil
import subprocess
import sys

from PIL import Image

GRID = [
    "................",
    "................",
    "................",
    ".........TT.....",
    ".......TTTHS....",
    "......FTTSSSAA..",
    "......MFSSSAAAC.",
    "....GGAFSAAACCK.",
    "..GGggAAAACCKKK.",
    ".G.gBBAACCKKKwK.",
    ".g.KBBBCKKKwKK..",
    "...KbbKKKwKK....",
    "....bbKwKK......",
    "......KK........",
    "................",
    "................",
]
PALETTE = {
    'A': (118, 122, 128),  # hull deck, lit from above
    'B': (92, 95, 100),    # bow plate, facing the light
    'b': (62, 64, 68),     # lower nose in shade
    'C': (56, 58, 62),     # hull side, away from the light
    'K': (28, 29, 30),     # track
    'w': (84, 86, 89),     # road-wheel rims
    'T': (152, 156, 162),  # turret roof
    'F': (112, 115, 120),  # turret front
    'S': (86, 89, 94),     # turret side
    'M': (40, 42, 45),     # mantlet shadow at the gun root
    'G': (150, 154, 160),  # 88 mm tube and muzzle brake, lit
    'g': (84, 87, 92),     # tube underside and its shadow
    'H': (186, 190, 196),  # cupola
}
SNOW = {'A': (188, 194, 200), 'T': (214, 220, 226), 'H': (236, 240, 244)}
OUTLINE = (14, 15, 17)
SIZES = (32, 48, 64, 128, 256, 512)
ROOT = pathlib.Path(__file__).resolve().parent.parent


def draw(palette):
    image = Image.new('RGBA', (16, 16), (0, 0, 0, 0))
    filled = set()
    for y, row in enumerate(GRID):
        assert len(row) == 16, f'row {y} is {len(row)} wide'
        for x, key in enumerate(row):
            if key != '.':
                image.putpixel((x, y), (*palette[key], 255))
                filled.add((x, y))
    # A one-pixel outline on every side a filled pixel touches empty space.
    for x, y in filled:
        for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
            if 0 <= nx < 16 and 0 <= ny < 16 and (nx, ny) not in filled:
                image.putpixel((nx, ny), (*OUTLINE, 255))
    return image


def main():
    snow = '--snow' in sys.argv
    palette = {**PALETTE, **(SNOW if snow else {})}
    stem = 'panzer_iso_icon_snow' if snow else 'panzer_iso_icon'
    out = ROOT / 'assets' / 'ui'
    out.mkdir(parents=True, exist_ok=True)
    icon = draw(palette)
    icon.save(out / f'{stem}.png')
    written = [out / f'{stem}.png']
    for size in SIZES:
        path = out / f'{stem}_{size}.png'
        icon.resize((size, size), Image.NEAREST).save(path)
        written.append(path)
    print('\n'.join(f'wrote {p}' for p in written))
    if '--install' not in sys.argv:
        return
    theme = pathlib.Path.home() / '.local' / 'share' / 'icons' / 'hicolor'
    for size, source in zip((16,) + SIZES, written):
        target = theme / f'{size}x{size}' / 'apps' / 'panzerrechnersystem.png'
        target.parent.mkdir(parents=True, exist_ok=True)
        keep = target.with_name(target.name + '.pre-iso')
        if target.exists() and not keep.exists():
            shutil.copy2(target, keep)
        shutil.copyfile(source, target)
        print(f'installed {target}')
    for command in (['gtk-update-icon-cache', '-f', '-t', str(theme)], ['kbuildsycoca6']):
        if shutil.which(command[0]):
            subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)


if __name__ == '__main__':
    main()
