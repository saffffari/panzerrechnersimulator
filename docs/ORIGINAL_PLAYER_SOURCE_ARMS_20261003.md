# Original source player arms

The player now uses the original CC0 `german-ww2-soldier-cc0` LowpolySoldier identity, including its unchanged atlas/red band. CPU Blender metadata baking retained all 49 original bone weights against the existing seated and relaxed static GLBs; maximum matching error is below 0.00000031 m. No source texture or GLB was replaced.

`first_person_arms.rs` partitions the original source into head-excluded body and source-weighted arms; arms update at most 30 Hz and reuse the existing two-link IK. P08 final gun/reload transforms feed its grip targets. Driver controls and carried ammunition also feed world targets. Gunner wheel target is an approximate local offset. Finger curls and grip points remain approximate until native witness validation. This is not an implemented pose for every possible cigarette, flashlight, repair and hatch interaction.

The local body is ordinary world geometry rather than an image overlay. Head/neck triangles are excluded to avoid self occlusion. Source compaction is checked against the renderer upload cap. Native visibility, gun grip, witness/third-person identity continuity and terrain/body self-clipping remain unvalidated. Root coordinates build and GPU tests; this lane did not launch or stop the game.

## Persistent living wounds

Nonfatal anatomical outcomes now persist separately from the KIA roster, produce blood at solved contact positions, and mask source triangles by imported bone family. Masking degenerates index triples without moving healthy vertices and uses a private actor index buffer; portable visibility refits the same topology. Original local arms/body and seated witness bodies have this mask. The commander witness mesh has distinct topology and does not yet share its mask.

Severed coherent parts use a bounded original-source part pool with actual fragment momentum divided by body-part kg, gravity and a planar cabin floor. No whole corpse is manufactured for a living survivor. The planar floor is not a complete moving/rotating cabin collision solution. Loss of both forearms prevents manual mechanisms without marking the operator KIA. Injury thresholds, bone-family triangle boundaries and wounds need native validation; these are implemented game rules, not measured human injury validation.

## Transition presence

The local body no longer disappears during climbing or seat changes. Climbing selects the original relaxed source pose and anchors feet to the same validated camera/perch path used by `finish_climb` (eye minus the physical foot-eye height). Normal dismounted body uses actual `foot.pos`; seated transitions follow the interpolated physical cabin eye. Hands retain their source neutral bounded pose during transition instead of being pulled to a mechanism in another seat. This preserves original source body/limb shadow presence, but the camera-local head remains excluded. An authored gripping/clamber animation and complete limb-to-armor collision clearance are not verified; no claim of perfect climbing anatomy is warranted.

## Camera-clearance correction

A native singleplayer P08 capture exposed a hand IK scale regression: the imported source Hand bone axes each measure approximately 0.00879023, but the solved wrist matrix was rebuilt at unit scale. Applying its delta against the inverse scaled source bone expanded hand/finger geometry approximately 113.76 times. The solved hand now retains decomposed source scale and normalized source rotation. The CPU regression checks imported TRS roundtrip and finite bounded posed arm vertices across three aimed pitches for both source poses. Native per-page isolation captures remain the acceptance evidence; a neutral center ray missing the chest was insufficient.

Seated source bodies now remain anchored to the physical station eye. Remote optic/port render-camera motion cannot drag the chest forward; actual seat transition still uses the physical interpolated eye. Main optic exterior passes suppress only the local observer arm/body slots, restoring their prior draw state for the interior pass.
