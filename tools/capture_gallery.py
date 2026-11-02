#!/usr/bin/env python3
"""One place to find every still and clip the simulator produced, across every worktree.

    python3 tools/capture_gallery.py            # write /home/alex/panzer/captures/index.html
    python3 tools/capture_gallery.py --consolidate
        # also reflink-copy every image and video from each worktree's captures/ into the shared
        # folder as <worktree>-<relative path with '-' for '/'>, never overwriting, deleting nothing

The shared folder is the one the engine writes to when `KESSEL_CAPTURES_DIR` is set (the demo
launcher sets it): new captures land there directly, prefixed with the worktree they came from.
The page lists everything newest first, grouped by day, with the worktree, the file's own name and
its time, so a frame can be compared with the earlier versions beside it.
"""
import argparse
import datetime as dt
import html
import os
import shutil
import sys
from pathlib import Path

PANZER = Path("/home/alex/panzer")
SHARED = PANZER / "captures"
IMAGES = {".png", ".jpg", ".jpeg", ".webp", ".gif"}
VIDEOS = {".mp4", ".webm", ".mkv", ".mov"}


def worktree_captures():
    for entry in sorted(PANZER.iterdir()):
        if entry.name == "captures" or not entry.is_dir():
            continue
        folder = entry / "captures"
        if folder.is_dir():
            yield entry.name, folder


def media(folder):
    for path in folder.rglob("*"):
        if path.is_file() and path.suffix.lower() in IMAGES | VIDEOS:
            yield path


def consolidate():
    copied = 0
    for worktree, folder in worktree_captures():
        for path in media(folder):
            rel = path.relative_to(folder)
            target = SHARED / f"{worktree}-{'-'.join(rel.parts)}"
            if target.exists():
                continue
            try:
                os.system(f"cp --reflink=auto -p {str(path)!r} {str(target)!r}")
                copied += 1
            except Exception as error:  # noqa: BLE001
                print(f"skip {path}: {error}", file=sys.stderr)
    print(f"consolidated {copied} new files into {SHARED}")


def gallery():
    rows = []
    seen = set()
    for path in media(SHARED):
        rows.append(("shared", path))
        seen.add(path.name)
    for worktree, folder in worktree_captures():
        for path in media(folder):
            rel = path.relative_to(folder)
            if f"{worktree}-{'-'.join(rel.parts)}" in seen:
                continue
            rows.append((worktree, path))
    rows.sort(key=lambda r: r[1].stat().st_mtime, reverse=True)
    out = ["<!doctype html><meta charset='utf-8'><title>Panzer captures</title>",
           "<style>body{background:#111;color:#ddd;font:14px system-ui;margin:16px}"
           "h2{margin:24px 0 8px;font-weight:600}.grid{display:flex;flex-wrap:wrap;gap:10px}"
           ".card{width:320px;background:#1b1b1b;padding:6px;border-radius:6px}"
           ".card img,.card video{width:308px;height:173px;object-fit:contain;background:#000;display:block}"
           ".meta{font-size:12px;color:#aaa;margin-top:4px;word-break:break-all}.wt{color:#8cf}</style>",
           f"<h1>Panzer captures ({len(rows)} files, newest first)</h1>",
           "<p>Shared folder: " + html.escape(str(SHARED)) + ". Set by the demo launcher as KESSEL_CAPTURES_DIR; "
           "run <code>python3 tools/capture_gallery.py</code> to refresh this page.</p>"]
    day = None
    for worktree, path in rows:
        when = dt.datetime.fromtimestamp(path.stat().st_mtime)
        if when.date() != day:
            if day is not None:
                out.append("</div>")
            day = when.date()
            out.append(f"<h2>{day.isoformat()}</h2><div class='grid'>")
        href = html.escape(path.as_uri())
        tag = (f"<video src='{href}' controls preload='metadata'></video>" if path.suffix.lower() in VIDEOS
               else f"<a href='{href}'><img loading='lazy' src='{href}'></a>")
        out.append(f"<div class='card'>{tag}<div class='meta'><span class='wt'>{html.escape(worktree)}</span> "
                   f"{when.strftime('%H:%M')} · {html.escape(path.name)}</div></div>")
    if day is not None:
        out.append("</div>")
    SHARED.mkdir(parents=True, exist_ok=True)
    index = SHARED / "index.html"
    index.write_text("\n".join(out))
    print(f"wrote {index} ({len(rows)} files)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--consolidate", action="store_true")
    args = parser.parse_args()
    if args.consolidate:
        consolidate()
    gallery()
