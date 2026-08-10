# 12 — Hardcore Tank-Sim Realism Systems, Translated to Kessel

Status: RESEARCH / DESIGN (no C++ this phase — VS/MSVC absent; do not build or launch). Engine: UE 5.8.0
at `C:/Program Files/Epic Games/UE_5.8`.

Scope: survey the **realism systems** that the reference hardcore tank sims model — **Gunner HEAT PC**
(Radian Simulations), **Steel Beasts Pro** (eSim Games), **War Thunder Simulator/"sim" battles**, and
**Graviteam Tactics / Steel Armor / "WT" Graviteam-lineage** sims — and translate each into Kessel's
specific fiction: **a lone Tiger crew of one breaking out of the Stalingrad pocket in a General-Winter
whiteout**, under the governing pillar **"deep simulation, abstracted presentation."**

For **each** system this doc gives a fixed five-part card:
1. **What it simulates** (the real mechanic the hardcore sims model).
2. **Why it matters for a lone Tiger breaking encirclement** (crew-of-1 + escape + whiteout framing).
3. **Abstracted visual** — a few stylized cues only, never photoreal (the pillar: realism lives in the
   *system*, the picture is denied by the blizzard).
4. **UE 5.8 implementation** — the in-box feature, cited to on-disk truth where confirmable; community/
   unverifiable items tagged **ASSUMPTION**.
5. **Effort tier** (see scale below).

This is a *prioritization* document: it ranks systems by **simulation/feeling payoff per unit effort**
under the pillars (`DESIGN_PILLARS.md`), and slots each against the Milestone-1 plan
(`MILESTONE_1_PLAN.md`) and the existing combat/movement specs (`04-ballistics-armor.md`,
`03-tracked-vehicle.md`, `07-view-modes-controls.md`). It does **not** restate those docs; it extends
them and flags where a system is already partially specced.

> **Pillar litmus applied throughout:** *"Does this deepen the simulation or the feeling?"* Every card
> below earns its place on systems/feeling, **not** on pixels. Where a hardcore sim spends its budget on
> a high-poly spall shower or a photoreal engine deck, Kessel spends it on the *model* and lets the
> whiteout, the optic vignette, and the spatial-audio layer (`AUDIO_ARCHITECTURE.md`, planned) carry the
> presentation with a handful of cues.

---

## 0. Effort-tier scale & M1 fit (used by every card)

| Tier | Meaning | Rough cost | Typical M1 fit |
|---|---|---|---|
| **T1** | Trivial — a few fields + one timer/curve; mostly data | hours–1 day | In M1 reach |
| **T2** | Moderate — one component + state, reuses existing systems | days | M1 if prioritized |
| **T3** | Heavy — new subsystem, physics/audio integration, tuning loop | 1–2 weeks | Post-M1 / stretch |
| **T4** | Research-grade — GPU/solver work or large content lift | weeks+ | Long-tail roadmap |

**Verified on-disk UE 5.8 features referenced below** (Grep/ls confirmed this phase):
- Chaos rigid body + forces: `UPrimitiveComponent::AddForceAtLocation` `Runtime/Engine/Public/Components/PrimitiveComponent.h:1770`; `UWorld::LineTraceSingleByChannel` `Engine/Classes/Engine/World.h:2161` (carried from `ARCHITECTURE.md §14`).
- Vehicles (car-only; tank pivot must be authored): `ChaosVehiclesPlugin` Experimental, off by default (`.../ChaosVehiclesPlugin/ChaosVehiclesPlugin.uplugin:13`); also on disk: `ChaosModularVehicle`, `ChaosMover` (Experimental).
- Destruction: `GeometryCollectionPlugin`, `FieldSystemPlugin`, `ChaosSolverPlugin`, `ChaosNiagara`, `ChaosCaching` (all under `Engine/Plugins/Experimental`).
- FX: `Niagara`, `NiagaraFluids` (`Engine/Plugins/FX`).
- Audio (Runtime, in-box): `Metasound`, `Synthesis`, `AudioModulation`, `AudioMotorSim`, `AudioSynesthesia`, `ResonanceAudio`, `AudioCapture`, `AudioWidgets`, `MicrosoftSpatialAudio` (`Engine/Plugins/Runtime`).
- World subsystem / data assets / timers: `UWorldSubsystem` `Subsystems/WorldSubsystem.h:16`; `UPrimaryDataAsset` `Engine/DataAsset.h:47`; `FTimerManager` (engine, ubiquitous) — carried from `ARCHITECTURE.md §14`.

Anything not on this list (e.g. a marketplace tracked-vehicle plugin, true ray-traced acoustics) is
tagged **ASSUMPTION** at point of use and noted as un-buildable until MSVC lands.

---

## 1. Internal armor scheme + spaced/sloped armor + module/crew knockout

**What it simulates.** The reference sims do not treat a tank as a hit-point bag. Gunner HEAT PC and
Steel Beasts model a **multi-layer armor scheme** (outer plate → air gap / spaced skirt → inner
structure), **slope** (effective LOS = nominal/cos θ) and **overmatch/normalization**, then trace the
penetrator (or shaped-charge jet) **into the interior** where it strikes discrete **modules and crew at
their real positions** — breech, recoil guard, ammo stowage, fuel cells, gunner's seat. Spaced armor and
schürzen defeat HEAT by forcing standoff and pre-detonation; sloped glacis multiplies effective
thickness; a clean penetration sprays a **spall cone** that disables whatever is behind the entry point.

**Why it matters for the lone Tiger.** This is already the spine of Kessel's combat
(`04-ballistics-armor.md`): kinetic AP/APCBC pipeline → ricochet → LOS thickness → behind-armor spall
cone vs `FCrewModule[]`. The crew-of-1 twist makes the *interior* model dramatic rather than
bookkeeping: the enemy needs ≥N of M crew, but **you are the M=1** — a single solid crew-position
penetration is mission-ending (`04 §9.4`). The Tiger's flat slab armor (100 mm near-vertical front,
80 mm sides, **25 mm roof/40 mm late turret roof** — overmatch-vulnerable) makes *angling the hull* and
*not showing your thin side/roof* a live skill, exactly the encirclement problem: enemies are all
around, so the face you present to each threat is a constant decision. Spaced armor matters less for the
Tiger's own scheme but matters as a *threat model* — enemy HEAT/ATR and the side-skirt option are
reserved schema (`04 §2`).

**Abstracted visual (a few cues, not a spall sim).**
- **Hit outcome reads in three stylized states, not particles:** RICOCHET = a hard bright spark streak
  skating off the plate + a sharp *spang* hard-panned in the spatial mix; NON-PEN = dull flat impact
  flash + interior dust puff, no penetration; PEN = a brief inward **emissive spall spark** at the entry
  point plus a red interior flash. One sprite each, all gated by the existing `MasterRetro` palette
  (`ART_BIBLE §3`). No high-poly fragment shower — the fog and the dark interior hide everything else.
- **Internal damage is communicated by instruments and audio, not gore:** a knocked module flips a
  CRT-green panel element to red `#FF2A2A` (ART_BIBLE §5 status LEDs), the relevant system *behaves*
  broken (gun won't lay, engine note dies), and a directional impact thump tells the ear where it came
  from. The "armor scheme" itself is **never seen** — it lives in data.
- **Slope/angle feedback** is a single diegetic cue: at a steep grazing geometry the reticle/optic can
  carry a faint angle indicator, but mostly the player *learns* angling by surviving — the abstraction is
  that the math is invisible and the consequence is loud.

**UE 5.8 implementation.** Already locked: `UKesselBallisticsSubsystem : UWorldSubsystem`
(`WorldSubsystem.h:16`) running the `04 §4` pipeline; `UKesselArmorComponent` resolves face via the
axis-compare cube mapping (M1) or bone/socket (real model); `FVehicleArmorLayout`/`FArmorSection`/
`FCrewModule` data assets. **Spaced/multi-layer** extends cleanly: model a spaced face as an *ordered
list of plates* with an air gap that (a) strips a HEAT jet's penetration by standoff and (b) is a
no-op-but-energy-bleeding pre-plate for kinetic — a `TArray<FArmorSection>` per face with a gap field,
resolved by re-running the pen step per layer. No engine feature needed beyond what `04` already cites;
spall cone is math + one Niagara spark. Per-layer is **schema-reserved** for M1.

**Effort tier.** Core kinetic scheme **T2 (in M1, already planned, steps 7–10)**. Spaced/multi-layer
HEAT defeat and per-poly mantlet **T3 (post-M1)**.

---

## 2. Ammo cook-off & propellant fire

**What it simulates.** When a penetrator or jet reaches stowed ammunition with enough residual energy,
the propellant ignites. Hardcore sims distinguish **propellant deflagration** (a fast, blinding internal
fire / "wet stowage saves you" vs "dry stowage kills you") from a full **high-order detonation** that
blows the turret off. Cook-off can also be **delayed** — a fire starts, then reaches the racks seconds
later — and ammo state depends on *how many rounds remain stowed* (a full rack is a bomb; a near-empty
one often just burns). This is the single most lethal post-penetration event in WW2 armor and the reason
crews stack rounds in fluid/under the turret floor.

**Why it matters for the lone Tiger.** For a crew-of-1 escape, cook-off is the **catastrophic-kill**
branch already named in `04 §9.4` (`AmmoRack` detonation = instant mission end). It also creates a
*risk-management* layer unique to a single operator who can't delegate: every round you have **not yet
fired** is stowed danger, so the encirclement run has an implicit "spend ammo / reduce your own bomb"
tension. The Tiger stows 88 mm rounds in the **sponsons (side)** — the same large, side-facing target
the spall/side-shot cards lean on — so a flank hit from the all-around threat ring is doubly punishing:
it's both the easiest plate to defeat (80→thin) *and* where the propellant lives.

**Abstracted visual.**
- **Two-stage emissive event, reusing the atmosphere system for free.** A cook-off is a *light*, not a
  mesh sim: an intense `#FFE8B4`→`#D9531C` interior bloom that **spills through the apertures** onto the
  cabin (ART_BIBLE §4d), a screen-filling warm flash, then — on a high-order — fade-to-black + the
  death/abort sequence (`MILESTONE_1_PLAN` step 10). On the *enemy*, a knocked tank that cooks off
  spawns the **burning-wreck emissive light** already specced (step 10), bleeding red-orange into the
  snow-fog: the death ties straight into the look with zero new art.
- **Delayed cook-off as audio dread:** a penetrating fire-start can play as a rising internal roar +
  smoke haze for a beat before the rack goes, giving the player a horrible half-second — pure "feeling"
  payoff, almost free (a timer + a light ramp).
- No turret-toss ragdoll, no photoreal fireball — the whiteout and the first-person-only rule mean you
  never see your own turret leave; you see white, then black.

**UE 5.8 implementation.** Pure gameplay + the existing emissive/atmosphere stack: a `bCritical` module
hit in the spall cone (`FCrewModule.Critical`, `04 §10`) rolls cook-off; `FTimerManager` schedules the
delayed-detonation beat; the flash is a pulsed `UPointLightComponent` with high `VolumetricScatteringIntensity`
(`LightComponentBase.h:120`, per `06`/ARCHITECTURE §8) + a Niagara smoke `ULocalFogVolumeComponent` plume
(already in the atmosphere plan). Enemy wreck-fire reuses step-10's light. No new engine system.

**Effort tier.** **T1–T2.** The catastrophic branch is essentially in M1 already (step 10); the
delayed-cook-off drama and the "stowed-rounds-remaining" modifier are a few fields + a timer — very high
feeling-per-effort.

---

## 3. Mobility kill / track damage / throwing a track

**What it simulates.** A hit to the running gear — track, drive sprocket, idler, road wheels, final
drive — that doesn't kill the tank but **immobilizes** it ("M-kill"). Sims model **throwing a track**
(the track snaps/derails and the tank slews/locks to one side), **single-track drive** (one track dead →
you can only pivot toward the dead side, dragging), and **field-repair** of a thrown track (Steel Beasts
/ Graviteam let crews remount over time). In snow/mud the track is also the *traction* surface, so track
damage and terrain interact.

**Why it matters for the lone Tiger.** For an **escape** game this is arguably *the* most thematically
loaded failure state — not death, but **stranded inside the Kessel**, which for a survival-horror framing
is worse. It's already half-specced: `04 §9.4` makes Engine+Transmission death a "mobility kill — not a
full kill but mission-relevant for an escape game," and `Tracks` is a listed module (`04 §9.1`). A thrown
track on a crew-of-1 means **you** must (diegetically) dismount to repair under fire in a blizzard — a
natural high-tension set-piece. Asymmetric single-track drive also couples directly to the authored
neutral-steer model (`03`): lose the left track and your `vL` clamps to ~0, so the custom track layer
*already has the hook* to express it (force one side's tractive force to zero).

**Abstracted visual.**
- **Felt through the controls, shown with one cue.** A thrown track = the tank **yaws hard toward the
  dead side and won't drive straight**; the player feels it in the movement model, not in a track-link
  mesh sim. A single stylized cue (a track-warning element going red on the driver panel + a grinding/
  snapping sound hard-panned to the damaged side) carries it. Optional: the side periscope shows the
  track "gone slack," but in M1 the placeholder running gear means this is an instrument + audio + handling
  story.
- **Repair = a timed, exposed, diegetic action**, not an animation set: head-out / hatch state, a
  progress beat, vulnerability while it runs — abstraction is that the *repair* is a state machine and a
  sound, the steel and snow do the rest.

**UE 5.8 implementation.** Lives in the **custom tracked movement component** (`03 §3`,
`UKesselTrackedMovementComponent`): a per-side `bTrackDisabled` / `TrackHealth` that zeroes or clamps
that side's tractive force and raises lateral drag asymmetrically → authentic drag-to-one-side, all on
`AddForceAtLocation` (`PrimitiveComponent.h:1770`). The hit that causes it is the existing armor→module
path (`Tracks` module). Repair = `FTimerManager` + the view-mode state machine (`07`). No new engine
dependency — and notably this is **buildable on the stable Chaos rigid-body core**, not the experimental
car solver (`03 §6` risk note).

**Effort tier.** **T2.** The movement hook exists in the Phase-2 plan; wiring track-module-hit →
per-side disable + the repair state is days. High payoff: it's the signature "escape" failure.

---

## 4. Engine / transmission / fuel + overheating

**What it simulates.** A real powertrain model: an **engine torque/RPM curve** feeding a
**transmission** (gear ratios, shift time, clutch), **fuel** as both a consumable and a *fire hazard*,
and **thermal state** — overheating from sustained load, cold-start difficulty, and damage that drops
power or stalls. Gunner HEAT PC / Steel Beasts model engine and transmission as **separately killable
modules** (lose the engine → coast to a stop; lose the transmission → engine runs but no drive), fuel-cell
hits as fire sources, and (Graviteam especially) **cold-weather start/bog** behavior.

**Why it matters for the lone Tiger.** General Winter is the antagonist: a **Maybach HL230 in a
Stalingrad blizzard** is a cold-start, overheat-then-freeze, fuel-anxiety machine, and the escape is
literally "keep this thing running long enough to get out." The movement doc already wants the engine
*feel* (`03 §4`: flat fat diesel/petrol torque curve, slow rev-up/down, audible idle for the cabin) and
ties engine note to audio. Modeling **fuel as finite** turns the run into a resource-pressure arc; **engine
overheat** punishes flooring it through deep snow (couples to the suspension/terrain card); and the
**engine/transmission split kill** is the other half of the mobility-kill story (card 3). For a crew-of-1,
the engine is also your **primary sensor companion** — its note *is* the dashboard you hear, so its
health is conveyed almost entirely through sound (Pillar 3).

**Abstracted visual.**
- **Almost entirely an audio + instrument system — ideal for the pillar.** Engine state = the **engine
  loop's pitch/timbre** (idle rumble → strained high-load → stutter/stall), a temperature needle or a
  CRT bar climbing into red, and a fuel gauge. No engine-deck heat-haze, no photoreal exhaust required —
  exhaust is one Niagara `ExhaustFX` plume (already in the pawn plan, ARCHITECTURE §2) that thickens with
  load and coughs on a cold start.
- **Overheat cue:** the temp bar reddens, the engine note roughens, power visibly sags in the handling;
  optional steam/exhaust thickening. Damage = the note dies and a red engine LED.
- This is the cleanest example of "abstracted presentation": a deep powertrain model presented as **two
  gauges, one engine sound, one smoke plume**.

**UE 5.8 implementation.** Engine/transmission **feel** mirrors the Chaos `FVehicleEngineConfig` /
`FVehicleTransmissionConfig` *shape* (`ChaosWheeledVehicleMovementComponent.h:223-258,312`) inside the
custom layer (`03 §3-4`), as a data-tunable struct — *not* by running the car solver. Thermal/fuel are
plain `float` state on the movement component updated per tick, gating max tractive force. Crucially,
**`AudioMotorSim`** (in-box Runtime plugin, confirmed on disk: `Engine/Plugins/Runtime/AudioMotorSim`)
is purpose-built to drive **RPM-/load-based engine audio** and pairs with MetaSound — a strong, in-box
fit for the engine-note-as-sensor goal (ASSUMPTION on exact 5.8 API surface; verify when MSVC lands).
Engine/transmission/fuel hits reuse the armor→module path. No new heavy dependency.

**Effort tier.** Feel + audio engine note **T2** (the movement layer is already planned; `AudioMotorSim`
is in-box). Thermal/fuel-as-resource + cold-start **T2** (mostly float state + curves). Full
shift/clutch fidelity is **T3** but low priority for a single-gear-feel heavy tank (`03 §4`).

---

## 5. Gun handling — traverse/elevation rates, gun depression, turret ring, breech & reload

**What it simulates.** The *mechanics of laying the gun*: finite **turret traverse rate** (hand-cranked
vs powered; the Tiger's hydraulic traverse is slow and engine-RPM-dependent), **elevation/depression
limits** set by the mantlet and **by terrain** (you cannot depress into your own hull; a hull-down or
nose-up attitude changes your usable arc), the **turret ring** (traverse can **jam** if the ring is
damaged or the tank is tilted), and the **breech/loading** cycle: open breech, ram round, close, fire,
extract, with a **reload time** that depends on round type, loader fatigue, and stowage location.
Hardcore sims model traverse as a *rate you fight*, gun-laying as a chase toward the reticle, and reload
as a real timed vulnerability.

**Why it matters for the lone Tiger.** This is **Pillar 2 (you are the crew of one) made literal.** In a
real Tiger the *commander*, *gunner*, and *loader* are three people; here the slow hydraulic traverse,
the elevation limits, and the reload are all **your** bottleneck, and you cannot drive, lay, and reload
at once. The escape is a constant workload-triage problem: a threat appears off your gun's current
bearing and the **slow traverse** (the doc already specs ~6–10°/s, `07 §5` / ARCHITECTURE §5) means you
must *choose* to turn the hull (using the movement model) to help the turret around — coupling guns to
mobility. Reload time is the moment of maximum dread for a solo crew: you've fired, the breech is empty,
and an enemy in the all-around ring is still there. **Gun depression limited by terrain** matters for an
escape over rough snow ridges: cresting a rise exposes your thin roof while your gun can't depress onto a
close threat. Turret-ring jam (from a mantlet/ring hit) is a partial mobility-kill of the *gun* —
terrifying for a lone operator.

**Abstracted visual.**
- **The whole system is already first-person-diegetic and needs almost no new visual.** Traverse =
  the *world slews slowly* past the optic at the capped rate (you feel the weight); elevation hits its
  **−8°…+15° limits** (ARCHITECTURE §5) and the reticle simply stops. A single **traverse-rate cue**
  (the slow drift) plus the reload state sell it.
- **Reload** = a CRT-green/amber **reload indicator** (a bar or a "LOADING/READY" element, ART_BIBLE §5
  style), an audible breech *clack-ram-clack* in the cabin mix, and the gun being inert until READY. The
  loader is *you*, so the sound is the only "loader" the game has — perfect Pillar-3 economy.
- **Turret-ring jam** = traverse rate drops to ~0 with a grinding sound + a red panel element; **terrain
  depression limit** = the gun visibly refuses to depress further with the hull attitude. No new meshes.

**UE 5.8 implementation.** Almost entirely the **already-specced** `UKesselTurretAimComponent` (`07 §5`,
ARCHITECTURE §5): mouse delta → desired yaw/pitch, actual lay **chases at a capped rate**
(`FMath::FixedTurn` / `FInterpConstantTo`), hull-independent, with the elevation clamp. Extensions are
data/state, not engine features: make traverse rate a function of engine-RPM/power state (couples to
card 4) and turret-ring health; add **terrain-relative depression** by clamping gun pitch against hull
roll/pitch each tick. Reload is `UKesselGunComponent` + `FTimerManager` (ARCHITECTURE §4). Audio via
MetaSound one-shots in the cabin bus. No new dependency.

**Effort tier.** Traverse/elevation/reload **T1–T2 — already in the M1 critical path (steps 5–6, 9).**
RPM-coupled traverse, ring-jam, and terrain-limited depression are **T2** polish that massively
sharpens the crew-of-1 feel for little cost.

---

## 6. Fire control — ranging, lead, zeroing, parallax, optic shake

**What it simulates.** Hitting a tank at range is a *procedure*: **estimate range** (stadiametric ranging
off the reticle's mil/stadia marks against a known target size, or a coincidence/rangefinder), **set the
sight** (dial elevation for the round's drop, "zero" the gun), apply **lead** for a moving target,
account for **parallax** (the gun and the sight are not coaxial, so a near-vs-far aim point differs), and
fight **optic shake** — the sight picture jitters from the running engine, settles when stopped, and
**recoils hard** on firing then re-settles. Gunner HEAT PC and Steel Beasts model the WW2 path
faithfully: no laser rangefinder, manual range estimation via the reticle scale, and a real ballistic
drop you must compensate. War Thunder sim-battles strip the lead indicator and force the same manual
solution.

**Why it matters for the lone Tiger.** This is the **skill ceiling of the gunnery loop** and a perfect
fit for the whiteout: with view distance crushed to **40–80 m** (ART_BIBLE §4a, MILESTONE_1_PLAN), most
engagements are *short*, so drop is small but **target ID and ranging are dominated by fog**, not by
optics. The 8.8 cm KwK 36's flat trajectory (`04`) means at fog ranges you mostly **point and shoot** —
which is *good*, because the lone crew can't run a full fire-control procedure under pressure. But the
**manual range estimation** via the gunner reticle's stadia ticks (already an asset: `WBP_GunnerReticle`
with "a few range ticks," `07 §6` / ART_BIBLE §7b) gives an expert player a real edge at the fog's outer
edge where a silhouette is barely there. **Optic shake** is huge for *feeling*: the engine-idle jitter
makes you want to **halt to shoot** (coupling to the engine/mobility cards — stopping is safer for the
shot but exposes you longer), and the **recoil reticle kick** is the satisfying punctuation of every
88 mm shot, paired with the muzzle-bloom-into-fog (ART_BIBLE §4b).

**Abstracted visual.**
- **All of it rides the existing optic + reticle, no new geometry.** Ranging = **CRT-green stadia ticks**
  on the reticle (`#33FF66`, ART_BIBLE §5/§7b) the player visually matches to a target's width; an
  optional amber range readout (`#FFB020`) appears only if we add a diegetic rangefinder later.
- **Optic shake** = camera/reticle micro-jitter driven by engine load + a sharp **recoil punch** on fire
  that decays over ~0.3–0.5 s — a curve on the camera, not an animation. Settles to dead-steady when the
  engine is idling low / halted.
- **Zeroing/elevation** = a simple sight-elevation offset the player can step (a few clicks), shown as a
  reticle aim-point shift — or, for maximum abstraction, folded into "aim a touch high past X meters" and
  left to the player's eye. **Parallax** at fog ranges is sub-pixel and can be a **modeled-but-tiny**
  offset (correct, invisible) — exactly the "deep sim, abstracted" sweet spot: the math is right, the
  picture doesn't fuss about it.
- Muzzle flash blooms hugest here into the fog (ART_BIBLE §7b) — the one big visual moment, already
  planned (step 11).

**UE 5.8 implementation.** Optic = the existing FOV-stepping gunner camera (`07`, `CameraComponent.h:46`);
reticle = the UMG `WBP_GunnerReticle`. Ranging is **UMG + player skill**, zero engine cost. Optic shake
= drive the camera's local rotation/`PostProcessSettings` from an engine-load scalar + a fired-recoil
impulse curve (per-tick, trivial). Ballistic drop is the `04 §3` `v(r)` model already specced; the
*aiming* arc is the cosmetic projectile path. Parallax = a small offset between `MuzzlePoint` and
`GunnerEye` (both are child `USceneComponent`s, MILESTONE_1_PLAN locked) — it falls out of geometry for
free. No new engine feature.

**Effort tier.** Reticle stadia + recoil/idle shake **T1–T2 (M1-adjacent; reticle is step 6, shake is a
small add).** Manual elevation/zero steps and a diegetic rangefinder **T2 (post-M1)**. Full coincidence
rangefinder UI **T3** and low priority given fog ranges.

---

## 7. Spotting / identification in whiteout

**What it simulates.** *Detection and recognition* as distinct, costly steps: hardcore sims model **line
of sight**, **acquisition difficulty** (a hull-down or stationary target is far harder to spot than a
moving one), **friend/foe identification** (Steel Beasts and Graviteam punish shooting before you ID),
and **what the optic can resolve** at range and in weather. War Thunder sim-battles remove the spotting
"markers" entirely — you must find tanks with your eyes. Smoke, dust, foliage, and weather all degrade
acquisition.

**Why it matters for the lone Tiger.** This is **Pillar 3 (sound is the primary sensor)** and the entire
mood of the game. The blizzard is a **hard acquisition limiter by design**: enemies "loom out of nothing
as black silhouettes" at 40–80 m (ART_BIBLE §0/§4a). For a crew-of-1 with **no allies ever on screen**
(Pillar 5, and you never see friendlies anyway), spotting is a lonely, dread-soaked scan — you can't ask
a commander to watch your flanks, so **threats arrive from the all-around ring mostly as sound first**,
then as a shape. Identification is dramatized rather than punished (you're surrounded; nearly everything
is hostile), which removes the friendly-fire nag and keeps the focus on *"where is it and how close."*
The interplay — **hear it left → traverse the slow turret → strain to resolve a silhouette in the fog →
fire before it fires** — is the core loop, and the whiteout makes the abstraction (crude geometry) read
as atmosphere, not as a budget limit (Pillar 1).

**Abstracted visual.**
- **The fog *is* the spotting system.** Volumetric fog crushing draw distance (the locked atmosphere
  recipe, `06`/ARCHITECTURE §8) means acquisition is governed by **distance-fade silhouettes**: far
  enemies are pure value-on-fog with no detail (ART_BIBLE §4a), resolving only as they close. No
  detection meter, no outline highlight, no nameplate — the eye does the work.
- **Audio-first acquisition** (the headline of the user's audio vision): an engine/track sound *left of
  the tank* arrives in the left ear before the shape does; a shell impact bangs from its true bearing.
  Buttoned-up muffles and de-directionalizes it (you're half-deaf in the steel); head-out sharpens it at
  the cost of exposure. This is the spotting model's primary channel — see `AUDIO_ARCHITECTURE.md`
  (planned) and the audio research track.
- **Optic differences** as cues: the gunner optic resolves a distant silhouette slightly better (narrow
  FOV) but blinds you to the flanks; the commander/head-out view trades resolution for a wider scan and
  more exposure (ART_BIBLE §7). The *choice of aperture* is the spotting mechanic.

**UE 5.8 implementation.** Visual side: the in-box **volumetric fog** (`ExponentialHeightFogComponent.h:136`,
locked in `06`) plus distance-driven silhouette fade — no new system; LOS uses the same
`LineTraceSingleByChannel` (`World.h:2161`) the enemy stub already needs (ARCHITECTURE §9). Audio side:
spatialized MetaSound sources with occlusion/attenuation; snow muffling and buttoned/head-out mixing tied
to the **view-mode state machine** (`UKesselViewComponent`, `07`) — the in-box `Metasound` + `AudioModulation`
+ (for HRTF/spatial) `ResonanceAudio`/`MicrosoftSpatialAudio` stack (all confirmed on disk) covers it.
True **ray-traced acoustics** is the user's stretch wish and is **ASSUMPTION/T4** — no in-box ray-traced
audio engine ships in 5.8; approximate with occlusion traces + reverb/attenuation zones. (Full treatment
belongs to the audio doc; this card only notes the spotting↔audio coupling.)

**Effort tier.** Fog-as-spotting + silhouette fade **T1 (largely free — it's the atmosphere we're already
building).** Audio-first acquisition (spatial + muffle + view-state mixing) **T2–T3** and is the single
highest feeling-per-effort system in the game — but lives in the audio track, not here.

---

## 8. Suspension over rough / snow terrain

**What it simulates.** Per-roadwheel **suspension** (spring + damper) following ground contours, the hull
**pitching and rolling** over obstacles, **traction loss** on slopes/ice/mud, **bogging** in deep snow,
and the way terrain attitude feeds back into gunnery (hull pitch changes your gun arc — card 5) and into
ride-induced **optic shake** (card 6). Graviteam in particular models soft-soil/snow mobility (sinkage,
reduced speed) seriously; all the sims model suspension as the thing that makes a tank feel like *tonnage
on torsion bars* rather than a hovercraft.

**Why it matters for the lone Tiger.** "Heavy, sluggish, high momentum" is the explicit movement target
(`03`), and **snow is the medium of the entire game.** The Tiger's stiff interleaved roadwheels, its
57-tonne mass, and **low snow grip** (`03 §4`: `muLong` 0.5–0.7, "low enough to lose traction on ice
slopes") make the escape a *terrain-reading* problem: pick a line that doesn't bog you, don't crest a
ridge that exposes your roof, don't slide a track on ice into a firing position you didn't want. The
suspension model is also what physically generates the **idle/drive jitter** that the optic-shake card
turns into a "halt to shoot" decision, and the hull pitch that limits gun depression — so this one system
*couples to gunnery, fire control, and mobility-kill at once*, which is exactly the kind of deep-systems
interlock the pillars reward.

**Abstracted visual.**
- **Felt, barely seen.** The hero presentation is the **camera ride**: the first-person view pitches,
  rolls, and jitters with the hull, snow streaks across the optic (camera-anchored Niagara blizzard,
  already planned step 11), and spindrift kicks up — you *feel* the terrain through the eyepiece without
  any high-detail track-link or terrain-deform mesh. The locked atmosphere hides terrain detail anyway.
- **Snow interaction = a couple of cues:** spindrift/track-spray Niagara when moving, a deeper engine
  strain + speed sag when bogging (couples to card 4 audio), and that's it. **No terrain deformation
  mesh** in M1 — sinkage can be a *handling* effect (max speed scales with snow depth) with a particle
  cue, not a renderer feature. ASSUMPTION: terrain-deform (Chaos/Niagara) is a T4 long-tail nicety the
  whiteout makes unnecessary.

**UE 5.8 implementation.** This is the **custom tracked movement component's core** (`03 §3`): per-
roadwheel `LineTraceSingleByChannel` (`World.h:2161`) → spring `k·x` + damper `c·v` along the contact
normal → `AddForceAtLocation` (`PrimitiveComponent.h:1770`) on the `HullMesh` rigid body, with high
damping (the heavy tank "does not bounce," `03 §4`) and physics **sub-stepping** for stable forces on the
57 t mass (`03 §6` risk). Snow grip = the authored `muLong`/`muLat` per-side friction. Camera ride is
free (the cameras are children of the hull). Built entirely on the **stable Chaos rigid-body core**, not
the experimental car solver — the deliberate `03` choice. Snow/ice can vary friction via
`UPhysicalMaterial` per surface. No new engine feature; terrain deformation explicitly **out of scope**.

**Effort tier.** Suspension + snow-grip + camera ride **T3 — it's Phase-2 movement (MILESTONE_1_PLAN step
14), the biggest movement lift but already on the roadmap.** Per-surface friction (ice patches) **T2** on
top. Terrain deformation **T4 / descoped**.

---

## 9. Thermal / morale of the lone crew

**What it simulates.** The *human* layer hardcore sims gesture at: **crew morale/suppression** (Steel
Beasts and Graviteam model crews that button up, panic, get suppressed by near-misses, slow their reload
when shaken, or bail), **crew fatigue/wounding** degrading performance, and — less commonly modeled but
core to *this* setting — **cold/thermal stress**: numb hands, frosted optics, the physiological cost of a
Russian-winter cabin. In multi-crew sims this is distributed across roles; the loss of a crewman degrades
the specific job he did.

**Why it matters for the lone Tiger.** Kessel collapses the entire crew into **one person under
sustained pressure** (Pillar 2: "workload and isolation under pressure"), so morale/thermal isn't a squad
stat — it's the **player-character's own state**, and it's the most direct lever on the survival-horror
*feeling* the game is built around. Concretely: a near-miss or a non-pen hit can **shake the operator**
(brief screen/optic shake, slower hands → a temporarily longer reload, a moment of de-directionalized
hearing — i.e. the spotting/audio sensor degrades when you're rattled), and **cold** can frost the
periscope glass, stiffen traverse, and make the warm cabin light / engine heat *matter* (a reason to keep
the engine running beyond mobility). This makes the **buttoned-up vs head-out** choice carry a
psychological weight, not just a sightline one — head-out is colder and more exposed but sees and hears
more; buttoned is warmer and safer but half-blind and half-deaf. It also gives the **bleed-out window**
already hinted at in `04 §9.4` ("a brief bleed-out window for drama") a home: a solid-but-not-instant
crew wound degrades *you* before it ends the run.

**Abstracted visual.**
- **Entirely a post-process + audio + instrument story — zero new geometry, maximal feeling.** Shaken =
  optic shake spike + a brief desaturation/vignette pulse + muffled/ringing audio (a low-pass on the
  spatial mix) that recovers over seconds. Cold = **periscope-glass frost accumulation** (the optional
  frozen-lens overlay already in ART_BIBLE §6d/§7a), breath fog, a slow blue color-grade creep, stiffer
  traverse. Wounded = a red vignette + heartbeat in the mix + degraded hands (slower reload/aim) during
  the bleed-out beat.
- All of these are **dials on systems already being built** (PostProcess on the cameras, the audio bus,
  the reload timer, the traverse rate) — nothing here asks for art. It is the purest expression of the
  pillar: a "crew morale & thermal" simulation presented as **a vignette, a frost overlay, and the way
  the sound goes muffled when you're scared.**

**UE 5.8 implementation.** A small `UKesselCrewStateComponent` holding `Morale`/`Cold`/`Wound` floats
that **modulate existing systems**: camera `PostProcessSettings` (`CameraComponent.h:353`) for vignette/
desat/frost; a low-pass/ducking on the audio via `AudioModulation` (in-box); the reload/traverse rates on
the gun/turret components; the periscope frost via a `UMaterialInstanceDynamic` parameter on the optic
mask (the reticle/mask material already planned, `07`). Near-miss detection reuses the projectile/impact
events. No new engine feature — it's a thin orchestrator over the camera, audio, and combat systems.

**Effort tier.** **T2–T3.** Each effect is cheap individually (T1 dials), but doing the *coherent* crew-
state model (what raises/lowers morale, cold accumulation, the bleed-out arc, the buttoned/head-out
thermal trade) is a tuning loop → **post-M1**, but it is the **highest "feeling"-payoff system after
audio** and should be an early post-M1 track. The hooks (PostProcess, audio modulation, view-state,
reload timer) must all exist first — and per this plan, they will.

---

## 10. Priority ranking (payoff-per-effort, under the pillars)

Ordered by simulation/feeling payoff per unit effort, annotated with M1 status. This feeds
`SIMULATION_ROADMAP.md`.

| Rank | System | Tier | Payoff driver | M1 status |
|---|---|---|---|---|
| 1 | **Internal armor + module/crew knockout** (card 1) | T2 | The whole combat loop; crew-of-1 lethality | **In M1** (steps 7–10) |
| 2 | **Gun handling: traverse/elev/reload** (card 5) | T1–T2 | Pillar-2 workload made literal; core loop | **In M1** (steps 5–6, 9) |
| 3 | **Spotting in whiteout (fog + audio-first)** (card 7) | T1 visual / T2–T3 audio | Pillar-3; the entire mood | Fog **in M1**; audio is the big near-term track |
| 4 | **Ammo cook-off & propellant fire** (card 2) | T1–T2 | Highest drama-per-line; reuses wreck light | Catastrophic branch **in M1** (step 10); drama post-M1 |
| 5 | **Mobility kill / thrown track** (card 3) | T2 | *The* thematic escape failure; movement hook exists | Hook in Phase-2 (step 14); wiring post-M1 |
| 6 | **Engine/transmission/fuel + overheat** (card 4) | T2 | Engine-as-sensor; resource arc; `AudioMotorSim` in-box | Feel in Phase-2; thermal/fuel post-M1 |
| 7 | **Fire control: ranging/shake/recoil** (card 6) | T1–T2 | Gunnery skill ceiling + the satisfying shot | Reticle/recoil M1-adjacent; rest post-M1 |
| 8 | **Suspension over snow** (card 8) | T3 | Makes the tank weigh 57 t; couples to 3 other cards | **Phase-2 movement** (step 14) |
| 9 | **Crew thermal/morale (lone operator)** (card 9) | T2–T3 | Highest feeling-payoff after audio; pure abstraction | Post-M1, early track once hooks exist |

**Cross-system interlocks worth protecting** (the deep-sim payoff is in the *couplings*, per Pillar 1):
engine-RPM → traverse rate (cards 4→5); hull attitude → gun depression + optic shake (cards 8→5,6);
track/engine kill → stranded-in-the-Kessel (cards 3,4); buttoned/head-out → spotting + audio + thermal
(cards 7,9); side hit → easiest plate **and** ammo stowage **and** fuel (cards 1,2,4). Building the
systems so these couplings emerge — rather than scripting each outcome — is what makes Kessel "deep sim"
rather than a scripted set-piece.

---

## 11. Open items / explicit ASSUMPTIONS

- **No code this phase.** Every "implementation" above is paper; nothing compiles until VS 2022 17.14.x +
  MSVC land (MILESTONE_1_PLAN §4). Community/marketplace systems (tracked-vehicle plugins, any
  ray-traced-audio plugin) **cannot be built or validated** now → all such items are T4/ASSUMPTION.
- **Ray-traced acoustics** (the user's "ideal"): **no in-box ray-traced audio engine ships in UE 5.8**
  (on-disk audio plugins are MetaSound/Synthesis/AudioModulation/Resonance/MicrosoftSpatialAudio/
  AudioMotorSim/AudioSynesthesia/AudioCapture). Approximate with occlusion line-traces + attenuation +
  reverb zones + HRTF; treat true geometric/ray acoustics as **T4 research**. Full treatment is owed to
  `AUDIO_ARCHITECTURE.md` (referenced by the pillars but **not yet on disk** — flagged as a gap).
- **`AudioMotorSim` 5.8 API surface** (engine-note-as-sensor, card 4) is confirmed *present* on disk but
  its exact class/property names are **unverified this phase** — ASSUMPTION until MSVC.
- **Terrain deformation / snow sinkage as geometry** (card 8) is **descoped** — modeled as a handling
  effect + particle cue, not a renderer feature; full deform is T4 and the whiteout makes it unnecessary.
- **Spaced-armor / HEAT defeat** (card 1) and **APCR/HE** are **schema-reserved, unwired** in M1 (`04 §2`).
- **Crew thermal/morale numbers** (card 9) — what moves morale/cold, recovery rates, the bleed-out
  duration — are entirely **ASSUMPTION / tuning targets**, like the ballistics and movement constants.
- This doc **prioritizes and translates**; it does not replace the load-bearing specs it extends
  (`04-ballistics-armor.md`, `03-tracked-vehicle.md`, `07-view-modes-controls.md`, `06-volumetric-fog-
  scatter.md`). Where they disagree with a card, **they win** and this doc should be reconciled.

---

## 12. Sources / grounding

- **Pillars & locked plans (primary truth):** `docs/DESIGN_PILLARS.md`, `docs/MILESTONE_1_PLAN.md`,
  `docs/ARCHITECTURE.md`, `docs/ART_BIBLE.md`.
- **Existing system specs extended here:** `docs/research/04-ballistics-armor.md` (armor/spall/cook-off/
  modules), `docs/research/03-tracked-vehicle.md` (movement/suspension/engine/track), `docs/research/
  07-view-modes-controls.md` (optic/traverse/reticle/view-state), `docs/research/06-volumetric-fog-
  scatter.md` (fog-as-spotting/emissive).
- **UE 5.8 on-disk features (confirmed this phase):** plugin listing under
  `C:/Program Files/Epic Games/UE_5.8/Engine/Plugins/{Experimental,Runtime,FX}` (Chaos*, GeometryCollection/
  FieldSystem, Niagara/NiagaraFluids, Metasound/Synthesis/AudioModulation/AudioMotorSim/AudioSynesthesia/
  ResonanceAudio/MicrosoftSpatialAudio); API anchors carried from `ARCHITECTURE.md §14`
  (`PrimitiveComponent.h:1770`, `World.h:2161`, `WorldSubsystem.h:16`, `CameraComponent.h:46/353`,
  `LightComponentBase.h:120`, `ExponentialHeightFogComponent.h:136`, Chaos engine/transmission config
  `ChaosWheeledVehicleMovementComponent.h:223-258,312`).
- **Reference hardcore sims (design touchstones, behavior models — not engine truth):** Gunner HEAT PC
  (Radian Simulations) — multi-layer armor + interior module/crew + manual WW2 fire control; Steel Beasts
  Pro (eSim Games) — crew/morale, gunnery procedure, M-kills, repair; War Thunder sim battles — manual
  spotting/ranging, DeMarre-style pen + overmatch/normalization/ricochet (already the `04` model
  reference); Graviteam Tactics / Steel Armor — soft-soil/snow mobility, suppression/morale. Cited as
  *gameplay-model* references, consistent with `04 §15`'s use of War Thunder conventions; all tuning
  numbers remain Kessel's own.
</content>
</invoke>
