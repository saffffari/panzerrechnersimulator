# Tiger I kit CAD: measured component envelopes for layered damage — 2 November 2026

Bounded measurement pass by an AI agent. **The game's Tiger GLBs are tessellations of this very kit
CAD, scaled 1.112 × the kit's 1:35.** Twenty-eight components are identified with evidence and given
oriented boxes in engine metres: 27 in the catalog's `components`, and one (three fuel cans on the
turret floor) under `cad_only_components` because no game mesh draws it. Everything else is listed as
not identified. Nothing under `src/` or `docs/PHYSICAL_TRUTH_AUDIT.md` was changed, nothing was
committed, no GPU was used.

Outputs:

- `catalog/tiger_cad_components.v1.json` — the compact catalog (27 components plus one CAD-only;
  boxes, volumes, envelope errors, evidence, doubts, game-mesh presence, the transform and the frames).
- `tools/bake_tiger_cad_components.py` — FreeCAD stage: every STEP solid measured, subassembly STEPs
  matched to the full assembly, tessellation, and the per-component boolean/B-rep checks.
- `tools/tiger_cad_register.py` — plain-Python stage: SolidWorks metadata, registration to the game
  hull frame, component table, envelopes, meshes and the catalog.
- `assets/fea/tiger_cad/` (gitignored): `solids.json`, `meshes_full.bin`, `sw_parts.json`,
  `registration.json`, `components.json`, `component_unions.json`, `solid_index.json`,
  `components/<id>.obj`, `assemblies/{engine_system,transmission_final_drives,gun_88mm}.obj`,
  `bake_log.json`.

## What the CAD is

`/windows/shop/ts_assets/_archive/_extracted/_Reference_CAD_Tigers/panzer-vi-tiger-1-35-1.snapshot.2/…`
holds the Rye Field Model RM-5010 Tiger I 1/35 full-interior kit CAD:

| File | Content | Solids |
|---|---|---:|
| `TG_RM5010-0A_Tiger I.STEP` (176 MB) | whole vehicle, one PRODUCT, every entity named `NONE` | 1,490 |
| `TG_RM5010-0B16_Base Unit.STEP` (89 MB) | hull subassembly | 190 |
| `TG_RM5010-0B29_EngineSystem.STEP` (11 MB) | engine subassembly | 14 |
| `Tiger-I(SW).zip` (235 MB) | 378 SolidWorks part and 25 assembly files (OLE2 documents) | — |

The STEP files are SolidWorks AP214 exports in millimetres. FreeCAD 1.1.4 reads all 1,490 solids with
no free shells or faces; one solid (id 35, a small hook at the hull rear) is an invalid B-rep whose
tessellation is garbage, and one (id 541, a 1.3 mm³ turret-roof detail) has negative volume. Both are
excluded from extents and from every component.

The STEP carries no names, but the SolidWorks part files do (377 of 378 carry properties). Its OLE2 property set
`Config-0-Properties` holds the designer's custom properties in Traditional Chinese: part name
(`零件名稱`, "ENGLISH(中文)"), part drawing number, sprue number, assembly drawing number, material
(ABS throughout), the kit pieces merged into the CAD part, and the **mass SolidWorks cached at the
last save** (`零件重量`, grams, two decimals). The kit's material density is ABS 1.02 g/cm³: the engine
block's 10,406.02 mm³ gives 10.614 g against the cached 10.61 g, and every large part agrees the same
way. That cached mass is what links a SolidWorks name to a STEP solid. It is decisive for large parts
and weak for small ones: 61 of the 1,490 solids have exactly one mass-consistent part, 977 have ten or
more. Names below therefore always combine mass with position or shape, and the evidence says which.
The zip was read in memory, never extracted; its listing equals the owner's extracted folder.

The separate subassembly STEPs are in the full assembly's own frame: the EngineSystem file is offset
by (0, 24.1, −106.2) mm, the Base Unit file not at all. All 14 engine solids and all 190 base solids
match full-assembly solids on rigid-motion invariants (volume, area and principal moments within
1e-4/1e-4/1e-3 relative; measured agreement is exact to the printed digits) and on position (0.00 mm
residual). The Base Unit file holds the lower hull, its side armour, and the driver's, fighting- and
engine-room contents including all 14 engine-system solids; it does not hold the upper hull plate,
front and rear plates, turret, running gear or tracks.

## Registration: kit millimetres to the game hull frame

### Result

```
engine_hull_m = s * kit_mm + t        rotation = identity
s = 0.038919827 m per kit mm          t = (0.000002, 0.570671, 2.657689) m
kit +X = vehicle left = engine +X     kit +Y = up     kit +Z = forward (barrel)
turret-local = engine_hull − (0, 0.524, 0.019)   (src/tank.rs TIGER_PARTS.turret_pivot, yaw 0)
```

### Method

The game's baked Tiger (`tiger_hull.glb`, `tiger_turret.glb`, `tiger_gun.glb`) turned out to be the
same solids: its 1,215 hull nodes carry a uniform node scale of 38.92 and the lower-hull node
`Hull_0619` has the kit lower hull's volume to 0.03 %. So the registration was measured on solid
correspondences, not on silhouettes:

1. per GLB node, the closed-mesh volume, area and volume centroid (divergence theorem);
2. a scale from the densest cluster of pairs whose volume ratio and area ratio give the same scale
   (s_V = (V_glb/V_kit)^⅓ and s_A = (A_glb/A_kit)^½ within 0.2 %), 3,970 of 4,419 such pairs fall in the
   0.0385–0.039 bin;
3. pairs whose volume and area agree within 1 % at that scale and are unique on both sides seed a
   fixed-seed RANSAC of a similarity transform; all invariant-compatible pairs are then assigned one
   to one by residual (< 4 mm) and refitted; finally the rotation is tested by refitting with R = I.

### Residuals (446 hull solid ↔ GLB node pairs, millimetres in the game frame)

| | x | y | z | norm |
|---|---:|---:|---:|---:|
| RMS | 0.047 | 0.069 | 0.148 | 0.170 |
| max abs | 0.52 | 0.73 | 1.41 | 1.41 (p95 0.26) |

The free-rotation fit is 0.000145° from the identity and changes the RMS by 0.0001 mm, so R = I is
used. These residuals are centroid differences between two tessellations of the same solids (the
game's mesh and this bake's 0.02 kit-mm chord), not CAD-to-vehicle errors.

The turret GLB (66 pairs) and gun GLB (9 pairs), placed with the game's own rig constants, agree with
the hull transform to RMS 0.64 mm and 0.71 mm. The turret GLB's own origin is at hull
(−0.05, 523.99, 19.27) mm, 0.27 mm from the code's pivot constant; the gun GLB's origin implies a
trunnion of (4.46, 1845.62, 1039.80) mm turret-local against the code's (4, 1845, 1040). The kit's gun
sits at 0° elevation (the gun-GLB rotation is 0.0008° from the identity).

### Cross-check from overall extents

An independent estimate from coarse features (kit tessellation extents of all valid solids against the
GLBs assembled with the game's rig constants, turret yaw 0, gun 0°):

| Feature | Kit mm | GLB m | Implied s, m/mm |
|---|---:|---:|---:|
| overall length with gun (z) | 242.040 | 9.42032 | 0.0389204 |
| overall width (x) | 105.900 | 4.12161 | 0.0389198 |
| ground to turret-roof top (y) | 77.913 | 3.03238 | 0.0389201 |
| track width, each side | 20.850 | 0.81148 | 0.0389198 |
| track centre spacing | 79.990 | 3.11320 | 0.0389198 |

| Position | Kit mm | GLB m | Residual at the fit, mm |
|---|---:|---:|---:|
| ground line (track bottom) | −14.663 | 0.0 | −0.006 |
| front extreme (muzzle) | 82.200 | 5.8571 | −0.20 |
| rear extreme | −159.840 | −3.56322 | −0.05 |
| left / right extreme | ±52.950 | ±2.0608 | 0.003 / 0.002 |
| turret ring gear top / bottom | 39.850 / 35.050 | 2.12165 / 1.93484 | −0.03 / −0.03 |
| turret ring axis x (circle fit) | 0.000 | 0.0 (game pivot) | 0.002 |
| turret ring axis z (circle fit) | −67.850 | 0.019 (game pivot) | **−2.02** |

The extents-only fit (scale from length, width and track width; translation from the ground line,
lateral centre and muzzle) gives s = 0.0389200 and t within (−0.002, 0.008, 0.19) mm of the
correspondence fit. Height uses the turret roof plate because the kit's open loader and commander
hatches stand above the game's closed ones; the kit's tallest raw point (75.46 mm) is the defective
solid 35.

**Finding — turret pivot.** The CAD's turret ring axis (circle fit to the inner rim of the 10A12 ring
gear, matched by the turret floor and basket) is at engine z = 0.01698 m. The game rotates the turret
about z = 0.019 m (`TIGER_PARTS.turret_pivot`; `tiger_interior.json` says 0.0192, `tiger_cameras.json`
0.0189). The rendered turret is placed correctly at yaw 0 (0.27 mm) but swings about an axis 2.0 mm
forward of the CAD's, so at 180° it is displaced about 4 mm. Not changed here.

### Proportions and absolute size

Proportions agree: one uniform scale, identity rotation, sub-millimetre RMS residuals on every axis
(largest single pair 1.4 mm). The
absolute size does not: **s / 0.035 = 1.1120**, so the game's Tiger is 11.2 % larger than the kit's
stated 1:35. At 1:35 the kit measures 8.47 m long with the gun (the task's 8.45 m), 6.41 m over the
hull with its shackles and exhaust guards, 3.71 m wide and 0.730 m per track; the game measures
9.42 m, 7.13 m, 4.12 m and 0.811 m. The `hull 6.316 m` figure in the brief is the real vehicle's, not
the game mesh's. Every box in the catalog is in the game's scale so that it lines up with the meshes
the game renders and collides; the catalog's `scale_note` says so.

## Components

Frames: `hull` is the engine hull frame; `turret` is turret-local (hull minus the game pivot, yaw 0).
Parts marked † elevate with the gun: their boxes are at the CAD's 0° elevation and rotate about the
turret-local x axis through the trunnion (0.004, 1.845, 1.040). Sizes are full box edges in metres
along the box axes (boxes are frame-aligned unless marked tilted). Solid and box volumes are m³; the empty
fraction is 1 − fused solid volume / box volume. Confidence: `kit_label` — the kit's own name says
what it is; `kit_label+geometry` — the name needs position or shape to pick the solid or the function;
`inferred` — the kit label does not say it.

| id | frame | centre (hull m) | size (m) | solids m³ | box m³ | empty | confidence | game mesh |
|---|---|---|---|---:|---:|---:|---|---|
| `engine_block` | hull | 0.038, 1.250, -1.951 | 1.049 × 1.335 × 1.302 | 0.6135 | 1.8229 | 0.66 | kit_label | present |
| `engine_air_filters` | hull | 0.000, 1.775, -1.894 | 0.463 × 0.253 × 0.991 | 0.0754 | 0.1159 | 0.35 | kit_label | present |
| `radiator_left` | hull | 1.148, 1.645, -2.176 | 0.996 × 0.772 × 0.321 (tilted 4°) | 0.1647 | 0.2466 | 0.33 | kit_label | present |
| `radiator_right` | hull | -1.148, 1.633, -2.176 | 0.996 × 0.749 × 0.321 (tilted 4°) | 0.1639 | 0.2392 | 0.31 | kit_label | present |
| `cooling_fans_left` | hull | 1.148, 1.620, -2.605 | 1.010 × 0.640 × 0.615 | 0.0468 | 0.3978 | 0.88 | kit_label+geometry | present |
| `cooling_fans_right` | hull | -1.148, 1.620, -2.605 | 1.010 × 0.640 × 0.615 | 0.0468 | 0.3978 | 0.88 | kit_label+geometry | present |
| `fuel_tank_left` | hull | 1.148, 1.633, -1.609 | 0.992 × 0.643 × 0.895 | 0.1178 | 0.5715 | 0.79 | kit_label+geometry | present |
| `fuel_tank_right` | hull | -1.148, 1.633, -1.609 | 0.992 × 0.643 × 0.895 | 0.1178 | 0.5715 | 0.79 | kit_label+geometry | present |
| `gearbox` | hull | -0.042, 1.058, 1.820 | 0.574 × 0.617 × 1.368 | 0.2890 | 0.4848 | 0.40 | kit_label+geometry | present |
| `steering_unit` | hull | 0.000, 1.041, 2.773 | 1.969 × 0.742 × 0.797 | 0.3309 | 1.1646 | 0.72 | kit_label+geometry | present |
| `radio` | hull | 0.021, 1.625, 1.717 | 0.360 × 0.615 × 0.955 | 0.0682 | 0.2113 | 0.68 | inferred | present |
| `final_drive_left` | hull | 1.232, 0.982, 3.010 | 0.486 × 1.024 × 0.970 | 0.0691 | 0.4833 | 0.86 | kit_label+geometry | present |
| `final_drive_right` | hull | -1.232, 0.982, 3.010 | 0.486 × 1.024 × 0.970 | 0.0691 | 0.4833 | 0.86 | kit_label+geometry | present |
| `ammo_sponson_left_rear` | hull | 1.348, 1.643, -0.547 | 0.582 × 0.623 × 1.166 | 0.1611 | 0.4226 | 0.62 | kit_label | partial |
| `ammo_sponson_left_front` | hull | 1.347, 1.660, 0.611 | 0.584 × 0.588 × 1.150 | 0.1579 | 0.3946 | 0.60 | kit_label | partial |
| `ammo_sponson_right_rear` | hull | -1.327, 1.643, -0.547 | 0.623 × 0.623 × 1.166 | 0.1638 | 0.4523 | 0.64 | kit_label | partial |
| `ammo_sponson_right_front` | hull | -1.347, 1.660, 0.611 | 0.584 × 0.588 × 1.150 | 0.1579 | 0.3946 | 0.60 | kit_label | partial |
| `ammo_floor_left` | hull | 0.568, 0.863, 0.099 | 0.381 × 0.117 × 1.203 | 0.0217 | 0.0536 | 0.59 | kit_label | present |
| `ammo_floor_right` | hull | -0.568, 0.863, 0.099 | 0.381 × 0.117 × 1.203 | 0.0217 | 0.0536 | 0.59 | kit_label | present |
| `hull_mg` | hull | -0.663, 1.768, 2.309 | 0.154 × 0.237 × 1.192 (tilted 5°) | 0.0045 | 0.0435 | 0.90 | kit_label+geometry | present |
| `driver_seat` | hull | 0.564, 0.937, 2.020 | 0.409 × 0.702 × 0.599 | 0.0134 | 0.1717 | 0.92 | kit_label+geometry | present |
| `gun_breech` † | turret | 0.004, 2.449, 0.340 | 0.564 × 0.358 × 0.693 | 0.0643 | 0.1400 | 0.54 | kit_label | present |
| `gun_cradle` † | turret | 0.004, 2.426, 0.832 | 0.669 × 0.434 × 1.047 | 0.0733 | 0.3041 | 0.76 | kit_label | present |
| `coax_mg` † | turret | -0.497, 2.453, 0.961 | 0.204 × 0.331 × 1.273 (tilted 7°) | 0.0106 | 0.0862 | 0.88 | kit_label+geometry | present |
| `gunner_sight` † | turret | 0.545, 2.521, 1.019 | 0.311 × 0.123 × 0.938 (tilted 3°) | 0.0103 | 0.0360 | 0.71 | kit_label+geometry | partial |
| `turret_traverse_motor` | turret | 0.533, 1.570, 0.036 | 0.595 × 1.162 × 0.414 (tilted 42°) | 0.0424 | 0.2856 | 0.85 | kit_label+geometry | present |
| `gunner_seat` | turret | 0.547, 1.664, 0.237 | 0.502 × 0.198 × 0.376 | 0.0116 | 0.0375 | 0.69 | kit_label+geometry | present |
| `fuel_cans_turret_floor` | turret | 0.019, 1.421, -0.533 | 0.562 × 0.486 × 0.392 | 0.0850 | 0.1072 | 0.21 | kit_label+geometry | absent (CAD only) |

Centres of turret parts are shown in the hull frame at turret yaw 0 for comparison; the catalog stores
them turret-local. "Game mesh" says whether the game's own GLBs draw the component's kit solids (next
section). Box axes, half extents, per-member solid ids, kit masses and the full evidence and doubt
texts are in the catalog.

### Identification evidence, one line each

Kit names are quoted as the SolidWorks files give them; the English glosses are translations.

- `engine_block`: 28U05 `MAIN(引擎主體)` "engine main body", EngineSystem; 10.61 g = 10.614 g; the
  largest EngineSystem solid; merges kit pieces U6/U7/U16/U21/U22/U29/U32/H35/H55/H61/H63/H64.
- `engine_air_filters`: 28U20 `FILTER TANK`, the only EngineSystem part placed twice (14 solids from
  13 files); two identical drums on the engine.
- `radiator_left/right`: 27K76/27K77 `COOLBOX(冷卻箱)` "cooling box", 2.85/2.84 g, each unique; flat
  9.3 kit-mm cores across the airflow with filler necks, between the tank shells and the fan shroud.
- `cooling_fans_left/right`: 27H13 `COVER(風扇罩)` "fan cover" with two ducts, plus the twin eight-blade
  rotor solid seated in them (0.21 g, consistent with the unlabelled EngineRoom part 27H25). The game's
  fan derivative was cut from the rotor solid's mesh (`Hull_1102`/`Hull_1103`).
- `fuel_tank_left/right`: 28H21/28H22 `COVER(油箱外蓋)` "fuel tank outer cover" plus the unlabelled
  27H12 box with a filler neck; two nested thin-walled wedge shells ahead of each radiator. The game's
  fuel-service derivative removed the left 27H12 solid (`Hull_0902`).
- `gearbox`: 21K49 `BOX(轉向機)` "steering gear", 5.00 g, unique; the longitudinal casing between the
  driver and radio operator, behind the steering unit. Named gearbox from shape and position.
- `steering_unit`: 21K54 `轉向機中` and 21K51 `轉向機後` ("steering gear, middle / rear", 1.43 / 2.57 g)
  plus 21K59 `COVER(外罩)` "outer cover" drum ends ×2; the transverse unit at the hull front.
- `radio` (inferred): 21K57 `BOX(控制盒)` "control box", 1.18 g; two stacked boxes on the gearbox
  between driver and radio operator, knob-and-plug face turned to the radio operator.
- `final_drive_left/right`: 29D56 `DRIVER` (0.58 g, ×2) coaxial with the drive sprocket between hull
  side and sprocket, plus 18-22/18-23 `COVER(前輪殼)` "front-wheel housing" (0.61 g pair).
- `ammo_sponson_*` (4 bins): 23M03 `AMMO(砲彈)` "shell" — the complete round, 0.13 g, 25.2 × 3.0 kit
  mm, 70 identical solids — 4 × 4 per bin, with 25M06 `BLACKET(砲彈架)` "shell rack" shelves and the
  24H01–24H04 `COVER(砲彈儲槽蓋)` "shell storage bin cover"; two bins per sponson.
- `ammo_floor_left/right`: three 23M03 rounds each on the hull floor beside the propeller shaft.
- `hull_mg`: two collinear rods behind the right front plate, masses of 28B-K81 and 28B-K07
  `GUN(機槍)` "machine gun" (0.05 / 0.03 g); the game's `MG Operater` node sits beside it.
- `driver_seat`: a chair with backrest (20K67 `CHAIR(椅子)`, 0.17 g) on a four-legged frame (20K65
  `CHAIR(椅座架)` "seat frame", 0.06 g) at the left front.
- `fuel_cans_turret_floor` (CAD only): three identical 0.49 g boxes on the rear of the turret floor,
  05O27 `FUEL(油箱)` "fuel tank" (merged O27–O30); 0.16 × 0.44 × 0.35 m at 1:35, a 20-litre canister's
  size. No game mesh draws them.
- `gun_breech` †: 02U03 `ROOM(供彈座)` "loading seat" (merged U3+U4), 1.11 g, unique; the solid the
  game's `tiger_gun.glb` names `Breech`.
- `gun_cradle` †: 02J07 `BASE(砲管座)` "gun-tube seat" (merged J07+U18+J31) with 02J50/02J51
  `OIL BOTTLE(油壓罐)` "hydraulic cylinder" ×2 along the barrel (recoil brake and recuperator).
- `coax_mg` †: an MG-length body (1.15 m at 1:35) right of the gun with belt links feeding it; 0.18 g
  of 03J19 `GUN(機槍)`; its axis passes the game's `TIGER_COAX_PORT`.
- `gunner_sight` †: a tube left of the gun whose ends meet the game's `TIGER_SIGHT_EYEPIECE` and
  `TIGER_SIGHT_OBJECTIVE`; 0.18 g of 01J18 `VIEWER(觀測鏡)` "sight".
- `turret_traverse_motor`: 05U13 `MOTOR(馬達)` (merged U13+U14), 0.73 g, on the turret floor beside
  the axis; the kit body includes its tall vertical drive shaft, hence the 85 % empty box.
- `gunner_seat`: a round cushion at the gunner's station, 0.20 g of 06J20B `SEAT(座墊)` "seat
  cushion"; its game mesh (`Turret_0092`) uses the game's `seats_black` material.

### Doubts that matter for damage

- **Kit labels do not always name function.** The three transmission housings are all `轉向機`;
  the gearbox/steering split is by shape and position. The radio is inferred (`控制盒`, "control
  box"). Mirror pairs (28H21/28H22, 18-22/18-23, 24H01/24H02, the carburettors and pipes) cannot be
  told apart by mass, so their left/right file assignment is open; the solids themselves are certain.
- **Kit mouldings are hollow, not casings.** The ray test (24 × 24 rays per axis; a shell is hit by at
  least half the rays on some axis and at least half of those cross two walls with under 30 % material)
  marks the fuel-tank shells (mean wall 2V/A 0.76 and 0.92 kit mm), the fan shrouds (0.65), the final
  drive hubs (0.95) and the gun cradle sleeve (0.77) as hollow. Those numbers are 1/35 plastic, not the
  real casing (×35 they would be 23–33 mm), and the kit models no fuel, coolant, oil or gears; the
  catalog repeats this under `kit_shell`. The engine block is nearly solid plastic: five small internal
  void shells total 67.7 kit mm³ (0.65 % of it). No wall thickness is given for anything else.
- **The 70 rounds are the kit's, not the vehicle's stowage.** The CAD has 4 × 16 in the sponsons and
  2 × 3 on the floor.
- **The fuel tanks**: the kit shows two shells per side; whether that space is one tank or two, and
  whether further tanks existed elsewhere, is not in the CAD.

## Presence in the game's meshes

The game's GLBs are this CAD, but not all of it. For every component solid the fraction of its
tessellation vertices with a game-GLB vertex in the same or a neighbouring 4 mm cell was measured
(GLBs assembled with the game's rig constants); where that was ambiguous (0.05–0.8, large flat game
triangles have few vertices) it was replaced by the fraction of 400 sampled vertices within 2 mm of a
game triangle. Result:

- **present** (≥ 0.8) for 22 components, including both fan shrouds (0.64 by vertex proximity,
  1.00 by the exact check);
- **sponson ammunition bins, partial**: `tiger_hull.glb` omits all 64 sponson rounds
  (`tiger_rack_source.json`: cartridges excluded from the static hull) and the four bin covers
  24H01–24H04; the shelves are present. The game draws rounds at 64 of its 65 `tiger_interior.json`
  shell slots, and those slots lie on these rounds' axes (lateral offset ≤ 5.5 mm; the slot point sits
  74 mm along the axis from the solid centroid). The bins' envelopes therefore coincide with rounds the
  game does draw; the covers are kit-only;
- **gunner's sight, partial**: about a third (0.35) of the kit solid's surface lies within 2 mm of
  game geometry; the game's sight differs from the kit part;
- **fuel cans on the turret floor, absent**: no game mesh draws them. A damage volume there would be a
  hidden primitive with no visible geometry (CLAUDE.md physical truth rule 3), so the catalog keeps
  them apart under `cad_only_components`.

## Not identified

| What | Why |
|---|---|
| radio operator's, commander's and loader's seats | kit seat parts share masses with many small parts; not located with evidence |
| two unlabelled boxes on the floor ahead of the engine bulkhead (24H37, kit mm (±7.5, 9.2, −86)) | no description, no distinctive shape |
| hydraulic reservoirs 19K15 / 20H65 `油壓罐` in the driver's compartment | found by mass and position; their system is not stated |
| engine ancillaries (exhaust manifolds, cylinder-bank `COOLBOX` 28U01/28U02, pipes, carburettors, 28U23/28U31) | named per solid in `assemblies/engine_system.obj`, no damage boxes (they sit on the engine block's box) |
| further fuel tanks, batteries, traverse oil reservoir, MG ammunition bags | not distinguishable kit parts, or not located with evidence |
| solid 35 (invalid B-rep), solid 541 (negative volume) | defective STEP bodies, excluded |

## Against the current coarse module stations

`src/game.rs ModuleKind::station` (hull metres) against the measured box centres (turret parts at
yaw 0). This is for the damage-model author; nothing was changed.

| Module | Station | Measured centre | Note |
|---|---|---|---|
| Engine | (0.00, 1.05, −2.40) | engine_block (0.04, 1.25, −1.95) | 0.45 m further forward |
| Transmission | (0.00, 0.75, 2.20) | gearbox (−0.04, 1.06, 1.82); steering_unit (0.00, 1.04, 2.77) | |
| Fuel | (0.70, 0.95, −1.70) | fuel tanks (±1.15, 1.63, −1.61) | one each side, higher and outboard |
| AmmoRack | (0.00, 0.85, −0.10) | sponson bins (±1.35, 1.64–1.66, −0.55 / +0.61); floor (±0.57, 0.86, 0.10) | |
| Radio | (−0.55, 1.15, 1.50) | radio (0.02, 1.63, 1.72) | on the gearbox, centreline |
| HullMg | (−0.75, 1.05, 1.90) | hull_mg (−0.66, 1.77, 2.31) | |
| MainGun | (0.00, 1.90, 1.00) | gun_breech (0.00, 2.45, 0.34); gun_cradle (0.00, 2.43, 0.83) | |
| CoaxMg | (−0.20, 1.90, 0.95) | coax_mg (−0.50, 2.45, 0.96) | |
| GunnerSight | (0.40, 1.95, 0.90) | gunner_sight (0.55, 2.52, 1.02) | |
| TurretTraverse | (0.00, 0.90, 0.00) | turret_traverse_motor (0.53, 1.57, 0.04) | |
| Left/RightTrack | (±1.75, 0.50, 0.00) | track runs centred at ±1.557 m, 0.811 m wide | |

## Envelope error

Boxes are fitted to the tessellated vertices in the component's frame: the smallest of a frame-aligned
box, a PCA box and 1° sweeps about each frame axis, keeping the frame-aligned box unless a rotated one
is more than 3 % smaller. For all 28 boxes:

- largest tessellated vertex outside its box: 0.000 mm (by construction);
- largest exact B-rep excursion past a box face, measured in FreeCAD with OCC optimal bounds on the
  untriangulated surfaces in each box's frame: 0.38 mm (cooling fans); 0.30 mm gunner seat, 0.16 mm
  driver seat, every other component ≤ 0.07 mm (a point's distance from the box is at most √3 times
  the per-face figure, 0.66 mm). The tessellation chord tolerance was 0.02 kit mm = 0.78 mm in the
  game frame, which bounds the excursion as expected;
- overlap between a component's own solids (boolean fuse): 3.27 kit mm³ in the steering unit, 0.32 in
  each fan pair, 0.11 in the driver's seat, otherwise below 0.001; the catalog's solid volume is the
  fused volume;
- empty fraction (1 − fused solid / box): 0.21 (fuel cans) to 0.92 (driver's seat); 0.33 for the
  radiators, 0.40 for the gearbox, 0.66 for the engine block, 0.6 for the ammunition bins, 0.79 for the
  hollow tank shells. A box is a conservative hit volume; this fraction is how much of it is not kit
  material, and for the hollow shells it is mostly the tank's interior.

These boxes are a geometric simplification derived from the source geometry with a measured error
bound, as `CLAUDE.md` requires; the change that uses them in the damage model should name them in
`docs/PHYSICAL_TRUTH_AUDIT.md`.

## Meshes and names for exploded views

Per-component OBJ files (`assets/fea/tiger_cad/components/<id>.obj`, component frame, engine metres)
carry one object per solid named `component/kit part/solid_NNNN`. Three assembly files carry named
objects for exploded-view line drawings:

- `assemblies/engine_system.obj` — all 14 EngineSystem solids: `28U05_MAIN`, `28U20_FILTER TANK` ×2,
  `28U01_COOLBOX`, `28U02_COOLBOX`, `28H43_EXHAUST`, `28H46_EXHAUST`, `28U35_BOX`, and the mass-ambiguous
  `28U09_PIPE|28U10_PIPE` ×2, `28U37_CARBURETOR|28U43_CARBURETOR` ×2, `28U23_BASE|28U31_CONVERTER` ×2;
- `assemblies/transmission_final_drives.obj` — gearbox, steering unit and drum covers, radio, both
  final drives (10 solids);
- `assemblies/gun_88mm.obj` — breech ring, cradle, recoil cylinders, recoil guard, mantlet, mantlet
  base, three barrel sections and the muzzle brake (11 solids, turret frame).

Elsewhere the STEP gives no names. `solid_index.json` lists, for every solid, the SolidWorks parts its
mass is consistent with, its GLB node and its component: usable as names only where one candidate
remains or position settles it. The engine block is one merged kit body, so an engine exploded view
shows the kit's 14 parts, not the real engine's internals.

## Other findings

- `catalog/tiger_workshop_cooling.v1.json` calls the −X fan assembly (`Hull_1102`) `left` and the +X
  one `right`. With +X = vehicle left — confirmed here by the driver's seat, `Drivers Hatch Pivot`
  (+X) and the hull MG and `MG Operater` (−X), and used by `catalog/tiger_fuel_service.v1.json` — those
  two labels are swapped.
- The game's turret pivot is 2.0 mm forward of the CAD's ring axis (above).

## Commands and run time

```sh
# stage 1, FreeCAD: about 3 minutes (168 s and 186 s in the last two runs, the second with another
# FreeCAD job sharing the CPU): full STEP read 24 s, its 1,490 solids measured 45 s and tessellated
# 70 s; Base Unit 28-38 s and EngineSystem 4-9 s (exact B-rep principal boxes, no tessellation)
FreeCADCmd tools/bake_tiger_cad_components.py < /dev/null
# stage 2, plain Python + numpy, runs the FreeCAD union stage itself: 140 s (union 57 s; the
# component stage, 41 s, runs twice to merge the union measurements)
python3 -I tools/tiger_cad_register.py
```

Defaults point at the owner's archive and baked GLBs; `TIGER_CAD_DIR`, `TIGER_CAD_OUT`,
`TIGER_CAD_CHORD_MM`, `TIGER_CAD_ANGLE_DEG`, `TIGER_CAD_FILES`, `TIGER_CAD_MESH` (FreeCAD stage) and
`--cad-dir`, `--baked`, `--out`, `--catalog`, `--freecad`, `--stage` (stage 2) override them.
Tessellation: chord 0.02 kit mm, angular deflection 20°, 5.2 million triangles (`meshes_full.bin`,
94 MB). Three FreeCAD bakes produced a bit-identical `meshes_full.bin`; two final stage-2 runs produced
byte-identical outputs (all 39 files including the catalog).
