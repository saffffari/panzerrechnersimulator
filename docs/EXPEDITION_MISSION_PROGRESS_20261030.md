# Expedition mission and terminal-state checkpoint â€” 2026-10-30

This records the implemented mission rules and the parent-run build31 evidence. The thirteen checks in `captures/expedition_missions_20261030_b31a.json` passed. The witness stages combat outcomes and travel explicitly; it is not a human playthrough. Later source changes are identified separately below.

## Terminal campaign state

The successful final operation now opens a results screen and pauses the existing world. It flushes the complete world save before reporting that the result was saved. Review shows the same frozen battlefield; returning to title and resuming the completed save keeps the clock stopped. No successor, new campaign, refill or further physiology tick is created by the completion screen.

## Relief departures

A camp successor records a persistent relief obligation with the successor and predecessor PersonIds and the actual camp origin. Crossing 65 metres spends one preparation opportunity exactly once. The obligation exists even when no order was active at death or the Tiger remains mechanically intact. A new order cannot erase it. Withdrawing an interrupted objective preserves an already dispatched relief journey. With no retained objective, remounting a capable Tiger after the recorded departure finishes recovery and permits orders again.

`CampRelief` records that actual camp journey; `FieldContinuation` records takeover by a surviving actor already on the battlefield without charging another departure. Recovery still works at zero preparation opportunities. Neither action invents another person or heals an actor. The mission owner saves the pending obligation and validates its references against the coordinator.

## Rescue failure

A dead finite rescue target closes its uncompleted rescue order with `Failed`, marks the site unavailable for that rescue and never offers that impossible objective again. A loss discovered between missions also removes the order. An active recovery dispatch survives that failure. A previously completed rescue cannot be repeated after the rescued person later dies or the settlement changes hands.

## Territory and support movement

Each authored site retains Hostile, Friendly or Contested ownership, a recapture clock, capture count and optional permanent order failure. Living, active, upright enemy tanks or living enemy infantry physically inside 125 metres contest a friendly site. Without a capable friendly defender within 150 metres, 45 seconds of pressure recaptures it. A defender arrests the clock; removing the attackers reduces the pressure. These durations are authored gameplay rules, not historical measurements.

A loss reopens the location for an ordinary retaking order, except a permanently failed or previously accomplished rescue. Its original enemy IDs, counterattack-release flag, fuel, shell carts, workshop components, wrecks and casualty identities remain. There is no replenishment or spawned replacement garrison. The frontier count derives from owned sites, and the monochrome map marks boundaries along the authored road network. This is a finite site-control model, not a strategic simulation of unseen armies.

If the camp's current sector is lost, the support detail requests physical relocation to the nearest retained friendly site, with the original camp site available as fallback. Existing terrain/source-contact admission and offscreen relocation rules still govern whether it can move. Occupied medical beds defer relocation. The coordinator permits a retained unsafe camp while this evacuation is pending; it still rejects relocating into a hostile destination. Camp relocation is represented as an offscreen move of the same finite people, stocks, geometry and parked transport, not a drivable convoy simulation.

## Map presentation

The physical sheet keeps its original UV and click zones. Header, site title, detail line and controls use larger type; the main title is shortened to fit. Current notices now appear on the actual paper, including failure reasons and delayed recovery/relocation. Building outlines and road strokes are thicker. The long horizontal mark in the prior Hagen area capture is the real road toward Kessel, not an oversized building footprint. Build31 renders the physical map and preserves its actual ink across paused restore. Fine typography remains hard to judge in the 480p preview; a full-resolution reading and usability pass is still needed.

## Verification entry points

- CPU filter: `expedition_theatre_game::tests`.
- CPU filter: `expedition::tests::field_survivor_continues_interrupted_dispatch_without_a_camp_spend`.
- `Game::run_expedition_mission_review(prefix)` / CLI `expedition-mission-review <prefix>`.
- Repeat the complete world-owner roundtrip review after owner layout changes.

The mission review constructs the actual Game and invokes ordinary order/territory/relocation rules. It explicitly stages travel positions, the existing defenders' deaths, one finite rescue casualty and a surviving invader's position. It exercises acceptance, duplicate rejection, holds, withdrawal, impossible rescue closure, support advance, territorial loss, support retreat, retaking with unchanged finite stores, and a zero-budget finale. Only mission/coordinator time advances during those fixtures. It does not claim ballistic accuracy, successful human combat, driving, whole-world time-course simulation or a complete human playthrough.

The new binary owner versions are `world.progression=3` and `transport.recovery=3`. Old development snapshots are rejected by strict source/build fingerprints and owner versions; no migration or missing-state default is implied.

## Remaining practical limits

- Front boundaries are cartographic annotations, not invisible blockers or newly spawned enemy formations.
- Recapture only occurs if a surviving physical enemy actually reaches a secured location. Current patrols may not naturally produce many such cases; the witness deliberately stages that pressure.
- Failure or withdrawal does not conjure an enemy victory in an unrelated sector. Existing enemy pressure and actual ownership determine camp retreat.
- Finale completion currently permits a living player on foot to secure the final area; it does not require the Tiger to be parked within the objective radius.
- The witness's physical camp relocation may legitimately fail if the source terrain cannot admit a safe destination. That is reported rather than silently moving the camp to an invalid pad.

## Follow-up after build30

The parent reported all nine then-current mission CPU tests passed. The next source checkpoint adds three protections found in review: an enemy arriving from a different site prevents occupation, an unconscious controlled actor cannot advance the hold, and contested or physically threatened sites cannot receive the support detail. The witness first asserts that a player beside camp prevents its offscreen move, then uses a deliberately staged clear position for the relocation check. It now retains earlier checks if a later step returns an error. A tenth CPU case covers rejecting contested/threatened retreat destinations. These follow-up source changes still require the parent's next compilation/run.

## Verified build31 and current acceptance boundary

The parent ran `expedition-mission-review expedition_missions_20261030_b31a`: all thirteen recorded checks passed, including unconsciousness blocking occupation, permanently failed rescue, near-player relocation deferral, physical camp advance and retreat, a surviving enemy's recapture, a foreign invader blocking retaking, unchanged finite stores on retake, and victory at zero remaining preparation slots. The parent also reported all 47 expedition CPU cases passed in build31. The build31 world witness retained the mission owner exactly across cold restore and ordinary continuation.

The implemented theatre is **12 km square with twelve authored operation sites and two surveyed river crossings**. It uses one current source seed (1944), one source geography/road network and explicit finite site ownership; selecting a different coordinator run seed does not generate a new mission graph or terrain. The earlier 32.768 km streaming proposal remains a roadmap item. Mission order and outcomes can vary; this is not yet evidence of many independently generated campaigns.

The campaign has an actual completion state and persistent relief consequences. Its remaining acceptance work includes a continuous human mission sequence, deliberate evacuation through transport and care, ordinary replacement boarding, and repeated failure/recovery under hostile pressure. The mission witness does not substitute for those loops. Large engine, transmission and turret overhauls remain beyond the demonstrated optical/track/work-lamp service operations.

## Source follow-up after build32

The fresh-run entry now starts the same controlled person on foot at the installed support camp before any clock advance, facing the physical orders location. A new world-review assertion checks the actor bindings, fuel/ammunition, dispatch budget and retained Tiger position before the first tick. Resume never invokes this initializer.

The previous starting Tiger at (-4200,2400) stood only 825 metres from Hagen's authored origin; its near-side guard was visible to the scripted gunner at about 700 metres. A 97-second build31 armed review stayed almost stationary because its test driver stopped whenever the gunner had a target. That is a test-driver policy, not evidence that cruise cannot move the vehicle. The commander died through the ordinary fatal-casualty record; the last armor log was a stopped frontal cannon strike, but the old receipt omitted the separate projectile-contact ledgers needed to establish which contact killed him. No conclusion that a bounce itself killed him, or that he was exposed, is supported by that receipt.

The camp and matching first road endpoint have been moved to (-5250,3200) in the source and private terrain rebaked. This creates physical preparation distance instead of suspending enemies or granting protection. The next actual Game review must still verify initial camera, contact/visibility and departure from this new location. Its generated receipt reports 105 buildings, 11,295 placed props/trees, the same twelve sites and two crossings. Those counts are generation output, not visual acceptance of every object.
