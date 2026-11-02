# Engine service: primary-source and source-mesh notes, 2026-10-30

Read-only research checkpoint. No runtime, registered geometry, or asset was changed by this investigation. The engine-cover correction is the current implementation priority. Neither coolant replenishment nor a selectable fan-speed upgrade is certified as implemented here.

## Primary scans and page references

Page numbers below distinguish the printed page from the one-based PDF page. Scans were visually inspected; both PDFs are image scans, so ordinary text extraction is empty.

1. **Chassis description, February 1943 draft.** The scanned title is *Panzerkampfwagen Tiger, Ausführung E — Firmen-Gerätbeschreibung und Bedienungsanweisung zum Fahrgestell*, marked **ENTWURF**, February 1943. The host catalogs it as D 656/21, March 1943 / second edition; the scan's actual title/date take precedence. [Catalog](https://bushmakow.com/library/155-d-656_21-tiger/) · [PDF](https://bushmakow.com/wp-content/uploads/2024/05/155.-D-656_21-TIGER-compressed_bushmakow.pdf).
   - Printed 44 / PDF 81: coolant filler, circuit, drain and approximate 114 L capacity.
   - Printed 45 / PDF 82: fan shafts, disconnecting clutches, and the two-speed fan-drive selector.
   - Printed 46–47 / PDF 83–84: fan transmission lubrication and fan construction.
   - Printed 39–41 / PDF 74–76: four fuel tanks and their connecting valves.
   - Printed 82 / PDF 147: coolant checks; approximately 85°C normal temperature and investigation above 95°C.
2. **D 656/30 b und c — Pz Kpfw Tiger Ausf. E, Instandsetzungsanleitung für Panzerwarte, Motor und Triebwerk**, **1 August 1944**, confirmed by scanned title, PDF 2. [Catalog](https://bushmakow.com/library/182-d-656_30b-c-armored-combat-vehicle-tiger-design-e-repair-instruction-f-tank-servicemen-motor-a-engine/) · [PDF](https://bushmakow.com/wp-content/uploads/2024/05/182.-D-656_30bc-Panzerkampfwagen-Tiger-Ausf-E-Instandsetzungsanleitung-f-Panzerwarte-Motor-u-Triebwerk-compressed_bushmakow.pdf).
   - Printed 7 / PDF 5 right: explicitly identifies the **HL 230 P 45**.
   - Printed 49 / PDF 26 right: cooling circuit, rubber connections, filler/relief valve, fan-drive arrangement, 114 L water and 3 L oil per angle gearbox.
   - Printed 50 / PDF 27 left: actual photographs and procedure for the pressure-relief valve beneath its armored plug.
   - Printed 51 / PDF 27 right: actual photographs and removal procedure for fan universal-joint shafts.

Local full PDFs: `C:/Users/saffari/AppData/Local/Temp/panzer_coolant_manual_20261030/D656_21_chassis.pdf` and `D656_30bc_motor.pdf`. These temporary copies are not vendored game assets. The original URLs above are the durable retrieval references.

## Coolant identification: preserve the unresolved conflict

The February 1943 draft explicitly places the filler on the **left in the direction of travel**, with a safety valve on the other radiator. The August 1944 manual says **left pressure valve, right filler**, without specifying viewing direction in that paragraph. Do not silently reinterpret this disagreement as perspective, or assume a configuration change: neither explanation has been established. The early manual includes HL 230 material but is not proof that every paragraph applies unchanged to every later engine installation.

The source model's plain round protruding cap is on `Hull_1010`, beneath exterior plug `Hull_1012`; the other side has visibly different detail. Runtime **+X is vehicle left**, supported by the driver's position in `game.rs`. This geometrical identification is firm; universal historical left/right applicability remains unresolved. The source cap also does not reproduce the hexagonal filler shape described by the secondary direct survey [Tiger I Information Center radiator study](https://tiger1.info/EN/Ustar-radiators.html).

### Measured candidate split, not an enabled refill

Source: `assets/tanks/tiger/baked/tiger_hull.glb`, SHA-256 `51938f43eb90ba321ff452c2be953d06673f7aab670465835144cf8ce88c9a84`. All coordinates are transformed hull metres; triangle numbers are zero-based indices within the original root primitive.

- `Hull_1010` / node 986 contains 5,016 triangles. Welding source positions to 1 μm gives zero boundary or nonmanifold edges.
- Complete protruding cap: **62 original triangles**, ranges **0–21 and 4976–5015**. Radius **44.758 mm**; axial height **38.91982 mm**.
- Cap top center `(1.148134456, 2.027847334, -2.202930100)`; outward axis `(0, 0.999679463, -0.025317417)`.
- Removing those 62 triangles leaves 4,954 radiator triangles and exactly **20 planar boundary edges**, with no nonmanifold edges. The boundary bounds are `(1.103377, 1.987807, -2.246688)` to `(1.192892, 1.990073, -2.157201)`.
- A flat cap underside on that boundary closes a volume of **0.0002409295 m³** (0.241 L of geometric envelope, **not coolant capacity**). The measured volume used a 20-triangle centroid fan; an alternative triangulation of the same planar polygon can use 18 triangles.
- Do **not** split by merely selecting triangles above the apparent halfway ring at 19.45991 mm. Those vertices do not form a complete existing seam; this selection produces a ragged boundary.
- Proposed preparation: separate the exact complete protrusion and create its actual underside. The receiving opening and any visible inner wall/neck must then be inspected and explicitly modeled as an authored derivative. Source geometry does not establish wall thickness, threads, seal, inner neck or fluid cavity dimensions. Closing the radiator opening and permitting a pour through it would contradict the geometry.

Exact current surface evidence: `Hull_1010` triangle 5001 lies on the cap top; centroid `(1.160204415, 2.028069335, -2.194163576)`. `Hull_1012` triangle 1547 is the exterior plug above it. A vertical ray at `x=1.1481347, z=-2.203` hits plug top `y=2.069089838`, plug underside `2.028224065`, then cap `2.027845564`. The source remains closed until both real obstructions are handled.

## Fan-speed selector: historical mechanism, source control unverified

The actual paragraph is **February 1943 draft, printed 45 / PDF 82**, beginning “Am Maybachmotor befindet sich hinter dem Flansch der Lüfterantriebswelle”. It describes a small selectable gear drive behind the fan-drive-shaft flange. A lever secured by a screw bolt changes the setting **with the engine stopped**. Reported flange speeds are 3,000 or 4,500 rpm; subsequent ratios 110:124 and 70:66 produce approximately 3,580 or 5,380 rpm at the fans. The setting depends on outside temperature. This is a historical candidate for meaningful cooling versus shaft-demand tradeoffs, not evidence for a fabricated 1.15× spare-fan upgrade.

The precise selector lever has **not** been positively located in the current source. The documented engine STEP has 14 product instances / 13 unique component products; none is separately named as a lever or selector. Original SolidWorks component filenames likewise contain no independently named selector.

- Source assembly: `C:/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/panzer-vi-tiger-1-35-1.snapshot.2/panzer-vi-tiger-1-35-1.snapshot.2/TG_RM5010-0B29_EngineSystem.STEP`.
- `28U31_CONVERTER` → `Hull_1080`, 7,022 triangles, bounds `(-0.167355, 1.360569, -2.701575)` to `(0.167355, 1.666270, -2.456380)`. Its case and opposing flanges make it a **candidate** in the drive train, not a certified selector or exact fan-gearbox mapping.
- `28U35_BOX` → `Hull_0956` is a separate rectangular component; no selector identity established.
- `28U05_MAIN` → `Hull_0971` includes rear engine details. Inspection did not positively identify a selector lever embedded there.
- The August 1944 HL 230 cooling section confirms fan shafts and angle gearboxes but **does not independently repeat or illustrate this selector**. Its applicability to the present engine configuration remains unconfirmed.

## Local evidence images and receipts

All paths below are under `C:/shop/panzerrechnersystem/tmp/pdfs/coolant_manual_20261030/`; they are research scans / CPU mesh projections, not playable-game captures.

| File | Evidence |
| --- | --- |
| `chassis-002.png` | Actual February 1943 draft title/date |
| `detail-081.png` | Printed 44, explicit driving-direction filler location and coolant flow |
| `detail-082.png` | Printed 45, complete fan-selector paragraph and ratios |
| `detail-083.png`, `detail-084.png` | Fan-drive lubrication / fan construction |
| `motor-02.png`, `motor-05.png` | August 1944 title and HL 230 identification |
| `motor-cooling-26.png` | Printed 48–49, conflicting filler/relief wording |
| `motor-cooling-27.png` | Printed 50–51, actual valve and fan-shaft service photographs |
| `source_actual_left_radiator.png` | Correct +X / vehicle-left radiator source projection |
| `source_fan_gear_candidates.png` | Hull 1080 / 0956 source projections; functions remain candidates |
| `source_engine_rear.png` | Main engine rear source projection with separate 1080 omitted for inspection |
| `coolant_cap_split_measurement.json` | Complete exact cap triangle list, cut planes, bounds and volume calculations; half-height candidate is invalid as explained above |

The older filename `source_left_radiator.png` is misleading: it depicts the **negative-X / vehicle-right** side. Do not use its filename as identification evidence.

For engine-cover access only, the [Tiger I Information Center reconstruction](https://tiger1.info/EN/Opening-engine-hatch.html) gives approximately 64–72° turret right with the rearmost right spare link removed for the relevant bracket configuration. This is a measured **secondary reconstruction**, supported there by period photographs, not a recovered original-manual angle instruction. It explains why a 90° source test can collide without certifying a complete removal path.

## Checkpoint

Research and source measurements are complete for this bounded checkpoint. No refill contact, selector interaction, historical upgrade claim, source split, Cargo build or GPU run was added. Resolve the primary-source perspective/configuration conflict and exact modeled control before expanding the live thermal mechanic.
