# Missions: beats, orders and in-world guidance (27 October 2026, branch `missions`)

The owner's direction: "we need more engaging and directed gameplay like missions, or at least
more guidance than 'idk figure out where to go'". The mechanics were deep but the default session
(the Canyon v2 winter run) dropped the player in without a destination. This branch makes play
directed: a run or an act is an ordered list of **beats**, authored as data, and the game always
answers *where do I go, what do I do, why* through things that exist in the world: a written
order, marks and a bearing on the paper map, the march bearing on the pocket compass, signal
stars, smoke columns, enemies that have heard you, and terse radio traffic. There is no floating
waypoint, no HUD arrow and no checklist.

Code: `src/mission.rs` (the model and the state machine, unit tested), `src/mission_game.rs`
(its binding to the simulation), `src/mission_capture.rs` (`mission-captures`), hooks of a few
lines each in `game.rs`, `canyon_game.rs`, `canyon_director.rs`, `canyon_map.rs`, `hand_map.rs`,
`hand_compass.rs`, `ui.rs`, `main.rs` and `canyon_attempt.rs`. Sheets:
`assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json` (the Canyon v2 run) and
`assets/worldgen/act1/mission.v1.json` (Act One, the existing objectives in the same form).

## A note on words

`CLAUDE.md`'s first principle keeps the campaign wordless. The parent session decided that the
Canyon run's guidance may use a written order and radio traffic in the terse period register the
platoon branch uses (`src/radio_llm.rs`: "Verstanden. Engaging bearing 300. Out."), because the
run is a field exercise with a rear party on the net, not the campaign's lone Tiger. The Act One
sheet therefore carries only the written order and the paper marks: its beats say nothing on the
air, and the paper itself carries numbers and symbols plus one typed slip with the open beat's
number and title. If the owner wants the campaign entirely wordless again, delete that sheet's
`order` lines and the slip (`MapGuidance::slip`); the marks and the compass bearing remain.

## The schema (`kessel.mission`, version 1)

```json
{
  "schema": "kessel.mission", "version": 1,
  "id": "canyon-v2-rearguard", "title": "BEFEHL: ...", "act_id": 1,
  "callsign": "Kiebitz", "net": "Adler",
  "order": ["who you are", "the situation", "where you must get to and why"],
  "beats": [ { ...beat... } ]
}
```

A **beat**:

| Field | Meaning |
| --- | --- |
| `id`, `title` | Unique id; the title appears on the map slip ("3  THE GUN LINE AT THE FORK"). |
| `goal` | The verb of the order: `reach`, `hold`, `destroy`, `observe`, `recover`, `escort`, `survive_until`. Documentation and the order's register; the runtime condition is `complete`. |
| `at` | A place: `{"x","z"}` world; `{"ref":"route_fraction","f"}` along the course route by arc length; `course_start`, `course_goal`; `act_spawn`, `act_goal` (the campaign route's start and exit when a campaign is live); `{"ref":"route_site","index"}` (a campaign route's authored site); `{"ref":"encounter_role","role"}` (the first planned campaign encounter with that role: `blocking_force`, `boss`, ...). Resolved once when the mission is enabled; an unresolvable place is an authoring error that refuses the sheet. |
| `radius_m` | The beat's working radius. |
| `trigger` | `start` (with the mission), `previous` (when the beat before is done or failed; the default), `{"kind":"route_fraction","f"}`, `{"kind":"proximity","radius_m"}`. One beat is open at a time, in order. |
| `complete` | `enter {dwell_s}`; `observe {cone_deg, dwell_s}` (inside the radius with the place within the cone of the hull or the eye); `destroy_near {count, radius_m, guns}` (dead tanks, or silenced guns, near the place); `hold {seconds}` (accumulated inside); `survive {seconds}`; `recover` (on foot inside, then mounted); `escort` (a living ally inside); `campaign_objective {objective}` (the director's own credit, e.g. `Act1BreakWesternCord`); `course_finished`. |
| `bypass_fraction` | The beat also completes once the Tiger has passed this fraction of the course route (a strongpoint gone round is a strongpoint passed). |
| `fail` | `none`, `timeout {seconds}` (fails forward: the next beat opens), `ally_lost`. |
| `lines` | `open`, `done`, `failed`, `nudge`, `consequence`: lists of `{"from","text"}`. `from` is a callsign; empty means the crew's own note. |
| `map` | `mark`: `box`, `cross`, `pin`, `circle`; `bearing`: pencil the dashed bearing line and set the compass. |
| `cues` | `flare {color, at, delay_s, repeat_s}`, `smoke_column {at, height_m, duration_s}`, `alert {at, radius_m}`. |
| `pacing` | `release {count, fraction}` sent when the beat opens; `live_cap` for the course director while the beat is open; `nudge_after_s` (lines and the first flare again, every that often); `consequence_after_s` with `consequence {count, fraction}` (once). |

Every cue is something in the world. A **flare** is a 26.5 mm signal cartridge: the star leaves
the ground at 46 m/s, climbs under gravity to about 110 m, opens its parachute at the apex and
sinks at 3.2 m/s while the wind carries it; it burns 24 s and fades over its last 3.5 s. It is a
point light with a true source size (90 mm), an emissive grain the display's glare spreads, a
launch-smoke thread and a thin smoke thread above the star. A **smoke column** is a sustained
dark plume from a burning vehicle: three source-connected media supports stacked to its height,
leaning with the wind, growing over its first twenty seconds. An **alert** tells the stealth
model that the platoons within the radius have heard the Tiger: Unaware platoons go to its own
Searching state (its own 22 s timeout, the Tiger's current position as the last known area), and
the guns' crews stand to (`heard`). No one is handed a fix the model would not give; nothing
fires that did not fire. A **release** is the course director's own platoon entry (the same
siting rules, the same hidden approach) at a knot of the corridor or down to the Tiger.

Radio lines are text (display-resolution, outside the pixel crush, top left, oldest first, 16 s
each) with the headset's key click. The crew's own transmissions use its callsign.

## Guidance the player sees

- **The order sheet.** At the start the typed Befehl is held up over the world: the title, the
  three lines, the first beat. It folds away when the crew moves (after 4 s) or after 28 s.
- **The map (M).** The survey carries every opened beat's mark with its number (done marks go
  faint), the dead-reckoned position (a dashed pencil ring whose radius is the crew's uncertainty:
  12 m at a fix, 4.5 cm per metre travelled since, 110 m at most; a completed beat is a fix) and
  a dashed bearing line from the estimate to the open beat's mark with its three figures. A slip
  in the sheet's corner names the open beat. The same marks print on the campaign's sheet.
- **The compass (C, or on the map).** The pocket compass's body turns to the open beat's bearing
  as a Marschkompass bezel is set: walk until the needle lies over the north mark and you face the
  bearing. The needle itself always points north.
- **The world.** Flares over phase lines (white: held by the rear party, red: enemy on it), the
  smoke column over the fork, the guns and platoons that heard you, the wrecks of the column that
  went before.
- **The net.** A line when a beat opens, completes or fails; a nudge when a beat is ignored; a
  line with the consequence.

## The Canyon v2 run as a mission

`assets/worldgen/workshops/gaea-canyon-run-v2/mission.v1.json`. Fiction: Panzer 231 is the
rearguard of a Kampfgruppe that went up the ravine at first light; its rear party (`Adler`)
holds the ravine head and signals the phase lines. Six beats along the authored 4.93 km course:

| # | Beat | Goal | Place (route) | Completes | Cues | Pacing |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Read the ground | observe | the wreck field marker (-736, 595), 6 % | the field within 100 deg of the hull or the eye for 4 s, inside 260 m | the wrecks themselves | cap 4; nudge 150 s; after 420 s a platoon of 3 at 14 % |
| 2 | Phase line Anton: the hamlet | reach | 12.3 % (-747, 295) | inside 110 m for 1 s | white star every 90 s | cap 4; nudge 240 s; after 600 s a platoon at 19 % |
| 3 | The gun line at the fork | destroy | the choke-1 candidates' centroid (-560, -200) | one 45 mm gun silenced within 650 m, or past 34 % | smoke column at the fork (-690, -88), alert 700 m | cap 6; nudge 300 s; after 720 s a platoon down to the Tiger |
| 4 | The factory yard | destroy | platoon B's beat (345, 190), opens at 44 % | two tanks dead within 420 m, or past 72 % | red star at 72 %, alert 650 m | +2 at 60 % on open; cap 7; nudge 300 s; after 780 s a platoon down |
| 5 | The final basin | destroy | platoon A's beat (1180, 420), opens at 76 % | two tanks dead within 450 m, or past 94 % | red star at 93 %, alert 700 m | cap 9; nudge 300 s; after 720 s a platoon down |
| 6 | The ravine head | reach | the goal disc | the run's own finish | white star every 75 s | cap 9; nudge 240 s |

The written order: *Panzer 231, rearguard of the Kampfgruppe. Crew: you alone. The column went
up the ravine at first light; you are the last vehicle. / Soviet tanks and 45 mm guns have cut
the ravine at the settlement, the saddle and the factory. The rear party holds the ravine head: a
white star over each phase line it still holds, a red one where the enemy stands. / Break through
to the ravine head, 4.9 km: north to the hamlet, then east. Arrive mounted before dark. Nothing
waits for you behind.*

The mid-run complication is the blocking force in the factory yard (platoon B, already authored
in the course's street encounter) with two more sent to the yard's far side when the beat opens.
The opening deployment (three platoons at 16, 31 and 46 %), the adaptive director and every
combatant's laws are unchanged; the beats only cap the director's living armour per phase and add
their own named platoons.

## Act One in the same form

`assets/worldgen/act1/mission.v1.json`: six beats over the existing `ACT1_OBJECTIVES` without
changing a rule. Choose the corridor (route site 1, `Act1ChooseBreakoutRoute`), read the city's
two layers (site 2, `Act1WitnessCityLayers`), recover the third sheet (site 3, on foot inside 150
m then mounted), break the western cordon (the planned blocking force's anchor,
`Act1BreakWesternCord`), the KV-2 (the boss anchor, `Act1DefeatKv2`), the extraction gate (the
route's exit, `Act1ReachWesternExit`). The sheet is enabled at the end of `configure_campaign`;
the beats read the director's objective credits non-destructively before the front end drains
them. No radio lines, no cues: marks, the bearing and the order only.

## Verified by play (27 October 2026, RTX 4090, shader backend)

Filled in from the receipts under `captures/` (see the end of this file).

## Open

- The flare's radiant intensity (`FLARE_RADIANT_INTENSITY`) is set by eye against the muzzle
  flash and the cabin practicals, not from a photometric source. A 26.5 mm parachute star is in
  the tens of thousands of candela; the renderer's unit is not calibrated to candela.
- The smoke column has no flame and no ember light: the fiction is a burnt-out lorry still
  smouldering. A burning wreck of the game's own kind (`Wreck`, surface fire) would be truer; it
  needs a vehicle body at the place.
- The position estimate's error model is a design value (4.5 cm per metre), not a survey of
  dead-reckoning error in a tank.
- The run-loop branch adds its own opening card (`RunBrief`, rules and controls) before play;
  after a merge the order sheet follows it. Folding the order into that card is a one-line change
  in `start_canyon`.
- The platoon branch draws its radio lines bottom left; this branch's net is top left so both
  can show. One net (the platoon's wingmen and the rear party on the same list) is the right end
  state.
- Escort beats have no ally to escort on the Canyon run (the platoon branch's wingmen would be
  the natural ones); the condition is implemented and untested in play.
