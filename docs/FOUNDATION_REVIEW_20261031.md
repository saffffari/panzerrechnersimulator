# Foundation review â€” 31 October 2026

Scope: repair current raymarching-branch behavior before adding features. Existing dirty work preserved; no branch switch or publication. This record is provisional until the validation section is completed.

## Findings and corrections under verification

- Crew mechanical targets advanced every simulation tick while skins could wait for the 30 Hz idle clock. Changed grips now update immediately; error receipts compare against the current target. Loader zero-time restoration and turning discontinuities are corrected. Player mechanical grips receive the same cadence correction.
- Low-quality cached optics could project an old capture using the new camera. Captures now retain the actual encoded camera/extent/frame and invalidate when their source changes. Gunner eyepiece acceptance is computed per source fragment through a finite exit pupil. Other vision-block optical approximations remain documented.
- Exposure metering formerly used one large serialized workgroup. The new scheduling distributes the same sample accumulators over more groups, preserving sample order, reduction order and adaptation. Actual GPU comparison passed 144 whole-state, bit-identical cases; this is correctness evidence, not a measured speedup.
- Dynamic wrecks need source support and gravity, not a fixed overturned transform. Fallen trees need their actual root/soil intersection and whole-source ground contact. Runtime loose-rock placement must use the same supported transform for rendering, cover and collision. Authored assemblies are preserved.
- A surviving field soldier could be omitted from succession after the registered replacements were exhausted. The correction preserves the actual person, location, wounds and inventory. The build46 passenger review now passes all five checks, including a cold resume during the pending physical exit and control transfer to that same survivor.
- Ammunition carts could use terrain below a bridge and move the operator around an unchecked arc. Movement now retains the real operator offset and admits the coupled cart/person motion against the retained world surfaces.
- Carrier casualty transport used side berths that intersected the retained benches/floor. One centered berth now fits the actual aisle and body; the existing winch must be physically moved clear. Withdrawal checks the complete body at the rear gate. Legacy incompatible berth occupancy is rejected explicitly, without deleting or healing anyone.

## Performance conditions

The RTX 4090 measured 97â€“99% utilization with this game stopped and KSP open. The owner explicitly requested that KSP remain open. Current rendering checks therefore establish appearance and behavior only. Older frame-rate numbers lack controlled contention and do not establish standalone performance. No 30/60 FPS claim is made.

## Evidence recorded so far

- Baseline b43 CPU suite: 2,284 passed, 0 failed, 78 ignored, 75 network-related cases filtered.
- b44 suite: 2,304 passed, 2 failed. The exposure binding assertion was corrected; further real-source tree tests found root obstructions.
- b45 suite: 2,314 passed, 7 failed, 81 ignored, 75 filtered. All crew/contact cadence, field succession, cart and eye-meter checks passed. Five detached-turret and two tree cases failed and remain release blockers until rerun.
- b45 private stretcher/source withdrawal test passed. The live medical check then failed while moving the real winch aside, before casualty loading. That route is being corrected.
- b45 actual Tiger support cost: 1,796,654 retained source vertices; 25.88 ms for an exhaustive support query and 77.25 ms for one settling step in the CPU test build. These figures triggered exact BVH acceleration rather than accepting the new cost. They are not game FPS.
- Targeted b45 rendering contract checks found two stale test harnesses: the extra non-radiance meter-sum buffer and a missing weather-field shader include. Their invariant checks are updated to the actual production composition, pending rerun.
- `artifacts/renderer-performance-20261031/exposure_scheduling_b44.json`: actual GPU exposure equivalence, 144 comparisons passed.
- `captures/foundation_loader_20261031_b44_manifest.json` and matching MP4: one ordinary discharge, one finite round consumed, reload complete by 8 seconds, loader returns to standing. Active hand-target error at one-second samples was at most 0.000000477 m. This does not prove clearance between samples or wrapped finger grasps.
- `captures/foundation_medical_20261031_b43.json`: preserved failed baseline. Transport initially progressed, but the patient path and physical berth failed review. Do not present this as successful end-to-end evacuation.
- b47 full regression: 2,356 passed, 6 failed, 83 ignored, 75 filtered. Failures were two detached-turret settling cases, three tree-support cases and the loose-rock scale fixture. Its separate private static-wreck test also failed. The rendering-contract runs passed 3 and 32 tests respectively, with one ignored; the previous shader-composition harness failures were cleared.
- b48 focused reruns cleared those support failures: detached turrets 9/9, trees 6/6, scatter support 6/6, loose-rock decoded scale 1/1, static wreck support 8/8 and static wreck ownership 4/4. Terrain bounds 6/6, physical terrain state 6/6 and world-save tests 5/5 also passed. These are focused results, not a fresh full-suite pass. Logs are `artifacts/expedition-20261030/foundation_b48_*.log`.
- b48's actual cold-terrain GPU test passed its exact before/after checks: 338 height samples, five opening-mask samples, resident-page membership, uploads and unchanged physical owner payload. The test as a whole **failed** its separate CPU/GPU parity assertion: 2.667353 mm at `(0.625, 1.625)`. CPU relief used the documented fixed 1.2207031 m wavelength while the shader used this fixture's 1 m grid spacing. Independent f32 evaluation reproduced the difference to about 7.5 nanometres; this was not a restore failure or rut-precision explanation. The far-index terrain-cache regression also failed in b48 and requires its corrected-fixture rerun.
- b49 release and native compilation passed. Full non-network regression: **2,372 passed, 0 failed, 85 ignored, 75 filtered**, 228.52 seconds. Its shared physical-wavelength fix preserves the CPU surface and corrects GPU geometry and normal filtering. The separate ignored GPU restoration test also passed: all 338 heights and five opening probes match exactly after reconstruction; maximum CPU/GPU height gap was 0.0000086 m, within the original strict tolerance. Logs: `foundation_b49_full.log` and `foundation_b49_terrain_gpu_restore.log` in `artifacts/expedition-20261030/`.
- b49 live medical review passed field care, complete patient loading, a 30.5 m journey, unloading and complete source-body clearance. It then failed because the automated helper approached the medical tent through closed canvas at local z=2.07 instead of its central opening. The collision was correct. Controller-only doorway/bedside routing corrections await b50; the failed video and receipt are preserved.
- b49 actual canyon placement receipt: **124 admitted, 140 unresolved originals, 109 preserved assembly components**. The unresolved placements comprise 83 unstable, 35 search-budget and 22 post-clearance failures. Three canyon rock prototypes also failed source admission. These real-map findings remain open despite the green unit suite. The first overview centers an unresolved PzIII and must not be described as a corrected placement.

## Limits still requiring work

### Build57 CPU checkpoint

Native non-network regression passes **2,390 tests, 0 failures, 92 ignored, 75 filtered**
in 274.97 seconds with normal Windows temporary-file access. The five held-cartridge
regressions pass. A paused loader now retains the round in the hull frame so common vehicle
motion also moves its rendered mesh, hand targets, picking geometry and eventual release.
Turret-only rejected-proposal behavior is unchanged; no source-sweep collision claim is made.
New saves use crew owner version4. Legacy version3 payload conversion uses the saved hull;
its measured maximum source-vertex error is 0.244617 mm at the remote camp coordinates,
within the unchanged 1 mm conversion allowance. This is not arbitrary cross-build save
compatibility: the existing source/build fingerprint gate remains active.

The two build56 test failures came from a ray exactly on a tessellation seam, where the
existing strict triangle picker can select the opposite wall after micron-scale roundoff.
The corrected frame regression targets an actual face interior with unchanged tolerances
and retains the seam result as a diagnostic. Production seam picking remains an open issue.

Release57 compilation passes. Its full-Game review additionally stows the reading torch
through the normal action before the trigger; that review-only helper line was added after
the native57 test executable was compiled. Gameplay/save source is otherwise the same.
The actual map comparison passes preservation of all 145 placements admitted by either
build52 or build54: **146 admitted, 118 unresolved (93 unstable / 25 search-budget),
109 preserved assembly parts and 43 embedded formations**. No prior admitted identity was
lost, and preserved identities/reasons and formation records are unchanged. Preparation
took 51.678 seconds including prototype loads, not a frame-rate measurement. The new
overview and receipt are published in the review page.

The build57 moving-loader recording passes all 11 declared outcomes and required controls:
same slot49 carried at 5/6 seconds (0.669/1.499 m/s), rammed at 7 seconds (2.189 m/s),
reload complete by 8 seconds and idle at 9. The observed stop occurs between 7 and 8;
the earlier brake input does not establish earlier deceleration. Sampled current loader
hand error stays below 0.016 mm. Root and independent review inspected frames at 5, 6,
7.5 and 8 seconds. No clear world-stranded round is visible, but hand/brass occlusion,
uncurled fingers and incomplete swept contacts prevent a finished-animation claim.
The 18-second, 24 fps offline film has game sound and is published alongside the earlier
parked reload. Its internal render is 640x360; encoded 1920x1080 is integer-scaled output.

### Build58 actual cold-resume result

The actual full-world review passes, including active **Carry and Ram**, exact zero-tick
cartridge identity/world pose and all owner bytes, then 12 identical ordinary 120 Hz ticks
after each restoration. The map/manual, retained frontend candidate and corpse-allocation
branches also pass. Receipt: `captures/foundation_world_20261031_b58.json`.

The preserved build57 failure was an unequal review setup: the initial manual candidate
retained audio simulation, whereas the restored Carry candidate disabled it. Only the
acoustic state differed after continuation; cartridge state and the initial restore were
exact. Build58 makes the intended headless mute policy equal on both sides and records
assertions for it. No owner comparison or tolerance was removed. Release58 changes only
the review harness; the production code remains the version covered by the 2,390-test run.
This does not verify live audio-device phase or acoustic convolution-cache continuation.

### Build55 medical result and build56 verification

The continuous medical journey now passes **all 15 checks**, in 343.427 seconds of simulation.
Only the two spare stretchers were moved outward to open the actual furniture aisle; the
four stretchers, two treatment beds, patient identity, anatomy and finite medical capacity
are retained. Source-body yaw checks pass 9/9, the spare-layout replay passes 1/1 and the
helper-route check passes 1/1. The same wounded man receives field aid, rides in the carrier,
returns on a fuel-consuming drive, leaves through the real gate and reaches a named medic.
The uncut video and receipt are `captures/foundation_medical_20261031_b55.mp4` and `.json`.
An 83-second normal-speed excerpt is published alongside the full run in the review page.
It includes cuts; it must not be described as a continuous 83-second journey.

This verifies transport/care state, not finished animation. The admitted patient keeps his
seated joint angles because bed placement is rigid and camp-owned bodies skip ordinary
settling. Supported limb placement remains unfinished. The final camera also retains its
crouched aim after standing and looks too high; the body is still present. The slow drag
across camp takes roughly three minutes in this cautious scripted route. These issues must
remain visible when assessing pacing and presentation.

Build56 native compilation passes, and static wreck tests pass **15/15**, including actual
source regressions that had lost their previously accepted placements. Actual full-map
placement verification is pending. The held-cartridge frame correction passes 3/5 focused
tests; two picking-distance assertions require diagnosis before it is called verified. No
full-suite pass or finished loader-motion claim is made for build56.

### Build54 checkpoint

Release and test compilation passed. Focused native results: patient-yaw checks **8/8**, actual
helper route **1/1**, static support **14/14**, including the previously failing actual Canyon260.
The default non-network suite passed 2,370 cases but 15 filesystem cases failed because Windows
denied temporary-directory writes. Those exact 15 cases then passed under the normal native
execution permissions, with no source changes. This is 2,385 distinct passing cases across the
run and its targeted rerun, not a single uninterrupted green run; 89 were ignored and 75 filtered.

The real map now reports **143 admitted / 121 unresolved / 109 preserved**, plus 43 embedded
formations, in 49.475 seconds of preparation. Index260 is admitted using the explicitly marked
initial-placement neighbor search. Comparing all identities found two previously admitted
placements (11962 and 12521) lost when eight search attempts were reserved for the fallback.
That regression needs correction before considering the new budget allocation complete.

The medical recording still fails and remains unpublished as a success. Steering is now correct;
the actual injured Driver catches on a spare stretcher stand because the furniture aisle is too
narrow for his asymmetric retained pose. The exact final anatomy and original source vertices
are saved in `captures/foundation_medical_20261031_b54.json` for replay. The next correction moves
the two visible spare stretchers outward; it must preserve every stretcher, both treatment beds,
the finite care capacity and actual collision geometry.

### Build52 checkpoint (supersedes the older pending status below)

- Release and test compilation passed. Default non-network regression: **2,382 passed,
  0 failed, 88 ignored, 75 filtered**, 273.73 seconds. This excludes the separately failing
  ignored, private-source Canyon260 placement case; a green default suite is not full admission.
- Source body-yaw checks passed seven cases, including the retained patient and camp corridor;
  loading sweep checks passed six. The continuous medical recording nevertheless failed beside
  the medical bed after successful field care, loading, fuel-consuming travel and unloading.
  The runtime used the small position-servo error as its turning direction, suppressing yaw
  during slow dragging until the body met the spare stretcher. The automated helper also could
  advance a waypoint while the patient remained blocked. Both require correction and a new run.
- The current actual canyon receipt is **142 admitted, 122 unresolved originals, 109 preserved
  assembly components**, plus **43 explicitly preserved generator-owned embedded formations**.
  The latter are intentionally buried rock masses, not loose-stone support admissions. The
  exact receipt is `artifacts/expedition-20261030/ground_canyon_b52_a.json`; its overview still
  centers unresolved PzIII index260 and is not a completed grounding demonstration.
- The test-only build53 whole-face neighborhood scan found nine certified same-XZ orientations
  near index260. The smallest measured height increase is about 0.195 mm, with a 224.93 mm
  support margin. This establishes local placement feasibility, not a downhill or swept path.
  A bounded initial-placement correction is being evaluated; no support threshold was relaxed.

Focused loading sweeps, real-source stretcher admission/withdrawal, both winch routes, exact support BVH, terrain bounds, winter bridge support, and passenger owner tests passed in build46. Its full medical recording failed before patient handling because its helper cut through a parked winch on approach. Build49 clears that obstruction and completes the patient journey through unloading, but the final tent-entry route still needs its corrected recording. The detached-turret settling and translated-slope cases subsequently passed in b48 and the full b49 suite.

The private Tiger test retains all 1,796,654 source vertices and matches the exhaustive result. The b47 CPU-test settling step measured 85.716 to 0.161 ms on flat ground, 86.070 to 0.476 ms on the uneven grid, and 271.677 to 1.679 ms with detailed terrain and a crater. These isolated measurements include the exact terrain-bounds cache and justify the acceleration; they are not game FPS.

The actual build46 passenger succession review passes five checks, including all eleven owner payloads after cold reconstruction and unchanged survivor wounds/ammunition at control transfer. Staged losses and diagnostic exterior cameras are explicitly recorded. The actual build46 gunner image verifies the deeper-orange, ten-percent-lower-power dial illuminator. Both are in the existing private review page.

A further save audit found physical detailed-terrain crater/rut events absent from the saved owners, even though the renderer retained its visual rut atlas. Matching the existing eleven payloads alone did not cover that gap. The versioned physical terrain owner is now implemented, including exact sparse rut pages, craters, walker cadence and consumed presentation flags. Its state tests, exact GPU cold-reconstruction comparisons and strict CPU/GPU parity checks now pass in b49; no parity tolerance was widened.

The preferred Sketchfab SDKFZ 251 by Applepie68905 has **not been acquired or integrated**. Official download attempts returned no verified local file or path; object separation, pivots, materials and interior remain uninspected. Attempts were stopped, with the blocker recorded in `artifacts/halftrack251-20261031/ACQUISITION_CHECKPOINT.md`. The game still uses its existing generic transport prototype.

Continuous whole-body/held-cartridge clearance, wrapped finger grasps and planted turning feet are not complete. The old corpse-disappearance report has not been reproduced in the current actor-owned pages; no speculative corpse lifetime fix was made. Assisted casualty handling passes the complete build55 medical journey, with the bed pose and pacing limitations described above. Static wreck and loose-prop source tests now pass, but unresolved real-map placements and the unstable authored sledge retain their original poses; they are not reported as physically settled. Tree and turret support retain their documented source sampling and numerical contact limits. None of these automated recordings substitutes for a longer independent human playtest.
