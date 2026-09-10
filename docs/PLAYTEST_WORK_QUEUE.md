# Panzerrechnersystem playtest work queue

Updated 2026-09-09 UTC. This is the shared routing sheet for the current player session and the
campaign work continuing alongside it. A source change, a CPU test, a structural campaign check,
and an actual player observation are separate evidence. Keep the running session available while
background work proceeds; route any requested restart through the session owner.

## Now (in user playtest)

| ID | Work | State / owner | Next acceptance check |
|---|---|---|---|
| N1 | Fullscreen, window focus and usable menu/input transitions | Root owns the active window and `main.rs` / UI work. | Player can enter play, pause, change display mode and resume with working mouse capture. Record the exact running binary and launch log. |
| N2 | Respond to mechanical feedback from the current Playtest session | Root triages; game/mechanics owner reproduces a reported failure. | Record mode, station, action, expected result and observed result; add capture/time or a log when available. An ordinary T-34 kill of the player is not by itself a defect or authorization to rebalance enemies. |
| N3 | Make essential campaign controls discoverable before the first map recovery | Approved narrow text patch implemented in `Game::mission_hud`, `ui::draw_controls`, and `CONTROLS.md`; awaiting root build and visual check. | The first guarded-site hint now includes `M` for the map and `Home` for controls. The controls reference includes `R` to reload/search a map case. Check legibility and spacing; physical search and objective rules are unchanged. |

N3 evidence: before this patch, `Game::mission_hud` initially said **KARTENORT UNTER FEINDKONTROLLE**;
its first explicit `M` hint appeared after recovery. `ui::draw_held_map` already provides a `?` search area
before the first recovery. `Game::update_campaign_director` requires the crew to be on foot, within
2.4 m of the active cleared map case, and use the reload/search input; its local hover label says
**KARTENTASCHE DURCHSUCHEN [R]**. `ui::draw_controls` and the [controls document](CONTROLS.md)
previously omitted that search action. The patch closes a documentation and discoverability gap;
it does not claim to repair an impossible objective.

## Campaign (background)

| ID | Work | State / owner | Next acceptance check |
|---|---|---|---|
| C1 | Rerun structural Act I → II → finale verification | Root owns the GPU/executable gate; campaign owner owns progression. | Produce a fresh successful `campaign-verify` receipt from the rebuilt executable. The existing failed receipt is not a passing campaign check. |
| C2 | Review Act II bridge-crossing endpoint handling | Campaign owner identified a narrow crossing issue and owns its correction. | Test full-bank traversal, a step ending exactly on the centerline, and adjacent off-bridge motion; then include it in C1. Do not bypass the crossing objective. |
| C3 | Validate first-run navigation and physical access to map rooms | Source and route tests exist; player access across all authored candidates is unverified. | Observe a normal first map search, then sample the remaining route candidates for reachable cases, legible clues, remounting and a working exit after all required progress. |
| C4 | Reconcile current navigation documentation with the implemented route system | Documentation debt; this queue records the discrepancy without rewriting historical design. | Update the current-use section of `WORLD_AND_NAVIGATION.md`: routes are generated per attempt, three guarded sheets reveal the exit, and the map stays live. Preserve clearly marked historical design below. |
| C5 | Develop the locked Berlin/Reichstag campaign | Planned production work, not part of the current playability fix. | Replace the implemented Halbe finale through authored campaign work and its tests. Do not relabel the existing Halbe world/HUD as Berlin without implementing that world and progression. |

C1 detail: [the existing verification log](../artifacts/play-readiness-20260909/campaign-verify.log)
ends at `Blocked { act_id: 1, remaining: [Act1WitnessCityLayers] }`. The campaign owner confirmed
that this predates the correction to `Game::debug_complete_campaign_act`: the structural fixture
now performs the actual cleared map-case recovery path and emits its objective signals. That
fixture correction is in source; a new successful structural run is still required. It did not
remove a live campaign gate.

C4/C5 detail: [STATUS.md](STATUS.md) explicitly identifies the shipped three-sector Halbe
prototype and the unimplemented Reichstag direction. `WORLD_AND_NAVIGATION.md` still describes an
always-west exit visible as a red X from the start, while the live held map reveals the generated
exit only after three recoveries. Its old build-order entries also call already integrated work
"next." The current final-sector caption, HUD and endings consistently describe the earlier Halbe
scenario; that is known production scope, not a new naming bug.

## Verified

| Scope | Evidence and limits |
|---|---|
| Current product name | The title UI, launcher messages, README and product-name policy use **Panzerrechnersystem**. The executable is `panzerrechnersystem`. [PRODUCT_NAME.md](PRODUCT_NAME.md) retains intentional environment/schema/save/protocol compatibility names and the historical geographic word *Kessel*. This audit did not alter archival receipts or asset paths. |
| Authored sector identities | The three active `assets/worldgen/actN/manifest.v1.json` titles are **Die Stadt**, **Die Heimkehr**, and **Kameraden**, matching compiled `world::act` labels. The manifests carry Spawn/Goal markers and recognized navigation landmarks; the manifest and terrain tests pass in the receipt below. This does not prove every physical route is traversable. |
| Latest inspected CPU suite | [post-reboot-unit-tests.log](../artifacts/play-readiness-20260909/post-reboot-unit-tests.log): **807 passed, 0 failed, 5 ignored**. This is the recorded binary-unit-test run, not a fresh human campaign completion or a claim about every later edit. |
| Campaign coordinator and disk checkpoints | That CPU receipt includes required-objective gate refusal, same-Tiger/stores carry, discovery/retry behavior, and save tests for entrance restoration, the same numbered attempt, later-sector/finale reload, invalid-data rejection and atomic disk replacement. [STATUS.md](STATUS.md) describes the live contract: one manual checkpoint, sector-entrance restoration, no autosave, no Playtest save, and no restoration of current position/combat/map annotations. |
| Stationary loader sparkle comparison | The paired 72-frame captures [before](../captures/astra/readiness_loader_before_20260909.mp4) and [gameplay](../captures/astra/readiness_loader_gameplay_20260909.mp4) use identical camera/G-buffer geometry. In the selected optic region, mean frame-to-frame maximum RGB-channel change falls from 0.938 to 0.005 display levels; all 27,753 optical pixels have identical raw current lighting between the two inspected gameplay frames. Severe changes over 25/255 in the selected steel and green regions fall about 98% and 95%, respectively. These are fixed-camera, three-second measurements, not a moving-camera or whole-campaign guarantee. |
| Visual tradeoffs remain explicit | The compared gameplay image has about 3% lower mean opaque display luminance; the already bright optic slit gains about 39% linear radiance, largely compressed by display response. Fine diffuse/dither variation remains. Gameplay lighting is a deliberate approximation; the comparison does not establish energy equivalence with Full transport. |

## Needs user feedback

Collect these from ordinary play rather than stopping the session for a questionnaire:

- Whether the player can discover the first search area, dismount, identify the map case and
  recover it without outside instructions. Record the first confusing step.
- Whether moving, turning and changing stations expose distracting sparkle, poor visibility or
  motion trails after the stationary lighting correction.
- Whether firing, cycling the breech, loading, repairing and recovering from damage communicate
  their state clearly. Distinguish an unavailable action, a missed interaction target and an
  actual mechanism failure before changing behavior.
- Whether pause/load wording makes the sector-entrance checkpoint limitation clear enough, and
  whether return to play restores control reliably.

For each actionable report, append its exact observation to N2 or C3 with the running build/log,
then record the smallest reproduction and an explicit acceptance result. Keep difficulty changes
and the future campaign rewrite separate from repairs to reported malfunctioning mechanics.
