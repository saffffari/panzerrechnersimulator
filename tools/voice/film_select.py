"""The film's lines: measure each take, reject cracks and squeaks, keep one per line, process it
through the v2 intercom chain, then schedule the cues with the real durations.
venv-qwen/bin/python -I film_select.py FILM_DIR"""
import sys, json, glob
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts")
sys.path.insert(0, "/home/alex/panzer/demo-20261101/tools/voice")
from pathlib import Path
import chain
from measure import measure
from crew_callouts import schedule
W = Path(sys.argv[1])
lines = json.load(open(W / "film_lines.json"))
pick, rows = {}, []
for lid, role, reg, de, en in lines:
    best = None
    for f in sorted(glob.glob(str(W / "takes" / f"{lid}_t*.wav"))):
        r = measure(f, de)
        bad = []
        if not (r["f0_p95"] <= 200): bad.append(f"p95 {r['f0_p95']:.0f}")
        if not (r["f0_max_sustained"] <= 200): bad.append(f"held {r['f0_max_sustained']:.0f}")
        if not (75 <= r["f0_median"] <= 140): bad.append(f"median {r['f0_median']:.0f}")
        if r["cer"] > 0.25: bad.append(f"cer {r['cer']:.2f}")
        rows.append(dict(line=lid, file=f, reject=bad, **{k: r[k] for k in ("f0_median", "f0_p95", "f0_max_sustained", "share_lt300", "cer", "asr", "dur")}))
        print(f"{Path(f).stem:8s} {role} med {r['f0_median']:5.0f} p95 {r['f0_p95']:5.0f} held {r['f0_max_sustained']:5.0f} cer {r['cer']:.2f} {'REJECT ' + ', '.join(bad) if bad else 'ok'} | {r['asr']}", flush=True)
        if not bad and (best is None or (round(r["cer"], 2), r["f0_p95"]) < (round(best[1]["cer"], 2), best[1]["f0_p95"])):
            best = (f, r)
    if best is None:
        print(f"{lid} {de}: NO TAKE PASSED", flush=True)
        continue
    out = W / "lines" ; out.mkdir(exist_ok=True)
    dry = chain.dry(best[0]); chain.write_wav(out / f"{lid}-dry.wav", dry)
    ic = chain.intercom_v2(out / f"{lid}-dry.wav"); chain.write_wav(out / f"{lid}.wav", ic)
    # the spoken part's length (the chain pads 0.17 s)
    pick[(role, de)] = dict(file=f"{lid}.wav", dur=best[1]["dur"], take=best[0], f0_median=best[1]["f0_median"], f0_p95=best[1]["f0_p95"])
cues = json.load(open(W / "cues_raw.json"))
durations = {k: v["dur"] for k, v in pick.items()}
cues = [c for c in cues if (c["role"], c["de"]) in pick]
sched = schedule(cues, durations)
for c in sched:
    c["file"] = pick[(c["role"], c["de"])]["file"]
json.dump(sched, open(W / "cues.json", "w"), indent=1, ensure_ascii=False)
json.dump(dict(rows=rows, picked={f"{k[0]}|{k[1]}": v for k, v in pick.items()}), open(W / "film_selection.json", "w"), indent=1, ensure_ascii=False, default=float)
print(f"{len(sched)} cues scheduled of {len(cues)}")
for c in sched:
    print(f"{c['start']:7.2f} {c['dur']:4.1f} {c['role']:3s} {c['de']} | {c['en']}")
