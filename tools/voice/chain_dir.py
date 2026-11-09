#!/usr/bin/env python3
"""Put a folder of dry rendered crew lines through the v2 intercom or radio chain.

    python3 tools/voice/chain_dir.py DRY_DIR OUT_DIR [LINES.json]

Files are named <id>_<role>[_tN].wav by render_elevenlabs.py. The company net voice (KP) and any
line whose text carries a "radio voice" tag go through the radio chain; everything else is the
intercom. Output is 48 kHz mono, same file names. CPU only (ffmpeg).
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import crew_chain as chain  # noqa: E402


def main():
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    lines_path = Path(sys.argv[3]) if len(sys.argv) > 3 else HERE / "crew_lines_tagged.json"
    text = {l["id"]: l["de"] for l in json.load(open(lines_path, encoding="utf-8"))}
    out.mkdir(parents=True, exist_ok=True)
    for wav in sorted(src.glob("*.wav")):
        line_id, role = wav.stem.split("_")[:2]
        on_net = role == "KP" or "[radio voice" in text.get(line_id, "")
        chain.write_wav(out / wav.name, chain.radio_v2(wav) if on_net else chain.intercom_v2(wav))
        print(f"{wav.name}: {'radio' if on_net else 'intercom'}")


if __name__ == "__main__":
    main()
