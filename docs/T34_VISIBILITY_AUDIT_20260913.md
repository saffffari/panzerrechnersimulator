# T-34 visibility and Playtest retaliation audit — 2026-09-13

**Status: source and existing-log audit; corrections proposed, not implemented.** No gameplay, showcase fixture, build, or GPU changes were made for this audit.

Inspected the working source in `/home/alex/panzer/shader-graphics`, branch `codex/shader-graphics-rebuild`, HEAD `11744b7c07031756e0620c187a61f31bae3e6e5d`. The checkout has ongoing changes, so the line references below identify the inspected working files rather than asserting that HEAD alone contains them.

## Finding

The reported unfair encounter is strongly supported by implementation defects. Initial detection ignores the fog and snow that obscure the player's view. After acquisition, a tank or its alerted platoon tracks the player's exact current position indefinitely, and the main-gun firing gate does not recheck visibility. Six independently firing T-34s and deliberate pincer movement amplify those defects. Their number, cadence, and orientation are balance choices; the perception and contact-memory behavior needs correction before balancing difficulty.

## Current rules and source evidence

| Area | Implemented behavior | Working-source evidence |
| --- | --- | --- |
| Detection range | Stationary daytime Tiger: 450 m; moving: up to 800 m. Night multiplies the radius by 0.35. | `src/stealth.rs:19-21,242-244` |
| Detection modifiers | Open cupola radius ×1.18. Engine off radius ×0.42 and rate ×0.55. Engine restart radius and rate ×1.7 for a five-second noise interval. Movement changes range and acquisition rate. | `src/game.rs:13793,22883-22903` |
| Viewing direction | Uses the better of hull and turret facing. Front ±50° has full weight; side to 120° has weight 0.3; rear has weight 0.1. Rear is weak, not completely blind. | `src/stealth.rs:26-30,260-288`; `src/game.rs:22919-22927` |
| Acquisition time | A meter accumulates to 1. At default skill, engine running, stationary, closed hatch, daytime, facing the enemy and with clear geometric LOS, the 340–420 m range gives approximately 2.7–3.5 s to acquire. This is a calculation from the rate, not a measured session timeline. Movement or engine restart can make it faster. | `src/stealth.rs:33-40,275-288`; `src/game.rs:22931-22932` |
| Geometric LOS | Initial acquisition checks cover AABBs, operable doors, terrain and standing tree trunks. It uses a center-to-center sightline rather than a visibility sample through each modeled optic. Grass, smoke, fog and precipitation do not enter this check. | `src/game.rs:22928-22938,22947-22961` |
| Winter visibility | Act I's renderer uses storm 0.98 and ground extinction 0.0068/m, with winter precipitation. The AI's detection inputs contain no corresponding weather visibility term. Thus its radius does not shrink with the whiteout that the player sees. | `src/renderer.rs:633-650,5184-5188`; `src/game.rs:22883-22938` |
| Individual target memory | The acquisition meter decays when unseen, but `spotted` stays true. Clearing requires special platoon transitions, not ordinary LOS/range loss. A spotted tank is immediately in Track mode. | `src/stealth.rs:139-158,181-183,203-209`; `src/game.rs:22852-22859` |
| Platoon sharing | Groups of three. The first spotter surviving five seconds changes its platoon to Engaged. Killing it before that resets that platoon's meters and puts it into Searching. Other individually acquired tanks and the other platoon are separate. The five seconds is a sharing delay, not a guaranteed reaction delay before the first tank fires. | `src/stealth.rs:43-45,145-201` |
| Lost contact | Engaged refreshes `last_known` from the actual player position every tick. It has no transition back to Searching on lost contact. Track aims at the actual current threat, not a stale observation. | `src/stealth.rs:214-217`; `src/game.rs:22817-22823` |
| Shot permission | Track fires when laid, reloaded, allowed by the optional controller, and return fire is enabled. There is no current LOS, perception range, weather visibility, or fresh reaction/dwell gate. | `src/game.rs:22824-22830,23168-23197` |
| Physical ballistics | Enemy shots are real swept projectiles. Doors, armor, hard cover, structures, trees and terrain can intercept them. The AI aiming through obscuration does **not** establish that the projectile ignores physical walls. | `src/game.rs:21107-21209,23190-23197` |
| Shot reveal | The normal player main-gun discharge creates its projectile, effects, sound and campaign pressure. It does not directly alert all T-34s or write their detection meters. Engine-start noise is a separate explicit modifier. | `src/game.rs:21839-21872,13793,22890` |

## Playtest deployment and retaliation

`opening_enemy_swarm` supplies six T-34s. Preferred positions are two rows at offsets `(−110, −340)`, `(0, −340)`, `(110, −340)` and `(−110, −405)`, `(0, −405)`, `(110, −405)` from the player. Their ranges are 340–420 m and preferred lateral bearings are approximately ±15–18° from the player's initial forward direction. All are set to face the player. See `src/game.rs:1452-1465,1483-1508,22205-22234`.

When a preferred position is blocked, the bounded placement search can search the full circle. Clearance checks structure overlap and terrain slope, not player-to-target sightline or fog visibility (`src/game.rs:1490-1507,22210-22219`). Therefore the initial engagement does not guarantee that all active threats are reasonably visible to the player.

Opening reloads are 8, 9.2, 10.4, 11.6, 12.8 and 14 seconds. Turning return fire on extends those timers, but does not clear old target knowledge. Subsequent default-skill reload is about 5.4 seconds (`src/game.rs:415,1473-1475,1513-1525,22828-22830`). There is no separate post-acquisition reaction delay. The turret slews at 0.80 rad/s, approximately 46°/s; the nearby comment saying approximately 23°/s is stale (`src/tank.rs:18-22,786-801`).

Tracking enemies intentionally alternate left and right pincer movement, then close on the Tiger's side/rear. Their maneuver uses the player's actual current position and hull orientation (`src/game.rs:23018-23062`). That explains a flanker arriving while the player is occupied with the first target; indefinite contact knowledge makes this materially less fair in poor visibility.

## Existing runtime evidence and limits

Inspected available launcher logs under `artifacts/runtime/` and searched `artifacts/` and `captures/` files ending in `.log` for the placement-failure message.

The run `artifacts/runtime/shader-playtest-20260913-125729-287120.log` records:

1. All six targets placed, authored spawn `(1850, 250)` (line 86).
2. T-34 #0 spots the player, then platoon 0 becomes Engaged (lines 100–101).
3. Player destroys #0 at 199 m (lines 102–103).
4. T-34 #1 penetrates the Tiger's hull side at 92.5 m; subsequent ammunition detonation destroys the Tiger (lines 109–119).

This is direct evidence of the first-target kill followed by a different member of its engaged platoon delivering the lethal side hit. The log does not record frame visibility or event timestamps, so it does not prove the flanker was visually invisible or quantify an “instant” response. It also references the earlier canonical Act-I manifest, not the newer winter revision now selected by the shader launcher (log line 87; `tools/launch_shader_playtest.sh:33`). It is supporting evidence, not a new reproduction on the current binary.

The other recent launcher run, `artifacts/runtime/shader-playtest-20260913-121342-222044.log`, also records all six targets at line 86 and T-34 #5 acquisition followed by platoon 1 engagement at lines 123–125.

### Conditional hidden pooled-tank defect — not reproduced

If placement fails, `src/game.rs:22225-22228` marks the enemy inactive but leaves its existing `alive` value unchanged. The normal AI loop at `22712-22715` and detection loop at `22911-22916` check `alive`, not `enemy_active`. Consequently an inactive, hidden enemy can remain eligible for acquisition and combat.

There were **no** `no clear practice position` matches in the searched logs, and the two recent launcher runs explicitly placed six targets. This path is a source-level defect, but the available evidence does not identify it as the cause of the player's encounter.

## Return-fire policy

Return fire OFF skips enemy detection and all AI orders, retaining target settling. `enemy_fire` also checks the flag independently (`src/game.rs:22192-22193,22679-22694,23169`). Already airborne projectiles are not removed by toggling the flag. Startup logs saying OFF describe startup state; they do not prove the setting remained off during a later engagement.

## Bounded correction order

1. **Exclude inactive enemies everywhere they can perceive, aim, maneuver or discharge.** Also clear liveness on failed placement or enforce a single active-combat predicate. This closes the conditional hidden-pool path without changing enemy difficulty.
2. **Create one bounded current-visibility result per active enemy.** Combine existing geometry LOS, range, observer direction and the same weather state used for rendering. Reuse it for acquisition and permission to take a fresh aimed shot. Check muzzle-to-target physical clearance where appropriate. Cache/reuse queries rather than adding repeated unrestricted LOS scans.
3. **Use observed information for contact memory.** Save the position only when visible; make contact decay after loss. An alerted teammate receives the observed location/general area, not a continuously refreshed exact fix. Transition Engaged to Searching on contact loss, and require renewed acquisition/reaction time before resuming aimed fire. Searching toward the last observation may continue.
4. **Verify basic perception before spawn and cadence tuning.** Test terrain/door cover, disappearance into the same winter fog seen by the player, reappearance, spotter death before/after sharing, inactive enemies, and return-fire OFF. A fresh shot must not target the player's new hidden position. A shot already in flight retains its normal physical behavior.
5. **Then adjust the encounter deliberately.** Use an authored initial patrol orientation rather than every crew already facing the player; constrain fallback deployment to an appropriate opening sector and visible lanes; consider fewer or staged live opponents. Tune reaction dwell, pincer speed and volley staggering after visibility is fair. These are balance options, not substitutes for fixing target knowledge.

No historical claim is made for the current detection coefficients, radio-sharing timer, traverse speed, spawn arrangement or proposed balance settings. No correction above has been implemented or playtested as part of this audit.
