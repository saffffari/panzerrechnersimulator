# Infantry swarm: plan, costs and approximations (9 November 2026)

The owner, 9 November 2026: "infantry climbing over the tank in infestation ... they need to enter
the vehicle through any open hatches they can find. when smaller in number infantry should still
attempt to toss grenades or molotov cocktails at the tank which could cause the interior to heat
up if fires are not extinguished. infantry need to be more intelligent though they should try and
stay to cover and also avoid the main gun when they can. lets try and make the infantry vastly
more numerous, and increase their involvement in the game".

Switch: `KESSEL_INFANTRY_SWARM`. Unset: on for the two playtest courses (`spring-bridge-v2`, the
menu's "Bridge: Take and Hold", and `canyon-v2`, "Canyon Run"); `0`: today's garrisons exactly
(60 men, no swarm code runs); `1`: on for any course map.

## What exists and what it costs (stage A, measured)

The Soviet riflemen are `infantry::Soldier` (pose, animation, corpse) paired one-to-one with
`infantry_combat::Combatant` (the shared, renderer-free combat authority used by training) and
stepped by `mixed_combat_runtime::update_mixed_infantry`: sensing (`sensed_actors`, `observe`),
squad cohesion and hearing (`infantry_squad`), a 5 Hz policy or doctrine action, movement against
the world's cover index. Both playtest courses field 60 (`CANYON_SOLDIERS`) at the course's posts
(`canyon_game::enable_canyon_course_seeded`, groups of six). The draw is already instanced: 17
moving pose pools of the one authored character (`soviet_pose_batches`), one matrix per man.

Measured with `swarm-bench` (new, `src/swarm_bench.rs`): the course built as the menu builds it,
the Tiger invincible and set down, 60 Hz simulation and a fenced 1024x576 render per frame
(raymarch, high preset, deltap-real 15,15,7), RTX 4090, game and editor not running.

| Run (game camera, 20 s) | Soviets alive | Infantry phase ms (mean / p99) | Simulation ms | Render ms | Frame ms (mean / p99) |
|---|---|---|---|---|---|
| Bridge, swarm off, Tiger at the west edge | 60 | 1.60 / 2.49 | 3.76 | 37.7 | 41.5 / 53.7 |
| Bridge, swarm on, same place | 156 | 0.75 / 2.15 | 16.5 | 45.6 | 62.1 / 84.3 |
| Bridge, swarm on, 48 men staged round the Tiger on the bridge, hatches open | 156 (64 within 200 m) | 2.89 / 7.13 | 11.8 | 40.0 | 51.8 / 86.0 |
| Canyon v2, swarm off | 60 | 1.29 / 1.90 | 5.54 | 36.6 | 42.2 / 47.0 |
| Canyon v2, swarm on | 156 | 0.83 / 1.17 | 5.01 | 36.6 | 41.6 / 45.9 |

- One idle soldier costs about 25 us of simulation a frame today (1.5 ms for 60 mostly idle men).
- A soldier near the Tiger cost about 0.2 ms a frame: `small_arm_player_aim_point` sweeps the
  hull's source triangles for every crew head and the hull centre, every frame (6.9 ms for 35 men
  in the first staged run). Cached at sensor cadence (below) the staged assault's infantry phase
  fell from 8.9 to 2.8 ms.
- Draw: the 96 extra men are instances in the existing pose pools; the canyon run (men out of the
  view) shows no render difference (36.6 vs 36.6 ms).
- **The frame target is not met by this headless harness even with the swarm off** (render 37 ms
  from the game's cabin camera on both courses). This measures a fenced frame, simulation then
  render, with no pipelining; the live window overlaps them. The swarm's own cost is the
  difference between the rows.
- **The large cost the swarm brings is the existing fire presentation.** A petrol bomb lights the
  existing source-surface fire; while the Tiger burns, `drive_interior_fire` takes 8-12 ms of
  simulation (10 ms of it `fire_smoke_clearance`, 3,072 smoke cubes a patch tested against the
  source mesh) and the render 8 ms more. This is the same cost any cabin fire has today; the brief
  says new fire uses the existing fire rendering as it is, so it was not changed. Two exact
  accelerations were added (empty frames skipped in `clear_source_paths` and the fire sweep);
  they did not measurably move it. This is the first thing to fix for the frame target.

## What was built

| Stage | Where | What |
|---|---|---|
| B numbers | `infantry_swarm::SWARM_SOLDIERS` (156), `game.rs` build, `canyon_game.rs` | 156 Soviet riflemen on a swarm course, groups of six; a post may hold several groups on their own ground (rings of 22 m, 20 m apart); a group with no clear ground is not fielded (counted). Far men (over 180 m from the Tiger and the man on foot, no contact) are stepped every 4th update, over 400 m every 8th, with the summed time; staggered. The Tiger aim point is resampled every 0.2 s per man and kept in the hull's frame between samples. |
| C staying alive | `infantry_swarm.rs`, `swarm_runtime::swarm_think` | Each man who can see the turret (line of sight within 300 m, a look every 0.25 s) records the gun's and hull's pointing and measures the traverse from his own two latest looks. From that alone: danger arcs (the gun and coax: half the TZF 9b's field; the bow MG: its ball's traverse plus its sight's half field), the time until a swinging gun reaches him. He goes to ground (prone: the prone wounded man's rotation, eye and contact volumes) when in an arc or about to be, stands after the arc has been away 1.2 s, bounds at 3.6 m/s to the best of twelve candidate points toward the Tiger's blind side (covered from the muzzle first, then low danger, then progress; a covered man never leaves cover for open ground) only when the arc cannot reach him before he arrives, and otherwise holds. Assaulting men do not rout when a comrade falls. |
| D grenades and bottles | `thrown_weapons.rs`, `exterior_fire.rs`, `swarm_runtime` | RGD-33 and F-1 grenades (published mass, filler, fuse ranges) and petrol bottles. Two grenades a man, a bottle with every third. Within 7-24 m, with the arc away, a man stands, winds up 1.1 s and throws on the drag-free ballistic solution with a range-scaled aim error, at an open hatch he can see, else the engine deck over an intake. Flight with bounces off the Tiger's source surfaces and the world; the fuse bursts it: the existing exterior blast law for the men round it (boarders included), the man on foot, and the crew's existing casualty route (fragments swept against the source armour and lids, pressure where the path is open). A bottle breaking on the Tiger lights three existing surface-fire patches at its splash; the swarm's heat path feeds the existing cabin heat model; the existing extinguisher's foam puts them out. |
| E climbing aboard | `infantry_boarding.rs`, `swarm_runtime::swarm_step_boarder` | When six or more men are within 30 m and the Tiger is under 2.5 m/s, men within 14 m run to the nearest climb place, climb 2.4 s, then walk the deck graph (feet on the actual source surface swept under them) to hatches, the engine deck and the driver's visor. Shut hatch: hammered 4 s, then the next. Open hatch: a grenade dropped in, shots fired down at a crewman in sight through the ordinary projectile path, or a man down into the aperture fighting from it (the lid cannot shut on him). A man with a bottle breaks it on the engine deck. A man at the visor stands in front of it (his body blocks it). Thrown off by the hull's acceleration at his point past his grip, or by the barrel sweeping through him; falls and lands by his speed. Counters: closing hatches, driving hard, traversing the gun across the hull deck, the commander's P08 (the ordinary projectile hits a man on the deck), a wingman's coaxial MG in platoon mode. |
| F sound hooks | `swarm_sound.rs` | 22 named events (NF-01, 03, 04, 10-15, 17, 18, 21, 23-27, HT-18, HT-21, FR-01, FR-03, FR-07), counted and logged; each plays `assets/sounds/<ID>_<nn>.ogg` from the place if such a file exists, silent otherwise. Grenade bursts also play the existing HE burst report and pressure front. |

## Approximations (each in docs/PHYSICAL_TRUTH_AUDIT.md, "Infantry swarm")

See the ledger section for the list with sources and estimates. In short: cadenced stepping of far
men and cadenced aim points; danger arcs from sight fields, not the gunner's actual intent; a
fixed rush speed; drag-free throws; per-surface bounce coefficients; a hashed fuse inside the
published range; a bottle-break speed; fragments sampled as 48 rays with an exponential speed
fall-off; one wound per crewman per burst (the part with the most work); exterior bursts do not
damage the Tiger's armour, tracks or vision blocks; a grenade inside does not strike the rounds
or heat the gas; a bottle broken on the ground leaves no fire; the heat path shares (intake 0.8,
plate 0.03), the engine bay's ignition after 20 s of intake fire, the fixed extinguisher's 6 s
delay and single charge; the deck graph and its places; grip 0.42 g; climb 2.4 s; landing
thresholds; no throwing pose (a thrower is drawn at aim); no prone frame (the aim frame laid flat);
far men walk in place between their steps; a man in a hatch stands in the aperture, not in the
cabin's spaces.

## Not done

- Infantry waves with the mission's counterattacks (the garrisons are denser; no new waves).
- The Tiger's AI crew does not react to boarders (the player drives, traverses, shuts hatches and
  shoots); AI crewmen do not fire pistols from the hatches.
- Bundled charges and anti-tank grenades on the deck (no armour solve for a contact charge).
- A throwing pose and a prone frame in the authored Soviet library.
- The frame target (above).
