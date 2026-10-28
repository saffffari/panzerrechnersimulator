# Canyon v2 — the rule-based crew drives the course armed (27 October 2026)

Branch `run-loop`, worktree `C:\shop\panzerrechnersystem\.claude\worktrees\run-loop`, Windows
checkout. Every attempt below is `canyon-ai-run` (release binary of this branch, shader renderer,
High preset, 60 Hz, Canyon v2 manifest, `KESSEL_ASSET_ROOT` on the main checkout) and every
claim names its capture under `captures/` of the main checkout. Only `src/canyon_pilot.rs`
changed: the game's reload rules, hotspots, enemy AI, damage and ammunition are as they were.

## 1. The stall, reproduced

`canyon-ai-run captures/pilot_fullrun_s29_repro 29 900` (`KESSEL_LEVER_DBG=1`) reproduces the
baseline to the second: one shot at 32.9 s, the loader walks to the breech, opens the lever at
+1.9 s, pulls the case, racks a round (breech `Loaded` at +7.9 s), and the scripted lever-close
click at +13.5..13.7 s never registers. The trace ends at 4:34 in phase `reload_stalled` with
`breech: Loaded, lever: 1.0, carrying: false, station: Gunner` — a live round in an open breech,
the man back in the gunner's seat (`pilot_fullrun_s29_repro_trace.jsonl`, `_result.json`).

The per-frame lever debug shows the crosshair dead on the lever at the click: `dist=0.43
dot=1.00 lever=1.00 breech=true` for the whole window. So it was not aim. The old pilot pressed
on a stopwatch and never read the prompt under the crosshair; the game gives one click to
whichever hotspot owns the sight line, and after a ram the chambered round's own
"PULL LOADED ROUND [LMB]" prompt sits exactly where the lever gaze lands (the new pilot's log
records this at the moment it enters the close-lever stage in 3 of 36 reloads:
`pilot_fullrun_v3_s7_20261027.log` t=809.90, 1092.15, 1292.50). A spent case lying on the sight
line does the same for the rack grab (`pilot_fullrun_v3_s101_20261027.log` t=759.45). The
baseline's own hover was not logged, so which prompt stole its click is inferred, not recorded;
what is recorded is that a time-scripted press at that instant fails and a prompt-gated press
does not.

Earlier successful reloads in the same runs (seed 101 baseline: three at 31–72 s, 147–160 s)
differ only in whether the prompt happened to be the lever's when the clock said click.

## 2. What changed in the pilot

- **Stage machine instead of a stopwatch.** Walk → open lever → pull case → carry case away →
  grab round → ram → close lever → walk back → seat. A stage ends on the game's state (lever
  position, breech contents, hands, seat), and a press happens only when `hover` is the stage's
  own prompt (`GunLever`, `SpentCasing`, `RackSlot`, the gunner seat). If another prompt is
  under the crosshair the gaze is nudged through small turret-local offsets (10 cm for the wide
  lever/rack/seat cones, 3.5 cm for the tight case grip) until the right one shows. The spent
  case is carried to the rear-left corner before it is let go, off every sight line the rest of
  the cycle needs. A stage that waits longer than its patience restarts the cycle from the walk,
  twice, then reports `reload_stalled` with the state in the trace. `KESSEL_PILOT_DBG=1` prints
  every stage change and press with the prompt, lever, breech, hands and walk position.
- **Walk inputs bypass vehicle avoidance.** The pilot's drive/steer pair doubles as the
  loader's walk; passing it through `safe_ai_tiger_drive` could turn it into an avoidance
  "turn" and pin the man in the basket corner (seed 101 rev. 1: three walk timeouts at
  (−0.64, −0.64), `pilot_fullrun_s101_20261027.log` t=360–389).
- **Engagement.** The gunner holds his seat and lay 3 s after a contact drops out of sight
  (the per-second engage/advance flip cost a station change each way). A contact more than
  20° off the bore is met by pivoting the hull (neutral steer 0.7 rad/s) while the turret
  chases at 6°/s: in rev. 1 a T-34 on the flank at 63 m fired three rounds in the 15 s the
  turret needed for 97° and the third went through the side (seeds 29 and 7,
  `pilot_fullrun_s29_20261027.log`, `_s7_`); in rev. 2 the same tank was killed in five seconds.
- **Buttoned up.** Rev. 2 seed 7 survived that fight and then lost the man head-out in the
  cupola to the hamlet's posted riflemen (`CREW LOST`, no penetration, no module lost,
  `pilot_fullrun_v2_s7_20261027_runcard.png`). The crew now drives with the hatch shut; this
  driver's contact model is geometric and gained nothing from the open cupola.
- **Navigation recoveries.** The first stuck recovery is a straight full-throttle run-up
  (momentum is what presses a trunk or a brick pile down); the sideways detour ladder starts on
  the second failure and stays inside the 12 m lane (5, 8, 11, 14, 17 m). The old 16–48 m
  offsets drove the hull into the hamlet's frontage houses on both sides of the street
  (`pilot_fullrun_v2_s29_20261027.log`: `TANK STOPPED ... canyon.hamlet.frontage.b12.right`).

Unit tests: `game::canyon_pilot::navigation_tests` (3) pass in the release profile.

## 3. Attempts, 1500 s cap

| Seed | Revision | Outcome | Time | Reached | Shots / hits / kills | Hits taken (pens) | Reloads (stalled) | Capture prefix |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 29 | baseline | pilot stalled at reload | 4:34 | 358 m (7 %) | 1 / 1 / 1 | 0 (0) | 0 (1) | `run_loop_baseline_s29_20261027` |
| 7 | baseline (fixed obliquity) | pilot stalled at reload | 5:09 | 367 m (7 %) | 2 / 2 / 2 | 7 (0) | 1 (1) | `run_loop_fixed_s7_20261027` |
| 101 | baseline | pilot stalled at reload | 7:20 | 598 m (12 %) | 4 / 3 / 3 | 2 (0) | 3 (1) | `run_loop_baseline_s101_20261027` |
| 29 | rev. 1 (stage reload) | knocked out, ammunition detonation | 4:01 | 750 m (15 %) | 6 / 4 / 4 | 3 (1) | 6 (0) | `pilot_fullrun_s29_20261027` (card) |
| 7 | rev. 1 | knocked out, ammunition detonation | 2:59 | 681 m (14 %) | 4 / 2 / 2 | 3 (1) | 4 (0) | `pilot_fullrun_s7_20261027` (card) |
| 101 | rev. 1 | pilot stalled (loader walk) | 9:47 | 690 m (14 %) | 5 / 4 / 4 | 40 (0) | 4 (0) | `pilot_fullrun_s101_20261027` |
| 29 | rev. 2 (+ pivot, hold, walk bypass) | pilot stalled (navigation, hamlet) | 9:30 | 807 m (16 %) | 7 / 5 / 5 | 2 (0) | 7 (0) | `pilot_fullrun_v2_s29_20261027` |
| 7 | rev. 2 | knocked out, crew lost (head-out) | 3:35 | 684 m (14 %) | 5 / 3 / 3 | 2 (0) | 5 (0) | `pilot_fullrun_v2_s7_20261027` (card) |
| 101 | rev. 2 | pilot stalled (navigation, hamlet) | 9:14 | 686 m (14 %) | 6 / 4 / 4 | 0 (0) | 6 (0) | `pilot_fullrun_v2_s101_20261027` |
| 7 | rev. 3 (+ buttoned up, run-up, lane detours) | **alive at the cap** | 25:00 | 3241 m (66 %) | 20 / 15 / 14 | 2 (0) | 20 (0) | `pilot_fullrun_v3_s7_20261027` |
| 101 | rev. 3 | **alive at the cap** | 25:00 | 3842 m (78 %) | 16 / 11 / 11 | 0 (0) | 16 (0) | `pilot_fullrun_v3_s101_20261027` |
| 29 | rev. 3 | **alive at the cap** | 25:00 | 3481 m (71 %) | 19 / 15 / 14 | 4 (0) | 19 (0) | `pilot_fullrun_v3_s29_20261027` |

"Reloads" is the pilot's count of completed cycles; "stalled" is how many ended in
`reload_stalled`. In the three rev. 3 runs the physical reload (walk, lever, case, rack, ram,
lever, walk back, seat) took 7.6–10.2 s, mean 8.9 s, over 55 cycles with no retry and no stall.
After-action cards exist for every run that ended in a knock-out (`*_runcard.png`, `_runcard.json`);
a run that reaches the cap alive writes none, as in the window.

## 4. Longer cap

`pilot_fullrun_v3long_s101_20261027` (seed 101, 3000 s cap) follows the 1500 s run exactly (the
simulation is deterministic per seed) and goes on: at 1635 s the Tiger is alive at **84 %
(4145 m)**, east of the third defended area at (1033, 539), 20 rounds fired, 15 hits, 14 kills,
20 reloads, **no hit taken**, every one of the 14 T-34s fielded so far dead (`live_t34s: 0`,
last complete trace row). The process then ended on its own: exit status 1, no `_result.json`,
no card, the trace cut mid-row at 1636 s, and nothing in the captured stdout/stderr after the
director's "platoon 0 eliminated" — no panic text, no error. The last log lines are the
director's; a reinforcement platoon is due 60 s after a lull.

It was not the game. From 19:03 every launch of this binary on the machine died the same way
within seconds, at a different point of loading each time (`pilot_fullrun_v3long2_s101`,
`_v3long3_s101`, two 20 s diagnostics, and `ui runbrief`, which never touches the Canyon world):
exit status −1 (0xFFFFFFFF, the code .NET's `Process.Kill` / `Stop-Process` sets), no text,
with a private `KESSEL_PIPELINE_CACHE_DIR` and with a frozen copy of the workshop manifest
alike. The main checkout's `heights.npy` hash was unchanged (fnv1a64 3a9d12d0aaa33e7e, as in
every log). At that time sibling sessions were running two 3000 s `canyon-ai-run` attempts of
their own binary, an armour review, a raymarch capture, a graphics benchmark and two release
builds on the same box. The 84 % run was alive and unhurt when it was terminated; a completion
needs a quiet machine and ~35 minutes, and is the first thing to do next.

## 5. What is still in the way

1. **Pace.** At rev. 3 the crew needs more than 25 minutes for 4.93 km: a reload every
   engagement (9 s plus the station changes), the hull stopped for each fight, and stretches
   driven at 0.6 throttle. The 10–15 minute design target is a human's with no stops.
2. **The hamlet (route 600–800 m).** Every seed loses one to four minutes there. The hull is
   stopped on the authored straight (x = −747) around z = 220 and z = 155 by something the ground
   state does not name (`obstacle: ""`, `carried_by: "terrain"`, speed decaying from 5 m/s to 0
   with a small bounce back and the hull yawed against the steer); no manifest placement lies
   within 9 m of either point, so the procedural trees are the remaining candidate. The run-up
   and the narrower detours get past it now, but not quickly.
3. **One 76 mm penetration is still the end of the run** (rev. 1, both seeds: nine or ten
   modules and the rack in one hit). Balance is the owner's call under the physical-truth rules.
4. The old baseline's stolen prompt was inferred from the new logging, not recorded in the
   baseline itself (see §1).
5. **No completion was recorded.** The 3000 s attempt was terminated from outside at 84 %
   (§4) and every relaunch after 19:03 was killed during loading. Rerun
   `canyon-ai-run captures/pilot_fullrun_v3long_s101 101 3000` on a quiet machine; the
   trajectory is deterministic per seed, so it will pass 84 % at 1635 s with the same 14 kills
   and no hit taken, and the last 16 % (choke 3, the final basin) is new ground for the pilot.

## Reproduce

```bash
# from the run-loop worktree; prefixes are explicit so nothing overwrites
KESSEL_ASSET_ROOT='C:\shop\panzerrechnersystem' KESSEL_CANYON_V2=1 \
KESSEL_CANYON_MANIFEST='C:\shop\panzerrechnersystem\assets\worldgen\workshops\gaea-canyon-run-v2\manifest.v1.json' \
KESSEL_RENDERER=shader KESSEL_GRAPHICS_PRESET=high KESSEL_INTERNAL_RES=800x450 KESSEL_PILOT_DBG=1 \
target/release/panzerrechnersystem.exe canyon-ai-run captures/pilot_fullrun_v3_s101 101 1500
```
