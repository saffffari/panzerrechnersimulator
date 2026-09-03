# Kessel — INTEGRATED PRODUCTION PLAN v2

> **Superseded plan (2026-09-02):** this is the archived Unreal/C++ integration plan, not the current
> plan of record. Use `docs/STATUS.md` and `docs/LINUX_4090_HANDOFF.md` for the Rust/Linux production
> order and settled five-man crew/medical/Reichstag direction. Old open decisions remain below as a
> record of how the design evolved; they are not awaiting answers.

Status: **HISTORICAL PLAN.** Folds the six team docs (`docs/teams/*`), the locked
`MILESTONE_1_PLAN` (steps 0–14), the `SIMULATION_ROADMAP` (#1–#20), and the cross-team
integration + feasibility findings into one dependency-ordered roadmap and a single
**execution sequence** that respects the hard single-live-editor constraint.

Reads as the lead synthesis. Nothing here overrides a team doc's internal detail; it
**sequences** them, **resolves the conflicts the integration pass found**, and partitions
all work into three non-colliding lanes so Claude-main (the one live editor connection),
the C++ author, and the user never contend for the same resource.

Grounding (do not contradict): `DESIGN_PILLARS.md`, `GAME_DESIGN.md`, `ARCHITECTURE.md`,
`ART_BIBLE.md`, `AUDIO_ARCHITECTURE.md`, `MILESTONE_1_PLAN.md`, `SIMULATION_ROADMAP.md`,
`docs/teams/{atmosphere-system, world-map-blockout, ballistics-refinement, eva-looting,
cigarette-system, crew-system}.md`.

---

## 0. How to read this plan

Three orthogonal lanes run in parallel; **within a lane work is serial**, and the cross-lane
gates below say where one lane must wait on another.

- **Lane A — CLAUDE LIVE-EDITOR (serialized, single connection).** Anything that mutates the
  live editor/level: World-Partition setup, batch actor placement, PCG scatter, lighting-rig
  placement, data-asset creation, atmosphere wiring, retro material stack, HLOD generation,
  in-PIE verification. **Only one of these may run at a time and it must never overlap a
  user hand-edit on the same level** (concurrent mutation is a corruption hazard, not a merge
  nuisance — R-M2). Claude runs a batch pass, then *hands the editor to the user*.
- **Lane B — CODE (C++ authored/built offline, no live editor).** Ballistics body, EVA pawn +
  components, the unified crew/condition owner, the cigarette/effect framework, the weather/time
  controller, the firearm soft-target path, the enemy-AI track. Authored as headers/`.cpp` now;
  **compiles only once VS2022/MSVC lands** (the gating blocker). Independent of Lane A until
  integration.
- **Lane C — USER (sourcing, manual detailing, decisions).** VS2022 + Node.js install, asset
  sourcing/import (meshes, KIT, audio), hand-curation of the route and hero beats, EVA loot
  interiors, combat staging, and the open-decision rulings that unblock B and A.

A system is "done" only when it has compiled and run in PIE. **Until VS2022 lands nothing in
Lane B can compile and nothing in Lane A can be verified** — that install is task −1 for the
whole project.

Milestone spine (from `SIMULATION_ROADMAP §0.4`, extended for the four new directives):

- **M1 — Drivable + shooting tank in fog** (`MILESTONE_1_PLAN` steps 0–14) **+ ONE
  batch-proven vertical-slice region (Krasny)** whose real job is to *measure authoring cost*
  and *playtest whether 10 minutes of fog-driving is fun*. Reserve-only seams for everything later.
- **M2 — The acoustic spine + powertrain + dynamic atmosphere + first credible enemy AI + more
  regions** via the now-proven pipeline.
- **M3 — Consequence & lone-crew life: the unified crew/condition owner (#14), crew
  degradation full-wire, mobility-kill/track-repair, field-mods, more map.**
- **M4 — EVA + cigarette system + foot-scale interiors + infantry AI** (the deliberate Pillar-5
  exception, exactly where the EVA team places it).

---

## 1. TASK −1 — the gating blocker (must precede everything)

Both feasibility reviews rank this the literal first task. No UBT, no editor launch, no compile,
no batch automation until it lands. **Two installs, done together:**

1. **Visual Studio 2022, Game-Dev-C++ workload** — pinned **17.14.x / MSVC 14.44.35207+**,
   Windows SDK 10.0.22621.0, with the banned MSVC ranges per `MILESTONE_1_PLAN §4`
   (`14.39.x`, `14.40.0–14.43.x`, `14.44.0–14.44.35210`, `14.50.0–14.50.35722`). Include
   `Component.Unreal.Ide` + `Component.Unreal.Debugger` + pinned `VC.14.44.17.14`.
2. **Node.js** — required for the `runreal/unreal-mcp` Python remote-exec batch path that the
   entire world-map methodology (`world-map §4.0/§4.2/§4.3`) depends on. Install in the same
   pass so the map pipeline is not blocked a second time.

Until both land: **cap all work at schema/spec drafting** (which is exactly where the six team
docs already are) and do not let any doc claim a system is built. This plan assumes they land
before M1 step 1.

---

## 2. SEAMS TO LOCK NOW (the integration pass's resolutions — cheap at M1, expensive later)

The integration review found the five system docs *largely coherent* but flagged one structural
conflict and several assumed-but-unowned seams. **None block M1**, but the cheap M1 reservations
must be made now to avoid an expensive re-plumb. These are decisions, not code; they cost paper.

### 2.1 BLOCKER resolved — ONE health/condition owner, pawn-agnostic

Three docs modelled the same domain three ways: roadmap **#14** `UKesselCrewStateComponent`
(Morale/Cold/Wound), EVA's persistent crew-state argument, and cigarette's new
`UKesselStatusComponent` "Condition" pool. **Ruling:**

- The crew/condition state is a **single persistent, pawn-agnostic owner** — a
  `UKesselCrewSubsystem` (`UWorldSubsystem`, per-run/world-scoped, mirroring
  `UKesselBallisticsSubsystem`) **or** the `AKesselPlayerController` — **NOT a pawn component**
  (a pawn component on the tank does not survive the EVA possession swap — exactly the failure
  EVA's persistent-owner rule exists to prevent).
- The cigarette team's `UKesselStatusComponent` **IS the implementation of roadmap #14**, not a
  sibling. "Condition" unifies with Morale/Cold/Wound (Condition = the aggregate; designer's call
  whether to keep one float or the three named floats, decided once).
- **All downstream consequence modifiers** (aim sway, blue post-process grade, slower
  reload/traverse, vignette, EVA move-speed, the crew-system capability mask) read from that
  single owner through **one aggregate query surface** so #14 and the cigarette/crew effects
  never double-apply.
- **Sequencing:** the cigarette team's vertical-slice step 1 ("Condition + cold drain") is
  **re-labelled** "build #14's crew-state owner" and lands at **M3 with crew degradation + EVA's
  cold-ramp**, not standalone earlier. Do not manufacture the parallel pool the roadmap warns
  against.
- **Fail-state ruling (folds the feasibility finding):** Condition is a **soft attrition
  modifier** (degrades aim/stamina/morale) that **never directly kills** for M1–M3 — permadeath
  stays **kinetic-only** (a confirmed crew/commander penetration). A telegraphed Condition→0
  collapse fail-state is a *later, optional* design (cigarette OQ-2 / crew-system OD), not a
  second invisible health bar shipped early. This keeps the two-hitbar confusion out.

### 2.2 RNG — one master seed, independent named streams (not a shared object)

EVA loot and cigarette loot both said they roll off "the same `FRandomStream` the ballistics
subsystem owns." Sharing the stream **object** makes ricochet outcomes depend on how much the
player looted (non-reproducible per seed). **Ruling:** keep one master run-seed; derive separate
deterministic streams seeded as `hash(masterSeed, channelId)` — `ballistics`, `loot`,
`weather-director`. Each subsystem owns **its own** `FRandomStream`. Update EVA §4.2 and
cigarette §8 to read "same per-run **SEED**, **independent** stream."

### 2.3 Player-emitted-luminance — one aggregator, one unit scale

Headlamp+cabin (atmosphere), torch/lighter (EVA), muzzle flash, and the cigarette cherry
(`GetCherryLuminosity()`) all feed the night-detection channel the AI consumes, but nobody owns
summing them and the producers live on different pawns. **Ruling:** the atmosphere/crew owner is
the **single luminance aggregator** — one `RegisterEmitter(source, luminosity)` contract (or a
per-frame sum the controller computes from registered light sources on whichever pawn is
possessed) with **one fixed unit scale**. Headlamp/cabin, torch, cherry register into it; AI
reads the **aggregate only**. This resolves cigarette OQ-4 and EVA §5.4 against one definition.

### 2.4 Atmosphere output — split the bus; add `WeatherSeverity`

- MPC_Atmosphere is for **materials/retro only** (it is a `UMaterialParameterCollection`; reading
  scalars back into per-tick C++ gameplay is fragile — R-M2). **Add gameplay-facing C++ accessors**
  on the atmosphere controller: `GetFogConcealment()`, `GetWeatherSeverity()`, `GetAmbientDarkness()`.
  AI detection, EVA cold-exposure, and cigarette/crew cold-drain read these **directly**; the MPC
  becomes a **render-only projection** of the same authoritative values.
- **Add one authoritative `WeatherSeverity` (0..1)** scalar to the atmosphere output (blended from
  the active weather state's extinction/snowfall/wind). EVA exposure and cigarette/crew cold-drain
  both scale off this single value — `WindStrength` was only a proxy.

### 2.5 Map ↔ atmosphere trigger seam (explicit ownership)

Because TimeScale is fixed, a fast 1.5 h run and a slow 2.5 h run reach each map location at a
different in-game hour — without a pin the climax could land in daylight. **Ruling:**

- **Map team places** region-gate trigger volumes / region-origin `TargetPoint`s; on overlap they
  call a public **atmosphere-owned API**: `AKesselWeatherTimeController::RequestClockSnap(hour)`
  and `ForceWeatherBeat(state)`.
- The `UWeatherProfile`'s map-progress pins key off **player west-displacement (−X)**, not
  wall-clock — so "force a building blizzard near the western edge" lands by *position*, not by
  *time elapsed*. Document the seam in both `atmosphere-system.md` and `world-map-blockout.md`.

### 2.6 Shared burning-wreck actor contract (serves three masters)

The burning wreck is an atmosphere light archetype (M1 step-10 death spawn + ruin placement), an
EVA **cold-relief source** ("warm at a fire"), and an `AKesselLootCache` (wreck subtype, ember
light if still burning). **Ruling:** one shared actor contract — the burning-wreck archetype
**optionally implements `IKesselInteractable`** (warmth source feeding the crew cold model) and
carries a **burn-state scalar** that EVA loot/warmth AND atmosphere light both read, so "cooling
over minutes" consistently dims the light *and* removes the warmth/searchability. **Atmosphere
owns** the light + burn-state; **EVA owns** the warmth/loot interaction on the same actor.

### 2.7 M1 gun ammo pool = inventory-compatible store (paper reservation at step 9)

EVA re-arm feeds the gun's stowed supply; its `FKesselInventoryItem` uses
`{EItemClass::MainGunAmmo, FName Id}`; ballistics keys shells by `DT_Shells` row `FName`
(e.g. `8,8cm_PzGr39`). **Ruling (cheap M1 decision):** the gun's ammo pool is a
`UKesselInventoryComponent`-compatible store (or a clean ammo-pool seam), and the **inventory
item `Id` == the `DT_Shells` row FName**. Ballistics + EVA + gun teams sign off the shared key at
M1 step 9. Paper-level now; prevents an expensive re-plumb when EVA lands.

### 2.8 IMC arbitration precedence on the controller

The view system (per-view IMC swap, M1), the EVA dismount swap (foot IMC), and the smoking
component (suppress drive/fire/aim during the ritual) all mutate the single `AddMappingContext`
stack. **Ruling — one arbitration owner/state on `AKesselPlayerController`:** dismount swap is
the **outermost** context selector (tank vs foot); smoking is a **modal suppressor layered on
top** that restores the pre-smoke context for whichever pawn is active; **smoking is disallowed
during the `Climbing_*` transition states**. Document the precedence once in the controller's
ownership notes.

### 2.9 Crew-module geometry (the crew arm's one dependency)

The crew fragment-cone needs each crewman's interior position. Ballistics' committed `FCrewModule`
is `{Name, Health, Criticality}`. **Ruling:** ballistics **restores `FVector LocalPos`
(+`FName Kind`) on `FCrewModule`** (research/04 §10 had it; the committed struct dropped it).
Interim until it lands: `UKesselCrewComponent` carries the five `LocalPos` offsets so M1 is not
blocked. Cheap, additive, ballistics-compatible.

### 2.10 The unscoped, load-bearing system: ENEMY-TANK AI gets an owned track

No team owns it, yet the whole map tension contract ("~5 gated fights at chokepoints") and the
EVA/cigarette vulnerability windows assume a credible thinking opponent. The M1 enemy (step 10) is
a stationary no-nav turret stub — a 22 km map of stationary turrets is a shooting gallery, not the
survival-horror loop. **Ruling — create an owned AI track, staged:**

- **M1:** keep the no-nav stub (fine for proving ballistics + the duel close-out).
- **M2:** first **credible** slice — path-following to authored loom positions, LOS-gated fire,
  hull-down/retreat, and an **acoustic-reveal hook** (a fight is *heard before seen*, Pillar 3),
  tied to the M2 acoustic spine and the now-proven region pipeline. **Do not author 8 regions of
  chokepoint fights until one chokepoint fight is fun against real AI.**
- **M4:** infantry AI for EVA, against scripted/static threats first (exactly EVA's hedge).

---

## 3. INTEGRATED MILESTONE ROADMAP (all arms × M1–M4)

Per milestone: what ships, which team owns it, which lane, what it depends on.

### M1 — Drivable + shooting in fog + ONE instrumented slice

The locked `MILESTONE_1_PLAN` steps 0–14 are the spine. Folded in per team docs and findings:

| # | Deliverable | Team | Lane | Notes / dependency |
|---|---|---|---|---|
| −1 | VS2022 + Node.js install | User | C | gates everything |
| 0 | Placeholder kit (BasicShapes) + IA_/IMC input assets | lead | A | step 0 |
| 1 | Project skeleton, module, build, editor opens | lead | B→A | needs VS2022 |
| 2 | `L_KesselTest` + **atmosphere first** (fog renders, 40–80 m crush) | atmosphere | A | step 2 |
| 3 | Pawn + Phase-1 throwaway movement | lead | B→A | crude steering (do NOT ship) |
| 3b | **Phase-2 tracked neutral-steer pivot SPIKE** (throwaway) | lead | B | de-risk the signature feel early (R-V1); stable core APIs only |
| 4 | FP camera + view component (single mode) | view | B→A | resolve R-V2 (CalcCamera) |
| 5 | Turret/gun aim (rate-limited) | gun | B→A | step 5 |
| 6 | Multi-sight + FOV zoom + UMG reticles | view | B→A | step 6 |
| 7 | Ballistics body + `DT_Shells` + `DA_Armor_*` + duel unit tests | ballistics | B | apply R-B1/B2/B3 + OD-1 ruling **before** step 7; T1–T14 gate |
| 8 | `WeaponTrace` channel + armor-face axis mapping | ballistics | B | reserve `Interaction`+soft-target channel names now (EVA §11.1) |
| 9 | Gun + projectile + hit application; **ammo pool = inventory-compatible (2.7)** | gun/ballistics/EVA | B | step-9 assertion: projectile sets `entryPoint` (non-zero) before spall cone |
| 10 | Enemy **no-nav stub** + end-state closure + **burning-wreck archetype reuse (2.6)** | AI-stub/atmosphere | B→A | cook-off catastrophic branch (#5) folds here |
| 10c | **Crew degradation SKELETON** (5-man roster, capability mask, `ApplyCrewHit` into the cone, Wounded/Killed SM, **2 zero-art degradations: Loader→manual reload, Gunner→degraded lay**, roster strip + loss banner, static presence/effects props) | crew | B→A | uses interim crew `LocalPos` (2.9); commander-pen → step-10 death flow |
| 11 | Muzzle/cabin emissive scatter + blizzard Niagara | atmosphere | A | publishes `PlayerEmittedLuminance`/`AmbientDarkness`/`FogConcealment` + **`WeatherSeverity` (2.4)** |
| 11b | **Day/night slice**: sun+moon cross-fade + skylight + **exposure clamp that keeps night dark** + headlamp/cabin toggles + 2–3-state weather lerp (Overcast↔Snow↔Blizzard) via `AKesselWeatherTimeController` | atmosphere | B→A | subclass existing `AKesselAtmosphereController`; finish scaffold TODOs |
| 12 | PS1 retro layer (`MasterRetro`) | retro | A | R-P1 (non-temporal AA); OD-6 default |
| 13 | (decision-gated) thin audio pass | audio | A | OD-3 |
| 14 | Phase-2 movement swap (productionize the 3b spike) + Linux bring-up | lead | B→A | |
| S | **Krasny vertical-slice region** (instrumented): WP skeleton, batch-place primitive, ONE KIT block generator, ONE PCG scatter graph, one assembled town block + adjoining steppe + one chokepoint | world-map | A (Claude) + C (user curate) | **MEASURE authoring-hours-per-km²**; **playtest "are 10 min of fog-driving tense or dull?"** |

**M1 reserve-only seams (paper, near-zero cost):** DismountPoint `USceneComponent` on the tank
pawn (EVA §11.1); inventory-compatible ammo pool + `EItemClass`/`FKesselInventoryItem` shape
(2.7); `Interaction` + soft-target trace channel **names** alongside `WeaponTrace` (step 8);
winter `EPhysicalSurface` slots (#4 — feeds tracks now, foot later); crew `LocalPos` (2.9);
single-luminance-aggregator contract stub (2.3); `WeatherSeverity` accessor (2.4).

**Explicitly NOT in M1:** acoustic spine, engine/fuel/overheat, full weather director, full
asymmetric night-stealth model, EVA, cigarette/Condition, full crew thermal/morale, field-mods,
all destruction/terrain-deform, the 100 km² world.

### M2 — Acoustic spine + powertrain + dynamic atmosphere + first credible AI + more regions

| Deliverable | Team | Lane | Dependency |
|---|---|---|---|
| Acoustic sensor spine (#8): spatial + occlusion + buttoned/open mix; cue vocabulary | audio | B→A | uses M1 view-state + projectile/impact events |
| Engine / transmission / fuel + overheat (#9) | powertrain | B | needs Phase-2 movement (M1 step 14) |
| **Full dynamic atmosphere**: 6-state `UWeatherProfile` director, map-progress-pinned beats via the **trigger seam (2.5)**, audio approach cues, ruin-light archetype library, scattered beacons | atmosphere | A + B | map-team places triggers; atmosphere owns API |
| **First credible enemy-tank AI** (2.10): path-follow to loom positions, LOS-gated fire, hull-down/retreat, acoustic-reveal hook | AI track | B→A | rides on the acoustic spine; ONE chokepoint fight tuned fun first |
| **More regions** via the now-proven (M1-measured) batch/PCG/KIT pipeline; HLOD silhouettes; data layers `DL_Region/DL_TimeOfDay/DL_Weather` | world-map | A (Claude) + C (user) | gated on the M1 cost measurement; lock minimal branch-route count |
| Suspension/camera-ride polish (#7), recoil/idle-shake (#6), fog-as-spotting tuning (#3) | gun/movement | B | |

### M3 — Consequence & lone-crew life

| Deliverable | Team | Lane | Dependency |
|---|---|---|---|
| **The unified crew/condition owner (#14 / 2.1)** — `UKesselCrewSubsystem` (or controller): Morale/Cold/Wound/Condition, one aggregate query surface; **this IS the cigarette `UKesselStatusComponent`** | crew/cigarette | B | the relabelled "cig step 1"; reads `WeatherSeverity` (2.4) |
| **Crew degradation full-wire**: Driver-down mode-split (needs Phase-2 swap), Radio-down MG + map-degrade, morale/cold continuous drain, full ration economy | crew | B→A | builds on M1 10c skeleton |
| Mobility-kill / thrown track (#10) + **track repair** as a vulnerable timed install state | movement | B | EVA later ferries the spare links into this |
| Ammo cook-off drama + stowage modifier (#11); recoil-stroke (#12); stadiametric ranging (#13) | gun | B | |
| **Field-mod progression** (applique/sandbags/captured optic+MG/track-links) — visible hull accretion, all trade-offs, none restores a dead man | crew | B→A | depends on movement + scavenge economy |
| More map regions + retrace `DL_Damage` reverse-order beats (hand-work) | world-map | A + C | |

### M4 — EVA + cigarette + foot-scale interiors + infantry AI (the Pillar-5 exception)

Ratify the EVA team's own placement (post-M3, M4-class). EVA's *feeling* depends on M2 audio + M3
thermal + the dynamic atmosphere existing first, or it ships hollow. Build order per
`eva-looting §11.2`:

| Slice | Deliverable | Lane | Dependency |
|---|---|---|---|
| 1 | Possession swap: `AKesselFootCharacter` (FP movement/cam) + `UKesselDismountComponent` handshake + foot IMC (precedence per **2.8**) | B | tank persists + stays vulnerable; uses M1 DismountPoint seam |
| 2 | Interaction + loot: `IKesselInteractable` + `UKesselInteractionComponent` + `AKesselLootCache` + `UKesselLootTableAsset` (rolls the **independent `loot` stream, 2.2**) | B | |
| 3 | Inventory + transfer + **re-arm** (ferried main-gun ammo feeds the gun via the **2.7 seam**) | B | |
| 4 | The Luger: `UKesselFirearmComponent` (hitscan vs **soft-target health, kept OUT of the AP ballistics subsystem**) | B | |
| 5 | **Cigarette system (full)**: catalog assets, smoking ritual state machine, cherry light registering into the **luminance aggregator (2.3)**, spliff, tolerance; exposure cold-ramp reading `WeatherSeverity` | cigarette/EVA | B→A | regen tuning lives on the M3 crew owner |
| 6 | Risk closure: unmanned-tank vulnerability, engine-off cold-start, lights-at-night-betray, foot permadeath | B | |
| 7 | **Foot-scale loot-site content** (first-class budgeted line item): reusable interior modules + tank-parking aprons at western-route sites, via the map METHOD | A (Claude) + C (user) | the second-largest content lift; reuse modules like the ruin KIT |
| ‖ | Infantry AI (parallel track, **scripted/static threats first**, 2.10) | B | |

**Cigarette MVP discipline (feasibility finding):** the *first* cigarette prototype is **one
`UCigaretteData` + a Condition float that ticks up over a timer while smoking + input gating** —
**no cherry light, no detection coupling, no tolerance, no spliff.** The symmetric buff/debuff
framework, the cherry-as-night-liability, and the spliff are added **after** the night-detection
channel (M2 atmosphere) and the M3 thermal regen target actually exist — otherwise you tune
against stubs.

---

## 4. EXECUTION SEQUENCE (dependency- and payoff-ordered, partitioned by lane)

The single ordered list the orchestrator runs. **`[A]` = Claude live-editor (serialized),
`[B]` = code, `[C]` = user.** Cross-lane gates are explicit. Payoff rationale: prove the
risky/load-bearing things (compile toolchain, vehicle feel, ballistics calibration, loop fun,
authoring cost) **before** content scales.

1. `[C]` **Install VS2022 (pinned MSVC) + Node.js.** Gate: `vswhere` finds the toolchain; UBT
   runs; `node -v` works. *Nothing below compiles or verifies until this passes.*
2. `[C]` **Rule the blocking open decisions** that gate code: OD-1 (ballistics feel baseline),
   the §2.1 health-owner ruling (accept), EVA-OD-1 (EVA at M4), cigarette OQ-2 fail-state
   (accept "soft modifier, kinetic-only permadeath"), world-map (a) −X=west, (e) Krasny slice.
3. `[A]` **Step 0** — BasicShapes placeholder kit + IA_/IMC input assets (Claude, live editor).
4. `[B]→[A]` **Step 1** — project skeleton/module; build Development Editor; open editor.
5. `[A]` **Step 2** — `L_KesselTest` + atmosphere-first (fog renders, view crushes to 40–80 m).
   *(Atmosphere lead supplies the recipe; Claude places the rig.)*
6. `[B]→[A]` **Step 3** — pawn + Phase-1 throwaway movement (drives in fog).
7. `[B]` **Step 3b SPIKE** — Phase-2 tracked neutral-steer pivot throwaway, to de-risk the
   signature heavy-pivot feel **before** content scales (R-V1). Stable core APIs only.
8. `[B]→[A]` **Steps 4–6** — FP camera (resolve R-V2) → turret/gun aim → multi-sight + zoom +
   UMG reticles. Serial; each is a B-author then an A-verify.
9. `[B]` **Step 7** — apply R-B1/B2/B3 + the OD-1 ruling, land the `ResolveKineticHit` body,
   author `DT_Shells` + `DA_Armor_*`, pass the **T1–T14 duel gate** (no NaN). *(Pure code; runs
   in parallel with any A-lane verify above once VS2022 is in.)*
10. `[B]` **Steps 8–9** — `WeaponTrace` channel + armor-face axis mapping; gun + projectile +
    hit application. **Reserve at step 8:** `Interaction` + soft-target channel names. **At step
    9:** ammo pool = inventory-compatible (§2.7 sign-off); assert projectile sets non-zero
    `entryPoint` before the spall cone.
11. `[B]→[A]` **Step 10 + 10c** — enemy no-nav stub + end-state closure + burning-wreck reuse
    (§2.6); **crew degradation skeleton** (Loader→manual-reload + Gunner→degraded-lay are the two
    zero-art degradations; roster strip + loss banner; static presence/effects props). Uses
    interim crew `LocalPos` (§2.9).
12. `[A]` **Steps 11 + 11b** — muzzle/cabin emissive scatter + blizzard; **day/night slice** +
    exposure clamp + headlamp/cabin toggles + 2–3-state weather lerp via
    `AKesselWeatherTimeController`. Publish the AI channels + `WeatherSeverity` + the
    split-bus gameplay accessors (§2.4) + the luminance-aggregator stub (§2.3).
13. `[A]` **Step 12** — PS1 retro stack (R-P1: non-temporal AA; OD-6 default ≈0.7).
14. `[A]` **Step 13** — (if OD-3 approves) thin placeholder audio pass.
15. `[B]→[A]` **Step 14** — productionize the 3b spike as `UKesselTrackedMovementComponent`;
    remove Phase-1; Linux bring-up.
16. `[A]` **Krasny slice — INSTRUMENTED** (build the batch-place primitive + ONE KIT block
    generator + ONE PCG scatter graph; assemble one town block + adjoining steppe + one
    chokepoint). **Strictly sequential Claude-batch / user-curate handoff** — never concurrent.
17. `[C]` **Measure & playtest the slice** — record actual authoring-hours-per-km² (multiply by
    ~100 before committing the full-map schedule) **and** answer "are 10 minutes of fog-driving
    tense or dull?" If dull, the cheap fix is now: tighten dread cadence / fuel-heat pressure /
    nav stakes, or shorten the median run target below 2.0 h (the map is sized by path length, so
    the time band is a cheap dial). **This gate decides whether M2's full-map track is greenlit.**
18. **— M1 COMPLETE —** then M2 in lane order: `[B]` acoustic spine → `[B]` engine/fuel →
    `[B]→[A]` first credible enemy AI (tune ONE chokepoint fight fun) → `[A]+[C]` full dynamic
    atmosphere director (trigger seam §2.5) → `[A]+[C]` more regions via the proven pipeline.
19. **M3:** `[B]` build the **unified crew/condition owner (#14/§2.1)** = the relabelled cigarette
    step 1 → `[B]→[A]` crew degradation full-wire (driver-split, radio MG/map, morale/cold drain,
    field-mods) → `[B]` mobility-kill/track-repair, cook-off drama, ranging → `[A]+[C]` more map +
    retrace `DL_Damage` beats.
20. **M4:** `[B]` EVA slices 1→6 in order (possession → loot → re-arm → Luger → cigarette full →
    risk closure) ‖ infantry AI (scripted first) → `[A]+[C]` foot-scale loot-site content (budget
    as a first-class line item; reuse interior modules).

---

## 5. THE THREE LANES, EXPLICITLY

### Lane A — CLAUDE LIVE-EDITOR (serialized, one connection)

Order as in §4. Discipline (from R-M2 + the hard constraint): **batch ACTOR spawn/transform +
PCG scatter only** (visually verifiable); do material/MPC/Blueprint-graph work **by hand in-editor
or via the runreal Python fallback**, and verify atmosphere MPC writes manually in PIE (`stat GPU`,
`r.VolumetricFog`) — **never by agent self-report.** After each batch pass, **stop and hand the
editor to the user**; Claude's passes and the user's hand-edits are **never concurrent on the same
level.** A-lane items: step-0 kit, `L_KesselTest`, lighting rig + day/night + weather wiring,
emissive scatter/blizzard, retro stack, atmosphere data-asset creation, Krasny WP skeleton +
batch-place + KIT + PCG + HLOD, M2 region passes, M4 loot-site placement.

### Lane B — CODE (authored now, compiled when VS2022 lands)

Ballistics body (drop-in ready in `ballistics-refinement §4`); crew component + capability mask;
the unified crew/condition owner (#14); the cigarette effect framework + smoking state machine;
the weather/time controller (subclass of the existing atmosphere controller); EVA pawn +
interaction/inventory/firearm/exposure/torch components + possession handshake; the firearm
soft-target path (kept strictly out of the AP subsystem); the enemy-AI track. All headers/`.cpp`
can be drafted to scaffold conventions now; none is "done" until it compiles and runs.

### Lane C — USER (sourcing, detailing, decisions)

VS2022 + Node.js install (task −1); the open-decision rulings (§6); asset sourcing/import
(real tank/enemy meshes, the modular ruin KIT, the Tiger interior hero asset, audio samples,
period cigarette/personal-effect props); hand-curation the batch tools deliberately do **not**
automate — the exact drivable route line, the reverse-order story beats, EVA loot interiors +
tank aprons, combat staging tuned to the 40–80 m heard-then-seen reveal, per-region weather/light
mood; and the **Krasny cost/fun measurement** that gates the full-map commitment.

---

## 6. OPEN DECISIONS FOR THE USER

Blocking (gate code or the next milestone):

- **OD-A (ballistics OD-1) — feel baseline.** Gameplay-hot (157/145/132, Tiger ~5% hot) vs
  canonical (MV 773, 120/110/100). Needed **before step 7**; fixes every duel number.
- **OD-B (§2.1) — health/condition owner.** Ratify: ONE persistent pawn-agnostic owner
  (`UKesselCrewSubsystem`/controller), cigarette `UKesselStatusComponent` IS roadmap #14, built
  at M3. Accept?
- **OD-C (cigarette OQ-2 / crew OD) — Condition fail-state.** Ratify: Condition is a **soft
  attrition modifier, permadeath stays kinetic-only** for M1–M3 (no second hitbar). A telegraphed
  collapse fail-state is a later option. Accept?
- **OD-D (crew-system §0) — supersede Pillar 2.** OK to rewrite P2 "you are the crew of one" →
  "you decay toward the crew of one" (full crew of 5 start, attrition to lone operator)? This
  reframes the whole emotional arc.
- **OD-E (EVA-OD-1) — EVA at M4.** Confirm EVA is a dedicated post-M3 (M4-class) milestone with
  only cheap seams reserved at M1 — not pulled earlier.
- **OD-F (world-map) — (a) −X = west axis; (b) median run 2.0 h hold or shift; (c) branch-route
  count (replay vs authoring cost — recommend minimal for the first full pass); (e) Krasny (R1)
  as the M1 instrumented slice.**
- **OD-G — enemy-AI track ownership (§2.10).** Stand up an owned AI track now (M2 first credible
  slice, M4 infantry). Confirm scope: M2 = one tuned chokepoint fight before any region authoring.

Non-blocking but needed soon:

- **OD-H (OD-2) — real Tiger interior timing** (decoupled post-loop art track vs in-M1).
- **OD-I (OD-3) — audio in M1** (thin placeholder pass vs explicit defer).
- **OD-J (OD-6) — default `MasterRetro`** (proposed ≈0.7).
- **OD-K (OD-7/OD-8) — MCP toolset breadth + confirm Node.js install** for the runreal fallback
  that the map pipeline needs.
- **OD-L (crew-system OQ) — driver-down mode feel** (hard split vs degraded concurrency);
  manual-reload input (single hold vs two-beat); cold/morale drain in M3.
- **OD-M (EVA-OD-2..8) — stamina in/out; carry constraint (weight cap recommended); infantry
  ambition (scripted first recommended); smoking ownership split; re-arm scope (simple top-up
  recommended); cold-start punishment; FP body (hands-only recommended).**
- **OD-N (cigarette OQ-1/OQ-5) — hold-to-smoke vs auto-with-cancel; buttoned-smoking safer-but-
  weaker.**

---

## 7. THE BIG RISKS, RESTATED (so they are not lost in the lanes)

1. **VS2022/Node.js absent** — everything is paper until task −1 lands.
2. **Vehicle feel on Experimental Chaos** — prove the Phase-2 neutral-steer pivot as an early
   throwaway spike (3b); do NOT ship Phase-1 steering; heavy mass likely needs physics sub-stepping.
3. **100 km² AUTHORING cost is unmeasured** (the whiteout makes it cheap to *render*, not to
   *author*) — the Krasny slice is an **instrumentation experiment**; measure hours/km² before
   committing the schedule; cut region/branch count if a block costs more than a few curated hours.
4. **The fog-driving loop's FUN is unvalidated** — pull the playtest forward to the M1 slice
   (10-minute drive), not Phase 4 of the full map.
5. **Enemy AI is load-bearing and unowned** — owned track, one fun chokepoint fight before scaling.
6. **MCP weak read-back (R-M2)** — bound automation to verifiable actor/PCG work; hand-do graph
   work; enforce the strictly-sequential batch/curate handoff as a hard process rule.
7. **Two-hitbar / parallel-survival-model drift** — resolved by the single crew/condition owner
   (§2.1) and the kinetic-only-permadeath ruling (OD-C); hold the line.
</content>
</invoke>
