# Duel and overturned-vehicle correctness checkpoint — 2026-10-30

The user's archived combat reels showed vehicles repeatedly circling and apparently driving beneath the terrain after overturning. These are separate issues. This checkpoint changes movement/contact behavior; it does not replace the renderer, world generation, or campaign.

## Evidence and cause

- The archived `captures/videos/gallery-artifacts-reel_1b8f797-brawl-brawl.mp4` and matching numbered screenshots show two pairs tracing the same loops at 60, 120, and 180 seconds. They are not prescribed benchmark routes. The actual `Game::enemy_ai` brawl branch pursued the nearest opposing vehicle with full drive beyond 70 m and **0.45 drive inside 70 m**, so it never requested a firing stop. Two turning pursuers can settle into a mutual orbit.
- That brawl branch takes priority over `enemy_policy`. The old harness can report that weights loaded even though the brawl maneuver is scripted. This checkpoint leaves the existing harness unchanged; the new review explicitly reports the policy actually used.
- Ordinary scripted flanking also retained a tangential command after reaching a vulnerable side, and searching retained positive drive after reaching the last-known area. The former now enters a firing band, and the latter stops at its existing arrival distance.
- The suspension integrates heave, pitch, and roll with wheel/belly forces. It has no roof contact or full-body tumble solver. Previously an inverted hull could continue receiving wheel support and powered horizontal travel. There was no requirement that the hull-up axis or loaded tracks support the command.
- A native 240-second sustained-turn regression on level ground passed **before** the fixes. It is incorrect to claim that sustained turning always flips tanks. A simplified numerical reproduction of the existing suspension equations found inversion during a fast turn on a 10% slope; the native slope regression and review receipt distinguish actual observed inversion from an intentionally inverted fixture.
- Sampled night-duel frames also retained an optic vignette after switching to the aerial camera. The new external review clears all relevant optic/cabin flags each captured frame. This is independent of the suspension fault.

## Changes

`vehicle_engagement.rs`, registered beneath `tank`, provides a horizontal range controller: turn toward the target, approach with a braking envelope, hold between 40 and 75 m, and reverse when closer. Those distances are explicit encounter tuning. The actual drivetrain still produces speed and braking; this is not position interpolation. The brawl branch and an already-achieved scripted flank use it. Search stops within its existing 25 m arrival radius.

`Tank` no longer applies powered translation or steering when overturned. When airborne, it retains existing horizontal momentum without adding powered speed/yaw. Track load from the existing suspension is the contact authority after initial placement.

`Ride` latches an inversion when the hull-up axis crosses the horizon. `vehicle_rollover.rs` then retains the overturned pitch/roll, uses the same hull/turret source factories as the drawn vehicle, falls vertically, and arrests penetration against support using source vertices. Player support also queries the existing bridge/floor/rubble support path. Normal drive never scans or loads this extra geometry. Ordinary settlement does not right an inverted vehicle. Explicit ride reset remains a deliberate placement operation.

This is a **bounded inelastic support fallback**, not six-axis crash simulation. Angular tumble is arrested at the retained pose. Barrel digging/deformation, sliding or rolling to a new stable pose, hull/terrain triangle CCD, and recovery towing are not implemented by this change. Vertex contact is exact on a plane but may miss terrain peaks between source vertices. The rolled Tiger's much larger source needs profiling; no normal-drive source-scan cost was added.

The follow-up contact audit found two integration faults: the generic contact correction and winter wall clamp could replace an overturned body's falling Y with terrain height, and a popped turret could still support the hull through its old mounted geometry. Both XZ constraints now preserve the rolled vertical state. Contact uses the same `WreckKind::TurretPop` membership as rendering, and a destroyed rolled hull gets one vertical step per host update because dead actors no longer run AI movement. A newly detached supporting turret therefore lets its hull fall continuously onto its own remaining source geometry.

Root's first 120-second build-16 receipt (`captures/vehicle_brawl_fix_20261030_a.json`) shows four vehicles reaching rest without rollover. It is movement validation, **not battle validation**: no shots were recorded. The subsequent source audit found that brawl movement and gun aim chose an opposing vehicle while `enemy_mode` still checked visibility to the distant referee Tiger. The explicit brawl mode now uses the same live, fielded opponent for its sight gate and gun lay, retaining optical obstruction, first-shot delay, reload, aim and muzzle-lane checks. It does not force Track or damage. The review no longer repeatedly forces player detection and now records opponent, mode, both sight gates, first-shot age/delay, reload, turret/gun pose, aim error, and actual shot sequence.

The first deliberately inverted review retained source clearance and zero travel but exposed an upside-down gun still traversing and firing once. AI crew incapacity after inversion is now an explicit authored rule: the enemy's vertical physics advances once before the engagement branches, while gun lay and discharge are disabled. It preserves liveness rather than declaring a kill. The inverted review uses a 23 m exterior inspection view of that hull and fails verification if the actor's shot sequence is nonzero.

## Verified runs

The parent executed these runs; the receipts and relevant log were inspected for this update.

| Evidence | Verified result | Boundary |
| --- | --- | --- |
| `captures/vehicle_brawl_fix_20261030_b.json` and `.mp4`; `artifacts/expedition-20261030/vehicle-brawl-b.log` | 90 simulated seconds, 90 samples, four T-34s. Three actual main-gun discharges and three logged BR-350B penetrations (160, 115 and 116 m). Three knocked-out vehicles, including enemy 2's ammunition detonation/turret pop; enemy 4 survives and stops. No sampled vehicle overturned. | Actual brawl AI and projectile outcome, not forced kills. This seed is not a combat-balance assessment or proof of every encounter. |
| `captures/vehicle_inverted_fix_20261030_b.json` and `.mp4` | 20 simulated seconds, 20 samples. Deliberately inverted enemy 2 retains roll π, zero XZ displacement, zero speed and zero shots. Turret yaw and gun pitch remain zero. Source-body clearance is 0 to 0.000000238 m (floating-point tolerance). The receipt's `inverted_fixture_zero_shots` check is true. | Deliberate inverted placement, not a naturally generated rollover. The body remains alive but its crew cannot fight; other vehicles in the scene still fight. |

Both reviews simulate at 60 Hz, sample at 1 Hz and produce silent 12× accelerated playback. Sparse samples establish the recorded states, not continuous visual inspection of every physics step. In the inverted receipt, the sight-derived mode can still read `track`; `crew_incapacitated_by_rollover=true` is the earlier action gate that prevents laying or firing. Clearance is against the loaded-track support surface, which includes the existing snow-compaction convention, not necessarily the top of drawn snow.

## CPU verification

`artifacts/expedition-20261030/focused-tests-19.log` records **nine relevant passing tests**: four `vehicle_ground` tests (including detached-turret support), two `vehicle_engagement` tests, and the three Game regressions for overturned crew incapacity, brawl target selection and preserved falling Y. No failure or ignored test appears in those selected runs. The complete log contains 42 passing selected tests across these and other work areas; this is not a full-suite result. The older baseline flat-turn result remains in `artifacts/expedition-20261030/vehicle-turn-baseline.log`.

Relevant filters:

- `vehicle_engagement::tests`: commands and two actual T-34 drivetrains reaching/resting at a firing position instead of mutual orbit.
- `ai_sustained_turn_does_not_overturn_on_level_ground`: 240 seconds, both 0.45 and 1.0 drive.
- `an_overturned_tank_cannot_drive_its_roof_through_the_ground`: actual source body stays above terrain, pitch/roll retained, no powered travel.
- `airborne_tracks_cannot_add_speed_or_steering`: existing velocity retained, no airborne steering or ground teleport.
- `sustained_turn_on_slope_keeps_source_body_above_ground_after_any_rollover`: native 10% slope, exact first inversion logged if it occurs; does not require inversion to happen.
- `inverted_source_roof_stays_above_ground_without_righting_pose`: analytical source-point contact fixture.
- `detaching_the_supporting_turret_makes_the_hull_fall_onto_its_own_source`: detached-part support and continuous falling of a destroyed inverted hull.
- `horizontal_contact_correction_preserves_an_airborne_rolled_wreck`: actual Game reground helper retains falling Y for live and dead inverted bodies while ordinary upright reground is unchanged.
- `brawl_target_selection_excludes_referee_teammates_inactive_and_destroyed_hulls`: fielded opposing-team identity selection.
- `incapacitated_enemy_keeps_falling_without_laying_its_gun`: the actual early AI branch advances gravity while both aim APIs retain gun pose and refuse fire readiness.
- Existing `suspension::tests`, `vehicle_ground::tests`, and `tank::tests` for normal-drive regression coverage.

## Bounded visual review

`vehicle-motion-review PREFIX [SECONDS] [inverted]` runs actual `Game` brawl AI at 60 Hz, captures one frame and four vehicle records per simulated second, and writes a **12× accelerated, silent** MP4. Audio is configured offline before Game construction. Output PNG/JSON/MP4 names are flat and the prefix must be unused. Durations are bounded to 10–180 seconds.

The module and CLI are registered. Example commands for a new review (use an unused prefix):

```text
vehicle-motion-review captures/vehicle_brawl_fix_20261030_c 90
vehicle-motion-review captures/vehicle_inverted_fix_20261030_c 20 inverted
```

The second command deliberately places enemy 2 upside down at the start and labels that in the receipt. It proves a retained contact state; it does not prove the live encounter naturally generated a rollover. The JSON records actor ID/type/liveness, position, heading, pitch, roll, vertical hull-up component, speed, inversion state, origin height relative to terrain, and source-body clearance for overturned vehicles. The ordinary brawl does not force damage, overturning, or a victory. The referee Tiger is invulnerable as in the original harness.
