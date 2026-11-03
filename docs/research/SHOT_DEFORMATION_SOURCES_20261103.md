# Soviet AP shot deformation and break-up: sourced facts for the voxel shot model

Compiled 2026-11-03. Primary = archival or test report. Secondary = a blog or book quoting a primary source. **UNSOURCED** = not found; do not treat as fact.

**Key sources**
- [W589] C. A. Riddle, *Metallurgical Examination of Soviet 76mm APHE Projectile Mod. BR-354B*, Watertown Arsenal Lab. Rpt WAL 762/589, 10 Apr 1953 (DTIC AD014698). https://archive.org/download/DTIC_AD0014698
- [W582] C. A. Riddle & A. Hurlich, *Metallurgical Examination of Soviet 45mm, 57mm and 85mm APHE Projectiles*, WAL 762/582, 6 Aug 1952 (DTIC ADA443218). https://archive.org/download/DTIC_ADA443218
- [TA-shape] Tank Archives (P. Samsonov), "Shell Shape", 29 Nov 2013, quoting the GAU KA reply to Malyshev (1942), CAMD RF 81-12042-85. https://www.tankarchives.com/2013/11/shell-shape.html
- [TA-cmp] Tank Archives, "Comparing Shells", 2 Mar 2022, quoting the GBTU report of 13 May 1945, CAMD RF F.38 Op.11355 D.510. https://www.tankarchives.com/2022/03/comparing-shells.html
- [TA-IS2] Tank Archives, "IS-2 vs the German Big Cats", 10 Mar 2013 (CAMD RF 38-11377-12, among others). https://www.tankarchives.com/2013/03/is-2-vs-german-big-cats.html
- [NOG] US Naval Academy, *Naval Ordnance and Gunnery* Vol. 1, ch. 4, NavPers 10797-A, 1957. https://eugeneleeslover.com/USNAVY/CHAPTER-4-B.html
- [Ipson] T. W. Ipson, R. F. Recht & W. A. Schmeling, *Effect of Projectile Nose Shape on Ballistic Limit Velocity, Residual Velocity and Ricochet Obliquity*, NWC TP 5607, 1973 (DTIC AD916925).
- [Hitch] H. P. Hitchcock, *Effects of Ricochet on the Motion of Projectiles*, BRL Rpt 629, 1947 (DTIC AD479824).
- [Ban] B. Banerjee, "The Mechanical Threshold Stress model for various tempers of AISI 4340 steel", arXiv:cond-mat/0510330, 2005.
- [Chi] Y. C. Chi, S. H. Lee, K. Cho & J. Duffy, "Effects of tempering and test temperature on the dynamic fracture initiation behavior of an AISI 4340 VAR steel", Brown Univ./ARO 1988 (DTIC ADA201606); also *Mater. Sci. Eng. A* 114 (1989) 105–126.
- [MK] H. W. Meyer & D. S. Kleponis, *An Analysis of Parameters for the Johnson-Cook Strength Model for 2-in-Thick RHA*, ARL-TR-2528, 2001 (DTIC ADA392414).
- [JC83] G. R. Johnson & W. H. Cook, Proc. 7th Int. Symp. Ballistics, 1983, pp. 541–547. [JC85] Johnson & Cook, *Eng. Fract. Mech.* 21 (1985) 31–48. The JC85 values below come from the table in Zhou, Wang & Wen, *Chin. J. High Pressure Phys.* 33 (2019) 042101, which cites JC83/85.

## 1. Soviet projectiles

| Shell | Mass | Filler | Nose | Source |
|---|---|---|---|---|
| BR-350A (76 mm) | 6.3 kg | **UNSOURCED** | blunt with ballistic cap (reader comments only) | Wikipedia F-34 page, citing Zaloga 1984 (secondary) |
| BR-354B (76 mm) | 13.95 lb (6.33 kg) | cavity Ø~1 in × 3.5 in deep, drilled in the base | **flat nose ~1 in across**, deep-drawn windshield, 2 circumferential notches | [W589] |
| BR-354P (76 mm) | — | — | This is the **HVAP/APCR** sub-calibre round, not APHE ("76mm Projectile, HVAP MOD UBR-354-P") | [W589] App. A |
| BR-350B (76 mm) | **UNSOURCED** | **UNSOURCED** | — | — |
| BR-365K (85 mm) | not given | — | **sharp tangent ogive, 1.45 cal radius**, 1 notch below the bourrelet | [W582] |
| BR-365 (85 mm) | 9.2 kg (generic 85 mm shell weight) | **UNSOURCED** (conflicting figures online) | blunt with ballistic cap (paywalled secondary database) | Wikipedia 52-K; fenixinsight |
| BR-471 (122 mm) | 25.0 kg | 0.156 kg | sharp-nosed; MD-8 fuze | Wikipedia A-19, citing Shirokorad 2000; Potapov (losthistory.net) |
| BR-471B (122 mm) | 25.0 kg | **UNSOURCED** | blunt with ballistic cap; "from early 1945" | same sources |

**Steel and hardness (primary sources, measured on captured shot):**
- **76 mm BR-354B:** 0.34% C and 1.54% Si, a Si-Cr steel. Hardness averages Rc 42 from the nose to the band, then drops to **Rc 16.5 at the base**. Across the section it falls from **Rc 47 at the surface to Rc 37.5 at the centre**. No zone is fully martensitic: the nose has bainite and ferrite in martensite, and the base is ferrite and pearlite (air-cooled). The report concludes: "the shot would probably tend to fracture in a brittle manner upon impact against moderately severe targets" [W589].
- **85 mm BR-365K:** C 0.32, Mn 0.92, Si 1.41, Cr 1.17, Ni 0.22, Mo 0.03. Hardness averages **Rc 47 from the nose to the lower band, then falls to Rc 26 at the base**, and runs Rc 50 at the surface to Rc 44 at the centre. The report found "brittle structures in the base" [W582].
- **45/57 mm:** uniform hardness of Rc 52 and 54 [W582].
- Soviet shot carbon was 0.32–0.38%, against 0.50–0.60% in US shot. Soviet maximum hardness was therefore Rc 50–55, against Rc 60 for US shot [W582]. This fits the GAU's statement that Soviet shot used the "more ductile" steel 35KhGS [TA-shape].
- Windshield: rimmed low-carbon steel, about 205 BHN, crimped on. Band: pure copper, about 127 BHN, knurled seat, "difficult" to remove [W589].

**Notches, and how designers expected the shot to break.** The notches were meant to "localize fracture during armor penetration to the notched zone" [W582]:
- Ogival shot (85 mm): the nose breaks off on highly sloped plate, "permitting the blunt body section which remains to punch through." Intact noses "tend to ricochet." At normal or low obliquity "the bending moment may be insufficient to fracture the shot."
- Blunt shot: the notch keeps fracture in the solid nose and leaves the explosive cavity intact for a high-order detonation [W582, W589].
- US tests of notched shot showed no overall advantage [W589].

**Expected performance.** The 45/57 mm blunt shot should do well against 1–2 in plate at up to 60°, but "very poor against 3" and 4" armor" [W582]. The 85 mm shot was rated close to the US 90 mm T33 against 3–4 in plate at 0–30°, and worse against more severe targets [W582]. Blunt shot perforate "by the plugging mechanism", while sharp ogival shot "tend to ricochet off ... highly sloped armor" [W589].

**Why the BR-471B has a blunt nose.**
- In 1942 the GAU KA argued that blunt shot survive impact better and that a sharp shot is less rigid at an angle. It recommended a blunt section about 0.7 calibre long [TA-shape].
- On 13 May 1945 the GBTU reported that, for 122 mm, "flat-tipped shell clearly better" than sharp-tipped, both in stock and in experimental steel. The 100/122 mm flat-tipped shells were accepted into service before full firing trials [TA-cmp].
- The story that a BR-471 shattered on a Panther glacis and so prompted the blunt design is a **forum claim and UNSOURCED**.

**Trials against Tiger and King Tiger** [TA-IS2]:
- A BR-471 hit on a Tiger turret roof ricocheted, leaving a 25 mm dent and through-cracks.
- On the King Tiger upper glacis, "AP flat" with a reduced charge (2700 m equivalent) ricocheted and left a 60 mm dent. At 500 m the same shell left a 100 mm dent but did not penetrate. "AP pointed" penetrated at 600 m and only dented at 700 m.
- None of these reports mention shell break-up.
- A Tank Archives post on 85 mm guns describes a D-5 hit on the King Tiger front plate at 300 m as a "dent ... including remains of the shell that welded itself to the armour". https://www.tankarchives.com/2013/03/soviet-85-mm-guns-vs-tigers.html

## 2. How AP shot behaves against plate

- **Hardness and shatter.** US practice hardened shot to Rc 60 or more, then base-tempered it to about Rc 59 at the bourrelet and Rc 45 at the base. "Lowering the hardness of shot noses below Rockwell C 60 results in more extensive shatter ... as well as a reduction in the velocity of impact at which shatter initiates" [W582, W589]. The preferred method was full martensite with a differentially tempered base, so that "body and base section remain intact" [W589].
- **German shot.** Captured uncapped German shot had sharp noses at 62–65 Rc [TA-shape].
- **Nose upset (mushrooming).** In 0.50 cal tests, Rc 53 4340 projectiles showed "no deformation". Rc 30 projectiles deformed and "expend much more energy", so they need a much higher velocity to perforate [Ipson]. There is no WWII full-calibre measurement; that remains **UNSOURCED**.
- **Shatter gap** (failure in a velocity band above the ballistic limit): reported for US 76/90 mm against Panther plate only in **secondary or forum sources**. Bird & Livingston (2001) reportedly treat it. The quoted Navy figures (76 mm APCBC, 3 in plate at 30°: V50 about 2069 fps, failures 2073–2376 fps) are **unverified**. No WAL shatter-gap report was retrieved.
- **Cap and windshield.** The AP cap "increases the biting angle", "spreads the shock of impact over the periphery of the nose", and "pre-stresses the armor plate ... before the cap shatters away". The windshield is a light false ogive [NOG]. Post-1930 USN hard caps measured 500–625 BHN at the face (N. Okun, navweaps tech-085).
- **Bending at obliquity.** A tough body is required because of "the violent transverse stress caused by crashing through armor plate at an angle" [NOG]. See also the notch mechanism above.
- **Fracture into 2–3 pieces, driving band stripping, mass loss on perforation:** no quantitative source found; **UNSOURCED**.

## 3. Material data

- **4340 yield (σ0) vs hardness** [Ban, fitted to ASM Handbook data]: σ0 = exp(0.0355·Rc + 5.5312) MPa, with B/σ0 = 0.6339, n = 0.26, C = 0.014 (at ε̇0 = 1/s), m = 1.03 (298–1040 K), Tm = 1793 K. Data cover Rc 30–49, so anything outside that range is extrapolation. Computed values: Rc 17 → 462 MPa; Rc 30 → 732; Rc 45 → 1247; Rc 47 → 1339; Rc 60 → 2124.
- **JC strength, 4340 at Rc 30** [JC83]: A = 792 MPa, B = 510, n = 0.26, C = 0.014, m = 1.03.
- **JC fracture, 4340** [JC85 via Zhou 2019]: D1 = 0.05, D2 = 3.44, D3 = −2.12. D4 = 0.002 and D5 = 0.61 are commonly quoted but **not verified here**.
- **Dynamic fracture toughness, 4340 VAR** [Chi], at a loading rate of about 2×10⁶ MPa√m/s. Tempers gave Rc 55, 49 and 45.
  - Rc 55 temper: K_Id rises from 47 MPa√m at −140 °C to **69 MPa√m at room temperature** and 94 at +100 °C.
  - Dynamic toughness is about 40% above quasi-static at room temperature for the Rc 49 temper.
  - Ravichandran & Clifton measured 62 MPa√m by plate impact at Rc 52.
  - Derived energy release rate G = K²(1−ν²)/E, with E = 205 GPa and ν = 0.29 **assumed**: 69 MPa√m gives about 21 kJ/m², and 47 MPa√m gives about 10 kJ/m².
- **RHA** [MK]: quasi-static yield 0.82 GPa at 38 mm and 0.69 GPa at 101.6 mm. Recommended JC for 2-in RHA: A = 0.78 GPa, B = 0.78 GPa, n = 0.106, C = 0.004, m = 1.00. This is modern US MIL-A-12560 plate, not German steel. German thick plate ran about 260 BHN ("The Chieftain's Hatch: US Guns, German Armour Pt 1", Wargaming, n.d.; secondary).
- No source found for fracture strain or K_Ic of RHA or of Soviet 35KhGS shot; **UNSOURCED**.

## 4. Ricochet and spin

- **Critical ricochet angle for full-calibre AP against thick RHA: no numeric value found; UNSOURCED.** The qualitative evidence is that blunt shot outperform sharp shot at high obliquity [W589, TA-shape]. The 45/57 mm blunt shot were expected to work up to 60° against undermatching plate [W582].
- **After ricochet** [Ipson]: tests used rigid Rc 53 shot against 230 BHN plate at 30°. At low velocity the shot leaves almost parallel to the surface; as velocity approaches V50 the exit angle steepens because the plate deforms. For rigid shot the exit angle is independent of nose shape.
- **Spin and tumble** [Hitch]: these were ground ricochets, not armour.
  - The impact gives the shot's axis "a large angular velocity" and yaw becomes very large.
  - The shot loses "a larger proportion of its linear velocity than its spin", so gyroscopic stability can rise after ricochet (worked example: stability factor s = 4.5).
  - Mean yaw grows with sectional density.
- **Spin rate:** ω = 2π·v / (n·d), where n is the twist in calibres per turn. **The twist of the D-25T, A-19, F-34 and ZiS-5 was not found and is UNSOURCED.** Only the groove count was found: ZiS-3 has 32 grooves and a 2570 mm rifled length (RGASPI 644-2-36, via Tank Archives, 26 Oct 2016). Illustration only, with an assumed n = 25: 122 mm at 800 m/s gives 262 rev/s; 76.2 mm at 662 m/s gives 348 rev/s.

## Recommended model parameters

| Parameter | Value | Status |
|---|---|---|
| Soviet 76/85 mm nose hardness | Rc 45–47 (surface Rc 47–50, core Rc 37.5–44) | SOURCED [W589, W582] |
| Soviet base hardness | Rc 16.5 (76 mm), Rc 26 (85 mm); brittle ferrite and pearlite | SOURCED |
| US reference shot | Rc 60 nose, Rc 59 bourrelet, Rc 45 base | SOURCED [W582] |
| German uncapped shot nose | 62–65 Rc | SOURCED (secondary) [TA-shape] |
| Shot yield vs Rc | exp(0.0355·Rc + 5.5312) MPa; nose ~1.25–1.34 GPa, base ~0.46–0.61 GPa | SOURCED fit; base value extrapolated |
| JC hardening (shot) | B/σ0 = 0.634, n = 0.26, C = 0.014, m = 1.03, Tm = 1793 K | SOURCED [Ban, JC83] |
| JC failure (shot) | D1 = 0.05, D2 = 3.44, D3 = −2.12; D4 = 0.002, D5 = 0.61 | D1–D3 SOURCED; D4–D5 unverified |
| Shot K_Id | 47–69 MPa√m (Rc 55, −140 °C to room temperature); G ≈ 10–21 kJ/m² | K SOURCED [Chi]; G derived with assumed E |
| Notch as fracture locus | one or two V-notches near the bourrelet | SOURCED [W582] |
| RHA (target) | A = 0.78 GPa, B = 0.78, n = 0.106, C = 0.004, m = 1.0; yield 0.69 GPa at 100 mm | SOURCED [MK] (US RHA) |
| Windshield / band | 205 BHN mild steel / 127 BHN copper | SOURCED [W589] |
| BR-471 / BR-471B mass | 25.0 kg; BR-471 filler 0.156 kg | SOURCED (secondary, Shirokorad) |
| BR-350A/B, BR-365, BR-471B filler | — | UNSOURCED |
| Critical ricochet angle (AP vs RHA) | — | UNSOURCED |
| Twist of D-25T / A-19 / F-34 / ZiS-5 | — | UNSOURCED |
| Shatter-gap velocity band | — | UNSOURCED (forum only) |
| Perforation mass loss, band stripping | — | UNSOURCED |
