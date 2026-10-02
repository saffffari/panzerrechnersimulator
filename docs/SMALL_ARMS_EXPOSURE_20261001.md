# Small-arms exposure and armor contacts — 2026-10-01

This change is saved in the main and old-renderer review trees. It does not replace the running playtest executable. Native gameplay verification remains pending.

## Behavior

Mounted infantry perception now considers a visible anatomical head or the nearest visible Tiger source surface. Normal sensor range, field of view, world cover, terrain/tree sight and remembered-contact rules still apply. A source point is used only in the perception snapshot; it is not an invented human in the damage list. Buttoned-up targets can therefore attract ordinary rifle/MG fire and real armor-hit sound without receiving small-arms armor damage.

Incoming live small arms still travel on the existing swept 240 Hz projectile clock. The player's articulated source triangles now replace its coarse solid hull/turret bounds for these contacts. The source set is the same retained hull, turret, gun, moving hatches, glass, optics and interior hardware used by the source-surface system. All normal world contacts, source armor and the existing eleven anatomical body OBBs compete by nearest swept distance. There is no camera-dependent immunity, hatch-state protection override, through-hull hit, trigger-time hit or forced hit.

An actual energetic crew-body contact uses the existing crew injury policy and retained crew identity. It cannot wound an empty station or duplicate a recorded casualty. Ordinary foot-player HP behavior remains unchanged. The couch commander's raised render pose and anatomy now share the same head-out transform.

## Explicit limits

- Source-triangle contact is exact for the loaded collision source geometry, not a claim that that authored mesh exactly represents every real Tiger material or aperture.
- Anatomical hit volumes remain the existing eleven OBB approximation and the existing energetic-contact incapacitation policy. This does not add a new wound/medical model.
- Other vehicles retain their existing small-arms bounds. Cannon vehicle contact and ordinary foot/NPC body approximations are unchanged.
- The mounted target sensor keeps its existing 45 m range and field-of-view rules; this change does not grant free location knowledge or increase range.
- The CPU source fixture covers the authored hull/turret/gun/cupola lid and transformed exposed body, plus nearer-wall ordering. It does not substitute for a full live-game engagement capture.

## Verification

OLD compiled successfully with two low-priority build jobs. Small-arms tests: **3 passed, 0 failed, 1 existing GPU test ignored**. The source fixture passed on two transformed hull poses, including hull pitch/roll and independent turret yaw. Infantry perception/pose/aim regressions: **10 passed**; crew identity/casualty regressions: **6 passed**; swept projectile regressions: **4 passed**. AT geometry tests passed **4/4**, including their exposed-crew/nearer-contact ordering.

Evidence: `artifacts/runtime/small_arms_exposure_tests_20261001.log` and `artifacts/runtime/exposure_combined_cpu_filters_20261001.log` (per-filter machine summary beside it as JSON). The combined batch also passed the requested powertrain, traction, terrain support, authored tilt, squads, Canyon, equipment, graphics and mounted-MG tests. Zero-match exploratory filters are retained honestly in that receipt; the correct infantry filter was subsequently run.

The same small-arms source changes are mirrored to main; this checkpoint compiled OLD. No GPU capture or playtest restart was performed. New changes from other work lanes after the compiler snapshot need the next coordinated build.
