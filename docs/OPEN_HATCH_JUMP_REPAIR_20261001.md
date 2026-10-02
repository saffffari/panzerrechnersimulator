# Open-hatch falling and ordinary jumping

The on-foot controller used a one-frame feet crossing test against a horizontal hatch plane. The cupola rim is higher than the adjacent turret roof. A body walking into that opening could already have its feet below the rim before the fall began, so the mode never changed from `Foot` to the interior controls.

Natural roof entry now runs after accepted source-geometry collision. Feet must be below and within the fully open aperture, and an exact source-triangle segment from those feet back through that opening must be clear. A 1 mm contact skin avoids re-hitting the support under the feet. Closed lids, neighboring roof and the hull underbelly are not bypassed. No invisible roof or new collision volume is added. This also recognizes an already-stranded body under a still-open, unobstructed hatch.

A completed physical crossing cannot be invalidated by tank speed or by a different crewman occupying that hatch's usual post. The same entry handoff chooses a vacant interior post. The five-person roster leaves a post available while the local body is dismounted; a corrupt fully occupied mask still fails rather than taking someone else's post. Intentional Space entry retains the stationary, open, reachable and vacant-post gates.

Outside, Space jumps normally. Only explicitly aiming at a reachable, fully open hatch selects deliberate hatch entry. Merely standing near the chassis, below a closed lid, or over an opening while looking elsewhere no longer substitutes a scripted multi-deck climb. Interior Space behavior and LMB operation of actual lids remain unchanged.

Validation: `open_roof_entry_recovers_below_cupola_rim_only_through_actual_clear_source_aperture` passed against the loaded Tiger hull, turret and articulated lids. It covers a translated and pitched hull with three turret angles, the old below-rim failure, ordinary closed-lid descent, open descent, adjacent roof rejection and underbelly rejection. Native gameplay review is a separate pending check; this document does not claim a live playtest result.


The final OLD CPU checkpoint passed all 26 foot and source-hatch checks (21 foot-controller tests and five source/entry filters). The ordinary source route lands at 1.378 m on the track/fender, 2.052 m on the hull, then 2.894 m on the turret with three normal jump presses. Every landing is checked against the original mesh frame. Evidence: `artifacts/runtime/final_cpu_checkpoint_20261001.json` and its named filter logs.

Airborne collision now sweeps the feet as well as torso/shoulders along the vertical displacement; the prior 32 cm lowest probe could miss a ledge struck by the feet. Grounded 30 cm steps remain unchanged. The terrain-only update briefly marks roof walkers airborne before source support resolves them, so the airborne gate uses prior flight or positive jump velocity rather than that transient flag.

Review recipe: `catalog/shader_reviews/canyon_v2_ordinary_jump_cupola_fall_native800_20261001.v1.json`. It uses the original winter Canyon map, three ordinary Space/W jumps, then a W walk into the open cupola. The initial outside placement and already-open cupola are disclosed staging. There is no mount command or assigned vertical path during recording. Runtime/visual acceptance remains pending until its receipt passes and the actual frames are inspected.


Native review completed on the final OLD release SHA256 `19c9866299be4d27b6fff6ddf14d65ea6834c4e4f7447d51d91667ae60af48e1`: `captures/canyon_jump_fall_native800_v1_20261001.mp4`, 192 frames at24fps, 800×450, eight seconds, stereo44.1kHz gameplay audio. Its manifest passes9/9 state checks. The actual pitched Canyon spawn needed a longer third W pulse than the flat source fixture (0.5s instead of0.233s); failed rehearsal v2 is retained and no collision/jump-strength change was made. Rehearsal v3 and the final film both pass.

Decoded frame review confirms the exterior Tiger and upward jumps, clear cupola opening/swung lid at5.3s, then the actual interior at6.4s. Native aperture/inside stills and a contact sheet sit beside the video. `artifacts/runtime/canyon_jump_fall_native800_review_20261001.json` records the inspection and limits. This is an ordinary-input replay and frame review, not a human playtest; the closed-roof case remains CPU source-geometry evidence rather than a claim about this already-open-hatch film.
