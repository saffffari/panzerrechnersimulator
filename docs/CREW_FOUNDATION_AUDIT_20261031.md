# Crew foundation audit — 2026-10-31

Scope: ordinary solo Loader motion, mechanical hand contacts, zero-time restoration, retained corpse lifetime, and the subsequent hull-local cartridge-owner correction. Source assets, lighting, limb lengths and collider tolerances remain unchanged. The follow-up explicitly versions cartridge frame semantics as `crew.runtime4`.

## Reproducible findings and bounded corrections

1. **Hands lagged moving ammunition between pose samples.** Cartridge work advances every simulation step, but the complete skin formerly waited for the 30 Hz clock. The old receipt compared hands to the targets cached with that same skin, masking the difference from the current cartridge. Changed mechanical targets now refresh that actor immediately; idle balance keeps its existing cadence. Receipt fields separate current-mechanism error from cached solver error.
2. **Zero-time Loader service changed procedure state.** A presentation/save-restore update could clear moving/gaze state and run phase/inventory transitions despite no elapsed time. Non-positive/non-finite steps now preserve Work and its finite round ownership while keeping the derived `loader_holding` flag truthful.
3. **Work direction snapped the whole standing body.** Periscope, rack, and breech goals previously assigned yaw instantly. The root now follows the shortest yaw arc at a bounded authored 3 rad/s. This does not claim foot pivot contact or animation blending is finished.
4. **Spent-case extraction advanced without testing the grasp.** Case travel now uses the same exact original-limb contact admission as carrying/ramming a live round. An unreachable candidate pauses extraction; it does not stretch the source arms or move the case without a grip.
5. **A held cartridge retained a stale world pose during chassis motion.** The last accepted pose is now hull-local. Rendered brass, source hand targets, ammunition contact and interrupted drops reconstruct it from the current hull, including chassis projection after loader service. Relative turret rejection behavior is deliberately unchanged; this is common-motion covariance, not new collision handling. `crew.runtime4` converts an admitted v3 world matrix using the exact saved/restored hull. The complete-world same-build fingerprint gate remains in force.

## Evidence and validation status

- `rustfmt --edition 2021 --emit stdout` parsed both changed Rust files successfully. Cargo/tests/GPU remain owned by the parent agent and were not run by this audit.
- Independent Python source-joint audit: `artifacts/expedition-20261030/crew_extraction_source_audit_20261031.json`. All nine ordinary extraction cases (30/60/120 Hz; gun pitch −0.1, 0, +0.2 rad) completed in 0.80–0.834 seconds without blocked reach steps. This establishes original-joint reach only, not full-body collision or visual quality.
- New Rust tests: `changed_mechanical_contact_bypasses_idle_pose_cadence`, `contact_receipt_detects_current_mechanism_motion_hidden_by_cached_target`, `loader_zero_time_preserves_work_and_inventory`, `loader_turn_uses_bounded_short_arc_without_overshoot`, and ignored private-source `spent_case_extraction_keeps_exact_source_two_hand_reach`.
- Prior visual evidence examined: `captures/crew_loader_balance_20261030_v2_frame05.png` and `..._frame09.png`, plus the v2 manifest. These show bent rack handling and return to standing; dark/occluded hands and one-second receipts do not establish continuous contact. No new current-build animation acceptance is claimed.
- Parent-run native57 suite: **2,390 passed, 0 failed, 92 ignored, 75 filtered**. All five new cartridge-frame regressions passed, including blocked Carry/Ram, late hull projection, unchanged turret-only rejection, actual source-face picking/grips/drop, and independent legacy wire conversion. Build58 changed the review's audio policy only; the production frame correction is unchanged.

## Corpse persistence audit

Current corpse pages are keyed by stable actor identity, retain up to the finite 16-person roster, and have no decay timer. Source review found ordinary removal only when explicitly reembodying that exact living, capable, world-space patient; fresh-match reset and whole-world restore are separate paths. Historical disappearance clips have not been reproduced against the current actor-owned pages. No speculative lifetime fix was applied.

## Remaining limitations

- Full source-triangle swept clearance of a loading body/cartridge is not implemented by the basket/breech walking envelope. Exact hand endpoints do not prove forearm, shoulder, head, or cartridge clearance.
- Current IK has no palm alignment, finger curl, or continuous hand-change animation. Reach/crouch transitions can still be abrupt.
- The ammunition picker still uses strict per-triangle barycentric tests and is not watertight at source facet seams. Frame tests use an actual face-interior ray and retain seam diagnostics; they do not claim to repair picking at seams.
- Gait follows distance moved, but turning in place lacks planted-foot pivots. The new yaw cap removes instantaneous turns without claiming a complete locomotion solution.
- No current corpse-evaporation reproducer or performance measurement was obtained. The bounded reload evidence below is not general animation or collision acceptance.

## Moving-cartridge follow-up witness

`catalog/shader_reviews/solo_loader_moving_cartridge_witness_20261031.v1.json` retains the
single ordinary shot and disclosed render-only crew camera of the original witness. The b44
baseline has Carry at 5/6 seconds and Ram at 7 seconds, while its old driving input did not
start until 12 seconds. The new definition accelerates at 4.25-6.6 seconds and brakes through
opposite drive input at 6.6-7.7 seconds. Actual speed checkpoints and unchanged finite-round
outcomes are required. Inspect the actual nested phase receipts to establish that both phases
really overlap movement; the input schedule does not force animation or reload success.
The parent-run build57 receipt `captures/foundation_loader_moving_20261031_b57_manifest.json`
passes all 11 outcomes and required timeline actions. The actual samples show Carry at 5s
(0.6692 m/s) and 6s (1.4995 m/s), then Ram at 7s (2.1886 m/s), all holding the same slot49
cartridge in the hull frame. Close/completed1 occurs by 8s and Idle by 9s. The tank moves
3.7049 m in total. Sampled current hand error is at most 0.00001526 m, zero at 6/7s.

Speed rises from 6s to 7s despite the requested brake input beginning at 6.6s, then reaches
zero by 8s. These one-second samples establish moving Carry and Ram and the later stop;
they do not establish the exact start of braking or continuous contact throughout it.
The 5/6/7.5/8s frames were inspected: brass progresses toward the breech without an obvious
world-stranded round, but hand/brass occlusion, absent finger curl and tight body geometry
still prevent declaring full collision or animation acceptance.

## Active-cartridge cold restore verified through build58

`captures/foundation_world_20261031_b58.json` passes separate ordinary Carry and Ram
branches. Each retains the same cartridge source identity and exact world pose immediately
after cold reconstruction, with **zero simulation ticks** before comparison. All 11 saved
owner payloads match exactly at that point and again after 12 ordinary 120 Hz continuation
ticks; continued Work also matches. This closes the pending active-loader whole-world
continuation check for the narrow hull-frame correction.

Both uninterrupted and restored branches use the same existing mute policy. Acoustic owner
bytes remain in the comparisons, but this is **muted state continuity**, not live-audio voice
or cache continuity. Legacy frame conversion retains its measured f32 error allowance;
new v4 zero-tick payloads are exact. Neither result expands the source-contact or animation
claims above, nor does it bypass the full-save build/source compatibility gates.

## Medical bed pose and capture framing remain unfinished

The b55 continuous recovery run passed all 15 state checks, including physical loading,
driving, unloading, delivery, bed admission and finite medic care. The admitted and treated
images were inspected separately from that functional result. All 60 care samples retain
the same living Driver actor 1397751810 / Person4 at the same world position; there is no
evidence in this run of disappearing anatomy or identity replacement.

Bed admission currently applies one rigid placement at the bed surface plus a fixed 0.20 m
centre offset. It preserves the existing seated/injured joint angles and then excludes the
camp-owned patient from ordinary ragdoll settling. Raised knees therefore remain raised;
this is not finished placement animation or source-verified support on the canvas. The fixed
centre offset also does not prove correct hand/back contact. A future correction needs
supported anatomical repositioning and source clearance, rather than stretching, shrinking,
silently resetting the injury pose, or calling the present functional test animation acceptance.

The final treated screenshot has a separate framing defect: the 60-second care wait uses
ordinary standing input, raising the eye from the prior 1.05 m crouch to 1.70 m while keeping
the old aim. The photo helper does not look back down at the patient. A diagnostic correction
can aim at the actual retained body from the unchanged helper position; it must be disclosed
as camera framing, with no body relocation or lighting boost. Production bed behavior and
the medical review controller were not changed by this follow-up audit.
