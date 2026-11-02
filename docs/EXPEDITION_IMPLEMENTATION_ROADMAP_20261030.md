# Tiger expedition: implementation roadmap

> Evidence status, 2026-10-30 build31: the implemented theatre is 12 km square, with twelve authored sites and two crossings. Thirteen mission-rule checks and the complete eleven-owner world/continuation/paused-paper witness passed in parent-run reviews. This is not completion of every checkbox below: 32.768 km streaming, every CAD component serviceable, full heavy overhaul, arbitrary generated campaigns and extended human acceptance remain proposals or unfinished work. See `EXPEDITION_MISSION_PROGRESS_20261030.md` and `EXPEDITION_WORLD_PERSISTENCE_20261030.md` for exact evidence and limitations.

30 October 2026. Owner-authorized development roadmap. This supersedes the fixed retreat,
single-life commander, and act-retry assumptions in earlier design documents. Checkboxes are
implementation status, not aspirations. A task is complete only when its acceptance checks pass.

Current evidence and known failures are tracked in `EXPEDITION_DEVELOPMENT_LOG_20261030.md`.
The recovery succession test passes on the installed Act One camp; the full expedition is still
in progress. Unticked compound stages below are not automatically complete because one subsystem
has passed. The physical book is now readable in reviewed cabin views and the source camp rig tests
pass; complete mission flow, recovery equipment, animation polish and performance acceptance remain open.

## Active completion pass â€” owner request, 30 October

The owner explicitly requested completion of recovery/transport, the workshop and upgrades,
the continuous theatre and mission progression, full live save/resume, and extended playtests.
This is implementation work, not a planning-only request. Keep the raymarching branch and all
existing private assets. The local review page remains available during work.

1. Stabilize the in-flight physical crew-panel, pictogram, amber-dial and crew-transfer changes.
   Build a named snapshot, run focused checks, inspect footage, and correct visual/input faults.
2. Make recovery a complete physical journey: finite named party and supplies, transport loading,
   retained casualties, evacuation/medical handoff, grounded recovery equipment, repairs, remounting,
   and return to the interrupted order. Preserve a failed party's people and equipment.
3. Make workshop work persistent and physical: source-identified accessible components, compatible
   finite parts, tools, removal/installation/fastening, and a functional test. Field recovery and
   workshop restoration have different equipment requirements. Add real capability tradeoffs.
4. Supply a continuous seeded theatre with connected roads, river crossings, mission settlements,
   contrasting regions and mobile camp sites. Mission objectives arise from actual world state.
   Orders offer resource, personnel and improvement priorities, followed by a final operation.
5. Save the complete supported live simulation with versioned owner snapshots, exact floats,
   source compatibility checks, atomic replacement and backup. Restore into a candidate world
   before replacing the running game. Do not silently substitute fresh missing state.
6. Exercise a complete preparation/recovery/repair/mission/save/finale sequence, interrupted work,
   repeated deaths, exhausted supplies, blocked routes, and loss conditions. Record playable clips
   and performance evidence. Independent human feedback remains a separate external acceptance
   step: agent-driven tests cannot be reported as another person's playtest.

Ownership during this pass: parent handles theatre, mission progression, frontend integration,
serial Cargo/GPU runs and end-to-end playtests; crew agent handles assignment and recovery/transport;
state agent handles complete persistence; workshop agent handles physical repair/upgrade jobs.
Every owner supplies serializable state and explicit integration contracts. No checkbox below is
promoted from source presence alone; recorded runtime acceptance remains required.

## 1. The game we are building

One persistent Tiger, a finite group of named soldiers, and a mobile support camp in a continuous
theatre. Prepare for an ultimate operation while spending people, ammunition, fuel, time, and
equipment on missions and recovery. Death transfers control to a different living soldier;
the battlefield and completed work persist. The Tiger is always recoverable, even after a major
fire. This is an explicit game rule, not a historical claim. People do not resurrect.

### Locked decisions and first tuning values

- Target a successful run of **2â€“4 hours**, with save-and-resume. Recovery-heavy runs may run longer.
- **Eight preparation dispatches** before the final operation. Accepting a mission or launching
  a major recovery consumes one slot. Resolve or withdraw from the current dispatch before another.
  Repeated death/recovery expeditions spend further slots. No slot is spent for reading, repairs,
  ordinary travel, or selecting equipment. Dispatch cost is committed exactly once with its save.
- At zero slots, ordinary preparation assignments close and the finale becomes available. Recovery
  remains available and cannot softlock the run merely because the budget is spent. Missed priorities
  become explicit setbacks to the final operation, not invisible prerequisite locks.
- Default finale retains recovery. A separately labeled test variant disables succession only after
  a clearly communicated final commitment; it is not the initial shipping rule.
- First balance configuration: **16 people total**, five initially aboard and eleven at camp. Names,
  training, wounds, and assignments persist. Recruitable people are a finite seeded pool, never an
  endlessly repeatable reward. Count and timings are tuning data, not buried constants.
- Campaign loss means no living person can eventually continue. Stabilized patients can recover;
  an empty reserve list does not mean defeat while viable survivors remain in the field.
- The controlled person is an identity, not a fixed commander slot. A new commander never inherits
  a corpse's wounds, inventory, or appearance merely because they occupy the same seat.
- Default main-gun firing remains the player's job. A deliberate autonomous-gunner demo is retained
  for comparison, with a visible mode indication and the same physical weapon operation.
- Written words are allowed; spoken dialogue and narration are not. Orders, the map, the repair
  manual, names, radio transcripts, and casualty records communicate purpose.
- A fictional compressed eastern theatre uses winter in the north, open eastern farmland, and
  western German-inspired towns. It is not presented as literal contiguous historical geography.
- Headless development captures; no surprise test windows. Private assets/builds never go to GitHub.

### Inputs to consolidate

| Action | Default |
|---|---|
| Interact, equip, enter, use held tool | LMB; a held weapon owns firing, empty hands own interaction |
| Exit one interaction/view/station layer, stow held item | X |
| Driver cruise, while a capable driver occupies the post | Q down / E up: -3, -2, -1, stop, +1, +2, +3 |
| Rough gun traverse/elevation | Arrow keys through a living assigned gunner; direct at gun sights |
| Fine gun aim and fire | Mouse and LMB in gun sights |
| Sector designation | V; approximate bearing, never perfect hidden target tracking |
| Physical map / manual | M / B |
| Crew selection cursor | Hold Left Alt; LMB person, RMB destination seat/task |
| Handheld light / ammunition type | F / T |
| Explicit autonomous gunner experiment | K, with visible state |
| Pause and controls reference | Escape / Home |

Books and cursor interaction consume gameplay input. X closes only the top layer. Q/E never
double as interaction. Manual driving remains possible from the driver's station when the driver
is dead. Walking inside does not cancel standing driver orders. No command survives a dead or
incapacitated operator as if a ghost were working the controls.

## 2. Implementation order and deliverables

### Stage 0 â€” reconcile the working baseline and protect existing work

- [x] Preserve local design documents and authoring artifacts before integrating the handoff.
- [x] Fast-forward `raymarching` to verified `main` while retaining the branch name and private assets.
- [ ] Record the starting revision and named baseline test failures; do not misreport old failures as new.
- [ ] Amend the written-word principle and outdated lone-crew opening text; show the exact new wording.
- [ ] Keep the previous campaign selectable while the expedition is incomplete. Add a separate save
  format; do not reinterpret or overwrite old act-entrance saves.
- [ ] Establish CPU checks, deterministic headless scenarios, screenshot/video naming, and performance
  receipts. Run one owned GPU process at a time, terminate only its PID.
- [ ] Fix misleading cleared/bypassed reports and the interior-walking driver-order reset.

Acceptance: clean source integration, reproducible baseline, preserved private material, and honest
mode labels distinguishing complete gameplay from experiments.

### Stage 1 â€” persistent expedition state and reliable saving

- [ ] Add a renderer-independent expedition coordinator with stable person, vehicle, item, component,
  site, and mission identities. Separate identity, trained role, assigned post, and physical location.
- [ ] Represent camp roster, people aboard/on foot/in transport/in treatment, permanent deaths,
  wounds, personal equipment, and crew skill. Death events must be idempotent.
- [ ] Represent one persistent Tiger plus support vehicles, their poses, installed assemblies,
  ammunition locations, fuel, fires, damage, scars, stowage, and unfinished repair steps.
- [ ] Persist actual encountered enemy state, objectives, damaged buildings/bridges, abandoned loads,
  casualties, loose parts, supplies, and completed work. Death never regenerates an encounter.
- [ ] Implement dispatch, success, partial success, withdrawal, failure, recovery, succession,
  camp relocation, preparation-budget exhaustion, finale, victory, and expedition-loss transitions.
- [ ] Save atomically at dispatch, death, succession, major transfers, completed repairs, and mission
  resolution; support explicit suspend anywhere at a simulation boundary and periodic autosaves.
- [ ] Serialize RNG state and necessary live timers so loading cannot reroll impacts or reset bleeding.
  Save in-flight projectiles and active physical jobs before claiming mid-combat suspension works.
- [ ] Validate IDs, ownership, quantities, assignments, references, and the unique Tiger invariant.
  Preserve the previous valid save on interrupted writes; maintain a corruption-recovery backup.
- [ ] Keep irreversible event records ordered; load and replay cannot duplicate supplies or dispatches.
- [ ] No offline real-time simulation while the game is closed. Succession selection pauses simulation;
  once control resumes, wounds and threats resume from retained state.

Acceptance: kill a commander, change identity, save/load, and find the same tank, people, enemies,
supplies and partial repair exactly where the preceding character left them.

### Stage 2 â€” responsive controls and embodied crew

- [ ] Implement and document the input table, resolve existing Q/E/X/V conflicts, and update prompts.
- [ ] Add cruise notches using the existing drivetrain speed controller and brakes, including controlled
  reversal. Dead-driver gating, station handover, and overlay behavior have focused tests.
- [ ] Route remote arrows through the living gunner with manual sight control independent of that
  person's survival. Manual commands preempt autonomous aiming without changing ballistic accuracy.
- [ ] Make direct player gunnery default; retain a deliberate comparison setting for gunner AI firing.
- [ ] Add the small crew-position diagram with names/roles, healthy/wounded/dead states, and assignment.
  Use shape/text as well as white/orange/red so color alone is not required.
- [ ] Block seats occupied by bodies until physically cleared. Implement grabbing, supported dragging,
  and carrying assistance; body removal must not delete inventory, wounds, or identity.
- [ ] Connect trained roles to real operations: loading pace, driver control smoothness, MG handling,
  and gunner response/settling. Do not give a trained gunner magically stronger shells.
- [ ] Add crew commands for hold, follow, board, dismount, carry supplies, assist casualty, and take cover.
  A surviving crew continues useful work after the player's death; do not erase the whole team.

Acceptance: a living driver/gunner extends control, losing either meaningfully removes it, and a
replacement restores that role without healing or relocating unrelated people.

### Stage 3 â€” physical manual and first complete mechanical repair

- [ ] Build a physical bound book with front/back covers, paper thickness, normally lit page surfaces,
  page turns, readable close view, bookmarks, index, and persistent last-opened page.
- [ ] Author symptom-led yes/no diagnosis using observations the player can actually make. The book
  must not secretly read a hidden fault and select the right answer for the player.
- [ ] First chapter: no movement â†’ engine response â†’ track inspection â†’ missing/broken link â†’ tools,
  safe access, link replacement, pin connection, tension/function check â†’ recovery if unresolved.
- [ ] Use line drawings from inspected geometry with labels, arrows, tools, access covers and final checks.
  Clearly separate user instructions from historical facsimiles and unverified historical claims.
- [ ] Connect manual steps to actual track geometry and existing track-repair interaction; preserve
  progress when interrupted, when another person takes over, and across saves.
- [ ] Group repetitive fasteners into deliberate tool actions. Retain access order, connections and
  component mass; do not turn dozens of identical screws into mandatory individual clicks.
- [ ] Build discovery/onboarding through a calm initial camp fault and crew demonstration. No large
  tutorial overlay; optional controls reference remains available.

Acceptance: an unfamiliar tester diagnoses and completes one repair from the book without developer
coaching, then completes it faster a second time through learned understanding.

### Stage 4 â€” first recovery operation: the central playable milestone

- [ ] Construct a compact test area with camp, two approaches, one disabled Tiger, cover, and a persistent
  enemy capable of threatening the wreck without continuously seeing every approach.
- [ ] Integrate death-to-successor control at camp, a named casualty/recovery dispatch, loadout selection,
  departure, arrival, casualty extraction, track repair, remounting, and return to the original task.
- [ ] Preserve earlier rescue parties, dropped tools, casualties, ammunition, connected cables, and partial
  repairs. A failed expedition can leave useful progress for the next one.
- [ ] Add player-controlled recovery transport with bounded support crew tasks, not a combat vehicle
  that makes the Tiger unnecessary. Cargo is visible and has capacity/attachment rules.
- [ ] Include a short repair-in-place option and a move-to-cover option. Threats keep their own patrol,
  search, retreat and occupation behavior rather than freezing eternally beside the wreck.
- [ ] Exercise repeated failure with a small test roster: death in Tiger, death on recovery, last
  available survivor, no camp reserves but a living field soldier, and eventual personnel exhaustion.
- [ ] Keep a practice scenario separate from campaign attrition for testing controls and unfamiliar tools.

Acceptance: a complete playable recovery cycle, including a second failed expedition, with no world
reset, inventory duplication, unexplained death, or need for a developer command to continue.

### Stage 5 â€” wounded survivors and the medical tent

- [ ] Separate dead, unconscious, wounded-but-capable, stabilized, recovering, and fit-for-duty states.
- [ ] Retain the existing wound simulation where verified; calibrate bleeding/treatment against gameplay
  legibility and avoid claiming clinical accuracy for placeholder thresholds.
- [ ] Give recoverable casualties visible/audible nonverbal cues and inspectable signs; radio reports
  convey reported urgency, not omniscient exact death countdowns.
- [ ] Add immediate field treatment, assisted extraction, stretchers, transport positions, and a camp
  medical tent staffed from actual people. Treatment consumes tracked supplies.
- [ ] Stabilization buys transport time; reaching camp starts recovery rather than instantly replacing
  the casualty. Reassignment is blocked until the person is capable of the requested task.
- [ ] Simulate wounded people while a recovery is underway, including when away from the camera, so
  evacuation urgency is real. Pause/closed-game time does not advance their injuries.
- [ ] Provide meaningful choices: evacuate people now, spend time making the Tiger mobile, or divide
  the party. Neither a notification nor mission completion silently rescues a person.
- [ ] Add injury presentation, localized blood, clotting/soaking/residue where appropriate, persistent
  bodies and detached parts, and supported ragdoll transitions. Damage effects follow actual injury.

Acceptance: a timely, supplied recovery saves someone who would otherwise bleed out; delayed arrival
has a readable consequence; loading and succession never reset or double-apply a wound.

### Stage 6 â€” source models, physical assembly, tools, and advanced repairs

- [ ] Inventory the existing separated Tiger CAD assembly; assign durable part IDs, semantic names,
  scale, pivots, material, mass, attachment points, access dependencies and collision representation.
- [ ] Preserve source objects and their hierarchy. Separate runtime detail from authoring masters.
  Existing separate meshes do not prove that pistons/gearing or every hidden mechanism is modeled.
- [ ] Prepare only the next playable assemblies first: tracks, access panels, optics, turret drive,
  engine services, cooling, transmission, gun/recoil, electrical/radio, then major hull/turret work.
- [ ] Use shared part/connection/job definitions for inspection, removal, installation, tool use,
  crew assistance, diagrams, persistence and physical behavior.
- [ ] Implement wrench, welder, powered workshop tool, jack, winch, hoist, fuel hose and track tools
  with correct contact/connection requirements, distinct sound and visible progress.
- [ ] Parts remain physical objects after removal. Prevent clipping a large assembly through unopened
  covers, lifting beyond equipment capacity, or installing incompatible mounts.
- [ ] Implement fault-specific work: obstruction removal, hose/connection repair, replacement optics,
  cooling service, drive repair, assembly swaps, and armor patching. Welding is not a universal heal.
- [ ] Distinguish a field patch from a restored original plate. Retain local armor damage and mass;
  added material affects the actual vehicle rather than a generic defense percentage.
- [ ] Heavy engine/transmission/turret replacement uses suitable transport, rigging, access and lifting
  equipment; separate turret transport from a small cargo bed. Verify source procedures before authoring.
- [ ] Salvage compatible parts from donor wrecks with intact provenance/condition. Keep fictional
  upgrades and historical production changes labeled separately in development records.
- [ ] Add physical optics/cooling/stowage/protection and weapon-service choices with costs and tradeoffs;
  reject arbitrary damage bonuses or enemy armor that scales invisibly with player upgrades.
- [ ] Add track tension/sag and broken-track behavior tied to geometry and support. Validate energy,
  traction, detached links, wheels and collision; improve fidelity without exploding per-link cost.
- [ ] Persist soot, fire damage, oil/coolant/fuel traces, wear and scars as inspectable state. They must
  be useful symptoms or believable history, not random decoration that contradicts a fault.

Acceptance: every supported failure has a readable symptom, a real cause, a documented access path,
an achievable repair/recovery path, and a functional check; saved assemblies reload without drift.

### Stage 7 â€” halftrack, heavy recovery, and asset admission

- [ ] Search existing documented private asset sources before purchasing or downloading anything.
- [ ] Find a suitable heavy halftrack and lifting-equipment model; verify license, attribution,
  provenance, scale, topology, textures, separate moving parts and permission for game distribution.
  Do not download an unlicensed model or spend money without specific authorization.
- [ ] Import permitted assets into the private library, retain receipts/source licenses, pack textures,
  produce normalized Blender masters and runtime assets, and record source-to-runtime mappings.
- [ ] Implement driving, braking, boarding, seats, crew and cargo contact, fuel, damage, loading and
  unloading, and a clear inspection view. Validate actual carrying/towing/lifting capacities.
- [ ] Implement tow hooks, cable attachment/detachment, slack/tension, winch control, grounded loads,
  slope/traction limits and recovery of a disabled support vehicle.
- [ ] Add a matching crane/gantry and trailer for heavy assembly handling. Use supporting tractors or
  prepared hauling equipment when required; do not silently give a light carrier Tiger-towing power.
- [ ] Retain damaged support vehicles and their cargo after failed expeditions. Critical tools have
  a discoverable recovery/reconditioning path, rather than disappearing with the driver.
- [ ] Build the main loop with one controllable recovery-vehicle family before expanding civilian cars
  or other vehicles. Additional player fighting tanks remain outside the initial expedition design.

Acceptance: load people and parts, drive out, connect a physical tow, move the Tiger into cover,
unload, repair, and recover the transporter; no teleporting loads or invisible towing force.

### Stage 8 â€” camp, bodies, locomotion, and animation

- [ ] Author a forward support camp with living quarters, cooking/rest area, orders point, medical tent,
  workshop, parts storage, fuel point, ammunition storage, loading area and recovery parking.
- [ ] Make every reserve a visible named person. Close hover shows rank, surname, training, assignment
  and relevant availability. No extra anonymous workers that secretly duplicate the finite roster.
- [ ] Schedule plausible work/rest loops using actual jobs and objects: carry ammunition, inspect,
  service equipment, treat patients, eat, rest, maintain the camp and prepare transport.
- [ ] Selection/assignment produces a brief context-appropriate acknowledgment/salute when safe,
  followed by packing and boarding. Avoid forced saluting while carrying a patient or under fire.
- [ ] Add humanoid collision derived from the character rig, avoidance, yielding, shoulder/body turning,
  crouching and squeeze motion in the interior. No walking straight through crew.
- [ ] Build/retarget locomotion, starts/stops, turning, seated operation, reach/grasp, two-hand carry,
  dragging, lifting, climbing, saluting, treatment and tool-use animations with foot/hand contacts.
- [ ] Add joint limits, pelvis/spine balance, planted feet, handholds, obstacle-aware reach and correct
  transitions into ragdoll/injury poses. Test differently sized people and bulky equipment.
- [ ] Crew negotiate a blocked aisle and yield to a passing player where space exists; when it does
  not, provide a clear physical obstruction, never hidden penetration or an inescapable crowd jam.
- [ ] Relocate camp to reachable friendly sites after strategic outcomes; transfer the same people,
  patients, equipment and stores. No free resupply, healed patients, or duplicated quarters inventory.

Acceptance: assign a named reserve, watch them collect the selected load and board, pass crew in the
Tiger without clipping, evacuate a casualty, and relocate camp while retaining all identities/items.

### Stage 9 â€” fuel, ammunition, stowage, and supply missions

- [ ] Unify inventory quantities with actual containers, rounds, racks, carts, tanks and cargo slots.
- [ ] Implement pushable ammunition carts at depots and loading beside a stationary Tiger. A capable
  assigned loader transfers real rounds into reachable compatible rack spaces without duplication.
- [ ] Without a loader, allow deliberate manual loading with continuous held assistance for repetitive
  handling; still show and account for each round, with interruptions preserving progress.
- [ ] Make the loader pause when threatened, obstructed, incapacitated or reassigned. Prevent loading
  through a closed hatch or into a full rack; show understandable blocked reasons.
- [ ] Implement fuel containers, fuel transfer connections, tank capacity, consumption, leaks and spills.
  Refueling is a physical task with tracked amounts, not a free mission-complete refill.
- [ ] Add exterior racks and mounted storage with validated attachment locations, clearance, mass,
  vulnerability and access. Essential controls/hatches cannot be permanently buried by placement.
- [ ] Add finite depot, fuel convoy, cache, workshop and salvage opportunities; rewards are located
  physical supplies. Seeds must not require an unavailable unique consumable to continue.

Acceptance: loading, unloading, spilling, firing, salvaging and saving conserve quantities; a lost
transport retains its cargo; acquiring supplies changes what the next expedition can attempt.

### Stage 10 â€” connected theatre and physical atlas

- [ ] Build a deterministic **32.768 km square** theatre, using **2.048 km cells**, seeded terrain and
  authored reusable locations. Prototype only a compact district first; scale after recovery is fun.
- [ ] Use global cell coordinates plus local precision; stream geometry and cache state without
  changing damage/physics rules between graphics presets. Preserve long-range sight and shots.
- [ ] Generate connected roads, drainage/rivers, crossings, plausible slopes, terrain cover and three
  regional palettes. Roads, visible buildings, collision, AI paths and paper maps share one manifest.
- [ ] Place an initial catalogue of **48 sites**, including villages, bridges, farms, depots, workshops,
  camps, observation points and final-town approaches. Choose a compact connected subset per run.
- [ ] Validate routes for Tiger/transport width, bridge clearance, bank geometry, tow turns, alternate
  approaches and foot access; do not rely on visual roads that vehicles cannot actually traverse.
- [ ] Replace color cartography with monochrome roads, building outlines, water boundaries, contours,
  symbols and labels. Add atlas page index, adjacent-page references, zoom and pencil annotation.
- [ ] Add the owner's requested current-position marker and mission-start/location marker. The
  positional assistance is explicit; avoid silently retaining old dead-reckoning error behavior.
- [ ] Represent known versus reported threats and updated frontline information without omniscient
  enemy dots. Include camp, casualty, abandoned equipment and recovery positions when known.
- [ ] Keep normal assignments geographically close enough for the 2â€“4 hour target; a huge map does
  not require empty cross-map driving. Use the larger theatre for route/run variation.

Acceptance: travel across cells and save/load without shifting vehicles, wrecks, maps, or damage;
follow a hand-drawn route to an objective and choose a different viable approach on another run.

### Stage 11 â€” mission director, frontline, and narrative

- [ ] Replace linearly triggered clear-the-route beats with mission sites, purposes, prerequisites,
  physical completion conditions, partial outcomes, withdrawal conditions and persistent consequences.
- [ ] Deliver written orders at the radio station and camp with purpose, location, reported opposition,
  expected duration, strategic stakes and possible supplies. Keep an accessible message history.
- [ ] Offer a bounded choice of priority/supply/personnel missions. Local movement is free; dispatching
  commits its operation cost. Never charge a slot merely for inspecting an offer.
- [ ] Build attack-and-hold village, crossing defence, delaying withdrawal, crew rescue, fuel/ammunition
  seizure, recovery escort, workshop capture, and reconnaissance/reopen-road templates.
- [ ] Credit what physically happened: held, extracted, escorted, observed, bypassed, withdrawn, failed;
  touching a marker or killing unrelated enemies cannot substitute for the actual objective.
- [ ] Advance the front through site ownership and connected supply access; move camp when necessary
  with readable notice and valid access. Mission failure changes future opportunities and final defences.
- [ ] Add enemy patrol, search, reinforcement, occupation and withdrawal appropriate to each objective.
  Use finite authored forces, local perception, and meaningful reactions to smoke, sound and casualties.
- [ ] Establish the ultimate operation early through orders/maps. Start outside the Tiger at camp with
  a crew, personal possessions and a manageable preparation task; show what this unit is trying to do.
- [ ] Carry narrative through recurring people, signed reports, camp changes, rescued allies, belongings,
  letters/records used sparingly and sourced environmental scenes. No spoken exposition.
- [ ] Record historical provenance and fictional adaptations. Avoid mythic kill-count framing or
  pretending all historical situations happened to this one fictional crew.

Acceptance: two runs with different choices produce different routes, reserves, supply positions,
frontlines and final conditions, without hiding why a mission succeeded or failed.

### Stage 12 â€” final operation

- [ ] Implement a final town/crossing operation: open an approach, secure the crossing, hold for a
  friendly column, then extract the surviving expedition. It uses ordinary combat/damage rules.
- [ ] Earlier reconnaissance, engineers, depots, workshop access and losses alter routes, support,
  supplies and enemy positions. No invisible stat scaling to negate the player's preparation.
- [ ] Retain default recovery and persistent progress between deaths. The final column and objective
  phases have explicit physical success/failure conditions and readable local urgency.
- [ ] Build the optional no-recovery-after-commitment test variant separately, with advance warning;
  compare tension, perceived fairness and replay cost before changing the default.
- [ ] Finish with a record of the actual survivors, casualties, Tiger condition and completed mission;
  do not overwrite the ending with a generic scripted crew or pristine tank.

Acceptance: both a prepared and a battered expedition can reach the finale, preparation materially
helps, and wins/losses follow visible mission conditions rather than surprise rule changes.

### Stage 13 â€” damage, effects, sound, visual refinement and performance

- [ ] Audit armor penetration, residual projectile, armor debris and shell explosive effects separately;
  log their energy and collision authority. Stop treating every penetration as an identical outcome.
- [ ] Add energy loss/occlusion through actual bodies and components where supported and validated.
  Remove unconditional all-crew death/all-ammunition detonation shortcuts before calling them realistic.
- [ ] Keep an irreparably killed person distinct from a repairable machine. The always-recoverable hull
  is a campaign concession; it must not secretly immunize occupants from real damage events.
- [ ] Refine explosion source, flash, gas/smoke evolution, debris, sparks, pressure response, lighting,
  occlusion, distance sound and residue. Match effect scale to actual event, not a universal fireball.
- [ ] Improve wound/death visuals and anatomical motion using damage location and retained bodies;
  do not add unrelated gore merely to every impact. Provide an effects-intensity option without
  changing collision, medical consequences or damage simulation.
- [ ] Profile after each playable addition: CPU update segments, GPU pass times, frame-time tails,
  streaming stutter, memory, physics and animation cost. Compare the same seed/view/workload.
- [ ] Capture regular collision-safe screenshots and short videos in the user-designated flat captures
  locations; keep reference/source media separate. Describe what each capture proves and what it does not.
- [ ] Optimize visibility, caching, shared geometry, acceleration structures, animation evaluation and
  sampling; do not change world truth by graphics preset or hide missing physics with visual effects.
- [ ] Keep FEA/slow-motion penetration as a later offline research experiment. It must not delay the
  recovery loop or be described as an implemented/validated live solver.

Acceptance: consistent physical outcomes at supported presets, no unexplained frame-time regression,
readable damage under normal camera conditions, and effects that reflect their underlying events.

## 3. Interfaces and persistence boundaries

- `Expedition` owns the strategic roster, vehicles, dispatches, camp, supplies, losses and campaign
  status. Commands validate then mutate; event IDs make casualty, transfer and dispatch application
  idempotent. The renderer and UI are consumers, never alternate authorities.
- `ExpeditionSave` owns an independent versioned live-state format, compatibility checks and atomic
  installation. Existing campaign checkpoint saves remain separate and loadable in their old mode.
- `Game` exports/imports live tactical state by stable ID: identity/assignment, pose, wounds, exact
  faults, installed parts, fires, ammo, fuel, enemy state and physical jobs. Generic health percentages
  cannot silently replace detailed fault/crew records.
- Shared definitions connect `Component`, `Connection`, `Fault`, `Tool`, `RepairJob`, `CargoSlot`,
  `MissionSite`, and `MissionOutcome` to simulation, authored geometry, manual diagrams and persistence.
- `WorldManifest` and cell deltas provide one source for terrain/site coordinates, collision/navigation,
  streamed rendering, mission anchors and the monochrome atlas.
- Keep data and new subsystems in focused modules; do not put the entire campaign expansion into
  the already-large input/game files. Use explicit adapters at the old/new mode boundary.

## 4. Verification and release gates

### Automated invariants

- Permanent deaths; no double casualty; successor selection respects identity and actual location.
- Empty camp plus living field survivor; stabilized last patient; truly exhausted roster.
- One Tiger across death/load/relocation; recoverable wreck; no accidental replacement spawn.
- Exact inventory accounting through every load/unload/use/death/save path.
- Half-finished repair remains unfinished; disconnected parts do not regain functionality on load.
- Wounds/fire/projectiles and RNG retain authoritative state through suspend and cell transitions.
- Operation cost once only; withdrawal consumes a committed dispatch; finale/recovery at zero budget.
- Mission completion reflects world facts; failed prerequisites become explicit final-operation setbacks.
- Driver/gunner casualty gates, no stale input, no pickup-fire double action, no body/cargo clipping.
- Invalid/corrupt save cannot overwrite a valid save; old campaign saves are not consumed by expedition.

### Playable gates, in order

1. Baseline controls: direct driving/gunnery, interior walking, equipment and station transitions.
2. Book + track: first-time diagnosis, interruption, successful functional test.
3. Recovery: death, successor, persistent threat, repair/remount, second recovery, final-person loss.
4. Medical recovery: stabilize, load, transport, tent treatment, return to duty or permanent casualty.
5. Three linked operations: fuel/ammo shortage, camp relocation, crew replacement and one upgrade.
6. Full short run: all eight preparation slots, final operation, save/resume, win and loss.

Record time spent navigating, fighting, diagnosing, handling repeated objects, recovering and
waiting; count misclicks, control confusion, unrecoverable states and unexplained deaths. Test short
sequences after each change and adjust obvious friction immediately. Use human feedback to evaluate
fun: headless assertions and captures establish behavior/appearance, not enjoyment.

### Integration and reporting

- Commit coherent verified increments. Integrate to main only after required checks pass; push only
  verified main. Keep the active development checkout on the user-requested raymarching branch.
- Do not publish assets, textures, licensed models, captured media, archives or packaged builds.
- Keep this checklist and a development log current with completed checks, known failures and exact
  capture paths. Never promote a pure state-model test into a claim that the playable feature exists.
- The first delivery target is Stages 0â€“4. Full model preparation, a bustling camp, major lifts and
  theatre-scale content follow the successful recovery loop, not the other way around.

## 5. Explicit boundaries

No LAN work in this stage. No engine replacement. No purchased-provider generation without approval.
No claim that all CAD internals already exist. No hidden invulnerability for recovery parties. No
per-frame full-theatre physics requirement: strategic scheduling and local physical simulation have
explicit handover boundaries, and active threats/casualties must retain consequential state. No
requirement to make every bolt an individual click. No automatic claim of anatomical, medical,
ballistic or historical accuracy merely because a model or equation looks detailed.

This is a substantial staged build. Every listed feature remains unchecked until implemented and
verified; the execution log must make partial work and remaining dependencies visible.
