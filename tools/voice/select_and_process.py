"""Measure every take (pitch, low band, ASR), reject cracks and squeaks, pick one take per line,
write dry + intercom-v2 files and the comparison reel to the captures folder, and a table.
venv-qwen/bin/python -I select_and_process.py ENGINE[,ENGINE...]"""
import os, sys, json, glob, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # tools/voice (a copy of the work dir's scripts)
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts")
from pathlib import Path
import numpy as np
from crew import LINES, ROLES, COMPARE
try:
    import chain
except ImportError:
    import crew_chain as chain
from measure import measure
ROOT = Path("/home/alex/panzer/tts-radio-work-20261108/crew-v2/out")
CAP = Path("/home/alex/panzer/captures")
PRE = "tts-crew-20261108-v2-"
engines = sys.argv[1].split(",")
MAX_P95, MAX_SUS, MED_LO, MED_HI, MAX_CER = 200.0, 200.0, 75.0, 140.0, 0.25

rows = []
for eng in engines:
    for f in sorted(glob.glob(str(ROOT / eng / "takes" / "*.wav"))):
        lid = Path(f).stem.split("_")[0]
        if lid not in {l[0] for l in LINES}:
            continue
        line = next(l for l in LINES if l[0] == lid)
        r = measure(f, line[3])
        r.update(engine=eng, line=lid, role=line[1], register=line[2])
        reasons = []
        if not (r["f0_p95"] <= MAX_P95): reasons.append(f"p95 {r['f0_p95']:.0f} Hz")
        if not (r["f0_max_sustained"] <= MAX_SUS): reasons.append(f"held {r['f0_max_sustained']:.0f} Hz")
        if not (MED_LO <= r["f0_median"] <= MED_HI): reasons.append(f"median {r['f0_median']:.0f} Hz")
        if r["cer"] > MAX_CER: reasons.append(f"CER {r['cer']:.2f}")
        r["reject"] = reasons
        rows.append(r)
        print(f"{eng:10s} {Path(f).stem:8s} med {r['f0_median']:5.0f} p95 {r['f0_p95']:5.0f} sus {r['f0_max_sustained']:5.0f} "
              f"<300 {r['share_lt300']:.2f} cer {r['cer']:.2f} {'REJECT ' + '; '.join(reasons) if reasons else 'ok'} | {r['asr']}", flush=True)

pick = {}
for lid, role, reg, de, en in LINES:
    cands = [r for r in rows if r["line"] == lid and not r["reject"]]
    # Prefer the cloned engine (one voice per crewman), then the fewest errors, then the lowest p95.
    order = {e: i for i, e in enumerate(engines)}
    cands.sort(key=lambda r: (order[r["engine"]], round(r["cer"], 2), r["f0_p95"]))
    pick[lid] = cands[0] if cands else None

table = []
for lid, role, reg, de, en in LINES:
    r = pick[lid]
    if r is None:
        print(f"{lid}: no take passed", flush=True); table.append(dict(line=lid, role=role, picked=None)); continue
    stem = f"{PRE}{lid}-{ROLES[role][1].replace(' ', '-')}"
    dry = chain.dry(r["file"]); chain.write_wav(CAP / f"{stem}-dry.wav", dry)
    ic = chain.intercom_v2(CAP / f"{stem}-dry.wav"); chain.write_wav(CAP / f"{stem}-intercom.wav", ic)
    after = measure(str(CAP / f"{stem}-intercom.wav"), de)
    table.append(dict(line=lid, role=role, register=reg, de=de, en=en, engine=r["engine"], take=r["file"],
                      dry=dict((k, r[k]) for k in ("f0_median", "f0_p5", "f0_p95", "f0_max_sustained", "share_lt300", "share_120_300", "share_gt3k", "cer", "asr", "dur")),
                      intercom=dict((k, after[k]) for k in ("f0_median", "f0_p95", "share_lt300", "share_120_300", "share_gt3k", "cer", "asr")),
                      files=[f"{stem}-dry.wav", f"{stem}-intercom.wav"]))
    print(f"{lid} {role} {r['engine']} -> {stem}", flush=True)

# Comparison reel: the same dry take through the v1 intercom chain, then the v2 chain.
r = pick.get(COMPARE)
if r:
    stem = f"{PRE}{COMPARE}-{ROLES[r['role']][1]}"
    src = CAP / f"{stem}-dry.wav"
    gap = np.zeros(int(0.8 * chain.SR), np.float32)
    v1 = chain.intercom_v1(src); v2 = chain.intercom_v2(src); d = chain.dry(src)
    reel = np.concatenate([d, gap, v1, gap, v2])
    chain.write_wav(CAP / f"{PRE}comparison-{COMPARE}-dry-old-new.wav", reel)
    m1 = measure(str(CAP / f"{PRE}comparison-{COMPARE}-dry-old-new.wav"), None, do_asr=False)
    chain.write_wav(ROOT / "cmp_v1.wav", v1); chain.write_wav(ROOT / "cmp_v2.wav", v2)
    table.append(dict(comparison=dict(line=COMPARE, order="dry, then the v1 intercom chain, then the v2 chain",
        v1=measure(str(ROOT / "cmp_v1.wav"), None, do_asr=False), v2=measure(str(ROOT / "cmp_v2.wav"), None, do_asr=False))))
json.dump(dict(rows=rows, picked=table), open(ROOT / "selection.json", "w"), indent=1, ensure_ascii=False, default=float)
print("wrote", ROOT / "selection.json")
