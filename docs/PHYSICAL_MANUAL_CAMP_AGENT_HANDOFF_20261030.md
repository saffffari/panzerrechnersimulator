# Physical manual, effects, camp and reserve agent handoff

Updated after the parent's source-20 verification on 2026-10-30. All work is in the shared
`C:\shop\panzerrechnersystem` checkout on the requested raymarching branch. The parent owns
serial Cargo/GPU validation. Its `artifacts/expedition-20261030/focused-tests-20.log` records
43 passing focused tests, including eight manual tests and three dressing tests. Source-20
release compilation passes, and `live-camp-v5.log` independently retains all 146 pieces and
27 pushables after the query-boundary safeguard. The v5 live camp review also passes all six
staged bedside-care checks, and approach/medical/ammunition images are byte-identical to v4. The
checkpoints below are bounded proofs, not a claim that the game is complete.

## Latest source milestone: finite visible camp reserves

`src/camp_people.rs` and crew agent's `src/camp_people_rig.rs` are registered as Game child
modules. `Game::build_expedition_act` allocates sixteen hidden deformable source meshes before
the legacy effect tail; standard modes allocate none. Camp slots join the renderer's existing
raster-only articulated actor list and temporal-dynamic tagging. `Game::update` advances the
actual local actors. All narrow Game field/initializer/module hooks are written.

People come only from `Expedition.people`: living, capable, at this camp, not controlled,
not bound to the five live field slots, and not admitted to a medical bed. Medical patients
retain their existing physical bodies. The actor appearance uses the five reviewed crew
atlases and the original unified source UV layout; no new textures or people are generated.

The source rig validates the original 39 joints, bind matrices, 914 runtime vertices, 564
triangles and both weight sets. Procedural walking follows actual distance at a 0.8 m stride;
normal/tangent skinning uses the same source-world joint deltas. Salute/work poses are authored
gestures. Zero-time capture no longer resets a walking pose. Flat feet, exact source geometry,
and the original anatomy are retained. Visual gait quality is not yet reviewed.

Actors perform local duty/rest walks. LMB assignment, once wired in main, salutes and then
walks to a distinct loading position. This does not board a vehicle, create cargo, consume a
dispatch or replenish stock. Local routes use bounded A* and finite body sweeps against the
same visible camp triangles. Actors steer/yield around one another. No arbitrary camp-sized
collision boxes or teleports are used to bypass blocked passages.

Public APIs ready for parent wiring:

- `Game::camp_person_hover(eye, direction) -> Option<String>`: exact source triangle target,
  clear sight, rank/surname, trained role, task and available interaction.
- `Game::camp_person_interact(eye, direction) -> bool`: consumes an actual person click; an
  available person salutes and reports to a loading point. Call before ordinary on-foot LMB
  firing, on the existing primary press edge. Main and HUD are intentionally not modified.
- `Game::camp_people_receipt() -> serde_json::Value`: identities, position, goal, moving state,
  task, remaining waypoints and source vertex count. Add to live capture receipts.
- `Game::camp_medic_at_bed(bed_id: u8) -> bool`: actual named medic alive/capable, physically
  within 0.65 m of that bed's attendant point and assigned medical duty. The medical agent's
  live procedure gate already calls this. The medic follows active triage, including rotated
  camp anchors. Patient physiology continues while the medic walks.

Required follow-up: compile and run the focused tests; wire hover/LMB; capture the actual live
camp roster, a selected reserve walking through a real door, and medic travel between both
beds. Verify geometry/shadows, gait, names and no duplicate bodies across succession.

Known limits: only initial PersonIds 1–16 have reserved slots; source reserve projectile hits,
ragdoll conversion and general on-foot body blocking are not connected. Camp-to-field face
continuity still follows existing field slot identity and needs review. There is no real cargo
boarding/loading task yet. Pose/work gestures are not a hand-contact or biomechanical solver.
Do not describe this as combat-complete camp AI.

## Camp installation and source geometry

`support_camp.rs`, `support_camp_runtime.rs`, `camp_terrain_pad.rs` and
`support_camp_capture.rs` are complete source milestones. See
`SUPPORT_CAMP_PROTOTYPE_20261030.md` for exact source paths and anchors.

The optional live build installs a medical tent with two source stretchers, sixteen empty
cots, quarters, workshop, food/rest area, ammunition/fuel props and a parked source halftrack.
The geometry itself creates no personnel or inventory. `installed_camp_anchors()` returns
world-space bed/medic/load points; `installed_camp_receipt()` reports installed source parts
and any terrain grading. `expedition_initial_camp_position()` uses this installed root.

The site search covers the full source footprint and rejects actual source buildings/trees,
openings, enemy spawn sites and final late cover. Current installation accepts at most 0.60 m
relief and seats the individual shelters on the existing terrain. It does not grade or flatten
the ground: the actual receipt has `grading: null`. Only the workshop has its visible 7 x 10 m
concrete service slab. `camp_terrain_pad.rs` remains available source but is not this live path.

Existing foot body clearance and world sight checks use the source triangle contact cache.
Canvas stops walking/sight but is not a walkable roof. Projectile materials, camp fire,
destruction, tank crushing and live relocation remain separate unfinished integration.
The halftrack now has a separate prototype driving runtime; its limited proof is recorded below.

Parent already registered the `camp-review <prefix>` capture command. It makes six source
layout views on a neutral inspection ground, not a live medical/roster proof. Parent also
owns `expedition-live-review` and its installed-camp option for the actual world.

## Camp approach: actual generation restored, visual density still limited

`src/camp_dressing.rs` uses the existing worldgen prototypes, pages and scatter contacts. In
the failed camp-v3 run, the nearest clear endpoint belonged to a disconnected region, so the
generator produced zero pieces. The reachable-endpoint fallback now chooses the nearest
endpoint in the camp-connected navigation region. Terrain, source obstacles, openings,
camp footprint and the eight-metre route clearance remain enforced. Shorter routes omit
later pockets instead of stacking several pockets at their last point.

The actual source-19 run is `artifacts/expedition-20261030/live-camp-v4.log`, lines 381–382 and
406. For seed 2637955107 it attempted 264 pieces, accepted **146**, registered **27 pushables**,
and retained all 146 after final world-cover culling. There were no missing sources. Initial
candidate rejections were 93 overlaps with accepted pieces, two camp overlaps, eleven overly
tall low-detail pieces and twelve source-obstacle overlaps; zero were rejected by final culling.
The receipt records the actual route and confirms `requested_end_connected: false`. Limits
remain 240 pieces and 36 pushables, in six separated shoulder pockets along the bounded approach.

`captures/expedition_live_camp_20261030_v4_approach.png` and `_overview.png` are the live-world
views. The approach still appears sparse at its ordinary viewing distance; the numerical
placement success does not resolve the user's broader empty-world concern. The generator
does not create a road, guarantee the entire camp-to-deployment journey, or add mission content.

The final read-only audit found an edge-case risk: a fallback route could hug the navigation
rectangle, allowing a side pocket to extend beyond the bounds used to collect source obstacles.
The follow-up source fix now rejects a candidate unless its complete transformed XZ footprint
plus the existing 0.15 m obstacle margin lies inside the covered query rectangle. The receipt
names this rejection `obstacle_query_bounds`. The new regression
`edge_pocket_cannot_overlap_an_obstacle_outside_the_queried_rectangle` covers both protruding
geometry and protruding clearance with an inside centre. All three dressing tests now pass
against source-20. The independent post-guard `live-camp-v5.log` again records 146 pieces,
27 pushables and all 146 retained after final cover insertion. The older v4 receipt remains
the original source-19 run; its provenance has not been changed.

## Halftrack driving: eight bounded checks passed on build 18

The review uses the actual expedition camp, source transport, current person, ordinary boarding,
drive/brake inputs and exit. Only the person's starting feet/look beside the accessible cab are
staged; one ordinary on-foot neutral update establishes the environment before boarding.
`captures/halftrack_driving_20261030_a.json` records all eight checks passing, with four stills
at the same prefix ending `_board.png`, `_drive.png`, `_brake.png` and `_exit.png`. The run log
is `artifacts/expedition-20261030/halftrack-driving-a.log`.

The transport moved 9.019 m during three seconds of drive, consumed finite fuel, refused exit
while moving, stopped under Space braking despite held drive input, and permitted a source-clear
exit with the same person and unchanged personal ammunition. The source contact registry remained
available. This was not a crash test or collision-accuracy proof. Handling/fuel rates are authored;
the source has no modelled cab, so the camera follows outside. Cargo, towing, crane operation,
vehicle damage/armor authority, animated track belts and full-game disk resume remain unfinished.

## Focused verification filters

Parent should run the project's normal Cargo command with these focused filters:

- `camp_people::tests`: finite identity exclusions and exact transformed triangle picking.
- `camp_people_rig::tests`: original source bind/skin/material contract and planted-foot gait.
- `support_camp::tests`: source mesh/collision correspondence, doorway gaps and medical radius.
- `support_camp_runtime::tests`: site admission; ignored private-asset source contact proof.
- `camp_terrain_pad::tests`: authored grid/detail correspondence and unchanged outside nodes.
- Ignored `medic_and_reserve_routes_use_actual_openings_and_props`: loads the private camp
  assets and tests routes from every initial reserve post and to both beds at two camp angles.

Source-19 focused results are recorded at the top of this handoff; they do not imply every
historical filter above was rerun. Private-asset failures must not be replaced with proxy
anatomy or hidden collision geometry.

## Repair manual: v12 framed and illuminated, eight focused tests passed

The physical manual retains eight spreads, independent turning leaf faces, symptom yes/no
navigation, track diagnosis diagrams and page animation. Complete history invalidation removed
old ink ghosting. The physical holding pose now uses yaw +30 degrees and pitch -25 degrees so
the actual page faces receive the existing cabin lamps. Body type is 0.038 m with 0.040 m line
spacing, darker ink and clearer diagrams; four long sections were shortened without changing
the diagnostic branches or controls. No emissive paper, HUD replacement or added fill light was
used. A whole-book clearance adjustment protects the near plane throughout zoom/leaf turns.

The default holding frame was then raised and moved slightly outward, preserving full covers
and pages inside the reading view. The final review is
`captures/expedition_manual_20261030_v12_framed.json`; `_spread_00.png` visibly includes the
previously clipped lower-left corner. `_zoom_left.png` and `_zoom_right.png` retain the intended
close reading views. The receipt reaches all eight spreads and records consistent first/revisited
page lighting. Source-19 passes all eight manual tests, including actual-vertex default framing,
near-eye clearance through turns, real page picking and text/diagram separation.

This improves the actual lit book, but small type remains dependent on the low-resolution render
and reading zoom. Human readability, native pointer interaction and ordinary playtest usability
still require review. The diagrams are authored game-interaction schematics, not historical
manual facsimiles or traced engineering drawings. Preserve the parent's diagnostic capture and
renderer changes; the capture harness still uses its documented audio-capture environment.

## Effects and monochrome map changes

Effects source changes in `renderer_raymarch.rs`: authoritative heavy BLAST classification,
one shared 60 ms detonation heat law, removal of unsupported 48 stationary glowing residue
points, and ground ejecta tint derived from the actual ground event's snow fraction. Parent
ran paired 18-frame captures without runtime errors; those short timing samples are not an
optimization verdict. `EFFECTS_REVIEW_20261030.md` records precise capture recipe and limits,
including unchanged camera-dependent blood droplet draw scaling.

Monochrome map work uses actual authored building outlines/roads and black markers via
`map_linework.rs` and map render modules. Parent integrated the runtime hook. No fabricated
building rectangles or enemy omniscience markers were introduced.
