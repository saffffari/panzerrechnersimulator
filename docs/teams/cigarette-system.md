# Kessel — CIGARETTE / VICE SYSTEM (design + C++/data architecture)

Status: **DESIGN — post-M1 system, architecture authored now.** No C++ compiled (VS/MSVC gate per
`MILESTONE_1_PLAN §0`). This is the VICE team's spec for the interactive smoking mechanic, the
data-driven cigarette catalog, the reusable timed-effect framework, and the spliff trap. Header
sketches below match the on-disk scaffold conventions (`SCAFFOLD_REPORT.md`) and cite the same
verified UE 5.8 anchors used by the existing code.

Implements the `[[tank-sim-cigarette]]` intent: **smoking is the health-regen ritual**, the glowing
cherry is a warm fog-scatter light AND a night visibility liability, the catalog is fully data-driven
with rarity-tiered loot, and the spliff is a purely-negative curiosity.

---

## 0. How this answers to the pillars

| Pillar | How smoking honors it |
|---|---|
| **1 — Deep sim, abstracted** | Smoking is a *modeled interactive ritual* (light → draw → hold → exhale, timed phases, partial benefit on early stub-out) presented with a few stylized cues (a cherry, a plume, hand silhouette), never a photoreal cig. The benefit is a real timed-effect simulation, not an instant heal. |
| **2 — Crew of one** | Smoking **occupies your hands**. While you smoke you cannot drive, lay the gun, or fire — the loader/gunner/driver is *you*, and you chose to take 20 s off. The friction *is* the mechanic: regen costs exposure and tempo. |
| **3 — Sound is the sensor** | The match strike, the inhale crackle, the exhale — diegetic, spatial. Head-out or on EVA, those sounds carry; a listening enemy can be drawn by a careless smoke. |
| **4 — Cold, dark, few warm lights** | The cherry is a textbook instance of the §4 emissive-scatter rule: a tiny warm ember that brightens on each draw and **scatters into the fog volume** — and at night it is the smallest, most damning light you own. |
| **5 — Always inside the steel** | You can smoke buttoned-up (cherry glows on your face in the dark cabin, the safe way), head-out (warmer, faster calm, but exposed), or **on EVA** (the deliberate exception — most exposed of all). |
| **6 — Historical grounding** | Real period brands and tobaccos (makhorka, papirosa, Juno, Eckstein), grounded but dramatized. Frostbitten men chain-smoked in the *Kessel*; the nicotine calm is the only medicine in a game with no food and no medkit cabinet. |

**No food/water rule preserved:** smoking does **not** model hunger/thirst. It services a single
combined **Condition** pool (physical + nerve), the game's only soft attrition layer beside ammo and
components. It is vice, not survival-needs.

---

## 1. The core fantasy

You have been buttoned in the cold steel for forty minutes. Your hands shake on the gun. The reticle
sways; the screen has crept toward grey. You pull a crushed pack of **makhorka** off a frozen Soviet
gunner you looted two ruins back, thumb a match, and for twenty seconds you do nothing but smoke —
gun cold, tracks still, the cherry throwing a faint amber bloom across the breech in the dark. The
sway settles. The grey pulls back. Then you flick the stub, and you are a crew of one again, warmer,
steadier, and twenty seconds further behind the night.

That is the loop: **Condition decays → you spend a vulnerability window and a consumable to buy it
back → the act itself can betray you.** Risk, reward, and a small human ritual in a dead world.

---

## 2. The Condition model (the "health" smoking regenerates)

This team proposes the **Condition** pool as the health quantity smoking services. (Coordinate with
any future Survival/Health team — this is the canonical proposal until one exists.)

- **`Condition` ∈ [0,1]** — one pool, combined physical warmth + nerve. Authored on
  `UKesselStatusComponent` (§5), serialized into the run save.
- **Drains** (per-second, summed):
  - **Cold** — buttoned with a working heater is slow; head-out faster; **EVA fastest** (you left the
    steel). Blizzard intensity (atmosphere team's weather state) scales it.
  - **Wounds** — a spall graze / non-fatal crew hit (`UKesselArmorComponent` fragment cone, ARCH §4)
    applies a bleeding *debuff effect* that drains Condition over time until it lapses.
  - **Stress** — sustained combat proximity, being under fire, driving blind in whiteout.
- **Restores** — almost only by **smoking** (a `HealthRegen` effect, §5). Regen is **over-time across
  the smoke duration**, not instant: you must survive the ~15–25 s ritual to bank it. Stub out early
  → partial. A warm cabin gives a tiny passive trickle; that is the only non-vice source.
- **Consequences of low Condition** (degrade continuously, no hard cliff until 0):
  - aim sway amplitude up (feeds `UKesselTurretAimComponent` sway), reload cadence slower,
  - spotting/hearing dulled (couples to the audio sensor — pillar 3),
  - screen creeps to desaturated/vignetted (a post-process drive, art team).
  - **Condition → 0 = collapse:** the crewman blacks out. In a permadeath run that is **run-ending**
    (you freeze / are overrun). This makes the regen genuinely load-bearing without inventing
    food/water. Honest scope note: collapse is a *proposed* fail-state; sign-off needed from the
    permadeath/end-state owner.

Condition is **decoupled from instant lethality** — a clean crew penetration still kills outright via
the ballistics path (ARCH §4). Condition is the slow attrition the design wants ("attrition is the
pressure," GAME_DESIGN), not a second hitbar.

---

## 3. The interactive smoking action

A first-person, hands-occupied, interruptible ritual. Owned by **`UKesselSmokingComponent`** (§9.4),
a small state machine driving (a) input gating, (b) the cherry light, (c) effect application, (d)
first-person presentation, (e) audio events.

### 3.1 State machine

```
Idle ──(Smoke pressed, has pack, hands free)──► Lighting
Lighting ──(match flare, ~1.2 s)──► Drawing
Drawing  ──(inhale, cherry brightens, ~1.5 s)──► Holding
Holding  ──(breath held, ~1.0 s)──► Exhaling
Exhaling ──(plume, ~2.0 s)──► Drawing      (loop N draws)
Drawing/Holding/Exhaling ──(Smoke released OR Cancel)──► Stubbing
…after Draw N or stub ──► Stubbing ──(flick, ~0.6 s)──► Idle
```

- **Hold-to-smoke** (preferred): hold the bind to keep drawing; release to stub. Or a one-press
  "smoke the whole cigarette" auto-loop you can cancel. Decide in playtest (OQ-1).
- **Phase timings** live on the data asset (§9.1 `FKesselSmokeTiming`), not hardcoded — different
  cigarettes burn at different speeds (a quick papirosa vs. a slow officer cigar).

### 3.2 Phase timing table (authoring defaults, per draw)

| Phase | Default s | What happens |
|---|---|---|
| Lighting | 1.2 | match/lighter flare (a brief *bright* warm point light, separate from the cherry), strike SFX, hands raise. One-time, front of the ritual. |
| Drawing (inhale) | 1.5 | cherry **brightens** (intensity + radius spike, §4); inhale crackle SFX; effect *charge* accrues. |
| Holding | 1.0 | cherry dims to ember; breath held; brief stillness. |
| Exhaling | 2.0 | smoke-plume Niagara; exhale SFX; in cold, a visible breath-and-smoke mix. |
| (loop) | — | 3–6 draws typical (`DrawCount` on the asset). |
| Stubbing | 0.6 | flick/grind; cherry extinguishes; hands lower. |

Full cigarette ≈ `Lighting + DrawCount·(Draw+Hold+Exhale) + Stub` ≈ 15–30 s. **Long on purpose** —
the time-cost is the balancing knob.

### 3.3 Input gating & the crew-of-one cost

On entering any non-Idle state the component **suppresses** the drive / fire / aim mapping contexts
(coordinate with `AKesselPlayerController`, the sole `AddMappingContext` owner — SCAFFOLD §1) and
shows a "stow weapon / smoke" hand layer. You are **defenceless and motionless** while smoking.
Taking a hit, a loud nearby threat, or pressing Cancel triggers an instant **panic-stub** (skip to
Stubbing, partial benefit). This is the deliberate tension: the calm you need is the calm you can't
afford.

### 3.4 Benefit timing (early-out is honest)

Effects are **granted per completed draw**, not all at the end — each finished Exhale commits its
share of the cigarette's effect specs to the status component (§5). Stub at draw 2 of 5 → you banked
~40%. No exploit where you tap the button for full value; no punishment for a forced panic-stub
beyond the lost remainder.

### 3.5 First-person presentation (abstracted)

Per pillar 1 + ART_BIBLE §1 (hands appear only during interactions): a **gloved-hand layer** raises a
stylized cigarette into the lower frame; the hero asset is the *light*, not the model. Buttoned: the
cherry rim-lights the breech/face in near-black (ART_BIBLE §4d, the single-emissive-keys-the-cabin
look). Head-out / EVA: plume drifts into the blizzard. Keep geometry cheap — this sells on light,
smoke, and timing, exactly the abstraction the pillars want.

---

## 4. The cherry — warm scatter light AND night liability

The cigarette's lit tip is a real, tiny **`UPointLightComponent`** (the "Cherry"), attached to a
mouth/hand socket. It is the smallest expression of the §4 lighting philosophy and a gameplay signal.

### 4.1 As emissive fog-scatter (pillar 4 / ART_BIBLE §4b)

- Color ember orange-red ≈ `#FF5A1E`→`#B5300E` (ART_BIBLE warm-accent swatches).
- Small `AttenuationRadius` (e.g. 60–120 uu), low base `Intensity`, **high
  `VolumetricScatteringIntensity`** (`LightComponentBase.h:120`, the same property the muzzle/cabin
  lights use, ARCH §8) so it smears a warm core into the cool fog albedo.
- **Pulses with the draw:** on Inhale, intensity + radius spike (the tip glows hot as you pull); on
  Hold/Exhale it falls back to a dim ember. A small `FInterpTo`/curve drives this each tick. A
  flicker noise keeps it alive. `Movable` mobility (dynamic, fog-affecting).

### 4.2 As a detectability liability (the cost of warmth)

The cherry is, at night, **the brightest thing you emit when everything else is dark and cold.**
- It exposes a **`Luminosity` signal** to the world's detection layer (couple to the
  enemy/atmosphere visibility system — the same channel headlights/cabin light feed). Smoking
  head-out or on EVA at night raises your detectability; buttoned, the hull occludes most of it.
- Day/blizzard whiteout largely washes it out (low marginal risk); clear night with the lights off
  makes it a genuine tell. This makes **when and where you smoke** a real decision, mirroring the
  "your lights save you AND betray you" directive at the smallest scale.
- The match/lighter **flare** at `Lighting` is a brief, *much* brighter spike — the single most
  visible instant of the whole ritual. Lighting up in the open at night is the riskiest moment.

Exposing a scalar `GetCherryLuminosity()` (0 when Idle, peak on Inhale, mid on ember) lets the
detection system sample it without knowing anything about smoking.

---

## 5. The generic EFFECT framework (reused by all consumables)

The heart of the data design: **buffs and debuffs are the same data.** A cigarette grants a list of
`FKesselEffectSpec`; a spliff grants a list of `FKesselEffectSpec` that happen to all be negative; a
future flask, pep-pill, or bandage reuses the identical types. Nothing about the framework knows what
a cigarette is.

### 5.1 `EKesselEffectType` (what a stat modifier touches)

| Value | Effect | Sign used by |
|---|---|---|
| `HealthRegen` | restores Condition over time (regen-per-second) | cigarettes (core) |
| `AimSteady` | reduces turret/gun sway amplitude | "steady hand" boosts |
| `ColdResist` | slows the Cold drain on Condition (brief warmth) | warming boosts |
| `Calm` / `Morale` | reduces the Stress drain; dampens low-Condition vignette | calming boosts |
| `ReloadSpeed` | scales reload cadence | rare stimulant |
| `MoveSpeed` | scales EVA on-foot move speed | **spliff (negative)** |
| `VisionClarity` | post-process blur/narrowing (negative = blurred/tunnel vision) | **spliff (negative)** |
| `Detectability` | modifies emitted luminosity/noise | (situational) |

Same enum drives buffs (positive magnitude) and debuffs (negative). A spliff is just negative
`VisionClarity` + negative `MoveSpeed` with **no `HealthRegen` entry at all**.

### 5.2 `FKesselEffectSpec` (authored, on the data asset)

```cpp
USTRUCT(BlueprintType)
struct FKesselEffectSpec
{
    GENERATED_BODY()

    /** Which stat/system this effect modifies. */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect")
    EKesselEffectType Type = EKesselEffectType::HealthRegen;

    /** Signed magnitude. Meaning depends on Type: regen/s, sway-scale delta, speed-scale
     *  delta, etc. NEGATIVE = debuff (the spliff uses only negatives). */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect")
    float Magnitude = 0.0f;

    /** Seconds the effect persists after it is applied. 0 = instant/one-shot. */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect", meta = (ClampMin = "0.0"))
    float Duration = 30.0f;

    /** For regen-style effects: apply Magnitude per second across Duration; for modifier-style
     *  effects: hold a flat Magnitude for Duration then drop. */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect")
    bool bIsPerSecond = false;

    /** Optional shaping over the effect's lifetime (e.g. nicotine "hit then fade"). X in [0,1]
     *  = normalized elapsed; Y scales Magnitude. Empty = flat. */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect")
    FRuntimeFloatCurve Falloff;

    /** How a second instance of the same Type combines with one already active. */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Effect")
    EKesselEffectStack Stacking = EKesselEffectStack::RefreshDuration;
};

UENUM(BlueprintType)
enum class EKesselEffectStack : uint8
{
    RefreshDuration,   // re-apply resets the timer, magnitude unchanged (anti-chain-smoke)
    AddMagnitude,      // stack stronger (capped)
    IgnoreIfActive,    // can't re-up while active
    Replace            // newest wins
};
```

`RefreshDuration` (default) plus a per-type cap on the status component is the **anti-abuse knob**:
chain-smoking refreshes calm but cannot stack Condition regen to infinity, and a nicotine
**tolerance** debuff can be layered later (smoke too often → diminishing `HealthRegen`, a small green
"overdose" malus) — all expressible as more `FKesselEffectSpec`s, no new code.

### 5.3 The status/effect component

**`UKesselStatusComponent : UActorComponent`** on the player pawn (and reusable on crew/EVA pawn). It
is the single sink for every timed effect in the game.

- Owns `Condition` and the runtime list of `FKesselActiveEffect` (spec + remaining time + source).
- `ApplyEffect(const FKesselEffectSpec&, UObject* Source)` / `ApplyEffects(TArray<…>)` — the public
  entry any consumable calls.
- `TickComponent` decrements timers, integrates `HealthRegen`/`bIsPerSecond` into `Condition`,
  applies the cold/wound/stress **drains**, and recomputes cached aggregate modifiers.
- **Queryable aggregates** consumed by other systems (they never see "cigarette," only a number):
  `GetConditionalAimScale()`, `GetMoveSpeedScale()`, `GetVisionClarity()`, `GetColdResistScale()`,
  `GetCalmScale()`, plus `GetCondition()`. `UKesselTurretAimComponent` multiplies sway by the aim
  scale; the EVA move component multiplies speed by the move scale; the post-process reads vision
  clarity; the Condition drain reads cold-resist/calm.

This is the reuse story: smoking, the spliff, a future bandage, and an environmental cold source all
push `FKesselEffectSpec`s into the same component and the same query surface.

---

## 6. The cigarette catalog (fully data-driven)

### 6.1 `UCigaretteData : UPrimaryDataAsset`

One asset = one cigarette/tobacco type. (Sibling of `UAtmospherePreset`/`UTankSightProfile` —
`UPrimaryDataAsset` for AssetManager registration + async load + per-asset editing + the nested
effect array; same rationale as ARCH §6. Base verified `Engine/DataAsset.h:47`.) Full header in §9.1.
It carries: identity (name, faction, flavor text), **rarity tier**, **loot-source tags**, the
**`FKesselSmokeTiming`** burn profile + `DrawCount`, the **`FKesselCherryParams`** light recipe, the
**`TArray<FKesselEffectSpec> Effects`** granted, smoke/SFX refs, and pack/stack metadata.

### 6.2 Rarity tiers

```cpp
UENUM(BlueprintType)
enum class EKesselRarity : uint8
{
    Common,        // everywhere — the bread-and-butter calm
    Uncommon,      // standard issue, decent regen
    Rare,          // special-boost variants (steady aim / warmth / morale)
    VeryRare,      // strong, multi-effect officer stock
    ExtremelyRare  // the spliff, and one-of curios
};
```

### 6.3 Faction / flavor

```cpp
UENUM(BlueprintType)
enum class EKesselCigaretteFaction : uint8
{
    Soviet,    // makhorka, papirosa
    German,    // Juno, Eckstein, R6
    Special,   // officer / medical / unknown caches
    Unknown    // the spliff masquerades here
};
```

### 6.4 The catalog (authoring intent — each is one `UCigaretteData` asset)

| Asset (DA_Cig_*) | Faction | Rarity | Burn | Core effects (signed) | Found in (EVA) |
|---|---|---|---|---|---|
| `Makhorka` (loose newspaper roll) | Soviet | Common | slow, harsh | `HealthRegen +`, small `Calm +` | Soviet bodies, trench positions, ruined izbas |
| `Papirosa` (Belomorkanal-style, cardboard tube) | Soviet | Common | fast, few draws | `HealthRegen +` (quick, lower total) | Soviet bodies, supply dumps |
| `Juno` | German | Uncommon | medium | `HealthRegen ++` | German dead, your own column wrecks, packs in the cab |
| `Eckstein No.5` | German | Uncommon | medium | `HealthRegen ++`, slight `Calm +` | German positions, depots |
| `Steady-hand stash` (e.g. dry Sturm cigarette) | Special | Rare | medium | `HealthRegen +`, **`AimSteady +`** (~30 s) | officer kit, command bunkers |
| `Warming shag` (strong dark tobacco) | Special | Rare | slow | `HealthRegen +`, **`ColdResist +`** (brief warmth vs the cold drain) | field-medical caches, frozen aid posts |
| `Morale ration cigarette` | Special | Rare | medium | `HealthRegen +`, **`Calm/Morale ++`** (stress drain cut, vignette eased) | morale-ration crates, mail sacks |
| `Officer's cigar / Pervitin-laced` | Special | VeryRare | long | `HealthRegen ++`, `AimSteady +`, `ReloadSpeed +`, **+ small tolerance debuff** | rare officer caches only |
| `The spliff` (§7) | Unknown | ExtremelyRare | erratic | **VisionClarity −, MoveSpeed − only — NO regen** | extremely rare, mislabeled curio |

All numbers are authoring intent; tuned as data without code. New brands = new assets, zero code.

---

## 7. The SPLIFF — purely negative, a trap and a curiosity

The spliff needs **no special code path**. It is a `UCigaretteData` with:

- `Faction = Unknown`, `Rarity = ExtremelyRare`, ominous/ambiguous flavor text (it does not announce
  itself — it looks like a fat hand-rolled cigarette, a curiosity off a strange corpse).
- `Effects = { VisionClarity −0.6 (Duration ~45 s, Falloff easing in then out),
  MoveSpeed −0.35 (Duration ~45 s) }`. **No `HealthRegen`. No positive entry of any kind.** Condition
  is not restored; you spent a smoke window and got *worse*.
- A heavier, lower-pulsing cherry and a thicker, slower plume so it *reads* different after the fact.

Mechanically: blurred + tunnel-narrowed vision (post-process driven by `GetVisionClarity()` going
negative) and slowed movement (EVA move scale < 1) for the duration — most punishing **on EVA**,
where slow + half-blind in the open is genuinely dangerous. It is a deliberate **rug-pull**: extremely
rare, no upside, a story the player tells once. The fact that the catalog expresses both the best
cigarette and the worst with the *same struct* is the proof the effect framework is right.

(One green flourish: because it's pure data, a "is this a spliff?" can stay hidden until smoked —
ambiguous naming + no preview of effects for `Unknown`-faction items — turning identification into
its own small risk.)

---

## 8. EVA loot economy integration

Cigarettes are the soft consumable that **motivates dismounting** (the EVA team's exposure loop):
leaving the warm steel to scavenge ruins, the only weapon a Luger, the tank sitting exposed behind
you — and one of the prizes is the medicine that keeps you upright.

- **Carried as a stack:** `FKesselConsumableStack { TObjectPtr<UCigaretteData>, int32 Count }` in the
  shared EVA inventory (coordinate with the EVA/inventory team — this team defines the *item type*,
  they define the *container*). A pack is N cigarettes; smoking one decrements `Count`.
- **Loot placement by tags:** each `UCigaretteData` carries `LootSourceTags`
  (`Soviet.Body`, `German.Body`, `Depot`, `OfficerCache`, `MedicalCache`, `MailSack`, …) and its
  `Rarity`. The EVA loot-table roller weights spawns by rarity and matches source tags to the ruin /
  corpse archetype — Soviet dead drop makhorka/papirosa, your own burnt column drops Juno, officer
  bunkers gate the rare boosts, the spliff sits at the bottom of the rarity weight almost nowhere.
- **Found, never bought** — consistent with "upgrades and ammo are scavenged" (GAME_DESIGN). Smoking
  is part of the attrition economy: a finite, lootable supply of calm you spend down across the run.
- **The exposure tradeoff is doubled:** you risk the tank to gather cigarettes, then risk *another*
  vulnerability window to smoke them — and if you smoke in the open at night, the cherry can call the
  thing that finds your unmanned tank. Three nested risk/reward decisions on one vice.

---

## 9. C++ / data architecture (header sketches)

Conventions match `SCAFFOLD_REPORT.md`: `// Copyright Kessel.` banner, `CoreMinimal.h` first, base
header, own `*.generated.h` last; `KESSEL_API` on cross-file types; `Category = "Kessel|…"`;
`TObjectPtr` members; verified-anchor citations in comments. New files would live under
`Source/Kessel/Public|Private/` and the module would re-enable `Niagara`/`UMG` deps (already deferred
in `Kessel.Build.cs`) when this lands.

### 9.1 `KesselCigaretteData.h` — the data asset

```cpp
// Copyright Kessel. All Rights Reserved.
#pragma once

#include "CoreMinimal.h"
#include "Engine/DataAsset.h"
#include "Curves/CurveFloat.h"              // FRuntimeFloatCurve (held by value in FKesselEffectSpec)
#include "KesselEffectTypes.h"              // FKesselEffectSpec, EKessel* enums (shared, §9.2)
#include "KesselCigaretteData.generated.h"

class UNiagaraSystem;
class USoundBase;
class UTexture2D;

/** Per-draw burn timing; lets a fast papirosa and a slow cigar use the same code. */
USTRUCT(BlueprintType)
struct FKesselSmokeTiming
{
    GENERATED_BODY()
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Smoke") float LightSeconds   = 1.2f;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Smoke") float DrawSeconds    = 1.5f;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Smoke") float HoldSeconds    = 1.0f;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Smoke") float ExhaleSeconds  = 2.0f;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Smoke") float StubSeconds    = 0.6f;
};

/** Cherry point-light recipe (the §4 emissive-scatter ember). */
USTRUCT(BlueprintType)
struct FKesselCherryParams
{
    GENERATED_BODY()
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cherry") FLinearColor Color = FLinearColor(1.0f, 0.35f, 0.12f);
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cherry") float EmberIntensity = 0.4f;   // between draws
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cherry") float DrawIntensity  = 2.5f;   // peak on inhale
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cherry") float AttenuationRadius = 90.0f;
    /** High value → strong warm core melting into the cool fog (LightComponentBase.h:120). */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cherry") float VolumetricScattering = 6.0f;
};

/**
 * UCigaretteData — one cigarette/tobacco type, fully data-driven.
 * Verified base: UPrimaryDataAsset : UDataAsset
 *   C:/Program Files/Epic Games/UE_5.8/.../Engine/Classes/Engine/DataAsset.h:47
 * (PrimaryDataAsset chosen for the nested Effects array + AssetManager/async load, ARCH §6.)
 */
UCLASS(BlueprintType)
class KESSEL_API UCigaretteData : public UPrimaryDataAsset
{
    GENERATED_BODY()
public:
    // --- Identity ---
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Id") FText DisplayName;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Id") FText FlavorText;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Id") EKesselCigaretteFaction Faction = EKesselCigaretteFaction::Soviet;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Id") EKesselRarity Rarity = EKesselRarity::Common;
    /** If true, hide effects/identity in UI until first smoked (the spliff trick). */
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Id") bool bIdentityHidden = false;

    // --- The effect payload (THE data-driven heart; spliff = all-negative, no regen) ---
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Effects") TArray<FKesselEffectSpec> Effects;

    // --- Ritual ---
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Smoke", meta=(ClampMin="1")) int32 DrawCount = 4;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Smoke") FKesselSmokeTiming Timing;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Smoke") FKesselCherryParams Cherry;

    // --- Presentation ---
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|FX") TObjectPtr<UNiagaraSystem> SmokePlume;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|FX") TObjectPtr<USoundBase> LightSound;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|FX") TObjectPtr<USoundBase> DrawSound;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|FX") TObjectPtr<USoundBase> ExhaleSound;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Ui")  TObjectPtr<UTexture2D> Icon;

    // --- Loot economy (EVA team consumes these) ---
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Loot") TArray<FName> LootSourceTags;
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category = "Cigarette|Loot", meta=(ClampMin="1")) int32 PackSize = 10;
};
```

### 9.2 `KesselEffectTypes.h` — the reusable framework (shared, no cigarette knowledge)

Holds `EKesselEffectType`, `EKesselEffectStack`, `EKesselRarity`, `EKesselCigaretteFaction`, and
`FKesselEffectSpec` (§5.1–5.2) plus the runtime `FKesselActiveEffect { FKesselEffectSpec Spec; float
Elapsed; TWeakObjectPtr<UObject> Source; }`. This is the analog of `KesselBallisticsTypes.h` — a
shared type header included by the data asset, the status component, and any future consumable.

### 9.3 `KesselStatusComponent.h` — the effect sink + Condition pool

```cpp
// Copyright Kessel. All Rights Reserved.
#pragma once
#include "CoreMinimal.h"
#include "Components/ActorComponent.h"       // UActorComponent : UObject  (ActorComponent.h:160)
#include "KesselEffectTypes.h"
#include "KesselStatusComponent.generated.h"

/**
 * UKesselStatusComponent — single sink for every timed effect + the Condition (health) pool.
 * Reused by any consumable (cigarettes, spliff, future bandage) and by environmental drains.
 */
UCLASS(ClassGroup=(Kessel), meta=(BlueprintSpawnableComponent))
class KESSEL_API UKesselStatusComponent : public UActorComponent
{
    GENERATED_BODY()
public:
    UKesselStatusComponent();
    virtual void TickComponent(float Dt, ELevelTick, FActorComponentTickFunction*) override;

    /** Public entry every consumable calls. */
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") void ApplyEffect(const FKesselEffectSpec& Spec, UObject* Source);
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") void ApplyEffects(const TArray<FKesselEffectSpec>& Specs, UObject* Source);

    // Aggregated queries — other systems read these, never the source.
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetCondition() const { return Condition; }
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetConditionalAimScale() const;   // → UKesselTurretAimComponent sway
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetMoveSpeedScale() const;        // → EVA move
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetVisionClarity() const;         // → post-process (spliff blur)
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetColdResistScale() const;       // → Condition cold drain
    UFUNCTION(BlueprintCallable, Category="Kessel|Status") float GetCalmScale() const;             // → Condition stress drain

protected:
    UPROPERTY(EditAnywhere, BlueprintReadOnly, Category="Kessel|Status", meta=(ClampMin="0.0", ClampMax="1.0"))
    float Condition = 1.0f;

    /** Per-type cap so chain-smoking can't stack regen to infinity (§5.2). */
    UPROPERTY(EditAnywhere, Category="Kessel|Status") float MaxConditionRegenPerSec = 0.05f;

    UPROPERTY() TArray<FKesselActiveEffect> ActiveEffects;
    // cached aggregates recomputed each tick…
};
```

### 9.4 `KesselSmokingComponent.h` — the interaction state machine

```cpp
// Copyright Kessel. All Rights Reserved.
#pragma once
#include "CoreMinimal.h"
#include "Components/ActorComponent.h"
#include "KesselSmokingComponent.generated.h"

class UCigaretteData;
class UKesselStatusComponent;
class UPointLightComponent;       // the Cherry  (Components/PointLightComponent.h)
class UNiagaraComponent;

UENUM(BlueprintType)
enum class EKesselSmokeState : uint8 { Idle, Lighting, Drawing, Holding, Exhaling, Stubbing };

/**
 * UKesselSmokingComponent — drives the light→draw→hold→exhale→stub ritual:
 *  - gates drive/fire/aim input while smoking (crew-of-one cost),
 *  - pulses the Cherry UPointLightComponent (§4) and the smoke plume,
 *  - commits each completed draw's FKesselEffectSpecs into UKesselStatusComponent (§3.4),
 *  - exposes GetCherryLuminosity() to the detection layer (§4.2).
 */
UCLASS(ClassGroup=(Kessel), meta=(BlueprintSpawnableComponent))
class KESSEL_API UKesselSmokingComponent : public UActorComponent
{
    GENERATED_BODY()
public:
    UKesselSmokingComponent();
    virtual void TickComponent(float Dt, ELevelTick, FActorComponentTickFunction*) override;

    /** Begin smoking the given cigarette from the inventory stack (decrements on light). */
    UFUNCTION(BlueprintCallable, Category="Kessel|Smoke") bool BeginSmoke(UCigaretteData* Cigarette);
    /** Stub out early (release or panic). Banks only completed draws. */
    UFUNCTION(BlueprintCallable, Category="Kessel|Smoke") void StubOut(bool bPanic);

    /** 0 when Idle; peaks on inhale; mid on ember. Sampled by the world detection system. */
    UFUNCTION(BlueprintCallable, Category="Kessel|Smoke") float GetCherryLuminosity() const;

    UFUNCTION(BlueprintCallable, Category="Kessel|Smoke") EKesselSmokeState GetState() const { return State; }

protected:
    UPROPERTY(Transient) EKesselSmokeState State = EKesselSmokeState::Idle;
    UPROPERTY(Transient) TObjectPtr<UCigaretteData> Current = nullptr;
    UPROPERTY(Transient) int32 DrawsCompleted = 0;

    UPROPERTY() TObjectPtr<UPointLightComponent> Cherry = nullptr;   // attached to mouth/hand socket
    UPROPERTY() TObjectPtr<UNiagaraComponent>    Plume  = nullptr;
    UPROPERTY() TObjectPtr<UKesselStatusComponent> Status = nullptr; // resolved from owner

    // phase timer; on each Exhale→Status->ApplyEffects(scaled share); panic-stub on hit/threat…
};
```

### 9.5 Wiring summary

```
EVA inventory (FKesselConsumableStack) ──Smoke bind──► UKesselSmokingComponent.BeginSmoke(UCigaretteData*)
   │ decrement Count                                       │ gate input (via PlayerController IMCs)
   │                                                        │ spawn/pulse Cherry UPointLightComponent ──► fog scatter (ART_BIBLE §4)
   │                                                        │                                         └─► GetCherryLuminosity() ──► detection layer (night liability)
   │                                                        │ per completed draw:
   │                                                        ▼
   │                                  UKesselStatusComponent.ApplyEffects(Cigarette.Effects)
   │                                       │ HealthRegen → Condition ; AimSteady/ColdResist/Calm/Vision/Move modifiers
   │                                       ▼
   │             aggregates queried by: UKesselTurretAimComponent (sway), EVA move (speed),
   │                                    post-process (vision), Condition drains (cold/stress)
   └─ loot placement: UCigaretteData.LootSourceTags + Rarity ──► EVA loot-table roller
```

---

## 10. Audio (pillar 3 hooks)

Per `AUDIO_ARCHITECTURE.md` the ritual is a small spatial event chain: **match strike** (sharp,
bright — the loudest beat), **inhale crackle** (close, intimate when buttoned), **exhale** (soft,
with a cold-breath layer), **stub flick**. Buttoned the hull occludes them; head-out / EVA they
propagate and can draw a listening threat — sound and the cherry are *the same liability expressed in
two senses*. Route through `UKesselAudioComponent` (the placeholder seam, SCAFFOLD §1) when the audio
pass lands.

---

## 11. Scope — M1 vs later (honest)

**Not in M1.** M1's done-when is "drivable + shooting tank in volumetric fog" (MILESTONE_1_PLAN);
smoking, Condition, EVA, and the catalog are explicitly post-M1. Nothing here blocks the M1 critical
path and nothing here should be built before it.

**Smallest first vertical slice (post-M1, before the full system):**
1. `KesselEffectTypes.h` + `UKesselStatusComponent` with just `Condition` + `HealthRegen` +
   one cold drain. (No cigarettes yet — proves the pool.)
2. `UCigaretteData` + **one** asset (`DA_Cig_Makhorka`) + `UKesselSmokingComponent` with a single
   draw and the Cherry `UPointLightComponent` scattering into the existing fog. Bind one key.
   *Done-when:* hold the key → cherry blooms in the fog, Condition ticks up, input is gated.
3. Add the rarity/faction/loot fields and 4–5 more catalog assets; hand them to the EVA team's loot
   roller.
4. Add `AimSteady`/`ColdResist`/`Calm` modifiers + wire the aim/post-process/drain consumers.
5. Add the spliff asset (pure-negative) + the `Detectability`/night-liability coupling.
6. First-person hand animation layer + full audio chain (polish).

**Deliberately later / out of scope:** bespoke per-brand hand animations, smoke-fluid-sim, nicotine
tolerance economy depth, and any third-person view of smoking (forbidden by pillar 5 regardless).

---

## 12. Open questions / coordination

- **OQ-1** Hold-to-smoke vs press-to-auto-smoke-with-cancel? (playtest the feel of the time-cost.)
- **OQ-2** Condition→0 = run-ending collapse — needs sign-off from the **permadeath/end-state owner**;
  alternative is a deep but non-lethal floor with a long penalty.
- **OQ-3** Inventory container shape owned by the **EVA/inventory team**; this team owns only the item
  type (`UCigaretteData`) and the stack struct.
- **OQ-4** Detectability units — needs the **enemy/atmosphere visibility** channel definition so the
  cherry's `GetCherryLuminosity()` plugs into the same scale as headlights/cabin light.
- **OQ-5** Should buttoned smoking be safer *and* less effective (less calm without the cold air) to
  push the head-out/EVA risk tradeoff? (data-tunable either way.)

---

## 13. Cross-references
- Pillars: `docs/DESIGN_PILLARS.md` (esp. 1 sim/abstract, 2 crew-of-one, 3 sound, 4 light, 5 inside-the-steel).
- Vision/economy: `docs/GAME_DESIGN.md` (no food/water; scavenged attrition).
- Light/scatter art intent: `docs/ART_BIBLE.md` §4 (cherry = emissive-into-fog), §1 (hands-only FP).
- Existing types/conventions: `docs/SCAFFOLD_REPORT.md`, `Source/Kessel/Public/AtmospherePreset.h`,
  `KesselSightProfile.h`, `KesselArmorComponent.h`, `KesselBallisticsTypes.h`.
- Engine anchors (verified, reused): `Engine/DataAsset.h:47` (UPrimaryDataAsset),
  `Components/ActorComponent.h:160` (UActorComponent), `LightComponentBase.h:120`
  (`VolumetricScatteringIntensity`), `Components/PointLightComponent.h` (the Cherry).
- Coordinates with: EVA/loot team (inventory + loot tables), Atmosphere/weather team (cold drain,
  night detectability), Audio (`docs/AUDIO_ARCHITECTURE.md`), and a future Survival/Health owner.
</content>
</invoke>
