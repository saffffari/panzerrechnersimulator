# 01 — UE 5.8 C++ Project Setup (Windows + Linux, FP Vehicle Sim "Kessel")

Status: research only. No compile/launch performed (VS/MSVC absent per environment ground truth).
Every API/version claim below is grounded in an on-disk engine file (path:line). Items marked ASSUMPTION are not directly provable from engine files and reflect standard UE workflow.

Engine: UE **5.8.0**, CL **55116800**, branch `++UE5+Release-5.8`.
Source: `C:/Program Files/Epic Games/UE_5.8/Engine/Build/Build.version`.

Project name used throughout: **Kessel** (primary game module = `Kessel`). Rename consistently if changed.

---

## 1. Toolchain versions REQUIRED by 5.8 (confirmed)

### Windows (Win64 host + target)
Source of truth: `Engine/Config/Windows/Windows_SDK.json` (the `.cs` SDK files defer to this JSON — see `Engine/Source/Programs/UnrealBuildTool/Platform/Windows/MicrosoftPlatformSDK.Versions.cs:11`).

- **Visual Studio 2022**: minimum **17.8** (`"MinimumVisualStudio2022Version": "17.8"`). VS2026 (18.0) also supported (`"MinimumVisualStudio2026Version": "18.0"`).
- **MSVC toolchain (preferred / what UBT auto-selects)**:
  - `14.50.35717+` = VS2026 18.0, OR
  - `14.44.35207+` = VS2022 **17.14.x** (LTSC)
  (`"PreferredVisualCppVersions"` in Windows_SDK.json)
- **MSVC absolute minimum**: `14.38.33130` (`"MinimumVisualCppVersion"`).
- **BANNED MSVC** (will error): `14.39.x`, `14.40.0–14.43.99999`, `14.44.0–14.44.35210`, `14.50.0–14.50.35722` (`"BannedVisualCppVersions"`). Practical takeaway: install a recent VS2022 17.14.x — do NOT stop at 17.9/17.10.
- **Windows SDK**: main **10.0.22621.0**, minimum **10.0.19041.0** (`"MainVersion"`, `"MinVersion"`).
- **Clang on Windows** (optional alt toolchain): preferred 20.1.x / 19.x / 18.1.8+, min `18.1.8` (`"PreferredClangVersions"`, `"MinimumClangVersion"`).

**Exact VS install set** (from `"VisualStudioSuggestedComponents"` + `"VisualStudio2022SuggestedComponents"` in Windows_SDK.json — paste these component IDs into a `.vsconfig` or select in the VS Installer):
```
Microsoft.VisualStudio.Workload.NativeDesktop      (Desktop development with C++)
Microsoft.VisualStudio.Workload.NativeGame         (Game development with C++)
Microsoft.VisualStudio.Workload.ManagedDesktop     (.NET desktop — for UBT/tooling)
Microsoft.VisualStudio.Component.VC.Tools.x86.x64
Microsoft.VisualStudio.Component.Windows11SDK.22621
Microsoft.VisualStudio.Component.VC.Llvm.Clang
Microsoft.Net.Component.4.6.2.TargetingPack
Component.Unreal.Ide                               (UE editor VS integration)
Component.Unreal.Debugger                          (UE Natvis / debug visualizers)
Microsoft.VisualStudio.Component.VC.14.44.17.14.x86.x64   (pinned LTSC MSVC 14.44)
Microsoft.VisualStudio.Component.VC.14.44.17.14.ATL
```
For Linux cross-compiling from VS, also: `Microsoft.VisualStudio.Workload.NativeCrossPlat` (`"VisualStudioSuggestedLinuxComponents"`).

### Linux (cross-compile target from the Windows host)
Source of truth: `Engine/Config/Linux/Linux_SDK.json`.

- **Cross toolchain version: `v26_clang-20.1.8-rockylinux8`** (Main = Min = Max; only this exact version is accepted).
- AutoSDK platform id: `Linux_x64`.
- Install the matching "-v26_clang-20.1.8-rockylinux8" cross-toolchain archive from Epic, then point UBT at it via the **`LINUX_MULTIARCH_ROOT`** environment variable (required — UBT throws "LINUX_MULTIARCH_ROOT environment variable is not set; cannot instantiate Linux toolchain" without it).
  - Confirmed: `Engine/Source/Programs/UnrealBuildTool/Platform/Linux/LinuxToolChain.cs:137` and `.../LinuxPlatformSDK.cs:192` (reads `LINUX_MULTIARCH_ROOT`). `LINUX_ROOT` is a legacy single-arch fallback (`LinuxPlatformSDK.cs:228`).
- Cross-compiling Linux binaries **from Windows requires no Linux machine** — it produces ELF binaries for `Linux` platform. ASSUMPTION (standard workflow): after VS + toolchain install, `Engine/Build/BatchFiles/RunUAT BuildCookRun ... -platform=Linux` packages the Linux build; the editor's "Platforms > Linux" menu lights up once `LINUX_MULTIARCH_ROOT` is detected.

---

## 2. `.uproject` structure (5.8)

JSON `FileVersion` for project files in 5.8 is `3` (matches every engine `*.uplugin`, e.g. `EnhancedInput.uplugin` line 1). `EngineAssociation` for an installed Launcher build is the engine version string `"5.8"` (ASSUMPTION on exact string form — installed builds use the version; a source build uses a GUID). Set it to `"5.8"` for this Launcher install.

File: `C:/shop/tank_simulator/Kessel.uproject`
```json
{
	"FileVersion": 3,
	"EngineAssociation": "5.8",
	"Category": "",
	"Description": "First-person single-crew tank escape simulator.",
	"Modules": [
		{
			"Name": "Kessel",
			"Type": "Runtime",
			"LoadingPhase": "Default",
			"AdditionalDependencies": [
				"Engine",
				"EnhancedInput"
			]
		}
	],
	"Plugins": [
		{
			"Name": "EnhancedInput",
			"Enabled": true
		},
		{
			"Name": "ChaosVehiclesPlugin",
			"Enabled": true
		},
		{
			"Name": "Niagara",
			"Enabled": true
		}
	],
	"TargetPlatforms": [
		"Windows",
		"Linux"
	]
}
```
Notes / why:
- **`ChaosVehiclesPlugin` MUST be explicitly enabled** — its manifest has `"EnabledByDefault": false` and `"IsExperimentalVersion": true` (`Engine/Plugins/Experimental/ChaosVehiclesPlugin/ChaosVehiclesPlugin.uplugin`). The plugin folder name (`ChaosVehiclesPlugin`) is the `"Name"` you list here; the C++ MODULE it exposes is `ChaosVehicles` (different name — see §4).
- `EnhancedInput` (`Engine/Plugins/EnhancedInput/EnhancedInput.uplugin`) and `Niagara` (`Engine/Plugins/FX/Niagara/Niagara.uplugin`) both have `"EnabledByDefault": true`. Listing them explicitly is harmless and self-documenting.
- Experimental plugin warning: shipping on `ChaosVehiclesPlugin` carries Epic's experimental caveat. For a tracked tank you will likely still drive `UChaosWheeledVehicleMovementComponent` (tracks modeled as wheel sets / differential). Flag for the vehicle-sim design doc.

---

## 3. Source layout

```
C:/shop/tank_simulator/
├── Kessel.uproject
├── Source/
│   ├── Kessel.Target.cs            # Game target (packaged Win64/Linux)
│   ├── KesselEditor.Target.cs      # Editor target
│   └── Kessel/                     # primary game module
│       ├── Kessel.Build.cs
│       ├── Kessel.cpp              # IMPLEMENT_PRIMARY_GAME_MODULE
│       ├── Kessel.h
│       ├── Public/
│       └── Private/
└── Config/
    ├── DefaultEngine.ini
    ├── DefaultGame.ini
    └── DefaultInput.ini
```
The module folder name, `.Build.cs` class name, and the second arg to `IMPLEMENT_PRIMARY_GAME_MODULE` must all equal `Kessel`.

---

## 4. Modules to enable (minimal set, grounded)

| Module name | Provided by | Default-on? | How included |
|---|---|---|---|
| `Core`, `CoreUObject`, `Engine`, `InputCore` | engine runtime | always available | `PublicDependencyModuleNames` |
| `EnhancedInput` | EnhancedInput plugin (Runtime, PreDefault) | yes (`EnabledByDefault:true`) | dep module + plugin listed |
| `ChaosVehicles` | ChaosVehiclesPlugin (Runtime, PreDefault) | **no** (experimental) | dep module + plugin **must be enabled** |
| `PhysicsCore` / `Chaos` / `ChaosCore` | engine | n/a | **transitive** via ChaosVehicles → `SetupModulePhysicsSupport` |
| `Niagara` | Niagara plugin (Runtime) | yes | dep module + plugin |
| `UMG` (+ `Slate`, `SlateCore`) | engine | always available | `PublicDependencyModuleNames` for HUD/sights |
| `RenderCore`, `RHI` | engine | n/a | transitive via ChaosVehicles; add explicitly if you touch render/volumetrics |

Grounding:
- EnhancedInput module manifest: `EnhancedInput.uplugin` Modules → `{"Name":"EnhancedInput","Type":"Runtime","LoadingPhase":"PreDefault"}`.
- ChaosVehicles module manifest: `ChaosVehiclesPlugin.uplugin` Modules → `{"Name":"ChaosVehicles","Type":"Runtime","LoadingPhase":"PreDefault"}`.
- ChaosVehicles' own deps (so you do NOT re-list them): `Core, CoreUObject, Engine, EngineSettings, RenderCore, AnimGraphRuntime, RHI, ChaosVehiclesCore, ChaosVehiclesEngine` plus `SetupModulePhysicsSupport(Target)` — `Engine/Plugins/Experimental/ChaosVehiclesPlugin/Source/ChaosVehicles/ChaosVehicles.Build.cs`.
- `SetupModulePhysicsSupport` adds `PhysicsCore, ChaosCore, Chaos, NNE` as public deps + `Chaos` include path — `Engine/Source/Programs/UnrealBuildTool/Configuration/Rules/ModuleRules.cs:1793-1813`. So depending on `ChaosVehicles` is sufficient to get physics; you only need `PhysicsCore` explicitly if a header you include requires it directly. Listing `PhysicsCore` explicitly is still fine and recommended for clarity.

### Verified class names (for later combat/drive code — confirms the modules expose what M1 needs)
- `AWheeledVehiclePawn : APawn` — `.../ChaosVehicles/Public/WheeledVehiclePawn.h:20`.
- `UChaosWheeledVehicleMovementComponent : UChaosVehicleMovementComponent` — `.../ChaosWheeledVehicleMovementComponent.h:633`.
- `UChaosVehicleMovementComponent : UPawnMovementComponent` — `.../ChaosVehicleMovementComponent.h:721`.
- `UChaosVehicleWheel : UObject` — `.../ChaosVehicleWheel.h:66`.
- Enhanced Input (5.8): `UInputAction : UDataAsset` (`InputAction.h:55`), `UInputMappingContext : UDataAsset` (`InputMappingContext.h:87`), `UEnhancedInputComponent` (`EnhancedInputComponent.h:373`), `UEnhancedInputLocalPlayerSubsystem : ULocalPlayerSubsystem, IEnhancedInputSubsystemInterface` (`EnhancedInputSubsystems.h:21`). Public headers list: `EnhancedInputSubsystems.h`, `EnhancedInputComponent.h`, `InputAction.h`, `InputMappingContext.h`, `InputActionValue.h`, `InputModifiers.h`, `InputTriggers.h`, `PlayerMappableInputConfig.h` (dir listing of `.../EnhancedInput/Source/EnhancedInput/Public`).

### Enhanced Input expectations in 5.8
Standard 5.8 wiring (ASSUMPTION on exact call sites — class names above are verified, the call pattern is the documented UE5 norm):
1. Create `UInputMappingContext` + `UInputAction` data assets (drive, turret yaw/pitch, fire, sight-cycle).
2. In the pawn's `SetupPlayerInputComponent`, cast `PlayerInputComponent` to `UEnhancedInputComponent` and `BindAction(Action, ETriggerEvent::Triggered, this, &AFn)`.
3. In `PawnClientRestart`/`BeginPlay`, get `UEnhancedInputLocalPlayerSubsystem` from the local player and `AddMappingContext(IMC, Priority)`.
4. Project-wide default input classes are set in `Config/DefaultInput.ini` (`DefaultPlayerInputClass=/Script/EnhancedInput.EnhancedPlayerInput`, `DefaultInputComponentClass=/Script/EnhancedInput.EnhancedInputComponent`). The legacy `UInputSettings` axis/action mappings are not used by Enhanced Input.

---

## 5. Ready-to-paste templates

### `Source/Kessel.Target.cs`
Pattern mirrors `Engine/Source/UnrealGame.Target.cs` (verified). `IncludeOrderVersion = Latest` and the shared-build env match 5.8 engine targets.
```csharp
// Copyright Kessel. All Rights Reserved.

using UnrealBuildTool;
using System.Collections.Generic;

[SupportedPlatforms(UnrealPlatformClass.Desktop)]   // Win64 + Linux + Mac; restrict later if needed
public class KesselTarget : TargetRules
{
	public KesselTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Game;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("Kessel");
	}
}
```

### `Source/KesselEditor.Target.cs`
Pattern mirrors `Engine/Source/UnrealEditor.Target.cs` (verified).
```csharp
// Copyright Kessel. All Rights Reserved.

using UnrealBuildTool;
using System.Collections.Generic;

public class KesselEditorTarget : TargetRules
{
	public KesselEditorTarget(TargetInfo Target) : base(Target)
	{
		Type = TargetType.Editor;
		DefaultBuildSettings = BuildSettingsVersion.Latest;
		IncludeOrderVersion = EngineIncludeOrderVersion.Latest;
		ExtraModuleNames.Add("Kessel");
	}
}
```
(Engine targets set `BuildEnvironment = TargetBuildEnvironment.Shared` because they ship with the engine. For a project on an **installed** Launcher build, omit it — installed engines force `Unique` project build environment automatically. ASSUMPTION: leaving it unset is correct for Launcher installs; do NOT copy the `Shared` line.)

### `Source/Kessel/Kessel.Build.cs`
```csharp
// Copyright Kessel. All Rights Reserved.

using UnrealBuildTool;

public class Kessel : ModuleRules
{
	public Kessel(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

		PublicDependencyModuleNames.AddRange(new string[]
		{
			"Core",
			"CoreUObject",
			"Engine",
			"InputCore",
			"EnhancedInput",     // Enhanced Input (plugin module)
			"ChaosVehicles",     // brings PhysicsCore/Chaos/ChaosCore/NNE transitively
			"PhysicsCore",       // explicit for direct physics headers
			"Niagara",           // muzzle flash / fog FX
			"UMG",               // sight reticles / HUD
			"Slate",
			"SlateCore"
		});

		PrivateDependencyModuleNames.AddRange(new string[]
		{
			"RenderCore",
			"RHI"
		});

		// EnhancedInput module is LoadingPhase PreDefault; no special handling needed.
	}
}
```

### `Source/Kessel/Kessel.h`
```cpp
// Copyright Kessel. All Rights Reserved.
#pragma once

#include "CoreMinimal.h"
```

### `Source/Kessel/Kessel.cpp`
Macro signature confirmed: `IMPLEMENT_PRIMARY_GAME_MODULE(ModuleImplClass, ModuleName, GameName)` — `Engine/Source/Runtime/Core/Public/Modules/ModuleManager.h:1100/1114/1130`. Engine reference impl: `Engine/Source/Runtime/UnrealGame/Private/UnrealGame.cpp`.
```cpp
// Copyright Kessel. All Rights Reserved.
#include "Kessel.h"
#include "Modules/ModuleManager.h"

IMPLEMENT_PRIMARY_GAME_MODULE(FDefaultGameModuleImpl, Kessel, "Kessel");
```

### `Config/DefaultInput.ini` (Enhanced Input defaults)
```ini
[/Script/Engine.InputSettings]
DefaultPlayerInputClass=/Script/EnhancedInput.EnhancedPlayerInput
DefaultInputComponentClass=/Script/EnhancedInput.EnhancedInputComponent
```

---

## 6. Project generation & build (DO NOT RUN THIS PHASE — for the build phase)

After VS 2022 (17.14.x, components from §1) is installed:
1. Right-click `Kessel.uproject` → "Generate Visual Studio project files" (invokes `Engine/Build/BatchFiles/Build.bat` / UBT). Produces `Kessel.sln` + `Intermediate/`.
2. Build `Development Editor | Win64` in VS, then open the editor.
3. Linux: set `LINUX_MULTIARCH_ROOT` to the `v26_clang-20.1.8-rockylinux8` toolchain dir, then package via `Engine/Build/BatchFiles/RunUAT.bat BuildCookRun -project=.../Kessel.uproject -platform=Linux -cook -stage -pak`.

All of §6 is the standard UBT workflow (ASSUMPTION on exact menu/cmd strings); the version constraints they enforce are the verified ones in §1.

---

## 7. Open items for downstream docs
- Tracked-vehicle modeling on `UChaosWheeledVehicleMovementComponent` (tank tracks vs wheels) — vehicle-sim design doc.
- Armor penetration/deflection is gameplay C++ on top of Chaos collision/hit results — not a module concern; no extra module needed beyond `Engine`/`PhysicsCore`.
- Volumetric fog + emissive in-scatter is renderer/material config (Exponential Height Fog volumetric + local lights, or a custom volumetric pass) — not a module dependency; revisit if a custom render module is added (would then need `Renderer`/`RHI` explicitly).
