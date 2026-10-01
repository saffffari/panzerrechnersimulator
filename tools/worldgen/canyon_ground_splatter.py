"""Small authored blood deposits on the actual Canyon heightfield.

These are static aftermath props, not a wound event or a fluid simulation. Their
broken square footprint follows the live cubical droplet/film vocabulary.
"""
import math

import numpy as np


def seed_at(x, z):
    return (round((x+2500)*1000)*73856093 ^ round((z+2500)*1000)*19349663) & 0xffffffff


def splatter(mesh, ground, x, z, seed):
    rng = np.random.default_rng(seed)
    angle = rng.uniform(0, math.tau)
    along = np.array([math.cos(angle), math.sin(angle)])
    across = np.array([-along[1], along[0]])
    # Most sites have one small contact group, some a second uneven spray. Avoid
    # concentric contours, a common stain radius, or a bright pink perimeter.
    centers = [(rng.uniform(.38, .57), rng.uniform(-.18, .18))]
    if rng.random() < .45:
        centers.append((rng.uniform(.48, .72), rng.uniform(-.26, .26)))
    colors = [(83, 17, 20), (104, 20, 23), (124, 26, 28), (93, 18, 21)]
    count, area = 0, 0.
    for group, (u, v) in enumerate(centers):
        # Touching small cells make a genuinely covered centre. An irregular boundary and
        # disconnected tail retain the droplet silhouette instead of a large uniform decal.
        cell = .014
        ru, rv = rng.uniform(.13,.22), rng.uniform(.085,.14)
        phase = rng.uniform(0,math.tau)
        cells = {}
        for iz in range(-12,13):
            for ix in range(-19,20):
                du, dv = ix*cell, iz*cell
                radius = math.hypot(du/ru,dv/rv)
                boundary = 1+.13*math.sin(ix*.47+phase)*math.cos(iz*.61-phase)
                if radius > boundary or (radius > .78 and rng.random() < .30):
                    continue
                cells[ix,iz] = colors[int(rng.integers(len(colors)))]
        for (ix,iz), color in cells.items():
            # Connected 14 mm film cells share their buried faces. Only the terrain-following
            # top and exposed perimeter exist; isolated tail deposits below remain cuboids.
            top = []
            for dx,dz in [(-.5,-.5),(-.5,.5),(.5,.5),(.5,-.5)]:
                p = np.array([x,z])+along*(u+(ix+dx)*cell)+across*(v+(iz+dz)*cell)
                top.append(np.array([p[0],ground(*p)+.003,p[1]]))
            mesh.quad(*top,color)
            for edge,neighbor in enumerate([(-1,0),(0,1),(1,0),(0,-1)]):
                if (ix+neighbor[0],iz+neighbor[1]) in cells:
                    continue
                a,b=top[edge],top[(edge+1)%4]
                down=np.array([0,.006,0])
                mesh.quad(a,a-down,b-down,b,color)
            count += 1
            area += cell*cell
        for _ in range(int(rng.integers(28, 64))):
            # Compact irregular contact, with isolated drops on one shorter tail.
            du, dv = rng.normal(0, .072), rng.normal(0, .053)
            if rng.random() < .27:
                du += rng.uniform(.10, .42)
                dv += rng.normal(0, .08)
            p = np.array([x, z]) + along*(u+du) + across*(v+dv)
            width = .006 + .026*rng.random()**2
            depth = width*rng.uniform(.72, 1.10)
            height = min(width, .003 + .007*rng.random())
            # Slightly embedded cubes stay in contact across the terrain slope.
            y = min(ground(p[0]+dx, p[1]+dz)
                    for dx, dz in [(-width/2, -depth/2), (width/2, -depth/2),
                                   (width/2, depth/2), (-width/2, depth/2)])
            mesh.box((p[0], y+height/2-.001, p[1]), (width, height, depth),
                     colors[int(rng.integers(len(colors)))])
            count += 1
            area += width*depth
    return dict(drops=count, summed_footprint_m2=area,
                width_m=[.006, .032], maximum_height_m=.010)
