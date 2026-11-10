#!/usr/bin/env python3
"""Set the fx-lab's old and new effects side by side (docs/FX_OVERHAUL_20261109.md).

    python3 tools/fx_lab_compare.py CAPTURE_PREFIX

CAPTURE_PREFIX is the full path prefix the `fx-lab` runs wrote (for example
/home/alex/panzer/captures/fx-overhaul-20261109-fxlab-20261109). For every scene with both an
`old` and a `new` receipt it writes, beside them:

  <prefix>-<scene>-old-left-new-right-<shot>.jpg   4096x1152 (each half 2x the 1024x576 grid)
  <prefix>-<scene>-old-left-new-right-<clip>.mp4   the two clips side by side, 30 fps

Each half is labelled with its variant and the scene's measured cost over its empty view.
"""
import json
import pathlib
import subprocess
import sys

FONT = "/usr/share/fonts/noto/NotoSansMono-Regular.ttf"


def cost_line(receipt):
    costs = receipt.get("costs", {})
    empty = next((v for k, v in costs.items() if k.endswith("_empty")), None)
    full = next((v for k, v in costs.items() if not k.endswith("_empty") and "readback" not in k), None)
    if not empty or not full:
        return ""
    dg = full["gpu_frame_mean_ms"] - empty["gpu_frame_mean_ms"]
    ds = full["simulation_mean_ms"] - empty["simulation_mean_ms"]
    return f"effects {dg:+.1f} ms GPU frame, {ds:+.1f} ms simulation (over the empty view)"


def label(variant, receipt):
    text = f"{variant.upper()}  {cost_line(receipt)}".replace(":", r"\:")
    return (f"drawtext=fontfile={FONT}:text='{text}':x=24:y=20:fontsize=34:fontcolor=white:"
            "box=1:boxcolor=black@0.6:boxborderw=10")


def run(args):
    subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-y", *args], check=True)


def main():
    prefix = pathlib.Path(sys.argv[1])
    folder, stem = prefix.parent, prefix.name
    for new_receipt in sorted(folder.glob(f"{stem}-*-new.json")):
        scene = new_receipt.name[len(stem) + 1:-len("-new.json")]
        old_receipt = new_receipt.with_name(f"{stem}-{scene}-old.json")
        if not old_receipt.exists():
            continue
        old, new = json.loads(old_receipt.read_text()), json.loads(new_receipt.read_text())
        base = f"{stem}-{scene}"
        for still in sorted(folder.glob(f"{base}-new-*.jpg")):
            shot = still.name[len(base) + len("-new-"):-4]
            other = folder / f"{base}-old-{shot}.jpg"
            if not other.exists():
                continue
            out = folder / f"{base}-old-left-new-right-{shot}.jpg"
            run(["-i", str(other), "-i", str(still), "-filter_complex",
                 f"[0]scale=2048:1152:flags=neighbor,{label('old', old)}[a];"
                 f"[1]scale=2048:1152:flags=neighbor,{label('new', new)}[b];[a][b]hstack",
                 "-q:v", "2", str(out)])
            print(out)
        for clip in sorted(folder.glob(f"{base}-new-*.mp4")):
            name = clip.name[len(base) + len("-new-"):-4]
            other = folder / f"{base}-old-{name}.mp4"
            if not other.exists():
                continue
            out = folder / f"{base}-old-left-new-right-{name}.mp4"
            run(["-i", str(other), "-i", str(clip), "-filter_complex",
                 f"[0]scale=2048:1152:flags=neighbor,{label('old', old)}[a];"
                 f"[1]scale=2048:1152:flags=neighbor,{label('new', new)}[b];[a][b]hstack=shortest=1",
                 "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p",
                 "-movflags", "+faststart", str(out)])
            print(out)


if __name__ == "__main__":
    main()
