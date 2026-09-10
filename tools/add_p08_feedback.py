#!/usr/bin/env python3
"""Append the owner's exact P08 stage recordings without rewriting existing sound choices."""
import argparse, hashlib, json, shutil
from pathlib import Path

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--asset-root',type=Path,required=True);args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1];root=args.asset_root.resolve()
    source=root/'source_assets/legacy-imports/legacy-main-assets-20260902/pizza/_extracted'
    bank=root/'assets/sounds/pizza_private/feedback_20260905';bank.mkdir(parents=True,exist_ok=True)
    manifest=repo/'catalog/audio_feedback_20260905.json';doc=json.loads(manifest.read_text())
    base='Rust & Blood - SFX Library v1.1d/Rust & Blood - SFX Library/OGG/Sound Effects/Firearms/'
    recipes=[('p08_mag_out','Reloading/pistol_reloading_mag_out_ufx_1.ogg',.13,.46,1.4),
             ('p08_mag_in','Reloading/pistol_reloading_mag_in_ufx_1.ogg',.115,.30,1.0),
             ('p08_toggle_pull','Racking/pistol_racking_ufx_1.ogg',.09,.255,.8),
             ('p08_toggle_close','Racking/pistol_racking_ufx_1.ogg',.30,.50,.8)]
    entries=[]
    for key,rel,start,end,gain in recipes:
        payload=(source/(base+rel)).read_bytes();digest=hashlib.sha256(payload).hexdigest();target=bank/(digest+'.ogg')
        if target.exists() and target.read_bytes()!=payload:raise RuntimeError('Hash path mismatch')
        if not target.exists():shutil.copyfile(source/(base+rel),target)
        entries.append(dict(key=key,source=base+rel,path=str(target.relative_to(root)),bytes=len(payload),sha256=digest,start=start,end=end,pitch=1.0,gain=gain,lowpass_hz=7500.0,bass_gain=1.0))
    keys={e['key'] for e in entries};doc['entries']=[e for e in doc['entries'] if e['key'] not in keys]+entries
    text=json.dumps(doc,indent=2)+'\n';manifest.write_text(text);(bank/'BANK-RECEIPT.json').write_text(text)
    print('Added four exact source-backed P08 stage cues; existing recipes retained.')
if __name__=='__main__':main()
