# Tiger workshop source registration — 30 October 2026

This began as a CPU source audit. The initial registration did not modify or export models;
later bounded derivatives and runtime work are identified in the checkpoint below. The source
audit used Blender 5.1.2 in background mode for geometry inspection and preparation. The live
GLBs were decoded with the repository's accessor and transform helpers. Build and gameplay
validation are separate root-agent runs, not outcomes inferred from this geometry audit.

The immediate result is two machine-readable registration catalogs:

- [Seven turret track mounts](../catalog/tiger_workshop_track_mounts.v1.json): exact source track geometry, mount transforms, neighboring retainers and exposed contact samples.
- [Seven cupola vision mounts](../catalog/tiger_workshop_vision_mounts.v1.json): exact rigid correspondence between the original housing roots. This does **not** establish equivalence of the separately reconstructed optical prescriptions.

## Provenance and frames

| Existing source | SHA-256 | Evidence |
|---|---|---|
| `assets/tanks/tiger/baked/tiger_turret.glb` | `20f86e9bacd1487869d42b59e40f86e0548af0a3626e57c250272b21f546e195` | 172 nodes; exact live turret root geometry |
| `assets/tanks/tiger/baked/tiger_hull.glb` | `51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84` | 1,215 nodes; anonymous hull root geometry |
| `assets/tanks/tiger/repair_link.glb` | `ce55948d2a2b89674127ae588df8a929ddff88ee5b88ec539ff43714f28c8f88` | 5,426 triangles; original node label `Tiger track shoe Hull_0628` |
| `assets/tanks/tiger/Panzerkampfwagen_VI_Tiger_Ausf.E.blend` | `c77d62794e20e4988150421b4895955443cc454bcf97b38e67832e6bf11a5c39` | Existing authoring file, 111,120,390 bytes; hashed, not edited |
| `C:/shop/ts_assets/cad_tiger_i_rm5010_engine_system.blend` | `965492f7fe9b6944276e9cd800967a2596b51b068c8fb1c2a9c316b79d3334e1` | 14 mesh objects, 591,688 triangles |
| `C:/shop/ts_assets/cad_tiger_i_rm5010_base_unit.blend` | `77ad2ff6f54c2ae352c2a1b81e628d527fc5307f16bc4439408ff955f2d58884` | 190 mesh objects, 9,938,992 triangles in conversion receipt |
| `C:/shop/ts_assets/cad_tiger_i_rm5010_complete.blend` | `905f930cbaf3a132e259b397bae693b603f185383f4edbf11e966156e3f38f0f` | 1,481 mesh objects, 31,081,446 triangles in conversion receipt |

The archived conversion receipt is `C:/shop/ts_assets/_archive/_conversion_report_20261028/cad_manifest.json`. It records the original STEP digests: EngineSystem `90346296c51578669d23777bfcb1e8ca8f16c7af904004166ccc68b18e25d424`; Base Unit `08944bca10dcabea4626864552108250d9a1878dff97f5d4bfbce71c5b3d7c42`; complete Tiger I `ca8afcc58e15b1746ea39bfe02613ba3989de372d1f8a0dbe5bd8dcf46015c04`. The receipt's old pre-archive source paths are historical references, not current paths to dereference blindly.

All runtime bounds and mount coordinates below are metres in their existing engine hull or turret frame. Do not apply the original glTF node's scale a second time: the catalog fits already operate on decoded, transformed source vertices. In both catalogs, `canonical_to_turret_matrix_cols` is suitable for `glam::Mat4::from_cols_array_2d`.

## Exact turret spare links

Every following source root has 5,426 triangles. Expanding its index buffer into triangle vertices and fitting a proper rigid transform against `repair_link.glb` gives a maximum residual below **0.723 micrometre** and rotation determinant +1. Differences in split vertex count and index numbers are exporter differences; expanded triangle order corresponds. This is substantially stronger evidence than a matching bounding box.

| Mount | Track root (node/mesh index) | Canonical repair-link origin in turret metres | Upper retainer | Vertical retainer | Paired outboard fittings |
|---|---|---|---|---|---|
| 0 | `Turret_0096` (75) | `(1.34255803, 2.00533883, 0.15978741)` | `Turret_0097` | `Turret_0102` | `Turret_0093`, `Turret_0094` |
| 1 | `Turret_0128` (97) | `(-1.34766363, 2.00533883, -0.07974950)` | `Turret_0130` | `Turret_0120` | `Turret_0132`, `Turret_0131` |
| 2 | `Turret_0134` (102) | `(1.34797286, 2.00533883, -0.10207131)` | `Turret_0133` | `Turret_0138` | `Turret_0135`, `Turret_0136` |
| 3 | `Turret_0147` (112) | `(-1.30663482, 2.00533883, -0.34343067)` | `Turret_0145` | `Turret_0141` | `Turret_0149`, `Turret_0150` |
| 4 | `Turret_0148` (113) | `(1.30230200, 2.00533883, -0.35933286)` | `Turret_0146` | `Turret_0157` | `Turret_0151`, `Turret_0152` |
| 5 | `Turret_0198` (126) | `(1.20498901, 2.00533883, -0.61544661)` | `Turret_0197` | `Turret_0202` | `Turret_0200`, `Turret_0201` |
| 6 | `Turret_0214` (136) | `(1.05122164, 2.00533883, -0.85164243)` | `Turret_0213` | `Turret_0216` | `Turret_0218`, `Turret_0219` |

The upper retainers have 1,364 triangles each; vertical retainers 430 each; small paired fittings 376 each. Their functional names are inferred from assembly placement. The complete CAD calls these families `15F37_BROCK`, `15F35_BROCK`, `15N03_PLATE`, and the shoes `15N90_TRACK`. The CAD placements are not identical to the edited live turret, so the catalog deliberately retains the **live** geometry and transforms.

For each upper retainer, the catalog also records triangle 1324's centroid and a 0.5 m exterior test eye. A ray through the complete closed source turret hits that centroid first, within 0.00001 m. These are exposed **surface** candidates for targeting. They are not proven historical pin axes, guaranteed player reach, or permission to omit a live obstruction query.

These seven shoes are already visible in the static turret. Separate them from the turret remainder before adding removable copies, or they will be duplicated and their original collision will remain behind after removal. Retainers should remain unless their own source assemblies are deliberately articulated. The finite stock should follow the seven physical identities, including a removed or damaged shoe; a counter must not create a second copy.

## Exact cupola housing correspondence

The seven roots `Turret_0129_housing`, `Turret_0139_housing`, `Turret_0140_housing`, `Turret_0163_housing`, `Turret_0164_housing`, `Turret_0205_housing`, and `Turret_0206_housing` each contain 78 source triangles. Proper rigid fits from block 0 give a maximum residual below **0.293 micrometre**.

Unlike the centred repair link, the vision catalog's canonical coordinates are **block 0's existing turret-local coordinates**. The recorded matrix transforms those vertices into each target's turret frame. Centring the spare later requires composing an additional explicit translation.

`src/tiger_periscopes.rs` generates window/mirror/relay geometry using a shared calibration eye and housing-specific construction. The housing fit alone is not proof that all eight generated optical parts can be copied with the same transform. Before substitution, validate each generated part or retain each mount's own prescription. Removal must alter the actual optical and contact authorities as well as the renderer slot; an invisible but still transmitting sight is not a physical replacement.

## Powertrain source registration

The named RM5010 CAD is related to the live source, rather than an unrelated replacement. Its EngineSystem's object bounds become the current hull's engine bounds with this measured mapping:

```text
runtime hull = 38.91981506347656 * (CAD.x, CAD.z, -CAD.y)
             + (0, 1.5086444912, -1.4756005299)
```

For the full/Base Unit CAD, the measured mapping instead adds `(0, 0.5706769481, 2.6576838298)` after the same scale and axis change. The different translation accounts for EngineSystem's separate assembly origin. These are **registration measurements for the existing edited game model**, not a claim that a 1:35 model should universally be scaled by 38.9198 or that the current tank's scale has been historically recertified.

The following mappings are strongly supported by unique matching position, dimensions and recognizable CAD labels. Runtime and CAD tessellations differ, so they are **not** exact triangle-equivalence proofs. A nearest-vertex check was also performed; it must not be misreported as a surface Hausdorff error because coarse triangle interiors need not contain matching vertices.

| CAD suffix within `TG_RM5010-0B29_EngineSystem` | Live hull root (node) | Runtime triangles | Meaning established by source |
|---|---|---:|---|
| `28U05_MAIN` | `Hull_0971` (947) | 64,684 | Main engine assembly |
| `28U20_FILTER TANK` | `Hull_0916` (892) | 5,482 | Forward filter canister |
| `28U20_FILTER TANK.001` | `Hull_1006` (982) | 5,482 | Rear filter canister |
| `28U01_COOLBOX` | `Hull_0968` (944) | 6,200 | Right-side engine cooling box as labelled |
| `28U02_COOLBOX` | `Hull_0967` (943) | 6,204 | Left-side engine cooling box as labelled |
| `28U09_PIPE` | `Hull_1070` (1046) | 888 | Right pipe assembly |
| `28U10_PIPE` | `Hull_1069` (1045) | 888 | Left pipe assembly |
| `28H43_EXHAUST` | `Hull_0990` (966) | 3,666 | Left exhaust assembly |
| `28H46_EXHAUST` | `Hull_0984` (960) | 3,308 | Right exhaust assembly |
| `28U23_BASE` | `Hull_0955` (931) | 2,028 | Source-labelled base |
| `28U31_CONVERTER` | `Hull_1080` (1056) | 7,022 | Source-labelled converter; function needs manual confirmation |
| `28U35_BOX` | `Hull_0956` (932) | 1,458 | Source-labelled box; function needs manual confirmation |
| `28U37_CARBURETOR` | `Hull_1075` (1051) | 3,438 | Right rear carburetor-labelled assembly |
| `28U43_CARBURETOR` | `Hull_1074` (1050) | 3,438 | Left rear carburetor-labelled assembly |

The initial common transform was estimated from the main-engine bounds and had an x offset of −0.00049583 m. The symmetric filter/carburetor pairs show that x=0 is the better common registration; the main-engine x-maximum differs by approximately 0.992 mm between tessellations. Do not bake the initial half-millimetre offset into all fourteen parts. Surface-level registration and source triangle conservation remain required before changing a live assembly.

Useful live hull-space bounds:

| Part | Minimum metres | Maximum metres |
|---|---|---|
| Main `Hull_0971` | `(-0.48650, 0.58235, -2.60233)` | `(0.56136, 1.91730, -1.30046)` |
| Forward filter `Hull_0916` | `(-0.23157, 1.64876, -1.88329)` | `(0.23157, 1.90136, -1.39873)` |
| Rear filter `Hull_1006` | `(-0.23157, 1.64876, -2.38924)` | `(0.23157, 1.90136, -1.90469)` |
| Main deck cover `Hull_0975` | `(-0.58866, 1.94844, -2.65876)` | `(0.59061, 2.05352, -1.31214)` |

There are 14 mesh objects in the inspected EngineSystem conversion. `28U05_MAIN` alone is one 383,886-triangle CAD mesh. No separately named piston or crankshaft objects exist in this fourteen-object assembly. This does not prove that no internal detail exists inside that mesh, or that another authoring source cannot separate it. It does establish that piston-level replacement is not already a fourteen-object visibility toggle.

## Access covers, cooling assemblies, and front drivetrain

The main engine access cover has a particularly strong registration: CAD `34Z03_COVER` maps to `Hull_0975` (node 951, 4,418 triangles), with summed minimum/maximum bound discrepancy below 0.54 micrometre. Adjacent `34B18_COVER` maps to `Hull_0899` (875, 1,972 triangles); `34B24_COVER` to `Hull_0999` (975, 3,170 triangles, approximately 0.17 mm bound discrepancy). A cover family name is not a hinge axis. Moving the large deck cover also requires identifying its attached handles, hinges, fasteners and clearance from surrounding panels.

| CAD family | Live root(s) | Evidence and limit |
|---|---|---|
| `27H12_BOX` / `.001` | `Hull_0901`, `Hull_0902` | Exact-position paired boxes forward of the cooling packs; ~8.75 micrometre bounds discrepancy. Their source label does not establish that they are fuel tanks or radiators. |
| `27K77_COOLBOX` / `27K76_COOLBOX` | `Hull_1009`, `Hull_1010` | Cooling assemblies; best bound correspondence differs by ~11 mm, so validate surfaces/pose before articulation. |
| `27H13_COVER` / `.001` | `Hull_1081`, `Hull_1082` | Rear cooling cover family, ~15.8 mm bound discrepancy. |
| `27H25_PLATE` / `.001` | `Hull_1102`, `Hull_1103` | Later rotated-AABB correction: approximately0.130mm bound correspondence. Original fused fan/support solids now have measured rotor axes and capped-clearance derivatives in `catalog/tiger_workshop_cooling.v1.json`. |
| `34F13_FAN COVER`, `34F14_FAN COVER` | `Hull_1115`, `Hull_1116` | Upper fan cover meshes; ~0.30–0.32 mm bound discrepancies. |
| `34F12_FAN COVER`, `34F11_FAN COVER` | `Hull_1125`, `Hull_1126` | Lower fan cover meshes; <0.47 micrometre bound discrepancy. |
| `21K49_BOX` | `Hull_0342` (334, 9,178 triangles) | Long forward transmission-region casing, bounds match <0.067 micrometre. The supplied screenshot's selected CAD object is `21K49_BOX`; exact internal gearbox type still needs reference confirmation. |
| `21K57_BOX` | `Hull_0355` (347, 10,946 triangles) | Upper front casing, bounds match <0.218 micrometre. |
| `21K51_BOX` | `Hull_0085` (91, 7,884 triangles) | Transverse front casing, bounds match ~3.44 micrometres. |
| `21K54_BOX` | `Hull_0145` (151, 6,254 triangles) | Wider front casing, bounds match ~50.8 micrometres. |
| `21K59_COVER` and `.001` | `Hull_0137`, `Hull_0138` | Paired large side covers, ~0.354 mm bound discrepancy. |
| `22K80_SHAFT` | `Hull_0524` (516, 1,924 triangles) | Forward driveshaft segment, ~34 micrometre bound discrepancy. |

`Hull_0342` spans `(-0.329262,0.749708,1.135919)` to `(0.245195,1.366587,2.503951)` in hull metres. `Hull_0355` spans `(-0.159182,1.317937,1.239057)` to `(0.200437,1.932870,2.194538)`. The source labels are generic BOX/COVER: the mappings identify *which authored mesh* to work on, not a validated mechanical diagram.

## Additional work-lamp support candidate

A source-supported clamp can sit on `tiger_turret.glb`, `Turret_0099`, node 77, triangle 426. The exact triangle centroid and orthonormal mounting basis in turret metres are:

```text
surface = (-1.1780822583, 2.1010172687, 0.0941548321)
right   = (-0.0854902674, 0, -0.9963390057)
up      = (0.0020542205, 0.9999978746, -0.0001762612)
normal  = (0.9963368880, -0.0020617687, -0.0854900857)
```

The normal faces into the cabin. A 0.08 × 0.12 m clamp footprint was sampled with 5 × 7 exact source rays, each starting 2 mm off the supporting plane. All 35 find supporting source geometry with matching normal; maximum depth variation is 0.1645 mm. A continuous six-plane polygon clipping test against all 372,655 source turret triangles found **zero** intersections in that footprint's inward prism between 2 and 302 mm from the mounting plane. Two triangles passed the broad AABB test and both were clipped away. The closest existing annunciator centre is 0.5214 m away; the current crew-status panel centre is 2.339 m away.

The existing runtime torch normalization was independently reproduced from `hand_prop_models::load_hand_torch`, including its embedded emissive-atlas threshold, area-weighted lens centre/normal and radius-based scale. Source `assets/pizza/_extracted/PSX Mega Pack 3.1.3/PSX Mega Pack/Models/GLB (recommended)/Items & Weapons/flashlight_mp_1.glb` has SHA-256 `90aa1087f7a97aeb486f3e0257919800b0dced873e79f387c1c2e390189eaf2f`. It separates into 235 body triangles and 47 lens triangles. Its normalized local bounds are:

```text
body min = (-0.0176098406, -0.0179723246, -0.0693724165)
body max = ( 0.0178227512,  0.0175325273,  0.0349004135)
lens min = (-0.0145167767, -0.0149075899,  0.0330811602)
lens max = ( 0.0147652636,  0.0143730911,  0.0367883003)
```

Use torch local +Z along the mounting normal, +X along `right`, +Y along `up`, and origin `surface + normal * 0.0723724165`. The backmost body point then has exactly 3 mm wall clearance. The lens's normalized centre plane is 0.1073724165 m from the wall; its furthest point reaches 0.1091607168 m. The entire source body/lens fits inside the continuously checked empty prism. A 2 mm base plate within the supported 80 × 120 mm footprint is source-supported; narrow cradle/retainer pieces still need to fit the actual body surface. The bounds must not be mistaken for a cylindrical body or used to hide an unsupported stand-off.

This establishes source support and static closed-turret clearance, not a guarantee against moving crew, gun elevation/recoil, or an arbitrarily rotated torch. Mounting the torch origin directly on the wall would bury the rear body. The nearby triangle 430 is **rejected** despite supporting the footprint: its inward prism reaches the side-port assembly within 49 mm. No additional light or fixture was added by this audit.

## Bounded implementation checkpoint

The first optical/track/lamp loop is now present in source. Build26 healthy optical service and
build27 damaged replacement passed their actual-input reviews (build27:51checks). Other parts need
separate gameplay evidence. It partitions the exact seven shoes, moves original and
replacement source objects with durable IDs, switches source contact/acoustic frames, rebinds
mounted optics, conserves fitted track shoes, and installs a finite battery light. Whole-world
save ownership includes partial procedure state and transient input/tool clocks. This does
not complete the heavy powertrain loop below. Read `PHYSICAL_TRUTH_AUDIT.md` for the current
hand-manipulation, placement, fragment and ballistic limits.

## Remaining implementation steps for heavy assemblies

1. Partition the identified source roots at load time while retaining their existing material, transform and triangle sum. Keep the authored source files unchanged. Prove that recombining the closed assemblies reproduces the current hull/turret.
2. Give each removable assembly a durable item/component identity, finite location, condition and mount state. Removal must leave the actual old part in the world; fitting a replacement must consume that particular physical item's mounted availability.
3. Register cover pivots or removal trajectories from the actual hinge/fastener geometry. Establish accessible paths, required released attachments and support/lifting needs before a heavy assembly can move. Opening a lid must open the same collision, ballistic and light-transport aperture.
4. Update exact scene contact/ballistic and optical references to the articulated transforms. A renderer-only transform or a retained invisible casing is insufficient.
5. Add causal subsystem state for the chosen supported repairs/upgrades: coolant quantity and flow, heat transfer and fan drive; engine operation and load; transmission engagement and torque transfer. Validate changes under identical ordinary driving/combat input. Existing binary module health is not this simulation.
6. Author compatible replacement variants and document their period/configuration and actual tradeoffs. Current two-filter engine geometry does not justify an HL210→HL230 reward applied to an already HL230-style vehicle. A new name or scalar power bonus is not a new mechanical part.
7. Validate access, removing, retaining, carrying, fitting, securing, testing and save/resume during each intermediate state in the real game. This audit has not run those gameplay or rendering checks.

The tractable first physical loop is source-conserved optics, track spares and a supported work lamp. The full model makes further mechanical work possible, but it does not already supply semantic registration, hinge axes, a thermal model, internal failure states, or player-facing assembly procedures.

## Engine and coolant access checkpoint before build29

The cooling source preparation is recorded in `catalog/tiger_workshop_cooling.v1.json` and
`tools/prepare_tiger_cooling.py`. Original hull files remain unchanged. Two fused fan/support
solids now have capped rotor/stator derivatives with a measured minimum1.482488mm radial gap;
approximately1.21357% of their solid volume was removed. This permits source rotor rotation,
not cassette removal. Every cassette and grille service action remains blocked because a
straight lift intersects actual surrounding source geometry. The thermal/accessory coupling
is held behind one release gate until finite replenishment and failure recovery are playable.

Engine-cover preparation is still confined to `artifacts/workshop-source-20261030/`:

- Main cover `Hull_0975` has a measured +X hinge axis through
  `(0,2.00876402,-2.62762745)` metres. Its14 sampled bore points fit radius15.5678724mm
  within0.185 micrometres. Negative X rotation raises the front of the cover.
- Side locks `Hull_0932` and `Hull_0933` have measured +Z axes through
  `(-0.60617611,2.06130581,-1.74025518)` and
  `(0.60812212,2.06130581,-1.74025518)`. The left rotates positive Z and right negative Z
  to move outward. These axes do not by themselves certify an unobstructed full motion.
- A candidate moving-lug relief is closed and measurable, but still intersects fixed deck
  `Hull_0708` at the outer knuckles. It is not an approved live derivative. Source detail has
  not established that those outer ears are stationary, so no fictitious fixed copies were made.
- Rear buckle `Hull_1068` combines its stud/base and arm in one connected solid. Rotating
  the entire named object moves its mounting stud through the lid; it needs a real geometric
  partition before articulation. A source object's name does not define its moving assembly.
- Right turret poses64–70 degrees, with the rear spare shoe removed, still find source
  carrier plates/brackets obstructing the cover. Source preparation must account for those
  real attachments. Arbitrarily hiding their collision is not an acceptable release path.

The runtime vehicle's left is **positive X**. The left coolant filler candidate lies above
`Hull_1010` (node986), under removable outer plug `Hull_1012` (988). Its top cap plane is centred
at `(1.14813446,2.02784733,-2.20293010)` metres, with normal approximately
`(0,0.99967946,-0.02531742)`. A vertical ray finds the outer plug and then the inner cap without
intervening hull plate. However, that inner cap is fused into the radiator source at a1µm weld:
there is no open filler mouth yet. A finite pour must wait for a measured cap/neck derivative;
fluid cannot be granted through a closed visible cap. Negative-X `Hull_1009`/`Hull_1011` is
the opposite side and must not be selected by an ambiguous CAD filename alone. Functional
identification remains provisional: the1943 D656/21 draft names the filler on the left in the
driving direction, whereas1944 HL230 D656/30 b+c names it on the right without an explicit
viewpoint in that paragraph. The measured source cap candidate does not resolve that primary
source conflict. No live coolant fill point has been enabled from this incomplete identification.

These are access blockers and preparation evidence, not a completed engine overhaul,
recommission, coolant refill, or live cooling upgrade. Build27 proved51 actual optical-service
and damaged-replacement checks. The build29 lamp witness and coupled-release regression are
pending the root agent's next serialized build and review.

## Engine access source package after build30

This supersedes the cover-preparation blockers above; it does not certify a playable engine
exchange. `catalog/tiger_engine_service.v1.json` and
`tools/prepare_tiger_engine_service.py` now describe fifteen verified private GLBs in
`assets/tanks/tiger/workshop_engine/`. Thirteen are active geometry; the two original side-lock
copies are provenance only. Every admitted mesh is finite, closed and consistently wound at
the stated 1 µm topology weld, with no collapsed edges or degenerate triangles. The original
hull and turret hashes remain unchanged. Existing source materials and UVs are preserved.

The active moving assemblies are `lid`, `left_lock`, `right_lock` and `rear_latch`. Their
parent hierarchy and actual source contact triangles are in the catalog. The lid includes
both handles, the two attached covers, the two side-lock blocks and the rear latch pedestal.
The side locks and rear latch inherit lid motion after their own release rotations.

- The cover uses a bounded moving-knuckle derivative: 16 mm lug bores, a 54.1 mm heel
  envelope and four local outer-knuckle notches. It removes 0.583424% of the original
  leaf's solid volume. Fixed deck `Hull_0708` is unchanged.
- Both side-lock pins retain their full source length and 288 triangles. Their measured
  9.72996 mm source radius is reduced to 9.40 mm, a maximum vertex movement of 0.330 mm.
  This supplies a conservative 0.24656 mm clearance within the polygonal source bore.
  All 146 unique vertices per lock pass the 1° trajectory probes to ±110°.
- The fused rear latch becomes a fixed pedestal/journal and a moving clevis arm. The final
  combined geometry removes 7.96358% of the original latch volume. The moving head's
  maximum local reshaping is 11.1975 mm, explicitly an authored reconstruction rather
  than unchanged historical geometry. Its 1,461 unique vertices pass the 0–90° probe.

Large carrier-plate cuts were rejected: the candidate cuts removed 38–82% of the plate's
volume. Instead, two complete native source objects are exported unchanged and must remain
finite physical items when removed:

| Service part | Original root / triangles | Actual source contact | Measured withdrawal |
|---|---|---|---|
| `carrier_plate` | `Turret_0132`, 376 triangles | Triangle105, turret metres `(-1.487083,1.759820,-0.054243)` | 160 mm opposite the two studs' shared axis: delta `(-0.159994,0,-0.001341)` |
| `lower_retainer` | `Turret_0153`, 1,346 triangles | Triangle339, turret metres `(-1.468839,1.591056,-0.403291)` | Lift 60 mm in +Y, then pull 150 mm outboard in −X |

The plate path requires both front spare shoe `Turret_0128` and rear shoe `Turret_0147`
absent. The wing-retainer path was also checked with both absent; the earlier probe with
only the rear shoe absent was clear. These are component-level withdrawal paths, with no
thread-pitch claim. The estimated masses are 1.29341 kg and 0.243623 kg, respectively,
using closed source volume times an assumed steel density of 7,850 kg/m³. Exported source
positions differ by at most 0.060 µm due to float storage; triangle indices, material and
the measured contact centroids match the original source.

The combined lid assembly has 11,344 unique vertices. At turret yaw −64°, −65°, −66°,
−67° and −68° (64–68° right), 1° lid steps from closed to −90° find only sixteen
separating initial fitted-seam contacts below 0.3 µm at the deck. There are no other vertex
trajectory hits with `0147`, `0132` and `0153` physically absent. Front shoe `0128` remained
present in this particular lid probe, making that check conservative relative to the required
plate-removal state.

**These are vertex-trajectory probes, not full-face or continuous solid sweeps.** The catalog
keeps `service_enabled:false` and `runtime_full_face_admission_pending:true`. The runtime
must validate the complete ordered removal sequence, conservative swept-face admission,
retained item identities, actual access and return/reinstallation. Source preparation does
not establish that removing the shoes before their carrier hardware is a fully playable
sequence. This kit also needs more removable hardware than the surveyed real-Tiger opening
procedure; that discrepancy is not presented as historical fact. The main engine remains
unmodified and no engine lift or repair has been admitted by this package.

The exact probe receipts are listed in the catalog. The packer's `--verify` command validates
all fifteen file hashes and geometry checks against the unchanged original source hashes;
the four engine-preparation Python tools pass syntax compilation. No Cargo or GPU check was
run for this source package by the source-audit agent.


## Unregistered major-service preparation and CPU33 latch finding

The root agent enabled the existing lid package for runtime source-admission testing in
build32; this supersedes the earlier source-package paragraph saying that its gate remains
false. This note does not claim that the ordered runtime operation has passed.

New, unregistered source work is recorded in `catalog/tiger_engine_coupling.v1.json` and
`catalog/tiger_engine_major_service_cassette.v1.json`. These do not alter the original
baked hull/turret or the registered lid assets. Their private outputs live in
`assets/tanks/tiger/workshop_engine_core/`.

- The rear driveline coupling is split at the modeled neck of source0671, with a visible
  0.5mm seam and short sleeve socket. Both closed parts have zero topology defects. All
  1,120 unique coupling vertices pass a20mm forward release while the finite coupling
  remains supported on the fixed shaft. This is an axial release, not full removal.
  Driveline torque must be interrupted in that state; engine idle is still possible.
  Initial top-contact access failed all nine sampled rays. A later surface scan finds
  alternate exterior faces visible from low central fighting-compartment samples, but
  turret occlusion and actual player/body access are not yet certified.
- An explicitly authored rear roof/hinge cassette separates a T-shaped portion of0708,
  carries the complete100-degree-open lid and native1136 cover, and retains the roof tongue
  beneath that cover. Fixed deck43,108triangles and cassette5,408triangles are both closed;
  the seam and local underside fit remove0.0176504% of original deck volume. All moving
  vertices pass a250mm lift except five departing initial rear-hull seams bounded by1.47µm.
  Full static triangle checks at1,10,50,100,200 and250mm are clear. This is not a continuous
  full-face sweep. Estimated complete mass is961kg from model solid volumes and assumed
  steel density, not a measured historical assembly mass. Visible retainers and a rated
  lifting fixture are still required; existing hand handles have not been certified as
  lifting eyes. The cassette service gate remains false.
- A bounded main-engine foot fit preserves floor0619 and removes6.245e-6m³ from source0971,
  or0.001013% of its solid volume. Two pre-existing zero-area faces are dropped. Local
  Boolean cleanup, two T-junction splits and two small cut-face caps yield a closed65,424-
  triangle derivative. This corrects source fit only; it does not certify engine extraction.
  Base-unit collars1084/1085, the coupled-shaft area and other physical attachments remain
  extraction constraints. No whole-engine exchange has been admitted.

CPU33's real swept-face test found what the earlier vertex paths missed: moving left-lock
triangle52 shares its axial seating plane with fixed `native_hull_0930.glb` triangle113.
The obstacle coordinates match that exported triangle exactly. A candidate insets both
moving-wing end planes by150µm and translates each adjoining pin section inward by the same
amount, preserving pin axial length. Both candidates retain288triangles and closed topology;
all full static triangle checks at0.25-degree intervals through110degrees are clear.
`tools/prepare_engine_lock_axial_clearance.py` and the staged
`engine_side_lock_axial_candidates.json` receipt record the bounded change. At this checkpoint
these candidates have not replaced the registered assets, and the runtime continuous sweep
has not been rerun. No collision-contact exception was introduced.

The root subsequently authorized promotion of the two150µm axial-fit derivatives. Only
`engine_left_lock.glb` and `engine_right_lock.glb` changed in the registered asset folder;
the catalog records their new hashes, topology, changed source contact centroids and
measured axial-change receipt. The service flag and all other geometry are unchanged.
The original native lock copies and earlier staged radial-fit candidates remain intact.
Continuous runtime admission awaits the root agent's rerun of the compiled CPU33 test.


CPU33 then passed both side locks and found genuine rear-arm face interference with
`engine_latch_base.glb` triangle1434. The prior nonlinear shaping had left long clevis
faces crossing the journal despite vertex clearance. Re-bore and tessellation attempts
were rejected when they failed topology or full-face intersection checks. The promoted
replacement is direct CSG from untouched source1068: the same19.7mm arm bore, central
clevis slot and23.8mm outer head clearance envelope, with no nonlinear vertex deformation.
It is4,222triangles, closed and defect-free, with zero full static triangle intersections
at0.25-degree steps through90degrees. Fixed base, hinge axis and lever contact remain
unchanged. The active base+arm now removes5.70543% of source1068 solid volume; this
supersedes the old7.96358% deformation figure. Only registered `engine_latch_arm.glb`
changed for this correction; the catalog and packer record its new source/CSG receipt and
hash. The root agent will rerun the actual conservative continuous sweep. No global
collision waiver was introduced. Registered engine-access assets are frozen forbuild34.


The parent reports that compiled CPU33 subsequently passed all three latch operations
through ordinary conservative substeps in6.53s. CPU34's complete lid stroke then failed
at the initial deck-edge seating contact: moving leaf triangle441 versus original0708
triangle25631, the front-right chamfer. A complete source contact audit finds348 initial
leaf/deck intersection pairs, all bounded by0.205µm underside overlap when compared in
double source coordinates. Each pair has a finite cardinal support plane separating the
whole triangles within the existing38µm source precision bound, and every moving vertex
has nonnegative local departure velocity along an available separating axis. Counts fall
to52 at0.001–0.1degrees and zero at0.25degrees. No underside cut was made; the runtime
owner is assessing a bounded analytic finite-plane departure proof. The exact record is
`artifacts/workshop-source-20261030/lid_initial_finite_contact_probe.json`; it is local
and static evidence, not itself a continuous swept-face proof.

The proposed intake-removal access alternative was tested separately while registered
assets remained frozen. Fifty standing/crouched rear-deck eye samples see no coupling
faces with source0916 absent, with0916/1006 absent, or with those plus1074/1075 absent.
Only these explicitly identified prospective upper-component removals were excluded;
shaft-support0838 and the main engine remained. The native filters also show initial0955
seating interference during a straight lift, clearing by50mm, so that motion is not admitted.
The runtime's additional solid turret floor and its decorative inspection-cover box do not
supply an existing access opening. No supported standing repair approach, exact removable
floor cover, or valid prone body pose has been established for the coupling.
