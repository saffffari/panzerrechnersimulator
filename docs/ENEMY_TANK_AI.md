# Enemy tank crews (T-34), 2026-10-28

Branch `t34-ai`. Owner report after playing the merged build: the T-34s "stand still and wait to
be shot". This note records why, what the crews do now, the before/after measurements, and what is
still open.

## Cause

On Canyon v2 every T-34 is driven by the winter encounter (`src/winter_encounter_ai.rs`), not by
the swarm manoeuvres in `Game::enemy_maneuver` or the evolved policy: `enable_canyon_course_seeded`
builds a `WinterEncounter::in_streets`, and `enemy_ai` hands every slot with a winter order to it.
That encounter was written for the courtyard stealth scene against a **man on foot**, and its
contact rules were applied unchanged to the Tiger:

1. **Visual contact = hold a standoff.** A leader in sight of its target held a 165-235 m band
   at 0-20 % throttle and only turned its hull (`lead`, `Phase::Track`). A test pinned exactly that
   (`confirmed_contacts_hold_standoff_and_can_return_fire_without_charging`).
2. **Wingmen fight from the halt.** A wingman that saw the target stopped (`follow`, drive 0); in the
   streets the platoon "fights in column" behind the leader, so the second and third tanks have no
   line of fire. A platoon that lost its leader held its ground for 30 s (`hold`).
3. **Contact decays to a slow search.** Without sight a leader crept (0.25 throttle, 3 m/s) to a
   point 112 m outside the fix and stopped where the area came into view; contact memory ends
   after 22 s (`SEARCH_TIMEOUT`), and the platoon walks back to its beat at 0.12 throttle with
   10-14 s halts at every corner.
4. **No reaction and no armour reasoning.** Nothing read the Tiger's facing, the crew's
   penetration chances, a hit on its own hull, or the Tiger's gun turning on it.
5. **Perception gaps.** The Tiger's muzzle flash did not help a crew spot it (only the sound report
   did, as an uncertain area); its engine was never heard.

Static obstacles were a secondary cause: on seed 137 two wingmen of platoon A were pinned for
240 s and 124 s (they drove 43 m and 76 m all run) following their leader; elsewhere stuck time
was small. The opening rigs are not benched: all eleven tanks of a run are fielded and
patrol. The evolved policy only drives the AI lab (`train`, `ai-gauntlet`, the showreels); the
windowed Gefecht arena ran the scripted swarm.

## What the crews do now

Against an armoured target (`Observation::on_foot == false`) every crew that knows of it fights by
`WinterEncounter::assault`; against a man on foot the courtyard rules are unchanged.

- **Knowledge, not omniscience.** A crew acts on the Tiger in its own sight, its own memory of it,
  a leader's radio fix, or (a wingman) its leader's fix. Its hull heading is read only while it is
  in sight; unseen, the crew assumes it faces them. Spotting still runs through the stealth meters
  (detection envelope, arc, weather contrast, line of sight, the 5-second hand-off), now with two
  more real signatures: the **muzzle flash** (a crew looking at least side-on toward a Tiger shot,
  within 2 km, with a clear line through air that passes 3 % of the light, learns exactly where the
  flash was; a fix of that moment, not a track) and the **engine** (crews within earshot get an
  uncertain 32 m area every 3 s; louder at speed, quieter in the storm and through cover).
- **Armour solution.** `tank_doctrine::kill_margin_mm` gives the 76 mm BR-350B's margin over the
  Tiger I plate it presents from a bearing, from the same plate table and penetration law as the
  shell solve: the front is immune at every range; the side and rear go through at fighting range.
- **Fix and flank.** The platoon leader takes a post in its own sector, edged off the Tiger's
  frontal arc, hull-down where it can be, and fires to fix the Tiger. The first wingman goes for
  the flank on the leader's other side, the second deeper on the leader's side (a pincer). Posts
  are chosen from a ring of candidates on the flanks and rear at 110-420 m, scored by the armour
  solution there, the drive, exposure to the frontal arc on the way, spacing from the other tanks
  and their posts, then by sight probes (target visible from the cupola; hull-down when it is not
  visible from the hull roof). In the streets every post is snapped to the measured drivable cells
  and reached along them; a post no street leads to is remembered and avoided.
- **Bounding overwatch.** While the platoon has the Tiger in sight within 800 m, at least one tank
  stays at its post covering while the others move. Bounds run at 0.62 throttle (about 27 km/h);
  closing on an unseen fix at 0.5.
- **Shoot and relocate.** Two shots from a post, then a new post. A tank with a killing shot on
  the way makes a short halt to fire (dispersion grows with speed). A post held 14 s without sight
  is given up for one 20 % closer, and again, so contact is pressed rather than waited out.
- **React.** A hit, or a Tiger round bursting within 9 m, sends a tank backing out of the line of
  fire for 4.5-6 s with its nose (the thickest plate and the gun) on the threat, unless it holds a
  loaded killing shot. The Tiger's gun laid on a tank that has already fired from its post and
  has no killing shot does the same. A tank ordered to drive that has not moved for 3 s backs off
  and chooses again. Crews bail out of a penetrated hull as before (`enemy_crew`).
- **No more orphans or vigils.** A platoon that lost its leader joins another leader only within
  600 m; otherwise each tank fights on alone on its own senses (before, two wingmen "joined" a
  leader 2 km away with no street to him and sat 425 m from the Tiger for the rest of the run). A
  leader halted at a reported area, in view of it, or with no street to it gives the report up
  after 20 s instead of waiting out the 150 s sound memory.
- **Fire discipline.** The leader fires to fix; others fire where their round can kill, when the
  Tiger's gun is on them, or inside 300 m.
- **Numbers.** Leaders still relay a fix to the other platoons after the 5-second hand-off; every
  platoon with a fix converges on its own flank posts. In armour contact a platoon follows the
  Tiger up to 1,400 m from its beat (was 650 m). Investigating a report runs at 0.4 throttle
  (was 0.25).
- **Arena.** The Gefecht arena installs the evolved T-34 policy (`assets/ai/weights_academy.bin`),
  the ground it was evolved on; the scripted swarm remains the fallback.

Everything is deterministic: no random draws, the per-tank jitter is by slot index, and the
encounter runs in simulation time.

## Measurements

Headless `canyon-ai-run SEED 900` on Canyon v2 (the workshop snapshot the integration build loads,
frozen in the worktree), the rule-based autopilot as the Tiger, mortal, one process at a time on
the shared GPU; `before` = `62b2cec` (telemetry only, AI as on `main`), `after` = this branch.
Receipts: `artifacts/t34-ai-20261028/{before,after}/s<seed>_result.json` (the `final.enemy_ai`
block), gzipped stdout with every `[armor]` line; the table is `tools/enemy_ai_compare.py`.

Every run ends early for the same reason: the autopilot jams at 5-6 % of the course (its own
navigation, `phase: detouring`, 6 recoveries) and the harness stops after 240 s without progress
or a Tiger shot ("pilot_stalled"), unless the Tiger dies first ("knocked_out"). The jammed Tiger
is a fair test of the old complaint: before, on every seed, eleven T-34s left a stationary Tiger
alone for four minutes.

Five seeds together:

| | before | after |
|---|---|---|
| Tiger killed by T-34s | 0 of 5 | 2 of 5 (seeds 29, 137, at 112 s: turret side holed at 422 m, ammunition) |
| T-34 main-gun shots front / side / rear | 4 / 0 / 0 | 2 / 6 / 0 |
| shots with a kill solution | 0 | 5 |
| hits on the Tiger front / side / rear | 2 / 0 / 0 | 0 / 4 / 0 |
| penetrations of the Tiger | 0 | 2 |
| idle share of time in contact | 0.44 | 0.37 |
| stuck tank-seconds per simulated second | 0.24 | 0.08 |
| distance driven per simulated second | 8.2 m | 9.6 m |
| T-34s knocked out by the Tiger | 6 | 7 |
| wall time / simulated time (shared GPU, indicative) | 0.76 | 0.24 |

Per seed:

| metric | s29 before | s29 after | s137 before | s137 after | s7 before | s7 after | s101 before | s101 after | s53 before | s53 after |
|---|---|---|---|---|---|---|---|---|---|---|
| outcome | pilot_stalled | knocked_out | pilot_stalled | knocked_out | pilot_stalled | pilot_stalled | pilot_stalled | pilot_stalled | pilot_stalled | pilot_stalled |
| simulated s | 346 | 112 | 345 | 112 | 316 | 435 | 290 | 338 | 313 | 406 |
| wall / simulated | 1.02 | 0.14 | 0.75 | 0.18 | 0.8 | 0.34 | 0.52 | 0.2 | 0.66 | 0.21 |
| Tiger course progress | 0.048 | 0.057 | 0.054 | 0.056 | 0.057 | 0.046 | 0.05 | 0.057 | 0.057 | 0.057 |
| Tiger alive | True | False | True | False | True | True | True | True | True | True |
| Tiger death cause | - | AmmoDetonation | - | AmmoDetonation | - | - | - | - | - | - |
| T-34s fielded | 11 | 11 | 11 | 11 | 11 | 11 | 11 | 11 | 11 | 11 |
| T-34s that knew of the Tiger | 11 | 11 | 11 | 11 | 3 | 6 | 3 | 3 | 3 | 3 |
| first visual contact s | 97.7 | 97.7 | 97.5 | 97.5 | - | - | - | - | - | 94.9 |
| first T-34 shot s | 100.7 | 100.7 | 100.5 | 100.5 | - | - | - | - | - | - |
| first hit on Tiger s | 101.4 | 106.8 | 101.2 | 106.7 | - | - | - | - | - | - |
| tank-s in contact | 590 | 121 | 586 | 121 | 18 | 128 | 12 | 51 | 2 | 45 |
| idle share in contact | 0.43 | 0.53 | 0.46 | 0.51 | 0.0 | 0.15 | 0.9 | 0.24 | 0.0 | 0.32 |
| stuck tank-s | 16 | 1 | 366 | 60 | 0 | 39 | 0 | 0 | 0 | 6 |
| distance driven m | 3220 | 1266 | 2976 | 1309 | 2479 | 4372 | 2124 | 2637 | 2408 | 3825 |
| T-34 shots front/side/rear | 2/0/0 | 1/3/0 | 2/0/0 | 1/3/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 |
| share of shots from flank or rear | 0.0 | 0.75 | 0.0 | 0.75 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 |
| shots with a kill solution | 0 | 2 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 |
| hits on Tiger front/side/rear | 1/0/0 | 0/2/0 | 1/0/0 | 0/2/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 | 0/0/0 |
| penetrations of the Tiger | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 |
| Tiger hits on T-34s front/side/rear | 1/0/0 | 0/0/0 | 1/0/0 | 0/0/0 | 1/0/0 | 2/0/0 | 1/1/0 | 2/1/0 | 0/1/0 | 1/1/0 |
| T-34s knocked out | 1 | 0 | 1 | 0 | 1 | 2 | 2 | 3 | 1 | 2 |
| Tiger shots | 1 | 0 | 1 | 0 | 2 | 3 | 2 | 3 | 1 | 2 |

Time to first contact is unchanged (about 98 s on seeds 29 and 137, none on 7, 101, 53): first
contact is a perception event and the opening deployment is the same.

On seeds 7, 101 and 53 only the platoon nearest the start ever learns of the Tiger, and in every
case the Tiger kills its tanks at about 400 m before any of them recognises it in the storm. The
autopilot's sight is geometric within 420 m and ignores the weather (`canyon_pilot`), while the
crews' recognition needs 10 % contrast through `weather_view`; a parked Tiger at 400 m fills a
cued crew's meter at roughly 0.2/s when the air allows recognition at all. Those seeds measure the
harness Tiger's weather-blind sight as much as the T-34s. After the change the crews do move on
the flash and the engine (6 crews knew of the Tiger on seed 7 instead of 3, and drove 4.4 km
instead of 2.5 km); the flankers were killed en route.

Arena: `ai-gauntlet` (evolved T-34 policy against the evolved Tiger policy, act 0, 3 seeds x 1-6
T-34s, 90 s) is identical before and after, row for row: T-34s win 0/3, 2/3, 2/3, 1/3, 2/3, 3/3
for 1-6 tanks. The gauntlet does not run the encounter, so it is a regression check; the evolved
policy now also drives the windowed Gefecht arena.

Unit tests (CPU only): `cargo test --release --bin panzerrechnersystem -- winter_encounter_ai
tank_doctrine enemy_ai_telemetry street_nav` (20 pass). The open-ground duel has a platoon facing
a Tiger at 380 m reach killing flank posts at 48 s, 52 s and 108 s, the wingmen on opposite flanks,
never all three moving at once; the street-district duel has all three manoeuvring 240-400 m
along the streets without entering a block.

Full suite from the worktree (`cargo test --release --bin panzerrechnersystem`): 1,933 passed /
90 failed / 61 ignored, against the integration gate's 1,927 / 90 / 61 on main; the six extra
passes are this branch's new tests, the 90 failures are the worktree's missing git-ignored assets
(the two with nearby names, `normal_enemy_report_recipes_require_discharge_and_reject_injected_impacts`
and `canyon_props_bound_rotated_contact_expansion_and_exclude_casualties`, fail on "cannot find the
path"). List: `artifacts/t34-ai-20261028/full_tests.txt`.

## Open items

- **The autopilot sees through the storm.** Make `canyon_pilot_visible_target` respect
  `weather_view.recognizable` (and its own optics), or the Canyon receipts will keep favouring the
  Tiger in weather. Owner of the pilot's call; not changed here.
- **The autopilot jams at 5-6 %** on every seed tried (29, 7, 137, 101, 53), so no run measured the
  hamlet street or the later course. The fights measured are all the first platoon's.
- **Balance is the owner's call.** On the two seeds where the crews saw the Tiger they killed it at
  112 s with a turret-side penetration and an ammunition detonation. Against a human who moves and
  uses the optics this may be right or too harsh; `BOUND_SPEED`, `SHOTS_PER_POST`, the fire rule
  and `CUED_SEARCH_GAIN` are the knobs.
- **No smoke.** T-34-76s of the period carried no smoke dischargers; MDSh smoke canisters were
  fitted to some tanks. Not modelled; concealment comes from the terrain, buildings, wrecks and
  the storm (the sight probes and the existing weather visibility).
- **The learned policy is not used on the Canyon.** It was evolved on open ground with a
  14-input view and drives into walls; a close-combat hand-off (open ground, under 150 m, in
  sight) is possible but untested.
- **Turret faces** are not in the armour solution (hull faces only), and the solution ignores
  scars and the solver's per-shot variation.
- **Reinforcement platoons** are only reached by the radio relay of a living leader; a wiped
  platoon tells nobody. Gunfire beyond 900 m is unheard.
- The arena gauntlet seed `0x34a00002` is a 90 s stalemate for one to four T-34s before and after
  (nobody engages): the evolved policies' own behaviour, not touched here.
- Decision log: `KESSEL_T34_DBG=1` prints every post choice, refusal and withdrawal, and
  `canyon-ai-run` prints the encounter state every 10 s.
