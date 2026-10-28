"""Before/after table of the enemy tank crews' fighting from canyon-ai-run receipts.

Usage: python tools/enemy_ai_compare.py artifacts/t34-ai-20261028 29 7 137
Reads <dir>/{before,after}/s<seed>_result.json (the receipt's final `enemy_ai` block) and prints
a Markdown table per metric (docs/ENEMY_TANK_AI.md).
"""
import json
import sys

ROWS = [
    ("outcome", lambda r, f, e: r["outcome"]),
    ("simulated s", lambda r, f, e: round(f["time_s"])),
    ("wall / simulated", lambda r, f, e: round(r["wall_seconds"] / max(f["time_s"], 1e-3), 2)),
    ("Tiger course progress", lambda r, f, e: round(f["progress"], 3)),
    ("Tiger alive", lambda r, f, e: f["alive"]),
    ("Tiger death cause", lambda r, f, e: f["death_cause"] or "-"),
    ("T-34s fielded", lambda r, f, e: e["tanks_fielded"]),
    ("T-34s that knew of the Tiger", lambda r, f, e: e["tanks_in_contact"]),
    ("first visual contact s", lambda r, f, e: rnd(e["first_contact_s"])),
    ("first T-34 shot s", lambda r, f, e: rnd(e["first_enemy_shot_s"])),
    ("first hit on Tiger s", lambda r, f, e: rnd(e["first_hit_on_tiger_s"])),
    ("tank-s in contact", lambda r, f, e: round(e["contact_tank_s"])),
    ("idle share in contact", lambda r, f, e: round(e["idle_in_contact_fraction"], 2)),
    ("stuck tank-s", lambda r, f, e: round(e["stuck_tank_s"])),
    ("distance driven m", lambda r, f, e: round(e["travelled_m"])),
    ("T-34 shots front/side/rear", lambda r, f, e: "/".join(map(str, e["shots_front_side_rear"]))),
    ("share of shots from flank or rear", lambda r, f, e: round(e["shots_flank_or_rear_fraction"], 2)),
    ("shots with a kill solution", lambda r, f, e: e["shots_with_kill_solution"]),
    ("hits on Tiger front/side/rear", lambda r, f, e: "/".join(map(str, e["hits_on_tiger_front_side_rear"]))),
    ("penetrations of the Tiger", lambda r, f, e: e["pens_on_tiger"]),
    ("Tiger hits on T-34s front/side/rear", lambda r, f, e: "/".join(map(str, e["tiger_hits_taken_front_side_rear"]))),
    ("T-34s knocked out", lambda r, f, e: e["tanks_knocked_out"]),
    ("Tiger shots", lambda r, f, e: f["shots"]),
]


def rnd(v):
    return "-" if v is None else round(v, 1)


def main():
    root, seeds = sys.argv[1], sys.argv[2:]
    runs = {}
    for side in ("before", "after"):
        for seed in seeds:
            r = json.load(open(f"{root}/{side}/s{seed}_result.json", encoding="utf-8"))
            runs[(side, seed)] = (r, r["final"], r["final"]["enemy_ai"])
    head = "| metric | " + " | ".join(f"s{s} before | s{s} after" for s in seeds) + " |"
    print(head)
    print("|" + "---|" * (1 + 2 * len(seeds)))
    for name, get in ROWS:
        cells = []
        for seed in seeds:
            for side in ("before", "after"):
                try:
                    cells.append(str(get(*runs[(side, seed)])))
                except (KeyError, TypeError):
                    cells.append("?")
        print(f"| {name} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
