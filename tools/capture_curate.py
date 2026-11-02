#!/usr/bin/env python3
"""Curate the capture folders (owner, 2026-11-01): PNG stills become JPEGs, and a clip whose
muxed copy with sound exists loses its silent picture and its soundtrack wav.

    python3 tools/capture_curate.py --dry-run         # count and list what would change
    python3 tools/capture_curate.py                   # do it, across every worktree's captures/ and the shared folder

Conversion keeps the image's own size (quality 90, RGB); the source PNG is removed only after the
JPEG was written and re-read. A silent picture is `<stem>_picture.mp4`, `<stem>_silent.mp4` or `<stem>_video_only.mp4`
beside `<stem>.mp4`; its soundtrack is `<stem>_soundtrack.wav` (or `<stem>_damage_soundtrack.wav`).
Nothing else is touched; JSON receipts that name the old files keep their names.
"""
import argparse
import sys
from pathlib import Path

from PIL import Image

PANZER = Path("/home/alex/panzer")


def capture_folders():
    yield PANZER / "captures"
    for entry in sorted(PANZER.iterdir()):
        if entry.is_dir() and entry.name != "captures" and (entry / "captures").is_dir():
            yield entry / "captures"


def convert_png(path, dry):
    target = path.with_suffix(".jpg")
    if target.exists():
        return 0, 0
    before = path.stat().st_size
    if dry:
        return before, 0
    with Image.open(path) as im:
        im.convert("RGB").save(target, "JPEG", quality=90, optimize=True)
    with Image.open(target) as check:
        check.verify()
    after = target.stat().st_size
    path.unlink()
    return before, after


def prune_silent(folder, dry):
    freed = 0
    removed = []
    for clip in folder.rglob("*.mp4"):
        stem = clip.name[:-4]
        for suffix in ("_picture", "_silent", "_video_only", "_damage_video_only"):
            if stem.endswith(suffix):
                muxed = clip.with_name(stem[: -len(suffix)] + ".mp4")
                if muxed.exists() and muxed.stat().st_size > 0:
                    freed += clip.stat().st_size
                    removed.append(clip)
    for wav in folder.rglob("*_soundtrack.wav"):
        stem = wav.name[: -len("_soundtrack.wav")]
        for base in (stem, stem[: -len("_damage")] if stem.endswith("_damage") else None):
            if base and (wav.with_name(base + ".mp4")).exists():
                freed += wav.stat().st_size
                removed.append(wav)
                break
    if not dry:
        for path in removed:
            path.unlink()
    return freed, removed


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    total_before = total_after = pngs = 0
    pruned_bytes = 0
    pruned = 0
    for folder in capture_folders():
        for png in folder.rglob("*.png"):
            try:
                before, after = convert_png(png, args.dry_run)
            except Exception as error:  # noqa: BLE001
                print(f"skip {png}: {error}", file=sys.stderr)
                continue
            if before:
                pngs += 1
                total_before += before
                total_after += after
        freed, removed = prune_silent(folder, args.dry_run)
        pruned_bytes += freed
        pruned += len(removed)
        if args.dry_run and removed:
            for path in removed[:8]:
                print(f"  would remove {path}")
    verb = "would convert" if args.dry_run else "converted"
    print(f"{verb} {pngs} PNG stills: {total_before/1e9:.2f} GB" + ("" if args.dry_run else f" -> {total_after/1e9:.2f} GB"))
    print(f"{'would remove' if args.dry_run else 'removed'} {pruned} silent pictures / soundtracks: {pruned_bytes/1e9:.2f} GB")


if __name__ == "__main__":
    main()
