# Kessel — EXPLOSIONS & VFX SYSTEM

**Team:** Explosions & VFX
**Owns:** every combat and ambient particle/destruction/emissive effect — muzzle flash, impact
sparks (ricochet / non-pen / small-arms), glowing penetration holes, the ammo cook-off + turret-pop
catastrophic-kill sequence, the burning-wreck handoff, track snow/dust kick, and engine exhaust
smoke. The *triggering logic that decides which effect plays* (read off ballistics + movement) and
the *spawn/pooling architecture* that plays it.
**Does NOT own:** the ballistics math (we consume `FHitResolution` + the struck `FArmorSection` /
`FCrewModule`), the fog/light *medium* and the burning-wreck *light archetype* (atmosphere arm owns
those — our fire FX feed their emissive-scatter system, never reinvent it), the crew mortality model
(the cook-off raises `ApplyCrewHit`; crew arm owns the consequence), the PS1 `MasterRetro` post stack
(we author so our effects survive it), and live-editor placement (Claude-main executes the method in
§11 on the single MCP connection).

Pillar anchors:
- **P1 — deep sim, abstracted presentation.** Effects are *driven by* the real sim result
  (`bPen`/`bRicochet`/`spallPower`/`AmmoRack`-crit) but *presented* punchy and low-fi: stylized spark
  bursts, a hard emissive hot-hole, a jack-in-the-box turret on a Chaos impulse — never photoreal
  shatter/FEM. The litmus is "does it read the sim and the feeling?" not "is it realistic?"
- **P3 — sound is the primary sensor.** Every VFX here is the *visible twin* of an audio event; we
  raise the same triggers the audio arm consumes (impact ping, cook-off roar, the report).
- **P4 — the cold, the dark, and the few warm lights.** Our fire/muzzle/ember effects are the
  *content* of P4's warm punctures; they bleed into the atmosphere arm's volumetric fog via
  `VolumetricScatteringIntensity`, they never light the scene with their own ambient.
- **P5 — always inside the steel.** Every effect is authored to read *through an aperture* (optic
  vignette, periscope slit) and to spill onto the interior glass. Exhaust/track FX are seen from
  inside or in the periscope, never a chase-cam beauty shot — there is no exterior camera to play to.
- **P6 — historical grounding.** A penetrated Tiger/T-34 ammo rack *does* throw its turret; cook-off,
  fire jets, the burning hulk are the real death of a WW2 tank, dramatized not museum-ified.

Grounded on: `DESIGN_PILLARS.md`, `ART_BIBLE.md` §3–§4 (palette + scatter), `ARCHITECTURE.md`
§2/§4/§7/§9 (class map, hit flow, enemy wreck-glow seam), `research/04-ballistics-armor.md` §9
(behind-armor / AmmoRack / knockout), `research/06-volumetric-fog-scatter.md` §3–§5 (per-light
scatter, local fog plumes, Niagara), and the live scaffold headers (`KesselProjectile.h`,
`KesselGunComponent.h`, `KesselArmorComponent.h`, `KesselBallisticsTypes.h`, `KesselTankPawn.h`,
`KesselEnemyTank.h`). Integrates with the **Atmosphere**, **Crew**, and **Ballistics** arms; does not
contradict them.

---

## 0. One-paragraph vision

Combat in Kessel is a few seconds of violence punched out of long cold silence, and the VFX exist to
make those seconds *legible* and *terrible*. You loose a round — the muzzle throws a single hot
amber-white bloom that floods the fog and washes the inside of your optic glass (P4). Down-range, the
hit *tells you what the sim decided*: a flat orange spray of sparks that skitters off a slope (ricochet),
a dull grey scuff-and-spall puff that means *not through* (non-pen), or a clean punch that leaves a
**glowing orange hole** cooling slowly in the enemy's flank (pen). And when your shell finds the ammo
rack, the tank *dies the iconic death*: a half-second of white fire-jets gouting from every hatch,
then the **turret leaps off the hull** on a Chaos impulse and crashes into the snow, the hulk settling
into a burning wreck that bleeds red-orange into the whiteout for the rest of the run — the same warm
beacon the atmosphere arm already lights the fog with. Around all this, the ambient layer keeps the
machine alive: a diesel exhaust haze curling from the rear into the fog, and snow spraying off the
tracks as you grind west. Everything is stylized, low-fi, punchy, and fog-friendly — built to read at
a glance through a scratched periscope, and built to survive every position of the `MasterRetro` dial.

---

## 1. Effect catalog — triggers mapped to sim events

The whole system is **event-driven, never per-tick** for combat (consistent with ARCHITECTURE §7
"ballistics is event-driven"); only the two ambient effects (exhaust, track-kick) sample movement
state each frame. Two trigger sources:

**A. Ballistics impact resolution** — `AKesselProjectile::OnProjectileImpact(FHitResult)` already
exists as the seam (`KesselProjectile.h:67`). After it calls the armor component + ballistics
subsystem and gets back an `FHitResolution`, it forwards `{Hit, FHitResolution, FShellData}` to our
combat-FX entry point. The `FHitResolution` fields (`bPen`, `bRicochet`, `spallPower`, `residualFrac`,
`entryPoint`, `dir`, `LOS`, `P0`) **are** the switch.

**B. Movement / pawn state** — speed + surface (from the movement component) and throttle (from the
gun/engine state) drive the two continuous emitters on the owning vehicle.

| # | Effect | Tech | Trigger condition (exact) | Where it spawns |
|---|---|---|---|---|
| **E1** | **Muzzle flash + light pulse** | Niagara (`MuzzleFX`) + pulsed `UPointLightComponent` (`MuzzleLight`) | `UKesselGunComponent::Fire()` succeeds (`CanFire()`==true) | `MuzzlePoint` scene comp on the firing pawn (player + enemy) |
| **E2** | **Ricochet sparks** | Niagara one-shot (GPU burst), warm | `FHitResolution.bRicochet == true` | world @ `Hit.ImpactPoint`, oriented to deflect direction |
| **E3** | **Non-pen spall/scuff** | Niagara one-shot, dull grey + few sparks | `!bPen && !bRicochet` (shell stopped on armor) | world @ `entryPoint`, hemisphere off `Hit.ImpactNormal` |
| **E4** | **Small-arms ping** | Niagara one-shot, tiny | small-arms/MG hit event (reserved; same entry, `EImpactClass::SmallArms`) | world @ impact |
| **E5** | **Glowing penetration hole** | Emissive deferred decal (`UDecalComponent` via MID) + faint point light | `bPen == true` | attached to struck component @ `entryPoint`, on the *entry* face |
| **E6** | **Penetration interior flash** | Niagara cone + brief spall sparks (inside) | `bPen == true` (spall cone, research/04 §9) | `entryPoint` along `dir`, interior |
| **E7** | **Ammo cook-off / blowout** | Niagara fire-jets + secondary-detonation sequence + light pulses | a `bPen` whose spall cone delivers lethal `spallPower` to an `AmmoRack` `FCrewModule` (`Criticality≥1`) | the struck vehicle's hatches/turret ring |
| **E8** | **Turret pop (jack-in-the-box)** | Chaos rigid-body **impulse** on a detachable turret actor | fires at the climax of E7's cook-off sequence | the struck vehicle's `TurretRoot` → detached turret body |
| **E9** | **Burning wreck** | Atmosphere arm's **Burning-wreck archetype** (point light + `LocalFogVolume` plume) + our residual flame Niagara | end of E7/E8 (the kill resolves) | the settled hulk (enemy `WreckGlowLight` seam, `KesselEnemyTank.h:78`) |
| **E10** | **Track snow/dust kick** | Niagara continuous, ribbon/sprite, GPU | movement: `Speed > threshold`; intensity ∝ speed; palette by surface | track contact points (rear of each track) |
| **E11** | **Exhaust smoke** | Niagara continuous, GPU | always on; density/velocity ∝ throttle (idle vs throttle) | rear-deck exhaust attach point |

E4/E6 are reserved-but-scaffolded (small-arms is post-M1; interior spall flash is cheap polish). E1,
E2, E3, E5, E10, E11 are the **M1 cheap-wins slice**; E7+E8+E9 are the **dramatic kill** and ship in
M1 as the headline.

---

## 2. Tech per effect — and why

### E1 — Muzzle flash + light pulse  *(Niagara + pulsed light)*
- **Niagara `MuzzleFX`**: a short-lived (~0.06 s) burst — one flat star-shaped flash sprite + a stubby
  smoke puff + a handful of forward ember sprites. GPU not required (tiny count); CPU emitter is fine,
  burst-spawn then die. Authored at the muzzle, oriented to gun-forward.
- **Pulsed `MuzzleLight`** (`UPointLightComponent`, Movable): color hot amber-white `#FFE8B4`,
  `VolumetricScatteringIntensity` pulsed **6+** for the 0.06 s (atmosphere arm §3 / §8c spec). This is
  what makes the **huge transient fog bloom** on every shot and the involuntary night beacon (atmosphere
  §4: muzzle flash = a 0.06 s position-scream). Driven by a timeline curve in the gun component.
- These two members are the deferred `MuzzleFX` / `MuzzleLight` already reserved on the pawn
  (`KesselTankPawn.h:129-134`). We just author + wire them. Same effect on the enemy (it has a gun
  component) so enemy fire blooms in the fog too.

### E2 / E3 / E4 — Impact sparks  *(Niagara one-shots, the spark MUST read the sim)*
The single most important "deep-sim, abstracted" hook in this doc: **the spark visual is a function of
`FHitResolution`, so the player reads the armor result with their eyes** (and ears, P3).

| Result | Niagara variant | Look (abstracted) |
|---|---|---|
| **Ricochet** (`bRicochet`) | `NS_Impact_Ricochet` | **Bright warm-orange streak-spray** that *skips along the plate* — sparks emitted in a tight cone around the **deflection vector** (reflect `dir` about `Hit.ImpactNormal`), long stretched sprites, a sharp metallic flash. Reads instantly as "it slid off." |
| **Non-pen** (`!bPen && !bRicochet`) | `NS_Impact_NonPen` | **Dull, short, grey-white scuff** — a hemisphere of *short* cold sparks + a grey dust/paint-chip puff off the normal, little/no streak. Reads as "stopped — angle the next one or get closer." |
| **Small-arms ping** (reserved) | `NS_Impact_SmallArms` | **Tiny bright pinprick** + faint dust; near-zero cost; many can co-occur (MG bursts). |

- All three are **GPU sprite bursts** (one emitter, burst spawn, sub-second life) so an MG raking the
  hull (many pings) stays cheap. Orientation comes from the hit: ricochet uses the reflected vector,
  non-pen/ping use `Hit.ImpactNormal`.
- A scalar **`Intensity`** user param scales spark count/size by `FShellData.Caliber_mm` (an 88 ricochet
  is bigger than a 76) and, for ricochet, by how *grazing* the hit was (steeper = more spectacular
  skip). Keeps one system data-driven across all calibers.
- Color stays in the warm-accent band (`#FFE8B4` core → `#FF9B3D` → `#B5300E` falloff, ART_BIBLE §3b)
  so sparks are *the only saturated thing* in the cold frame and survive the palette-snap.

### E5 — Glowing penetration hole  *(emissive deferred decal + MID, cooling over time)*
- On `bPen`, spawn a **`UDecalComponent`** at `entryPoint`, oriented by `Hit.ImpactNormal`, **attached
  to the struck primitive** so it rides the (possibly moving) enemy hull. Material is a
  `UMaterialInstanceDynamic` of `M_PenHole_Emissive`: a small dark punched-through ring with a
  **hot emissive core** (`#FFE8B4`→`#D9531C`→`#B5300E`).
- **Cooling curve:** a per-decal scalar `Heat` (1→0) lerped over ~20–40 s drives the emissive intensity
  and color (white-hot → orange → deep-ember → cold dark scar that *persists* as a hole). A tiny
  short-range `UPointLightComponent` (scatter 0.5–1) rides the first few seconds so the hot hole bleeds
  a faint warm dot into the fog at night, then is destroyed; the decal stays as a cold scar.
- **Decal, not Niagara**, because it must *stick* to the armor and *persist* — a hole is a surface
  mark, not a particle. Capped pool (§8) so a hull peppered with hits doesn't accumulate unbounded
  decals; oldest cold scars recycle first.
- Decal size scales with `residualFrac` (a clean overmatch punch is a bigger, brighter hole than a
  marginal pen). This is the visible readout of *how badly* it went through.

### E6 — Penetration interior flash  *(Niagara cone, polish)*
On `bPen`, a brief inward spall-spark cone from `entryPoint` along `dir` (matching the research/04 §9
fragment cone half-angle), seen if the camera is inside/looking through. Cheap, sells the behind-armor
violence. M1-optional; flagged.

### E7 — Ammo cook-off / blowout  *(the kill sequence)*
A short scripted **timeline**, not a single particle, owned by the per-vehicle FX component:
1. **t=0 (trigger):** the spall cone (research/04 §9) delivers lethal `spallPower` to an `AmmoRack`
   `FCrewModule`. Ballistics/armor raises the *catastrophic-kill* event (see §3). Crew arm's
   `ApplyCrewHit` fires in parallel → "everyone sprayed" (crew doc §2.2 step 3).
2. **t=0–0.5 s — fire jets:** Niagara `NS_CookOff` gouts **white-hot vertical fire-jets** from hatches
   / the turret ring / the engine deck — fast, bright, magnesium-white core `#FFE8B4` → orange. Pulsed
   bright point lights (scatter 6+) flood the fog. Sharp secondary "crack" sub-bursts.
3. **t≈0.4–0.6 s — turret pop (E8):** at the jets' peak, detonation impulse throws the turret.
4. **t=0.6 s+ — settle to wreck (E9):** jets decay into a steady residual flame; the
   Burning-wreck archetype lights up; smoke plume `LocalFogVolume` swells.

All timings are data (`UCookOffSequence` data asset) so the beat is tunable. The whole thing is rare
(a kill) so it can afford to be the most expensive single event in the game.

### E8 — Turret pop  *(Chaos rigid-body impulse — the jack-in-the-box)*
**Primary M1 mechanism (cheap, portable, honest to the no-skeletal-mesh constraint):**
- The vehicle's turret visual is a **separate static mesh** (`TurretMesh` on the enemy,
  `KesselEnemyTank.h:56`; deferred `TurretMesh` on the player). On cook-off we **detach it** from
  `TurretRoot` and either (a) flip the existing `TurretMesh` to `SetSimulatePhysics(true)`, or (b) hide
  it and spawn a one-shot `AKesselTurretDebris` rigid-body actor at the turret transform. Then:
  - `AddImpulse` straight up + a bias along the shell's `dir` (the blast shoves it off-axis),
  - `AddAngularImpulse` (random-ish, seeded) for the tumbling spin,
  - it arcs, falls, and `OnHit` thuds into the snow (audio + a snow-puff Niagara + a small
    `LocalFogVolume` of kicked spindrift).
- Uses only **Chaos rigid-body forces** — already a project dependency (`PhysicsCore`/Chaos via
  ChaosVehicles, ARCHITECTURE §1) — so it is cross-platform (Win+Linux, ARCHITECTURE §12) with no new
  module. This is the "cheapest win that still lands the iconic image."
- **Deferred polish (post-M1):** a **Geometry Collection** fracture (turret ring tears, plates peel)
  via the `GeometryCollectionEngine` plugin for a richer break. Flagged, not M1 — it adds a plugin and
  a heavier render path for one rare event; the rigid-body impulse already delivers the silhouette of a
  turret cartwheeling out of the fog, which is all the abstracted style needs.
- The detached turret leaves the **glowing turret-ring** (a hot emissive decal/ring on the hull top,
  reusing E5's cooling material) — the open wound where the turret was.

### E9 — Burning wreck  *(handoff to atmosphere, not our light)*
We do **not** invent the wreck light. The atmosphere arm already specs the **Burning-wreck archetype**
(point light `#D9531C`→`#B5300E` flicker, `VolumetricScatteringIntensity 2–4`, paired
`ULocalFogVolumeComponent` smoke plume, burns out over minutes) and already wires it to the M1 step-10
enemy-death spawn (atmosphere doc §5, MILESTONE_1_PLAN step-10; the enemy's `WreckGlowLight` member is
the seam, `KesselEnemyTank.h:78`). **Our** contribution is only the *flame Niagara* sitting on top of
their light (licking flames + rising embers as Niagara Light-Renderer motes that scatter into the fog,
research/06 §5) and feeding our cook-off sequence *into* their archetype spawn so combat-kills and
pre-placed ruin-wrecks use **one look, one code path**. The hulk becomes the persistent warm beacon
that bleeds into the whiteout for the rest of the run — exactly P4 / the `1afb2910…` hero reference.

### E10 — Track snow/dust kick  *(Niagara continuous, surface-driven)*
- A GPU Niagara emitter at each track's rear contact zone. **Spawn rate / velocity ∝ vehicle speed**
  (read from the movement component as a user param), gated off below a crawl threshold.
- **Surface switch:** a `ESurfaceType` (snow / bare ground / mud) — from the movement component's
  suspension line-trace `PhysMaterial` (the track layer already line-traces per roadwheel,
  ARCHITECTURE §3) — selects palette + behavior:
  - **Snow:** soft white spray + lingering spindrift puff, kicked up and back, drifts on wind.
  - **Bare ground / mud:** browner, heavier, lower dust + clod sprites.
- Cold-base palette; the snow spray is near-white `#E6EAEC`/`#AEBCC6` (ART_BIBLE §3a) so it reads as
  *part of the cold world*, not a warm accent. Wind direction comes from `MPC_Atmosphere.WindStrength`
  (atmosphere handoff) so track spray blows consistently with the blizzard.

### E11 — Exhaust smoke  *(Niagara continuous, throttle-driven)*
- A GPU Niagara plume at a rear-deck exhaust attach point. **Idle:** thin, slow, curling haze. **Under
  throttle:** denser, faster, darker diesel/petrol belch (a brief over-rich puff on throttle-up reads
  as the heavy engine labouring — the slow-diesel feel ARCHITECTURE §3 wants).
- Driven by a throttle/RPM user param from the movement/engine state (the `FVehicleEngineConfig` RPM
  shape ARCHITECTURE §3 already exposes). Cool desaturated grey-blue base so it curls into the fog as
  part of the medium (it can feed a tiny `LocalFogVolume` for density at high throttle — atmosphere
  §4 tool — but the default is cheap Niagara only).
- Persistent but tiny particle count; it's the one always-on combat-adjacent emitter, so it's GPU and
  budgeted (§8).

---

## 3. Integration with the existing systems

### 3a. Ballistics impact → which effect (the core seam)
```
AKesselProjectile::OnProjectileImpact(Hit)            [KesselProjectile.h:67 — exists]
   │  (after Armor->ResolveArmorFace + Subsystem->ResolveKineticHit, ARCHITECTURE §7)
   ▼
FHitResolution R  +  FShellData  +  FHitResult
   │
   ▼
UKesselCombatFxSubsystem::PlayImpact(R, Shell, Hit)    [NEW — UWorldSubsystem, §6]
   ├─ R.bRicochet            → E2  NS_Impact_Ricochet (reflected dir, Intensity∝caliber·grazing)
   ├─ !R.bPen && !R.bRicochet→ E3  NS_Impact_NonPen   (hemisphere off normal)
   ├─ R.bPen                 → E5  glowing hole decal @ R.entryPoint (Heat=1, size∝residualFrac)
   │                          + E6 interior spall cone (along R.dir)
   └─ (small-arms class)     → E4  ping
```
- The subsystem is **stateless + world-scoped** (mirrors `UKesselBallisticsSubsystem`), so **player and
  enemy fire resolve to the same VFX** (symmetric combat, ARCHITECTURE §10). One call site, no
  duplication.
- It reads *only* the `FHitResolution` contract that already exists in `KesselBallisticsTypes.h` — **no
  change to the ballistics math or structs is required.** We are a pure consumer of its output (same
  stance the crew arm takes).

### 3b. Catastrophic kill → atmosphere emissive light + crew death (the cook-off seam)
The cook-off is the junction of three arms; we are the *trigger router*, not the owner of the
consequences:
```
bPen spall cone hits AmmoRack FCrewModule (Criticality≥1) with lethal spallPower   (research/04 §9.4)
   │
   ├─►  CREW arm:   UKesselCrewComponent::ApplyCrewHit(...) for every crew node the cone catches
   │                → "ammo-rack cookoff sprays everyone" (crew doc §2.2) → run-end if it's the player
   │
   ├─►  US (VFX):   UKesselVehicleFxComponent::TriggerCookOff()  → E7 fire-jets → E8 turret pop
   │
   └─►  ATMOSPHERE: spawn the Burning-wreck archetype (their light + LocalFog plume, atmosphere §5)
                    → warm emissive scatter into the volumetric fog for the rest of the run
```
- **Who decides "catastrophic":** the ballistics/armor behind-armor step (research/04 §9.4) already
  defines AmmoRack-detonation as a knockout. We ask that path to raise **one event** —
  `OnCatastrophicKill(Vehicle, EntryPoint, Dir)` — which our vehicle-FX component and the atmosphere
  spawn both subscribe to. (If that event isn't there yet, our component can detect it by watching the
  armor component's module-knockout broadcast; a one-line addition, same pattern as the crew arm's
  `ApplyCrewHit` insertion.) This is the only new cross-arm signal we need.
- **Player vs enemy:** for an *enemy*, cook-off → turret pop → wreck, fully played. For the *player*, a
  confirmed ammo-rack/commander catastrophic hit routes to the crew arm's **death/abort flow** (crew
  doc §2.2 commander exception); we still play the *interior* fire-flash + the screen going to the
  death sequence (first-person, P5 — the player never sees their own turret fly, they're inside it as
  it dies). The full external turret-pop is an enemy spectacle; the player's version is felt from
  within.

### 3c. Movement / surface → track + exhaust
- `UKesselVehicleFxComponent` reads, each tick, the owning pawn's movement state: **speed** (for E10
  rate + E11 not needed), **throttle/RPM** (E11 density), and **surface type** (E10 palette) from the
  movement component's per-roadwheel suspension trace `PhysMaterial` (ARCHITECTURE §3). No new physics
  work — we sample what the track layer already computes.
- Wind from `MPC_Atmosphere.WindStrength` (atmosphere handoff) advects both track spray and exhaust so
  ambient FX agree with the blizzard.

### 3d. Smoke / sparks scattering into the volumetric fog
- Every warm light we pulse (muzzle, cook-off jets, hot-hole dot, ember motes) is a **Movable**
  `UPointLightComponent` with `VolumetricScatteringIntensity` set per the atmosphere arm's §8c table —
  so they inject into the *existing* froxel solve and read as warm-core/cool-halo smears, never as
  flat sprites. We **do not** add our own fog; we light theirs.
- Smoke/embers that need *density* (cook-off column, big exhaust belch) use the atmosphere arm's
  `ULocalFogVolumeComponent` tool (research/06 §4), parented to the effect, with a dirty warm-grey
  albedo — cheap, integrated into the one fog solve, no Heterogeneous Volumes in M1.
- Niagara **Light-Renderer ember motes** off the wreck/cook-off scatter into the fog via their own
  per-particle scatter intensity (research/06 §5) — drifting warm motes glowing in the volume.
- **Respect the light budget:** atmosphere §7 caps live scatter-contributing Movable lights at ~8–12.
  Our bursty combat lights (muzzle, cook-off) are *transient* (fractions of a second) so they rarely
  collide with that cap, but the **persistent** ones (hot-hole dots, wreck flames) must obey
  `AtmosphereLOD` — distant hot-holes drop their light and keep just the decal; distant wreck flames
  drop shadowing. We coordinate this cap with the atmosphere arm (§8 risk).

### 3e. PS1 / MasterRetro survival
- Effects are authored at modern fidelity and **crunched by the same post stack** (ART_BIBLE §2). Warm
  emissive (sparks, fire, hot-holes) is tagged into the `EmissiveAccent` un-posterized path (ART_BIBLE
  §3c) so glows stay smooth even at 5 levels/channel. Sprites are simple, readable shapes that *gain*
  from pixel-crunch (a PS1 spark burst is on-style). Nothing here depends on high-frequency detail the
  retro dial would destroy — the abstraction is a feature, per P1.

---

## 4. Abstracted-style art direction (punchy + low-fi, not realistic)

The mandate (P1): realism in the *sim*, abstraction in the *pixels*. Concrete rules for this team:

- **Read-at-a-glance silhouettes over detail.** A ricochet is a *shape* (a skipping orange streak), a
  pen is a *hot dot*, a kill is a *turret leaving*. The player should parse the result in one frame
  through fog and a scratched optic. No subtle, realistic, easily-missed cues.
- **Few sprites, bold shapes, hard color.** Flat star-flash for the muzzle; stretched streak sprites
  for skipping sparks; chunky ember quads; a hard emissive ring for the hole. Lean on the limited warm
  palette (ART_BIBLE §3b) — these are the *only* saturated pixels in a cold frame, so restraint makes
  them pop. If everything sparks, nothing reads.
- **The fog does the work.** We don't need volumetric smoke detail — the atmosphere's whiteout + our
  emissive scatter *is* the production value (ART_BIBLE §0/§4). A muzzle bloom is mostly the fog
  lighting up, not the sprite. This is cheaper *and* more on-style.
- **Punchy timing.** Hard attack, fast decay. Muzzle 0.06 s. Sparks sub-second. Cook-off jets a
  brutal half-second before the turret goes. No lingering realistic smolder except the intended slow
  wreck cool-down (which is a *navigation/mood* feature, atmosphere §5).
- **Cold ambient, warm violence.** Track spray and exhaust are *cold* (part of General Winter); only
  combustion (muzzle/pen-heat/fire) is warm. The two-temperature discipline (ART_BIBLE §3) is absolute
  — never a warm dust puff, never a cold spark.
- **No gore, no photoreal debris fields.** The turret pop is a clean iconic *jack-in-the-box*, not a
  shrapnel-physics showcase. Stylized, dramatic, legible.

---

## 5. Performance (Niagara budgets, GPU sim, huge streamed world)

The world is World-Partition streamed; like the atmosphere arm, our cost must be **camera-relative and
O(1) in world size**, and bounded per effect-class.

- **GPU sim where count or persistence is high:** track-kick (E10), exhaust (E11), blizzard-adjacent
  embers, and any many-instance burst (MG pings) are **GPU** emitters. One-shot combat bursts (muzzle,
  single ricochet/non-pen) are small CPU bursts — cheaper to spawn-and-die than to manage on GPU.
- **Pooling, not spawn-churn.** Impact one-shots (E2–E4) and pen decals (E5) come from **pre-warmed
  pools** (Niagara Component Pool / a `UKesselFxPool`); the subsystem acquires/releases rather than
  `SpawnActor` per shot. Combat is bursty (a duel = a handful of shots) so a small pool suffices.
- **Hard caps per class (data-driven `UVfxBudget`):**
  - Glowing-hole decals: cap ~32 live; oldest *cold* scars recycle first (a hull can't accrue
    unbounded decals over a 2 h run).
  - Active cook-off sequences: effectively 1–2 at once (kills are rare); the turret-debris actor is a
    single pooled rigid body, despawned when the wreck is cold or streamed out.
  - Impact bursts: pool of ~16; excess simultaneous pings degrade to the tiny `NS_Impact_SmallArms`.
- **`AtmosphereLOD` / significance** on persistent emissive FX (wreck flames, hot-holes): near = full
  flicker + light + plume; far = decal/sprite only, no light, no plume (folds into atmosphere §7's
  light cap). Niagara **Significance Manager** culls distant ambient emitters (a wreck 400 m off in the
  whiteout is barely visible anyway — the fog hides it for free).
- **Camera-anchored ambient:** track/exhaust live on the player pawn (always near camera); enemy track/
  exhaust emitters cull by distance/fog (you rarely see an enemy's tracks through the whiteout).
- **Chaos turret pop is one rigid body, one rare event** — negligible. Geometry Collection fracture
  (deferred) would be the only heavy destruction path and is explicitly post-M1.
- **Two quality tiers** (mirroring atmosphere §7 / ARCHITECTURE §12): 4090/high vs Linux/min-spec —
  min-spec halves pool caps, drops interior-spall (E6) and ember-motes, decimates track/exhaust spawn
  rate, and forces decal-only hot-holes. Wire into the same "Atmosphere"/"Effects" Scalability group.
- **RTX 4090 headroom (ART_BIBLE §6):** we are not perf-bound on the dev target; the budgets exist to
  protect the *Linux/min-spec* path and to keep the froxel-inject light cost sane, not because the
  4090 struggles.

---

## 6. C++ / Niagara / Chaos architecture

Fits the existing scaffold pattern (owned components on the pawn + a stateless world subsystem),
mirroring `Gun/Armor/View/Aiming` and `UKesselBallisticsSubsystem`. **No new runtime module** — lives
in `Kessel`. Niagara is already a default-on dependency (ARCHITECTURE §1); Chaos rigid-body forces come
transitively via `PhysicsCore`/ChaosVehicles. (Build.cs currently *defers* the Niagara/light includes
on the pawn — when we author E1–E11, the deferred `MuzzleFX`/`ExhaustFX`/`MuzzleLight`/`CabinLight`
members named in `KesselTankPawn.h:129-134` get created and `Niagara` is pulled into the gameplay TUs.)

### 6a. New types

| Type | Base | Role |
|---|---|---|
| `UKesselCombatFxSubsystem` | `UWorldSubsystem` | Stateless router for **world-space impact + kill** FX. `PlayImpact(FHitResolution, FShellData, FHitResult)` switches to E2–E6; `PlayCatastrophic(Vehicle, EntryPoint, Dir)` kicks E7. Owns the impact/decal **pools** + the `UVfxBudget`. One per world; shared by player+enemy (symmetric, §3a). |
| `UKesselVehicleFxComponent` | `UActorComponent` | The **per-vehicle** FX driver. Owns/wires `MuzzleFX`+`MuzzleLight` (E1), `ExhaustFX` (E11), the track emitters (E10); runs the **cook-off timeline** (E7) and the **turret-pop** (E8) on *its* vehicle; spawns the atmosphere Burning-wreck archetype on kill (E9). One on the player pawn, one on `AKesselEnemyTank`. |
| `AKesselTurretDebris` | `AActor` (static mesh, SimulatePhysics) | Pooled rigid-body turret thrown by E8. `Launch(Impulse, AngularImpulse)`; `OnHit` → snow-puff + thud. (Alt: reuse the vehicle's own `TurretMesh` flipped to physics; debris actor is the cleaner pooled path.) |
| `UCookOffSequence` | `UPrimaryDataAsset` | Data for E7: jet timings, secondary-detonation count, turret impulse range, light-pulse curve, wreck handoff delay. Live-tunable. |
| `UImpactFxSet` | `UPrimaryDataAsset` | Maps result-class + caliber band → the Niagara system + `Intensity` scaling for E2–E4, and the `M_PenHole_Emissive` MID params + cooling curve for E5. Keeps "which spark for which result" **data-driven**. |
| `UVfxBudget` | `UPrimaryDataAsset` | Pool sizes + per-class caps + LOD distances + the two quality-tier overrides (§5). |
| `EImpactClass` | `UENUM` | `Ricochet, NonPen, Pen, SmallArms` — the decoded result the subsystem switches on. |
| `ESurfaceType` | `UENUM` | `Snow, BareGround, Mud` — drives E10 palette (from movement trace `PhysMaterial`). |

```cpp
// The single combat-VFX entry the projectile calls after ballistics resolves (no math here).
void UKesselCombatFxSubsystem::PlayImpact(
    const FHitResolution& R, const FShellData& Shell, const FHitResult& Hit)
{
    if (R.bRicochet)            { SpawnImpact(EImpactClass::Ricochet, R, Shell, Hit); return; }
    if (!R.bPen)               { SpawnImpact(EImpactClass::NonPen,    R, Shell, Hit); return; }
    // PEN:
    SpawnPenHole(R, Shell, Hit);                 // E5 emissive decal, Heat=1, size∝residualFrac
    SpawnSpallCone(R, Hit);                       // E6 interior flash (flag-gated in M1)
    // Catastrophic routing is decided by the armor/behind-armor step (research/04 §9.4),
    // which raises OnCatastrophicKill → UKesselVehicleFxComponent::TriggerCookOff (E7+E8+E9).
}
```

### 6b. Niagara assets (authoring list)
`NS_MuzzleFlash`, `NS_Impact_Ricochet`, `NS_Impact_NonPen`, `NS_Impact_SmallArms`, `NS_SpallCone`,
`NS_CookOff` (fire-jets + secondary bursts + ember light-motes), `NS_WreckFlame`, `NS_TrackKick`
(snow/ground variants via surface param), `NS_Exhaust`, `NS_TurretImpactSnow`. All expose user params
the C++ pushes: `Intensity`, `SurfaceType`, `Speed`, `Throttle`, `WindStrength`, `Tint`.

### 6c. Materials / decals
`M_PenHole_Emissive` (MID per hole — `Heat`, `Size`, `Tint`; cooling curve), `M_TurretRingScar` (the
glowing wound where the turret was, reuses the cooling material). Authored into the `EmissiveAccent`
un-posterized path so glows survive `MasterRetro` (ART_BIBLE §3c).

### 6d. Chaos
Turret pop uses **`UPrimitiveComponent::AddImpulse` + `AddAngularImpulse`** on a SimulatePhysics static
mesh (Chaos rigid body) — the same Chaos force API family ARCHITECTURE §3 already uses for the track
layer, so it's cross-platform with no new module. Seeded from the ballistics `FRandomStream` (or a VFX
stream) so a given kill's turret arc is reproducible for debugging. **Geometry Collection fracture is
deferred** (would add `GeometryCollectionEngine` + a heavier path for one rare event).

### 6e. How gameplay code spawns effects (call sites)
- **E1:** `UKesselGunComponent::Fire()` (`KesselGunComponent.h:55`) → after spawning the projectile,
  call `VehicleFx->PlayMuzzle()` (Niagara burst + light-pulse timeline). Already the documented place
  the gun "pulses the muzzle light/FX" (`KesselGunComponent.h:18`).
- **E2–E6:** `AKesselProjectile::OnProjectileImpact` (`KesselProjectile.h:67`) →
  `GetWorld()->GetSubsystem<UKesselCombatFxSubsystem>()->PlayImpact(R, ShellData, Hit)`.
- **E7–E9:** armor/behind-armor catastrophic path → `OnCatastrophicKill` → `VehicleFx->TriggerCookOff()`
  + atmosphere Burning-wreck spawn (reuses the enemy `WreckGlowLight` seam, `KesselEnemyTank.h:78`).
- **E10–E11:** `UKesselVehicleFxComponent::TickComponent` samples movement speed/throttle/surface and
  pushes Niagara user params. The *only* per-tick FX work in the system.

---

## 7. Per-tick / event data flow (where we slot into ARCHITECTURE §7)

```
ON FIRE (event)
  UKesselGunComponent::Fire()
    spawn AKesselProjectile ; VehicleFx->PlayMuzzle()  ─► E1 NS_MuzzleFlash + MuzzleLight pulse(scatter6+)
                                                          └─► warm bloom injected into volumetric fog

ON IMPACT (event)
  AKesselProjectile::OnProjectileImpact(Hit)
    Armor->ResolveArmorFace ; Subsystem->ResolveKineticHit ─► FHitResolution R   (UNCHANGED ballistics)
    CombatFx->PlayImpact(R, Shell, Hit)
      ├ bRicochet → E2 (reflected dir)            ├ !bPen → E3 (off normal)
      └ bPen → E5 hot hole decal (cooling) + E6 interior spall cone
    if R.bPen spall cone kills AmmoRack(crit) ─► OnCatastrophicKill
        ├─► Crew: ApplyCrewHit (spray)               (crew arm owns consequence)
        ├─► VehicleFx->TriggerCookOff()  ─► E7 fire-jets ─► E8 turret pop (Chaos impulse)
        └─► Atmosphere: Burning-wreck archetype ─► E9 wreck light + LocalFog plume into the fog

PER FRAME (ambient only)
  UKesselVehicleFxComponent::TickComponent
    sample movement speed / throttle / surface(PhysMaterial) + MPC_Atmosphere.WindStrength
    push user params ─► E10 NS_TrackKick (snow/ground)  ; E11 NS_Exhaust (idle↔throttle)

RENDER (passive)
  all warm pulses/flames/embers (Movable lights, scatter intensity per atmosphere §8c)
    ─► inject into the froxel grid ─► warm-core/cool-halo smear (atmosphere owns the solve)
  emissive sparks/holes/fire tagged EmissiveAccent ─► survive the PS1 palette-snap (ART_BIBLE §3c)
```

---

## 8. M1 slice vs later

### M1 (ship in Milestone 1 — extends MILESTONE_1_PLAN step 8 & 10)
**Cheap wins first**, then the dramatic kill — exactly the brief's ordering.

- **E1 Muzzle flash + light pulse** — wire the deferred `MuzzleFX`/`MuzzleLight` (pawn + enemy). The
  fog-bloom-on-fire is already a step-10 gate ("firing throws a warm bloom into the cool fog").
- **E2 ricochet / E3 non-pen sparks** — `UKesselCombatFxSubsystem::PlayImpact` switching on
  `FHitResolution`; the visible readout of the duel result (the step-8 gate "pen/ricochet/non-pen
  results matching the duel math" becomes *visible*). Highest sim-legibility payoff, low cost.
- **E5 glowing penetration holes** — emissive decal + cooling MID at `entryPoint` on `bPen`. Cheap,
  iconic, persistent.
- **E10 track snow-kick + E11 exhaust** — the ambient life of the machine; Niagara reading the
  movement state that already exists (scaffold movement in M1, full track layer at step 12).
- **E7 cook-off + E8 turret pop + E9 burning wreck** — the **headline dramatic kill.** Turret pop via
  the **Chaos rigid-body impulse** path (no Geometry Collection, no new module). Wired to the
  AmmoRack-catastrophic event and the atmosphere Burning-wreck spawn already scheduled for step-10.
  For the *enemy* this is the full external spectacle; for the *player* it's the interior fire-flash
  into the crew death/abort flow.
- **Pools + `UVfxBudget` + two quality tiers** — minimal but present, so M1 doesn't leak decals/actors.

Cheap-wins (E1/E2/E3/E5/E10/E11) land alongside step 8–10; the cook-off/turret-pop is the M1 "wow"
and rides the step-10 enemy-death + atmosphere wreck spawn.

### Later (post-M1, honest deferrals)
- **Geometry Collection turret/hull fracture** (peeling plates, torn ring) replacing/augmenting the
  rigid-body impulse — needs `GeometryCollectionEngine`; richer break for the rare kill.
- **E4 small-arms ping** + **E6 interior spall flash** full pass (waits on the MG / hull-gunner and the
  crew interior-prop work).
- **Hull damage decals / scarring accretion** beyond pen-holes (scorch, track-link gouges) — ties to
  the crew arm's "visible hull accretion" field-mods (crew §6).
- **Mud/water/deep-snow surface variants** for track-kick as the world art matures; snow-depth-driven
  spray.
- **Sympathetic detonation chains / progressive fire** (fuel-tank ignition as a distinct slower burn
  vs ammo flash) — richer behind-armor model when research/04's HE/fire path lands.
- **Periscope-glass effects from blast** (a near-miss concussion shudder / frost-crack) — coordinates
  with the atmosphere frozen-lens material.
- **Heterogeneous-volume hero smoke columns** for a major wreck (atmosphere defers these too).

---

## 9. Concise structured summary

### Key decisions
- **One stateless `UKesselCombatFxSubsystem`** routes world-space impact/kill FX off the existing
  `FHitResolution` contract — **zero change to ballistics math/structs**; player+enemy share it
  (symmetric, ARCHITECTURE §10). One `UKesselVehicleFxComponent` per vehicle drives muzzle, exhaust,
  track-kick, and runs the cook-off/turret-pop on its own hull.
- **The spark visual is a pure function of the sim result** — ricochet vs non-pen vs pen each get a
  distinct, abstracted, read-at-a-glance burst, so the player *sees* (and hears, P3) the armor
  decision. This is the headline "deep-sim, abstracted-presentation" hook.
- **Turret pop = Chaos rigid-body impulse** on a detached turret static mesh (`AddImpulse` +
  `AddAngularImpulse`) — cheapest path to the iconic jack-in-the-box, cross-platform, **no new module,
  no Geometry Collection** (that's deferred polish). Honest to the no-skeletal-mesh constraint.
- **Glowing penetration holes = emissive deferred decal** (attached, cooling MID, size∝`residualFrac`)
  — persists as a hot-then-cold scar; capped pool.
- **We light the atmosphere arm's fog, we don't make our own.** Muzzle/cook-off/ember/hot-hole warm
  lights inject into *their* froxel solve via `VolumetricScatteringIntensity`; the **burning wreck is
  their Burning-wreck archetype** (we add only the flame Niagara + feed the cook-off into their spawn),
  so combat-kills and pre-placed ruin-wrecks are one look / one code path.
- **Cook-off is a three-arm junction we route, not own:** AmmoRack-crit pen → crew `ApplyCrewHit`
  (spray/death) + our fire-jets/turret-pop + atmosphere wreck-light. Needs **one new event**
  (`OnCatastrophicKill`) off the behind-armor step.
- **Track-kick + exhaust read movement state** (speed, throttle, surface `PhysMaterial`, wind from
  `MPC_Atmosphere`) — the only per-tick FX; cold-palette ambient vs warm-palette violence (two-temp
  discipline). Everything authored into the `EmissiveAccent` path so it survives `MasterRetro`.

### M1 slice
Cheap wins — **E1 muzzle flash+pulse, E2 ricochet sparks, E3 non-pen spall, E5 glowing pen-holes, E10
track snow-kick, E11 exhaust** — plus the dramatic **E7 cook-off + E8 turret-pop (rigid-body impulse) +
E9 burning-wreck** kill, riding the existing step-8 hit resolution and step-10 enemy-death + atmosphere
wreck spawn. Pools + budget + two quality tiers present. Geometry-Collection fracture, small-arms/
interior-spall full pass, hull scarring, and surface variants deferred.

### Dependencies on other arms
- **Ballistics/Armor (one new signal):** raise `OnCatastrophicKill(Vehicle, EntryPoint, Dir)` on the
  AmmoRack-detonation knockout (research/04 §9.4) — or we watch the armor module-knockout broadcast
  (one-line insert, like crew's `ApplyCrewHit`). Otherwise pure consumer of `FHitResolution` (no math
  change).
- **Atmosphere:** owns the Burning-wreck **light archetype** + `LocalFogVolume` plume + the froxel
  solve our lights inject into; owns the **8–12 live scatter-light cap** we must respect (LOD our
  persistent hot-holes/flames). We feed their wreck spawn from cook-off and read `MPC_Atmosphere`
  (`WindStrength`) for ambient advection.
- **Crew:** cook-off raises their `ApplyCrewHit` (spray-everyone); player catastrophic routes to their
  death/abort flow (we play the interior flash, not an external turret-pop — P5).
- **Movement:** speed / throttle-RPM / per-roadwheel surface `PhysMaterial` for track-kick + exhaust.
- **Audio (P3):** every effect raises the twin audio trigger (report, ping, cook-off roar, turret
  thud) — they own the mix.
- **Retro/Post:** our emissive tagged `EmissiveAccent` so it survives palette-snap (ART_BIBLE §3c).

### Open questions for the user
1. **Player-kill spectacle:** confirm the player *never* sees their own turret fly (P5 — interior
   fire-flash → death sequence only), vs a brief allowed exterior beat for drama? Recommend interior-
   only (pillar-clean).
2. **Turret-pop fidelity in M1:** rigid-body impulse only (recommended), or push for Geometry Collection
   fracture now (adds a plugin + heavier path for one rare event)?
3. **Pen-hole persistence:** holes cool to permanent cold scars that *accumulate* over the run (mood,
   but pool-capped), or fade out entirely after cooling? Recommend persist-but-capped.
4. **Interior spall flash (E6) in M1** or defer? Cheap, but needs the camera to plausibly catch it.
5. **Cook-off → catastrophic trigger:** is the ballistics arm willing to raise `OnCatastrophicKill`, or
   should we detect it ourselves off the armor knockout broadcast?

### Risks
- **Light-budget collision (the real perf risk).** Bursty combat lights are transient and safe, but
  persistent hot-hole dots + multiple wreck flames can crowd the atmosphere arm's 8–12 froxel-inject
  cap in a ruin full of pre-placed wrecks. Mitigation: aggressive `AtmosphereLOD` (decal/sprite-only at
  distance, drop the light), coordinated with atmosphere. **Unverifiable on paper** — needs `stat GPU`/
  `r.VolumetricFog` profiling once the editor is live.
- **Sim-readability of the spark cue.** If ricochet/non-pen/pen sparks look too similar through fog +
  the retro crunch, the "read the sim with your eyes" payoff fails. Mitigation: bold distinct
  silhouettes + the warm/cold/shape grammar + the audio twin (P3) as redundancy. Tune against PIE.
- **Turret-pop tuning.** A Chaos impulse that's too weak reads as a shrug, too strong as comedy. The
  `UCookOffSequence` impulse range needs playtest tuning; seeded stream keeps it reproducible.
- **Cross-arm event timing.** Cook-off fires crew-death + VFX + atmosphere spawn from one signal; if
  they desync (turret pops before the crew banner, wreck light before the fire-jets) the beat reads
  wrong. Mitigation: single `OnCatastrophicKill` event + the `UCookOffSequence` timeline as the one
  clock all three read.
- **Decal/actor leakage over a 1.5–2.5 h run.** Without the pool caps, pen-holes and debris would
  accumulate. Mitigation: `UVfxBudget` caps + recycle-oldest-cold-first from day one (in the M1 slice).
- **Niagara/Chaos module pull-in.** Authoring E1–E11 pulls Niagara (and the light includes) into the
  gameplay TUs the pawn currently defers (`KesselTankPawn.h:129`). Low risk (Niagara is already
  default-on, ARCHITECTURE §1), but the Build.cs dependency + the `.uproject` plugin enable must be
  confirmed when the C++ phase opens (gated on the VS toolchain, like everything else).

---

## 10. Cross-references
- Hit flow + class map + enemy wreck-glow seam: `docs/ARCHITECTURE.md` §2/§4/§7/§9/§10.
- Behind-armor / AmmoRack / knockout that triggers cook-off: `docs/research/04-ballistics-armor.md` §9;
  `docs/teams/ballistics-refinement.md` §1/§2 (`FHitResolution`, AmmoRack modules, `OnCatastrophicKill`
  ask).
- Per-light scatter, local fog plumes, Niagara recipe, light budget: `docs/research/06-volumetric-fog-scatter.md`
  §3–§5/§7; **Burning-wreck archetype + light cap + `MPC_Atmosphere`**: `docs/teams/atmosphere-system.md`
  §5/§7/§8c.
- Cook-off → crew spray/death: `docs/teams/crew-system.md` §2.2 (`ApplyCrewHit`, commander exception).
- Palette (warm-accent / `EmissiveAccent`), scatter language, PS1 survival: `docs/ART_BIBLE.md` §2/§3/§4.
- Pillars: `docs/DESIGN_PILLARS.md` P1/P3/P4/P5/P6.
- Live scaffold seams: `Source/Kessel/Public/{KesselProjectile,KesselGunComponent,KesselArmorComponent,
  KesselBallisticsTypes,KesselTankPawn,KesselEnemyTank}.h`.

## 11. In-editor method (Claude-main executes — we do not touch the live editor)
Ordered method for the single MCP connection, after M1 step-8/10 (hit resolution + enemy death) exist:
1. **Author Niagara systems** (`/Game/FX/`): `NS_MuzzleFlash`, `NS_Impact_Ricochet`, `NS_Impact_NonPen`,
   `NS_Impact_SmallArms`, `NS_SpallCone`, `NS_CookOff`, `NS_WreckFlame`, `NS_TrackKick`, `NS_Exhaust`,
   `NS_TurretImpactSnow` — each with the user params in §6b. GPU for E10/E11/ember motes; CPU bursts for
   one-shots.
2. **Author materials** (`/Game/FX/Mat/`): `M_PenHole_Emissive` (Heat/Size/Tint + cooling curve),
   `M_TurretRingScar`; tag emissive into the `EmissiveAccent` MPC path (ART_BIBLE §3c).
3. **Create data assets**: `DA_CookOff_Default` (`UCookOffSequence`), `DA_ImpactFx` (`UImpactFxSet`),
   `DA_VfxBudget` (`UVfxBudget`) with the two quality-tier overrides.
4. **Wire pawn FX**: create the deferred `MuzzleFX`/`ExhaustFX`/`MuzzleLight` + track emitters +
   `UKesselVehicleFxComponent` on `AKesselTankPawn` and `AKesselEnemyTank`; set scatter intensities per
   atmosphere §8c.
5. **Turret-debris**: make `AKesselTurretDebris` (static-mesh, SimulatePhysics) BP/actor; pool it.
6. **Hand the Burning-wreck archetype** (atmosphere §5) to the cook-off end-step so E9 reuses it.
7. **Scalability**: fold pool caps + LOD distances into the "Atmosphere"/"Effects" Scalability group.
8. **Verify (manual, PIE):** fire → muzzle bloom in fog; shoot a sloped plate → ricochet streak; a thin
   plate square-on → pen-hole that cools; kill an enemy via the ammo rack → fire-jets → turret pops and
   thuds into the snow → wreck bleeds red into the whiteout; drive → track snow-spray + exhaust haze.
   Profile `stat GPU` / `r.VolumetricFog` against the light cap (§5 risk).
