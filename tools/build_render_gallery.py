"""Build a self-contained HTML render gallery from every captures/ folder across the worktrees.

usage: python -I build_gallery.py OUT_DIR SINCE_ISO  [DIR=LABEL ...]
Thumbnails (JPEG, 480 px wide) and video posters are written next to index.html; originals are linked by
absolute file:// path so the page stays small.
"""
import datetime as dt, html, json, os, re, subprocess, sys
from pathlib import Path

out = Path(sys.argv[1]); since = dt.datetime.fromisoformat(sys.argv[2])
sources = [a.split("=", 1) for a in sys.argv[3:]]
FFMPEG = os.environ.get("FFMPEG", "ffmpeg")
thumbs = out / "thumbs"; thumbs.mkdir(parents=True, exist_ok=True)

def prefix_of(name: str) -> str:
    stem = re.sub(r"\.(png|jpg|mp4)$", "", name, flags=re.I)
    stem = re.sub(r"_?\d{8}(-\d{4,6})?", "", stem)          # dates
    stem = re.sub(r"_(v|t|s|seed)?\d+$", "", stem)            # trailing version/seed/frame
    parts = stem.split("_")
    return "_".join(parts[:2]) if len(parts) > 1 else parts[0]

def thumb(src: Path, dst: Path, is_video: bool) -> bool:
    if dst.exists() and dst.stat().st_mtime >= src.stat().st_mtime:
        return True
    args = [FFMPEG, "-v", "error", "-y"]
    if is_video:
        args += ["-ss", "2", "-i", str(src), "-frames:v", "1"]
    else:
        args += ["-i", str(src)]
    args += ["-vf", "scale=480:-2", "-q:v", "4", str(dst)]
    return subprocess.run(args, capture_output=True).returncode == 0

groups = {}   # label -> prefix -> [entries]
totals = {"png": 0, "mp4": 0}
for src_dir, label in sources:
    d = Path(src_dir)
    if not d.is_dir():
        continue
    for f in sorted(d.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True):
        if not f.is_file() or f.suffix.lower() not in (".png", ".jpg", ".mp4"):
            continue
        m = dt.datetime.fromtimestamp(f.stat().st_mtime)
        if m < since:
            continue
        is_video = f.suffix.lower() == ".mp4"
        t = thumbs / (label + "__" + f.stem + ".jpg")
        if not thumb(f, t, is_video):
            continue
        totals["mp4" if is_video else "png"] += 1
        groups.setdefault(label, {}).setdefault(prefix_of(f.name), []).append(
            {"name": f.name, "path": f.resolve().as_uri(), "thumb": "thumbs/" + t.name,
             "video": is_video, "mtime": m.strftime("%Y-%m-%d %H:%M"), "kb": f.stat().st_size // 1024})

css = """
body{font:14px/1.4 system-ui,sans-serif;background:#111;color:#ddd;margin:0;padding:16px}
h1{font-size:20px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 6px;color:#fff;border-bottom:1px solid #333}
h3{font-size:13px;margin:16px 0 6px;color:#aaa;font-weight:normal}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:8px}
.card{background:#1a1a1a;border-radius:4px;overflow:hidden}.card img{width:100%;display:block;image-rendering:pixelated}
.card .cap{padding:4px 6px;font-size:11px;color:#999;word-break:break-all}.card.video .cap::before{content:"\\25B6 ";color:#f90}
.summary{color:#999;margin-bottom:12px}nav a{color:#9cf;margin-right:12px;font-size:13px}
"""
parts = [f"<!doctype html><meta charset=utf-8><title>Panzerrechnersystem render gallery</title><style>{css}</style>",
         f"<h1>Panzerrechnersystem render gallery</h1><div class=summary>{totals['png']} stills, {totals['mp4']} videos, "
         f"captured since {since:%Y-%m-%d}, generated {dt.datetime.now():%Y-%m-%d %H:%M}. Click a thumbnail to open the original.</div><nav>"]
for label in groups:
    parts.append(f"<a href='#{html.escape(label)}'>{html.escape(label)}</a>")
parts.append("</nav>")
for label, prefixes in groups.items():
    n = sum(len(v) for v in prefixes.values())
    parts.append(f"<h2 id='{html.escape(label)}'>{html.escape(label)} <small>({n})</small></h2>")
    for prefix, entries in sorted(prefixes.items(), key=lambda kv: -len(kv[1])):
        parts.append(f"<h3>{html.escape(prefix)} ({len(entries)})</h3><div class=grid>")
        for e in entries:
            cls = "card video" if e["video"] else "card"
            parts.append(f"<a class='{cls}' href='{e['path']}' title='{e['mtime']} {e['kb']} KB'><img loading=lazy src='{e['thumb']}'>"
                         f"<div class=cap>{html.escape(e['name'])}</div></a>")
        parts.append("</div>")
(out / "index.html").write_text("\n".join(parts), encoding="utf-8")
(out / "manifest.json").write_text(json.dumps(groups, indent=1), encoding="utf-8")
print(json.dumps(totals), out / "index.html")
