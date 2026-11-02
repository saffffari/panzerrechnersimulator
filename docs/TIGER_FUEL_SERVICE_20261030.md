# Tiger physical fuel service â€” 2026-10-30

Status: runtime and live Game/input/save integration are on disk for build35; build35 native compilation and all five fuel tests passed; GPU gameplay verification remains pending. The instant rear-zone transfer has been removed. The standalone full-face CPU source sweep passed all four complete cap motions at50 samples each (111.02seconds); build35 native tests also passed; the translated/rotated picking additions and live witness remain required for build36.

## Source, references and limited reconstruction

The original `assets/tanks/tiger/baked/tiger_hull.glb` is unchanged (SHA256 `51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84`). Vehicle-left outer armored plug is `Hull_0875`, centered about `(1.1462,2.0341,-1.3491)` in hull metres. The upper fuel tank, including a fused inner cap, is `Hull_0902`; the surrounding deck is `Hull_0708`. These are distinct from the radiator service cap farther aft.

[The February1943 chassis draft, D656/21](https://bushmakow.com/wp-content/uploads/2024/05/155.-D-656_21-TIGER-compressed_bushmakow.pdf), printed pages39â€“40, describes four tanks connected in two groups, with an interconnecting pipe permitting filling from one side when the valves are open. It states534L; the game currently retains its existing540L calibration. [DavidByrden's original Tiger131 upper-fuel-tank survey](https://tiger1.info/EN/Upper-fuel-tanks.html) identifies the filling pipe and corresponding cap in the armored grille. Neither source certifies the dimensions of our authored service bore.

The kit model is a closed CAD solid. `tools/prepare_tiger_fuel.py` preserves the original files and produces four private derivatives under `assets/tanks/tiger/workshop_fuel/`, with hashes/topology checks in `catalog/tiger_fuel_service.v1.json`:

- The original outer plug, unchanged.
- Its fused inner fuel cap split from the upper tank at y1.934m, with a closed underside.
- A visible45mm-radius receiving well down to y1.805m in the tank solid. This is a shallow blind receiver, not a claim of a reconstructed full tank interior.
- A deck derivative giving the outer plug98.25mm radial clearance only above y2.0259m. The original97.30mm-radius plug's fitted facets otherwise catch the deck lip during lift. The original155mm plug socket remains intact. The105mm lower throat is visibly enlarged to140.6mm because the113mm inner cap is eccentric beneath it; this bounded reconstruction is necessary for an actual clear path.

Boolean seam cleanup is explicit:10micrometres for the inner cap,0.4micrometres for the tank,1micrometre for the deck; nine small tank seam edges totaling4.671mm are closed. Each admitted derivative is closed, consistently wound and has no collapsed edges or zero-area triangles at the declared audit precision. Earlier full-depth and upper-lip-only deck-clearance candidates remain private and unused; only `deck_filler_clearance_v3.glb` is admitted.

The rendered derivative triangles are the contact geometry. No hidden opening, permissive rear zone or unrendered collision proxy is used. Source-motion admission uses the existing full-face translation SAT, plus explicitly nonpenetrating fitted/edge contact. An entire separating support plane may depart along translation;1mm penetration must still fail the regression.

## Interaction and accounting

Only the vehicle-left filler is operable in this slice. Park upright (the guided service requires up.y≥0.95), stop the engine, put equipment down, hold LMB on the actual outer plug, then the inner cap. Both original identities remain visibly parked on the source deck. The path is guided manipulation, not free-body dynamics or simulated thread pitch. Releasing stops at the exact partial pose; another press reverses an interrupted cap stroke. The outer plug cannot close over an absent inner cap.

Pick up a finite fuel can and aim at the actual open receiving well; hold LMB to transfer at an authored0.8L/s. The existing `PortableItem` owns the source litres and the existing Tiger fuel field owns the destination. The model's actual flexible spout end is measured from the six source rim vertices; its downturned pour pose places that spout over the visible opening. No can, replacement crew, mission, save or load creates fuel.

Release, aim away, change hands, move the tank, start the engine, lose consciousness, die, empty the can or fill the tank: the transfer stops. Destination rounding is resolved before draining the can, so quantities too small to represent at the destination remain in the can. No fuel vanishes merely because a can contains less than0.01L. Partial cap poses, work direction, source pickup index and transfer progress are serialized; pickups and Tiger litres remain under their existing owners.

Unimplemented: separately operated connecting/reserve valves, historical534L capacity recalibration, cap seal leakage, spilling/fire from an open filler, free detached-cap motion, thread torque and liquid CFD. A new normal-play review is still required to assess reach, hand posture, source occlusion, sound and readability.

## Implemented integration contract (build35 source)

Register Game child `#[path="tiger_refuel.rs"] mod tiger_refuel;` and add `tiger_refuel: tiger_refuel::Runtime` to Game.

At scene construction, immediately after workshop installation and before `CabinFire`/acoustic source-cache construction:

```rust
let tiger_refuel = tiger_refuel::Runtime::install(&mut scene, &slots)?;
// Add tiger_refuel to the Game initializer.
fire_surfaces.extend(tiger_refuel.source_surfaces(&scene));
armor_surfaces.extend(tiger_refuel.armor_surfaces(&scene));
```

The old deck/fuel-tank roots are removed by exact triangle multiset identity and replaced before the source caches are built. The moving plug/cap reserve frames77/78; the fixed tank/deck remain frame0. Workshop and acoustic masks must leave foreign frames77/78 enabled. `FRAMES=80` already fits this reservation.

Call `g.tiger_refuel.configure(&mut r)` once after Renderer construction. At the end of `fire_surface_models_at`, after workshop frame composition, call `self.tiger_refuel.append_models(&mut models)`. During ordinary Game update, after foot/carried-prop presentation, call `self.update_tiger_refueling(input,dt,r)` exactly once. It uses normal input, existing physiology and source contacts. Initial and restored presentation uses `self.present_tiger_refueling(r)` without advancing time.

Main LMB routing, before the held-can drop/portable action:

```rust
if game.tiger_refuel_interact(pos, fwd) { /* consume this primary action */ }
```

Add `game.tiger_refuel_hover(pos,fwd)` to ordinary contextual hover priority. Remove the instant refuel branch from `Game::interact_portable_item`; that method should only pick up or put down cans. Remove its rear-zone pour prompt in `update_foot`; ordinary held-can fallback is `PUT CAN DOWN [X]`. Remove the unused `standalone_fuel::refuel_from_pickup` path after no callers remain. Existing fuel-consumption law remains unchanged.

`game.physical` owner becomes version3, including:

```rust
fields.put("tiger_refuel", &self.tiger_refuel.state)?;
// During candidate import, before mutation:
let fuel: tiger_refuel::State = fields.take("tiger_refuel")?;
fuel.validate(self.pickups.len())?;
self.tiger_refuel.state = fuel;
```

Use the same version in export, import and `REQUIRED_OWNERS`. There is no default-fill compatibility fallback. The source catalog/GLBs are captured by existing source fingerprinting. The new fields are not an additional fuel inventory.

## Verification to run

`python tools/prepare_tiger_fuel.py --verify` checks source and derivative hashes. `python tools/validate_tiger_fuel_motion.py` is a standalone CPU full-face source sweep, with a structured receipt under `artifacts/tiger-fuel-source-20261030/`. It is preliminary evidence; the native Rust loader/contact test remains authority.

Native filter: `tiger_refuel::tests` includes capacity/small-remainder conservation, exact interrupted-pour binary continuation, access-order invariants, nonpenetrating source-plane admission, and both caps' complete source-triangle opening/closing paths against the real hull/deck/tank/turret. Root runs Cargo and GPU serially.

The live witness must prove ordinary LMB opening, exact cap identity, finite partial/full transfer, release/aim-away/death interruption, and complete-world mid-pour save/cold-restore/continuation. Until those receipts pass, this document does not claim the feature works in the running game.


The final parked-cap placement uses a100micrometre conservative clearance above the measured deck support y2.0379541m. This clearance is explicit numerical contact skin, not an invisible opening. `artifacts/tiger-fuel-source-20261030/full-face-motion.json` records the complete preliminary sweep and its38.147micrometre source error bound. The registered nested `tiger_refuel_review.rs` stages a quiet deck experiment, exercises ordinary LMB/can actions, and compares complete-world owner bytes before and after a cold mid-pour restore; it has not been run.

Build36 additionally fixes local-frame source-contact conversion for an actual translated/rotated Tiger, and keeps set-down cans on real Tiger source surfaces rather than forcing them through the deck to the terrain. The latter is bounded base-point support in the existing prop integrator, not general rigid-body can contact; obstacle sides, arbitrary camp surfaces and tilting onto the can's full source shape remain outside this change. Its new real-source native regression is pending.
