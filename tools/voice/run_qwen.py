"""Qwen3-TTS 1.7B VoiceDesign, crew v2: per role a reference read (for cloning) and every showcase
line, with deep-voice designs and pace/force (not pitch) for urgency.
venv-qwen/bin/python -I run_qwen.py [ref_takes] [line_takes]"""
import os, sys, json, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # tools/voice (a copy of the work dir's scripts)
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts")
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/panic/scripts")
import numpy as np, torch, soundfile as sf
from pathlib import Path
from crew import ROLES, DELIVERY, REF_TEXT, LINES
from guard import pick_device
OUT = Path("/home/alex/panzer/tts-radio-work-20261108/crew-v2/out/qwen"); (OUT / "refs").mkdir(parents=True, exist_ok=True); (OUT / "takes").mkdir(exist_ok=True)
REV = "5ecdb67327fd37bb2e042aab12ff7391903235d3"
RT = int(sys.argv[1]) if len(sys.argv) > 1 else 4
LT = int(sys.argv[2]) if len(sys.argv) > 2 else 4
ONLY_ROLES = os.environ.get("ROLES", "").split(",") if os.environ.get("ROLES") else None
dev = pick_device(7000, wait_s=600)
from qwen_tts import Qwen3TTSModel
kw = dict(device_map="cuda:0", dtype=torch.bfloat16) if dev == "cuda" else dict(device_map="cpu", dtype=torch.float32)
model = Qwen3TTSModel.from_pretrained(f"/home/alex/panzer/tts-radio-work-20261108/panic/hf/hub/models--Qwen--Qwen3-TTS-12Hz-1.7B-VoiceDesign/snapshots/{REV}", attn_implementation="sdpa", **kw)
stats = []
def gen(text, instruct, seed, path):
    torch.manual_seed(seed); np.random.seed(seed)
    t = time.time()
    wavs, sr = model.generate_voice_design(text=text, language="German", instruct=instruct)
    a = np.asarray(wavs[0], dtype=np.float32)
    sf.write(path, a, sr)
    stats.append(dict(file=str(path), seed=seed, sr=sr, dur=len(a) / sr, compute_s=time.time() - t, instruct=instruct, text=text))
    print(path.name, f"{len(a)/sr:.2f}s in {time.time()-t:.1f}s", flush=True)
for ri, (role, (_, _, voice)) in enumerate(ROLES.items()):
    if ONLY_ROLES and role not in ONLY_ROLES:
        continue
    for k in range(RT):
        gen(REF_TEXT, voice + DELIVERY["calm"], 5000 + 101 * ri + k, OUT / "refs" / f"{role}_ref_t{k}.wav")
for lid, role, reg, de, en in LINES:
    if ONLY_ROLES and role not in ONLY_ROLES:
        continue
    for k in range(LT):
        gen(de, ROLES[role][2] + DELIVERY[reg], 7000 + 17 * k + LINES.index(next(l for l in LINES if l[0] == lid)), OUT / "takes" / f"{lid}_t{k}.wav")
json.dump(dict(device=dev, stats=stats), open(OUT / f"stats{'_' + '_'.join(ONLY_ROLES) if ONLY_ROLES else ''}.json", "w"), indent=1, ensure_ascii=False)
