#!/usr/bin/env python3
"""Turn the crew line sheets (docs/crew_lines/*.md) into the list render_elevenlabs.py reads.

    python3 tools/voice/lines_md_to_json.py [OUT.json] [--min-int N] [--max-int N] [--role KDT,RS,...]

Each table row `| ID | Wann | Int | Zeile | English |` becomes
{"id", "role", "register", "int", "when", "de", "en"}. `de` keeps the square-bracket tags, which
Eleven v3 reads as direction. `register` is "calm" for intensity 1-2 and "urgent" above, the two
voice settings the render script has. Rows that are only a sound (no words) are kept.
"""
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHEETS = HERE.parent.parent / "docs" / "crew_lines"
ROW = re.compile(r"^\| ([A-Z]{2,3})-(\d{3}) \| (.*?) \| ([1-5]) \| (.*?) \| (.*?) \|\s*$")


def main():
    args = sys.argv[1:]
    opt = lambda k, d: args[args.index(k) + 1] if k in args else d
    lo, hi = int(opt("--min-int", 1)), int(opt("--max-int", 5))
    roles = set(opt("--role", "KDT,RS,LS,FA,FU").split(","))
    out_path = next((a for i, a in enumerate(args) if not a.startswith("--") and (i == 0 or not args[i - 1].startswith("--"))), None)
    lines = []
    for sheet in sorted(SHEETS.glob("*.md")):
        for raw in sheet.read_text(encoding="utf-8").splitlines():
            m = ROW.match(raw)
            if not m:
                continue
            role, num, when, level, de, en = m.groups()
            level = int(level)
            if role in roles and lo <= level <= hi:
                lines.append({"id": f"{role}-{num}", "role": role, "register": "calm" if level <= 2 else "urgent",
                              "int": level, "when": when, "de": de, "en": en})
    text = json.dumps(lines, ensure_ascii=False, indent=1)
    if out_path:
        Path(out_path).write_text(text + "\n", encoding="utf-8")
        print(f"{len(lines)} lines -> {out_path}")
    else:
        print(text)


if __name__ == "__main__":
    main()
