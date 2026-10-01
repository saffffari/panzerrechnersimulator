#!/usr/bin/env python3
"""Package native wreck-fire comparisons, without post grading or asset-library delivery."""
import hashlib
import html
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CAP = ROOT / "captures"
BEFORE = "wreck_fire_before_20261001"
AFTER = "wreck_fire_review_20261001"
PLAY = "wreck_fire_gameplay_20261001"
COMPARE = "wreck_fire_comparison_20261001.mp4"


def main():
    before = json.loads((CAP / f"{BEFORE}.json").read_text())
    after = json.loads((CAP / f"{AFTER}.json").read_text())
    for key in ("fire_s", "strength", "fuel", "ignition_load"):
        assert before["state"][key] == after["state"][key], key
    for a, b in zip(before["shots"], after["shots"]):
        assert a["eye"] == b["eye"] and a["target"] == b["target"]
    assert before["renderer"] == after["renderer"] == "Shader"
    assert before["internal_resolution"] == after["internal_resolution"] == [640, 360]
    assert len(after["state"]["rendered_sources"]) == 1
    old = before["state"]["legacy_radiant_intensity"]
    new = after["state"]["rendered_sources"][0]["radiant_intensity"]
    reduction = (1 - new / old) * 100
    if not (CAP / COMPARE).exists():
        graph = (
            "[0:v]scale=960:540:flags=neighbor[a];"
            "[1:v]scale=960:540:flags=neighbor[b];"
            "[a][b]hstack=inputs=2,pad=1920:600:0:60:color=0x10131a,"
            "drawtext=fontfile=/usr/share/fonts/TTF/DejaVuSans.ttf:"
            "text='BEFORE':fontsize=28:fontcolor=white:x=24:y=17,"
            "drawtext=fontfile=/usr/share/fonts/TTF/DejaVuSans.ttf:"
            "text='REVISED - same wreck and camera':fontsize=28:fontcolor=white:x=984:y=17[v]"
        )
        subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-n",
            "-i", str(CAP / f"{BEFORE}.mp4"), "-i", str(CAP / f"{AFTER}.mp4"),
            "-filter_complex", graph, "-map", "[v]", "-an", "-c:v", "libx264",
            "-crf", "18", "-preset", "fast", "-pix_fmt", "yuv420p", "-movflags",
            "+faststart", str(CAP / COMPARE)], check=True)
    files = [COMPARE, f"{BEFORE}.mp4", f"{AFTER}.mp4", f"{PLAY}.mp4",
             f"{BEFORE}.json", f"{AFTER}.json", f"{PLAY}_result.json",
             f"{PLAY}_segments.json", f"{PLAY}_cuts.json"]
    for name in ("near", "reverse", "ground"):
        files += [f"{BEFORE}_{name}.png", f"{AFTER}_{name}.png"]
    files += [f"{AFTER}_deck.png", f"{PLAY}_cut00_preview.png", f"{PLAY}_cut01_preview.png"]
    assert all((CAP / name).is_file() for name in files)
    videos = [(COMPARE, "Matched comparison · 12 seconds · silent"),
              (f"{AFTER}.mp4", "Revised wreck · 12 seconds · silent staged fixture"),
              (f"{PLAY}.mp4", "Actual first-person Canyon footage · 24 seconds · native audio"),
              (f"{BEFORE}.mp4", "Original wreck · 12 seconds · silent staged fixture")]
    player = lambda name, label: f'<figure><video controls preload="metadata" src="{name}"></video><figcaption>{html.escape(label)}</figcaption></figure>'
    panels = "".join(player(*item) for item in videos)
    for name in ("ground", "near", "reverse"):
        panels += f'<h2>{name.title()} view</h2><div class="pair">'
        for prefix, label in ((BEFORE, "Before"), (AFTER, "Revised")):
            file = f"{prefix}_{name}.png"
            panels += f'<figure><a href="{file}"><img loading="lazy" src="{file}"></a><figcaption>{label}</figcaption></figure>'
        panels += '</div>'
    panels += f'<h2>Revised engine-deck fire</h2><a href="{AFTER}_deck.png"><img src="{AFTER}_deck.png"></a>'
    for i in range(2):
        file = f"{PLAY}_cut{i:02}_preview.png"
        panels += f'<h2>Actual first-person frame {i + 1}</h2><a href="{file}"><img src="{file}"></a>'
    page = f'''<!doctype html><html lang="en"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>T-34 fire review</title>
<style>body{{background:#10131a;color:#eee;font:17px/1.5 system-ui;margin:0 auto;max-width:1400px;padding:24px}}h1{{font-size:34px}}h2{{font-size:23px;margin-top:36px}}p{{max-width:920px;color:#c9cbd0}}video,img{{display:block;width:100%;height:auto;background:#090b0f}}figure{{margin:18px 0}}figcaption{{padding:10px 0;color:#c9cbd0}}.pair{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}a{{color:#e9ab68}}@media(max-width:700px){{.pair{{grid-template-columns:1fr}}}}</style>
<h1>T-34 fire: larger flames, smaller light footprint</h1>
<p>The surface renderer uses physically based material response. The old wreck effect added an independent unshadowed light. The revised effect derives its source from the actual flame cells and adds shadows from the wreck and nearby walls.</p>
<p>In the matched fixture, aggregate source intensity fell from {old:.3f} to {new:.3f}: {reduction:.1f}% lower. These are renderer units, not measured fire power. Larger rising tongues retain the fine cube style.</p>
<p>The comparison stages one burning wreck at the same position, damage, fuel, time, weather and cameras. Each camera's normal eye exposure is allowed to settle independently. Observer footage is a lighting fixture. The separate first-person clip comes from the actual seeded Canyon attempt; the Tiger still stalls at the industrial obstruction and does not finish the course.</p>
<p>640×360 native rendering. Flame motion and emission remain authored; the raster shadows use a bounded 8 m cubemap. This is not a combustion simulation or full path tracing. No post color grading.</p>
{panels}</html>'''
    page_name = f"{AFTER}.html"
    (CAP / page_name).write_text(page)
    files += [page_name]
    evidence = {"schema": "panzer.wreck-fire-delivery.v1", "intensity_before": old,
                "intensity_after": new, "intensity_reduction_percent": reduction,
                "matched_state": True, "matched_cameras": True,
                "binary_sha256": hashlib.sha256((ROOT / "target/release/panzerrechnersystem").read_bytes()).hexdigest(),
                "world_manifest_sha256": hashlib.sha256(Path(after["manifest"]).read_bytes()).hexdigest(),
                "world_edits_sha256": hashlib.sha256(Path(after["manifest"]).with_name("manifest_edits.json").read_bytes()).hexdigest(),
                "files": [{"name": name, "bytes": (CAP / name).stat().st_size,
                           "sha256": hashlib.sha256((CAP / name).read_bytes()).hexdigest()} for name in files]}
    receipt = f"{AFTER}_evidence.json"
    (CAP / receipt).write_text(json.dumps(evidence, indent=2) + "\n")
    files += [receipt]
    (CAP / f"{AFTER}_files.txt").write_text("\n".join(files) + "\n")
    (CAP / f"{AFTER}_SHA256SUMS").write_text("".join(
        f"{hashlib.sha256((CAP / name).read_bytes()).hexdigest()}  {name}\n" for name in files))
    print(f"{len(files)} content files, {sum((CAP / name).stat().st_size for name in files):,} bytes")


if __name__ == "__main__":
    main()
