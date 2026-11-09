#!/usr/bin/env python3
"""Put rendered sound-effect takes where the game looks for them.

    python3 tools/sound/install_sfx.py RENDER_DIR [ASSET_ROOT]

RENDER_DIR holds <ID>_<nn>.mp3 (dry source) and <ID>_w<nn>.mp3 (heard from inside the turret) with
prompts.json, as tools/sound/render_sfx.py writes them. Each becomes ASSET_ROOT/assets/sounds/
<same name>.ogg (Vorbis, 44.1 kHz, channels as rendered), the naming the game's ID-keyed hooks
read (src/swarm_sound.rs). A file that already exists is never overwritten. The folder is the
owner's private, git-ignored sound store; GENERATED_20261109_SOURCES.txt there records what each
file is and the prompt that made it. Level is left as rendered: the game sets it.
"""
import json
import subprocess
import sys
from pathlib import Path

src = Path(sys.argv[1])
root = Path(sys.argv[2] if len(sys.argv) > 2 else "/windows/shop/panzerrechnersystem")
dst = root / "assets" / "sounds"
log = json.loads((src / "prompts.json").read_text()) if (src / "prompts.json").exists() else {}
made = kept = 0
for mp3 in sorted(src.glob("*.mp3")):
    out = dst / (mp3.stem + ".ogg")
    if out.exists():
        kept += 1
        continue
    subprocess.run(["ffmpeg", "-v", "error", "-n", "-i", str(mp3), "-c:a", "libvorbis", "-q:a", "6", "-ar", "44100", str(out)], check=True)
    made += 1
lines = ["Generated sound effects, ElevenLabs sound-effect generator, rendered 2026-11-09 from",
         "docs/SOUND_PROMPTS_20261109.md by tools/sound/render_sfx.py. file | row | model | prompt", ""]
for name in sorted(log):
    ogg = dst / (Path(name).stem + ".ogg")
    if ogg.exists():
        m = log[name]
        lines.append(f"{ogg.name} | {m.get('id', '')} {m.get('name', '')} | {m.get('model', 'eleven_text_to_sound_v2')} | {m.get('text', '')}")
(dst / "GENERATED_20261109_SOURCES.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"installed {made}, already there {kept}, listed {len(lines) - 3}, in {dst}")
