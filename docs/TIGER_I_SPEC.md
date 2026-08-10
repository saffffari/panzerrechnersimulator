# Tiger I (Pz.Kpfw. VI Tiger Ausf. E, Sd.Kfz. 181) — Modeling Spec

Authoritative reference for the player/hero tank. Compiled from user research (Jentz/Doyle-derived,
cross-checked). **Contested numbers are flagged** — don't "correct" them silently. Setting: Stalingrad
breakout, **winter 1942–43** → Soviet opponents are T‑34/76, KV‑1/KV‑1S, T‑70, towed 45 mm & 76 mm.
No T‑34/85, IS‑2, or SU/ISU assault guns yet (all 1944). The Tiger is a near-frontal god here by design.

## Dimensions & weight
- Crew 5: commander, gunner, loader (turret); driver, radio-op/bow-gunner (hull). See [[CREW_SEATS.md]].
- Combat weight ~57,250 kg (transport ~52,250). Length w/ gun 8.45 m, hull 6.32 m. Width 3.72 m
  (combat track) / 3.14 m (transport track). Height: hull roof 1.78 m, cupola top 3.00 m.

## Mobility
- Engine: Maybach HL 210 P45 (650 hp @3000 rpm, first 250 units) → HL 230 P45 (700 hp @3000 rpm,
  from ~May 1943). ~11–12 hp/tonne → ponderous; ~25–35 s to road top speed (ESTIMATE from p/w).
- Top speed ~45 km/h optimal (governed/sustained ~38–40); cross-country ~20–25, less in rough.
  8 fwd gears ≈ 2.8/4.2/6.1/9.1/14.0/20.8/30.1/45.0 km/h. Reverse 4 gears, top ~10–12 km/h (ESTIMATE).
- Range poor: ~110–120 km road, ~60–80 cross-country. Heavy fuel use (fold into attrition tension).
- **Driving:** steering WHEEL (Henschel L 600 C regenerative hydraulic) + Maybach-Olvar semi-auto
  pre-selector 8-spd. Easy/agile for its weight. **True neutral steer** — pivots in place, turning
  circle 3.44 m. Driver vision very limited (visor + periscope); drives on commander's intercom.

## Suspension & tracks (see [[MOVEMENT_SUSPENSION.md]])
- Torsion-bar, interleaved/overlapping road wheels (Schachtellaufwerk): smooth ride, great weight
  distribution, low ground pressure (wide 725 mm combat track). Front sprocket, rear idler.
- **Freeze-jam vulnerability:** packed mud/snow froze overnight & jammed wheels → Soviets attacked at
  dawn. Keep as flavor. Transport track 520 mm (rail loading).

## Turret traverse & elevation — KEY GAMEPLAY HOOK
- Traverse is **hydraulic, powered off the engine** (Boehringer-Sturm L4) → **speed depends on engine
  RPM**:
  - early: capped **6°/s**.
  - later high-speed gear: up to **19°/s @2000 rpm**, **>36°/s @3000 rpm** max.
  - foot-pedal proportional: as fine as **0.1°/s** for precise lay. 360° ≈ 60 s at idle.
  - **HOOK: to slew fast the driver must REV the engine — loud, betrays position.** Core tension for a
    stealth breakout. Manual handwheel backup ~700 turns/360° (engine-off only).
- Elevation: fully **manual handwheel**, **+15° / −8°**, ~13 turns per 10°.
- **Gun travel-lock engaged when moving → effectively CANNOT fire on the move** (no stabilizer).

## Optics & fire control
- Early **TZF 9b**: binocular, **fixed 2.5×**, ~23° FOV (sources 23–26°; use 23–25°). Reticle to
  4000 m (main) / 1200 m (coax), stadiametric 7-triangle ranging.
- Late (1944) **TZF 9c**: monocular, selectable **2.5× / 5×** (FOV ~12–13° at 5×). ← our dual-power model.
- Gun fired by **electric primer** → instant, no mechanical lag. Gunner also fires coaxial MG34 (foot
  trigger). Gunner has tunnel vision (narrow sight FOV only).
- Commander: cupola (5 vision slits early "drum" → 7 periscopes cast cupola from mid-1943), 360°
  observation, SF14Z scissor scope option. **No gun override, no driving controls** — directs only.
- Loader: manages ammo, hand-traverse wheel, close-defense weapon (1943+). Radio-op: bow MG34 in ball
  mount (KZF 2 sight, ±15°), FuG 5 radio.

## Main gun: 8.8 cm KwK 36 L/56
- Ammo cap up to 92× 88 mm (hull sponson bins) + ~4,500–5,700× 7.92 mm for the two MG34s.
- Reload 6–16 s by turret position/bin → model ~6 s ready rack, up to ~15 s far bins (6–8 rpm).
- Pen vs armor sloped 30° from vertical (German standard), Jentz-derived:

| Shell | Type | Mass | MV | @100m | @500m | @1000m | @1500m | @2000m |
|---|---|---|---|---|---|---|---|---|
| **PzGr 39** | APCBC-HE (standard AT) | 10.2 kg | **773 m/s** ⚠ | 120 | 110 | 100 | 91 | 84 |
| **PzGr 40** | APCR (tungsten, SCARCE) | 7.3 kg | 930 m/s | 171 (≈201 @PB) | 156 | 138 | 123 | 110 |
| **Gr 39 HL** | HEAT | 7.65 kg | 600 m/s | ~90 at ALL ranges | | | | |
| **Sprgr** | HE (anti-soft, ~0.9 kg filler) | 9.0 kg | 820 m/s | n/a | | | | |

- ⚠ PzGr 39 MV contested: **773 m/s** (majority, KwK 36) vs 800 (Flak); likely AP/HE swap in the
  original Datenblatt. **Use 773.** PzGr 39 is APCBC-**HE**: base fuze detonates after penetrating
  ≥30 mm → model post-pen spall/burst, NOT a solid slug. PzGr 40 tungsten chronically scarce → give
  the player only a handful (good tension, historically right).
- Realistic 92-round mix: ~half PzGr 39, ~half Sprgr HE, a few PzGr 40, a few HEAT.

## Armor — slab-sided, raw thickness, RHA (NOT face-hardened)

| Surface | Thickness | Angle from vert. | Notes |
|---|---|---|---|
| Driver's front plate | 100 mm | ~9° | |
| Nose (lower front) | 100 mm | ~25° | |
| Upper glacis | 60 mm | ~80° | only steep plate |
| Hull side upper (superstructure/sponson) | 80 mm | 0° | |
| Hull side lower | 60 mm | 0° | behind running gear |
| Hull rear | 80 mm | ~9° | |
| Turret front | 100 mm | 0° | |
| Gun mantlet | 100–120 edges, ~200 at gun | curved | ≥200 in places |
| Turret sides & rear | 80 mm | ~0° | |
| Roof (hull & turret) | 25 mm | horiz | turret roof → 40 mm ~Sept 1943 |
| Belly | 25 mm | horiz | |

- RHA, BHN ~255–280, electro-welded interlocking/stepped joints. British tests: 80 mm upper side
  resisted ≈90 mm British RHA; 60 mm lower side ≈80 mm → **model effective protection slightly ABOVE
  nominal.**

## Matchup (1942–43) — what's bulletproof, what kills you
- T‑34/76 F‑34 (BR‑350A) pens only ~60–70 mm @500 m → Tiger **100 mm front immune at all ranges**,
  **80 mm sides/rear immune except close (<~500 m)**. The 88 shreds T‑34 front from 1500 m.
- Real threats in-era: **flank/rear close shots, mobility kills (tracks/wheels), soft spots, swarming
  in rubble, immobilization, running out of fuel/ammo.** NOT frontal duels.
- **Weak spots to model:** turret ring / mantlet base (jams traverse — no ring protector until Feb
  1944), roof & belly 25 mm (plunging fire/mines/grenades), tracks & interleaved wheels (mobility +
  freeze-jam), vision blocks/cupola/barrel, and systemic: gunner tunnel-vision + engine-revs-to-traverse.

## The three quirks that make it FEEL like a Tiger
1. **Engine-RPM-dependent traverse** (rev to slew fast = loud).
2. **Cannot fire on the move** (travel lock).
3. **Electric primer** (instant fire, no lag).

## Reconcile with current code (TODO when wiring)
Present `BeginPlay` values differ from this spec and should be reconciled:
- Pen curves use @0 m anchors (APCBC 145, APCR 200) vs spec @100 m (120 / 171). Re-anchor to the table.
- Armor layout in `KesselTankPawn` is a 5-face simplification (Front/Side/Rear/Roof/Floor) — expand to
  the per-plate table above (driver plate vs nose vs glacis; turret vs hull; upper vs lower side).
- Traverse fixed 6°/s now (CONTROLS.md) → add RPM-linked curve (6 → 19 → 36°/s) once movement lands.
- Add: travel-lock (no fire while moving), electric-primer (already instant), HEAT shell (4th type).
