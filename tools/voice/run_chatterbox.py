"""Chatterbox Multilingual (MIT), crew v2: every line cloned from its role's chosen reference read
(a Qwen3-TTS VoiceDesign take, out/refs_chosen/ROLE.wav), so each crewman keeps one voice.
Low exaggeration: Chatterbox's exaggeration raises pitch and energy together; urgency comes from
a lower cfg_weight (faster, pushier pacing) and the wording. No pitch processing.
venv-chatterbox/bin/python -I run_chatterbox.py [takes]   (env LINES_FILE=json list overrides LINES)"""
import os, sys, json, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # tools/voice (a copy of the work dir's scripts)
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/crew-v2/scripts")
sys.path.insert(0, "/home/alex/panzer/tts-radio-work-20261108/panic/scripts")
import numpy as np, torch, soundfile as sf
from pathlib import Path
from crew import LINES
from guard import pick_device
ROOT = Path("/home/alex/panzer/tts-radio-work-20261108/crew-v2/out")
OUT = Path(os.environ.get("OUT_DIR", ROOT / "chatterbox")); (OUT / "takes").mkdir(parents=True, exist_ok=True)
TAKES = int(sys.argv[1]) if len(sys.argv) > 1 else 3
PARAMS = {"calm": dict(exaggeration=0.35, cfg_weight=0.5, temperature=0.7),
          "urgent": dict(exaggeration=0.5, cfg_weight=0.3, temperature=0.75)}
lines = LINES
if os.environ.get("LINES_FILE"):
    lines = [tuple(l) for l in json.load(open(os.environ["LINES_FILE"]))]
dev = pick_device(6000, wait_s=900)
from chatterbox.mtl_tts import ChatterboxMultilingualTTS
model = ChatterboxMultilingualTTS.from_pretrained(device=dev, t3_model="v3")
stats = []
for n, (lid, role, reg, de, en) in enumerate(lines):
    ref = ROOT / "refs_chosen" / f"{role}.wav"
    start = int(os.environ.get("TAKE_START", "0"))
    only = os.environ.get("LINES_ONLY", "").split(",") if os.environ.get("LINES_ONLY") else None
    if only and lid not in only:
        continue
    for k in range(start, start + TAKES):
        seed = 9000 + 31 * k + n
        torch.manual_seed(seed); np.random.seed(seed)
        t = time.time()
        w = model.generate(de, language_id="de", audio_prompt_path=str(ref), **PARAMS[reg])
        a = w.squeeze(0).cpu().numpy().astype(np.float32)
        p = OUT / "takes" / f"{lid}_t{k}.wav"
        sf.write(p, a, model.sr)
        stats.append(dict(file=str(p), line=lid, role=role, reg=reg, take=k, seed=seed, sr=model.sr, dur=len(a) / model.sr,
                          compute_s=time.time() - t, params=PARAMS[reg], ref=str(ref)))
        print(p.name, f"{len(a)/model.sr:.2f}s in {time.time()-t:.1f}s", flush=True)
json.dump(dict(device=dev, stats=stats), open(OUT / f"stats{os.environ.get('TAKE_START', '')}.json", "w"), indent=1, ensure_ascii=False)
