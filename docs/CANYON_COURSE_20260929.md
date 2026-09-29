# CANYON: a playtest course from the title menu (29 September 2026)

The owner asked for "a canyon mode from main menu that is a kind of playtest challenge course where
you have to punch straight through a canyon of soviets to get to the other side ... buildings and
cover ... a reasonably sized canyon so as to be dynamic, but still directional." This is what was
built. `CLAUDE.md`'s two principles govern it: no words in play, and physical truth with its debts in
`docs/PHYSICAL_TRUTH_AUDIT.md` (rows "Tiger static tipping and the CANYON course" and "Winter snow on
steep faces").

## How to play it

- Title menu: **Canyon** (the last entry). Its line shows the last and the best run once a run is
  complete; before that, what the course is.
- `tools/launch_winter_session.sh --canyon` starts the course directly (with `--menu`, the title
  menu); it writes a missing workshop first (about 6 s) or one an older tool wrote.
- `tools/launch_current_build.sh --singleplayer --canyon` builds if needed, then starts it; the
  desktop entry `/home/alex/panzer/panzerrechnersystem-canyon-dev.desktop` runs exactly that on the
  sound-overhaul worktree (its "Title menu" action passes `--menu --canyon`).
- `python3 tools/worldgen/canyon_course.py [--plan captures/canyon_plan.png]` writes the workshop
  `assets/worldgen/workshops/canyon-course/` (deterministic, byte-identical reruns).

One crewman in the Tiger, as in every singleplayer mode. Reaching the ravine's head in the Tiger
(the goal disc, 85 m round the ramp's top) ends the run: the Soviet crews stop firing, the engine and
the battle fade with the picture over 4 s, black and silence hold, and the title menu shows the time.
Times are kept beside the campaign checkpoint (`$XDG_STATE_HOME/panzerrechnersystem/canyon-times.v1.json`;
the branch launchers put state in `artifacts/shader-state/`). Death is black and silence and the retry
builds the course afresh (about 10 s under the black).

## What the course is

A composite of the balkas that cut Stalingrad's factory district from the steppe to the Volga
(Dolgiy, Krutoy, Banny) and the Tsaritsa gorge: a steep-sided winter ravine with a workers'
settlement and a factory edge in it. Its depth and width are a playtest course's, not one ravine's;
no engagement is depicted. Act I, January 1943, a grey forenoon (the ephemeris puts the Sun about 20°
up, shining along the ravine).

| Measure | Value |
|---|---|
| Length along the floor | 2,683 m, start (south) to the ramp's top (north) |
| Floor width | 150-390 m by profile; wall to wall, with the walls' gullies and buttresses, 137-402 m; three chokes (146, 159 and 138 m), two arenas (the settlement about 390 m, the factory edge about 360 m), a start basin, a final basin and six side pockets 45-60 m deep; the ramp up to the head narrows to 70-120 m |
| Walls | a talus foot, then loess-like faces of 66-78° to a rim 21-39 m over the floor with a rounded shoulder; eight side gullies notch the plateau and leave the main wall at least 22 m high; twelve rockfall cones at the foot; the floor rises and falls about 10 m, with hummocks and a frozen stream channel |
| Buildings (grammar) | 53 standing, 4 razed to rubble mounds: 25 burned wooden houses, 10 brick merchant houses, 15 workshops, 3 workers' apartment blocks; a hamlet near the start (6), the settlement (26), the factory edge (14), the strongpoint (9), two lone ruins at chokes 1 and 2; 35 burned, 12 partly collapsed, 6 shelled |
| Cover | 143 brick yard and field walls (breakable), 13 KV-1 and Pz III wrecks (static), 120 stripped trees and pines (hinge-fall), stumps, logs, debris, planks, crates, snow drifts, 26 craters |
| Defenders | two T-34/76 platoons on their beats: A of three in the final basin, B of two in the factory arena (they investigate what they hear and counter-attack what they see); three 45 mm 53-K guns with crews of four, one per choke, each the best of that choke's 342-738 candidate sites by the field-of-fire probe (siting on the current workshop: choke 1 at (-265, 552), 156-351 m; choke 2 at (55, -109), 101-353 m; choke 3 at (-203, -853), 112-292 m); six riflemen in the ruins (two each in the settlement, the factory edge and the strongpoint), six riding platoon A's tanks; density rises toward the goal (tanks plus guns by thirds: 1, 3, 4) |

## How it is built

- `tools/worldgen/canyon_course.py` (tests `tools/worldgen/test_canyon_course.py`): the terrain
  (1024² at 3 m), buildings on flat pads, dressing, the `street_encounter` block (kill zones per
  choke, candidate sites, beats, the floor as ways) and the `canyon_course` block (start, goal, gun
  zones, posts). It reuses the urban intro district tool's grammar tables, sizes, condition shares,
  rubble mounds, craters and reviewed kit. The manifest declares `authoring.mode = "course"`, so the
  game lays no legacy Act-I city, story overlay or spare tanks over it.
- `src/canyon.rs` (the course block, the run's clock, the finish fade, the kept times) and
  `src/canyon_game.rs` (fielding the course with the winter encounter's machinery: the nav grid,
  `muster_winter_platoons`, the per-site `probe_gun_site` shared with the street encounter). AT gun
  slots grew from two to three.
- `src/tank_support.rs`: the static tipping refusal (56.0°), which with the 35° gradient makes any
  face steeper than 58.6° unclimbable at every heading. `shaders/terrain_material.wgsl`: winter snow
  thins on faces from 45.6° to 66.4°.

## Evidence

- Tests: the unit suite (1,384 passed, 0 failed, 51 ignored); the tool's 7 tests (determinism,
  start and goal, 2-3 km and 150-400 m, walls unclimbable but the goal reachable under 35°, buildings
  valid, flat, apart and off the route, the defence in depth, assets on disk); the GPU test
  `canyon_game::tests::the_canyon_course_fields_its_defence_stops_the_tiger_at_its_walls_and_completes_at_the_goal`
  (5 T-34s, 3 guns, 6 riflemen; nothing fires at the start; driven at the walls head-on and obliquely
  the Tiger is stopped by the step and gradient rules at most 4 m up; the first choke's gun engages
  a Tiger in its field; the goal completes the run once and stops the fire; a rebuild starts again).
- Stills (`captures/`): `canyon_aerial_south_20260929.png`, `canyon_aerial_settlement_20260929.png`,
  head-out `canyon_headout_{start,settlement,factory,strongpoint}_20260929.png`, the gunner's sight
  `canyon_gunner_choke2_20260929.png`, the title menu `canyon_title_menu_20260929.png` (and
  `canyon_title_menu_times_sample_20260929.png`, rendered with a hand-written sample times file).
- Film: `captures/canyon_fight_20260929.mp4` (scenario `catalog/shader_reviews/canyon_fight_20260929.v1.json`,
  32 s, synced soundtrack, contact sheet `canyon_fight_20260929_sheet.png`): at the second choke the
  gun fires four BR-240 rounds that bounce off the hull front at 292 m, the gunner fires twice (the
  disclosed staged-discharge fixture), then the crewman rides head-out up the ravine; all checks pass.
- Frame time, shader backend, `winter-bench dense 60` at the start, 1280 × 720, RTX 4090 shared with
  other jobs (84 % busy): mean 20.5 ms, p95 21.2 ms (simulation 0.53 ms, submission 1.1 ms); the
  courtyard workshop under the same load 14.2 ms (documented 7.2 ms on an idle GPU).

## Limits

- The walls are a heightfield: no overhangs or dugouts in them. On the 3 m grid a face crossing the
  grid obliquely still shows a faint saw pattern along its rim.
- AI tanks and riflemen do not meet the solid city or the Tiger's limits; they keep to the floor by
  navigation. The T-34s' nav covers the floor; a platoon that counter-attacks can reach the start.
- Only two riflemen per ruin group: the soldier pool (24 Soviet) is shared with the escorts and the
  gun crews.
- The goal is reached, not held: the run ends in the disc whatever is still alive.
- The plateau ends at the 3 km map edge, visible only from the air.
