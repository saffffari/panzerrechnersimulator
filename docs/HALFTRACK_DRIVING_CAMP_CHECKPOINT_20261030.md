# Transport and compact camp source checkpoint — 2026-10-30

The optional transport has passed a bounded live-input driving review, and the real installed camp
has passed a bedside-care review. This is not a completed recovery campaign or a blanket visual
approval. The parent owns compilation, serial CPU tests and GPU review; no child Cargo/GPU processes
were launched. The evidence below was read back from the saved receipts and logs.

## Transport

`src/halftrack_runtime.rs` is an optional Game child installed only with the expedition support
camp. It replaces the live camp's baked transport geometry with one independent runtime instance.
The source-only camp inspection can still deliberately include a static transport for layout review.

The reviewed private Polyfork exterior, source parts, textures and pivot frames remain unchanged.
The asset has opaque painted windows and no modeled cab interior. This milestone uses an exterior
follow camera. Boarding is a deliberate prototype transition, not a physical door/seat animation.

Game APIs for the parent frontend:

- `halftrack_occupied()`
- `halftrack_hover(eye, direction)`
- `try_board_halftrack(eye, direction, renderer)`
- `try_exit_halftrack(renderer)`
- `halftrack_receipt()`

The controls are LMB to board the visible nearby cab, W/S to accelerate forward/reverse,
A/D to steer, mouse to orbit the inspection camera, Space to brake and X to exit when stationary.
The input adapter and frontend suppress tank weapons/station channels and unrelated foot-equipment
actions while aboard. The live review below exercises boarding, forward drive, braking and exit;
it does not constitute a manual playthrough of every input combination.

Handling is explicitly authored prototype data: 9 m/s forward, 3 m/s reverse, 3.2 m steering
wheelbase and finite initial 40 L with an approximate consumption law. These are not historical
performance, armored protection or recovery-capacity claims. No cargo, towing, crane, drivetrain
damage, troop/passenger transport, vehicle projectile-material or chassis mass simulation exists.
Rear source wheel pairs remain shared and the track belts remain static.

The same real PersonId occupies the vehicle. The physical Game retains the transport's pose across
succession; position, orientation, velocity, fuel and the person's Aboard location project into
VehicleId 2. Death brakes the transport and does not create a new vehicle or refill its fuel.
Complete live-game disk restore is still unavailable, as before this milestone.

`halftrack_contacts.rs` retains actual source triangles. Bounds are broad-phase rejection only.
Loose world meshes, current building/rubble instances, manifest structures/trees/scatter, moving
door leaves and enemy vehicle pages retain their existing SceneRef; current visibility and models
come from the read-only Renderer accessor. The detailed Tiger supplies its existing articulated
source contacts. Motion uses bounded source-vertex sweeps and source-edge intersections, not
invisible wall boxes. A saturated/missing source query stops motion. Terrain and nearby road/slab
support use the actual heightfield and admitted source surfaces. This remains a conservative
kinematic approximation, with no crushing or rigid-body impulse exchange.

## Compact camp

The prior broad rectangle was caused by grading the full camp footprint. Live installation now
searches for gentle unobstructed natural terrain and seats each shelter group independently without
editing any heightfield samples or ground materials. A source concrete service slab under the
workshop is the only raised continuous surface: 7 by 10 m, nominally 14 cm above the local high
ground. Its solid source triangles also support people and the transport.

The camp is roughly 20 by 26 m before the parked vehicle and clearance. One quarters tent contains
sixteen cots. The medical tent contains two active treatment beds and two spare stretcher stations,
source medical shelving and first-aid cases. The one-medic/two-bed coordinator capacity is unchanged;
spare furniture does not confer treatment capacity. The ammo tent has canonical game 88 mm rounds
in physical racks and Pizza small-arms boxes. A separate fuel tent contains twelve Pizza drums.
The workshop, rest, cooking and orders spaces share the compact central access route.

Visible storage is display geometry at this stage. It does not refill or override the sole tracked
medical AidBox, person ammunition or campaign item ledger. Real stock-item transfer and depletion
presentation remain future integration. No new assets were downloaded for this camp revision.

Source19 corrects the medical and small-arms shelf contact. The actual wooden deck tops are
0.30/0.80/1.30 m, whereas the earlier 0.32/0.79/1.25 m placements floated or embedded cases. Placement
now subtracts each prop's source `min.y` from the correct deck height. Medical racks retain exactly
18 cases, with two pairs stacked using the case's real 0.08 m height. The 18 open small-arms boxes
remain single-layer because their contents project above the box. No item or treatment ledger is
refilled. The private-source test checks case bottoms against the actual drawn shelf/case triangles.

The source layout fixture still uses a neutral ground plane to inspect meshes; it is not the live
camp terrain. The v4 review below uses the actual Act One terrain, camp and roster, with separate
overview, medical, ammunition, fuel and approach photographs.

## Verified evidence

`captures/halftrack_driving_20261030_a.json` reports **8/8 checks passed**. Only the current person's
starting feet/look beside the cab were staged. Actual source cab targeting boards PersonId 1 into
the existing VehicleId 2; three seconds of ordinary drive move it 9.019 m to 5.85 m/s and consume fuel.
A moving exit is refused; two seconds of Space braking while drive remains held stop it, then a
source-clear exit retains the same person. Total travel including braking is 12.419 m. Fuel changes
from 40.0 to 39.968685 L, and personal ammunition is not replenished. The source contact controller
remains available. This run does **not** prove deliberate collision response, death/succession,
cargo, towing, crane operation, historical handling, armor/damage or disk restore.

`captures/expedition_live_camp_20261030_v4.json` reports **6/6 checks passed**. Driver PersonId 4,
actor 1397751810, is explicitly given an incapacitating wound and delivered beside a bed as a fixture.
Actual anatomical pickup, source-bed selection and admission preserve identity, wound and blood.
Named medic Gefreiter Schneider physically reaches the bed. Care consumes finite supplies without
replacing lost blood; the same patient survives, remaining wounded. Twelve samples cover 60 seconds
of care. This validates the bedside interaction and treatment path; it does not prove a shell wound,
field extraction journey, transport evacuation, automatic healing or save/resume.

`artifacts/expedition-20261030/live-camp-v4.log` records **146 admitted approach-dressing pieces,
including 27 pushables**, with zero missing assets and all 146 retained after the final world-cover
check. The bounded generator uses a nominal 180 m approach and an 8 m lane. Its receipt reports
`requested_end_connected=false`: the camp-side route reaches a reachable fallback, not a verified
connection to the exact requested endpoint. Admission counts are not proof of visual density or
complete route playability.

`artifacts/expedition-20261030/focused-tests-19.log` records six passing `support_camp` tests,
including the actual private props and shelf-contact test; three passing `halftrack_runtime` tests,
including source-contact rejection, bounded drive/fuel and reverse steering; and two passing
`camp_dressing` route-clearance tests. These 11 selected tests have no failures or ignores. They are
not the whole repository suite or a claim that every older private route test was rerun.

## Remaining review and integration

Final source-20 follow-up: release compilation and 43 focused tests pass. The added third dressing
test protects candidate footprints/clearance beyond the obstacle-query bounds. Independent
`live-camp-v5.log` retains all 146 pieces/27 pushables, and `expedition_live_camp_20261030_v5.json`
again passes all six staged bedside-care checks. Approach/medical/ammunition images are
byte-identical to v4, so the visual limitations below still apply.

- The inspected v4 approach image still reads sparse at distance despite the admitted dressing.
  The medical view now separates the medic's lowered arms from stretcher rails and shows some cases,
  but stock remains dark/noisy. The ammunition doorway is partly occupied by two soldiers, and the
  interior stock is too obscured to approve fully. Do not label every object visually validated.
- Complete deliberate wall/tent/vehicle contact driving, turn/reverse, collision and camera/pivot
  review under live inputs. The current pass is a short board/forward/brake/exit sequence.
- Test transport continuity across an actual controlled death and explicit successor selection,
  confirming the one vehicle's stopped position and finite fuel. Durable records and source code
  alone are not that live proof; complete disk restore remains unavailable.
- Add real cargo, passengers, towing/crane/recovery mechanics and visible inventory transfer/depletion
  as separate work. None is implied by the transport or shelf-display milestone.
