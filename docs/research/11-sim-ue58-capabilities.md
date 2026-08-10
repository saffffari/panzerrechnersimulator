# 11 — UE 5.8 Simulation Capabilities for Tank Realism

Status: RESEARCH (design only; no C++, no editor launch, no build this phase).
Engine: UE 5.8.0 at `C:/Program Files/Epic Games/UE_5.8`.
Method: every capability below is confirmed against an **on-disk path** under the installed engine
(source headers and `.uplugin` manifests are PRIMARY truth). Plugin maturity flags are read directly
from each `.uplugin`. Anything not provable on disk is tagged **ASSUMPTION**.

Governing pillar: *Deep simulation, abstracted presentation.* This doc catalogs what UE 5.8 can
**simulate** in service of tank realism (the systems), independent of the deliberately abstracted
look. Items are scored for **simulation/feeling payoff per unit effort** and triaged M1 / post-M1 /
avoid. Audio is covered only where physics feeds it (collision/impact events → acoustic triggers);
the acoustic engine itself lives in `AUDIO_ARCHITECTURE.md` and its own research doc.

---

## 0. Maturity legend (read from each `.uplugin` on disk)

| Tag | Meaning | Build risk for Kessel |
|---|---|---|
| **Stable / in Engine core** | Lives in `Engine/Source/Runtime/...`, no plugin enable needed, no maturity flag | None — always available, ships Win+Linux |
| **Beta** (`"IsBetaVersion": true`) | API mostly settled, may shift | Low — usable, pin behavior, expect minor churn |
| **Experimental** (`"IsExperimentalVersion": true`) | API unstable, may change/break | Medium — prototype only, do not hard-depend for ship features |
| **EnabledByDefault:false** | Must be turned on in `.uproject`/plugin browser | Trivial (a flag), but note it |

Reminder (env): MSVC/VS is still installing, so anything requiring a **C++ recompile of a community
plugin** cannot be validated this phase. Every capability in this doc is **in-box** (engine or shipped
engine plugin), so none needs a third-party recompile — but Experimental in-box modules still carry
API-churn risk and several need their plugin enabled.

---

## 1. Chaos physics — rigid bodies, forces, substepping (the hull as a body)

**Maturity: Stable engine core** (the solver plugin `ChaosSolverPlugin` is Beta/EnabledByDefault, but
the rigid-body + force API used for the tank is plain `UPrimitiveComponent`/`PhysicsSettings` engine
core — no plugin enable required).

This is the spine of the tank. The 57 t hull is a single Chaos rigid body (`HullMesh`,
`SimulatePhysics=true`), driven by per-substep forces from the custom track layer (ARCHITECTURE §3).

Confirmed force/impulse API on `UPrimitiveComponent`
(`Source/Runtime/Engine/Classes/Components/PrimitiveComponent.h`):
- `AddForce` `:1759`, `AddForceAtLocation` `:1770`, `AddForceAtLocationLocal` `:1781` — tractive +
  suspension forces at contact points.
- `AddTorqueInRadians` `:1802` (degrees overload `:1813`) — yaw/pivot moment.
- `AddImpulse` `:1692`, `AddRadialImpulse` `:1748`, `AddRadialForce` `:1793` — recoil kick on firing,
  blast shove from a near-miss HE, collision impulses.

Substepping (mandatory for a heavy body under small per-tick forces, else track forces feel mushy —
ARCHITECTURE §3 / §13.7) is configured in
`Source/Runtime/Engine/Classes/PhysicsEngine/PhysicsSettings.h`:
- `bSubstepping` `:325`, `bSubsteppingAsync` `:329`, `MaxSubstepDeltaTime` `:341`,
  `MaxSubsteps` (clamp 1–16) `:344-345`.

Async physics tick (run gameplay force code on the physics thread at substep cadence, deterministic):
- `Source/Runtime/Engine/Public/Physics/AsyncPhysicsInputComponent.h`,
  `.../Physics/AsyncPhysicsData.h`. `UActorComponent` exposes async-physics-tick registration
  (`Source/Runtime/Engine/Classes/Components/ActorComponent.h`). Low-level Chaos physics-object access:
  `Source/Runtime/Experimental/Chaos/Public/Chaos/PhysicsObjectInterface.h`.

**Payoff: critical, foundational.** Effort: medium (the track layer is custom; the rigid body itself
is free). **M1.** This is already the chosen movement spine (ARCHITECTURE §3, doc 03).

---

## 2. Chaos Destruction / Geometry Collection / Fracture (wrecks, blown-off components)

**Maturity: BETA + EnabledByDefault:false — must enable, treat as prototype for ship.**

`.uplugin` facts on disk:
- `Plugins/Experimental/GeometryCollectionPlugin/GeometryCollectionPlugin.uplugin` —
  `"IsBetaVersion": true`, `"EnabledByDefault": false`. Runtime component:
  `Source/Runtime/Experimental/GeometryCollectionEngine/Public/GeometryCollection/GeometryCollectionComponent.h`.
- `Plugins/Experimental/Fracture/Fracture.uplugin` — `"IsBetaVersion": true`,
  `"EnabledByDefault": false` (the editor-time fracture tools: Voronoi/cluster/slice).
- `Plugins/Experimental/PlanarCutPlugin/PlanarCutPlugin.uplugin` (planar/slice fracture math).
- `Plugins/Experimental/ChaosSolverPlugin/ChaosSolverPlugin.uplugin` — `"IsBetaVersion": true`,
  `"EnabledByDefault": true` (the destruction solver itself ships on).
- `Plugins/Experimental/ChaosCaching/ChaosCaching.uplugin` — **Experimental** — record/replay a
  destruction sim to a Chaos Cache asset (bake a hero wreck collapse, replay cheaply).

What it buys tank realism:
- **Enemy-tank wrecks**: a destructible turret/hull that fractures into clustered pieces on
  catastrophic kill (ammo cook-off blowing the turret off is the iconic WW2 image). Geometry
  Collection supports clustered hierarchical fracture with per-cluster **damage thresholds** and
  internal-strain breaking, so a hit knocks off a track guard / fender / stowage without shattering the
  whole vehicle.
- **Blown-off components**: fenders, track links, tools, the commander's cupola — model as separate
  low-cluster Geometry Collections released on local damage.
- **Static-scene destruction**: shattered masonry, splintered fences, blasted snow berms as the tank
  shoulders through — fits the Stalingrad-rubble fiction.

Caveats for the abstracted look: destruction is a *systems* win (it reads as consequence and weight),
and the abstracted style is forgiving — chunky low-poly fracture pieces are on-style, and the whiteout
hides them quickly. But it is **Beta + must-enable**, GPU/CPU heavy at high cluster counts, and bakes
a sizable asset pipeline (fracture authoring in-editor). **Do not let it onto M1's critical path.**

**Payoff: high feeling, medium-high effort + Beta risk.** **Post-M1** (one hero wreck-on-kill is a
strong early showcase; full destructible environment is later). For M1, a swapped static "wreck mesh"
+ smoke is a cheap stand-in that costs nothing and survives the whiteout.

---

## 3. Chaos Fields (physics fields driving destruction & forces)

**Maturity: BETA + EnabledByDefault:false.**

`.uplugin`: `Plugins/Experimental/FieldSystemPlugin/FieldSystemPlugin.uplugin` —
`"IsBetaVersion": true`, `"EnabledByDefault": false`.
Runtime: `Source/Runtime/Experimental/FieldSystem/Source/FieldSystemEngine/Public/Field/FieldSystemActor.h`
and `.../FieldSystemComponent.h`. Field types:
`Source/Runtime/Experimental/Chaos/Public/Field/FieldSystemTypes.h` — `EFieldPhysicsType` exposes
`Field_DynamicState` `:151`, `Field_LinearForce` `:152`, `Field_ExternalClusterStrain` `:153`,
`Field_LinearVelocity` `:155` (and more).

What it buys: a **spatial field** that drives Geometry Collections — e.g. a `Field_ExternalClusterStrain`
sphere at a shell impact to fracture exactly the struck region; a `Field_LinearForce`/`LinearVelocity`
radial field to blast debris outward from a penetration or HE burst; `Field_DynamicState` to wake/anchor
pieces. This is the clean way to make a hit *locally* shatter armor skirts or scatter a wreck's pieces
in the blast direction, rather than uniformly.

**Payoff: medium (only matters once §2 destruction is in), low-medium effort.** **Post-M1**, paired
with §2. Strictly a companion to Geometry Collections — no value without them.

---

## 4. Niagara (debris, smoke, snow, muzzle, GPU sims reading collisions)

**Maturity: Stable, EnabledByDefault:true** (`Plugins/FX/Niagara/Niagara.uplugin` —
`"IsBetaVersion": false`, `"EnabledByDefault": true`). Already a default-on M1 module (ARCHITECTURE §1).

Core uses for the sim/feel:
- **Blizzard** (camera-anchored, reads vehicle velocity — ARCHITECTURE §8), **muzzle flash/smoke**,
  **exhaust**, **embers/light-motes** drifting through fog, **track snow-spray**, **impact sparks/spall
  cones** on ricochet, **dust/snow kicked by movement**, **barrel-heat shimmer**.
- **GPU simulation** for cheap high-count particles (snow, embers, spall) on the 4090 — Niagara
  supports CPU and GPU sim targets natively.

GPU sims that **read the physics world** (this is the part that serves simulation, not just dressing) —
data interfaces confirmed in `Plugins/FX/Niagara/Source/Niagara`:
- `NiagaraDataInterfaceCollisionQuery.h` — particles **trace against scene collision** (snow/spall
  bounces off the hull and ground, embers collide). `Classes/NiagaraDataInterfaceCollisionQuery.h`.
- `Public/NiagaraDataInterfacePhysicsAsset.h` and
  `Public/NiagaraDataInterfaceRigidMeshCollisionQuery.h` — collide/sample against a skeletal/rigid mesh
  (snow accreting on or sliding off the moving tank body).
- `Classes/NiagaraDataInterfaceLandscape.h` — sample terrain height/normal (snow settles on ground
  contour; track spray follows the surface).
- Grid data interfaces (`NiagaraDataInterfaceGrid2DCollection.h`, `Grid3DCollection.h`,
  `NiagaraDataInterfaceRenderTarget2D.h`, `IntRenderTarget2D.h`) — GPU fields for volumetric smoke or a
  **persistent snow-deformation/accumulation render target** the tracks write into (see §10).

**Chaos → Niagara bridge** (secondary destruction FX driven by the solver) — plugin
`Plugins/Experimental/ChaosNiagara/ChaosNiagara.uplugin` (`"IsBetaVersion": true`,
`"EnabledByDefault": true`). Data interfaces:
- `.../ChaosNiagara/Classes/NiagaraDataInterfaceChaosDestruction.h` — spawn dust/sparks **from actual
  fracture events** (break locations, impulses) of a Geometry Collection.
- `.../ChaosNiagara/Public/NiagaraDataInterfaceGeometryCollection.h` and
  `.../Classes/NiagaraDataInterfacePhysicsField.h` — read collection state / physics fields into FX.

**NiagaraFluids** (`Plugins/FX/NiagaraFluids/NiagaraFluids.uplugin` — `"IsBetaVersion": true`):
GPU gas/smoke fluid sim. Tempting for billowing engine/wreck smoke, but heavy and Beta. **Avoid for
M1** — flat scrolling smoke cards into the fog read fine under abstraction; revisit a single hero
fluid-smoke column post-M1 only if budget allows.

**Payoff: very high feeling, low-medium effort** for the standard emitters (M1, already planned).
The collision-reading GPU sims are a **medium payoff, medium effort post-M1** polish that makes snow
and spall feel physical.

---

## 5. Physical materials & surface types (snow/ice/mud friction → mobility)

**Maturity: Stable engine core.**

This is how the *ground* becomes a simulation input rather than a texture — central to mobility feel
in winter.
- `UPhysicalMaterial` — `Source/Runtime/Engine/Classes/PhysicalMaterials/PhysicalMaterial.h`
  (friction, restitution, density). `PhysicalMaterialMask.h` (per-texel surface variation across one
  mesh — e.g. ice patches vs packed snow on one ground tile).
- `EPhysicalSurface` enum — `Source/Runtime/PhysicsCore/Public/Chaos/ChaosEngineInterface.h:20`
  (`SurfaceType1..SurfaceTypeN`, `:23+`). User-named surface slots are declared in
  `Source/Runtime/Engine/Classes/PhysicsEngine/PhysicsSettings.h` (`PhysicalSurfaces` array) — define
  `Snow_Deep`, `Snow_Packed`, `Ice`, `Mud`, `Frozen_Ground`, `Rubble`, `Road`.

How it drives mobility (ties into the custom track layer, ARCHITECTURE §3): each roadwheel down-trace
(`LineTraceSingleByChannel`, §6) returns `FHitResult.PhysMaterial`; the track layer reads its
`EPhysicalSurface` to pick **per-surface longitudinal/lateral friction (muLong/muLat)** and rolling
resistance. Deep snow = low grip + high sink/resistance (sluggish, bogging); ice = near-zero lateral
grip (the tail slides on a pivot, terrifying on a slope); mud = grip but high drag. This makes the
*same* throttle feel completely different by ground — exactly the "deep simulation" mandate, and it is
nearly free because the traces already exist for suspension.

Surface type also routes **audio** (track/squeak/crunch per surface — Pillar 3) and **VFX** (snow
spray vs mud spatter vs sparks on rubble) from one shared lookup. One surface enum feeds physics +
sound + particles.

**Payoff: very high simulation, low effort.** **M1-adjacent** — wire it the moment the Phase-2 track
layer lands (ARCHITECTURE §11 step 12). Cheapest large realism gain in the locomotion stack.

---

## 6. Line / sweep traces (ballistics, line-of-sight, suspension, spotting)

**Maturity: Stable engine core.** `Source/Runtime/Engine/Classes/Engine/World.h`:
- `LineTraceSingleByChannel` `:2161`, `LineTraceMultiByChannel` `:2210` (multi = pass-through ordering
  for spall/overpenetration), `SweepSingleByChannel` `:2286` (thick-projectile / volume sweep,
  anti-tunneling for the 800 m/s round — ARCHITECTURE §4 path B), `OverlapMultiByChannel` `:2418`.

Tank uses, all core and free:
- **Ballistics**: per-substep trace along the analytic trajectory → `FHitResult` gives hit component,
  bone, location, normal, surface → feeds `UKesselBallisticsSubsystem::ResolveKineticHit` (ARCHITECTURE
  §4). Multi-trace handles the fragment/spall cone behind a penetration.
- **Suspension**: per-roadwheel down-trace for spring compression (ARCHITECTURE §3).
- **Line-of-sight / spotting**: trace eye→target through the fog to gate "can the gunner actually see
  it," and to decide occlusion for audio (a trace from sound source to the player ear → muffle if the
  hull/terrain blocks — Pillar 3, cross-ref audio doc).
- **Custom collision channels** (`ECollisionChannel`, declared in `PhysicsSettings.h`): a dedicated
  `Projectile` and `Visibility/Optics` channel keeps ballistics and LOS traces cheap and unambiguous.

**Payoff: critical, low effort.** **M1** (already the ballistics + suspension spine).

---

## 7. Physics constraints (suspension, tracks, recoil, hatches)

**Maturity: Stable engine core.** `Source/Runtime/Engine/Classes/PhysicsEngine/`:
`PhysicsConstraintComponent.h`, `ConstraintInstance.h`, `ConstraintDrives.h`, `ConstraintTypes.h`,
`ConstraintInstanceBlueprintLibrary.h`, `PhysicsAsset.h`, `BodySetup.h`, `AggregateGeom.h`.

Where constraints help tank realism:
- **Recoil**: a prismatic/linear-drive constraint on the gun barrel body gives a real recoil
  stroke-and-return (the breech slams back, returns on the recuperator) instead of a faked animation —
  a strong tactile detail seen down the gunner's optic.
- **Hatch / cupola / mantlet**: hinge constraints for physically swinging hatches (head-out
  transition) and a constrained mantlet.
- **Suspension (alternative path)**: torsion-bar suspension could be modeled as constrained roadwheel
  bodies. **However** ARCHITECTURE §3 deliberately chose ray-cast suspension (line traces + analytic
  spring) over a full constrained-bodies rig for stability and tuning simplicity on a 57 t hull — keep
  that decision; constraints here are a *later* fidelity option, not M1.
- **Tracks**: a fully simulated link-by-link constrained track is **not** advisable (huge solver cost,
  instability) — the design correctly fakes track *behavior* with forces (§1) + a scrolling-texture/
  bone visual. Constraints are for the gun and hatches, not the track.

Caveat: `ChaosCloth`/`ChaosClothAsset` (`Plugins/ChaosCloth`, `Plugins/ChaosClothAsset`) exists for
simulated cloth — tarps, tow ropes, a hanging map — but it is niche for an interior-only tank view and
not worth M1 effort.

**Payoff: medium (recoil = high tactile feel; rest minor), low-medium effort.** **Post-M1** —
recoil-stroke constraint is a satisfying early-polish item; hatch hinges optional.

---

## 8. Scene capture & post-process (optics, periscope PiP, thermal substrate)

**Maturity: Stable engine core.**
- `Source/Runtime/Engine/Classes/Components/SceneCaptureComponent2D.h`,
  `SceneCaptureComponent.h`, `SceneCaptureComponentCube.h`; render targets in
  `Source/Runtime/Engine/Classes/Engine/TextureRenderTarget2D.h`.
- `ESceneCaptureSource` enum — `Source/Runtime/Engine/Classes/Engine/EngineTypes.h:534`: `SCS_SceneColorHDR`
  `:536`, `SCS_FinalColorLDR` `:538`, `SCS_DeviceDepth` `:541`, `SCS_Normal` `:542`, `SCS_BaseColor`
  `:543`. (Depth/normal/base-color captures are the raw substrate for a faux-thermal or rangefinder
  overlay.)
- `Source/Runtime/Engine/Classes/Components/PostProcessComponent.h` — volume-less global post; per-optic
  post lives on each `UCameraComponent.PostProcessSettings` (ARCHITECTURE §5).

Tank uses:
- **Optic look** (cold tint, vignette, chromatic aberration, reticle glass) is best done on the
  **camera's `PostProcessSettings`**, *not* a SceneCapture — ARCHITECTURE §5 already **rejects**
  `USceneCaptureComponent2D` as the default optic because it re-renders the scene (≈2× cost). Keep that
  ruling. The three first-person cameras + FOV stepping deliver zoom for free.
- **SceneCapture is justified only for picture-in-picture**: a separate magnified image inside the
  commander's periscope, a rear-facing episcope, or a (post-M1) thermal/IR sub-display — a genuine
  second view that a post-process pass cannot fake. Budget it as an explicit, optional second render.

**Payoff: high for optics (via PostProcess, near-free); SceneCapture medium payoff / medium cost.**
Optic post = **M1** (planned). SceneCapture PiP = **post-M1**, opt-in.

---

## 9. IR / thermal-relevant features (faux-thermal, not a physical IR sim)

**Maturity: Stable engine core building blocks; no native thermal/IR simulation exists in 5.8.**

There is **no physically-based infrared/thermal-radiometry system in UE 5.8** (searched on disk —
nothing under engine source models emissive-IR or heat radiance). A convincing thermal sight is
therefore an **abstracted compositing trick**, which suits the pillars perfectly (and is historically a
*stretch* for a 1942 Tiger anyway — flag as fiction/optional, post-M1 at most):
- **Custom Depth + Custom Stencil** to tag "hot" actors and render them bright/inverted:
  `UPrimitiveComponent::bRenderCustomDepth` (`PrimitiveComponent.h:712`), `CustomDepthStencilValue`
  `:861-862`, setters `SetRenderCustomDepth` `:2092` / `SetCustomDepthStencilValue` `:2096`. A
  post-process material reads the stencil to white-hot the engine deck, exhaust, recently-fired guns,
  and crewed enemy tanks against a cold scene.
- **Per-object heat via emissive/material params or a heat render-target** (a Niagara grid / RVT, §4/§10)
  that decays over time — barrel heats when fired, engine glows with throttle, a fresh wreck cools.
- **SceneCapture depth/normal/base-color sources** (§8) as the raw greyscale substrate, then a
  thermal LUT post material.

This is **systems-cheap and on-style**: thermal becomes a stencil + post material + a scalar "heat"
value per object that *other systems already compute* (engine load, rounds fired). It's a presentation
of simulated heat state, not a new radiative sim.

**Payoff: medium feeling (cool factor), low-medium effort — but historically dubious for a Tiger.**
**Post-M1 / optional.** Mention to user as a stylistic option, not a realism claim.

---

## 10. Terrain deformation (tracks in snow) — adjacent capability

**Maturity: Stable engine core (RVT) + Beta (Niagara grids); no turnkey deformation system.**

The user's winter setting makes **track ruts in snow** a high-feel detail. UE 5.8 has no one-click
runtime terrain-deform, but the pieces exist on disk:
- **Runtime Virtual Texture** — `Source/Runtime/Engine/Classes/VT/RuntimeVirtualTexture.h`,
  `Components/RuntimeVirtualTextureComponent.h`, material nodes
  `Public/Materials/MaterialExpressionRuntimeVirtualTexture*.h`. Write a persistent snow-displacement/
  trail mask into an RVT under the tank and sample it in the ground material (WPO displacement + albedo
  darkening for compressed snow).
- **Niagara render-target/grid DIs** (§4) as the GPU writer for the deformation/accumulation field.
- **VirtualHeightfieldMesh** (`Plugins/Experimental/VirtualHeightfieldMesh`) and **Landmass**
  (`Plugins/Experimental/Landmass`) for heightfield-driven terrain (Experimental — prototype only).

Under abstraction + whiteout the bar is low: a darkened compressed-snow decal trail + light WPO dip
reads as ruts without a full deformation sim. **Payoff: medium-high feeling, medium effort. Post-M1.**

---

## 11. Vehicle-specific physics modules (context / decisions already made)

For completeness, the Chaos vehicle family on disk and why Kessel only borrows from it:
- `Plugins/Experimental/ChaosVehiclesPlugin/ChaosVehiclesPlugin.uplugin` — **Experimental**,
  `"EnabledByDefault": false`. Used ONLY as throwaway Phase-1 scaffolding
  (`UChaosWheeledVehicleMovementComponent`), then discarded — it is a *car* sim (Ackermann steering, no
  neutral-steer), unfit for a tracked pivot (ARCHITECTURE §3, doc 03). Confirmed there are zero
  `TankVehicle/TrackedVehicle/SkidSteer` classes engine-wide.
- `Plugins/Experimental/ChaosModularVehicle/ChaosModularVehicle.uplugin` — **Experimental**. A
  component-based vehicle kit; same wheeled-paradigm limitation. Not used.
- `PhysicsThruster` / `PhysicsThrusterComponent` (`Source/Runtime/Engine/Classes/PhysicsEngine/`) —
  stable core; a thruster applies a steady directional force at a point. A possible cheap building
  block for per-track tractive force, but the design's explicit `AddForceAtLocation` approach (§1) is
  more controllable — note as an alternative, not the plan.

Decision stands: **custom track layer on a Chaos rigid body (§1)**, not any packaged vehicle module.

---

## 12. Cross-cutting: one simulated state, many presentations

The pillar pays off when a single simulated quantity feeds physics + audio + VFX + (faux) thermal at
once. Concrete shared signals this catalog enables:

| Simulated signal (source) | Physics | Audio | VFX (Niagara) | Thermal/Optic |
|---|---|---|---|---|
| Ground `EPhysicalSurface` under each wheel (§5,§6 trace) | grip/drag → mobility | crunch/squeak per surface | snow vs mud vs sparks spray | — |
| Shell impact `FHitResult` (§6) + pen result (ballistics) | recoil/blast impulse (§1) | directional bang (Pillar 3) | spall cone / sparks / dust (§4) | hot impact flash (§9) |
| Engine load / RPM (movement) | tractive force | engine note (motor audio) | exhaust density | hot engine deck (§9 stencil) |
| Gun fired (gun comp) | recoil stroke (§7) | muzzle blast L/R mix | muzzle flash + smoke (§4) | hot barrel (§9 decay) |
| Geometry-collection break event (§2) | debris bodies | impact/clatter | Chaos→Niagara dust (§4) | cooling wreck (§9) |

This table is the argument for spending simulation budget once and presenting it many ways — exactly
"deep simulation, abstracted presentation."

---

## 13. Triage summary (payoff-per-effort under the pillars)

**M1 (already on the critical path or trivially adjacent):**
- §1 Chaos rigid body + forces + substepping — foundational.
- §6 Line/sweep traces — ballistics, suspension, LOS.
- §5 Physical materials / surface types — biggest cheap mobility-realism win; wire with Phase-2 tracks.
- §4 Niagara standard emitters (blizzard, muzzle, smoke, exhaust, spall) — planned default module.
- §8 Optic look via camera `PostProcessSettings` (NOT SceneCapture) — planned.

**Post-M1 (high payoff, deferred for cost/Beta-risk):**
- §2 Geometry Collection destruction — hero wreck-on-kill first (Beta + must-enable; off critical path).
- §3 Chaos Fields — only with §2.
- §4 collision-reading GPU sims + Chaos→Niagara destruction FX.
- §7 Recoil-stroke constraint (strong tactile detail); hatch hinges optional.
- §10 Snow-rut terrain deformation via RVT + Niagara grid.
- §8 SceneCapture picture-in-periscope (opt-in second render).
- §9 Faux-thermal via Custom Stencil + post material — optional, historically dubious for a Tiger.

**Avoid / not for this game:**
- NiagaraFluids gas sim for smoke (Beta, heavy — flat smoke cards read fine under whiteout).
- Fully simulated link-by-link tracks and constrained-body suspension (instability/cost; design already
  fakes behavior with forces).
- Packaged Chaos vehicle modules as the *shipping* movement (Experimental, wheeled-only; scaffolding
  use only).
- Any photoreal-chasing GPU feature that doesn't deepen a system (pillar litmus test).

---

## 14. Risks / open verification items (resolve once VS/MSVC + editor are available)

1. **Geometry Collection is Beta + EnabledByDefault:false** — confirm runtime stability and asset
   pipeline cost before committing a destructible feature; keep a static-wreck fallback.
2. **Field System / ChaosNiagara are Beta/Experimental** — API may shift; prototype only.
3. **ChaosVehiclesPlugin is Experimental** — fine as throwaway scaffolding; do not let its API leak into
   shipping movement code.
4. **Async physics tick determinism** (§1) and substep tuning for a 57 t hull — verify force stability
   in-editor (carried open item from ARCHITECTURE §13.7).
5. **No native IR/thermal** confirmed by absence on disk — any thermal sight is explicitly a compositing
   trick; do not promise radiometric realism.
6. **SceneCapture cost** — measure the second-render hit before shipping any PiP/thermal sub-display;
   the default optic must stay on plain cameras (ARCHITECTURE §5 ruling).
7. **Linux parity** — all cited features are cross-platform engine/plugin code with no platform `#ifdef`
   (consistent with ARCHITECTURE §12); confirm destruction/Niagara GPU sims behave on the Linux/Vulkan
   path at the reserved min-spec tier.
8. **Surface-type slot budget** — `EPhysicalSurface` has a finite named-slot count; reserve the winter
   set (`Snow_Deep/Packed`, `Ice`, `Mud`, `Frozen_Ground`, `Rubble`, `Road`) early in `PhysicsSettings`.

---

## 15. On-disk anchor index (verified UE 5.8)

- Rigid-body forces — `Source/Runtime/Engine/Classes/Components/PrimitiveComponent.h` (`AddForce:1759`,
  `AddForceAtLocation:1770`, `AddTorqueInRadians:1802`, `AddImpulse:1692`, `AddRadialForce:1793`,
  `AddRadialImpulse:1748`); CustomDepth `:712`, `CustomDepthStencilValue:861`, setters `:2092/:2096`.
- Substepping — `Source/Runtime/Engine/Classes/PhysicsEngine/PhysicsSettings.h` (`bSubstepping:325`,
  `bSubsteppingAsync:329`, `MaxSubstepDeltaTime:341`, `MaxSubsteps:344-345`).
- Async physics — `Source/Runtime/Engine/Public/Physics/AsyncPhysicsInputComponent.h`,
  `.../AsyncPhysicsData.h`; Chaos `Source/Runtime/Experimental/Chaos/Public/Chaos/PhysicsObjectInterface.h`.
- Traces — `Source/Runtime/Engine/Classes/Engine/World.h` (`LineTraceSingleByChannel:2161`,
  `LineTraceMultiByChannel:2210`, `SweepSingleByChannel:2286`, `OverlapMultiByChannel:2418`).
- Physical materials — `Source/Runtime/Engine/Classes/PhysicalMaterials/PhysicalMaterial.h`,
  `PhysicalMaterialMask.h`; `EPhysicalSurface` `Source/Runtime/PhysicsCore/Public/Chaos/ChaosEngineInterface.h:20`;
  slots in `.../PhysicsEngine/PhysicsSettings.h`.
- Constraints — `Source/Runtime/Engine/Classes/PhysicsEngine/PhysicsConstraintComponent.h`,
  `ConstraintInstance.h`, `ConstraintDrives.h`, `PhysicsAsset.h`, `BodySetup.h`.
- Scene capture / post — `Source/Runtime/Engine/Classes/Components/SceneCaptureComponent2D.h`,
  `PostProcessComponent.h`; `ESceneCaptureSource` `Source/Runtime/Engine/Classes/Engine/EngineTypes.h:534`;
  render targets `.../Engine/TextureRenderTarget2D.h`.
- Geometry Collection — `Source/Runtime/Experimental/GeometryCollectionEngine/Public/GeometryCollection/GeometryCollectionComponent.h`;
  `Plugins/Experimental/GeometryCollectionPlugin/GeometryCollectionPlugin.uplugin` (Beta, off);
  `Plugins/Experimental/Fracture/Fracture.uplugin` (Beta, off);
  `Plugins/Experimental/PlanarCutPlugin/...`; `Plugins/Experimental/ChaosSolverPlugin/...` (Beta, on);
  `Plugins/Experimental/ChaosCaching/...` (Experimental).
- Fields — `Source/Runtime/Experimental/FieldSystem/Source/FieldSystemEngine/Public/Field/FieldSystemComponent.h`,
  `FieldSystemActor.h`; `EFieldPhysicsType` `Source/Runtime/Experimental/Chaos/Public/Field/FieldSystemTypes.h:151+`;
  `Plugins/Experimental/FieldSystemPlugin/FieldSystemPlugin.uplugin` (Beta, off).
- Niagara — `Plugins/FX/Niagara/Niagara.uplugin` (stable, on);
  DIs `.../Niagara/Source/Niagara/Classes/NiagaraDataInterfaceCollisionQuery.h`,
  `.../Public/NiagaraDataInterfacePhysicsAsset.h`, `.../Public/NiagaraDataInterfaceRigidMeshCollisionQuery.h`,
  `.../Classes/NiagaraDataInterfaceLandscape.h`, `.../Classes/NiagaraDataInterfaceRenderTarget2D.h`,
  `.../Classes/NiagaraDataInterfaceGrid2DCollection.h`;
  Chaos→Niagara `Plugins/Experimental/ChaosNiagara/ChaosNiagara.uplugin` (Beta, on),
  `.../Classes/NiagaraDataInterfaceChaosDestruction.h`, `.../Public/NiagaraDataInterfaceGeometryCollection.h`;
  `Plugins/FX/NiagaraFluids/NiagaraFluids.uplugin` (Beta).
- Vehicles (scaffolding/context) — `Plugins/Experimental/ChaosVehiclesPlugin/ChaosVehiclesPlugin.uplugin`
  (Experimental, off), `Plugins/Experimental/ChaosModularVehicle/...` (Experimental);
  `Source/Runtime/Engine/Classes/PhysicsEngine/PhysicsThrusterComponent.h`.
- Terrain deform — `Source/Runtime/Engine/Classes/VT/RuntimeVirtualTexture.h`,
  `Components/RuntimeVirtualTextureComponent.h`, `Public/Materials/MaterialExpressionRuntimeVirtualTextureOutput.h`;
  `Plugins/Experimental/VirtualHeightfieldMesh`, `Plugins/Experimental/Landmass` (Experimental).
