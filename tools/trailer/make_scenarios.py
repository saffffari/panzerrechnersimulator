#!/usr/bin/env python3
"""Write the trailer's workshop variants and first-person scenarios."""
import json, os, shutil
from pathlib import Path
ROOT = Path('/home/alex/panzer/adaptive-audio-20261002')
W = ROOT/'assets/worldgen/workshops'
OUT = Path(__file__).resolve().parent/'scenarios'; OUT.mkdir(exist_ok=True)

def variant(name, utc, overcast=None):
    src, dst = W/'steppe-spring-v1', W/f'trailer-steppe-{name}-20261004'
    dst.mkdir(exist_ok=True)
    for f in os.listdir(src):
        if f == 'manifest.v1.json':
            if (dst/f).is_symlink(): (dst/f).unlink()
            shutil.copyfile(src/f, dst/f)
        elif f != 'manifest_edits.json' and not (dst/f).exists(): os.symlink(src/f, dst/f)
    e = json.loads((src/'manifest_edits.json').read_text())
    e['environment_lighting']['utc'] = utc
    if overcast is not None: e['environment_lighting']['overcast'] = overcast
    (dst/'manifest_edits.json').write_text(json.dumps(e))
    return str(dst/'manifest.v1.json')

DAY = str(W/'steppe-spring-v1/manifest.v1.json')
SUNSET = variant('sunset', '1943-07-16T16:46:00Z')
DUSK = variant('dusk', '1943-07-16T17:12:00Z')
NIGHT = variant('night', '1943-07-16T19:30:00Z')
WINTER = str(W/'gaea-canyon-run-v2/manifest.v1.json')
SPAWN = [-1622.0, 1627.0]

def scene(name, manifest, station, seconds, setup, timeline):
    base = dict(playtest=True, singleplayer_crew=True, enemy_policy='hold_fire')
    base.update(setup)
    spec = dict(schema='kessel.first-person-mechanic.v1', act_id=1, world_manifest=manifest, night=False,
        id=f'trailer_{name}_20261004', title=f'Trailer beat: {name}', station=station, seconds=seconds,
        status='trailer_capture', setup=base, timeline=timeline,
        historical_scope='Staged trailer footage of the present build; not a historical reconstruction.',
        limitations=['Scripted Game inputs, not native mouse/keyboard capture. Gameplay audio is retained.'])
    (OUT/f'{name}.json').write_text(json.dumps(spec, indent=1))

HEAD_OUT = [dict(at_s=0.3, action='hatch'), dict(at_s=1.2, action='head_out', allow_rejected=True)]
look = lambda a, b, dx, dy=0.0: dict(from_s=a, until_s=b, input=dict(look_dx=dx, look_dy=dy))
drive = lambda a, b, v=1.0, steer=0.0: dict(from_s=a, until_s=b, input=dict(drive=v, steer=steer))

scene('sunset', SUNSET, 'commander', 26, dict(canyon_session=True),
    HEAD_OUT + [drive(3, 26, 0.7)])
scene('moon', NIGHT, 'commander', 16, dict(canyon_session=True),
    HEAD_OUT + [drive(2, 16, 0.7), look(4.0, 9.0, 0.25)])
scene('nightfire', NIGHT, 'commander', 10, dict(canyon_session=True),
    HEAD_OUT + [dict(at_s=4.0, action='staged_discharge')])
scene('daydrive', DAY, 'commander', 14, dict(canyon_session=True),
    HEAD_OUT + [drive(1.5, 14, 1.0)])
r = json.load(open(ROOT/'.claude/worktrees/bug-squash-20261003/catalog/shader_reviews/canyon_v2_reload_native800_20261001.v1.json'))
scene('reload', WINTER, 'gunner', r['seconds'], dict(canyon_session=True), r['timeline'])
scene('wintercombat', WINTER, 'commander', 18, dict(canyon_session=True, enemy_policy='live',
        canyon_stage=dict(gun=0, range_m=180, lay_on_gun=True)),
    [dict(at_s=0.3, action='hatch'), dict(at_s=1.2, action='head_out', allow_rejected=True),
     drive(1.5, 10, 0.1), dict(at_s=5.5, action='staged_discharge'),
     dict(at_s=13, action='lay_on_at_gun', value='0'), dict(at_s=14, action='staged_discharge')])
scene('sight', DAY, 'gunner', 14, dict(canyon_session=True, enemy_policy='live',
        canyon_stage=dict(gun=0, range_m=320, lay_on_gun=True)),
    [dict(at_s=0.5, action='lay_on_at_gun', value='0'), dict(at_s=1.0, action='sight_zoom'),
     dict(at_s=4.5, action='staged_discharge'), dict(at_s=9.0, action='lay_on_at_gun', value='0'),
     dict(at_s=11.0, action='staged_discharge')])
scene('smoke', DUSK, 'loader', 14, dict(canyon_session=True),
    [dict(at_s=0.5, action='cigarette'), dict(from_s=2, until_s=4, input=dict(aim=True)),
     dict(from_s=3.25, until_s=3.35, input=dict(mg_coax=True)), dict(from_s=5, until_s=5.1, input=dict(mg_coax=True)),
     dict(from_s=6.5, until_s=8.5, input=dict(fire2=True)), dict(from_s=10.5, until_s=12.5, input=dict(fire2=True))])
scene('onfoot', DUSK, 'commander', 12, dict(on_foot=True, player_xz=SPAWN, player_heading_deg=135,
        foot_xz=[SPAWN[0]+9, SPAWN[1]-7], look_at_xz=SPAWN),
    [drive(1.0, 6.0, 0.6), look(6.0, 10.0, 0.12)])
print('wrote', sorted(p.name for p in OUT.glob('*.json')))
