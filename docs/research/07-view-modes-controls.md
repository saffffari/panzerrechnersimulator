# 07 — First-Person Multi-Sight View System + War Thunder Sim Controls

Project: **Kessel** — single-crew (crew of 1) first-person tank-escape sim. Milestone 1.
Scope of this doc: the three first-person view modes, the camera/view state machine, zoom
strategy, reticle overlay approach, mouse→turret/gun mapping with realistic traverse, and the
full Enhanced Input keybind table. **First-person ONLY. No third-person camera exists in code.**

All engine APIs below were confirmed against the on-disk UE 5.8 source
(`C:/Program Files/Epic Games/UE_5.8`). Citations are `path:line`. Anything not verified on disk
is marked **ASSUMPTION**. WT control conventions are from community references (cited at end) and
are design targets, not engine facts.

---

## 0. TL;DR decisions

| Question | Decision |
|---|---|
| Zoom mechanism | **FOV-stepping on a single `UCameraComponent`** for gunner + commander buttoned views. Reject per-frame `USceneCaptureComponent2D` optic as the default (cost). Keep scene-capture as an OPTIONAL TPP-into-RT path only if we later want a separate-resolution scope picture-in-periscope; not needed M1. |
| Reticle | **UMG widget (`WidgetComponent`-free, screen-space `UUserWidget`)** driven by view state, with the reticle texture in a `UImage`/material. Etched-glass look = a `UMaterial` sampled by the UMG `Image` brush so it can tint/scale with zoom. Reject pure-material-on-postprocess (harder to author, no crisp text). |
| View state owner | A `UTankViewComponent` (UActorComponent) on the player pawn holds the `EViewMode` state machine and swaps active camera + IMC + reticle widget. |
| Turret control | Mouse delta → desired turret yaw / gun pitch **targets**; an interpolator drives actual turret toward target at a capped traverse rate (deg/s), fully decoupled from hull. Gunner camera is parented to the gun mantlet so the optic is gun-laid. |
| Camera count | Three `UCameraComponent`s pre-placed (Gunner on mantlet, Commander-buttoned on turret roof, Unbuttoned on a hatch socket), activated one at a time. Simpler & cheaper than reparenting one camera. |

---

## 1. The three view modes

### Mode A — Gunner's sight (`EViewMode::GunnerOptic`)
- Narrow-FOV gun-laid optic. The camera is **attached to the gun barrel/mantlet socket** so it
  elevates with the gun and traverses with the turret; player sees exactly where the gun points.
- **Multiple discrete zoom steps** (see §4), e.g. x2.5 / x5 / x8 (FOV ~28° / 14° / 8.75°).
- Full reticle overlay: range scale, ranging stadia, ballistic chevrons (the WT "telescopic
  sight" framing). Vignette ring + etched glass tint to sell the optic.
- Aiming is precise but tunnel-vision; this is the "fire" view.

### Mode B — Commander's sight, buttoned (`EViewMode::CommanderButtoned`)
- Wider FOV (~55–70°), camera at the commander's head position **inside** the turret, framed by a
  **periscope / vision-block mask** (UMG overlay with cut-out aperture, mild distortion). Player is
  still protected (buttoned up). Limited situational awareness, no exposure.
- Can free-look (mouse rotates view independently of turret) — see §5 free-look.
- ASSUMPTION (gameplay): optionally "commander-lay" — holding a modifier lets the commander view
  slew the turret toward look direction (WT "use commander sight to aim"). Default OFF in M1.

### Mode C — Unbuttoned commander (`EViewMode::Unbuttoned`)
- Head out of the hatch. Camera on a hatch-top socket, **full ~75–90° FOV, no mask**, free 360°
  look. Best situational awareness — and the only way to spot flanking enemies in the fog.
- **Vulnerable**: exposes the operator to MG/HE/shrapnel (combat system applies a "crew exposed"
  damage modifier; out of scope for this doc but the view state must publish an `bExposed` flag).
- Transition to/from buttoned is a timed animation (hatch open/close) the state machine gates on.

> Because the crew is **1**, these are not separate crew stations — they are the single operator
> physically shifting between the gun telescope, the periscope, and standing up in the hatch. The
> state machine enforces that only one is active and that combat capability differs per mode
> (e.g. firing allowed in A always, in B only if commander-lay enabled, in C the gun keeps its last
> lay and can still fire but you aren't looking through the optic).

---

## 2. Camera & view state machine

### 2.1 Components (verified classes)
- `UCameraComponent` — `Engine/Source/Runtime/Engine/Classes/Camera/CameraComponent.h`.
  - `float FieldOfView;` (`CameraComponent.h:44`); `virtual void SetFieldOfView(float)` (`:46`).
  - `float AspectRatio;` + `bConstrainAspectRatio` (`:108`, `:120`) — keep aspect free, no bars.
  - `FPostProcessSettings PostProcessSettings;` (`:353`) and `float PostProcessBlendWeight;`
    (`:317`), `SetPostProcessBlendWeight(float)` (`:231`) — per-view PP (cold optic tint, vignette,
    chromatic aberration on the gunner scope) rides on the camera itself.
  - Has `FirstPersonFieldOfView` / `bEnableFirstPersonFieldOfView` (`:51`,`:209`) — a separate FOV
    for first-person-rendered meshes (hands/weapon). Useful so the **interior hull mesh** can keep a
    sane FOV while the world zooms. ASSUMPTION we use this for the interior cage; verify behavior in
    editor later.
- Activation: each `UCameraComponent` has `Activate()/Deactivate()` (inherited from
  `UActorComponent`); the active one with lowest unambiguous priority is used by the player camera
  manager when the pawn's camera is auto-managed. We will instead **explicitly** drive view via the
  pawn's `CalcCamera`/an active-camera pointer to avoid ambiguity (see 2.3).

### 2.2 States & transitions
```
EViewMode { GunnerOptic, CommanderButtoned, Unbuttoned }

GunnerOptic  --[CycleSight]-->  CommanderButtoned
CommanderButtoned --[CycleSight]--> GunnerOptic        (cycle is A<->B toggle)
CommanderButtoned --[UnbuttonToggle, hatch-open anim]--> Unbuttoned   (sets bExposed=true)
Unbuttoned   --[UnbuttonToggle, hatch-close anim]--> CommanderButtoned (sets bExposed=false)
GunnerOptic  --[UnbuttonToggle]--> (first snaps to CommanderButtoned, then opens)  // optional guard
```
- `CycleSight` swaps A<->B instantly (both buttoned, no anim).
- `UnbuttonToggle` is the only timed transition; during the hatch anim, input for sight-cycle and
  fire-from-optic is locked; `bTransitioning` true.
- Each state, on Enter: (1) `Deactivate()` previous camera, `Activate()` this camera; (2) swap the
  active **Input Mapping Context** (gunner IMC vs commander IMC share most binds but differ on zoom
  ranges / free-look availability); (3) show the matching reticle/mask widget, hide others;
  (4) set `bExposed`, `bCanFireFromView`, FOV defaults.

### 2.3 Owner: `UTankViewComponent`
- A `UActorComponent` on the pawn (`APawn` subclass `ATankPawn`). Holds `EViewMode CurrentMode`,
  pointers to the three `UCameraComponent`s, the zoom index per optic, and the active reticle widget.
- The pawn overrides `AActor::CalcCamera` (or sets `bFindCameraComponentWhenViewTarget` with a
  single active camera) to report the active camera's POV. Using one-active-camera + the engine's
  auto-find is simplest; **ASSUMPTION**: with exactly one active `UCameraComponent`, the default
  `APawn` view target logic uses it — confirm in editor once VS is installed.

### 2.4 Why three cameras vs one reparented camera
- Three fixed cameras avoid per-frame `AttachToComponent` churn and let each carry its own baked PP
  profile and FOV. Cost is negligible (inactive cameras don't render). Chosen.

---

## 3. Zoom strategy — FOV-stepping (NOT scene capture)

### 3.1 Decision
Use `UCameraComponent::SetFieldOfView` to step the **main view** FOV. This is what WT effectively
does for optics and is by far the cheapest, sharpest, TAA-friendly option.

**Reject** `USceneCaptureComponent2D` rendering the optic into a `UTextureRenderTarget2D` as the
default scope, because:
- It renders the scene a **second time** every frame (`bCaptureEveryFrame`,
  `SceneCaptureComponent.h:85`) — doubles GPU scene cost for the gunner view for no visual gain when
  the optic fills the screen.
- Volumetric fog / lighting in a capture needs `CaptureSource` = FinalColor and full show-flags,
  and won't share the main view's TAA history cleanly.

### 3.2 Scene-capture: keep as an OPTIONAL tool, not the M1 path
`USceneCaptureComponent2D` (`Components/SceneCaptureComponent2D.h`) is verified and available if we
later want a **picture-in-periscope** (a small magnified inset inside a wider commander view):
- `float FOVAngle;` (`SceneCaptureComponent2D.h:44`) — the capture's own FOV (zoom of the inset).
- `TObjectPtr<UTextureRenderTarget2D> TextureTarget;` (`:80`) — render target to sample in UMG.
- `FPostProcessSettings PostProcessSettings;` (`:87`); `CaptureScene()` (`:299`).
- Base `USceneCaptureComponent`: `CaptureSource` (`SceneCaptureComponent.h:81`), `bCaptureEveryFrame`
  (`:85`), `PrimitiveRenderMode` (`:78`), `ShowOnlyActors`/`HiddenActors` (`:148`/`:140`),
  `CaptureSortPriority` (`:160`).
- If used, gate it: only capture while that inset is visible, and cap to a small RT (e.g. 512²) to
  bound cost. **Not implemented in M1.**

### 3.3 Zoom steps and FOV/magnification math
WT optic magnification maps to FOV by `M = FOV_ref / FOV_actual` (small-angle); WT states an x10
scope ≈ 7° FOV, implying a vertical reference ≈ 70°. We adopt **FOV_ref = 70°** for magnification
labeling. Precise relation for finite angles: `M = tan(FOV_ref/2) / tan(FOV_actual/2)`.

Gunner optic discrete steps (tunable data asset, defaults):

| Step | Magnification (label) | Camera FOV (deg) |
|---|---|---|
| 0 | x2.5 | 28.0 |
| 1 | x5.0 | 14.0 |
| 2 | x8.0 | 8.75 |

Commander buttoned periscope steps (wider, fewer):

| Step | Magnification | Camera FOV (deg) |
|---|---|---|
| 0 | x1 (vision block) | 60–70 |
| 1 | x6 (commander optic) | ~11.7 |

Unbuttoned: single FOV ~80° (no zoom; mouse wheel could nudge a small ±10° "lean in" — optional).

- Zoom transitions are **interpolated** (FInterpTo, ~0.12 s) for a smooth optic feel, not snapped.
- The magnification table lives in a `UDataAsset` (`UTankSightProfile`) so the whole zoom ladder is
  tunable per the "graphics style must be tunable" mandate; same asset carries the reticle texture
  and PP profile for that optic.

---

## 4. Reticle / optic overlay approach

### Decision: UMG (`UUserWidget`) screen-space overlay, per view mode.
- Each optic mode owns a widget added to the viewport (`CreateWidget` + `AddToViewport`, hidden via
  visibility, not add/remove, to avoid churn). One widget per mode:
  - **Gunner**: `WBP_GunnerReticle` — central reticle drawn by a `UImage` whose brush is a
    `UMaterialInstanceDynamic` (etched-glass: thin bright lines, slight glow, tint shifts with
    ambient). Range numerals + chevrons baked into the reticle texture (one texture per optic family)
    OR drawn as child `UImage`s that scale/translate with zoom step. Outer ring = vignette `UImage`.
  - **Commander buttoned**: `WBP_PeriscopeMask` — a full-screen `UImage` with an aperture cut-out
    (alpha) framing the visible scene as a vision-block slit/periscope; optional faint reticle.
  - **Unbuttoned**: `WBP_OpenHatchHUD` — minimal/no reticle (you're eyeballing), maybe a thin aim dot.
- **Why UMG over material-on-postprocess**: crisp vector-like lines at any resolution, trivial to
  author range text, easy to swap reticle textures per optic, and it composites above the 3D view
  without a second scene pass. The "etched glass that scatters light" look is achieved with the DMI
  material on the brush (emissive lines + slight bloom contribution), giving the material richness
  without a postprocess pass.
- Reticle reacts to zoom: on zoom step change, the view component pushes the new scale + range-scale
  values into the gunner widget (so stadia spacing stays calibrated to magnification).
- **Material option kept for ONE thing**: the optic **vignette + chromatic aberration + cold tint**
  are done as camera `PostProcessSettings` (verified `CameraComponent.h:353`) on the gunner camera,
  not UMG, so they affect the rendered scene itself (proper lens feel). Reticle lines stay UMG.

---

## 5. Mouse aim → turret/gun, with realistic traverse (hull-independent)

### 5.1 Model
Mouse is a **rate/target controller for the turret and gun**, not a direct camera rotator (sim feel):
- Accumulate mouse X delta → `DesiredTurretYaw` (world/hull-relative), mouse Y → `DesiredGunPitch`.
- Actual turret yaw and gun pitch **chase** the desired values at a **capped angular speed**
  (traverse-rate limited), so whipping the mouse can't teleport the gun.
- Turret traverse and gun elevation are **fully independent of the hull**: turret yaw is stored
  relative to the hull and re-applied each tick, so when the hull rotates the turret holds its
  world-space lay (counter-rotates against hull yaw). Gun pitch is independent of both.

### 5.2 Parameters (tunable, defaults reflect a Tiger I-class heavy)
| Param | Default | Notes |
|---|---|---|
| Turret traverse rate | ~6–10 °/s | Slow (hand-cranked heavy turret feel); data-driven. |
| Gun elevation rate | ~8 °/s | |
| Gun elevation limits | −8° to +15° | Clamp `DesiredGunPitch`. |
| Turret yaw range | 360° continuous | |
| Mouse sensitivity | scalar per view | Lower in gunner (precise), higher unbuttoned. |
| Aim deadzone | small | via `UInputModifierDeadZone`. |

### 5.3 Implementation per tick
```
DesiredTurretYaw += mouseX * sens; DesiredTurretYaw wraps 0..360
DesiredGunPitch  = clamp(DesiredGunPitch - mouseY * sens, minElev, maxElev)
// rate-limited chase, hull-independent:
ActualTurretYawRel = FMath::FixedTurn(ActualTurretYawRel, DesiredTurretYaw, TraverseRate*dt) // shortest-arc, capped
TurretComp->SetRelativeRotation(Yaw=ActualTurretYawRel)   // relative to hull => auto hull-independent
ActualGunPitch = FMath::FInterpConstantTo(ActualGunPitch, DesiredGunPitch, dt, ElevRate)
GunComp->SetRelativeRotation(Pitch=ActualGunPitch)        // relative to turret
```
- In **GunnerOptic**, the camera is a child of `GunComp` (mantlet socket) → optic is gun-laid; what
  you see is where you shoot.
- In **CommanderButtoned/Unbuttoned**, mouse normally **free-looks** the camera (head rotation)
  WITHOUT slewing the turret, unless commander-lay modifier held. So the same `Aim` action is
  routed by the view component to either the turret controller (gunner) or the head/look controller
  (commander), based on `CurrentMode`. This routing is why gunner vs commander use different IMCs or
  different action handlers.
- **Gun stays laid** when you switch to commander/unbuttoned (turret/gun hold last target), so you
  can look around then drop back to the optic and fire.

### 5.4 Enhanced Input wiring (verified classes)
- `UInputAction` data assets (`InputAction.h:55`), `ValueType` Axis1D/Axis2D/Bool (`:116`).
- `UInputMappingContext` (`InputMappingContext.h:87`), `MapKey(Action,Key)` (`:228`).
- Bind in pawn via `UEnhancedInputComponent::BindAction(Action, ETriggerEvent, Obj, Func)`
  (`EnhancedInputComponent.h:482/497`). Trigger events: `Started/Triggered/Ongoing/Completed`
  (`InputTriggers.h:34`).
- Add contexts through `UEnhancedInputLocalPlayerSubsystem::AddMappingContext(IMC, Priority)`
  (`EnhancedInputSubsystems.h:21/37`).
- Triggers available: `UInputTriggerPressed/Released/Down/Hold/Tap/Pulse`
  (`InputTriggers.h:279/298/262/318/365/457`) — e.g. Hold for unbutton, Pulse for held-zoom.
- Modifiers: `UInputModifierScalar` (`:213`, sensitivity), `UInputModifierNegate` (`:253`, reverse
  axis), `UInputModifierDeadZone` (`:170`), `UInputModifierSwizzleAxis` (`:412`),
  `UInputModifierSmoothDelta`/`Smooth` (`:108`/`:275`, mouse smoothing).
- Drive/throttle/turret use Axis1D/Axis2D actions; one-shot actions (fire, cycle, zoom step,
  unbutton) use Bool + Pressed/Hold triggers.

---

## 6. War Thunder sim-style keybind table (Kessel defaults)

Single-crew remapping of WT ground sim conventions. Mouse = turret/gun + look; keyboard = drive +
mode/fire. All rebindable via Enhanced Input IMCs. WT in-game has **no default commander-sight key**
(must be set manually) — we ship sensible defaults.

| Action | Input Action (Axis/Bool) | Default key/binding | Trigger | Notes |
|---|---|---|---|---|
| Drive forward | `IA_Throttle` (Axis1D +) | **W** | Down | Holds throttle. |
| Drive backward / reverse | `IA_Throttle` (Axis1D −) | **S** | Down | Negate modifier. |
| Steer left | `IA_Steer` (Axis1D −) | **A** | Down | Track brake / differential steer. |
| Steer right | `IA_Steer` (Axis1D +) | **D** | Down | |
| Neutral steer (pivot) | `IA_NeutralSteer` | **Left Shift** (mod) + A/D | Down | Counter-rotate tracks in place. |
| Gear up | `IA_GearUp` | **R** | Pressed | If manual gearbox enabled. |
| Gear down | `IA_GearDown` | **F** | Pressed | |
| Toggle auto/manual gearbox | `IA_GearMode` | **G** | Pressed | |
| Handbrake / full stop | `IA_Brake` | **Space** | Down | |
| Turret traverse + gun elevation | `IA_Aim` (Axis2D) | **Mouse XY** | Triggered | Rate/target, hull-independent (§5). |
| Fire main gun | `IA_FireMain` | **Left Mouse** | Pressed | Allowed in Gunner always; commander only if commander-lay on. |
| Fire machine gun | `IA_FireMG` | **Right Mouse** (or **C**) | Down | Coax MG. |
| Cycle sight (Gunner ↔ Commander) | `IA_CycleSight` | **V** | Pressed | A<->B toggle. |
| Gunner scope (force) | `IA_SightGunner` | **Shift (hold)** or **1** | Pressed/Hold | WT: Shift = scope. |
| Commander sight (force) | `IA_SightCommander` | **2** | Pressed | WT default unbound; we bind it. |
| Zoom in (step) | `IA_ZoomIn` | **Mouse Wheel Up** / **Z** | Pressed | Steps up table (§3.3). |
| Zoom out (step) | `IA_ZoomOut` | **Mouse Wheel Down** / **Shift+Z** | Pressed | |
| Hold-zoom (max while held) | `IA_ZoomHold` | **Left Alt** | Hold/Released | Snap to max, restore on release. |
| Button up / unbutton toggle | `IA_UnbuttonToggle` | **H** | Hold (0.25s) | Hatch open/close anim; sets bExposed. |
| Free-look (override) | `IA_FreeLook` | **Left Alt (hold)** in buttoned, default-on unbuttoned | Hold | Look without slewing turret. |
| Commander-lay modifier (slew turret from commander view) | `IA_CommanderLay` | **Middle Mouse (hold)** | Hold | Optional; off by default in M1. |
| Reset view / recenter optic to gun | `IA_RecenterView` | **Mouse Wheel Click** or **End** | Pressed | Snap free-look back to gun line. |
| Rangefinder / set sight distance + | `IA_RangeUp` | **PageUp** | Pressed/Pulse | Reticle range scale. |
| Rangefinder − | `IA_RangeDown` | **PageDown** | Pressed/Pulse | |
| Toggle interior/instrument light | `IA_CabinLight` | **L** | Pressed | Emissive interior (fog scatter showcase). |
| Headlight / driving light | `IA_Headlight` | **K** | Pressed | Volumetric beam into fog. |
| Pause / menu | `IA_Menu` | **Esc** | Pressed | |

Notes:
- **Aim routing**: `IA_Aim` is bound in every IMC but its handler checks `CurrentMode` to route to
  turret (gunner) vs head free-look (commander/unbuttoned), per §5.3.
- **Left Shift overloaded** (neutral-steer modifier vs WT "scope") — resolved by IMC priority: the
  drive IMC owns Shift as a steer modifier; the gunner-scope use is moved to **1** to avoid the
  clash. Documented so user can rebind to taste.
- WT parity callouts: Shift=scope, mouse-wheel/Z=zoom, commander sight unbound-by-default — matched
  or improved. (Sources §8.)

---

## 7. Open questions / to verify when MSVC is installed
1. Confirm a single active `UCameraComponent` is auto-selected as the pawn view without overriding
   `CalcCamera` (2.3 ASSUMPTION).
2. Confirm `FirstPersonFieldOfView` (`CameraComponent.h:51`) renders interior hull mesh at a fixed
   FOV while world FOV zooms — desired so the optic zooms the world but not the eyecup geometry.
3. Validate volumetric fog reads correctly inside a narrow-FOV camera (it should; same main view).
4. Decide whether commander-lay (firing from commander sight) ships in M1 or later.
5. Tune traverse rates against the reference "heavy, slow turret" feel.

## 8. Sources (WT control/optic conventions — design targets, not engine facts)
- Steam Community guide, WT ground keybinds: https://steamcommunity.com/sharedfiles/filedetails/?id=2927093323
- War Thunder Wiki — Optics: https://wiki.warthunder.com/mechanics/235-optics
- War Thunder Wiki (old) — Optics / Target Magnification: https://old-wiki.warthunder.com/Optics

## 9. Engine API anchors (verified on disk, UE 5.8)
- `CameraComponent.h:44` FieldOfView; `:46` SetFieldOfView; `:51` FirstPersonFieldOfView;
  `:108` AspectRatio; `:317` PostProcessBlendWeight; `:353` PostProcessSettings.
- `Components/SceneCaptureComponent2D.h:44` FOVAngle; `:80` TextureTarget; `:87` PostProcessSettings;
  `:299` CaptureScene.
- `Components/SceneCaptureComponent.h:78` PrimitiveRenderMode; `:81` CaptureSource;
  `:85` bCaptureEveryFrame; `:140` HiddenActors; `:148` ShowOnlyActors; `:160` CaptureSortPriority.
- `EnhancedInput/.../InputAction.h:55` UInputAction; `:116` ValueType.
- `.../InputMappingContext.h:87` UInputMappingContext; `:228` MapKey.
- `.../InputTriggers.h:34` ETriggerEvent enum; `:262/279/298/318/365/457`
  Down/Pressed/Released/Hold/Tap/Pulse triggers.
- `.../InputModifiers.h:108/170/213/253/412` SmoothDelta/DeadZone/Scalar/Negate/SwizzleAxis.
- `.../EnhancedInputComponent.h:482,497` BindAction overloads.
- `.../EnhancedInputSubsystems.h:21` UEnhancedInputLocalPlayerSubsystem; `:37` AddMappingContext.
