# 04 — Ballistics & Armor Penetration/Deflection Model (Milestone 1)

Status: DESIGN (no C++ this phase). Engine: UE 5.8.0.
Scope: tractable-but-believable kinetic ballistics + armor pen/deflect for a single-tank combat duel, in the spirit of War Thunder / Gunner HEAT PC. Milestone 1 ships **kinetic AP/APCBC only**; HE/HEAT are stubbed for later.

This doc gives: shell taxonomy, the actual runtime formulas (drag falloff, line-of-sight thickness, normalization, ricochet, overmatch, behind-armor effect), the data schema (`FShellData`, `FArmorSection`, per-face layout), and sanity-checked example values for the **Tiger I 8.8 cm KwK 36** and **T-34-85 85 mm ZiS-S-53** plus their armor. All historical numbers are cited at the bottom and tagged where they are interpolated/assumed.

---

## 1. Design goals & guardrails

- **Believable, not FEM.** No finite-element shatter. We model: range→velocity falloff, velocity→penetration, impact-angle→effective thickness, angle/overmatch→ricochet, then a behind-armor "spall pool" against crew/modules. This is the same conceptual stack War Thunder uses (DeMarre-style penetration + angle + normalization + ricochet thresholds + post-pen cone of fragments). [WT-pen]
- **Data-driven.** Every weapon and every armor face is a row of data. Gameplay code never hardcodes a tank. This keeps the "graphics/feel is TUNABLE" philosophy of the project consistent on the sim side too.
- **Deterministic core, bounded randomness.** Penetration math is deterministic; only ricochet near the threshold and behind-armor module selection use a seeded RNG so duels are reproducible for debugging.
- **Numerically safe.** Every `cos`/`tan`/divide that can blow up near 90° impact is clamped (see §11). No `NaN` shall ever reach gameplay.

### Angle convention (FIX THIS ONCE — everything depends on it)
We use **impact angle from normal**, symbol `theta_n`, in degrees:
- `theta_n = 0°` → shell travels straight down the surface normal (perpendicular hit, "flat on"). Thinnest LOS path. Best penetration.
- `theta_n = 90°` → shell travels parallel to the plate (grazing). Infinite LOS path. Always ricochet/miss.

This is the War-Thunder-style "angle of impact." Note the historical penetration tables below are quoted the *other* way (German "0°/30°" means 0° and 30° **of plate slope from vertical**, i.e. from normal for a horizontal shot) — we convert in §8. Be careful: tankers' tables and game conventions disagree, and mixing them is the #1 source of wrong numbers.

---

## 2. Shell taxonomy

### Milestone 1 (implement now)
| Type | Code | Behavior model | Notes |
|---|---|---|---|
| Armor-Piercing Capped Ballistic Capped | `APCBC` | Kinetic. Full normalization + ricochet + overmatch. | Primary main-gun round for both example tanks. The ballistic cap improves drag (higher pen at range) and the penetrating cap improves normalization & bite at angle. Modeled as parameters, not geometry. |
| Armor-Piercing (uncapped) | `AP` | Kinetic. Same pipeline, *worse* normalization, *higher* ricochet tendency. | Cheap fallback / older ammo. |

Both run the **identical** `ResolveKineticHit()` pipeline (§4); they differ only by data (`normalizationAngle`, `ricochetAngle`, drag).

### Later milestones (schema reserved, NOT implemented now)
| Type | Code | Why deferred | Reserved fields |
|---|---|---|---|
| High-Explosive | `HE` | Needs blast/overpressure + spall-vs-thin-plate + soft-target model. | `explosiveMassTNTeq`, `fuzeDelay` |
| High-Explosive Anti-Tank (shaped charge) | `HEAT` | Velocity-independent jet penetration; angle hurts but range doesn't; defeated by standoff/spaced armor. Different `penAtRanges` semantics (constant). | `staticPen`, `standoffOptimal` |
| AP Composite Rigid / APDS | `APCR`/`APDS` | Light core → huge near pen, terrible falloff; different drag + overmatch behavior. | `coreCaliber`, `coreMass` |

The enum and the reserved fields exist in the schema from day one so save-data and asset rows don't break later. `ResolveHit()` switches on `EShellType` and dispatches; M1 only wires the kinetic branch.

---

## 3. Stage A — Exterior ballistics: range → impact velocity

We do **not** run a full 3-DOF trajectory for the penetration number (the projectile *visual* may arc under gravity for aiming, but pen is computed from the speed at impact). Impact speed from a flat-fire drag model:

```
v(r) = v0 * exp(-k * r)
```

- `v0` = muzzle velocity (m/s), from `FShellData.muzzleVel`.
- `r`  = slant range to impact (m).
- `k`  = drag decay constant (1/m) = `FShellData.dragCoef`.

`k` is fit so the model reproduces the published velocity (or penetration) at a couple of ranges rather than derived from physical Cd/area/mass — this is the tractable choice and matches how arcade-sim hybrids tune it. Typical large-caliber AP `k ≈ 1.0e-4 … 1.6e-4` /m (loses ~10–15% velocity per km). See worked fits in §10.

**Authoring shortcut (preferred):** instead of trusting the analytic drag for pen, store a **measured penetration curve** `penAtRanges[]` (range→mm pairs from the historical table) and **interpolate** it directly (§5, Option 1). The drag `v(r)` is then only used for (a) round visual lead and (b) extrapolating beyond the tabulated ranges. This keeps numbers honest against public data.

---

## 4. Stage B — The hit-resolution pipeline (per impact)

```
ResolveKineticHit(shell, faceSection, impactDir, surfaceNormal, range):
  1. theta_n  = AngleFromNormal(impactDir, surfaceNormal)      // clamp, §11
  2. P0       = PenetrationAtRange(shell, range)               // §5, mm RHA @ normal
  3. overmatch = shell.caliber / faceSection.thickness
  4. norm     = EffectiveNormalization(shell, overmatch)       // §7, degrees
  5. theta_eff = max(theta_n - norm, 0)                        // normalization bends path toward normal
  6. RICOCHET CHECK (§6):
        if (overmatch < 3.0) and (theta_n > RicochetAngle(shell, overmatch)):
            roll = Rand01(seed)
            if roll < RicochetProb(theta_n, shell): -> RICOCHET (exit, shell deflects)
  7. LOS       = EffectiveThickness(faceSection, theta_eff)    // §8, mm
  8. if P0 >= LOS:  -> PENETRATION, residualFrac = (P0 - LOS)/P0  -> §9 behind-armor
     else:          -> NON-PEN (may still cause spall if P0 close, §9.3)
```

Order matters: **ricochet is checked before the thickness compare** (a shell can bounce off armor it would otherwise have the raw pen to defeat — that is the whole point of sloped armor), and normalization is applied to the angle used for *both* the LOS path and as an input to ricochet leniency via overmatch.

---

## 5. Stage B.2 — Penetration vs velocity / mass / caliber (P0 at range)

`P0` is penetration into **RHA at 0° (normal)** at the impact range. Two interchangeable implementations; ship Option 1, keep Option 2 for extrapolation and for new guns lacking tables.

### Option 1 — Tabulated curve (authoring truth)
`FShellData.penAtRanges` = sorted array of `{ rangeM, penMM }` (the historical 0°-slope column). Piecewise-linear interpolate; clamp-extrapolate the ends using the drag-scaled DeMarre exponent below so we never read past the array.

```
P0(r) = LerpClamped(penAtRanges, r)
```

### Option 2 — DeMarre-style analytic (generative / extrapolation)
Classic penetration scaling (de Marre): penetration grows with velocity and projectile mass, falls with caliber and target hardness.

```
P0 = C * (m^0.55) * (v^1.10) / (d^0.65)        // de Marre form, exponents tunable
```

- `m` = projectile mass (kg), `d` = caliber (m or cm — fold unit into `C`), `v` = impact velocity (m/s) from §3.
- `C` = ballistic/quality constant fit so the formula reproduces the tank's *known* pen at one reference range, then it predicts the others. The commonly used exponents are ~1.43 on v and ~0.71 on m in the "naval" de Marre; we use the flattened (v^1.1, m^0.55, d^0.65) set because it tracks WWII tank-gun tables better across 100–2000 m. Treat exponents as **ASSUMPTION / tuning knobs**, not physics. [demarre]

Because `v` falls with range (§3) and `P0 ~ v^1.1`, penetration falls ~1.1× as fast as velocity — i.e. a gun losing 12%/km velocity loses ~13%/km pen, which matches the example tables (§12).

---

## 6. Stage B.6 — Ricochet / deflection

Two gates: an **overmatch suppression** and an **angle-vs-probability** roll.

### Overmatch suppression (no bounce on thin plate)
If the projectile is large relative to the plate it punches through instead of skipping:
```
overmatch = caliber_mm / plateThickness_mm
if overmatch >= 3.0:   ricochet is DISABLED (shell always bites)   // WT rule
```
This is why an 88 mm (caliber 88) vs a 25 mm roof (overmatch 3.5) never ricochets even at a glancing angle, but the same shell vs 80–100 mm plate (overmatch ~0.9–1.1) can skip. [WT-overmatch]

### Angle threshold + probability
Each shell has a `ricochetAngle` (deg from normal) below which ricochet is impossible. Above it, ricochet probability ramps to 1.0 by 90°:

```
RicochetProb(theta_n, shell):
   a0 = shell.ricochetAngle              // e.g. APCBC ~70°, AP ~65°
   if theta_n <= a0: return 0
   if theta_n >= 90: return 1
   return clamp( (theta_n - a0) / (90 - a0), 0, 1 )   // linear ramp; smoothstep optional
```

Typical thresholds (from-normal):
| Shell | `ricochetAngle` (full-bounce certainty by 90°) | Rationale |
|---|---|---|
| `APCBC` | **70°** | Penetrating + ballistic cap "bites," resisting skip; WT/historical "AP ricochets past ~70°." [WT-ricochet] |
| `AP` (uncapped) | **65°** | No cap → skips earlier and harder. |
| `HEAT` (later) | ~80° but **shatters** rather than skips; different model. | reserved |

The roll uses a seeded stream so a given shot is reproducible. Near-threshold randomness (a few degrees of fuzz) is intentional: it prevents a hard binary "lol bounce" line and reads as believable.

---

## 7. Stage B.4 — Normalization (angle bonus from the cap / bite)

On impact the projectile "turns into" the plate, reducing the effective angle and thus the LOS path. APCBC normalizes more than bare AP. Overmatch increases normalization (a big shell on thin plate straightens out more).

```
EffectiveNormalization(shell, overmatch):
   base = shell.normalizationAngle          // APCBC ~5°, AP ~1.5°  (from-normal degrees)
   // WT-style overmatch bonus: bigger caliber vs thinner plate normalizes harder
   bonus = clamp( (overmatch - 1.0) * NORM_OVERMATCH_GAIN, 0, NORM_MAX_BONUS )
   return base + bonus                       // degrees
```
Suggested constants: `NORM_OVERMATCH_GAIN ≈ 1.4°`, `NORM_MAX_BONUS ≈ 8°`. Then `theta_eff = max(theta_n - norm, 0)` is fed to the thickness formula (§8). Normalization never makes the path *longer* and is clamped at 0.

---

## 8. Stage B.7 — Effective (line-of-sight) armor thickness

Nominal plate `T` (mm) presented at impact angle `theta_eff` (from normal) gives the LOS path the shell must defeat:

```
LOS = T / cos(theta_eff)
```

This is the canonical `nominal / cos(impactAngle)` rule. Plate **slope** is baked into the geometry: the artist/data gives each face a `slope` (deg of the plate from vertical, for documentation/inspection), but at runtime the *actual* `theta_eff` comes from the dot product of the incoming ray and the **mesh surface normal** — so a hull-down hit, a side shot on a turned tank, and a sloped glacis all fall out of one formula. `slope` in data is the design-time nominal used to pre-compute the example "effective" numbers and to drive a simplified hitbox if we ever skip per-poly normals.

**Divide-by-zero guard (mandatory):** clamp `theta_eff` to `<= THETA_MAX` (e.g. 85°) before `cos`, and floor `cos` at `COS_MIN` (e.g. 0.0436 ≈ cos 87.5°):
```
c = max(cos(clamp(theta_eff, 0, THETA_MAX)), COS_MIN)
LOS = T / c                                  // never divides by ~0
```
Anything steeper than `THETA_MAX` has already been caught by the ricochet gate (§6) in practice; the clamp is the seatbelt.

---

## 9. Stage C — Behind-armor effect (post-penetration)

Kept deliberately simple but lethal-feeling. On a PEN we spawn a **fragment cone** into the interior and damage whatever it intersects from a crew/module pool.

### 9.1 Module/crew pool
The tank interior is a small list of `FCrewModule { type, hp, position, critical }`:
- **Crew (single):** the lone operator. In *Kessel* the player is crew-of-1, so a solid crew hit = mission-ending wound (heavy effect, not necessarily instant — see 9.4). Enemy tanks have 3–5 crew slots; killing enough = knockout.
- **Modules:** `Engine`, `Transmission`, `FuelTank`, `AmmoRack`, `Gun/Breech`, `Tracks`, `Optics`.
- `AmmoRack` hit with sufficient residual energy = catastrophic kill (detonation).

### 9.2 Damage dealt = residual penetration energy
```
residualFrac = (P0 - LOS) / P0            // 0 at a marginal pen, →1 for massive overmatch
spallPower   = residualFrac * shell.caliber * SPALL_K
```
Cast a short cone (half-angle ~15–25°, widened a touch for capped/overmatch hits) along `theta_eff` direction from the entry point; each module in the cone takes `spallPower * falloff(distance)`. Critical modules within the first ~0.5 m of entry take near-full power. A clean overmatch through thin armor (`residualFrac→1`) sprays the whole compartment; a marginal pen (`residualFrac→~0`) just barely wounds the nearest thing.

### 9.3 Non-pen spall (optional, cheap realism)
If `P0` is within a small band below `LOS` (e.g. `P0 >= 0.9 * LOS`) on a *thin* plate, spawn a weak interior spall with `spallPower` scaled by how close it came. Off by default in M1; flag-gated.

### 9.4 Knockout
A tank is **knocked out** when any of:
- `AmmoRack` detonation, or `FuelTank` ignition with enough power, or
- crew killed/incapacitated beyond the platform's threshold (enemy: ≥ N of M crew; player crew-of-1: a confirmed solid crew hit triggers the death/abort sequence — possibly with a brief bleed-out window for drama), or
- `Engine` + `Transmission` both dead → mobility kill (not a full kill but mission-relevant for an escape game).

This is enough fidelity for a M1 duel and leaves clear seams for a richer crew/morale model later.

---

## 10. Data schema

USTRUCTs (UE 5.8, `BlueprintType`, exposed for data-table authoring). Field names match the task spec. **ASSUMPTION:** standard `USTRUCT`/`UPROPERTY(EditAnywhere, BlueprintReadWrite)` reflection (the ubiquitous UE pattern; verify against engine headers when C++ lands, but these are stable macros, not 5.8-new API).

```cpp
UENUM(BlueprintType)
enum class EShellType : uint8 { AP, APCBC, APCR, APDS, HE, HEAT };

USTRUCT(BlueprintType)
struct FPenSample {                 // one point on the measured pen curve
    UPROPERTY(EditAnywhere) float RangeM   = 0.f;
    UPROPERTY(EditAnywhere) float PenMM    = 0.f;   // RHA @ normal (0° from normal)
};

USTRUCT(BlueprintType)
struct FShellData {
    UPROPERTY(EditAnywhere) FName        Id;
    UPROPERTY(EditAnywhere) EShellType   Type           = EShellType::APCBC;
    UPROPERTY(EditAnywhere) float        MassKg         = 0.f;   // projectile mass
    UPROPERTY(EditAnywhere) float        CaliberMM      = 0.f;   // for overmatch & spall
    UPROPERTY(EditAnywhere) float        MuzzleVelMS    = 0.f;   // v0
    UPROPERTY(EditAnywhere) float        DragCoef       = 1.0e-4f;// k, 1/m  (§3)
    UPROPERTY(EditAnywhere) TArray<FPenSample> PenAtRanges;      // measured curve (§5 opt1)
    UPROPERTY(EditAnywhere) float        NormalizationAngle = 5.f;// deg from normal (§7)
    UPROPERTY(EditAnywhere) float        RicochetAngle  = 70.f;  // deg from normal (§6)
    // de Marre fallback (§5 opt2)
    UPROPERTY(EditAnywhere) float        DemarreC       = 0.f;   // fit constant, 0 = use table only
    // ---- reserved for later shell types (unused in M1) ----
    UPROPERTY(EditAnywhere) float        ExplosiveMassTNTeq = 0.f; // HE
    UPROPERTY(EditAnywhere) float        StaticPenMM        = 0.f; // HEAT (range-independent)
    UPROPERTY(EditAnywhere) float        CoreCaliberMM      = 0.f; // APCR/APDS
};

USTRUCT(BlueprintType)
struct FArmorSection {
    UPROPERTY(EditAnywhere) FName  Id;              // "HullFront", "TurretSideL", ...
    UPROPERTY(EditAnywhere) float  ThicknessMM = 0.f;
    UPROPERTY(EditAnywhere) float  SlopeDeg    = 0.f;   // nominal plate slope from vertical (doc/fallback; runtime uses mesh normal)
    UPROPERTY(EditAnywhere) FName  BoneOrSocket;        // which collision section this maps to
};

USTRUCT(BlueprintType)
struct FCrewModule {
    UPROPERTY(EditAnywhere) FName    Id;
    UPROPERTY(EditAnywhere) FName    Kind;          // Crew/Engine/AmmoRack/...
    UPROPERTY(EditAnywhere) float    Hp        = 100.f;
    UPROPERTY(EditAnywhere) FVector  LocalPos  = FVector::ZeroVector;
    UPROPERTY(EditAnywhere) bool     Critical  = false;  // ammo/fuel → catastrophic
};

USTRUCT(BlueprintType)
struct FVehicleArmorLayout {
    UPROPERTY(EditAnywhere) FName               VehicleId;
    UPROPERTY(EditAnywhere) TArray<FArmorSection> Faces;
    UPROPERTY(EditAnywhere) TArray<FCrewModule>   Modules;
};
```

Tuning constants (one `UDataAsset` `UBallisticsConfig`): `THETA_MAX=85`, `COS_MIN=0.0436`, `OVERMATCH_NORICO=3.0`, `NORM_OVERMATCH_GAIN=1.4`, `NORM_MAX_BONUS=8`, `SPALL_K`, cone half-angle. All live-tunable to keep the sim "feel" adjustable like the graphics.

---

## 11. Numerical-safety checklist (must hold in code)

1. `theta_n = acos( clamp( dot(-impactDir, normal), -1, 1 ) )` — clamp the dot before `acos` (float error can push it past ±1 → `NaN`).
2. Use `-impactDir` (incoming ray reversed) vs outward `normal`; if dot < 0 the ray hit a back-face → ignore/treat as already-inside.
3. `theta_eff` clamped to `[0, THETA_MAX]`; `cos` floored at `COS_MIN` (§8). LOS finite always.
4. Overmatch: guard `plateThickness > 0.1 mm` before divide.
5. Ricochet ramp denominator `(90 - a0)` with `a0 < 90` enforced at data-load (validate `RicochetAngle ∈ [40,89]`).
6. `residualFrac` clamped `[0,1]`; `P0 > 0` guaranteed (validate pen curve non-empty, positive).
7. RNG from a per-duel seeded `FRandomStream` so ricochet/spall rolls are reproducible.

---

## 12. Example data — Tiger I 8.8 cm KwK 36 (PzGr. 39 APCBC)

Round `8,8cm_PzGr39`:
| Field | Value | Source/Note |
|---|---|---|
| Type | `APCBC` | capped AP |
| MassKg | **10.2** | [acg][ww2w] |
| CaliberMM | **88** | |
| MuzzleVelMS | **800** | [acg] |
| NormalizationAngle | 5° | model default for capped |
| RicochetAngle | 70° | model |
| DragCoef k | **~1.05e-4 /m** | fit so pen falls 157→132 over 0→1000 m (≈ matches table) |

`PenAtRanges` (mm RHA @ **0° from normal**, i.e. the German "0°" column):
| Range m | PenMM |
|---|---|
| 100 | **157** |
| 500 | **145** |
| 1000 | **132** |
| 1500 | ~120 (interp/extrap, ASSUMPTION) |
| 2000 | ~110 (extrap, ASSUMPTION) |

Cross-check: the table's 30°-slope column (127/117/106 at 100/500/1000) is what our pipeline should *reproduce* when fed `theta_n = 30°` — `LOS = T/cos30 = 1.155 T`, and pen-at-angle ≈ pen0/1.155·(normalization bump). Good internal consistency test. [acg]

(Earlier `8,8cm_PzGr` uncapped variant for an `AP` round: 9.65 kg @ 820 m/s, 122/115/108 mm at 100/500/1000 m — usable as the cheaper AP shell. [acg])

### Tiger I armor layout (`FVehicleArmorLayout VehicleId="TigerI"`)
| Face | ThicknessMM | SlopeDeg (from vert.) | Note/Source |
|---|---|---|---|
| HullFront (driver plate) | **100** | ~10–25 (near-vertical) | [wiki-tiger][alanhamby] |
| HullFrontLower (nose) | 100 | ~25 | |
| HullSideUpper (superstructure) | **80** | 0 | [wiki-tiger] |
| HullSideLower (sponson) | 60–63 | 0 | lower hull thinner [alanhamby] |
| HullRear | 80 | ~8 | |
| TurretFront | **100** | ~0–10 | [wiki-tiger] |
| GunMantlet | **110 edges / up to 200 at gun** | curved | model as 120 effective [wiki-tiger] |
| TurretSide / TurretRear | **80** | 0 | [wiki-tiger] |
| Roof / Floor | **25** (turret roof 40 from Mar'44) | 90 (horizontal) | overmatch-vulnerable [wiki-tiger] |

Modules (illustrative local layout): Crew×5 (Cmdr, Gunner, Loader, Driver, Radio), Engine(rear), Transmission(front), AmmoRack(sponsons — large, vulnerable side target), FuelTanks(rear sides).

---

## 13. Example data — T-34-85 85 mm ZiS-S-53 (BR-365 APCBC)

Round `85mm_BR365`:
| Field | Value | Source/Note |
|---|---|---|
| Type | `APCBC` | |
| MassKg | **9.2** | [grok-apcbc][tank-afv] |
| CaliberMM | **85** | |
| MuzzleVelMS | **792** (round to 792) | [tankarch][panzerwar] |
| NormalizationAngle | 5° | |
| RicochetAngle | 70° | |
| DragCoef k | **~1.2e-4 /m** | fit to 121→94 over 0→1000 m |

`PenAtRanges` (mm RHA @ **0° from normal**): [panzerwar]
| Range m | PenMM |
|---|---|
| 0/100 | **121** |
| 500 | **113** (panzer-war lists 117/113 across short bands) |
| 1000 | **94–103** (use ~100) |
| 1500 | ~88 (extrap, ASSUMPTION) |

Note the spread between Soviet domestic figures (often higher, ~111 @ 1000 m) and Western re-tests; we adopt the conservative panzer-war column and tag the high-range points ASSUMPTION. [tankarch][panzerwar]

### T-34-85 armor layout (`FVehicleArmorLayout VehicleId="T34_85"`)
| Face | ThicknessMM | SlopeDeg (from vert.) | Effective ≈ T/cos | Source |
|---|---|---|---|---|
| HullFront (glacis) | **45** | **60** | ~90 | [wiki-t34][te-t3485] |
| HullSide | **45** | ~0 (vertical) | 45 | [te-t3485] |
| HullRear | 45 | ~45 | ~64 | |
| TurretFront / Mantlet | **90** | low | ~90 | [te-t3485] |
| TurretSide | **75** | ~20 | ~80 | [te-t3485] |
| TurretRear | **52** | ~20 | ~55 | [te-t3485] |
| Roof / Floor | **20** | 90 | — | overmatch target |

Modules: Crew×5 (Cmdr, Gunner, Loader, Driver, Hull MG), Engine(rear), Transmission(rear), AmmoRack(hull floor — classic T-34 weak point), FuelTanks(hull sides — fire risk).

### Duel sanity check (does the model "feel right"?)
- **Tiger 88 vs T-34 glacis @ 500 m:** `P0=145`, glacis 45 mm @ 60° slope. A frontal shot has `theta_n≈60°` (overmatch 88/45≈1.96 → ricochet *enabled* since <3, but 60° < 70° threshold → no bounce; normalization ~5°+overmatch bonus pulls `theta_eff` to ~50°). `LOS = 45/cos50 ≈ 70 mm` ≪ 145 → **clean pen.** Matches reality: Tiger front-penetrates T-34 easily at range.
- **T-34 85 vs Tiger turret front @ 500 m:** `P0≈113`, turret front 100 mm near-vertical (`theta_n≈small`). `LOS≈100` < 113 → **marginal pen** with low `residualFrac` (~0.12) → wounds but may not one-shot. Matches reality: 85 mm could defeat the Tiger's 100 mm front at medium range but not trivially.
- **T-34 85 vs Tiger upper side (80 mm) @ 45° turned hull:** `theta_n≈45°`, `LOS=80/cos45≈113 ≈ P0` → knife-edge; small range/angle changes flip pen↔non-pen. Reads as tense and believable.

These three back-of-envelope results are the acceptance test for the implemented pipeline.

---

## 14. Open items / explicit ASSUMPTIONS

- Drag `k` values are **fits**, not measured Cd; revisit once trajectory visuals exist.
- de Marre exponents (v^1.1, m^0.55, d^0.65) and constant `C` are tuning knobs; the **tabulated `PenAtRanges` is authoritative**, de Marre only extrapolates/generates.
- Normalization & ricochet constants are WT-flavored approximations, not from primary armor reports.
- High-range pen rows (1500/2000 m) for both guns are interpolated/extrapolated and tagged ASSUMPTION.
- Mantlet modeled as a single 120 mm effective face (real mantlet is curved 110–200 mm); refine with per-poly normals later.
- HE/HEAT/APCR fields are reserved but **not wired** in M1.
- Engine-API note: schema uses standard `USTRUCT`/`UPROPERTY`/`UENUM`/`UDataAsset`/`FRandomStream` — stable UE patterns; **verify exact headers when C++ phase begins** (this doc asserts no 5.8-specific engine symbol).

---

## 15. Sources

Penetration / shell data:
- [acg] Armchair General forum, "8,8cm KwK 36 PzGr 39-1 Penetration Figures" — 10.2 kg @ 800 m/s; 157/145/132 (0°) & 127/117/106 (30°) at 100/500/1000 m. https://forums.armchairgeneral.com/forum/historical-events-eras/world-war-ii/armor-in-world-war-ii/144851
- [ww2w] WW2-Weapons, "Penetration Tank Ammunition." https://www.ww2-weapons.com/penetration-tank-ammunition/
- [tankarch] Tank Archives, "Penetration" / "Penetration Part 2." https://www.tankarchives.com/2013/03/penetration.html , https://www.tankarchives.com/2013/05/penetration-part-2.html
- [panzerwar] Panzer War, Russian gun penetration tables (85 mm S-53 BR-365: 792 m/s; 121/117/113/103/94). http://www.panzer-war.com/page58.html
- [grok-apcbc] Grokipedia, "APCBC shell" — BR-365 9.2 kg @ 800 m/s. https://grokipedia.com/page/Armour-piercing,_capped,_ballistic_capped_shell
- [tank-afv] tank-afv.com, T-34/85 (1943). https://tank-afv.com/ww2/ussr/T-34-85.php
- [demarre] de Marre penetration formula (general reference for `C·m^a·v^b/d^c` form).

Armor data:
- [wiki-tiger] Wikipedia, "Tiger I" — 100 mm hull/turret front, 80 mm sides, 25 mm roof (40 mm turret roof from Mar'44), mantlet 110–200 mm. https://en.wikipedia.org/wiki/Tiger_I
- [alanhamby] Tiger I Information Center, Technical Specs. http://www.alanhamby.com/technical.shtml
- [wiki-t34] Wikipedia, "T-34." https://en.wikipedia.org/wiki/T-34
- [te-t3485] Tanks Encyclopedia, "T-34-85" — hull 45 mm @ 60°, side 45 mm, turret 90 mm face / 75 mm side / 52 mm rear. https://tanks-encyclopedia.com/ww2/soviet/soviet_t34-85/
- [WT-pen][WT-overmatch][WT-ricochet] War Thunder damage model conventions (DeMarre pen, overmatch ≥3× suppresses ricochet, AP ricochet onset ~70° from normal) — community wiki/dev docs, used as the gameplay-model reference. https://wiki.warthunder.com/

All combat numbers are game-tuning targets, not ballistics-lab claims; see §14 for what is fitted vs measured.
