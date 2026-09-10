# Retained source-contact blood — 2026-09-05

Status: integrated source, Rust 2021 syntax parsing and whitespace checks passed. Root's coordinated Cargo gate and current executable first-person video proof are pending. This is not art approval or a performance claim.

## Cause and changed behavior

The previous wet grains were 22 mm cubes. Its floor model reserved eight pooling stamps per casualty; when those slots filled, a remote new contact could enlarge the nearest old pool. Some side contacts also came from an analytic cabin cylinder, floor plane or breech box rather than the visible source geometry. That path and its proxy-only tests are removed from the live Game.

The replacement is `src/surface_blood.rs`. It preserves the existing 8,192-grain capacity, eightfold visual allocation from recorded wound quantities, casualty peer IDs, owner slots, grain seeds, exact wound origin, fan distribution, inherited body velocity, and ejection velocity. It creates no injury, fragment, or penetration events. Grain cubes now have a fixed physical width of 4 mm. Actual triangle contact absorbs each wet grain immediately into retained film; there is no artificial rebound-stamp multiplication. An airborne timeout or overwritten ring slot creates no stain.

Film consists of 5 mm surface cells. One measured contact grants nine cells of authored area. This is a presentation model for a thin blood film, not a calibrated fluid volume or a viscosity/gravity solver. The same mechanism works on horizontal, vertical and sloped source faces. Cells retain source-frame position and normal; hatches, turret, barrel and floor transforms move their own evidence. A component retains the original contributing actor IDs and grain seeds. Its attribution represents the contributors to the connected pool, not a claim to uniquely identify the source of each mixed film cell.

Every prospective cell is swept against the existing cabin source triangle bins. Six continuous support spans plus subtraction of the union of coplanar source triangles from the complete cell footprint prove supported area. The full-area test rejects enclosed holes which edge or diagonal probes could miss. Proof is confined to the named frame; a neighboring moving component or the opposite armor face cannot supply missing support. Components join only once accepted cells physically meet. An unverified queued cell cannot spend another contact's budget elsewhere.

## Bounds and rendering

- 8,192 airborne grain slots; unchanged injury-side allocation.
- 65,536 retained cells, at most 1.6384 square metres of certified film before overlap effects.
- 65,536 contributing source identities; additional coverage is rejected at the bound, without relocating, erasing or enlarging remote deposits.
- Hash-indexed accepted, queued and rejected cells; bounded component frontier. No all-pairs particle/contact search.
- At most 384 frontier work items per update, with a 12,000-item-per-second clock. Work includes rejected candidates and pending component joins; it is not a guaranteed rate of accepted cells.
- Source frame inverses are prepared once per update and reused across grain sweeps and film candidates.
- Contiguous certified rows coalesce into thin boxes in the physical moving-instance/RT pool. This reduces live instances while preserving cell-sized exposed edges and avoiding unsupported gaps. Maximum reserve remains one instance per retained cell; typical run counts require runtime measurement.
- All persistent pages use the same dark, non-emissive material; page assignment cannot introduce random color patches.

The existing `dried_blood_stains` counter now means retained cells. `settled_cubic_blood` is zero because actual contact absorbs the grain immediately. The capture snapshot adds `surface_blood` with emitted/contact/airborne/cell/wall/run/rejection counts, capacity and physical dimensions. Old small-stamp count thresholds are obsolete. Film can continue growing within its contact credits after the final airborne grain retires, so short-term persistence is nondecreasing until growth settles.

## Authored checks, awaiting root execution

Eight module tests cover actual grain contact and moving-frame retention with actor attribution; repeated-contact connected growth with immutable old cells; simultaneous unverified seeds retaining their credits only after connection proof; hard saturation without remote relocation; floor gaps and opposite wall faces; sloped source triangles; full-area coverage across 21 source strips and rejection of an enclosed hole; fixed 4 mm grain dimensions and zero deposition on timeout/recycling. The Game test covers the unit-box persistent primitive and shared capacities. The live-source contract now requires Game to call the new source-backed transport and render paths.

## Required visual/performance proof

Record a real incoming penetrating hit from an ordinary first-person crew view, with the same injury/fragment events, camera and audio as the comparison. Inspect the visible wet fan, immediate contacted wall/floor film, and several seconds of connected accumulation. Include a second first-person view after the hit to inspect floor edges and vertical splatter. Report source-contact/cell/run counts and the frame-time impact with the actual executable identity. Do not tune a favorable hit seed or force extra wounds to make this look denser. Verify no surface leakage and no disappearing early deposits during a second genuine hit.

The 4 mm/5 mm sizes are physical world dimensions, not an exact screen-pixel lock at every depth. Snow/dust have a separate one-internal-pixel rendering contract. Unsupported curved transitions and pathological polygon-fragment cases fail closed; this can underfill awkward source surfaces and needs visual review rather than relaxed geometry checks.

## Coordinated v14 CPU result

The frozen v14 `v14-unit-tests-attempt3.log` passes all eight `game::surface_blood::tests`. Its one unrelated asset-catalog failure came from missing copied Python tools in the isolated snapshot; root reports the corrected rerun passed, for 753 unit passes and three ignored overall. The blood source tests therefore passed. The final user-visible grain density, pooling, wall contact, frame time and first-person audio/video still require their own matched executable review.
