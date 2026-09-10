# Native Soviet HE audit, 2026-09-05

The T-34/F-34 now has an OF-350 **review candidate**, selected by gun identity. Its projectile
mass and TNT filling are primary-source verified. Its muzzle velocity remains provisional and
its drag is uncalibrated. This is not a historical validation or policy promotion.

| Gun / actor | Candidate and verified data | Current limitations |
| --- | --- | --- |
| F-34 / current T-34 | OF-350, 6.20 kg including fuze, 0.621 kg TNT; explicit F-34 compatibility | 680 m/s retained from existing research, not verified in a primary velocity table; exponential drag 1.25e-4 /m is a family estimate |
| M-10T / KV-2 | Existing OF-530 model retained unchanged | Its 40 kg / 5.9 kg / 508 m/s values and drag were not revalidated in this audit |
| D-5S-85 | Primary service manual confirms O-365K percussion-fuzed shell and 785 m/s | Projectile mass/filling not verified; no new shell enabled |
| ZiS-S-53 / requested T-34-85 | No new shell enabled | No T-34-85 actor currently exists; do not give the 76 mm actor 85 mm ammunition |
| D-10S / SU-100 | No new shell enabled | Mass, filling and gun-specific velocity not primary-verified; auxiliary casemate controls also remain absent |
| D-25T / IS-2, IS-3 | No new shell enabled | Inspected D-25S manual identifies OF-471/OF-471N but does not justify blindly copying the old 800 m/s A-19-family research row into D-25T |
| ML-20S / ISU-152 | No new shell enabled | M-10T's OF-530 launch profile is not an interchangeable ML-20S charge; auxiliary casemate controls remain absent |

## 76 mm primary receipt

GAU, *Artillery ammunition: ammunition for 76-mm ground, tank and self-propelled guns*, Moscow,
Military Publishing House of the USSR Armed Forces Ministry, 1949. The digitization is at
[Bulletpicker](https://ukr.bulletpicker.com/pdf/Руководство%20боеприасы%20к%2076-мм%20орудиям.pdf).
The downloaded source is `artifacts/astra-audit-20260905/soviet-he-sources/76mm-ammunition-manual.pdf`.

- Printed pp. 29-30 describe OF-350; p. 30 explicitly gives 621 g pressed TNT and separately
  mentions wartime amatol 80/20 fillings with a TNT plug. The game candidate is the TNT variant.
- Appendix 4, printed pp. 146-149, is the F-34 cartridge table. Row 6 on printed p. 148 lists
  full-charge UOF-354M with OF-350, projectile mass including fuze 6.20 kg and filling 0.621 kg.
  PDF page indices are one lower than printed page numbers. The numerical row and appendix
  heading were inspected as page images, not only extracted text.
- The manual directs F-34 firing to tables 125T / 117, but the inspected text supplies no 680 m/s
  velocity row. The publicly listed 1943 F-34 service-manual scan link returned 404 during audit.
- The older repository research said 710 g without a precise source. That is not blended with
  the inspected 621 g variant. This audit does not establish that no other filling variant existed.

## Other inspected primary material

[GAU, short service manual for the 85-mm self-propelled gun model 1943](https://ukr.bulletpicker.com/pdf/Краткое%20руководство%20службы%2085-мм%20самоходная%20пушка%20обр%201943%20г.pdf),
ammunition list and appendix 1: O-365K with percussion fuze and 785 m/s. O-365 with T-5 time fuze
is a distinct listed configuration; neither has been substituted for the other.

[GAU, D-25S service manual, 1946](https://djvu.online/file/ZXosAWpyvdQl7), opening note and ammunition
section: D-25S versus earlier 122-mm gun velocities differ, and OF-471 / OF-471N have distinct
ballistic treatment. The incomplete audit is insufficient to enable D-25T HE.

## Runtime and model boundaries

`ballistics::native_soviet_he(SovietGun)` returns an explicit profile or `None`. It cannot return
German Sprgr. 88 as a fallback. `mixed_combat_runtime::enemy_native_he` maps all six actual enemy
kinds to their gun. The renderer-free trainer uses the same lookup, alternating F-34 and M-10T by episode seed. Per-tank telemetry
records the shell name and primary-verification / drag-calibration flags.

The new OF-350 has zero KE penetration anchor. It uses the existing HE terminal and exterior
casualty abstractions; their simplified blast law is not a validated pressure, fragment or injury
model. The candidate does not add a new fuze-delay model or imply measured dispersion/drag.
Existing policy checkpoints are unchanged and require fresh combat evaluation after weapon changes.
