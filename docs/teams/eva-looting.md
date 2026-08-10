# Kessel — EVA / DISMOUNTED LOOTING

Status: **DESIGN / SPEC-AHEAD** (no C++ compiled this phase — VS/MSVC absent; do not build or launch).
Engine: **UE 5.8.0** at `C:/Program Files/Epic Games/UE_5.8`.
Team: EVA / Dismounted Looting. This doc owns the **on-foot mode** — leaving the tank, exploring ruins,
looting, the Luger, the dismounted inventory, and the possession architecture that swaps between the
tank pawn and the foot character.

This system is **deliberately later** (NOT M1 — see §13). It is specced in full now so the load-bearing
hooks (possession seam, inventory-as-resource-truth, warm-cabin thermal coupling) can be reserved early
for near-zero cost, exactly as the simulation roadmap reserves armor/track/crew schema at M1 start
(`SIMULATION_ROADMAP.md §2.1`).

Grounding (do not contradict): `DESIGN_PILLARS.md`, `GAME_DESIGN.md`, `ARCHITECTURE.md`, `ART_BIBLE.md`,
`SIMULATION_ROADMAP.md`, `MILESTONE_1_PLAN.md`. Engine APIs are cited `path:line` where carried from the
locked docs; anything not provable on disk this phase is tagged **ASSUMPTION** (most line-level anchors
for `ACharacter`/`UCharacterMovementComponent` are unverified until MSVC lands and are flagged as such).

---

## 0. The pillar exception — handle honestly

EVA is the **single explicit, deliberate exception** to Pillar 5 ("Always inside the steel"). It must be
treated as a hard, narrow exception, not a loophole that erodes the rule. The discipline:

- **Pillar 5 is suspended ONLY in its "inside the vehicle" clause.** The *first-person-only* half of the
  rule is **never** suspended. EVA is still **first-person ONLY** — gloved hands, a held Luger, the inside
  of a gas-mask/collar frame, the world seen down your own nose. **No third-person, no exterior chase cam,
  ever** (Pillar 5 / `ART_BIBLE §1` / §8 DO-NOT). You leave the steel; you do not leave first-person.
- **Every other pillar holds, unchanged, and EVA must actively serve them:**
  - **Pillar 1 (deep-sim / abstracted):** the looting/inventory/exposure systems simulate deep; the
    *presentation* stays abstracted — fog-swallowed ruins, silhouette, grain, isolated emissive. On foot
    you are even more inside the whiteout. Bucket-A default (`SIMULATION_ROADMAP §0.2`).
  - **Pillar 3 (sound is the primary sensor):** on foot you have **no hull, no engine note, no buttoned
    mix** — you are naked in the acoustic field. Your own footfall in snow, your breath, the wind, and a
    rifle bolt in the dark are the whole sensorium. EVA is the most acute expression of sound-as-sensor.
  - **Pillar 4 (the cold, the dark, the few warm lights):** off-tank you lose the cabin's warmth and its
    lights; at night your torch/lighter saves you AND betrays you (mirrors the tank-lights trade). Cold
    exposure is *worse* outside the steel — this is the dramatic point of leaving.
  - **Pillar 6 (historical grounding):** a tanker's personal sidearm is the **Luger P08** — desperate,
    weak, anti-personnel only. No tanker clears a ruin with a rifle he doesn't carry. Plausible, not
    pedantic.
  - **Permadeath (GAME_DESIGN):** on foot you are at your most fragile. One rifle round can end the run.
    Leaving the tank is the highest-variance decision in the game.

- **Why the exception earns its place (the litmus, `SIMULATION_ROADMAP §5.1`):** EVA "deepens the
  simulation AND the feeling." It makes the *attrition* loop physical — resources (ammo + components) are
  scavenged from places a 57-tonne Tiger **cannot reach**: cellars, bunkers, building interiors, wreck
  hulls, the dead. It converts the map's ruins from backdrop into interactive resource, and it makes the
  tank itself a thing you can *lose by leaving*. If EVA were only "a cool feature," it would be rejected
  by the pillar; it survives because it is the resource economy's engine and the tensest choice in the run.

**Guardrail:** if any EVA feature pulls toward "open-world infantry shooter," it has failed the exception.
EVA is short, dread-soaked, resource-driven excursions from a vulnerable parked tank — never the main mode,
never a power fantasy. The Luger being *bad* is load-bearing (§6).

---

## 1. The dismounted loop (one paragraph + beats)

You are driving west. You reach a **handcrafted loot site** on or just off your route — a shelled village,
a wreck field, a supply dump, a bunker line, a frozen crossing littered with the dead. Some of it you can
shell or drive over; the loot worth having is **inside** — cellars, hulls, buildings, dugouts — where the
tank can't go. You **park** (engine off = quiet + cold + a risky cold-restart later; engine on = warm,
ready, fuel-burning, a beacon). You **pop the hatch and climb out** — a deliberate, committed transition,
not a menu. Now you are a lone man in the snow with a pistol, a satchel, and a finite carry capacity. You
**search** wrecks/ruins/bodies, **loot** ammo, components, upgrades, and cigarettes, and **ferry** it back
to the tank in trips — because you can't carry it all at once, and **every second out there your tank sits
unmanned and exposed**, its cold engine a liability, its lights (if on) a beacon, threats possibly closing
on the silhouette you left behind. You **re-board through the hatch**, transfer your satchel into stowage,
and either press on or go out again. Death on foot — a rifle shot from a ruin, freezing, a threat you can't
fight with a Luger — ends the run as surely as a penetrated crew compartment.

**Beats:** Approach & park → Commit (pop hatch, dismount) → Excursion (move, search, fight-or-flee, loot)
→ Ferry/return (re-board, transfer) → Decide (press on / go again). Tension peaks at maximum distance from
the tank with a full satchel.

---

## 2. Exit / enter-tank flow — the possession swap

The core mechanism. The player controls **one** of two pawns at a time; an explicit, diegetic handshake
swaps possession between `AKesselTankPawn` and a new `AKesselFootCharacter`. Never both, never a body
double walking around the tank you also drive.

### 2.1 Ownership & swap (who possesses what)

- The **`AKesselPlayerController`** is the single possessing entity (it already owns the input mapping
  contexts and swaps them per view mode — `KesselPlayerController.h`, `ARCHITECTURE §2`). It gains a
  `UKesselDismountComponent` (or the logic lives on a `UKesselCrewSubsystem` it talks to — see §11) that
  drives the swap state machine.
- **Dismount:** controller `UnPossess()`es the tank → spawns/places `AKesselFootCharacter` at the tank's
  **`DismountPoint`** (a child `USceneComponent` at a hull-side hatch step, like the existing
  `MuzzlePoint`/`GunnerEye` relative-offset points, `KesselTankPawn.h`) → `Possess()`es the foot character
  → swaps to the **foot input mapping context** → swaps to the foot camera. The tank pawn is left **in the
  world, idle, fully simulated and fully vulnerable** (see §9 risk).
- **Re-board:** the foot character looks at the tank's `IKesselInteractable` boarding volume, holds
  *Interact* → controller `UnPossess()`es the foot character → re-`Possess()`es the tank → swaps input/
  camera back to the gunner optic → the foot character is **destroyed (or pooled/hidden)**.

### 2.2 Transition is committed and diegetic (no instant teleport)

Per `ART_BIBLE §7` ("Transitions between modes are diegetic … no abrupt cuts"), exit/enter is a short,
**interruptible-only-at-defined-points** sequence, not a snap:

- **Exit:** `H`/hold to unbutton is already the open-hatch state (`UKesselViewComponent::CommanderUnbuttoned`,
  `bExposed=true`). EVA extends it: from unbuttoned, a *hold-to-climb-out* finishes the dismount (camera
  rises out of the cupola, then the possession swap fires at the apex, masking the pawn switch). This reuses
  the existing buttoned→unbuttoned→exposed escalation as the on-ramp to fully-out. The cost of leaving is
  *felt* (you are exposed for the whole climb).
- **Enter:** approach the hull, hold *Interact* at the boarding volume → climb-in sequence → swap at the
  apex → settle into the optic. A few seconds of vulnerability bracket every transition (you cannot fight
  mid-climb) — deliberate tension, not a loading gate.

### 2.3 State machine (`EKesselCrewLocus`)

```
EKesselCrewLocus { Mounted, Unbuttoned, Climbing_Out, Dismounted, Climbing_In }
  Mounted        ──H hold──►  Unbuttoned         (existing UKesselViewComponent state, bExposed)
  Unbuttoned     ──hold climb─► Climbing_Out      (camera rise; input locked; cancellable until apex)
  Climbing_Out   ──apex──►     Dismounted         (POSSESSION SWAP: tank → foot character)
  Dismounted     ──Interact at boarding vol──► Climbing_In
  Climbing_In    ──apex──►     Mounted            (POSSESSION SWAP: foot → tank; settle to optic)
```

The swap (the two `Climbing_*` → terminal edges) is the only place possession changes; everything else is
camera/state on a single pawn. This keeps the swap **rare, explicit, and masked**, which is the safe way to
do mid-game possession changes.

### 2.4 What persists across the swap

- **Run resources are the tank's truth, not the pawn's** (§7): ammo, components, cigarettes live in a
  persistent store the *tank* owns (its stowage). The foot character carries only a transient **satchel**.
  This is what makes ferrying a thing.
- **Cold/morale/wound state is the operator's, and crosses both pawns** (§5.3 / coupling to roadmap #14):
  it is a property of *you*, not of the tank or the body — so it lives on the persistent crew-state owner
  (the controller/subsystem), read by whichever pawn you currently inhabit. You don't get warm by swapping
  pawns; you get warm by being in the cabin, near fire, or smoking.

---

## 3. The dismounted first-person character

### 3.1 `AKesselFootCharacter : ACharacter`

A first-person character pawn. Uses the engine `ACharacter` + `UCharacterMovementComponent` (walking,
crouch, the standard capsule + step/slope handling) — this is the one place in Kessel we *do* want the
engine's character locomotion, because authoring it would be pure cost for no pillar payoff.

- **First-person camera** (`UCameraComponent`) at eye height, same `path:line` component the tank uses
  (`CameraComponent.h:32`, carried from `KesselTankPawn.h`). Optional thin first-person body/arms presence
  (gloved hands holding the Luger / reaching to search), consistent with `ART_BIBLE §1` "First-person body
  presence … hands appear only during interactions." **No full third-person body is ever rendered to the
  player** — a head-shadow/arms rig at most.
- **Movement feel:** slow, heavy, cold. This is not an athletic shooter. Base walk is deliberate; crouch
  for cover/searching; a **short, stamina-limited sprint** (§5.1) for the desperate dash back to the hull.
  Deep snow slows you (couples to the same `EPhysicalSurface` enum the tank reads — roadmap #4; compute the
  surface once, present it many ways — `SIMULATION_ROADMAP §4`). Ice is treacherous on foot too.
- **No vehicle interplay weirdness:** while you are the foot character, the tank is an inert physics actor;
  you can walk around it, under its gun, behind it for cover. (You cannot, of course, see it in third
  person from *inside* it — you literally are outside now; this is the sanctioned exception.)

### 3.2 Owned components (foot character)

```
AKesselFootCharacter : ACharacter
 → UCharacterMovementComponent   Movement        engine locomotion (walk/crouch/sprint)
 → UCameraComponent              FootCam          first-person eye
 → USkeletalMeshComponent        Arms (optional)  FP hands/Luger only; no full-body to the player
 → UKesselInteractionComponent   Interaction      forward-trace prompt + Interact() dispatch (§4)
 → UKesselInventoryComponent     Satchel          transient carry (slot/weight limited) (§7)
 → UKesselFirearmComponent        Luger            the only weapon (§6)
 → UKesselExposureComponent      Exposure         off-tank cold/stamina (§5); writes to crew-state owner
 → UKesselTorchComponent         Torch (optional) hand-light/lighter: saves AND betrays at night (§5.4)
```

`UKesselInteractionComponent`, `UKesselInventoryComponent`, and `UKesselFirearmComponent` are written as
**reusable `UActorComponent`s** so the tank can share them where it makes sense (the tank's stowage is also
a `UKesselInventoryComponent` with a bigger capacity; the *Interact* dispatch is the same on both pawns).
This mirrors the existing shared-component discipline (`ARCHITECTURE §10`: Armor/Gun/Turret shared between
player tank and enemy).

---

## 4. Interaction + looting system

### 4.1 `IKesselInteractable` (UINTERFACE) + `UKesselInteractionComponent`

- **`IKesselInteractable`** — a UInterface implemented by anything the player can act on: loot caches,
  bodies, the tank's boarding volume, a fire to warm at, a wall switch, a smoking spot. Minimal surface:
  `CanInteract(instigator) → bool`, `GetPrompt() → FText` (CRT-green diegetic prompt, `ART_BIBLE §5`),
  `Interact(instigator)`, optional `GetHoldDuration()`.
- **`UKesselInteractionComponent`** on the foot character — each tick (or on a short timer) runs a single
  **forward `LineTraceSingleByChannel`** on a dedicated `Interaction` trace channel from `FootCam`
  (`World.h:2161`, the same trace primitive the ballistics/armor path uses). If it hits an
  `IKesselInteractable` within reach and `CanInteract`, it surfaces the prompt; on *Interact* (tap or hold,
  via `GetHoldDuration`) it dispatches. One channel, one trace, no per-object tick — cheap (Bucket A).

### 4.2 Loot sources & the search act

Three loot archetypes, all `IKesselInteractable`, all backed by a **`UKesselLootTableAsset`** (a
`UPrimaryDataAsset`, consistent with `DA_Armor_*` / `UAtmospherePreset` — `ARCHITECTURE §6`) that rolls
contents from a seeded `FRandomStream` (the **same per-run seed** the ballistics subsystem already owns,
`UKesselBallisticsSubsystem`, so loot is reproducible per run — `ARCHITECTURE §4`):

| Source | Actor | What it yields | Search feel |
|---|---|---|---|
| **Wreck** (your own burnt column, enemy hulls, trucks) | `AKesselLootCache` (wreck subtype) | main-gun ammo (from same-calibre wrecks), components, sometimes a salvageable upgrade | reach into the hull; timed hold; emissive ember light if still burning (reuses wreck light, `SIMULATION_ROADMAP #5/#15`) |
| **Ruin / depot / bunker** (cellars, dugouts, crates, lockers) | `AKesselLootCache` (container subtype) | components, ammo crates, cigarettes, rare upgrades | open/pry; the *interior* is the reason to be on foot |
| **Body** (the dead — your trail of advance, GAME_DESIGN spine) | `AKesselLootCache` (corpse subtype) | Luger 9mm ammo, cigarettes (incl. rare types), small components, environmental story | a quiet, grim search; environmental storytelling (`GAME_DESIGN`: "story told by the world") |

- **The search is a timed, vulnerable hold** (`GetHoldDuration`), not an instant pickup — you are head-down,
  hands busy, exposed, for a beat. This is the foot-scale analogue of the reload's "moment of maximum dread"
  (`SIMULATION_ROADMAP #2`).
- **Loot reads as the world, not a UI explosion:** found items are a terse CRT-green diegetic line
  ("8.8cm PzGr · ×2", "ENGINE PART", "Machorka cigarettes") — `ART_BIBLE §5` typography — plus a sound.
  No loot-beam, no rarity confetti (fails the screenshot-drift test, `SIMULATION_ROADMAP §5.7`).
- **Empty/already-searched caches stay searched** (a flipped bool on the actor) so a ruin you cleared reads
  as cleared — no infinite farming; **attrition is the pressure** (`GAME_DESIGN`).

### 4.3 Upgrades (scavenged progression)

Per `GAME_DESIGN` ("Upgrades and ammo are scavenged along the way … your tank grows stronger or weaker by
what you find and what you lose"), some loot is a **component upgrade** (better optic, spare track links,
an extra ammo rack, engine parts that restore powertrain health — couples to roadmap #9 engine/transmission
and #10 track repair). On foot you **carry the part back**; installing it is a tank-side interaction (a
field-repair/install state, possibly the same exposed timed state as #10's track repair). EVA is thus the
*acquisition* arm of the whole progression economy; the install/effect lives in the relevant tank system's
doc. EVA owns finding and carrying; the tank systems own applying.

---

## 5. Off-tank survival: cold, stamina, light

### 5.1 Stamina (lightweight, optional-but-recommended)

A single `Stamina` float on `UKesselExposureComponent`. Sprinting and climbing drain it; standing/walking
recover it. **Cold suppresses recovery** (coupling, below). Purpose is narrow: stamina gates the *panic
sprint* back to the hull, so fleeing is a resource you can run out of at the worst moment — it is a tension
lever, not a survival meter. Keep it minimal (T1). **No food/water/hunger ever** (`GAME_DESIGN` hard rule).

### 5.2 Cold exposure off-tank (the reason leaving is dangerous)

The tank cabin is *relatively* warm (it is the only friendly thing in the world — `ART_BIBLE §1`). **Outside
in the blizzard, cold accumulates faster** and there is no cabin to duck back into without crossing open
ground. `UKesselExposureComponent` ramps a `Cold` value while dismounted, modulated by wind/weather and
whether you're sheltered (inside a ruin = slower) or in the open blizzard (fastest). High cold = the same
abstracted consequences the crew-thermal system already specs (`SIMULATION_ROADMAP #14`): numb/slower hands
(longer Luger reload, fumbled searches), stiffer aim, a slow blue post-process grade creep, breath fog,
heartbeat in the mix. **It is presented entirely through post-process + audio + instrument — zero new
geometry** (Bucket A→B, `SIMULATION_ROADMAP #14`).

**This is the same crew-state model, not a parallel one.** `Cold`/`Morale`/`Wound` live on the persistent
crew-state owner (§2.4) and are read by whichever pawn you inhabit; EVA simply *adds a faster cold-ramp and
removes the cabin-warmth relief while dismounted*. The reliefs are: **return to the cabin**, **warm at a
fire** (a burning wreck is now a friend, not just a threat marker — emissive coupling), or **smoke** (§5.3).

### 5.3 Cigarettes = the health-regen mechanic (EVA owns acquisition + the smoking interaction)

Per the user directive, **smoking is the health/condition-regen mechanic**, and cigarettes are an inventory
resource looted on foot. EVA owns the **inventory + acquisition + the smoking interaction hook**; the exact
regen tuning is a coupled crew-state concern (`SIMULATION_ROADMAP #14`, deferred numbers).

- **Smoking is a deliberate, vulnerable interaction** (an `IKesselInteractable` self-action or a held input):
  you stop, light up (a warm `#FF9B3D` lighter flame — emissive puncture, `ART_BIBLE §3b/§4b`, that
  **betrays you at night**, §5.4), and over several seconds the cigarette **recovers morale/condition** and
  warms you slightly. You are head-down and stationary while smoking — a real risk, especially on foot.
- **Many types, data-driven** (`UKesselCigaretteType : UPrimaryDataAsset`): Russian (e.g. machorka — harsh,
  small regen, common from bodies) vs German (issue cigarettes — standard regen) vs **special-boost** types
  (rare, larger morale/steadiness boost — e.g. a brief steadier-aim/calmer window). Each is a row of
  {regen amount, duration, warmth, rarity, side-effect}.
- **SPLIFFS are PURELY NEGATIVE and extremely rare** (explicit user directive): no upside whatsoever — a
  duration of **vision blur** (post-process — couples to the same grade/vignette stack) **+ slowed movement**
  (a movement-scale debuff on `UCharacterMovementComponent`), no regen, no warmth. They exist as a rare,
  nasty "wrong pocket" hazard / dark-comedy beat. Looting is a gamble; not everything you find helps you.
- Cigarettes/lighting work **inside the tank too** (the cabin is where you'd most want a smoke), so the
  smoking interaction is pawn-agnostic — but the *acquisition* (looting them) is an EVA activity.

### 5.4 Light at night: saves you AND betrays you (Pillar 4)

On foot at night you may carry a **torch/lantern or a lighter** (`UKesselTorchComponent`, optional). It is
the on-foot mirror of the tank-headlight trade (`ART_BIBLE §4`, the dynamic day/night directive): light
lets you see the ruin, read the loot, and not break an ankle — and it is a **warm emissive point source
scattering into the fog** (`ART_BIBLE §4b`) that any threat can see from far off. Smoking's lighter flame is
the same trade in miniature. **Darkness is safety and blindness; light is sight and exposure** — the core
Pillar-4 tension, now in your hand. This couples directly to the dynamic atmosphere team's day/night system.

---

## 6. The Luger P08 — your only weapon, and it's bad on purpose

Per the user directive: the **Luger pistol is the ONLY dismounted weapon** — desperate, weak, useless
against anything armored, for infantry/soft threats only. The weakness is **load-bearing** (§0 guardrail):
it keeps EVA tense and non-power-fantasy, and it makes "flee back to the tank" the correct answer most of
the time.

- **`UKesselFirearmComponent`** (reusable component; the Luger is its first/only instance via a
  `UKesselFirearmData : UPrimaryDataAsset`). Hitscan `LineTraceSingleByChannel` (`World.h:2161`) — no need
  for the heavy substepped projectile path; pistol ranges are short and the round is anti-personnel.
- **Handling (historical, Pillar 6):** Luger P08, **9×19mm Parabellum, 8-round box magazine**, slow
  deliberate fire, a real reload cycle (`FTimerManager`, cold hands lengthen it — §5.2 coupling), iron
  sights, meaningful recoil/sway for a pistol. It feels like a last-ditch sidearm, because it is.
- **Damage model:** effective only against **unarmored/soft targets** (infantry, dogs, a threatening
  silhouette). It does **literally nothing meaningful to armor** — firing it at a tank, an armored car, or a
  bunker is futile (a flat *spang*, no effect), which the player must *feel* so they never treat it as a
  combat tool. It routes through a **separate soft-target damage path, NOT the kinetic AP/armor ballistics
  subsystem** (that subsystem is for tank rounds vs armor faces, `ARCHITECTURE §4`); the Luger is a small
  flat-damage hitscan vs an infantry health pool. (Reserve a clean seam so the two damage systems never
  entangle.)
- **Ammo:** 9mm rounds are an inventory line (§7), looted mostly from **bodies** (§4.2). Scarcity keeps every
  shot a decision — same "every shell counts" ethos as the main gun (`GAME_DESIGN`), scaled down.
- **Abstracted presentation:** muzzle flash is a *small* warm puncture into the fog (a fraction of the 88's
  bloom, `ART_BIBLE §4b`); the report is sharp and **terrifyingly loud in the open acoustic field** (Pillar
  3) — firing the Luger advertises your position to everything. Bucket B.

**Infantry threats** (what the Luger is *for*) are a coupled later system: full foot-enemy AI is its own
large track (not owned here). The first EVA slice can ship with **scripted/static threats** (a sniper sound
cue you must avoid, a single scripted infantryman) and grow real AI later. EVA *needs* the Luger to exist;
it does not need a populated battlefield of infantry to be valuable (looting + cold + the exposed tank carry
the tension on their own).

---

## 7. The inventory — lightweight, ammo + components + cigarettes (NO food/water)

### 7.1 Two tiers: carry (satchel) vs stowage (tank)

The inventory's job is to make **ferrying** a real act and to be the **resource truth** of the run.

- **`UKesselInventoryComponent`** — one reusable component, two instances with different capacities:
  - **Satchel** (on `AKesselFootCharacter`): small **carry capacity** (slots and/or weight). You cannot
    haul a whole depot at once — you make trips, and trips cost exposure time.
  - **Stowage** (on `AKesselTankPawn`): the large hold — the **persistent run resource** the tank's gun and
    systems actually draw from. Surviving the swap (§2.4) because it lives on the tank, which persists in
    the world.
- **Contents (the only resource classes — `GAME_DESIGN` hard rule, NO food/water/survival-needs):**
  - **Ammunition** — main-gun rounds by `EShellType` (`KesselBallisticsTypes.h`: AP/APCBC…), **and** Luger
    9mm. The main-gun ammo you ferry in directly **feeds the gun's chambered/stowed supply** — EVA is how the
    tank re-arms mid-run.
  - **Components** — generic repair/spare parts + named upgrades (optic, track links, ammo rack, engine
    parts). Feed repair/upgrade systems (couples to roadmap #9/#10).
  - **Cigarettes** — by `UKesselCigaretteType` (§5.3), incl. the rare negative spliff.
- **Transfer at the hatch:** re-boarding (§2) opens a terse transfer — empty the satchel into stowage. Kept
  minimal and diegetic (CRT-green list, `ART_BIBLE §5`), not a grid-Tetris management game. Overflow you
  can't fit you must drop or leave behind (another attrition lever).

### 7.2 Data shape (reserve early — §13)

```
FKesselInventoryItem (USTRUCT)   { EItemClass Class; FName Id; int32 Count; float Weight; }
EItemClass (UENUM)               { MainGunAmmo, PistolAmmo, Component, Cigarette }   // NO food/water
UKesselInventoryComponent        { TArray<FKesselInventoryItem> Items; float Capacity; Add/Remove/Transfer }
UKesselLootTableAsset : UPrimaryDataAsset  { weighted entries; rolled by the per-run FRandomStream }
UKesselCigaretteType : UPrimaryDataAsset   { Regen, Duration, Warmth, Rarity, ESideEffect (incl. Spliff) }
UKesselFirearmData : UPrimaryDataAsset     { MagSize=8, RPM, Damage(soft-only), Reload, Spread, 9mm ammo id }
```

Homogeneous rolled loot → consider a `UDataTable` for loot entries; heterogeneous per-asset configs
(cigarette/firearm/loot-table) → `UPrimaryDataAsset` — same rationale as `ARCHITECTURE §6`.

---

## 8. Risk & tension — the whole point of leaving

EVA must always feel like a **gamble against permadeath**, never a safe farming chore. The risk gradient:

- **Your tank sits exposed and unmanned.** While you're out it is an inert actor that **cannot defend
  itself**. Threats can find it, shoot it, or burn it. **If the tank is destroyed while you're on foot, the
  run is effectively over** — you cannot cross a 1.5–2.5-hour map (`GAME_DESIGN` scale directive) on foot
  with a Luger. Leaving the tank stakes the whole run.
- **Engine off vs on (the parking dilemma):** engine **off** = quiet, hidden, fuel-saved — but the cabin
  goes cold and you face a **risky cold-start** (couples to roadmap #9 cold-start difficulty) when you
  sprint back under threat. Engine **on** = warm, instantly ready to flee — but **loud, fuel-burning, and a
  thermal/acoustic beacon**. A real decision every time you dismount.
- **Lights betray** (§5.4): the tank's lights left on at night, or your own torch/lighter/cigarette, are
  warm scatter any threat can read across the fog.
- **Distance = danger.** Risk scales with how far from the hull you stray and how full your satchel is.
  Maximum tension = far away, overloaded, cold, a sound in the dark, a slow sprint home on draining stamina.
- **You are fragile on foot.** No 100 mm of frontal armor — **a single rifle round can kill you**, and
  permadeath means that's the run. The Luger (§6) means you usually *cannot win* a fight you start; the
  correct play is stealth, avoidance, and the dash home.
- **The committed transition** (§2.2) brackets every dismount/board with a few seconds you cannot fight —
  you choose your moment to climb out and to climb back in.

These are **emergent from shared signals** (exposure timer, the persistent tank actor's vulnerability, the
day/night light state, distance), not scripted — the deep-sim/couplings discipline (`SIMULATION_ROADMAP §4`).

---

## 9. Integration with the tank systems & the huge map

- **Possession & input** reuse the controller's existing context-swap machinery (`KesselPlayerController.h`)
  and the unbutton/exposed escalation already in `UKesselViewComponent` (§2.2). EVA is an *extension* of the
  buttoned→unbuttoned→out ladder, not a separate subsystem bolted on the side.
- **Inventory is the re-arm/upgrade conduit** (§7): the tank's gun, engine, and track-repair systems draw
  from stowage that EVA fills. EVA is the **acquisition arm of the entire attrition economy** — without it,
  ammo/components are only what you started with or grabbed by driving over open dumps.
- **Crew-state is shared** (§5.2): EVA adds a faster cold-ramp and removes cabin warmth, but uses the *same*
  `Cold`/`Morale`/`Wound` model as the buttoned/head-out trade (`SIMULATION_ROADMAP #14`).
- **Surface enum is shared** (§3.1): snow/ice/mud on foot read the same `EPhysicalSurface` slots reserved at
  M1 for the tracks (`SIMULATION_ROADMAP #4`) — compute the surface once, present it for both pawns.
- **Emissive/atmosphere is shared:** torch, lighter, muzzle flash, and the now-friendly burning-wreck warmth
  all use the locked volumetric-fog + emissive-scatter stack (`ART_BIBLE §4`, `research/06`). No new render
  system for EVA's lighting.
- **The huge map frames EVA as local excursions, not traversal.** The world is handcrafted, center→west, a
  multi-hour run (`GAME_DESIGN`). EVA happens at **discrete handcrafted loot sites strung along the western
  route** — you drive the distances; you walk only the *sites*. Foot-scale interiors (cellars, bunkers,
  building footprints, wreck hulls) are authored at those sites precisely because **the tank can't enter
  them** — that geometric fact is the justification for getting out. EVA does **not** add open-world infantry
  traversal across the whole map (that would break the exception, §0). The map-blockout team's METHOD should
  mark these sites as EVA zones with authored foot-scale interiors and `AKesselLootCache` placements; EVA
  does not place actors itself (hard constraint).

---

## 10. C++ / architecture plan

New classes (module `Kessel`, UE convention; `→` owns/has-a, `:` inherits). Mirrors the
`ARCHITECTURE §2` class-map style. **None compiled this phase.**

```
POSSESSION / CONTROL
  AKesselPlayerController : APlayerController        (existing) — single possessing entity
   → UKesselDismountComponent (NEW)                  the swap state machine (EKesselCrewLocus, §2.3);
                                                     UnPossess/Possess handshake; input+camera swap
  UKesselCrewSubsystem : UWorldSubsystem (NEW, opt)  persistent crew-state owner across both pawns:
                                                     Cold/Morale/Wound/Stamina; per-run (world-scoped),
                                                     like UKesselBallisticsSubsystem (WorldSubsystem.h:16)

THE FOOT PAWN
  AKesselFootCharacter : ACharacter (NEW)
   → UCharacterMovementComponent   Movement          engine walk/crouch/sprint (CharacterMovementComponent.h —
                                                     ASSUMPTION line-level until MSVC)
   → UCameraComponent              FootCam           first-person eye (CameraComponent.h:32)
   → USkeletalMeshComponent        Arms (optional)   FP hands/Luger ONLY — no full third-person body
   → UKesselInteractionComponent   Interaction       forward-trace prompt + Interact() dispatch (§4)
   → UKesselInventoryComponent     Satchel           transient carry (§7) — REUSABLE component
   → UKesselFirearmComponent       Luger             the only weapon (§6) — REUSABLE component
   → UKesselExposureComponent      Exposure          off-tank cold/stamina ramp; writes to UKesselCrewSubsystem
   → UKesselTorchComponent         Torch (optional)  hand-light point source (§5.4)

INTERACTION / LOOT
  IKesselInteractable (NEW UINTERFACE)               CanInteract/GetPrompt/Interact/GetHoldDuration (§4.1)
  UKesselInteractionComponent : UActorComponent (NEW) single forward LineTraceSingleByChannel (World.h:2161)
  AKesselLootCache : AActor (NEW)                     implements IKesselInteractable; wreck/container/corpse
                                                      subtypes; rolls a UKesselLootTableAsset; bSearched latch
  (AKesselTankPawn gains a boarding IKesselInteractable volume + DismountPoint USceneComponent)

INVENTORY / DATA (data-driven, ARCHITECTURE §6 pattern)
  UKesselInventoryComponent : UActorComponent (NEW)  TArray<FKesselInventoryItem>; Capacity; Add/Remove/Transfer
   - on AKesselFootCharacter (small Satchel) and AKesselTankPawn (large Stowage; the run-resource truth)
  FKesselInventoryItem (USTRUCT)                      { EItemClass; FName Id; int32 Count; float Weight }
  EItemClass (UENUM)                                  { MainGunAmmo, PistolAmmo, Component, Cigarette }  // no food/water
  UKesselLootTableAsset : UPrimaryDataAsset (NEW)     weighted entries; rolled by the per-run FRandomStream
  UKesselCigaretteType  : UPrimaryDataAsset (NEW)     { Regen, Duration, Warmth, Rarity, ESideEffect(Spliff…) }
  UKesselFirearmData    : UPrimaryDataAsset (NEW)     { MagSize, RPM, SoftDamage, Reload, Spread, AmmoId }

WEAPON
  UKesselFirearmComponent : UActorComponent (NEW)     hitscan vs SOFT-target health ONLY (NOT the AP/armor
                                                      ballistics subsystem); mag/reload/spread; muzzle FX

SURVIVAL
  UKesselExposureComponent : UActorComponent (NEW)    Cold ramp (weather/shelter-gated) + Stamina; modulates
                                                      PostProcess/audio/handling via UKesselCrewSubsystem (#14)
  UKesselTorchComponent : UActorComponent (NEW, opt)  UPointLightComponent w/ VolumetricScatteringIntensity (#4 light trade)
```

### 10.1 Per-tick / data flow (dismounted)

```
INPUT (foot mapping context, swapped in by UKesselDismountComponent)
  WASD/Shift ─► UCharacterMovementComponent (walk/sprint; snow surface scales speed)
  Mouse ─────► FootCam look
  LMB ───────► UKesselFirearmComponent.Fire()  (hitscan vs soft health; futile vs armor)
  E (tap/hold)► UKesselInteractionComponent → IKesselInteractable.Interact()  (search/board/smoke/warm)

TICK
  UKesselInteractionComponent: one forward LineTrace → prompt if IKesselInteractable in reach
  UKesselExposureComponent:    Cold += f(weather, sheltered?, mounted?) ; Stamina recover (cold-suppressed)
                               → write Cold/Morale/Stamina to UKesselCrewSubsystem
  UKesselCrewSubsystem:        Cold/Morale/Wound → camera PostProcess (grade/vignette/blur),
                               audio duck/heartbeat, handling/reload-rate modifiers (read by current pawn)

EVENTS
  Search:  IKesselInteractable.Interact() (timed hold) → roll UKesselLootTableAsset → Satchel.Add(...)
  Board:   tank boarding volume Interact() → UKesselDismountComponent swap (foot→tank) → transfer Satchel→Stowage
  Smoke:   smoking interaction → UKesselCigaretteType effect → Morale/Warmth regen (or Spliff: blur+slow)
  Fire:    hitscan → soft-target health; loud report into the open acoustic field (Pillar 3)
```

### 10.2 Engine-API honesty (verify once MSVC lands)

- `ACharacter` / `UCharacterMovementComponent` are standard engine
  (`Runtime/Engine/Classes/GameFramework/Character.h`,
  `Runtime/Engine/Classes/GameFramework/CharacterMovementComponent.h`) — **exact line anchors are
  ASSUMPTION** until the editor/MSVC is available, consistent with how the locked docs tag unprovable
  items. `APlayerController::Possess`/`UnPossess`, `UCameraComponent` (`:32`), `UWorldSubsystem`
  (`WorldSubsystem.h:16`), `LineTraceSingleByChannel` (`World.h:2161`), `UPrimaryDataAsset`
  (`DataAsset.h:47`), `FRandomStream`, `FTimerManager` are all carried-verified from `ARCHITECTURE §14`.
- EVA pulls in **no new plugin** — it reuses Enhanced Input, the fog/light stack, Niagara FX, UMG. The one
  module addition is whatever `ACharacter` needs that the tank pawn didn't (it's all in `Engine`). Keep the
  soft-target damage path **strictly separate** from `UKesselBallisticsSubsystem` (armor/AP only).

---

## 11. M1 vs later — the slice

EVA is **NOT M1.** M1 is "drivable + shooting tank in volumetric fog" (`MILESTONE_1_PLAN`), and EVA is the
single largest scope addition *outside* the "inside the steel" pillar — a whole second pawn, character
locomotion, interaction, inventory, a weapon, survival exposure, infantry threats, and foot-scale level
content. Honest placement: **a dedicated post-M3 "EVA milestone" (effectively M4-class)**, because it
depends on systems that themselves land at M2/M3:

- needs the **crew-thermal/morale model** (`SIMULATION_ROADMAP #14`, M3) for cold-off-tank to mean anything;
- needs the **acoustic sensor spine** (`#8`, M2) for on-foot sound-as-sensor to land;
- needs the **dynamic day/night atmosphere** (separate team) for the light-betrays-you trade;
- the **upgrade-install / re-arm** hooks couple to engine/track systems (`#9/#10`, M2/M3).

### 11.1 RESERVE at / near M1 (cheap, paper-or-thin, prevents churn — `SIMULATION_ROADMAP §2.1` style)

- **The possession seam:** keep the controller as the sole possessing entity and reserve a `DismountPoint`
  `USceneComponent` on the tank pawn now (it's one more relative-offset point alongside
  `MuzzlePoint`/`GunnerEye` — `KesselTankPawn.h`; near-zero cost).
- **Inventory-as-resource-truth:** when the gun's ammo supply is implemented (M1 step 9), shape it as a
  `UKesselInventoryComponent`-compatible store (or at least a clean ammo-pool seam) so EVA later *fills* it
  rather than forcing a re-plumb. Reserve `EItemClass` and the `FKesselInventoryItem` shape on paper.
- **Crew-state ownership:** put `Cold`/`Morale`/`Wound`/`Stamina` on the persistent owner (controller or a
  `UKesselCrewSubsystem`) when #14 is built (M3), explicitly designed to be read by *either* pawn — so EVA
  adds a cold-ramp, not a parallel model.
- **`Interaction` + soft-target trace channels:** reserve the channel names alongside the M1 `WeaponTrace`
  channel (`MILESTONE_1_PLAN` step 8) — finite named-slot budget, late churn is costly.
- **Surface enum:** the winter `EPhysicalSurface` slots already reserved at M1 (`SIMULATION_ROADMAP §2.1`)
  serve the foot character for free.

### 11.2 The EVA milestone build order (when it lands)

1. **Possession swap (vertical slice):** `AKesselFootCharacter` (movement + FP cam) + `UKesselDismountComponent`
   handshake + foot input context. **Done when:** you climb out at the hatch, walk around the parked tank in
   first person, and climb back in — tank persists and stays vulnerable.
2. **Interaction + loot:** `IKesselInteractable` + `UKesselInteractionComponent` + `AKesselLootCache` +
   `UKesselLootTableAsset`. **Done when:** you search a wreck/container/body and items appear in the satchel.
3. **Inventory + transfer + re-arm:** `UKesselInventoryComponent` (satchel + stowage) + hatch transfer; ferried
   main-gun ammo feeds the gun. **Done when:** looted rounds re-arm the tank across the swap.
4. **The Luger:** `UKesselFirearmComponent` + `UKesselFirearmData` (8-round, soft-only, futile vs armor).
   **Done when:** you can fire/reload, drop a scripted soft threat, and do nothing to a tank.
5. **Exposure + smoking:** `UKesselExposureComponent` cold-ramp/stamina + the smoking interaction +
   `UKesselCigaretteType` (incl. the rare negative spliff) + torch/light-betrays trade. **Done when:** cold
   ramps off-tank, smoking regens, a spliff blurs/slows, and light reads as scatter in the fog.
6. **Risk closure:** unmanned-tank vulnerability + engine-off cold-start + lights-at-night-betray + permadeath
   on foot. **Done when:** the tank can be lost while you're out, and a foot death ends the run.
7. **Loot-site content pass (with the map-blockout team's METHOD):** foot-scale interiors + cache placements at
   handcrafted western-route sites. **Done when:** at least one full site plays the loop end-to-end.

Infantry AI is a parallel later track; the slice above ships against scripted/static threats first (§6).

---

## 12. Bucket / tier summary (roadmap taxonomy)

| EVA sub-system | Bucket | Tier | Milestone |
|---|---|---|---|
| Possession swap / dismount handshake | A | T2 | EVA milestone (reserve seam at M1) |
| Foot character (movement/FP cam) | A→B | T2 | EVA milestone |
| Interaction + loot caches | A→B | T2 | EVA milestone |
| Inventory (satchel + stowage + transfer) | A | T1–T2 | EVA milestone (reserve shape at M1 step 9) |
| Luger (soft-only hitscan firearm) | B | T1–T2 | EVA milestone |
| Off-tank cold exposure + stamina | A→B | T1–T2 | EVA milestone (rides on crew-state #14, M3) |
| Cigarettes / smoking regen / spliff | A→B | T1 | EVA milestone (couples to #14) |
| Torch / light-betrays-at-night | B | T1 | EVA milestone (couples to day/night atmosphere) |
| Unmanned-tank exposure / risk closure | A | T1 | EVA milestone |
| Foot infantry AI threats | A | T3 | parallel later track (scripted first) |
| Foot-scale loot-site content | B/C | T3 | EVA milestone (map-blockout METHOD) |

---

## 13. Cross-system couplings to protect

- **Dismount ↔ the unbutton ladder:** EVA extends `UKesselViewComponent`'s buttoned→unbuttoned→exposed
  escalation into climbing-out; don't build a separate exit path (§2.2).
- **Satchel/stowage ↔ the whole attrition economy:** EVA is the acquisition arm; the gun/engine/track systems
  are the consumption/upgrade arms. One inventory truth (§7).
- **Cold-off-tank ↔ crew-thermal/morale (#14):** same `Cold`/`Morale`/`Wound` model, faster ramp, no cabin
  relief — not a parallel survival meter (§5.2).
- **Light-betrays ↔ Pillar 4 / day-night atmosphere:** torch/lighter/cigarette/tank-lights are one trade —
  sight vs exposure — shared with the headlight system (§5.4).
- **Surface enum ↔ both pawns:** snow/ice/mud computed once (#4), felt by tank and foot (§3.1).
- **Engine-off parking ↔ cold-start (#9) ↔ acoustic beacon (#8):** the parking dilemma is three coupled
  consequences of one choice (§8).
- **Soft-target damage path ↔ kept OUT of the AP/armor ballistics subsystem:** two damage systems, one clean
  seam, never entangled (§6, §10.2).

---

## 14. Open decisions (for the user)

- **EVA-OD-1 — Milestone placement.** Confirm EVA is a **dedicated post-M3 (M4-class) milestone**, with only
  the cheap seams reserved at/near M1 (§11.1). Or is an earlier vertical slice wanted (raising scope
  materially before the tank loop is even feeling-complete)?
- **EVA-OD-2 — Stamina: in or out?** Recommended as a *minimal tension lever* gating the panic-sprint (§5.1),
  explicitly **not** a survival meter. Keep it, or cut to pure cold + carry-weight?
- **EVA-OD-3 — Carry constraint: slots vs weight vs both.** Drives how much ferrying/trip tension exists (§7).
  Recommend a simple **weight cap** (one number, easy to tune) over slot-Tetris.
- **EVA-OD-4 — Infantry threat ambition.** First slice against **scripted/static threats** (recommended) vs
  committing to real foot-enemy AI as part of the EVA milestone (a large parallel track, §6)?
- **EVA-OD-5 — Smoking ownership.** EVA owns acquisition + the smoking interaction; the **regen tuning**
  (how much morale/condition, recovery curves) is crew-state (#14). Confirm that split, or have EVA own the
  full smoking-health model end-to-end?
- **EVA-OD-6 — Re-arm scope.** Does ferried main-gun ammo simply top up the gun's stowed pool (recommended,
  simplest), or is there a per-rack physical-stowage model (couples to cook-off #5/#11 — a side hit on a
  full rack is worse)? The latter is richer but heavier.
- **EVA-OD-7 — Cold-start consequence.** How punishing is the engine-off restart under threat (§8)? A flat
  delay, a failure-chance roll, or full coupling to the #9 thermal/cold-start model (preferred long-term)?
- **EVA-OD-8 — First-person body presence.** Hands-only (cheapest, matches `ART_BIBLE §1`) vs a visible-legs/
  arms FP body for immersion (more rig/anim cost). Recommend **hands/arms-only** for M-EVA, body later.

---

## 15. Cross-references

- Pillars & the "inside the steel" rule EVA excepts: `DESIGN_PILLARS.md §5` (+ §1/§3/§4/§6).
- Core loop, permadeath, scavenged ammo/components, no food/water, huge handcrafted map, environmental
  story: `GAME_DESIGN.md`.
- Possession/input ownership, class-map style, shared-component discipline, data-asset choices, engine-API
  anchors: `ARCHITECTURE.md §2/§6/§10/§14`.
- View-mode unbutton/exposed ladder EVA extends: `KesselViewComponent.h`, `research/07-view-modes-controls.md`.
- Crew-thermal/morale model EVA's cold-exposure rides on; surface enum; cook-off; bucket/tier taxonomy;
  reserve-early discipline: `SIMULATION_ROADMAP.md` (#4, #5/#11, #9, #10, #14; §0/§2/§4/§5).
- Abstracted presentation, emissive-scatter lighting (torch/lighter/muzzle), CRT-green diegetic prompts,
  first-person-only rule: `ART_BIBLE.md §1/§3/§4/§5/§7/§8`.
- Soft-target weapon must stay OUT of: the kinetic AP/armor `UKesselBallisticsSubsystem`
  (`research/04-ballistics-armor.md`, `KesselBallisticsTypes.h`).
- Day/night + dynamic ruin lighting (light-betrays trade): the dynamic-atmosphere team's doc (sibling).
- Loot-site placement is a METHOD for the live editor owner, not actor placement by this team (hard
  constraint): the map-blockout team's doc (sibling).
