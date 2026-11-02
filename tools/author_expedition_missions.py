#!/usr/bin/env python3
"""Author the mission sheets of The March (catalog/expedition_missions.v1.json).

One `kessel.mission` sheet per theatre site, on the Canyon run's model (docs/MISSIONS.md): a
written order of three lines, then beats with their net traffic, map marks and cues. The prose is
authored here, per site and per kind of order; the geography is derived from the theatre source
(catalog/expedition_theatre.v1.json): the approach point lies on the road network 700 m short of
the site along the shortest road path from the support detail's first camp, and the distances and
bearings quoted on the net are measured from that camp. Re-run after editing either file:

    python3 tools/author_expedition_missions.py

The sheets name only places the theatre has (site positions and road knots). Nothing here spawns,
moves or alerts an enemy: the beats observe what the theatre's own rules already track (the site's
guard tanks, its hold clock, the counterattack it releases) and say so on the net.
"""
import heapq
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "catalog/expedition_theatre.v1.json"
TARGET = ROOT / "catalog/expedition_missions.v1.json"
APPROACH_M = 700.0
CALLSIGN, NET = "Elster", "Stab"
HEADING = "Unterstützungstrupp  ·  schriftlicher Befehl"
STAKES = "The field is persistent: what you leave stays, what you lose is not replaced."


def compass(dx, dz):
    # Engine x runs east, z runs south (the map is drawn north up with +z downward).
    names = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"]
    angle = math.degrees(math.atan2(dx, -dz)) % 360.0
    return names[int((angle + 22.5) // 45) % 8], angle


def km(metres):
    return f"{metres / 1000:.1f} km"


def road_graph(spec):
    knots, edges = [], {}

    def knot(p):
        for i, q in enumerate(knots):
            if math.dist(p, q) < 1.0:
                return i
        knots.append(tuple(p))
        edges[len(knots) - 1] = set()
        return len(knots) - 1

    for road in spec["roads"]:
        ids = [knot(p) for p in road]
        for a, b in zip(ids, ids[1:]):
            edges[a].add(b)
            edges[b].add(a)
    return knots, edges


def nearest_knot(knots, p):
    return min(range(len(knots)), key=lambda i: math.dist(knots[i], p))


def shortest_path(knots, edges, start, goal):
    dist = {start: 0.0}
    prev = {}
    heap = [(0.0, start)]
    while heap:
        d, u = heapq.heappop(heap)
        if u == goal:
            break
        if d > dist.get(u, math.inf):
            continue
        for v in edges[u]:
            nd = d + math.dist(knots[u], knots[v])
            if nd < dist.get(v, math.inf):
                dist[v] = nd
                prev[v] = u
                heapq.heappush(heap, (nd, v))
    path = [goal]
    while path[-1] != start:
        path.append(prev[path[-1]])
    return [knots[i] for i in reversed(path)]


def point_before_end(path, back_m):
    """The point `back_m` before the path's end, by arc length (or 60 % along a short path)."""
    total = sum(math.dist(a, b) for a, b in zip(path, path[1:]))
    back = min(back_m, total * 0.4)
    remaining = back
    for a, b in zip(reversed(path[1:]), reversed(path[:-1])):
        seg = math.dist(a, b)
        if seg >= remaining:
            t = remaining / seg
            return (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t), total
        remaining -= seg
    return path[0], total


# Per-site prose: where it is (for the written order), the ground as the detail reports it, and
# the approach the net names. The fiction is a fictional compressed theatre (the theatre source's
# provenance line); the order kinds are recurring operational tasks, not named battles.
SITES = {
    10: dict(where="the Hagen ammunition yard, on the camp road east of the detail",
             ground="the shell carts stand in the open beside the road",
             approach="east along the camp road"),
    11: dict(where="the Birken fuel stop, south of the Hagen yard on the Birken road",
             ground="the fuel cans are stacked beside the road",
             approach="south from the Hagen yard along the Birken road"),
    12: dict(where="the Kessel repair works, east of the Hagen yard on the camp road",
             ground="the parts we need are laid out beside the road",
             approach="east along the camp road past the Hagen yard"),
    13: dict(where="the southern crossing of the river, east of the repair works",
             ground="a concrete road bridge; the far bank is theirs",
             approach="east along the camp road to the west abutment"),
    14: dict(where="the broken column, on the far bank south-east of the southern crossing",
             ground="one of ours is alive and on foot by the road",
             approach="over the southern crossing and south-east along the road"),
    15: dict(where="Ostfeld village, north-east of the broken column",
             ground="a road junction held in strength",
             approach="north-east along the road from the broken column"),
    16: dict(where="the grain depot, south of Ostfeld on the eastern road",
             ground="the shell carts stand by the road junction, held in strength",
             approach="south along the eastern road from Ostfeld"),
    17: dict(where="the relay station, on the far bank south of the broken column",
             ground="a dispatch table with its ledger, in the open by the road",
             approach="south along the road from the broken column"),
    18: dict(where="the northern relief, far north on the Birken road past Linden junction",
             ground="one of ours is alive and on foot by the road",
             approach="north along the Birken road through Linden junction"),
    19: dict(where="Linden junction, north of the Birken fuel stop",
             ground="a crossroads in open ground; expect a counterattack from the north",
             approach="north along the Birken road from the fuel stop"),
    20: dict(where="the northern crossing of the river, east of Linden junction",
             ground="a concrete road bridge; their armour is on the far bank",
             approach="east along the northern road from Linden junction"),
    21: dict(where="the last corridor, far south-east of the relay station",
             ground="the blocking force sits across the road out; the column waits on you",
             approach="south along the road from the relay station"),
}

# The task line per kind, the same words as `expedition_theatre::Site::task` (src/expedition_theatre.rs).
TASKS = {
    "ammunition": "Clear the yard. Secure the shell supply, then load what you can carry.",
    "fuel": "Clear the depot. Collect its finite fuel cans and refill the Tiger or transport.",
    "workshop": "Secure the workshop. Inspect and collect its surviving components.",
    "bridge": "Occupy the crossing and hold it against the approaching counterattack.",
    "rescue": "Reach the disabled column. Defeat its guards and evacuate the surviving crew.",
    "assault": "Take the settlement. Clear the defenders and hold the road junction.",
    "dispatch": "Reach the relay station on foot and acknowledge the written order in its dispatch ledger.",
    "defence": "Secure the junction and defeat the approaching armoured counterattack.",
    "finale": "Open the last corridor. Break the blocking force, then hold the exit.",
}

SHORT = {10: "the yard", 11: "the depot", 12: "the works", 13: "the crossing", 14: "the column",
         15: "the village", 16: "the depot", 17: "the relay", 18: "the relief", 19: "the junction",
         20: "the crossing", 21: "the corridor"}


def cap(short):
    """Sentence-initial form of a short place name: 'the yard' -> 'Yard'."""
    word = short[4:] if short.startswith("the ") else short
    return word[0].upper() + word[1:]


def armour(n):
    return "one T-34" if n == 1 else f"{['', '', 'two', 'three'][n]} T-34"


def secure_beat(site, short, hold):
    kind = site["kind"]
    rewards = site["rewards"]
    supplies = []
    if any(r.startswith("ammunition") for r in rewards):
        supplies.append("88 mm AP and HE on the carts")
    if "fuel.petrol" in rewards:
        supplies.append("petrol in cans")
    if any(r.startswith("workshop") for r in rewards):
        parts = [r.split(".", 1)[1].replace("_", " ") for r in rewards if r.startswith("workshop")]
        supplies.append("parts: " + ", ".join(parts))
    supply_line = ("; ".join(supplies) + ".") if supplies else ""
    hold_s = f"{hold:.0f} seconds"
    text = {
        "ammunition": (f"Clear the sentries and hold {short} {hold_s} for the loaders. {supply_line}",
                       f"{cap(short)} secured. Loading."),
        "fuel": (f"Clear the sentries and hold {short} {hold_s}. {supply_line} Refill the Tiger before you leave.",
                 f"{cap(short)} secured. Refuelling."),
        "workshop": (f"Clear the sentries and hold {short} {hold_s}. {supply_line} The detail moves up behind you.",
                     f"{cap(short)} secured. Detail moving up."),
        "bridge": (f"Occupy {short} and hold it {hold_s}. Their armour will come at you from the far bank once you are on it.",
                   f"{cap(short)} held. Counterattack beaten."),
        "rescue": (f"One of ours is alive on foot by the wrecks. Bring him to the medic; the order stands until he is on a bed. Hold {short} {hold_s}.",
                   f"He is with the medic. {cap(short)} secured."),
        "assault": (f"Clear the sentries and hold the junction {hold_s}. {supply_line}",
                    f"{cap(short)} held."),
        "dispatch": (f"Dismount at the hut and acknowledge the order in the dispatch ledger, then hold {short} {hold_s}.",
                     f"Order acknowledged in the ledger. {cap(short)} secured."),
        "defence": (f"Secure the junction and hold it {hold_s}. Their armour will counterattack from the north once you are on it. {supply_line}",
                    f"{cap(short)} held. Counterattack beaten."),
        "finale": (f"Break the blocking force and hold the exit {hold_s}. The column follows you out.",
                   "Corridor open. Column moving."),
    }[kind]
    goal = {"rescue": "recover", "dispatch": "reach"}.get(kind, "hold")
    return {
        "id": "secure", "title": {"rescue": f"Extract him from {short}", "dispatch": "Acknowledge the order",
                                  "bridge": f"Hold {short}", "defence": f"Hold {short}", "finale": "Hold the exit"}.get(kind, f"Secure {short}"),
        "goal": goal,
        "at": {"x": site["position"][0], "z": site["position"][1]}, "radius_m": 100.0,
        "trigger": {"kind": "previous"},
        "complete": {"kind": "site_secured"},
        "lines": {
            "open": [{"from": NET, "text": f"{CALLSIGN}, {NET}. {text[0]} Out."}],
            "done": [{"from": CALLSIGN, "text": f"{NET}, {CALLSIGN}. {text[1]} Out."}],
            "nudge": [{"from": NET, "text": f"{CALLSIGN}, {NET}. {cap(short)}. Report. Out."}],
        },
        "map": {"mark": "circle", "bearing": True},
        "cues": [],
        "pacing": {"nudge_after_s": 300.0},
    }


def sheet(spec, knots, edges, site):
    sid = site["id"]
    prose = SITES[sid]
    short = SHORT[sid]
    camp = tuple(spec["camp"])
    pos = tuple(site["position"])
    path = shortest_path(knots, edges, nearest_knot(knots, camp), nearest_knot(knots, pos))
    path = path + [pos] if math.dist(path[-1], pos) > 1.0 else path
    approach, road_m = point_before_end(path, APPROACH_M)
    direction, bearing = compass(pos[0] - camp[0], pos[1] - camp[1])
    straight = math.dist(camp, pos)
    guards = site["guards"]
    kind = site["kind"]
    hint = {
        "ammunition": "Take them from the flank, not up the road; do not fire into the carts.",
        "fuel": "Take them from the flank; do not fire into the cans, they are ours once it is clear.",
        "workshop": "Take them from the flank; the parts lie beside the road.",
        "bridge": "Take the near-bank guards before you show yourself on the deck.",
        "rescue": "Sentries are with the guards. He has a flare pistol and will fire a white star when he hears you.",
        "assault": "Work the junction from the west; it is covered from three sides.",
        "dispatch": "Take them from the flank; dismount for the ledger only once they are down.",
        "defence": "Take the guards at the crossroads first, then face north.",
        "finale": "The blocking force sits across the road out. Break it from the flank.",
    }[kind]
    cues = []
    if kind == "rescue":
        cues.append({"kind": "flare", "color": "white", "delay_s": 3.0, "repeat_s": 150.0})
    march = {
        "id": "march", "title": f"The approach to {short}", "goal": "reach",
        "at": {"x": round(approach[0], 1), "z": round(approach[1], 1)}, "radius_m": 160.0,
        "trigger": {"kind": "start"},
        "complete": {"kind": "enter", "dwell_s": 1.0},
        "bypass_near": {"at": {"x": pos[0], "z": pos[1]}, "radius_m": 350.0},
        "lines": {
            "open": [{"from": NET, "text": f"{CALLSIGN}, {NET}. Order {sid}: {site['name']}, {km(straight)} {direction}, bearing {bearing:03.0f}. "
                                           f"Road: {prose['approach']}, {km(road_m)}. Report at the turn, {APPROACH_M:.0f} metres short. Out."}],
            "done": [{"from": CALLSIGN, "text": f"{NET}, {CALLSIGN}. At the turn. {cap(short)} in sight. Out."}],
            "bypassed": [{"from": CALLSIGN, "text": f"{NET}, {CALLSIGN}. At {short}. Out."}],
            "nudge": [{"from": NET, "text": f"{CALLSIGN}, {NET}. Report your position. Out."}],
        },
        "map": {"mark": "box", "bearing": True},
        "cues": [],
        "pacing": {"nudge_after_s": 300.0},
    }
    contact = {
        "id": "contact", "title": f"{armour(guards)[0].upper()}{armour(guards)[1:]} at {short}", "goal": "destroy",
        "at": {"x": pos[0], "z": pos[1]}, "radius_m": 450.0,
        "trigger": {"kind": "previous"},
        "complete": {"kind": "site_guards_down"},
        "lines": {
            "open": [{"from": NET, "text": f"{CALLSIGN}, {NET}. Reported at {short}: {armour(guards)} and sentries. {hint} Out."}],
            "done": [{"from": CALLSIGN, "text": f"{NET}, {CALLSIGN}. Armour at {short} destroyed. Out."}],
            "nudge": [{"from": NET, "text": f"{CALLSIGN}, {NET}. Report. Out."}],
        },
        "map": {"mark": "cross", "bearing": True},
        "cues": cues,
        "pacing": {"nudge_after_s": 300.0},
    }
    order = [
        f"{CALLSIGN}, Tiger of the support detail. Order {sid}: {prose['where']}, {km(straight)} {direction} of the first camp "
        f"({km(road_m)} by road: {prose['approach']}).",
        f"Reported: {armour(guards)} and sentries; {prose['ground']}. {TASKS[kind]}",
        f"Hold {site['hold_s']:.0f} seconds once it is clear. {STAKES}",
    ]
    return {
        "schema": "kessel.mission", "version": 1,
        "id": f"expedition-{sid}", "site": sid,
        "title": f"BEFEHL {sid}: {site['name'].upper()}",
        "heading": HEADING, "act_id": 1,
        "callsign": CALLSIGN, "net": NET,
        "note": f"The March, order {sid} ({kind}). Geography from catalog/expedition_theatre.v1.json: the approach point is "
                f"{APPROACH_M:.0f} m short of the site on the road path from the first camp. The contact and secure beats "
                f"observe the theatre's own guard, hold and counterattack rules; no beat spawns, moves or alerts an enemy.",
        "order": order,
        "beats": [march, contact, secure_beat(site, short, site["hold_s"])],
    }


def main():
    spec = json.loads(SOURCE.read_text())
    knots, edges = road_graph(spec)
    sheets = [sheet(spec, knots, edges, s) for s in spec["sites"]]
    out = {
        "schema": "kessel.expedition_missions", "version": 1,
        "theatre": spec["name"], "source": "catalog/expedition_theatre.v1.json",
        "generator": "tools/author_expedition_missions.py",
        "provenance": spec["provenance"],
        "sheets": sheets,
    }
    TARGET.write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n")
    for s in sheets:
        a = s["beats"][0]["at"]
        print(f"{s['site']:>3} {s['title']:<40} approach ({a['x']:>8.1f}, {a['z']:>8.1f})")
    print(f"wrote {TARGET.relative_to(ROOT)} ({len(sheets)} sheets)")


if __name__ == "__main__":
    main()
