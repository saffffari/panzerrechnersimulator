#!/usr/bin/env python3
"""Fetch the OpenStreetMap data the Berlin 1945 map is laid out from, once, into the asset root.

    KESSEL_ASSET_ROOT=/windows/shop/panzerrechnersystem python3 tools/berlin1945/fetch_osm.py

Writes $KESSEL_ASSET_ROOT/assets/worlds/berlin1945/osm_berlin_mitte.json (gitignored): the Spree,
the named streets, building outlines and the landmark relations in a 2.4 km square around the
Reichstag, in the map's local frame (metres east and north of the Reichstag's footprint centre,
OSM relation 2201742). Data (c) OpenStreetMap contributors, ODbL 1.0; the snapshot time is kept.

Present-day OSM is used only where the 1945 city survives in it: the course of the Spree, the
street lines of the Dorotheenstadt and of the Tiergarten's avenues (unchanged since the
eighteenth and nineteenth centuries), the Reichstag's and the Brandenburg Gate's outlines, the
plots of the rebuilt Pariser Platz and Unter den Linden. What 1945 had and today's city does not
(Koenigsplatz, the Kroll, the Alsenviertel, the ministry block) comes from the 1928 aerial
survey and the photographs instead (tools/worldgen/berlin_reichstag.py, docs/BERLIN_REICHSTAG_1945.md).
"""
import json
import math
import os
import subprocess
import sys
from pathlib import Path

LAT0, LON0 = 52.51859575, 13.3761008      # the Reichstag's footprint centre (OSM relation 2201742)
HALF = 1200.0
MIRRORS = ['https://overpass-api.de/api/interpreter', 'https://maps.mail.ru/osm/tools/overpass/api/interpreter',
           'https://overpass.kumi.systems/api/interpreter']


def metres_per_degree(lat):
    r = math.radians(lat)
    mlat = 111132.954-559.822*math.cos(2*r)+1.175*math.cos(4*r)
    mlon = 111412.84*math.cos(r)-93.5*math.cos(3*r)
    return mlat, mlon


MLAT, MLON = metres_per_degree(LAT0)


def local(lat, lon):
    return round((lon-LON0)*MLON, 2), round((lat-LAT0)*MLAT, 2)


def bbox():
    dlat, dlon = HALF/MLAT, HALF/MLON
    return f'{LAT0-dlat},{LON0-dlon},{LAT0+dlat},{LON0+dlon}'


def query():
    b = bbox()
    return (f'[out:json][timeout:180];('
            f'way["highway"]["name"]({b});'
            f'way["waterway"="river"]({b});way["natural"="water"]({b});relation["natural"="water"]({b});'
            f'way["building"]({b});relation["building"]({b});'
            f'way["railway"~"^(rail|light_rail)$"]({b});'
            f');out geom;')


def main():
    root = Path(os.environ.get('KESSEL_ASSET_ROOT', '.'))
    out = root/'assets/worlds/berlin1945/osm_berlin_mitte.json'
    out.parent.mkdir(parents=True, exist_ok=True)
    raw = None
    for url in MIRRORS:
        r = subprocess.run(['curl', '-s', '-m', '300', '-A', 'panzerrechnersystem-worldgen/1.0',
                            '--data-urlencode', f'data={query()}', url], capture_output=True, text=True)
        try:
            raw = json.loads(r.stdout)
            break
        except json.JSONDecodeError:
            print(f'[fetch_osm] {url}: no JSON ({r.stdout[:80]!r})', file=sys.stderr)
    if raw is None:
        sys.exit('[fetch_osm] every Overpass mirror failed')
    elements = []
    for e in raw['elements']:
        tags = e.get('tags', {})
        if e['type'] == 'way' and 'geometry' in e:
            elements.append(dict(type='way', id=e['id'], tags=tags,
                                 pts=[local(p['lat'], p['lon']) for p in e['geometry']]))
        elif e['type'] == 'relation':
            rings = []
            for m in e.get('members', []):
                if m.get('geometry'):
                    rings.append(dict(role=m.get('role'), pts=[local(p['lat'], p['lon']) for p in m['geometry']]))
            elements.append(dict(type='relation', id=e['id'], tags=tags, rings=rings))
    doc = dict(schema='panzer.berlin1945-osm', version=1, origin=dict(lat=LAT0, lon=LON0),
               frame='x metres east, n metres north of the origin (local equirectangular)',
               half_extent_m=HALF, osm_base=raw.get('osm3s', {}).get('timestamp_osm_base'),
               licence='Data (c) OpenStreetMap contributors, ODbL 1.0, https://www.openstreetmap.org/copyright',
               elements=elements)
    out.write_text(json.dumps(doc, separators=(',', ':')))
    print(f'[fetch_osm] {len(elements)} elements, snapshot {doc["osm_base"]} -> {out}')


if __name__ == '__main__':
    main()
