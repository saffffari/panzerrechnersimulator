#!/usr/bin/env python3
"""Tabulate a directory of `gfxbench` logs (one per internal resolution) as Markdown.

    python tools/summarize_gfxbench.py DIR [--json OUT.json]

Each `gfxbench_<W>x<H>.log` gives the timed-frame statistics and the `[gpuprof]` per-pass means;
the optional `gfxbench_<W>x<H>.gpu_before.txt` beside it holds `nvidia-smi` utilisation and the
other GPU processes captured just before that point started.
"""
import argparse
import glob
import json
import os
import re
import sys


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("dir")
    ap.add_argument("--json")
    a = ap.parse_args()
    rows = []
    for path in glob.glob(os.path.join(a.dir, "gfxbench_*.log")):
        m = re.search(r"gfxbench_(\d+)x(\d+)\.log$", path)
        if not m:
            continue
        w, h = int(m.group(1)), int(m.group(2))
        txt = open(path, encoding="utf-8", errors="replace").read()
        fm = re.search(r"\[gfxbench\] (\d+) frames @ (\d+)x(\d+) \| avg ([\d.]+) ms \(([\d.]+) fps\) \| p50 ([\d.]+) \| p99 ([\d.]+) \| min ([\d.]+) max ([\d.]+)", txt)
        passes = {k: float(v) for k, v in re.findall(r"\[gpuprof\] (\S+)\s+([\d.]+) ms/frame", txt)}
        before = os.path.join(a.dir, f"gfxbench_{w}x{h}.gpu_before.txt")
        util = others = None
        if os.path.exists(before):
            lines = open(before, encoding="utf-8", errors="replace").read().splitlines()
            if lines:
                util = lines[0].strip()
            # `nvidia-smi --query-compute-apps` rows are "pid, path"; keep the games and captures.
            others = sorted({os.path.basename(l.split(",", 1)[1].strip()) for l in lines[1:]
                             if re.match(r"^\d+, ", l) and re.search(r"panzer|ksp|capture|\\target\\", l, re.I)})
        row = {"res": f"{w}x{h}", "pixels": w * h, "passes_ms": passes, "gpu_before": util, "other_gpu_apps": others}
        if fm:
            row.update({"frames": int(fm.group(1)), "avg_ms": float(fm.group(4)), "fps": float(fm.group(5)),
                        "p50_ms": float(fm.group(6)), "p99_ms": float(fm.group(7)), "min_ms": float(fm.group(8)),
                        "max_ms": float(fm.group(9)), "gpu_sum_ms": sum(passes.values())})
        else:
            row["error"] = (re.findall(r"panicked at[^\n]*\n[^\n]*", txt) or ["no [gfxbench] result line"])[0]
        rows.append(row)
    rows.sort(key=lambda r: r["pixels"])
    print("| internal | avg ms (fps) | p50 | p99 | max | GPU pass sum | rm-gbuffer | rm-sky | rm-sky-cover | rm-shadow-far | rm-shadow-near | rm-point-face ×12 | rm-shade | rm-volume | GPU before / other apps |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        if "avg_ms" not in r:
            print(f"| {r['res']} | failed: {r['error']} |" + " |" * 13)
            continue
        p = r["passes_ms"]
        cell = lambda k: f"{p.get(k, 0.0):.2f}"
        print(f"| {r['res']} | {r['avg_ms']:.2f} ({r['fps']:.1f}) | {r['p50_ms']:.2f} | {r['p99_ms']:.2f} | {r['max_ms']:.2f} | "
              f"{r['gpu_sum_ms']:.2f} | {cell('rm-gbuffer')} | {cell('rm-sky')} | {cell('rm-sky-cover')} | {cell('rm-shadow-far')} | "
              f"{cell('rm-shadow-near')} | {cell('rm-point-face')} | {cell('rm-shade')} | {cell('rm-volume')} | "
              f"{r['gpu_before']} / {', '.join(r['other_gpu_apps'] or []) or 'none'} |")
    if a.json:
        with open(a.json, "w", encoding="utf-8", newline="\n") as f:
            json.dump(rows, f, indent=1)


if __name__ == "__main__":
    main()
