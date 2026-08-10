# Kessel — Milestone 1 C++ Architecture

Status: DESIGN (no C++ authored — VS/MSVC absent; do not compile this phase).
Engine: UE 5.8.0 (CL 55116800, `++UE5+Release-5.8`).
Scope: the C++ class/module architecture to reach **a drivable, shooting tank in volumetric
winter fog, first-person only**. This doc consolidates research docs 01–07 into one
implementable plan. Every engine API is cited `path:line` against the on-disk 5.8 source under
`C:/Program Files/Epic Games/UE_5.8/Engine/Source`. Anything not provable on disk is tagged
**ASSUMPTION**.

Cross-references: `docs/research/01-ue58-cpp-setup.md` (modules/templates),
`03-tracked-vehicle.md` (movement), `04-ballistics-armor.md` (combat),
`05-ps1-aesthetic.md` (retro post), `06-volumetric-fog-scatter.md` (atmosphere),
`07-view-modes-controls.md` (view/input).

---

## 0. Toolchain prerequisite (gate on everything below)

NONE of this compiles until **Visual Studio 2022 17.14.x** (Game Dev C++ workload) + Windows SDK
10.0.22621 are installed, per `01-ue58-cpp-setup.md §1`. Linux shipping additionally needs the
`v26_clang-20.1.8-rockylinux8` cross-toolchain and `LINUX_MULTIARCH_ROOT`
(`LinuxToolChain.cs:137`, `LinuxPlatformSDK.cs:192`). Until then this is paper only — no UBT, no
editor launch. All "verify in editor" items are collected in §13.

**Portability rule (Win + Linux):** every system below uses only cross-platform engine APIs —
Chaos rigid-body forces, line traces, Enhanced Input, UMG, core fog/light components. No
platform `#ifdef`, no DX/PhysX-specific calls, no Windows-only headers. The PS1 layer is CVars +
materials (portable). This keeps the Linux ELF build identical to Win64.

---

## 1. Module list

Single runtime game module `Kessel` for M1 (no need to split until editor-only tooling appears).
Build rules already authored in `01-ue58-cpp-setup.md §5` (`Kessel.Build.cs`). Dependencies and
grounding:

| Module | Why | Default-on? | Grounding |
|---|---|---|---|
| `Core`,`CoreUObject`,`Engine`,`InputCore` | base runtime/reflection/pawn | always | engine |
| `EnhancedInput` | drive + mouse aim + view actions | yes | `EnhancedInput.uplugin` |
| `ChaosVehicles` | Phase-1 scaffolding pawn; brings `PhysicsCore/Chaos/ChaosCore/NNE` transitively via `SetupModulePhysicsSupport` | **no (experimental — must enable plugin)** | `ChaosVehiclesPlugin.uplugin`; `ModuleRules.cs:1793-1813` |
| `PhysicsCore` | explicit, for direct rigid-body/force headers used by the custom track layer | n/a | transitive but listed for clarity |
| `Niagara` | muzzle flash, blizzard, ember light-motes | yes | `Niagara.uplugin` |
| `UMG`,`Slate`,`SlateCore` | sight reticles / periscope masks / instruments | always | engine |
| `RenderCore`,`RHI` (private) | touch volumetric/material params; transitive but explicit | n/a | engine |

`.uproject` must explicitly enable `ChaosVehiclesPlugin` (`"EnabledByDefault": false`,
`"IsExperimentalVersion": true`), `EnhancedInput`, `Niagara` — see `01 §2`.

ASSUMPTION (revisit post-M1): if we add an editor-only toolset (data validation, asset cooking
helpers) it becomes a second `KesselEditor` module; not needed for M1.

---

## 2. Text class map

All classes live in module `Kessel`, namespaced by prefix only (UE convention). `→` = "owns/has
a"; `:` = "inherits".

```
GAME FRAMEWORK
  AKesselGameMode : AGameModeBase
      DefaultPawn = AKesselTankPawn ; PlayerController = AKesselPlayerController
  AKesselPlayerController : APlayerController
      adds Enhanced Input mapping contexts; routes view/fire/drive actions to pawn components
  UKesselGameInstance : UGameInstance            (optional; holds run seed)

THE PLAYER TANK
  AKesselTankPawn : APawn                          (the one controllable tank)
   → UStaticMeshComponent      HullMesh            ROOT, SimulatePhysics=true → the rigid body
   → UKesselTrackedMovementComponent  Movement     locomotion model (see §3)            [Phase 2]
   → (Phase 1 only) UKesselWheeledScaffoldMovement : UChaosWheeledVehicleMovementComponent
   → USceneComponent           TurretRoot          child of Hull; yaw only (turret traverse)
   → USceneComponent           GunMount            child of TurretRoot; pitch only (gun elevation)
   → UStaticMeshComponent      TurretMesh, GunMesh  visuals parented to TurretRoot/GunMount
   → UStaticMeshComponent      InteriorMesh        cockpit cage (FirstPersonFOV candidate)
   → UCameraComponent          CamGunner           on GunMount mantlet socket (gun-laid optic)
   → UCameraComponent          CamCommander        turret roof (buttoned periscope)
   → UCameraComponent          CamUnbuttoned       hatch socket (open, exposed)
   → UKesselTurretAimComponent Aiming              mouse→turret/gun rate-limited chase (see §5)
   → UKesselViewComponent      View                EViewMode state machine, camera/IMC/reticle swap
   → UKesselGunComponent       Gun                 chambered shell, reload timer, spawns AProjectile
   → UKesselArmorComponent     Armor               holds FVehicleArmorLayout; receives hit resolution
   → UPointLightComponent      CabinLight, MuzzleLight   emissive fog-scatter sources (see §8)
   → UNiagaraComponent         MuzzleFX, ExhaustFX

COMBAT / BALLISTICS
  UKesselBallisticsSubsystem : UWorldSubsystem     stateless solver + per-run FRandomStream
      ResolveKineticHit(FShellData, FArmorSection, dir, normal, range) -> FHitResolution
  AKesselProjectile : AActor                        flying round; visual arc; reports impact
   → USphereComponent / UProjectileMovementComponent   (or custom substepped trace — see §4)
  FShellData         (USTRUCT)   one round's ballistics  (DataTable row)        [04 §10]
  FArmorSection      (USTRUCT)   one armor face
  FCrewModule        (USTRUCT)   crew/module interior node
  FVehicleArmorLayout(USTRUCT)   faces[] + modules[] for a vehicle
  FHitResolution     (USTRUCT)   {bPen,bRicochet,LOS,P0,residualFrac,spallPower,entryPoint,dir}
  EShellType         (UENUM)     AP,APCBC,APCR,APDS,HE,HEAT (M1 wires AP/APCBC only)
  UBallisticsConfig  : UPrimaryDataAsset            THETA_MAX, COS_MIN, OVERMATCH_NORICO, norm/ricochet/spall constants

ENEMY
  AKesselEnemyTank : APawn (stub)                   non-driving target; HullMesh + UKesselArmorComponent
      simple "loom out of fog / aim at player / fire on cadence" AI; full AI is post-M1

DATA ASSETS (data-driven, see §6)
  UTankSightProfile  : UPrimaryDataAsset            zoom ladder + reticle tex + optic PP per sight  [07 §3]
  UAtmospherePreset  : UPrimaryDataAsset            fog/light values → applied to fog+lights        [06 §6]
  MPC_Retro          (UMaterialParameterCollection) PS1 dials (asset, not C++)                       [05 §6]
  DT_Shells          (UDataTable of FShellData)     8,8cm_PzGr39, 85mm_BR365, ...                    [04 §12-13]
  DA_Armor_TigerI / DA_Armor_T34_85 (UPrimaryDataAsset wrapping FVehicleArmorLayout)

ATMOSPHERE / RETRO (mostly placed actors + materials, thin C++)
  AKesselAtmosphereController : AActor               applies UAtmospherePreset to the level fog/lights
   → UExponentialHeightFogComponent (or references the level's AExponentialHeightFog)
  BP_RetroController (Blueprint)                     one MasterRetro slider → MPC_Retro + CVars       [05 §8]
```

---

## 3. Movement component (recommended approach)

Per `03-tracked-vehicle.md`: **phased hybrid, custom track layer on a Chaos rigid body**. Chaos
Vehicles 5.8 is a *car* simulator with no skid/neutral-steer (verified: `EVehicleDifferential` is
wheeled-only `ChaosWheeledVehicleMovementComponent.h:64-70`; `ESteeringType` is Ackermann-family
`:433-438`; zero `TankVehicle/TrackedVehicle/SkidSteer` hits engine-wide). So pivot must be authored.

- **Phase 1 (throwaway scaffolding):** `UKesselWheeledScaffoldMovement :
  UChaosWheeledVehicleMovementComponent` (`ChaosWheeledVehicleMovementComponent.h:633`), 8
  roadwheels, every wheel `bAffectedBySteering=false` (`ChaosVehicleWheel.h:126`), W/S throttle/brake,
  crude pivot via `SetYawInput` (`ChaosVehicleMovementComponent.h:915`). Purpose: validate scene
  scale, camera, mass feel, snow/fog in days. **Do NOT ship its steering.**
- **Phase 2 (target):** `UKesselTrackedMovementComponent : UPawnMovementComponent`
  (`PawnMovementComponent.h:42`). Per physics sub-step on the `HullMesh` rigid body:
  1. Per roadwheel: `LineTraceSingleByChannel` (`World.h:2161`) down from the hull; suspension
     spring `k·x` + damper `c·v` along contact normal.
  2. Desired track speeds from throttle+steer: `vL = thr·vmax + steer·pivot`,
     `vR = thr·vmax − steer·pivot` (steer±1 with throttle 0 → equal/opposite → **neutral-steer
     pivot**).
  3. Per side: tractive force = `clamp(engineForce, ±muLong·sideLoad)` applied at the side's
     average contact point along hull forward.
  4. Strong **lateral** track resistance opposing sideways contact velocity (turns by gripping,
     not sliding).
  5. Apply via `AddForceAtLocation` (`PrimitiveComponent.h:1770`) + `AddTorqueInRadians`.
  Reuse Chaos `Mass`/`CenterOfMassOverride` pattern (`ChaosVehicleMovementComponent.h:748,754,760`)
  and the `FVehicleEngineConfig` RPM/torque shape (`ChaosWheeledVehicleMovementComponent.h:223-258`)
  for audio + the slow-diesel feel curve.

All tunables (mass ~57000 kg, muLong/muLat, PivotSpeed, SteerRate, suspension k/c) are
`UPROPERTY(EditAnywhere)` on a "feel config" struct so locomotion is data-tunable like the graphics.
Heavy mass + low per-frame force → **use physics sub-stepping** for stable track forces (mushy
otherwise).

---

## 4. Ballistics & combat

Adopt `04-ballistics-armor.md` as the M1 spec: kinetic AP/APCBC only; HE/HEAT/APCR enum+fields
reserved-but-unwired.

- **`UKesselBallisticsSubsystem : UWorldSubsystem`** (`WorldSubsystem.h:16`) — stateless math, one
  per world, owns a per-run `FRandomStream` so ricochet/spall rolls are reproducible. Core entry
  `ResolveKineticHit(shell, faceSection, impactDir, surfaceNormal, range) → FHitResolution`
  implementing the `04 §4` pipeline exactly: `theta_n` (clamp dot before `acos`), `P0 =
  PenetrationAtRange` (tabulated `PenAtRanges` lerp, de Marre only for extrapolation), overmatch,
  normalization, **ricochet check before the thickness compare**, `LOS = T/cos(theta_eff)` with the
  `COS_MIN` floor, then `residualFrac` → spall. WorldSubsystem (not GameInstance) because the seed
  and any duel state are per-level.
- **`AKesselProjectile : AActor`** — the fired round. Two viable impl paths:
  - (A) `UProjectileMovementComponent` + `USphereComponent` overlap/hit for the visual arc, then on
    `OnComponentHit` read `FHitResult` normal/location and call the subsystem. Simplest; good enough
    for M1 at tank ranges/velocities.
  - (B) custom per-substep `LineTraceSingleByChannel` along the analytic trajectory (avoids tunneling
    at 800 m/s). **Recommended for correctness**; arc is cosmetic, penetration uses impact speed
    `v(r)=v0·exp(−k·r)` (`04 §3`). Decide during build; B is the safer default.
- **Hit application:** the trace/overlap returns the hit `UPrimitiveComponent` + bone + normal. The
  owning actor's `UKesselArmorComponent` maps bone/socket → `FArmorSection` (via
  `FVehicleArmorLayout.Faces`), the subsystem resolves, and on PEN a short fragment cone damages
  `FCrewModule`s (`04 §9`). Knockout conditions per `04 §9.4`. Player is crew-of-1 → a solid crew
  pen triggers the death/abort sequence.
- **`UKesselGunComponent`** — holds chambered `FShellData` (row from `DT_Shells`), reload timer
  (`GetWorldTimerManager().SetTimer`), spawns `AKesselProjectile` from the muzzle socket with muzzle
  velocity + direction = gun forward, and pulses `MuzzleLight`/`MuzzleFX` (§8).
- **Data:** `FShellData`, `FArmorSection`, `FCrewModule`, `FVehicleArmorLayout`, `EShellType`
  exactly as `04 §10`. Constants in `UBallisticsConfig : UPrimaryDataAsset`
  (`DataAsset.h:47`). The three `04 §13` duel checks are the acceptance test once C++ compiles.

Numerical-safety checklist (`04 §11`) is mandatory in code: clamp the `acos` dot to [−1,1], floor
`cos` at `COS_MIN`, guard `plateThickness>0.1`, validate `RicochetAngle∈[40,89]`, clamp
`residualFrac∈[0,1]`. No NaN reaches gameplay.

---

## 5. View system + input

Per `07-view-modes-controls.md`: **three first-person cameras, FOV-stepping zoom, UMG reticles,
mouse-as-rate-controller**. First-person ONLY — no third-person camera exists in code.

- **`UKesselViewComponent : UActorComponent`** owns `EViewMode {GunnerOptic, CommanderButtoned,
  Unbuttoned}`. On state Enter: `Deactivate()` old `UCameraComponent` / `Activate()` new
  (`CameraComponent.h`), swap Enhanced Input mapping context, show the matching UMG widget, set
  `bExposed`/`bCanFireFromView`/FOV. ASSUMPTION (`07 §2.3`): a single active `UCameraComponent` is
  auto-selected as the pawn view target without overriding `CalcCamera` (`Actor.h:3687`) — verify in
  editor; if not, the pawn overrides `CalcCamera` to report the active camera's POV.
- **Zoom = `UCameraComponent::SetFieldOfView`** (`CameraComponent.h:46`) stepping a discrete ladder
  (gunner x2.5/x5/x8 → FOV 28/14/8.75; commander x1/x6), `FInterpTo` ~0.12 s. **Reject**
  `USceneCaptureComponent2D` as the default optic (doubles scene cost; `SceneCaptureComponent.h:85`)
  — keep only as an optional post-M1 picture-in-periscope. Optic cold tint / vignette / chromatic
  aberration ride on the camera's `PostProcessSettings` (`CameraComponent.h:353`); the interior cage
  can use `FirstPersonFieldOfView` (`CameraComponent.h:51`) so the eyecup stays fixed while the world
  zooms (ASSUMPTION — verify).
- **Reticles = UMG `UUserWidget`** per mode (gunner reticle / periscope mask / open-hatch HUD),
  toggled by visibility not add/remove. Etched-glass lines via a `UMaterialInstanceDynamic` on the
  brush. Reticle scale/range pushed from the view component on zoom change (`07 §4`).
- **`UKesselTurretAimComponent`** (`07 §5`): mouse delta → `DesiredTurretYaw`/`DesiredGunPitch`
  targets; actual turret/gun **chase** at capped traverse rate (`FMath::FixedTurn` /
  `FInterpConstantTo`), **hull-independent** (turret yaw stored relative to hull, re-applied each
  tick via `SetRelativeRotation`). Defaults: traverse ~6–10°/s, elevation ~8°/s, limits −8°..+15°.
  `IA_Aim` is routed by the view component to turret (gunner) vs head free-look (commander).
- **Enhanced Input** (`07 §5.4`, all verified): `UInputAction`/`UInputMappingContext`, bind in pawn
  `SetupPlayerInputComponent` via `UEnhancedInputComponent::BindAction`
  (`EnhancedInputComponent.h:482/497`), add contexts via
  `UEnhancedInputLocalPlayerSubsystem::AddMappingContext` (`EnhancedInputSubsystems.h:37`). Full
  keybind table in `07 §6` (W/S/A/D drive, Shift+A/D pivot, Mouse XY aim, LMB fire, V cycle sight,
  wheel/Z zoom, H unbutton, L cabin light, K headlight).

---

## 6. Data-driven assets

Everything tunable is data, not hardcoded — consistent with the "graphics/feel must be tunable"
mandate.

| Asset | Type | Holds | Doc |
|---|---|---|---|
| `DT_Shells` | `UDataTable` (row=`FShellData`) | per-round ballistics + `PenAtRanges` curve | 04 §10/§12-13 |
| `DA_Armor_*` | `UPrimaryDataAsset` wrapping `FVehicleArmorLayout` | faces + crew/modules per vehicle | 04 §10 |
| `UBallisticsConfig` | `UPrimaryDataAsset` | THETA_MAX, COS_MIN, OVERMATCH_NORICO, norm/ricochet/spall constants | 04 §10 |
| `UTankSightProfile` | `UPrimaryDataAsset` | zoom ladder, reticle texture, optic PP per sight | 07 §3 |
| `UAtmospherePreset` | `UPrimaryDataAsset` | fog density/albedo/extinction/phase-G + per-light scatter | 06 §6 |
| `MPC_Retro` | `UMaterialParameterCollection` | PS1 dials (MasterRetro, ColorLevels, SnapStrength…) | 05 §6 |

Choice rationale: **DataTable for shells** (homogeneous rows, easy bulk authoring/CSV import);
**PrimaryDataAsset for armor layouts, sight/atmosphere presets, ballistics config** (heterogeneous,
nested arrays, per-asset editing, async-loadable). `UPrimaryDataAsset` confirmed
`DataAsset.h:47`. Reserved-but-unwired HE/HEAT/APCR fields exist in `FShellData` from day one so
save-data/rows never break (`04 §2`).

---

## 7. Per-tick interaction (data flow)

```
INPUT (Enhanced Input, PlayerController/Pawn)
  W/S/A/D/Shift ─► UKesselTrackedMovementComponent.SetThrottle/Brake/Steer
  Mouse XY ──────► UKesselTurretAimComponent (Desired yaw/pitch)
  LMB ───────────► UKesselGunComponent.Fire()
  V/Z/wheel/H ───► UKesselViewComponent (mode/zoom/unbutton)

PHYSICS SUBSTEP (async/substepped)
  UKesselTrackedMovementComponent:
    roadwheel line traces ─► suspension forces ─► per-track tractive + lateral resistance
    ─► AddForceAtLocation / AddTorqueInRadians on HullMesh  ─► Chaos integrates rigid body

GAME TICK (after physics)
  UKesselTurretAimComponent:
    Desired*  ─chase(rate-limited)─►  Actual turret yaw (rel hull) + gun pitch (rel turret)
    ─► SetRelativeRotation(TurretRoot/GunMount)  (hull-independent lay)
  UKesselViewComponent:
    interp camera FOV toward zoom step ; ensure exactly one active camera ; widget visibility

ON FIRE (event, not every tick)
  UKesselGunComponent.Fire():
    spawn AKesselProjectile(muzzle xform, v0, dir) ; pulse MuzzleLight/MuzzleFX ; start reload timer
  AKesselProjectile (its own substepped trace):
    trace along arc ─► FHitResult(component,bone,normal,location,range)
    ─► hitActor.UKesselArmorComponent.MapBoneToFace() ─► FArmorSection
    ─► UKesselBallisticsSubsystem.ResolveKineticHit(...) ─► FHitResolution
    ─► if PEN: fragment cone vs FCrewModule[]  ─► damage/knockout
       if RICOCHET/NON-PEN: spark/spall VFX, projectile deflects/stops

RENDER (passive each frame)
  ExponentialHeightFog volumetric solve ◄─ AKesselAtmosphereController applied UAtmospherePreset
  MuzzleLight/CabinLight VolumetricScatteringIntensity ─► warm scatter into cool fog albedo
  PP_Retro post material ◄─ MPC_Retro dials (MasterRetro) ; r.ScreenPercentage/AA CVars
```

Ordering note: aim chase runs **after** physics so the turret lay is applied on top of the settled
hull rotation each frame (hull-independent counter-rotation). Ballistics is event-driven, never
per-tick.

---

## 8. Atmosphere + PS1 post (setup/data, thin C++)

Per `06` and `05` these are mostly **placed actors + materials**, with one thin C++ controller each
so values are runtime-tunable.

- **Atmosphere:** one `AExponentialHeightFog` in the level, `bEnableVolumetricFog=true`
  (`ExponentialHeightFogComponent.h:136`). `AKesselAtmosphereController` applies a `UAtmospherePreset`
  via the confirmed setters: `SetVolumetricFogExtinctionScale` (`:311`),
  `SetVolumetricFogScatteringDistribution` (`:308`), `SetVolumetricFogAlbedo` (`:314`),
  `SetVolumetricFogDistance` (`:320`), `SetFogDensity` (`:251`). Starting recipe `06 §8`: density
  0.1, extinction 3.0, distance 8000, albedo (150,170,200), phase-G 0.3. Warm Movable point/spot
  lights carry `VolumetricScatteringIntensity` 2–4 (`LightComponentBase.h:120`), muzzle pulsed 6+,
  giving the red/amber-into-cool-fog smear. Blizzard = camera-anchored Niagara reading vehicle
  velocity. Grid pushed to `r.VolumetricFog.GridPixelSize 8` / `GridSizeZ 128` on the 4090; reserve
  16/64 for the Linux/min-spec path.
- **PS1 retro:** `MPC_Retro` + `MF_VertexSnap` (WPO) + `PP_Retro` (posterize/dither/palette,
  `MD_PostProcess` `MaterialDomain.h:23`, `BL_SceneColorAfterTonemapping` `BlendableInterface.h:75`)
  + `BP_RetroController` mapping one `MasterRetro` slider → MPC scalars and the CVars
  (`r.AntiAliasingMethod 0`, `r.ScreenPercentage` integer-divisor, `r.Upscale.Quality 0`). Affine
  texture warp descoped for M1 (needs custom vertex factory). All per `05 §8`.

These two systems are deliberately decoupled from gameplay code (no per-tick dependency) so the look
stays tunable without touching the sim.

---

## 9. Enemy tank stub

`AKesselEnemyTank : APawn` — minimal target so the combat loop is testable:
- `HullMesh` (static, may simulate physics later) + `TurretMesh`/`GunMesh` for hit geometry.
- `UKesselArmorComponent` with `DA_Armor_T34_85` so the player's rounds resolve against real faces.
- Trivial behavior: idle until player in range, slowly traverse gun toward player, fire on a fixed
  cadence with a `FShellData` row. No navigation/driving in M1 — it "looms out of the fog" at scripted
  positions. Reuses the **same** `UKesselGunComponent` + `AKesselProjectile` + ballistics subsystem as
  the player (symmetric combat). Full AI/driving is post-M1.

---

## 10. Shared vs player-only components

To keep player and enemy symmetric and avoid duplication, these are reusable `UActorComponent`s
usable by both pawns: `UKesselArmorComponent`, `UKesselGunComponent`, `UKesselTurretAimComponent`
(enemy drives it from AI targets instead of mouse). Player-only: `UKesselViewComponent`,
`UKesselTrackedMovementComponent`, the three cameras. This means the ballistics pipeline is exercised
identically whether the player or an enemy fires.

---

## 11. CONCRETE ORDERED Milestone-1 build sequence

Smallest path to **drivable + shooting tank in fog**. Each step is independently verifiable in the
editor. (All steps assume VS 2022 is installed first.)

1. **Project skeleton.** Author `Kessel.uproject` + `Source/` targets + `Kessel.Build.cs` from
   `01 §5`. Generate VS project files, build `Development Editor | Win64`, open editor. Gate: editor
   opens with the empty `Kessel` module loaded.
2. **Empty arena + atmosphere first** (so everything after is seen in the real look). Place
   `AExponentialHeightFog` (volumetric on) + Movable SkyLight + overcast DirectionalLight; apply the
   `06 §8` recipe by hand. Gate: cold whiteout fog renders. (Add `AKesselAtmosphereController` +
   `UAtmospherePreset` for runtime tuning right after.)
3. **Pawn + Phase-1 scaffolding movement.** `AKesselTankPawn` with `HullMesh` +
   `UKesselWheeledScaffoldMovement`; `AKesselGameMode` sets it as default pawn; basic W/S/A/D via
   Enhanced Input. Gate: a box-tank drives around in the fog (steering crude/throwaway).
4. **First-person camera + view component (single mode).** Add `CamGunner` + `UKesselViewComponent`
   with only `GunnerOptic`; confirm single-active-camera is the view (or override `CalcCamera`).
   Gate: you see first-person from the gun optic while driving. (Resolves the `07 §2.3` ASSUMPTION
   early.)
5. **Turret/gun aim.** `UKesselTurretAimComponent` (mouse→rate-limited turret yaw + gun pitch,
   hull-independent) + `TurretRoot`/`GunMount`. Gate: mouse slews a slow heavy turret independent of
   hull heading.
6. **Multi-sight + zoom + reticles.** Add `CamCommander`/`CamUnbuttoned`, the `EViewMode` state
   machine, FOV-step zoom, and the three UMG widgets + `UTankSightProfile`. Gate: V cycles sights, Z
   zooms, H unbuttons, reticle shows.
7. **Ballistics data + subsystem.** Author `FShellData`/`FArmorSection`/`FCrewModule`/
   `FVehicleArmorLayout`/`EShellType`, `DT_Shells` (8,8cm_PzGr39, 85mm_BR365), `DA_Armor_*`,
   `UBallisticsConfig`, and `UKesselBallisticsSubsystem::ResolveKineticHit`. Unit-test the three
   `04 §13` duels with synthetic inputs (no projectile yet). Gate: the three duel checks pass.
8. **Gun + projectile + hit application.** `UKesselGunComponent` (reload, spawn) +
   `AKesselProjectile` (substepped trace path B) + `UKesselArmorComponent` (bone→face map). Wire LMB
   fire → projectile → subsystem → fragment cone. Gate: firing at a placed armor box produces
   pen/ricochet/non-pen results matching the duel math.
9. **Enemy stub.** `AKesselEnemyTank` with armor + gun; place a couple at scripted positions that
   loom from the fog and fire on cadence. Gate: a two-way duel resolves (player can knock it out; it
   can wound the player).
10. **Muzzle/cabin emissive scatter + blizzard.** Add `MuzzleLight` pulse + `CabinLight` +
    Niagara blizzard/muzzle FX, all scattering into fog. Gate: firing throws a warm bloom into the
    cool fog; snow drives past the optic.
11. **PS1 retro layer.** `MPC_Retro` + `MF_VertexSnap` + `PP_Retro` + `BP_RetroController`. Gate:
    one MasterRetro slider slides full-retro ↔ modern-grimy.
12. **Phase-2 movement swap.** Replace scaffolding with `UKesselTrackedMovementComponent` (custom
    track layer, neutral-steer pivot, sub-stepped). Gate: the tank pivots in place and feels heavy;
    remove the Phase-1 component.
13. **Linux bring-up.** Set `LINUX_MULTIARCH_ROOT`, package `-platform=Linux`, verify the build runs
    with the reserved 16/64 fog grid. Gate: Linux ELF build boots into the same scene.

Critical path to "drivable + shooting in fog" is steps 1–8; 9–13 layer on combat opponent, look, feel,
and portability. Phase-1 movement (step 3) intentionally precedes the custom layer (step 12) to prove
the loop fast, per `03`'s phased-hybrid recommendation.

---

## 12. Portability summary (Win + Linux)

- All gameplay uses cross-platform engine APIs only (Chaos forces, line traces, Enhanced Input, UMG,
  fog/light components). No platform `#ifdef` in M1.
- Two render-quality tiers driven by data/CVars, not code branches: 4090 (GridPixelSize 8 / GridSizeZ
  128) vs min-spec/Linux (16 / 64) — `06 §7`.
- Linux toolchain `v26_clang-20.1.8-rockylinux8` + `LINUX_MULTIARCH_ROOT` is the only extra
  prerequisite (`01 §1`); no source differences.

---

## 13. Open items to verify once VS/MSVC is installed (consolidated)

1. Single active `UCameraComponent` auto-selected as pawn view without overriding `CalcCamera`
   (`07 §2.3`) — resolved early at build step 4.
2. `FirstPersonFieldOfView` (`CameraComponent.h:51`) renders interior mesh at fixed FOV while world
   zooms (`07 §2`).
3. Volumetric fog reads correctly inside a narrow-FOV optic camera (should; same main view).
4. Exact CVar names `r.TemporalAA.Upsampling`, `r.Tonemapper.Quality`; absence of a global
   force-point-sampling CVar; MPC parameter cap (`05 §7`).
5. Master `r.VolumetricFog` enable CVar existence; `BaseScalability.ini` "Volumetric Fog" group
   (`06 §7`).
6. Standard `USTRUCT`/`UPROPERTY`/`UENUM`/`UDataAsset`/`FRandomStream` headers for the ballistics
   schema (stable macros, but confirm at C++ time per `04 §14`).
7. Physics sub-stepping config for stable track forces on the 57 t hull (`03 §6`).
8. Whether commander-lay (firing from commander sight) ships in M1 or later (`07 §7`).

---

## 14. Engine API anchor index (verified on disk, UE 5.8)

New anchors confirmed for this doc:
- `UWorldSubsystem : USubsystem` — `Runtime/Engine/Public/Subsystems/WorldSubsystem.h:16`
  (`UTickableWorldSubsystem` at `:80`); `UGameInstanceSubsystem` — `.../GameInstanceSubsystem.h:16`.
- `UPrimaryDataAsset : UDataAsset` — `Runtime/Engine/Classes/Engine/DataAsset.h:47`.
- `UPawnMovementComponent : UNavMovementComponent` — `.../GameFramework/PawnMovementComponent.h:42`.
- `AActor::CalcCamera` — `.../GameFramework/Actor.h:3687`.
- `UPrimitiveComponent::AddForceAtLocation` — `.../Components/PrimitiveComponent.h:1770`
  (`AddForceAtLocationLocal` `:1781`).
- `UWorld::LineTraceSingleByChannel` — `.../Engine/Classes/Engine/World.h:2161`.

All other anchors are carried from the research docs and cited inline above (movement `03`,
ballistics `04`, retro `05`, fog `06`, view/input `07`, modules/templates `01`).
</content>
</invoke>
