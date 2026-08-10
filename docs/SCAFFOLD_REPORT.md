# Kessel — C++ Scaffold Report (Milestone 1, Build Step 1)

Status: **AUTHORED — NOT YET COMPILED.** Visual Studio / MSVC was installing during this phase, so
per the environment ground truth **no UBT run, no VS project generation, no compile, no editor launch
was performed.** Everything below is text authored against the on-disk UE 5.8.0 engine source
(`C:/Program Files/Epic Games/UE_5.8`, CL 55116800, `++UE5+Release-5.8`) and is queued for the first
green build once the toolchain lands.

Two adversarial audits (engine-API/MSVC compile-readiness, and architecture consistency vs
`ARCHITECTURE.md` §2 + `MILESTONE_1_PLAN.md`) were run against all scaffold files; all 11 raised issues
were fixed. Final verdict: **PASS — no compile blockers.** Every base class, include path,
`.generated.h` placement, macro, and cross-file `KESSEL_API` use was confirmed against on-disk 5.8
headers; all method bodies are empty/trivial (only `CreateDefaultSubobject` / `SetupAttachment` /
pointer-assignment seams), so there is no speculative-API surface.

---

## 1. File manifest (what was scaffolded)

### Project root
| Path | Purpose | Grounding |
|---|---|---|
| `Kessel.uproject` | `FileVersion 3`, `EngineAssociation "5.8"`; enables `EnhancedInput`, `ChaosVehiclesPlugin`, `Niagara`; `TargetPlatforms` Windows + Linux. | research/01 §2 |
| `.gitignore` | Ignores `Binaries/ Build/ Intermediate/ Saved/ DerivedDataCache/`, `*.sln`, `*.generated.h`; keeps `Config/ Source/ docs/ references/`. | — |

### Build system (`Source/`)
| Path | Type / base | Grounding |
|---|---|---|
| `Source/Kessel.Target.cs` | `KesselTarget : TargetRules`, `Type=Game`, `BuildSettingsVersion.Latest`, `IncludeOrderVersion.Latest`. | research/01 §5 (mirrors `Engine/Source/UnrealGame.Target.cs`) |
| `Source/KesselEditor.Target.cs` | `KesselEditorTarget : TargetRules`, `Type=Editor`. | research/01 §5 (mirrors `UnrealEditor.Target.cs`) |
| `Source/Kessel/Kessel.Build.cs` | `Kessel : ModuleRules`. Public deps: `Core, CoreUObject, Engine, InputCore, EnhancedInput, ChaosVehicles, PhysicsCore`. Private: `RenderCore, RHI`. `Niagara/UMG/Slate/SlateCore` are **commented out**, deferred to build steps 6/10 until a file first includes their headers (audit fix #4). | research/01 §4 |
| `Source/Kessel/Kessel.h` | `#pragma once` + `CoreMinimal.h`. | research/01 §5 |
| `Source/Kessel/Kessel.cpp` | `IMPLEMENT_PRIMARY_GAME_MODULE(FDefaultGameModuleImpl, Kessel, "Kessel")`. | `ModuleManager.h:1100/1114/1130` |
| `Source/Kessel/Public/.gitkeep`, `Source/Kessel/Private/.gitkeep` | Keep empty dirs tracked. | — |

### Config (`Config/`)
| Path | Purpose |
|---|---|
| `Config/DefaultEngine.ini` | Engine/renderer defaults. |
| `Config/DefaultGame.ini` | Project/game defaults. |
| `Config/DefaultInput.ini` | Enhanced Input defaults (`DefaultPlayerInputClass=/Script/EnhancedInput.EnhancedPlayerInput`, `DefaultInputComponentClass=/Script/EnhancedInput.EnhancedInputComponent`). |

### Gameplay C++ — Public headers (`Source/Kessel/Public/`)
| Header | Type → verified base | Grounding (path:line) |
|---|---|---|
| `KesselTankPawn.h` | `AKesselTankPawn : APawn` — owns HullMesh root, `TurretRoot`/`GunMount` `USceneComponent`s, attach points (`MuzzlePoint`/`GunnerEye`/`CommanderEye`/`HatchEye`), 3× `UCameraComponent` (`CamGunner`→`GunMount`, `CamCommander`/`CamUnbuttoned`→`TurretRoot`), and owned `Gun`/`Armor`/`View`/`Aiming` components. Holds `UInputAction*` refs only (no IMC). | `GameFramework/Pawn.h`; `Camera/CameraComponent.h:32` |
| `KesselWheeledScaffoldMovement.h` | `UKesselWheeledScaffoldMovement : UChaosWheeledVehicleMovementComponent` (Phase-1 throwaway). Ctor `(const FObjectInitializer&)` forwards to Super (audit fix #1). | `ChaosWheeledVehicleMovementComponent.h:633` |
| `KesselTrackedMovementComponent.h` | `UKesselTrackedMovementComponent : UPawnMovementComponent` — **empty Phase-2 seam stub** (TODO step 14). | `ChaosVehicleMovementComponent.h:721` (UPawnMovementComponent base) |
| `KesselGameMode.h` | `AKesselGameMode : AGameModeBase` — sets `DefaultPawnClass`/`PlayerControllerClass`. | `GameFramework/GameModeBase.h` |
| `KesselPlayerController.h` | `AKesselPlayerController : APlayerController` — **sole `AddMappingContext` owner**; holds `DefaultMappingContext` (`UInputMappingContext*`) (audit fix #11). | `GameFramework/PlayerController.h` |
| `KesselViewComponent.h` | `UKesselViewComponent : UActorComponent` — `EViewMode` state, `SetCameras(...)` seam. | `Components/ActorComponent.h` |
| `KesselTurretAimComponent.h` | `UKesselTurretAimComponent : UActorComponent` — `SetTurretComponents(TurretRoot, GunMount)`. | `Components/ActorComponent.h` |
| `KesselSightProfile.h` | `UTankSightProfile : UPrimaryDataAsset` — FOV zoom ladder, `UTexture2D*` reticle, `FPostProcessSettings` optic PP (audit fix #7, M1 step 6). | `Engine/DataAsset.h:47`; `Engine/Scene.h:711` |
| `KesselBallisticsTypes.h` | `EShellType` (UENUM), `FShellData`/`FArmorSection`/`FCrewModule`/`FVehicleArmorLayout` (USTRUCT), `FHitResolution` (`{bPen,bRicochet,LOS,P0,residualFrac,spallPower,entryPoint,dir}`, audit fix #8). Shared seam included by Armor/Gun/Projectile/Subsystem. | ARCHITECTURE §4 |
| `KesselBallisticsConfig.h` | `UBallisticsConfig : UDataAsset` (locked: stays `UDataAsset`). C++ type only — no `.uasset`/`.csv` fabricated. | `Engine/DataAsset.h` |
| `KesselBallisticsSubsystem.h` | `UKesselBallisticsSubsystem : UWorldSubsystem` — `ResolveKineticHit(Shell, Section, ImpactDir, SurfaceNormal, Range)` (audit fix #8). | `Subsystems/WorldSubsystem.h:16` |
| `KesselArmorComponent.h` | `UKesselArmorComponent : UActorComponent` — per-face axis-compare mapping; refs `FArmorSection::Name` (audit fix #2). | `Components/ActorComponent.h` |
| `KesselGunComponent.h` | `UKesselGunComponent : UActorComponent` — chambered `FShellData`, reload timer, muzzle spawn. | `Components/ActorComponent.h` |
| `KesselProjectile.h` | `AKesselProjectile : AActor` — substepped trace path. | `GameFramework/Actor.h` |
| `KesselEnemyTank.h` | `AKesselEnemyTank : APawn` — `TurretRoot`/`GunMount` chain, `TurretMesh`/`GunMesh` hit geometry, shared `UKesselTurretAimComponent` (audit fix #6). | `GameFramework/Pawn.h` |
| `AtmospherePreset.h` | `UAtmospherePreset : UPrimaryDataAsset` (audit fix #9/10). | `Engine/DataAsset.h:47` |
| `KesselAtmosphereController.h` | `AKesselAtmosphereController : AActor`, `UCLASS(Blueprintable)` (audit fix #3). | `GameFramework/Actor.h` |
| `KesselAudioComponent.h` | `UKesselAudioComponent : UActorComponent` — **placeholder seam only**, TODO (full audio designed separately). | `Components/ActorComponent.h` |

### Gameplay C++ — Private sources (`Source/Kessel/Private/`)
Matching `.cpp` for each non-stub class (bodies empty/trivial-safe):
`KesselTankPawn.cpp`, `KesselWheeledScaffoldMovement.cpp`, `KesselGameMode.cpp`,
`KesselPlayerController.cpp`, `KesselViewComponent.cpp`, `KesselTurretAimComponent.cpp`,
`KesselBallisticsSubsystem.cpp`, `KesselArmorComponent.cpp`, `KesselGunComponent.cpp`,
`KesselProjectile.cpp`, `KesselEnemyTank.cpp`, `KesselAtmosphereController.cpp`,
`KesselAudioComponent.cpp`.
(`KesselTrackedMovementComponent`, `KesselSightProfile`, `KesselBallisticsTypes`,
`KesselBallisticsConfig`, `AtmospherePreset` are header-only — no `.cpp` by design.)

**Total: 1 uproject + 1 gitignore + 5 build/module files + 2 .gitkeep + 3 Config + 18 Public headers
+ 13 Private cpp = 43 authored/queued files.**

Every `UObject`-derived header includes `CoreMinimal.h`, its base header, and its own
`*.generated.h` **as the last include**; cross-file types carry `KESSEL_API`.

---

## 2. EXACT first-build command sequence (run only after VS finishes installing)

Install paths for this machine: engine `C:/Program Files/Epic Games/UE_5.8`; project
`C:/shop/tank_simulator/Kessel.uproject`. Required toolchain (research/01 §1, `Windows_SDK.json`):
**VS2022 17.14.x** (LTSC MSVC `14.44.35207+`; min `14.38.33130`; do NOT stop at 17.9/17.10 — several
intermediate MSVC versions are BANNED), Windows SDK `10.0.22621.0`, Game-Dev-C++ workload +
`Component.Unreal.Ide`/`Component.Unreal.Debugger`.

### Step A — Generate Visual Studio project files from `Kessel.uproject`
**Option 1 (GUI, simplest):** In Explorer, right-click `C:/shop/tank_simulator/Kessel.uproject` →
**"Generate Visual Studio project files"** (the shell verb registered by Epic's
`UnrealVersionSelector.exe`, typically at
`C:/Program Files (x86)/Epic Games/Launcher/Engine/Binaries/Win64/UnrealVersionSelector.exe`).
Produces `Kessel.sln` + `Intermediate/ProjectFiles/`.

**Option 2 (command line, deterministic — VERIFIED batch file exists at
`C:/Program Files/Epic Games/UE_5.8/Engine/Build/BatchFiles/Build.bat`):**
```cmd
"C:\Program Files\Epic Games\UE_5.8\Engine\Build\BatchFiles\Build.bat" -projectfiles -project="C:\shop\tank_simulator\Kessel.uproject" -game -rocket -progress
```
(`-rocket` = installed/Launcher build. Equivalent direct UBT call, exe VERIFIED at
`Engine/Binaries/DotNET/UnrealBuildTool/UnrealBuildTool.exe`:
`UnrealBuildTool.exe -projectfiles -project="C:\shop\tank_simulator\Kessel.uproject" -game -engine -progress`.)

### Step B — Build "Development Editor | Win64"
**Option 1 (VS):** open `Kessel.sln`, set config **Development Editor**, platform **Win64**, set
`Kessel` as startup project, Build.

**Option 2 (command line — `KesselEditor` target, Development, Win64):**
```cmd
"C:\Program Files\Epic Games\UE_5.8\Engine\Build\BatchFiles\Build.bat" KesselEditor Win64 Development -project="C:\shop\tank_simulator\Kessel.uproject" -waitmutex
```

### Step C — Open the editor (editor exe VERIFIED at `Engine/Binaries/Win64/UnrealEditor.exe`)
```cmd
"C:\Program Files\Epic Games\UE_5.8\Engine\Binaries\Win64\UnrealEditor.exe" "C:\shop\tank_simulator\Kessel.uproject"
```
**Done-when (MILESTONE_1_PLAN step 1):** the editor opens with the empty `Kessel` module loaded.

---

## 3. Immediately-after: enable the first-party Unreal MCP server + register with Claude Code

(research/02 §A/D; MILESTONE_1_PLAN §2 MCP row. Server is precompiled by Epic — needs no MSVC.)

1. Editor → **Edit → Plugins** → enable **Unreal MCP** (`ModelContextProtocol`, on-disk at
   `Engine/Plugins/Experimental/ModelContextProtocol/ModelContextProtocol.uplugin`). Accept the
   auto-enable of **Toolset Registry** (`ToolsetRegistry`). Enable toolsets — either **All Toolsets**
   (`AllToolsets`) for the broad surface, or the minimal Kessel set (`EditorToolset`,
   `PhysicsToolsets`, `UMGToolSet`, `SlateInspectorToolset`, material/Niagara). Restart when prompted.
2. **Edit → Editor Preferences → Model Context Protocol → enable + auto-start the server** and confirm
   it is **listening on `http://127.0.0.1:8000/mcp`** (loopback-only) **before** generating the client
   config. This is risk **R-M1**: a not-yet-started server returns connection-refused and looks like a
   config bug. The editor must stay open (the server is in-process).
3. In the editor console (`~` / Output Log cmd line) run:
   ```
   ModelContextProtocol.GenerateClientConfig ClaudeCode
   ```
   This writes `.mcp.json` into the project root (`C:/shop/tank_simulator`).
4. **Register with Claude Code:** launch Claude Code from `C:/shop/tank_simulator` so it auto-discovers
   the generated `.mcp.json`. Manual/explicit equivalent (HTTP transport — verify exact flag with
   `claude mcp add --help`; the generated file is the authoritative path):
   ```
   claude mcp add --transport http unreal-mcp http://127.0.0.1:8000/mcp
   ```
5. Fallback (only if a material-graph/Blueprint-graph tool is missing — R-M2): `runreal/unreal-mcp`
   over Python Remote Execution, which needs **Node.js (currently absent)**. Defer until needed.

---

## 4. Known risks — what might still not compile, and why

1. **Untested by definition.** No MSVC was available, so nothing was compiled. "PASS" is a static audit
   against on-disk 5.8 headers, not a build result. First real compile is the true test.
2. **Scaffold movement ctor vs the Chaos legacy-macro base (highest residual, judged safe).**
   `UChaosWheeledVehicleMovementComponent` uses `GENERATED_UCLASS_BODY`, so it expects an
   `(const FObjectInitializer&)` constructor. The audit fix gives `UKesselWheeledScaffoldMovement` an
   explicit `(const FObjectInitializer& OI) : Super(OI)` ctor — proven safe by the engine's own
   `UPawnMovementComponent`/`UNavMovementComponent` pattern. If 5.8 changed the macro contract this is
   the single most likely first-compile failure; low probability.
3. **`UBallisticsConfig` left as `UDataAsset`** (locked brief), while peers (`UAtmospherePreset`,
   `UTankSightProfile`) are `UPrimaryDataAsset`. Both bases verified in `Engine/DataAsset.h`; the mix is
   intentional, not a bug.
4. **`Niagara`/`UMG`/`Slate`/`SlateCore` are commented out of `Kessel.Build.cs`.** They are added back
   the moment a file first includes their headers (build steps 6/10). If a header is added without
   re-enabling its module, that file (not the scaffold) will fail to link — a step-local task, not a
   scaffold defect.
5. **`.uasset`/`.csv`/`.umap` data deliberately absent.** Per the locked brief only C++ types exist;
   `DT_Shells`, `DA_Armor_*`, `UTankSightProfile` instances, `L_KesselTest`, and the `IA_*`/`IMC_*`
   input assets are editor-authored later (steps 0/2/6/7). C++ input binding resolves nothing until the
   `IA_*`/`IMC_*` assets exist — expected, not a compile error.
6. **`ChaosVehiclesPlugin` is Experimental + off-by-default** (`ChaosVehiclesPlugin.uplugin`,
   `EnabledByDefault:false`, `IsExperimentalVersion:true`); it is explicitly enabled in the uproject.
   API drift across 5.x is possible (R-V1) — but Phase-1 is throwaway and Phase-2 depends only on stable
   core physics, so the seam is insulated.
7. **`.gitignore` ignores `*.generated.h`** (correct — UHT regenerates them) and `*.sln`. Anyone cloning
   must run Step A locally to regenerate project files; no `.generated.h` is committed.
8. **`EngineAssociation "5.8"` is an ASSUMPTION** for Launcher installs (research/01 §2). If the editor
   prompts to "fix" the association on first open, accept it — that rewrites the field to the exact
   installed identifier.

---

## 5. What's next — mapping to MILESTONE_1_PLAN build steps 2+

The scaffold completes **step 1 (project skeleton)** and pre-stages classes for steps 3–10. Critical
path to "drivable + shooting in fog" is steps 0–9.

- **Step 0 — Placeholder asset kit + input assets** (editor work; do before step 3). Copy engine
  BasicShapes (`Engine/Content/BasicShapes/{Cube,Cylinder,Cone,Sphere}`) for Hull/Turret/Gun/Interior;
  copy/rename VehicleAdv template `IA_*`/`IMC` into `IA_Throttle/Steer/NeutralSteer/Aim/FireMain/
  CycleSight/ZoomIn/Out/UnbuttonToggle/CabinLight/Headlight` + one IMC per view mode. These feed the
  pawn's `UInputAction*` refs and the controller's `DefaultMappingContext`.
- **Step 2 — `L_KesselTest` + atmosphere.** Build the test level (snow plane, one
  `AExponentialHeightFog` volumetric-on, SkyLight + overcast DirectionalLight, `PlayerStart` at ~180 uu,
  enemy spawn marks at 6000–9000 uu, cube wrecks). Place `AKesselAtmosphereController` + author a
  `UAtmospherePreset` instance. **Done when:** cold whiteout crushes view to ~40–80 m.
- **Step 3 — Pawn + Phase-1 drive.** Fill `AKesselTankPawn` body (HullMesh `SimulatePhysics`), wire
  `UKesselWheeledScaffoldMovement`, set `AKesselGameMode` defaults, bind `IA_Throttle`/`IA_Steer`.
  **Done when:** the box-tank drives in fog (crude steering).
- **Step 4 — FP camera + view component (single mode).** Activate `CamGunner` via
  `UKesselViewComponent` (`GunnerOptic` only). Resolve R-V2: confirm single active camera is the pawn
  view, else override `CalcCamera` (`Actor.h:3687`).
- **Step 5 — Turret/gun aim.** Implement `UKesselTurretAimComponent` (rate-limited yaw + pitch
  −8°..+15°, `FMath::FixedTurn`/`FInterpConstantTo`) on the `TurretRoot`/`GunMount` chain.
- **Step 6 — Multi-sight + zoom + reticles.** Add `CamCommander`/`CamUnbuttoned`, `EViewMode` machine,
  FOV-step zoom; author `UTankSightProfile` instances + UMG reticles. **Re-enable `UMG`/`Slate` deps.**
- **Step 7 — Ballistics data + subsystem.** Implement `ResolveKineticHit`; author `DT_Shells`,
  `DA_Armor_TigerI`, `DA_Armor_T34_85`. **Apply the §5 ballistics fixes R-B1..R-B3** (angled-pen
  recalibration, `DragCoef k` refit, ricochet/clamp gap) and unit-test the corrected duels.
- **Step 8 — Collision/trace + armor-face mapping.** Define `WeaponTrace` channel; implement
  `UKesselArmorComponent` axis-compare face resolution.
- **Step 9 — Gun + projectile + hit application.** Implement `UKesselGunComponent` (reload, muzzle
  spawn) + `AKesselProjectile` (substepped `LineTraceSingleByChannel`, anti-tunnel); wire
  LMB → projectile → armor face → subsystem → fragment cone vs `FCrewModule[]`.
- **Step 10 — Enemy stub + end-state.** Fill `AKesselEnemyTank` Tick (turret-to-player, fire on
  LOS+range); knockout spawns burning-wreck emissive light; player crew-pen → disable/fade/reload.
  **Re-enable `Niagara` for FX in/after step 10/11.**
- **Steps 11–14** (layer-on): muzzle/cabin emissive scatter + Niagara blizzard (11); PS1 retro layer —
  `MPC_Retro`/`MF_VertexSnap`/`PP_Retro`/`BP_RetroController` (12); optional thin audio pass into
  `UKesselAudioComponent` (13, decision OD-3); Phase-2 movement swap to
  `UKesselTrackedMovementComponent` + Linux bring-up via `LINUX_MULTIARCH_ROOT` (14).

Open decisions still gating content: OD-1 ballistics baseline, OD-2 interior timing, OD-3 audio in M1,
OD-7 MCP toolset breadth, OD-8 Node.js install.
