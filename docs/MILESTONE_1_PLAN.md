# Kessel — MILESTONE 1 MASTER PLAN

Status: **DESIGN LOCKED** (no C++ compiled this phase — VS/MSVC absent; do not build/launch).
Engine: **UE 5.8.0** (CL 55116800, `++UE5+Release-5.8`), installed at `C:/Program Files/Epic Games/UE_5.8`.
This is the lead synthesis of `docs/ARCHITECTURE.md`, `docs/ART_BIBLE.md`, and `docs/research/01–07`,
reconciled with the four verification audits (engine-API, ballistics, MCP bridge, content/asset completeness).
Engine APIs are cited `path:line` against on-disk 5.8 source. Unprovable items are tagged **ASSUMPTION**.

---

## 1. Vision recap (one paragraph)

You are sealed alone inside cold steel — a lone Tiger operator breaking out of a winter encirclement
("the Kessel"). Through a scratched periscope and a CRT-green optical gunsight, a cold blue-grey
whiteout swallows the world 40–80 m out; enemy tanks loom from nothing as black silhouettes. The only
warmth in the frame is emissive and isolated — the amber pulse of your own muzzle, a burning wreck
bleeding red-orange into the snow-fog — and it *scatters* into the fog volume. **First-person ONLY,
always rendered from inside the interior; never a third-person or exterior view; the player never sees
allies.** Milestone 1 delivers a drivable, shooting tank with believable armor penetration/deflection,
atmospheric volumetric fog with emissive in-scatter, and the multi-sight first-person view system. The
exact graphics style is deliberately FLEXIBLE and TUNABLE: one `MasterRetro` dial slides the whole
image from PS1 whiteout crunch to modern-but-grimy, while the volumetric fog + emissive scatter (the
soul of the game) stay on at every dial position.

---

## 2. LOCKED tech decisions

| Area | Decision | Key grounding / risk |
|---|---|---|
| **Engine / module** | Single runtime module `Kessel` on UE 5.8. `.uproject` explicitly enables `EnhancedInput`, `ChaosVehiclesPlugin`, `Niagara`. Cross-platform APIs only (no `#ifdef`). | `01 §2/§4`. `ChaosVehiclesPlugin` is Experimental + off-by-default (`ChaosVehiclesPlugin.uplugin:13`). |
| **Vehicle approach** | **Phased hybrid.** Phase-1 throwaway scaffolding `UKesselWheeledScaffoldMovement : UChaosWheeledVehicleMovementComponent` (steering disabled per wheel, crude `SetYawInput` pivot) to prove the loop fast; Phase-2 target `UKesselTrackedMovementComponent : UPawnMovementComponent` — custom track layer on the Chaos rigid body (per-side tractive force + high lateral resistance → true neutral-steer pivot), physics sub-stepped. | `03 §3`. Chaos Vehicles 5.8 is a *car* sim: `EVehicleDifferential`/`ESteeringType` wheeled-only (`ChaosWheeledVehicleMovementComponent.h:64-70,433-438`), zero Tank/SkidSteer hits — pivot **must** be authored. |
| **MCP bridge** | **PRIMARY: Epic first-party `ModelContextProtocol` ("Unreal MCP") + `ToolsetRegistry` + selected toolsets.** Ships in-box, precompiled by Epic (zero C++ compile → MSVC-absence is a non-issue), HTTP on `127.0.0.1:8000/mcp`, loopback-only. **FALLBACK: `runreal/unreal-mcp`** (Node→Python Remote Execution) — needs Node.js (absent). **AVOID** community C++ bridges (all ≤5.7, need ABI recompile). | `02 §A/D`. **Risk (med):** must explicitly start/auto-start the server in Editor Preferences → Model Context Protocol before the client connects (connection-refused otherwise); editor must stay open (in-process). **Risk (med):** weak closed-loop read-back this release — material-graph & Blueprint-graph node edits often can't self-verify; treat verification as manual / drop to Python fallback for material work. |
| **View system** | **Three pre-placed `UCameraComponent`s, one active at a time** (Gunner on gun mantlet, Commander buttoned on turret roof, Unbuttoned on hatch), owned by `UKesselViewComponent` `EViewMode` state machine. **Zoom = FOV-stepping** (`SetFieldOfView`, `CameraComponent.h:46`) on a discrete ladder, FInterp ~0.12 s. **Reject** `USceneCaptureComponent2D` optic (doubles scene cost). Reticles = UMG `UUserWidget` per mode. Optic vignette/tint/CA ride camera `PostProcessSettings`. | `07 §0–5`. ASSUMPTION resolved at build step 4: single active camera is auto-selected as pawn view without overriding `CalcCamera` (`Actor.h:3687`). |
| **Ballistics model** | **Data-driven kinetic AP/APCBC** via `UKesselBallisticsSubsystem : UWorldSubsystem` (`WorldSubsystem.h:16`), seeded `FRandomStream`. Pipeline: `theta_n` → `P0=PenAtRange` (tabulated lerp, de Marre only for extrapolation) → overmatch → normalization → **ricochet check before thickness compare** → `LOS=T/cos(theta_eff)` (floored) → residual/spall. HE/HEAT/APCR enum+fields reserved-but-unwired. | `04`. **Apply 3 calibration fixes before/at C++ (see §5 risk register R-B1..R-B3):** the §12/§13 "internal-consistency" acceptance test is mis-stated (model over-predicts angled pen); both `DragCoef k` values are ~2× too low to fit the pen tables; the `THETA_MAX=85°` clamp leaves an 85–90° false-pen window. None block authoring the schema. |
| **Atmosphere** | **One `AExponentialHeightFog`, `bEnableVolumetricFog=true`** (`ExponentialHeightFogComponent.h:136`), driven by `UAtmospherePreset` via confirmed setters (Scattering/Extinction/Albedo/Distance/Density, `:308–320,251`). Warm **Movable** point/spot lights carry `VolumetricScatteringIntensity` 2–4 (muzzle pulsed 6+) (`LightComponentBase.h:120`) → warm core melting to cool halo. `ULocalFogVolumeComponent` for wreck plumes. Blizzard = camera-anchored Niagara. Two quality tiers via CVars (4090: GridPixelSize 8 / GridSizeZ 128; Linux/min-spec 16 / 64). | `06`. Volumetric fog + Local Fog Volume are core-Engine in 5.8 (no plugin). |
| **PS1-tunable** | **One `MasterRetro` (0..1) in `MPC_Retro`** + per-feature dials. Four dial-able layers: (A) low internal res + nearest upscale via CVars (`r.ScreenPercentage`/`r.Upscale.Quality 0`/`r.AntiAliasingMethod 0` — temporal AA steals the upscale, so AA **must** be non-temporal, `SceneUtils.h:44-60`); (B) vertex snap via `MF_VertexSnap` WPO; (C) point-sampled texels; (D) `PP_Retro` posterize+Bayer-dither+palette (`MD_PostProcess` `Runtime/Engine/Public/MaterialDomain.h:23`, `BL_SceneColorAfterTonemapping` `BlendableInterface.h:75`). `BP_RetroController` maps one slider → MPC scalars + CVars. **Volumetric fog/emissive scatter + CRT-green stay ON at every dial position.** Affine texture-warp descoped (needs custom vertex factory). | `05`, ART_BIBLE §2. Default ship look `MasterRetro ≈ 0.7`. |

**Engine-API audit verdict: PASS.** Every load-bearing 5.8 symbol was located on disk with the exact
name claimed. Two cosmetic nits to fold in (non-blocking): bump the five `LocalFogVolumeComponent.h`
field anchors in `06 §4` by +1 line (names all correct: `RadialFogExtinction:21`,
`HeightFogExtinction:25`, `FogPhaseG:37`, `FogAlbedo:41`, `FogEmissive:45`); and carry full paths into
ARCHITECTURE §8 (`Runtime/Engine/Public/MaterialDomain.h:23`,
`Runtime/Engine/Public/Materials/MaterialParameterCollection.h:78`) to avoid a wrong-path lookup at C++ time.

### Static-vs-skeletal — LOCKED (resolves the ARCHITECTURE §2 ↔ research/03 conflict)
**M1 uses NO skeletal mesh, NO rig, NO animation assets.** The pawn is StaticMesh-based: turret yaw and
gun pitch are `SetRelativeRotation` on plain `USceneComponent`s (`TurretRoot`/`GunMount`) holding
`UStaticMeshComponent` visuals. Recoil/hatch = component transforms or simple timelines. Camera attach
points and the projectile spawn point are **child `USceneComponent`s at relative offsets**
(`MuzzlePoint`, `GunnerEye`, `CommanderEye`, `HatchEye`), **not** named mesh sockets — placeholders have
no authored sockets; swap to real sockets only when the real model arrives.

### Scale reference — LOCKED (so every placeholder + the fog recipe agree)
`1 uu = 1 cm`. Player hull `630×300×250 uu`; gunner eye height `~180 uu`; enemies placed `6000–9000 uu`
(60–90 m) out so they sit at the fog silhouette edge; mass `~57000 kg`. Fog grid covers `8000 uu`.

---

## 3. ORDERED build sequence (small verifiable steps)

All steps assume **VS 2022 installed first** (§4 blocker). Critical path to "drivable + shooting in fog"
is steps 0–9; 10–14 layer on opponent, look, feel, portability. Each step lists C++ classes / assets and
a **done-when** gate.

> **Step 0 — Placeholder asset kit (NEW; do before step 3).** Establish the cheap content path so every
> later step has geometry. No DCC art. **Assets:** copy engine BasicShapes
> (`C:/Program Files/Epic Games/UE_5.8/Engine/Content/BasicShapes/{Cube,Cylinder,Cone,Sphere}.uasset`):
> player Hull = Cube scaled to `630×300×250 uu`, `TurretMesh` = Cylinder, `GunMesh` = thin elongated
> Cube/Cylinder, `InteriorMesh` = a crude boxed-in Cube around the camera. Copy the VehicleAdv template
> input assets (`C:/Program Files/Epic Games/UE_5.8/Templates/TP_VehicleAdv/Content/VehicleTemplate/Input/`)
> as the starting `IA_*`/`IMC` set; rename/extend to `IA_Throttle`, `IA_Steer`, `IA_NeutralSteer`,
> `IA_Aim`(Axis2D), `IA_FireMain`, `IA_CycleSight`, `IA_ZoomIn/Out`, `IA_UnbuttonToggle`, `IA_CabinLight`,
> `IA_Headlight`, plus one IMC per view mode. **Done when:** the kit + input assets exist and import clean.

1. **Project skeleton.** Author `Kessel.uproject` + `Source/` targets + `Kessel.Build.cs` (`01 §5`).
   Generate VS files, build `Development Editor | Win64`, open editor. **Classes:** module bootstrap
   (`IMPLEMENT_PRIMARY_GAME_MODULE`). **Done when:** editor opens with the empty `Kessel` module loaded.
2. **Test map + atmosphere first** (so everything later is seen in the real look). **Asset (NEW
   deliverable): `L_KesselTest`** — large scaled BasicShapes Plane (or flat Landscape) as snow ground,
   one `AExponentialHeightFog` (volumetric on, `06 §8` recipe), Movable SkyLight + overcast
   DirectionalLight, a `PlayerStart` at eye height `~180 uu`, 2–3 enemy spawn marks at `6000–9000 uu`,
   a few cube "wrecks". Add `AKesselAtmosphereController` + `UAtmospherePreset` for runtime tuning.
   **Done when:** cold whiteout fog renders and view distance crushes to ~40–80 m.
3. **Pawn + Phase-1 scaffolding movement.** **Classes:** `AKesselTankPawn : APawn` (HullMesh root,
   SimulatePhysics), `UKesselWheeledScaffoldMovement`, `AKesselGameMode` (default pawn),
   `AKesselPlayerController`. Drive via Enhanced Input `IA_Throttle`/`IA_Steer` (from step 0).
   **Done when:** the box-tank drives around in the fog (steering crude/throwaway).
4. **First-person camera + view component (single mode).** **Classes:** `CamGunner` (`UCameraComponent`
   child of `GunMount`) + `UKesselViewComponent` with only `GunnerOptic`. Resolves the `07 §2.3`
   ASSUMPTION: confirm single active camera is the pawn view, else override `CalcCamera`.
   **Done when:** you see first-person from the gun optic while driving.
5. **Turret/gun aim.** **Classes:** `UKesselTurretAimComponent` + `TurretRoot`/`GunMount`
   `USceneComponent`s. Mouse `IA_Aim` → rate-limited, hull-independent turret yaw + gun pitch
   (`FMath::FixedTurn`/`FInterpConstantTo`, limits −8°..+15°). **Done when:** mouse slews a slow heavy
   turret independent of hull heading.
6. **Multi-sight + zoom + reticles.** **Classes:** `CamCommander`/`CamUnbuttoned`, `EViewMode` state
   machine, FOV-step zoom. **Assets:** `UTankSightProfile` (zoom ladder/optic PP), `WBP_GunnerReticle`
   /`WBP_PeriscopeMask`/`WBP_OpenHatchHUD` — first-playable reticle = UMG primitives, centered green
   `#33FF66` crosshair + a few range ticks, **no texture/MID** (defer etched-glass to the polish pass).
   **Done when:** V cycles sights, Z/wheel zooms, H unbuttons, a reticle shows.
7. **Ballistics data + subsystem.** **Classes/structs:** `FShellData`/`FArmorSection`/`FCrewModule`/
   `FVehicleArmorLayout`/`EShellType`, `UBallisticsConfig`, `UKesselBallisticsSubsystem::ResolveKineticHit`.
   **Assets:** `DT_Shells` (`8,8cm_PzGr39`, `85mm_BR365`), `DA_Armor_TigerI`/`DA_Armor_T34_85`.
   **Apply the §5 ballistics fixes here** (recalibrate the 0° pen column or add an angle-effectiveness
   multiplier; refit `DragCoef k`; close the ricochet/clamp gap). Unit-test the (recalibrated) `04 §13`
   duels with synthetic inputs (no projectile yet). **Done when:** the three corrected duel checks pass
   and no path produces NaN.
8. **Collision/trace plan + armor-face mapping (NEW — unblocks step 9's box placeholder).** Define a
   dedicated **`WeaponTrace` collision channel**. **Class:** `UKesselArmorComponent` resolves the armor
   face for a plain cube by comparing hit normal/impact point to the hull's local axes
   (front/side/rear/top/bottom) → 4–6 `FArmorSection` faces — **no bones/sockets needed**. Reserve
   bone/socket mapping for the real model. **Done when:** a trace against the cube returns a deterministic
   `FArmorSection` per face.
9. **Gun + projectile + hit application.** **Classes:** `UKesselGunComponent` (chambered `FShellData`,
   reload timer, spawns from `MuzzlePoint`), `AKesselProjectile` (substepped `LineTraceSingleByChannel`
   path B, anti-tunnel). Wire LMB → projectile → `UKesselArmorComponent` face → subsystem → fragment
   cone vs `FCrewModule[]`. **Done when:** firing at a placed armor box yields pen/ricochet/non-pen
   matching the duel math.
10. **Enemy stub + end-state closure (NEW closure spec).** **Class:** `AKesselEnemyTank : APawn`
    (HullMesh + `UKesselArmorComponent` w/ `DA_Armor_T34_85` + shared `UKesselGunComponent`). Tick: rotate
    turret toward player and fire every N s when player within X m and LOS clear (no nav). **On enemy
    knockout:** stop firing + spawn the burning-wreck emissive light (reuses the atmosphere system → ties
    death into the look for free). **On player crew-pen (crew-of-1):** disable input + camera shake +
    fade-to-black + reload level; add a non-fatal "you are hit" cue (red flash/shake). **Done when:** a
    two-way duel resolves with clear win/lose closure.
11. **Muzzle/cabin emissive scatter + blizzard.** **Components/assets:** `MuzzleLight` (pulsed point) +
    `CabinLight` + camera-anchored Niagara blizzard + a simple `MuzzleFX` sprite system. **Done when:**
    firing throws a warm bloom into the cool fog; snow drives past the optic.
12. **PS1 retro layer.** **Assets:** `MPC_Retro`, `MF_VertexSnap` (WPO), `PP_Retro`
    (posterize/dither/palette), `BP_RetroController`. **Done when:** one `MasterRetro` slider slides
    full-retro ↔ modern-grimy without breaking fog/CRT.
13. **(Optional, decision-gated) thin audio pass.** If approved (§6 OD-3): `AudioComponent` on the pawn
    using engine sample sounds (`Engine/Content/EngineSounds`) — engine loop pitched by speed, gun report
    on Fire, impact on hit. **Done when:** the loop has engine/gun/impact placeholder audio (or this step
    is explicitly deferred in writing).
14. **Phase-2 movement swap + Linux bring-up.** Replace scaffolding with `UKesselTrackedMovementComponent`
    (custom track layer, neutral-steer pivot, sub-stepped); remove the Phase-1 component. Then set
    `LINUX_MULTIARCH_ROOT`, package `-platform=Linux` with the reserved 16/64 fog grid. **Done when:** the
    tank pivots in place and feels heavy, and a Linux ELF build boots into the same scene.

---

## 4. Prerequisites & blockers

**HARD BLOCKER (gates ALL C++):**
- **Visual Studio 2022 — Game Dev C++ workload — currently MISSING** (vswhere not found; no MSVC). Required
  versions (`01 §1`, from `Windows_SDK.json`): VS2022 **17.14.x** (LTSC MSVC `14.44.35207+`; min `14.38.33130`;
  **BANNED:** `14.39.x`, `14.40.0–14.43.x`, `14.44.0–14.44.35210`, `14.50.0–14.50.35722` — do **not** stop at
  17.9/17.10). Windows SDK **10.0.22621.0**. Paste the §1 `VisualStudioSuggestedComponents` IDs (incl.
  `Component.Unreal.Ide`, `Component.Unreal.Debugger`, pinned `VC.14.44.17.14`). Until installed: **no UBT,
  no editor launch, no compile** — paper only.

**Content / placeholder prerequisites (block first PLAYABLE, not compilation — none planned before this doc):**
- **Tank/enemy meshes** — none on disk (`references/` is images only). → Step 0 BasicShapes kit. **(blocker)**
- **Test map** — no level deliverable existed. → Step 2 `L_KesselTest`. **(blocker)**
- **Input data assets** (`IA_*`/`IMC_*`) — C++ binds resolve nothing without them. → Step 0 from VehicleAdv
  template. **(blocker)**
- **Armor-face hit geometry** — a single cube has no per-face bones. → Step 8 axis-compare face mapping +
  `WeaponTrace` channel. **(high)**
- **Interior hero asset** — ART_BIBLE makes the steel interior the on-screen-100% hero; a full Tiger interior
  is the single largest art lift. → **Decouple from first-playable:** UMG aperture overlay (black border /
  periscope-slit / circular optic mask, no texture) + a crude boxed-in `InteriorMesh`; schedule the real
  interior as a post-loop art track. **(high)**
- **Reticle / Niagara / sight assets** — none exist. → UMG-primitive reticles (step 6) and one simple sprite
  emitter each (step 11); defer bespoke FX/etched-glass to polish. **(med/low)**

**Linux secondary prerequisite (only step 14):** cross-toolchain `v26_clang-20.1.8-rockylinux8` +
`LINUX_MULTIARCH_ROOT` env var (`LinuxToolChain.cs:137`, `LinuxPlatformSDK.cs:192`).

**Tooling for the MCP fallback:** **Node.js absent** — until installed, the first-party plugin is effectively
the ONLY available MCP bridge (the only zero-install secondary is the in-editor Python console driven manually,
or RemoteControl REST — not an MCP client). Plan Node.js before relying on `runreal`.

---

## 5. Risk register (from verification) + mitigations

| ID | Risk (severity) | Mitigation |
|---|---|---|
| **R-B1** | **Ballistics — angled-pen over-prediction (high).** The `04 §12/§13` "internal-consistency" acceptance test does not hold: feeding `theta_n=30°` to the 0° pen column over-predicts (model defeats ~142 mm where the historical 30° figure is 127 mm). Model is systematically too lethal at slope. | Either (a) derive the 0° `PenAtRanges` column *through the same model that consumes it* (to land 127 mm at 30° with 5° norm, set 0° value ≈140 mm not 157), or (b) keep 157 as true normal-incidence pen but add an explicit angle-effectiveness multiplier (>1 on effective thickness for `theta_n>30°`). Update §13 duel expectations after recalibration. Apply at build step 7. |
| **R-B2** | **Ballistics — `DragCoef k` ~2× too low (med).** Quoted `k` (88: 1.05e-4, 85: 1.2e-4) reproduce ~10%/km *velocity* loss, not the steeper *pen* falloff in the tables → extrapolated 1500/2000 m and de-Marre-generated guns over-state long-range pen. | Decide what `k` fits. If it fits the pen table: `k≈1.6e-4` (88), `≈2.3e-4` (85). If it fits real velocity: keep Option-1 table strictly authoritative and extrapolate the table's own slope, not velocity-derived de Marre. Fix the inline comments either way. |
| **R-B3** | **Ballistics — 85–90° false-pen window (med).** `RicochetProb` only reaches 1.0 at exactly 90°, so an 85–89.9° "no-ricochet" roll computes LOS at the clamped 85° (armor ~2.5× too thin) → grazing hits falsely penetrate. `COS_MIN=0.0436` is dead code under the 85° clamp. | Pick one: force `RicochetProb=1.0` for `theta_n≥THETA_MAX`, **or** raise `THETA_MAX` to ~89° and let `COS_MIN` be the real cap (~23·T). Make the two clamps consistent. |
| **R-B4** | **Ballistics — guns tuned ~5% hot / minor mislabels (low).** 88 MV 800 vs canonical 773; the forum 30° column runs ~5–6% high; `BR-365` is properly APBC not APCBC. | Acceptable as tunable targets — document explicitly in §14. Optionally align to standard 120/110/100 (88) and give BR-365 a smaller NormalizationAngle. Non-blocking. |
| **R-M1** | **MCP — stopped server looks like a config bug (med).** First-run failure mode: server not started → client gets connection-refused at `127.0.0.1:8000/mcp`. | Add the missing setup step: Editor Preferences → Model Context Protocol → enable + auto-start the server and confirm it's listening **before** `GenerateClientConfig`. Editor must stay open (in-process). |
| **R-M2** | **MCP — weak closed-loop read-back (med).** Material-graph & Blueprint-graph node edits often can't self-verify this release; per-call undo sprawl. M1's fog/emissive material work is exactly the weak area. | Treat verification as manual this release; drop to the runreal Python fallback (or do material-graph work by hand in-editor) for node editing. Don't trust agent self-report on graph edits. |
| **R-V1** | **Vehicle — Chaos Vehicles is Experimental + off-by-default (med).** API may shift across 5.x; (a) scaffolding structurally cannot pivot. | Phase-2 custom layer depends only on stable core (`AddForceAtLocation` `PrimitiveComponent.h:1770`, line traces, rigid body), not the car solver. **Do not ship** Phase-1 steering. Heavy mass needs physics sub-stepping or track forces feel mushy. |
| **R-P1** | **PS1 — temporal AA steals the upscale (med).** With TAA/TSR, `r.Upscale.Quality 0` is ignored → smooth non-retro image (`SceneUtils.h:56-60`). | Full-retro **requires** `r.AntiAliasingMethod 0` (None) so the spatial nearest upscaler runs; pick `r.ScreenPercentage` as an integer divisor (16.667/25/33.3) to avoid shimmer. |
| **R-V2** | **View — single-active-camera assumption unverified (med).** `07 §2.3` assumes one active `UCameraComponent` is auto-selected as pawn view without overriding `CalcCamera`. | Resolve early at build step 4; if false, override `CalcCamera` (`Actor.h:3687`) to report the active camera's POV. Also verify `FirstPersonFieldOfView` keeps the interior fixed while the world zooms. |
| **R-API1** | **Engine-API anchor nits (low).** `06 §4` LocalFogVolume field anchors off by +1; ARCHITECTURE §8 cites some headers by bare filename. | Cosmetic. Bump the five anchors +1; carry full `Runtime/Engine/Public/...` paths into ARCHITECTURE §8. Names/behavior unaffected. |
| **R-C1** | **Content — scope holes if undecided (low/med).** Audio is a scope hole load-bearing to the "feeling" north star but silently deferrable; Niagara/interior could stall the loop if over-built. | Make audio an explicit decision (OD-3). Keep first-playable assets cheap (placeholder kit, UMG reticles, single sprite emitters). Schedule the real interior + bespoke FX as post-loop tracks. |

---

## 6. Open decisions requiring the user

- **OD-1 — Ballistics calibration target (blocks R-B1/R-B2 closure).** Two valid stances: **(A)** keep the
  optimistic forum-sourced values (Tiger tuned ~5% hot) for gameplay punch, accepting the recalibration is
  internal-consistency only; **(B)** re-baseline to canonical Wikipedia/panzerworld tables (88 MV 773; 30°
  column 120/110/100). Which is the desired "feel" baseline? Affects every duel number.
- **OD-2 — Real interior asset timing.** Confirm the interior is decoupled from first-playable (UMG aperture
  overlay + crude box for M1, real Tiger interior as a separate post-loop art track). Is that acceptable, or
  is a modeled interior required inside M1? (It is the single largest art lift; modeling it now stalls the
  playable loop for weeks.)
- **OD-3 — Audio in M1?** **(a)** defer audio to post-M1 in writing, or **(b)** add the thin placeholder pass
  (step 13: engine sample sounds — engine loop pitched by speed, gun report, impact). Even placeholder audio
  massively sells the survival-horror loop. Needs an explicit call so it isn't silently dropped.
- **OD-4 — Commander-lay (firing from the commander sight) in M1 or later?** (`07 §7`; default OFF.)
- **OD-5 — Roof/deck ricochet feel.** Overmatch suppression uses *nominal* thickness (WT-faithful), so thin
  roofs/decks become near-impossible to bounce off even at grazing angles. Keep WT behavior, or add an
  absolute-angle escape (still allow ricochet beyond ~80° regardless of overmatch)?
- **OD-6 — Default `MasterRetro` ship value.** ART_BIBLE proposes **~0.7**; confirm or set the M1 default
  position on the retro↔modern dial.
- **OD-7 — MCP toolset breadth.** Enable `AllToolsets` (broad surface, more token overhead) vs the minimal
  Kessel set (`EditorToolset`, `PhysicsToolsets`, `UMGToolSet`, `SlateInspectorToolset`, material/Niagara)?
- **OD-8 — Node.js install for the MCP fallback.** Install now so `runreal/unreal-mcp` is available for the
  material-graph capability gap (R-M2), or stay first-party-only for M1?
```
