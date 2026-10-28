# Vehicle dynamics: mass, momentum and suspension for every tank

Branch `vehicle-dynamics`, 2026-10-28. Ledger: docs/PHYSICAL_TRUTH_AUDIT.md, "Vehicle dynamics".

The playtest report: the Tiger "feels like 5 lbs", gets stuck on branches, bounces around; other
tanks' tracks are visible now, so they need suspension too, historically. What was wrong:

- The hull had no suspension. `tank_support::settle` (player) and `vehicle_ground::settle` (AI)
  put a rigid hull on the highest supports under its tracks every frame, so each pebble, rubble
  piece or scatter support snapped it up at once (rate-limited only at 1.5 rad/s and `2 v + 0.1`
  m/s). That is the bounce and much of the weightless feel.
- Every AI tank used one kinematic controller: 12 m/s top speed, 6 m/s^2 (0.6 g) acceleration and
  deceleration, 0.7 rad/s turns, identical for a 28 t T-34 and a 52 t KV-2.
- The player Tiger already had a force-based drivetrain (`powertrain`) and a momentum push model
  (`tank_push`); the "stuck on branches" part was the scatter tiering (fixed on `scatter`, merged:
  pieces under 5 kg / 0.15 m have no vehicle collision). The push model is unchanged and still
  reads the drivetrain's speed, so what the scatter tiers call crushable is crushed and heavy
  pieces are shoved by momentum.

## What is new

| Module | What it does |
|---|---|
| `src/vehicle_spec.rs` | One sourced row per vehicle: Tiger I, T-34/76, SU-100, KV-2, IS-2, ISU-152, IS-3. Mass, engine power and rated rpm, gears and their road speeds, documented road speed, steering yaw rate, suspension type, stations per side, wheel diameter, gauge, track width, contact length, hull size, clearance, CG height, unsprung share, spring rate or static deflection, jounce and droop travel, friction and shock-absorber damping, step/trench/gradient. Tiger II and Panzer IV exist only as editor showcase names; nothing drives them. |
| `src/suspension.rs` | `Layout` (stations on the drawn hull) and `Ride` (heave, pitch, roll integrated at <= 1/240 s): per-station spring, damper, droop (wheel leaves the ground), bump stop, wheel-radius envelope over the supports, the track nose lifting the end stations, belly contact, squat/dive/roll load transfer. `HeadSway`: the crewman's head on his neck. |
| `src/powertrain.rs` | The player's drivetrain, now parameterised by the spec row (the Tiger's numbers are unchanged; its tests pass as before). `Powertrain::for_ai` (full grade; the owner-directed climb assist stays player-only). `ai_driver`: the AI's speed command becomes throttle and brake pedal. |
| `src/tank.rs` | AI tanks (`Tank::update`) drive through their own `powertrain` and `ride`; the interface to the AI is unchanged (drive, steer in [-1, 1]; drive is the wanted speed as a share of the vehicle's own road speed). |
| `src/vehicle_ground.rs` | `settle_ride` (AI ride on the heightfield), `settle_rigid` (spawns, placements), `settle` keeps a running ride's pose after in-frame constraint/contact pushes and re-seats only on a support it did not see (the Act-II bridge deck). |
| `src/solid_city.rs` | The player hull rides its springs on the supports `tank_samples` finds (terrain, rubble, building solids); the rigid rest still decides refusals and supplies the grade. |
| `src/game.rs` | Head sway applied to the in-hull camera (not the gunner's sight). |
| `src/vehdyn_trace.rs` | `vehdyn-trace PREFIX [SEED] [FROM_M] [SECONDS]`, the headless receipt below. |

The refusals did not change: step, trench, gradient, tipping and belly hang-up remain
`tank_support`'s rigid-rest rules and decide what the hull may cross; the sprung model decides how
it moves while crossing it.

## The numbers (from `cargo test ride_table -- --nocapture`)

Ride frequencies on each vehicle's historical contact patch; standing start and stop on the level
through the drivetrain with the AI driver.

| vehicle | t | PS/t | k station kN/m | static defl. m | heave Hz | pitch Hz | roll Hz | 0-20 km/h s | speed after 60 s km/h | stop from 30 km/h m |
|---|---|---|---|---|---|---|---|---|---|---|
| Tiger I | 57.2 | 11.4 | 237 | 0.126 | 1.41 | 0.80 | 1.41 | 9.8 | 32.2 | 9.8 |
| T-34/76 | 28.5 | 17.5 | 151 | 0.159 | 1.25 | 0.85 | 1.30 | 5.2 | 49.6 | 10.4 |
| SU-100 | 31.6 | 15.8 | 170 | 0.159 | 1.25 | 0.89 | 1.34 | 6.1 | 45.6 | 10.3 |
| KV-2 | 52.0 | 10.0 | 288 | 0.127 | 1.40 | 0.96 | 1.34 | 9.2 | 29.5 | 9.9 |
| IS-2 | 46.0 | 11.3 | 255 | 0.127 | 1.40 | 0.97 | 1.35 | 9.3 | 32.6 | 10.1 |
| ISU-152 | 46.0 | 11.3 | 255 | 0.127 | 1.40 | 0.96 | 1.41 | 9.5 | 35.2 | 10.2 |
| IS-3 | 49.0 | 10.6 | 272 | 0.127 | 1.40 | 0.96 | 1.40 | 10.4 | 33.0 | 10.3 |

Before, every AI vehicle reached 20 km/h in 0.9 s and stopped from 30 km/h in 5.8 m (6 m/s^2 both
ways). The Tiger's rates come from its torsion bars (55 mm, 58 mm at the ends, 1.89 m, tiger1.info)
over an estimated 0.41 m arm; the others' from a static deflection chosen for a named heave
frequency. The Tiger's end-arm shock absorbers damp its pitch; the T-34's Christie springs have
none (sourced), so it pitches longer after a bump or a stop
(`the_tiger_pitch_settles_faster_than_the_undamped_christie_t34`).

## Load transfer and feel

Accelerating squats the tail and braking dives the nose by `m a h / K_pitch` (Tiger: about 0.6
degrees per m/s^2, so a full 0.65 g stop dives the nose about 3.5 degrees), a turn rolls the hull outward; the camera rides the hull, and the crewman's
head lags the seat on a 1.8 Hz, 0.5-damped neck (8 cm travel): a lurch forward when the hull stops,
a lean in a turn, no painted shake. The existing crush shudder (`crush_rumble`) is unchanged.

## Headless receipt (`vehdyn-trace`)

One headless process (GPU hold), Canyon v2 (`gaea-canyon-run-v2/manifest.v1.json`), seed 29,
raymarch backend:

```sh
KESSEL_ASSET_ROOT=C:/shop/panzerrechnersystem KESSEL_RENDERER=raymarch KESSEL_CANYON_MANIFEST=C:/shop/panzerrechnersystem/assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json target/release/panzerrechnersystem vehdyn-trace C:/shop/panzerrechnersystem/captures/vehdyn_20261028_s29 29 450 75
```

The Tiger is staged (disclosed, god mode: a ride trace, not survivability) 450 m along the route
and the commander's drive orders take it 359 m on through the hamlet street and the rough ground
(75 s, up to 7.7 m/s, 5th to 8th gear; carried by the terrain throughout). The combat pilot
(`VEHDYN_PILOT=1`) was tried first and spent the run fighting and reloading at the staging point,
which is its job but not a ride trace. A T-34 is then staged at the same point and steered along
the same route at 60 % of its road speed (480 m). Each frame logs the sprung pose beside the rigid
pose the hull used to take (player: `tank_support::settle`'s rest; T-34: the six-contact fit).
Files: `captures/vehdyn_20261028_s29_{tiger,t34}.jsonl`, `_summary.json` in the main checkout.

| | Tiger, rigid rest (before) | Tiger, sprung (after) | T-34, rigid fit (before) | T-34, sprung (after) |
|---|---|---|---|---|
| peak pitch rate, deg/s | 88.3 | 15.3 | 28.1 | 20.0 |
| peak roll rate, deg/s | 77.8 | 10.9 | 19.4 | 9.3 |
| RMS pitch rate, deg/s | 6.57 | 3.02 | 3.48 | 6.22 |
| RMS roll rate, deg/s | 5.28 | 1.81 | 4.29 | 2.07 |
| RMS heave acceleration, m/s^2 | 4.70 | 0.44 | 11.66 | 0.37 |
| largest height step in one frame, m | 0.021 | 0.006 | 0.034 | 0.007 |

The Tiger's peak pitch rate before is the old 1.5 rad/s rate limit: the rigid rest snapping onto
each support. While moving, the sprung pitch stays within 2.2 degrees of the rigid rest (mean 0.46);
wheel travel reaches 0.12 m. At 57.6 s the drive's vehicle avoidance brakes for something on the
route: a steady 2.2 m/s^2 stop from 6.8 m/s over 2.4 s, the nose dives 2.1 degrees under braking
and rocks back 2.2 degrees past level as it stops (the rigid rest stayed within 1 degree of
level throughout).

The T-34 heaves far less but pitches more than the rigid fit: its Christie springs have no shock
absorbers (sourced), so with only friction damping (0.08, an estimate) it keeps porpoising after
each bump and gear change. That matches the type's reputation, but the friction figure is the
estimate to revisit if it reads as too lively. On the drivetrain it reached 60 % of its road speed
(8.1 m/s) in 11.7 s; the old controller reached it in 1.35 s (6 m/s^2).

The player's longitudinal speed model is unchanged (it already was the drivetrain); its grade now
comes from the rigid rest pitch, not the sprung pose, so squat and dive do not feed back into the
climb.

## Visible running gear: what was possible

The baked Tiger hull (`tiger_hull.glb`) keeps its 48 road wheels as separate scene-root nodes
(three interleaved wheels on each of eight arms per side, 0.88 m across as drawn, stations
0.571-0.573 m apart), and its track links are separate nodes too. The player layout's stations are
those wheels' measured centres (`suspension::TIGER_STATION_RUNS`, re-measured by
`tiger_stations_match_the_baked_wheels` through `gltf_load::glb_root_bounds`), and `Ride::travel`
gives every station's wheel travel every frame. The loader can already split named scene roots into
separate meshes (`load_glb_partitioned_roots`).

Not done: drawing them. Splitting the hull into a body and sixteen station groups (wheels plus the
bottom-run links under them) means new scene meshes and slots in `Game::build`, a per-frame
`set_model` per station, and keeping the full hull for the registrations that read it (fire and
armor surfaces, the track footprint, snow); the AI meshes have not been checked for separable
wheels. That is a follow-up with its own captures, not part of this change.

## Not done / follow-ups

- Wheel and track articulation on screen (above).
- The spec's step, trench and gradient limits are not wired into the AI vehicles' gating, which
  still uses `terrain::TANK_STEP_M` for every kind.
- Unsprung (wheel and track) bounce, track wrap and tension, and soft-ground sinkage per vehicle
  are not modelled.
- LAN clients receive position and heading only, not the sprung pitch and roll.
