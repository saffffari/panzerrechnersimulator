# Physical Pizza doors — source implementation, awaiting executable proof

This change supplies actual hinged door leaves to the recognized manifest doorway modules and the featured functional house. The v13 and v14 coordinated unit logs pass the door geometry/behavior tests. First-person appearance and ordinary interaction footage remain pending; the coordinating agent owns recording.

## Source geometry

- Leaf: `PSX Mega Pack II v1.8/…/Doors & Gates/door_hr_14.glb`, with original texture and source bounds x −1…0 m, y 0…2.1 m, z 0…0.05 m. Its x=0 vertical edge remains the hinge.
- `doorway_hr_1.glb` has a 1.6×2.1 m aperture **and an embedded `metal_door_1` child**, translated +0.5 m in x. The ordinary loader previously included that static child. The live manifest path now detaches that exact child subtree before installing the moving leaves; jambs, lintel and their materials remain.
- `doorway_hr_2_wide.glb` has a 2.2×2.5 m opening with no embedded leaf. The three featured-house doors use its compiler placement transforms, including the house's existing vertical offset.
- Two leaves fill each aperture, retaining a small center seam and floor/lintel clearance. Hinges mount at local z=0.26 m: beyond the source trim's 0.21 m front face and the featured jamb proxy's 0.23 m extent. The fixed 110° outward swing clears that face throughout its travel. Approaching from the other side does not reverse the hinge through the frame.
- The older `assets/worlds/astra-20260905/playtest/manifest.json` contains 19 recognized frames, all scale 1, with yaw 0° or 90°. This is an audited manifest, not a claim that it is the currently selected main-menu revision. Runtime collection follows the manifest actually loaded.

## Live behavior

`world_doors.rs` allocates a physical moving-RT pool for each pair. Each pair keeps two reserved object IDs; removing a destroyed frame empties that pair without reassigning the IDs of other doors. The source mesh therefore participates in the normal scene's ray transport and shadows.

The same model matrices supply oriented leaf-box interaction, line-of-sight and projectile contact, and swept soldier collision. E and controller A/Cross use `Game::interact_world_door`; the capture action `door` calls that same method using the current first-person eye and aim. Reach is 2.2 m, and intervening live world geometry blocks interaction. A held can does not steal the door interaction when the crosshair rests on a reachable door.

Movement is reversible at 2.4 rad/s. Angular substeps of at most 0.02 rad stop the leaf before entering the player's or another living infantry member's standing cylinder. The prompt requests stepping back when obstructed. The door resumes its requested movement once clear. A nearby soldier can therefore obstruct opening as well as closing.

Both mixed-combat factions use live door LOS and final foot collision. Hitscan range is shortened by the current leaf; projectiles contact it as existing hard cover. The ordinary Soviet doctrine and learned observer additionally receive the exact door-occlusion result, preventing target acquisition through a closed leaf.

Featured-house module failure removes its door's rendering and collision. Compact building snapshot version 2 stores canonical open/closed/locked/destroyed states. Version 1 imports without door data preserve the authored defaults; malformed counts and states are rejected transactionally. The LAN building codec reads both versions. A partially moving door saves as the existing coarse open/closed state, not a continuous hinge animation.

Capture telemetry includes `world_doors.pairs`, `live_leaves`, `open_pairs`, `moving_pairs`, `blocked_pairs`, `locked_pairs`, `blocked_apertures` and `physical_moving_rt`. A successful interaction alone is not sufficient proof: the next first-person clip must visibly move the leaf, change aperture blockage, permit walking through, and close again.

## Validation authored for the next coordinated gate

Ten new tests cover source hinge invariance, the full outward sweep's frame clearance, actual GLB child partition and material retention, matching walk/LOS changes on open and close, closing obstruction and recovery, rotated/scaled OBBs, featured placement and save round-trip/legacy fallback/invalid import, destroyed frames, infantry perception gating, and legacy/new LAN field boundaries. The coordinated `v13-unit-tests-fixed.log` and `v14-unit-tests-attempt3.log` show all six direct `world_doors::tests` passing, including the real source-child partition, as well as the integrated gate result reported by root. This does not approve appearance.

## Explicit limits

- Visual fit, texture orientation and ordinary first-person interaction still need the next executable video; source geometry and unit tests cannot approve the appearance.
- Contact uses the source leaf's oriented bounding box, not per-triangle collision or a calibrated wood/metal penetration model. A door currently receives the game's existing hard-cover shot treatment; it has no independent breakable-leaf damage state or tank-ram response.
- Featured doors join the existing compact building snapshot. Independent manifest-door state remains session-local. Network door-button authority for a remote client is not added by this bounded change.
- NPCs respect doors as obstacles; they do not yet operate the handle or plan a route that opens a door. Ordinary first-person movement and E interaction remain authoritative.
- This implementation changes recognized doorways only. Boarded windows and unrelated module choices remain separate world-authoring work.

## Current world review recipes

The selected corrected Playtest revision uses nineteen wide `doorway_hr_2_wide` frames and preserves all placement IDs. `fp_garage_source_doors_v15`, `fp_warehouse_source_doors_v15` and `fp_featured_source_door_stairs_v15` are now prepared under `assets/review/first_person/`. The capture validator previously omitted `door` from its allowlist despite implementing the action; that omission is fixed in the next source revision. New flat telemetry aliases permit assertions on actual open pairs, moving pairs, blocked apertures and physical moving-RT registration, plus standing eye height. See `ASTRA_GROUND_HEADROOM_20260905.md` for exact proof scope and remaining runtime gate.
