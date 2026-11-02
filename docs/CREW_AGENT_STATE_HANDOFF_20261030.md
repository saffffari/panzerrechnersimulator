# Crew controls, identity, bodies and medical integration handoff

30 October 2026. Shared checkout remains `raymarching`; this agent performed no Git operations,
Cargo builds, GPU runs or visible game launches. Parent owns compilation and visual verification.
Parent agent was reported errored during remote compaction/connection while the final camp work
was in progress. These notes preserve the current compile checkpoint, not a completion claim.

## Resumed continuation: living relief capacity

Parent resumed and authorized the live-patient slot fix. `SingleplayerCrew` now keeps original
source appearance per actor and can restore an existing actor into a vacant slot without
incrementing the finite admission count. `CrewSolo` unbound records hold living evacuees as
well as dead people; they advance under the field clock unless bed-owned. A living incumbent
may be relieved only after their actual retained body is world-space; controlled living people
cannot be overwritten. Health, death and wound callbacks resolve actor IDs rather than a
former slot. The expedition agent extended sync, bed imports and succession over all bindings.

New APIs: `singleplayer_actor_for_person`, `singleplayer_actor_bindings`,
`singleplayer_slot_for_actor`, `singleplayer_actor_casualty`,
`singleplayer_actor_casualty_mut`, `import_singleplayer_actor_casualty`. Existing-person takeover
can restore an archived capable survivor into the dead controlled slot. Original atlas/PBR
channels transfer to live slots through `Renderer::copy_actor_source_material`, restricted to
raster-only crew. Corpse prototypes also retain actor-owned source appearance. No new GPU
geometry or fallback material is fabricated. Root owns compilation/visual validation.

## Implemented and integrated

- Solo X exit/stow, LMB interaction, E/Q seven-notch capable-driver cruise, remote gun arrows
  through a capable gunner, manual sight controls, V approximate bearing, default hold fire,
  explicit K autonomous-gunner experiment. Walking inside preserves standing drive orders.
  `crew_solo.rs`, narrow `main.rs`/`game.rs`, current section of `docs/CONTROLS.md`.
- Living personal-space contact in `crew_personal_space.rs`: source body triangle BVH,
  same uploaded arm pose, source-sized player capsule, bounded sliding/contact work and receipts.
  Missing source assets do not produce invisible replacement blockers.
- Stable PersonId to monotonic anatomical actor identity in `singleplayer_crew.rs`; five reusable
  appearance slots, finite sixteen-actor admission. `living_crew_wounds.rs` addresses actor IDs.
  Historical physiology and bodies remain separate when a dead slot receives a replacement.
- `crew_corpse_render.rs`: sixteen actor-owned source anatomy allocations, shared source
  prototypes, original atlas appearance/helmet/reviewed bones. Missing source fails visibly.
  LAN/couch retain their existing render and peer identity paths.
- `crew_body_recovery.rs`: exact flesh-triangle picking, assisted extraction through a real open
  hatch, persistent world-space articulated bodies, terrain/Tiger support. On-foot deaths use
  the actual controller position rather than spawning another body at the former seat.
- Historical actor-based seat obstruction and body clearing. A fresh on-foot commander can be
  bound while the old body still obstructs its station. Existing living-person takeover changes
  control without replacing the person or resetting their physiology.
- Living incapacitated extraction reuses the same body path without KIA. It excludes the old
  standing mesh and crew duty, retains inside physical pose until extraction, and removes the
  actor from cabin damage/fire after becoming a world body. Field bleeding continues.
- Medical body APIs integrate with `expedition_medical_runtime.rs`: physical bed placement,
  held actor lookup, camp-only physiology authority, same-person reembodiment. Small-arms hits
  on the actual detached world body interrupt bedside ownership before applying new injury.
- Corrected the touched small-arms path to record nonfatal crew physiology and to avoid killing
  the vehicle when only the controlled person dies. Parent's `soldier_life.rs` owns the broader
  person-versus-vehicle liveness split.

## Public adapter contracts

`singleplayer_actor_id`, `singleplayer_person_id`, `singleplayer_person_for_actor`,
`bind_singleplayer_person`, `replace_singleplayer_person`, `replace_controlled_person_on_foot`,
`take_control_existing_singleplayer_person` are in `singleplayer_crew.rs`.

`interact_crew_body`, `release_crew_body`, `crew_body_held_actor`,
`place_detached_crew_body(actor, bed_pose)`,
`reembody_singleplayer_person(person) -> Result<Option<Vec3>>`,
`debug_controlled_casualty`, `crew_body_recovery_receipt` are in `crew_body_recovery.rs`.
Bed translation means intended body center of mass; rotation means pelvis orientation. Relative
joint state and missing tissue are retained. Reembody rejects dead/incapable people and deletes
only the selected living actor's detached pose; returned point is actual body X/Z and support Y.

Parent's `lose_controlled_soldier` calls `retain_controlled_corpse`; confirmed in source.
Camp agent owns `expedition_camp_owns_actor`, `expedition_camp_owns_casualty` and
`expedition_release_medical_actor`. Main must route bed admission before generic body pickup.

## Camp skinning contribution

`camp_people_rig.rs` is delivered to the camp actor agent, who owns registration and
`camp_people.rs`. It imports all 39 original GLB joints, both source weight sets, inverse binds
and node transforms; verifies every flattened vertex (914) and the 564-triangle topology.
`Rig::load`, `mesh`, `neutral`, `pose(phase, moving, salute_weight, work_weight)` and
`STRIDE_M = 0.8` are available. The model faces local +Z. Neutral preserves the source T-pose;
ordinary idle lowers the arms. Walking uses bounded two-link IK and flat ankle orientation.
No shared arm-animation or GLTF-loader implementation was changed for this module.

## Verification checkpoint

Parent previously reported actor identity 7, living wound 4, crew control 10 and personal-space
6 tests passing, followed by a successful release snapshot04. Later work above needs a fresh
compile. A snapshot05 namespace error was fixed (`super::surface_fire`, not `crate::surface_fire`).
The newest targeted filters are:

- `singleplayer_crew::tests` — now 9, including finite registry admission and restored identity/appearance.
- `living_crew_wounds::tests` — 4.
- `crew_solo::tests` — 12, including living archive/restore and failed restore preserving physiology.
- `crew_personal_space::tests` — 6.
- `crew_body_recovery::tests` — 3: source aperture, world-detachment invariance, bed placement.
- `crew_corpse_render::tests` — 1 bounded actor-page mapping.
- `small_arms_runtime::tests` — rerun after the exposed-body/physiology changes.
- `camp_people_rig::tests` — 2 source-skin/ground and matched-root stance tests.

Existing parent harnesses are `solo-controls-review` and `expedition-live-review`. The latter
stages deaths and succession, not wound evacuation, actual hostile recovery, or medical admission.
No new source/GPU proof is claimed here. See `PHYSICAL_TRUTH_AUDIT.md` for explicit approximations.

## Remaining constraints

- Five station/controller slots remain, while up to sixteen real admitted actor records may
  exist across slots and world-space patients/bodies. Relief is an explicit adapter operation,
  not automatic boarding or transport. New relief/takeover paths still require root runtime proof.
- Extraction is assisted whole-body movement, not full limb/hatch collision. Exterior settling
  uses existing segment bounds and sampled terrain/Tiger surfaces, not all buildings or bodies.
- Detached world people have small-arms exposure, but cannon/blast splash, free-world fire and
  blood transport remain incomplete. No follower AI is claimed. Living patients do not receive
  fabricated corpse gore pools.
- Source camp gait lacks terrain/slope adaptation and smooth start/stop locomotion blending.
  Work/salute targets are authored gestures, not guaranteed tool contact. Camp caller must use
  actual travelled distance for phase and the same skinned vertices for render/contact queries.
- Camp-agent static review raised hard-coded medic yaw under rotated camps and `advance(0)`
  converting a walking pose to idle; that agent owns the fixes.


## Source skin frame correction after resumed tests

The source camp gait test exposed that the static GLB loader applies the mesh node's rotated/scaled frame, whereas this skin's jointWorld × inverseBind is identity at rest. Rig admission now verifies the static-loader vertex order and then undoes each vertex's exact mesh-node frame, including inverse normal/tangent transformation, before skinning. Raw source POSITION and source joint pivots consequently share the same metre-scale frame. No inferred rescale, ground lift, or looser floor tolerance was introduced. Root must rerun the original source ground test and review the resulting camp actors before claiming validation.


## Manual rendering diagnosis / first-frame ray-query ordering (2026-10-30)

The complete v7 G-buffer proves all page albedo is intact. v6/v7 original spread00 is bright only on its first visit; returning to identical spread00 with lamps on is dark. The previously cited G0 .08 sample is an actual printed glyph at projected source UV (.4905,.3347), whereas the same point on page00 is blank .86 paper. Do not treat that sample as a material failure.

Renderer::rt_update now records the scene TLAS allocation/build before calling Backend::bind_scene_ray_queries. Previously Backend::prepare selected the empty TLAS before the first actual allocation; the same ordering could bind a retired TLAS on capacity growth. Scene ray-query binding now follows actual allocation in all existing live/optic/headless callers. No lamp power, book pose, material, exposure or transport coefficient changed. Manual capture writes first/revisited original-page HDR means and checks ratio <=2 (old measured ratio ~45), preserving a diagnostic JSON before reporting failure. Root must compile and run this end-to-end regression; it is not yet claimed fixed by capture.

Separately, weak night surfaces receive a sixfold cool rod-display contribution in crush.wgsl/observer.rs. docs/RAYMARCHING_ENGINE.md already records the resulting colorless hull views. Existing KESSEL_ROD_DISPLAY_GAIN override can isolate that reproduction stage without changing HDR. Root owns any further lighting/display decisions; no coefficient change here.

## Crew posture / real loader sequence checkpoint (2026-10-30, pending root build)

Root measured the TLAS regression at ratio 1.0043 in v8: first/revisited page initialization is
now consistent. Root subsequently owns warm practical/display changes and physical book tilt.

The loader's old ordinary path deliberately disabled skinning after a 17–54 cm grip error witness,
and the AI service path performed timed ejection/ram strokes without a visible held cartridge.
New `crew_loader_work.rs` (child of crew_solo) owns a separate physical AI cartridge, with stable
source ammunition identity, visible source rack mesh, actual damage/fire inventory and interruption
drop into the existing floor-body pool. Phases are open, extract, fetch, reach, carry, ram, close.
It leaves the player's carried objects and LAN service path alone. The exact standing binding export
is assigned to the orders agent; it gates the new loader behavior. Without it, the legacy service
remains available and the standing figure isn't guessed.

`crew_balance.rs` responds to the actual suspension's linear/angular acceleration and one actual
shot's projectile-momentum impulse. No idle sine bob. Original bone pivots and weights drive torso
lean, constrained two-link limbs, planted boot pivots, and bounded head stabilization. Rendered
vertices, helmet, contact geometry and anatomical centres share the same sampled pose at 30 Hz.
Root must run `crew_arm_animation::tests`, `crew_arm_animation::balance::tests`, and
`crew_solo::loader_work::tests`, then visual/contact-distance verification. `crew_arm_pose_receipt`
includes `loader_work`, source availability, hand error and body offsets.

Limits: the loader currently selects accessible left-side racks, and moves within the existing
basket/breech envelope. Whole-body source contact planning, swept cartridge/steel contact, exact
finger grips and terrain-adaptive feet are not claimed. Free-hand bracing uses real source-surface
contacts but is conditional on reach. Gait is authored IK driven by actual travelled distance.
Do not call this visually verified before the new source binding and GPU witness pass.

Corpse audit: the current solo actor-owned renderer has no time/distance fade, and five live slot
reuse does not delete old actors. The blanket ragdoll clear is a fresh-game reset. The sole explicit
in-play ragdoll removal is guarded living-patient reembodiment. Existing finite corpse pages render
retained anatomy until reset; destructible anatomical regions can lose flesh but retain the existing
bone path. Root's old videos may predate these actor-owned pages; current death/retention captures
are still required to distinguish a visual occlusion/fall from a missing spawn.


### Loader v1 witness and source-contact follow-up (2026-10-30, build17 pending)

The build16 offscreen witness `captures/crew_loader_balance_20261030_v1_manifest.json`
passed five state assertions: one ordinary shot, one rack cartridge consumed, live ammunition
delta -1, reloaded breech, and surviving vehicle. Images at 6 and 9 seconds show a standing
loader moving a real cartridge and returning to his periscope. The camera is crowded by other
crew, so the clip is not a polished-animation acceptance result. The receipt exposed a real
9.4 cm miss for both hands at 5 seconds during low-rack carry; extraction, later carry, and
ram samples had zero reported hand error. Acceleration/braking generated bounded motion
(about 2.8 cm fore/aft maximum in the one-second samples), settling to near zero at rest.

The next source checkpoint replaces the arbitrary 35 cm crouch cap with the intersection of
both original arm reach spheres and a vertical pelvis travel bounded by original leg lengths.
Unreachable rack contacts are rejected, and cartridge movement/completion pauses rather than
extending arms. Spent-case stow is now owned by the active loader, has the same visible case as
its hand target, and drops that case when interrupted. An inaccessible rear-shelf path is
abandoned after six seconds with the case released at its last pose. The original manual/LAN
stow path remains. Step direction now follows actual travel relative to the body, including
sideways/backwards travel. Two focused source-rig tests were added; root owns compilation and
execution. These follow-up changes have only been parsed, not yet GPU-verified.

Remaining limits: authored IK contact points are palms, not articulated fingers. No complete
whole-body obstacle/cloth/self-collision solver exists. Root travel still uses the existing
conservative basket/breech envelope, turning in place is not a contact-planned pivot, and
reach/phase changes need further animation blending. These are exposed limits, not claims of
photorealistic motion. Current corpse registries have no automatic time/distance disappearance;
root's current KIA state review passed, but its very dark image does not prove visual corpse
persistence. Old video observations cannot yet be attributed to a particular failure.


Build18 source checkpoint correction: a CPU-only exact-source audit rejected the first
vertical-only crouch filter because it admitted no normal racks under the old floor envelope.
That incomplete checkpoint was not accepted as gameplay proof. The corrected solver adds
bounded forward flex about the original Spine_1 joint (0..0.5 rad) and chooses the matching two
brass grips, then solves the same original-length arm reach and planted-foot crouch interval.
The independent numeric audit admits actual source IDs 41, 47, 50, 53 and 62 at zero traverse;
source50 uses 0.479 m crouch / 0.25 rad waist flex, source53 0.445 m / 0.10 rad. A new actual
SHELL_SLOTS invariant verifies these five contacts using the source rig. This checkpoint still
awaits root's compiled tests and GPU witness. Hand reassignment and changing waist targets are
bounded kinematics; smooth regrasp/finger motion remains unfinished.


### Build18 loader v2 GPU witness reviewed (2026-10-30)

Evidence: `captures/crew_loader_balance_20261030_v2_manifest.json` and matching `.mp4`.
Five outcome checks passed: one shot, rack 64 -> 63, all represented live ammunition
161 -> 160, loaded breech, living tank. The load cycle completed by the 8-second receipt.
Case index 0 was explicitly owned for stow at 2 and 3 seconds. The 5-second low-rack carry
reported 0.3243 m crouch and zero hand error, replacing v1's 9.4 cm hand miss.

Inspected native-video extractions at 2/5/7/9/13 seconds, saved flat as
`captures/crew_loader_balance_20261030_v2_frame02.png`, `_frame05.png`, `_frame07.png`,
`_frame09.png` and `_frame13.png`. At 2 seconds both arms accompany the case; at 5 seconds
the loader crouches toward the low rack, with the hands substantially hidden by foreground
crew; at 7 seconds both arms extend at the breech; at 9 seconds he stands at his periscope;
at 13 seconds one arm bends toward a bracing contact. No obvious severe mesh failure was
visible in these views. The camera does not expose the feet/knees, and foreground body
overlap prevents a clean whole-body clipping verdict. Stills alone do not establish smooth
transitions, planted gait, realistic fingers, or believable turning/regrasp.

The receipt contains 16 active loader-hand observations over 10 one-second samples, all
reported as zero error. This is explicitly NOT a continuous-contact guarantee: the metric
compares each solved endpoint with its stored pose-time target, while source poses are
sampled at up to 30 Hz. It does not independently measure every intervening frame against
the current moving cartridge or test collision with the breech/other crew. Transient lag,
phase-transition pops, and finger/whole-body interference remain unproven. Root will run
the three new ignored private-source CPU tests with `--include-ignored` using build19.
No runtime changes were made during this review.


### Recovery, field personnel and exact owner snapshots — current source checkpoint

Root owns all Cargo, CPU test execution, GPU captures and Git. Check23/check24 reported no
crew/recovery errors; the field-person and gameplay-review follow-up has only passed parsing
and awaits the next whole-project compile. Do not present this section as playtest acceptance.

- `crew_world_state.rs` owns `crew.runtime` v1: stable slots/PersonId bindings and appearance
  donors, all retired physiology, exact Casualty float bits (including never-dosed Infinity),
  procedures, source pose sample vertices/bones and balance clocks, assignment paths, wounds,
  retained bodies/extraction, driver controls and crew status selection. Diagnostic JSON logs
  are serialized as byte strings inside binary records. No save restore resets wounds.
- `halftrack_runtime.rs` owns `transport.recovery` v1: motion/fuel/occupant, tailgate hinge,
  rigging stages/fuel/load counters, finite cargo identities/local transforms and timed movement,
  passengers/routes/source pose inputs, retained patients/local articulated poses and lift stage.
  All mesh pools are allocated at initial Game construction, not on injury or cargo load.
- `crew_assignment.rs` provides the bounded Gunner/Loader source-swept route and atomic
  identity-preserving role swap. Broader hull/commander/player swaps remain refused rather
  than moving through walls. This route still requires its live acceptance capture.
- `recovery_runtime.rs` splits the original transport's 14 rear-panel triangles into an authored
  hinged loading ramp. Same geometry is drawn and collides. Vehicle drive waits until the ramp
  closes; loading/walking people hold it open. The original source has no authored hinge/cab.
- Spare parts transfer by actual ItemId using workshop ownership APIs; one source-sized bay
  owns each rigid local pose. LMB stows a held part over 1.5 seconds and actual source-mesh
  picking unloads it. People and cargo currently use exclusive loading-aisle configurations.
- Living or dead retained bodies load after 1.2 seconds holding beside the stopped open ramp,
  followed by a two-second rigid articulated lift. This is an assisted handling abstraction,
  not a human strength simulation. A patient remains the same actor with uninterrupted field
  physiology until actual medical admission. Unloaded bodies now collide with source carrier
  and gate triangles, as well as the existing terrain/Tiger contacts.
- Four finite reserve passengers can board on a source-rig walk/seat path. Source poses are
  drawn at 1.05 scale; root motion uses bounded body-height ray checks, not continuous full
  mesh self-collision. LMB on an actually visible passenger requests disembark. The same
  PersonId is restored at the actual ramp exit. Initial rescue placement transfers existing
  IDs; it never invents extra personnel (ID16 is the designated medic; use14/15 for rescue).
- The authored recovery equipment is a 180kg wheeled ground-anchor/winch prototype. It rolls
  down the ramp in12 seconds, is pushed at0.45m/s and never occupies a hand-carry pose.
  Drawing/picking/foot contacts use the same authored parts. Fictional engineering ratings:
  60kN line, two-part purchase,0.85 efficiency,110kN anchor,7kW motor,50m total cable,
  1200kg payload,5L initial winch fuel. This is not a historical halftrack tow rating. It only
  rolls a stopped, free-track Tiger along its tracks when slope/load/source collision permit.
  It does not recover a destroyed transmission, locked tracks, flipped tank or detached turret.
- `field_people_runtime.rs` now queries exact current posed camp/passenger source triangles
  and original skin-weight anatomical ownership for small arms/direct cannon/HE exposure.
  First injury registers that existing PersonId once in actor history, imports its exact
  Casualty, retains its source-joint world pose and removes the duplicate camp/passenger
  presentation. It then uses the existing anatomical wound/death solver and retired-actor
  clock. First injury currently removes autonomous camp duty even if the casualty remains
  capable; resuming minor-wounded NPC work is an unimplemented transition. The retained
  eleven-part corpse collision is still the prior approximation. Camp bone sampling can lag
  the visual sample by one30Hz pose interval. No new tissue/penetration accuracy claim.
- The prototype carrier still has no historical armor/engine destruction model. Source
  body contacts do not imply an armored safe room. Passenger/field personnel can be injured;
  enemy independent targeting of reserve NPCs, continuous source collision for hands, cable
  sag/strain/fracture and turret lifting remain unimplemented.

`Game::review_recovery_gameplay(prefix)` is the serial offscreen entry for root's new
`recovery-gameplay-review` command. It writes a JSON receipt and six stills. It labels all
starting-pose, spare-in-hand, explicit source-hit and open-field winch fixtures. Actual normal
interaction/timing/ownership paths determine success, and failed checks remain failed. This
is not projectile-flight, combat-difficulty or continuous-video proof. New focused CPU filters:
`field_registry_tests`, `field_people::tests`, `halftrack_runtime::recovery::tests`, plus the
existing `crew_world_state::tests` binary-state checks. Root must compile and run them.


### Build26 follow-up checkpoint: finite refuelling and carrier mobility damage

Source parse reviewed; this follow-up has **not yet been compiled or run**. Root owns the next
serial build and `recovery-gameplay-review` run. The earlier build26 binary does not contain it.

- Carrier and winch each have a visible authored filler cap with the same geometry used for
  picking. The carrier cap mounts on an actual outward source cab triangle (local centre near
  -0.93,1.373,0.66); the winch cap sits on its authored motor housing. These are disclosed
  prototype additions, not evidence of a historical filler location. Targeting checks the real
  carrier, kit, Tiger and world surfaces for obstruction.
- A held existing jerrycan transfers up to0.8L/s with LMB held, capped at40L carrier /5L winch.
  Releasing LMB, moving aim away, boarding, moving the carrier or an empty can stops transfer.
  The exact destination fuel and active pour target/timer are binary-saved. The can remains the
  same Pickup/PortableItem saved by the complete world owner. No camp refill occurs.
- The can uses an assisted tilted handling pose near the filler. Its spout has not been measured
  against the source and there is no fluid simulation; do not describe this as a fully rigged
  hand/spout contact animation. Added CPU conservation/capacity/pause test and live harness
  carrier/winch fill phases. The harness explicitly stages each destination0.5L below capacity
  and equips an existing can; it neither duplicates nor refills that can.
- Fixed an existing finite-resource bug: `PortableItem::drain_fuel_liters` used to discard a
  positive remainder of0.01L or less. It now retains every positive remainder.
- Authoritative projectile travel queries actual source carrier faces. Hits on front wheel
  parts (10kJ) and running gear parts (25kJ) disable motion; an actual front bonnet hit (500kJ,
  localz>1.2) disables a disclosed **engine-bay proxy**. These are authored gameplay thresholds,
  not calibrated historical damage or FEA. The source has no internal engine. Recent projectile
  identity/part masks prevent counting one flight segment repeatedly and are saved exactly.
- Carrier skin still does not absorb projectile energy or provide calibrated armor. Passengers
  are not in a magical safe room. Carrier mobility damage currently has no repair operation;
  no claim that these flags can be reset by refuelling or camp return. Actual source damage
  does not alter the intact rendered mesh yet.

New focused filter: `carrier_damage_tests`. `halftrack_runtime::recovery::tests` now includes
metered fuel conservation and pause/capacity checks. Recovery-state validation additionally
rejects duplicate bays, conflicting people/cargo, nonfinite cable hooks and overfull fuel.


Build26 recovery review found the original14-triangle tailgate extraction incorrect: it moved
only the inner skin, leaving the outer panel and its reinforcing strips blocking the visible
winch. The corrected bounded source selection moves all78 rear-panel triangles (inner/outer
skins, top and attached latch/strip geometry); no triangle is deleted. CPU source survey proves
that the formerly blocked rear-to-winch line clears with this panel open. An ignored private
source test `complete_source_rear_panel_opens_real_loading_line` asserts the closed/open ray,
exact78-face admission and total triangle conservation. Runtime visual retry is still required.


### Spatial theatre climate checkpoint (source only; next root build pending)

`Renderer::theatre_weather` is an explicit persisted flag set from the actual continuous
TheatreRuntime. It uses unused existing uniform lanes, so GPU buffer sizes do not change.
`weather_density.wgsl` is now the one pure field source assembled into both raster optical
weather and raymarch pipelines. CPU WeatherView uses the corresponding function. Winter weight
is smoothstep(600,2600,-worldZ): full southern clarity beforeZ=-600, full winter afterZ=-2600.
Southern aerosol extinction is0.00012/m; northern density retains the existing winter-front
coefficient and drifting banks. The transition stays fixed on the map while banks advect.
Raymarch theatre mode bypasses its old globally winter spindrift/fog-bank formulation so its
field matches the CPU/raster calculation; the old path is unchanged in legacy acts.

Snow particle identities are thinned at each actual particle world position, not at the camera.
Terrain material mood and terrain snow glitter use the same fixed gradient. Optical sources,
AI recognition and camera rays all sample the geographical field. Global overhead clouds/sky
remain shared; this does not claim three independent weather simulations or seasons. Snow
cover on vehicles, ground friction and physiology have not been expanded into climate models.
The southern aerosol and transition distances are authored gameplay values, not historical
weather reconstruction.

Source parsing passed; shader validation and GPU images remain root-owned and pending. New
CPU filters: `weather_visibility::tests` and `regional_weather_tests`. Root should also run the
existing raster and raymarch shader-validation suites because shared function signatures changed.
No Cargo/test/GPU command has been run by this agent.

## Latest checkpoint: recovery continuation awaiting build28
- Root build27 review now proves actual full-source gate, deployed winch, cable,0.675m Tiger pull
  for5.007mL,44.93kN<102kN, cargo and casualty identity. Fuel can absent because isolated pickup
  vector was cleared; fixed theatre initialization with known preallocated source slot+eight camp cans.
- New children registered under recovery: carrier_field_service.rs (four finite visible service
  packs/toolbox/source-surface procedure) and replacement_boarding.rs (same-person source body route,
  register actor only at arrival, retain actual field casualty if hit during climb).
- Source frozen for build28; no Cargo/GPU by this agent. Rustfmt parse check passed all changed files.
- Boarding runtime proof still required; current paths may stop at source fit conflicts. Source
  whole-body sweeps are continuous per frame, but hand-over-hand pose, fellow-crew clearance and
  cancel/re-route UX are not finished. Do not bypass physical contacts to claim a working route.
- Planned post-freeze fixes: transport.recovery owner version2 (coordinate expedition agent);
  arrival preflight before actor allocation if a seat gets occupied mid-route; restored-boarder
  source pose presentation; carrier source-service proof; integrate boarding witness.

## Checkpoint29 follow-up (source frozen for root)
- `Game::run_replacement_boarding_review(prefix)` registered under recovery; root owns main CLI.
  Receipt has top-level pass and returns nonzero after saving any failed checks. Explicit quiet
  scene/cleared-body/bonnet-hit fixtures are disclosed. It does NOT call whole-world save while
  soldiers are suppressed; only the transport owner is round-tripped during partial service.
- Vertical aperture descent precedes lateral movement to an offset seat. Ground route points
  remain world-fixed, supported hull/turret portions follow actual suspension frame settling.
  Vacancy is checked again before arrival allocates/binds an actor. A source fit conflict still
  stops at its actual coordinates; no contact waiver or body shrinking was added.
- `transport.recovery` export/decode now version2; expedition agent owns REQUIRED_OWNERS update.
  Restored transport presents service packs/rig/cargo-associated pool visibility and reskins
  passenger/boarder source poses without advancing clocks.
- World body pickup now tests actual carrier triangles as well as ordinary cover/Tiger armor.
  Previous recovery review side-wall viewpoint was not sufficient proof; new unload camera is
  behind the open loading ramp. This source correction needs root's next runtime witness.
- Medical audit: unload detaches only carrier ownership; bleeding remains under field physiology.
  Bed admission checks helper2.8m/body1.5m, same actor, and physical source bed before switching
  one time-course authority to camp. The named medic must reach that bed. No automatic unloading,
  remote bed transfer or resurrection is provided.
- New focused CPU filters: passage_blends_keep_one_actor_stature;
  boarding_owner_rejects_completed_cursor_and_changed_stature;
  service_consumes_distinct_finite_packs_and_only_selected_fault.

### Final source audit before root build29
- Corrected zero-time/blocked boarding source-pose upload; no seated-rest visual with standing
  contact geometry. Supported poses preserve source stature through normalized platform rotations.
  Identical frame matrices skip accumulated transforms.
- Conservative swept-body AABB filters original world/carrier triangles. A narrow-obstruction
  test compares exact full-source and filtered contact results. Receipt counts candidate triangles.
- Horizontal travel uses source gait in BOTH rendering and vertex sweeps. Climbing remains an
  assisted source-body translation, with no claimed authored handgrips.
- Only one route advances at once; remaining living crew's actual posed anatomical segments
  block vertex movement. This uses the existing segment OBB approximation, not cloth collision.
- Arrival checks seat AND controlled identity again before registering an actor. Registry update
  is committed only after the same person's physiology is admitted. Existing bodies are retained.
- Additional test: carried_pose_does_not_shrink_after_repeated_support_motion.
- All registered source frozen; root owns build29 and the new dedicated witness. No test or GPU
  execution by this agent. Actual hatch fit and service affordance still require that witness.

### Boarding posture checkpoint after build29 (2026-10-30)

The build29 witness stopped at a fixture panic before evaluating any route: clearing the soldier vector invalidated mixed-combat source indices. The review now preserves the entire array, disables mixed combat, and marks those fixture soldiers inactive. This correction is not evidence of boarding success.

`crew_hatch_posture.rs` now crouches at the original leg joints while keeping the base ankle targets, tilts at the source waist, and raises the forearms using the original two-link lengths. The same 914 weighted source vertices drive rendering and the existing vertex-sweep contact admission; the same joint matrices drive field-person injury anatomy. No stature reduction, hatch enlargement or collision bypass was added. Descent is subdivided into 10 cm route samples with seated posture blending. Tuck is persisted in transport.recovery owner v3 and restored at zero time. The new ignored private-source test checks original limb lengths, ankle targets and unchanged UVs. It still needs root execution.

Receipts now include the actual route target, standing/tuck amount, and source-frame/local/world position of a blocked vertex path. Exact 564-triangle continuous deforming-mesh collision is not claimed: admission still samples moving source vertices, with the existing source-derived anatomical bounds for other cabin occupants. Hand-to-rung constraints and reverse/cancel routing are still absent. Runtime fit must be decided by the next actual source witness.
### Build30 boarding/service result and follow-up

Actual private-source test `hatch_posture_keeps_original_limb_lengths_and_ankle_support` passed, as did four boarding owner/stature/broadphase tests. `replacement_boarding_20261030_b30a.json` shows normal carrier bonnet service passed, preserved partial work across transport owner restore, and consumed exactly one finite pack. It does not show successful replacement boarding: the driver stopped at the actual front hull at local(1.511,1.967,2.497) while crossing diagonally from the lower fender; the loader's rear-deck first step exceeded the existing 1.90m climb reach.

The follow-up routes both roles via the actual front fender, lifts before crossing the front roof, and routes around the raised original loader lid. Aperture centering measures opposite original source surfaces; descent placement centers the actual weighted mesh section through three roof-height planes. These are planned body translations, not expanded holes, scaled people, or contact waivers. The actual source sweep remains the final gate. A downward roof crossing can no longer take the old broad toe-support exemption. Runtime success remains pending the next review.

Camp relocation also now moves the actual loaded patient segments/centers in the same transition, preserves severed world pieces, refreshes cargo and recovery meshes, and defers while a replacement is boarding. Stowed equipment follows the carrier; deployed anchors retain their world coordinates. This fixes the prior one-frame/save discrepancy between relocated transport and retained patient bodies.
Additional source survey: `boarding_measured_apertures.json` measures the driver opening centre at +15.568 mm X from its interaction anchor. The loader opening is an oval approximately 0.553 m across X and 0.385 m fore/aft at the sampled lower roof planes. The loader now descends facing forward so his shoulders use the wide axis, then turns toward the breech below the roof and lowers his arms at the working post. This uses the original opening; its narrower axis is not expanded. The follow-up remains awaiting actual route verification.

### Build34 results and build35 source checkpoint

Root reports build34 test and release compilation passed. `source_cab_side_partition_conserves_geometry` passes with70 original triangles per complete side; the earlier66 count came from a cropped reference survey that omitted four rear-cap faces. Both source drag tests, both finite field-procedure tests and reverse-ramp state continuity passed. These are CPU results, not a successful complete medical journey or boarding proof.

Registered field care now uses one physical source first-aid kit backed by the existing finite crew AidBox. LMB equips the actual bag and selects the actual injured anatomical body; step-specific hold/press/wrap/turn/watch input advances the existing saved procedure, consumes only completed step supplies, and preserves all injuries and blood. X interrupts and drops the source bag. Camp medic supplies remain a separate existing owner. There is no hand/bandage application animation and the field bag inventory remains shared with in-tank first aid.

The cab retains a dead driver's original source anatomy and blocks a new driver until that body is extracted. Original source cab-side geometry hinges open under LMB; this is explicitly authored prototype functionality, not a historical halftrack door claim. Whole-body dragging uses bounded source-vertex sweeps/sliding against real Tiger, carrier, world and terrain. Cabin hatch pulling still uses its older assisted translated articulated pose, not a whole-body force solver. Live cab death/extraction remains unverified.

Build35 source fixes include:
- Safe original-Tiger physical access is separated from the machine's operational latch in mount/dismount, open-roof entry, hatch hover/interaction, tool use and track service. Existing drive/fire gates remain unchanged. Workshop agent owns the related loader hand-service gate.
- Deployed winch now has a12-second reverse loading path to the same carrier, preserving fuel and pull history. Stow is in the real central aisle instead of the side bench; folded spades clear the cargo floor. Python720-step source vertex sweep passed, and the ignored Rust `reverse_winch_route_clears_complete_source_carrier` uses the production edge/contact solver against full original carrier geometry. That Rust geometry test and runtime return remain pending root build35.
- A patient being unloaded uses existing saved Pull.stage2, preventing automatic loading from immediately undoing the same gesture. Explicit release/re-grab permits a new loading gesture. The source body remains level while over the cargo floor, then ordinary swept lowering resumes after clearing the bed. This is assisted translation, not a rescuer strength/force model.
- `Game::run_field_medical_recovery_review(prefix)` produces JSON, stills and continuous12fps video. Only the initial injury/helper/carrier field placement are fixtures. It attempts actual field kit care, physical body loading, a20-40m continuous carrier drive, source-swept unloading/walking, real medical bed admission and named medic finite care. It saves failed phases honestly and returns failure. It has not been compiled or run at this checkpoint.

Full replacement boarding is still blocked. Build32 proved the opening alone, but the actual driver route then met the real gun frame. The user must manually traverse the gun clear; runtime now explains that blocker and never silently moves it. A broader Python full-cabin survey (original hull/turret/gun, source-weighted bones, unchanged1.05 stature) also found lower-cabin hip/boot and loader elbow/breech contacts. Candidate tucked-joint routes remain unregistered research; no source contact bypass, hole enlargement or body shrink is accepted. The existing final loader posture itself needs a collision-safe hand transition around stowage brackets. Recovered, already-registered NPC reboarding must retain the original actor/PersonId/physiology and explicit route body authority; the initial fresh-reserve-only path does not yet complete that case.

Root owns all compilation, tests and GPU. This agent ran only source parsing and independent Python source calculations. Registered sources are frozen for build35; source assets were not changed during root's immutable build34 GPU batch.

### Build36 medical diagnostics and build37 crew-panel checkpoint (2026-10-31)

Root's build35 recovery review passed28 of29 checks, including returning and redeploying the same finite winch. Its only failed check was fuel-can interaction being consumed by the cab-side opening action. Build36 source gives a held finite can's valid, unobscured physical filler contact priority over cab interaction. Runtime verification of that correction remains pending.

The build35 medical journey passed physical kit pickup, first-aid completion, source-body loading, ordinary cab entry, a20–40m drive, fuel expenditure, exact transport-owner restoration and same-actor unloading. Its dressing assertion was incorrect: the actual first required procedure was airway care. Build36 preserves that priority and explicitly performs the next needed procedure before checking the finite dressing. The actual body then stalled near the carrier rear hinge after roughly0.79m of unloading. Build36 adds exact current/target/source-vertex/triangle diagnostics and a useful rearward-drag camera; it does not yet claim to fix the physical obstruction or complete bed admission.

The physical crew instrument now shows two forward hull positions (driver left, radio operator right) and three rear turret positions (gunner left, commander centre, loader right), with tank front at the top. Canonical role indices and actor identities are unchanged. Physical lenses, their exact click boxes, source-conforming pictograms and orthogonal wires all use the same positions. The footprint is approximately11.2×13.8cm on the existing surveyed source wall. No asset, light-power, roster, assignment or serialized-state change was required. The final idle-bus pass hides its redundant half rather than drawing overlapping wire geometry.

Only `src/crew_status_panel.rs` changes the panel. Rustfmt parsing passed; root must execute these focused tests and capture the real instrument before visual claims:
- `crew_panel_two_front_three_rear_match_physical_seats`
- `crew_panel_physical_picking_and_wires_share_seat_coordinates`
- `crew_panel_wire_energy_scales_with_actual_surface_area`
- `crew_panel_mounts_and_paint_contact_the_private_source_wall` with the actual private Tiger source.

For a truthful solo interior combat clip, reuse the ordinary K/perception/cruise loop from `expedition_theatre_review`, keep the actual commander station eye, turn only the head toward the loader, and continuously record a natural shot/reload sequence. The old `crew_solo_capture` and `crew_battle_capture` fixtures inject damage or hold/reset enemies and are not suitable unchanged. Record `crew_solo_receipt`, `crew_arm_pose_receipt` and `crew_loader_work_receipt` alongside the frames; do not infer continuous hand contact from the older one-second samples. This is a capture recommendation, not completed new footage.
