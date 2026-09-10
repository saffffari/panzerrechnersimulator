# Source-surface fire and persistent foam

Status: the v10.2 first-person sweep extinguished the fire and left persistent surface coating; 16 of 17 capture checks passed. Its remaining failure was stale ammunition-exposure telemetry after extinction. The v10.3 cleanup fix passed the coordinated CPU gate (701 unit tests passed, one ignored; all three live-FX contract tests passed). **Final v10.3 video verification is pending.** These results do not establish art approval, calibrated thermal behavior, or gameplay performance.

## Behavior

`src/surface_fire.rs` receives the same CPU Tiger meshes submitted to the renderer. Triangles are indexed into 20 cm spatial bins in eight articulated frames. Flying foam sweeps its motion segment against those triangles; deposits stay in the contacted frame and follow its movement. The retired fixed floor and cabin cylinder are gone.

The spray emits 2,400 8 mm matte cubes per second. Up to 4,096 flying cubes and 65,536 deposits are retained. Deposits persist until the tank state resets; existing deposits are preserved when the pool fills. The same source surface cell stacks up to four cubes, without counting repeated layers as new covered area. This uses source-surface columns, not pairwise rigid-body interactions between every grain.

Four fire patches are projected onto actual source surfaces near the existing ignition proposals. Patches grow, heat neighboring patches, and are suppressed by deposited coverage within their local 12 cm footprint. Holding F and spraying elsewhere does not reduce the fire. The nozzle follows the actual view direction; the previous automatic aim at the fire was removed.

The authored exposure calculation uses the positions of current live ammunition. It feeds the existing cook-off delay and ammunition detonation path. Emptying the ammunition inventory removes that modeled exposure risk while allowing the fire itself to continue. The diagnostic named `ammunition_heat` is a normalized gameplay exposure value, **not an ammunition temperature**, energy measurement, or calibrated thermal mass. Its inactive-fire contract clears that exposure; it does not simulate hot ammunition physically cooling to ambient.

The visual flame budget is 6,144 cells per interior patch, using 8–10 mm cubes. The palette has deep red bases and brighter orange cores. The aggregate light is derived from the visible cells' area and linear RGB radiance; it has no separate fire-strength multiplier. Brightness and connected glow still require visual review against the user's cabin-lamp reference.

## Evidence and limits

The original four CPU tests exercise swept triangle contact, persistent deposits following an articulated frame, bounded stacking without false coverage, wrong-aim versus covered-fire suppression, and full versus empty ammunition exposure risk. The additional v10.3 regression warms exposure with live ammunition, suppresses the fire through actual deposited coverage, then verifies that the next inactive update clears exposure while retaining the coating. The live-FX integration contract also rejects an inactive return that bypasses cleanup. These are causal unit tests, not first-person gameplay proof.

The heat, spread, and coverage coefficients are authored gameplay values. This is not a calibrated combustion, fuel, oxygen, pressure, or fluid model. Surface material flammability is not inferred from textures. Coating is represented by local occupied columns; the grain shape does not deform. Crew bodies and loose moving props are not yet included in the collision pages. The bounded maximum deposit count is not a modeled extinguisher charge.

The diagnostic hook `Game::debug_surface_fire()` returns persistent deposit count, coverage at the first patch, and ammunition heat. `debug_foam_counts()` returns all visible foam and airborne counts.

## Recorded first-person sweep: v10.2

The ten-second [video with gameplay audio](../captures/astra/loader_fire_glow_sweep_v10_2_20260905.mp4) and [manifest](../captures/astra/loader_fire_glow_sweep_v10_2_20260905_manifest.json) use the loader's ordinary in-game camera, rendered at 640×360 and encoded at 1920×1080, 24 fps. Audio comes from the ordinary gameplay sinks on the simulation clock. The [sweep scenario](../assets/review/first_person/fp_loader_surface_fire_foam_sweep_v10_20260905.json) starts one disclosed fixture fire, holds ordinary F from 2–7 seconds, and applies fixed mouse input across five rows from 2.65–5.2 seconds. There is no runtime autoaim or direct deposit, suppression, inventory, or heat manipulation after setup.

| Time | Fire strength | Persistent deposits | First-patch coverage | Airborne foam |
| --- | ---: | ---: | ---: | ---: |
| 3 s | 0.8681 | 420 | 18.67% | 705 |
| 4 s | 0.4486 | 2,388 | 78.52% | 676 |
| 5 s | 0 | 3,978 | 96.77% | 624 |
| 6 s | 0 | 4,683 | 100% | 718 |
| 7 s | 0 | 4,691 | 100% | 718 |
| 8 s through final state | 0 | 4,691 | 100% | 0 |

The fire is extinguished by the 5-second sample. The deposits and coverage persist after the trigger is released and flying grains expire. Coverage is the first patch's occupied area, not the percentage of the entire cabin coated.

The preceding stationary v10.1 recording reached only 393 deposits and 10.75% first-patch coverage, equivalent to 76 unique coverage cells. Stacking and repeated hits in that narrow footprint did not count as extra area, and the fire remained active. The sweep retained all of that attempt's acceptance thresholds.

The v10.2 sweep passed 16 of 17 checks. Its sole failure was `ammunition_heat <= 0.01`: the value remained at 0.2183625 from extinction through the final sample. `Game::update_fire` returned immediately when fire became inactive, bypassing the surface-fire module's existing inactive cleanup. That stale value was not evidence that the extinguished patch was still emitting heat.

## v10.3 cleanup and pending proof

`Game::update_fire` now calls the surface-fire module's inactive branch before returning. It clears the authored exposure value and cook-off timer, without resetting the module or removing persistent foam. The flame, glow, aim, coverage, and suppression thresholds were not changed by this cleanup.

The [v10.3 unit gate](../artifacts/astra-audit-20260905/v10_3-unit-tests.log) passed 701 tests with one ignored; the [live-FX contract gate](../artifacts/astra-audit-20260905/v10_3-live-fx-tests.log) passed all three tests. The final first-person sweep must still be recorded against the v10.3 executable and pass **all 17 unchanged checks**, including final ammunition exposure at or below 0.01. Do not label v10.2 as an all-checks pass or treat the CPU cleanup proof as the final v10.3 media result.

## Remaining first-person checks

- Run the final v10.3 sweep above and inspect the actual flame shape, connected glow, surface coating, and held-nozzle alignment. Successful state checks do not establish that the user approves the appearance.
- Record wrong-aim behavior: turn away and spray; deposits must persist elsewhere while the fire continues. `assets/review/first_person/fp_extinguisher_wrong_aim.json` remains an older scenario requiring preflight correction of its fractional expectation time before use; it is not a completed v10 runtime proof.
- Inspect the held nozzle for contact with the actual cabin wall and for visible alignment with the camera direction. Source inspection alone cannot establish that pose.
- Recheck cook-off timing with live and empty racks. New heat propagation may invalidate an older recording's assertion times; do not force a detonation merely to preserve an old clip's timing.
- Measure startup geometry indexing and active-spray frame time before increasing budgets further.

Use the executable identified by each recording. Do not present old v8/v9 footage as proof of these v10 mechanics, or the v10.2 sweep as verification of the later cleanup.


### Completed v10.3 video proof

[Actual first-person video with game audio](../captures/astra/loader_fire_glow_sweep_v10_3_20260905.mp4)
passes all **17 of 17** declared outcome checks. At 5 seconds fire and authored ammunition
exposure are both zero. Final coating has 4,691 deposits / 100 percent source coverage,
zero airborne foam after release, and the ammunition rack and tank remain intact.
Six decoded frames confirm the orange core / red base and shell, connected glow, aimed sweep,
extinction and retained surface coating. Visual mechanic coverage passes; sound quality and
final art approval remain pending. Existing sky/transport noise and dark driver lighting are
not resolved by this fire pass. Output: 10 seconds, 1920x1080 nearest upscale from 640x360,
24 fps with stereo 44.1 kHz AAC gameplay audio. All supervised GPU jobs have finished.

Exact MP4 SHA-256: 0e27e66ae9f037bb1352798832ce452a90cf87e9324eeed0e067af5378fc2764
Capture receipt SHA-256: 2535b3fddc94b5333c98194a2f79ae1b0677572b3b462f952c1e6e08ba7d7059
Frozen-binary provenance: ../artifacts/astra-first-person-20260905_feedback_v10_3/queue.json
