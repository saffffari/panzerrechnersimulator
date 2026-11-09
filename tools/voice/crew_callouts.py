#!/usr/bin/env python3
"""Crew callouts from an ai-film receipt (src/game_ai_film.rs, its `events`): what the crewman who
would really see or feel it says, and when, on the film's clock.

    python3 tools/voice/crew_callouts.py RECEIPT.json CUES.json

Rules (wording ours: plausible Wehrmacht intercom German, not a period source; see
docs/CREW_VOICE_OPTIONS_20261108.md):
- a T-34 first in the commander's line of sight: the commander gives it, clock and range
  ("Panzer, zwo Uhr, achthundert!"); further ones at the same moment: "Noch einer, ... Uhr!";
- the gun laid on a T-34 in sight: the gunner acknowledges ("Erkannt!"), a later new target is
  the commander's ("Naechstes Ziel, ... Uhr!");
- the Tiger fires: the commander's "Feuer!" ends just before the shot; the breech loaded again:
  the loader's "Geladen!";
- after a round: a T-34's armour holed and it lives, the gunner's "Treffer!"; marked but not
  holed, the commander's "Abgeprallt!"; nothing marked within 3.5 s, the commander's "Daneben!";
- a T-34 knocked out: the gunner's "Abschuss!" ("Abschuss! Der brennt!" when its ammunition
  went); the last one: the radio operator reports to Adler;
- a hit on the Tiger: by where it struck (hull front: the driver; turret: the commander; side:
  the commander), "haelt" when it did not come through; a penetration: "Durchschlag!";
- crew posts fallen: the man beside him calls it; a fire: the loader in the fighting compartment,
  the driver for an engine-bay fire (the round came in at the rear);
- the Tiger destroyed: nothing (the death's own shots follow).
Lines never overlap (`schedule`): the most important are placed first, each as near its event as
the others allow, and "Feuer!" and "Geladen!" give way to anything else.
"""
import json
import sys

CLOCK = {1: "ein", 2: "zwo", 3: "drei", 4: "vier", 5: "fünf", 6: "sechs", 7: "sieben", 8: "acht",
         9: "neun", 10: "zehn", 11: "elf", 12: "zwölf"}
ONES = ["", "ein", "zwo", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun"]
TEENS = {10: "zehn", 11: "elf", 12: "zwölf", 13: "dreizehn", 14: "vierzehn", 15: "fünfzehn",
         16: "sechzehn", 17: "siebzehn", 18: "achtzehn", 19: "neunzehn"}


def range_words(m):
    """A range in the gun's spoken form: hundreds, rounded to 50 m ("vierhundertfünfzig")."""
    m = int(round(m / 50.0) * 50)
    if m < 100:
        return "nah", "close"
    h, rest = divmod(m, 100)
    if h >= 10:
        head = TEENS.get(h, str(h)) + "hundert"
    else:
        head = ("" if h == 1 else ONES[h]) + "hundert"
    tail = "fünfzig" if rest == 50 else ""
    return head + tail, str(m)


def clock_words(c):
    return CLOCK.get(int(c), str(c)) + " Uhr", f"{int(c)} o'clock"


POST_DE = {"Commander": ("Kommandant", "Commander"), "Gunner": ("Richtschütze", "Gunner"),
           "Loader": ("Ladeschütze", "Loader"), "Driver": ("Fahrer", "Driver"),
           "Radioman": ("Funker", "Radio operator")}
# Who sees a fallen man: the loader and the gunner see each other and the commander; the driver
# and the radio operator sit side by side in the hull.
WITNESS = {"Commander": "RS", "Gunner": "LS", "Loader": "RS", "Driver": "FU", "Radioman": "FA"}


MISSES = [("Daneben. Nochmal.", "Missed. Again."), ("Vorbei. Neu richten.", "Wide. Lay again."),
          ("Kein Treffer. Weiter.", "No hit. Keep at it.")]


def plan(events):
    cues = []

    def add(t, role, reg, de, en, why, prio=1, follows=None):
        cues.append(dict(id=len(cues), t=round(max(0.0, t), 3), role=role, register=reg, de=de, en=en, event=why,
                         prio=prio, follows=follows))
        return len(cues) - 1

    call = None  # the commander's latest target call, which the gunner's "Erkannt!" answers

    first_sight_t = None
    shots = [e for e in events if e["kind"] == "tiger_fired"]
    marks = [e for e in events if e["kind"] == "enemy_armour_marked"]
    kills = [e for e in events if e["kind"] == "kill"]
    total_enemies = len({e["enemy"] for e in events if e["kind"] in ("enemy_in_sight", "gun_laid_on")}) or 0
    killed = 0
    misses = 0
    for e in events:
        t, k = e["film_s"], e["kind"]
        if k == "enemy_in_sight":
            (cd, ce), (rd, re) = clock_words(e["clock"]), range_words(e["range_m"])
            if first_sight_t is None or t - first_sight_t > 2.0:
                first_sight_t = t
                call = add(t + 0.4, "KDT", "urgent" if e["range_m"] < 300 else "calm",
                           f"Panzer, {cd}, {rd}!", f"Tank, {ce}, {re}!", "enemy_in_sight", 2)
            else:
                add(t + 0.4, "KDT", "urgent", f"Noch einer, {cd}!", f"Another one, {ce}!", "enemy_in_sight", 2)
        elif k == "gun_laid_on":
            if not e.get("in_sight"):
                continue
            if first_sight_t is not None and t - first_sight_t < 3.0 and call is not None:
                add(t + 1.6, "RS", "calm", "Erkannt!", "Identified!", "gun_laid_on", 2, follows=call)
            else:
                cd, ce = clock_words(e["clock"])
                call = add(t + 0.3, "KDT", "calm", f"Nächstes Ziel, {cd}!", f"Next target, {ce}!", "gun_laid_on", 2)
                add(t + 1.8, "RS", "calm", "Erkannt!", "Identified!", "gun_laid_on", 2, follows=call)
        elif k == "tiger_fired":
            add(t - 0.55, "KDT", "urgent", "Feuer!", "Fire!", "tiger_fired", 0)
            hit = [m for m in marks if 0 <= m["film_s"] - t <= 3.5] + [x for x in kills if 0 <= x["film_s"] - t <= 3.5]
            if not hit:
                # The fall of shot is not in the log, so the call does not say short or over.
                de, en = MISSES[misses % len(MISSES)]
                misses += 1
                add(t + 2.2, "KDT", "calm", de, en, "miss", 1)
        elif k == "breech_loaded":
            add(t + 0.1, "LS", "calm", "Geladen!", "Loaded!", "breech_loaded", 0)
        elif k == "enemy_armour_marked":
            if any(0 <= x["film_s"] - t <= 0.5 and x["enemy"] == e["enemy"] for x in kills):
                continue
            if e["hole"] and e["alive"]:
                add(t + 0.5, "RS", "urgent", "Treffer!", "Hit!", "enemy_armour_marked", 2)
            elif not e["hole"]:
                add(t + 0.5, "KDT", "urgent", "Abgeprallt! Nochmal!", "Bounced off! Again!", "enemy_armour_marked", 2)
        elif k == "kill":
            killed += 1
            if e.get("catastrophic"):
                add(t + 0.5, "RS", "urgent", "Abschuss! Der brennt!", "Kill! He's burning!", "kill", 3)
            else:
                add(t + 0.5, "RS", "urgent", "Abschuss!", "Kill!", "kill", 3)
            if total_enemies and killed >= total_enemies:
                add(t + 4.0, "FU", "calm", f"Adler von Kiebitz: {killed} Feindpanzer abgeschossen.",
                    f"Adler from Kiebitz: {killed} enemy tanks knocked out.", "all_killed", 2)
        elif k in ("tiger_struck", "tiger_penetrated"):
            scar = e.get("scar") or ""
            where = scar.split(" ")[0] if scar else ""
            pen = k == "tiger_penetrated"
            if where.startswith("Turret"):
                role, de, en = "KDT", "Treffer am Turm!", "Hit on the turret!"
            elif where == "HullFront" or where == "Glacis":
                role, de, en = "FA", "Treffer vorne!", "Hit up front!"
            else:
                role, de, en = "KDT", "Treffer seitlich!", "Hit on the side!"
            if pen:
                de, en = de[:-1] + ", Durchschlag!", en[:-1] + ", it's through!"
                ko = (e.get("chain") or {}).get("knocked_out") or []
                if any("engine" in x.lower() for x in ko):
                    add(e["film_s"] + 2.0, "FA", "urgent", "Motor ausgefallen!", "Engine's out!", "damage", 2)
            else:
                de, en = de + " Hält.", en + " It held."
            add(t + 0.35, role, "urgent", de, en, k, 3)
        elif k == "crew_posts_fallen":
            for post in e["posts"]:
                who = WITNESS.get(post, "KDT")
                name_de, name_en = POST_DE.get(post, (post, post))
                if post == "Commander" and len(e["posts"]) >= 4:
                    continue
                add(t + 0.8, who, "urgent", f"{name_de} ist getroffen!", f"{name_en} is hit!", "crew_posts_fallen", 3)
        elif k == "fire_origin":
            entry = e.get("entry_hull_local_m")
            if entry is not None and entry[2] < -1.4:
                add(t + 0.8, "FA", "urgent", "Motor brennt!", "Engine's on fire!", "fire", 3)
            else:
                add(t + 0.6, "LS", "urgent", "Feuer im Kampfraum!", "Fire in the fighting compartment!", "fire", 3)
        elif k == "tiger_destroyed":
            death = t
            cues[:] = [c for c in cues if c["t"] < death]
            break
    return cues


def schedule(cues, durations):
    """Lines never overlap. The most important cues are placed first, each at the earliest moment
    from its event that is clear of the lines already placed (a 0.15 s gap), waiting at most
    0.4 s ("Feuer!", "Geladen!"), 1.2 s (most calls) or 2.0 s (kills, hits on the Tiger, wounds,
    fire); a cue that cannot be placed is dropped."""
    wait = {0: 0.4, 1: 1.2, 2: 1.2, 3: 2.0}
    placed = []
    ends = {}
    # A reply ("Erkannt!") is placed after its call, or not at all.
    for c in sorted(cues, key=lambda c: (-c["prio"], c.get("follows") is not None, c["t"])):
        d = durations.get((c["role"], c["de"]), 1.2)
        if c.get("follows") is not None:
            if c["follows"] not in ends:
                continue
            c = dict(c, t=ends[c["follows"]] + 0.25)
            starts = [c["t"]] + [e + 0.15 for (_, e, _) in placed if c["t"] <= e + 0.15 <= c["t"] + 0.8]
        else:
            starts = sorted({c["t"]} | {e + 0.15 for (_, e, _) in placed if c["t"] <= e + 0.15 <= c["t"] + wait[c["prio"]]})
        for s0 in starts:
            if all(s0 + d + 0.15 <= a0 or s0 >= e0 + 0.15 for (a0, e0, _) in placed):
                placed.append((s0, s0 + d, dict(c, start=round(s0, 3), dur=round(d, 3))))
                ends[c.get("id")] = s0 + d
                break
    return [x[2] for x in sorted(placed, key=lambda x: x[0])]


if __name__ == "__main__":
    receipt = json.load(open(sys.argv[1]))
    cues = plan(receipt["events"])
    json.dump(cues, open(sys.argv[2], "w"), indent=1, ensure_ascii=False)
    for c in cues:
        print(f"{c['t']:7.2f} {c['role']:3s} {c['register']:6s} {c['de']:45s} | {c['en']}  ({c['event']})")
