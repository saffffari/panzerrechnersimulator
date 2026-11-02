# Expedition development log — 30 October 2026

## Source 35 verification and source 36 corrections

- The offline CPU suite passes: 2,299 passed, 78 ignored, 36 LAN tests excluded. This is not a LAN result. Log: `artifacts/expedition-20261030/test35-full-offline.log`.
- Native fuel source tests pass all four cap opening/return paths, finite transfer and binary continuation. The live filler witness is registered for source 36; its translated-world contact correction was not in executable 35.
- The reverse winch route passes the complete original carrier geometry test. The live recovery review passes 28 of 29 checks, including actual restowing and redeployment with preserved fuel/work. Its remaining failure was a cab interaction taking priority over the held fuel can. That routing is corrected for source 36 and needs another runtime check. Receipt: `captures/recovery_gameplay_20261030_b35a.json`.
- Cold recommission passes fifteen checks through four actual rack contacts and the ordinary starter, preserving the disabled hull until crank completion. Fresh resume then fails because corpse render pools were allocated lazily and their source identities were not reconstructed. Source 36 adds explicit actor-owned source allocation persistence and restoration; it is not yet verified. Receipt: `captures/expedition_cold_recommission_20261030_b35a.json`.
- The continuous medical review reaches treatment, source-body loading, walked cab access, a loaded carrier journey, and unloading. It fails the dressing assertion because triage correctly chose airway first, then fails the route to the tent while the patient remains at the carrier. The review now reassesses for the dressing and records exact body-contact diagnostics; medical evacuation is still open. Receipt and 52-second clip: `captures/field_medical_recovery_20261030_b35a.json`.
- Full engine-cover motion still fails a real contact with rear turret track-carrier hardware. The initial numerical fitted-edge issue is corrected; the remaining obstacle cannot be waived. Full engine exchange remains open.
- The previous long test failed at 300 seconds because its omitted presentation interval accumulated unconsumed track stamps beyond the save bound. The corrected run presents every five simulation ticks, including intervals not recorded to video. Its 300-second battlefield checkpoint now writes successfully (59,492,380 bytes); the requested 1,800-second run is still underway. This is not yet a complete-duration result or human feedback.

## Completion pass — source 33 / source 34 verification in progress

This section supersedes the source-20 status below. Work remains on `raymarching`; nothing has
been published. Original captures and private source assets remain local.

- Build31 complete-world review passed: all eleven owner blocks match immediately after a fresh
  reconstruction and after twelve further simulation steps. The real frontend candidate also
  leaves the old world unchanged. Open map ink and a manual saved during a page turn restore
  exactly. Receipt: `captures/expedition_world_20261030_b31a.json`. Later owner changes require
  another runtime pass; this is not a cross-build save migration guarantee.
- Build31 recovery review passed26 checks, including finite fuel/cargo, the same wounded actor
  aboard a30m carrier drive, braking, owner restoration, unloading, anchored winching and
  remounting the same Tiger. Its45-second video contains explicitly labelled setup cuts. It does
  not prove tent admission, replacement boarding, a complete heavy overhaul or a hostile recovery.
- Build31 source workshop review passed69 checks for a finite optic replacement and battery lamp.
  Twelve mission sites on one12km theatre have13 passed Game-rule checks for dispatch cost,
  capture, rescue loss, withdrawal, finite stock, camp relocation, recapture and finale. Travel
  and enemy outcomes in that mission witness are staged, not played mission victories.
- The600-second ordinary travel run covers754m before retained track damage. The subsequent
  armed1800-second attempt ended at97.133s with the commander killed. It is **not** a30-minute
  pass. New diagnostics retain projectile/contact evidence; the test driver now alternates
  short firing halts with movement. Initial camp was moved about2km from its nearest guard.
- Source33 test compilation passes. The full CPU suite records2316 passed,7 failed,76 ignored
  (`artifacts/expedition-20261030/test33-full.log`). Four failures are LAN network behavior,
  two are old machine-specific capture paths (corrected; six related checks then pass), and one
  is a cross-platform generated-float text comparison (semantic parity fix awaiting source34).
- Focused source33 checks pass ammunition hazards/intake3, rack inspection4, engine gates4,
  field-kit conservation2, source body dragging2, hoist3, and the previously failing toolbox,
  acoustic ownership and pause-menu checks. Counts exclude filters that matched no tests.
- Engine side locks required a measured150micrometre axial fit clearance; the rear arm needed
  its documented bore recut after head shaping. All three latches now pass the complete original
  source sweep (`test33c-engine-cover-source.log`). Full lid return and runtime LMB checks follow.
- Source34 adds safe cold recommission through the actual starter, saved reverse winch loading,
  and the physical manual's field-care/recovery chapters. They are not considered playable passes
  until their runtime reviews succeed. Boarding still finds actual lower-cabin body contacts;
  there is no collision exemption, miniature body or arrival teleport.

Remaining gates: complete casualty-to-tent-to-duty transport, source-clear replacement boarding,
repeatable winch stow/use/return, severe engine/fuel repairs, cold restart through real access,
latest complete-world continuation, and a linked operation run under ordinary enemies. Independent
human feedback has not occurred; automation and agent review do not establish subjective fun.

## Visual and correctness review — earlier source-20 checkpoint

This section supersedes the older checkpoint below. The owner redirected this pass toward
restoring the warm cabin, world density, the compact stocked camp, vehicle AI/rollover correctness,
physical loader work and reactive crew poses. The full expedition roadmap remains unfinished.

- Optimized snapshot 20 compiled (`artifacts/expedition-20261030/release-build-20.log`). All 43
  focused checks passed against its matching CPU test build, including the three private standing
  rig checks and actual shelf-support geometry (`focused-tests-20.log`). This is a local headless
  development snapshot on `raymarching`, not a published build or full-suite acceptance result.
- Five fixed Act One station views (commander, loader, driver, gunner, night loader) were byte-identical
  between snapshot 11 and the owner's saved Oct 29 play executable, using the same current assets.
  `captures/renderer_review_20261030_comparison.json` preserves hashes/settings. This rules out a
  difference in those views between those binaries, not older regressions or shared asset changes.
- Preferred references are `rm_ao6_32_commander_noon_shut.png`, `rm_int6_noon_radioman.png` and
  `tpz_needle_widths_480.png`. Their receipts identify the original Canyon workshop, Raymarch,
  clear fixed clocks and parked station cameras. Matching modern harness captures confirmed lost
  orange illumination. The former instrument illuminator had been heavily reduced; a sixfold
  outdoor rod-display boost also washed night cabin colours blue-grey.
- Warm revision C uses the actual finite dome fixture, a restrained TPZ local light, and normal
  cabin rod display gain while keeping the outdoor boost. Indicator outputs are unchanged.
  Current examples: `renderer_warm_20261030_c_night_loader.png` and
  `renderer_warm_20261030_c_dial_tpz_close.png`. This is an authored look calibration, not measured
  historical bulb power. See the physical-truth audit for limits.
- The physical book's G0 albedo was correct. A first-frame empty/stale TLAS binding caused the
  initial lighting to differ from a later identical page. Binding after actual TLAS allocation
  fixed that: v8 first/revisited mean luminance ratio 1.0043. The book now faces the actual dome
  lamp, with larger print, less crowded lines and a geometry-derived reading distance. Revision 12
  fits the entire resting spread in view and passes all spread/return checks (luminance ratio
  1.00027); eight CPU manual checks pass. Inspected images are under
  `expedition_manual_20261030_v12_framed`; revision 11 also has a page-turn video with the same
  readable print but an older resting frame. Inspection zoom remains useful for the far page.
  No emissive paper/fill-light workaround was added.
- Compact camp now places shelters independently on unmodified terrain. Only the 7×10 m workshop
  slab is raised. Four medical stretchers (two active beds), sixteen quarters cots, medical racks,
  twelve fuel drums and shell/small-arms stores are placed. Medical and ammunition cases now sit on
  measured source shelf surfaces; medical cases include two small stacks, with unchanged finite
  supplies. The actual-world review again passes six bedside checks
  (`expedition_live_camp_20261030_v4.json` and the final source-20 v5 repeat). It stages a delivered wounded body, not extraction or
  transport. The oblique medical view exposes beds and the medic, but stock remains dark/noisy;
  personnel obscure the ammunition doorway. These are not complete visual stock acceptance.
  Patient and some body poses remain visibly unfinished.
- Private-asset door/contact and medic/reserve route tests pass with the dense layout. Source camp
  rig tests pass after the mesh-node/skin-frame correction. The furniture count and dome radiometry
  assertions were corrected for the intentional changes and passed in the focused verification.
- The default Expedition workshop lacks the GPU scatter layer present in Canyon v2. A bounded
  180 m camp approach now uses the existing world instancing path (240 pieces/36 pushables maximum,
  real source props, lane/obstacle clearance). The original nearest endpoint was disconnected from
  camp and produced zero objects. Reachable-endpoint fallback fixes that: v4 retains all 146 pieces,
  including 27 pushables, after final cover insertion. `live-camp-v4.log` preserves route/rejection
  receipts. A final source-20 guard rejects full footprints whose obstacle-clearance margin exceeds
  the queried region; its boundary regression passes. Independent v5 placement again retains all
  146 objects, with byte-identical approach/medical/ammunition images against v4. The distant
  approach still looks sparse; incremental frame cost is unprofiled. This
  does not populate the entire world. Low debris is traversable; significant props use existing
  source-bounds pushable collision rather than exact triangle contact.
- Halftrack live input review passes all eight checks (`halftrack_driving_20261030_a.json`): actual
  boarding, acceleration, finite fuel use, moving-exit refusal, braking and stopped exit preserve
  person/ammunition identity. Three focused runtime checks also pass. Handling is kinematic with
  an exterior camera and static track belts. Cargo, towing, crane, protection, full succession and
  crash accuracy are not proven by this short run.
- The circling brawl forced forward pursuit at close range. Approach/hold/reverse now stops at
  useful range. A second bug used the referee instead of the actual opponent for firing visibility;
  movement, lay and fire now agree on the opponent. `vehicle_brawl_fix_20261030_b` records 90 seconds,
  three shots/penetrations and three knockouts, including a turret separation. The inverted fixture
  `vehicle_inverted_fix_20261030_b` records 20 seconds with zero horizontal drive or shots and zero
  source support penetration. Roof support uses actual source vertices and excludes a detached
  turret. Contact correction preserves falling height. This deliberate inversion fixture and
  bounded inelastic fallback do not establish full tumbling/crash physics or every duel scenario.
- The loader now retains one actual cartridge from rack to chamber, moves both arms through case
  handling/crouch/carry/ram, and stands at the source periscope when idle. The v2 witness passes five
  ammunition/reload checks; 16 sampled active hand contacts report zero error. Those samples measure
  stored pose-time targets, not continuous moving-cartridge contact. Source feet and some grips are
  obscured, and turning, finger motion and whole-body clipping remain unfinished. Actual chassis
  acceleration and one projectile-momentum recoil impulse drive bounded source-rig balance;
  authored springs/IK are not a complete biomechanical simulation.
- `crew_retained_kia_20261030_b` passes six checks and visibly retains the same loader corpse after
  eight seconds. Capture exposure now converges before inspection. No automatic corpse lifetime
  removal is present; crowded poses/clipping still need work, and this is one exercised death path.

All new captures stay flat under `captures/`. No private assets or packages were published.

## Baseline and scope

- Detailed work breakdown: `EXPEDITION_IMPLEMENTATION_ROADMAP_20261030.md`.
- Active branch stays `raymarching`; fast-forwarded from `5ac80b7` to handoff `63269ea`.
- Original untracked design copies preserved under ignored
  `artifacts/handoff-source-backup-20261030/`; no private asset or package removed.
- Written-word principle amended: practical written material allowed, spoken dialogue excluded.
- The new expedition is not yet a playable replacement for the old campaign. Do not describe its
  state-model proof as a physical recovery playtest or claim the live Game can suspend/restore yet.

## Earlier checkpoint 10 (historical; superseded by the review above)

Working branch remains `raymarching` at the integrated handoff `63269ea`, with local source changes.
The latest verified optimized snapshot is `bench_expedition_20261030_10.exe`. It is a private,
headless development executable, not a published or finished play build. No private assets were
committed. Set `KESSEL_RENDERER=raymarch` explicitly for each capture invocation; shell environment
does not persist between tool calls. The default renderer is the older physical backend.

### Verified integrated behavior

- `solo-controls-review` passed ten control/state assertions on the source Tiger: seven cruise
  notches, crew liveness gates, remote gun control and explicit autonomous-gunner experiment.
  Source-geometry personal-space checks also passed. This does not establish subjective pacing.
- `expedition-live-review` passed three consecutive commander deaths and replacements in one
  retained Game, first in the source-Tiger studio and then in actual Act One with the support camp
  installed. The Tiger pose/modules, finite personal ammunition and three distinct corpses remain;
  the roster falls from sixteen to thirteen. Reselecting a live soldier cannot refill ammunition.
  Receipts: `captures/expedition_succession_20261030_v1.json` and
  `captures/expedition_succession_camp_20261030_v2.json`.
- Human death is separated from vehicle destruction in this opt-in mode. New menu entry is
  **Recovery Playtest**; explicit succession retains the world rather than rebuilding the act.
- Persistent corpses use actor-owned render slots. An extracted body becomes world-space and no
  longer follows the tank. Assisted extraction uses a real open hatch; moving the articulated
  body as a unit is a disclosed approximation, not a biomechanical lifting solver.
- Actual medical physiology, finite supplies, two beds and one named medic are coordinated
  without free healing. Ten medical tests pass, including elapsed wound time during medic travel,
  finite-stock use, selected-bed ownership and field/camp authority handoff. The live bedside
  review is in progress and is not yet a passed evacuation test.
- Camp source geometry includes medical/quarters tents, sixteen cots, workshop/supply/rest areas
  and the private imported halftrack. Source-triangle placement/contact and routes through real
  doors pass focused CPU tests, including the private-asset route test.
- Camp grading changes both visible terrain and the authoritative heightfield. A failed apron
  correspondence test exposed under-refinement at the cut/fill edge; refining the whole apron
  fixed it and both terrain tests now pass.

### New source work awaiting its next integrated verification

- Camp reserve actors are tied to real PersonIds, with source skeleton/weights, named hover,
  duty/rest walks, salute and report-to-load interactions. No decorative duplicate personnel,
  cargo creation or completed vehicle boarding is implied. The source rig test found a real
  mesh-node versus skin-frame mismatch; the exact inverse transform correction needs rerun.
- Living evacuated casualties can retain their own physiology/body while their old field slot is
  filled. Restoring a patient reuses identity, wounds and source appearance. Latest expanded-slot
  tests and live handoff review are pending.
- The relief screen can explicitly advance ten seconds for a medical report, with the actual
  battle, medic movement and wounds advancing. Selection otherwise pauses; no automatic healing
  or takeover. This resolves the all-survivors-temporarily-incapacitated dead end.
- LMB and hover routes now include physical beds and named camp reserves.
- Halftrack driving is being implemented separately. The currently captured halftrack is still
  a static imported transport prototype, not a historical heavy-recovery vehicle or working crane.

### Rendering and presentation findings

- Physical manual has sixteen pages, actual page geometry/leaf turns, yes/no symptom branches,
  index/bookmark and inspection zoom. Seven earlier CPU checks passed. **It has not passed visual
  review:** later spreads render black. Actual page albedo readbacks v6 are correct, narrowing the
  issue to physical surface rendering/visibility. No emissive paper or fill-light workaround added.
- `captures/expedition_manual_20261030_v5_page_turn.mp4` records the unresolved state, not a success.
- Monochrome map uses actual roads and building outlines. Full navigation/playability review remains.
- Explosion changes preserve actual damage: distinct detonation identity, faster physical cooling,
  removal of unsupported stationary embers and ground-dependent ejecta tint. Paired 18-frame
  captures show the early fireball cooling into dust rather than remaining a giant orange sphere.
  This is too short and includes other changes; it is not a performance improvement claim.
- First source-camp overview was obscured by the bare renderer's default fog. The source-only
  inspection now requests the existing clear-day environment and discloses it. Actual expedition
  weather is unchanged. Better visual capture is pending.

### Still unfinished

Full medical extraction/transport playtest; camp-reserve combat damage; drivable recovery vehicle,
towing and crane; full component workshop/tool gameplay; resource loading/refueling; large unified
theatre and its missions/frontline/finale; complete save/resume of a live battlefield; longer
performance measurements and human playtest. Existing improvements must not be represented as the
completed expedition. The detailed roadmap remains the scope authority.

### Latest verification receipts

- Build logs `release-build-08.log`, `release-build-09.log`, `release-build-10.log` in
  `artifacts/expedition-20261030/` passed.
- Unit compilation `compile-tests-10.log` passed. Focused medical, body, camp geometry and route
  tests passed; source rig test failed and has a proposed correction, still to verify.
- Earlier full-suite run: 2,134 passed, 18 failed, 63 ignored. Windows campaign-save failures have
  since been fixed and the nine focused save tests pass. Other baseline/asset/platform failures
  remain recorded; there is no claim of a clean whole-suite run.
- Private capture paths remain flat under repository `captures/` under the latest owner rules.

## Initial implementation notes (historical checkpoint)

## Implemented, with verification so far

- New `expedition` coordinator: sixteen named people, identity separate from post, eight preparation
  dispatches, permanent casualties, successor selection, abandoned equipment, repair progress,
  camp relocation and default recovery during the finale. Nine CPU tests passed.
- Independent versioned expedition save/transaction layer, idempotent events, corruption backup and
  explicit incomplete-live-adapter marker. Five CPU tests passed with TEMP/TMP in the workspace.
  The system temporary directory rejected rename under the execution sandbox; no unsafe fallback
  was added to the writer.
- CPU proof executable `expedition-proof`: real save/load transactions through repeated casualties,
  partial repairs, exhausted preparation budget and final-operation recovery. Passed; receipt:
  `artifacts/expedition-20261030/core-recovery-proof-01.json`.
- Q/E seven-notch driver cruise, standing orders through interior walking, remote gun arrows through
  living gunner, X exit/stow, consolidated LMB, V coarse designation and deliberate K auto-gunner
  comparison mode. Nine crew unit tests passed. Full visual/control playtest remains pending.
- Physical manual: normally lit covers/pages, hinged page turn, sixteen pages, symptom decisions,
  bookmark, inspection zoom and physical pointer intersection. Six CPU tests passed. Drawings are
  labeled explanatory schematics, not extracted CAD or historical facsimiles.
- Manual input integrated: B opens, X closes, click printed choices/page corners, wheel/PageUp/
  PageDown turns, +/- zoom. Binocular shortcut moved to I. Book consumes gameplay actions.
- Release build passed before the first GPU book review. Subsequent edits require another build.

## In progress

- Headless manual screenshots and short video; inspect for readability, page registration,
  illumination and occlusion before declaring visual completion.
- Monochrome map linework, live-position marker and actual building outlines.
- Crew personal-space contacts using actual posed geometry, before larger animation work.
- Halftrack license/provenance search and private import. No purchased asset authorized or acquired.

## Important remaining dependencies

The live Game conflates player death with Tiger life and ties wound identity to five role slots.
That must be separated before succession can safely reuse the same world. New save records do not
yet export/import in-flight projectiles, all live wounds, fires and exact mechanical state. No
playable camp, medical evacuation, recovery driving, towing, assembly crane, massive theatre, or
final mission has been completed. Those tasks remain explicit in the roadmap.

## Verification evidence

- `artifacts/expedition-20261030/expedition-tests.log` records the initial system-temp restriction;
  the workspace-temp rerun passed all save tests.
- `artifacts/expedition-20261030/manual-tests.log`: six tests passed.
- `artifacts/expedition-20261030/crew-controls-tests.log`: nine tests passed.
- `artifacts/expedition-20261030/release-build.log`: optimized build passed.
- Captures are private, local, and never committed. GPU runs use an owned `bench_*.exe`, one at a time.
