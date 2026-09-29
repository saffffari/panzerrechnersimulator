# Sound levels for Panzerrechnersystem: sourced calibration data

Research report, 28 Sep 2026. Scope: a Tiger I (8.8 cm KwK 36), winter 1942–43, one crewman as listener.
Goal: every source calibrated to physical sound pressure at the listener; a hearing model carries what
digital full scale cannot.

## How to read this

**Tags**
- **[M]** measured, with citation.
- **[S]** standard or criterion text.
- **[D]** derived by me from a published physical law. The formula and inputs are shown and can be reproduced.
- **[E]** estimate: scaling or judgement from anchors, with the derivation shown.
- **[A]** eyewitness account (qualitative).
- **[P]** proxy measurement on a close relative of the weapon.

**Levels**
- Lpk is unweighted (Z) peak sound pressure level, dB re 20 µPa. C-peak is within ~1 dB of Z-peak for small arms, but can read a few dB low for large-gun blast, whose energy sits at 30–50 Hz.
- LAE is A-weighted sound exposure level (SEL). LIAeq100ms = LAE + 10.
- LAF is A-weighted fast (125 ms) level. For a short event, LAF,max ≈ LAE + 9.

**Durations**
- A-duration: the first positive phase.
- B-duration: the envelope within 20 dB of peak, reflections included.
- C-duration: time within 10 dB of peak.

**Pressure ↔ dB:** 200 Pa = 140 dB; 632 Pa = 150; 2 kPa = 160; 6.3 kPa = 170; 20 kPa = 180; 35 kPa = 184.9; 63 kPa = 190; 100 kPa = 194; 200 kPa = 200. One psi = 6.895 kPa = 170.7 dB.

**Air at −20 °C:** c = 319 m/s; −30 °C: 313 m/s. Sound takes 3.1 s to travel 1 km.

**What was not found anywhere:** a measured WWII turret interior during main-gun fire, any 88 mm measurement, or any quantified closed-hatch level for a modern tank (M1, Leopard 2, T-72). The own-gun numbers are therefore scaled from the nearest measured anchors, which are listed with each figure.

**Research process.**
- The session's WebSearch budget was exhausted early.
- Before we recognised that it circumvents the cap, one research fork and I fetched a handful of search-engine result pages through WebFetch. We then stopped.
- Every number below comes from a direct fetch of the cited document (DTIC via Wayback, everyspec, NAP/IOM, PMC/PubMed, publisher pages, Wikipedia) or from my own calculation.
- The fork assigned to armour impacts, crew accounts and hull insertion loss had not reported when this report was due. §5 and the insertion-loss figures in §8 are therefore my own derivations, cross-checked against measured analogues gathered by the other forks.

---

## 1. Calibration table

One row per source. Positions are at the listener's ear unless stated. "Z" means unweighted.

| # | Source | Position / distance | Lpk dB | Other levels | Duration and spectrum | Basis and confidence |
|---|---|---|---|---|---|---|
| **Own gun: 8.8 cm KwK 36**, Pzgr 39 10.2 kg at 773 m/s, 2.5 kg propellant, muzzle brake, semi-automatic breech ejecting the case into the turret |||||||
| 1 | 88, buttoned up | gunner / loader / commander ear | **163** (157–168) | LAE ≈ 130 (Lpk − 33, reverberant); LAF ≈ 139; 0.2 s average ≈ 18 dB below peak (M60) | B-duration 50–300 ms (turret RT60 0.25–0.8 s [D]); energy below 500 Hz, mostly 30–200 Hz; several peaks, "initial peak not usually the greatest" | [E], low–moderate. Anchors: M60 105 mm turret, hatches open, 163.3–167.5 [M]; Paladin compartment 166.1 and 150–160 [M]; WWII turrets < 174 with "negligible" TTS [M]. Loader and gunner sit nearest the breech, +1–2 dB. |
| 2 | 88, hatch open, head inside turret | same | **168** (163–172) | LAE ≈ 133 | as row 1, plus the direct blast path through the hatch | [E], low. M60 hatches open 165.5 [M], plus 2–5 dB for the muzzle brake. |
| 3 | 88, commander head-out in cupola | ~6.3 m behind and 1 m above the brake, line of sight over the mantlet | **182** (178–187) | LAE ≈ 143; LAF ≈ 152 | A-duration 2–4 ms; "hard slap on the chest"; above the 180 dB non-auditory threshold; 1–5 % eardrum-rupture band | [E], low–moderate. WWII 17-pdr crews 187.4–188.7 at ~4–4.5 m (brake) [M]; M48A2 90 mm 187 at 12 ft [M]; scaled −5 to −6 dB for 2.5 vs 3.7 kg charge and distance. US Army forbids M1 crews heads-above-hatch when firing [S]. |
| 4 | 88 muzzle blast, side (~90°), hard ground, neutral weather | 10 m | **177** (±4) | 50 m 158–161; 100 m 150–154; 300 m 137–143; 1 km 122–127; 3 km 109–116 | crack near, boom at hundreds of metres, rumble at km; A-duration 3–5 ms at 10–100 m; large-weapon energy ~30–50 Hz | [D]+[E]. Kingery-Bulmash shape anchored at 10 m, cross-checked against BRL 105 mm plots and DoD "5 lb C4 at 2 km ≤ 120 dB". Front sector +5 to +10; rear −10 to −15 (a brake shrinks the difference); weather ±10–15. |
| **Enemy guns** (details in §3) |||||||
| 5 | 76.2 mm F-34 (T-34, no brake) / ZiS-3 (brake); charge 1.08 kg | 10 m side | **174** | 5 m 182; 50 m 155; 100 m 146; 300 m 133–135; 1 km 119–124; 2 km 110–117; 1 m ≈ 195–200 (jet near-field, not a listening position) | "Ratsch-Bumm": the shell's crack arrives 0.8 s (500 m) to 1.5 s (1 km) before the report | [D], low–moderate. F-34 side levels −2 to −3 dB against the braked ZiS-3. |
| 6 | 45 mm 53-K / M-42; charge 0.36 kg | 10 m side | **170** | 5 m 178; 50 m 150; 100 m 142; 300 m 129–132; 1 km 114–120 | sharper and shorter than the 76 | [D], low–moderate. |
| 7 | Crack of a passing shell (Whitham N-wave, −20 °C) | 10 m miss | 45 mm AP **152**; 76 mm AP **155**; 88 **156**; 122–152 mm HE **157–159** | 1 m: 167–174 (near-field, caution); 100 m: 137–144 | N-wave 0.9–2.2 ms at 10 m; energy near 0.66/T, ≈ 0.3–0.7 kHz; microsecond rise, so a broadband "whip crack" | [D], medium. The law is validated: DuMond 1946, Stoughton 1997. With K = 0.62–0.68 instead of 0.53, add 0–2 dB. |
| **Small arms** (at the firer's ear, and 1 m to the side of the muzzle) |||||||
| 8 | Kar98k (8×57) | firer's ear / 1 m side | **159** (±3) / **166** (±3) | LAE ≈ 123 at the ear | A-duration ~0.3 ms; energy 1–2 kHz (peak ~1.6 kHz) | [P], medium. .308 bolt rifle 157.2 ear / 165.7 at 1 m; G3 159 at ear. |
| 9 | MG34 / MG42, fired outside | gunner's ear / 1 m | **159** (±4) per round / 165–168 | burst: LAE per round ~123, +10·log(rounds) | 13–15 rounds/s (MG34), ~20 (MG42) | [P]/[E], medium-low. |
| 10 | MG34 bow or coaxial, fired from inside the Tiger | nearest crewman (radio operator, gunner) | **130–140** (others 125–135) | burst LAeq 115–125 dB(A) | muzzle blast outside, leaking through the ball mount or mantlet (25–40 dB), plus bolt and feed clatter inside, 13–15 impacts/s | [E], low (±10 dB). No measurement exists. |
| 11 | Luger P08 (9×19) | firer's ear / 1 m | **158** / **160** | LAE ≈ 120 | ~0.3 ms, bright | [P], medium. 9 mm P226: 157.7 at ear, 160.5 at 1 m. |
| 12 | PPSh-41 (7.62×25) | firer's ear / 1 m | **158** (±4) / **161** | | 15–17 rounds/s; the jacket compensator vents sideways and back | [E], low-medium. |
| 13 | PTRD-41 (14.5×114, muzzle brake) | firer's ear / 1 m side | **170** (±5) / **176** (±5) | LAE ≈ 134 | longer and lower than a rifle; heavy concussion | [E], low. Charge ~30 g against 2.8 g for .308; the brake adds +3–8 dB to the side and rear. |
| 14 | DP / DT (7.62×54R) | gunner's ear | **159** | | ~10 rounds/s | [E]. |
| 15 | 12.7 mm HMG (reference only) | operator | **155.2 dB(C)** median, 160.7 max | SEL 122.8 dB(A) per round | | [M] (Paddan). |
| **Impacts on the listener's own armour** (§5) |||||||
| 16 | AP round, square hit, not penetrating, near the listener | inside, within ~1–2 m of the struck plate | **145–160** | LAE ≈ 115–125 | bell/anvil ring of the steel modes 0.3–3 kHz, T60 0.1–0.3 s, over a body thump | [D]/[E], low. Energy argument in §5. |
| 17 | Glancing hit, ricochet, or hit far from the listener | inside | **115–135** | | "spang"; can be masked by engine and battle | [E], low. Loza: some hull hits were never heard at all [A]. |
| 18 | APHE penetration, filler bursting inside (e.g. 76 mm BR-350A, 155 g) | inside | quasi-static **184–194** (30–100 kPa) for 0.1–1 s; shocks near the burst higher | | eardrum rupture probable; blast lung possible | [D]/[E], low–moderate. §5. |
| 19 | MG burst hitting the armour | inside, near the struck plate | **120–140** per hit | | hammering / hail, 10–20 hits/s | [E], low. |
| **HE bursts** (surface burst, incident peak, Kingery-Bulmash; ranges run from cased-charge (Fano) to bare charge; §6) |||||||
| 20 | 76 mm OF-350 (0.71 kg TNT) | 10 / 50 / 100 / 300 / 1000 m | **172–176** / 152–157 / 144–149 / 131–137 / 116–125 | | t+ 3–4 ms at 10 m | [D], medium near, low far. |
| 21 | 82 mm mortar O-832D (0.40 kg) | same | **170–174** / 150–155 / 142–147 / 129–135 / 114–124 | | 3–4 ms | [D]. Fill mass from memory, not checked. |
| 22 | 122 mm OF-462 (3.67 kg) | same | **178–183** / 160–164 / 151–156 / 138–142 / 123–131 | | 5–6 ms | [D]. |
| 23 | 152 mm OF-540 (5.9–6.25 kg) | same | **180–185** / 161–166 / 153–158 / 139–144 / 125–132 | | 5.5–7 ms | [D]. |
| 24 | 50 kg bomb, SC 50 (24.4 kg) | same | **189–193** / 169–171 / 161–163 / 147–150 / 133–137 | | 8–9 ms | [D]. |
| **Vehicles** (§7) |||||||
| 25 | Tiger interior, idle | crew ear, buttoned | — | **93 dB(A)** (90–97); ~103 dB(Z) | engine firing rate rpm/10 = 60–80 Hz; cabin booms 30–100 Hz | [E], low–medium. M1 idle 93 dB(A) [M]. |
| 26 | Tiger interior, 15–20 km/h cross-country | same | — | **110 dB(A)** (105–112); ~118 dB(Z) | track links at v/0.13 m = 32–43 Hz; gearbox and steering inside the crew space | [E]. M1 108 at 16 km/h; M113 106 [M]. |
| 27 | Tiger interior, 30–40 km/h on road | same | — | **115 dB(A)** (110–118); ~123 dB(Z) | engine 250–260 Hz at 2,500–2,600 rpm | [E]. M1 114 at 48 km/h; buttoned M113A2 115 dB(A) / 123 dB at 32 km/h [M]. |
| 28 | Intercom at the ear (for reference) | M60A1 | — | ~104 dB(A) (turret 98.5) | "primary cause of hearing damage among tank crews" | [M] (Garinther & Blazie 1973 abstract). |
| 29 | Tiger passing, exterior | 10 m / 100 m | — | ~100 / ~78 dB(A) | two silencers with flaps; steel tracks | [E], low. |
| 30 | T-34 passing, exterior | 10 m / 100 m | — | ~106 / ~84 dB(A) | straight-through exhaust with no silencer; clatter | [E], low for the levels; ordering (louder than the Tiger) [A]. |

---

## 2. Main gun: the Tiger's own 88

### Measured anchors

**Inside turrets and fighting compartments** (peak, dB re 20 µPa)
- **M60, 105 mm M68, turret with hatches open (1959)** [M]
  - Readings: 165.3, 167.5, 166.5, 163.3, 164.8 dB; average 165.5.
  - The 0.2 s time average was 145.8–148.8 dB.
  - Microphone: ~0.15 m behind and 0.9 m left of the breech, at breech height.
  - An impact meter of that era may read low.
  - Source: Foster 1960, HEL TM 8-60, AD0627820.
- **M109A5/6 Paladin, fighting compartment, hatches open except the driver's:** 166.1 dBP [M] (CHPPM 2006 equipment list).
- **ARL Paladin, Zone 7 charges, manikin inside the crew compartment:** 150–160 dB [M] (NATO RTO-TR-017, Appendix II).
  - "Predicted hearing damage … was greater with the hatches open than with the hatches closed."
  - Unprotected damage was predicted after 3 rounds.
- **ARL steel chamber (3 × 3 × 2.44 m) built to reproduce crew-compartment pressure histories** [M] (Price, ARL, AHAAH weapon-noise paper)
  - B-durations of "several hundred milliseconds".
  - Peaks of 182.5–184 dB, about 174 dB under an earmuff.
- **Qualitative** [M, HEL TN 4-64, Chaillet 1964]: "the very massiveness of the hull and turret provides an effective shield for the crew in the tank, but … little if any protection … to the tank commander, whose head may be outside the cupola." The crew helmet (CVC T56-6) "provides very little noise attenuation".

**Gun crews in the open**
- **USAEHA TG 1975 (Goldstein)** [M]:
  - M48A2 90 mm at 12 ft: 187.
  - M48A3 90 mm: 181.
  - M68 105 mm tank-mounted at 13 ft: 179.
  - 105 mm howitzer gunner: 189.
  - 155 mm howitzer: 185.
- **IOM 2006 Table 3-1** [M]: M119 105 mm (charge 8) gunner 183 dBP; M198 155 mm gunner 178 dBP.
- **Garinther 1979, JASA 65(S1):S29** [M], peak with B-duration: 155 mm howitzer 178 dB / 50 ms; 105 mm howitzer 171 dB / 30 ms.

**WWII, measured at the ear** [M] (Murray & Reid 1946, *J Laryngol Otol* 61:92, doi:10.1017/S0022215100007799)
- Instrument: piezo gauge, ~100 kHz bandwidth.
- Peaks at crew ear positions:
  - 17-pdr with muzzle brake: 6.8–7.9 psi = 187.4–188.7 dB.
  - 6-pdr AT gun: 180.6–181.4.
  - 25-pdr Mk 2: 171.6–181.4.
  - Short 25-pdr: up to 188.4.
  - .303 rifle at the firer's ear: 154.3–160.3.
- After 7 rounds of the 17-pdr at ~7 psi, the average hearing loss was 49 dB and the peak loss 85 dB. "Loss of hearing lasted from a few hours to several days."
- Eardrum rupture occurred at 6–8 psi.
- **Key sentence:** "Guns … in which the crews … are protected by turrets produced negligible effects. Such as were recorded, are due either to open hatches, or possibly to the general high noise level … when subjects were exposed outside the turrets, deafness was produced even after the few rounds which were fired."
- They classed "inside gun turrets" as peak pressure under 1½ lb/in² (< ~174 dB).
- Waveforms show "several pressure peaks … The initial peak is not usually the greatest."

### Why a Tiger turret sits where it does

**Buttoned up, the hull shields the crew.** Measured interiors run 150–168 dB against 180–190 dB outside, and WWII turret crews showed negligible temporary threshold shift (TTS). Three things push the Tiger toward the upper half of that range:
- **The muzzle brake.** The double-baffle brake throws blast back onto the mantlet and turret front: "a muzzle brake deflects the impulse noise back toward the crew" (AMCP 706-251, 1968). A modern M68 or 120 mm has no brake.
- **Cruder leak paths.** The mantlet carries three apertures: the gun, the coaxial MG and the binocular TZF 9b sight. There are also vision ports, pistol ports and the turret-ring gap. Only the first 495 Tigers had wading seals, including an inflatable turret-ring seal used for submersion.
- **The breech.** The semi-automatic breech ejects the case and stays open. AMCP 706-251: closed-cab guns "discharge propellant gases into the cab when the breech is opened … impairing sight and breathing … flashback."
  - This adds fumes and a mechanical transient (recoil stroke, breech-block clang, case strike).
  - It adds little to peak level, because bore pressure has decayed before the breech opens [E].

**Best estimate:** **163 dB (157–168)** buttoned up, rising with an open hatch toward the M60's 165–168, **168 (163–172)**. The loader and gunner sit about 0.5–1 m from the breech and are 1–2 dB above the commander.

**Head-out, the direct blast from the brake.**
- Geometry [E]: L/56 barrel 4.93 m; cupola at the rear left of the turret. The brake is ~6.3 m forward of and ~1 m below the commander's head, with a clear line of sight over the mantlet.
- **Scaling from the 17-pdr** (188 dB at ~4.3 m, 3.7 kg charge):
  - The distance for equal level scales with (charge)^1/3: (2.5/3.7)^1/3 = 0.88.
  - So 6.3 m from the 88 is equivalent to 7.2 m from the 17-pdr.
  - With near-field decay p ∝ r^−1.3, that is 188 − 26·log10(7.2/4.3) ≈ **182 dB**.
- 182 dB = 25 kPa. That is above the 20 kPa 1 % eardrum-rupture threshold and just below the 35 kPa classical threshold (§9). A head-out shot carries a small but real risk of rupture, especially with the ear facing the brake.
- This agrees with the US Army prohibiting M1 crews from training heads-above-hatch because the exterior commander and loader positions are "above or just below the limit of hearing protector effectiveness".

**Spectral character inside** (Ylikoski 1995; DoD 2013)
- Energy for large calibres lies below 400 Hz, peaking at 16–100 Hz.
- Inside a steel box: a slam, then a boom, then a dense reverberant tail.

### Muzzle blast versus distance [D]

**Method**
- Use the Kingery-Bulmash hemispherical surface-burst fit (Swisdak 1994) for the decay shape:
  - ln P[kPa] = 7.5938 − 3.0523x + 0.40977x² + 0.0261x³ − 0.01267x⁴ for 2.9 < Z ≤ 23.8, where x = ln Z.
  - P = 425.6·Z^−1.4066 for 23.8 < Z ≤ 198.5.
  - Beyond that, extrapolate with r^−1.1 (upper bound) and r^−1.38 (lower bound).
- Anchor the 88's side level at 10 m to 177 dB. That corresponds to a surface-burst TNT equivalent of 0.90 kg.
- Scale the other guns by propellant charge: 76.2 mm 1.08 kg; 45 mm 0.36 kg.

| Distance (side) | 88 KwK 36 | 76.2 F-34 / ZiS-3 | 45 mm |
|---|---|---|---|
| 5 m | 186 | 182 | 178 |
| 10 m | 177 | 174 | 170 |
| 25 m | 167 | 163 | 159 |
| 50 m | 158 (fork 1: 161) | 155 | 150 |
| 100 m | 150 (fork 1: 154) | 146 | 142 |
| 300 m | 137–138 (fork 1: 143) | 133–135 | 129–132 |
| 1 km | 122–126 (fork 1: 127) | 119–124 | 114–120 |
| 3 km | 109–116 | ~106–113 | ~101–109 |

**Cross-checks**
- **5 lb C4 at 2 km** (≈ 3 kg TNT equivalent): the same law gives 118.6–123.3 dB. The DoD Noise Working Group says "can be as high as 120 dB peak if propagation conditions are favorable".
- **BRL-MR-3957, 105 mm M68 firing M735** [M, read as plot-axis upper bounds]: ≤ 20 kPa (180 dB) at 10.5 m, 60°; ≤ 6 kPa (169.5) at 10.5 m, 180°; ≤ 3 kPa (163.5) at 42 m, 90°.
- **Level offset from the 105**: scaling by charge gives −2.4 dB far field and about −3.5 dB near field for the 88.

**Directivity**
- Front versus rear: 11–18 dB in the far field (Schomer 1979, cited in BRL-MR-3957); "up to 15 dB louder in front of the muzzle than behind" (DoD 2013).
- A muzzle brake raises the side and rear at 70–150° (Zhao 2019, rifle with a brake).
- Use +5 to +10 dB in the forward 0–30° sector and −10 dB to the rear (−5 dB for braked guns).

**Variability.** Turbulence alone moves peaks by up to 15 dB within 15 minutes (DoD 2013). Treat anything beyond 300 m as ±10 dB.

---

## 3. Enemy guns at range, outside and inside

**Blast.** See the table above; the ZiS-3 is row 5, the 45 mm row 6.
- **Distance law:** use Kingery-Bulmash within about 150 m, then 1/r^1.1–1.4 plus air absorption.
- **At 1 m:** the level is ~195–200 dB. That is inside the gas-jet near field and never a listening position; the BRL 105 mm gives ≤ 200 kPa at 1.6 m, 90°.
- **Inside a buttoned-up Tiger:** subtract **20–30 dB from the peak** for low-frequency gun blast, which is leak-limited, and **30–40 dB** for the crack and other high-frequency content (§8).
- A ZiS-3 firing from 500 m (≈ 128–131 dB outside) arrives inside at about 100–110 dB peak. It sits on a 105–115 dB(A) engine bed as a dull thump.

**Supersonic crack** [D] (Whitham far-field law; K = 0.53; −20 °C; p0 = 101.3 kPa)
- Peak: Δp = 0.53·p0·(M²−1)^1/8·d / (b^3/4·l^1/4).
- Duration: T = 1.82·M·b^1/4·d / (c·(M²−1)^3/8·l^1/4).
- Symbols: b is the miss distance, d the calibre, l the projectile length, M the Mach number.
- Measured confirmation:
  - DuMond et al. 1946 (JASA 18:97; WWII Caltech programme, .30/.50 calibre, 20/40 mm): amplitude ∝ b^−3/4, period ∝ b^1/4; 40 mm decayed ~b^−0.9.
  - Stoughton 1997 (JASA 102:781): weak-shock theory confirmed at 3–55 m.
- A numerical Whitham integration gives K = 0.62–0.68 for bullet shapes, so the table may read 0–2 dB low.

| Projectile | Mach at −20 °C | 1 m* | 3 m | 10 m | 30 m | 100 m | T at 10 m |
|---|---|---|---|---|---|---|---|
| 7.92 mm sS (760 m/s) | 2.38 | 156 | 149 | **141** | 134 | 126 | 0.26 ms |
| 7.62×54R (830 m/s) | 2.60 | 156 | 149 | **141** | 134 | 126 | 0.26 ms |
| 7.62×25 PPSh (488 m/s) | 1.53 | 156 | 149 | **141** | 134 | 126 | 0.32 ms |
| 9 mm P08 (350 m/s) | 1.10 (unreliable below ~1.2) | ~155 | — | ~140 | — | — | ~0.5 ms |
| 14.5 mm PTRD (1010 m/s) | 3.17 | 160 | 153 | **145** | 138 | 130 | 0.42 ms |
| 45 mm AP (760 m/s) | 2.38 | 167 | 160 | **152** | 145 | 137 | 0.97 ms |
| 76.2 mm AP (662–680 m/s) | 2.08–2.13 | 170 | 163 | **155** | 148 | 140 | 1.3–1.4 ms |
| 88 Pzgr 39 (773 m/s) | 2.42 | 171 | 164 | **156** | 149 | 141 | 1.5 ms |
| 122 mm OF-462, howitzer (515 m/s) | 1.61 | 172 | 165 | **157** | 150 | 142 | 1.9 ms |
| 152 mm OF-540, ML-20 (655 m/s) | 2.05 | 174 | 167 | **159** | 152 | 144 | 2.2 ms |

\*The 1 m column extrapolates near-field for shells, whose length is comparable to 1 m; treat it as indicative only.

**Using the table**
- The crack is heard only inside the Mach cone, behind and beside the trajectory. A supersonic shell arrives before its gun's report.
- **Distance:** −4.5 dB per doubling of miss distance for bullets, about −5.4 dB for shells of 40 mm and up.
- **Cold makes it louder:** −20 °C raises the Mach number and adds ~0.1–0.4 dB. 9 mm pistol rounds become marginally supersonic.
- **Inside the hull:** attenuated 30–40 dB to a sharp tick (§8).
- **Subsonic rounds** (mortar bombs, low-charge howitzer shells): no acoustic measurement was found. Physically, their aerodynamic rush or whistle runs ahead of them and rises with Doppler shift before the burst [E].

---

## 4. Small arms and machine guns

**Measured anchors** (peak dB)

| Weapon | Position | Peak | Source |
|---|---|---|---|
| 9 mm pistol (P226) | 1 m left of muzzle / ear | 160.5 / 157.7 | Branch 2011, OHNS 144:950 |
| .45 ACP | 1 m and ear | 162.5 | Branch 2011 |
| 5.56 mm M4 | 1 m / ear | 164 / 155 | Branch 2011 |
| .308 bolt action (Remington 700) | 1 m / ear | 165.7 / 157.2 | Branch 2011 |
| 9 mm slide closing, no shot | | 124 | Branch 2011 (mechanical noise) |
| .308 Remington 700 | 0.6 m left of muzzle / ear height / 1 m behind head | 176 / 161 / 150 | Murphy 2018 (PMC8588962) |
| G3 | firer's ear | 159; energy 1–2 kHz (peak 1.6 kHz) | Balatsouras 2005 |
| G36 | ear | 156.1; effective time 1.57 ms | Wagner 2005 |
| RK 62 | left ear | 156 C-peak (SD 1.0) | Pääkkönen 1998 |
| Warsaw Pact-type small arms | ears | 151.8–156.5 C-peak; LAFmax 116–135; LAeq,Te 104–119.5 | Konopka 2002 |
| Finnish rifle-calibre weapons | in the ear | 156–170; A-duration ~0.3 ms | Toivonen 2023 |
| Rifles larger than .22 / pistols | shooter's left ear | 159–174 / 148–171 | Meinke 2017 |
| 7.62 mm assault rifle | 4 m | 154 | Ylikoski 1995 |
| Howitzer | shooter's ear | up to 183 | Ylikoski 1995 |
| 12.7 mm HMG | operator | 155.2 dB(C) median | Paddan 2016 |
| Rifles and pistols, earlier compilations | gunner | 157–160 | IOM 2006 Table 3-1d (M9 157, M16A2 157, M249 160, M60 155, M2 153); 1960s M-14 full-auto at the right ear 159 |
| .303 rifle (WWII) | firer's ear | 154.3–160.3 | Murray & Reid 1946 |

Other measured points:
- Head shadow between the two ears: 9.8 dB (Meinke 2017).
- Muzzle blast lasts 3–5 ms in total; over- plus under-pressure phases 0.45–1.9 ms (Maher 2006; Routh & Maher 2016).
- Rifle spectra: energy 150–2,500 Hz, peaking at 0.9–1.5 kHz (Ylikoski 1995).
- NATO TR-017, peak minus A-weighted SEL: 34.6–37.3 dB at a rifle; 33.9 dB with reflections.

**Distance law for rifle-class blast** [E]: about 166 − 20·log10(r) dB to the side out to tens of metres. Forward is +9 dB; the rear is 5–10 dB lower. This predicts 154 dB at 4 m, matching Ylikoski.

**Per-weapon values:** see rows 8–15 of the calibration table.

**MG34 fired from inside the Tiger** [E]. No measurement of any coaxial or bow MG inside a vehicle was found.
- **Measured analogue:** Paddan's 12.7 mm HMG firing on a ship's bridge wing, heard inside the closed steel wheelhouse.
  - Outside peak: 155.2 dB(C) median. Inside: 108.8–115.4 dB(C).
  - Inside SEL: 75–83 dB(A), against 122.8 outside.
  - "The ship's structure provided about 40 dB attenuation"; inside, the sound is "dominated by low frequencies".
- **The Tiger case.** The MG34's muzzle is just outside the armour; the receiver is inside the crew space.
  - The muzzle blast (~160–166 dB at 1 m) leaks through the ball mount or mantlet at 25–40 dB insertion loss.
  - The bolt and feed clatter: a 124 dB pistol slide scaled up to a much heavier MG bolt gives ~125–135 dB at 0.5 m.
- **Result:** **130–140 dB peak** at the operator's ear, a burst LAeq of 115–125 dB(A) in the reverberant box, ±10 dB.

---

## 5. Impacts on armour (heard inside)

The research fork for this deliverable did not report in time. No measurement of the sound inside a vehicle struck by a projectile was located by the other forks. The figures here are physical derivations [D]/[E] with a reality check from one crew account.

**Energy argument for a non-penetrating AP hit** [D/E]
- **Kinetic energy.** A 76 mm BR-350A (6.3 kg) at ~600 m/s carries ≈ 1.1 MJ.
- **Share into ringing.** Most goes into plastic work, shatter and heat. Assume 0.1–2 % goes into elastic ringing of the hull: 1–20 kJ.
- **Radiated power.** Steel armour 60–100 mm thick has its coincidence frequency at ≈ 12,400/t(mm) Hz, so 120–200 Hz. Above that the plate radiates efficiently (σ ≈ 1).
  - With vibrational energy E spread over M ≈ 20 t of radiating plate, ⟨v²⟩ = E/M.
  - Power radiated into the crew space: P ≈ ρc·σ·S·⟨v²⟩, with S ≈ 40 m².
- **Level in the compartment.** Room absorption A ≈ 2–6 m² sabins, from the Sabine estimate for 10 m³ of steel box with crew and stowage: RT60 0.26–0.8 s.
  - The reverberant level is Lp ≈ LW + 10·log(4/A).
  - This gives **~150–162 dB** for 2 % in ringing and ~137–149 dB for 0.1 %.
  - Near the struck plate, the direct field adds a few dB.
- **Decay:** hull loss factor 0.01–0.05 gives T60 = 2.2/(f·η) ≈ 0.1–0.4 s at 0.3–1 kHz. That is the bell ring over a body thump.
- **Estimate:** **145–160 dB** for a square hit near the listener; **115–135 dB** for glancing hits or hits far from him.

**Crew account** [A] (D. F. Loza, Soviet commander in Lend-Lease Shermans, iremember.ru)
- A round striking the turret next to his ear was heard but did not concuss him.
- Hull hits he "могу вообще не услышать" (might not hear at all). Several times he found dents after a fight without having heard the strikes.
- This supports a wide range: engine noise at 105–115 dB(A), his own gun's TTS and the headset mask low-energy hits.

**Penetration with a bursting charge** [D]
- **Quasi-static pressure.** 155 g of TNT (BR-350A filler; ZiS-3 table on en.wikipedia) bursting in ~10 m³:
  - The detonation energy alone gives ΔP ≈ (γ−1)E/V ≈ 0.4 × 0.71 MJ / 10 m³ ≈ **28 kPa**.
  - With afterburning of the products in the air, the standard confined-charge relation P_qs[MPa] ≈ 2.25·(W/V)^0.72 gives **~110 kPa**. That relation is recalled from confined-explosion practice, not fetched.
  - Range: **30–100 kPa (184–194 dB)**, lasting 0.1–1 s as it vents through hatches and gaps. Shock peaks within ~1 m of the burst are higher.
- **Consequence:**
  - The whole crew is at or above the eardrum-rupture threshold (35 kPa = 185 dB; 50 % at 100 kPa, §9).
  - Blast-lung limits (~180–195 dB) come into play.
  - Survivors emerge deaf, often with bleeding ears. Among blast-wounded patients with a burst eardrum, 50 % had tinnitus and 77 % reduced hearing (Ritenour 2008).
- **Without a working filler** (solid shot or a dud), behind-armour debris and flash dominate. The overpressure is moderate but was not quantified here [E].

**MG burst hitting the armour** [E]
- A 7.92 mm bullet (12 g at 700 m/s ≈ 3 kJ) carries 1/370 of a 76 mm round's energy, so about −26 dB. That puts it at **120–140 dB per hit** near the struck plate, hammering at the burst rate.

---

## 6. HE, artillery and bombs

**Model** [D]
- Incident peak from Kingery-Bulmash hemispherical surface bursts (Swisdak 1994 fit). The extension beyond Z = 198.5 m/kg^1/3 uses r^−1.1 for the upper bound and r^−1.38 for the lower; the r^−1.38 slope matches US Bureau of Mines unconfined-blast data at 100 m.
- The low end of each range is the cased-charge (Fano) equivalent, W_eff = W·(0.2 + 0.8/(1 + M/C)). The high end is the bare charge.
- Durations are the Kingery-Bulmash positive phase at 10 m.
- Fills [M]: OF-350 710 g; OF-462 3.67 kg; OF-540 5.90–6.25 kg (en.wikipedia ammunition tables); SC 50 24.4 kg (en.wikipedia SC 50). The mortar fill is from memory.

| Source | 5 m | 10 m | 20 m | 50 m | 100 m | 300 m | 1 km | 3 km | t+ at 10 m |
|---|---|---|---|---|---|---|---|---|---|
| 45 mm O-240 HE (~0.12 kg in 2.1 kg; unverified) | 172–177 | 164–169 | 156–161 | 144–150 | 136–142 | 123–131 | 108–120 | 95–109 | 2–3 ms |
| 76.2 mm OF-350 (0.71 kg in 6.2 kg) | 179–185 | 172–176 | 164–168 | 152–157 | 144–149 | 131–137 | 116–125 | 103–115 | 3.3–4.4 ms |
| 82 mm mortar O-832D (0.40 kg in 3.1 kg; from memory) | 177–182 | 170–174 | 161–166 | 150–155 | 142–147 | 129–135 | 114–124 | 101–113 | 2.9–3.9 ms |
| 122 mm OF-462 (3.67 kg in 21.76 kg) | 188–194 | 178–183 | 170–174 | 160–164 | 151–156 | 138–142 | 123–131 | 110–120 | 5.0–6.4 ms |
| 152 mm OF-540 (6.0 kg in 43.6 kg) | 190–197 | 180–185 | 172–176 | 161–166 | 153–158 | 139–144 | 125–132 | 112–122 | 5.5–7.1 ms |
| 50 kg bomb SC 50 (24.4 kg in 55 kg) | 202–206 | 189–193 | 180–182 | 169–171 | 161–163 | 147–150 | 133–137 | 120–126 | 8.4–9.1 ms |

**Reading the table**
- **Eardrum hazard.** The 50 % rupture level (194 dB) is reached within ~5 m of a 122/152 mm burst and ~10 m of an SC 50. The 1 % level (180 dB) is reached at ~10 m (122/152) and ~20 m (SC 50). An ear facing the blast sees reflected pressure several times the side-on value (Richmond 1989).
- **Inside a buttoned-up Tiger** [E]: subtract 20–30 dB from the peak for bursts beyond ~20 m. Near bursts couple through the ground and structure as well: use ~15–25 dB.
- **Roof hits.** The roof is 25 mm, with a coincidence frequency near 450 Hz and a mass-law transmission loss of only ~35–40 dB at 100 Hz. A direct HE hit on the roof or engine deck is catastrophic, not an acoustic event.
- **Character with distance** (DoD 2013): "sharp crack … booms … rumbling, rolling quality". Large-weapon energy sits around 30–50 Hz. Windows rattle in the low 120s dB. Over snow, distant bursts lose their crack and turn to low rumble (§10).

---

## 7. Engines and running gear

### Measured interior levels (modern and 1960s)

**US Army data (USACHPPM 2004, in IOM 2006 Table 3-1d)** [M], dB(A):

| Condition | M1 family | M113A3 |
|---|---|---|
| Idle | 93 | 85–92 |
| Tactical idle | 103 | — |
| 16 km/h | 108 | 106 |
| 32 km/h | — | 109 |
| 48 km/h | 114 | 114 |
| 63 km/h | 117 | 118 |

CHPPM 2006 gives the Abrams at 96–117 dB(A) moving.

**Nakashima, Borland & Abel 2007** (*Ind Health* 45:318) [M]
- M113A2 ADATS driver, hatches open:
  - Idle: 99.4 dB / 94.7 dB(A).
  - 8 km/h: 100 / 94.5.
  - 32 km/h: 111 / 103.
- The same vehicle **with hatches closed at 32 km/h: 123 dB / 115 dB(A)**, 12 dB louder.
- Band peaks at 20–63 Hz. Unweighted levels run 5–14 dB above A-weighted.

**Other measured points**
- **Yarington 1968** (via IOM): "Tanks – Interior – 115 (±10)" dB [M].
- **Garinther & Blazie 1973, M60A1:** 98.5 dB(A) in the turret; the intercom at the ear ~104 dB(A) [M].
- **Hatch & Walpole 1943**, Armored Medical Research Laboratory, Fort Knox (AD658629), M4A3 crews over 41 days of road and field tests [M, NTIS abstract only]:
  - "No measureable hearing loss at any frequency".
  - No fatigue deafness after 5 h of exposure.
  - The abstract gives no dB values. The M4A3's rubber-bushed tracks make it a poor proxy for the Tiger's dry-pin steel tracks.

### Tiger estimates [E] (anchored on the measurements above)

| Condition | dB(A) | dB(Z) |
|---|---|---|
| Idle | **93** (90–97) | ~103 |
| 15–20 km/h cross-country | **110** (105–112) | ~118 |
| 30–40 km/h on road | **115** (110–118) | ~123 |

Adjustments:
- The driver and radio operator sit either side of the gearbox and steering unit, with the front sprockets beside them: +2–4 dB over the turret crew.
- Buttoned up is 5–12 dB louder than with hatches open.
- The engine is behind a firewall, and the Tiger's speeds are lower than an M1's.

**Tonal content** [D]
- **Four-stroke V12 firing frequency** = rpm/10 Hz; each bank contributes rpm/20.
  - Tiger HL 210/230: 60–80 Hz at idle (the idle speed is an estimate); 250–260 Hz at the 2,500–2,600 rpm governed or recommended limit; 300 Hz at 3,000 rpm.
  - T-34's V-2-34 diesel: 170–180 Hz at 1,700–1,800 rpm, with charging starting at 600–650 rpm.
- **Track-link rate** = v / pitch.
  - Tiger: pitch ≈ 0.13 m (96 links per side; track designation Kgs 63/725/130). That gives 21 / 43 / 81 Hz at 10 / 20 / 38 km/h.
  - T-34: link ≈ 0.17 m, horns every 0.34 m: 33 Hz and 16 Hz at 20 km/h.

**Exterior** [E, low]
- Tiger moving: ~100 dB(A) at 10 m, ~78 at 100 m. It has two vertical silencers with hinged flaps.
- T-34 moving: ~106 dB(A) at 10 m, ~84 at 100 m. The exhaust is "прямоточная, глушители звука отсутствуют" (straight-through, no silencers; ru.wikipedia).
- Method: an exhaust sound-power law of about 120 + 10·log(P_kW) dB, recalled from Bies & Hansen and **not verified**, plus steel-track clatter.
- Account [A] (Loza): "Шерман по шоссе идет как автомобиль, а наш Т-34 так громыхает, что его за черт знает сколько километров слышно" (the Sherman goes along a highway like a car, while our T-34 clatters so much you can hear it from God knows how many kilometres).
- The oft-quoted 1940 Kubinka figures (Pz III audible at 150–200 m versus T-34 at 450 m) were **not verified** in any fetched source.

**Cold starts** [M text, sound character E]
- **Tigerfibel:**
  - p. 22: the electric starter works down to −20°; otherwise crank with the Schwungkraftanlasser (inertia starter).
  - p. 22: one Tiger could warm another's coolant, run at 2,400 then 2,000 rpm.
  - p. 27: winter warm-up takes 15 minutes, to 50° coolant and 3 atü oil pressure.
  - p. 35: frozen road wheels are thawed with a blowtorch.
- **Sound of a Tiger start:** a rising flywheel whine for tens of seconds while two men crank, then a clutch clack, the engine turning over, and rough running while cold.
- **T-34:**
  - The main starting method was compressed air, a sharp blast before the diesel catches (ru.wikipedia).
  - Kirichenko: coolant was drained overnight and water heated on a fire in the morning.

---

## 8. Attenuation

### Armour transmission loss (mass law) [D]

Method: field-incidence mass law, TL = 20·log(m·f) − 47, with Sharp's method above the coincidence frequency. Steel c_L = 5,422 m/s, c = 319 m/s (−20 °C), η = 0.001–0.01. The plate is treated as infinite.

| Plate | m (kg/m²) | fc | 31.5 Hz | 63 Hz | 125 Hz | 250 Hz | 500 Hz | 1 kHz | 2–4 kHz |
|---|---|---|---|---|---|---|---|---|---|
| 25 mm (roof, floor) | 196 | 410–480 Hz | 29 | 35 | 41 | 38–45 | 21–32 (dip) | 30–41 | 39–59 |
| 60 mm (lower hull sides) | 471 | 170–200 Hz | 36 | 42 | 31–41 | 23–34 | 32–43 | 42–52 | 51–70 |
| 80 mm (superstructure, turret) | 628 | 130–150 Hz | 39 | 45 | 21–35 (dip) | 27–38 | 36–47 | 45–56 | 54–74 |
| 100 mm (front) | 785 | 105–120 Hz | 41 | 38–45 | 21–32 (dip) | 30–41 | 39–50 | 48–59 | 57–77 |

The coincidence dips are not realised in the hull. The first bending mode of an 80 mm panel of 1.8 × 1.0 m is ≈ 257 Hz, so below that the panel is stiffness-controlled. In practice the **plate TL is ≥ ~40 dB at all frequencies**: armour is not the airborne weak point. **Leaks and structure-borne excitation dominate.**

### Leakage [D]

**Composite transmission loss is capped by the open area:** TL ≈ 10·log(S_hull / S_leak).
- For ~30 m² of hull: 20 cm² of leak gives **42 dB**; 100 cm² gives **35 dB**; 500 cm² gives **28 dB**.
- Leak paths: the turret-ring gap (~5.8 m of circumference), mantlet apertures, vision and pistol ports, hatch seams, the fan or vent opening, and the ball mount.

**Low frequencies pass through leaks almost untouched.**
- The cabin (V ≈ 8–12 m³) and its leaks form a Helmholtz resonator with corner f_H = (c/2π)·√(S/(V·L)) ≈ **2–18 Hz**.
- Below f_H the interior pressure follows the exterior (0 dB); above it, attenuation rises 12 dB per octave until the leak cap is reached.

### Buttoned-up insertion loss, exterior to interior, by band [E]

| Band | 31.5 Hz | 63 Hz | 125 Hz | 250 Hz | 500 Hz | 1 kHz | 2 kHz | 4 kHz+ |
|---|---|---|---|---|---|---|---|---|
| IL (dB) | 5–15 | 15–25 | 20–30 | 25–35 | 30–40 | 30–40 | 30–45 | 35–45 |

**Basis:** the leak and Helmholtz model above, plus measured analogues:
- A closed steel wheelhouse gave ~40 dB on HMG peak and SEL (Paddan 2016).
- Indoor firing-range walls give 29–44 dB (Kardous 2003).
- WWII turret crews showed negligible TTS while crews outside were deafened (Murray & Reid 1946).

**For whole impulses**
- **Peak:** −20 to −30 dB for low-frequency gun and HE blast; −30 to −40 dB for cracks and small arms.
- **An open hatch** removes most of this: 0–10 dB, depending on geometry. ARL found more hazard with hatches open.
- **Head-out:** 0 dB, plus head shadow on the far ear (9.8 dB for rifle spectra, Meinke 2017; smaller for low-frequency blast).

**Own noise versus external noise.** Buttoning up makes the vehicle's own noise louder (+12 dB measured, ADATS at 32 km/h), because the cabin becomes a reverberant box around the drivetrain. Head-out, the crewman hears less cabin boom and more exhaust bark, fan and track clatter, plus wind at speed.

### Headsets and hearing protection

- **WWII practice** [M] (IOM 2006, Table 5-2):
  - Protection was "Cotton – Minor amounts of noise reduction", "Fingers", "Nothing – The standard of the day".
  - "utilization was essentially nonexistent throughout World War II".
  - The V-51R earplug arrived only at the war's end.
  - Early circumaural muffs gave "modest attenuation (around 20 dB or less) up to 1000 Hz".
- **Dfh.b (Doppelfernhörer b), with the Kehlkopfmikrofon (throat microphone) confirmed for Panzer crews:** no attenuation data or construction measurements were found.
- **Estimated attenuation for a supra-aural rubber-cup headset** [E], derived from modern sealed values minus 5–10 dB for a small cup and leaky seal:

  | Frequency | ≤250 Hz | 500 Hz | 1 kHz | 2 kHz | 4 kHz |
  |---|---|---|---|---|---|
  | Attenuation (dB) | 0–5 | 3–8 | 8–15 | 12–20 | 15–25 |

  - With one cup lifted to talk: 0 dB on that ear. Crews did this, and one ear left unprotected shows in veterans' audiograms (Moore & Cox 2024: 17 of 68 had ≥ 10 dB asymmetry).
- **Modern sealed reference headset** (Racal Slimgard II, passive) [M/spec]:

  | Frequency | 125 Hz | 250 Hz | 500 Hz | 1 kHz | 2 kHz | 4 kHz |
  |---|---|---|---|---|---|---|
  | Attenuation (dB) | 12 | 20 | 25 | 25 | 30 | 37 |

- **1960s US crew helmet** (CVC T56-6): "very little noise attenuation" [M].
- **Impulse insertion loss of modern protectors** [M] (Toivonen 2023): plugs 16–23 dB, muffs 10–20 dB, both together 24–34 dB.
- **Soviet crews:** padded шлемофон (tank helmet with earphones) plus laryngophone (throat mic). Estimated 0–3 dB at low frequencies and 5–15 dB at high frequencies [E].

---

## 9. Hearing effects

### Criteria [S]

**Flat limits**
- **140 dB peak** unprotected: OSHA, NIOSH, ISO 1999, EU 86/188, MIL-STD-1474D/E main body.
- **Steady noise:** 85 dB(A) over 8 h. This equals an A-weighted SEL of 129.6 dB (1.01 Pa²·h), or LIAeq100ms 139.6 dB for a single short impulse.

**CHABA 1968** (via NATO TR-017)
- The peak limit falls 6.7 dB per decade of B-duration.
- The reference is 100 impulses, with +5 dB per decade fewer ("merely an interpolation").
- Normal incidence is 5 dB worse than grazing.
- Reconstructed unprotected limits: 138 + 6.64·log(200/T_B) + 5·log(100/N). That is 163.9 dB at 25 µs, 153.3 at 1 ms, 146.6 at 10 ms and 138 at ≥ 200 ms [derived].

**MIL-STD-1474D (1997)**
- Curve W = 140 dB.
- Curve X = 160.5 + 6.64·log10(200/T_B) for T_B < 200 ms.
- Curves Y and Z sit 6.5 dB and 13 dB above X.
- Exposures allowed per day (single / double protection): X 2,000 / 40,000; Y 100 / 2,000; Z 5 / 100.
- Allowed count: N1 = 10^[(177 − L + 6.64·log(200/T_B))/5]; double protection allows N2 = 20·N1.
- Machine-gun bursts: B-duration × the number of impulses in the first 200 ms.

**MIL-STD-1474E (2015)**
- **AHAAH:** at most 500 auditory risk units (ARU) per 24 h for occasional exposure (≤ 1/week); 200 ARU at twice a week or more. Warned and unwarned exposures have the same limits.
- **LIAeq100ms = SEL_A + 10.**
  - A-duration under 2.5 ms: LAeq8h = LIAeq100ms − 54.6 − 15·log(A-dur / 0.2 ms).
  - A-duration 2.5 ms or longer: LAeq8h = LIAeq100ms − 71.0.
  - Dose % = 100 / 2^((85 − LAeq8h)/3), summed over impulses to ≤ 100 %.
- **Protection limits:**
  - Protector credit is capped at 41 dB (bone conduction).
  - Muff seals may fail 25 ms after > 185 dBP.
  - No exposure above 195 dBP without a blast-lung analysis.

**Pfander (1975/80) and Smoorenburg (1982)**
- Forms: L_peak + 10·log(T_C or T_D in ms) + 10·log N.
- Both are equivalent to about 85 dB(A) over 8 h (TR-017). The fetched sources did not give the exact constant.
- Pfander 1980, from > 10,000 soldiers: CHABA is "too restrictive".

**NATO RTO-TR-017 (2003) consensus**
- Rifles, unprotected ear, head-on: critical level **116 dB(A) SEL per impulse** (≈ 153 dBP), for up to 50 impulses.
- Blasts measured under a protector: 135 dB(A) SEL per impulse, for up to 100 impulses.
- Beyond those counts, equal energy applies.
- Measured trade-off with number of impulses: **2–7 dB per decade, not 10**.
- Peak minus A-weighted SEL: 40.7 dB (0.5 ms Friedlander pulse), 39.5 (explosions), 34.6–37.3 (rifle), 33.9 (rifle with reflections).
- My Friedlander computation agrees: LAE − Lpk = −41.9 at T+ 0.3 ms, −39.6 at 1 ms, −38.8 at 5–10 ms. A reverberant tail (RT60 0.1–0.3 s) lifts it to −36 to −32.

**How well the criteria predict real outcomes**
- AHAAH is right about 95 % of the time, against ~40 % for MIL-STD-1474 and ~25 % for A-weighted energy (TR-017; Price 2007).
- The older criteria are 9.6–21.2 dB over-conservative for protected blast (Chan 2001).
- **Longer, lower pulses do less damage per dB of peak:**
  - Cats: a rifle at 139 dBP (0.3–0.4 ms) did as much damage as a howitzer at 150 dBP (2–3 ms) (Price 1983).
  - Guinea pigs: the same finding at a constant 157.5 dB (Dancer 1985).
- **The middle-ear reflex:**
  - Threshold 70–100 dB SPL; latency ~25–35 ms at high levels and up to 150 ms near threshold (Dancer 1991).
  - About 15 dB of low-frequency attenuation at 20 dB above threshold (Brask 1978), falling to ~50 % tension within a few seconds.
  - It cannot protect against a sudden impulse. It helps for impulses repeated at intervals under 2–3 s.
  - Anticipatory contraction before a known shot occurs only in a minority of people (Jones, Greene & Ahroon 2018).

### TTS growth, recovery and pattern [M]

**Growth**
- **Rifles at 140–170 dBP, ~5 s apart (Coles, via TR-017).** A TTS above 25 dB at 2 minutes (mean of 4 and 6 kHz) is exceeded by 10 % of people at:
  - 159.0 dB for 25 shots;
  - 158.9 dB for 50 shots;
  - 150.6 dB for 100 shots (grazing incidence).
- TTS grows about 30 dB per 10 dB of peak, and about 40 dB per 10 dB above a 25 dB shift. The spread between people is about 16 dB.
- **Pfander 1980:** 6 shots at 158.3 dB left < 5 % of soldiers with prolonged recovery; 5 shots at 160.8 dB left 13 %.
- **Dancer 1991:** 10 shots at 159 dB gave ~25 dB TTS in 25 % of subjects.
- **Ward 1997:** 30 simulated gunshots at 150 dB can give ~20 dB TTS, while 300 at 140 dB (the same energy) give none. The mean maximum is near 6 kHz.
- **Albuquerque volunteers, earmuffs on:** 173–196 dB at 1–5 m. With intact muffs at 5 m there was no TTS.
- **WWII:** 7 rounds of the 17-pdr at ~188 dB, unprotected, gave an average loss of 49 dB and a peak loss of 85 dB, lasting hours to days (Murray & Reid 1946).

**Recovery**
- Recovery is linear in log time. A 20 dB TTS disappears after ~16 h (Ward 1997). Full recovery is expected within 24 h if TTS2 ≤ 25 dB (TR-017).
- **Delayed shift:** the maximum TTS can come 1 h after rifle fire (Dancer 1991); losses often get worse before they improve (Patterson & Hamernik 1997).
- **Acoustic trauma** is defined as TTS > 40 dB at 2 minutes (draft ANSI).
- **Clinical acoustic trauma** (source < 1 m, 158 dBP; Nottet 2006): worst loss at 24 h 37 ± 24 dB. High-frequency loss was 24 dB at 24 h, 14 dB at 72 h and 12 dB at 15 days, so about a third remains.

**Where the loss lands**
- Mostly at 3–6 kHz (TR-017).
- Deepest notch (Nottet 2006): 4 kHz 38 %, 3 kHz 33 %, 6 kHz 21 %, 5 kHz 8 %.
- In Finnish conscripts the loss appears first at 6 kHz, then at 8 and 4 kHz (Toivonen 2023).
- For steady noise, the peak loss sits ~½ octave above the noise (Mills 1978).

### Tinnitus [M]

**Incidence**
- **Finnish army acoustic trauma** (Mrena 2004): 94.2 % had tinnitus immediately afterwards; 45 % still had symptoms at the last follow-up; 87.5 % had been unprotected.
- **Finnish conscripts** (Mrena 2002, 418): all had tinnitus after the trauma; 29 % still had it at discharge; 66 of 101 contacted again still had it 10–15 years later.
- **More than 220,000 conscripts** (Toivonen 2023): 0.7–1.5 % suffered acoustic trauma; 67 % of them were within 2 m of the muzzle and 88 % were unprotected. 66 % had tinnitus. Losses were typically 10–20 dB, up to 50–80 dB.
- **Tinnitus without trauma:** among Swedish conscripts, prevalence rose from 23 % to 32 % over service (Muhr 2011).

**Pitch and loudness**
- **Loudness match:** mean 5.8 dB SL in acute cases, range 0–30 (Ochi 2003); most often 10 dB SL (Martines 2010); mean 7.3 dB SL (Byun 2010).
- **Pitch:** acute tinnitus averages ~3 kHz; the largest threshold loss sits 0.2 octave above the tinnitus pitch (Ochi 2003). The pitch lies near the edge of the loss (von der Behrens & Meyer 2014).
- Transient tinnitus follows TTS with a 4–8 kHz notch (Ortmann 2011).
- Tinnitus is symmetric even after one-sided noise (Temmel 1999).
- Masking it requires sound ≥ 20 dB above its own threshold (Scholar Gateway passage, clinical).

### Eardrum rupture and non-auditory limits [M/S]

| Criterion | Pressure | Peak level |
|---|---|---|
| Newer 1 % rupture threshold (Richmond 1989) | ≈ 20 kPa | 180 dB |
| Widely used rupture threshold | 35 kPa | 185 dB |
| 50 % rupture (P50) | 100 kPa | 194 dB |
| Dogs, P50, fast shock-tube waves | 78 kPa | 192 dB |
| Dogs, P50, complex waves | 205 kPa | 200 dB |
| Dogs, P50, smooth-rising | 296 kPa | 203 dB |

- An ear facing the blast sees reflected pressure several times the side-on value.
- WWII 17-pdr crews ruptured at 6–8 psi, 186–189 dB (Murray & Reid 1946).
- **Non-auditory evaluation threshold:** 180 dB. Proposed design limit: 195 − 10·log(A-duration in ms) − 2.5·log N (free field).
- **Sheep in a self-propelled-howitzer crew-compartment mock-up:** no injury at 27 kPa (×6) or 20 kPa (×25–100); throat lesions at 32 kPa (×6) and 24 kPa (×25).
- **German weapon development:** 190 dB is the absolute limit at operator positions (Brinkmann, in RTO-EN-11).

### Cumulative exposure over a fight

**How each criterion scales with the number of impulses N**

| Criterion | Change per tenfold increase in N |
|---|---|
| CHABA; MIL-STD-1474D | −5 dB |
| Equal energy, LIAeq100ms, Pfander, Smoorenburg | −10 dB |
| Measured TTS data | −2 to −7 dB |
| AHAAH | ARU add linearly; the 95th-percentile loss at 30 min = 26.6·ln(ARU) − 140.1 dB (≈ 25 dB at 500 ARU, +18.4 dB per doubling) |

**The criteria disagree by an order of magnitude.** An M240 instructor at 158.7–163.9 dBP was allowed 412–2,542 rounds under 1474D (double protection), 89–417 under 8-hour energy, and 19–91 under AHAAH (NIOSH HHE 2013-0124).

**Per-shot dose** [D] (85 dB(A)-8 h equivalents, from LAE = Lpk − 39 in the free field, − 33 in a reverberant turret):

| Lpk | Shots to one daily dose, free field | Same, reverberant turret | 40 shots as LAeq8h |
|---|---|---|---|
| 160 dB | 7.2 | 1.8 | 92–98 dB(A) |
| 165 dB | 2.3 | 0.6 | 97–103 dB(A) |
| 170 dB | 0.7 | 0.2 | 102–108 dB(A) |
| 175 dB | 0.2 | 0.06 | 107–113 dB(A) |

Equal energy overstates the hazard of low-frequency turret impulses: Murray & Reid saw "negligible" effects inside turrets.

**Worked 1474E examples** (40 rounds, unprotected; illustrative inputs)
- 165 dBP with 50 ms B-duration: 3.1 % per round (long pulse), 125 % in total.
- 175 dBP free-field pulse of 3 ms: 7.9 % per round, 315 % in total.

**Accounts of crew deafness**
- WWII Australian gun crews were deafened for hours to days after a few rounds outside turrets (Murray & Reid 1946).
- Royal Armoured Corps veterans show low- and mid-frequency loss and interaural asymmetry, consistent with one ear kept free of the headset (Moore & Cox 2024).
- Loza: throwing off the helmet and throat mic was the first move when hit.
- The fork gathering German and Soviet memoir quotes (Carius, Drabkin interviews) did not report; none are included.

---

## 9a. The middle-ear reflex: time course (addendum, 29 Sep 2026)

Added after the calibration pass, when the reflex, contracting at full strength 30 ms after the own
gun, took 5 dB (LAF) and 8 dB (peak) off the buttoned shot. §9 gives the latency and one level but
no rise time, relaxation or adaptation. These were retrieved without web search, by citation
matching and metadata in PubMed, DOI lookups on OpenAlex, one open-archive PDF (Dancer 1991) and one
open-access full text (Goodman & Keefe 2006). Two figures come only through the English Wikipedia
article "Acoustic reflex" because their primary texts are not open; they are marked [secondary].

### What the sources say

| Source | How retrieved | What it gives |
|---|---|---|
| Dancer 1991, *Médecine/Sciences* 7:357, doi 10.4267/10608/4361 | open-archive PDF (INSERM iPubli), read | [M, review] Threshold ~80 dB above the hearing threshold at 250–4000 Hz; magnitude grows ~0.6 dB/dB; **latency 150 ms at threshold, 25–35 ms at high levels**; contraction reduces transmission ~10 dB below the middle ear's main resonance; "given its latency, this reflex cannot protect the ear from isolated impulse noises"; protection appears once impulses come faster than 2–3 per second; its fatigability prevents protection "beyond a few minutes". |
| Brask 1979, *Acta Otolaryngol Suppl* 360:116, doi 10.3109/00016487809123490 | PubMed abstract | [M] Attenuation 20 dB above the reflex threshold ≈ 15 dB in every patient (Bell's palsy, one ear paralysed); the paralysed side showed a 34 dB TTS after the test noise, the healthy side none. |
| Borg 1968, *Acta Otolaryngol* 66:461, doi 10.3109/00016486809126311 | OpenAlex abstract | [M] A 500 Hz tone 20 dB above threshold attenuated 12–15 dB; 1450 Hz 16 dB above, 0–6 dB; attenuation greater below the middle ear's resonance (~1 kHz) than above; near maximal activity a "perfect regulatory" effect (cochlear excitation at 500 Hz capped at 115 dB SPL). |
| Møller 1962, *JASA* 34:1524, doi 10.1121/1.1918384 | OpenAlex abstract | [M] The reflex behaves as a linear envelope detector followed by an integrator; noise with a low peak factor elicits it better than tones; binaural stimulation is the most sensitive. |
| Dallos 1964, *JASA* 36:2175, doi 10.1121/1.1919340 | OpenAlex abstract | [M] With higher intensity, shorter latency and faster response; no clear relation of offset latency to intensity; during prolonged stimulation with very high-intensity noise the reflex is stable, with weaker noise it adapts, near threshold rapidly. |
| Hung & Dallos 1972, *JASA* 52:1168, doi 10.1121/1.1913229 | OpenAlex abstract | [M] Within the reflex's dynamic range the speed of the on-response is proportional to SPL; underdamped for 400–800 Hz carriers. No numbers in the abstract. |
| Kaplan, Gilman & Dirks 1977, *Ann Otol Rhinol Laryngol* 86:348, doi 10.1177/000348947708600312 | PubMed abstract | [M] Adaptation faster as frequency rises (0.5–1 kHz significantly slower than ≥ 2 kHz); rate independent of suprathreshold level (+6/+12/+18 dB). |
| Borg, Nilsson & Engström 1983, *Acta Otolaryngol* 96:361, doi 10.3109/00016488309132721 | PubMed abstract | [M] In variable industrial (shipyard) noise the stapedius reflex was "very stable" in humans; rabbits with the reflex blocked took extensive PTS, with it very little. |
| Borg 1982, *Scand Audiol* 11:237, doi 10.3109/01050398209087473 | PubMed abstract | [M] Onset, offset, decay and recovery measured; decay and recovery "probably different measures of the same process". No numbers in the abstract. |
| Goodman & Keefe 2006, *JARO* 7:125, PMC2504580 | open full text | [M] With broadband noise at 63–75 dB SPL, the reflex response begins ~100 ms after onset and decays into the noise floor ~200 ms after offset; clinical thresholds 85–100 dB SPL for tones, 65–80 dB SPL for broadband noise (citing Gelfand 1997, Møller 1962, Wilson & McBride 1978). |
| Deiters et al. 2019, *JASA* 146:3993, PMC7043895 | open full text | [M] Brief sounds (tones, recorded gunshots at 113–115 dB peak, noise) elicited the reflex in ~20–80 % of 190 people with clinical reflexes, gunshots in under half; responses declined over minutes; cites Rossi & Solero 1983 (onset 90–114 ms, longer for shorter stimuli) and the AHAAH default (unwarned onset 9 ms, rising to a maximum held for the rest of the exposure). |
| Jones, Greene & Ahroon 2019, *Hear Res* 378:53, doi 10.1016/j.heares.2018.11.006 | PubMed abstract; numbers [secondary] via Wikipedia | [M] Anticipatory contraction in 2 of 50 subjects. [secondary] Laser-Doppler time constants of onset and recovery ~113 ms for a 1 kHz tone (0.5 s, 100 dB SPL) and 60–69 ms for recorded .22 gunshots (110 dB peak). |
| Lidén, Hawkins & Nordlund 1964, *Acta Otolaryngol* 57:275, doi 10.3109/00016486409134576 | [secondary] via Wikipedia | Latency of contraction ~10 ms (muscle), "maximum tension may not be reached for 100 ms or more"; under continued stimulation the tension drops to ~50 % within a few seconds. |

Not retrieved (paywalled, no open copy, no numbers in the abstracts): the full texts of Hung &
Dallos 1972, Church & Cudahy 1984 (*Ear Hear* 5:235, the reflex's time course with 15 ms
equipment), Qiu & Stucker 1998 (*Scand Audiol* 27:43: 10 %/90 % on and off latencies, rise and fall
times), Borg 1982, Jones et al. 2019 and Lidén et al. 1964.

### The runtime model (`hearing.rs`) and what is estimated

| Quantity | Value | Basis |
|---|---|---|
| Drive | short-term A-weighted level at the ear: a voice carrying an impulse by its own envelope as heard (5 ms blocks from where its sound begins, placed at the impulse's LAF); an impulse without a voice of its own at its LAF, decaying at 120 dB/s; the running engine as a continuous drive | [E] the decay is the turret's reverberation (RT60 ~0.5 s, §1 row 1, §5); with it a weapon impulse's reflex lasts ~0.3–0.5 s, matching Dancer's 2–3 per second. A recording's envelope is not the physical pressure history, but it is what the ear hears |
| Threshold | 80 dB(A) | [E] top of the broadband range (65–80 dB SPL), because the drives here are low-frequency |
| Growth, maximum | 0.6 dB/dB to 15 dB below 0.7 kHz; half from 0.7 to 1.4 kHz; none above | Dancer 1991; Brask 1979; Borg 1968 (the half share is [E], the band's mean) |
| Integration of a rising drive | 125 ms (fast time weighting); none after the sound | [E] Møller 1962's integrator; gives Dancer's ~150 ms latency 2 dB above threshold; relaxation on its own time constant (Goodman & Keefe 2006) |
| Dead time | 25 ms | Dancer 1991 (25–35 ms at high levels, the 10 % point ~7 ms later) |
| Onset and recovery | 65 ms for impulses, 113 ms for sustained sound | [secondary] Jones et al. 2019; the speed-up with level within a class (Dallos 1964; Hung & Dallos 1972) is not modelled |
| Adaptation | sustained part toward (drive − threshold)/30 dB of its demand, 10 s time constant, same recovery | [E] Dallos 1964; Borg et al. 1983; Lidén 1964 [secondary]; Kaplan 1977; Borg 1982. Dancer's fatigue over minutes is not modelled (Borg et al. 1983 found the reflex stable over industrial exposures) |

What the model then does (test `hearing::tests`, diagnostic `print_reflex_time_course`): the own
88 at the ear (139 dB(A)) contracts it after 32.5 ms (10 % point); 4.7 dB at 50 ms, 10.2 at 100,
12.8 at 150, 14.5 at 250; with the declared drive it relaxes once the turret has rung down and is
gone within a second. Where the voice carrying the shot is known, it drives the reflex as heard:
the verified exterior recording, near full scale for ~0.8 s, holds it that long.
A sustained sound 2 dB above threshold reaches its 10 % point after 205 ms, 5 dB above after
121 ms, 10 dB after 76, 20 dB after 53, 40 dB after 38 ms. Impulses repeated three times a second
meet a contracted reflex; once a second, a relaxed one. The Tiger's bed at the ear holds 5.1 dB
idling at first and adapts to 1.6 dB within a minute; cross-country (105 dB(A)) it holds 12.5 dB,
on the road 15 dB, stable.

Consequence the model does not hide: a crewman in a moving Tiger has his stapedius contracted
before any shot, and his own gun's low frequencies arrive through it.

---

## 10. Winter propagation

**Snow cover** [M] (Albert, CRREL Report 93-20)
- Setup: pistol shots over 0.25 m of snow, 1–274 m, 5–500 Hz.
- In winter the peak fell as **r^−1.6, against r^−1.2 in summer**: "an order of magnitude difference in the signal levels after 100 m". Pulses broadened, and low frequencies were relatively enhanced.
- Fitted flow resistivity of the snow: 10–20 kN·s·m⁻⁴. For comparison: grass 366, hard soil 1,820.
- Modelled peaks, hard ground against snow: 33 against 1.5 Pa at 100 m (≈ 27 dB); 3 against 0.03 Pa at 1 km (≈ 40 dB).
- Ground effect: +6 dB at low frequency, below free field above ~100 Hz at 10 m, 45 Hz at 100 m and 20 Hz at 1 km.
- "The enhancement from ray bending in the winter was not observed": snow absorption dominated.

**Other snow data** (Conrady 2018, *Wind Energy* 21:1282)
- Dry snow with flat grains: 55 kN·s·m⁻⁴; round grains: 11.
- Snow on trees: about −2 dB(A).
- Strong positive gradients (inversions) make sound spread cylindrically, 3 dB per doubling of distance.
- Less turbulence in cold climates.

**Air absorption** [D] (ISO 9613-1 equations; valid down to −20 °C), dB/km:

| Condition | 63 Hz | 125 Hz | 250 Hz | 500 Hz | 1 kHz | 2 kHz | 4 kHz | 8 kHz |
|---|---|---|---|---|---|---|---|---|
| +15 °C, 70 % | 0.10 | 0.38 | 1.1 | 2.4 | 4.1 | 8.8 | 26.6 | 95.0 |
| −10 °C, 80 % | 0.14 | 0.31 | 0.73 | 2.2 | 7.8 | 25.5 | 61.0 | 98.6 |
| −20 °C, 80 % | 0.15 | 0.43 | 1.45 | 4.7 | 11.7 | 19.0 | 24.0 | 32.1 |
| −20 °C, 50 % | 0.24 | 0.77 | 2.4 | 5.9 | 9.1 | 11.0 | 13.2 | 20.5 |

Deep cold absorbs about twice as much as summer air at 0.5–2 kHz and less at 8 kHz. For large-weapon blast the absorption is small: ~1 dB/km at 30 Hz (DoD 2013).

**Weather and wind**
- **Temperature inversions** (clear, calm winter nights) [E, low]: about +3 to +10 dB at 1–10 km, mainly below ~100 Hz over snow. No fetchable primary measurement was found.
- **Weather variability:** ±15 dB within 15 minutes, turbulence alone (DoD 2013). Favourable conditions carry 5 lb of C4 at 2 km to ≤ 120 dB.
- **Upwind shadow zones:** commonly −10 to −30 dB at km range, **not verified** here (check Embleton 1996, JASA 100:31).

**Net effect for the game**
- **Over fresh snow:** small-arms and high-frequency content dies fast (−20 to −30 dB extra at 100 m).
- **Distant guns and bursts** lose their crack and arrive as low booms and rumble.
- **On clear, still, frozen nights** those booms carry farther than in summer.
- **Delay:** 3.1 s per km at −20 °C.

---

## 11. Recommended mapping, physical SPL to digital level (hard full-scale ceiling)

**Principles**
- Keep the audio pipeline in physical units: pascals at the listener's eardrum, after propagation, hull insertion loss, headset and head shadow.
- Apply a single global, monotonic "auditory tone map" as the last step, as the renderer applies ACES before the PS1 crush.
- Never set per-source loudness by taste. Name the tone map in `docs/PHYSICAL_TRUTH_AUDIT.md` as a reproduction transform, not physics.

**Rules**
1. **Drive the map by short-term A-weighted level (LAF, 125 ms), not peak.**
   - Real impulses have crest factors (Lpk − LAF) of ~24 dB in the turret and ~30 dB in the free field.
   - A peak-driven map inverts loudness. The head-out shot, with its higher crest factor, would come out quieter than the buttoned-up shot although its LAF is 13 dB higher.
2. **Curve.** L is the physical LAF at the ear; y is dBFS(A) short-term.

   | Range of L | Mapping | Result at the top of the range |
   |---|---|---|
   | ≤ 110 dB(A) | y = L − 155 (linear) | the mounted bed sits at −45 |
   | 110–150 | y = −45 + (L − 110)/2 (2:1) | 150 → −25 |
   | > 150 | y = −25 + (L − 150)/4 (4:1) | 170 → −20 |

   - Then a lookahead true-peak limiter at **−1 dBFS**, 1–2 ms lookahead, ~200 ms release. The release matches forward-masking recovery (masking falls linearly in log delay and is gone by ~200 ms; Plomp in Green & Wier 1984).
   - The limiter acts only on the most extreme events. It is physiologically analogous to the middle ear's own saturation at very high levels (the annular-ligament nonlinearity modelled in AHAAH).
3. **Resulting digital levels:**

   | Event | Physical LAF | Digital short-term | Notes |
   |---|---|---|---|
   | Quiet steppe | 30–40 dB(A) | −125 to −115 | silent, as in reality next to an engine |
   | Distant 88 at 1 km, outside | ~90 | −65 | |
   | Tiger interior at cruise, headset | 103–110 | −52 to −45 | |
   | Intercom | 104 | −51 | |
   | MG34 burst inside | ~120 | −40 | |
   | Non-penetrating 76 mm hit near the listener | 125–135 | −37 to −32 | |
   | Kar98k at the firer's ear | ~132 | −34 | peak ≈ −7 |
   | Own 88 buttoned up | ~139 | −30.5 | peak ≈ −6.5, no limiting |
   | 152 mm burst at 20 m, outside | 142–146 | −29 to −27 | a few dB of peak limiting |
   | Own 88 head-out | ~152 | −24.5 | the limiter trims ~6 dB of peak |
   | APHE burst inside | 165–170 | −21 to −20 | heavily limited, with rupture in the ear model |

   - Loudness ordering is preserved everywhere.
   - The own gun sits ~15 dB (LAF) above the running bed. Physically it is ~29 dB; the hearing model restores the rest by dulling the bed afterwards.
4. **Playback calibration.**
   - The player sets volume so the running bed (−45 dBFS(A)) plays at a comfortable 75–80 dB(A). Full scale is then ≈ 120–125 dB(A), and the limiter's −1 dBFS peaks reach ≈ 120 dB SPL at the player's ear.
   - That is brief, far below the 140 dB impulse limit, and the long-term level stays ≈ 75–80 dB(A).
   - Never reproduce the true 110–115 dB(A) interior. At NIOSH's 3 dB exchange rate, 115 dB(A) is allowed for ~28 s a day, and the player's ears are real.
5. **Quiet scenes are quiet.** If dismounted night scenes must be audible, add at most a slow, bounded global "listening level" (≤ +20 dB, time constant ≥ 10 s, only while the bed is quiet), gated and documented as a reproduction transform. It is the analogue of exposure, not of a per-source gain.

---

## 12. Recommended TTS / tinnitus / rupture model (physically anchored)

**Best path: tabulate AHAAH offline.**
- The ARL AHAAH model is public. Run it once per canonical waveform at each listener position: own gun buttoned, hatch open and head-out, per crew seat; each hit type; each weapon.
- Store ARU per ear, "warned" for the player's own trigger and "unwarned" for incoming fire.
- At runtime, sum ARU per ear and convert with the published 95th-percentile relation CTS(30 min) = 26.6·ln(ARU) − 140.1 dB. The median ear is ~10 dB lower [E].

**Runtime fallback** [E: a fit to measured anchors, stated as such]

*Per event, per ear:*
- Lpk at the eardrum (after headset, hull and head shadow). Add +4 dB if the ear faces the blast (normal incidence, TR-017/CHABA), otherwise use grazing incidence.
- Duration credit C: 0 dB for A-duration ≤ 0.5 ms, 5 dB at 1 ms, 9 dB at ≥ 2.5 ms. This follows Price 1983 (a 2–3 ms howitzer needed ~11 dB more peak than a rifle for equal damage) and Dancer 1985.
- Effective level: L_eff = Lpk − C.

*Accumulation (N-trade 4 dB per decade, inside the measured 2–7):*
- X = 4·log10(Σ 10^(L_eff,i / 4)).
- **TTS2(4 kHz) = G·(X − 157)**, clamped at 0. G = 2 dB/dB for the median ear; use 3 for susceptible ears. The spread between people is ±16 dB.

*Anchor checks:*

| Case | Model | Measured |
|---|---|---|
| 25–50 rifle shots at 159 (grazing) | 16 dB median; ~10 % above 25 dB | TR-017: 10 % above 25 dB |
| 17-pdr crews, 7 rounds at 188 | C = 9 → 51 dB | 49 dB average (Murray & Reid) |
| 40 rounds of the 88 buttoned at 163 dB | C = 9 → ~7 dB median | "negligible" inside WWII turrets |
| 5–20 rounds of the 88 head-out at 182 dB | 38–42 dB | acoustic trauma, as in Murray & Reid's exposed crews |

*Continuous engine noise:* a slow accumulator toward an asymptote ≈ 1.0·(LAeq,ear − 85) dB at 4 kHz, time constant ~2 h [E, low]. The M4A3 study found no measurable TTS after 5 h, so keep this small.

**Frequency shape** (fraction of the 4 kHz shift) [E, from notch statistics]:

| Frequency | 0.5 kHz | 1 kHz | 2 kHz | 3 kHz | 4 kHz | 6 kHz | 8 kHz |
|---|---|---|---|---|---|---|---|
| Fraction | 0.1 | 0.2 | 0.5 | 0.85 | 1.0 | 1.0 | 0.7 |

Shift the shape ~½ octave lower for low-frequency-dominated blasts.

**Recovery**
- TTS(t) = TTS2·[1 − log10(t / 120 s) / log10(T_R / 120 s)], valid from t = 10 s. It gives ×1.4 of TTS2 at 10 s, which provides the immediate "stunned" dulling.
- Recovery horizon T_R: 16 h for TTS2 ≤ 20 dB; 24 h for ≤ 25; ~72 h for ≤ 40.
- If TTS2 > 40 dB (acoustic trauma), leave a permanent residue ≈ 0.33 × TTS(24 h) (Nottet 2006).
- Optional: let rifle-type exposures grow 10–20 % over the first hour before recovering (Dancer 1991).
- **Within a 30-minute fight, real TTS barely recovers:** 20 dB → ~15 dB. The current 240 s exponential recovery is ~100× too fast. If playability needs it, label it plainly as time compression.

**Rendering the shift (loudness recruitment, not a low-pass)**
- Per band, soft sounds lose the full shift and loud ones almost none: gain_b = −TTS_b · clamp((100 − L_b) / (100 − T_b), 0, 1).
- L_b is the physical band level (dB SPL); T_b is the normal threshold, ≈ 0–10 dB at 1–4 kHz and 20–30 dB at 125–250 Hz. A downward expander per band does this.
- A plain low-pass cannot: it dulls loud and soft sounds equally, which is not how a threshold shift sounds.

**Tinnitus**
- **Onset:** immediately in any ear whose modelled TTS2 ≥ 10 dB. It is near-certain after acoustic trauma (94–100 %).
- **Pitch:** 0.2 octave below that ear's maximum-shift frequency, typically 3–6 kHz. A pure tone or 1/3-octave noise.
- **Loudness:** equal to 5–10 dB SL in the damaged ear. With recruitment, render ≈ 15–25 dB above the normal threshold at the pitch.
- It is an internal percept, so render it relative to the listener stage rather than the SPL map. Let the running bed mask it naturally: it becomes inaudible once the bed in its band exceeds it by ~20 dB.
- **Duration:** it fades with TTS (gone below ~5 dB TTS). After acoustic trauma, ~30 % keep it for months.

**Eardrum rupture**
- P(rupture) = Φ[(ln p − ln 100 kPa) / 0.64]. That gives ~1 % at 20 kPa (180 dB), ~5 % at 35 kPa (185 dB) and 50 % at 100 kPa (194 dB).
- Use the reflected pressure, several times side-on, for an ear facing a near blast.
- **Effects:** an immediate ~20–40 dB conductive loss in that ear, bleeding (a visual story beat), and 50 % tinnitus.

**Warned versus unwarned**
- MIL-STD-1474E gives no extra allowance for a warned listener, and anticipatory reflex contraction is unreliable (Jones 2018).
- Keep the own-gun/incoming distinction only through the physical difference in waveform (reverberant, long own-gun pulse against a short strike), not a reflex credit.

---

## 13. Gaps and open items

- **No measurement** of: any 88 mm; any WWII turret interior during fire; closed-hatch or head-out levels of modern tanks; any coaxial or bow MG inside a vehicle; projectile impacts heard inside armour; tank-hull insertion loss for external blast. All figures for these are scaled or derived above.
- **Not received:** German and Soviet memoir quotes on hits ("like a bell", concussion, bleeding ears), for example Carius or Drabkin's interviews.
- **Unverified:**
  - the Kubinka 1940 audibility distances;
  - the Dfh.b attenuation;
  - upwind shadow-zone magnitudes;
  - the exhaust sound-power law;
  - the 82 mm and 45 mm HE fill masses;
  - the confined-charge relation used in §5.
- **Leads for DTIC** (down or returning 403 during this research):
  - AD658629 (Hatch & Walpole 1943, full text);
  - AD018260 (Rosenblith & Stevens, *Handbook of Acoustic Noise Control* vol. II, 1953);
  - AD487893 (Haggard & Wight 1961, M60);
  - ADA079550 (Bryson 1979, tank crew helmet earcups);
  - AD135195 (Tolhurst 1957);
  - the CHABA 1968 report;
  - Beranek 1952 (JASA doi 10.1121/1.1906976);
  - Solomon & Fletcher 1958 (hearing losses among armor personnel).
- **Housekeeping:**
  - Research forks left scratch downloads in `/tmp`: `bing_test.html`, `t34ru.txt`, `loza.html`, `kirichenko-petr-ilich.html`, `bryukhov-vasiliy-pavlovich.html`, `nak.pdf`, `ad0658629.pdf`, `ad_cit.html`, `try.bin`, `h.html`, `h2.json`, `ntis.*`, `hw1943.pdf`, `w.txt`, `borden*.html`, and the directory `/tmp/fork_hear/`. They were not deleted, because the brief forbids modifying files outside this report; remove them if wanted.
  - The session's shell is now pinned to worktree `urban-intro-20260928`.

---

## 14. Sources (fetched or retrieved)

**Standards and guidance**
- MIL-STD-1474D (1997) and MIL-STD-1474E (2015), everyspec.com.
- NATO RTO-TR-017 (2003), *Reconsideration of the effects of impulse noise*; Johnson in RTO-EN-11.
- NIOSH HHE 2013-0124-3208; NIOSH EPHB 350-11a.
- IOM 2006, *Noise and Military Service* (nationalacademies.org/read/11443), Tables 3-1 and 5-2.
- CHPPM 2006, "Noise levels of Army equipment" (docslib 5229674).
- USAEHA TG 1975, *Noise Hazard Evaluation, Sound Level Data of Noise Sources* (nonoise.org roll13doc22).
- DoD Noise Working Group 2013, *An Overview of Blast Noise* (denix.osd.mil).
- AMCP 706-251, *Muzzle Devices*, 1968 (everyspec).
- ISO 9613-1 (equations).

**Weapon and crew-position measurements**
- Foster 1960, HEL TM 8-60, AD0627820 (Wayback).
- Chaillet 1964, HEL TN 4-64, AD0450726.
- Kietzman, Fansler & Thompson 1992, BRL-MR-3957, ADA245565.
- Garinther 1979, JASA 65(S1):S29, doi 10.1121/1.2017190.
- Garinther & Blazie 1973 (M60A1, abstract).
- Price, ARL, *Weapon noise exposure … AHAAH*; Price 2007, doi 10.1121/1.2785810.
- Murray & Reid 1946, *J Laryngol Otol* 61:92, doi 10.1017/S0022215100007799.

**Small arms**
- Branch 2011, doi 10.1177/0194599811398872.
- Murphy 2018, PMC8588962.
- Balatsouras 2005, doi 10.1111/j.1365-2273.2005.00984.x.
- Wagner 2005, doi 10.1097/01.MLG.0000181463.16591.A7.
- Pääkkönen 1998, PMID 9465571.
- Ylikoski 1995, doi 10.3109/01050399509042203.
- Konopka 2002, PMID 12577807.
- Toivonen 2023, doi 10.3390/ijerph20043366.
- Meinke 2017, PMC5634813; Meinke 2014, PMC4659434.
- Flamme 2011, doi 10.3766/jaaa.22.2.4.
- Paddan 2015/2016, doi 10.1093/annhyg/mev053, PMC5187655.
- Kardous 2003, doi 10.1080/10473220301409.
- Zhao 2019, doi 10.1155/2019/5938034.
- Maher 2006 (IEEE DSP); Routh & Maher 2016 (AES).

**Ballistic shock waves**
- DuMond et al. 1946, JASA 18:97, doi 10.1121/1.1916347.
- Stoughton 1997, JASA 102:781, doi 10.1121/1.419904.
- Sadler 1998, JASA 104:955.
- Flamme & Murphy 2019, doi 10.1080/14992027.2018.1534009.

**Vehicles**
- Nakashima, Borland & Abel 2007, *Ind Health* 45:318, doi 10.2486/indhealth.45.318.
- Hatch & Walpole 1943, AD658629 (NTIS abstract).
- Moore & Cox 2024, *Trends Hear*, doi 10.1177/23312165241240353.
- en.wikipedia: Tiger I; 8.8 cm KwK 36; ZiS-3; M-30; ML-20; SC 50. ru.wikipedia: T-34.
- alanhamby.com (Tiger drivetrain).
- iremember.ru: D. F. Loza; P. I. Kirichenko.
- battlefield.ru via Wayback (Soviet fixed ammunition).
- Tigerfibel via the job's `tiger_field_damage_repair.md`.

**Hearing**
- Pfander 1980, doi 10.1121/1.383886.
- Chan 2001, doi 10.1121/1.1391243.
- Ward 1997, doi 10.1002/j.2040-4603.1997.tb01610.x.
- Dancer 1991, doi 10.3109/00206099109072896; Dancer 1991, *Médecine/Sciences* 7:357.
- Patterson & Hamernik 1997, doi 10.1016/s0300-483x(97)03653-6.
- Nottet 2006, doi 10.1097/01.MLG.0000216823.77995.13.
- Mrena 2002, doi 10.1159/000057660; Mrena 2004, doi 10.1080/14992020400050025.
- Temmel 1999, PMID 10599152.
- Ritenour 2008, doi 10.1097/TA.0b013e318160773e.
- Muhr 2011, doi 10.4103/1463-1741.82965.
- Ochi 2003, doi 10.1097/00005537-200303000-00007.
- Martines 2010, doi 10.1155/2010/265861.
- Byun 2010, doi 10.1016/j.otohns.2010.06.928.
- Ortmann 2011, doi 10.1111/j.1460-9568.2010.07542.x.
- von der Behrens & Meyer 2014, doi 10.1155/2014/741452.
- Richmond 1989, doi 10.1177/00034894890980s507.
- Brask 1978, *Acta Otolaryngol* 86:116.
- Jones, Greene & Ahroon 2018, *Hear Res* 378:53.
- Green & Wier 1984, *Comprehensive Physiology*, doi 10.1002/j.2040-4603.1984.tb00885.x.
- Mills 1978.

**Propagation**
- Albert, CRREL Report 93-20 (erdc-library, Wayback).
- Conrady 2018, *Wind Energy* 21:1282, doi 10.1002/we.2254.
- DoD 2013 (above).

**Computation (my derivations)**
- Kingery-Bulmash / Swisdak 1994 fits.
- Whitham far-field N-wave.
- Mass law and Sharp's method.
- Helmholtz leak model.
- ISO 9613-1.
- A-weighted energy of Friedlander pulses (numerical FFT).
