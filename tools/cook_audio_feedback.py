#!/usr/bin/env python3
"""Cook exact, owner-private game recordings for the September gameplay review.

No pack is redistributed and originals are never edited. Recipes remain provisional
until heard in first-person video; file names and waveform checks are not auditions.
"""
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted'
BANK = ROOT / 'assets/sounds/pizza_private/feedback_20260905'
MANIFEST = ROOT / 'catalog/audio_feedback_20260905.json'


def main():
    rust = 'Rust & Blood - SFX Library v1.1d/Rust & Blood - SFX Library/OGG/Sound Effects/'
    echo = 'Echoes - Audio Super Kit v2.0.1/Echoes - Audio Super Kit/OGG/SFX/'
    # Crop the moving portion of the heavy door separately from the hard metal stop.
    recipes = [
        ('hatch_move', echo+'Doors & Drawers/heavy_door_open-close_1.ogg', .20, .90, .75, 2.8, 1800., 1.3),
        ('hatch_stop', rust+'Impact & Break/Metal/impact_metal_ufx_1.ogg', 0., .70, .72, .8, 2400., 1.6),
        ('trigger', rust+'Firearms/Misc/firearm_universal_safety_switch_ufx_1.ogg', 0., .25, 1., 1., 4500., 1.),
        ('hammer_strike', rust+'Melee/Hit/blunt_hit_metal_ufx_1.ogg', 0., .8, .76, 1.4, 2400., 2.1),
        ('luger_shot', rust+'Firearms/Fire/old_pistol_fire_ufx_1.ogg', 0., 4., 1., .85, 11000., 1.),
        ('rifle_shot', rust+'Firearms/Fire/old_rifle_fire_ufx_1.ogg', 0., 4., 1., .85, 11000., 1.),
        # Authored blast-body adaptation, not a claim of a recorded Tiger cannon.
        ('gun_interior', rust+'Explosions/explosion_close_short_ufx_1.ogg', 0., 2.4, .72, .8, 1500., 2.2),
        ('gun_exterior', rust+'Explosions/explosion_close_short_ufx_2.ogg', 0., 2.4, .82, .8, 4200., 1.6),
    ]
    BANK.mkdir(parents=True, exist_ok=True)
    entries = []
    for key, rel, start, end, pitch, gain, cutoff, bass in recipes:
        source = SOURCE / rel
        data = source.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        target = BANK / (digest+'.ogg')
        if target.exists() and target.read_bytes() != data:
            raise RuntimeError('Existing bank hash does not match its name')
        if not target.exists():
            shutil.copyfile(source, target)
        entries.append(dict(key=key, source=rel, path=str(target.relative_to(ROOT)),
                            bytes=len(data), sha256=digest, start=start, end=end,
                            pitch=pitch, gain=gain, lowpass_hz=cutoff, bass_gain=bass))
    manifest = dict(schema='kessel.audio-feedback', version=1,
                    license='pizzadoggy-local-purchase', owner_private=True,
                    review='first-person-video-audition-pending',
                    preserved=['engine_loop', 'interior_ambience', 'kv2_mg', '45mm_impact'],
                    no_matching_recording=['tiger_starter', 'turret_jam'], entries=entries)
    MANIFEST.write_text(json.dumps(manifest, indent=2)+'\n')
    (BANK/'BANK-RECEIPT.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(f'{len(entries)} exact recordings cooked; subjective video review pending')


if __name__ == '__main__':
    main()
