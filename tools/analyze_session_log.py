#!/usr/bin/env python3
"""Summarise a KESSEL_SESSION_LOG run (src/session_log.rs) for a performance report.

    python tools/analyze_session_log.py RUN.jsonl[.gz] [--console RUN.console.log] [--memory RUN.memory.csv]
        [--json OUT.json] [--label NAME]

Prints Markdown: frame-time percentiles over time (per minute), the worst five seconds with the
gameplay snapshot of each, the fenced GPU pass table ranked by mean cost, the CPU simulation /
render-submission split, memory (when a sampler CSV is given), and gameplay data read from the
snapshots: time to first contact, hits taken and dealt, reload times, deaths and causes, mission
beat completion times, stalls.

Percentiles across a run are taken over the per-second records (the log keeps per-second
statistics, not every frame): "p99 of per-second p99" is the 99th percentile of the seconds'
own p99 values. The overall mean frame time is exact (frame-count weighted).
"""
import argparse
import csv
import gzip
import json
import math
import re
import statistics
import sys
from collections import defaultdict


def pct(values, q):
    if not values:
        return float("nan")
    values = sorted(values)
    return values[min(len(values) - 1, max(0, round((len(values) - 1) * q)))]


def load(path):
    lines = []
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as f:
        for raw in f:
            raw = raw.strip()
            if raw:
                try:
                    lines.append(json.loads(raw))
                except json.JSONDecodeError:
                    pass
    return lines


def g(d, *keys, default=None):
    for k in keys:
        if not isinstance(d, dict) or k not in d or d[k] is None:
            return default
        d = d[k]
    return d


def fmt(v, digits=2):
    if v is None or (isinstance(v, float) and math.isnan(v)):
        return "—"
    if isinstance(v, float):
        return f"{v:.{digits}f}"
    return str(v)


def snapshot_line(rec):
    game = rec.get("game") or {}
    extras = g(rec, "render", "extras", default={}) or {}
    rm = extras.get("raymarch") or {}
    weather = extras.get("weather") or {}
    combat = game.get("combat") or {}
    run = game.get("run") or {}
    bits = []
    near = combat.get("nearest_enemy_vehicle_m")
    bits.append(f"enemy {near:.0f} m" if near is not None else "no enemy vehicle")
    bits.append(f"{combat.get('living_enemy_vehicles', 0)} live AFV")
    if combat.get("projectiles_in_flight"):
        bits.append(f"{combat['projectiles_in_flight']} shells in flight")
    if run.get("live_blasts"):
        bits.append(f"{run['live_blasts']} live blasts")
    if rm:
        bits.append(f"rm events {rm.get('events', 0)}, sparks {rm.get('spark_records', 0)}, "
                    f"parcels {rm.get('parcels', 0)}, lights {rm.get('lights', 0)}")
    bits.append(f"lights offered {g(rec, 'render', 'point_lights', 'offered', default='?')}")
    if weather:
        bits.append(f"storm {weather.get('storm', 0):.2f}")
    bits.append(f"station {game.get('station')}")
    if game.get("optic_composite"):
        bits.append("optic")
    beat = run.get("mission_beat")
    if beat:
        bits.append(f"beat {beat[0]} '{beat[1]}'")
    if run.get("death_cause"):
        bits.append(f"DEAD: {run['death_cause']}")
    return "; ".join(bits)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("log")
    ap.add_argument("--console")
    ap.add_argument("--memory")
    ap.add_argument("--json")
    ap.add_argument("--label", default="")
    a = ap.parse_args()
    recs = load(a.log)
    if not recs:
        sys.exit(f"no records in {a.log}")
    # Seconds of actual play: the canyon clock is running. Loading/brief/card seconds are kept
    # out of the frame statistics but counted.
    play = [r for r in recs if g(r, "game", "canyon", "elapsed_s") is not None and g(r, "frame_ms", "n")]
    if not play:
        play = [r for r in recs if g(r, "frame_ms", "n")]
    out = {"label": a.label, "log": a.log, "lines": len(recs), "play_lines": len(play)}
    p = print
    p(f"### {a.label or a.log}\n")
    p(f"Session log `{a.log}`: {len(recs)} one-second lines, {len(play)} with the Canyon clock running.\n")

    # ---- overall -------------------------------------------------------------------------------
    n_tot = sum(r["frame_ms"]["n"] for r in play)
    mean_ft = sum(r["frame_ms"]["mean"] * r["frame_ms"]["n"] for r in play) / max(1, n_tot)
    sec_p50 = [r["frame_ms"]["p50"] for r in play]
    sec_p95 = [r["frame_ms"]["p95"] for r in play]
    sec_p99 = [r["frame_ms"]["p99"] for r in play]
    sec_max = [r["frame_ms"]["max"] for r in play]
    fps = [r["fps"] for r in play]
    sim = [r["cpu_sim_ms"]["mean"] for r in play if r.get("cpu_sim_ms")]
    sub = [r["cpu_render_submit_ms"]["mean"] for r in play if r.get("cpu_render_submit_ms")]
    sim_p99 = [r["cpu_sim_ms"]["p99"] for r in play if r.get("cpu_sim_ms")]
    sub_p99 = [r["cpu_render_submit_ms"]["p99"] for r in play if r.get("cpu_render_submit_ms")]
    overall = {
        "frames": n_tot, "seconds": len(play), "mean_frame_ms": mean_ft, "mean_fps": 1000.0 / mean_ft,
        "median_second_fps": statistics.median(fps), "p05_second_fps": pct(fps, 0.05),
        "p50_of_p50_ms": pct(sec_p50, 0.5), "p95_of_p95_ms": pct(sec_p95, 0.95),
        "p99_of_p99_ms": pct(sec_p99, 0.99), "max_ms": max(sec_max),
        "seconds_over_33ms_p99": sum(1 for v in sec_p99 if v > 33.3),
        "seconds_over_16ms_p50": sum(1 for v in sec_p50 if v > 16.7),
        "cpu_sim_mean_ms": statistics.fmean(sim) if sim else None,
        "cpu_sim_p99_of_p99_ms": pct(sim_p99, 0.99) if sim_p99 else None,
        "cpu_submit_mean_ms": statistics.fmean(sub) if sub else None,
        "cpu_submit_p99_of_p99_ms": pct(sub_p99, 0.99) if sub_p99 else None,
    }
    out["overall"] = overall
    res = g(play[0], "render", "internal_resolution")
    surf = g(play[0], "render", "surface")
    p(f"Internal grid {res}, surface {surf}. {n_tot} frames over {len(play)} s.\n")
    p("| metric | value |\n|---|---|")
    p(f"| mean frame time (exact) | {mean_ft:.2f} ms ({1000/mean_ft:.1f} fps) |")
    p(f"| median second fps / 5th-percentile second fps | {overall['median_second_fps']:.1f} / {overall['p05_second_fps']:.1f} |")
    p(f"| p50 of per-second p50 | {overall['p50_of_p50_ms']:.2f} ms |")
    p(f"| p95 of per-second p95 | {overall['p95_of_p95_ms']:.2f} ms |")
    p(f"| p99 of per-second p99 | {overall['p99_of_p99_ms']:.2f} ms |")
    p(f"| worst single frame | {overall['max_ms']:.1f} ms |")
    p(f"| seconds with p99 > 33.3 ms | {overall['seconds_over_33ms_p99']} |")
    p(f"| seconds with p50 > 16.7 ms | {overall['seconds_over_16ms_p50']} |")
    p(f"| CPU simulation mean / p99-of-p99 | {fmt(overall['cpu_sim_mean_ms'])} / {fmt(overall['cpu_sim_p99_of_p99_ms'])} ms |")
    p(f"| CPU render submission mean / p99-of-p99 | {fmt(overall['cpu_submit_mean_ms'])} / {fmt(overall['cpu_submit_p99_of_p99_ms'])} ms |")
    if sim and sub:
        rest = mean_ft - overall["cpu_sim_mean_ms"] - overall["cpu_submit_mean_ms"]
        p(f"| remainder of the frame (acquire/present/GPU wait/UI) | {rest:.2f} ms |")
    p("")

    # ---- per minute ----------------------------------------------------------------------------
    p("Frame time over the run (per minute of play; per-second statistics pooled):\n")
    p("| minute | fps mean | p50 ms | p95 ms | p99 ms | max ms | sim ms | submit ms | storm | enemies ≤420 m (s) |")
    p("|---|---|---|---|---|---|---|---|---|---|")
    minutes = defaultdict(list)
    t0 = play[0]["t_s"]
    for r in play:
        minutes[int((r["t_s"] - t0) // 60)].append(r)
    per_min = []
    for m in sorted(minutes):
        rs = minutes[m]
        n = sum(r["frame_ms"]["n"] for r in rs)
        mft = sum(r["frame_ms"]["mean"] * r["frame_ms"]["n"] for r in rs) / max(1, n)
        storm = [g(r, "render", "extras", "weather", "storm", default=0.0) for r in rs]
        contact = sum(1 for r in rs if (g(r, "game", "combat", "nearest_enemy_vehicle_m") or 1e9) <= 420)
        row = {"minute": m, "fps": 1000 / mft, "p50": pct([r["frame_ms"]["p50"] for r in rs], 0.5),
               "p95": pct([r["frame_ms"]["p95"] for r in rs], 0.95), "p99": pct([r["frame_ms"]["p99"] for r in rs], 0.99),
               "max": max(r["frame_ms"]["max"] for r in rs),
               "sim": statistics.fmean([r["cpu_sim_ms"]["mean"] for r in rs if r.get("cpu_sim_ms")] or [float("nan")]),
               "submit": statistics.fmean([r["cpu_render_submit_ms"]["mean"] for r in rs if r.get("cpu_render_submit_ms")] or [float("nan")]),
               "storm": statistics.fmean(storm), "contact_s": contact}
        per_min.append(row)
        p(f"| {m} | {row['fps']:.1f} | {row['p50']:.2f} | {row['p95']:.2f} | {row['p99']:.2f} | {row['max']:.1f} | "
          f"{row['sim']:.2f} | {row['submit']:.2f} | {row['storm']:.2f} | {contact} |")
    out["per_minute"] = per_min
    p("")

    # ---- worst five seconds --------------------------------------------------------------------
    worst = sorted(play, key=lambda r: r["frame_ms"]["p99"], reverse=True)[:5]
    p("Worst five seconds (by that second's p99 frame time) and what the snapshot says was happening:\n")
    p("| play time | fps | mean / p99 / max ms | sim mean / p99 | submit mean / p99 | GPU pass sum (if fenced) | snapshot |")
    p("|---|---|---|---|---|---|---|")
    out["worst"] = []
    for r in worst:
        el = g(r, "game", "canyon", "elapsed_s")
        line = snapshot_line(r)
        out["worst"].append({"t_s": r["t_s"], "elapsed_s": el, "fps": r["fps"], "frame": r["frame_ms"],
                             "sim_p99": g(r, "cpu_sim_ms", "p99"), "submit_p99": g(r, "cpu_render_submit_ms", "p99"),
                             "snapshot": line})
        p(f"| {fmt(el, 0)} s | {r['fps']:.1f} | {r['frame_ms']['mean']:.1f} / {r['frame_ms']['p99']:.1f} / {r['frame_ms']['max']:.1f} | "
          f"{fmt(g(r, 'cpu_sim_ms', 'mean'))} / {fmt(g(r, 'cpu_sim_ms', 'p99'))} | "
          f"{fmt(g(r, 'cpu_render_submit_ms', 'mean'))} / {fmt(g(r, 'cpu_render_submit_ms', 'p99'))} | "
          f"{fmt(g(r, 'gpu', 'pass_sum_ms'))} | {line} |")
    p("")

    # ---- GPU passes -----------------------------------------------------------------------------
    passes = defaultdict(list)
    fenced = []
    sums = []
    for r in recs:
        gpu = r.get("gpu")
        if not gpu:
            continue
        fenced.append(gpu.get("fenced_frame_wall_ms") or float("nan"))
        sums.append(gpu.get("pass_sum_ms") or 0.0)
        # A label can repeat within one frame (12 point-light faces, glare levels, a second
        # scene render for an optic): sum it per frame first.
        frame = defaultdict(float)
        for label, ms in gpu.get("passes_ms", []):
            frame[label] += ms
        for label, ms in frame.items():
            passes[label].append(ms)
    out["gpu_samples"] = len(sums)
    if sums:
        nsamp = len(sums)
        rows = []
        for label, v in passes.items():
            total = sum(v) / nsamp  # mean over all fenced samples (absent counts as 0)
            med = pct(v + [0.0] * (nsamp - len(v)), 0.5)
            rows.append((label, total, med, pct(v, 0.95), max(v), len(v)))
        # Ranked by the median frame: on a time-sliced GPU a few fenced frames are preempted and
        # their single-pass times (tens of ms) dominate a mean.
        rows.sort(key=lambda x: (x[2], x[1]), reverse=True)
        mean_sum = statistics.fmean(sums)
        med_sum = pct(sums, 0.5)
        p(f"GPU passes from {nsamp} fenced frames (pass sum: median {med_sum:.2f} ms, mean {mean_sum:.2f} ms; "
          f"fenced frame wall median {pct([x for x in fenced if not math.isnan(x)], 0.5):.2f} ms). "
          f"Ranked by median cost per frame:\n")
        p("| rank | pass | median ms/frame | share of median sum | mean | p95 | max | frames |")
        p("|---|---|---|---|---|---|---|---|")
        for i, (label, tot, med, p95, mx, cnt) in enumerate(rows[:25], 1):
            p(f"| {i} | `{label}` | {med:.3f} | {100*med/max(med_sum,1e-9):.1f} % | {tot:.3f} | {p95:.3f} | {mx:.3f} | {cnt}/{nsamp} |")
        out["gpu_passes"] = [{"pass": r[0], "mean_ms": r[1], "median_ms": r[2], "p95_ms": r[3], "max_ms": r[4], "present": r[5]} for r in rows]
        out["gpu_pass_sum_mean_ms"] = mean_sum
        out["gpu_pass_sum_median_ms"] = med_sum
        p("")

    # ---- simulation phases (game.sim_phases_ms: mean ms per Game::update call) ------------------
    def band(r):
        n = g(r, "game", "combat", "nearest_enemy_vehicle_m")
        if n is None:
            return "no enemy vehicle"
        return "enemy < 150 m" if n < 150 else "enemy 150-420 m" if n < 420 else "enemy > 420 m"
    phased = [r for r in play if g(r, "game", "sim_phases_ms")]
    if phased:
        names = [k for k in phased[0]["game"]["sim_phases_ms"] if k != "updates"]
        groups = defaultdict(list)
        for r in phased:
            groups["all"].append(r)
            groups[band(r)].append(r)
        p("CPU simulation by phase (ms per presented frame = mean per update × updates ÷ frames), "
          "split by the nearest enemy vehicle's range:\n")
        p("| seconds | " + " | ".join(names) + " | other | cpu_sim total |")
        p("|---|" + "---|" * (len(names) + 2))
        out["sim_phases"] = {}
        for key in ["all", "no enemy vehicle", "enemy > 420 m", "enemy 150-420 m", "enemy < 150 m"]:
            rs = groups.get(key)
            if not rs:
                continue
            per = {k: statistics.fmean(r["game"]["sim_phases_ms"][k] * r["game"]["sim_phases_ms"]["updates"] / max(1, r["frame_ms"]["n"]) for r in rs) for k in names}
            total = statistics.fmean(r["cpu_sim_ms"]["mean"] for r in rs)
            # "enemy_ai.stealth" etc. are inside enemy_ai: only top-level phases add up.
            other = total - sum(v for k, v in per.items() if "." not in k)
            out["sim_phases"][key] = {"seconds": len(rs), **per, "other": other, "cpu_sim": total}
            p(f"| {key} ({len(rs)} s) | " + " | ".join(f"{per[k]:.2f}" for k in names) + f" | {other:.2f} | {total:.2f} |")
        p("")

    # ---- render work ----------------------------------------------------------------------------
    rm_keys =["lights", "shadowed_lights", "parcels", "cell_items", "embers", "spark_records", "tracer_records", "events", "prepare_ms", "encode_ms"]
    rmv = defaultdict(list)
    for r in play:
        rm = g(r, "render", "extras", "raymarch")
        if rm:
            for k in rm_keys:
                rmv[k].append(rm.get(k, 0))
    offered = [g(r, "render", "point_lights", "offered", default=0) for r in play]
    inst = [g(r, "render", "instances", "live", default=0) for r in play]
    if rmv:
        p("Raymarch work per frame (sampled once a second):\n")
        p("| counter | median | p95 | max |\n|---|---|---|---|")
        for k in rm_keys:
            p(f"| {k} | {fmt(pct(rmv[k],0.5))} | {fmt(pct(rmv[k],0.95))} | {fmt(max(rmv[k]))} |")
        p(f"| point lights offered | {pct(offered,0.5)} | {pct(offered,0.95)} | {max(offered)} |")
        p(f"| live instances | {pct(inst,0.5)} | {pct(inst,0.95)} | {max(inst)} |")
        p("")
        out["raymarch_work"] = {k: {"p50": pct(v, 0.5), "p95": pct(v, 0.95), "max": max(v)} for k, v in rmv.items()}

    # ---- memory -------------------------------------------------------------------------------
    if a.memory:
        with open(a.memory, newline="") as f:
            mem = list(csv.DictReader(f))
        if mem:
            ws = [float(m["working_set_mb"]) for m in mem]
            pv = [float(m["private_mb"]) for m in mem]
            vr = [float(m["gpu_mem_used_mb"]) for m in mem if m.get("gpu_mem_used_mb") not in (None, "", "NA")]
            p("Memory (process sampler every 5 s; GPU memory is the whole board, shared with other processes):\n")
            p("| | start | median | peak |\n|---|---|---|---|")
            p(f"| working set | {ws[0]:.0f} MB | {pct(ws,0.5):.0f} MB | {max(ws):.0f} MB |")
            p(f"| private bytes | {pv[0]:.0f} MB | {pct(pv,0.5):.0f} MB | {max(pv):.0f} MB |")
            if vr:
                p(f"| board VRAM used | {vr[0]:.0f} MB | {pct(vr,0.5):.0f} MB | {max(vr):.0f} MB |")
            p("")
            out["memory"] = {"working_set_mb": [ws[0], pct(ws, 0.5), max(ws)], "private_mb": [pv[0], pct(pv, 0.5), max(pv)],
                             "board_vram_mb": [vr[0], pct(vr, 0.5), max(vr)] if vr else None}

    # ---- gameplay ------------------------------------------------------------------------------
    gp = {}
    def el(r):
        return g(r, "game", "canyon", "elapsed_s")
    first_contact = next((el(r) for r in play if (g(r, "game", "combat", "nearest_enemy_vehicle_m") or 1e9) <= 420), None)
    first_shot = next((el(r) for r in play if (g(r, "game", "run", "main_gun_shots") or 0) > 0), None)
    first_hit_taken = next((el(r) for r in play if (g(r, "game", "run", "hits_on_player") or 0) > 0), None)
    last = play[-1]
    lr = g(last, "game", "run", default={}) or {}
    gp.update({"first_enemy_vehicle_within_420m_s": first_contact, "first_main_gun_shot_s": first_shot,
               "first_hit_taken_s": first_hit_taken, "main_gun_shots": lr.get("main_gun_shots"),
               "hits_dealt": lr.get("hits_by_player"), "kills": lr.get("kills_by_player"),
               "hits_taken": lr.get("hits_on_player"), "penetrations_taken": lr.get("penetrations_on_player"),
               "death_cause": next((g(r, "game", "run", "death_cause") for r in recs if g(r, "game", "run", "death_cause")), None),
               "finished_s": next((g(r, "game", "canyon", "finished_s") for r in recs if g(r, "game", "canyon", "finished_s")), None),
               "final_goal_distance_m": g(last, "game", "canyon", "goal_distance_m"),
               "modules_out": g(last, "game", "tiger", "modules_out")})
    # Reloads: breech leaves Loaded (the shot) and returns to Loaded.
    reloads = []
    shot_at = None
    prev = None
    for r in play:
        b = g(r, "game", "ammunition", "breech")
        if b is None:
            continue
        loaded = b.startswith("Loaded") or b.startswith("Round")
        if prev is not None and prev and not loaded:
            shot_at = el(r)
        if prev is not None and not prev and loaded and shot_at is not None:
            reloads.append(el(r) - shot_at)
            shot_at = None
        prev = loaded
    gp["reload_s"] = reloads
    # Mission beats.
    beats = []
    cur = None
    for r in recs:
        b = g(r, "game", "run", "mission_beat")
        key = tuple(b) if b else None
        if key != cur:
            beats.append({"elapsed_s": el(r), "beat": b})
            cur = key
    gp["beat_changes"] = beats
    # Stalls: 20+ consecutive seconds under 0.3 m/s with the Canyon clock running and no enemy in 420 m.
    stalls = []
    run_start = None
    for r in play:
        slow = abs(g(r, "game", "tiger", "speed_m_s", default=0.0)) < 0.3
        quiet = (g(r, "game", "combat", "nearest_enemy_vehicle_m") or 1e9) > 420
        if slow and quiet:
            if run_start is None:
                run_start = r
        else:
            if run_start is not None and el(r) - el(run_start) >= 20:
                stalls.append({"from_s": el(run_start), "to_s": el(r), "at": g(run_start, "game", "tiger", "position")})
            run_start = None
    if run_start is not None and el(play[-1]) - el(run_start) >= 20:
        stalls.append({"from_s": el(run_start), "to_s": el(play[-1]), "at": g(run_start, "game", "tiger", "position")})
    gp["stalls"] = stalls
    if a.console:
        opener = gzip.open if a.console.endswith(".gz") else open
        with opener(a.console, "rt", encoding="utf-8", errors="replace") as f:
            txt = f.read()
        gp["tank_stopped_lines"] = len(re.findall(r"TANK STOPPED", txt))
        m = re.search(r"\[autopilot-result\] (\{.*\})", txt)
        gp["autopilot_result"] = json.loads(m.group(1)) if m else None
        m = re.search(r"\[canyon\] after-action: (.*)", txt)
        gp["after_action"] = m.group(1)[:400] if m else None
        gp["stage_timeouts"] = len(re.findall(r"TIMEOUT", txt))
        gp["panics"] = re.findall(r"panicked at[^\n]*", txt)[:3]
    out["gameplay"] = gp
    p("Gameplay from the snapshots:\n")
    p("| | |\n|---|---|")
    for k, v in gp.items():
        if k in ("beat_changes",):
            continue
        if k == "reload_s":
            v = (f"{len(v)} reloads; median {statistics.median(v):.1f} s, max {max(v):.1f} s" if v else "none")
        p(f"| {k} | {v} |")
    p("\nMission beat changes (Canyon clock):\n")
    for b in beats:
        p(f"- {fmt(b['elapsed_s'], 1)} s: {b['beat']}")
    p("")
    if a.json:
        with open(a.json, "w", encoding="utf-8", newline="\n") as f:
            json.dump(out, f, indent=1)


if __name__ == "__main__":
    main()
