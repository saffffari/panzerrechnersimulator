# 03 — Tracked Vehicle Physics (UE 5.8) — Kessel

Status: RESEARCH (Milestone 1). No C++ authored. All API claims cite on-disk UE 5.8 source.
Target feel: Tiger-like WW2 heavy tank — heavy, sluggish, high momentum, slow turn,
neutral-steer (pivot-in-place), ~20–40 km/h top speed.

---

## 1. Ground truth: what Chaos Vehicles actually provides in 5.8

Plugin: `ChaosVehiclesPlugin` — **Experimental**, **not enabled by default**, Category "Physics".
- `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins/Experimental/ChaosVehiclesPlugin/ChaosVehiclesPlugin.uplugin:13` → `"EnabledByDefault": false`
- Runtime module `ChaosVehicles` + editor module `ChaosVehiclesEditor`.

Confirmed classes (headers under `.../ChaosVehiclesPlugin/Source/ChaosVehicles/Public/`):
- `UChaosVehicleMovementComponent` (base) — `ChaosVehicleMovementComponent.h`
- `UChaosWheeledVehicleMovementComponent : UChaosVehicleMovementComponent` — `ChaosWheeledVehicleMovementComponent.h:633`
- `UChaosVehicleWheel` — `ChaosVehicleWheel.h`
- `AWheeledVehiclePawn` — `WheeledVehiclePawn.h`
- Sim core (no UObject): `FSimpleWheeledVehicle` — `Engine/Source/Runtime/Experimental/ChaosVehicles/ChaosVehiclesCore/Public/SimpleVehicle.h:35`

### Tracked / skid-steer support: NONE (verified)
- Differential is wheeled-only. `enum class EVehicleDifferential` = `{ Undefined, AllWheelDrive, FrontWheelDrive, RearWheelDrive }` — `ChaosWheeledVehicleMovementComponent.h:64-70`. No LSD, no clutch-brake, no left/right (tank) differential.
- Steering is car-only. `enum class ESteeringType` = `{ SingleAngle, AngleRatio, Ackermann }` — `ChaosWheeledVehicleMovementComponent.h:433-438`. No skid/differential steer, no pivot.
- Engine-wide search for `TankVehicle | TrackedVehicle | SkidSteer` over `ChaosVehiclesPlugin/Source` and `Runtime/Experimental` → **0 hits**.
- Modular sim system (`.../ChaosVehiclesCore/Public/SimModule/`) ships Engine/Motor/Clutch/Transmission/Axle/Wheel/Suspension/Chassis/Thruster/Aerofoil modules — **no Track module**.
- In `SteeringUtility.h` the word "Track" means **wheel-track width** (lateral distance between left/right wheels), NOT tank tracks: `SteeringUtility.h:18` "T - Track width", `CalculateBetaDegrees(TrackWidth, WheelBase)`.
- `WheelTrackDimensions` (`FVector2D`) is Wheelbase(X)/Track(Y) — `ChaosWheeledVehicleMovementComponent.h:952`.

Conclusion: Chaos Vehicles 5.8 is a **car simulator**. There is no first-party tracked/skid-steer
path. (This is unchanged from the PhysX/`UTankVehicleMovementComponent`-era loss — that legacy
class does NOT exist in Chaos.) Anything tank-like must be authored.

---

## 2. Options evaluated

### (a) Chaos wheeled, multiple roadwheels/side, fake tracks via differential/yaw
Idea: many `FChaosWheelSetup` entries (`ChaosWheeledVehicleMovementComponent.h:514`, array `Wheels` at `:674`),
all with `bAffectedBySteering=false` (no Ackermann), and approximate tank steering by
torque-biasing left vs right wheels and/or feeding `SetYawInput` (`ChaosVehicleMovementComponent.h:915`).

- PROS: Free per-roadwheel suspension + ground trace (great for the 8-roadwheel Tiger look and
  snow terrain following), wheel contact/slip data for VFX (`FWheelStatus` exposes contact point,
  `bIsSkidding`, `SkidMagnitude`, `PhysMaterial`), engine RPM / transmission / audio hooks,
  `Mass`/COM override handled by the component, async physics thread already wired.
- CONS: The drive/differential model splits torque **front/rear only** — there is no exposed
  **left/right** torque vectoring, which is the entire basis of tank steering. Steering is
  fundamentally Ackermann (wheels turn); **neutral-steer pivot is not achievable** through the
  steering system. `SetYawInput` is an arcade yaw-assist torque, not a track differential — it
  feels floaty, not like grinding one track against the other. Faking per-side drive requires
  bypassing the mechanical sim and driving wheels directly, at which point you are fighting the
  component rather than using it.
- VERDICT: Fast to stand up, good substrate, but cannot deliver authentic pivot/feel alone.

### (b) Custom physics-driven tracked movement component
Idea: a `UPawnMovementComponent` (or thin movement layer on a physics-simulating
`UPrimitiveComponent`) that models two virtual tracks. Each tick: sample N roadwheel contacts per
side via line traces, apply suspension spring/damper forces, then apply **per-track longitudinal
tractive force** at each side's contact line. Steering = asymmetric track forces; neutral-steer
pivot = equal-and-opposite left/right forces with throttle ~0; momentum/weight come from the
rigid body `Mass` + inertia tensor.

- PROS: Exactly the desired model — true neutral-steer pivot, per-track friction, authentic
  momentum and slow turn. Fully tunable (the project requires tunability). Deterministic, no
  fighting a car solver. Cross-platform (pure Chaos rigid-body forces; Win + Linux identical).
- CONS: Must implement suspension traces, ground friction (longitudinal grip + lateral
  track-slip resistance), and track-speed/engine coupling yourself. No built-in audio/anim/wheel
  VFX wiring — must be authored (cheap; we want custom CRT/audio anyway).
- VERDICT: Highest fidelity to the brief, most work, lowest long-term risk.

### (c) Third-party tracked plugin
- No first-party tracked plugin on disk. Marketplace/Fab options exist (community "Advanced
  Tank/Tracked Vehicle" systems) but **none present in this engine install** and none verified
  for 5.8 + Linux. ASSUMPTION: availability/quality/Linux support unknown.
- VERDICT: Rejected for Milestone 1 — unverifiable, external dependency risk on a sim that must
  ship Linux. Revisit only if custom work stalls.

---

## 3. Recommendation: phased hybrid, custom track layer on a Chaos rigid body

Primary target = **(b) custom tracked movement component**, because pivot + weight are core to the
brief and (a) structurally cannot pivot. To de-risk Milestone 1, build it in two phases so we are
"drivable in fog" quickly and only invest in fidelity once the loop is proven.

- **Phase 1 (prototype, ~days): approach (a) as scaffolding.** Subclass
  `UChaosWheeledVehicleMovementComponent`, 4 roadwheels/side (8 total), steering disabled per
  wheel, throttle/brake from keyboard, crude yaw from `SetYawInput`. Goal: validate camera,
  collision, mass feel, snow/fog scene scale. Throwaway-grade steering.
- **Phase 2 (target): approach (b) custom track layer.** Replace locomotion with the custom
  tracked component below. Keep the Chaos rigid body + roadwheel **visual/suspension** trace data.

This keeps one pawn/skeletal setup across both phases; only the force model swaps.

### Component class plan

```
AKesselTankPawn : APawn
  ├─ UStatic/SkeletalMeshComponent  HullMesh        (simulates physics; the rigid body)
  ├─ UKesselTrackedMovementComponent  Movement      (the authored model — see below)
  ├─ USceneComponent  TurretRoot / GunSightSockets  (handled by view-system doc, not here)
  └─ (Phase 1 only) UChaosWheeledVehicleMovementComponent for scaffolding

UKesselTrackedMovementComponent : UPawnMovementComponent
  Data:
    - TrackSide Left, Right  (each: array of roadwheel local offsets, current track linear speed)
    - FEngineModel (RPM, torque-curve lookup, idle, rev rate)  — mirror Chaos engine config shape
    - FTransmissionModel (single forward "gear" ratio + reverse; tanks need little gearing for feel)
  Per physics sub-step (async or substepped on game thread):
    1. For each roadwheel: line-trace down from hull; compute suspension compression →
       spring force (k·x) + damper (c·v) along contact normal; accumulate.
    2. Compute desired track speeds from throttle + steering:
         vL = throttle*vmax + steer*pivotSpeed ;  vR = throttle*vmax - steer*pivotSpeed
       (steer=±1 with throttle 0 → equal/opposite → pivot in place = neutral steer.)
    3. Per side: tractive force = clamp(engineForceFromRPM, ±muLong*sideNormalLoad) applied at the
       average contact point along hull forward axis; sign drives track toward its target speed.
    4. Lateral track resistance: strong force opposing sideways velocity at each contact
       (tracks resist slip far more than tires) — this is what makes it turn on its tracks, not slide.
    5. Apply forces/torques to HullMesh body via AddForceAtLocation / AddTorqueInRadians.
  Tuning hooks: all of section 4 exposed UPROPERTY EditAnywhere with a "GraphicsTunable"-style
    config struct so feel is data-driven (matches project's tunability mandate).
```

Reuse from Chaos where free: `Mass` / `bEnableCenterOfMassOverride` / `CenterOfMassOverride`
(`ChaosVehicleMovementComponent.h:748,754,760`) pattern for mass setup; `FVehicleEngineConfig`
shape (RPM/torque curve) for the engine model so audio + the WW2 feel curve are reusable; and the
async physics callback structure if we want sub-stepping.

---

## 4. Key tunables (initial Tiger-I-flavored values — ASSUMPTION, tune in editor)

Hull / mass — basis `ChaosVehicleMovementComponent.h:745-772`:
- `Mass` ≈ **57000 kg** (Tiger I ~57 t). Heavy mass is the source of the sluggish feel.
- `CenterOfMassOverride` ≈ low + slightly rear (z negative, x slightly back) for stability.
- `ChassisHeight` / inertia: keep yaw inertia high → slow turn initiation.
- `DragCoefficient`: low (tracks/rolling resistance dominate, not air).

Engine — basis `FVehicleEngineConfig`, `ChaosWheeledVehicleMovementComponent.h:223-258`:
- `TorqueCurve` (`:234`) — flat, fat low-end diesel/petrol curve (Maybach HL230 ~700 hp); peak low.
- `MaxTorque` (`:238`) ≈ high; tune so net force/mass gives gentle accel.
- `MaxRPM` (`:242`) ≈ 3000; `EngineIdleRPM` (`:246`) ≈ 500–600 (audible idle for the cabin).
- `EngineRevUpMOI` (`:254`) high, `EngineRevDownRate` (`:258`) low → slow, laggy throttle response.

Top speed / transmission — basis `FVehicleTransmissionConfig` (`:312`):
- Gear ratio set so `vmax` ≈ **20–25 km/h** forward (Tiger road ~38–45 km/h max, but governed/
  cross-country ~20–25 for the trapped-in-snow feel), reverse slower.
- Long `GearChangeTime` or single-gear feel; avoid snappy shifts.

Track friction / steering (custom layer params — no Chaos equivalent, so authored):
- `muLong` (forward grip on snow) ≈ 0.5–0.7 — low enough to lose traction on ice slopes.
- `muLat` (lateral track resistance) HIGH relative to muLong → turns by gripping, not sliding.
- `PivotSpeed` (neutral-steer yaw rate) low → pivot takes ~6–10 s for 360° (heavy + slow).
- `SteerRate` (input slew, deg/s of internal steer ramp) low → no instant turn; momentum-led.
- Per-side `EngineForceMax` / track speed governor.
- (If keeping any Chaos wheels for visuals) per-wheel `FrictionForceMultiplier`
  (`ChaosVehicleWheel.h:106`), `SlipThreshold` (`:114`), `SkidThreshold` (`:118`),
  `LateralSlipGraph` (`:157`), `bAffectedBySteering=false` (`:126`), `bAffectedByEngine` (`:138`).

Suspension (custom, per roadwheel): spring rate moderate, damping high (heavy tank does not
bounce), short travel — Tiger interleaved roadwheels ride stiff.

---

## 5. Input mapping (sim-mode, keyboard drive)

Base component already exposes the verbs we mirror in the custom layer:
- `SetThrottleInput(float)` `ChaosVehicleMovementComponent.h:887`
- `SetBrakeInput(float)` `:899`
- `SetSteeringInput(float)` `:903` (we reinterpret as track differential, not Ackermann)
- `SetYawInput(float)` `:915` (Phase 1 pivot hack only)
- `SetHandbrakeInput(bool)` `:919` (lock tracks)
- `SetTargetGear(int32,bool)` `:939`

Keyboard: W/S throttle+brake, A/D track-differential steer (A/D with no throttle = pivot),
Space = lock both tracks. Mouse reserved for gunsight (separate view-system doc).

---

## 6. Risks

- Chaos Vehicles is **Experimental + off by default** — must enable the plugin; API may shift in
  future 5.x. Mitigated by Phase-2 custom layer depending on stable core (`AddForceAtLocation`,
  line traces, rigid body), not on the experimental car solver.
- (a) cannot pivot — do NOT ship the Phase-1 scaffolding as final steering.
- Custom suspension/friction is the main authoring cost; numbers in §4 are ASSUMPTIONS needing
  in-editor tuning once VS/MSVC is installed (cannot compile this phase).
- No verified third-party tracked plugin; do not plan around one.
- Linux parity: keep model in pure Chaos rigid-body forces (no platform-specific physics calls).
- Async/substepping: heavy mass + low frame forces can feel mushy if applied at game-frame rate;
  plan for physics sub-stepping for stable track forces.
