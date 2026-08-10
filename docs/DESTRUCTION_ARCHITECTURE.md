# Kessel — DESTRUCTION ARCHITECTURE (Chaos / Geometry Collections / Field Systems)

**Team:** Destruction & World Damage
**Owns:** the destructible-world model — what shatters, how, and how cheaply: loose rubble physics,
pre-fractured buildings/walls/fences, shells punching *through* structures, the vehicle
catastrophic-kill break, and the runtime impact→field→break hooks that drive all of it.
**Does NOT own:** the ballistics math (we consume the projectile's `FHitResult` + `FHitResolution`),
the *VFX dressing* of a break (sparks, fire, smoke, dust — the Explosions & VFX arm owns those; our
debris is the physical twin their effects sit on), the *light archetype* of a burning wreck
(atmosphere arm), and live-editor placement/fracturing (Claude-main executes the editor method in §6).

Status: **DESIGN** (no C++ authored — gated on the VS/MSVC toolchain like the rest of the project;
nothing here compiles until that lands). Engine: **UE 5.8.0** (CL 55116800). Every engine API below
is cited `path:line` against the on-disk 5.8 source; anything not provable on disk is tagged
**ASSUMPTION** or **UNVERIFIED**.

Grounded on: `ARCHITECTURE.md` §2/§3/§4/§7 (class map, kinematic movement, hit flow), the
`explosions-vfx.md` team doc (turret-pop = Chaos impulse, the cook-off seam, the *deferred*
Geometry-Collection note), `DESIGN_PILLARS.md` (P1 deep-sim/abstracted, P4 cold-dark-few-warm-lights),
`ART_BIBLE.md` (PSX low-poly, two-temperature palette), and the live scaffold
(`KesselProjectile.{h,cpp}`, `KesselTankPawn.{h,cpp}`, `Kessel.Build.cs`).

Pillar anchors: **P1** — the *physics* is real Chaos rigid-body/strain simulation; the *presentation*
is chunky low-poly PSX debris, not photoreal FEM shatter. **P4** — a collapsing wall is mostly read by
the warm scatter of the burning wreck behind it; the fog does the production value, not polygon count.
**P5** — destruction is seen *through an aperture* (optic, periscope slit) at tank ranges, which is
exactly the regime where low-poly fracture reads fine and high-frequency detail is wasted.

---

## 0. One-paragraph vision — and the direct answer to the question

> *"What's the current destruction engine? I feel like that level of destruction needs to be carefully
> engineered? Or are we good? Will shells blast through buildings? If not, how do we build that?"*

**The destruction engine is Chaos** — UE 5.8's built-in destruction system (Chaos Destruction /
Geometry Collections / Field Systems). We already depend on it transitively: `ChaosVehicles` +
`PhysicsCore` pull in `Chaos`/`ChaosCore` (`Kessel.Build.cs:18-19`). **It is NOT a free
"everything-shatters" switch.** Out of the box Chaos gives us *loose rigid-body physics* — knock a
crate, scatter a pile of bricks, throw a detached turret on an impulse — and that tier is cheap and
already in the plan (the turret-pop is spec'd in `explosions-vfx.md` §6d as a plain `AddImpulse`).
**Shells will NOT blast through a building by default.** A building is just a static mesh; a projectile
trace stops on it and nothing breaks. To get "shell punches through the wall and blows debris out the
far side" we must (a) **pre-fracture** that building into a **Geometry Collection** asset in the editor,
(b) give it **damage thresholds + a connection graph** so it holds together until hit, and (c) at
runtime, when our projectile strikes a `UGeometryCollectionComponent`, **spawn a transient Field
System** at the impact point that applies a *strain* field (to break the local cluster) plus an
*impulse* field (to blow the chunks through). All three are real, verified UE 5.8 systems — but (a) is a
hand-authoring task per building and (c) is new code that pulls in two new modules. So: **yes, it needs
to be carefully engineered, and yes it's very doable** — it's a tiered, budgeted, deliberately-authored
system, not a global toggle. This doc specifies exactly what's free, what's engineered, and the build
order to get there.

---

## 1. What Chaos gives us for free vs what must be engineered

| | **Free / near-free (out of the box)** | **Must be engineered/authored** |
|---|---|---|
| **Loose rigid debris** | Drop a static mesh with `SimulatePhysics=true`; Chaos tumbles, collides, sleeps it. Knock-over of fences/crates/barrels = one bool. | Nothing — this is the baseline. Just collision + mass tuning. |
| **Impulse-thrown bodies** | `AddImpulse`/`AddAngularImpulse` on a simulating body (turret-pop). Already planned. | Deciding *when* (event wiring) — code we already have seams for. |
| **A mesh that breaks into pieces** | The *runtime simulation* of an already-fractured Geometry Collection (clusters break under strain, debris falls, sleeps, dissolves) is engine-provided. | **Authoring the fracture** (Fracture Mode, §6), **damage thresholds**, the **connection graph**, **anchoring** the base so it doesn't collapse under gravity. |
| **Shell-through-building** | The *break itself*, once triggered, is engine-driven. | **The trigger.** A projectile impact must explicitly apply a **strain + impulse field** at the hit point (§2). No automatic "kinematic body damages GC" path exists for our arcade mover (§4). |
| **Break notifications** | `OnChaosBreakEvent` fires with world location/velocity/mass per broken piece (`FChaosBreakEvent`) — free hook for spawning VFX/audio/streaming debris out. | Subscribing + budgeting what we do per event (§5). |
| **Performance scaffolding** | Sleep, clustering, LOD, anchored (kinematic) pieces, removal-on-sleep are all built into the GC component/asset. | **Choosing the budgets**: instance caps, anchor radius, debris lifespan, ISM conversion of settled rubble (§5). |

**Net:** Chaos hands us the *solver* and the *runtime break behaviour*. We engineer the *content*
(fractured assets), the *trigger* (impact→field), and the *budget* (what's allowed to simulate at once).

---

## 2. Shells-through-buildings pipeline (step by step)

This is the headline feature. End to end, author-time → runtime:

**Author time (in-editor, per building — §6):**
1. **Author/import the building mesh** as a normal static mesh (low-poly PSX wall/house/pillbox).
2. **Fracture Mode** (editor): select the mesh → create a **Geometry Collection** from it →
   apply a **Uniform** or **Voronoi** fracture to scatter cut sites (Voronoi for rubble-like
   irregular chunks; planar/slicing for clean wall sections). Add **multiple fracture *levels*** for
   nested clustering (e.g. Level 1 = whole wall sections, Level 2 = bricks within a section) so distant
   buildings stay coarse and only the hit region subdivides.
3. **Cluster** the pieces and set the **connection graph** — the asset's `ClusterConnectionType`
   (`EClusterConnectionTypeEnum`, `GeometryCollectionObject.h:595`) decides which pieces are bonded to
   which neighbours; this is what makes the structure *hold together* until a bond's strain is exceeded.
4. **Set damage thresholds** on the asset: `UGeometryCollection::DamageThreshold` (TArray<float>, one
   per cluster level, `GeometryCollectionObject.h:573`), with `bUseSizeSpecificDamageThreshold`
   (`:577`) / `PerClusterOnlyDamageThreshold` (`:587`) if we want big sections to need more energy than
   bricks. `EnableClustering` (`:557`) gates the whole connection-graph behaviour.
5. **Anchor the base.** A building must not collapse under its own weight on `BeginPlay`. Anchoring
   options, all verified: bake anchored bones in the **Fracture-Mode Anchor tool** (preferred for
   static buildings), or pin the base region at spawn with
   `UGeometryCollectionComponent::SetAnchoredByBox(FBox WorldSpaceBox, bool bAnchored, int32 MaxLevel)`
   (`GeometryCollectionComponent.h:752` — the practical "anchor the bottom of the wall" call) or
   `SetAnchoredByIndex(Index, true)` (`:746`), or drive a `Field_DynamicState` field over the base
   pieces to force them kinematic. **NOTE:** the old `UAnchorField` node does **not** exist in 5.8's
   `FieldSystemObjects.h` (grepped — absent); use the Anchor tool / `SetAnchoredBy*`.
6. Save as a **`UGeometryCollection`** asset (`GeometryCollectionObject.h:388`). Optionally
   `EnableNanite` (`:654`) for the fragment meshes — likely **off** for our PSX style.

**Place time:**
7. Drop an **`AGeometryCollectionActor`** (`GeometryCollectionActor.h:19`) referencing the asset, which
   owns a **`UGeometryCollectionComponent`** (`GeometryCollectionComponent.h`). Set its collision so the
   projectile `WeaponTrace` channel (`KesselProjectile.h:21-24`) blocks against it. Optionally point it
   at a shared **`AChaosSolverActor`** (`ChaosSolverActor.h:107`, module `ChaosSolverEngine`) so all
   destructibles share one budgeted solver.

**Runtime (the code we write):**
8. The projectile's existing impact seam fires:
   `AKesselProjectile::OnProjectileImpact(const FHitResult& Hit)` (`KesselProjectile.h:67`,
   stub in `KesselProjectile.cpp:24`). After the normal armor/ballistics resolution, branch on the
   struck component type:

   ```cpp
   void AKesselProjectile::OnProjectileImpact(const FHitResult& Hit)
   {
       // ... existing: armor face resolve + UKesselBallisticsSubsystem::ResolveKineticHit ...

       if (auto* GC = Cast<UGeometryCollectionComponent>(Hit.GetComponent()))
       {
           // Direct path (simplest): break the local cluster at the impact point.
           //   ItemIndex<0 lets the component resolve the bone from Location.
           GC->ApplyExternalStrain(/*ItemIndex*/ -1, Hit.ImpactPoint,
                                   /*Radius*/ BreakRadius, /*PropagationDepth*/ 2,
                                   /*PropagationFactor*/ 1.f, /*Strain*/ ShellBreakStrain);
           // Then shove the freed chunks through the wall along the shell's path:
           GC->ApplyBreakingLinearVelocity(/*ItemIndex*/ -1, Velocity.GetSafeNormal() * DebrisSpeed);
       }
       // ... else: spark/ricochet VFX as before ...
   }
   ```

   - **`ApplyExternalStrain(int32 ItemIndex, const FVector& Location, float Radius, int32
     PropagationDepth, float PropagationFactor, float Strain)`** — verified
     `GeometryCollectionComponent.h:714`. This is the *direct, no-Field-actor* way to break a cluster
     locally; `Radius` + `PropagationDepth` control how far the break spreads through the connection
     graph (a bigger shell = bigger radius/depth = a hole, not a pinprick).
   - **`ApplyBreakingLinearVelocity(int32 ItemIndex, const FVector& LinearVelocity)`** — verified
     `:773` — imparts the through-and-out velocity to the broken pieces (our arcade projectile can't do
     it via contact; see §4).

9. **Field-System alternative (for area effects / HE / the "blow a section out" look).** Instead of (or
   in addition to) the direct call, spawn a transient **`UFieldSystemComponent`**
   (`FieldSystemComponent.h`) — or place an **`AFieldSystemActor`** (`FieldSystemActor.h:18`) — at the
   impact and apply, in one tick:
   - **Strain to break:** `ApplyStrainField(bool Enabled, FVector Position, float Radius, float
     Magnitude, int32 Iterations)` — verified `FieldSystemComponent.h:182` (Blueprint display name
     "Apply External Strain"); this dispatches a `Field_ExternalClusterStrain` command to the solver
     (confirmed in `FieldSystemComponent.cpp:388`).
   - **Impulse to throw debris:** `ApplyRadialVectorFalloffForce(bool, FVector Position, float Radius,
     float Magnitude)` — verified `:143` — a radial **`URadialVector`**-style force that falls off with
     distance (the "blast" pushing chunks outward); or `ApplyLinearForce` (`:96`) /
     `ApplyUniformVectorFalloffForce` (`:162`) for a directional shove along the shell's path.
   - **Full custom network:** `ApplyPhysicsField(bool, EFieldPhysicsType Target, UFieldSystemMetaData*,
     UFieldNodeBase* Field)` — verified `:202` — lets us pass an authored node graph built from
     **`URadialFalloff`** (`FieldSystemObjects.h:371`), **`URadialVector`** (`:673`),
     **`UUniformVector`** (`:635`), combined via **`UOperatorField`** (`:793`) / culled with
     **`UCullingField`** (`:907`). `EFieldPhysicsType` values verified in use:
     `Field_ExternalClusterStrain`, `Field_LinearForce`, `Field_LinearVelocity`, `Field_LinearImpulse`,
     `Field_DynamicState` (`FieldSystemComponent.cpp:330-388`, `PerSolverFieldSystem.h:18-42`).

   **Recommendation:** use the **direct `ApplyExternalStrain` + `ApplyBreakingLinearVelocity`** calls
   for a single AP shell hit (cheapest, no extra actor), and reserve the **Field System** path for
   HE/area blasts and the authored "section blows out" set-pieces. Both reach the same solver.

10. **React to the break (free hook):** subscribe to the component's
    **`OnChaosBreakEvent`** (`FOnChaosBreakEvent`, `GeometryCollectionComponent.h:1269`; enable with
    `bNotifyBreaks`, `:1446`). The delegate hands us **`FChaosBreakEvent`**
    (`ChaosEventType.h:83`) with `Component`, `Location`, `Velocity`, `AngularVelocity`, `Mass`, `Index`
    (`:94-122`) — the VFX arm spawns dust/debris-trail off this, audio plays the collapse, and §5's
    streaming logic decides whether to keep simulating or convert to static rubble. `OnChaosCrumblingEvent`
    (`:1275`, `bNotifyCrumblings` `:1462`) covers mass crumble.

> **BUILD.CS CALL-OUT — REQUIRED.** None of the GC/Field code above compiles until we add the two
> modules to `Kessel.Build.cs` `PublicDependencyModuleNames` (currently `Kessel.Build.cs:11-25`):
> ```
> "GeometryCollectionEngine",   // UGeometryCollectionComponent, ApplyExternalStrain, FChaosBreakEvent hooks
> "FieldSystemEngine",          // UFieldSystemComponent, ApplyStrainField, the field-node UClasses
> ```
> Both are real engine modules under `Engine/Source/Runtime/Experimental/` (Build.cs files verified:
> `GeometryCollectionEngine/GeometryCollectionEngine.Build.cs`,
> `FieldSystem/Source/FieldSystemEngine/FieldSystemEngine.Build.cs`). They are marked **Experimental** —
> stable enough to ship on but expect occasional API churn between engine versions. The `.uproject` may
> also need the GeometryCollection/Field plugins/feature enabled; confirm at C++-phase open. Until a file
> actually `#include`s one of these headers, do **not** add the modules (keep the dependency graph honest,
> same discipline as the deferred `Niagara`/`UMG` lines in `Kessel.Build.cs:20-24`).

---

## 3. Tiered destruction model for Kessel

Four tiers, cheapest first. Each maps to an **authoring cost** (human/editor time) and a **runtime
cost** (per-frame physics/render). The discipline: spend Geometry Collections *only* where a shell must
go through structure; everything else is loose rigid bodies or fakes.

| Tier | What | Tech | Authoring cost | Runtime cost | When used |
|---|---|---|---|---|---|
| **0** | **Cosmetic only** | Decals (scorch, bullet pocks), spark/dust Niagara, no geometry change | ~zero (VFX arm assets) | trivial | Hits on intact armor/concrete that *don't* break it; most small-arms; distant impacts |
| **1** | **Loose rigid debris** | Plain static meshes, `SimulatePhysics=true`, knocked by overlap impulse (§4) | low (place props, set mass) | low–medium (bounded count, sleeps fast) | Fences, crates, barrels, oil drums, sign posts, snow-laden junk you shove driving past |
| **2** | **Pre-fractured destructibles** | **Geometry Collection** (`UGeometryCollectionComponent`) with damage thresholds + connection graph; broken by impact field (§2) | **high** (per-asset Fracture Mode pass, threshold tuning, anchoring) | **medium–high** while breaking; low once anchored/asleep | Walls, building façades, pillbox sections, rubble piles, wooden fences that splinter, the corridor ruins a shell must punch through |
| **3** | **Vehicle catastrophic kill** | Detach turret static mesh → `SetSimulatePhysics(true)` + `AddImpulse`/`AddAngularImpulse`; ammo cook-off VFX; wreck-glow light | low (already spec'd, reuses existing `TurretMesh`) | low (one rigid body, one rare event) | The iconic turret-pop on an ammo-rack kill (`explosions-vfx.md` §6d/E8) |

**Tier-by-tier notes:**
- **Tier 1 is the workhorse.** Most "destruction" the player feels (driving through a fence, clipping a
  barrel) is loose rigid bodies, *not* GCs. It's nearly free and reads great at PSX fidelity.
- **Tier 2 is the expensive one and the answer to "shells through buildings."** Budget it: only
  buildings/walls the player can plausibly shoot through or drive into get the Fracture pass. A purely
  background skyline ruin stays a static mesh (Tier 0 decals if hit). See §5 for caps.
- **Tier 3 stays a rigid-body impulse, NOT a Geometry Collection** — this is the *existing* decision in
  `explosions-vfx.md` §6d/§9: the turret-pop is one `AddImpulse` on a detached `TurretMesh`, no GC, no
  new module, honest to the no-skeletal-mesh constraint. A GC turret (peeling plates) is explicitly
  **deferred polish**. This doc does not change that; Tier 2 is where the GC investment goes first.

The two-temperature art discipline (`ART_BIBLE.md`) holds: **debris and rubble are cold** (snow-grey
concrete, splintered wood); only combustion (cook-off, burning wreck) is warm. A collapsing wall in
front of a burning wreck reads almost entirely by the warm scatter bleeding through the new gap (P4).

---

## 4. Drive-over / crush — the kinematic-vs-physics honesty

**The problem, stated plainly.** The player tank currently uses a **kinematic arcade mover**:
`AKesselTankPawn::Tick` does `AddActorWorldOffset(Delta, /*bSweep*/ true, &MoveHit)` plus a
ground line-trace, with `HullMesh->SetSimulatePhysics(false)` (`KesselTankPawn.cpp:34,114-127`;
`FollowGround` `:129-145`). A kinematic body is *moved by us*, not by the solver. **Chaos does not
automatically impart contact force from a kinematic body into dynamic bodies** — so a kinematic tank
sweeping forward will, by default, either pass through a Chaos crate or get *stopped dead* by the sweep
(`KesselTankPawn.cpp:120-123` hard-stops on any blocking hit). It will **not** organically shove a pile
of rubble aside or crush a fence the way a physically-simulated 57-tonne hull would. This is the truth
the brief asks us to be honest about: **true "drive over and crush everything" physics interaction is
gated on the later physics-based tracked mover** (`UKesselTrackedMovementComponent` — currently an empty
stub, `KesselTrackedMovementComponent.h:22-30`; planned as the Phase-2 swap, `ARCHITECTURE.md` §3).

**Interim approach (works with the kinematic mover, ship-now):**
- **Overlap-triggered impulses.** Give the hull a forward overlap/probe volume; on overlap with a Tier-1
  debris body or a Tier-2 GC component, push it explicitly:
  - debris → `UPrimitiveComponent::AddImpulse` along hull-forward, scaled by `CurrentSpeed`
    (`KesselTankPawn.cpp:190`), so fences/barrels fly off as you grind through;
  - a low-threshold GC (a flimsy wooden fence/wall) → `ApplyExternalStrain` at the contact point
    (`GeometryCollectionComponent.h:714`) so it breaks and `ApplyBreakingLinearVelocity` to scatter it.
- **Keep the sweep for hard structure.** Against a *real* building/heavy wall the existing sweep
  hard-stop (`:120-123`) is correct — a tank does not drive through a stone façade. Only flag *light*
  destructibles as crushable (a per-actor "Crushable" tag + a minimum-speed gate) so we don't turn the
  hull into a bulldozer that phases through everything.
- This is a *scripted* interaction (we decide the impulse), not emergent physics — but it sells "the
  tank shoves stuff aside" honestly within the kinematic model, and it reuses the exact same
  `ApplyExternalStrain`/`ApplyBreakingLinearVelocity` calls as the shell path (§2), so there's one code
  seam.

**The real solution (Phase 2, `UKesselTrackedMovementComponent`):** once the hull is a
*physically-simulated* Chaos rigid body driven by per-track forces (`ARCHITECTURE.md` §3 — suspension
traces + `AddForceAtLocation`/`AddTorqueInRadians` on a `SimulatePhysics=true` `HullMesh`), **Chaos
contact resolves crush for free**: the heavy dynamic hull contacts dynamic debris and GC clusters and
imparts real momentum, no scripting. Rubble gets shoved, weak walls break from contact strain, the tank
rocks as it climbs debris. That is the "drive over / crush everything" fantasy — and it is **explicitly
downstream of the physics-mover swap (`ARCHITECTURE.md` step 12), not available against the current
kinematic Gate-1 mover.** Don't promise emergent crush until that swap lands; ship the scripted interim
first.

---

## 5. Performance budget (multi-hour open-world run)

The world is World-Partition streamed and a single run is **1.5–2.5 hours** (`atmosphere-system.md` §0).
Destruction cost must be **camera-relative and O(1) in world size**, and must not leak over hours.

- **GC instance cap.** Cap *simultaneously-simulating* Geometry Collections to a small number near the
  player (e.g. ~8–16 active). Distant buildings stay **fully anchored** (all bones kinematic via the
  Anchor tool / `SetAnchoredByIndex`, `:746`) — an anchored GC costs essentially a static mesh until
  something hits it. World-Partition unloads the ones you've driven past.
- **Anchor distant structure; only wake on hit.** A GC that's never been shot is rendered as its intact
  proxy and not solving. The *first* shell that hits it (§2) is what spins up its simulation island. This
  is the key to affording GC buildings across a huge map: 99% are asleep/anchored at any moment.
- **Break-event-driven debris streaming.** Use `OnChaosBreakEvent` (`:1269`, `FChaosBreakEvent` mass/
  velocity, `ChaosEventType.h:83`) to *meter* debris: only the largest-mass / nearest pieces become
  fully-simulated chunks; small fragments spawn as short-lived VFX (VFX arm) rather than persistent rigid
  bodies. The event gives us the data to make that call per piece.
- **Debris lifespan / sleep / dissolve.** Freed chunks must **sleep** quickly (Chaos auto-sleeps
  low-velocity bodies) and then **remove or dissolve** on a timer (the GC component supports
  removal-on-sleep). Over a 2 h run, *nothing* dynamic may accumulate unbounded — same discipline as the
  VFX arm's pen-hole decal pool (`explosions-vfx.md` §5/§8). Settled rubble that must persist for
  navigation/cover converts to a **static ISM** (Instanced Static Mesh) and the dynamic bodies are
  destroyed — cheap, and the rubble pile stays as cover.
- **When to use ISMs.** Static, never-going-to-move rubble (pre-placed ruin debris, settled collapse
  remains) = **ISM**, not GC, not loose rigid bodies. GC only for *the moment of breaking*; the aftermath
  bakes to ISM/static. This keeps the froxel-lit ruin field cheap.
- **Fracture-level LOD / dynamic clustering.** Coarse top-level clusters for distant/standing buildings;
  only the *hit region* subdivides to its fine fracture level (authored in §2 step 2). Don't fracture a
  whole building to brick resolution when one corner took a shell.
- **One shared solver, budgeted.** Route destructibles through a single `AChaosSolverActor`
  (`ChaosSolverActor.h:107`) so iteration counts / sleep thresholds are tuned in one place, and the
  global simulating-particle count is visible and capped.
- **Two quality tiers** (mirroring `ARCHITECTURE.md` §12 / VFX §5): 4090/high vs Linux/min-spec —
  min-spec halves the active-GC cap, shortens debris lifespan, forces earlier ISM bake, and can drop
  Tier-2 buildings to "decal + simple knock-down" on the lowest setting. Fold into the existing
  "Effects" scalability group.

**Reality check:** the single most expensive thing is a *large building fully fracturing at once*. Avoid
it — author buildings to break **locally** (a shell makes a hole, the rest stays standing on its
anchored frame), which is both cheaper *and* more believable than a whole façade exploding.

---

## 6. Authoring workflow — human/editor pass vs codeable

A hard line runs through this system, and it's important for planning:

- **Fracturing a Geometry Collection is an in-editor authoring task — it is NOT generatable from
  headless C++.** It happens in **Fracture Mode** in the Unreal Editor: select mesh → New Geometry
  Collection → apply Uniform/Voronoi/Planar fracture → set fracture levels → cluster → paint anchors →
  tune `DamageThreshold`/`ClusterConnectionType` on the resulting `UGeometryCollection` asset
  (`GeometryCollectionObject.h:573/595`). This is per-building artist/designer work. There is no
  supported "fracture this mesh from a C++ commandlet" path we can rely on; treat each destructible
  building as a **hand pass**. (Claude-main can *drive* this via the editor MCP connection as a method,
  but it is still editor-side operation, not compiled code — same constraint the atmosphere/VFX arms note
  for live placement.)
- **The runtime impact→field→break hooks ARE code** — and they're the cheap, repeatable part. Once
  written, `AKesselProjectile::OnProjectileImpact` (`KesselProjectile.h:67`) breaks *any* GC it hits;
  the per-building cost is only the authoring, not new code. Write the hook once, author buildings
  forever.
- **Damage-threshold/anchor tuning is data**, editable on the asset/component without recompiling —
  iterable by a designer once the C++ hook exists.

**Editor method (Claude-main executes — we do not touch the live editor from here), once the C++ hook
and a test GC exist:**
1. Pick 1–2 corridor wall/building meshes; in Fracture Mode create GCs with a Voronoi fracture (~30–80
   sites) + one extra cluster level.
2. Set `ClusterConnectionType` + a `DamageThreshold` that survives gravity but breaks under a shell;
   anchor the base row of bones.
3. Place as `AGeometryCollectionActor`s in a test cell; set `WeaponTrace` collision to Block.
4. PIE-verify: fire an AP round → local hole punches through, debris blows out the far side, the rest of
   the wall stands; drive a flimsy fence → it crushes/scatters; profile `stat Chaos` / simulating-island
   count against the §5 caps.

---

## 7. Concrete next steps / build order

Ordered to deliver the user's two wishes in sequence: **"drive around and shoot tanks with debris
flying" first**, **"shells blast through buildings" second.** All steps assume the VS toolchain is
installed (the project-wide gate).

**Phase A — debris & vehicle kills (no new module, Tier 1 + Tier 3):**
1. **Tier 1 loose debris.** Place fences/crates/barrels as `SimulatePhysics` static meshes; tune mass +
   sleep. Gate: shooting/driving near them scatters them. *(Free Chaos, zero new modules.)*
2. **Interim crush.** Add the hull overlap-probe + `AddImpulse`/`ApplyExternalStrain`-on-light-debris
   (§4). Gate: driving through a fence shoves/breaks it (scripted, honest to the kinematic mover).
3. **Tier 3 turret-pop.** Wire the cook-off → detach `TurretMesh` → `SetSimulatePhysics(true)` +
   `AddImpulse`/`AddAngularImpulse` (already spec'd, `explosions-vfx.md` §6d/E8). Gate: ammo-rack kill
   throws the turret into the snow. *(Still no GC, no new module.)*
   - **End of Phase A = "drive around and shoot tanks with debris flying." This is the first milestone
     and needs no Geometry Collections at all.**

**Phase B — shells through buildings (Tier 2, adds the two modules):**
4. **Add the modules.** `GeometryCollectionEngine` + `FieldSystemEngine` to `Kessel.Build.cs`
   PublicDependencyModuleNames (§2 call-out). Rebuild.
5. **One test GC building.** Author a single fractured wall (Fracture Mode, §6). Place as
   `AGeometryCollectionActor`, anchor its base, set thresholds.
6. **The impact hook.** Implement the `UGeometryCollectionComponent` branch in
   `OnProjectileImpact` (`KesselProjectile.cpp:24`): `ApplyExternalStrain` +
   `ApplyBreakingLinearVelocity` (§2 step 8). Gate: an AP shell punches a hole through the test wall and
   blows debris out the back; the wall otherwise stands.
7. **Field-System path for HE/area.** Add the transient `UFieldSystemComponent` /
   `ApplyStrainField` + `ApplyRadialVectorFalloffForce` path (§2 step 9) for blast/area breaks.
8. **`OnChaosBreakEvent` wiring.** Subscribe (`bNotifyBreaks`), feed the VFX arm dust/debris off
   `FChaosBreakEvent`, feed §5's streaming/ISM-bake logic. Gate: collapses spawn dust + don't leak.
9. **Roll out + budget.** Author the handful of corridor buildings/walls that gameplay actually needs
   destructible; anchor the rest; apply the §5 caps + two quality tiers. Gate: a multi-building firefight
   holds frame and doesn't accumulate debris over time.

**Phase C — emergent crush (downstream of the physics mover):**
10. When `UKesselTrackedMovementComponent` (physics tracked mover, `ARCHITECTURE.md` step 12) lands and
    `HullMesh` simulates physics, **remove the scripted interim crush** — Chaos contact now resolves
    drive-over/crush emergently (§4). Gate: the heavy hull shoves rubble and breaks weak walls by contact
    alone.

---

## 8. Verified engine API anchor index (UE 5.8, on disk)

Path prefix: `C:/Program Files/Epic Games/UE_5.8/Engine/Source/Runtime/Experimental/` unless noted.

**Geometry Collection (module `GeometryCollectionEngine`):**
- `UGeometryCollection` (asset) — `GeometryCollectionEngine/Public/GeometryCollection/GeometryCollectionObject.h:388`;
  `EnableClustering:557`, `DamageThreshold:573`, `bUseSizeSpecificDamageThreshold:577`,
  `PerClusterOnlyDamageThreshold:587`, `ClusterConnectionType` (`EClusterConnectionTypeEnum`)`:595`,
  `EnableNanite:654`.
- `AGeometryCollectionActor` — `.../GeometryCollection/GeometryCollectionActor.h:19`.
- `UGeometryCollectionComponent` — `.../GeometryCollection/GeometryCollectionComponent.h`:
  `ApplyExternalStrain(int32,FVector,float,int32,float,float):714`,
  `CrumbleActiveClusters():740`, `SetAnchoredByIndex(int32,bool):746`,
  `SetAnchoredByBox(FBox,bool,int32):752`, `ApplyBreakingLinearVelocity(int32,FVector):773`,
  `ClusterGroupIndex:1006`,
  `DamageThreshold (TArray<float>):1030` / `SetDamageThreshold:1036`,
  `DamagePropagationData (FGeometryCollectionDamagePropagationData):1051`,
  `OnChaosBreakEvent (FOnChaosBreakEvent):1269`, `OnChaosCrumblingEvent:1275`,
  `bNotifyBreaks:1446`, `bNotifyCrumblings:1462`; delegate decls `FOnChaosBreakEvent:53` /
  `FOnChaosCrumblingEvent:57`. (`ClusterConnectionType_DEPRECATED:1074` — moved to the asset.)

**Field System (module `FieldSystemEngine`, under `FieldSystem/Source/FieldSystemEngine/`):**
- `UFieldSystemComponent` — `Public/Field/FieldSystemComponent.h`:
  `ApplyLinearForce:96`, `ApplyRadialForce:126`, `ApplyRadialVectorFalloffForce:143`,
  `ApplyUniformVectorFalloffForce:162`, `ApplyStrainField(bool,FVector,float,float,int32):182`,
  `ApplyPhysicsField(bool,EFieldPhysicsType,UFieldSystemMetaData*,UFieldNodeBase*):202`,
  `AddFieldCommand:259`.
- `AFieldSystemActor` — `Public/Field/FieldSystemActor.h:18`.
- Field-node UClasses — `Public/Field/FieldSystemObjects.h`: `UFieldNodeBase:125`,
  `URadialFalloff:371`, `UUniformVector:635`, `URadialVector:673`, `UOperatorField:793`,
  `UCullingField:907`. **(No `UAnchorField` in 5.8 — verified absent; anchor via the Fracture-Mode
  Anchor tool / `SetAnchoredByIndex` / a `Field_DynamicState` mask.)**
- `EFieldPhysicsType` values in use — `Field_ExternalClusterStrain`, `Field_LinearForce`,
  `Field_LinearVelocity`, `Field_LinearImpulse`, `Field_DynamicState`
  (`FieldSystemEngine/Private/Field/FieldSystemComponent.cpp:330-388`;
  `Chaos/Public/PhysicsProxy/PerSolverFieldSystem.h:18-42`).

**Solver / events:**
- `AChaosSolverActor` — `ChaosSolverEngine/Public/Chaos/ChaosSolverActor.h:107` (module `ChaosSolverEngine`).
- `FChaosBreakEvent` — `Runtime/Engine/Public/Physics/Experimental/ChaosEventType.h:83` (fields
  `Component:94`, `Location:98`, `Orientation:102`, `Velocity:106`, `AngularVelocity:110`,
  `Extents:114`, `Mass:118`, `Index:122`).

**Modules to add to `Kessel.Build.cs`:** `GeometryCollectionEngine`
(`.../GeometryCollectionEngine/GeometryCollectionEngine.Build.cs`), `FieldSystemEngine`
(`.../FieldSystem/Source/FieldSystemEngine/FieldSystemEngine.Build.cs`). Both **Experimental**.

**Project seams referenced:** `AKesselProjectile::OnProjectileImpact` (`KesselProjectile.h:67`,
stub `KesselProjectile.cpp:24`); kinematic mover (`KesselTankPawn.cpp:34,114-127`, `FollowGround:129`);
physics mover stub (`KesselTrackedMovementComponent.h:22`); existing Chaos dep
(`Kessel.Build.cs:18-19`).

---

## 9. Open questions for the user

1. **How much of the world is destructible?** Recommend: only corridor buildings/walls gameplay routes
   *through or into* get Tier-2 GCs; the skyline ruins stay static (Tier 0). Confirms the §5 budget.
2. **Does "drive through buildings" matter, or just "shoot through them"?** Drive-through crush of *heavy*
   structure is gated on the physics mover (Phase C). Shoot-through (Phase B) works now. Confirm priority.
3. **Interim scripted crush vs wait for the physics mover?** Recommend shipping the scripted interim
   (§4) so Phase-A milestone *feels* like a tank, then deleting it when Phase C lands.
4. **Persistent rubble for cover/navigation, or dissolve it?** Recommend ISM-bake the settled piles
   (cover + cheap), dissolve the small fragments (§5).
5. **Accept the Experimental module risk?** `GeometryCollectionEngine`/`FieldSystemEngine` are
   Experimental — fine to ship on, but expect minor API churn across engine updates.

---

## 10. Cross-references
- Hit flow, kinematic mover, class map, physics-mover swap: `docs/ARCHITECTURE.md` §3/§4/§7, step 12.
- Turret-pop = Chaos impulse (Tier 3), the GC-fracture *deferral*, cook-off seam, break VFX:
  `docs/teams/explosions-vfx.md` §6d/§8/§9 (E7/E8/E9).
- Streamed-world / camera-relative budget discipline, two quality tiers: `docs/teams/atmosphere-system.md`
  §0/§7; `docs/teams/explosions-vfx.md` §5.
- Palette (cold debris / warm combustion), PSX low-poly fracture reads fine through an aperture:
  `docs/ART_BIBLE.md` §3; `docs/DESIGN_PILLARS.md` P1/P4/P5.
- Live scaffold seams: `Source/Kessel/{Public/KesselProjectile.h, Private/KesselProjectile.cpp,
  Public/KesselTankPawn.h, Private/KesselTankPawn.cpp, Public/KesselTrackedMovementComponent.h,
  Kessel.Build.cs}`.
