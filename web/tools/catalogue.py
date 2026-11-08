"""Extract native coefficients; no network or assets outside this checkout."""
import re,json,pathlib
root=pathlib.Path(__file__).resolve().parents[2]
src=(root/'src/ballistics.rs').read_text()
order=['BR_240_M42','BR_240','BR_240P','BR_271_ZIS2','BR_350B','OF_350_F34','BR_354P','BR_365','BR_412','BR_471','OF_530','BR_540','PZGR_39','PZGR_40','SPRGR_88','PZGR_39_43']
rows=[]
for key in order:
    block=re.search(r'pub static '+key+r': ShellSpec = ShellSpec \{(.*?)\n\};',src,re.S).group(1)
    row={'id':key,'name':re.search(r'name: "([^"]+)"',block).group(1),'kind':re.search(r'kind: ShellKind::(\w+)',block).group(1)}
    for field in ['caliber_mm','mass_kg','muzzle_velocity','filler_g','drag_k','pen_ref_mm','ref_range_m']:
        row[field]=float(re.search(field+r': ([\d.eE+\-]+)',block).group(1))
    rows.append(row)
(root/'web/src/catalogue.json').write_text(json.dumps(rows,indent=2)+'\n')
