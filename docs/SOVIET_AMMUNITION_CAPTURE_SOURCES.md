# Soviet ammunition comparison: source and model boundaries

The impact gallery is a controlled **game-model comparison**, not a reconstruction of a
particular casualty, a proof of historical penetration limits, or a validated medical simulation.
Use the same target pose, plate, angle, range and fixed seed when comparing projectiles. Reset the
target before each shot; do not select seeds for casualties or weaken the armor with warm-up hits.
Increasing caliber is a useful presentation order, but is not a universal ranking of penetration
or internal damage. A 57 mm anti-tank projectile can outperform a 76 mm one against armor.

The turret-side review uses an **isolated turret OBB**, not the first contact of the complete
vehicle. The existing Tiger hull envelope reaches 2.595 m high and ±2.061 m wide because it encloses
the visible hull assembly, including attachments and tracks. Its empty upper corners can intercept
a head-height side ray before the narrower turret. The review therefore selects the existing
`ArmorPart::Turret` through `swept_vehicle_armor_part_contact`, verifies `TurretSide`, then invokes
the unchanged common terminal resolver. The whole-vehicle gameplay sweep uses that same helper
for both parts and still chooses the nearest; its bounds have not been silently changed here.

`debug_review_turret_side_impact` returns serializable `ReviewImpactContact` metadata with the
selected contact and the unfiltered whole-vehicle contact, part/facing, local and world positions,
normal, target line and seeds. A manifest must label this scope and record both contacts. No
pose/seed is selected for lethality. This isolates the intended plate comparison without claiming
that the coarse collision volumes are the actual armor surface or a full external-flight replay.

## New low-caliber profiles

| Runtime profile | Primary source parameters | Availability represented |
| --- | --- | --- |
| `BR_240_M42` | BR-240, 45 mm, 1.43 kg, 870 m/s; 30°-from-normal penetration rows below | M-42 towed anti-tank gun, model 1942. This is not the older, slower 45 mm tank gun. |
| `BR_271_ZIS2` | BR-271, 57 mm, 3.14 kg, 990 m/s, 18 g filler; rows below | ZiS-2 towed anti-tank gun, model 1943. This is ordinary APHE, not solid BR-271SP or subcaliber BR-271P. |

The 45 mm source is the Soviet Ministry of Defence's **1953 service manual for the model-1942
45 mm anti-tank gun**, opening penetration table and appendix 1. The inspected source distinguishes
870 m/s for its model-1942 ammunition from 760 m/s for the model-1937 gun. Its publication date does
not establish that every listed ammunition lot existed in 1942. The filler quantity was not found
in the inspected primary tables: `filler_g = 0` deliberately omits its internal burst. **It does
not mean BR-240 historically had no filler.**
[Digitized primary manual](https://djvu.online/file/MO2E3ZeyTSb68).

The 57 mm source is **GAU Red Army, short firing tables TS No. 0316, second edition, 1943**,
printed pp. 10–12. It lists the ordinary and solid projectiles separately. The same wartime book
explicitly identifies the Tiger among the targets for the separate BR-271P subcaliber projectile;
that APCR profile has not been added here.
[Digitized wartime primary tables](https://djvu.online/file/8OJbdmw0asVoM).

## Empirical fit

The manuals' 60° *encounter angle* is 30° from the plate normal. Their threshold values are not
automatically equivalent to the game's ±8% stochastic marginal band or to a different country's
test criterion. The fit uses these tabulated thicknesses in millimeters:

| Range (m) | M-42 BR-240 | ZiS-2 BR-271 |
| --- | ---: | ---: |
| 100 | 57 | 93 |
| 300 | 53 | 89 |
| 500 | 50 | 86 |
| 1000 | 41 | 78 |

The existing model has `v(r) = v0 exp(-k r)` and DeMarre exponent `N = 1.43`. Its slope factor
also depends on plate thickness, so a direct exponential fit of thickness alone would not match
the implemented solver. For each row use `B_i = t_i × slope_multiplier(shell, 30°, t_i)` and
`x_i = r_i - 100`. Keep the 100 m anchor exact, then least-squares fit
`k = sum[x_i × ln(B_100 / B_i)] / (N × sum[x_i²])`.

This gives `k = 0.0002685560278 /m` for M-42 and `0.0001478350503 /m` for ZiS-2. Solving the
model back into 30° threshold thickness yields respectively:

- M-42: 57.000, 53.052, 49.371, 41.224 mm.
- ZiS-2: 93.000, 89.423, 85.980, 77.926 mm.

These coefficients fit **penetration falloff**, not independently measured velocity, flight time,
pressure, fragment count or sound. The resulting velocity law and downstream effects remain model
approximations. Tests require the reconstructed thickness rows to stay within 0.7 mm; they do not
assert an exact historical casualty outcome. This fit covers 100–1000 m; extrapolation is not
independently established by this change.

## Existing profiles and useful gallery scope

The existing runtime constants already span 76 mm BR-350B, 76 mm APCR (`BR_354P`, designation needs
review), 85 mm BR-365, 100 mm BR-412, 122 mm BR-471 and 152 mm BR-540. The 85 mm and 76 mm APCR
constants are not selected by any current `enemy_shell` mapping; a terminal comparison can exercise
them directly without pretending a corresponding new enemy vehicle is implemented.

- **76/85 mm:** a reproduced 16 June 1944 report from the 63rd Guards Tank Brigade documents
  T-34/T-34-85 firing trials against a captured Tiger at several ranges. It supports these gun
  families and variable outcomes, not exact game-model threshold calibration. The report itself
  calls the T-34's gun “ZiS-5”; do not silently use that inconsistency to relabel the F-34.
  [Primary report reproduction and translation](https://www.tankarchives.com/2019/05/t-34s-vs-tiger.html).
- **85 mm:** the wartime short service manual for the model-1943 self-propelled gun is available
  as a primary scan, useful for subsequent per-projectile verification. The current BR-365
  profile has not received a fresh numerical calibration in this change.
  [Primary manual scan](https://ukr.bulletpicker.com/pdf/Краткое%20руководство%20службы%2085-мм%20самоходная%20пушка%20обр%201943%20г.pdf).
- **122 mm:** the GAU **1946 D-25S service manual**, pp. 53–54 and 65, distinguishes sharp-nosed
  BR-471 without a ballistic cap from blunt-nosed BR-471B with one. `BR_471.kind` is corrected
  from `Apcbc` to `Ap`; the existing 25 kg / 781 m/s identity agrees with the manual. This is
  an identity correction, not a fresh validation of its existing penetration/filler parameters.
  A ballistic windscreen alone is not an armor-piercing cap.
  [Primary D-25S manual](https://djvu.online/file/ZXosAWpyvdQl7).
- **152 mm:** `BR_540` is already the 48.8 kg / 600 m/s AP profile for ML-20S on ISU-152. A
  wartime 1945 artillery instruction book lists BR-540 for the model-1937 howitzer-gun, supporting
  the projectile/gun-family identity. Those existing numerical parameters were not recalibrated
  here. `OF_530` is currently the KV-2/M-10T HE profile and must not be relabeled ML-20/ISU-152 HE;
  a separately sourced OF-540 profile is still missing.
  [Primary 1945 artillery instruction book, table 4](https://djvu.online/file/rDko0NRdGPOtc).

The acoustic bridge and `DamageSoundContext.internal_detonation` now recognize modeled filler in
both `Ap` and `Apcbc` projectiles. Previously the uncapped Soviet APHE profiles could produce
filler-driven crew damage while being classified as dry AP by the audio path. A regression test
checks BR-271, BR-350B and BR-471 plus the explicitly uncalibrated dry M-42 profile. Nose/cap
classification and filler content are separate properties; this correction does not establish a
validated fuze, pressure-wave or internal explosion model.

Use a common 500 m reference range with optional 100 m and 1000 m repeats, labeled as selected
comparison distances. These are not asserted to be the ranges of a specific witnessed encounter.
The low-end additions are gun/projectile profiles, not newly modeled AI platforms. For a historical
campaign, device and ammunition availability still needs to be bound to each scene's place/date;
do not populate a single supposed historical battle with the entire catalog. In particular the
existing KV-2 and IS-3 game actors should not automatically be treated as ordinary late-war Tiger I
opponents. No new claim about a particular documented encounter is made by this gallery.

## Capture labels (5 September 2026)

Every incoming-contact scenario identifies a representative platform, its gun, ammunition and
selected range. The caption also says `CONTACT TEST`: these fixtures inject an isolated
turret-side terminal contact and do not record an actual firing vehicle. The scenario's
`incoming_profile.firing_platform_recorded` is therefore `false`. Existing sealed video receipts
keep the labels that were actually recorded; revised source definitions apply to new captures.

| Profile | Representative platform | Gun | Caption ammunition |
| --- | --- | --- | --- |
| `BR_240_M42` | Towed anti-tank gun | 45 mm M-42 | BR-240 |
| `BR_271_ZIS2` | Towed anti-tank gun | 57 mm ZiS-2 | BR-271 |
| `BR_350B` | T-34 | 76.2 mm F-34 | BR-350B |
| `BR_354P` | T-34 family comparison | 76.2 mm F-34 | BR-354P? APCR; runtime designation unverified |
| `BR_365` | SU-85 | 85 mm D-5S | BR-365 |
| `BR_412` | SU-100 | 100 mm D-10S | BR-412 |
| `BR_471` | IS-2 | 122 mm D-25T | BR-471 |
| `BR_540` | ISU-152 | 152 mm ML-20S | BR-540 |
| `OF_530` | KV-2 | 152 mm M-10T | OF-530 HE |

The former `fp_incoming_152mm_he` title incorrectly called its unchanged OF-530 launch profile
ML-20. It now identifies KV-2/M-10T. No velocity, filler, penetration law, fixture trajectory or
range was changed to make this label correction. The SU-85 label selects one representative
D-5S platform for the existing 85 mm profile; it does not imply that an SU-85 actor fired in the
clip. The general availability and calibration limits above still apply.
