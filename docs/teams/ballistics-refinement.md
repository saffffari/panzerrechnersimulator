# Ballistics & Armor — Refinement & Implementation Draft (M1)

Status: **DESIGN + C++ DRAFT** (no compile — VS/MSVC absent per MILESTONE_1_PLAN §4). Engine: UE 5.8.0.
Owner: Ballistics & Armor team. Consumed by: build step 7 (Claude integrates the `.cpp` into `Source/`).
Scope: finalize the kinetic AP/APCBC penetration/deflection model, fix the three calibration defects
(R-B1 angled-pen over-prediction, R-B2 `DragCoef k` misfit, R-B3 85–90° false-pen window), supply
**period-correct** data for the Stalingrad-pocket cast (player **Tiger I 8.8 cm KwK 36**; opponents
**T-34/76 with the 76 mm F-34** — *not* the /85 — and **KV-1**), the data schema (DataTable shell rows +
per-vehicle armor `DataAsset` incl. ammo-rack/crew modules), and the drop-in body for
`UKesselBallisticsSubsystem::ResolveKineticHit`, plus the corrected duel unit tests.

This doc is grounded on `docs/research/04-ballistics-armor.md` and `docs/MILESTONE_1_PLAN.md` §2/§5/§6
(R-B1..R-B4, OD-1, OD-5). It does **not** contradict the pillars: deep-sim-but-abstracted, crew-of-1,
permadeath (a confirmed solid crew hit on the player = abort/death), sound-as-sensor (the report/impact
this model raises is the audio team's trigger). It does **not** touch the live editor (DataTable/DataAsset
rows below are authoring specs, not actor placement).

> **Period note.** Stalingrad pocket = Nov 1942 – Feb 1943. The opponents are the **T-34 Model 1942/43
> (76 mm F-34)** and the **KV-1 Model 1942** — both firing the same 76 mm BR-350A. The T-34-85 (research
> §13) is **1944** and is hereby **retired from M1 data**; its schema row is kept only as a future entry.
> The Tiger-at-Stalingrad is the game's established fiction (the 8.8 cm KwK 36 / PzGr. 39 is correct).

---

## 0. What changed vs research/04 (one-screen diff)

| Item | research/04 (old) | This doc (corrected) | Why |
|---|---|---|---|
| **Angle model** | `LOS = T / cos(theta_eff)` (geometric only) | `LOS = T / cos(theta_eff)^SlopeExp`, `SlopeExp≈2.15` | R-B1. Pure `1/cos` over-predicts angled pen by ~12% (defeats 142 mm where the historical 30° figure is 127 mm). The exponent reproduces **both** the 0° **and** 30° historical columns exactly while keeping the honest 157 mm normal-incidence value. |
| **`DragCoef k`** | 88: 1.05e-4, "fits velocity" | 88: **1.58e-4**, 76: **2.54e-4**, fit to the **pen** table | R-B2. Old `k` matched ~10 %/km *velocity* loss, not the steeper *pen* falloff → long-range/de-Marre extrapolation overstated pen. |
| **Grazing clamp** | `THETA_MAX=85°`, `COS_MIN=cos85=0.0436` | `LosClampDeg=89°`, `CosMin=cos89=0.01745` **+** guaranteed ricochet ≥ `GrazeRicochetDeg=88°` for non-overmatched plate | R-B3. Old clamp capped grazing LOS at 11.5·T (≈2.5× too thin) → 85–89.9° grazes false-penned. |
| **Opponents** | T-34-85 (85 mm ZiS-S-53) | **T-34/76 (76 mm F-34)** + **KV-1** | Period correctness (1942/43, not 1944). |
| **Config** | `ThetaMaxDeg=60`, `CosMin=0.1` (mislabeled) | New explicit fields (§3.4); old two retuned | Make the two clamps consistent & named for what they do. |

The **structs in `KesselBallisticsTypes.h` do not change** (the scaffold schema is good). Only
`UBallisticsConfig` gains fields and the subsystem body lands. Authoring can proceed immediately.

---

## 1. The corrected model (final spec)

Angle convention unchanged from research §1: **`theta_n` = impact angle from the surface normal**, degrees.
`0°` = perpendicular (thinnest path, best pen); `90°` = grazing (infinite path, always deflect). Historical
penetration tables are quoted as **plate slope from vertical**; for a level shot the two coincide, so the
German "30°" column is our `theta_n = 30°` test (the calibration target below).

### 1.1 Pipeline (per impact) — order is load-bearing
```
ResolveKineticHit(Shell, Section, ImpactDir, SurfaceNormal, Range):
  1.  cosImpact = dot( normalize(-ImpactDir), normalize(SurfaceNormal) )   // clamp [-1,1]
        if cosImpact <= KINDA_SMALL → back-face / parallel: treat as no-hit (safe default)
  2.  theta_n   = degrees( acos(cosImpact) )
  3.  T         = max(Section.Thickness_mm, Config.MinPlateThickness_mm)
  4.  P0        = PenetrationAtRange(Shell, Range)            // §1.3 tabulated lerp + de-Marre tail
  5.  overmatch = Shell.Caliber_mm / T
  6.  normEff   = Shell.NormalizationAngle
                  + clamp((overmatch - 1) * Config.NormOvermatchGain, 0, Config.NormMaxBonus)
  7.  ----- RICOCHET (before the thickness compare) -----
        if overmatch < Config.OvermatchNoRicochet:           // big shell vs thin plate never skips
            if theta_n >= Config.GrazeRicochetDeg            → RICOCHET (certain; closes R-B3)
            elif theta_n >  Shell.RicochetAngle:
                pRic = (theta_n - Shell.RicochetAngle) / (90 - Shell.RicochetAngle)   // linear ramp→1 @90
                if RandomStream.FRand() < pRic               → RICOCHET
  8.  theta_eff = clamp( max(theta_n - normEff, 0), 0, Config.LosClampDeg )
  9.  c         = max( cos(radians(theta_eff)), Config.CosMin )         // never ~0  (R-B3 seatbelt)
 10.  LOS       = T / pow(c, Config.SlopeExp)                           // R-B1 angle-hardening
 11.  if P0 >= LOS:  PEN.  residualFrac = clamp((P0 - LOS)/P0, 0, 1)
                     spallPower  = residualFrac * Shell.Caliber_mm * Config.SpallScale
      else:          NON-PEN (residualFrac=0, spallPower=0; optional thin-plate spall band deferred)
```

Three properties that make this honest:
- **Reproduces the historical 0° column** — at `theta_n=0`, `theta_eff=0`, `LOS=T`, so the gun defeats
  exactly its tabulated normal pen (88: 157 mm @ 100 m). No double-count.
- **Reproduces the historical 30° column** — see §1.2 fit; `SlopeExp` is *fit to that column*, so a 30°
  shot lands on 127/117/106 mm for the 88. The model is anchored at both ends of the only two
  experimentally-quoted angles.
- **Grazing is genuinely safe** — at `theta_n→89°`, `c=CosMin=0.0175`, `pow(c,2.15)=1.5e-4`, so
  `LOS≈6800·T`. No false pen, and for non-overmatched plate the shot has already certain-ricocheted at 88°.

### 1.2 R-B1 fix — `SlopeExp` derivation (kept honest at 0° AND 30°)

We require the model, fed the historical **0° pen as `P0`**, to *just defeat* the historical **30° pen
thickness** at `theta_n=30°`. For the 88 mm PzGr. 39 (APCBC, normalization 5°), overmatch vs a ~127 mm
plate is `88/127=0.69 < 1` so the overmatch bonus is 0 → `normEff=5°` → `theta_eff=25°`:

```
LOS(127, 25°) = 127 / cos(25°)^SlopeExp = P0 = 157
=> cos(25°)^SlopeExp = 127/157 = 0.8094
=> SlopeExp = ln(0.8094) / ln(cos 25°) = (-0.2115)/(-0.0984) = 2.154
```

Verification across the whole table (P0 = 0° column, expected = 30° column):

| Range | P0 (0°) | model LOS-just-defeated @30° | historical 30° col | error |
|---|---|---|---|---|
| 100 m | 157 | 156.9 | **127** (127×1.236=157.0) | <0.1 % |
| 500 m | 145 | 144.6 | **117** | <0.3 % |
| 1000 m | 132 | 131.0 | **106** | <0.8 % |

`SlopeExp = 2.15` (global Config constant, live-tunable). It is fit to APCBC; uncapped `AP` and the blunt
`APBC` BR-350A differ only by their smaller `NormalizationAngle`, which correctly makes them lose a few mm
more at angle than the capped 88. This **replaces** the research §12 "internal-consistency" note, which was
the over-predicting case.

> **OD-1 stance baked in:** we keep the optimistic forum 0° column (157/145/132, Tiger tuned ~5 % hot) as
> the gameplay-punch baseline (option A). If the user later picks the canonical baseline (option B: MV 773,
> 120/110/100), only the `DT_Shells` rows + `SlopeExp` re-fit change — the code is untouched.

### 1.3 R-B2 fix — `DragCoef k` refit (fit to PEN, not velocity)

`PenAtRange` (Option 1, tabulated lerp) is **authoritative inside the table**. `k` is now only used to
extrapolate the de-Marre tail *beyond* the last tabulated range, and it is fit so that tail matches the
table's own pen slope. Because de Marre gives `P0 ∝ v^1.10` and `v(r)=v0·e^{-k·r}`:

```
k = -(1/1.10) · ln( pen(r2)/pen(r1) ) / (r2 - r1)
```

| Gun | pen ratio used | refit `k` (1/m) | old `k` |
|---|---|---|---|
| 8.8 cm KwK 36 (PzGr. 39) | 132/157 over 1000 m | **1.58e-4** | 1.05e-4 |
| 76 mm F-34 (BR-350A) | 62/82 over 1000 m | **2.54e-4** | (n/a) |

The blunt, lower-velocity 76 mm sheds pen faster (steeper `k`) — physically correct. Inline `.cpp`/data
comments must say "k fit to the **pen** table" so nobody re-derives it from velocity again.

### 1.4 R-B3 fix — close the grazing window

Two consistent clamps (was: one clamp at 85° + a ramp that only certain-ricochets at exactly 90°):
1. **`LosClampDeg = 89°`** and **`CosMin = cos(89°) = 0.01745`** — LOS can now legitimately reach
   ~6800·T at 89° (instead of being capped at 11.5·T). `CosMin` is the real seatbelt, no longer dead code.
2. **`GrazeRicochetDeg = 88°`** — for any non-overmatched plate (`overmatch < 3`), `theta_n ≥ 88°` is a
   **guaranteed** ricochet, independent of the per-shell ramp. The 85–89.9° false-pen band is gone.

> **OD-5 left as-is (WT-faithful):** overmatch ≥ 3 still *suppresses* ricochet, so a 25 mm roof under a
> grazing 88 mm hit does not bounce — but with `SlopeExp` the roof's effective LOS is now enormous
> (`16 mm → ~250 mm @ 87°`), so it correctly **non-pens** instead of false-penning. If the user later wants
> the OD-5 "absolute-angle escape," add `if theta_n ≥ GrazeRicochetDeg force ricochet regardless of
> overmatch` — a one-line change flagged in the `.cpp`.

---

## 2. Period-correct data

All combat numbers are **game-tuning targets**, not lab claims. `[T]`=tuning/interpolated. Sources §6.

### 2.1 Shells (`DT_Shells` rows)

#### `8,8cm_PzGr39` — Tiger I 8.8 cm KwK 36 (APCBC) — PLAYER
| Field | Value | Note |
|---|---|---|
| ShellType | `APCBC` | capped AP, HE filler ignored in M1 |
| Mass | **10.2** kg | [acg][ww2w] |
| Caliber_mm | **88** | |
| MuzzleVelocity | **773** m/s | canonical KwK 36 PzGr. 39 (research used 800; 773 is the standard figure — adopt the honest one) `[T]` |
| DragCoef | **1.58e-4** | refit to pen table (§1.3) |
| NormalizationAngle | **5°** | capped |
| RicochetAngle | **70°** | |
| PenAtRange (range_m → pen_mm @ normal) | **100→157, 500→145, 1000→132, 1500→120, 2000→110** | 100–1000 from [acg] 0° column; 1500/2000 extrap `[T]` |

#### `76mm_BR350A` — F-34 / ZiS-5 (APBC, blunt + ballistic cap) — T-34/76 **and** KV-1
| Field | Value | Note |
|---|---|---|
| ShellType | `APCBC` | M1 enum has no APBC; model as capped with a *smaller* normalization to read as blunt |
| Mass | **6.5** kg | BR-350A `[T]` |
| Caliber_mm | **76.2** | |
| MuzzleVelocity | **655** m/s | F-34 BR-350A |
| DragCoef | **2.54e-4** | refit to pen table (§1.3) |
| NormalizationAngle | **4°** | blunt-capped: bites but normalizes a touch less than the 88 |
| RicochetAngle | **68°** | slightly earlier skip than the capped 88 `[T]` |
| PenAtRange | **100→82, 500→70, 1000→62, 1500→54** | tuned so the 76 can **just** defeat the Tiger 80 mm side only at near-perpendicular point-blank, and **never** the 100 mm front — matches the historical "had to flank to <300 m" reality `[T]` |

> Both the T-34/76 and the KV-1 Model 1942 fire this round (the F-34 and the KV's ZiS-5 are ballistic
> twins). One shell row serves both enemy vehicles.

#### (Reserved, NOT in M1 data) `85mm_BR365` — T-34-85 — schema-only future row; do not author in `DT_Shells` for M1.

### 2.2 Armor layouts (`DA_Armor_*` `UPrimaryDataAsset` wrapping `FVehicleArmorLayout`)

Thickness in mm; Slope = deg from vertical (documentation/fallback — runtime `theta_n` comes from the mesh
normal per MILESTONE_1_PLAN step 8 axis-compare on the placeholder cube). Modules carry `LocalPos` for the
spall cone and `Criticality` for the knockout decision.

#### `DA_Armor_TigerI` (player)
| Face (`FArmorSection.Name`) | Thickness_mm | SlopeAngle | Note |
|---|---|---|---|
| HullFront | 100 | 22 | driver plate [wiki-tiger] |
| HullFrontLower | 100 | 25 | nose |
| HullSideUpper | 80 | 0 | superstructure |
| HullSideLower | 60 | 0 | sponson (thinner) [alanhamby] |
| HullRear | 80 | 8 | |
| TurretFront | 100 | 8 | |
| GunMantlet | 120 | 0 | single effective face (real 110–200 curved) `[T]` |
| TurretSide / TurretRear | 80 | 0 | |
| TurretRoof | 25 | 90 | (40 from Mar'44) overmatch target |
| HullRoof / Floor | 25 | 90 | |

Modules: `Commander`/`Gunner`/`Loader`/`Driver`/`Radio` (Crit 1.0 each — crew-of-1 mechanic: only the
player's active station drives the abort flow, the rest are AI-crew placeholders), `Engine`(rear, 0.4),
`Transmission`(front, 0.4), `AmmoRack`(sponsons — large vulnerable side target, **Crit 1.0**),
`FuelTank`(rear sides, 0.7).

#### `DA_Armor_T34_76` (Model 1942/43, cast hex turret)
| Face | Thickness_mm | SlopeAngle | Note |
|---|---|---|---|
| HullGlacis (upper front) | 45 | **60** | the famous sloped glacis [te-t34][wiki-t34] |
| HullLowerFront | 45 | 53 | |
| HullSideUpper | 40 | 40 | sloped upper side `[T]` |
| HullSideLower | 45 | 0 | vertical |
| HullRear | 45 | 47 | |
| TurretFront | 52 | 5 | cast hexagonal turret '42 |
| Mantlet | 90 | 0 | cast |
| TurretSide | 52 | 20 | |
| TurretRear | 45 | 20 | |
| Roof | 16 | 90 | overmatch target |
| Floor | 16 | 90 | |

Modules: Crew×4–5 (`Commander`/`Gunner` in the cramped two-man turret, `Driver`, `HullGunner`/`Loader`),
`Engine`(rear, 0.4), `Transmission`(rear, 0.4), `AmmoRack`(**hull floor — the classic T-34 weak point**,
Crit 1.0), `FuelTank`(hull sides — fire risk, 0.7).

#### `DA_Armor_KV1` (Model 1942)
| Face | Thickness_mm | SlopeAngle | Note |
|---|---|---|---|
| HullGlacis | 75 | 30 | (some '42 up-armored to 90 appliqué) [wiki-kv] |
| HullLowerFront | 70 | 25 | |
| HullSideUpper | 75 | 0 | vertical slab sides |
| HullSideLower | 75 | 0 | |
| HullRear | 70 | 0 | |
| TurretFront | 90 | 8 | cast/welded '42 turret |
| Mantlet | 90 | 0 | |
| TurretSide | 75 | 5 | |
| TurretRear | 75 | 5 | |
| Roof | 30 | 90 | |
| Floor | 30 | 90 | |

Modules: Crew×5 (`Commander`/`Gunner`/`Loader`/`Driver`/`HullGunner`), `Engine`(rear, 0.4),
`Transmission`(rear, 0.4), `AmmoRack`(hull sides/floor, Crit 1.0), `FuelTank`(0.7).

> The KV-1 is a deliberately *tough* mid-run threat: near-vertical 75 mm everywhere defeats the 76 mm but
> is reliably beaten by the player's 88. The T-34/76 is the *agile, well-sloped* threat whose glacis is a
> real plate even to the 88.

---

## 3. Data schema

### 3.1 No change to `KesselBallisticsTypes.h`
The scaffold structs (`EShellType`, `FShellData`, `FArmorSection`, `FCrewModule`, `FVehicleArmorLayout`,
`FHitResolution`) are accepted **as-is**. `FShellData.PenAtRange` is `TArray<FVector2D>` (X=range_m,
Y=pen_mm) — the rows in §2.1 map directly. `FHitResolution` already exposes
`{bPen,bRicochet,LOS,P0,residualFrac,spallPower,entryPoint,dir}`, which the `.cpp` below fills.

### 3.2 `DT_Shells` (DataTable, row struct `FShellData`)
Two rows for M1: `8,8cm_PzGr39`, `76mm_BR350A` (values §2.1). Row name = the FName used by
`UKesselGunComponent`'s chambered shell.

### 3.3 `DA_Armor_*` (one `UPrimaryDataAsset` per vehicle wrapping `FVehicleArmorLayout`)
`DA_Armor_TigerI`, `DA_Armor_T34_76`, `DA_Armor_KV1` (faces+modules §2.2). The armor component
(MILESTONE step 8) maps a cube-face hit-normal to one `FArmorSection` by local axis; real bones later.

### 3.4 `UBallisticsConfig` — REVISED (the only header that must change)
The scaffold's `ThetaMaxDeg=60` / `CosMin=0.1` were mislabeled. Replace with explicit, consistent fields.
**Recommended `UBallisticsConfig` field set (Claude to apply):**

```cpp
// keep + retune:
float CosMin               = 0.01745f; // cos(89°); real grazing seatbelt (R-B3)
float OvermatchNoRicochet  = 3.0f;     // caliber/T ≥ this suppresses ricochet (WT-faithful, OD-5)
float DeMarreScale         = 1.0f;     // tail scalar when extrapolating off the table
float SpallScale           = 0.02f;    // residualFrac*caliber*this → spallPower (tune in step 9)
float MinPlateThickness_mm = 0.1f;
// new:
float SlopeExp             = 2.15f;    // R-B1 angle-hardening exponent (fit to 88 0°/30° columns)
float NormOvermatchGain    = 1.4f;     // deg of extra normalization per unit overmatch>1
float NormMaxBonus         = 8.0f;     // cap on the overmatch normalization bonus (deg)
float LosClampDeg          = 89.0f;    // clamp theta_eff before cos (R-B3)
float GrazeRicochetDeg     = 88.0f;    // theta_n ≥ this → guaranteed ricochet if not overmatched (R-B3)
// REMOVE: ThetaMaxDeg (was 60, mislabeled) — superseded by LosClampDeg + NormMaxBonus
```

All live-tunable so the sim "feel" stays adjustable like the graphics dial.

---

## 4. Drafted implementation — `UKesselBallisticsSubsystem::ResolveKineticHit`

Drop-in replacement for the TODO body in `Source/Kessel/Private/KesselBallisticsSubsystem.cpp`. Uses only
the existing signature + `FHitResolution` fields + the revised `UBallisticsConfig` (§3.4). Numerically
safe per research §11 (every `acos`/`cos`/divide clamped; no `NaN` can reach gameplay). **Do not edit
`Source/` from this doc — Claude integrates.**

```cpp
// Copyright Kessel. All Rights Reserved.

#include "KesselBallisticsSubsystem.h"
#include "KesselBallisticsConfig.h"

namespace
{
    // Piecewise-linear lerp of the tabulated pen curve, with a de-Marre (pen ∝ v^1.10,
    // v=v0·e^{-k·r}) tail beyond the last sample. Curve is authoritative inside its range.
    float PenetrationAtRange(const FShellData& Shell, float RangeM, float DeMarreScale)
    {
        const TArray<FVector2D>& Curve = Shell.PenAtRange;
        if (Curve.Num() == 0)
        {
            return 0.f; // validated non-empty at data load; defensive
        }
        if (Curve.Num() == 1 || RangeM <= Curve[0].X)
        {
            return FMath::Max(Curve[0].Y, 0.f);
        }

        const FVector2D& Last = Curve.Last();
        if (RangeM >= Last.X)
        {
            // Extrapolate past the table using the round's own drag-scaled de Marre slope.
            const float V0   = FMath::Max(Shell.MuzzleVelocity, 1.f);
            const float vEnd = V0 * FMath::Exp(-Shell.DragCoef * Last.X);
            const float vNow = V0 * FMath::Exp(-Shell.DragCoef * RangeM);
            const float Ratio = (vEnd > KINDA_SMALL_NUMBER)
                ? FMath::Pow(vNow / vEnd, 1.10f)
                : 0.f;
            return FMath::Max(Last.Y * Ratio * DeMarreScale, 0.f);
        }

        // Inside the table: find the bracketing pair and lerp.
        for (int32 i = 1; i < Curve.Num(); ++i)
        {
            if (RangeM <= Curve[i].X)
            {
                const FVector2D& A = Curve[i - 1];
                const FVector2D& B = Curve[i];
                const float Span = FMath::Max(B.X - A.X, KINDA_SMALL_NUMBER);
                const float T    = FMath::Clamp((RangeM - A.X) / Span, 0.f, 1.f);
                return FMath::Max(FMath::Lerp(A.Y, B.Y, T), 0.f);
            }
        }
        return FMath::Max(Last.Y, 0.f); // unreachable; defensive
    }
}

FHitResolution UKesselBallisticsSubsystem::ResolveKineticHit(
    const FShellData& Shell, const FArmorSection& Section,
    const FVector& ImpactDir, const FVector& SurfaceNormal, float Range)
{
    FHitResolution Out;
    Out.entryPoint = FVector::ZeroVector; // entry point is set by the projectile (world hit); kept for the contract
    Out.dir        = ImpactDir;

    // Tuning source: fall back to sane defaults if no config asset is assigned.
    const float SlopeExp        = Config ? Config->SlopeExp            : 2.15f;
    const float NormGain        = Config ? Config->NormOvermatchGain   : 1.4f;
    const float NormMaxBonus    = Config ? Config->NormMaxBonus        : 8.0f;
    const float OvermatchNoRico = Config ? Config->OvermatchNoRicochet : 3.0f;
    const float LosClampDeg     = Config ? Config->LosClampDeg         : 89.0f;
    const float CosMin          = Config ? Config->CosMin              : 0.01745f;
    const float GrazeRicoDeg    = Config ? Config->GrazeRicochetDeg    : 88.0f;
    const float SpallScale      = Config ? Config->SpallScale          : 0.02f;
    const float MinPlate        = Config ? Config->MinPlateThickness_mm: 0.1f;

    // 1. Impact angle from normal. Clamp the dot before acos (float error can push it past ±1 → NaN).
    const FVector InDir  = ImpactDir.GetSafeNormal();
    const FVector OutN   = SurfaceNormal.GetSafeNormal();
    if (InDir.IsNearlyZero() || OutN.IsNearlyZero())
    {
        return Out; // degenerate input → safe non-pen, no NaN
    }
    const float CosImpact = FMath::Clamp(FVector::DotProduct(-InDir, OutN), -1.f, 1.f);
    if (CosImpact <= KINDA_SMALL_NUMBER)
    {
        return Out; // back-face or parallel graze → ignore (already inside / no engagement)
    }
    const float ThetaN = FMath::RadiansToDegrees(FMath::Acos(CosImpact));

    // 2-5. Plate, P0, overmatch, normalization.
    const float T         = FMath::Max(Section.Thickness_mm, MinPlate);
    const float P0        = PenetrationAtRange(Shell, FMath::Max(Range, 0.f),
                                               Config ? Config->DeMarreScale : 1.0f);
    Out.P0 = P0;
    const float Overmatch = Shell.Caliber_mm / T;
    const float NormEff   = Shell.NormalizationAngle
                          + FMath::Clamp((Overmatch - 1.f) * NormGain, 0.f, NormMaxBonus);

    // 6. RICOCHET — resolved BEFORE the thickness compare (a shell can bounce off armor it could
    //    otherwise out-penetrate; that is the entire point of sloped armor).
    if (Overmatch < OvermatchNoRico)
    {
        if (ThetaN >= GrazeRicoDeg)
        {
            Out.bRicochet = true; // guaranteed graze deflect (R-B3 — closes the 85–90° window)
            return Out;
        }
        if (ThetaN > Shell.RicochetAngle)
        {
            const float Denom = FMath::Max(90.f - Shell.RicochetAngle, KINDA_SMALL_NUMBER);
            const float PRic  = FMath::Clamp((ThetaN - Shell.RicochetAngle) / Denom, 0.f, 1.f);
            if (RandomStream.FRand() < PRic)
            {
                Out.bRicochet = true;
                return Out;
            }
        }
    }

    // 7-8. Effective (line-of-sight) thickness with angle-hardening exponent (R-B1).
    const float ThetaEff = FMath::Clamp(FMath::Max(ThetaN - NormEff, 0.f), 0.f, LosClampDeg);
    const float CosEff   = FMath::Max(FMath::Cos(FMath::DegreesToRadians(ThetaEff)), CosMin);
    const float LOS      = T / FMath::Pow(CosEff, SlopeExp);
    Out.LOS = LOS;

    // 9. Penetration decision + behind-armor energy.
    if (P0 >= LOS && P0 > KINDA_SMALL_NUMBER)
    {
        Out.bPen         = true;
        Out.residualFrac = FMath::Clamp((P0 - LOS) / P0, 0.f, 1.f);
        Out.spallPower   = Out.residualFrac * Shell.Caliber_mm * SpallScale;
    }
    // else: NON-PEN. (Optional thin-plate spall band P0≥0.9·LOS is flag-gated, deferred from M1.)

    return Out;
}

void UKesselBallisticsSubsystem::SeedRng(int32 InSeed)
{
    // Seed once per duel/level so ricochet (and later spall) rolls are reproducible for debugging.
    RandomStream.Initialize(InSeed);
}
```

Integration notes for Claude:
- `entryPoint` is left zero here; the projectile (`AKesselProjectile`, step 9) owns the world hit location
  and should set `Out.entryPoint = Hit.ImpactPoint` after the call, then aim the spall cone along `Out.dir`.
- Requires the §3.4 `UBallisticsConfig` fields. If you keep the old `ThetaMaxDeg` temporarily, the `Config ?
  ... : default` fallbacks keep it compiling, but the **defaults bake in the corrected values** so behavior
  is right even with no asset assigned.
- `FVector2D`/`FMath`/`FRandomStream`/`KINDA_SMALL_NUMBER` are all `CoreMinimal`; no new includes.

---

## 5. Unit tests — the corrected duels (acceptance gate, build step 7)

Synthetic inputs (no projectile yet): call `ResolveKineticHit` directly. `theta_n` is supplied via a unit
`SurfaceNormal` and `ImpactDir` whose dot gives the stated angle (e.g. normal `+X`, dir at `theta_n` off
`-X`). Seed RNG to a fixed value so the one ricochet-probability case is deterministic. Tolerances ±1 mm on
LOS, exact on `bPen`/`bRicochet`.

| # | Shooter → target face | Range | theta_n | Expect | Key numbers (model) |
|---|---|---|---|---|---|
| **T1** | 88 → Tiger-cal plate 127 mm | 100 | 30° | **PEN, marginal** | P0 157, normEff 5, θeff 25, LOS 156.9 → residualFrac ≈ 0.0006 (calibration anchor) |
| **T2** | 88 → Tiger-cal plate 117 mm | 500 | 30° | **PEN, marginal** | P0 145, LOS 144.6 (anchor) |
| **T3** | 88 → Tiger-cal plate 106 mm | 1000 | 30° | **PEN, marginal** | P0 132, LOS 131.0 (anchor) |
| **T4** | 88 → T-34/76 glacis 45 mm | 500 | 60° | **PEN, marginal** | overmatch 1.96, normEff 6.34, θeff 53.66, **LOS 138.7**, P0 145 → residualFrac ≈ 0.043 *(old model said LOS≈70 / trivial pen — the fix)* |
| **T5** | 88 → T-34/76 glacis 45 mm | 1500 | 60° | **NON-PEN** | P0 120 < LOS 138.7 (angled glacis defeats the 88 at long range) |
| **T6** | 88 → T-34/76 glacis, hull turned | 500 | 72° | **RICOCHET likely** | overmatch<3, 72>70 → pRic 0.10; if not bounced LOS≈303 ≫145 → NON-PEN |
| **T7** | 88 → KV-1 hull front 75 mm | 500 | 25° | **PEN, clean** | normEff 5.24, θeff 19.8, LOS 85.5, P0 145 → residualFrac ≈ 0.41 |
| **T8** | 88 → T-34/76 roof 16 mm (overmatch) | 300 | 87° | **NON-PEN, no ricochet** | overmatch 5.5 ≥3 → no bounce; normEff 13 (capped+8), θeff 74, LOS ≈ 250 ≫ P0 *(R-B3: was a false pen)* |
| **T9** | 76 → Tiger turret front 100 mm | 500 | 5° | **NON-PEN** | normEff 4, θeff 1, LOS ≈100, P0 70 < 100 (76 can never front-pen the Tiger) |
| **T10** | 76 → Tiger hull side 80 mm, square | 100 | 0° | **PEN, hairline** | LOS 80, P0 82 → residualFrac ≈ 0.024 (only point-blank perpendicular works) |
| **T11** | 76 → Tiger hull side 80 mm, turned | 300 | 45° | **NON-PEN** | normEff 4, θeff 41, LOS ≈ 137, P0 74 < 137 (angling the Tiger defeats the 76) |
| **T12** | 76 → Tiger hull side 80 mm | 1000 | 0° | **NON-PEN** | P0 62 < 80 (range kills the 76's side pen) |
| **T13** | grazing 89° on Tiger 80 mm side, overmatch<3 | 500 | 89° | **RICOCHET (certain)** | θn ≥ 88 GrazeRicochetDeg (R-B3 guaranteed deflect) |
| **T14** | any | any | any | **no NaN/Inf** in any `FHitResolution` field across a fuzz of θn ∈ [0,90], T ∈ [10,250], R ∈ [0,3000] |

**Done-when (step 7 gate):** T1–T3 land on their LOS anchors (<1 mm), T4/T5 show the corrected *marginal*
(not trivial) glacis result, T8/T13 prove the grazing window is closed, T9–T12 show the 76 mm's correct
impotence vs the Tiger front and its point-blank-only side threat, and T14 finds no NaN. These supersede
the research §13 back-of-envelope checks (which used the over-predicting `1/cos` model).

---

## 6. Sources & explicit assumptions

Penetration/shell: [acg] Armchair General "8,8cm KwK 36 PzGr 39 Penetration" (10.2 kg; 157/145/132 @0°,
127/117/106 @30°); [ww2w] WW2-Weapons penetration tables; canonical KwK 36 MV 773 m/s; [panzerwar]/[te]
F-34 BR-350A (655 m/s, 6.5 kg). Armor: [wiki-tiger]/[alanhamby] Tiger I (100 front, 80 side, 25 roof,
mantlet 110–200); [te-t34]/[wiki-t34] T-34 (45@60 glacis, cast hex turret 52); [wiki-kv] KV-1 Model 1942
(75–90 mm slab). Model conventions: War Thunder (DeMarre pen, overmatch ≥3 suppresses ricochet, AP
ricochet onset ~70°).

ASSUMPTIONS (tagged `[T]` above): `SlopeExp=2.15` is fit to the 88's two-column data and applied globally;
`DragCoef k` are pen-table fits, not measured Cd; 1500/2000 m rows and the entire BR-350A curve are tuned
to land the *gameplay-correct* duels (Tiger-front-immune-to-76, T-34-glacis-is-a-real-plate), not lab
transcriptions; BR-350A is true APBC modeled as `APCBC` with reduced normalization; mantlet is one 120 mm
effective face; HE/HEAT/APCR enum+fields remain reserved/unwired in M1; the T-34-85/85 mm data from
research §13 is removed from M1 (period-incorrect) and kept only as a future schema row.
