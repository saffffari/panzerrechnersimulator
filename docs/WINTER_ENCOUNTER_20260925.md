# Winter courtyard encounter — 25 September 2026

Launch the current authored encounter:

```sh
/home/alex/panzer/shader-graphics/tools/launch_winter_session.sh --singleplayer
/home/alex/panzer/shader-graphics/tools/launch_winter_session.sh --coop
```

Without a mode flag this launcher opens the title menu with Singleplayer and Co-op buttons. The winter launcher selects the local encounter; normal campaign/checkpoint entry outside it remains separate. Co-op uses keyboard/mouse plus one controller, with two horizontal views (top/bottom).

**Esc → Restart Game** rebuilds the current map and resets the encounter in the current mode. **Esc → Main Menu** ends that session and stops its gameplay audio. The title menu then offers Singleplayer and Co-op. Seven scoped pause-menu/campaign-restart tests passed; these two new lifecycle actions still need native end-to-end review.

## Field order

An approximately ten-minute, untimed exploration/combat exercise. Actual duration depends on navigation and the enemy patrols. Press **M** to unfold the local field sketch and read the current order; **+ / −** or the wheel magnifies the physical sheet. The initial order fades after fourteen simulated seconds. There is no persistent enemy marker or timed enemy-spawn trigger.

1. Leave the stationary Tiger and search the ruined house north of the start. Spend at least four seconds inside the central rooms.
2. At the burnt Tiger east/southeast of the house, look at an exposed **88 mm AP** cartridge and hold **LMB**. Carry it home. Release it while aiming at a compatible empty physical rack cradle in the turret cabin. Two AP cradles start empty; recover two distinct cartridges. Setting one down and picking it up again does not create another cartridge or another objective credit.
3. Observe the northern approaches. Work the gun from the Gunner station, fire, clear the hot case at the Loader station, take a fresh cartridge, hold it on the tray, deliberately flick the mouse/stick to ram it, and close the breech. Fire the second round from the Gunner station.
4. Drive the Tiger to the withdrawal point south of the start. The field order then marks completion and leaves free exploration available.

**Space** from the buttoned cupola descends into the turret cabin. With your head outside an open cupola, Space pulls you onto the roof. On foot, aim toward the vehicle and use separate Space presses for chassis, turret roof and an open hatch. Work the hatch with LMB; leave it open before returning with ammunition. **Shift** sprints, **Ctrl** crouches. The cupola has no main-gun trigger. **RMB** draws/aims the P08 while exposed; holding a cartridge takes priority over the pistol.

## Environment and history

The live sidecar has 873 operations: all 804 prior edits plus 69 props using 8,749 added triangles and two additional mesh asset paths. Interior belongings, localized ash, charred splinters, snow at openings, packing crates and displaced timber follow a clearance-checked route. Six existing practical lights remain; no new light count was added. Two quiet generated spatial one-shots use the existing obstruction/stereo sound path. They are explicitly generated cues, not archival recordings. The continuous weather bed still follows shelter, snow and visibility.

This courtyard is a fictional atmosphere study. The three separately documented historical workshops retain their distinct locations, dates and evidence boundaries. They were not moved together to imply historical adjacency.

## Validation record

Source-scene traversal, native input delivery, source-query parity, historical comparison and display-frame performance are separate kinds of evidence.

- Full release binary regression suite: **1,114 passed, 0 failed, 27 ignored**. Asset-root/host run: `artifacts/runtime/winter-encounter-all-tests-host-final-20260925.log`. Ignored GPU scenarios are checked separately, not counted as passing here.
- Native keyboard/mouse: `artifacts/runtime/native_keyboard_20260925_v2_review.json` records **12/12 checks** through winit and the ordinary Game loop. Commander primary did not fire the main gun; M opened/closed the physical map, plus/minus changed magnification, Space descended into Loader, and Shift/Ctrl reached the movement input. The movement checks were in the cabin, not an outdoor sprint-speed measurement. An earlier paused run is retained as incomplete evidence, not a success.
- Patrol perception: `captures/astra/winter_encounter_ai_20260925_v1.json`. Real source-scene simulation stayed quiet for 120 seconds at the starting tank and 120 seconds at the house. A real discharge prompted uncertain investigations; actual enemy fire waited for that enemy's own visual contact. These are deterministic scene fixtures, not a human stealth-difficulty assessment.
- Fire collision-query optimization: `artifacts/performance/winter-encounter-20260925/fire-source-query-parity-20260925.json`. The BVH query matched the original result on 3,744 source-mesh rays, including exact hit fractions and normals. No particles, surface patches, collision meshes or orange-light strength were removed to obtain the improvement.
- Armor: `docs/research/ARMOR_CALIBRATION_AUDIT_20260925.md`. Cover now reduces surviving kinetic-shell residual energy and the existing damage scalar consistently. Historical sources are labelled by eligibility; no unverified penetration or armor-quality constants were fitted. Roof/belly treatment, high-obliquity behavior and the empirical casualty model remain unvalidated.

- Continuous physical route: `captures/astra/winter_route_20260925_v4.json`. All six sections, actual AP pickup, held return, chassis/roof climbs, cupola entry, cabin descent and crouched movement around the breech passed. Ordinary grip release stowed the same cartridge in rack 11 and advanced the order to **1/2**. The test stages only the initial grounded pose/open hatch; fire is held and audio muted. Second-cradle usability and the complete two-round mission remain separate checks.
- Native top/bottom co-op: `captures/astra/native_coop_20260925_v3_split.png`. The ordinary gilrs backend recognized the temporary validation pad; remote Gunner RT produced a real discharge while keyboard player remained Commander with no local fire input. An independently connected PS5 Controller was detected during the run; its compatibility and ergonomics were not certified by the virtual-pad assertions.

At the user's request the current build is being launched for play. **Corrected sealed scene-frame performance runs remain pending** so profiling does not contend with that live game. Earlier `*-shipping.json`/unbounded v1 measurements in `artifacts/performance/winter-encounter-20260925/` are diagnostic only; `first-pass-limitations.json` explains their render-graph and contention issues. `run_sealed_v2.py` is prepared for sequential same-binary reference/BVH comparisons after closing the live session. No final FPS claim is made.

Virtual device tests do not certify physical stick calibration, Bluetooth latency or felt haptics. A complete human mission playthrough is still a separate usability check. Pause-menu source tests were added after the full 1,114-test suite; those seven scoped tests passed separately.
