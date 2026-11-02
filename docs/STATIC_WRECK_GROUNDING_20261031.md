# Static wreck support checkpoint — 2026-10-31

`src/static_wreck_support.rs` and `src/static_wreck_ownership.rs` are registered with the actual
manifest integration hook. Build48 passed all eight static support fixtures, including native
KV-1/PzIII flat and mild-grade cases and safe rejection of an overturned sledge outside the
authored correction bound. Build57's actual Canyon receipt demonstrates partial coverage:
146 of264 independent wrecks are now admitted in the actual build57 map;118 original poses remain
unresolved, with109 assembly members and43 deliberately embedded rock formations preserved.
The generic checkpoint correction passes all15 focused native tests and retains every one of
the145 previously admitted build52/54 source identities. This is not a completed grounding pass.
Root owns shared render/collision admission, compilation and actual
scene review. No Cargo or GPU was run by
the source-support agent. `rustfmt --emit stdout` parsed the new module and candidate hook;
that is not a compile or test pass.

## Actual build57 map validation

`artifacts/expedition-20261030/ground_canyon_b57_a.json` and its log report146 admissions,
118 unresolved originals (93`Unstable`,25`SearchBudget`),109 preserved assembly members and43
embedded formations. Exact `(asset,source_index)` set comparison retains **all145** previously
admitted build52/54 identities, with zero losses. Preserved owner identity/reason sets and complete
embedded-formation records are unchanged. The compact durable comparison is
`artifacts/expedition-20261030/ground_canyon_b57_admission_comparison.json`.

Index1248 now succeeds at112 proposals and197,412 face pairs using its query88 checkpoint;
11962/12521 retain their original96-query descent-only results.11689 succeeds at104 proposals,
and260 at95. Index11794 is one new admission beyond the old union: a previously certified candidate
is retained when a later probe consumes the remaining shared face budget. Observed maxima are
112 proposals and250,000 face pairs. No support threshold or source geometry changed.

The runtime log reports51.678s manifest preparation, including all prototypes. This is not isolated
solver timing or gameplay FPS. `captures/ground_canyon_b57_a_overview.png` was visually inspected;
the visible PzIII260 is now supported according to the receipt, but this image cannot certify every
clutter item or submillimetre contact. The118 unresolved originals remain an explicit limitation,
along with neighboring-prop collision, decorative-wreck response to later terrain changes and
editor-preview parity.

## Actual Canyon build49 evidence

`artifacts/expedition-20261030/ground_canyon_b49_a.json` was written by the runtime for the exact
current launcher manifest. It records 124 admissions, 140 unresolved original poses, and 109
preserved cart/sledge/horse assembly members. The earlier 316-wreck inventory excludes 57 horse
meshes; its 52 preserved cart/sledge members plus those horses explain the 109 owner entries.
The unresolved cases comprise 83 `Unstable`, 35 `SearchBudget`, and 22
`UnsupportedAfterClearance` results. Whole-face clearance safely rejected these last cases after
source-vertex descent found a different apparent rest. Terrain residual ranges in that category
are 0.015–1.065 m. The logged manifest preparation took 46.478 seconds; it includes all prototype
preparation and is not an isolated support-solver timer.

`captures/ground_canyon_b49_a_overview.png` shows PzIII source index260, which remains unresolved;
it must not be presented as a corrected example. `captures/ground_canyon_b49_b_kv1_269.png` shows
an admitted KV-1. Its origin moved upward 0.289 m, with nine source contacts and 66 support
queries; all 42,038 vertices and 19,975 triangles were retained. Visible track contact is
plausible in this view, while exact support comes from the receipt rather than pixel inspection.

The three Canyon `rock_0/1/2.glb` sources have 396 vertices and 132 triangles each, but 22 open
boundary edges. Longitude-varying pole heights in `canyon_v2_assets.py::Mesh.ellipsoid` cause
the strict closed-solid rock builder to reject them. They are also the wrong placement class
for loose-rock rotation: the current map has 43 generator-owned `large_rock` formations, all
scale1 with a deliberate ground-minus-0.55m authored origin and upward snow material. The
build50 candidate recognizes only the exact version6 generator and these three source paths,
preserves unedited scale1 base formations, and reports them separately as
`embedded_rock_formations`. Explicit editor edits/additions retain their existing preservation.
This is a classification correction, not a claim of repaired topology or newly supported rocks.
Loose stones still require actual closed-source support; no closure tolerance was relaxed.
Full source hashes and topology evidence are in
`artifacts/static-wreck-grounding-20261031/rock_source_topology_20261031.json`.

## Build52 actual map and build53 diagnostic

`artifacts/expedition-20261030/ground_canyon_b52_a.json` reports 142 admitted, 122 unresolved
originals, 109 preserved assembly members and 43 preserved embedded formations. The source
still does not admit index260. The 62.953-second manifest preparation includes all prototype
work; it is not an isolated support timer or a gameplay FPS measurement.

`artifacts/expedition-20261030/foundation_b53_canyon260_scan.log` records a test-only audit of
58 orientations around index260's final rejected pose, retaining all 36,903 source vertices,
17,960 triangles and the actual loaded detail lattice. Nine samples meet the unchanged 10 mm
COM inset after the complete face/terrain clearance certificate. All nine have greater COM
height than the rejected pose. The lowest-energy certificate adds +0.0005 radians to both
pitch and roll: its margin is 224.925 mm, raw COM height rises 0.194834 mm, and represented
placement height rises 0.194550 mm. Its minimum source-vertex clearance is 0.285149 mm against
the 0.287982 mm numerical allowance; full-face clearance remains the authoritative certificate.
No hypothetical XZ relocations were sampled because fixed-XZ certified poses were found.

Fine probes also lower whole-face COM height by 23.31 and 10.53 micrometres without meeting
the inset. Therefore the final vertex/witness search is not proven to have reached the true
whole-face minimum, and the failure is not solely its one-micrometre energy floor. The sampled
neighborhood proves feasible fixed-XZ authoring poses; it does not prove global infeasibility
of a downhill supported pose or a physically traversable falling/sliding path.

The build54 correction reserved eight of the existing 96 support queries for a fixed
eight-neighbor initial-placement check when descent cannot produce a final certificate. Its
angular step is derived from the source radius and 5 mm contact band. Every candidate must
retain the same XZ, yaw, scale, source mesh and ownership; meet the existing 12-degree authored
correction bound; and pass the complete original-face certificate with the unchanged 5 mm
contact band and 10 mm COM inset. The total movement of every source vertex, including the
new fitted origin height, must be at most 5 mm from the rejected face-fitted pose. Candidates
share the original cumulative 250,000 face-pair budget. No support or budget limit increases.

An accepted fallback is labelled `nearby_full_face_certified_authoring_pose` and reports the
angle, maximum source displacement and COM-height change. It is an initial authoring adjustment,
not simulated gravity descent. Its shared transform still feeds render, walk, salvage and
collision registration. Neighboring-prop collision remains unverified, full falling dynamics
are not implemented here, and a failed check retains and reports the original unresolved pose.
The existing native260 assertion remains intact; all14 build54 static tests passed, including
native260 (95 total queries,23,282 face pairs), narrow-peak and diagonal/budget regressions.
The diagnostic's pair total counts successful
probes only; tiny signed energy differences alone are not a physical-motion guarantee.

## Build54 regression and explicit build55 phase budgets

`ground_canyon_b54_a.json` records143 admissions,121 unresolved originals,109 preserved assemblies
and43 embedded formations. Compared with build52,260,1248 and11689 become admitted, but11962
and12521 regress to `SearchBudget`. Both previously required exactly96 descent queries and no
face refinement.11962 is a KV-1 with7 contacts,176 face pairs and pitch/roll−6.200252°/−5.254509°;
12521 is a lorry with87 contacts,3,788 face pairs and pitch/roll−2.249225°/−6.986212°. Their authored
angles were+1.740°/+0.170° and−0.310°/+2.220° respectively, within the unchanged12° correction
bound. Reserving eight queries reduced their available energy search to88 and lost known-safe
poses. Failure receipts do not record the truncated terminal angles; those are not inferred.

Build55 therefore explicitly restores **96 descent proposals plus up to8 separate authoring
proposals,104 total**. This is a disclosed budget increase from build54, not an increase to the
historical96-query descent phase. The cumulative250,000 face-pair budget and all physical gates
remain unchanged. Receipts split `descent_queries`, `authoring_queries` and total`support_queries`,
and state all three limits. The native regression includes both prior96-query successes and the
two new88-query fallback successes,1248/11689, because their fallback neighborhood may change
after more descent. Native260 and the impossible-support controls remain required.

The extra descent work is at most8 BVH/witness proposals, while fallback remains at most8
full-face proposals. For the two regressed sources, conservative full-scan ceilings for those
eight proposals are336,304 KV-1 points and61,056 lorry points. No startup performance gain is
claimed:52's62.953s and54's49.475s preparation totals include unrelated prototype work. Native
per-case time and full-map admission-set preservation must be checked by the parent. Sources,
XZ, yaw, owner assemblies, support margins and original unresolved records are unchanged.

## Build55 result and build56 saved-neighborhood correction

`foundation_b55_target_2.log` restores11962 and12521 at exactly96 descent queries with no authoring
probes. Their native attempts take246.354ms and10.783ms respectively, with176 and3,788 face pairs.
11689 remains admitted at96+8 queries and20,364 pairs (192.794ms).1248 fails `SearchBudget` after
297.020ms: extending descent changed the final neighborhood and discarded its earlier, previously
certified88-query neighborhood. These are individual CPU fixture timings, not frame-time or
whole-map startup measurements. The prior-safe-placement test remains failing until corrected.

Build56 retains one generic best-pose checkpoint at query88, including the current iteration's
best-so-far candidate, then continues the original96-query descent exactly once. If its final
eight-neighbor phase fails, it re-certifies that earlier pose and tries the same fixed eight
neighbors there. Identical final/checkpoint angles are deduplicated. No source index, known outcome
transform, random search or repeated96-query run enters the runtime algorithm.

The explicit bound is now **96 descent +8 final-neighborhood +8 checkpoint-neighborhood proposals,
112 total**. All phases and the one reused-pose face certificate share the unchanged250,000
face-pair ceiling. Receipts include checkpoint-probe and checkpoint-certificate counts; an accepted
checkpoint neighborhood identifies `reference_descent_query:88`. The existing5mm total source
movement bound is relative to that explicitly identified, fully face-fitted reference pose.
All geometry, XZ/yaw/scale,12° correction limit,5mm contact band,10mm inset and source ownership
checks remain intact. This is still bounded initial authoring, not falling/sliding dynamics or
neighboring-prop collision admission. The narrow-peak negative test exercises both neighborhoods
and the112-proposal bound. `artifacts/expedition-20261030/foundation_b56_static.log` reports
**15 passed,0 failed**, including all five known-safe native placements, the impossible-support
controls and safe rejection beyond the authored-angle limit. The focused test process took1.99s;
its output does not include per-case timing/counters because successful test output was captured.
This is not a whole-map or startup-performance claim. The later actual build57 receipt above
confirms preservation of all145 combined52/54 admitted source identities.

## Actual affected assets

`tools/launch_windows.ps1` selects `assets/worldgen/workshops/gaea-canyon-run-v2/manifest.v1.json`.
The bounded inventory found 316 relevant static wreck placements there, 232 with authored
nonzero pitch/roll. Eight are whole source-baked overturned sledges. Their placement up vectors
remain positive, so checking placement roll alone would miss the overturned source geometry.
The March manifest has no such static wreck assets. Its dynamic wrecks have a different owner.

The ten prepared Canyon source files have 544–19,975 triangles and 1,632–42,038 vertices.
They all exceed the existing closed-rock helper's 512-triangle limit. The largest are the
prepared Pz III and KV-1 wrecks, not the multi-million-triangle live native Tiger donor. A
Gen-only retained-donor fix would miss the 316 current launcher placements.

`artifacts/static-wreck-grounding-20261031/manifest_wreck_inventory.json` records all eight
inspected manifests, effective source indices, asset IDs/paths, transforms, edit operation
indices and available original owner tags/scene metadata. No manifest was modified. Source
counts are header/index-count evidence, not a native runtime contact or timing receipt.

## Module contract

```rust
SourceStaticBody::new(&CpuMesh) -> Result<SourceStaticBody, Rejected>
SourceStaticBody::place(
    original_model: Mat4,
    hf: &Heightfield,
    exact_node: &impl Fn(u32, u32) -> f32,
) -> Result<Admitted, Rejected>
```

Build one cache from the actual already-loaded, flattened `CpuMesh` per asset. It retains all
source vertices, triangle indices and one point BVH. The COM is explicitly a uniform source
surface-area thin-shell estimate, not a measured vehicle mass distribution. An explicit
estimated COM may instead be supplied through `with_estimated_com`.

The node callback reads `Detail::lattice_node_height(i,j)` when detail exists, otherwise
`hf.heights[j as usize * hf.res + i as usize]`. Do not reconstruct a node by calling
`grade_at(i*step-half)`: inverse f32 grid arithmetic can interpolate another location.

The solver preserves X/Z, scale and authored yaw, searching at most 12 degrees of pitch/roll
correction from the authored pose. It never substitutes a hardcoded upright pose. An already
stable overturned asset remains overturned. Vertex support queries use every original source
point through the shared exact-point BVH. The estimated COM must be at least 10 mm inside the
contact polygon, with a named 5 mm near-contact band.

For affine or nearly affine terrain, a full-footprint min/max residual interval supplies a
conservative whole-face clearance certificate. If that is too broad, the final check clips
actual low source triangles against the actual terrain triangles. A source face is skipped
only when its lowest possible Y is above a conservative terrain upper bound. Within each
clipped polygon both surfaces are affine, so their maximum height difference occurs at a
polygon vertex. Vertical source faces use the same vertical-prism clipping. This catches a
terrain peak between all original source vertices and can obtain support from actual face
interiors. The visible source is translated upward by the required clearance; no bury offset
or replacement support primitive is added.

Current build56 caps are100,000 source triangles,96 descent plus two8-proposal authoring neighborhoods
(112 total),65,536 terrain nodes,250,000 clipped
source-face/terrain-triangle pairs and 65,536 contact witnesses. Failures return named reasons.
The detailed final face check is startup placement work, not per-frame simulation.

## Integration and ownership

The parent integrated the hook after the actual `CpuMesh` loads and before existing walk surface,
salvage, collider and render-page registration. It changes the one shared page transform and
produces a receipt. `Ownership::from_manifest(&ActManifest)` captures effective source ownership
before paging discards metadata; `policy(index, asset_path)` returns `None` for ineligible assets,
or `Policy::Independent` / `Policy::Preserve(reason)`. It mirrors editor index rules and checks
the source path again. Deleted slots remain allocated. Untagged hand-set operations revoke
generator ownership; an editor addition alone does not establish independence.

Known complete source paths from the exact Canyon base generator and tagged Canyon wreckage
additions are eligible. Read-only expected policy counts for the 316 inventoried complete wrecks
are **264 independent candidates and 52 preserved horse-traces cart/sledge pieces**. Exact
source indices are in `artifacts/static-wreck-grounding-20261031/canyon_ownership_expected.json`.
These are ownership counts, not successful geometric admissions. Other source pieces, including
horses and multipart tanks, may contribute additional preserved records in the runtime receipt.

- Preserve rigid scene blocks and separately modeled hull/turret/gun assemblies together.
- Preserve identifiable intentional-mired authored placements and report that reason.
- A `canyon_v2_wreckage.armour.xx` scene tag is narrative grouping, not proof that all its
  objects form one rigid mechanical assembly. Owner classification is still required.
- Horse/cart traces or other mechanically connected neighboring objects need an assembly
  decision before one member's transform changes.
- Source-baked overturned whole assets remain eligible only when independently owned.

An unsuccessful solve leaves the original model untouched **and reports it as unresolved**;
it is not counted as grounded. Root must choose a visible authoring repair or explicit rejection
for these records. There is no silent deletion. Wholly preserved owner records are also logged.

## Validation and limits

Five pure support tests are written but have not run at this checkpoint: tilted/inverted box rest, rejection
of one central terrain peak that cannot stably support a box, support from four actual interior
face/terrain contacts, vertical-source-face clipping, and affine-versus-nonplanar residual bounds.
An ignored native support test loads actual Pz III and KV-1 GLBs on flat and 2.5% grade,
reporting retained counts, contact counts, BVH/face work, minimum source clearance and
cache/placement timing. A separate ignored test requires the source-baked overturned sledge
to reject safely: its reachable stable rest lies outside the 12-degree authored-pose correction
contract. It is not called grounded. Four ownership tests cover index stability, intentional
burial, multipart pieces, horse/cart association, and actual Canyon editor-page index parity.
None was run by this agent; the parent supplied the build-47 results above. Actual native
placement admission counts and startup timing remain required.

## Build-47 native failure and build-48 correction

The KV-1's lowest contacts belong to actual `L.Truck`/`L.Truck.001` source meshes. Independent
pitch/roll coordinate descent stalled on a diagonal support ridge around pitch -1.936 degrees,
roll -0.358 degrees, with estimated COM 3.74 mm outside the narrow contact polygon. Isolated
axis proposals raised energy although a combined gravity-driven tilt lowered it. The source
is now tried along the actual Euler-axis gravity direction about its closest support edge,
in addition to the existing axis proposals. Every proposal must still lower measured full-source
energy and meet the unchanged contact/stability admission. No support band, COM margin,
12-degree correction limit or 96-query budget was enlarged.

An external source diagnostic found KV-1 flat support with a 235 mm COM interior margin in
51 queries and Pz III support with 306 mm margin in 26. At 2.5% grade, the corresponding probes
used 61 and 31 queries. These Python probes establish the cause and candidate direction;
they do not replace the native Rust tests. Full receipts and source hashes are under
`artifacts/static-wreck-grounding-20261031/native_support*_diagnostic*.json`.

A new closed V-underside regression demonstrates the same diagonal valley: each isolated
pitch/roll move raises COM, while a coupled move reaches a broad real source face. The unrelated
scatter scale fixture was corrected by centering its authored origin inside the closed diamond;
its former face-through-origin geometry made support height genuinely insensitive to scale.
All original packed-transform, source-clearance and cover agreement assertions remain.

This is an initial static support proposal. It does not simulate a fall, validate a swept motion,
solve suspension or check nearby building/prop collision. Any changed pose still needs the
owner's normal obstruction admission against other scene objects. The bounded orientation
search can reject a feasible distant equilibrium. Its initial proposal uses source-vertex
support; the final full-face check can reject that pose without searching a second face-driven
equilibrium. The thin-shell COM remains an estimate, and the 5 mm band is a named numerical
contact tolerance. A rejected budget, ambiguous assembly or unsupported pose is not completion.

The older `PLAN.md` and `static_wreck_grounding.candidate.rs` under the artifact directory describe
the superseded Gen-only investigation; this generic actual-manifest contract is the current plan.
